//! `VenueCaps` (one row per `VenueId`), its enums, and the start-up cross-check against the
//! compiled-in `VenueProtocol` and `StrategySet` facts (`check_assembly`).

use hft_types::{AssetId, HostClass, MarketKind, VenueId};

use crate::engine::EngineConfig;
use crate::types::{LoadErr, LoadErrCode, LoadReport, wire_enum};
use crate::{RefData, blob::sec};

wire_enum!(
    /// How realized funding is pushed.
    FundingSettleKind { PerSymbolPush = 1, BalanceOnlyPush = 2, RateOnly = 3, NotApplicable = 4 }
);
wire_enum!(
    /// Required / observed position mode (same codes as hft-event `AccountConfigReport`).
    PositionMode { NotApplicable = 1, OneWay = 2, Hedge = 3 }
);
wire_enum!(
    /// Required / observed margin mode; `Mixed` is an observation only (rejected as a requirement).
    MarginMode { NotApplicable = 1, Cross = 2, Isolated = 3, Mixed = 4 }
);
wire_enum!(
    /// Observed multi-asset mode code (`AccountConfigReport.multi_asset`), not a requirement.
    MultiAsset { NotApplicable = 1, Off = 2, On = 3 }
);
wire_enum!(
    /// How own liquidation / ADL orders are identified.
    LiqMarker { NotIdentifiable = 1, ClientIdPrefix = 2, OrderTypeField = 3, ExecTypeField = 4 }
);
wire_enum!(
    /// Exchange-side dead man's switch (one-to-one with hft-venue-proto `DeadManSpec`).
    DeadManCap { None = 1, CountdownPerInst = 2, CountdownAccount = 3, OnDisconnect = 4, Schedule = 5 }
);
wire_enum!(
    /// Test environment availability.
    TestnetKind { None = 1, Testnet = 2, Demo = 3 }
);

/// Which asset the fee is charged in (decides the fill's net quantity).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum FeeAssetRule {
    Quote,
    ReceivedAsset,
    ReceivedAssetUnlessDiscount { token: AssetId },
    Configured(AssetId),
}

impl FeeAssetRule {
    /// Wire discriminant (Quote 1, ReceivedAsset 2, ReceivedAssetUnlessDiscount 3, Configured 4).
    pub const fn code(self) -> u8 {
        match self {
            FeeAssetRule::Quote => 1,
            FeeAssetRule::ReceivedAsset => 2,
            FeeAssetRule::ReceivedAssetUnlessDiscount { .. } => 3,
            FeeAssetRule::Configured(_) => 4,
        }
    }
    /// Referenced asset (0 for the variants without one).
    pub const fn asset(self) -> AssetId {
        match self {
            FeeAssetRule::Quote | FeeAssetRule::ReceivedAsset => AssetId(0),
            FeeAssetRule::ReceivedAssetUnlessDiscount { token } => token,
            FeeAssetRule::Configured(a) => a,
        }
    }
    pub(crate) const fn decode(code: u8, asset: u16) -> Option<FeeAssetRule> {
        match code {
            1 => Some(FeeAssetRule::Quote),
            2 => Some(FeeAssetRule::ReceivedAsset),
            3 => Some(FeeAssetRule::ReceivedAssetUnlessDiscount {
                token: AssetId(asset),
            }),
            4 => Some(FeeAssetRule::Configured(AssetId(asset))),
            _ => None,
        }
    }
}

/// Source of margin ratio / liquidation price / ADL rank. Wire: Push 1, Poll 2 (+ interval_ms), NotApplicable 3.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum MarginData {
    Push,
    Poll { interval_ms: u32 },
    NotApplicable,
}

impl MarginData {
    pub const fn code(self) -> u8 {
        match self {
            MarginData::Push => 1,
            MarginData::Poll { .. } => 2,
            MarginData::NotApplicable => 3,
        }
    }
    pub(crate) const fn decode(code: u8, interval_ms: u32) -> Option<MarginData> {
        match code {
            1 => Some(MarginData::Push),
            2 => Some(MarginData::Poll { interval_ms }),
            3 => Some(MarginData::NotApplicable),
            _ => None,
        }
    }
}

/// Capability bits of a venue.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct CapFlags(pub u32);

impl CapFlags {
    pub const AMEND: CapFlags = CapFlags(1);
    pub const POST_ONLY: CapFlags = CapFlags(2);
    pub const IOC: CapFlags = CapFlags(4);
    pub const FOK: CapFlags = CapFlags(8);
    pub const BATCH_CANCEL: CapFlags = CapFlags(16);
    /// Perpetual execution/query reports supply complete realized-cash settlement facts.
    /// Consumers require coverage and never reconstruct wallet cash from local average cost.
    pub const PERP_CASH: CapFlags = CapFlags(32);
    /// Perpetual tail close using exact reduce-only quantity or a full-position endpoint.
    pub const POSITION_CLOSE: CapFlags = CapFlags(64);
    /// Position-close ignores the requested quantity and closes the symbol's entire position.
    pub const POSITION_CLOSE_ALL: CapFlags = CapFlags(128);
    /// Union of every defined bit.
    pub const ALL: CapFlags = CapFlags(255);

    #[inline]
    pub const fn contains(self, other: CapFlags) -> bool {
        self.0 & other.0 == other.0
    }
}

impl core::ops::BitOr for CapFlags {
    type Output = CapFlags;
    #[inline]
    fn bitor(self, rhs: CapFlags) -> CapFlags {
        CapFlags(self.0 | rhs.0)
    }
}

/// Wire encoding of `Option<bool>`: None 1, Some(false) 2, Some(true) 3.
pub const fn opt_bool_code(v: Option<bool>) -> u8 {
    match v {
        None => 1,
        Some(false) => 2,
        Some(true) => 3,
    }
}

pub(crate) const fn opt_bool_decode(c: u8) -> Option<Option<bool>> {
    match c {
        1 => Some(None),
        2 => Some(Some(false)),
        3 => Some(Some(true)),
        _ => None,
    }
}

/// Exchange capability row of one `VenueId` (the effective row written by hft-cfg).
/// `#[repr(C)]` so the capability bits and dead-man fields sit in the first cache line; the
/// borrowed-string fields (cold) come last.
#[repr(C)]
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct VenueCaps {
    pub venue: VenueId,
    /// Redundant: derived from `VenueMeta.product`.
    pub market: MarketKind,
    pub reduce_only: bool,
    pub required_position_mode: PositionMode,
    pub required_margin_mode: MarginMode,
    /// `None` = not checked.
    pub required_multi_asset: Option<bool>,
    /// Spot borrow requirement; `Some(true)` is rejected at load.
    pub required_spot_borrow: Option<bool>,
    pub funding_settle: FundingSettleKind,
    pub own_liquidation_marker: LiqMarker,
    pub dms: DeadManCap,
    pub testnet: TestnetKind,
    pub amend_qty_down_only: bool,
    pub fee_asset_rule: FeeAssetRule,
    pub flags: CapFlags,
    pub margin_data: MarginData,
    pub dms_min_ms: u32,
    pub dms_max_ms: u32,
    pub dms_timeout_ms: u32,
    pub dms_refresh_ms: u32,
    /// Always 0 (explicit padding).
    pub _pad: u32,
    pub liq_marker_arg: Box<str>,
    pub ignorable_user_events: Box<[Box<str>]>,
}
const _: () = assert!(size_of::<FeeAssetRule>() == 4 && align_of::<FeeAssetRule>() == 2);
const _: () = assert!(size_of::<MarginData>() == 8 && align_of::<MarginData>() == 4);
const _: () = assert!(size_of::<VenueCaps>() == 80);
const _: () = assert!(core::mem::offset_of!(VenueCaps, fee_asset_rule) == 12);
const _: () = assert!(core::mem::offset_of!(VenueCaps, flags) == 16);
const _: () = assert!(core::mem::offset_of!(VenueCaps, margin_data) == 20);
const _: () = assert!(core::mem::offset_of!(VenueCaps, dms) == 9);
const _: () = assert!(core::mem::offset_of!(VenueCaps, dms_refresh_ms) == 40);
const _: () = assert!(core::mem::offset_of!(VenueCaps, _pad) == 44);
const _: () = assert!(core::mem::offset_of!(VenueCaps, liq_marker_arg) == 48);

impl VenueCaps {
    /// NONE row: every capability bit 0, no dead man's switch.
    pub fn none(venue: VenueId) -> VenueCaps {
        VenueCaps {
            venue,
            market: MarketKind::Spot,
            fee_asset_rule: FeeAssetRule::Quote,
            reduce_only: false,
            required_position_mode: PositionMode::NotApplicable,
            required_margin_mode: MarginMode::NotApplicable,
            required_multi_asset: None,
            required_spot_borrow: None,
            funding_settle: FundingSettleKind::NotApplicable,
            margin_data: MarginData::NotApplicable,
            own_liquidation_marker: LiqMarker::NotIdentifiable,
            liq_marker_arg: Box::default(),
            ignorable_user_events: Box::default(),
            flags: CapFlags(0),
            amend_qty_down_only: false,
            dms: DeadManCap::None,
            dms_min_ms: 0,
            dms_max_ms: 0,
            dms_timeout_ms: 0,
            dms_refresh_ms: 0,
            testnet: TestnetKind::None,
            _pad: 0,
        }
    }

    /// Protocol comparison (`CapsExceedProtocol`): bits ⊆ protocol, `amend_qty_down_only`
    /// true when the protocol's is, same dead-man variant.
    pub fn within_protocol(&self, proto: &VenueCaps) -> bool {
        proto.flags.contains(self.flags)
            && (!proto.amend_qty_down_only || self.amend_qty_down_only)
            && self.dms == proto.dms
    }
}

/// Facts of one compiled-in `VenueProtocol`.
#[derive(Clone, Debug)]
pub struct ProtocolFacts {
    pub venue: VenueId,
    pub market: MarketKind,
    /// `VenueProtocol::default_caps()`.
    pub caps: VenueCaps,
    pub stale_guard: crate::types::StaleGuardKind,
}

/// Everything hft-core knows about the binary at assembly time.
#[derive(Copy, Clone, Debug)]
pub struct AssemblyFacts<'a> {
    pub host: HostClass,
    pub max_universe_insts: u32,
    pub protocols: &'a [ProtocolFacts],
    /// `StrategySet::kinds()`.
    pub kinds: &'a [u16],
}

/// Start-up / `ReplaceRefdata` assembly check (README "跨 blob 与跨组件校验" item 1);
/// collects every error in rule order. Pure.
pub fn check_assembly(rd: &RefData, ec: &EngineConfig, f: &AssemblyFacts<'_>) -> Result<(), LoadReport> {
    let mut e = Vec::new();
    if ec.host_class() != f.host {
        e.push(LoadErr::new(LoadErrCode::HostClassMismatch, 0, 0));
    }
    let n_insts = rd.inst_count();
    if n_insts > f.max_universe_insts as usize {
        e.push(LoadErr::new(LoadErrCode::ValueRange, sec::INSTS, 0));
    }
    let proto = |v: VenueId| f.protocols.iter().find(|p| p.venue == v);
    let traded: Vec<(u32, VenueId)> = rd
        .venue_order
        .iter()
        .enumerate()
        .filter(|(_, v)| !rd.insts_of_venue(**v).is_empty())
        .map(|(row, v)| (row as u32, *v))
        .collect();
    for &(row, v) in &traded {
        if proto(v).is_none() {
            e.push(LoadErr::new(LoadErrCode::UnknownVenue, sec::VENUES, row));
        }
    }
    for &(_, v) in &traded {
        if let (Some(p), Some(cr)) = (proto(v), rd.caps_row(v))
            && rd.caps(v).market != p.market
        {
            e.push(LoadErr::new(LoadErrCode::CapsMarketMismatch, sec::VENUE_CAPS, cr));
        }
    }
    for &(_, v) in &traded {
        if let (Some(p), Some(cr)) = (proto(v), rd.caps_row(v))
            && !rd.caps(v).within_protocol(&p.caps)
        {
            e.push(LoadErr::new(LoadErrCode::CapsExceedProtocol, sec::VENUE_CAPS, cr));
        }
    }
    for &(row, v) in &traded {
        if let Some(p) = proto(v)
            && rd.venue(v).stale_guard != p.stale_guard
        {
            e.push(LoadErr::new(LoadErrCode::StaleGuardMismatch, sec::VENUES, row));
        }
    }
    for (row, inst) in rd.instances_in_order().iter().enumerate() {
        if !f.kinds.contains(&inst.kind_id) {
            e.push(LoadErr::new(
                LoadErrCode::UnknownStrategyKind,
                sec::INSTANCES,
                row as u32,
            ));
        }
    }
    if e.is_empty() {
        Ok(())
    } else {
        Err(LoadReport { errors: e })
    }
}
