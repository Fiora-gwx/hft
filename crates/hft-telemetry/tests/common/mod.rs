//! Shared fixtures of the hft-telemetry tests (TESTS.md "约定": standard calibration, standard
//! sample), a record feeder for `Telemetry` and a seq-journal writer on the heap backend.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use hft_event::{
    Bbo, ClockStatus, EventKind, HostProfile, LatencySample, MarketHdr, OrderEvent, OrderEventKind,
    RecordFlags, RecordHeader, Timer,
};
use hft_journal::testing;
use hft_journal::{
    InstanceName, JournalConfig, JournalKind, JournalReader, JournalWriter, PageSize, PrepEnv,
    PreparedSegment, ReaderConfig, RetiredSegment, RollInfo, RollReason, SegmentPreparer, WriterEnv,
};
use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule,
    FeePair, FeeSpec, FundingSettleKind, InstKind, InstSpec, InstStatus, LiqMarker, MarginData, MarginMode,
    PositionMode, RdbBuilder, RefData, SimBalanceSpec, SimSpec, StaleGuardKind, TestnetKind, TsUnit,
    VenueCaps, VenueSpec,
};
use hft_rt::{Backing, CalibPair, Consumer, Producer, spsc};
use hft_telemetry::{ClockCal, ClockStatusRow, StrategyCatalog, Telemetry, TelemetryConfig};
use hft_types::{
    AccountId, AssetExp, AssetId, ConnId, InstId, MarketKind, Notional, Px, Qty, Rate, SlotId, StrategyId,
    Tsc, VenueId,
};
use zerocopy::FromZeros;

/// T0 (tsc) and P0 (ns) of the standard calibration.
pub const T0: u64 = 1_000_000_000;
pub const P0: i64 = 5_000_000_000_000;
/// 1 GHz in milli-Hz.
pub const HZ1G: u64 = 1_000_000_000_000;

pub const VENUE: VenueId = VenueId::BINANCE_SPOT;
/// Accounts of `rd()`: 1 Live, 2 Sim.
pub const LIVE: AccountId = AccountId(1);
pub const SIM: AccountId = AccountId(2);
/// Inst 1 of `rd()`.
pub const INST: InstId = InstId(1);

/// Calibration row `(tsc, ns)` with `calib_err_ns = 100`, `max_error_ns = 1000`, Synced.
pub fn row(tsc: u64, ns: i64) -> ClockStatusRow {
    ClockStatusRow {
        tsc,
        realtime_ns: ns,
        phc_ns: ns,
        tsc_hz_milli: HZ1G,
        max_error_ns: 1_000,
        calib_err_ns: 100,
        state: 1,
    }
}

/// Standard calibration a, b.
pub fn std_a() -> ClockStatusRow {
    row(T0, P0)
}
pub fn std_b() -> ClockStatusRow {
    row(2 * T0, P0 + 1_000_000_000)
}

/// Standard calibration as a `ClockCal` (`cal_max_err_ns = 1000`).
pub fn std_cal() -> ClockCal {
    ClockCal::new(None, std_a(), Some(std_b()), 1_000)
}

/// `ClockStatus` body of a row.
pub fn cs(r: &ClockStatusRow) -> ClockStatus {
    ClockStatus {
        tsc: Tsc(r.tsc),
        realtime_ns: r.realtime_ns,
        phc_ns: r.phc_ns,
        tsc_hz_milli: r.tsc_hz_milli,
        chrony_offset_ns: 0,
        max_error_ns: r.max_error_ns,
        rms_ns: 0,
        calib_err_ns: r.calib_err_ns,
        source: 1,
        state: r.state,
        _pad: 0,
    }
}

/// `src` with every point present (0, 1, 2, 7 HW; 3–6 TSC).
pub const SRC_ALL: u32 = 1 | (1 << 2) | (1 << 4) | (3 << 6) | (3 << 8) | (3 << 10) | (3 << 12) | (1 << 14);

/// Sets the 2-bit source of point `pt`.
pub fn set_src(s: &mut LatencySample, pt: u32, code: u32) {
    s.src = (s.src & !(3 << (2 * pt))) | (code << (2 * pt));
}

/// The standard sample (market-triggered, `Bbo`, `action = New`, `link_role = 0`).
pub fn std_sample() -> LatencySample {
    let mut s = LatencySample::new_zeroed();
    s.venue = VENUE;
    s.account = LIVE;
    s.strategy = StrategyId(1);
    s.slot = SlotId(1);
    s.src = SRC_ALL;
    s.ex_ts_ns = P0 - 2_000_000;
    s.ex_send_ns = P0 - 1_000_000;
    s.hw_rx_ns = P0 + 400_000;
    s.sw_rx_ns = P0 + 405_000;
    s.rx_tsc = Tsc(T0 + 412_000);
    s.proc_tsc = Tsc(T0 + 415_000);
    s.decided_tsc = Tsc(T0 + 420_000);
    s.sent_tsc = Tsc(T0 + 428_000);
    s.ack_hw_rx_ns = P0 + 1_428_000;
    s.trigger_kind = EventKind::BBO.0;
    s
}

/// A `MarketHdr` on `VENUE`/conn 0/`INST` with the given receive points.
pub fn mhdr(hw: i64, sw: i64, rx_tsc: u64, gap: u32) -> MarketHdr {
    let mut h = MarketHdr::new_zeroed();
    h.venue = VENUE;
    h.conn = ConnId(0);
    h.inst = INST;
    h.ex_ts_ns = P0 - 2_000_000;
    h.ex_send_ns = P0 - 1_000_000;
    h.hw_rx_ns = hw;
    h.rx_tsc = rx_tsc;
    h.sw_rx_ns = sw;
    h.rx_empty_gap_ns = gap;
    h
}

pub fn bbo(h: MarketHdr) -> Bbo {
    let mut b = Bbo::new_zeroed();
    b.h = h;
    b.bid_px = Px(100);
    b.ask_px = Px(101);
    b.bid_qty = Qty(1);
    b.ask_qty = Qty(1);
    b
}

/// An `OrderEvent` report frame.
pub fn order_event(what: OrderEventKind) -> OrderEvent {
    OrderEvent {
        venue: VENUE,
        account: LIVE,
        what,
        detail: 0,
        slot: SlotId(1),
        r#gen: 1,
        inst: INST,
        px: Px(100),
        qty: Qty(1),
        cum_filled: Qty(0),
        fee: Qty(0),
        fee_asset: AssetId(0),
        epoch: 0,
        reject_code: 0,
        exch_order_id: 1,
        ex_ts_ns: P0 - 2_000_000,
        hw_rx_ns: P0 + 400_000,
        rx_tsc: T0 + 412_000,
        avg_px: Px(0),
        cash_qty: hft_types::Qty(0),
        cash_sum: hft_types::Qty(0),
    }
}

pub fn timer() -> Timer {
    Timer {
        timer_id: 1,
        owner: 1,
        _pad: 0,
        group: hft_types::GroupId(0),
        _pad2: 0,
        deadline_tsc: 0,
    }
}

/// Test refdata: Binance spot insts 1..=3 (−8/−8); account 1 Live, 2 Sim.
pub fn rd() -> RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    for (id, name, class) in [(1u16, "BTC", AssetClass::Crypto), (2, "USDT", AssetClass::Stable)] {
        b.asset(AssetSpec {
            id: AssetId(id),
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: -8,
                qty_exp: -8,
            },
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1_000),
        });
    }
    b.venue(VenueSpec {
        id: VENUE,
        exchange: Exchange::Binance,
        product: InstKind::Spot,
        ws_max_streams_per_conn: 1024,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 36,
        client_id_prefix: "".into(),
        max_batch: 5,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    });
    b.caps(VenueCaps {
        venue: VENUE,
        market: MarketKind::Spot,
        fee_asset_rule: FeeAssetRule::ReceivedAsset,
        reduce_only: false,
        required_position_mode: PositionMode::NotApplicable,
        required_margin_mode: MarginMode::NotApplicable,
        required_multi_asset: None,
        required_spot_borrow: None,
        funding_settle: FundingSettleKind::NotApplicable,
        margin_data: MarginData::NotApplicable,
        own_liquidation_marker: LiqMarker::NotIdentifiable,
        liq_marker_arg: "".into(),
        ignorable_user_events: Vec::new().into(),
        flags: CapFlags::POST_ONLY | CapFlags::IOC,
        amend_qty_down_only: false,
        dms: DeadManCap::None,
        dms_min_ms: 0,
        dms_max_ms: 0,
        dms_timeout_ms: 0,
        dms_refresh_ms: 0,
        testnet: TestnetKind::Testnet,
        _pad: 0,
    });
    for id in 1..=3u32 {
        b.inst(InstSpec {
            id: InstId(id),
            venue: VENUE,
            symbol: format!("SYM{id}USDT"),
            status: InstStatus::Trading,
            kind: InstKind::Spot,
            asset: AssetId(1),
            quote: AssetId(2),
            tick: 1,
            lot: 1,
            min_qty: Qty(1_000),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: 64,
            l2_depth: 20,
            crossed_tolerance: 0,
            tick_str: "0.00000001".into(),
            lot_str: "0.00000001".into(),
            min_qty_str: "0.00001".into(),
            ..InstSpec::default()
        });
    }
    b.fee(FeeSpec {
        venue: VENUE,
        tier: 0,
        fee: FeePair {
            maker: Rate(200_000),
            taker: Rate(500_000),
        },
    });
    b.account(AccountSpec {
        id: LIVE,
        label: "live".into(),
        venues: 1 << VENUE.0,
        ..AccountSpec::default()
    });
    b.account(AccountSpec {
        id: SIM,
        sim: true,
        label: "sim".into(),
        venues: 1 << VENUE.0,
        ..AccountSpec::default()
    });
    b.account_venue(AccountVenueSpec {
        account: LIVE,
        venue: VENUE,
        uid: "1001".into(),
    });
    b.sim(SimSpec {
        account: SIM,
        fee_tier: 0,
        latency_model: 0,
        margin_model: 0,
    });
    b.sim_balance(SimBalanceSpec {
        account: SIM,
        venue: VENUE,
        asset: AssetId(2),
        qty: Qty(1_000_000_000_000),
    });
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => r,
        Err(e) => panic!("test refdata rejected: {e:?}"),
    }
}

/// `TelemetryConfig` of the tests: defaults, `max_series = 1000`, `cal_max_err_ns = 1000`.
pub fn cfg() -> TelemetryConfig {
    TelemetryConfig {
        max_series: 1_000,
        cal_max_err_ns: 1_000,
        ..TelemetryConfig::default()
    }
}

/// Feeds records to a `Telemetry` with consecutive seqs.
pub struct Feed {
    pub tel: Telemetry,
    pub seq: u64,
}

impl Feed {
    pub fn new(c: &TelemetryConfig) -> Feed {
        Feed {
            tel: Telemetry::new(c, &rd(), &StrategyCatalog::empty()),
            seq: 0,
        }
    }

    /// Feeds one record with header `tsc` and `flags`; returns its seq.
    pub fn rec<B: hft_event::Body>(&mut self, tsc: u64, flags: u16, b: &B) -> u64 {
        self.seq += 1;
        let body = b.as_bytes();
        let h = RecordHeader {
            len: hft_event::record_len(body.len(), false),
            kind: B::KIND.0,
            flags,
            seq: self.seq,
            tsc,
        };
        self.tel.ingest(&h, body);
        self.seq
    }

    pub fn clock(&mut self, r: &ClockStatusRow) -> u64 {
        self.rec(r.tsc, 0, &cs(r))
    }

    pub fn sample(&mut self, s: &LatencySample) -> u64 {
        self.rec(s.sent_tsc.0 + 1_000, 0, s)
    }

    /// Standard calibration a, a `Bbo` trigger, the sample (trigger fields filled), b.
    pub fn std_with(&mut self, mut s: LatencySample) {
        self.clock(&std_a());
        let t = self.rec(
            s.proc_tsc.0,
            RecordFlags::HW_TS.0,
            &bbo(mhdr(s.hw_rx_ns, s.sw_rx_ns, s.rx_tsc.0, 0)),
        );
        s.trigger_seq = t;
        self.sample(&s);
        self.clock(&std_b());
    }
}

static N: AtomicU32 = AtomicU32::new(0);

/// HostProfile with `hash` as `profile_sha256` and `release`.
pub fn host_profile(hash: [u8; 32], release: &str) -> HostProfile {
    let mut h = HostProfile::new_zeroed();
    h.profile_sha256 = hash;
    h.tsc_hz_milli = HZ1G;
    let r = release.as_bytes();
    h.release[..r.len()].copy_from_slice(r);
    h
}

/// A seq journal on the heap backend written in the tests.
pub struct Jw {
    pub cfg: JournalConfig,
    pub w: JournalWriter,
    pub prep: SegmentPreparer,
    _retired: Consumer<RetiredSegment>,
}

fn rings() -> (
    Producer<PreparedSegment>,
    Consumer<PreparedSegment>,
    Producer<RetiredSegment>,
    Consumer<RetiredSegment>,
) {
    let (ptx, prx) = spsc(4, Backing::Heap).unwrap();
    let (rtx, rrx) = spsc(256, Backing::Heap).unwrap();
    (ptx, prx, rtx, rrx)
}

impl Jw {
    /// Opens a fresh journal (segment size `seg_mib` MiB) and writes the first boot block with
    /// `HostProfile(hash)`.
    pub fn new(hash: [u8; 32], seg_mib: u64) -> Jw {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("hftt-{}-{n}", std::process::id()));
        let hot = root.join("hot");
        testing::mem_root(&hot);
        let cold = root.join("cold");
        std::fs::create_dir_all(&cold).unwrap();
        let cfg = JournalConfig {
            name: "seq".into(),
            kind: JournalKind::Seq,
            venue: VenueId(0),
            instance: InstanceName::Prod,
            hot_dir: hot,
            cold_root: cold,
            segment_bytes: seg_mib * 1_048_576,
            page: PageSize::Normal,
            size_idle_pct: 75,
            size_valve_pct: 90,
            hot_keep: 8,
        };
        let (ptx, prx, rtx, rrx) = rings();
        let prep = SegmentPreparer::new(
            &cfg,
            PrepEnv {
                writer_epoch: 1,
                build_git: [0; 20],
                host_profile_sha256: hash,
                to_writer: ptx,
            },
        );
        let (w, _) = JournalWriter::open(
            &cfg,
            WriterEnv {
                writer_epoch: 1,
                build_git: [0; 20],
                host_profile_sha256: hash,
                prepared: prx,
                retired: rtx,
            },
        )
        .unwrap();
        let mut j = Jw {
            cfg,
            w,
            prep,
            _retired: rrx,
        };
        j.roll(hash, "v0.1.0");
        j
    }

    /// Rolls to a new segment whose boot block carries `HostProfile(hash, release)`.
    pub fn roll(&mut self, hash: [u8; 32], release: &str) {
        self.prep.poll();
        let info = RollInfo {
            utc_ns: 1_790_812_800_000_000_000,
            calib: CalibPair {
                tsc: Tsc(1),
                realtime_ns: 0,
                phc_ns: 0,
                err_ns: 0,
            },
            tsc_hz_milli: HZ1G,
            configs: &[],
        };
        let mut b = self.w.roll(RollReason::Startup, &info).unwrap();
        b.append(Tsc(1), &host_profile(hash, release)).unwrap();
        b.finish().unwrap();
        self.w.publish();
    }

    /// Appends one Input event record; returns its seq.
    pub fn input<B: hft_event::Body>(&mut self, tsc: u64, flags: u16, b: &B) -> u64 {
        let s = self.w.append(RecordFlags(flags), Tsc(tsc), b).unwrap().0;
        self.w.end_event();
        self.w.publish();
        s
    }

    /// Appends one Meta record (ClockStatus, LatencySample, HostProfile); returns its seq.
    pub fn meta<B: hft_event::Body>(&mut self, tsc: u64, b: &B) -> u64 {
        let s = self.w.append_meta(Tsc(tsc), b).unwrap().0;
        self.w.publish();
        s
    }

    pub fn clock(&mut self, r: &ClockStatusRow) -> u64 {
        self.meta(r.tsc, &cs(r))
    }

    /// Appends a multi-part/var record from pieces (L2Snapshot); returns its seq.
    pub fn var(&mut self, kind: EventKind, tsc: u64, flags: u16, pieces: &[&[u8]]) -> u64 {
        let s = self
            .w
            .append_var(kind, RecordFlags(flags), Tsc(tsc), pieces)
            .unwrap()
            .0;
        self.w.end_event();
        self.w.publish();
        s
    }

    /// Writes Timer events until the next record gets `seq`.
    pub fn pad_to(&mut self, seq: u64) {
        while self.w.next_seq().0 < seq {
            self.input(1, 0, &timer());
        }
    }

    /// A reader from the oldest record that stops at the current end.
    pub fn reader(&self) -> JournalReader {
        let mut c = ReaderConfig::new(
            self.cfg.hot_dir.clone(),
            self.cfg.cold_root.clone(),
            self.cfg.instance,
            self.cfg.name.clone(),
        );
        c.tail_poll_us = 0;
        JournalReader::open(&c).unwrap()
    }

    /// Hot segment files of the journal.
    pub fn files(&self) -> Vec<PathBuf> {
        testing::hot_files(&self.cfg.hot_journal_dir())
    }
}

/// `EventWeightedCfg` of the tests: whole journal, threshold 1000 ns, `burst_gap_ns = 50 000`.
pub fn ew_cfg() -> hft_telemetry::EventWeightedCfg {
    hft_telemetry::EventWeightedCfg {
        from_seq: 0,
        to_seq: u64::MAX,
        cal_max_err_ns: 1_000,
        burst_gap_ns: 50_000,
        max_series: 1_000,
    }
}

/// Probe journal of TEL-036–TEL-038 under the standard calibration: `calm` calm-triggered probe
/// samples with `tick_to_trade` 1 000, 2 000, … ns, `burst` burst-triggered ones with 10 001 000,
/// 10 002 000, … ns (each after its own `Bbo` trigger; `kernel_rx = 0`), and `extra_calm` calm
/// `Bbo` records without samples. Calm records have `rx_empty_gap_ns = 10`, burst ones 60 000.
pub fn probe_journal(calm: usize, burst: usize, extra_calm: usize) -> Jw {
    let mut j = Jw::new([7; 32], 32);
    j.clock(&std_a());
    let n = calm + burst + extra_calm;
    let step = 750_000_000 / n.max(1) as u64;
    for i in 0..n {
        let rx = T0 + 1_000 + i as u64 * step;
        let hw = P0 + (rx - T0) as i64;
        let (gap, v) = if i < calm {
            (10, Some(1_000 * (i as u64 + 1)))
        } else if i < calm + burst {
            (60_000, Some(10_001_000 + 1_000 * (i - calm) as u64))
        } else {
            (10, None)
        };
        let trig = j.input(rx + 500, RecordFlags::HW_TS.0, &bbo(mhdr(hw, hw + 100, rx, gap)));
        if let Some(v) = v {
            let mut s = std_sample();
            s.trigger_seq = trig;
            s.hw_rx_ns = hw;
            s.sw_rx_ns = hw + 100;
            s.rx_tsc = Tsc(rx);
            s.proc_tsc = Tsc(rx + 500);
            s.decided_tsc = Tsc(rx + 600);
            s.sent_tsc = Tsc(rx + v);
            s.ack_hw_rx_ns = hw + v as i64 + 1_000_000;
            j.meta(rx + v + 1_000, &s);
        }
    }
    j.clock(&std_b());
    j
}
