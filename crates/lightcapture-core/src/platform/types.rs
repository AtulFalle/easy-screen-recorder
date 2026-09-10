use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::stats::{SessionStats, StatsInner};
use crate::Error;

/// A monitor the user can pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureDisplay {
    pub index: usize,
    pub name: String,
    /// Stable-enough id: `\\.\DISPLAYn` from GDI. Match by [`Self::name`] if this shifts.
    pub device_id: String,
    pub width: u32,
    pub height: u32,
}

/// A window the user can pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureWindow {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

/// An in-progress recording. Dropping without [`Recording::stop`] may leave a truncated MP4.
pub struct Recording {
    pub(crate) output: PathBuf,
    pub(crate) stats: Arc<StatsInner>,
    pub(crate) paused: Arc<AtomicBool>,
    pub(crate) stopper: Option<Box<dyn FnOnce() -> crate::Result<()> + Send>>,
}

impl Recording {
    #[must_use]
    pub fn output(&self) -> &std::path::Path {
        &self.output
    }

    #[must_use]
    pub fn stats(&self) -> SessionStats {
        self.stats.snapshot()
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Capture/encode thread stored a fatal error and tried to keep the file.
    #[must_use]
    pub fn failure(&self) -> Option<String> {
        self.stats.snapshot().failure
    }

    #[must_use]
    pub fn has_failed(&self) -> bool {
        self.stats.has_failed()
    }

    /// Finalize the MP4 and join the capture thread.
    pub fn stop(mut self) -> crate::Result<PathBuf> {
        let (path, _) = self.stop_inner()?;
        Ok(path)
    }

    pub(crate) fn stop_inner(&mut self) -> crate::Result<(PathBuf, SessionStats)> {
        let stopper = self.stopper.take().ok_or(Error::Stopped)?;
        stopper()?;
        let stats = self.stats.snapshot();
        if stats.frames_encoded == 0 && stats.failure.is_none() {
            return Err(Error::NoFrames);
        }
        if let Some(message) = stats.failure {
            return Err(crate::classify_encode_failure(&message));
        }
        Ok((self.output.clone(), stats))
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if let Some(stopper) = self.stopper.take() {
            let _ = stopper();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::StatsInner;

    #[test]
    fn pause_flag_round_trips() {
        let recording = Recording {
            output: PathBuf::from("out.mp4"),
            stats: StatsInner::new(30, crate::EncoderKind::Software),
            paused: Arc::new(AtomicBool::new(false)),
            stopper: Some(Box::new(|| Ok(()))),
        };
        assert!(!recording.is_paused());
        recording.set_paused(true);
        assert!(recording.is_paused());
        recording.set_paused(false);
        assert!(!recording.is_paused());
    }
}
