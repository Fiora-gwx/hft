//! `DurableMeta::write_sync` system-call order under strace (JNL-098). Linux, CI installs strace.
#![cfg(target_os = "linux")]

#[path = "../common/mod.rs"]
mod common;

use std::process::{Command, Stdio};

use common::harness::*;
use common::proc::*;
use hft_journal::{DurableMeta, InstanceName};

const T: &str = "jnl_098_write_sync_order";

#[test]
fn jnl_098_write_sync_order() {
    if role().is_some() {
        let m = DurableMeta::open(std::path::Path::new(&var("JNL_ROOT")), &InstanceName::Prod).unwrap();
        // Marker: the traced region starts after this line.
        std::fs::write(var("JNL_MARK"), b"go").unwrap();
        m.write_sync("engine_epoch", 9).unwrap();
        exit(0);
    }
    if Command::new("strace")
        .arg("-V")
        .stdout(Stdio::null())
        .status()
        .is_err()
    {
        eprintln!("prerequisite missing: strace (installed by the CI job)");
        return;
    }
    let root = cold_dir();
    let trace = root.join("trace");
    let mark = root.join("mark");
    let st = Command::new("strace")
        .args(["-f", "-qq", "-o"])
        .arg(&trace)
        .args([
            "-e",
            "trace=openat,write,fsync,fdatasync,rename,renameat,renameat2",
        ])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", T, "--nocapture", "--test-threads=1"])
        .env(ROLE, "traced")
        .env("JNL_ROOT", &root)
        .env("JNL_MARK", &mark)
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(st.success());
    let t = std::fs::read_to_string(&trace).unwrap();
    let lines: Vec<&str> = t.lines().skip_while(|l| !l.contains("mark")).collect();
    let pos = |pred: &dyn Fn(&str) -> bool| lines.iter().position(|l| pred(l)).unwrap_or(usize::MAX);
    let open_tmp = pos(&|l| l.contains("openat") && l.contains("engine_epoch.tmp"));
    let fd = |i: usize| -> String { lines[i].rsplit("= ").next().unwrap().trim().to_string() };
    let tmp_fd = fd(open_tmp);
    let write = pos(&|l| l.contains(&format!("write({tmp_fd},")));
    let fsync_tmp = pos(&|l| l.contains(&format!("fsync({tmp_fd})")));
    let rename =
        pos(&|l| l.contains("rename") && l.contains("engine_epoch.tmp") && l.contains("meta/engine_epoch\""));
    let open_dir = lines
        .iter()
        .enumerate()
        .skip(rename.min(lines.len()))
        .find(|(_, l)| l.contains("openat") && l.contains("/meta\""))
        .map(|(i, _)| i)
        .unwrap_or(usize::MAX);
    let dir_fd = fd(open_dir);
    let fsync_dir = lines
        .iter()
        .enumerate()
        .skip(open_dir)
        .find(|(_, l)| l.contains(&format!("fsync({dir_fd})")))
        .map(|(i, _)| i)
        .unwrap_or(usize::MAX);
    assert!(
        open_tmp < write
            && write < fsync_tmp
            && fsync_tmp < rename
            && rename < open_dir
            && open_dir < fsync_dir,
        "{lines:#?}"
    );
    assert!(fsync_dir < lines.len());
}
