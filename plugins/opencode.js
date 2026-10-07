// petit-poucet for OpenCode: the memory in every model call of a session, and the save reminder when a session's turn ends.
// Written by `petit-poucet setup`, which also removes it on `setup --uninstall`.
import { spawnSync } from "node:child_process";

const EXE = __PETIT_POUCET__;

// The reply of `petit-poucet hook <event>`, which prints nothing when it has nothing to say.
function hook(event, input) {
  const run = spawnSync(EXE, ["hook", event, "--agent", "opencode"], { input: JSON.stringify(input), encoding: "utf8" });
  try {
    return JSON.parse(run.stdout);
  } catch {
    return {};
  }
}

export default {
  id: "petit-poucet",
  async setup(ctx) {
    // The sessions this instance serves, with their folder and memory: OpenCode loads one instance per folder it has open.
    const sessions = new Map();
    const reminded = new Set();

    // The memory is read once per session and added to each of its model calls.
    await ctx.session.hook("context", async (event) => {
      if (!sessions.has(event.sessionID)) {
        const cwd = (await ctx.session.get({ sessionID: event.sessionID })).location.directory;
        sessions.set(event.sessionID, { cwd, context: hook("session-start", { session_id: event.sessionID, cwd }).context });
      }
      const { context } = sessions.get(event.sessionID);
      if (context) event.system.push({ type: "text", text: context });
    });

    const abort = new AbortController();
    (async () => {
      for await (const event of ctx.event.subscribe({ signal: abort.signal })) {
        // Every instance hears every session's events; only the one serving it counts its turns.
        if (event.type !== "session.execution.succeeded" || !sessions.has(event.data.sessionID)) continue;
        const sessionID = event.data.sessionID;
        // The turn that answers our own reminder must not remind again.
        const input = { session_id: sessionID, cwd: sessions.get(sessionID).cwd, stop_hook_active: reminded.delete(sessionID) };
        const reason = hook("stop", input).reason;
        if (!reason) continue;
        reminded.add(sessionID);
        await ctx.session.prompt({ sessionID, text: reason });
      }
    })();
    return () => abort.abort();
  },
};
