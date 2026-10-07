//! NET-020–NET-044: WS deframing on the receive path (MemPeer + ScriptIo).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod support;

use hft_net::{ConnPhase, ErrBits, Opcode};
use support::mempeer::{self, MemPeer, cfg, deliver, pair, pair_cfg};
use support::ws_ref::{self, server_frame, text};

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 7 + 3) as u8).collect()
}

/// Delivers `frames` (one record each) as one read and takes every frame.
fn send_and_collect(
    conn: &mut hft_net::Conn,
    peer: &mut MemPeer,
    frames: &[Vec<u8>],
) -> Vec<(Opcode, Vec<u8>, bool)> {
    let b = peer.frames(frames);
    deliver(conn, 0, 0, b);
    conn.poll_rx(0);
    collect(conn)
}

fn collect(conn: &mut hft_net::Conn) -> Vec<(Opcode, Vec<u8>, bool)> {
    let mut out = Vec::new();
    while let Some(f) = conn.next_frame() {
        out.push((f.opcode, f.payload.to_vec(), f.reassembled));
    }
    out
}

#[test]
fn net_020_server_frame_lengths() {
    let rows = [
        ("NET-020.1", 0usize, vec![0x81, 0x00]),
        ("NET-020.2", 125, vec![0x81, 0x7D]),
        ("NET-020.3", 126, vec![0x81, 0x7E, 0x00, 0x7E]),
        ("NET-020.4", 65_535, vec![0x81, 0x7E, 0xFF, 0xFF]),
        ("NET-020.5", 65_536, vec![0x81, 0x7F, 0, 0, 0, 0, 0, 1, 0, 0]),
    ];
    for (row, n, head) in rows {
        let mut c = cfg();
        c.rx_buf = 262_144;
        c.max_msg = 131_072;
        let (mut conn, mut peer) = pair_cfg(c);
        let p = payload(n);
        let f = text(&p);
        assert_eq!(&f[..head.len()], &head[..], "{row} header");
        let b = peer.split(&f, &[16_384; 8]);
        deliver(&mut conn, 0, 0, b);
        conn.poll_rx(0);
        let got = collect(&mut conn);
        assert_eq!(got.len(), 1, "{row}");
        assert_eq!(got[0].0, Opcode::Text, "{row}");
        assert_eq!(got[0].1, p, "{row}");
        assert!(!got[0].2, "{row}");
        assert!(conn.err().is_empty(), "{row}: {:?}", conn.err());
    }
}

#[test]
fn net_021_masked_server_frame_rejected() {
    let (mut conn, mut peer) = pair();
    let mut f = vec![0x81, 0x85, 1, 2, 3, 4];
    f.extend(ws_ref::mask(b"hello", 0x0102_0304));
    let b = peer.records(&[&f]);
    deliver(&mut conn, 0, 0, b);
    conn.poll_rx(0);
    assert!(conn.next_frame().is_none());
    assert!(conn.err().contains(ErrBits::WS_PROTO));
    assert_eq!(conn.phase(), ConnPhase::Dead);
}

#[test]
fn net_022_rsv_bits_rejected() {
    for (row, b0) in [
        ("NET-022.1", 0xC1u8),
        ("NET-022.2", 0xA1),
        ("NET-022.3", 0x91),
        ("NET-022.4", 0x81),
    ] {
        let (mut conn, mut peer) = pair();
        let f = vec![b0, 0x01, b'x'];
        let got = send_and_collect(&mut conn, &mut peer, &[f]);
        if b0 == 0x81 {
            assert_eq!(got, vec![(Opcode::Text, b"x".to_vec(), false)], "{row}");
            assert!(conn.err().is_empty(), "{row}");
        } else {
            assert!(got.is_empty(), "{row}");
            assert!(conn.err().contains(ErrBits::WS_PROTO), "{row}");
        }
    }
}

#[test]
fn net_023_control_frame_len_limit() {
    for (row, n) in [("NET-023.1", 124usize), ("NET-023.2", 125), ("NET-023.3", 126)] {
        let (mut conn, mut peer) = pair();
        let p = payload(n);
        let f = server_frame(true, 0x9, &p);
        if n == 126 {
            assert_eq!(&f[..4], &[0x89, 0x7E, 0x00, 0x7E]);
        }
        let got = send_and_collect(&mut conn, &mut peer, &[f]);
        assert!(got.is_empty(), "{row}");
        conn.finish_poll(0);
        peer.pump(&mut conn);
        let pongs: Vec<_> = peer
            .client_frames()
            .into_iter()
            .filter(|f| f.opcode == 0xA)
            .collect();
        if n <= 125 {
            assert_eq!(pongs.len(), 1, "{row}");
            assert_eq!(pongs[0].payload, p, "{row}");
            assert!(conn.err().is_empty(), "{row}");
        } else {
            assert_eq!(pongs.len(), 0, "{row}");
            assert!(conn.err().contains(ErrBits::WS_PROTO), "{row}");
        }
    }
}

#[test]
fn net_024_fragmented_control_rejected() {
    let (mut conn, mut peer) = pair();
    let got = send_and_collect(&mut conn, &mut peer, &[vec![0x09, 0x02, b'a', b'b']]);
    assert!(got.is_empty());
    conn.finish_poll(0);
    peer.pump(&mut conn);
    assert!(conn.err().contains(ErrBits::WS_PROTO));
    assert_eq!(peer.client_frames().iter().filter(|f| f.opcode == 0xA).count(), 0);
}

#[test]
fn net_025_unknown_opcode_rejected() {
    for (row, op) in [
        ("NET-025.1", 0x3u8),
        ("NET-025.2", 0x7),
        ("NET-025.3", 0xB),
        ("NET-025.4", 0xF),
        ("NET-025.5", 0x2),
    ] {
        let (mut conn, mut peer) = pair();
        let got = send_and_collect(&mut conn, &mut peer, &[server_frame(true, op, b"x")]);
        if op == 0x2 {
            assert_eq!(got.len(), 1, "{row}");
            assert_eq!(got[0].0, Opcode::Binary, "{row}");
            assert!(conn.err().is_empty(), "{row}");
        } else {
            assert!(got.is_empty(), "{row}");
            assert!(conn.err().contains(ErrBits::WS_PROTO), "{row}");
        }
    }
}

#[test]
fn net_026_ping_queues_pong() {
    let (mut conn, mut peer) = pair();
    let b = peer.frames(&[server_frame(true, 0x9, b"p1"), text(b"d1")]);
    deliver(&mut conn, 0, 0, b);
    conn.poll_rx(0);
    let f = conn.next_frame().map(|f| (f.opcode, f.payload.to_vec()));
    assert_eq!(f, Some((Opcode::Text, b"d1".to_vec())));
    assert!(conn.next_frame().is_none());
    peer.pump(&mut conn);
    assert!(peer.client_frames().is_empty(), "no Pong before finish_poll");
    conn.finish_poll(0);
    peer.pump(&mut conn);
    let fr = peer.client_frames();
    assert_eq!(fr.len(), 1);
    assert_eq!(
        (fr[0].opcode, fr[0].payload.as_slice(), fr[0].masked),
        (0xA, &b"p1"[..], true)
    );
}

fn pings(n: &[&[u8]]) -> Vec<Vec<u8>> {
    n.iter().map(|p| server_frame(true, 0x9, p)).collect()
}

#[test]
fn net_027_four_pings_four_pongs() {
    let (mut conn, mut peer) = pair();
    let got = send_and_collect(&mut conn, &mut peer, &pings(&[b"a", b"b", b"c", b"d"]));
    assert!(got.is_empty());
    conn.finish_poll(0);
    peer.pump(&mut conn);
    let p: Vec<Vec<u8>> = peer.client_frames().into_iter().map(|f| f.payload).collect();
    assert_eq!(
        p,
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]
    );
}

#[test]
fn net_028_pong_queue_overflow_keeps_last() {
    let (mut conn, mut peer) = pair();
    send_and_collect(&mut conn, &mut peer, &pings(&[b"a", b"b", b"c", b"d", b"e"]));
    conn.finish_poll(0);
    peer.pump(&mut conn);
    let p: Vec<Vec<u8>> = peer.client_frames().into_iter().map(|f| f.payload).collect();
    assert_eq!(p, vec![b"e".to_vec()]);
    assert!(conn.err().is_empty());
}

#[test]
fn net_029_pong_records_rtt() {
    let (mut conn, mut peer) = pair();
    conn.send_ping(1_000_000).unwrap();
    peer.pump(&mut conn);
    let fr = peer.client_frames();
    assert_eq!(
        (fr[0].opcode, fr[0].payload.clone()),
        (0x9, 1_000_000u64.to_le_bytes().to_vec())
    );
    let b = peer.frames(&[server_frame(true, 0xA, &1_000_000u64.to_le_bytes())]);
    deliver(&mut conn, 1_200_000, 1_250_000, b);
    conn.poll_rx(0);
    assert!(conn.next_frame().is_none());
    assert_eq!(conn.stats().rtt.count, 1);
    assert_eq!(conn.stats().rtt.last_ns, 250_000);
    assert_eq!(conn.last_rx_tsc(), 1_250_000);
}

#[test]
fn net_030_foreign_pong_ignored() {
    let rows: [(&str, Vec<u8>); 3] = [
        ("NET-030.1", 999_999u64.to_le_bytes().to_vec()),
        ("NET-030.2", vec![]),
        ("NET-030.3", vec![1, 2, 3, 4, 5, 6, 7]),
    ];
    for (row, p) in rows {
        let (mut conn, mut peer) = pair();
        conn.send_ping(1_000_000).unwrap();
        peer.pump(&mut conn);
        let b = peer.frames(&[server_frame(true, 0xA, &p)]);
        deliver(&mut conn, 10, 20, b);
        conn.poll_rx(0);
        assert!(conn.next_frame().is_none(), "{row}");
        assert_eq!(conn.stats().rtt.count, 0, "{row}");
        assert!(conn.err().is_empty(), "{row}");
        assert_eq!(conn.last_rx_tsc(), 20, "{row}");
    }
}

#[test]
fn net_031_close_echo_and_phase() {
    let (mut conn, mut peer) = pair();
    let got = send_and_collect(&mut conn, &mut peer, &[server_frame(true, 0x8, &[0x03, 0xE8])]);
    assert!(got.is_empty());
    assert!(conn.err().contains(ErrBits::WS_CLOSE));
    assert_eq!(conn.phase(), ConnPhase::Closing);
    conn.finish_poll(0);
    peer.pump(&mut conn);
    let fr = peer.client_frames();
    assert_eq!(fr.len(), 1);
    assert_eq!(
        (fr[0].opcode, fr[0].masked, fr[0].payload.as_slice()),
        (0x8, true, &[0x03u8, 0xE8][..])
    );
    assert_eq!(conn.phase(), ConnPhase::Dead);
}

#[test]
fn net_032_empty_close_echo() {
    let (mut conn, mut peer) = pair();
    send_and_collect(&mut conn, &mut peer, &[server_frame(true, 0x8, &[])]);
    conn.finish_poll(0);
    peer.pump(&mut conn);
    let fr = peer.client_frames();
    assert_eq!(fr.len(), 1);
    assert_eq!((fr[0].opcode, fr[0].payload.len()), (0x8, 0));
    assert!(conn.err().contains(ErrBits::WS_CLOSE));
    assert_eq!(conn.phase(), ConnPhase::Dead);
}

#[test]
fn net_033_one_byte_close_rejected() {
    let (mut conn, mut peer) = pair();
    send_and_collect(&mut conn, &mut peer, &[server_frame(true, 0x8, &[0x03])]);
    conn.finish_poll(0);
    peer.pump(&mut conn);
    assert!(conn.err().contains(ErrBits::WS_PROTO));
    assert!(!conn.err().contains(ErrBits::WS_CLOSE));
    assert_eq!(peer.client_frames().iter().filter(|f| f.opcode == 0x8).count(), 0);
    assert_eq!(conn.phase(), ConnPhase::Dead);
}

#[test]
fn net_034_fragments_reassembled() {
    let (mut conn, mut peer) = pair();
    let frames = vec![
        server_frame(false, 0x1, b"abc"),
        server_frame(false, 0x0, b"def"),
        server_frame(true, 0x0, b"gh"),
    ];
    let got = send_and_collect(&mut conn, &mut peer, &frames);
    assert_eq!(got, vec![(Opcode::Text, b"abcdefgh".to_vec(), true)]);
    assert_eq!(conn.stats().reassembled, 1);
    assert!(conn.err().is_empty());
}

#[test]
fn net_035_orphan_continuation_rejected() {
    let (mut conn, mut peer) = pair();
    let got = send_and_collect(&mut conn, &mut peer, &[server_frame(true, 0x0, b"gh")]);
    assert!(got.is_empty());
    assert!(conn.err().contains(ErrBits::WS_PROTO));
}

#[test]
fn net_036_new_message_during_fragments_rejected() {
    let (mut conn, mut peer) = pair();
    let got = send_and_collect(
        &mut conn,
        &mut peer,
        &[server_frame(false, 0x1, b"abc"), text(b"x")],
    );
    assert!(got.is_empty());
    assert!(conn.err().contains(ErrBits::WS_PROTO));
}

#[test]
fn net_037_ping_between_fragments() {
    let (mut conn, mut peer) = pair();
    let frames = vec![
        server_frame(false, 0x1, b"abc"),
        server_frame(true, 0x9, b"p"),
        server_frame(true, 0x0, b"def"),
    ];
    let got = send_and_collect(&mut conn, &mut peer, &frames);
    assert_eq!(got, vec![(Opcode::Text, b"abcdef".to_vec(), true)]);
    conn.finish_poll(0);
    peer.pump(&mut conn);
    let fr = peer.client_frames();
    assert_eq!(fr.len(), 1);
    assert_eq!((fr[0].opcode, fr[0].payload.as_slice()), (0xA, &b"p"[..]));
    assert!(conn.err().is_empty());
}

#[test]
fn net_038_max_msg_boundary() {
    for (row, n) in [("NET-038.1", 999usize), ("NET-038.2", 1000), ("NET-038.3", 1001)] {
        let mut c = cfg();
        c.max_msg = 1000;
        c.rx_buf = 65_536;
        let (mut conn, mut peer) = pair_cfg(c);
        let f = text(&payload(n));
        if n == 1001 {
            // Only the header and 10 payload bytes arrive.
            let b = peer.records(&[&f[..4 + 10]]);
            deliver(&mut conn, 0, 0, b);
            conn.poll_rx(0);
            assert!(conn.next_frame().is_none(), "{row}");
            assert!(conn.err().contains(ErrBits::RX_FULL), "{row}");
        } else {
            let got = send_and_collect(&mut conn, &mut peer, &[f]);
            assert_eq!(got.len(), 1, "{row}");
            assert_eq!(got[0].1.len(), n, "{row}");
            assert!(conn.err().is_empty(), "{row}");
        }
    }
}

#[test]
fn net_039_fragments_exceed_max_msg() {
    let mut c = cfg();
    c.max_msg = 1000;
    let (mut conn, mut peer) = pair_cfg(c);
    let got = send_and_collect(
        &mut conn,
        &mut peer,
        &[
            server_frame(false, 0x1, &payload(600)),
            server_frame(true, 0x0, &payload(401)),
        ],
    );
    assert!(got.is_empty());
    assert!(conn.err().contains(ErrBits::RX_FULL));
    assert_eq!(conn.stats().reassembled, 0);
}

#[test]
fn net_040_payload_borrows_rx_buffer() {
    let (mut conn, mut peer) = pair();
    let b = peer.frames(&[text(&payload(300))]);
    deliver(&mut conn, 0, 0, b);
    conn.poll_rx(0);
    let (lo, hi) = conn.test_rx_range();
    let f = conn.next_frame().unwrap();
    let p = f.payload.as_ptr() as usize;
    assert!(p >= lo && p + f.payload.len() <= hi);
    assert!(!f.reassembled);
    assert_eq!(conn.stats().reassembled, 0);
}

#[test]
fn net_042_last_rx_tsc_advances() {
    let rows: [(&str, Vec<u8>, u64); 5] = [
        ("NET-042.1", text(b"d"), 5_000),
        ("NET-042.2", server_frame(true, 0x9, b"p"), 5_000),
        ("NET-042.3", server_frame(true, 0xA, b"q"), 5_000),
        ("NET-042.4", server_frame(true, 0x8, &[0x03, 0xE8]), 5_000),
        ("NET-042.5", text(&payload(100))[..2 + 40].to_vec(), 0),
    ];
    for (row, f, want) in rows {
        let (mut conn, mut peer) = pair();
        assert_eq!(conn.last_rx_tsc(), 0, "{row}");
        let b = peer.records(&[&f]);
        deliver(&mut conn, 4_000, 5_000, b);
        conn.poll_rx(0);
        let _ = conn.next_frame();
        assert_eq!(conn.last_rx_tsc(), want, "{row}");
    }
}

#[test]
fn net_043_binary_frame() {
    let (mut conn, mut peer) = pair();
    let got = send_and_collect(&mut conn, &mut peer, &[vec![0x82, 0x03, 1, 2, 3]]);
    assert_eq!(got, vec![(Opcode::Binary, vec![1, 2, 3], false)]);
}

#[test]
fn net_044_rx_buf_boundary() {
    for (row, n) in [("NET-044.1", 4069usize), ("NET-044.2", 4070), ("NET-044.3", 4071)] {
        let mut c = cfg();
        c.rx_buf = 4096;
        c.max_msg = 65_536;
        let (mut conn, mut peer) = pair_cfg(c);
        let f = text(&payload(n));
        assert_eq!(f.len(), n + 4);
        let b = peer.records(&[&f]);
        assert_eq!(b.len(), n + 4 + 22, "{row}: one record, 22 B overhead");
        deliver(&mut conn, 0, 0, b);
        conn.poll_rx(0);
        let got = collect(&mut conn);
        if n <= 4070 {
            assert_eq!(got.len(), 1, "{row}");
            assert_eq!(got[0].1.len(), n, "{row}");
            assert!(conn.err().is_empty(), "{row}");
        } else {
            assert!(got.is_empty(), "{row}");
            assert!(conn.err().contains(ErrBits::RX_FULL), "{row}");
        }
    }
}

#[allow(unused)]
fn _uses(_: &mempeer::MemPeer) {}
