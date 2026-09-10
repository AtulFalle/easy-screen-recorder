//! LightCapture recording engine: capture, encode, and mux.
//!
//! Frontends (`lightcapture-cli`, `lightcapture-app`) call this crate only.

#![deny(unsafe_op_in_unsafe_fn)]

mod adaptive;
mod audio;
mod config;
mod error;
mod frame_gate;
mod hardware;
mod platform;
mod session;
mod stats;
mod stream;

pub use adaptive::step_down_fps;
pub use config::{
    recording_filename, sanitize_source_slug, AudioConfig, CaptureTarget, Quality, RecordConfig,
};
pub use error::{classify_encode_failure, is_disk_full_message, Error, Result};
pub use hardware::{classify_encoder, pick_encoder, probe, EncoderKind, HardwareInfo};
pub use session::{list_displays, list_windows, start, CaptureDisplay, CaptureWindow, Recording};
pub use stats::SessionStats;
pub use stream::{publish_file, view_url};

/// Workspace package version for CLI and app banners.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_populated() {
        assert!(!version().is_empty());
        assert!(version().contains('.'));
    }
}
