// Checks the model calls of a three-turn session: each carries petit-poucet's memory once with its clean-as-you-go rule, its tools and its skills,
// and the save reminder comes once, after the third turn; with --no-reminder (an agent without a hook after an answer), never.
// Usage: node calls.mjs <log file> <text that shows the agent offered petit-poucet's tools> [--no-reminder] [--skill=<name>]
// With --presence=<a,b,c> it only prints, per call, whether each name appears (a diagnostic: exit 1 with a fixed message if the capture has no usable calls).
// With --catalog=<a,b,c> [--codex-version=<v>] it prints instead whether each name is an entry of the skills catalog section, with the entry and character counts,
// and exits 1 if a call has no parsable catalog section or lacks one of the names.
// With --session=<the hook's reply file> (once or twice: the view before and after the update check adds its notice) --budget=<bytes>,
// the memory each call carries is exactly one of those whole session views, which must be partial (the vault is oversized), within the budget,
// and name the situational preference without listing it.
// The skill looked for is tidy-memory, unless --skill names another for an agent that doesn't get it.
import { readFileSync } from "node:fs";

const [log, tools, ...options] = process.argv.slice(2);
const reminded = !options.includes("--no-reminder");
const skill = options.find((o) => o.startsWith("--skill="))?.slice("--skill=".length) ?? "tidy-memory";
const MEMORY = "the Index below is already loaded";
// The vault's one situational preference (lib.sh oversize_memory): its summary must stay out of the view, which only counts it.
const SITUATIONAL_PROBE = "situational-probe-7c2e";
const SITUATIONAL_LINE = "situational preference not loaded (each applies only in some situations)";
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
// Diagnostics print one line per call and never what the calls contain; on failure only a fixed message is printed,
// because the capture holds the prompts, so no error text may leak.
const diagnose = (describe, label = "presence diagnostic", problem = () => undefined) => {
  const fail = (reason) => {
    console.error(`${label} error: ${reason}`);
    process.exit(1);
  };
  let lines;
  try {
    lines = readCalls().map(describe);
  } catch {
    fail("unreadable or malformed capture");
  }
  if (!lines.length) fail("no calls with tools");
  lines.forEach((line, i) => console.log(`call ${i + 1}: ${line}`));
  if (problem()) fail(problem());
  process.exit(0);
};
if (presence) diagnose((call) => presence.map((name) => `${name} ${hasToken(call, name)}`).join(", "));

// Codex lists skills as "- name: description (file: path)" lines under one "### Available skills" heading, inside a text of the request.
const CATALOG_HEADING = "### Available skills";
const strings = (value) =>
  typeof value === "string" ? [value] : value && typeof value === "object" ? Object.values(value).flatMap(strings) : [];
const catalogSections = (body) =>
  strings(JSON.parse(body)).flatMap((text) => {
    const lines = text.split(/\r?\n/);
    return lines.flatMap((line, i) => {
      if (line.trim() !== CATALOG_HEADING) return [];
      const rest = lines.slice(i + 1);
      const end = rest.findIndex((l) => /^#{1,6} /.test(l));
      return [(end < 0 ? rest : rest.slice(0, end)).join("\n")];
    });
  });
const reportCatalog = (names, version) => {
  let problem;
  console.log(`codex ${/^[\w. -]{1,40}$/.test(version) ? version : "unknown"}`);
  diagnose(
    (call) => {
      const sections = catalogSections(call);
      const entries = sections.length === 1 ? sections[0].split("\n").flatMap((line) => line.match(/^- (\S+?): /)?.slice(1) ?? []) : [];
      if (!entries.length) {
        problem ??= "catalog section not found or unparseable";
        return "catalog section not found or unparseable, no booleans";
      }
      if (!names.every((name) => entries.includes(name))) problem ??= "skill missing from catalog";
      return `catalog entries ${entries.length}, chars ${sections[0].length}, ${names.map((name) => `${name} ${entries.includes(name)}`).join(", ")}`;
    },
    "catalog check",
    () => problem,
  );
};
const catalog = options.find((o) => o.startsWith("--catalog="))?.slice("--catalog=".length).split(",");
if (catalog) reportCatalog(catalog, options.find((o) => o.startsWith("--codex-version="))?.slice("--codex-version=".length) ?? "");

const calls = readCalls();

const budget = Number(options.find((o) => o.startsWith("--budget="))?.slice("--budget=".length));
const sessionFiles = options.filter((o) => o.startsWith("--session=")).map((o) => o.slice("--session=".length));
// The views the hook builds: before the update check has found a release and after, which adds its notice line.
const views = [
  ...new Set(
    sessionFiles.map((file) => strings(JSON.parse(readFileSync(file, "utf8"))).find((text) => text.startsWith("Agent memory (petit-poucet)"))),
  ),
];
const viewOk =
  !sessionFiles.length ||
  views.every(
    (view) =>
      view &&
      view.includes("Not loaded (over the") &&
      view.includes(SITUATIONAL_LINE) &&
      !view.includes(SITUATIONAL_PROBE) &&
      Buffer.byteLength(view) <= budget,
  );
if (!viewOk) console.log("a session view is missing, not partial, lacks the situational line, lists a situational preference, or is over the budget");
// The texts a request gives the model as context, in the formats the mock serves (Anthropic, OpenAI chat and responses, Gemini):
// system and instructions, and the system, developer and user messages; never tool results, tool-call arguments or other fields.
const CONTEXT_ROLES = ["system", "developer", "user"];
const texts = (content) =>
  typeof content === "string"
    ? [content]
    : Array.isArray(content)
      ? content.flatMap((part) => (["text", "input_text"].includes(part?.type) && typeof part.text === "string" ? [part.text] : []))
      : [];
const contextTexts = (request) => [
  ...texts(request.system),
  ...texts(request.instructions),
  ...(typeof request.input === "string" ? [request.input] : []),
  ...[...(request.messages ?? []), ...(Array.isArray(request.input) ? request.input : [])]
    .filter((m) => CONTEXT_ROLES.includes(m?.role) && (m.type ?? "message") === "message")
    .flatMap((m) => texts(m.content)),
  ...[request.systemInstruction, ...(request.contents ?? []).filter((c) => c?.role === "user")].flatMap((c) =>
    (c?.parts ?? []).flatMap((part) => (typeof part?.text === "string" ? [part.text] : [])),
  ),
];
const memoryIn = (call) =>
  sessionFiles.length
    ? contextTexts(JSON.parse(call)).reduce((sum, text) => sum + views.reduce((n, view) => n + count(text, view.trimEnd()), 0), 0)
    : count(call, MEMORY);

let ok = viewOk && calls.length === (reminded ? 4 : 3);
const reminders = calls.map((call, i) => {
  const memory = memoryIn(call);
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
