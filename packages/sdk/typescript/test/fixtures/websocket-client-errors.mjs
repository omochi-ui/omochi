import assert from "node:assert/strict";
import { once } from "node:events";
import { readFileSync } from "node:fs";
import { WebSocket } from "ws";
import { App, schema } from "@omochi-ui/sdk";
import { WebSocketServerTransport } from "@omochi-ui/sdk/node";

const fixture = JSON.parse(readFileSync(new URL("../../../../../spec/fixtures/client-errors.json", import.meta.url), "utf8"));
const actions = JSON.parse(readFileSync(new URL("../../../../../spec/fixtures/client-actions.json", import.meta.url), "utf8"));
const state = { count: 0 };
const app = new App(state);
const increment = app.action("increment", schema.literal(null), async (_input, context) => {
  await context.update(current => { current.count += 1; });
});
app.surface("main", (ui, current) => {
  ui.text(`Count: ${current.count}`, { key: "count" });
  ui.button("Increment", increment.call(null), { key: "increment" });
});

const transport = new WebSocketServerTransport({ port: 0 });
const running = app.start(transport);
const clients = [];
try {
  await transport.ready;
  const client = new WebSocket(transport.url);
  clients.push(client);
  const pending = [];
  const waiters = [];
  let closeError;
  client.on("message", data => {
    const message = JSON.parse(data.toString());
    if (message.omochi === "resources/1") return;
    const waiter = waiters.shift();
    if (waiter) waiter.resolve(message);
    else pending.push(message);
  });
  client.on("close", (code, reason) => {
    closeError = new Error(`unexpected close ${code}: ${reason}`);
    for (const waiter of waiters.splice(0)) waiter.reject(closeError);
  });
  const next = () => pending.length
    ? Promise.resolve(pending.shift())
    : closeError ? Promise.reject(closeError) : new Promise((resolve, reject) => waiters.push({ resolve, reject }));
  await once(client, "open");
  await Promise.all([next(), next(), next()]);

  const invoke = async (expectedCount, action = actions.actions[0]) => {
    client.send(JSON.stringify(action));
    const update = await next();
    assert.equal(state.count, expectedCount);
    assert.equal(update.updateComponents.components.find(component => component.id === "count").text, `Count: ${expectedCount}`);
    assert.equal(client.readyState, WebSocket.OPEN);
  };

  for (const [index, report] of fixture.reports.entries()) {
    client.send(JSON.stringify(report));
    await invoke(index + 1);
  }
  for (const [index, action] of actions.actions.entries()) {
    await invoke(fixture.reports.length + index + 1, action);
  }
  for (const message of [...fixture.invalid, ...actions.invalid]) {
    const invalid = new WebSocket(transport.url);
    clients.push(invalid);
    await once(invalid, "open");
    const closed = once(invalid, "close");
    invalid.send(JSON.stringify(message));
    assert.equal((await closed)[0], 1007, JSON.stringify(message));
  }
  await invoke(fixture.reports.length + actions.actions.length + 1);
} finally {
  for (const client of clients) client.terminate();
  await transport.close();
  await running;
}
console.log("client errors handled");
