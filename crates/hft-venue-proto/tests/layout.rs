//! VPR-001: `RxMeta` layout.

use core::mem::{align_of, offset_of, size_of};

use hft_venue_proto::RxMeta;

#[test]
fn vpr_001_rx_meta_layout() {
    // VPR-001.1
    assert_eq!((size_of::<RxMeta>(), align_of::<RxMeta>()), (48, 8), "VPR-001.1");
    // VPR-001.2
    assert_eq!(
        [
            offset_of!(RxMeta, rx_tsc),
            offset_of!(RxMeta, hw_rx_ns),
            offset_of!(RxMeta, sw_rx_ns),
            offset_of!(RxMeta, prev_override),
        ],
        [0, 8, 16, 24],
        "VPR-001.2"
    );
    // VPR-001.3
    assert_eq!(
        [
            offset_of!(RxMeta, conn),
            offset_of!(RxMeta, account),
            offset_of!(RxMeta, _pad)
        ],
        [32, 33, 34],
        "VPR-001.3"
    );
    // VPR-001.4
    assert_eq!(
        [
            offset_of!(RxMeta, ts_flags),
            offset_of!(RxMeta, rx_empty_gap_ns),
            offset_of!(RxMeta, _pad2),
        ],
        [36, 40, 44],
        "VPR-001.4"
    );
}
