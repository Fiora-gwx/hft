//! Linux syscall observation, outside measured intervals. Safe IO markers delimit the test thread.
use std::{
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub fn role() -> Option<String> {
    std::env::var("HFT_VEN_TRACE").ok()
}
pub fn register_traced_thread() {}
thread_local! {static OPEN:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
pub fn mark() {
    OPEN.with(|c| {
        let open = !c.get();
        c.set(open);
        std::io::stderr()
            .write_all(if open { b"VEN_BEGIN\n" } else { b"VEN_END\n" })
            .unwrap();
    });
}
pub fn n(m: &BTreeMap<String, u32>, s: &str) -> u32 {
    m.get(s).copied().unwrap_or(0)
}
pub fn strace_segments(test: &str, _: Option<&str>) -> Vec<BTreeMap<String, u32>> {
    let dir = std::env::temp_dir().join(format!("hft-venue-strace-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut child = Command::new("strace")
        .args(["-f", "-ff", "-qq", "-e", "trace=futex,write", "-o"])
        .arg(dir.join("trace"))
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env("HFT_VEN_TRACE", "child")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("CI installs strace");
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "traced child failed");
            break;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("traced child timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut result = Vec::new();
    for file in std::fs::read_dir(&dir).unwrap() {
        let text = std::fs::read_to_string(file.unwrap().path()).unwrap();
        let mut active = None;
        for line in text.lines() {
            if line.contains("VEN_BEGIN") {
                assert!(active.is_none());
                active = Some(BTreeMap::new());
            } else if line.contains("VEN_END") {
                result.push(active.take().expect("unpaired marker"));
            } else if let Some(map) = &mut active
                && let Some((name, _)) = line.split_once('(')
                && matches!(name, "futex" | "write")
            {
                *map.entry(name.to_owned()).or_default() += 1;
            }
        }
        assert!(active.is_none());
    }
    std::fs::remove_dir_all(dir).unwrap();
    result
}
