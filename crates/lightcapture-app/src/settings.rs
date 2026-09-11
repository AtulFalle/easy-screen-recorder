use std::fs;
use std::path::{Path, PathBuf};

use lightcapture_core::{
    sanitize_source_slug, AudioConfig, CaptureDisplay, CaptureTarget, Quality,
};
use serde::{Deserialize, Serialize};

pub const RECENT_MAX: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualitySetting {
    #[serde(rename = "720p24")]
    P720p24,
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
            QualitySetting::P720p24 => Self::P720p24,
            QualitySetting::P720p30 => Self::P720p30,
            QualitySetting::P1080p30 => Self::P1080p30,
            QualitySetting::P1080p60 => Self::P1080p60,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProfileId {
    #[default]
    Work,
    Game,
    Silent,
}

impl ProfileId {
    pub fn apply(self, settings: &mut Settings) {
        settings.profile = self;
        match self {
            Self::Work => {
                settings.quality = QualitySetting::P1080p30;
                settings.audio = AudioSetting {
                    system: true,
                    microphone: true,
                };
            }
            Self::Game => {
                settings.quality = QualitySetting::P1080p60;
                settings.audio = AudioSetting {
                    system: true,
                    microphone: false,
                };
            }
            Self::Silent => {
                settings.quality = QualitySetting::P720p30;
                settings.audio = AudioSetting {
                    system: false,
                    microphone: false,
                };
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSetting {
    pub system: bool,
    pub microphone: bool,
}

impl Default for AudioSetting {
    fn default() -> Self {
        Self {
            system: true,
            microphone: true,
        }
    }
}

impl From<AudioSetting> for AudioConfig {
    fn from(value: AudioSetting) -> Self {
        Self {
            system: value.system,
            microphone: value.microphone,
        }
    }
}

impl AudioSetting {
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        self.system || self.microphone
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceSetting {
    Primary,
    Display {
        #[serde(default)]
        index: usize,
        #[serde(default)]
        id: String,
        #[serde(default)]
        name: String,
    },
    Foreground,
    Window {
        title: String,
    },
}

impl SourceSetting {
    #[cfg(test)]
    #[must_use]
    pub fn to_target(&self) -> CaptureTarget {
        match self {
            Self::Primary => CaptureTarget::PrimaryDisplay,
            Self::Display { id, index, .. } if !id.is_empty() => {
                CaptureTarget::Display { id: id.clone() }
            }
            Self::Display { index, .. } => CaptureTarget::DisplayIndex(*index),
            Self::Foreground => CaptureTarget::ForegroundWindow,
            Self::Window { title } => CaptureTarget::WindowTitle(title.clone()),
        }
    }

    #[must_use]
    pub fn resolve(&self, displays: &[CaptureDisplay]) -> CaptureTarget {
        match self {
            Self::Primary => CaptureTarget::PrimaryDisplay,
            Self::Foreground => CaptureTarget::ForegroundWindow,
            Self::Display { id, name, index } => {
                if !id.is_empty() && displays.iter().any(|d| d.device_id == *id) {
                    return CaptureTarget::Display { id: id.clone() };
                }
                if !name.is_empty() {
                    if let Some(display) = displays.iter().find(|d| d.name == *name) {
                        return CaptureTarget::Display {
                            id: display.device_id.clone(),
                        };
                    }
                }
                if *index >= 1 {
                    CaptureTarget::DisplayIndex(*index)
                } else if !id.is_empty() {
                    CaptureTarget::Display { id: id.clone() }
                } else {
                    CaptureTarget::PrimaryDisplay
                }
            }
            Self::Window { title } => CaptureTarget::WindowTitle(title.clone()),
        }
    }

    #[must_use]
    pub fn source_slug(&self) -> String {
        match self {
            Self::Primary => "primary".into(),
            Self::Foreground => "foreground".into(),
            Self::Display { name, id, index } => {
                if !name.trim().is_empty() {
                    sanitize_source_slug(name)
                } else if !id.is_empty() {
                    sanitize_source_slug(id)
                } else {
                    format!("display{index}")
                }
            }
            Self::Window { title } => sanitize_source_slug(title),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub quality: QualitySetting,
    pub source: SourceSetting,
    pub output_dir: PathBuf,
    pub include_cursor: bool,
    #[serde(default)]
    pub audio: AudioSetting,
    #[serde(default)]
    pub profile: ProfileId,
    #[serde(default)]
    pub recent: Vec<PathBuf>,
    #[serde(default)]
    pub stream_enabled: bool,
    #[serde(default = "default_stream_url")]
    pub stream_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quality: QualitySetting::P1080p30,
            source: SourceSetting::Primary,
            output_dir: default_output_dir(),
            include_cursor: true,
            audio: AudioSetting::default(),
            profile: ProfileId::Work,
            recent: Vec::new(),
            stream_enabled: false,
            stream_url: default_stream_url(),
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

    pub fn push_recent(&mut self, path: PathBuf) {
        self.recent.retain(|existing| existing != &path);
        self.recent.insert(0, path);
        self.recent.truncate(RECENT_MAX);
    }
}

#[must_use]
pub fn default_stream_url() -> String {
    "rtsp://127.0.0.1:8554/live".into()
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
        assert_eq!(settings.profile, ProfileId::Work);
        assert!(settings.audio.system && settings.audio.microphone);
        assert_eq!(Quality::from(settings.quality).fps(), 30);
    }

    #[test]
    fn source_maps_to_engine_target() {
        assert_eq!(
            SourceSetting::Primary.to_target(),
            CaptureTarget::PrimaryDisplay
        );
        assert_eq!(
            SourceSetting::Display {
                index: 2,
                id: String::new(),
                name: String::new()
            }
            .to_target(),
            CaptureTarget::DisplayIndex(2)
        );
        assert_eq!(
            SourceSetting::Display {
                index: 2,
                id: r"\\.\DISPLAY2".into(),
                name: "Office".into()
            }
            .to_target(),
            CaptureTarget::Display {
                id: r"\\.\DISPLAY2".into()
            }
        );
        assert_eq!(
            SourceSetting::Foreground.to_target(),
            CaptureTarget::ForegroundWindow
        );
    }

    #[test]
    fn resolve_prefers_name_when_id_missing() {
        let displays = [CaptureDisplay {
            index: 2,
            name: "Office".into(),
            device_id: r"\\.\DISPLAY2".into(),
            width: 1920,
            height: 1080,
        }];
        let source = SourceSetting::Display {
            index: 1,
            id: r"\\.\DISPLAY9".into(),
            name: "Office".into(),
        };
        assert_eq!(
            source.resolve(&displays),
            CaptureTarget::Display {
                id: r"\\.\DISPLAY2".into()
            }
        );
    }

    #[test]
    fn game_profile_is_1080p60_system_only() {
        let mut settings = Settings::default();
        ProfileId::Game.apply(&mut settings);
        assert_eq!(settings.quality, QualitySetting::P1080p60);
        assert!(settings.audio.system);
        assert!(!settings.audio.microphone);
        ProfileId::Silent.apply(&mut settings);
        assert_eq!(settings.quality, QualitySetting::P720p30);
        assert!(!settings.audio.is_enabled());
    }

    #[test]
    fn recent_is_newest_first_capped() {
        let mut settings = Settings::default();
        for i in 0..12 {
            settings.push_recent(PathBuf::from(format!("f{i}.mp4")));
        }
        assert_eq!(settings.recent.len(), RECENT_MAX);
        assert_eq!(settings.recent[0], PathBuf::from("f11.mp4"));
        settings.push_recent(PathBuf::from("f11.mp4"));
        assert_eq!(
            settings
                .recent
                .iter()
                .filter(|p| p.file_name().unwrap() == "f11.mp4")
                .count(),
            1
        );
        assert_eq!(settings.recent[0], PathBuf::from("f11.mp4"));
    }

    #[test]
    fn round_trip_json() {
        let path = temp_settings_path("round-trip");
        let _ = fs::remove_file(&path);
        let original = Settings {
            quality: QualitySetting::P720p30,
            source: SourceSetting::Display {
                index: 1,
                id: r"\\.\DISPLAY1".into(),
                name: "Office".into(),
            },
            output_dir: PathBuf::from(r"D:\recordings"),
            include_cursor: false,
            audio: AudioSetting {
                system: true,
                microphone: false,
            },
            profile: ProfileId::Game,
            recent: vec![PathBuf::from(r"D:\recordings\a.mp4")],
            stream_enabled: true,
            stream_url: "rtsp://127.0.0.1:8554/live".into(),
        };
        original.save_to(&path).expect("save");
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded, original);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn legacy_display_index_json_loads() {
        let path = temp_settings_path("legacy-display");
        fs::write(
            &path,
            r#"{"quality":"1080p30","source":{"kind":"display","index":2},"output_dir":"out","include_cursor":true}"#,
        )
        .expect("write");
        let loaded = Settings::load_from(&path);
        assert_eq!(
            loaded.source,
            SourceSetting::Display {
                index: 2,
                id: String::new(),
                name: String::new()
            }
        );
        assert_eq!(loaded.profile, ProfileId::Work);
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
        assert!(!loaded.stream_enabled);
        assert_eq!(loaded.stream_url, default_stream_url());
    }
}
