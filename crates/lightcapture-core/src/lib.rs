//! LightCapture recording engine.
//!
//! This crate will own capture, encode, audio, and mux. It currently exposes
//! only the workspace version so the foundation builds without recorder logic.

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
