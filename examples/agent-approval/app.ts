import path from "node:path";
import { ensureRuntimeRoot } from "@omochi-ui/discovery";
import { omochiRuntimeRoot, UnixSocketServerTransport } from "@omochi-ui/sdk/node";
import { ApprovalQueue } from "./queue.ts";
import { startApprovalServer } from "./server.ts";
import { createApprovalApp } from "./ui.ts";

export async function startApprovalApp(options: { runtimeDir?: string; serviceId?: string; waitMs?: number } = {}) {
  const root = options.runtimeDir ?? omochiRuntimeRoot();
  await ensureRuntimeRoot(root);
  const transport = new UnixSocketServerTransport({
    serviceId: options.serviceId ?? process.env.OMOCHI_SERVICE ?? "agent-approval",
    name: "Agent approvals", icon: "✋", runtimeDir: root,
  });
  const queue = new ApprovalQueue(options.waitMs);
  const runtime = createApprovalApp(queue).build();
  let server: Awaited<ReturnType<typeof startApprovalServer>> | undefined;
  let tick: ReturnType<typeof setInterval> | undefined;
  let running: Promise<void> | undefined;
  let closePromise: Promise<void> | undefined;
  const close = () => closePromise ??= (async () => {
    clearInterval(tick);
    queue.onChange = () => {};
    queue.close();
    await server?.close();
    await transport.close();
    await running;
  })();
  try {
    await transport.ready;
    running = runtime.start(transport);
    void running.catch(() => {});
    await runtime.ready();
    const refresh = () => { void runtime.update((state) => { state.now = Date.now(); }).catch((error) => console.error(error)); };
    queue.onChange = refresh;
    const socketPath = path.join(transport.directory!, "approval.sock");
    server = await startApprovalServer(socketPath, queue);
    tick = setInterval(() => { if (queue.pending.size) refresh(); }, 1000);
    return { queue, runtime, transport, socketPath, close };
  } catch (error) {
    await close();
    throw error;
  }
}

if (import.meta.main) {
  const app = await startApprovalApp();
  console.error(`Agent approvals listening on ${app.socketPath}`);
  const stop = () => { void app.close().catch((error) => { console.error(error); process.exitCode = 1; }); };
  process.once("SIGINT", stop);
  process.once("SIGTERM", stop);
  try { await app.runtime.waitClosed(); }
  finally { await app.close(); }
}
