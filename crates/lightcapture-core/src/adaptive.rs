use std::time::{Duration, Instant};

/// Step encode FPS down under encoder backpressure. Resolution stays fixed (one encoder).
pub(crate) struct Adaptive {
    fps: u32,
    last_backpressure: u64,
    last_encoded: u64,
    last_check: Instant,
}

impl Adaptive {
    #[must_use]
    pub(crate) fn new(fps: u32) -> Self {
        Self {
            fps: fps.max(1),
            last_backpressure: 0,
            last_encoded: 0,
            last_check: Instant::now(),
        }
    }

    /// Returns a new FPS when the encoder is falling behind. Never steps up.
    pub(crate) fn tick(&mut self, now: Instant, backpressure: u64, encoded: u64) -> Option<u32> {
        if now.duration_since(self.last_check) < Duration::from_secs(2) {
            return None;
        }
        let bp_delta = backpressure.saturating_sub(self.last_backpressure);
        let enc_delta = encoded.saturating_sub(self.last_encoded);
        self.last_backpressure = backpressure;
        self.last_encoded = encoded;
        self.last_check = now;
        let work = bp_delta.saturating_add(enc_delta);
        if work < 10 {
            return None;
        }
        if bp_delta.saturating_mul(5) < work {
            return None;
        }
        let next = step_down_fps(self.fps);
        if next < self.fps {
            self.fps = next;
            Some(next)
        } else {
            None
        }
    }
}

/// 60 → 30 → 24. Matches the MVP2 FPS ladder without restarting the encoder.
#[must_use]
pub fn step_down_fps(fps: u32) -> u32 {
    if fps > 30 {
        30
    } else if fps > 24 {
        24
    } else {
        fps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_stops_at_24() {
        assert_eq!(step_down_fps(60), 30);
        assert_eq!(step_down_fps(30), 24);
        assert_eq!(step_down_fps(24), 24);
    }

    #[test]
    fn steps_down_when_backpressure_is_high() {
        let start = Instant::now();
        let mut adaptive = Adaptive::new(60);
        adaptive.last_check = start;
        let next = adaptive.tick(start + Duration::from_secs(2), 8, 12);
        assert_eq!(next, Some(30));
    }

    #[test]
    fn ignores_healthy_encode() {
        let start = Instant::now();
        let mut adaptive = Adaptive::new(60);
        adaptive.last_check = start;
        assert_eq!(adaptive.tick(start + Duration::from_secs(2), 0, 60), None);
    }
}
