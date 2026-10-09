//! `CtxHarness` (records the intents of one `on_event`, checks Note registration) and
//! `ModelHarness` (Advice output, checkpoints in chunks, allocation peak).

use hft_types::{AccountId, GroupId};

use crate::StrategyDesc;
use crate::advice::{AdviceBufs, AdviceOut};
use crate::ctx::{CancelReq, Ctx, CtxBufs, NoteReq, Op, TimerId, TimerReq};
use crate::intent::{IntentHandle, IntentOutcome, OrderIntent};
use crate::snapshot::{RestoreError, SnapshotReader, SnapshotWriter};
use crate::strategy::{Model, SnapshotProgress};
use crate::target::TargetIntent;

/// What one `CtxHarness::run` collected, per kind in call order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recorded {
    pub places: Vec<OrderIntent>,
    pub targets: Vec<TargetIntent>,
    pub clears: Vec<(AccountId, GroupId)>,
    /// Cancels and amends.
    pub cancels: Vec<CancelReq>,
    pub timers: Vec<TimerReq>,
    pub cancel_timers: Vec<TimerId>,
    pub notes: Vec<NoteReq>,
    pub halts: Vec<(GroupId, u16)>,
    /// `CtxFull` count (notes included).
    pub full: u32,
    /// Handles of the operations in call order.
    pub handles: Vec<IntentHandle>,
}

/// Owns the `Ctx` buffers of one strategy instance and the outcome table of the previous event.
#[derive(Debug)]
pub struct CtxHarness {
    desc: &'static StrategyDesc,
    bufs: Box<CtxBufs>,
}

impl CtxHarness {
    /// Every recorded Note is asserted against `desc.notes` (code registered, level equal).
    pub fn new(desc: &'static StrategyDesc) -> CtxHarness {
        CtxHarness {
            desc,
            bufs: Box::new(CtxBufs::new()),
        }
    }

    /// Runs one event's worth of `Ctx` calls. Outcomes set by `set_outcomes` since the previous
    /// `run` are visible to `f` through `Ctx::outcome`; afterwards they are forgotten.
    pub fn run(&mut self, f: impl FnOnce(&mut Ctx<'_>)) -> Recorded {
        self.bufs.begin();
        {
            let mut ctx = self.bufs.ctx();
            f(&mut ctx);
        }
        self.bufs.clear_outcomes();
        self.collect()
    }

    /// The same as `run` for a closure that already holds a `Ctx` (strategies under test):
    /// `h.run_with(|ctx| strategy.on_event(&ev, &views, ctx))`.
    pub fn run_with<R>(&mut self, f: impl FnOnce(&mut Ctx<'_>) -> R) -> (R, Recorded) {
        self.bufs.begin();
        let r = {
            let mut ctx = self.bufs.ctx();
            f(&mut ctx)
        };
        self.bufs.clear_outcomes();
        (r, self.collect())
    }

    fn collect(&self) -> Recorded {
        let mut r = Recorded {
            full: self.bufs.full_count(),
            ..Recorded::default()
        };
        for (h, op) in self.bufs.ops() {
            r.handles.push(h);
            match op {
                Op::Place(p) => r.places.push(*p),
                Op::Target(t) => r.targets.push(*t),
                Op::ClearTarget(a, g) => r.clears.push((a, g)),
                Op::Cancel(c) => r.cancels.push(*c),
                Op::Timer(t) => r.timers.push(*t),
                Op::CancelTimer(id) => r.cancel_timers.push(id),
                Op::Halt { group, reason } => r.halts.push((group, reason)),
            }
        }
        for n in self.bufs.notes() {
            let d = self.desc.notes.iter().find(|d| d.code == n.code);
            assert!(
                d.is_some(),
                "CtxHarness: Note code {} is not registered in {}.notes",
                n.code,
                self.desc.name
            );
            if let Some(d) = d {
                assert!(
                    d.level == n.level,
                    "CtxHarness: Note code {} written with level {:?}, registered level {:?}",
                    n.code,
                    n.level,
                    d.level
                );
            }
            r.notes.push(*n);
        }
        r
    }

    /// Simulates the engine's results for the previous event's intents: `o[i]` is the outcome of
    /// `IntentHandle(i)`.
    pub fn set_outcomes(&mut self, o: &[IntentOutcome]) {
        self.bufs.clear_outcomes();
        for (i, x) in o.iter().enumerate() {
            if let Ok(h) = u16::try_from(i) {
                self.bufs.set_outcome(IntentHandle(h), *x);
            }
        }
    }

    /// Timer id counter (strategies snapshot timer ids; tests restoring a strategy into a new
    /// harness carry it over).
    pub fn next_timer_id(&self) -> u64 {
        self.bufs.next_timer_id()
    }
    pub fn set_next_timer_id(&mut self, n: u64) {
        self.bufs.set_next_timer_id(n);
    }
}

/// Allocation meter of the test binary's counting global allocator (the SDK has no allocator of
/// its own): current live bytes, peak live bytes since the last reset, and the reset.
#[derive(Clone, Copy, Debug)]
pub struct AllocMeter {
    pub live: fn() -> u64,
    pub peak: fn() -> u64,
    pub reset_peak: fn(),
}

/// A checkpoint as the advisor would write it: the path-state section, the raw-sample section
/// (all chunks concatenated) and the chunk count; `chunks` records each `snapshot_chunk` call
/// (`from_gix`, result, bytes written).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ckpt {
    pub path: Vec<u8>,
    pub body: Vec<u8>,
    pub n_chunks: u32,
    pub chunks: Vec<(u16, SnapshotProgress, u32)>,
}

impl Ckpt {
    /// `path.len() + body.len()`.
    pub fn total(&self) -> u64 {
        u64::try_from(self.path.len() + self.body.len()).unwrap_or(u64::MAX)
    }
}

/// Owns the `AdviceOut` buffers of a Model under test and measures its heap peak.
#[derive(Debug)]
pub struct ModelHarness {
    bufs: AdviceBufs,
    meter: Option<AllocMeter>,
    peak: u64,
    base: Option<u64>,
    w: SnapshotWriter,
}

/// Writer limit of `snapshot_all` (Model checkpoints have no fixed limit; 64 MiB is far above any
/// test Model).
const CKPT_LIMIT: usize = 64 << 20;

impl Default for ModelHarness {
    fn default() -> Self {
        ModelHarness::new()
    }
}

impl ModelHarness {
    /// Without an allocation meter (`peak_alloc_bytes` stays 0).
    pub fn new() -> ModelHarness {
        ModelHarness {
            bufs: AdviceBufs::new(),
            meter: None,
            peak: 0,
            base: None,
            w: SnapshotWriter::new(CKPT_LIMIT),
        }
    }
    /// With the test binary's allocation meter. The harness's own buffers are allocated before
    /// the baseline is taken.
    pub fn with_meter(m: AllocMeter) -> ModelHarness {
        let mut h = ModelHarness::new();
        h.base = Some((m.live)());
        h.meter = Some(m);
        h
    }

    /// Runs `f` (Model construction, `on_params`, restores…) and folds its heap peak above the
    /// baseline into `peak_alloc_bytes`.
    pub fn track<R>(&mut self, f: impl FnOnce() -> R) -> R {
        let Some(m) = self.meter else { return f() };
        (m.reset_peak)();
        let r = f();
        let base = self.base.unwrap_or(0);
        self.peak = self.peak.max((m.peak)().saturating_sub(base));
        r
    }

    /// One `Model::on_event`'s worth of output: `(group, local_kind, payload)` in emit order.
    pub fn run(&mut self, f: impl FnOnce(&mut AdviceOut<'_>)) -> Vec<(GroupId, u16, Vec<u8>)> {
        self.bufs.begin();
        let meter = self.meter;
        let base = self.base.unwrap_or(0);
        if let Some(m) = meter {
            (m.reset_peak)();
        }
        {
            let mut out = self.bufs.out();
            f(&mut out);
        }
        if let Some(m) = meter {
            self.peak = self.peak.max((m.peak)().saturating_sub(base));
        }
        self.bufs.iter().map(|(g, k, p)| (g, k, p.to_vec())).collect()
    }

    /// `AdviceFull` count of the last `run`.
    pub fn full_count(&self) -> u32 {
        self.bufs.full_count()
    }

    /// `snapshot_path`, then `snapshot_chunk` with `max_bytes` from `from_gix = 0` until `Done`
    /// (empty chunks are not counted).
    pub fn snapshot_all<M: Model>(&mut self, m: &M, max_bytes: u32) -> Ckpt {
        let mut c = Ckpt::default();
        self.w.clear();
        m.snapshot_path(&mut self.w);
        c.path = self.w.as_bytes().to_vec();
        let mut from = 0u16;
        for _ in 0..=u32::from(u16::MAX) {
            self.w.clear();
            let p = m.snapshot_chunk(from, &mut self.w, max_bytes);
            let n = self.w.len();
            c.chunks.push((from, p, u32::try_from(n).unwrap_or(u32::MAX)));
            if n > 0 {
                c.n_chunks += 1;
                c.body.extend_from_slice(self.w.as_bytes());
            }
            match p {
                SnapshotProgress::More(next) if next > from => from = next,
                _ => break,
            }
        }
        c
    }

    /// The advisor's restore order: `restore` (raw-sample section), then `restore_path` when
    /// `keep_path` (false simulates a `PATH_VERSION` mismatch). Measured by `track`.
    pub fn restore_all<M: Model>(
        &mut self,
        m: &mut M,
        c: &Ckpt,
        keep_path: bool,
    ) -> Result<(), RestoreError> {
        self.track(|| {
            m.restore(&mut SnapshotReader::new(&c.body))?;
            if keep_path {
                m.restore_path(&mut SnapshotReader::new(&c.path))?;
            }
            Ok(())
        })
    }

    /// Largest heap use above the baseline seen in `track`/`run`/`restore_all` (0 without a meter).
    pub fn peak_alloc_bytes(&self) -> u64 {
        self.peak
    }
}

/// Asserts the Model's declarations: `peak_alloc_bytes ≤ M::mem_bytes(p, n_groups)` and the
/// checkpoint size ≤ `M::snapshot_bytes(p, n_groups)`.
pub fn assert_within_declaration<M: Model>(h: &ModelHarness, p: &M::Params, n_groups: u16, c: &Ckpt) {
    let mem = M::mem_bytes(p, n_groups);
    assert!(
        h.peak_alloc_bytes() <= mem,
        "Model allocation peak {} B exceeds mem_bytes {} B",
        h.peak_alloc_bytes(),
        mem
    );
    let snap = M::snapshot_bytes(p, n_groups);
    assert!(
        c.total() <= snap,
        "checkpoint {} B exceeds snapshot_bytes {} B",
        c.total(),
        snap
    );
}
