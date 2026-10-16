//! Scenario generator of the property tests (SIM-114, SIM-120, SIM-121, SIM-124): records and
//! commands over the `rd_sim()` insts, 1–4 Sim accounts, Fixed or Empirical latency.
#![allow(dead_code, clippy::type_complexity)]

use hft_event::{
    Bbo, Funding, GwCmd, GwOp, L2BatchHdr, L2BatchView, L2SnapHdr, L2SnapshotPart, Level, MarkPrice, RepMsg,
    Trade,
};
use hft_refdata::{RefData, SimAccountCfg};
use hft_sim::{Dist, LatencyModel, MarketEventRef, SimExchange, SimInput, SimSetup};
use hft_types::{AccountId, InstId, Px, Qty, Side, SlotId, Tif, Tsc};
use proptest::prelude::*;
use zerocopy::IntoBytes;

use super::*;

pub const INSTS: [InstId; 4] = [BTC_S1, BNB_S1, BTC_P2, ETH_P2];
pub const ACCTS: [AccountId; 4] = [A9, AccountId(6), AccountId(7), A8];

/// Base price (raw, exponent −4), tick and lot of each generated inst.
pub fn shape(i: InstId) -> (i64, i64, i64) {
    match i.0 {
        11 => (600_000_000, 100, 1_000),
        12 => (6_000_000, 100, 100_000),
        21 => (600_000_000, 1_000, 100_000),
        _ => (30_000_000, 100, 1_000_000),
    }
}

#[derive(Clone, Debug)]
pub enum Ev {
    Bbo {
        inst: usize,
        off: i64,
        spread: i64,
        qty: i64,
    },
    TopN {
        inst: usize,
        off: i64,
        n: usize,
        qty: i64,
    },
    Batch {
        inst: usize,
        off: i64,
    },
    Trade {
        inst: usize,
        off: i64,
        qty: i64,
    },
    Funding {
        inst: usize,
        rate: i64,
        next_ms: u64,
    },
    Mark {
        inst: usize,
        off: i64,
    },
    Place {
        acct: usize,
        inst: usize,
        buy: bool,
        lots: i64,
        off: i64,
        tif: u8,
    },
    Cancel {
        acct: usize,
        back: u32,
    },
    Amend {
        acct: usize,
        back: u32,
        off: i64,
    },
    Query {
        acct: usize,
        back: u32,
    },
    CancelAll {
        acct: usize,
        inst: Option<usize>,
    },
    Transfer {
        acct: usize,
        amount: i64,
    },
    Reset {
        acct: usize,
    },
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub n_accts: usize,
    pub n_insts: usize,
    pub empirical: bool,
    /// `(gap_ms, event)`.
    pub evs: Vec<(u64, Ev)>,
}

fn ev_strategy(n_accts: usize, n_insts: usize) -> impl Strategy<Value = Ev> {
    let a = 0..n_accts;
    let i = 0..n_insts;
    prop_oneof![
        6 => (i.clone(), -50i64..50, 1i64..5, 1i64..2_000).prop_map(|(inst, off, spread, qty)| Ev::Bbo { inst, off, spread, qty }),
        4 => (i.clone(), -50i64..50, 1usize..6, 1i64..2_000).prop_map(|(inst, off, n, qty)| Ev::TopN { inst, off, n, qty }),
        1 => (i.clone(), -50i64..50).prop_map(|(inst, off)| Ev::Batch { inst, off }),
        3 => (i.clone(), -60i64..60, 1i64..500).prop_map(|(inst, off, qty)| Ev::Trade { inst, off, qty }),
        1 => (i.clone(), -100_000i64..100_000, 1u64..20_000).prop_map(|(inst, rate, next_ms)| Ev::Funding { inst, rate, next_ms }),
        3 => (i.clone(), -400i64..400).prop_map(|(inst, off)| Ev::Mark { inst, off }),
        6 => (a.clone(), i.clone(), any::<bool>(), 1i64..20, -40i64..40, 0u8..4)
            .prop_map(|(acct, inst, buy, lots, off, tif)| Ev::Place { acct, inst, buy, lots, off, tif }),
        2 => (a.clone(), 0u32..5).prop_map(|(acct, back)| Ev::Cancel { acct, back }),
        1 => (a.clone(), 0u32..5, -20i64..20).prop_map(|(acct, back, off)| Ev::Amend { acct, back, off }),
        1 => (a.clone(), 0u32..8).prop_map(|(acct, back)| Ev::Query { acct, back }),
        1 => (a.clone(), proptest::option::of(i.clone())).prop_map(|(acct, inst)| Ev::CancelAll { acct, inst }),
        1 => (a.clone(), 1i64..2_000).prop_map(|(acct, amount)| Ev::Transfer { acct, amount }),
        1 => a.prop_map(|acct| Ev::Reset { acct }),
    ]
}

/// 1–4 accounts, 1–4 insts, up to `max_evs` events.
pub fn scenario(max_evs: usize) -> impl Strategy<Value = Scenario> {
    (1usize..=4, 1usize..=4, any::<bool>()).prop_flat_map(move |(na, ni, emp)| {
        proptest::collection::vec((0u64..300, ev_strategy(na, ni)), 0..max_evs).prop_map(move |evs| {
            Scenario {
                n_accts: na,
                n_insts: ni,
                empirical: emp,
                evs,
            }
        })
    })
}

pub fn cfg_gen() -> SimAccountCfg {
    SimAccountCfg {
        initial_balances: vec![
            bal(S1, USDT, usdt("100000")),
            bal(S1, BTC, q("1")),
            bal(S1, BNB, q("50")),
            bal(P2, USDT, usdt("20000")),
        ]
        .into(),
        latency_model: 0,
        fee_tier: 1,
        margin_model: 0,
    }
}

pub fn setup_for(sc: &Scenario, seed: u64) -> SimSetup {
    let mut s = setup_sim();
    s.tuning.seed = seed;
    s.accounts = ACCTS[..sc.n_accts].iter().map(|a| (*a, cfg_gen())).collect();
    if sc.empirical {
        let d = Dist::from_points(&[(0, 500), (5_000, 1_500), (9_000, 4_000), (10_000, 20_000)]);
        s.latency_models = vec![LatencyModel::Empirical { entry: d, resp: d }];
    }
    s
}

/// One delivered reply rendered to bytes (account, due, kind, body, bulk).
pub fn reply_bytes(account: AccountId, due: u64, m: &RepMsg, bulk: &[u8]) -> Vec<u8> {
    let mut v = vec![account.0];
    v.extend_from_slice(&due.to_le_bytes());
    let (tag, body): (u8, &[u8]) = match m {
        RepMsg::OrderEvent(x, _) => (1, x.as_bytes()),
        RepMsg::SendReport(x) => (2, x.as_bytes()),
        RepMsg::PositionReport(x) => (3, x.as_bytes()),
        RepMsg::BalanceReport(x) => (4, x.as_bytes()),
        RepMsg::FundingSettle(x) => (5, x.as_bytes()),
        RepMsg::ConnState(x) => (6, x.as_bytes()),
        RepMsg::ExternalFill(x) => (7, x.as_bytes()),
        RepMsg::AccountConfigReport(x) => (8, x.as_bytes()),
        RepMsg::MarginReport(x) => (9, x.as_bytes()),
        RepMsg::Note(x) => (10, x.as_bytes()),
    };
    v.push(tag);
    v.extend_from_slice(body);
    v.extend_from_slice(bulk);
    v
}

/// Observer of a run: `on_step` after each input, `on_reply` per delivered reply (including its
/// generation sequence, which is distinct from delivery order under empirical latency),
/// `on_cmd` per command sent (`(account, slot, decided, qty)` for order ops).
pub trait Obs {
    fn on_reply(&mut self, _a: AccountId, _due: u64, _gen_seq: u64, _m: &RepMsg, _bulk: &[u8]) {}
    fn on_cmd(&mut self, _c: &GwCmd, _t: u64) {}
    fn on_step(&mut self, _ex: &SimExchange, _t: u64) {}
}

/// Drives a scenario; `drain_all` delivers every pending reply after each input.
pub fn run(sc: &Scenario, setup: &SimSetup, rd: &RefData, drain_all: bool, obs: &mut impl Obs) {
    let mut ex = match SimExchange::new(setup, rd) {
        Ok(x) => x,
        Err(e) => panic!("{e:?}"),
    };
    let mut slots = [0u32; 256];
    let mut snap_id = 0u64;
    let mut t = ms(10);
    let deliver =
        |ex: &mut SimExchange, now: u64, obs: &mut dyn FnMut(AccountId, u64, u64, &RepMsg, &[u8])| {
            ex.drain_due(Tsc(now), |r| {
                obs(r.account, r.due_tsc.0, r.gen_seq, &r.msg, r.bulk)
            });
        };
    deliver(
        &mut ex,
        if drain_all { u64::MAX } else { t },
        &mut |a, d, g, m, b| obs.on_reply(a, d, g, m, b),
    );
    for (gap, ev) in &sc.evs {
        t += ms(*gap) + 1;
        let ip = |k: usize| INSTS[k % sc.n_insts.max(1)];
        let mk = |inst: InstId, ex_ts: u64| mhdr(venue_of(inst), inst, ex_ts as i64);
        match ev {
            Ev::Bbo {
                inst,
                off,
                spread,
                qty,
            } => {
                let i = ip(*inst);
                let (b, tick, lot) = shape(i);
                let bid = b + off * tick;
                let x = Bbo {
                    h: mk(i, t),
                    bid_px: Px(bid),
                    bid_qty: Qty(qty * lot),
                    ask_px: Px(bid + spread * tick),
                    ask_qty: Qty(qty * lot),
                    update_id: 0,
                };
                ex.on_record(&rhdr(t), SimInput::Market(MarketEventRef::Bbo(&x)));
            }
            Ev::TopN { inst, off, n, qty } => {
                let i = ip(*inst);
                let (b, tick, lot) = shape(i);
                let mid = b + off * tick;
                let bids: Vec<Level> = (0..*n as i64)
                    .map(|k| Level {
                        px: Px(mid - (k + 1) * tick),
                        qty: Qty((qty + k) * lot),
                    })
                    .collect();
                let asks: Vec<Level> = (0..*n as i64)
                    .map(|k| Level {
                        px: Px(mid + k * tick),
                        qty: Qty((qty + k) * lot),
                    })
                    .collect();
                snap_id += 1;
                let hdr = L2SnapHdr {
                    h: mk(i, t),
                    snap_id,
                    origin: L2SnapHdr::TOP_N,
                    _pad: [0; 3],
                    n_bids: *n as u16,
                    n_asks: *n as u16,
                };
                let p = L2SnapshotPart {
                    hdr,
                    bids: &bids,
                    asks: &asks,
                    part: 0,
                    parts: 1,
                };
                ex.on_record(&rhdr(t), SimInput::Market(MarketEventRef::L2Snapshot(p)));
            }
            Ev::Batch { inst, off } => {
                let i = ip(*inst);
                let (b, tick, lot) = shape(i);
                let lv = [Level {
                    px: Px(b + off * tick + 50 * tick),
                    qty: Qty(lot),
                }];
                let hdr = L2BatchHdr {
                    h: mk(i, t),
                    prev_id: 0,
                    last_id: 1,
                    n_bids: 0,
                    n_asks: 1,
                    bflags: 0,
                    _pad: 0,
                };
                ex.on_record(
                    &rhdr(t),
                    SimInput::Market(MarketEventRef::L2Batch(L2BatchView { hdr, levels: &lv })),
                );
            }
            Ev::Trade { inst, off, qty } => {
                let i = ip(*inst);
                let (b, tick, lot) = shape(i);
                let x = Trade {
                    h: mk(i, t),
                    px: Px(b + off * tick),
                    qty: Qty(qty * lot),
                    trade_id: 1,
                    taker_side: Side::Buy,
                    _pad: [0; 7],
                };
                ex.on_record(&rhdr(t), SimInput::Market(MarketEventRef::Trade(&x)));
            }
            Ev::Funding { inst, rate, next_ms } => {
                let i = ip(*inst);
                let x = Funding {
                    h: mk(i, t),
                    rate: hft_types::Rate(*rate),
                    next_funding_ns: (t + ms(*next_ms)) as i64,
                    interval_s: 0,
                    _pad: 0,
                };
                ex.on_record(&rhdr(t), SimInput::Market(MarketEventRef::Funding(&x)));
            }
            Ev::Mark { inst, off } => {
                let i = ip(*inst);
                let (b, tick, _) = shape(i);
                let x = MarkPrice {
                    h: mk(i, t),
                    px: Px(b + off * tick),
                };
                ex.on_record(&rhdr(t), SimInput::Market(MarketEventRef::Mark(&x)));
            }
            Ev::Place {
                acct,
                inst,
                buy,
                lots,
                off,
                tif,
            } => {
                let a = ACCTS[*acct];
                let i = ip(*inst);
                let (b, tick, lot) = shape(i);
                let s = &mut slots[usize::from(a.0)];
                *s += 1;
                let c = GwCmd {
                    decision_seq: u64::from(*s),
                    decided_tsc: Tsc(t),
                    px: Px(b + off * tick),
                    qty: Qty(lots * lot * if i.0 == 11 || i.0 == 12 { 100 } else { 1 }),
                    inst: i,
                    slot: SlotId(*s),
                    r#gen: 1,
                    target_slot: SlotId(0),
                    op: GwOp::Place,
                    account: a,
                    side: if *buy { Side::Buy } else { Side::Sell },
                    tif: [Tif::Ioc, Tif::Gtc, Tif::Fok, Tif::PostOnly][usize::from(*tif)],
                    oflags: 0,
                    _pad: [0; 2],
                    arg: 0,
                    arg2: 0,
                };
                obs.on_cmd(&c, t);
                ex.on_command(&c, Tsc(t));
            }
            Ev::Cancel { acct, back } | Ev::Query { acct, back } | Ev::Amend { acct, back, .. } => {
                let a = ACCTS[*acct];
                let s = slots[usize::from(a.0)];
                let target = s.saturating_sub(*back);
                let (op, p, x) = match ev {
                    Ev::Cancel { .. } => (GwOp::Cancel, Px(0), Qty(0)),
                    Ev::Query { .. } => (GwOp::Query, Px(0), Qty(0)),
                    Ev::Amend { off, .. } => (GwOp::Amend, Px(600_000_000 + off * 100), Qty(5_000)),
                    _ => (GwOp::Query, Px(0), Qty(0)),
                };
                let c = GwCmd {
                    decision_seq: 0,
                    decided_tsc: Tsc(t),
                    px: p,
                    qty: x,
                    inst: BTC_S1,
                    slot: SlotId(target),
                    r#gen: 1,
                    target_slot: SlotId(target),
                    op,
                    account: a,
                    side: Side::Buy,
                    tif: Tif::Gtc,
                    oflags: 0,
                    _pad: [0; 2],
                    arg: 0,
                    arg2: 0,
                };
                obs.on_cmd(&c, t);
                ex.on_command(&c, Tsc(t));
            }
            Ev::CancelAll { acct, inst } => {
                let a = ACCTS[*acct];
                let mut c = GwCmd::sim_reset(a);
                c.op = GwOp::CancelAll;
                c.inst = inst.map_or(InstId(0), ip);
                c.decided_tsc = Tsc(t);
                obs.on_cmd(&c, t);
                ex.on_command(&c, Tsc(t));
            }
            Ev::Transfer { acct, amount } => {
                ex.on_record(&rhdr(t), SimInput::Clock);
                deliver(
                    &mut ex,
                    if drain_all { u64::MAX } else { t },
                    &mut |a, d, g, m, b| obs.on_reply(a, d, g, m, b),
                );
                let c = GwCmd::sim_transfer(ACCTS[*acct], S1, P2, USDT, Qty(amount * 1_000_000));
                obs.on_cmd(&c, t);
                ex.on_command(&c, Tsc(t));
            }
            Ev::Reset { acct } => {
                ex.on_record(&rhdr(t), SimInput::Clock);
                deliver(
                    &mut ex,
                    if drain_all { u64::MAX } else { t },
                    &mut |a, d, g, m, b| obs.on_reply(a, d, g, m, b),
                );
                let c = GwCmd::sim_reset(ACCTS[*acct]);
                obs.on_cmd(&c, t);
                ex.on_command(&c, Tsc(t));
            }
        }
        deliver(
            &mut ex,
            if drain_all { u64::MAX } else { t },
            &mut |a, d, g, m, b| obs.on_reply(a, d, g, m, b),
        );
        obs.on_step(&ex, t);
    }
    ex.advance(Tsc(t + ms(1_000)));
    deliver(&mut ex, u64::MAX, &mut |a, d, g, m, b| {
        obs.on_reply(a, d, g, m, b)
    });
    obs.on_step(&ex, t + ms(1_000));
}

/// Collects reply bytes and the state-hash trail.
#[derive(Default)]
pub struct Trace {
    pub replies: Vec<Vec<u8>>,
    pub hashes: Vec<u64>,
    /// Only replies of this account (None = all).
    pub only: Option<AccountId>,
}

impl Obs for Trace {
    fn on_reply(&mut self, a: AccountId, due: u64, _gen_seq: u64, m: &RepMsg, bulk: &[u8]) {
        if self.only.is_none_or(|o| o == a) {
            self.replies.push(reply_bytes(a, due, m, bulk));
        }
    }
    fn on_step(&mut self, ex: &SimExchange, _t: u64) {
        self.hashes.push(ex.state_hash());
    }
}
