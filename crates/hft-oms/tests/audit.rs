//! OMS-463: the final hedge fill and next first leg share one diagnostic Input.
use crate::common::*;
use hft_oms::LinkState;
use hft_types::Tif;

#[test]
fn oms_463_completed_link_reused_in_one_input() {
    let mut h = H::with(|c| c.links_per_account = 1);
    let (first, id) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    let created_seq = h.seq;
    h.fill(first, q("0.012"), p("60000"), 1);
    let req = one_hedge(&h);
    let hedge = place_hedge(&mut h, &req, 1).unwrap();

    h.o.audit_begin();
    h.fill(hedge, q("0.012"), p("60000"), 2);
    assert_eq!(h.link(id).state, LinkState::Done);
    let seq = h.seq;
    let next =
        h.o.place(
            &first_leg(q("0.003"), Tif::Ioc, plan_l1()),
            ms(2),
            seq,
            &mut h.out,
        )
        .unwrap();
    let rows = h.o.audit_retired();
    assert_eq!(rows.len(), 1);
    let old = rows[0];
    assert_eq!(old.link.state, LinkState::Done);
    assert_eq!(old.link.first, first);
    assert_eq!(old.first_epoch, EPOCH as u16);
    assert_eq!(old.created_seq, created_seq);
    assert_eq!(old.link.first_net, q("0.012"));
    assert_eq!(old.link.hedge_req, q("0.012"));
    assert_eq!(old.link.hedge_filled, q("0.012"));
    let current = h.o.audit_link(id).unwrap();
    assert_eq!(current.link.first, next);
    assert_eq!(current.created_seq, seq);
    assert_eq!(current.link.first_net, q("0"));
    assert_ne!(current.link.first, old.link.first);
    assert!(!h.o.audit_overflow());
    h.o.audit_begin();
    assert!(h.o.audit_retired().is_empty());
    assert_eq!(h.o.audit_link(id).unwrap().created_seq, seq);
}
