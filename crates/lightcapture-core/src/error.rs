use std::io;

/// Recoverable engine failure. Library code returns this instead of panicking.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("LightCapture recording requires Windows")]
    UnsupportedPlatform,
    #[error("capture target not found: {0}")]
    TargetNotFound(String),
    #[error("invalid capture target: {0}")]
    InvalidTarget(String),
    #[error("capture failed: {0}")]
    Capture(String),
    #[error("encoder failed: {0}")]
    Encoder(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("no frames were encoded")]
    NoFrames,
    #[error("the recording session is no longer running")]
    Stopped,
}

pub type Result<T> = std::result::Result<T, Error>;
