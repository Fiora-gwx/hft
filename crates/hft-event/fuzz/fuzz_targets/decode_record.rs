//! EVT-200: any bytes → `decode_record`; on success `classify` and the typed reader of every
//! registered kind never panic. Corpus: EVT-173 and EVT-071 fixtures + mutations.
#![no_main]

use hft_event::*;
use libfuzzer_sys::fuzz_target;

macro_rules! read_fixed {
    ($r:expr, $k:expr; $($t:ty),*) => {
        $(if $k == <$t as Body>::KIND { let _ = $r.read::<$t>(); let _ = $r.view::<$t>(); })*
    };
}

fn evt_200_fuzz_decode_record(data: &[u8]) {
    let Ok(r) = decode_record(data) else { return };
    let k = EventKind(r.hdr.kind);
    if kind_info(k).is_none() {
        return;
    }
    let _ = classify(k, r.body);
    read_fixed!(r, k; Bbo, L2Delta, Trade, Funding, MarkPrice, IndexPrice, Liquidation, OrderDecision, SendReport,
        OrderEvent, PositionReport, BalanceReport, FundingSettle, ExternalFill, ExecDecision, ChainHead, PoolQuote,
        SwapDecision, TxLifecycle, CmdResult, ReconcileReport, Timer, ConnState, LatencySample, Note, HostProfile,
        ClockStatus, SegmentRoll, EngineClock, UtcAnchor);
    let _ = r.var::<AdviceHdr>();
    let _ = r.var::<MarginReportHdr>();
    let _ = r.var::<AccountConfigReportHdr>();
    let _ = r.var::<L2BatchHdr>();
    let logical = r.body.get(16..).unwrap_or(&[]);
    match k {
        EventKind::L2_SNAPSHOT => drop(multi_view::<L2SnapHdr>(logical)),
        EventKind::CONFIG_BLOB => drop(multi_view::<BlobHdr>(logical)),
        EventKind::CONTROL_CMD => drop(multi_view::<CmdHdr>(logical)),
        EventKind::STATE_SNAPSHOT => drop(multi_view::<SnapshotHdr>(logical)),
        EventKind::RAW_RX | EventKind::RAW_TX => drop(multi_view::<RawHdr>(logical)),
        _ => {}
    }
}

fuzz_target!(|data: &[u8]| evt_200_fuzz_decode_record(data));
