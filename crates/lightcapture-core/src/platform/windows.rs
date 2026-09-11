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

use crate::adaptive::Adaptive;
use crate::audio::{
    missing_audio_frames, silence_i16_le, start_pump, AudioPump, PcmQueue, CHUNK_FRAMES,
};
use crate::config::{
    default_bitrate_bps, even_dimension, AudioConfig, CaptureTarget, RecordConfig,
};
use crate::frame_gate::FrameGate;
use crate::hardware::{pick_encoder, EncoderKind, HardwareInfo};
use crate::platform::types::{CaptureDisplay, CaptureWindow, Recording};
use crate::stats::StatsInner;
use crate::{classify_encode_failure, Error};

mod scale;

#[derive(Clone)]
struct EncoderParams {
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
    path: PathBuf,
    stats: Arc<StatsInner>,
    paused: Arc<AtomicBool>,
    audio: AudioConfig,
}

struct CaptureHandler {
    encoder: Option<VideoEncoder>,
    gate: FrameGate,
    stats: Arc<StatsInner>,
    paused: Arc<AtomicBool>,
    pcm: Option<Arc<PcmQueue>>,
    audio: Option<AudioPump>,
    scaler: Option<scale::GpuScaler>,
    encode_width: u32,
    encode_height: u32,
    adaptive: Adaptive,
    first_video_hns: Option<i64>,
    last_video_hns: Option<i64>,
}

impl CaptureHandler {
    fn finish_encoder(&mut self) -> crate::Result<()> {
        let encoder = self.take_encoder_after_audio()?;
        if let Some(encoder) = encoder {
            encoder
                .finish()
                .map_err(|e| Error::Encoder(e.to_string()))?;
        }
        Ok(())
    }

    fn take_encoder_after_audio(&mut self) -> crate::Result<Option<VideoEncoder>> {
        if let Some(mut pump) = self.audio.take() {
            pump.stop();
        }
        self.drain_audio()?;
        self.pad_audio_to_video()?;
        Ok(self.encoder.take())
    }

    fn pad_audio_to_video(&mut self) -> crate::Result<()> {
        if self.pcm.is_none() {
            return Ok(());
        }
        let Some(encoder) = self.encoder.as_mut() else {
            return Ok(());
        };
        let (Some(first), Some(last)) = (self.first_video_hns, self.last_video_hns) else {
            return Ok(());
        };
        let video_hns = last.saturating_sub(first).max(0) as u64;
        let sent = self.stats.audio_frames_sent.load(Ordering::Relaxed);
        let mut missing = missing_audio_frames(sent, video_hns);
        while missing > 0 {
            let frames = usize::try_from(missing.min(CHUNK_FRAMES as u64)).unwrap_or(0);
            if frames == 0 {
                break;
            }
            encoder
                .send_audio_buffer(&silence_i16_le(frames), 0)
                .map_err(|e| Error::Encoder(e.to_string()))?;
            self.stats
                .audio_frames_sent
                .fetch_add(frames as u64, Ordering::Relaxed);
            missing -= frames as u64;
        }
        Ok(())
    }

    fn note_video_timestamp(&mut self, frame: &Frame<'_>) {
        let Ok(ts) = frame.timestamp() else {
            return;
        };
        if self.first_video_hns.is_none() {
            self.first_video_hns = Some(ts.Duration);
        }
        self.last_video_hns = Some(ts.Duration);
    }

    fn drain_audio(&mut self) -> crate::Result<()> {
        let Some(pcm) = &self.pcm else {
            return Ok(());
        };
        let Some(encoder) = self.encoder.as_mut() else {
            return Ok(());
        };
        for chunk in pcm.pop_all() {
            if chunk.is_empty() {
                continue;
            }
            encoder
                .send_audio_buffer(&chunk, 0)
                .map_err(|e| Error::Encoder(e.to_string()))?;
            let frames = u64::try_from(chunk.len() / 4).unwrap_or(0);
            self.stats
                .audio_frames_sent
                .fetch_add(frames, Ordering::Relaxed);
        }
        self.stats.audio_drops.store(pcm.drops(), Ordering::Relaxed);
        Ok(())
    }

    fn fail_session(&mut self, message: &str) {
        self.stats.note_failure(message);
        let _ = self.finish_encoder();
    }

    fn ensure_scaled(&mut self, frame: &Frame<'_>) -> crate::Result<()> {
        if frame.width() == self.encode_width && frame.height() == self.encode_height {
            return Ok(());
        }
        let recreate = match &self.scaler {
            Some(scaler) => !scaler.matches(
                frame.width(),
                frame.height(),
                self.encode_width,
                self.encode_height,
            ),
            None => true,
        };
        if recreate {
            self.scaler = Some(scale::GpuScaler::new(
                frame.device(),
                frame.width(),
                frame.height(),
                self.encode_width,
                self.encode_height,
                frame.desc().Format,
            )?);
        }
        let Some(scaler) = self.scaler.as_ref() else {
            return Err(Error::Encoder("GPU scaler missing".into()));
        };
        scaler.scale_into_frame(frame)
    }

    fn maybe_adapt(&mut self, now: Instant) {
        let encoded = self.stats.frames_encoded.load(Ordering::Relaxed);
        if let Some(fps) = self.adaptive.tick(now, self.gate.backpressure(), encoded) {
            self.gate.set_fps(fps);
            self.stats.fps_target.store(fps, Ordering::Relaxed);
        }
    }
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = EncoderParams;
    type Error = Error;

    fn new(ctx: Context<Self::Flags>) -> std::result::Result<Self, Self::Error> {
        let params = ctx.flags;
        params.stats.width.store(params.width, Ordering::Relaxed);
        params.stats.height.store(params.height, Ordering::Relaxed);
        let audio_setup = if params.audio.is_enabled() {
            Some(start_pump(
                params.audio.system,
                params.audio.microphone,
                Arc::clone(&params.paused),
            )?)
        } else {
            None
        };
        let audio_settings = if audio_setup.is_some() {
            AudioSettingsBuilder::new()
        } else {
            AudioSettingsBuilder::default().disabled(true)
        };
        let encoder = match VideoEncoder::new(
            VideoSettingsBuilder::new(params.width, params.height)
                .sub_type(VideoSettingsSubType::H264)
                .frame_rate(params.fps)
                .bitrate(params.bitrate),
            audio_settings,
            ContainerSettingsBuilder::default(),
            &params.path,
        ) {
            Ok(encoder) => encoder,
            Err(e) => {
                drop(audio_setup);
                return Err(Error::Encoder(e.to_string()));
            }
        };
        Ok(Self {
            encoder: Some(encoder),
            gate: FrameGate::recording(params.fps),
            stats: params.stats,
            paused: params.paused,
            pcm: audio_setup.as_ref().map(|s| Arc::clone(&s.queue)),
            audio: audio_setup.map(|s| s.pump),
            scaler: None,
            encode_width: params.width,
            encode_height: params.height,
            adaptive: Adaptive::new(params.fps),
            first_video_hns: None,
            last_video_hns: None,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        capture_control: InternalCaptureControl,
    ) -> std::result::Result<(), Self::Error> {
        self.stats.frames_captured.fetch_add(1, Ordering::Relaxed);
        if !self.paused.load(Ordering::Relaxed) {
            if let Err(err) = self.drain_audio() {
                self.fail_session(&err.to_string());
                capture_control.stop();
                return Ok(());
            }
        }
        if self.paused.load(Ordering::Relaxed) {
            return Ok(());
        }
        if !self.gate.try_accept(Instant::now()) {
            self.stats
                .frames_dropped
                .store(self.gate.dropped(), Ordering::Relaxed);
            self.maybe_adapt(Instant::now());
            return Ok(());
        }
        if let Err(err) = self.ensure_scaled(frame) {
            self.gate.release();
            self.fail_session(&err.to_string());
            capture_control.stop();
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
                self.note_video_timestamp(frame);
                self.stats.frames_encoded.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .frames_dropped
                    .store(self.gate.dropped(), Ordering::Relaxed);
                self.maybe_adapt(Instant::now());
                Ok(())
            }
            Err(e) => {
                let classified = classify_encode_failure(&e.to_string());
                self.fail_session(&classified.to_string());
                capture_control.stop();
                Ok(())
            }
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
        let device_id = monitor
            .device_name()
            .unwrap_or_else(|_| format!("display-{index}"));
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
            device_id,
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
        CaptureTarget::Display { id } => {
            let monitor = monitor_from_id(id)?;
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

fn monitor_from_id(id: &str) -> crate::Result<Monitor> {
    let monitors = Monitor::enumerate().map_err(|e| Error::Capture(e.to_string()))?;
    let mut by_name = None;
    for monitor in monitors {
        if monitor.device_name().ok().as_deref() == Some(id) {
            return Ok(monitor);
        }
        if by_name.is_none() && monitor.name().ok().as_deref() == Some(id) {
            by_name = Some(monitor);
        }
    }
    by_name.ok_or_else(|| Error::TargetNotFound(id.to_string()))
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
    let (encode_width, encode_height) = config.quality.encode_size(width, height);
    let encoder_kind = probe()
        .map(|info| info.selected)
        .unwrap_or(EncoderKind::Software);
    let stats = StatsInner::new(fps, encoder_kind);
    let paused = Arc::new(AtomicBool::new(false));
    let params = EncoderParams {
        width: encode_width,
        height: encode_height,
        fps,
        bitrate: default_bitrate_bps(encode_width, encode_height, fps),
        path: config.output.clone(),
        stats: Arc::clone(&stats),
        paused: Arc::clone(&paused),
        audio: config.audio,
    };
    let cursor = if config.include_cursor {
        CursorCaptureSettings::WithCursor
    } else {
        CursorCaptureSettings::WithoutCursor
    };
    let interval_ms = if config.audio.is_enabled() {
        10
    } else {
        (1000 / u64::from(fps.max(1))).max(1)
    };
    let settings = Settings::new(
        item,
        cursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Custom(Duration::from_millis(interval_ms)),
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        params,
    );
    let control =
        CaptureHandler::start_free_threaded(settings).map_err(|e| Error::Capture(e.to_string()))?;
    let output = config.output.clone();
    let stopper = Box::new(move || {
        let encoder = {
            let callback = control.callback();
            let mut handler = callback.lock();
            handler.take_encoder_after_audio()?
        };
        let stop_result = control.stop().map_err(|e| Error::Capture(e.to_string()));
        let finish_result = if let Some(encoder) = encoder {
            encoder.finish().map_err(|e| Error::Encoder(e.to_string()))
        } else {
            Ok(())
        };
        stop_result?;
        finish_result
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
        config.audio = crate::AudioConfig::none();
        let recording = start(config).expect("start");
        std::thread::sleep(Duration::from_secs(2));
        let path = recording.stop().expect("stop");
        assert!(path.is_file());
        assert!(fs::metadata(&path).expect("meta").len() > 0);
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop session with WASAPI devices"]
    fn records_primary_display_with_audio() {
        let dir = std::env::temp_dir();
        let output = dir.join("lightcapture-hw-audio.mp4");
        let _ = fs::remove_file(&output);
        let mut config = RecordConfig::new(&output);
        config.quality = crate::Quality::P720p30;
        let mut recording = start(config).expect("start");
        std::thread::sleep(Duration::from_secs(3));
        let (path, stats) = recording.stop_inner().expect("stop");
        assert!(path.is_file());
        assert!(fs::metadata(&path).expect("meta").len() > 0);
        assert!(
            stats.audio_frames_sent > 0,
            "expected WASAPI samples in the session, drops={}",
            stats.audio_drops
        );
        assert!(
            stats.width <= 1280 && stats.height <= 720,
            "720p30 must not encode above 1280x720, got {}x{}",
            stats.width,
            stats.height
        );
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop session"]
    fn records_1080p_preset_not_source_4k() {
        let dir = std::env::temp_dir();
        let output = dir.join("lightcapture-hw-1080p.mp4");
        let _ = fs::remove_file(&output);
        let mut config = RecordConfig::new(&output);
        config.quality = crate::Quality::P1080p30;
        config.audio = crate::AudioConfig::none();
        let mut recording = start(config).expect("start");
        std::thread::sleep(Duration::from_secs(2));
        let (path, stats) = recording.stop_inner().expect("stop");
        assert!(path.is_file());
        assert!(fs::metadata(&path).expect("meta").len() > 0);
        assert!(
            stats.width <= 1920 && stats.height <= 1080,
            "1080p30 must not encode above 1920x1080, got {}x{}",
            stats.width,
            stats.height
        );
    }
}
