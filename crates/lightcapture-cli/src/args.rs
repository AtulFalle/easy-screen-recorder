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

    /// Stop after this many seconds. Omit to wait for Ctrl+C.
    #[arg(long)]
    pub duration: Option<u64>,

    /// Hide the mouse cursor in the recording.
    #[arg(long)]
    pub no_cursor: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum QualityArg {
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
            QualityArg::P720p30 => Self::P720p30,
            QualityArg::P1080p30 => Self::P1080p30,
            QualityArg::P1080p60 => Self::P1080p60,
        }
    }
}
