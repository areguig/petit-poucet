// The mock model for the agent tests: aimock answers "ok" in each provider's format (Anthropic, OpenAI, Gemini).
// Its journal keeps a normalized, size-capped copy of each request, so the raw request is logged here first.
// Usage: node model.mjs <log file>; prints the port the agents should use.
import { appendFileSync } from "node:fs";
import { createServer } from "node:http";
import { LLMock } from "@copilotkit/aimock";

const log = process.argv[2];
const mock = new LLMock({ port: 0, logLevel: "silent" });
// Codex groups MCP tools in a namespace that its calls must name, a field aimock doesn't write.
const namespace = process.env.REVIEW_NAMESPACE;
const asked = (prompt) => (m) => m.role === "user" && JSON.stringify(m.content).includes(prompt);
// Once only: agents report tool results in different shapes, so a later request can't tell it was answered.
const callOnce = (prompt, tool, argumentsOf) => {
  let called = false;
  const due = (req) => !called && req.tools?.length > 0 && req.messages.some(asked(prompt)) && argumentsOf(req) !== undefined;
  mock.on({ predicate: (req) => due(req) && (called = true) }, (req) => ({ toolCalls: [{ name: tool, arguments: argumentsOf(req) }] }));
};
// With REVIEW_TOOL set, the first request naming petit-poucet-review gets one call to that tool, with REVIEW_ARGS.
const review = process.env.REVIEW_TOOL;
if (review) callOnce("petit-poucet-review", review, () => process.env.REVIEW_ARGS ?? "{}");
// With READ_TOOL set, the first request naming petit-poucet-read gets one call to that tool, with READ_ARGS where {name} is
// the short name that the session view in the requests shows for the note summarized "short-name probe": what a model reading the view would pass.
// The journal cuts long requests, so the name is taken from the raw request, in the recorder below.
const read = process.env.READ_TOOL;
const PROBE_LINE = /\[\[([a-z0-9]+(?:-[a-z0-9]+)*)\]\] — short-name probe/;
let probeName;
if (read) callOnce("petit-poucet-read", read, () => probeName && process.env.READ_ARGS.replace("{name}", probeName));
// With SAVE_TOOL set, the first request naming petit-poucet-save gets one call to that tool, with SAVE_ARGS.
const save = process.env.SAVE_TOOL;
if (save) callOnce("petit-poucet-save", save, () => process.env.SAVE_ARGS);
mock.on({ predicate: () => true }, { content: "ok" });
await mock.start();

const recorder = createServer(async (req, res) => {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const body = Buffer.concat(chunks);
  // Parsed and written again, so that an agent's \u2014 escape reads as the dash.
  const view = body.length ? JSON.stringify(JSON.parse(body)) : "";
  probeName = view.match(PROBE_LINE)?.[1] ?? probeName;
  if (body.length) appendFileSync(log, JSON.stringify({ path: req.url, body: body.toString() }) + "\n");
  const answer = await fetch(mock.url + req.url, {
    method: req.method,
    headers: req.headers,
    body: body.length ? body : undefined,
  });
  if (namespace) {
    const text = [review, read, save].reduce(
      (text, tool) => (tool ? text.replaceAll(`"name":"${tool}"`, `"name":"${tool}","namespace":"${namespace}"`) : text),
      await answer.text(),
    );
    const headers = Object.fromEntries(answer.headers);
    delete headers["content-length"];
    res.writeHead(answer.status, headers);
    res.end(text);
    return;
  }
  res.writeHead(answer.status, Object.fromEntries(answer.headers));
  for await (const chunk of answer.body ?? []) res.write(chunk);
  res.end();
});
recorder.listen(0, "127.0.0.1", () => console.log(recorder.address().port));
