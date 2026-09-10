use std::fs;
use std::path::{Path, PathBuf};

use lightcapture_core::{CaptureTarget, Quality};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualitySetting {
    #[serde(rename = "720p30")]
    P720p30,
    #[serde(rename = "1080p30")]
    P1080p30,
    #[serde(rename = "1080p60")]
    P1080p60,
}

impl From<QualitySetting> for Quality {
    fn from(value: QualitySetting) -> Self {
        match value {
            QualitySetting::P720p30 => Self::P720p30,
            QualitySetting::P1080p30 => Self::P1080p30,
            QualitySetting::P1080p60 => Self::P1080p60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceSetting {
    Primary,
    Display { index: usize },
    Foreground,
}

impl SourceSetting {
    #[must_use]
    pub fn to_target(&self) -> CaptureTarget {
        match self {
            Self::Primary => CaptureTarget::PrimaryDisplay,
            Self::Display { index } => CaptureTarget::DisplayIndex(*index),
            Self::Foreground => CaptureTarget::ForegroundWindow,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub quality: QualitySetting,
    pub source: SourceSetting,
    pub output_dir: PathBuf,
    pub include_cursor: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quality: QualitySetting::P1080p30,
            source: SourceSetting::Primary,
            output_dir: default_output_dir(),
            include_cursor: true,
        }
    }
}

impl Settings {
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(&settings_path())
    }

    #[must_use]
    pub fn load_from(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&settings_path())
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, raw)
    }
}

#[must_use]
pub fn settings_path() -> PathBuf {
    match directories::BaseDirs::new() {
        Some(base) => base.config_dir().join("LightCapture").join("settings.json"),
        None => PathBuf::from("lightcapture-settings.json"),
    }
}

#[must_use]
pub fn default_output_dir() -> PathBuf {
    if let Some(user) = directories::UserDirs::new() {
        if let Some(videos) = user.video_dir() {
            return videos.join("LightCapture");
        }
        return user.home_dir().join("LightCapture");
    }
    PathBuf::from("LightCapture")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lightcapture-settings-{}-{name}.json",
            std::process::id()
        ))
    }

    #[test]
    fn default_quality_is_1080p30() {
        let settings = Settings {
            output_dir: PathBuf::from("out"),
            ..Settings::default()
        };
        assert_eq!(settings.quality, QualitySetting::P1080p30);
        assert!(settings.include_cursor);
        assert_eq!(settings.source, SourceSetting::Primary);
        assert_eq!(Quality::from(settings.quality).fps(), 30);
    }

    #[test]
    fn source_maps_to_engine_target() {
        assert_eq!(
            SourceSetting::Primary.to_target(),
            CaptureTarget::PrimaryDisplay
        );
        assert_eq!(
            SourceSetting::Display { index: 2 }.to_target(),
            CaptureTarget::DisplayIndex(2)
        );
        assert_eq!(
            SourceSetting::Foreground.to_target(),
            CaptureTarget::ForegroundWindow
        );
    }

    #[test]
    fn round_trip_json() {
        let path = temp_settings_path("round-trip");
        let _ = fs::remove_file(&path);
        let original = Settings {
            quality: QualitySetting::P720p30,
            source: SourceSetting::Display { index: 1 },
            output_dir: PathBuf::from(r"D:\recordings"),
            include_cursor: false,
        };
        original.save_to(&path).expect("save");
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded, original);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn invalid_json_uses_defaults() {
        let path = temp_settings_path("invalid");
        fs::write(&path, "{not json").expect("write");
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.quality, QualitySetting::P1080p30);
        assert_eq!(loaded.source, SourceSetting::Primary);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_uses_defaults() {
        let path = temp_settings_path("missing");
        let _ = fs::remove_file(&path);
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.quality, QualitySetting::P1080p30);
    }
}
