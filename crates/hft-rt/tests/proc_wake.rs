//! A producer without a wake hook makes no system call on commit (RT-010). Linux + strace.
#![cfg(all(target_os = "linux", not(loom)))]

#[allow(dead_code)]
mod common {
    pub mod proc;
}

use common::proc::*;
use hft_rt::{Backing, cpu_relax, spsc};

const N: u64 = 100_000;

#[test]
fn rt_010_no_wake_no_syscall() {
    if role().as_deref() == Some("traced") {
        let (mut p, mut c) = spsc::<u64>(1024, Backing::Heap).unwrap();
        p.set_wake(None);
        let cons = std::thread::spawn(move || {
            let mut want = 0;
            while want < N {
                match c.try_pop() {
                    Some(v) => {
                        assert_eq!(v, want);
                        want += 1;
                    }
                    None => cpu_relax(),
                }
            }
        });
        register_traced_thread();
        mark();
        for i in 0..N {
            while p.try_push(i).is_err() {
                cpu_relax();
            }
        }
        mark();
        cons.join().unwrap();
        return;
    }
    let segs = strace_segments("rt_010_no_wake_no_syscall", Some("futex,write,epoll_wait"));
    assert_eq!(segs.len(), 1);
    assert_eq!((n(&segs[0], "futex"), n(&segs[0], "write")), (0, 0), "{segs:?}");
}
