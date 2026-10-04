//! Shared helpers for the hft-event integration tests (only public API is used).
#![allow(dead_code)]

use hft_event::*;

pub mod canonical;

/// Registry entry by kind name.
pub fn info(name: &str) -> &'static KindInfo {
    KINDS
        .iter()
        .find(|k| k.name == name)
        .unwrap_or_else(|| panic!("kind {name}"))
}

/// Field of a field table by name (top level).
pub fn field(fields: &'static [FieldDesc], name: &str) -> &'static FieldDesc {
    fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("field {name}"))
}

/// Canonical bytes for a field table: field k (flattened, pads skipped) = k + 1 little-endian,
/// enum fields = largest legal code, `Bytes` filled with k + 1, pads 0.
pub fn canonical(fields: &[FieldDesc]) -> Vec<u8> {
    let size = fields.last().map_or(0, |l| (l.offset + l.ty.width()) as usize);
    let mut out = vec![0u8; size];
    let mut k = 0u64;
    fill(&mut out, 0, fields, &mut k);
    out
}

fn fill(out: &mut [u8], base: usize, fields: &[FieldDesc], k: &mut u64) {
    for f in fields {
        let o = base + f.offset as usize;
        let w = f.ty.width() as usize;
        match f.ty {
            FieldTy::Pad(_) => continue,
            FieldTy::Nested(inner) => {
                fill(out, o, inner, k);
                continue;
            }
            _ => {}
        }
        *k += 1;
        match f.ty {
            FieldTy::Enum(d) => {
                let v = d.items.iter().map(|i| i.value).max().unwrap_or(0) as u64;
                out[o..o + w].copy_from_slice(&v.to_le_bytes()[..w]);
            }
            FieldTy::Bytes(_) => out[o..o + w].fill(*k as u8),
            _ => out[o..o + w].copy_from_slice(&k.to_le_bytes()[..w]),
        }
    }
}

/// Builds a raw record (header + body + zero padding + optional CRC) without the typed encoders.
pub fn raw_record(kind: u16, flags: u16, seq: u64, tsc: u64, body: &[u8]) -> Vec<u8> {
    let crc = flags & RecordFlags::CRC.0 != 0;
    let n = record_len(body.len(), crc) as usize;
    let mut r = vec![0u8; n];
    r[0..4].copy_from_slice(&(n as u32).to_le_bytes());
    r[4..6].copy_from_slice(&kind.to_le_bytes());
    r[6..8].copy_from_slice(&flags.to_le_bytes());
    r[8..16].copy_from_slice(&seq.to_le_bytes());
    r[16..24].copy_from_slice(&tsc.to_le_bytes());
    r[24..24 + body.len()].copy_from_slice(body);
    if crc {
        let c = crc32c::crc32c(&r[..n - 4]);
        r[n - 4..].copy_from_slice(&c.to_le_bytes());
    }
    r
}

/// Same with an explicit `len` field (for malformed records).
pub fn with_len(mut r: Vec<u8>, len: u32) -> Vec<u8> {
    r[0..4].copy_from_slice(&len.to_le_bytes());
    r
}

/// 8-byte aligned copy of `bytes` (the returned Vec<u64> owns the storage).
pub struct Aligned(Vec<u64>, usize);
impl Aligned {
    pub fn new(bytes: &[u8]) -> Self {
        let mut v = vec![0u64; bytes.len().div_ceil(8) + 1];
        let b: &mut [u8] = zerocopy::IntoBytes::as_mut_bytes(v.as_mut_slice());
        b[..bytes.len()].copy_from_slice(bytes);
        Aligned(v, bytes.len())
    }
    pub fn bytes(&self) -> &[u8] {
        &zerocopy::IntoBytes::as_bytes(self.0.as_slice())[..self.1]
    }
}

pub fn bbo() -> Bbo {
    let mut b: Bbo = zerocopy::FromBytes::read_from_bytes(&canonical(Bbo::FIELDS)[..]).unwrap();
    b.h._rsv = 0;
    b
}

pub fn order_decision() -> OrderDecision {
    let mut od: OrderDecision =
        zerocopy::TryFromBytes::try_read_from_bytes(&canonical(OrderDecision::FIELDS)[..]).unwrap();
    od.oflags = 0x30;
    od
}

pub fn timer() -> Timer {
    Timer {
        timer_id: 1,
        owner: 1,
        _pad: 0,
        group: hft_types::GroupId(0),
        _pad2: 0,
        deadline_tsc: 77,
    }
}

/// Encodes a fixed body into a fresh Vec.
pub fn enc<B: Body>(b: &B, seq: u64, tsc: u64, flags: RecordFlags) -> Vec<u8> {
    let mut v = vec![0u8; 512];
    let n = encode_fixed(&mut v, seq, tsc, flags, b) as usize;
    v.truncate(n);
    v
}
