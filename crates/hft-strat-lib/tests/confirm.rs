//! `ConfirmWindow` (TESTS.md SLB-020–SLB-028, SLB-030–SLB-032).

mod common;

use common::{ConfirmRow, confirm_row, snap_confirm};
use hft_strat_lib::{Confirm, ConfirmParams, ConfirmParamsNs, ConfirmWindow};
use hft_strategy_sdk::SnapshotReader;

fn p(min_ms: u32, min_updates: u16, min_slow_updates: u16, min_slow_span: u8) -> ConfirmParamsNs {
    ConfirmParams {
        min_ms,
        min_updates,
        min_slow_updates,
        min_slow_span,
    }
    .ns()
}

#[test]
fn slb_020_all_zero_confirms_first() {
    let mut w = ConfirmWindow::default();
    assert_eq!(
        w.observe(true, false, 0, 1_000, &p(0, 0, 0, 0)),
        Confirm::Confirmed
    );
}

#[test]
fn slb_021_min_updates_boundary() {
    let p = p(0, 3, 0, 0);
    let mut w = ConfirmWindow::default();
    let want = [
        ("SLB-021.1", Confirm::Pending),
        ("SLB-021.2", Confirm::Pending),
        ("SLB-021.3", Confirm::Confirmed),
        ("SLB-021.4", Confirm::Confirmed),
    ];
    for (k, (row, st)) in want.into_iter().enumerate() {
        let t = (k as i64 + 1) * 1_000;
        assert_eq!(w.observe(true, false, 0, t, &p), st, "{row}");
    }
}

#[test]
fn slb_022_min_ms_boundary() {
    let p = p(100, 0, 0, 0);
    assert_eq!(p.min_ns, 100_000_000);
    let rows = [
        ("SLB-022.1", 1_099_999_999, Confirm::Pending),
        ("SLB-022.2", 1_100_000_000, Confirm::Confirmed),
        ("SLB-022.3", 1_100_000_001, Confirm::Confirmed),
    ];
    for (row, t, want) in rows {
        let mut w = ConfirmWindow::default();
        assert_eq!(
            w.observe(true, false, 0, 1_000_000_000, &p),
            Confirm::Pending,
            "{row}"
        );
        assert_eq!(w.observe(true, false, 0, t, &p), want, "{row}");
    }
}

#[test]
fn slb_023_min_slow_updates() {
    let p = p(0, 0, 2, 0);
    let rows: &[(&str, &[bool], Confirm)] = &[
        ("SLB-023.1", &[false, false, false], Confirm::Pending),
        ("SLB-023.2", &[true], Confirm::Pending),
        ("SLB-023.3", &[true, false, true], Confirm::Confirmed),
        ("SLB-023.4", &[true, true], Confirm::Confirmed),
    ];
    for &(row, seq, want) in rows {
        let mut w = ConfirmWindow::default();
        let mut last = Confirm::Idle;
        for (k, &fs) in seq.iter().enumerate() {
            last = w.observe(true, fs, 0, k as i64 + 1, &p);
            if k + 1 < seq.len() {
                assert_ne!(last, Confirm::Confirmed, "{row} step {}", k + 1);
            }
        }
        assert_eq!(last, want, "{row}");
    }
}

#[test]
fn slb_024_min_slow_span() {
    let p = p(0, 0, 0, 3);
    let rows: &[(&str, &[u64], Confirm)] = &[
        ("SLB-024.1", &[100, 101], Confirm::Pending),
        ("SLB-024.2", &[100, 101, 102], Confirm::Confirmed),
        ("SLB-024.3", &[100, 102], Confirm::Confirmed),
        ("SLB-024.4", &[100, 101, 99], Confirm::Pending),
        ("SLB-024.5", &[100, 99, 98], Confirm::Pending),
        ("SLB-024.6", &[0, 0, 0], Confirm::Pending),
    ];
    for &(row, seqs, want) in rows {
        let mut w = ConfirmWindow::default();
        let mut last = Confirm::Idle;
        for (k, &s) in seqs.iter().enumerate() {
            last = w.observe(true, true, s, k as i64 + 1, &p);
        }
        assert_eq!(last, want, "{row}");
    }
}

#[test]
fn slb_025_false_resets() {
    let p = p(0, 2, 0, 0);
    let rows: &[(&str, &[bool], Confirm)] = &[
        ("SLB-025.1", &[true, false], Confirm::Idle),
        ("SLB-025.2", &[true, true, false], Confirm::Idle),
        ("SLB-025.3", &[true, false, true], Confirm::Pending),
        ("SLB-025.4", &[false], Confirm::Idle),
    ];
    for &(row, conds, want) in rows {
        let mut w = ConfirmWindow::default();
        let mut got = Vec::new();
        for (k, &c) in conds.iter().enumerate() {
            got.push(w.observe(c, false, 0, k as i64 + 1, &p));
        }
        assert_eq!(*got.last().unwrap(), want, "{row}");
        if row == "SLB-025.2" {
            assert_eq!(got[1], Confirm::Confirmed, "{row}");
        }
        if row == "SLB-025.3" {
            assert_eq!(confirm_row(&w).n_updates, 1, "{row}");
        }
    }
}

#[test]
fn slb_026_confirmed_stays() {
    let p = p(0, 2, 0, 0);
    let mut w = ConfirmWindow::default();
    w.observe(true, true, 10, 1, &p);
    assert_eq!(w.observe(true, false, 11, 2, &p), Confirm::Confirmed);
    assert_eq!(w.observe(true, true, 12, 3, &p), Confirm::Confirmed);
    assert_eq!(
        confirm_row(&w),
        ConfirmRow {
            since_ens: 1,
            n_updates: 3,
            n_slow: 2,
            first_slow_seq: 10,
            last_slow_seq: 12,
            state: Confirm::Confirmed as u8,
        }
    );
}

#[test]
fn slb_027_counters_saturate() {
    let p = p(0, 65_535, 65_535, 0);
    let mut w = ConfirmWindow::default();
    for k in 1..=70_000i64 {
        let r = w.observe(true, true, 0, k, &p);
        match k {
            1..=65_534 => assert_eq!(r, Confirm::Pending, "observe {k}"),
            _ => assert_eq!(r, Confirm::Confirmed, "observe {k}"),
        }
    }
    let row = confirm_row(&w);
    assert_eq!((row.n_updates, row.n_slow), (65_535, 65_535));
}

#[test]
fn slb_028_reset_restarts() {
    let p = p(100, 0, 0, 0);
    let mut w = ConfirmWindow::default();
    w.observe(true, false, 0, 1_000_000_000, &p);
    w.reset();
    assert_eq!(w.observe(true, false, 0, 1_150_000_000, &p), Confirm::Pending);
    assert_eq!(confirm_row(&w).since_ens, 1_150_000_000);
    assert_eq!(w.observe(true, false, 0, 1_249_999_999, &p), Confirm::Pending);
    assert_eq!(w.observe(true, false, 0, 1_250_000_000, &p), Confirm::Confirmed);
}

#[test]
fn slb_030_params_ns() {
    let a = ConfirmParams {
        min_ms: 250,
        min_updates: 3,
        min_slow_updates: 2,
        min_slow_span: 5,
    }
    .ns();
    assert_eq!(
        a,
        ConfirmParamsNs {
            min_ns: 250_000_000,
            min_updates: 3,
            min_slow_updates: 2,
            min_slow_span: 5
        },
        "SLB-030.1"
    );
    assert_eq!(p(u32::MAX, 0, 0, 0).min_ns, 4_294_967_295_000_000, "SLB-030.2");
    assert_eq!(p(0, 0, 0, 0).min_ns, 0, "SLB-030.3");
}

/// W after SLB-031's two observations.
fn w31(p: &ConfirmParamsNs) -> ConfirmWindow {
    let mut w = ConfirmWindow::default();
    w.observe(true, true, 7, 1_000, p);
    assert_eq!(w.observe(true, false, 8, 2_000, p), Confirm::Pending);
    w
}

#[test]
fn slb_031_snapshot_restore() {
    let p = p(0, 4, 0, 0);
    let mut w = w31(&p);
    let b = snap_confirm(&w);
    assert_eq!(b.len(), 29);
    assert_eq!(
        confirm_row(&w),
        ConfirmRow {
            since_ens: 1_000,
            n_updates: 2,
            n_slow: 1,
            first_slow_seq: 7,
            last_slow_seq: 8,
            state: Confirm::Pending as u8,
        }
    );
    let mut w2 = ConfirmWindow::default();
    assert_eq!(w2.restore(&mut SnapshotReader::new(&b)), Ok(()));
    for x in [&mut w, &mut w2] {
        assert_eq!(x.observe(true, false, 9, 3_000, &p), Confirm::Pending);
        assert_eq!(x.observe(true, true, 9, 4_000, &p), Confirm::Confirmed);
    }
    assert_eq!(snap_confirm(&w), snap_confirm(&w2));
}

#[test]
fn slb_032_restore_truncated() {
    let p = p(0, 4, 0, 0);
    let b = snap_confirm(&w31(&p));
    let mut t = ConfirmWindow::default();
    assert!(t.restore(&mut SnapshotReader::new(&b[..28])).is_err());
    assert_eq!(snap_confirm(&t), vec![0u8; 29]);
}
