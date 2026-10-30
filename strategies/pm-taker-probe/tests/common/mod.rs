//! Test fixture of `pm-taker-probe`: a binary venue `VB` with one outcome-token slot inst `U`
//! (px_exp −3, qty_exp −6, tick 0.001, lot 0.01, quote `USDC`) and a second inst `X` on the same
//! venue; account `A1` (Sim) bound to instance `S1` of this kind; group `G1` = (U).
#![allow(dead_code)]

use hft_event::{GroupSpec, LegDef};
use hft_refdata::{
    AccountSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule, FeePair, FeeSpec,
    FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LiqMarker, MarginData, MarginMode,
    PositionMode, RdbBuilder, RefData, SimBalanceSpec, SimSpec, StaleGuardKind, TestnetKind, TsUnit,
    VenueCaps, VenueSpec,
};
use hft_strategy_sdk::testing::{MockViews, RefFixture};
use hft_strategy_sdk::{GroupPosView, GroupTable, LegPos};
use hft_types::{
    AccountId, AssetExp, AssetId, GroupId, InstId, MarketKind, Notional, Px, Qty, Rate, StrategyId, Tsc,
    VenueId,
};

pub const VB: VenueId = VenueId::POLYMARKET_CLOB;
pub const U: InstId = InstId(1);
pub const X: InstId = InstId(2);
pub const USDC: AssetId = AssetId(1);
pub const TOK: AssetId = AssetId(2);
pub const TOK2: AssetId = AssetId(3);
pub const A1: AccountId = AccountId(1);
pub const S1: StrategyId = StrategyId(1);
pub const G1: GroupId = GroupId(0x0100_0001);
/// `max_groups_per_instance` of the fixture.
pub const CAPACITY: u16 = 4;
/// 1 GHz TSC (ticks per ms).
pub const TSC_PER_MS: u64 = 1_000_000;

fn inst(b: &mut RdbBuilder, id: InstId, asset: AssetId, sym: &str) {
    b.inst(InstSpec {
        id,
        venue: VB,
        symbol: sym.into(),
        status: InstStatus::Trading,
        kind: InstKind::Spot,
        asset,
        quote: USDC,
        tick: 1,
        lot: 10_000,
        min_qty: Qty(10_000),
        max_qty: Qty(1_000_000_000_000),
        min_notional: Notional(1),
        book_window: 128,
        l2_depth: 100,
        crossed_tolerance: 0,
        tick_str: "0.001".into(),
        lot_str: "0.01".into(),
        min_qty_str: "0.01".into(),
        ..InstSpec::default()
    });
}

/// The fixture refdata.
pub fn refdata() -> RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    for (id, name, class, px, qty) in [
        (USDC, "USDC", AssetClass::Stable, -6, -6),
        (TOK, "TOKU", AssetClass::Crypto, -3, -6),
        (TOK2, "TOKX", AssetClass::Crypto, -3, -6),
    ] {
        b.asset(AssetSpec {
            id,
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: px,
                qty_exp: qty,
            },
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
    }
    b.venue(VenueSpec {
        id: VB,
        exchange: Exchange::Polymarket,
        product: InstKind::Spot,
        ws_max_streams_per_conn: 1024,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 36,
        client_id_prefix: "".into(),
        max_batch: 1,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    });
    b.caps(VenueCaps {
        venue: VB,
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
        liq_marker_arg: "".into(),
        ignorable_user_events: Vec::new().into(),
        flags: CapFlags::POST_ONLY | CapFlags::IOC | CapFlags::FOK,
        amend_qty_down_only: false,
        dms: DeadManCap::None,
        dms_min_ms: 0,
        dms_max_ms: 0,
        dms_timeout_ms: 0,
        dms_refresh_ms: 0,
        testnet: TestnetKind::Testnet,
        _pad: 0,
    });
    b.fee(FeeSpec {
        venue: VB,
        tier: 0,
        fee: FeePair {
            maker: Rate(0),
            taker: Rate(0),
        },
    });
    inst(&mut b, U, TOK, "btc-up-0");
    inst(&mut b, X, TOK2, "btc-dn-0");
    b.instance(InstanceSpec {
        id: S1,
        kind_id: 8,
        name: "pm-taker-probe-1".into(),
    });
    b.account(AccountSpec {
        id: A1,
        sim: true,
        bound_instance: S1,
        is_prod: false,
        venues: 1 << VB.0,
        label: "sim-pm-taker-probe".into(),
        ..AccountSpec::default()
    });
    b.sim(SimSpec {
        account: A1,
        fee_tier: 0,
        latency_model: 0,
        margin_model: 0,
    });
    b.sim_balance(SimBalanceSpec {
        account: A1,
        venue: VB,
        asset: USDC,
        qty: Qty(10_000_000_000),
    });
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => r,
        Err(e) => panic!("fixture refdata rejected: {e:?}"),
    }
}

/// The fixture as a `RefFixture` of account `A1`.
pub fn fixture() -> RefFixture {
    RefFixture::new(refdata(), A1)
}

/// A group with the legs `legs` in order.
pub fn spec(id: GroupId, legs: &[InstId]) -> GroupSpec {
    let none = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let mut l = [none; 4];
    for (d, i) in l.iter_mut().zip(legs.iter()) {
        *d = LegDef {
            inst: *i,
            venue: VB,
            kind: LegDef::CEX,
            _pad: 0,
            route: 0,
            _pad2: 0,
        };
    }
    GroupSpec {
        id,
        asset: TOK,
        n_legs: legs.len() as u8,
        open_allowed: 1,
        legs: l,
        _rsv: 0,
    }
}

/// `G1` = (U).
pub fn g1() -> GroupTable {
    GroupTable::new(&[spec(G1, &[U])], CAPACITY)
}

/// Quote of `i` (raw prices, 100 shares each side), connection evidence at the views' TSC.
pub fn quote(v: &mut MockViews, i: InstId, bid: i64, ask: i64) {
    let t = now_tsc(v);
    v.bbo(i, (Px(bid), Qty(100_000_000)), (Px(ask), Qty(100_000_000)));
    v.conn(i, Tsc(t), true, false);
}

/// Position of `G1`'s leg (raw quantity).
pub fn position(v: &mut MockViews, qty: i64) {
    let mut p = GroupPosView::EMPTY;
    p.n_legs = 1;
    if let Some(l) = p.legs.first_mut() {
        *l = LegPos {
            qty: Qty(qty),
            entry_px: Px(110),
            upnl: Notional(0),
        };
    }
    v.group_pos(A1, G1, p);
}

// ------------------------------------------------------------------------------- driver

use hft_strategy_sdk::ctx::{CtxBufs, NoteReq, Op};
use hft_strategy_sdk::params::{RawParams, SchemaView};
use hft_strategy_sdk::{
    EventRef, IntentOutcome, OrderIntent, OrderRef, OrderUpdate, OrderUpdateKind, Strategy, StrategyInstance,
    Views,
};
use pm_taker_probe::{PmTakerProbe, PmTakerProbeParams};

/// `r<n>`: the OMS slot handle the driver gives the n-th placed order.
pub fn r_of(n: u32) -> OrderRef {
    OrderRef::new(VB, A1, n, 1)
}

/// Parameters: the schema defaults with `(field index, value)` overrides (0 `enabled`,
/// 1 `spend_usdc`, 2 `sell_back`).
pub fn params(set: &[(usize, i64)]) -> PmTakerProbeParams {
    let schema = PmTakerProbe::DESC.schema;
    let mut defaults: Vec<i64> = (0..schema.n_fields())
        .filter_map(|i| schema.field(i).map(|f| f.default))
        .collect();
    for (i, v) in set {
        if let Some(d) = defaults.get_mut(*i) {
            *d = *v;
        }
    }
    PmTakerProbeParams::from_raw(
        &RawParams {
            defaults,
            overrides: Vec::new(),
        },
        1,
    )
}

/// What one call produced.
#[derive(Debug, Default)]
pub struct Out {
    pub places: Vec<OrderIntent>,
    pub notes: Vec<NoteReq>,
    /// Handles given to `places` (in order) as `Placed` outcomes.
    pub refs: Vec<OrderRef>,
}

impl Out {
    /// Note codes in call order.
    pub fn codes(&self) -> Vec<u32> {
        self.notes.iter().map(|n| n.code).collect()
    }
}

/// One instance on `MockViews` with its own `CtxBufs`: every place gets `Placed(r<n>)` on the next
/// call unless `next` says otherwise. Each call is checked for zero allocations.
pub struct D {
    pub s: PmTakerProbe,
    pub v: MockViews,
    pub bufs: Box<CtxBufs>,
    pub groups: GroupTable,
    pub p: PmTakerProbeParams,
    pub next: Option<IntentOutcome>,
    pub next_r: u32,
}

impl D {
    /// Built with `set` (the instance never starts armed), at engine time 1 s; `U` quoted 0.100 /
    /// 0.110.
    pub fn new(set: &[(usize, i64)]) -> D {
        let p = params(set);
        let groups = g1();
        let mut v = MockViews::new(fixture());
        v.at_engine_ns(1_000_000_000);
        let inst = StrategyInstance {
            strategy: S1,
            account: A1,
            kind_id: 8,
        };
        let s = match PmTakerProbe::new(inst, &p, &groups, &v.refview()) {
            Ok(s) => s,
            Err(e) => panic!("new: {e:?}"),
        };
        let mut d = D {
            s,
            v,
            bufs: Box::new(CtxBufs::new()),
            groups,
            p,
            next: None,
            next_r: 1,
        };
        d.quote(100, 110);
        d
    }

    /// Quote of `U` (raw prices; 0 = side empty), connection live now.
    pub fn quote(&mut self, bid: i64, ask: i64) {
        quote(&mut self.v, U, bid, ask);
    }

    /// Applies `set` on top of the current parameters.
    pub fn set(&mut self, set: &[(usize, i64)]) {
        let mut all: Vec<(usize, i64)> = vec![
            (0, i64::from(self.s.fast.enabled)),
            (1, self.s.fast.spend.0),
            (2, i64::from(self.s.fast.sell_back)),
        ];
        all.extend_from_slice(set);
        let p = params(&all);
        if let Err(e) = self.s.on_params(&p, &self.groups, &self.v.refview()) {
            panic!("on_params: {e:?}");
        }
        self.p = p;
    }

    pub fn feed(&mut self, ev: &EventRef<'_>) -> Out {
        self.bufs.begin();
        let a = hft_testkit::CountingAlloc::is_installed().then(hft_testkit::CountingAlloc::scope);
        {
            let mut ctx = self.bufs.ctx();
            self.s.on_event(ev, &self.v, &mut ctx);
        }
        let allocs = a.map_or(0, |x| x.allocs());
        assert_eq!(allocs, 0, "on_event allocated");
        let mut out = Out::default();
        let mut handles = Vec::new();
        for (h, op) in self.bufs.ops() {
            if let Op::Place(p) = op {
                out.places.push(*p);
                handles.push(h);
            }
        }
        out.notes = self.bufs.notes().to_vec();
        self.bufs.clear_outcomes();
        for h in handles {
            let o = match self.next.take() {
                Some(o) => o,
                None => {
                    let r = r_of(self.next_r);
                    self.next_r += 1;
                    out.refs.push(r);
                    IntentOutcome::Placed(r)
                }
            };
            self.bufs.set_outcome(h, o);
        }
        out
    }

    /// A book update of `U`.
    pub fn book(&mut self) -> Out {
        self.feed(&EventRef::Book { inst: U, gix: 0 })
    }

    /// An update of order `r`.
    pub fn order(&mut self, r: OrderRef, kind: OrderUpdateKind) -> Out {
        let u = OrderUpdate {
            r,
            group: G1,
            tag: 0,
            kind,
        };
        self.feed(&EventRef::Order { gix: 0, u: &u })
    }
}

/// `Filled{done}` with cumulative `cum` (raw).
pub fn filled(cum: i64, done: bool) -> OrderUpdateKind {
    OrderUpdateKind::Filled {
        d: Qty(0),
        px: Px(110),
        fee: 0,
        fee_asset: USDC,
        cum: Qty(cum),
        done,
    }
}

/// `quote` reading the views' current TSC for the connection evidence.
fn now_tsc(v: &MockViews) -> u64 {
    Views::now(v).tsc.0
}
