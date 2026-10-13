use hft_event::{EventKind, OrderEvent, SnapshotHdr};
use hft_sequencer::snapshot::PURPOSE_ON_DEMAND;
use serde_json::Value;

use super::{args, fixture, run};

#[test]
fn rpl_285_spot_notional_files_verify_full_and_between_fill_snapshots() {
    for side in [hft_types::Side::Buy, hft_types::Side::Sell] {
        let f = fixture::spot_notional(side);
        let before = std::fs::read(&f.file).unwrap();
        let log = fixture::read(&f);
        let fills: Vec<_> = log
            .recs
            .iter()
            .filter(|r| r.kind == EventKind::ORDER_EVENT)
            .map(|r| r.read::<OrderEvent>().unwrap())
            .collect();
        assert_eq!(fills.len(), 2);
        assert_eq!(
            (fills[0].cum_filled.0, fills[1].cum_filled.0),
            (400_000, 1_000_000)
        );
        let snapshots: Vec<_> = log
            .recs
            .iter()
            .filter(|r| {
                r.kind == EventKind::STATE_SNAPSHOT
                    && r.read::<SnapshotHdr>()
                        .is_some_and(|h| h.purpose == PURPOSE_ON_DEMAND)
            })
            .map(|r| r.seq)
            .collect();
        assert_eq!(snapshots.len(), 3);
        for (seq, count) in snapshots.iter().zip(0..=2) {
            assert_eq!(
                log.recs
                    .iter()
                    .filter(|r| r.seq < *seq && r.kind == EventKind::ORDER_EVENT)
                    .count(),
                count
            );
        }
        for from in [None, Some(snapshots[0]), Some(snapshots[1])] {
            let mut a = args(&f.file);
            if let Some(seq) = from {
                a.extend(["--from-seq".into(), seq.to_string()]);
            }
            let (code, text, err) = run(&a);
            assert_eq!(code, 0, "{side:?}, {from:?}: {text} {err}");
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["summary"]["verify"], "pass");
            assert!(v["divergences"].as_array().unwrap().is_empty());
            assert!(v["summary"]["snapshots_checked"].as_u64().unwrap() >= 2);
        }
        assert_eq!(std::fs::read(&f.file).unwrap(), before);
    }
}

#[test]
fn rpl_282_original_receipt_files_verify_full_and_snapshot_windows() {
    for positive in [false, true] {
        let f = fixture::original_receipt(positive);
        let before = std::fs::read(&f.file).unwrap();
        let log = fixture::read(&f);
        let duplicates: Vec<_> = log
            .recs
            .iter()
            .filter(|r| r.kind == EventKind::ORDER_EVENT)
            .map(|r| r.read::<OrderEvent>().unwrap())
            .collect();
        assert_eq!(duplicates.len(), 2);
        assert_eq!(
            duplicates.iter().map(|e| e.slot.0).collect::<Vec<_>>(),
            vec![0, 700]
        );
        assert!(duplicates.iter().all(|e| (e.trade_id > 0) == positive));
        let checkpoints: Vec<_> = log
            .recs
            .iter()
            .filter(|r| {
                r.kind == EventKind::STATE_SNAPSHOT
                    && r.read::<SnapshotHdr>()
                        .is_some_and(|h| h.purpose == PURPOSE_ON_DEMAND)
            })
            .map(|r| r.seq)
            .collect();
        assert_eq!(checkpoints.len(), 4, "one snapshot per SnapshotNow command");
        for (seq, (raw, orders)) in checkpoints.iter().zip([(0, 0), (1, 0), (4, 0), (6, 2)]) {
            let count = |kind| log.recs.iter().filter(|r| r.seq < *seq && r.kind == kind).count();
            assert_eq!(
                count(EventKind::EXTERNAL_FILL),
                raw,
                "raw receipts before {seq:?}"
            );
            assert_eq!(
                count(EventKind::ORDER_EVENT),
                orders,
                "cumulative duplicates before {seq:?}"
            );
        }
        // The last frame is the endpoint. All earlier windows must replay the duplicates.
        for from in
            std::iter::once(None).chain(checkpoints[..checkpoints.len() - 1].iter().copied().map(Some))
        {
            let mut a = args(&f.file);
            if let Some(seq) = from {
                a.extend(["--from-seq".into(), seq.to_string()]);
            }
            let (code, text, err) = run(&a);
            assert_eq!(code, 0, "window {from:?}: {text} {err}");
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["summary"]["verify"], "pass");
            assert!(v["divergences"].as_array().unwrap().is_empty());
            assert!(v["summary"]["snapshots_checked"].as_u64().unwrap() >= 2);
        }
        assert_eq!(std::fs::read(&f.file).unwrap(), before);
    }
}
