pub mod button;
pub mod command;
pub mod events;
pub mod http;
#[cfg(unix)]
pub mod input;
#[cfg(windows)]
#[path = "input_windows.rs"]
pub mod input;
#[cfg(any(windows, test))]
mod input_utf16;
pub mod mcp;
pub mod output;
pub mod process;
pub mod publisher;
pub mod stream;
pub mod surface;
pub mod tail;
pub mod watch;

mod local;
