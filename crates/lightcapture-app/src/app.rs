use std::fs;
use std::process::Command as ProcessCommand;
use std::time::Duration;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use lightcapture_core::{
    list_displays, publish_file, start, view_url, Error, RecordConfig, Recording,
};
use tray_icon::menu::MenuEvent;

use crate::notify;
use crate::pump;
use crate::settings::{Settings, SourceSetting};
use crate::status::{self, HotkeyStatus};
use crate::tray::{parse_hotkey_id, parse_menu_id, Command, TrayUi};

pub fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pump::init_com();

    let mut settings = Settings::load();
    let hotkey_manager = GlobalHotKeyManager::new()?;
    let record_hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyR);
    let pause_hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyP);
    let hotkeys = HotkeyStatus {
        record: match hotkey_manager.register(record_hotkey) {
            Ok(()) => true,
            Err(err) => {
                eprintln!("could not register Ctrl+Shift+R: {err}");
                false
            }
        },
        pause: match hotkey_manager.register(pause_hotkey) {
            Ok(()) => true,
            Err(err) => {
                eprintln!("could not register Ctrl+Shift+P: {err}");
                false
            }
        },
    };
    let displays = list_displays().unwrap_or_default();
    let mut tray = TrayUi::new(&settings, &displays, hotkeys)?;
    let mut recording: Option<Recording> = None;
    let mut error: Option<String> = None;

    loop {
        if !pump::wait_and_pump(Duration::from_millis(50)) {
            break;
        }

        if recording.as_ref().is_some_and(Recording::has_failed) {
            let _ = handle_command(
                Command::Toggle,
                &mut settings,
                &mut recording,
                &mut error,
                &mut tray,
                hotkeys,
            );
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(command) = parse_menu_id(event.id.as_ref()) {
                handle_command(
                    command,
                    &mut settings,
                    &mut recording,
                    &mut error,
                    &mut tray,
                    hotkeys,
                )?;
            }
        }

        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed {
                if let Some(command) =
                    parse_hotkey_id(event.id, record_hotkey.id(), pause_hotkey.id())
                {
                    handle_command(
                        command,
                        &mut settings,
                        &mut recording,
                        &mut error,
                        &mut tray,
                        hotkeys,
                    )?;
                }
            }
        }

        refresh_tooltip(&tray, &recording, &error, hotkeys);
    }

    if let Some(active) = recording.take() {
        let path = active.output().to_path_buf();
        match active.stop() {
            Ok(_) => {}
            Err(err) => {
                eprintln!("{err}");
                if path.is_file() {
                    eprintln!("kept {}", path.display());
                }
            }
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
    hotkeys: HotkeyStatus,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let is_recording = recording.is_some();
    match command {
        Command::Toggle => toggle(settings, recording, error, tray)?,
        Command::Pause => {
            if let Some(active) = recording.as_ref() {
                let paused = !active.is_paused();
                active.set_paused(paused);
                tray.set_paused(paused);
            }
        }
        Command::Exit => {
            if let Some(active) = recording.take() {
                let path = active.output().to_path_buf();
                if let Err(err) = active.stop() {
                    eprintln!("{err}");
                    if path.is_file() {
                        eprintln!("kept {}", path.display());
                    }
                }
            }
            tray.set_recording(false);
            pump::request_quit();
        }
        Command::OpenFolder => open_folder(&settings.output_dir),
        Command::OpenRecent(index) => {
            if let Some(path) = settings.recent.get(index) {
                notify::open_file(path);
            }
        }
        Command::ShowRecent(index) => {
            if let Some(path) = settings.recent.get(index) {
                notify::show_in_folder(path);
            }
        }
        Command::ToggleStream => {
            settings.stream_enabled = !settings.stream_enabled;
            persist(settings, tray);
        }
        Command::OpenStreamViewer => open_stream_viewer(&settings.stream_url),
        Command::Quality(_)
        | Command::Source(_)
        | Command::PickFolder
        | Command::ToggleCursor
        | Command::Profile(_)
            if is_recording => {}
        Command::Quality(quality) => {
            settings.quality = quality;
            persist(settings, tray);
        }
        Command::Source(source) => {
            settings.source = resolve_source(source);
            persist(settings, tray);
        }
        Command::Profile(profile) => {
            profile.apply(settings);
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
        Command::SetAudio(_) if is_recording => {}
        Command::SetAudio(audio) => {
            settings.audio = audio;
            persist(settings, tray);
        }
        Command::ShowBar => {}
    }
    refresh_tooltip(tray, recording, error, hotkeys);
    Ok(())
}

fn resolve_source(source: SourceSetting) -> SourceSetting {
    match source {
        SourceSetting::Display { index, .. } => {
            let displays = list_displays().unwrap_or_default();
            if let Some(display) = displays.iter().find(|d| d.index == index) {
                SourceSetting::Display {
                    index,
                    id: display.device_id.clone(),
                    name: display.name.clone(),
                }
            } else {
                SourceSetting::Display {
                    index,
                    id: String::new(),
                    name: String::new(),
                }
            }
        }
        other => other,
    }
}

fn refresh_tooltip(
    tray: &TrayUi,
    recording: &Option<Recording>,
    error: &Option<String>,
    hotkeys: HotkeyStatus,
) {
    let stats = recording.as_ref().map(Recording::stats);
    let paused = recording.as_ref().is_some_and(Recording::is_paused);
    tray.set_tooltip(status::tooltip(
        stats.as_ref(),
        error.as_deref(),
        paused,
        hotkeys,
    ));
}

fn persist(settings: &Settings, tray: &TrayUi) {
    if let Err(err) = settings.save() {
        eprintln!("could not save settings: {err}");
    }
    tray.apply_settings(settings);
}

fn persist_and_refresh_menu(settings: &Settings, tray: &mut TrayUi, recording: bool) {
    if let Err(err) = settings.save() {
        eprintln!("could not save settings: {err}");
    }
    tray.apply_settings(settings);
    let displays = list_displays().unwrap_or_default();
    let _ = tray.refresh_menu(&displays, settings, recording);
}

fn toggle(
    settings: &mut Settings,
    recording: &mut Option<Recording>,
    error: &mut Option<String>,
    tray: &mut TrayUi,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(active) = recording.take() {
        finish_recording(active, settings, error);
        tray.set_recording(false);
        persist_and_refresh_menu(settings, tray, false);
        return Ok(());
    }

    if let Err(err) = fs::create_dir_all(&settings.output_dir) {
        *error = Some(err.to_string());
        eprintln!("{err}");
        return Ok(());
    }
    let displays = list_displays().unwrap_or_default();
    let output = RecordConfig::output_path(&settings.output_dir, &settings.source.source_slug());
    let mut config = RecordConfig::new(output);
    config.target = settings.source.resolve(&displays);
    config.quality = settings.quality.into();
    config.include_cursor = settings.include_cursor;
    config.audio = settings.audio.into();
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

fn finish_recording(active: Recording, settings: &mut Settings, error: &mut Option<String>) {
    let path = active.output().to_path_buf();
    let failed = active.failure();
    match active.stop() {
        Ok(saved) => {
            error.take();
            settings.push_recent(saved.clone());
            eprintln!("saved {}", saved.display());
            notify::recording_saved(&saved);
            maybe_publish(settings, &saved);
        }
        Err(err) => {
            let kept = path.is_file();
            if kept {
                settings.push_recent(path.clone());
            }
            let message = failure_message(&err, &path, kept);
            *error = Some(message.clone());
            eprintln!("{message}");
            if kept {
                match &err {
                    Error::DiskFull => {
                        notify::recording_kept("Disk full — kept partial file", &path)
                    }
                    Error::EncoderLost => {
                        notify::recording_kept("Encoder lost — kept partial file", &path)
                    }
                    _ => notify::recording_kept("Recording stopped", &path),
                }
                maybe_publish(settings, &path);
            }
            if let Some(reason) = failed {
                eprintln!("{reason}");
            }
        }
    }
}

fn failure_message(err: &Error, path: &std::path::Path, kept: bool) -> String {
    match err {
        Error::DiskFull if kept => format!("Disk full — kept {}", path.display()),
        Error::EncoderLost if kept => format!("Encoder lost — kept {}", path.display()),
        other => other.to_string(),
    }
}

fn maybe_publish(settings: &Settings, path: &std::path::Path) {
    if !settings.stream_enabled {
        return;
    }
    let path = path.to_path_buf();
    let url = settings.stream_url.clone();
    std::thread::spawn(move || match publish_file(&path, &url) {
        Ok(()) => eprintln!("published {} to {url}", path.display()),
        Err(err) => eprintln!("{err}"),
    });
}

fn open_stream_viewer(url: &str) {
    let view = view_url(url);
    if let Err(err) = ProcessCommand::new("cmd")
        .args(["/C", "start", "", &view])
        .spawn()
    {
        eprintln!("could not open stream viewer: {err}");
    }
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
