//! Segment file naming and directories (JNL-010). Linux (tmpfs hot); heap backend elsewhere.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::harness::*;
use hft_journal::testing;
use hft_journal::*;

const DAY: i64 = 86_400_000_000_000;

#[test]
fn jnl_010_paths() {
    let hot = linux_hot();
    let cold = cold_dir();
    let mut h = H::open0(c0(&hot, &cold));
    assert!(testing::exists(&hot.join("prod/seq/0000000001.jrn")), "JNL-010.1");
    h.events(3);
    h.persist_all();
    let day1 = cold.join("prod/seq/20261001");
    assert!(
        day1.join("0000000001-00000000000000000001.jrn").exists(),
        "JNL-010.2"
    );
    h.utc = UTC0 + DAY - 1;
    h.roll(RollReason::Hourly);
    h.persist_all();
    assert!(
        day1.join("0000000001-00000000000000000001.seal").exists(),
        "JNL-010.3"
    );
    let first2 = h.hdr(2).first_seq;
    assert!(
        day1.join(format!("0000000002-{first2:020}.jrn")).exists(),
        "JNL-010.4"
    );
    h.utc = UTC0 + DAY;
    h.roll(RollReason::Hourly);
    h.events(1);
    h.persist_all();
    let first3 = h.hdr(3).first_seq;
    assert!(
        cold.join(format!("prod/seq/20261002/0000000003-{first3:020}.jrn"))
            .exists(),
        "JNL-010.5"
    );

    let mut cs = c0(&hot, &cold);
    cs.instance = InstanceName::Sim;
    let mut s = H::open0(cs);
    s.events(1);
    s.persist_all();
    assert!(testing::exists(&hot.join("sim/seq/0000000001.jrn")), "JNL-010.6");
    assert!(
        cold.join("sim/seq/20261001/0000000001-00000000000000000001.jrn")
            .exists(),
        "JNL-010.6"
    );
    let prod_files = testing::hot_files(&hot.join("prod/seq"));
    assert!(
        prod_files.iter().all(|p| !p.to_string_lossy().contains("sim")),
        "JNL-010.6"
    );

    let r = H::open0(r0(&hot, &cold));
    assert!(
        testing::exists(&hot.join("prod/raw-feed-binance-usdm/0000000001.jrn")),
        "JNL-010.7"
    );
    let rh = r.hdr(1);
    assert_eq!((rh.journal_kind, rh.venue.0), (1, 1), "JNL-010.7");

    let f = h.hdr(1);
    let mut name = [0u8; 32];
    name[..3].copy_from_slice(b"seq");
    let mut inst = [0u8; 16];
    inst[..4].copy_from_slice(b"prod");
    assert_eq!((f.journal_name, f.instance), (name, inst), "JNL-010.8");
}
