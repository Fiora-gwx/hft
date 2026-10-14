#![cfg(target_os = "linux")]
mod support;
use hft_ctl::client::CtlClient;
use hft_ctl_proto::{AccountSel, CmdBody, ResultCode};
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

#[test]
fn core_380_ready_precedes_gate_release_and_control_drain_stops_cleanly() {
    use std::os::unix::net::UnixDatagram;
    use std::sync::atomic::Ordering;
    let d = tempfile::tempdir().unwrap();
    let hot = tempfile::tempdir_in("/dev/shm").unwrap();
    let user = std::env::var("USER").unwrap();
    let path = support::write(d.path(), hot.path(), Some(&user));
    // Keep CLOCK closed throughout the test; READY announces usable control/journal,
    // not permission to trade. No exchange or external clock dependency is involved.
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("interval_ms = 1000", "interval_ms = 60000");
    std::fs::write(&path, text).unwrap();
    let notify_path = d.path().join("notify.sock");
    let notify = UnixDatagram::bind(&notify_path).unwrap();
    notify.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let stderr = std::fs::File::create(d.path().join("stderr")).unwrap();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_hft-core"))
            .args(["--dry-run", "--config"])
            .arg(&path)
            .env("NOTIFY_SOCKET", &notify_path)
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()
            .unwrap(),
    );
    let mut datagram = [0; 128];
    let n = notify.recv(&mut datagram).unwrap();
    assert_eq!(&datagram[..n], b"READY=1");
    let heartbeat = hft_rt::HeartbeatBlock::open_ro(&hot.path().join("sim/heartbeat")).unwrap();
    assert_eq!(heartbeat.hk.state.load(Ordering::Acquire), hft_rt::HkLine::WARMUP);
    let deadline = Instant::now() + Duration::from_secs(10);
    while heartbeat.hk.n_slots.load(Ordering::Acquire) != 6
        || heartbeat.slots[..6].iter().any(|h| h.read() == 0)
    {
        assert!(Instant::now() < deadline, "worker did not publish progress");
        assert!(process.0.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut names: Vec<_> = heartbeat.slots[..6]
        .iter()
        .map(|h| {
            let bytes = h.name_bytes();
            String::from_utf8(bytes.into_iter().take_while(|b| *b != 0).collect()).unwrap()
        })
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "ctl-gateway",
            "housekeeping",
            "paper-feed",
            "persist",
            "sequencer",
            "sim-gw"
        ]
    );
    let ctl = CtlClient::new(d.path().join("core.sock"), format!("cli:{user}"));
    let request = ctl.request(
        AccountSel::All,
        CmdBody::Drain {
            cancel_resting: Some(true),
            account: None,
        },
    );
    // The final process teardown may close the socket after journal commit but before
    // the gateway can flush a response. The journal below is the completion authority.
    let _ = ctl.send(&request, Duration::from_secs(3));
    let status = loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "{}", diagnostic(&path, d.path()));
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success(), "{}", diagnostic(&path, d.path()));
    assert_eq!(
        heartbeat.hk.state.load(Ordering::Acquire),
        hft_rt::HkLine::STOPPED
    );
    let config = hft_core::config::load(&path, true).unwrap();
    let mut reader = hft_core::storage::reader(&config.cfg).unwrap();
    let mut result = false;
    loop {
        match reader.poll() {
            hft_journal::Next::Record(r) if r.hdr.kind == hft_event::EventKind::CMD_RESULT.0 => {
                let r = r.read::<hft_event::CmdResult>().unwrap();
                result |= r.cmd == hft_event::CmdKind::DRAIN && r.status == ResultCode::Applied as u8;
            }
            hft_journal::Next::Record(_) | hft_journal::Next::Event(_) => {}
            hft_journal::Next::Err(e) => panic!("{e:?}"),
            _ => break,
        }
    }
    assert!(result, "Drain was not journaled as Applied");
}

#[test]
fn core_048_sim_instance_boots_without_keys() {
    let d = tempfile::tempdir().unwrap();
    let hot = tempfile::tempdir_in("/dev/shm").unwrap();
    let user = std::env::var("USER").unwrap();
    let path = support::write(d.path(), hot.path(), Some(&user));
    let stdout = std::fs::File::create(d.path().join("stdout")).unwrap();
    let stderr = std::fs::File::create(d.path().join("stderr")).unwrap();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_hft-core"))
            .args(["--dry-run", "--config"])
            .arg(&path)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(25);
    while !d.path().join("core.sock").exists() {
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "{}",
            std::fs::read_to_string(d.path().join("stderr")).unwrap()
        );
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut reader = hft_core::storage::reader(&hft_core::config::load(&path, true).unwrap().cfg).unwrap();
    let (_, clock_ok, _) = hft_event::TimerCode::Sys(hft_event::SysTimer::ClockOk)
        .encode()
        .unwrap();
    loop {
        assert!(
            Instant::now() < deadline,
            "startup clock gate: {}",
            diagnostic(&path, d.path())
        );
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "{}",
            diagnostic(&path, d.path())
        );
        match reader.poll() {
            hft_journal::Next::Record(r) if r.hdr.kind == hft_event::EventKind::TIMER.0 => {
                let t = r.read::<hft_event::Timer>().unwrap();
                if t.owner == 1 && t.timer_id == clock_ok {
                    break;
                }
            }
            hft_journal::Next::Err(e) => panic!("{e:?}"),
            _ => std::thread::sleep(Duration::from_millis(1)),
        }
    }
    let ctl = CtlClient::new(d.path().join("core.sock"), format!("cli:{user}"));
    let req = ctl.request(
        AccountSel::Name("paper".into()),
        CmdBody::SetMode {
            strategy: Some("paper-probe".into()),
            group: None,
            account: None,
            mode: hft_types::RunMode::Live,
        },
    );
    let preview = ctl.preview(&req).unwrap();
    assert_eq!(
        preview.code,
        Some(ResultCode::Applied),
        "preview={preview:?}; {}",
        diagnostic(&path, d.path())
    );
    let result = ctl.confirm(&req, preview.ctl_state_version).unwrap();
    assert_eq!(
        result.code,
        Some(ResultCode::Applied),
        "{result:?}; {}",
        diagnostic(&path, d.path())
    );
    let mut placed = false;
    let mut ack = false;
    let mut canceled = false;
    while !canceled {
        assert!(
            Instant::now() < deadline,
            "placed={placed} ack={ack}; {}",
            diagnostic(&path, d.path())
        );
        assert!(process.0.try_wait().unwrap().is_none());
        match reader.poll() {
            hft_journal::Next::Record(r) if r.hdr.kind == hft_event::EventKind::ORDER_EVENT.0 => {
                let e = r.read::<hft_event::OrderEvent>().unwrap();
                ack |= e.what == hft_event::OrderEventKind::Ack;
                canceled |= e.what == hft_event::OrderEventKind::CancelAck;
            }
            hft_journal::Next::Record(r) if r.hdr.kind == hft_event::EventKind::SEND_REPORT.0 => {
                placed = true
            }
            hft_journal::Next::Err(e) => panic!("{e:?}"),
            _ => std::thread::sleep(Duration::from_millis(1)),
        }
    }
    assert!(placed && ack);
    // SIGTERM uses the same authenticated gateway's internal Drain path as systemd.
    assert!(
        Command::new("kill")
            .args(["-TERM", &process.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let status = loop {
        if let Some(s) = process.0.try_wait().unwrap() {
            break s;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    };
    let log = std::fs::read_to_string(d.path().join("stderr")).unwrap();
    let summary = std::fs::read_to_string(d.path().join("stdout")).unwrap();
    assert!(status.success(), "{status} {summary}\n{log}");
    assert!(summary.contains("persisted=true"), "{summary}");
    let heartbeat = hft_rt::HeartbeatBlock::open_ro(&hot.path().join("sim/heartbeat")).unwrap();
    assert_eq!(
        heartbeat.hk.state.load(std::sync::atomic::Ordering::Acquire),
        hft_rt::HkLine::STOPPED
    );
    assert!(heartbeat.sequencer().read() > 0);
    assert!(!d.path().join("core.sock").exists());
    assert!(d.path().join("cold/sim/seq").is_dir());
    let again = Command::new(env!("CARGO_BIN_EXE_hft-core"))
        .args(["--dry-run", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        again.status.code(),
        Some(4),
        "restart must not discard account state"
    );
}

fn diagnostic(config: &std::path::Path, dir: &std::path::Path) -> String {
    let mut kinds = std::collections::BTreeMap::<u16, usize>::new();
    let c = hft_core::config::load(config, true).unwrap();
    let mut reader = hft_core::storage::reader(&c.cfg).unwrap();
    let mut tail = String::new();
    for _ in 0..10000 {
        match reader.poll() {
            hft_journal::Next::Record(r) => *kinds.entry(r.hdr.kind).or_default() += 1,
            hft_journal::Next::Event(e) => *kinds.entry(e.kind.0).or_default() += 1,
            other => {
                tail = format!("{other:?}");
                break;
            }
        }
    }
    format!(
        "journal={kinds:?} tail={tail}; {}",
        std::fs::read_to_string(dir.join("stderr")).unwrap()
    )
}
