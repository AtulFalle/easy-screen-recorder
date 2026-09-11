use lightcapture_core::CaptureDisplay;
use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::settings::{AudioSetting, ProfileId, QualitySetting, Settings, SourceSetting};
use crate::status;

pub const ID_TOGGLE: &str = "toggle";
pub const ID_PAUSE: &str = "pause";
pub const ID_QUALITY_720_24: &str = "quality-720p24";
pub const ID_QUALITY_720: &str = "quality-720p30";
pub const ID_QUALITY_1080: &str = "quality-1080p30";
pub const ID_QUALITY_1080_60: &str = "quality-1080p60";
pub const ID_SOURCE_PRIMARY: &str = "source-primary";
pub const ID_SOURCE_FOREGROUND: &str = "source-foreground";
pub const ID_SOURCE_DISPLAY_PREFIX: &str = "source-display-";
pub const ID_OUTPUT_FOLDER: &str = "output-folder";
pub const ID_SHOW_CURSOR: &str = "show-cursor";
pub const ID_OPEN_FOLDER: &str = "open-folder";
pub const ID_EXIT: &str = "exit";
pub const ID_PROFILE_WORK: &str = "profile-work";
pub const ID_PROFILE_GAME: &str = "profile-game";
pub const ID_PROFILE_SILENT: &str = "profile-silent";
pub const ID_RECENT_OPEN_PREFIX: &str = "recent-open-";
pub const ID_RECENT_SHOW_PREFIX: &str = "recent-show-";
pub const ID_STREAM: &str = "stream";
pub const ID_STREAM_VIEW: &str = "stream-view";
pub const ID_SHOW_BAR: &str = "show-bar";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Pause,
    Quality(QualitySetting),
    Source(SourceSetting),
    Profile(ProfileId),
    PickFolder,
    ToggleCursor,
    OpenFolder,
    OpenRecent(usize),
    ShowRecent(usize),
    ToggleStream,
    OpenStreamViewer,
    SetAudio(AudioSetting),
    ShowBar,
    Exit,
}

#[must_use]
pub fn parse_menu_id(id: &str) -> Option<Command> {
    match id {
        ID_TOGGLE => Some(Command::Toggle),
        ID_PAUSE => Some(Command::Pause),
        ID_QUALITY_720_24 => Some(Command::Quality(QualitySetting::P720p24)),
        ID_QUALITY_720 => Some(Command::Quality(QualitySetting::P720p30)),
        ID_QUALITY_1080 => Some(Command::Quality(QualitySetting::P1080p30)),
        ID_QUALITY_1080_60 => Some(Command::Quality(QualitySetting::P1080p60)),
        ID_SOURCE_PRIMARY => Some(Command::Source(SourceSetting::Primary)),
        ID_SOURCE_FOREGROUND => Some(Command::Source(SourceSetting::Foreground)),
        ID_OUTPUT_FOLDER => Some(Command::PickFolder),
        ID_SHOW_CURSOR => Some(Command::ToggleCursor),
        ID_OPEN_FOLDER => Some(Command::OpenFolder),
        ID_EXIT => Some(Command::Exit),
        ID_PROFILE_WORK => Some(Command::Profile(ProfileId::Work)),
        ID_PROFILE_GAME => Some(Command::Profile(ProfileId::Game)),
        ID_PROFILE_SILENT => Some(Command::Profile(ProfileId::Silent)),
        ID_STREAM => Some(Command::ToggleStream),
        ID_STREAM_VIEW => Some(Command::OpenStreamViewer),
        ID_SHOW_BAR => Some(Command::ShowBar),
        other => {
            if let Some(rest) = other.strip_prefix(ID_SOURCE_DISPLAY_PREFIX) {
                return rest.parse().ok().map(|index| {
                    Command::Source(SourceSetting::Display {
                        index,
                        id: String::new(),
                        name: String::new(),
                    })
                });
            }
            if let Some(rest) = other.strip_prefix(ID_RECENT_OPEN_PREFIX) {
                return rest.parse().ok().map(Command::OpenRecent);
            }
            if let Some(rest) = other.strip_prefix(ID_RECENT_SHOW_PREFIX) {
                return rest.parse().ok().map(Command::ShowRecent);
            }
            None
        }
    }
}

#[must_use]
pub fn parse_hotkey_id(id: u32, record_id: u32, pause_id: u32) -> Option<Command> {
    if id == record_id {
        Some(Command::Toggle)
    } else if id == pause_id {
        Some(Command::Pause)
    } else {
        None
    }
}

pub struct TrayUi {
    tray: TrayIcon,
    toggle: MenuItem,
    pause: MenuItem,
    q720_24: CheckMenuItem,
    q720: CheckMenuItem,
    q1080: CheckMenuItem,
    q1080_60: CheckMenuItem,
    source_primary: CheckMenuItem,
    source_foreground: CheckMenuItem,
    source_displays: Vec<CheckMenuItem>,
    profile_work: CheckMenuItem,
    profile_game: CheckMenuItem,
    profile_silent: CheckMenuItem,
    output: MenuItem,
    cursor: CheckMenuItem,
    open: MenuItem,
    recent_open: Vec<MenuItem>,
    recent_show: Vec<MenuItem>,
    recent_entries: Vec<Submenu>,
    recent_empty: Option<MenuItem>,
    stream: CheckMenuItem,
    stream_view: MenuItem,
    show_bar: MenuItem,
    exit: MenuItem,
    idle_icon: Icon,
    recording_icon: Icon,
    paused_icon: Icon,
}

impl TrayUi {
    pub fn new(
        settings: &Settings,
        displays: &[CaptureDisplay],
        hotkeys: status::HotkeyStatus,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let idle_icon = status::idle_icon();
        let recording_icon = status::recording_icon();
        let paused_icon = status::paused_icon();
        let toggle = MenuItem::with_id(ID_TOGGLE, "Start recording", true, None);
        let pause = MenuItem::with_id(ID_PAUSE, status::pause_menu_text(false), false, None);
        let q720_24 = CheckMenuItem::with_id(
            ID_QUALITY_720_24,
            "720p24",
            true,
            settings.quality == QualitySetting::P720p24,
            None,
        );
        let q720 = CheckMenuItem::with_id(
            ID_QUALITY_720,
            "720p30",
            true,
            settings.quality == QualitySetting::P720p30,
            None,
        );
        let q1080 = CheckMenuItem::with_id(
            ID_QUALITY_1080,
            "1080p30",
            true,
            settings.quality == QualitySetting::P1080p30,
            None,
        );
        let q1080_60 = CheckMenuItem::with_id(
            ID_QUALITY_1080_60,
            "1080p60",
            true,
            settings.quality == QualitySetting::P1080p60,
            None,
        );
        let source_primary = CheckMenuItem::with_id(
            ID_SOURCE_PRIMARY,
            "Primary display",
            true,
            matches!(settings.source, SourceSetting::Primary),
            None,
        );
        let source_foreground = CheckMenuItem::with_id(
            ID_SOURCE_FOREGROUND,
            "Foreground window",
            true,
            matches!(settings.source, SourceSetting::Foreground),
            None,
        );
        let source_displays = display_items(displays, &settings.source);
        let profile_work = CheckMenuItem::with_id(
            ID_PROFILE_WORK,
            "Work",
            true,
            settings.profile == ProfileId::Work,
            None,
        );
        let profile_game = CheckMenuItem::with_id(
            ID_PROFILE_GAME,
            "Game",
            true,
            settings.profile == ProfileId::Game,
            None,
        );
        let profile_silent = CheckMenuItem::with_id(
            ID_PROFILE_SILENT,
            "Silent",
            true,
            settings.profile == ProfileId::Silent,
            None,
        );
        let output = MenuItem::with_id(ID_OUTPUT_FOLDER, "Output folder…", true, None);
        let cursor = CheckMenuItem::with_id(
            ID_SHOW_CURSOR,
            "Show cursor",
            true,
            settings.include_cursor,
            None,
        );
        let open = MenuItem::with_id(ID_OPEN_FOLDER, "Open recordings folder", true, None);
        let recent = recent_items(settings);
        let stream = CheckMenuItem::with_id(
            ID_STREAM,
            "Stream to MediaMTX",
            true,
            settings.stream_enabled,
            None,
        );
        let stream_view = MenuItem::with_id(ID_STREAM_VIEW, "Open stream viewer", true, None);
        let show_bar = MenuItem::with_id(ID_SHOW_BAR, "Show recorder", true, None);
        let exit = MenuItem::with_id(ID_EXIT, "Exit", true, None);
        let mut ui = Self {
            tray: TrayIconBuilder::new()
                .with_tooltip(status::tooltip(None, None, false, hotkeys))
                .with_icon(idle_icon.clone())
                .with_title("LightCapture")
                .build()?,
            toggle,
            pause,
            q720_24,
            q720,
            q1080,
            q1080_60,
            source_primary,
            source_foreground,
            source_displays,
            profile_work,
            profile_game,
            profile_silent,
            output,
            cursor,
            open,
            recent_open: recent.open,
            recent_show: recent.show,
            recent_entries: recent.entries,
            recent_empty: recent.empty,
            stream,
            stream_view,
            show_bar,
            exit,
            idle_icon,
            recording_icon,
            paused_icon,
        };
        ui.rebuild_menu()?;
        Ok(ui)
    }

    pub fn set_recording(&self, recording: bool) {
        self.toggle.set_text(if recording {
            "Stop recording"
        } else {
            "Start recording"
        });
        self.pause.set_enabled(recording);
        self.pause.set_text(status::pause_menu_text(false));
        let options_enabled = !recording;
        self.q720_24.set_enabled(options_enabled);
        self.q720.set_enabled(options_enabled);
        self.q1080.set_enabled(options_enabled);
        self.q1080_60.set_enabled(options_enabled);
        self.source_primary.set_enabled(options_enabled);
        self.source_foreground.set_enabled(options_enabled);
        for item in &self.source_displays {
            item.set_enabled(options_enabled);
        }
        self.profile_work.set_enabled(options_enabled);
        self.profile_game.set_enabled(options_enabled);
        self.profile_silent.set_enabled(options_enabled);
        self.output.set_enabled(options_enabled);
        self.cursor.set_enabled(options_enabled);
        let icon = if recording {
            &self.recording_icon
        } else {
            &self.idle_icon
        };
        let _ = self.tray.set_icon(Some(icon.clone()));
    }

    pub fn set_paused(&self, paused: bool) {
        self.pause.set_text(status::pause_menu_text(paused));
        let icon = if paused {
            &self.paused_icon
        } else {
            &self.recording_icon
        };
        let _ = self.tray.set_icon(Some(icon.clone()));
    }

    pub fn set_tooltip(&self, text: impl AsRef<str>) {
        let _ = self.tray.set_tooltip(Some(text.as_ref()));
    }

    pub fn apply_settings(&self, settings: &Settings) {
        self.q720_24
            .set_checked(settings.quality == QualitySetting::P720p24);
        self.q720
            .set_checked(settings.quality == QualitySetting::P720p30);
        self.q1080
            .set_checked(settings.quality == QualitySetting::P1080p30);
        self.q1080_60
            .set_checked(settings.quality == QualitySetting::P1080p60);
        self.source_primary
            .set_checked(matches!(settings.source, SourceSetting::Primary));
        self.source_foreground
            .set_checked(matches!(settings.source, SourceSetting::Foreground));
        for item in &self.source_displays {
            let checked = match &settings.source {
                SourceSetting::Display { index, .. } => {
                    item.id().as_ref() == format!("{ID_SOURCE_DISPLAY_PREFIX}{index}")
                }
                _ => false,
            };
            item.set_checked(checked);
        }
        self.profile_work
            .set_checked(settings.profile == ProfileId::Work);
        self.profile_game
            .set_checked(settings.profile == ProfileId::Game);
        self.profile_silent
            .set_checked(settings.profile == ProfileId::Silent);
        self.cursor.set_checked(settings.include_cursor);
        self.stream.set_checked(settings.stream_enabled);
    }

    pub fn refresh_menu(
        &mut self,
        displays: &[CaptureDisplay],
        settings: &Settings,
        recording: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.source_displays = display_items(displays, &settings.source);
        let recent = recent_items(settings);
        self.recent_open = recent.open;
        self.recent_show = recent.show;
        self.recent_entries = recent.entries;
        self.recent_empty = recent.empty;
        let enabled = !recording;
        self.source_primary.set_enabled(enabled);
        self.source_foreground.set_enabled(enabled);
        for item in &self.source_displays {
            item.set_enabled(enabled);
        }
        self.rebuild_menu()?;
        Ok(())
    }

    fn rebuild_menu(&mut self) -> Result<(), tray_icon::menu::Error> {
        let menu = assemble_menu(&MenuBits {
            toggle: &self.toggle,
            pause: &self.pause,
            q720_24: &self.q720_24,
            q720: &self.q720,
            q1080: &self.q1080,
            q1080_60: &self.q1080_60,
            source_primary: &self.source_primary,
            source_displays: &self.source_displays,
            source_foreground: &self.source_foreground,
            profile_work: &self.profile_work,
            profile_game: &self.profile_game,
            profile_silent: &self.profile_silent,
            output: &self.output,
            cursor: &self.cursor,
            open: &self.open,
            recent_entries: &self.recent_entries,
            recent_empty: self.recent_empty.as_ref(),
            stream: &self.stream,
            stream_view: &self.stream_view,
            show_bar: &self.show_bar,
            exit: &self.exit,
        })?;
        self.tray.set_menu(Some(Box::new(menu)));
        Ok(())
    }
}

fn display_items(displays: &[CaptureDisplay], source: &SourceSetting) -> Vec<CheckMenuItem> {
    displays
        .iter()
        .map(|display| {
            let id = format!("{ID_SOURCE_DISPLAY_PREFIX}{}", display.index);
            let checked = match source {
                SourceSetting::Display { id, index, name } => {
                    (!id.is_empty() && *id == display.device_id)
                        || (!name.is_empty() && *name == display.name)
                        || (*index == display.index)
                }
                _ => false,
            };
            CheckMenuItem::with_id(id, status::display_label(display), true, checked, None)
        })
        .collect()
}

struct RecentBits {
    open: Vec<MenuItem>,
    show: Vec<MenuItem>,
    entries: Vec<Submenu>,
    empty: Option<MenuItem>,
}

fn recent_items(settings: &Settings) -> RecentBits {
    if settings.recent.is_empty() {
        return RecentBits {
            open: Vec::new(),
            show: Vec::new(),
            entries: Vec::new(),
            empty: Some(MenuItem::with_id(
                "recent-empty",
                "No recent recordings",
                false,
                None,
            )),
        };
    }
    let mut open = Vec::new();
    let mut show = Vec::new();
    let mut entries = Vec::new();
    for (index, path) in settings.recent.iter().enumerate() {
        let label = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("recording.mp4");
        let open_item = MenuItem::with_id(
            format!("{ID_RECENT_OPEN_PREFIX}{index}"),
            "Open",
            true,
            None,
        );
        let show_item = MenuItem::with_id(
            format!("{ID_RECENT_SHOW_PREFIX}{index}"),
            "Show in folder",
            true,
            None,
        );
        let entry = Submenu::with_items(label, true, &[&open_item, &show_item])
            .unwrap_or_else(|_| Submenu::new(label, true));
        open.push(open_item);
        show.push(show_item);
        entries.push(entry);
    }
    RecentBits {
        open,
        show,
        entries,
        empty: None,
    }
}

struct MenuBits<'a> {
    toggle: &'a MenuItem,
    pause: &'a MenuItem,
    q720_24: &'a CheckMenuItem,
    q720: &'a CheckMenuItem,
    q1080: &'a CheckMenuItem,
    q1080_60: &'a CheckMenuItem,
    source_primary: &'a CheckMenuItem,
    source_displays: &'a [CheckMenuItem],
    source_foreground: &'a CheckMenuItem,
    profile_work: &'a CheckMenuItem,
    profile_game: &'a CheckMenuItem,
    profile_silent: &'a CheckMenuItem,
    output: &'a MenuItem,
    cursor: &'a CheckMenuItem,
    open: &'a MenuItem,
    recent_entries: &'a [Submenu],
    recent_empty: Option<&'a MenuItem>,
    stream: &'a CheckMenuItem,
    stream_view: &'a MenuItem,
    show_bar: &'a MenuItem,
    exit: &'a MenuItem,
}

fn assemble_menu(bits: &MenuBits<'_>) -> Result<Menu, tray_icon::menu::Error> {
    let quality = Submenu::with_items(
        "Quality",
        true,
        &[bits.q720_24, bits.q720, bits.q1080, bits.q1080_60],
    )?;
    let profile = Submenu::with_items(
        "Profile",
        true,
        &[bits.profile_work, bits.profile_game, bits.profile_silent],
    )?;
    let mut source_refs: Vec<&dyn IsMenuItem> = vec![bits.source_primary];
    for item in bits.source_displays {
        source_refs.push(item);
    }
    source_refs.push(bits.source_foreground);
    let source = Submenu::with_items("Capture source", true, &source_refs)?;
    let mut recent_refs: Vec<&dyn IsMenuItem> = Vec::new();
    if let Some(empty) = bits.recent_empty {
        recent_refs.push(empty);
    }
    for entry in bits.recent_entries {
        recent_refs.push(entry);
    }
    let recent = Submenu::with_items("Recent", true, &recent_refs)?;
    Menu::with_items(&[
        bits.toggle,
        bits.pause,
        bits.show_bar,
        &PredefinedMenuItem::separator(),
        &quality,
        &profile,
        &source,
        bits.output,
        bits.cursor,
        bits.stream,
        bits.stream_view,
        &PredefinedMenuItem::separator(),
        bits.open,
        &recent,
        &PredefinedMenuItem::separator(),
        bits.exit,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixed_ids() {
        assert_eq!(parse_menu_id(ID_TOGGLE), Some(Command::Toggle));
        assert_eq!(parse_menu_id(ID_PAUSE), Some(Command::Pause));
        assert_eq!(
            parse_menu_id(ID_QUALITY_1080),
            Some(Command::Quality(QualitySetting::P1080p30))
        );
        assert_eq!(
            parse_menu_id(ID_SOURCE_PRIMARY),
            Some(Command::Source(SourceSetting::Primary))
        );
        assert_eq!(parse_menu_id(ID_EXIT), Some(Command::Exit));
    }

    #[test]
    fn maps_hotkey_ids() {
        assert_eq!(parse_hotkey_id(1, 1, 2), Some(Command::Toggle));
        assert_eq!(parse_hotkey_id(2, 1, 2), Some(Command::Pause));
        assert_eq!(parse_hotkey_id(9, 1, 2), None);
    }

    #[test]
    fn parses_display_index() {
        assert_eq!(
            parse_menu_id("source-display-2"),
            Some(Command::Source(SourceSetting::Display {
                index: 2,
                id: String::new(),
                name: String::new()
            }))
        );
    }

    #[test]
    fn parses_profile_and_recent() {
        assert_eq!(
            parse_menu_id(ID_PROFILE_GAME),
            Some(Command::Profile(ProfileId::Game))
        );
        assert_eq!(parse_menu_id("recent-open-0"), Some(Command::OpenRecent(0)));
        assert_eq!(parse_menu_id("recent-show-3"), Some(Command::ShowRecent(3)));
        assert_eq!(parse_menu_id(ID_STREAM), Some(Command::ToggleStream));
        assert_eq!(
            parse_menu_id(ID_STREAM_VIEW),
            Some(Command::OpenStreamViewer)
        );
        assert_eq!(
            parse_menu_id(ID_QUALITY_720_24),
            Some(Command::Quality(QualitySetting::P720p24))
        );
    }

    #[test]
    fn unknown_id_is_none() {
        assert_eq!(parse_menu_id("nope"), None);
    }
}
