use lightcapture_core::CaptureDisplay;
use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::settings::{QualitySetting, Settings, SourceSetting};
use crate::status;

pub const ID_TOGGLE: &str = "toggle";
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Quality(QualitySetting),
    Source(SourceSetting),
    PickFolder,
    ToggleCursor,
    OpenFolder,
    Exit,
}

#[must_use]
pub fn parse_menu_id(id: &str) -> Option<Command> {
    match id {
        ID_TOGGLE => Some(Command::Toggle),
        ID_QUALITY_720 => Some(Command::Quality(QualitySetting::P720p30)),
        ID_QUALITY_1080 => Some(Command::Quality(QualitySetting::P1080p30)),
        ID_QUALITY_1080_60 => Some(Command::Quality(QualitySetting::P1080p60)),
        ID_SOURCE_PRIMARY => Some(Command::Source(SourceSetting::Primary)),
        ID_SOURCE_FOREGROUND => Some(Command::Source(SourceSetting::Foreground)),
        ID_OUTPUT_FOLDER => Some(Command::PickFolder),
        ID_SHOW_CURSOR => Some(Command::ToggleCursor),
        ID_OPEN_FOLDER => Some(Command::OpenFolder),
        ID_EXIT => Some(Command::Exit),
        other => other
            .strip_prefix(ID_SOURCE_DISPLAY_PREFIX)
            .and_then(|rest| rest.parse().ok())
            .map(|index| Command::Source(SourceSetting::Display { index })),
    }
}

pub struct TrayUi {
    tray: TrayIcon,
    toggle: MenuItem,
    q720: CheckMenuItem,
    q1080: CheckMenuItem,
    q1080_60: CheckMenuItem,
    source_primary: CheckMenuItem,
    source_foreground: CheckMenuItem,
    source_displays: Vec<CheckMenuItem>,
    output: MenuItem,
    cursor: CheckMenuItem,
    open: MenuItem,
    exit: MenuItem,
    idle_icon: Icon,
    recording_icon: Icon,
}

impl TrayUi {
    pub fn new(
        settings: &Settings,
        displays: &[CaptureDisplay],
        hotkey_ok: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let idle_icon = status::idle_icon();
        let recording_icon = status::recording_icon();
        let toggle = MenuItem::with_id(ID_TOGGLE, "Start recording", true, None);
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
        let output = MenuItem::with_id(ID_OUTPUT_FOLDER, "Output folder…", true, None);
        let cursor = CheckMenuItem::with_id(
            ID_SHOW_CURSOR,
            "Show cursor",
            true,
            settings.include_cursor,
            None,
        );
        let open = MenuItem::with_id(ID_OPEN_FOLDER, "Open recordings folder", true, None);
        let exit = MenuItem::with_id(ID_EXIT, "Exit", true, None);
        let menu = assemble_menu(&MenuBits {
            toggle: &toggle,
            q720: &q720,
            q1080: &q1080,
            q1080_60: &q1080_60,
            source_primary: &source_primary,
            source_displays: &source_displays,
            source_foreground: &source_foreground,
            output: &output,
            cursor: &cursor,
            open: &open,
            exit: &exit,
        })?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(status::tooltip(None, None, hotkey_ok))
            .with_icon(idle_icon.clone())
            .with_title("LightCapture")
            .build()?;
        Ok(Self {
            tray,
            toggle,
            q720,
            q1080,
            q1080_60,
            source_primary,
            source_foreground,
            source_displays,
            output,
            cursor,
            open,
            exit,
            idle_icon,
            recording_icon,
        })
    }

    pub fn set_recording(&self, recording: bool) {
        self.toggle.set_text(if recording {
            "Stop recording"
        } else {
            "Start recording"
        });
        let options_enabled = !recording;
        self.q720.set_enabled(options_enabled);
        self.q1080.set_enabled(options_enabled);
        self.q1080_60.set_enabled(options_enabled);
        self.source_primary.set_enabled(options_enabled);
        self.source_foreground.set_enabled(options_enabled);
        for item in &self.source_displays {
            item.set_enabled(options_enabled);
        }
        self.output.set_enabled(options_enabled);
        self.cursor.set_enabled(options_enabled);
        let icon = if recording {
            &self.recording_icon
        } else {
            &self.idle_icon
        };
        let _ = self.tray.set_icon(Some(icon.clone()));
    }

    pub fn set_tooltip(&self, text: impl AsRef<str>) {
        let _ = self.tray.set_tooltip(Some(text.as_ref()));
    }

    pub fn apply_settings(&self, settings: &Settings) {
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
                SourceSetting::Display { index } => {
                    item.id().as_ref() == format!("{ID_SOURCE_DISPLAY_PREFIX}{index}")
                }
                _ => false,
            };
            item.set_checked(checked);
        }
        self.cursor.set_checked(settings.include_cursor);
    }

    pub fn refresh_displays(
        &mut self,
        displays: &[CaptureDisplay],
        settings: &Settings,
        recording: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.source_displays = display_items(displays, &settings.source);
        let enabled = !recording;
        self.source_primary.set_enabled(enabled);
        self.source_foreground.set_enabled(enabled);
        for item in &self.source_displays {
            item.set_enabled(enabled);
        }
        let menu = assemble_menu(&MenuBits {
            toggle: &self.toggle,
            q720: &self.q720,
            q1080: &self.q1080,
            q1080_60: &self.q1080_60,
            source_primary: &self.source_primary,
            source_displays: &self.source_displays,
            source_foreground: &self.source_foreground,
            output: &self.output,
            cursor: &self.cursor,
            open: &self.open,
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
            let checked =
                matches!(source, SourceSetting::Display { index } if *index == display.index);
            CheckMenuItem::with_id(id, status::display_label(display), true, checked, None)
        })
        .collect()
}

struct MenuBits<'a> {
    toggle: &'a MenuItem,
    q720: &'a CheckMenuItem,
    q1080: &'a CheckMenuItem,
    q1080_60: &'a CheckMenuItem,
    source_primary: &'a CheckMenuItem,
    source_displays: &'a [CheckMenuItem],
    source_foreground: &'a CheckMenuItem,
    output: &'a MenuItem,
    cursor: &'a CheckMenuItem,
    open: &'a MenuItem,
    exit: &'a MenuItem,
}

fn assemble_menu(bits: &MenuBits<'_>) -> Result<Menu, tray_icon::menu::Error> {
    let quality = Submenu::with_items("Quality", true, &[bits.q720, bits.q1080, bits.q1080_60])?;
    let mut source_refs: Vec<&dyn IsMenuItem> = vec![bits.source_primary];
    for item in bits.source_displays {
        source_refs.push(item);
    }
    source_refs.push(bits.source_foreground);
    let source = Submenu::with_items("Capture source", true, &source_refs)?;
    Menu::with_items(&[
        bits.toggle,
        &PredefinedMenuItem::separator(),
        &quality,
        &source,
        bits.output,
        bits.cursor,
        &PredefinedMenuItem::separator(),
        bits.open,
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
    fn parses_display_index() {
        assert_eq!(
            parse_menu_id("source-display-2"),
            Some(Command::Source(SourceSetting::Display { index: 2 }))
        );
    }

    #[test]
    fn unknown_id_is_none() {
        assert_eq!(parse_menu_id("nope"), None);
    }
}
