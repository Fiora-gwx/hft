//! NET-086, NET-087, NET-135, NET-163: the receive and send paths on real loopback sockets (Linux).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(target_os = "linux")]

use crate::support;

use std::time::{Duration, Instant};

use hft_net::*;
use hft_rt::tsc_now;
use hft_types::Tsc;
use support::looppeer::*;
use support::ws_ref;

fn one(md_rx: u32) -> (LoopPeer, Connector, Box<ConnPool<128>>, PeerConn) {
    let peer = LoopPeer::start(LoopOpts::default());
    let mut cfg = net_cfg(peer.port);
    cfg.rx_buf.md = md_rx;
    let arena = ConnArena::new(&[(ConnRole::Md, 1)], &cfg, clock());
    let mut c = Connector::new(arena, &cfg).unwrap();
    let (h, pc) = open(&mut c, &peer, 1, ConnRole::Md);
    let mut p = pool();
    p.install(h, None).unwrap();
    (peer, c, p, pc)
}

#[test]
fn net_086_loopback_short_read() {
    let (_peer, _c, mut p, mut pc) = one(262_144);
    let conn = p.get(ConnId(1)).unwrap();
    let payload = vec![b'z'; 272];
    pc.text(&payload);
    std::thread::sleep(Duration::from_millis(1));
    wait_readable(&[conn.raw_fd()]);
    let r0 = conn.sys().recvmsg;
    let n = conn.poll_rx(0);
    assert!(n > 250 && n < 400, "one TLS record of about 300 B: {n}");
    let mut frames = Vec::new();
    while let Some(f) = conn.next_frame() {
        frames.push(f.payload.to_vec());
    }
    assert_eq!(conn.sys().recvmsg - r0, 1);
    assert_eq!(frames, vec![payload]);
}

#[test]
fn net_087_loopback_backlog_two_polls() {
    let (_peer, _c, mut p, mut pc) = one(65_536);
    let conn = p.get(ConnId(1)).unwrap();
    let frames: Vec<Vec<u8>> = (0..100u32)
        .map(|i| (0..1024u32).map(|k| (i * 31 + k) as u8).collect())
        .collect();
    for f in &frames {
        pc.s.write_all_frame(&ws_ref::text(f));
    }
    std::io::Write::flush(&mut pc.s).unwrap();
    std::thread::sleep(Duration::from_millis(5));
    // Slow hosts: wait until more than the free space is queued.
    let fd = conn.raw_fd();
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let mut q: libc::c_int = 0;
        unsafe { libc::ioctl(fd, libc::FIONREAD, &mut q) };
        if q as usize > 65_536 || Instant::now() > end {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut got = Vec::new();
    let mut first = true;
    let end = Instant::now() + Duration::from_secs(5);
    while got.len() < 100 && Instant::now() < end {
        let r0 = conn.sys().recvmsg;
        let n = conn.poll_rx(0);
        assert!(conn.sys().recvmsg - r0 <= 1);
        if first {
            assert_eq!(n, 65_536, "the first poll_rx fills the buffer");
            first = false;
        }
        while let Some(f) = conn.next_frame() {
            got.push(f.payload.to_vec());
        }
    }
    assert_eq!(got, frames);
}

trait WriteFrame {
    fn write_all_frame(&mut self, b: &[u8]);
}

impl WriteFrame for rustls::StreamOwned<rustls::ServerConnection, std::net::TcpStream> {
    fn write_all_frame(&mut self, b: &[u8]) {
        std::io::Write::write_all(self, b).unwrap();
    }
}

#[test]
fn net_135_gap_upper_bound() {
    let (_peer, _c, mut p, mut pc) = one(262_144);
    let clk = clock();
    let conn = p.get(ConnId(1)).unwrap();
    let fd = conn.raw_fd();
    let spin = |us: u64| {
        let t = Instant::now();
        while t.elapsed() < Duration::from_micros(us) {}
    };
    let read_frame = |conn: &mut Conn| -> Timestamps {
        wait_readable(&[fd]);
        assert!(conn.poll_rx(0) > 0);
        conn.next_frame().unwrap().ts
    };
    for group in ["A", "B", "C"] {
        for i in 0..200u64 {
            let d = [10u64, 100, 1000][(i % 3) as usize];
            match group {
                "A" => {
                    // Queue confirmed empty by EAGAIN.
                    while conn.poll_rx(0) > 0 {
                        while conn.next_frame().is_some() {}
                    }
                }
                "B" => {
                    pc.text(b"lead");
                    read_frame(conn);
                }
                _ => conn.note_empty(tsc_now().0),
            }
            spin(d);
            let w = tsc_now().0;
            pc.text(b"probe");
            let ts = read_frame(conn);
            let bound = clk.delta_ns(Tsc(w), Tsc(ts.rx_tsc));
            assert!(ts.rx_empty_gap_ns != 0, "group {group} #{i}: gap 0");
            assert!(
                ts.rx_empty_gap_ns as u64 >= bound,
                "group {group} #{i}: gap {} < {bound}",
                ts.rx_empty_gap_ns
            );
        }
    }
}

#[test]
fn net_163_tiny_sndbuf_all_delivered() {
    let peer = LoopPeer::start(LoopOpts::default());
    let mut cfg = net_cfg(peer.port);
    cfg.so_sndbuf = 4608;
    let arena = ConnArena::new(&[(ConnRole::Order, 1)], &cfg, clock());
    let mut c = Connector::new(arena, &cfg).unwrap();
    let (h, mut pc) = open(&mut c, &peer, 1, ConnRole::Order);
    let mut p = pool();
    p.install(h, None).unwrap();
    let reader = std::thread::spawn(move || {
        let mut acc = Vec::new();
        let mut out: Vec<Vec<u8>> = Vec::new();
        let end = Instant::now() + Duration::from_secs(60);
        while out.len() < 1000 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(1));
            for f in pc.read_frames(1, &mut acc) {
                out.push(f.payload);
            }
        }
        out
    });
    let conn = p.get(ConnId(1)).unwrap();
    let mut x = support::xorshift::XorShift64Star(77);
    let mut want = Vec::new();
    let mut f = Box::new(MaskedFrame::<2048>::new());
    for i in 0..1000u32 {
        let len = 300 + (x.next_u32() % 1701) as usize;
        let mut body = vec![(i % 251) as u8; len];
        body[..4].copy_from_slice(&i.to_be_bytes());
        f.unseal();
        f.body_mut()[..len].copy_from_slice(&body);
        f.seal(Opcode::Text, len, x.next_u32());
        loop {
            match conn.send_frame(&*f) {
                Ok(_) => break,
                Err(SendErr::TxBacklog) => {
                    conn.finish_poll(0);
                    std::thread::sleep(Duration::from_micros(100));
                }
                Err(e) => panic!("frame {i}: {e:?}"),
            }
        }
        want.push(body);
        conn.finish_poll(0);
    }
    let end = Instant::now() + Duration::from_secs(30);
    while conn.tx_pending() > 0 && Instant::now() < end {
        conn.finish_poll(0);
        std::thread::sleep(Duration::from_micros(200));
    }
    let got = reader.join().unwrap();
    assert_eq!(got.len(), 1000);
    assert_eq!(got, want);
    assert!(conn.stats().short_writes >= 1);
}
