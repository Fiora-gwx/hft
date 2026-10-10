//! CTL-213.

use super::*;

#[test]
fn ctl_213_reconcile_report_group() {
    let mut r = E2e::with(Opts {
        no_recon: vec![(ACCT_A, V_BG_SPOT)],
        ..Opts::default()
    });
    let bit = 1u64 << V_BG_SPOT.0;
    assert_eq!(
        r.s.engine().risk.mode.venue_bitmaps(ACCT_A)[0] & bit,
        0,
        "CTL-213 前置"
    );
    let m = r.mark();
    let items = json!([
        {"venue":"bitget-spot","account":"acct-a","item":"Position","inst":"bitget-spot:BTCUSDT","engine_qty":"0","exch_qty":"0"},
        {"venue":"bitget-spot","account":"acct-a","item":"Summary"}
    ]);
    let x = r.report(9, items);
    want(&x, "Applied", None, "CTL-213");
    assert_eq!(x.status, hft_ctl_proto::TransportStatus::Ok, "CTL-213");
    let reps: Vec<ReconcileReport> = r
        .kinds_since(m, EventKind::RECONCILE_REPORT)
        .iter()
        .filter_map(|x| x.read::<ReconcileReport>())
        .collect();
    assert_eq!(reps.len(), 2, "CTL-213 records");
    assert_eq!(
        (reps[0].report_id, reps[0].idx, reps[0].item),
        (9, 0, 1),
        "CTL-213 order"
    );
    assert_eq!(
        (reps[1].report_id, reps[1].idx, reps[1].item),
        (9, 1, 5),
        "CTL-213 order"
    );
    assert_ne!(
        r.s.engine().risk.mode.venue_bitmaps(ACCT_A)[0] & bit,
        0,
        "CTL-213 RECONCILED"
    );
    assert!(
        r.kinds_since(m, EventKind::CMD_RESULT).is_empty(),
        "CTL-213 CmdResult"
    );
}
