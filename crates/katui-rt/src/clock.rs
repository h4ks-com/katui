//! The frame clock: fixed-frame deadlines, where a frame that misses its
//! slot restarts the schedule so a stall never ends in a burst.

use std::thread;
use std::time::{Duration, Instant};

/// A fixed-rate frame pacer.
#[derive(Debug)]
pub struct FrameClock {
    frame_time: Duration,
    deadline: Instant,
    frame: u64,
}

impl FrameClock {
    /// A clock ticking `frames_per_second` times a second from now.
    #[must_use]
    pub fn new(frames_per_second: u32) -> Self {
        let rate = f64::from(frames_per_second.max(1));
        Self {
            frame_time: Duration::from_secs_f64(1.0 / rate),
            deadline: Instant::now(),
            frame: 0,
        }
    }

    /// One frame's duration.
    #[must_use]
    pub fn frame_time(&self) -> Duration {
        self.frame_time
    }

    /// The frames issued so far.
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Sleep until the next frame deadline and advance. A late frame
    /// returns immediately.
    pub fn wait_for_next_frame(&mut self) {
        let remaining = self.advance(Instant::now());
        if !remaining.is_zero() {
            thread::sleep(remaining);
        }
    }

    /// Move to the next slot and report the sleep it needs at `now`.
    fn advance(&mut self, now: Instant) -> Duration {
        self.frame += 1;
        self.deadline += self.frame_time;
        if self.deadline < now {
            self.deadline = now;
        }
        self.deadline.duration_since(now)
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
    fn a_stall_restarts_the_schedule_instead_of_bursting() {
        let mut clock = FrameClock::new(10);
        let stalled_until = clock.deadline + Duration::from_secs(5);
        assert!(
            clock.advance(stalled_until).is_zero(),
            "the late frame runs now"
        );
        assert_eq!(
            clock.advance(stalled_until),
            Duration::from_millis(100),
            "the next frame gets a whole slot, not a catch-up"
        );
    }

    #[test]
    fn waiting_counts_frames_without_panicking() {
        let mut clock = FrameClock::new(1000);
        clock.wait_for_next_frame();
        clock.wait_for_next_frame();
        assert_eq!(clock.frame(), 2);
    }
}
