//! WebSocket framing (RFC 6455 §5): the 2–14 B server-frame header parser, client masking and
//! `MaskedFrame` (README "签名草案", §1, §2).

use crate::types::Opcode;

/// Opcodes on the wire.
pub(crate) const OP_CONT: u8 = 0x0;
pub(crate) const OP_TEXT: u8 = 0x1;
pub(crate) const OP_BINARY: u8 = 0x2;
pub(crate) const OP_CLOSE: u8 = 0x8;
pub(crate) const OP_PING: u8 = 0x9;
pub(crate) const OP_PONG: u8 = 0xA;

/// A parsed server-frame header.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FrameHead {
    pub fin: bool,
    pub opcode: u8,
    /// Payload length.
    pub len: u64,
    /// Header bytes (2, 4 or 10; servers never mask).
    pub hdr_len: usize,
}

/// Outcome of `parse_head`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Head {
    /// Fewer bytes than the header needs.
    NeedMore,
    /// Valid header.
    Ok(FrameHead),
    /// Protocol violation (`WS_PROTO`): mask bit, RSV bits, unknown opcode, fragmented or > 125 B
    /// control frame, 64-bit length with the top bit set.
    Proto,
}

/// Parses a server-frame header from the start of `b` (no allocation, no copy).
#[inline]
pub fn parse_head(b: &[u8]) -> Head {
    let (b0, b1) = match b {
        [b0, b1, ..] => (*b0, *b1),
        _ => return Head::NeedMore,
    };
    let fin = b0 & 0x80 != 0;
    let rsv = b0 & 0x70;
    let opcode = b0 & 0x0F;
    if rsv != 0 {
        return Head::Proto;
    }
    match opcode {
        OP_CONT | OP_TEXT | OP_BINARY | OP_CLOSE | OP_PING | OP_PONG => {}
        _ => return Head::Proto,
    }
    if b1 & 0x80 != 0 {
        return Head::Proto;
    }
    let len7 = b1 & 0x7F;
    let control = opcode >= 0x8;
    if control && (!fin || len7 > 125) {
        return Head::Proto;
    }
    match len7 {
        126 => match b.get(2..4) {
            Some(&[a, c]) => Head::Ok(FrameHead {
                fin,
                opcode,
                len: u16::from_be_bytes([a, c]) as u64,
                hdr_len: 4,
            }),
            _ => Head::NeedMore,
        },
        127 => match b.get(2..10) {
            Some(x) => {
                let mut v = [0u8; 8];
                v.copy_from_slice(x);
                let len = u64::from_be_bytes(v);
                if len >> 63 != 0 {
                    return Head::Proto;
                }
                Head::Ok(FrameHead {
                    fin,
                    opcode,
                    len,
                    hdr_len: 10,
                })
            }
            None => Head::NeedMore,
        },
        n => Head::Ok(FrameHead {
            fin,
            opcode,
            len: n as u64,
            hdr_len: 2,
        }),
    }
}

/// XORs `data` with the 4 mask bytes, starting at key phase 0 (8-byte words, then the tail).
#[inline]
pub fn apply_mask(data: &mut [u8], key: [u8; 4]) {
    let k64 = u64::from_ne_bytes([key[0], key[1], key[2], key[3], key[0], key[1], key[2], key[3]]);
    let mut chunks = data.chunks_exact_mut(8);
    for c in &mut chunks {
        let mut w = [0u8; 8];
        w.copy_from_slice(c);
        let v = u64::from_ne_bytes(w) ^ k64;
        c.copy_from_slice(&v.to_ne_bytes());
    }
    for (i, b) in chunks.into_remainder().iter_mut().enumerate() {
        *b ^= key[i & 3];
    }
}

/// Writes a client header (FIN = 1, MASK = 1) for `len` payload bytes ending right before
/// `buf[end]`; returns the header length (6 or 8; 14 for lengths above 65 535).
#[inline]
pub(crate) fn write_client_head(buf: &mut [u8], end: usize, opcode: u8, len: usize, key: [u8; 4]) -> usize {
    let mut h = [0u8; 14];
    h[0] = 0x80 | (opcode & 0x0F);
    let n = if len < 126 {
        h[1] = 0x80 | len as u8;
        2
    } else if len <= 0xFFFF {
        h[1] = 0x80 | 126;
        h[2..4].copy_from_slice(&(len as u16).to_be_bytes());
        4
    } else {
        h[1] = 0x80 | 127;
        h[2..10].copy_from_slice(&(len as u64).to_be_bytes());
        10
    };
    h[n..n + 4].copy_from_slice(&key);
    let hl = n + 4;
    if let Some(dst) = end.checked_sub(hl).and_then(|s| buf.get_mut(s..end)) {
        dst.copy_from_slice(&h[..hl]);
    }
    hl
}

/// A client frame with its WS header written and its payload masked; the only input of
/// `Conn::send_frame`. The payload lives at the fixed offset `HDR_MAX`; the header is written
/// right-aligned in front of it, so sealing moves no bytes.
#[repr(C, align(64))]
pub struct MaskedFrame<const CAP: usize = 2048> {
    /// `as_bytes()` start = `HDR_MAX − header length`.
    start: u16,
    /// Header + payload.
    len: u16,
    opcode: u8,
    /// 0 = being filled; 1 = sealed.
    sealed: u8,
    _pad: [u8; 2],
    /// `[0..14)` header area, `[14..)` payload.
    buf: [u8; CAP],
}

const _: () = assert!(size_of::<MaskedFrame<2048>>() == 2112);
const _: () = assert!(size_of::<MaskedFrame<8192>>() == 8256);
const _: () = assert!(align_of::<MaskedFrame<2048>>() == 64);
const _: () = assert!(std::mem::offset_of!(MaskedFrame<2048>, buf) == 8);

impl<const CAP: usize> std::fmt::Debug for MaskedFrame<CAP> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaskedFrame")
            .field("cap", &CAP)
            .field("start", &self.start)
            .field("len", &self.len)
            .field("opcode", &self.opcode)
            .field("sealed", &self.sealed)
            .finish()
    }
}

impl<const CAP: usize> Default for MaskedFrame<CAP> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const CAP: usize> MaskedFrame<CAP> {
    /// Header area in front of the payload.
    pub const HDR_MAX: usize = 14;
    /// `len` is a `u16` and the header area must fit.
    const CAP_OK: () = assert!(CAP > 14 && CAP <= 65_535, "MaskedFrame CAP must be in 15..=65535");

    /// Startup; reuse afterwards.
    pub fn new() -> Self {
        #[allow(clippy::let_unit_value)]
        let () = Self::CAP_OK;
        MaskedFrame {
            start: Self::HDR_MAX as u16,
            len: 0,
            opcode: 0,
            sealed: 0,
            _pad: [0; 2],
            buf: [0; CAP],
        }
    }

    /// Writable payload area, `CAP − 14` bytes. Calling it on a sealed frame is a caller error.
    #[inline]
    pub fn body_mut(&mut self) -> &mut [u8] {
        debug_assert!(
            self.sealed == 0,
            "MaskedFrame::body_mut on a sealed frame (unseal first)"
        );
        &mut self.buf[Self::HDR_MAX..]
    }

    /// Writes the header (FIN = 1, MASK = 1, key bytes `mask_key.to_be_bytes()`) and masks
    /// `payload[..len]` in place. `len > CAP − 14` or an already sealed frame is a caller error:
    /// debug_assert, frame unchanged.
    #[inline]
    pub fn seal(&mut self, op: Opcode, len: usize, mask_key: u32) {
        self.seal_raw(op as u8, len, mask_key);
    }

    #[inline]
    pub(crate) fn seal_raw(&mut self, opcode: u8, len: usize, mask_key: u32) {
        debug_assert!(
            self.sealed == 0,
            "MaskedFrame::seal on a sealed frame (unseal first)"
        );
        debug_assert!(
            len <= CAP - Self::HDR_MAX,
            "MaskedFrame::seal: len exceeds CAP - 14"
        );
        if self.sealed != 0 || len > CAP - Self::HDR_MAX {
            return;
        }
        let key = mask_key.to_be_bytes();
        if let Some(p) = self.buf.get_mut(Self::HDR_MAX..Self::HDR_MAX + len) {
            apply_mask(p, key);
        }
        let hl = write_client_head(&mut self.buf, Self::HDR_MAX, opcode, len, key);
        self.start = (Self::HDR_MAX - hl) as u16;
        self.len = (hl + len) as u16;
        self.opcode = opcode;
        self.sealed = 1;
    }

    /// Header + masked payload (empty before `seal`).
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        let s = self.start as usize;
        let e = s + self.len as usize;
        self.buf.get(s..e).unwrap_or(&[])
    }

    /// Call before reuse.
    #[inline]
    pub fn unseal(&mut self) {
        self.sealed = 0;
        self.start = Self::HDR_MAX as u16;
        self.len = 0;
    }

    #[inline]
    pub fn is_sealed(&self) -> bool {
        self.sealed != 0
    }

    /// `sealed` field (0 or 1).
    #[inline]
    pub fn sealed(&self) -> u8 {
        self.sealed
    }

    /// `start` field.
    #[inline]
    pub fn start(&self) -> u16 {
        self.start
    }
}

/// Field offsets of `MaskedFrame<CAP>`: `start`, `len`, `opcode`, `sealed`, `_pad`, `buf`.
pub(crate) fn masked_frame_offsets<const CAP: usize>() -> [usize; 6] {
    [
        std::mem::offset_of!(MaskedFrame<CAP>, start),
        std::mem::offset_of!(MaskedFrame<CAP>, len),
        std::mem::offset_of!(MaskedFrame<CAP>, opcode),
        std::mem::offset_of!(MaskedFrame<CAP>, sealed),
        std::mem::offset_of!(MaskedFrame<CAP>, _pad),
        std::mem::offset_of!(MaskedFrame<CAP>, buf),
    ]
}
