import path from "node:path";
import { omochiRuntimeRoot } from "@omochi-ui/sdk/node";
import type { JsonValue } from "@omochi-ui/sdk";

export const AGENTS = ["codex", "claude-code", "cursor", "opencode"] as const;
export type Agent = typeof AGENTS[number];
export const WAIT_MS = 5 * 60_000;
export const MAX_FRAME_BYTES = 1024 * 1024;
export const DEFAULT_REJECTION = "Rejected by the user in Omochi.";

export interface ApprovalRequest {
  agent: Agent;
  sessionId?: string;
  toolCallId?: string;
  cwd: string;
  tool: string;
  input: JsonValue;
}

export interface Decision {
  outcome: "approve" | "reject" | "abstain";
  reason?: string;
}

export interface ApprovalResponse extends Decision {
  requestId?: string;
}

export function approvalSocketPath(env: NodeJS.ProcessEnv = process.env): string {
  return env.OMOCHI_APPROVAL_SOCKET ?? path.join(omochiRuntimeRoot(env), "agent-approval", "approval.sock");
}

export function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Expected an object");
  return value as Record<string, unknown>;
}

export function parseRequest(value: unknown): ApprovalRequest {
  const item = record(value);
  if (!AGENTS.includes(item.agent as Agent) || typeof item.cwd !== "string" ||
      typeof item.tool !== "string" || !item.tool || !Object.hasOwn(item, "input")) {
    throw new Error("Invalid approval request");
  }
  for (const key of ["sessionId", "toolCallId"]) {
    if (item[key] !== undefined && typeof item[key] !== "string") throw new Error(`Invalid ${key}`);
  }
  return {
    agent: item.agent as Agent, cwd: item.cwd, tool: item.tool, input: item.input as JsonValue,
    ...(typeof item.sessionId === "string" ? { sessionId: item.sessionId } : {}),
    ...(typeof item.toolCallId === "string" ? { toolCallId: item.toolCallId } : {}),
  };
}

export function parseResponse(value: unknown): ApprovalResponse {
  const item = record(value);
  if (!["approve", "reject", "abstain"].includes(item.outcome as string) ||
      (item.reason !== undefined && typeof item.reason !== "string") ||
      (item.requestId !== undefined && typeof item.requestId !== "string")) {
    throw new Error("Invalid approval response");
  }
  return item as unknown as ApprovalResponse;
}
