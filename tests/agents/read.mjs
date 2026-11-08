// Checks that a model which read a note by the short name its session view shows got the note's body back, in a tool result.
// Usage: node read.mjs <log file> <text only the note's body holds>; prints one line per tool result, never what the calls contain.
import { resultsIn, texts } from "./tool_results.mjs";

const fail = (reason) => {
  console.error(`read check error: ${reason}`);
  process.exit(1);
};

const [log, body] = process.argv.slice(2);
let results;
try {
  results = resultsIn(log);
} catch {
  fail("unreadable or malformed capture");
}
if (!results.length) fail("no tool results");
const found = results.map((result) => texts(result).some((text) => text.includes(body)));
found.forEach((holds, i) => console.log(`tool result ${i + 1}: note body ${holds}`));
if (!found.includes(true)) fail("no tool result holds the note's body");
