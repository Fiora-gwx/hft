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
