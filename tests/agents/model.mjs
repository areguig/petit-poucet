// The mock model for the agent tests: aimock answers "ok" in each provider's format (Anthropic, OpenAI, Gemini).
// Its journal keeps a normalized, size-capped copy of each request, so the raw request is logged here first.
// Usage: node model.mjs <log file>; prints the port the agents should use.
import { appendFileSync } from "node:fs";
import { createServer } from "node:http";
import { LLMock } from "@copilotkit/aimock";

const log = process.argv[2];
const mock = new LLMock({ port: 0, logLevel: "silent" });
// With REVIEW_TOOL set, the first request naming petit-poucet-review gets one call to that tool, with REVIEW_ARGS.
const review = process.env.REVIEW_TOOL;
// Codex groups MCP tools in a namespace that its calls must name, a field aimock doesn't write.
const namespace = process.env.REVIEW_NAMESPACE;
if (review) {
  // Once only: agents report tool results in different shapes, so a later request can't tell it was answered.
  let called = false;
  const asked = (m) => m.role === "user" && JSON.stringify(m.content).includes("petit-poucet-review");
  mock.on(
    { predicate: (req) => !called && req.tools?.length > 0 && req.messages.some(asked) && (called = true) },
    { toolCalls: [{ name: review, arguments: process.env.REVIEW_ARGS ?? "{}" }] },
  );
}
mock.on({ predicate: () => true }, { content: "ok" });
await mock.start();

const recorder = createServer(async (req, res) => {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const body = Buffer.concat(chunks);
  if (body.length) appendFileSync(log, JSON.stringify({ path: req.url, body: body.toString() }) + "\n");
  const answer = await fetch(mock.url + req.url, {
    method: req.method,
    headers: req.headers,
    body: body.length ? body : undefined,
  });
  if (namespace) {
    const text = (await answer.text()).replaceAll(`"name":"${review}"`, `"name":"${review}","namespace":"${namespace}"`);
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
