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

## パッケージ名

TypeScriptは`@omochi-ui/sdk`、Node専用Transportは`@omochi-ui/sdk/node`から読み込みます。Pythonは`omochi_ui`、Rustはcrate名`omochi-ui`を`omochi_ui`として使います。

依存関係は同梱exampleのマニフェストを参照してください。TypeScriptは事前ビルド、Pythonは3.11以上、RustはCargoとTokioが必要です。公開エントリーポイントは[SDKリファレンス](../../reference/sdk/)にまとめています。
