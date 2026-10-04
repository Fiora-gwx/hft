//! Code tables and bit sets (EVT-054, EVT-055, EVT-061, EVT-073, EVT-074).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_event::*;

fn ty_desc(fields: &'static [FieldDesc], name: &str) -> &'static EnumDesc {
    match field(fields, name).ty {
        FieldTy::Enum(d) | FieldTy::Bits(d) => d,
        t => panic!("{name}: {t:?} is not a code table"),
    }
}

fn items(d: &EnumDesc) -> Vec<(&'static str, u32)> {
    d.items.iter().map(|i| (i.name, i.value)).collect()
}

fn assert_table(row: &str, d: &EnumDesc, want: &[(&str, u32)]) {
    let got = items(d);
    assert_eq!(got.len(), want.len(), "{row}: {got:?}");
    for (g, w) in got.iter().zip(want) {
        assert_eq!((g.0, g.1), (w.0, w.1), "{row}");
    }
}

fn values(d: &EnumDesc) -> Vec<u32> {
    d.items.iter().map(|i| i.value).collect()
}

#[test]
fn evt_054_refdata_mode_codes() {
    use hft_refdata::{AccountKind, MarginMode, MultiAsset, PositionMode, SimAccountCfg};
    let f = AccountConfigReportHdr::FIELDS;
    let pm: Vec<_> = PositionMode::ALL.iter().map(|m| (m.name(), *m as u32)).collect();
    assert_eq!(
        pm,
        vec![("NotApplicable", 1), ("OneWay", 2), ("Hedge", 3)],
        "EVT-054.1"
    );
    assert_eq!(items(ty_desc(f, "position_mode")), pm, "EVT-054.1");
    let mm: Vec<_> = MarginMode::ALL.iter().map(|m| (m.name(), *m as u32)).collect();
    assert_eq!(
        mm,
        vec![("NotApplicable", 1), ("Cross", 2), ("Isolated", 3), ("Mixed", 4)],
        "EVT-054.2"
    );
    assert_eq!(items(ty_desc(f, "margin_mode")), mm, "EVT-054.2");
    let ma: Vec<_> = MultiAsset::ALL.iter().map(|m| (m.name(), *m as u32)).collect();
    assert_eq!(ma, vec![("NotApplicable", 1), ("Off", 2), ("On", 3)], "EVT-054.3");
    assert_eq!(items(ty_desc(f, "multi_asset")), ma, "EVT-054.3");
    let sim = AccountKind::Sim(SimAccountCfg {
        initial_balances: Box::new([]),
        latency_model: 0,
        fee_tier: 0,
        margin_model: 0,
    });
    let ak = vec![
        ("Live", u32::from(AccountKind::Live.code())),
        ("Sim", u32::from(sim.code())),
    ];
    assert_eq!(ak, vec![("Live", 1), ("Sim", 2)], "EVT-054.4");
    assert_eq!(items(ty_desc(CmdPreview::FIELDS, "target_kind")), ak, "EVT-054.4");
}

#[test]
fn evt_055_target_origin_wire() {
    use TargetOriginWire::*;
    let ok = [
        (1u8, Strategy),
        (2, EngineMarginStress),
        (3, EngineExternal),
        (4, EngineCommand),
        (5, EngineKillFlatten),
        (6, EngineAutoFix),
    ];
    for (i, (v, w)) in ok.into_iter().enumerate() {
        assert_eq!(TargetOriginWire::try_from(v), Ok(w), "EVT-055.{}", i + 1);
    }
    for (row, v) in [("EVT-055.7", 0u8), ("EVT-055.8", 7), ("EVT-055.9", 255)] {
        assert_eq!(
            TargetOriginWire::try_from(v),
            Err(DecodeErr::InvalidEnum),
            "{row}"
        );
    }
}

fn read_patched<B: Body>(off: usize, v: u8, other: Option<(usize, &[u8])>) -> Result<B, DecodeErr> {
    let mut body = canonical(kind_info(B::KIND).unwrap().fields);
    body[off] = v;
    if let Some((o, b)) = other {
        body[o..o + b.len()].copy_from_slice(b);
    }
    decode_record(&raw_record(B::KIND.0, 0, 1, 1, &body))
        .unwrap()
        .read::<B>()
}

#[test]
fn evt_061_chain_codes() {
    use TxState::*;
    for (v, w) in [
        (1u8, Sent),
        (2, IncludedOk),
        (3, IncludedReverted),
        (4, Dropped),
        (5, Unknown),
        (6, Finalized),
    ] {
        assert_eq!(
            read_patched::<TxLifecycle>(8, v, None).unwrap().state,
            w,
            "EVT-061.1"
        );
    }
    assert_eq!(
        read_patched::<TxLifecycle>(8, 0, None).unwrap_err(),
        DecodeErr::InvalidEnum,
        "EVT-061.2"
    );
    assert_eq!(
        read_patched::<TxLifecycle>(8, 7, None).unwrap_err(),
        DecodeErr::InvalidEnum,
        "EVT-061.3"
    );
    let s = read_patched::<SwapDecision>(110, 1, Some((100, &[0; 4]))).unwrap();
    assert_eq!(
        (s.hedge_inst.0, s.hedge_side),
        (0, hft_types::Side::Buy),
        "EVT-061.4"
    );
    assert_eq!(
        read_patched::<SwapDecision>(110, 0, Some((100, &[0; 4]))).unwrap_err(),
        DecodeErr::InvalidEnum,
        "EVT-061.5"
    );
}

fn disjoint(row: &str, d: &EnumDesc, want: &[u32]) {
    let v = values(d);
    assert_eq!(v, want, "{row}");
    for (i, a) in v.iter().enumerate() {
        for b in &v[i + 1..] {
            assert_eq!(a & b, 0, "{row}");
        }
    }
}

#[test]
fn evt_073_bitsets_disjoint() {
    disjoint(
        "EVT-073.1",
        ty_desc(ReconcileReport::FIELDS, "sflags"),
        &[1, 2, 4, 8],
    );
    assert_eq!(ReconcileReport::AUTO_EXTERNAL, 8);
    disjoint("EVT-073.2", ty_desc(CmdResult::FIELDS, "rflags"), &[1, 2, 4]);
    disjoint(
        "EVT-073.3",
        ty_desc(AccountConfigReportHdr::FIELDS, "mismatches"),
        &[1, 2, 4, 8, 16, 32, 64, 128, 256],
    );
    disjoint(
        "EVT-073.4",
        ty_desc(AccountConfigReportHdr::FIELDS, "rflags"),
        &[1, 2],
    );
    disjoint(
        "EVT-073.4",
        ty_desc(AccountConfigReportHdr::FIELDS, "uid_keys"),
        &[1, 2],
    );
    disjoint("EVT-073.5", ty_desc(MarginReportHdr::FIELDS, "mflags"), &[1, 2]);
    disjoint("EVT-073.6", ty_desc(ExecDecision::FIELDS, "eflags"), &[1, 2]);
    disjoint("EVT-073.7", ty_desc(CapRecord::FIELDS, "cflags"), &[1, 2, 4]);
    disjoint("EVT-073.8", ty_desc(RawHdr::FIELDS, "rflags"), &[1, 2]);
    disjoint("EVT-073.8", ty_desc(L2BatchHdr::FIELDS, "bflags"), &[1]);
    disjoint("EVT-073.8", ty_desc(L2Delta::FIELDS, "dflags"), &[1]);
    for (n, d) in [
        ("USERSTREAM_DOWN", "EVT-073.1"),
        ("TRUNCATED", "EVT-073.1"),
        ("HISTORY_UNCHECKED", "EVT-073.1"),
        ("AUTO_EXTERNAL", "EVT-073.1"),
    ] {
        assert!(
            ty_desc(ReconcileReport::FIELDS, "sflags").value(n).is_some(),
            "{d} {n}"
        );
    }
}

#[test]
fn evt_074_code_tables() {
    let od = OrderDecision::FIELDS;
    assert_table(
        "EVT-074.1",
        ty_desc(od, "action"),
        &[
            ("New", 1),
            ("Cancel", 2),
            ("Amend", 3),
            ("CancelAll", 4),
            ("Query", 5),
        ],
    );
    let lr = [
        ("None", 0),
        ("First", 1),
        ("Second", 2),
        ("SecondRetry", 3),
        ("HedgeFix", 4),
        ("Reverse", 5),
    ];
    assert_table("EVT-074.2", ty_desc(od, "link_role"), &lr);
    assert_table("EVT-074.2", ty_desc(LatencySample::FIELDS, "link_role"), &lr);
    assert_table(
        "EVT-074.3",
        ty_desc(SendReport::FIELDS, "status"),
        &[
            ("Written", 1),
            ("KillBlocked", 2),
            ("RateLimited", 3),
            ("WriteError", 4),
            ("NoConn", 5),
            ("EncodeError", 6),
            ("RingFull", 7),
            ("QueryUnsupported", 8),
        ],
    );
    let gw = [
        ("Place", 1),
        ("Cancel", 2),
        ("Amend", 3),
        ("CancelAll", 4),
        ("Query", 5),
        ("DeadManRefresh", 6),
        ("Rotate", 7),
        ("ResetRate", 8),
        ("Resubscribe", 9),
        ("SetEpoch", 10),
        ("SessionKeepalive", 11),
        ("MirrorRestore", 12),
        ("SimReset", 13),
        ("SimTransfer", 14),
        ("SimFault", 15),
    ];
    assert_table("EVT-074.4", ty_desc(SendReport::FIELDS, "op"), &gw);
    assert_table("EVT-074.4", ty_desc(GwCmd::FIELDS, "op"), &gw);
    for a in OrderAction::ALL {
        assert_eq!(
            GwOp::from_repr(*a as u8).map(|g| g as u8),
            Some(*a as u8),
            "EVT-074.4"
        );
    }
    assert_table(
        "EVT-074.5",
        ty_desc(OrderEvent::FIELDS, "what"),
        &[
            ("Ack", 1),
            ("Reject", 2),
            ("PartialFill", 3),
            ("Fill", 4),
            ("CancelAck", 5),
            ("CancelReject", 6),
            ("AmendAck", 7),
            ("AmendReject", 8),
            ("Expired", 9),
            ("QueryResult", 10),
            ("OutcomeUnknown", 11),
            ("FeeOnly", 12),
        ],
    );
    assert_table(
        "EVT-074.6",
        ty_desc(ExternalFill::FIELDS, "cause"),
        &[
            ("Liquidation", 1),
            ("Adl", 2),
            ("Manual", 3),
            ("Delivery", 4),
            ("Unknown", 5),
        ],
    );
    assert_table(
        "EVT-074.7",
        ty_desc(MarginReportHdr::FIELDS, "source"),
        &[("Push", 1), ("Poll", 2)],
    );
    assert_table(
        "EVT-074.8",
        ty_desc(AccountConfigReportHdr::FIELDS, "source"),
        &[("Startup", 1), ("Push", 2), ("Poll", 3)],
    );
    let ed = ExecDecision::FIELDS;
    assert_table(
        "EVT-074.9",
        ty_desc(ed, "action"),
        &[
            ("TargetSet", 1),
            ("TargetReplaced", 2),
            ("TargetCleared", 3),
            ("TargetRejected", 4),
            ("Slice", 5),
            ("State", 6),
            ("HedgeFix", 7),
            ("ResidualTrim", 8),
            ("ExternalSeen", 9),
            ("ExternalDefault", 10),
            ("ForceStep", 11),
            ("CmdHedgeFix", 12),
            ("ChainLegExcess", 13),
            ("AutoFix", 14),
        ],
    );
    assert_table(
        "EVT-074.10",
        ty_desc(ed, "algo"),
        &[
            ("None", 0),
            ("TakerSlice", 1),
            ("PairOnFill", 2),
            ("Simultaneous", 3),
            ("MakerFirst", 4),
            ("Engine", 5),
        ],
    );
    assert_table(
        "EVT-074.11",
        ty_desc(ed, "reason"),
        &[
            ("Rebalance", 1),
            ("Exit", 2),
            ("MarginStress", 3),
            ("Hard", 4),
            ("Manual", 5),
        ],
    );
    assert_table(
        "EVT-074.12",
        ty_desc(ed, "urgency"),
        &[("Passive", 1), ("Normal", 2), ("Urgent", 3)],
    );
    let ts = [("Idle", 1), ("Working", 2), ("Stuck", 3), ("Broken", 4)];
    assert_table("EVT-074.13", ty_desc(ed, "state_from"), &ts);
    assert_table("EVT-074.13", ty_desc(ed, "state_to"), &ts);
    assert_eq!(ty_desc(ed, "origin"), &TargetOriginWire::DESC, "EVT-074.14");
    assert_table(
        "EVT-074.15",
        ty_desc(ReconcileReport::FIELDS, "item"),
        &[
            ("Position", 1),
            ("Balance", 2),
            ("OpenOrder", 3),
            ("UnknownResolved", 4),
            ("Summary", 5),
            ("Funding", 6),
        ],
    );
    assert_table(
        "EVT-074.16",
        ty_desc(ReconcileReport::FIELDS, "severity"),
        &[("Ignore", 1), ("Warn", 2)],
    );
    let os = [
        ("New", 1),
        ("PartiallyFilled", 2),
        ("Filled", 3),
        ("Cancelled", 4),
        ("Rejected", 5),
        ("Expired", 6),
        ("NotFound", 7),
    ];
    let mut ros = vec![("Other", 0)];
    ros.extend(os);
    assert_table(
        "EVT-074.17",
        ty_desc(ReconcileReport::FIELDS, "order_status"),
        &ros,
    );
    assert_table(
        "EVT-074.18",
        &codes::LATENCY_SRC,
        &[("Missing", 0), ("HW", 1), ("SW", 2), ("TSC", 3)],
    );
    assert_table(
        "EVT-074.19",
        ty_desc(LatencySample::FIELDS, "action"),
        &[("New", 0), ("Cancel", 1), ("Amend", 2)],
    );
    assert_table(
        "EVT-074.20",
        ty_desc(Note::FIELDS, "level"),
        &[("P0", 1), ("P1", 2), ("P2", 3), ("Info", 4)],
    );
    assert_table(
        "EVT-074.21",
        ty_desc(Note::FIELDS, "source"),
        &[
            ("Sequencer", 1),
            ("Strategy", 2),
            ("Risk", 3),
            ("Oms", 4),
            ("Venue", 5),
            ("CtlAudit", 6),
            ("Measure", 7),
            ("Housekeeping", 8),
            ("Ledger", 9),
            ("Exec", 10),
            ("Boot", 11),
            ("Advisor", 12),
        ],
    );
    assert_table(
        "EVT-074.22",
        ty_desc(ConnState::FIELDS, "state"),
        &[
            ("Up", 1),
            ("Down", 2),
            ("Resync", 3),
            ("Rotated", 4),
            ("Degraded", 5),
            ("Alive", 6),
        ],
    );
    assert_table(
        "EVT-074.23",
        ty_desc(ConnState::FIELDS, "role"),
        &[("Md", 1), ("Order", 2), ("UserStream", 3), ("Rest", 4)],
    );
    assert_table(
        "EVT-074.24",
        ty_desc(ClockStatus::FIELDS, "source"),
        &[("Phc", 1), ("Ntp", 2), ("None", 3)],
    );
    assert_table(
        "EVT-074.24",
        ty_desc(ClockStatus::FIELDS, "state"),
        &[("Synced", 1), ("Unsynced", 2), ("Stepped", 3)],
    );
    let sr = SegmentRoll::FIELDS;
    assert_table(
        "EVT-074.25",
        ty_desc(sr, "mark"),
        &[("End", 1), ("Begin", 2), ("BootDone", 3)],
    );
    assert_table(
        "EVT-074.25",
        ty_desc(sr, "reason"),
        &[
            ("Hourly", 1),
            ("Size", 2),
            ("Startup", 3),
            ("Recovery", 4),
            ("Manual", 5),
            ("RecoverFromSnapshot", 6),
        ],
    );
    assert_table(
        "EVT-074.25",
        ty_desc(sr, "journal_kind"),
        &[("Raw", 1), ("Seq", 2), ("Chain", 3)],
    );
    assert_table(
        "EVT-074.26",
        ty_desc(EngineClock::FIELDS, "source"),
        &[("Fresh", 1), ("Tail", 2), ("Snapshot", 3)],
    );
    assert_table(
        "EVT-074.27",
        ty_desc(L2SnapHdr::FIELDS, "origin"),
        &[("Exchange", 1), ("Keyframe", 2), ("TopN", 3)],
    );
    assert_table(
        "EVT-074.28",
        ty_desc(BlobRef::FIELDS, "kind"),
        &[
            ("Refdata", 1),
            ("StrategyParams", 2),
            ("RiskParams", 3),
            ("HostManifest", 4),
            ("Engine", 5),
            ("SimConfig", 6),
        ],
    );
    assert_table(
        "EVT-074.29",
        ty_desc(RawHdr::FIELDS, "what"),
        &[
            ("WsText", 1),
            ("WsBinary", 2),
            ("RestResponse", 3),
            ("Ping", 4),
            ("Pong", 5),
            ("Connect", 6),
            ("Disconnect", 7),
            ("SubAck", 8),
            ("WsClose", 9),
        ],
    );
    assert_table(
        "EVT-074.30",
        ty_desc(TxLifecycle::FIELDS, "state"),
        &[
            ("Sent", 1),
            ("IncludedOk", 2),
            ("IncludedReverted", 3),
            ("Dropped", 4),
            ("Unknown", 5),
            ("Finalized", 6),
        ],
    );
    assert_table(
        "EVT-074.31",
        ty_desc(ChainHead::FIELDS, "source"),
        &[("Node", 1), ("BlockStream", 2), ("Rpc", 3)],
    );
    assert_table(
        "EVT-074.31",
        ty_desc(PoolQuote::FIELDS, "level"),
        &[("L0", 1), ("L1", 2)],
    );
    assert_table(
        "EVT-074.31",
        ty_desc(SwapDecision::FIELDS, "exact"),
        &[("ExactIn", 0), ("ExactOut", 1)],
    );
    assert_table(
        "EVT-074.31",
        ty_desc(SwapDecision::FIELDS, "hedge_px_mode"),
        &[("Cross", 1), ("Mid", 2), ("LimitBp", 3)],
    );
    assert_table(
        "EVT-074.32",
        ty_desc(StrategyParamsHdr::FIELDS, "source"),
        &[("Pipeline", 1), ("Manual", 2)],
    );
    assert_table(
        "EVT-074.32",
        ty_desc(LegDef::FIELDS, "kind"),
        &[("Cex", 1), ("Chain", 2)],
    );
    assert_table(
        "EVT-074.33",
        ty_desc(CmdHdr::FIELDS, "role"),
        &[("Process", 0), ("Viewer", 1), ("Trader", 2), ("Admin", 4)],
    );
    assert!(!codes::CMD_ROLE.contains(3), "EVT-074.33 retired 3");
    assert_table("EVT-074.33", ty_desc(CmdHdr::FIELDS, "hflags"), &[("PREVIEW", 1)]);
    assert_table(
        "EVT-074.34",
        &codes::ANNOTATE_KIND,
        &[("OpsNote", 1), ("LocalWrite", 2), ("Incident", 3)],
    );
    assert_table(
        "EVT-074.34",
        &codes::VENUE_NOTICE_KIND,
        &[("Delisting", 1), ("FundingInterval", 2), ("Maintenance", 3)],
    );
    assert_table(
        "EVT-074.34",
        &codes::ADOPT_ITEM,
        &[("Position", 0), ("Balance", 1)],
    );
    assert_table("EVT-074.35", &codes::ORDER_STATUS, &os);
    assert!(!codes::ORDER_STATUS.contains(0), "EVT-074.35");
    assert_table("EVT-074.35", &codes::REJECT_CLASS, &[("OTHER", 0), ("GONE", 1)]);
    assert!(!codes::RECON_SEVERITY.contains(3), "EVT-074.16 retired 3");
}
