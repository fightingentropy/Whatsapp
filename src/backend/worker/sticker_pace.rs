//! Shared limit for recent and received sticker downloads (ZapFast b75be18).
use std::time::{Duration, Instant};

const IN_FLIGHT: usize = 2;
const FIRST: Duration = Duration::from_secs(30);
const LONGEST: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
pub(super) struct Pace {
    until: Option<Instant>,
    pause: Duration,
}

impl Pace {
    pub(super) fn slots(&self, now: Instant, running: usize) -> usize {
        if self.until.is_some_and(|until| now < until) {
            0
        } else {
            IN_FLIGHT.saturating_sub(running)
        }
    }

    pub(super) fn limited(&mut self, now: Instant) {
        self.pause = if self.until.is_some_and(|until| now < until + self.pause) {
            (self.pause * 2).min(LONGEST)
        } else {
            FIRST
        };
        self.until = Some(now + self.pause);
    }
}

pub(super) fn rate_limited(error: &str) -> bool {
    error.contains("rate-overlimit")
        || error.contains("code=429")
        || error.contains("status: 429")
        || error.contains("HTTP status 429")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_two_slots_and_backs_off_after_throttling() {
        let mut pace = Pace::default();
        let now = Instant::now();
        assert_eq!(pace.slots(now, 0), 2);
        assert_eq!(pace.slots(now, 1), 1);
        assert_eq!(pace.slots(now, 2), 0);
        pace.limited(now);
        assert_eq!(pace.slots(now + FIRST - Duration::from_secs(1), 0), 0);
        assert_eq!(pace.slots(now + FIRST, 0), 2);
        pace.limited(now + FIRST);
        assert_eq!(pace.slots(now + FIRST * 2, 0), 0);
        assert_eq!(pace.slots(now + FIRST * 3, 1), 1);
        for _ in 0..10 {
            pace.limited(now);
        }
        assert_eq!(pace.pause, LONGEST);
        pace.limited(now + LONGEST * 3);
        assert_eq!(pace.pause, FIRST);
        assert!(rate_limited("server returned status: 429"));
        assert!(rate_limited("rate-overlimit"));
        assert!(!rate_limited("status: 404"));
    }
}
