//! VEN-001: `InstSeq` layout.

use core::mem::{align_of, offset_of, size_of};

use hft_venue::InstSeq;

#[test]
fn ven_001_inst_seq_layout() {
    assert_eq!(
        (size_of::<InstSeq>(), align_of::<InstSeq>()),
        (64, 64),
        "VEN-001.1"
    );
    assert_eq!(
        (offset_of!(InstSeq, last_fwd), size_of::<[u64; 6]>()),
        (0, 48),
        "VEN-001.2"
    );
    assert_eq!(
        [
            offset_of!(InstSeq, l2_state),
            offset_of!(InstSeq, resync_slot),
            offset_of!(InstSeq, _pad),
            offset_of!(InstSeq, resync_n),
            offset_of!(InstSeq, _pad2),
        ],
        [48, 49, 50, 52, 56],
        "VEN-001.3"
    );
}
