//! Transport-independent framing for the new gateway protocol.
//!
//! SDK JSONL and browser WebSocket messages remain separate contracts.
pub mod data_path;
pub mod fanout;
pub mod queue;
pub mod session;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const VERSION: u32 = 4;
pub const ALPN: &[u8] = b"omochi/gateway/4";
pub const PAIR_ALPN: &[u8] = b"omochi/pair/1";
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// The first frame on every bidirectional stream establishes its purpose.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum OpenStream {
    Control { protocol: u32 },
    Service { service: String, generation: u64 },
    Resource { service: String, generation: u64 },
}

/// EOF is clean only between frames; a truncated header or body is an error.
/// Use one reader task per stream. Dropping this future mid-frame is not safe
/// unless the entire stream is discarded.
pub async fn read_frame<R: AsyncRead + Unpin, T: DeserializeOwned>(
    reader: &mut R,
) -> io::Result<Option<T>> {
    read_frame_limited(reader, MAX_FRAME_BYTES).await
}

/// Apply a smaller cap to unauthenticated application messages (for example enrollment).
pub async fn read_frame_limited<R: AsyncRead + Unpin, T: DeserializeOwned>(
    reader: &mut R,
    limit: usize,
) -> io::Result<Option<T>> {
    let mut header = [0u8; 4];
    if reader.read(&mut header[..1]).await? == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut header[1..]).await?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > limit.min(MAX_FRAME_BYTES) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid frame length",
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub async fn write_frame<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    value: &T,
) -> io::Result<()> {
    let body = serde_json::to_vec(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if body.is_empty() || body.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    writer.write_all(&(body.len() as u32).to_be_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[tokio::test]
    async fn fragmented_frames_preserve_boundaries() {
        let (mut tx, mut rx) = tokio::io::duplex(3);
        let task = tokio::spawn(async move {
            write_frame(&mut tx, &json!({"hello": "世界"}))
                .await
                .unwrap();
            write_frame(&mut tx, &json!([1, 2])).await.unwrap();
        });
        assert_eq!(
            read_frame::<_, Value>(&mut rx).await.unwrap(),
            Some(json!({"hello": "世界"}))
        );
        assert_eq!(
            read_frame::<_, Value>(&mut rx).await.unwrap(),
            Some(json!([1, 2]))
        );
        assert_eq!(read_frame::<_, Value>(&mut rx).await.unwrap(), None);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_truncation_and_oversized_headers_before_allocation() {
        for bytes in [vec![0], vec![0, 0, 0, 2, b'{'], vec![255; 4], vec![0; 4]] {
            assert!(read_frame::<_, Value>(&mut bytes.as_slice()).await.is_err());
        }
    }

    #[tokio::test]
    async fn rejects_invalid_json_and_utf8() {
        for bytes in [vec![0, 0, 0, 1, b'{'], vec![0, 0, 0, 1, 255]] {
            assert!(read_frame::<_, Value>(&mut bytes.as_slice()).await.is_err());
        }
    }
}
