//! `HotRetirer::audit` counts hugetlbfs pages of unlinked but still mapped segments (JNL-054).
//! Linux, CI prerequisite: the 2 MiB hugetlbfs mount of JNL-043 (`HFT_TEST_HUGE2M`).
#![cfg(target_os = "linux")]

#[path = "../common/mod.rs"]
mod common;

use std::path::PathBuf;

use common::harness::*;
use common::proc::*;
use hft_journal::*;

const T: &str = "jnl_054_audit_unlinked_mapped";

fn cfg(hot: &str, cold: &str) -> JournalConfig {
    let mut c = c0(&PathBuf::from(hot), &PathBuf::from(cold));
    c.page = PageSize::Huge2M;
    c.segment_bytes = 4 << 20;
    c.hot_keep = 0;
    c
}

/// Maps segment 2 read-only and keeps it until told to exit.
fn child() -> ! {
    let mut rc = ReaderConfig::new(
        var("JNL_HOT").into(),
        var("JNL_COLD").into(),
        InstanceName::Prod,
        "seq".into(),
    );
    rc.start = StartAt::SegmentBoot(2);
    let mut r = JournalReader::open(&rc).unwrap();
    let _ = r.poll();
    assert_ne!(r.debug_map_addr(), 0);
    say("mapped");
    let _ = listen();
    exit(0)
}

#[test]
fn jnl_054_audit_unlinked_mapped() {
    if role().is_some() {
        child();
    }
    let Some(mnt) = prereq("HFT_TEST_HUGE2M", "hugetlbfs pagesize=2M mount") else {
        return;
    };
    let hot = PathBuf::from(&mnt).join(format!("jnl054-{}", std::process::id()));
    std::fs::create_dir_all(&hot).unwrap();
    let cold = cold_dir();
    let mut h = H::open0(cfg(hot.to_str().unwrap(), cold.to_str().unwrap()));
    h.events(10);
    h.roll(RollReason::Hourly);
    h.events(10);
    h.roll(RollReason::Hourly);
    h.persist_all();
    let base = h.ret.audit();
    let mut kid = spawn(
        T,
        "reader",
        &[
            ("JNL_HOT", hot.to_string_lossy().into()),
            ("JNL_COLD", cold.to_string_lossy().into()),
        ],
    );
    assert_eq!(kid.hear(), "mapped");
    let t = h.utc;
    h.ret.poll(t);
    assert!(h.ret.poll(t + 2_000_000_000) >= 1);
    assert!(!hft_journal::testing::exists(&h.seg_path(2)));
    let a = h.ret.audit();
    assert_eq!(a.pages_2m - base.pages_2m, 2);
    kid.tell("exit");
    let _ = kid.wait();
    let a = h.ret.audit();
    assert_eq!(a.pages_2m, base.pages_2m);
}
