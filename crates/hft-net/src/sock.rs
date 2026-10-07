//! Socket system calls: creation, options, `bind`, non-blocking `connect`, `recvmsg`/`sendmsg`,
//! `shutdown`/`close`. Every call is counted in the owner's `SysCount`; with feature `test-io` a
//! socket can be replaced by a `ScriptIo` that gives each call's result, bytes, stamps and TSCs.
//! All `unsafe` of the socket layer is here.

use std::net::Ipv4Addr;

use crate::cmsg::{self, CMSG_BUF, RxStamp};

/// Linux errno values used by the crate (the script backend and every host speak these).
pub mod errno {
    pub const EINTR: i32 = 4;
    pub const EAGAIN: i32 = 11;
    pub const ENOMEM: i32 = 12;
    pub const EBADF: i32 = 9;
    pub const EEXIST: i32 = 17;
    pub const ENOENT: i32 = 2;
    pub const EPIPE: i32 = 32;
    pub const ENOSYS: i32 = 38;
    pub const ENOBUFS: i32 = 105;
    pub const ECONNRESET: i32 = 104;
    pub const ETIMEDOUT: i32 = 110;
    pub const ECONNREFUSED: i32 = 111;
    pub const EINPROGRESS: i32 = 115;
}

/// Socket option names (Linux, generic ABI: x86_64 and aarch64).
pub mod opt {
    pub const SOL_SOCKET: i32 = 1;
    pub const IPPROTO_TCP: i32 = 6;
    pub const SO_REUSEADDR: i32 = 2;
    pub const SO_SNDBUF: i32 = 7;
    pub const SO_RCVBUF: i32 = 8;
    pub const SO_BINDTODEVICE: i32 = 25;
    pub const SO_TIMESTAMPING: i32 = 37;
    pub const SO_BUSY_POLL: i32 = 46;
    pub const SO_INCOMING_NAPI_ID: i32 = 56;
    pub const SO_PREFER_BUSY_POLL: i32 = 69;
    pub const SO_BUSY_POLL_BUDGET: i32 = 70;
    pub const TCP_NODELAY: i32 = 1;
    pub const TCP_MAXSEG: i32 = 2;
    pub const TCP_QUICKACK: i32 = 12;
    pub const TCP_USER_TIMEOUT: i32 = 18;
    /// `SOF_TIMESTAMPING_*` receive flags set by `Connector` (README §5).
    pub const TS_RX_HARDWARE: i32 = 1 << 2;
    pub const TS_RX_SOFTWARE: i32 = 1 << 3;
    pub const TS_SOFTWARE: i32 = 1 << 4;
    pub const TS_RAW_HARDWARE: i32 = 1 << 6;
    pub const TS_TX_SOFTWARE: i32 = 1 << 1;
    pub const TS_TX_SCHED: i32 = 1 << 8;
    pub const TS_OPT_ID: i32 = 1 << 7;
    pub const TS_OPT_TSONLY: i32 = 1 << 11;
    pub const TS_RX_FLAGS: i32 = TS_RX_HARDWARE | TS_RX_SOFTWARE | TS_RAW_HARDWARE | TS_SOFTWARE;
}

/// Fixed-size ring of the last `N` entries plus the total pushed (never allocates).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Ring<T: Copy + Default, const N: usize> {
    items: [T; N],
    total: u64,
}

impl<T: Copy + Default, const N: usize> Default for Ring<T, N> {
    fn default() -> Self {
        Ring {
            items: [T::default(); N],
            total: 0,
        }
    }
}

impl<T: Copy + Default, const N: usize> Ring<T, N> {
    #[inline]
    pub fn push(&mut self, v: T) {
        let i = (self.total % N as u64) as usize;
        if let Some(s) = self.items.get_mut(i) {
            *s = v;
        }
        self.total = self.total.wrapping_add(1);
    }

    /// Entries pushed so far.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// The retained entries, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = T> + '_ {
        let n = self.total.min(N as u64);
        let first = self.total - n;
        (first..self.total).filter_map(move |k| self.items.get((k % N as u64) as usize).copied())
    }

    /// The newest entry.
    pub fn last(&self) -> Option<T> {
        if self.total == 0 {
            None
        } else {
            self.items.get(((self.total - 1) % N as u64) as usize).copied()
        }
    }
}

/// One `setsockopt`/`getsockopt` call: `(level, option, value, phase code at the call)`.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct OptCall {
    pub level: i32,
    pub opt: i32,
    pub val: i32,
    pub phase: u8,
}

/// One `sendmsg`: iovec count and the first two lengths.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct SendCall {
    pub iovs: u8,
    pub lens: [u32; 2],
}

/// System-call counts of one owner (a `Conn`, the `Connector`, a `ReadyPoll`), plus small logs.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct SysCount {
    pub recvmsg: u64,
    pub sendmsg: u64,
    pub setsockopt: u64,
    pub getsockopt: u64,
    pub connect: u64,
    pub shutdown: u64,
    pub close: u64,
    pub getaddrinfo: u64,
    pub epoll_ctl: u64,
    pub epoll_pwait: u64,
    pub socket: u64,
    pub bind: u64,
    pub poll: u64,
    pub dup: u64,
    pub fcntl: u64,
    pub getsockname: u64,
    /// `timeout` argument of the last `epoll_pwait`.
    pub last_epoll_timeout: i32,
    pub sendmsg_log: Ring<SendCall, 16>,
    pub setsockopt_log: Ring<OptCall, 32>,
    pub getsockopt_log: Ring<OptCall, 16>,
    /// Arguments of the last `bind` and `connect`.
    pub bind_addr: Option<(Ipv4Addr, u16)>,
    pub connect_addr: Option<(Ipv4Addr, u16)>,
}

impl SysCount {
    /// Every counted call.
    pub fn total(&self) -> u64 {
        self.recvmsg
            + self.sendmsg
            + self.setsockopt
            + self.getsockopt
            + self.connect
            + self.shutdown
            + self.close
            + self.getaddrinfo
            + self.epoll_ctl
            + self.epoll_pwait
            + self.socket
            + self.bind
            + self.poll
            + self.dup
            + self.fcntl
            + self.getsockname
    }

    /// Retained `setsockopt` calls on `(level, opt)`: their values, oldest first.
    pub fn setsockopt_values(&self, level: i32, opt: i32) -> impl Iterator<Item = i32> + '_ {
        self.setsockopt_log
            .iter()
            .filter(move |c| c.level == level && c.opt == opt)
            .map(|c| c.val)
    }

    /// Retained `getsockopt` calls on `(level, opt)`: the phase code at each call.
    pub fn getsockopt_phases(&self, level: i32, opt: i32) -> impl Iterator<Item = u8> + '_ {
        self.getsockopt_log
            .iter()
            .filter(move |c| c.level == level && c.opt == opt)
            .map(|c| c.phase)
    }
}

/// Result of one `recvmsg`.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct RecvOut {
    /// TSC read right before the call.
    pub pre: u64,
    /// TSC read right after the call returned.
    pub post: u64,
    /// Bytes (> 0), 0 = EOF, < 0 = −errno.
    pub ret: isize,
    pub stamp: RxStamp,
}

/// Result of one `sendmsg`.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct SendOut {
    pub post: u64,
    /// Bytes written, < 0 = −errno.
    pub ret: isize,
}

#[cfg(feature = "test-io")]
pub use script::*;

#[cfg(feature = "test-io")]
mod script {
    use std::collections::VecDeque;
    use std::net::Ipv4Addr;

    /// What one scripted `recvmsg` returns.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum RecvKind {
        /// Bytes with the stamps of this read (0 = absent). Longer than the free space → the rest
        /// stays for the next call (same stamps and TSCs).
        Data {
            bytes: Vec<u8>,
            hw_ns: i64,
            sw_ns: i64,
        },
        Eagain,
        Eof,
        Errno(i32),
    }

    /// One scripted `recvmsg`: the TSC before and after the call and the result.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct RecvStep {
        pub pre_tsc: u64,
        pub post_tsc: u64,
        pub kind: RecvKind,
    }

    impl RecvStep {
        pub fn data(pre_tsc: u64, post_tsc: u64, bytes: Vec<u8>) -> RecvStep {
            RecvStep {
                pre_tsc,
                post_tsc,
                kind: RecvKind::Data {
                    bytes,
                    hw_ns: 0,
                    sw_ns: 0,
                },
            }
        }

        pub fn stamped(pre_tsc: u64, post_tsc: u64, bytes: Vec<u8>, hw_ns: i64, sw_ns: i64) -> RecvStep {
            RecvStep {
                pre_tsc,
                post_tsc,
                kind: RecvKind::Data { bytes, hw_ns, sw_ns },
            }
        }

        pub fn eagain(pre_tsc: u64) -> RecvStep {
            RecvStep {
                pre_tsc,
                post_tsc: pre_tsc,
                kind: RecvKind::Eagain,
            }
        }
    }

    /// What one scripted `sendmsg` does.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum SendKind {
        /// Accept everything.
        All,
        /// Accept the first `n` bytes.
        Bytes(usize),
        Eagain,
        Errno(i32),
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct SendStep {
        pub post_tsc: u64,
        pub kind: SendKind,
    }

    /// The scripted socket (feature `test-io`). Queues are consumed front first; an empty receive
    /// queue answers `EAGAIN` and an empty send queue accepts everything, both at `idle_tsc`.
    #[derive(Clone, Debug)]
    pub struct ScriptIo {
        pub recv: VecDeque<RecvStep>,
        pub send: VecDeque<SendStep>,
        /// Bytes accepted by `sendmsg`, in order (the test drains it).
        pub sent: Vec<u8>,
        /// `connect` result: 0 = connected at once, `EINPROGRESS` = in progress, else the errno.
        pub connect: i32,
        /// Writability polls answered "not yet" before "writable"; `u32::MAX` = never.
        pub writable_after: u32,
        /// `SO_ERROR` once writable.
        pub so_error: i32,
        pub napi_id: i32,
        pub mss: i32,
        /// The next `n` record encryptions fail (`SendErr::Encrypt`).
        pub fail_encrypt: u32,
        pub idle_tsc: u64,
        /// Local address reported by `getsockname`.
        pub local: (Ipv4Addr, u16),
        #[doc(hidden)]
        pub recv_off: usize,
    }

    impl Default for ScriptIo {
        fn default() -> Self {
            ScriptIo {
                recv: VecDeque::new(),
                send: VecDeque::new(),
                sent: Vec::new(),
                connect: 0,
                writable_after: 0,
                so_error: 0,
                napi_id: 0,
                mss: 1448,
                fail_encrypt: 0,
                idle_tsc: 0,
                local: (Ipv4Addr::new(127, 0, 0, 1), 40_000),
                recv_off: 0,
            }
        }
    }

    impl ScriptIo {
        pub fn new() -> ScriptIo {
            ScriptIo::default()
        }

        pub fn push_recv(&mut self, s: RecvStep) {
            self.recv.push_back(s);
        }

        pub fn push_send(&mut self, s: SendStep) {
            self.send.push_back(s);
        }

        /// Takes the bytes sent so far.
        pub fn take_sent(&mut self) -> Vec<u8> {
            std::mem::take(&mut self.sent)
        }
    }
}

/// One socket: fd (or script) and its call counts.
#[derive(Debug)]
pub(crate) struct Sock {
    pub fd: i32,
    pub sys: SysCount,
    /// Phase code logged with option calls.
    pub phase: u8,
    #[cfg(feature = "test-io")]
    pub script: Option<Box<ScriptIo>>,
}

impl Default for Sock {
    fn default() -> Self {
        Sock {
            fd: -1,
            sys: SysCount::default(),
            phase: 0,
            #[cfg(feature = "test-io")]
            script: None,
        }
    }
}

#[inline]
fn tsc() -> u64 {
    hft_rt::tsc_now().0
}

#[cfg(target_os = "linux")]
#[inline]
fn last_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(errno::ENOSYS)
}

impl Sock {
    /// Whether the socket has an fd or a script.
    pub fn is_open(&self) -> bool {
        #[cfg(feature = "test-io")]
        if self.script.is_some() {
            return true;
        }
        self.fd >= 0
    }

    /// `recvmsg(fd, [buf], cmsg, MSG_DONTWAIT)` with the TSC read right before and right after.
    #[inline]
    pub fn recv(&mut self, buf: &mut [u8]) -> RecvOut {
        self.sys.recvmsg += 1;
        #[cfg(feature = "test-io")]
        if let Some(s) = self.script.as_deref_mut() {
            return script_recv(s, buf);
        }
        let mut ctl = [0u64; CMSG_BUF / 8];
        let pre = tsc();
        let (ret, clen, flags) = sys_recvmsg(self.fd, buf, &mut ctl);
        let post = tsc();
        let stamp = if ret > 0 {
            let bytes = ctl_bytes(&ctl, clen);
            cmsg::parse_timestamping(bytes, flags)
        } else {
            RxStamp::default()
        };
        RecvOut {
            pre,
            post,
            ret,
            stamp,
        }
    }

    /// `sendmsg(fd, iov, MSG_DONTWAIT | MSG_NOSIGNAL)`; the TSC right after it returned.
    #[inline]
    pub fn send(&mut self, iov: &[&[u8]]) -> SendOut {
        self.sys.sendmsg += 1;
        let mut lens = [0u32; 2];
        for (l, b) in lens.iter_mut().zip(iov.iter()) {
            *l = b.len() as u32;
        }
        self.sys.sendmsg_log.push(SendCall {
            iovs: iov.len() as u8,
            lens,
        });
        #[cfg(feature = "test-io")]
        if let Some(s) = self.script.as_deref_mut() {
            return script_send(s, iov);
        }
        let ret = sys_sendmsg(self.fd, iov);
        SendOut { post: tsc(), ret }
    }

    pub fn setsockopt_i32(&mut self, level: i32, name: i32, val: i32) -> Result<(), i32> {
        self.sys.setsockopt += 1;
        self.sys.setsockopt_log.push(OptCall {
            level,
            opt: name,
            val,
            phase: self.phase,
        });
        #[cfg(feature = "test-io")]
        if self.script.is_some() {
            return Ok(());
        }
        sys_setsockopt(self.fd, level, name, &val.to_ne_bytes())
    }

    pub fn setsockopt_bytes(&mut self, level: i32, name: i32, val: &[u8]) -> Result<(), i32> {
        self.sys.setsockopt += 1;
        self.sys.setsockopt_log.push(OptCall {
            level,
            opt: name,
            val: val.len() as i32,
            phase: self.phase,
        });
        #[cfg(feature = "test-io")]
        if self.script.is_some() {
            return Ok(());
        }
        sys_setsockopt(self.fd, level, name, val)
    }

    pub fn getsockopt_i32(&mut self, level: i32, name: i32) -> Result<i32, i32> {
        self.sys.getsockopt += 1;
        self.sys.getsockopt_log.push(OptCall {
            level,
            opt: name,
            val: 0,
            phase: self.phase,
        });
        #[cfg(feature = "test-io")]
        if let Some(s) = self.script.as_deref() {
            return Ok(match (level, name) {
                (opt::SOL_SOCKET, opt::SO_INCOMING_NAPI_ID) => s.napi_id,
                (opt::IPPROTO_TCP, opt::TCP_MAXSEG) => s.mss,
                (opt::SOL_SOCKET, 4) => s.so_error,
                _ => 0,
            });
        }
        sys_getsockopt_i32(self.fd, level, name)
    }

    /// `socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0)`.
    pub fn open_tcp(&mut self) -> Result<(), i32> {
        self.sys.socket += 1;
        #[cfg(feature = "test-io")]
        if self.script.is_some() {
            return Ok(());
        }
        self.fd = sys_socket()?;
        Ok(())
    }

    pub fn bind(&mut self, ip: Ipv4Addr, port: u16) -> Result<(), i32> {
        self.sys.bind += 1;
        self.sys.bind_addr = Some((ip, port));
        #[cfg(feature = "test-io")]
        if self.script.is_some() {
            return Ok(());
        }
        sys_bind(self.fd, ip, port)
    }

    /// Non-blocking connect: `Ok(true)` connected, `Ok(false)` in progress, `Err(errno)`.
    pub fn connect(&mut self, ip: Ipv4Addr, port: u16) -> Result<bool, i32> {
        self.sys.connect += 1;
        self.sys.connect_addr = Some((ip, port));
        #[cfg(feature = "test-io")]
        if let Some(s) = self.script.as_deref() {
            return match s.connect {
                0 => Ok(true),
                errno::EINPROGRESS => Ok(false),
                e => Err(e),
            };
        }
        match sys_connect(self.fd, ip, port) {
            Ok(()) => Ok(true),
            Err(errno::EINPROGRESS) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// `poll(fd, POLLOUT, 0)`: whether the in-progress connect finished.
    pub fn writable(&mut self) -> Result<bool, i32> {
        self.sys.poll += 1;
        #[cfg(feature = "test-io")]
        if let Some(s) = self.script.as_deref_mut() {
            return Ok(match s.writable_after {
                0 => true,
                u32::MAX => false,
                n => {
                    s.writable_after = n - 1;
                    false
                }
            });
        }
        sys_poll_out(self.fd)
    }

    pub fn getsockname(&mut self) -> Result<(Ipv4Addr, u16), i32> {
        self.sys.getsockname += 1;
        #[cfg(feature = "test-io")]
        if let Some(s) = self.script.as_deref() {
            return Ok(s.local);
        }
        sys_getsockname(self.fd)
    }

    /// `shutdown(SHUT_RDWR)` + `close`; the socket is closed afterwards.
    pub fn shutdown_close(&mut self) {
        if !self.is_open() {
            return;
        }
        self.sys.shutdown += 1;
        self.sys.close += 1;
        #[cfg(feature = "test-io")]
        if self.script.take().is_some() {
            self.fd = -1;
            return;
        }
        sys_shutdown_close(self.fd);
        self.fd = -1;
    }

    /// Takes one injected encryption failure (always `false` without `test-io`).
    #[inline]
    pub fn take_encrypt_failure(&mut self) -> bool {
        #[cfg(feature = "test-io")]
        if let Some(s) = self.script.as_deref_mut()
            && s.fail_encrypt > 0
        {
            s.fail_encrypt -= 1;
            return true;
        }
        false
    }
}

#[cfg(feature = "test-io")]
fn script_recv(s: &mut ScriptIo, buf: &mut [u8]) -> RecvOut {
    let Some(step) = s.recv.front() else {
        return RecvOut {
            pre: s.idle_tsc,
            post: s.idle_tsc,
            ret: -(errno::EAGAIN as isize),
            stamp: RxStamp::default(),
        };
    };
    let (pre, post) = (step.pre_tsc, step.post_tsc);
    match &step.kind {
        RecvKind::Data { bytes, hw_ns, sw_ns } => {
            let rest = bytes.get(s.recv_off..).unwrap_or(&[]);
            let n = rest.len().min(buf.len());
            if let (Some(d), Some(src)) = (buf.get_mut(..n), rest.get(..n)) {
                d.copy_from_slice(src);
            }
            // Through the real cmsg parser.
            let mut ctl = [0u8; 64];
            let hw = (*hw_ns / 1_000_000_000, *hw_ns % 1_000_000_000);
            let sw = (*sw_ns / 1_000_000_000, *sw_ns % 1_000_000_000);
            let clen = cmsg::build_timestamping(&mut ctl, [sw, (0, 0), hw]);
            let stamp = cmsg::parse_timestamping(ctl.get(..clen).unwrap_or(&[]), 0);
            if n < rest.len() {
                s.recv_off += n;
            } else {
                s.recv_off = 0;
                s.recv.pop_front();
            }
            RecvOut {
                pre,
                post,
                ret: n as isize,
                stamp,
            }
        }
        RecvKind::Eagain => {
            s.recv.pop_front();
            RecvOut {
                pre,
                post,
                ret: -(errno::EAGAIN as isize),
                stamp: RxStamp::default(),
            }
        }
        RecvKind::Eof => {
            s.recv.pop_front();
            RecvOut {
                pre,
                post,
                ret: 0,
                stamp: RxStamp::default(),
            }
        }
        RecvKind::Errno(e) => {
            let e = *e;
            s.recv.pop_front();
            RecvOut {
                pre,
                post,
                ret: -(e as isize),
                stamp: RxStamp::default(),
            }
        }
    }
}

#[cfg(feature = "test-io")]
fn script_send(s: &mut ScriptIo, iov: &[&[u8]]) -> SendOut {
    let total: usize = iov.iter().map(|b| b.len()).sum();
    let step = s.send.pop_front().unwrap_or(SendStep {
        post_tsc: s.idle_tsc,
        kind: SendKind::All,
    });
    let n = match step.kind {
        SendKind::All => total,
        SendKind::Bytes(n) => n.min(total),
        SendKind::Eagain => {
            return SendOut {
                post: step.post_tsc,
                ret: -(errno::EAGAIN as isize),
            };
        }
        SendKind::Errno(e) => {
            return SendOut {
                post: step.post_tsc,
                ret: -(e as isize),
            };
        }
    };
    let mut left = n;
    for b in iov {
        let k = left.min(b.len());
        s.sent.extend_from_slice(b.get(..k).unwrap_or(&[]));
        left -= k;
    }
    SendOut {
        post: step.post_tsc,
        ret: n as isize,
    }
}

#[inline]
#[allow(dead_code)]
fn ctl_bytes(ctl: &[u64; CMSG_BUF / 8], len: usize) -> &[u8] {
    // SAFETY: `ctl` is a plain u64 array; viewing its bytes as u8 is always valid, and the
    // length is clamped to the array size.
    let all = unsafe { std::slice::from_raw_parts(ctl.as_ptr().cast::<u8>(), CMSG_BUF) };
    all.get(..len.min(CMSG_BUF)).unwrap_or(&[])
}

// ---------------------------------------------------------------- Linux system calls

#[cfg(target_os = "linux")]
fn sockaddr(ip: Ipv4Addr, port: u16) -> libc::sockaddr_in {
    libc::sockaddr_in {
        sin_family: libc::AF_INET as libc::sa_family_t,
        sin_port: port.to_be(),
        sin_addr: libc::in_addr {
            s_addr: u32::from_ne_bytes(ip.octets()),
        },
        sin_zero: [0; 8],
    }
}

#[cfg(target_os = "linux")]
fn sys_recvmsg(fd: i32, buf: &mut [u8], ctl: &mut [u64; CMSG_BUF / 8]) -> (isize, usize, i32) {
    let mut iov = libc::iovec {
        iov_base: buf.as_mut_ptr().cast(),
        iov_len: buf.len(),
    };
    // SAFETY: msghdr is plain data; all-zero is a valid empty header.
    let mut mh: libc::msghdr = unsafe { std::mem::zeroed() };
    mh.msg_iov = &mut iov;
    mh.msg_iovlen = 1;
    mh.msg_control = ctl.as_mut_ptr().cast();
    mh.msg_controllen = CMSG_BUF as _;
    // SAFETY: the iovec points at `buf` (valid for writes of its length) and the control buffer
    // at `ctl` (CMSG_BUF bytes); both outlive the call.
    let r = unsafe { libc::recvmsg(fd, &mut mh, libc::MSG_DONTWAIT) };
    if r < 0 {
        (-(last_errno() as isize), 0, 0)
    } else {
        (r, mh.msg_controllen as usize, mh.msg_flags)
    }
}

#[cfg(target_os = "linux")]
fn sys_sendmsg(fd: i32, bufs: &[&[u8]]) -> isize {
    let mut iov = [libc::iovec {
        iov_base: std::ptr::null_mut(),
        iov_len: 0,
    }; 2];
    let n = bufs.len().min(2);
    for (v, b) in iov.iter_mut().zip(bufs.iter()) {
        v.iov_base = b.as_ptr() as *mut libc::c_void;
        v.iov_len = b.len();
    }
    // SAFETY: msghdr is plain data; all-zero is a valid empty header.
    let mut mh: libc::msghdr = unsafe { std::mem::zeroed() };
    mh.msg_iov = iov.as_mut_ptr();
    mh.msg_iovlen = n as _;
    // SAFETY: the iovecs point at the caller's slices (read-only for sendmsg), valid for the call.
    let r = unsafe { libc::sendmsg(fd, &mh, libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL) };
    if r < 0 { -(last_errno() as isize) } else { r }
}

#[cfg(target_os = "linux")]
fn sys_setsockopt(fd: i32, level: i32, name: i32, val: &[u8]) -> Result<(), i32> {
    // SAFETY: `val` is valid for reads of its length during the call.
    let r = unsafe { libc::setsockopt(fd, level, name, val.as_ptr().cast(), val.len() as libc::socklen_t) };
    if r == 0 { Ok(()) } else { Err(last_errno()) }
}

#[cfg(target_os = "linux")]
fn sys_getsockopt_i32(fd: i32, level: i32, name: i32) -> Result<i32, i32> {
    let mut v: i32 = 0;
    let mut l = size_of::<i32>() as libc::socklen_t;
    // SAFETY: `v`/`l` are valid for writes; the kernel writes at most `l` bytes.
    let r = unsafe { libc::getsockopt(fd, level, name, (&mut v as *mut i32).cast(), &mut l) };
    if r == 0 { Ok(v) } else { Err(last_errno()) }
}

#[cfg(target_os = "linux")]
fn sys_socket() -> Result<i32, i32> {
    // SAFETY: plain system call without pointers.
    let fd = unsafe {
        libc::socket(
            libc::AF_INET,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if fd < 0 { Err(last_errno()) } else { Ok(fd) }
}

#[cfg(target_os = "linux")]
fn sys_bind(fd: i32, ip: Ipv4Addr, port: u16) -> Result<(), i32> {
    let sa = sockaddr(ip, port);
    // SAFETY: `sa` is a valid sockaddr_in for the call's duration.
    let r = unsafe {
        libc::bind(
            fd,
            (&sa as *const libc::sockaddr_in).cast(),
            size_of::<libc::sockaddr_in>() as libc::socklen_t,
        )
    };
    if r == 0 { Ok(()) } else { Err(last_errno()) }
}

#[cfg(target_os = "linux")]
fn sys_connect(fd: i32, ip: Ipv4Addr, port: u16) -> Result<(), i32> {
    let sa = sockaddr(ip, port);
    // SAFETY: `sa` is a valid sockaddr_in for the call's duration.
    let r = unsafe {
        libc::connect(
            fd,
            (&sa as *const libc::sockaddr_in).cast(),
            size_of::<libc::sockaddr_in>() as libc::socklen_t,
        )
    };
    if r == 0 { Ok(()) } else { Err(last_errno()) }
}

#[cfg(target_os = "linux")]
fn sys_poll_out(fd: i32) -> Result<bool, i32> {
    let mut p = libc::pollfd {
        fd,
        events: libc::POLLOUT,
        revents: 0,
    };
    // SAFETY: one valid pollfd; timeout 0 (never blocks).
    let r = unsafe { libc::poll(&mut p, 1, 0) };
    if r < 0 {
        Err(last_errno())
    } else {
        Ok(r > 0 && p.revents & (libc::POLLOUT | libc::POLLERR | libc::POLLHUP) != 0)
    }
}

#[cfg(target_os = "linux")]
fn sys_getsockname(fd: i32) -> Result<(Ipv4Addr, u16), i32> {
    // SAFETY: sockaddr_in is plain data; all-zero is valid.
    let mut sa: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    let mut l = size_of::<libc::sockaddr_in>() as libc::socklen_t;
    // SAFETY: `sa`/`l` are valid for writes; the kernel writes at most `l` bytes.
    let r = unsafe { libc::getsockname(fd, (&mut sa as *mut libc::sockaddr_in).cast(), &mut l) };
    if r != 0 {
        return Err(last_errno());
    }
    Ok((
        Ipv4Addr::from(sa.sin_addr.s_addr.to_ne_bytes()),
        u16::from_be(sa.sin_port),
    ))
}

#[cfg(target_os = "linux")]
fn sys_shutdown_close(fd: i32) {
    if fd < 0 {
        return;
    }
    // SAFETY: plain system calls on an fd this socket owns.
    unsafe {
        libc::shutdown(fd, libc::SHUT_RDWR);
        libc::close(fd);
    }
}

// ---------------------------------------------------------------- other hosts: no sockets

#[cfg(not(target_os = "linux"))]
fn sys_recvmsg(_: i32, _: &mut [u8], _: &mut [u64; CMSG_BUF / 8]) -> (isize, usize, i32) {
    (-(errno::ENOSYS as isize), 0, 0)
}
#[cfg(not(target_os = "linux"))]
fn sys_sendmsg(_: i32, _: &[&[u8]]) -> isize {
    -(errno::ENOSYS as isize)
}
#[cfg(not(target_os = "linux"))]
fn sys_setsockopt(_: i32, _: i32, _: i32, _: &[u8]) -> Result<(), i32> {
    Err(errno::ENOSYS)
}
#[cfg(not(target_os = "linux"))]
fn sys_getsockopt_i32(_: i32, _: i32, _: i32) -> Result<i32, i32> {
    Err(errno::ENOSYS)
}
#[cfg(not(target_os = "linux"))]
fn sys_socket() -> Result<i32, i32> {
    Err(errno::ENOSYS)
}
#[cfg(not(target_os = "linux"))]
fn sys_bind(_: i32, _: Ipv4Addr, _: u16) -> Result<(), i32> {
    Err(errno::ENOSYS)
}
#[cfg(not(target_os = "linux"))]
fn sys_connect(_: i32, _: Ipv4Addr, _: u16) -> Result<(), i32> {
    Err(errno::ENOSYS)
}
#[cfg(not(target_os = "linux"))]
fn sys_poll_out(_: i32) -> Result<bool, i32> {
    Err(errno::ENOSYS)
}
#[cfg(not(target_os = "linux"))]
fn sys_getsockname(_: i32) -> Result<(Ipv4Addr, u16), i32> {
    Err(errno::ENOSYS)
}
#[cfg(not(target_os = "linux"))]
fn sys_shutdown_close(_: i32) {}

// ---------------------------------------------------------------- epoll (ReadyPoll)

/// Ready-event buffer of `ReadyPoll` (64 entries).
#[cfg(target_os = "linux")]
pub(crate) struct EpollBuf(pub [libc::epoll_event; 64]);

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub(crate) struct EpollBuf(pub [u64; 64]);

impl EpollBuf {
    pub fn new() -> EpollBuf {
        #[cfg(target_os = "linux")]
        {
            EpollBuf([libc::epoll_event { events: 0, u64: 0 }; 64])
        }
        #[cfg(not(target_os = "linux"))]
        {
            EpollBuf([0; 64])
        }
    }
}

/// `epoll_create1(EPOLL_CLOEXEC)`.
pub(crate) fn epoll_create() -> Result<i32, i32> {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: plain system call without pointers.
        let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if fd < 0 { Err(last_errno()) } else { Ok(fd) }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(errno::ENOSYS)
    }
}

/// `epoll_ctl(epfd, ADD | DEL, fd, EPOLLIN, data = key)`.
pub(crate) fn epoll_ctl(epfd: i32, add: bool, fd: i32, key: u64) -> Result<(), i32> {
    #[cfg(target_os = "linux")]
    {
        let mut ev = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: key,
        };
        let op = if add {
            libc::EPOLL_CTL_ADD
        } else {
            libc::EPOLL_CTL_DEL
        };
        // SAFETY: `ev` is a valid epoll_event for the call's duration.
        let r = unsafe { libc::epoll_ctl(epfd, op, fd, &mut ev) };
        if r == 0 { Ok(()) } else { Err(last_errno()) }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (epfd, add, fd, key);
        Err(errno::ENOSYS)
    }
}

/// `epoll_pwait(epfd, buf, 64, timeout = 0, NULL)`; calls `f(key)` for each ready entry.
/// Returns the ready count or `-errno`.
#[inline]
pub(crate) fn epoll_pwait0(epfd: i32, buf: &mut EpollBuf, mut f: impl FnMut(u64)) -> i32 {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: `buf` holds 64 writable epoll_event entries; timeout 0 never blocks; a null
        // sigmask keeps the thread's mask.
        let n = unsafe { libc::epoll_pwait(epfd, buf.0.as_mut_ptr(), 64, 0, std::ptr::null()) };
        if n < 0 {
            return -last_errno();
        }
        for e in buf.0.iter().take(n as usize) {
            let key = e.u64;
            f(key);
        }
        n
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (epfd, buf, &mut f);
        -errno::ENOSYS
    }
}

/// `fcntl(fd, F_GETFD)` (tests: `FD_CLOEXEC`).
pub fn fd_flags(fd: i32) -> Result<i32, i32> {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: plain system call without pointers.
        let r = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if r < 0 { Err(last_errno()) } else { Ok(r) }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = fd;
        Err(errno::ENOSYS)
    }
}

// ---------------------------------------------------------------- netdev netlink (NAPI id → ifindex)

/// Asks the kernel (netdev generic netlink family, kernel ≥ 6.9) which interface owns NAPI
/// instance `napi_id`. Warm thread only; `None` on any failure.
pub(crate) fn napi_ifindex(napi_id: u32) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        netlink::napi_ifindex(napi_id)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = napi_id;
        None
    }
}

#[cfg(target_os = "linux")]
mod netlink {
    const NETLINK_GENERIC: i32 = 16;
    const GENL_ID_CTRL: u16 = 0x10;
    const CTRL_CMD_GETFAMILY: u8 = 3;
    const CTRL_ATTR_FAMILY_ID: u16 = 1;
    const CTRL_ATTR_FAMILY_NAME: u16 = 2;
    const NETDEV_CMD_NAPI_GET: u8 = 11;
    const NETDEV_A_NAPI_IFINDEX: u16 = 1;
    const NETDEV_A_NAPI_ID: u16 = 2;
    const NLM_F_REQUEST: u16 = 1;
    const NLMSG_ERROR: u16 = 2;

    struct Fd(i32);
    impl Drop for Fd {
        fn drop(&mut self) {
            // SAFETY: closes the netlink socket this guard owns.
            unsafe {
                libc::close(self.0);
            }
        }
    }

    fn attr(buf: &mut Vec<u8>, typ: u16, val: &[u8]) {
        let len = 4 + val.len();
        buf.extend_from_slice(&(len as u16).to_ne_bytes());
        buf.extend_from_slice(&typ.to_ne_bytes());
        buf.extend_from_slice(val);
        while !buf.len().is_multiple_of(4) {
            buf.push(0);
        }
    }

    fn request(fd: &Fd, family: u16, cmd: u8, attrs: &[u8], seq: u32) -> Option<Vec<u8>> {
        let mut m = Vec::with_capacity(64);
        let len = 16 + 4 + attrs.len();
        m.extend_from_slice(&(len as u32).to_ne_bytes());
        m.extend_from_slice(&family.to_ne_bytes());
        m.extend_from_slice(&NLM_F_REQUEST.to_ne_bytes());
        m.extend_from_slice(&seq.to_ne_bytes());
        m.extend_from_slice(&0u32.to_ne_bytes());
        m.extend_from_slice(&[cmd, 1, 0, 0]);
        m.extend_from_slice(attrs);
        // SAFETY: `m` is valid for reads of its length.
        let r = unsafe { libc::send(fd.0, m.as_ptr().cast(), m.len(), 0) };
        if r < 0 {
            return None;
        }
        let mut buf = vec![0u8; 8192];
        // SAFETY: `buf` is valid for writes of its length.
        let n = unsafe { libc::recv(fd.0, buf.as_mut_ptr().cast(), buf.len(), 0) };
        if n < 20 {
            return None;
        }
        buf.truncate(n as usize);
        let typ = u16::from_ne_bytes([buf[4], buf[5]]);
        if typ == NLMSG_ERROR {
            return None;
        }
        Some(buf)
    }

    /// Attributes after the nlmsghdr + genlmsghdr.
    fn find(msg: &[u8], want: u16) -> Option<&[u8]> {
        let total = u32::from_ne_bytes(msg.get(0..4)?.try_into().ok()?) as usize;
        let mut off = 20;
        let end = total.min(msg.len());
        while off + 4 <= end {
            let len = u16::from_ne_bytes(msg.get(off..off + 2)?.try_into().ok()?) as usize;
            let typ = u16::from_ne_bytes(msg.get(off + 2..off + 4)?.try_into().ok()?) & 0x3FFF;
            if len < 4 {
                return None;
            }
            if typ == want {
                return msg.get(off + 4..off + len);
            }
            off += (len + 3) & !3;
        }
        None
    }

    pub(super) fn napi_ifindex(napi_id: u32) -> Option<u32> {
        // SAFETY: plain system call without pointers.
        let fd = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_RAW | libc::SOCK_CLOEXEC,
                NETLINK_GENERIC,
            )
        };
        if fd < 0 {
            return None;
        }
        let fd = Fd(fd);
        let mut a = Vec::new();
        attr(&mut a, CTRL_ATTR_FAMILY_NAME, b"netdev\0");
        let resp = request(&fd, GENL_ID_CTRL, CTRL_CMD_GETFAMILY, &a, 1)?;
        let fam = find(&resp, CTRL_ATTR_FAMILY_ID)?;
        let family = u16::from_ne_bytes(fam.get(0..2)?.try_into().ok()?);
        let mut a = Vec::new();
        attr(&mut a, NETDEV_A_NAPI_ID, &napi_id.to_ne_bytes());
        let resp = request(&fd, family, NETDEV_CMD_NAPI_GET, &a, 2)?;
        let idx = find(&resp, NETDEV_A_NAPI_IFINDEX)?;
        Some(u32::from_ne_bytes(idx.get(0..4)?.try_into().ok()?))
    }
}
