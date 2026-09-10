use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

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
}

#[derive(Debug)]
pub(crate) struct StatsInner {
    pub frames_captured: AtomicU64,
    pub frames_encoded: AtomicU64,
    pub frames_dropped: AtomicU64,
    pub width: AtomicU32,
    pub height: AtomicU32,
    pub fps_target: AtomicU32,
    started: Instant,
}

impl StatsInner {
    pub(crate) fn new(fps_target: u32) -> Arc<Self> {
        Arc::new(Self {
            frames_captured: AtomicU64::new(0),
            frames_encoded: AtomicU64::new(0),
            frames_dropped: AtomicU64::new(0),
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
            fps_target: AtomicU32::new(fps_target),
            started: Instant::now(),
        })
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reads_atomics() {
        let inner = StatsInner::new(30);
        inner.frames_encoded.store(12, Ordering::Relaxed);
        inner.width.store(1920, Ordering::Relaxed);
        let snap = inner.snapshot();
        assert_eq!(snap.frames_encoded, 12);
        assert_eq!(snap.width, 1920);
        assert_eq!(snap.fps_target, 30);
    }
}
