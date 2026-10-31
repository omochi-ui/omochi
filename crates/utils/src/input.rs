//! Cancellable stdin for pipes/terminals: Tokio's blocking stdin cannot be interrupted.
use std::{
    io::{self, Read},
    os::fd::{AsRawFd, FromRawFd},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    fs::File,
    io::{AsyncRead, ReadBuf, unix::AsyncFd},
};
pub enum Input {
    Regular(File),
    Pollable {
        file: AsyncFd<std::fs::File>,
        flags: i32,
    },
}
impl Input {
    pub fn stdin() -> io::Result<Self> {
        let fd = unsafe { libc::dup(libc::STDIN_FILENO) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        let info = file.metadata()?;
        if info.is_file() {
            return Ok(Self::Regular(File::from_std(file)));
        }
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: File owns the descriptor and is retained for the AsyncFd lifetime.
        match unsafe { AsyncFd::register(file) } {
            Ok(file) => Ok(Self::Pollable { file, flags }),
            Err(error) => {
                let (file, error) = error.into_parts();
                unsafe {
                    libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags);
                }
                if error.raw_os_error() == Some(libc::EPERM) {
                    Ok(Self::Regular(File::from_std(file)))
                } else {
                    Err(error)
                }
            }
        }
    }
}
impl AsyncRead for Input {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Regular(file) => Pin::new(file).poll_read(cx, buffer),
            Self::Pollable { file, .. } => loop {
                let mut ready = std::task::ready!(file.poll_read_ready(cx))?;
                match ready.try_io(|file| file.get_ref().read(buffer.initialize_unfilled())) {
                    Ok(Ok(count)) => {
                        buffer.advance(count);
                        return Poll::Ready(Ok(()));
                    }
                    Ok(Err(error)) => return Poll::Ready(Err(error)),
                    Err(_) => continue,
                }
            },
        }
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        if let Self::Pollable { file, flags } = self {
            unsafe {
                libc::fcntl(file.get_ref().as_raw_fd(), libc::F_SETFL, *flags);
            }
        }
    }
}
