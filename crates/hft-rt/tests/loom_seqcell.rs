//! loom: SeqCell readers never see a torn value and never go backwards (RT-042).
//!
//! Each read is one `try_load` attempt (`load` = retry `try_load` until `Some`): a retry loop
//! lets loom read stale values forever, so the model checks every successful attempt instead.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(loom)]

use loom::sync::Arc;

use hft_rt::SeqCell;

#[test]
fn rt_042_loom_seqcell() {
    loom::model(|| {
        let cell = Arc::new(SeqCell::new((0u64, 0u64)));
        let w = {
            let cell = cell.clone();
            loom::thread::spawn(move || {
                cell.store((1, 1));
                cell.store((2, 2));
            })
        };
        let readers: Vec<_> = (0..1)
            .map(|_| {
                let cell = cell.clone();
                loom::thread::spawn(move || {
                    let a = cell.try_load();
                    let b = cell.try_load();
                    for v in [a, b].into_iter().flatten() {
                        assert_eq!(v.0, v.1, "torn read");
                    }
                    if let (Some(a), Some(b)) = (a, b) {
                        assert!(b.0 >= a.0, "went backwards");
                    }
                })
            })
            .collect();
        w.join().unwrap();
        for r in readers {
            r.join().unwrap();
        }
        // After the writer is done, a read always succeeds with the last value.
        assert_eq!(cell.load(), (2, 2));
    });
}
