//! Cross-connection de-duplication of the feed task (README "多连接去重"): every market message has
//! a key (event type, exchange time and the exchange's `hash` / `transaction_hash` fields), and the
//! table counts how often each connection has delivered it. A connection's n-th delivery of a key
//! is new only when no other connection has delivered that key n times, so two identical fills on
//! one connection stay two events while the same fill on two connections is one.
//!
//! Two fixed tables (current and previous generation), open addressing, no allocation after
//! `new`. When the current one has taken `FILL` keys it becomes the previous one and the old
//! previous one is cleared and reused, so a key is remembered for at least `FILL` later keys.

/// Slots per table (power of two).
pub(crate) const SLOTS: usize = 1 << 15;
/// Keys inserted per generation (load ≤ 1/2).
pub(crate) const FILL: u32 = 1 << 14;
/// Connections tracked per key; further connections count as duplicates.
const CONNS: usize = 4;
const NO_CONN: u8 = u8::MAX;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct Slot {
    /// 0 = empty (keys are forced odd).
    key: u64,
    conn: [u8; CONNS],
    n: [u8; CONNS],
}

const EMPTY: Slot = Slot {
    key: 0,
    conn: [NO_CONN; CONNS],
    n: [0; CONNS],
};

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct Seen {
    cur: Box<[Slot]>,
    prev: Box<[Slot]>,
    fill: u32,
}

/// FNV-1a key builder.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Key(u64);

impl Key {
    pub(crate) fn new(tag: u8) -> Key {
        Key(0xcbf2_9ce4_8422_2325).byte(tag)
    }

    fn byte(self, b: u8) -> Key {
        Key((self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3))
    }

    pub(crate) fn bytes(mut self, s: &[u8]) -> Key {
        for &b in s {
            self = self.byte(b);
        }
        // Field separator, so ("ab", "c") and ("a", "bc") differ.
        self.byte(0xff)
    }

    pub(crate) fn num(mut self, v: u64) -> Key {
        for b in v.to_le_bytes() {
            self = self.byte(b);
        }
        self
    }

    pub(crate) fn done(self) -> u64 {
        self.0 | 1
    }
}

impl Seen {
    pub(crate) fn new() -> Seen {
        Seen {
            cur: vec![EMPTY; SLOTS].into_boxed_slice(),
            prev: vec![EMPTY; SLOTS].into_boxed_slice(),
            fill: 0,
        }
    }

    /// Position of `key` in table `t`, or the empty slot where it would go (`None`: full, which
    /// the fill limit rules out).
    fn find(t: &[Slot], key: u64) -> Option<(usize, bool)> {
        let mask = SLOTS - 1;
        let mut i = (key as usize) & mask;
        for _ in 0..SLOTS {
            let s = t.get(i)?;
            if s.key == key {
                return Some((i, true));
            }
            if s.key == 0 {
                return Some((i, false));
            }
            i = (i + 1) & mask;
        }
        None
    }

    /// Records one delivery of `key` by connection `conn`; `true` = new (first delivery of this
    /// occurrence), `false` = another connection delivered it already.
    pub(crate) fn note(&mut self, key: u64, conn: u8) -> bool {
        let Some((i, hit)) = Self::find(&self.cur, key) else {
            return true;
        };
        let s = if hit {
            self.cur.get_mut(i)
        } else if let Some((j, true)) = Self::find(&self.prev, key) {
            self.prev.get_mut(j)
        } else {
            if self.fill >= FILL {
                self.rotate();
                return self.note(key, conn);
            }
            self.fill += 1;
            if let Some(s) = self.cur.get_mut(i) {
                *s = Slot {
                    key,
                    conn: [conn, NO_CONN, NO_CONN, NO_CONN],
                    n: [1, 0, 0, 0],
                };
            }
            return true;
        };
        let Some(s) = s else { return true };
        let at = match s.conn.iter().position(|&c| c == conn) {
            Some(k) => k,
            None => match s.conn.iter().position(|&c| c == NO_CONN) {
                Some(k) => {
                    if let Some(c) = s.conn.get_mut(k) {
                        *c = conn;
                    }
                    k
                }
                None => return false,
            },
        };
        let mine = match s.n.get_mut(at) {
            Some(n) => {
                *n = n.saturating_add(1);
                *n
            }
            None => return false,
        };
        let others =
            s.n.iter()
                .enumerate()
                .filter(|&(k, _)| k != at)
                .map(|(_, &n)| n)
                .max()
                .unwrap_or(0);
        mine > others
    }

    /// The current table becomes the previous one; the old previous one is cleared and reused.
    fn rotate(&mut self) {
        core::mem::swap(&mut self.cur, &mut self.prev);
        self.cur.iter_mut().for_each(|s| *s = EMPTY);
        self.fill = 0;
    }
}
