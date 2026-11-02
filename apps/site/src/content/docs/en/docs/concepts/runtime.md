---
title: "State, actions & surfaces"
description: "State, actions & surfaces · omochi"
sidebar:
  order: 1
---

## Application state

`App` owns your application's state and declarations. `App.build()` creates a transport-independent runtime. Mutate running state through `context.update()` in an action, or `runtime.update()` for independent work.

## Surfaces

A surface is a named view of state. Render functions declare components; the reconciler turns changes into A2UI messages. Every root component has ID `root`. Give conditional or reorderable components explicit keys to preserve identity.

## Actions and progress

Actions receive concrete typed input and an update context, not renderer bindings. Each awaited context update renders every surface and sends the differences before returning. Transport completion does not mean the browser has painted.

Actions are serial. Publishing progress does not permit another action to run concurrently. Do not await `runtime.update()` inside an active action: it queues behind that action. Await context updates before the handler returns. An error does not roll back updates already sent.

## Transport boundary

Starting an app defaults to JSONL over stdin/stdout. The development bridge, direct WebSocket adapter, or discoverable Unix socket connects it to a renderer. A transport carries messages for one application; hub session routing lives outside A2UI messages.
