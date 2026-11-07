// Checks that the first memory_review page of big_vault's notes reached the model whole and inline, in a tool result:
// its header, then its notes in order from the first up to where src/review.rs ends page 1, then its line naming page 2.
// Only the first page is called for, so the later pages are not checked.
// Usage: node review.mjs <log file>; prints one line per tool result, never what the calls contain.
import { readFileSync } from "node:fs";

const fail = (reason) => {
  console.error(`review check error: ${reason}`);
  process.exit(1);
};

// Where each API carries a tool's result: Anthropic, OpenAI Chat Completions, OpenAI Responses, Gemini.
const toolResults = (node) => {
  if (Array.isArray(node)) return node.flatMap(toolResults);
  if (!node || typeof node !== "object") return [];
  if (node.type === "tool_result") return [node.content];
  if (node.role === "tool") return [node.content];
  if (node.type === "function_call_output") return [node.output];
  if (node.functionResponse) return [node.functionResponse.response];
  return Object.values(node).flatMap(toolResults);
};
// Some agents pass the MCP result on as JSON text, which Hermes frames in an untrusted-data block,
// so the JSON a string is, or holds between its first { and last }, is searched inside too.
const json = (text) => {
  for (const candidate of [text, text.slice(text.indexOf("{"), text.lastIndexOf("}") + 1)]) {
    try {
      const parsed = JSON.parse(candidate);
      if (parsed && typeof parsed === "object") return parsed;
    } catch {}
  }
};
const texts = (value) => {
  if (typeof value === "string") {
    const parsed = json(value);
    return parsed ? [value, ...texts(parsed)] : [value];
  }
  return value && typeof value === "object" ? Object.values(value).flatMap(texts) : [];
};

// big_vault's notes, in the order the review lists them.
const NOTES = Array.from({ length: 250 }, (_, i) => `fact-${i}.md`)
  .sort()
  .map((name) => name.slice("fact-".length, -".md".length));
const NOTE = /^- fact-(\d+) \| project \| 2026-09-01 \| ([^|]*) \| fact (\d+) of a large vault, worded like a real one-line summary$/;
// src/review.rs's page contract: PAGE_BYTES less the header and more: line at their widest page numbers (usize::MAX),
// and a note goes on the next page once it no longer fits, so the vault path's length moves where page 1 ends.
const PAGE_BYTES = 3 * 1024;
const WIDEST = "18446744073709551615";
const bytes = (text) => Buffer.byteLength(text);
const holdsFirstPage = (text) => {
  const lines = text.split(/\r?\n/);
  const header = lines.findIndex((line) => /^page 1 of \d+ \(/.test(line));
  const more = lines.indexOf("more: call memory_review with page=2");
  if (header < 2 || more < header || lines[header + 1] !== "## Projects/big") return false;
  const notes = lines.slice(header + 2, more).map((line) => line.match(NOTE));
  if (!notes.length || !notes.every((m, i) => m && m[1] === NOTES[i] && m[3] === NOTES[i])) return false;
  const widest = lines[header].replace(/^page 1 of \d+/, `page ${WIDEST} of ${WIDEST}`);
  const budget = PAGE_BYTES - bytes(`${lines[header - 2]}\n${lines[header - 1]}\n${widest}\nmore: call memory_review with page=${WIDEST}\n`);
  const body = bytes(lines.slice(header + 1, more).join("\n") + "\n");
  const next = NOTES[notes.length];
  const nextLine = `- fact-${next} | project | 2026-09-01 | ${notes.at(-1)[2]} | fact ${next} of a large vault, worded like a real one-line summary`;
  return body <= budget && body + bytes(nextLine) + 1 > budget;
};

let results;
try {
  results = readFileSync(process.argv[2], "utf8")
    .split("\n")
    .filter((line) => line.trim())
    .flatMap((line) => toolResults(JSON.parse(JSON.parse(line).body)));
} catch {
  fail("unreadable or malformed capture");
}
if (!results.length) fail("no tool results");
const found = results.map((result) => texts(result).some(holdsFirstPage));
found.forEach((whole, i) => console.log(`tool result ${i + 1}: first page whole ${whole}`));
if (!found.includes(true)) fail("no tool result holds the whole first page");
