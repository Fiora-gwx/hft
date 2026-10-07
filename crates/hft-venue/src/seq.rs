//! `InstSeq` and the forward / duplicate / gap decision (README §3).

use hft_venue_proto::{SeqIds, SeqRule, StreamKind};

/// One cache line per inst (dense array per feed task, local inst index).
#[repr(C, align(64))]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct InstSeq {
    /// Per stream: BBO / depth (TopN or incremental) / Trade / Mark / Index / Funding — sequence or
    /// exchange time last forwarded.
    pub last_fwd: [u64; 6],
    /// `L2_*` state.
    pub l2_state: u8,
    /// Resync slot held (incremental depth), `0xFF` = none.
    pub resync_slot: u8,
    pub _pad: [u8; 2],
    pub resync_n: u32,
    pub _pad2: [u8; 8],
}
const _: () = assert!(core::mem::size_of::<InstSeq>() == 64);
const _: () = assert!(core::mem::align_of::<InstSeq>() == 64);
const _: () = assert!(core::mem::offset_of!(InstSeq, l2_state) == 48);
const _: () = assert!(core::mem::offset_of!(InstSeq, resync_n) == 52);

impl InstSeq {
    /// `l2_state` values (the last three only with incremental depth).
    pub const L2_LIVE: u8 = 0;
    pub const L2_AWAIT_FIRST: u8 = 1;
    pub const L2_RESYNC: u8 = 2;
    pub const L2_AWAIT_SNAPSHOT: u8 = 3;
    pub const L2_ALIGNING: u8 = 4;
    pub const L2_DEGRADED: u8 = 5;

    pub fn new() -> InstSeq {
        InstSeq {
            resync_slot: 0xFF,
            ..InstSeq::default()
        }
    }
}

/// `last_fwd` index of a stream; Liquidation and DepthIncr share the closest slot.
pub fn slot_of(s: StreamKind) -> usize {
    match s {
        StreamKind::Bbo => 0,
        StreamKind::DepthTopN | StreamKind::DepthIncr => 1,
        StreamKind::Trade | StreamKind::Liquidation => 2,
        StreamKind::Mark => 3,
        StreamKind::Index => 4,
        StreamKind::Funding => 5,
    }
}

/// Decision on one message.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Verdict {
    Forward,
    Duplicate,
    Gap,
}

/// The README §3 table. `refreshing`: the inst is in BBO refresh (§3a step 5) — `last ≥ last_fwd`
/// forwards. `Chain` (optional extension) is decided on `prev_id`.
#[inline]
pub fn judge(rule: SeqRule, last_fwd: u64, ids: &SeqIds, refreshing: bool) -> Verdict {
    match rule {
        SeqRule::None => Verdict::Forward,
        SeqRule::Monotonic | SeqRule::Snapshot | SeqRule::ByTime => {
            if ids.last_id > last_fwd || (refreshing && ids.last_id == last_fwd) {
                Verdict::Forward
            } else {
                Verdict::Duplicate
            }
        }
        SeqRule::Chain => {
            if ids.last_id <= last_fwd {
                Verdict::Duplicate
            } else if ids.prev_id == last_fwd {
                Verdict::Forward
            } else {
                Verdict::Gap
            }
        }
    }
}
