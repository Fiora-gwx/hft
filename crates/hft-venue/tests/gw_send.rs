//! VEN-196–VEN-203, VEN-205, VEN-206, VEN-240, VEN-241, VEN-246: the gw send path.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use hft_event::{GwCmd, GwOp, SendStatus};
use hft_net::ConnId;
use hft_net::sock::{SendKind, SendStep};
use hft_types::{ClientOrderId, KillBits, SlotId};
use hft_venue::{Effect, exch_id_value};
use hft_venue_proto::{EncodeErr, VenueProtocol};
use proptest::prelude::*;
use support::gwh::*;
use support::rd::*;
use support::stub::{self, Stub, StubProto, V_SIGN};

const P: ConnId = ConnId(112);
const S: ConnId = ConnId(113);

fn texts(h: &mut Gh<StubProto>, c: ConnId) -> Vec<String> {
    h.net.sent_text(&mut h.rt, c)
}

#[test]
fn ven_196_rate_limited() {
    let mut h = gw::<StubProto>();
    let t = 3_000_000;
    for i in 0..11 {
        let c = h.place(A1, 100 + i, 1, S1, false);
        h.cmd(c);
    }
    let c = h.place(A2, 200, 1, S2, false);
    h.cmd(c);
    h.poll(t);
    h.poll(t + 1);
    let v = h.reps();
    let r = reports(&v);
    let a1: Vec<SendStatus> = r.iter().filter(|x| x.account == A1).map(|x| x.status).collect();
    assert_eq!(a1.len(), 11);
    assert!(a1[..10].iter().all(|s| *s == SendStatus::Written), "{a1:?}");
    assert_eq!(a1[10], SendStatus::RateLimited);
    assert_eq!(
        r.iter().find(|x| x.account == A2).unwrap().status,
        SendStatus::Written
    );
    let sent = texts(&mut h, P);
    assert_eq!(
        sent.iter().filter(|s| s.starts_with("P|")).count(),
        10,
        "no frame for the 11th"
    );
    // Step 2.
    h.cmd(base(GwOp::ResetRate, A1));
    let c = h.place(A1, 120, 1, S1, false);
    h.cmd(c);
    h.poll(t + 2);
    let v = h.reps();
    assert_eq!(
        statuses(&v),
        vec![SendStatus::Written],
        "ResetRate has no SendReport"
    );
    // Step 3: a response with RateObs{used 10, limit 10}.
    h.net.push(P, b"R|120-1-1-5|0|10|10|1000");
    let c = h.place(A1, 121, 1, S1, false);
    h.poll(t + 3);
    h.cmd(c);
    h.poll(t + 4);
    let v = h.reps();
    assert_eq!(statuses(&v), vec![SendStatus::RateLimited]);
}

#[test]
fn ven_197_conn_choice() {
    // .1
    let mut h = gw::<StubProto>();
    let c = h.place(A1, 1, 1, S1, false);
    h.cmd(c);
    h.poll(3_000_000);
    let r = reports(&h.reps());
    assert_eq!((r[0].status, r[0].conn), (SendStatus::Written, P), "VEN-197.1");
    // .2 and .4: the primary is not Open (Closing) → send_frame returns NotOpen → standby.
    for row in ["VEN-197.2", "VEN-197.4"] {
        let mut h = gw::<StubProto>();
        h.rt.conn(P).unwrap().test_set_phase(hft_net::ConnPhase::Closing);
        let c = h.place(A1, 1, 1, S1, false);
        h.cmd(c);
        h.poll(3_000_000);
        let r = reports(&h.reps());
        assert_eq!((r[0].status, r[0].conn), (SendStatus::Written, S), "{row}");
        assert_eq!(h.net.raw_sent_len(&mut h.rt, P), 0, "{row}: no ciphertext on 112");
    }
    // .3
    let mut h = gw::<StubProto>();
    h.rt.conn(P).unwrap().test_set_phase(hft_net::ConnPhase::Closing);
    h.rt.conn(S).unwrap().test_set_phase(hft_net::ConnPhase::Dead);
    let c = h.place(A1, 1, 1, S1, false);
    h.cmd(c);
    h.poll(3_000_000);
    let r = reports(&h.reps());
    assert_eq!(r[0].status, SendStatus::NoConn, "VEN-197.3");
    // .5: 112 TxBacklog (tx full of unsent ciphertext), 113 NotOpen.
    let mut h = gw::<StubProto>();
    {
        let c = h.rt.conn(P).unwrap();
        for _ in 0..1000 {
            c.script().unwrap().push_send(SendStep {
                post_tsc: 0,
                kind: SendKind::Eagain,
            });
        }
        // Fill tx with unsent ciphertext until it refuses more (< 78 B left).
        let small = vec![b'x'; 50];
        while c.send_text(&small, 0).is_ok() {}
        c.script().unwrap().sent.clear();
    }
    h.rt.conn(S).unwrap().test_set_phase(hft_net::ConnPhase::Closing);
    let c = h.place(A1, 1, 1, S1, false);
    h.cmd(c);
    h.poll(3_000_000);
    let r = reports(&h.reps());
    assert_eq!(r[0].status, SendStatus::NoConn, "VEN-197.5");
    assert_eq!(h.net.raw_sent_len(&mut h.rt, S), 0, "VEN-197.5");
}

#[test]
fn ven_198_write_error() {
    let mut h = gw::<StubProto>();
    h.rt.conn(P).unwrap().script().unwrap().push_send(SendStep {
        post_tsc: 0,
        kind: SendKind::Errno(104),
    });
    let c = h.place(A1, 20, 3, S1, false);
    h.cmd(c);
    h.poll(3_000_000);
    let r = reports(&h.reps());
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].op, r[0].slot, r[0].r#gen, r[0].status, r[0].os_errno),
        (GwOp::Place, SlotId(20), 3, SendStatus::WriteError, 104)
    );
    assert_eq!(h.net.raw_sent_len(&mut h.rt, S), 0, "no retry on the standby");
    let m = h.rt.mirror(20).unwrap();
    assert_eq!((m.r#gen, m.epoch, m.live), (3, 5, true));
    // Step 2: Query on the remaining connection with slot 20's client id.
    let q = h.targeted(GwOp::Query, A1, 20);
    h.cmd(q);
    h.poll(3_000_001);
    let r = reports(&h.reps());
    assert_eq!((r[0].status, r[0].conn), (SendStatus::Written, S));
    let id = ClientOrderId {
        epoch: 5,
        venue: V7,
        account: A1,
        slot: 20,
        r#gen: 3,
    };
    let sent = texts(&mut h, S);
    assert!(
        sent.iter()
            .any(|s| s.starts_with("Q|") && s.contains(&stub::cid_text(&id))),
        "{sent:?}"
    );
}

fn with_slot7() -> Gh<StubProto> {
    let mut h = gw::<StubProto>();
    let c = h.place(A1, 7, 2, S1, false);
    h.cmd(c);
    h.poll(3_000_000);
    h.reps();
    h.net.sent_text(&mut h.rt, P);
    h
}

#[test]
fn ven_199_send_report_fields() {
    // .1
    let mut h = with_slot7();
    let c = h.place(A1, 9, 1, S1, false);
    h.cmd(c);
    h.poll(4_000_000);
    let r = reports(&h.reps())[0];
    assert_eq!(
        (
            r.op,
            r.slot,
            r.r#gen,
            r.target_slot,
            r.target_gen,
            r.status,
            r.conn,
            r.venue,
            r.account
        ),
        (
            GwOp::Place,
            SlotId(9),
            1,
            SlotId(0),
            0,
            SendStatus::Written,
            P,
            V7,
            A1
        ),
        "VEN-199.1"
    );
    assert_eq!(r.sent_tsc.0, 4_000_000, "VEN-199.1 sent_tsc");
    // .2
    let mut h = with_slot7();
    let c = h.targeted(GwOp::Cancel, A1, 7);
    h.cmd(c);
    h.poll(4_000_000);
    let r = reports(&h.reps())[0];
    assert_eq!(
        (r.op, r.slot, r.r#gen, r.target_slot, r.target_gen, r.status),
        (GwOp::Cancel, SlotId(7), 2, SlotId(7), 2, SendStatus::Written),
        "VEN-199.2"
    );
    // .3
    let mut h = with_slot7();
    h.set_kill(KillBits::GLOBAL.0);
    let c = h.targeted(GwOp::Amend, A1, 7);
    h.cmd(c);
    h.poll(4_000_000);
    let r = reports(&h.reps())[0];
    assert_eq!(
        (r.op, r.target_slot, r.target_gen, r.status),
        (GwOp::Amend, SlotId(7), 2, SendStatus::KillBlocked),
        "VEN-199.3"
    );
    // .4
    let mut h = with_slot7();
    h.rt.conn(P).unwrap().test_set_phase(hft_net::ConnPhase::Closing);
    h.rt.conn(S).unwrap().test_set_phase(hft_net::ConnPhase::Closing);
    let c = h.targeted(GwOp::Query, A1, 7);
    h.cmd(c);
    h.poll(4_000_000);
    let r = reports(&h.reps())[0];
    assert_eq!(
        (r.op, r.target_slot, r.target_gen, r.status),
        (GwOp::Query, SlotId(7), 2, SendStatus::NoConn),
        "VEN-199.4"
    );
    // .5
    let mut h = with_slot7();
    let c = h.targeted(GwOp::CancelAll, A1, 0);
    h.cmd(c);
    h.poll(4_000_000);
    let r = reports(&h.reps())[0];
    assert_eq!(
        (r.op, r.target_slot, r.status),
        (GwOp::CancelAll, SlotId(0), SendStatus::Written),
        "VEN-199.5"
    );
    // .6
    let mut h = with_slot7();
    stub::set_cfg(|c| c.encode_err = Some(EncodeErr::FieldOverflow));
    let c = h.place(A1, 9, 1, S1, false);
    h.cmd(c);
    h.poll(4_000_000);
    stub::set_cfg(|c| c.encode_err = None);
    let r = reports(&h.reps())[0];
    assert_eq!(
        (r.op, r.slot, r.status),
        (GwOp::Place, SlotId(9), SendStatus::EncodeError),
        "VEN-199.6"
    );
    assert_eq!(r.status as u8, 6);
}

fn is_order(op: GwOp) -> bool {
    matches!(
        op,
        GwOp::Place | GwOp::Cancel | GwOp::Amend | GwOp::CancelAll | GwOp::Query
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1000, .. ProptestConfig::default() })]
    #[test]
    fn ven_200_one_report_per_order_cmd(
        ops in prop::collection::vec((0u8..12, 0u32..24, 0u8..4, any::<bool>(), 0u8..16), 1..500),
    ) {
        let mut h = gw::<StubProto>();
        let mut want: Vec<(u64, GwOp)> = Vec::new();
        let mut now = 3_000_000u64;
        for (k, (op, slot, acct, rr, chaos)) in ops.iter().copied().enumerate() {
            let account = [A1, A2, A9, A1][acct as usize];
            let c = match op {
                0..=3 => h.place(account, slot, 1, S1, rr),
                4 => h.targeted(GwOp::Cancel, account, slot),
                5 => h.targeted(GwOp::Amend, account, slot),
                6 => h.targeted(GwOp::Query, account, slot),
                7 => h.targeted(GwOp::CancelAll, account, 0),
                8 => GwCmd { decision_seq: h.next_seq(), ..base(GwOp::DeadManRefresh, account) },
                9 => GwCmd { decision_seq: h.next_seq(), ..base(GwOp::ResetRate, account) },
                10 => GwCmd { decision_seq: h.next_seq(), ..base(GwOp::Rotate, account) },
                _ => GwCmd { decision_seq: h.next_seq(), ..GwCmd::set_epoch(5, false, 0) },
            };
            if is_order(c.op) {
                want.push((c.decision_seq, c.op));
            }
            match chaos {
                0 => h.set_kill(KillBits::GLOBAL.0),
                1 => h.set_kill(0),
                2 => stub::set_cfg(|c| c.encode_err = Some(EncodeErr::FieldOverflow)),
                3 => stub::set_cfg(|c| c.encode_err = None),
                4 => {
                    if let Some(c) = h.rt.conn(S) {
                        c.test_set_phase(hft_net::ConnPhase::Closing);
                    }
                }
                5 => {
                    if let Some(c) = h.rt.conn(P)
                        && let Some(s) = c.script()
                    {
                        s.push_send(SendStep { post_tsc: 0, kind: SendKind::Errno(104) });
                    }
                }
                _ => {}
            }
            h.cmd(c);
            if k % 7 == 0 {
                now += 1_000;
                h.poll(now);
            }
        }
        for _ in 0..200 {
            now += 1_000;
            h.poll(now);
        }
        stub::set_cfg(|c| c.encode_err = None);
        let v = h.reps();
        let mut got: Vec<(u64, GwOp)> = reports(&v).iter().map(|r| (r.decision_seq, r.op)).collect();
        got.sort_by_key(|x| x.0);
        want.sort_by_key(|x| x.0);
        prop_assert_eq!(got, want);
    }
}

#[test]
fn ven_201_missing_target() {
    let setup = || {
        let mut h = gw::<StubProto>();
        stub::set_cfg(|c| c.query_by_cid = false);
        let c = h.place(A1, 41, 1, S1, false);
        h.cmd(c);
        h.poll(3_000_000);
        h.reps();
        h.net.sent_text(&mut h.rt, P);
        stub::take_log();
        h
    };
    for (row, op, slot) in [("VEN-201.1", GwOp::Cancel, 40), ("VEN-201.2", GwOp::Amend, 40)] {
        let mut h = setup();
        let c = h.targeted(op, A1, slot);
        h.cmd(c);
        h.poll(4_000_000);
        assert_eq!(statuses(&h.reps()), vec![SendStatus::EncodeError], "{row}");
        assert!(stub::calls("encode").is_empty(), "{row}: encode not called");
        assert!(texts(&mut h, P).is_empty(), "{row}: no frame");
    }
    let mut h = setup();
    let c = h.targeted(GwOp::Cancel, A1, 41);
    h.cmd(c);
    h.poll(4_000_000);
    assert_eq!(statuses(&h.reps()), vec![SendStatus::EncodeError], "VEN-201.3");
    assert_eq!(
        stub::calls("encode").len(),
        1,
        "VEN-201.3: encode returned UnknownTarget"
    );
    assert!(texts(&mut h, P).is_empty());
    stub::set_cfg(|c| c.query_by_cid = true);
}

#[test]
fn ven_202_query_unsupported() {
    let setup = |by_cid: bool| {
        let mut h = gw::<StubProto>();
        let c = h.place(A1, 41, 1, S1, false);
        h.cmd(c);
        h.poll(3_000_000);
        h.reps();
        h.net.sent_text(&mut h.rt, P);
        stub::set_cfg(|c| c.query_by_cid = by_cid);
        h
    };
    let mut h = setup(false);
    let c = h.targeted(GwOp::Query, A1, 40);
    h.cmd(c);
    h.poll(4_000_000);
    let r = reports(&h.reps())[0];
    assert_eq!(
        (r.op, r.target_slot, r.status),
        (GwOp::Query, SlotId(40), SendStatus::QueryUnsupported),
        "VEN-202.1"
    );
    assert_eq!(r.status as u8, 8);
    assert!(texts(&mut h, P).is_empty());
    let c = h.targeted(GwOp::Query, A1, 41);
    h.cmd(c);
    h.poll(4_000_001);
    assert_eq!(
        statuses(&h.reps()),
        vec![SendStatus::QueryUnsupported],
        "VEN-202.2"
    );
    assert!(texts(&mut h, P).is_empty());
    let mut h = setup(true);
    let c = h.targeted(GwOp::Query, A1, 41);
    h.cmd(c);
    h.poll(4_000_000);
    assert_eq!(statuses(&h.reps()), vec![SendStatus::Written], "VEN-202.3");
    let id = ClientOrderId {
        epoch: 5,
        venue: V7,
        account: A1,
        slot: 41,
        r#gen: 1,
    };
    assert!(
        texts(&mut h, P)
            .iter()
            .any(|s| s.starts_with("Q|") && s.contains(&stub::cid_text(&id)))
    );
}

#[test]
fn ven_203_mirror_lifecycle() {
    let mut h = gw::<StubProto>();
    let c = h.place(A1, 50, 4, S1, false);
    h.cmd(c);
    h.poll(3_000_000);
    let m = h.rt.mirror(50).unwrap();
    assert_eq!(
        (m.r#gen, m.epoch, m.inst, m.exch_order_id, m.live),
        (4, 5, S1, 0, true)
    );
    let id = ClientOrderId {
        epoch: 5,
        venue: V7,
        account: A1,
        slot: 50,
        r#gen: 4,
    };
    let cid = stub::cid_text(&id);
    h.net.push(P, format!("O|{cid}|EX-77|Ack|0|0|0|0|0|0").as_bytes());
    h.poll(3_000_001);
    let m = h.rt.mirror(50).unwrap();
    assert_eq!(m.exch_order_id, exch_id_value(b"EX-77"));
    assert_eq!(h.rt.id_table(A1).unwrap().get(b"EX-77"), Some((5, 50, 4)));
    h.net
        .push(P, format!("O|{cid}|EX-77|Fill|1|1|1|0|0|9").as_bytes());
    h.poll(3_000_002);
    assert!(!h.rt.mirror(50).unwrap().live);
    h.reps();
    let c = h.targeted(GwOp::Cancel, A1, 50);
    h.cmd(c);
    h.poll(3_000_003);
    assert_eq!(statuses(&h.reps()), vec![SendStatus::EncodeError]);
}

#[test]
fn ven_205_send_order_of_effects() {
    type Signed = Stub<2048, V_SIGN>;
    let mut h = gw_opts::<Signed>(GhOpts {
        tx_tags: true,
        ..GhOpts::default()
    });
    let c = h.place(A1, 60, 1, S1, false);
    h.cmd(c);
    h.rt.trace(true);
    h.poll(3_000_000);
    let tr = h.rt.take_trace();
    let send = tr.iter().position(|e| *e == Effect::Send(P)).unwrap();
    let rep = tr.iter().position(|e| *e == Effect::Rep).unwrap();
    let tx = tr.iter().position(|e| *e == Effect::RawTx).unwrap();
    assert!(send < rep && rep < tx, "{tr:?}");
    let tags: Vec<_> = std::iter::from_fn(|| h.tags.as_mut().unwrap().try_pop()).collect();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].slot, 60);
    assert_eq!(h.rt.inflight(P), 1);
    let r = reports(&h.reps())[0];
    let rec = h
        .raw
        .records()
        .into_iter()
        .rev()
        .find(|r| r.kind == hft_event::EventKind::RAW_TX)
        .unwrap();
    assert_eq!(rec.tsc, r.sent_tsc.0);
    assert_ne!(rec.hdr.rflags & hft_event::RawHdr::REDACTED, 0);
    assert!(rec.payload.is_empty(), "metadata only");
}

#[test]
fn ven_206_cancel_all() {
    let setup = |acct_ca: bool, inst_ca: bool| {
        let mut h = gw::<StubProto>();
        stub::set_cfg(|c| {
            c.account_cancel_all = acct_ca;
            c.inst_cancel_all = inst_ca;
        });
        for (a, slot, inst) in [(A1, 1, S1), (A1, 2, S1), (A1, 3, S2), (A2, 4, S3)] {
            let c = h.place(a, slot, 1, inst, false);
            h.cmd(c);
        }
        h.poll(3_000_000);
        h.reps();
        h.net.sent_text(&mut h.rt, P);
        h.net.sent_text(&mut h.rt, ConnId(114));
        stub::take_log();
        h
    };
    // .1
    let mut h = setup(true, true);
    let c = h.targeted(GwOp::CancelAll, A1, 0);
    h.cmd(c);
    h.poll(4_000_000);
    let sent = texts(&mut h, P);
    assert_eq!(sent.len(), 1, "VEN-206.1 {sent:?}");
    assert_eq!(
        reports(&h.reps())
            .iter()
            .map(|r| (r.op, r.status))
            .collect::<Vec<_>>(),
        vec![(GwOp::CancelAll, SendStatus::Written)]
    );
    // .2
    let mut h = setup(false, true);
    let c = h.targeted(GwOp::CancelAll, A1, 0);
    h.cmd(c);
    h.poll(4_000_000);
    let enc: Vec<String> = stub::calls("encode").into_iter().map(|c| c.arg).collect();
    assert_eq!(
        enc,
        vec![
            "CancelAll slot=0 inst=0",
            "CancelAll slot=0 inst=701",
            "CancelAll slot=0 inst=702"
        ],
        "VEN-206.2"
    );
    let sent = texts(&mut h, P);
    assert_eq!(sent.len(), 2, "VEN-206.2 (no S3)");
    assert_eq!(statuses(&h.reps()), vec![SendStatus::Written]);
    // .3 with encode_batch Ok, then Unsupported.
    for (batch, frames) in [(true, 1usize), (false, 3)] {
        let mut h = setup(false, false);
        stub::set_cfg(|c| c.encode_batch_ok = batch);
        let c = h.targeted(GwOp::CancelAll, A1, 0);
        h.cmd(c);
        h.poll(4_000_000);
        stub::set_cfg(|c| c.encode_batch_ok = true);
        let sent = texts(&mut h, P);
        assert_eq!(sent.len(), frames, "VEN-206.3 batch={batch} {sent:?}");
        let r = reports(&h.reps());
        assert_eq!(
            r.iter().map(|r| (r.op, r.status)).collect::<Vec<_>>(),
            vec![(GwOp::CancelAll, SendStatus::Written)],
            "VEN-206.3"
        );
    }
    stub::set_cfg(|c| {
        c.account_cancel_all = true;
        c.inst_cancel_all = true;
    });
}

#[test]
fn ven_240_cid_generation() {
    let mut h = gw::<StubProto>();
    let c = h.place(A2, 33, 7, S1, false);
    h.cmd(c);
    h.poll(3_000_000);
    let id = ClientOrderId {
        epoch: 5,
        venue: V7,
        account: A2,
        slot: 33,
        r#gen: 7,
    };
    let sent = texts(&mut h, ConnId(114));
    let f = sent.iter().find(|s| s.starts_with("P|")).unwrap();
    let cid = f.split('|').nth(2).unwrap();
    let mut canon = [0u8; 24];
    id.encode(&mut canon);
    assert_eq!(cid, format!("st-{}", String::from_utf8_lossy(&canon)));
    assert_eq!(StubProto::parse_cid(cid.as_bytes()), Some(id));
}

#[test]
fn ven_241_foreign_account() {
    let mut h = gw::<StubProto>();
    stub::take_log();
    let c = h.place(A9, 5, 1, S1, false);
    h.cmd(c);
    h.poll(3_000_000);
    let r = reports(&h.reps());
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].op, r[0].slot, r[0].account, r[0].status),
        (GwOp::Place, SlotId(5), A9, SendStatus::NoConn)
    );
    assert!(stub::calls("encode").is_empty());
    assert!(texts(&mut h, P).is_empty() && texts(&mut h, ConnId(114)).is_empty());
    assert_eq!(h.rt.counters().cmd_bad_account, 1);
}

#[test]
fn ven_246_wall_clock_and_expiry() {
    let mut h = gw::<StubProto>();
    stub::take_log();
    let now = 1_700_000_000_123_456_789u64;
    let s0 = *h.rt.conn(P).unwrap().sys();
    let c = h.place(A1, 1, 1, S1, false);
    h.cmd(c);
    h.poll(now);
    let e = stub::calls("encode");
    let clk = e[0].clk.unwrap();
    assert_eq!(
        (clk.unix_ms, clk.unix_us),
        (1_700_000_000_123, 1_700_000_000_123_456)
    );
    let f = texts(&mut h, P)
        .into_iter()
        .find(|s| s.starts_with("P|"))
        .unwrap();
    let exp: i64 = f.split('|').nth(7).unwrap().parse().unwrap();
    assert_eq!(exp, 1_700_000_000_123 + 1500);
    // Only socket I/O on the gw's connection: no clock (or other) system call (T4; clippy-enforced).
    let s1 = *h.rt.conn(P).unwrap().sys();
    assert_eq!(
        s1.total() - s0.total(),
        (s1.recvmsg - s0.recvmsg) + (s1.sendmsg - s0.sendmsg)
    );
}
