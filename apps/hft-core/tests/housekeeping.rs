use hft_event::HkMsg;
use hft_rt::{CalibPair, ClockVerdict};

#[test]
fn core_418_clock_tick_outputs() {
    use ClockVerdict::*;
    for (verdicts, states, expected) in [
        ([Synced, Synced, Synced], [1, 1, 1], 0),
        ([Synced, Stepped { jump_ns: 60000 }, Synced], [1, 3, 1], 2),
        (
            [Stepped { jump_ns: 60000 }, Stepped { jump_ns: -80000 }, Unsynced],
            [3, 3, 2],
            3,
        ),
    ] {
        let (mut tx, mut rx) = hft_core::runtime::ring(16).unwrap();
        let mut previous = Some(1);
        for verdict in verdicts {
            hft_core::runtime::clock_tick(
                &mut tx,
                CalibPair::default(),
                1_000_000_000_000,
                verdict,
                &mut previous,
            )
            .unwrap();
        }
        let mut observed = vec![];
        let mut decisions = 0;
        while let Some(msg) = rx.try_pop() {
            match msg {
                HkMsg::ClockStatus(status) => observed.push(status.state),
                HkMsg::Sys { .. } => decisions += 1,
                _ => panic!("unexpected housekeeping input"),
            }
        }
        assert_eq!(observed, states);
        assert_eq!(decisions, expected);
    }
}

#[test]
fn core_382_clock_observations_are_never_coalesced() {
    let (mut tx, mut rx) = hft_core::runtime::ring(32).unwrap();
    let mut previous = Some(1);
    for second in 1..=10 {
        hft_core::runtime::clock_tick(
            &mut tx,
            CalibPair {
                realtime_ns: second * 1_000_000_000,
                ..Default::default()
            },
            1_000_000_000_000,
            ClockVerdict::Synced,
            &mut previous,
        )
        .unwrap();
    }
    for second in 1..=10 {
        let Some(HkMsg::ClockStatus(status)) = rx.try_pop() else {
            panic!("missing clock observation")
        };
        assert_eq!(status.realtime_ns, second * 1_000_000_000);
    }
    assert!(rx.try_pop().is_none());
}
