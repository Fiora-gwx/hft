//! `ClientOrderId` ↔ canonical text (24 lowercase hex characters).
//!
//! 96 bits, most significant first: `ver:4 | epoch:16 | venue:8 | account:8 | slot:20 | gen:32 |
//! check:8`, `ver = 1`, `check` = XOR of the first 11 bytes.

use crate::types::{AccountId, VenueId};

/// Logical client order id (field semantics belong to hft-oms).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ClientOrderId {
    /// Bit 15 is reserved (always encoded as 0); low 15 bits are the engine epoch.
    pub epoch: u16,
    pub venue: VenueId,
    pub account: AccountId,
    /// Low 20 bits.
    pub slot: u32,
    pub r#gen: u32,
}

/// Decode error; with several faults the first of Length → Charset → Version → Check → Reserved.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum CidErr {
    Length = 1,
    /// Not lowercase hex.
    Charset = 2,
    Version = 3,
    Check = 4,
    /// Reserved bit set.
    Reserved = 5,
}
const _: () = assert!(size_of::<CidErr>() == 1);

const VERSION: u8 = 1;
const SLOT_MASK: u32 = (1 << 20) - 1;
const HEX: &[u8; 16] = b"0123456789abcdef";

/// Lowercase hex digit value, `None` for anything else.
#[inline]
fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(c.wrapping_sub(b'a').wrapping_add(10)),
        _ => None,
    }
}

impl ClientOrderId {
    /// Always encoded as 0 (input bit ignored); decoding a 1 → `CidErr::Reserved`.
    pub const RESERVED_BIT: u16 = 1 << 15;

    /// Render the canonical text; `slot ≥ 2^20` is a caller error (debug_assert; release
    /// encodes the low 20 bits).
    pub fn encode(&self, out: &mut [u8; 24]) {
        debug_assert!(self.slot <= SLOT_MASK, "slot exceeds 20 bits");
        let epoch = self.epoch & !Self::RESERVED_BIT;
        let slot = self.slot & SLOT_MASK;
        let e = epoch.to_be_bytes();
        let s = slot.to_be_bytes(); // s[0] == 0; 20 bits in s[1] (low nibble) .. s[3]
        let g = self.r#gen.to_be_bytes();
        let mut b = [
            (VERSION << 4) | (e[0] >> 4),
            (e[0] << 4) | (e[1] >> 4),
            (e[1] << 4) | (self.venue.0 >> 4),
            (self.venue.0 << 4) | (self.account.0 >> 4),
            (self.account.0 << 4) | (s[1] & 0x0F),
            s[2],
            s[3],
            g[0],
            g[1],
            g[2],
            g[3],
            0,
        ];
        b[11] = b[..11].iter().fold(0, |a, x| a ^ x);
        for (pair, byte) in out.chunks_exact_mut(2).zip(b.iter()) {
            if let [hi, lo] = pair {
                *hi = HEX[usize::from(byte >> 4)];
                *lo = HEX[usize::from(byte & 0x0F)];
            }
        }
    }

    /// Parse the canonical text; does not judge epoch, venue or account.
    pub fn decode(text: &[u8]) -> Result<ClientOrderId, CidErr> {
        let Ok(text) = <&[u8; 24]>::try_from(text) else {
            return Err(CidErr::Length);
        };
        let mut b = [0u8; 12];
        for (dst, pair) in b.iter_mut().zip(text.chunks_exact(2)) {
            let (Some(hi), Some(lo)) = (
                pair.first().copied().and_then(hex_val),
                pair.get(1).copied().and_then(hex_val),
            ) else {
                return Err(CidErr::Charset);
            };
            *dst = (hi << 4) | lo;
        }
        if b[0] >> 4 != VERSION {
            return Err(CidErr::Version);
        }
        if b[..11].iter().fold(0, |a, x| a ^ x) != b[11] {
            return Err(CidErr::Check);
        }
        let epoch = u16::from_be_bytes([(b[0] << 4) | (b[1] >> 4), (b[1] << 4) | (b[2] >> 4)]);
        if epoch & Self::RESERVED_BIT != 0 {
            return Err(CidErr::Reserved);
        }
        let venue = (b[2] << 4) | (b[3] >> 4);
        let account = (b[3] << 4) | (b[4] >> 4);
        let slot = u32::from_be_bytes([0, b[4] & 0x0F, b[5], b[6]]);
        let r#gen = u32::from_be_bytes([b[7], b[8], b[9], b[10]]);
        Ok(ClientOrderId {
            epoch,
            venue: VenueId(venue),
            account: AccountId(account),
            slot,
            r#gen,
        })
    }
}
