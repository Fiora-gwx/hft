//! The advisor side of the Strategy/Model contract (SDK-031, SDK-037–SDK-039).

use std::cell::RefCell;
use std::collections::BTreeSet;

use hft_event::{EventKind, Timer};
use hft_sequencer::codes::status;
use hft_sequencer::driver::replay::Log;
use hft_strategy_sdk::{EvMask, Part};
use proptest::prelude::*;

use super::*;
use crate::common::*;

/// Model-tick deliveries of s1's Model: the seq of the record each was delivered on.
fn tick_seqs() -> Vec<u64> {
    model(S1)
        .into_iter()
        .filter(|g| g.kind == "Timer" && g.text.contains("tag: 4294967295"))
        .map(|g| g.now.seq)
        .collect()
}

/// `(seq, utc)` of every `Timer{Sys, AdvisorTick}` record.
fn advisor_ticks(h: &H) -> Vec<(u64, i64)> {
    let log = committed(h.j());
    log.recs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == EventKind::TIMER)
        .filter(|(_, r)| r.read::<Timer>().is_some_and(|t| t.owner == 1 && t.timer_id == 6))
        .map(|(_, r)| (r.seq, UTC0 + (r.tsc as i64 - T0 as i64)))
        .collect()
}

/// Script: s1's Model ticks every `every_ms` (and, with `bbo`, gets a1 Bbos).
fn tick_script(every_ms: u32, bbo: bool) {
    reset_script();
    SCRIPT.with(|x| x.borrow_mut().model_tick.push((S1, every_ms)));
    if bbo {
        set_subs(S1, vec![(A1, 0, EvMask::BBO, Part::Model)]);
    }
}

/// B0 (`GT3`) with s1's Model on a model tick of `every_ms`.
fn rig_tick(every_ms: u32, bbo: bool) -> H {
    let b = b_gt3();
    tick_script(every_ms, bbo);
    let mut h = b.boot();
    h.steady();
    clear_log();
    h
}

/// Steps one round at TSC `t`.
fn step_at(h: &mut H, t: u64) {
    h.at(t);
    h.run();
}

#[test]
fn sdk_031_model_created_and_fed() {
    let mut b = b_gt3();
    b.params.retain(|p| p.strategy != S1);
    set_subs(S1, vec![(A1, 0, EvMask::BBO, Part::Model)]);
    let mut h = b.boot();
    h.steady();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    clear_log();
    let c0 = calls().len();
    let m = h.mark();
    let id = h.set_params(&gt3(1));
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    for k in 0..3 {
        let ask = format!("{}", 50_002 + k);
        h.quote(A1, "50000", &ask);
        h.run();
    }
    catch_up(&mut a, &h);
    let c: Vec<&str> = calls()
        .into_iter()
        .skip(c0)
        .filter(|c| c.0 == S1 && c.1.contains("model"))
        .map(|c| c.1)
        .collect();
    assert_eq!(c.first(), Some(&"create_model"));
    let bbos: Vec<u64> = h
        .kinds_since(m, EventKind::BBO)
        .iter()
        .filter(|r| r.read::<hft_event::Bbo>().is_some_and(|b| b.h.inst == A1))
        .map(|r| r.seq)
        .collect();
    assert_eq!(bbos.len(), 3);
    let got: Vec<(&str, u16, u64)> = model(S1).iter().map(|g| (g.kind, g.gix, g.now.seq)).collect();
    assert_eq!(
        got,
        bbos.iter().map(|s| ("Bbo", 0u16, *s)).collect::<Vec<_>>(),
        "the Model's first delivery follows create_model"
    );
    assert!(!fast(S1).iter().any(|g| g.kind == "Bbo"));
}

#[test]
fn sdk_037_model_tick_utc_aligned() {
    let mut h = rig_tick(2_000, false);
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    let n0 = advisor_ticks(&h).len();
    // AdvisorTicks every second at UTC0 + k s + 950 ms, k = 1..=4.
    for k in 1..=4u64 {
        step_at(&mut h, T0 + k * SEC + 950 * MS);
    }
    let ticks: Vec<(u64, i64)> = advisor_ticks(&h).into_iter().skip(n0).collect();
    let rel: Vec<i64> = ticks.iter().map(|t| t.1 - UTC0).collect();
    assert_eq!(
        rel,
        vec![1_950_000_000, 2_950_000_000, 3_950_000_000, 4_950_000_000]
    );
    clear_log();
    catch_up(&mut a, &h);
    // A 2 s model tick: delivered on the first AdvisorTick past UTC0 + 2 s and UTC0 + 4 s.
    assert_eq!(tick_seqs(), vec![ticks[1].0, ticks[3].0]);
    let t: Vec<Got> = model(S1).into_iter().filter(|g| g.kind == "Timer").collect();
    assert!(
        t.iter()
            .all(|g| g.text.contains("gix: 65535") && g.text.contains("GroupId(0)")),
        "{t:?}"
    );
    assert!(!fast(S1).iter().any(|g| g.text.contains("tag: 4294967295")));
}

#[test]
fn sdk_038_model_tick_rounds_to_resolution() {
    let mut h = rig_tick(1_500, false);
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    let n0 = advisor_ticks(&h).len();
    for k in 1..=6u64 {
        step_at(&mut h, T0 + k * SEC + 40 * MS);
    }
    let ticks: Vec<(u64, i64)> = advisor_ticks(&h).into_iter().skip(n0).collect();
    assert_eq!(ticks.len(), 6);
    clear_log();
    catch_up(&mut a, &h);
    // 1.5 s boundaries at 1.5, 3.0, 4.5, 6.0 s: the first tick past each one.
    let want: Vec<u64> = [1usize, 2, 4, 5].iter().map(|i| ticks[*i].0).collect();
    assert_eq!(tick_seqs(), want);
}

// ------------------------------------------------------------------------------- SDK-039

/// A journal of ~2 minutes (a Bbo every 100 ms, AdvisorTicks every second) per `every_ms`.
fn tick_journal(every_ms: u32) -> (MemJournalOwned, hft_sequencer::EngineSetup) {
    thread_local! {
        static CACHE: RefCell<Vec<(u32, MemJournalOwned, hft_sequencer::EngineSetup)>> =
            const { RefCell::new(Vec::new()) };
    }
    if let Some(x) = CACHE.with(|c| {
        c.borrow()
            .iter()
            .find(|x| x.0 == every_ms)
            .map(|x| (x.1.clone(), x.2.clone()))
    }) {
        return x;
    }
    let mut h = rig_tick(every_ms, true);
    let t0 = h.now();
    for k in 1..=1_200u64 {
        h.at(t0 + k * 100 * MS + (k * 7_919 % 50) * MS / 10);
        let ask = format!("{}", 50_001 + k % 5);
        h.quote(A1, "50000", &ask);
        h.run();
    }
    let log = committed(h.j());
    let out = (MemJournalOwned(log), h.setup.clone());
    CACHE.with(|c| c.borrow_mut().push((every_ms, out.0.clone(), out.1.clone())));
    out
}

#[derive(Clone)]
struct MemJournalOwned(Log);

fn start_on(log: &Log, setup: &hft_sequencer::EngineSetup, ckpt: MemCkpt) -> Adv {
    match hft_sequencer::advisor::Advisor::start(log, setup, adv_cfg(), ckpt, Ring::big(), hb()) {
        Ok(a) => a,
        Err(e) => panic!("advisor start: {e:?}"),
    }
}

/// Model-tick delivery seqs of one advisor run over `log`.
fn run_ticks(a: &mut Adv, log: &Log) -> BTreeSet<u64> {
    clear_log();
    catch_up_log(a, log);
    tick_seqs().into_iter().collect()
}

fn restart_case(every_ms: u32, k_frac: f64, with_ckpt: bool) {
    let (MemJournalOwned(log), setup) = tick_journal(every_ms);
    tick_script(every_ms, true);
    let reference = {
        let mut a = start_on(&log, &setup, MemCkpt::default());
        run_ticks(&mut a, &log)
    };
    assert!(!reference.is_empty());
    let n = log.recs.len();
    // The advisor needs a snapshot to start from: k counts from just after the first one.
    let first = log
        .recs
        .iter()
        .position(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .expect("snapshot");
    let k = (first + 1 + ((n - first - 1) as f64 * k_frac) as usize).clamp(first + 1, n - 1);
    let mut head = log.clone();
    head.recs.truncate(k);
    let mut a = start_on(&head, &setup, MemCkpt::default());
    let first = run_ticks(&mut a, &head);
    let got = if with_ckpt {
        let store = checkpoint_now(&mut a);
        let last = store.gens.get(&S1).map(|g| g.0.last_seq).expect("checkpoint");
        let mut b = start_on(&log, &setup, store);
        let second = run_ticks(&mut b, &log);
        let mut all: BTreeSet<u64> = first.into_iter().filter(|s| *s <= last).collect();
        all.extend(second);
        all
    } else {
        // Memory dropped at k: empty state and a full replay from the snapshot.
        let mut b = start_on(&log, &setup, MemCkpt::default());
        run_ticks(&mut b, &log)
    };
    assert_eq!(got, reference, "every_ms {every_ms} k {k} ckpt {with_ckpt}");
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("SDK_039_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(200),
        ..ProptestConfig::default()
    })]
    #[test]
    fn sdk_039_model_tick_restart_invariant(
        ix in 0usize..4,
        k_frac in 0.0f64..1.0,
        with_ckpt in any::<bool>(),
    ) {
        let every = [100u32, 250, 1_000, 60_000][ix];
        restart_case(every, k_frac, with_ckpt);
    }
}
