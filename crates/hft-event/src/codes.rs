//! Code tables and bit sets of the wire fields that are plain integers (`u8`/`u16`/`u32`) or
//! hft-types enums, registered here so `FieldDesc.ty` and `gen-schema` can export them.
//! Codes are append-only; retired values are never listed again.

use crate::schema::{EnumDesc, EnumItem};

macro_rules! desc {
    ($(#[$m:meta])* $id:ident, $name:literal, $w:literal, [$($v:ident = $n:expr),* $(,)?]) => {
        $(#[$m])*
        pub static $id: EnumDesc = EnumDesc {
            name: $name,
            width: $w,
            items: &[$(EnumItem { name: stringify!($v), value: $n }),*],
        };
    };
}

// hft-types enums.
desc!(SIDE, "Side", 1, [Buy = 1, Sell = 2]);
desc!(TIF, "Tif", 1, [Gtc = 1, Ioc = 2, Fok = 3, PostOnly = 4]);
desc!(RUN_MODE, "RunMode", 1, [Shadow = 1, CloseOnly = 3, Live = 4]);
desc!(KILL_LEVEL, "KillLevel", 1, [Venue = 2, Account = 3, Global = 4]);
desc!(
    CAP_SOURCE,
    "CapSource",
    1,
    [
        Kill = 1,
        DailyLoss = 2,
        Recon = 3,
        UnknownTimeout = 4,
        Broken = 5,
        Reorg = 6,
        Recovery = 7,
        Halted = 8,
        Draining = 9,
        ClockBad = 10,
        HedgeFailure = 11,
        MarginStress = 12,
        Storage = 13,
        Delisting = 14,
        AccountCfg = 15,
    ]
);
desc!(
    /// `CmdKind` numbers (0x0005 reserved; 0x000B–0x000D retired).
    CMD_KIND, "CmdKind", 2, [
    SetMode = 0x0001, SetParams = 0x0002, SetRisk = 0x0003, Kill = 0x0004, RotateConn = 0x0006,
    Resubscribe = 0x0007, SnapshotNow = 0x0008, ReplaceRefdata = 0x0009, Resume = 0x000A, HedgeFix = 0x000E,
    CancelAll = 0x000F, Drain = 0x0010, RecordTransfer = 0x0011, Annotate = 0x0012, AdoptExchange = 0x0013,
    SimReset = 0x0014, SimTransfer = 0x0015, VenueNotice = 0x0016, SetBalanceBaseline = 0x0017,
    StrategyCmd = 0x0018, InjectWarmup = 0x0019, SmokeOrder = 0x001A, WriteOffResidual = 0x001B, SimFault = 0x001C,
    BindSlot = 0x001D, Settle = 0x001E,
]);

// Execution.
desc!(
    LINK_ROLE,
    "LinkRole",
    1,
    [
        None = 0,
        First = 1,
        Second = 2,
        SecondRetry = 3,
        HedgeFix = 4,
        Reverse = 5
    ]
);
desc!(
    ORDER_FLAGS,
    "OrderFlags",
    2,
    [
        REDUCE_ONLY = 1,
        POST_ONLY = 2,
        LINKED = 4,
        HEDGE_FIX = 8,
        RISK_REDUCING = 16,
        SMOKE = 32,
        POSITION_CLOSE = 64,
    ]
);
desc!(
    /// `OrderEvent.detail` for `what = QueryResult`; same codes as `ReconcileReport.order_status`.
    ORDER_STATUS, "OrderStatus", 1, [
    New = 1, PartiallyFilled = 2, Filled = 3, Cancelled = 4, Rejected = 5, Expired = 6, NotFound = 7,
]);
desc!(
    /// `OrderEvent.detail` for `what ∈ {CancelReject, AmendReject}`.
    REJECT_CLASS, "RejectClass", 1, [OTHER = 0, GONE = 1]);
desc!(MARGIN_SOURCE, "MarginSource", 1, [Push = 1, Poll = 2]);
desc!(
    MARGIN_MFLAGS,
    "MarginFlags",
    1,
    [MARGIN_CALL = 1, ITEMS_TRUNCATED = 2]
);
desc!(
    POSITION_MODE,
    "PositionMode",
    1,
    [NotApplicable = 1, OneWay = 2, Hedge = 3]
);
desc!(
    MARGIN_MODE,
    "MarginMode",
    1,
    [NotApplicable = 1, Cross = 2, Isolated = 3, Mixed = 4]
);
desc!(MULTI_ASSET, "MultiAsset", 1, [NotApplicable = 1, Off = 2, On = 3]);
desc!(
    ACR_SOURCE,
    "AccountConfigSource",
    1,
    [Startup = 1, Push = 2, Poll = 3]
);
desc!(
    CFG_MISMATCH,
    "CfgMismatch",
    4,
    [
        POS_MODE = 1,
        MARGIN_MODE = 2,
        MULTI_ASSET = 4,
        FEE_RULE = 8,
        ACCT_TYPE = 16,
        AUTO_BORROW = 32,
        KEY_PERM = 64,
        UID = 128,
        UNREADABLE = 256,
    ]
);
desc!(
    ACR_RFLAGS,
    "AccountConfigFlags",
    2,
    [PARTIAL = 1, LEV_TRUNCATED = 2]
);
desc!(UID_KEYS, "UidKeys", 1, [SESSION = 1, REST = 2]);
desc!(
    EXEC_ACTION,
    "ExecAction",
    1,
    [
        TargetSet = 1,
        TargetReplaced = 2,
        TargetCleared = 3,
        TargetRejected = 4,
        Slice = 5,
        State = 6,
        HedgeFix = 7,
        ResidualTrim = 8,
        ExternalSeen = 9,
        ExternalDefault = 10,
        ForceStep = 11,
        CmdHedgeFix = 12,
        ChainLegExcess = 13,
        AutoFix = 14,
    ]
);
desc!(
    EXEC_ALGO,
    "ExecAlgo",
    1,
    [
        None = 0,
        TakerSlice = 1,
        PairOnFill = 2,
        Simultaneous = 3,
        MakerFirst = 4,
        Engine = 5
    ]
);
desc!(
    TARGET_REASON,
    "TargetReason",
    1,
    [Rebalance = 1, Exit = 2, MarginStress = 3, Hard = 4, Manual = 5]
);
desc!(URGENCY, "Urgency", 1, [Passive = 1, Normal = 2, Urgent = 3]);
desc!(
    TARGET_STATE,
    "TargetState",
    1,
    [Idle = 1, Working = 2, Stuck = 3, Broken = 4]
);
desc!(EXEC_EFLAGS, "ExecFlags", 1, [SHADOW = 1, CLEARED_BY_ENGINE = 2]);

// Chain (optional).
desc!(
    CHAIN_HEAD_SOURCE,
    "ChainHeadSource",
    1,
    [Node = 1, BlockStream = 2, Rpc = 3]
);
desc!(
    CHAIN_HFLAGS,
    "ChainHeadFlags",
    1,
    [FINALITY_STALLED = 1, NODE_BEHIND = 2]
);
desc!(POOL_LEVEL, "PoolLevel", 1, [L0 = 1, L1 = 2]);
desc!(
    POOL_QFLAGS,
    "PoolQuoteFlags",
    1,
    [
        STALE = 1,
        HOOK_POOL = 2,
        MULTIHOP = 4,
        INDICATIVE = 8,
        NO_LIQ = 16
    ]
);
desc!(SWAP_EXACT, "SwapExact", 1, [ExactIn = 0, ExactOut = 1]);
desc!(HEDGE_PX_MODE, "HedgePxMode", 1, [Cross = 1, Mid = 2, LimitBp = 3]);
desc!(SWAP_SFLAGS, "SwapFlags", 1, [RISK_REDUCING = 1]);

// Control and system.
desc!(
    CMD_RFLAGS,
    "CmdResultFlags",
    4,
    [PREVIEW = 1, NEEDS_CONFIRM = 2, PREVIEW_TTL = 4]
);
desc!(ACCOUNT_KIND, "AccountKind", 1, [Live = 1, Sim = 2]);
desc!(
    RECON_ITEM,
    "ReconItem",
    1,
    [
        Position = 1,
        Balance = 2,
        OpenOrder = 3,
        UnknownResolved = 4,
        Summary = 5,
        Funding = 6,
    ]
);
desc!(RECON_SEVERITY, "ReconSeverity", 1, [Ignore = 1, Warn = 2]);
desc!(
    RECON_ORDER_STATUS,
    "ReconOrderStatus",
    1,
    [
        Other = 0,
        New = 1,
        PartiallyFilled = 2,
        Filled = 3,
        Cancelled = 4,
        Rejected = 5,
        Expired = 6,
        NotFound = 7,
    ]
);
desc!(
    RECON_SFLAGS,
    "ReconFlags",
    2,
    [
        USERSTREAM_DOWN = 1,
        TRUNCATED = 2,
        HISTORY_UNCHECKED = 4,
        AUTO_EXTERNAL = 8
    ]
);
desc!(
    /// Per-point 2-bit source of `LatencySample.src`.
    LATENCY_SRC, "LatencySrc", 4, [Missing = 0, HW = 1, SW = 2, TSC = 3]);
desc!(
    LATENCY_ACTION,
    "LatencyAction",
    1,
    [New = 0, Cancel = 1, Amend = 2]
);
desc!(CLOCK_SOURCE, "ClockSource", 1, [Phc = 1, Ntp = 2, None = 3]);
desc!(
    CLOCK_STATE,
    "ClockState",
    1,
    [Synced = 1, Unsynced = 2, Stepped = 3]
);
desc!(ROLL_MARK, "RollMark", 1, [End = 1, Begin = 2, BootDone = 3]);
desc!(
    ROLL_REASON,
    "RollReason",
    1,
    [
        Hourly = 1,
        Size = 2,
        Startup = 3,
        Recovery = 4,
        Manual = 5,
        RecoverFromSnapshot = 6,
    ]
);
desc!(JOURNAL_KIND, "JournalKind", 1, [Raw = 1, Seq = 2, Chain = 3]);
desc!(
    ENGINE_CLOCK_SOURCE,
    "EngineClockSource",
    1,
    [Fresh = 1, Tail = 2, Snapshot = 3]
);
desc!(
    CAP_CFLAGS,
    "CapFlags",
    1,
    [HARD = 1, CONDITIONAL = 2, NON_CLEARABLE = 4]
);
desc!(
    SNAPSHOT_PURPOSE,
    "SnapshotPurpose",
    1,
    [Boot = 1, Keyframe = 2, OnDemand = 3, Recovery = 4, Drain = 5]
);

// Variable-length prefixes.
desc!(L2_ORIGIN, "L2Origin", 1, [Exchange = 1, Keyframe = 2, TopN = 3]);
desc!(L2_BFLAGS, "L2BatchFlags", 2, [SNAPSHOT_BRIDGE = 1]);
desc!(L2_DFLAGS, "L2DeltaFlags", 1, [LAST = 1]);
desc!(BOOT, "Boot", 1, [Off = 0, On = 1]);
desc!(
    RAW_WHAT,
    "RawWhat",
    1,
    [
        WsText = 1,
        WsBinary = 2,
        RestResponse = 3,
        Ping = 4,
        Pong = 5,
        Connect = 6,
        Disconnect = 7,
        SubAck = 8,
        WsClose = 9,
    ]
);
desc!(RAW_RFLAGS, "RawFlags", 1, [REDACTED = 1, SAMPLED = 2]);

// Strategy containers and command envelope.
desc!(LEG_KIND, "LegKind", 1, [Cex = 1, Chain = 2]);
desc!(
    CMD_ROLE,
    "CmdRole",
    1,
    [Process = 0, Viewer = 1, Trader = 2, Admin = 4]
);
desc!(CMD_HFLAGS, "CmdHdrFlags", 1, [PREVIEW = 1]);
desc!(
    ANNOTATE_KIND,
    "AnnotateKind",
    2,
    [OpsNote = 1, LocalWrite = 2, Incident = 3]
);
desc!(
    VENUE_NOTICE_KIND,
    "VenueNoticeKind",
    1,
    [Delisting = 1, FundingInterval = 2, Maintenance = 3]
);
desc!(ADOPT_ITEM, "AdoptItem", 1, [Position = 0, Balance = 1]);
desc!(
    SIM_FAULT,
    "SimFault",
    1,
    [Off = 1, RejectNew = 2, Disconnect = 3, DelayReports = 4]
);

/// Every table above plus the wire enums' `DESC`, for `gen-schema`.
pub static ALL: &[&EnumDesc] = &[
    &SIDE,
    &TIF,
    &RUN_MODE,
    &KILL_LEVEL,
    &CAP_SOURCE,
    &CMD_KIND,
    &LINK_ROLE,
    &ORDER_FLAGS,
    &ORDER_STATUS,
    &REJECT_CLASS,
    &MARGIN_SOURCE,
    &MARGIN_MFLAGS,
    &POSITION_MODE,
    &MARGIN_MODE,
    &MULTI_ASSET,
    &ACR_SOURCE,
    &CFG_MISMATCH,
    &ACR_RFLAGS,
    &UID_KEYS,
    &EXEC_ACTION,
    &EXEC_ALGO,
    &TARGET_REASON,
    &URGENCY,
    &TARGET_STATE,
    &EXEC_EFLAGS,
    &CHAIN_HEAD_SOURCE,
    &CHAIN_HFLAGS,
    &POOL_LEVEL,
    &POOL_QFLAGS,
    &SWAP_EXACT,
    &HEDGE_PX_MODE,
    &SWAP_SFLAGS,
    &CMD_RFLAGS,
    &ACCOUNT_KIND,
    &RECON_ITEM,
    &RECON_SEVERITY,
    &RECON_ORDER_STATUS,
    &RECON_SFLAGS,
    &LATENCY_SRC,
    &LATENCY_ACTION,
    &CLOCK_SOURCE,
    &CLOCK_STATE,
    &ROLL_MARK,
    &ROLL_REASON,
    &JOURNAL_KIND,
    &ENGINE_CLOCK_SOURCE,
    &CAP_CFLAGS,
    &SNAPSHOT_PURPOSE,
    &L2_ORIGIN,
    &L2_BFLAGS,
    &L2_DFLAGS,
    &BOOT,
    &RAW_WHAT,
    &RAW_RFLAGS,
    &LEG_KIND,
    &CMD_ROLE,
    &CMD_HFLAGS,
    &ANNOTATE_KIND,
    &VENUE_NOTICE_KIND,
    &ADOPT_ITEM,
    &SIM_FAULT,
    &crate::exec::OrderAction::DESC,
    &crate::exec::SendStatus::DESC,
    &crate::exec::GwOp::DESC,
    &crate::exec::OrderEventKind::DESC,
    &crate::exec::ExternalCause::DESC,
    &crate::exec::TargetOriginWire::DESC,
    &crate::chain::TxState::DESC,
    &crate::system::NoteLevel::DESC,
    &crate::system::NoteSource::DESC,
    &crate::system::ConnStateKind::DESC,
    &crate::system::ConnRole::DESC,
    &crate::var::BlobKind::DESC,
    &crate::strategy::ParamSource::DESC,
];
