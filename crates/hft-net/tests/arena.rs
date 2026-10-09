//! NET-287: a connection moves warm → hot → warm through arena slot numbers on SPSC rings (Linux).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(target_os = "linux")]

use crate::support;

use std::time::{Duration, Instant};

use hft_net::*;
use hft_rt::{Backing, spsc};
use support::looppeer::*;

#[test]
fn net_287_handoff_cycle() {
    let peer = LoopPeer::start(LoopOpts::default());
    let mut c = connector(peer.port, &[(ConnRole::Md, 2)]);
    let (mut to_hot, mut from_warm) = spsc::<Handoff>(4, Backing::Heap).unwrap();
    let (mut to_warm, mut from_hot) = spsc::<RetireMsg>(4, Backing::Heap).unwrap();
    let hot = std::thread::spawn(move || {
        let mut p: ConnPool<2> = ConnPool::new(PoolMode::Race, 1, RotatePolicy::default());
        let mut served = 0u32;
        let end = Instant::now() + Duration::from_secs(60);
        while served < 10 && Instant::now() < end {
            let Some(h) = from_warm.try_pop() else {
                std::hint::spin_loop();
                continue;
            };
            let id = p.install(h, None).unwrap();
            let conn = p.get(id).unwrap();
            let mut n = 0;
            while n < 10 && Instant::now() < end {
                conn.poll_rx(0);
                while let Some(f) = conn.next_frame() {
                    assert_eq!(f.payload, format!("down{n}").as_bytes());
                    n += 1;
                }
            }
            for k in 0..10 {
                conn.send_text(format!("up{k}").as_bytes(), 0).unwrap();
                conn.finish_poll(0);
            }
            while conn.tx_pending() > 0 {
                conn.finish_poll(0);
            }
            let m = p.retire(id, ErrBits::IDLE_TIMEOUT).unwrap();
            assert!(to_warm.try_push(m).is_ok());
            served += 1;
        }
        served
    });
    let mut held = [false; 2];
    let mut in_flight = 0u32;
    for cycle in 0..10u8 {
        let (h, mut pc) = open(&mut c, &peer, cycle + 1, ConnRole::Md);
        let slot = h.slot() as usize;
        assert!(slot < 2, "slot {slot}");
        assert!(!held[slot], "slot {slot} handed out twice");
        held[slot] = true;
        in_flight += 1;
        assert!(to_hot.try_push(h).is_ok());
        for k in 0..10 {
            pc.text(format!("down{k}").as_bytes());
        }
        let mut acc = Vec::new();
        let up = pc.read_frames(10, &mut acc);
        assert_eq!(up.len(), 10, "cycle {cycle}");
        let end = Instant::now() + Duration::from_secs(10);
        let m = loop {
            if let Some(m) = from_hot.try_pop() {
                break m;
            }
            assert!(Instant::now() < end, "no RetireMsg");
            std::thread::sleep(Duration::from_micros(100));
        };
        assert_eq!(m.slot() as usize, slot);
        held[slot] = false;
        in_flight -= 1;
        c.reclaim(m);
        assert!(pc.sees_eof(1000), "cycle {cycle}: peer sees FIN");
    }
    assert_eq!(hot.join().unwrap(), 10);
    assert_eq!(in_flight, 0);
    assert_eq!(c.test_free_slots(), 2);
}
