//! `SignedFlow` (README "节点库"): the last `N` prints passed to it (UTC time, inst, quantity
//! signed by the taker side, + buy); the oldest is overwritten. The caller decides which prints
//! count (size filter, inst set).

use hft_strategy_sdk::prelude::{INST_NONE, InstId};

use crate::{ColumnDesc, Events, Feature, FeatureRow};

#[derive(Clone, Debug)]
pub struct SignedFlow<const N: usize> {
    t: [i64; N],
    inst: [InstId; N],
    q: [i64; N],
    n: usize,
}

impl<const N: usize> Default for SignedFlow<N> {
    fn default() -> SignedFlow<N> {
        SignedFlow {
            t: [i64::MIN; N],
            inst: [INST_NONE; N],
            q: [0; N],
            n: 0,
        }
    }
}

impl<const N: usize> SignedFlow<N> {
    /// Handler: a print of `inst` at `utc_ns` with signed quantity `signed_qty` (raw).
    pub fn push(&mut self, utc_ns: i64, inst: InstId, signed_qty: i64) {
        let k = self.n % N;
        if let (Some(t), Some(i), Some(q)) = (self.t.get_mut(k), self.inst.get_mut(k), self.q.get_mut(k)) {
            *t = utc_ns;
            *i = inst;
            *q = signed_qty;
        }
        self.n = self.n.wrapping_add(1);
    }
    /// Eval: net taker flow toward `x` over prints at or after `from_ns`: buys of `x` and sells of
    /// `o` count +, sells of `x` and buys of `o` count − (raw quantity).
    pub fn net(&self, x: InstId, o: InstId, from_ns: i64) -> i128 {
        let mut v = 0i128;
        for ((t, i), q) in self.t.iter().zip(self.inst.iter()).zip(self.q.iter()) {
            if *t < from_ns {
                continue;
            }
            if *i == x {
                v += i128::from(*q);
            } else if *i == o {
                v -= i128::from(*q);
            }
        }
        v
    }
}

impl<const N: usize> Feature for SignedFlow<N> {
    /// (UTC ns, inst, signed raw quantity).
    type Input<'a> = (i64, InstId, i64);
    const EVENTS: Events = Events::TRADE;
    /// Evals take a pair and a window; nothing is exported by itself.
    const COLUMNS: &'static [ColumnDesc] = &[];
    fn on_event(&mut self, x: (i64, InstId, i64)) {
        self.push(x.0, x.1, x.2);
    }
    fn export(&self, _row: &mut FeatureRow<'_>) {}
}
