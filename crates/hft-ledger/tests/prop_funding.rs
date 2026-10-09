//! LED-115, LED-244: settle-time normalization at every entry; per-settlement funding converges
//! to the exchange amounts under lost, duplicated and reordered pushes and repeated reports.

#![allow(clippy::disallowed_types)] // allow-list: test code, not a hot path

mod common;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use common::opgen::cases;
use common::*;
use hft_ledger::{FundingPush, FundingRecon, FundingReconItem, Ledger};
use hft_refdata::{FundingSettleKind, RefData};
use hft_types::{INST_NONE, InstId, Notional, funding_settle_norm_ns};
use proptest::prelude::*;

/// `rd_lf` with `P2` funding `kind` and anchor `anchor` (built once each).
fn rd_kind_anchor(kind: FundingSettleKind, anchor: u32) -> &'static RefData {
    static ALL: OnceLock<Vec<(FundingSettleKind, u32, &'static RefData)>> = OnceLock::new();
    let all = ALL.get_or_init(|| {
        let mut v = Vec::new();
        for k in [
            FundingSettleKind::PerSymbolPush,
            FundingSettleKind::BalanceOnlyPush,
            FundingSettleKind::RateOnly,
        ] {
            for a in [0u32, 1_800] {
                v.push((
                    k,
                    a,
                    rd_with(&RdOpts {
                        p2_funding: k,
                        p2_anchor_s: a,
                        ..RdOpts::default()
                    }),
                ));
            }
        }
        v
    });
    all.iter()
        .find(|x| x.0 == kind && x.1 == anchor)
        .map(|x| x.2)
        .expect("combo")
}

fn ledger(kind: FundingSettleKind, anchor: u32, interval: u32) -> Ledger {
    let mut l = ledger_with(rd_kind_anchor(kind, anchor), &cfg_lf());
    boot_a1(&mut l, 0);
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    mark(&mut l, BTC_P2, "60000", s(1));
    for i in [BTC_P2, ETH_P2] {
        l.on_funding_interval(i, interval);
    }
    l
}

const DAY: i64 = 86_400_000_000_000;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases(2_000)))]

    #[test]
    fn led_115_normalization_idempotent(
        interval in prop::sample::select(vec![3_600u32, 14_400, 28_800]),
        anchor in prop::sample::select(vec![0u32, 1_800]),
        tss in prop::collection::vec(D - 30 * DAY..=D + 30 * DAY, 1..=8),
    ) {
        let mut ps = ledger(FundingSettleKind::PerSymbolPush, anchor, interval);
        let mut bp = ledger(FundingSettleKind::BalanceOnlyPush, anchor, interval);
        prop_assert_eq!(ps.funding_grid(P2, BTC_P2), (interval, anchor));
        prop_assert_eq!(bp.funding_grid(P2, INST_NONE), (interval, anchor));
        for (k, &ts) in tss.iter().enumerate() {
            let norm = funding_settle_norm_ns(ts, interval, anchor);
            let t = s(10 + k as i64);
            // apply_funding (push).
            let n0 = ps.fund_hist(A1, P2).len();
            if ps.apply_funding(&push(BTC_P2, "1.0", ts), t).is_some() {
                prop_assert!(ps.fund_hist(A1, P2).iter().any(|h| h.inst == BTC_P2 && h.settle_ts_ns == norm));
                prop_assert_eq!(ps.fund_hist(A1, P2).len(), n0 + 1);
            }
            prop_assert!(ps.apply_funding(&push(BTC_P2, "1.0", norm), t).is_none(), "normalized value re-fed");
            // synth_funding.
            let x = ps.synth_funding(A1, BTC_P2, px("60000"), rate("0.0001"), ts).expect("synth");
            prop_assert_eq!(x.settle_ts_ns, norm);
            let y = ps.synth_funding(A1, BTC_P2, px("60000"), rate("0.0001"), norm).expect("synth");
            prop_assert_eq!(y.settle_ts_ns, norm);
            // reconcile_funding: FundHist after the watermark, FundNoHist before it.
            let it = FundingReconItem {
                account: A1,
                venue: P2,
                inst: ETH_P2,
                settle_ts_ns: ts,
                exch_amount: n("2.0"),
                engine_amount: Notional(0),
                report_id: k as u64,
            };
            ps.reconcile_funding(&it, norm + H, t);
            if norm < D {
                prop_assert!(ps.fund_no_hist(A1, P2).iter().any(|x| x.0 == ETH_P2 && x.1 == norm));
            } else {
                prop_assert!(ps.fund_hist(A1, P2).iter().any(|h| h.inst == ETH_P2 && h.settle_ts_ns == norm));
            }
            let again = FundingReconItem { settle_ts_ns: norm, ..it };
            let r = ps.reconcile_funding(&again, norm + H, t);
            prop_assert!(matches!(r, FundingRecon::Matched | FundingRecon::NoHistoryHeld), "{:?}", r);
            // apply_funding_push (BalanceOnlyPush).
            let mut p = push(INST_NONE, "0", ts);
            p.amount = Notional(1_000_000 + k as i64);
            if matches!(bp.apply_funding_push(&p, t), FundingPush::Booked { .. }) {
                prop_assert_eq!(bp.fund_seen(A1, P2).last().copied(), Some((norm, p.amount)));
            }
            p.settle_ts_ns = norm;
            prop_assert_eq!(bp.apply_funding_push(&p, t), FundingPush::Duplicate);
        }
    }
}

/// One settlement event of LED-244.
#[derive(Clone, Debug)]
enum Ev {
    /// Push or synthesis of (inst slot `i`, settle slot `k`), `noise` added to the amount.
    Book {
        i: usize,
        k: usize,
        noise: i64,
    },
    /// Wallet push of settle slot `k` (BalanceOnlyPush).
    Wallet {
        k: usize,
        noise: i64,
    },
    Recon {
        i: usize,
        k: usize,
        late: bool,
    },
}

fn ev(n_inst: usize, n_k: usize) -> impl Strategy<Value = Ev> {
    prop_oneof![
        3 => (0..n_inst, 0..n_k, prop_oneof![3 => Just(0i64), 1 => -50_000i64..=50_000])
            .prop_map(|(i, k, noise)| Ev::Book { i, k, noise }),
        2 => (0..n_k, prop_oneof![3 => Just(0i64), 1 => -50_000i64..=50_000]).prop_map(|(k, noise)| Ev::Wallet { k, noise }),
        3 => (0..n_inst, 0..n_k, any::<bool>()).prop_map(|(i, k, late)| Ev::Recon { i, k, late }),
    ]
}

fn scenario() -> impl Strategy<Value = (usize, usize, usize, Vec<i64>, Vec<Ev>)> {
    (0usize..3, 1usize..=2, 1usize..=20).prop_flat_map(|(kind, n_inst, n_k)| {
        (
            Just(kind),
            Just(n_inst),
            Just(n_k),
            prop::collection::vec(-20_000_000i64..=20_000_000, n_inst * n_k),
            prop::collection::vec(ev(n_inst, n_k), 0..=120),
        )
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases(2_000)))]

    #[test]
    fn led_244_funding_converges_to_exchange((kind_ix, n_inst, n_k, exch, evs) in scenario()) {
        let kind = [FundingSettleKind::PerSymbolPush, FundingSettleKind::BalanceOnlyPush, FundingSettleKind::RateOnly][kind_ix];
        let mut l = ledger(kind, 0, 28_800);
        let insts = [BTC_P2, ETH_P2];
        let ts = |k: usize| D + (k as i64 + 1) * 8 * H;
        let exch_of = |i: usize, k: usize| exch[i * n_k + k];
        let wallet_exch = |k: usize| (0..n_inst).map(|i| exch_of(i, k)).sum::<i64>();
        let bop = kind == FundingSettleKind::BalanceOnlyPush;
        // (inst, settle) → exchange amount of the item the reconciler reports.
        let mut items: BTreeMap<(u32, i64), i64> = BTreeMap::new();
        for k in 0..n_k {
            if bop {
                items.insert((INST_NONE.0, ts(k)), wallet_exch(k));
            } else {
                for (i, inst) in insts.iter().enumerate().take(n_inst) {
                    items.insert((inst.0, ts(k)), exch_of(i, k));
                }
            }
        }
        // Report clock: the reconciler's report times are monotonic (journal order).
        let mut clock = 0i64;
        let mut recon = |l: &mut Ledger, inst: InstId, settle: i64, amount: i64, late: bool, t: i64| {
            let it = FundingReconItem {
                account: A1,
                venue: P2,
                inst,
                settle_ts_ns: settle,
                exch_amount: Notional(amount),
                engine_amount: Notional(0),
                report_id: 1,
            };
            clock = clock.max(if late { settle + H } else { settle + 60_000_000_000 });
            l.reconcile_funding(&it, clock, t)
        };
        let mut t = s(10);
        for e in &evs {
            t += s(1);
            match *e {
                Ev::Book { i, k, noise } => {
                    let synth = u8::from(kind != FundingSettleKind::PerSymbolPush);
                    let mut x = fs(insts[i], "0", ts(k), synth);
                    x.amount = Notional(exch_of(i, k) + noise);
                    l.apply_funding(&x, t);
                }
                Ev::Wallet { k, noise } => {
                    if bop {
                        let mut x = push(INST_NONE, "0", ts(k));
                        x.amount = Notional(wallet_exch(k) + noise);
                        l.apply_funding_push(&x, t);
                    }
                }
                Ev::Recon { i, k, late } => {
                    let (inst, amount) = if bop { (INST_NONE, wallet_exch(k)) } else { (insts[i], exch_of(i, k)) };
                    recon(&mut l, inst, ts(k), amount, late, t);
                }
            }
        }
        // Final reports in settle order (past the lag), twice: every item still in the window
        // ends within dust of the exchange amount, and the repeat changes nothing.
        let dust = 10_000i64;
        let mut order: Vec<((u32, i64), i64)> = items.iter().map(|(k, v)| (*k, *v)).collect();
        order.sort_by_key(|((inst, settle), _)| (*settle, *inst));
        let mut in_window = BTreeMap::new();
        for round in 0..2 {
            for &((inst, settle), amount) in &order {
                t += s(1);
                let r = recon(&mut l, InstId(inst), settle, amount, true, t);
                let inside = matches!(r, FundingRecon::Matched | FundingRecon::Corrected { .. });
                if round == 0 {
                    in_window.insert((inst, settle), inside);
                } else if inside && in_window.get(&(inst, settle)) == Some(&true) {
                    prop_assert_eq!(r, FundingRecon::Matched, "repeat of ({}, {})", inst, settle);
                }
                if inside {
                    let rec = l.fund_hist(A1, P2).into_iter().find(|h| h.inst.0 == inst && h.settle_ts_ns == settle);
                    let got = rec.map_or(0, |h| h.booked.0 + h.corrected.0);
                    prop_assert!((got - amount).abs() < dust, "({}, {}): booked+corrected {} vs exchange {}", inst, settle, got, amount);
                }
            }
        }
    }
}
