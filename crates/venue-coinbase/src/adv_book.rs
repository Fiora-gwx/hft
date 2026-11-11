//! Level-2 book of one product on one connection (README "Advanced Trade：level2 → Bbo"): two
//! price-sorted sides of fixed capacity, enough to keep the best bid and ask exact. No allocation
//! after construction.

use hft_types::{ConnId, InstId};

/// Levels kept per side. The 2026-11-11 capture's full book has 644 bids and 788 offers; a side
/// that overflows drops its worst levels and remembers it (`dropped`).
pub const LEVELS: usize = 1024;

/// One side, best price first.
#[derive(Clone, Debug)]
pub(crate) struct BookSide {
    /// `(px, qty)`, best first: descending for bids, ascending for offers.
    lv: Vec<(i64, i64)>,
    bid: bool,
    /// A level was dropped for lack of room: when the side empties its best is unknown.
    dropped: bool,
}

impl BookSide {
    fn new(bid: bool) -> BookSide {
        BookSide {
            lv: Vec::with_capacity(LEVELS),
            bid,
            dropped: false,
        }
    }

    fn clear(&mut self) {
        self.lv.clear();
        self.dropped = false;
    }

    /// Index of `px`, or where it belongs.
    fn find(&self, px: i64) -> Result<usize, usize> {
        if self.bid {
            self.lv.binary_search_by(|(p, _)| px.cmp(p))
        } else {
            self.lv.binary_search_by(|(p, _)| p.cmp(&px))
        }
    }

    /// Sets level `px` to `qty` (0 deletes it).
    pub(crate) fn set(&mut self, px: i64, qty: i64) {
        match self.find(px) {
            Ok(i) => {
                if qty <= 0 {
                    self.lv.remove(i);
                } else if let Some(l) = self.lv.get_mut(i) {
                    l.1 = qty;
                }
            }
            Err(i) => {
                if qty <= 0 {
                    return;
                }
                if self.lv.len() >= LEVELS {
                    self.dropped = true;
                    if i >= LEVELS {
                        return;
                    }
                    self.lv.pop();
                }
                self.lv.insert(i, (px, qty));
            }
        }
    }

    /// Best `(px, qty)`; `None` when the side is empty.
    pub(crate) fn best(&self) -> Option<(i64, i64)> {
        self.lv.first().copied()
    }

    /// The side is empty although levels were dropped: its real best is not known.
    pub(crate) fn lost(&self) -> bool {
        self.lv.is_empty() && self.dropped
    }
}

/// The book of `(conn, inst)`; `inst == 0` = free.
#[derive(Clone, Debug)]
pub(crate) struct Book {
    pub(crate) conn: ConnId,
    pub(crate) inst: InstId,
    pub(crate) bids: BookSide,
    pub(crate) asks: BookSide,
    /// A snapshot was applied (updates before it are ignored).
    pub(crate) live: bool,
    /// Use stamp (the slot least recently used is recycled).
    pub(crate) used: u64,
    /// Best quote last seen on this connection: `(bid px, bid qty, ask px, ask qty)`.
    pub(crate) top: (i64, i64, i64, i64),
}

impl Book {
    pub(crate) fn new() -> Book {
        Book {
            conn: ConnId(0),
            inst: InstId(0),
            bids: BookSide::new(true),
            asks: BookSide::new(false),
            live: false,
            used: 0,
            top: (0, 0, 0, 0),
        }
    }

    pub(crate) fn reset(&mut self, conn: ConnId, inst: InstId) {
        self.conn = conn;
        self.inst = inst;
        self.bids.clear();
        self.asks.clear();
        self.live = false;
        self.top = (0, 0, 0, 0);
    }

    /// Best bid and ask when both exist, are known and do not cross.
    pub(crate) fn bbo(&self) -> Option<(i64, i64, i64, i64)> {
        if self.bids.lost() || self.asks.lost() {
            return None;
        }
        let (b, a) = (self.bids.best()?, self.asks.best()?);
        (b.0 < a.0).then_some((b.0, b.1, a.0, a.1))
    }
}
