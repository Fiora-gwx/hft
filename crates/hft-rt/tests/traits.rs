//! Endpoint Send/Sync/Clone constraints, checked at compile time (RT-007).
#![cfg(not(loom))]

use hft_rt::{ByteConsumer, ByteProducer, Consumer, MailReceiver, MailSender, Producer};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[test]
fn rt_007_endpoint_traits() {
    // RT-007.1
    assert_impl_all!(Producer<u64>: Send);
    assert_not_impl_any!(Producer<u64>: Sync, Clone);
    // RT-007.2
    assert_impl_all!(Consumer<u64>: Send);
    assert_not_impl_any!(Consumer<u64>: Sync, Clone);
    // RT-007.3
    assert_not_impl_any!(MailSender<Box<u64>>: Clone);
    assert_not_impl_any!(MailReceiver<Box<u64>>: Clone);
    // RT-007.4
    assert_not_impl_any!(ByteProducer: Clone);
    assert_not_impl_any!(ByteConsumer: Clone);
}
