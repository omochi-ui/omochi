import { createConnection } from "node:net";
import { approvalSocketPath, MAX_FRAME_BYTES, parseResponse, WAIT_MS, type ApprovalRequest, type ApprovalResponse } from "./protocol.ts";

export interface ClientOptions {
  socketPath?: string;
  timeoutMs?: number;
  signal?: AbortSignal;
}

/** A failure abstains from deciding; it never manufactures an approval. */
export function requestApproval(request: ApprovalRequest, options: ClientOptions = {}): Promise<ApprovalResponse> {
  return new Promise((resolve) => {
    if (options.signal?.aborted) { resolve({ outcome: "abstain", reason: "Hook cancelled." }); return; }
    const socket = createConnection(options.socketPath ?? approvalSocketPath());
    let received = "";
    let settled = false;
    const finish = (response: ApprovalResponse) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      options.signal?.removeEventListener("abort", cancelled);
      socket.destroy();
      resolve(response);
    };
    const abstain = (reason: string) => finish({ outcome: "abstain", reason });
    const cancelled = () => abstain("Hook cancelled.");
    const timer = setTimeout(() => abstain("Approval timed out."), options.timeoutMs ?? WAIT_MS + 1000);
    options.signal?.addEventListener("abort", cancelled, { once: true });
    socket.setEncoding("utf8");
    socket.once("connect", () => {
      try {
        const frame = `${JSON.stringify(request)}\n`;
        if (Buffer.byteLength(frame) > MAX_FRAME_BYTES) { abstain("Approval request is too large."); return; }
        socket.write(frame);
      } catch { abstain("Invalid approval request."); }
    });
    socket.on("data", (chunk: string) => {
      received += chunk;
      if (Buffer.byteLength(received) > MAX_FRAME_BYTES) { abstain("Invalid approval response."); return; }
      const newline = received.indexOf("\n");
      if (newline < 0) return;
      try { finish(parseResponse(JSON.parse(received.slice(0, newline)))); }
      catch { abstain("Invalid approval response."); }
    });
    socket.on("error", () => abstain("Approval service unavailable."));
    socket.on("close", () => abstain("Approval service disconnected."));
  });
}
