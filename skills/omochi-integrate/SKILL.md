---
name: omochi-integrate
description: Add an omochi-ui browser interface to an existing Python, TypeScript, or Rust program, including typed actions, editable controls, progress, and local dashboard connections. Use when the user asks to integrate omochi or attach an omochi UI to a script, CLI, or service.
---

# Integrate omochi

Attach a declarative UI to the host program. Keep business logic and committed
state in its existing language and process; omochi sends A2UI updates to a shared
browser renderer. Each surface is an independently placeable view of the app.

## Find the integration seam

Inspect the target program's entry point, dependencies, long-running work, and
existing lifecycle. Connect actions to its existing functions and expose the
state users need to inspect or change. Preserve CLI behavior when adding a UI
entry point or mode. A shell command button or log viewer alone may be served by
[`omochi-utils`](https://omochi.fyi/en/docs/reference/utils/).

Read only the relevant language reference for imports and a complete form/action
example: [Python](references/python.md), [TypeScript](references/typescript.md),
or [Rust](references/rust.md). Before selecting dependencies and a browser
connection, read [setup and transports](references/setup.md).

The installed skill is self-contained; its parent directory is not an omochi
checkout. Check the target project's installed SDK version and API first. The
examples describe the source API at the time this skill was updated. For version
differences or additional controls, consult the
[SDK guide](https://omochi.fyi/en/docs/guides/sdks/) and
[component reference](https://omochi.fyi/en/docs/reference/components/), or the
matching SDK source. Keep the project's package manager and dependency strategy.

## Model interaction

- Render functions declare UI from state. They run again after updates; perform
  business operations in actions or managed background work, outside rendering.
- Use `ui.local()` for browser-owned unfinished form input. Bind controls to that
  local and pass its current value as typed action input. The initial value is
  applied once and survives rerenders; resetting requires `resetKey` (TS),
  `reset_key` (Python), or `local_with` (Rust). Updating application state alone
  does not reset the draft. Each connected browser owns its draft.
- Register actions with their input types/schema. Actions receive concrete
  values resolved by the renderer; they do not read bindings or browser state.
  Validate domain constraints before calling the existing operation.
- Inside actions, await `context.update()` to mutate committed state and publish
  changes. For long work, publish a running state, do the operation outside the
  update, then publish success or an actionable error. Keep blocking work off the
  async event loop using the host program's threading/process conventions.
- Actions are serial. Progress updates do not allow another action to run during
  the current one. If interactive cancellation or concurrent work is required,
  use managed background work with `runtime.update()` and explicit shutdown.
  Never await `runtime.update()` from inside an active action: it queues behind
  that action. Updates already published are not rolled back on action failure.
- Give conditional and reorderable components stable explicit keys. Use the
  bundled typed catalog components; custom catalog bindings also need a
  compatible renderer implementation.
- For local images/audio/video, register content with `app.resources` and render
  its returned URI. See the [media guide](https://omochi.fyi/en/docs/guides/media/)
  when needed; a browser on another machine cannot read a host filesystem path.

## Verify the result

Run the target project's relevant checks. Verify the initial surface and one
representative action using `app.build()` / Runtime or the existing test harness;
check that concrete input reaches the original operation and updates the UI.
For long work, check progress and failure states. When browser access is
available, verify editing a draft and submitting it through the chosen transport.

Deliver the integration with exact launch commands, browser URL, and shutdown
instructions. Distinguish what was exercised from any unverified browser or
external-service behavior.
