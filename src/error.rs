//! Error types for `rusty-embers`.

use thiserror::Error;

/// Result type alias for `rusty-embers`.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in `rusty-embers`.
#[derive(Debug, Error)]
pub enum Error {
    /// An S101 framing error.
    #[error("S101 framing error: {0}")]
    S101(String),

    /// A Glow/BER encoding or decoding error.
    #[error("Glow error: {0}")]
    Glow(String),

    /// An I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
