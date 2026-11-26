// Copyright (C) 2026 by the Quassel Project and the REVYTECH Rust port authors
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program; if not, write to the Free Software Foundation,
// Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301, USA.

/*!
The rate limit of the login attempts.

A core that answers every wrong password at once can be used to try a whole
dictionary against one account.  The core therefore counts the failed
attempts of every user name and address, and waits a little longer after
every failure once the count is over the threshold.

The wait grows with the number of failures, but it never grows for ever: the
failures of a user are forgotten after a while, so that a lockout cannot be
caused by an attacker and last longer than the administrator expects.
*/

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How many failed attempts are answered without a wait.
pub const DEFAULT_THRESHOLD: u32 = 3;
/// How long the core waits after the threshold is reached.
pub const DEFAULT_BASE_DELAY: Duration = Duration::from_secs(1);
/// The longest wait the core ever takes.
pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(60);
/// How long the failures of a user are remembered.
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(15 * 60);

/// The key the attempts of one user and address are counted under.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LoginKey {
    /// The user name that was tried.
    pub username: String,
    /// The address the attempt came from.
    pub address: IpAddr,
}

impl LoginKey {
    /// The key of a user name and an address.
    pub fn new(username: &str, address: IpAddr) -> Self {
        Self {
            username: username.to_owned(),
            address,
        }
    }
}

/// What the limiter knows about one key.
#[derive(Clone, Copy, Debug)]
struct Attempt {
    /// How many attempts failed.
    failures: u32,
    /// When the last one failed.
    last: Instant,
}

/// The limiter of the login attempts.
#[derive(Debug)]
pub struct LoginLimiter {
    attempts: Mutex<HashMap<LoginKey, Attempt>>,
    threshold: u32,
    base_delay: Duration,
    max_delay: Duration,
    window: Duration,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginLimiter {
    /// A limiter with the default settings.
    pub fn new() -> Self {
        Self::with_settings(
            DEFAULT_THRESHOLD,
            DEFAULT_BASE_DELAY,
            DEFAULT_MAX_DELAY,
            DEFAULT_WINDOW,
        )
    }

    /// A limiter with the given settings.
    pub fn with_settings(
        threshold: u32,
        base_delay: Duration,
        max_delay: Duration,
        window: Duration,
    ) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            threshold,
            base_delay,
            max_delay,
            window,
        }
    }

    /// How long the caller has to wait before the attempt is tried.
    ///
    /// The wait is zero as long as the number of failures is under the
    /// threshold, and it doubles with every failure above it.
    pub fn wait_time(&self, key: &LoginKey) -> Duration {
        let mut attempts = self.attempts.lock().expect("the login limiter lock");
        let Some(attempt) = attempts.get_mut(key) else {
            return Duration::ZERO;
        };
        if attempt.last.elapsed() > self.window {
            // The failures are old; the user gets a clean slate again.
            attempts.remove(key);
            return Duration::ZERO;
        }
        if attempt.failures < self.threshold {
            return Duration::ZERO;
        }
        let steps = (attempt.failures - self.threshold).min(6);
        let factor = 1u32 << steps;
        self.base_delay.saturating_mul(factor).min(self.max_delay)
    }

    /// Records a failed attempt.
    pub fn record_failure(&self, key: &LoginKey) {
        let mut attempts = self.attempts.lock().expect("the login limiter lock");
        let attempt = attempts.entry(key.clone()).or_insert(Attempt {
            failures: 0,
            last: Instant::now(),
        });
        attempt.failures = attempt.failures.saturating_add(1);
        attempt.last = Instant::now();
    }

    /// Forgets the attempts of a user that logged in.
    pub fn record_success(&self, key: &LoginKey) {
        let mut attempts = self.attempts.lock().expect("the login limiter lock");
        attempts.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> LoginKey {
        LoginKey::new("alice", "127.0.0.1".parse().expect("an address"))
    }

    #[test]
    fn the_first_attempts_are_answered_at_once() {
        let limiter = LoginLimiter::new();
        let key = key();
        assert_eq!(limiter.wait_time(&key), Duration::ZERO);
        for _ in 0..2 {
            limiter.record_failure(&key);
            assert_eq!(limiter.wait_time(&key), Duration::ZERO);
        }
        // The third failure reaches the threshold: the next attempt waits.
        limiter.record_failure(&key);
        assert_eq!(limiter.wait_time(&key), DEFAULT_BASE_DELAY);
    }

    #[test]
    fn the_wait_grows_with_the_failures() {
        let limiter = LoginLimiter::new();
        let key = key();
        for _ in 0..3 {
            limiter.record_failure(&key);
        }
        assert_eq!(limiter.wait_time(&key), DEFAULT_BASE_DELAY);
        limiter.record_failure(&key);
        assert_eq!(limiter.wait_time(&key), DEFAULT_BASE_DELAY * 2);
    }

    #[test]
    fn the_wait_never_grows_past_the_maximum() {
        let limiter = LoginLimiter::new();
        let key = key();
        for _ in 0..40 {
            limiter.record_failure(&key);
        }
        assert_eq!(limiter.wait_time(&key), DEFAULT_MAX_DELAY);
    }

    #[test]
    fn a_successful_login_forgets_the_failures() {
        let limiter = LoginLimiter::new();
        let key = key();
        for _ in 0..5 {
            limiter.record_failure(&key);
        }
        assert!(limiter.wait_time(&key) > Duration::ZERO);
        limiter.record_success(&key);
        assert_eq!(limiter.wait_time(&key), Duration::ZERO);
    }

    #[test]
    fn another_user_is_not_punished_for_the_failures_of_one() {
        let limiter = LoginLimiter::new();
        let alice = key();
        let bob = LoginKey::new("bob", "127.0.0.1".parse().expect("an address"));
        for _ in 0..5 {
            limiter.record_failure(&alice);
        }
        assert!(limiter.wait_time(&alice) > Duration::ZERO);
        assert_eq!(limiter.wait_time(&bob), Duration::ZERO);
    }
}
