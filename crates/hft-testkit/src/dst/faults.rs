//! The fault catalog (README "6. DST 框架" 故障目录), per-category rates and switches, and the fault
//! plan generated once per run from the seed (README "DST 的世界模型" 4).
//!
//! A plan is `Vec<(virtual_ns, Fault)>` sorted by time; nothing is added while the run proceeds
//! (TK-045). Actors read the faults that concern them; `shrink` deletes items and reruns.

use hft_types::{AccountId, InstId, VenueId};

use crate::dst::sut::CrashPoint;
use crate::dst::world::{ActorId, RngExt, WorldSpec, actor_rng};

/// Fault categories (one switch and one rate each).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum FaultCategory {
    /// 连接
    Conn,
    /// 回报
    Report,
    /// 拒绝
    Reject,
    /// 两腿与断腿
    TwoLeg,
    /// 行情
    Market,
    /// 账户与保证金
    Account,
    /// 用户流
    UserStream,
    /// 控制
    Control,
    /// 模型与模拟
    ModelSim,
    /// 存储与进程
    Storage,
}

impl FaultCategory {
    pub const ALL: [FaultCategory; 10] = [
        FaultCategory::Conn,
        FaultCategory::Report,
        FaultCategory::Reject,
        FaultCategory::TwoLeg,
        FaultCategory::Market,
        FaultCategory::Account,
        FaultCategory::UserStream,
        FaultCategory::Control,
        FaultCategory::ModelSim,
        FaultCategory::Storage,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            FaultCategory::Conn => "conn",
            FaultCategory::Report => "report",
            FaultCategory::Reject => "reject",
            FaultCategory::TwoLeg => "two_leg",
            FaultCategory::Market => "market",
            FaultCategory::Account => "account",
            FaultCategory::UserStream => "user_stream",
            FaultCategory::Control => "control",
            FaultCategory::ModelSim => "model_sim",
            FaultCategory::Storage => "storage",
        }
    }
    const fn ix(self) -> usize {
        self as usize
    }
}

/// Connection role of a connection fault.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum ConnKind {
    /// BBO / market-data connection.
    Md,
    Order,
    UserStream,
}

/// `CrashFault{at_step, repeat, point}`: drop the engine mid-Input at step `at_step`; `repeat = 2`
/// crashes again on the same Input after the first rebuild (poison).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct CrashFault {
    pub at_step: u64,
    pub repeat: u8,
    pub point: CrashPoint,
}

/// `CleanRestartFault{at_step}`: one clean Drain restart.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct CleanRestartFault {
    pub at_step: u64,
}

/// `AdvisorLagFault{from_ens, to_ens, lag_ms}`: within engine time `[from_ens, to_ens)` the Model
/// lags the journal by `lag_ms`; Advice produced then is delivered `lag_ms` later.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct AdvisorLagFault {
    pub from_ens: i64,
    pub to_ens: i64,
    pub lag_ms: u32,
}

/// One fault of the catalog. Durations start at the item's virtual time.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Fault {
    // ---- 连接
    /// Connection down for `down_ms`, reconnect; `resync` = resynchronise after reconnect.
    ConnDown {
        venue: VenueId,
        conn: ConnKind,
        down_ms: u32,
        resync: bool,
    },
    /// Gray failure: alive but latency × `factor`.
    GrayConn {
        venue: VenueId,
        dur_ms: u32,
        factor: u32,
    },
    // ---- 回报
    ReportReorder {
        venue: VenueId,
        window: u8,
        dur_ms: u32,
    },
    ReportDup {
        venue: VenueId,
        dur_ms: u32,
    },
    /// Heavy-tailed extra delay of every report for `dur_ms`.
    ReportDelay {
        venue: VenueId,
        max_ms: u32,
        dur_ms: u32,
    },
    FillBeforeAck {
        venue: VenueId,
    },
    CancelRejectFilled {
        venue: VenueId,
    },
    PartialFills {
        venue: VenueId,
        dur_ms: u32,
    },
    LateFeeOnly {
        venue: VenueId,
        /// Delay after the perturbed fill; one queued fault splits one non-zero fee.
        delay_ms: u32,
    },
    StaleGenFill {
        venue: VenueId,
    },
    LateFillAfterRecycle {
        venue: VenueId,
    },
    /// A real external execution delivered with stale or unresolved order identity. The
    /// optional raw receipt precedes the cumulative copy; exchange truth settles only once.
    ForeignOrderFill {
        account: AccountId,
        inst: InstId,
        qty: i64,
        buy: bool,
        positive_trade_id: bool,
        unresolved: bool,
        raw_first: bool,
    },
    // ---- 拒绝
    RateLimit {
        venue: VenueId,
        dur_ms: u32,
    },
    ServerError {
        venue: VenueId,
    },
    /// Sent, no report at all.
    Timeout {
        venue: VenueId,
    },
    UnknownCode {
        venue: VenueId,
    },
    NotFoundNoGuard {
        venue: VenueId,
    },
    SecondLegReject {
        venue: VenueId,
    },
    // ---- 两腿与断腿
    QuietBook {
        inst: InstId,
        dur_ms: u32,
    },
    SecondLegSilent {
        venue: VenueId,
        dur_ms: u32,
        resync: bool,
    },
    SecondLegFunds {
        account: AccountId,
        venue: VenueId,
    },
    LotMismatch {
        account: AccountId,
    },
    SlipBeyond {
        inst: InstId,
        over_rr_cross: bool,
    },
    /// Second leg rejected for `dur_ms` (cell `Broken`, no manual HedgeFix); variants: the
    /// auto-fix itself rejected, the hedge venue down while the extra leg's venue is healthy.
    PersistentSecondReject {
        venue: VenueId,
        dur_ms: u32,
        autofix_rejected: bool,
        hedge_venue_down: bool,
    },
    NoClientIdFill {
        venue: VenueId,
        within_hold: bool,
    },
    // ---- 行情
    ExTsDisorder {
        venue: VenueId,
    },
    ClockSkew {
        venue: VenueId,
        skew_ms: i32,
    },
    FundingPeriodChange {
        inst: InstId,
    },
    TopNReplace {
        inst: InstId,
    },
    // ---- 账户与保证金
    MarginPath {
        account: AccountId,
        venue: VenueId,
    },
    OneSidedMove {
        inst: InstId,
        pct_bp: i32,
        over_ms: u32,
    },
    L3OtherLegSilent {
        account: AccountId,
        venue: VenueId,
        dur_ms: u32,
    },
    L3ReduceOnlyFull {
        account: AccountId,
        inst: InstId,
    },
    LeverageChange {
        account: AccountId,
        venue: VenueId,
        x100: u32,
    },
    MarginStale {
        account: AccountId,
        venue: VenueId,
        dur_ms: u32,
    },
    SelfLiquidation {
        account: AccountId,
        inst: InstId,
    },
    /// ADL; `gap` = only in the exchange truth (found by recon); `replay` = pushed again after the
    /// user stream reconnects.
    Adl {
        account: AccountId,
        inst: InstId,
        gap: bool,
        replay: bool,
    },
    /// No initial push. A partial/full ADL waits for committed independent booking proof;
    /// only then may `replay` deliver the same original receipt after reconnection.
    ReconciledAdl {
        account: AccountId,
        inst: InstId,
        full: bool,
        replay: bool,
    },
    ManualFill {
        account: AccountId,
        inst: InstId,
    },
    AcctCfgMismatch {
        account: AccountId,
        venue: VenueId,
        at_start: bool,
    },
    UidMismatch {
        account: AccountId,
        venue: VenueId,
    },
    CrossFundingBalanceOnly {
        account: AccountId,
        venue: VenueId,
    },
    BaseFee {
        venue: VenueId,
    },
    FundingFlowDiff {
        account: AccountId,
        venue: VenueId,
    },
    BrokenAtL3 {
        account: AccountId,
    },
    DustResidual {
        account: AccountId,
    },
    // ---- 用户流
    UnknownEvent {
        venue: VenueId,
        ignorable: bool,
    },
    // ---- 控制
    CmdReorder,
    CmdDup,
    CmdExpired,
    SetParamsKillInterleave {
        account: AccountId,
    },
    KillHard {
        account: AccountId,
    },
    KillFlatten {
        account: AccountId,
        broken: bool,
        gate_closed: bool,
        draining: bool,
    },
    FlattenLockTarget {
        account: AccountId,
    },
    ViewerEmergency {
        variant: u8,
    },
    SameVenueSetParams {
        account: AccountId,
    },
    Delisting {
        inst: InstId,
    },
    SimReset {
        account: AccountId,
    },
    SimTransfer {
        account: AccountId,
    },
    InjectWarmup {
        disorder: bool,
        oversize: bool,
    },
    SmokeOrder {
        variant: u8,
    },
    WriteOffResidual {
        account: AccountId,
        broken: bool,
    },
    ResumeKill {
        hard: bool,
    },
    // ---- 模型与模拟
    AdvisorLag(AdvisorLagFault),
    SimReplyFlood {
        n: u32,
    },
    // ---- 存储与进程
    PersistLag {
        lag_ms: u32,
    },
    DiskLow,
    Crash(CrashFault),
    CleanRestart(CleanRestartFault),
    LongDowntime {
        ms: u32,
    },
    BootIdChange,
    StopCoreMarker {
        aborted: bool,
    },
}

impl Fault {
    pub const fn category(&self) -> FaultCategory {
        use Fault::*;
        match self {
            ConnDown { .. } | GrayConn { .. } => FaultCategory::Conn,
            ReportReorder { .. }
            | ReportDup { .. }
            | ReportDelay { .. }
            | FillBeforeAck { .. }
            | CancelRejectFilled { .. }
            | PartialFills { .. }
            | LateFeeOnly { .. }
            | StaleGenFill { .. }
            | LateFillAfterRecycle { .. }
            | ForeignOrderFill { .. } => FaultCategory::Report,
            RateLimit { .. }
            | ServerError { .. }
            | Timeout { .. }
            | UnknownCode { .. }
            | NotFoundNoGuard { .. }
            | SecondLegReject { .. } => FaultCategory::Reject,
            QuietBook { .. }
            | SecondLegSilent { .. }
            | SecondLegFunds { .. }
            | LotMismatch { .. }
            | SlipBeyond { .. }
            | PersistentSecondReject { .. }
            | NoClientIdFill { .. } => FaultCategory::TwoLeg,
            ExTsDisorder { .. } | ClockSkew { .. } | FundingPeriodChange { .. } | TopNReplace { .. } => {
                FaultCategory::Market
            }
            MarginPath { .. }
            | OneSidedMove { .. }
            | L3OtherLegSilent { .. }
            | L3ReduceOnlyFull { .. }
            | LeverageChange { .. }
            | MarginStale { .. }
            | SelfLiquidation { .. }
            | Adl { .. }
            | ReconciledAdl { .. }
            | ManualFill { .. }
            | AcctCfgMismatch { .. }
            | UidMismatch { .. }
            | CrossFundingBalanceOnly { .. }
            | BaseFee { .. }
            | FundingFlowDiff { .. }
            | BrokenAtL3 { .. }
            | DustResidual { .. } => FaultCategory::Account,
            UnknownEvent { .. } => FaultCategory::UserStream,
            CmdReorder
            | CmdDup
            | CmdExpired
            | SetParamsKillInterleave { .. }
            | KillHard { .. }
            | KillFlatten { .. }
            | FlattenLockTarget { .. }
            | ViewerEmergency { .. }
            | SameVenueSetParams { .. }
            | Delisting { .. }
            | SimReset { .. }
            | SimTransfer { .. }
            | InjectWarmup { .. }
            | SmokeOrder { .. }
            | WriteOffResidual { .. }
            | ResumeKill { .. } => FaultCategory::Control,
            AdvisorLag(_) | SimReplyFlood { .. } => FaultCategory::ModelSim,
            PersistLag { .. }
            | DiskLow
            | Crash(_)
            | CleanRestart(_)
            | LongDowntime { .. }
            | BootIdChange
            | StopCoreMarker { .. } => FaultCategory::Storage,
        }
    }
}

/// Switch and rate of one category: each fault slot draws one fault of the category with
/// probability `per_mille / 1000` (`1000` = rate 1.0: one per slot).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CatRate {
    pub on: bool,
    pub per_mille: u32,
}

/// Per-category rates; the values come from each suite's `DstSuite` (no global default).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FaultRates {
    /// Length of one fault slot (virtual ms).
    pub slot_ms: u64,
    pub rates: [CatRate; 10],
}

impl FaultRates {
    /// Every category off.
    pub const fn off() -> FaultRates {
        FaultRates {
            slot_ms: 1000,
            rates: [CatRate {
                on: false,
                per_mille: 0,
            }; 10],
        }
    }
    /// Only `c`, at `per_mille`.
    pub fn only(c: FaultCategory, per_mille: u32) -> FaultRates {
        let mut r = FaultRates::off();
        r.set(c, true, per_mille);
        r
    }
    pub fn set(&mut self, c: FaultCategory, on: bool, per_mille: u32) {
        self.rates[c.ix()] = CatRate { on, per_mille };
    }
    pub fn get(&self, c: FaultCategory) -> CatRate {
        self.rates[c.ix()]
    }
    /// Switches `c` off (rate kept).
    pub fn disable(&mut self, c: FaultCategory) {
        self.rates[c.ix()].on = false;
    }
}

/// The run's complete fault schedule, sorted by virtual time (ties keep generation order).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct FaultPlan {
    pub items: Vec<(u64, Fault)>,
}

impl FaultPlan {
    /// Generates the plan of one run: for every slot of `slot_ms` in `[0, horizon_ns)` and every
    /// enabled category (catalog order), one fault with probability `per_mille / 1000`, drawn from
    /// the fault stream of `seed`. Crash and clean-restart steps are drawn in `[1, steps]`.
    pub fn generate(
        seed: u64,
        rates: &FaultRates,
        world: &WorldSpec,
        horizon_ns: u64,
        steps: u64,
    ) -> FaultPlan {
        let mut rng = actor_rng(seed, ActorId::Faults);
        let slot_ns = rates.slot_ms.max(1) * 1_000_000;
        let mut items = Vec::new();
        let mut t = 0u64;
        while t < horizon_ns {
            for c in FaultCategory::ALL {
                let r = rates.get(c);
                if r.on && rng.chance(u64::from(r.per_mille), 1000) {
                    let at = t + rng.below(slot_ns);
                    items.push((at, draw(c, &mut rng, world, steps, at)));
                }
            }
            t += slot_ns;
        }
        let mut p = FaultPlan { items };
        p.sort();
        p
    }
    /// Stable sort by time.
    pub fn sort(&mut self) {
        self.items.sort_by_key(|(t, _)| *t);
    }
    /// Adds scripted items (scenario `故障`) and re-sorts.
    pub fn with(mut self, scripted: &[(u64, Fault)]) -> FaultPlan {
        self.items.extend_from_slice(scripted);
        self.sort();
        self
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn count(&self, c: FaultCategory) -> usize {
        self.items.iter().filter(|(_, f)| f.category() == c).count()
    }
    /// The plan without the item at `ix`.
    pub fn without(&self, ix: usize) -> FaultPlan {
        let mut p = self.clone();
        p.items.remove(ix);
        p
    }
}

fn pick<T: Copy, R: RngExt>(rng: &mut R, xs: &[T]) -> Option<T> {
    if xs.is_empty() {
        None
    } else {
        Some(xs[rng.below(xs.len() as u64) as usize])
    }
}

/// One fault of category `c` aimed at the world's venues, insts and accounts.
fn draw<R: RngExt>(c: FaultCategory, rng: &mut R, w: &WorldSpec, steps: u64, at: u64) -> Fault {
    use Fault::*;
    let venues: Vec<VenueId> = w.venues.iter().map(|v| v.id).collect();
    let insts: Vec<InstId> = w.insts.iter().map(|i| i.id).collect();
    let accts: Vec<AccountId> = w.accounts.iter().map(|a| a.id).collect();
    let v = pick(rng, &venues).unwrap_or(VenueId(1));
    let i = pick(rng, &insts).unwrap_or(InstId(1));
    let a = pick(rng, &accts).unwrap_or(AccountId(1));
    let ms = |rng: &mut R, lo: u64, hi: u64| rng.range(lo, hi) as u32;
    let flag = |rng: &mut R| rng.chance(1, 2);
    let k = |rng: &mut R, n: u64| rng.below(n);
    match c {
        FaultCategory::Conn => match k(rng, 2) {
            0 => ConnDown {
                venue: v,
                conn: [ConnKind::Md, ConnKind::Order, ConnKind::UserStream][k(rng, 3) as usize],
                down_ms: ms(rng, 100, 10_000),
                resync: flag(rng),
            },
            _ => GrayConn {
                venue: v,
                dur_ms: ms(rng, 500, 20_000),
                factor: 100,
            },
        },
        FaultCategory::Report => match k(rng, 9) {
            0 => ReportReorder {
                venue: v,
                window: rng.range(2, 8) as u8,
                dur_ms: ms(rng, 500, 10_000),
            },
            1 => ReportDup {
                venue: v,
                dur_ms: ms(rng, 500, 10_000),
            },
            2 => ReportDelay {
                venue: v,
                max_ms: ms(rng, 50, 5_000),
                dur_ms: ms(rng, 500, 10_000),
            },
            3 => FillBeforeAck { venue: v },
            4 => CancelRejectFilled { venue: v },
            5 => PartialFills {
                venue: v,
                dur_ms: ms(rng, 500, 10_000),
            },
            6 => LateFeeOnly {
                venue: v,
                delay_ms: 50,
            },
            7 => StaleGenFill { venue: v },
            _ => LateFillAfterRecycle { venue: v },
        },
        FaultCategory::Reject => match k(rng, 6) {
            0 => RateLimit {
                venue: v,
                dur_ms: ms(rng, 200, 5_000),
            },
            1 => ServerError { venue: v },
            2 => Timeout { venue: v },
            3 => UnknownCode { venue: v },
            4 => NotFoundNoGuard { venue: v },
            _ => SecondLegReject { venue: v },
        },
        FaultCategory::TwoLeg => match k(rng, 7) {
            0 => QuietBook {
                inst: i,
                dur_ms: ms(rng, 1_000, 30_000),
            },
            1 => SecondLegSilent {
                venue: v,
                dur_ms: ms(rng, 1_000, 30_000),
                resync: flag(rng),
            },
            2 => SecondLegFunds { account: a, venue: v },
            3 => LotMismatch { account: a },
            4 => SlipBeyond {
                inst: i,
                over_rr_cross: flag(rng),
            },
            5 => PersistentSecondReject {
                venue: v,
                dur_ms: ms(rng, 5_000, 120_000),
                autofix_rejected: flag(rng),
                hedge_venue_down: flag(rng),
            },
            _ => NoClientIdFill {
                venue: v,
                within_hold: flag(rng),
            },
        },
        FaultCategory::Market => match k(rng, 4) {
            0 => ExTsDisorder { venue: v },
            1 => ClockSkew {
                venue: v,
                skew_ms: rng.range(0, 4_000) as i32 - 2_000,
            },
            2 => FundingPeriodChange { inst: i },
            _ => TopNReplace { inst: i },
        },
        FaultCategory::Account => match k(rng, 16) {
            0 => MarginPath { account: a, venue: v },
            1 => OneSidedMove {
                inst: i,
                pct_bp: if flag(rng) { 3_000 } else { -3_000 },
                over_ms: 3_600_000,
            },
            2 => L3OtherLegSilent {
                account: a,
                venue: v,
                dur_ms: 30_000,
            },
            3 => L3ReduceOnlyFull { account: a, inst: i },
            4 => LeverageChange {
                account: a,
                venue: v,
                x100: rng.range(100, 1_000) as u32,
            },
            5 => MarginStale {
                account: a,
                venue: v,
                dur_ms: ms(rng, 1_000, 60_000),
            },
            6 => SelfLiquidation { account: a, inst: i },
            7 => Adl {
                account: a,
                inst: i,
                gap: flag(rng),
                replay: flag(rng),
            },
            8 => ManualFill { account: a, inst: i },
            9 => AcctCfgMismatch {
                account: a,
                venue: v,
                at_start: at == 0,
            },
            10 => UidMismatch { account: a, venue: v },
            11 => CrossFundingBalanceOnly { account: a, venue: v },
            12 => BaseFee { venue: v },
            13 => FundingFlowDiff { account: a, venue: v },
            14 => BrokenAtL3 { account: a },
            _ => DustResidual { account: a },
        },
        FaultCategory::UserStream => UnknownEvent {
            venue: v,
            ignorable: flag(rng),
        },
        FaultCategory::Control => match k(rng, 16) {
            0 => CmdReorder,
            1 => CmdDup,
            2 => CmdExpired,
            3 => SetParamsKillInterleave { account: a },
            4 => KillHard { account: a },
            5 => KillFlatten {
                account: a,
                broken: flag(rng),
                gate_closed: flag(rng),
                draining: flag(rng),
            },
            6 => FlattenLockTarget { account: a },
            7 => ViewerEmergency {
                variant: rng.below(4) as u8,
            },
            8 => SameVenueSetParams { account: a },
            9 => Delisting { inst: i },
            10 => SimReset { account: a },
            11 => SimTransfer { account: a },
            12 => InjectWarmup {
                disorder: flag(rng),
                oversize: flag(rng),
            },
            13 => SmokeOrder {
                variant: rng.below(3) as u8,
            },
            14 => WriteOffResidual {
                account: a,
                broken: flag(rng),
            },
            _ => ResumeKill { hard: flag(rng) },
        },
        FaultCategory::ModelSim => match k(rng, 2) {
            0 => {
                let from = at as i64;
                AdvisorLag(AdvisorLagFault {
                    from_ens: from,
                    to_ens: from + i64::from(ms(rng, 1_000, 10_000)) * 1_000_000,
                    lag_ms: ms(rng, 100, 5_000),
                })
            }
            _ => SimReplyFlood {
                n: rng.range(100, 900) as u32,
            },
        },
        FaultCategory::Storage => match k(rng, 7) {
            0 => PersistLag {
                lag_ms: ms(rng, 500, 5_000),
            },
            1 => DiskLow,
            2 => Crash(CrashFault {
                at_step: rng.range(1, steps.max(1)),
                repeat: 1,
                point: CrashPoint::SeqInput,
            }),
            3 => CleanRestart(CleanRestartFault {
                at_step: rng.range(1, steps.max(1)),
            }),
            4 => LongDowntime {
                ms: ms(rng, 3_000, 600_000),
            },
            5 => BootIdChange,
            _ => StopCoreMarker { aborted: flag(rng) },
        },
    }
}
