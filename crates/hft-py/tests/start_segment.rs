//! `events()` start segment of a processing-time window (TESTS.md PY-170).
use std::collections::BTreeMap;
use std::path::PathBuf;

use hft_journal::SegmentHeaderFixed;
use hft_py::journal::{SegFile, start_segment};
use zerocopy::FromZeros;

fn seg(no: u64, created: i64, present: bool) -> SegFile {
    let mut h = SegmentHeaderFixed::new_zeroed();
    h.segment_no = no;
    h.created_utc_ns = created;
    SegFile {
        no,
        first_seq: no * 100,
        hot: None,
        cold: present.then(|| PathBuf::from(format!("{no:010}.jrn"))),
        sealed: true,
        header: Some(h),
    }
}

#[test]
fn py_170_start_segment_of_a_window() {
    let segs: BTreeMap<u64, SegFile> = [(1, 1_000), (2, 2_000), (3, 3_000), (4, 4_000)]
        .into_iter()
        .map(|(no, t)| (no, seg(no, t, true)))
        .collect();
    // .1 the segment before the newest one created at or before t0.
    assert_eq!(start_segment(&segs, 3_500), Some(2), "PY-170.1");
    assert_eq!(start_segment(&segs, 4_000), Some(3), "PY-170.1");
    assert_eq!(start_segment(&segs, 9_999), Some(3), "PY-170.1");
    // .2 fewer than two segments qualify: the journal start.
    assert_eq!(start_segment(&segs, 1_500), None, "PY-170.2");
    assert_eq!(start_segment(&segs, 500), None, "PY-170.2");
    // .3 a segment without a file is not a start.
    let mut gone = segs.clone();
    gone.insert(2, seg(2, 2_000, false));
    assert_eq!(start_segment(&gone, 3_500), Some(1), "PY-170.3");
}
