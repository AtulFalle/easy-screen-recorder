use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use lightcapture_core::Quality;

#[derive(Debug, Parser)]
#[command(name = "lightcapture-cli", version = lightcapture_core::version(), about = "LightCapture headless recorder")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List displays, GPU names, and the encoder preference.
    Probe,
    /// List capturable windows.
    Windows,
    /// Record the screen or a window to an MP4 file.
    Record(RecordArgs),
}

#[derive(Debug, clap::Args)]
pub struct RecordArgs {
    /// Output MP4 path. Default: ./lightcapture-<unix-time>.mp4
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// One-based display index. Default: primary display.
    #[arg(long)]
    pub display: Option<usize>,

    /// Capture a window whose title contains this text.
    #[arg(long, conflicts_with = "display")]
    pub window: Option<String>,

    /// Record the foreground window.
    #[arg(long, conflicts_with_all = ["display", "window"])]
    pub foreground: bool,

    #[arg(long, value_enum, default_value_t = QualityArg::P1080p30)]
    pub quality: QualityArg,

    /// Remux the finished MP4 to this RTSP/RTMP URL (`ffmpeg -c copy`).
    #[arg(long, value_name = "URL")]
    pub stream_url: Option<String>,

    /// Stop after this many seconds. Omit to wait for Ctrl+C.
    #[arg(long)]
    pub duration: Option<u64>,

    /// Hide the mouse cursor in the recording.
    #[arg(long)]
    pub no_cursor: bool,

    /// Do not capture system (loopback) audio.
    #[arg(long)]
    pub no_system_audio: bool,

    /// Do not capture the microphone.
    #[arg(long)]
    pub no_mic: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum QualityArg {
    #[value(name = "720p24")]
    P720p24,
    #[value(name = "720p30")]
    P720p30,
    #[value(name = "1080p30")]
    P1080p30,
    #[value(name = "1080p60")]
    P1080p60,
}

impl From<QualityArg> for Quality {
    fn from(value: QualityArg) -> Self {
        match value {
            QualityArg::P720p24 => Self::P720p24,
            QualityArg::P720p30 => Self::P720p30,
            QualityArg::P1080p30 => Self::P1080p30,
            QualityArg::P1080p60 => Self::P1080p60,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn audio_flags_parse() {
        let cli = Cli::try_parse_from([
            "lightcapture-cli",
            "record",
            "--no-system-audio",
            "--no-mic",
        ])
        .expect("parse");
        match cli.command {
            Command::Record(args) => {
                assert!(args.no_system_audio);
                assert!(args.no_mic);
            }
            other => panic!("expected record, got {other:?}"),
        }
    }
}
