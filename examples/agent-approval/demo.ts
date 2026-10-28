import { requestApproval } from "./client.ts";
import { AGENTS, type ApprovalRequest } from "./protocol.ts";

// These are display-only requests: the demo never executes their commands.
const requests: Pick<ApprovalRequest, "tool" | "input">[] = [
  { tool: "Bash", input: { command: "git status --short" } },
  { tool: "Write", input: { file_path: "src/greeting.ts", content: "export const greeting = 'Hello, Omochi!';\n" } },
  { tool: "Shell", input: { command: "bun run test" } },
  { tool: "bash", input: { command: "git push origin main" } },
];

await Promise.all(AGENTS.map(async (agent, index) => {
  const response = await requestApproval({
    agent, sessionId: `demo-${agent}`, toolCallId: `demo-${index + 1}`, cwd: process.cwd(), ...requests[index]!,
  });
  console.log(`${agent}: ${JSON.stringify(response)}`);
}));
