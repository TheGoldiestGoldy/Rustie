// Single-writer, single-reader latest-sample monitor for live input-to-output.
// The recording callback never blocks on the audio engine. Producer and
// consumer share only atomic numbers; they never allocate in callbacks.
use crate::graph::StereoFrame;
use std::sync::atomic::{AtomicU64, Ordering};

pub const MONITOR_FRAMES: usize = 16_384;

#[derive(Debug)]
pub struct RecordingMonitor {
    frames: Box<[AtomicU64]>,
    next: AtomicU64,
    peak: AtomicU64,
}
impl RecordingMonitor {
    pub fn new() -> Self {
        Self {
            frames: (0..MONITOR_FRAMES)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            next: AtomicU64::new(0),
            peak: AtomicU64::new(0),
        }
    }
    pub fn write(&self, frame: StereoFrame) {
        let at = self.next.load(Ordering::Relaxed);
        let packed = (u64::from(frame.left.to_bits()) << 32) | u64::from(frame.right.to_bits());
        self.frames[at as usize % MONITOR_FRAMES].store(packed, Ordering::Relaxed);
        self.next.store(at.wrapping_add(1), Ordering::Release);
        // The frame counter also serves as a real capture-progress meter.
        // This peak is the latest callback sample, not a calibrated VU.
        let peak = frame.left.abs().max(frame.right.abs()).clamp(0.0, 1.0);
        self.peak
            .store(u64::from(peak.to_bits()), Ordering::Relaxed);
    }
    pub fn next_index(&self) -> u64 {
        self.next.load(Ordering::Acquire)
    }
    pub fn last_peak(&self) -> f32 {
        f32::from_bits(self.peak.load(Ordering::Relaxed) as u32)
    }
    pub fn take(&self, cursor: &mut u64) -> StereoFrame {
        let newest = self.next.load(Ordering::Acquire);
        // Live monitoring should never replay a backlog accumulated while
        // another audio driver was starting/stopped. Catch up conservatively.
        if newest.saturating_sub(*cursor) > MONITOR_FRAMES as u64 / 2 {
            *cursor = newest.saturating_sub(128);
        }
        if *cursor >= newest {
            return StereoFrame::ZERO;
        }
        let value = self.frames[*cursor as usize % MONITOR_FRAMES].load(Ordering::Relaxed);
        *cursor = cursor.saturating_add(1);
        StereoFrame {
            left: f32::from_bits((value >> 32) as u32),
            right: f32::from_bits(value as u32),
        }
    }
}
impl Default for RecordingMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitor_moves_forward_and_drops_old_backlog() {
        let ring = RecordingMonitor::new();
        let mut at = 0;
        assert_eq!(ring.take(&mut at), StereoFrame::ZERO);
        ring.write(StereoFrame {
            left: 0.25,
            right: -0.4,
        });
        let output = ring.take(&mut at);
        assert_eq!(output.left, 0.25);
        assert_eq!(output.right, -0.4);
        assert_eq!(ring.next_index(), 1);
        for i in 0..MONITOR_FRAMES * 2 {
            ring.write(StereoFrame {
                left: i as f32,
                right: 0.0,
            });
        }
        let stale = ring.take(&mut at);
        assert!(stale.left > 1000.0);
    }
}
