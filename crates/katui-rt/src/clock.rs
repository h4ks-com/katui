//! The frame clock: fixed-frame deadlines paced from one start instant,
//! so a slow frame catches up with the next one instead of drifting.

use std::thread;
use std::time::{Duration, Instant};

/// A fixed-rate frame pacer.
#[derive(Debug)]
pub struct FrameClock {
    frame_time: Duration,
    start: Instant,
    frame: u64,
}

impl FrameClock {
    /// A clock ticking `frames_per_second` times a second from now.
    #[must_use]
    pub fn new(frames_per_second: u32) -> Self {
        let rate = f64::from(frames_per_second.max(1));
        Self {
            frame_time: Duration::from_secs_f64(1.0 / rate),
            start: Instant::now(),
            frame: 0,
        }
    }

    /// One frame's duration.
    pub fn frame_time(&self) -> Duration {
        self.frame_time
    }

    /// The frames issued so far.
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Sleep until the next frame deadline and advance. A late frame
    /// returns immediately; the schedule, not the workload, rules.
    pub fn wait_for_next_frame(&mut self) {
        self.frame += 1;
        let frames = u32::try_from(self.frame).unwrap_or(u32::MAX);
        let target = self.frame_time.saturating_mul(frames);
        if let Some(deadline) = self.start.checked_add(target)
            && let Some(remaining) = deadline.checked_duration_since(Instant::now())
        {
            thread::sleep(remaining);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::FrameClock;

    #[test]
    fn a_sixty_fps_clock_allots_sixteen_milli_frames() {
        let clock = FrameClock::new(60);
        let frame_time = clock.frame_time();
        assert!(frame_time >= Duration::from_millis(16));
        assert!(frame_time <= Duration::from_millis(17));
    }

    #[test]
    fn waiting_counts_frames_without_panicking() {
        let mut clock = FrameClock::new(1000);
        clock.wait_for_next_frame();
        clock.wait_for_next_frame();
        assert_eq!(clock.frame(), 2);
    }
}
