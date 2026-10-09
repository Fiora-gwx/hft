//! The strategy traits (README "Strategy 与 Model", "参数 schema 与 ParamSet"): `ParamSet`,
//! `Strategy`, `Model`, `NoModel`, `StrategyInstance`, `ModelParamEffect`, `GroupResets`,
//! `ResetCause`, `SnapshotProgress`.

use hft_types::{AccountId, StrategyId};

use crate::StrategyDesc;
use crate::advice::AdviceOut;
use crate::ctx::{Ctx, SubsBuilder};
use crate::event::EventRef;
use crate::group::GroupTable;
use crate::layout::ModelLayoutDesc;
use crate::params::ParamError;
use crate::snapshot::{RestoreError, SnapshotReader, SnapshotWriter};
use crate::views::{ModelViews, RefView, Views};

/// Decoded, validated fixed-point parameters of one strategy kind (`define_params!` generates the
/// implementation).
pub trait ParamSet: Sized + Send {
    /// ctl-gateway thread (cold): decodes the body in schema order (`params::decode_body`); may
    /// allocate (per-group override tables).
    fn decode(body: &[u8], schema_ver: u16, groups: &GroupTable) -> Result<Self, ParamError>;
    /// Cross-field and per-group-shape checks; after `decode`, before applying.
    fn validate(&self, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError>;
    /// Whether any `structural` field's effective value differs between `old` at `old_gix` and
    /// `self` at `gix` (the same GroupId in both versions). `define_params!` generates it; the
    /// default (no structural fields) is `false`.
    fn structural_changed(&self, old: &Self, old_gix: u16, gix: u16) -> bool {
        let _ = (old, old_gix, gix);
        false
    }
}

/// One strategy instance = (strategy, account). The instance never sees the account kind
/// (Live/Sim): strategy code is identical on simulated and live accounts.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct StrategyInstance {
    pub strategy: StrategyId,
    pub account: AccountId,
    pub kind_id: u16,
}

/// The fast part (sequencer thread). `on_event` is the only hot-path entry: no allocation,
/// blocking, system clock, panic or f64.
pub trait Strategy: Sized {
    const DESC: StrategyDesc;
    type Params: ParamSet;
    /// `NoModel<Self::Params>` when there is no slow part.
    type Model: Model<Params = Self::Params>;

    /// Command path: preallocate every group's state for `GroupTable::capacity()`.
    fn new(
        inst: StrategyInstance,
        p: &Self::Params,
        groups: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<Self, ParamError>;
    /// After loading and after every parameter application; the sequencer rebuilds its
    /// inst → [(instance, gix)] index from it.
    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>);
    /// The only hot-path entry.
    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>);
    /// Before a `SetParams` applies (command path); `Err` keeps the old parameters. Align state by
    /// GroupId; new GroupIds and changed fingerprints start empty.
    fn on_params(
        &mut self,
        p: &Self::Params,
        groups: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<(), ParamError>;
    /// Writes the whole fast-part state (one go in the keyframe).
    fn snapshot(&self, w: &mut SnapshotWriter);
    /// `r.desc_version()` = `StrategyDesc.version` when the section was written; accept the
    /// current and the previous version, otherwise `VersionUnsupported`.
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError>;
}

/// Next chunk of `Model::snapshot_chunk`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SnapshotProgress {
    /// More to write; the next chunk's `from_gix`.
    More(u16),
    Done,
}

/// Why a group's Model state restarts empty; codes equal hft-event `Note{ModelGroupReset}` v[0]
/// (`ResetAll` is 4 in that Note and not a value of this enum).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum ResetCause {
    /// A `structural` field's value changed on the group.
    Structural = 1,
    /// New GroupId (no old fingerprint) or `GroupSpec::fingerprint()` changed.
    Fingerprint = 2,
    /// The storage-interpretation config saved in the checkpoint differs from the current one.
    PctConfig = 3,
}

impl ResetCause {
    /// `ModelGroupReset` v[0] code of `ResetAll`.
    pub const RESET_ALL_CODE: u64 = 4;
    pub const fn code(self) -> u8 {
        self as u8
    }
}

/// At most one `ResetCause` per gix of one parameter version (capacity = `GroupTable::capacity()`;
/// allocated on the command path).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct GroupResets {
    causes: Box<[Option<ResetCause>]>,
}

impl GroupResets {
    pub fn new(cap: u16) -> GroupResets {
        GroupResets {
            causes: vec![None; usize::from(cap)].into_boxed_slice(),
        }
    }
    /// Several causes for one gix keep the smallest code; gix ≥ capacity is ignored.
    pub fn set(&mut self, gix: u16, c: ResetCause) {
        if let Some(s) = self.causes.get_mut(usize::from(gix)) {
            *s = Some(match *s {
                Some(o) if o <= c => o,
                _ => c,
            });
        }
    }
    pub fn get(&self, gix: u16) -> Option<ResetCause> {
        self.causes.get(usize::from(gix)).copied().flatten()
    }
    /// `(gix, cause)` in ascending gix.
    pub fn iter(&self) -> impl Iterator<Item = (u16, ResetCause)> + '_ {
        self.causes
            .iter()
            .enumerate()
            .filter_map(|(i, c)| Some((u16::try_from(i).ok()?, (*c)?)))
    }
    /// Number of reset groups.
    pub fn count(&self) -> usize {
        self.causes.iter().filter(|c| c.is_some()).count()
    }
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }
}

/// What `Model::on_params` reset.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum ModelParamEffect {
    None,
    ResetGroups(GroupResets),
    ResetAll,
}

/// The slow part (advisor thread, warm). Rules: README "Model 检查点", "确定性规则".
pub trait Model: Sized {
    type Params: ParamSet;
    /// Raw-sample section layout version (part of the checkpoint key).
    const LAYOUT_VERSION: u32;
    /// Raw-sample section layout description (CI compares its canonical JSON golden).
    const LAYOUT: &'static ModelLayoutDesc;
    /// Path-state section version (not in the key; a mismatch drops only that section). 0 = no
    /// path state, `snapshot_path` writes nothing.
    const PATH_VERSION: u32;
    /// The Model can be warmed up by warmup injection (only used by the reset preview estimate).
    const WARMUP: bool = false;
    /// Heap upper bound (bytes) for `p` and `n_groups` groups. Pure, cold.
    fn mem_bytes(p: &Self::Params, n_groups: u16) -> u64;
    /// Checkpoint upper bound (bytes): `snapshot_path` plus every `snapshot_chunk`.
    fn snapshot_bytes(p: &Self::Params, n_groups: u16) -> u64;
    /// Longest engine time (ms) until output matches a continuous run again after the path section
    /// was dropped; 0 when `PATH_VERSION = 0`.
    fn path_settle_ms(p: &Self::Params) -> u64;
    /// Longest event time (ms) from an empty group (no warmup) to enough samples to open; 0 when
    /// sample-independent.
    fn cold_start_ms(p: &Self::Params, gix: u16) -> u64;
    fn new(inst: StrategyInstance, p: &Self::Params, groups: &GroupTable, rd: &RefView<'_>) -> Self;
    /// After an Applied SetParams/ReplaceRefdata, and once after checkpoint restore. Idempotent;
    /// align by GroupId; report the groups restarted empty.
    fn on_params(&mut self, p: &Self::Params, groups: &GroupTable, rd: &RefView<'_>) -> ModelParamEffect;
    /// Market data, model ticks, warmup and subscribed own Order/Exec/Manual.
    fn on_event<V: ModelViews>(&mut self, ev: &EventRef<'_>, v: &V, out: &mut AdviceOut<'_>);
    /// Path-state section (first chunk of the checkpoint, one go). Nothing when `PATH_VERSION = 0`.
    fn snapshot_path(&self, w: &mut SnapshotWriter);
    /// Reads the path-state section (only when the file's version = `PATH_VERSION`, after
    /// `restore`); on error leaves no partial state.
    fn restore_path(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError>;
    /// Raw-sample section in chunks: `from_gix = 0` writes the instance section first; then whole
    /// group records from `from_gix` in gix order until one more would exceed `max_bytes`
    /// (at least one group per chunk).
    fn snapshot_chunk(&self, from_gix: u16, w: &mut SnapshotWriter, max_bytes: u32) -> SnapshotProgress;
    /// Reads the raw-sample section (all chunks concatenated) and recomputes derived state.
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError>;
}

/// The Model of strategies without a slow part: empty `on_event`, all versions and declarations 0,
/// no checkpoint.
#[derive(Debug)]
pub struct NoModel<P>(core::marker::PhantomData<P>);

impl<P: ParamSet> Model for NoModel<P> {
    type Params = P;
    const LAYOUT_VERSION: u32 = 0;
    const LAYOUT: &'static ModelLayoutDesc = &ModelLayoutDesc::EMPTY;
    const PATH_VERSION: u32 = 0;
    fn mem_bytes(_: &P, _: u16) -> u64 {
        0
    }
    fn snapshot_bytes(_: &P, _: u16) -> u64 {
        0
    }
    fn path_settle_ms(_: &P) -> u64 {
        0
    }
    fn cold_start_ms(_: &P, _: u16) -> u64 {
        0
    }
    fn new(_: StrategyInstance, _: &P, _: &GroupTable, _: &RefView<'_>) -> Self {
        NoModel(core::marker::PhantomData)
    }
    fn on_params(&mut self, _: &P, _: &GroupTable, _: &RefView<'_>) -> ModelParamEffect {
        ModelParamEffect::None
    }
    fn on_event<V: ModelViews>(&mut self, _: &EventRef<'_>, _: &V, _: &mut AdviceOut<'_>) {}
    fn snapshot_path(&self, _: &mut SnapshotWriter) {}
    fn restore_path(&mut self, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
    fn snapshot_chunk(&self, _: u16, _: &mut SnapshotWriter, _: u32) -> SnapshotProgress {
        SnapshotProgress::Done
    }
    fn restore(&mut self, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
}
