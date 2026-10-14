//! LED-120–LED-139, LED-268, LED-270: funding reconciliation corrections, no-history items,
//! window sweep, record ring and watermark.

use crate::common;

use common::*;
use hft_event::NoteLevel;
use hft_ledger::notes::code;
use hft_ledger::snapshot::{FundMarkRow, FundNoHistRow, SECTION_LEDGER_GROUPS, row_kind};
use hft_ledger::{
    AdjustDelta, AdjustKind, FixedVec, FundingRecon, FundingReconItem, Ledger, LedgerConfig, PnlBreakdown,
    PnlDelta, fc_flags,
};
use hft_refdata::FundingSettleKind::{self, BalanceOnlyPush, PerSymbolPush, RateOnly};
use hft_types::{GROUP_NONE, INST_NONE, InstId, Notional, Px, Qty};
use zerocopy::FromBytes;

const MIN: i64 = 60_000_000_000;

fn fund_a1_cfg(kind: FundingSettleKind, cfg: LedgerConfig) -> Ledger {
    let mut l = ledger_with(rd_funding(kind), &cfg);
    boot_a1(&mut l, 0);
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    mark(&mut l, BTC_P2, "60000", s(1));
    l.clear_notes();
    l
}

fn fund_a1(kind: FundingSettleKind) -> Ledger {
    fund_a1_cfg(kind, cfg_lf())
}

/// `R(inst, settle, exch, engine)`.
fn r(inst: InstId, settle: i64, exch: &str, engine: &str) -> FundingReconItem {
    FundingReconItem {
        account: A1,
        venue: P2,
        inst,
        settle_ts_ns: settle,
        exch_amount: n(exch),
        engine_amount: n(engine),
        report_id: 9,
    }
}

/// LED-090's state (`T1` push of 6.0 booked).
fn after_090() -> Ledger {
    let mut l = fund_a1(PerSymbolPush);
    l.apply_funding(&push(BTC_P2, "6.0", T1), s(10));
    l.clear_notes();
    l
}

fn wallet(l: &Ledger) -> Notional {
    l.margin(A1, P2).wallet
}

fn adj_qty(x: &FundingRecon) -> Option<Qty> {
    match x {
        FundingRecon::Corrected { adj, .. } => Some(adj.qty),
        _ => None,
    }
}

fn fc_notes(l: &Ledger) -> Vec<hft_event::Note> {
    notes_of(l, code::FUNDING_CORRECTED)
}

#[test]
fn led_120_too_recent_boundary() {
    let rows = [
        (T1 + s(599), false),
        (T1 + s(600) - 1, false),
        (T1 + s(600), true),
        (T1 + s(600) + 1, true),
    ];
    for (k, (now, corrected)) in rows.iter().enumerate() {
        let mut l = after_090();
        let before = snap(&l);
        let x = l.reconcile_funding(&r(BTC_P2, T1, "9.0", "6.0"), *now, s(30));
        if *corrected {
            assert_eq!(adj_qty(&x), Some(usdt("3.0")), "LED-120.{}", k + 1);
        } else {
            assert_eq!(x, FundingRecon::TooRecent, "LED-120.{}", k + 1);
            assert_eq!(snap(&l), before, "LED-120.{}", k + 1);
        }
    }
}

#[test]
fn led_121_outside_window_boundary() {
    for (k, (now, corrected)) in [
        (T1 + 48 * H - 1, true),
        (T1 + 48 * H, true),
        (T1 + 48 * H + 1, false),
    ]
    .iter()
    .enumerate()
    {
        let mut l = after_090();
        let before = snap(&l);
        let x = l.reconcile_funding(&r(BTC_P2, T1, "9.0", "6.0"), *now, s(30));
        if *corrected {
            assert!(adj_qty(&x).is_some(), "LED-121.{}", k + 1);
        } else {
            assert_eq!(x, FundingRecon::OutsideWindow, "LED-121.{}", k + 1);
            assert_eq!(snap(&l), before, "LED-121.{}", k + 1);
        }
    }
}

fn dust_rows(id: &str) {
    for (k, (exch, want)) in [
        ("6.009999", None),
        ("6.01", Some(usdt("0.01"))),
        ("5.990001", None),
    ]
    .iter()
    .enumerate()
    {
        let mut l = after_090();
        let x = l.reconcile_funding(&r(BTC_P2, T1, exch, "6.0"), T1 + H, s(30));
        match want {
            None => assert_eq!(x, FundingRecon::Matched, "{id}.{}", k + 1),
            Some(q) => assert_eq!(adj_qty(&x), Some(*q), "{id}.{}", k + 1),
        }
    }
}

#[test]
fn led_122_matched_dust_boundary() {
    dust_rows("LED-122");
}

#[test]
fn led_268_dust_boundary() {
    dust_rows("LED-268");
}

/// LED-123's step on LED-090's state.
fn after_123() -> (Ledger, FundingRecon) {
    let mut l = after_090();
    let x = l.reconcile_funding(&r(BTC_P2, T1, "9.0", "6.0"), T1 + H, s(30));
    (l, x)
}

#[test]
fn led_123_per_symbol_corrected() {
    let l0 = after_090();
    let (w0, u0, g0, pos0) = (
        wallet(&l0),
        cash(&l0, A1, P2, USDT).total,
        l0.group(A1, G1).pnl.funding,
        *l0.pos(A1, BTC_P2),
    );
    let (l, x) = after_123();
    let mut d = PnlBreakdown::zero();
    d.funding = n("3");
    assert_eq!(
        x,
        FundingRecon::Corrected {
            adj: AdjustDelta {
                account: A1,
                venue: P2,
                kind: AdjustKind::Funding as u8,
                px_src: 0,
                inst: BTC_P2,
                asset: USDT,
                _pad: 0,
                group: G1,
                qty: usdt("3.0"),
                px: Px(0),
                report_id: 9,
                cmd_seq: 0,
                settle_ts_ns: T1,
            },
            pnl: PnlDelta {
                account: A1,
                group: G1,
                d,
            },
        }
    );
    assert_eq!(cash(&l, A1, P2, USDT).total.0 - u0.0, usdt("3").0);
    assert_eq!(wallet(&l).0 - w0.0, n("3").0);
    assert_eq!(l.group(A1, G1).pnl.funding.0 - g0.0, n("3").0);
    let h = l.fund_hist(A1, P2);
    assert_eq!(h[0].corrected, n("3"));
    let ns = fc_notes(&l);
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].level, NoteLevel::P1);
    assert_eq!(
        ns[0].v,
        [
            u64::from(P2.0) | u64::from(A1.0) << 8,
            u64::from(BTC_P2.0),
            T1 as u64,
            n("9").0 as u64,
            n("6").0 as u64,
            n("3").0 as u64,
            u64::from(G1.0),
            0
        ]
    );
    assert_eq!(*l.pos(A1, BTC_P2), pos0);
}

#[test]
fn led_124_rate_only_corrected() {
    let mut l = fund_a1(RateOnly);
    let x = l
        .synth_funding(A1, BTC_P2, px("60000"), rate("0.0001"), T1)
        .expect("synth");
    l.apply_funding(&x, s(10));
    let (w0, g0) = (wallet(&l), l.group(A1, G1).pnl.funding);
    let y = l.reconcile_funding(&r(BTC_P2, T1, "6.2", "6.0"), T1 + H, s(30));
    assert_eq!(adj_qty(&y), Some(usdt("0.2")));
    assert_eq!(wallet(&l).0 - w0.0, n("0.2").0);
    assert_eq!(l.group(A1, G1).pnl.funding.0 - g0.0, n("0.2").0);
}

/// LED-095's state (BalanceOnlyPush, synthesized 6.00 at `T1`, no push).
fn after_095() -> Ledger {
    let mut l = fund_a1(BalanceOnlyPush);
    let x = l
        .synth_funding(A1, BTC_P2, px("60000"), rate("0.0001"), T1)
        .expect("synth");
    l.apply_funding(&x, s(10));
    l.clear_notes();
    l
}

#[test]
fn led_126_wallet_item_push_lost() {
    let mut l = after_095();
    let (w0, u0) = (wallet(&l), cash(&l, A1, P2, USDT).total);
    assert_eq!(l.account(A1).funding_residual, n("-6"));
    let x = l.reconcile_funding(&r(INST_NONE, T1, "6.10", "0"), T1 + H, s(30));
    match x {
        FundingRecon::Corrected { adj, .. } => {
            assert_eq!(
                (adj.inst, adj.group, adj.qty),
                (INST_NONE, GROUP_NONE, usdt("6.10"))
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(wallet(&l).0 - w0.0, n("6.10").0);
    assert_eq!(cash(&l, A1, P2, USDT).total.0 - u0.0, usdt("6.10").0);
    assert_eq!(l.account(A1).funding_residual, n("0.10"));
    let rec = l
        .fund_hist(A1, P2)
        .into_iter()
        .find(|h| h.inst == INST_NONE)
        .expect("record");
    assert_eq!((rec.booked, rec.corrected), (Notional(0), n("6.10")));
    assert_eq!(fc_notes(&l)[0].v[7], fc_flags::NEW_RECORD);
}

#[test]
fn led_127_wallet_item_double_push() {
    let mut l = after_095();
    l.apply_funding_push(&push(INST_NONE, "6.10", T1), s(11));
    l.apply_funding_push(&push(INST_NONE, "6.09", T1), s(12));
    let (w0, res0) = (wallet(&l), l.account(A1).funding_residual);
    let x = l.reconcile_funding(&r(INST_NONE, T1, "6.10", "12.19"), T1 + H, s(30));
    assert_eq!(adj_qty(&x), Some(usdt("-6.09")));
    assert_eq!(wallet(&l).0 - w0.0, n("-6.09").0);
    assert_eq!(l.account(A1).funding_residual.0 - res0.0, n("-6.09").0);
}

#[test]
fn led_128_second_report_matched() {
    let (mut l, _) = after_123();
    l.clear_notes();
    let (w0, g0) = (wallet(&l), l.group(A1, G1).pnl.funding);
    let x = l.reconcile_funding(&r(BTC_P2, T1, "9.0", "9.0"), T1 + 2 * H, s(40));
    assert_eq!(x, FundingRecon::Matched);
    assert_eq!((wallet(&l), l.group(A1, G1).pnl.funding), (w0, g0));
    assert!(fc_notes(&l).is_empty());
}

#[test]
fn led_129_missing_record_full_correction() {
    // .1
    let mut l = fund_a1(PerSymbolPush);
    let x = l.reconcile_funding(&r(BTC_P2, T1, "6.0", "0"), T1 + H, s(30));
    match x {
        FundingRecon::Corrected { adj, .. } => {
            assert_eq!((adj.group, adj.qty), (G1, usdt("6.0")), "LED-129.1")
        }
        other => panic!("LED-129.1 {other:?}"),
    }
    let h = l.fund_hist(A1, P2);
    assert_eq!((h[0].booked, h[0].corrected), (Notional(0), n("6")), "LED-129.1");
    assert_eq!(fc_notes(&l)[0].v[7], fc_flags::NEW_RECORD, "LED-129.1");
    // .2
    let mut l = fund_a1(PerSymbolPush);
    l.set_cell_shadow(A1, G1, true);
    let g0 = l.group(A1, G1).pnl.funding;
    let x = l.reconcile_funding(&r(BTC_P2, T1, "6.0", "0"), T1 + H, s(30));
    match x {
        FundingRecon::Corrected { adj, .. } => assert_eq!(adj.group, GROUP_NONE, "LED-129.2"),
        other => panic!("LED-129.2 {other:?}"),
    }
    assert_eq!(l.account(A1).day.funding, n("6"), "LED-129.2");
    assert_eq!(l.group(A1, G1).pnl.funding, g0, "LED-129.2");
}

/// LED-130's state: one held no-history item (`D − 8 h`, exch 5.0, engine 2.0).
fn after_130() -> (Ledger, FundingRecon) {
    let mut l = fund_a1(PerSymbolPush);
    let x = l.reconcile_funding(&r(BTC_P2, D - 8 * H, "5.0", "2.0"), D + H, s(30));
    (l, x)
}

fn rows_of(l: &Ledger, kinds: &[u8]) -> Vec<(hft_ledger::snapshot::LedgerRowHdr, Vec<u8>)> {
    let b = snap(l);
    ledger_rows(&section(&b, SECTION_LEDGER_GROUPS))
        .into_iter()
        .filter(|(h, _)| kinds.contains(&h.kind))
        .collect()
}

fn no_hist_rows(l: &Ledger) -> Vec<FundNoHistRow> {
    rows_of(l, &[row_kind::FUND_NO_HIST])
        .iter()
        .map(|(_, b)| FundNoHistRow::read_from_bytes(b).expect("row"))
        .collect()
}

#[test]
fn led_130_no_history_held() {
    let l0 = fund_a1(PerSymbolPush);
    let kinds = [row_kind::ACCOUNT, row_kind::GROUP, row_kind::MARGIN];
    let (rows0, inv0) = (rows_of(&l0, &kinds), section(&snap(&l0), 22).body.to_vec());
    let (l, x) = after_130();
    assert_eq!(x, FundingRecon::NoHistoryHeld);
    assert_eq!(rows_of(&l, &kinds), rows0);
    assert_eq!(
        section(&snap(&l), 22).body,
        inv0.as_slice(),
        "Cash rows unchanged"
    );
    assert_eq!(
        no_hist_rows(&l),
        [FundNoHistRow {
            settle_ts_ns: D - 8 * H,
            exch_amount: n("5"),
            engine_amount: n("2"),
            report_id: 9,
        }]
    );
    let ns = fc_notes(&l);
    assert_eq!(ns.len(), 1);
    assert_eq!((ns[0].level, ns[0].v[7]), (NoteLevel::P1, fc_flags::NO_HISTORY));
}

#[test]
fn led_131_no_history_updated() {
    let (mut l, _) = after_130();
    let before = rows_of(&l, &[row_kind::ACCOUNT, row_kind::GROUP, row_kind::MARGIN]);
    let mut it = r(BTC_P2, D - 8 * H, "5.5", "2.0");
    it.report_id = 10;
    assert_eq!(
        l.reconcile_funding(&it, D + 2 * H, s(40)),
        FundingRecon::NoHistoryHeld
    );
    let rows = no_hist_rows(&l);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].exch_amount, rows[0].report_id), (n("5.5"), 10));
    assert_eq!(
        rows_of(&l, &[row_kind::ACCOUNT, row_kind::GROUP, row_kind::MARGIN]),
        before
    );
}

fn sweep(l: &mut Ledger, now: i64, t: i64) -> Vec<FundingRecon> {
    let mut out = FixedVec::new();
    l.funding_window_sweep(A1, P2, now, t, &mut out);
    out.iter().copied().collect()
}

#[test]
fn led_132_window_sweep() {
    let (mut l, _) = after_130();
    l.clear_notes();
    assert!(sweep(&mut l, D - 8 * H + 48 * H, s(50)).is_empty(), "LED-132.1");
    assert_eq!(no_hist_rows(&l).len(), 1, "LED-132.1");
    let (w0, g0) = (wallet(&l), l.group(A1, G1).pnl.funding);
    let out = sweep(&mut l, D - 8 * H + 48 * H + 1, s(51));
    assert_eq!(out.len(), 1, "LED-132.2");
    assert_eq!(adj_qty(&out[0]), Some(usdt("3.0")), "LED-132.2");
    assert_eq!(wallet(&l).0 - w0.0, n("3").0, "LED-132.2");
    assert_eq!(l.group(A1, G1).pnl.funding.0 - g0.0, n("3").0, "LED-132.2");
    assert_eq!(
        fc_notes(&l)[0].v[7],
        fc_flags::NO_HISTORY | fc_flags::WINDOW_END,
        "LED-132.2"
    );
    assert!(no_hist_rows(&l).is_empty(), "LED-132.2");
    let before = snap(&l);
    assert!(
        sweep(&mut l, D - 8 * H + 48 * H + 2, s(52)).is_empty(),
        "LED-132.3"
    );
    assert_eq!(snap(&l), before, "LED-132.3");
    let x = l.reconcile_funding(&r(BTC_P2, D - 8 * H, "5.0", "2.0"), D + 41 * H, s(53));
    assert_eq!(x, FundingRecon::OutsideWindow, "LED-132.4");
}

#[test]
fn led_133_sweep_engine_zero_full_amount() {
    let mut l = fund_a1(PerSymbolPush);
    let x = l.reconcile_funding(&r(BTC_P2, D - 8 * H, "4.0", "0"), D + H, s(30));
    assert_eq!(x, FundingRecon::NoHistoryHeld);
    let out = sweep(&mut l, D + 40 * H + 1, s(50));
    assert_eq!(out.len(), 1);
    assert_eq!(adj_qty(&out[0]), Some(usdt("4.0")));
}

#[test]
fn led_134_no_hist_table_full() {
    let mut l = fund_a1(PerSymbolPush);
    l.on_funding_interval(BTC_P2, 1_800);
    for k in 2..=65 {
        let x = l.reconcile_funding(&r(BTC_P2, D - k * 30 * MIN, "1.0", "0"), D + H, s(30));
        assert_eq!(x, FundingRecon::NoHistoryHeld, "k = {k}");
    }
    l.clear_notes();
    let x = l.reconcile_funding(&r(BTC_P2, D - 30 * MIN, "1.0", "0"), D + H, s(30));
    match x {
        FundingRecon::Corrected { adj, .. } => {
            assert_eq!((adj.settle_ts_ns, adj.qty), (D - 65 * 30 * MIN, usdt("1.0")));
        }
        other => panic!("{other:?}"),
    }
    let flags: Vec<u64> = fc_notes(&l).iter().map(|n| n.v[7]).collect();
    assert!(
        flags.contains(&(fc_flags::NO_HISTORY | fc_flags::WINDOW_END)),
        "{flags:?}"
    );
    assert!(flags.contains(&fc_flags::NO_HISTORY), "{flags:?}");
    assert_eq!(l.fund_no_hist(A1, P2).len(), 64);
    assert!(l.fund_no_hist(A1, P2).iter().any(|x| x.1 == D - 30 * MIN));
}

#[test]
fn led_135_balance_only_sweep_routes() {
    let mut l = fund_a1(BalanceOnlyPush);
    let a = l.reconcile_funding(&r(BTC_P2, D - 8 * H, "2.0", "0"), D + H, s(30));
    assert_eq!(a, FundingRecon::NotApplicable);
    let b = l.reconcile_funding(&r(INST_NONE, D - 8 * H, "2.0", "0"), D + H, s(31));
    assert_eq!(b, FundingRecon::NoHistoryHeld);
    let held: Vec<InstId> = l.fund_no_hist(A1, P2).iter().map(|x| x.0).collect();
    assert_eq!(held, [INST_NONE], "LED-135.1");
    let (g0, w0, res0) = (
        l.group(A1, G1).pnl.funding,
        wallet(&l),
        l.account(A1).funding_residual,
    );
    let out = sweep(&mut l, D + 40 * H + 1, s(50));
    assert_eq!(out.len(), 1);
    assert_eq!(l.group(A1, G1).pnl.funding, g0, "LED-135.1");
    assert_eq!(wallet(&l).0 - w0.0, n("2").0, "LED-135.2");
    assert_eq!(l.account(A1).funding_residual.0 - res0.0, n("2").0, "LED-135.2");
}

#[test]
fn led_136_hist_ring_capacity() {
    let mut cfg = cfg_lf();
    cfg.funding_hist_cap = 4;
    let mut l = fund_a1_cfg(PerSymbolPush, cfg);
    for k in 1..=4 {
        l.apply_funding(&push(BTC_P2, "1.0", D + k * 8 * H), s(10 + k));
    }
    assert_eq!(l.fund_hist(A1, P2).len(), 4, "LED-136.1");
    assert_eq!(l.hist_from_ns(A1, P2), Some(D), "LED-136.1");
    l.apply_funding(&push(BTC_P2, "1.0", D + 5 * 8 * H), s(20));
    let ts: Vec<i64> = l.fund_hist(A1, P2).iter().map(|h| h.settle_ts_ns).collect();
    assert_eq!(
        ts,
        (2..=5).map(|k| D + k * 8 * H).collect::<Vec<_>>(),
        "LED-136.2"
    );
    assert_eq!(l.hist_from_ns(A1, P2), Some(D + 8 * H + 1), "LED-136.2");
    let x = l.reconcile_funding(&r(BTC_P2, D + 8 * H, "1.0", "1.0"), D + 41 * H, s(30));
    assert_eq!(x, FundingRecon::NoHistoryHeld, "LED-136.3");
    let x = l.reconcile_funding(&r(BTC_P2, D + 16 * H, "1.0", "1.0"), D + 41 * H, s(31));
    assert_eq!(x, FundingRecon::Matched, "LED-136.4");
}

#[test]
fn led_137_hist_from_at_init() {
    let mut l = ledger_lf();
    l.on_balance(&bal(A1, P2, USDT, usdt("50000"), Qty(0), D + 3 * H), true, s(1));
    let marks: Vec<FundMarkRow> = rows_of(&l, &[row_kind::FUND_MARK])
        .iter()
        .filter(|(h, _)| h.account == A1 && h.venue == P2)
        .map(|(_, b)| FundMarkRow::read_from_bytes(b).expect("row"))
        .collect();
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].hist_from_ns, D + 3 * H);
    let x = l.reconcile_funding(&r(BTC_P2, D, "1.0", "0"), D + 4 * H, s(2));
    assert_eq!(x, FundingRecon::NoHistoryHeld);
}

#[test]
fn led_138_hist_from_after_restore_without_mark() {
    let l = after_090();
    let bytes = snap(&l);
    let rows: Vec<_> = ledger_rows(&section(&bytes, SECTION_LEDGER_GROUPS))
        .into_iter()
        .filter(|(h, _)| {
            !(h.account == A1 && h.venue == P2 && matches!(h.kind, row_kind::FUND_MARK | row_kind::FUND_HIST))
        })
        .collect();
    let edited = with_rows13(&bytes, &rows);
    let mut r2 = restore(&edited).expect("restore");
    assert_eq!(r2.hist_from_ns(A1, P2), None);
    let x = r2.reconcile_funding(&r(BTC_P2, T1, "6.0", "6.0"), T1 + 2 * H, s(30));
    assert_eq!(x, FundingRecon::NoHistoryHeld);
    assert_eq!(r2.hist_from_ns(A1, P2), Some(T1 + 2 * H));
    let b2 = snap(&r2);
    let mark = ledger_rows(&section(&b2, SECTION_LEDGER_GROUPS))
        .into_iter()
        .find(|(h, _)| h.kind == row_kind::FUND_MARK && h.account == A1 && h.venue == P2)
        .map(|(_, b)| FundMarkRow::read_from_bytes(&b).expect("row").hist_from_ns);
    assert_eq!(mark, Some(T1 + 2 * H));
}

#[test]
fn led_139_no_hist_survives_restore() {
    let (l, _) = after_130();
    let mut direct = l.clone();
    let want = sweep(&mut direct, D + 40 * H + 1, s(60));
    let mut r2 = restore(&snap(&l)).expect("restore");
    let got = sweep(&mut r2, D + 40 * H + 1, s(60));
    assert_eq!(got.len(), 1);
    assert_eq!(adj_qty(&got[0]), Some(usdt("3.0")));
    assert_eq!(got, want);
}

#[test]
fn led_270_balance_only_inst_item_not_applicable() {
    let mut l = after_095();
    l.apply_funding_push(&push(INST_NONE, "6.04", T1 + 1_500_000_000), s(11));
    l.clear_notes();
    let before = (
        l.group(A1, G1).pnl.funding,
        l.account(A1).funding_residual,
        wallet(&l),
        cash(&l, A1, P2, USDT).total,
    );
    let x = l.reconcile_funding(&r(BTC_P2, T1, "6.04", "6.00"), T1 + H, s(30));
    assert_eq!(x, FundingRecon::NotApplicable);
    assert_eq!(
        (
            l.group(A1, G1).pnl.funding,
            l.account(A1).funding_residual,
            wallet(&l),
            cash(&l, A1, P2, USDT).total
        ),
        before
    );
    assert!(l.fund_no_hist(A1, P2).is_empty());
    assert!(fc_notes(&l).is_empty());
}
