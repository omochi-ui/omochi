---
title: "Use the SDKs"
description: "Use the SDKs · omochi"
sidebar:
  order: 1
---

Build an app that receives a slider value in Python, TypeScript, or Rust. On Linux or macOS, [start the Hub](../../start/installation/) first:

```sh
npx @omochi-ui/cli hub
```

Choose a language and create the following file in your own project. Each app publishes a Unix socket that the Hub discovers.

## Python

Use Python 3.11 or newer.

```sh
python3 -m venv .venv
. .venv/bin/activate
pip install omochi-ui
```

Save this as `app.py`.

```python
import asyncio
from dataclasses import dataclass

from omochi_ui import ActionContext, App, UnixSocketServerTransport, ui


@dataclass
class State:
    temperature: float = 50
    result: str | None = None

@dataclass
class RunInput:
    temperature: float

app = App(State())


@app.action
async def run(input: RunInput, context: ActionContext[State]):
    async with context.update() as state:
        state.temperature = input.temperature
        state.result = f"Ran at temperature {input.temperature:g}"


def main(state: State) -> None:
    form = ui.local("form", RunInput(state.temperature))
    ui.text("Hello", key="title", variant="h2")
    ui.slider("temperature", 0, 100, bind=form.temperature)
    ui.button("Run", on_click=run, input=form, variant="primary")
    if state.result:
        ui.text(state.result, key="result")


app.mount(main)


asyncio.run(app.start(UnixSocketServerTransport("temperature-python", name="Temperature")))
```

```sh
python app.py
```

## TypeScript

With Node.js 22+, create a project in a new directory.

```sh
npm init -y
npm install @omochi-ui/sdk
npm install --save-dev tsx typescript
```

Save this as `app.mts`.

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

await app.start(new UnixSocketServerTransport({
  serviceId: "temperature-typescript",
  name: "Temperature",
}));
```

```sh
npx tsx app.mts
```

## Rust

Use Rust 1.85 or newer.

```sh
cargo new temperature-ui
cd temperature-ui
cargo add omochi-ui
cargo add serde --features derive
cargo add serde_json
cargo add tokio --features macros,rt-multi-thread
```

Replace `src/main.rs` with the following.

```rust
use omochi_ui::{ActionInput, Button, ButtonVariant, Input, Slider, Text, TextVariant};
use omochi_ui::{App, Result, UnixSocketServerOptions, UnixSocketServerTransport};
use serde::{Deserialize, Serialize};

struct State {
    temperature: f64,
    result: Option<String>,
}
#[derive(Deserialize, Serialize, ActionInput)]
struct RunInput {
    temperature: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            temperature: 50.0,
            result: None,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = App::new(State::default());

    let run = app.action::<RunInput, _, _>("run", |input, context| async move {
        context
            .update(|state| {
                state.temperature = input.temperature;
                state.result = Some(format!("Ran at temperature {}", input.temperature));
            })
            .await?;
        Ok(())
    })?;

    app.surface("main", move |ui, state| {
        let form = ui.local(
            "form",
            RunInput {
                temperature: state.temperature,
            },
        )?;
        ui.add(Text::new("Hello").key("title").variant(TextVariant::H2))?;
        ui.add(
            Slider::new("temperature", 0.0, 100.0)
                .bind(form.field(RunInput::field_temperature()).bind()?),
        )?;
        ui.add(
            Button::new("Run")
                .on_press(run.call(RunInputInput {
                    temperature: Input::expression(
                        form.field(RunInput::field_temperature()).value(),
                    ),
                })?)
                .variant(ButtonVariant::Primary),
        )?;
        if let Some(result) = &state.result {
            ui.add(Text::new(result.clone()).key("result"))?;
        }
        Ok(())
    })?;

    let transport = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("temperature-rust").with_name("Temperature"),
    ).await?;
    app.start_with(transport).await
}
```

```sh
cargo run
```

## Use the app in the Hub

Open **http://127.0.0.1:5555/**, create a dashboard, and add `Temperature`'s `main` Surface through **Add view**. Move the slider and press **Run**. The app receives the value and displays the result.

Run the app and Hub as the same OS user with the same runtime directory. Keep both running while using the view; stop them with Ctrl+C. Service IDs must be unique among running apps and stable if you want to reuse saved layouts.

## Extend the app

- State, Actions, and Surfaces: [Runtime concepts](../../concepts/runtime/)
- Draft values and resets: [Browser-owned input](../../concepts/inputs/)
- Validation, reusable forms, and Serde: [Typed inputs](../../reference/typed-inputs/)
- Connection choices and shutdown: [Transports](../../reference/transports/)
- Custom components: [Components & catalogs](../../reference/components/)
- Images, audio, and video: [Media](../media/)

## Transports

These examples publish Unix sockets. See [Transports](../../reference/transports/) for stdio, WebSocket, custom connections, and shutdown.
