use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
    VideoSettingsSubType,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use crate::config::{default_bitrate_bps, even_dimension, CaptureTarget, RecordConfig};
use crate::frame_gate::FrameGate;
use crate::hardware::{pick_encoder, EncoderKind, HardwareInfo};
use crate::platform::types::{CaptureDisplay, CaptureWindow, Recording};
use crate::stats::StatsInner;
use crate::Error;

#[derive(Clone)]
struct EncoderParams {
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
    path: PathBuf,
    stats: Arc<StatsInner>,
    paused: Arc<AtomicBool>,
}

struct CaptureHandler {
    encoder: Option<VideoEncoder>,
    gate: FrameGate,
    stats: Arc<StatsInner>,
    paused: Arc<AtomicBool>,
}

impl CaptureHandler {
    fn finish_encoder(&mut self) -> crate::Result<()> {
        if let Some(encoder) = self.encoder.take() {
            encoder
                .finish()
                .map_err(|e| Error::Encoder(e.to_string()))?;
        }
        Ok(())
    }
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = EncoderParams;
    type Error = Error;

    fn new(ctx: Context<Self::Flags>) -> std::result::Result<Self, Self::Error> {
        let params = ctx.flags;
        params.stats.width.store(params.width, Ordering::Relaxed);
        params.stats.height.store(params.height, Ordering::Relaxed);
        let encoder = VideoEncoder::new(
            VideoSettingsBuilder::new(params.width, params.height)
                .sub_type(VideoSettingsSubType::H264)
                .frame_rate(params.fps)
                .bitrate(params.bitrate),
            AudioSettingsBuilder::default().disabled(true),
            ContainerSettingsBuilder::default(),
            &params.path,
        )
        .map_err(|e| Error::Encoder(e.to_string()))?;
        Ok(Self {
            encoder: Some(encoder),
            gate: FrameGate::recording(params.fps),
            stats: params.stats,
            paused: params.paused,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        _capture_control: InternalCaptureControl,
    ) -> std::result::Result<(), Self::Error> {
        self.stats.frames_captured.fetch_add(1, Ordering::Relaxed);
        if self.paused.load(Ordering::Relaxed) {
            return Ok(());
        }
        if !self.gate.try_accept(Instant::now()) {
            self.stats
                .frames_dropped
                .store(self.gate.dropped(), Ordering::Relaxed);
            return Ok(());
        }
        let Some(encoder) = self.encoder.as_mut() else {
            self.gate.release();
            return Ok(());
        };
        let send = encoder.send_frame(frame);
        self.gate.release();
        match send {
            Ok(()) => {
                self.stats.frames_encoded.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .frames_dropped
                    .store(self.gate.dropped(), Ordering::Relaxed);
                Ok(())
            }
            Err(e) => Err(Error::Encoder(e.to_string())),
        }
    }
}

pub fn probe() -> crate::Result<HardwareInfo> {
    let displays = list_displays()?
        .into_iter()
        .map(|d| format!("{} ({}x{})", d.name, d.width, d.height))
        .collect();
    let mut encoder_names = Vec::new();
    if let Ok(monitors) = Monitor::enumerate() {
        for monitor in monitors {
            if let Ok(gpu) = monitor.device_string() {
                if !gpu.is_empty() && !encoder_names.iter().any(|n| n == &gpu) {
                    encoder_names.push(gpu);
                }
            }
        }
    }
    let selected = if encoder_names.is_empty() {
        EncoderKind::Software
    } else {
        pick_encoder(&encoder_names)
    };
    Ok(HardwareInfo {
        displays,
        encoder_names,
        selected,
    })
}

pub fn list_displays() -> crate::Result<Vec<CaptureDisplay>> {
    let monitors = Monitor::enumerate().map_err(|e| Error::Capture(e.to_string()))?;
    let mut out = Vec::with_capacity(monitors.len());
    for monitor in monitors {
        let index = monitor.index().map_err(|e| Error::Capture(e.to_string()))?;
        let name = monitor
            .name()
            .or_else(|_| monitor.device_name())
            .unwrap_or_else(|_| format!("Display {index}"));
        let width = monitor.width().map_err(|e| Error::Capture(e.to_string()))?;
        let height = monitor
            .height()
            .map_err(|e| Error::Capture(e.to_string()))?;
        out.push(CaptureDisplay {
            index,
            name,
            width,
            height,
        });
    }
    Ok(out)
}

pub fn list_windows() -> crate::Result<Vec<CaptureWindow>> {
    let windows = Window::enumerate().map_err(|e| Error::Capture(e.to_string()))?;
    let mut out = Vec::new();
    for window in windows {
        if !window.is_valid() {
            continue;
        }
        let Ok(title) = window.title() else {
            continue;
        };
        if title.trim().is_empty() {
            continue;
        }
        let width = window.width().unwrap_or(0).max(0) as u32;
        let height = window.height().unwrap_or(0).max(0) as u32;
        out.push(CaptureWindow {
            title,
            width,
            height,
        });
    }
    Ok(out)
}

pub fn start(config: RecordConfig) -> crate::Result<Recording> {
    if let Some(parent) = config.output.parent() {
        if parent.components().next().is_some() {
            fs::create_dir_all(parent)?;
        }
    }
    match &config.target {
        CaptureTarget::PrimaryDisplay => {
            let monitor = Monitor::primary().map_err(|e| Error::TargetNotFound(e.to_string()))?;
            start_monitor(monitor, config)
        }
        CaptureTarget::DisplayIndex(index) => {
            if *index < 1 {
                return Err(Error::InvalidTarget(
                    "display index is one-based (1 is the first monitor)".into(),
                ));
            }
            let monitor =
                Monitor::from_index(*index).map_err(|e| Error::TargetNotFound(e.to_string()))?;
            start_monitor(monitor, config)
        }
        CaptureTarget::WindowTitle(title) => {
            let window = Window::from_contains_name(title)
                .map_err(|e| Error::TargetNotFound(e.to_string()))?;
            start_window(window, config)
        }
        CaptureTarget::ForegroundWindow => {
            let window = Window::foreground().map_err(|e| Error::TargetNotFound(e.to_string()))?;
            start_window(window, config)
        }
    }
}

fn start_monitor(monitor: Monitor, config: RecordConfig) -> crate::Result<Recording> {
    let width = even_dimension(monitor.width().map_err(|e| Error::Capture(e.to_string()))?);
    let height = even_dimension(
        monitor
            .height()
            .map_err(|e| Error::Capture(e.to_string()))?,
    );
    begin(monitor, width, height, config)
}

fn start_window(window: Window, config: RecordConfig) -> crate::Result<Recording> {
    let width = even_dimension(window.width().unwrap_or(0).max(0) as u32);
    let height = even_dimension(window.height().unwrap_or(0).max(0) as u32);
    begin(window, width, height, config)
}

fn begin<T>(item: T, width: u32, height: u32, config: RecordConfig) -> crate::Result<Recording>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
{
    if width < 16 || height < 16 {
        return Err(Error::InvalidTarget(
            "capture surface is smaller than 16x16".into(),
        ));
    }
    let fps = config.quality.fps();
    let stats = StatsInner::new(fps);
    let paused = Arc::new(AtomicBool::new(false));
    let params = EncoderParams {
        width,
        height,
        fps,
        bitrate: default_bitrate_bps(width, height, fps),
        path: config.output.clone(),
        stats: Arc::clone(&stats),
        paused: Arc::clone(&paused),
    };
    let cursor = if config.include_cursor {
        CursorCaptureSettings::WithCursor
    } else {
        CursorCaptureSettings::WithoutCursor
    };
    let settings = Settings::new(
        item,
        cursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Custom(Duration::from_millis(
            (1000 / u64::from(fps.max(1))).max(1),
        )),
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        params,
    );
    let control =
        CaptureHandler::start_free_threaded(settings).map_err(|e| Error::Capture(e.to_string()))?;
    let output = config.output.clone();
    let stopper = Box::new(move || {
        {
            let callback = control.callback();
            let mut handler = callback.lock();
            handler.finish_encoder()?;
        }
        control.stop().map_err(|e| Error::Capture(e.to_string()))
    });
    Ok(Recording {
        output,
        stats,
        paused,
        stopper: Some(stopper),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    #[ignore = "requires an interactive Windows desktop session"]
    fn records_primary_display_briefly() {
        let dir = std::env::temp_dir();
        let output = dir.join("lightcapture-hw-smoke.mp4");
        let _ = fs::remove_file(&output);
        let mut config = RecordConfig::new(&output);
        config.quality = crate::Quality::P720p30;
        let recording = start(config).expect("start");
        std::thread::sleep(Duration::from_secs(2));
        let path = recording.stop().expect("stop");
        assert!(path.is_file());
        assert!(fs::metadata(&path).expect("meta").len() > 0);
    }
}
