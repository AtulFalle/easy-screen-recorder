use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::time::{SystemTime, UNIX_EPOCH};

/// Where to capture from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget {
    PrimaryDisplay,
    /// One-based display index (`1` is the first monitor, matching Windows Capture).
    DisplayIndex(usize),
    /// Stable monitor id (`\\.\DISPLAY1` or a DisplayConfig path).
    Display {
        id: String,
    },
    /// Window whose title contains this substring.
    WindowTitle(String),
    ForegroundWindow,
}

impl CaptureTarget {
    /// Filesystem-safe token used in default recording names.
    #[must_use]
    pub fn source_slug(&self) -> String {
        match self {
            Self::PrimaryDisplay => "primary".into(),
            Self::DisplayIndex(index) => format!("display{index}"),
            Self::Display { id } => sanitize_source_slug(id),
            Self::WindowTitle(title) => sanitize_source_slug(title),
            Self::ForegroundWindow => "foreground".into(),
        }
    }
}

/// Quality preset. FPS is applied now; frames are GPU-scaled to fit the preset box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Fit inside 1280×720 at 24 FPS. Never upscales.
    P720p24,
    /// Fit inside 1280×720 at 30 FPS. Never upscales.
    P720p30,
    /// Fit inside 1920×1080 at 30 FPS. Never upscales.
    P1080p30,
    /// Fit inside 1920×1080 at 60 FPS when the source can supply it. Never upscales.
    P1080p60,
}

impl Quality {
    #[must_use]
    pub const fn fps(self) -> u32 {
        match self {
            Self::P720p24 => 24,
            Self::P720p30 | Self::P1080p30 => 30,
            Self::P1080p60 => 60,
        }
    }

    /// Bounding box the encoder must not exceed.
    #[must_use]
    pub const fn max_size(self) -> (u32, u32) {
        match self {
            Self::P720p24 | Self::P720p30 => (1280, 720),
            Self::P1080p30 | Self::P1080p60 => (1920, 1080),
        }
    }

    /// Even H.264 size that fits in this preset. Source pixels are never upscaled.
    #[must_use]
    pub fn encode_size(self, src_w: u32, src_h: u32) -> (u32, u32) {
        let src_w = even_dimension(src_w).max(16);
        let src_h = even_dimension(src_h).max(16);
        let (max_w, max_h) = self.max_size();
        if src_w <= max_w && src_h <= max_h {
            return (src_w, src_h);
        }
        let h_from_w = src_h.saturating_mul(max_w) / src_w.max(1);
        if h_from_w <= max_h {
            (
                even_dimension(max_w).max(16),
                even_dimension(h_from_w).max(16),
            )
        } else {
            let w_from_h = src_w.saturating_mul(max_h) / src_h.max(1);
            (
                even_dimension(w_from_h).max(16),
                even_dimension(max_h).max(16),
            )
        }
    }
}

/// Which WASAPI sources to open for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioConfig {
    /// Default render device loopback (system audio).
    pub system: bool,
    /// Default capture device (microphone).
    pub microphone: bool,
}

impl AudioConfig {
    /// Both sources off — video-only encode, matching MVP0.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            system: false,
            microphone: false,
        }
    }

    #[must_use]
    pub const fn is_enabled(self) -> bool {
        self.system || self.microphone
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            system: true,
            microphone: true,
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
    pub audio: AudioConfig,
}

impl RecordConfig {
    #[must_use]
    pub fn new(output: impl Into<PathBuf>) -> Self {
        Self {
            target: CaptureTarget::PrimaryDisplay,
            quality: Quality::P1080p30,
            output: output.into(),
            include_cursor: true,
            audio: AudioConfig::default(),
        }
    }

    /// `LightCapture-YYYYMMDD-HHMMSS-primary.mp4` in `dir`.
    #[must_use]
    pub fn default_output_in(dir: &Path) -> PathBuf {
        Self::output_path(dir, "primary")
    }

    /// `LightCapture-YYYYMMDD-HHMMSS-{source}.mp4` in `dir`.
    #[must_use]
    pub fn output_path(dir: &Path, source: &str) -> PathBuf {
        dir.join(recording_filename(&local_stamp(), source))
    }
}

/// `LightCapture-YYYYMMDD-HHMMSS-{source}.mp4`
#[must_use]
pub fn recording_filename(stamp: &str, source: &str) -> String {
    format!("LightCapture-{stamp}-{}.mp4", sanitize_source_slug(source))
}

/// Keep ASCII letters, digits, `_`, and `-`; collapse other runs to `-`.
#[must_use]
pub fn sanitize_source_slug(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if out.len() >= 40 {
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "source".into()
    } else {
        out
    }
}

fn local_stamp() -> String {
    #[cfg(windows)]
    {
        // SAFETY: GetLocalTime writes a stack SYSTEMTIME; the wrapper returns it by value.
        let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        )
    }
    #[cfg(not(windows))]
    {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("{secs}")
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
        assert_eq!(Quality::P720p24.fps(), 24);
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
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("LightCapture-"));
        assert!(name.ends_with("-primary.mp4"));
    }

    #[test]
    fn encode_size_downscales_4k_to_1080() {
        assert_eq!(Quality::P1080p30.encode_size(3840, 2160), (1920, 1080));
        assert_eq!(Quality::P720p30.encode_size(3840, 2160), (1280, 720));
    }

    #[test]
    fn encode_size_does_not_upscale() {
        assert_eq!(Quality::P1080p30.encode_size(1280, 720), (1280, 720));
        assert_eq!(Quality::P1080p60.encode_size(800, 600), (800, 600));
    }

    #[test]
    fn encode_size_fits_16_by_10() {
        assert_eq!(Quality::P1080p30.encode_size(2560, 1600), (1728, 1080));
    }

    #[test]
    fn recording_filename_uses_slug() {
        assert_eq!(
            recording_filename("20260910-184000", "Office Monitor"),
            "LightCapture-20260910-184000-Office-Monitor.mp4"
        );
        assert_eq!(sanitize_source_slug(r"\\.\DISPLAY1"), "DISPLAY1");
        assert_eq!(sanitize_source_slug("@@@"), "source");
    }

    #[test]
    fn source_slug_for_targets() {
        assert_eq!(CaptureTarget::PrimaryDisplay.source_slug(), "primary");
        assert_eq!(CaptureTarget::ForegroundWindow.source_slug(), "foreground");
        assert_eq!(
            CaptureTarget::WindowTitle("Figma — File".into()).source_slug(),
            "Figma-File"
        );
    }

    #[test]
    fn audio_defaults_on() {
        let audio = AudioConfig::default();
        assert!(audio.system);
        assert!(audio.microphone);
        assert!(audio.is_enabled());
        let config = RecordConfig::new("out.mp4");
        assert_eq!(config.audio, audio);
    }

    #[test]
    fn audio_none_is_disabled() {
        assert!(!AudioConfig::none().is_enabled());
    }
}
