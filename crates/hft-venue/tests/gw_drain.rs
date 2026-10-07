//! VEN-190, VEN-191: `drain_out` batch reordering and per-account quotas.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

mod support;

use hft_event::{GwCmd, GwOp, SendStatus};
use hft_net::ConnId;
use support::gwh::*;
use support::rd::*;
use support::stub::{self, StubProto};

const P: ConnId = ConnId(112);

/// `P5` = Place slot 5, `C9` = Cancel of slot 9, `A5`, `Q5`, `DM`.
fn label(f: &str) -> String {
    let mut it = f.split('|');
    let k = it.next().unwrap_or("");
    if k == "DM" {
        return "DM".into();
    }
    let slot = it.next().and_then(|r| r.split('-').next()).unwrap_or("?");
    format!("{k}{slot}")
}

fn prep(slots: &[u32]) -> Gh<StubProto> {
    let mut h = gw::<StubProto>();
    for &s in slots {
        let c = h.place(A1, s, 1, S1, false);
        h.cmd(c);
    }
    h.poll(3_000_000);
    h.reps();
    h.net.sent_text(&mut h.rt, P);
    h
}

#[test]
#[allow(clippy::type_complexity)]
fn ven_190_batch_reorder() {
    let dm = || GwCmd {
        arg: 30_000,
        ..base(GwOp::DeadManRefresh, A1)
    };
    let rows: Vec<(
        &str,
        Vec<u32>,
        Box<dyn Fn(&mut Gh<StubProto>) -> Vec<GwCmd>>,
        Vec<&str>,
    )> = vec![
        (
            "VEN-190.1",
            vec![9],
            Box::new(|h: &mut Gh<StubProto>| {
                vec![
                    h.place(A1, 1, 1, S1, false),
                    h.place(A1, 2, 1, S1, true),
                    h.targeted(GwOp::Cancel, A1, 9),
                    h.place(A1, 4, 1, S1, true),
                ]
            }),
            vec!["P2", "P4", "P1", "C9"],
        ),
        (
            "VEN-190.2",
            vec![9],
            Box::new(|h: &mut Gh<StubProto>| {
                let mut a3 = h.targeted(GwOp::Amend, A1, 5);
                a3.oflags = hft_event::OrderFlags::RISK_REDUCING.0;
                vec![
                    h.place(A1, 5, 1, S1, false),
                    h.targeted(GwOp::Cancel, A1, 9),
                    a3,
                    h.place(A1, 7, 1, S1, true),
                ]
            }),
            vec!["P7", "P5", "C9", "A5"],
        ),
        (
            "VEN-190.3",
            vec![3],
            Box::new(|h: &mut Gh<StubProto>| {
                let mut a1 = h.targeted(GwOp::Amend, A1, 3);
                a1.oflags = hft_event::OrderFlags::RISK_REDUCING.0;
                vec![a1, h.place(A1, 2, 1, S1, false)]
            }),
            vec!["A3", "P2"],
        ),
        (
            "VEN-190.4",
            vec![],
            Box::new(|h: &mut Gh<StubProto>| {
                vec![
                    h.place(A1, 5, 1, S1, false),
                    h.targeted(GwOp::Query, A1, 5),
                    h.place(A1, 3, 1, S1, true),
                ]
            }),
            vec!["P3", "P5", "Q5"],
        ),
        (
            "VEN-190.5",
            vec![1],
            Box::new(move |h: &mut Gh<StubProto>| vec![dm(), h.place(A1, 11, 1, S1, true)]),
            vec!["P11", "DM"],
        ),
        (
            "VEN-190.6",
            vec![],
            Box::new(|h: &mut Gh<StubProto>| {
                vec![h.place(A1, 5, 1, S1, true), h.targeted(GwOp::Cancel, A1, 5)]
            }),
            vec!["P5", "C5"],
        ),
    ];
    for (row, pre, mk, want) in rows {
        let mut h = prep(&pre);
        for c in mk(&mut h) {
            h.cmd(c);
        }
        h.poll(4_000_000);
        let got: Vec<String> = h.net.sent_text(&mut h.rt, P).iter().map(|f| label(f)).collect();
        assert_eq!(got, want, "{row}");
    }
}

#[test]
fn ven_191_per_account_quota() {
    let mut h = gw::<StubProto>();
    let a: Vec<GwCmd> = (1..=20).map(|i| h.place(A1, i, 1, S1, false)).collect();
    let b: Vec<GwCmd> = (1..=2).map(|i| h.place(A2, 100 + i, 1, S2, false)).collect();
    for c in a.iter().chain(b.iter()) {
        h.cmd(*c);
    }
    let a2 = ConnId(114);
    let mut rounds = Vec::new();
    for k in 0..3u64 {
        // One second between rounds: the 10/s order bucket is not what this case is about.
        h.poll(3_000_000 + k * 1_000_000_001);
        let s1: Vec<String> = h.net.sent_text(&mut h.rt, P).iter().map(|f| label(f)).collect();
        let s2: Vec<String> = h.net.sent_text(&mut h.rt, a2).iter().map(|f| label(f)).collect();
        rounds.push((s1, s2));
    }
    let p = |r: std::ops::RangeInclusive<u32>| r.map(|i| format!("P{i}")).collect::<Vec<_>>();
    assert_eq!(rounds[0].0, p(1..=8));
    assert_eq!(rounds[0].1, vec!["P101", "P102"]);
    assert_eq!(rounds[1].0, p(9..=16));
    assert_eq!(rounds[2].0, p(17..=20));
    let r = reports(&h.reps());
    assert_eq!(r.len(), 22);
    assert!(r.iter().all(|x| x.status == SendStatus::Written));
    let _ = stub::cfg();
}
