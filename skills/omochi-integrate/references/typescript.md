# TypeScript integration

Use ESM with the target project's existing Bun/Node tooling. Import the SDK
from `@omochi-ui/sdk`; server-specific transports come from
`@omochi-ui/sdk/node`. `app.action(name, schema, handler)` infers handler input
from the schema. A Local field's `.bind()` connects an editable control;
`.value()` supplies its current value to the action.

This standalone entry point supports both the stdio preview and a local hub.
Replace the result calculation in `run` with the program's existing operation;
await long work outside `context.update()`.

```ts
import { schema, App } from "@omochi-ui/sdk";
import { UnixSocketServerTransport } from "@omochi-ui/sdk/node";

const state: { temperature: number; result: string | null } = {
  temperature: 50,
  result: null,
};

const app = new App(state);

const run = app.action("run", schema.object({ temperature: schema.number() }), async (input, context) => {
  await context.update((current) => { current.temperature = input.temperature; current.result = `Ran at temperature ${input.temperature}`; });
});

app.surface("main", (ui, current) => {
  const form = ui.local("form", { temperature: current.temperature });
  ui.text("Hello", { key: "title", variant: "h2" });
  ui.slider("temperature", 0, 100, { bind: form.field("temperature").bind() });
  ui.button("Run", run.call({ temperature: form.field("temperature").value() }), { variant: "primary" });
  if (current.result) ui.text(current.result, { key: "result" });
});

// Publishes on a discoverable unix socket when OMOCHI_SERVICE names a service id.
// Without it the app speaks JSONL on stdio, which is what `omochi-ui dev` expects.
const serviceId = process.env.OMOCHI_SERVICE;
await app.start(
  serviceId
    ? new UnixSocketServerTransport({ serviceId, name: "TypeScript example", icon: "\u{1F537}" })
    : undefined,
);
```

For the local hub, run `OMOCHI_SERVICE=my-tool bun run ui_app.ts` from a project
with the SDK installed. Without the variable, launch this program under
`omochi-ui dev --` as described in [setup](setup.md). For Node, compile using the
project's TypeScript tooling and run the resulting ESM entry point.

Use `console.error()` for logs when using stdio.

API and further controls: [SDK guide](https://omochi.fyi/en/docs/guides/sdks/).
