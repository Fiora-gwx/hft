//! Mode state: effective mode, cap records, resume rules, Recovery auto release, kill word,
//! Kill cancels, flatten lock, ClockBad and Delisting (RSK-060–RSK-094, RSK-140–RSK-142,
//! RSK-210, RSK-223).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

mod common;
use common::*;

use hft_event::{CapRecord, CmdScope, NoteLevel};
use hft_refdata::{Exchange, InstKind, MarginData};
use hft_risk::mode::{
    CancelScope, CapLevel, CellKey, Effective, GateEvent, ModeErr, ModeInput, ModeOutcome, Preview,
    ReconItemKey, RecoveryFlags, StorageKind, VenueNoticeCmd, cfg_params, level_of, mismatch,
};
use hft_risk::snapshot::{ModeRow, SECTION_MODE_TABLE, mode_row_flags};
use hft_risk::{Capacities, EquityView, Origin, ReconcileGroup, Risk};
use hft_strategy_sdk::RiskEvent;
use hft_testkit::alloc::CountingAlloc;
use hft_types::{
    AccountId, CapSource, GROUP_ALL, GROUP_NONE, GroupId, InstId, KillBits, KillLevel, RejectBits, RunMode,
    Side, StrategyId, Tif, VenueId,
};
use zerocopy::FromBytes;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

const CLOSE: Effective = Effective::CloseOnly {
    deferred_shadow: false,
};
const DEFERRED: Effective = Effective::CloseOnly {
    deferred_shadow: true,
};

fn c1() -> CellKey {
    cell(1, 1, G1)
}
fn c3() -> CellKey {
    cell(1, 1, G3)
}
fn all_cells() -> [CellKey; 4] {
    [cell(1, 1, G1), cell(1, 1, G3), cell(2, 2, G2), cell(3, 3, GS)]
}
fn open_a1() -> hft_risk::IntentView {
    open(1, G1, A1, Side::Buy, "0.01", "50000.00", Tif::Gtc)
}
fn acfg(a: u8, v: VenueId, p: u64) -> ModeInput<'static> {
    ModeInput::Auto(rec(CapSource::ACCOUNT_CFG, 0, 2, sc_av(a, v), p))
}
fn auto_clear(source: CapSource, scope: CmdScope) -> ModeInput<'static> {
    ModeInput::AutoClear { source, scope }
}
fn delist_cmd(v: VenueId, i: InstId) -> VenueNoticeCmd {
    VenueNoticeCmd {
        venue: v,
        kind: VenueNoticeCmd::DELISTING,
        inst: i,
        a: 1_700_000_000_000_000_000,
        b: 0,
    }
}
fn scope(strategy: u8, venue: VenueId, account: u8, group: GroupId, inst: InstId) -> CmdScope {
    let mut s = sc_any();
    s.strategy = StrategyId(strategy);
    s.venue = venue;
    s.account = AccountId(account);
    s.group = group;
    s.inst = inst;
    s
}
fn sorted_cells(o: &ModeOutcome) -> Vec<(CellKey, Effective, Effective)> {
    let mut v: Vec<_> = o.cells.iter().copied().collect();
    v.sort_by_key(|c| c.0);
    v
}
/// The records carried by the `CapSet` notes of the last apply.
fn capset_records(o: &ModeOutcome) -> Vec<CapRecord> {
    o.notes
        .iter()
        .filter(|x| note_n(x) == N_CAP_SET)
        .map(|x| {
            let mut b = [0u8; 48];
            for (k, w) in x.v.iter().take(6).enumerate() {
                b[k * 8..k * 8 + 8].copy_from_slice(&w.to_le_bytes());
            }
            CapRecord::read_from_bytes(&b).unwrap()
        })
        .collect()
}
fn empty_outcome(o: &ModeOutcome) -> bool {
    o.cells.is_empty()
        && o.cancels.is_empty()
        && !o.kill_changed
        && !o.resolve_broken
        && o.released.is_empty()
        && o.hedge_gate_open.is_empty()
        && o.venue_gate_open.is_empty()
        && o.gate_changes.is_empty()
        && o.risk_events.is_empty()
        && o.drain.is_none()
        && o.notes.is_empty()
}

// ------------------------------------------------------------------------------------ RSK-060

#[test]
fn rsk_060_effective_mode() {
    let eff1 = |w: &W| eff(w, c1());
    let w = r0();
    assert_eq!(eff1(&w), LIVE, "RSK-060.1");
    let mut w = r0();
    w.ok(set_mode(1, 1, G1, RunMode::CloseOnly));
    assert_eq!(eff1(&w), CLOSE, "RSK-060.2");
    let mut w = r0();
    w.ok(set_mode(1, 1, G1, RunMode::Shadow));
    assert_eq!(eff1(&w), Effective::Shadow, "RSK-060.3");
    assert_eq!(
        sorted_cells(&w.out),
        vec![(c1(), LIVE, Effective::Shadow)],
        "RSK-060.3 cells"
    );
    let mut w = r0();
    w.mm.not_flat.insert(c1());
    w.ok(set_mode(1, 1, G1, RunMode::Shadow));
    assert_eq!(eff1(&w), DEFERRED, "RSK-060.4");
    assert_eq!(
        sorted_cells(&w.out),
        vec![(c1(), LIVE, DEFERRED)],
        "RSK-060.4 cells"
    );
    let v = w.risk.mode.ctl_state_version();
    w.mm.not_flat.clear();
    w.ok(ModeInput::CellFlat {
        cell: c1(),
        flat: true,
    });
    assert_eq!(eff1(&w), Effective::Shadow, "RSK-060.5");
    assert_eq!(
        sorted_cells(&w.out),
        vec![(c1(), DEFERRED, Effective::Shadow)],
        "RSK-060.5 cells"
    );
    assert_eq!(w.risk.mode.ctl_state_version(), v, "RSK-060.5 version");
    let mut w = r0();
    for flat in [true, false] {
        w.ok(ModeInput::CellFlat { cell: c1(), flat });
        assert!(empty_outcome(&w.out), "RSK-060.6 flat {flat}");
        assert_eq!(eff1(&w), LIVE, "RSK-060.6");
    }
    let mut w = r0();
    w.ok(daily(1));
    assert_eq!(eff1(&w), CLOSE, "RSK-060.7");
    let mut w = r0();
    w.ok(set_mode(1, 1, G1, RunMode::Shadow));
    w.ok(daily(1));
    assert_eq!(eff1(&w), Effective::Shadow, "RSK-060.8");
    let mut w = r0();
    w.ok(auto(CapSource::RECON, sc_av(1, B)));
    assert_eq!(eff1(&w), LIVE, "RSK-060.9");
    assert_eq!(w.risk.mode.venue_level(acc(1), B), CapLevel::CloseOnly);
    assert_eq!(w.risk.mode.venue_level(acc(1), A), CapLevel::None);
    let mut w = r0();
    w.ok(kill_venue(B, 1));
    assert_eq!(eff1(&w), LIVE, "RSK-060.10");
    assert_eq!(w.risk.mode.venue_level(acc(1), B), CapLevel::CloseOnly);
    let mut w = r0();
    w.ok(kill_acct(1, true));
    assert_eq!(eff1(&w), CLOSE, "RSK-060.11");
    assert_eq!(w.risk.mode.venue_level(acc(1), A), CapLevel::Hard);
    let mut w = r0();
    w.ok(halted(1, 1, G1));
    assert_eq!((eff1(&w), eff(&w, c3())), (CLOSE, LIVE), "RSK-060.12");
    let mut w = r0();
    w.ok(auto(CapSource::HALTED, sc_cell(1, 1, GROUP_NONE)));
    assert_eq!((eff1(&w), eff(&w, c3())), (LIVE, LIVE), "RSK-060.13");
    let mut w = r0();
    w.ok(kill_global(false));
    for c in all_cells() {
        assert_eq!(eff(&w, c), CLOSE, "RSK-060.14 {c:?}");
    }
    assert_eq!(w.out.cells.len(), 4, "RSK-060.14 cells");
    for (k, inp) in [
        acfg(1, B, cfg_params::MISMATCH),
        auto(CapSource::UNKNOWN_TIMEOUT, sc_av(1, B)),
    ]
    .into_iter()
    .enumerate()
    {
        let mut w = r0();
        w.ok(inp);
        assert_eq!(eff1(&w), LIVE, "RSK-060.15 sub {k}");
    }
    let mut w = r0();
    w.delist(B, B1);
    assert_eq!(eff1(&w), LIVE, "RSK-060.15 delisting");
    assert!(w.risk.mode.inst_blocked(B, B1) && !w.risk.mode.inst_blocked(B, B2));
    let mut w = r0();
    w.delist(B, InstId(0));
    assert!(
        w.risk.mode.inst_blocked(B, B1) && w.risk.mode.inst_blocked(B, B2),
        "RSK-060.16"
    );
    assert!(!w.risk.mode.inst_blocked(A, A1), "RSK-060.16 A");
    for k in 0..3 {
        let mut w = r0();
        match k {
            0 => w.clock_bad(),
            1 => w.storage(1),
            _ => w.ok(draining()),
        }
        for c in all_cells() {
            assert_eq!(eff(&w, c), CLOSE, "RSK-060.17 sub {k} {c:?}");
        }
    }
}

#[test]
fn rsk_061_setmode_and_caps_independent() {
    let mut w = r0();
    let all1 = ModeInput::SetMode {
        scope: sc_cell(1, 1, GROUP_ALL),
        mode: RunMode::CloseOnly,
    };
    w.ok(all1);
    w.ok(daily(1));
    assert!(w.cap(CapSource::DAILY_LOSS, sc_acct(1)).is_some());
    assert_eq!(w.risk.mode.desired(c1()), Some(RunMode::CloseOnly));
    assert_eq!(w.risk.mode.desired(c3()), Some(RunMode::CloseOnly));
    w.ok(ModeInput::SetMode {
        scope: sc_cell(1, 1, GROUP_ALL),
        mode: RunMode::Live,
    });
    assert_eq!(w.risk.mode.desired(c1()), Some(RunMode::Live));
    assert!(w.cap(CapSource::DAILY_LOSS, sc_acct(1)).is_some());
    assert_eq!(eff(&w, c1()), CLOSE);
    assert!(w.out.cells.is_empty());
    w.ok(resume(CapSource::DAILY_LOSS, sc_acct(1)));
    assert_eq!(w.risk.mode.desired(c1()), Some(RunMode::Live));
    assert_eq!(eff(&w, c1()), LIVE);
    assert_eq!(
        sorted_cells(&w.out),
        vec![(c1(), CLOSE, LIVE), (c3(), CLOSE, LIVE)]
    );
}

/// Rows of section 14.
fn mode_rows(bytes: &[u8]) -> Vec<ModeRow> {
    let r = hft_strategy_sdk::SnapshotReader::new(bytes);
    let s = r.find_section(SECTION_MODE_TABLE).unwrap().unwrap();
    let mut x = s.reader();
    (0..s.count).map(|_| x.get_row::<ModeRow>().unwrap()).collect()
}

#[test]
fn rsk_062_default_mode_for_new_group() {
    let mut w = r0();
    w.ok(ModeInput::SetMode {
        scope: sc_cell(1, 1, GROUP_ALL),
        mode: RunMode::Shadow,
    });
    let g4 = GroupId(0x0100_0003);
    w.ok(ModeInput::GroupsChanged {
        added: &[cell(1, 1, g4)],
        removed: &[c3()],
    });
    assert_eq!(eff(&w, cell(1, 1, g4)), Effective::Shadow);
    assert_eq!(w.risk.mode.desired(c3()), None);
    let rows = mode_rows(&w.snapshot());
    assert!(
        !rows
            .iter()
            .any(|r| r.account == 1 && r.group == G3.0 && r.flags & mode_row_flags::DEFAULT == 0)
    );
    assert!(rows.iter().any(|r| *r
        == ModeRow {
            strategy: 1,
            account: 1,
            mode: RunMode::Shadow as u8,
            flags: mode_row_flags::DEFAULT,
            group: GROUP_NONE.0,
        }));
}

#[test]
fn rsk_063_level_of() {
    let r = |s: CapSource, level: u8, cf: u8| CapRecord {
        level,
        ..rec(s, level, cf, sc_any(), 0)
    };
    assert_eq!(
        level_of(&r(CapSource::KILL, 4, CapRecord::HARD)),
        CapLevel::Hard,
        "RSK-063.1"
    );
    assert_eq!(CapLevel::Hard as u8, 3);
    for l in [KillLevel::Venue, KillLevel::Account, KillLevel::Global] {
        assert_eq!(
            level_of(&r(CapSource::KILL, l as u8, 0)),
            CapLevel::CloseOnly,
            "RSK-063.2 {l:?}"
        );
    }
    assert_eq!(
        level_of(&r(CapSource::MARGIN_STRESS, 3, 2)),
        CapLevel::CloseOnly,
        "RSK-063.3"
    );
    for s in [
        CapSource::DRAINING,
        CapSource::CLOCK_BAD,
        CapSource::DAILY_LOSS,
        CapSource::RECON,
        CapSource::UNKNOWN_TIMEOUT,
        CapSource::BROKEN,
        CapSource::REORG,
        CapSource::RECOVERY,
        CapSource::HALTED,
        CapSource::HEDGE_FAILURE,
        CapSource::STORAGE,
        CapSource::DELISTING,
        CapSource::ACCOUNT_CFG,
    ] {
        assert_eq!(level_of(&r(s, 0, 0)), CapLevel::CloseOnly, "RSK-063.4 {s:?}");
    }
}

// ------------------------------------------------------------------------------------ RSK-070

#[test]
fn rsk_070_cap_record_encoding() {
    let g = GROUP_ALL;
    let none = VenueId(0);
    let i0 = InstId(0);
    let exp = |source: CapSource, level: u8, cflags: u8, sc: CmdScope, p0: u64| CapRecord {
        source,
        level,
        cflags,
        _pad: [0; 5],
        scope: sc,
        set_seq: 500,
        params: [p0, 0],
    };
    let run = |id: &str, set: &dyn Fn(&mut W) -> ModeInput<'static>, want: &[CapRecord]| {
        let mut w = r0();
        let inp = set(&mut w);
        assert_eq!(w.apply_seq(inp, 500), Ok(()), "{id}");
        assert_eq!(capset_records(&w.out), want, "{id}");
    };
    run(
        "RSK-070.1",
        &|_| kill(KillLevel::Account, sc_acct(1), true, false, false),
        &[exp(CapSource::KILL, 3, 0, scope(0, none, 1, g, i0), 1)],
    );
    run(
        "RSK-070.2",
        &|_| kill(KillLevel::Global, sc_any(), true, false, true),
        &[exp(
            CapSource::KILL,
            4,
            CapRecord::HARD,
            scope(0, none, 0, g, i0),
            1,
        )],
    );
    run(
        "RSK-070.3",
        &|_| kill(KillLevel::Venue, scope(0, B, 1, g, i0), false, false, false),
        &[exp(CapSource::KILL, 2, 0, scope(0, B, 1, g, i0), 0)],
    );
    run(
        "RSK-070.4",
        &|_| kill(KillLevel::Account, sc_acct(1), true, true, false),
        &[exp(CapSource::KILL, 3, 0, scope(0, none, 1, g, i0), 3)],
    );
    run(
        "RSK-070.5",
        &|w| {
            let eq = EquityView {
                total: n("200000"),
                valid: true,
            };
            w.risk
                .on_pnl(acc(1), n("-5000.000001"), eq)
                .expect("DailyLoss input")
        },
        &[exp(
            CapSource::DAILY_LOSS,
            0,
            0,
            scope(0, none, 1, g, i0),
            (-5_000_000_000i64) as u64,
        )],
    );
    run(
        "RSK-070.6 recon",
        &|_| auto(CapSource::RECON, sc_av(1, B)),
        &[exp(CapSource::RECON, 0, 0, scope(0, B, 1, g, i0), 0)],
    );
    run(
        "RSK-070.6 unknown",
        &|_| auto(CapSource::UNKNOWN_TIMEOUT, sc_av(1, B)),
        &[exp(CapSource::UNKNOWN_TIMEOUT, 0, 0, scope(0, B, 1, g, i0), 0)],
    );
    run(
        "RSK-070.7",
        &|_| broken(1, G1),
        &[
            exp(CapSource::BROKEN, 0, 0, scope(0, none, 1, G1, i0), 0),
            exp(CapSource::HEDGE_FAILURE, 0, 0, scope(0, none, 1, g, i0), 0),
        ],
    );
    run(
        "RSK-070.8 recovery",
        &|_| auto(CapSource::RECOVERY, sc_acct(1)),
        &[exp(CapSource::RECOVERY, 0, 0, scope(0, none, 1, g, i0), 0)],
    );
    run(
        "RSK-070.8 halted",
        &|_| halted(1, 1, G1),
        &[exp(CapSource::HALTED, 0, 0, scope(1, none, 1, G1, i0), 0)],
    );
    run(
        "RSK-070.9",
        &|_| draining(),
        &[exp(
            CapSource::DRAINING,
            0,
            CapRecord::NON_CLEARABLE,
            scope(0, none, 0, g, i0),
            1,
        )],
    );
    run(
        "RSK-070.10",
        &|_| ModeInput::Clock { ok: false },
        &[exp(
            CapSource::CLOCK_BAD,
            0,
            CapRecord::CONDITIONAL,
            scope(0, none, 0, g, i0),
            0,
        )],
    );
    run(
        "RSK-070.11",
        &|w| {
            w.margin(1, B, &msample("0.15", "0.10", T0), T0);
            *w.mo.mode.get(0).expect("MarginStress input")
        },
        &[exp(
            CapSource::MARGIN_STRESS,
            2,
            2,
            scope(0, none, 1, g, i0),
            1 << B.0,
        )],
    );
    run(
        "RSK-070.12",
        &|_| ModeInput::Storage {
            kind: StorageKind::PersistLag,
            level: 1,
        },
        &[exp(CapSource::STORAGE, 0, 2, scope(0, none, 0, g, i0), 1)],
    );
    {
        let mut w = r0();
        let cmd = delist_cmd(B, B1);
        assert_eq!(w.apply_seq(ModeInput::Notice(&cmd), 500), Ok(()));
        assert_eq!(
            capset_records(&w.out),
            vec![exp(
                CapSource::DELISTING,
                0,
                0,
                scope(0, B, 0, g, B1),
                1_700_000_000_000_000_000
            )],
            "RSK-070.13"
        );
    }
    {
        let mut w = r0();
        let h = hdr(1, B, 0, KEYS);
        w.risk.on_account_cfg(&h, &[lev(B1, "3.01"), lev(B2, "3.00")]);
        w.mm.sync(&w.risk);
        let r = w.risk.mode.apply(
            ModeInput::Gate(GateEvent::AccountCfg {
                hdr: &h,
                keys_required: KEYS,
            }),
            500,
            &w.mm,
            &mut w.out,
        );
        assert_eq!(r, Ok(()));
        assert_eq!(
            capset_records(&w.out),
            vec![exp(
                CapSource::ACCOUNT_CFG,
                0,
                2,
                scope(0, B, 1, g, i0),
                cfg_params::LEVERAGE
            )],
            "RSK-070.14"
        );
    }
}

#[test]
fn rsk_071_cap_merge() {
    let k1 = || kill(KillLevel::Account, sc_acct(1), true, false, false);
    let k1f = || kill(KillLevel::Account, sc_acct(1), true, true, false);
    {
        let mut w = r0();
        w.apply_seq(k1(), 500).unwrap();
        let v = w.risk.mode.ctl_state_version();
        w.apply_seq(k1f(), 600).unwrap();
        let caps: Vec<_> = w
            .risk
            .mode
            .caps()
            .iter()
            .filter(|c| c.source == CapSource::KILL)
            .copied()
            .collect();
        assert_eq!(caps.len(), 1, "RSK-071.1");
        assert_eq!((caps[0].params[0], caps[0].set_seq), (3, 500), "RSK-071.1");
        assert_eq!(capset_records(&w.out), caps, "RSK-071.1 note");
        assert_eq!(w.risk.mode.ctl_state_version(), v + 1, "RSK-071.1 version");
        let v = w.risk.mode.ctl_state_version();
        w.apply_seq(k1(), 700).unwrap();
        assert_eq!(
            w.cap(CapSource::KILL, sc_acct(1)).unwrap().params[0],
            3,
            "RSK-071.2"
        );
        assert!(empty_outcome(&w.out), "RSK-071.2 outputs");
        assert_eq!(w.risk.mode.ctl_state_version(), v, "RSK-071.2 version");
    }
    {
        let mut w = r0();
        w.apply_seq(kill_global(false), 500).unwrap();
        w.apply_seq(kill_global(true), 600).unwrap();
        let caps: Vec<_> = w
            .risk
            .mode
            .caps()
            .iter()
            .filter(|c| c.source == CapSource::KILL)
            .copied()
            .collect();
        assert_eq!(caps.len(), 1, "RSK-071.3");
        assert_eq!(caps[0].cflags, CapRecord::HARD);
        assert!(w.out.kill_changed);
    }
    {
        let mut w = r0();
        let dl = |p: i64| ModeInput::Auto(rec(CapSource::DAILY_LOSS, 0, 0, sc_acct(1), p as u64));
        w.apply_seq(dl(-5_000_000_000), 500).unwrap();
        w.apply_seq(dl(-4_000_000_000), 600).unwrap();
        assert_eq!(
            w.cap(CapSource::DAILY_LOSS, sc_acct(1)).unwrap().params[0],
            (-4_000_000_000i64) as u64,
            "RSK-071.4"
        );
        assert_eq!(capset_records(&w.out).len(), 1, "RSK-071.4 note");
        w.apply_seq(dl(-6_000_000_000), 700).unwrap();
        assert_eq!(
            w.cap(CapSource::DAILY_LOSS, sc_acct(1)).unwrap().params[0],
            (-4_000_000_000i64) as u64,
            "RSK-071.5"
        );
        assert!(empty_outcome(&w.out), "RSK-071.5 outputs");
    }
    {
        let mut w = r0();
        w.apply_seq(acfg(1, B, cfg_params::MISMATCH), 500).unwrap();
        w.apply_seq(acfg(1, B, cfg_params::LEVERAGE), 600).unwrap();
        assert_eq!(
            w.cap(CapSource::ACCOUNT_CFG, sc_av(1, B)).unwrap().params[0],
            3,
            "RSK-071.6"
        );
    }
    {
        let mut w = r0();
        w.apply_seq(margin_auto(1, 1, 1 << B.0), 500).unwrap();
        w.apply_seq(margin_auto(1, 2, 1 << B.0), 600).unwrap();
        assert_eq!(
            w.cap(CapSource::MARGIN_STRESS, sc_acct(1)).unwrap().level,
            2,
            "RSK-071.7"
        );
        assert_eq!(capset_records(&w.out).len(), 1, "RSK-071.7 note");
        assert!(w.out.risk_events.iter().any(|(a, e)| *a == acc(1)
            && matches!(e, RiskEvent::CapSet { source, .. } if *source == CapSource::MARGIN_STRESS)));
        w.apply_seq(margin_auto(1, 1, 1 << B.0), 700).unwrap();
        assert_eq!(
            w.cap(CapSource::MARGIN_STRESS, sc_acct(1)).unwrap().level,
            1,
            "RSK-071.8"
        );
        assert_eq!(capset_records(&w.out).len(), 1, "RSK-071.8 note");
    }
}

#[test]
fn rsk_072_resume_exact_match() {
    let mut w = r0();
    w.ok(kill_acct(1, false));
    w.ok(kill_global(false));
    w.ok(halted(1, 1, G1));
    assert_eq!(w.apply(resume(CapSource::KILL, sc_acct(1))), Ok(()), "step 1");
    assert!(w.cap(CapSource::KILL, sc_any()).is_some());
    assert_eq!(eff(&w, c1()), CLOSE);
    assert!(w.out.cells.is_empty());
    assert_eq!(
        w.apply(resume(CapSource::KILL, sc_acct(2))),
        Err(ModeErr::NoMatchingCap),
        "step 2"
    );
    assert_eq!(
        w.apply(resume(CapSource::HALTED, sc_cell(1, 1, GROUP_ALL))),
        Err(ModeErr::NoMatchingCap),
        "step 3"
    );
    assert_eq!(w.apply(resume(CapSource::KILL, sc_any())), Ok(()), "step 4");
    for c in [c3(), cell(2, 2, G2), cell(3, 3, GS)] {
        assert_eq!(eff(&w, c), LIVE, "{c:?}");
    }
    assert_eq!(eff(&w, c1()), CLOSE);
}

#[test]
fn rsk_073_resumable_sources() {
    let rows: [(CapSource, ModeInput<'static>, CmdScope); 5] = [
        (CapSource::DRAINING, draining(), sc_any()),
        (CapSource::CLOCK_BAD, ModeInput::Clock { ok: false }, sc_any()),
        (CapSource::MARGIN_STRESS, margin_auto(1, 1, 1 << B.0), sc_acct(1)),
        (
            CapSource::STORAGE,
            ModeInput::Storage {
                kind: StorageKind::PersistLag,
                level: 1,
            },
            sc_any(),
        ),
        (
            CapSource::ACCOUNT_CFG,
            acfg(1, B, cfg_params::MISMATCH),
            sc_av(1, B),
        ),
    ];
    for (s, set, sc) in rows {
        let mut w = r0();
        w.ok(set);
        assert_eq!(
            w.apply(resume(s, sc)),
            Err(ModeErr::NotResumable),
            "RSK-073.1 {s:?}"
        );
        assert!(w.cap(s, sc).is_some(), "RSK-073.1 {s:?} kept");
    }
    let mut w = r0();
    for (s, sc) in [
        (CapSource::KILL, sc_acct(1)),
        (CapSource::DAILY_LOSS, sc_acct(1)),
        (CapSource::RECON, sc_av(1, B)),
        (CapSource::UNKNOWN_TIMEOUT, sc_av(1, B)),
        (CapSource::BROKEN, sc_ag(1, G1)),
        (CapSource::RECOVERY, sc_acct(1)),
        (CapSource::HALTED, sc_cell(1, 1, G1)),
        (CapSource::HEDGE_FAILURE, sc_acct(1)),
        (CapSource::DELISTING, sc_vi(B, B1)),
    ] {
        assert_eq!(
            w.apply(resume(s, sc)),
            Err(ModeErr::NoMatchingCap),
            "RSK-073.2 {s:?}"
        );
    }
    let mut w = r0();
    w.delist(B, B1);
    assert_eq!(
        w.apply(resume(CapSource::DELISTING, sc_vi(B, B1))),
        Ok(()),
        "RSK-073.3"
    );
    assert!(!w.risk.mode.inst_blocked(B, B1));
    let mut w = r0();
    w.ok(halted(1, 1, G1));
    assert_eq!(
        w.apply(resume(CapSource::HALTED, sc_cell(1, 1, G1))),
        Ok(()),
        "RSK-073.4"
    );
}

/// Levels of the `CapSet` / `CapCleared` notes of the last apply.
fn levels(o: &ModeOutcome, n: u32) -> Vec<NoteLevel> {
    o.notes
        .iter()
        .filter(|x| note_n(x) == n)
        .map(|x| x.level)
        .collect()
}

#[test]
fn rsk_074_cap_note_levels() {
    use NoteLevel::{Info, P1};
    // (set, clear, CapSet levels, CapCleared levels)
    let case = |id: &str,
                set: &dyn Fn(&mut W),
                set_lv: &[NoteLevel],
                clear: &dyn Fn(&mut W),
                clr_lv: &[NoteLevel]| {
        let mut w = r0();
        set(&mut w);
        assert_eq!(levels(&w.out, N_CAP_SET), set_lv, "{id} set");
        for x in w.out.notes.iter().filter(|x| note_n(x) == N_CAP_SET) {
            assert_eq!(x.source, hft_event::NoteSource::Risk, "{id}");
        }
        clear(&mut w);
        assert_eq!(levels(&w.out, N_CAP_CLEARED), clr_lv, "{id} clear");
    };
    case(
        "RSK-074.1",
        &|w| w.ok(kill_acct(1, false)),
        &[Info],
        &|w| w.ok(resume(CapSource::KILL, sc_acct(1))),
        &[Info],
    );
    case(
        "RSK-074.2",
        &|w| w.delist(B, B1),
        &[Info],
        &|w| w.ok(resume(CapSource::DELISTING, sc_vi(B, B1))),
        &[Info],
    );
    case(
        "RSK-074.3",
        &|w| w.ok(auto(CapSource::RECOVERY, sc_acct(1))),
        &[Info],
        &|w| w.ok(resume(CapSource::RECOVERY, sc_acct(1))),
        &[Info],
    );
    case("RSK-074.4", &|w| w.ok(draining()), &[Info], &|_| {}, &[]);
    let p1_rows: [(&str, &dyn Fn(&mut W), &dyn Fn(&mut W)); 9] = [
        ("daily", &|w| w.ok(daily(1)), &|w| {
            w.ok(resume(CapSource::DAILY_LOSS, sc_acct(1)))
        }),
        ("recon", &|w| w.ok(auto(CapSource::RECON, sc_av(1, B))), &|w| {
            w.ok(resume(CapSource::RECON, sc_av(1, B)))
        }),
        (
            "unknown",
            &|w| w.ok(auto(CapSource::UNKNOWN_TIMEOUT, sc_av(1, B))),
            &|w| w.ok(resume(CapSource::UNKNOWN_TIMEOUT, sc_av(1, B))),
        ),
        ("margin", &|w| w.ok(margin_auto(1, 1, 1 << B.0)), &|w| {
            w.ok(auto_clear(CapSource::MARGIN_STRESS, sc_acct(1)))
        }),
        ("storage", &|w| w.storage(1), &|w| {
            w.ok(ModeInput::Storage {
                kind: StorageKind::Both,
                level: 0,
            })
        }),
        ("cfg", &|w| w.ok(acfg(1, B, cfg_params::MISMATCH)), &|w| {
            w.ok(auto_clear(CapSource::ACCOUNT_CFG, sc_av(1, B)))
        }),
        ("clock", &|w| w.clock_bad(), &|w| {
            w.ok(ModeInput::Clock { ok: true })
        }),
        ("halted", &|w| w.ok(halted(1, 1, G1)), &|w| {
            w.ok(resume(CapSource::HALTED, sc_cell(1, 1, G1)))
        }),
        ("broken", &|w| w.ok(broken(1, G1)), &|w| {
            w.ok(auto_clear(CapSource::BROKEN, sc_ag(1, G1)))
        }),
    ];
    for (name, set, clear) in p1_rows {
        let want: &[NoteLevel] = if name == "broken" { &[P1, P1] } else { &[P1] };
        case(&format!("RSK-074.5 {name}"), set, want, clear, &[Info]);
    }
    {
        // HedgeFailure: set with Broken, cleared after the Broken is gone.
        let mut w = r0();
        w.ok(broken(1, G1));
        w.ok(auto_clear(CapSource::BROKEN, sc_ag(1, G1)));
        w.ok(resume(CapSource::HEDGE_FAILURE, sc_acct(1)));
        assert_eq!(
            levels(&w.out, N_CAP_CLEARED),
            vec![Info],
            "RSK-074.5 hedge failure"
        );
    }
    {
        let mut w = r0();
        let h = hdr(1, A, mismatch::UID, KEYS);
        w.risk.on_account_cfg(&h, &[]);
        w.ok(ModeInput::Gate(GateEvent::AccountCfg {
            hdr: &h,
            keys_required: KEYS,
        }));
        let k = w.cap(CapSource::KILL, sc_acct(1)).expect("UID kill");
        assert_eq!(k.cflags & CapRecord::HARD, CapRecord::HARD);
        assert_eq!(levels(&w.out, N_CAP_SET), vec![P1], "RSK-074.6 set");
        w.recon_all();
        w.ok(resume(CapSource::KILL, sc_acct(1)));
        assert_eq!(levels(&w.out, N_CAP_CLEARED), vec![Info], "RSK-074.6 clear");
    }
}

fn events(o: &ModeOutcome) -> Vec<(u8, bool, CapSource)> {
    o.risk_events
        .iter()
        .filter_map(|(a, e)| match e {
            RiskEvent::CapSet { source, .. } => Some((a.0, true, *source)),
            RiskEvent::CapCleared { source, .. } => Some((a.0, false, *source)),
            _ => None,
        })
        .collect()
}

#[test]
fn rsk_075_risk_events_per_account() {
    let k = CapSource::KILL;
    let mut w = r0();
    w.ok(kill_global(false));
    assert_eq!(
        events(&w.out),
        vec![(1, true, k), (2, true, k), (3, true, k)],
        "RSK-075.1"
    );
    w.ok(resume(k, sc_any()));
    assert_eq!(
        events(&w.out),
        vec![(1, false, k), (2, false, k), (3, false, k)],
        "RSK-075.6"
    );
    let mut w = r0();
    w.ok(auto(CapSource::RECON, sc_av(1, B)));
    assert_eq!(events(&w.out), vec![(1, true, CapSource::RECON)], "RSK-075.2");
    let mut w = r0();
    w.storage(1);
    let s = CapSource::STORAGE;
    assert_eq!(
        events(&w.out),
        vec![(1, true, s), (2, true, s), (3, true, s)],
        "RSK-075.3"
    );
    let mut w = r0();
    w.ok(kill_venue(B, 1));
    assert_eq!(events(&w.out), vec![(1, true, k)], "RSK-075.4");
    let mut w = r0();
    w.delist(B, B1);
    let d = CapSource::DELISTING;
    assert_eq!(
        events(&w.out),
        vec![(1, true, d), (2, true, d), (3, true, d)],
        "RSK-075.5"
    );
    let mut w = r0();
    w.ok(kill_venue(C, 0));
    assert_eq!(events(&w.out), vec![(1, true, k)], "RSK-075.7");
}

#[test]
fn rsk_076_state_version() {
    let run = |id: &str, steps: &dyn Fn(&mut W), dv: u64| {
        let mut w = r0();
        let v = w.v0;
        steps(&mut w);
        assert_eq!(w.out.ctl_state_version, v + dv, "{id} outcome");
        assert_eq!(w.risk.mode.ctl_state_version(), v + dv, "{id}");
    };
    run("RSK-076.1", &|w| w.ok(set_mode(1, 1, G1, RunMode::CloseOnly)), 1);
    run(
        "RSK-076.2",
        &|w| {
            w.ok(set_mode(1, 1, G1, RunMode::CloseOnly));
            w.ok(set_mode(1, 1, G1, RunMode::CloseOnly));
        },
        1,
    );
    run("RSK-076.3", &|w| w.ok(broken(1, G1)), 1);
    run(
        "RSK-076.4",
        &|w| {
            w.ok(ModeInput::Gate(GateEvent::OrderConns {
                account: acc(1),
                venue: B,
                primary_up: false,
                standby_up: true,
            }))
        },
        0,
    );
    run(
        "RSK-076.5",
        &|w| {
            w.mm.not_flat.insert(c1());
            w.ok(set_mode(1, 1, G1, RunMode::Shadow));
            w.mm.not_flat.clear();
            w.ok(ModeInput::CellFlat {
                cell: c1(),
                flat: true,
            });
        },
        1,
    );
    run(
        "RSK-076.6",
        &|w| {
            let r = w.apply(resume(CapSource::DAILY_LOSS, sc_acct(1)));
            assert_eq!(r, Err(ModeErr::NoMatchingCap));
            w.out.ctl_state_version = w.risk.mode.ctl_state_version();
        },
        0,
    );
    run(
        "RSK-076.7",
        &|w| {
            w.clock_bad();
            w.ok(ModeInput::Clock { ok: true });
        },
        2,
    );
    run(
        "RSK-076.8",
        &|w| {
            w.ok(kill_acct(1, false));
            w.ok(resume(CapSource::KILL, sc_acct(1)));
        },
        2,
    );
}

/// Applies `inp`, expects `want`; a rejection must leave the snapshot bytes, the version and the
/// outcome untouched (RSK-077.11).
fn expect_apply(w: &mut W, inp: ModeInput<'_>, want: Result<(), ModeErr>, id: &str) {
    let before = w.snapshot();
    let v = w.risk.mode.ctl_state_version();
    let r = w.apply(inp);
    assert_eq!(r, want, "{id}");
    if r.is_err() {
        assert_eq!(w.snapshot(), before, "{id} snapshot");
        assert_eq!(w.risk.mode.ctl_state_version(), v, "{id} version");
        assert!(empty_outcome(&w.out), "{id} outcome");
    }
}

#[test]
fn rsk_077_mode_err_order() {
    let mut w = r0();
    let mut kv = sc_any();
    kv.venue = B;
    for (k, inp) in [
        kill(KillLevel::Venue, kv, true, false, true),
        kill(KillLevel::Global, sc_any(), true, true, false),
        kill(KillLevel::Account, sc_acct(1), true, true, true),
    ]
    .into_iter()
    .enumerate()
    {
        expect_apply(
            &mut w,
            inp,
            Err(ModeErr::InvalidScope),
            &format!("RSK-077.1 sub {k}"),
        );
    }
    let mut w = r0();
    w.ok(draining());
    expect_apply(
        &mut w,
        resume(CapSource::DRAINING, sc_any()),
        Err(ModeErr::NotResumable),
        "RSK-077.2",
    );
    let mut w = g0();
    w.ok(draining());
    w.ok(daily(1));
    expect_apply(
        &mut w,
        resume(CapSource::DAILY_LOSS, sc_acct(1)),
        Err(ModeErr::Draining),
        "RSK-077.3",
    );
    let mut w = g0();
    expect_apply(
        &mut w,
        resume(CapSource::DAILY_LOSS, sc_acct(1)),
        Err(ModeErr::GateNotReleased),
        "RSK-077.4",
    );
    let mut w = r0();
    expect_apply(
        &mut w,
        resume(CapSource::DAILY_LOSS, sc_acct(1)),
        Err(ModeErr::NoMatchingCap),
        "RSK-077.5",
    );
    let mut w = g0();
    expect_apply(
        &mut w,
        set_mode(1, 1, G1, RunMode::Live),
        Err(ModeErr::GateNotReleased),
        "RSK-077.6 a",
    );
    expect_apply(
        &mut w,
        set_mode(1, 1, G1, RunMode::CloseOnly),
        Ok(()),
        "RSK-077.6 b",
    );
    let mut w = g0();
    w.ok(draining());
    expect_apply(&mut w, kill_acct(1, false), Ok(()), "RSK-077.7 a");
    expect_apply(&mut w, kill_flatten(1), Ok(()), "RSK-077.7 b");
    assert!(w.risk.view(acc(1), &w.nowv()).flatten_lock, "RSK-077.7 lock");
    let every = ModeInput::SetMode {
        scope: sc_any(),
        mode: RunMode::Live,
    };
    let mut w = g0();
    w.ok(set_mode(2, 2, G2, RunMode::CloseOnly));
    expect_apply(&mut w, every, Err(ModeErr::GateNotReleased), "RSK-077.8");
    assert_eq!(
        w.risk.mode.desired(cell(2, 2, G2)),
        Some(RunMode::CloseOnly),
        "RSK-077.8 account 2"
    );
    let mut w = r0();
    w.ok(set_mode(2, 2, G2, RunMode::CloseOnly));
    expect_apply(&mut w, every, Ok(()), "RSK-077.9");
    let mut w = r0();
    w.ok(auto(CapSource::UNKNOWN_TIMEOUT, sc_av(1, B)));
    w.mm.unknown.insert((1, B.0), 1);
    expect_apply(
        &mut w,
        resume(CapSource::UNKNOWN_TIMEOUT, sc_av(1, B)),
        Err(ModeErr::UnknownPending),
        "RSK-077.10 a",
    );
    w.mm.unknown.insert((1, B.0), 0);
    expect_apply(
        &mut w,
        resume(CapSource::UNKNOWN_TIMEOUT, sc_av(1, B)),
        Ok(()),
        "RSK-077.10 b",
    );
}

#[test]
fn rsk_078_is_upgrade() {
    let w = r0();
    let up = |i: ModeInput<'_>| w.risk.mode.is_upgrade(&i);
    assert!(up(set_mode(1, 1, G1, RunMode::Live)), "RSK-078.1");
    assert!(!up(set_mode(1, 1, G1, RunMode::CloseOnly)), "RSK-078.2 a");
    assert!(!up(set_mode(1, 1, G1, RunMode::Shadow)), "RSK-078.2 b");
    for s in [CapSource::KILL, CapSource::DAILY_LOSS, CapSource::RECOVERY] {
        assert!(up(resume(s, sc_acct(1))), "RSK-078.3 {s:?}");
    }
    assert!(up(kill_flatten(1)), "RSK-078.4");
    assert!(!up(kill_acct(1, false)), "RSK-078.5 a");
    assert!(!up(kill_global(true)), "RSK-078.5 b");
    let cmd = delist_cmd(B, B1);
    assert!(!up(ModeInput::Notice(&cmd)), "RSK-078.5 c");
}

#[test]
fn rsk_079_preview_lists_undisposed() {
    let mut w = g0();
    let mut steps = w.release_steps(1);
    for s in steps.iter_mut() {
        if let Step::Recon(v, items) = s
            && *v == B
        {
            let mut g = vec![summary(1, B, 7, w.seq + 200, 0, 0)];
            g.push(pos_item(1, B, 7, B1, "-0.003", "150"));
            g.push(pos_item(1, B, 7, B2, "0.02", "50"));
            *items = g;
        }
    }
    w.run(1, &steps);
    let before = w.snapshot();
    let v = w.risk.mode.ctl_state_version();
    let mut p = Preview::default();
    w.mm.sync(&w.risk);
    w.risk
        .preview(&resume(CapSource::RECOVERY, sc_acct(1)), &w.mm, &mut p);
    let b_items: Vec<_> = p
        .items
        .iter()
        .filter(|x| x.account == acc(1) && x.venue == B)
        .copied()
        .collect();
    assert_eq!(b_items.len(), 2);
    let mut got: Vec<(ReconItemKey, hft_types::Qty)> = b_items.iter().map(|x| (x.key, x.last_diff)).collect();
    got.sort_by_key(|(k, _)| k.id);
    assert_eq!(got[0].0.id, B1.0);
    assert_eq!(got[0].1, q("-0.003"));
    assert_eq!(got[1].0.id, B2.0);
    assert_eq!(got[1].1, q("0.02"));
    assert!(got.iter().all(|(k, _)| k.item == ReconItemKey::POSITION));
    for vv in [A, C] {
        assert!(
            !p.items.iter().any(|x| x.account == acc(1) && x.venue == vv),
            "{vv:?}"
        );
    }
    let mut p2 = Preview::default();
    w.risk.preview(&kill_global(false), &w.mm, &mut p2);
    assert_eq!(w.snapshot(), before);
    assert_eq!(w.risk.mode.ctl_state_version(), v);
}

/// RSK-080 refdata: 20 Live accounts, odd ones on {A, B}, even ones on {C, D}.
fn twenty_two_venues() -> &'static hft_refdata::RefData {
    const D: VenueId = VenueId(10);
    let d = D;
    let mut sp = r0_spec();
    sp.accounts = (1..=20u8)
        .map(|a| account_spec(a, false, a, if a % 2 == 1 { &[A, B] } else { &[C, D] }))
        .collect();
    sp.instances = (1..=20u8)
        .map(|i| hft_refdata::InstanceSpec {
            id: StrategyId(i),
            kind_id: 0xFF00,
            name: format!("inst-{i}").into(),
        })
        .collect();
    sp.sims.clear();
    sp.venues.push(venue_spec(d, Exchange::Bitget, InstKind::Perp));
    sp.caps.push(caps_spec(d, true, MarginData::Push));
    sp.limits.extend(order_limits(d, 100, 600));
    load(&sp)
}

#[test]
fn rsk_080_outcome_capacity_bound() {
    let rd = twenty_two_venues();
    let caps = Capacities {
        max_insts: 64,
        max_cells_total: 32,
        max_caps: 256,
    };
    let mut w = W::base(rd, p0_for(rd, 20), MC, caps);
    // 32 cells: accounts 1..=12 two groups, 13..=20 one.
    let mut cells = Vec::new();
    for a in 1..=20u8 {
        let k = if a <= 12 { 2 } else { 1 };
        for j in 1..=k {
            cells.push(cell(a, a, GroupId(u32::from(a) << 24 | j)));
        }
    }
    assert_eq!(cells.len(), 32);
    w.ok(ModeInput::GroupsChanged {
        added: &cells,
        removed: &[],
    });
    for a in 1..=20u8 {
        w.release(a);
        w.ok(ModeInput::SetMode {
            scope: sc_cell(a, a, GROUP_ALL),
            mode: RunMode::Live,
        });
    }
    w.out = ModeOutcome::with_capacity(32, 20, 40);
    w.mm.sync(&w.risk);
    let mut allocs = 0u64;
    {
        let sc = CountingAlloc::scope();
        let s = w.seq;
        let r = w.risk.mode.apply(kill_global(true), s, &w.mm, &mut w.out);
        allocs += sc.allocs();
        assert_eq!(r, Ok(()));
    }
    w.seq += 1;
    assert_eq!(w.out.cells.len(), 32, "step 1 cells");
    assert_eq!(w.out.risk_events.len(), 20, "step 1 risk_events");
    assert_eq!(w.out.notes.len(), 1, "step 1 notes");
    for a in 1..=20u8 {
        w.ok(auto(CapSource::RECOVERY, sc_acct(a)));
        w.ok(ModeInput::RecoveryFlags {
            account: acc(a),
            flags: RecoveryFlags(0),
        });
        let steps: Vec<Step> = w
            .release_steps(a)
            .into_iter()
            .filter(|s| !matches!(s, Step::Clock(_)))
            .collect();
        for st in &steps {
            if let Step::Cfg(h, _, _) = st {
                w.risk.on_account_cfg(h, &[]);
            }
        }
        w.mm.sync(&w.risk);
        let sc = CountingAlloc::scope();
        for st in &steps {
            w.step(a, st).unwrap();
        }
        allocs += sc.allocs();
    }
    w.mm.sync(&w.risk);
    {
        let sc = CountingAlloc::scope();
        let s = w.seq;
        let r = w.risk.mode.apply(
            ModeInput::Gate(GateEvent::Clock { ok: true }),
            s,
            &w.mm,
            &mut w.out,
        );
        allocs += sc.allocs();
        assert_eq!(r, Ok(()));
    }
    let mut rel: Vec<u8> = w.out.released.iter().map(|a| a.0).collect();
    rel.sort_unstable();
    assert_eq!(rel, (1..=20).collect::<Vec<u8>>(), "step 2 released");
    let mut gc: Vec<u8> = w.out.gate_changes.iter().map(|g| g.account.0).collect();
    gc.sort_unstable();
    assert_eq!(gc, (1..=20).collect::<Vec<u8>>(), "step 2 gate_changes");
    assert_eq!(allocs, 0, "allocations");
}

// ------------------------------------------------------------------------------------ RSK-090

/// Runs the release steps of `a`, returning the outcome of the releasing apply; asserts the
/// Recovery cap is present before it.
fn release_and_find(w: &mut W, a: u8) -> ModeOutcome {
    let steps = w.release_steps(a);
    let mut rel = None;
    for s in &steps {
        let had = w.cap(CapSource::RECOVERY, sc_acct(a)).is_some();
        w.step(a, s).unwrap();
        if w.out.released.iter().any(|x| *x == acc(a)) {
            rel = Some(w.out.clone());
            break;
        }
        assert!(had, "Recovery missing before release at {s:?}");
    }
    rel.expect("released")
}

#[test]
fn rsk_090_clean_drain_auto_resume() {
    let mut w = g0_flags(RecoveryFlags::CLEAN_DRAIN, MC);
    let v = w.risk.mode.ctl_state_version();
    let o = release_and_find(&mut w, 1);
    assert_eq!(o.released.as_slice(), &[acc(1)]);
    assert!(w.cap(CapSource::RECOVERY, sc_acct(1)).is_none());
    assert_eq!(levels(&o, N_CAP_CLEARED), vec![NoteLevel::Info]);
    assert!(events(&o).contains(&(1, false, CapSource::RECOVERY)));
    let cs = sorted_cells(&o);
    assert!(
        cs.contains(&(c1(), CLOSE, LIVE)) && cs.contains(&(c3(), CLOSE, LIVE)),
        "{cs:?}"
    );
    assert_eq!(w.risk.mode.ctl_state_version(), v + 1);
    assert!(!o.resolve_broken);
}

#[test]
fn rsk_091_unclean_keeps_recovery() {
    use RecoveryFlags as F;
    let off = hft_risk::ModeCfg {
        auto_resume_after_clean_drain: false,
        ..MC
    };
    for (id, flags, mc) in [
        ("RSK-091.1", 0, MC),
        ("RSK-091.2", F::CLEAN_DRAIN | F::TAIL_LOST, MC),
        ("RSK-091.3", F::CLEAN_DRAIN | F::RECOVER_MISMATCH, MC),
        ("RSK-091.4", F::CLEAN_DRAIN | F::POISON, MC),
        ("RSK-091.5", F::CLEAN_DRAIN | F::FROM_SNAPSHOT, MC),
        ("RSK-091.6", F::CLEAN_DRAIN, off),
    ] {
        let mut w = g0_flags(flags, mc);
        let o = release_and_find(&mut w, 1);
        assert_eq!(o.released.as_slice(), &[acc(1)], "{id}");
        assert!(w.cap(CapSource::RECOVERY, sc_acct(1)).is_some(), "{id}");
        assert!(levels(&o, N_CAP_CLEARED).is_empty(), "{id}");
        assert!(levels(&o, N_RECOVERY_HELD_SIM).is_empty(), "{id}");
        if id == "RSK-091.1" {
            assert_eq!(
                w.apply(resume(CapSource::RECOVERY, sc_acct(1))),
                Ok(()),
                "RSK-091.7"
            );
            assert!(w.cap(CapSource::RECOVERY, sc_acct(1)).is_none(), "RSK-091.7");
        }
    }
}

#[test]
fn rsk_092_stop_marker_kill_survives() {
    let mut w = g0_flags(RecoveryFlags::CLEAN_DRAIN, MC);
    w.ok(kill_acct(1, false));
    release_and_find(&mut w, 1);
    assert!(w.cap(CapSource::RECOVERY, sc_acct(1)).is_none());
    assert!(w.cap(CapSource::KILL, sc_acct(1)).is_some());
    assert_eq!(w.eval(&open_a1()).map(|_| ()), Err(RejectBits::KILLED));
}

#[test]
fn rsk_093_sim_instance_recovery() {
    use RecoveryFlags as F;
    let mut sp = r0_spec();
    sp.accounts = vec![account_spec(1, true, 1, &[A, B])];
    sp.instances.truncate(1);
    sp.sims = vec![hft_refdata::SimSpec {
        account: acc(1),
        fee_tier: 0,
        latency_model: 0,
        margin_model: 0,
    }];
    let rd = load(&sp);
    let off = hft_risk::ModeCfg {
        auto_resume_after_clean_drain: false,
        ..MC
    };
    for (id, flags, mc, removed, held) in [
        ("RSK-093.1", 0, MC, true, None),
        ("RSK-093.2", F::TAIL_LOST, MC, true, None),
        ("RSK-093.3", F::FROM_SNAPSHOT | F::TAIL_LOST, MC, true, None),
        ("RSK-093.4", F::RECOVER_MISMATCH, MC, false, Some(2u64)),
        ("RSK-093.5", F::POISON | F::TAIL_LOST, MC, false, Some(5)),
        ("RSK-093.6", 0, off, false, None),
        ("RSK-093.7", F::CLEAN_DRAIN, MC, true, None),
    ] {
        let mut w = world(rd, p0_for(rd, 1), mc, CAPS);
        w.ok(auto(CapSource::RECOVERY, sc_acct(1)));
        w.ok(ModeInput::RecoveryFlags {
            account: acc(1),
            flags: RecoveryFlags(flags),
        });
        let o = release_and_find(&mut w, 1);
        assert_eq!(w.cap(CapSource::RECOVERY, sc_acct(1)).is_none(), removed, "{id}");
        if removed {
            assert_eq!(levels(&o, N_CAP_CLEARED).len(), 1, "{id} CapCleared");
        }
        let hs: Vec<_> = o
            .notes
            .iter()
            .filter(|x| note_n(x) == N_RECOVERY_HELD_SIM)
            .copied()
            .collect();
        match held {
            Some(v1) => {
                assert_eq!(hs.len(), 1, "{id}");
                assert_eq!(
                    (hs[0].level, hs[0].v[0], hs[0].v[1]),
                    (NoteLevel::P1, 1, v1),
                    "{id}"
                );
            }
            None => assert!(hs.is_empty(), "{id}"),
        }
    }
}

#[test]
fn rsk_094_mixed_instance_no_sim_rule() {
    let mut w = r0();
    for a in [3u8, 1] {
        w.ok(auto(CapSource::RECOVERY, sc_acct(a)));
        w.ok(ModeInput::RecoveryFlags {
            account: acc(a),
            flags: RecoveryFlags(RecoveryFlags::TAIL_LOST),
        });
    }
    for a in [3u8, 1] {
        let o = release_and_find(&mut w, a);
        assert!(w.cap(CapSource::RECOVERY, sc_acct(a)).is_some(), "account {a}");
        assert!(levels(&o, N_RECOVERY_HELD_SIM).is_empty(), "account {a}");
    }
}

// ------------------------------------------------------------------------------------ RSK-140

fn kb(w: &W) -> [u32; 3] {
    let b = w.risk.mode.kill_bits();
    [b[0].0, b[1].0, b[2].0]
}

#[test]
fn rsk_140_kill_bits_projection() {
    let w = r0();
    assert_eq!(kb(&w), [0, 0, 0], "RSK-140.1");
    let mut b_all = sc_any();
    b_all.venue = B;
    let rows: [(&str, Vec<ModeInput<'static>>, [u32; 3], Option<bool>); 12] = [
        ("RSK-140.2", vec![kill_global(false)], [1, 1, 1], Some(true)),
        ("RSK-140.3", vec![kill_global(true)], [5, 5, 5], Some(true)),
        (
            "RSK-140.4",
            vec![kill_acct(1, false)],
            [0x100, 0x100, 0x100],
            Some(true),
        ),
        (
            "RSK-140.5",
            vec![kill_acct(2, false)],
            [0x200, 0x200, 0],
            Some(true),
        ),
        (
            "RSK-140.6",
            vec![kill_acct(1, true)],
            [0x0010_0100; 3],
            Some(true),
        ),
        (
            "RSK-140.7",
            vec![kill_acct(2, true)],
            [0x0020_0200, 0x0020_0200, 0],
            Some(true),
        ),
        (
            "RSK-140.8",
            vec![kill(KillLevel::Venue, b_all, true, false, false)],
            [0, 2, 0],
            Some(true),
        ),
        ("RSK-140.9", vec![kill_venue(B, 2)], [0, 0x200, 0], Some(true)),
        ("RSK-140.10", vec![kill_acct(3, false)], [0, 0, 0], Some(false)),
        (
            "RSK-140.11",
            vec![kill_global(false), kill_acct(1, true)],
            [0x0010_0101; 3],
            None,
        ),
        (
            "RSK-140.13",
            vec![
                daily(1),
                margin_auto(1, 1, 1 << B.0),
                ModeInput::Storage {
                    kind: StorageKind::PersistLag,
                    level: 1,
                },
            ],
            [0, 0, 0],
            Some(false),
        ),
        (
            "RSK-140.12",
            vec![kill_global(false), kill_acct(1, true)],
            [0x0010_0100; 3],
            Some(true),
        ),
    ];
    for (id, inputs, want, changed) in rows {
        let mut w = r0();
        let mut any_changed = false;
        for i in inputs {
            w.ok(i);
            any_changed |= w.out.kill_changed;
        }
        if id == "RSK-140.12" {
            w.ok(resume(CapSource::KILL, sc_any()));
            any_changed = w.out.kill_changed;
        }
        assert_eq!(kb(&w), want, "{id}");
        if let Some(c) = changed {
            assert_eq!(any_changed, c, "{id} kill_changed");
        }
    }
    let _ = KillBits::GLOBAL;
}

#[test]
fn rsk_141_kill_levels_and_cancels() {
    let one = |w: &W| -> Vec<(CancelScope, bool)> {
        w.out.cancels.iter().map(|c| (c.scope, c.open_only)).collect()
    };
    let mut w = r0();
    w.ok(kill(
        KillLevel::Venue,
        scope(0, B, 1, GROUP_ALL, InstId(0)),
        true,
        false,
        false,
    ));
    assert_eq!(
        one(&w),
        vec![(CancelScope::VenueAccount(B, acc(1)), true)],
        "RSK-141.1"
    );
    assert!(w.out.cells.is_empty(), "RSK-141.1 cells");
    let mut w = r0();
    w.ok(kill(
        KillLevel::Venue,
        scope(0, B, 1, GROUP_ALL, InstId(0)),
        false,
        false,
        false,
    ));
    assert!(one(&w).is_empty(), "RSK-141.2");
    let mut w = r0();
    assert_eq!(
        w.apply(kill(
            KillLevel::Venue,
            scope(0, B, 1, GROUP_ALL, InstId(0)),
            true,
            false,
            true
        )),
        Err(ModeErr::InvalidScope),
        "RSK-141.3"
    );
    let mut w = r0();
    w.ok(kill_acct(1, false));
    assert_eq!(one(&w), vec![(CancelScope::Account(acc(1)), true)], "RSK-141.4");
    assert_eq!(w.out.cells.len(), 2, "RSK-141.4 cells");
    let mut w = r0();
    w.ok(kill_acct(1, true));
    assert_eq!(one(&w), vec![(CancelScope::Account(acc(1)), false)], "RSK-141.5");
    let mut w = r0();
    w.ok(kill_global(false));
    assert_eq!(one(&w), vec![(CancelScope::Global, true)], "RSK-141.6 a");
    let mut w = r0();
    w.ok(kill_global(true));
    assert_eq!(one(&w), vec![(CancelScope::Global, false)], "RSK-141.6 b");
    let mut w = r0();
    assert_eq!(
        w.apply(kill(KillLevel::Global, sc_any(), true, true, false)),
        Err(ModeErr::InvalidScope),
        "RSK-141.7"
    );
    let mut w = r0();
    w.ok(set_mode(1, 1, G1, RunMode::CloseOnly));
    assert_eq!(one(&w), vec![(CancelScope::Cell(c1()), true)], "RSK-141.8");
    let mut w = r0();
    w.delist(B, B1);
    assert_eq!(one(&w), vec![(CancelScope::Inst(B, B1), true)], "RSK-141.9");
    assert!(w.out.cells.is_empty(), "RSK-141.9 cells");
    for (k, inp) in [
        auto(CapSource::RECON, sc_av(1, B)),
        acfg(1, B, cfg_params::MISMATCH),
    ]
    .into_iter()
    .enumerate()
    {
        let mut w = r0();
        w.ok(inp);
        assert!(one(&w).is_empty(), "RSK-141.10 sub {k}");
    }
}

#[test]
fn rsk_142_flatten_lock() {
    let mut w = r0();
    w.mv.set_pos(1, B1, "-0.02");
    w.ok(kill(KillLevel::Account, sc_acct(1), true, true, false));
    let now = w.nowv();
    assert!(w.risk.view(acc(1), &now).flatten_lock);
    assert!(!w.risk.view(acc(2), &now).flatten_lock);
    w.ok(kill_acct(1, false));
    assert!(w.risk.view(acc(1), &now).flatten_lock, "merged FLATTEN kept");
    assert_eq!(w.eval(&open_a1()).map(|_| ()), Err(RejectBits::KILLED));
    let five = [
        w.reduce(1, G1, B1, Side::Buy, "0.01", "50005.0"),
        w.flatten(1, G1, B1, Side::Buy, "0.01", "50005.0", Origin::Exec),
        hedge(1, G1, B1, Side::Sell, "0.01", "49995.0", "0.01"),
        hedge_fix(1, G1, B1, Side::Buy, "0.01", "50005.0", "0.01"),
        w.flatten(1, G1, B1, Side::Buy, "0.01", "50005.0", Origin::Exec),
    ];
    for (k, it) in five.iter().enumerate() {
        assert!(w.eval(it).is_ok(), "step 3 sub {k}: {:?}", w.eval(it));
    }
    assert!(
        w.eval(&open(2, G2, A1, Side::Buy, "0.01", "50000.00", Tif::Gtc))
            .is_ok()
    );
    assert_eq!(w.apply(resume(CapSource::KILL, sc_acct(1))), Ok(()));
    assert!(!w.risk.view(acc(1), &now).flatten_lock);
    assert!(events(&w.out).contains(&(1, false, CapSource::KILL)));
    assert_eq!(levels(&w.out, N_CAP_CLEARED).len(), 1);
}

#[test]
fn rsk_210_clock_bad_cap() {
    let mut w = r0();
    let bits = w.risk.mode.gate_bits(acc(1));
    w.clock_bad();
    let c = w.cap(CapSource::CLOCK_BAD, sc_any()).expect("ClockBad");
    assert_eq!(c.cflags, CapRecord::CONDITIONAL);
    assert_eq!(levels(&w.out, N_CAP_SET), vec![NoteLevel::P1]);
    assert_eq!(w.out.cells.len(), 4);
    assert!(w.out.cells.iter().all(|c| c.2 == CLOSE));
    assert_eq!(w.risk.mode.gate_bits(acc(1)), bits);
    let v = w.risk.mode.ctl_state_version();
    w.clock_bad();
    assert!(empty_outcome(&w.out), "step 2");
    assert_eq!(w.risk.mode.ctl_state_version(), v);
    w.ok(ModeInput::Clock { ok: true });
    assert!(w.cap(CapSource::CLOCK_BAD, sc_any()).is_none());
    assert_eq!(levels(&w.out, N_CAP_CLEARED), vec![NoteLevel::Info]);
    assert_eq!(w.out.cells.len(), 4);
    assert!(w.out.cells.iter().all(|c| c.2 == LIVE));
    let v = w.risk.mode.ctl_state_version();
    w.ok(ModeInput::Clock { ok: true });
    assert!(empty_outcome(&w.out), "step 4");
    assert_eq!(w.risk.mode.ctl_state_version(), v);
}

#[test]
fn rsk_223_delisting() {
    let mut w = r0();
    w.delist(B, B1);
    assert_eq!(levels(&w.out, N_CAP_SET), vec![NoteLevel::Info]);
    assert!(w.risk.mode.inst_blocked(B, B1));
    let cs: Vec<_> = w.out.cancels.iter().map(|c| (c.scope, c.open_only)).collect();
    assert_eq!(cs, vec![(CancelScope::Inst(B, B1), true)]);
    assert!(w.out.cells.is_empty());
    let v = w.risk.mode.ctl_state_version();
    let ncaps = w.risk.mode.caps().len();
    for (kind, inst) in [
        (VenueNoticeCmd::FUNDING_INTERVAL, B1),
        (VenueNoticeCmd::MAINTENANCE, InstId(0)),
    ] {
        let cmd = VenueNoticeCmd {
            venue: B,
            kind,
            inst,
            a: 1,
            b: 2,
        };
        w.ok(ModeInput::Notice(&cmd));
        assert!(empty_outcome(&w.out), "step 2 kind {kind}");
        assert_eq!(w.risk.mode.caps().len(), ncaps);
        assert_eq!(w.risk.mode.ctl_state_version(), v);
    }
    assert_eq!(w.apply(resume(CapSource::DELISTING, sc_vi(B, B1))), Ok(()));
    assert!(!w.risk.mode.inst_blocked(B, B1));
    assert_eq!(levels(&w.out, N_CAP_CLEARED).len(), 1);
    w.delist(B, B1);
    let snap = w.snapshot();
    // refdata without b1: only a1 and a2 remain (ids stay dense).
    let mut sp = r0_spec();
    sp.insts.retain(|i| i.id == A1 || i.id == A2);
    let rd2 = load(&sp);
    let mut r = hft_strategy_sdk::SnapshotReader::new(&snap);
    let risk = Risk::restore(
        &mut r,
        &p0_for(rd2, 3),
        rd2,
        &gws_for(rd2),
        &CAPS,
        &exec_bounds(3),
        MC,
    )
    .unwrap();
    assert!(!risk.mode.caps().iter().any(|c| c.source == CapSource::DELISTING));
    let _ = ReconcileGroup { items: &[] };
}
