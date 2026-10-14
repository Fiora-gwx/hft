//! `ConnArena` and the ownership tokens `Handoff`/`RetireMsg` (README "类型总览", "调用约束", §10).
//!
//! The arena is built once at startup (all buffers allocated and pre-touched) and leaked to
//! `'static`. A slot is owned by exactly one thread at a time: the `Connector` (warm) while it
//! builds, then whoever holds the slot's `Handoff` (moved through an `hft_rt::spsc` ring, hence
//! `Copy`), then the `Connector` again after `RetireMsg` comes back. The ring transfer gives the
//! happens-before edge; a token must be used once (protocol, checked with `debug_assert`).

use std::cell::UnsafeCell;

use hft_event::ConnRole;
use hft_rt::TscClock;
use hft_types::ConnId;

use crate::config::{ConnCfg, NetConfig};
use crate::conn::Conn;
use crate::types::ErrBits;

struct Slot {
    role: ConnRole,
    conn: UnsafeCell<Conn>,
}

/// The pre-allocated connection slots.
pub struct ConnArena {
    slots: Box<[Slot]>,
    clock: TscClock,
}

// SAFETY: a slot's `Conn` is only touched by the thread holding that slot's token (Connector
// before `handoff` and after `reclaim`, the hot pool in between); tokens move through SPSC rings,
// whose release/acquire pair orders the previous owner's writes before the next owner's reads.
unsafe impl Sync for ConnArena {}

impl std::fmt::Debug for ConnArena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnArena")
            .field("slots", &self.slots.len())
            .finish()
    }
}

impl ConnArena {
    /// Startup: `plan` = `(role, slots)`; each slot gets the role's `rx_buf`, `tx_buf` and a
    /// `max_msg` fragment area, written once (pre-touched). `clock` converts TSC deltas.
    pub fn new(plan: &[(ConnRole, u16)], cfg: &NetConfig, clock: TscClock) -> &'static ConnArena {
        let mut v = Vec::new();
        for &(role, n) in plan {
            for _ in 0..n {
                v.push(Slot {
                    role,
                    conn: UnsafeCell::new(Conn::new_free(ConnCfg::from_net(cfg, role), role, clock)),
                });
            }
        }
        Box::leak(Box::new(ConnArena {
            slots: v.into_boxed_slice(),
            clock,
        }))
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn clock(&self) -> TscClock {
        self.clock
    }

    pub(crate) fn role(&self, slot: u16) -> Option<ConnRole> {
        self.slots.get(slot as usize).map(|s| s.role)
    }

    /// # Safety
    /// The caller holds the token of `slot` (it is the slot's only user right now) and does not
    /// create a second live reference to the same slot.
    #[allow(clippy::mut_from_ref)]
    pub(crate) unsafe fn conn_mut(&self, slot: u16) -> Option<&mut Conn> {
        // SAFETY: per the function contract the caller is the slot's sole owner.
        self.slots
            .get(slot as usize)
            .map(|s| unsafe { &mut *s.conn.get() })
    }
}

/// An open connection handed from the `Connector` to a hot thread: arena slot + identity.
#[derive(Copy, Clone)]
pub struct Handoff {
    pub(crate) arena: &'static ConnArena,
    pub(crate) slot: u16,
    pub(crate) id: ConnId,
    pub(crate) role: ConnRole,
}

impl std::fmt::Debug for Handoff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handoff")
            .field("slot", &self.slot)
            .field("id", &self.id)
            .field("role", &self.role)
            .finish()
    }
}

impl Handoff {
    /// A completed scripted TLS connection, exclusively owned by its returned token.
    #[cfg(feature = "test-io")]
    #[doc(hidden)]
    pub fn test_from_conn(conn: Box<Conn>) -> Self {
        let id = conn.id();
        let role = conn.role;
        let arena = Box::leak(Box::new(ConnArena {
            slots: vec![Slot {
                role,
                conn: UnsafeCell::new(*conn),
            }]
            .into_boxed_slice(),
            clock: hft_rt::TscClock::from_hz_milli(1_000_000_000_000, hft_rt::CalibPair::default()),
        }));
        Self {
            arena,
            slot: 0,
            id,
            role,
        }
    }
    /// Arena slot number.
    pub fn slot(&self) -> u16 {
        self.slot
    }

    pub fn id(&self) -> ConnId {
        self.id
    }

    pub fn role(&self) -> ConnRole {
        self.role
    }

    /// The connection, for the token's current holder.
    pub(crate) fn conn(&mut self) -> Option<&mut Conn> {
        // SAFETY: holding the token makes this thread the slot's sole owner (module docs); the
        // returned borrow is tied to `&mut self`.
        unsafe { self.arena.conn_mut(self.slot) }
    }
}

/// A retired connection going back to the warm thread for `shutdown` + `close`.
#[derive(Copy, Clone, Debug)]
pub struct RetireMsg {
    pub(crate) h: Handoff,
    pub why: ErrBits,
}

impl RetireMsg {
    pub fn id(&self) -> ConnId {
        self.h.id
    }

    pub fn slot(&self) -> u16 {
        self.h.slot
    }
}
