//! Reconciliation grading, Funding, recovery streak, AUTO_EXTERNAL precondition, groups without
//! Summary (RSK-200–RSK-203, RSK-282).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

mod common;
use common::*;

use hft_event::ReconcileReport;
use hft_risk::mode::{GateEvent, ModeErr, ModeInput, ReconItemKey, RecoveryFlags};
use hft_risk::{ReconVerdicts, ReconcileGroup};
use hft_types::{CapSource, InstId};

/// A reconciliation round on (1, B): Summary plus `items`.
struct Rounds {
    id: u64,
    vs: u64,
}
impl Rounds {
    fn new() -> Rounds {
        Rounds {
            id: 60_000,
            vs: 100_000,
        }
    }
    fn round(&mut self, w: &mut W, sflags: u16, skipped: u16, items: &[Item]) -> ReconVerdicts {
        self.id += 1;
        self.vs += 10;
        let mut g = vec![summary(1, B, self.id, self.vs, sflags, skipped)];
        for it in items {
            g.push(it.build(self.id));
        }
        w.risk.on_reconcile(&ReconcileGroup { items: &g })
    }
    /// A group with no Summary (one UnknownResolved item).
    fn u_group(&mut self, w: &mut W) -> ReconVerdicts {
        self.id += 1;
        let mut x = rep(1, B, self.id, 0, 4);
        x.inst = B1;
        w.risk.on_reconcile(&ReconcileGroup { items: &[x] })
    }
}

#[derive(Clone, Copy)]
enum Item {
    Pos(&'static str, &'static str),
    PosAuto(&'static str, &'static str),
    Bal(&'static str, &'static str),
    Funding(InstId, &'static str),
}
impl Item {
    fn build(self, id: u64) -> ReconcileReport {
        match self {
            Item::Pos(d, n) => pos_item(1, B, id, B1, d, n),
            Item::PosAuto(d, n) => {
                let mut x = pos_item(1, B, id, B1, d, n);
                x.sflags = ReconcileReport::AUTO_EXTERNAL;
                x
            }
            Item::Bal(d, n) => bal_item(1, B, id, USDT, d, n),
            Item::Funding(i, d) => {
                let mut x = rep(1, B, id, 0, 6);
                x.inst = i;
                x.diff = n(d);
                x
            }
        }
    }
}

fn is_recon(v: &ReconVerdicts) -> bool {
    matches!(v.recon, Some(ModeInput::Auto(r)) if r.source == CapSource::RECON && r.scope == sc_av(1, B))
}

#[test]
fn rsk_200_grading() {
    use Item::*;
    let run = |rounds: &[&[Item]]| -> ReconVerdicts {
        let mut w = r0();
        let mut r = Rounds::new();
        let mut last = None;
        for items in rounds {
            last = Some(r.round(&mut w, 0, 0, items));
        }
        last.unwrap()
    };
    let v = run(&[&[Pos("0.0009", "45")]]);
    assert_eq!(
        (v.n_warn, v.recon.is_none(), v.gate.unwrap().keys.len()),
        (0, true, 0),
        "RSK-200.1"
    );
    let v = run(&[&[Pos("0.001", "50")]]);
    assert_eq!((v.n_warn, v.recon.is_none()), (1, true), "RSK-200.2");
    assert_eq!(
        *v.gate.unwrap().keys.get(0).unwrap(),
        ReconItemKey::new(ReconItemKey::POSITION, B1.0)
    );
    assert_eq!(run(&[&[Bal("0.009999", "0.009999")]]).n_warn, 0, "RSK-200.3 a");
    assert_eq!(run(&[&[Bal("0.01", "0.01")]]).n_warn, 1, "RSK-200.3 b");
    assert!(
        is_recon(&run(&[&[Pos("0.003", "150")], &[Pos("0.003", "150")]])),
        "RSK-200.4"
    );
    assert!(
        is_recon(&run(&[&[Pos("0.003", "150")], &[Pos("0.0035", "175")]])),
        "RSK-200.5"
    );
    let v = run(&[&[Pos("0.003", "150")], &[Pos("0.004", "200")]]);
    assert!(v.recon.is_none() && v.n_warn == 1, "RSK-200.6");
    assert!(
        run(&[&[Pos("0.003", "150")], &[Pos("-0.003", "150")]])
            .recon
            .is_none(),
        "RSK-200.7"
    );
    assert!(
        run(&[&[Pos("0.002", "99.999999")], &[Pos("0.002", "99.999999")]])
            .recon
            .is_none(),
        "RSK-200.8"
    );
    assert!(
        is_recon(&run(&[&[Pos("0.002", "100")], &[Pos("0.002", "100")]])),
        "RSK-200.9"
    );
    assert!(
        is_recon(&run(&[&[Bal("150", "150")], &[Bal("150", "150")]])),
        "RSK-200.10"
    );
    assert!(
        is_recon(&run(&[&[PosAuto("-0.003", "150")], &[PosAuto("-0.003", "150")]])),
        "RSK-200.11"
    );
    assert!(
        run(&[&[Pos("0.003", "150")], &[], &[Pos("0.003", "150")]])
            .recon
            .is_none(),
        "RSK-200.12"
    );
}

#[test]
fn rsk_201_funding_not_graded() {
    use Item::*;
    let start = || {
        let (mut w, mut r) = recon_set();
        r.round(&mut w, 0, 0, &[]);
        (w, r)
    };
    let (mut w, mut r) = start();
    let v = r.round(&mut w, 0, 0, &[Funding(B1, "12"), Funding(InstId(0), "12")]);
    assert_eq!(v.n_warn, 0, "RSK-201.1");
    assert!(v.gate.unwrap().keys.is_empty());
    assert!(w.risk.recon_resumable(acc(1), B), "RSK-201.1 resumable");
    let (mut w, mut r) = start();
    let v = r.round(&mut w, 0, 0, &[Bal("0", "0"), Funding(B1, "12")]);
    assert_eq!(v.n_warn, 0, "RSK-201.2");
    assert!(v.recon.is_none());
    let (mut w, mut r) = start();
    let v = r.round(&mut w, 0, 0, &[Bal("12", "12"), Funding(B1, "12")]);
    assert_eq!(v.n_warn, 1, "RSK-201.3");
}

#[test]
fn rsk_202_recon_recovery() {
    use Item::*;
    let start = || {
        let mut w = r0();
        w.mm.recon = None;
        let mut r = Rounds::new();
        r.round(&mut w, 0, 0, &[Pos("0.003", "150")]);
        let v = r.round(&mut w, 0, 0, &[Pos("0.003", "150")]);
        w.ok(v.recon.expect("Recon"));
        (w, r)
    };
    #[derive(Clone, Copy)]
    enum Rd {
        Ignore,
        Inflight,
        Warn,
        UsDown,
        StreamDownCall,
    }
    use Rd::*;
    let run = |id: &str, rounds: &[Rd], resumable: bool, want_resume: Option<Result<(), ModeErr>>| {
        let (mut w, mut r) = start();
        for x in rounds {
            match x {
                Ignore => {
                    r.round(&mut w, 0, 0, &[]);
                }
                Inflight => {
                    r.round(&mut w, 0, 1, &[]);
                }
                Warn => {
                    r.round(&mut w, 0, 0, &[Pos("0.002", "100")]);
                }
                UsDown => {
                    r.round(&mut w, ReconcileReport::USERSTREAM_DOWN, 0, &[]);
                }
                StreamDownCall => w.risk.on_user_stream_down(acc(1), B),
            }
        }
        assert_eq!(w.risk.recon_resumable(acc(1), B), resumable, "{id}");
        if let Some(want) = want_resume {
            assert_eq!(
                w.apply(resume(CapSource::RECON, sc_av(1, B))),
                want,
                "{id} resume"
            );
        }
        (w, r)
    };
    run("RSK-202.1", &[Ignore], false, Some(Err(ModeErr::ReconNotClean)));
    run("RSK-202.2", &[Ignore, Ignore], true, Some(Ok(())));
    run("RSK-202.3", &[Ignore, Inflight, Ignore], true, Some(Ok(())));
    run(
        "RSK-202.4",
        &[Ignore, Inflight],
        false,
        Some(Err(ModeErr::ReconNotClean)),
    );
    run("RSK-202.5", &[Ignore, Warn, Ignore], false, None);
    let (mut w, mut r) = run("RSK-202.6", &[Ignore, StreamDownCall, Ignore], false, None);
    r.round(&mut w, 0, 0, &[]);
    assert!(w.risk.recon_resumable(acc(1), B), "RSK-202.6 next");
    run("RSK-202.7", &[Ignore, UsDown], false, None);
}

/// (1, B) user stream goes up at `seq`.
fn up_at(w: &mut W, seq: u64, up: bool) {
    w.apply_seq(
        ModeInput::Gate(GateEvent::OrderConns {
            account: acc(1),
            venue: B,
            primary_up: up,
            standby_up: true,
        }),
        seq,
    )
    .unwrap();
}

fn g(w: &mut W, id: u64, vs: u64, sflags: u16) {
    let items = [summary(1, B, id, vs, sflags, 0)];
    w.risk.on_reconcile(&ReconcileGroup { items: &items });
}

#[test]
fn rsk_203_auto_external_ok() {
    let down = ReconcileReport::USERSTREAM_DOWN;
    let base = |g1_view: u64, g1_flags: u16| {
        let mut w = r0();
        up_at(&mut w, 90, false);
        up_at(&mut w, 100, true);
        g(&mut w, 70_001, g1_view, g1_flags);
        w
    };
    let ok = |w: &W, sflags: u16| w.risk.auto_external_ok(acc(1), B, sflags);
    assert!(ok(&base(150, 0), 0), "RSK-203.1");
    assert!(!ok(&base(150, 0), down), "RSK-203.2");
    assert!(!ok(&base(150, down), 0), "RSK-203.3");
    let mut w = base(150, 0);
    up_at(&mut w, 160, false);
    up_at(&mut w, 170, true);
    assert!(!ok(&w, 0), "RSK-203.4");
    assert!(!ok(&base(100, 0), 0), "RSK-203.5");
    {
        let mut w = r0();
        up_at(&mut w, 90, false);
        g(&mut w, 70_001, 150, 0);
        assert!(!ok(&w, 0), "RSK-203.6");
    }
    let mut w = base(150, 0);
    w.apply_seq(
        ModeInput::RecoveryFlags {
            account: acc(1),
            flags: RecoveryFlags(0),
        },
        200,
    )
    .unwrap();
    up_at(&mut w, 300, true);
    assert!(!ok(&w, 0), "RSK-203.7 a");
    g(&mut w, 70_002, 350, 0);
    assert!(ok(&w, 0), "RSK-203.7 b");
    let mut w = base(150, 0);
    let snap = w.snapshot();
    w.restore_from(&snap);
    w.risk.set_run_start(T0 + SEC);
    assert!(ok(&w, 0), "RSK-203.8");
    {
        let mut w = W::base(w.rd, w.p.clone(), MC, CAPS);
        let snap = w.snapshot();
        w.restore_from(&snap);
        assert!(!ok(&w, 0), "RSK-203.9");
    }
}

#[test]
fn rsk_282_group_without_summary() {
    use Item::*;
    let mut w = r0();
    let mut r = Rounds::new();
    let v = r.u_group(&mut w);
    assert_eq!(
        (v.recon.is_none(), v.gate.is_none(), v.n_warn),
        (true, true, 0),
        "RSK-282.1"
    );
    let mut w = r0();
    let mut r = Rounds::new();
    r.round(&mut w, 0, 0, &[Pos("0.003", "150")]);
    r.u_group(&mut w);
    assert!(
        is_recon(&r.round(&mut w, 0, 0, &[Pos("0.003", "150")])),
        "RSK-282.2"
    );
    let start = recon_set;
    let (mut w, mut r) = start();
    r.round(&mut w, 0, 0, &[]);
    r.u_group(&mut w);
    r.round(&mut w, 0, 0, &[]);
    assert_eq!(
        w.apply(resume(CapSource::RECON, sc_av(1, B))),
        Ok(()),
        "RSK-282.3"
    );
    let (mut w, mut r) = start();
    r.round(&mut w, 0, 0, &[]);
    r.u_group(&mut w);
    assert_eq!(
        w.apply(resume(CapSource::RECON, sc_av(1, B))),
        Err(ModeErr::ReconNotClean),
        "RSK-282.4"
    );
    let mut w = g0();
    let mut r = Rounds::new();
    let v = r.u_group(&mut w);
    assert!(v.gate.is_none(), "RSK-282.5");
    assert_eq!(
        w.risk.mode.venue_bitmaps(acc(1))[0] & (1u64 << B.0),
        0,
        "RSK-282.5 RECONCILED"
    );
}

/// R0 with `Recon{1, B}` set the way grading sets it (two rounds of the same difference, which
/// also resets the Ignore streak).
fn recon_set() -> (W, Rounds) {
    let mut w = r0();
    w.mm.recon = None;
    let mut r = Rounds::new();
    r.round(&mut w, 0, 0, &[Item::Pos("0.003", "150")]);
    let v = r.round(&mut w, 0, 0, &[Item::Pos("0.003", "150")]);
    w.ok(v.recon.expect("Recon"));
    (w, r)
}
