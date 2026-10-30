//! `SecHist` (README "节点库"): one value per UTC second, the last write of the second winning,
//! in a ring of `N` seconds (`N` a power of two keeps the slot a mask).

use crate::{ColumnDesc, Events, Feature, FeatureRow};

#[derive(Clone, Debug)]
pub struct SecHist<const N: usize> {
    sec: [i64; N],
    val: [i64; N],
}

impl<const N: usize> Default for SecHist<N> {
    fn default() -> SecHist<N> {
        SecHist {
            sec: [i64::MIN; N],
            val: [0; N],
        }
    }
}

impl<const N: usize> SecHist<N> {
    fn slot(sec: i64) -> usize {
        (sec.rem_euclid(N as i64)) as usize
    }
    /// Handler: value `v` at UTC second `sec`.
    pub fn put(&mut self, sec: i64, v: i64) {
        let k = Self::slot(sec);
        if let (Some(s), Some(x)) = (self.sec.get_mut(k), self.val.get_mut(k)) {
            *s = sec;
            *x = v;
        }
    }
    /// Eval: the value of second `t`, else of `t − 1`, else of `t − 2`.
    pub fn at(&self, t: i64) -> Option<i64> {
        (0..3).find_map(|back| {
            let s = t.saturating_sub(back);
            let k = Self::slot(s);
            (self.sec.get(k) == Some(&s))
                .then(|| self.val.get(k).copied())
                .flatten()
        })
    }
}

impl<const N: usize> Feature for SecHist<N> {
    /// (UTC second, value).
    type Input<'a> = (i64, i64);
    const EVENTS: Events = Events::ANY;
    /// Evals take a second; nothing is exported by itself.
    const COLUMNS: &'static [ColumnDesc] = &[];
    fn on_event(&mut self, x: (i64, i64)) {
        self.put(x.0, x.1);
    }
    fn export(&self, _row: &mut FeatureRow<'_>) {}
}
