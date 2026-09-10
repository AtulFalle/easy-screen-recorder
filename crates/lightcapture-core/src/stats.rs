use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::hardware::EncoderKind;

/// Live counters for the UI and CLI. Cheap to snapshot; no frame buffers.
#[derive(Debug, Clone)]
pub struct SessionStats {
    pub frames_captured: u64,
    pub frames_encoded: u64,
    pub frames_dropped: u64,
    pub width: u32,
    pub height: u32,
    pub fps_target: u32,
    pub elapsed_secs: u64,
    pub audio_frames_sent: u64,
    pub audio_drops: u64,
    pub encoder: EncoderKind,
    pub failure: Option<String>,
}

#[derive(Debug)]
pub(crate) struct StatsInner {
    pub frames_captured: AtomicU64,
    pub frames_encoded: AtomicU64,
    pub frames_dropped: AtomicU64,
    pub width: AtomicU32,
    pub height: AtomicU32,
    pub fps_target: AtomicU32,
    pub audio_frames_sent: AtomicU64,
    pub audio_drops: AtomicU64,
    pub encoder: EncoderKind,
    failed: AtomicBool,
    failure: Mutex<Option<String>>,
    started: Instant,
}

impl StatsInner {
    pub(crate) fn new(fps_target: u32, encoder: EncoderKind) -> Arc<Self> {
        Arc::new(Self {
            frames_captured: AtomicU64::new(0),
            frames_encoded: AtomicU64::new(0),
            frames_dropped: AtomicU64::new(0),
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
            fps_target: AtomicU32::new(fps_target),
            audio_frames_sent: AtomicU64::new(0),
            audio_drops: AtomicU64::new(0),
            encoder,
            failed: AtomicBool::new(false),
            failure: Mutex::new(None),
            started: Instant::now(),
        })
    }

    pub(crate) fn note_failure(&self, message: impl Into<String>) {
        let message = message.into();
        if let Ok(mut slot) = self.failure.lock() {
            if slot.is_none() {
                *slot = Some(message);
            }
        }
        self.failed.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub(crate) fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    pub(crate) fn snapshot(&self) -> SessionStats {
        SessionStats {
            frames_captured: self.frames_captured.load(Ordering::Relaxed),
            frames_encoded: self.frames_encoded.load(Ordering::Relaxed),
            frames_dropped: self.frames_dropped.load(Ordering::Relaxed),
            width: self.width.load(Ordering::Relaxed),
            height: self.height.load(Ordering::Relaxed),
            fps_target: self.fps_target.load(Ordering::Relaxed),
            elapsed_secs: self.started.elapsed().as_secs(),
            audio_frames_sent: self.audio_frames_sent.load(Ordering::Relaxed),
            audio_drops: self.audio_drops.load(Ordering::Relaxed),
            encoder: self.encoder,
            failure: self.failure.lock().ok().and_then(|g| g.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reads_atomics() {
        let inner = StatsInner::new(30, EncoderKind::NvidiaNvenc);
        inner.frames_encoded.store(12, Ordering::Relaxed);
        inner.width.store(1920, Ordering::Relaxed);
        let snap = inner.snapshot();
        assert_eq!(snap.frames_encoded, 12);
        assert_eq!(snap.width, 1920);
        assert_eq!(snap.fps_target, 30);
        assert_eq!(snap.audio_frames_sent, 0);
        assert_eq!(snap.audio_drops, 0);
        assert_eq!(snap.encoder, EncoderKind::NvidiaNvenc);
        assert!(snap.failure.is_none());
    }

    #[test]
    fn first_failure_wins() {
        let inner = StatsInner::new(30, EncoderKind::Software);
        inner.note_failure("disk is full");
        inner.note_failure("later");
        assert!(inner.has_failed());
        assert_eq!(inner.snapshot().failure.as_deref(), Some("disk is full"));
    }
}
