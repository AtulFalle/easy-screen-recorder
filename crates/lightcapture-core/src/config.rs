use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where to capture from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget {
    PrimaryDisplay,
    /// One-based display index (`1` is the first monitor, matching Windows Capture).
    DisplayIndex(usize),
    /// Window whose title contains this substring.
    WindowTitle(String),
    ForegroundWindow,
}

/// Quality preset. Resolution follows the source until a scaler exists; FPS is applied now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// 720p30 intent; encodes source size at 30 FPS.
    P720p30,
    /// 1080p30 intent; encodes source size at 30 FPS.
    P1080p30,
    /// 1080p60 intent; encodes source size at 60 FPS when the source can supply it.
    P1080p60,
}

impl Quality {
    #[must_use]
    pub const fn fps(self) -> u32 {
        match self {
            Self::P720p30 | Self::P1080p30 => 30,
            Self::P1080p60 => 60,
        }
    }
}

/// Settings for one recording session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordConfig {
    pub target: CaptureTarget,
    pub quality: Quality,
    pub output: PathBuf,
    pub include_cursor: bool,
}

impl RecordConfig {
    #[must_use]
    pub fn new(output: impl Into<PathBuf>) -> Self {
        Self {
            target: CaptureTarget::PrimaryDisplay,
            quality: Quality::P1080p30,
            output: output.into(),
            include_cursor: true,
        }
    }

    /// `lightcapture-YYYYMMDD-HHMMSS.mp4` in `dir`.
    #[must_use]
    pub fn default_output_in(dir: &Path) -> PathBuf {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        dir.join(format!("lightcapture-{secs}.mp4"))
    }
}

/// H.264 CBR bitrate from geometry and FPS, clamped to 4–16 Mbps.
#[must_use]
pub(crate) fn default_bitrate_bps(width: u32, height: u32, fps: u32) -> u32 {
    let raw = u64::from(width) * u64::from(height) * u64::from(fps) / 10;
    u32::try_from(raw.clamp(4_000_000, 16_000_000)).unwrap_or(8_000_000)
}

/// H.264 requires even width and height.
#[must_use]
pub(crate) fn even_dimension(value: u32) -> u32 {
    value & !1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_fps() {
        assert_eq!(Quality::P720p30.fps(), 30);
        assert_eq!(Quality::P1080p30.fps(), 30);
        assert_eq!(Quality::P1080p60.fps(), 60);
    }

    #[test]
    fn bitrate_clamps() {
        assert_eq!(default_bitrate_bps(640, 360, 24), 4_000_000);
        assert_eq!(default_bitrate_bps(3840, 2160, 60), 16_000_000);
        assert_eq!(default_bitrate_bps(1920, 1080, 30), 1920 * 1080 * 30 / 10);
    }

    #[test]
    fn even_rounds_down() {
        assert_eq!(even_dimension(1920), 1920);
        assert_eq!(even_dimension(1919), 1918);
        assert_eq!(even_dimension(1), 0);
    }

    #[test]
    fn default_output_is_mp4() {
        let path = RecordConfig::default_output_in(Path::new("out"));
        assert!(path.extension().is_some_and(|e| e == "mp4"));
    }
}
