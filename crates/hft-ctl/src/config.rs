//! `GatewayConfig` and its defaults.

use std::path::PathBuf;

use crate::principals::PrincipalTable;

/// Default `result_wait_ms`.
pub const DEFAULT_RESULT_WAIT_MS: u32 = 5_000;
/// Default `write_rate_per_min`.
pub const DEFAULT_WRITE_RATE_PER_MIN: u16 = 12;
/// Default `warmup_rate_per_s`.
pub const DEFAULT_WARMUP_RATE_PER_S: u16 = 50;
/// Default `dedup_capacity`.
pub const DEFAULT_DEDUP_CAPACITY: u32 = 4_096;
/// Pending table capacity (= concurrent connection limit).
pub const PENDING_CAPACITY: usize = 64;
/// Smallest ctl ring (bytes).
pub const MIN_RING_BYTES: usize = 256 * 1024;

/// Gateway configuration (README "gateway").
#[derive(Clone, Debug)]
pub struct GatewayConfig {
    /// `paths.ctl_socket` of hft-core.
    pub socket_path: PathBuf,
    pub principals: PrincipalTable,
    pub result_wait_ms: u32,
    pub write_rate_per_min: u16,
    pub warmup_rate_per_s: u16,
    pub dedup_capacity: u32,
    /// ECF1 section 5 `advisor_mem_budget_mib` (no default).
    pub advisor_mem_budget_mib: u32,
    /// ECF1 section 5 `advisor_ckpt_max_mib` (no default).
    pub advisor_ckpt_max_mib: u32,
    /// ECF1 `Capacities.max_groups_per_instance` (capacity of the `GroupTable` handed to
    /// `S::decode_params`; no default).
    pub max_groups_per_instance: u16,
}

impl GatewayConfig {
    /// A configuration with every defaulted field at its default; the fields without a default
    /// (assembly reads them from ECF1) are arguments.
    pub fn new(
        socket_path: PathBuf,
        principals: PrincipalTable,
        advisor_mem_budget_mib: u32,
        advisor_ckpt_max_mib: u32,
        max_groups_per_instance: u16,
    ) -> GatewayConfig {
        GatewayConfig {
            socket_path,
            principals,
            result_wait_ms: DEFAULT_RESULT_WAIT_MS,
            write_rate_per_min: DEFAULT_WRITE_RATE_PER_MIN,
            warmup_rate_per_s: DEFAULT_WARMUP_RATE_PER_S,
            dedup_capacity: DEFAULT_DEDUP_CAPACITY,
            advisor_mem_budget_mib,
            advisor_ckpt_max_mib,
            max_groups_per_instance,
        }
    }
}
