//! Configuration keys (README "配置项"; this table is their single definition), `InitErr`, the
//! `feed.ready_poll` decision and the `FeedCmd` destination rule.

use hft_event::FeedCmd;
use hft_net::EndpointId;
use hft_rt::{IdlePolicy, TscUtc};
use hft_types::MAX_BBO_CONN_SLOTS;
use hft_venue_proto::{VenueCfg, VenueEnv};

/// Routes a feed task carries; hft-core: `feed-bbo:<v>` → Bbo, `feed-md:<v>` → Md, `feed:<v>` → All.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FeedRoutes {
    Bbo = 1,
    Md = 2,
    All = 3,
}

impl FeedRoutes {
    pub fn has_bbo(self) -> bool {
        matches!(self, FeedRoutes::Bbo | FeedRoutes::All)
    }
    pub fn has_md(self) -> bool {
        matches!(self, FeedRoutes::Md | FeedRoutes::All)
    }
}

/// Constructor failure of every runtime (hft-core treats it as a startup failure).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum InitErr {
    /// A key of "取值为 0 的键" is 0; the value is the key name.
    Cfg(&'static str),
    /// `feed.bbo_conns × groups + 1 > MAX_BBO_CONN_SLOTS`.
    BboSlots,
    /// Rings or journals present/absent against `routes` and `feed.depth` (README §1).
    DepsMismatch,
    /// Subscription frame rate < 1 frame/s (§2 step 8).
    SubRate,
    /// Effective `ws_max_conn_age_s ≠ 0` and `conn.rotate_margin_s ≥` it.
    RotateMargin,
    /// GwRuntime / AcctFetchTask got an `AccountKind::Sim` account.
    SimAccount,
    /// GwRuntime: `CAP < P::ORDER_FRAME_CAP`.
    FrameCap,
    /// `gw.id_table_cap` not a power of two or `< 2 × (OMS slots + gw.id_tomb_cap)`.
    IdTableCap,
    /// Effective `margin_data = Poll{interval_ms}` with `interval_ms < gw.margin_poll_min_ms`.
    MarginPoll,
}

/// `feed.ready_poll`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ReadyPollMode {
    Auto,
    Off,
}

/// `feed.ready_poll` decision (hft-core assembly; the result goes into `FeedCfg.ready`): `auto`, a
/// `Spin` thread and at least 3 planned connections over all feed tasks of that thread.
pub fn ready_poll_on(mode: ReadyPollMode, policy: IdlePolicy, thread_conns: u16) -> bool {
    mode == ReadyPollMode::Auto && matches!(policy, IdlePolicy::Spin) && thread_conns >= 3
}

/// Which feedctl ring(s) a `FeedCmd` goes to in the split form (README §1).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FeedCmdDest {
    Bbo,
    Md,
    Both,
}

pub fn feed_cmd_dest(c: &FeedCmd) -> FeedCmdDest {
    match c {
        FeedCmd::Resync { .. } => FeedCmdDest::Md,
        FeedCmd::Rotate { conn } if conn.0 < MAX_BBO_CONN_SLOTS => FeedCmdDest::Bbo,
        FeedCmd::Rotate { .. } => FeedCmdDest::Md,
        FeedCmd::Subscribe { .. } | FeedCmd::Unsubscribe { .. } => FeedCmdDest::Both,
        // Wire ids are rendered by the depth/Md routes' subscription frames.
        FeedCmd::BindWire { .. } => FeedCmdDest::Md,
    }
}

/// `feed.depth`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DepthMode {
    TopN,
    /// Optional extension (README §3); not in this build.
    Incremental,
}

/// `feed.raw.bbo` / `feed.raw.md`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RawMode {
    Off,
    Sampled,
    On,
}

/// Connection targets of a feed task (from hft-core `venues.<v>.feed.conns` and the endpoint table).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FeedEndpoints {
    pub bbo: EndpointId,
    pub depth: EndpointId,
    pub md: EndpointId,
    /// Explicit endpoints for protocols with multiple Md routes. Unset entries use `md`.
    pub md_routes: [Option<EndpointId>; 256],
    pub src_ip_idx: u8,
}

impl Default for FeedEndpoints {
    fn default() -> Self {
        Self {
            bbo: EndpointId(0),
            depth: EndpointId(0),
            md: EndpointId(0),
            md_routes: [None; 256],
            src_ip_idx: 0,
        }
    }
}

/// Feed-task configuration: the README keys plus the hft-core assembly results.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FeedCfg {
    pub routes: FeedRoutes,
    /// `ready_poll_on` result.
    pub ready: bool,
    /// hft-core `venues.<v>.max_sub_insts`.
    pub max_sub_insts: u32,
    pub ready_poll: ReadyPollMode,
    pub bbo_conns: u8,
    pub bbo_frames_per_poll: u16,
    pub frames_per_poll: u16,
    pub bbo_ping_ms: u32,
    pub bbo_dead_ms: u32,
    pub bbo_refresh_grace_ms: u32,
    pub depth: DepthMode,
    pub topn_levels: u8,
    pub snapshot_part_levels: u16,
    pub resync_slots: u8,
    pub resync_buf: u32,
    pub snapshot_backoff_ms: (u32, u32),
    pub raw_dups: bool,
    pub raw_bbo: RawMode,
    pub raw_md: RawMode,
    pub raw_sample_ms: u32,
    pub ring_full_park_us: u32,
    pub rotate_margin_s: u32,
    pub venue: VenueCfg,
    pub endpoints: FeedEndpoints,
}

impl Default for FeedCfg {
    fn default() -> Self {
        FeedCfg {
            routes: FeedRoutes::All,
            ready: false,
            max_sub_insts: 0,
            ready_poll: ReadyPollMode::Auto,
            bbo_conns: 2,
            bbo_frames_per_poll: 8,
            frames_per_poll: 64,
            bbo_ping_ms: 1000,
            bbo_dead_ms: 3000,
            bbo_refresh_grace_ms: 500,
            depth: DepthMode::TopN,
            topn_levels: 5,
            snapshot_part_levels: 64,
            resync_slots: 4,
            resync_buf: 1024,
            snapshot_backoff_ms: (100, 5000),
            raw_dups: false,
            // Per host class and instance in the hft-core memory table; `on` without one.
            raw_bbo: RawMode::On,
            raw_md: RawMode::On,
            raw_sample_ms: 1000,
            ring_full_park_us: 200,
            rotate_margin_s: 3600,
            venue: VenueCfg {
                env: VenueEnv::Prod,
                stale_guard_ms: 1500,
            },
            endpoints: FeedEndpoints::default(),
        }
    }
}

/// gw configuration (the `gw.*` keys, `conn.rotate_margin_s` and the assembly values).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct GwCfg {
    pub cmds_per_loop: u16,
    pub cmds_per_account_per_loop: u16,
    pub aux_every: u16,
    pub rest_idle_check_ms: u32,
    pub rotate_wait_max_ms: u32,
    pub warm_interval_ns: u64,
    pub id_table_cap: u32,
    pub id_tomb_cap: u32,
    pub id_tomb_ms: u32,
    pub unmatched_hold_ms: u32,
    pub unmatched_hold_cap: u16,
    pub dedup_reseed_ms: u32,
    pub inflight_timeout_ms: u32,
    pub report_decode_down_after: u8,
    pub stale_guard_ms: u32,
    pub margin_poll_min_ms: u32,
    pub margin_push_stale_ms: u32,
    pub acct_cfg_recheck_s: u32,
    pub acct_query_timeout_ms: u32,
    pub acct_query_degraded_after: u8,
    pub acct_body_max: u32,
    pub unknown_evt_note_every: u32,
    pub rotate_margin_s: u32,
    /// OMS slot count (slot mirror size; hft-oms).
    pub oms_slots: u32,
    pub env: VenueEnv,
    /// Order and user-stream connection targets.
    pub order_endpoint: EndpointId,
    pub user_endpoint: EndpointId,
}

impl Default for GwCfg {
    fn default() -> Self {
        GwCfg {
            cmds_per_loop: 16,
            cmds_per_account_per_loop: 8,
            aux_every: 8,
            rest_idle_check_ms: 1000,
            rotate_wait_max_ms: 60_000,
            warm_interval_ns: 0,
            id_table_cap: 16_384,
            id_tomb_cap: 4096,
            id_tomb_ms: 120_000,
            unmatched_hold_ms: 5000,
            unmatched_hold_cap: 256,
            dedup_reseed_ms: 900_000,
            inflight_timeout_ms: 3000,
            report_decode_down_after: 3,
            stale_guard_ms: 1500,
            margin_poll_min_ms: 1000,
            margin_push_stale_ms: 5000,
            acct_cfg_recheck_s: 600,
            acct_query_timeout_ms: 3000,
            acct_query_degraded_after: 3,
            acct_body_max: 262_144,
            unknown_evt_note_every: 1000,
            rotate_margin_s: 3600,
            oms_slots: 2048,
            env: VenueEnv::Prod,
            order_endpoint: EndpointId::default(),
            user_endpoint: EndpointId::default(),
        }
    }
}

impl GwCfg {
    /// The `VenueCfg` handed to the protocol (`gw.stale_guard_ms` is the expiry window).
    pub fn venue_cfg(&self) -> VenueCfg {
        VenueCfg {
            env: self.env,
            stale_guard_ms: self.stale_guard_ms,
        }
    }
}

/// snap-fetch configuration.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SnapCfg {
    pub parallel: u8,
    pub rest_idle_check_ms: u32,
    pub rotate_margin_s: u32,
    pub timeout_ms: u32,
    pub request_gap_ms: u32,
    /// No greater than one quarter of the output byte-ring capacity.
    pub max_record_bytes: u32,
}

impl Default for SnapCfg {
    fn default() -> Self {
        SnapCfg {
            parallel: 2,
            rest_idle_check_ms: 1000,
            rotate_margin_s: 3600,
            timeout_ms: 2000,
            request_gap_ms: 100,
            max_record_bytes: 65536,
        }
    }
}

/// TSC ↔ duration conversions from the runtime's `TscUtc` (TSC deltas only; never across boots).
#[derive(Copy, Clone, Debug)]
pub(crate) struct Ticks {
    pub clock: TscUtc,
}

impl Ticks {
    /// Ticks in `ns` nanoseconds (`ns << shift / mult`).
    pub fn ns(&self, ns: u64) -> u64 {
        let mult = u128::from(self.clock.mult.max(1));
        let v = (u128::from(ns) << self.clock.shift.min(64)) / mult;
        u64::try_from(v).unwrap_or(u64::MAX)
    }
    pub fn ms(&self, ms: u64) -> u64 {
        self.ns(ms.saturating_mul(1_000_000))
    }
    pub fn us(&self, us: u64) -> u64 {
        self.ns(us.saturating_mul(1000))
    }
    /// UTC nanoseconds at `tsc`.
    pub fn utc_ns(&self, tsc: u64) -> i64 {
        self.clock.utc_ns(hft_types::Tsc(tsc))
    }
    /// Counter frequency in milli-hertz (journal segment headers).
    pub fn hz_milli(&self) -> u64 {
        let mult = u128::from(self.clock.mult.max(1));
        let v = (1_000_000_000_000u128 << self.clock.shift.min(64)) / mult;
        u64::try_from(v).unwrap_or(u64::MAX)
    }
}
