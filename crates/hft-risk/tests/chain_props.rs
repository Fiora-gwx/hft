//! Check-chain properties (RSK-030, RSK-044, RSK-045, RSK-271).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

#[path = "model/band_ref.rs"]
mod band_ref;
#[path = "model/chain_ref.rs"]
mod chain_ref;
mod common;

use chain_ref::{Env, Opt};
use common::*;
use hft_event::OrderFlags;
use hft_risk::mode::{ModeErr, ModeInput, cfg_params};
use hft_risk::{EquityView, ExecBounds, InstClassRow, IntentView, Origin, PreVec, PxBand, Risk, Scope};
use hft_strategy_sdk::BboView;
use hft_types::{
    CapSource, GroupId, HedgePxLimits, InstId, KillLevel, Notional, Px, Qty, Rate, RejectBits, RunMode, Side,
    StrategyId, Tif, VenueId, cap_cross_px,
};
use proptest::prelude::*;

// ------------------------------------------------------------------------------------- shared

thread_local! {
    static BASE: W = r0();
}

fn base() -> W {
    BASE.with(|b| b.clone())
}

fn zero_exec(n: usize) -> Vec<ExecBounds> {
    let mut e = exec_bounds(n);
    for b in e.iter_mut() {
        b.force_max_cross = Rate(0);
    }
    e
}

const fn inst_of(k: usize) -> InstId {
    [A1, B1, A2, B2, C1][k]
}
fn tick_of(i: InstId) -> i64 {
    match i.0 {
        1 | 2 | 4 => 1,
        _ => 10,
    }
}
fn lot_of(i: InstId) -> i64 {
    match i.0 {
        1 => 10_000,
        2 => 100_000,
        4 => 1_000_000,
        _ => 100_000,
    }
}
fn venue_of(i: InstId) -> VenueId {
    match i.0 {
        1 | 2 => A,
        3 | 4 => B,
        _ => C,
    }
}

fn set_bbo(w: &mut W, i: InstId, bid: i64, ask: i64) {
    let b = w.mv.bbo_mut(i);
    b.bid_px = Px(bid);
    b.ask_px = Px(ask);
}

// ------------------------------------------------------------------------------------- RSK-030

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]
    #[test]
    fn rsk_030_hedge_px_limits_same_source(
        conn in 1_000u32..=10_000,
        classes in prop::collection::vec((100_000i64..=100_000_000, 100_000i64..=100_000_000, 100_000i64..=100_000_000), 1..=8),
        map in prop::collection::vec(0usize..8, 5),
        k in 0usize..5,
        mid_ticks in 100i64..=10_000_000,
        buy in any::<bool>(),
        frac in 0i64..=200_000_000,
        up in any::<bool>(),
    ) {
        let rd = r0_refdata();
        let mut p = p0_for(rd, 3);
        p.conn_liveness_ms = conn;
        let mut fv = hft_risk::FixedVec::new();
        for (d, i, r) in &classes {
            fv.push(PxBand { max_px_dev: Rate(*d), ioc_max_cross: Rate(*i), rr_max_cross: Rate(*r) });
        }
        p.classes = fv;
        let rows: Vec<InstClassRow> = (0..5)
            .map(|j| InstClassRow { inst: inst_of(j), class: (map[j] % classes.len()) as u8 })
            .collect();
        p.risk_class = PreVec::from_vec(rows);
        let mut w = world_ex(rd, p, MC, CAPS, &zero_exec(3));
        let i = inst_of(k);
        let v = venue_of(i);
        let tick = tick_of(i);
        // mid 1.00..100 000.00 on the tick grid; bid/ask one tick around it.
        let mid = (mid_ticks.max(2 * tick) / tick) * tick;
        set_bbo(&mut w, i, mid - tick, mid + tick);
        let off = (i128::from(mid) * i128::from(frac) / 1_000_000_000) as i64 / tick * tick;
        let px = if up { mid + off } else { mid - off };
        prop_assume!(px > 0);
        let now = w.nowv();
        let lim = w.risk.hedge_px_limits(acc(1), v, i);
        let vf = w.risk.view_for(acc(1), i, &now);
        prop_assert_eq!(lim, HedgePxLimits { conn_liveness_ms: conn, rr_max_cross: vf.rr_max_cross });
        let g = if i == A2 || i == B2 { G3 } else { G1 };
        let side = if buy { Side::Buy } else { Side::Sell };
        let mut it = base_intent(1, g, i, side, "0", "0", );
        it.class = hft_strategy_sdk::IntentClass::Hedge;
        it.origin = Origin::Oms;
        it.link_role = Some(hft_strategy_sdk::LinkRole::Second);
        it.tif = Tif::Ioc;
        it.flags = OrderFlags::RISK_REDUCING.0 | OrderFlags::LINKED.0;
        it.qty = Qty(lot_of(i));
        it.px = Px(px);
        it.notional = Risk::notional(it.qty, it.px, EXP, Rate(1_000_000_000));
        it.hedge_cap_qty = it.qty;
        let r = w.eval(&it).map(|_| ());
        let want = band_ref::rr_ok(buy, px, mid, lim.rr_max_cross.0);
        prop_assert_eq!(r.is_ok(), want, "result {:?}", r);
        if let Err(b) = r {
            prop_assert_eq!(b, RejectBits::PX_BAND);
        }
    }
}

// ------------------------------------------------------------------------------------- RSK-044

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]
    #[test]
    fn rsk_044_engine_orders_pass_band_and_notional(
        on_b in any::<bool>(),
        mid_ticks in 10_000i64..=10_000_000,
        rr in 1_000_000i64..=50_000_000,
        touch_frac in 0i64..=3_000,
        buy in any::<bool>(),
        l3_mode in 0u8..3,
        l3_frac in 0i64..=1_000,
        ioc in 500_000i64..=25_000_000,
        kind in 0u8..3,
    ) {
        let rd = r0_refdata();
        let mut p = p0_for(rd, 3);
        let dev = 30_000_000;
        let mut fv = hft_risk::FixedVec::new();
        fv.push(PxBand { max_px_dev: Rate(dev), ioc_max_cross: Rate(ioc), rr_max_cross: Rate(rr) });
        p.classes = fv;
        p.risk_class = PreVec::from_vec((0..5).map(|j| InstClassRow { inst: inst_of(j), class: 0 }).collect());
        // l3 ∈ {0, rr..2 rr}.
        p.l3_max_cross = if l3_mode == 0 { Rate(0) } else { Rate(rr + rr * l3_frac / 1_000) };
        let l3 = p.l3_max_cross;
        let mut w = world_ex(rd, p, MC, CAPS, &zero_exec(3));
        let i = if on_b { B1 } else { A1 };
        let tick = tick_of(i);
        let lot = lot_of(i);
        // mid 100.00..100 000.00 (raw at exponent −2).
        let mid = (mid_ticks / tick) * tick;
        let off_raw = (i128::from(mid) * i128::from(rr) * i128::from(touch_frac) / 1_000 / 1_000_000_000) as i64;
        let off = (off_raw / tick).max(1) * tick;
        prop_assume!(mid - off > 0);
        set_bbo(&mut w, i, mid - off, mid + off);
        let side = if buy { Side::Buy } else { Side::Sell };
        let touch = Px(if buy { mid + off } else { mid - off });
        let (max_n, it_kind) = match kind {
            0 => (n("10000"), 0u8), // Flatten (max_reduce_notional)
            1 => (n("20000"), 1),   // hedge leg (max_hedge_notional)
            _ => (n("10000"), 2),   // Open slice (max_order_notional)
        };
        let (cross, mid_cap) = match it_kind {
            0 => {
                let eff = if l3.0 > 0 { Rate(rr.max(l3.0)) } else { Rate(rr) };
                (eff, eff)
            }
            1 => (Rate(rr), Rate(rr)),
            _ => (Rate(ioc), Rate(dev)),
        };
        let lim = HedgePxLimits { conn_liveness_ms: 2_000, rr_max_cross: mid_cap };
        let px = cap_cross_px(side, touch, Px(mid), cross, &lim, tick);
        prop_assume!(px.0 > 0);
        // qty = ⌊max_notional / px⌋ on the lot grid (notional raw 1e-6, qty×px raw 1e-10).
        let q_raw = (i128::from(max_n.0) * 10_000 / i128::from(px.0)) as i64 / lot * lot;
        prop_assume!(q_raw >= lot);
        let qty = Qty(q_raw);
        let g = G1;
        let mut it = base_intent(1, g, i, side, "0", "0");
        it.qty = qty;
        it.px = px;
        it.notional = Risk::notional(qty, px, EXP, Rate(1_000_000_000));
        // Position large enough for any reduce.
        w.mv.pos.insert((1, i.0), if buy { -1_000_000_000_000 } else { 1_000_000_000_000 });
        match it_kind {
            0 => {
                it.class = hft_strategy_sdk::IntentClass::Flatten;
                it.origin = Origin::Exec;
                it.tif = Tif::Ioc;
                it.flags = OrderFlags::RISK_REDUCING.0 | if on_b { OrderFlags::REDUCE_ONLY.0 } else { 0 };
            }
            1 => {
                it.class = hft_strategy_sdk::IntentClass::Hedge;
                it.origin = Origin::Oms;
                it.link_role = Some(hft_strategy_sdk::LinkRole::Second);
                it.tif = Tif::Ioc;
                it.flags = OrderFlags::RISK_REDUCING.0 | OrderFlags::LINKED.0;
                it.hedge_cap_qty = qty;
            }
            _ => {
                it.origin = Origin::Exec;
                it.tif = Tif::Ioc;
            }
        }
        let r = w.eval(&it).map(|_| ());
        if let Err(b) = r {
            prop_assert!(
                b != RejectBits::PX_BAND && b != RejectBits::NOTIONAL_ORDER && b != RejectBits::HEDGE_LIMIT,
                "kind {} rejected {:?} (px {:?}, qty {:?}, notional {:?})", it_kind, b, px, qty, it.notional
            );
        }
    }
}

// ------------------------------------------------------------------------------------- RSK-045

const OPTS: [Opt; 24] = [
    Opt::KillGlobal { hard: false },
    Opt::KillGlobal { hard: true },
    Opt::KillAcct {
        a: 1,
        hard: false,
        flatten: false,
    },
    Opt::KillAcct {
        a: 1,
        hard: true,
        flatten: false,
    },
    Opt::KillAcct {
        a: 1,
        hard: false,
        flatten: true,
    },
    Opt::KillAcct {
        a: 2,
        hard: false,
        flatten: false,
    },
    Opt::KillVenue { v: B, a: 1 },
    Opt::KillVenue { v: A, a: 2 },
    Opt::Draining,
    Opt::Daily { a: 1 },
    Opt::Recon { a: 1, v: B },
    Opt::Recon { a: 2, v: A },
    Opt::Unknown { a: 1, v: B },
    Opt::Broken { a: 1, g: G1 },
    Opt::Margin { a: 1 },
    Opt::Storage,
    Opt::Cfg { a: 1, v: B },
    Opt::Recovery { a: 2 },
    Opt::Halted { s: 1, a: 1, g: G1 },
    Opt::Delist { v: B, i: B1 },
    Opt::Delist { v: A, i: A2 },
    Opt::ClockBad,
    Opt::CloseCell { s: 1, a: 1, g: G3 },
    Opt::Margin { a: 2 },
];

/// Applies one option to the product world.
fn apply_opt(w: &mut W, o: Opt) -> Result<(), ModeErr> {
    match o {
        Opt::KillGlobal { hard } => w.apply(kill(KillLevel::Global, sc_any(), true, false, hard)),
        Opt::KillAcct { a, hard, flatten } => {
            w.apply(kill(KillLevel::Account, sc_acct(a), true, flatten, hard))
        }
        Opt::KillVenue { v, a } => w.apply(kill_venue(v, a)),
        Opt::Draining => w.apply(draining()),
        Opt::Daily { a } => w.apply(daily(a)),
        Opt::Recon { a, v } => w.apply(auto(CapSource::RECON, sc_av(a, v))),
        Opt::Unknown { a, v } => w.apply(auto(CapSource::UNKNOWN_TIMEOUT, sc_av(a, v))),
        Opt::Broken { a, g } => {
            w.apply(broken(a, g))?;
            w.apply(auto(CapSource::HEDGE_FAILURE, sc_acct(a)))
        }
        Opt::Margin { a } => w.apply(margin_auto(a, 1, 1 << B.0)),
        Opt::Storage => w.apply(ModeInput::Storage {
            kind: hft_risk::mode::StorageKind::PersistLag,
            level: 1,
        }),
        Opt::Cfg { a, v } => w.apply(ModeInput::Auto(rec(
            CapSource::ACCOUNT_CFG,
            0,
            2,
            sc_av(a, v),
            cfg_params::MISMATCH,
        ))),
        Opt::Recovery { a } => w.apply(auto(CapSource::RECOVERY, sc_acct(a))),
        Opt::Halted { s, a, g } => w.apply(halted(s, a, g)),
        Opt::Delist { v, i } => {
            let cmd = hft_risk::mode::VenueNoticeCmd {
                venue: v,
                kind: hft_risk::mode::VenueNoticeCmd::DELISTING,
                inst: i,
                a: 1_700_000_000_000_000_000,
                b: 0,
            };
            w.apply(ModeInput::Notice(&cmd))
        }
        Opt::ClockBad => w.apply(ModeInput::Clock { ok: false }),
        Opt::CloseCell { s, a, g } => w.apply(set_mode(s, a, g, RunMode::CloseOnly)),
    }
}

/// Generated state besides the caps.
#[derive(Clone, Debug)]
struct ViewGen {
    not_ready: [bool; 3],
    unknown: [bool; 3],
    bbo: [u8; 4],
    b1_pos: u8,
    a1_pos: u8,
    b1_open: (u8, u8, u8),
    b1_pend: (u8, u8, u8),
    held: (u8, u8, u8),
    equity: u8,
    quote: u8,
    margin: u8,
    used_a: u8,
    used_b: u8,
    backoff: u8,
    legs: u8,
    lev_unknown: bool,
}

fn view_gen() -> impl Strategy<Value = ViewGen> {
    let w = |p: f64| prop::bool::weighted(p);
    (
        (
            [w(0.08), w(0.08), w(0.08)],
            [w(0.08), w(0.08), w(0.08)],
            [0u8..16, 0u8..16, 0u8..16, 0u8..16],
        ),
        (0u8..6, 0u8..3, (0u8..2, 0u8..2, 0u8..2), (0u8..2, 0u8..2, 0u8..2)),
        ((0u8..3, 0u8..2, 0u8..2), 0u8..3, 0u8..4, 0u8..4),
        (0u8..4, 0u8..2, 0u8..3, 0u8..4, w(0.1)),
    )
        .prop_map(
            |((nr, un, bb), (bp, ap, bo, bpe), (h, eq, qu, ma), (ua, ub, bk, lg, lu))| ViewGen {
                not_ready: nr,
                unknown: un,
                bbo: bb,
                b1_pos: bp,
                a1_pos: ap,
                b1_open: bo,
                b1_pend: bpe,
                held: h,
                equity: eq,
                quote: qu,
                margin: ma,
                used_a: ua,
                used_b: ub,
                backoff: bk,
                legs: lg,
                lev_unknown: lu,
            },
        )
}

/// Generated intent.
#[derive(Clone, Debug)]
struct ItGen {
    kind: u8,
    acct2: bool,
    inst: usize,
    buy: bool,
    ioc: bool,
    lots: usize,
    px: usize,
    cap_short: bool,
    ro: bool,
    bump: u8,
    bad: u8,
    exec: bool,
    link: u8,
    link_n: u8,
}

fn it_gen() -> impl Strategy<Value = ItGen> {
    (
        (
            0u8..8,
            any::<bool>(),
            0usize..4,
            any::<bool>(),
            any::<bool>(),
            0usize..5,
            0usize..12,
        ),
        (
            any::<bool>(),
            any::<bool>(),
            0u8..4,
            0u8..40,
            any::<bool>(),
            0u8..4,
            0u8..4,
        ),
    )
        .prop_map(
            |((kind, acct2, inst, buy, ioc, lots, px), (cap_short, ro, bump, bad, exec, link, link_n))| {
                ItGen {
                    kind,
                    acct2,
                    inst,
                    buy,
                    ioc,
                    lots,
                    px,
                    cap_short,
                    ro,
                    bump,
                    bad,
                    exec,
                    link,
                    link_n,
                }
            },
        )
}

/// R0 bid/ask raw of an inst.
fn r0_quote(i: InstId) -> (i64, i64) {
    match i.0 {
        1 => (4_999_900, 5_000_100),
        2 => (249_950, 250_050),
        3 => (4_999_500, 5_000_500),
        _ => (249_900, 250_100),
    }
}

/// Candidate limit prices around the band edges of the inst's class.
fn px_candidate(i: InstId, buy: bool, k: usize) -> i64 {
    let (bid, ask) = r0_quote(i);
    let mid = (bid + ask) / 2;
    let tick = tick_of(i);
    let (dev, ioc, rr) = if i == A2 || i == B2 {
        (50_000_000i64, 10_000_000i64, 20_000_000i64)
    } else {
        (30_000_000, 5_000_000, 10_000_000)
    };
    let s: i64 = if buy { 1 } else { -1 };
    let edge =
        |base: i64, rate: i64| (i128::from(base) * i128::from(rate) / 1_000_000_000) as i64 / tick * tick;
    let touch = if buy { ask } else { bid };
    let px = match k {
        0 => mid / tick * tick,
        1 => touch,
        2 => mid + s * edge(mid, dev),
        3 => mid + s * (edge(mid, dev) + tick),
        4 => mid + s * edge(mid, rr),
        5 => mid + s * (edge(mid, rr) + tick),
        6 => touch + s * edge(touch, ioc),
        7 => touch + s * (edge(touch, ioc) + tick),
        8 => mid - s * edge(mid, rr),
        9 => mid - s * (edge(mid, dev) + tick),
        10 => touch + s * tick,
        _ => mid + s * 3 * tick,
    };
    px.max(tick)
}

const LOTS: [i64; 5] = [1, 2, 10, 100, 1_000];

struct Built {
    it: IntentView,
}

/// Builds the intent of a generated case against the world (positions already set).
fn build_intent(w: &W, g: &ItGen) -> Built {
    let side = if g.buy { Side::Buy } else { Side::Sell };
    let mut i = inst_of(g.inst);
    let mut a = 1u8;
    let mut grp = if i == A2 || i == B2 { G3 } else { G1 };
    if g.acct2 && matches!(g.kind, 0 | 5 | 7) || g.kind == 7 {
        a = 2;
        if i == A2 {
            i = A1;
        }
        if i == B2 {
            i = B1;
        }
        grp = G2;
    }
    let lot = lot_of(i);
    let qty = LOTS[g.lots] * lot;
    let px = px_candidate(i, g.buy, g.px);
    let mk = |w: &W| {
        let mut it = base_intent(a, grp, i, side, "0", "0");
        it.qty = Qty(qty);
        it.px = Px(px);
        it.notional = Risk::notional(it.qty, it.px, EXP, Rate(1_000_000_000));
        let _ = w;
        it
    };
    let mut it = match g.kind {
        0 | 7 => {
            let mut it = mk(w);
            it.tif = if g.ioc { Tif::Ioc } else { Tif::Gtc };
            it
        }
        1 | 2 => {
            let mut it = mk(w);
            it.class = hft_strategy_sdk::IntentClass::Hedge;
            it.tif = Tif::Ioc;
            if g.kind == 1 {
                it.origin = Origin::Oms;
                it.link_role = Some(hft_strategy_sdk::LinkRole::Second);
                it.flags = OrderFlags::RISK_REDUCING.0 | OrderFlags::LINKED.0;
            } else {
                it.origin = Origin::Exec;
                it.link_role = Some(hft_strategy_sdk::LinkRole::HedgeFix);
                it.flags = OrderFlags::RISK_REDUCING.0 | OrderFlags::HEDGE_FIX.0;
            }
            if g.ro && venue_of(i) != A {
                it.flags |= OrderFlags::REDUCE_ONLY.0;
            }
            it.hedge_cap_qty = Qty(if g.cap_short { qty - lot } else { qty });
            it
        }
        3 => {
            let it = mk(w);
            let d = draft(&it, hft_strategy_sdk::IntentClass::Reduce);
            let c = Risk::classify(
                &d,
                Origin::Strategy,
                &w.mv.pos_facts(a, i),
                w.rd.caps(venue_of(i)),
            );
            classified(it, c)
        }
        4 => {
            let mut it = mk(w);
            it.origin = if g.exec { Origin::Exec } else { Origin::Strategy };
            it.tif = Tif::Ioc;
            let d = draft(&it, hft_strategy_sdk::IntentClass::Flatten);
            let c = Risk::classify(&d, it.origin, &w.mv.pos_facts(a, i), w.rd.caps(venue_of(i)));
            classified(it, c)
        }
        5 => {
            let sa = if g.bad == 3 { 3 } else { a };
            smoke(sa, sa)
        }
        _ => {
            let ln = ["495", "505", "20000", "20000.000001"][usize::from(g.link_n)];
            match g.link {
                0 => link_open(
                    1,
                    G1,
                    A1,
                    side,
                    "0.01",
                    "50000.00",
                    (1, B, B1, Side::Sell, "0.01", ln),
                ),
                1 => link_open(
                    2,
                    G2,
                    A1,
                    side,
                    "0.01",
                    "50000.00",
                    (1, B, B1, Side::Sell, "0.01", ln),
                ),
                2 => link_open(
                    1,
                    G1,
                    B1,
                    Side::Sell,
                    "0.01",
                    "49995.0",
                    (1, A, A1, Side::Buy, "0.01", ln),
                ),
                _ => link_open(
                    1,
                    G1,
                    B1,
                    Side::Buy,
                    "0.01",
                    "50005.0",
                    (1, A, A1, Side::Sell, "0.01", ln),
                ),
            }
        }
    };
    match g.bump {
        1 => it.notional = Notional(it.notional.0 + 1),
        2 => it.notional = n("10000.000001"),
        _ => {}
    }
    match g.bad {
        0 => it.qty = Qty(it.qty.0 + 1),
        1 => it.px = Px(it.px.0 + 1),
        2 => it.strategy = StrategyId(if it.strategy.0 == 1 { 2 } else { 1 }),
        _ => {}
    }
    Built { it }
}

/// Sets up a world for a generated case; `None` when an input is refused.
fn setup(
    opts: &[bool],
    v: &ViewGen,
) -> Option<(
    W,
    Vec<Opt>,
    Vec<((VenueId, u8), i64)>,
    Vec<(VenueId, u8)>,
    Vec<((u8, GroupId), (i64, i64))>,
    Vec<((u8, InstId), i64)>,
)> {
    let mut w = base();
    // Token use first (on the clean state).
    let used_a = [0i64, 79, 80, 100][usize::from(v.used_a)];
    let used_b = [0i64, 80][usize::from(v.used_b)];
    let ha = hedge(1, G1, A1, Side::Buy, "0.0001", "50001.00", "0.0001");
    let hb = hedge(1, G1, B1, Side::Sell, "0.001", "49995.0", "0.001");
    w.commits(&ha, used_a as usize);
    w.commits(&hb, used_b as usize);
    let mut used = vec![((A, 1u8), used_a), ((B, 1u8), used_b)];
    let mut backoff = Vec::new();
    match v.backoff {
        1 => {
            w.hit(A, 1);
            backoff.push((A, 1));
        }
        2 => {
            w.hit(B, 1);
            backoff.push((B, 1));
        }
        _ => {}
    }
    for (k, on) in opts.iter().enumerate() {
        if *on && apply_opt(&mut w, OPTS[k]).is_err() {
            return None;
        }
    }
    let caps: Vec<Opt> = opts
        .iter()
        .enumerate()
        .filter(|(_, on)| **on)
        .map(|(k, _)| OPTS[k])
        .collect();
    // View.
    for (k, (vv, a)) in [(A, 1u8), (B, 1), (A, 2)].into_iter().enumerate() {
        if v.not_ready[k] {
            w.mv.not_ready.insert((vv.0, a));
        }
    }
    for (k, (a, g)) in [(1u8, G1), (2, G2), (1, G3)].into_iter().enumerate() {
        if v.unknown[k] {
            w.mv.unknown.insert((a, g.0));
        }
    }
    for (k, i) in [A1, B1, A2, B2].into_iter().enumerate() {
        match v.bbo[k] {
            0 => bid_empty(&mut w, i),
            1 => w.mv.bbo_mut(i).conn_up = false,
            2 => w.mv.bbo_mut(i).resync = true,
            3 => {
                w.mv.silence.insert(i.0, 2_000_000_000);
            }
            4 => {
                w.mv.silence.insert(i.0, 2_000_000_001);
            }
            _ => {}
        }
    }
    w.mv.set_pos(
        1,
        B1,
        ["0", "-0.02", "0.5", "-1", "-0.9", "0.02"][usize::from(v.b1_pos)],
    );
    w.mv.set_pos(1, A1, ["0", "0.01", "0.02"][usize::from(v.a1_pos)]);
    let pick = |b: u8, x: &'static str| if b == 1 { x } else { "0" };
    w.mv.set_open(1, B1, Side::Buy, pick(v.b1_open.0, "0.4"));
    w.mv.set_open(1, B1, Side::Sell, pick(v.b1_open.1, "0.4"));
    w.mv.set_open_imm(1, B1, Side::Buy, pick(v.b1_open.2, "0.2"));
    w.mv.set_pend(1, B1, Side::Sell, pick(v.b1_pend.0, "0.1"));
    w.mv.set_pend(1, B1, Side::Buy, pick(v.b1_pend.1, "0.1"));
    w.mv.set_pend_imm(1, B1, Side::Sell, pick(v.b1_pend.2, "0.1"));
    w.mv.set_held(
        1,
        Scope::Account,
        ["0", "95000", "95000.000001"][usize::from(v.held.0)],
    );
    w.mv.set_held(1, Scope::Group(G1), ["0", "45000"][usize::from(v.held.1)]);
    w.mv.set_held(1, Scope::Venue(A), ["0", "75000.000001"][usize::from(v.held.2)]);
    match v.equity {
        1 => {
            w.mv.equity.insert(
                1,
                EquityView {
                    total: n("90000"),
                    valid: true,
                },
            );
        }
        2 => {
            w.mv.equity.insert(
                1,
                EquityView {
                    total: n("1000"),
                    valid: false,
                },
            );
        }
        _ => {}
    }
    match v.quote {
        1 => {
            w.mv.avail_quote.insert((1, A.0), Some(n("530.25").0));
        }
        2 => {
            w.mv.avail_quote.insert((1, A.0), Some(n("530.249999").0));
        }
        3 => {
            w.mv.avail_quote.insert((1, A.0), None);
        }
        _ => {}
    }
    match v.margin {
        1 => {
            w.mv.avail_margin.insert((1, B.0), Some(n("173.25").0));
        }
        2 => {
            w.mv.avail_margin.insert((1, B.0), Some(n("173.249999").0));
        }
        3 => {
            w.mv.avail_margin.insert((1, B.0), None);
        }
        _ => {}
    }
    let mut legs = Vec::new();
    let lg = [
        None,
        Some(("0.5", "1000")),
        Some(("0.500000001", "100")),
        Some(("0.9", "1000")),
    ][usize::from(v.legs)];
    if let Some((ra, na)) = lg {
        w.legs(1, G1, ra, na);
        legs.push(((1u8, G1), (r(ra).0, n(na).0)));
    }
    let mut lev = vec![
        ((1u8, B1), 3_000_000_000i64),
        ((1, B2), 3_000_000_000),
        ((1, C1), 3_000_000_000),
    ];
    for i in [B1, B2] {
        lev.push(((2, i), 3_000_000_000));
    }
    if v.lev_unknown {
        w.risk
            .on_account_cfg(&hdr(1, B, 0, KEYS), &[lev_item(B1, "0"), lev_item(B2, "3.00")]);
        lev.retain(|(k, _)| *k != (1, B1));
    }
    used.retain(|(_, u)| *u != 0);
    Some((w, caps, used, backoff, legs, lev))
}

fn lev_item(i: InstId, x: &str) -> hft_event::LeverageItem {
    lev(i, x)
}

const RR_NEVER: [RejectBits; 11] = [
    RejectBits::CLOCK,
    RejectBits::CLOSE_ONLY,
    RejectBits::KILLED,
    RejectBits::DRAINING,
    RejectBits::DAILY_LOSS,
    RejectBits::RECON_DEVIATION,
    RejectBits::VENUE_NOT_READY,
    RejectBits::UNKNOWN_PENDING,
    RejectBits::EXPOSURE_GATE,
    RejectBits::LINK_STALE,
    RejectBits::LINK_FUNDS,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]
    #[test]
    fn rsk_045_single_bit_matches_reference(
        opts in prop::collection::vec(prop::bool::weighted(0.12), OPTS.len()),
        v in view_gen(),
        g in it_gen(),
    ) {
        let s = setup(&opts, &v);
        prop_assume!(s.is_some());
        let (mut w, caps, used, backoff, legs, lev) = s.unwrap();
        let b = build_intent(&w, &g);
        let got = w.eval(&b.it).map(|_| ());
        let env = Env {
            mv: &w.mv,
            caps: &caps,
            now_tsc: w.now as u64,
            used: &used,
            backoff: &backoff,
            legs: &legs,
            lev: &lev,
        };
        let want = chain_ref::first_fail(&b.it, &env);
        if let Err(bit) = got {
            prop_assert_eq!(bit.0.count_ones(), 1, "more than one bit: {:?}", bit);
            if b.it.flags & OrderFlags::RISK_REDUCING.0 != 0 {
                prop_assert!(!RR_NEVER.contains(&bit), "risk-reducing intent got {:?}", bit);
            }
        }
        prop_assert_eq!(got.err(), want, "intent {:?} caps {:?}", b.it, caps);
    }
}

// ------------------------------------------------------------------------------------- RSK-271

/// The cap options of RSK-271, applied to account `a` (`s`/`g` = its bound strategy and group).
fn sym_opt(k: u8, a: u8, s: u8, g: GroupId) -> Vec<ModeInput<'static>> {
    match k {
        0 => vec![kill_acct(a, false)],
        1 => vec![kill_acct(a, true)],
        2 => vec![daily(a)],
        3 => vec![margin_auto(a, 1, 1 << B.0)],
        4 => vec![kill_venue(B, a)],
        5 => vec![auto(CapSource::RECON, sc_av(a, B))],
        6 => vec![broken(a, g), auto(CapSource::HEDGE_FAILURE, sc_acct(a))],
        7 => vec![halted(s, a, g)],
        8 => vec![kill_flatten(a)],
        _ => vec![auto(CapSource::UNKNOWN_TIMEOUT, sc_av(a, B))],
    }
}

fn global_opt(k: u8) -> ModeInput<'static> {
    match k {
        0 => kill_global(false),
        1 => kill_global(true),
        2 => draining(),
        3 => ModeInput::Clock { ok: false },
        _ => ModeInput::Storage {
            kind: hft_risk::mode::StorageKind::PersistLag,
            level: 1,
        },
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]
    #[test]
    fn rsk_271_sim_same_as_live(
        sym in prop::collection::vec(prop::bool::weighted(0.2), 10),
        glob in prop::collection::vec(prop::bool::weighted(0.1), 5),
        kind in 0u8..5,
        buy in any::<bool>(),
        tokens in 0usize..=100,
    ) {
        let side = if buy { Side::Buy } else { Side::Sell };
        let mk = |a: u8, g: GroupId, w: &W| -> IntentView {
            match kind {
                0 => open(a, g, A1, side, "0.01", if buy { "50000.00" } else { "49999.00" }, Tif::Gtc),
                1 => open(a, g, B1, side, "0.01", if buy { "50000.0" } else { "49995.0" }, Tif::Gtc),
                2 => hedge(a, g, B1, Side::Sell, "0.01", "49995.0", "0.01"),
                3 => w.reduce(a, g, B1, Side::Buy, "0.01", "50005.0"),
                _ => w.flatten(a, g, B1, Side::Buy, "0.01", "50005.0", Origin::Exec),
            }
        };
        let mut worlds = Vec::new();
        for (a, s, g) in [(1u8, 1u8, G1), (3, 3, GS)] {
            let mut w = base();
            let h = hedge(a, g, A1, Side::Buy, "0.0001", "50001.00", "0.0001");
            w.commits(&h, tokens);
            w.mv.set_pos(a, B1, "-0.02");
            let mut ok = true;
            for (k, on) in glob.iter().enumerate() {
                if *on {
                    ok &= w.apply(global_opt(k as u8)).is_ok();
                }
            }
            for (k, on) in sym.iter().enumerate() {
                if *on {
                    for inp in sym_opt(k as u8, a, s, g) {
                        ok &= w.apply(inp).is_ok();
                    }
                }
            }
            prop_assume!(ok);
            worlds.push((w, a, g));
        }
        let (mut wl, al, gl) = worlds.remove(0);
        let (mut ws, as_, gs) = worlds.remove(0);
        let il = mk(al, gl, &wl);
        let is = mk(as_, gs, &ws);
        let rl = wl.eval(&il).map(|_| ());
        let rs = ws.eval(&is).map(|_| ());
        prop_assert_eq!(rl, rs);
        // The Sim account occupies no kill bit: its world's kill words equal a world with only
        // the global options.
        let mut wg = base();
        for (k, on) in glob.iter().enumerate() {
            if *on {
                let _ = wg.apply(global_opt(k as u8));
            }
        }
        prop_assert_eq!(ws.risk.mode.kill_bits(), wg.risk.mode.kill_bits());
    }
}

#[allow(dead_code)]
fn unused(_: BboView) {}
