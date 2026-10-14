use crate::support::mempeer::{self, Opts};
use hft_net::*;

fn handoff(id: u8) -> Handoff {
    Handoff::test_from_conn(
        mempeer::pair_with(Opts {
            id: ConnId(id),
            ..Opts::default()
        })
        .0,
    )
}
fn pool() -> ConnPool<3> {
    ConnPool::new(
        PoolMode::PrimaryStandby,
        2,
        RotatePolicy {
            stagger_tsc: 3_600_000_000_000,
        },
    )
}
fn installed(mode: PoolMode) -> ConnPool<3> {
    let mut p = ConnPool::new(mode, 2, RotatePolicy::default());
    p.install(handoff(7), None).unwrap();
    p.install(handoff(3), None).unwrap();
    p
}
#[test]
fn net_240_fill_then_pending() {
    let mut p = pool();
    assert_eq!(p.next_build(0), Some(BuildWhy::Fill));
    assert_eq!(p.next_build(0), None);
    assert_eq!(p.install(handoff(7), None), Ok(ConnId(7)));
    assert_eq!(p.next_build(0), Some(BuildWhy::Fill));
}
#[test]
fn net_241_build_failed_clears() {
    let mut p = pool();
    p.next_build(0);
    assert_eq!(p.next_build(0), None);
    p.build_failed();
    assert_eq!(p.next_build(0), Some(BuildWhy::Fill));
}
#[test]
fn net_242_full_pool() {
    let mut p = installed(PoolMode::PrimaryStandby);
    p.install(handoff(8), None).unwrap();
    assert_eq!(p.next_build(0), None);
    assert_eq!(p.install(handoff(9), None), Err(PoolErr::Full));
}
#[test]
fn net_243_install_not_open() {
    let (mut c, _) = mempeer::pair();
    c.test_set_phase(ConnPhase::Dead);
    let mut p = pool();
    assert_eq!(p.install(Handoff::test_from_conn(c), None), Err(PoolErr::NotOpen));
    assert_eq!(p.active_count(), 0);
}
#[test]
fn net_244_primary_then_standby() {
    let mut p = installed(PoolMode::PrimaryStandby);
    assert_eq!(p.primary().unwrap().id(), ConnId(7));
    assert_eq!(p.standby().unwrap().id(), ConnId(3));
    assert_eq!(
        p.active().map(|c| c.id()).collect::<Vec<_>>(),
        [ConnId(7), ConnId(3)]
    );
}
#[test]
fn net_245_swap_indices_only() {
    let mut p = installed(PoolMode::PrimaryStandby);
    let before: Vec<_> = [7, 3]
        .map(|id| {
            let c = p.get(ConnId(id)).unwrap();
            (*c.sys(), c.test_tx_seq(), c.test_rx_range())
        })
        .into();
    assert_eq!(p.swap_primary(), Ok((ConnId(3), ConnId(7))));
    for (id, expected) in [7, 3].into_iter().zip(before) {
        let c = p.get(ConnId(id)).unwrap();
        assert_eq!((*c.sys(), c.test_tx_seq(), c.test_rx_range()), expected);
    }
}
#[test]
fn net_246_swap_errors() {
    let mut p = pool();
    p.install(handoff(7), None).unwrap();
    assert_eq!(p.swap_primary(), Err(PoolErr::NoStandby), "NET-246.1");
    p.install(handoff(3), None).unwrap();
    p.standby().unwrap().test_set_phase(ConnPhase::Dead);
    assert_eq!(p.swap_primary(), Err(PoolErr::NoStandby), "NET-246.2");
    assert_eq!(
        installed(PoolMode::Race).swap_primary(),
        Err(PoolErr::WrongMode),
        "NET-246.3"
    );
}
#[test]
fn net_247_retire_primary_promotes() {
    let mut p = installed(PoolMode::PrimaryStandby);
    let r = p.retire(ConnId(7), ErrBits::RESET).unwrap();
    assert_eq!(r.id(), ConnId(7));
    assert!(r.why.contains(ErrBits::RESET));
    assert_eq!(p.primary().unwrap().id(), ConnId(3));
    assert!(p.standby().is_none());
    assert_eq!(p.next_build(0), Some(BuildWhy::Fill));
}
#[test]
fn net_248_retire_unknown() {
    let mut p = installed(PoolMode::PrimaryStandby);
    assert!(matches!(
        p.retire(ConnId(42), ErrBits::RESET),
        Err(PoolErr::UnknownConn)
    ));
    assert_eq!(p.active_count(), 2);
    assert_eq!(p.primary().unwrap().id(), ConnId(7));
}
#[test]
fn net_249_last_retired_refills() {
    let mut p = ConnPool::<3>::new(PoolMode::Race, 2, RotatePolicy::default());
    p.install(handoff(7), None).unwrap();
    p.retire(ConnId(7), ErrBits::PEER_EOF).unwrap();
    assert_eq!(p.active_count(), 0);
    assert_eq!(p.next_build(0), Some(BuildWhy::Fill));
}
#[test]
fn net_251_rotation_boundary() {
    for now in [999_999, 1_000_000, 1_000_001] {
        let mut p = pool();
        p.install(handoff(7), Some(1_000_000)).unwrap();
        assert_eq!(p.rotate_due(now), (now >= 1_000_000).then_some(ConnId(7)));
        assert_eq!(
            p.get(ConnId(7)).unwrap().err().contains(ErrBits::ROTATE_DUE),
            now >= 1_000_000
        );
        assert_eq!(
            p.next_build(now),
            Some(if now >= 1_000_000 {
                BuildWhy::Rotate(ConnId(7))
            } else {
                BuildWhy::Fill
            })
        );
    }
}
#[test]
fn net_254_race_both_active() {
    let mut p = installed(PoolMode::Race);
    assert_eq!(p.active().count(), 2);
    p.retire(ConnId(7), ErrBits::PEER_EOF).unwrap();
    assert_eq!(p.active().map(|c| c.id()).collect::<Vec<_>>(), [ConnId(3)]);
}
