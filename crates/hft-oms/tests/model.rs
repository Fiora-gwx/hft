#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
//! Reference model of one order (README "回报处理表" encoded cell by cell, independent of the
//! implementation), used by `prop_fsm.rs` (OMS-380).
#![allow(dead_code)]

/// Model states (same codes as `OrderState`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum St {
    PendingNew,
    Live,
    PendingCancel,
    PendingAmend,
    Filled,
    Cancelled,
    Rejected,
    Unknown,
}

impl St {
    pub fn terminal(self) -> bool {
        matches!(self, St::Filled | St::Cancelled | St::Rejected)
    }
    pub fn code(self) -> u8 {
        match self {
            St::PendingNew => 1,
            St::Live => 2,
            St::PendingCancel => 3,
            St::PendingAmend => 4,
            St::Filled => 5,
            St::Cancelled => 6,
            St::Rejected => 7,
            St::Unknown => 8,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Rep {
    Ack,
    Reject,
    Fill(i64),
    CancelAck(i64),
    Expired(i64),
    CancelReject { gone: bool, cum: i64 },
    AmendAck { cum: i64, qty: i64 },
    AmendReject { gone: bool, cum: i64 },
    OutcomeUnknown,
    FeeOnly,
    Query { status: u8, cum: i64, qty: i64 },
}

/// One order of the model.
#[derive(Copy, Clone, Debug)]
pub struct Order {
    pub st: St,
    pub prev: St,
    pub qty: i64,
    pub filled: i64,
    pub pending_qty: i64,
    pub cancel_sent: bool,
    pub amend_sent: bool,
}

impl Order {
    pub fn new(qty: i64) -> Order {
        Order {
            st: St::PendingNew,
            prev: St::PendingNew,
            qty,
            filled: 0,
            pending_qty: 0,
            cancel_sent: false,
            amend_sent: false,
        }
    }

    fn to(&mut self, s: St) {
        if s == St::Unknown && self.st != St::Unknown {
            self.prev = self.st;
        }
        self.st = s;
        if !matches!(s, St::PendingCancel | St::Unknown) {
            self.cancel_sent = false;
        }
        if !matches!(s, St::PendingAmend | St::PendingCancel | St::Unknown) {
            self.amend_sent = false;
        }
    }

    fn inc(&mut self, cum: i64) -> bool {
        if cum > self.filled {
            self.filled = cum;
            true
        } else {
            false
        }
    }

    pub fn cancel(&mut self) -> bool {
        if matches!(self.st, St::Live | St::PendingNew | St::PendingAmend) && !self.cancel_sent {
            self.cancel_sent = true;
            self.to(St::PendingCancel);
            true
        } else {
            false
        }
    }

    pub fn amend(&mut self, qty: i64, venue_amend: bool) -> bool {
        if self.st == St::Live && venue_amend {
            self.pending_qty = qty;
            self.amend_sent = true;
            self.to(St::PendingAmend);
            true
        } else {
            false
        }
    }

    /// `SendReport{Place}`: 0 Written, 1 WriteError, 2 not written.
    pub fn send_place(&mut self, kind: u8) {
        if self.st != St::PendingNew {
            return;
        }
        match kind {
            1 => self.to(St::Unknown),
            2 => self.to(St::Rejected),
            _ => {}
        }
    }

    /// Order connection Down: Pending* → Unknown.
    pub fn conn_down(&mut self) {
        if matches!(self.st, St::PendingNew | St::PendingCancel | St::PendingAmend) {
            self.to(St::Unknown);
        }
    }

    fn gone(&mut self) {
        let s = if self.filled >= self.qty {
            St::Filled
        } else {
            St::Unknown
        };
        self.to(s);
    }

    pub fn rep(&mut self, r: Rep) {
        use St::*;
        let st = self.st;
        match r {
            Rep::Ack => {
                if matches!(st, PendingNew | Unknown) {
                    self.to(Live);
                }
            }
            Rep::Reject => {
                if matches!(st, PendingNew | Unknown) {
                    self.to(Rejected);
                }
            }
            Rep::Fill(cum) => {
                if !self.inc(cum) {
                    return;
                }
                let s = if cum >= self.qty {
                    Filled
                } else {
                    match st {
                        PendingNew | Live => Live,
                        PendingCancel | PendingAmend => st,
                        Unknown => {
                            if matches!(self.prev, PendingCancel | PendingAmend) {
                                self.prev
                            } else {
                                Live
                            }
                        }
                        _ => st,
                    }
                };
                if s != st {
                    self.to(s);
                }
            }
            Rep::CancelAck(cum) | Rep::Expired(cum) => {
                if st.terminal() {
                    if cum > self.filled {
                        self.rep(Rep::Fill(cum));
                    }
                    return;
                }
                self.inc(cum);
                let s = if self.filled >= self.qty {
                    Filled
                } else {
                    Cancelled
                };
                self.to(s);
            }
            Rep::CancelReject { gone, cum } => match st {
                PendingCancel | PendingAmend if gone => {
                    self.inc(cum);
                    self.gone();
                }
                PendingCancel => {
                    self.inc(cum);
                    self.to(Live);
                }
                Unknown => {}
                PendingNew | Live | PendingAmend => self.anomaly_fill(cum),
                _ => {}
            },
            Rep::AmendAck { cum, qty } => match st {
                PendingAmend => {
                    self.inc(cum);
                    self.qty = self.pending_qty.max(self.filled);
                    let s = if self.filled >= self.qty { Filled } else { Live };
                    self.to(s);
                }
                PendingCancel if self.amend_sent => {
                    self.inc(cum);
                    self.qty = self.pending_qty.max(self.filled);
                    self.amend_sent = false;
                }
                Unknown => {
                    self.inc(cum);
                    if qty > 0 {
                        self.qty = qty.max(self.filled);
                    }
                    self.to(Live);
                }
                PendingNew | Live | PendingCancel => self.anomaly_fill(cum),
                _ => {}
            },
            Rep::AmendReject { gone, cum } => match st {
                PendingAmend if gone => {
                    self.inc(cum);
                    self.gone();
                }
                PendingAmend => {
                    self.inc(cum);
                    self.to(Live);
                }
                PendingCancel if self.amend_sent => {
                    self.inc(cum);
                    self.amend_sent = false;
                }
                Unknown => {}
                PendingNew | Live | PendingCancel => self.anomaly_fill(cum),
                _ => {}
            },
            Rep::OutcomeUnknown => {
                if matches!(st, PendingNew | PendingCancel | PendingAmend) {
                    self.to(Unknown);
                }
            }
            Rep::FeeOnly => {}
            Rep::Query { status, cum, qty } => {
                if st.terminal() {
                    self.inc(cum);
                    return;
                }
                if status == 7 {
                    if st != Unknown {
                        self.to(Unknown);
                    }
                    return;
                }
                let s = match status {
                    1 | 2 => Live,
                    3 => Filled,
                    4 | 6 => Cancelled,
                    _ => Rejected,
                };
                if s == Live && qty > 0 {
                    self.qty = qty.max(self.filled);
                }
                self.inc(cum);
                if s != st {
                    self.to(s);
                }
            }
        }
    }

    /// "异常 Note" cell with a fill increment: the PartialFill/Fill row applies.
    fn anomaly_fill(&mut self, cum: i64) {
        if cum > self.filled {
            self.rep(Rep::Fill(cum));
        }
    }
}

// ------------------------------------------------------------------------------- SendReport (B-1)

use hft_event::{GwOp, SendStatus};
use hft_oms::{OrderState, SlotFlags};

/// The parts of a slot the `SendReport` table reads and writes.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SendSlot {
    pub state: OrderState,
    pub sflags: u16,
    pub query_count: u8,
}

/// Expected effect of one `SendReport` on one slot (README "SendReport 处理", table form).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SendEffect {
    pub state: OrderState,
    /// Expected `sflags & flag_mask`.
    pub flags: u16,
    pub flag_mask: u16,
    pub query_count: u8,
    /// `OrderUpdate::Rejected{LOCAL_SEND_FAIL, status}` to the owner.
    pub rejected: bool,
    /// `Note{OmsCancelNotSent}` (one per report).
    pub cancel_not_sent: bool,
    /// A `Query` decision for this slot.
    pub query: bool,
    /// `Note{OmsNeedsRestQuery}`.
    pub needs_rest: bool,
}

const MASK: u16 = SlotFlags::CANCEL_SENT | SlotFlags::AMEND_SENT | SlotFlags::VERIFY;

impl SendEffect {
    pub fn unchanged(s: &SendSlot) -> SendEffect {
        SendEffect {
            state: s.state,
            flags: s.sflags & MASK,
            flag_mask: MASK,
            query_count: s.query_count,
            rejected: false,
            cancel_not_sent: false,
            query: false,
            needs_rest: false,
        }
    }
}

/// The naive table: `(op, status)` → effect on slot `s` (for `CancelAll`, `s` is any slot of the
/// table; only `PendingCancel` slots with `CANCEL_SENT` are in its range).
pub fn send_report_table(s: &SendSlot, op: GwOp, st: SendStatus) -> SendEffect {
    let mut e = SendEffect::unchanged(s);
    let written = st == SendStatus::Written;
    let wr_err = st == SendStatus::WriteError;
    let open = !matches!(
        s.state,
        OrderState::Filled | OrderState::Cancelled | OrderState::Rejected | OrderState::Free
    );
    match op {
        GwOp::Place => {
            if !written && s.state == OrderState::PendingNew {
                e.flag_mask = 0;
                e.flags = 0;
                if wr_err {
                    e.state = OrderState::Unknown;
                    e.query = true;
                } else {
                    e.state = OrderState::Rejected;
                    e.rejected = true;
                }
            }
        }
        GwOp::Cancel | GwOp::Amend => {
            if !written {
                if wr_err {
                    if open {
                        e.flags |= SlotFlags::VERIFY;
                        e.query = true;
                    }
                } else {
                    let f = if op == GwOp::Cancel {
                        SlotFlags::CANCEL_SENT
                    } else {
                        SlotFlags::AMEND_SENT
                    };
                    e.flags &= !f;
                    e.cancel_not_sent = true;
                }
            }
        }
        GwOp::Query => {
            if written {
                e.query_count = s.query_count.saturating_add(1);
            } else if st == SendStatus::QueryUnsupported {
                e.needs_rest = true;
            }
        }
        GwOp::CancelAll => {
            if !written {
                let hit = s.state == OrderState::PendingCancel && s.sflags & SlotFlags::CANCEL_SENT != 0;
                if wr_err {
                    if hit {
                        e.flags |= SlotFlags::VERIFY;
                        e.query = true;
                    }
                } else {
                    if hit {
                        e.flags &= !SlotFlags::CANCEL_SENT;
                    }
                    e.cancel_not_sent = true;
                }
            }
        }
        _ => {}
    }
    e
}
