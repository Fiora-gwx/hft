//! Shared helpers of the risk / kill / Broken / mode cases (scenarios MD-2–MD-4, B-10, BK-1–BK-3,
//! BK-7, FL-1; SEQ-181, SEQ-185, SEQ-202, SEQ-203, SEQ-341, SEQ-353, SEQ-368).

use hft_event::{
    CapRecord, CmdScope, ConnRole, ConnStateKind, ControlCmd, EventKind, ExecDecision, Note, NoteSource,
    OrderDecision, OrderEventKind,
};
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, IntentClass, LinkRole, OrderIntent};
use hft_types::{AccountId, CapSource, GroupId, InstId, KillLevel, Px, Qty, Side, StrategyId, Tif, VenueId};

use super::*;

/// `Kill{Account(a)}`.
pub fn kill_account(a: AccountId, hard: bool, flatten: bool, cancel_resting: bool) -> ControlCmd<'static> {
    ControlCmd::Kill {
        scope: scope(StrategyId(0), a, GroupId(0)),
        level: KillLevel::Account,
        flatten,
        cancel_resting,
        hard,
    }
}

/// A command scope of (a, v) (every group).
pub fn scope_av(a: AccountId, v: VenueId) -> CmdScope {
    CmdScope {
        strategy: StrategyId(0),
        venue: v,
        account: a,
        _pad: 0,
        group: hft_types::GROUP_ALL,
        inst: InstId(0),
        _pad2: 0,
    }
}

/// `Resume{source}` of an account-wide scope.
pub fn resume_account(a: AccountId, source: CapSource) -> ControlCmd<'static> {
    ControlCmd::Resume {
        scope: scope(StrategyId(0), a, GroupId(0)),
        source,
    }
}

/// Runs a confirmed command and returns `(status, detail)` of its result.
pub fn run_cmd(h: &mut H, a: AccountId, c: &ControlCmd<'_>) -> (u8, u32) {
    let id = h.cmd_confirmed(a, c);
    h.run();
    h.res(id)
}

/// `OrderDecision`s since `m`.
pub fn decs_since(h: &H, m: usize) -> Vec<OrderDecision> {
    decisions(&h.since(m)).into_iter().map(|d| d.1).collect()
}

/// `ExecDecision`s since `m`.
pub fn exec_decs_since(h: &H, m: usize) -> Vec<ExecDecision> {
    h.since(m)
        .iter()
        .filter(|r| r.kind == EventKind::EXEC_DECISION)
        .filter_map(|r| hft_sequencer::types::read_body::<ExecDecision>(&r.body))
        .collect()
}

/// `(table, code, arg)` of an `ExecDecision.detail`.
pub fn exec_detail(d: &ExecDecision) -> (u8, u8, u16) {
    hft_exec::decision::unpack(d.detail)
}

/// Risk `CapSet` (`set = true`) or `CapCleared` records since `m`.
pub fn cap_notes(h: &H, m: usize, set: bool) -> Vec<CapRecord> {
    let code = if set { 2 } else { 3 };
    h.notes_since(m)
        .iter()
        .filter(|n| n.source == NoteSource::Risk && n.code & 0xFFFF == code)
        .map(hft_risk::cap_of_note)
        .collect()
}

/// Whether a cap of `src` on account `a` (and group `g` when given) is in effect.
pub fn cap_on(h: &H, src: CapSource, a: AccountId, g: Option<GroupId>) -> bool {
    h.s.engine()
        .risk
        .mode
        .caps()
        .iter()
        .any(|c| c.source == src && c.scope.account == a && g.is_none_or(|g| c.scope.group == g))
}

/// Exec notes of code `c` (`hft_exec::decision::note::*`) since `m`.
pub fn exec_notes(h: &H, m: usize, c: u32) -> Vec<Note> {
    let code = hft_exec::decision::note_code(c);
    h.notes_since(m).into_iter().filter(|n| n.code == code).collect()
}

/// Market connections of A and B report `Alive` (the BBO connections answer pings).
pub fn md_alive(h: &mut H) {
    let t = h.now();
    for v in [A, B] {
        h.push(
            RingId::Rep(0),
            &conn(
                v,
                0,
                ConnStateKind::Alive,
                ConnRole::Md,
                InstId(0),
                AccountId(0),
                t,
            ),
        );
    }
    h.run();
}

/// Advances to `t` (absolute TSC) in steps of at most `step`, the market connections alive and
/// due timers firing.
pub fn alive_to(h: &mut H, t: u64, step: u64) {
    while h.now() < t {
        let d = (t - h.now()).min(step);
        h.adv(d);
        md_alive(h);
    }
}

/// An IOC of `inst` at the touch from instance `s` in group `g` (class `class`), placed on the
/// next Bbo of `inst`; returns the decision (not acknowledged).
pub fn place_in(
    h: &mut H,
    s: StrategyId,
    g: GroupId,
    inst: InstId,
    side: Side,
    qty: Qty,
    class: IntentClass,
) -> OrderDecision {
    place_tif(h, s, g, inst, side, qty, class, Tif::Ioc)
}

/// [`place_in`] with time in force `tif`.
pub fn place_tif(
    h: &mut H,
    s: StrategyId,
    g: GroupId,
    inst: InstId,
    side: Side,
    qty: Qty,
    class: IntentClass,
    tif: Tif,
) -> OrderDecision {
    let trigger = h.upd + 1;
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst: i, b, .. } = ev
            && *i == inst
            && hx.s.strategy == s
            && b.update_id == trigger
        {
            let b = hx.v.bbo(inst);
            let p = if side == Side::Buy { b.ask_px } else { b.bid_px };
            let mut it = intent(hx, inst, side, tif, class, p, qty);
            it.group = g;
            let r = ctx.place(it);
            PLACE_RESULT.with(|c| *c.borrow_mut() = format!("{r:?}"));
        }
    });
    PLACE_RESULT.with(|c| c.borrow_mut().clear());
    let m = h.mark();
    requote(h, inst);
    clear_hook();
    let a = account_of(h, s);
    decs_since(h, m)
        .into_iter()
        .find(|d| d.inst == inst && d.account == a)
        .unwrap_or_else(|| {
            panic!(
                "no decision of the placed intent (place → {})",
                PLACE_RESULT.with(|c| c.borrow().clone())
            )
        })
}

/// Instance `s` places `f(hx)` on the next a1 Bbo (a1 re-quoted unchanged); returns the mark.
pub fn place_with(h: &mut H, s: StrategyId, f: fn(&Hx<'_>) -> OrderIntent) -> usize {
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == s
        {
            let _ = ctx.place(f(hx));
        }
    });
    let m = h.mark();
    requote(h, A1);
    clear_hook();
    m
}

/// The account bound to instance `s`.
pub fn account_of(h: &H, s: StrategyId) -> AccountId {
    h.s.instances()
        .slots
        .iter()
        .find(|x| x.id == s)
        .map(|x| x.account)
        .expect("bound instance")
}

fn dec2(p: Px) -> String {
    let sign = if p.0 < 0 { "-" } else { "" };
    let v = p.0.unsigned_abs();
    format!("{sign}{}.{:02}", v / 100, v % 100)
}

/// Re-quotes `inst` at its current top (a new update id) and runs.
pub fn requote(h: &mut H, inst: InstId) {
    let t = *h.s.engine().books.top(inst);
    let (b, a) = (dec2(t.bid_px), dec2(t.ask_px));
    h.quote(inst, &b, &a);
    h.run();
}

/// An `Expired` (IOC remainder cancelled) report of `d`.
pub fn expire_order(h: &mut H, d: &OrderDecision) {
    let ev = order_event(d, OrderEventKind::Expired, h.epoch());
    let r = h.rep_ring(d.account);
    h.push(r, &ev);
    h.run();
}

/// OMS `HedgeRetry` timers in the heap: `(deadline, link_id)`.
pub fn retry_timers(h: &H) -> Vec<(u64, u32)> {
    h.s.engine()
        .heap
        .sorted()
        .iter()
        .filter(|e| e.owner == 3 && e.timer_id & 0xFF == 4)
        .map(|e| (e.deadline, e.timer_id >> 8))
        .collect()
}

/// Steps to `t` (absolute TSC) in ≤ 50 ms steps, letting due timers fire (no keep-alive).
pub fn step_to(h: &mut H, t: u64) {
    while h.now() < t {
        let d = (t - h.now()).min(50 * MS);
        h.adv(d);
        h.run();
    }
}

thread_local! {
    static PLACE_RESULT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static LINK_QTY: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

/// Opens the a1-buy / b1-sell link of instance `s` (`pair_first`, `qty`), acknowledges and
/// fills the first leg, then lets every hedge order expire unfilled until the link is Broken.
/// Returns the journal mark of the Input that broke the link.
pub fn break_link(h: &mut H, s: StrategyId, qty: Qty) -> usize {
    LINK_QTY.with(|c| c.set(qty.0));
    let m = place_with(h, s, |hx| {
        pair_first(hx, Qty(LINK_QTY.with(std::cell::Cell::get)))
    });
    let a = account_of(h, s);
    let first = decs_since(h, m)
        .into_iter()
        .find(|d| d.inst == A1 && d.account == a)
        .expect("first leg");
    assert_eq!(first.reject, 0, "{first:?}");
    h.ack(&first);
    let m0 = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    for _ in 0..8 {
        let open = decs_since(h, m0)
            .into_iter()
            .rev()
            .find(|d| d.inst == B1 && d.account == a && d.reject == 0)
            .expect("hedge leg");
        h.ack(&open);
        let m = h.mark();
        expire_order(h, &open);
        if cap_notes(h, m, true)
            .iter()
            .any(|c| c.source == CapSource::BROKEN)
        {
            return m;
        }
        let due = retry_timers(h).first().map(|t| t.0).expect("retry timer");
        let m = h.mark();
        step_to(h, due);
        if cap_notes(h, m, true)
            .iter()
            .any(|c| c.source == CapSource::BROKEN)
        {
            return m;
        }
    }
    panic!("link not Broken");
}

/// The engine `now.engine_ns`.
pub fn ens(h: &H) -> i64 {
    h.s.engine().now.engine_ns
}

/// Whether a decision carries `link_role = r`.
pub fn is_role(d: &OrderDecision, r: LinkRole) -> bool {
    d.link_role == r as u8
}

/// Seq of the Input a record at `recs[k]` belongs to (the nearest Input at or before it).
pub fn input_of(recs: &[hft_sequencer::MemRecord], k: usize) -> Option<u64> {
    recs.get(..=k)?
        .iter()
        .rev()
        .find(|r| r.role() == hft_event::Role::Input)
        .map(|r| r.seq)
}

/// Index of the first Note record satisfying `f`.
pub fn note_pos(recs: &[hft_sequencer::MemRecord], f: impl Fn(&Note) -> bool) -> Option<usize> {
    recs.iter()
        .position(|r| r.kind == EventKind::NOTE && r.read::<Note>().is_some_and(|n| f(&n)))
}

/// Whether `n` is a risk `CapSet` (`set`) or `CapCleared` note of `src`.
pub fn is_cap_note(n: &Note, set: bool, src: CapSource) -> bool {
    n.source == NoteSource::Risk
        && n.code & 0xFFFF == if set { 2 } else { 3 }
        && hft_risk::cap_of_note(n).source == src
}

/// An `ExternalFill` of (a, inst) with cause `Manual` (a trade on the venue's web page).
pub fn manual_fill(h: &H, a: AccountId, inst: InstId, side: Side, qty: &str) -> hft_event::ExternalFill {
    external_fill(h, a, inst, side, qty, hft_event::ExternalCause::Manual)
}

/// An `ExternalFill` of (a, inst) at 50 000 without fee.
pub fn external_fill(
    h: &H,
    a: AccountId,
    inst: InstId,
    side: Side,
    qty: &str,
    cause: hft_event::ExternalCause,
) -> hft_event::ExternalFill {
    hft_event::ExternalFill {
        venue: inst_venue(inst),
        account: a,
        side,
        cause,
        inst,
        qty: q(qty),
        px: px("50000"),
        fee: Qty(0),
        fee_asset: hft_types::AssetId(0),
        _pad: [0; 6],
        exch_order_id: 0,
        trade_id: 900_000 + h.now() % 100_000_000,
        ex_ts_ns: 0,
        rx_tsc: hft_types::Tsc(h.now()),
        fill_seq: 0,
        cash_qty: Qty(0),
        cash_amount: Qty(0),
    }
}

/// `SendReport{op, Written}` of `d` (decision at `seq`), sent at `t`.
pub fn sent(d: &OrderDecision, op: hft_event::GwOp, seq: u64, t: u64) -> hft_event::SendReport {
    hft_event::SendReport {
        decision_seq: seq,
        sent_tsc: hft_types::Tsc(t),
        slot: d.slot,
        r#gen: d.r#gen,
        target_slot: d.slot,
        target_gen: d.r#gen,
        venue: d.venue,
        account: d.account,
        conn: hft_types::ConnId(0),
        status: hft_event::SendStatus::Written,
        op,
        _pad: [0; 3],
        os_errno: 0,
        _pad2: 0,
    }
}

thread_local! {
    static REPORT_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(50_000) };
}

/// One clean reconciliation round (a `Summary` with no differences) of account `a` on A and B,
/// viewed at the current seq (the strict condition of a HARD release, the Recon recovery rounds).
pub fn clean_recon(h: &mut H, a: AccountId) {
    let view = h.s.engine().cur_seq;
    for v in [A, B] {
        let id = REPORT_ID.with(|c| {
            c.set(c.get() + 1);
            c.get()
        });
        h.push(RingId::Rep(0), &recon_summary(a, v, id, view));
    }
    h.run();
}

/// Instance `s`'s one-leg IOCs of group `g` are acknowledged, `Kill{Account, hard}` of its
/// account applies, then they fill: the naked quantity lands under HARD, so no repair runs.
pub fn naked_under_hard(
    h: &mut H,
    s: StrategyId,
    g: GroupId,
    legs: &[(InstId, Side, &str)],
) -> Vec<OrderDecision> {
    let a = account_of(h, s);
    let ds: Vec<OrderDecision> = legs
        .iter()
        .map(|(i, side, x)| {
            let d = place_in(h, s, g, *i, *side, q(x), IntentClass::Open);
            assert_eq!(d.reject, 0, "{d:?}");
            h.ack(&d);
            d
        })
        .collect();
    let r = run_cmd(h, a, &kill_account(a, true, false, false));
    assert_eq!(r.0, hft_sequencer::codes::status::APPLIED, "{r:?}");
    for d in &ds {
        h.fill(d, d.qty, d.qty, d.px);
    }
    ds
}

/// A clean reconciliation round, then `Resume{Kill, Account(a)}` deleting the HARD record.
pub fn release_hard(h: &mut H, a: AccountId) {
    clean_recon(h, a);
    let r = run_cmd(h, a, &resume_account(a, CapSource::KILL));
    assert_eq!(r.0, hft_sequencer::codes::status::APPLIED, "{r:?}");
}

/// g of instance `s` Broken by `resolve_broken` with the given filled legs (no link, no
/// `HedgeFailure`).
pub fn broken_by_release(h: &mut H, s: StrategyId, g: GroupId, legs: &[(InstId, Side, &str)]) {
    naked_under_hard(h, s, g, legs);
    let a = account_of(h, s);
    release_hard(h, a);
    assert!(
        cap_on(h, CapSource::BROKEN, a, Some(g)),
        "Broken after the HARD release"
    );
}

/// `MarginReport` of (a, B): balance 100 000, maintenance 1 000, ratio `ratio`, and one item
/// (b1, `liq_px`) when given (`rep_liq_dist` = |mark − liq_px| / mark).
pub fn margin_with(h: &mut H, a: AccountId, ratio: &str, liq_px: Option<&str>) {
    use zerocopy::{FromZeros, IntoBytes};
    let mut hd = hft_event::MarginReportHdr::new_zeroed();
    hd.venue = B;
    hd.account = a;
    hd.source = 1;
    hd.margin_balance = n("100000");
    hd.maint_margin = n("1000");
    hd.avail_margin = n("90000");
    hd.margin_ratio = r(ratio);
    hd.rx_tsc = hft_types::Tsc(h.now());
    hd.n = u16::from(liq_px.is_some());
    let mut body = hd.as_bytes().to_vec();
    if let Some(l) = liq_px {
        let it = hft_event::MarginItem {
            inst: B1,
            adl_quantile: 255,
            _pad: [0; 3],
            liq_px: px(l),
        };
        body.extend_from_slice(it.as_bytes());
    }
    h.push_bytes(RingId::Rep(0), EventKind::MARGIN_REPORT, body);
    h.run();
}

/// `OrderDecision.reason` codes of executor orders (hft-exec `ExecOrderKind`).
pub const REASON_CMD_HEDGE_FIX: u32 = 9;
pub const REASON_AUTO_FIX: u32 = 10;
