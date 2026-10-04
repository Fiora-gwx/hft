//! Layouts: KillWord, CachePadded, Heartbeat, HeartbeatBlock (RT-044, RT-090, RT-091, RT-094).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

use core::mem::{align_of, offset_of, size_of};
use core::sync::atomic::{AtomicU32, Ordering};

use hft_rt::{CachePadded, Heartbeat, HeartbeatBlock, HkLine, KillWord};

#[test]
fn rt_044_killword_layout() {
    assert_eq!(size_of::<KillWord>(), 128);
    assert_eq!(align_of::<KillWord>(), 128);
    let kw: KillWord = CachePadded(AtomicU32::new(0));
    let w: &AtomicU32 = &kw.0;
    w.store(0x8000_0004, Ordering::Release);
    assert_eq!(kw.load(Ordering::Acquire), 0x8000_0004);
}

#[test]
fn rt_090_cache_padded() {
    assert_eq!(
        (size_of::<CachePadded<u64>>(), align_of::<CachePadded<u64>>()),
        (128, 128),
        "RT-090.1"
    );
    assert_eq!(
        (
            size_of::<CachePadded<[u8; 200]>>(),
            align_of::<CachePadded<[u8; 200]>>()
        ),
        (256, 128),
        "RT-090.2"
    );
    assert_eq!(
        (
            size_of::<CachePadded<AtomicU32>>(),
            align_of::<CachePadded<AtomicU32>>()
        ),
        (128, 128),
        "RT-090.3"
    );
}

#[test]
fn rt_091_heartbeat_layout() {
    assert_eq!(size_of::<Heartbeat>(), 128);
    assert_eq!(align_of::<Heartbeat>(), 128);
    assert_eq!(offset_of!(Heartbeat, name), 8);
    assert_eq!(offset_of!(Heartbeat, core), 24);
    assert_eq!(offset_of!(Heartbeat, kind), 26);
    assert_eq!(offset_of!(Heartbeat, _pad), 27);
    // `count` is private; it is the repr(C) first field: offset 0.
    let hb = Heartbeat::new();
    hb.beat();
    // SAFETY: reads the first 8 bytes of a live Heartbeat (the count at offset 0).
    let raw = unsafe { core::ptr::read((&hb as *const Heartbeat).cast::<u64>()) };
    assert_eq!(raw, 1);
}

#[test]
fn rt_094_heartbeat_block_layout() {
    assert_eq!(size_of::<HeartbeatBlock>(), 4096, "RT-094.1");
    assert_eq!(
        (
            offset_of!(HeartbeatBlock, beat),
            offset_of!(HeartbeatBlock, hk),
            offset_of!(HeartbeatBlock, slots)
        ),
        (0, 128, 256),
        "RT-094.2"
    );
    assert_eq!(
        [
            offset_of!(HkLine, magic),
            offset_of!(HkLine, layout_version),
            offset_of!(HkLine, pid),
            offset_of!(HkLine, instance_id),
            offset_of!(HkLine, _rsv),
            offset_of!(HkLine, state),
            offset_of!(HkLine, n_slots),
            offset_of!(HkLine, _p),
        ],
        [0, 8, 12, 16, 17, 18, 19, 20],
        "RT-094.3"
    );
    assert_eq!(
        [
            offset_of!(HkLine, boot_id),
            offset_of!(HkLine, started_utc_ns),
            offset_of!(HkLine, hk_utc_ns)
        ],
        [24, 40, 48],
        "RT-094.4"
    );
    assert_eq!(size_of::<HkLine>(), 56, "RT-094.5");
}
