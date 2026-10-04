//! Old-version records read by the current reader (EVT-071). Fixtures in
//! `fixtures/hft-event/EVT-071/` (old field values non-zero); `HFT_REGEN_GOLDEN=1` rewrites them.

mod common;

use common::*;
use hft_event::*;
use std::path::PathBuf;
use zerocopy::IntoBytes;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/hft-event/EVT-071")
}

/// `(row, kind, body)`: the canonical body cut to the old length (with the row's byte overrides).
fn rows() -> Vec<(&'static str, EventKind, Vec<u8>)> {
    let cut = |f: &[FieldDesc], n: usize| canonical(f)[..n].to_vec();
    let mut ls2 = cut(LatencySample::FIELDS, 128);
    ls2[121] = 0;
    let mut ls3 = cut(LatencySample::FIELDS, 136);
    ls3[121] = 2;
    let mut cr2 = cut(CmdResult::FIELDS, 144);
    cr2[104..112].fill(0);
    vec![
        (
            "EVT-071.1",
            EventKind::EXTERNAL_FILL,
            cut(ExternalFill::FIELDS, 72),
        ),
        (
            "EVT-071.2",
            EventKind::RECONCILE_REPORT,
            cut(ReconcileReport::FIELDS, 80),
        ),
        (
            "EVT-071.3",
            EventKind::RECONCILE_REPORT,
            cut(ReconcileReport::FIELDS, 96),
        ),
        (
            "EVT-071.4",
            EventKind::LATENCY_SAMPLE,
            cut(LatencySample::FIELDS, 112),
        ),
        ("EVT-071.5", EventKind::LATENCY_SAMPLE, ls2),
        ("EVT-071.6", EventKind::LATENCY_SAMPLE, ls3),
        ("EVT-071.7", EventKind::CMD_RESULT, cr2),
        ("EVT-071.8", EventKind::CONN_STATE, cut(ConnState::FIELDS, 16)),
        (
            "EVT-071.9",
            EventKind::FUNDING_SETTLE,
            cut(FundingSettle::FIELDS, 48),
        ),
    ]
}

fn load(row: &str, kind: EventKind, body: &[u8]) -> Vec<u8> {
    let path = dir().join(format!("{row}.bin"));
    if std::env::var_os("HFT_REGEN_GOLDEN").is_some() {
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(&path, raw_record(kind.0, RecordFlags::EVENT_END.0, 1, 2, body)).unwrap();
    }
    let rec = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(rec.len(), record_len(body.len(), false) as usize, "{row}");
    rec
}

#[test]
fn evt_071_old_versions_zero_fill() {
    for (row, kind, body) in rows() {
        let rec = load(row, kind, &body);
        let r = decode_record(&rec).unwrap();
        // Old fields are read back as written (the record's own body, zero padding included).
        let n = body.len();
        match row {
            "EVT-071.1" => {
                let e = r.read::<ExternalFill>().unwrap();
                assert_eq!(&e.as_bytes()[..n], &body[..], "{row}");
                assert_eq!(e.fill_seq, 0, "{row}");
            }
            "EVT-071.2" | "EVT-071.3" => {
                let x = r.read::<ReconcileReport>().unwrap();
                assert_eq!(&x.as_bytes()[..n], &body[..], "{row}");
                assert_eq!((x.sflags, x.skipped_inflight), (0, 0), "{row}");
                if row == "EVT-071.2" {
                    assert_eq!((x.exch_amount.0, x.engine_amount.0), (0, 0), "{row}");
                } else {
                    assert_ne!(x.exch_amount.0, 0, "{row}");
                    assert_ne!(x.engine_amount.0, 0, "{row}");
                }
            }
            "EVT-071.4" => {
                let x = r.read::<LatencySample>().unwrap();
                assert_eq!(
                    (x.sw_rx_ns, x.link_role, x.action, x.trigger_kind),
                    (0, 0, 0, 0),
                    "{row}"
                );
            }
            "EVT-071.5" => {
                let x = r.read::<LatencySample>().unwrap();
                assert_eq!((x.trigger_kind, x.action), (0, 0), "{row}");
                assert_eq!(&x.as_bytes()[112..121], &body[112..121], "{row}");
            }
            "EVT-071.6" => assert_eq!(r.read::<LatencySample>().unwrap().action, 2, "{row}"),
            "EVT-071.7" => {
                let x = r.read::<CmdResult>().unwrap();
                assert_eq!((x.preview.reset_groups, x.preview.no_open_ms), (0, 0), "{row}");
                assert_eq!(x.as_bytes(), &body[..], "{row}");
            }
            "EVT-071.8" => assert_eq!(r.read::<ConnState>().unwrap().rx_tsc, 0, "{row}"),
            _ => {
                assert_eq!(r.read::<FundingSettle>().unwrap().synth, 0, "{row}");
                assert_eq!(
                    classify(EventKind::FUNDING_SETTLE, r.body),
                    Ok(Role::Input),
                    "{row}"
                );
            }
        }
    }
}
