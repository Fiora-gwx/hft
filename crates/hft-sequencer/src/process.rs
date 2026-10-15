//! `process` (README "每条 Input 的处理顺序"): append first, per-kind engine handling, strategy
//! deliveries, executor advance, the second delivery, outputs appended in production order with
//! `decided_tsc`, `EVENT_END`, publish, then the ring pushes.

use hft_event::{
    AccountConfigReportHdr, AdviceHdr, BalanceReport, Bbo, ConnRole, ConnState, ConnStateKind, EngineClock,
    EventKind, ExternalFill, Funding, FundingSettle, IndexPrice, L2BatchHdr, L2SnapHdr, Level, Liquidation,
    MarginReportHdr, MarkPrice, Note, NoteLevel, OrderEvent, PositionReport, RecordFlags, RecordHeader,
    SendReport, Timer, Trade,
};
use hft_ledger::PnlDelta;
use hft_refdata::FundingSettleKind;
use hft_risk::mode::{GateEvent, ModeInput};
use hft_risk::{EquitySource, EquityView, MarginSample, RiskTimer};
use hft_rt::latency_trace::{self as lt, Context as TraceContext, Stage as TraceStage};
use hft_strategy_sdk::{ExternalFillView, LiquidationView, StrategySet, TradeView};
use hft_types::{AccountId, GROUP_NONE, GroupId, InstId, MAX_BBO_CONN_SLOTS, Px, Qty, Seq, Tsc, VenueId};
use zerocopy::IntoBytes;

use crate::Driver;
use crate::Sequencer;
use crate::codes::note;
use crate::engine::{Emit, Engine};
use crate::journal::{JournalSink, role_of};
use crate::market::{BBO_RESYNC, BBO_SW_TS};
use crate::notify::Notif;
use crate::strategies::{Direct, Instances, MarketEv};
use crate::types::{FatalReason, InputSlot, OutRecord};

type R<T> = Result<T, FatalReason>;

/// What the per-kind handler leaves for the delivery step.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct Pending<'a> {
    pub market: Option<MarketEv>,
    pub direct: Option<(usize, Direct<'a>, bool)>,
}

/// Processes the Input in `seq.slot`.
pub(crate) fn process<D: Driver<S>, S: StrategySet>(seq: &mut Sequencer<D, S>) -> R<()> {
    let Sequencer {
        e, inst, drv, slot, ..
    } = seq;
    process_parts(e, inst, drv, slot)
}

pub(crate) fn process_parts<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &InputSlot,
) -> R<()> {
    let _trace_scope = lt::scope(TraceContext {
        event_hash: if lt::enabled() {
            lt::fingerprint(slot.body())
        } else {
            0
        },
        kind_inst: u64::from(slot.kind.0),
        venue_conn: if slot.kind == EventKind::SEND_REPORT {
            crate::types::read_body::<SendReport>(slot.body())
                .map_or(0, |r| u64::from(r.venue.0) | (u64::from(r.conn.0) << 8))
        } else if (0x100..0x300).contains(&slot.kind.0) {
            slot.body().first().map_or(0, |v| u64::from(*v))
        } else {
            0
        },
        ..TraceContext::default()
    });
    lt::stamp(TraceStage::ProcessStart, [slot.tsc.0, 0, 0, 0]);
    bench_stage!(e, JournalInput);
    e.out.clear();
    e.notify.begin();
    // Any non-BBO input may change connections, depth or group membership. Preserve the
    // existing gate evaluation boundary; repeated BBOs only change readiness on first quote
    // or membership growth. A derived cache is always invalid after restore.
    e.feeds_dirty |= slot.kind != EventKind::BBO;
    e.gpos_bbo = None;
    e.trace.rec("append.input", u64::from(slot.kind.0), 0);
    // 1. Record first.
    let s = drv
        .journal()
        .append_var(slot.kind, slot.flags, slot.tsc, &[slot.body()])
        .map_err(|_| FatalReason::JournalFull)?;
    lt::set_context(TraceContext {
        input_seq: s.0,
        ..lt::context()
    });
    lt::stamp(TraceStage::JournalInput, [0; 4]);
    bench_stage!(e, DispatchClock);
    // Meta never reaches the decision clock or callbacks. Calibration is cold-reader data.
    if crate::journal::role_of(slot.kind, slot.body()) == hft_event::Role::Meta {
        if slot.kind == EventKind::CLOCK_STATUS
            && let Some(c) = slot.read::<hft_event::ClockStatus>()
        {
            e.rec.clock_cal = crate::snapshot::ClockCalRow::recorded(c, s.0);
        }
        return Ok(());
    }
    e.cur_seq = s.0;
    e.cur_kind = slot.kind;
    e.inputs_since_keyframe += 1;
    // Engine time of this Input (EngineClock sets its own base first).
    if slot.kind == EventKind::ENGINE_CLOCK {
        let Some(b) = slot.read::<EngineClock>() else {
            return Err(FatalReason::ClockRange);
        };
        let row = crate::clock::EngineClockRow {
            base_engine_ns: b.base_engine_ns,
            base_tsc: slot.tsc.0,
            tsc_hz_milli: b.tsc_hz_milli,
            clock_seq: s.0,
        };
        e.clock = crate::clock::EngineClock::new(row).map_err(|_| FatalReason::ClockRange)?;
    }
    if !e.clock.is_set() {
        // Before the first EngineClock (replay of a pre-clock record): use the anchor at 0.
        e.now = e.now_of(slot.tsc, s.0);
    } else {
        let _ = e.clock.ens_of(slot.tsc).map_err(|_| FatalReason::ClockRange)?;
        e.now = e.now_of(slot.tsc, s.0);
    }
    e.trig = trig_of(slot, s.0);
    #[cfg(feature = "testkit")]
    {
        e.oms.audit_begin();
        e.audit.begin();
        e.exec.audit_begin();
        drv.observe_input(e, slot, true);
    }
    let mut p = Pending::default();
    // 2./3. per kind.
    dispatch(e, inst, drv, slot, &mut p)?;
    // 4./5. first delivery round.
    let mut q = core::mem::take(&mut e.notify.delivery);
    e.notify.take_queue(&mut q);
    crate::strategies::deliver_round(e, inst, drv, p.market, &q, p.direct)?;
    bench_stage!(e, Exec);
    // 6. executor advance.
    lt::stamp(TraceStage::ExecStart, [0; 4]);
    crate::exec_glue::advance(e, inst, drv)?;
    lt::stamp(TraceStage::ExecEnd, [0; 4]);
    bench_stage!(e, FinalChecks);
    // 8. second delivery round (notifications produced by Ctx application and advance).
    e.notify.take_queue(&mut q);
    if !q.is_empty() {
        crate::strategies::deliver_round(e, inst, drv, None, &q, None)?;
        // Notifications produced after the second round wait for NotifyContinue.
        e.notify.take_queue(&mut q);
        for r in q.drain(..) {
            e.notify.push_spill(r);
        }
    }
    q.clear();
    e.notify.delivery = q;
    bench_stage!(e, FinalChecks);
    // Storage recheck while a StorageCritical drain is deferred.
    if e.risk.mode.drain_deferred() {
        e.trace.rec("apply.StorageRecheck", 0, 0);
        crate::mode_glue::apply_mode(e, inst, drv, ModeInput::StorageRecheck)?;
    }
    crate::subs::position_changed(e, inst);
    crate::commands::drain_check(e, inst, drv)?;
    bench_stage!(e, Output);
    finish_event(e, drv, slot.tsc)?;
    lt::stamp(TraceStage::ProcessEnd, [0; 4]);
    #[cfg(feature = "testkit")]
    {
        drv.observe_input(e, slot, false);
        drv.observe_boundary(crate::audit::StateView { e, inst });
    }
    Ok(())
}

/// Steps 9–12: measure, append outputs, stamp, end_event, publish, push rings.
pub(crate) fn finish_event<D: Driver<S>, S: StrategySet>(e: &mut Engine, drv: &mut D, tsc: Tsc) -> R<()> {
    let measured = drv.measure();
    let n = e.out.recs.len();
    e.out.seqs.clear();
    for k in 0..n {
        let Some(r) = e.out.recs.get(k).copied() else {
            continue;
        };
        if r.kind == EventKind::ORDER_DECISION
            && let Some(b) = e.out.bytes.get_mut(r.off as usize + 8..r.off as usize + 16)
        {
            b.copy_from_slice(&measured.0.to_le_bytes());
        }
        if r.kind == EventKind::STATE_SNAPSHOT
            && let Some(body) = e.out.bytes.get_mut(r.off as usize..(r.off + r.len) as usize)
        {
            // Include Outputs buffered earlier in this same event, in journal order.
            crate::snapshot::set_out_hash(body, e.out_hash.digest());
        }
        let body = e.out.body(&r);
        let s = drv
            .journal()
            .append_var(r.kind, r.flags, tsc, &[body])
            .map_err(|_| FatalReason::JournalFull)?;
        if r.kind == EventKind::ORDER_DECISION
            && lt::enabled()
            && let Some(d) = crate::types::read_body::<hft_event::OrderDecision>(body)
        {
            let _scope = lt::scope(TraceContext {
                input_seq: d.trigger_seq,
                decision_seq: s.0,
                order: (u64::from(d.r#gen) << 32) | u64::from(d.slot.0),
                venue_conn: u64::from(d.venue.0),
                kind_inst: u64::from(d.inst.0) << 32,
                ..TraceContext::default()
            });
            lt::stamp(
                TraceStage::Decision,
                [
                    u64::from(d.intent_no),
                    u64::from(d.link_role),
                    u64::from(d.reason),
                    u64::from(d.reject),
                ],
            );
        }
        if role_of(r.kind, body) == hft_event::Role::Output {
            e.out_hash.add(r.kind, r.flags, &[body]);
        }
        if r.kind == EventKind::NOTE
            && let Some(nt) = crate::types::read_body::<Note>(body)
            && nt.code == crate::codes::seq_note(note::EPOCH_START)
        {
            e.boot_seq = s.0;
        }
        e.out.seqs.push(s.0);
    }
    for r in e.out.stamped.iter() {
        e.oms.stamp_decided(*r, measured);
    }
    drv.journal().end_event();
    e.last_ens = e.now.engine_ns;
    e.last_tsc = tsc.0;
    if !e.out.emits.is_empty() {
        drv.journal().publish();
        for em in e.out.emits.iter() {
            match em {
                Emit::Gw { ring, cmd, rec } => {
                    let mut c = *cmd;
                    if let Some(ix) = rec {
                        c.decision_seq = e.out.seqs.get(*ix as usize).copied().unwrap_or(0);
                        c.decided_tsc = measured;
                    }
                    e.trace.rec("emit.gw", c.decision_seq, c.op as u64);
                    drv.emit(&OutRecord::Out(*ring, &c));
                }
                Emit::Feed { venue, cmd } => {
                    e.trace.rec("emit.feed", u64::from(venue.0), 0);
                    drv.emit(&OutRecord::FeedCtl(*venue, cmd));
                }
                Emit::Pair { rings, cmds, recs } => {
                    let mut pair = [(rings[0], cmds[0]), (rings[1], cmds[1])];
                    for (k, (_, c)) in pair.iter_mut().enumerate() {
                        c.decision_seq = e.out.seqs.get(recs[k] as usize).copied().unwrap_or(0);
                        c.decided_tsc = measured;
                        e.trace.rec("emit.gw", c.decision_seq, c.op as u64);
                    }
                    let venues = [e.rd.inst(cmds[0].inst).venue, e.rd.inst(cmds[1].inst).venue];
                    drv.emit(&OutRecord::Pair(&pair, venues));
                }
            }
        }
    }
    #[cfg(feature = "testkit")]
    e.audit.finish(&e.out.seqs);
    e.out.clear();
    Ok(())
}

fn trig_of(slot: &InputSlot, seq: u64) -> hft_oms::TrigStamps {
    let mut t = hft_oms::TrigStamps {
        proc_tsc: slot.tsc,
        trigger_seq: seq,
        kind: slot.kind.0,
        ..hft_oms::TrigStamps::default()
    };
    // Every market body starts with MarketHdr.
    let is_market = matches!(
        slot.kind,
        EventKind::BBO
            | EventKind::L2_DELTA
            | EventKind::TRADE
            | EventKind::FUNDING
            | EventKind::MARK_PRICE
            | EventKind::INDEX_PRICE
            | EventKind::LIQUIDATION
            | EventKind::L2_BATCH
            | EventKind::L2_SNAPSHOT
    );
    if is_market && let Some(h) = crate::types::read_body_plain::<hft_event::MarketHdr>(slot.body()) {
        t.ex_ts_ns = h.ex_ts_ns;
        t.ex_send_ns = h.ex_send_ns;
        t.hw_rx_ns = h.hw_rx_ns;
        t.sw_rx_ns = h.sw_rx_ns;
        t.rx_tsc = Tsc(h.rx_tsc);
    }
    if slot.kind == EventKind::ORDER_EVENT
        && let Some(o) = slot.read::<OrderEvent>()
    {
        t.ex_ts_ns = o.ex_ts_ns;
        t.hw_rx_ns = o.hw_rx_ns;
        t.rx_tsc = Tsc(o.rx_tsc);
    }
    t
}

fn hdr_of(e: &Engine, slot: &InputSlot) -> RecordHeader {
    RecordHeader {
        len: 0,
        kind: slot.kind.0,
        flags: slot.flags.0,
        seq: e.cur_seq,
        tsc: slot.tsc.0,
    }
}

fn dispatch<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &'a InputSlot,
    p: &mut Pending<'a>,
) -> R<()> {
    match slot.kind {
        EventKind::ENGINE_CLOCK => engine_clock(e, inst, drv, slot),
        EventKind::BBO => bbo(e, inst, drv, slot, p),
        EventKind::L2_BATCH => l2_batch(e, inst, drv, slot, p),
        EventKind::L2_SNAPSHOT => l2_snapshot(e, inst, drv, slot, p),
        EventKind::TRADE => {
            if let Some(t) = slot.read::<Trade>() {
                if let Some(r) = e.market.row_mut(t.h.inst) {
                    r.trade_px = t.px;
                    r.trade_qty = t.qty;
                    r.trade_ts_ns = t.h.ex_ts_ns;
                }
                p.market = Some(MarketEv::Trade {
                    inst: t.h.inst,
                    t: TradeView {
                        px: t.px,
                        qty: t.qty,
                        side: t.taker_side,
                        ex_ts_ns: t.h.ex_ts_ns,
                    },
                });
            }
            Ok(())
        }
        EventKind::FUNDING => funding(e, inst, drv, slot, p),
        EventKind::MARK_PRICE => mark(e, inst, drv, slot, p),
        EventKind::INDEX_PRICE => {
            if let Some(m) = slot.read::<IndexPrice>() {
                if let Some(r) = e.market.row_mut(m.h.inst) {
                    r.index_px = m.px;
                }
                p.market = Some(MarketEv::Index {
                    inst: m.h.inst,
                    px: m.px,
                });
            }
            Ok(())
        }
        EventKind::LIQUIDATION => {
            if let Some(l) = slot.read::<Liquidation>() {
                p.market = Some(MarketEv::Liq {
                    inst: l.h.inst,
                    l: LiquidationView {
                        px: l.px,
                        qty: l.qty,
                        side: l.side,
                        ex_ts_ns: l.h.ex_ts_ns,
                    },
                });
            }
            Ok(())
        }
        EventKind::ORDER_EVENT => order_event(e, inst, drv, slot),
        EventKind::SEND_REPORT => {
            if let Some(r) = slot.read::<SendReport>() {
                let h = hdr_of(e, slot);
                let mut o = e.take_oms();
                e.trace.rec("oms.on_send_report", u64::from(r.slot.0), 0);
                e.oms.on_send_report(&r, &h, &mut o);
                touch_report_groups(e, &o);
                crate::intents::drain_oms(e, inst, drv, &mut o)?;
                e.put_oms(o);
                after_oms(e, inst, drv, r.account, r.venue)?;
            }
            Ok(())
        }
        EventKind::POSITION_REPORT => {
            if let Some(r) = slot.read::<PositionReport>() {
                let eng = e.ledger.pos(r.account, r.inst).qty;
                if eng != r.qty {
                    e.seq_note(
                        note::POSITION_DIFF,
                        NoteLevel::P1,
                        GroupId(0),
                        r.inst,
                        [
                            u64::from(r.venue.0) | (u64::from(r.account.0) << 8),
                            eng.0 as u64,
                            r.qty.0 as u64,
                            0,
                            0,
                            0,
                            0,
                            0,
                        ],
                    );
                }
            }
            Ok(())
        }
        EventKind::BALANCE_REPORT => {
            if let Some(b) = slot.read::<BalanceReport>() {
                let quiet = balance_quiet(e, b.account, b.venue);
                e.trace
                    .rec("ledger.on_balance", u64::from(b.account.0), u64::from(quiet));
                let _ = e.ledger.on_balance(&b, quiet, e.now.engine_ns);
                ledger_notes(e);
            }
            Ok(())
        }
        EventKind::FUNDING_SETTLE => funding_settle(e, inst, drv, slot),
        EventKind::EXTERNAL_FILL => {
            if let Some(x) = slot.read::<ExternalFill>() {
                external_fill(e, inst, drv, &x, false, false)?;
            }
            Ok(())
        }
        EventKind::MARGIN_REPORT => margin_report(e, inst, drv, slot),
        EventKind::ACCOUNT_CONFIG_REPORT => account_cfg(e, inst, drv, slot),
        EventKind::CONN_STATE => conn_state(e, inst, drv, slot),
        EventKind::RECONCILE_REPORT => crate::commands::reconcile(e, inst, drv, slot),
        EventKind::RECONCILE_BATCH => crate::commands::reconcile_batch(e, inst, drv, slot),
        EventKind::CONTROL_CMD => crate::commands::control(e, inst, drv, slot, p),
        EventKind::CONFIG_BLOB => crate::commands::config_blob(e, slot),
        EventKind::TIMER => timer(e, inst, drv, slot, p),
        EventKind::ADVICE => advice(e, inst, slot, p),
        _ => Ok(()),
    }
}

// ------------------------------------------------------------------------------- EngineClock

fn engine_clock<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    _slot: &InputSlot,
) -> R<()> {
    e.market.clear_run();
    if e.cmd.conns.physical {
        e.cmd.conns.clear_run();
    }
    e.cmd.previews.clear();
    let base = e.clock.row.base_engine_ns;
    e.trace.rec("ledger.on_run_start", base as u64, 0);
    e.ledger.on_run_start(base);
    e.trace.rec("risk.set_run_start", base as u64, 0);
    e.risk.set_run_start(base);
    e.seq_note(
        note::EPOCH_START,
        NoteLevel::Info,
        GroupId(0),
        InstId(0),
        [u64::from(e.epoch), base as u64, e.cur_seq, 0, 0, 0, 0, 0],
    );
    e.market.set_rx_from(e.cur_seq);
    register_periodic(e)?;
    if e.set_epoch_due {
        e.set_epoch_due = false;
        for (v, accounts) in crate::engine::live_gw_venues(&e.rd) {
            for a in accounts {
                let mut c = hft_event::GwCmd::set_epoch(e.epoch as u16, true, e.prev_epoch as u16);
                c.account = a;
                e.out.emits.push(Emit::Gw {
                    ring: crate::types::OutRing::Venue(v),
                    cmd: c,
                    rec: None,
                });
            }
        }
        crate::recover::mirror_restore_all(e);
    }
    crate::subs::resend_all(e, inst);
    let _ = drv;
    Ok(())
}

/// Registers the periodic system timers of this run (replacing same-key entries).
fn register_periodic(e: &mut Engine) -> R<()> {
    let ms = e.now.tsc_per_ms.max(1);
    let t0 = e.now.tsc.0;
    // OmsSweep keeps an earlier restored deadline: restarts must not postpone the sweep.
    let sweep = t0 + 1_000 * ms;
    if e.heap.find_key(3, 2, 0).is_none_or(|x| x.deadline > sweep) {
        e.heap.remove_key(3, 2, 0);
        e.heap
            .push(crate::timers::TimerEntry {
                deadline: sweep,
                owner: 3,
                timer_id: 2,
                group: 0,
            })
            .map_err(|_| FatalReason::TimerOverflow)?;
    }
    for (v, accounts) in crate::engine::live_gw_venues(&e.rd) {
        let caps = e.rd.caps(v);
        if caps.dms != hft_refdata::DeadManCap::None {
            for a in accounts.iter() {
                let id = 4 | (u32::from(v.0) << 8) | (u32::from(a.0) << 16);
                e.heap.remove_key(1, id, 0);
                e.heap
                    .push(crate::timers::TimerEntry {
                        deadline: t0 + u64::from(caps.dms_refresh_ms.max(1)) * ms,
                        owner: 1,
                        timer_id: id,
                        group: 0,
                    })
                    .map_err(|_| FatalReason::TimerOverflow)?;
            }
        }
        let rot = 5 | (u32::from(v.0) << 8);
        e.heap.remove_key(1, rot, 0);
        let age = u64::from(e.rd.venue(v).ws_max_conn_age_s).max(1);
        e.heap
            .push(crate::timers::TimerEntry {
                deadline: t0 + age.saturating_mul(1_000).saturating_mul(ms),
                owner: 1,
                timer_id: rot,
                group: 0,
            })
            .map_err(|_| FatalReason::TimerOverflow)?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------- market

fn bbo<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &'a InputSlot,
    p: &mut Pending<'a>,
) -> R<()> {
    bench_stage!(e, Book);
    let Some(b) = slot.read::<Bbo>() else {
        return Ok(());
    };
    let i = b.h.inst;
    e.feeds_dirty |= e.books.top(i).seq == 0;
    e.gpos_bbo = Some(i);
    let out = e.books.top_mut(i).apply(&b, Seq(e.cur_seq));
    e.trace.rec("top.apply", u64::from(i.0), u64::from(out.bits()));
    let applied = !(out.contains(hft_book::TopOutcome::DUPLICATE)
        || out.contains(hft_book::TopOutcome::OLDER)
        || out.contains(hft_book::TopOutcome::INVALID));
    // Liveness.
    let venue = b.h.venue;
    let conn = b.h.conn.0;
    if conn < MAX_BBO_CONN_SLOTS {
        if let Some(c) = e.market.conn_mut(venue, conn) {
            c.live_tsc = c.live_tsc.max(b.h.rx_tsc);
            c.used = true;
        }
        if let Some(r) = e.market.row_mut(i) {
            e.feeds_dirty |= r.bbo_member_mask & (1u8 << conn) == 0;
            r.bbo_member_mask |= 1u8 << conn;
            if applied {
                r.bbo_flags &= !BBO_RESYNC;
                r.bbo_hw_rx_ns = b.h.hw_rx_ns;
                if slot.flags.contains(RecordFlags::SW_TS) {
                    r.bbo_flags |= BBO_SW_TS;
                } else {
                    r.bbo_flags &= !BBO_SW_TS;
                }
            }
        }
        e.trace.rec("liveness", u64::from(i.0), 0);
    }
    if !applied {
        return Ok(());
    }
    bench_stage!(e, Ledger);
    e.trace.rec("ledger.on_ref_px", u64::from(i.0), 0);
    e.ledger.on_ref_px(i, b.bid_px, b.ask_px, e.now.engine_ns);
    ledger_notes(e);
    upnl_pnl(e, inst, drv)?;
    e.trace.rec("exec.touch_inst", u64::from(i.0), 0);
    e.exec.touch_inst(i);
    bench_stage!(e, FeedsGate);
    if e.feeds_dirty {
        feeds_gate(e, inst, drv)?;
    }
    p.market = Some(MarketEv::Bbo {
        inst: i,
        b: e.market.bbo_view(&e.books, i),
    });
    Ok(())
}

fn after_book<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    i: InstId,
) -> R<()> {
    let (bid, ask) = {
        let b = e.books.l2(i);
        (
            b.and_then(|b| b.best_bid()).map_or(Px(0), |l| l.px),
            b.and_then(|b| b.best_ask()).map_or(Px(0), |l| l.px),
        )
    };
    if bid.0 > 0 && ask.0 > 0 {
        e.trace.rec("ledger.on_ref_px", u64::from(i.0), 1);
        e.ledger.on_ref_px(i, bid, ask, e.now.engine_ns);
        ledger_notes(e);
        upnl_pnl(e, inst, drv)?;
    }
    e.trace.rec("exec.touch_inst", u64::from(i.0), 1);
    e.exec.touch_inst(i);
    feeds_gate(e, inst, drv)
}

fn l2_batch<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &'a InputSlot,
    p: &mut Pending<'a>,
) -> R<()> {
    let body = slot.body();
    let Some(h) = crate::types::read_body::<L2BatchHdr>(body) else {
        return Ok(());
    };
    let n = usize::from(h.n_bids) + usize::from(h.n_asks);
    let levels = level_slice(body.get(core::mem::size_of::<L2BatchHdr>()..).unwrap_or(&[]), n);
    let i = h.h.inst;
    if let Some(b) = e.books.l2_mut(i) {
        b.apply_batch(&h, &levels);
        e.trace.rec("book.apply_batch", u64::from(i.0), n as u64);
    }
    after_book(e, inst, drv, i)?;
    p.market = Some(MarketEv::Book { inst: i });
    Ok(())
}

fn l2_snapshot<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &'a InputSlot,
    p: &mut Pending<'a>,
) -> R<()> {
    let body = slot.body();
    let Some(h) = crate::types::read_body::<L2SnapHdr>(body) else {
        return Ok(());
    };
    let n = usize::from(h.n_bids) + usize::from(h.n_asks);
    let levels = level_slice(body.get(core::mem::size_of::<L2SnapHdr>()..).unwrap_or(&[]), n);
    let i = h.h.inst;
    if let Some(b) = e.books.l2_mut(i) {
        b.apply_snapshot(&h, &levels);
        e.trace
            .rec("book.apply_snapshot", u64::from(i.0), u64::from(h.origin));
    }
    if h.origin == L2SnapHdr::TOP_N {
        after_book(e, inst, drv, i)?;
        p.market = Some(MarketEv::Book { inst: i });
    }
    Ok(())
}

/// A boot-block `L2Snapshot{origin = Keyframe}` read while starting from a snapshot: levels only.
pub(crate) fn apply_l2_keyframe(e: &mut Engine, body: &[u8]) {
    let Some(h) = crate::types::read_body::<L2SnapHdr>(body) else {
        return;
    };
    let n = usize::from(h.n_bids) + usize::from(h.n_asks);
    let levels = level_slice(body.get(core::mem::size_of::<L2SnapHdr>()..).unwrap_or(&[]), n);
    if let Some(b) = e.books.l2_mut(h.h.inst) {
        b.apply_snapshot(&h, &levels);
    }
}

fn level_slice(b: &[u8], n: usize) -> Vec<Level> {
    let mut v = Vec::with_capacity(n);
    for k in 0..n {
        if let Some(x) = b.get(16 * k..16 * k + 16)
            && let Some(l) = crate::types::read_body_plain::<Level>(x)
        {
            v.push(l);
        }
    }
    v
}

fn funding<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &'a InputSlot,
    p: &mut Pending<'a>,
) -> R<()> {
    let Some(f) = slot.read::<Funding>() else {
        return Ok(());
    };
    let i = f.h.inst;
    // Synthesis when the next settle time moves past an unsettled one.
    let (old_next, cut) = e
        .market
        .row(i)
        .map_or((0, 0), |r| (r.next_funding_ns, r.settled_cut_ns));
    if old_next != 0 && f.next_funding_ns > old_next && cut < old_next {
        let v = e.rd.inst(i).venue;
        if needs_synth(e, v) {
            crate::commands::synth_funding(e, inst, drv, v, old_next)?;
        }
    }
    if f.interval_s != 0 {
        let prev = e.market.row(i).map_or(0, |r| r.interval_s);
        if let Some(r) = e.market.row_mut(i) {
            if prev != f.interval_s {
                r.prev_interval_s = if prev == 0 { f.interval_s } else { prev };
            } else if r.prev_interval_s == 0 {
                r.prev_interval_s = prev;
            }
            r.interval_s = f.interval_s;
        }
        e.trace.rec(
            "ledger.on_funding_interval",
            u64::from(i.0),
            u64::from(f.interval_s),
        );
        e.ledger.on_funding_interval(i, f.interval_s);
    }
    if let Some(r) = e.market.row_mut(i) {
        r.funding_rate = f.rate;
        r.next_funding_ns = f.next_funding_ns;
    }
    if let Some(m) = e.market.mem.get_mut(i.0 as usize) {
        m.funding_rx_tsc = f.h.rx_tsc;
        m.funding_seq = e.cur_seq;
    }
    e.market.refresh_funding(i);
    p.market = Some(MarketEv::Funding { inst: i });
    Ok(())
}

pub(crate) fn needs_synth(e: &Engine, v: VenueId) -> bool {
    matches!(
        e.rd.caps(v).funding_settle,
        FundingSettleKind::BalanceOnlyPush | FundingSettleKind::RateOnly
    )
}

fn mark<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &'a InputSlot,
    p: &mut Pending<'a>,
) -> R<()> {
    let Some(m) = slot.read::<MarkPrice>() else {
        return Ok(());
    };
    let i = m.h.inst;
    if let Some(r) = e.market.row_mut(i) {
        r.mark_px = m.px;
        r.mark_ts_ns = m.h.ex_ts_ns;
        r.mark_ens = e.now.engine_ns;
    }
    let mut touched = hft_ledger::FixedVec::<(AccountId, VenueId), 4>::new();
    e.trace
        .rec("ledger.on_mark", u64::from(i.0), e.now.engine_ns as u64);
    e.ledger.on_mark(i, m.px, e.now.engine_ns, &mut touched);
    ledger_notes(e);
    for (a, v) in touched.iter().copied() {
        margin_to_risk(e, inst, drv, a, v, false)?;
    }
    upnl_pnl(e, inst, drv)?;
    e.trace.rec("exec.touch_inst", u64::from(i.0), 2);
    e.exec.touch_inst(i);
    p.market = Some(MarketEv::Mark { inst: i, px: m.px });
    Ok(())
}

/// Projects the ledger `MarginState` to a `MarginSample` and runs `risk.on_margin`.
fn margin_to_risk<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    v: VenueId,
    truncated: bool,
) -> R<()> {
    let m = e.ledger.margin(a, v);
    let mut mark_ens = m.mark_oldest_ens;
    for i in e.rd.insts_of_venue(v) {
        if e.oms.inflight().open_buy(a, *i).0 != 0 || e.oms.inflight().open_sell(a, *i).0 != 0 {
            mark_ens = mark_ens.min(e.market.row(*i).map_or(0, |r| r.mark_ens));
        }
    }
    let s = MarginSample {
        rep_liq_dist: m.rep_liq_dist,
        est_liq_dist: m.est_liq_dist,
        reported_ratio: m.rep_ratio,
        est_ratio: m.est_ratio,
        report_ens: m.rep_ens,
        mark_ens,
        has_report: m.has_report,
        truncated,
    };
    let mut out = e.take_margin();
    e.trace.rec("risk.on_margin", u64::from(a.0), u64::from(v.0));
    e.risk.on_margin(a, v, &s, &e.now, &mut out);
    margin_out(e, inst, drv, &out)?;
    e.put_margin(out);
    Ok(())
}

/// Executes a `MarginOut`: mode → events → force → timers → notes.
fn margin_out<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    out: &hft_risk::MarginOut,
) -> R<()> {
    let modes: Vec<ModeInput<'static>> = out.mode.iter().copied().collect();
    for m in modes {
        crate::mode_glue::apply_mode(e, inst, drv, m)?;
    }
    for (a, ev) in out.events.iter().copied().collect::<Vec<_>>() {
        crate::strategies::queue_for_account(e, inst, a, Notif::Risk { a, e: ev });
        e.trace.rec("exec.on_risk", u64::from(a.0), 1);
        e.exec.on_risk(a, &ev, e.now);
    }
    if let Some(f) = out.force {
        crate::exec_glue::force(e, inst, drv, f.account, f.venue, f.frac)?;
    }
    for (t, ens) in out.timers.iter().copied().collect::<Vec<_>>() {
        let id = t.timer_id();
        e.heap.remove_key(4, id, 0);
        e.heap
            .push(crate::timers::TimerEntry {
                deadline: e.now.tsc_of(ens).0,
                owner: 4,
                timer_id: id,
                group: 0,
            })
            .map_err(|_| FatalReason::TimerOverflow)?;
    }
    for n in out.notes.iter().copied().collect::<Vec<_>>() {
        e.out.push(RecordFlags(0), &n);
    }
    Ok(())
}

// ------------------------------------------------------------------------------- exec reports

fn order_event<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &InputSlot,
) -> R<()> {
    let Some(ev) = slot.read::<OrderEvent>() else {
        return Ok(());
    };
    let h = hdr_of(e, slot);
    // A smoke order has no owner and so no `OrderUpdate` (hft-oms): its Unknown → Live
    // convergence is seen here, for the standing cancel obligation (B-10).
    let r = hft_strategy_sdk::OrderRef::new(ev.venue, ev.account, ev.slot.0, ev.r#gen);
    let smoke_unknown = e.oms.slot(r).is_some_and(|s| {
        s.state == hft_oms::OrderState::Unknown && s.oflags & hft_event::OrderFlags::SMOKE.0 != 0
    });
    let mut o = e.take_oms();
    e.trace
        .rec("oms.on_order_event", u64::from(ev.slot.0), ev.what as u64);
    e.oms.on_order_event(&ev, &h, &mut o);
    touch_report_groups(e, &o);
    crate::intents::drain_oms(e, inst, drv, &mut o)?;
    e.put_oms(o);
    if smoke_unknown
        && e.oms
            .slot(r)
            .is_some_and(|s| s.state == hft_oms::OrderState::Live)
    {
        crate::mode_glue::cancel_obligation_on_live(e, inst, drv, ev.account, ev.venue)?;
    }
    after_oms(e, inst, drv, ev.account, ev.venue)?;
    Ok(())
}

/// Terminal/non-fill reports also change pending and in-flight legs. Refresh after the
/// complete OMS output (including execution-layer callbacks), as for an ordinary fill.
fn touch_report_groups(e: &mut Engine, out: &hft_oms::OmsOut) {
    for update in out.updates.iter() {
        touch_legs(e, update.r.account, update.group);
    }
}

/// After an OMS report: `risk.on_legs` for the touched groups, `CellFlat`, the Unknown gate.
pub(crate) fn after_oms<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    v: VenueId,
) -> R<()> {
    // Callbacks can touch more groups. Swap in an empty, startup-sized buffer so they
    // retain the same next-pass semantics without allocating a copy of this batch.
    let mut groups = core::mem::take(&mut e.cmd.legs_spare);
    core::mem::swap(&mut groups, &mut e.cmd.legs_touched);
    let result = (|| {
        for g in &groups {
            crate::mode_glue::on_legs(e, inst, drv, a, *g)?;
            let cell = crate::strategies::cell(e.rd.account(a).bound_instance, a, *g);
            let flat = {
                let ad = crate::views::ModeAdapter {
                    oms: &e.oms,
                    ledger: &e.ledger,
                    exec: &e.exec,
                };
                hft_risk::ModeView::cell_flat(&ad, cell)
            };
            crate::mode_glue::apply_mode(e, inst, drv, ModeInput::CellFlat { cell, flat })?;
        }
        unknown_gate(e, inst, drv, a)
    })();
    groups.clear();
    e.cmd.legs_spare = groups;
    result?;
    refresh_flat_margin(e, inst, drv, a, v)
}

fn refresh_flat_margin<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    v: VenueId,
) -> R<()> {
    // The last fill/cancel can remove the final mark-dependent exposure. Refresh that fact
    // before the stale timer evaluates its cached sample; a live order still needs its mark.
    if v.market() == Some(hft_types::MarketKind::LinearPerp)
        && e.ledger.margin(a, v).mark_oldest_ens == i64::MAX
        && e.rd
            .insts_of_venue(v)
            .iter()
            .all(|i| e.oms.inflight().open_buy(a, *i).0 == 0 && e.oms.inflight().open_sell(a, *i).0 == 0)
    {
        margin_to_risk(e, inst, drv, a, v, false)?;
    }
    Ok(())
}

/// Marks a group whose legs changed in this Input.
pub(crate) fn touch_legs(e: &mut Engine, _a: AccountId, g: GroupId) {
    if g.0 != 0 && !e.cmd.legs_touched.contains(&g) {
        debug_assert!(e.cmd.legs_touched.len() < e.cmd.legs_touched.capacity());
        e.cmd.legs_touched.push(g);
    }
}

/// `risk.on_pnl` for one delta (day P&L of the account).
pub(crate) fn pnl<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    d: PnlDelta,
) -> R<()> {
    account_pnl(e, inst, drv, d.account)
}

/// `risk.on_pnl` of every account whose unrealized the last `ledger.on_ref_px`/`on_mark` moved
/// (README "process": the day PnL includes the unrealized, hft-risk "当日亏损").
pub(crate) fn upnl_pnl<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
) -> R<()> {
    while let Some(a) = e.ledger.take_upnl_changed() {
        account_pnl(e, inst, drv, a)?;
    }
    Ok(())
}

/// `risk.on_pnl` with account `a`'s current day PnL (realized + funding + fees + unrealized).
fn account_pnl<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
) -> R<()> {
    if a.0 == 0 {
        return Ok(());
    }
    let day = e.ledger.account(a).day.net();
    let eq = e.ledger.equity(a);
    e.trace.rec("risk.on_pnl", u64::from(a.0), 0);
    if let Some(inp) = e.risk.on_pnl(
        a,
        day,
        EquityView {
            total: eq.total,
            valid: eq.valid,
        },
    ) {
        crate::mode_glue::apply_mode(e, inst, drv, inp)?;
    }
    Ok(())
}

/// Writes the ledger's pending Notes into the current Input's output.
pub(crate) fn ledger_notes(e: &mut Engine) {
    for n in e.ledger.notes() {
        e.out.push(RecordFlags(0), n);
    }
    e.ledger.clear_notes();
}

/// `ExternalFill` handling (README "process" ExternalFill row); also the OMS foreign fills and
/// `AUTO_EXTERNAL`. `oms_foreign`: `x` is the OMS's own `ForeignFill`, whose cumulative quantity
/// the OMS already tracks per `exch_order_id` (`foreign` table): it is not registered as
/// `ext_held` (that would deduct it again from the next report of the same order).
pub(crate) fn external_fill<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    x: &ExternalFill,
    auto_recon: bool,
    oms_foreign: bool,
) -> R<()> {
    let a = x.account;
    let settled =
        e.rd.caps(x.venue)
            .flags
            .contains(hft_refdata::CapFlags::PERP_CASH)
            || e.rd
                .caps(e.rd.inst(x.inst).venue)
                .flags
                .contains(hft_refdata::CapFlags::PERP_CASH);
    if settled && auto_recon && x.cash_qty != x.qty {
        return Err(FatalReason::UnsupportedExternalCash);
    }
    if settled && !oms_foreign && e.oms.external_cash_guard(x).map_err(FatalReason::OmsCash)? {
        return if auto_recon {
            Err(FatalReason::UnsupportedExternalCash)
        } else {
            Ok(())
        };
    }
    let before = e.ledger.pos(a, x.inst).qty.0 != 0;
    e.trace
        .rec("ledger.apply_external", u64::from(a.0), u64::from(auto_recon));
    let r = if settled && auto_recon {
        e.ledger
            .apply_reconciled_external(x, e.now.engine_ns)
            .map_err(FatalReason::Settlement)?
    } else if settled {
        e.ledger
            .apply_settled_external(x, e.now.engine_ns)
            .map_err(FatalReason::Settlement)?
    } else {
        e.ledger.apply_external(x, auto_recon, e.now.engine_ns)
    };
    if settled && !oms_foreign {
        e.oms
            .remember_external_cash(x, r.is_some() && !auto_recon)
            .map_err(FatalReason::OmsCash)?;
    }
    ledger_notes(e);
    let Some(app) = r else {
        return Ok(());
    };
    if before != (e.ledger.pos(a, x.inst).qty.0 != 0) {
        e.pos_touched.push(x.inst);
    }
    let mut parts: Vec<(GroupId, Qty)> = Vec::with_capacity(2);
    if app.net_qty.0 != 0 {
        e.trace
            .rec("oms.external_adjust", u64::from(a.0), u64::from(app.group.0));
        let mut o = e.take_oms();
        e.oms.external_adjust(
            a,
            x.venue,
            app.group,
            x.inst,
            app.net_qty,
            if oms_foreign { 0 } else { x.exch_order_id },
            x.cause,
            &mut o,
        );
        crate::intents::drain_oms(e, inst, drv, &mut o)?;
        e.put_oms(o);
        parts.push((app.group, app.net_qty));
    }
    if app.none_qty.0 != 0 {
        parts.push((GROUP_NONE, app.none_qty));
    }
    let mut views: Vec<(GroupId, ExternalFillView)> = Vec::with_capacity(2);
    for (g, q) in parts.iter() {
        let mut v = ExternalFillView {
            venue: x.venue,
            inst: x.inst,
            group: *g,
            leg_ix: if *g == GROUP_NONE { 0xFF } else { app.leg_ix },
            side: x.side,
            qty: Qty(q.0.abs()),
            px: x.px,
            fee: x.fee.0,
            fee_asset: x.fee_asset,
            cause: x.cause,
            ex_ts_ns: x.ex_ts_ns,
            deadline_tsc: Tsc(0),
        };
        let d = crate::exec_glue::on_external(e, inst, drv, a, &v)?;
        v.deadline_tsc = d;
        views.push((*g, v));
    }
    let d = PnlDelta {
        account: a,
        group: app.group,
        d: hft_ledger::PnlBreakdown::zero(),
    };
    pnl(e, inst, drv, d)?;
    if app.group != GROUP_NONE {
        crate::mode_glue::on_legs(e, inst, drv, a, app.group)?;
    }
    for (g, v) in views {
        let gix = crate::strategies::gix_for(inst, a, g);
        crate::strategies::queue_for_account(e, inst, a, Notif::External { gix, f: v });
    }
    refresh_flat_margin(e, inst, drv, a, x.venue)
}

fn funding_settle<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &InputSlot,
) -> R<()> {
    let Some(f) = slot.read::<FundingSettle>() else {
        return Ok(());
    };
    match e.rd.caps(f.venue).funding_settle {
        FundingSettleKind::PerSymbolPush | FundingSettleKind::RateOnly => {
            e.trace.rec("ledger.apply_funding", u64::from(f.account.0), 0);
            if let Some(d) = e.ledger.apply_funding(&f, e.now.engine_ns) {
                ledger_notes(e);
                pnl(e, inst, drv, d)?;
            } else {
                ledger_notes(e);
            }
        }
        FundingSettleKind::BalanceOnlyPush => {
            e.trace
                .rec("ledger.apply_funding_push", u64::from(f.account.0), 0);
            let _ = e.ledger.apply_funding_push(&f, e.now.engine_ns);
            ledger_notes(e);
            pnl(
                e,
                inst,
                drv,
                PnlDelta {
                    account: f.account,
                    group: GROUP_NONE,
                    d: hft_ledger::PnlBreakdown::zero(),
                },
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn margin_report<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &InputSlot,
) -> R<()> {
    let body = slot.body();
    let Some(h) = crate::types::read_body::<MarginReportHdr>(body) else {
        return Ok(());
    };
    let items = body.get(core::mem::size_of::<MarginReportHdr>()..).unwrap_or(&[]);
    let view = hft_ledger::MarginReportView { hdr: h, items };
    e.trace.rec("ledger.on_margin", u64::from(h.account.0), 0);
    e.ledger.on_margin(&view, e.now.engine_ns);
    ledger_notes(e);
    let trunc = h.mflags & MarginReportHdr::ITEMS_TRUNCATED != 0;
    margin_to_risk(e, inst, drv, h.account, h.venue, trunc)
}

fn account_cfg<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &InputSlot,
) -> R<()> {
    let body = slot.body();
    let Some(h) = crate::types::read_body::<AccountConfigReportHdr>(body) else {
        return Ok(());
    };
    let n = usize::from(h.n_lev);
    let tail = body
        .get(core::mem::size_of::<AccountConfigReportHdr>()..)
        .unwrap_or(&[]);
    let mut items = Vec::with_capacity(n);
    for k in 0..n {
        if let Some(x) = tail.get(16 * k..16 * k + 16)
            && let Some(l) = crate::types::read_body_plain::<hft_event::LeverageItem>(x)
        {
            items.push(l);
        }
    }
    e.trace.rec(
        "risk.on_account_cfg",
        u64::from(h.account.0),
        u64::from(h.venue.0),
    );
    e.risk.on_account_cfg(&h, &items);
    let keys = e
        .setup
        .keys_required
        .iter()
        .find(|k| k.0 == h.account && k.1 == h.venue)
        .map_or(0, |k| k.2);
    crate::mode_glue::apply_mode(
        e,
        inst,
        drv,
        ModeInput::Gate(GateEvent::AccountCfg {
            hdr: &h,
            keys_required: keys,
        }),
    )
}

// ------------------------------------------------------------------------------- connections

fn conn_state<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &InputSlot,
) -> R<()> {
    let Some(c) = slot.read::<ConnState>() else {
        return Ok(());
    };
    let bbo_conn = c.role == ConnRole::Md && c.conn < MAX_BBO_CONN_SLOTS;
    if bbo_conn {
        liveness(e, &c);
        if c.state == ConnStateKind::Alive {
            return Ok(());
        }
    } else if c.state == ConnStateKind::Alive {
        return Ok(());
    }
    // Depth connections: book invalidation / resync.
    if c.role == ConnRole::Md && !bbo_conn {
        if c.inst.0 != 0 {
            if let Some(b) = e.books.l2_mut(c.inst) {
                b.on_conn(c.state);
            }
            if c.state == ConnStateKind::Up {
                feeds_gate(e, inst, drv)?;
            }
        } else if c.state == ConnStateKind::Down {
            let insts: Vec<InstId> = e.rd.insts_of_venue(c.venue).to_vec();
            for i in insts {
                if let Some(b) = e.books.l2_mut(i) {
                    b.on_conn(ConnStateKind::Down);
                }
            }
        }
    }
    // Order/user-stream connections: OMS, rate limit, gates, cancel obligation.
    if matches!(c.role, ConnRole::Order | ConnRole::UserStream | ConnRole::Rest) {
        let h = hdr_of(e, slot);
        let mut o = e.take_oms();
        e.trace
            .rec("oms.on_conn_state", u64::from(c.account.0), c.state as u64);
        e.oms.on_conn_state(&c, &h, &mut o);
        crate::intents::drain_oms(e, inst, drv, &mut o)?;
        e.put_oms(o);
        if c.state == ConnStateKind::Degraded && c.reason == RATE_LIMIT_REASON {
            e.trace.rec(
                "risk.on_rate_limit_hit",
                u64::from(c.venue.0),
                u64::from(c.account.0),
            );
            let n = e.risk.on_rate_limit_hit(c.venue, c.account, &e.now);
            e.out.push(RecordFlags(0), &n);
        }
        if matches!(c.role, ConnRole::Order | ConnRole::UserStream) {
            order_conns(e, inst, drv, &c)?;
        }
        if c.state == ConnStateKind::Up {
            crate::mode_glue::cancel_obligation_on_live(e, inst, drv, c.account, c.venue)?;
        }
    }
    // Strategy Conn events: instances subscribed to any inst of the venue (SDK "EventRef"); an
    // account's own connections only to the instance bound to it.
    for ix in 0..inst.slots.len() {
        let Some(s) = inst.slots.get(ix) else { continue };
        if s.inst.is_none() {
            continue;
        }
        let a = s.account;
        let subscribed = s.subs.iter().any(|x| e.rd.inst(x.inst).venue == c.venue);
        if subscribed && (c.account.0 == 0 || c.account == a) {
            e.notify.push(
                ix as u16,
                Notif::Conn {
                    venue: c.venue,
                    state: c.state,
                },
            );
        }
    }
    unknown_gate(e, inst, drv, c.account)
}

/// hft-venue `ConnState.reason` RATE_LIMIT.
pub(crate) const RATE_LIMIT_REASON: u32 = 11;

fn liveness(e: &mut Engine, c: &ConnState) {
    let v = c.venue;
    let k = c.conn;
    match (c.state, c.inst.0) {
        (ConnStateKind::Up, 0) => {
            if let Some(s) = e.market.conn_mut(v, k) {
                s.up = true;
                s.resync = false;
                s.live_tsc = c.rx_tsc;
                s.used = true;
            }
        }
        (ConnStateKind::Up, _) => {
            if let Some(r) = e.market.row_mut(c.inst) {
                r.bbo_member_mask |= 1u8 << k;
            }
            if let Some(s) = e.market.conn_mut(v, k) {
                s.live_tsc = s.live_tsc.max(c.rx_tsc);
                s.used = true;
            }
        }
        (ConnStateKind::Down, 0) => {
            if let Some(s) = e.market.conn_mut(v, k) {
                s.up = false;
                s.used = true;
            }
            let insts: Vec<InstId> = e.rd.insts_of_venue(v).to_vec();
            for i in insts {
                if let Some(r) = e.market.row_mut(i) {
                    r.bbo_member_mask &= !(1u8 << k);
                }
            }
        }
        (ConnStateKind::Down, _) => {
            if let Some(r) = e.market.row_mut(c.inst) {
                r.bbo_member_mask &= !(1u8 << k);
            }
        }
        (ConnStateKind::Alive, _) => {
            if let Some(s) = e.market.conn_mut(v, k) {
                s.live_tsc = s.live_tsc.max(c.rx_tsc);
                s.used = true;
            }
        }
        (ConnStateKind::Resync, 0) => {
            if let Some(s) = e.market.conn_mut(v, k) {
                s.resync = true;
                s.used = true;
            }
        }
        (ConnStateKind::Resync, _) => {
            if let Some(r) = e.market.row_mut(c.inst) {
                r.bbo_flags |= BBO_RESYNC;
            }
        }
        _ => {}
    }
}

/// gw order connections (conn 0 = primary, 1 = standby) and user streams → `Gate(OrderConns)`.
fn order_conns<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    c: &ConnState,
) -> R<()> {
    let up = c.state == ConnStateKind::Up;
    let down = matches!(c.state, ConnStateKind::Down);
    if !up && !down {
        return Ok(());
    }
    let mut st = e.cmd.conns.get(c.account, c.venue);
    if e.cmd.conns.physical {
        let bits = if c.role == ConnRole::UserStream {
            st.us_seen = true;
            &mut st.users
        } else {
            &mut st.orders
        };
        let word = usize::from(c.conn) / 64;
        let mask = 1u64 << (c.conn % 64);
        if up {
            bits[word] |= mask;
        } else {
            bits[word] &= !mask;
        }
        let orders: u32 = st.orders.iter().map(|x| x.count_ones()).sum();
        st.primary = orders != 0;
        st.standby = orders > 1;
        st.us_up = st.users.iter().any(|x| *x != 0);
    } else {
        match (c.role, c.conn) {
            (ConnRole::UserStream, _) => {
                st.us_seen = true;
                st.us_up = up;
            }
            (_, 0) => st.primary = up,
            _ => st.standby = up,
        }
    }
    e.cmd.conns.set(c.account, c.venue, st);
    let primary_up = st.primary && (!st.us_seen || st.us_up);
    let standby_up = st.standby;
    e.trace
        .rec("gate.order_conns", u64::from(primary_up), u64::from(standby_up));
    crate::mode_glue::apply_mode(
        e,
        inst,
        drv,
        ModeInput::Gate(GateEvent::OrderConns {
            account: c.account,
            venue: c.venue,
            primary_up,
            standby_up,
        }),
    )
}

/// `Gate(Unknown{account, count})` when the account's Unknown count changed.
pub(crate) fn unknown_gate<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
) -> R<()> {
    if a.0 == 0 {
        return Ok(());
    }
    let mut n = 0u32;
    for v in 0..64u8 {
        if e.rd.account(a).venues & (1u64 << v) != 0 {
            n += e.oms.table_view(VenueId(v), a).map_or(0, |t| t.unknown_count);
        }
    }
    let last = e.cmd.unknown_last.get(usize::from(a.0)).copied().unwrap_or(0);
    if n != last {
        if let Some(x) = e.cmd.unknown_last.get_mut(usize::from(a.0)) {
            *x = n;
        }
        crate::mode_glue::apply_mode(
            e,
            inst,
            drv,
            ModeInput::Gate(GateEvent::Unknown { account: a, count: n }),
        )?;
    }
    Ok(())
}

/// `Gate(Feeds{account, ok})` when every subscribed leg inst of the account is quoted.
pub(crate) fn feeds_gate<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
) -> R<()> {
    e.feeds_dirty = false;
    for ix in 0..inst.slots.len() {
        let Some(s) = inst.slots.get(ix) else { continue };
        let a = s.account;
        let Some(g) = s.groups() else { continue };
        let mut ok = true;
        for (_, sp) in g.iter() {
            for l in sp.legs.iter().take(usize::from(sp.n_legs)) {
                let b = e.market.bbo_view(&e.books, l.inst);
                let book_ok = e
                    .books
                    .l2(l.inst)
                    .is_none_or(|b| b.state() == hft_book::BookState::Live);
                if b.seq == 0 || !b.conn_up || !book_ok {
                    ok = false;
                }
            }
        }
        let last = e.cmd.feeds_last.get(usize::from(a.0)).copied().unwrap_or(false);
        if ok != last {
            if let Some(x) = e.cmd.feeds_last.get_mut(usize::from(a.0)) {
                *x = ok;
            }
            e.trace.rec("gate.feeds", u64::from(a.0), u64::from(ok));
            crate::mode_glue::apply_mode(e, inst, drv, ModeInput::Gate(GateEvent::Feeds { account: a, ok }))?;
        }
    }
    Ok(())
}

/// `quiet` of a BalanceReport: no in-flight orders of the account on the venue and no fill for
/// `balance_check_quiet_ms`.
fn balance_quiet(e: &Engine, a: AccountId, v: VenueId) -> bool {
    let f = e.oms.inflight();
    let none_inflight =
        e.rd.insts_of_venue(v)
            .iter()
            .all(|i| f.open_buy(a, *i).0 == 0 && f.open_sell(a, *i).0 == 0);
    let mut last_fill = 0i64;
    for i in e.rd.insts_of_venue(v) {
        last_fill = last_fill.max(e.ledger.pos(a, *i).last_fill_ens);
    }
    let quiet_ns = i64::from(e.setup.ledger.balance_check_quiet_ms) * 1_000_000;
    none_inflight && (last_fill == 0 || e.now.engine_ns - last_fill > quiet_ns)
}

// ------------------------------------------------------------------------------- timers

fn timer<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &'a InputSlot,
    p: &mut Pending<'a>,
) -> R<()> {
    let Some(t) = slot.read::<Timer>() else {
        return Ok(());
    };
    let owner = t.owner;
    let id = t.timer_id;
    let group = t.group;
    // Remove the fired heap entry (heap timers only).
    let fired = crate::timers::TimerEntry {
        deadline: t.deadline_tsc,
        owner,
        timer_id: id,
        group: group.0,
    };
    let in_heap = e.heap.remove_exact(&fired);
    e.trace.rec("timer", u64::from(owner), u64::from(id));
    match owner {
        1 => crate::commands::sys_timer(e, inst, drv, id, slot, p)?,
        2 => crate::snapshot::l2_keyframe(e, InstId(id)),
        3 => oms_timer(e, inst, drv, id)?,
        4 => {
            if let Some(rt) = RiskTimer::from_timer_id(id) {
                let mut out = e.take_margin();
                e.trace.rec("risk.on_timer", u64::from(id), 0);
                e.risk.on_timer(rt, &e.now, &mut out);
                margin_out(e, inst, drv, &out)?;
                e.put_margin(out);
            }
        }
        5 => {
            let token = u64::from(id) | (u64::from(group.0) << 32);
            crate::exec_glue::on_timer(e, inst, drv, token)?;
        }
        o if o > hft_event::TIMER_OWNER_STRATEGY_BASE => {
            let sid = hft_types::StrategyId((o - hft_event::TIMER_OWNER_STRATEGY_BASE) as u8);
            if in_heap {
                e.strat_timers = e.strat_timers.saturating_sub(1);
            }
            if let Some(ix) = inst.ix(sid) {
                if in_heap
                    && let Some(s) = inst.slots.get_mut(ix)
                    && let Some(k) = s
                        .timers
                        .iter()
                        .position(|x| x.tag == id && x.group == group && x.at.0 == t.deadline_tsc)
                {
                    s.timers.remove(k);
                }
                e.notify.push(ix as u16, Notif::Timer { tag: id, group });
            }
        }
        _ => {}
    }
    Ok(())
}

fn oms_timer<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    id: u32,
) -> R<()> {
    let kind = id & 0xFF;
    let arg = id >> 8;
    let mut o = e.take_oms();
    match kind {
        1 => {
            e.trace.rec("oms.on_continue", 0, 0);
            e.oms.on_continue(e.now.tsc, e.cur_seq, &mut o);
        }
        2 => {
            e.trace.rec("oms.on_sweep", 0, 0);
            e.oms.on_sweep(e.now.tsc, &mut o);
            let ms = e.now.tsc_per_ms.max(1);
            e.heap
                .push(crate::timers::TimerEntry {
                    deadline: e.now.tsc.0 + 1_000 * ms,
                    owner: 3,
                    timer_id: 2,
                    group: 0,
                })
                .map_err(|_| FatalReason::TimerOverflow)?;
        }
        3 => {
            e.trace.rec("oms.on_unknown_query", u64::from(arg), 0);
            e.oms.on_unknown_query(arg, e.now.tsc, e.cur_seq, &mut o);
        }
        4 => {
            e.trace.rec("oms.on_hedge_retry", u64::from(arg), 0);
            e.oms.on_hedge_retry(arg, e.now.tsc, e.cur_seq, &mut o);
        }
        5 => {
            let (a, v) = (AccountId((arg & 0xFF) as u8), VenueId(((arg >> 8) & 0xFF) as u8));
            let keep = crate::commands::draining_keep(e);
            e.trace.rec("oms.on_smoke_ttl", u64::from(a.0), u64::from(keep));
            e.oms.on_smoke_ttl(a, v, keep, e.now.tsc, e.cur_seq, &mut o);
        }
        _ => {}
    }
    crate::intents::drain_oms(e, inst, drv, &mut o)?;
    e.put_oms(o);
    Ok(())
}

// ------------------------------------------------------------------------------- advice

fn advice<'a, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    slot: &'a InputSlot,
    p: &mut Pending<'a>,
) -> R<()> {
    let body = slot.body();
    let Some(h) = crate::types::read_body::<AdviceHdr>(body) else {
        return Ok(());
    };
    let payload = body.get(32..32 + usize::from(h.len)).unwrap_or(&[]);
    let misrouted = |e: &mut Engine| {
        e.seq_note(
            note::ADVICE_MISROUTED,
            NoteLevel::P2,
            h.group,
            InstId(0),
            [
                u64::from(h.strategy.0),
                u64::from(h.account.0),
                u64::from(h.len),
                0,
                0,
                0,
                0,
                0,
            ],
        );
    };
    let Some(ix) = inst.ix(h.strategy) else {
        misrouted(e);
        return Ok(());
    };
    let created = inst.slots.get(ix).is_some_and(|s| s.inst.is_some());
    if !created
        || usize::from(h.len) > hft_event::ADVICE_MAX_PAYLOAD
        || h.group == hft_types::GROUP_ALL
        || (h.group != GROUP_NONE && h.group.strategy() != h.strategy)
        || (h.account.0 != 0 && e.rd.account(h.account).bound_instance != h.strategy)
    {
        misrouted(e);
        return Ok(());
    }
    let gix = if h.group == GROUP_NONE {
        hft_strategy_sdk::GIX_NONE
    } else {
        let s = inst.slots.get(ix);
        match (
            s.and_then(|s| s.gix_of(h.group)),
            s.and_then(|s| s.spec_seq_of(h.group)),
        ) {
            (Some(g), Some(ss)) if h.basis_seq >= ss => g,
            _ => {
                e.seq_note(
                    note::ADVICE_STALE,
                    NoteLevel::P2,
                    h.group,
                    InstId(0),
                    [u64::from(h.strategy.0), h.basis_seq, 0, 0, 0, 0, 0, 0],
                );
                return Ok(());
            }
        }
    };
    p.direct = Some((
        ix,
        Direct::Advice {
            gix,
            local_kind: h.local_kind,
            payload,
            basis_seq: h.basis_seq,
            basis_tsc: h.basis_tsc,
        },
        false,
    ));
    Ok(())
}

// ------------------------------------------------------------------------------- broken scan

/// Continues the `resolve_broken` scan (≤ 64 groups per call).
pub(crate) fn naked_scan<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
) -> R<()> {
    let Some((mut a, mut cursor)) = e.naked_scan else {
        return Ok(());
    };
    let n_acc = crate::engine::account_count(&e.rd) as u8;
    let mut budget = 64usize;
    while a.0 <= n_acc && budget > 0 {
        let mut buf = hft_oms::FixedVec::<GroupId, 64>::new();
        let more = e.oms.naked_groups(a, &mut buf, &mut cursor);
        e.trace.rec("oms.naked_groups", u64::from(a.0), buf.len() as u64);
        budget = budget.saturating_sub(buf.len().max(1));
        for g in buf.as_slice().iter().copied() {
            let hit = {
                let v = crate::noexec_views!(e);
                e.exec.resolve_broken(a, g, &v)
            };
            e.trace.rec("exec.resolve_broken", u64::from(a.0), u64::from(g.0));
            if hit {
                crate::mode_glue::broken(e, inst, drv, a, g)?;
            }
        }
        if !more {
            a = AccountId(a.0 + 1);
            cursor = 0;
        }
    }
    e.naked_scan = if a.0 <= n_acc { Some((a, cursor)) } else { None };
    Ok(())
}

/// The equity source of `risk.on_day_roll`.
pub(crate) struct LedgerEquity<'a>(pub &'a hft_ledger::Ledger);
impl EquitySource for LedgerEquity<'_> {
    fn equity(&self, a: AccountId) -> EquityView {
        let e = self.0.equity(a);
        EquityView {
            total: e.total,
            valid: e.valid,
        }
    }
}

#[allow(dead_code)]
fn _as_bytes(n: &Note) -> &[u8] {
    n.as_bytes()
}
