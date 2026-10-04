//! HugeRegion, Arena, lock_all (RT-085 – RT-088). Linux.
#![cfg(all(target_os = "linux", not(loom)))]

#[allow(dead_code)]
mod common {
    pub mod proc;
}

use common::proc::*;
use hft_rt::{Arena, HugeRegion, OpenMode, PageSize, RtErr, lock_all};

#[test]
fn rt_085_anon_region() {
    let r = HugeRegion::anon(10_000, PageSize::Normal, None).unwrap();
    r.prefault();
    assert_eq!(r.len(), 12_288, "RT-085.1");
    // SAFETY: the region is a live private mapping of `len` bytes owned by this test.
    let bytes = unsafe { std::slice::from_raw_parts(r.as_ptr(), r.len()) };
    assert!(bytes.iter().all(|b| *b == 0), "RT-085.1");
    assert_eq!(
        HugeRegion::anon(1 << 30, PageSize::Huge1G, None).err(),
        Some(RtErr::NoMem),
        "RT-085.2"
    );
    assert_eq!(
        HugeRegion::anon(2 << 20, PageSize::Huge2M, None).err(),
        Some(RtErr::NoMem),
        "RT-085.3"
    );
}

#[test]
fn rt_086_file_region() {
    let dir = shm_dir();
    let p_new = dir.join("new");
    let r = HugeRegion::file(&p_new, 8192, PageSize::Normal, OpenMode::CreateNew);
    assert!(r.is_ok(), "RT-086.1");
    assert_eq!(std::fs::metadata(&p_new).unwrap().len(), 8192, "RT-086.1");
    let again = HugeRegion::file(&p_new, 8192, PageSize::Normal, OpenMode::CreateNew);
    assert_eq!(again.err(), Some(RtErr::Os(17)), "RT-086.2");
    let missing = HugeRegion::file(
        &dir.join("missing"),
        8192,
        PageSize::Normal,
        OpenMode::OpenExisting,
    );
    assert_eq!(missing.err(), Some(RtErr::Os(2)), "RT-086.3");
    let short = dir.join("short");
    std::fs::write(&short, vec![0u8; 4096]).unwrap();
    let r = HugeRegion::file(&short, 8192, PageSize::Normal, OpenMode::OpenExisting);
    assert_eq!(r.err(), Some(RtErr::NoMem), "RT-086.4");
    let full = dir.join("full");
    std::fs::write(&full, vec![0xABu8; 8192]).unwrap();
    let r = HugeRegion::file(&full, 8192, PageSize::Normal, OpenMode::OpenExisting).unwrap();
    r.prefault();
    // SAFETY: live shared mapping of 8192 bytes.
    let m = unsafe { std::slice::from_raw_parts(r.as_ptr(), 8192) };
    assert!(m.iter().all(|b| *b == 0xAB), "RT-086.5 mapping");
    drop(r);
    assert!(
        std::fs::read(&full).unwrap().iter().all(|b| *b == 0xAB),
        "RT-086.5 file"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rt_087_arena() {
    let mut a = Arena::new(HugeRegion::anon(4096, PageSize::Normal, None).unwrap());
    let s1 = a.alloc_slice::<u64>(10, 7).unwrap();
    assert_eq!(s1.len(), 10);
    assert!(s1.iter().all(|v| *v == 7));
    assert_eq!(s1.as_ptr() as usize % 8, 0);
    let s1_range = s1.as_ptr_range();
    let b = a.alloc_slice::<u8>(1, 1).unwrap();
    let b_addr = b.as_ptr() as usize;
    let s2 = a.alloc_slice::<u64>(1, 9).unwrap();
    assert_eq!(s2.as_ptr() as usize % 8, 0);
    assert_eq!(s2[0], 9);
    let s2_addr = s2.as_ptr() as usize;
    assert!(s2_addr >= s1_range.end as usize && s2_addr > b_addr);
    assert_eq!(a.alloc_slice::<u8>(0, 0).map(|s| s.len()), Ok(0));
    assert_eq!(a.alloc_slice::<u8>(4096, 0).err(), Some(RtErr::ArenaFull));
}

#[test]
fn rt_088_lock_all_limit() {
    if role().as_deref() == Some("limited") {
        let lim = libc::rlimit {
            rlim_cur: 64 * 1024,
            rlim_max: 64 * 1024,
        };
        // SAFETY: valid rlimit struct.
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &lim) }, 0);
        exit(if lock_all() == Err(RtErr::Os(12)) { 0 } else { 1 });
    }
    assert_eq!(spawn("rt_088_lock_all_limit", "limited", &[]).code(), 0);
}
