//! Pinning and thread names (RT-070, RT-071). Linux.
#![cfg(all(target_os = "linux", not(loom)))]

use hft_rt::{RtErr, name_current_thread, pin_current_thread};

fn online() -> u16 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u16)
        .unwrap_or(1)
}

#[test]
fn rt_070_pin_current_thread() {
    let rows: [(&str, u16, Result<(), RtErr>); 3] = [
        ("RT-070.1", 0, Ok(())),
        ("RT-070.2", online() - 1, Ok(())),
        ("RT-070.3", 255, Err(RtErr::Os(22))),
    ];
    for (row, cpu, want) in rows {
        let (got, now) = std::thread::spawn(move || {
            let r = pin_current_thread(cpu);
            // SAFETY: no arguments.
            (r, unsafe { libc::sched_getcpu() })
        })
        .join()
        .unwrap();
        assert_eq!(got, want, "{row}");
        if want.is_ok() {
            assert_eq!(now, cpu as i32, "{row}");
        }
    }
}

#[test]
fn rt_071_thread_name() {
    let rows = [
        ("RT-071.1", "feed-bbo-bn", "feed-bbo-bn"),
        ("RT-071.2", "abcdefghijklmno", "abcdefghijklmno"),
        ("RT-071.3", "abcdefghijklmnopqrst", "abcdefghijklmno"),
    ];
    for (row, name, want) in rows {
        let comm = std::thread::spawn(move || {
            name_current_thread(name);
            // SAFETY: no arguments.
            let tid = unsafe { libc::gettid() };
            std::fs::read_to_string(format!("/proc/self/task/{tid}/comm")).unwrap()
        })
        .join()
        .unwrap();
        assert_eq!(comm.trim_end(), want, "{row}");
    }
}
