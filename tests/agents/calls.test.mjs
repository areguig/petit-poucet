// Fixture test of calls.mjs --presence and --catalog: only names and booleans are printed, never what the calls contain.
import assert from "node:assert";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const SECRET = "SECRET_BODY /home/someone/vault API_KEY=hunter2";
const log = join(mkdtempSync(join(tmpdir(), "calls-")), "calls.jsonl");
const call = (text) => JSON.stringify({ body: JSON.stringify({ tools: [{}], input: `${SECRET} ${text}` }) });
const sideRequest = JSON.stringify({ body: JSON.stringify({ input: "migrate-memory" }) });
writeFileSync(log, [call("tidy-memory"), call("memory migrate-memory tidy-memory"), sideRequest].join("\n"));

test("--presence reports per-call booleans and nothing else", () => {
  const script = fileURLToPath(new URL("calls.mjs", import.meta.url));
  const run = spawnSync("node", [script, log, "tools", "--presence=memory,migrate-memory,tidy-memory"], { encoding: "utf8" });
  assert.strictEqual(run.status, 0);
  assert.strictEqual(
    run.stdout,
    "call 1: memory false, migrate-memory false, tidy-memory true\ncall 2: memory true, migrate-memory true, tidy-memory true\n",
  );
  assert.strictEqual(run.stderr, "");
});

test("--presence keeps a valid call whose names are all missing as a normal result", () => {
  const file = join(mkdtempSync(join(tmpdir(), "calls-none-")), "calls.jsonl");
  writeFileSync(file, call("nothing"));
  const run = spawnSync("node", [script(), file, "tools", "--presence=memory"], { encoding: "utf8" });
  assert.strictEqual(run.status, 0);
  assert.strictEqual(run.stdout, "call 1: memory false\n");
});

const script = () => fileURLToPath(new URL("calls.mjs", import.meta.url));
const dir = mkdtempSync(join(tmpdir(), "calls-err-"));
const fixture = (name, content) => {
  const file = join(dir, name);
  writeFileSync(file, content);
  return file;
};

for (const [name, file] of [
  ["a missing capture", join(dir, "missing.jsonl")],
  ["a malformed capture", fixture("malformed.jsonl", `${SECRET} {not json\n`)],
  ["a malformed body", fixture("body.jsonl", JSON.stringify({ body: `${SECRET} {not json` }))],
  ["zero tool-bearing calls", fixture("zero.jsonl", JSON.stringify({ body: JSON.stringify({ input: SECRET }) }))],
  ["an empty capture", fixture("empty.jsonl", "")],
]) {
  test(`--presence fails with a bounded diagnostic on ${name}`, () => {
    const run = spawnSync("node", [script(), file, "tools", "--presence=memory"], { encoding: "utf8" });
    assert.notStrictEqual(run.status, 0);
    assert.match(run.stderr, /^presence diagnostic error: [a-z -]{1,60}\n$/);
    assert.strictEqual(run.stdout, "");
    for (const leak of ["SECRET_BODY", "hunter2", "/home/someone", "SyntaxError", "ENOENT", "    at "]) {
      assert.ok(!(run.stdout + run.stderr).includes(leak), `leaked ${leak}`);
    }
  });
}

const catalogCall = (catalogText, noise = "") =>
  JSON.stringify({
    body: JSON.stringify({ tools: [{ description: "for the tidy-memory skill" }], instructions: noise, input: [{ content: [{ text: catalogText }] }] }),
  });
const runCatalog = (lines) =>
  spawnSync("node", [script(), fixture(`catalog-${Math.random()}.jsonl`, lines.join("\n")), "tools", "--catalog=memory,migrate-memory,tidy-memory", "--codex-version=codex-cli 1.2.3"], {
    encoding: "utf8",
  });
const ALL = ["- memory: d (file: /x/SKILL.md)", "- migrate-memory: d (file: /y/SKILL.md)", "- tidy-memory: d (file: /z/SKILL.md)"];
// Mentions of skill names around the section, and a list item after it, are not entries.
const catalog = (...entries) => `intro migrate-memory\n### Available skills\n${entries.join("\n")}\n### How to use skills\n- migrate-memory: not an entry\nmemory`;

const assertSafe = (run) => {
  for (const leak of ["SECRET_BODY", "hunter2", "/x/SKILL.md", "/home/someone"]) assert.ok(!(run.stdout + run.stderr).includes(leak), `leaked ${leak}`);
};

test("--catalog passes when every call's catalog section, found in nested data, lists all the skills", () => {
  const run = runCatalog([catalogCall(catalog(...ALL), `${SECRET} migrate-memory`), catalogCall(catalog(...ALL, "- other: d"))]);
  assert.strictEqual(run.status, 0);
  const lines = run.stdout.split("\n");
  assert.strictEqual(lines[0], "codex codex-cli 1.2.3");
  assert.match(lines[1], /^call 1: catalog entries 3, chars \d+, memory true, migrate-memory true, tidy-memory true$/);
  assert.match(lines[2], /^call 2: catalog entries 4, chars \d+, memory true, migrate-memory true, tidy-memory true$/);
  assert.strictEqual(run.stderr, "");
  assertSafe(run);
});

test("--catalog fails when a skill is only mentioned outside the catalog entries", () => {
  const run = runCatalog([catalogCall(catalog(ALL[0], ALL[2]), `${SECRET} migrate-memory tidy-memory memory`)]);
  assert.strictEqual(run.status, 1);
  assert.match(run.stdout, /call 1: catalog entries 2, chars \d+, memory true, migrate-memory false, tidy-memory true\n$/);
  assert.strictEqual(run.stderr, "catalog check error: skill missing from catalog\n");
  assertSafe(run);
});

for (const [name, text] of [
  ["is missing", "tidy-memory"],
  ["is repeated", `${catalog(...ALL)}\n${catalog(...ALL)}`],
  ["has no entries", catalog("no entries here")],
]) {
  test(`--catalog fails when the section ${name}`, () => {
    const run = runCatalog([catalogCall(catalog(...ALL)), catalogCall(text, SECRET)]);
    assert.strictEqual(run.status, 1);
    assert.match(run.stdout, /call 2: catalog section not found or unparseable, no booleans\n$/);
    assert.strictEqual(run.stderr, "catalog check error: catalog section not found or unparseable\n");
    assertSafe(run);
  });
}

// --session: the whole session view the hook produces reached the model in every call, within the budget.
const VIEW = [
  "Agent memory (petit-poucet): the Index below is only part of it.",
  "Not loaded (over the 4000-byte session budget): 3 preferences.",
  "fix it now (memory_save or memory_delete)",
  "1 situational preference not loaded (each applies only in some situations): memory_search finds them by what you're doing.",
  "- [[Preferences/last-line]] — 日本語 🪨",
  "",
].join("\n");
const NOTICE = "Tell the user once, in one line: petit-poucet: 99.0.0 is out.\n";
const hookOutput = (text) => fixture(`hook-${Math.random()}.json`, JSON.stringify({ additionalContext: text }));
const sessionCall = (text) => JSON.stringify({ body: JSON.stringify({ tools: [{ name: "memory_search" }], input: `tidy-memory ${text}` }) });
const withNotice = VIEW.replace("- [[", `${NOTICE}- [[`);
const runSession = (texts, { views = [VIEW, withNotice], budget = 4000 } = {}) =>
  spawnSync(
    "node",
    [
      script(),
      fixture(`session-${Math.random()}.jsonl`, texts.map(sessionCall).join("\n")),
      "memory_search",
      "--no-reminder",
      ...views.map((view) => `--session=${hookOutput(view)}`),
      `--budget=${budget}`,
    ],
    { encoding: "utf8" },
  );

test("--session passes when each call holds the whole view, from before or after the update notice", () => {
  const run = runSession([VIEW, withNotice, VIEW]);
  assert.strictEqual(run.status, 0, run.stdout + run.stderr);
});

test("--session fails when a call holds a view that is neither the one before nor the one after the notice", () => {
  const drifted = withNotice.replace("3 preferences", "4 preferences");
  assert.strictEqual(runSession([VIEW, drifted, VIEW]).status, 1);
});

test("--session fails when a call holds the view cut short", () => {
  const run = runSession([VIEW, VIEW.replace("🪨", ""), VIEW]);
  assert.strictEqual(run.status, 1);
});

test("--session fails when the view is over the budget or is not partial", () => {
  assert.strictEqual(runSession([VIEW, VIEW, VIEW], { budget: Buffer.byteLength(VIEW) - 1 }).status, 1);
  const whole = VIEW.replace(/Not loaded.*\n/, "");
  assert.strictEqual(runSession([whole, whole, whole], { views: [whole] }).status, 1);
});

test("--session fails when the view lacks the situational line or lists the situational preference", () => {
  const without = VIEW.replace(/1 situational.*\n/, "");
  assert.strictEqual(runSession([without, without, without], { views: [without] }).status, 1);
  const listed = VIEW.replace("- [[", "- [[situational-probe-7c2e]] — situational-probe-7c2e\n- [[");
  assert.strictEqual(runSession([listed, listed, listed], { views: [listed] }).status, 1);
});

// Requests in each supported format, with the view where that format gives the model its context.
const DELIVERED = {
  "Anthropic system": (view) => ({ system: [{ type: "text", text: view }], messages: [{ role: "user", content: "tidy-memory" }] }),
  "Anthropic user message": (view) => ({ messages: [{ role: "user", content: [{ type: "text", text: `tidy-memory ${view}` }] }] }),
  "OpenAI chat system message": (view) => ({ messages: [{ role: "system", content: view }, { role: "user", content: "tidy-memory" }] }),
  "OpenAI responses developer message": (view) => ({
    instructions: "tidy-memory",
    input: [{ type: "message", role: "developer", content: [{ type: "input_text", text: view }] }],
  }),
  "Gemini system instruction": (view) => ({ systemInstruction: { parts: [{ text: view }] }, contents: [{ role: "user", parts: [{ text: "tidy-memory" }] }] }),
};
// The view where no model reads it as context: request metadata, tool-call arguments, tool results.
const UNDELIVERED = {
  "request metadata": (view) => ({ input: "tidy-memory", metadata: { undelivered_debug_context: view } }),
  "a responses function-call argument": (view) => ({
    input: [
      { type: "message", role: "user", content: [{ type: "input_text", text: "tidy-memory" }] },
      { type: "function_call", name: "memory_search", arguments: view },
    ],
  }),
  "a chat tool-call argument": (view) => ({
    messages: [
      { role: "user", content: "tidy-memory" },
      { role: "assistant", tool_calls: [{ function: { name: "memory_search", arguments: view } }] },
    ],
  }),
  "an Anthropic tool result": (view) => ({
    messages: [{ role: "user", content: [{ type: "text", text: "tidy-memory" }, { type: "tool_result", content: [{ type: "text", text: view }] }] }],
  }),
};
const runShaped = (shape) =>
  spawnSync(
    "node",
    [
      script(),
      fixture(
        `shaped-${Math.random()}.jsonl`,
        [1, 2, 3].map(() => JSON.stringify({ body: JSON.stringify({ tools: [{ name: "memory_search" }], ...shape(VIEW) }) })).join("\n"),
      ),
      "memory_search",
      "--no-reminder",
      `--session=${hookOutput(VIEW)}`,
      "--budget=4000",
    ],
    { encoding: "utf8" },
  );

for (const [name, shape] of Object.entries(DELIVERED)) {
  test(`--session passes when the view is in the ${name}`, () => {
    const run = runShaped(shape);
    assert.strictEqual(run.status, 0, run.stdout + run.stderr);
  });
}

for (const [name, shape] of Object.entries(UNDELIVERED)) {
  test(`--session fails when the view is only in ${name}`, () => {
    assert.strictEqual(runShaped(shape).status, 1);
  });
}
