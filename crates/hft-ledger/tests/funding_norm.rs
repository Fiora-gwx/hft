//! LED-110–LED-114, LED-116: settle-time normalization grid.

use crate::common;

use common::*;
use hft_ledger::snapshot::{LedgerRowHdr, SECTION_LEDGER_GROUPS, row_kind};
use hft_ledger::{FundingCheck, FundingPush, FundingRecon, FundingReconItem, Ledger};
use hft_refdata::FundingSettleKind::{self, BalanceOnlyPush, PerSymbolPush};
use hft_strategy_sdk::SnapshotReader;
use hft_types::{INST_NONE, Notional};
use zerocopy::FromBytes;

fn fund_a1(kind: FundingSettleKind) -> Ledger {
    let mut l = ledger_with(rd_funding(kind), &cfg_lf());
    boot_a1(&mut l, 0);
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    mark(&mut l, BTC_P2, "60000", s(1));
    l.clear_notes();
    l
}

#[test]
fn led_110_grid_defaults_from_refdata() {
    let l = ledger_lf();
    assert_eq!(l.funding_grid(P2, BTC_P2), (28_800, 0));
    assert_eq!(l.funding_grid(P2, INST_NONE), (28_800, 0));
}

#[test]
fn led_111_interval_change_regrids() {
    let mut l = fund_a1(PerSymbolPush);
    l.on_funding_interval(BTC_P2, 14_400);
    l.apply_funding(&push(BTC_P2, "3.0", D + 4 * H + 300_000_000), s(10));
    assert_eq!(l.funding_grid(P2, BTC_P2), (14_400, 0));
    let h = l.fund_hist(A1, P2);
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].settle_ts_ns, D + 4 * H);
}

#[test]
fn led_112_inst_none_finest_grid() {
    let mut l = fund_a1(BalanceOnlyPush);
    l.on_funding_interval(ETH_P2, 14_400);
    l.apply_funding_push(&push(INST_NONE, "1.0", D + 4 * H + 1_500_000_000), s(10));
    assert_eq!(l.funding_grid(P2, INST_NONE), (14_400, 0));
    let h = l.fund_hist(A1, P2);
    assert!(
        h.iter()
            .any(|x| x.inst == INST_NONE && x.settle_ts_ns == D + 4 * H),
        "{h:?}"
    );
}

#[test]
fn led_113_late_push_pairs() {
    let mut l = fund_a1(BalanceOnlyPush);
    l.apply_funding(&fs(BTC_P2, "6.00", T1, 1), s(5));
    let r = l.apply_funding_push(&push(INST_NONE, "6.00", T1 + s(2)), s(10));
    assert_eq!(
        r,
        FundingPush::Booked {
            check: FundingCheck::Ok
        }
    );
    assert_eq!(l.account(A1).funding_residual, Notional(0));
    assert!(l.fund_pending(A1, P2).is_empty());
}

#[test]
fn led_114_recon_item_matches_same_record() {
    let mut l = fund_a1(PerSymbolPush);
    l.apply_funding(&push(BTC_P2, "6.0", T1), s(10));
    let it = FundingReconItem {
        account: A1,
        venue: P2,
        inst: BTC_P2,
        settle_ts_ns: T1 - s(1),
        exch_amount: n("6.0"),
        engine_amount: n("6.0"),
        report_id: 1,
    };
    assert_eq!(l.reconcile_funding(&it, T1 + H, s(20)), FundingRecon::Matched);
    assert_eq!(l.fund_hist(A1, P2).len(), 1);
}

#[test]
fn led_116_grid_rebuilt_after_restore() {
    let mut l = fund_a1(PerSymbolPush);
    l.on_funding_interval(BTC_P2, 14_400);
    let bytes = snap(&l);
    let mut r = restore_with(&bytes, rd_funding(PerSymbolPush)).expect("restore");
    assert_eq!(r.funding_grid(P2, BTC_P2), (28_800, 0), "step 1");
    r.on_funding_interval(BTC_P2, 14_400);
    assert_eq!(r.funding_grid(P2, BTC_P2), l.funding_grid(P2, BTC_P2), "step 2");
    assert_eq!(r.funding_grid(P2, BTC_P2), (14_400, 0), "step 2");
    // Section 13 holds only the listed row kinds (no grid rows).
    let sec = section(&bytes, SECTION_LEDGER_GROUPS);
    let mut rr = SnapshotReader::new(sec.body);
    let mut kinds = Vec::new();
    for _ in 0..sec.count {
        let h: LedgerRowHdr = rr.get_row().expect("hdr");
        kinds.push(h.kind);
        let body = match h.kind {
            row_kind::ACCOUNT => 88,
            row_kind::RATE | row_kind::FUND_PENDING | row_kind::FUND_HIST | row_kind::FUND_NO_HIST => 32,
            row_kind::GROUP => 552,
            row_kind::MARGIN => {
                let m = rr.get_bytes(64).expect("margin");
                let n_items = u16::read_from_bytes(&m[56..58]).expect("n");
                usize::from(n_items) * 16
            }
            row_kind::FUND_MARK | row_kind::FUND_SEEN | row_kind::AUTO_HELD => 16,
            row_kind::PX => 48,
            row_kind::EXT_SEEN => 24,
            k => panic!("unexpected row kind {k}"),
        };
        rr.get_bytes(body).expect("body");
    }
    assert!(!kinds.is_empty());
}
