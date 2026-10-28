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

#[test]
fn core_437_utc_anchor_cadence() {
    use ClockVerdict::*;
    let pair = |k: i64| CalibPair {
        tsc: hft_rt::Tsc(1_000 + k as u64 * 999_970_000),
        realtime_ns: 1_791_072_000_000_000_000 + k * 1_000_000_000,
        phc_ns: 0,
        err_ns: 35,
    };
    // Which ticks of a verdict sequence send a `UtcAnchor` (every = 3).
    let run = |verdicts: &[ClockVerdict]| -> Vec<(usize, hft_event::UtcAnchor)> {
        let (mut tx, mut rx) = hft_core::runtime::ring(64).unwrap();
        let mut st = hft_core::runtime::AnchorTick::new(3);
        let mut out = Vec::new();
        for (k, v) in verdicts.iter().enumerate() {
            hft_core::runtime::anchor_tick(&mut tx, pair(k as i64), *v, Some(999_970_000_000), &mut st)
                .unwrap();
            while let Some(m) = rx.try_pop() {
                match m {
                    HkMsg::UtcAnchor(a) => out.push((k, a)),
                    _ => panic!("unexpected housekeeping input"),
                }
            }
        }
        out
    };
    let ticks = |v: &[ClockVerdict]| run(v).iter().map(|x| x.0).collect::<Vec<_>>();
    assert_eq!(ticks(&[Synced; 7]), vec![0, 3, 6], "CORE-437.1");
    assert_eq!(
        ticks(&[Unsynced, Unsynced, Synced, Synced, Synced, Synced]),
        vec![2, 5],
        "CORE-437.2"
    );
    assert_eq!(
        ticks(&[
            Synced,
            Synced,
            Stepped { jump_ns: 60_000 },
            Unsynced,
            Synced,
            Synced
        ]),
        vec![0, 4],
        "CORE-437.3"
    );
    let (k, a) = run(&[Synced])[0];
    let p = pair(k as i64);
    assert_eq!(
        (a.tsc, a.realtime_ns, a.hz_milli, a.err_ns),
        (p.tsc, p.realtime_ns, 999_970_000_000, 35),
        "CORE-437.4"
    );
    // Cadence from the configuration: whole calibration ticks, rounded up.
    let clock = |interval_ms, anchor_interval_ms| hft_core::config::Clock {
        step_threshold_ns: 50_000,
        interval_ms,
        anchor_interval_ms,
    };
    assert_eq!(clock(1_000, 10_000).anchor_every(), 10, "CORE-437.5");
    assert_eq!(clock(1_000, 2_500).anchor_every(), 3, "CORE-437.5");
    assert_eq!(clock(1_000, 1).anchor_every(), 1, "CORE-437.5");
}

#[test]
fn core_438_live_io_clock() {
    use hft_core::{
        io,
        runtime::{byte_ring, ring},
    };
    use hft_sequencer::driver::{LiveIo, RingId};
    use hft_types::Tsc;
    use zerocopy::{FromZeros, IntoBytes};
    type S = hft_core::Strategies;
    const R0: i64 = 1_791_072_000_000_000_000;
    let hz = 1_000_000_000_000u64;
    let base = CalibPair {
        tsc: Tsc(1_000),
        realtime_ns: R0,
        ..Default::default()
    };
    let clock = hft_rt::TscClock::from_hz_milli(hz, base);
    let (_ctl_tx, ctl_rx) = byte_ring(1 << 16).unwrap();
    let (mut hk_tx, hk_rx) = ring(16).unwrap();
    let (_sim_tx, sim_rx) = byte_ring(1 << 16).unwrap();
    let (sim_orders, _sim_orders_rx) = ring(16).unwrap();
    let (_parsed_tx, parsed_rx) = Box::leak(Box::new(hft_rt::Mailbox::new())).split();
    let (retired_tx, _retired_rx) = Box::leak(Box::new(hft_rt::Mailbox::new())).split();
    let mut env = io::Io {
        startup: Default::default(),
        journal: hft_sequencer::MemJournal::new(hft_sequencer::SegClock {
            tsc_hz_milli: hz,
            calib_tsc: 1_000,
            calib_realtime_ns: R0,
        }),
        clock,
        ctl: io::Inputs::new(ctl_rx),
        hk: hk_rx,
        sim_reports: io::Inputs::new(sim_rx),
        sim_orders,
        feeds: vec![],
        gws: vec![],
        parsed: parsed_rx,
        retired: retired_tx,
        retired_pending: None,
        clock_status: hft_sequencer::snapshot::clock_status(base, hz),
        profile: hft_sequencer::snapshot::host_profile(hz),
        orders: 0,
        cancels: 0,
        reports: 0,
    };
    let mut slot = hft_sequencer::InputSlot::with_capacity(4096);
    // .1 Before any housekeeping status: the boot pair.
    assert_eq!(
        LiveIo::<S>::utc_of(&env, Tsc(1_000 + 5_000_000_000)),
        R0 + 5_000_000_000,
        "CORE-438.1"
    );
    // .2 A newer ClockStatus (realtime 30 µs ahead of the boot line) re-bases the driver's UTC;
    // the segment calibration pair stays the boot pair.
    let mut c = hft_sequencer::snapshot::clock_status(
        CalibPair {
            tsc: Tsc(1_000 + 1_000_000_000),
            realtime_ns: R0 + 1_000_030_000,
            ..Default::default()
        },
        hz,
    );
    c.state = 1;
    hk_tx.try_push(HkMsg::ClockStatus(c)).unwrap();
    assert!(LiveIo::<S>::pop(&mut env, RingId::Hk, &mut slot), "CORE-438.2");
    assert_eq!(slot.kind, hft_event::EventKind::CLOCK_STATUS, "CORE-438.2");
    assert_eq!(
        LiveIo::<S>::utc_of(&env, Tsc(1_000 + 2_000_000_000)),
        R0 + 2_000_030_000,
        "CORE-438.2"
    );
    assert_eq!(LiveIo::<S>::roll_info(&mut env).calib, base, "CORE-438.2");
    // .3 HkMsg::UtcAnchor becomes the Input UtcAnchor with the same body.
    let mut a = hft_event::UtcAnchor::new_zeroed();
    a.tsc = Tsc(7);
    a.realtime_ns = R0 + 9;
    a.hz_milli = 999_970_000_000;
    a.err_ns = 40;
    hk_tx.try_push(HkMsg::UtcAnchor(a)).unwrap();
    assert!(LiveIo::<S>::pop(&mut env, RingId::Hk, &mut slot), "CORE-438.3");
    assert_eq!(slot.kind, hft_event::EventKind::UTC_ANCHOR, "CORE-438.3");
    assert_eq!(slot.body(), a.as_bytes(), "CORE-438.3");
}
