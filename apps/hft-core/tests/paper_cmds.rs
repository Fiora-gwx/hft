//! CORE-427: `out-sim` commands that project no order decision leave the ring without waiting for
//! an order (`paper::pop_unsequenced`).
use hft_core::paper::pop_unsequenced;
use hft_event::{GwCmd, GwOp};
use hft_types::{AccountId, InstId, Px, VenueId};

fn settle(end_s: u32) -> GwCmd {
    GwCmd::sim_settle(AccountId(1), VenueId::POLYMARKET_CLOB, InstId(3), Px(1000), end_s)
}

fn order(decision_seq: u64) -> GwCmd {
    GwCmd {
        decision_seq,
        op: GwOp::Place,
        ..settle(0)
    }
}

#[test]
fn core_427_unsequenced_commands_do_not_wait_for_an_order() {
    let (mut tx, mut rx) = hft_core::runtime::ring::<GwCmd>(16).unwrap();
    // .1 empty ring.
    assert!(pop_unsequenced(&mut rx).is_none(), "CORE-427.1");
    // .2 two settles queue up: both leave in order.
    tx.try_push(settle(100)).unwrap();
    tx.try_push(settle(200)).unwrap();
    let a = pop_unsequenced(&mut rx).map(|c| c.arg2);
    let b = pop_unsequenced(&mut rx).map(|c| c.arg2);
    assert_eq!((a, b), (Some(100), Some(200)), "CORE-427.2");
    // .3 an order command at the head stays (its decision barrier), and so does what follows it.
    tx.try_push(order(7)).unwrap();
    tx.try_push(settle(300)).unwrap();
    assert!(pop_unsequenced(&mut rx).is_none(), "CORE-427.3");
    assert_eq!(rx.try_pop().map(|c| c.decision_seq), Some(7), "CORE-427.3");
    assert_eq!(pop_unsequenced(&mut rx).map(|c| c.arg2), Some(300), "CORE-427.3");
}
