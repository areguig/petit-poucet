// Fixture test of calls.mjs --presence: only names and booleans are printed, never what the calls contain.
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
