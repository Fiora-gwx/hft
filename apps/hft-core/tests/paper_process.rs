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
    let result = ctl.confirm(&req, preview.ctl_state_version).unwrap();
    assert_eq!(result.code, Some(ResultCode::Applied), "{result:?}");
    let mut reader = hft_core::storage::reader(&hft_core::config::load(&path, true).unwrap().cfg).unwrap();
    let mut placed = false;
    let mut ack = false;
    let mut canceled = false;
    while !canceled {
        assert!(
            Instant::now() < deadline,
            "placed={placed} ack={ack}; {}",
            std::fs::read_to_string(d.path().join("stderr")).unwrap()
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
