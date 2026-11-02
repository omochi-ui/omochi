---
title: "Browser-owned input"
description: "Browser-owned input · omochi"
sidebar:
  order: 2
---

## Drafts stay in the renderer

Declare `ui.local("form", initialValue)` inside the surface and bind editable controls to that Local. `form.bind("/temperature")` connects an input to a JSON pointer. `form.value("/temperature")` builds an expression for an action payload.

When the user invokes the action, the renderer resolves expressions into concrete JSON. The application runtime never stores the unfinished draft.

## Rerenders and resets

A Local's initial value is applied once. Server rerenders preserve an existing draft. Use `resetKey` (TypeScript), `reset_key` (Python), or `local_with` (Rust) when you explicitly need to reset it. Updating application state alone is not a request to overwrite a draft.

Use stable component keys for controls that appear conditionally. See the [SDK examples](../../guides/sdks/) for a complete form.
