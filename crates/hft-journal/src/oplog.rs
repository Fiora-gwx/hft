//! Per-thread operation log of the backend (feature `mem-backend`; empty otherwise). Tests use it
//! to observe the order of preparation, copies, syncs, retirement and mapping calls on a thread.

/// One logged operation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// `mmap` of a segment (or backend equivalent).
    Map,
    /// `munmap` of a segment.
    Unmap,
    /// `unlink` of a hot segment file.
    Unlink,
    /// A segment was prepared (`segment_no`).
    Prepare(u64),
    /// Bytes copied to a cold copy: `(segment_no, to)` = new `persisted_len`.
    Copy(u64, u64),
    /// `fdatasync` of a cold copy.
    Sync,
    /// `retired = 1` set on a hot segment.
    Retire(u64),
    /// A record appended by roll/seal/recovery (`kind`, `seq`).
    Append(u16, u64),
}

#[cfg(feature = "mem-backend")]
mod imp {
    #[allow(clippy::disallowed_types)] // allow-list: mem-backend test hook
    use std::sync::{Mutex, OnceLock};
    use std::thread::ThreadId;

    use super::Op;

    /// Runs `f` on the calling thread's log (a registry keyed by thread id: clippy's
    /// `disallowed_macros` cannot be allowed on a `thread_local!` item).
    #[allow(clippy::disallowed_types)] // allow-list: mem-backend test hook (per-thread log)
    fn with_log<R>(f: impl FnOnce(&mut Option<Vec<Op>>) -> R) -> R {
        type Logs = Vec<(ThreadId, Option<Vec<Op>>)>;
        static LOGS: OnceLock<Mutex<Logs>> = OnceLock::new();
        let id = std::thread::current().id();
        let mut g = LOGS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let i = match g.iter().position(|(t, _)| *t == id) {
            Some(i) => i,
            None => {
                g.push((id, None));
                g.len() - 1
            }
        };
        match g.get_mut(i) {
            Some((_, l)) => f(l),
            None => f(&mut None),
        }
    }

    pub(crate) fn record(op: Op) {
        with_log(|l| {
            if let Some(v) = l.as_mut() {
                v.push(op);
            }
        });
    }

    pub(crate) fn start() {
        with_log(|l| *l = Some(Vec::new()));
    }

    pub(crate) fn take() -> Vec<Op> {
        with_log(|l| l.take().unwrap_or_default())
    }
}

#[cfg(not(feature = "mem-backend"))]
mod imp {
    use super::Op;

    #[inline(always)]
    pub(crate) fn record(_: Op) {}
}

pub(crate) use imp::*;
