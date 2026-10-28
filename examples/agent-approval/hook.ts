import { hookOutput, normalizeHook, type CommandAgent } from "./adapters.ts";
import { requestApproval } from "./client.ts";
import { MAX_FRAME_BYTES } from "./protocol.ts";

async function main(): Promise<void> {
  const agent = process.argv[2];
  if (!["codex", "claude-code", "cursor"].includes(agent ?? "")) {
    console.error("Usage: bun hook.ts codex|claude-code|cursor");
    return;
  }
  const controller = new AbortController();
  const cancel = () => { controller.abort(); process.stdin.destroy(); };
  process.once("SIGINT", cancel);
  process.once("SIGTERM", cancel);
  // This outer deadline also covers a stalled stdin. Agent settings use 360 seconds.
  const deadline = setTimeout(cancel, 301_000);
  try {
    const chunks: Buffer[] = [];
    let bytes = 0;
    for await (const chunk of process.stdin) {
      const data = Buffer.from(chunk);
      bytes += data.length;
      if (bytes > MAX_FRAME_BYTES) throw new Error("Hook input is too large.");
      chunks.push(data);
    }
    const request = normalizeHook(agent as CommandAgent, JSON.parse(Buffer.concat(chunks).toString("utf8")));
    const decision = await requestApproval(request, { signal: controller.signal });
    if (decision.outcome === "abstain") console.error(`Omochi: ${decision.reason} Using the agent's normal permissions.`);
    process.stdout.write(hookOutput(agent as CommandAgent, decision));
  } catch (error) {
    console.error(`Omochi: ${error instanceof Error ? error.message : "Hook unavailable."} Using the agent's normal permissions.`);
  } finally {
    clearTimeout(deadline);
    process.off("SIGINT", cancel);
    process.off("SIGTERM", cancel);
  }
}

if (import.meta.main) await main();
