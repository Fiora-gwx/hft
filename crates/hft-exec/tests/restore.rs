//! EXE-509: `recovery_end` takes `stop_tsc` from the snapshot record header.
use crate::common;
use common::*;
use hft_types::Tsc;

#[test]
fn exe_509_stop_tsc_from_record_header() {
    const REC_TSC: u64 = 4_000_000_000_000;
    const HZ: u64 = 1_000_000_000_000;
    let n = Tsc(1_000_000_000);
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.cm(A1, G7).ext_deadline = Tsc(REC_TSC + 2_000_000_000);
    let s = fx.snap();
    let mut ex = fx.restore(&s);
    ex.recovery_end(n, Tsc(REC_TSC), HZ, HZ);
    let d = ex.cell(A1, G7).unwrap().ext_deadline;
    assert_eq!(d, Tsc(n.0 + 2_000_000_000));
    assert_ne!(d, Tsc(n.0 + 2_001_000_000));
}
