//! NET-328: Linux TCP syscall experiment, outside the engine. Immutable TLS-sized wire bytes.
//! Loopback includes local receive processing; it is NOT remote wire latency or a host gate.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros
)]

#[cfg(target_os = "linux")]
mod linux {
    // Linux UAPI linux/errqueue.h; libc exposes the payload layout but not these tags.
    const ORIGIN_ZEROCOPY: u8 = 5;
    const ZEROCOPY_COPIED: u8 = 1;
    use std::{
        io::Read,
        net::{TcpListener, TcpStream},
        os::fd::AsRawFd,
        time::{Duration, Instant},
    };

    #[derive(Clone, Copy, Debug)]
    enum Method {
        Send,
        Sendmsg,
        TwoIov,
        Writev,
        Zerocopy,
        Uncork,
    }

    fn option(fd: i32, level: i32, name: i32, value: i32) {
        // SAFETY: value is readable for the supplied length; fd belongs to the live TCP socket.
        assert_eq!(
            unsafe { libc::setsockopt(fd, level, name, (&value as *const i32).cast(), 4) },
            0,
            "setsockopt: {}",
            std::io::Error::last_os_error()
        );
    }

    fn completed(fd: i32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let mut control = [0usize; 64];
            // SAFETY: zero is a valid empty msghdr. Only control bytes are requested.
            let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
            msg.msg_control = control.as_mut_ptr().cast();
            msg.msg_controllen = std::mem::size_of_val(&control);
            // SAFETY: control is aligned, writable and lives through the call.
            let n = unsafe { libc::recvmsg(fd, &mut msg, libc::MSG_ERRQUEUE | libc::MSG_DONTWAIT) };
            if n < 0 {
                assert!(Instant::now() < deadline, "zero-copy completion timeout");
                assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::EAGAIN));
                std::hint::spin_loop();
                continue;
            }
            // SAFETY: the kernel returned msg_controllen bytes in this aligned control buffer;
            // CMSG traversal checks boundaries and the error payload length is checked below.
            unsafe {
                let mut c = libc::CMSG_FIRSTHDR(&msg);
                while !c.is_null() {
                    if (*c).cmsg_level == libc::SOL_IP
                        && (*c).cmsg_type == libc::IP_RECVERR
                        && (*c).cmsg_len
                            >= libc::CMSG_LEN(std::mem::size_of::<libc::sock_extended_err>() as u32) as usize
                    {
                        let e =
                            std::ptr::read_unaligned(libc::CMSG_DATA(c).cast::<libc::sock_extended_err>());
                        if e.ee_origin == ORIGIN_ZEROCOPY {
                            assert_eq!(e.ee_errno, 0);
                            // One outstanding send at a time; completion may signal a copied fallback.
                            return e.ee_code == ZEROCOPY_COPIED;
                        }
                    }
                    c = libc::CMSG_NXTHDR(&msg, c);
                }
            }
        }
    }

    fn run(method: Method, bytes: usize, sndbuf: i32, idle: bool) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let sender = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut receiver, _) = listener.accept().unwrap();
        sender.set_nodelay(true).unwrap();
        receiver.set_nodelay(true).unwrap();
        sender.set_nonblocking(true).unwrap();
        receiver.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let fd = sender.as_raw_fd();
        option(fd, libc::SOL_SOCKET, libc::SO_SNDBUF, sndbuf);
        option(fd, libc::IPPROTO_TCP, libc::TCP_CORK, 0);
        option(fd, libc::SOL_SOCKET, libc::SO_BUSY_POLL, 0);
        if matches!(method, Method::Zerocopy) {
            option(fd, libc::SOL_SOCKET, libc::SO_ZEROCOPY, 1);
        }
        // Never mutate or release this storage until each zero-copy completion has arrived.
        let wire = vec![b'x'; bytes];
        let split = if matches!(method, Method::TwoIov) {
            5
        } else {
            bytes
        };
        let iov = [
            libc::iovec {
                iov_base: wire.as_ptr() as *mut _,
                iov_len: split,
            },
            libc::iovec {
                iov_base: wire[split..].as_ptr() as *mut _,
                iov_len: bytes - split,
            },
        ];
        // SAFETY: all-zero is an empty msghdr; its sole iovec is initialized below.
        let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
        msg.msg_iov = iov.as_ptr() as *mut _;
        msg.msg_iovlen = if matches!(method, Method::TwoIov) { 2 } else { 1 };
        let mut read = vec![0; bytes];
        let count = if idle { 1_000 } else { 10_000 };
        let mut samples = Vec::with_capacity(count);
        let mut copied = 0;
        for i in 0..count + 500 {
            if idle {
                std::thread::sleep(Duration::from_millis(1));
            }
            if matches!(method, Method::Uncork) {
                option(fd, libc::IPPROTO_TCP, libc::TCP_CORK, 1);
            }
            let flags = libc::MSG_DONTWAIT
                | libc::MSG_NOSIGNAL
                | if matches!(method, Method::Zerocopy) {
                    libc::MSG_ZEROCOPY
                } else {
                    0
                };
            let start = Instant::now();
            // SAFETY: wire and its iovecs are immutable and live through the syscall and, for
            // MSG_ZEROCOPY, through the completion collected below. The peer remains open.
            let n = unsafe {
                match method {
                    Method::Send => libc::send(fd, wire.as_ptr().cast(), bytes, flags),
                    Method::Writev => libc::writev(fd, iov.as_ptr(), 1),
                    _ => libc::sendmsg(fd, &msg, flags),
                }
            };
            if matches!(method, Method::Uncork) {
                option(fd, libc::IPPROTO_TCP, libc::TCP_CORK, 0);
            }
            let ns = start.elapsed().as_nanos() as u64;
            assert_eq!(n, bytes as isize, "send: {}", std::io::Error::last_os_error());
            receiver.read_exact(&mut read).unwrap();
            if matches!(method, Method::Zerocopy) {
                copied += usize::from(completed(fd));
            }
            assert_eq!(read, wire);
            if i >= 500 {
                samples.push(ns);
            }
        }
        samples.sort_unstable();
        let q = |n: usize| samples[(samples.len() * n).div_ceil(1000) - 1];
        println!(
            "{}",
            serde_json::json!({"method":format!("{method:?}"),"bytes":bytes,
            "sndbuf_requested":sndbuf,"idle_1ms":idle,"n":samples.len(),"p50_ns":q(500),
            "p99_ns":q(990),"p999_ns":q(999),"max_ns":samples.last(),"zerocopy_copied_completions":copied,
            "scope":"loopback submission; drain/completion untimed; uncork includes setsockopt"})
        );
    }

    pub fn main() {
        for bytes in [322, 2048, 16384] {
            for method in [
                Method::Send,
                Method::Sendmsg,
                Method::TwoIov,
                Method::Writev,
                Method::Zerocopy,
                Method::Uncork,
            ] {
                run(method, bytes, 65536, false);
            }
        }
        for size in [16384, 262144, 1048576] {
            run(Method::Sendmsg, 322, size, false);
        }
        run(Method::Sendmsg, 322, 65536, true);
        run(Method::Send, 322, 65536, true);
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    linux::main();
    #[cfg(not(target_os = "linux"))]
    println!("send_profile requires Linux; no substitute timing on this platform");
}
