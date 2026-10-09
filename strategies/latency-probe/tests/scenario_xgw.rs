//! Cross-gw quote variant (TESTS.md LPR-028–LPR-032).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use common::*;
use hft_strategy_sdk::{OrderRef, OrderUpdateKind};

/// Leg 0 (I2) probe placed at +0; Acked at +50 ms when `ack`.
fn leg0(h: &mut H, ack: bool) -> OrderRef {
    let rec = h.bbo(I2);
    assert_eq!(rec.places.len(), 1);
    assert_eq!(rec.places[0].inst, I2);
    let r0 = rec.refs[0];
    if ack {
        h.at(50);
        let rec = h.order(GX, r0, probe_tag(0), acked());
        assert!(rec.cancels.is_empty());
    }
    r0
}

#[test]
fn lpr_028_leg0_ack_kept() {
    let mut h = H::new(&[], vec![gx()]);
    leg0(&mut h, true);
}

#[test]
fn lpr_029_leg1_needs_acked_leg0() {
    for (row, state, places) in [("LPR-029.1", 0, 0), ("LPR-029.2", 1, 0), ("LPR-029.3", 2, 1)] {
        let mut h = H::new(&[], vec![gx()]);
        if state > 0 {
            leg0(&mut h, state == 2);
        }
        h.at(100);
        let rec = h.bbo(I3);
        assert_eq!(rec.places.len(), places, "{row}");
        if places == 1 {
            assert_eq!(rec.places[0].inst, I3, "{row}");
        }
    }
}

#[test]
fn lpr_030_leg1_ack_cancels_both() {
    for (row, aged) in [("LPR-030.1", false), ("LPR-030.2", true)] {
        let mut h = H::new(&[], vec![gx()]);
        let r0 = leg0(&mut h, true);
        h.at(100);
        let r1 = h.bbo(I3).refs[0];
        if aged {
            h.at(120);
            h.order(GX, r0, probe_tag(0), OrderUpdateKind::Aged);
        }
        h.at(150);
        let rec = h.order(GX, r1, probe_tag(1), acked());
        let got: Vec<OrderRef> = rec.cancels.iter().map(|c| c.r).collect();
        let want = if aged { vec![r1] } else { vec![r1, r0] };
        assert_eq!(got, want, "{row}");
    }
}

#[test]
fn lpr_031_leg0_aged_next_round() {
    let mut h = H::new(&[], vec![gx()]);
    let r0 = leg0(&mut h, true);
    h.at(2_050);
    h.order(GX, r0, probe_tag(0), OrderUpdateKind::Aged);
    h.at(2_100);
    let rec = h.bbo(I2);
    assert_eq!(rec.places.len(), 1);
    assert_eq!(rec.places[0].inst, I2);
}

#[test]
fn lpr_032_leg2_plain() {
    let mut h = H::new(&[], vec![gx3()]);
    let rec = h.bbo(I1);
    assert_eq!(rec.places.len(), 1);
    assert_eq!(rec.places[0].inst, I1);
    let r2 = rec.refs[0];
    h.at(50);
    let rec = h.order(GX3, r2, probe_tag(2), acked());
    assert_eq!(rec.cancels.iter().map(|c| c.r).collect::<Vec<_>>(), vec![r2]);
}
