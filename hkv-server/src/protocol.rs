//! # RESP2 Parser
//!
//! Parse RESP2 arrays of bulk strings from a streaming TCP buffer.
//!
//! ## Design Principles
//!
//! 1. **State Machine Pattern**: Explicit parser states avoid backtracking and
//!    keep control flow predictable.
//! 2. **Streaming Friendly**: The parser consumes from a mutable buffer and
//!    returns `None` when more data is needed.
//! 3. **Low Allocation**: Only bulk string arguments are copied into `Vec<u8>`.
//! 4. **Fail Fast**: Malformed frames return a protocol error immediately.

use bytes::{Buf, BytesMut};

/// RESP parser errors surfaced to the server for client responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RespError {
    /// The input is not valid RESP2 for the supported subset.
    Protocol,
}

/// RESP2 parser for arrays of bulk strings.
#[derive(Debug)]
pub struct RespParser {
    state: ParseState,
    args: Vec<Vec<u8>>,
    remaining: usize,
    bulk_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    ArrayLen,
    BulkLen,
    BulkData,
}

impl RespParser {
    /// Creates a new parser in the initial state.
    pub fn new() -> Self {
        RespParser {
            state: ParseState::ArrayLen,
            args: Vec::new(),
            remaining: 0,
            bulk_len: 0,
        }
    }

    /// Attempts to parse a single command from the buffer.
    ///
    /// Returns `Ok(None)` if more data is required.
    pub fn parse(&mut self, buf: &mut BytesMut) -> Result<Option<Vec<Vec<u8>>>, RespError> {
        loop {
            match self.state {
                ParseState::ArrayLen => {
                    let line = match read_line(buf) {
                        Some(line) => line,
                        None => return Ok(None),
                    };
                    if line.first() != Some(&b'*') {
                        return Err(RespError::Protocol);
                    }
                    let count = parse_usize(&line[1..])?;
                    self.args.clear();
                    self.remaining = count;
                    if self.remaining == 0 {
                        self.state = ParseState::ArrayLen;
                        return Ok(Some(Vec::new()));
                    }
                    self.state = ParseState::BulkLen;
                }
                ParseState::BulkLen => {
                    let line = match read_line(buf) {
                        Some(line) => line,
                        None => return Ok(None),
                    };
                    if line.first() != Some(&b'$') {
                        return Err(RespError::Protocol);
                    }
                    let len = parse_usize(&line[1..])?;
                    self.bulk_len = len;
                    self.state = ParseState::BulkData;
                }
                ParseState::BulkData => {
                    if buf.len() < self.bulk_len + 2 {
                        return Ok(None);
                    }
                    let data = buf.split_to(self.bulk_len).to_vec();
                    if buf.get_u8() != b'\r' || buf.get_u8() != b'\n' {
                        return Err(RespError::Protocol);
                    }
                    self.args.push(data);
                    self.remaining -= 1;
                    if self.remaining == 0 {
                        self.state = ParseState::ArrayLen;
                        return Ok(Some(std::mem::take(&mut self.args)));
                    }
                    self.state = ParseState::BulkLen;
                }
            }
        }
    }
}

fn read_line(buf: &mut BytesMut) -> Option<BytesMut> {
    let mut idx = 1;
    while idx < buf.len() {
        if buf[idx] == b'\n' && buf[idx - 1] == b'\r' {
            let line = buf.split_to(idx - 1);
            buf.advance(2);
            return Some(line);
        }
        idx += 1;
    }
    None
}

fn parse_usize(data: &[u8]) -> Result<usize, RespError> {
    if data.is_empty() {
        return Err(RespError::Protocol);
    }
    let mut value: usize = 0;
    for &b in data {
        if b < b'0' || b > b'9' {
            return Err(RespError::Protocol);
        }
        value = value.saturating_mul(10).saturating_add((b - b'0') as usize);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_get() {
        let mut buf = BytesMut::from("*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n");
        let mut parser = RespParser::new();
        let cmd = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(cmd.len(), 2);
        assert_eq!(cmd[0], b"GET");
        assert_eq!(cmd[1], b"key");
    }

    #[test]
    fn handles_partial_frames() {
        let mut buf = BytesMut::from("*1\r\n$4\r\nPIN");
        let mut parser = RespParser::new();
        assert!(parser.parse(&mut buf).unwrap().is_none());
        buf.extend_from_slice(b"G\r\n");
        let cmd = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(cmd[0], b"PING");
    }
}
