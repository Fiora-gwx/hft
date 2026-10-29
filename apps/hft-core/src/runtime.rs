//! Startup ordering, task ownership, supervision and graceful drain.
use crate::{
    Error, Strategies,
    config::{Config, Idle, Loaded, Thread},
    io, paper, storage,
};
use hft_event::{HkMsg, SysTimer, TimerCode};
use hft_rt::{Backing, Consumer, Producer, TscClock};
use hft_sequencer::driver::live::{LiveCfg, LiveDriver};
use hft_sequencer::{Sequencer, StepOutcome};
use hft_types::VenueId;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub fn ring<T: Copy>(n: usize) -> Result<(Producer<T>, Consumer<T>), Error> {
    hft_rt::spsc(n, Backing::Heap).map_err(|e| Error::runtime("Ring", format!("{e:?}"), 3))
}
pub fn byte_ring(n: usize) -> Result<(hft_rt::ByteProducer, hft_rt::ByteConsumer), Error> {
    hft_rt::byte_spsc(n, Backing::Heap).map_err(|e| Error::runtime("Ring", format!("{e:?}"), 3))
}
pub fn policy(c: &Config, task: &str) -> Result<Thread, Error> {
    c.threads
        .iter()
        .find(|t| t.task == task)
        .cloned()
        .ok_or_else(|| Error::config("ThreadPlan", task))
}
pub fn pin(t: &Thread) -> Result<(), Error> {
    if t.idle == Idle::Spin {
        let isolated = std::fs::read_to_string("/sys/devices/system/cpu/isolated").unwrap_or_default();
        let allowed = t.core.is_some_and(|cpu| {
            isolated.trim().split(',').any(|part| {
                let (first, last) = part.split_once('-').unwrap_or((part, part));
                first
                    .parse::<u16>()
                    .ok()
                    .zip(last.parse::<u16>().ok())
                    .is_some_and(|(first, last)| first <= cpu && cpu <= last)
            })
        });
        if !allowed {
            return Err(Error::runtime(
                "ThreadPolicy",
                "spin requires a dedicated isolated CPU",
                3,
            ));
        }
    }
    if let Some(cpu) = t.core {
        hft_rt::pin_current_thread(cpu)
            .map_err(|e| Error::runtime("Affinity", format!("{}: {e:?}", t.name), 3))?;
    }
    #[cfg(target_os = "linux")]
    {
        // All settings target the calling Linux thread; do not inherit an RT policy
        // or the parent's nice value accidentally. Setup runs before the task body.
        let param = libc::sched_param { sched_priority: 0 };
        // SAFETY: param points to an initialized sched_param; pid 0 means this thread.
        if unsafe { libc::sched_setscheduler(0, libc::SCHED_OTHER, &param) } != 0 {
            return Err(Error::runtime(
                "ThreadPolicy",
                format!("{}: SCHED_OTHER", t.name),
                3,
            ));
        }
        // SAFETY: PR_SET_TIMERSLACK takes an unsigned long value, no pointers.
        if unsafe { libc::prctl(libc::PR_SET_TIMERSLACK, 1 as libc::c_ulong, 0, 0, 0) } != 0 {
            return Err(Error::runtime(
                "ThreadPolicy",
                format!("{}: timerslack", t.name),
                3,
            ));
        }
        // SAFETY: PRIO_PROCESS with id 0 addresses only the calling Linux thread.
        if unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, i32::from(t.nice)) } != 0 {
            return Err(Error::runtime("ThreadPolicy", format!("{}: nice", t.name), 3));
        }
    }
    hft_rt::name_current_thread(&t.name);
    Ok(())
}
pub fn idle(t: &Thread) {
    match t.idle {
        Idle::Park => std::thread::park_timeout(Duration::from_micros(t.park_us)),
        Idle::Spin => std::hint::spin_loop(),
    }
}

/// A worker owns the only writer of its claimed heartbeat slot.
#[derive(Debug)]
pub struct ThreadContext {
    pub policy: Thread,
    pub heartbeat: &'static hft_rt::Heartbeat,
}
impl std::ops::Deref for ThreadContext {
    type Target = Thread;
    fn deref(&self) -> &Thread {
        &self.policy
    }
}

fn claim(
    heartbeat: &'static hft_rt::HeartbeatBlock,
    t: &Thread,
) -> Result<&'static hft_rt::Heartbeat, Error> {
    use hft_rt::HbKind;
    let kind = match t.task.as_str() {
        "sequencer" => HbKind::Sequencer,
        "persist" => HbKind::Persist,
        "housekeeping" => HbKind::Housekeeping,
        "ctl-gateway" => HbKind::Ctl,
        "sim-gw" => HbKind::Gw,
        "paper-feed" => HbKind::Feed,
        s if s.starts_with("gw:") => HbKind::Gw,
        s if s.starts_with("feed:") => HbKind::Feed,
        _ => HbKind::Mixed,
    };
    heartbeat
        .claim(&t.name, t.core.unwrap_or(u16::MAX), kind as u8)
        .ok_or_else(|| Error::config("ThreadPlan", "heartbeat slots exhausted"))
}

/// Assemble a calibration observation and its decision input, keeping the three clock states.
/// Every step is a fresh discontinuity; identical consecutive steps must not be coalesced.
pub fn clock_tick(
    hk: &mut Producer<HkMsg>,
    pair: hft_rt::CalibPair,
    hz: u64,
    verdict: hft_rt::ClockVerdict,
    previous: &mut Option<u8>,
) -> Result<(), Error> {
    let state = match verdict {
        hft_rt::ClockVerdict::Synced => 1,
        hft_rt::ClockVerdict::Unsynced => 2,
        hft_rt::ClockVerdict::Stepped { .. } => 3,
    };
    let mut status = hft_sequencer::snapshot::clock_status(pair, hz);
    status.source = 2;
    status.state = state;
    hk.try_push(HkMsg::ClockStatus(status))
        .map_err(|_| Error::runtime("HkFull", "clock record", 3))?;
    if *previous != Some(state) || state == 3 {
        let (_, timer_id, _) = TimerCode::Sys(if state == 1 {
            SysTimer::ClockOk
        } else {
            SysTimer::ClockBad
        })
        .encode()
        .map_err(|_| Error::runtime("Clock", "timer encoding", 3))?;
        hk.try_push(HkMsg::Sys { timer_id })
            .map_err(|_| Error::runtime("HkFull", "clock verdict", 3))?;
        *previous = Some(state);
    }
    Ok(())
}

/// `UtcAnchor` cadence of the housekeeping calibration tick (README "引擎 UTC").
#[derive(Copy, Clone, Debug)]
pub struct AnchorTick {
    every: u32,
    since: Option<u32>,
}

impl AnchorTick {
    /// One anchor every `every` Synced ticks (`clock.anchor_every()`).
    pub fn new(every: u32) -> AnchorTick {
        AnchorTick {
            every: every.max(1),
            since: None,
        }
    }
}

/// After `clock_tick`: on a `Synced` tick that is due (the first Synced tick, then every
/// `every`-th), sends the pair and the tracker's counter rate as `HkMsg::UtcAnchor`; any other
/// verdict defers it to the next Synced tick.
pub fn anchor_tick(
    hk: &mut Producer<HkMsg>,
    pair: hft_rt::CalibPair,
    verdict: hft_rt::ClockVerdict,
    hz_milli: Option<u64>,
    st: &mut AnchorTick,
) -> Result<(), Error> {
    if verdict != hft_rt::ClockVerdict::Synced {
        st.since = None;
        return Ok(());
    }
    if let Some(n) = st.since
        && n + 1 < st.every
    {
        st.since = Some(n + 1);
        return Ok(());
    }
    let mut a = <hft_event::UtcAnchor as zerocopy::FromZeros>::new_zeroed();
    a.tsc = pair.tsc;
    a.realtime_ns = pair.realtime_ns;
    a.hz_milli = hz_milli.unwrap_or(0);
    a.err_ns = pair.err_ns;
    hk.try_push(HkMsg::UtcAnchor(a))
        .map_err(|_| Error::runtime("HkFull", "utc anchor", 3))?;
    st.since = Some(0);
    Ok(())
}

#[cfg(target_os = "linux")]
pub(crate) fn notify(message: &str) -> Result<(), Error> {
    use std::os::{
        linux::net::SocketAddrExt,
        unix::{
            ffi::OsStrExt,
            net::{SocketAddr, UnixDatagram},
        },
    };
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else {
        return Ok(());
    };
    let bytes = path.as_bytes();
    let addr = if let Some(name) = bytes.strip_prefix(b"@") {
        SocketAddr::from_abstract_name(name)
    } else {
        SocketAddr::from_pathname(path)
    }
    .map_err(|_| Error::runtime("Notify", "invalid notify address", 3))?;
    UnixDatagram::unbound()
        .and_then(|socket| socket.send_to_addr(message.as_bytes(), &addr))
        .map_err(|_| Error::runtime("Notify", "cannot notify service manager", 3))?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn notify(_message: &str) -> Result<(), Error> {
    Ok(())
}

/// Bounded shutdown joins (README "优雅退出与 kill 语义"): workers, then the persister (its final
/// sync copies the hot tail).
pub const SHUTDOWN_WORKERS: Duration = Duration::from_secs(30);
pub const SHUTDOWN_PERSIST: Duration = Duration::from_secs(120);

#[derive(Debug)]
pub struct Workers {
    pub(crate) trace: std::collections::BTreeMap<String, hft_rt::latency_trace::Writer>,
    pub stop: Arc<AtomicBool>,
    joins: Vec<JoinHandle<Result<(), Error>>>,
    heartbeat: &'static hft_rt::HeartbeatBlock,
}
impl Workers {
    pub fn new(heartbeat: &'static hft_rt::HeartbeatBlock) -> Self {
        Self {
            trace: std::collections::BTreeMap::new(),
            stop: Arc::new(AtomicBool::new(false)),
            joins: Vec::new(),
            heartbeat,
        }
    }
    pub fn spawn(
        &mut self,
        t: Thread,
        f: impl FnOnce(Arc<AtomicBool>, ThreadContext) -> Result<(), Error> + Send + 'static,
    ) -> Result<(), Error> {
        // Slot identities remain owned by the single assembly thread.
        let heartbeat = claim(self.heartbeat, &t)?;
        let stop = self.stop.clone();
        // The main thread waits only for OS setup, never for network or task work.
        // This keeps a failed affinity/priority setup from racing READY.
        let (setup_tx, setup_rx) = std::sync::mpsc::sync_channel(1);
        let trace = self.trace.remove(&t.name);
        let t_name = t.name.clone();
        self.joins.push(
            std::thread::Builder::new()
                .name(t.name.clone())
                .spawn(move || {
                    if let Some(trace) = trace {
                        hft_rt::latency_trace::install(trace)
                            .map_err(|_| Error::runtime("LatencyTrace", "writer already installed", 3))?;
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        if let Err(error) = pin(&t) {
                            let _ = setup_tx.send(Err(error));
                            return Ok(());
                        }
                        if setup_tx.send(Ok(())).is_err() {
                            return Ok(());
                        }
                        f(stop.clone(), ThreadContext { policy: t, heartbeat })
                    }))
                    .unwrap_or_else(|_| Err(Error::runtime("ThreadPanic", "worker panicked", 134)));
                    if let Err(e) = &result {
                        // Reported at once: the shutdown that follows may not finish.
                        eprintln!("hft-core: {} thread {} failed: {e}", utc_stamp(), t_name);
                        stop.store(true, Ordering::Release);
                    }
                    result
                })
                .map_err(|e| Error::runtime("Thread", e.to_string(), 3))?,
        );
        match setup_rx.recv() {
            Ok(Ok(())) => Ok(()),
            result => {
                self.stop.store(true, Ordering::Release);
                Err(match result {
                    Ok(Err(error)) => error,
                    _ => Error::runtime("ThreadPanic", "worker setup did not complete", 134),
                })
            }
        }
    }
    pub fn join(&mut self) -> Result<(), Error> {
        join_all_within(std::mem::take(&mut self.joins), None)
    }
    /// `join` bounded by `limit`: threads still running then are named in a `ShutdownStuck` error
    /// and left detached (the process exits without them).
    pub fn join_within(&mut self, limit: Duration) -> Result<(), Error> {
        join_all_within(std::mem::take(&mut self.joins), Some(limit))
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.join_within(Duration::from_secs(5));
    }
}

/// Wall-clock `unix_seconds.millis` for diagnostics on stderr.
fn utc_stamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", d.as_secs(), d.subsec_millis())
}

/// Joins `joins` (first error wins). With `limit`, waits at most that long for all of them; the
/// ones still running are reported (stderr and `ShutdownStuck`, exit 134) and not joined.
pub fn join_all_within(
    joins: Vec<JoinHandle<Result<(), Error>>>,
    limit: Option<Duration>,
) -> Result<(), Error> {
    if let Some(limit) = limit {
        let deadline = Instant::now() + limit;
        while joins.iter().any(|j| !j.is_finished()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let mut first = None;
    let mut stuck = Vec::new();
    for j in joins {
        if limit.is_some() && !j.is_finished() {
            stuck.push(j.thread().name().unwrap_or("?").to_string());
            continue;
        }
        if let Err(e) = j
            .join()
            .unwrap_or_else(|_| Err(Error::runtime("ThreadPanic", "worker panicked", 134)))
        {
            first.get_or_insert(e);
        }
    }
    if !stuck.is_empty() {
        let e = Error::runtime(
            "ShutdownStuck",
            format!("threads not exiting: {}", stuck.join(", ")),
            134,
        );
        eprintln!("hft-core: {} {e}", utc_stamp());
        return Err(first.unwrap_or(e));
    }
    first.map_or(Ok(()), Err)
}
pub fn run(l: Loaded, duration_ms: Option<u64>, secrets: crate::secrets::SecretFiles) -> Result<u8, Error> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (l, duration_ms, secrets);
        Err(Error::runtime(
            "HostCheck",
            "runtime requires Linux (tmpfs journal and Unix ctl); --check and schemas are portable",
            3,
        ))
    }
    #[cfg(target_os = "linux")]
    {
        run_linux(l, duration_ms, secrets)
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn run_linux(
    mut l: Loaded,
    duration_ms: Option<u64>,
    secrets: crate::secrets::SecretFiles,
) -> Result<u8, Error> {
    // Every mode-dependent branch below reads the assembly resolved at load.
    let assembly = l.assembly;
    let dry = assembly.is_sim();
    let c = &l.cfg;
    l.startup_trace.mark(3, false);
    #[cfg(target_os = "linux")]
    let signals = Arc::new(crate::signals::Signals::block()?);
    let lease = crate::recovery::InstanceLease::acquire(&c.paths.journal_cold, storage::instance(c))?;
    if c.paths
        .ctl_socket
        .try_exists()
        .map_err(|_| Error::runtime("CtlSocket", "cannot inspect socket path", 3))?
    {
        return Err(Error::runtime(
            "InstanceBusy",
            "ctl socket already exists; verify its owner before removing it",
            3,
        ));
    }
    let clock = TscClock::calibrate(&hft_rt::CalibConfig::default())
        .map_err(|e| Error::runtime("Clock", format!("{e:?}"), 3))?;
    // The published TSC → UTC line of the non-sequencer threads; housekeeping is its writer.
    let (utc, mut utc_pub) = hft_rt::UtcClock::shared(&clock, c.clock.interval_ms);
    // A partial recovery implementation must never silently reset account/OMS state.
    let jcfg = storage::config(c, "seq", hft_journal::JournalKind::Seq, VenueId(0));
    let cold = c.paths.journal_cold.join(storage::instance(c).as_str());
    if jcfg.hot_journal_dir().is_dir() || cold.join("seq").is_dir() {
        return Err(Error::runtime(
            "RecoveryRequired",
            "existing journal requires recovery support; preserve it and use a fresh directory for a new paper session",
            4,
        ));
    }
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(c.paths.ctl_socket.clone());
    let meta = hft_journal::DurableMeta::open(&c.paths.journal_cold, &storage::instance(c))
        .map_err(|e| Error::runtime("Epoch", e.to_string(), 4))?;
    // Recovery is deliberately rejected above; the fresh session has no snapshot epoch.
    let epoch = lease.reserve_epoch(&meta, None)?;
    #[cfg(target_os = "linux")]
    crate::budget::preflight(c, assembly)?;
    l.setup = hft_sequencer::EngineSetup::decode_blob(
        &l.configs.engine_blob,
        &l.rd,
        hft_sequencer::SegClock {
            tsc_hz_milli: clock.hz_milli,
            calib_tsc: clock.base.tsc.0,
            calib_realtime_ns: clock.base.realtime_ns,
        },
        epoch,
        0,
    )
    .map_err(|e| Error::config("EngineMismatch", format!("{e:?}")))?;
    let mut sides = Vec::new();
    let (ctl_tx, ctl_rx) = byte_ring(c.rings.bytes)?;
    let (mut hk_tx, hk_rx) = ring(c.rings.elements)?;
    let (sim_tx, sim_rx) = byte_ring(c.rings.bytes)?;
    let (sim_orders, sim_out) = ring(c.rings.elements)?;
    let (parsed_tx, parsed_rx) = Box::leak(Box::new(hft_rt::Mailbox::new())).split();
    let (retired_tx, retired_rx) = Box::leak(Box::new(hft_rt::Mailbox::new())).split();
    let (_internal_tx, internal_rx) = ring::<hft_ctl::InternalCmd>(16)?;
    let heartbeat_dir = c.paths.journal_ram.join(storage::instance(c).as_str());
    std::fs::create_dir_all(&heartbeat_dir)
        .map_err(|_| Error::runtime("Heartbeat", "cannot create instance heartbeat directory", 3))?;
    let heartbeat = hft_rt::HeartbeatBlock::create(&heartbeat_dir.join("heartbeat"), if dry { 2 } else { 1 })
        .map_err(|e| Error::runtime("Heartbeat", format!("{e:?}"), 3))?;
    let hb = heartbeat.sequencer();
    let mut paper_feeds = Vec::new();
    let mut feed_inputs = Vec::new();
    let mut venue_tasks = Vec::new();
    let mut gateway_inputs = Vec::new();
    let mut initial_balances = Vec::new();
    let mut initial_positions = Vec::new();
    let rd: &'static hft_refdata::RefData = Box::leak(Box::new(l.rd));
    l.startup_trace.mark(3, true);
    l.startup_trace.mark(4, false);
    for (name, v) in &c.venues {
        let (bbo_tx, bbo_rx) = ring(c.rings.elements)?;
        let (md_tx, md_rx) = ring(c.rings.elements)?;
        let (bulk_tx, bulk_rx) = byte_ring(c.rings.bytes)?;
        let (feed_tx, feed_rx) = ring(c.rings.elements)?;
        feed_inputs.push(io::FeedInput {
            venue: VenueId(v.id),
            bbo: bbo_rx,
            md: md_rx,
            bulk: bulk_rx,
            commands: feed_tx,
        });
        if dry && !v.live_feed {
            let p = c
                .sim
                .as_ref()
                .ok_or_else(|| Error::config("SimConfig", "missing sim"))?;
            let quotes: Vec<_> = p
                .quotes
                .iter()
                .filter(|q| rd.inst(hft_types::InstId(q.inst)).venue == VenueId(v.id))
                .cloned()
                .collect();
            let subscribed = vec![false; quotes.len()];
            paper_feeds.push(paper::Feed {
                venue: VenueId(v.id),
                out: bbo_tx,
                commands: feed_rx,
                quotes,
                subscribed,
                clock: utc.clone(),
                interval_ticks: (u128::from(clock.hz_milli) * u128::from(p.quote_interval_ms) / 1_000_000)
                    as u64,
                next: 0,
                update: 0,
            });
            let _ = (md_tx, bulk_tx);
        } else {
            let vtask = crate::venues::assemble(
                c, &l.setup, name, v, rd, clock, &utc, epoch, bbo_tx, md_tx, bulk_tx, feed_rx, &secrets,
            )?;
            let (task, gw, js, baselines) = vtask;
            for b in baselines {
                initial_balances.extend(b.balances);
                initial_positions.extend(b.positions);
            }
            venue_tasks.push(task);
            if let Some(g) = gw {
                gateway_inputs.push(g);
            }
            sides.extend(js);
        }
    }
    drop(secrets); // zeroize raw input buffers after handing credentials to their owners
    let (writer, seq_side) = storage::open(jcfg, epoch)?;
    sides.push(seq_side);
    l.startup_trace.mark(4, true);
    l.startup_trace.mark(5, false);
    // Gateway order must be the same as ECF1/GwMap's kill-word order.
    gateway_inputs.sort_by_key(|g| l.setup.gws.venues.iter().position(|v| *v == g.venue));
    let mut status = hft_sequencer::snapshot::clock_status(clock.base, clock.hz_milli);
    status.source = 2;
    status.state = 2;
    let env = io::Io {
        startup: Default::default(),
        journal: writer,
        clock,
        ctl: io::Inputs::new(ctl_rx),
        hk: hk_rx,
        sim_reports: io::Inputs::new(sim_rx),
        sim_orders,
        feeds: feed_inputs,
        gws: gateway_inputs,
        parsed: parsed_rx,
        retired: retired_tx,
        retired_pending: None,
        clock_status: status,
        profile: hft_sequencer::snapshot::host_profile(clock.hz_milli),
        orders: 0,
        cancels: 0,
        reports: 0,
    };
    let active_params = l
        .configs
        .params
        .iter()
        .map(|(b, _)| {
            hft_sequencer::parsed::parse_params::<Strategies>(
                b,
                crate::config::digest(b),
                rd,
                l.setup.caps.max_groups_per_instance,
                None,
            )
            .map(|p| hft_sequencer::ParsedConfig::StrategyParams(Box::new(p)))
            .map_err(|e| Error::config("StrategyParams", e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut seq = Sequencer::boot_accounts(
        l.configs,
        &l.setup,
        LiveDriver::new(
            env,
            LiveCfg {
                source: 1,
                feed_stage_cap: c.rings.elements,
                ..LiveCfg::default()
            },
        ),
        hb,
        &initial_balances,
        &initial_positions,
    )
    .map_err(|e| Error::config("Assembly", format!("{e:?}")))?;
    seq.step(); // publish EngineClock before readers and runtime inputs can reach the sequencer
    l.startup_trace.mark(5, true);
    // One producer owns hk throughout serial startup. Publish all buffered Meta before READY.
    for note in l.startup_trace.notes(c.startup) {
        if note.v[0] == 4 && note.v[1] == 0 {
            for warning in rd.warnings() {
                let warning = crate::startup::note(
                    crate::startup::REFDATA_WARNING,
                    hft_event::NoteLevel::P1,
                    [
                        warning.code as u64,
                        u64::from(warning.section),
                        u64::from(warning.row),
                        0,
                    ],
                );
                hk_tx
                    .try_push(HkMsg::Note(warning))
                    .map_err(|_| Error::runtime("Startup", "startup metadata ring full", 3))?;
            }
        }
        hk_tx
            .try_push(HkMsg::Note(note))
            .map_err(|_| Error::runtime("Startup", "startup metadata ring full", 3))?;
    }
    while seq.driver_mut().io.hk.len_hint() != 0 {
        match seq.step() {
            StepOutcome::Idle | StepOutcome::Busy => {}
            other => return Err(Error::runtime("Startup", format!("{other:?}"), 4)),
        }
        seq.driver_mut().taken_log.clear();
    }
    heartbeat.set_state(hft_rt::HkLine::WARMUP);
    if let Some(parent) = c.paths.ctl_socket.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::runtime("CtlSocket", e.to_string(), 3))?;
    }
    let gwcfg = hft_ctl::GatewayConfig::new(
        c.paths.ctl_socket.clone(),
        l.principals,
        l.setup.seq.advisor_mem_budget_mib,
        l.setup.seq.advisor_ckpt_max_mib,
        l.setup.caps.max_groups_per_instance,
    );
    let ctl_rd = hft_refdata::RefData::load(rd.blob(), None)
        .map_err(|e| Error::config("Refdata", format!("{e:?}")))?;
    #[cfg(target_os = "linux")]
    let mut ctl = hft_ctl::gateway::Gateway::<Strategies, _, _>::new(
        gwcfg,
        ctl_rd,
        io::AtomicCtl(ctl_tx),
        c.rings.bytes,
        parsed_tx,
        retired_rx,
        internal_rx,
        |_, _| Err(1),
        hft_sequencer::config_blob::decode_risk_blob,
        active_params,
        storage::reader(c)?,
        utc.clone(),
        Default::default(),
    )
    .map_err(|e| Error::runtime("CtlSocket", format!("{e:?}"), 3))?;
    #[cfg(not(target_os = "linux"))]
    let mut ctl = hft_ctl::Core::<Strategies, _>::new(
        gwcfg,
        ctl_rd,
        io::AtomicCtl(ctl_tx),
        parsed_tx,
        retired_rx,
        |_, _| Err(1),
        hft_sequencer::config_blob::decode_risk_blob,
        active_params,
        utc.clone(),
        Default::default(),
    );
    #[cfg(not(target_os = "linux"))]
    let _ = internal_rx;
    let mut workers = Workers::new(heartbeat);
    let (mut trace_writers, mut trace_drain) = crate::latency_trace::prepare(c, clock)?;
    let seq_trace = trace_writers.remove(&policy(c, "sequencer")?.name);
    workers.trace = trace_writers;
    #[cfg(target_os = "linux")]
    hft_rt::lock_all().map_err(|_| Error::runtime("MemLock", "mlockall failed; check LimitMEMLOCK", 3))?;
    let mut persist_workers = Workers::new(heartbeat);
    let mut persist = storage::Persist::new(sides);
    let persist_utc = utc.clone();
    persist_workers.spawn(policy(c, "persist")?, move |stop, t| {
        let start = Instant::now();
        while !stop.load(Ordering::Acquire) {
            t.heartbeat.beat();
            trace_drain.poll()?;
            if !persist.poll(
                start.elapsed().as_nanos() as u64,
                persist_utc.utc_ns(hft_rt::tsc_now()),
            )? {
                idle(&t);
            }
        }
        trace_drain.finish()?;
        persist.flush(
            start.elapsed().as_nanos() as u64,
            persist_utc.utc_ns(hft_rt::tsc_now()),
        )
    })?;
    let drain = Arc::new(AtomicBool::new(false));
    let request = drain.clone();
    let cancel = c.shutdown.cancel_open_orders;
    workers.spawn(policy(c, "ctl-gateway")?, move |stop, t| {
        #[cfg(target_os = "linux")]
        use hft_rt::PollTask;
        let mut sent = false;
        while !stop.load(Ordering::Acquire) {
            t.heartbeat.beat();
            let now = hft_rt::tsc_now();
            if request.load(Ordering::Acquire) && !sent {
                #[cfg(target_os = "linux")]
                {
                    sent = ctl.core_mut().on_internal(
                        hft_ctl::InternalCmd::Drain {
                            cancel_resting: cancel,
                        },
                        now,
                    );
                }
                #[cfg(not(target_os = "linux"))]
                {
                    sent = ctl.on_internal(
                        hft_ctl::InternalCmd::Drain {
                            cancel_resting: cancel,
                        },
                        now,
                    );
                }
            }
            #[cfg(target_os = "linux")]
            ctl.poll_once(now);
            idle(&t);
        }
        Ok(())
    })?;
    if dry {
        let p = c
            .sim
            .as_ref()
            .ok_or_else(|| Error::config("SimConfig", "missing sim"))?;
        let tail_cfg = storage::reader_cfg(c);
        let model = paper::model(p)?;
        if let Some(sha) = &model.calib_sha {
            eprintln!("hft-core: sim-gw calibration table sha256 {sha}");
        }
        let gateway = paper::Gateway::new(
            paper::exchange(p, rd, clock.hz_milli)?,
            storage::reader(c)?,
            Box::new(move |seq| {
                let mut r = tail_cfg.clone();
                r.start = hft_journal::StartAt::Seq(seq);
                hft_journal::JournalReader::open(&r)
                    .map_err(|e| Error::runtime("JournalReader", format!("{e:?}"), 4))
            }),
            sim_out,
            sim_tx,
            rd,
            epoch,
        );
        // Pools draw by the condition of the Sim accounts' venues (hft-sim README "条件跟踪与校准表").
        let mut gateway = match model.venue.pools {
            Some(_) => {
                let venues = (1..=255u8)
                    .map(|a| rd.account(hft_types::AccountId(a)))
                    .filter(|m| m.id.0 != 0 && matches!(m.kind, hft_refdata::AccountKind::Sim(_)))
                    .fold(0u64, |v, m| v | m.venues);
                gateway.with_condition(
                    hft_sim::CondTracker::new(model.cond, venues, clock.hz_milli),
                    utc.clone(),
                )
            }
            None => gateway,
        };
        workers.spawn(policy(c, "sim-gw")?, move |stop, t| {
            while !stop.load(Ordering::Acquire) {
                t.heartbeat.beat();
                if !gateway.poll(hft_rt::tsc_now())? {
                    idle(&t);
                }
            }
            Ok(())
        })?;
        workers.spawn(policy(c, "paper-feed")?, move |stop, t| {
            while !stop.load(Ordering::Acquire) {
                t.heartbeat.beat();
                for f in &mut paper_feeds {
                    f.poll(hft_rt::tsc_now())?;
                }
                idle(&t);
            }
            Ok(())
        })?;
    }
    // Network venues: every venue in network mode, `live_feed` venues in --dry-run.
    for task in venue_tasks {
        task.spawn(&mut workers)?;
    }
    let ready = Arc::new(AtomicBool::new(false));
    let ready_sent = Arc::new(AtomicBool::new(false));
    let hk_ready_sent = ready_sent.clone();
    let startup_phase = Arc::new(AtomicU8::new(0));
    let hk_startup_phase = startup_phase.clone();
    let hk_ready = ready.clone();
    #[cfg(target_os = "linux")]
    let hk_drain = drain.clone();
    #[cfg(target_os = "linux")]
    let hk_signals = signals.clone();
    let mut hk = hk_tx;
    let threshold = c.clock.step_threshold_ns;
    let interval = Duration::from_millis(u64::from(c.clock.interval_ms));
    let anchor_every = c.clock.anchor_every();
    // This is an observation of sequencer-owned state, never a decision input.
    let observed_state = Arc::new(AtomicU8::new(hft_rt::HkLine::WARMUP));
    let hk_state = observed_state.clone();
    let stale_after = Duration::from_millis(u64::from(c.watchdog.hb_stale_ms));
    workers.spawn(policy(c, "housekeeping")?, move |stop, t| {
        let mut tracker = hft_rt::DriftTracker::new(threshold);
        let mut anchor = AnchorTick::new(anchor_every);
        let mut last = None;
        let mut next = Instant::now();
        let mut seq_count = hb.read();
        let mut seq_progress = Instant::now();
        let mut announced = false;
        let mut status_step = 5;
        while !stop.load(Ordering::Acquire) {
            t.heartbeat.beat();
            #[cfg(target_os = "linux")]
            if hk_signals.drain_requested()? {
                hk_drain.store(true, Ordering::Release);
            }
            if !announced {
                if !hk_ready.load(Ordering::Acquire) {
                    idle(&t);
                    continue;
                }
                notify("READY=1")?;
                announced = true;
                hk_ready_sent.store(true, Ordering::Release);
            }
            let phase = hk_startup_phase.load(Ordering::Acquire).min(8);
            while status_step < phase {
                status_step += 1;
                notify(&format!("STATUS=Startup step {status_step}"))?;
            }
            heartbeat.set_state(hk_state.load(Ordering::Acquire));
            let count = hb.read();
            if count != seq_count {
                seq_count = count;
                seq_progress = Instant::now();
            }
            if Instant::now() >= next {
                let pair = hft_rt::sample_pair(&clock, None)
                    .map_err(|e| Error::runtime("Clock", format!("{e:?}"), 3))?;
                let verdict = tracker.push(pair);
                utc_pub.update(&pair, tracker.hz_milli());
                clock_tick(&mut hk, pair, clock.hz_milli, verdict, &mut last)?;
                anchor_tick(&mut hk, pair, verdict, tracker.hz_milli(), &mut anchor)?;
                heartbeat.hk.hk_utc_ns.store(pair.realtime_ns, Ordering::Release);
                if seq_progress.elapsed() < stale_after {
                    notify("WATCHDOG=1")?;
                }
                // Missed periods cannot fabricate several observations of one instant.
                next = Instant::now() + interval;
            }
            idle(&t);
        }
        Ok(())
    })?;
    let stop = workers.stop.clone();
    let persist_stop = persist_workers.stop.clone();
    let seq_policy = policy(c, "sequencer")?;
    let seq_slot = claim(heartbeat, &seq_policy)?;
    let accounts: Vec<_> = (1..=u8::MAX)
        .map(hft_types::AccountId)
        .filter(|a| rd.account(*a).id.0 != 0)
        .collect();
    let seq_ready = ready.clone();
    let startup_cfg = c.startup;
    let startup_base_us = l.startup_trace.elapsed();
    let startup_base_tsc = hft_rt::tsc_now();
    let seq_thread = std::thread::Builder::new()
        .name(seq_policy.name.clone())
        .spawn(move || -> Result<(u8, u64, u64, u64), Error> {
            if let Some(trace) = seq_trace {
                hft_rt::latency_trace::install(trace)
                    .map_err(|_| Error::runtime("LatencyTrace", "writer already installed", 3))?;
            }
            pin(&seq_policy)?;
            seq_ready.store(true, Ordering::Release);
            let mut running = false;
            let mut startup_started = false;
            loop {
                seq_slot.beat();
                if stop.load(Ordering::Acquire) || persist_stop.load(Ordering::Acquire) {
                    return Err(Error::runtime(
                        "WorkerFailed",
                        "worker failed; journal retained",
                        3,
                    ));
                }
                if let Some(p) = seq.driver_mut().io.retired_pending.take() {
                    seq.driver_mut().io.retired_pending = seq.driver_mut().io.retired.try_send(p).err();
                }
                let outcome = seq.step();
                let released = accounts.iter().all(|a| seq.engine().risk.mode.released(*a));
                if !startup_started && ready_sent.load(Ordering::Acquire) {
                    let now = hft_rt::tsc_now();
                    let elapsed = startup_base_us
                        + (u128::from(now.0.saturating_sub(startup_base_tsc.0)) * 1_000_000_000
                            / u128::from(clock.hz_milli)) as u64;
                    seq.driver_mut().io.startup =
                        crate::startup::Live::start(startup_cfg, elapsed, now, clock.hz_milli);
                    startup_started = true;
                }
                if startup_started && seq.driver_mut().io.startup.phase() < 9 {
                    let bits = hft_risk::mode::gate_bits::FEEDS | hft_risk::mode::gate_bits::ORDER_CONNS;
                    let connected = accounts
                        .iter()
                        .all(|a| seq.engine().risk.mode.gate_bits(*a) & bits == bits);
                    seq.driver_mut()
                        .io
                        .startup
                        .observe(hft_rt::tsc_now(), connected, released);
                    startup_phase.store(seq.driver_mut().io.startup.phase(), Ordering::Release);
                }
                if seq.view().draining() {
                    observed_state.store(hft_rt::HkLine::DRAINING, Ordering::Release);
                } else if !running && released {
                    running = true;
                    observed_state.store(hft_rt::HkLine::RUNNING, Ordering::Release);
                }
                seq.driver_mut().taken_log.clear();
                match outcome {
                    StepOutcome::Idle => idle(&seq_policy),
                    StepOutcome::Busy => {}
                    StepOutcome::Done { cause, .. } => {
                        let io = seq.into_driver().io;
                        let counts = (crate::exit_code(cause), io.orders, io.cancels, io.reports);
                        io.journal
                            .seal_and_close()
                            .map_err(|e| Error::runtime("JournalClose", format!("{e:?}"), 4))?;
                        return Ok(counts);
                    }
                    StepOutcome::Fatal(f) => {
                        return Err(Error::runtime("SequencerFatal", format!("{f:?}"), 134));
                    }
                }
            }
        })
        .map_err(|e| Error::runtime("Thread", e.to_string(), 3))?;
    eprintln!(
        "hft-core: STARTED instance={} ctl={}",
        c.instance.name,
        c.paths.ctl_socket.display()
    );
    let start = Instant::now();
    while !seq_thread.is_finished() {
        #[cfg(target_os = "linux")]
        if signals.drain_requested()? {
            drain.store(true, Ordering::Release);
        }
        if duration_ms.is_some_and(|ms| start.elapsed() >= Duration::from_millis(ms)) {
            drain.store(true, Ordering::Release);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let result = seq_thread
        .join()
        .map_err(|_| Error::runtime("ThreadPanic", "sequencer", 134))?;
    if let Err(e) = &result {
        eprintln!("hft-core: {} thread sequencer stopped: {e}", utc_stamp());
    }
    workers.stop.store(true, Ordering::Release);
    let work_result = workers.join_within(SHUTDOWN_WORKERS);
    persist_workers.stop.store(true, Ordering::Release);
    let persist_result = persist_workers.join_within(SHUTDOWN_PERSIST);
    work_result?;
    persist_result?;
    let (code, orders, cancels, reports) = result?;
    // This is the release authority consumed by hft-shm-release: never publish it
    // on a worker failure, journal close failure, or before the persister finishes.
    heartbeat.set_state(hft_rt::HkLine::STOPPED);
    notify("STOPPING=1")?;
    println!(
        "hft-core: STOPPED exit={code} orders={orders} cancels={cancels} order_reports={reports} persisted=true"
    );
    Ok(code)
}
