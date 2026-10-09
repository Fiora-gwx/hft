//! Advice payloads (README "Advice 载荷"): `AdvicePayload`, `PayloadWriter`/`PayloadReader`,
//! the per-`Model::on_event` output `AdviceOut` and its buffers `AdviceBufs`.
//!
//! Payloads are strategy-defined, integer fixed point only, first byte = version of that
//! `local_kind`; the engine never interprets them. The advisor fills `basis_seq`/`basis_tsc`.

use hft_types::{GROUP_ALL, GroupId};

/// Largest payload of one Advice (= hft-event `ADVICE_MAX_PAYLOAD`).
pub const ADVICE_MAX_LEN: usize = 4096;
const _: () = assert!(ADVICE_MAX_LEN == hft_event::ADVICE_MAX_PAYLOAD);
/// Advices per `Model::on_event`.
pub const ADVICE_CAP: usize = 128;
/// Payload bytes per `Model::on_event`.
pub const ADVICE_CAP_BYTES: usize = 128 * 1024;

/// Whether `k` is a reserved `local_kind` (0 and `0xFF00..=0xFFFF`), for Advice and Warmup alike.
pub const fn local_kind_reserved(k: u16) -> bool {
    k == 0 || k >= 0xFF00
}

/// The payload does not fit its `MAX_LEN` (or the writer's buffer).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PayloadFull;

/// Payload decode failure; the fast part treats it as "no data".
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum PayloadError {
    /// Shorter than its layout.
    Truncated,
    /// Unknown first-byte version.
    Version(u8),
    /// A field value outside its domain.
    BadValue,
    /// Bytes after the layout.
    TrailingBytes,
}

/// The `AdviceOut` capacity is used up, or the advice is invalid (reserved `local_kind`, payload
/// over 4096 B, `group = GROUP_ALL`); not written, counted.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct AdviceFull;

/// A strategy's Advice payload type.
pub trait AdvicePayload: Sized {
    /// Unique within the strategy; 0 and `0xFF00..=0xFFFF` reserved (checked at compile time by
    /// `AdviceOut::emit`).
    const LOCAL_KIND: u16;
    /// ≤ 4096 (checked at compile time by `AdviceOut::emit`).
    const MAX_LEN: usize;
    fn encode(&self, w: &mut PayloadWriter<'_>) -> Result<(), PayloadFull>;
    /// Fast-part call: allocation-free, O(len).
    fn decode(b: &[u8]) -> Result<Self, PayloadError>;
}

/// Little-endian writer over a caller buffer, bounded by a limit.
#[derive(Debug)]
pub struct PayloadWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
    limit: usize,
    over: bool,
}

macro_rules! put_le {
    ($name:ident, $t:ty) => {
        pub fn $name(&mut self, v: $t) -> Result<(), PayloadFull> {
            self.put_bytes(&v.to_le_bytes())
        }
    };
}

impl<'a> PayloadWriter<'a> {
    /// Writes into `buf`, at most `min(limit, buf.len())` bytes.
    pub fn new(buf: &'a mut [u8], limit: usize) -> PayloadWriter<'a> {
        let limit = limit.min(buf.len());
        PayloadWriter {
            buf,
            len: 0,
            limit,
            over: false,
        }
    }
    /// Bytes written so far.
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Whether a write was refused (the payload is then incomplete and must not be used).
    pub fn overflowed(&self) -> bool {
        self.over
    }
    pub fn as_bytes(&self) -> &[u8] {
        self.buf.get(..self.len).unwrap_or(&[])
    }
    pub fn put_bytes(&mut self, b: &[u8]) -> Result<(), PayloadFull> {
        let end = self.len.checked_add(b.len()).filter(|&e| e <= self.limit);
        match end.and_then(|e| self.buf.get_mut(self.len..e)) {
            Some(d) => {
                d.copy_from_slice(b);
                self.len += b.len();
                Ok(())
            }
            None => {
                self.over = true;
                Err(PayloadFull)
            }
        }
    }
    put_le!(put_u8, u8);
    put_le!(put_u16, u16);
    put_le!(put_u32, u32);
    put_le!(put_u64, u64);
    put_le!(put_i8, i8);
    put_le!(put_i16, i16);
    put_le!(put_i32, i32);
    put_le!(put_i64, i64);
    put_le!(put_i128, i128);
}

/// Little-endian reader for `AdvicePayload::decode` (allocation-free).
#[derive(Copy, Clone, Debug)]
pub struct PayloadReader<'a> {
    b: &'a [u8],
    pos: usize,
}

macro_rules! get_le {
    ($name:ident, $t:ty) => {
        pub fn $name(&mut self) -> Result<$t, PayloadError> {
            const N: usize = core::mem::size_of::<$t>();
            let s = self.get_bytes(N)?;
            let mut a = [0u8; N];
            a.copy_from_slice(s);
            Ok(<$t>::from_le_bytes(a))
        }
    };
}

impl<'a> PayloadReader<'a> {
    pub const fn new(b: &'a [u8]) -> PayloadReader<'a> {
        PayloadReader { b, pos: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.b.len().saturating_sub(self.pos)
    }
    pub fn get_bytes(&mut self, n: usize) -> Result<&'a [u8], PayloadError> {
        let end = self.pos.checked_add(n).ok_or(PayloadError::Truncated)?;
        let s = self.b.get(self.pos..end).ok_or(PayloadError::Truncated)?;
        self.pos = end;
        Ok(s)
    }
    /// `Err(TrailingBytes)` unless every byte was read.
    pub fn finish(&self) -> Result<(), PayloadError> {
        if self.pos == self.b.len() {
            Ok(())
        } else {
            Err(PayloadError::TrailingBytes)
        }
    }
    get_le!(get_u8, u8);
    get_le!(get_u16, u16);
    get_le!(get_u32, u32);
    get_le!(get_u64, u64);
    get_le!(get_i8, i8);
    get_le!(get_i16, i16);
    get_le!(get_i32, i32);
    get_le!(get_i64, i64);
    get_le!(get_i128, i128);
}

/// One emitted Advice (payload at `bytes[off..off + len]` of the buffers).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct AdviceEntry {
    pub group: GroupId,
    pub local_kind: u16,
    pub off: u32,
    pub len: u32,
}

/// Advisor-owned output buffers of one Model (128 entries, 128 KiB; allocated once).
#[derive(Clone, Debug)]
pub struct AdviceBufs {
    entries: Vec<AdviceEntry>,
    bytes: Vec<u8>,
    used: usize,
    full: u32,
}

impl AdviceBufs {
    pub fn new() -> AdviceBufs {
        AdviceBufs {
            entries: Vec::with_capacity(ADVICE_CAP),
            bytes: vec![0; ADVICE_CAP_BYTES],
            used: 0,
            full: 0,
        }
    }
    /// Clears the previous output (before a `Model::on_event` call).
    pub fn begin(&mut self) {
        self.entries.clear();
        self.used = 0;
        self.full = 0;
    }
    pub fn out(&mut self) -> AdviceOut<'_> {
        AdviceOut { b: self }
    }
    /// `(group, local_kind, payload)` in emit order.
    pub fn iter(&self) -> impl Iterator<Item = (GroupId, u16, &[u8])> + '_ {
        self.entries.iter().map(move |e| {
            let s = e.off as usize;
            let p = self.bytes.get(s..s + e.len as usize).unwrap_or(&[]);
            (e.group, e.local_kind, p)
        })
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// `AdviceFull` count of the last call.
    pub fn full_count(&self) -> u32 {
        self.full
    }
}

impl Default for AdviceBufs {
    fn default() -> Self {
        AdviceBufs::new()
    }
}

/// Output of one `Model::on_event` call.
#[derive(Debug)]
pub struct AdviceOut<'a> {
    b: &'a mut AdviceBufs,
}

struct Check<P>(core::marker::PhantomData<P>);
impl<P: AdvicePayload> Check<P> {
    const OK: () = {
        assert!(
            !local_kind_reserved(P::LOCAL_KIND),
            "AdvicePayload::LOCAL_KIND is reserved"
        );
        assert!(
            P::MAX_LEN <= ADVICE_MAX_LEN,
            "AdvicePayload::MAX_LEN exceeds 4096"
        );
    };
}

impl AdviceOut<'_> {
    fn reject(&mut self) -> AdviceFull {
        self.b.full = self.b.full.saturating_add(1);
        AdviceFull
    }
    /// Encodes `p` for `group` (`GROUP_NONE` = not for a group; `GROUP_ALL` invalid).
    pub fn emit<P: AdvicePayload>(&mut self, group: GroupId, p: &P) -> Result<(), AdviceFull> {
        #[allow(clippy::let_unit_value)]
        let () = Check::<P>::OK;
        if group == GROUP_ALL || self.b.entries.len() >= ADVICE_CAP {
            return Err(self.reject());
        }
        let start = self.b.used;
        let room = ADVICE_CAP_BYTES.saturating_sub(start).min(P::MAX_LEN);
        let len = {
            let Some(buf) = self.b.bytes.get_mut(start..start + room) else {
                return Err(self.reject());
            };
            let mut w = PayloadWriter::new(buf, room);
            if p.encode(&mut w).is_err() || w.overflowed() {
                return Err(self.reject());
            }
            w.len()
        };
        self.commit(group, P::LOCAL_KIND, start, len)
    }
    /// Copies raw payload bytes (reserved `local_kind`, > 4096 B or `GROUP_ALL` → `AdviceFull`).
    pub fn emit_raw(&mut self, group: GroupId, local_kind: u16, bytes: &[u8]) -> Result<(), AdviceFull> {
        if group == GROUP_ALL
            || local_kind_reserved(local_kind)
            || bytes.len() > ADVICE_MAX_LEN
            || self.b.entries.len() >= ADVICE_CAP
        {
            return Err(self.reject());
        }
        let start = self.b.used;
        let Some(d) = self.b.bytes.get_mut(start..start + bytes.len()) else {
            return Err(self.reject());
        };
        d.copy_from_slice(bytes);
        self.commit(group, local_kind, start, bytes.len())
    }
    fn commit(
        &mut self,
        group: GroupId,
        local_kind: u16,
        start: usize,
        len: usize,
    ) -> Result<(), AdviceFull> {
        let (Ok(off), Ok(l)) = (u32::try_from(start), u32::try_from(len)) else {
            return Err(self.reject());
        };
        self.b.entries.push(AdviceEntry {
            group,
            local_kind,
            off,
            len: l,
        });
        self.b.used = start + len;
        Ok(())
    }
}
