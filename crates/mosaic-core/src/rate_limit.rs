//! Synchronous rate limiter shared across scrapers (per-source instance).

use std::time::{Duration, Instant};

/// Throttle requests to one source: sleeps until `min_interval` has elapsed
/// since the previous request.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    min_interval: Duration,
    last_request: Option<Instant>,
}

impl RateLimiter {
    pub fn new(min_interval: Duration) -> Self {
        RateLimiter {
            min_interval,
            last_request: None,
        }
    }

    /// Sleep if needed so the caller may issue the next request now.
    pub fn wait(&mut self) {
        if let Some(last) = self.last_request {
            let elapsed = last.elapsed();
            if elapsed < self.min_interval {
                std::thread::sleep(self.min_interval - elapsed);
            }
        }
        self.last_request = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_call_does_not_sleep() {
        let mut limiter = RateLimiter::new(Duration::from_secs(60));
        let start = Instant::now();
        limiter.wait();
        assert!(start.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn second_call_sleeps_to_min_interval() {
        let mut limiter = RateLimiter::new(Duration::from_millis(80));
        limiter.wait();
        std::thread::sleep(Duration::from_millis(20));
        let start = Instant::now();
        limiter.wait();
        // Allow clock/precision slack; the limiter must sleep the gap.
        assert!(start.elapsed() >= Duration::from_millis(50));
    }
}
