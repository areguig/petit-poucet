// What the model was sent as tool results, whichever API an agent speaks.
import { readFileSync } from "node:fs";

// Where each API carries a tool's result: Anthropic, OpenAI Chat Completions, OpenAI Responses, Gemini.
export const toolResults = (node) => {
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
export const texts = (value) => {
  if (typeof value === "string") {
    const parsed = json(value);
    return parsed ? [value, ...texts(parsed)] : [value];
  }
  return value && typeof value === "object" ? Object.values(value).flatMap(texts) : [];
};


// Every tool result in a capture of requests (one JSON line each: path and body).
export const resultsIn = (log) =>
  readFileSync(log, "utf8")
    .split("\n")
    .filter((line) => line.trim())
    .flatMap((line) => toolResults(JSON.parse(JSON.parse(line).body)));
