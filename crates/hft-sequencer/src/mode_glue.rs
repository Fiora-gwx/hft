//! Mode and cap driving (README "模式与 cap 的驱动"): Inputs → `ModeInput`, `risk.apply`, and the
//! execution of `ModeOutcome` in the order ①–⑧.

use hft_event::{CapRecord, CmdScope, NoteLevel, RecordFlags};
use hft_risk::mode::{CancelScope, CellKey, DrainCause, Effective, ModeErr, ModeInput, ModeOutcome};
use hft_strategy_sdk::StrategySet;
use hft_types::{AccountId, CapSource, GroupId, InstId, KillBits, RunMode, VenueId};

use crate::Driver;
use crate::codes::note;
use crate::engine::Engine;
use crate::notify::Notif;
use crate::strategies::Instances;
use crate::types::FatalReason;
use crate::views::ModeAdapter;

type R<T> = Result<T, FatalReason>;

/// An `Auto` cap record (level and flags per hft-risk conventions).
pub(crate) fn auto_rec(source: CapSource, scope: CmdScope, seq: u64, p0: u64) -> CapRecord {
    let cflags = match source {
        CapSource::CLOCK_BAD | CapSource::MARGIN_STRESS | CapSource::STORAGE | CapSource::ACCOUNT_CFG => {
            CapRecord::CONDITIONAL
        }
        CapSource::DRAINING => CapRecord::NON_CLEARABLE,
        _ => 0,
    };
    CapRecord {
        source,
        level: 0,
        cflags,
        _pad: [0; 5],
        scope,
        set_seq: seq,
        params: [p0, 0],
    }
}

pub(crate) fn eff_mode(x: Effective) -> RunMode {
    match x {
        Effective::Shadow => RunMode::Shadow,
        Effective::CloseOnly { .. } => RunMode::CloseOnly,
        Effective::Live => RunMode::Live,
    }
}

fn input_tag(inp: &ModeInput<'_>) -> u64 {
    match inp {
        ModeInput::SetMode { .. } => 1,
        ModeInput::Kill(_) => 2,
        ModeInput::Resume { .. } => 3,
        ModeInput::Auto(_) => 4,
        ModeInput::AutoClear { .. } => 5,
        ModeInput::Clock { .. } => 6,
        ModeInput::Storage { .. } => 7,
        ModeInput::StorageRecheck => 8,
        ModeInput::Notice(_) => 9,
        ModeInput::Gate(_) => 10,
        ModeInput::CellFlat { .. } => 11,
        ModeInput::GroupsChanged { .. } => 12,
        ModeInput::RecoveryFlags { .. } => 13,
    }
}

/// `risk.apply` + outcome execution. `Ok(Err(e))` = the ModeErr of a command.
pub(crate) fn apply_mode_r<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    inp: ModeInput<'_>,
) -> R<Result<(), ModeErr>> {
    let mut out = e.take_mode();
    let tag = input_tag(&inp);
    let r = {
        let ad = ModeAdapter {
            oms: &e.oms,
            ledger: &e.ledger,
            exec: &e.exec,
        };
        e.risk.apply(inp, e.cur_seq, &ad, &mut out)
    };
    e.trace.rec("risk.apply", tag, u64::from(r.is_ok()));
    if r.is_ok() {
        execute(e, inst, drv, &out)?;
    }
    e.put_mode(out);
    Ok(r)
}

/// `risk.apply` + execution, ignoring the ModeErr (non-command inputs).
pub(crate) fn apply_mode<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    inp: ModeInput<'_>,
) -> R<()> {
    apply_mode_r(e, inst, drv, inp).map(|_| ())
}

/// `apply(Auto(Broken{a, g}))` then `on_legs(a, g)` in the same Input (README "process" 7).
pub(crate) fn broken<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    g: GroupId,
) -> R<()> {
    let rec = auto_rec(
        CapSource::BROKEN,
        hft_risk::account_group_scope(a, g),
        e.cur_seq,
        0,
    );
    e.trace.rec("apply.Broken", u64::from(a.0), u64::from(g.0));
    apply_mode(e, inst, drv, ModeInput::Auto(rec))?;
    on_legs(e, inst, drv, a, g)
}

/// `risk.on_legs(a, g, LegSummary)`; `AutoClear{Broken}` → apply.
pub(crate) fn on_legs<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    g: GroupId,
) -> R<()> {
    if g.0 == 0 {
        return Ok(());
    }
    let spec = inst.spec(a, g).copied();
    let s = crate::views::leg_summary(&e.oms, &e.market, &e.rd, a, g, spec.as_ref());
    e.trace.rec("risk.on_legs", u64::from(a.0), u64::from(g.0));
    if let Some(inp) = e.risk.on_legs(a, g, &s) {
        apply_mode(e, inst, drv, inp)?;
    }
    Ok(())
}

/// Executes one outcome in the order ①–⑧.
pub(crate) fn execute<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    out: &ModeOutcome,
) -> R<()> {
    // ① cells: Mode events (live instances first), ledger shadow flag, ShadowDeferred notes.
    let mut cells: Vec<(CellKey, Effective, Effective)> = out.cells.as_slice().to_vec();
    cells.sort_by_key(|c| {
        inst.ix(c.0.strategy)
            .and_then(|i| inst.dispatch_pos.get(i).copied())
            .unwrap_or(u16::MAX)
    });
    for (k, old, new) in cells.iter() {
        let (om, nm) = (eff_mode(*old), eff_mode(*new));
        if om != nm
            && let Some(ix) = inst.ix(k.strategy)
        {
            let gix = inst
                .slots
                .get(ix)
                .and_then(|s| s.gix_of(k.group))
                .unwrap_or(hft_strategy_sdk::GIX_NONE);
            e.trace
                .rec("queue.mode", u64::from(k.strategy.0), u64::from(k.group.0));
            e.notify.push(
                ix as u16,
                Notif::Mode {
                    gix,
                    old: om,
                    new: nm,
                },
            );
        }
        let was_shadow = matches!(old, Effective::Shadow);
        let is_shadow = matches!(new, Effective::Shadow);
        if was_shadow != is_shadow {
            e.trace.rec(
                "ledger.set_cell_shadow",
                u64::from(k.account.0),
                u64::from(is_shadow),
            );
            e.ledger.set_cell_shadow(k.account, k.group, is_shadow);
        }
        let deferred_now = matches!(
            new,
            Effective::CloseOnly {
                deferred_shadow: true
            }
        );
        let landed = matches!(
            old,
            Effective::CloseOnly {
                deferred_shadow: true
            }
        ) && is_shadow;
        if (deferred_now
            && !matches!(
                old,
                Effective::CloseOnly {
                    deferred_shadow: true
                }
            ))
            || landed
        {
            e.seq_note(
                note::SHADOW_DEFERRED,
                NoteLevel::Info,
                k.group,
                InstId(0),
                [
                    u64::from(k.strategy.0) | (u64::from(k.account.0) << 8),
                    u64::from(k.group.0),
                    u64::from(landed),
                    0,
                    0,
                    0,
                    0,
                    0,
                ],
            );
        }
    }
    // ② cancels.
    for c in out.cancels.as_slice().iter() {
        let mut o = e.take_oms();
        match c.scope {
            CancelScope::Cell(k) => {
                e.trace
                    .rec("oms.cancel_open", u64::from(k.account.0), u64::from(k.group.0));
                e.oms.cancel_open(
                    hft_oms::CellKey {
                        strategy: k.strategy,
                        account: k.account,
                        group: k.group,
                    },
                    e.now.tsc,
                    e.cur_seq,
                    &mut o,
                );
            }
            sc => {
                e.trace.rec("oms.cancel_all", u64::from(c.open_only), 0);
                e.oms
                    .cancel_all(oms_scope(sc), c.open_only, e.now.tsc, e.cur_seq, &mut o);
            }
        }
        crate::intents::drain_oms(e, inst, drv, &mut o)?;
        e.put_oms(o);
    }
    // ③ kill words.
    if out.kill_changed {
        let bits: Vec<KillBits> = e.risk.mode.kill_bits().to_vec();
        e.trace.rec("kill_words", bits.len() as u64, 0);
        drv.write_kill(&bits);
    }
    // ④ resolve_broken (HARD Kill record deleted).
    if out.resolve_broken {
        e.naked_scan = Some((AccountId(1), 0));
        crate::process::naked_scan(e, inst, drv)?;
    }
    // ⑤ gates.
    for (a, v) in out.hedge_gate_open.as_slice().iter() {
        e.trace
            .rec("oms.on_hedge_gate_open", u64::from(a.0), u64::from(v.0));
        let mut o = e.take_oms();
        e.oms.on_hedge_gate_open(*a, *v, e.now.tsc, e.cur_seq, &mut o);
        crate::intents::drain_oms(e, inst, drv, &mut o)?;
        e.put_oms(o);
    }
    for (a, v) in out.venue_gate_open.as_slice().iter() {
        e.trace
            .rec("exec.on_venue_gate_open", u64::from(a.0), u64::from(v.0));
        e.exec.on_venue_gate_open(*a, *v, e.now);
    }
    for a in out.released.as_slice().iter() {
        e.trace.rec("exec.on_gate_open", u64::from(a.0), 0);
        e.exec.on_gate_open(*a, e.now);
    }
    // ⑥ risk events → bound instance + exec.on_risk; effective-mode changes → exec.on_mode.
    for (a, ev) in out.risk_events.as_slice().iter() {
        if let Some(ix) = inst.ix_of_account(*a) {
            e.trace.rec("queue.risk", u64::from(a.0), 0);
            e.notify.push(ix as u16, Notif::Risk { a: *a, e: *ev });
        }
        e.trace.rec("exec.on_risk", u64::from(a.0), 0);
        e.exec.on_risk(*a, ev, e.now);
    }
    for (k, old, new) in cells.iter() {
        let (om, nm) = (eff_mode(*old), eff_mode(*new));
        if om != nm {
            crate::exec_glue::on_mode(e, inst, drv, k.account, k.group, om, nm)?;
        }
    }
    for a in out.daily_loss_resumed.as_slice().iter() {
        let day = e.ledger.account(*a).day.net_realized();
        e.risk.on_daily_loss_resumed(*a, day);
    }
    // ⑦ internal Drain.
    if let Some(d) = out.drain {
        match d {
            DrainCause::StorageCritical => {
                crate::commands::start_drain(e, inst, drv, true, crate::types::DoneCause::StorageCritical)?;
            }
            DrainCause::IdentityMismatch { account, venue } => {
                e.seq_note(
                    note::IDENTITY_MISMATCH,
                    NoteLevel::P0,
                    GroupId(0),
                    InstId(0),
                    [
                        u64::from(venue.0) | (u64::from(account.0) << 8),
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                );
                crate::commands::start_drain(e, inst, drv, true, crate::types::DoneCause::IdentityMismatch)?;
            }
        }
    }
    // ⑧ gate changes and cap notes.
    for g in out.gate_changes.as_slice().iter() {
        e.seq_note(
            note::STARTUP_GATE,
            NoteLevel::Info,
            GroupId(0),
            InstId(0),
            [
                u64::from(g.account.0),
                u64::from(g.old_bits),
                u64::from(g.new_bits),
                u64::from(g.released),
                0,
                0,
                0,
                0,
            ],
        );
    }
    for n in out.notes.as_slice().iter() {
        e.out.push(RecordFlags(0), n);
    }
    Ok(())
}

/// hft-risk `CancelScope` → hft-oms `CancelScope` (same values).
pub(crate) fn oms_scope(s: CancelScope) -> hft_oms::CancelScope {
    match s {
        CancelScope::Global => hft_oms::CancelScope::Global,
        CancelScope::Account(a) => hft_oms::CancelScope::Account(a),
        CancelScope::Venue(v) => hft_oms::CancelScope::Venue(v),
        CancelScope::VenueAccount(v, a) => hft_oms::CancelScope::VenueAccount(v, a),
        CancelScope::Cell(k) => hft_oms::CancelScope::Cell(hft_oms::CellKey {
            strategy: k.strategy,
            account: k.account,
            group: k.group,
        }),
        CancelScope::Inst(v, i) => hft_oms::CancelScope::Inst(v, i),
    }
}

/// The cancel obligation of an active `Kill{cancel_resting}` / `Draining{cancel_resting}`
/// (README "撤单持续义务"): `(scope, open_only)` for an account, if any.
pub(crate) fn cancel_obligation(e: &Engine, a: AccountId) -> Option<(hft_oms::CancelScope, bool)> {
    for c in e.risk.mode.caps() {
        let applies = c.scope.account.0 == 0 || c.scope.account == a;
        if !applies {
            continue;
        }
        if c.source == CapSource::KILL && c.params[0] & hft_risk::mode::kill_params::CANCEL_RESTING != 0 {
            let hard = c.cflags & CapRecord::HARD != 0;
            let scope = if c.scope.account.0 == 0 {
                hft_oms::CancelScope::Global
            } else {
                hft_oms::CancelScope::Account(a)
            };
            return Some((scope, !hard));
        }
        if c.source == CapSource::DRAINING && c.params[0] == 1 {
            return Some((hft_oms::CancelScope::Global, false));
        }
    }
    None
}

/// Re-issues the cancels of an active obligation after `ConnState{Up}` or Unknown → Live.
pub(crate) fn cancel_obligation_on_live<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    _v: VenueId,
) -> R<()> {
    if let Some((scope, open_only)) = cancel_obligation(e, a) {
        e.trace
            .rec("oms.reissue_cancels", u64::from(a.0), u64::from(open_only));
        let mut o = e.take_oms();
        e.oms
            .reissue_cancels(scope, open_only, e.now.tsc, e.cur_seq, &mut o);
        crate::intents::drain_oms(e, inst, drv, &mut o)?;
        e.put_oms(o);
    }
    Ok(())
}
