---
title: "Browser-owned input"
description: "Browser-owned input · omochi"
sidebar:
  order: 2
---

## Drafts stay in the renderer

Declare `ui.local("form", initialValue)` inside the surface and select the field
that an editable control should change. For a form with a numeric `temperature`:

- TypeScript: use `form.field("temperature").bind()` for a control and
  `form.field("temperature").value()` in an action payload.
- Python: use a typed initial value, such as a `RunInput` dataclass, and pass
  `form.temperature` as the control's `bind=`. Pass `input=form` to send the whole
  form or `input={"temperature": form.temperature}` to compose the payload.
  Explicit typed projection uses `form.field("temperature", float)` followed by
  `.bind()` or `.value()`.
- Rust: derive `ActionInput` for `RunInput`, then use
  `form.field(RunInput::field_temperature()).bind()?` for a control and
  `form.field(RunInput::field_temperature()).value()` in the generated `RunInputInput`.

`bind()` and `value()` take no pointer argument. Without a field projection,
they refer to the whole Local value. See the [SDK reference](../../reference/sdk/)
for action registration and [SDK examples](../../guides/sdks/) for complete forms.

When the user invokes the action, the renderer resolves expressions into concrete JSON. The application runtime never stores the unfinished draft.

## Rerenders and resets

A Local's initial value is applied once. Server rerenders preserve an existing draft. Use `resetKey` (TypeScript), `reset_key` (Python), or `local_with` (Rust) when you explicitly need to reset it. Updating application state alone is not a request to overwrite a draft.

Use stable component keys for controls that appear conditionally. See the [SDK examples](../../guides/sdks/) for a complete form.
