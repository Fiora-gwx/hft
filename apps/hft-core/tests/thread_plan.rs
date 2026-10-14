mod support;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, toml::Value) {
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    (d, path, value)
}

#[test]
fn core_061_current_tasks_are_required_exactly_once() {
    let (_d, path, base) = fixture();
    hft_core::config::load(&path, true).unwrap();
    let tasks = base["threads"].as_array().unwrap();
    for i in 0..tasks.len() {
        for duplicate in [false, true] {
            let mut value = base.clone();
            let changed = value["threads"].as_array_mut().unwrap();
            if duplicate {
                let mut row = changed[i].clone();
                row["name"] = "duplicate".into();
                changed.push(row);
            } else {
                changed.remove(i);
            }
            std::fs::write(&path, toml::to_string(&value).unwrap()).unwrap();
            let errors = hft_core::config::load(&path, true).unwrap_err();
            assert!(errors.iter().any(|e| e.code == "ThreadPlan"), "{errors:?}");
        }
    }
}

#[test]
fn core_070_housekeeping_priority_is_validated_at_load() {
    let (_d, path, base) = fixture();
    let hk = base["threads"]
        .as_array()
        .unwrap()
        .iter()
        .position(|t| t["task"].as_str() == Some("housekeeping"))
        .unwrap();
    for (nice, valid) in [(0, true), (5, false), (19, false), (20, false)] {
        let mut value = base.clone();
        value["threads"][hk]["nice"] = nice.into();
        std::fs::write(&path, toml::to_string(&value).unwrap()).unwrap();
        let result = hft_core::config::load(&path, true);
        assert_eq!(result.is_ok(), valid, "nice={nice}: {result:?}");
        if let Err(errors) = result {
            assert!(errors.iter().any(|e| e.code == "ThreadPlan"));
        }
    }
    let mut value = base;
    value["threads"][hk].as_table_mut().unwrap().remove("nice");
    std::fs::write(&path, toml::to_string(&value).unwrap()).unwrap();
    assert!(
        hft_core::config::load(&path, true)
            .unwrap_err()
            .iter()
            .any(|e| { e.code == "MissingField" && e.detail == format!("threads[{hk}].nice") })
    );
}

#[cfg(target_os = "linux")]
mod linux {
    use hft_core::{config::Thread, runtime::Workers};
    use std::sync::atomic::Ordering;

    fn allowed() -> libc::cpu_set_t {
        // SAFETY: cpu_set_t is a plain bit set, initialized by sched_getaffinity.
        let mut set = unsafe { std::mem::zeroed() };
        // SAFETY: set is a writable buffer of the supplied size; pid 0 is this thread.
        assert_eq!(
            unsafe { libc::sched_getaffinity(0, size_of::<libc::cpu_set_t>(), &mut set) },
            0
        );
        set
    }

    fn contains(set: &libc::cpu_set_t, cpu: u16) -> bool {
        // SAFETY: callers supply indices below 256, within CPU_SETSIZE.
        unsafe { libc::CPU_ISSET(usize::from(cpu), set) }
    }

    #[test]
    fn core_084_worker_applies_policy_before_task_and_beats_own_slot() {
        let set = allowed();
        let cpu = (0..256).find(|cpu| contains(&set, *cpu)).unwrap();
        let d = tempfile::tempdir().unwrap();
        let hb = hft_rt::HeartbeatBlock::create(&d.path().join("heartbeat"), 2).unwrap();
        let mut workers = Workers::new(hb);
        for (index, task, nice) in [(0, "housekeeping", 0), (1, "persist", 10), (2, "ctl-gateway", 10)] {
            let (tx, rx) = std::sync::mpsc::channel();
            workers
                .spawn(
                    Thread {
                        name: task.into(),
                        task: task.into(),
                        core: Some(cpu),
                        park_us: 100,
                        nice,
                    },
                    move |_, context| {
                        let set = allowed();
                        assert_eq!((0..256).filter(|c| contains(&set, *c)).collect::<Vec<_>>(), [cpu]);
                        // SAFETY: these calls query the calling thread; the prctl buffer is writable.
                        assert_eq!(unsafe { libc::prctl(libc::PR_GET_TIMERSLACK, 0, 0, 0, 0) }, 1);
                        // SAFETY: PRIO_PROCESS/id 0 queries this Linux thread.
                        assert_eq!(
                            unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) },
                            i32::from(nice)
                        );
                        let mut name = [0u8; 16];
                        // SAFETY: PR_GET_NAME writes at most 16 bytes to this buffer.
                        assert_eq!(
                            unsafe { libc::prctl(libc::PR_GET_NAME, name.as_mut_ptr(), 0, 0, 0) },
                            0
                        );
                        assert_eq!(&name[..task.len()], task.as_bytes());
                        assert_eq!(context.heartbeat.name_bytes()[..task.len()], *task.as_bytes());
                        context.heartbeat.beat();
                        tx.send(()).unwrap();
                        Ok(())
                    },
                )
                .unwrap();
            rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            assert_eq!(hb.hk.n_slots.load(Ordering::Acquire), index + 1);
            assert_eq!(hb.slots[usize::from(index)].read(), 1);
        }
        workers.join().unwrap();
    }

    #[test]
    fn core_086_thread_policy_does_not_inherit_batch_scheduler() {
        std::thread::spawn(|| {
            let param = libc::sched_param { sched_priority: 0 };
            // SAFETY: valid sched_param; pid 0 is this disposable test thread.
            assert_eq!(
                unsafe { libc::sched_setscheduler(0, libc::SCHED_BATCH, &param) },
                0
            );
            hft_core::runtime::pin(&Thread {
                name: "policy-test".into(),
                task: "housekeeping".into(),
                core: None,
                park_us: 100,
                nice: 0,
            })
            .unwrap();
            // SAFETY: sched_getscheduler queries the calling thread for pid 0.
            assert_eq!(unsafe { libc::sched_getscheduler(0) }, libc::SCHED_OTHER);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn core_084_invalid_worker_affinity_never_announces_ready() {
        use std::{os::unix::net::UnixDatagram, process::Command};
        let set = allowed();
        let cpu = (0..256).find(|cpu| !contains(&set, *cpu)).unwrap();
        let d = tempfile::tempdir().unwrap();
        let hot = tempfile::tempdir_in("/dev/shm").unwrap();
        let user = std::env::var("USER").unwrap();
        let path = super::support::write(d.path(), hot.path(), Some(&user));
        let mut cfg: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let persist = cfg["threads"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|t| t["task"].as_str() == Some("persist"))
            .unwrap();
        persist
            .as_table_mut()
            .unwrap()
            .insert("core".into(), i64::from(cpu).into());
        std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
        let socket = d.path().join("notify.sock");
        let notify = UnixDatagram::bind(&socket).unwrap();
        notify.set_nonblocking(true).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_hft-core"))
            .args(["--dry-run", "--run-for-ms", "1000", "--config"])
            .arg(&path)
            .env("NOTIFY_SOCKET", socket)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(3),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stderr).contains("error[Affinity]"));
        let mut buffer = [0; 128];
        while let Ok(n) = notify.recv(&mut buffer) {
            assert_ne!(&buffer[..n], b"READY=1");
        }
    }
}
