//! NET-150–NET-162: the send path — one encryption and one `sendmsg` per frame, short writes,
//! `EAGAIN`, iovecs, errors, phase and backlog checks, `flush`, `send_text`, `send_ping`.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::support;

use hft_net::sock::{SendKind, SendStep, errno};
use hft_net::{ConnPhase, ErrBits, MaskedFrame, Opcode, SendErr, SendOk};
use support::mempeer::{MemPeer, cfg, deliver, pair, pair_cfg};
use support::ws_ref::text;

fn frame<const C: usize>(payload: &[u8], key: u32) -> Box<MaskedFrame<C>> {
    let mut f = Box::new(MaskedFrame::<C>::new());
    f.body_mut()[..payload.len()].copy_from_slice(payload);
    f.seal(Opcode::Text, payload.len(), key);
    f
}

fn push_send(conn: &mut hft_net::Conn, post_tsc: u64, kind: SendKind) {
    conn.script().unwrap().push_send(SendStep { post_tsc, kind });
}

fn last_iovs(conn: &hft_net::Conn) -> (u8, [u32; 2]) {
    let c = conn.sys().sendmsg_log.last().unwrap();
    (c.iovs, c.lens)
}

fn payloads(peer: &mut MemPeer) -> Vec<Vec<u8>> {
    peer.client_frames().into_iter().map(|f| f.payload).collect()
}

#[test]
fn net_150_send_once() {
    let (mut conn, mut peer) = pair();
    push_send(&mut conn, 7_777, SendKind::All);
    let f = frame::<2048>(b"order", 9);
    assert_eq!(
        conn.send_frame(&*f),
        Ok(SendOk {
            sent_tsc: 7_777,
            queued: 0
        })
    );
    assert_eq!(conn.sys().sendmsg, 1);
    assert_eq!(last_iovs(&conn).0, 1);
    peer.pump(&mut conn);
    assert_eq!(payloads(&mut peer), vec![b"order".to_vec()]);
    assert!(!conn.err().contains(ErrBits::TX_PARTIAL));
}

#[test]
fn net_151_tls_seq_advances_only_on_send() {
    let mut c = cfg();
    c.tx_buf = 4096;
    let (mut conn, mut peer) = pair_cfg(c);
    conn.send_frame(&*frame::<2048>(b"f1", 1)).unwrap();
    let seq = conn.test_tx_seq();
    let f2 = frame::<2048>(b"warm", 2);
    for _ in 0..1_000_000 {
        conn.warm(&*f2);
    }
    let _f4 = frame::<2048>(b"f4", 4);
    let mut unsealed = MaskedFrame::<2048>::new();
    unsealed.body_mut()[..2].copy_from_slice(b"u!");
    assert_eq!(conn.send_frame(&unsealed), Err(SendErr::NotSealed));
    let frames: Vec<Vec<u8>> = (0..100).map(|i| text(format!("s{i}").as_bytes())).collect();
    let b = peer.frames(&frames);
    deliver(&mut conn, 0, 0, b);
    let mut n = 0;
    while n < 100 {
        conn.poll_rx(0);
        while conn.next_frame().is_some() {
            n += 1;
        }
        conn.finish_poll(0);
    }
    assert_eq!(
        conn.test_tx_seq(),
        seq,
        "step 2 left the send sequence number alone"
    );
    conn.send_frame(&*frame::<2048>(b"f3", 3)).unwrap();
    peer.pump(&mut conn);
    let idx: Vec<u64> = peer.recs.iter().map(|r| r.index).collect();
    assert_eq!(idx, vec![0, 1]);
    assert_eq!(payloads(&mut peer), vec![b"f1".to_vec(), b"f3".to_vec()]);
}

/// A sealed frame whose record is 322 B of ciphertext (`as_bytes` 300 B).
fn f322(p: &[u8; 1]) -> Box<MaskedFrame<2048>> {
    let mut body = vec![p[0]; 292];
    body[0] = b'#';
    frame::<2048>(&body, 5)
}

#[test]
fn net_152_short_write_resumes() {
    let (mut conn, mut peer) = pair();
    push_send(&mut conn, 1, SendKind::Bytes(100));
    let f = f322(b"a");
    assert_eq!(f.as_bytes().len(), 300);
    let r = conn.send_frame(&*f).unwrap();
    assert_eq!(r.queued, 222);
    assert!(conn.err().contains(ErrBits::TX_PARTIAL));
    assert_eq!(conn.stats().short_writes, 1);
    conn.finish_poll(0);
    assert_eq!(conn.sys().sendmsg, 2);
    assert_eq!(last_iovs(&conn).1[0], 222);
    peer.pump(&mut conn);
    assert_eq!(peer.recs.len(), 1, "one record: no re-encryption");
    assert_eq!(payloads(&mut peer).len(), 1);
}

#[test]
fn net_153_eagain_queues_all() {
    let (mut conn, mut peer) = pair();
    push_send(&mut conn, 1, SendKind::Eagain);
    let r = conn.send_frame(&*f322(b"b")).unwrap();
    assert_eq!(r.queued, 322);
    assert!(conn.err().contains(ErrBits::TX_PARTIAL));
    conn.finish_poll(0);
    peer.pump(&mut conn);
    assert_eq!(payloads(&mut peer).len(), 1);
}

#[test]
fn net_154_two_iovecs_after_partial() {
    let (mut conn, mut peer) = pair();
    push_send(&mut conn, 1, SendKind::Bytes(100));
    conn.send_frame(&*f322(b"1")).unwrap();
    let f2 = frame::<2048>(b"second", 6);
    conn.send_frame(&*f2).unwrap();
    let (iovs, lens) = last_iovs(&conn);
    assert_eq!(iovs, 2);
    assert_eq!(lens, [222, (f2.as_bytes().len() + 22) as u32]);
    peer.pump(&mut conn);
    let p = payloads(&mut peer);
    assert_eq!(p.len(), 2);
    assert_eq!(p[1], b"second");
}

#[test]
fn net_155_send_errno() {
    let rows = [
        ("NET-155.1", errno::ECONNRESET, ErrBits::RESET),
        ("NET-155.2", errno::EPIPE, ErrBits::RESET),
        ("NET-155.3", errno::ENOBUFS, ErrBits::IO),
    ];
    for (row, e, bit) in rows {
        let (mut conn, _peer) = pair();
        push_send(&mut conn, 1, SendKind::Errno(e));
        let seq = conn.test_tx_seq().unwrap();
        assert_eq!(
            conn.send_frame(&*frame::<2048>(b"x", 1)),
            Err(SendErr::Io(e)),
            "{row}"
        );
        assert!(conn.err().contains(bit), "{row}");
        assert_eq!(conn.test_tx_seq().unwrap(), seq + 1, "{row}");
        if bit == ErrBits::IO {
            assert!(!conn.err().contains(ErrBits::RESET), "{row}");
            assert_eq!(conn.last_errno(), e, "{row}");
        }
        assert!(conn.tx_pending() > 0, "{row}: ciphertext stays in tx");
    }
}

#[test]
fn net_156_not_open() {
    for (row, p) in [
        ("NET-156.1", ConnPhase::Closing),
        ("NET-156.2", ConnPhase::Dead),
        ("NET-156.3", ConnPhase::WsUpgrade),
    ] {
        let (mut conn, _peer) = pair();
        conn.test_set_phase(p);
        let seq = conn.test_tx_seq();
        assert_eq!(
            conn.send_frame(&*frame::<2048>(b"x", 1)),
            Err(SendErr::NotOpen(p)),
            "{row}"
        );
        assert_eq!(conn.sys().sendmsg, 0, "{row}");
        assert_eq!(conn.test_tx_seq(), seq, "{row}");
    }
}

#[test]
fn net_157_tx_backlog_boundary() {
    let rows = [
        ("NET-157.1", 3073usize, Some(4095u32)),
        ("NET-157.2", 3074, Some(4096)),
        ("NET-157.3", 3075, None),
    ];
    for (row, bytes, want) in rows {
        let mut c = cfg();
        c.tx_buf = 4096;
        let (mut conn, _peer) = pair_cfg(c);
        // 1 000 B of ciphertext stuck in tx (as_bytes 978 → record 1 000 B).
        push_send(&mut conn, 1, SendKind::Eagain);
        conn.send_frame(&*frame::<2048>(&vec![1u8; 970], 1)).unwrap();
        assert_eq!(conn.tx_pending(), 1000, "{row}");
        push_send(&mut conn, 1, SendKind::Eagain);
        let f = frame::<8192>(&vec![2u8; bytes - 8], 2);
        assert_eq!(f.as_bytes().len(), bytes, "{row}");
        let seq = conn.test_tx_seq();
        match want {
            Some(q) => assert_eq!(conn.send_frame(&*f).map(|o| o.queued), Ok(q), "{row}"),
            None => {
                assert_eq!(conn.send_frame(&*f), Err(SendErr::TxBacklog), "{row}");
                assert!(conn.err().contains(ErrBits::TX_BACKLOG), "{row}");
                assert_eq!(conn.test_tx_seq(), seq, "{row}");
                assert_eq!(conn.tx_pending(), 1000, "{row}");
            }
        }
    }
}

#[test]
fn net_158_encrypt_failure() {
    let (mut conn, _peer) = pair();
    conn.script().unwrap().fail_encrypt = 1;
    let pending = conn.tx_pending();
    assert_eq!(conn.send_frame(&*frame::<2048>(b"x", 1)), Err(SendErr::Encrypt));
    assert!(conn.err().contains(ErrBits::TLS_PROTO));
    assert_eq!(conn.sys().sendmsg, 0);
    assert_eq!(conn.tx_pending(), pending);
}

#[test]
fn net_159_flush() {
    let rows = [
        ("NET-159.1", Some(SendKind::All), Ok(0u32)),
        ("NET-159.2", Some(SendKind::Bytes(100)), Ok(122)),
        ("NET-159.3", Some(SendKind::Eagain), Ok(222)),
        (
            "NET-159.4",
            Some(SendKind::Errno(errno::ECONNRESET)),
            Err(SendErr::Io(104)),
        ),
        ("NET-159.5", None, Ok(0)),
    ];
    for (row, kind, want) in rows {
        let (mut conn, mut peer) = pair();
        match kind {
            Some(k) => {
                push_send(&mut conn, 1, SendKind::Bytes(100));
                conn.send_frame(&*f322(b"q")).unwrap();
                assert_eq!(conn.tx_pending(), 222, "{row}");
                push_send(&mut conn, 1, k);
                assert_eq!(conn.flush(), want, "{row}");
                if k == SendKind::All {
                    peer.pump(&mut conn);
                    assert_eq!(peer.recs.len(), 1, "{row}");
                }
                if want.is_err() {
                    assert!(conn.err().contains(ErrBits::RESET), "{row}");
                }
            }
            None => {
                let n = conn.sys().sendmsg;
                assert_eq!(conn.flush(), want, "{row}");
                assert_eq!(conn.sys().sendmsg, n, "{row}");
            }
        }
    }
}

#[test]
fn net_160_send_text() {
    let (mut conn, mut peer) = pair();
    let msg = b"{\"op\":\"subscribe\"}";
    assert!(conn.send_text(msg, 0).is_ok());
    peer.pump(&mut conn);
    let f = peer.client_frames();
    assert_eq!(f.len(), 1);
    assert_eq!(
        (f[0].opcode, f[0].masked, f[0].payload.as_slice()),
        (0x1, true, &msg[..])
    );
}

#[test]
fn net_161_send_ping_payload() {
    let (mut conn, mut peer) = pair();
    conn.send_ping(0x0102_0304_0506_0708).unwrap();
    peer.pump(&mut conn);
    let f = peer.client_frames();
    assert_eq!(
        (f[0].opcode, f[0].payload.as_slice()),
        (0x9, &[8u8, 7, 6, 5, 4, 3, 2, 1][..])
    );
}

#[test]
fn net_162_send_does_not_touch_rx() {
    let (mut conn, mut peer) = pair();
    let x: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
    let b = peer.records(&[&text(&x)]);
    let half = b.len() / 2;
    deliver(&mut conn, 0, 0, b[..half].to_vec());
    conn.poll_rx(0);
    assert!(conn.next_frame().is_none());
    conn.send_frame(&*frame::<2048>(b"o1", 1)).unwrap();
    conn.send_frame(&*frame::<2048>(b"o2", 2)).unwrap();
    deliver(&mut conn, 0, 0, b[half..].to_vec());
    conn.poll_rx(0);
    assert_eq!(conn.next_frame().unwrap().payload, &x[..]);
    peer.pump(&mut conn);
    assert_eq!(payloads(&mut peer), vec![b"o1".to_vec(), b"o2".to_vec()]);
}
