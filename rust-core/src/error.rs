//! Error types for the MorseLink core.

use std::io;

/// A single error type surfaced across the FFI boundary. It carries a
/// high-level `kind` (used to drive UI messaging) and a human message.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("discovery error: {0}")]
    Discovery(String),
    #[error("transfer error: {0}")]
    Transfer(String),
    #[error("io error: {0}")]
    Io(String),
}

impl EngineError {
    pub fn transport(e: impl ToString) -> Self {
        Self::Transport(e.to_string())
    }
    pub fn discovery(e: impl ToString) -> Self {
        Self::Discovery(e.to_string())
    }
    pub fn transfer(e: impl ToString) -> Self {
        Self::Transfer(e.to_string())
    }
}

impl From<io::Error> for EngineError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<serde_json::Error> for EngineError {
    fn from(e: serde_json::Error) -> Self {
        Self::Transport(format!("protocol serialization: {e}"))
    }
}

impl From<quinn::ConnectError> for EngineError {
    fn from(e: quinn::ConnectError) -> Self {
        Self::Transport(format!("connect: {e}"))
    }
}

impl From<quinn::ConnectionError> for EngineError {
    fn from(e: quinn::ConnectionError) -> Self {
        Self::Transport(format!("connection: {e}"))
    }
}

impl From<quinn::WriteError> for EngineError {
    fn from(e: quinn::WriteError) -> Self {
        Self::Transport(format!("write: {e}"))
    }
}

impl From<quinn::ReadError> for EngineError {
    fn from(e: quinn::ReadError) -> Self {
        Self::Transport(format!("read: {e}"))
    }
}

impl From<quinn::FinishError> for EngineError {
    fn from(e: quinn::FinishError) -> Self {
        Self::Transport(format!("finish: {e}"))
    }
}

/// Public result alias.
pub type Result<T> = std::result::Result<T, EngineError>;
