//! VEN-192–VEN-195: `kill_admit` and the gw's kill check.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use std::sync::atomic::Ordering;

use hft_event::{GwOp, OrderFlags, SendStatus};
use hft_types::{KillBits, SlotId};
use hft_venue::kill_admit;
use proptest::prelude::*;
use support::gwh::*;
use support::rd::*;
use support::stub::{self, StubProto};

const MASK: u32 = 0x7;
const RR: u16 = OrderFlags::RISK_REDUCING.0;

fn bits(ix: u8) -> u32 {
    KillBits::account(ix).0 | KillBits::account_hard(ix).0
}

#[test]
fn ven_192_kill_admit_table() {
    assert_eq!(bits(0), 0x0010_0100);
    assert_eq!(bits(11), 0x8008_0000);
    let g = KillBits::GLOBAL.0;
    let v = KillBits::VENUE.0;
    let h = KillBits::HARD.0;
    let a = |ix| KillBits::account(ix).0;
    let ah = |ix| KillBits::account_hard(ix).0;
    let k =
        |w: u32, ix: u8, op: GwOp, rr: bool| kill_admit(w, MASK, bits(ix), ix, op, if rr { RR } else { 0 });
    let rows: &[(&str, bool, bool)] = &[
        ("VEN-192.1", k(0, 0, GwOp::Place, false), true),
        ("VEN-192.2a", k(g, 0, GwOp::Place, false), false),
        ("VEN-192.2b", k(g, 0, GwOp::Place, true), true),
        ("VEN-192.3a", k(v, 0, GwOp::Amend, false), false),
        ("VEN-192.3b", k(v, 0, GwOp::Amend, true), true),
        ("VEN-192.4a", k(a(0), 0, GwOp::Place, false), false),
        ("VEN-192.4b", k(a(0), 0, GwOp::Place, true), true),
        ("VEN-192.5", k(a(1), 0, GwOp::Place, false), true),
        ("VEN-192.6a", k(h, 0, GwOp::Place, true), false),
        ("VEN-192.6b", k(h, 0, GwOp::Amend, true), false),
        ("VEN-192.7", k(ah(0), 0, GwOp::Place, true), false),
        ("VEN-192.8", k(ah(1), 0, GwOp::Place, false), true),
        ("VEN-192.9a", k(g | h | ah(0), 0, GwOp::Cancel, false), true),
        ("VEN-192.9b", k(g | h | ah(0), 0, GwOp::CancelAll, false), true),
        ("VEN-192.9c", k(g | h | ah(0), 0, GwOp::Query, false), true),
        ("VEN-192.10a", k(h, 0, GwOp::DeadManRefresh, false), true),
        ("VEN-192.10b", k(h, 0, GwOp::Rotate, false), true),
        ("VEN-192.10c", k(h, 0, GwOp::SetEpoch, false), true),
        ("VEN-192.10d", k(h, 0, GwOp::MirrorRestore, false), true),
        ("VEN-192.11a", k(a(11), 11, GwOp::Place, true), true),
        ("VEN-192.11b", k(a(11), 11, GwOp::Place, false), false),
        ("VEN-192.12", k(ah(11), 11, GwOp::Place, true), false),
        ("VEN-192.13", k(0xF8, 0, GwOp::Place, false), true),
    ];
    for (row, got, want) in rows {
        assert_eq!(got, want, "{row}");
    }
}

fn reference(word: u32, ix: u8, op: GwOp, rr: bool) -> bool {
    if !matches!(op, GwOp::Place | GwOp::Amend) {
        return true;
    }
    let hard = word & (KillBits::HARD.0 | KillBits::account_hard(ix).0) != 0;
    let soft = word & (KillBits::GLOBAL.0 | KillBits::VENUE.0 | KillBits::account(ix).0) != 0;
    !hard && (!soft || rr)
}

const OPS: [GwOp; 9] = [
    GwOp::Place,
    GwOp::Amend,
    GwOp::Cancel,
    GwOp::CancelAll,
    GwOp::Query,
    GwOp::DeadManRefresh,
    GwOp::Rotate,
    GwOp::SetEpoch,
    GwOp::ResetRate,
];

proptest! {
    #![proptest_config(ProptestConfig { cases: 10_000, .. ProptestConfig::default() })]
    #[test]
    fn ven_193_kill_admit_matches_projection(word in any::<u32>(), ix in 0u8..12, k in 0usize..9, rr in any::<bool>()) {
        // Exhaustive part (32 single-bit words × ix ∈ {0, 11} × {Place, Amend} × RR) once.
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let mut n = 0;
            for b in 0..32 {
                for ix in [0u8, 11] {
                    for op in [GwOp::Place, GwOp::Amend] {
                        for rr in [false, true] {
                            let w = 1u32 << b;
                            let got = kill_admit(w, MASK, bits(ix), ix, op, if rr { RR } else { 0 });
                            assert_eq!(got, reference(w, ix, op, rr), "bit {b} ix {ix} {op:?} rr {rr}");
                            n += 1;
                        }
                    }
                }
            }
            assert_eq!(n, 256);
        });
        let op = OPS[k];
        prop_assert_eq!(kill_admit(word, MASK, bits(ix), ix, op, if rr { RR } else { 0 }), reference(word, ix, op, rr));
    }
}

#[test]
fn ven_194_kill_blocked() {
    let mut h = gw::<StubProto>();
    let c = h.place(A1, 3, 1, S1, false);
    h.cmd(c);
    h.poll(3_000_000);
    h.reps();
    h.net.sent_text(&mut h.rt, hft_net::ConnId(112));
    h.set_kill(KillBits::account(0).0);
    stub::take_log();
    let mut p10 = h.place(A1, 10, 1, S1, false);
    p10.decision_seq = 100;
    let p11 = h.place(A1, 11, 1, S1, true);
    let p12 = h.place(A2, 12, 1, S2, false);
    let c3 = h.targeted(GwOp::Cancel, A1, 3);
    for c in [p10, p11, p12, c3] {
        h.cmd(c);
    }
    h.poll(3_000_001);
    let r = reports(&h.reps());
    let s10 = r.iter().find(|x| x.slot == SlotId(10)).unwrap();
    assert_eq!(
        (s10.decision_seq, s10.op, s10.status),
        (100, GwOp::Place, SendStatus::KillBlocked)
    );
    assert_eq!(s10.status as u8, 2);
    let enc = stub::calls("encode");
    assert!(
        !enc.iter().any(|c| c.arg.contains("slot=10")),
        "encode not called for slot 10"
    );
    for s in [11, 12] {
        assert_eq!(
            r.iter().find(|x| x.slot == SlotId(s)).unwrap().status,
            SendStatus::Written
        );
    }
    assert_eq!(
        r.iter().find(|x| x.op == GwOp::Cancel).unwrap().status,
        SendStatus::Written
    );
    let sent = h.net.sent_text(&mut h.rt, hft_net::ConnId(112));
    assert_eq!(sent.len(), 2, "slot 11 and the Cancel: {sent:?}");
}

#[test]
fn ven_195_kill_reloaded_per_cmd() {
    let mut h = gw::<StubProto>();
    let kw = h.kill;
    let mut first = true;
    stub::set_encode_hook(Some(Box::new(move |_c| {
        if first {
            kw.store(KillBits::GLOBAL.0, Ordering::Release);
            first = false;
        }
    })));
    let p1 = h.place(A1, 1, 1, S1, false);
    let p2 = h.place(A1, 2, 1, S1, false);
    h.cmd(p1);
    h.cmd(p2);
    h.poll(3_000_000);
    stub::set_encode_hook(None);
    let r = reports(&h.reps());
    assert_eq!(
        r.iter().find(|x| x.slot == SlotId(1)).unwrap().status,
        SendStatus::Written
    );
    assert_eq!(
        r.iter().find(|x| x.slot == SlotId(2)).unwrap().status,
        SendStatus::KillBlocked
    );
}
