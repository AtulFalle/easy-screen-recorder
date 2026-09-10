use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use lightcapture_core::{
    list_displays, list_windows, probe, publish_file, start, AudioConfig, CaptureTarget,
    RecordConfig,
};

use crate::args::{Cli, Command, RecordArgs};

pub fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match cli.command {
        Command::Probe => probe_cmd(),
        Command::Windows => windows_cmd(),
        Command::Record(args) => record_cmd(args),
    }
}

fn probe_cmd() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let info = probe()?;
    println!("LightCapture {}", lightcapture_core::version());
    println!("Selected encoder: {}", info.selected.as_str());
    if info.encoder_names.is_empty() {
        println!("GPUs: (none reported)");
    } else {
        println!("GPUs:");
        for name in &info.encoder_names {
            println!("  - {name}");
        }
    }
    println!("Displays:");
    if info.displays.is_empty() {
        println!("  (none)");
    } else {
        for display in &info.displays {
            println!("  - {display}");
        }
    }
    let listed = list_displays()?;
    if !listed.is_empty() {
        println!("Display indexes:");
        for display in listed {
            println!(
                "  [{}] {} ({}) ({}x{})",
                display.index, display.name, display.device_id, display.width, display.height
            );
        }
    }
    Ok(())
}

fn windows_cmd() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for window in list_windows()? {
        println!("{} ({}x{})", window.title, window.width, window.height);
    }
    Ok(())
}

fn record_cmd(args: RecordArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let target = if args.foreground {
        CaptureTarget::ForegroundWindow
    } else if let Some(title) = args.window {
        CaptureTarget::WindowTitle(title)
    } else if let Some(index) = args.display {
        CaptureTarget::DisplayIndex(index)
    } else {
        CaptureTarget::PrimaryDisplay
    };
    let output = args.output.unwrap_or_else(|| {
        RecordConfig::output_path(
            &std::env::current_dir().unwrap_or_default(),
            &target.source_slug(),
        )
    });
    let mut config = RecordConfig::new(output.clone());
    config.target = target;
    config.quality = args.quality.into();
    config.include_cursor = !args.no_cursor;
    config.audio = AudioConfig {
        system: !args.no_system_audio,
        microphone: !args.no_mic,
    };

    eprintln!("recording to {}", config.output.display());
    let recording = start(config)?;
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = Arc::clone(&running);
        ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);
        })?;
    }
    let started = Instant::now();
    let limit = args.duration.map(Duration::from_secs);
    while running.load(Ordering::SeqCst) {
        if limit.is_some_and(|d| started.elapsed() >= d) {
            break;
        }
        if recording.has_failed() {
            break;
        }
        thread::sleep(Duration::from_millis(250));
        let stats = recording.stats();
        eprint!(
            "\r{:>3}s  {}x{}  {}  {}fps  encoded {}  dropped {}  captured {}",
            stats.elapsed_secs,
            stats.width,
            stats.height,
            stats.encoder.as_short(),
            stats.fps_target,
            stats.frames_encoded,
            stats.frames_dropped,
            stats.frames_captured
        );
        let _ = io::stderr().flush();
    }
    eprintln!();
    let path = recording.output().to_path_buf();
    match recording.stop() {
        Ok(saved) => {
            eprintln!("saved {}", saved.display());
            if let Some(url) = args.stream_url {
                match publish_file(&saved, &url) {
                    Ok(()) => eprintln!("published {} to {url}", saved.display()),
                    Err(err) => eprintln!("{err}"),
                }
            }
            Ok(())
        }
        Err(err) => {
            eprintln!("{err}");
            if path.is_file() {
                eprintln!("kept {}", path.display());
            }
            Err(err.into())
        }
    }
}
