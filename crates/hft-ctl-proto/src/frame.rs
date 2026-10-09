//! Wire frame: `u32` big-endian length N | N bytes of compact UTF-8 JSON.

use crate::MAX_FRAME;

/// Frame errors.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FrameErr {
    /// Announced length above `MAX_FRAME`.
    TooLong,
}

/// Body length announced by a frame header; `N > MAX_FRAME` → `TooLong`.
pub fn frame_len(hdr: &[u8; 4]) -> Result<usize, FrameErr> {
    let n = u32::from_be_bytes(*hdr) as usize;
    if n > MAX_FRAME {
        Err(FrameErr::TooLong)
    } else {
        Ok(n)
    }
}

/// Appends the big-endian length and the bytes of `json` to `out` (existing bytes are kept).
pub fn encode_frame(json: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(json.len() as u32).to_be_bytes());
    out.extend_from_slice(json);
}
