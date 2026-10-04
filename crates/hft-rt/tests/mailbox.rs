//! Mailbox send/take (RT-040).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

use hft_rt::Mailbox;

#[test]
fn rt_040_mailbox_send_take() {
    let mb: &'static Mailbox<Box<u64>> = Box::leak(Box::new(Mailbox::new()));
    let (mut tx, mut rx) = mb.split();
    assert!(tx.try_send(Box::new(1)).is_ok());
    match tx.try_send(Box::new(2)) {
        Err(b) => assert_eq!(*b, 2),
        Ok(()) => panic!("second send must bounce"),
    }
    assert_eq!(rx.try_take().map(|b| *b), Some(1));
    assert_eq!(rx.try_take().map(|b| *b), None);
    assert!(tx.try_send(Box::new(2)).is_ok());
    assert_eq!(rx.try_take().map(|b| *b), Some(2));
}
