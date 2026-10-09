//! SIM-124: money conservation, positions = Σ fills, reply timing — against a per-fill ledger model
//! rebuilt from the delivered replies and the commands.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

use std::collections::BTreeMap;

use common::scen::{Obs, cfg_gen, run, scenario, setup_for};
use common::*;
use hft_event::{GwCmd, GwOp, OrderEventKind, RepMsg};
use hft_refdata::RefData;
use hft_sim::SimExchange;
use hft_types::{AccountId, AssetId, InstId, Side, VenueId};
use proptest::prelude::*;

fn rd() -> &'static RefData {
    static R: std::sync::OnceLock<RefData> = std::sync::OnceLock::new();
    R.get_or_init(rd_sim)
}

fn base_of(i: InstId) -> AssetId {
    match i.0 {
        11 | 21 => BTC,
        12 => BNB,
        _ => ETH,
    }
}

#[derive(Default)]
struct Model {
    bal: BTreeMap<(u8, VenueId, AssetId), i128>,
    pos: BTreeMap<(u8, InstId), (i128, i128)>,
    transfers: Vec<(u64, u8, i128)>,
    decided: BTreeMap<(u8, u32), (u64, i64)>,
    last_ex: BTreeMap<(u8, u32), i64>,
    err: Option<String>,
    seen_accounts: Vec<u8>,
}

impl Model {
    fn init(&mut self, a: u8) {
        for b in cfg_gen().initial_balances.iter() {
            *self.bal.entry((a, b.venue, b.asset)).or_insert(0) += i128::from(b.qty.0);
        }
    }

    fn reset(&mut self, a: u8) {
        self.bal.retain(|k, _| k.0 != a);
        self.pos.retain(|k, _| k.0 != a);
        self.transfers.retain(|x| x.1 != a);
        self.init(a);
    }

    fn add(&mut self, a: u8, v: VenueId, asset: AssetId, d: i128) {
        *self.bal.entry((a, v, asset)).or_insert(0) += d;
    }

    fn fill(&mut self, a: u8, inst: InstId, side: Side, q: i128, p: i128, fee: i128, fee_asset: AssetId) {
        let v = venue_of(inst);
        let val = q * p; // exponent −12 → USDT −6: / 1e6
        if v == P2 {
            let e = self.pos.entry((a, inst)).or_insert((0, 0));
            let sgn = i128::from(side.sign());
            let mut left = q;
            let mut pnl = 0i128;
            if e.0 != 0 && e.0.signum() != sgn {
                let abs = e.0.abs();
                let c = left.min(abs);
                let num = e.1 * c;
                let (qq, r) = (num.div_euclid(abs), num.rem_euclid(abs));
                let removed = if 2 * r > abs || (2 * r == abs && qq % 2 != 0) {
                    qq + 1
                } else {
                    qq
                };
                let cp = c * p;
                pnl = if e.0 > 0 { cp - removed } else { removed - cp };
                e.1 -= removed;
                e.0 += c * sgn;
                left -= c;
                if e.0 == 0 {
                    e.1 = 0;
                }
            }
            if left > 0 {
                e.1 += left * p;
                e.0 += left * sgn;
            }
            self.add(a, P2, USDT, pnl.div_euclid(1_000_000));
        } else {
            let base = base_of(inst);
            match side {
                Side::Buy => {
                    self.add(a, v, USDT, -(val + 999_999).div_euclid(1_000_000));
                    self.add(a, v, base, q);
                }
                Side::Sell => {
                    self.add(a, v, USDT, val.div_euclid(1_000_000));
                    self.add(a, v, base, -q);
                }
            }
        }
        self.add(a, v, fee_asset, -fee);
    }

    fn fail(&mut self, s: String) {
        if self.err.is_none() {
            self.err = Some(s);
        }
    }
}

impl Obs for Model {
    fn on_cmd(&mut self, c: &GwCmd, t: u64) {
        let a = c.account.0;
        if !self.seen_accounts.contains(&a) {
            self.seen_accounts.push(a);
        }
        match c.op {
            GwOp::Place => {
                self.decided.insert((a, c.slot.0), (t, c.qty.0));
            }
            GwOp::SimTransfer => {
                let amt = i128::from(c.qty.0);
                self.add(a, S1, USDT, -amt);
                self.transfers.push((t + ms(300_000), a, amt));
            }
            GwOp::SimReset => self.reset(a),
            _ => {}
        }
    }

    fn on_reply(&mut self, acc: AccountId, due: u64, m: &RepMsg, _b: &[u8]) {
        let a = acc.0;
        match m {
            RepMsg::OrderEvent(e) => {
                let k = (a, e.slot.0);
                if let Some(&(dec, qty)) = self.decided.get(&k) {
                    if due < dec {
                        self.fail(format!("{k:?}: due {due} < decided {dec}"));
                    }
                    if e.cum_filled.0 > qty {
                        self.fail(format!("{k:?}: cum {} > qty {qty}", e.cum_filled.0));
                    }
                }
                let order_report = self.decided.contains_key(&k)
                    && !matches!(
                        e.what,
                        OrderEventKind::QueryResult
                            | OrderEventKind::CancelReject
                            | OrderEventKind::AmendReject
                    );
                let last = self.last_ex.get(&k).copied().unwrap_or(i64::MIN);
                if order_report && e.ex_ts_ns < last {
                    self.fail(format!("{k:?}: ex_ts went back"));
                }
                if order_report {
                    self.last_ex.insert(k, last.max(e.ex_ts_ns));
                }
                if matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill) {
                    let side = self.side_of(a, e.slot.0, e.inst);
                    self.fill(
                        a,
                        e.inst,
                        side,
                        i128::from(e.qty.0),
                        i128::from(e.px.0),
                        i128::from(e.fee.0),
                        e.fee_asset,
                    );
                }
            }
            RepMsg::ExternalFill(f) => {
                self.fill(
                    a,
                    f.inst,
                    f.side,
                    i128::from(f.qty.0),
                    i128::from(f.px.0),
                    i128::from(f.fee.0),
                    f.fee_asset,
                );
            }
            RepMsg::FundingSettle(f) => self.add(a, f.venue, USDT, i128::from(f.amount.0)),
            _ => {}
        }
    }

    fn on_step(&mut self, ex: &SimExchange, t: u64) {
        let land: Vec<_> = self.transfers.iter().filter(|x| x.0 <= t).copied().collect();
        self.transfers.retain(|x| x.0 > t);
        for (_, a, amt) in land {
            self.add(a, P2, USDT, amt);
        }
        for &a in &common::scen::ACCTS {
            let Some(v) = ex.account(a) else { continue };
            for venue in [S1, P2] {
                for asset in [USDT, BTC, ETH, BNB] {
                    let want = self.bal.get(&(a.0, venue, asset)).copied().unwrap_or(0);
                    let got = i128::from(v.balance(venue, asset).total.0);
                    if want != got {
                        self.fail(format!("t {t} {a:?} {venue:?} {asset:?}: model {want} sim {got}"));
                    }
                }
            }
            for inst in [BTC_P2, ETH_P2] {
                let want = self.pos.get(&(a.0, inst)).map_or(0, |p| p.0);
                let got = i128::from(v.position(inst).qty.0);
                if want != got {
                    self.fail(format!("t {t} {a:?} {inst:?}: model pos {want} sim {got}"));
                }
            }
        }
    }
}

impl Model {
    fn side_of(&self, _a: u8, _slot: u32, _inst: InstId) -> Side {
        Side::Buy
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]
    #[test]
    fn sim_124_invariants(sc in scenario(150), seed in any::<u64>()) {
        let setup = setup_for(&sc, seed);
        let mut m = SideModel::default();
        for a in &common::scen::ACCTS[..sc.n_accts] {
            m.model.init(a.0);
        }
        run(&sc, &setup, rd(), true, &mut m);
        prop_assert!(m.model.err.is_none(), "{:?}", m.model.err);
    }
}

/// `Model` plus the side of each order (from the commands).
#[derive(Default)]
struct SideModel {
    model: Model,
    sides: BTreeMap<(u8, u32), Side>,
}

impl Obs for SideModel {
    fn on_cmd(&mut self, c: &GwCmd, t: u64) {
        if c.op == GwOp::Place {
            self.sides.insert((c.account.0, c.slot.0), c.side);
        }
        self.model.on_cmd(c, t);
    }
    fn on_reply(&mut self, a: AccountId, due: u64, m: &RepMsg, b: &[u8]) {
        if let RepMsg::OrderEvent(e) = m
            && matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill)
        {
            let side = self.sides.get(&(a.0, e.slot.0)).copied().unwrap_or(Side::Buy);
            let mut e2 = *e;
            // Route the fill through the model with its real side.
            self.model.fill(
                a.0,
                e2.inst,
                side,
                i128::from(e2.qty.0),
                i128::from(e2.px.0),
                i128::from(e2.fee.0),
                e2.fee_asset,
            );
            e2.what = OrderEventKind::Ack;
            self.model.on_reply(a, due, &RepMsg::OrderEvent(e2), b);
            return;
        }
        self.model.on_reply(a, due, m, b);
    }
    fn on_step(&mut self, ex: &SimExchange, t: u64) {
        self.model.on_step(ex, t);
    }
}
