//! TK-071–TK-074: state-hash trails.

use std::cell::Cell;

use hft_event::{SectionHdr, SnapshotHdr};
use hft_testkit::dst::{ToyEngine, run_record};
use hft_testkit::hash::{
    Component, ComponentHashes, Divergence, HashTrail, bisect_divergence, component_hashes, diff_trails,
};
use xxhash_rust::xxh3::xxh3_64;
use zerocopy::{FromZeros, IntoBytes};

use crate::support::cfg;

fn h(n: u64) -> ComponentHashes {
    ComponentHashes {
        book: n,
        oms: n + 1,
        risk: n + 2,
        ledger: n + 3,
        exec: n + 4,
        strategies: n + 5,
        all: u128::from(n + 6),
    }
}

fn trail_a() -> HashTrail {
    HashTrail {
        every_n: 100,
        points: vec![(100, h(10)), (200, h(20)), (300, h(30))],
    }
}

#[test]
fn tk_071_diff_trails() {
    let a = trail_a();
    // TK-071.1
    assert_eq!(diff_trails(&a, &a.clone()), None, "TK-071.1");
    // TK-071.2
    let mut b = a.clone();
    for p in b.points.iter_mut().skip(1) {
        p.1.oms ^= 1;
        p.1.all ^= 1;
    }
    assert_eq!(
        diff_trails(&a, &b),
        Some(Divergence {
            seq: 200,
            component: Some(Component::Oms)
        }),
        "TK-071.2"
    );
    // TK-071.3: ledger and exec differ; ledger comes first in the section write order.
    let mut b = a.clone();
    b.points[1].1.ledger ^= 1;
    b.points[1].1.exec ^= 1;
    assert_eq!(
        diff_trails(&a, &b).map(|d| d.component),
        Some(Some(Component::Ledger)),
        "TK-071.3"
    );
    // TK-071.4
    let mut b = a.clone();
    b.points.truncate(2);
    assert_eq!(diff_trails(&a, &b).map(|d| d.seq), Some(300), "TK-071.4");
}

#[test]
fn tk_072_bisect() {
    let calls = Cell::new(0u32);
    let run = |seq: u64| {
        calls.set(calls.get() + 1);
        let a = h(seq);
        let mut b = a;
        if seq >= 1234 {
            b.oms ^= 0xFF;
        }
        (a, b)
    };
    assert_eq!(bisect_divergence(run, 1000, 2000), 1234);
    assert!(calls.get() <= 12, "{} calls", calls.get());
}

fn section(n: u16, body: &[u8]) -> Vec<u8> {
    let mut s = SectionHdr::new_zeroed();
    s.section = n;
    s.count = 1;
    s.len = body.len() as u32;
    let mut v = s.as_bytes().to_vec();
    v.extend_from_slice(body);
    v
}

#[test]
fn tk_073_component_hashes() {
    let secs: Vec<(u16, Vec<u8>)> = [1u16, 2, 3, 4, 5, 6, 7, 12, 13, 16, 21, 22]
        .iter()
        .map(|n| (*n, section(*n, &[*n as u8; 16])))
        .collect();
    let mut hdr = SnapshotHdr::new_zeroed();
    hdr.purpose = 1;
    hdr.n_sections = secs.len() as u32;
    hdr.state_hash = 0xABCD_u128.to_le_bytes();
    let mut body = hdr.as_bytes().to_vec();
    for (_, b) in &secs {
        body.extend_from_slice(b);
    }
    let c = component_hashes(&body).expect("parses");
    let cat = |ns: &[u16]| {
        let mut v = Vec::new();
        for (n, b) in &secs {
            if ns.contains(n) {
                v.extend_from_slice(b);
            }
        }
        xxh3_64(&v)
    };
    assert_eq!(c.ledger, cat(&[1, 13, 22]));
    assert_eq!(c.oms, cat(&[2, 3, 12]));
    assert_eq!(c.risk, cat(&[4]));
    assert_eq!(c.strategies, cat(&[5]));
    assert_eq!(c.book, cat(&[6, 7]));
    assert_eq!(c.exec, cat(&[21]));
    assert_eq!(c.all, 0xABCD, "all = the header's state_hash");
}

#[test]
fn tk_074_sampling() {
    let mut c = cfg(5, 230, &[], 0, &[]);
    c.hash_every = 50;
    let r = run_record::<ToyEngine>(&c);
    assert_eq!(r.steps_run, 230);
    let seqs: Vec<u64> = r.trail.points.iter().map(|p| p.0).collect();
    assert_eq!(seqs, vec![50, 100, 150, 200]);
}
