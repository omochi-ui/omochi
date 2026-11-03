---
title: "SDKを使う"
description: "SDKを使う · omochi"
sidebar:
  order: 1
---

3つのSDKは共通のプロトコルとランタイムの概念を使います。次の例はビルド済みチェックアウトのローカルパッケージを使います。`app.start()`は標準入出力を使うため、[開発プレビュー](../../start/first-ui/)経由で表示してください。

## Python

```python
import asyncio
from dataclasses import dataclass

from omochi_ui import ActionContext, App, ui


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


asyncio.run(app.start())
```

## TypeScript

```ts
import { schema, App } from "@omochi-ui/sdk";

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

await app.start();
```

## Rust

```rust
use omochi_ui::{ActionInput, Button, ButtonVariant, Input, Slider, Text, TextVariant};
use omochi_ui::{App, Result};
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

    app.start().await
}
```

## パッケージ名

TypeScriptは`@omochi-ui/sdk`、Node専用Transportは`@omochi-ui/sdk/node`から読み込みます。Pythonは`omochi_ui`、Rustはcrate名`omochi-ui`を`omochi_ui`として使います。

依存関係は同梱exampleのマニフェストを参照してください。TypeScriptは事前ビルド、Pythonは3.11以上、RustはCargoとTokioが必要です。公開エントリーポイントは[SDKリファレンス](../../reference/sdk/)にまとめています。

型付き入力と移行手順の詳細は[英語版SDKガイド](../../../en/docs/guides/sdks/#typed-inputs-and-migration)を参照してください。
