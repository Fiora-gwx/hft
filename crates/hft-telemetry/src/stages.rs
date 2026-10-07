//! Time points → segments (latency-budget §2), classification by trigger kind, `sw_rx`
//! diagnostic segments (README "分段(`StageSet`)").

use hft_event::{EventKind, LatencySample};
use serde::Serialize;

use crate::clock::{ClockCal, Domain};

/// Segment names of latency-budget §2 (production and diagnostic; `ws_rtt` is connection-level
/// and not a segment of an event). Names, formulas and semantics are owned by latency-budget.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum Stage {
    ExQueue,
    WireIn,
    FeedOneway,
    KernelRx,
    NicToKernel,
    KernelToUser,
    Ingest,
    FeedHost,
    Decide,
    Egress,
    KernelTx,
    TickToTrade,
    RepToOrder,
    HedgeHost,
    OrderRtt,
    TickToAck,
    LegGap,
}

/// Number of `Stage` values.
pub const N_STAGES: usize = 17;

/// On-chain segments of latency-budget §2.2 (optional extension; not computed by this build).
pub const CHAIN_STAGES: &[&str] = &[
    "chain_decide",
    "sol_decide",
    "chain_signal_to_send",
    "chain_include_blocks",
    "chain_include_seen",
    "leg_gap",
    "exposure",
    "relay_hop",
    "evidence_to_emit",
];

impl Stage {
    pub const ALL: [Stage; N_STAGES] = [
        Stage::ExQueue,
        Stage::WireIn,
        Stage::FeedOneway,
        Stage::KernelRx,
        Stage::NicToKernel,
        Stage::KernelToUser,
        Stage::Ingest,
        Stage::FeedHost,
        Stage::Decide,
        Stage::Egress,
        Stage::KernelTx,
        Stage::TickToTrade,
        Stage::RepToOrder,
        Stage::HedgeHost,
        Stage::OrderRtt,
        Stage::TickToAck,
        Stage::LegGap,
    ];

    /// The latency-budget §2 name.
    pub const fn name(self) -> &'static str {
        match self {
            Stage::ExQueue => "ex_queue",
            Stage::WireIn => "wire_in",
            Stage::FeedOneway => "feed_oneway",
            Stage::KernelRx => "kernel_rx",
            Stage::NicToKernel => "nic_to_kernel",
            Stage::KernelToUser => "kernel_to_user",
            Stage::Ingest => "ingest",
            Stage::FeedHost => "feed_host",
            Stage::Decide => "decide",
            Stage::Egress => "egress",
            Stage::KernelTx => "kernel_tx",
            Stage::TickToTrade => "tick_to_trade",
            Stage::RepToOrder => "rep_to_order",
            Stage::HedgeHost => "hedge_host",
            Stage::OrderRtt => "order_rtt",
            Stage::TickToAck => "tick_to_ack",
            Stage::LegGap => "leg_gap",
        }
    }

    /// Segments with a PHC (or `sw_rx` REALTIME) endpoint: converted by interpolation, dropped
    /// from quantiles when the calibration is not usable (`unsynced_cal`).
    pub const fn cross_domain(self) -> bool {
        matches!(
            self,
            Stage::KernelRx
                | Stage::NicToKernel
                | Stage::KernelToUser
                | Stage::FeedHost
                | Stage::TickToTrade
                | Stage::RepToOrder
                | Stage::HedgeHost
                | Stage::OrderRtt
                | Stage::TickToAck
        )
    }

    /// Segments with an exchange-clock endpoint (trusted only within the offset bound).
    pub const fn exchange_clock(self) -> bool {
        matches!(self, Stage::ExQueue | Stage::WireIn | Stage::FeedOneway)
    }
}

impl Serialize for Stage {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

/// Source of the `hw_rx` point: separate datasets (latency-budget §5).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
pub enum TsSrc {
    Hw,
    Sw,
    Missing,
}

/// Class of the trigger event of a sample (README "按触发事件分类").
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TriggerClass {
    /// A market record (`tick_to_trade`).
    Market,
    /// A report frame `OrderEvent` (`rep_to_order`, `hedge_host`).
    Report,
    /// Timer, command, Advice or unknown: only segments without `hw_rx`.
    Other,
}

/// Market kinds carrying `MarketHdr` that can trigger an order.
pub fn is_market_kind(k: EventKind) -> bool {
    (EventKind::BBO.0..=EventKind::L2_BATCH.0).contains(&k.0)
}

/// Classification of a trigger kind.
pub fn trigger_class(k: EventKind) -> TriggerClass {
    if is_market_kind(k) {
        TriggerClass::Market
    } else if k == EventKind::ORDER_EVENT {
        TriggerClass::Report
    } else {
        TriggerClass::Other
    }
}

/// `LatencySample.src` point code `SW` (hft-event `LatencySrc`: 0 missing, 1 HW, 2 SW, 3 TSC).
const SRC_SW: u32 = 2;
/// `LatencySample.link_role` `Second`.
pub const LINK_SECOND: u8 = 2;
/// `LatencySample.action` `New`.
pub const ACTION_NEW: u8 = 0;

/// All segments of one sample (`None` = not produced), with the per-sample error bound.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct StageSet {
    vals: [Option<i64>; N_STAGES],
    /// Bit `stage as u8`: the segment applies to this sample but an endpoint is missing.
    missing: u32,
    /// latency-budget §2.1 error bound; `u32::MAX` without the next `ClockStatus`.
    pub err_bound_ns: u32,
    /// Calibration usable (next `ClockStatus` present, no `Stepped` end, bound ≤ threshold).
    pub cal_ok: bool,
    /// Offset bound of the two `ClockStatus` (exchange-clock trust is judged by the caller).
    pub ex_offset_ns: u32,
    pub ts_src: TsSrc,
    pub trigger: TriggerClass,
}

impl StageSet {
    fn new(cal: &ClockCal, ts_src: TsSrc, trigger: TriggerClass) -> StageSet {
        let err_bound_ns = cal.err_bound_ns();
        StageSet {
            vals: [None; N_STAGES],
            missing: 0,
            err_bound_ns,
            cal_ok: cal.b.is_some() && !cal.stepped() && err_bound_ns <= cal.max_err_ns,
            ex_offset_ns: cal.ex_offset_ns(),
            ts_src,
            trigger,
        }
    }

    /// Value of a segment.
    pub fn get(&self, s: Stage) -> Option<i64> {
        self.vals[s as usize]
    }

    /// The segment applies but an endpoint is missing (counted as `missing`, never filled with 0).
    pub fn is_missing(&self, s: Stage) -> bool {
        self.missing & (1 << s as u32) != 0
    }

    /// Sets a segment: `Some` value, or `None` = applicable but an endpoint missing.
    pub fn set(&mut self, s: Stage, v: Option<i64>) {
        self.vals[s as usize] = v;
        if v.is_none() {
            self.missing |= 1 << s as u32;
        } else {
            self.missing &= !(1 << s as u32);
        }
    }

    /// Produced segments in `Stage::ALL` order.
    pub fn iter(&self) -> impl Iterator<Item = (Stage, i64)> + '_ {
        Stage::ALL.into_iter().filter_map(|s| self.get(s).map(|v| (s, v)))
    }
}

/// Source code (2 bits) of time point `pt` (0..7) of a sample.
fn src(s: &LatencySample, pt: u32) -> u32 {
    (s.src >> (2 * pt)) & 3
}

/// Time points → segments (README "使用本组件" `Telemetry::stages`). The trigger class comes
/// from `s.trigger_kind` (0 = unknown: only segments without `hw_rx`); `leg_gap` needs the
/// trigger record and is filled by `Telemetry`; `kernel_tx` needs `tx_sw` (optional extension,
/// off) and is never produced.
pub fn stages(s: &LatencySample, cal: &ClockCal) -> StageSet {
    let present = |pt: u32, v: i64| src(s, pt) != 0 && v != 0;
    let ex_ts = present(0, s.ex_ts_ns).then_some(s.ex_ts_ns);
    let ex_send = present(1, s.ex_send_ns).then_some(s.ex_send_ns);
    let hw = present(2, s.hw_rx_ns).then_some(s.hw_rx_ns);
    let rx = present(3, s.rx_tsc.0 as i64).then_some(s.rx_tsc.0);
    let proc = present(4, s.proc_tsc.0 as i64).then_some(s.proc_tsc.0);
    let decided = present(5, s.decided_tsc.0 as i64).then_some(s.decided_tsc.0);
    let sent = present(6, s.sent_tsc.0 as i64).then_some(s.sent_tsc.0);
    let ack = present(7, s.ack_hw_rx_ns).then_some(s.ack_hw_rx_ns);
    let sw = (s.sw_rx_ns != 0).then_some(s.sw_rx_ns);

    let ts_src = match (hw, src(s, 2)) {
        (None, _) => TsSrc::Missing,
        (Some(_), SRC_SW) => TsSrc::Sw,
        _ => TsSrc::Hw,
    };
    // Domain of hw_rx: PHC with HW_TS, REALTIME when it is the kernel software stamp.
    let hw_dom = if ts_src == TsSrc::Sw {
        Domain::Realtime
    } else {
        Domain::Phc
    };
    let ack_dom = if src(s, 7) == SRC_SW {
        Domain::Realtime
    } else {
        Domain::Phc
    };
    let trigger = trigger_class(EventKind(s.trigger_kind));
    let mut st = StageSet::new(cal, ts_src, trigger);

    let tsc = |a: Option<u64>, b: Option<u64>| Some(cal.tsc_ns(a?, b?));
    if trigger != TriggerClass::Other {
        st.set(Stage::ExQueue, ex_send.zip(ex_ts).map(|(s1, t)| s1 - t));
        st.set(Stage::WireIn, hw.zip(ex_send).map(|(h, s1)| h - s1));
        st.set(Stage::FeedOneway, hw.zip(ex_ts).map(|(h, t)| h - t));
        let kernel_rx = hw.zip(rx).map(|(h, r)| cal.to_domain(r, hw_dom) - h);
        st.set(Stage::KernelRx, kernel_rx);
        st.set(
            Stage::NicToKernel,
            hw.zip(sw)
                .map(|(h, w)| cal.convert(w, Domain::Realtime, hw_dom) - h),
        );
        st.set(
            Stage::KernelToUser,
            rx.zip(sw).map(|(r, w)| cal.to_domain(r, Domain::Realtime) - w),
        );
        let ingest = tsc(rx, proc);
        st.set(Stage::Ingest, ingest);
        st.set(Stage::FeedHost, kernel_rx.zip(ingest).map(|(k, i)| k + i));
        // tick_to_trade / rep_to_order / hedge_host = kernel_rx + (sent_tsc − rx_tsc).
        let host = kernel_rx.zip(tsc(rx, sent)).map(|(k, d)| k + d);
        match trigger {
            TriggerClass::Market => st.set(Stage::TickToTrade, host),
            _ => {
                st.set(Stage::RepToOrder, host);
                if s.link_role == LINK_SECOND {
                    st.set(Stage::HedgeHost, host);
                }
            }
        }
        st.set(
            Stage::TickToAck,
            ack.zip(hw)
                .map(|(a, h)| cal.convert(a, ack_dom, Domain::Phc) - cal.convert(h, hw_dom, Domain::Phc)),
        );
    }
    st.set(Stage::Decide, tsc(proc, decided));
    st.set(Stage::Egress, tsc(decided, sent));
    if s.action == ACTION_NEW {
        st.set(
            Stage::OrderRtt,
            ack.zip(sent)
                .map(|(a, t)| cal.convert(a, ack_dom, Domain::Phc) - cal.to_domain(t, Domain::Phc)),
        );
    }
    st
}
