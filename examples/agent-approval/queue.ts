import { randomUUID } from "node:crypto";
import { DEFAULT_REJECTION, WAIT_MS, type ApprovalRequest, type ApprovalResponse, type Decision } from "./protocol.ts";

export interface PendingRequest extends ApprovalRequest {
  id: string;
  receivedAt: number;
  expiresAt: number;
}

export interface CompletedRequest extends PendingRequest, Decision {
  completedAt: number;
}

export class ApprovalQueue {
  readonly pending = new Map<string, PendingRequest>();
  readonly history: CompletedRequest[] = [];
  onChange: () => void = () => {};
  private readonly waiters = new Map<string, {
    resolve: (response: ApprovalResponse) => void;
    timer: ReturnType<typeof setTimeout>;
  }>();
  private closed = false;

  constructor(private readonly waitMs = WAIT_MS) {}

  submit(request: ApprovalRequest): { id: string; result: Promise<ApprovalResponse> } {
    const id = randomUUID();
    if (this.closed) return { id, result: Promise.resolve({ requestId: id, outcome: "abstain", reason: "Service stopped." }) };
    const receivedAt = Date.now();
    this.pending.set(id, { ...structuredClone(request), id, receivedAt, expiresAt: receivedAt + this.waitMs });
    const result = new Promise<ApprovalResponse>((resolve) => {
      const timer = setTimeout(() => this.settle(id, { outcome: "abstain", reason: "Approval timed out." }), this.waitMs);
      this.waiters.set(id, { resolve, timer });
    });
    this.onChange();
    return { id, result };
  }

  decide(id: string, outcome: "approve" | "reject", note = ""): boolean {
    const pending = this.pending.get(id);
    if (!pending) return false;
    // A late browser action must not win just because the timer callback is delayed.
    if (Date.now() >= pending.expiresAt) {
      this.settle(id, { outcome: "abstain", reason: "Approval timed out." });
      return false;
    }
    return this.settle(id, outcome === "reject"
      ? { outcome, reason: note.trim() || DEFAULT_REJECTION }
      : { outcome });
  }

  cancel(id: string): void {
    this.settle(id, { outcome: "abstain", reason: "Hook disconnected." });
  }

  close(): void {
    this.closed = true;
    for (const id of this.pending.keys()) this.settle(id, { outcome: "abstain", reason: "Service stopped." });
  }

  private settle(id: string, decision: Decision): boolean {
    const pending = this.pending.get(id);
    const waiter = this.waiters.get(id);
    if (!pending || !waiter) return false;
    clearTimeout(waiter.timer);
    this.pending.delete(id);
    this.waiters.delete(id);
    this.history.unshift({ ...pending, ...decision, completedAt: Date.now() });
    this.history.splice(100);
    waiter.resolve({ requestId: id, ...decision });
    this.onChange();
    return true;
  }
}
