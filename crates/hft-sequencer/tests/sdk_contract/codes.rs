//! SDK codes against the wire (SDK-143, SDK-190).

use hft_event::codes::{EXEC_ALGO, LINK_ROLE, TARGET_REASON, URGENCY};
use hft_event::schema::EnumDesc;
use hft_event::{ExternalCause, TargetOriginWire};
use hft_sequencer::codes::status;
use hft_strategy_sdk::{
    BrokenReason, EngineOrigin, EventRef, ExecSpec, FirstLeg, HedgeKind, IntentClass, LegLink, LegLinkOpt,
    LinkRole, NoteLevel, OnFail, PxMode, ResetCause, ResidualPolicy, RxTsSrc, SimulOnFail, StuckReason,
    TargetOrigin, TargetReason, TargetReject, Urgency,
};
use hft_types::{RejectBits, Side, Tif};

use super::*;
use crate::common::*;

fn wire(d: &EnumDesc, name: &str) -> u32 {
    d.items
        .iter()
        .find(|i| i.name == name)
        .map(|i| i.value)
        .unwrap_or_else(|| panic!("{} has no {name}", d.name))
}

#[test]
fn sdk_143_enum_codes_match_wire() {
    // .1 IntentClass.
    assert_eq!(
        [
            IntentClass::Open,
            IntentClass::Reduce,
            IntentClass::Hedge,
            IntentClass::Flatten
        ]
        .map(|c| c as u8),
        [1, 2, 3, 4],
        "SDK-143.1"
    );
    // .2 LinkRole against OrderDecision.link_role.
    for (r, n) in [
        (LinkRole::First, "First"),
        (LinkRole::Second, "Second"),
        (LinkRole::SecondRetry, "SecondRetry"),
        (LinkRole::HedgeFix, "HedgeFix"),
        (LinkRole::Reverse, "Reverse"),
    ] {
        assert_eq!(u32::from(r as u8), wire(&LINK_ROLE, n), "SDK-143.2 {n}");
    }
    assert_eq!(wire(&LINK_ROLE, "None"), 0);
    // .3
    assert_eq!(
        [HedgeKind::None, HedgeKind::OnFill, HedgeKind::Simultaneous].map(|c| c as u8),
        [0, 1, 2],
        "SDK-143.3"
    );
    assert_eq!(
        [PxMode::Cross, PxMode::Mid, PxMode::Limit].map(|c| c as u8),
        [1, 2, 3]
    );
    assert_eq!(
        [OnFail::Retry, OnFail::Reverse, OnFail::Hold].map(|c| c as u8),
        [1, 2, 3]
    );
    assert_eq!(
        [
            ResidualPolicy::Defer,
            ResidualPolicy::TrimExcess,
            ResidualPolicy::Hold
        ]
        .map(|c| c as u8),
        [1, 2, 3]
    );
    // .4 against ExecDecision.reason / urgency.
    for (r, n) in [
        (TargetReason::Rebalance, "Rebalance"),
        (TargetReason::Exit, "Exit"),
        (TargetReason::MarginStress, "MarginStress"),
        (TargetReason::Hard, "Hard"),
        (TargetReason::Manual, "Manual"),
    ] {
        assert_eq!(u32::from(r as u8), wire(&TARGET_REASON, n), "SDK-143.4 {n}");
    }
    for (u, n) in [
        (Urgency::Passive, "Passive"),
        (Urgency::Normal, "Normal"),
        (Urgency::Urgent, "Urgent"),
    ] {
        assert_eq!(u32::from(u as u8), wire(&URGENCY, n), "SDK-143.4 {n}");
    }
    // .5 ExecSpec against ExecDecision.algo.
    assert_eq!(
        u32::from(ExecSpec::TakerSlice(slice0()).algo()),
        wire(&EXEC_ALGO, "TakerSlice")
    );
    assert_eq!(
        u32::from(ExecSpec::PairOnFill(pair0()).algo()),
        wire(&EXEC_ALGO, "PairOnFill")
    );
    assert_eq!(wire(&EXEC_ALGO, "Simultaneous"), 3);
    assert_eq!(wire(&EXEC_ALGO, "MakerFirst"), 4);
    // .6
    assert_eq!(
        [
            FirstLeg::ByLiquidity,
            FirstLeg::PerpFirst,
            FirstLeg::SpotFirst,
            FirstLeg::Fixed
        ]
        .map(|c| c as u8),
        [1, 2, 3, 4],
        "SDK-143.6"
    );
    assert_eq!([SimulOnFail::Reverse, SimulOnFail::Hold].map(|c| c as u8), [1, 2]);
    // .7 TargetReject: 11 codes 1..=11, `from_code` inverse.
    for c in 1..=11u8 {
        let r = TargetReject::from_code(c).unwrap_or_else(|| panic!("SDK-143.7 code {c}"));
        assert_eq!(r.code(), c);
    }
    assert_eq!(TargetReject::NotOwner.code(), 1);
    assert_eq!(TargetReject::LotMismatch.code(), 10);
    assert_eq!(TargetReject::ReduceOnlyCmd.code(), 11);
    assert!(TargetReject::from_code(12).is_none());
    // .8 StuckReason.
    let stuck = [
        StuckReason::Gated(RejectBits(1)),
        StuckReason::Shadow,
        StuckReason::NoLiquidity,
        StuckReason::StaleQuote,
        StuckReason::SliceFails,
        StuckReason::External,
        StuckReason::EngineOverride,
        StuckReason::MinOrder,
    ];
    assert_eq!(stuck.map(|s| s.code()), [1, 2, 3, 4, 5, 6, 7, 8], "SDK-143.8");
    // .9 BrokenReason.
    assert_eq!(
        [
            BrokenReason::LinkBroken,
            BrokenReason::ReverseFailed,
            BrokenReason::FixExhausted,
            BrokenReason::HedgeVeto
        ]
        .map(|b| b.code()),
        [1, 2, 3, 4],
        "SDK-143.9"
    );
    // .10 ExternalCause is hft-event's own type.
    assert_eq!(
        [
            ExternalCause::Liquidation,
            ExternalCause::Adl,
            ExternalCause::Manual,
            ExternalCause::Delivery,
            ExternalCause::Unknown
        ]
        .map(|c| c as u8),
        [1, 2, 3, 4, 5],
        "SDK-143.10"
    );
    // .11 TargetOrigin ↔ TargetOriginWire.
    let origins = [
        TargetOrigin::Strategy,
        TargetOrigin::Engine(EngineOrigin::MarginStress),
        TargetOrigin::Engine(EngineOrigin::External),
        TargetOrigin::Engine(EngineOrigin::Command),
        TargetOrigin::Engine(EngineOrigin::KillFlatten),
        TargetOrigin::Engine(EngineOrigin::AutoFix),
    ];
    for (k, o) in origins.iter().enumerate() {
        let w = TargetOriginWire::from(*o);
        assert_eq!(w as u8, k as u8 + 1, "SDK-143.11 {o:?}");
        assert_eq!(TargetOrigin::from(w), *o);
    }
    // .12 ResetCause (ResetAll = 4 is not one of its values).
    assert_eq!(
        [
            ResetCause::Structural,
            ResetCause::Fingerprint,
            ResetCause::PctConfig
        ]
        .map(|c| c.code()),
        [1, 2, 3],
        "SDK-143.12"
    );
    assert_eq!(ResetCause::RESET_ALL_CODE, 4);
    // .13
    assert_eq!(
        [RxTsSrc::None, RxTsSrc::Hw, RxTsSrc::Sw].map(|c| c as u8),
        [0, 1, 2],
        "SDK-143.13"
    );
    // .14 NoteLevel is hft-event's.
    assert_eq!(
        [NoteLevel::P0, NoteLevel::P1, NoteLevel::P2, NoteLevel::Info].map(|c| c as u8),
        [1, 2, 3, 4],
        "SDK-143.14"
    );
}

#[test]
fn sdk_190_intent_record_consistency() {
    let mut h = rig();
    // A linked first leg (OnFill hedge) and a PairOnFill target in one on_event; every field
    // a distinct non-zero value.
    let trig = h.upd + 1;
    add_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.update_id == trig
        {
            let mut it = order_in(
                hx,
                GT1,
                A1,
                Side::Buy,
                Tif::Ioc,
                IntentClass::Open,
                b.ask_px,
                q("0.0007"),
            );
            it.tag = 0x0123;
            it.link = LegLinkOpt::some(LegLink {
                group: GT1,
                role: LinkRole::First,
                _pad: [0; 3],
                tolerance: r("0.02"),
                hedge: onfill(AC1, B1, Side::Sell),
            });
            let _ = ctx.place(it);
            let mut t = target(
                AC1,
                GT2,
                &[(0, q("0.01")), (1, q("-0.01"))],
                ExecSpec::PairOnFill(pair0()),
            );
            t.reason = TargetReason::Exit;
            t.urgency = Urgency::Urgent;
            t.tag = 0x0456;
            let _ = ctx.target(t);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    let recs = h.since(m);
    let d = decs(&recs)
        .into_iter()
        .find(|d| d.inst == A1 && d.link_role == 1)
        .expect("first leg");
    assert_eq!(d.px, px("50001"));
    assert_eq!(d.qty, q("0.0007"));
    assert_eq!(d.group, GT1);
    assert_eq!(d.side, Side::Buy);
    assert_eq!(d.tif, Tif::Ioc);
    assert_eq!(d.reject, 0);
    let x = execs(&recs)
        .into_iter()
        .find(|x| x.group == GT2 && x.action == 1)
        .expect("TargetSet");
    assert_eq!(x.reason, TargetReason::Exit as u8);
    assert_eq!(x.urgency, Urgency::Urgent as u8);
    assert_eq!(x.algo, 2);
    assert_eq!(x.tag, 0x0456);
    let _ = status::APPLIED;
}
