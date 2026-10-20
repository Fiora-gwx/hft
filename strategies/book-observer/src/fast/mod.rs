//! Fast part of `book-observer` (sequencer thread, hot): per group and leg, counts of applied book
//! updates and trades and the last top of book. No intents, no allocation after `new`; snapshots are
//! keyed by `GroupId`.
#![deny(clippy::float_arithmetic)]

use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    Ctx, EvMask, EventRef, GroupTable, Part, RestoreError, SnapshotReader, SnapshotWriter, SubsBuilder, Views,
};

use crate::params::BookObserverParams;

/// Legs per group (the `GroupSpec` maximum).
pub const LEGS: usize = 4;

/// Book (and, with `trades`, trades) of every leg of every group, delivered to the fast part.
pub fn subscribe(groups: &GroupTable, trades: bool, out: &mut SubsBuilder<'_>) {
    let mask = if trades {
        EvMask(EvMask::BOOK.0 | EvMask::TRADE.0)
    } else {
        EvMask::BOOK
    };
    for (gix, g) in groups.iter() {
        for l in g.legs.iter().take(usize::from(g.n_legs).min(LEGS)) {
            let _ = out.market(l.inst, gix, mask, Part::Fast);
        }
    }
}

/// State of one group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupState {
    pub id: GroupId,
    pub legs: [InstId; LEGS],
    /// `Book` events per leg.
    pub books: [u64; LEGS],
    /// `Trade` events per leg.
    pub trades: [u64; LEGS],
    /// Top of book after the leg's last `Book` event (raw `Px`; 0 = side empty or none yet).
    pub bid: [i64; LEGS],
    pub ask: [i64; LEGS],
}

impl GroupState {
    fn of(g: &GroupSpec) -> GroupState {
        let mut legs = [INST_NONE; LEGS];
        for (d, l) in legs.iter_mut().zip(g.legs.iter().take(usize::from(g.n_legs))) {
            *d = l.inst;
        }
        GroupState {
            id: g.id,
            legs,
            books: [0; LEGS],
            trades: [0; LEGS],
            bid: [0; LEGS],
            ask: [0; LEGS],
        }
    }

    fn leg(&self, inst: InstId) -> Option<usize> {
        if inst == INST_NONE {
            return None;
        }
        self.legs.iter().position(|l| *l == inst)
    }
}

/// The fast part's state.
#[derive(Debug)]
pub struct Fast {
    /// One entry per gix; capacity `GroupTable::capacity()` (no reallocation on parameter changes).
    pub groups: Vec<GroupState>,
    /// Previous states during `on_params` (same capacity, allocated in `new`).
    scratch: Vec<GroupState>,
    pub trades: bool,
}

impl Fast {
    pub fn new(p: &BookObserverParams, groups: &GroupTable) -> Fast {
        let mut f = Fast {
            groups: Vec::with_capacity(usize::from(groups.capacity())),
            scratch: Vec::with_capacity(usize::from(groups.capacity())),
            trades: true,
        };
        f.on_params(p, groups);
        f
    }

    /// New parameters: groups with a new `GroupId` or a changed leg set start empty, the others keep
    /// their state (SDK "参数变化时的 group 重置").
    pub fn on_params(&mut self, p: &BookObserverParams, groups: &GroupTable) {
        self.trades = p.at(0).trades;
        self.scratch.clear();
        self.scratch.extend_from_slice(&self.groups);
        self.groups.clear();
        for (_, g) in groups.iter() {
            let fresh = GroupState::of(g);
            let kept = self
                .scratch
                .iter()
                .find(|s| s.id == g.id && s.legs == fresh.legs)
                .copied()
                .unwrap_or(fresh);
            self.groups.push(kept);
        }
    }

    pub fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, _ctx: &mut Ctx<'_>) {
        match *ev {
            EventRef::Book { inst, gix } => {
                if let Some(g) = self.groups.get_mut(usize::from(gix))
                    && let Some(k) = g.leg(inst)
                {
                    let b = v.bbo(inst);
                    g.books[k] += 1;
                    g.bid[k] = b.bid_px.0;
                    g.ask[k] = b.ask_px.0;
                }
            }
            EventRef::Trade { inst, gix, .. } => {
                if let Some(g) = self.groups.get_mut(usize::from(gix))
                    && let Some(k) = g.leg(inst)
                {
                    g.trades[k] += 1;
                }
            }
            _ => {}
        }
    }

    /// `n: u16`, then per group `id: u32` and per leg `books: u64, trades: u64, bid: i64, ask: i64`
    /// in gix order.
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_u16(self.groups.len() as u16);
        for g in &self.groups {
            w.put_u32(g.id.0);
            for k in 0..LEGS {
                w.put_u64(g.books[k]);
                w.put_u64(g.trades[k]);
                w.put_i64(g.bid[k]);
                w.put_i64(g.ask[k]);
            }
        }
    }

    /// Restores by `GroupId`; saved groups that are no longer configured are dropped.
    pub fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let n = r.get_u16()?;
        for _ in 0..n {
            let id = GroupId(r.get_u32()?);
            let mut books = [0u64; LEGS];
            let mut trades = [0u64; LEGS];
            let mut bid = [0i64; LEGS];
            let mut ask = [0i64; LEGS];
            for k in 0..LEGS {
                books[k] = r.get_u64()?;
                trades[k] = r.get_u64()?;
                bid[k] = r.get_i64()?;
                ask[k] = r.get_i64()?;
            }
            if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
                g.books = books;
                g.trades = trades;
                g.bid = bid;
                g.ask = ask;
            }
        }
        Ok(())
    }
}
