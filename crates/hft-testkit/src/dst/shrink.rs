//! `shrink` (README "DST 的世界模型" 7): delete fault-plan items, rerun, keep a deletion when the
//! run still violates the same invariant; chunks first (halves, quarters, …), then single items,
//! until nothing more can be deleted or the wall-clock budget is spent. The budget only bounds how
//! far shrinking gets; every rerun is itself deterministic.

use std::time::Instant;

use crate::dst::run::run;
use crate::dst::sut::Sut;
use crate::dst::{DstConfig, DstFailure, DstOutcome, FaultPlan};

/// Smallest plan found within `budget_s` seconds that still fails with `fail.invariant`;
/// `budget_s = 0` returns `fail` unchanged.
pub fn shrink<E: Sut>(cfg: &DstConfig, fail: &DstFailure, budget_s: u32) -> DstFailure {
    if budget_s == 0 {
        return fail.clone();
    }
    let start = Instant::now();
    let over = || start.elapsed().as_secs() >= u64::from(budget_s);
    let mut best = fail.clone();
    let mut plan = fail.fault_schedule.clone();
    let mut c = cfg.clone();
    c.rerun = crate::dst::RerunPolicy::Never;
    let mut still_fails = |p: &FaultPlan, best: &mut DstFailure| -> bool {
        c.plan = Some(p.clone());
        c.steps = cfg.steps;
        match run::<E>(&c) {
            DstOutcome::Fail(f) if f.invariant == fail.invariant => {
                *best = *f;
                true
            }
            _ => false,
        }
    };
    let mut chunk = plan.len().div_ceil(2).max(1);
    while chunk >= 1 && !over() {
        let mut i = 0;
        let mut removed_any = false;
        while i < plan.len() && !over() {
            let end = (i + chunk).min(plan.len());
            let mut cand = plan.clone();
            cand.items.drain(i..end);
            if still_fails(&cand, &mut best) {
                plan = cand;
                removed_any = true;
            } else {
                i = end;
            }
        }
        if chunk == 1 && !removed_any {
            break;
        }
        if !removed_any {
            chunk /= 2;
        }
        if chunk == 0 {
            break;
        }
    }
    best.fault_schedule = plan;
    best
}
