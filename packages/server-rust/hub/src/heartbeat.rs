//! A stalled writer or a missing matching pong ends the entire gateway session.
use anyhow::{Context, Result};
use omochi_protocol::{session::Control, write_frame};
use std::time::Duration;
use tokio::{io::AsyncWrite, sync::watch};

pub async fn run(
    mut writer: impl AsyncWrite + Unpin,
    mut replies: watch::Receiver<u64>,
    interval: Duration,
) -> Result<()> {
    let mut id = 0u64;
    loop {
        tokio::time::sleep(interval).await;
        id = id.wrapping_add(1);
        tokio::time::timeout(interval, async {
            write_frame(&mut writer, &Control::Ping { id }).await?;
            loop {
                replies.changed().await?;
                if *replies.borrow_and_update() == id {
                    return Ok::<_, anyhow::Error>(());
                }
            }
        })
        .await
        .context("gateway heartbeat timed out")??;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omochi_protocol::read_frame;

    #[tokio::test]
    async fn matching_pongs_keep_session_alive_but_stale_pongs_do_not() {
        let (writer, mut reader) = tokio::io::duplex(1024);
        let (pong, replies) = watch::channel(0);
        let task = tokio::spawn(run(writer, replies, Duration::from_millis(50)));
        for expected in 1..=3 {
            let message = tokio::time::timeout(
                Duration::from_secs(2),
                read_frame::<_, Control>(&mut reader),
            )
            .await
            .unwrap()
            .unwrap()
            .unwrap();
            assert!(matches!(message, Control::Ping { id } if id == expected));
            pong.send_replace(if expected == 3 { 2 } else { expected });
        }
        let error = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("heartbeat timed out"));
    }
}
