// Checks the model calls of a three-turn session: each carries petit-poucet's memory once, its tools and its skills,
// and the save reminder comes once, after the third turn.
// Usage: node calls.mjs <log file> <text that shows the agent offered petit-poucet's tools>
import { readFileSync } from "node:fs";

const [log, tools] = process.argv.slice(2);
const MEMORY = "the Index below is already loaded";
const count = (text, part) => text.split(part).length - 1;

// Calls without tools are the agents' side requests (conversation titles, summaries).
const calls = readFileSync(log, "utf8")
  .trim()
  .split("\n")
  .map((line) => JSON.parse(line).body)
  .filter((body) => JSON.parse(body).tools?.length);

let ok = calls.length === 4;
const reminders = calls.map((call, i) => {
  const memory = count(call, MEMORY);
  const offered = call.includes(tools);
  const skills = call.includes("tidy-memory");
  const reminder = call.includes("Memory check");
  console.log(`call ${i + 1}: memory ${memory}x, tools ${offered}, skills ${skills}, reminder ${reminder}`);
  ok &&= memory === 1 && offered && skills;
  return reminder;
});
ok &&= reminders.join() === "false,false,false,true";
if (!ok) console.log("expected 4 calls with the memory once, the tools and skills, and the reminder in the 4th only");
process.exit(ok ? 0 : 1);
