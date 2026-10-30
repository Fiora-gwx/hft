//! Row structures and enums of the decoded reference data (types only, no load logic).

use hft_types::{AccountId, AssetExp, AssetId, InstId, Notional, Px, Qty, Rate, StrategyInstanceId, VenueId};

/// Default preallocation bound for the number of inst rows (`refdata.max_universe_insts`).
pub const DEFAULT_MAX_UNIVERSE_INSTS: u32 = 8192;
/// Inline key length of the symbol table (`refdata.max_symbol_len`).
pub const MAX_SYMBOL_LEN: usize = 27;

/// `#[repr(u8)]` wire enum with `from_u8` (0 and out-of-range values → `None`) and `name`.
macro_rules! wire_enum {
    ($(#[$m:meta])* $name:ident { $($v:ident = $n:literal),* $(,)? }) => {
        $(#[$m])*
        #[repr(u8)]
        #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
        pub enum $name { $($v = $n),* }
        impl $name {
            /// Every variant in code order.
            pub const ALL: &'static [$name] = &[$($name::$v),*];
            /// Decodes a wire byte; illegal discriminants → `None`.
            #[inline]
            pub const fn from_u8(v: u8) -> Option<$name> {
                match v { $($n => Some($name::$v),)* _ => None }
            }
            /// Variant name as written in the spec (`CamelCase`).
            pub const fn name(self) -> &'static str {
                match self { $($name::$v => stringify!($v),)* }
            }
        }
        const _: () = assert!(size_of::<$name>() == 1);
    };
}
pub(crate) use wire_enum;

wire_enum!(
    /// Trading status of an inst; `Invalid` (0) only appears in the NONE row, never in a blob.
    InstStatus { Invalid = 0, Trading = 1, ReduceOnly = 2, Halted = 3, Maintenance = 4, PreTrading = 5, Delisted = 6 }
);
wire_enum!(
    /// Contract kind; `Delivery` is rejected at load (`UnsupportedKind`).
    InstKind { Spot = 1, Perp = 2, Delivery = 3 }
);
wire_enum!(
    /// Maintenance window kind.
    MaintKind { VenueMaintenance = 1, MarketClosed = 2, Delisting = 3 }
);
wire_enum!(
    /// Asset class; inst quotes must be `Stable`.
    AssetClass { Crypto = 1, Stable = 2, Equity = 3, Commodity = 4 }
);
wire_enum!(
    /// Scope of a rate limit.
    LimitScope { Ip = 1, Uid = 2, Account = 3, Conn = 4, Inst = 5 }
);
wire_enum!(
    /// What a rate limit counts.
    LimitBucket { Orders = 1, Weight = 2, WsInMsgs = 3, NewConns = 4, Amends = 5, Cancels = 6 }
);
wire_enum!(
    /// Role of an endpoint.
    EndpointRole { MdPublic = 1, MdMarket = 2, UserData = 3, OrderWs = 4, Rest = 5, RestBackup = 6 }
);
wire_enum!(
    /// Environment of an endpoint.
    EndpointEnv { Prod = 1, Testnet = 2, Demo = 3 }
);
wire_enum!(
    /// Exchange timestamp precision.
    TsUnit { Ms = 1, Us = 2, Ns = 3 }
);
wire_enum!(
    /// Order-request expiry protection of a venue (cross-checked against `VenueProtocol::stale_guard()`).
    StaleGuardKind { None = 1, RecvWindow = 2, ExpireAt = 3 }
);
wire_enum!(
    /// Exchange a `VenueId` belongs to.
    Exchange { Binance = 1, Okx = 2, Bitget = 3, Gate = 4, Bybit = 5, Kucoin = 6, Hyperliquid = 7, Polymarket = 8, Chain = 32 }
);
wire_enum!(
    /// Whether the exchange UID of an account can be verified (TOML `uid_policy`, default `verified`).
    UidPolicy { Verified = 1, Unverifiable = 2 }
);

/// Hot inst row: one cache line, indexed by `InstId` (index 0 = NONE row, `status = Invalid`).
#[repr(C, align(64))]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct InstHot {
    pub venue: VenueId,
    pub status: InstStatus,
    pub kind: InstKind,
    /// `InstHot::NO_AMEND`: the inst does not support amends (overrides the venue capability).
    pub flags: u8,
    pub px_exp: i8,
    pub qty_exp: i8,
    /// `px_exp + px_unit_shift`.
    pub px_wire_exp: i8,
    /// `qty_exp − qty_unit_shift`.
    pub qty_wire_exp: i8,
    pub asset: AssetId,
    pub quote: AssetId,
    /// Book window half-width W (ticks).
    pub book_window: u32,
    /// Price step, integer at the asset's `px_exp`.
    pub tick: i64,
    /// Quantity step, integer at the asset's `qty_exp` (asset units).
    pub lot: i64,
    pub min_qty: Qty,
    pub max_qty: Qty,
    pub min_notional: Notional,
    pub l2_depth: u16,
    pub crossed_tolerance: u16,
    pub _pad: u32,
}
const _: () = assert!(size_of::<InstHot>() == 64);
const _: () = assert!(align_of::<InstHot>() == 64);
const _: () = assert!(core::mem::offset_of!(InstHot, venue) == 0);
const _: () = assert!(core::mem::offset_of!(InstHot, status) == 1);
const _: () = assert!(core::mem::offset_of!(InstHot, kind) == 2);
const _: () = assert!(core::mem::offset_of!(InstHot, flags) == 3);
const _: () = assert!(core::mem::offset_of!(InstHot, px_exp) == 4);
const _: () = assert!(core::mem::offset_of!(InstHot, qty_exp) == 5);
const _: () = assert!(core::mem::offset_of!(InstHot, px_wire_exp) == 6);
const _: () = assert!(core::mem::offset_of!(InstHot, qty_wire_exp) == 7);
const _: () = assert!(core::mem::offset_of!(InstHot, asset) == 8);
const _: () = assert!(core::mem::offset_of!(InstHot, quote) == 10);
const _: () = assert!(core::mem::offset_of!(InstHot, book_window) == 12);
const _: () = assert!(core::mem::offset_of!(InstHot, tick) == 16);
const _: () = assert!(core::mem::offset_of!(InstHot, lot) == 24);
const _: () = assert!(core::mem::offset_of!(InstHot, min_qty) == 32);
const _: () = assert!(core::mem::offset_of!(InstHot, max_qty) == 40);
const _: () = assert!(core::mem::offset_of!(InstHot, min_notional) == 48);
const _: () = assert!(core::mem::offset_of!(InstHot, l2_depth) == 56);
const _: () = assert!(core::mem::offset_of!(InstHot, crossed_tolerance) == 58);
const _: () = assert!(core::mem::offset_of!(InstHot, _pad) == 60);

impl InstHot {
    /// `flags` bit: the inst does not support amends.
    pub const NO_AMEND: u8 = 1;
    /// Row returned for id 0 and out-of-range ids.
    pub const NONE: InstHot = InstHot {
        venue: VenueId(0),
        status: InstStatus::Invalid,
        kind: InstKind::Spot,
        flags: 0,
        px_exp: 0,
        qty_exp: 0,
        px_wire_exp: 0,
        qty_wire_exp: 0,
        asset: AssetId(0),
        quote: AssetId(0),
        book_window: 0,
        tick: 0,
        lot: 0,
        min_qty: Qty(0),
        max_qty: Qty(0),
        min_notional: Notional(0),
        l2_depth: 0,
        crossed_tolerance: 0,
        _pad: 0,
    };
}

/// One maintenance-margin tier (RDB1 section 18).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct MmrTier {
    /// Upper bound (inclusive) of position notional in this tier.
    pub notional_max: Notional,
    pub mmr: Rate,
}

/// Exchange price-limit band; `up = down = Rate(0)` = not registered.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PriceBand {
    pub up: Rate,
    pub down: Rate,
}

impl Default for PriceBand {
    fn default() -> Self {
        PriceBand {
            up: Rate(0),
            down: Rate(0),
        }
    }
}

/// Cold inst fields. Strings are owned copies of the blob's string pool.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct InstMeta {
    pub id: InstId,
    pub symbol: Box<str>,
    pub venue_num_id: u64,
    /// The venue quotes prices per 10^s asset units (`1000PEPEUSDT` → 3).
    pub px_unit_shift: i8,
    /// One venue quantity unit = 10^s asset units (`1000PEPEUSDT` → 3, OKX ctVal 1e7 → 7).
    pub qty_unit_shift: i8,
    pub tick_str: Box<str>,
    pub lot_str: Box<str>,
    pub min_qty_str: Box<str>,
    /// Default funding period; the `Funding` event is authoritative at run time.
    pub funding_interval_s: u32,
    pub funding_anchor_s: u32,
    pub max_leverage: u16,
    pub listed_utc_ns: i64,
    pub delivery_utc_ns: i64,
    /// Ascending by `notional_max`; empty = not registered.
    pub mmr_tiers: Box<[MmrTier]>,
    pub price_band: PriceBand,
    pub(crate) kind: InstKind,
}

impl InstMeta {
    /// `Spot → Spot`, `Perp → LinearPerp` (`Delivery` never loads; the NONE row maps to `Spot`).
    pub fn market(&self) -> hft_types::MarketKind {
        match self.kind {
            InstKind::Perp => hft_types::MarketKind::LinearPerp,
            InstKind::Spot | InstKind::Delivery => hft_types::MarketKind::Spot,
        }
    }

    pub(crate) fn none() -> InstMeta {
        InstMeta {
            id: InstId(0),
            symbol: Box::default(),
            venue_num_id: 0,
            px_unit_shift: 0,
            qty_unit_shift: 0,
            tick_str: Box::default(),
            lot_str: Box::default(),
            min_qty_str: Box::default(),
            funding_interval_s: 0,
            funding_anchor_s: 0,
            max_leverage: 0,
            listed_utc_ns: 0,
            delivery_utc_ns: 0,
            mmr_tiers: Box::default(),
            price_band: PriceBand::default(),
            kind: InstKind::Spot,
        }
    }
}

/// Asset row; also the builder input (`AssetSpec`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct AssetMeta {
    pub id: AssetId,
    pub name: Box<str>,
    pub class: AssetClass,
    pub exp: AssetExp,
    pub px_max: Px,
    pub qty_max: Qty,
    /// Asset-level dust (tolerance for balance, inventory and funding comparisons); `0 < dust ≤ qty_max`.
    pub dust: Qty,
}

impl AssetMeta {
    pub(crate) fn none() -> AssetMeta {
        AssetMeta {
            id: AssetId(0),
            name: Box::default(),
            class: AssetClass::Crypto,
            exp: AssetExp {
                px_exp: 0,
                qty_exp: 0,
            },
            px_max: Px(0),
            qty_max: Qty(0),
            dust: Qty(0),
        }
    }
}

/// Venue row (connection and protocol constants); also the builder input (`VenueSpec`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct VenueMeta {
    pub id: VenueId,
    pub exchange: Exchange,
    pub product: InstKind,
    pub ws_max_streams_per_conn: u16,
    pub ws_max_in_msgs_per_s: u16,
    pub ws_max_conn_age_s: u32,
    pub ws_new_conn_per_5min_ip: u16,
    pub app_ping_interval_s: u16,
    pub ts_unit: TsUnit,
    pub client_id_max_len: u8,
    pub client_id_prefix: Box<str>,
    pub max_batch: u8,
    pub max_inflight_amend: u8,
    pub crossed_tolerance_default: u16,
    pub stale_guard: StaleGuardKind,
}

impl VenueMeta {
    pub(crate) fn none() -> VenueMeta {
        VenueMeta {
            id: VenueId(0),
            exchange: Exchange::Binance,
            product: InstKind::Spot,
            ws_max_streams_per_conn: 0,
            ws_max_in_msgs_per_s: 0,
            ws_max_conn_age_s: 0,
            ws_new_conn_per_5min_ip: 0,
            app_ping_interval_s: 0,
            ts_unit: TsUnit::Ms,
            client_id_max_len: 0,
            client_id_prefix: Box::default(),
            max_batch: 0,
            max_inflight_amend: 0,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::None,
        }
    }
}

/// Strategy instance (RDB1 section 15); also the builder input (`InstanceSpec`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct InstanceMeta {
    /// Dense, 1..=254; permanently bound to `(kind_id, name)`.
    pub id: StrategyInstanceId,
    pub kind_id: u16,
    pub name: Box<str>,
}

/// One initial balance of a Sim account (SimBalances, section 17).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SimBalance {
    pub venue: VenueId,
    pub asset: AssetId,
    pub qty: Qty,
}

/// Sim account configuration (semantics owned by hft-sim).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct SimAccountCfg {
    pub initial_balances: Box<[SimBalance]>,
    pub latency_model: u16,
    pub fee_tier: u8,
    pub margin_model: u16,
}

/// Account type; in-row code Live = 1, Sim = 2.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum AccountKind {
    Live,
    Sim(SimAccountCfg),
}

impl AccountKind {
    /// In-row code (same as `CmdPreview.target_kind`).
    pub const fn code(&self) -> u8 {
        match self {
            AccountKind::Live => 1,
            AccountKind::Sim(_) => 2,
        }
    }
}

/// Account row; the hot fields (`kind`, `bound_instance`, `venues`) are in the first cache line.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct AccountMeta {
    pub id: AccountId,
    pub kind: AccountKind,
    /// The only strategy instance allowed to trade this account; 0 = unbound.
    pub bound_instance: StrategyInstanceId,
    pub is_prod: bool,
    /// `VenueId` bitmap (bit v = venue v).
    pub venues: u64,
    pub label: Box<str>,
    pub uid_policy: UidPolicy,
}
const _: () = assert!(core::mem::offset_of!(AccountMeta, kind) + size_of::<AccountKind>() <= 64);
const _: () = assert!(core::mem::offset_of!(AccountMeta, bound_instance) < 64);
const _: () = assert!(core::mem::offset_of!(AccountMeta, venues) < 64);

impl AccountMeta {
    pub(crate) fn none() -> AccountMeta {
        AccountMeta {
            id: AccountId(0),
            kind: AccountKind::Live,
            bound_instance: hft_types::StrategyId(0),
            is_prod: false,
            venues: 0,
            label: Box::default(),
            uid_policy: UidPolicy::Verified,
        }
    }
}

/// Maker/taker rates; negative maker = rebate.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FeePair {
    pub maker: Rate,
    pub taker: Rate,
}

/// One static rate limit of a venue.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RateLimit {
    pub scope: LimitScope,
    pub bucket: LimitBucket,
    pub limit: u32,
    pub window_ms: u32,
    pub default_cost: u16,
}

/// One endpoint of a venue.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Endpoint {
    pub role: EndpointRole,
    pub env: EndpointEnv,
    pub url: Box<str>,
    pub conn_group: u8,
}

/// Maintenance window `[start_ns, end_ns)`; `inst = 0` = whole venue.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct MaintWindow {
    pub venue: VenueId,
    pub inst: InstId,
    pub start_ns: i64,
    pub end_ns: i64,
    pub kind: MaintKind,
}

/// On-chain token mapping (optional chain venues).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ChainToken {
    pub chain: VenueId,
    pub asset: AssetId,
    pub address: [u8; 32],
    pub decimals: u8,
}

/// Load and check error codes. Values are registered here only; append-only, never reused.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum LoadErrCode {
    Structure = 1,
    Strings = 2,
    Ids = 3,
    UnknownVenue = 4,
    InstanceIdRange = 5,
    DanglingRef = 6,
    QuoteNotStable = 7,
    InstanceMultiBound = 8,
    MissingCaps = 9,
    CapsMarketMismatch = 10,
    SpotBorrowRequired = 11,
    DeadManRange = 12,
    CapsValue = 13,
    ExpRange = 14,
    NotionalExp = 15,
    UnsupportedKind = 16,
    TickMismatch = 17,
    AssetExpTooCoarse = 18,
    ValueRange = 19,
    DuplicateSymbol = 20,
    SameExchangeMarkets = 21,
    DuplicateVenueUid = 22,
    MissingFee = 23,
    MissingVenueUid = 24,
    TooManyAccountsPerVenue = 25,
    SimIsProd = 26,
    SimHasUid = 27,
    SimCfg = 28,
    UidPolicyProd = 29,
    AlreadyActive = 30,
    VersionNotIncreasing = 31,
    InstRemoved = 32,
    AssetChanged = 33,
    MultiplierChanged = 34,
    AccountKindChanged = 35,
    UidChanged = 36,
    InstanceChanged = 37,
    AccountBound = 38,
    KindMismatch = 39,
    LegNotTradable = 40,
    LegVenueNotInAccount = 41,
    LegAssetMismatch = 42,
    QuoteMismatch = 43,
    SameExchangeLegs = 44,
    NoValidator = 45,
    HostClassMismatch = 46,
    RefInstNotSubscribed = 47,
    UnknownStrategyKind = 48,
    CapsExceedProtocol = 49,
    StaleGuardMismatch = 50,
    BindingChangedWithPosition = 51,
}
const _: () = assert!(size_of::<LoadErrCode>() == 2);

impl LoadErrCode {
    /// Every code in value order (exported by `schema`).
    pub const ALL: [LoadErrCode; 51] = [
        LoadErrCode::Structure,
        LoadErrCode::Strings,
        LoadErrCode::Ids,
        LoadErrCode::UnknownVenue,
        LoadErrCode::InstanceIdRange,
        LoadErrCode::DanglingRef,
        LoadErrCode::QuoteNotStable,
        LoadErrCode::InstanceMultiBound,
        LoadErrCode::MissingCaps,
        LoadErrCode::CapsMarketMismatch,
        LoadErrCode::SpotBorrowRequired,
        LoadErrCode::DeadManRange,
        LoadErrCode::CapsValue,
        LoadErrCode::ExpRange,
        LoadErrCode::NotionalExp,
        LoadErrCode::UnsupportedKind,
        LoadErrCode::TickMismatch,
        LoadErrCode::AssetExpTooCoarse,
        LoadErrCode::ValueRange,
        LoadErrCode::DuplicateSymbol,
        LoadErrCode::SameExchangeMarkets,
        LoadErrCode::DuplicateVenueUid,
        LoadErrCode::MissingFee,
        LoadErrCode::MissingVenueUid,
        LoadErrCode::TooManyAccountsPerVenue,
        LoadErrCode::SimIsProd,
        LoadErrCode::SimHasUid,
        LoadErrCode::SimCfg,
        LoadErrCode::UidPolicyProd,
        LoadErrCode::AlreadyActive,
        LoadErrCode::VersionNotIncreasing,
        LoadErrCode::InstRemoved,
        LoadErrCode::AssetChanged,
        LoadErrCode::MultiplierChanged,
        LoadErrCode::AccountKindChanged,
        LoadErrCode::UidChanged,
        LoadErrCode::InstanceChanged,
        LoadErrCode::AccountBound,
        LoadErrCode::KindMismatch,
        LoadErrCode::LegNotTradable,
        LoadErrCode::LegVenueNotInAccount,
        LoadErrCode::LegAssetMismatch,
        LoadErrCode::QuoteMismatch,
        LoadErrCode::SameExchangeLegs,
        LoadErrCode::NoValidator,
        LoadErrCode::HostClassMismatch,
        LoadErrCode::RefInstNotSubscribed,
        LoadErrCode::UnknownStrategyKind,
        LoadErrCode::CapsExceedProtocol,
        LoadErrCode::StaleGuardMismatch,
        LoadErrCode::BindingChangedWithPosition,
    ];

    /// Variant name as written in the code table.
    pub fn name(self) -> &'static str {
        match self {
            LoadErrCode::Structure => "Structure",
            LoadErrCode::Strings => "Strings",
            LoadErrCode::Ids => "Ids",
            LoadErrCode::UnknownVenue => "UnknownVenue",
            LoadErrCode::InstanceIdRange => "InstanceIdRange",
            LoadErrCode::DanglingRef => "DanglingRef",
            LoadErrCode::QuoteNotStable => "QuoteNotStable",
            LoadErrCode::InstanceMultiBound => "InstanceMultiBound",
            LoadErrCode::MissingCaps => "MissingCaps",
            LoadErrCode::CapsMarketMismatch => "CapsMarketMismatch",
            LoadErrCode::SpotBorrowRequired => "SpotBorrowRequired",
            LoadErrCode::DeadManRange => "DeadManRange",
            LoadErrCode::CapsValue => "CapsValue",
            LoadErrCode::ExpRange => "ExpRange",
            LoadErrCode::NotionalExp => "NotionalExp",
            LoadErrCode::UnsupportedKind => "UnsupportedKind",
            LoadErrCode::TickMismatch => "TickMismatch",
            LoadErrCode::AssetExpTooCoarse => "AssetExpTooCoarse",
            LoadErrCode::ValueRange => "ValueRange",
            LoadErrCode::DuplicateSymbol => "DuplicateSymbol",
            LoadErrCode::SameExchangeMarkets => "SameExchangeMarkets",
            LoadErrCode::DuplicateVenueUid => "DuplicateVenueUid",
            LoadErrCode::MissingFee => "MissingFee",
            LoadErrCode::MissingVenueUid => "MissingVenueUid",
            LoadErrCode::TooManyAccountsPerVenue => "TooManyAccountsPerVenue",
            LoadErrCode::SimIsProd => "SimIsProd",
            LoadErrCode::SimHasUid => "SimHasUid",
            LoadErrCode::SimCfg => "SimCfg",
            LoadErrCode::UidPolicyProd => "UidPolicyProd",
            LoadErrCode::AlreadyActive => "AlreadyActive",
            LoadErrCode::VersionNotIncreasing => "VersionNotIncreasing",
            LoadErrCode::InstRemoved => "InstRemoved",
            LoadErrCode::AssetChanged => "AssetChanged",
            LoadErrCode::MultiplierChanged => "MultiplierChanged",
            LoadErrCode::AccountKindChanged => "AccountKindChanged",
            LoadErrCode::UidChanged => "UidChanged",
            LoadErrCode::InstanceChanged => "InstanceChanged",
            LoadErrCode::AccountBound => "AccountBound",
            LoadErrCode::KindMismatch => "KindMismatch",
            LoadErrCode::LegNotTradable => "LegNotTradable",
            LoadErrCode::LegVenueNotInAccount => "LegVenueNotInAccount",
            LoadErrCode::LegAssetMismatch => "LegAssetMismatch",
            LoadErrCode::QuoteMismatch => "QuoteMismatch",
            LoadErrCode::SameExchangeLegs => "SameExchangeLegs",
            LoadErrCode::NoValidator => "NoValidator",
            LoadErrCode::HostClassMismatch => "HostClassMismatch",
            LoadErrCode::RefInstNotSubscribed => "RefInstNotSubscribed",
            LoadErrCode::UnknownStrategyKind => "UnknownStrategyKind",
            LoadErrCode::CapsExceedProtocol => "CapsExceedProtocol",
            LoadErrCode::StaleGuardMismatch => "StaleGuardMismatch",
            LoadErrCode::BindingChangedWithPosition => "BindingChangedWithPosition",
        }
    }
}

impl TryFrom<u16> for LoadErrCode {
    type Error = u16;
    /// 0 and unassigned values → `Err(value)`.
    fn try_from(v: u16) -> Result<Self, u16> {
        let ix = usize::from(v).checked_sub(1).ok_or(v)?;
        LoadErrCode::ALL.get(ix).copied().ok_or(v)
    }
}

/// One error: code, section id (0 = none) and row index within the section (0 = none).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LoadErr {
    pub code: LoadErrCode,
    pub section: u16,
    pub row: u32,
}

impl LoadErr {
    #[inline]
    pub const fn new(code: LoadErrCode, section: u16, row: u32) -> LoadErr {
        LoadErr { code, section, row }
    }
}

/// All errors of the failing step, in check order.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct LoadReport {
    pub errors: Vec<LoadErr>,
}

impl LoadReport {
    /// Code of the first error (the only one carried on the wire); an empty report → `Structure`.
    pub fn code(&self) -> LoadErrCode {
        self.errors.first().map_or(LoadErrCode::Structure, |e| e.code)
    }
}

/// Same-shape view of hft-event `GroupSpec` (first `n_legs` legs).
#[derive(Copy, Clone, Debug)]
pub struct GroupSpecView<'a> {
    pub asset: AssetId,
    /// `GroupSpec.open_allowed != 0`.
    pub open_allowed: bool,
    pub legs: &'a [LegView],
}

/// One leg of a `GroupSpecView`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct LegView {
    pub inst: InstId,
    pub venue: VenueId,
}
