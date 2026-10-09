//! NET-360: TX timestamps stay off by default (no TX flags, no `dup`, no `TxTag`) (Linux).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(target_os = "linux")]

use crate::support;

use hft_net::sock::opt;
use hft_net::*;
use support::looppeer::*;

#[test]
fn net_360_tx_ts_off() {
    let peer = LoopPeer::start(LoopOpts::default());
    let cfg = net_cfg(peer.port);
    assert!(!cfg.tx_timestamps);
    let arena = ConnArena::new(&[(ConnRole::Order, 1)], &cfg, clock());
    let mut c = Connector::new(arena, &cfg).unwrap();
    let (h, mut pc) = open(&mut c, &peer, 1, ConnRole::Order);
    let mut p = pool();
    let id = p.install(h, None).unwrap();
    let conn = p.get(id).unwrap();
    let mut f = MaskedFrame::<2048>::new();
    for i in 0..10u32 {
        f.unseal();
        f.body_mut()[..4].copy_from_slice(&i.to_be_bytes());
        f.seal(Opcode::Text, 4, i);
        conn.send_frame(&f).unwrap();
    }
    let mut acc = Vec::new();
    assert_eq!(pc.read_frames(10, &mut acc).len(), 10);
    let mut v: i32 = 0;
    let mut l = 4u32;
    let r = unsafe {
        libc::getsockopt(
            conn.raw_fd(),
            libc::SOL_SOCKET,
            opt::SO_TIMESTAMPING,
            (&mut v as *mut i32).cast(),
            &mut l,
        )
    };
    assert_eq!(r, 0);
    assert_eq!(
        v & (opt::TS_TX_SCHED | opt::TS_TX_SOFTWARE | opt::TS_OPT_ID | opt::TS_OPT_TSONLY),
        0
    );
    assert_eq!(conn.sys().dup, 0);
    // No TxTag source exists while tx_timestamps is off: the send path has no TX-stamp output.
    assert_eq!(c.sys().dup, 0);
}
