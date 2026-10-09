//! `LatencyModel` / `Dist` / `MarginModel` validation, per-account RNG sub-streams and latency
//! sampling (README "配置", "9. 确定性").

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::types::{Dist, LatencyModel, SimCfgErr};

/// `splitmix64` finaliser (the sub-stream seed of an account).
pub(crate) const fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Sub-stream of account `a`: `ChaCha8Rng` seeded with `splitmix64(seed ⊕ a)`.
pub(crate) fn account_rng(seed: u64, a: u8) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(splitmix64(seed ^ u64::from(a)))
}

/// `n ∈ [2, 64]`, `q_bp` strictly increasing from 0 to 10 000, `us` non-decreasing.
pub(crate) fn check_dist(d: &Dist) -> Result<(), SimCfgErr> {
    let n = usize::from(d.n);
    if !(2..=64).contains(&n) {
        return Err(SimCfgErr::BadDist);
    }
    let q = d.q_bp.get(..n).ok_or(SimCfgErr::BadDist)?;
    let u = d.us.get(..n).ok_or(SimCfgErr::BadDist)?;
    if q.first() != Some(&0) || q.last() != Some(&10_000) {
        return Err(SimCfgErr::BadDist);
    }
    if q.windows(2).any(|w| w[0] >= w[1]) || u.windows(2).any(|w| w[0] > w[1]) {
        return Err(SimCfgErr::BadDist);
    }
    Ok(())
}

pub(crate) fn check_latency(m: &LatencyModel) -> Result<(), SimCfgErr> {
    match m {
        LatencyModel::Fixed { .. } => Ok(()),
        LatencyModel::Empirical { entry, resp } => {
            check_dist(entry)?;
            check_dist(resp)
        }
    }
}

/// One draw from a validated `Dist`, in µs: a uniform position in [0, 10 000) bp (16 extra bits of
/// resolution) interpolated linearly between the surrounding quantiles, rounded down.
fn sample_dist(d: &Dist, rng: &mut ChaCha8Rng) -> u32 {
    let u = rng.next_u64();
    // x in units of bp / 65 536, x < 10 000 × 65 536.
    let x = ((u128::from(u) * 10_000 * 65_536) >> 64) as u64;
    let n = usize::from(d.n).min(64);
    let mut i = 0;
    while i + 2 < n {
        let next = u64::from(d.q_bp.get(i + 1).copied().unwrap_or(10_000)) << 16;
        if x < next {
            break;
        }
        i += 1;
    }
    let q0 = u64::from(d.q_bp.get(i).copied().unwrap_or(0)) << 16;
    let q1 = u64::from(d.q_bp.get(i + 1).copied().unwrap_or(10_000)) << 16;
    let u0 = u64::from(d.us.get(i).copied().unwrap_or(0));
    let u1 = u64::from(d.us.get(i + 1).copied().unwrap_or(0));
    let span = q1.saturating_sub(q0).max(1);
    let off = x.saturating_sub(q0).min(span);
    let v = u0 + (u1.saturating_sub(u0) * off) / span;
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// Which leg of the latency model.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum Leg {
    Entry,
    Resp,
}

/// Latency draw in µs; `Fixed` consumes no random numbers.
pub(crate) fn sample_us(m: &LatencyModel, leg: Leg, rng: &mut ChaCha8Rng) -> u32 {
    match (m, leg) {
        (LatencyModel::Fixed { entry_us, .. }, Leg::Entry) => *entry_us,
        (LatencyModel::Fixed { resp_us, .. }, Leg::Resp) => *resp_us,
        (LatencyModel::Empirical { entry, .. }, Leg::Entry) => sample_dist(entry, rng),
        (LatencyModel::Empirical { resp, .. }, Leg::Resp) => sample_dist(resp, rng),
    }
}
