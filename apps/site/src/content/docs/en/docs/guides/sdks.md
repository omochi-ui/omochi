---
title: "Use the SDKs"
description: "Use the SDKs · omochi"
sidebar:
  order: 1
---

The three SDKs share the same protocol and runtime concepts. Examples here use local packages from a built checkout. `app.start()` uses stdin/stdout; connect it using the [development preview](../../start/first-ui/).

## Python

```python
import asyncio

from omochi_ui import App

state = {"temperature": 50, "result": None}
app = App(state)

@app.action("run")
async def run(input, ctx):
    await ctx.update(lambda state: state.update(
        temperature=input["temperature"],
        result=f'Result: {input["temperature"]}',
    ))

@app.surface("main")
def main(ui, state):
    form = ui.local("form", {"temperature": state["temperature"]})
    ui.text("Hello", key="title")
    ui.slider("temperature", 0, 100, bind=form.bind("/temperature"))
    ui.button("Run", run.call({"temperature": form.value("/temperature")}))
    if state["result"]:
        ui.text(state["result"], key="result")

asyncio.run(app.start())  # JSONL over stdin/stdout by default
```

## TypeScript

```ts
import { App } from "@omochi-ui/sdk";

const state = { temperature: 50, result: null as string | null };
const app = new App(state);

const run = app.action<{ temperature: number }>("run", async (input, ctx) => {
  await ctx.update((state) => {
    state.temperature = input.temperature;
    state.result = `Result: ${input.temperature}`;
  });
});

app.surface("main", (ui, state) => {
  const form = ui.local("form", { temperature: state.temperature });
  ui.text("Hello", { key: "title" });
  ui.slider("temperature", 0, 100, { bind: form.bind("/temperature") });
  ui.button("Run", run.call({ temperature: form.value("/temperature") }));
  if (state.result) ui.text(state.result, { key: "result" });
});

await app.start();
```

## Rust

```rust
use omochi_ui::{App, ButtonOptions, Result, SliderOptions};
use serde::Deserialize;

#[derive(Default)]
struct State { result: Option<f64> }
#[derive(Deserialize)]
struct RunInput { temperature: f64 }

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = App::new(State::default());
    let run = app.action::<RunInput, _, _>("run", |input, context| async move {
        context.update(|state| { state.result = Some(input.temperature); }).await?;
        Ok(())
    })?;
    app.surface("main", move |ui, state| {
        let form = ui.local("form", serde_json::json!({ "temperature": 50.0 }))?;
        ui.text("Hello")?;
        ui.slider("temperature", 0.0, 100.0,
            SliderOptions { bind: Some(form.bind("/temperature")?), ..Default::default() })?;
        ui.button("Run", run.call(serde_json::json!({ "temperature": form.value("/temperature")? }))?, ButtonOptions::default())?;
        if let Some(result) = state.result {
            ui.text(result)?;
        }
        Ok(())
    })?;
    app.start().await
}
```

## Package names

TypeScript imports `@omochi-ui/sdk`; Node-only transports are in `@omochi-ui/sdk/node`. Python imports `omochi_ui`. The Rust crate is `omochi-ui`, imported as `omochi_ui`.

Use the bundled example project manifests to resolve language dependencies. Build before running TypeScript examples. Python requires 3.11+, Rust examples require Cargo and Tokio. See [SDK reference](../../reference/sdk/) for public entry points.
