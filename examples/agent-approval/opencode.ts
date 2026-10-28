import { createOpenCodeHook } from "./adapters.ts";

// Structural typing keeps the example independent of an installed OpenCode version.
// Export only the plugin: OpenCode treats every exported function as a plugin entry.
export default async function omochiApproval({ directory }: { directory: string }) {
  return { "tool.execute.before": createOpenCodeHook(directory) };
}
