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
