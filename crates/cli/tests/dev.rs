use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
const CHILD: &str = r#"
import json, sys, base64, threading
def send(v): print(json.dumps(v),flush=True)
send({"omochi":"resources/1","kind":"hello"})
send({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}})
send({"version":"v0.9","updateDataModel":{"surfaceId":"main","value":{"count":1}}})
for line in sys.stdin:
    message=json.loads(line)
    if message.get("omochi")=="resources/1":
        result={"omochi":"resources/1","kind":"response","id":message["id"],"status":200}
        if message["op"]=="stat": result.update(size=5,mime="image/png",etag='"'+'a'*32+'"')
        else: result["data"]=base64.b64encode(b"hello"[message["offset"]:message["offset"]+message["length"]]).decode()
        send(result)
    else:
        action=message["action"]
        if action["name"]=="exit": sys.exit(7)
        if action["name"]=="exit0": sys.exit(0)
        if action["name"]=="bad":
            print("not json",flush=True)
            threading.Event().wait()
        send({"version":"v0.9","updateDataModel":{"surfaceId":action["surfaceId"],"path":"/echo","value":message}})
"#;
#[tokio::test]
async fn native_dev_replays_unprefixed_stdio_routes_resources_and_propagates_child_failure() {
    for (ending, exit_code, close_code) in [("exit", 7, 1011), ("bad", 1, 1002), ("exit0", 0, 1000)]
    {
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("app.py");
        std::fs::write(&script, CHILD).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_omochi-ui"))
            .arg("--config-dir")
            .arg(root.path().join("config"))
            .args(["dev", "--port", "0", "--", "/usr/bin/python3"])
            .arg(script)
            .env("PATH", "/nonexistent")
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut logs = BufReader::new(child.stderr.take().unwrap()).lines();
        let line = tokio::time::timeout(Duration::from_secs(5), logs.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let url = line.strip_prefix("[omochi-ui] bridge: ").unwrap();
        let (mut ws, _) = connect_async(url).await.unwrap();
        let first: Value =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(first["createSurface"]["surfaceId"], "main");
        let second: Value =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(second["updateDataModel"]["value"]["count"], 1);
        let action = json!({"version":"v0.9","action":{"surfaceId":"main","name":"echo","context":{"answer":"yes"}},"metadata":{"a2uiClientDataModel":{"surfaces":{"main":{"draft":"untouched"}}}}});
        ws.send(Message::Text(action.to_string().into()))
            .await
            .unwrap();
        let echo: Value =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(echo["updateDataModel"]["value"], action);
        let (mut late, _) = connect_async(url).await.unwrap();
        let first: Value =
            serde_json::from_str(late.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(first["createSurface"]["surfaceId"], "main");
        let snapshot: Value =
            serde_json::from_str(late.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(snapshot["updateDataModel"]["value"]["echo"], action);
        late.close(None).await.unwrap();
        drop(late);
        for (message, expected) in [
            (Message::Binary(vec![1].into()), 1003),
            (Message::Text("not json".into()), 1002),
        ] {
            let (mut invalid, _) = connect_async(url).await.unwrap();
            invalid.send(message).await.unwrap();
            let close = tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if let Message::Close(Some(frame)) = invalid.next().await.unwrap().unwrap() {
                        break frame.code;
                    }
                }
            })
            .await
            .unwrap();
            assert_eq!(u16::from(close), expected);
        }
        let address = url.trim_start_matches("ws://").trim_end_matches('/');
        let mut http = tokio::net::TcpStream::connect(address).await.unwrap();
        http.write_all(format!("GET /resources/_/{} HTTP/1.1\r\nHost: localhost\r\nRange: bytes=1-3\r\nConnection: close\r\n\r\n","a".repeat(32)).as_bytes()).await.unwrap();
        let mut response = String::new();
        http.read_to_string(&mut response).await.unwrap();
        assert!(response.starts_with("HTTP/1.1 206"));
        assert!(response.contains("ell"));
        ws.send(Message::Text(
            json!({"version":"v0.9","action":{"surfaceId":"main","name":ending,"context":{}}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let close = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Message::Close(Some(frame)) = ws.next().await.unwrap().unwrap() {
                    break frame.code;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(u16::from(close), close_code);
        let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.code(), Some(exit_code));
    }
}
