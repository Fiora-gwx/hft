//! `perturb`: reorder within a window, duplicate, drop and delay any report sequence.

use proptest::prelude::*;

/// Perturbation parameters; probabilities in `[0, 1]`, `delay_ns` = upper bound of the extra
/// delivery delay attached to each output item.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Perturb {
    pub reorder_window: usize,
    pub dup_p: f64,
    pub drop_p: f64,
    pub delay_ns: u64,
}

impl Perturb {
    /// No perturbation: the output equals the input item for item with zero delay.
    pub const NONE: Perturb = Perturb {
        reorder_window: 0,
        dup_p: 0.0,
        drop_p: 0.0,
        delay_ns: 0,
    };
}

/// One delivered item: `index` = its position in the input, `dup` = a repeated delivery
/// (immediately after the original), `delay_ns ∈ [0, delay_ns]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery<T> {
    pub item: T,
    pub index: usize,
    pub dup: bool,
    pub delay_ns: u64,
}

/// Reorders: item `i` gets the key `i + u` with `u ∈ [0, reorder_window]`, stable sort by key
/// (so every item moves at most `reorder_window` places); then each item is dropped with `drop_p`,
/// else delivered and, with `dup_p`, delivered once more right after.
pub fn perturb<T: Clone + core::fmt::Debug + 'static>(
    seq: Vec<T>,
    p: Perturb,
) -> impl Strategy<Value = Vec<Delivery<T>>> {
    assert!(
        (0.0..=1.0).contains(&p.dup_p) && (0.0..=1.0).contains(&p.drop_p),
        "perturb: bad p"
    );
    let n = seq.len();
    let per_item = (
        0..=p.reorder_window,
        any::<u32>(),
        any::<u32>(),
        0..=p.delay_ns,
        0..=p.delay_ns,
    );
    proptest::collection::vec(per_item, n..=n).prop_map(move |rolls| {
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| (i + rolls[i].0, i));
        let thr = |prob: f64| (prob * f64::from(u32::MAX)) as u64;
        let mut out = Vec::with_capacity(n * 2);
        for i in order {
            let (_, drop_roll, dup_roll, d0, d1) = rolls[i];
            if u64::from(drop_roll) < thr(p.drop_p) && p.drop_p > 0.0 {
                continue;
            }
            out.push(Delivery {
                item: seq[i].clone(),
                index: i,
                dup: false,
                delay_ns: d0,
            });
            if u64::from(dup_roll) < thr(p.dup_p) && p.dup_p > 0.0 {
                out.push(Delivery {
                    item: seq[i].clone(),
                    index: i,
                    dup: true,
                    delay_ns: d1,
                });
            }
        }
        out
    })
}
