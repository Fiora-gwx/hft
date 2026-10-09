#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::OrderEventKind;
use hft_oms::*;

#[test]
fn oms_005_fifo_free_list() {
    let mut h = H::with(|c| c.slots = PerTable::new(8));
    let rs: Vec<_> = (0..8).map(|_| h.place_ok(&buy_x(), 0)).collect();
    for (i, r) in rs.iter().enumerate() {
        assert_eq!((r.slot, r.r#gen), (i as u32 + 1, 1));
    }
    assert_eq!(h.place(&buy_x(), 0), Err(PlaceError::NoFreeSlot));
    assert!(h.out.is_empty());
    h.what(rs[2], OrderEventKind::CancelAck, 10);
    h.what(rs[4], OrderEventKind::CancelAck, 10);
    h.sweep(5011);
    let a = h.place_ok(&buy_x(), 5012);
    let b = h.place_ok(&buy_x(), 5012);
    assert_eq!((a.slot, a.r#gen), (3, 2));
    assert_eq!((b.slot, b.r#gen), (5, 2));
}

#[test]
fn oms_034_gen_increments() {
    let mut h = H::with(|c| c.slots = PerTable::new(1));
    let mut t = 0;
    for want in 1..=4u32 {
        let r = h.place_ok(&buy_x(), t);
        assert_eq!((r.slot, r.r#gen), (1, want));
        if want == 4 {
            break;
        }
        h.what(r, OrderEventKind::CancelAck, t);
        h.sweep(t + 5001);
        let tb = h.o.tomb(VS, A1, 1, want).unwrap();
        assert_eq!(tb.r#gen, want);
        t += 5002;
    }
}
