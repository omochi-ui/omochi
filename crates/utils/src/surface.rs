use crate::{output::OutputBuffer, publisher::Publisher};
use anyhow::Result;
use omochi_ui::{Reconciler, STANDARD_CATALOG, TextOptions, Ui};

pub struct TextSurface {
    reconciler: Reconciler,
    publisher: Publisher,
    name: String,
}
impl TextSurface {
    pub fn new(publisher: Publisher, name: String) -> Self {
        Self {
            reconciler: Reconciler::default(),
            publisher,
            name,
        }
    }
    pub async fn render(&mut self, status: &str, output: &OutputBuffer) -> Result<()> {
        self.render_controls(status, output, &[]).await
    }
    pub async fn render_controls(
        &mut self,
        status: &str,
        output: &OutputBuffer,
        controls: &[(&str, &str)],
    ) -> Result<()> {
        self.render_sections(status, output, controls, &[]).await
    }
    pub async fn render_sections(
        &mut self,
        status: &str,
        output: &OutputBuffer,
        controls: &[(&str, &str)],
        sections: &[(&str, &str, String)],
    ) -> Result<()> {
        let mut ui = Ui::new("main", STANDARD_CATALOG);
        ui.text_with(
            &*self.name,
            TextOptions {
                key: Some("title".into()),
                variant: Some("h3".into()),
            },
        )?;
        let mut actions = omochi_ui::App::new(());
        for (name, label) in controls {
            let action =
                actions.action::<serde_json::Value, _, _>(*name, |_, _| async { Ok(()) })?;
            ui.button(
                *label,
                action.call(serde_json::Value::Null)?,
                omochi_ui::ButtonOptions {
                    key: Some((*name).into()),
                    ..Default::default()
                },
            )?;
        }
        ui.text_with(
            status,
            TextOptions {
                key: Some("status".into()),
                ..Default::default()
            },
        )?;
        ui.text_with(
            output.fenced(),
            TextOptions {
                key: Some("output".into()),
                ..Default::default()
            },
        )?;
        for (key, title, text) in sections {
            ui.text_with(
                *title,
                TextOptions {
                    key: Some(format!("{key}-title")),
                    variant: Some("h4".into()),
                },
            )?;
            ui.text_with(
                text.as_str(),
                TextOptions {
                    key: Some((*key).into()),
                    ..Default::default()
                },
            )?;
        }
        let messages = self.reconciler.render(ui.build())?;
        if !messages.is_empty() {
            self.publisher.publish(messages).await?;
        }
        Ok(())
    }
}
