//! The calibration table (`SimCalib`, README "校准表"): the simulated venue's behaviour options
//! and the latency-condition thresholds, as JSON. Also reads the research `latc.json` form
//! (`{"thr": [...], "pools": {"ack|l|m|ph": [s, …], "ack|l": …, "cxl": …}}`, seconds).

use std::collections::BTreeMap;

use hft_sim::{FeeModel, LatPools, MakerFill, Pool, PoolLeg, TakerDelay, VenueModel};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::venue::{CondCfg, SimParams};

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TakerJson {
    base_us: u32,
    #[serde(default)]
    slow_us: u32,
    #[serde(default)]
    slow_ppm: u32,
    #[serde(default)]
    resp_us: u32,
}

/// The JSON fields; all optional (absent = the option off / the current value kept).
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct CalibJson {
    maker_fill: Option<String>,
    tombstone: Option<bool>,
    /// `[entry_us, resp_us]`.
    cancel_us: Option<[u32; 2]>,
    taker: Option<TakerJson>,
    /// Binary taker fee coefficient (0.07 = 7 %).
    fee_k: Option<f64>,
    gtd_ms: Option<u32>,
    fill_resp_us: Option<u32>,
    split_us: Option<u32>,
    min_n: Option<u16>,
    /// Venue-load thresholds (minute median of per-second md-lag p90), ms; `load` = number of
    /// thresholds below the value.
    thr: Option<Vec<f64>>,
    /// Window and opening-phase lengths (s) of the phase bucket.
    window_s: Option<u32>,
    phase_s: Option<u32>,
    /// `"leg|load|move|phase"`, `"leg|load"` or `"leg"` (leg `ack` = place, `cxl` = cancel,
    /// `take` = taker) → samples in seconds.
    pools: Option<BTreeMap<String, Vec<f64>>>,
    /// Provenance written by `replayer calib` (not interpreted).
    #[allow(dead_code)]
    meta: Option<serde_json::Value>,
}

fn leg_of(s: &str) -> Option<PoolLeg> {
    match s {
        "ack" | "place" => Some(PoolLeg::Place),
        "cxl" | "cancel" => Some(PoolLeg::Cancel),
        "take" | "taker" => Some(PoolLeg::Taker),
        _ => None,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn secs_us(v: f64) -> u32 {
    (v * 1e6).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Applies the calibration table `text` to `p`; returns its SHA-256 (hex).
pub fn apply(text: &[u8], p: &mut SimParams) -> Result<String, String> {
    let c: CalibJson = serde_json::from_slice(text).map_err(|e| format!("calibration: {e}"))?;
    let vm: &mut VenueModel = &mut p.venue;
    if let Some(m) = &c.maker_fill {
        p.maker_fill = match m.as_str() {
            "queue_cap" => MakerFill::QueueCap,
            "queue" => MakerFill::Queue,
            "through" => MakerFill::TradeThrough,
            _ => return Err(format!("calibration: maker_fill {m}")),
        };
    }
    if let Some(t) = c.tombstone {
        vm.tombstone = t;
    }
    if let Some([e, r]) = c.cancel_us {
        vm.cancel_us = Some((e, r));
    }
    if let Some(t) = &c.taker {
        vm.taker = Some(TakerDelay {
            base_us: t.base_us,
            slow_us: t.slow_us,
            slow_ppm: t.slow_ppm,
            resp_us: t.resp_us,
        });
    }
    if let Some(k) = c.fee_k {
        if !(0.0..=1.0).contains(&k) {
            return Err(format!("calibration: fee_k {k}"));
        }
        #[allow(clippy::cast_possible_truncation)]
        let k_e9 = (k * 1e9).round() as i64;
        vm.fee = FeeModel::BinaryTaker { k_e9 };
    }
    if let Some(g) = c.gtd_ms {
        vm.gtd_ms = g;
    }
    if let Some(f) = c.fill_resp_us {
        vm.fill_resp_us = Some(f);
    }
    if let Some(pools) = &c.pools {
        let mut out = Vec::new();
        for (k, v) in pools {
            let mut it = k.split('|');
            let leg = it.next().and_then(leg_of);
            let Some(leg) = leg else {
                continue;
            };
            let num = |x: Option<&str>| -> Result<Option<u8>, String> {
                x.map(|s| s.parse::<u8>().map_err(|_| format!("calibration: pool key {k}")))
                    .transpose()
            };
            let (load, mv, phase) = (num(it.next())?, num(it.next())?, num(it.next())?);
            out.push(Pool {
                leg,
                load,
                mv,
                phase,
                us: v.iter().map(|s| secs_us(*s)).collect(),
            });
        }
        vm.pools = Some(LatPools {
            split_us: c.split_us.unwrap_or(5_000),
            min_n: c.min_n.unwrap_or(20),
            pools: out,
        });
    }
    let cond = &mut p.cond;
    if let Some(t) = &c.thr {
        #[allow(clippy::cast_possible_truncation)]
        let ns: Vec<i64> = t.iter().map(|ms| (ms * 1e6).round() as i64).collect();
        cond.load_thr_ns = ns;
    }
    if let Some(w) = c.window_s {
        cond.window_s = w.max(1);
    }
    if let Some(ph) = c.phase_s {
        cond.phase_s = ph;
    }
    let _: &CondCfg = cond;
    Ok(crate::variants::hex(&Sha256::digest(text)))
}
