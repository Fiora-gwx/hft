//! `pm-md-probe`: Polymarket market-data acceptance probe (README "概要"). Runs the production
//! market-data path — hft-net connection, hft-venue `FeedRuntime`, venue-polymarket decoder —
//! against live 5-minute markets, rebuilds every subscribed token's book from the emitted
//! `L2Snapshot`/`L2Batch` events and writes one JSON line per event with the resulting best
//! bid/ask, for comparison with an independent recording. Probe only: no orders, no strategy, no
//! sequencer; never in a production archive. Specification: `tools/pm-md-probe/README.md`.

use std::collections::BTreeMap;
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use hft_event::{ConnRole, EventKind, FeedCmd, FeedMsg, L2BatchHdr, L2SnapHdr, Level};
use hft_journal::{
    InstanceName, JournalConfig, JournalKind, JournalWriter, PageSize, PrepEnv, PreparedSegment,
    RetiredSegment, RollInfo, RollReason, SegmentPreparer, WriterEnv,
};
use hft_net::{BuildReq, ConnArena, Connector, Endpoint, EndpointId, NetConfig, RetireMsg};
use hft_refdata::{
    AssetClass, AssetSpec, Exchange, FeePair, FeeSpec, InstKind, InstSpec, InstStatus, RateLimit, RdbBuilder,
    RefData, StaleGuardKind, TsUnit, VenueSpec,
};
use hft_rt::{Backing, CalibConfig, CalibPair, Consumer, Producer, SysClock, TscClock, byte_spsc, spsc};
use hft_types::{AssetExp, AssetId, InstId, Notional, Px, Qty, Rate, Side, Tsc, VenueId};
use hft_venue::{
    DepthMode, FeedCfg, FeedDeps, FeedEndpoints, FeedRoutes, FeedRuntime, Handover, RawMode, SessEvent,
    SessionCfg, SessionDriver, streams,
};
use hft_venue_proto::{VenueCfg, VenueEnv, VenueProtocol};
use serde::Deserialize;
use venue_polymarket::{FeedCtx, PolymarketClob as Pm};

mod journal;
mod tap;
use tap::Tap;
use zerocopy::FromBytes;

const V: VenueId = VenueId::POLYMARKET_CLOB;
const USDC: AssetId = AssetId(1);
const EP: EndpointId = EndpointId(0);

/// One `--bindings` entry (written by the discovery helper from the Gamma API).
#[derive(Deserialize, Clone, Debug)]
struct Bind {
    /// Slot symbol, e.g. `btc-up-0`.
    slot: String,
    /// Outcome-token id (decimal).
    token: String,
}

#[derive(Debug)]
struct Args {
    bindings: PathBuf,
    seconds: u64,
    out: PathBuf,
    dir: PathBuf,
    src_ip: Ipv4Addr,
    bind_device: String,
    host: String,
    /// `SO_RCVBUF` of the market connections; 0 = hft-net default.
    so_rcvbuf: u32,
    /// hft-venue `feed.depth_conns`: connections per depth group.
    depth_conns: u8,
}

fn usage() -> ! {
    eprintln!(
        "usage: pm-md-probe --bindings FILE --seconds N --out FILE --dir DIR --src-ip IP --bind-device DEV \
         [--host ws-subscriptions-clob.polymarket.com] [--so-rcvbuf BYTES] [--depth-conns 2]"
    );
    std::process::exit(2)
}

fn args() -> Args {
    let mut a = std::env::args().skip(1);
    let (mut bindings, mut seconds, mut out, mut dir, mut src_ip, mut dev) =
        (None, None, None, None, None, None);
    let mut host = "ws-subscriptions-clob.polymarket.com".to_string();
    let mut so_rcvbuf = 0u32;
    let mut depth_conns = 2u8;
    while let Some(k) = a.next() {
        let v = a.next().unwrap_or_else(|| usage());
        match k.as_str() {
            "--bindings" => bindings = Some(PathBuf::from(v)),
            "--seconds" => seconds = v.parse().ok(),
            "--out" => out = Some(PathBuf::from(v)),
            "--dir" => dir = Some(PathBuf::from(v)),
            "--src-ip" => src_ip = v.parse().ok(),
            "--bind-device" => dev = Some(v),
            "--host" => host = v,
            "--so-rcvbuf" => so_rcvbuf = v.parse().unwrap_or_else(|_| usage()),
            "--depth-conns" => depth_conns = v.parse().unwrap_or_else(|_| usage()),
            _ => usage(),
        }
    }
    match (bindings, seconds, out, dir, src_ip, dev) {
        (Some(bindings), Some(seconds), Some(out), Some(dir), Some(src_ip), Some(bind_device)) => Args {
            bindings,
            seconds,
            out,
            dir,
            src_ip,
            bind_device,
            host,
            so_rcvbuf,
            depth_conns,
        },
        _ => usage(),
    }
}

/// Refdata of the run: USDC plus one token asset and one spot inst per binding (ids 1..=n, in
/// file order). Tick 0.01 (px_exp −3), lot 0.000001 (qty_exp −6).
fn refdata(binds: &[Bind]) -> Result<&'static RefData, String> {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    let asset = |id: AssetId, name: String, class| AssetSpec {
        id,
        name: name.into(),
        class,
        exp: AssetExp {
            px_exp: -3,
            qty_exp: -6,
        },
        px_max: Px(1_000_000_000),
        qty_max: Qty(1_000_000_000_000_000),
        dust: Qty(1),
    };
    b.asset(asset(USDC, "USDC".into(), AssetClass::Stable));
    for k in 0..binds.len() {
        b.asset(asset(
            AssetId(k as u16 + 2),
            format!("PMSLOT{k}"),
            AssetClass::Crypto,
        ));
    }
    b.venue(VenueSpec {
        id: V,
        exchange: Exchange::Polymarket,
        product: InstKind::Spot,
        // One market (two tokens) per connection (venue-polymarket README "端点").
        ws_max_streams_per_conn: 2,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 10,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 32,
        client_id_prefix: "".into(),
        max_batch: 1,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::None,
    });
    b.fee(FeeSpec {
        venue: V,
        tier: 0,
        fee: FeePair {
            maker: Rate(0),
            taker: Rate(0),
        },
    });
    b.caps(Pm::default_caps(|_| None));
    for r in Pm::rate_rules() {
        b.limit(
            V,
            RateLimit {
                scope: r.scope,
                bucket: r.bucket,
                limit: r.limit,
                window_ms: r.window_ms,
                default_cost: 1,
            },
        );
    }
    for (k, x) in binds.iter().enumerate() {
        b.inst(InstSpec {
            id: InstId(k as u32 + 1),
            venue: V,
            symbol: x.slot.clone(),
            status: InstStatus::Trading,
            kind: InstKind::Spot,
            asset: AssetId(k as u16 + 2),
            quote: USDC,
            tick: 10,
            lot: 1,
            min_qty: Qty(1),
            max_qty: Qty(1_000_000_000_000),
            min_notional: Notional(1_000_000),
            book_window: 128,
            l2_depth: 100,
            crossed_tolerance: 0,
            tick_str: "0.01".into(),
            lot_str: "0.000001".into(),
            min_qty_str: "0.000001".into(),
            ..InstSpec::default()
        });
    }
    let blob = b.finish();
    RefData::load(&blob, None)
        .map(|r| &*Box::leak(Box::new(r)))
        .map_err(|e| format!("refdata: {e:?}"))
}

/// The raw-feed journal the runtime requires (raw recording is off; only status records land).
fn raw_journal(dir: &std::path::Path) -> Result<JournalWriter, String> {
    let cfg = JournalConfig {
        name: "raw-feed-polymarket".into(),
        kind: JournalKind::Raw,
        venue: V,
        instance: InstanceName::Prod,
        hot_dir: dir.join("hot"),
        cold_root: dir.join("cold"),
        segment_bytes: 64 * 1_048_576,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: 2,
    };
    if !cfg.validate() {
        return Err("journal configuration invalid".into());
    }
    std::fs::create_dir_all(cfg.hot_journal_dir()).map_err(|e| format!("{e}"))?;
    std::fs::create_dir_all(&cfg.cold_root).map_err(|e| format!("{e}"))?;
    let (ptx, prx): (Producer<PreparedSegment>, Consumer<PreparedSegment>) =
        spsc(4, Backing::Heap).map_err(|e| format!("{e:?}"))?;
    let (rtx, _rrx): (Producer<RetiredSegment>, Consumer<RetiredSegment>) =
        spsc(256, Backing::Heap).map_err(|e| format!("{e:?}"))?;
    let mut prep = SegmentPreparer::new(
        &cfg,
        PrepEnv {
            writer_epoch: 1,
            build_git: [0; 20],
            host_profile_sha256: [0; 32],
            to_writer: ptx,
        },
    );
    let (mut w, _) = JournalWriter::open(
        &cfg,
        WriterEnv {
            writer_epoch: 1,
            build_git: [0; 20],
            host_profile_sha256: [0; 32],
            prepared: prx,
            retired: rtx,
        },
    )
    .map_err(|e| format!("journal: {e:?}"))?;
    let _ = prep.poll();
    let now = utc_ns();
    let info = RollInfo {
        utc_ns: now,
        calib: CalibPair {
            tsc: Tsc(hft_rt::tsc_now().0),
            realtime_ns: now,
            phc_ns: 0,
            err_ns: 0,
        },
        tsc_hz_milli: 0,
        configs: &[],
    };
    let b = w
        .roll(RollReason::Startup, &info)
        .map_err(|e| format!("roll: {e:?}"))?;
    b.finish().map_err(|e| format!("{e:?}"))?;
    w.publish();
    Ok(w)
}

fn utc_ns() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn calibrate() -> Result<TscClock, String> {
    let cfg = CalibConfig::default();
    match TscClock::calibrate(&cfg) {
        Ok(c) => Ok(c),
        Err(_) => TscClock::calibrate_with(
            &mut SysClock,
            None,
            &CalibConfig {
                cpuid_tolerance_ppm: u32::MAX,
                ..cfg
            },
        )
        .map_err(|e| format!("calibration: {e:?}")),
    }
}

/// One book: price → size per side.
#[derive(Default)]
struct Book {
    bids: BTreeMap<i64, i64>,
    asks: BTreeMap<i64, i64>,
}

impl Book {
    fn best(&self) -> (Option<i64>, Option<i64>) {
        (
            self.bids.keys().next_back().copied(),
            self.asks.keys().next().copied(),
        )
    }
}

fn levels(body: &[u8], n: usize) -> Vec<Level> {
    (0..n)
        .filter_map(|k| {
            let at = k * size_of::<Level>();
            Level::read_from_bytes(body.get(at..at + size_of::<Level>())?).ok()
        })
        .collect()
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.first().map(String::as_str) == Some("journal") {
        if let Err(e) = journal::run(&raw[1..]) {
            eprintln!("pm-md-probe: {e}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(e) = run(args()) {
        eprintln!("pm-md-probe: {e}");
        std::process::exit(1);
    }
}

fn run(a: Args) -> Result<(), String> {
    let text = std::fs::read_to_string(&a.bindings).map_err(|e| format!("{}: {e}", a.bindings.display()))?;
    let binds: Vec<Bind> = serde_json::from_str(&text).map_err(|e| format!("bindings: {e}"))?;
    if binds.is_empty() {
        return Err("no bindings".into());
    }
    let rd = refdata(&binds)?;
    let mut ctx = FeedCtx::with_capacity(binds.len());
    for (k, x) in binds.iter().enumerate() {
        ctx.bind(InstId(k as u32 + 1), x.token.as_bytes())
            .map_err(|e| format!("bind {}: {e:?}", x.slot))?;
    }
    let clock = calibrate()?;
    let utc = clock.tsc_utc(&clock.base);
    let ticks_per_s = clock.hz_milli / 1000;

    let mut path = hft_venue_proto::EndpointSpec::default();
    Pm::endpoint(
        ConnRole::Md,
        Pm::routes().first().map(|r| r.id).unwrap_or_default(),
        &VenueCfg {
            env: VenueEnv::Prod,
            stale_guard_ms: 1500,
        },
        None,
        None,
        0,
        &mut path,
    )
    .map_err(|e| format!("endpoint: {e:?}"))?;
    let mut net = NetConfig {
        endpoints: vec![Endpoint {
            host: a.host.clone(),
            sni: a.host.clone(),
            port: 443,
            path: String::from_utf8_lossy(path.path_query.as_slice()).into_owned(),
            ips: Vec::new(),
            upgrade_headers: Vec::new(),
        }],
        src_ips: vec![a.src_ip],
        bind_device: vec![(a.src_ip, a.bind_device.clone())],
        ts_expect_hw: false,
        ..NetConfig::default()
    };
    if a.so_rcvbuf > 0 {
        net.so_rcvbuf = a.so_rcvbuf;
    }
    let arena = ConnArena::new(
        &[
            (ConnRole::Md, 32),
            (ConnRole::Order, 1),
            (ConnRole::UserStream, 1),
            (ConnRole::Rest, 1),
        ],
        &net,
        clock,
    );
    let mut connector = Connector::new(arena, &net).map_err(|e| format!("connector: {e:?}"))?;
    let ring = |e: hft_rt::RtErr| format!("ring: {e:?}");
    let (b_tx, b_rx) = spsc::<BuildReq>(64, Backing::Heap).map_err(ring)?;
    let (h_tx, h_rx) = spsc::<Handover>(64, Backing::Heap).map_err(ring)?;
    let (r_tx, r_rx) = spsc::<RetireMsg>(64, Backing::Heap).map_err(ring)?;
    let (md_tx, mut md_rx) = spsc::<FeedMsg>(1 << 16, Backing::Heap).map_err(ring)?;
    let (bulk_tx, mut bulk_rx) = byte_spsc(64 << 20, Backing::Heap).map_err(ring)?;
    let (mut cmd_tx, cmd_rx) = spsc::<FeedCmd>(1024, Backing::Heap).map_err(ring)?;
    let raw = raw_journal(&a.dir)?;
    let venue = VenueCfg {
        env: VenueEnv::Prod,
        stale_guard_ms: 1500,
    };
    let cfg = FeedCfg {
        routes: FeedRoutes::Md,
        max_sub_insts: binds.len() as u32,
        depth: DepthMode::TopN,
        topn_levels: 100,
        snapshot_part_levels: 256,
        depth_conns: a.depth_conns,
        raw_bbo: RawMode::Off,
        raw_md: RawMode::Off,
        venue,
        endpoints: FeedEndpoints {
            bbo: EP,
            depth: EP,
            md: EP,
            src_ip_idx: 0,
            ..FeedEndpoints::default()
        },
        ..FeedCfg::default()
    };
    let mut feed = FeedRuntime::<Tap>::new(
        cfg,
        FeedDeps {
            rd,
            bbo: None,
            md: Some(md_tx),
            bulk: Some(bulk_tx),
            cmds: cmd_rx,
            snapreq: None,
            snap: None,
            ready: None,
            raw,
            handoff: h_rx,
            retire: r_tx,
            build: b_tx,
            clock: utc,
        },
        ctx,
    )
    .map_err(|e| format!("FeedRuntime: {e:?}"))?;
    for k in 0..binds.len() {
        cmd_tx
            .try_push(FeedCmd::Subscribe {
                inst: InstId(k as u32 + 1),
                streams: streams::BOOK | streams::TRADE,
            })
            .map_err(|_| "feed command ring full".to_string())?;
    }

    let stop = Arc::new(AtomicBool::new(false));
    // Conn-builder (warm): builds the feed's connections and hands them over.
    let s = Arc::clone(&stop);
    let warm = std::thread::Builder::new()
        .name("warm".into())
        .spawn(move || {
            let mut d = SessionDriver::<Tap>::new(
                SessionCfg {
                    venue,
                    ready_bbo: None,
                    ready_md: None,
                },
                Vec::new(),
            );
            let (mut b_rx, mut h_tx, mut r_rx) = (b_rx, h_tx, r_rx);
            let mut failed = 0u64;
            while !s.load(Ordering::Acquire) {
                let now = utc_ns();
                let mut did = false;
                while let Some(req) = b_rx.try_pop() {
                    did = true;
                    if d.start(&mut connector, req, None, now).is_err() {
                        let _ = h_tx.try_push(Handover::Failed(req.conn));
                    }
                }
                while let Some(m) = r_rx.try_pop() {
                    did = true;
                    d.retire(&mut connector, m);
                }
                for _ in 0..16 {
                    let Some(ev) = d.drive(&mut connector, now) else {
                        break;
                    };
                    did = true;
                    let h = match ev {
                        SessEvent::Ready(h) => Handover::Ready(h),
                        SessEvent::Failed { conn, code } => {
                            failed += 1;
                            eprintln!("pm-md-probe: connection {} failed: code {code:#x}", conn.0);
                            Handover::Failed(conn)
                        }
                    };
                    let _ = h_tx.try_push(h);
                }
                if !did {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            failed
        })
        .map_err(|e| format!("spawn warm: {e}"))?;
    // Feed task.
    let s = Arc::clone(&stop);
    let feed_t = std::thread::Builder::new()
        .name("feed".into())
        .spawn(move || {
            let mut idle = 0u32;
            while !s.load(Ordering::Acquire) {
                if feed.poll_once(hft_rt::tsc_now().0).did_work {
                    idle = 0;
                } else {
                    idle += 1;
                    if idle > 1000 {
                        std::thread::sleep(std::time::Duration::from_micros(100));
                    } else {
                        hft_rt::cpu_relax();
                    }
                }
            }
            let n = feed.counters();
            let (dups, lates) = (feed.ctx().dups(), feed.ctx().lates());
            feed.retire_all(hft_rt::tsc_now().0);
            format!("{n:?}, decoder dups {dups}, lates {lates}")
        })
        .map_err(|e| format!("spawn feed: {e}"))?;

    // Consumer (this thread): books and output lines until the deadline.
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(&a.out).map_err(|e| format!("{}: {e}", a.out.display()))?,
    );
    let mut books: Vec<Book> = (0..=binds.len()).map(|_| Book::default()).collect();
    let deadline = hft_rt::tsc_now().0 + a.seconds * ticks_per_s;
    let (mut n_snap, mut n_batch, mut n_trade, mut n_state, mut bad) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let slot = |inst: InstId| {
        binds
            .get((inst.0 as usize).wrapping_sub(1))
            .map_or("?", |b| b.slot.as_str())
    };
    while hft_rt::tsc_now().0 < deadline {
        let Some(m) = md_rx.try_pop() else {
            std::thread::sleep(std::time::Duration::from_micros(200));
            continue;
        };
        match m {
            FeedMsg::Bulk { kind, len, .. } => {
                let Some(body) = bulk_rx.peek().map(<[u8]>::to_vec) else {
                    bad += 1;
                    continue;
                };
                bulk_rx.advance();
                if body.len() != len as usize {
                    bad += 1;
                }
                if kind == EventKind::L2_SNAPSHOT {
                    let Ok((h, rest)) = L2SnapHdr::read_from_prefix(&body) else {
                        bad += 1;
                        continue;
                    };
                    let (nb, na) = (usize::from(h.n_bids), usize::from(h.n_asks));
                    let lv = levels(rest, nb + na);
                    let Some(b) = books.get_mut(h.h.inst.0 as usize) else {
                        continue;
                    };
                    // A snapshot cut into parts: the first part of an id replaces the book.
                    b.bids.clear();
                    b.asks.clear();
                    for (k, l) in lv.iter().enumerate() {
                        let side = if k < nb { &mut b.bids } else { &mut b.asks };
                        side.insert(l.px.0, l.qty.0);
                    }
                    n_snap += 1;
                    let (bb, ba) = b.best();
                    let _ = writeln!(
                        out,
                        "{{\"k\":\"snap\",\"slot\":\"{}\",\"ex_ms\":{},\"rx_ns\":{},\"id\":{},\"bb\":{},\"ba\":{}}}",
                        slot(h.h.inst),
                        h.h.ex_ts_ns / 1_000_000,
                        h.h.sw_rx_ns,
                        h.snap_id,
                        bb.map_or(-1, |x| x),
                        ba.map_or(-1, |x| x)
                    );
                } else if kind == EventKind::L2_BATCH {
                    let Ok((h, rest)) = L2BatchHdr::read_from_prefix(&body) else {
                        bad += 1;
                        continue;
                    };
                    let (nb, na) = (usize::from(h.n_bids), usize::from(h.n_asks));
                    let lv = levels(rest, nb + na);
                    let Some(b) = books.get_mut(h.h.inst.0 as usize) else {
                        continue;
                    };
                    for (k, l) in lv.iter().enumerate() {
                        let side = if k < nb { &mut b.bids } else { &mut b.asks };
                        if l.qty.0 == 0 {
                            side.remove(&l.px.0);
                        } else {
                            side.insert(l.px.0, l.qty.0);
                        }
                    }
                    n_batch += 1;
                    let (bb, ba) = b.best();
                    let _ = writeln!(
                        out,
                        "{{\"k\":\"batch\",\"slot\":\"{}\",\"ex_ms\":{},\"rx_ns\":{},\"prev\":{},\"last\":{},\"bb\":{},\"ba\":{}}}",
                        slot(h.h.inst),
                        h.h.ex_ts_ns / 1_000_000,
                        h.h.sw_rx_ns,
                        h.prev_id,
                        h.last_id,
                        bb.map_or(-1, |x| x),
                        ba.map_or(-1, |x| x)
                    );
                }
            }
            FeedMsg::Trade(t, _) => {
                n_trade += 1;
                let _ = writeln!(
                    out,
                    "{{\"k\":\"trade\",\"slot\":\"{}\",\"ex_ms\":{},\"rx_ns\":{},\"px\":{},\"qty\":{},\"side\":\"{}\"}}",
                    slot(t.h.inst),
                    t.h.ex_ts_ns / 1_000_000,
                    t.h.sw_rx_ns,
                    t.px.0,
                    t.qty.0,
                    if t.taker_side == Side::Buy { "BUY" } else { "SELL" }
                );
            }
            FeedMsg::ConnState(c) => {
                n_state += 1;
                let _ = writeln!(out, "{{\"k\":\"conn\",\"state\":\"{c:?}\"}}");
            }
            _ => {}
        }
    }
    stop.store(true, Ordering::Release);
    let counters = feed_t.join().unwrap_or_default();
    let failed = warm.join().unwrap_or(0);
    let _ = out.flush();
    println!(
        "pm-md-probe: snapshots {n_snap}, batches {n_batch}, trades {n_trade}, conn states {n_state}, \
         malformed {bad}, failed connections {failed}"
    );
    println!("pm-md-probe: feed counters {counters}");
    Ok(())
}
