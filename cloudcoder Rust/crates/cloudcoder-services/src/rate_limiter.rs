//! Rate limiter module
//!
//! Provides rate limiting functionality for API calls and tool executions.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of requests per window
    pub max_requests: u32,
    /// Window duration in milliseconds
    pub window_ms: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_ms: 60000, // 1 minute
        }
    }
}

/// Entry tracking rate limit state
struct RateLimitEntry {
    count: u32,
    window_start: Instant,
}

impl RateLimitEntry {
    fn new() -> Self {
        Self {
            count: 0,
            window_start: Instant::now(),
        }
    }
}

/// In-memory rate limiter
pub struct RateLimiter {
    config: RateLimitConfig,
    entries: HashMap<String, RateLimitEntry>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
        }
    }

    /// Check if a request is allowed for the given key
    pub fn check(&mut self, key: &str) -> bool {
        let now = Instant::now();
        let window_duration = Duration::from_millis(self.config.window_ms);

        let entry = self.entries.entry(key.to_string()).or_insert_with(RateLimitEntry::new);

        // Reset window if expired
        if now.duration_since(entry.window_start) >= window_duration {
            entry.count = 0;
            entry.window_start = now;
        }

        if entry.count < self.config.max_requests {
            entry.count += 1;
            true
        } else {
            false
        }
    }

    /// Get remaining requests for a key
    pub fn remaining(&self, key: &str) -> u32 {
        if let Some(entry) = self.entries.get(key) {
            self.config.max_requests.saturating_sub(entry.count)
        } else {
            self.config.max_requests
        }
    }

    /// Get time until reset for a key (in milliseconds)
    pub fn reset_time_ms(&self, key: &str) -> Option<u64> {
        if let Some(entry) = self.entries.get(key) {
            let elapsed = Instant::now().duration_since(entry.window_start);
            let window = Duration::from_millis(self.config.window_ms);
            if elapsed < window {
                Some((window - elapsed).as_millis() as u64)
            } else {
                Some(0)
            }
        } else {
            None
        }
    }

    /// Clear all rate limit entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Clear rate limit entry for a specific key
    pub fn clear_key(&mut self, key: &str) {
        self.entries.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiting() {
        let mut limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 3,
            window_ms: 60000,
        });

        assert!(limiter.check("user1"));
        assert!(limiter.check("user1"));
        assert!(limiter.check("user1"));
        assert!(!limiter.check("user1")); // Exceeded limit

        assert!(limiter.check("user2")); // Different key
    }
}