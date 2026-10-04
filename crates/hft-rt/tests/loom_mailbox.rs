//! loom: every value sent through a Mailbox is taken exactly once and dropped once (RT-041).
#![cfg(loom)]

use loom::sync::Arc;
use loom::sync::atomic::{AtomicUsize, Ordering};

use hft_rt::Mailbox;

struct Counted {
    v: u32,
    drops: Arc<AtomicUsize>,
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn rt_041_loom_mailbox() {
    loom::model(|| {
        let drops = Arc::new(AtomicUsize::new(0));
        let mb: &'static Mailbox<Counted> = Box::leak(Box::new(Mailbox::new()));
        let (mut tx, mut rx) = mb.split();
        let d = drops.clone();
        let sender = loom::thread::spawn(move || {
            for v in [1u32, 2] {
                let mut item = Counted { v, drops: d.clone() };
                loop {
                    match tx.try_send(item) {
                        Ok(()) => break,
                        Err(back) => {
                            item = back;
                            loom::thread::yield_now();
                        }
                    }
                }
            }
        });
        let mut got = Vec::new();
        while got.len() < 2 {
            match rx.try_take() {
                Some(c) => {
                    got.push(c.v);
                    drop(c);
                }
                None => loom::thread::yield_now(),
            }
        }
        sender.join().unwrap();
        assert_eq!(got, vec![1, 2]);
        assert_eq!(drops.load(Ordering::Relaxed), 2);
        assert!(rx.try_take().is_none());
    });
}
