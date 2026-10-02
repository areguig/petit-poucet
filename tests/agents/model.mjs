// The mock model for the agent tests: aimock answers "ok" in each provider's format (Anthropic, OpenAI, Gemini).
// Its journal keeps a normalized, size-capped copy of each request, so the raw request is logged here first.
// Usage: node model.mjs <log file>; prints the port the agents should use.
import { appendFileSync } from "node:fs";
import { createServer } from "node:http";
import { LLMock } from "@copilotkit/aimock";

const log = process.argv[2];
const mock = new LLMock({ port: 0, logLevel: "silent" });
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
  res.writeHead(answer.status, Object.fromEntries(answer.headers));
  for await (const chunk of answer.body ?? []) res.write(chunk);
  res.end();
});
recorder.listen(0, "127.0.0.1", () => console.log(recorder.address().port));
