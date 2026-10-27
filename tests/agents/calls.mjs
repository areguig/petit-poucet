// Checks the model calls of a three-turn session: each carries petit-poucet's memory once with its clean-as-you-go rule, its tools and its skills,
// and the save reminder comes once, after the third turn; with --no-reminder (an agent without a hook after an answer), never.
// Usage: node calls.mjs <log file> <text that shows the agent offered petit-poucet's tools> [--no-reminder] [--skill=<name>]
// The skill looked for is tidy-memory, unless --skill names another for an agent that doesn't get it.
import { readFileSync } from "node:fs";

const [log, tools, ...options] = process.argv.slice(2);
const reminded = !options.includes("--no-reminder");
const skill = options.find((o) => o.startsWith("--skill="))?.slice("--skill=".length) ?? "tidy-memory";
const MEMORY = "the Index below is already loaded";
const CLEAN_AS_YOU_GO = "fix it now (memory_save or memory_delete)";
const count = (text, part) => text.split(part).length - 1;

// Calls without tools are the agents' side requests (conversation titles, summaries).
const calls = readFileSync(log, "utf8")
  .trim()
  .split("\n")
  .map((line) => JSON.parse(line).body)
  .filter((body) => JSON.parse(body).tools?.length);

let ok = calls.length === (reminded ? 4 : 3);
const reminders = calls.map((call, i) => {
  const memory = count(call, MEMORY);
  const offered = call.includes(tools);
  const skills = call.includes(skill);
  const rule = call.includes(CLEAN_AS_YOU_GO);
  const reminder = call.includes("Memory check");
  console.log(`call ${i + 1}: memory ${memory}x, rule ${rule}, tools ${offered}, skills ${skills}, reminder ${reminder}`);
  ok &&= memory === 1 && rule && offered && skills;
  return reminder;
});
ok &&= reminders.join() === (reminded ? "false,false,false,true" : "false,false,false");
if (!ok) console.log(`expected ${reminded ? "4 calls" : "3 calls"} with the memory once, its rule, the tools and skills, and ${reminded ? "the reminder in the 4th only" : "no reminder"}`);
process.exit(ok ? 0 : 1);
