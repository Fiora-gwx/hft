use hft_core::startup::{Config, Live};
use hft_types::Tsc;

#[test]
fn core_221_startup_step_flags_and_late_release() {
    let config = Config {
        budget_s: 1,
        gate_timeout_s: 2,
    };
    let mut live = Live::start(config, 100, Tsc(100), 1_000_000_000);
    live.observe(Tsc(1_500_100), true, false);
    live.observe(Tsc(3_500_100), true, false);
    let mut notes = Vec::new();
    while let Some(note) = live.pop() {
        notes.push(note);
    }
    assert_eq!(
        notes.iter().map(|n| (n.v[0], n.v[1])).collect::<Vec<_>>(),
        [(6, 0), (6, 1), (7, 0), (7, 1)]
    );
    assert_eq!(notes[1].v[3], 1);
    assert_eq!(notes[3].v[3], 3, "budget and gate timeout are independent flags");
    assert!(notes.iter().filter(|n| n.v[1] == 0).all(|n| n.v[3] == 0));
    live.observe(Tsc(4_000_100), true, true);
    assert_eq!(live.pop().unwrap().v[..2], [8, 0]);
    assert_eq!(live.pop().unwrap().v[..2], [8, 1]);
    live.observe(Tsc(5_000_100), true, true);
    assert!(live.is_empty());
}

#[cfg(target_os = "linux")]
mod support;

#[cfg(target_os = "linux")]
mod process {
    use super::support;
    use hft_event::{EventKind, NoteSource, note_code};
    use std::os::unix::net::UnixDatagram;
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    #[test]
    fn core_220_startup_step_notes() {
        let root = tempfile::tempdir().unwrap();
        let hot = tempfile::tempdir_in("/dev/shm").unwrap();
        let path = support::write(root.path(), hot.path(), None);
        let socket = root.path().join("notify");
        let notify = UnixDatagram::bind(&socket).unwrap();
        notify.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        let mut process = Command::new(env!("CARGO_BIN_EXE_hft-core"))
            .args([
                "--config",
                path.to_str().unwrap(),
                "--dry-run",
                "--run-for-ms",
                "4500",
            ])
            .env("NOTIFY_SOCKET", &socket)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut notices = Vec::new();
        loop {
            let mut buffer = [0u8; 256];
            if let Ok(n) = notify.recv(&mut buffer) {
                notices.push(String::from_utf8(buffer[..n].to_vec()).unwrap());
            }
            if process.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = process.kill();
                panic!("startup timed out");
            }
        }
        let output = process.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(notices.iter().filter(|n| *n == "READY=1").count(), 1);
        let ready = notices.iter().position(|n| n == "READY=1").unwrap();
        assert!(notices.iter().position(|n| n == "STATUS=Startup step 5").unwrap() < ready);
        assert!(notices.iter().position(|n| n == "STATUS=Startup step 6").unwrap() > ready);
        let cfg = hft_core::config::load(&path, true).unwrap();
        let mut reader = hft_core::storage::reader(&cfg.cfg).unwrap();
        let mut steps = Vec::new();
        let mut first_input = None;
        loop {
            match reader.poll() {
                hft_journal::Next::Record(r) if r.hdr.kind == EventKind::NOTE.0 => {
                    let n = r.read::<hft_event::Note>().unwrap();
                    if n.code == note_code(NoteSource::Boot, 6) {
                        steps.push(n);
                    }
                }
                hft_journal::Next::Record(r) => {
                    if r.hdr.kind == EventKind::ENGINE_CLOCK.0 {
                        first_input = Some(r.hdr.seq);
                    }
                }
                hft_journal::Next::Event(_) => {}
                hft_journal::Next::Err(e) => panic!("{e:?}"),
                _ => break,
            }
        }
        assert!(first_input.is_some());
        assert_eq!(steps.len(), 16);
        for (i, note) in steps.iter().enumerate() {
            assert_eq!((note.v[0], note.v[1]), ((i / 2 + 1) as u64, (i % 2) as u64));
            assert_eq!(note.v[3], 0);
        }
        assert!(steps.windows(2).all(|p| p[0].v[2] <= p[1].v[2]));
    }
}
