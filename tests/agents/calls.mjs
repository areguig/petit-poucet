// Checks the model calls of a three-turn session: each carries petit-poucet's memory once with its clean-as-you-go rule, its tools and its skills,
// and the save reminder comes once, after the third turn; with --no-reminder (an agent without a hook after an answer), never.
// Usage: node calls.mjs <log file> <text that shows the agent offered petit-poucet's tools> [--no-reminder] [--skill=<name>]
// With --presence=<a,b,c> it only prints, per call, whether each name appears (a diagnostic: exit 1 with a fixed message if the capture has no usable calls).
// The skill looked for is tidy-memory, unless --skill names another for an agent that doesn't get it.
import { readFileSync } from "node:fs";

const [log, tools, ...options] = process.argv.slice(2);
const reminded = !options.includes("--no-reminder");
const skill = options.find((o) => o.startsWith("--skill="))?.slice("--skill=".length) ?? "tidy-memory";
const MEMORY = "the Index below is already loaded";
const CLEAN_AS_YOU_GO = "fix it now (memory_save or memory_delete)";
const count = (text, part) => text.split(part).length - 1;

// Calls without tools are the agents' side requests (conversation titles, summaries).
const readCalls = () =>
  readFileSync(log, "utf8")
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line).body)
    .filter((body) => JSON.parse(body).tools?.length);

const presence = options.find((o) => o.startsWith("--presence="))?.slice("--presence=".length).split(",");
// A name counts only as a whole token, so "memory" doesn't match inside "migrate-memory".
const hasToken = (text, name) => new RegExp(`(?<![-\\p{L}\\p{N}_])${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![-\\p{L}\\p{N}_])`, "u").test(text);
if (presence) {
  // Only a fixed message is printed on failure: the capture holds the prompts, so no error text may leak.
  const fail = (reason) => {
    console.error(`presence diagnostic error: ${reason}`);
    process.exit(1);
  };
  let found;
  try {
    found = readCalls();
  } catch {
    fail("unreadable or malformed capture");
  }
  if (!found.length) fail("no calls with tools");
  found.forEach((call, i) => console.log(`call ${i + 1}: ${presence.map((name) => `${name} ${hasToken(call, name)}`).join(", ")}`));
  process.exit(0);
}

const calls = readCalls();

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
