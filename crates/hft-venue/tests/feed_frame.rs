//! VEN-060–VEN-064, VEN-066: one frame (README §2 step 5).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

mod support;

use hft_event::{ConnStateKind, EventKind, FeedMsg};
use hft_net::{ConnId, ErrBits, TsFlags};
use hft_types::InstId;
use hft_venue::{Effect, RawMode, codes, raw_what, streams};
use hft_venue_proto::StreamKind;
use support::feedh::*;
use support::net::Ev;
use support::rd::*;
use support::stub;

const C0: ConnId = ConnId(0);
const C1: ConnId = ConnId(1);

/// Connections 0 and 1 up, S1's subscription frame sent on both but not answered.
fn sent_not_acked(raw: RawMode) -> (Fh, u64) {
    let mut cfg = f0();
    cfg.routes = hft_venue::FeedRoutes::Bbo;
    cfg.raw_bbo = raw;
    let mut h = feed(cfg);
    h.sub(S1, streams::BBO);
    h.poll(100);
    h.complete_builds();
    h.poll(200);
    let sent = h.net.sent_text(&mut h.rt, C0);
    assert_eq!(sent, vec!["SUB|701b"]);
    h.net.sent_text(&mut h.rt, C1);
    h.clear();
    (h, 300)
}

#[test]
fn ven_060_control_frames() {
    let (mut h, t) = sent_not_acked(RawMode::Off);
    let before = h.raw.records().len();
    h.net.push(C0, b"ACK|1|1|0");
    h.poll(t);
    let v = h.drain_bbo();
    assert_eq!(v.len(), 1);
    assert!(has_state(&v, 0, ConnStateKind::Up, S1, 0));
    assert!(bbos(&v).is_empty());
    let recs = h.raw.records();
    let new: Vec<_> = recs[before..].to_vec();
    assert_eq!(new.len(), 1, "{new:?}");
    assert_eq!((new[0].hdr.what, new[0].hdr.conn), (raw_what::SUB_ACK, C0));
}

#[test]
fn ven_061_sub_rejected() {
    let (mut h, t) = sent_not_acked(RawMode::On);
    h.net.push(C0, b"ACK|1|0|1121");
    h.poll(t);
    let v = h.drain_bbo();
    let s = states(&v);
    assert_eq!(s.len(), 1);
    assert_eq!(
        (s[0].state, s[0].role, s[0].conn, s[0].inst, s[0].reason),
        (ConnStateKind::Degraded, hft_event::ConnRole::Md, 0, S1, 4)
    );
}

#[test]
fn ven_062_unsubscribed_symbol() {
    let (mut h, t) = bbo_up(|_| {}, &[S1]);
    let before = h.raw.records().len();
    h.net.push(C0, b"B|ZZZ|5|1.00|1.000|1.01|1.000");
    h.poll(t);
    assert!(bbos(&h.drain_bbo()).is_empty());
    assert_eq!(h.rt.counters().unsubscribed, 1);
    assert!(stub::calls("decode_md").is_empty());
    let recs = h.raw.records();
    assert_eq!(recs.len() - before, 1);
    assert_eq!(recs.last().unwrap().hdr.what, raw_what::WS_TEXT);
}

#[test]
fn ven_063_md_decode_errors() {
    let rows: [(&str, &[u8]); 3] = [
        ("VEN-063.1", b"B|S1|"),
        ("VEN-063.2", b"B|S1|12|1x.5|1.000|100.01|2.000"),
        ("VEN-063.3", b"B|S1|12|99999999999999999999|1.000|100.01|2.000"),
    ];
    for (row, bad) in rows {
        let (mut h, t) = bbo_up(|_| {}, &[S1]);
        h.rt.set_last_fwd(S1, StreamKind::Bbo, 10);
        let before = h.raw.records().len();
        h.net.push(C0, bad);
        h.net.push(C0, &b("S1", 11));
        h.poll(t);
        let v = h.drain_bbo();
        assert_eq!(h.rt.counters().decode_err[0], 1, "{row}");
        assert!(states(&v).is_empty(), "{row}: no ConnState");
        assert_eq!(
            bbos(&v).iter().map(|x| x.update_id).collect::<Vec<_>>(),
            vec![11],
            "{row}"
        );
        assert_eq!(h.raw.records().len() - before, 2, "{row}: raw for both");
    }
    // .4: a 1 MiB frame (over max_msg) → RX_FULL, handled as fatal.
    let (mut h, t) = bbo_up(|_| {}, &[S1]);
    let mut f = vec![0x81u8, 127];
    f.extend_from_slice(&(1u64 << 20).to_be_bytes());
    f.extend(std::iter::repeat_n(b'x', 1 << 20));
    h.net.at(C0, 0, Ev::Frame(f));
    h.poll(t);
    let v = h.drain_bbo();
    assert!(
        has_state(&v, 0, ConnStateKind::Down, InstId(0), ErrBits::RX_FULL.bits()),
        "VEN-063.4 {v:?}"
    );
}

#[test]
fn ven_064_rx_meta_passthrough() {
    let (mut h, _) = bbo_up(|_| {}, &[S1]);
    // The queue of connection 1 was confirmed empty at 4300 (an EAGAIN read then).
    h.net.idle(&mut h.rt, 4300);
    h.rt.poll_once(4300);
    h.net
        .deliver_stamped(&mut h.rt, C1, &[&b("S1", 20)], 4900, 5000, 4100, 4500);
    h.rt.poll_once(5000);
    let e = stub::calls("decode_md");
    let rx = e.last().unwrap().rx.unwrap();
    assert_eq!(
        (
            rx.rx_tsc,
            rx.hw_rx_ns,
            rx.sw_rx_ns,
            rx.prev_override,
            rx.conn,
            rx.ts_flags,
            rx.rx_empty_gap_ns
        ),
        (5000, 4100, 4500, 0, C1, TsFlags::HW | TsFlags::SW, 700)
    );
    let bb = bbos(&h.drain_bbo());
    let hd = bb[0].h;
    assert_eq!(
        (hd.rx_tsc, hd.hw_rx_ns, hd.sw_rx_ns, hd.conn, hd.rx_empty_gap_ns),
        (5000, 4100, 4500, C1, 700)
    );
}

#[test]
fn ven_066_ring_then_raw() {
    let (mut h, _) = bbo_up(|_| {}, &[p(1), p(2), S1]);
    let frame = b"B|S1|30|10000.00|1000.000|10000.01|200.000";
    assert_eq!(frame.len(), 42);
    let before = h.raw.records().len();
    h.rt.trace(true);
    h.net
        .deliver_stamped(&mut h.rt, C0, &[frame], 8900, 9000, 8800, 0);
    h.rt.poll_once(9000);
    let tr = h.rt.take_trace();
    let ring = tr.iter().position(|e| *e == Effect::Bbo).unwrap();
    let raw = tr
        .iter()
        .position(|e| *e == Effect::RawRx(raw_what::WS_TEXT))
        .unwrap();
    assert!(ring < raw, "{tr:?}");
    let recs = h.raw.records();
    assert_eq!(recs.len() - before, 1);
    let r = recs.last().unwrap();
    assert_eq!(r.kind, EventKind::RAW_RX);
    assert_eq!(
        (
            r.hdr.venue,
            r.hdr.conn,
            r.hdr.what,
            r.hdr.stream,
            r.hdr.hw_rx_ns,
            r.hdr.payload_len
        ),
        (V7, C0, raw_what::WS_TEXT, 3, 8800, 42)
    );
    assert_eq!(r.tsc, 9000);
    assert_eq!(&r.payload[..], &frame[..]);
    assert!(matches!(h.drain_bbo()[..], [FeedMsg::Bbo(_)]));
    let _ = codes::DECODE;
}
