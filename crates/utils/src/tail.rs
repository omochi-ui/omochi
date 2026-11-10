use crate::{
    output::{Decoder, OUTPUT_LIMIT, OutputBuffer},
    surface::TextSurface,
};
use anyhow::{Result, bail};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct Tail {
    pub output: OutputBuffer,
    decoder: Decoder,
    identity: Option<(u64, u64)>,
    position: u64,
    seen: bool,
}
impl Tail {
    pub async fn poll(&mut self, path: &Path, lines: usize) -> String {
        match self.read(path, lines).await {
            Ok(()) => format!(
                "Following {} · {}",
                path.display(),
                chrono::Utc::now().to_rfc3339()
            ),
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                if self.identity.is_some() {
                    self.output
                        .append("\n--- File missing; waiting for replacement ---\n");
                }
                self.identity = None;
                self.position = 0;
                self.decoder = Decoder::default();
                format!("Waiting for {}…", path.display())
            }
            Err(error) => format!("Tail error: {error}"),
        }
    }
    async fn read(&mut self, path: &Path, lines: usize) -> Result<()> {
        // Metadata and bytes come from the same descriptor, avoiding rotation races.
        let path = path.to_path_buf();
        let (file, info, identity) = tokio::task::spawn_blocking(move || -> Result<_> {
            let mut options = std::fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NONBLOCK);
            }
            let file = options.open(path)?;
            let info = file.metadata()?;
            if !info.is_file() {
                bail!("path is not a regular file");
            }
            let identity = omochi_local::files::handle_identity(&file)?;
            Ok((file, info, identity))
        })
        .await??;
        let mut file = tokio::fs::File::from_std(file);
        let size = info.len();
        let initial = self.identity != Some(identity) || size < self.position;
        if initial {
            if self.seen {
                self.output.append(&format!(
                    "\n--- File {} at {} ---\n",
                    if self.identity != Some(identity) {
                        "replaced"
                    } else {
                        "truncated"
                    },
                    chrono::Utc::now().to_rfc3339()
                ));
            }
            self.decoder = Decoder::default();
        }
        let requested = if initial { 0 } else { self.position };
        let start = requested.max(size.saturating_sub(OUTPUT_LIMIT as u64));
        if !initial && start != requested {
            self.output
                .append("\n--- Earlier appended data skipped ---\n");
            self.decoder = Decoder::default();
        }
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let mut bytes = Vec::new();
        file.take(size.saturating_sub(start))
            .read_to_end(&mut bytes)
            .await?;
        let mut offset = 0;
        if start > requested {
            while offset < bytes.len() && bytes[offset] & 0xc0 == 0x80 {
                offset += 1;
            }
        }
        let decoded = self.decoder.write(&bytes[offset..]);
        if initial {
            let split: Vec<_> = decoded.split('\n').collect();
            let selected = split[split
                .len()
                .saturating_sub(lines.saturating_add(usize::from(decoded.ends_with('\n'))))..]
                .join("\n");
            if self.seen {
                self.output.append(&selected);
            } else {
                self.output.reset(&selected);
            }
        } else {
            self.output.append(&decoded);
        }
        self.position = start + bytes.len() as u64;
        self.identity = Some(identity);
        self.seen = true;
        Ok(())
    }
}
pub async fn run(
    path: PathBuf,
    lines: usize,
    mut surface: TextSurface,
    stop: CancellationToken,
) -> Result<()> {
    let mut tail = Tail::default();
    surface.render("Waiting for file…", &tail.output).await?;
    let mut poll = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! { _ = stop.cancelled() => return Ok(()), _ = poll.tick() => {} }
        let status = tail.poll(&path, lines).await;
        surface.render(&status, &tail.output).await?;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn creation_appends_rotation_and_truncation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("log");
        let mut tail = Tail::default();
        assert!(tail.poll(&path, 2).await.contains("Waiting"));
        tokio::fs::write(&path, b"one\ntwo\nthree\n").await.unwrap();
        tail.poll(&path, 2).await;
        assert_eq!(tail.output.text, "two\nthree\n");
        tokio::fs::write(&path, b"one\ntwo\nthree\nfour\n")
            .await
            .unwrap();
        tail.poll(&path, 2).await;
        assert!(tail.output.text.ends_with("four\n"));
        tokio::fs::rename(&path, root.path().join("old"))
            .await
            .unwrap();
        tokio::fs::write(&path, b"rotated\n").await.unwrap();
        tail.poll(&path, 2).await;
        assert!(tail.output.text.contains("File replaced"));
        tokio::fs::write(&path, b"short\n").await.unwrap();
        tail.poll(&path, 2).await;
        assert!(tail.output.text.contains("File truncated"));
    }
}
