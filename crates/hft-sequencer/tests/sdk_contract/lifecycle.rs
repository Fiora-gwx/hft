//! Instance lifecycle (SDK-016–SDK-021, SDK-026, SDK-061, SDK-207).

use hft_refdata::AccountSpec;
use hft_sequencer::codes::{detail, status};
use hft_sequencer::{SnapshotSel, StepOutcome};
use hft_strategy_sdk::{EventRef, IntentClass};
use hft_types::{AccountId, GroupId, Side, StrategyId, Tif};

use super::*;
use crate::common::*;

/// B0 with s1 not yet parameterized, steady.
fn rig_no_s1() -> H {
    let mut b = b_gt3();
    b.params.retain(|p| p.strategy != S1);
    let mut h = b.boot();
    h.steady();
    clear_log();
    h
}

/// SDK-016 then the call-log mark after it.
fn after_016() -> (H, usize) {
    let mut h = rig_no_s1();
    let id = h.set_params(&gt3(1));
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    let m = calls().len();
    (h, m)
}

#[test]
fn sdk_016_first_params_create_then_subscribe() {
    let mut h = rig_no_s1();
    let c0 = calls().len();
    let id = h.set_params(&gt3(1));
    // Nothing is called before the command's Input is processed.
    assert!(calls_of(S1, c0).is_empty());
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    assert_eq!(calls_of(S1, c0), vec!["create", "subscriptions"]);
    let slot = h.s.instances().get(S1).expect("s1 slot");
    let inst = slot.inst.as_ref().expect("created");
    assert_eq!(inst.groups.len(), 3);
    assert_eq!(inst.groups.capacity(), 8);
}

#[test]
fn sdk_017_later_params_on_params_then_subscribe() {
    let (mut h, m) = after_016();
    let mut p = gt3(2);
    p.values[F_THR] = 30_000;
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    assert_eq!(calls_of(S1, m), vec!["on_params", "subscriptions"]);
    let fp = SCRIPT.with(|x| x.borrow().fast_params.last().copied());
    assert_eq!(fp, Some((S1, "on_params", 30_000)));
}

#[test]
fn sdk_018_on_params_err_keeps_old() {
    let (mut h, m) = after_016();
    SCRIPT.with(|x| x.borrow_mut().fail_on_params = true);
    let mut p = gt3(2);
    p.values[F_THR] = 30_000;
    let id = h.set_params(&p);
    h.run();
    SCRIPT.with(|x| x.borrow_mut().fail_on_params = false);
    let (st, d) = h.res(id);
    assert_eq!(st, status::INVALID_ARGS);
    assert_eq!(d & 0xFFFF, u32::from(detail::PARAM_INVALID));
    assert_eq!((d >> 16) & 0xF, u32::from(detail::PARAM_KIND_STRATEGY));
    let c = calls_of(S1, m);
    assert_eq!(c, vec!["on_params"], "no subscriptions after a failed on_params");
    let cur =
        h.s.instances()
            .get(S1)
            .and_then(|s| s.cur.as_ref())
            .expect("params");
    assert_eq!(cur.version, 1);
    // The old parameters stay in force (thr = 5 of version 1).
    assert_eq!(cur.params.get(F_THR), 5);
    clear_log();
    h.quote(A1, "50000", "50002");
    h.run();
    assert!(fast(S1).iter().any(|g| g.kind == "Bbo"));
}

fn snap_row(r: &hft_sequencer::MemRecord, s: StrategyId) -> (u16, Vec<u8>) {
    let (_, secs) = snap(r);
    let (hd, _, body) = secs
        .strategies
        .iter()
        .find(|x| x.0.strategy == s)
        .expect("strategy row");
    (hd.desc_version, body.clone())
}

fn rig_v5() -> H<TestSetV5> {
    reset_script();
    let mut b: B0<TestSetV5> = B0::new();
    b.rd.insts.push(inst_spec(
        F23,
        B,
        "F23_USDT",
        true,
        FILL,
        ("0.01", 1),
        ("0.001", 100_000),
    ));
    b.caps.max_timers = 512;
    b.params = vec![gt3(1), Spb::s2(1), Spb::s9(1)];
    let mut h = b.boot();
    h.steady();
    h
}

#[test]
fn sdk_020_keyframe_snapshot_once() {
    let mut h = rig_v5();
    h.quote(A1, "50000", "50002");
    h.run();
    let c0 = calls().len();
    let n_events =
        h.s.instances()
            .get(S1)
            .and_then(|s| s.inst.as_ref())
            .map(|i| i.n_events);
    let k = h.keyframe();
    assert_eq!(calls_of(S1, c0).iter().filter(|c| **c == "snapshot").count(), 1);
    let (dv, body) = snap_row(&k, S1);
    assert_eq!(dv, 5);
    assert_eq!(body, n_events.expect("instance").to_le_bytes().to_vec());
}

#[test]
fn sdk_021_restore_sees_desc_version() {
    let mut h = rig_v5();
    h.quote(A1, "50000", "50002");
    h.run();
    let k = h.keyframe();
    let (_, body) = snap_row(&k, S1);
    SCRIPT.with(|x| x.borrow_mut().restore_desc.clear());
    let c0 = calls().len();
    let h2 =
        H::crash_restart(h, plan(SnapshotSel::AtSeq(k.seq), false, 1, 0), Rerun::default()).expect("restart");
    assert_eq!(calls_of(S1, c0).iter().filter(|c| **c == "restore").count(), 1);
    let rd: Vec<_> = SCRIPT.with(|x| {
        x.borrow()
            .restore_desc
            .iter()
            .copied()
            .filter(|r| r.0 == S1)
            .collect()
    });
    assert_eq!(rd, vec![(S1, 5)]);
    let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    h2.s.write_sections(&mut w);
    let secs = hft_sequencer::snapshot::read_seq_sections(w.as_bytes()).expect("sections");
    let (_, _, b2) = secs.strategies.iter().find(|x| x.0.strategy == S1).expect("row");
    assert_eq!(b2, &body);
}

#[test]
fn sdk_026_remove_group_with_position_rejected() {
    let mut h = rig();
    open_in(&mut h, S1, GT2, A2, Side::Buy, "0.01");
    let tid = {
        let c = calls().len();
        set_hook(move |hx, ev, ctx| {
            if let EventRef::Bbo { .. } = ev
                && hx.s.strategy == S1
            {
                let now = hx.v.now();
                let _ = ctx.timer_at(now.tsc_of(now.engine_ns + 60_000_000_000), 8, GT2);
            }
        });
        h.quote(A1, "50000", "50002");
        h.run();
        clear_hook();
        c
    };
    let _ = tid;
    let m = calls().len();
    let mut p = Spb::new(S1, AC1, TAKER, 2, vec![gspec(GT1), gspec_fill(GT3)]);
    p.values[F_THR] = 6;
    let id = h.set_params(&p);
    h.run();
    let (st, d) = h.res(id);
    assert_eq!(st, status::PRECONDITION_FAILED);
    assert_eq!(d & 0xFFFF, u32::from(detail::GROUP_HAS_POSITION));
    assert!(calls_of(S1, m).is_empty(), "{:?}", calls_of(S1, m));
    // G2 keeps its place and its timer.
    let cur =
        h.s.instances()
            .get(S1)
            .and_then(|s| s.cur.as_ref())
            .expect("params");
    assert_eq!(cur.version, 1);
    assert!(cur.groups.gix_of(GT2).is_some());
    let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    h.s.write_sections(&mut w);
    let secs = hft_sequencer::snapshot::read_seq_sections(w.as_bytes()).expect("sections");
    assert!(secs.timers.iter().any(|t| t.group == GT2), "{:?}", secs.timers);
}

#[test]
fn sdk_061_non_prod_only_rejected_on_prod() {
    reset_script();
    let mut b = b0();
    b.caps.max_timers = 1_024;
    b.caps.max_instances = 8;
    // Instance 3 (probe-np) on account 3 (not prod); instance 4 (probe-np) on account 4 (prod).
    for (k, prod) in [(3u8, false), (4u8, true)] {
        b.rd.accounts[usize::from(k) - 1] = AccountSpec {
            id: AccountId(k),
            sim: false,
            bound_instance: StrategyId(k),
            is_prod: prod,
            venues: vbits(&[A, B]),
            label: format!("acct-{k}"),
            uid_policy: hft_refdata::UidPolicy::Verified,
        };
        b.rd.instances[usize::from(k) - 1].kind_id = NP;
    }
    let mut h = b.boot();
    h.steady();
    let c0 = calls().len();
    let p3 = Spb::new(
        StrategyId(3),
        AccountId(3),
        NP,
        1,
        vec![gspec(GroupId(0x0300_0001))],
    );
    let id3 = h.set_params_as(&p3, "cli:test", ADMIN, true, |_| {});
    h.run();
    assert_eq!(h.res(id3), (status::APPLIED, 0));
    let p4 = Spb::new(
        StrategyId(4),
        AccountId(4),
        NP,
        1,
        vec![gspec(GroupId(0x0400_0001))],
    );
    let id4 = h.set_params_as(&p4, "cli:test", ADMIN, true, |_| {});
    h.run();
    let (st, d) = h.res(id4);
    assert_eq!(st, status::INVALID_ARGS);
    assert_eq!(d & 0xFFFF, u32::from(detail::PARAM_INVALID));
    assert_eq!((d >> 16) & 0xF, u32::from(detail::PARAM_KIND_NON_PROD_ONLY));
    assert!(calls_of(StrategyId(4), c0).is_empty());
    assert_eq!(calls_of(StrategyId(3), c0), vec!["create", "subscriptions"]);
}

#[test]
fn sdk_207_sim_and_live_identical() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    // Both instances (s1 live, s9 sim; same kind and parameter values) buy 0.001 a1 at the ask
    // on every a1 Bbo whose update id is a multiple of 4.
    add_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && (hx.s.strategy == S1 || hx.s.strategy == S9)
            && b.update_id.is_multiple_of(4)
        {
            let it = intent(
                hx,
                A1,
                Side::Buy,
                Tif::Ioc,
                IntentClass::Open,
                b.ask_px,
                q("0.001"),
            );
            let _ = ctx.place(it);
        }
    });
    let m = h.mark();
    // 20 Bbos, 5 of them with an update id that is a multiple of 4.
    while !h.upd.is_multiple_of(4) {
        h.upd += 1;
    }
    for k in 0..20u32 {
        let ask = format!("{}", 50_001 + k);
        h.quote(A1, "50000", &ask);
        h.run();
    }
    let all = decisions(&h.since(m));
    let live: Vec<_> = all.iter().filter(|d| d.1.strategy == S1).collect();
    let sim: Vec<_> = all.iter().filter(|d| d.1.strategy == S9).collect();
    assert_eq!(live.len(), 5);
    assert_eq!(sim.len(), 5);
    for (l, s) in live.iter().zip(sim.iter()) {
        // Same Input; the live instance's decision first.
        assert_eq!(l.1.trigger_seq, s.1.trigger_seq);
        assert!(l.0 < s.0);
        let mut a = l.1;
        let mut b = s.1;
        for d in [&mut a, &mut b] {
            d.strategy = StrategyId(0);
            d.account = AccountId(0);
            d.slot = hft_types::SlotId(0);
            d.r#gen = 0;
            d.intent_no = 0;
            d.group = GroupId(0);
        }
        assert_eq!(a, b);
    }
    let _ = StepOutcome::Idle;
}
