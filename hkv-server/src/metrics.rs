//! # Server Metrics
//!
//! Provide lightweight counters and a latency histogram to compute
//! QPS, error rate, and tail latency for the user-space server.
//!
//! ## Design Principles
//! 1. **Accumulator Pattern**: Use atomic counters to aggregate events cheaply.
//! 2. **Fixed Buckets**: Keep histogram buckets in a contiguous array for cache locality.
//! 3. **Zero-Cost Access**: Expose snapshots as plain structs without heap work.
//! 4. **FFI-Free**: Pure Rust types keep the hot path safe and portable.
//!
//! ## Notes
//! - Metrics are intentionally decoupled from the request path to keep the
//!   server fast; wiring and sampling policy are left to the caller.
//! - Bucket boundaries are expressed in microseconds and can be tuned later.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Default latency bucket boundaries in microseconds.
///
/// These are coarse on purpose to keep bucket scans short (performance-first).
pub const DEFAULT_LATENCY_BUCKETS_US: [u64; 12] =
    [1, 2, 5, 10, 20, 50, 100, 200, 500, 1_000, 2_000, 5_000];

/// Snapshot of all server metrics at a point in time.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    /// Total number of requests observed.
    pub requests_total: u64,
    /// Total number of error responses observed.
    pub errors_total: u64,
    /// Current in-flight requests.
    pub inflight: u64,
    /// Latency histogram snapshot.
    pub latency: LatencySnapshot,
}

/// Snapshot of the latency histogram.
#[derive(Debug, Clone)]
pub struct LatencySnapshot {
    /// Bucket boundaries in microseconds.
    pub bounds_us: Vec<u64>,
    /// Bucket counts, including the overflow bucket at the end.
    pub buckets: Vec<u64>,
    /// Total number of samples.
    pub samples: u64,
    /// Sum of latencies in microseconds.
    pub sum_us: u64,
}

/// Thread-safe metrics aggregator for the server.
///
/// The struct is intentionally small and uses `AtomicU64` so record calls are
/// zero-allocation and cheap. `Ordering::Relaxed` is sufficient because we do
/// not require cross-field ordering, only eventual consistency.
pub struct Metrics {
    requests_total: AtomicU64,
    errors_total: AtomicU64,
    inflight: AtomicU64,
    latency: LatencyHistogram,
}

impl Metrics {
    /// Creates a new metrics aggregator with the default latency buckets.
    pub fn new() -> Self {
        Metrics{
            requests_total: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            inflight: AtomicU64::new(0),
            latency: LatencyHistogram::new(DEFAULT_LATENCY_BUCKETS_US.to_vec()),
        }
    }

    /// Creates a new metrics aggregator with custom latency bucket boundaries.
    ///
    /// The boundaries must be sorted ascending and represent microseconds.
    ///
    /// **Input**: `bounds_us` (ascending microsecond thresholds).
    /// **Output**: a `Metrics` instance configured with those buckets.
    pub fn with_latency_buckets(bounds_us: Vec<u64>) -> Self {
        Metrics{
            requests_total: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            inflight: AtomicU64::new(0),
            latency: LatencyHistogram::new(bounds_us),
        }
    }

    /// Records the start of a request.
    ///
    /// Call this when a request is accepted to increment totals and in-flight.
    pub fn record_request_start(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.inflight.fetch_add(1, Ordering::Relaxed);
    }

    /// Records the end of a request.
    ///
    /// Call this on completion to decrement in-flight and capture latency.
    pub fn record_request_end(&self, latency: Duration) {
        self.inflight.fetch_sub(1, Ordering::Relaxed);
        self.latency.record(latency);
    }

    /// Records an error response.
    pub fn record_error(&self) {
        self.errors_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns a snapshot of all counters and histogram buckets.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            requests_total: self.requests_total.load(Ordering::Relaxed),
            errors_total: self.errors_total.load(Ordering::Relaxed),
            inflight: self.inflight.load(Ordering::Relaxed),
            latency: self.latency.snapshot(),
        }
    }
}

/// Fixed-bucket latency histogram.
///
/// Uses a linear scan to pick buckets; this is O(buckets) but the list is small
/// and stays hot in cache. If you need faster bucket selection, replace with a
/// binary search or precomputed lookup table.
pub struct LatencyHistogram {
    bounds_us: Vec<u64>,
    buckets: Vec<AtomicU64>,
    sum_us: AtomicU64,
    samples: AtomicU64,
}

impl LatencyHistogram {
    /// Creates a histogram with explicit bucket boundaries (microseconds).
    pub fn new(bounds_us: Vec<u64>) -> Self {
        let mut buckets = Vec::with_capacity(bounds_us.len() + 1);
        for _ in 0..=bounds_us.len() {
            buckets.push(AtomicU64::new(0));
        }

        LatencyHistogram {
            bounds_us,
            buckets,
            sum_us: AtomicU64::new(0),
            samples: AtomicU64::new(0),
        }
    }

    /// Records a latency measurement into the histogram.
    ///
    /// Caller passes `Duration` to avoid unit ambiguity.
    pub fn record(&self, latency: Duration) {
        let micros = latency.as_micros() as u64;
        self.samples.fetch_add(1, Ordering::Relaxed);
        self.sum_us.fetch_add(micros, Ordering::Relaxed);

        let mut bucket_idx = self.bounds_us.len();
        for (i, &bound) in self.bounds_us.iter().enumerate() {
            if micros <= bound {
                bucket_idx = i;
                break;
            }
        }
        self.buckets[bucket_idx].fetch_add(1, Ordering::Relaxed);
    }

    /// Returns a point-in-time snapshot of the histogram.
    pub fn snapshot(&self) -> LatencySnapshot {
        let buckets: Vec<u64> = self.buckets.iter()
            .map(|b| b.load(Ordering::Relaxed))
            .collect();

        LatencySnapshot {
            bounds_us: self.bounds_us.clone(),
            buckets,
            samples: self.samples.load(Ordering::Relaxed),
            sum_us: self.sum_us.load(Ordering::Relaxed),
        }
    }
}
