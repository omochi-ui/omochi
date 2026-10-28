import { App } from "@omochi-ui/sdk";
import { ApprovalQueue } from "./queue.ts";
import { record, type Agent } from "./protocol.ts";

const names: Record<Agent, string> = { codex: "Codex", "claude-code": "Claude Code", cursor: "Cursor", opencode: "OpenCode" };
const escapeText = (value: string) => value.replace(/[\\`*_{}\[\]()<>#+.!|~-]/g, "\\$&");

export function codeBlock(value: unknown): string {
  const text = JSON.stringify(value, null, 2);
  const longest = Math.max(2, ...(text.match(/`+/g) ?? []).map((run) => run.length));
  const fence = "`".repeat(longest + 1);
  return `${fence}json\n${text}\n${fence}`;
}

export function createApprovalApp(queue: ApprovalQueue) {
  const app = new App({ now: Date.now() });
  const decide = app.action("decide", (value) => {
    // Stale tabs and malformed actions must not terminate the service's receive loop.
    try {
      const input = record(value);
      if (typeof input.id !== "string" || (input.outcome !== "approve" && input.outcome !== "reject")) return;
      if (input.note !== undefined && typeof input.note !== "string") return;
      queue.decide(input.id, input.outcome, typeof input.note === "string" ? input.note : "");
    } catch { /* Ignore malformed actions. */ }
  });
  app.surface("main", (ui, state) => {
    ui.text("Agent approvals", { key: "title", variant: "h2" });
    ui.text("Review tool calls from your coding agents. Rejections can include a note.", { key: "intro" });
    ui.text(`${queue.pending.size} waiting · ${queue.history.length} recent`, { key: "count", variant: "caption" });
    if (!queue.pending.size) ui.text("No pending requests. Your next hooked tool call will appear here.", { key: "empty" });
    for (const request of queue.pending.values()) {
      const key = request.id;
      const form = ui.local(`note:${key}`, { note: "" });
      ui.card({ key }, (ui) => ui.column({ key: `${key}:content` }, (ui) => {
        ui.text(`${names[request.agent]} · ${escapeText(request.tool)}`, { key: `${key}:title`, variant: "h3" });
        ui.text(`Working directory: ${escapeText(request.cwd)}`, { key: `${key}:cwd`, variant: "caption" });
        ui.text(`Session: ${escapeText(request.sessionId ?? "not provided")} · Call: ${escapeText(request.toolCallId ?? "not provided")}`, { key: `${key}:session`, variant: "caption" });
        const seconds = Math.max(0, Math.floor((state.now - request.receivedAt) / 1000));
        const remaining = Math.max(0, Math.ceil((request.expiresAt - state.now) / 1000));
        ui.text(`Waiting ${seconds}s · returns to agent permissions in ${remaining}s`, { key: `${key}:time`, variant: "caption" });
        ui.text(codeBlock(request.input), { key: `${key}:input` });
        ui.textField(`note:${key}`, { key: `${key}:note`, label: "Rejection note (optional)", variant: "longText", bind: form.bind("/note") });
        ui.row({ key: `${key}:buttons`, justify: "start" }, (ui) => {
          ui.button("Approve", decide.call({ id: key, outcome: "approve" }), { key: `${key}:approve`, variant: "primary" });
          ui.button("Reject", decide.call({ id: key, outcome: "reject", note: form.value("/note") }), { key: `${key}:reject` });
        });
      }));
    }
    if (queue.history.length) {
      ui.divider({ key: "divider" });
      ui.text("Recent decisions", { key: "history-title", variant: "h3" });
      for (const item of queue.history) {
        const outcome = { approve: "Approved", reject: "Rejected", abstain: "Returned to agent permissions" }[item.outcome];
        ui.text(`${outcome} · ${names[item.agent]} · ${escapeText(item.tool)} · ${escapeText(item.sessionId ?? item.id)}` +
          (item.reason ? `\n${escapeText(item.reason)}` : ""), { key: `history:${item.id}` });
      }
    }
  });
  return app;
}
