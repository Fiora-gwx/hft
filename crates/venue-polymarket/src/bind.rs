//! Token binding table of the feed task (README "槽位绑定"): outcome-token id (decimal text, up to
//! 80 digits) ↔ slot `InstId`, plus the per-inst synthetic sequence ids that make the exchange's
//! sequence-less book stream a continuous `L2Batch` chain. Fixed capacity, allocated once by
//! `FeedCtx::with_capacity`; lookups never allocate.

use hft_types::InstId;

/// Longest token id accepted (uint256 in decimal is at most 78 digits).
pub const TOKEN_MAX: usize = 80;

/// Why a binding was refused.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum BindErr {
    /// Empty, longer than `TOKEN_MAX` or not all ASCII digits.
    Token,
    /// Inst 0.
    Inst,
    /// The token is already bound to another inst.
    TokenInUse,
    /// No free entry.
    Full,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct Entry {
    inst: InstId,
    len: u8,
    token: [u8; TOKEN_MAX],
    /// Last synthetic book id handed out for this inst (survives rebinding).
    book_id: u64,
    /// Last synthetic trade id handed out for this inst.
    trade_id: u64,
    /// Exchange time (ms) of the last `book` of this inst; older level changes are stale.
    book_ms: u64,
    /// The token this inst was bound to before its latest rebinding: an unsubscription frame
    /// still names it.
    old_len: u8,
    old: [u8; TOKEN_MAX],
}

const EMPTY: Entry = Entry {
    inst: InstId(0),
    len: 0,
    token: [0; TOKEN_MAX],
    book_id: 0,
    trade_id: 0,
    book_ms: 0,
    old_len: 0,
    old: [0; TOKEN_MAX],
};

/// FNV-1a over the token text.
fn hash(t: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in t {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Feed-task state (contract 25: no refdata cached). Entries are per inst; an inst keeps its
/// entry (and its id counters) when it is rebound to a new token, and an unbound inst keeps its
/// counters with an empty token.
#[derive(Clone, Eq, PartialEq)]
pub struct FeedCtx {
    entries: Box<[Entry]>,
    /// Open-addressed token index into `entries` (`u16::MAX` = empty); rebuilt on every change.
    index: Box<[u16]>,
}

impl core::fmt::Debug for FeedCtx {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FeedCtx")
            .field("capacity", &self.entries.len())
            .finish()
    }
}

impl Default for FeedCtx {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

impl FeedCtx {
    /// Startup: room for `insts` slot insts (clamped to 1..=16384).
    pub fn with_capacity(insts: usize) -> FeedCtx {
        let n = insts.clamp(1, 16_384);
        FeedCtx {
            entries: vec![EMPTY; n].into_boxed_slice(),
            index: vec![u16::MAX; (n * 2).next_power_of_two()].into_boxed_slice(),
        }
    }

    /// Binds `inst` to `token` (warm: startup or a binding command). Rebinding an inst replaces
    /// its token and keeps its id counters.
    pub fn bind(&mut self, inst: InstId, token: &[u8]) -> Result<(), BindErr> {
        if token.is_empty() || token.len() > TOKEN_MAX || !token.iter().all(u8::is_ascii_digit) {
            return Err(BindErr::Token);
        }
        if inst.0 == 0 {
            return Err(BindErr::Inst);
        }
        let other = self.inst_of(token);
        if other.0 != 0 && other != inst {
            return Err(BindErr::TokenInUse);
        }
        let at = match self.entries.iter().position(|e| e.inst == inst) {
            Some(i) => i,
            None => self
                .entries
                .iter()
                .position(|e| e.inst.0 == 0)
                .ok_or(BindErr::Full)?,
        };
        let e = self.entries.get_mut(at).ok_or(BindErr::Full)?;
        if e.inst == inst && e.len != 0 && e.token.get(..usize::from(e.len)) != Some(token) {
            e.old = e.token;
            e.old_len = e.len;
        }
        e.inst = inst;
        e.token = [0; TOKEN_MAX];
        if let Some(d) = e.token.get_mut(..token.len()) {
            d.copy_from_slice(token);
        }
        e.len = token.len() as u8;
        // A new token starts without a book.
        e.book_ms = 0;
        self.reindex();
        Ok(())
    }

    /// Removes the token of `inst` (its counters stay; the token remains its previous one).
    pub fn unbind(&mut self, inst: InstId) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.inst == inst) {
            if e.len != 0 {
                e.old = e.token;
                e.old_len = e.len;
            }
            e.len = 0;
            e.token = [0; TOKEN_MAX];
        }
        self.reindex();
    }

    /// The token an unsubscription of `inst` names: its previous token after a rebinding,
    /// else the current one.
    pub fn unsub_token_of(&self, inst: InstId) -> Option<&[u8]> {
        let e = self.entries.iter().find(|e| e.inst == inst)?;
        if e.old_len != 0 {
            e.old.get(..usize::from(e.old_len))
        } else if e.len != 0 {
            e.token.get(..usize::from(e.len))
        } else {
            None
        }
    }

    /// Binds `inst` to a uint256 wire id (32 bytes, big-endian; all zero = unbind), rendered as
    /// the exchange's decimal token text.
    pub fn bind_wire(&mut self, inst: InstId, id: &[u8; 32]) -> Result<(), BindErr> {
        if *id == [0; 32] {
            if inst.0 == 0 {
                return Err(BindErr::Inst);
            }
            self.unbind(inst);
            return Ok(());
        }
        let mut n = *id;
        let mut digits = [0u8; TOKEN_MAX];
        let mut k = 0usize;
        while n != [0; 32] {
            // n /= 10, remainder = next digit (least significant first).
            let mut rem: u32 = 0;
            for b in n.iter_mut() {
                let cur = (rem << 8) | u32::from(*b);
                *b = (cur / 10) as u8;
                rem = cur % 10;
            }
            let d = digits.get_mut(k).ok_or(BindErr::Token)?;
            *d = b'0' + rem as u8;
            k += 1;
        }
        let text = digits.get_mut(..k).ok_or(BindErr::Token)?;
        text.reverse();
        let mut out = [0u8; TOKEN_MAX];
        let t = out.get_mut(..k).ok_or(BindErr::Token)?;
        t.copy_from_slice(text);
        self.bind(inst, out.get(..k).unwrap_or(&[]))
    }

    fn reindex(&mut self) {
        self.index.iter_mut().for_each(|x| *x = u16::MAX);
        let mask = self.index.len() - 1;
        for (i, e) in self.entries.iter().enumerate() {
            if e.len == 0 {
                continue;
            }
            let mut k = (hash(e.token.get(..usize::from(e.len)).unwrap_or(&[])) as usize) & mask;
            loop {
                match self.index.get_mut(k) {
                    Some(x) if *x == u16::MAX => {
                        *x = i as u16;
                        break;
                    }
                    Some(_) => k = (k + 1) & mask,
                    None => break,
                }
            }
        }
    }

    fn find(&self, token: &[u8]) -> Option<usize> {
        if token.is_empty() || token.len() > TOKEN_MAX {
            return None;
        }
        let mask = self.index.len() - 1;
        let mut k = (hash(token) as usize) & mask;
        for _ in 0..self.index.len() {
            let i = *self.index.get(k)?;
            if i == u16::MAX {
                return None;
            }
            let e = self.entries.get(usize::from(i))?;
            if e.token.get(..usize::from(e.len)) == Some(token) {
                return Some(usize::from(i));
            }
            k = (k + 1) & mask;
        }
        None
    }

    /// Inst bound to `token`; 0 = not bound.
    pub fn inst_of(&self, token: &[u8]) -> InstId {
        self.find(token)
            .and_then(|i| self.entries.get(i))
            .map_or(InstId(0), |e| e.inst)
    }

    /// Token bound to `inst`; `None` = not bound.
    pub fn token_of(&self, inst: InstId) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|e| e.inst == inst && e.len != 0)
            .and_then(|e| e.token.get(..usize::from(e.len)))
    }

    /// Entry index and inst bound to `token` (hot path of the decoder).
    pub(crate) fn lookup(&self, token: &[u8]) -> Option<(usize, InstId)> {
        let i = self.find(token)?;
        Some((i, self.entries.get(i)?.inst))
    }

    /// Next book id of entry `i` for a snapshot: the new id (`snap_id`).
    pub(crate) fn next_snapshot(&mut self, i: usize) -> u64 {
        self.entries.get_mut(i).map_or(0, |e| {
            e.book_id = e.book_id.wrapping_add(1);
            e.book_id
        })
    }

    /// Records the exchange time of entry `i`'s latest `book`.
    pub(crate) fn set_book_ms(&mut self, i: usize, ms: u64) {
        if let Some(e) = self.entries.get_mut(i) {
            e.book_ms = e.book_ms.max(ms);
        }
    }

    /// Whether a level change at `ms` predates entry `i`'s latest `book` (already contained in it).
    pub(crate) fn stale_change(&self, i: usize, ms: u64) -> bool {
        self.entries.get(i).is_some_and(|e| ms < e.book_ms)
    }

    /// Next book ids of entry `i` for an increment: `(prev_id, last_id)`.
    pub(crate) fn next_batch(&mut self, i: usize) -> (u64, u64) {
        self.entries.get_mut(i).map_or((0, 0), |e| {
            let prev = e.book_id;
            e.book_id = e.book_id.wrapping_add(1);
            (prev, e.book_id)
        })
    }

    /// Next trade id of entry `i`.
    pub(crate) fn next_trade(&mut self, i: usize) -> u64 {
        self.entries.get_mut(i).map_or(0, |e| {
            e.trade_id = e.trade_id.wrapping_add(1);
            e.trade_id
        })
    }
}
