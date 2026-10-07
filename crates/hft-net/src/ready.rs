//! `ReadyPoll`/`ReadyHandle`: readiness probing for market-data tasks (README "签名草案";
//! enabling rule in hft-venue `feed.ready_poll`). Level-triggered `EPOLLIN`, event data = `ConnId`.
//! The epoll system calls live in `sock.rs`.

use hft_types::ConnId;

use crate::conn::Conn;
use crate::sock::{self, EpollBuf, SysCount};
use crate::types::{ConnSet, NetErr};

/// The probe side (owned by the market-data task's thread).
pub struct ReadyPoll {
    epfd: i32,
    buf: EpollBuf,
    sys: SysCount,
    #[cfg(feature = "test-io")]
    script_err: Option<i32>,
    /// Scripted readiness: the latest set pushed by the test is the level-triggered ready set.
    #[cfg(feature = "test-io")]
    script_ready: Option<(hft_rt::Consumer<ConnSet>, ConnSet)>,
}

impl std::fmt::Debug for ReadyPoll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadyPoll").field("epfd", &self.epfd).finish()
    }
}

impl ReadyPoll {
    /// Startup: `epoll_create1(EPOLL_CLOEXEC)`; the epfd lives for the whole process.
    pub fn new() -> Result<ReadyPoll, NetErr> {
        let epfd = sock::epoll_create().map_err(NetErr::Os)?;
        Ok(ReadyPoll {
            epfd,
            buf: EpollBuf::new(),
            sys: SysCount::default(),
            #[cfg(feature = "test-io")]
            script_err: None,
            #[cfg(feature = "test-io")]
            script_ready: None,
        })
    }

    /// The registration side for `conn-builder`.
    pub fn handle(&self) -> ReadyHandle {
        ReadyHandle { epfd: self.epfd }
    }

    /// Hot thread: one `epoll_pwait(epfd, events, 64, timeout = 0)`; writes the ready `ConnId`s
    /// into `out` (cleared first) and returns their count. More than 64 ready → the rest next
    /// time (level-triggered). A failed call → 0 and `out` empty. No other system call.
    #[inline]
    pub fn probe(&mut self, out: &mut ConnSet) -> u16 {
        out.clear();
        self.sys.epoll_pwait += 1;
        self.sys.last_epoll_timeout = 0;
        #[cfg(feature = "test-io")]
        if self.script_err.is_some() {
            return 0;
        }
        #[cfg(feature = "test-io")]
        if let Some((rx, cur)) = self.script_ready.as_mut() {
            while let Some(set) = rx.try_pop() {
                *cur = set;
            }
            *out = *cur;
            return cur.bits.iter().map(|w| w.count_ones() as u16).sum();
        }
        let n = sock::epoll_pwait0(self.epfd, &mut self.buf, |key| out.insert(ConnId(key as u8)));
        if n < 0 { 0 } else { n as u16 }
    }

    /// The epoll fd.
    pub fn epfd(&self) -> i32 {
        self.epfd
    }

    /// System calls made by `probe`.
    pub fn sys(&self) -> &SysCount {
        &self.sys
    }

    /// A probe whose `epoll_pwait` fails with `errno` (feature `test-io`).
    #[cfg(feature = "test-io")]
    #[doc(hidden)]
    pub fn scripted_error(errno: i32) -> ReadyPoll {
        ReadyPoll {
            epfd: -1,
            buf: EpollBuf::new(),
            sys: SysCount::default(),
            script_err: Some(errno),
            script_ready: None,
        }
    }

    /// A probe whose ready set is the latest `ConnSet` pushed into `rx` (feature `test-io`; scripted
    /// connections have no fd). Each `probe` still counts one `epoll_pwait(timeout = 0)`.
    #[cfg(feature = "test-io")]
    #[doc(hidden)]
    pub fn scripted(rx: hft_rt::Consumer<ConnSet>) -> ReadyPoll {
        ReadyPoll {
            epfd: -1,
            buf: EpollBuf::new(),
            sys: SysCount::default(),
            script_err: None,
            script_ready: Some((rx, ConnSet::default())),
        }
    }
}

/// Registration handle (`Send`); used only on the warm thread.
#[derive(Copy, Clone, Debug)]
pub struct ReadyHandle {
    epfd: i32,
}

impl ReadyHandle {
    /// `EPOLL_CTL_ADD` before the hand-off; a second registration → `Os(EEXIST)`.
    pub fn add(&self, c: &Conn) -> Result<(), NetErr> {
        #[cfg(feature = "test-io")]
        if self.epfd < 0 {
            return Ok(());
        }
        sock::epoll_ctl(self.epfd, true, c.raw_fd(), c.id().0 as u64).map_err(NetErr::Os)
    }

    /// `EPOLL_CTL_DEL` before `Connector::reclaim`; not registered → `Os(ENOENT)`.
    pub fn del(&self, c: &Conn) -> Result<(), NetErr> {
        self.del_fd(c.raw_fd())
    }

    /// `EPOLL_CTL_DEL` by descriptor (the caller remembered it at hand-off; a retired connection is
    /// no longer reachable as a `Conn` before `Connector::reclaim`).
    pub fn del_fd(&self, fd: i32) -> Result<(), NetErr> {
        #[cfg(feature = "test-io")]
        if self.epfd < 0 {
            return Ok(());
        }
        sock::epoll_ctl(self.epfd, false, fd, 0).map_err(NetErr::Os)
    }
}
