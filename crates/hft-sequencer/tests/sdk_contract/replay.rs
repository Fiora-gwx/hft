//! Replay never runs the Model (SDK-058).

use hft_event::{AdviceHdr, EventKind};
use hft_sequencer::driver::live::RingId;
use zerocopy::{FromZeros, IntoBytes};

use super::*;
use crate::common::*;

#[test]
fn sdk_058_replay_never_runs_model() {
    let mut h = rig();
    h.keyframe();
    for k in 0..50u64 {
        let mut hd = AdviceHdr::new_zeroed();
        hd.strategy = S1;
        hd.account = AC1;
        hd.local_kind = 1;
        hd.group = if k % 2 == 0 { GT1 } else { GT2 };
        hd.len = 8;
        hd.basis_seq = h.mark() as u64;
        let mut rec = hd.as_bytes().to_vec();
        rec.extend_from_slice(&k.to_le_bytes());
        h.push_bytes(RingId::Advice, EventKind::ADVICE, rec);
        h.run();
    }
    let live: Vec<String> = fast(S1)
        .into_iter()
        .filter(|g| g.kind == "Advice")
        .map(|g| g.text)
        .collect();
    assert_eq!(live.len(), 50);
    clear_log();
    let c0 = calls().len();
    let m = h.verify();
    assert!(m.is_empty(), "{m:?}");
    let replayed: Vec<String> = fast(S1)
        .into_iter()
        .filter(|g| g.kind == "Advice")
        .map(|g| g.text)
        .collect();
    assert_eq!(replayed, live);
    // No Model call of any kind during the verify replay.
    assert!(!log().iter().any(|g| g.model));
    assert!(
        !calls()
            .into_iter()
            .skip(c0)
            .any(|c| c.1.starts_with("model") || c.1 == "create_model")
    );
}
