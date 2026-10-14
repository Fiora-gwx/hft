#![cfg(target_os = "linux")]
mod support;
use std::os::unix::net::UnixDatagram;
use std::sync::atomic::Ordering;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn signal_drain(signal: i32, directed: bool, cancel: bool) {
    let root = tempfile::tempdir().unwrap();
    let hot = tempfile::tempdir_in("/dev/shm").unwrap();
    let user = std::env::var("USER").unwrap();
    let path = support::write(root.path(), hot.path(), Some(&user));
    if !cancel {
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("cancel_open_orders = true", "cancel_open_orders = false");
        std::fs::write(&path, text).unwrap();
    }
    let sock = root.path().join("notify");
    let notify = UnixDatagram::bind(&sock).unwrap();
    notify.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_hft-core"))
            .args(["--dry-run", "--config"])
            .arg(&path)
            .env("NOTIFY_SOCKET", &sock)
            .stdout(Stdio::null())
            .stderr(Stdio::from(
                std::fs::File::create(root.path().join("stderr")).unwrap(),
            ))
            .spawn()
            .unwrap(),
    );
    let mut buf = [0; 256];
    let n = notify.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"READY=1");
    let pid = process.0.id() as i32;
    let expected_mask = [libc::SIGTERM, libc::SIGINT, libc::SIGHUP]
        .into_iter()
        .fold(0u64, |bits, s| bits | (1 << (s - 1)));
    for task in std::fs::read_dir(format!("/proc/{pid}/task")).unwrap() {
        let status = std::fs::read_to_string(task.unwrap().path().join("status")).unwrap();
        let mask = status.lines().find_map(|l| l.strip_prefix("SigBlk:")).unwrap();
        assert_eq!(
            u64::from_str_radix(mask.trim(), 16).unwrap() & expected_mask,
            expected_mask
        );
    }
    assert!(
        std::fs::read_dir(format!("/proc/{pid}/fd"))
            .unwrap()
            .any(|entry| {
                std::fs::read_link(entry.unwrap().path())
                    .is_ok_and(|p| p.to_string_lossy() == "anon_inode:[signalfd]")
            })
    );
    // SIGHUP must leave the process alive and cannot reload unjournaled configuration.
    assert_eq!(unsafe { libc::kill(pid, libc::SIGHUP) }, 0);
    std::thread::sleep(Duration::from_millis(50));
    assert!(process.0.try_wait().unwrap().is_none());
    let rc = if directed {
        unsafe { libc::syscall(libc::SYS_tgkill, pid, pid, signal) as i32 }
    } else {
        unsafe { libc::kill(pid, signal) }
    };
    assert_eq!(rc, 0);
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            assert!(
                status.success(),
                "{}",
                std::fs::read_to_string(root.path().join("stderr")).unwrap()
            );
            break;
        }
        assert!(Instant::now() < deadline, "signal did not drain");
        std::thread::sleep(Duration::from_millis(10));
    }
    let hb = hft_rt::HeartbeatBlock::open_ro(&hot.path().join("sim/heartbeat")).unwrap();
    assert_eq!(hb.hk.state.load(Ordering::Acquire), hft_rt::HkLine::STOPPED);
    let cfg = hft_core::config::load(&path, true).unwrap();
    let mut reader = hft_core::storage::reader(&cfg.cfg).unwrap();
    let mut applied = 0;
    let mut drain_seen = 0;
    loop {
        match reader.poll() {
            hft_journal::Next::Record(r) if r.hdr.kind == hft_event::EventKind::CMD_RESULT.0 => {
                let body = r.read::<hft_event::CmdResult>().unwrap();
                if body.cmd == hft_event::CmdKind::DRAIN
                    && body.status == hft_ctl_proto::ResultCode::Applied as u8
                {
                    applied += 1;
                }
            }
            hft_journal::Next::Event(event) if event.kind == hft_event::EventKind::CONTROL_CMD => {
                let (hdr, cmd) = hft_event::ControlCmd::decode(event.body).unwrap();
                if let hft_event::ControlCmd::Drain { cancel_resting } = cmd {
                    assert_eq!(cancel_resting, cancel);
                    assert_eq!(&hdr.issuer[..11], b"cli:systemd");
                    drain_seen += 1;
                }
            }
            hft_journal::Next::Record(_) | hft_journal::Next::Event(_) => {}
            hft_journal::Next::Err(e) => panic!("{e:?}"),
            _ => break,
        }
    }
    assert_eq!(applied, 1);
    assert_eq!(drain_seen, 1);
}

#[test]
fn core_300_sigterm_journaled_drain() {
    signal_drain(libc::SIGTERM, false, true);
}
#[test]
fn core_301_sigint_journaled_drain() {
    signal_drain(libc::SIGINT, false, true);
}
#[test]
fn core_302_signal_respects_cancel_setting() {
    signal_drain(libc::SIGTERM, false, false);
}
#[test]
fn core_381_main_thread_masks_signals() {
    signal_drain(libc::SIGTERM, true, true);
}
