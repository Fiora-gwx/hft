//! Cold copy directory modes (JNL-092). Linux, non-root, with a supplementary group.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[allow(unused_imports)] // used by the Linux-only tests below
use crate::common;

#[cfg(target_os = "linux")]
#[test]
fn jnl_092_cold_subdir_mode() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use common::harness::*;
    use hft_journal::*;

    // SAFETY: getuid/getgid/umask have no preconditions; this test binary has one test, so the
    // process-wide umask change does not race with other tests.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    if uid == 0 {
        eprintln!("JNL-092: needs a non-root user; prerequisite missing");
        return;
    }
    // A supplementary group of this user other than the primary group.
    let groups = std::process::Command::new("id").arg("-G").output().unwrap();
    let g: Option<u32> = String::from_utf8_lossy(&groups.stdout)
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .find(|g| *g != gid);
    let Some(g) = g else {
        eprintln!("JNL-092: the user has no supplementary group; prerequisite missing");
        return;
    };
    // SAFETY: see above.
    unsafe { libc::umask(0o077) };
    for (row, parent_mode, want_mode, want_gid) in
        [("JNL-092.1", 0o750, 0o770, gid), ("JNL-092.2", 0o2750, 0o2770, g)]
    {
        let cold = cold_dir();
        let prod = cold.join("prod");
        std::fs::create_dir(&prod).unwrap();
        std::os::unix::fs::chown(&prod, None, Some(g)).unwrap();
        std::fs::set_permissions(&prod, std::fs::Permissions::from_mode(parent_mode)).unwrap();
        let mut h = H::open0(c0(&linux_hot(), &cold));
        h.events(2);
        h.persist_all();
        DurableMeta::open(&cold, &InstanceName::Prod).unwrap();
        for d in [
            prod.join("seq"),
            prod.join("seq").join("20261001"),
            prod.join("meta"),
        ] {
            let m = std::fs::metadata(&d).unwrap();
            assert_eq!(m.permissions().mode() & 0o7777, want_mode, "{row} {d:?}");
            assert_eq!(m.gid(), want_gid, "{row} {d:?}");
        }
    }
}
