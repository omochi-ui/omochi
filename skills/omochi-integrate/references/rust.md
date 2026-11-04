# Rust integration

Use the `omochi-ui` crate with the dependencies in [setup](setup.md), including
`serde_json` for the derive macro. `ActionInput` generates typed
field accessors and the companion `RunInputInput` expression type used below;
that type is generated, not another struct to implement manually.

This standalone entry point supports both the stdio preview and a local hub.
Replace the result calculation in the action with the program's existing
operation. Await slow work outside `context.update()`, and preserve the host's
Tokio lifecycle if it already has one.

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

    // Publishes on a discoverable unix socket when OMOCHI_SERVICE names a service id.
    // Without it the app speaks JSONL on stdio, which is what `omochi-ui dev` expects.
    match std::env::var("OMOCHI_SERVICE") {
        Ok(service_id) if !service_id.is_empty() => {
            let transport = UnixSocketServerTransport::bind(
                UnixSocketServerOptions::new(service_id)
                    .with_name("Rust example")
                    .with_icon("\u{1F980}"),
            )
            .await?;
            app.start_with(transport).await
        }
        _ => app.start().await,
    }
}
```

For the local hub, run `OMOCHI_SERVICE=my-tool cargo run` from the target crate.
Without the variable, launch the binary or `cargo run` under `omochi-ui dev --`
as described in [setup](setup.md). Rust selects an explicit transport using
`app.start_with(transport)`.

Use `eprintln!()` or stderr logging when using stdio.

API and further controls: [SDK guide](https://omochi.fyi/en/docs/guides/sdks/).
