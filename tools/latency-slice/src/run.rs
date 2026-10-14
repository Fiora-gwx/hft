//! One run (main or dry): refdata, clock, journals, rings, threads, shutdown order and report
//! (README "线程布局", "收尾顺序", "产出").

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use hft_event::{FeedCmd, FeedMsg, GwCmd, HkMsg, HostProfile, RepMsg};
use hft_journal::JournalKind;
use hft_rt::{Backing, KillWord, TscClock, TscUtc, byte_spsc, spsc};
use hft_types::{KillBits, VenueId};
use hft_venue::{
    FeedCfg, FeedDeps, FeedEndpoints, FeedRoutes, FeedRuntime, GwAccountSpec, GwCfg, GwDeps, GwRuntime,
    RawMode, ReadyPollMode, SnapReq, ready_poll_on,
};
use hft_venue_proto::{Credential, VenueEnv, VenueProtocol};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use venue_binance::{FeedCtx, GwCtx, Spot, Templates, Usdm};
use zerocopy::FromZeros;

use crate::config::{Config, Idle, Keys, Product, RawOpt};
use crate::dry::{LoopGw, ReplayFeed, generate};
use crate::hk::{Hk, calibrate, nominal_hz};
use crate::journal::{self, Boot, JSide, Persist, jcfg, roll_info};
use crate::live::{self, Ballast, Client, EP_BBO, EP_DEPTH, EP_ORDER};
use crate::rd::{self, ACCOUNT, Built};
use crate::report::{self, ReportCfg};
use crate::seq::{Halt, RealClock, Seq, SeqCfg, SeqIn, SeqInst, SeqOut};
use crate::threads::{Stops, idle, mono_ns, spawn, utc_ns};

/// What to run.
#[derive(Clone, Debug)]
pub enum Kind {
    Live {
        keys: Option<Keys>,
        print_conn_stats: bool,
    },
    Dry {
        seconds: u32,
        seed: u64,
    },
}

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    pub code: i32,
    pub report: Option<Value>,
    pub message: String,
}

fn fail(code: i32, m: impl Into<String>) -> Outcome {
    Outcome {
        code,
        report: None,
        message: m.into(),
    }
}

/// SIGINT/SIGTERM seen (README "安全约束" 5).
static SIGNALLED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_signal(_s: libc::c_int) {
    SIGNALLED.store(true, Ordering::Release);
}

fn install_signals() {
    #[cfg(unix)]
    {
        // SAFETY: the handler only stores into an atomic (async-signal-safe).
        unsafe {
            libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
            libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        }
    }
}

fn text_field<const N: usize>(s: &str) -> [u8; N] {
    let mut o = [0u8; N];
    for (d, c) in o.iter_mut().zip(s.bytes()) {
        *d = c;
    }
    o
}

fn read_trim(p: &str) -> String {
    std::fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// The `HostProfile` of this host (`profile_sha256` of `/run/hft/hostprofile.json` when present).
pub fn host_profile(hz_milli: u64) -> HostProfile {
    let mut h = HostProfile::new_zeroed();
    let boot = read_trim("/proc/sys/kernel/random/boot_id").replace('-', "");
    for (i, d) in h.boot_id.iter_mut().enumerate() {
        *d = u8::from_str_radix(boot.get(2 * i..2 * i + 2).unwrap_or("00"), 16).unwrap_or(0);
    }
    let prof = std::fs::read("/run/hft/hostprofile.json").unwrap_or_else(|_| hostname().into_bytes());
    h.profile_sha256 = Sha256::digest(&prof).into();
    h.release = text_field(&format!("slice-{}", env!("CARGO_PKG_VERSION")));
    h.kernel = text_field(&read_trim("/proc/sys/kernel/osrelease"));
    h.tsc_hz_milli = hz_milli;
    h
}

pub fn hostname() -> String {
    let h = read_trim("/proc/sys/kernel/hostname");
    if h.is_empty() {
        std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".into())
    } else {
        h
    }
}

/// The sequencer parameters of `cfg`.
pub fn seq_cfg(cfg: &Config, built: &Built, clock: &TscClock) -> Result<SeqCfg, String> {
    let rd = built.rd;
    let insts = built
        .insts
        .iter()
        .filter(|i| i.probed)
        .map(|i| {
            let row = rd.inst(i.id);
            SeqInst {
                inst: i.id,
                tick: row.tick,
                lot: row.lot,
                min_qty: row.min_qty.0,
                min_notional: row.min_notional,
                exp: hft_types::AssetExp {
                    px_exp: row.px_exp,
                    qty_exp: row.qty_exp,
                },
            }
        })
        .collect();
    Ok(SeqCfg {
        venue: rd::venue(cfg.product),
        account: ACCOUNT,
        insts,
        orders_enabled: cfg.orders_enabled,
        probe_period_ms: cfg.probe_period_ms,
        burst_period_ms: cfg.burst_period_ms,
        far_bp: cfg.far_bp,
        quote_side: cfg.quote_side,
        spot: cfg.product == Product::Spot,
        max_spread_bp: cfg.max_spread_bp,
        size_margin_pct: cfg.size_margin_pct,
        quote_rest_ms: cfg.quote_rest_ms,
        max_consec_fail: cfg.max_consec_fail,
        max_open_orders: cfg.max_open_orders,
        max_order_notional: hft_types::Notional(
            cfg.max_order_notional
                .raw(-6)
                .ok_or("orders.max_order_notional not representable")?,
        ),
        max_orders: cfg.max_orders,
        ts_expect_hw: cfg.ts_expect_hw,
        ticks_per_ms: (clock.hz_milli / 1_000_000).max(1),
        hz_milli: clock.hz_milli,
    })
}

/// Runs `kind` with `cfg`.
pub fn execute(cfg: &Config, kind: Kind) -> Outcome {
    match cfg.product {
        Product::Usdm => execute_p::<Usdm>(cfg, kind, FeedCtx::USDM),
        Product::Spot => execute_p::<Spot>(cfg, kind, FeedCtx::SPOT),
    }
}

/// The hot directory of one run (removed after the cold copies are complete).
fn run_hot(cfg: &Config) -> PathBuf {
    cfg.hot_dir
        .join(format!("run-{}-{}", std::process::id(), utc_ns() / 1_000_000_000))
}

struct Journals {
    seq: hft_journal::JournalWriter,
    raw_feed: Option<hft_journal::JournalWriter>,
    raw_gw: Option<hft_journal::JournalWriter>,
    sides: Vec<JSide>,
    boot: Boot,
}

fn open_journals(
    cfg: &Config,
    hot: &Path,
    venue: VenueId,
    built: &Built,
    clock: &TscClock,
    raw: bool,
) -> Result<Journals, String> {
    let profile = host_profile(clock.hz_milli);
    let boot = Boot::new(profile, built.blob.clone());
    let cold = &cfg.cold_dir;
    let (mut seq, sseq) = journal::open(
        jcfg("seq", JournalKind::Seq, VenueId(0), hot, cold, cfg.segment_mib),
        profile.profile_sha256,
    )?;
    let now = hft_rt::tsc_now().0;
    let utc = utc_ns();
    let configs = [boot.blob_ref];
    let info = roll_info(now, utc, clock.hz_milli, &configs);
    boot.roll(&mut seq, hft_journal::RollReason::Startup, &info)?;
    let mut sides = vec![sseq];
    let (mut raw_feed, mut raw_gw) = (None, None);
    if raw {
        let vname = match venue {
            VenueId::BINANCE_USDM => "binance-usdm",
            _ => "binance-spot",
        };
        let rinfo = roll_info(now, utc, clock.hz_milli, &[]);
        for (name, slot) in [
            (format!("raw-feed-{vname}-bbo"), &mut raw_feed),
            (format!("raw-gw-{vname}"), &mut raw_gw),
        ] {
            let (mut w, mut side) = journal::open(
                jcfg(&name, JournalKind::Raw, venue, hot, cold, cfg.segment_mib),
                profile.profile_sha256,
            )?;
            journal::first_roll_raw(&mut w, &mut side, &rinfo)?;
            sides.push(side);
            *slot = Some(w);
        }
    }
    Ok(Journals {
        seq,
        raw_feed,
        raw_gw,
        sides,
        boot,
    })
}

fn execute_p<P>(cfg: &Config, kind: Kind, feed_ctx: FeedCtx) -> Outcome
where
    P: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx, Templates = Templates> + Send,
{
    let built = match rd::build(cfg) {
        Ok(b) => b,
        Err(e) => return fail(2, e.to_string()),
    };
    let clock = match calibrate() {
        Ok(c) => c,
        Err(e) => return fail(3, e),
    };
    let utc_conv: TscUtc = clock.tsc_utc(&clock.base);
    let venue = P::VENUE;
    if journal::exists(&cfg.cold_dir, "seq") {
        return fail(
            2,
            format!(
                "{} already holds a seq journal: give a new --out",
                cfg.cold_dir.display()
            ),
        );
    }
    let hot = run_hot(cfg);
    let live = matches!(kind, Kind::Live { .. });
    let Journals {
        seq: seq_w,
        raw_feed,
        raw_gw,
        sides,
        boot,
    } = match open_journals(cfg, &hot, venue, &built, &clock, live) {
        Ok(j) => j,
        Err(e) => return fail(3, e),
    };
    let scfg = match seq_cfg(cfg, &built, &clock) {
        Ok(s) => s,
        Err(e) => return fail(2, e),
    };
    let ticks_per_us = (clock.hz_milli / 1_000_000_000).max(1);
    install_signals();
    let stops = Stops::new();
    let started_utc = utc_ns();

    // Rings.
    let mk = |e: hft_rt::RtErr| format!("ring: {e:?}");
    let rings = (|| -> Result<_, String> {
        Ok((
            spsc::<HkMsg>(256, Backing::Heap).map_err(mk)?,
            spsc::<RepMsg>(8192, Backing::Heap).map_err(mk)?,
            spsc::<FeedMsg>(8192, Backing::Heap).map_err(mk)?,
            spsc::<GwCmd>(1024, Backing::Heap).map_err(mk)?,
            spsc::<FeedCmd>(256, Backing::Heap).map_err(mk)?,
        ))
    })();
    let ((hk_tx, hk_rx), (rep_tx, rep_rx), (bbo_tx, bbo_rx), (out_tx, out_rx), (fc_tx, fc_rx)) = match rings {
        Ok(r) => r,
        Err(e) => return fail(3, e),
    };
    let th = cfg.threads;
    let dry = matches!(kind, Kind::Dry { .. });
    // Dry runs never pin and park when idle.
    let place = |t: crate::config::ThreadCfg| {
        if dry {
            crate::config::ThreadCfg {
                core: None,
                idle: Idle::Park,
            }
        } else {
            t
        }
    };

    // ---- persist.
    let s = Arc::clone(&stops);
    let tc = place(th.persist);
    let persist = match spawn("persist", tc, &stops, move || {
        let mut p = Persist::new(sides);
        while !s.persist.load(Ordering::Acquire) {
            if !p.poll(mono_ns(), utc_ns()) {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        p.flush(mono_ns(), utc_ns());
    }) {
        Ok(h) => h,
        Err(e) => return fail(3, e),
    };

    // ---- hk.
    let s = Arc::clone(&stops);
    let tc = place(th.hk);
    let mut hkr = match Hk::new(clock, cfg.phc.as_deref(), !dry) {
        Ok(h) => h,
        Err(e) => return fail(3, e),
    };
    let run_ms: u64 = match &kind {
        Kind::Dry { seconds, .. } => u64::from(*seconds) * 1_000,
        Kind::Live { .. } => u64::from(cfg.minutes) * 60_000,
    };
    let mut hk_tx = hk_tx;
    let hk = match spawn("hk", tc, &stops, move || {
        let t0 = Instant::now();
        let mut next = Instant::now();
        while !s.hk.load(Ordering::Acquire) {
            if Instant::now() >= next {
                let _ = hk_tx.try_push(HkMsg::ClockStatus(hkr.sample()));
                next += Duration::from_millis(1_000);
            }
            if t0.elapsed() >= Duration::from_millis(run_ms) || SIGNALLED.load(Ordering::Acquire) {
                s.drain.store(true, Ordering::Release);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }) {
        Ok(h) => h,
        Err(e) => return fail(3, e),
    };

    // ---- seq.
    let s = Arc::clone(&stops);
    let tc = place(th.seq);
    let seq_h = match spawn("seq", tc, &stops, move || {
        let mut seq = Seq::new(
            scfg,
            RealClock,
            seq_w,
            boot,
            utc_conv,
            SeqIn {
                hk: hk_rx,
                rep: rep_rx,
                bbo: bbo_rx,
            },
            SeqOut {
                gw: out_tx,
                feed: fc_tx,
            },
        );
        seq.start();
        let mut ir = 0;
        let mut drain_at: Option<Instant> = None;
        let mut drained_clock: Option<u64> = None;
        loop {
            if seq.poll() {
                ir = 0;
            } else {
                idle(tc.idle, &mut ir);
            }
            if seq.halted().is_some() {
                s.drain.store(true, Ordering::Release);
            }
            if drain_at.is_none() && s.drain.load(Ordering::Acquire) {
                seq.begin_drain(hft_rt::tsc_now().0);
                drain_at = Some(Instant::now());
            }
            if let Some(t) = drain_at {
                if drained_clock.is_none() && seq.drained() {
                    drained_clock = Some(seq.stats().clock_status);
                }
                // Done once drained and one more ClockStatus closed the last samples' interval.
                let closed = drained_clock.is_some_and(|c| seq.stats().clock_status > c);
                if closed || t.elapsed() > Duration::from_secs(6) {
                    break;
                }
            }
        }
        let stats = seq.stats();
        let halted = seq.halted();
        let open = seq.open_orders();
        let mut w = seq.into_writer();
        w.publish();
        let _ = w.seal_and_close();
        (stats, halted, open)
    }) {
        Ok(h) => h,
        Err(e) => return fail(3, e),
    };

    // ---- hot side: feed + gw (+ warm, ballast) per kind.
    let mut hot_handles: Vec<std::thread::JoinHandle<Value>> = Vec::new();
    let mut warm_handle = None;
    match &kind {
        Kind::Dry { seconds, seed } => {
            let probed: Vec<_> = built.insts.iter().filter(|i| i.probed).map(|i| i.id).collect();
            let events = generate(venue, built.rd, &probed, (*seconds).max(1), *seed);
            let tpl = match P::build_templates(
                &live::venue_cfg(VenueEnv::Testnet),
                built.rd,
                ACCOUNT,
                &Credential::default(),
            ) {
                Ok(t) => t,
                Err(e) => return fail(3, format!("templates: {e:?}")),
            };
            let rd = built.rd;
            let s = Arc::clone(&stops);
            let mut feed = ReplayFeed::<P>::new(events, bbo_tx, rd, feed_ctx, ticks_per_us);
            let tc = place(th.feed_bbo);
            match spawn("feed-bbo", tc, &stops, move || {
                let mut ir = 0;
                while !s.hot.load(Ordering::Acquire) {
                    if feed.poll(hft_rt::tsc_now().0) {
                        ir = 0;
                    } else {
                        idle(tc.idle, &mut ir);
                    }
                }
                drop(fc_rx);
                json!({"delivered": feed.delivered, "decode_err": feed.decode_err, "dropped": feed.dropped})
            }) {
                Ok(h) => hot_handles.push(h),
                Err(e) => return fail(3, e),
            }
            let s = Arc::clone(&stops);
            let mut gw = LoopGw::<P>::new(out_rx, rep_tx, rd, tpl, ACCOUNT, cfg.epoch, ticks_per_us);
            let tc = place(th.gw);
            match spawn("gw", tc, &stops, move || {
                let mut ir = 0;
                while !s.hot.load(Ordering::Acquire) {
                    if gw.poll(hft_rt::tsc_now().0) {
                        ir = 0;
                    } else {
                        idle(tc.idle, &mut ir);
                    }
                }
                json!({"sent": gw.sent, "answered": gw.answered, "encode_err": gw.encode_err})
            }) {
                Ok(h) => hot_handles.push(h),
                Err(e) => return fail(3, e),
            }
        }
        Kind::Live {
            keys,
            print_conn_stats,
        } => {
            match spawn_live::<P>(
                cfg,
                &built,
                &clock,
                utc_conv,
                feed_ctx,
                keys.as_ref(),
                *print_conn_stats,
                (raw_feed, raw_gw),
                LiveRings {
                    bbo_tx,
                    fc_rx,
                    out_rx,
                    rep_tx,
                },
                &stops,
            ) {
                Ok((h, w)) => {
                    hot_handles = h;
                    warm_handle = Some(w);
                }
                Err(e) => return fail(3, e),
            }
        }
    }

    // ---- shutdown order.
    let (stats, halted, open) = seq_h.join().unwrap_or_default_tuple();
    stops.hot.store(true, Ordering::Release);
    let mut tasks = Vec::new();
    for h in hot_handles {
        tasks.push(h.join().unwrap_or(Value::Null));
    }
    std::thread::sleep(Duration::from_millis(50));
    stops.warm.store(true, Ordering::Release);
    let held = warm_handle.map_or(0, |h| h.join().unwrap_or(0));
    stops.hk.store(true, Ordering::Release);
    let _ = hk.join();
    stops.persist.store(true, Ordering::Release);
    let _ = persist.join();
    let pin_failed = stops.pin_failed.load(Ordering::Acquire);

    let run = json!({
        "mode": if dry { "dry-run" } else { "run" },
        "product": format!("{:?}", cfg.product).to_lowercase(),
        "insts": built.insts.iter().filter(|i| i.probed).map(|i| i.symbol.clone()).collect::<Vec<_>>(),
        "ballast_conns": if dry { 0 } else { cfg.ballast_conns },
        "host": hostname(),
        "kernel": read_trim("/proc/sys/kernel/osrelease"),
        "tsc_hz_nominal": nominal_hz(),
        "tsc_hz_milli_calibrated": clock.hz_milli,
        "ts_expect_hw": cfg.ts_expect_hw,
        "start_utc_ns": started_utc,
        "end_utc_ns": utc_ns(),
        "stats": format!("{stats:?}"),
        "halted": halted.map(|h| format!("{h:?}")),
        "open_orders_at_end": open,
        "held_user_stream_builds": held,
        "pin_failed": pin_failed,
        "tasks": tasks,
    });
    let rc = ReportCfg {
        burst_gap_ns: cfg.burst_gap_ns,
        cal_max_err_ns: cfg.cal_max_err_ns,
    };
    let report = match report::build(&hot, &cfg.cold_dir, &rc, Some(run)) {
        Ok(v) => v,
        Err(e) => return fail(3, format!("report: {e}")),
    };
    if let Err(e) = report::write(&cfg.out_dir, &report) {
        return fail(3, format!("report: {e}"));
    }
    let _ = std::fs::remove_dir_all(&hot);
    let code = if halted.is_some() || pin_failed { 1 } else { 0 };
    Outcome {
        code,
        report: Some(report),
        message: match halted {
            Some(h) => format!("safety stop: {h:?}"),
            None if pin_failed => "a thread could not be pinned".into(),
            None => "ok".into(),
        },
    }
}

trait JoinDefault {
    fn unwrap_or_default_tuple(self) -> (crate::seq::SeqStats, Option<Halt>, usize);
}

impl JoinDefault for std::thread::Result<(crate::seq::SeqStats, Option<Halt>, usize)> {
    fn unwrap_or_default_tuple(self) -> (crate::seq::SeqStats, Option<Halt>, usize) {
        self.unwrap_or((crate::seq::SeqStats::default(), None, 0))
    }
}

struct LiveRings {
    bbo_tx: hft_rt::Producer<FeedMsg>,
    fc_rx: hft_rt::Consumer<FeedCmd>,
    out_rx: hft_rt::Consumer<GwCmd>,
    rep_tx: hft_rt::Producer<RepMsg>,
}

type Spawned = (Vec<std::thread::JoinHandle<Value>>, std::thread::JoinHandle<u64>);

#[allow(clippy::too_many_arguments)]
fn spawn_live<P>(
    cfg: &Config,
    built: &Built,
    clock: &TscClock,
    utc_conv: TscUtc,
    feed_ctx: FeedCtx,
    keys: Option<&Keys>,
    print_conn_stats: bool,
    raw: (
        Option<hft_journal::JournalWriter>,
        Option<hft_journal::JournalWriter>,
    ),
    r: LiveRings,
    stops: &Arc<Stops>,
) -> Result<Spawned, String>
where
    P: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx, Templates = Templates> + Send,
{
    let net = live::net_config::<P>(cfg)?;
    let n_probe = built.insts.iter().filter(|i| i.probed).count();
    let md_slots = u16::from(cfg.bbo_conns) * 2 + u16::from(cfg.ballast_conns) + 4;
    let arena = live::arena(&net, *clock, md_slots);
    let connector = hft_net::Connector::new(arena, &net).map_err(|e| format!("connector: {e:?}"))?;
    let ring_err = |e: hft_rt::RtErr| format!("ring: {e:?}");
    let th = cfg.threads;
    let ready_on = ready_poll_on(
        ReadyPollMode::Auto,
        if th.feed_bbo.idle == Idle::Spin {
            hft_rt::IdlePolicy::Spin
        } else {
            hft_rt::IdlePolicy::Park { park_us: 100 }
        },
        u16::from(cfg.bbo_conns),
    );
    let ready = if ready_on {
        Some(hft_net::ReadyPoll::new().map_err(|e| format!("ReadyPoll: {e:?}"))?)
    } else {
        None
    };
    let ready_handle = ready.as_ref().map(|r| r.handle());
    let mut clients = Vec::new();
    let mut mk_client = |acct| -> Result<_, String> {
        let (b_tx, b_rx) = spsc::<hft_net::BuildReq>(64, Backing::Heap).map_err(ring_err)?;
        let (h_tx, h_rx) = spsc::<hft_venue::Handover>(64, Backing::Heap).map_err(ring_err)?;
        let (r_tx, r_rx) = spsc::<hft_net::RetireMsg>(64, Backing::Heap).map_err(ring_err)?;
        clients.push(Client {
            build: b_rx,
            handoff: h_tx,
            retire: r_rx,
            acct,
        });
        Ok((b_tx, h_rx, r_tx))
    };
    let (fb, fh, fr) = mk_client(None)?;
    let (gb, gh, gr) = mk_client(Some(ACCOUNT))?;
    let ballast_ring = if cfg.ballast_conns > 0 {
        Some(mk_client(None)?)
    } else {
        None
    };
    let (_snap_tx, snap_rx) = byte_spsc(1 << 16, Backing::Heap).map_err(ring_err)?;
    let (snapreq_tx, snapreq_rx) = spsc::<SnapReq>(64, Backing::Heap).map_err(ring_err)?;
    let fcfg = FeedCfg {
        routes: FeedRoutes::Bbo,
        ready: ready_on,
        max_sub_insts: u32::try_from(n_probe).unwrap_or(1).max(1),
        bbo_conns: cfg.bbo_conns.max(1),
        raw_bbo: match cfg.raw {
            RawOpt::On => RawMode::On,
            RawOpt::Off => RawMode::Off,
        },
        venue: live::venue_cfg(VenueEnv::Prod),
        endpoints: FeedEndpoints {
            bbo: EP_BBO,
            depth: EP_DEPTH,
            md: EP_DEPTH,
            src_ip_idx: 0,
        },
        ..FeedCfg::default()
    };
    let raw_feed = raw.0.ok_or("raw-feed journal missing")?;
    let raw_gw = raw.1.ok_or("raw-gw journal missing")?;
    let mut feed = FeedRuntime::<P>::new(
        fcfg,
        FeedDeps {
            rd: built.rd,
            bbo: Some(r.bbo_tx),
            md: None,
            bulk: None,
            cmds: r.fc_rx,
            snapreq: Some(snapreq_tx),
            snap: Some(snap_rx),
            ready,
            raw: raw_feed,
            handoff: fh,
            retire: fr,
            build: fb,
            clock: utc_conv,
        },
        feed_ctx,
    )
    .map_err(|e| format!("FeedRuntime: {e:?}"))?;
    let gcfg = GwCfg {
        env: VenueEnv::Testnet,
        oms_slots: 128,
        id_table_cap: 1024,
        id_tomb_cap: 256,
        order_endpoint: EP_ORDER,
        user_endpoint: EP_ORDER,
        ..GwCfg::default()
    };
    let rest = Credential::default();
    let tpl = P::build_templates(&gcfg.venue_cfg(), built.rd, ACCOUNT, &rest)
        .map_err(|e| format!("templates: {e:?}"))?;
    let (_acct_tx, acct_rx) = byte_spsc(1 << 16, Backing::Heap).map_err(ring_err)?;
    let kill: &'static KillWord = Box::leak(Box::new(KillWord::default()));
    let mut gw = GwRuntime::<P, 2048>::new(
        gcfg,
        GwDeps {
            rd: built.rd,
            cmds: r.out_rx,
            out: r.rep_tx,
            acct: acct_rx,
            raw: raw_gw,
            kill,
            kill_mask: KillBits::GLOBAL.0 | KillBits::VENUE.0 | KillBits::HARD.0,
            tx_tags: None,
            handoff: gh,
            retire: gr,
            build: gb,
            clock: utc_conv,
        },
        vec![GwAccountSpec {
            account: ACCOUNT,
            src_ip_idx: 0,
            tpl,
            ctx: GwCtx::default(),
        }],
    )
    .map_err(|e| format!("GwRuntime: {e:?}"))?;
    let driver = live::driver::<P>(keys, ACCOUNT, ready_handle)?;
    let mut handles = Vec::new();

    let s = Arc::clone(stops);
    let tc = th.feed_bbo;
    handles.push(spawn("feed-bbo", tc, stops, move || {
        let mut ir = 0;
        while !s.hot.load(Ordering::Acquire) {
            if feed.poll_once(hft_rt::tsc_now().0).did_work {
                ir = 0;
            } else {
                idle(tc.idle, &mut ir);
            }
        }
        let mut conns = Vec::new();
        if print_conn_stats {
            let ids: Vec<_> = feed.conn_ids().collect();
            for id in ids {
                if let Some(c) = feed.conn(id) {
                    let st = c.stats();
                    let line = json!({
                        "conn": id.0, "frames": st.frames, "bytes": st.bytes, "ts_reads": st.ts_reads,
                        "hw_ts_reads": st.hw_ts_reads, "ts_missing_reads": st.ts_reads.saturating_sub(st.hw_ts_reads),
                        "multi_skb": st.multi_skb, "reassembled": st.reassembled,
                    });
                    println!("conn_stats {line}");
                    conns.push(line);
                }
            }
        }
        let n = feed.counters();
        drop(snapreq_rx);
        feed.retire_all(hft_rt::tsc_now().0);
        json!({"conn_stats": conns, "counters": format!("{n:?}")})
    })?);

    let s = Arc::clone(stops);
    let tc = th.gw;
    handles.push(spawn("gw", tc, stops, move || {
        let mut ir = 0;
        while !s.hot.load(Ordering::Acquire) {
            if gw.poll_once(hft_rt::tsc_now().0).did_work {
                ir = 0;
            } else {
                idle(tc.idle, &mut ir);
            }
        }
        let n = gw.counters();
        gw.retire_all(hft_rt::tsc_now().0);
        json!({"counters": format!("{n:?}")})
    })?);

    if let Some((bb, bh, br)) = ballast_ring {
        // Ballast insts: every non-probed inst plus probed insts listed in `ballast.symbols`.
        let ballast_insts: Vec<_> = built
            .insts
            .iter()
            .filter(|i| !i.probed || cfg.ballast_symbols.contains(&i.symbol))
            .map(|i| i.id)
            .collect();
        let tpm = (clock.hz_milli / 1_000_000).max(1);
        let mut ballast = Ballast::new::<P>(cfg.ballast_conns, built.rd, &ballast_insts, bb, bh, br, tpm)?;
        let s = Arc::clone(stops);
        let tc = th.feed_md;
        handles.push(spawn("feed-md", tc, stops, move || {
            let mut ir = 0;
            while !s.hot.load(Ordering::Acquire) {
                if ballast.poll(hft_rt::tsc_now().0) {
                    ir = 0;
                } else {
                    idle(tc.idle, &mut ir);
                }
            }
            ballast.retire_all();
            json!({"ballast_frames": ballast.frames_rx, "ballast_bytes": ballast.bytes_rx, "ballast_drops": ballast.drops})
        })?);
    }

    let s = Arc::clone(stops);
    let tc = th.warm;
    let warm = spawn("warm", tc, stops, move || {
        live::warm_loop::<P>(connector, driver, clients, s, tc.idle)
    })?;
    Ok((handles, warm))
}
