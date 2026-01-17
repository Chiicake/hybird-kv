//! # HybridKV Error Types
//!
//! ## Design Principles
//!
//! 1. **Stable Codes**: Each variant has a fixed numeric code for FFI-safe transport.
//! 2. **Categorized Ranges**: Codes are grouped by intent (client, server, transient, protocol).
//! 3. **Low Overhead**: Enums are `Copy` and `repr(u16)` to keep payloads small.
//! 4. **Recoverability Hints**: Transient errors are explicitly marked as retryable.

use core::fmt;

/// Result type used across HybridKV components.
pub type HkvResult<T> = core::result::Result<T, HkvError>;

/// High-level category for grouping error codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum HkvErrorCategory {
    /// Invalid input or user request issues.
    Client,
    /// Server-side resource or invariant failures.
    Server,
    /// Retryable conditions such as contention or timeouts.
    Transient,
    /// Protocol or versioning mismatches across boundaries.
    Protocol,
}

impl HkvErrorCategory {
    /// Returns true if the category is safe to retry.
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::Transient)
    }
}

/// Error codes shared between user space and kernel space.
#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum HkvError {
    /// Client error: input failed validation (code 1).
    InvalidInput = 1,
    /// Client error: key not found (code 2).
    NotFound = 2,
    /// Client error: key length exceeds MAX_KEY_SIZE (code 3).
    KeyTooLong = 3,
    /// Client error: value length exceeds MAX_VALUE_SIZE (code 4).
    ValueTooLong = 4,

    /// Server error: kernel memory limit reached (code 10).
    OutOfMemory = 10,
    /// Server error: cache capacity limit reached (code 11).
    CapacityExceeded = 11,
    /// Server error: internal invariant violated (code 12).
    InternalError = 12,

    /// Transient error: resource busy, retry later (code 20).
    Busy = 20,
    /// Transient error: operation timed out (code 21).
    Timeout = 21,
    /// Transient error: operation interrupted (code 22).
    Interrupted = 22,

    /// Protocol error: version mismatch (code 30).
    VersionMismatch = 30,
    /// Protocol error: request/response schema mismatch (code 31).
    ProtocolViolation = 31,
    /// Protocol error: command not supported (code 32).
    UnsupportedCommand = 32,
}

impl HkvError {
    /// Returns the stable numeric code for the error.
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// Returns the coarse category of the error.
    pub const fn category(self) -> HkvErrorCategory {
        match self {
            Self::InvalidInput
            | Self::NotFound
            | Self::KeyTooLong
            | Self::ValueTooLong => HkvErrorCategory::Client,
            Self::OutOfMemory | Self::CapacityExceeded | Self::InternalError => {
                HkvErrorCategory::Server
            }
            Self::Busy | Self::Timeout | Self::Interrupted => HkvErrorCategory::Transient,
            Self::VersionMismatch | Self::ProtocolViolation | Self::UnsupportedCommand => {
                HkvErrorCategory::Protocol
            }
        }
    }

    /// Returns true if callers should retry the operation.
    pub const fn is_retryable(self) -> bool {
        self.category().is_retryable()
    }

    /// Converts a numeric code into a typed error.
    pub const fn from_code(code: u16) -> Option<Self> {
        match code {
            1 => Some(Self::InvalidInput),
            2 => Some(Self::NotFound),
            3 => Some(Self::KeyTooLong),
            4 => Some(Self::ValueTooLong),
            10 => Some(Self::OutOfMemory),
            11 => Some(Self::CapacityExceeded),
            12 => Some(Self::InternalError),
            20 => Some(Self::Busy),
            21 => Some(Self::Timeout),
            22 => Some(Self::Interrupted),
            30 => Some(Self::VersionMismatch),
            31 => Some(Self::ProtocolViolation),
            32 => Some(Self::UnsupportedCommand),
            _ => None,
        }
    }
}

impl fmt::Display for HkvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::InvalidInput => "invalid input",
            Self::NotFound => "not found",
            Self::KeyTooLong => "key too long",
            Self::ValueTooLong => "value too long",
            Self::OutOfMemory => "out of memory",
            Self::CapacityExceeded => "capacity exceeded",
            Self::InternalError => "internal error",
            Self::Busy => "busy",
            Self::Timeout => "timeout",
            Self::Interrupted => "interrupted",
            Self::VersionMismatch => "version mismatch",
            Self::ProtocolViolation => "protocol violation",
            Self::UnsupportedCommand => "unsupported command",
        };
        write!(f, "{}", label)
    }
}

#[cfg(test)]
mod tests {
    use super::{HkvError, HkvErrorCategory};

    #[test]
    fn maps_error_categories() {
        assert_eq!(HkvError::InvalidInput.category(), HkvErrorCategory::Client);
        assert_eq!(HkvError::OutOfMemory.category(), HkvErrorCategory::Server);
        assert_eq!(HkvError::Busy.category(), HkvErrorCategory::Transient);
        assert_eq!(HkvError::VersionMismatch.category(), HkvErrorCategory::Protocol);
    }

    #[test]
    fn retryable_only_for_transient() {
        assert!(HkvError::Busy.is_retryable());
        assert!(!HkvError::InvalidInput.is_retryable());
    }

    #[test]
    fn converts_from_code() {
        assert_eq!(HkvError::from_code(1), Some(HkvError::InvalidInput));
        assert_eq!(HkvError::from_code(99), None);
    }
}
