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
    #[error("disk is full; kept the bytes already written")]
    DiskFull,
    #[error("encoder lost; kept the bytes already written")]
    EncoderLost,
    #[error("audio failed: {0}")]
    Audio(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("no frames were encoded")]
    NoFrames,
    #[error("the recording session is no longer running")]
    Stopped,
    #[error("stream ingest failed: {0}")]
    Stream(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// True when a Windows/Media Foundation error string looks like a full volume.
#[must_use]
pub fn is_disk_full_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("disk full")
        || lower.contains("not enough space")
        || lower.contains("there is not enough space")
        || lower.contains("0x80070070")
        || lower.contains("0x80070027")
        || lower.contains("error_disk_full")
        || lower.contains("error_handle_disk_full")
}

/// Map an encoder/mux failure onto [`Error::DiskFull`], [`Error::EncoderLost`], or [`Error::Encoder`].
#[must_use]
pub fn classify_encode_failure(message: &str) -> Error {
    if is_disk_full_message(message) {
        Error::DiskFull
    } else {
        Error::EncoderLost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_full_hresult() {
        assert!(is_disk_full_message("Write failed 0x80070070"));
        assert!(is_disk_full_message(
            "There is not enough space on the disk."
        ));
        assert!(!is_disk_full_message("MFT process down"));
        assert!(matches!(
            classify_encode_failure("0x80070070"),
            Error::DiskFull
        ));
        assert!(matches!(
            classify_encode_failure("hardware MFT disappeared"),
            Error::EncoderLost
        ));
    }
}
