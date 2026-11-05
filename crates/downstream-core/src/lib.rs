//! Pure downstream contracts and application/acknowledgement state.
//! Transport, clocks, cancellation and persistence belong to adapters.
pub mod protocol;
pub mod session;
pub use protocol::*;
pub use session::{SessionState, SourceLimits};

#[cfg(test)]
mod tests;
