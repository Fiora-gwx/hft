//! NET-010, NET-220–NET-227: `ConnSet` and `ReadyPoll`.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[allow(unused_imports)] // used by the Linux-only tests below
use crate::support;

use hft_net::{ConnId, ConnSet, ReadyPoll};

#[test]
fn net_010_conn_set_bits() {
    let rows = [
        ("NET-010.1", 0u8, [1u64, 0, 0, 0]),
        ("NET-010.2", 63, [1 << 63, 0, 0, 0]),
        ("NET-010.3", 64, [0, 1, 0, 0]),
        ("NET-010.4", 255, [0, 0, 0, 1 << 63]),
    ];
    for (row, id, bits) in rows {
        let mut s = ConnSet { bits: [0; 4] };
        s.insert(ConnId(id));
        assert_eq!(s.bits, bits, "{row}");
        assert!(s.contains(ConnId(id)), "{row}");
    }
}

#[test]
fn net_226_probe_error() {
    let mut rp = ReadyPoll::scripted_error(hft_net::sock::errno::EBADF);
    let mut out = ConnSet::default();
    out.insert(ConnId(5));
    assert_eq!(rp.probe(&mut out), 0);
    assert!(out.is_empty());
}

#[cfg(target_os = "linux")]
mod lo {
    use super::support::looppeer::*;
    use hft_net::*;

    struct Setup {
        _peer: LoopPeer,
        _c: Connector,
        p: Box<ConnPool<128>>,
        pcs: Vec<PeerConn>,
        rp: ReadyPoll,
    }

    fn setup(n: u8) -> Setup {
        let peer = LoopPeer::start(LoopOpts::default());
        let mut c = connector(peer.port, &[(ConnRole::Md, n as u16)]);
        let rp = ReadyPoll::new().unwrap();
        let hd = rp.handle();
        let mut p = pool();
        let mut pcs = Vec::new();
        for id in 1..=n {
            let (h, pc) = open(&mut c, &peer, id, ConnRole::Md);
            let id = p.install(h, None).unwrap();
            hd.add(p.get(id).unwrap()).unwrap();
            pcs.push(pc);
        }
        Setup {
            _peer: peer,
            _c: c,
            p,
            pcs,
            rp,
        }
    }

    fn write_to(s: &mut Setup, ids: &[u8]) {
        for id in ids {
            s.pcs[*id as usize - 1].text(b"x");
        }
        let fds: Vec<i32> = ids
            .iter()
            .map(|i| s.p.get(ConnId(*i)).unwrap().raw_fd())
            .collect();
        wait_readable(&fds);
    }

    fn set(ids: &[u8]) -> ConnSet {
        let mut s = ConnSet::default();
        for i in ids {
            s.insert(ConnId(*i));
        }
        s
    }

    #[test]
    fn net_220_probe_ready_set() {
        let mut s = setup(12);
        write_to(&mut s, &[3, 9]);
        let mut out = ConnSet::default();
        let ctl = s.rp.sys().epoll_ctl;
        assert_eq!(s.rp.probe(&mut out), 2);
        assert_eq!(out, set(&[3, 9]));
        assert_eq!(s.rp.sys().epoll_pwait, 1);
        assert_eq!(s.rp.sys().last_epoll_timeout, 0);
        assert_eq!(s.rp.sys().epoll_ctl, ctl);
    }

    #[test]
    fn net_221_del_removes() {
        let mut s = setup(12);
        write_to(&mut s, &[3, 9]);
        let hd = s.rp.handle();
        hd.del(s.p.get(ConnId(3)).unwrap()).unwrap();
        let mut out = ConnSet::default();
        assert_eq!(s.rp.probe(&mut out), 1);
        assert_eq!(out, set(&[9]));
    }

    fn drain(s: &mut Setup, id: u8) {
        let conn = s.p.get(ConnId(id)).unwrap();
        for _ in 0..100 {
            let n = conn.poll_rx(0);
            while conn.next_frame().is_some() {}
            if n == 0 {
                break;
            }
        }
    }

    #[test]
    fn net_222_level_triggered() {
        let mut s = setup(12);
        write_to(&mut s, &[3, 9]);
        let mut out = ConnSet::default();
        s.rp.probe(&mut out);
        drain(&mut s, 3);
        assert_eq!(s.rp.probe(&mut out), 1);
        assert_eq!(out, set(&[9]));
        drain(&mut s, 9);
        assert_eq!(s.rp.probe(&mut out), 0);
    }

    #[test]
    fn net_223_probe_clears_out() {
        let mut s = setup(12);
        write_to(&mut s, &[3, 9]);
        let mut out = set(&[1, 200]);
        s.rp.probe(&mut out);
        assert_eq!(out, set(&[3, 9]));
    }

    #[test]
    fn net_224_more_than_64_ready() {
        let mut s = setup(70);
        let all: Vec<u8> = (1..=70).collect();
        write_to(&mut s, &all);
        let mut a = ConnSet::default();
        let mut b = ConnSet::default();
        assert_eq!(s.rp.probe(&mut a), 64);
        s.rp.probe(&mut b);
        let mut u = a;
        for (x, y) in u.bits.iter_mut().zip(b.bits) {
            *x |= y;
        }
        assert_eq!(u, set(&all));
    }

    #[test]
    fn net_225_ctl_errors() {
        let mut s = setup(1);
        let rp = ReadyPoll::new().unwrap();
        let hd = rp.handle();
        let c = s.p.get(ConnId(1)).unwrap();
        assert_eq!(hd.add(c), Ok(()), "NET-225.1");
        assert_eq!(hd.add(c), Err(NetErr::Os(17)), "NET-225.1");
        let rp2 = ReadyPoll::new().unwrap();
        assert_eq!(rp2.handle().del(c), Err(NetErr::Os(2)), "NET-225.2");
        let rp3 = ReadyPoll::new().unwrap();
        assert_eq!(rp3.handle().add(c), Ok(()), "NET-225.3");
        assert_eq!(rp3.handle().del(c), Ok(()), "NET-225.3");
        let _ = &mut s.rp;
    }

    #[test]
    fn net_227_epfd_cloexec() {
        let rp = ReadyPoll::new().unwrap();
        let fl = hft_net::sock::fd_flags(rp.epfd()).unwrap();
        assert_ne!(fl & libc::FD_CLOEXEC, 0);
    }
}
