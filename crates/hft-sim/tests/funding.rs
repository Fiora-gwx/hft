//! SIM-060–SIM-063: funding settlement time, amount rounding, push forms, settlement time updates.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod common;

use common::*;
use hft_event::{FundingSettle, RepMsg};
use hft_refdata::FundingSettleKind;
use hft_types::{InstId, Notional, Qty, Rate};

/// `T` = 8 h.
const T: u64 = 8 * 3_600 * 1_000_000_000;
const H1: u64 = 3_600 * 1_000_000_000;
const S: u64 = 1_000_000_000;

fn settles(s: &Sim) -> Vec<FundingSettle> {
    s.out
        .iter()
        .filter_map(|r| match r.msg {
            RepMsg::FundingSettle(f) => Some(f),
            _ => None,
        })
        .collect()
}

fn base060(kind: FundingSettleKind, positions: &[(InstId, &str, &str)]) -> Sim {
    let rd = rd_with(|s| s.caps_mut(P2).funding_settle = kind);
    let mut s = Sim::with(&setup_sim(), &rd);
    s.seed(A9, &seed9_state("18000", positions));
    s
}

#[test]
fn sim_060_settle_timing() {
    let rows: [(&str, u64, Option<&str>); 3] = [
        ("SIM-060.1", T - 1, None),
        ("SIM-060.2", T, Some("18006")),
        ("SIM-060.3", T + S, Some("18006")),
    ];
    for (row, ex, want) in rows {
        let mut s = base060(FundingSettleKind::PerSymbolPush, &[(BTC_P2, "-1.0", "60000")]);
        s.funding_ex(T - H1, (T - H1) as i64, BTC_P2, "0.0001", T as i64);
        s.mark_ex(T - S, (T - S) as i64, BTC_P2, px("60000"));
        s.mark_ex(ex, ex as i64, BTC_P2, px("61000"));
        if row == "SIM-060.3" {
            s.mark_ex(ex + S, (ex + S) as i64, BTC_P2, px("61000"));
        }
        s.run_to(ex + 2 * S);
        let fs = settles(&s);
        match want {
            None => {
                assert!(fs.is_empty(), "{row}");
                assert_eq!(s.total(A9, P2, USDT), usdt("18000").0, "{row}");
            }
            Some(w) => {
                assert_eq!(fs.len(), 1, "{row}");
                let f = fs[0];
                assert_eq!(
                    (f.inst, f.amount, f.rate, f.pos_qty, f.settle_ts_ns, f.synth),
                    (BTC_P2, notional("6.0"), rate("0.0001"), q("-1.0"), T as i64, 0),
                    "{row}"
                );
                assert_eq!(s.total(A9, P2, USDT), usdt(w).0, "{row}");
            }
        }
    }
}

#[test]
fn sim_061_funding_amount_rounding() {
    let rows: [(&str, &str, Option<i64>); 3] = [
        ("SIM-061.1", "-0.001", Some(7_407)),
        ("SIM-061.2", "0.001", Some(-7_408)),
        ("SIM-061.3", "0", None),
    ];
    for (row, pos, want) in rows {
        let positions: Vec<(InstId, &str, &str)> = if pos == "0" {
            vec![]
        } else {
            vec![(BTC_P2, pos, "60000")]
        };
        let mut s = base060(FundingSettleKind::PerSymbolPush, &positions);
        s.funding_ex(T - H1, (T - H1) as i64, BTC_P2, "0.000123457", T as i64);
        s.mark_ex(T - S, (T - S) as i64, BTC_P2, px("60000.0001"));
        s.mark_ex(T, T as i64, BTC_P2, px("60000.0001"));
        s.run_to(T + S);
        let fs = settles(&s);
        match want {
            None => assert!(fs.is_empty(), "{row}"),
            Some(raw) => {
                assert_eq!(fs.len(), 1, "{row}");
                assert_eq!(fs[0].amount, Notional(raw), "{row}");
                assert_eq!(s.total(A9, P2, USDT), usdt("18000").0 + raw, "{row}");
            }
        }
    }
}

#[test]
fn sim_062_push_forms() {
    let rows = [
        ("SIM-062.1", FundingSettleKind::PerSymbolPush),
        ("SIM-062.2", FundingSettleKind::BalanceOnlyPush),
        ("SIM-062.3", FundingSettleKind::RateOnly),
    ];
    for (row, kind) in rows {
        let mut s = base060(kind, &[(BTC_P2, "-1.0", "60000"), (ETH_P2, "-10.0", "3000")]);
        s.funding_ex(T - H1, (T - H1) as i64, BTC_P2, "0.0001", T as i64);
        s.funding_ex(T - H1, (T - H1) as i64, ETH_P2, "0.0001", T as i64);
        s.mark_ex(T - S, (T - S) as i64, BTC_P2, px("60000"));
        s.mark_ex(T - S, (T - S) as i64, ETH_P2, px("3000"));
        s.mark_ex(T, T as i64, BTC_P2, px("60000"));
        s.run_to(T + S);
        let fs: Vec<_> = settles(&s)
            .into_iter()
            .map(|f| (f.inst, f.amount, f.rate, f.pos_qty))
            .collect();
        let want = match kind {
            FundingSettleKind::PerSymbolPush => vec![
                (BTC_P2, notional("6.0"), rate("0.0001"), q("-1.0")),
                (ETH_P2, notional("3.0"), rate("0.0001"), q("-10.0")),
            ],
            FundingSettleKind::BalanceOnlyPush => vec![(InstId(0), notional("9.0"), Rate(0), Qty(0))],
            _ => vec![],
        };
        assert_eq!(fs, want, "{row}");
        assert_eq!(s.total(A9, P2, USDT), usdt("18009").0, "{row}");
    }
}

#[test]
fn sim_063_settle_time_follows_funding() {
    let mut s = base060(FundingSettleKind::PerSymbolPush, &[(BTC_P2, "-1.0", "60000")]);
    s.funding_ex(T - H1, (T - H1) as i64, BTC_P2, "0.0001", T as i64);
    s.funding_ex(
        T - H1 / 2,
        (T - H1 / 2) as i64,
        BTC_P2,
        "0.0002",
        (T + 4 * H1) as i64,
    );
    s.mark_ex(T + S, (T + S) as i64, BTC_P2, px("60000"));
    s.run_to(T + 2 * S);
    assert!(settles(&s).is_empty(), "no settlement at T");
    s.mark_ex(T + 4 * H1, (T + 4 * H1) as i64, BTC_P2, px("60000"));
    s.run_to(T + 4 * H1 + S);
    let fs = settles(&s);
    assert_eq!(fs.len(), 1);
    assert_eq!(fs[0].amount, notional("12.0"));
    assert_eq!(s.total(A9, P2, USDT), usdt("18012").0);
}
