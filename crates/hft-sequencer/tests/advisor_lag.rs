//! `AdvisorLag` (crates/hft-event/TESTS.md EVT-080–EVT-084; the definition is in hft-event, the
//! writer is this crate's advisor).

use hft_event::{EventKind, Note, NoteSource};
use hft_sequencer::advisor::lag_note;
use hft_sequencer::driver::live::RingId;
use hft_types::StrategyId;
use zerocopy::IntoBytes;

use crate::common::*;

const S3: StrategyId = StrategyId(3);
const S4: StrategyId = StrategyId(4);
const LAG_CODE: u32 = 3;

#[test]
fn evt_080_threshold_boundary() {
    for (lag, want) in [
        (4_999_999_999i64, None),
        (5_000_000_000, None),
        (5_000_000_001, Some(5_000u64)),
        (12_345_678_901, Some(12_345)),
    ] {
        let mut last = Vec::new();
        let n = lag_note(S3, lag, 100_000_000_000, 1_000_500, 1_000_000, 5_000, &mut last);
        match want {
            None => assert!(n.is_none(), "{lag}"),
            Some(ms) => {
                let n = n.expect("note");
                assert_eq!(n.source, NoteSource::Advisor);
                assert_eq!(n.code, hft_event::note_code(NoteSource::Advisor, LAG_CODE));
                assert_eq!(n.level, hft_event::NoteLevel::P1);
                assert_eq!(n.strategy, S3);
                assert_eq!(n.v[0], 3);
                assert_eq!(n.v[1], ms);
                assert_eq!(n.v[2], 500);
                assert_eq!(n.v[3], 1_000_000);
            }
        }
    }
}

#[test]
fn evt_081_rate_limit_per_instance() {
    let t0 = 1_000_000_000_000i64;
    let mut last = Vec::new();
    let mut got3 = Vec::new();
    let mut got4 = Vec::new();
    for k in 0..=250i64 {
        let ens = t0 + k * 100_000_000;
        if lag_note(S3, 8_000_000_000, ens, 10, 1, 5_000, &mut last).is_some() {
            got3.push(ens - t0);
        }
        if lag_note(S4, 8_000_000_000, ens, 10, 1, 5_000, &mut last).is_some() {
            got4.push(ens - t0);
        }
    }
    assert_eq!(got3, vec![0, 10_000_000_000, 20_000_000_000]);
    assert_eq!(got4, got3);
}

#[test]
fn evt_082_no_note_below_threshold() {
    let t0 = 1_000_000_000_000i64;
    let mut last = Vec::new();
    let mut got = Vec::new();
    for k in 0..=320i64 {
        let ens = t0 + k * 100_000_000;
        let lag = if k < 10 {
            8_000_000_000
        } else if k < 310 {
            4_000_000_000
        } else {
            9_000_000_000
        };
        if lag_note(S3, lag, ens, 10, 1, 5_000, &mut last).is_some() {
            got.push(ens - t0);
        }
    }
    assert_eq!(got, vec![0, 31_000_000_000]);
}

#[test]
fn evt_083_engine_ns_across_restart() {
    // The previous run's last consumed record: engine_ns 100 s; this run's committed end: 106 s
    // (EngineClock.base = 100 s, so the hour of downtime is not counted).
    let mut last = Vec::new();
    let consumed_ens = 100_000_000_000i64;
    let end_ens = 106_000_000_000i64;
    let n = lag_note(S3, end_ens - consumed_ens, end_ens, 2, 1, 5_000, &mut last).expect("note");
    assert_eq!(n.v[1], 6_000);
}

#[test]
fn evt_084_sequencer_never_writes_advisor_lag() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    // The advisor's AdvisorLag Notes arrive through the advice ring (Meta) while the run goes on.
    let mut last = Vec::new();
    for k in 0..40u64 {
        h.adv(1_000_000_000);
        let ens = (k as i64 + 1) * 1_000_000_000;
        if let Some(n) = lag_note(S2, 6_000_000_000, ens, 10, 1, 5_000, &mut last) {
            h.push_bytes(RingId::Advice, EventKind::NOTE, n.as_bytes().to_vec());
        }
        h.quote(A1, "50000", "50002");
        h.run();
    }
    let lag = hft_event::note_code(NoteSource::Advisor, LAG_CODE);
    let logged: Vec<Note> = h.recs().iter().filter(|r| r.kind == EventKind::NOTE).filter_map(|r| r.read::<Note>()).filter(|n| n.code == lag).collect();
    assert!(!logged.is_empty());
    assert!(logged.iter().all(|n| n.source == NoteSource::Advisor));
    // Verification replay regenerates every Output and never an AdvisorLag.
    let m = h.verify();
    assert!(m.is_empty(), "{m:?}");
    assert!(!h.recs().iter().any(|r| r.kind == EventKind::NOTE
        && r.role() == hft_event::Role::Output
        && r.read::<Note>().is_some_and(|n| n.code & 0x00FF_FFFF == LAG_CODE && n.source == NoteSource::Sequencer)));
}
