//! TK-089: `CountingAlloc` counts only this thread's allocations since the scope began.

use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use hft_testkit::CountingAlloc;

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

#[test]
fn tk_089_counting_alloc() {
    assert!(CountingAlloc::is_installed());

    // TK-089.1: nothing in the scope.
    let s = CountingAlloc::scope();
    assert_eq!(s.allocs(), 0, "TK-089.1");
    assert_eq!(s.bytes(), 0);

    // TK-089.2: one Vec::with_capacity(8).
    let s = CountingAlloc::scope();
    let v: Vec<u64> = black_box(Vec::with_capacity(8));
    assert_eq!(s.allocs(), 1, "TK-089.2");
    assert_eq!(s.bytes(), 64);
    drop(v);

    // TK-089.3: the 9th push into a capacity-8 Vec reallocates once.
    let mut v: Vec<u64> = Vec::with_capacity(8);
    for i in 0..8 {
        v.push(i);
    }
    let s = CountingAlloc::scope();
    v.push(8);
    black_box(&v);
    assert_eq!(s.allocs(), 1, "TK-089.3");
    assert_eq!(s.bytes(), (v.capacity() * size_of::<u64>()) as u64);

    // TK-089.4: another thread allocates 10 times. The thread is spawned before the scope and
    // released through an atomic flag so no allocation of the spawn falls inside the scope.
    let flag = Arc::new(AtomicU8::new(0));
    let f2 = Arc::clone(&flag);
    let h = std::thread::spawn(move || {
        while f2.load(Ordering::Acquire) == 0 {
            std::hint::spin_loop();
        }
        let mut keep = [const { None::<Box<u64>> }; 10];
        for (i, slot) in keep.iter_mut().enumerate() {
            *slot = Some(black_box(Box::new(i as u64)));
        }
        black_box(&keep);
        f2.store(2, Ordering::Release);
    });
    let s = CountingAlloc::scope();
    flag.store(1, Ordering::Release);
    while flag.load(Ordering::Acquire) != 2 {
        std::hint::spin_loop();
    }
    let n = s.allocs();
    let bytes = s.bytes();
    h.join().expect("allocating thread");
    assert_eq!(n, 0, "TK-089.4");
    assert_eq!(bytes, 0);

    // TK-089.5: dropping a Vec is not counted.
    let v: Vec<u64> = black_box(vec![1, 2, 3]);
    let s = CountingAlloc::scope();
    drop(black_box(v));
    assert_eq!(s.allocs(), 0, "TK-089.5");
    assert_eq!(s.bytes(), 0);
}
