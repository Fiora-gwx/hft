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
    use std::cell::RefCell;

    use super::Op;

    thread_local! {
        static LOG: RefCell<Option<Vec<Op>>> = const { RefCell::new(None) };
    }

    pub(crate) fn record(op: Op) {
        LOG.with(|l| {
            if let Some(v) = l.borrow_mut().as_mut() {
                v.push(op);
            }
        });
    }

    pub(crate) fn start() {
        LOG.with(|l| *l.borrow_mut() = Some(Vec::new()));
    }

    pub(crate) fn take() -> Vec<Op> {
        LOG.with(|l| l.borrow_mut().take().unwrap_or_default())
    }
}

#[cfg(not(feature = "mem-backend"))]
mod imp {
    use super::Op;

    #[inline(always)]
    pub(crate) fn record(_: Op) {}
}

pub(crate) use imp::*;
