// Fixture test of lib.sh's check_review, run as the agent scripts run it: only the first review page, whole and inline
// in a tool result, passes; a verdict never shows what the calls contain.
import assert from "node:assert";
import { spawnSync } from "node:child_process";
import { chmodSync, cpSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const SECRET = "SECRET_BODY /home/someone/vault API_KEY=hunter2";
const here = dirname(fileURLToPath(import.meta.url));
// lib.sh runs from a repo root with a release build: a stub binary stands in, as check_review never runs it.
const root = mkdtempSync(join(tmpdir(), "review-root-"));
cpSync(here, join(root, "tests/agents"), { recursive: true, filter: (path) => !path.includes("node_modules") });
mkdirSync(join(root, "target/release"), { recursive: true });
for (const stub of ["petit-poucet", "petit-poucet.exe"]) {
  writeFileSync(join(root, "target/release", stub), "#!/bin/sh\n");
  chmodSync(join(root, "target/release", stub), 0o755);
}
const dir = mkdtempSync(join(tmpdir(), "review-calls-"));
let fixtures = 0;
const capture = (content) => {
  const file = join(dir, `calls-${++fixtures}.jsonl`);
  writeFileSync(file, content);
  return file;
};
const checkReview = (file) =>
  spawnSync("sh", ["-c", '. tests/agents/lib.sh && calls="$1" && check_review', "sh", file], {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, PATH: `${dirname(process.execPath)}${delimiter}${process.env.PATH}` },
  });

// big_vault's notes, in the order the review lists them.
const names = Array.from({ length: 250 }, (_, i) => `fact-${i}.md`).sort();
const note = (name) => {
  const n = name.slice("fact-".length, -".md".length);
  return `- fact-${n} | project | 2026-09-01 | never | fact ${n} of a large vault, worded like a real one-line summary`;
};
const HEADER = "vault: /synthetic/vault\nfirst cleanup: all 250 notes, then the problems found in the whole vault\npage 1 of 9 (slug | type | created[/updated] | reads and last read | summary)\n## Projects/big\n";
const lines = names.slice(0, 30).map(note);
const MORE = "more: call memory_review with page=2\n";
const PAGE = HEADER + lines.join("\n") + "\n" + MORE;

const call = (request) => JSON.stringify({ path: "/v1", body: JSON.stringify({ tools: [{ description: "memory_review: pages of notes" }], ...request }) });
// Each exercised agent's API, as the mock receives it: the page is the tool result of the call after the tool call.
const deliveries = {
  "claude (Anthropic content blocks)": (text) =>
    call({ messages: [{ role: "user", content: SECRET }, { role: "user", content: [{ type: "tool_result", tool_use_id: "t", content: [{ type: "text", text }] }] }] }),
  "codex (Responses, MCP content as JSON)": (text) =>
    call({ input: [{ role: "user", content: [{ type: "input_text", text: SECRET }] }, { type: "function_call_output", call_id: "c", output: JSON.stringify([{ type: "text", text }]) }] }),
  "copilot, omp, hermes (Chat Completions)": (text) =>
    call({ messages: [{ role: "user", content: SECRET }, { role: "tool", tool_call_id: "c", content: text }] }),
  "opencode (Chat Completions, execute's JSON result)": (text) =>
    call({ messages: [{ role: "tool", tool_call_id: "c", content: JSON.stringify({ content: [{ type: "text", text }] }) }] }),
  "antigravity (Gemini function response)": (text) =>
    call({ contents: [{ role: "user", parts: [{ text: SECRET }] }, { role: "user", parts: [{ functionResponse: { name: "call_mcp_tool", response: { output: text } } }] }] }),
};
const first = call({ messages: [{ role: "user", content: `${SECRET} petit-poucet-review` }] });

const assertSafe = (run) => {
  for (const leak of ["SECRET_BODY", "hunter2", "/home/someone", "/synthetic", "fact 0 of", "SyntaxError", "ENOENT", "    at "]) {
    assert.ok(!(run.stdout + run.stderr).includes(leak), `leaked ${leak}`);
  }
};
const assertPasses = (file) => {
  const run = checkReview(file);
  assert.strictEqual(run.status, 0, run.stderr);
  assertSafe(run);
};
const assertFails = (file) => {
  const run = checkReview(file);
  assert.strictEqual(run.status, 1);
  assert.match(run.stderr, /FAIL: the first memory_review page didn't reach the model whole and inline\n$/);
  assertSafe(run);
};

for (const [agent, deliver] of Object.entries(deliveries)) {
  test(`the whole first page inline passes for ${agent}`, () => assertPasses(capture([first, deliver(PAGE)].join("\n"))));
  // Agents cap or spill long tool results; each of these shapes reached a model in place of the page.
  for (const [name, text] of [
    ["the heading only", HEADER],
    ["the issue's spill preview", "page 1 of 12; remaining tool output saved to /synthetic/file"],
    ["the heading and a spill notice", `${HEADER}${lines[0]}\n[remaining tool output saved to /synthetic/file]`],
    ["a page cut before its end", `${HEADER}${lines.slice(0, 20).join("\n")}\n`],
    ["a page cut in the middle", `${HEADER}${lines.slice(0, 5).join("\n")}\n[... 2000 characters omitted ...]\n${lines.slice(-5).join("\n")}\n${MORE}`],
    ["a page missing a note", `${HEADER}${lines.filter((_, i) => i !== 15).join("\n")}\n${MORE}`],
    ["a page missing its first note", `${HEADER}${lines.slice(1).join("\n")}\n${MORE}`],
  ]) {
    test(`${name} in a tool result fails for ${agent}`, () => assertFails(capture([first, deliver(text)].join("\n"))));
  }
}

for (const [name, request] of [
  ["the user prompt", { messages: [{ role: "user", content: `petit-poucet-review ${PAGE}` }] }],
  ["the tool definitions", { tools: [{ description: PAGE }], messages: [{ role: "user", content: "petit-poucet-review" }] }],
  ["the model's own tool call", { messages: [{ role: "assistant", tool_calls: [{ function: { name: "memory_review", arguments: JSON.stringify({ page: PAGE }) } }] }] }],
  ["the system prompt", { system: PAGE, messages: [{ role: "user", content: "petit-poucet-review" }] }],
]) {
  test(`the page in ${name}, not a tool result, fails`, () => assertFails(capture(call(request))));
}

for (const [name, file] of [
  ["a missing capture", join(dir, "missing.jsonl")],
  ["an empty capture", capture("")],
  ["a malformed capture", capture(`${SECRET} {not json\n`)],
  ["a malformed body", capture(JSON.stringify({ body: `${SECRET} ${PAGE} {not json` }))],
  ["a capture without tool results", capture(first)],
]) {
  test(`${name} fails without showing the capture`, () => assertFails(file));
}
