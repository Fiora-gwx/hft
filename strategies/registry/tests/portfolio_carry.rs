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
use hft_strategy_sdk::Strategy;
use hft_strategy_sdk::params::{RawParams, encode_body, schema_digest};
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
    r.push_body(
        RingId::Bbo(vi(venue(i))),
        &Bbo {
            h: mh(i, r.tsc()),
            bid_px: Px(10_000),
            ask_px: Px(10_001),
            bid_qty: Qty(1_000_000_000),
            ask_qty: Qty(1_000_000_000),
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

#[test]
fn carry_010_linked_entry_funding_and_edge_decay_exit() {
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
    let entry = fill_orders(&mut r, &mut cursor);
    assert_eq!(entry.len(), 2, "out={:?}", r.out());
    assert_eq!(
        (entry[0].inst, entry[0].side),
        (I1, if reverse { Side::Buy } else { Side::Sell })
    );
    assert_eq!(
        (entry[1].inst, entry[1].side),
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
    let exit = fill_orders(&mut r, &mut cursor);
    assert_eq!(exit.len(), 2, "out={:?}", r.out());
    assert!(r.ledger().group_pos(A1, G1).legs.iter().all(|p| p.qty.0 == 0));
    assert_eq!(r.ledger().group(A1, G1).pnl.funding, actual);
    verify(&r);
}
