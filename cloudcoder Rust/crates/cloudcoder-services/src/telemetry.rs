//! Telemetry module
//!
//! Provides telemetry and metrics collection functionality.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::RwLock;

/// Metrics counter
#[derive(Debug, Default)]
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    pub fn increment(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }
}

/// Metrics gauge (can go up or down)
#[derive(Debug, Default)]
pub struct Gauge {
    value: AtomicU64,
}

impl Gauge {
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    pub fn set(&self, value: u64) {
        self.value.store(value, Ordering::Relaxed);
    }

    pub fn increment(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Histogram for tracking value distributions
#[derive(Debug)]
pub struct Histogram {
    buckets: Vec<AtomicU64>,
    boundaries: Vec<u64>,
}

impl Histogram {
    pub fn new(boundaries: Vec<u64>) -> Self {
        let buckets: Vec<AtomicU64> = boundaries.iter().map(|_| AtomicU64::new(0)).collect();
        Self {
            buckets,
            boundaries,
        }
    }

    pub fn observe(&self, value: u64) {
        for (i, &boundary) in self.boundaries.iter().enumerate() {
            if value <= boundary {
                self.buckets[i].fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        // If value exceeds all boundaries, add to last bucket
        if let Some(last) = self.buckets.last() {
            last.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn get_snapshot(&self) -> Vec<u64> {
        self.buckets.iter().map(|b| b.load(Ordering::Relaxed)).collect()
    }

    pub fn reset(&self) {
        for bucket in &self.buckets {
            bucket.store(0, Ordering::Relaxed);
        }
    }
}

/// Registry for storing metrics
pub struct MetricsRegistry {
    counters: RwLock<HashMap<String, Arc<Counter>>>,
    gauges: RwLock<HashMap<String, Arc<Gauge>>>,
    histograms: RwLock<HashMap<String, Arc<Histogram>>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
        }
    }

    pub async fn counter(&self, name: &str) -> Arc<Counter> {
        let counters = self.counters.read().await;
        if let Some(counter) = counters.get(name) {
            return Arc::clone(counter);
        }
        drop(counters);

        let mut counters = self.counters.write().await;
        counters
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Counter::new()))
            .clone()
    }

    pub async fn gauge(&self, name: &str) -> Arc<Gauge> {
        let gauges = self.gauges.read().await;
        if let Some(gauge) = gauges.get(name) {
            return Arc::clone(gauge);
        }
        drop(gauges);

        let mut gauges = self.gauges.write().await;
        gauges
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Gauge::new()))
            .clone()
    }

    pub async fn histogram(&self, name: &str, boundaries: Vec<u64>) -> Arc<Histogram> {
        let histograms = self.histograms.read().await;
        if let Some(histogram) = histograms.get(name) {
            return Arc::clone(histogram);
        }
        drop(histograms);

        let mut histograms = self.histograms.write().await;
        histograms
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Histogram::new(boundaries)))
            .clone()
    }

    pub async fn get_all_counters(&self) -> HashMap<String, u64> {
        let counters = self.counters.read().await;
        counters
            .iter()
            .map(|(k, v)| (k.clone(), v.get()))
            .collect()
    }

    pub async fn get_all_gauges(&self) -> HashMap<String, u64> {
        let gauges = self.gauges.read().await;
        gauges
            .iter()
            .map(|(k, v)| (k.clone(), v.get()))
            .collect()
    }

    pub async fn reset_all(&self) {
        let counters = self.counters.read().await;
        for counter in counters.values() {
            counter.reset();
        }

        let histograms = self.histograms.read().await;
        for histogram in histograms.values() {
            histogram.reset();
        }
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}