use std::time::{Duration, Instant};

/// Caps in-flight raw frames and target FPS. Excess frames are dropped, never queued.
pub struct FrameGate {
    min_interval: Duration,
    max_in_flight: u8,
    in_flight: u8,
    last_accepted: Option<Instant>,
    dropped: u64,
}

impl FrameGate {
    #[must_use]
    pub fn new(fps: u32, max_in_flight: u8) -> Self {
        let fps = fps.max(1);
        Self {
            min_interval: Duration::from_nanos(1_000_000_000 / u64::from(fps)),
            max_in_flight: max_in_flight.max(1),
            in_flight: 0,
            last_accepted: None,
            dropped: 0,
        }
    }

    /// Two-slot GPU pool used by the recording path.
    #[must_use]
    pub fn recording(fps: u32) -> Self {
        Self::new(fps, 2)
    }

    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Returns whether this frame should be submitted to the encoder.
    pub fn try_accept(&mut self, now: Instant) -> bool {
        if self.in_flight >= self.max_in_flight {
            self.dropped += 1;
            return false;
        }
        if let Some(last) = self.last_accepted {
            if now.duration_since(last) < self.min_interval {
                self.dropped += 1;
                return false;
            }
        }
        self.in_flight += 1;
        self.last_accepted = Some(now);
        true
    }

    /// Call after the encoder has taken the surface (or send failed).
    pub fn release(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_first_frame() {
        let mut gate = FrameGate::recording(30);
        let t0 = Instant::now();
        assert!(gate.try_accept(t0));
        assert_eq!(gate.dropped(), 0);
    }

    #[test]
    fn drops_faster_than_fps() {
        let mut gate = FrameGate::recording(30);
        let t0 = Instant::now();
        assert!(gate.try_accept(t0));
        gate.release();
        assert!(!gate.try_accept(t0 + Duration::from_millis(1)));
        assert_eq!(gate.dropped(), 1);
    }

    #[test]
    fn accepts_after_interval() {
        let mut gate = FrameGate::recording(30);
        let t0 = Instant::now();
        assert!(gate.try_accept(t0));
        gate.release();
        assert!(gate.try_accept(t0 + Duration::from_millis(34)));
        assert_eq!(gate.dropped(), 0);
    }

    #[test]
    fn drops_when_pool_full() {
        let mut gate = FrameGate::new(60, 2);
        let t0 = Instant::now();
        assert!(gate.try_accept(t0));
        assert!(gate.try_accept(t0 + Duration::from_millis(20)));
        assert!(!gate.try_accept(t0 + Duration::from_millis(40)));
        assert_eq!(gate.dropped(), 1);
        gate.release();
        assert!(gate.try_accept(t0 + Duration::from_millis(60)));
    }
}
