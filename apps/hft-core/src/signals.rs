//! Linux blocked-signal intake. Both readers only request the normal journaled ctl Drain.
//! Main must also read: a signal sent with tgkill to main is not pending for housekeeping.
use crate::Error;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

#[derive(Debug)]
pub struct Signals(OwnedFd);

impl Signals {
    /// Call on main before spawning any thread, so workers inherit the blocked mask.
    pub fn block() -> Result<Self, Error> {
        // SAFETY: all libc pointers refer to initialized, correctly sized local objects.
        unsafe {
            let mut mask: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut mask);
            for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
                libc::sigaddset(&mut mask, signal);
            }
            if libc::pthread_sigmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut()) != 0 {
                return Err(Error::runtime("Signal", "cannot block service signals", 3));
            }
            let fd = libc::signalfd(-1, &mask, libc::SFD_NONBLOCK | libc::SFD_CLOEXEC);
            if fd < 0 {
                return Err(Error::runtime("Signal", "cannot create signalfd", 3));
            }
            Ok(Self(OwnedFd::from_raw_fd(fd)))
        }
    }

    /// A bounded batch; SIGHUP is consumed without changing journaled configuration.
    pub fn drain_requested(&self) -> Result<bool, Error> {
        let mut drain = false;
        for _ in 0..16 {
            // SAFETY: the kernel writes at most sizeof(signalfd_siginfo) into this object.
            let mut info: libc::signalfd_siginfo = unsafe { std::mem::zeroed() };
            let len = std::mem::size_of_val(&info);
            let read = unsafe {
                libc::read(
                    self.0.as_raw_fd(),
                    (&mut info as *mut libc::signalfd_siginfo).cast(),
                    len,
                )
            };
            if read < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    break;
                }
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(Error::runtime("Signal", "signalfd read failed", 3));
            }
            if read as usize != len {
                return Err(Error::runtime("Signal", "short signalfd record", 3));
            }
            drain |= info.ssi_signo == libc::SIGTERM as u32 || info.ssi_signo == libc::SIGINT as u32;
        }
        Ok(drain)
    }
}
