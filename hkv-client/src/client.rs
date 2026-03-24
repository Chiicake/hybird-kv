//! # Synchronous Client API
//!
//! Expose a compact, blocking API for issuing Redis-compatible
//! commands to the HybridKV server over RESP2.
//!
//! ## Usage
//! ```no_run
//! use hkv_client::{ClientConfig, KVClient};
//! use std::time::Duration;
//!
//! let client = KVClient::connect("127.0.0.1:6379").expect("connect");
//! client.set(b"user:1", b"alice").expect("set");
//! let value = client.get(b"user:1").expect("get");
//! assert_eq!(value.as_deref(), Some(b"alice".as_slice()));
//! client.expire(b"user:1", Duration::from_secs(60)).expect("expire");
//!
//! let config = ClientConfig {
//!     addr: "127.0.0.1:6379".to_string(),
//!     max_idle: 4,
//!     max_total: 8,
//!     read_timeout: Some(Duration::from_secs(1)),
//!     write_timeout: Some(Duration::from_secs(1)),
//!     connect_timeout: Some(Duration::from_secs(1)),
//!     max_retries: 1,
//! };
//! let client = KVClient::with_config(config).expect("connect");
//! let _ = client.ping(None).expect("ping");
//! ```
//!
//! ## Connection Pooling Behavior
//! - Each request borrows one connection, performs one round-trip, then returns it.
//! - If the pool hits `max_total`, callers get a `PoolExhausted` error immediately.
//! - Idle connections are health-checked before reuse and dropped if the peer has closed them.
//! - Connections that hit IO/protocol errors are discarded to avoid reusing bad state.
//! - Retryable connection/setup failures can be retried on a fresh connection.
//!
//! ## Design Principles
//! 1. **Facade Pattern**: `KVClient` hides pooling and protocol details.
//! 2. **Borrow-Friendly API**: Accept `&[u8]` to avoid unnecessary copies.
//! 3. **Fail Fast**: Protocol violations surface immediately as errors.
//! 4. **Performance First**: Prefer direct TCP writes and buffer reuse.

use std::fmt;
use std::time::Duration;

use crate::pool::{ConnectionPool, ExecError, PoolConfig};
use crate::resp::RespValue;

/// Result type for the sync client.
pub type ClientResult<T> = Result<T, ClientError>;

/// Errors surfaced by the sync client.
#[derive(Debug)]
pub enum ClientError {
    /// Network or IO failure while reading/writing.
    Io(std::io::Error),
    /// RESP2 framing or parse error.
    Protocol,
    /// Server returned an error reply.
    Server { message: Vec<u8> },
    /// Response type did not match the expected command response.
    UnexpectedResponse,
    /// Pool is at capacity and no idle connections are available.
    PoolExhausted,
    /// Address could not be parsed into a socket address.
    InvalidAddress,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Io(err) => write!(f, "io error: {}", err),
            ClientError::Protocol => write!(f, "protocol error"),
            ClientError::Server { message } => {
                write!(f, "server error: {}", String::from_utf8_lossy(message))
            }
            ClientError::UnexpectedResponse => write!(f, "unexpected response"),
            ClientError::PoolExhausted => write!(f, "connection pool exhausted"),
            ClientError::InvalidAddress => write!(f, "invalid address"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<std::io::Error> for ClientError {
    fn from(err: std::io::Error) -> Self {
        ClientError::Io(err)
    }
}

/// TTL state returned by the server, mirroring Redis semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientTtl {
    /// Key is missing or already expired.
    Missing,
    /// Key exists without expiration.
    NoExpiry,
    /// Key expires after the provided duration.
    ExpiresIn(Duration),
}

/// Configuration for the synchronous client and its pool.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Server address, e.g. "127.0.0.1:6379".
    pub addr: String,
    /// Maximum idle connections kept in the pool.
    pub max_idle: usize,
    /// Maximum total connections (idle + in-use).
    pub max_total: usize,
    /// Optional TCP read timeout.
    pub read_timeout: Option<Duration>,
    /// Optional TCP write timeout.
    pub write_timeout: Option<Duration>,
    /// Optional TCP connect timeout.
    pub connect_timeout: Option<Duration>,
    /// Maximum transparent retries for retryable connection/setup failures.
    pub max_retries: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            addr: "127.0.0.1:6379".to_string(),
            max_idle: 8,
            max_total: 16,
            read_timeout: None,
            write_timeout: None,
            connect_timeout: None,
            max_retries: 1,
        }
    }
}

/// Synchronous client with connection pooling.
///
/// This is a facade over the pool and RESP encoder/decoder. Each call acquires
/// a connection, executes one command, and returns the connection to the pool.
pub struct KVClient {
    pool: ConnectionPool,
    max_retries: usize,
}

impl KVClient {
    /// Creates a client with default configuration.
    pub fn connect(addr: impl Into<String>) -> ClientResult<Self> {
        let config = ClientConfig {
            addr: addr.into(),
            ..ClientConfig::default()
        };
        Self::with_config(config)
    }

    /// Creates a client with a custom configuration.
    pub fn with_config(config: ClientConfig) -> ClientResult<Self> {
        let pool = ConnectionPool::new(PoolConfig {
            addr: config.addr,
            max_idle: config.max_idle,
            max_total: config.max_total,
            read_timeout: config.read_timeout,
            write_timeout: config.write_timeout,
            connect_timeout: config.connect_timeout,
        })?;
        Ok(KVClient {
            pool,
            max_retries: config.max_retries,
        })
    }

    /// Fetches a value by key.
    ///
    /// Returns `Ok(None)` when the key is missing.
    ///
    /// The server response is expected to be a bulk string or null bulk string.
    pub fn get(&self, key: &[u8]) -> ClientResult<Option<Vec<u8>>> {
        match self.exec_with_retry(&[b"GET", key])? {
            RespValue::Bulk(data) => Ok(data),
            RespValue::Error(message) => Err(ClientError::Server { message }),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Sets a value for a key without expiration.
    ///
    /// Uses RESP2 `SET key value` and expects a simple string response.
    pub fn set(&self, key: &[u8], value: &[u8]) -> ClientResult<()> {
        match self.exec_with_retry(&[b"SET", key, value])? {
            RespValue::Simple(_) => Ok(()),
            RespValue::Error(message) => Err(ClientError::Server { message }),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Sets a value and attaches an expiration in seconds.
    ///
    /// Uses RESP2 `SET key value EX seconds`. TTL seconds are encoded without heap allocations.
    pub fn set_with_ttl(&self, key: &[u8], value: &[u8], ttl: Duration) -> ClientResult<()> {
        let (seconds, len) = encode_u64(ttl.as_secs());
        match self.exec_with_retry(&[b"SET", key, value, b"EX", &seconds[..len]])? {
            RespValue::Simple(_) => Ok(()),
            RespValue::Error(message) => Err(ClientError::Server { message }),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Deletes a key. Returns true when a key was removed.
    ///
    /// `DEL` returns an integer count. Non-zero maps to true.
    pub fn delete(&self, key: &[u8]) -> ClientResult<bool> {
        match self.exec_with_retry(&[b"DEL", key])? {
            RespValue::Integer(count) => Ok(count > 0),
            RespValue::Error(message) => Err(ClientError::Server { message }),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Sets a time-to-live on a key. Returns true when the TTL was set.
    ///
    /// Mirrors Redis `EXPIRE` semantics: 1 when applied, 0 when missing.
    pub fn expire(&self, key: &[u8], ttl: Duration) -> ClientResult<bool> {
        let (seconds, len) = encode_u64(ttl.as_secs());
        match self.exec_with_retry(&[b"EXPIRE", key, &seconds[..len]])? {
            RespValue::Integer(value) => Ok(value == 1),
            RespValue::Error(message) => Err(ClientError::Server { message }),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Returns TTL status for a key.
    ///
    /// Converts Redis TTL conventions (-2 missing, -1 no expiry) into `ClientTtl`.
    pub fn ttl(&self, key: &[u8]) -> ClientResult<ClientTtl> {
        match self.exec_with_retry(&[b"TTL", key])? {
            RespValue::Integer(-2) => Ok(ClientTtl::Missing),
            RespValue::Integer(-1) => Ok(ClientTtl::NoExpiry),
            RespValue::Integer(value) if value >= 0 => {
                Ok(ClientTtl::ExpiresIn(Duration::from_secs(value as u64)))
            }
            RespValue::Error(message) => Err(ClientError::Server { message }),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Pings the server. Returns the raw response payload.
    ///
    /// A payload triggers bulk string echo; otherwise a simple "PONG".
    pub fn ping(&self, payload: Option<&[u8]>) -> ClientResult<Vec<u8>> {
        let response = match payload {
            Some(data) => self.exec_with_retry(&[b"PING", data])?,
            None => self.exec_with_retry(&[b"PING"])?,
        };
        match response {
            RespValue::Simple(text) => Ok(text),
            RespValue::Bulk(Some(data)) => Ok(data),
            RespValue::Error(message) => Err(ClientError::Server { message }),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Fetches server INFO output.
    ///
    /// Returns the raw bulk payload; parsing is left to the caller.
    pub fn info(&self) -> ClientResult<Vec<u8>> {
        match self.exec_with_retry(&[b"INFO"])? {
            RespValue::Bulk(Some(data)) => Ok(data),
            RespValue::Error(message) => Err(ClientError::Server { message }),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    fn exec_with_retry(&self, args: &[&[u8]]) -> ClientResult<RespValue> {
        let mut attempts = 0usize;

        loop {
            let mut conn = match self.pool.acquire() {
                Ok(conn) => conn,
                Err(err) if attempts < self.max_retries && is_retryable_connect_error(&err) => {
                    attempts += 1;
                    continue;
                }
                Err(err) => return Err(err),
            };

            match conn.exec(args) {
                Ok(response) => return Ok(response),
                Err(ExecError::Retryable(err)) if attempts < self.max_retries => {
                    attempts += 1;
                }
                Err(err) => return Err(err.into_client_error()),
            }
        }
    }
}

fn is_retryable_connect_error(err: &ClientError) -> bool {
    match err {
        ClientError::Io(err) => matches!(
            err.kind(),
            std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::NotConnected
                | std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::Interrupted
        ),
        _ => false,
    }
}

fn encode_u64(mut value: u64) -> ([u8; 20], usize) {
    // Stack buffer keeps conversion allocation-free (zero-cost abstraction).
    let mut buf = [0u8; 20];
    let mut len = 0;
    if value == 0 {
        buf[0] = b'0';
        return (buf, 1);
    }
    while value > 0 {
        buf[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    buf[..len].reverse();
    (buf, len)
}
