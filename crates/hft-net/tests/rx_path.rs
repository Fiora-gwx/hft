//! NET-080–NET-102: the receive path — one `recvmsg` per `poll_rx`, compaction, on-demand
//! decryption, TLS events, phases, QUICKACK, error bits and counters.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod support;

use hft_net::sock::{RecvKind, RecvStep, SendKind, SendStep, errno, opt};
use hft_net::{ConnPhase, ConnRole, ErrBits, MaskedFrame, Opcode, SendErr};
use support::mempeer::{Opts, cfg, deliver, pair, pair_cfg, pair_with};
use support::ws_ref::{server_frame, text};

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 13 + 1) as u8).collect()
}

/// A text frame whose single record is exactly `cipher` bytes (TLS 1.3 overhead 22 B).
fn frame_for_cipher(cipher: usize) -> Vec<u8> {
    let plain = cipher - 22;
    let p = if plain - 2 < 126 { plain - 2 } else { plain - 4 };
    let f = text(&payload(p));
    assert_eq!(f.len(), plain);
    f
}

#[test]
fn net_080_short_read_single_syscall() {
    let (mut conn, mut peer) = pair();
    let b = peer.records(&[&frame_for_cipher(300)]);
    assert_eq!(b.len(), 300);
    deliver(&mut conn, 1_000, 1_100, b);
    assert_eq!(conn.poll_rx(0), 300);
    assert_eq!(conn.sys().recvmsg, 1);
    assert!(
        conn.script().unwrap().recv.is_empty(),
        "no extra EAGAIN read consumed"
    );
    assert!(conn.next_frame().is_some());
    let b = peer.records(&[&frame_for_cipher(100)]);
    deliver(&mut conn, 2_000, 2_400, b);
    assert_eq!(conn.poll_rx(0), 100);
    let f = conn.next_frame().unwrap();
    assert_eq!(f.ts.rx_empty_gap_ns, 1_400);
}

#[test]
fn net_081_full_read_no_confirmation() {
    let mut c = cfg();
    c.rx_buf = 4096;
    let (mut conn, mut peer) = pair_cfg(c);
    conn.script().unwrap().push_recv(RecvStep::eagain(500));
    conn.poll_rx(0);
    // 4 096 B of records: two frames of 2 048 B ciphertext each.
    let b = peer.records(&[&frame_for_cipher(2048), &frame_for_cipher(2048)]);
    assert_eq!(b.len(), 4096);
    deliver(&mut conn, 1_000, 1_050, b);
    assert_eq!(conn.poll_rx(0), 4096);
    assert_eq!(conn.sys().recvmsg, 2);
    let f = conn.next_frame().unwrap();
    assert_eq!(f.ts.rx_empty_gap_ns, 550);
    assert!(conn.next_frame().is_some());
    assert!(conn.next_frame().is_none());
    let b = peer.records(&[&frame_for_cipher(32)]);
    let b10 = b[..10].to_vec();
    deliver(&mut conn, 3_000, 3_200, b10);
    conn.poll_rx(0);
    // The rest of the record arrives with the same return TSC, so the frame shows exactly the
    // gap of the 10-byte read (3 200 − 500: the full read confirmed nothing).
    deliver(&mut conn, 3_200, 3_200, b[10..].to_vec());
    conn.poll_rx(0);
    let f = conn.next_frame().unwrap();
    assert_eq!(f.ts.rx_empty_gap_ns, 2_700);
}

#[test]
fn net_082_zero_free_space_no_syscall() {
    let mut c = cfg();
    c.rx_buf = 4096;
    let (mut conn, mut peer) = pair_cfg(c);
    // One record larger than the buffer's first read: 4 096 B of an unfinished record.
    let big = peer.records(&[&text(&payload(8000))]);
    deliver(&mut conn, 0, 0, big[..4096].to_vec());
    assert_eq!(conn.poll_rx(0), 4096);
    let n = conn.sys().recvmsg;
    assert_eq!(conn.poll_rx(0), 0);
    assert_eq!(conn.sys().recvmsg, n);
    assert!(!conn.err().contains(ErrBits::PEER_EOF));
    assert_eq!(conn.phase(), ConnPhase::Open);
}

#[test]
fn net_083_eagain_confirms_empty() {
    let (mut conn, mut peer) = pair();
    conn.script().unwrap().push_recv(RecvStep::eagain(7_000));
    assert_eq!(conn.poll_rx(0), 0);
    assert!(conn.err().is_empty());
    assert_eq!(conn.sys().recvmsg, 1);
    let b = peer.records(&[&frame_for_cipher(50)]);
    deliver(&mut conn, 8_000, 8_500, b);
    conn.poll_rx(0);
    assert_eq!(conn.next_frame().unwrap().ts.rx_empty_gap_ns, 1_500);
}

#[test]
fn net_084_eof_sets_peer_eof() {
    let (mut conn, _peer) = pair();
    conn.script().unwrap().push_recv(RecvStep {
        pre_tsc: 1,
        post_tsc: 2,
        kind: RecvKind::Eof,
    });
    conn.poll_rx(0);
    assert_eq!(conn.poll_rx(0), 0);
    assert!(conn.err().contains(ErrBits::PEER_EOF));
    assert_eq!(conn.phase(), ConnPhase::Dead);
    assert_eq!(conn.sys().recvmsg, 1);
}

#[test]
fn net_085_recv_errno_mapping() {
    let rows = [
        ("NET-085.1", errno::ECONNRESET, ErrBits::RESET),
        ("NET-085.2", errno::ETIMEDOUT, ErrBits::RESET),
        ("NET-085.3", errno::ENOMEM, ErrBits::IO),
        ("NET-085.4", errno::EINTR, ErrBits::IO),
    ];
    for (row, e, bit) in rows {
        let (mut conn, _peer) = pair();
        conn.script().unwrap().push_recv(RecvStep {
            pre_tsc: 1,
            post_tsc: 2,
            kind: RecvKind::Errno(e),
        });
        conn.poll_rx(0);
        assert!(conn.err().contains(bit), "{row}");
        if bit == ErrBits::IO {
            assert!(!conn.err().contains(ErrBits::RESET), "{row}");
        }
        assert_eq!(conn.last_errno(), e, "{row}");
        assert_eq!(conn.phase(), ConnPhase::Dead, "{row}");
    }
}

#[test]
fn net_088_compaction_keeps_partial() {
    let mut c = cfg();
    c.rx_buf = 4096;
    let (mut conn, mut peer) = pair_cfg(c);
    let fa = frame_for_cipher(1000);
    let fb = frame_for_cipher(3200);
    let b = peer.records(&[&fa, &fb]);
    assert_eq!(b.len(), 4200);
    deliver(&mut conn, 0, 0, b[..3000].to_vec());
    assert_eq!(conn.poll_rx(0), 3000);
    let a = conn.next_frame().map(|f| f.payload.len());
    assert_eq!(a, Some(fa.len() - 4));
    assert!(conn.next_frame().is_none());
    deliver(&mut conn, 0, 0, b[3000..].to_vec());
    assert_eq!(conn.poll_rx(0), 1200);
    let f = conn.next_frame().unwrap();
    assert_eq!(f.payload, &fb[4..]);
}

#[test]
fn net_089_on_demand_decrypt() {
    let mut c = cfg();
    c.rx_buf = 262_144;
    // Row 1: the first frame owns one record.
    {
        let (mut conn, mut peer) = pair_cfg(c);
        let mut recs = vec![text(&payload(1000))];
        for _ in 0..60 {
            recs.push(text(&payload(1000)));
        }
        let b = peer.frames(&recs);
        assert!(b.len() > 60_000);
        deliver(&mut conn, 0, 0, b);
        conn.poll_rx(0);
        let base = conn.test_records_opened();
        assert!(conn.next_frame().is_some());
        assert_eq!(conn.test_records_opened() - base, 1, "NET-089.1");
    }
    // Row 2: the first frame spans two records.
    {
        let (mut conn, mut peer) = pair_cfg(c);
        let f1 = text(&payload(3000));
        let mut b = peer.split(&f1, &[1500, 1504]);
        for _ in 0..60 {
            b.extend(peer.frames(&[text(&payload(1000))]));
        }
        deliver(&mut conn, 0, 0, b);
        conn.poll_rx(0);
        let base = conn.test_records_opened();
        assert!(conn.next_frame().is_some());
        assert_eq!(conn.test_records_opened() - base, 2, "NET-089.2");
    }
    // Row 3: the first two frames share one record.
    {
        let (mut conn, mut peer) = pair_cfg(c);
        let mut two = text(&payload(500));
        two.extend(text(&payload(500)));
        let mut b = peer.records(&[&two]);
        for _ in 0..60 {
            b.extend(peer.frames(&[text(&payload(1000))]));
        }
        deliver(&mut conn, 0, 0, b);
        conn.poll_rx(0);
        let base = conn.test_records_opened();
        assert!(conn.next_frame().is_some());
        assert_eq!(conn.test_records_opened() - base, 1, "NET-089.3");
        assert!(conn.next_frame().is_some());
        assert_eq!(conn.test_records_opened() - base, 1, "NET-089.3 second frame");
    }
}

#[test]
fn net_090_send_between_frames() {
    let (mut conn, mut peer) = pair();
    let b = peer.frames(&[text(b"f1"), text(b"f2"), text(b"f3")]);
    deliver(&mut conn, 0, 0, b);
    conn.poll_rx(0);
    assert_eq!(conn.next_frame().unwrap().payload, b"f1");
    let mut o = MaskedFrame::<2048>::new();
    o.body_mut()[..2].copy_from_slice(b"o1");
    o.seal(Opcode::Text, 2, 77);
    conn.send_frame(&o).unwrap();
    assert_eq!(conn.next_frame().unwrap().payload, b"f2");
    assert_eq!(conn.next_frame().unwrap().payload, b"f3");
    peer.pump(&mut conn);
    let fr = peer.client_frames();
    assert_eq!(fr[0].payload, b"o1");
}

#[test]
fn net_091_frame_across_records() {
    let rows: [(&str, &[usize]); 3] = [
        ("NET-091.1", &[3004]),
        ("NET-091.2", &[1000, 2004]),
        ("NET-091.3", &[1, 1, 3002]),
    ];
    for (row, sizes) in rows {
        let (mut conn, mut peer) = pair();
        let p = payload(3000);
        let f = text(&p);
        assert_eq!(f.len(), 3004);
        let b = peer.split(&f, sizes);
        deliver(&mut conn, 0, 0, b);
        conn.poll_rx(0);
        let got = conn.next_frame().map(|f| (f.payload.to_vec(), f.reassembled));
        assert_eq!(got, Some((p.clone(), false)), "{row}");
    }
}

#[test]
fn net_093_key_update() {
    let (mut conn, mut peer) = pair();
    let mut b = peer.frames(&[text(b"a")]);
    b.extend(peer.key_update());
    b.extend(peer.frames(&[text(b"b")]));
    deliver(&mut conn, 0, 0, b);
    conn.poll_rx(0);
    let mut got = Vec::new();
    while let Some(f) = conn.next_frame() {
        got.push(f.payload.to_vec());
    }
    assert_eq!(got, vec![b"a".to_vec(), b"b".to_vec()]);
    assert!(conn.err().contains(ErrBits::KEY_UPDATE));
    assert!((conn.err() & ErrBits::FATAL).is_empty(), "{:?}", conn.err());
    conn.finish_poll(0);
    // The client's KeyUpdate reaches the server (one record without application data).
    let n = peer.pump(&mut conn);
    assert_eq!(n, 1);
    assert!(peer.recs.last().unwrap().plain.is_empty());
    conn.send_text(b"c", 0).unwrap();
    peer.pump(&mut conn);
    let fr = peer.client_frames();
    assert_eq!(fr.last().unwrap().payload, b"c");
}

#[test]
fn net_094_close_notify() {
    let (mut conn, mut peer) = pair();
    let b = peer.close_notify();
    deliver(&mut conn, 0, 0, b);
    conn.poll_rx(0);
    assert!(conn.next_frame().is_none());
    assert!(conn.err().contains(ErrBits::TLS_ALERT));
    assert_eq!(conn.phase(), ConnPhase::Dead);
}

#[test]
fn net_095_fatal_alert() {
    let (mut conn, mut peer) = pair();
    let b = peer.fatal_alert(80);
    deliver(&mut conn, 0, 0, b);
    conn.poll_rx(0);
    assert!(conn.next_frame().is_none());
    assert!(conn.err().contains(ErrBits::TLS_ALERT));
    assert_eq!(conn.phase(), ConnPhase::Dead);
}

#[test]
fn net_096_corrupt_record() {
    let (mut conn, mut peer) = pair();
    let mut b = peer.frames(&[text(b"x")]);
    let last = b.len() - 1;
    b[last] ^= 0x01;
    deliver(&mut conn, 0, 0, b);
    conn.poll_rx(0);
    assert!(conn.next_frame().is_none());
    assert!(conn.err().contains(ErrBits::TLS_PROTO));
    assert_eq!(conn.phase(), ConnPhase::Dead);
}

#[test]
fn net_097_blocked_handshake_on_hot() {
    let mut conn = hft_net::Conn::standalone(
        cfg(),
        ConnRole::Md,
        hft_net::ConnId(1),
        support::mempeer::clock(),
        hft_net::sock::ScriptIo::new(),
    );
    let ccfg = hft_net::client_tls_config(Some(support::mempeer::CA_PEM)).unwrap();
    assert!(conn.test_start_tls(&ccfg, "a.test"));
    conn.test_set_phase(ConnPhase::Open);
    // Server handshake bytes (a ServerHello-looking record) arrive on the hot thread.
    deliver(&mut conn, 0, 0, vec![22, 3, 3, 0, 4, 2, 0, 0, 0]);
    conn.poll_rx(0);
    assert!(conn.next_frame().is_none());
    assert!(conn.err().contains(ErrBits::TLS_PROTO));
}

#[test]
fn net_098_closing_dead_inert() {
    for (row, closing) in [("NET-098.1", true), ("NET-098.2", false)] {
        let (mut conn, mut peer) = pair();
        if closing {
            let b = peer.frames(&[server_frame(true, 0x8, &[0x03, 0xE8])]);
            deliver(&mut conn, 0, 0, b);
        } else {
            conn.script().unwrap().push_recv(RecvStep {
                pre_tsc: 0,
                post_tsc: 0,
                kind: RecvKind::Eof,
            });
        }
        conn.poll_rx(0);
        let _ = conn.next_frame();
        let phase = if closing {
            ConnPhase::Closing
        } else {
            ConnPhase::Dead
        };
        assert_eq!(conn.phase(), phase, "{row}");
        deliver(&mut conn, 0, 0, peer.frames(&[text(b"late")]));
        let n = conn.sys().recvmsg;
        assert_eq!(conn.poll_rx(0), 0, "{row}");
        assert_eq!(conn.sys().recvmsg, n, "{row}");
        assert!(conn.next_frame().is_none(), "{row}");
        let mut f = MaskedFrame::<2048>::new();
        f.seal(Opcode::Text, 1, 1);
        assert_eq!(conn.send_frame(&f), Err(SendErr::NotOpen(phase)), "{row}");
        assert_eq!(conn.send_text(b"t", 0), Err(SendErr::NotOpen(phase)), "{row}");
        assert_eq!(conn.send_ping(0), Err(SendErr::NotOpen(phase)), "{row}");
    }
}

#[test]
fn net_099_quickack() {
    let rows = [
        ("NET-099.1", false, false, true, 0),
        ("NET-099.2", true, false, true, 1),
        ("NET-099.3", true, false, false, 0),
        ("NET-099.4", false, true, true, 0),
    ];
    for (row, md, order, data, want) in rows {
        let mut c = cfg();
        c.quickack = hft_net::Quickack { md, order }.for_role(ConnRole::Md);
        let (mut conn, mut peer) = pair_cfg(c);
        if data {
            let b = peer.records(&[&frame_for_cipher(300)]);
            deliver(&mut conn, 0, 0, b);
        }
        conn.poll_rx(0);
        while conn.next_frame().is_some() {}
        conn.finish_poll(0);
        let vals: Vec<i32> = conn
            .sys()
            .setsockopt_values(opt::IPPROTO_TCP, opt::TCP_QUICKACK)
            .collect();
        assert_eq!(vals.len(), want, "{row}");
        if want == 1 {
            assert_eq!(vals[0], 1, "{row}");
        }
    }
}

#[test]
fn net_100_err_bits_accumulate() {
    let (mut conn, mut peer) = pair();
    let b = peer.key_update();
    deliver(&mut conn, 0, 0, b);
    conn.poll_rx(0);
    let _ = conn.next_frame();
    conn.mark(ErrBits::SERVER_HINT);
    assert!(conn.err().contains(ErrBits::KEY_UPDATE | ErrBits::SERVER_HINT));
    conn.clear_err();
    assert!(conn.err().is_empty());
}

#[test]
fn net_101_mark_allowed_bits() {
    let ok = [
        ("NET-101.1", ErrBits::PING_TIMEOUT, true),
        ("NET-101.2", ErrBits::IDLE_TIMEOUT, true),
        ("NET-101.3", ErrBits::SERVER_HINT, false),
    ];
    for (row, b, fatal) in ok {
        let (mut conn, _peer) = pair();
        conn.mark(b);
        assert!(conn.err().contains(b), "{row}");
        assert_eq!(!(conn.err() & ErrBits::FATAL).is_empty(), fatal, "{row}");
    }
    #[cfg(debug_assertions)]
    for (row, b) in [("NET-101.4", ErrBits::RESET), ("NET-101.5", ErrBits::ROTATE_DUE)] {
        let (mut conn, _peer) = pair();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| conn.mark(b)));
        assert!(r.is_err(), "{row}");
    }
}

#[test]
fn net_102_stats_frames_bytes() {
    let (mut conn, mut peer) = pair();
    let b = peer.frames(&[
        text(&payload(10)),
        server_frame(true, 0x2, &payload(20)),
        server_frame(true, 0x9, &payload(5)),
    ]);
    deliver(&mut conn, 0, 0, b);
    conn.poll_rx(0);
    while conn.next_frame().is_some() {}
    assert_eq!(conn.stats().frames, 2);
    assert_eq!(conn.stats().bytes, 30);
}

#[allow(dead_code)]
fn _unused(_: SendStep, _: SendKind, _: Opts) {
    let _ = pair_with;
}
