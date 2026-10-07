//! Thread start, pinning and the idle step (README "线程布局").

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use crate::config::{Idle, ThreadCfg};

/// Shared stop flags of one run.
#[derive(Debug, Default)]
pub struct Stops {
    /// hk → seq: stop probing and drain (duration reached, signal, safety stop).
    pub drain: AtomicBool,
    /// main → hot tasks: leave the loop.
    pub hot: AtomicBool,
    /// main → warm (conn-builder): leave the loop.
    pub warm: AtomicBool,
    /// main → persist: flush and leave.
    pub persist: AtomicBool,
    /// main → hk: leave.
    pub hk: AtomicBool,
    /// Any thread failed to pin.
    pub pin_failed: AtomicBool,
}

impl Stops {
    pub fn new() -> Arc<Stops> {
        Arc::new(Stops::default())
    }
}

/// Starts a named thread, pinned per `tc` (a pin failure is recorded in `stops`).
pub fn spawn<T: Send + 'static>(
    name: &str,
    tc: ThreadCfg,
    stops: &Arc<Stops>,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<JoinHandle<T>, String> {
    let n = name.to_string();
    let s = Arc::clone(stops);
    std::thread::Builder::new()
        .name(n.clone())
        .spawn(move || {
            hft_rt::name_current_thread(&n);
            if let Some(core) = tc.core
                && hft_rt::pin_current_thread(core).is_err()
            {
                eprintln!("latency-slice: thread {n}: cannot pin to core {core}");
                s.pin_failed.store(true, Ordering::Release);
            }
            f()
        })
        .map_err(|e| format!("spawn {name}: {e}"))
}

/// Idle step: `spin` relaxes the core; `park` sleeps 100 µs after 1 000 idle rounds.
#[inline]
pub fn idle(policy: Idle, idle_rounds: &mut u32) {
    *idle_rounds = idle_rounds.saturating_add(1);
    match policy {
        Idle::Spin => hft_rt::cpu_relax(),
        Idle::Park if *idle_rounds > 1_000 => std::thread::sleep(std::time::Duration::from_micros(100)),
        Idle::Park => hft_rt::cpu_relax(),
    }
}

/// UTC now (ns) from the system clock (warm threads only).
pub fn utc_ns() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// Monotonic now (ns) for the persister.
pub fn mono_ns() -> u64 {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64
}
