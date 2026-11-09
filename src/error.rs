//! Crate-wide error type.

use std::fmt;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// Invalid command-line usage.
    Usage(String),
    /// Missing or invalid configuration (environment variables, input values).
    Config(String),
    /// A network request failed after all retries.
    Network(String),
    /// GitHub answered with an error or an unexpected payload.
    GitHub(String),
    /// Reading or writing a file failed.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A JSON document could not be parsed or written.
    Json(String),
    /// Image encoding failed.
    Encode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(msg) => write!(f, "{msg}"),
            Self::Config(msg) => write!(f, "configuration error: {msg}"),
            Self::Network(msg) => write!(f, "network error: {msg}"),
            Self::GitHub(msg) => write!(f, "GitHub error: {msg}"),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Json(msg) => write!(f, "invalid JSON: {msg}"),
            Self::Encode(msg) => write!(f, "image encoding failed: {msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
