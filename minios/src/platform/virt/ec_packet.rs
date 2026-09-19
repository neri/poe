//! ChromeOS EC host command packets (protocol version 3)
//!
//! `struct ec_host_request` and `struct ec_host_response` of ec_commands.h.
//! This module has no hardware dependency.

pub const HEADER_SIZE: usize = 8;

const EC_HOST_REQUEST_VERSION: u8 = 3;
const EC_HOST_RESPONSE_VERSION: u8 = 3;

/// Error in a response packet
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseError {
    /// Malformed packet (version, reserved field, length or checksum)
    Invalid,
    /// The EC returned an error result code (EC_RES_*)
    Result(u16),
}

/// Sum of the bytes (the checksum makes the sum of a whole packet zero)
pub fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, v| acc.wrapping_add(*v))
}

/// Builds a request packet into `buf` and returns its size.
pub fn build_request(buf: &mut [u8], command: u16, version: u8, data: &[u8]) -> Option<usize> {
    let size = HEADER_SIZE + data.len();
    if size > buf.len() || data.len() > u16::MAX as usize {
        return None;
    }
    let command = command.to_le_bytes();
    let data_len = (data.len() as u16).to_le_bytes();
    buf[..HEADER_SIZE].copy_from_slice(&[
        EC_HOST_REQUEST_VERSION,
        0, // checksum
        command[0],
        command[1],
        version,
        0, // reserved
        data_len[0],
        data_len[1],
    ]);
    buf[HEADER_SIZE..size].copy_from_slice(data);
    buf[1] = checksum(&buf[..size]).wrapping_neg();
    Some(size)
}

/// Checks a response packet and returns the range of its data.
pub fn parse_response(buf: &[u8]) -> Result<core::ops::Range<usize>, ResponseError> {
    if buf.len() < HEADER_SIZE || buf[0] != EC_HOST_RESPONSE_VERSION {
        return Err(ResponseError::Invalid);
    }
    let result = u16::from_le_bytes([buf[2], buf[3]]);
    let data_len = u16::from_le_bytes([buf[4], buf[5]]) as usize;
    let reserved = u16::from_le_bytes([buf[6], buf[7]]);
    if reserved != 0 || HEADER_SIZE + data_len > buf.len() {
        return Err(ResponseError::Invalid);
    }
    if checksum(&buf[..HEADER_SIZE + data_len]) != 0 {
        return Err(ResponseError::Invalid);
    }
    if result != 0 {
        return Err(ResponseError::Result(result));
    }
    Ok(HEADER_SIZE..HEADER_SIZE + data_len)
}
