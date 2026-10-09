//! SIM-070–SIM-076: cross margin numbers, liquidation price, liquidation, MarginReport.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod common;

use common::*;
use hft_event::{ExternalCause, ExternalFill, MarginItem, MarginReportHdr, RepMsg};
use hft_refdata::MarginData;
use hft_sim::{MmrSource, SimSetup};
use hft_types::{InstId, Px, Rate, Side};
use zerocopy::FromBytes;

const S: u64 = 1_000_000_000;

fn ext_fills(s: &Sim) -> Vec<ExternalFill> {
    s.out
        .iter()
        .filter_map(|r| match r.msg {
            RepMsg::ExternalFill(f) => Some(f),
            _ => None,
        })
        .collect()
}

fn margin_reports(s: &Sim) -> Vec<(MarginReportHdr, Vec<MarginItem>)> {
    s.out
        .iter()
        .filter_map(|r| match r.msg {
            RepMsg::MarginReport(h) => {
                let items = r
                    .bulk
                    .chunks_exact(16)
                    .filter_map(|c| MarginItem::read_from_bytes(c).ok())
                    .collect();
                Some((h, items))
            }
            _ => None,
        })
        .collect()
}

/// Seeded `A9` with `wallet` and `positions`, marks given, at t = 0.
fn seeded(
    setup: &SimSetup,
    rd: &hft_refdata::RefData,
    wallet: &str,
    positions: &[(InstId, &str, &str)],
    marks: &[(InstId, &str)],
) -> Sim {
    let mut s = Sim::with(setup, rd);
    s.seed(A9, &seed9_state(wallet, positions));
    for (i, m) in marks {
        s.mark(0, *i, m);
    }
    s
}

#[test]
fn sim_070_margin_numbers() {
    let s = seeded(
        &setup_sim(),
        &rd_sim(),
        "18000",
        &[(BTC_P2, "-1.0", "60000")],
        &[(BTC_P2, "60000")],
    );
    let m = s.view(A9).margin(P2).map(|m| {
        (
            m.unrealized,
            m.margin_balance,
            m.maint,
            m.initial,
            m.avail,
            m.margin_ratio,
        )
    });
    assert_eq!(
        m,
        Some((
            notional("0"),
            notional("18000"),
            notional("300"),
            notional("12000"),
            notional("6000"),
            Rate(16_666_667)
        ))
    );
}

/// Reference liquidation price (README "7."): exact rational arithmetic in i128 on decimal
/// inputs; `mmr` per inst; other insts fixed at their marks.
fn reference_liq(wallet: i128, me: (i128, i128, i128, i128), others: &[(i128, i128, i128, i128)]) -> i64 {
    // Units: quantities × 1e8, prices × 1e4, rates × 1e9, wallet × 1e6 → common scale 1e12 × 1e9.
    let (q, e, _m, mmr) = me;
    let w = wallet * 1_000_000 * 1_000_000_000; // 1e6 → 1e12, × 1e9
    let mut u = 0i128;
    let mut mt = 0i128;
    for &(qo, eo, mo, mmro) in others {
        u += qo * (mo - eo) * 1_000_000_000;
        mt += qo.abs() * mo * mmro;
    }
    let num = w + u - mt - q * e * 1_000_000_000;
    let den = q.abs() * mmr - q * 1_000_000_000;
    let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
    if den == 0 || num <= 0 {
        return 0;
    }
    let p = if q < 0 {
        (num + den - 1).div_euclid(den)
    } else {
        num.div_euclid(den)
    };
    p as i64
}

#[test]
fn sim_071_liq_px() {
    let tiers = rd_with(|s| s.mmr = btc_p2_tiers());
    let mut setup_t = setup_sim();
    setup_t.margin_models[0].mmr = MmrSource::RefdataTiers;
    let flat = i128::from(rate("0.005").0);
    let qq = |x: &str| i128::from(q(x).0);
    let pp = |x: &str| i128::from(px(x).0);
    type Row<'a> = (
        &'a str,
        &'a str,
        Vec<(InstId, &'a str, &'a str)>,
        bool,
        &'a str,
        Option<i64>,
    );
    let rows: Vec<Row> = vec![
        (
            "SIM-071.1",
            "18000",
            vec![(BTC_P2, "-1.0", "60000")],
            false,
            "77611.9403",
            None,
        ),
        (
            "SIM-071.2",
            "18000",
            vec![(BTC_P2, "1.0", "60000")],
            false,
            "42211.0552",
            None,
        ),
        (
            "SIM-071.3",
            "18000",
            vec![(BTC_P2, "-1.0", "60000"), (ETH_P2, "-10.0", "3000")],
            false,
            "77462.6866",
            None,
        ),
        (
            "SIM-071.4",
            "54000",
            vec![(BTC_P2, "-3.0", "60000")],
            true,
            "77227.7228",
            None,
        ),
        (
            "SIM-071.5",
            "60000",
            vec![(BTC_P2, "1.0", "60000")],
            false,
            "0",
            Some(0),
        ),
    ];
    for (row, wallet, pos, tiered, want, _) in &rows {
        let (setup, rd) = if *tiered {
            (setup_t.clone(), tiers.clone())
        } else {
            (setup_sim(), rd_sim())
        };
        let marks: Vec<(InstId, &str)> = pos.iter().map(|(i, _, e)| (*i, *e)).collect();
        let s = seeded(&setup, &rd, wallet, pos, &marks);
        let got = s.view(A9).liq_px(BTC_P2);
        let want_px = if *want == "0" { Px(0) } else { px(want) };
        assert_eq!(got, want_px, "{row}");
        // SIM-071.6: equal to the reference solution (flat rows; the tiered row with its final tier).
        let me_q = qq(pos[0].1);
        let others: Vec<_> = pos[1..]
            .iter()
            .map(|(_, x, e)| (qq(x), pp(e), pp(e), flat))
            .collect();
        let mmr = if *tiered { i128::from(rate("0.01").0) } else { flat };
        let r = reference_liq(
            i128::from(usdt(wallet).0),
            (me_q, pp(pos[0].2), pp(pos[0].2), mmr),
            &others,
        );
        assert_eq!(got, Px(r), "SIM-071.6 ({row})");
    }
}

#[test]
fn sim_072_liquidation_boundary() {
    // SIM-072.1
    let mut s = seeded(
        &setup_sim(),
        &rd_sim(),
        "18000",
        &[(BTC_P2, "-1.0", "60000")],
        &[(BTC_P2, "60000")],
    );
    s.mark(ms(1), BTC_P2, "77611.9402");
    s.run_to(ms(10));
    assert!(ext_fills(&s).is_empty(), "SIM-072.1");
    let m = s.view(A9).margin(P2).map(|m| m.maint < m.margin_balance);
    assert_eq!(m, Some(true), "SIM-072.1");
    // SIM-072.2
    let mut s = seeded(
        &setup_sim(),
        &rd_sim(),
        "18000",
        &[(BTC_P2, "-1.0", "60000")],
        &[(BTC_P2, "60000")],
    );
    s.out.clear();
    s.mark(ms(1), BTC_P2, "77611.9403");
    s.run_to(ms(10));
    let f = ext_fills(&s);
    assert_eq!(f.len(), 1, "SIM-072.2");
    assert_eq!(
        (
            f[0].cause,
            f[0].side,
            f[0].inst,
            f[0].qty,
            f[0].px,
            f[0].fee,
            f[0].fee_asset
        ),
        (
            ExternalCause::Liquidation,
            Side::Buy,
            BTC_P2,
            q("1.0"),
            px("78000.0001"),
            usdt("390.000001"),
            USDT
        ),
        "SIM-072.2"
    );
    assert_eq!(s.view(A9).position(BTC_P2).qty, q("0"), "SIM-072.2");
    assert_eq!(s.total(A9, P2, USDT), -usdt("390.000101").0, "SIM-072.2");
    let pos_ef = s
        .out
        .iter()
        .position(|r| matches!(r.msg, RepMsg::ExternalFill(_)));
    let pos_mr = s
        .out
        .iter()
        .rposition(|r| matches!(r.msg, RepMsg::MarginReport(_)));
    assert!(
        pos_ef.is_some() && pos_mr > pos_ef,
        "SIM-072.2: MarginReport after the ExternalFill"
    );
    assert_eq!(s.view(A9).counters().liquidations, 1, "SIM-072.2");
}

#[test]
fn sim_074_liquidate_all_positions() {
    let pos = [(BTC_P2, "-1.0", "60000"), (ETH_P2, "-10.0", "3000")];
    let mut s = seeded(
        &setup_sim(),
        &rd_sim(),
        "18000",
        &pos,
        &[(BTC_P2, "60000"), (ETH_P2, "3000")],
    );
    s.out.clear();
    s.mark(ms(1), BTC_P2, "77462.6866");
    s.run_to(ms(10));
    let f: Vec<_> = ext_fills(&s)
        .iter()
        .map(|f| (f.inst, f.side, f.qty, f.px, f.fee))
        .collect();
    // fee = ceil(notional × 0.005) at 1e-6.
    let fee = |p: &str, x: &str| {
        let v = i128::from(px(p).0) * i128::from(q(x).0); // exponent −12
        let raw = v * 5_000_000; // × rate e-9 → exponent −21
        let d = 1_000_000_000_000_000i128; // to −6
        hft_types::Qty(((raw + d - 1) / d) as i64)
    };
    assert_eq!(
        f,
        vec![
            (
                BTC_P2,
                Side::Buy,
                q("1.0"),
                px("77850.0001"),
                fee("77850.0001", "1.0")
            ),
            (ETH_P2, Side::Buy, q("10.0"), px("3015.0000"), fee("3015", "10.0")),
        ]
    );
    assert_eq!(s.view(A9).position(BTC_P2).qty, q("0"));
    assert_eq!(s.view(A9).position(ETH_P2).qty, q("0"));
}

#[test]
fn sim_075_margin_report_fields() {
    let pos = [(BTC_P2, "-1.0", "60000"), (ETH_P2, "-10.0", "3000")];
    let mut s = seeded(
        &setup_sim(),
        &rd_sim(),
        "18000",
        &pos,
        &[(BTC_P2, "60000"), (ETH_P2, "3000")],
    );
    s.run_to(S);
    let mr = margin_reports(&s);
    let (h, items) = mr.last().cloned().unwrap_or_else(|| panic!("no MarginReport"));
    assert_eq!(
        (h.margin_balance, h.maint_margin, h.avail_margin, h.margin_ratio),
        (notional("18000"), notional("450"), notional("0"), rate("0.025"))
    );
    let it: Vec<_> = items.iter().map(|i| (i.inst, i.liq_px, i.adl_quantile)).collect();
    assert_eq!(
        it,
        vec![(BTC_P2, px("77462.6866"), 0), (ETH_P2, px("4746.2687"), 0)]
    );
}

/// Margin ratio raw of the SIM-076 position (short 1 @ 60 000, wallet 18 000) at mark `m` raw.
fn ratio_at(m: i64) -> i64 {
    let m = i128::from(m);
    let num = 5_000_000 * m;
    let den = 780_000_000 - m;
    ((num + den - 1) / den) as i64
}

/// Smallest mark raw whose ratio is at least `r0 + d`.
fn mark_for(d: i64) -> i64 {
    let r0 = ratio_at(600_000_000);
    let (mut lo, mut hi) = (600_000_000i64, 779_000_000i64);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if ratio_at(mid) - r0 >= d {
            hi = mid
        } else {
            lo = mid + 1
        }
    }
    lo
}

#[test]
fn sim_076_margin_report_cadence() {
    // (row, margin_data, mark raw, time after the first report, expect report)
    let below = mark_for(9_999_000);
    assert!(ratio_at(below) - ratio_at(600_000_000) < 10_000_000);
    let rows: [(&str, MarginData, i64, u64, bool); 6] = [
        ("SIM-076.1", MarginData::Push, below, 500, false),
        ("SIM-076.2", MarginData::Push, mark_for(10_000_000), 500, true),
        ("SIM-076.3", MarginData::Push, 600_000_000, 999, false),
        ("SIM-076.4", MarginData::Push, 600_000_000, 1_000, true),
        (
            "SIM-076.5",
            MarginData::Poll { interval_ms: 5_000 },
            mark_for(50_000_000),
            4_999,
            false,
        ),
        (
            "SIM-076.6",
            MarginData::Poll { interval_ms: 5_000 },
            600_000_000,
            5_000,
            true,
        ),
    ];
    for (row, md, m, t, want) in rows {
        let rd = rd_with(|s| s.caps_mut(P2).margin_data = md);
        let mut s = seeded(
            &setup_sim(),
            &rd,
            "18000",
            &[(BTC_P2, "-1.0", "60000")],
            &[(BTC_P2, "60000")],
        );
        s.drain(ms(10));
        assert_eq!(margin_reports(&s).len(), 1, "{row}: first report at 0 ms");
        s.out.clear();
        s.mark_ex(ms(t), ms(t) as i64, BTC_P2, Px(m));
        s.drain(ms(t) + ms(10));
        assert_eq!(!margin_reports(&s).is_empty(), want, "{row}");
    }
}
