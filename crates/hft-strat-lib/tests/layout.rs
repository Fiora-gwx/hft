//! Layouts and codes of the fast-part types (TESTS.md SLB-128, SLB-130).

use core::mem::{align_of, offset_of, size_of};

use hft_strat_lib::{CostItem, CostLine, FireKind, cost_flags};

#[test]
fn slb_128_cost_layout() {
    assert_eq!((size_of::<CostItem>(), align_of::<CostItem>()), (16, 8));
    assert_eq!(
        (
            offset_of!(CostItem, rate),
            offset_of!(CostItem, label),
            offset_of!(CostItem, flags),
            offset_of!(CostItem, _pad)
        ),
        (0, 8, 10, 11)
    );
    assert_eq!((size_of::<CostLine>(), align_of::<CostLine>()), (168, 8));
    assert_eq!(
        (
            offset_of!(CostLine, items),
            offset_of!(CostLine, n),
            offset_of!(CostLine, _pad),
            offset_of!(CostLine, rt_floor)
        ),
        (0, 128, 129, 136)
    );
}

#[test]
fn slb_130_fast_codes() {
    assert_eq!(
        (
            cost_flags::ROUND_TRIP,
            cost_flags::FIXED,
            cost_flags::OUT_OF_QUOTE
        ),
        (1, 2, 4),
        "SLB-130.1"
    );
    assert_eq!(
        [FireKind::Open, FireKind::Close, FireKind::Flatten, FireKind::Hard].map(|k| k as u8),
        [0, 1, 2, 3],
        "SLB-130.2"
    );
}
