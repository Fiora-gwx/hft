//! Cold Model replay. Logged Advice still drives ordinary Output verification; this mirror
//! independently checks the Model output and never substitutes new Advice into the journal.

use hft_event::{AdviceHdr, EventKind, Note, NoteSource, Role, SegmentRoll};
use hft_journal::RollReason;
use hft_rt::Heartbeat;
use hft_strategy_sdk::StrategySet;
use hft_types::{AccountId, GroupId, StrategyId, Tsc};

use super::recompute_compare::Compare;
use super::replay::{Log, ReplayDriver, ReplayMode};
use super::verify::{VerifyError, VerifyOptions};
use crate::advisor::{self, ModelSlot, checkpoint};
use crate::{BootSource, EngineSetup, RecoverPlan, RecoveryFlags, Sequencer, SnapshotSel, StepOutcome};

impl From<VerifyError> for RecomputeError {
    fn from(e: VerifyError) -> Self {
        Self::Verify(e)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RecomputeLimits {
    pub records: usize,
    pub bytes: usize,
}

/// One explicitly named checkpoint; the application resolves filenames without trusting
/// the identity inside the file. Directory entries absent for an instance are omitted.
#[derive(Clone, Debug)]
pub struct ModelCheckpoint {
    pub instance: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum RecomputeError {
    Verify(VerifyError),
    CheckpointFile {
        instance: String,
        error: checkpoint::FileError,
    },
    CheckpointKey {
        instance: String,
    },
    Restore {
        strategy: StrategyId,
        seq: u64,
    },
    Advice {
        seq: u64,
    },
    Note {
        seq: u64,
    },
    Clock {
        seq: u64,
    },
    DiagnosticLimit,
}

#[derive(Clone, Debug)]
pub struct AdviceDifference {
    pub seq: u64,
    pub strategy: StrategyId,
    pub account: AccountId,
    pub group: GroupId,
    pub local_kind: u16,
    pub basis_seq: u64,
    pub logged: Vec<u8>,
    /// None means no unmatched, same-key Model output exists.
    pub regenerated: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct RecomputeResult {
    pub compared: u64,
    pub coalesced: u64,
    pub differences: Vec<AdviceDifference>,
    pub model_no_start: Vec<String>,
    pub started_instances: Vec<String>,
    pub checkpoint_keys: Vec<(String, checkpoint::Key)>,
    pub recompute_from_seq: Option<u64>,
    /// The Output verifier must warm and compare this earlier prefix when a checkpoint
    /// precedes the user's comparison window. The Advice window itself is unchanged.
    pub verify_from_seq: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelIdentity {
    pub instance: String,
    pub strategy: StrategyId,
    pub account: AccountId,
    pub kind_id: u16,
    pub schema_id: u32,
    pub layout_version: u32,
}

/// Names and identities come from recorded refdata, never a checkpoint filename or current
/// machine configuration. A kind with NoModel is excluded even when it has parameters.
pub fn model_identities<S: StrategySet>(log: &Log) -> Result<Vec<ModelIdentity>, RecomputeError> {
    super::verify::validate_kinds::<S>(log)?;
    let plan = super::verify_runs::plan(log)?;
    let mut out = Vec::new();
    for run in plan.runs {
        let at = log.recs[run.snapshot].seq;
        let setup = EngineSetup::from_snapshot(log, at).map_err(VerifyError::Boot)?;
        let (e, inst) =
            crate::recover::restore_at::<S>(log, run.snapshot, &setup, None).map_err(VerifyError::Boot)?;
        for slot in &inst.slots {
            if !S::has_model(slot.kind_id) {
                continue;
            }
            let desc = S::kinds().iter().find(|d| d.kind_id == slot.kind_id).ok_or(
                VerifyError::UnknownStrategy {
                    seq: at,
                    kind: slot.kind_id,
                },
            )?;
            let row =
                e.rd.instance(slot.id)
                    .ok_or(VerifyError::InvalidParams { seq: at })?;
            let id = ModelIdentity {
                instance: row.name.to_string(),
                strategy: slot.id,
                account: slot.account,
                kind_id: slot.kind_id,
                schema_id: desc.schema.id,
                layout_version: S::model_layout_version(slot.kind_id),
            };
            if !out.contains(&id) {
                out.push(id);
            }
        }
    }
    Ok(out)
}

struct Source {
    name: String,
    has_model: bool,
    seen_params: bool,
    checkpoint: Option<usize>,
    forbidden: bool,
    after: Option<u64>,
    skip_basis_before: u64,
}

fn mirror<S: StrategySet>(
    log: &Log,
    start: usize,
    end: usize,
    hb: &'static Heartbeat,
) -> Result<Sequencer<ReplayDriver, S>, RecomputeError> {
    let at = log.recs[start].seq;
    let setup = EngineSetup::from_snapshot(log, at).map_err(VerifyError::Boot)?;
    let mut driver = ReplayDriver::new(ReplayMode::Mirror, log.clone(), 0, 1);
    driver.end = end;
    driver.sink.record_end = end;
    let plan = RecoverPlan {
        start: SnapshotSel::AtSeq(at),
        replay_tail: true,
        epoch: 0,
        flags: RecoveryFlags(0),
        quarantine: Vec::new(),
    };
    let mut seq = Sequencer::<ReplayDriver, S>::boot(BootSource::Journal(plan), &setup, driver, hb)
        .map_err(VerifyError::Boot)?;
    seq.inst.models.on = true;
    seq.inst.models.slots = (0..seq.inst.slots.len()).map(|_| ModelSlot::default()).collect();
    seq.inst.models.prev_tick_utc = advisor::prev_tick_utc(log, start);
    Ok(seq)
}

/// Engine time of each record, including pre-EngineClock boot records after a TSC reset.
/// A restart's boot prefix uses its saved stop time; no old TSC anchor crosses that boundary.
fn timeline<S: StrategySet>(
    log: &Log,
    runs: &[super::verify_runs::RunStart],
) -> Result<Vec<i64>, RecomputeError> {
    let mut times = vec![0; log.recs.len()];
    for (ri, run) in runs.iter().enumerate() {
        let end = runs.get(ri + 1).map_or(log.recs.len(), |r| r.first);
        let at = log.recs[run.snapshot].seq;
        let setup = EngineSetup::from_snapshot(log, at).map_err(VerifyError::Boot)?;
        let (e, _) =
            crate::recover::restore_at::<S>(log, run.snapshot, &setup, None).map_err(VerifyError::Boot)?;
        let reason = log.recs[run.first].read::<SegmentRoll>().map(|r| r.reason);
        let fresh = ri > 0
            || reason.is_some_and(|r| {
                matches!(
                    RollReason::from_u8(r),
                    Some(RollReason::Startup | RollReason::Recovery | RollReason::RecoverFromSnapshot)
                )
            });
        let mut clock = (!fresh && e.clock.is_set()).then_some(e.clock);
        for ix in run.first..end {
            let r = &log.recs[ix];
            if r.kind == EventKind::ENGINE_CLOCK {
                let ec = r
                    .read::<hft_event::EngineClock>()
                    .ok_or(RecomputeError::Clock { seq: r.seq })?;
                clock = Some(
                    crate::clock::EngineClock::new(crate::clock::EngineClockRow {
                        base_engine_ns: ec.base_engine_ns,
                        base_tsc: r.tsc,
                        tsc_hz_milli: ec.tsc_hz_milli,
                        clock_seq: r.seq,
                    })
                    .map_err(|_| RecomputeError::Clock { seq: r.seq })?,
                );
            }
            times[ix] = match clock {
                Some(c) => c
                    .ens_of(Tsc(r.tsc))
                    .map_err(|_| RecomputeError::Clock { seq: r.seq })?,
                None => e.last_ens,
            };
        }
    }
    Ok(times)
}

fn rebuild(log: &Log, times: &[i64], last: u64, settle_ms: u64) -> Result<Option<u64>, RecomputeError> {
    if log.recs.last().is_none_or(|r| last > r.seq) {
        return Err(RecomputeError::Clock { seq: last });
    }
    let next = log.recs.partition_point(|r| r.seq <= last);
    let base = if next == 0
        && log
            .recs
            .first()
            .is_some_and(|r| r.seq.checked_sub(1) == Some(last))
    {
        *times.first().ok_or(RecomputeError::Clock { seq: last })?
    } else {
        *next
            .checked_sub(1)
            .and_then(|i| times.get(i))
            .ok_or(RecomputeError::Clock { seq: last })?
    };
    let due = i128::from(base) + i128::from(settle_ms) * 1_000_000;
    Ok((next..times.len())
        .find(|&i| i128::from(times[i]) >= due)
        .map(|i| log.recs[i].seq))
}

fn activate<S: StrategySet>(
    seq: &mut Sequencer<ReplayDriver, S>,
    ix: usize,
    source: &mut Source,
    c: checkpoint::Decoded<'_>,
    log: &Log,
    times: &[i64],
    result: &mut RecomputeResult,
) -> Result<(), RecomputeError> {
    let sl = &seq.inst.slots[ix];
    let s = sl.id;
    if sl.cur.is_none() {
        return Err(RecomputeError::Restore {
            strategy: s,
            seq: c.key.last_seq,
        });
    }
    let kind = sl.kind_id;
    seq.inst.models.slots[ix].enabled = true;
    seq.inst.models.slots[ix].model = None;
    advisor::on_params(&seq.e, &mut seq.inst, ix);
    let reset = advisor::restore_model::<S>(&mut seq.inst, ix, &c.header(0), c.body, kind).map_err(|_| {
        RecomputeError::Restore {
            strategy: s,
            seq: c.key.last_seq,
        }
    })?;
    // Creation, raw restore, path restore, then parameters effective at the consumed boundary.
    advisor::on_params(&seq.e, &mut seq.inst, ix);
    if reset.is_some() {
        let p = seq.inst.slots[ix].cur.as_ref().ok_or(RecomputeError::Restore {
            strategy: s,
            seq: c.key.last_seq,
        })?;
        let point = rebuild(log, times, c.key.last_seq, S::model_path_settle_ms(&p.params))?;
        source.skip_basis_before = point.unwrap_or(u64::MAX);
        result.recompute_from_seq = point;
    }
    source.after = Some(c.key.last_seq);
    source.checkpoint = None;
    result.checkpoint_keys.push((source.name.clone(), c.key));
    Ok(())
}

/// Independently execute Models over every selected process run. Checkpoints are validated
/// before any Model is called. A missing origin is reported per instance, never empty success.
pub fn recompute<S: StrategySet>(
    log: &Log,
    opts: VerifyOptions,
    checkpoints: &[ModelCheckpoint],
    limits: RecomputeLimits,
    hb: &'static Heartbeat,
) -> Result<RecomputeResult, RecomputeError> {
    let from = opts.from_seq.unwrap_or(0);
    let to = opts.to_seq.unwrap_or(u64::MAX);
    if from > to || log.recs.last().is_none_or(|r| from > r.seq) {
        return Err(VerifyError::Window.into());
    }
    if limits.records == 0 || limits.bytes == 0 {
        return Err(RecomputeError::DiagnosticLimit);
    }
    let ids = model_identities::<S>(log)?;
    let mut decoded: Vec<(&str, checkpoint::Decoded<'_>)> = Vec::new();
    for c in checkpoints {
        if decoded.iter().any(|(name, _)| *name == c.instance.as_str()) {
            return Err(RecomputeError::CheckpointKey {
                instance: c.instance.clone(),
            });
        }
        let d =
            checkpoint::decode(&c.bytes, limits.bytes).map_err(|error| RecomputeError::CheckpointFile {
                instance: c.instance.clone(),
                error,
            })?;
        let matching: Vec<_> = ids.iter().filter(|id| id.instance == c.instance).collect();
        if matching.is_empty()
            || matching.iter().any(|id| {
                (
                    id.strategy,
                    id.account,
                    id.kind_id,
                    id.schema_id,
                    id.layout_version,
                ) != (
                    d.key.strategy,
                    d.key.account,
                    d.key.kind_id,
                    d.key.schema_id,
                    d.key.layout_version,
                )
            })
        {
            return Err(RecomputeError::CheckpointKey {
                instance: c.instance.clone(),
            });
        }
        decoded.push((c.instance.as_str(), d));
    }
    let planned = super::verify_runs::plan(log)?;
    let times = timeline::<S>(log, &planned.runs)?;
    let first_seq = log.recs[0].seq;
    let last_seq = log.recs.last().ok_or(VerifyError::NoSnapshot)?.seq;
    let mut result = RecomputeResult {
        verify_from_seq: opts.from_seq,
        ..RecomputeResult::default()
    };
    let mut compare = Compare::new(limits);
    for (ri, run) in planned.runs.iter().enumerate() {
        let end = planned.runs.get(ri + 1).map_or(log.recs.len(), |r| r.first);
        if log.recs[end - 1].seq < from || log.recs[run.first].seq > to {
            continue;
        }
        let in_run =
            |last: u64| last >= log.recs[run.first].seq.saturating_sub(1) && last <= log.recs[end - 1].seq;
        let wanted = decoded
            .iter()
            .filter(|(_, d)| in_run(d.key.last_seq))
            .map(|(_, d)| d.key.last_seq.saturating_add(1))
            .fold(from, u64::min);
        if let Some(window) = result.verify_from_seq {
            result.verify_from_seq = Some(window.min(wanted));
        }
        let start = (run.first..end)
            .rev()
            .find(|&i| log.recs[i].kind == EventKind::STATE_SNAPSHOT && log.recs[i].seq <= wanted)
            .unwrap_or(run.snapshot);
        if wanted < from {
            result.verify_from_seq = Some(result.verify_from_seq.unwrap_or(from).min(log.recs[start].seq));
        }
        if log.recs[start].seq > to {
            return Err(VerifyError::NoSnapshot.into());
        }
        let compare_end = (start + 1..end)
            .find(|&i| log.recs[i].seq > to && log.recs[i].role() == Role::Input)
            .unwrap_or(end);
        let window_end = decoded
            .iter()
            .filter(|(_, d)| in_run(d.key.last_seq))
            .map(|(_, d)| {
                (start + 1..end)
                    .find(|&i| log.recs[i].seq > d.key.last_seq && log.recs[i].role() == Role::Input)
                    .unwrap_or(end)
            })
            .fold(compare_end, usize::max);
        let mut seq = mirror::<S>(log, start, window_end, hb)?;
        let startup = start == run.snapshot
            && from <= log.recs[start].seq
            && log.recs[run.first]
                .read::<SegmentRoll>()
                .is_some_and(|r| r.reason == RollReason::Startup as u8);
        let mut sources = Vec::new();
        for ix in 0..seq.inst.slots.len() {
            let sl = &seq.inst.slots[ix];
            let name = seq
                .e
                .rd
                .instance(sl.id)
                .ok_or(VerifyError::InvalidParams {
                    seq: log.recs[start].seq,
                })?
                .name
                .to_string();
            let supplied = decoded.iter().position(|(n, _)| *n == name);
            let forbidden = supplied.is_some_and(|i| {
                decoded[i].1.key.last_seq < first_seq.saturating_sub(1)
                    || decoded[i].1.key.last_seq > last_seq
            });
            let checkpoint = supplied.filter(|&i| !forbidden && in_run(decoded[i].1.key.last_seq));
            let has_model = S::has_model(sl.kind_id);
            let seen_params = sl.cur.is_some();
            let empty = has_model && !forbidden && checkpoint.is_none() && startup && seen_params;
            sources.push(Source {
                name,
                has_model,
                seen_params,
                checkpoint,
                forbidden,
                after: empty.then_some(log.recs[run.first].seq.saturating_sub(1)),
                skip_basis_before: 0,
            });
            if empty {
                seq.inst.models.slots[ix].enabled = true;
                advisor::on_params(&seq.e, &mut seq.inst, ix);
            }
        }
        for ix in start..window_end {
            let r = &log.recs[ix];
            // The mirror has applied all preceding Inputs and their complete Outputs.
            for (mi, source) in sources.iter_mut().enumerate() {
                if let Some(ci) = source.checkpoint {
                    let c = decoded[ci].1;
                    if c.key.last_seq < r.seq || (ix == start && c.key.last_seq <= r.seq) {
                        activate(&mut seq, mi, source, c, log, &times, &mut result)?;
                    }
                }
            }
            if r.kind == EventKind::NOTE {
                let note = r.read::<Note>().ok_or(RecomputeError::Note { seq: r.seq })?;
                if let Some(mi) = seq.inst.ix(note.strategy) {
                    let source = &mut sources[mi];
                    let code = note.code;
                    if source.has_model
                        && code
                            == hft_event::note_code(
                                NoteSource::Advisor,
                                crate::codes::advisor_note::MODEL_CHECKPOINT_DISCARDED,
                            )
                        && r.seq >= from
                    {
                        if note.source != NoteSource::Advisor
                            || note.v[0] != u64::from(note.strategy.0)
                            || !(1..=5).contains(&note.v[1])
                        {
                            return Err(RecomputeError::Note { seq: r.seq });
                        }
                        if !source.forbidden && source.checkpoint.is_none() {
                            let ms = &mut seq.inst.models.slots[mi];
                            ms.enabled = true;
                            ms.model = None;
                            advisor::on_params(&seq.e, &mut seq.inst, mi);
                            if seq.inst.models.slots[mi].model.is_some() {
                                source.after = Some(r.seq);
                            }
                        }
                    }
                    if source.has_model
                        && code
                            == hft_event::note_code(
                                NoteSource::Advisor,
                                crate::codes::advisor_note::MODEL_PATH_RESET,
                            )
                        && r.seq >= from
                    {
                        if note.source != NoteSource::Advisor
                            || note.v[0] != u64::from(note.strategy.0)
                            || note.v[1] >= r.seq
                        {
                            return Err(RecomputeError::Note { seq: r.seq });
                        }
                        let point = rebuild(log, &times, note.v[1], note.v[2])?;
                        source.skip_basis_before = point.unwrap_or(u64::MAX);
                        result.recompute_from_seq = point;
                    }
                }
            }
            if r.kind == EventKind::ADVICE {
                let h = r
                    .read::<AdviceHdr>()
                    .ok_or(RecomputeError::Advice { seq: r.seq })?;
                let source = seq.inst.ix(h.strategy).and_then(|i| sources.get(i));
                let eligible = source.is_none_or(|s| {
                    !s.has_model
                        || s.after
                            .is_some_and(|a| r.seq > a && h.basis_seq >= s.skip_basis_before)
                });
                compare.logged(r, r.seq > from && r.seq <= to && eligible)?;
            }
            if ix == start || r.role() != Role::Input {
                continue;
            }
            for (mi, source) in sources.iter_mut().enumerate() {
                if source.has_model
                    && !source.forbidden
                    && source.checkpoint.is_none()
                    && source.after.is_none()
                    && seq.inst.slots[mi].cur.is_none()
                    && r.seq >= from
                {
                    seq.inst.models.slots[mi].enabled = true;
                }
            }
            let before = seq.drv.handed;
            seq.drv.end = ix + 1;
            let outcome = seq.step();
            seq.drv.end = window_end;
            if seq.drv.handed != before + 1 || matches!(outcome, StepOutcome::Fatal(_)) {
                return Err(VerifyError::Unfinished { seq: r.seq, outcome }.into());
            }
            for (mi, source) in sources.iter_mut().enumerate() {
                source.seen_params |= seq.inst.slots[mi].cur.is_some();
                if source.after.is_none() && seq.inst.models.slots[mi].model.is_some() {
                    source.after = Some(r.seq);
                }
            }
            for item in core::mem::take(&mut seq.inst.models.advice) {
                let source = seq.inst.ix(item.strategy).and_then(|i| sources.get(i));
                let countable = item.basis_seq > from
                    && item.basis_seq <= to
                    && source.is_some_and(|s| {
                        s.after.is_some_and(|a| item.basis_seq > a) && item.basis_seq >= s.skip_basis_before
                    });
                compare.produced(item, countable)?;
            }
            seq.inst.models.notes.clear();
        }
        // A checkpoint at the last record is valid even when there are no following Inputs.
        for (mi, source) in sources.iter_mut().enumerate() {
            if let Some(ci) = source.checkpoint {
                let c = decoded[ci].1;
                if c.key.last_seq <= log.recs[window_end - 1].seq {
                    activate(&mut seq, mi, source, c, log, &times, &mut result)?;
                }
            }
            if source.has_model
                && source.seen_params
                && source.after.is_none()
                && !result.model_no_start.contains(&source.name)
            {
                result.model_no_start.push(source.name.clone());
            }
            if source.after.is_some() && !result.started_instances.contains(&source.name) {
                result.started_instances.push(source.name.clone());
            }
        }
        compare.finish(); // no pending output or Model state leaks across independent runs
    }
    result.compared = compare.compared;
    result.coalesced = compare.coalesced;
    result.differences = compare.differences;
    Ok(result)
}
