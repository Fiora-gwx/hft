//! The main loop of the live driver (TESTS.md "主循环": SEQ-055–SEQ-075, SEQ-078).

use hft_event::{ControlCmd, EventKind, Role};
use hft_sequencer::StepOutcome;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::EventRef;
use hft_types::{GroupId, InstId, KillLevel, Tsc};

use crate::common::*;

fn inputs_since(h: &H, m: usize) -> Vec<hft_sequencer::MemRecord> {
    h.since(m)
        .into_iter()
        .filter(|r| r.role() == Role::Input)
        .collect()
}

fn len(h: &mut H, r: RingId) -> usize {
    h.io().ring(r).len()
}

/// A note element of the hk ring (Meta, `Note{Housekeeping}`).
fn hk_note(k: u64) -> Elem {
    let n = hft_event::Note {
        code: hft_event::note_code(hft_event::NoteSource::Housekeeping, 1),
        level: hft_event::NoteLevel::Info,
        source: hft_event::NoteSource::Housekeeping,
        strategy: hft_types::StrategyId(0),
        _pad: 0,
        group: GroupId(0),
        inst: InstId(0),
        v: [k, 0, 0, 0, 0, 0, 0, 0],
    };
    elem(&n)
}

fn ctl_bytes(n: usize) -> Elem {
    Elem {
        kind: EventKind::CONTROL_CMD,
        flags: hft_event::RecordFlags(0),
        body: vec![0u8; n],
    }
}

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

#[test]
fn seq_055_round_a_order() {
    // .1 the first round of a process: EngineClock only.
    reset_script();
    let mut h = b0().boot();
    h.quote(A1, "50000", "50002");
    let m = h.mark();
    h.step();
    let k: Vec<EventKind> = inputs_since(&h, m).iter().map(|r| r.kind).collect();
    assert_eq!(k, vec![EventKind::ENGINE_CLOCK]);
    // .2 with a stop marker: EngineClock, Kill.
    reset_script();
    let mut b = b0();
    b.live.stop_kill = Some((
        KillLevel::Global,
        scope(hft_types::StrategyId(0), hft_types::AccountId(0), GroupId(0)),
    ));
    let mut h = b.boot();
    h.quote(A1, "50000", "50002");
    let m = h.mark();
    h.step();
    h.step();
    let k: Vec<EventKind> = inputs_since(&h, m).iter().map(|r| r.kind).take(2).collect();
    assert_eq!(k, vec![EventKind::ENGINE_CLOCK, EventKind::CONTROL_CMD]);
}

/// Registers `n` strategy timers of s1 due at `at` (tags 1..=n; ≤ 8 per event, the Ctx capacity).
fn timers(h: &mut H, n: u32, at: u64) {
    let mut tag = 1u32;
    while tag <= n {
        let from = tag;
        let to = (tag + 7).min(n);
        set_hook(move |hx, ev, ctx| {
            if let EventRef::Bbo { inst, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
            {
                for t in from..=to {
                    let _ = ctx.timer_at(Tsc(at), t, G1);
                }
            }
        });
        h.quote(A1, "50000", "50003");
        h.run();
        clear_hook();
        tag = to + 1;
    }
}

#[test]
fn seq_056_due_timers_per_round() {
    let mut h = steady();
    let t = h.now();
    timers(&mut h, 20, t + 1_000);
    h.at(t + 2_000);
    let (c, _) = h.cmd_elem("cli:test", 4, AC1, &ControlCmd::SnapshotNow, |_| {});
    h.push_e(RingId::Ctl, c);
    let tt = h.now();
    h.push(
        RingId::Rep(0),
        &conn(
            A,
            1,
            hft_event::ConnStateKind::Alive,
            hft_event::ConnRole::Md,
            InstId(0),
            hft_types::AccountId(0),
            tt,
        ),
    );
    let m = h.mark();
    // Every take moves time past the timers again (they are all due already).
    h.step();
    let inp = inputs_since(&h, m);
    let tm: Vec<u32> = inp
        .iter()
        .filter(|r| r.kind == EventKind::TIMER)
        .filter_map(|r| r.read::<hft_event::Timer>())
        .filter(|t| t.owner >= 16)
        .map(|t| t.timer_id)
        .collect();
    assert_eq!(tm, (1..=16).collect::<Vec<u32>>());
    assert!(inp.iter().any(|r| r.kind == EventKind::CONTROL_CMD));
    assert_eq!(len(&mut h, RingId::Rep(0)), 1);
    let m = h.mark();
    h.step();
    let inp = inputs_since(&h, m);
    let tm: Vec<u32> = inp
        .iter()
        .filter(|r| r.kind == EventKind::TIMER)
        .filter_map(|r| r.read::<hft_event::Timer>())
        .filter(|t| t.owner >= 16)
        .map(|t| t.timer_id)
        .collect();
    assert_eq!(tm, vec![17, 18, 19, 20]);
    let p_last_timer = inp
        .iter()
        .rposition(|r| {
            r.kind == EventKind::TIMER && r.read::<hft_event::Timer>().is_some_and(|t| t.owner >= 16)
        })
        .unwrap();
    let p_rep = inp.iter().position(|r| r.kind == EventKind::CONN_STATE).unwrap();
    assert!(p_rep > p_last_timer);
}

#[test]
fn seq_057_ctl_hk_per_round() {
    let mut h = steady();
    for _ in 0..6 {
        let (c, _) = h.cmd_elem("cli:test", 4, AC1, &ControlCmd::SnapshotNow, |_| {});
        h.push_e(RingId::Ctl, c);
        h.push_e(RingId::Hk, hk_note(1));
    }
    h.step();
    assert_eq!(len(&mut h, RingId::Ctl), 2);
    assert_eq!(len(&mut h, RingId::Hk), 2);
}

#[test]
fn seq_058_ctl_bytes_per_round() {
    for (sizes, want) in [
        (vec![4_096usize, 4_096, 4_096], 2usize),
        (vec![4_096, 4_097], 1),
        (vec![4_096, 4_095, 1], 3),
        (vec![9_000], 1),
        (vec![100; 6], 4),
    ] {
        let mut h = steady();
        let n = sizes.len();
        for s in sizes.iter() {
            h.push_e(RingId::Ctl, ctl_bytes(*s));
        }
        h.step();
        assert_eq!(n - len(&mut h, RingId::Ctl), want, "{sizes:?}");
    }
}

fn alive(v: hft_types::VenueId, k: u64) -> hft_event::ConnState {
    conn(
        v,
        1,
        hft_event::ConnStateKind::Alive,
        hft_event::ConnRole::Md,
        InstId(0),
        hft_types::AccountId(0),
        k,
    )
}

#[test]
fn seq_059_rep_round_limits() {
    // .1
    let mut h = steady();
    for k in 0..40 {
        h.push(RingId::Rep(0), &alive(A, k));
    }
    h.step();
    assert_eq!(40 - len(&mut h, RingId::Rep(0)), 32);
    // .2
    let mut h = steady();
    for k in 0..40 {
        h.push(RingId::Rep(0), &alive(A, k));
        h.push(RingId::Rep(1), &alive(B, k));
    }
    h.step();
    assert_eq!(40 - len(&mut h, RingId::Rep(0)), 32);
    assert_eq!(40 - len(&mut h, RingId::Rep(1)), 32);
    // .3 three rings: 64 in total, ≤ 32 each.
    let mut h = steady();
    h.io().n_rep = 3;
    for k in 0..40 {
        h.push(RingId::Rep(0), &alive(A, k));
        h.push(RingId::Rep(1), &alive(B, k));
        h.push(RingId::Rep(2), &alive(B, k));
    }
    h.step();
    let taken: Vec<usize> = (0..3).map(|r| 40 - len(&mut h, RingId::Rep(r))).collect();
    assert_eq!(taken.iter().sum::<usize>(), 64);
    assert!(taken.iter().all(|t| *t <= 32));
    // .4 chain-in shares the total.
    let mut h = steady();
    h.io().chain = true;
    for k in 0..30 {
        h.push(RingId::Rep(0), &alive(A, k));
    }
    for k in 0..40 {
        h.push(RingId::Chain, &alive(B, k));
    }
    h.step();
    assert_eq!(len(&mut h, RingId::Rep(0)), 0);
    assert_eq!(40 - len(&mut h, RingId::Chain), 32);
    // .5 ctl is not limited by rep.
    let mut h = steady();
    for k in 0..63 {
        h.push(RingId::Rep(0), &alive(A, k));
    }
    let base = h.io().takes;
    let (c, _) = h.cmd_elem("cli:test", 4, AC1, &ControlCmd::SnapshotNow, |_| {});
    h.io().arrivals.push(Arrival {
        after_take: base + 2,
        ring: RingId::Ctl,
        elem: c,
    });
    h.step();
    assert_eq!(63 - len(&mut h, RingId::Rep(0)), 32);
}

#[test]
fn seq_060_bbo_round_limits() {
    let fill = |h: &mut H, ring: u8, n: u64| {
        for k in 0..n {
            let i = if ring.is_multiple_of(2) { A1 } else { B1 };
            let v = inst_venue(i);
            h.push(RingId::Bbo(ring), &bbo(i, v, 0, "50000", "50002", 10_000 + k, 1));
        }
    };
    let mut h = steady();
    fill(&mut h, 0, 50);
    h.step();
    assert_eq!(50 - len(&mut h, RingId::Bbo(0)), 32);
    let mut h = steady();
    h.io().n_bbo = 5;
    for r in 0..5 {
        fill(&mut h, r, 50);
    }
    h.step();
    let taken: Vec<usize> = (0..5).map(|r| 50 - len(&mut h, RingId::Bbo(r))).collect();
    assert_eq!(taken.iter().sum::<usize>(), 128);
    assert!(taken.iter().all(|t| *t <= 32));
    let mut h = steady();
    fill(&mut h, 0, 31);
    h.step();
    assert_eq!(len(&mut h, RingId::Bbo(0)), 0);
}

fn bbos(h: &mut H, n: u64) {
    for k in 0..n {
        h.push(RingId::Bbo(0), &bbo(A1, A, 0, "50000", "50002", 20_000 + k, 1));
    }
}

fn order_of(h: &H, m: usize) -> Vec<EventKind> {
    inputs_since(h, m).iter().map(|r| r.kind).collect()
}

#[test]
fn seq_061_ctl_preempts_bbo() {
    let mut h = steady();
    bbos(&mut h, 10);
    let base = h.io().takes;
    let (c, id) = h.cmd_elem(
        "cli:test",
        4,
        AC1,
        &ControlCmd::Kill {
            scope: scope(hft_types::StrategyId(0), AC1, GroupId(0)),
            level: KillLevel::Account,
            flatten: false,
            cancel_resting: true,
            hard: false,
        },
        |_| {},
    );
    h.io().arrivals.push(Arrival {
        after_take: base + 3,
        ring: RingId::Ctl,
        elem: c,
    });
    let m = h.mark();
    h.step();
    let k = order_of(&h, m);
    let pos = k.iter().position(|x| *x == EventKind::CONTROL_CMD).unwrap();
    assert_eq!(pos, 3);
    assert!(k[..3].iter().all(|x| *x == EventKind::BBO));
    assert_eq!(k.iter().filter(|x| **x == EventKind::BBO).count(), 10);
    let res = h
        .since(m)
        .iter()
        .position(|r| r.kind == EventKind::CMD_RESULT)
        .unwrap();
    let b4 = h
        .since(m)
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == EventKind::BBO)
        .nth(3)
        .unwrap()
        .0;
    assert!(res < b4);
    assert!(h.result_of(id).is_some());
}

#[test]
fn seq_062_rep_preempts_bbo() {
    let mut h = steady();
    bbos(&mut h, 10);
    let base = h.io().takes;
    for k in 0..2 {
        h.io().arrivals.push(Arrival {
            after_take: base + 3,
            ring: RingId::Rep(1),
            elem: elem(&alive(B, k)),
        });
    }
    let m = h.mark();
    h.step();
    let k = order_of(&h, m);
    assert_eq!(
        &k[..6],
        &[
            EventKind::BBO,
            EventKind::BBO,
            EventKind::BBO,
            EventKind::CONN_STATE,
            EventKind::CONN_STATE,
            EventKind::BBO
        ]
    );
}

#[test]
fn seq_063_preempted_count_against_limits() {
    let mut h = steady();
    for _ in 0..4 {
        let (c, _) = h.cmd_elem("cli:test", 4, AC1, &ControlCmd::SnapshotNow, |_| {});
        h.push_e(RingId::Ctl, c);
    }
    bbos(&mut h, 10);
    let base = h.io().takes;
    let (c, _) = h.cmd_elem("cli:test", 4, AC1, &ControlCmd::SnapshotNow, |_| {});
    h.io().arrivals.push(Arrival {
        after_take: base + 6,
        ring: RingId::Ctl,
        elem: c,
    });
    let m = h.mark();
    h.step();
    let k = order_of(&h, m);
    assert_eq!(k.iter().filter(|x| **x == EventKind::CONTROL_CMD).count(), 4);
    assert_eq!(k.iter().filter(|x| **x == EventKind::BBO).count(), 10);
    assert_eq!(len(&mut h, RingId::Ctl), 1);
    let m = h.mark();
    h.step();
    assert!(order_of(&h, m).contains(&EventKind::CONTROL_CMD));
}

#[test]
fn seq_064_advice_limit_and_preempt() {
    let mut h = steady();
    for k in 0..20u64 {
        let mut hd = <hft_event::AdviceHdr as zerocopy::FromZeros>::new_zeroed();
        hd.strategy = S2;
        hd.account = AC2;
        hd.local_kind = 7;
        hd.group = G2;
        hd.len = 8;
        hd.basis_seq = k;
        let mut rec = zerocopy::IntoBytes::as_bytes(&hd).to_vec();
        rec.extend_from_slice(&k.to_le_bytes());
        h.push_bytes(RingId::Advice, EventKind::ADVICE, rec);
    }
    let base = h.io().takes;
    h.io().arrivals.push(Arrival {
        after_take: base + 5,
        ring: RingId::Rep(0),
        elem: elem(&alive(A, 1)),
    });
    let m = h.mark();
    h.step();
    let k = order_of(&h, m);
    let adv: Vec<usize> = k
        .iter()
        .enumerate()
        .filter(|(_, x)| **x == EventKind::ADVICE)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(adv.len(), 16);
    let rep = k.iter().position(|x| *x == EventKind::CONN_STATE).unwrap();
    assert_eq!(rep, 5);
    assert_eq!(len(&mut h, RingId::Advice), 4);
}

#[test]
fn seq_066_md_time_slice() {
    for (p, want) in [
        (20_000u64, 3usize),
        (24_999, 3),
        (25_000, 2),
        (25_001, 2),
        (60_000, 1),
    ] {
        let mut h = steady();
        for k in 0..100u64 {
            let t = trade(A1, k + 1);
            h.push(RingId::Md(0), &t);
        }
        h.io().tsc_per_take = p;
        h.step();
        assert_eq!(100 - len(&mut h, RingId::Md(0)), want, "p = {p}");
    }
}

#[test]
fn seq_067_rep_sim_last_and_bounded() {
    for (n, want) in [(1_000usize, 32usize), (31, 31), (33, 32)] {
        let mut h = steady();
        for k in 0..n {
            h.push(RingId::RepSim, &alive(A, k as u64));
        }
        h.step();
        assert_eq!(n - len(&mut h, RingId::RepSim), want);
        if n == 33 {
            h.step();
            assert_eq!(len(&mut h, RingId::RepSim), 0);
        }
    }
    // .2 rep, bbo, md first, rep-sim last.
    let mut h = steady();
    for k in 0..1_000u64 {
        h.push(RingId::RepSim, &alive(A, k));
    }
    for k in 0..10u64 {
        h.push(RingId::Rep(0), &alive(B, k));
        h.push(RingId::Bbo(0), &bbo(A1, A, 0, "50000", "50002", 30_000 + k, 1));
        let t = trade(A1, k + 1);
        h.push(RingId::Md(0), &t);
    }
    let base = h.io().takes;
    h.step();
    let handed: Vec<Option<RingId>> = h.io().handed[base as usize..].to_vec();
    let first_sim = handed.iter().position(|r| *r == Some(RingId::RepSim)).unwrap();
    assert!(handed[first_sim..].iter().all(|r| *r == Some(RingId::RepSim)));
    assert_eq!(handed[first_sim..].len(), 32);
}

#[test]
fn seq_068_timer_mid_check() {
    for (dt, ends) in [(-1i64, false), (0, true), (1, true)] {
        let mut h = steady();
        let t = h.now();
        let d = t + 1_000_000;
        timers(&mut h, 1, d);
        h.at(t + 10);
        bbos(&mut h, 50);
        let base = h.io().takes;
        // Before the 3rd Bbo is handed (after the 2nd take), move TSC.
        h.io().arrivals.clear();
        let target = (d as i64 + dt) as u64;
        // Use the after_take hook through tsc_per_take: jump on the 2nd take.
        let m = h.mark();
        h.io().tsc_per_take = 0;
        h.step_until_take(base + 2, target);
        let k = order_of(&h, m);
        let n_bbo = k.iter().filter(|x| **x == EventKind::BBO).count();
        if ends {
            assert_eq!(n_bbo, 2, "dt {dt}");
            let m2 = h.mark();
            h.step();
            let first = inputs_since(&h, m2).into_iter().next().unwrap();
            assert_eq!(first.kind, EventKind::TIMER);
            assert!(first.tsc >= d);
        } else {
            assert!(n_bbo >= 3, "dt {dt}");
        }
    }
}

#[test]
fn seq_070_no_mid_check_in_b() {
    let mut h = steady();
    let t = h.now();
    timers(&mut h, 1, t + 100);
    for _ in 0..3 {
        let (c, _) = h.cmd_elem("cli:test", 4, AC1, &ControlCmd::SnapshotNow, |_| {});
        h.push_e(RingId::Ctl, c);
    }
    h.push(RingId::Rep(0), &alive(A, 1));
    // The timer becomes due during the a segment's end (before the first ctl take).
    let base = h.io().takes;
    let m = h.mark();
    h.step_until_take(base + 1, t + 100);
    let k = order_of(&h, m);
    assert_eq!(k.iter().filter(|x| **x == EventKind::CONTROL_CMD).count(), 3);
    assert_eq!(len(&mut h, RingId::Rep(0)), 1);
    let m = h.mark();
    h.step();
    let k = order_of(&h, m);
    assert_eq!(k[0], EventKind::TIMER);
    assert!(k.contains(&EventKind::CONN_STATE));
}

/// Scheduler-only boundary fixture. End-to-end creation, payload order and replay of a real
/// backlog are covered separately by SEQ-220–SEQ-223.
fn pressure_backlog(h: &mut H, headroom: usize) {
    assert!(!h.s.view().wants_continue());
    let ix = h.s.instances().ix(S1).unwrap() as u16;
    assert_eq!(h.s.engine().notify.backlog_cap(), 640);
    h.s.engine_mut().notify.fill_backlog(ix, 640 - headroom);
    assert_eq!(h.s.view().notify_headroom(), headroom);
    clear_log();
}

/// Every competing ring is nonempty. The live reports repeat the baseline cash fact and
/// produce no notification, so their admission cannot hide either headroom threshold.
fn pressure_inputs(h: &mut H) {
    h.cmd(AC1, &ControlCmd::SnapshotNow);
    for _ in 0..3 {
        h.push(
            RingId::Rep(0),
            &hft_event::BalanceReport {
                venue: A,
                account: AC1,
                asset: USDT,
                _pad: 0,
                free: q("100000"),
                locked: hft_types::Qty(0),
                ex_ts_ns: 0,
                rx_tsc: Tsc(h.now()),
            },
        );
    }
    for k in 0..5 {
        h.quote(A1, "50000", "50002");
        h.push(RingId::Md(0), &trade(A1, k + 1));
    }
    let mut a = <hft_event::AdviceHdr as zerocopy::FromZeros>::new_zeroed();
    a.strategy = S2;
    a.account = AC2;
    a.group = G2;
    a.local_kind = 7;
    a.len = 8;
    a.basis_seq = h.s.engine().cur_seq;
    let mut body = zerocopy::IntoBytes::as_bytes(&a).to_vec();
    body.extend_from_slice(&0u64.to_le_bytes());
    h.push_bytes(RingId::Advice, EventKind::ADVICE, body);
    for k in 0..2 {
        h.push(RingId::RepSim, &alive(A, k));
    }
}

const PRESSURE_RINGS: [(RingId, usize); 5] = [
    (RingId::Ctl, 1),
    (RingId::Bbo(0), 5),
    (RingId::Advice, 1),
    (RingId::Md(0), 5),
    (RingId::RepSim, 2),
];

fn assert_notify_first(h: &H, mark: usize) {
    let input = inputs_since(h, mark);
    assert_eq!(input[0].kind, EventKind::TIMER);
    let t = input[0].read::<hft_event::Timer>().unwrap();
    assert_eq!((t.owner, t.timer_id), (1, 17));
    assert_eq!(
        log()
            .iter()
            .filter(|g| !g.model && g.s == S1 && g.kind == "Timer")
            .count(),
        64
    );
}

#[test]
fn seq_071_backpressure_round() {
    let mut h = steady();
    pressure_backlog(&mut h, 300);
    pressure_inputs(&mut h);
    let mark = h.mark();
    let start = h.io_ref().handed.len();
    assert_eq!(h.step(), StepOutcome::Busy);
    assert_notify_first(&h, mark);
    assert_eq!(h.s.engine().notify.backlog_len(), 276);
    assert_eq!(h.s.view().notify_headroom(), 364);
    assert_eq!(len(&mut h, RingId::Rep(0)), 0);
    for (ring, n) in PRESSURE_RINGS {
        assert_eq!(len(&mut h, ring), n);
    }
    assert_eq!(
        &h.io_ref().handed[start..],
        &[
            None,
            Some(RingId::Rep(0)),
            Some(RingId::Rep(0)),
            Some(RingId::Rep(0))
        ]
    );
    // Admission is decided afresh at the next round, not latched for the whole backlog.
    clear_log();
    let mark = h.mark();
    let start = h.io_ref().handed.len();
    assert_eq!(h.step(), StepOutcome::Busy);
    assert_notify_first(&h, mark);
    let mut expected = vec![None];
    for (ring, n) in PRESSURE_RINGS {
        assert_eq!(len(&mut h, ring), 0);
        expected.extend(std::iter::repeat_n(Some(ring), n));
    }
    assert_eq!(h.io_ref().handed[start..], expected);
}

#[test]
fn seq_072_backpressure_thresholds() {
    for (headroom, pressure, reports) in [
        (319, true, 3),
        (320, false, 3),
        (321, false, 3),
        (95, true, 0),
        (96, true, 3),
    ] {
        let mut h = steady();
        pressure_backlog(&mut h, headroom);
        pressure_inputs(&mut h);
        let mark = h.mark();
        assert_eq!(h.step(), StepOutcome::Busy);
        assert_notify_first(&h, mark);
        assert_eq!(3 - len(&mut h, RingId::Rep(0)), reports, "headroom {headroom}");
        for (ring, n) in PRESSURE_RINGS {
            assert_eq!(
                len(&mut h, ring),
                if pressure { n } else { 0 },
                "headroom {headroom}, {ring:?}"
            );
        }
        if pressure {
            assert_eq!(h.s.view().notify_headroom(), headroom + 64);
            assert_eq!(inputs_since(&h, mark).len(), reports + 1);
        }
    }
}

#[test]
fn seq_073_replay_per_round() {
    let mut h = steady();
    let hb0 = h.hb.read();
    let _ = hb0;
    for k in 0..300u64 {
        h.push(RingId::Rep(0), &alive(A, k));
        if k % 32 == 31 {
            h.run();
        }
    }
    h.run();
    // Verify with replay_per_round = 100.
    let mut setup = h.setup.clone();
    setup.seq.replay_per_round = 100;
    let log = hft_sequencer::driver::replay::Log::from_segments(&h.j().segments);
    let n_inputs = log.recs.iter().filter(|r| r.role() == Role::Input).count();
    let first = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    let drv = hft_sequencer::driver::replay::ReplayDriver::new(
        hft_sequencer::driver::replay::ReplayMode::Verify,
        log,
        0,
        100,
    );
    let hb = hb();
    let p = plan(hft_sequencer::SnapshotSel::AtSeq(first), true, 0, 0);
    let mut s: hft_sequencer::Sequencer<hft_sequencer::driver::replay::ReplayDriver, TestSet> =
        hft_sequencer::Sequencer::boot(hft_sequencer::BootSource::Journal(p), &setup, drv, hb).unwrap();
    let mut busy = 0;
    loop {
        let before = s.driver().handed;
        match s.step() {
            StepOutcome::Busy => {
                busy += 1;
                assert!(s.driver().handed - before <= 100);
            }
            _ => break,
        }
    }
    assert_eq!(busy, n_inputs.div_ceil(100));
    assert_eq!(hb.read(), busy as u64 + 1);
}

#[test]
fn seq_074_publish_at_round_end() {
    let mut h = steady();
    for k in 0..3u64 {
        let t = trade(A1, 900 + k);
        h.push(RingId::Md(0), &t);
    }
    let p0 = h.j().publishes;
    h.step();
    assert_eq!(h.j().publishes, p0 + 1);
    assert_eq!(h.j().committed, h.j().len());
}

#[test]
fn seq_075_take_one_at_a_time() {
    let mut h = steady();
    bbos(&mut h, 5);
    h.io().tsc_per_take = 7;
    let m = h.mark();
    h.step();
    let ts: Vec<u64> = inputs_since(&h, m)
        .iter()
        .filter(|r| r.kind == EventKind::BBO)
        .map(|r| r.tsc)
        .collect();
    assert_eq!(ts.len(), 5);
    assert!(ts.windows(2).all(|w| w[1] > w[0]));
}

#[test]
fn seq_078_heartbeat_per_step() {
    let mut h = steady();
    let h0 = h.hb.read();
    bbos(&mut h, 5);
    h.step();
    h.step();
    h.step();
    assert_eq!(h.hb.read(), h0 + 3);
}
