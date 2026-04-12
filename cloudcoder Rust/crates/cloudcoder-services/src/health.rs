//! Health check module
//!
//! Provides health checking utilities for services.

use std::collections::HashMap;

use cloudcoder_core::{HealthStatus, HealthCheck};

/// Aggregates health status from multiple services
pub struct HealthAggregator {
    services: Vec<String>,
}

impl HealthAggregator {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }

    pub fn register(&mut self, service_name: impl Into<String>) {
        self.services.push(service_name.into());
    }

    pub async fn check_all(&self, checks: HashMap<String, HealthCheck>) -> HealthStatus {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let healthy = checks.values().all(|c| c.ok);

        HealthStatus {
            healthy,
            checks,
            last_check: now,
        }
    }
}

impl Default for HealthAggregator {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple health check builder
pub struct HealthCheckBuilder {
    ok: bool,
    message: Option<String>,
    value: Option<f64>,
}

impl HealthCheckBuilder {
    pub fn new() -> Self {
        Self {
            ok: true,
            message: None,
            value: None,
        }
    }

    pub fn ok(mut self, ok: bool) -> Self {
        self.ok = ok;
        self
    }

    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn value(mut self, value: f64) -> Self {
        self.value = Some(value);
        self
    }

    pub fn build(self) -> HealthCheck {
        HealthCheck {
            ok: self.ok,
            message: self.message,
            value: self.value,
        }
    }
}

impl Default for HealthCheckBuilder {
    fn default() -> Self {
        Self::new()
    }
}