//! `Note{source = Oms}` code table (names registered in hft-event; numbers `n` and `v[]` meanings
//! only here) and the Note constructor.

use hft_event::{Note, NoteLevel, NoteSource, note_code};
use hft_types::{GroupId, InstId, StrategyId};

/// One Oms Note code: name (hft-event registry), number `n`, default level.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct OmsNoteCode {
    pub name: &'static str,
    pub n: u32,
    pub level: NoteLevel,
}

macro_rules! codes {
    ($($c:ident = $n:literal, $name:literal, $lvl:ident;)*) => {
        $(pub const $c: OmsNoteCode = OmsNoteCode { name: $name, n: $n, level: NoteLevel::$lvl };)*
        /// Every Oms Note code in README "Note 码表" order.
        pub const ALL: &[OmsNoteCode] = &[$($c),*];
    };
}

codes! {
    TRANSITION = 1, "OmsTransition", Info;
    UNKNOWN = 2, "OmsUnknown", P1;
    UNKNOWN_STUCK = 3, "OmsUnknownStuck", P0;
    VENUE_READY = 4, "OmsVenueReady", Info;
    NEEDS_REST_QUERY = 5, "OmsNeedsRestQuery", Info;
    STALE_GEN_FILL = 6, "OmsStaleGenFill", P0;
    LATE_FILL = 7, "OmsLateFill", P0;
    OVERFILL = 8, "OmsOverfill", P0;
    FOREIGN_ORDER = 9, "OmsForeignOrder", P1;
    FOREIGN_FILL_VIA_ORDER_EVENT = 10, "OmsForeignFillViaOrderEvent", P1;
    FOREIGN_FILL = 11, "OmsForeignFill", Info;
    EXT_DEDUP = 12, "OmsExtDedup", P1;
    FILL_PX_ESTIMATED = 13, "OmsFillPxEstimated", P1;
    FEE_UNATTRIBUTED = 14, "OmsFeeUnattributed", P1;
    CANCEL_NOT_SENT = 15, "OmsCancelNotSent", P2;
    HEDGE_BROKEN = 16, "OmsHedgeBroken", P0;
    HEDGE_PX_STALE = 17, "OmsHedgePxStale", P1;
    HEDGE_PX_GATED = 18, "OmsHedgePxGated", P1;
    HEDGE_SLIP_CAPPED = 19, "OmsHedgeSlipCapped", P1;
    HEDGE_NAKED_CAPPED = 20, "OmsHedgeNakedCapped", P1;
    RESUME_RETRY = 21, "OmsResumeRetry", P1;
    RESUME_TIMEOUT = 22, "OmsResumeTimeout", P0;
    ANOMALY = 23, "OmsAnomaly", P1;
}

/// The same table in the form `cargo xtask gen-schema` scans (hft-event README "const 表的形式").
pub const NOTE_CODES: &[(NoteSource, u32, &str)] = &[
    (NoteSource::Oms, 1, "OmsTransition"),
    (NoteSource::Oms, 2, "OmsUnknown"),
    (NoteSource::Oms, 3, "OmsUnknownStuck"),
    (NoteSource::Oms, 4, "OmsVenueReady"),
    (NoteSource::Oms, 5, "OmsNeedsRestQuery"),
    (NoteSource::Oms, 6, "OmsStaleGenFill"),
    (NoteSource::Oms, 7, "OmsLateFill"),
    (NoteSource::Oms, 8, "OmsOverfill"),
    (NoteSource::Oms, 9, "OmsForeignOrder"),
    (NoteSource::Oms, 10, "OmsForeignFillViaOrderEvent"),
    (NoteSource::Oms, 11, "OmsForeignFill"),
    (NoteSource::Oms, 12, "OmsExtDedup"),
    (NoteSource::Oms, 13, "OmsFillPxEstimated"),
    (NoteSource::Oms, 14, "OmsFeeUnattributed"),
    (NoteSource::Oms, 15, "OmsCancelNotSent"),
    (NoteSource::Oms, 16, "OmsHedgeBroken"),
    (NoteSource::Oms, 17, "OmsHedgePxStale"),
    (NoteSource::Oms, 18, "OmsHedgePxGated"),
    (NoteSource::Oms, 19, "OmsHedgeSlipCapped"),
    (NoteSource::Oms, 20, "OmsHedgeNakedCapped"),
    (NoteSource::Oms, 21, "OmsResumeRetry"),
    (NoteSource::Oms, 22, "OmsResumeTimeout"),
    (NoteSource::Oms, 23, "OmsAnomaly"),
];

/// `Note.code` of an Oms code.
#[inline]
pub const fn code_of(c: OmsNoteCode) -> u32 {
    note_code(NoteSource::Oms, c.n)
}

/// Builds a `Note{source = Oms}` at the code's default level.
#[inline]
pub(crate) fn note(c: OmsNoteCode, strategy: StrategyId, group: GroupId, inst: InstId, v: [u64; 8]) -> Note {
    note_at(c, c.level, strategy, group, inst, v)
}

/// Builds a `Note{source = Oms}` at an explicit level.
#[inline]
pub(crate) fn note_at(
    c: OmsNoteCode,
    level: NoteLevel,
    strategy: StrategyId,
    group: GroupId,
    inst: InstId,
    v: [u64; 8],
) -> Note {
    Note {
        code: code_of(c),
        level,
        source: NoteSource::Oms,
        strategy,
        _pad: 0,
        group,
        inst,
        v,
    }
}
