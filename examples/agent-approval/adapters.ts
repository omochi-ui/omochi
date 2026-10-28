import { requestApproval, type ClientOptions } from "./client.ts";
import { DEFAULT_REJECTION, record, type Agent, type ApprovalRequest, type Decision } from "./protocol.ts";
import type { JsonValue } from "@omochi-ui/sdk";

export type CommandAgent = Exclude<Agent, "opencode">;

export function normalizeHook(agent: CommandAgent, value: unknown): ApprovalRequest {
  const input = record(value);
  const event = agent === "cursor" ? "preToolUse" : "PreToolUse";
  if (input.hook_event_name !== event || typeof input.tool_name !== "string" || !input.tool_name ||
      !Object.hasOwn(input, "tool_input")) throw new Error("Expected a tool pre-execution hook.");
  const sessionId = typeof input.session_id === "string" ? input.session_id : input.conversation_id;
  const roots = input.workspace_roots;
  const cwd = typeof input.cwd === "string" ? input.cwd
    : Array.isArray(roots) && typeof roots[0] === "string" ? roots[0] : process.cwd();
  return {
    agent, cwd, tool: input.tool_name, input: input.tool_input as JsonValue,
    ...(typeof sessionId === "string" ? { sessionId } : {}),
    ...(typeof input.tool_use_id === "string" ? { toolCallId: input.tool_use_id } : {}),
  };
}

export function hookOutput(agent: CommandAgent, decision: Decision): string {
  // No output + exit 0 means no decision. In Cursor this requires failClosed: false.
  if (decision.outcome === "abstain") return "";
  const rejected = decision.outcome === "reject";
  const reason = decision.reason?.trim() || DEFAULT_REJECTION;
  const output = agent === "cursor"
    ? { permission: rejected ? "deny" : "allow", ...(rejected ? { user_message: reason, agent_message: reason } : {}) }
    : { hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: rejected ? "deny" : "allow",
      ...(rejected ? { permissionDecisionReason: reason } : {}),
    } };
  return `${JSON.stringify(output)}\n`;
}

export function createOpenCodeHook(directory: string, options: ClientOptions = {}) {
  return async (input: { tool: string; sessionID: string; callID: string }, output: { args: Record<string, unknown> }) => {
    let decision: Decision;
    try {
      decision = await requestApproval({
        agent: "opencode", cwd: directory, tool: input.tool,
        sessionId: input.sessionID, toolCallId: input.callID, input: output.args as JsonValue,
      }, options);
    } catch { return; }
    // Keep the rejection outside the fail-open catch so a user refusal cannot be swallowed.
    if (decision.outcome === "reject") throw new Error(decision.reason || DEFAULT_REJECTION);
  };
}
