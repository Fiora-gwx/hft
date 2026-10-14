//! Startup ordering, task ownership, supervision and graceful drain.
use crate::{
    Error, Strategies,
    config::{Config, Loaded, Thread},
    io, paper, storage,
};
use hft_event::{HkMsg, SysTimer, TimerCode};
use hft_rt::{Backing, Consumer, Producer, TscClock};
use hft_sequencer::driver::live::{LiveCfg, LiveDriver};
use hft_sequencer::{BootSource, Sequencer, StepOutcome};
use hft_types::VenueId;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
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
    hft_rt::name_current_thread(&t.name);
    if let Some(cpu) = t.core {
        hft_rt::pin_current_thread(cpu)
            .map_err(|e| Error::runtime("Affinity", format!("{}: {e:?}", t.name), 3))?;
    }
    Ok(())
}
pub fn idle(t: &Thread) {
    std::thread::park_timeout(Duration::from_micros(t.park_us));
}

#[derive(Debug)]
pub struct Workers {
    pub stop: Arc<AtomicBool>,
    joins: Vec<JoinHandle<Result<(), Error>>>,
}
impl Default for Workers {
    fn default() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            joins: Vec::new(),
        }
    }
}
impl Workers {
    pub fn spawn(
        &mut self,
        t: Thread,
        f: impl FnOnce(Arc<AtomicBool>, Thread) -> Result<(), Error> + Send + 'static,
    ) -> Result<(), Error> {
        let stop = self.stop.clone();
        self.joins.push(
            std::thread::Builder::new()
                .name(t.name.clone())
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        pin(&t).and_then(|()| f(stop.clone(), t))
                    }))
                    .unwrap_or_else(|_| Err(Error::runtime("ThreadPanic", "worker panicked", 134)));
                    if result.is_err() {
                        stop.store(true, Ordering::Release);
                    }
                    result
                })
                .map_err(|e| Error::runtime("Thread", e.to_string(), 3))?,
        );
        Ok(())
    }
    pub fn join(&mut self) -> Result<(), Error> {
        let mut first = None;
        for j in self.joins.drain(..) {
            if let Err(e) = j
                .join()
                .unwrap_or_else(|_| Err(Error::runtime("ThreadPanic", "worker panicked", 134)))
            {
                first.get_or_insert(e);
            }
        }
        first.map_or(Ok(()), Err)
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.join();
    }
}
pub fn run(l: Loaded, dry: bool, duration_ms: Option<u64>) -> Result<u8, Error> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (l, dry, duration_ms);
        Err(Error::runtime(
            "HostCheck",
            "runtime requires Linux (tmpfs journal and Unix ctl); --check and schemas are portable",
            3,
        ))
    }
    #[cfg(target_os = "linux")]
    {
        run_linux(l, dry, duration_ms)
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn run_linux(mut l: Loaded, dry: bool, duration_ms: Option<u64>) -> Result<u8, Error> {
    let c = &l.cfg;
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
    std::fs::create_dir_all(&cold).map_err(|e| Error::runtime("Journal", e.to_string(), 3))?;
    let lock = cold.join("core.lock");
    let _lock = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .map_err(|_| {
            Error::runtime(
                "InstanceBusy",
                "core.lock exists; verify no process is running before removing a stale lock",
                3,
            )
        })?;
    struct Cleanup(std::path::PathBuf, std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            let _ = std::fs::remove_file(&self.1);
        }
    }
    let _cleanup = Cleanup(lock, c.paths.ctl_socket.clone());
    let meta = hft_journal::DurableMeta::open(&c.paths.journal_cold, &storage::instance(c))
        .map_err(|e| Error::runtime("Epoch", e.to_string(), 4))?;
    let epoch = meta
        .read("engine_epoch")
        .map_err(|e| Error::runtime("Epoch", e.to_string(), 4))?
        .unwrap_or(0)
        .checked_add(1)
        .and_then(|x| u32::try_from(x).ok())
        .ok_or_else(|| Error::runtime("Epoch", "epoch exhausted", 4))?;
    meta.write_sync("engine_epoch", u64::from(epoch))
        .map_err(|e| Error::runtime("Epoch", e.to_string(), 4))?;
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
    let (writer, seq_side) = storage::open(jcfg, epoch)?;
    let mut sides = vec![seq_side];
    let (ctl_tx, ctl_rx) = byte_ring(c.rings.bytes)?;
    let (hk_tx, hk_rx) = ring(c.rings.elements)?;
    let (sim_tx, sim_rx) = byte_ring(c.rings.bytes)?;
    let (sim_orders, sim_out) = ring(c.rings.elements)?;
    let (parsed_tx, parsed_rx) = Box::leak(Box::new(hft_rt::Mailbox::new())).split();
    let (retired_tx, retired_rx) = Box::leak(Box::new(hft_rt::Mailbox::new())).split();
    let (_internal_tx, internal_rx) = ring::<hft_ctl::InternalCmd>(16)?;
    let hb: &'static hft_rt::Heartbeat = Box::leak(Box::new(hft_rt::Heartbeat::new()));
    let mut paper_feeds = Vec::new();
    let mut feed_inputs = Vec::new();
    let mut venue_tasks = Vec::new();
    let mut gateway_inputs = Vec::new();
    let rd: &'static hft_refdata::RefData = Box::leak(Box::new(l.rd));
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
        if dry {
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
                clock,
                interval_ticks: (u128::from(clock.hz_milli) * u128::from(p.quote_interval_ms) / 1_000_000)
                    as u64,
                next: 0,
                update: 0,
            });
            let _ = (md_tx, bulk_tx);
        } else {
            let vtask = crate::venues::assemble(
                c, &l.setup, name, v, rd, clock, epoch, bbo_tx, md_tx, bulk_tx, feed_rx,
            )?;
            let (task, gw, js) = vtask;
            venue_tasks.push(task);
            if let Some(g) = gw {
                gateway_inputs.push(g);
            }
            sides.extend(js);
        }
    }
    // Gateway order must be the same as ECF1/GwMap's kill-word order.
    gateway_inputs.sort_by_key(|g| l.setup.gws.venues.iter().position(|v| *v == g.venue));
    let mut status = hft_sequencer::snapshot::clock_status(clock.base, clock.hz_milli);
    status.source = 2;
    status.state = 2;
    let env = io::Io {
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
    let mut seq = Sequencer::boot(
        BootSource::Empty { configs: l.configs },
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
    )
    .map_err(|e| Error::config("Assembly", format!("{e:?}")))?;
    seq.step(); // publish EngineClock before readers and runtime inputs can reach the sequencer
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
        clock.tsc_utc(&clock.base),
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
        clock.tsc_utc(&clock.base),
        Default::default(),
    );
    #[cfg(not(target_os = "linux"))]
    let _ = internal_rx;
    let mut workers = Workers::default();
    let mut persist_workers = Workers::default();
    let mut persist = storage::Persist::new(sides);
    persist_workers.spawn(policy(c, "persist")?, move |stop, t| {
        let start = Instant::now();
        while !stop.load(Ordering::Acquire) {
            if !persist.poll(
                start.elapsed().as_nanos() as u64,
                clock.to_utc_ns(hft_rt::tsc_now()),
            )? {
                idle(&t);
            }
        }
        persist.flush(
            start.elapsed().as_nanos() as u64,
            clock.to_utc_ns(hft_rt::tsc_now()),
        )
    })?;
    let drain = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(sig, drain.clone())
            .map_err(|e| Error::runtime("Signal", e.to_string(), 3))?;
    }
    let request = drain.clone();
    let cancel = c.shutdown.cancel_open_orders;
    workers.spawn(policy(c, "ctl-gateway")?, move |stop, t| {
        #[cfg(target_os = "linux")]
        use hft_rt::PollTask;
        let mut sent = false;
        while !stop.load(Ordering::Acquire) {
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
        let mut gateway = paper::Gateway::new(
            paper::exchange(p, rd, clock.hz_milli)?,
            storage::reader(c)?,
            sim_out,
            sim_tx,
            rd,
            epoch,
        );
        workers.spawn(policy(c, "sim-gw")?, move |stop, t| {
            while !stop.load(Ordering::Acquire) {
                if !gateway.poll(hft_rt::tsc_now())? {
                    idle(&t);
                }
            }
            Ok(())
        })?;
        workers.spawn(policy(c, "paper-feed")?, move |stop, t| {
            while !stop.load(Ordering::Acquire) {
                for f in &mut paper_feeds {
                    f.poll(hft_rt::tsc_now())?;
                }
                idle(&t);
            }
            Ok(())
        })?;
    } else {
        for task in venue_tasks {
            task.spawn(&mut workers)?;
        }
    }
    let mut hk = hk_tx;
    let threshold = c.clock.step_threshold_ns;
    let interval = Duration::from_millis(u64::from(c.clock.interval_ms));
    workers.spawn(policy(c, "housekeeping")?, move |stop, t| {
        let mut tracker = hft_rt::DriftTracker::new(threshold);
        let mut last = None;
        let mut next = Instant::now();
        while !stop.load(Ordering::Acquire) {
            if Instant::now() >= next {
                let pair = hft_rt::sample_pair(&clock, None)
                    .map_err(|e| Error::runtime("Clock", format!("{e:?}"), 3))?;
                let verdict = tracker.push(pair);
                let good = matches!(verdict, hft_rt::ClockVerdict::Synced);
                let mut s = hft_sequencer::snapshot::clock_status(pair, clock.hz_milli);
                s.source = 2;
                s.state = if good { 1 } else { 2 };
                hk.try_push(HkMsg::ClockStatus(s))
                    .map_err(|_| Error::runtime("HkFull", "clock record", 3))?;
                if last != Some(good) {
                    let (_, timer_id, _) = TimerCode::Sys(if good {
                        SysTimer::ClockOk
                    } else {
                        SysTimer::ClockBad
                    })
                    .encode()
                    .map_err(|_| Error::runtime("Clock", "timer encoding", 3))?;
                    hk.try_push(HkMsg::Sys { timer_id })
                        .map_err(|_| Error::runtime("HkFull", "clock verdict", 3))?;
                    last = Some(good);
                }
                eprintln!("hft-core: heartbeat={} clock_synced={good}", hb.read());
                next += interval;
            }
            idle(&t);
        }
        Ok(())
    })?;
    let stop = workers.stop.clone();
    let persist_stop = persist_workers.stop.clone();
    let seq_policy = policy(c, "sequencer")?;
    let seq_thread = std::thread::Builder::new()
        .name(seq_policy.name.clone())
        .spawn(move || -> Result<(u8, u64, u64, u64), Error> {
            pin(&seq_policy)?;
            loop {
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
        "hft-core: READY instance={} ctl={}",
        c.instance.name,
        c.paths.ctl_socket.display()
    );
    let start = Instant::now();
    while !seq_thread.is_finished() {
        if duration_ms.is_some_and(|ms| start.elapsed() >= Duration::from_millis(ms)) {
            drain.store(true, Ordering::Release);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let result = seq_thread
        .join()
        .map_err(|_| Error::runtime("ThreadPanic", "sequencer", 134))?;
    workers.stop.store(true, Ordering::Release);
    let work_result = workers.join();
    persist_workers.stop.store(true, Ordering::Release);
    let persist_result = persist_workers.join();
    work_result?;
    persist_result?;
    let (code, orders, cancels, reports) = result?;
    println!(
        "hft-core: STOPPED exit={code} orders={orders} cancels={cancels} order_reports={reports} persisted=true"
    );
    Ok(code)
}
