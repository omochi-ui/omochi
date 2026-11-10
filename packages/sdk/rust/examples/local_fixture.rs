//! Local publication lifecycle fixture, also demonstrating OS-independent endpoints.
use omochi_ui::{LocalServerOptions, LocalServerTransport};
use tokio::io::{AsyncBufReadExt, BufReader};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = BufReader::new(tokio::io::stdin()).lines();
    println!("waiting");
    if input.next_line().await?.as_deref() != Some("start") {
        return Ok(());
    }
    let root = std::env::args_os().nth(1).expect("runtime directory");
    let mut transport = LocalServerTransport::bind(
        LocalServerOptions::new("shared").with_runtime_dir(std::path::PathBuf::from(root)),
    )
    .await?;
    println!("ready");
    while let Some(line) = input.next_line().await? {
        if line == "close" {
            transport.close().await;
            println!("closed");
            break;
        }
    }
    transport.close().await;
    Ok(())
}
