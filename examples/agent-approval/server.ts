import { chmod, lstat, unlink } from "node:fs/promises";
import { createConnection, createServer, type Socket } from "node:net";
import { ApprovalQueue } from "./queue.ts";
import { MAX_FRAME_BYTES, parseRequest, type ApprovalResponse } from "./protocol.ts";

/** The caller owns the private service directory and closes this before its A2UI transport. */
export async function startApprovalServer(socketPath: string, queue: ApprovalQueue) {
  if (Buffer.byteLength(socketPath) > 100) throw new Error("Approval socket path is too long.");
  // Recover only a dead socket. Never unlink a live instance, a regular file, or a symlink.
  const existing = await lstat(socketPath).catch((error: NodeJS.ErrnoException) => {
    if (error.code !== "ENOENT") throw error;
    return null;
  });
  if (existing) {
    if (!existing.isSocket()) throw new Error(`Not a socket: ${socketPath}`);
    await new Promise<void>((resolve, reject) => {
      const probe = createConnection(socketPath);
      probe.once("connect", () => { probe.destroy(); reject(new Error("Approval service is already running.")); });
      probe.once("error", (error: NodeJS.ErrnoException) => {
        if (error.code === "ECONNREFUSED" || error.code === "ENOENT") resolve();
        else reject(error);
      });
    });
    await unlink(socketPath).catch((error: NodeJS.ErrnoException) => { if (error.code !== "ENOENT") throw error; });
  }
  const clients = new Set<Socket>();
  const server = createServer((socket) => {
    clients.add(socket);
    let buffer = "";
    let requestId: string | undefined;
    let answered = false;
    const reply = (response: ApprovalResponse) => {
      if (answered || socket.destroyed) return;
      answered = true;
      socket.end(`${JSON.stringify(response)}\n`);
    };
    // Bound connections that never send a complete frame, without shortening human review.
    socket.setTimeout(5000, () => socket.destroy());
    socket.setEncoding("utf8");
    socket.on("data", (chunk: string) => {
      if (requestId || answered) return;
      buffer += chunk;
      if (Buffer.byteLength(buffer) > MAX_FRAME_BYTES) {
        reply({ outcome: "abstain", reason: "Approval request is too large." });
        return;
      }
      const newline = buffer.indexOf("\n");
      if (newline < 0) return;
      try {
        const request = parseRequest(JSON.parse(buffer.slice(0, newline)));
        socket.setTimeout(0);
        const pending = queue.submit(request);
        requestId = pending.id;
        buffer = "";
        void pending.result.then(reply);
      } catch { reply({ outcome: "abstain", reason: "Invalid approval request." }); }
    });
    socket.on("error", () => socket.destroy());
    socket.on("close", () => {
      clients.delete(socket);
      if (requestId) queue.cancel(requestId);
    });
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(socketPath, () => { server.off("error", reject); resolve(); });
  });
  await chmod(socketPath, 0o600);
  let closePromise: Promise<void> | undefined;
  return {
    close(): Promise<void> {
      return closePromise ??= (async () => {
        queue.close();
        // Deliver abstentions before closing sockets, including clients without a request.
        await Promise.resolve();
        for (const socket of clients) socket.end();
        const forceClose = setTimeout(() => { for (const socket of clients) socket.destroy(); }, 100);
        await new Promise<void>((resolve) => server.close(() => resolve()));
        clearTimeout(forceClose);
      })();
    },
  };
}
