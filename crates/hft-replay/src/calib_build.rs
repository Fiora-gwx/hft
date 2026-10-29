//! `replayer calib` (README "校准表的生成"): the conditional latency pools of one account from a
//! live journal — decision → Ack of new orders and decision → CancelAck of cancels, each labelled
//! with the latency condition at the decision (load, move, phase).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hft_event::{EventKind, OrderAction, OrderDecision, OrderEvent, OrderEventKind};
use hft_sequencer::read_body;
use hft_sequencer::types::read_body_plain;
use hft_types::AccountId;
use serde_json::json;

use crate::source::Source;

/// Inputs of `build`.
#[derive(Clone, Debug)]
pub struct CalibOpts {
    pub files: Vec<PathBuf>,
    pub account: AccountId,
    pub reassembly: usize,
    pub window_s: u32,
    pub phase_s: u32,
}

/// A decision waiting for its report: `(record tsc, previous-minute load value, move, phase)`.
type Pending = (u64, Option<i64>, u8, u8);

/// One latency sample before the load bucket is known.
struct Sample {
    leg: &'static str,
    /// Minute median of per-second md-lag p90 of the previous minute (ns; None = none yet).
    load_v: Option<i64>,
    mv: u8,
    phase: u8,
    s: f64,
}

#[derive(Default)]
struct Lag {
    l2_venues: u64,
    sec: i64,
    lags: Vec<i64>,
    minute: i64,
    p90s: Vec<i64>,
    prev_minute: Option<i64>,
    minutes: Vec<i64>,
    ref_inst: Option<u32>,
    ref_hist: std::collections::VecDeque<(u64, i128)>,
}

/// Builds the calibration table (JSON value; `calib::apply` reads it).
pub fn build(o: &CalibOpts) -> Result<serde_json::Value, String> {
    let mut src = Source::open(&o.files, o.reassembly).map_err(|e| format!("journal: {e}"))?;
    let mut lag = Lag::default();
    let mut place: BTreeMap<(u32, u32), Pending> = BTreeMap::new();
    let mut cancel: BTreeMap<(u32, u32), Pending> = BTreeMap::new();
    let mut samples: Vec<Sample> = Vec::new();
    let mut first_seq = 0u64;
    let mut last_seq = 0u64;
    while let Some(rec) = src.next_rec().map_err(|e| format!("journal: {e}"))? {
        let r = rec.r;
        if first_seq == 0 {
            first_seq = r.seq;
        }
        last_seq = r.seq;
        let Some(clk) = src.segs.get(rec.seg).copied() else {
            continue;
        };
        let hz = clk.tsc_hz_milli.max(1);
        let one_s = hz / 1_000;
        let utc = |t: u64| -> i64 {
            let d = i128::from(t) - i128::from(clk.calib_tsc);
            clk.calib_realtime_ns + (d * 1_000_000_000_000 / i128::from(hz)) as i64
        };
        match r.kind {
            EventKind::L2_BATCH | EventKind::L2_SNAPSHOT | EventKind::TRADE | EventKind::BBO => {
                let Some(h) = read_body_plain::<hft_event::MarketHdr>(&r.body) else {
                    continue;
                };
                let vbit = 1u64 << (h.venue.0 & 63);
                if r.kind != EventKind::BBO {
                    if r.kind != EventKind::TRADE {
                        lag.l2_venues |= vbit;
                    }
                    if lag.l2_venues & vbit == 0 || h.sw_rx_ns <= 0 || h.ex_ts_ns <= 0 {
                        continue;
                    }
                    let sec = h.sw_rx_ns.div_euclid(1_000_000_000);
                    if sec != lag.sec {
                        if !lag.lags.is_empty() {
                            lag.lags.sort_unstable();
                            let k = (lag.lags.len() * 9 / 10).min(lag.lags.len() - 1);
                            lag.p90s.push(lag.lags[k]);
                            lag.lags.clear();
                        }
                        let minute = sec.div_euclid(60);
                        if minute != lag.minute {
                            if !lag.p90s.is_empty() {
                                lag.p90s.sort_unstable();
                                let med = lag.p90s[lag.p90s.len() / 2];
                                lag.prev_minute = Some(med);
                                lag.minutes.push(med);
                                lag.p90s.clear();
                            }
                            lag.minute = minute;
                        }
                        lag.sec = sec;
                    }
                    lag.lags.push(h.sw_rx_ns - h.ex_ts_ns);
                } else if lag.l2_venues & vbit == 0 {
                    let Some(b) = read_body_plain::<hft_event::Bbo>(&r.body) else {
                        continue;
                    };
                    if lag.ref_inst.is_none() {
                        lag.ref_inst = Some(b.h.inst.0);
                    }
                    if lag.ref_inst == Some(b.h.inst.0) && b.bid_px.0 > 0 && b.ask_px.0 > 0 {
                        lag.ref_hist
                            .push_back((r.tsc, i128::from(b.bid_px.0) + i128::from(b.ask_px.0)));
                        while lag
                            .ref_hist
                            .front()
                            .is_some_and(|x| r.tsc.saturating_sub(x.0) > 2 * one_s)
                        {
                            lag.ref_hist.pop_front();
                        }
                    }
                }
            }
            EventKind::ORDER_DECISION => {
                let Some(d) = read_body::<OrderDecision>(&r.body) else {
                    continue;
                };
                if d.account != o.account || d.reject != 0 {
                    continue;
                }
                let mv = match (
                    lag.ref_hist.iter().find(|x| r.tsc.saturating_sub(x.0) <= one_s),
                    lag.ref_hist.back(),
                ) {
                    (Some(a), Some(b)) if a.1 > 0 => {
                        let dd = (b.1 - a.1).abs();
                        if dd * 40_000 <= a.1 {
                            0
                        } else if dd * 10_000 <= a.1 {
                            1
                        } else {
                            2
                        }
                    }
                    _ => 0,
                };
                let s = utc(r.tsc).div_euclid(1_000_000_000);
                let phase = u8::from(s.rem_euclid(i64::from(o.window_s.max(1))) >= i64::from(o.phase_s));
                let v = (r.tsc, lag.prev_minute, mv, phase);
                match d.action {
                    OrderAction::New => {
                        place.insert((d.slot.0, d.r#gen), v);
                    }
                    OrderAction::Cancel => {
                        cancel.entry((d.target_slot.0, d.r#gen)).or_insert(v);
                    }
                    _ => {}
                }
            }
            EventKind::ORDER_EVENT => {
                let Some(e) = read_body::<OrderEvent>(&r.body) else {
                    continue;
                };
                if e.account != o.account {
                    continue;
                }
                let (leg, m) = match e.what {
                    OrderEventKind::Ack => ("ack", &mut place),
                    OrderEventKind::CancelAck => ("cxl", &mut cancel),
                    _ => continue,
                };
                if let Some((t0, load_v, mv, phase)) = m.remove(&(e.slot.0, e.r#gen)) {
                    let dt = r.tsc.saturating_sub(t0);
                    #[allow(clippy::cast_precision_loss)]
                    let s = (u128::from(dt) * 1_000_000_000_000 / u128::from(hz)) as f64 / 1e9;
                    samples.push(Sample {
                        leg,
                        load_v,
                        mv,
                        phase,
                        s,
                    });
                }
            }
            _ => {}
        }
    }
    // Load thresholds: terciles of the observed minute values.
    let mut mins = lag.minutes.clone();
    mins.sort_unstable();
    let thr: Vec<i64> = if mins.len() >= 3 {
        vec![mins[mins.len() / 3], mins[2 * mins.len() / 3]]
    } else {
        Vec::new()
    };
    let mut pools: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for x in &samples {
        let load = x.load_v.map_or(1, |v| thr.iter().filter(|t| v > **t).count());
        for k in [
            format!("{}|{}|{}|{}", x.leg, load, x.mv, x.phase),
            format!("{}|{}", x.leg, load),
            x.leg.to_string(),
        ] {
            pools.entry(k).or_default().push(x.s);
        }
    }
    let count = |leg: &str| samples.iter().filter(|x| x.leg == leg).count();
    #[allow(clippy::cast_precision_loss)]
    let thr_ms: Vec<f64> = thr.iter().map(|v| *v as f64 / 1e6).collect();
    Ok(json!({
        "split_us": 5000,
        "min_n": 20,
        "thr": thr_ms,
        "window_s": o.window_s,
        "phase_s": o.phase_s,
        "pools": pools,
        "meta": {
            "tool": "replayer calib",
            "journal": o.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "account": o.account.0,
            "seq_range": [first_seq, last_seq],
            "samples": {"ack": count("ack"), "cxl": count("cxl")},
            "minutes": lag.minutes.len(),
        },
    }))
}
