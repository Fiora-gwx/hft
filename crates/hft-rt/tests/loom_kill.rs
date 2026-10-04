//! loom: KillWord reads always return a complete written value, in write order (RT-045).
#![cfg(loom)]

use loom::sync::Arc;
use loom::sync::atomic::{AtomicU32, Ordering};

use hft_rt::{CachePadded, KillWord};

#[test]
fn rt_045_loom_killword() {
    loom::model(|| {
        let kw: Arc<KillWord> = Arc::new(CachePadded(AtomicU32::new(0)));
        let w = {
            let kw = kw.clone();
            loom::thread::spawn(move || {
                kw.store(0x0010_0107, Ordering::Release);
                kw.store(0x8000_0004, Ordering::Release);
            })
        };
        let order = |v: u32| match v {
            0 => 0,
            0x0010_0107 => 1,
            0x8000_0004 => 2,
            other => panic!("unwritten value {other:#x}"),
        };
        let a = kw.load(Ordering::Acquire);
        let b = kw.load(Ordering::Acquire);
        assert!(order(b) >= order(a));
        w.join().unwrap();
    });
}
