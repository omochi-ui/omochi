use crate::{
    output::{Decoder, OutputBuffer},
    surface::TextSurface,
};
use anyhow::Result;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::sync::CancellationToken;

pub async fn run(
    mut input: impl AsyncRead + Unpin,
    mut surface: TextSurface,
    stop: CancellationToken,
) -> Result<()> {
    let mut output = OutputBuffer::default();
    let mut decoder = Decoder::default();
    let mut buffer = [0u8; 8192];
    let mut dirty = false;
    let mut closed = false;
    let mut status = "Reading input…".to_owned();
    surface.render(&status, &output).await?;
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    loop {
        tokio::select! {
            _ = stop.cancelled() => return Ok(()),
            _ = interval.tick(), if dirty => { surface.render(&status,&output).await?; dirty = false; },
            read = input.read(&mut buffer), if !closed => {
                match read {
                    Ok(0) => { output.append(&decoder.finish()); status = "Input closed".into(); closed = true; },
                    Ok(n) => output.append(&decoder.write(&buffer[..n])),
                    Err(error) => { status = format!("Input error: {error}"); closed = true; },
                }
                dirty = true;
            }
        }
    }
}
