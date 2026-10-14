//! Existing trading DST world: real engine, matcher, account truth and full replay checks.
use hft_event::{CmdHdr, CmdScope, ControlCmd, EventKind, GwOp, OrderEvent, OrderEventKind};
use hft_testkit::dst::world::{ACCT_A, ACCT_B, P1, UTC0, VP};
use hft_testkit::dst::{DstConfig, RerunPolicy, SeqSut, WorldSpec, run_record};
use hft_types::{AccountId, CapSource, InstId, KillLevel, StrategyId, VenueId};
type Engine = SeqSut<hft_testkit::strategies::TestStrategies>;

fn config(seed: u64) -> DstConfig {
    let mut cfg = DstConfig::new(seed, "core");
    cfg.world = WorldSpec::w1_trading(true);
    cfg.steps = 2500;
    cfg.horizon_ns = 30_000_000_000;
    cfg.rerun = RerunPolicy::Always;
    cfg
}
fn scope(account: AccountId) -> CmdScope {
    CmdScope {
        strategy: StrategyId(0),
        venue: VenueId(0),
        account,
        _pad: 0,
        group: hft_types::GROUP_ALL,
        inst: InstId(0),
        _pad2: 0,
    }
}
fn command(cfg: &mut DstConfig, at: u64, id: u64, cmd: &ControlCmd<'_>) {
    cfg.confirmed_commands.push((
        at,
        hft_testkit::dst::actors::encode_cmd(
            cmd,
            "matrix",
            CmdHdr::ROLE_ADMIN,
            AccountId(0),
            UTC0 as u64 + at,
            id,
        ),
    ));
}
fn checked(cfg: &DstConfig) -> hft_testkit::dst::RunRecord {
    let run = run_record::<Engine>(cfg);
    assert!(run.outcome.is_pass(), "seed {}: {:?}", cfg.seed, run.outcome);
    assert!(run.journal_verification.is_some(), "full journal replay must run");
    assert!(
        run.journal
            .iter()
            .filter_map(|r| r.try_read::<OrderEvent>())
            .any(|e| e.what == OrderEventKind::Fill),
        "the world must actually trade"
    );
    run
}
#[test]
fn core_313_w1_hard_kill_account_isolation() {
    for seed in [1, 7, 19] {
        let mut cfg = config(seed);
        command(
            &mut cfg,
            1_000_000_000,
            901,
            &ControlCmd::Kill {
                scope: scope(ACCT_A),
                level: KillLevel::Account,
                flatten: false,
                cancel_resting: true,
                hard: true,
            },
        );
        let run = checked(&cfg);
        let kill_seq = run
            .journal
            .iter()
            .find(|r| {
                r.kind == EventKind::CONTROL_CMD
                    && matches!(
                        ControlCmd::decode(&r.body),
                        Ok((_, ControlCmd::Kill { hard: true, .. }))
                    )
            })
            .unwrap()
            .seq;
        assert!(
            !run.frames.iter().any(|(_, f)| f.trigger_seq >= kill_seq
                && f.cmd.account == ACCT_A
                && f.cmd.op == GwOp::Place)
        );
        assert!(
            run.frames
                .iter()
                .any(|(_, f)| f.trigger_seq > kill_seq && f.cmd.account == ACCT_B && f.cmd.op == GwOp::Place)
        );
        assert!(run.obs.caps.iter().any(|c| c.source == CapSource::KILL.0
            && c.scope == hft_testkit::dst::obs::CapScope::Account(ACCT_A)
            && c.hard));
    }
}
#[test]
fn c15_w1_delisting_and_resume_matrix() {
    for seed in [1, 7, 19] {
        let mut cfg = config(seed);
        command(
            &mut cfg,
            1_000_000_000,
            902,
            &ControlCmd::VenueNotice {
                venue: VP,
                kind: 1,
                _pad: 0,
                inst: P1,
                a: UTC0 + 7 * 86_400_000_000_000,
                b: 0,
            },
        );
        let mut s = scope(AccountId(0));
        s.venue = VP;
        s.inst = P1;
        command(
            &mut cfg,
            3_000_000_000,
            903,
            &ControlCmd::Resume {
                scope: s,
                source: CapSource::DELISTING,
            },
        );
        let run = checked(&cfg);
        let results: Vec<_> = run
            .journal
            .iter()
            .filter_map(|r| r.try_read::<hft_event::CmdResult>())
            .filter(|r| [902, 903].contains(&u64::from_le_bytes(r.cmd_id[..8].try_into().unwrap())))
            .collect();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.status == 0), "{results:?}");
        assert!(!run.obs.caps.iter().any(|c| c.source == CapSource::DELISTING.0));
        // The world may withhold intents while capped. Assert the actual cap lifecycle;
        // safety_scenarios independently submits Open/Hedge/Reduce against this cap.
        let lifecycle: Vec<_> = run
            .journal
            .iter()
            .filter_map(|r| r.try_read::<hft_event::Note>())
            .filter(|n| n.source == hft_event::NoteSource::Risk && [2, 3].contains(&(n.code & 0xffff)))
            .filter(|n| hft_risk::cap_of_note(n).source == CapSource::DELISTING)
            .map(|n| n.code & 0xffff)
            .collect();
        assert_eq!(lifecycle, [2, 3]);
    }
}
