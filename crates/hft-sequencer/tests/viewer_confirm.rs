//! Viewer emergency stop (TESTS.md SEQ-303).

use hft_event::{CmdHdr, ControlCmd};
use hft_sequencer::codes::{detail, status};
use hft_types::{AccountId, GroupId, KillLevel, RunMode, StrategyId, VenueId};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

const MS: u64 = 1_000_000;

/// K = non-HARD, non-flatten `Kill{Account(a)}` with `cancel_resting`.
fn kill(a: AccountId) -> ControlCmd<'static> {
    ControlCmd::Kill {
        scope: scope(StrategyId(0), a, GroupId(0)),
        level: KillLevel::Account,
        flatten: false,
        cancel_resting: true,
        hard: false,
    }
}

/// The gateway's command hash stands in as the target account and the variant flags.
fn hash_of(c: &ControlCmd<'_>) -> [u8; 32] {
    let mut x = [0u8; 32];
    if let ControlCmd::Kill {
        scope,
        level,
        flatten,
        hard,
        ..
    } = c
    {
        x[0] = scope.account.0;
        x[1] = *level as u8;
        x[2] = u8::from(*flatten);
        x[3] = u8::from(*hard);
        x[4] = scope.venue.0;
    }
    x
}

fn viewer(h: &mut H, issuer: &str, c: &ControlCmd<'_>, preview: bool, confirmed: bool) -> (u8, u32) {
    let hs = hash_of(c);
    let id = h.cmd_with(issuer, VIEWER, AccountId(0), c, |x| {
        x.cmd_hash = hs;
        if preview {
            x.hflags |= CmdHdr::PREVIEW;
        }
        x.confirmed = u8::from(confirmed);
    });
    h.run();
    h.res(id)
}

#[test]
fn seq_303_viewer_emergency_stop() {
    let k = kill(AC1);
    // .1 no preview, unconfirmed.
    let mut h = steady();
    assert_eq!(viewer(&mut h, "tg:v1", &k, false, false).0, status::NEEDS_CONFIRM);
    // .2 / .6 confirmed exactly at the TTL; ten state-version changes in between do not matter;
    // the preview is used up.
    let mut h = steady();
    assert_eq!(viewer(&mut h, "tg:v1", &k, true, false).0, status::APPLIED);
    for i in 0..10 {
        let m = if i % 2 == 0 {
            RunMode::Shadow
        } else {
            RunMode::Live
        };
        h.cmd_confirmed(
            AC2,
            &ControlCmd::SetMode {
                scope: scope(S2, AC2, G2),
                mode: m,
            },
        );
        h.run();
    }
    h.adv(60_000 * MS);
    assert_eq!(viewer(&mut h, "tg:v1", &k, false, true).0, status::APPLIED);
    assert_eq!(
        viewer(&mut h, "tg:v1", &k, false, true),
        (status::CONFIRM_INVALID, u32::from(detail::NO_VALID_PREVIEW))
    );
    // .3 one ns past the TTL.
    let mut h = steady();
    viewer(&mut h, "tg:v1", &k, true, false);
    h.adv(60_000 * MS + 1);
    assert_eq!(
        viewer(&mut h, "tg:v1", &k, false, true),
        (status::CONFIRM_INVALID, u32::from(detail::NO_VALID_PREVIEW))
    );
    // .4 another viewer; .5 another command.
    let mut h = steady();
    viewer(&mut h, "tg:v1", &k, true, false);
    assert_eq!(
        viewer(&mut h, "tg:v2", &k, false, true),
        (status::CONFIRM_INVALID, u32::from(detail::NO_VALID_PREVIEW))
    );
    assert_eq!(
        viewer(&mut h, "tg:v1", &kill(AC2), false, true),
        (status::CONFIRM_INVALID, u32::from(detail::NO_VALID_PREVIEW))
    );
    // .7 seventeen previews: the oldest is evicted, 16 remain.
    let mut h = steady();
    for i in 0..17 {
        viewer(&mut h, &format!("tg:p{i}"), &k, true, false);
    }
    assert_eq!(h.s.engine().viewer_preview_count(), 16);
    assert_eq!(
        viewer(&mut h, "tg:p0", &k, false, true),
        (status::CONFIRM_INVALID, u32::from(detail::NO_VALID_PREVIEW))
    );
    assert_eq!(viewer(&mut h, "tg:p1", &k, false, true).0, status::APPLIED);
    // .8 after a restart (EngineClock clears the table).
    let mut h = steady();
    viewer(&mut h, "tg:v1", &k, true, false);
    let mut h = H::crash_restart(
        h,
        plan(hft_sequencer::SnapshotSel::Latest, true, 1, 0),
        Rerun::default(),
    )
    .unwrap();
    h.run();
    assert_eq!(
        viewer(&mut h, "tg:v1", &k, false, true),
        (status::CONFIRM_INVALID, u32::from(detail::NO_VALID_PREVIEW))
    );
    // .9 everything else is beyond a viewer.
    let mut h = steady();
    let others = [
        ControlCmd::Kill {
            scope: scope(StrategyId(0), AC1, GroupId(0)),
            level: KillLevel::Account,
            flatten: false,
            cancel_resting: true,
            hard: true,
        },
        ControlCmd::Kill {
            scope: scope(StrategyId(0), AC1, GroupId(0)),
            level: KillLevel::Account,
            flatten: true,
            cancel_resting: true,
            hard: false,
        },
        ControlCmd::Kill {
            scope: hft_event::CmdScope {
                venue: B,
                ..scope(StrategyId(0), AccountId(0), GroupId(0))
            },
            level: KillLevel::Venue,
            flatten: false,
            cancel_resting: true,
            hard: false,
        },
        ControlCmd::Resume {
            scope: scope(StrategyId(0), AC1, GroupId(0)),
            source: hft_types::CapSource::KILL,
        },
        ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Shadow,
        },
    ];
    for (n, c) in others.iter().enumerate() {
        let id = h.cmd_with("tg:v1", VIEWER, AccountId(0), c, |x| {
            x.cmd_hash = hash_of(c);
            x.confirmed = 1;
        });
        h.run();
        assert_eq!(h.res(id).0, status::ROLE_INSUFFICIENT, "row 9.{}", n + 1);
    }
    // .10 a trader's confirmation after a state-version change (an upgrade needs confirmation).
    let mut h = steady();
    h.cmd_confirmed(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Shadow,
        },
    );
    h.run();
    let c = ControlCmd::SetMode {
        scope: scope(S1, AC1, G1),
        mode: RunMode::Live,
    };
    let v = h.s.engine().risk.mode.ctl_state_version();
    let id = h.cmd_with("cli:t1", TRADER, AC1, &c, |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::NEEDS_CONFIRM);
    h.cmd_confirmed(
        AC2,
        &ControlCmd::SetMode {
            scope: scope(S2, AC2, G2),
            mode: RunMode::Shadow,
        },
    );
    h.run();
    let id = h.cmd_with("cli:t1", TRADER, AC1, &c, |x| {
        x.confirmed = 1;
        x.expect_state_version = v;
    });
    h.run();
    assert_eq!(
        h.res(id),
        (status::CONFIRM_INVALID, u32::from(detail::STATE_CHANGED))
    );
    let _ = VenueId(0);
}
