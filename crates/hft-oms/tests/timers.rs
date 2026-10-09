#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{OrderAction, OrderEventKind};
use hft_oms::*;

#[test]
fn oms_024_timer_id_roundtrip() {
    assert_eq!(OmsTimerReq::unknown_query_id(0, 1), 256, "OMS-024.1");
    assert_eq!(OmsTimerReq::split_unknown_query_id(256), (0, 1), "OMS-024.1");
    assert_eq!(
        OmsTimerReq::unknown_query_id(255, 65_535),
        16_777_215,
        "OMS-024.2"
    );
    assert_eq!(
        OmsTimerReq::split_unknown_query_id(16_777_215),
        (255, 65_535),
        "OMS-024.2"
    );
    assert_eq!(OmsTimerReq::unknown_query_id(3, 2), 515, "OMS-024.3");
    // .3: id 515 acts on slot 2 of (VX, a1).
    let mut h = H::new();
    assert_eq!(h.o.table_view(VX, A1).unwrap().table_ix, 3);
    let _t1 = h.place_ok(&sell_y(), 0);
    let t2 = h.place_ok(&sell_y(), 0);
    assert_eq!(t2.slot, 2);
    h.what(t2, OrderEventKind::OutcomeUnknown, 1);
    h.out.clear();
    h.o.on_unknown_query(515, ms(1000), 1, &mut h.out);
    let q = h.decisions(OrderAction::Query);
    assert_eq!(q.len(), 1, "OMS-024.3");
    assert_eq!((q[0].venue, q[0].target_slot.0), (VX, 2), "OMS-024.3");
    // .4 HedgeRetry id = link_id (round trip through on_hedge_retry of a missing link: no output).
    h.retry(9, 2000);
    assert!(h.out.is_empty(), "OMS-024.4");
    assert_eq!(OmsTimerReq::smoke_id(A1, VX), 1281, "OMS-024.5");
    assert_eq!(OmsTimerReq::split_smoke_id(1281), (A1, VX), "OMS-024.5");
}
