//! EXE-450–EXE-461, EXE-463: snapshot section 21, restore and recovery.
mod common;
use std::mem::{align_of, offset_of, size_of};

use common::*;
use hft_exec::*;
use hft_strategy_sdk::ResidualPolicy;
use hft_strategy_sdk::{
    BrokenReason, EngineOrigin, ExecSpec, GroupTable, MakerParams, RefView, SimulOnFail, SimulParams,
    SnapshotReader, SnapshotWriter, StuckReason, TargetOrigin, TargetState,
};
use hft_types::{AccountId, Tsc};
use zerocopy::FromBytes;

const HZ: u64 = 1_000_000_000_000;
const N: Tsc = Tsc(1_000_000_000);
const S_: Tsc = Tsc(5_000_000_000_000);

fn snap_of(ex: &Exec) -> Vec<u8> {
    let mut w = SnapshotWriter::new(4 << 20);
    ex.snapshot(&mut w);
    w.as_bytes().to_vec()
}

fn try_restore(fx: &Fx, bytes: &[u8]) -> Result<Exec, hft_strategy_sdk::RestoreError> {
    let gt = GroupTable::new(&fx.groups, 256);
    let mut r = SnapshotReader::new(bytes);
    Exec::restore(&mut r, &cfg(c0()), &caps(256), &gt, &RefView::new(&fx.rd, A1))
}

/// `(kind, body offset in snap)` of every row.
fn offsets(snap: &[u8]) -> Vec<(u8, usize)> {
    let count = u32::from_le_bytes(snap[4..8].try_into().unwrap()) as usize;
    let mut off = 16;
    let mut v = Vec::new();
    for _ in 0..count {
        let len = u32::from_le_bytes(snap[off + 4..off + 8].try_into().unwrap()) as usize;
        v.push((snap[off], off + 8));
        off += 8 + len;
    }
    v
}

fn cell_off(snap: &[u8], a: AccountId, g: hft_types::GroupId) -> usize {
    offsets(snap)
        .into_iter()
        .find(|(k, o)| *k == ROW_CELL && snap[*o] == a.0 && snap[*o + 4..*o + 8] == g.0.to_le_bytes())
        .unwrap()
        .1
}

fn target_row_of(snap: &[u8], a: AccountId, g: hft_types::GroupId) -> ExecTargetRow {
    let o = cell_off(snap, a, g) + 400;
    ExecTargetRow::read_from_bytes(&snap[o..o + 232]).unwrap()
}

#[test]
fn exe_450_row_layouts() {
    assert_eq!(
        (size_of::<ExecRowHdr>(), offset_of!(ExecRowHdr, len)),
        (8, 4),
        "EXE-450.1"
    );
    assert_eq!(
        (
            size_of::<LegRef>(),
            offset_of!(LegRef, inst),
            offset_of!(LegRef, lot),
            offset_of!(LegRef, min_qty),
            offset_of!(LegRef, slot)
        ),
        (32, 4, 8, 16, 24),
        "EXE-450.2"
    );
    assert_eq!(
        (
            size_of::<ActiveSliceRow>(),
            offset_of!(ActiveSliceRow, link_id),
            offset_of!(ActiveSliceRow, first),
            offset_of!(ActiveSliceRow, planned)
        ),
        (32, 4, 8, 24),
        "EXE-450.3"
    );
    assert_eq!(size_of::<ExecCellRow>(), 400, "EXE-450.4");
    assert_eq!(
        [
            offset_of!(ExecCellRow, state),
            offset_of!(ExecCellRow, group),
            offset_of!(ExecCellRow, owner),
            offset_of!(ExecCellRow, flags),
            offset_of!(ExecCellRow, deferred),
            offset_of!(ExecCellRow, cap_block),
            offset_of!(ExecCellRow, gated_bits),
            offset_of!(ExecCellRow, set_seq)
        ],
        [1, 4, 8, 12, 13, 14, 20, 32],
        "EXE-450.5"
    );
    assert_eq!(
        [
            offset_of!(ExecCellRow, set_tsc),
            offset_of!(ExecCellRow, fix_first_tsc),
            offset_of!(ExecCellRow, ext_deadline),
            offset_of!(ExecCellRow, broken_since),
            offset_of!(ExecCellRow, tolerance)
        ],
        [40, 80, 96, 104, 112],
        "EXE-450.6"
    );
    assert_eq!(
        [
            offset_of!(ExecCellRow, filled),
            offset_of!(ExecCellRow, own_infl),
            offset_of!(ExecCellRow, active),
            offset_of!(ExecCellRow, auto_fix),
            offset_of!(ExecCellRow, legs)
        ],
        [120, 152, 184, 216, 272],
        "EXE-450.7"
    );
    assert_eq!(
        (
            size_of::<ExecAcctRow>(),
            offset_of!(ExecAcctRow, force_kind),
            offset_of!(ExecAcctRow, force_venues),
            offset_of!(ExecAcctRow, force_frac)
        ),
        (24, 3, 8, 16),
        "EXE-450.8"
    );
    assert_eq!(
        (size_of::<ExecSpecRow>(), offset_of!(ExecSpecRow, params)),
        (136, 8),
        "EXE-450.9"
    );
    assert_eq!(
        (
            size_of::<ExecTargetRow>(),
            [
                offset_of!(ExecTargetRow, group),
                offset_of!(ExecTargetRow, legs),
                offset_of!(ExecTargetRow, n_legs),
                offset_of!(ExecTargetRow, reason),
                offset_of!(ExecTargetRow, urgency),
                offset_of!(ExecTargetRow, band),
                offset_of!(ExecTargetRow, exec),
                offset_of!(ExecTargetRow, tag)
            ]
        ),
        (232, [4, 8, 72, 73, 74, 80, 88, 224]),
        "EXE-450.10"
    );
    assert_eq!(
        (
            size_of::<ExecGateRow>(),
            offset_of!(ExecGateRow, venue_open),
            offset_of!(ExecGateRow, gate_closed_tsc)
        ),
        (24, 8, 16),
        "EXE-450.11"
    );
    for a in [
        align_of::<ExecRowHdr>(),
        align_of::<LegRef>(),
        align_of::<ActiveSliceRow>(),
        align_of::<ExecCellRow>(),
        align_of::<ExecAcctRow>(),
        align_of::<ExecSpecRow>(),
        align_of::<ExecTargetRow>(),
        align_of::<ExecGateRow>(),
    ] {
        assert!(a <= 8, "EXE-450.12");
    }
}

#[test]
fn exe_451_row_order() {
    let g9 = gspec(G9, &[(P, B)]);
    let mut fx = Fx::with(c0(), vec![g7(), g8(), g9]);
    inject(&mut fx, &t0("10", "-10"));
    {
        let c = fx.cm(A1, GNONE);
        c.has_target = true;
        c.view.origin = TargetOrigin::Engine(EngineOrigin::MarginStress);
        c.view.state = TargetState::Working;
    }
    fx.cm(A2, G9).view.state = TargetState::Broken(BrokenReason::LinkBroken);
    fx.acct(A1).force_active = true;
    let s = fx.snap();
    let want: Vec<(u8, u8, u32)> = vec![
        (ROW_CELL, A1.0, 0),
        (ROW_CELL, A1.0, G7.0),
        (ROW_CELL, A2.0, G9.0),
        (ROW_ACCT, A1.0, 0),
        (ROW_GATE, A1.0, 0),
        (ROW_GATE, A2.0, 0),
        (ROW_GATE, S1.0, 0),
    ];
    let got: Vec<(u8, u8, u32)> = rows(&s)
        .iter()
        .map(|(k, b)| {
            (
                *k,
                b[0],
                if *k == ROW_CELL {
                    u32::from_le_bytes(b[4..8].try_into().unwrap())
                } else {
                    0
                },
            )
        })
        .collect();
    assert_eq!(got, want);
    for (k, o) in offsets(&s) {
        let len = u32::from_le_bytes(s[o - 4..o].try_into().unwrap());
        let want = if k == ROW_CELL { 632 } else { 24 };
        assert_eq!(len, want);
        assert_eq!(s[o - 7..o - 4], [0, 0, 0]);
    }
}

#[test]
fn exe_452_nonempty_cell() {
    type Setup = fn(&mut Fx);
    let rows: [(&str, Setup, bool); 9] = [
        ("EXE-452.1", |_| {}, false),
        (
            "EXE-452.2",
            |fx| {
                inject(fx, &t1("0"));
                fx.cm(A1, G8).view.state = TargetState::Idle;
            },
            true,
        ),
        (
            "EXE-452.3",
            |fx| fx.cm(A1, G8).view.state = TargetState::Stuck(StuckReason::NoLiquidity),
            true,
        ),
        ("EXE-452.4", |fx| fx.cm(A1, G8).fix.count = 1, true),
        ("EXE-452.5", |fx| fx.cm(A1, G8).ext_deadline = t(10), true),
        (
            "EXE-452.6",
            |fx| fx.cm(A1, G8).auto_fix.phase = AutoFixPhase::Failed as u8,
            true,
        ),
        ("EXE-452.7", |fx| fx.cm(A1, G8).serial = 1, true),
        ("EXE-452.8", |fx| fx.cm(A1, G8).hold = MANUAL_HOLD_DEFERRED, true),
        ("EXE-452.9", |fx| fx.cm(A1, G8).deferred = D_STATE, true),
    ];
    for (row, setup, want) in rows {
        let mut fx = Fx::new();
        setup(&mut fx);
        assert_eq!(cell_row(&fx.snap(), A1, G8).is_some(), want, "{row}");
    }
}

fn maker() -> MakerParams {
    MakerParams {
        offset: rate("0.0001"),
        requote: rate("0.0002"),
        max_rest_ms: 5000,
        _pad0: 0,
        chunk_notional: usd("500"),
        hedge: pair0(),
    }
}

fn simul() -> SimulParams {
    SimulParams {
        slice: slice0(),
        on_fail: SimulOnFail::Hold,
        _pad0: [0; 7],
        fix_tolerance: rate("0.02"),
        residual: ResidualPolicy::Hold,
        _pad1: [0; 7],
    }
}

#[test]
fn exe_453_target_row_roundtrip() {
    let rows: [(&str, hft_strategy_sdk::TargetIntent, u8, usize); 4] = [
        ("EXE-453.1", t1("-10"), 1, 48),
        ("EXE-453.2", t0("10", "-10"), 2, 96),
        (
            "EXE-453.3",
            target(
                A1,
                G7,
                &[(0, q("10")), (1, q("-10"))],
                ExecSpec::Simultaneous(simul()),
            ),
            3,
            72,
        ),
        (
            "EXE-453.4",
            target(
                A1,
                G7,
                &[(0, q("10")), (1, q("-10"))],
                ExecSpec::MakerFirst(maker()),
            ),
            4,
            128,
        ),
    ];
    for (row, tg, algo, n) in rows {
        let mut fx = Fx::new();
        inject(&mut fx, &tg);
        let s = fx.snap();
        let r = target_row_of(&s, A1, tg.group);
        assert_eq!(r.exec.algo, algo, "{row}");
        assert_eq!(r.exec._pad, [0; 7], "{row}");
        assert!(r.exec.params[n..].iter().all(|b| *b == 0), "{row}");
        assert!(r.legs.iter().all(|l| l._pad == [0; 7]), "{row}");
        assert_eq!((r._pad0, r._pad1, r._pad2), ([0; 3], [0; 5], 0), "{row}");
        let ex = fx.restore(&s);
        assert_eq!(ex.cell(A1, tg.group).unwrap().view.intent, tg, "{row}");
        assert_eq!(fx.snap(), s, "EXE-453.5");
        assert_eq!(snap_of(&ex), s, "EXE-453.5");
    }
    let mut fx = Fx::new();
    fx.cm(A1, G8).view.state = TargetState::Stuck(StuckReason::NoLiquidity);
    let s = fx.snap();
    let o = cell_off(&s, A1, G8) + 400;
    assert!(s[o..o + 232].iter().all(|b| *b == 0), "EXE-453.6");
}

#[test]
fn exe_454_restore_errors() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    inject(&mut fx, &t1("-10"));
    {
        let a = fx.acct(A1);
        a.force_active = true;
        a.force_kind = FORCE_VENUES;
    }
    let base = fx.snap();
    let g7 = cell_off(&base, A1, G7);
    let g8 = cell_off(&base, A1, G8);
    let acct = offsets(&base).into_iter().find(|x| x.0 == ROW_ACCT).unwrap().1;
    type Edit = Box<dyn Fn(&mut Vec<u8>)>;
    let set = |at: usize, v: u8| -> Edit { Box::new(move |s: &mut Vec<u8>| s[at] = v) };
    let rows: Vec<(&str, Edit)> = vec![
        ("EXE-454.1", set(g7 - 8, 4)),
        ("EXE-454.2", set(g7 - 4, 631u32.to_le_bytes()[0])),
        ("EXE-454.3", set(g7 + 1, 0)),
        ("EXE-454.3", set(g7 + 1, 5)),
        ("EXE-454.4", set(g7 + 2, 9)),
        ("EXE-454.5", set(g7 + 3, 5)),
        ("EXE-454.6", set(g7 + 11, 7)),
        ("EXE-454.7", set(g7 + 184, 8)),
        ("EXE-454.8", set(g7 + 216, 8)),
        ("EXE-454.9", set(g7 + 219, 3)),
        ("EXE-454.9", set(g7 + 219, 5)),
        ("EXE-454.10", set(g7 + 400 + 88, 0)),
        ("EXE-454.10", set(g7 + 400 + 88, 5)),
        ("EXE-454.11", set(g8 + 400 + 96 + 48, 1)),
        (
            "EXE-454.12",
            Box::new(move |s: &mut Vec<u8>| {
                s[g8 + 10] = 0;
                for b in &mut s[g8 + 400..g8 + 632] {
                    *b = 0;
                }
                s[g8 + 400 + 224] = 1;
            }),
        ),
        ("EXE-454.13", set(g7 + 273, 4)),
        ("EXE-454.14", set(acct + 3, 3)),
    ];
    for (row, edit) in rows {
        let mut s = base.clone();
        edit(&mut s);
        assert!(try_restore(&fx, &s).is_err(), "{row}");
    }
    assert!(try_restore(&fx, &base).is_ok(), "EXE-454.15");
}

fn tsc_rich(fx: &mut Fx) {
    inject(fx, &t0("10", "-10"));
    inject(fx, &t1("-10"));
    {
        let c = fx.cm(A1, G7);
        c.view.set_tsc = t(1);
        c.view.last_slice_tsc = t(2);
        c.wake_at = t(3);
        c.gated_since = t(4);
        c.stuck_since = t(5);
        c.fix = FixState {
            count: 1,
            first_tsc: t(6),
            next_tsc: t(7),
        };
        c.ext_deadline = t(8);
    }
    let c = fx.cm(A1, G8);
    c.view.state = TargetState::Broken(BrokenReason::LinkBroken);
    c.broken_since = t(9);
    c.auto_fix.phase = AutoFixPhase::Running as u8;
    c.auto_fix.due_tsc = t(10);
    c.auto_fix.started_tsc = t(11);
    c.auto_fix.run_mark_tsc = t(12);
    c.auto_fix.exec_ms = 13;
}

#[test]
fn exe_455_restore_keeps_tsc() {
    let mut fx = Fx::new();
    tsc_rich(&mut fx);
    fx.acct(A1).gate_closed_tsc = t(14);
    let x = fx.snap();
    let ex = fx.restore(&x);
    assert_eq!(snap_of(&ex), x);
}

#[test]
fn exe_456_recovery_shift() {
    type Field = fn(&mut TargetCell) -> &mut Tsc;
    let s = S_.0;
    let rows: [(&str, Field, u64, u64, u64, u64); 9] = [
        (
            "EXE-456.1",
            |c| &mut c.wake_at,
            s + 300_000_000,
            HZ,
            HZ,
            N.0 + 300_000_000,
        ),
        (
            "EXE-456.2",
            |c| &mut c.ext_deadline,
            s - 200_000_000,
            HZ,
            HZ,
            N.0 - 200_000_000,
        ),
        ("EXE-456.3", |c| &mut c.broken_since, s - 2_000_000_000, HZ, HZ, 1),
        ("EXE-456.4", |c| &mut c.fix.first_tsc, 0, HZ, HZ, 0),
        ("EXE-456.5", |c| &mut c.auto_fix.due_tsc, s, HZ, HZ, N.0),
        ("EXE-456.6", |c| &mut c.auto_fix.due_tsc, s + 1, HZ, HZ, N.0 + 1),
        (
            "EXE-456.7",
            |c| &mut c.wake_at,
            s + 300_000_000,
            HZ,
            2 * HZ,
            N.0 + 600_000_000,
        ),
        ("EXE-456.8", |c| &mut c.wake_at, s + 7, 2 * HZ, 3 * HZ, N.0 + 10),
        (
            "EXE-456.9",
            |c| &mut c.view.set_tsc,
            s - 300_000_000,
            HZ,
            2 * HZ,
            N.0 - 600_000_000,
        ),
    ];
    for (row, f, v, hp, hc, want) in rows {
        let mut fx = Fx::new();
        *f(fx.cm(A1, G7)) = Tsc(v);
        fx.ex.recovery_end(N, S_, hp, hc);
        let mut c = fx.cell(A1, G7);
        assert_eq!(f(&mut c).0, want, "{row}");
    }
    let mut fx = Fx::new();
    fx.cm(A1, G7).auto_fix.exec_ms = 4000;
    fx.acct(A1).gate_closed_tsc = Tsc(77);
    fx.ex.recovery_end(N, S_, HZ, HZ);
    assert_eq!(fx.cell(A1, G7).auto_fix.exec_ms, 4000, "EXE-456.10");
    assert_eq!(fx.ex.acct_state(A1).unwrap().gate_closed_tsc, N, "EXE-456.11");
    let others: [Field; 6] = [
        |c| &mut c.view.last_slice_tsc,
        |c| &mut c.gated_since,
        |c| &mut c.stuck_since,
        |c| &mut c.fix.next_tsc,
        |c| &mut c.auto_fix.started_tsc,
        |c| &mut c.auto_fix.run_mark_tsc,
    ];
    for f in others {
        let mut fx = Fx::new();
        *f(fx.cm(A1, G7)) = Tsc(s - 1_000_000);
        fx.ex.recovery_end(N, S_, HZ, HZ);
        let mut c = fx.cell(A1, G7);
        assert_eq!(f(&mut c).0, N.0 - 1_000_000, "EXE-456.12");
    }
}

#[test]
fn exe_457_recovery_end_gate_linkhold() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    inject(&mut fx, &t1("-10"));
    fx.cm(A1, G7).active.link_id = 9;
    fx.cm(A1, G7).active.kind = ACT_PAIR;
    fx.cm(A1, G8).active.kind = ACT_SLICE;
    let s = fx.snap();
    let mut ex = fx.restore(&s);
    ex.recovery_end(N, S_, HZ, HZ);
    let y = snap_of(&ex);
    assert_eq!(cell_row(&y, A1, G7).unwrap().flags & RF_LINK_HOLD, RF_LINK_HOLD);
    assert_eq!(cell_row(&y, A1, G8).unwrap().flags & RF_LINK_HOLD, 0);
    for a in [A1, A2, S1] {
        let g = gate_row(&y, a).unwrap();
        assert_eq!((g.gate_open, g.venue_open, g.gate_closed_tsc), (0, 0, N));
    }
}

#[test]
fn exe_458_recovery_timers() {
    let mut fx = Fx::new();
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(BrokenReason::LinkBroken);
        c.broken_since = Tsc(S_.0 - 50_000_000);
        c.wake_at = Tsc(S_.0 + 1_000_000);
        c.fix.next_tsc = Tsc(S_.0 + 2_000_000);
        c.ext_deadline = Tsc(S_.0 + 3_000_000);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = Tsc(S_.0 + 4_000_000);
    }
    fx.cm(A1, G8).wake_at = Tsc(S_.0 + 5_000_000);
    fx.ex.recovery_end(N, S_, HZ, HZ);
    let ts: Vec<ExecTimerReq> = fx.ex.recovery_timers().collect();
    let of = |g| -> Vec<(ExecTimerKind, Tsc)> {
        ts.iter()
            .filter(|x| x.group == g)
            .map(|x| (x.kind().unwrap(), x.at))
            .collect()
    };
    let alarm = Tsc(N.0 + u64::from(c0().gate_naked_alarm_ms) * 1_000_000);
    assert_eq!(
        of(G7),
        vec![
            (ExecTimerKind::Wake, Tsc(N.0 + 1_000_000)),
            (ExecTimerKind::Fix, Tsc(N.0 + 2_000_000)),
            (ExecTimerKind::External, Tsc(N.0 + 3_000_000)),
            (ExecTimerKind::AutoFix, Tsc(N.0 + 4_000_000)),
            (ExecTimerKind::Alarm, alarm),
        ]
    );
    assert_eq!(of(G8), vec![(ExecTimerKind::Wake, Tsc(N.0 + 5_000_000))]);
    let mut keys: Vec<(u32, u32)> = ts.iter().map(|x| (x.timer_id, x.group.0)).collect();
    let n = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), n);
}

fn continue_all(fx: &mut Fx) -> Vec<hft_event::ExecDecision> {
    let mut all = Vec::new();
    for _ in 0..200 {
        if !fx.ex.has_pending() {
            break;
        }
        fx.fire(ExecTimerKind::Continue, AccountId(0), GNONE);
        all.extend(fx.adv().decisions);
    }
    all
}

#[test]
fn exe_459_pending_roundtrip() {
    let mut fx = fx_many(64);
    for k in 0..64 {
        fx.legs(A1, many(k), &["5.00", "-5.0"]);
        fx.spot(A1, A, many(k), "5", "5");
    }
    fx.out = ExecOut::with_caps(OutCaps::README);
    fx.flatten(A1);
    let b0 = fx.adv();
    assert!(fx.ex.has_pending());
    let s = fx.snap();
    let pend = (0..64)
        .filter(|k| cell_row(&s, A1, many(*k)).is_some_and(|r| r.flags & RF_PENDING != 0))
        .count();
    let mut done: Vec<u16> = b0.orders.iter().map(|o| o.id.cix).collect();
    done.sort_unstable();
    done.dedup();
    assert_eq!(pend, 64 - done.len());
    assert!(pend > 0);
    let restored = fx.restore(&s);
    assert!(restored.has_pending());
    let reference = continue_all(&mut fx);
    fx.ex = restored;
    let again = continue_all(&mut fx);
    assert_eq!(norm(&again), norm(&reference));
    assert!(!reference.is_empty());
}

#[test]
fn exe_460_replay_uses_gate_rows() {
    let mut fx = Fx::new();
    fx.ex.recovery_end(t(0), t(0), HZ, HZ);
    fx.at(1000);
    let now = fx.now();
    fx.ex.on_venue_gate_open(A1, B, now);
    fx.adv();
    fx.at(2000);
    let s = fx.snap();
    let g = gate_row(&s, A1).unwrap();
    assert_eq!(g.gate_open, 0);
    assert_ne!(g.venue_open & (1 << B.0), 0);
    let replay = fx.restore(&s);
    let feed = |fx: &mut Fx| -> Rec {
        fx.at(2500);
        fx.legs(A1, G7, &["1.00", "-1.5"]);
        fx.leg_adjust(A1, G7);
        fx.adv()
    };
    let live = feed(&mut fx);
    let mut fx2 = Fx::new();
    fx2.ex = replay;
    let rep = feed(&mut fx2);
    assert_eq!(live.ord(ExecOrderKind::HedgeFix).len(), 1);
    assert_eq!(rep.ord(ExecOrderKind::HedgeFix).len(), 1);
    assert_eq!(norm(&live.decisions), norm(&rep.decisions));
}

#[test]
fn exe_461_acct_row() {
    // .1
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10"]);
    fx.force(A1, B, "0.2");
    fx.force(A1, C, "0.2");
    let s = fx.snap();
    let r = acct_row(&s, A1).unwrap();
    assert_eq!(
        (r.force_active, r.force_kind, r.force_venues, r.force_frac),
        (1, 1, (1 << B.0) | (1 << C.0), rate("0.2")),
        "EXE-461.1"
    );
    let restored = fx.restore(&s);
    let a = fx.adv();
    fx.ex = restored;
    let b = fx.adv();
    let z = |d: Vec<hft_event::ExecDecision>| -> Vec<hft_event::ExecDecision> {
        norm(&d)
            .into_iter()
            .map(|mut x| {
                x.set_seq = 0;
                x
            })
            .collect()
    };
    assert!(
        !a.dec(11).is_empty() && z(a.decisions.clone()) == z(b.decisions.clone()),
        "EXE-461.1"
    );
    // .2
    let mut fx = Fx::new();
    fx.flatten(A1);
    let r = acct_row(&fx.snap(), A1).unwrap();
    assert_eq!((r.flatten_lock, r.force_kind), (1, 2), "EXE-461.2");
    // .3
    let mut fx = Fx::new();
    fx.acct(A1).flatten_lock = true;
    let s = fx.snap();
    let r = acct_row(&s, A1).unwrap();
    assert_eq!((r.flatten_lock, r.force_kind), (1, 0), "EXE-461.3");
    assert!(fx.restore(&s).acct_state(A1).unwrap().flatten_lock, "EXE-461.3");
}

#[test]
fn exe_463_cross_host() {
    let (old, new) = (2 * HZ, 3 * HZ);
    let stop = S_;
    let now = Tsc(7_000_000_000);
    let mut fx = Fx::new();
    {
        let c = fx.cm(A1, G7);
        c.ext_deadline = Tsc(stop.0 + 3000 * 2_000_000);
        c.view.state = TargetState::Broken(BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = Tsc(stop.0 + 60_000 * 2_000_000);
        c.wake_at = Tsc(stop.0 + 100 * 2_000_000);
    }
    let s = fx.snap();
    let mut ex = fx.restore(&s);
    ex.recovery_end(now, stop, old, new);
    let c = *ex.cell(A1, G7).unwrap();
    let ns = |x: Tsc| (x.0 - now.0) / 3;
    assert_eq!(ns(c.ext_deadline), 3_000_000_000, "EXE-463.1");
    assert_eq!(ns(c.auto_fix.due_tsc), 60_000_000_000, "EXE-463.2");
    assert_eq!(ns(c.wake_at), 100_000_000, "EXE-463.3");
    for x in [c.ext_deadline, c.auto_fix.due_tsc, c.wake_at] {
        assert_ne!(x.0, 0, "EXE-463.4");
    }
}
