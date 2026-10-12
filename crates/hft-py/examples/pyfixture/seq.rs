//! `pyfixture seq <name> [<row>] <dir>`: journals written by a real hot process — hft-sequencer on
//! hft-testkit's `SeqRig` (TESTS.md `J_EXT`, `F_FF`, `F_SAMPLE`, `F_FILLS`). The rig's in-memory
//! journal is turned into a `TW` script (boot block → `boot` lines, the rest → one `bulk` file); at
//! each named step the hot ledger's snapshot sections (1, 13, 22) and its views are saved:
//!
//! <dir>/script.txt, <dir>/seq.bulk     TW input (one sealed segment)
//! <dir>/mem.bin                        every record of the rig journal (for `seq-check`)
//! <dir>/steps.json                     [{name, seq, proc_ns, file, positions, inventory, portfolio, pnl}]
//! <dir>/expected/ledger-<name>.bin     hot ledger sections after the step
//!
//! `pyfixture seq-check <cold_root> <instance> <dir>` reads the written journal back and compares
//! every record (seq, kind, EVENT_END, body) with `mem.bin`.

use std::cell::RefCell;
use std::path::Path;

use hft_event::{
    AccountConfigReportHdr, BalanceReport, Body, ConnRole, ConnState, ConnStateKind, ControlCmd, EventKind,
    ExternalCause, ExternalFill, GroupSpec, LegDef, LeverageItem, MarginReportHdr, MarkPrice, MarketHdr,
    OrderDecision, OrderEvent, ParamSource, ReconcileReport, RecordFlags, Role, StrategyParamsHdr, Timer,
};
use hft_journal::{EndAt, InstanceName, JournalReader, Next, ReaderConfig, StartAt};
use hft_ledger::{Capacities, LedgerConfig};
use hft_refdata::FundingSettleKind;
use hft_risk::{
    AcctLimits, DEFAULT_MARGIN, FixedVec, GroupLimits, MarginLimits, PreVec, PxBand, RiskParams, VenueLimits,
};
use hft_strategy_sdk::params::{RawParams, decode_body, encode_body, schema_digest, schema_json};
use hft_strategy_sdk::{
    AdviceOut, CmdDesc, Ctx, EvMask, EventRef, FieldDesc, GroupTable, IntentClass, LegLinkOpt, MinorChange,
    ModelLayoutDesc, ModelParamEffect, ModelViews, OrderIntent, ParamError, ParamSchemaDesc, Part, RefView,
    Relax, ResetPreview, RestoreError, SnapshotProgress, SnapshotReader, SnapshotWriter, StrategyDesc,
    StrategyInstance, StrategySet, SubsBuilder, Unit, Views,
};
use hft_testkit::seq_rig::{RigConfig, RingId, SeqRig};
use hft_types::{
    AccountId, AssetId, GROUP_NONE, GroupId, InstId, Notional, Px, Qty, Rate, RunMode, Side, Tif, Tsc,
    VenueId,
};
use serde_json::{Value, json};
use zerocopy::{FromZeros, IntoBytes};

use super::{ACC, ALPHA, BTC, D, ETH, I101, I102, I202, MS, P2, S, S1, USDT, die, hex, refdata_fs, tsc};

// ------------------------------------------------------------------------------------- strategy

/// The one parameter of the fixture strategy (unused by its logic).
static FIELDS: [FieldDesc; 1] = [FieldDesc {
    name: "unused",
    unit: Unit::Count,
    min: 0,
    max: 1,
    default: 0,
    per_group: false,
    structural: false,
    manual_only: false,
    relax: Relax::None,
    minor_change: MinorChange::None,
    since: 1,
    doc: "fixture field",
}];

static SCHEMA: ParamSchemaDesc = ParamSchemaDesc {
    id: 0xFF01,
    version: 1,
    fields: &FIELDS,
};

static KINDS: [StrategyDesc; 1] = [StrategyDesc {
    name: "py-fixture",
    kind_id: 0xFF01,
    version: 1,
    non_prod_only: false,
    schema: &SCHEMA,
    notes: &[],
    report: &[],
    cmds: &[],
}];

thread_local! {
    /// Intents the strategy places on its next `Bbo` event (the scripted "decision").
    static PENDING: RefCell<Vec<OrderIntent>> = const { RefCell::new(Vec::new()) };
}

/// Fast part of the fixture strategy: places the scripted intents.
#[derive(Debug)]
pub struct FxInst {
    i: StrategyInstance,
}

/// Its (empty) Model.
#[derive(Debug)]
pub struct FxModel;

/// The fixture strategy set (kind 0xFF01 = `alpha`'s kind in `RD_PY`).
#[derive(Debug)]
pub struct FxSet;

impl StrategySet for FxSet {
    type Inst = FxInst;
    type ModelInst = FxModel;
    type Params = RawParams;
    fn kinds() -> &'static [StrategyDesc] {
        &KINDS
    }
    fn decode_params(
        kind_id: u16,
        body: &[u8],
        schema_ver: u16,
        g: &GroupTable,
    ) -> Result<RawParams, ParamError> {
        if kind_id != 0xFF01 {
            return Err(ParamError::SchemaMismatch {
                want: 0xFF01,
                got: u32::from(kind_id),
            });
        }
        decode_body(&SCHEMA, schema_ver, g.len(), body)
    }
    fn validate_params(_: &RawParams, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
        Ok(())
    }
    fn create(
        i: StrategyInstance,
        _: &RawParams,
        _: &GroupTable,
        _: &RefView<'_>,
    ) -> Result<FxInst, ParamError> {
        Ok(FxInst { i })
    }
    fn create_model(_: StrategyInstance, _: &RawParams, _: &GroupTable, _: &RefView<'_>) -> FxModel {
        FxModel
    }
    fn on_event<V: Views>(s: &mut FxInst, ev: &EventRef<'_>, _v: &V, ctx: &mut Ctx<'_>) {
        if !matches!(ev, EventRef::Bbo { .. }) {
            return;
        }
        for mut it in PENDING.with(|p| std::mem::take(&mut *p.borrow_mut())) {
            it.account = s.i.account;
            if ctx.place(it).is_err() {
                die("fixture strategy: ctx.place failed");
            }
        }
    }
    fn on_params(_: &mut FxInst, _: &RawParams, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
        Ok(())
    }
    fn subscriptions(_: &FxInst, g: &GroupTable, out: &mut SubsBuilder<'_>) {
        for (gix, sp) in g.iter() {
            for l in sp.legs.iter().take(usize::from(sp.n_legs)) {
                let _ = out.market(l.inst, gix, EvMask::BBO, Part::Fast);
            }
        }
    }
    fn snapshot(_: &FxInst, _: &mut SnapshotWriter) {}
    fn restore(_: &mut FxInst, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
    fn model_mem_bytes(_: &RawParams, _: u16) -> u64 {
        0
    }
    fn model_snapshot_bytes(_: &RawParams, _: u16) -> u64 {
        0
    }
    fn has_model(_: u16) -> bool {
        false
    }
    fn model_layout(_: u16) -> &'static ModelLayoutDesc {
        &ModelLayoutDesc::EMPTY
    }
    fn model_layout_version(_: u16) -> u32 {
        0
    }
    fn model_on_event<V: ModelViews>(_: &mut FxModel, _: &EventRef<'_>, _: &V, _: &mut AdviceOut<'_>) {}
    fn model_on_params(_: &mut FxModel, _: &RawParams, _: &GroupTable, _: &RefView<'_>) -> ModelParamEffect {
        ModelParamEffect::None
    }
    fn model_snapshot_chunk(_: &FxModel, _: u16, _: &mut SnapshotWriter, _: u32) -> SnapshotProgress {
        SnapshotProgress::Done
    }
    fn model_restore(_: &mut FxModel, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
    fn model_path_version(_: u16) -> u32 {
        0
    }
    fn model_snapshot_path(_: &FxModel, _: &mut SnapshotWriter) {}
    fn model_restore_path(_: &mut FxModel, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
    fn model_path_settle_ms(_: &RawParams) -> u64 {
        0
    }
    fn reset_preview(_: &RawParams, _: &GroupTable, _: &RawParams, _: &GroupTable) -> ResetPreview {
        ResetPreview::default()
    }
    fn schema_json(kind_id: u16) -> Option<String> {
        KINDS
            .iter()
            .find(|d| d.kind_id == kind_id)
            .map(|d| schema_json(d, &ModelLayoutDesc::EMPTY, d.schema.version))
    }
    fn cmds(_: u16) -> &'static [CmdDesc] {
        &[]
    }
}

// ------------------------------------------------------------------------------------- config

pub const G1: GroupId = GroupId(0x0100_0001);
pub const G2: GroupId = GroupId(0x0100_0002);

fn leg(inst: InstId, venue: VenueId) -> LegDef {
    LegDef {
        inst,
        venue,
        kind: LegDef::CEX,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }
}

fn groups() -> Vec<GroupSpec> {
    let none = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    vec![
        GroupSpec {
            id: G1,
            asset: BTC,
            n_legs: 2,
            open_allowed: 1,
            legs: [leg(I101, S1), leg(I102, P2), none, none],
            _rsv: 0,
        },
        GroupSpec {
            id: G2,
            asset: ETH,
            n_legs: 1,
            open_allowed: 1,
            legs: [leg(I202, P2), none, none, none],
            _rsv: 0,
        },
    ]
}

/// SPB1 of `alpha` (fixture strategy, groups G1 and G2).
fn spb() -> Vec<u8> {
    let gs = groups();
    let body = encode_body(
        &SCHEMA,
        gs.len() as u16,
        &RawParams {
            defaults: vec![0],
            overrides: Vec::new(),
        },
    )
    .unwrap_or_else(|_| die("encode params"));
    let h = StrategyParamsHdr {
        strategy: ALPHA,
        account: ACC,
        kind_id: 0xFF01,
        schema_id: 0xFF01,
        schema_ver: 1,
        source: ParamSource::Manual,
        _pad: 0,
        n_groups: gs.len() as u16,
        _pad2: 0,
        version: 1,
        generated_at_ns: D,
        body_len: body.len() as u32,
        group_ctr_hwm: 2,
        schema_digest: schema_digest(&KINDS[0], 1),
    };
    let mut v = h.as_bytes().to_vec();
    for g in &gs {
        v.extend_from_slice(g.as_bytes());
    }
    v.extend_from_slice(&body);
    v
}

/// Generous risk parameters: nothing in the fixtures is limited by them.
fn risk() -> RiskParams {
    let big = Notional(1_000_000_000_000_000);
    let group = GroupLimits {
        max_group_notional: big,
        max_pos_qty: Qty(1_000_000_000_000_000),
        naked_gate: Rate(1_000_000_000),
        naked_gate_min: big,
    };
    let mut classes = FixedVec::new();
    classes.push(PxBand {
        max_px_dev: Rate(100_000_000),
        ioc_max_cross: Rate(50_000_000),
        rr_max_cross: Rate(50_000_000),
    });
    let acct = AcctLimits {
        account: ACC,
        max_order_notional: big,
        default_group: group,
        max_account_notional: big,
        account_notional_from_equity: Rate(100_000_000_000),
        max_hedge_notional: big,
        max_reduce_notional: big,
        daily_loss_kill: Rate(1_000_000_000),
        daily_loss_abs: big,
        daily_loss_max_resumes: 1,
        recon_closeonly_notional: big,
        unknown_blocks_open: false,
        funds_buffer: Rate(0),
        margin: MarginLimits {
            margin_stale_ms: 86_400_000,
            ..DEFAULT_MARGIN
        },
    };
    let venues = [S1, P2]
        .into_iter()
        .map(|v| VenueLimits {
            venue: v,
            account: ACC,
            max_venue_notional: big,
            inst_bucket_per_s: 1_000,
            backoff_base_ms: 1_000,
            backoff_max_ms: 8_000,
            rr_reserve: Rate(0),
        })
        .collect();
    RiskParams {
        version: 1,
        generated_at_ns: 0,
        author: [0; 16],
        rate_headroom: Rate(0),
        conn_liveness_ms: 3_600_000,
        max_perp_leverage_x100: 2_000,
        l3_max_cross: Rate(0),
        risk_class: PreVec::from_vec(Vec::new()),
        accounts: PreVec::from_vec(vec![acct]),
        venues: PreVec::from_vec(venues),
        classes,
        groups: PreVec::from_vec(Vec::new()),
    }
}

fn rig_config(p2_funding: FundingSettleKind) -> RigConfig {
    RigConfig {
        // No L2 books: the feed gate then needs only the quotes (hft-sequencer `feeds_gate`).
        refdata_blob: refdata_fs(false, 1, p2_funding, 0),
        params: vec![spb()],
        risk: risk(),
        risk_blob: b"PYFX-RISK".to_vec(),
        caps: Capacities {
            max_insts: 256,
            max_cells_total: 64,
            max_timers: 1_024,
            recent_cmd_ids: 256,
            max_subs_per_inst: 8,
            max_groups_per_instance: 16,
            max_instances: 4,
        },
        // Funding reconciliation items are compared 1 s after the settlement (default 10 min).
        ledger: LedgerConfig {
            funding_recon_lag_ms: 1_000,
            ..LedgerConfig::default()
        },
        venues: vec![S1, P2],
        keys: vec![(ACC, S1), (ACC, P2)],
        epoch: 3,
        tsc_hz_milli: 3_000_000_000_000,
        calib_tsc: 0,
        calib_utc_ns: D,
        boot_tsc: 0,
        max_groups: 16,
    }
}

// ------------------------------------------------------------------------------------- driving

/// Price raw (exponent −2) of `units` USDT.
const fn px(units: i64) -> Px {
    Px(units * 100)
}
/// BTC quantity raw (exponent −8) of `milli` thousandths.
const fn btc_milli(milli: i64) -> Qty {
    Qty(milli * 100_000)
}

fn vix(v: VenueId) -> u8 {
    if v == S1 { 0 } else { 1 }
}

fn venue_of(i: InstId) -> VenueId {
    if i == I101 { S1 } else { P2 }
}

/// One named step of a fixture.
struct Step {
    name: String,
    seq: u64,
    proc_ns: i64,
    bytes: Vec<u8>,
    views: Value,
}

/// A fixture run: the rig, the UTC of the next Input and the named steps.
struct Fx {
    rig: SeqRig<FxSet>,
    t: i64,
    upd: u64,
    rid: u64,
    steps: Vec<Step>,
    rd: hft_refdata::RefData,
    /// Extra facts of the run for the tests (`meta.json`).
    meta: Value,
}

fn mhdr(v: VenueId, i: InstId) -> MarketHdr {
    let mut h = MarketHdr::new_zeroed();
    h.venue = v;
    h.inst = i;
    h
}

impl Fx {
    fn new(p2_funding: FundingSettleKind) -> Fx {
        let c = rig_config(p2_funding);
        let rd = hft_refdata::RefData::load(&c.refdata_blob, None).unwrap_or_else(|e| die(&format!("{e:?}")));
        let rig = SeqRig::<FxSet>::boot(&c).unwrap_or_else(|e| die(&e));
        Fx {
            rig,
            t: D,
            upd: 0,
            rid: 4_000,
            steps: Vec::new(),
            rd,
            meta: json!({}),
        }
    }
    fn now_tsc(&self) -> u64 {
        tsc(self.t)
    }
    /// Advances the clock by 1 ms.
    fn tick(&mut self) {
        self.t += MS;
        let t = tsc(self.t);
        self.rig.set_tsc(t);
    }
    fn run(&mut self) {
        self.rig.run().unwrap_or_else(|e| die(&e));
    }
    fn last_seq(&self) -> u64 {
        self.rig.journal().records().last().map_or(0, |r| r.seq)
    }
    fn push<B: Body>(&mut self, r: RingId, b: &B) {
        self.rig.push_body(r, b);
    }
    /// One Input at the next millisecond, processed to idle; returns the Input's seq.
    fn input(&mut self, r: RingId, kind: EventKind, body: Vec<u8>) -> u64 {
        self.tick();
        let mark = self.rig.journal().len();
        self.rig.push(r, kind, body);
        self.run();
        self.rig
            .journal()
            .records()
            .skip(mark)
            .find(|x| x.kind == kind && x.role() == Role::Input)
            .map_or_else(|| die(&format!("input {kind:?} not journaled")), |x| x.seq)
    }
    fn input_b<B: Body>(&mut self, r: RingId, b: &B) -> u64 {
        self.input(r, B::KIND, b.as_bytes().to_vec())
    }
    /// A confirmed Admin command at the next millisecond; dies unless it succeeds.
    fn cmd(&mut self, c: &ControlCmd<'_>) -> u64 {
        self.tick();
        let mark = self.rig.journal().len();
        let id = self.rig.cmd(ACC, c).unwrap_or_else(|e| die(&e));
        self.run();
        match self.rig.result_of(id) {
            Some(r) if r.status == 0 => {}
            r => {
                let e = self.rig.sequencer().engine();
                eprintln!("gate bits {:#x}", e.risk.mode.gate_bits(ACC));
                for x in self.rig.journal().records().filter(|x| x.kind == EventKind::NOTE) {
                    if let Some(n) = x.read::<hft_event::Note>() {
                        eprintln!(
                            "note seq {} src {:?} code {} v {:?}",
                            x.seq, n.source, n.code, n.v
                        );
                    }
                }
                die(&format!("command {c:?} failed: {r:?}"))
            }
        }
        self.rig
            .journal()
            .records()
            .skip(mark)
            .find(|x| x.kind == EventKind::CONTROL_CMD)
            .map_or(0, |x| x.seq)
    }
    /// Records the hot ledger at this point as step `name` (its Input = `seq`).
    fn step(&mut self, name: &str, seq: u64) {
        let led = self.rig.ledger();
        let (pos, inv, port) = super::views_json(led, &self.rd);
        let mut pnl = [0i64; 8];
        for g in [GROUP_NONE, G1, G2] {
            let p = led.group(ACC, g).pnl;
            for (s, x) in pnl.iter_mut().zip([
                p.price_realized.0,
                p.funding.0,
                p.fee_cex.0,
                p.gas.0,
                p.bribe.0,
                p.unrealized.0,
                p.adjust.0,
                p.external.0,
            ]) {
                *s += x;
            }
        }
        let proc_ns = self
            .rig
            .journal()
            .records()
            .find(|r| r.seq == seq)
            .map_or(self.t, |r| D + (r.tsc / 3) as i64);
        let views = json!({"positions": pos, "inventory": inv, "portfolio": port, "pnl": pnl});
        let bytes = self.rig.ledger_sections();
        self.steps.push(Step {
            name: name.to_string(),
            seq,
            proc_ns,
            bytes,
            views,
        });
    }

    fn conn(&mut self, v: VenueId, k: u8, role: ConnRole, inst: InstId, a: AccountId) {
        let c = ConnState {
            venue: v,
            conn: k,
            state: ConnStateKind::Up,
            role,
            inst,
            reason: 0,
            account: a,
            _pad2: [0; 3],
            rx_tsc: self.now_tsc(),
        };
        self.push(RingId::Rep(vix(v)), &c);
    }
    fn sys(&mut self, id: u32) {
        let tm = Timer {
            timer_id: id,
            owner: 1,
            _pad: 0,
            group: GroupId(0),
            _pad2: 0,
            deadline_tsc: self.now_tsc(),
        };
        self.push(RingId::Hk, &tm);
    }
    fn quote(&mut self, i: InstId, bid: Px, ask: Px) -> u64 {
        self.upd += 1;
        let mut h = mhdr(venue_of(i), i);
        h.rx_tsc = self.now_tsc();
        let b = hft_event::Bbo {
            h,
            bid_px: bid,
            bid_qty: Qty(if i == I202 { 100_000_000 } else { 10_000_000_000 }),
            ask_px: ask,
            ask_qty: Qty(if i == I202 { 100_000_000 } else { 10_000_000_000 }),
            update_id: self.upd,
        };
        self.input_b(RingId::Bbo(vix(venue_of(i))), &b)
    }
    fn balance(&mut self, v: VenueId, asset: AssetId, free: i64) {
        let br = BalanceReport {
            venue: v,
            account: ACC,
            asset,
            _pad: 0,
            free: Qty(free),
            locked: Qty(0),
            ex_ts_ns: 0,
            rx_tsc: Tsc(self.now_tsc()),
        };
        self.push(RingId::Rep(vix(v)), &br);
    }
    fn margin(&mut self, bal: i64) {
        let mut h = MarginReportHdr::new_zeroed();
        h.venue = P2;
        h.account = ACC;
        h.source = 1;
        h.margin_balance = Notional(bal);
        h.maint_margin = Notional(0);
        h.avail_margin = Notional(bal);
        h.margin_ratio = Rate(0);
        h.rx_tsc = Tsc(self.now_tsc());
        self.rig
            .push(RingId::Rep(1), EventKind::MARGIN_REPORT, h.as_bytes().to_vec());
    }
    fn recon(
        &mut self,
        v: VenueId,
        item: u8,
        count: u16,
        idx: u16,
        f: impl FnOnce(&mut ReconcileReport),
    ) -> ReconcileReport {
        let mut x = ReconcileReport::new_zeroed();
        x.report_id = self.rid;
        x.venue = v;
        x.account = ACC;
        x.item = item;
        x.count = count;
        x.idx = idx;
        x.view_seq = self.last_seq();
        f(&mut x);
        x
    }

    /// Startup inputs that release every gate and put G1, G2 in Live (as hft-core's environment
    /// produces them; hft-sequencer TESTS.md B0 "steady").
    fn steady(&mut self) {
        self.run();
        self.tick();
        for v in [S1, P2] {
            self.conn(v, 0, ConnRole::Md, InstId(0), AccountId(0));
            for i in [I101, I102, I202] {
                if venue_of(i) == v {
                    self.conn(v, 0, ConnRole::Md, i, AccountId(0));
                }
            }
            for (k, role) in [
                (0u8, ConnRole::Order),
                (1, ConnRole::Order),
                (0, ConnRole::UserStream),
            ] {
                self.conn(v, k, role, InstId(0), ACC);
            }
        }
        self.run();
        self.tick();
        self.sys(8);
        for v in [S1, P2] {
            let levs: Vec<LeverageItem> = if v == P2 {
                [I102, I202]
                    .into_iter()
                    .map(|inst| LeverageItem {
                        inst,
                        _pad: 0,
                        leverage: Rate(3_000_000_000),
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let mut h = AccountConfigReportHdr::new_zeroed();
            h.venue = v;
            h.account = ACC;
            let (pm, mm, ma) = if v == P2 { (2, 2, 2) } else { (1, 1, 1) };
            h.position_mode = pm;
            h.margin_mode = mm;
            h.multi_asset = ma;
            h.source = 1;
            h.uid_keys = 3;
            h.n_lev = levs.len() as u16;
            h.rx_tsc = Tsc(self.now_tsc());
            let uid = format!("1{}", v.0);
            h.venue_uid[..uid.len()].copy_from_slice(uid.as_bytes());
            let mut body = h.as_bytes().to_vec();
            for l in &levs {
                body.extend_from_slice(l.as_bytes());
            }
            self.rig
                .push(RingId::Rep(vix(v)), EventKind::ACCOUNT_CONFIG_REPORT, body);
            self.rid += 1;
            let x = self.recon(v, 5, 1, 0, |_| {});
            self.push(RingId::Rep(vix(v)), &x);
        }
        self.sys(16 | (u32::from(ACC.0) << 16));
        self.run();
        self.tick();
        self.balance(S1, USDT, 10_000_000_000_000);
        self.balance(S1, BTC, 10_000_000_000);
        self.balance(P2, USDT, 10_000_000_000_000);
        self.margin(10_000_000_000_000);
        self.run();
        self.quote(I101, px(59_999), px(60_000));
        self.quote(I102, px(59_999), px(60_000));
        self.quote(I202, px(2_999), px(3_000));
        self.mark(I102, px(60_000));
        self.mark(I202, px(3_000));
        for g in [G1, G2] {
            let sc = hft_event::CmdScope {
                strategy: ALPHA,
                venue: VenueId(0),
                account: ACC,
                _pad: 0,
                group: g,
                inst: InstId(0),
                _pad2: 0,
            };
            self.cmd(&ControlCmd::SetMode {
                scope: sc,
                mode: RunMode::Live,
            });
        }
    }

    fn mark(&mut self, i: InstId, p: Px) -> u64 {
        let mut h = mhdr(venue_of(i), i);
        h.rx_tsc = self.now_tsc();
        let m = MarkPrice { h, px: p };
        self.input_b(RingId::Md(vix(venue_of(i))), &m)
    }

    /// The strategy places one GTC limit order (on a quote of `inst`); returns its decision.
    fn place(&mut self, inst: InstId, side: Side, g: GroupId, qty: Qty, p: Px) -> OrderDecision {
        let it = OrderIntent {
            venue: venue_of(inst),
            account: ACC,
            side,
            tif: Tif::Gtc,
            class: IntentClass::Open,
            flags: 0,
            _pad: [0; 2],
            inst,
            group: g,
            px: p,
            qty,
            link: LegLinkOpt::NONE,
            tag: 0,
            max_rest_ms: 0,
        };
        PENDING.with(|x| x.borrow_mut().push(it));
        let mark = self.rig.journal().len();
        let (bid, ask) = if inst == I202 {
            (px(2_999), px(3_000))
        } else {
            (px(59_999), px(60_000))
        };
        self.quote(inst, bid, ask);
        let d = self
            .rig
            .journal()
            .records()
            .skip(mark)
            .filter(|r| r.kind == EventKind::ORDER_DECISION)
            .find_map(|r| r.read::<OrderDecision>())
            .unwrap_or_else(|| die("no OrderDecision for the scripted intent"));
        if d.reject != 0 {
            die(&format!("scripted order rejected: {d:?}"));
        }
        d
    }

    /// An `OrderEvent` of decision `d`.
    fn order_event(&self, d: &OrderDecision, what: hft_event::OrderEventKind) -> OrderEvent {
        OrderEvent {
            venue: d.venue,
            account: d.account,
            what,
            detail: 0,
            slot: d.slot,
            r#gen: d.r#gen,
            inst: d.inst,
            px: d.px,
            qty: Qty(0),
            cum_filled: Qty(0),
            fee: Qty(0),
            fee_asset: USDT,
            epoch: self.rig.epoch() as u16,
            reject_code: 0,
            exch_order_id: 1_000 + u64::from(d.slot.0),
            ex_ts_ns: self.t - 2 * MS,
            hw_rx_ns: 0,
            rx_tsc: 0,
            avg_px: d.px,
            cash_qty: hft_types::Qty(0),
            cash_sum: hft_types::Qty(0),
            trade_id: 0,
        }
    }
    fn ack(&mut self, d: &OrderDecision) -> u64 {
        let ev = self.order_event(d, hft_event::OrderEventKind::Ack);
        self.input_b(RingId::Rep(vix(d.venue)), &ev)
    }
    /// A fill report of `d` (`qty` this fill, `cum` cumulative).
    fn fill(&mut self, d: &OrderDecision, qty: Qty, cum: Qty, p: Px, fee: (Qty, AssetId)) -> u64 {
        let what = if cum.0 >= d.qty.0 {
            hft_event::OrderEventKind::Fill
        } else {
            hft_event::OrderEventKind::PartialFill
        };
        let mut ev = self.order_event(d, what);
        ev.qty = qty;
        ev.cum_filled = cum;
        ev.px = p;
        ev.avg_px = p;
        ev.fee = fee.0;
        ev.fee_asset = fee.1;
        self.input_b(RingId::Rep(vix(d.venue)), &ev)
    }
    /// A filled order of the strategy (placed, acked, filled at `p`).
    fn position(&mut self, inst: InstId, side: Side, g: GroupId, qty: Qty, p: Px) -> OrderDecision {
        let d = self.place(inst, side, g, qty, p);
        self.ack(&d);
        self.fill(&d, qty, qty, p, (Qty(0), USDT));
        d
    }
    fn external(
        &mut self,
        cause: ExternalCause,
        side: Side,
        qty: Qty,
        p: Px,
        f: impl FnOnce(&mut ExternalFill),
    ) -> u64 {
        let mut x = ExternalFill {
            venue: P2,
            account: ACC,
            side,
            cause,
            inst: I102,
            qty,
            px: p,
            fee: Qty(0),
            fee_asset: USDT,
            _pad: [0; 6],
            exch_order_id: 0,
            trade_id: 0,
            ex_ts_ns: self.t,
            rx_tsc: Tsc(self.now_tsc()),
            fill_seq: 0,
            cash_qty: hft_types::Qty(0),
            cash_amount: hft_types::Qty(0),
        };
        f(&mut x);
        self.input_b(RingId::Rep(1), &x)
    }
    /// A foreign report on `I102`: `slot`/`gen` (0 = `SLOT_UNRESOLVED`), Buy (`detail`), `cum`.
    fn foreign(&mut self, slot: u32, r#gen: u32, cum: Qty, p: Px, eoid: u64) -> u64 {
        let ev = OrderEvent {
            venue: P2,
            account: ACC,
            what: hft_event::OrderEventKind::Fill,
            detail: Side::Buy as u8,
            slot: hft_types::SlotId(slot),
            r#gen,
            inst: I102,
            px: p,
            qty: cum,
            cum_filled: cum,
            fee: Qty(90_000),
            fee_asset: USDT,
            epoch: self.rig.epoch() as u16,
            reject_code: 0,
            exch_order_id: eoid,
            ex_ts_ns: self.t - 2 * MS,
            hw_rx_ns: 0,
            rx_tsc: 0,
            avg_px: p,
            cash_qty: hft_types::Qty(0),
            cash_sum: hft_types::Qty(0),
            trade_id: 0,
        };
        self.input_b(RingId::Rep(1), &ev)
    }

    // --------------------------------------------------------------------------------- output

    fn write(&self, dir: &Path) {
        // `PYFX_DEBUG=1`: every journal record (and its Note fields) on stderr.
        if std::env::var_os("PYFX_DEBUG").is_some() {
            for r in self.rig.journal().records() {
                let note = r.read::<hft_event::Note>().filter(|_| r.kind == EventKind::NOTE);
                eprintln!(
                    "seq {} kind {:#06x} flags {:#x} tsc {} {:?}",
                    r.seq,
                    r.kind.0,
                    r.flags.0,
                    r.tsc,
                    note.map(|n| (n.source, n.code, n.v))
                );
            }
        }
        let j = self.rig.journal();
        if j.segments.len() != 1 {
            die("the rig journal rolled; the fixtures expect one segment");
        }
        let recs: Vec<&hft_testkit::seq_rig::MemRecord> = j.records().collect();
        let boot_done = recs
            .iter()
            .position(|r| r.kind == EventKind::SEGMENT_ROLL && r.body.get(32) == Some(&3))
            .unwrap_or_else(|| die("no BootDone"));
        std::fs::create_dir_all(dir.join("expected")).unwrap_or_else(|e| die(&e.to_string()));
        let mut script = String::new();
        for r in recs.iter().take(boot_done).skip(1) {
            if r.tsc != 0 {
                die("boot record off the boot TSC");
            }
            script.push_str(&format!("boot {:#06x} {}\n", r.kind.0, hex(&r.body)));
        }
        let mut bulk = Vec::new();
        let mut mem = Vec::new();
        for r in &recs {
            mem.extend_from_slice(&r.seq.to_le_bytes());
            mem.extend_from_slice(&r.kind.0.to_le_bytes());
            mem.extend_from_slice(&r.flags.0.to_le_bytes());
            mem.extend_from_slice(&(r.body.len() as u32).to_le_bytes());
            mem.extend_from_slice(&r.tsc.to_le_bytes());
            mem.extend_from_slice(&r.body);
        }
        for r in recs.iter().skip(boot_done + 1) {
            let keep = !(RecordFlags::EVENT_END.0 | RecordFlags::PART.0 | RecordFlags::CRC.0);
            bulk.push(1);
            bulk.extend_from_slice(&r.kind.0.to_le_bytes());
            bulk.extend_from_slice(&(r.flags.0 & keep).to_le_bytes());
            bulk.extend_from_slice(&(r.body.len() as u32).to_le_bytes());
            bulk.extend_from_slice(&r.tsc.to_le_bytes());
            bulk.extend_from_slice(&r.body);
            if r.flags.0 & RecordFlags::EVENT_END.0 != 0 {
                bulk.push(2);
            }
        }
        if recs
            .last()
            .is_some_and(|r| r.flags.0 & RecordFlags::EVENT_END.0 == 0)
        {
            die("the rig journal ends inside an event");
        }
        let w = |name: &str, b: &[u8]| {
            std::fs::write(dir.join(name), b).unwrap_or_else(|e| die(&format!("{name}: {e}")));
        };
        w("seq.bulk", &bulk);
        w("mem.bin", &mem);
        script.push_str(&format!("bulk {}\nseal\n", dir.join("seq.bulk").display()));
        w("script.txt", script.as_bytes());
        let mut steps = Vec::new();
        for s in &self.steps {
            let file = format!("expected/ledger-{}.bin", s.name);
            w(&file, &s.bytes);
            let mut v = s.views.clone();
            if let Some(o) = v.as_object_mut() {
                o.insert("name".into(), json!(s.name));
                o.insert("seq".into(), json!(s.seq));
                o.insert("proc_ns".into(), json!(s.proc_ns));
                o.insert("file".into(), json!(file));
            }
            steps.push(v);
        }
        w("steps.json", Value::Array(steps).to_string().as_bytes());
        w("meta.json", self.meta.to_string().as_bytes());
    }
}

// ------------------------------------------------------------------------------------- fixtures

/// `J_EXT`: G1's perpetual leg long 10.0 (a filled order of the strategy), then E1–E4.
fn ext(f: &mut Fx) {
    f.position(I102, Side::Buy, G1, btc_milli(10_000), px(60_000));
    f.mark(I102, px(60_500));
    // A clean reconciliation round of P2: its view seq is after the user stream came up
    // (AUTO_EXTERNAL precondition, hft-risk `auto_external_ok`).
    f.rid += 1;
    let x = f.recon(P2, 5, 1, 0, |_| {});
    f.input_b(RingId::Rep(1), &x);
    let s = f.last_seq();
    f.step("E0", s);
    // E1: report 4242 = position item (AUTO_EXTERNAL: exchange 6.0, engine 10.0) + summary.
    f.rid = 4242;
    let p = f.recon(P2, 1, 2, 0, |x| {
        x.inst = I102;
        x.asset = BTC;
        x.engine_qty = btc_milli(10_000);
        x.exch_qty = btc_milli(6_000);
        x.sflags = ReconcileReport::AUTO_EXTERNAL;
    });
    f.input_b(RingId::Rep(1), &p);
    let sm = f.recon(P2, 5, 2, 1, |_| {});
    let e1 = f.input_b(RingId::Rep(1), &sm);
    f.step("E1", e1);
    // E2: the exchange's late ADL push of the same reduction (offset by auto_held).
    let e2 = f.external(
        ExternalCause::Adl,
        Side::Sell,
        btc_milli(1_000),
        px(60_500),
        |x| x.trade_id = 77,
    );
    f.step("E2", e2);
    // E3: a StaleGen report with a fill (slot 9 never used in this run).
    let e3 = f.foreign(9, 1, btc_milli(300), px(60_100), 99);
    f.step("E3", e3);
    // E4: a liquidation fill pushed as an Input.
    let e4 = f.external(
        ExternalCause::Liquidation,
        Side::Buy,
        btc_milli(400),
        px(60_200),
        |x| x.trade_id = 910,
    );
    f.step("E4", e4);
}

/// `F_FF` row `n` (PY-109): foreign fills through the OMS defensive and StaleGen paths.
fn ff(f: &mut Fx, row: u32) {
    let s0 = f.last_seq();
    f.step("start", s0);
    match row {
        1 => {
            let s = f.foreign(0, 0, btc_milli(300), px(60_100), 99);
            f.step("end", s);
        }
        2 => {
            let s = f.foreign(9, 1, btc_milli(300), px(60_100), 99);
            f.step("end", s);
        }
        4 => {
            let s = f.external(
                ExternalCause::Unknown,
                Side::Buy,
                btc_milli(300),
                px(60_100),
                |x| {
                    x.exch_order_id = 99;
                    x.fill_seq = btc_milli(500).0 as u64;
                },
            );
            f.step("ext", s);
            let s = f.foreign(0, 0, btc_milli(500), px(60_100), 99);
            f.step("end", s);
        }
        5 => {
            let s = f.foreign(0, 0, btc_milli(200), px(60_100), 99);
            f.step("first", s);
            let s = f.foreign(0, 0, btc_milli(500), px(60_100), 99);
            f.step("end", s);
        }
        _ => die("ff row: 1, 2, 4 or 5"),
    }
}

/// `F_SAMPLE` row `n` (PY-111): booking samples.
fn sample(f: &mut Fx, row: u32) {
    match row {
        1 => {
            // Spot buy 0.01 BTC on 101, fee 0.00001 BTC (base asset).
            let d = f.place(I101, Side::Buy, G1, btc_milli(10), px(60_000));
            f.ack(&d);
            let s = f.fill(&d, btc_milli(10), btc_milli(10), px(60_000), (Qty(1_000), BTC));
            f.step("end", s);
        }
        2 => {
            f.position(I102, Side::Buy, G1, btc_milli(1_000), px(60_000));
            let s = f.external(
                ExternalCause::Liquidation,
                Side::Sell,
                btc_milli(400),
                px(59_000),
                |x| {
                    x.trade_id = 31;
                    x.fee = Qty(10_000);
                },
            );
            f.step("end", s);
        }
        3 => {
            f.tick();
            f.balance(S1, USDT, 9_000_000_000_000);
            f.balance(S1, BTC, 20_000_000_000);
            f.run();
            f.cmd(&ControlCmd::SetBalanceBaseline {
                account: ACC,
                venue: S1,
            });
            f.tick();
            f.balance(S1, USDT, 9_500_000_000_000);
            f.run();
            let s = f.last_seq();
            f.step("drift", s);
            f.tick();
            // The original wallet is unchanged by either balance report. Record a matching
            // report so Python verifies both setting and clearing the cold drift flag.
            f.balance(S1, USDT, 10_000_000_000_000);
            f.run();
            let s = f.last_seq();
            f.step("end", s);
        }
        4 => {
            // BalanceOnlyPush venue: the funding of the held perpetual is synthesized at the
            // settlement (D, already passed: the driver's `Timer{Sys, FundingSettle}`), then the
            // reconciliation's wallet item books the exchange amount (`AdjustDelta::Funding`).
            f.position(I102, Side::Buy, G1, btc_milli(1_000), px(60_000));
            let mut h = mhdr(P2, I102);
            h.rx_tsc = f.now_tsc();
            let fu = hft_event::Funding {
                h,
                rate: Rate(100_000),
                next_funding_ns: D,
                interval_s: 28_800,
                _pad: 0,
            };
            let s = f.input_b(RingId::Md(1), &fu);
            let synth = f
                .rig
                .journal()
                .records()
                .filter(|r| r.kind == EventKind::FUNDING_SETTLE)
                .filter_map(|r| r.read::<hft_event::FundingSettle>())
                .last()
                .unwrap_or_else(|| die("no synthesized FundingSettle"));
            if synth.synth != 1 {
                die("the FundingSettle is not synthesized");
            }
            let s = f.last_seq().max(s);
            f.step("synth", s);
            // Past the funding reconciliation lag (`funding_recon_lag_ms`).
            f.t += 2 * S;
            f.rid += 1;
            let x = f.recon(P2, 6, 1, 0, |x| {
                x.inst = InstId(0);
                x.asset = USDT;
                x.exch_ts_ns = synth.settle_ts_ns;
                x.engine_amount = Notional(0);
                x.exch_amount = synth.amount;
            });
            let s = f.input_b(RingId::Rep(1), &x);
            f.step("end", s);
        }
        5 => {
            // Spot leg 0.01 BTC of G1 with no perpetual leg: the excess leg is written off to
            // the account level.
            let d = f.place(I101, Side::Buy, G1, btc_milli(10), px(60_000));
            f.ack(&d);
            let s = f.fill(&d, btc_milli(10), btc_milli(10), px(60_000), (Qty(0), USDT));
            f.step("filled", s);
            let s = f.cmd(&ControlCmd::WriteOffResidual {
                account: ACC,
                _pad: [0; 3],
                group: G1,
            });
            f.step("end", s);
        }
        6 => {
            f.rid += 1;
            let rid = f.rid;
            let p = f.recon(P2, 1, 2, 0, |x| {
                x.inst = I102;
                x.asset = BTC;
                x.engine_qty = Qty(0);
                x.exch_qty = btc_milli(500);
            });
            f.input_b(RingId::Rep(1), &p);
            let sm = f.recon(P2, 5, 2, 1, |_| {});
            f.input_b(RingId::Rep(1), &sm);
            let s = f.cmd(&ControlCmd::AdoptExchange {
                account: ACC,
                venue: P2,
                inst: I102,
                report_id: rid,
                // hft-event `codes::ADOPT_ITEM`: Position = 0.
                item: 0,
                _pad: 0,
                asset: BTC,
                group: G1,
                entry_px: px(60_000),
            });
            f.step("end", s);
        }
        _ => die("sample row: 1–6"),
    }
}

/// `F_FILLS` row `n` (PY-112): fills of one order deduplicated by `cum_filled`.
fn fills(f: &mut Fx, row: u32) {
    let d = f.place(I102, Side::Buy, G1, btc_milli(500), px(60_000));
    f.ack(&d);
    let s = f.last_seq();
    f.step("placed", s);
    let p = px(60_000);
    let fee = (Qty(0), USDT);
    let s = match row {
        1 => {
            f.fill(&d, btc_milli(200), btc_milli(200), p, fee);
            f.fill(&d, btc_milli(300), btc_milli(500), p, fee)
        }
        2 => {
            f.fill(&d, btc_milli(200), btc_milli(200), p, fee);
            f.fill(&d, btc_milli(200), btc_milli(200), p, fee);
            f.fill(&d, btc_milli(300), btc_milli(500), p, fee)
        }
        3 => {
            f.fill(&d, btc_milli(500), btc_milli(500), p, fee);
            f.fill(&d, btc_milli(200), btc_milli(200), p, fee)
        }
        4 => {
            let mut d2 = d;
            d2.r#gen = d.r#gen + 1;
            f.fill(&d2, btc_milli(500), btc_milli(500), p, fee)
        }
        _ => die("fills row: 1–4"),
    };
    f.step("end", s);
    f.meta = json!({"epoch": f.rig.epoch(), "slot": d.slot.0, "gen": d.r#gen, "inst": d.inst.0,
        "side": d.side as u8, "group": d.group.0, "action": d.action as u8});
}

/// `pyfixture seq <name> [<row>] <dir>`.
pub fn main(args: &[String]) {
    let arg = |i: usize| {
        args.get(i)
            .map(String::as_str)
            .unwrap_or_else(|| die("missing argument"))
    };
    let name = arg(0);
    let (row, dir) = if name == "ext" {
        (0, arg(1))
    } else {
        (arg(1).parse::<u32>().unwrap_or_else(|_| die("row")), arg(2))
    };
    let funding = if name == "sample" && row == 4 {
        FundingSettleKind::BalanceOnlyPush
    } else {
        FundingSettleKind::PerSymbolPush
    };
    let mut f = Fx::new(funding);
    f.steady();
    match name {
        "ext" => ext(&mut f),
        "ff" => ff(&mut f, row),
        "sample" => sample(&mut f, row),
        "fills" => fills(&mut f, row),
        _ => die("seq fixture: ext, ff, sample or fills"),
    }
    f.write(Path::new(dir));
}

/// `pyfixture seq-check <cold_root> <instance> <dir>`.
pub fn check(cold_root: &Path, instance: &str, dir: &Path) {
    let mem = std::fs::read(dir.join("mem.bin")).unwrap_or_else(|e| die(&format!("mem.bin: {e}")));
    let mut want = Vec::new();
    let mut o = 0usize;
    while o < mem.len() {
        let rd = |a: usize, n: usize| mem.get(a..a + n).unwrap_or_else(|| die("mem.bin truncated"));
        let seq = u64::from_le_bytes(rd(o, 8).try_into().unwrap_or([0; 8]));
        let kind = u16::from_le_bytes([rd(o + 8, 2)[0], rd(o + 8, 2)[1]]);
        let flags = u16::from_le_bytes([rd(o + 10, 2)[0], rd(o + 10, 2)[1]]);
        let len = u32::from_le_bytes(rd(o + 12, 4).try_into().unwrap_or([0; 4])) as usize;
        let body = rd(o + 24, len).to_vec();
        o += 24 + len;
        want.push((seq, kind, flags & RecordFlags::EVENT_END.0, body));
    }
    let inst: InstanceName = instance.parse().unwrap_or_else(|_| die("instance"));
    let mut rc = ReaderConfig::new(
        cold_root.join(".nohot"),
        cold_root.to_path_buf(),
        inst,
        "seq".into(),
    );
    rc.start = StartAt::Oldest;
    rc.end = EndAt::LastCompleteEvent;
    let mut r = JournalReader::open(&rc).unwrap_or_else(|e| die(&format!("open: {e:?}")));
    let mut got = Vec::new();
    loop {
        match r.poll() {
            Next::Record(v) => got.push((
                v.hdr.seq,
                v.hdr.kind,
                v.hdr.flags & RecordFlags::EVENT_END.0,
                v.body.to_vec(),
            )),
            Next::Event(e) => got.push((
                e.seq,
                e.kind.0,
                e.flags.0 & RecordFlags::EVENT_END.0,
                e.body.to_vec(),
            )),
            Next::End | Next::Pending => break,
            Next::Gap(g) => die(&format!("gap {g:?}")),
            Next::Err(e) => die(&format!("read error {e:?}")),
        }
    }
    // The writer's own records (segment End of the seal) follow the rig's last record.
    for (i, w) in want.iter().enumerate() {
        let Some(g) = got.get(i) else {
            die(&format!("journal ends at record {i}; the rig has {}", want.len()))
        };
        let roll = w.1 == EventKind::SEGMENT_ROLL.0;
        // A Meta record appended between events (e.g. `LatencySample` after the event's end) is a
        // complete event of its own in the writer (`EVENT_END` set); `MemJournal` leaves it unset.
        let meta = hft_sequencer_role(EventKind(w.1), &w.3) == Role::Meta;
        if g.0 != w.0 || g.1 != w.1 || (!meta && g.2 != w.2) || (!roll && g.3 != w.3) {
            eprintln!("pyfixture seq-check: record {i} differs");
            eprintln!(
                "  rig     seq {} kind {:#06x} end {} len {}",
                w.0,
                w.1,
                w.2,
                w.3.len()
            );
            eprintln!(
                "  journal seq {} kind {:#06x} end {} len {}",
                g.0,
                g.1,
                g.2,
                g.3.len()
            );
            std::process::exit(2);
        }
    }
}

/// Role of a record from its kind and body (hft-event classification; Meta when unknown).
fn hft_sequencer_role(kind: EventKind, body: &[u8]) -> Role {
    hft_event::classify(kind, body).unwrap_or(Role::Meta)
}
