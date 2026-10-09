//! LED-157, LED-159: margin derived values equal a full recomputation; `restore` +
//! `restore_marks` reproduce a continuous run.

// The reference walks positions, marks and liquidation prices in parallel by index.
#![allow(clippy::needless_range_loop)]

mod common;

use common::*;
use hft_ledger::{FixedVec, Ledger, MarginState};
use hft_types::{InstId, Notional, Px, Rate};
use proptest::prelude::*;

const INSTS: [InstId; 2] = [BTC_P2, ETH_P2];
const BASE_PX: [i64; 2] = [600_000_000, 30_000_000];

#[derive(Clone, Debug)]
enum M {
    /// Mark at `pct`% of the base price (60–140).
    Mark { i: usize, pct: i64 },
    /// `liq`: 0 = not given, 1 = crossed, 2 = normal; `maint` in USDT.
    Report { maint: i64, liq: [u8; 2] },
    /// Fill of `lots` × 0.1 at the current mark.
    Fill { i: usize, buy: bool, lots: i64 },
}

fn m_op() -> impl Strategy<Value = M> {
    prop_oneof![
        8 => (0usize..2, 60i64..=140).prop_map(|(i, pct)| M::Mark { i, pct }),
        1 => (1i64..=2_000, [0u8..3, 0u8..3]).prop_map(|(maint, liq)| M::Report { maint, liq }),
        3 => (0usize..2, any::<bool>(), 1i64..=20).prop_map(|(i, buy, lots)| M::Fill { i, buy, lots }),
    ]
}

/// Test-side state the reference needs: marks with their times and the last report's inputs.
#[derive(Clone)]
struct World {
    l: Ledger,
    marks: [(i64, i64); 2],
    has_report: bool,
    maint: i64,
    rep_n: i64,
    liq: [i64; 2],
    t: i64,
}

impl World {
    fn new() -> World {
        let mut l = booted();
        let mut touched = FixedVec::new();
        l.on_mark(BTC_P2, Px(BASE_PX[0]), s(1), &mut touched);
        l.on_mark(ETH_P2, Px(BASE_PX[1]), s(1), &mut touched);
        World {
            l,
            marks: [(BASE_PX[0], s(1)), (BASE_PX[1], s(1))],
            has_report: false,
            maint: 0,
            rep_n: 0,
            liq: [0, 0],
            t: s(1),
        }
    }

    fn notional(&self) -> i64 {
        (0..2)
            .map(|k| {
                (i128::from(self.l.pos(A1, INSTS[k]).qty.0).abs() * i128::from(self.marks[k].0) / 1_000_000)
                    as i64
            })
            .sum()
    }

    fn step(&mut self, op: &M) {
        self.t += s(1);
        let t = self.t;
        match *op {
            M::Mark { i, pct } => {
                let px = BASE_PX[i] * pct / 100 / 100 * 100;
                let mut touched = FixedVec::new();
                self.l.on_mark(INSTS[i], Px(px), t, &mut touched);
                self.marks[i] = (px, t);
            }
            M::Report { maint, liq } => {
                let mut items = Vec::new();
                for k in 0..2 {
                    let q = self.l.pos(A1, INSTS[k]).qty.0;
                    let m = self.marks[k].0;
                    let long = q >= 0;
                    let l = match liq[k] {
                        0 => 0,
                        1 => {
                            if long {
                                m + 100
                            } else {
                                m - 100
                            }
                        }
                        _ => {
                            if long {
                                m * 7 / 10 / 100 * 100
                            } else {
                                m * 13 / 10 / 100 * 100
                            }
                        }
                    };
                    self.liq[k] = l;
                    items.push((INSTS[k], Px(l), 1));
                }
                let bytes = margin_items(&items);
                let mut h = margin_hdr(A1, P2, "50000", "0", "40000", "0.01", 2);
                h.maint_margin = Notional(maint * 1_000_000);
                self.l.on_margin(
                    &hft_ledger::MarginReportView {
                        hdr: h,
                        items: &bytes,
                    },
                    t,
                );
                self.has_report = true;
                self.maint = maint * 1_000_000;
                self.rep_n = self.notional();
            }
            M::Fill { i, buy, lots } => {
                let inst = INSTS[i];
                let (g, leg) = if i == 0 { (G1, 1) } else { (G3, 0) };
                let mut f = f(if buy { Buy } else { Sell }, inst, "0.1", "1", "0", USDT, g, leg);
                f.qty = hft_types::Qty(lots * 10_000_000);
                f.net_qty = f.qty;
                f.px = Px(self.marks[i].0);
                self.l.apply_fill(&f, t);
            }
        }
    }
}

fn half_even(a: i128, b: i128) -> i128 {
    let (q, r) = (a.div_euclid(b), a.rem_euclid(b));
    match (2 * r).cmp(&b) {
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Equal => q + (q & 1),
        std::cmp::Ordering::Less => q,
    }
}

/// Full recomputation of the derived margin values (README "保证金数据").
fn reference(w: &World) -> (i64, i64, Rate, Rate, Rate, i64) {
    let one = 1_000_000_000i128;
    let (mut upnl, mut big_n, mut oldest, mut n_open) = (0i128, 0i128, i64::MAX, 0);
    let mut rep: Option<i128> = None;
    for k in 0..2 {
        let p = w.l.pos(A1, INSTS[k]);
        if p.qty.0 == 0 {
            continue;
        }
        n_open += 1;
        let (m, ens) = w.marks[k];
        let value = i128::from(p.qty.0) * i128::from(m);
        upnl += (if p.qty.0 > 0 {
            value - p.cost
        } else {
            value + p.cost
        }) / 1_000_000;
        big_n += i128::from(p.qty.0).abs() * i128::from(m) / 1_000_000;
        oldest = oldest.min(ens);
        let l = i128::from(w.liq[k]);
        if !w.has_report || l == 0 {
            continue;
        }
        let num = if p.qty.0 > 0 {
            i128::from(m) - l
        } else {
            l - i128::from(m)
        };
        let d = if num <= 0 {
            0
        } else {
            half_even(num * one, i128::from(m))
        };
        rep = Some(rep.map_or(d, |x| x.min(d)));
    }
    let wallet = i128::from(w.l.margin(A1, P2).wallet.0);
    let mb = wallet + upnl;
    let rep_d = rep.map_or(Rate::MAX, |v| Rate(v as i64));
    if !w.has_report {
        return (upnl as i64, mb as i64, Rate(0), rep_d, Rate::MAX, oldest);
    }
    let maint = i128::from(w.maint);
    let rn = i128::from(w.rep_n);
    let (m_num, m_den) = if rn > 0 && big_n > rn {
        (maint * big_n, rn)
    } else {
        (maint, 1)
    };
    let ratio = if mb <= 0 {
        Rate::MAX
    } else {
        Rate(half_even(m_num * one, m_den * mb) as i64)
    };
    let est = if n_open == 0 || big_n == 0 {
        Rate::MAX
    } else {
        let num = mb * m_den - m_num;
        let den = big_n * m_den + m_num;
        if num <= 0 {
            Rate(0)
        } else {
            Rate(half_even(num * one, den) as i64)
        }
    };
    (upnl as i64, mb as i64, ratio, rep_d, est, oldest)
}

fn derived(ms: &MarginState) -> (i64, i64, Rate, Rate, Rate, i64) {
    (
        ms.upnl.0,
        ms.est_margin_balance.0,
        ms.est_ratio,
        ms.rep_liq_dist,
        ms.est_liq_dist,
        ms.mark_oldest_ens,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(common::opgen::cases(2_000)))]

    #[test]
    fn led_157_incremental_equals_full(ops in prop::collection::vec(m_op(), 0..=260)) {
        let mut w = World::new();
        for (k, op) in ops.iter().enumerate() {
            w.step(op);
            prop_assert_eq!(derived(w.l.margin(A1, P2)), reference(&w), "step {} {:?}", k, op);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(common::opgen::cases(1_000)))]

    #[test]
    fn led_159_restore_marks_matches_continuous(
        ops in prop::collection::vec(m_op(), 0..=260),
        points in prop::collection::btree_set(0usize..=260, 1..=3),
    ) {
        let mut w = World::new();
        let mut copies: Vec<World> = Vec::new();
        for (k, op) in ops.iter().enumerate() {
            if points.contains(&k) {
                let mut c = w.clone();
                let mut r = restore(&snap(&w.l)).expect("restore");
                let marks: Vec<(InstId, Px, i64)> = (0..2).map(|i| (INSTS[i], Px(w.marks[i].0), w.marks[i].1)).collect();
                r.restore_marks(&marks);
                prop_assert_eq!(derived(r.margin(A1, P2)), derived(w.l.margin(A1, P2)), "restore at {}", k);
                for i in INSTS {
                    prop_assert_eq!(r.perp_view(A1, i), w.l.perp_view(A1, i), "upnl cache {:?} at {}", i, k);
                }
                prop_assert_eq!(r.group_pos(A1, G1), w.l.group_pos(A1, G1), "group view at {}", k);
                c.l = r;
                copies.push(c);
            }
            w.step(op);
            for c in copies.iter_mut() {
                c.step(op);
                prop_assert_eq!(snap(&c.l), snap(&w.l), "step {} {:?}", k, op);
                prop_assert_eq!(derived(c.l.margin(A1, P2)), derived(w.l.margin(A1, P2)), "step {}", k);
            }
        }
    }
}
