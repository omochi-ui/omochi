---
title: "SDKを使う"
description: "SDKを使う · omochi"
sidebar:
  order: 1
---

Python・TypeScript・Rustのいずれかで、スライダーの値を受け取るアプリを作ります。LinuxまたはmacOSで、[Hubを起動](../../start/installation/)しておいてください。

```sh
npx @omochi-ui/cli hub
```

言語をひとつ選び、以下のファイルを自分のプロジェクトに作成します。アプリはUnixソケットで公開され、Hubに検出されます。

## Python

Python 3.11以上を使います。

```sh
python3 -m venv .venv
. .venv/bin/activate
pip install omochi-ui
```

次を`app.py`に保存します。

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

Node.js 22以上で、新しいディレクトリにプロジェクトを作ります。

```sh
npm init -y
npm install @omochi-ui/sdk
npm install --save-dev tsx typescript
```

次を`app.mts`に保存します。

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

Rust 1.85以上を使います。

```sh
cargo new temperature-ui
cd temperature-ui
cargo add omochi-ui
cargo add serde --features derive
cargo add serde_json
cargo add tokio --features macros,rt-multi-thread
```

`src/main.rs`を次に置き換えます。

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

## Hubで操作する

**http://127.0.0.1:5555/** でダッシュボードを作成し、**Add view**から`Temperature`の`main` Surfaceを追加します。スライダーを動かして **Run** を押すと、アプリが値を受け取り結果を表示します。

アプリとHubは同じOSユーザー・実行時ディレクトリで起動してください。利用中は両方を動かしておき、終了時はCtrl+Cを押します。サービスIDは同時に動くアプリ間で一意にし、保存した配置を再利用する場合は固定してください。

## アプリを広げる

- 状態・Action・Surfaceの関係：[ランタイムの基本](../../concepts/runtime/)
- 入力途中の値とリセット：[ブラウザーが持つ入力](../../concepts/inputs/)
- 入力の検証・再利用可能なフォーム・Serde：[型付き入力](../../reference/typed-inputs/)
- 接続方法と終了処理：[Transport](../../reference/transports/)
- 独自の部品を使う：[コンポーネントとカタログ](../../reference/components/)
- 画像・音声・動画を使う：[メディア](../media/)

## Transport

これらの例はUnixソケットで公開します。標準入出力・WebSocket・独自接続の選択と終了処理は[Transportリファレンス](../../reference/transports/)を参照してください。
