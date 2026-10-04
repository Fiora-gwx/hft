//! loom: the wake protocol never loses a wake-up, and a producer that sees PARKING makes no
//! wake system call (RT-116). The consumer's wait is "spin until the state leaves PARKED"; the
//! wake system call is replaced by a counter.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(loom)]

use loom::sync::Arc;
use loom::sync::atomic::{AtomicUsize, Ordering};

use hft_rt::{Backing, ParkState, spsc};

#[test]
fn rt_116_loom_wake_protocol() {
    loom::model(|| {
        let st = Arc::new(ParkState::new());
        let wakes = Arc::new(AtomicUsize::new(0));
        let (mut p, mut c) = spsc::<u64>(2, Backing::Heap).unwrap();
        let prod = {
            let (st, wakes) = (st.clone(), wakes.clone());
            loom::thread::spawn(move || {
                p.try_push(7).unwrap();
                // Producer side of the protocol (what `Unparker::unpark` does).
                if st.notify() {
                    wakes.fetch_add(1, Ordering::Relaxed);
                }
            })
        };
        // Consumer: one park attempt, then drain.
        st.begin_park();
        let mut parked = false;
        if c.len_hint() > 0 {
            st.finish_park();
        } else if st.commit_park() {
            parked = true;
            // Wait: futex_wait(state, PARKED) returns once the state changes.
            while st.load() == ParkState::PARKED {
                loom::thread::yield_now();
            }
            st.finish_park();
        } else {
            st.finish_park();
        }
        prod.join().unwrap();
        // No lost wake-up: the data is there and the consumer is running.
        assert_eq!(c.try_pop(), Some(7));
        assert_eq!(st.load(), ParkState::RUNNING);
        // A wake system call happens exactly when the consumer really parked.
        assert_eq!(wakes.load(Ordering::Relaxed), parked as usize);
    });
}
