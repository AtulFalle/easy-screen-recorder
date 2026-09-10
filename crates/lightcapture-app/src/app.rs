use std::fs;
use std::process::Command as ProcessCommand;
use std::time::Duration;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use lightcapture_core::{list_displays, start, RecordConfig, Recording};
use tray_icon::menu::MenuEvent;

use crate::pump;
use crate::settings::Settings;
use crate::status;
use crate::tray::{parse_menu_id, Command, TrayUi};

pub fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pump::init_com();

    let mut settings = Settings::load();
    let hotkey_manager = GlobalHotKeyManager::new()?;
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyR);
    let hotkey_ok = match hotkey_manager.register(hotkey) {
        Ok(()) => true,
        Err(err) => {
            eprintln!("could not register Ctrl+Shift+R: {err}");
            false
        }
    };
    let displays = list_displays().unwrap_or_default();
    let mut tray = TrayUi::new(&settings, &displays, hotkey_ok)?;
    let mut recording: Option<Recording> = None;
    let mut error: Option<String> = None;

    loop {
        if !pump::wait_and_pump(Duration::from_millis(50)) {
            break;
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(command) = parse_menu_id(event.id.as_ref()) {
                handle_command(
                    command,
                    &mut settings,
                    &mut recording,
                    &mut error,
                    &mut tray,
                    hotkey_ok,
                )?;
            }
        }

        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed {
                handle_command(
                    Command::Toggle,
                    &mut settings,
                    &mut recording,
                    &mut error,
                    &mut tray,
                    hotkey_ok,
                )?;
            }
        }

        let stats = recording.as_ref().map(Recording::stats);
        tray.set_tooltip(status::tooltip(stats.as_ref(), error.as_deref(), hotkey_ok));
    }

    if let Some(active) = recording.take() {
        if let Err(err) = active.stop() {
            eprintln!("{err}");
        }
    }
    Ok(())
}

fn handle_command(
    command: Command,
    settings: &mut Settings,
    recording: &mut Option<Recording>,
    error: &mut Option<String>,
    tray: &mut TrayUi,
    hotkey_ok: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let is_recording = recording.is_some();
    match command {
        Command::Toggle => toggle(settings, recording, error, tray)?,
        Command::Exit => {
            if let Some(active) = recording.take() {
                if let Err(err) = active.stop() {
                    eprintln!("{err}");
                }
            }
            tray.set_recording(false);
            pump::request_quit();
        }
        Command::OpenFolder => open_folder(&settings.output_dir),
        Command::Quality(_) | Command::Source(_) | Command::PickFolder | Command::ToggleCursor
            if is_recording => {}
        Command::Quality(quality) => {
            settings.quality = quality;
            persist(settings, tray);
        }
        Command::Source(source) => {
            settings.source = source;
            persist(settings, tray);
        }
        Command::ToggleCursor => {
            settings.include_cursor = !settings.include_cursor;
            persist(settings, tray);
        }
        Command::PickFolder => {
            if let Some(dir) = rfd::FileDialog::new()
                .set_directory(&settings.output_dir)
                .pick_folder()
            {
                settings.output_dir = dir;
                persist(settings, tray);
            }
        }
    }
    let stats = recording.as_ref().map(Recording::stats);
    tray.set_tooltip(status::tooltip(stats.as_ref(), error.as_deref(), hotkey_ok));
    Ok(())
}

fn persist(settings: &Settings, tray: &TrayUi) {
    if let Err(err) = settings.save() {
        eprintln!("could not save settings: {err}");
    }
    tray.apply_settings(settings);
}

fn toggle(
    settings: &Settings,
    recording: &mut Option<Recording>,
    error: &mut Option<String>,
    tray: &mut TrayUi,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(active) = recording.take() {
        match active.stop() {
            Ok(path) => {
                error.take();
                eprintln!("saved {}", path.display());
            }
            Err(err) => {
                *error = Some(err.to_string());
                eprintln!("{err}");
            }
        }
        tray.set_recording(false);
        let displays = list_displays().unwrap_or_default();
        let _ = tray.refresh_displays(&displays, settings, false);
        return Ok(());
    }

    if let Err(err) = fs::create_dir_all(&settings.output_dir) {
        *error = Some(err.to_string());
        eprintln!("{err}");
        return Ok(());
    }
    let output = RecordConfig::default_output_in(&settings.output_dir);
    let mut config = RecordConfig::new(output);
    config.target = settings.source.to_target();
    config.quality = settings.quality.into();
    config.include_cursor = settings.include_cursor;
    match start(config) {
        Ok(active) => {
            error.take();
            *recording = Some(active);
            tray.set_recording(true);
        }
        Err(err) => {
            *error = Some(err.to_string());
            eprintln!("{err}");
            tray.set_recording(false);
        }
    }
    Ok(())
}

fn open_folder(dir: &std::path::Path) {
    if let Err(err) = fs::create_dir_all(dir) {
        eprintln!("could not create recordings folder: {err}");
        return;
    }
    if let Err(err) = ProcessCommand::new("explorer").arg(dir).spawn() {
        eprintln!("could not open recordings folder: {err}");
    }
}
