//! hugetlbfs 2 MiB pages: prepare, write, copy (JNL-043). Linux, CI prerequisite:
//! `sudo sysctl vm.nr_hugepages=16; sudo mount -t hugetlbfs -o pagesize=2M,uid=$(id -u) none /mnt/huge2m`,
//! mount path in `HFT_TEST_HUGE2M`.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(target_os = "linux")]

#[path = "../common/mod.rs"]
mod common;

use std::path::PathBuf;

use common::harness::*;
use common::proc::*;
use hft_journal::*;

const T: &str = "jnl_043_hugetlbfs_2m";

fn cfg(hot: &str, cold: &str) -> JournalConfig {
    let mut c = c0(&PathBuf::from(hot), &PathBuf::from(cold));
    c.page = PageSize::Huge2M;
    c.segment_bytes = 4 << 20;
    c
}

fn child() -> ! {
    let mut h = H::open0(cfg(&var("JNL_HOT"), &var("JNL_COLD")));
    h.events(1000);
    say("ready");
    assert_eq!(listen(), "go");
    let before = minflt();
    h.events(1000);
    let d = minflt() - before;
    say(&format!("minflt {d}"));
    std::mem::forget(h);
    exit(0)
}

#[test]
fn jnl_043_hugetlbfs_2m() {
    if role().is_some() {
        child();
    }
    let Some(mnt) = prereq("HFT_TEST_HUGE2M", "hugetlbfs pagesize=2M mount") else {
        return;
    };
    let hot = PathBuf::from(&mnt).join(format!("jnl043-{}", std::process::id()));
    std::fs::create_dir_all(&hot).unwrap();
    let cold = cold_dir();
    let c = cfg(hot.to_str().unwrap(), cold.to_str().unwrap());
    let mut kid = spawn(
        T,
        "writer",
        &[
            ("JNL_HOT", hot.to_string_lossy().into()),
            ("JNL_COLD", cold.to_string_lossy().into()),
        ],
    );
    assert_eq!(kid.hear(), "ready");
    // fstatfs of the hot directory is HUGETLBFS_MAGIC.
    let cs = std::ffi::CString::new(hot.to_str().unwrap()).unwrap();
    // SAFETY: zeroed statfs is a valid out-buffer; `cs` is NUL-terminated.
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: as above.
    assert_eq!(unsafe { libc::statfs(cs.as_ptr(), &mut st) }, 0);
    assert_eq!(st.f_type as i64, 0x9584_58f6);
    let mut p = Persister::new(PersistConfig::default());
    p.add(&c);
    let seg = hot.join("prod/seq/0000000001.jrn");
    let committed = |b: &[u8]| u64_at(b, 3072);
    for i in 0..1000u64 {
        p.poll(i * 25_000_000, UTC0);
        if p.lag().iter().all(|l| l.bytes == 0) && i > 2 {
            break;
        }
    }
    let hb = std::fs::read(&seg).unwrap();
    let n = committed(&hb) as usize;
    let cold_file = hft_journal::testing::cold_stem(&c.cold_journal_dir(), UTC0, 1, 1).with_extension("jrn");
    let cb = std::fs::read(cold_file).unwrap();
    let mask = |b: &[u8]| {
        let mut v = b[..n].to_vec();
        v[3072..3200].fill(0);
        v
    };
    assert_eq!(mask(&cb), mask(&hb));
    kid.tell("go");
    assert_eq!(kid.hear(), "minflt 0");
    let _ = kid.wait();
    let _ = std::fs::remove_dir_all(&hot);
}
