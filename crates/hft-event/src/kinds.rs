//! `EventKind` numbers, the `KINDS` registry (`KindInfo`), `kind_info` and the measurement mask.

use crate::bytes::{u16_at, u32_at};
use crate::schema::{FieldDesc, FieldTy};

/// Kind number of a record (`RecordHeader.kind`); numbers are never reused.
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct EventKind(pub u16);

impl EventKind {
    pub const BBO: Self = EventKind(0x0101);
    pub const L2_DELTA: Self = EventKind(0x0102);
    pub const L2_SNAPSHOT: Self = EventKind(0x0103);
    pub const TRADE: Self = EventKind(0x0104);
    pub const FUNDING: Self = EventKind(0x0105);
    pub const MARK_PRICE: Self = EventKind(0x0106);
    pub const INDEX_PRICE: Self = EventKind(0x0107);
    pub const LIQUIDATION: Self = EventKind(0x0108);
    pub const L2_BATCH: Self = EventKind(0x0109);
    pub const ORDER_DECISION: Self = EventKind(0x0201);
    pub const SEND_REPORT: Self = EventKind(0x0202);
    pub const ORDER_EVENT: Self = EventKind(0x0203);
    pub const POSITION_REPORT: Self = EventKind(0x0204);
    pub const BALANCE_REPORT: Self = EventKind(0x0205);
    pub const FUNDING_SETTLE: Self = EventKind(0x0206);
    pub const EXTERNAL_FILL: Self = EventKind(0x0207);
    pub const MARGIN_REPORT: Self = EventKind(0x0208);
    pub const ACCOUNT_CONFIG_REPORT: Self = EventKind(0x0209);
    pub const EXEC_DECISION: Self = EventKind(0x020A);
    pub const CHAIN_HEAD: Self = EventKind(0x0301);
    pub const POOL_QUOTE: Self = EventKind(0x0302);
    pub const SWAP_DECISION: Self = EventKind(0x0303);
    pub const TX_LIFECYCLE: Self = EventKind(0x0304);
    pub const CONTROL_CMD: Self = EventKind(0x0401);
    pub const CMD_RESULT: Self = EventKind(0x0402);
    pub const CONFIG_BLOB: Self = EventKind(0x0403);
    pub const RECONCILE_REPORT: Self = EventKind(0x0404);
    pub const TIMER: Self = EventKind(0x0501);
    pub const CONN_STATE: Self = EventKind(0x0502);
    pub const ADVICE: Self = EventKind(0x0503);
    pub const STATE_SNAPSHOT: Self = EventKind(0x0504);
    pub const LATENCY_SAMPLE: Self = EventKind(0x0505);
    pub const NOTE: Self = EventKind(0x0506);
    pub const HOST_PROFILE: Self = EventKind(0x0507);
    pub const CLOCK_STATUS: Self = EventKind(0x0508);
    pub const SEGMENT_ROLL: Self = EventKind(0x0509);
    pub const ENGINE_CLOCK: Self = EventKind(0x050A);
    pub const RAW_RX: Self = EventKind(0x0601);
    pub const RAW_TX: Self = EventKind(0x0602);
    /// Test kinds (feature `dev-kinds`; production readers reject `0x7Fxx`).
    pub const T_IN: Self = EventKind(0x7F01);
    pub const T_OUT: Self = EventKind(0x7F02);
    pub const T_META: Self = EventKind(0x7F03);
    pub const T_CRC: Self = EventKind(0x7F04);
    pub const T_MULTI: Self = EventKind(0x7F05);
    pub const T_VAR: Self = EventKind(0x7F06);
}

/// Kind domain (high byte of the number; `Test` = `0x7F`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Domain {
    Market,
    Exec,
    Chain,
    Control,
    System,
    Raw,
    Test,
}

/// Body layout of a kind.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Layout {
    /// Fixed body of `size` bytes.
    Fixed { size: u16 },
    /// Single record: prefix + tail (count in the prefix), tail ≤ `max`; no `PartHdr`.
    Var { prefix: u16, max: u32 },
    /// Starts with `PartHdr`, may be split; the logical body starts with a `prefix`-byte header.
    Multi { prefix: u16 },
}

/// Record class (`ByBody`: decided by a body field, see `classify`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Role {
    Input,
    Output,
    Meta,
    ByBody,
}

/// Journals a kind may appear in.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct JournalMask(pub u8);

impl JournalMask {
    pub const SEQ: Self = JournalMask(1);
    pub const RAW: Self = JournalMask(2);
    pub const CHAIN: Self = JournalMask(4);

    pub const fn union(self, o: Self) -> Self {
        JournalMask(self.0 | o.0)
    }

    pub const fn contains(self, o: Self) -> bool {
        self.0 & o.0 == o.0
    }
}

impl core::ops::BitOr for JournalMask {
    type Output = Self;
    fn bitor(self, o: Self) -> Self {
        self.union(o)
    }
}

/// Whether the kind carries a CRC in the seq journal.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum CrcPolicy {
    Never,
    Always,
}

/// Registry entry of one kind.
#[derive(Copy, Clone, Debug)]
pub struct KindInfo {
    pub kind: EventKind,
    pub name: &'static str,
    pub domain: Domain,
    pub layout: Layout,
    pub schema_version: u16,
    pub role: Role,
    pub journals: JournalMask,
    pub crc: CrcPolicy,
    pub optional: bool,
    /// Fixed: body fields; Var/Multi: prefix fields.
    pub fields: &'static [FieldDesc],
}

const SEQ: JournalMask = JournalMask::SEQ;
const SEQ_CHAIN: JournalMask = JournalMask::SEQ.union(JournalMask::CHAIN);
const ALL_J: JournalMask = JournalMask::SEQ.union(JournalMask::RAW).union(JournalMask::CHAIN);

#[allow(clippy::too_many_arguments)]
const fn k(
    kind: u16,
    name: &'static str,
    domain: Domain,
    layout: Layout,
    schema_version: u16,
    role: Role,
    journals: JournalMask,
    crc: CrcPolicy,
    fields: &'static [FieldDesc],
) -> KindInfo {
    KindInfo {
        kind: EventKind(kind),
        name,
        domain,
        layout,
        schema_version,
        role,
        journals,
        crc,
        optional: kind >= 0x0301 && kind <= 0x0304,
        fields,
    }
}

const fn fixed(size: u16) -> Layout {
    Layout::Fixed { size }
}
const fn var(prefix: u16, max: u32) -> Layout {
    Layout::Var { prefix, max }
}
const fn multi(prefix: u16) -> Layout {
    Layout::Multi { prefix }
}

const fn bytes_field(n: u16) -> FieldDesc {
    FieldDesc {
        name: "bytes",
        offset: 0,
        ty: FieldTy::Bytes(n),
        measure: false,
        doc: "opaque test body",
    }
}
const T_IN_FIELDS: &[FieldDesc] = &[bytes_field(80)];
const T_OUT_FIELDS: &[FieldDesc] = &[bytes_field(56)];
const T_META_FIELDS: &[FieldDesc] = &[bytes_field(32)];
const T_CRC_FIELDS: &[FieldDesc] = &[bytes_field(40)];
const T_VAR_FIELDS: &[FieldDesc] = &[
    FieldDesc {
        name: "len",
        offset: 0,
        ty: FieldTy::U32,
        measure: false,
        doc: "tail length L",
    },
    FieldDesc {
        name: "bytes",
        offset: 4,
        ty: FieldTy::Bytes(12),
        measure: false,
        doc: "opaque",
    },
];

use crate::chain::{ChainHead, PoolQuote, SwapDecision, TxLifecycle};
use crate::cmd::CmdHdr;
use crate::control::{CmdResult, ReconcileReport};
use crate::exec::{
    AccountConfigReportHdr, BalanceReport, ExecDecision, ExternalFill, FundingSettle, MarginReportHdr,
    OrderDecision, OrderEvent, PositionReport, SendReport,
};
use crate::market::{Bbo, Funding, IndexPrice, L2BatchHdr, L2Delta, Liquidation, MarkPrice, Trade};
use crate::strategy::AdviceHdr;
use crate::system::{
    ClockStatus, ConnState, EngineClock, HostProfile, LatencySample, Note, SegmentRoll, Timer,
};
use crate::var::{BlobHdr, L2SnapHdr, RawHdr, SnapshotHdr};
use CrcPolicy::{Always, Never};
use Domain::*;
use Role::{ByBody, Input, Meta, Output};

const N_BASE: usize = 39;
const N_ALL: usize = 45;

const KINDS_ALL: [KindInfo; N_ALL] = [
    k(
        0x0101,
        "Bbo",
        Market,
        fixed(96),
        1,
        Input,
        SEQ,
        Never,
        Bbo::FIELDS,
    ),
    k(
        0x0102,
        "L2Delta",
        Market,
        fixed(96),
        1,
        Input,
        SEQ,
        Never,
        L2Delta::FIELDS,
    ),
    k(
        0x0103,
        "L2Snapshot",
        Market,
        multi(72),
        1,
        ByBody,
        SEQ,
        Never,
        L2SnapHdr::FIELDS,
    ),
    k(
        0x0104,
        "Trade",
        Market,
        fixed(88),
        1,
        Input,
        SEQ,
        Never,
        Trade::FIELDS,
    ),
    k(
        0x0105,
        "Funding",
        Market,
        fixed(80),
        1,
        Input,
        SEQ,
        Never,
        Funding::FIELDS,
    ),
    k(
        0x0106,
        "MarkPrice",
        Market,
        fixed(64),
        1,
        Input,
        SEQ,
        Never,
        MarkPrice::FIELDS,
    ),
    k(
        0x0107,
        "IndexPrice",
        Market,
        fixed(64),
        1,
        Input,
        SEQ,
        Never,
        IndexPrice::FIELDS,
    ),
    k(
        0x0108,
        "Liquidation",
        Market,
        fixed(80),
        1,
        Input,
        SEQ,
        Never,
        Liquidation::FIELDS,
    ),
    k(
        0x0109,
        "L2Batch",
        Market,
        var(80, 64_000),
        1,
        Input,
        SEQ,
        Never,
        L2BatchHdr::FIELDS,
    ),
    k(
        0x0201,
        "OrderDecision",
        Exec,
        fixed(88),
        1,
        Output,
        SEQ,
        Never,
        OrderDecision::FIELDS,
    ),
    k(
        0x0202,
        "SendReport",
        Exec,
        fixed(48),
        1,
        Input,
        SEQ,
        Never,
        SendReport::FIELDS,
    ),
    k(
        0x0203,
        "OrderEvent",
        Exec,
        fixed(96),
        1,
        Input,
        SEQ,
        Never,
        OrderEvent::FIELDS,
    ),
    k(
        0x0204,
        "PositionReport",
        Exec,
        fixed(56),
        1,
        Input,
        SEQ,
        Never,
        PositionReport::FIELDS,
    ),
    k(
        0x0205,
        "BalanceReport",
        Exec,
        fixed(40),
        1,
        Input,
        SEQ,
        Never,
        BalanceReport::FIELDS,
    ),
    k(
        0x0206,
        "FundingSettle",
        Exec,
        fixed(56),
        2,
        ByBody,
        SEQ,
        Never,
        FundingSettle::FIELDS,
    ),
    k(
        0x0207,
        "ExternalFill",
        Exec,
        fixed(80),
        2,
        Input,
        SEQ,
        Never,
        ExternalFill::FIELDS,
    ),
    k(
        0x0208,
        "MarginReport",
        Exec,
        var(64, 16_384),
        1,
        Input,
        SEQ,
        Never,
        MarginReportHdr::FIELDS,
    ),
    k(
        0x0209,
        "AccountConfigReport",
        Exec,
        var(64, 16_384),
        1,
        Input,
        SEQ,
        Never,
        AccountConfigReportHdr::FIELDS,
    ),
    k(
        0x020A,
        "ExecDecision",
        Exec,
        fixed(104),
        1,
        Output,
        SEQ,
        Never,
        ExecDecision::FIELDS,
    ),
    k(
        0x0301,
        "ChainHead",
        Chain,
        fixed(80),
        1,
        Input,
        SEQ,
        Never,
        ChainHead::FIELDS,
    ),
    k(
        0x0302,
        "PoolQuote",
        Chain,
        fixed(96),
        1,
        Input,
        SEQ,
        Never,
        PoolQuote::FIELDS,
    ),
    k(
        0x0303,
        "SwapDecision",
        Chain,
        fixed(128),
        1,
        Output,
        SEQ,
        Never,
        SwapDecision::FIELDS,
    ),
    k(
        0x0304,
        "TxLifecycle",
        Chain,
        fixed(152),
        1,
        Input,
        SEQ,
        Never,
        TxLifecycle::FIELDS,
    ),
    k(
        0x0401,
        "ControlCmd",
        Control,
        multi(128),
        1,
        Input,
        SEQ,
        Always,
        CmdHdr::FIELDS,
    ),
    k(
        0x0402,
        "CmdResult",
        Control,
        fixed(144),
        3,
        Output,
        SEQ,
        Never,
        CmdResult::FIELDS,
    ),
    k(
        0x0403,
        "ConfigBlob",
        Control,
        multi(72),
        1,
        ByBody,
        SEQ_CHAIN,
        Always,
        BlobHdr::FIELDS,
    ),
    k(
        0x0404,
        "ReconcileReport",
        Control,
        fixed(104),
        3,
        Input,
        SEQ,
        Never,
        ReconcileReport::FIELDS,
    ),
    k(
        0x0501,
        "Timer",
        System,
        fixed(24),
        1,
        Input,
        SEQ,
        Never,
        Timer::FIELDS,
    ),
    k(
        0x0502,
        "ConnState",
        System,
        fixed(24),
        2,
        Input,
        SEQ,
        Never,
        ConnState::FIELDS,
    ),
    k(
        0x0503,
        "Advice",
        System,
        var(32, 4_096),
        1,
        Input,
        SEQ,
        Never,
        AdviceHdr::FIELDS,
    ),
    k(
        0x0504,
        "StateSnapshot",
        System,
        multi(96),
        1,
        Output,
        SEQ_CHAIN,
        Always,
        SnapshotHdr::FIELDS,
    ),
    k(
        0x0505,
        "LatencySample",
        System,
        fixed(136),
        3,
        Meta,
        SEQ,
        Never,
        LatencySample::FIELDS,
    ),
    k(
        0x0506,
        "Note",
        System,
        fixed(80),
        1,
        ByBody,
        SEQ,
        Never,
        Note::FIELDS,
    ),
    k(
        0x0507,
        "HostProfile",
        System,
        fixed(176),
        1,
        Meta,
        SEQ_CHAIN,
        Never,
        HostProfile::FIELDS,
    ),
    k(
        0x0508,
        "ClockStatus",
        System,
        fixed(56),
        1,
        Meta,
        SEQ_CHAIN,
        Never,
        ClockStatus::FIELDS,
    ),
    k(
        0x0509,
        "SegmentRoll",
        System,
        fixed(48),
        1,
        Meta,
        ALL_J,
        Never,
        SegmentRoll::FIELDS,
    ),
    k(
        0x050A,
        "EngineClock",
        System,
        fixed(32),
        1,
        Input,
        SEQ,
        Never,
        EngineClock::FIELDS,
    ),
    k(
        0x0601,
        "RawRx",
        Raw,
        multi(24),
        1,
        Meta,
        JournalMask::RAW,
        Always,
        RawHdr::FIELDS,
    ),
    k(
        0x0602,
        "RawTx",
        Raw,
        multi(24),
        1,
        Meta,
        JournalMask::RAW,
        Always,
        RawHdr::FIELDS,
    ),
    k(
        0x7F01,
        "TIn",
        Test,
        fixed(80),
        1,
        Input,
        ALL_J,
        Never,
        T_IN_FIELDS,
    ),
    k(
        0x7F02,
        "TOut",
        Test,
        fixed(56),
        1,
        Output,
        ALL_J,
        Never,
        T_OUT_FIELDS,
    ),
    k(
        0x7F03,
        "TMeta",
        Test,
        fixed(32),
        1,
        Meta,
        ALL_J,
        Never,
        T_META_FIELDS,
    ),
    k(
        0x7F04,
        "TCrc",
        Test,
        fixed(40),
        1,
        Input,
        ALL_J,
        Always,
        T_CRC_FIELDS,
    ),
    k(0x7F05, "TMulti", Test, multi(0), 1, Input, ALL_J, Never, &[]),
    k(
        0x7F06,
        "TVar",
        Test,
        var(16, 65_000),
        1,
        Input,
        ALL_J,
        Never,
        T_VAR_FIELDS,
    ),
];

static KINDS_TABLE: [KindInfo; N_ALL] = KINDS_ALL;

/// Production kinds only (the `gen-schema` export).
pub(crate) static KINDS_BASE: &[KindInfo] = KINDS_TABLE.split_at(N_BASE).0;

/// Every registered kind, ascending by number; with feature `dev-kinds` also `0x7F01`–`0x7F06`.
pub static KINDS: &[KindInfo] = if cfg!(feature = "dev-kinds") {
    &KINDS_TABLE
} else {
    KINDS_BASE
};

/// `(first index in KINDS, count)` per domain high byte.
const fn domain_slot(hi: u16) -> Option<(usize, usize)> {
    match hi {
        0x01 => Some((0, 9)),
        0x02 => Some((9, 10)),
        0x03 => Some((19, 4)),
        0x04 => Some((23, 4)),
        0x05 => Some((27, 10)),
        0x06 => Some((37, 2)),
        0x7F if cfg!(feature = "dev-kinds") => Some((39, 6)),
        _ => None,
    }
}

/// Registry entry of `k`; unregistered → `None` (readers skip the record by `len`).
#[inline]
pub fn kind_info(k: EventKind) -> Option<&'static KindInfo> {
    let (base, n) = domain_slot(k.0 >> 8)?;
    let lo = (k.0 & 0xFF) as usize;
    if lo == 0 || lo > n {
        return None;
    }
    KINDS.get(base + lo - 1).filter(|i| i.kind == k)
}

/// Compile-time registration check used by `event_body!` (`form`: 0 fixed, 1 var, 2 multi).
/// Unregistered kinds pass.
#[doc(hidden)]
pub const fn registered_ok(kind: EventKind, form: u8, size: usize, max: usize, ver: u16) -> bool {
    let mut rest: &[KindInfo] = &KINDS_ALL;
    while let [e, tail @ ..] = rest {
        rest = tail;
        if e.kind.0 == kind.0 {
            if e.schema_version != ver {
                return false;
            }
            return match (form, e.layout) {
                (0, Layout::Fixed { size: s }) => s as usize == size,
                (1, Layout::Var { prefix, max: m }) => prefix as usize == size && m as usize == max,
                (2, Layout::Multi { prefix }) => prefix as usize == size,
                _ => false,
            };
        }
    }
    true
}

/// Tail length L of a Var kind from its prefix bytes (README "Var 尾部长度"); `None` for other kinds.
pub(crate) fn var_tail_len(kind: EventKind, prefix: &[u8]) -> Option<usize> {
    let h = |o: usize| usize::from(u16_at(prefix, o));
    match kind {
        EventKind::ADVICE => Some(h(8)),
        EventKind::MARGIN_REPORT => Some(16 * h(4)),
        EventKind::ACCOUNT_CONFIG_REPORT => Some(16 * h(6)),
        EventKind::L2_BATCH => Some(16 * (h(72) + h(74))),
        EventKind::T_VAR if cfg!(feature = "dev-kinds") => Some(u32_at(prefix, 0) as usize),
        _ => None,
    }
}

/// Bytes masked when the verifying replay compares Output records: `(kind, body offset, length)`;
/// Multi offsets are in the reassembled logical body. The header `tsc` (16..24) is masked on every
/// Output record and is not listed.
pub static MEASURE_MASK: &[(EventKind, u16, u16)] = &[
    (EventKind::ORDER_DECISION, 8, 8),
    (EventKind::SWAP_DECISION, 24, 8),
    (EventKind::STATE_SNAPSHOT, 56, 8),
    (EventKind::STATE_SNAPSHOT, 64, 16),
    (EventKind::STATE_SNAPSHOT, 80, 8),
];
