//! MCP uses the official Rust SDK for negotiation, framing and request cancellation.
use crate::{input::Input, publisher::Publisher};
use anyhow::{Context, Result, bail};
use rmcp::{
    ServerHandler, ServiceExt,
    model::*,
    service::{RequestContext, RoleServer},
    transport::async_rw::AsyncRwTransport,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Server {
    publisher: Publisher,
    slots: std::sync::Arc<tokio::sync::Semaphore>,
}
fn tools() -> Vec<Tool> {
    [
        json!({"name":"publish_messages","description":"Publish an ordered, prevalidated batch of A2UI v0.9 createSurface/updateComponents/updateDataModel/deleteSurface messages.","inputSchema":{"type":"object","properties":{"messages":{"type":"array","minItems":1,"items":{"type":"object"}}},"required":["messages"]},"annotations":{"readOnlyHint":false,"destructiveHint":true,"openWorldHint":false}}),
        json!({"name":"list_surfaces","description":"List this process's service ID and currently published surface IDs.","inputSchema":{"type":"object","properties":{}},"annotations":{"readOnlyHint":true,"openWorldHint":false}}),
        json!({"name":"read_events","description":"Read actions without consuming them. Start after=0, then use nextCursor. Latest 1000 events retained, with a 16 MiB total limit; dropped reports missing events. timeoutMs waits up to 30000ms.","inputSchema":{"type":"object","properties":{"after":{"type":"integer","minimum":0,"maximum":9007199254740991u64},"timeoutMs":{"type":"integer","minimum":0,"maximum":30000}}},"annotations":{"readOnlyHint":true,"openWorldHint":false}}),
    ].into_iter().map(|value| serde_json::from_value(value).expect("static tool definition")).collect()
}
impl Server {
    async fn invoke(&self, name: &str, args: Value, cancel: &CancellationToken) -> Result<Value> {
        match name {
            "publish_messages" => {
                let messages = args["messages"]
                    .as_array()
                    .context("messages must be an array")?;
                if messages.iter().any(|v| !v.is_object()) {
                    bail!("messages must contain objects");
                }
                self.publisher.publish(messages.clone()).await
            }
            "list_surfaces" => self.publisher.list().await,
            "read_events" => {
                let number = |name: &str, maximum| -> Result<u64> {
                    match args.get(name) {
                        None => Ok(0),
                        Some(value) => {
                            value.as_u64().filter(|n| *n <= maximum).with_context(|| {
                                format!("{name} must be an integer between 0 and {maximum}")
                            })
                        }
                    }
                };
                self.publisher
                    .events
                    .read(
                        number("after", 9007199254740991)?,
                        number("timeoutMs", 30000)?,
                        cancel,
                    )
                    .await
            }
            _ => bail!("unknown tool: {name}"),
        }
    }
}
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        let example = json!({"messages":[
            {"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":omochi_ui::STANDARD_CATALOG,"sendDataModel":true}},
            {"version":"v0.9","updateComponents":{"surfaceId":"main","components":[
                {"id":"root","component":"Column","children":["title","submit"]},
                {"id":"title","component":"Text","text":"What should we build?"},
                {"id":"submit","component":"Button","child":"label","action":{"event":{"name":"submit","context":{}}}},
                {"id":"label","component":"Text","text":"Send"}
            ]}}
        ]});
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("omochi-utils",env!("CARGO_PKG_VERSION")))
            .with_instructions(format!("Publish A2UI v0.9 surfaces to the local Omochi gateway. Basic Catalog: {}. Use surface-local IDs (no gateway prefixes). Surfaces live only while this process runs. Call read_events to receive button/form actions, then publish_messages to respond; events do not execute code. Example publish_messages arguments: {example}",omochi_ui::STANDARD_CATALOG))
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: tools(),
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        if !tools().iter().any(|t| t.name == request.name) {
            return Err(ErrorData::invalid_params("unknown tool", None));
        }
        let Ok(_permit) = self.slots.try_acquire() else {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "too many concurrent tool calls",
            )])
            .into());
        };
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let result = match self.invoke(&request.name, arguments, &context.ct).await {
            Ok(value) => {
                let mut result = CallToolResult::structured(value);
                // Preserve the existing optional success marker.
                result.is_error = None;
                result
            }
            Err(error) => CallToolResult::error(vec![ContentBlock::text(format!("{error:#}"))]),
        };
        Ok(result.into())
    }
}
pub async fn run(publisher: Publisher, stop: CancellationToken) -> Result<()> {
    let transport = AsyncRwTransport::<RoleServer, _, _>::new(Input::stdin()?, tokio::io::stdout())
        .with_max_line_length(4 * 1024 * 1024);
    let service = tokio::select! {
        _ = stop.cancelled()=>return Ok(()),
        service = (Server { publisher, slots:std::sync::Arc::new(tokio::sync::Semaphore::new(16)) }).serve(transport)=>service?,
    };
    let cancel = service.cancellation_token();
    let mut waiting = Box::pin(service.waiting());
    tokio::select! {
        _ = stop.cancelled()=>{ cancel.cancel(); waiting.await?; },
        result = &mut waiting=>{ result?; },
    }
    Ok(())
}
