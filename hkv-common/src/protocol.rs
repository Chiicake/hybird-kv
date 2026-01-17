//! # Protocol Structures
//!
//! Purpose: Define FFI-safe request/response headers for ioctl communication.
//!
//! ## Design Principles
//!
//! 1. **FFI Stability**: Use `#[repr(C)]` to keep user/kernel layouts consistent.
//! 2. **Minimal Overhead**: Keep headers tiny to reduce copy and cache pressure.
//! 3. **Versioned ABI**: Embed a protocol version for forward compatibility checks.
//!
//! ## Memory Layout Example
//!
//! ```text
//! IoctlHeader (4 bytes total):
//! +--------+---------+----------+----------+
//! | magic  | version | command  | reserved |
//! +--------+---------+----------+----------+
//! | 1B     | 1B      | 1B       | 1B       |
//! +--------+---------+----------+----------+
//!
//! ReadRequest (262 bytes total):
//! +------------+---------+
//! | header:4B  | key:258B|
//! +------------+---------+
//!
//! ReadResponse (1032 bytes total):
//! +------------+-----------+-------------+
//! | header:4B  | status:2B | value:1026B |
//! +------------+-----------+-------------+
//!
//! PromoteRequest (1304 bytes total):
//! +------------+---------+-----------+-----------+--------+
//! | header:4B  | key:258B| value:1026B| version:8B| ttl:8B |
//! +------------+---------+-----------+-----------+--------+
//!
//! PromoteResponse (8 bytes total):
//! +------------+-----------+-------------+
//! | header:4B  | status:2B | reserved:2B |
//! +------------+-----------+-------------+
//! ```

use crate::ioctl::{IoctlCommand, IOCTL_MAGIC};
use crate::types::{Key, Ttl, Value, Version};

/// Protocol version for user/kernel ABI compatibility.
pub const PROTOCOL_VERSION: u8 = 1;

/// Status code indicating success in ioctl responses.
pub const STATUS_OK: u16 = 0;

/// Common header prepended to ioctl request/response payloads.
///
/// This header is `repr(C)` to preserve C ABI layout for kernel interop.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IoctlHeader {
    /// Magic number to validate the device protocol.
    pub magic: u8,
    /// Protocol version for ABI checks.
    pub version: u8,
    /// Command number describing the request.
    pub command: u8,
    /// Reserved for alignment and future flags; must be zero.
    pub reserved: u8,
}

impl IoctlHeader {
    /// Builds a header for the provided ioctl command.
    pub const fn new(command: IoctlCommand) -> Self {
        IoctlHeader {
            magic: IOCTL_MAGIC,
            version: PROTOCOL_VERSION,
            command: command.as_u8(),
            reserved: 0,
        }
    }
}

/// Read request payload for a cache lookup.
///
/// Uses the header + payload pattern to validate command metadata once and
/// keep the key inline for zero-allocation FFI transfers.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadRequest {
    /// Common ioctl header (command must be READ).
    pub header: IoctlHeader,
    /// Lookup key (length-prefixed, fixed-capacity buffer).
    pub key: Key,
}

impl ReadRequest {
    /// Builds a read request for the provided key.
    pub fn new(key: Key) -> Self {
        ReadRequest {
            header: IoctlHeader::new(IoctlCommand::Read),
            key,
        }
    }
}

/// Read response payload for a cache lookup.
///
/// The `status` field uses `STATUS_OK` for success or an `HkvError::code()`
/// value on failure. The `value` buffer is valid only when `status` is OK.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadResponse {
    /// Common ioctl header (command must be READ).
    pub header: IoctlHeader,
    /// Status code (0 on success, error code on failure).
    pub status: u16,
    /// Value buffer (length-prefixed, fixed-capacity buffer).
    pub value: Value,
}

impl ReadResponse {
    /// Builds a read response with an explicit status and value.
    pub fn new(status: u16, value: Value) -> Self {
        ReadResponse {
            header: IoctlHeader::new(IoctlCommand::Read),
            status,
            value,
        }
    }
}

/// Promote request payload for inserting a single entry into the kernel cache.
///
/// The header identifies the command, while the payload carries only the
/// minimum metadata needed for cache admission (version + TTL) to keep the
/// user/kernel copy as small as possible.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromoteRequest {
    /// Common ioctl header (command must be PROMOTE).
    pub header: IoctlHeader,
    /// Entry key to insert.
    pub key: Key,
    /// Entry value to insert.
    pub value: Value,
    /// Version to associate with the entry.
    pub version: Version,
    /// Absolute expiration timestamp for the entry.
    pub ttl: Ttl,
}

impl PromoteRequest {
    /// Builds a promote request for the provided entry data.
    pub fn new(key: Key, value: Value, version: Version, ttl: Ttl) -> Self {
        PromoteRequest {
            header: IoctlHeader::new(IoctlCommand::Promote),
            key,
            value,
            version,
            ttl,
        }
    }
}

/// Promote response payload indicating success or failure.
///
/// Uses `STATUS_OK` on success or an `HkvError::code()` value on failure.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromoteResponse {
    /// Common ioctl header (command must be PROMOTE).
    pub header: IoctlHeader,
    /// Status code (0 on success, error code on failure).
    pub status: u16,
    /// Reserved for future flags; must be zero.
    pub reserved: u16,
}

impl PromoteResponse {
    /// Builds a promote response with an explicit status.
    pub fn new(status: u16) -> Self {
        PromoteResponse {
            header: IoctlHeader::new(IoctlCommand::Promote),
            status,
            reserved: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ioctl_header_new() {
        let header = IoctlHeader::new(IoctlCommand::Read);
        assert_eq!(header.magic, IOCTL_MAGIC);
        assert_eq!(header.version, PROTOCOL_VERSION);
        assert_eq!(header.command, IoctlCommand::Read.as_u8());
        assert_eq!(header.reserved, 0);
    }

    #[test]
    fn test_ioctl_header_size() {
        assert_eq!(std::mem::size_of::<IoctlHeader>(), 4);
    }

    #[test]
    fn test_read_request_new() {
        let key = Key::new(b"alpha").unwrap();
        let request = ReadRequest::new(key.clone());
        assert_eq!(request.header, IoctlHeader::new(IoctlCommand::Read));
        assert_eq!(request.key, key);
    }

    #[test]
    fn test_read_response_new() {
        let value = Value::new(b"beta").unwrap();
        let response = ReadResponse::new(STATUS_OK, value.clone());
        assert_eq!(response.header, IoctlHeader::new(IoctlCommand::Read));
        assert_eq!(response.status, STATUS_OK);
        assert_eq!(response.value, value);
    }

    #[test]
    fn test_read_struct_sizes() {
        assert_eq!(std::mem::size_of::<ReadRequest>(), 262);
        assert_eq!(std::mem::size_of::<ReadResponse>(), 1032);
    }

    #[test]
    fn test_promote_request_new() {
        let key = Key::new(b"alpha").unwrap();
        let value = Value::new(b"beta").unwrap();
        let request = PromoteRequest::new(key.clone(), value.clone(), Version::ZERO, Ttl::INFINITE);
        assert_eq!(request.header, IoctlHeader::new(IoctlCommand::Promote));
        assert_eq!(request.key, key);
        assert_eq!(request.value, value);
        assert_eq!(request.version, Version::ZERO);
        assert_eq!(request.ttl, Ttl::INFINITE);
    }

    #[test]
    fn test_promote_response_new() {
        let response = PromoteResponse::new(STATUS_OK);
        assert_eq!(response.header, IoctlHeader::new(IoctlCommand::Promote));
        assert_eq!(response.status, STATUS_OK);
        assert_eq!(response.reserved, 0);
    }

    #[test]
    fn test_promote_struct_sizes() {
        assert_eq!(std::mem::size_of::<PromoteRequest>(), 1304);
        assert_eq!(std::mem::size_of::<PromoteResponse>(), 8);
    }
}
