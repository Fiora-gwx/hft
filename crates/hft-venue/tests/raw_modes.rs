//! VEN-068: race duplicates in raw-feed.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use hft_net::ConnId;
use hft_venue::{RawMode, raw_what};
use support::feedh::*;
use support::rd::*;

fn data_frames(mode: RawMode, dups: bool) -> usize {
    let (mut h, now) = bbo_up(
        |c| {
            c.raw_bbo = mode;
            c.raw_dups = dups;
        },
        &[S1],
    );
    let before = h
        .raw
        .records()
        .iter()
        .filter(|r| r.hdr.what == raw_what::WS_TEXT)
        .count();
    for u in 1..=10 {
        h.net.push(ConnId(0), &b("S1", u));
        h.net.push(ConnId(1), &b("S1", u));
        h.poll(now + u);
    }
    h.raw
        .records()
        .iter()
        .filter(|r| r.hdr.what == raw_what::WS_TEXT)
        .count()
        - before
}

#[test]
fn ven_068_raw_dups() {
    assert_eq!(data_frames(RawMode::On, false), 10, "VEN-068.1");
    assert_eq!(data_frames(RawMode::On, true), 20, "VEN-068.2");
    assert_eq!(
        data_frames(RawMode::Sampled, true),
        data_frames(RawMode::Sampled, false),
        "VEN-068.3"
    );
}

#[test]
fn ven_067_raw_modes_and_exact_sample_boundary() {
    use hft_venue::{FeedCfg, streams};
    for (bbo, md, nb, nm) in [
        (RawMode::On, RawMode::On, 25, 25),
        (RawMode::Sampled, RawMode::On, 3, 25),
        (RawMode::Off, RawMode::Sampled, 0, 3),
    ] {
        let mut h = feed(FeedCfg {
            raw_bbo: bbo,
            raw_md: md,
            ..f0()
        });
        let mut now = 100;
        h.up(&[S1], streams::BBO | streams::TRADE, &mut now);
        let md_conn = h.all_builds.iter().find(|r| r.conn.0 >= 8).unwrap().conn;
        for n in 0..25 {
            h.net.push(ConnId(0), &b("S1", n + 1));
            h.net
                .push(md_conn, format!("T|S1|{}|100.00|1.000|B", n + 1).as_bytes());
            h.poll(now + n * 100_000_000);
            h.clear();
        }
        let raw = h.raw.records();
        for (prefix, expected, mode) in [(b"B|".as_slice(), nb, bbo), (b"T|".as_slice(), nm, md)] {
            let frames: Vec<_> = raw.iter().filter(|r| r.payload.starts_with(prefix)).collect();
            assert_eq!(frames.len(), expected);
            assert!(
                frames
                    .iter()
                    .all(|r| (r.hdr.rflags & hft_event::RawHdr::SAMPLED != 0) == (mode == RawMode::Sampled))
            );
        }
        assert!(raw.iter().any(|r| r.hdr.what == raw_what::CONNECT));
        assert!(raw.iter().any(|r| r.hdr.what == raw_what::SUB_ACK));
        h.net.at(ConnId(0), 0, support::net::Ev::Rst);
        h.poll(now + 2_500_000_000);
        assert!(h.raw.records().iter().any(|r| r.hdr.what == raw_what::DISCONNECT));
    }
    let (mut h, _) = bbo_up(|c| c.raw_bbo = RawMode::Sampled, &[S1]);
    for (i, t) in [0, 999_000_000, 1_000_000_000, 1_001_000_000]
        .into_iter()
        .enumerate()
    {
        h.net.push(ConnId(0), &b("S1", i as u64 + 1));
        h.poll(t);
    }
    let raw = h.raw.records();
    let t: Vec<_> = raw
        .iter()
        .filter(|r| r.payload.starts_with(b"B|"))
        .map(|r| r.tsc)
        .collect();
    assert_eq!(t, [0, 1_000_000_000]);
}
