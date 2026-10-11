//! SIM-114, SIM-120–SIM-122: standard reply types, determinism, seeds, sub-stream isolation.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

use common::scen::{Obs, Trace, run, scenario, setup_for};
use common::*;
use hft_event::{RecordFlags, RepMsg, encode_fixed, encode_var};
use hft_refdata::RefData;
use hft_sim::{Dist, LatencyModel};
use hft_types::{AccountId, Tif};
use proptest::prelude::*;

fn rd() -> &'static RefData {
    static R: std::sync::OnceLock<RefData> = std::sync::OnceLock::new();
    R.get_or_init(rd_sim)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    #[test]
    fn sim_120_same_seed_same_trace(sc in scenario(400), seed in any::<u64>()) {
        let setup = setup_for(&sc, seed);
        let mut a = Trace::default();
        let mut b = Trace::default();
        run(&sc, &setup, rd(), false, &mut a);
        run(&sc, &setup, rd(), false, &mut b);
        prop_assert_eq!(a.replies, b.replies);
        prop_assert_eq!(a.hashes, b.hashes);
    }

    #[test]
    fn sim_121_seed_only_affects_sampling(mut sc in scenario(400), s1 in any::<u64>(), s2 in any::<u64>()) {
        // Fixed latency: identical replies whatever the seed.
        sc.empirical = false;
        let mut a = Trace::default();
        let mut b = Trace::default();
        run(&sc, &setup_for(&sc, s1), rd(), false, &mut a);
        run(&sc, &setup_for(&sc, s2), rd(), false, &mut b);
        prop_assert_eq!(a.replies, b.replies);
        // Empirical: per-order fill totals stay within the order quantity.
        sc.empirical = true;
        let mut c = Cum::default();
        run(&sc, &setup_for(&sc, s2), rd(), false, &mut c);
        prop_assert!(c.ok, "{}", c.why);
    }

    #[test]
    fn sim_114_replies_are_standard_events(sc in scenario(400), seed in any::<u64>()) {
        let mut e = Enc::default();
        run(&sc, &setup_for(&sc, seed), rd(), false, &mut e);
        prop_assert!(e.ok, "{}", e.why);
    }
}

/// Σ fills of each order equals its last `cum_filled` and never exceeds the order quantity.
#[derive(Default)]
struct Cum {
    qty: std::collections::BTreeMap<(u8, u32), i64>,
    sum: std::collections::BTreeMap<(u8, u32), i64>,
    ok_init: bool,
    ok: bool,
    why: String,
}

impl Obs for Cum {
    fn on_cmd(&mut self, c: &hft_event::GwCmd, _t: u64) {
        if !self.ok_init {
            self.ok = true;
            self.ok_init = true;
        }
        if c.op == hft_event::GwOp::Place {
            self.qty.insert((c.account.0, c.slot.0), c.qty.0);
            self.sum.insert((c.account.0, c.slot.0), 0);
        }
    }
    fn on_reply(&mut self, a: AccountId, _d: u64, _gen_seq: u64, m: &RepMsg, _b: &[u8]) {
        if !self.ok_init {
            self.ok = true;
            self.ok_init = true;
        }
        if let RepMsg::OrderEvent(e) = m
            && matches!(
                e.what,
                hft_event::OrderEventKind::PartialFill | hft_event::OrderEventKind::Fill
            )
        {
            let k = (a.0, e.slot.0);
            let s = self.sum.entry(k).or_insert(0);
            *s += e.qty.0;
            let q = self.qty.get(&k).copied().unwrap_or(i64::MAX);
            if *s != e.cum_filled.0 || *s > q {
                self.ok = false;
                self.why = format!("order {k:?}: Σ fills {s} cum {} qty {q}", e.cum_filled.0);
            }
        }
    }
}

/// Every reply encodes with the hft-event encoders.
#[derive(Default)]
struct Enc {
    ok: bool,
    seen: bool,
    why: String,
}

impl Obs for Enc {
    fn on_reply(&mut self, _a: AccountId, _d: u64, _gen_seq: u64, m: &RepMsg, bulk: &[u8]) {
        if !self.seen {
            self.ok = true;
            self.seen = true;
        }
        let mut buf = vec![0u8; 70_000];
        let f = RecordFlags(0);
        let n = match m {
            RepMsg::OrderEvent(x) => Ok(encode_fixed(&mut buf, 1, 1, f, x)),
            RepMsg::SendReport(x) => Ok(encode_fixed(&mut buf, 1, 1, f, x)),
            RepMsg::PositionReport(x) => Ok(encode_fixed(&mut buf, 1, 1, f, x)),
            RepMsg::BalanceReport(x) => Ok(encode_fixed(&mut buf, 1, 1, f, x)),
            RepMsg::FundingSettle(x) => Ok(encode_fixed(&mut buf, 1, 1, f, x)),
            RepMsg::ConnState(x) => Ok(encode_fixed(&mut buf, 1, 1, f, x)),
            RepMsg::ExternalFill(x) => Ok(encode_fixed(&mut buf, 1, 1, f, x)),
            RepMsg::AccountConfigReport(x) => encode_var(&mut buf, 1, 1, f, x, bulk),
            RepMsg::MarginReport(x) => encode_var(&mut buf, 1, 1, f, x, bulk),
        };
        if !matches!(n, Ok(k) if k > 0) {
            self.ok = false;
            self.why = format!("{m:?} does not encode: {n:?}");
        }
    }
}

fn run_122(with_other: bool) -> Vec<Vec<u8>> {
    let mut setup = setup_sim();
    let d = Dist::from_points(&[(0, 1_000), (10_000, 5_000)]);
    setup.latency_models = vec![LatencyModel::Empirical { entry: d, resp: d }];
    if with_other {
        setup.accounts.insert(0, (AccountId(3), cfg_a8()));
    }
    let mut s = Sim::with(&setup, &rd_sim());
    s.topn(ms(100), BTC_S1, &[("60000", "10"), ("60010", "10")], &[]);
    for k in 0..20u64 {
        let t = ms(100 + 10 * k);
        if with_other {
            for j in 0..3u64 {
                s.place(AccountId(3), BTC_S1, Buy, "0.001", "60010", Tif::Ioc, t + j);
            }
        }
        s.place(A9, BTC_S1, Buy, "0.01", "60010", Tif::Ioc, t);
        s.clock(t + ms(5));
    }
    s.run_to(ms(1_000));
    s.out
        .iter()
        .filter(|r| r.account == A9)
        .map(|r| common::scen::reply_bytes(r.account, r.due, &r.msg, &r.bulk))
        .collect()
}

#[test]
fn sim_122_new_account_no_perturbation() {
    let a = run_122(false);
    let b = run_122(true);
    assert!(!a.is_empty());
    assert_eq!(a, b);
}
