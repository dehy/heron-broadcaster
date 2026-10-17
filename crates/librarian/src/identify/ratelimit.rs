// SPDX-License-Identifier: AGPL-3.0-or-later

//! Spacing requests to a web service: MusicBrainz allows one request per
//! second, AcoustID three.
//!
//! The services count requests as they arrive, and a request can leave late
//! (a stall, a reconnection) while the next one leaves on time. So the
//! interval also restarts when a response comes back ([`RateLimiter::done`]):
//! requests made one after the other then always arrive at least one
//! interval apart, whatever the network does.

use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

pub struct RateLimiter {
    interval: Duration,
    next: Mutex<Option<Instant>>,
}

impl RateLimiter {
    /// At most one request per `interval`.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            next: Mutex::new(None),
        }
    }

    /// Waits for the next free slot. Callers are served in turn.
    pub async fn wait(&self) {
        let mut next = self.next.lock().await;
        let now = Instant::now();
        let slot = next.map_or(now, |next| next.max(now));
        *next = Some(slot + self.interval);
        tokio::time::sleep_until(slot).await;
    }

    /// Records that a response came back: the next request waits a full
    /// interval from now.
    pub async fn done(&self) {
        let mut next = self.next.lock().await;
        let after = Instant::now() + self.interval;
        if next.is_none_or(|next| next < after) {
            *next = Some(after);
        }
    }

    /// Pushes the next slot back (the service asked us to slow down).
    pub async fn back_off(&self, delay: Duration) {
        let mut next = self.next.lock().await;
        let after = Instant::now() + delay;
        if next.is_none_or(|next| next < after) {
            *next = Some(after);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn requests_are_spaced() {
        let limiter = std::sync::Arc::new(RateLimiter::new(Duration::from_secs(1)));
        let start = Instant::now();
        let mut tasks = Vec::new();
        for _ in 0..4 {
            let limiter = limiter.clone();
            tasks.push(tokio::spawn(async move {
                limiter.wait().await;
                Instant::now()
            }));
        }
        let mut times = Vec::new();
        for task in tasks {
            times.push(task.await.unwrap() - start);
        }
        times.sort();
        assert_eq!(
            times,
            [0, 1, 2, 3].map(Duration::from_secs).to_vec(),
            "one per second"
        );

        limiter.done().await;
        let before = Instant::now();
        limiter.wait().await;
        assert_eq!(
            Instant::now() - before,
            Duration::from_secs(1),
            "a full interval after a response"
        );

        limiter.back_off(Duration::from_secs(10)).await;
        let before = Instant::now();
        limiter.wait().await;
        assert_eq!(Instant::now() - before, Duration::from_secs(10));
    }
}
