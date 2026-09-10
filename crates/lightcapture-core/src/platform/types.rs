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

    /// Finalize the MP4 and join the capture thread.
    pub fn stop(mut self) -> crate::Result<PathBuf> {
        let stopper = self.stopper.take().ok_or(Error::Stopped)?;
        stopper()?;
        if self.stats.frames_encoded.load(Ordering::Relaxed) == 0 {
            return Err(Error::NoFrames);
        }
        Ok(self.output.clone())
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if let Some(stopper) = self.stopper.take() {
            let _ = stopper();
        }
    }
}
