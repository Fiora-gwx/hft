//! `NotifyQueue` (per Input, 128 entries in fixed partitions) and `NotifyBacklog` (`4 × U`,
//! continued by `Timer{Sys, NotifyContinue}`); notifications are never dropped (README "通知队列与续推").

use hft_event::ConnStateKind;
use hft_strategy_sdk::{ExecUpdate, ExternalFillView, OrderUpdate, RiskEvent, VenueNoticeView};
use hft_types::{AccountId, GroupId, InstId, RunMode, VenueId};

/// Partition capacities.
pub const PART_OMS: usize = 16;
pub const PART_EXEC: usize = 8;
pub const PART_RISK: usize = 8;
pub const PART_MODE: usize = 64;
pub const PART_MARKET: usize = 8;
pub const PART_OTHER: usize = 24;
/// Total per-Input queue capacity.
pub const NOTIFY_QUEUE_CAP: usize = PART_OMS + PART_EXEC + PART_RISK + PART_MODE + PART_MARKET + PART_OTHER;
const _: () = assert!(NOTIFY_QUEUE_CAP == 128);

/// Queue partition of a notification.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Part {
    Oms = 0,
    Exec = 1,
    Risk = 2,
    Mode = 3,
    Market = 4,
    Other = 5,
}

impl Part {
    pub const fn cap(self) -> usize {
        match self {
            Part::Oms => PART_OMS,
            Part::Exec => PART_EXEC,
            Part::Risk => PART_RISK,
            Part::Mode => PART_MODE,
            Part::Market => PART_MARKET,
            Part::Other => PART_OTHER,
        }
    }
}

/// An owned notification for one instance (index into the instance table).
#[derive(Copy, Clone, Debug)]
pub enum Notif {
    Order { gix: u16, u: OrderUpdate },
    Exec { gix: u16, u: ExecUpdate },
    External { gix: u16, f: ExternalFillView },
    Risk { a: AccountId, e: RiskEvent },
    Mode { gix: u16, old: RunMode, new: RunMode },
    Conn { venue: VenueId, state: ConnStateKind },
    Notice { inst: InstId, gix: u16, n: VenueNoticeView },
    Timer { tag: u32, group: GroupId },
}

impl Notif {
    pub fn part(&self) -> Part {
        match self {
            Notif::Order { .. } | Notif::External { .. } => Part::Oms,
            Notif::Exec { .. } => Part::Exec,
            Notif::Risk { .. } => Part::Risk,
            Notif::Mode { .. } => Part::Mode,
            Notif::Notice { .. } => Part::Market,
            Notif::Conn { .. } | Notif::Timer { .. } => Part::Other,
        }
    }
}

/// One queued notification: target instance (slot index) + payload.
#[derive(Copy, Clone, Debug)]
pub struct Queued {
    pub inst: u16,
    pub n: Notif,
}

/// The per-Input queue and the backlog.
#[derive(Clone, Debug)]
pub struct Notify {
    q: Vec<Queued>,
    used: [usize; 6],
    /// Set once a partition overflowed in this Input: everything after goes to the backlog.
    spilled: bool,
    backlog: std::collections::VecDeque<Queued>,
    backlog_cap: usize,
    /// High-water of the backlog (tests).
    pub backlog_peak: usize,
}

impl Notify {
    /// `u` = per-Input bound `max_cells_total + 128`; backlog capacity `4u`.
    pub fn new(u: usize) -> Notify {
        Notify {
            q: Vec::with_capacity(NOTIFY_QUEUE_CAP),
            used: [0; 6],
            spilled: false,
            backlog: std::collections::VecDeque::with_capacity(4 * u),
            backlog_cap: 4 * u,
            backlog_peak: 0,
        }
    }
    /// Starts a new Input.
    pub fn begin(&mut self) {
        self.q.clear();
        self.used = [0; 6];
        self.spilled = false;
    }
    /// Enqueues; partition full (or backlog non-empty) → backlog in production order.
    pub fn push(&mut self, inst: u16, n: Notif) {
        let p = n.part() as usize;
        let room = self.used.get(p).copied().unwrap_or(0) < n.part().cap();
        if !self.spilled && self.backlog.is_empty() && room {
            if let Some(u) = self.used.get_mut(p) {
                *u += 1;
            }
            self.q.push(Queued { inst, n });
            return;
        }
        if !room {
            self.spilled = true;
        }
        self.push_backlog(Queued { inst, n });
    }
    fn push_backlog(&mut self, x: Queued) {
        // Capacity is a construction bound (4U); exceeding it is an engine defect: keep the entry
        // anyway (never drop) and let the peak counter show it.
        self.backlog.push_back(x);
        self.backlog_peak = self.backlog_peak.max(self.backlog.len());
    }
    /// Into the backlog directly (notifications left after the second delivery round).
    pub fn push_spill(&mut self, x: Queued) {
        self.push_backlog(x);
    }
    /// Into this Input's queue regardless of partitions (backlog entries of a NotifyContinue).
    pub fn push_front_queue(&mut self, x: Queued) {
        self.q.push(x);
    }
    /// The queue of this Input.
    pub fn queued(&self) -> &[Queued] {
        &self.q
    }
    pub fn take_queue(&mut self, out: &mut Vec<Queued>) {
        out.clear();
        out.extend_from_slice(&self.q);
        self.q.clear();
        self.used = [0; 6];
    }
    pub fn backlog_len(&self) -> usize {
        self.backlog.len()
    }
    pub fn backlog_cap(&self) -> usize {
        self.backlog_cap
    }
    /// Free backlog slots.
    pub fn headroom(&self) -> usize {
        self.backlog_cap.saturating_sub(self.backlog.len())
    }
    /// Pops up to `n` backlog entries in order.
    pub fn pop_backlog(&mut self, n: usize, out: &mut Vec<Queued>) {
        out.clear();
        for _ in 0..n {
            match self.backlog.pop_front() {
                Some(x) => out.push(x),
                None => break,
            }
        }
    }
    /// Test hook: fills the backlog with `n` placeholder timer notifications for instance `inst`.
    pub fn fill_backlog(&mut self, inst: u16, n: usize) {
        for _ in 0..n {
            self.push_backlog(Queued {
                inst,
                n: Notif::Timer {
                    tag: u32::MAX - 1,
                    group: GroupId(0),
                },
            });
        }
    }
}
