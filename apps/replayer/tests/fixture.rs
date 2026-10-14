use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hft_event::{
    AccountConfigReportHdr, Bbo, BlobRef, ClockStatus, CmdHdr, ConnRole, ConnState, ConnStateKind,
    ControlCmd, EventKind, FeedCmd, GwCmd, HostProfile, LeverageItem, MarketHdr, ReconcileReport,
    RecordFlags, Timer,
};
use hft_journal::{
    EndAt, InstanceName, JournalConfig, JournalKind, JournalReader, JournalWriter, PageSize, ReaderConfig,
};
use hft_rt::{CalibPair, Heartbeat};
use hft_sequencer::driver::RollInfo;
use hft_sequencer::driver::live::{LiveCfg, LiveDriver, LiveIo, RingId};
use hft_sequencer::driver::replay::Log;
use hft_sequencer::driver::replay_io::LogReadLimits;
use hft_sequencer::journal::{JournalSink, RollMeta};
use hft_sequencer::{
    BootSource, InputSlot, MemJournal, OutRing, ParsedConfig, RecoverPlan, RecoveryFlags, Sequencer,
    SnapshotSel, StepOutcome,
};
use hft_strategy_sdk::StrategySet;
use hft_testkit::dst::seq::build;
use hft_testkit::dst::world::WorldSpec;
use hft_testkit::seq_rig::RigIo;
use hft_types::{AccountId, GroupId, InstId, KillBits, MarketKind, Px, Qty, Rate, Seq, Side, Tsc, VenueId};
use sha2::{Digest, Sha256};
use zerocopy::{FromBytes, FromZeros, IntoBytes};

pub mod dst;
mod model;
mod model_boundary;
mod path_model;
mod receipt;
mod restart_model;
mod spot_notional;

static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Fixture {
    pub root: PathBuf,
    pub file: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// Faults are applied before the real writer assigns seq/CRC/seals. Live recovery consumes
// those actual published bytes and creates its own RecoverMismatch Notes.
struct FaultWriter {
    inner: JournalWriter,
    fault: Option<EventKind>,
    changed: Option<Seq>,
    last_result: Option<hft_event::CmdResult>,
}
impl std::ops::Deref for FaultWriter {
    type Target = JournalWriter;
    fn deref(&self) -> &JournalWriter {
        &self.inner
    }
}
impl std::ops::DerefMut for FaultWriter {
    fn deref_mut(&mut self) -> &mut JournalWriter {
        &mut self.inner
    }
}
impl JournalSink for FaultWriter {
    type Boot<'a> = <JournalWriter as JournalSink>::Boot<'a>;
    fn append_var(
        &mut self,
        kind: EventKind,
        flags: RecordFlags,
        tsc: Tsc,
        pieces: &[&[u8]],
    ) -> Result<Seq, hft_journal::AppendErr> {
        if kind == EventKind::CMD_RESULT {
            let bytes: Vec<u8> = pieces.iter().flat_map(|p| p.iter().copied()).collect();
            self.last_result = hft_sequencer::types::read_body(&bytes);
        }
        if self.fault == Some(kind) {
            self.fault = None;
            let mut bytes: Vec<u8> = pieces.iter().flat_map(|p| p.iter().copied()).collect();
            let offset = if kind == EventKind::ORDER_DECISION { 24 } else { 0 };
            bytes[offset] ^= 1;
            let seq = JournalSink::append_var(&mut self.inner, kind, flags, tsc, &[&bytes])?;
            self.changed = Some(seq);
            Ok(seq)
        } else {
            JournalSink::append_var(&mut self.inner, kind, flags, tsc, pieces)
        }
    }
    fn end_event(&mut self) {
        JournalSink::end_event(&mut self.inner);
    }
    fn publish(&mut self) {
        self.inner.publish();
    }
    fn roll(
        &mut self,
        reason: hft_journal::RollReason,
        info: &RollMeta<'_>,
    ) -> Result<Self::Boot<'_>, hft_journal::RollErr> {
        JournalSink::roll(&mut self.inner, reason, info)
    }
    fn needs_roll(&self, now: Tsc) -> Option<(hft_journal::RollReason, hft_journal::RollAt)> {
        JournalSink::needs_roll(&self.inner, now)
    }
}

struct Io<S: StrategySet> {
    rings: RigIo<S>,
    writer: FaultWriter,
    old: Option<Log>,
    build: [u8; 20],
}
impl<S: StrategySet> LiveIo<S> for Io<S> {
    type J = FaultWriter;
    fn journal(&mut self) -> &mut Self::J {
        &mut self.writer
    }
    fn tsc(&mut self) -> Tsc {
        self.rings.tsc()
    }
    fn n_rep(&self) -> u8 {
        self.rings.n_rep()
    }
    fn n_bbo(&self) -> u8 {
        self.rings.n_bbo()
    }
    fn n_md(&self) -> u8 {
        self.rings.n_md()
    }
    fn len(&mut self, r: RingId) -> usize {
        self.rings.len(r)
    }
    fn peek_len(&mut self, r: RingId) -> Option<usize> {
        self.rings.peek_len(r)
    }
    fn peek_inst(&mut self, r: RingId) -> Option<InstId> {
        self.rings.peek_inst(r)
    }
    fn pop(&mut self, r: RingId, out: &mut InputSlot) -> bool {
        self.rings.pop(r, out)
    }
    fn push_out(&mut self, r: OutRing, c: &GwCmd) -> bool {
        self.rings.push_out(r, c)
    }
    fn push_feed(&mut self, v: VenueId, c: &FeedCmd) -> bool {
        self.rings.push_feed(v, c)
    }
    fn take_parsed(&mut self, r: &BlobRef) -> Option<ParsedConfig<S>> {
        self.rings.take_parsed(r)
    }
    fn retire(&mut self, p: ParsedConfig<S>) {
        self.rings.retire(p);
    }
    fn write_kill(&mut self, bits: &[KillBits]) {
        self.rings.write_kill(bits);
    }
    fn roll_info(&mut self) -> RollInfo {
        self.rings.roll_info()
    }
    fn host_profile(&self) -> HostProfile {
        let mut h = self.rings.host_profile();
        h.build_git = self.build;
        h
    }
    fn recovery_log(&mut self) -> Option<Log> {
        self.old.take()
    }
    fn clock_status(&self) -> ClockStatus {
        self.rings.clock_status()
    }
    fn utc_of(&self, t: Tsc) -> i64 {
        self.rings.utc_of(t)
    }
}

fn settle<S: StrategySet>(seq: &mut Sequencer<LiveDriver<S, Io<S>>, S>) {
    for _ in 0..1000 {
        match seq.step() {
            StepOutcome::Idle if !seq.view().snapshot_requested() => return,
            // SnapshotNow is emitted only after a previous idle round. The first Idle
            // after the command is not evidence that the requested snapshot was written.
            StepOutcome::Busy | StepOutcome::Idle => {}
            o => panic!("{o:?}"),
        }
    }
    panic!("fixture did not settle");
}

pub fn create<S: StrategySet>() -> Fixture {
    create_with_restart::<S>(None, false, false, None, None, None)
}

pub fn original_receipt(positive: bool) -> Fixture {
    finish_evidence(create_with_receipt::<hft_strategy_registry::Strategies>(
        None,
        false,
        false,
        None,
        None,
        None,
        Some(ReceiptCase::Original(positive)),
    ))
}

pub fn spot_notional(side: Side) -> Fixture {
    finish_evidence(create_with_receipt::<hft_strategy_registry::Strategies>(
        None,
        false,
        false,
        None,
        None,
        None,
        Some(ReceiptCase::SpotNotional(side)),
    ))
}

pub fn with_model() -> Fixture {
    create_with_restart::<hft_testkit::TestStrategies>(None, false, true, None, None, None)
}

/// Full-duration, path-bearing Model files. Publication as frozen T1/T3 fixtures is separate.
/// M2 verify uses one hour; Model/path acceptance uses the `heavy` tests.
#[cfg_attr(not(feature = "long-fixtures"), allow(dead_code))]
pub fn path_model(hours: u64) -> Fixture {
    assert!(matches!(hours, 1 | 3));
    let f = create_with_restart::<hft_testkit::strategies::ReplayStrategies>(
        None,
        false,
        false,
        Some(hours),
        None,
        None,
    );
    finish_evidence(f)
}

#[derive(Clone, Copy)]
pub struct ModelRestart {
    pub count: u8,
    pub settle_ms: u64,
    pub reason: u64,
    pub recovery_gap: bool,
}

pub fn model_restart(spec: ModelRestart) -> Fixture {
    let f = create_with_restart::<hft_testkit::strategies::ReplayStrategies>(
        None,
        false,
        false,
        Some(1),
        Some(spec),
        None,
    );
    finish_evidence(f)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelBoundary {
    Created,
    Warmup,
    Coalesced,
}

pub fn model_boundary(mode: ModelBoundary) -> Fixture {
    let f = if mode == ModelBoundary::Warmup {
        create_with_restart::<hft_testkit::strategies::ReplayWarmupStrategies>(
            None,
            false,
            false,
            Some(1),
            None,
            Some(mode),
        )
    } else {
        create_with_restart::<hft_testkit::strategies::ReplayStrategies>(
            None,
            false,
            false,
            Some(1),
            None,
            Some(mode),
        )
    };
    finish_evidence(f)
}

fn finish_evidence(f: Fixture) -> Fixture {
    for entry in std::fs::read_dir(f.file.parent().unwrap()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "jrn") {
            seal(&path);
        }
    }
    if let Some(root) = std::env::var_os("HFT_REPLAY_EVIDENCE_DIR") {
        let dest = PathBuf::from(root).join(f.root.file_name().unwrap());
        copy_evidence(&f.root, &dest);
    }
    f
}

// Copy only this fixture's generated files; no production journal or external source is read.
fn copy_evidence(src: &Path, dest: &Path) {
    std::fs::create_dir_all(dest).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dest.join(entry.file_name());
        let ty = entry.file_type().unwrap();
        assert!(!ty.is_symlink());
        if ty.is_dir() {
            copy_evidence(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).unwrap();
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Restart {
    Recovery,
    FromSnapshot,
    Version,
    Mismatch,
    GlobalMismatch,
    Gap(u64),
}

pub fn recovery_gap(skipped: u64) -> Fixture {
    finish_evidence(restarted::<hft_strategy_registry::Strategies>(Restart::Gap(
        skipped,
    )))
}

pub fn restarted<S: StrategySet>(restart: Restart) -> Fixture {
    create_with_restart::<S>(Some(restart), false, false, None, None, None)
}

/// RPL-025/026: genuine file-backed executions with 20 min before the restart.
pub fn long_restart<S: StrategySet>(restart: Restart) -> Fixture {
    assert!(matches!(restart, Restart::FromSnapshot | Restart::Version));
    let f = create_with_restart::<S>(Some(restart), true, false, None, None, None);
    // open_offline writes closed .jrn files, without running persist. Build the cold-copy
    // transport envelope from the actual header and End; leave every journal byte untouched.
    for entry in std::fs::read_dir(f.file.parent().unwrap()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "jrn") {
            seal(&path);
        }
    }
    f
}

pub fn seal(path: &Path) {
    let bytes = std::fs::read(path).unwrap();
    let (header, _) = hft_journal::SegmentHeaderFixed::read_from_prefix(&bytes).unwrap();
    assert_eq!(header.header_crc32c, header.compute_crc());
    let mut offset = hft_journal::HEADER_LEN;
    let mut last = None;
    let mut last_tsc = header.calib_tsc;
    while offset < bytes.len() {
        let r = hft_event::decode_record(&bytes[offset..]).unwrap();
        last_tsc = last_tsc.max(r.hdr.tsc);
        last = Some((
            r.hdr.seq,
            (r.hdr.kind == EventKind::SEGMENT_ROLL.0).then(|| r.read::<hft_event::SegmentRoll>().unwrap()),
        ));
        offset += r.hdr.len as usize;
    }
    assert_eq!(offset, bytes.len());
    let (last_seq, Some(end)) = last.unwrap() else {
        panic!("offline file does not end with SegmentRoll");
    };
    assert_eq!(end.mark, hft_journal::RollMark::End as u8);
    let mut hash = Sha256::new();
    hash.update(&bytes[..3072]);
    hash.update([0u8; 128]);
    hash.update(&bytes[3200..]);
    let utc = header.calib_realtime_ns
        + ((i128::from(last_tsc) - i128::from(header.calib_tsc)) * 1_000_000_000_000
            / i128::from(header.tsc_hz_milli)) as i64;
    let mut seal = hft_journal::SealRecord {
        magic: hft_journal::SEAL_MAGIC,
        segment_no: header.segment_no,
        first_seq: header.first_seq,
        last_seq,
        len: bytes.len() as u64,
        sealed_utc_ns: end.utc_ns.max(utc),
        sha256: hash.finalize().into(),
        reason: end.reason,
        flags: header.flags,
        _pad: [0; 2],
        _rsv: [0; 40],
        crc32c: 0,
    };
    seal.crc32c = seal.compute_crc();
    std::fs::write(path.with_extension("seal"), seal.as_bytes()).unwrap();
}

pub fn read(f: &Fixture) -> Log {
    read_root(&f.root)
}

fn read_root(root: &Path) -> Log {
    let mut c = ReaderConfig::new(
        root.to_path_buf(),
        root.join("absent"),
        InstanceName::Prod,
        "seq".into(),
    );
    c.end = EndAt::LastCompleteEvent;
    c.reassembly_cap = 16 << 20;
    let mut r = JournalReader::open(&c).unwrap();
    Log::from_reader(
        &mut r,
        LogReadLimits {
            records: 200_000,
            body_bytes: 256 << 20,
        },
    )
    .unwrap()
}

fn command<S: StrategySet>(s: &mut Sequencer<LiveDriver<S, Io<S>>, S>, id: u64, c: &ControlCmd<'_>) {
    let mut h = CmdHdr::zeroed();
    h.cmd_id[..8].copy_from_slice(&id.to_le_bytes());
    h.issuer[..7].copy_from_slice(b"cli:rpl");
    h.role = CmdHdr::ROLE_ADMIN;
    h.account = AccountId(0);
    h.cmd_ver = 1;
    h.confirmed = 1;
    h.expect_state_version = s.engine().risk.mode.ctl_state_version();
    let mut body = vec![0; 8192];
    let n = c.encode(&h, &mut body).unwrap();
    body.truncate(n);
    s.driver_mut()
        .io
        .rings
        .rings
        .entry((0, 0))
        .or_default()
        .push_back((EventKind::CONTROL_CMD, body));
}

fn input<S: StrategySet>(
    s: &mut Sequencer<LiveDriver<S, Io<S>>, S>,
    ring: (u8, u8),
    kind: EventKind,
    body: Vec<u8>,
) {
    s.driver_mut()
        .io
        .rings
        .rings
        .entry(ring)
        .or_default()
        .push_back((kind, body));
}

// SmokeOrder is an upgrade command: it needs the real startup gate, even when risk will
// reject the resulting order. Supply the same journal inputs as the DST live prologue.
fn prepare_smoke<S: StrategySet>(s: &mut Sequencer<LiveDriver<S, Io<S>>, S>, world: &WorldSpec) {
    let t = 1;
    s.driver_mut().io.rings.tsc = t;
    let account = world.accounts[0].id;
    for (vi, venue) in world.venues.iter().enumerate() {
        let base = ConnState {
            venue: venue.id,
            conn: 0,
            state: ConnStateKind::Up,
            role: ConnRole::Md,
            inst: InstId(0),
            reason: 0,
            account: AccountId(0),
            _pad2: [0; 3],
            rx_tsc: t,
        };
        input(s, (2, vi as u8), EventKind::CONN_STATE, base.as_bytes().to_vec());
        for i in world.insts.iter().filter(|i| i.venue == venue.id) {
            input(
                s,
                (2, vi as u8),
                EventKind::CONN_STATE,
                ConnState { inst: i.id, ..base }.as_bytes().to_vec(),
            );
        }
        for (conn, role) in [
            (0, ConnRole::Order),
            (1, ConnRole::Order),
            (0, ConnRole::UserStream),
        ] {
            input(
                s,
                (2, vi as u8),
                EventKind::CONN_STATE,
                ConnState {
                    conn,
                    role,
                    account,
                    ..base
                }
                .as_bytes()
                .to_vec(),
            );
        }
    }
    settle(s);
    for (vi, venue) in world.venues.iter().enumerate() {
        let perp = venue.id.market() == Some(MarketKind::LinearPerp);
        let levs: Vec<_> = world
            .insts
            .iter()
            .filter(|i| i.venue == venue.id && i.perp)
            .map(|i| LeverageItem {
                inst: i.id,
                _pad: 0,
                leverage: Rate(build::LEVERAGE_X * 1_000_000_000),
            })
            .collect();
        let mut h = AccountConfigReportHdr::new_zeroed();
        h.venue = venue.id;
        h.account = account;
        h.position_mode = if perp { 2 } else { 1 };
        h.margin_mode = h.position_mode;
        h.multi_asset = h.position_mode;
        h.source = 1;
        h.uid_keys = 3;
        h.n_lev = levs.len() as u16;
        h.rx_tsc = Tsc(t);
        let uid = build::uid_of(account, venue.id);
        h.venue_uid[..uid.len()].copy_from_slice(uid.as_bytes());
        let mut body = h.as_bytes().to_vec();
        for l in levs {
            body.extend_from_slice(l.as_bytes());
        }
        input(s, (2, vi as u8), EventKind::ACCOUNT_CONFIG_REPORT, body);
        let mut r = ReconcileReport::new_zeroed();
        r.report_id = 1000 + vi as u64;
        r.venue = venue.id;
        r.account = account;
        r.item = 5;
        r.count = 1;
        input(
            s,
            (2, vi as u8),
            EventKind::RECONCILE_REPORT,
            r.as_bytes().to_vec(),
        );
        for i in world.insts.iter().filter(|i| i.venue == venue.id) {
            let mut h = MarketHdr::new_zeroed();
            h.venue = venue.id;
            h.inst = i.id;
            h.rx_tsc = t;
            let b = Bbo {
                h,
                bid_px: Px(i.bid),
                bid_qty: Qty(i.depth),
                ask_px: Px(i.ask),
                ask_qty: Qty(i.depth),
                update_id: 1,
            };
            input(s, (4, vi as u8), EventKind::BBO, b.as_bytes().to_vec());
        }
    }
    for timer_id in [8, 16 | (u32::from(account.0) << 16)] {
        let timer = Timer {
            timer_id,
            owner: 1,
            _pad: 0,
            group: GroupId(0),
            _pad2: 0,
            deadline_tsc: t,
        };
        input(s, (1, 0), EventKind::TIMER, timer.as_bytes().to_vec());
    }
    settle(s);
    assert!(
        s.engine().risk.mode.released(account),
        "startup gate not released: bits={:#x}",
        s.engine().risk.mode.gate_bits(account)
    );
}

fn market_minutes<S: StrategySet>(
    s: &mut Sequencer<LiveDriver<S, Io<S>>, S>,
    world: &WorldSpec,
    minutes: u64,
    base_id: u64,
) {
    let first_tsc = s.driver().io.rings.tsc;
    let first_ens = s.engine().last_ens;
    let hz = s.driver().io.rings.hz;
    for seconds in (10..=minutes * 60).step_by(10) {
        let t = first_tsc + (u128::from(seconds) * u128::from(hz) / 1000) as u64;
        let utc = s.driver().io.utc_of(Tsc(t));
        s.driver_mut().io.rings.tsc = t;
        s.driver_mut().io.rings.calib.tsc = Tsc(t);
        s.driver_mut().io.rings.calib.realtime_ns = utc;
        let clock = s.driver().io.clock_status();
        input(s, (1, 0), EventKind::CLOCK_STATUS, clock.as_bytes().to_vec());
        for (vi, venue) in world.venues.iter().enumerate() {
            for i in world.insts.iter().filter(|i| i.venue == venue.id) {
                let mut h = MarketHdr::new_zeroed();
                h.venue = venue.id;
                h.inst = i.id;
                h.rx_tsc = t;
                h.ex_ts_ns = utc;
                let shift = (seconds / 10) as i64 * i.tick;
                let b = Bbo {
                    h,
                    bid_px: Px(i.bid + shift),
                    bid_qty: Qty(i.depth),
                    ask_px: Px(i.ask + shift),
                    ask_qty: Qty(i.depth),
                    update_id: base_id + seconds / 10,
                };
                input(s, (4, vi as u8), EventKind::BBO, b.as_bytes().to_vec());
            }
        }
        settle(s);
        // Exact 10/20 min anchors, independent of the automatic keyframe scheduler.
        if seconds.is_multiple_of(600) {
            command(s, base_id + seconds, &ControlCmd::SnapshotNow);
            settle(s);
        }
    }
    assert_eq!(
        s.engine().last_ens - first_ens,
        (minutes * 60 * 1_000_000_000) as i64
    );
}

fn create_with_restart<S: StrategySet>(
    restart: Option<Restart>,
    long: bool,
    model: bool,
    model_hours: Option<u64>,
    model_restarts: Option<ModelRestart>,
    boundary: Option<ModelBoundary>,
) -> Fixture {
    create_with_receipt::<S>(restart, long, model, model_hours, model_restarts, boundary, None)
}

#[derive(Copy, Clone)]
enum ReceiptCase {
    Original(bool),
    SpotNotional(Side),
}

fn create_with_receipt<S: StrategySet>(
    restart: Option<Restart>,
    long: bool,
    model: bool,
    model_hours: Option<u64>,
    model_restarts: Option<ModelRestart>,
    boundary: Option<ModelBoundary>,
    receipt: Option<ReceiptCase>,
) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "replayer-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let f = Fixture {
        file: root.join("prod/seq/0000000001.jrn"),
        root,
    };
    let mut world = WorldSpec::w1();
    if matches!(receipt, Some(ReceiptCase::SpotNotional(_))) {
        // A valid ten-millibitcoin smoke order allows two lot-aligned increments.
        world.insts[0].min_qty = 1_000_000;
    }
    if model_hours.is_some() {
        world.accounts.truncate(1);
        world.strategies.truncate(1);
        world.groups.retain(|g| g.account == world.accounts[0].id);
        for g in &mut world.groups {
            std::mem::swap(&mut g.first, &mut g.second);
        }
        world.risk.conn_liveness_ms = 30_000;
    }
    for binding in &mut world.strategies {
        binding.kind_id = if model {
            0xFF02
        } else {
            S::kinds().first().unwrap().kind_id
        };
    }
    if let Some(spec) = model_restarts {
        use hft_strategy_sdk::params::{RawParams, encode_body};
        world.strategies[0].instance = hft_types::StrategyId(9);
        world.groups[0].instance = world.strategies[0].instance;
        world.groups[0].group = hft_types::GroupId::compose(world.strategies[0].instance, 1).unwrap();
        world.strategies[0].params = encode_body(
            S::kinds()[0].schema,
            world.groups.len() as u16,
            &RawParams {
                defaults: vec![0, 1, 7, 5000, spec.settle_ms as i64],
                overrides: vec![],
            },
        )
        .unwrap();
    }
    // The refdata catalog has dense instance IDs. Earlier identities are reserved but
    // unbound; only the real world's instance receives parameters or owns an account.
    let mut catalog = world.clone();
    if model_restarts.is_some() {
        let active = world.strategies[0].clone();
        for id in 1..active.instance.0 {
            let mut reserved = active.clone();
            reserved.instance = hft_types::StrategyId(id);
            reserved.account = AccountId(0);
            reserved.params.clear();
            catalog.strategies.push(reserved);
        }
        catalog.strategies.sort_by_key(|s| s.instance.0);
    }
    let rd_blob = if model_hours.is_some() {
        build::refdata_blob_with_names(
            &catalog,
            false,
            &[(world.strategies[0].instance, "tsa")],
            &[(world.accounts[0].id, "acct-l")],
        )
        .unwrap()
    } else {
        build::refdata_blob(&world, false).unwrap()
    };
    let rd = build::load_refdata(&rd_blob).unwrap();
    if model_restarts.is_some() {
        assert_eq!(rd.instances_in_order().len(), 9);
        for id in 1..9 {
            assert!(rd.account_of(hft_types::StrategyId(id)).is_none());
        }
    }
    if model_hours.is_some() {
        assert_eq!(rd.account(world.accounts[0].id).label.as_ref(), "acct-l");
        assert_eq!(
            rd.account(world.accounts[0].id).bound_instance,
            world.strategies[0].instance
        );
        assert_eq!(
            rd.instance(world.strategies[0].instance).unwrap().name.as_ref(),
            "tsa"
        );
        assert!(
            build::refdata_blob_with_names(&world, false, &[(hft_types::StrategyId(255), "missing")], &[])
                .is_err()
        );
        assert!(build::refdata_blob_with_names(&world, false, &[], &[(AccountId(255), "missing")]).is_err());
        assert!(
            build::refdata_blob_with_names(
                &world,
                false,
                &[
                    (world.strategies[0].instance, "a"),
                    (world.strategies[0].instance, "b")
                ],
                &[]
            )
            .is_err()
        );
        assert!(
            build::refdata_blob_with_names(
                &world,
                false,
                &[],
                &[(world.accounts[0].id, "a"), (world.accounts[0].id, "b")]
            )
            .is_err()
        );
    }
    let mut setup = build::engine_setup(&world, 1_000_000_000, 1);
    if matches!(receipt, Some(ReceiptCase::Original(_))) {
        setup.ledger.ext_dedup_cap = 2;
    }
    if long || model_hours.is_some() {
        setup.seq.keyframe_period_ms = 300_000;
        setup.seq.keyframe_max_events = 1_500_000;
    }
    let mut configs = build::config_set::<S>(&world, &rd, rd_blob, &setup).unwrap();
    let pending = if boundary == Some(ModelBoundary::Created) {
        assert_eq!(configs.params.len(), 1);
        configs.params.pop()
    } else {
        None
    };
    let cfg = JournalConfig {
        name: "seq".into(),
        kind: JournalKind::Seq,
        venue: VenueId(0),
        instance: InstanceName::Prod,
        hot_dir: f.root.join("unused-hot"),
        cold_root: f.root.join("unused-cold"),
        segment_bytes: 32 << 20,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: 1,
    };
    let writer = JournalWriter::open_offline(&cfg, &f.root).unwrap();
    let io = Io {
        writer: FaultWriter {
            inner: writer,
            fault: None,
            changed: None,
            last_result: None,
        },
        old: None,
        build: [0; 20],
        rings: RigIo {
            j: MemJournal::new(setup.clock),
            tsc: 0,
            rings: BTreeMap::new(),
            n_venues: world.venues.len() as u8,
            out: Vec::new(),
            mailbox: Vec::new(),
            kills: Vec::new(),
            calib: CalibPair {
                tsc: Tsc(0),
                realtime_ns: setup.clock.calib_realtime_ns,
                phc_ns: 0,
                err_ns: 0,
            },
            hz: setup.clock.tsc_hz_milli,
        },
    };
    let mut s = Sequencer::boot(
        BootSource::Empty { configs },
        &setup,
        LiveDriver::new(io, LiveCfg::default()),
        Box::leak(Box::new(Heartbeat::new())),
    )
    .unwrap();
    settle(&mut s);
    if model {
        model::exercise(&mut s, &f, &world, &setup);
    }
    if let Some(case) = receipt {
        match case {
            ReceiptCase::Original(positive) => receipt::exercise(&mut s, &world, positive),
            ReceiptCase::SpotNotional(side) => spot_notional::exercise(&mut s, &world, side),
        }
    } else if let Some(mode) = boundary {
        model_boundary::exercise(&mut s, &f, &world, &setup, mode, pending);
    } else if let Some(spec) = model_restarts {
        s = restart_model::exercise(s, &f, &world, &mut setup, spec);
    } else if let Some(hours) = model_hours {
        path_model::exercise(&mut s, &f, &world, &setup, hours);
    } else if long {
        market_minutes(&mut s, &world, 20, 10_000);
    } else {
        for id in 1..=2u64 {
            command(&mut s, id, &ControlCmd::SnapshotNow);
            settle(&mut s);
        }
    }
    if let Some(restart) = restart {
        if matches!(restart, Restart::Mismatch | Restart::GlobalMismatch) {
            prepare_smoke(&mut s, &world);
            let i = &world.insts[0];
            s.driver_mut().io.writer.fault = Some(if matches!(restart, Restart::Mismatch) {
                EventKind::ORDER_DECISION
            } else {
                EventKind::CMD_RESULT
            });
            command(
                &mut s,
                3,
                &ControlCmd::SmokeOrder {
                    account: world.accounts[0].id,
                    venue: i.venue,
                    _pad: 0,
                    inst: i.id,
                    side: Side::Buy,
                    _pad2: [0; 3],
                    ttl_ms: 30_000,
                },
            );
            settle(&mut s);
            assert!(
                s.driver().io.writer.changed.is_some(),
                "fault never reached a real Output; command result: {:?}",
                s.driver().io.writer.last_result
            );
        }
        if matches!(restart, Restart::Version) {
            command(&mut s, 3, &ControlCmd::Drain { cancel_resting: true });
            let mut done = false;
            for _ in 0..1000 {
                match s.step() {
                    StepOutcome::Busy | StepOutcome::Idle => {}
                    StepOutcome::Done { .. } => {
                        done = true;
                        break;
                    }
                    o => panic!("drain: {o:?}"),
                }
            }
            assert!(done, "drain did not finish");
        }
        s.driver_mut().io.writer.publish();
        // open_offline persists only when sealing. Capture the actual published raw bytes
        // for this simulated process handoff; the later real roll replaces it with the
        // sealed segment. No record is synthesized or assigned a different physical seq.
        let prefix = hft_journal::testing::published_bytes(&s.driver().io.writer).unwrap();
        std::fs::write(&f.file, prefix).unwrap();
        let old = read(&f);
        let initial = old
            .recs
            .iter()
            .find(|r| {
                r.kind == EventKind::STATE_SNAPSHOT
                    && (!long
                        || r.read::<hft_event::SnapshotHdr>()
                            .is_some_and(|h| h.engine_ns == 600_000_000_000))
            })
            .unwrap()
            .seq;
        let prior_last = old.record_end_seq(old.recs.len() - 1).unwrap();
        let mut io = s.into_driver().io;
        io.old = Some(old);
        if let Restart::Gap(skipped) = restart {
            io.writer.debug_gap_on_roll(skipped, true);
        }
        // A new TSC epoch and frequency must come from the new segment and EngineClock.
        let next_utc = io.rings.utc_of(Tsc(io.rings.tsc)) + 20_000_000_000;
        io.rings.tsc = 100;
        io.rings.hz *= 2;
        io.rings.calib.tsc = Tsc(100);
        io.rings.calib.realtime_ns = next_utc;
        setup.clock.calib_tsc = 100;
        setup.clock.tsc_hz_milli = io.rings.hz;
        setup.clock.calib_realtime_ns = io.rings.calib.realtime_ns;
        if matches!(restart, Restart::Version) {
            io.build = [1; 20];
        }
        let from_snapshot = matches!(restart, Restart::FromSnapshot);
        let plan = RecoverPlan {
            start: if from_snapshot {
                SnapshotSel::AtSeq(initial)
            } else if long && matches!(restart, Restart::Version) {
                SnapshotSel::DrainTail
            } else {
                SnapshotSel::Latest
            },
            replay_tail: !(from_snapshot || long && matches!(restart, Restart::Version)),
            epoch: 0,
            flags: RecoveryFlags(if from_snapshot {
                RecoveryFlags::FROM_SNAPSHOT | RecoveryFlags::TAIL_LOST
            } else {
                0
            }),
            quarantine: Vec::new(),
        };
        s = Sequencer::boot(
            BootSource::Journal(plan),
            &setup,
            LiveDriver::new(
                io,
                LiveCfg {
                    prev_last_seq: prior_last,
                    source: if from_snapshot { 3 } else { 2 },
                    ..Default::default()
                },
            ),
            Box::leak(Box::new(Heartbeat::new())),
        )
        .unwrap();
        settle(&mut s);
        if long {
            market_minutes(&mut s, &world, if from_snapshot { 20 } else { 10 }, 20_000);
        } else {
            for id in 10..=11 {
                command(&mut s, id, &ControlCmd::SnapshotNow);
                settle(&mut s);
            }
        }
    }
    let mut d = s.into_driver();
    d.io.writer.publish();
    d.io.writer.inner.seal_and_close().unwrap();
    f
}
