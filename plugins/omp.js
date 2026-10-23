// petit-poucet for omp: the memory in every model call of a session, and the save reminder when a session's turn ends.
// Written by `petit-poucet setup`, which also removes it on `setup --uninstall`.
import { spawnSync } from "node:child_process";

const EXE = __PETIT_POUCET__;

// The reply of `petit-poucet hook <event>`, which prints nothing when it has nothing to say.
function hook(event, input) {
  const run = spawnSync(EXE, ["hook", event, "--agent", "omp"], { input: JSON.stringify(input), encoding: "utf8" });
  try {
    return JSON.parse(run.stdout);
  } catch {
    return {};
  }
}

// Joined to the first user message rather than sent as a message of its own: some providers refuse two user turns in a row.
function withMemory(message, memory) {
  const content = typeof message.content === "string" ? [{ type: "text", text: message.content }] : message.content;
  return { ...message, content: [{ type: "text", text: memory }, ...content] };
}

export default function (pi) {
  let memory;

  pi.on("session_start", async (_event, ctx) => {
    memory = hook("session-start", { cwd: ctx.cwd }).context;
  });

  // Each model call gets the memory without it being saved in the session, so it never piles up.
  pi.on("context", async (event) => {
    const first = event.messages.findIndex((m) => m.role === "user");
    if (!memory || first < 0) return;
    const messages = [...event.messages];
    messages[first] = withMemory(messages[first], memory);
    return { messages };
  });

  pi.on("session_stop", async (event, ctx) => {
    const { reason } = hook("stop", { session_id: event.session_id, cwd: ctx.cwd, stop_hook_active: event.stop_hook_active });
    if (reason) return { decision: "block", reason };
  });
}
