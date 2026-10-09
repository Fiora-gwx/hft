//! Cold-start recovery (TESTS.md "冷启动恢复": SEQ-256–SEQ-266).

use hft_sequencer::SnapshotSel;

use crate::common::*;

/// B0 steady + a few events; verify from the first snapshot passes.
#[test]
fn verify_smoke() {
    let mut h = b0().boot();
    h.steady();
    h.quote(A1, "50000", "50003");
    h.run();
    let m = h.verify();
    assert!(m.is_empty(), "{m:?}");
    let _ = SnapshotSel::Latest;
}
