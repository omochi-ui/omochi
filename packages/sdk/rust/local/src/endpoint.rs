use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum LocalEndpoint {
    #[serde(rename = "unix")]
    Unix { path: PathBuf },
    #[serde(rename = "namedPipe")]
    NamedPipe { name: String },
}
impl LocalEndpoint {
    pub fn for_socket(path: impl Into<PathBuf>) -> io::Result<Self> {
        #[cfg(unix)]
        {
            Ok(Self::Unix { path: path.into() })
        }
        #[cfg(windows)]
        {
            let _ = path.into();
            Ok(Self::NamedPipe {
                name: format!(r"\\.\pipe\omochi-local-v2-{}", crate::random_name()?),
            })
        }
    }
    pub fn resolve(&self, directory: &Path) -> io::Result<Self> {
        match self {
            Self::Unix { path } => {
                if path
                    .to_str()
                    .is_none_or(|value| value.is_empty() || value.contains(['/', '\\', '\0']))
                    || path.components().count() != 1
                    || !matches!(
                        path.components().next(),
                        Some(std::path::Component::Normal(_))
                    )
                {
                    return Err(crate::invalid("Unix endpoint must be a plain filename"));
                }
                #[cfg(not(unix))]
                {
                    Err(crate::invalid("unsupported local transport"))
                }
                #[cfg(unix)]
                {
                    Ok(Self::Unix {
                        path: directory.join(path),
                    })
                }
            }
            Self::NamedPipe { name } => {
                let suffix = name
                    .strip_prefix(r"\\.\pipe\omochi-local-v2-")
                    .ok_or_else(|| crate::invalid("invalid pipe namespace"))?;
                if suffix.len() != 32
                    || !suffix
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(crate::invalid("invalid pipe name"));
                }
                #[cfg(not(windows))]
                {
                    Err(crate::invalid("unsupported local transport"))
                }
                #[cfg(windows)]
                {
                    let _ = directory;
                    Ok(self.clone())
                }
            }
        }
    }
    pub async fn connect(&self) -> io::Result<LocalStream> {
        match self {
            #[cfg(unix)]
            Self::Unix { path } => Ok(LocalStream::from_io(
                tokio::net::UnixStream::connect(path).await?,
            )),
            #[cfg(windows)]
            Self::NamedPipe { .. } => {
                use std::os::windows::io::AsRawHandle;
                self.resolve(Path::new(""))?;
                let Self::NamedPipe { name } = self else {
                    unreachable!()
                };
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
                loop {
                    match tokio::net::windows::named_pipe::ClientOptions::new().open(name) {
                        Ok(pipe) => {
                            crate::platform::validate_owner(pipe.as_raw_handle(), false)?;
                            return Ok(LocalStream::from_io(pipe));
                        }
                        Err(e)
                            if e.raw_os_error()
                                == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32)
                                && tokio::time::Instant::now() < deadline =>
                        {
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
            _ => Err(crate::invalid("unsupported local transport")),
        }
    }
}
trait Duplex: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Duplex for T {}
pub struct LocalStream {
    inner: Box<dyn Duplex>,
    pub(crate) lifetime: Option<Arc<dyn Send + Sync>>,
}
impl LocalStream {
    pub fn from_io(inner: impl AsyncRead + AsyncWrite + Send + Unpin + 'static) -> Self {
        Self {
            inner: Box::new(inner),
            lifetime: None,
        }
    }
    pub fn into_split(self) -> (tokio::io::ReadHalf<Self>, tokio::io::WriteHalf<Self>) {
        tokio::io::split(self)
    }
}
impl AsyncRead for LocalStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_read(cx, buf)
    }
}
impl AsyncWrite for LocalStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.inner).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_shutdown(cx)
    }
}

pub struct LocalListener {
    #[cfg(unix)]
    inner: Mutex<Option<tokio::net::UnixListener>>,
    #[cfg(windows)]
    inner: Mutex<Option<tokio::net::windows::named_pipe::NamedPipeServer>>,
    endpoint: LocalEndpoint,
    stop: CancellationToken,
    pub(crate) lifetime: Option<Arc<dyn Send + Sync>>,
}
impl LocalListener {
    pub fn bind(endpoint: LocalEndpoint) -> io::Result<Self> {
        #[cfg(unix)]
        let inner = match &endpoint {
            LocalEndpoint::Unix { path } => {
                use std::os::unix::fs::PermissionsExt;
                if path.as_os_str().len() > 100 {
                    return Err(crate::invalid(
                        "Unix socket path exceeds 100 bytes; choose a shorter runtimeDir",
                    ));
                }
                let listener = tokio::net::UnixListener::bind(path)?;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
                listener
            }
            _ => return Err(crate::invalid("unsupported local transport")),
        };
        #[cfg(windows)]
        let inner = Self::pipe(&endpoint, true)?;
        Ok(Self {
            inner: Mutex::new(Some(inner)),
            endpoint,
            stop: CancellationToken::new(),
            lifetime: None,
        })
    }
    #[cfg(windows)]
    fn pipe(
        endpoint: &LocalEndpoint,
        first: bool,
    ) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        let LocalEndpoint::NamedPipe { name } = endpoint else {
            return Err(crate::invalid("unsupported local transport"));
        };
        let security = crate::platform::Security::new(false)?;
        let mut attributes = security.attributes();
        unsafe {
            tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    name,
                    (&mut attributes as *mut windows_sys::Win32::Security::SECURITY_ATTRIBUTES)
                        .cast(),
                )
        }
    }
    pub fn endpoint(&self) -> &LocalEndpoint {
        &self.endpoint
    }
    pub async fn accept(&self) -> io::Result<LocalStream> {
        tokio::select! {
            biased;
            _ = self.stop.cancelled() => Err(io::Error::new(io::ErrorKind::Interrupted, "listener closed")),
            result = async {
                let mut guard = self.inner.lock().await;
                let listener = guard.as_mut().ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "listener closed"))?;
                #[cfg(unix)]
                let mut stream = LocalStream::from_io(listener.accept().await?.0);
                #[cfg(windows)]
                let mut stream = {
                    listener.connect().await?;
                    let next = Self::pipe(&self.endpoint, false)?;
                    LocalStream::from_io(guard.replace(next).expect("connected pipe"))
                };
                stream.lifetime = self.lifetime.clone();
                Ok(stream)
            } => result,
        }
    }
    pub async fn close(&self) {
        self.stop.cancel();
        self.inner.lock().await.take();
    }
}
