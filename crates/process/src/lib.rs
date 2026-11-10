//! Owned child process trees with a platform-independent shutdown interface.
use std::{ffi::OsStr, io, path::Path, process::ExitStatus, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};
#[cfg(windows)]
mod windows;
pub const SIGTERM: i32 = 15;
pub const SIGKILL: i32 = 9;
pub const SIGINT: i32 = 2;
#[derive(Clone, Copy)]
pub enum Stdio {
    Inherit,
    Piped,
    Null,
}
impl Stdio {
    pub fn inherit() -> Self {
        Self::Inherit
    }
    pub fn piped() -> Self {
        Self::Piped
    }
    pub fn null() -> Self {
        Self::Null
    }
}
#[derive(Clone)]
pub struct Group(Arc<GroupInner>);
struct GroupInner {
    pid: u32,
    #[cfg(windows)]
    job: std::fs::File,
}
impl Group {
    pub fn signal(&self, signal: i32) {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.0.pid as i32), signal);
        }
        #[cfg(windows)]
        windows::signal(&self.0, signal);
    }
}
impl Drop for GroupInner {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.pid as i32), SIGKILL);
        }
        // On Windows the final job handle closes and kills its remaining members.
    }
}
pub struct Command {
    std: std::process::Command,
    input: Stdio,
    output: Stdio,
    error: Stdio,
}
impl Command {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            std: std::process::Command::new(program),
            input: Stdio::Inherit,
            output: Stdio::Inherit,
            error: Stdio::Inherit,
        }
    }
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.std.args(args);
        self
    }
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.std.arg(arg);
        self
    }
    pub fn current_dir(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.std.current_dir(path);
        self
    }
    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.std.env(key, value);
        self
    }
    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.std.envs(vars);
        self
    }
    pub fn stdin(&mut self, value: Stdio) -> &mut Self {
        self.input = value;
        self
    }
    pub fn stdout(&mut self, value: Stdio) -> &mut Self {
        self.output = value;
        self
    }
    pub fn stderr(&mut self, value: Stdio) -> &mut Self {
        self.error = value;
        self
    }
    pub fn spawn(&mut self) -> io::Result<Child> {
        #[cfg(unix)]
        {
            fn stdio(s: Stdio) -> std::process::Stdio {
                match s {
                    Stdio::Inherit => std::process::Stdio::inherit(),
                    Stdio::Piped => std::process::Stdio::piped(),
                    Stdio::Null => std::process::Stdio::null(),
                }
            }
            let mut cmd = tokio::process::Command::new(self.std.get_program());
            cmd.args(self.std.get_args());
            for (key, value) in self.std.get_envs() {
                if let Some(value) = value {
                    cmd.env(key, value);
                } else {
                    cmd.env_remove(key);
                }
            }
            if let Some(path) = self.std.get_current_dir() {
                cmd.current_dir(path);
            }
            cmd.stdin(stdio(self.input))
                .stdout(stdio(self.output))
                .stderr(stdio(self.error));
            let mut child = cmd.process_group(0).kill_on_drop(true).spawn()?;
            let group = Group(Arc::new(GroupInner {
                pid: child.id().unwrap(),
            }));
            Ok(Child {
                stdin: child.stdin.take().map(|v| Box::new(v) as _),
                stdout: child.stdout.take().map(|v| Box::new(v) as _),
                stderr: child.stderr.take().map(|v| Box::new(v) as _),
                inner: child,
                group,
            })
        }
        #[cfg(windows)]
        {
            windows::spawn(self)
        }
    }
}
pub struct Child {
    pub stdin: Option<Box<dyn AsyncWrite + Send + Unpin>>,
    pub stdout: Option<Box<dyn AsyncRead + Send + Unpin>>,
    pub stderr: Option<Box<dyn AsyncRead + Send + Unpin>>,
    #[cfg(unix)]
    inner: tokio::process::Child,
    #[cfg(windows)]
    inner: std::fs::File,
    group: Group,
}
impl Child {
    pub fn id(&self) -> Option<u32> {
        Some(self.group.0.pid)
    }
    pub fn group(&self) -> Group {
        self.group.clone()
    }
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        #[cfg(unix)]
        {
            self.inner.wait().await
        }
        #[cfg(windows)]
        {
            windows::wait(&self.inner).await
        }
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        self.group.signal(SIGKILL);
    }
}
pub fn exit_signal(status: &ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(windows)]
    {
        let _ = status;
        None
    }
}
pub struct ShutdownSignal {
    #[cfg(windows)]
    parent: windows::ParentMonitor,
    #[cfg(unix)]
    term: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(windows)]
    interrupt: tokio::signal::windows::CtrlC,
    #[cfg(windows)]
    term: tokio::signal::windows::CtrlBreak,
}
impl ShutdownSignal {
    pub fn new() -> io::Result<Self> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            Ok(Self {
                term: signal(SignalKind::terminate())?,
                interrupt: signal(SignalKind::interrupt())?,
            })
        }
        #[cfg(windows)]
        {
            Ok(Self {
                parent: windows::ParentMonitor::new()?,
                term: tokio::signal::windows::ctrl_break()?,
                interrupt: tokio::signal::windows::ctrl_c()?,
            })
        }
    }
    pub async fn wait(&mut self) {
        #[cfg(unix)]
        {
            tokio::select! { _ = self.term.recv() => {}, _ = self.interrupt.recv() => {} }
        }
        #[cfg(windows)]
        {
            tokio::select! { _ = self.term.recv() => {}, _ = self.interrupt.recv() => {}, _ = self.parent.wait() => {} }
        }
    }
}
pub fn open_browser(url: &str) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::open_browser(url)
    }
    #[cfg(unix)]
    {
        let executable = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let mut child = std::process::Command::new(executable)
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}
