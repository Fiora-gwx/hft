//! The strategy set (TESTS.md "策略集": SEQ-015–SEQ-017).

use hft_event::ControlCmd;
use hft_sequencer::driver::live::RingId;
use hft_types::{AccountId, GroupId, StrategyId};

use crate::common::*;

#[test]
fn seq_015_instance_created_on_first_params() {
    reset_script();
    let mut b = b0();
    b.params.retain(|p| p.strategy != S2);
    let mut h = b.boot();
    h.steady();
    clear_log();
    h.quote(A1, "50000", "50002");
    h.run();
    assert!(h.s.instances().get(S2).is_some_and(|s| s.inst.is_none()));
    assert!(!log().iter().any(|g| g.s == S2));
    let n0 = calls().iter().filter(|c| c.0 == S2 && c.1 == "create").count();
    let id = h.set_params(&Spb::s2(1));
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0));
    assert_eq!(
        calls().iter().filter(|c| c.0 == S2 && c.1 == "create").count(),
        n0 + 1
    );
    let mut p = Spb::s2(2);
    p.values[F_THR] = 7;
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0));
    assert_eq!(
        calls().iter().filter(|c| c.0 == S2 && c.1 == "create").count(),
        n0 + 1
    );
    assert_eq!(
        calls().iter().filter(|c| c.0 == S2 && c.1 == "on_params").count(),
        1
    );
}

#[test]
fn seq_016_dispatch_order_live_first() {
    reset_script();
    let mut b = b0();
    // s3 on Live account 3, s1 on Sim account 9, s2 on Live account 2.
    let mut a3 = b.rd.accounts[2].clone();
    a3.bound_instance = StrategyId(3);
    a3.venues = vbits(&[A, B]);
    b.rd.accounts[2] = a3;
    b.rd.accounts[8].bound_instance = StrategyId(1);
    b.rd.accounts[0].bound_instance = StrategyId(0);
    b.rd.accounts[0].venues = 0;
    b.rd.instances[8].kind_id = TAKER;
    b.params = vec![
        Spb::new(
            StrategyId(3),
            AccountId(3),
            TAKER,
            1,
            vec![gspec(GroupId(0x0300_0001))],
        ),
        Spb::new(S1, AC9, TAKER, 1, vec![gspec(G1)]),
        Spb::s2(1),
    ];
    let mut h = b.boot();
    let ord: Vec<StrategyId> =
        h.s.instances()
            .dispatch_order
            .iter()
            .map(|ix| h.s.instances().slots[usize::from(*ix)].id)
            .collect();
    assert_eq!(&ord[..3], &[S2, StrategyId(3), S1]);
    h.step();
    clear_log();
    h.quote(A1, "50000", "50002");
    h.run();
    let got: Vec<StrategyId> = log()
        .iter()
        .filter(|g| g.kind == "Bbo" && !g.model)
        .map(|g| g.s)
        .collect();
    assert_eq!(got, vec![S2, StrategyId(3), S1]);
}

#[test]
fn seq_017_desc_version_in_section5() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let n = h.snapshots().len();
    h.cmd(AC1, &ControlCmd::SnapshotNow);
    h.run();
    h.run();
    let snaps = h.snapshots();
    assert!(snaps.len() > n);
    let (hdr, secs) = snap(snaps.last().unwrap());
    assert_eq!(hdr.purpose, 3);
    let (r, g, _) = secs
        .strategies
        .iter()
        .find(|x| x.0.strategy == S1)
        .expect("s1 row");
    assert_eq!(r.account, AC1);
    assert_eq!(r.kind_id, TAKER);
    assert_eq!(r.desc_version, 3);
    assert_eq!(r.params_version, 1);
    assert_eq!(r.n_groups, 1);
    assert_eq!(g[0].group, G1);
    let _ = RingId::Ctl;
}
