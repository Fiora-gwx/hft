//! Thread-plan execution: `PollTask`, `TaskLoop`, `IdlePolicy`, `Parker`/`Unparker`.
//!
//! Wake protocol (README "唤醒协议"): the consumer's `state = PARKING` + `fence(SeqCst)` + ring
//! check and the producer's ring write + `fence(SeqCst)` + `state` load form a Dekker pair: either
//! the consumer sees the data or the producer sees PARKING/PARKED and takes the wake duty.

use core::sync::atomic::AtomicBool;

use crate::RtErr;
use crate::sync::{AtomicU32, Ordering, fence};
use crate::tsc::{Tsc, TscClock, tsc_now};
use crate::wait::{CachePadded, Heartbeat, cpu_relax};

/// Upper bound of `WaitFds`.
pub const MAX_WAIT_FDS: usize = 64;

/// File descriptors a task wants watched while its thread is parked (Epoll backend).
#[derive(Debug, Clone)]
pub struct WaitFds {
    fds: [i32; MAX_WAIT_FDS],
    len: usize,
}

impl Default for WaitFds {
    fn default() -> WaitFds {
        WaitFds::new()
    }
}

impl WaitFds {
    pub const fn new() -> WaitFds {
        WaitFds {
            fds: [-1; MAX_WAIT_FDS],
            len: 0,
        }
    }
    /// `false` when full.
    pub fn push(&mut self, fd: i32) -> bool {
        match self.fds.get_mut(self.len) {
            Some(s) => {
                *s = fd;
                self.len += 1;
                true
            }
            None => false,
        }
    }
    pub fn as_slice(&self) -> &[i32] {
        &self.fds[..self.len]
    }
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

/// One task of a thread plan (static dispatch: apps define an enum of their task types).
pub trait PollTask {
    /// Non-blocking, bounded work; whether anything was done.
    fn poll_once(&mut self, now: Tsc) -> Polled;
    /// Next TSC instant this task must be called at; `None` = input driven only.
    fn next_deadline(&self) -> Option<Tsc> {
        None
    }
    /// Sockets to watch while parked (Epoll backend).
    fn wait_fds(&self, out: &mut WaitFds) {
        let _ = out;
    }
    /// Last check before parking: anything pending in its inputs (read only).
    fn has_pending(&self) -> bool;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Polled {
    pub did_work: bool,
}

/// Idle-handling decision after a round; `Park { 0 }` is never returned (it means `Continue`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum IdleAction {
    Continue,
    Relax,
    Park { timeout_ns: u64 },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum IdlePolicy {
    Spin,
    SpinThenPark { spin_us: u32, park_us: u32 },
    Park { park_us: u32 },
}

pub(crate) const RUNNING: u32 = 0;
pub(crate) const PARKING: u32 = 1;
pub(crate) const PARKED: u32 = 2;

/// The park state word and its protocol steps (exposed for the loom model).
#[derive(Debug)]
pub struct ParkState {
    state: CachePadded<AtomicU32>,
}

impl Default for ParkState {
    fn default() -> ParkState {
        ParkState::new()
    }
}

impl ParkState {
    pub const RUNNING: u32 = RUNNING;
    pub const PARKING: u32 = PARKING;
    pub const PARKED: u32 = PARKED;

    pub fn new() -> ParkState {
        ParkState {
            state: CachePadded(AtomicU32::new(RUNNING)),
        }
    }

    /// Consumer step 1: `PARKING` then a SeqCst fence (Dekker pair with `notify`'s fence).
    pub fn begin_park(&self) {
        self.state.store(PARKING, Ordering::SeqCst);
        fence(Ordering::SeqCst);
    }

    /// Consumer: back to running (cancel, or after the wait).
    pub fn finish_park(&self) {
        self.state.store(RUNNING, Ordering::SeqCst);
    }

    /// Consumer step 3: `PARKING → PARKED`; `false` = a producer already reset it (cancel).
    pub fn commit_park(&self) -> bool {
        self.state
            .compare_exchange(PARKING, PARKED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn load(&self) -> u32 {
        self.state.load(Ordering::SeqCst)
    }

    /// Producer, after its Release store of the ring cursor: `true` = the consumer was PARKED
    /// and needs the system-call wake. No RMW when RUNNING; PARKING → reset without a syscall.
    #[inline(always)]
    pub fn notify(&self) -> bool {
        fence(Ordering::SeqCst);
        if self.state.load(Ordering::Relaxed) == RUNNING {
            return false;
        }
        self.state.swap(RUNNING, Ordering::SeqCst) == PARKED
    }

    #[cfg(all(target_os = "linux", not(loom)))]
    fn addr(&self) -> *mut u32 {
        self.state.0.as_ptr()
    }
}

/// Park backend: threads with sockets (feed/gw/ext) use `Epoll`, others `Futex`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ParkBackend {
    Futex,
    Epoll,
}

#[derive(Debug)]
struct ParkInner {
    st: ParkState,
    #[allow(dead_code)]
    backend: ParkBackend,
    #[allow(dead_code)]
    epfd: i32,
    #[allow(dead_code)]
    efd: i32,
}

/// The parking side (owned by the consumer thread's `TaskLoop`).
#[derive(Debug)]
pub struct Parker {
    inner: &'static ParkInner,
    #[cfg_attr(not(all(target_os = "linux", not(loom))), allow(dead_code))]
    registered: WaitFds,
}

/// The waking side, handed to producers at assembly time.
#[derive(Clone, Debug)]
pub struct Unparker {
    inner: &'static ParkInner,
}

impl Parker {
    /// Startup; `Epoll` creates an epoll fd and an eventfd. Off Linux → `Os(ENOSYS)`.
    pub fn new(backend: ParkBackend) -> Result<Parker, RtErr> {
        let (epfd, efd) = imp::make(backend)?;
        let inner = Box::leak(Box::new(ParkInner {
            st: ParkState::new(),
            backend,
            epfd,
            efd,
        }));
        Ok(Parker {
            inner,
            registered: WaitFds::new(),
        })
    }

    pub fn unparker(&self) -> Unparker {
        Unparker { inner: self.inner }
    }

    fn state(&self) -> &ParkState {
        &self.inner.st
    }

    /// Blocks until woken or `timeout_ns` elapsed (the state is PARKED on entry).
    fn wait(&mut self, timeout_ns: u64, fds: &WaitFds) {
        imp::wait(self, timeout_ns, fds);
    }
}

impl Unparker {
    /// Wake check after a commit; a system call only when the consumer is PARKED.
    #[inline(always)]
    pub fn unpark(&self) {
        if self.inner.st.notify() {
            imp::wake(self.inner);
        }
    }
}

#[cfg(all(target_os = "linux", not(loom)))]
mod imp {
    use super::*;
    use crate::sys::{clock_ns, errno};

    pub(super) fn make(backend: ParkBackend) -> Result<(i32, i32), RtErr> {
        match backend {
            ParkBackend::Futex => Ok((-1, -1)),
            ParkBackend::Epoll => {
                // SAFETY: plain flags.
                let epfd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
                if epfd < 0 {
                    return Err(RtErr::Os(errno()));
                }
                // SAFETY: plain flags.
                let efd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
                if efd < 0 {
                    let e = errno();
                    // SAFETY: closing the epoll fd created above.
                    unsafe { libc::close(epfd) };
                    return Err(RtErr::Os(e));
                }
                let mut ev = libc::epoll_event {
                    events: libc::EPOLLIN as u32,
                    u64: efd as u64,
                };
                // SAFETY: both descriptors are valid; `ev` outlives the call.
                if unsafe { libc::epoll_ctl(epfd, libc::EPOLL_CTL_ADD, efd, &mut ev) } != 0 {
                    let e = errno();
                    // SAFETY: closing descriptors created above.
                    unsafe {
                        libc::close(efd);
                        libc::close(epfd);
                    }
                    return Err(RtErr::Os(e));
                }
                Ok((epfd, efd))
            }
        }
    }

    pub(super) fn wake(inner: &ParkInner) {
        match inner.backend {
            ParkBackend::Futex => {
                // SAFETY: FUTEX_WAKE on the live, 'static state word.
                unsafe {
                    libc::syscall(
                        libc::SYS_futex,
                        inner.st.addr(),
                        libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG,
                        1,
                    )
                };
            }
            ParkBackend::Epoll => {
                let one: u64 = 1;
                // SAFETY: writes 8 bytes from a live u64 to the eventfd.
                unsafe { libc::write(inner.efd, (&one as *const u64).cast(), 8) };
            }
        }
    }

    fn sync_fds(p: &mut Parker, fds: &WaitFds) {
        let epfd = p.inner.epfd;
        let old = p.registered.clone();
        for &fd in old.as_slice() {
            if !fds.as_slice().contains(&fd) {
                // SAFETY: removing a registration; a stale fd only yields an error.
                unsafe { libc::epoll_ctl(epfd, libc::EPOLL_CTL_DEL, fd, core::ptr::null_mut()) };
            }
        }
        for &fd in fds.as_slice() {
            if !old.as_slice().contains(&fd) {
                let mut ev = libc::epoll_event {
                    events: libc::EPOLLIN as u32,
                    u64: fd as u64,
                };
                // SAFETY: valid epoll fd; `ev` outlives the call.
                unsafe { libc::epoll_ctl(epfd, libc::EPOLL_CTL_ADD, fd, &mut ev) };
            }
        }
        p.registered = fds.clone();
    }

    pub(super) fn wait(p: &mut Parker, timeout_ns: u64, fds: &WaitFds) {
        let deadline = clock_ns(libc::CLOCK_MONOTONIC).saturating_add(timeout_ns.min(i64::MAX as u64) as i64);
        match p.inner.backend {
            ParkBackend::Futex => loop {
                if p.state().load() != PARKED {
                    return;
                }
                let left = deadline - clock_ns(libc::CLOCK_MONOTONIC);
                if left <= 0 {
                    return;
                }
                let ts = libc::timespec {
                    tv_sec: (left / 1_000_000_000) as libc::time_t,
                    tv_nsec: (left % 1_000_000_000) as _,
                };
                // SAFETY: FUTEX_WAIT on the live state word with a valid relative timeout.
                unsafe {
                    libc::syscall(
                        libc::SYS_futex,
                        p.state().addr(),
                        libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG,
                        PARKED,
                        &ts as *const libc::timespec,
                    )
                };
            },
            ParkBackend::Epoll => {
                sync_fds(p, fds);
                let mut evs = [libc::epoll_event { events: 0, u64: 0 }; 8];
                loop {
                    if p.state().load() != PARKED {
                        break;
                    }
                    let left = deadline - clock_ns(libc::CLOCK_MONOTONIC);
                    if left <= 0 {
                        break;
                    }
                    let ms = (left as u64).div_ceil(1_000_000).min(i32::MAX as u64) as i32;
                    // SAFETY: valid epoll fd and event buffer.
                    let n = unsafe { libc::epoll_wait(p.inner.epfd, evs.as_mut_ptr(), evs.len() as i32, ms) };
                    if n > 0 {
                        break;
                    }
                }
                let mut buf = 0u64;
                // SAFETY: non-blocking read of the eventfd counter into a live u64 (drain).
                unsafe { libc::read(p.inner.efd, (&mut buf as *mut u64).cast(), 8) };
            }
        }
    }
}

#[cfg(not(all(target_os = "linux", not(loom))))]
mod imp {
    use super::*;

    pub(super) fn make(_: ParkBackend) -> Result<(i32, i32), RtErr> {
        Err(RtErr::Os(crate::sys::ENOSYS))
    }
    pub(super) fn wake(_: &ParkInner) {}
    pub(super) fn wait(_: &mut Parker, _: u64, _: &WaitFds) {}
}

/// Runs one thread's tasks in a fixed order with the configured idle policy.
pub struct TaskLoop<T: PollTask, const N: usize> {
    tasks: [T; N],
    policy: IdlePolicy,
    parker: Option<Parker>,
    idle_since: Option<Tsc>,
    spin_ticks: u64,
    park_ns: u64,
    clock: TscClock,
    fds: WaitFds,
}

impl<T: PollTask, const N: usize> TaskLoop<T, N> {
    /// `parker` is `None` for `Spin`; `clock` converts `spin_us`/`park_us` to counter ticks.
    pub fn new(
        tasks: [T; N],
        policy: IdlePolicy,
        parker: Option<Parker>,
        clock: &TscClock,
    ) -> TaskLoop<T, N> {
        let (spin_us, park_us) = match policy {
            IdlePolicy::Spin => (0, 0),
            IdlePolicy::SpinThenPark { spin_us, park_us } => (spin_us, park_us),
            IdlePolicy::Park { park_us } => (0, park_us),
        };
        TaskLoop {
            tasks,
            policy,
            parker,
            idle_since: None,
            spin_ticks: clock.us_to_ticks(spin_us as u64),
            park_ns: park_us as u64 * 1000,
            clock: *clock,
            fds: WaitFds::new(),
        }
    }

    pub fn tasks(&self) -> &[T; N] {
        &self.tasks
    }

    pub fn tasks_mut(&mut self) -> &mut [T; N] {
        &mut self.tasks
    }

    /// One `poll_once` per task in configured order; whether any task did work.
    #[inline]
    pub fn round(&mut self, now: Tsc) -> bool {
        let mut did = false;
        for t in self.tasks.iter_mut() {
            did |= t.poll_once(now).did_work;
        }
        did
    }

    /// Idle-handling steps 1–4 (pure; time from the argument).
    #[inline]
    pub fn idle_action(&mut self, now: Tsc, did_work: bool) -> IdleAction {
        if did_work {
            self.idle_since = None;
            return IdleAction::Continue;
        }
        let since = *self.idle_since.get_or_insert(now);
        match self.policy {
            IdlePolicy::Spin => IdleAction::Relax,
            IdlePolicy::SpinThenPark { .. } if now.0.saturating_sub(since.0) < self.spin_ticks => {
                IdleAction::Relax
            }
            IdlePolicy::SpinThenPark { .. } | IdlePolicy::Park { .. } => {
                let mut t = self.park_ns;
                for task in self.tasks.iter() {
                    if let Some(d) = task.next_deadline() {
                        t = t.min(self.clock.delta_ns(now, d));
                    }
                }
                if t == 0 {
                    IdleAction::Continue
                } else {
                    IdleAction::Park { timeout_ns: t }
                }
            }
        }
    }

    /// Thread main loop: `beat()`, one round, idle handling; returns once `stop` is set.
    pub fn run(&mut self, stop: &AtomicBool, hb: &Heartbeat) {
        loop {
            // Acquire pairs with the controller's Release store of `stop`.
            if stop.load(core::sync::atomic::Ordering::Acquire) {
                return;
            }
            hb.beat();
            let did = self.round(tsc_now());
            match self.idle_action(tsc_now(), did) {
                IdleAction::Continue => {}
                IdleAction::Relax => cpu_relax(),
                IdleAction::Park { timeout_ns } => self.park(timeout_ns),
            }
        }
    }

    fn park(&mut self, timeout_ns: u64) {
        let Some(parker) = self.parker.as_mut() else {
            cpu_relax();
            return;
        };
        let st = parker.state();
        st.begin_park();
        if self.tasks.iter().any(|t| t.has_pending()) {
            st.finish_park();
            return;
        }
        if !st.commit_park() {
            st.finish_park();
            return;
        }
        self.fds.clear();
        for t in self.tasks.iter() {
            t.wait_fds(&mut self.fds);
        }
        parker.wait(timeout_ns, &self.fds);
        parker.state().finish_park();
    }
}

impl<T: PollTask, const N: usize> core::fmt::Debug for TaskLoop<T, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TaskLoop")
            .field("n", &N)
            .field("policy", &self.policy)
            .field("idle_since", &self.idle_since)
            .finish_non_exhaustive()
    }
}
