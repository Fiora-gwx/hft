//! TK-174: test-owned temporary and evidence directories.

use hft_testkit::tmp::{TmpDir, per_process_file};

#[test]
fn tk_174_test_directories_removed_unless_kept() {
    // TK-174.1: a plain directory (with content) disappears when its guard drops.
    let d = TmpDir::in_temp("tk-174").unwrap();
    let p = d.path().to_path_buf();
    std::fs::write(p.join("a.txt"), b"x").unwrap();
    assert!(p.is_dir());
    drop(d);
    assert!(!p.exists(), "TK-174.1");
    // TK-174.2: two guards of one process get distinct directories.
    let (a, b) = (
        TmpDir::in_temp("tk-174").unwrap(),
        TmpDir::in_temp("tk-174").unwrap(),
    );
    assert_ne!(a.path(), b.path(), "TK-174.2");
    drop((a, b));
    // TK-174.3: an evidence directory dropped normally is removed.
    let e = TmpDir::evidence(std::env::temp_dir().join(format!("tk-174-e-{}", std::process::id()))).unwrap();
    let ep = e.path().to_path_buf();
    drop(e);
    assert!(!ep.exists(), "TK-174.3");
    // TK-174.4: an evidence directory its owner drops while panicking stays.
    let kept = std::thread::spawn(|| {
        let e =
            TmpDir::evidence(std::env::temp_dir().join(format!("tk-174-p-{}", std::process::id()))).unwrap();
        let p = e.path().to_path_buf();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _e = e;
            panic!("planted failure");
        }));
        assert!(r.is_err());
        p
    })
    .join()
    .unwrap();
    assert!(kept.is_dir(), "TK-174.4");
    std::fs::remove_dir_all(&kept).unwrap();
    // TK-174.5: `keep` hands the directory over; a plain guard dropped while panicking still
    // removes its directory.
    let k = TmpDir::in_temp("tk-174").unwrap();
    let kp = k.keep();
    assert!(kp.is_dir(), "TK-174.5");
    std::fs::remove_dir_all(&kp).unwrap();
    let gone = std::thread::spawn(|| {
        let d = TmpDir::in_temp("tk-174").unwrap();
        let p = d.path().to_path_buf();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _d = d;
            panic!("planted failure");
        }));
        p
    })
    .join()
    .unwrap();
    assert!(!gone.exists(), "TK-174.5");
    // TK-174.6: per-process evidence file names.
    let f = per_process_file(std::path::Path::new("target/dst"), "x-migration", "json");
    assert_eq!(
        f,
        std::path::Path::new("target/dst").join(format!("x-migration-{}.json", std::process::id())),
        "TK-174.6"
    );
}
