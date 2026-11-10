//! Protocol-independent, user-private IPC. Publication owns its lifetime lock until
//! the listener and every accepted stream have been dropped.
pub mod binding;
mod endpoint;
pub mod files;
#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
mod publication;
pub use endpoint::{LocalEndpoint, LocalListener, LocalStream};
pub use platform::{config_root, default_runtime_root};
pub use publication::{
    DiscoveredService, LocalRuntime, Manifest, ServicePublication, validate_service_id,
};

pub(crate) fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into())
}
pub(crate) fn random_name() -> std::io::Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
