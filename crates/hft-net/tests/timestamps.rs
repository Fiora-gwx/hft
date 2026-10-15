//! NET-121–NET-134: receive stamps, coverage counters, `MULTI_SKB` and `rx_empty_gap_ns`.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::support;

use hft_net::sock::RecvStep;
use hft_net::{ErrBits, TsFlags};
use support::mempeer::{cfg, deliver, deliver_stamped, pair, pair_cfg};
use support::ws_ref::text;

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 + 7) as u8).collect()
}

/// A text frame whose single record is exactly `cipher` bytes.
fn frame_for_cipher(cipher: usize) -> Vec<u8> {
    let plain = cipher - 22;
    let p = if plain - 2 < 126 { plain - 2 } else { plain - 4 };
    text(&payload(p))
}

#[test]
fn net_121_ts_missing() {
    let rows = [
        ("NET-121.1", true, Some((0i64, 50i64)), true),
        ("NET-121.2", true, Some((40, 50)), false),
        ("NET-121.3", true, None, false),
        ("NET-121.4", false, Some((0, 50)), false),
    ];
    for (row, expect_hw, read, missing) in rows {
        let mut c = cfg();
        c.ts_expect_hw = expect_hw;
        let (mut conn, mut peer) = pair_cfg(c);
        match read {
            Some((hw, sw)) => {
                let b = peer.records(&[&frame_for_cipher(100)]);
                deliver_stamped(&mut conn, 0, 0, b, hw, sw);
            }
            None => conn.script().unwrap().push_recv(RecvStep::eagain(0)),
        }
        conn.poll_rx(0);
        assert_eq!(conn.err().contains(ErrBits::TS_MISSING), missing, "{row}");
    }
}

#[test]
fn net_122_ts_coverage_counters() {
    let (mut conn, mut peer) = pair();
    for step in 0..4 {
        match step {
            0 | 3 => {
                let b = peer.records(&[&frame_for_cipher(100)]);
                deliver_stamped(&mut conn, 0, 0, b, 77, 78);
            }
            1 => {
                let b = peer.records(&[&frame_for_cipher(100)]);
                deliver_stamped(&mut conn, 0, 0, b, 0, 78);
            }
            _ => conn.script().unwrap().push_recv(RecvStep::eagain(0)),
        }
        conn.poll_rx(0);
        while conn.next_frame().is_some() {}
    }
    assert_eq!(conn.stats().ts_reads, 3);
    assert_eq!(conn.stats().hw_ts_reads, 2);
}

#[test]
fn net_123_single_read_frame_stamps() {
    let (mut conn, mut peer) = pair();
    conn.script().unwrap().push_recv(RecvStep::eagain(1_000));
    conn.poll_rx(0);
    let b = peer.records(&[&frame_for_cipher(300)]);
    deliver_stamped(&mut conn, 2_000, 2_100, b, 900_000, 950_000);
    conn.poll_rx(0);
    let f = conn.next_frame().unwrap();
    assert_eq!(
        f.ts,
        hft_net::Timestamps {
            hw_rx_ns: 900_000,
            sw_rx_ns: 950_000,
            rx_tsc: 2_100,
            flags: TsFlags::HW | TsFlags::SW,
            rx_empty_gap_ns: 1_100,
        }
    );
}

#[test]
fn net_124_frame_across_reads() {
    let (mut conn, mut peer) = pair();
    conn.script().unwrap().push_recv(RecvStep::eagain(1_000));
    conn.poll_rx(0);
    let b = peer.records(&[&frame_for_cipher(1500)]);
    deliver_stamped(&mut conn, 5_000, 5_100, b[..1000].to_vec(), 10, 11);
    conn.poll_rx(0);
    assert!(conn.next_frame().is_none());
    deliver_stamped(&mut conn, 9_000, 9_050, b[1000..].to_vec(), 20, 21);
    conn.poll_rx(0);
    let f = conn.next_frame().unwrap();
    assert_eq!(
        (f.ts.hw_rx_ns, f.ts.sw_rx_ns, f.ts.rx_tsc, f.ts.rx_empty_gap_ns),
        (10, 11, 9_050, 8_050)
    );
}

#[test]
fn net_125_frames_share_read_stamps() {
    use hft_rt::latency_trace::{self as lt, Stage};
    let (writer, mut reader) = lt::channel(256).unwrap();
    lt::install(writer).unwrap();
    let (mut conn, mut peer) = pair();
    let b = peer.frames(&[text(b"1"), text(b"2"), text(b"3")]);
    deliver_stamped(&mut conn, 3_000, 4_000, b, 30, 31);
    conn.poll_rx(0);
    let mut ts = Vec::new();
    while let Some(f) = conn.next_frame() {
        ts.push(f.ts);
    }
    assert_eq!(ts.len(), 3);
    let mut frames = Vec::new();
    while let Some(record) = reader.rx.try_pop() {
        if record.stage == Stage::Frame as u64 {
            frames.push(record);
        }
    }
    assert_eq!(frames.len(), 3);
    assert!(
        frames
            .windows(2)
            .all(|w| w[0].context.frame != w[1].context.frame)
    );
    assert!(
        frames
            .iter()
            .all(|r| r.values[0] == 4000 && r.values[1] == 30 && r.values[2] == 31 && r.values[3] > 0)
    );
    assert_eq!(reader.dropped(), 0);
    for t in &ts {
        assert_eq!(
            (t.rx_tsc, t.hw_rx_ns, t.rx_empty_gap_ns),
            (4_000, 30, ts[0].rx_empty_gap_ns)
        );
    }
}

#[test]
fn net_126_multi_skb_mss_boundary() {
    for (row, n, multi) in [
        ("NET-126.1", 1447usize, false),
        ("NET-126.2", 1448, false),
        ("NET-126.3", 1449, true),
    ] {
        let (mut conn, mut peer) = pair();
        let b = peer.records(&[&frame_for_cipher(n)]);
        assert_eq!(b.len(), n);
        deliver_stamped(&mut conn, 0, 0, b, 1, 1);
        conn.poll_rx(0);
        let f = conn.next_frame().unwrap();
        assert_eq!(f.ts.flags & TsFlags::MULTI_SKB != 0, multi, "{row}");
        if multi {
            assert_eq!(conn.stats().multi_skb, 1, "{row}");
        }
    }
}

#[test]
fn net_127_stamp_table_overflow() {
    let (mut conn, mut peer) = pair();
    let f = text(&payload(1650));
    let b = peer.records(&[&f]);
    let chunks: Vec<&[u8]> = b.chunks(100).collect();
    assert_eq!(chunks.len(), 17);
    for (k, c) in chunks.iter().enumerate() {
        let k = k as u64 + 1;
        deliver_stamped(
            &mut conn,
            1_000 * k - 50,
            1_000 * k,
            c.to_vec(),
            k as i64,
            k as i64,
        );
        conn.poll_rx(0);
        if k < 17 {
            assert!(conn.next_frame().is_none());
        }
    }
    let f = conn.next_frame().unwrap();
    assert_eq!(f.payload.len(), 1650);
    assert_eq!(f.ts.hw_rx_ns, 1);
    assert_ne!(f.ts.flags & TsFlags::MULTI_SKB, 0);
    assert_eq!(f.ts.rx_tsc, 17_000);
}

#[test]
fn net_128_gap_from_eagain() {
    let (mut conn, mut peer) = pair();
    conn.script().unwrap().push_recv(RecvStep::eagain(1_000));
    let b = peer.frames(&[text(b"x")]);
    deliver(&mut conn, 2_900, 3_000, b);
    conn.poll_rx(0);
    conn.poll_rx(0);
    assert_eq!(conn.next_frame().unwrap().ts.rx_empty_gap_ns, 2_000);
}

#[test]
fn net_129_gap_from_short_read() {
    let (mut conn, mut peer) = pair();
    let a = peer.frames(&[text(b"a")]);
    deliver(&mut conn, 1_000, 1_100, a);
    conn.poll_rx(0);
    assert_eq!(conn.next_frame().unwrap().ts.rx_empty_gap_ns, 0);
    let b = peer.frames(&[text(b"b")]);
    deliver(&mut conn, 4_900, 5_000, b);
    conn.poll_rx(0);
    assert_eq!(conn.next_frame().unwrap().ts.rx_empty_gap_ns, 4_000);
}

#[test]
fn net_130_gap_from_note_empty() {
    let (mut conn, mut peer) = pair();
    conn.note_empty(2_500);
    let b = peer.frames(&[text(b"x")]);
    deliver(&mut conn, 2_950, 3_000, b);
    conn.poll_rx(0);
    assert_eq!(conn.next_frame().unwrap().ts.rx_empty_gap_ns, 500);
}

#[test]
fn net_131_no_confirmation_zero() {
    let mut c = cfg();
    c.rx_buf = 4096;
    let (mut conn, mut peer) = pair_cfg(c);
    // `pair` performed the install step; the first read fills all 4 096 B.
    let b = peer.records(&[&frame_for_cipher(2048), &frame_for_cipher(2048)]);
    deliver(&mut conn, 8_000, 9_000, b);
    assert_eq!(conn.poll_rx(0), 4096);
    assert_eq!(conn.next_frame().unwrap().ts.rx_empty_gap_ns, 0);
}

#[test]
fn net_132_install_resets_confirmation() {
    let mut c = cfg();
    c.rx_buf = 4096;
    let (mut conn, mut peer) = pair_cfg(c);
    // Warm thread: an EAGAIN before the hand-off.
    conn.script().unwrap().push_recv(RecvStep::eagain(100));
    conn.poll_rx(0);
    conn.test_on_install();
    let b = peer.records(&[&frame_for_cipher(2048), &frame_for_cipher(2048)]);
    deliver(&mut conn, 5_000, 5_100, b);
    assert_eq!(conn.poll_rx(0), 4096);
    assert_eq!(conn.next_frame().unwrap().ts.rx_empty_gap_ns, 0);
}

#[test]
fn net_133_confirmation_monotonic() {
    let (mut conn, mut peer) = pair();
    conn.script().unwrap().push_recv(RecvStep::eagain(2_000));
    conn.poll_rx(0);
    conn.note_empty(1_500);
    let b = peer.frames(&[text(b"x")]);
    deliver(&mut conn, 2_900, 3_000, b);
    conn.poll_rx(0);
    assert_eq!(conn.next_frame().unwrap().ts.rx_empty_gap_ns, 1_000);
}

#[test]
fn net_134_gap_saturation() {
    let rows = [
        ("NET-134.1", 4_294_968_294u64, 4_294_967_294u32),
        ("NET-134.2", 4_294_968_295, 4_294_967_295),
        ("NET-134.3", 4_294_968_296, 4_294_967_295),
    ];
    for (row, post, want) in rows {
        let (mut conn, mut peer) = pair();
        conn.note_empty(1_000);
        let b = peer.frames(&[text(b"x")]);
        deliver(&mut conn, post - 1, post, b);
        conn.poll_rx(0);
        assert_eq!(conn.next_frame().unwrap().ts.rx_empty_gap_ns, want, "{row}");
    }
}
