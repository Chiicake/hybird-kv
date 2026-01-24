//! # Storage Engine Interface
//!
//! ## Design Principles
//!
//! 1. **Strategy Pattern**: Abstract the engine behind a trait so different
//!    implementations can be swapped without touching the caller.
//! 2. **Binary-Safe API**: Keys/values are byte buffers to match Redis semantics.
//! 3. **Zero-Cost Dispatch**: When used with generics, calls monomorphize to
//!    avoid dynamic dispatch overhead.
//! 4. **Explicit TTL**: Expose expiration via a dedicated method to keep the
//!    hot read path minimal.

use std::sync::Arc;
use std::time::Duration;

use hkv_common::HkvResult;

/// TTL query result for Redis-style semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtlStatus {
    /// Key does not exist or is already expired.
    Missing,
    /// Key exists but has no expiration set.
    NoExpiry,
    /// Key expires after the given duration.
    ExpiresIn(Duration),
}

/// Strategy pattern: defines the engine behavior surface for the server.
///
/// Keys and values are treated as bulk strings (binary-safe) for Phase 1.
pub trait KVEngine: Send + Sync {
    /// Returns the value for a key, or `None` if missing or expired.
    fn get(&self, key: &[u8]) -> HkvResult<Option<Arc<[u8]>>>;

    /// Inserts or replaces a key with the provided value.
    ///
    /// Takes ownership to avoid extra copies on the hot path.
    fn set(&self, key: Vec<u8>, value: Vec<u8>) -> HkvResult<()>;

    /// Removes a key. Returns true if the key existed and was removed.
    fn delete(&self, key: &[u8]) -> HkvResult<bool>;

    /// Sets an expiration on a key. Returns `NotFound` if the key is missing.
    fn expire(&self, key: &[u8], ttl: Duration) -> HkvResult<()>;

    /// Returns the TTL state for a key.
    fn ttl(&self, key: &[u8]) -> HkvResult<TtlStatus>;
}
