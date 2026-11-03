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
