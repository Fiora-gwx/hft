//! CTL-001–CTL-003.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use hft_ctl::config::{
    DEFAULT_DEDUP_CAPACITY, DEFAULT_RESULT_WAIT_MS, DEFAULT_WARMUP_RATE_PER_S, DEFAULT_WRITE_RATE_PER_MIN,
};
use hft_ctl::{GatewayConfig, PrincipalTable};

#[test]
fn ctl_001_config_defaults() {
    let c = GatewayConfig::new("/tmp/x.sock".into(), PrincipalTable::default(), 256, 1024, 8);
    assert_eq!(c.result_wait_ms, 5_000, "CTL-001.1");
    assert_eq!(c.write_rate_per_min, 12, "CTL-001.2");
    assert_eq!(c.warmup_rate_per_s, 50, "CTL-001.3");
    assert_eq!(c.dedup_capacity, 4_096, "CTL-001.4");
    assert_eq!(
        (
            DEFAULT_RESULT_WAIT_MS,
            DEFAULT_WRITE_RATE_PER_MIN,
            DEFAULT_WARMUP_RATE_PER_S,
            DEFAULT_DEDUP_CAPACITY
        ),
        (5_000, 12, 50, 4_096)
    );
    // CTL-001.5: the advisor budgets (and the group capacity) have no default: `new` takes them.
    assert_eq!(
        (
            c.advisor_mem_budget_mib,
            c.advisor_ckpt_max_mib,
            c.max_groups_per_instance
        ),
        (256, 1024, 8)
    );
}

#[cfg(target_os = "linux")]
mod linux {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};

    use hft_ctl::VecRing;
    use hft_ctl::gateway::Gateway;
    use hft_rt::{Backing, Mailbox, PollTask, spsc};
    use hft_types::Tsc;

    use crate::common::sock::{ResQ, Sock, tmpdir};
    use crate::common::*;

    fn try_new(socket: std::path::PathBuf, ring: usize) -> Result<(), String> {
        let (p_send, _p) = Box::leak(Box::new(Mailbox::new())).split();
        let (_r, r_take) = Box::leak(Box::new(Mailbox::new())).split();
        let (_i, i_cons) = spsc(16, Backing::Heap).unwrap();
        Gateway::<CtlTestSet, VecRing, ResQ>::new(
            config(socket),
            p0::p0(),
            VecRing::with_free(ring),
            ring,
            p_send,
            r_take,
            i_cons,
            refdata_check,
            risk_decode,
            Vec::new(),
            ResQ::default(),
            clock(),
            Default::default(),
        )
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
    }

    #[test]
    fn ctl_002_new_validation() {
        let d = tmpdir("002");
        let s = d.join("ctl.sock");
        try_new(s.clone(), 262_144).unwrap();
        let m = std::fs::metadata(&s).unwrap();
        assert!(m.file_type().is_socket(), "CTL-002.1");
        assert_eq!(m.permissions().mode() & 0o777, 0o660, "CTL-002.1");
        let s2 = d.join("ctl2.sock");
        assert!(try_new(s2.clone(), 262_143).is_err(), "CTL-002.2");
        assert!(!s2.exists(), "CTL-002.2");
        try_new(d.join("ctl3.sock"), 524_288).unwrap();
        assert!(try_new(d.join("missing/ctl.sock"), 262_144).is_err(), "CTL-002.4");
    }

    fn inodes(table: &str) -> Vec<String> {
        std::fs::read_to_string(table)
            .unwrap_or_default()
            .lines()
            .skip(1)
            .filter_map(|l| l.split_whitespace().nth(9).map(str::to_owned))
            .collect()
    }

    #[test]
    fn ctl_003_unix_socket_only() {
        let mut s = Sock::new("003", |_| {});
        let _c = s.connect_as(1201);
        for _ in 0..100 {
            s.gw.poll_once(Tsc(0));
        }
        let tcp: Vec<String> = ["/proc/net/tcp", "/proc/net/tcp6"]
            .iter()
            .flat_map(|t| inodes(t))
            .collect();
        // No socket of this process is TCP (other tests of the binary may open Unix pairs or
        // other non-TCP sockets concurrently; only TCP is excluded process-wide).
        for e in std::fs::read_dir("/proc/self/fd").unwrap().flatten() {
            let Ok(l) = std::fs::read_link(e.path()) else {
                continue;
            };
            let l = l.to_string_lossy().into_owned();
            if let Some(ino) = l.strip_prefix("socket:[").and_then(|x| x.strip_suffix(']')) {
                assert!(!tcp.iter().any(|t| t == ino), "TCP socket fd {l}");
            }
        }
        // The gateway's own sockets: one AF_UNIX listener and the AF_UNIX connection it accepted.
        let mut w = hft_rt::WaitFds::new();
        hft_rt::PollTask::wait_fds(&s.gw, &mut w);
        assert_eq!(w.as_slice().len(), 2);
        let opt = |fd: i32, name: i32| {
            let mut v: libc::c_int = 0;
            let mut len = size_of::<libc::c_int>() as libc::socklen_t;
            // SAFETY: `v` and `len` are valid for writes of the sizes passed; `fd` is open.
            let rc =
                unsafe { libc::getsockopt(fd, libc::SOL_SOCKET, name, (&raw mut v).cast(), &raw mut len) };
            assert_eq!(rc, 0);
            v
        };
        for (i, &fd) in w.as_slice().iter().enumerate() {
            assert_eq!(opt(fd, libc::SO_DOMAIN), libc::AF_UNIX, "fd {fd}");
            assert_eq!(opt(fd, libc::SO_ACCEPTCONN), i32::from(i == 0), "fd {fd}");
        }
    }
}
