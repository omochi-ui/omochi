import { expect } from "bun:test";
import { test, waitFor } from "@omochi-ui/core/testing";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

test("documented composition serves the dashboard and routes all four demo decisions", async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), "approval-up-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const repository = fileURLToPath(new URL("../../../", import.meta.url));
  const env: NodeJS.ProcessEnv = { ...process.env, XDG_RUNTIME_DIR: root, XDG_CONFIG_HOME: path.join(root, "config") };
  // Explicit endpoint avoids inheriting an unrelated developer override in the demo.
  env.OMOCHI_APPROVAL_SOCKET = path.join(root, "omochi", "agent-approval", "approval.sock");
  const hub = Bun.spawn([process.execPath, "packages/cli/bin/omochi-ui.js", "up", "--port", "0", "examples/agent-approval/omochi.toml"], {
    cwd: repository, env, stdout: "ignore", stderr: "pipe",
  });
  let log = "";
  const readLog = (async () => {
    for await (const chunk of hub.stderr) log += Buffer.from(chunk).toString();
  })();
  t.after(async () => { hub.kill("SIGTERM"); await hub.exited; await readLog; });
  await waitFor(() => /http:\/\/127\.0\.0\.1:\d+/.test(log), 8000);
  const url = log.match(/http:\/\/127\.0\.0\.1:\d+/)![0];
  const page = await fetch(url);
  expect(page.status).toBe(200);
  expect(await page.text()).toContain("<html");
  await waitFor(async () => (await (await fetch(`${url}/services`)).text()).includes("agent-approval"));

  const socket = new WebSocket(`${url.replace("http:", "ws:")}/ws`);
  t.after(() => socket.close());
  let surfaceId = "";
  const requests = new Map<string, string>();
  socket.onmessage = (event) => {
    const message = JSON.parse(String(event.data));
    if (message.createSurface) surfaceId = message.createSurface.surfaceId;
    for (const component of message.updateComponents?.components ?? []) {
      const input = component.action?.event?.context?.input;
      if (input?.outcome === "approve") requests.set(input.id, component.id);
    }
  };
  await waitFor(() => socket.readyState === WebSocket.OPEN && !!surfaceId);
  const demo = Bun.spawn([process.execPath, "examples/agent-approval/demo.ts"], {
    cwd: repository, env, stdout: "pipe", stderr: "pipe",
  });
  t.after(async () => { demo.kill(); await demo.exited; });
  await waitFor(() => requests.size === 4);
  for (const [index, [id, componentId]] of [...requests].entries()) {
    socket.send(JSON.stringify({
      version: "v0.9",
      action: {
        name: "decide", surfaceId, sourceComponentId: componentId, timestamp: new Date().toISOString(),
        context: { input: { id, outcome: index === 0 ? "reject" : "approve", note: "Demo refusal." } },
      },
    }));
  }
  expect(await demo.exited).toBe(0);
  const responses = (await new Response(demo.stdout).text()).trim().split("\n");
  expect(responses).toHaveLength(4);
  expect(responses.filter((line) => line.includes('"outcome":"approve"'))).toHaveLength(3);
  expect(responses.filter((line) => line.includes('"reason":"Demo refusal."'))).toHaveLength(1);
  expect(await new Response(demo.stderr).text()).toBe("");
});
