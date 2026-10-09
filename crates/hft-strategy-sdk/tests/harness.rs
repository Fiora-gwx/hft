//! `CtxHarness` (SDK-115, SDK-150).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::*;
use hft_strategy_sdk::ctx::NoteReq;
use hft_strategy_sdk::testing::CtxHarness;
use hft_strategy_sdk::*;
use hft_types::{GROUP_NONE, INST_NONE, InstId};

/// Runs one `note` call through a fresh harness; `Err(message)` when the harness asserted.
fn note(
    code: u32,
    level: NoteLevel,
    group: hft_types::GroupId,
    inst: InstId,
    v: [u64; 8],
) -> Result<Vec<NoteReq>, String> {
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut h = CtxHarness::new(&EchoProbe::DESC);
        h.run(|ctx| ctx.note(code, level, group, inst, v)).notes
    }));
    r.map_err(|p| {
        p.downcast_ref::<String>()
            .cloned()
            .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_default()
    })
}

#[test]
fn sdk_115_note_registration_asserted() {
    let n = note(1, NoteLevel::Info, G2, I1, [7, 0, 0, 0, 0, 0, 0, 0]).expect("SDK-115.1");
    assert_eq!(n.len(), 1, "SDK-115.1");
    let r = n[0];
    assert_eq!(
        (r.code, r.level, r.group, r.inst, r.v[0]),
        (1, NoteLevel::Info, G2, I1, 7),
        "SDK-115.1"
    );
    let n = note(1, NoteLevel::Info, GROUP_NONE, INST_NONE, [0; 8]).expect("SDK-115.2");
    assert_eq!((n.len(), n[0].group), (1, GROUP_NONE), "SDK-115.2");
    let e = note(4, NoteLevel::Info, G1, I1, [0; 8]).expect_err("SDK-115.3");
    assert!(e.contains("code 4"), "SDK-115.3: {e}");
    let e = note(2, NoteLevel::Info, G1, I1, [0; 8]).expect_err("SDK-115.4");
    assert!(e.contains("level"), "SDK-115.4: {e}");
}

#[test]
fn sdk_150_set_outcomes() {
    let mut h = CtxHarness::new(&TargetProbe::DESC);
    let r = h.run(|ctx| {
        let got = ctx.target(pair_target(A1, G1, 1_000, 0));
        assert_eq!(got, Ok(IntentHandle(0)));
    });
    assert_eq!(r.targets.len(), 1);
    h.set_outcomes(&[IntentOutcome::TargetRejected(TargetReject::LotMismatch)]);
    h.run(|ctx| {
        assert_eq!(
            ctx.outcome(IntentHandle(0)),
            Some(IntentOutcome::TargetRejected(TargetReject::LotMismatch))
        );
        assert_eq!(ctx.outcome(IntentHandle(1)), None);
    });
}
