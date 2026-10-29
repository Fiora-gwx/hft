//! One research run and a set of runs (README "运行"): boot from the journal's first snapshot,
//! the variant's `SetParams`, the step loop, the per-run tables.

use std::path::PathBuf;

use hft_event::{
    BlobKind, CmdHdr, CmdResult, CmdScope, ControlCmd, EventKind, ExternalFill, OrderAction, OrderDecision,
    OrderEvent, OrderEventKind, RecordFlags, StrategyParamsHdr,
};
use hft_refdata::RefData;
use hft_rt::Heartbeat;
use hft_sequencer::driver::replay_snapshot::check_snapshot;
use hft_sequencer::{
    BootSource, EngineSetup, RecoverPlan, RecoveryFlags, Sequencer, SnapshotSel, StepOutcome, read_body,
};
use hft_strategy_sdk::StrategySet;
use hft_types::{AccountId, GroupId, InstId, Side, StrategyId, VenueId};
use zerocopy::TryFromBytes;

use crate::driver::{Captured, DriverCfg, DriverStats, ResearchDriver, blob_input, read_prefix};
use crate::source::Source;
use crate::table::{Col, Table};
use crate::variants::{Variant, hex, variant_blob};
use crate::venue::{ResearchVenue, SimParams};

/// Inputs of one run.
#[derive(Clone, Debug)]
pub struct RunCfg {
    pub files: Vec<PathBuf>,
    /// Simulated accounts; `None` = every account bound to a strategy instance.
    pub accounts: Option<Vec<AccountId>>,
    pub variant: Variant,
    pub sim: SimParams,
    pub driver: DriverCfg,
    pub reassembly: usize,
}

/// What one run produced.
#[derive(Debug)]
pub struct RunOut {
    pub variant: String,
    pub seed: u64,
    /// SHA-256 of each simulated account's active parameter blob after the variant was applied.
    pub params_sha: Vec<(AccountId, String)>,
    pub mask: Vec<AccountId>,
    pub first_seq: u64,
    pub start_snapshot: u64,
    pub records_read: u64,
    pub stats: DriverStats,
    pub sim_state_hash: u64,
    pub decisions: Table,
    pub fills: Table,
    pub orders: Table,
    pub windows: Table,
}

static HB: Heartbeat = Heartbeat::new();

fn err<E: core::fmt::Debug>(what: &str) -> impl FnOnce(E) -> String + '_ {
    move |e| format!("{what}: {e:?}")
}

/// The research run of one variant and seed.
pub fn run_one<S: StrategySet>(cfg: &RunCfg) -> Result<RunOut, String> {
    let mut src = Source::open(&cfg.files, cfg.reassembly).map_err(err("journal"))?;
    let prefix = read_prefix(&mut src).map_err(err("journal"))?;
    for r in &prefix.recs {
        if r.kind == EventKind::CONFIG_BLOB
            && let Some(h) = r.read::<hft_event::BlobHdr>()
            && h.blob.kind == BlobKind::StrategyParams
            && let Some(b) = r.body.get(72..)
            && let Ok((p, _)) = StrategyParamsHdr::try_read_from_prefix(b)
            && !S::kinds().iter().any(|k| k.kind_id == p.kind_id)
        {
            return Err(format!(
                "strategy kind {} at seq {} is not registered",
                p.kind_id, r.seq
            ));
        }
    }
    let first_seq = prefix.recs.first().map_or(0, |r| r.seq);
    let at = prefix
        .recs
        .iter()
        .rev()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .map(|r| r.seq)
        .ok_or("no StateSnapshot")?;
    check_snapshot::<S>(&prefix, at).map_err(err("start snapshot"))?;
    let setup = EngineSetup::from_snapshot(&prefix, at).map_err(err("start snapshot"))?;
    let drv = ResearchDriver::new(prefix, src, cfg.driver);
    let plan = RecoverPlan {
        start: SnapshotSel::AtSeq(at),
        replay_tail: true,
        epoch: 0,
        flags: RecoveryFlags(0),
        quarantine: Vec::new(),
    };
    let mut seq = Sequencer::<ResearchDriver, S>::boot(BootSource::Journal(plan), &setup, drv, &HB)
        .map_err(err("boot"))?;
    let rd = seq.engine().rd.clone();
    let mask: Vec<AccountId> = match &cfg.accounts {
        Some(a) => a.clone(),
        None => {
            let mut v: Vec<AccountId> = seq.instances().slots.iter().map(|s| s.account).collect();
            v.sort();
            v.dedup();
            v
        }
    };
    let hz = match seq.engine().clock.row.tsc_hz_milli {
        0 => seq.engine().seg.tsc_hz_milli,
        h => h,
    };
    let venue = ResearchVenue::new(&rd, &mask, hz, &cfg.sim)?;
    seq.driver_mut().venue = Some(venue);
    let cmd_ids = inject_variant::<S>(&mut seq, &mask, &cfg.variant)?;
    loop {
        match seq.step() {
            StepOutcome::Busy => {}
            StepOutcome::Idle | StepOutcome::Done { .. } => break,
            StepOutcome::Fatal(f) => return Err(format!("engine fatal: {f:?}")),
        }
        if seq.driver().finished() {
            break;
        }
    }
    // The last event's outputs are stamped by one more view of the engine.
    let _ = seq.step();
    let drv = seq.driver();
    if let Some(e) = &drv.err {
        return Err(format!("journal: {e}"));
    }
    for id in cmd_ids {
        let r = drv
            .sink
            .captured
            .iter()
            .filter(|c| c.kind == EventKind::CMD_RESULT)
            .filter_map(|c| read_body::<CmdResult>(&c.body))
            .find(|r| r.cmd_id == id)
            .ok_or("variant SetParams has no result")?;
        if r.status != 0 {
            return Err(format!(
                "variant {} SetParams refused: status {} detail {:#x}",
                cfg.variant.name, r.status, r.detail
            ));
        }
    }
    let params_sha = seq
        .instances()
        .slots
        .iter()
        .filter(|s| mask.contains(&s.account))
        .map(|s| (s.account, hex(&s.blob.sha256)))
        .collect();
    let drv = seq.driver();
    let (fills, orders, windows) = order_tables(&rd, &drv.sink.captured, &drv.obs);
    Ok(RunOut {
        variant: cfg.variant.name.clone(),
        seed: cfg.sim.seed,
        params_sha,
        mask,
        first_seq,
        start_snapshot: at,
        records_read: 0,
        stats: drv.stats,
        sim_state_hash: drv.venue.as_ref().map_or(0, ResearchVenue::state_hash),
        decisions: decisions_table(&rd, &drv.sink, &drv.sink.captured),
        fills,
        orders,
        windows,
    })
}

/// `ConfigBlob` + `SetParams` (issuer `pipeline`) for every strategy instance bound to a masked
/// account whose schema has at least one of the variant's fields (each field applies to the
/// instances that have it; a field no instance has is an error). Returns the command ids.
fn inject_variant<S: StrategySet>(
    seq: &mut Sequencer<ResearchDriver, S>,
    mask: &[AccountId],
    v: &Variant,
) -> Result<Vec<[u8; 16]>, String> {
    if v.set.is_empty() {
        return Ok(Vec::new());
    }
    let targets: Vec<(StrategyId, AccountId, hft_event::BlobRef)> = seq
        .instances()
        .slots
        .iter()
        .filter(|s| mask.contains(&s.account) && s.blob.kind == BlobKind::StrategyParams)
        .map(|s| (s.id, s.account, s.blob))
        .collect();
    if targets.is_empty() {
        return Err(format!(
            "variant {}: no strategy instance on the simulated accounts",
            v.name
        ));
    }
    let tsc = seq.driver().start_tsc();
    let version = seq.engine().risk.mode.ctl_state_version();
    let mut ids = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    for (k, (sid, account, base_ref)) in targets.into_iter().enumerate() {
        let base = seq
            .driver()
            .blob(&base_ref)
            .ok_or_else(|| {
                format!(
                    "variant {}: active params blob of strategy {} not in the journal",
                    v.name, sid.0
                )
            })?
            .to_vec();
        let known = crate::variants::fields_of::<S>(&base)?;
        let mine = Variant {
            name: v.name.clone(),
            set: v
                .set
                .iter()
                .filter(|(n, _)| known.contains(&n.as_str()))
                .map(|(n, x)| (n.clone(), *x))
                .collect(),
        };
        if mine.set.is_empty() {
            continue;
        }
        used.extend(mine.set.keys().cloned());
        let (content, r) = variant_blob::<S>(&base, &base_ref, &mine)?;
        let scope = CmdScope {
            strategy: sid,
            venue: VenueId(0),
            account,
            _pad: 0,
            group: GroupId(0),
            inst: InstId(0),
            _pad2: 0,
        };
        let mut h = CmdHdr::zeroed();
        let id = {
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(b"research");
            id[8] = k as u8;
            id[15] = 0xA5;
            id
        };
        h.cmd_id = id;
        h.issuer[..8].copy_from_slice(b"pipeline");
        h.role = 0;
        h.account = account;
        h.cmd_ver = 1;
        h.confirmed = 1;
        h.expect_state_version = version;
        let c = ControlCmd::SetParams { scope, blob: r };
        let mut buf = vec![0u8; 8192];
        let n = c.encode(&h, &mut buf).map_err(err("SetParams"))?;
        buf.truncate(n);
        let bi = blob_input(r, scope, &content);
        seq.driver_mut().push_variant(r, content, bi, buf, tsc);
        ids.push(id);
    }
    if let Some(n) = v.set.keys().find(|n| !used.contains(*n)) {
        return Err(format!(
            "variant {}: unknown parameter {n} (no simulated strategy instance has it)",
            v.name
        ));
    }
    Ok(ids)
}

fn decisions_table(rd: &RefData, sink: &crate::driver::ResearchSink, cap: &[Captured]) -> Table {
    let mut t = DecCols::default();
    for c in cap.iter().filter(|c| c.kind == EventKind::ORDER_DECISION) {
        let Some(d) = read_body::<OrderDecision>(&c.body) else {
            continue;
        };
        t.seq.push(c.seq);
        t.trigger_seq.push(d.trigger_seq);
        t.trigger_log_seq.push(sink.log_seq(d.trigger_seq));
        t.tsc.push(c.tsc);
        t.utc_ns.push(c.utc_ns);
        t.engine_ns.push(c.engine_ns);
        t.strategy.push(d.strategy.0);
        t.account.push(d.account.0);
        t.inst.push(i64::from(d.inst.0));
        t.group.push(i64::from(d.group.0));
        t.action.push(d.action as u8);
        t.side.push(d.side as u8);
        t.tif.push(d.tif as u8);
        t.oflags.push(i64::from(d.oflags));
        t.px.push(d.px.0);
        t.qty.push(d.qty.0);
        t.px_exp.push(rd.inst(d.inst).px_exp as u8);
        t.qty_exp.push(rd.inst(d.inst).qty_exp as u8);
        t.slot.push(i64::from(d.slot.0));
        t.r#gen.push(i64::from(d.r#gen));
        t.target_slot.push(i64::from(d.target_slot.0));
        t.reject.push(i64::from(d.reject));
        t.reason.push(i64::from(d.reason));
        t.intent_no.push(i64::from(d.intent_no));
        t.shadow.push(u8::from(c.flags.0 & RecordFlags::SHADOW.0 != 0));
    }
    t.table()
}

#[derive(Default)]
struct DecCols {
    seq: Vec<u64>,
    trigger_seq: Vec<u64>,
    trigger_log_seq: Vec<u64>,
    tsc: Vec<u64>,
    utc_ns: Vec<i64>,
    engine_ns: Vec<i64>,
    strategy: Vec<u8>,
    account: Vec<u8>,
    inst: Vec<i64>,
    group: Vec<i64>,
    action: Vec<u8>,
    side: Vec<u8>,
    tif: Vec<u8>,
    oflags: Vec<i64>,
    px: Vec<i64>,
    qty: Vec<i64>,
    px_exp: Vec<u8>,
    qty_exp: Vec<u8>,
    slot: Vec<i64>,
    r#gen: Vec<i64>,
    target_slot: Vec<i64>,
    reject: Vec<i64>,
    reason: Vec<i64>,
    intent_no: Vec<i64>,
    shadow: Vec<u8>,
}

impl DecCols {
    fn table(self) -> Table {
        Table::new(vec![
            ("seq", Col::U64(self.seq)),
            ("trigger_seq", Col::U64(self.trigger_seq)),
            ("trigger_log_seq", Col::U64(self.trigger_log_seq)),
            ("tsc", Col::U64(self.tsc)),
            ("utc_ns", Col::I64(self.utc_ns)),
            ("engine_ns", Col::I64(self.engine_ns)),
            ("strategy", Col::U8(self.strategy)),
            ("account", Col::U8(self.account)),
            ("inst", Col::I64(self.inst)),
            ("group", Col::I64(self.group)),
            ("action", Col::U8(self.action)),
            ("side", Col::U8(self.side)),
            ("tif", Col::U8(self.tif)),
            ("oflags", Col::I64(self.oflags)),
            ("px", Col::I64(self.px)),
            ("qty", Col::I64(self.qty)),
            ("px_exp", Col::U8(self.px_exp)),
            ("qty_exp", Col::U8(self.qty_exp)),
            ("slot", Col::I64(self.slot)),
            ("gen", Col::I64(self.r#gen)),
            ("target_slot", Col::I64(self.target_slot)),
            ("reject", Col::I64(self.reject)),
            ("reason", Col::I64(self.reason)),
            ("intent_no", Col::I64(self.intent_no)),
            ("shadow", Col::U8(self.shadow)),
        ])
    }
}

/// `x × 10^exp` as f64 (cold reporting only).
fn scaled(x: i128, exp: i32) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let v = x as f64;
    v * 10f64.powi(exp)
}

/// One order of the simulated accounts, keyed by `(account, slot, gen)`.
#[derive(Clone, Copy, Debug, Default)]
struct Ord {
    account: u8,
    inst: u32,
    side: u8,
    px: i64,
    qty: i64,
    t_dec: i64,
    t_ack: i64,
    t_cxl_dec: i64,
    t_done: i64,
    first_fill: i64,
    cum: i64,
    n_fills: i64,
    last: u8,
    reject_code: i64,
}

/// Fills, orders and windows of the simulated accounts.
fn order_tables(rd: &RefData, cap: &[Captured], obs: &[Captured]) -> (Table, Table, Table) {
    use std::collections::BTreeMap;
    let mut orders: BTreeMap<(u8, u32, u32), Ord> = BTreeMap::new();
    for c in cap.iter().filter(|c| c.kind == EventKind::ORDER_DECISION) {
        let Some(d) = read_body::<OrderDecision>(&c.body) else {
            continue;
        };
        if d.reject != 0 || c.flags.0 & RecordFlags::SHADOW.0 != 0 {
            continue;
        }
        match d.action {
            OrderAction::New => {
                orders.insert(
                    (d.account.0, d.slot.0, d.r#gen),
                    Ord {
                        account: d.account.0,
                        inst: d.inst.0,
                        side: d.side as u8,
                        px: d.px.0,
                        qty: d.qty.0,
                        t_dec: c.utc_ns,
                        ..Ord::default()
                    },
                );
            }
            OrderAction::Cancel => {
                if let Some(o) = orders.get_mut(&(d.account.0, d.target_slot.0, d.r#gen))
                    && o.t_cxl_dec == 0
                {
                    o.t_cxl_dec = c.utc_ns;
                }
            }
            _ => {}
        }
    }
    // Fills.
    let (mut f_utc, mut f_acc, mut f_inst, mut f_side, mut f_px, mut f_qty, mut f_fee, mut f_slot, mut f_gen) = (
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    // Windows: per (account, inst) running totals closed by each delivery.
    #[derive(Default, Clone, Copy)]
    struct Win {
        bought: i64,
        cost: i128,
        sold: i64,
        proceeds: i128,
        fees: i128,
        n_fills: i64,
    }
    let mut open: BTreeMap<(u8, u32), Win> = BTreeMap::new();
    let (mut w_acc, mut w_inst, mut w_end, mut w_bought, mut w_cost, mut w_sold, mut w_proc, mut w_fees) =
        (vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    let (mut w_deliv, mut w_payout, mut w_pnl, mut w_nf) = (vec![], vec![], vec![], vec![]);
    let mut close = |key: (u8, u32), w: Win, end: i64, deliv: i64, payout: i64| {
        let i = rd.inst(InstId(key.1));
        let e = i32::from(i.px_exp) + i32::from(i.qty_exp);
        let value = i128::from(deliv) * i128::from(payout);
        w_acc.push(key.0);
        w_inst.push(i64::from(key.1));
        w_end.push(end);
        w_bought.push(w.bought);
        w_cost.push(scaled(w.cost, e));
        w_sold.push(w.sold);
        w_proc.push(scaled(w.proceeds, e));
        w_fees.push(scaled(w.fees, i32::from(rd.asset(i.quote).exp.qty_exp)));
        w_deliv.push(deliv);
        w_payout.push(payout);
        w_pnl.push(
            scaled(value + w.proceeds - w.cost, e) - scaled(w.fees, i32::from(rd.asset(i.quote).exp.qty_exp)),
        );
        w_nf.push(w.n_fills);
    };
    for c in obs {
        match c.kind {
            EventKind::ORDER_EVENT => {
                let Some(e) = read_body::<OrderEvent>(&c.body) else {
                    continue;
                };
                let key = (e.account.0, e.slot.0, e.r#gen);
                let o = orders.entry(key).or_default();
                o.last = e.what as u8;
                match e.what {
                    OrderEventKind::Ack if o.t_ack == 0 => o.t_ack = c.utc_ns,
                    OrderEventKind::PartialFill | OrderEventKind::Fill => {
                        if o.first_fill == 0 {
                            o.first_fill = c.utc_ns;
                        }
                        o.cum = e.cum_filled.0;
                        o.n_fills += 1;
                        if e.what == OrderEventKind::Fill {
                            o.t_done = c.utc_ns;
                        }
                        let side = if o.side == Side::Sell as u8 {
                            Side::Sell
                        } else {
                            Side::Buy
                        };
                        f_utc.push(c.utc_ns);
                        f_acc.push(e.account.0);
                        f_inst.push(i64::from(e.inst.0));
                        f_side.push(side as u8);
                        f_px.push(e.px.0);
                        f_qty.push(e.qty.0);
                        f_fee.push(e.fee.0);
                        f_slot.push(i64::from(e.slot.0));
                        f_gen.push(i64::from(e.r#gen));
                        let w = open.entry((e.account.0, e.inst.0)).or_default();
                        let n = i128::from(e.px.0) * i128::from(e.qty.0);
                        if side == Side::Buy {
                            w.bought += e.qty.0;
                            w.cost += n;
                        } else {
                            w.sold += e.qty.0;
                            w.proceeds += n;
                        }
                        w.fees += i128::from(e.fee.0);
                        w.n_fills += 1;
                    }
                    OrderEventKind::CancelAck | OrderEventKind::Expired => o.t_done = c.utc_ns,
                    OrderEventKind::Reject => {
                        o.t_done = c.utc_ns;
                        o.reject_code = i64::from(e.reject_code);
                    }
                    _ => {}
                }
            }
            EventKind::EXTERNAL_FILL => {
                let Some(x) = read_body::<ExternalFill>(&c.body) else {
                    continue;
                };
                let key = (x.account.0, x.inst.0);
                let w = open.remove(&key).unwrap_or_default();
                close(key, w, x.exch_order_id as i64, x.qty.0, x.px.0);
            }
            _ => {}
        }
    }
    for (key, w) in open {
        close(key, w, -1, 0, 0);
    }
    let fills = Table::new(vec![
        ("utc_ns", Col::I64(f_utc)),
        ("account", Col::U8(f_acc)),
        ("inst", Col::I64(f_inst)),
        ("side", Col::U8(f_side)),
        ("px", Col::I64(f_px)),
        ("qty", Col::I64(f_qty)),
        ("fee", Col::I64(f_fee)),
        ("slot", Col::I64(f_slot)),
        ("gen", Col::I64(f_gen)),
    ]);
    let mut oc: [Vec<i64>; 12] = Default::default();
    let (mut o_acc, mut o_side, mut o_last) = (vec![], vec![], vec![]);
    for ((_, slot, g), o) in &orders {
        o_acc.push(o.account);
        o_side.push(o.side);
        o_last.push(o.last);
        for (k, v) in [
            i64::from(*slot),
            i64::from(*g),
            i64::from(o.inst),
            o.px,
            o.qty,
            o.t_dec,
            o.t_ack,
            o.t_cxl_dec,
            o.t_done,
            o.first_fill,
            o.cum,
            o.reject_code,
        ]
        .into_iter()
        .enumerate()
        {
            oc[k].push(v);
        }
    }
    let [slot, g, inst, px, qty, t_dec, t_ack, t_cxl, t_done, ff, cum, rej] = oc;
    let orders = Table::new(vec![
        ("account", Col::U8(o_acc)),
        ("slot", Col::I64(slot)),
        ("gen", Col::I64(g)),
        ("inst", Col::I64(inst)),
        ("side", Col::U8(o_side)),
        ("px", Col::I64(px)),
        ("qty", Col::I64(qty)),
        ("t_dec", Col::I64(t_dec)),
        ("t_ack", Col::I64(t_ack)),
        ("t_cxl_dec", Col::I64(t_cxl)),
        ("t_done", Col::I64(t_done)),
        ("first_fill", Col::I64(ff)),
        ("cum", Col::I64(cum)),
        ("last_event", Col::U8(o_last)),
        ("reject_code", Col::I64(rej)),
    ]);
    let windows = Table::new(vec![
        ("account", Col::U8(w_acc)),
        ("inst", Col::I64(w_inst)),
        ("end_s", Col::I64(w_end)),
        ("bought", Col::I64(w_bought)),
        ("cost", Col::F64(w_cost)),
        ("sold", Col::I64(w_sold)),
        ("proceeds", Col::F64(w_proc)),
        ("fees", Col::F64(w_fees)),
        ("delivered", Col::I64(w_deliv)),
        ("payout_px", Col::I64(w_payout)),
        ("pnl", Col::F64(w_pnl)),
        ("n_fills", Col::I64(w_nf)),
    ]);
    (fills, orders, windows)
}

/// Prepends `variant` and `seed` columns.
pub fn tag(t: &mut Table, variant: &str, seed: u64) {
    let n = t.rows();
    t.cols.insert(0, ("seed", Col::U64(vec![seed; n])));
    t.cols
        .insert(0, ("variant", Col::Str(vec![variant.to_string(); n])));
}
