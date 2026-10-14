//! Carry in the real sequencer, with deterministic mock exchange fills and settlement evidence.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)]
#[path = "carry/common.rs"]
mod common;
#[path = "carry/risk.rs"]
mod risk_config;
use common::*;
use hft_event::*;
use hft_ledger::{Capacities, LedgerConfig};
use hft_risk::{
    AcctLimits, DEFAULT_MARGIN, FixedVec, GroupLimits, MarginLimits, PreVec, PxBand, RiskParams, VenueLimits,
};
use hft_strategy_registry::Strategies;
use hft_strategy_sdk::params::{RawParams, encode_body, schema_digest};
use hft_strategy_sdk::{LinkRole, Strategy};
use hft_testkit::seq_rig::{RigConfig, RingId, SeqRig};
use hft_types::*;
use portfolio_carry::{CarryParams, PortfolioCarry};
use zerocopy::{FromZeros, IntoBytes};

const D: i64 = 1_759_276_800_000_000_000;
const SETTLE: i64 = D + 8 * 3_600_000_000_000;
const UTC0: i64 = SETTLE - 120_000_000_000;
const EPOCH: u16 = 3;

fn spb() -> Vec<u8> {
    let g = spec(G1, &[(I1, V1), (I2, V2)]);
    let mut raw = RawParams {
        defaults: CarryParams::SCHEMA.fields.iter().map(|f| f.default).collect(),
        overrides: vec![],
    };
    raw.defaults[0] = 1;
    let body = encode_body(&CarryParams::SCHEMA, 1, &raw).unwrap();
    let header = StrategyParamsHdr {
        strategy: S1,
        account: A1,
        kind_id: 1,
        schema_id: 1,
        schema_ver: 1,
        source: ParamSource::Manual,
        _pad: 0,
        n_groups: 1,
        _pad2: 0,
        version: 1,
        generated_at_ns: UTC0,
        body_len: body.len() as u32,
        group_ctr_hwm: 1,
        schema_digest: schema_digest(&PortfolioCarry::DESC, 1),
    };
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(g.as_bytes());
    bytes.extend_from_slice(&body);
    bytes
}

fn config() -> RigConfig {
    RigConfig {
        refdata_blob: refdata().blob().to_vec(),
        params: vec![spb()],
        risk: risk_config::risk(),
        risk_blob: risk_config::risk().encode_blob().unwrap(),
        caps: Capacities {
            max_insts: 16,
            max_cells_total: 8,
            max_timers: 512,
            recent_cmd_ids: 128,
            max_subs_per_inst: 8,
            max_groups_per_instance: 8,
            max_instances: 2,
        },
        ledger: LedgerConfig::default(),
        venues: vec![V1, V2],
        keys: vec![],
        epoch: u32::from(EPOCH),
        tsc_hz_milli: 1_000_000_000_000,
        calib_tsc: 0,
        calib_utc_ns: UTC0,
        boot_tsc: 1_000_000,
        max_groups: 8,
    }
}
fn rig() -> SeqRig<Strategies> {
    SeqRig::boot(&config()).unwrap()
}

fn verify(r: &SeqRig<Strategies>) {
    use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
    use hft_sequencer::{BootSource, RecoverPlan, RecoveryFlags, Sequencer, SnapshotSel, StepOutcome};
    let cfg = config();
    let mut setup = hft_testkit::seq_rig::engine_setup(&cfg, &refdata());
    setup.risk_decode = Some(hft_sequencer::config_blob::decode_risk_blob);
    let log = Log::from_segments(&r.journal().segments);
    let first = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    let driver = ReplayDriver::new(ReplayMode::Verify, log, 0, setup.seq.replay_per_round);
    let plan = RecoverPlan {
        start: SnapshotSel::AtSeq(first),
        replay_tail: true,
        epoch: 0,
        flags: RecoveryFlags(0),
        quarantine: vec![],
    };
    let mut replay: Sequencer<ReplayDriver, Strategies> = Sequencer::boot(
        BootSource::Journal(plan),
        &setup,
        driver,
        Box::leak(Box::new(hft_rt::Heartbeat::new())),
    )
    .unwrap();
    for _ in 0..100_000 {
        if !matches!(replay.step(), StepOutcome::Busy) {
            break;
        }
    }
    assert!(
        replay.driver().mismatches().is_empty(),
        "{:?}",
        replay.driver().mismatches()
    );
    assert!(replay.driver().leftover().is_empty());
}

fn vi(v: VenueId) -> u8 {
    if v == V1 { 0 } else { 1 }
}
fn venue(i: InstId) -> VenueId {
    if i == I1 { V1 } else { V2 }
}
fn mh(i: InstId, now: u64) -> MarketHdr {
    let mut h = MarketHdr::new_zeroed();
    h.inst = i;
    h.venue = venue(i);
    h.rx_tsc = now;
    h
}
fn sys(r: &mut SeqRig<Strategies>, id: u32) {
    r.push_body(
        RingId::Hk,
        &Timer {
            timer_id: id,
            owner: 1,
            _pad: 0,
            group: GROUP_NONE,
            _pad2: 0,
            deadline_tsc: r.tsc(),
        },
    );
}
fn quote(r: &mut SeqRig<Strategies>, i: InstId) {
    quote_depth(r, i, 1_000_000_000, 1_000_000_000);
}
fn quote_depth(r: &mut SeqRig<Strategies>, i: InstId, bid: i64, ask: i64) {
    r.push_body(
        RingId::Bbo(vi(venue(i))),
        &Bbo {
            h: mh(i, r.tsc()),
            bid_px: Px(10_000),
            ask_px: Px(10_001),
            bid_qty: Qty(bid),
            ask_qty: Qty(ask),
            update_id: r.tsc(),
        },
    );
    r.push_body(
        RingId::Md(vi(venue(i))),
        &MarkPrice {
            h: mh(i, r.tsc()),
            px: Px(10_000),
        },
    );
}
fn rate(r: &mut SeqRig<Strategies>, i: InstId, amount: i64, next: i64) {
    r.push_body(
        RingId::Md(vi(venue(i))),
        &Funding {
            h: mh(i, r.tsc()),
            rate: Rate(amount),
            next_funding_ns: next,
            interval_s: 28_800,
            _pad: 0,
        },
    );
}

fn ready(r: &mut SeqRig<Strategies>) {
    r.run().unwrap();
    r.set_tsc(1_000_000_000);
    for (v, i) in [(V1, I1), (V2, I2)] {
        for (conn, role, inst, acct) in [
            (0, ConnRole::Md, i, AccountId(0)),
            (0, ConnRole::Md, INST_NONE, AccountId(0)),
            (0, ConnRole::Order, INST_NONE, A1),
            (1, ConnRole::Order, INST_NONE, A1),
            (0, ConnRole::UserStream, INST_NONE, A1),
        ] {
            r.push_body(
                RingId::Rep(vi(v)),
                &ConnState {
                    venue: v,
                    conn,
                    state: ConnStateKind::Up,
                    role,
                    inst,
                    reason: 0,
                    account: acct,
                    _pad2: [0; 3],
                    rx_tsc: r.tsc(),
                },
            );
        }
        let mut config = AccountConfigReportHdr::new_zeroed();
        config.venue = v;
        config.account = A1;
        config.position_mode = 2;
        config.margin_mode = 2;
        config.multi_asset = 1;
        config.source = 1;
        config.n_lev = 1;
        config.rx_tsc = Tsc(r.tsc());
        let mut body = config.as_bytes().to_vec();
        body.extend_from_slice(
            LeverageItem {
                inst: i,
                _pad: 0,
                leverage: Rate(2_000_000_000),
            }
            .as_bytes(),
        );
        r.push(RingId::RepSim, EventKind::ACCOUNT_CONFIG_REPORT, body);
        let mut recon = ReconcileReport::new_zeroed();
        recon.report_id = u64::from(v.0);
        recon.venue = v;
        recon.account = A1;
        recon.item = 5;
        recon.count = 1;
        r.push_body(RingId::RepSim, &recon);
        r.push_body(
            RingId::RepSim,
            &BalanceReport {
                venue: v,
                account: A1,
                asset: USDT,
                _pad: 0,
                free: Qty(10_000_000_000),
                locked: Qty(0),
                ex_ts_ns: UTC0,
                rx_tsc: Tsc(r.tsc()),
            },
        );
        let mut margin = MarginReportHdr::new_zeroed();
        margin.venue = v;
        margin.account = A1;
        margin.source = 1;
        margin.margin_balance = Notional(10_000_000_000);
        margin.avail_margin = Notional(10_000_000_000);
        margin.rx_tsc = Tsc(r.tsc());
        r.push(
            RingId::RepSim,
            EventKind::MARGIN_REPORT,
            margin.as_bytes().to_vec(),
        );
        quote(r, i);
    }
    sys(r, 8);
    sys(r, 16 | (u32::from(A1.0) << 16));
    r.run().unwrap();
    r.cmd(
        A1,
        &ControlCmd::SetMode {
            scope: CmdScope {
                strategy: S1,
                venue: VenueId(0),
                account: A1,
                _pad: 0,
                group: G1,
                inst: INST_NONE,
                _pad2: 0,
            },
            mode: RunMode::Live,
        },
    )
    .unwrap();
    r.run().unwrap();
}

/// The mock fills every IOC on its own venue. Only engine output can cause a fill.
fn fill_orders(r: &mut SeqRig<Strategies>, cursor: &mut usize) -> Vec<GwCmd> {
    let mut fills = vec![];
    for _ in 0..12 {
        r.run().unwrap();
        let out = r.out().get(*cursor..).unwrap_or(&[]).to_vec();
        *cursor = r.out().len();
        let orders: Vec<_> = out
            .into_iter()
            .filter_map(|(_, c)| (c.op == GwOp::Place).then_some(c))
            .collect();
        if orders.is_empty() {
            break;
        }
        for c in orders {
            for what in [OrderEventKind::Ack, OrderEventKind::Fill] {
                let fill = what == OrderEventKind::Fill;
                r.push_body(
                    RingId::RepSim,
                    &OrderEvent {
                        venue: venue(c.inst),
                        account: A1,
                        what,
                        detail: 0,
                        slot: c.slot,
                        r#gen: c.r#gen,
                        inst: c.inst,
                        px: c.px,
                        qty: if fill { c.qty } else { Qty(0) },
                        cum_filled: if fill { c.qty } else { Qty(0) },
                        fee: Qty(0),
                        fee_asset: USDT,
                        epoch: EPOCH,
                        reject_code: 0,
                        exch_order_id: u64::from(c.slot.0) + 1,
                        ex_ts_ns: UTC0,
                        hw_rx_ns: 0,
                        rx_tsc: r.tsc(),
                        avg_px: c.px,
                        cash_qty: Qty(0),
                        cash_sum: Qty(0),
                        trade_id: 0,
                    },
                );
            }
            fills.push(c);
        }
    }
    r.run().unwrap();
    fills
}

fn decisions(r: &SeqRig<Strategies>) -> Vec<OrderDecision> {
    r.journal()
        .records()
        .filter(|r| r.kind == EventKind::ORDER_DECISION)
        .filter_map(|r| r.read::<OrderDecision>())
        .filter(|d| d.action == OrderAction::New)
        .collect()
}

/// Inspect both outbound orders before delivering any exchange response.
fn dispatch_pair(r: &mut SeqRig<Strategies>, cursor: usize) -> [GwCmd; 2] {
    r.run().unwrap();
    let mut out: Vec<_> = r.out()[cursor..]
        .iter()
        .filter_map(|(_, c)| (c.op == GwOp::Place).then_some(*c))
        .collect();
    out.sort_by_key(|c| c.inst.0);
    assert_eq!(out.len(), 2, "both legs must leave before any fill: {out:?}");
    let ds = decisions(r);
    let ds = &ds[ds.len() - 2..];
    assert!(ds.iter().all(|d| d.reject == 0));
    assert_eq!(ds[0].trigger_seq, ds[1].trigger_seq);
    let first = ds.iter().find(|d| d.link_role == LinkRole::First as u8).unwrap();
    let second = ds.iter().find(|d| d.link_role == LinkRole::Second as u8).unwrap();
    assert_eq!(first.group, G1);
    assert_eq!(second.group, G1);
    assert_eq!(second.target_slot, first.slot);
    assert_eq!(out[0].qty, out[1].qty);
    for c in &out {
        assert_eq!(c.tif, Tif::Ioc);
        // Default max_cross = 0.0001; price rounding must stay inside the observed bound.
        match c.side {
            Side::Buy => assert!(i128::from(c.px.0) * 1_000_000_000 <= 10_001 * 1_000_100_000_i128),
            Side::Sell => assert!(i128::from(c.px.0) * 1_000_000_000 >= 10_000 * 999_900_000_i128),
        }
        assert!(c.qty.0 > 0);
        assert_eq!(c.qty.0 % 1_000, 0);
        // px_exp + qty_exp = -10, Notional exponent = -6; cap is 100 quote units.
        assert!(i128::from(c.qty.0) * i128::from(c.px.0) <= 100_000_000_i128 * 10_000);
    }
    [out[0], out[1]]
}

fn terminal(r: &mut SeqRig<Strategies>, c: GwCmd, qty: Qty, query: bool) {
    let what = if query {
        OrderEventKind::QueryResult
    } else if qty.0 == 0 {
        OrderEventKind::Reject
    } else if qty == c.qty {
        OrderEventKind::Fill
    } else {
        OrderEventKind::Expired
    };
    r.push_body(
        RingId::RepSim,
        &OrderEvent {
            venue: venue(c.inst),
            account: A1,
            what,
            detail: if query {
                if qty == c.qty { 3 } else { 4 }
            } else {
                0
            },
            slot: c.slot,
            r#gen: c.r#gen,
            inst: c.inst,
            px: c.px,
            qty,
            cum_filled: qty,
            fee: Qty(0),
            fee_asset: USDT,
            epoch: EPOCH,
            reject_code: 0,
            exch_order_id: u64::from(c.slot.0) + 1,
            ex_ts_ns: UTC0,
            hw_rx_ns: 0,
            rx_tsc: r.tsc(),
            avg_px: c.px,
            cash_qty: Qty(0),
            cash_sum: Qty(0),
            trade_id: 0,
        },
    );
    r.run().unwrap();
}

fn order_connection(r: &mut SeqRig<Strategies>, v: VenueId, state: ConnStateKind) {
    for conn in [0, 1] {
        r.push_body(
            RingId::Rep(vi(v)),
            &ConnState {
                venue: v,
                conn,
                state,
                role: ConnRole::Order,
                inst: INST_NONE,
                reason: 0,
                account: A1,
                _pad2: [0; 3],
                rx_tsc: r.tsc(),
            },
        );
    }
    r.run().unwrap();
}

#[test]
fn carry_010_linked_entry_funding_and_edge_decay_exit() {
    check_simultaneous_slice_uses_both_sides_depth_and_caps();
    check_simultaneous_fault_seeds_repair_and_replay();
    for seed in 0..4 {
        for reverse in [false, true] {
            scenario(seed, reverse);
        }
    }
}

fn scenario(seed: i64, reverse: bool) {
    let mut r = rig();
    ready(&mut r);
    let mut cursor = r.out().len();
    let high = 2_000_000 + seed * 100_000;
    rate(&mut r, I1, if reverse { 200_000 } else { high }, SETTLE);
    rate(&mut r, I2, if reverse { high } else { 200_000 }, SETTLE);
    let pair = dispatch_pair(&mut r, cursor);
    let entry = fill_orders(&mut r, &mut cursor);
    assert_eq!(entry.len(), 2, "out={:?}", r.out());
    assert_eq!(
        (pair[0].inst, pair[0].side),
        (I1, if reverse { Side::Buy } else { Side::Sell })
    );
    assert_eq!(
        (pair[1].inst, pair[1].side),
        (I2, if reverse { Side::Sell } else { Side::Buy })
    );
    assert_eq!(entry[0].qty, entry[1].qty);
    let pos = r.ledger().group_pos(A1, G1);
    assert_eq!(pos.legs[0].qty.0 > 0, reverse);
    assert_eq!(pos.legs[0].qty.0, -pos.legs[1].qty.0);
    r.set_tsc(120_000_000_000);
    let actual = Notional(if reverse { -123_456 } else { 123_456 });
    let settlement = FundingSettle {
        venue: V1,
        account: A1,
        _pad: 0,
        inst: I1,
        amount: actual,
        rate: Rate(0),
        pos_qty: Qty(0),
        settle_ts_ns: SETTLE,
        rx_tsc: Tsc(r.tsc()),
        synth: 0,
        _pad2: [0; 7],
    };
    for _ in 0..2 {
        r.push_body(RingId::RepSim, &settlement);
        r.run().unwrap();
    }
    assert_eq!(r.ledger().group(A1, G1).pnl.funding, actual);
    assert_eq!(r.ledger().account(A1).day.funding, actual);
    r.set_tsc(122_000_000_000);
    quote(&mut r, I1);
    quote(&mut r, I2);
    rate(&mut r, I1, 0, SETTLE + 28_800_000_000_000);
    rate(&mut r, I2, 0, SETTLE + 28_800_000_000_000);
    let closing = dispatch_pair(&mut r, cursor);
    for (open, close) in pair.iter().zip(&closing) {
        assert_eq!(close.side, open.side.opposite());
        assert_eq!(close.qty, open.qty);
    }
    let exit = fill_orders(&mut r, &mut cursor);
    assert_eq!(exit.len(), 2, "out={:?}", r.out());
    assert!(r.ledger().group_pos(A1, G1).legs.iter().all(|p| p.qty.0 == 0));
    assert_eq!(r.ledger().group(A1, G1).pnl.funding, actual);
    verify(&r);
}

fn check_simultaneous_slice_uses_both_sides_depth_and_caps() {
    for reverse in [false, true] {
        // Make each venue's executable side the bottleneck in turn. The other side is a decoy.
        for thin in [I1, I2] {
            let mut r = rig();
            ready(&mut r);
            r.set_tsc(1_001_000_000); // A new update_id makes the changed depth authoritative.
            for i in [I1, I2] {
                let depth = if i == thin { 20_000_000 } else { 30_000_000 };
                let buy = (i == I1) == reverse;
                quote_depth(
                    &mut r,
                    i,
                    if buy { 1_000 } else { depth },
                    if buy { depth } else { 1_000 },
                );
            }
            let cursor = r.out().len();
            rate(&mut r, I1, if reverse { 200_000 } else { 2_000_000 }, SETTLE);
            rate(&mut r, I2, if reverse { 2_000_000 } else { 200_000 }, SETTLE);
            let pair = dispatch_pair(&mut r, cursor);
            assert_eq!(pair[0].qty, Qty(20_000_000));
            verify(&r);
        }
    }
}

fn check_simultaneous_fault_seeds_repair_and_replay() {
    for seed in 0..12 {
        // Replay is checked in every run; a second execution also checks deterministic outputs/state.
        assert_eq!(fault_scenario(seed), fault_scenario(seed), "seed {seed}");
    }
}

fn fault_scenario(seed: usize) -> Vec<u8> {
    let mut r = rig();
    ready(&mut r);
    let reverse = !seed.is_multiple_of(2);
    rate(&mut r, I1, if reverse { 200_000 } else { 2_000_000 }, SETTLE);
    rate(&mut r, I2, if reverse { 2_000_000 } else { 200_000 }, SETTLE);
    let pair = dispatch_pair(&mut r, 0);
    let full = seed / 3 % 2;
    let other = 1 - full;
    let down = seed % 3 == 2;
    let partial = if seed % 3 == 1 {
        0
    } else {
        pair[other].qty.0 / 2 / 1_000 * 1_000
    };
    let before = decisions(&r).len();
    if down {
        order_connection(&mut r, venue(pair[other].inst), ConnStateKind::Down);
        assert!(r.ledger().group_pos(A1, G1).legs.iter().all(|p| p.qty.0 == 0));
    }
    terminal(&mut r, pair[full], pair[full].qty, false);
    assert_eq!(decisions(&r).len(), before, "wait for the unresolved leg");
    assert_eq!(r.ledger().pos(A1, pair[other].inst).qty, Qty(0));
    assert_eq!(
        r.ledger().pos(A1, pair[full].inst).qty.0,
        pair[full].qty.0 * pair[full].side.sign()
    );
    if down {
        order_connection(&mut r, venue(pair[other].inst), ConnStateKind::Up);
        assert_eq!(decisions(&r).len(), before, "reconnect is not terminal evidence");
    }
    terminal(&mut r, pair[other], Qty(partial), down);
    let ds = decisions(&r);
    let repairs = &ds[before..];
    assert_eq!(repairs.len(), 1, "seed {seed}: {repairs:?}");
    let repair = repairs[0];
    assert_eq!(repair.reject, 0);
    assert_eq!(repair.link_role, LinkRole::Reverse as u8);
    assert_eq!(repair.inst, pair[full].inst);
    assert_eq!(repair.side, pair[full].side.opposite());
    assert_eq!(repair.qty, Qty(pair[full].qty.0 - partial));
    let command = *r.out().iter().rev().find(|(_, c)| c.op == GwOp::Place).unwrap();
    terminal(&mut r, command.1, repair.qty, false);
    let pos = r.ledger().group_pos(A1, G1);
    assert_eq!(pos.legs[0].qty.0, partial * pair[0].side.sign());
    assert_eq!(pos.legs[0].qty.0, -pos.legs[1].qty.0);
    assert_eq!(
        decisions(&r).len(),
        before + 1,
        "no duplicate reverse or immediate retry"
    );
    verify(&r);
    let mut evidence = r.ledger_sections();
    for d in decisions(&r) {
        evidence.extend_from_slice(d.as_bytes());
    }
    evidence
}
