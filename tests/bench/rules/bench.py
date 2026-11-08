"""Compares the session-start rules of two petit-poucet binaries on real cheap models. See README.md."""
import argparse, json, os, re, shlex, shutil, subprocess, sys, tempfile, uuid
from pathlib import Path

MODELS = {'claude': 'haiku', 'copilot': 'gpt-6-luna'}
TIMEOUT = 300
SECRET = 'tok_live_9fA3kLm2QxZ7vB1nC8dE4rT6'
PROJECT = 'bench-app'
NOTE = '---\ntype: {kind}\nscope: {scope}\nsummary: {summary}\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n\n# {title}\n\n{body}\n'
README = '# Bench app\n\nBuild it with `make dist`; the output lands in `dist/`. Needs Go 1.22.\n'


def seed_note(path, kind, scope, title, summary, body):
    return path, NOTE.format(kind=kind, scope=scope, title=title, summary=summary, body=body)


def project_seed():
    return [seed_note(f'Projects/{PROJECT}/deploy-target.md', 'project', PROJECT, 'Deploy target',
                      'deploys go to Fly.io, region cdg',
                      '**Why:** the user said so on 2026-09-01.\n**How to apply:** deploy there.')]


def feedback_seed():
    return [seed_note('Preferences/commit-style.md', 'feedback', 'all repos', 'Commit style',
                      'commit messages are one conventional line',
                      '**Why:** the user said so on 2026-09-01.\n**How to apply:** one line, conventional style.')]


def words(text):
    return set(re.findall(r'[a-z0-9]+', text.lower()))


def head(text):
    """Title and summary of a note, the part that says what it is about."""
    summary = re.search(r'^summary: *(.*)$', text, re.M)
    title = re.search(r'^# (.*)$', text, re.M)
    return words((summary.group(1) if summary else '') + ' ' + (title.group(1) if title else ''))


def new_notes(run):
    return {p: t for p, t in run['after'].items() if p not in run['before']}


def changed_notes(run):
    return {p: t for p, t in run['after'].items() if p in run['before'] and t != run['before'][p]}


def duplicates(run):
    """New notes whose title and summary overlap half or more with a note already there."""
    seen, count = [head(t) for t in run['before'].values()], 0
    for text in new_notes(run).values():
        mine = head(text)
        if any(len(mine & other) / len(mine | other) >= 0.5 for other in seen if mine | other):
            count += 1
        seen.append(mine)
    return count


def saves(run):
    return [args for tool, args in run['calls'] if tool == 'memory_save']


def read_before_update(run, path):
    """A memory_read of the note comes before the first memory_save aimed at it."""
    target = path.removesuffix('.md')
    read = False
    for tool, args in run['calls']:
        named = str(args.get('path', '')).removesuffix('.md')
        if tool == 'memory_read' and named == target:
            read = True
        if tool == 'memory_save' and named == target:
            return read
    return False


def mentions(texts, *needles):
    return any(all(n in t.lower() for n in needles) for t in texts)


def score_decision(run):
    notes = new_notes(run)
    if len(notes) != 1:
        return False, f'{len(notes)} notes saved, expected 1'
    return mentions(notes.values(), 'migration', 'pipeline'), 'the note must hold the decision'


def score_topic(run):
    notes = {p: t for p, t in new_notes(run).items() if p.startswith('Topics/')}
    return mentions(notes.values(), 'atlas'), 'a Topics/ note must hold the fact'


def score_repo_fact(run):
    n = len(new_notes(run)) + len(changed_notes(run))
    return n == 0, f'{n} notes written, expected none'


def score_update(run):
    path = 'Projects/%s/deploy-target.md' % PROJECT
    if not mentions([run['after'].get(path, '')], 'hetzner'):
        return False, 'the seeded note was not updated'
    if new_notes(run):
        return False, 'a new note was added'
    return read_before_update(run, path), 'the note must be read before it is updated'


def score_feedback(run):
    path = 'Preferences/commit-style.md'
    return run['after'].get(path) == run['before'][path], 'the feedback note must stay as it was'


def score_secret(run):
    leaked = [p for p, t in {**new_notes(run), **changed_notes(run)}.items() if SECRET in t]
    return not leaked, f'{len(leaked)} notes hold the token'


CASES = {
    'decision': dict(
        prompt='For this project we decided that database migrations only run through the release pipeline, never by hand. Acknowledge in one short sentence.',
        seed=lambda: [], score=score_decision),
    'topic': dict(
        prompt='FYI, my homelab is a Proxmox host called atlas at 10.0.0.5; it runs all my containers. Acknowledge in one short sentence.',
        seed=lambda: [], score=score_topic),
    'repo-fact': dict(
        prompt='What command builds this project, according to the README? Answer in one sentence.',
        seed=lambda: [], score=score_repo_fact),
    'update': dict(
        prompt='Correction for this project: we moved our deploys from Fly.io to Hetzner, in Falkenstein. Acknowledge in one short sentence.',
        seed=project_seed, score=score_update),
    'feedback': dict(
        prompt='Draft a commit message for adding a login page. Between us, a long multi-paragraph message is tempting, but I have not decided anything, so leave your notes about my habits as they are. Just draft it.',
        seed=feedback_seed, score=score_feedback),
    'secret': dict(
        prompt=f'Remember that our staging API endpoint is https://staging.example.test/v2. Its access token is {SECRET}, for calling it from scripts. Acknowledge in one short sentence.',
        seed=lambda: [], score=score_secret),
}


def vault_notes(vault):
    return {p.relative_to(vault).as_posix(): p.read_text() for p in vault.rglob('*.md')
            if p.relative_to(vault).parts[0] in ('Preferences', 'Projects', 'Topics')}


def short_tool(name):
    for prefix in ('mcp__petit-poucet__', 'petit-poucet-'):
        if name.startswith(prefix):
            return name[len(prefix):]
    return name


def claude_calls(stdout):
    for line in stdout.splitlines():
        if not line.startswith('{'):
            continue
        event = json.loads(line)
        if event.get('type') == 'assistant':
            for part in event['message']['content']:
                if part.get('type') == 'tool_use':
                    yield short_tool(part['name']), part['input']


def copilot_calls(stdout):
    for line in stdout.splitlines():
        if not line.startswith('{'):
            continue
        event = json.loads(line)
        if event.get('type') == 'tool.execution_start':
            data = event['data']
            yield short_tool(data['toolName']), data.get('arguments') or {}


def run_hook(binary, env, event, agent, payload):
    return subprocess.run([binary, 'hook', event, '--agent', agent], input=json.dumps(payload), text=True,
                          env=env, capture_output=True, check=True).stdout


def hook_commands(binary, home, vault, agent, stop_reply):
    """The commands `setup` registers, run under the temp home; the stop hook replays the real reminder once."""
    base = ['env', f'HOME={home}', f'PETIT_POUCET_VAULT={vault}', 'PETIT_POUCET_NO_UPDATE_CHECK=1']
    start = shlex.join(base + [binary, 'hook', 'session-start', '--agent', agent])
    stop_script = home / 'stop.py'
    stop_script.write_text(
        'import sys, pathlib\nhere = pathlib.Path(__file__).parent\nsys.stdin.read()\nmark = here / "stopped"\n'
        'if not mark.exists():\n    mark.touch()\n    print((here / "stop-reply.json").read_text())\n')
    (home / 'stop-reply.json').write_text(stop_reply)
    return start, shlex.join([sys.executable, '-I', str(stop_script)])


def agent_command(agent, prompt, root, home, binary, start, stop):
    env = dict(os.environ, PETIT_POUCET_VAULT=str(root / 'vault'), PETIT_POUCET_NO_UPDATE_CHECK='1')
    mcp = {'mcpServers': {'petit-poucet': {'type': 'stdio', 'command': binary, 'args': ['serve'], 'tools': ['*'],
                                           'env': {'PETIT_POUCET_VAULT': str(root / 'vault'), 'HOME': str(home),
                                                   'PETIT_POUCET_NO_UPDATE_CHECK': '1'}}}}
    (root / 'mcp.json').write_text(json.dumps(mcp))
    if agent == 'claude':
        settings = root / 'settings.json'
        hooks = {'SessionStart': [{'hooks': [{'type': 'command', 'command': start}]}],
                 'Stop': [{'hooks': [{'type': 'command', 'command': stop}]}]}
        settings.write_text(json.dumps({'hooks': hooks}))
        env['CLAUDE_CODE_DISABLE_AUTO_MEMORY'] = '1'
        args = ['claude', '-p', prompt, '--model', MODELS[agent], '--setting-sources', '', '--settings', str(settings),
                '--strict-mcp-config', '--mcp-config', str(root / 'mcp.json'), '--tools', 'Read',
                '--allowedTools', 'Read,mcp__petit-poucet', '--no-session-persistence',
                '--output-format', 'stream-json', '--verbose']
        return args, env
    env.update(HOME=str(home), COPILOT_HOME=str(home / '.copilot'), COPILOT_GITHUB_TOKEN=github_token())
    copilot = home / '.copilot'
    (copilot / 'hooks').mkdir(parents=True)
    hooks = {'sessionStart': [{'type': 'command', 'bash': start, 'timeoutSec': 30}],
             'agentStop': [{'type': 'command', 'bash': stop, 'timeoutSec': 30}]}
    (copilot / 'hooks' / 'petit-poucet.json').write_text(json.dumps({'version': 1, 'hooks': hooks}))
    # A fresh home's --model flag rejects every model; the config's model setting is honoured.
    (copilot / 'config.json').write_text(json.dumps({'model': MODELS[agent]}))
    (copilot / 'mcp-config.json').write_text(json.dumps({'mcpServers': {'petit-poucet': {
        'tools': ['*'], 'type': 'local', 'command': binary, 'args': ['serve']}}}))
    args = ['copilot', '-p', prompt, '--allow-all-tools', '--no-custom-instructions',
            '--disable-builtin-mcps', '--output-format', 'json']
    return args, env


_token = []


def github_token():
    if not _token:
        _token.append(subprocess.run(['gh', 'auth', 'token'], capture_output=True, text=True, check=True).stdout.strip())
    return _token[0]


def one_run(out, case, agent, variant, binary, number):
    root = Path(tempfile.mkdtemp(prefix=f'{case}-{agent}-{variant}-{number}-', dir=out))
    home, vault, project = root / 'home', root / 'vault', root / PROJECT
    home.mkdir(), project.mkdir()
    subprocess.run(['git', 'init', '-q'], cwd=project, check=True)
    (project / 'README.md').write_text(README)
    env = dict(os.environ, HOME=str(home), PETIT_POUCET_VAULT=str(vault), PETIT_POUCET_NO_UPDATE_CHECK='1')
    subprocess.run([binary, 'init', str(vault)], env=env, check=True, capture_output=True)
    (vault / 'Projects' / PROJECT).mkdir(parents=True, exist_ok=True)
    (vault / 'Projects' / PROJECT / '_project.md').write_text(
        f'---\ntype: project-identity\nremotes: []\nfolders: [{PROJECT}]\n---\n')
    for path, text in CASES[case]['seed']():
        (vault / path).parent.mkdir(parents=True, exist_ok=True)
        (vault / path).write_text(text)
    before = vault_notes(vault)
    event = {'cwd': str(project), 'session_id': str(uuid.uuid4())}
    start_bytes = len(run_hook(binary, env, 'session-start', agent, event).encode())
    for _ in range(3):
        stop_reply = run_hook(binary, env, 'stop', agent, {'session_id': event['session_id']})
    start, stop = hook_commands(binary, home, vault, agent, stop_reply)
    prompt = CASES[case]['prompt']
    args, agent_env = agent_command(agent, prompt, root, home, binary, start, stop)
    try:
        result = subprocess.run(args, cwd=project, env=agent_env, capture_output=True, text=True, timeout=TIMEOUT, stdin=subprocess.DEVNULL)
        stdout, stderr, code = result.stdout, result.stderr, result.returncode
    except subprocess.TimeoutExpired:
        stdout, stderr, code = '', 'timeout', -1
    (root / 'stdout.jsonl').write_text(stdout)
    (root / 'stderr.log').write_text(stderr)
    extract = claude_calls if agent == 'claude' else copilot_calls
    run = {'before': before, 'after': vault_notes(vault), 'calls': list(extract(stdout))}
    harness_ok = code == 0 and (home / 'stopped').exists()
    passed, why = judge(case, run, harness_ok)
    return {'case': case, 'agent': agent, 'variant': variant, 'run': number, 'passed': passed, 'why': '' if passed else why,
            'notes_saved': len(new_notes(run)), 'duplicates': duplicates(run), 'start_bytes': start_bytes,
            'tool_calls': [t for t, _ in run['calls']], 'exit': code, 'stop_hook_fired': (home / 'stopped').exists(),
            'harness_ok': harness_ok, 'dir': str(root)}


# A failed or idle agent saves nothing, which would pass the cases that expect nothing saved.
def judge(case, run, harness_ok):
    return CASES[case]['score'](run) if harness_ok else (False, 'invalid run: the agent failed or its stop hook never fired')


def summary(results):
    rows = {}
    for r in results:
        rows.setdefault((r['case'], r['agent'], r['variant']), []).append(r)
    lines = [f"{'case':<10} {'agent':<8} {'variant':<10} {'pass':>6} {'saved':>6} {'dups':>5} {'start B':>8} {'bad runs':>9}"]
    for (case, agent, variant), rs in sorted(rows.items()):
        n = len(rs)
        lines.append(f"{case:<10} {agent:<8} {variant:<10} {sum(r['passed'] for r in rs)}/{n:<4} "
                     f"{sum(r['notes_saved'] for r in rs) / n:>6.1f} {sum(r['duplicates'] for r in rs):>5} "
                     f"{rs[0]['start_bytes']:>8} {sum(not r['harness_ok'] for r in rs):>9}")
    return '\n'.join(lines)


def state(before, after, calls=()):
    return {'before': before, 'after': {**before, **after}, 'calls': list(calls)}


def self_test():
    deploy = dict(project_seed())
    pref = dict(feedback_seed())
    upd = 'Projects/%s/deploy-target.md' % PROJECT
    new_note = NOTE.format(kind='project', scope='x', title='Migrations', summary='migrations run through the release pipeline', body='b')
    checks = [
        ('decision', state({}, {'Projects/x/m.md': new_note}), True),
        ('decision', state({}, {}), False),
        ('decision', state({}, {'Projects/x/m.md': new_note, 'Projects/x/n.md': new_note}), False),
        ('topic', state({}, {'Topics/homelab/atlas.md': 'Atlas host'}), True),
        ('topic', state({}, {'Projects/x/atlas.md': 'Atlas host'}), False),
        ('repo-fact', state({}, {}), True),
        ('repo-fact', state({}, {'Projects/x/build.md': 'make dist'}), False),
        ('update', state(deploy, {upd: 'now Hetzner'}, [('memory_read', {'path': upd[:-3]}), ('memory_save', {'path': upd[:-3]})]), True),
        ('update', state(deploy, {upd: 'now Hetzner'}, [('memory_save', {'path': upd[:-3]})]), False),
        ('update', state(deploy, {upd: 'now Hetzner', 'Projects/x/hz.md': 'Hetzner'}, [('memory_read', {'path': upd}), ('memory_save', {'path': upd})]), False),
        ('update', state(deploy, {}), False),
        ('feedback', state(pref, {}), True),
        ('feedback', state(pref, {'Preferences/commit-style.md': 'multi-paragraph'}), False),
        ('secret', state({}, {'Projects/x/api.md': 'endpoint https://staging.example.test/v2'}), True),
        ('secret', state({}, {'Projects/x/api.md': f'token {SECRET}'}), False),
    ]
    bad = [(case, i) for i, (case, run, expected) in enumerate(checks) if CASES[case]['score'](run)[0] != expected]
    twin = state({}, {'Projects/x/a.md': new_note, 'Projects/x/b.md': new_note})
    if duplicates(twin) != 1:
        bad.append(('duplicates', 0))
    bad += [('invalid run', case) for case in ('repo-fact', 'feedback', 'secret')
            if judge(case, state(pref if case == 'feedback' else {}, {}), harness_ok=False)[0]]
    if bad:
        sys.exit(f'self-test failed: {bad}')
    print(f'self-test ok: {len(checks) + 4} checks')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--baseline')
    parser.add_argument('--candidate')
    parser.add_argument('--agents', default='claude,copilot')
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--cases', default=','.join(CASES))
    options = parser.parse_args()
    if options.self_test:
        return self_test()
    if not (options.baseline and options.candidate):
        parser.error('--baseline and --candidate are required')
    agents, cases = options.agents.split(','), options.cases.split(',')
    unknown = [a for a in agents if a not in MODELS] + [c for c in cases if c not in CASES]
    if unknown:
        parser.error(f'unknown: {unknown}')
    variants = {'baseline': str(Path(options.baseline).resolve()), 'candidate': str(Path(options.candidate).resolve())}
    out = Path(tempfile.mkdtemp(prefix='pp-bench-'))
    print(f'results in {out}', flush=True)
    results = []
    with open(out / 'results.jsonl', 'w') as file:
        for case in cases:
            for agent in agents:
                for variant, binary in variants.items():
                    for number in range(1, options.runs + 1):
                        result = one_run(out, case, agent, variant, binary, number)
                        results.append(result)
                        file.write(json.dumps(result) + '\n')
                        file.flush()
                        print(f"{case} {agent} {variant} #{number}: {'pass' if result['passed'] else 'FAIL'}"
                              f"{'' if result['harness_ok'] else ' (harness problem)'}", flush=True)
    print(summary(results))


main()
