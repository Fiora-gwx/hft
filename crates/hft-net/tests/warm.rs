//! NET-180: `warm` is a no-op while the optional TLS warm-up is off.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod support;

use hft_net::{MaskedFrame, Opcode};
use hft_testkit::CountingAlloc;
use support::mempeer::pair;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn net_180_warm_disabled_noop() {
    assert!(CountingAlloc::is_installed());
    let (mut conn, _peer) = pair();
    let mut f = MaskedFrame::<2048>::new();
    f.body_mut()[..300].fill(b'w');
    f.seal(Opcode::Text, 300, 3);
    let sys = conn.sys().total();
    let seq = conn.test_tx_seq();
    let tx = conn.tx_pending();
    let scope = CountingAlloc::scope();
    for _ in 0..1000 {
        conn.warm(&f);
    }
    assert_eq!(scope.allocs(), 0);
    assert_eq!(conn.sys().total(), sys);
    assert_eq!(conn.test_tx_seq(), seq);
    assert_eq!(conn.tx_pending(), tx);
    assert_eq!(conn.scratch_sessions(), 0);
}
