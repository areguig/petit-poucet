// Checks that a text reached the model in a tool result, e.g. a note's body after a read by the short name its session view shows.
// Usage: node holds.mjs <log file> <text>; prints one line per tool result, never what the calls contain.
import { resultsIn, texts } from "./tool_results.mjs";

const fail = (reason) => {
  console.error(`holds check error: ${reason}`);
  process.exit(1);
};

const [log, wanted] = process.argv.slice(2);
let results;
try {
  results = resultsIn(log);
} catch {
  fail("unreadable or malformed capture");
}
if (!results.length) fail("no tool results");
const found = results.map((result) => texts(result).some((text) => text.includes(wanted)));
found.forEach((holds, i) => console.log(`tool result ${i + 1}: text ${holds}`));
if (!found.includes(true)) fail("no tool result holds the text");
