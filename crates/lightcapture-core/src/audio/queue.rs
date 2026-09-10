use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

/// About 8 × 20 ms chunks. Extra PCM is dropped, never queued without bound.
pub const PCM_QUEUE_CAP: usize = 8;

/// Bounded PCM inbox. Full → drop oldest and count a drop.
pub struct PcmQueue {
    slots: Mutex<VecDeque<Vec<u8>>>,
    drops: AtomicU64,
}

impl PcmQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Mutex::new(VecDeque::with_capacity(PCM_QUEUE_CAP)),
            drops: AtomicU64::new(0),
        }
    }

    pub fn push(&self, chunk: Vec<u8>) {
        let mut slots = lock_slots(&self.slots);
        while slots.len() >= PCM_QUEUE_CAP {
            slots.pop_front();
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
        slots.push_back(chunk);
    }

    #[must_use]
    pub fn pop_all(&self) -> Vec<Vec<u8>> {
        lock_slots(&self.slots).drain(..).collect()
    }

    #[must_use]
    pub fn drops(&self) -> u64 {
        self.drops.load(Ordering::Relaxed)
    }
}

fn lock_slots(slots: &Mutex<VecDeque<Vec<u8>>>) -> MutexGuard<'_, VecDeque<Vec<u8>>> {
    slots
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_oldest_increments_drops() {
        let queue = PcmQueue::new();
        for i in 0..=PCM_QUEUE_CAP {
            queue.push(vec![i as u8]);
        }
        assert_eq!(queue.drops(), 1);
        let chunks = queue.pop_all();
        assert_eq!(chunks.len(), PCM_QUEUE_CAP);
        assert_eq!(chunks[0], vec![1]);
        assert_eq!(chunks[PCM_QUEUE_CAP - 1], vec![PCM_QUEUE_CAP as u8]);
    }

    #[test]
    fn pop_all_empties() {
        let queue = PcmQueue::new();
        queue.push(vec![9]);
        assert_eq!(queue.pop_all(), vec![vec![9]]);
        assert!(queue.pop_all().is_empty());
    }
}
