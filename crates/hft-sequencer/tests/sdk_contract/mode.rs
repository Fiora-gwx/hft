//! Run modes and intents (SDK-164–SDK-170).

use hft_event::{ControlCmd, OrderAction, OrderDecision, RecordFlags};
use hft_risk::snapshot::{RiskRowHdr, SECTION_RISK_COUNTERS, row_kind};
use hft_sequencer::codes::{detail, note, seq_note, status};
use hft_strategy_sdk::{
    EventRef, ExecSpec, ExecUpdateKind, IntentClass, IntentOutcome, OrderIntent, StuckReason,
};
use hft_types::{GroupId, InstId, Qty, RejectBits, RunMode, Side, Tif, VenueId};

use super::target::{capture_exec, issue, open_legs, read_view, tview};
use super::*;
use crate::common::*;

// ------------------------------------------------------------------------------- helpers

/// `SetMode{s1, g, acct-1, mode}` (confirmed); returns the result.
fn set_mode(h: &mut H, g: GroupId, mode: RunMode) -> (u8, u32) {
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, g),
            mode,
        },
    );
    h.run();
    h.res(id)
}

/// `SetParams` (version `v`) setting G1's `open_allowed = 0`.
fn close_g1(h: &mut H, v: u64) {
    let mut g1 = gspec(GT1);
    g1.open_allowed = 0;
    let id = h.set_params(&Spb::new(
        S1,
        AC1,
        TAKER,
        v,
        vec![g1, gspec_eth(GT2), gspec_fill(GT3)],
    ));
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    clear_log();
}

/// One IOC of `class` in G1 (Open: a1 buy; Reduce/Flatten: a1 sell; Hedge: b1 buy), 0.001.
fn intent_of(hx: &Hx<'_>, class: IntentClass) -> OrderIntent {
    let (inst, side) = match class {
        IntentClass::Open => (A1, Side::Buy),
        IntentClass::Hedge => (B1, Side::Buy),
        _ => (A1, Side::Sell),
    };
    let p = touch(hx, inst, side);
    order_in(hx, GT1, inst, side, Tif::Ioc, class, p, q("0.001"))
}

/// Gateway commands pushed so far.
fn gw(h: &H) -> usize {
    h.io_ref().out.len()
}

/// The (10 s window) token count of acct-1's bucket on `v` (nano-tokens).
fn tokens(h: &H, v: VenueId) -> i64 {
    let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    h.s.engine().risk.snapshot(&mut w);
    let r = hft_strategy_sdk::SnapshotReader::new(w.as_bytes());
    let s = r
        .find_section(SECTION_RISK_COUNTERS)
        .ok()
        .flatten()
        .expect("risk counters");
    let mut rr = s.reader();
    for _ in 0..s.count {
        let hd: RiskRowHdr = rr.get_row().expect("row header");
        // ACCT and BUCKET bodies are both 40 B; they come first.
        if hd.kind != row_kind::ACCT && hd.kind != row_kind::BUCKET {
            break;
        }
        let b: hft_risk::snapshot::RiskBucketRow = rr.get_row().expect("row");
        if hd.kind == row_kind::BUCKET && hd.account == AC1.0 && hd.venue == v.0 && hd.inst == 0 {
            return b.tokens_10s;
        }
    }
    panic!("no bucket row");
}

/// Mode deliveries to s1's fast part since the log was cleared: (gix, text).
fn modes() -> Vec<(u16, String)> {
    fast(S1)
        .into_iter()
        .filter(|g| g.kind == "Mode")
        .map(|g| (g.gix, g.text))
        .collect()
}

fn mode_is(t: &str, old: &str, new: &str) -> bool {
    t.contains(&format!("old: {old}")) && t.contains(&format!("new: {new}"))
}

// ------------------------------------------------------------------------------- SDK-164

#[test]
fn sdk_164_mode_table_orders() {
    use IntentClass::*;
    // (row, mode, open_allowed, class)
    let rows: [(u32, RunMode, bool, IntentClass); 8] = [
        (1, RunMode::Shadow, true, Open),
        (2, RunMode::CloseOnly, true, Open),
        (3, RunMode::CloseOnly, true, Reduce),
        (4, RunMode::CloseOnly, true, Hedge),
        (5, RunMode::Live, true, Open),
        (6, RunMode::Live, false, Open),
        (7, RunMode::Live, false, Reduce),
        (8, RunMode::Live, false, Flatten),
    ];
    for (row, mode, oa, class) in rows {
        let mut h = rig();
        if mode != RunMode::Shadow {
            open_legs(&mut h, GT1, &[(A1, Side::Buy, "0.01"), (B1, Side::Sell, "0.01")]);
        }
        if !oa {
            close_g1(&mut h, 2);
        }
        if mode != RunMode::Live {
            let (st, _) = set_mode(&mut h, GT1, mode);
            assert_eq!(st, status::APPLIED, "SDK-164.{row}");
        }
        let n0 = gw(&h);
        let (o, recs) = issue_with_hx(&mut h, move |hx, ctx| ctx.place(intent_of(hx, class)).ok());
        let rs: Vec<_> = recs
            .iter()
            .filter(|r| r.kind == hft_event::EventKind::ORDER_DECISION)
            .collect();
        assert_eq!(rs.len(), 1, "SDK-164.{row}: {:?}", decs(&recs));
        let rec = rs[0];
        let d: OrderDecision = rec.read().expect("decision");
        let shadow = rec.flags.0 & RecordFlags::SHADOW.0 != 0;
        let sent = gw(&h) - n0;
        let ctx = format!("SDK-164.{row}: {d:?} {o:?}");
        match row {
            1 => {
                assert!(shadow, "{ctx}");
                assert_eq!((d.reject, d.slot.0, sent), (0, 0, 0), "{ctx}");
                assert_eq!(o, Some(IntentOutcome::Shadowed), "{ctx}");
            }
            // .4: a strategy `place` of class Hedge without a link is classified Open (hft-risk).
            2 | 4 => {
                assert_eq!(
                    (d.reject, d.slot.0, sent),
                    (RejectBits::CLOSE_ONLY.0, 0, 0),
                    "{ctx}"
                );
                assert_eq!(o, Some(IntentOutcome::Rejected(RejectBits::CLOSE_ONLY)), "{ctx}");
            }
            3 => {
                assert!(!shadow, "{ctx}");
                assert_eq!(d.reject, 0, "{ctx}");
                assert_ne!(d.slot.0, 0, "{ctx}");
                assert_eq!(sent, 1, "{ctx}");
                assert!(matches!(o, Some(IntentOutcome::Placed(_))), "{ctx}");
            }
            6 => {
                assert!(!shadow, "{ctx}");
                assert_eq!(
                    (d.reject, d.slot.0, sent),
                    (RejectBits::MODE_SHADOW.0, 0, 0),
                    "{ctx}"
                );
                assert_eq!(o, Some(IntentOutcome::Shadowed), "{ctx}");
            }
            _ => assert!(matches!(o, Some(IntentOutcome::Placed(_))), "{ctx}"),
        }
    }
}

/// `issue_with` whose closure also sees the hook context.
fn issue_with_hx(
    h: &mut H,
    f: impl Fn(&Hx<'_>, &mut hft_strategy_sdk::Ctx<'_>) -> Option<hft_strategy_sdk::IntentHandle> + 'static,
) -> (Option<IntentOutcome>, Vec<hft_sequencer::MemRecord>) {
    let hc: Cap<hft_strategy_sdk::IntentHandle> = cap();
    let oc: Cap<Option<IntentOutcome>> = cap();
    let fired: Cap<bool> = cap();
    let trig = h.upd + 1;
    {
        let (hc, oc, fired) = (hc.clone(), oc.clone(), fired.clone());
        set_hook(move |hx, ev, ctx| {
            if hx.s.strategy != S1 {
                return;
            }
            if !fired.borrow().is_empty() {
                if oc.borrow().is_empty() {
                    let o = hc.borrow().last().and_then(|x| ctx.outcome(*x));
                    oc.borrow_mut().push(o);
                }
                return;
            }
            if let EventRef::Bbo { inst, b, .. } = ev
                && *inst == A1
                && b.update_id == trig
            {
                fired.borrow_mut().push(true);
                if let Some(x) = f(hx, ctx) {
                    hc.borrow_mut().push(x);
                }
            }
        });
    }
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    let m2 = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    assert!(!fired.borrow().is_empty(), "trigger Bbo not delivered");
    let o = oc.borrow().first().copied().flatten();
    // Records of the placing Input only.
    let recs: Vec<_> = h.since(m).into_iter().take(m2 - m).collect();
    (o, recs)
}

// ------------------------------------------------------------------------------- SDK-165

#[test]
fn sdk_165_shadow_no_commit() {
    // A's order bucket holds 5 tokens; acct-1's OMS tables have 64 slots.
    let mut b = b_gt3();
    b.rd.limits.retain(|l| l.0 != A);
    b.rd.limits.extend(order_limits(A, 5, 600));
    b.setup_edit.push(|s| s.oms.slots = hft_oms::PerTable::new(64));
    let mut h = b.boot();
    h.steady();
    clear_log();
    assert_eq!(set_mode(&mut h, GT1, RunMode::Shadow).0, status::APPLIED);
    let tok0 = tokens(&h, A);
    let free0 = h.s.engine().oms.table_view(A, AC1).expect("table").free_len;
    assert_eq!(tok0, 5_000_000_000, "5 tokens");
    assert_eq!(free0, 64);
    let (_, recs) = issue_with_hx(&mut h, |hx, ctx| {
        for _ in 0..3 {
            let _ = ctx.place(intent_of(hx, IntentClass::Open));
        }
        None
    });
    let rs: Vec<_> = recs
        .iter()
        .filter(|r| r.kind == hft_event::EventKind::ORDER_DECISION)
        .collect();
    assert_eq!(rs.len(), 3);
    assert!(rs.iter().all(|r| r.flags.0 & RecordFlags::SHADOW.0 != 0));
    assert_eq!(tokens(&h, A), tok0);
    assert_eq!(
        h.s.engine().oms.table_view(A, AC1).expect("table").free_len,
        free0
    );
    let lv = read_view(&mut h, |hx| hx.v.legs(AC1, GT1));
    assert_eq!(lv.leg[0].inflight_immediate, Qty(0));
}

// ------------------------------------------------------------------------------- SDK-166

/// New order decisions of `inst` in records.
fn news(recs: &[hft_sequencer::MemRecord], inst: InstId) -> Vec<(bool, OrderDecision)> {
    recs.iter()
        .filter(|r| r.kind == hft_event::EventKind::ORDER_DECISION)
        .filter_map(|r| {
            let d: OrderDecision = r.read()?;
            (d.inst == inst && d.action == OrderAction::New)
                .then_some((r.flags.0 & RecordFlags::SHADOW.0 != 0, d))
        })
        .collect()
}

/// 10 a1 Bbos, 400 ms apart.
fn ten_bbos(h: &mut H) {
    for _ in 0..10 {
        h.adv(400 * MS);
        h.quote(A1, "50000", "50001");
        h.quote(B1, "50010", "50011");
        h.run();
    }
}

#[test]
fn sdk_166_mode_table_targets() {
    let t = |x: &str| target(AC1, GT1, &[(0, q(x))], ExecSpec::TakerSlice(slice0()));
    let stuck = |ex: &Cap<(u16, hft_strategy_sdk::ExecUpdate)>, r: StuckReason| {
        ex.borrow()
            .iter()
            .any(|(g, u)| *g == 0 && u.kind == ExecUpdateKind::Stuck(r))
    };
    // .1 Shadow / .3 Live with open_allowed = 0: one recorded slice, then Stuck(Shadow).
    for row in [1u32, 3] {
        let mut h = rig();
        if row == 1 {
            assert_eq!(set_mode(&mut h, GT1, RunMode::Shadow).0, status::APPLIED);
        } else {
            close_g1(&mut h, 2);
        }
        let ex = capture_exec();
        let m = h.mark();
        let (o, _) = issue(&mut h, t("0.02"));
        assert_eq!(o, Some(IntentOutcome::TargetShadowed), "SDK-166.{row}");
        let s = news(&h.since(m), A1);
        assert_eq!(s.len(), 1, "SDK-166.{row}: {s:?}");
        assert_eq!(s[0].1.slot.0, 0, "SDK-166.{row}");
        if row == 1 {
            assert!(s[0].0 && s[0].1.reject == 0, "SDK-166.1: {s:?}");
        } else {
            assert!(
                !s[0].0 && s[0].1.reject == RejectBits::MODE_SHADOW.0,
                "SDK-166.3: {s:?}"
            );
        }
        assert!(
            stuck(&ex, StuckReason::Shadow),
            "SDK-166.{row}: {:?}",
            ex.borrow()
        );
        let m = h.mark();
        ten_bbos(&mut h);
        assert!(news(&h.since(m), A1).is_empty(), "SDK-166.{row}");
    }
    // .2 CloseOnly: no slice, Stuck(Gated(CLOSE_ONLY)), the target stays.
    let mut h = rig();
    assert_eq!(set_mode(&mut h, GT1, RunMode::CloseOnly).0, status::APPLIED);
    let ex = capture_exec();
    let m = h.mark();
    let (o, _) = issue(&mut h, t("0.02"));
    assert_eq!(o, Some(IntentOutcome::TargetAccepted), "SDK-166.2");
    ten_bbos(&mut h);
    assert!(news(&h.since(m), A1).is_empty(), "SDK-166.2");
    assert!(
        stuck(&ex, StuckReason::Gated(RejectBits::CLOSE_ONLY)),
        "SDK-166.2: {:?}",
        ex.borrow()
    );
    assert_eq!(tview(&h, GT1).map(|v| v.intent), Some(t("0.02")));
    // .4 Live, open_allowed = 1: the slice goes out.
    let mut h = rig();
    let n0 = h.io_ref().out.len();
    let m = h.mark();
    let (o, _) = issue(&mut h, t("0.02"));
    assert_eq!(o, Some(IntentOutcome::TargetAccepted), "SDK-166.4");
    let s = news(&h.since(m), A1);
    assert!(
        !s.is_empty() && s[0].1.reject == 0 && s[0].1.slot.0 != 0,
        "SDK-166.4: {s:?}"
    );
    assert!(h.io_ref().out[n0..].iter().any(|x| x.1.inst == A1));
    // .5 CloseOnly with spot 0.02, target 0: the reducing slice goes out.
    let mut h = rig();
    open_legs(&mut h, GT1, &[(A1, Side::Buy, "0.02"), (B1, Side::Sell, "0.02")]);
    assert_eq!(set_mode(&mut h, GT1, RunMode::CloseOnly).0, status::APPLIED);
    let m = h.mark();
    let (o, _) = issue(&mut h, t("0"));
    assert_eq!(o, Some(IntentOutcome::TargetAccepted), "SDK-166.5");
    let s = news(&h.since(m), A1);
    assert!(
        !s.is_empty() && s[0].1.reject == 0 && s[0].1.slot.0 != 0 && s[0].1.side == Side::Sell,
        "SDK-166.5: {s:?}"
    );
}

// ------------------------------------------------------------------------------- SDK-167

#[test]
fn sdk_167_mode_event_and_resting_cancel() {
    let mut h = rig();
    open_legs(&mut h, GT1, &[(A1, Side::Buy, "0.01"), (B1, Side::Sell, "0.01")]);
    // G1: Open GTC buy (s1) and Reduce GTC sell (s2) resting on a1.
    let (_, recs) = issue_with_hx(&mut h, |hx, ctx| {
        let _ = ctx.place(order_in(
            hx,
            GT1,
            A1,
            Side::Buy,
            Tif::Gtc,
            IntentClass::Open,
            px("49500"),
            q("0.001"),
        ));
        let _ = ctx.place(order_in(
            hx,
            GT1,
            A1,
            Side::Sell,
            Tif::Gtc,
            IntentClass::Reduce,
            px("50500"),
            q("0.001"),
        ));
        None
    });
    let rest: Vec<OrderDecision> = decs(&recs).into_iter().filter(|d| d.slot.0 != 0).collect();
    assert_eq!(rest.len(), 2, "{:?}", decs(&recs));
    for d in &rest {
        h.ack(d);
    }
    let (s1, s2) = (rest[0].slot, rest[1].slot);
    // G2: an increasing target (a2 +0.1 in slices of ≤ 70 USDT), first slice filled.
    let mut sl = slice0();
    sl.max_chunk_notional = n("70");
    let t2 = target(AC1, GT2, &[(0, q("0.1"))], ExecSpec::TakerSlice(sl));
    let m = h.mark();
    let (o, _) = issue(&mut h, t2);
    assert_eq!(o, Some(IntentOutcome::TargetAccepted));
    let d = news(&h.since(m), A2)
        .into_iter()
        .find(|x| x.1.slot.0 != 0)
        .expect("G2 slice")
        .1;
    h.ack(&d);
    h.fill(&d, d.qty, d.qty, d.px);
    // Step 1.
    clear_log();
    let m = h.mark();
    assert_eq!(set_mode(&mut h, GT1, RunMode::CloseOnly).0, status::APPLIED);
    assert_eq!(set_mode(&mut h, GT2, RunMode::CloseOnly).0, status::APPLIED);
    for _ in 0..5 {
        h.adv(400 * MS);
        h.quote(A2, "3000", "3000.5");
        h.quote(A1, "50000", "50001");
        h.run();
    }
    let ms = modes();
    for g in [0u16, 1] {
        let n = ms
            .iter()
            .filter(|x| x.0 == g && mode_is(&x.1, "Live", "CloseOnly"))
            .count();
        assert_eq!(n, 1, "gix {g}: {ms:?}");
    }
    let recs = h.since(m);
    let cancels: Vec<u32> = decs(&recs)
        .iter()
        .filter(|d| d.action == OrderAction::Cancel)
        .map(|d| d.target_slot.0)
        .collect();
    assert_eq!(cancels, vec![s1.0], "s2 = {s2:?}");
    assert!(tview(&h, GT2).is_some());
    assert!(news(&recs, A2).is_empty(), "{:?}", news(&recs, A2));
    // Step 2: back to Live; the executor resumes G2 in the same Input.
    clear_log();
    assert_eq!(set_mode(&mut h, GT1, RunMode::Live).0, status::APPLIED);
    let m = h.mark();
    assert_eq!(set_mode(&mut h, GT2, RunMode::Live).0, status::APPLIED);
    let ms = modes();
    for g in [0u16, 1] {
        assert!(
            ms.iter().any(|x| x.0 == g && mode_is(&x.1, "CloseOnly", "Live")),
            "{ms:?}"
        );
    }
    let s = news(&h.since(m), A2);
    assert!(
        s.iter()
            .any(|x| x.1.reject == 0 && x.1.slot.0 != 0 && x.1.side == Side::Buy),
        "{s:?}"
    );
}

// ------------------------------------------------------------------------------- SDK-168

#[test]
fn sdk_168_shadow_deferred() {
    let mut h = rig();
    open_legs(&mut h, GT1, &[(A1, Side::Buy, "0.01"), (B1, Side::Sell, "0.01")]);
    clear_log();
    // Step 1.
    assert_eq!(
        set_mode(&mut h, GT1, RunMode::Shadow),
        (status::APPLIED, u32::from(detail::SHADOW_DEFERRED))
    );
    let ms = modes();
    assert!(
        ms.iter().any(|x| x.0 == 0 && mode_is(&x.1, "Live", "CloseOnly")),
        "{ms:?}"
    );
    // Step 2: Reduce both legs to 0.
    let (_, recs) = issue_with_hx(&mut h, |hx, ctx| {
        let _ = ctx.place(order_in(
            hx,
            GT1,
            A1,
            Side::Sell,
            Tif::Ioc,
            IntentClass::Reduce,
            touch(hx, A1, Side::Sell),
            q("0.01"),
        ));
        let _ = ctx.place(order_in(
            hx,
            GT1,
            B1,
            Side::Buy,
            Tif::Ioc,
            IntentClass::Reduce,
            touch(hx, B1, Side::Buy),
            q("0.01"),
        ));
        None
    });
    let ds: Vec<OrderDecision> = decs(&recs);
    assert_eq!(ds.len(), 2, "{ds:?}");
    assert!(ds.iter().all(|d| d.reject == 0 && d.slot.0 != 0), "{ds:?}");
    for d in &ds {
        h.ack(d);
    }
    h.fill(&ds[0], ds[0].qty, ds[0].qty, ds[0].px);
    clear_log();
    let m = h.mark();
    h.fill(&ds[1], ds[1].qty, ds[1].qty, ds[1].px);
    let recs = h.since(m);
    let fill_seq = recs
        .iter()
        .find(|r| r.kind == hft_event::EventKind::ORDER_EVENT)
        .map(|r| r.seq)
        .expect("fill input");
    let landed: Vec<Got> = fast(S1)
        .into_iter()
        .filter(|g| g.kind == "Mode" && g.gix == 0 && mode_is(&g.text, "CloseOnly", "Shadow"))
        .collect();
    assert_eq!(landed.len(), 1, "{:?}", fast(S1));
    assert_eq!(landed[0].now.seq, fill_seq);
    let nt = h
        .notes_since(m)
        .into_iter()
        .find(|x| x.code == seq_note(note::SHADOW_DEFERRED))
        .expect("Note{ShadowDeferred}");
    assert_eq!(nt.v[2], 1);
}

// ------------------------------------------------------------------------------- SDK-169

#[test]
fn sdk_169_second_leg_bypasses_mode_gate() {
    let mut h = rig();
    let (_, recs) = issue_with_hx(&mut h, |hx, ctx| {
        let _ = ctx.place(pair_first(hx, q("0.01")));
        None
    });
    let d1 = decs(&recs)
        .into_iter()
        .find(|d| d.inst == A1 && d.slot.0 != 0)
        .expect("first leg");
    // Step 1: position 0, in flight ≠ 0 → CloseOnly (Shadow deferred).
    assert_eq!(set_mode(&mut h, GT1, RunMode::Shadow).0, status::APPLIED);
    assert_eq!(
        h.mode(S1, AC1, GT1),
        hft_risk::mode::Effective::CloseOnly {
            deferred_shadow: true
        }
    );
    // Step 2.
    h.ack(&d1);
    let n0 = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&d1, d1.qty, d1.qty, d1.px);
    let recs = h.since(m);
    let second: Vec<_> = recs
        .iter()
        .filter(|r| r.kind == hft_event::EventKind::ORDER_DECISION)
        .filter_map(|r| r.read::<OrderDecision>().map(|d| (r.flags, d)))
        .filter(|x| x.1.link_role == 2)
        .collect();
    assert_eq!(second.len(), 1, "{second:?}");
    let (fl, d2) = second[0];
    assert_eq!(d2.reject, 0, "{d2:?}");
    assert_eq!(fl.0 & RecordFlags::SHADOW.0, 0);
    let out: Vec<_> = h.io_ref().out[n0..].iter().filter(|x| x.1.inst == B1).collect();
    assert_eq!(out.len(), 1, "{out:?}");
}

// ------------------------------------------------------------------------------- SDK-170

#[test]
fn sdk_170_default_mode_for_new_groups() {
    let g4 = GroupId(0x0100_0004);
    let f15 = InstId(15);
    let mut h = rig();
    // Step 1.
    assert_eq!(
        set_mode(&mut h, GroupId(0), RunMode::CloseOnly).0,
        status::APPLIED
    );
    let ms = modes();
    for g in 0u16..3 {
        assert!(
            ms.iter().any(|x| x.0 == g && x.1.contains("new: CloseOnly")),
            "gix {g}: {ms:?}"
        );
    }
    // Step 2: G4 = inst 15 (FILL spot on A) alone.
    h.quote(f15, "10", "10.01");
    h.run();
    let gs = |with4: bool| {
        let mut v = vec![gspec(GT1), gspec_eth(GT2), gspec_fill(GT3)];
        if with4 {
            v.push(group(g4, FILL, &[leg(f15, A)]));
        }
        v
    };
    let id = h.set_params(&Spb::new(S1, AC1, TAKER, 2, gs(true)));
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    let m4 = read_view(&mut h, move |hx| hx.v.mode(AC1, g4));
    assert_eq!(m4, RunMode::CloseOnly);
    // Step 3.
    let id = h.set_params(&Spb::new(S1, AC1, TAKER, 3, gs(false)));
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    clear_log();
    for _ in 0..3 {
        h.adv(400 * MS);
        h.quote(f15, "10", "10.01");
        h.quote(A1, "50000", "50001");
        h.run();
    }
    let got = fast(S1);
    assert!(got.iter().any(|g| g.kind == "Bbo"));
    assert!(got.iter().all(|g| g.gix != 3 && g.inst != f15), "{got:?}");
}
