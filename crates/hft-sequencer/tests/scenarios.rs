//! Cross-component hedge-wiring scenarios on the real engine (docs/tests/scenarios.md §6–§8:
//! B1, B2, B6, LK-1–LK-3, FP-1, FP-2), run as deterministic step sequences on the B0 rig.
//!
//! Mapping to world W1: venue A ≙ `VS` (spot), B ≙ `VP` (USDT perp), a1 ≙ `S1`, b1 ≙ `P1`,
//! account 1 / instance s1 / g1 ≙ `a` / instance 1 / `g1`, account 2 / s2 / g2 ≙ `b` / 2 / `g2`.
//! The "开仓链接" is `pair_first`: a1 buy IOC with an OnFill link selling b1 (ratio 1/1, Retry).
//! Thresholds come from the B0 parameters (`conn_liveness_ms` 2 000, `rr_max_cross` 0.01,
//! `max_hedge_notional` 20 000, OMS README defaults: `hedge_retry_max` 3, `hedge_retry_min_ms`
//! 100, `hedge_deadline_ms` 3 000) and are read back from the engine where a case computes them.

use hft_event::{
    CapRecord, ConnRole, ConnStateKind, ControlCmd, EventKind, GwCmd, Note, NoteSource, OrderAction,
    OrderDecision, OrderEventKind, OrderFlags,
};
use hft_oms::{LinkBroken, LinkEntry, LinkState, notes};
use hft_sequencer::OutRing;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, LinkRole, OrderIntent};
use hft_types::{
    AccountId, CapSource, GroupId, InstId, KillLevel, Px, Qty, RejectBits, RoundMode, Side, StrategyId,
};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

/// Every instance in `who` places `f(hx)` on the next a1 Bbo; returns the journal mark before it.
fn place_on_a1(h: &mut H, who: &'static [StrategyId], f: fn(&Hx<'_>) -> OrderIntent) -> usize {
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && who.contains(&hx.s.strategy)
        {
            let _ = ctx.place(f(hx));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    m
}

fn link_open(hx: &Hx<'_>) -> OrderIntent {
    pair_first(hx, q("0.01"))
}

fn decs(h: &H, m: usize) -> Vec<OrderDecision> {
    decisions(&h.since(m)).into_iter().map(|d| d.1).collect()
}

fn frames(h: &H, n: usize) -> Vec<(OutRing, GwCmd)> {
    h.io_ref().out[n..].to_vec()
}

fn notes_of(h: &H, m: usize, c: notes::OmsNoteCode) -> Vec<Note> {
    let code = notes::code_of(c);
    h.notes_since(m).into_iter().filter(|n| n.code == code).collect()
}

/// `CapSet` records written since `m`.
fn cap_sets(h: &H, m: usize) -> Vec<CapRecord> {
    h.notes_since(m)
        .iter()
        .filter(|n| n.source == NoteSource::Risk && n.code & 0xFFFF == 2)
        .map(hft_risk::cap_of_note)
        .collect()
}

fn has_cap(h: &H, src: CapSource, a: AccountId, g: Option<GroupId>) -> bool {
    h.s.engine()
        .risk
        .mode
        .caps()
        .iter()
        .any(|c| c.source == src && c.scope.account == a && g.is_none_or(|g| c.scope.group == g))
}

/// The newest link of account `a` (link ids are small; the B0 table holds 4 per account).
fn last_link(h: &H, a: AccountId) -> Option<LinkEntry> {
    (1..=256u32)
        .filter_map(|id| h.s.engine().oms.link(id).copied())
        .filter(|l| l.account == a && l.state != LinkState::Free)
        .max_by_key(|l| l.link_id)
}

/// OMS hedge-retry timers in the heap: `(deadline, link_id)`.
fn hedge_retry_timers(h: &H) -> Vec<(u64, u32)> {
    h.s.engine()
        .heap
        .sorted()
        .iter()
        .filter(|e| e.owner == 3 && e.timer_id & 0xFF == 4)
        .map(|e| (e.deadline, e.timer_id >> 8))
        .collect()
}

fn role(d: &OrderDecision) -> u8 {
    d.link_role
}

/// Opens the a1-buy / b1-sell link of s1 and acknowledges the first leg.
fn open_first(h: &mut H) -> OrderDecision {
    let m = place_on_a1(h, &[S1], link_open);
    let d = decs(h, m).into_iter().find(|d| d.inst == A1).expect("first leg");
    assert_eq!(d.reject, 0, "{d:?}");
    h.ack(&d);
    d
}

fn expire(h: &mut H, d: &OrderDecision) {
    let ev = order_event(d, OrderEventKind::Expired, h.epoch());
    let r = h.rep_ring(d.account);
    h.push(r, &ev);
    h.run();
}

/// Steps to `t` (absolute TSC), letting due timers fire.
fn run_to(h: &mut H, t: u64) {
    while h.now() < t {
        let d = (t - h.now()).min(50 * MS);
        h.adv(d);
        h.run();
    }
}

fn no_hedge_fix(h: &H, m: usize) -> bool {
    let fix_decision = h.since(m).iter().any(|r| {
        r.kind == EventKind::EXEC_DECISION
            && r.read::<hft_event::ExecDecision>()
                .is_some_and(|x| x.action == hft_exec::decision::action::HEDGE_FIX)
    });
    !fix_decision && decs(h, m).iter().all(|d| role(d) != LinkRole::HedgeFix as u8)
}

// ------------------------------------------------------------------------------------- B1

#[test]
fn b1_link_open_rejected_when_hedge_venue_killed() {
    // Main: account 1 has a Kill on (B, a); account 2's identical open is untouched.
    let mut h = steady();
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::Kill {
            scope: hft_event::CmdScope {
                strategy: StrategyId(0),
                venue: B,
                account: AC1,
                _pad: 0,
                group: hft_types::GROUP_ALL,
                inst: InstId(0),
                _pad2: 0,
            },
            level: KillLevel::Venue,
            flatten: false,
            cancel_resting: true,
            hard: false,
        },
    );
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0), "Kill applied");
    let n = h.io_ref().out.len();
    let open_before = h.s.engine().oms.inflight().pos(AC1, A1).open_buy;
    let m = place_on_a1(&mut h, &[S1, S2], link_open);
    let ds = decs(&h, m);
    let a: Vec<_> = ds.iter().filter(|d| d.account == AC1).collect();
    assert_eq!(a.len(), 1, "{ds:?}");
    assert_eq!((a[0].inst, a[0].reject), (A1, RejectBits::KILLED.0));
    assert!(
        frames(&h, n).iter().all(|f| f.1.account != AC1),
        "no frame of account 1"
    );
    assert_eq!(h.s.engine().oms.inflight().pos(AC1, A1).open_buy, open_before);
    // Account 2: the first leg goes out, the second leg in its fill's Input.
    let b = ds
        .iter()
        .find(|d| d.account == AC2)
        .copied()
        .expect("account 2 first leg");
    assert_eq!(b.reject, 0);
    assert!(frames(&h, n).iter().any(|f| f.1.account == AC2 && f.1.inst == A1));
    h.ack(&b);
    let m = h.mark();
    h.fill(&b, b.qty, b.qty, b.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("account 2 second leg");
    assert_eq!((second.account, second.reject), (AC2, 0));

    // B1.1: (B, a) order connection down → VENUE_NOT_READY, nothing out.
    let mut h = steady();
    let t = h.now();
    for k in [0u8, 1] {
        h.push(
            RingId::Rep(0),
            &conn(B, k, ConnStateKind::Down, ConnRole::Order, InstId(0), AC1, t),
        );
    }
    h.run();
    let n = h.io_ref().out.len();
    let m = place_on_a1(&mut h, &[S1], link_open);
    let d = decs(&h, m);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].reject, RejectBits::VENUE_NOT_READY.0);
    assert!(frames(&h, n).is_empty());

    // B1.4: b1's BBO connection silent for more than conn_liveness_ms → LINK_STALE.
    let mut h = steady();
    h.adv(3 * SEC);
    h.run();
    let n = h.io_ref().out.len();
    let m = place_on_a1(&mut h, &[S1], link_open);
    let d = decs(&h, m);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].reject, RejectBits::LINK_STALE.0);
    assert!(frames(&h, n).is_empty());

    // B1.5: (B, a) available margin 100 → LINK_FUNDS.
    let mut h = steady();
    h.margin_report(AC1, "100", "0", "100");
    h.run();
    let n = h.io_ref().out.len();
    let m = place_on_a1(&mut h, &[S1], link_open);
    let d = decs(&h, m);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].reject, RejectBits::LINK_FUNDS.0);
    assert!(frames(&h, n).is_empty());
}

/// B1.6: Broken by an exhausted second leg → `CapSet{Broken, (a, g1)}` and
/// `CapSet{HedgeFailure, a}` in one Input; every Open of account 1 is `CLOSE_ONLY` while account
/// 2 trades; `Resume{HedgeFailure}` is refused while Broken remains and applies after HedgeFix.
#[test]
fn b1_6_broken_blocks_account_opens_until_fixed() {
    let mut h = steady();
    let first = open_first(&mut h);
    h.fill(&first, first.qty, first.qty, first.px);
    // Every second leg fails until the retries are exhausted.
    let mut broken_at = None;
    for _ in 0..8 {
        let m = h.mark();
        let open = decs(&h, 0)
            .into_iter()
            .rev()
            .find(|d| d.inst == B1 && d.reject == 0)
            .expect("hedge leg");
        expire(&mut h, &open);
        if !cap_sets(&h, m).is_empty() {
            broken_at = Some(m);
            break;
        }
        let due = hedge_retry_timers(&h).first().map(|t| t.0).expect("retry timer");
        run_to(&mut h, due);
        if !cap_sets(&h, m).is_empty() {
            broken_at = Some(m);
            break;
        }
    }
    let m = broken_at.expect("link Broken");
    let caps = cap_sets(&h, m);
    assert!(
        caps.iter()
            .any(|c| c.source == CapSource::BROKEN && c.scope.account == AC1 && c.scope.group == G1),
        "{caps:?}"
    );
    assert!(
        caps.iter()
            .any(|c| c.source == CapSource::HEDGE_FAILURE && c.scope.account == AC1),
        "{caps:?}"
    );
    // Any Open of account 1 (a plain one-leg open too) → CLOSE_ONLY; account 2 unaffected.
    let m = place_on_a1(&mut h, &[S1, S2], |hx| open_buy(hx, A1));
    let ds = decs(&h, m);
    let a = ds.iter().find(|d| d.account == AC1).expect("account 1");
    let b = ds.iter().find(|d| d.account == AC2).expect("account 2");
    assert_eq!(a.reject, RejectBits::CLOSE_ONLY.0);
    assert_eq!(b.reject, 0);
    h.ack(b);
    h.fill(b, b.qty, b.qty, b.px);
    // Resume{HedgeFailure, a} while Broken remains → Rejected{BrokenRemaining}.
    let resume = ControlCmd::Resume {
        scope: scope(StrategyId(0), AC1, GroupId(0)),
        source: CapSource::HEDGE_FAILURE,
    };
    let id = h.cmd_confirmed(AC1, &resume);
    h.run();
    assert_eq!(
        h.res(id),
        (
            hft_sequencer::codes::status::REJECTED,
            u32::from(hft_sequencer::codes::detail::BROKEN_REMAINING)
        )
    );
    // HedgeFix sells the extra a1; its fill clears Broken, then the same Resume applies.
    let m = h.mark();
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::HedgeFix {
            scope: scope(S1, AC1, G1),
        },
    );
    h.run();
    assert_eq!(
        h.res(id).0,
        hft_sequencer::codes::status::APPLIED,
        "{:?}",
        h.res(id)
    );
    let fix = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == A1 && role(d) == LinkRole::HedgeFix as u8)
        .expect("HedgeFix order");
    assert_eq!((fix.side, fix.qty, fix.reject), (Side::Sell, q("0.01"), 0));
    h.ack(&fix);
    let m = h.mark();
    h.fill(&fix, fix.qty, fix.qty, fix.px);
    assert!(
        !has_cap(&h, CapSource::BROKEN, AC1, Some(G1)),
        "CapCleared{{Broken}}"
    );
    assert!(
        h.notes_since(m)
            .iter()
            .any(|n| n.source == NoteSource::Risk && n.code & 0xFFFF == 3),
        "a CapCleared note"
    );
    let id = h.cmd_confirmed(AC1, &resume);
    h.run();
    assert_eq!(
        h.res(id).0,
        hft_sequencer::codes::status::APPLIED,
        "{:?}",
        h.res(id)
    );
    let m = place_on_a1(&mut h, &[S1], |hx| open_buy(hx, A1));
    assert_eq!(decs(&h, m)[0].reject, 0);
}

/// B1.2: a one-leg b1 IOC of g1 goes Unknown when (B, a)'s order connection drops; after the
/// connection is back the venue stays not ready until the slot converges: the linked open is
/// `VENUE_NOT_READY` (#4 before `UNKNOWN_PENDING`), and passes after the query result.
#[test]
fn b1_2_unknown_on_hedge_venue_blocks_link_open_until_converged() {
    let mut h = steady();
    let m = place_on_a1(&mut h, &[S1], b1_sell_open);
    let (seq, d) = decisions(&h.since(m))
        .into_iter()
        .find(|d| d.1.inst == B1)
        .expect("b1 one-leg IOC");
    assert_eq!(d.reject, 0);
    let t = h.now();
    h.push(RingId::Rep(0), &send_report(&d, hft_event::GwOp::Place, seq, t));
    h.run();
    order_conns(&mut h, B, AC1, ConnStateKind::Down, &[0, 1]);
    let r = hft_strategy_sdk::OrderRef::new(B, AC1, d.slot.0, d.r#gen);
    assert_eq!(
        h.s.engine().oms.slot(r).map(|s| s.state),
        Some(hft_oms::OrderState::Unknown)
    );
    h.adv(100 * MS);
    let m = h.mark();
    order_conns(&mut h, B, AC1, ConnStateKind::Up, &[0]);
    let (qseq, _) = decisions(&h.since(m))
        .into_iter()
        .find(|d| d.1.action == OrderAction::Query)
        .expect("query on Up");
    let t = h.now();
    h.push(RingId::Rep(0), &send_report(&d, hft_event::GwOp::Query, qseq, t));
    h.run();
    h.adv(400 * MS);
    let n = h.io_ref().out.len();
    let m = place_on_a1(&mut h, &[S1], link_open);
    let x = decs(&h, m);
    assert_eq!(x.len(), 1, "{x:?}");
    assert_eq!(x[0].reject, RejectBits::VENUE_NOT_READY.0);
    assert!(frames(&h, n).is_empty());
    // The query finds the IOC expired: Cancelled, venue ready; the next open goes out.
    let mut ev = order_event(&d, OrderEventKind::QueryResult, h.epoch());
    ev.detail = 4;
    h.push(RingId::Rep(0), &ev);
    h.run();
    assert_eq!(
        h.s.engine().oms.slot(r).map(|s| s.state),
        Some(hft_oms::OrderState::Cancelled)
    );
    let n = h.io_ref().out.len();
    let m = place_on_a1(&mut h, &[S1], link_open);
    assert_eq!(decs(&h, m)[0].reject, 0);
    assert_eq!(frames(&h, n).len(), 1);
}

fn a1_buy_open(hx: &Hx<'_>) -> OrderIntent {
    open_buy(hx, A1)
}

/// A one-leg b1 sell IOC Open of g1 at the bid.
fn b1_sell_open(hx: &Hx<'_>) -> OrderIntent {
    let b = hx.v.bbo(B1);
    let mut it = intent(
        hx,
        B1,
        Side::Sell,
        hft_types::Tif::Ioc,
        hft_strategy_sdk::IntentClass::Open,
        b.bid_px,
        q("0.001"),
    );
    it.group = G1;
    it
}

/// Advances to `t` in 250 ms steps with both venues' market connections alive.
fn keep_alive_to(h: &mut H, t: u64) {
    while h.now() < t {
        let d = (t - h.now()).min(250 * MS);
        h.adv(d);
        let now = h.now();
        for v in [A, B] {
            h.push(RingId::Rep(0), &md(v, ConnStateKind::Alive, now));
        }
        h.run();
    }
}

/// An exchange rate-limit rejection of a one-leg `inst` IOC of s1, with the gateway's
/// `ConnState{Degraded, RATE_LIMIT}` (the hit that clears the buckets and starts the backoff).
fn rate_limit_hit(h: &mut H, inst: InstId) -> OrderDecision {
    let f: fn(&Hx<'_>) -> OrderIntent = if inst == A1 { a1_buy_open } else { b1_sell_open };
    let m = place_on_a1(h, &[S1], f);
    let d = decs(h, m)
        .into_iter()
        .find(|d| d.inst == inst)
        .expect("probe order");
    assert_eq!(d.reject, 0);
    venue_rate_reject(h, &d);
    d
}

/// The venue rejects `d` with its rate-limit code (-1015) and the gateway reports
/// `ConnState{Degraded, RATE_LIMIT}` on the order connection.
fn venue_rate_reject(h: &mut H, d: &OrderDecision) {
    let mut ev = order_event(d, OrderEventKind::Reject, h.epoch());
    ev.reject_code = 0xFFFF_FC09;
    let t = h.now();
    let mut cs = conn(
        d.venue,
        0,
        ConnStateKind::Degraded,
        ConnRole::Order,
        InstId(0),
        AC1,
        t,
    );
    cs.reason = 11;
    h.push(RingId::Rep(0), &ev);
    h.push(RingId::Rep(0), &cs);
    h.run();
}

/// B1.3 (on the 60 s window): after a rate-limit hit on (B, a) the windows refill from 0 at
/// 10 tokens/s; the 60 s window crosses its reserve (0.20 × 600 = 120) at +12 s while the 10 s
/// window is full. The linked open needs ≥ 1 token above the link venue's reserve: +11.9 s and
/// +12.0 s are `RATE`, +12.1 s goes out.
#[test]
fn b1_3_link_venue_rate_reserve_boundary() {
    for (dt_ms, ok) in [(11_900u64, false), (12_000, false), (12_100, true)] {
        let mut h = steady();
        rate_limit_hit(&mut h, B1);
        let t = h.now();
        keep_alive_to(&mut h, t + dt_ms * MS);
        h.quote(B1, "50010", "50011");
        h.run();
        let n = h.io_ref().out.len();
        let m = place_on_a1(&mut h, &[S1], link_open);
        let d = decs(&h, m);
        assert_eq!(d.len(), 1, "+{dt_ms} ms: {d:?}");
        if ok {
            assert_eq!(d[0].reject, 0, "+{dt_ms} ms");
            assert_eq!(frames(&h, n).len(), 1);
        } else {
            assert_eq!(d[0].reject, RejectBits::RATE.0, "+{dt_ms} ms");
            assert!(frames(&h, n).is_empty());
        }
    }
}

// ------------------------------------------------------------------------------------- B2

#[test]
fn b2_open_link_retry_is_second_retry_not_reduce_only() {
    let mut h = steady();
    let first = open_first(&mut h);
    let m0 = h.mark();
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg in the fill's Input");
    assert_eq!(
        (role(&second), second.side, second.qty, second.reject),
        (LinkRole::Second as u8, Side::Sell, q("0.01"), 0)
    );
    assert!(second.oflags & OrderFlags::RISK_REDUCING.0 != 0);
    assert!(second.oflags & OrderFlags::REDUCE_ONLY.0 == 0);
    // IOC expires unfilled → HedgeRetry ≥ now + hedge_retry_min_ms, no counted retry yet.
    h.ack(&second);
    let t_exp = h.now();
    expire(&mut h, &second);
    let timers = hedge_retry_timers(&h);
    assert_eq!(timers.len(), 1, "{timers:?}");
    assert!(timers[0].0 >= t_exp + 100 * MS);
    let m = h.mark();
    let n = h.io_ref().out.len();
    run_to(&mut h, timers[0].0);
    let retries: Vec<_> = decs(&h, m)
        .into_iter()
        .filter(|d| d.inst == B1 && role(d) == LinkRole::SecondRetry as u8)
        .collect();
    assert_eq!(retries.len(), 1, "{retries:?}");
    let r = retries[0];
    assert_eq!((r.qty, r.reject, r.side), (q("0.01"), 0, Side::Sell));
    assert!(r.oflags & OrderFlags::RISK_REDUCING.0 != 0);
    assert!(r.oflags & OrderFlags::REDUCE_ONLY.0 == 0);
    let fr: Vec<_> = frames(&h, n).into_iter().filter(|f| f.1.inst == B1).collect();
    assert_eq!(fr.len(), 1);
    assert!(
        fr[0].1.oflags & OrderFlags::REDUCE_ONLY.0 == 0,
        "no exchange reduceOnly"
    );
    h.ack(&r);
    h.fill(&r, r.qty, r.qty, r.px);
    let l = last_link(&h, AC1).expect("link");
    assert_eq!((l.state, l.retries), (LinkState::Done, 1));
    assert!(
        decs(&h, m0)
            .iter()
            .all(|d| d.reject != RejectBits::REDUCE_INVALID.0)
    );
    assert!(no_hedge_fix(&h, m0));

    // B2.2: every hedge order fills nothing → exactly hedge_retry_max SecondRetry, then Broken.
    let mut h = steady();
    let first = open_first(&mut h);
    let m0 = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let mut broke = None;
    for _ in 0..6 {
        let open = decs(&h, m0)
            .into_iter()
            .rev()
            .find(|d| d.inst == B1 && d.reject == 0)
            .expect("hedge leg");
        h.ack(&open);
        let m = h.mark();
        expire(&mut h, &open);
        if last_link(&h, AC1).is_some_and(|l| l.state == LinkState::Broken) {
            broke = Some(m);
            break;
        }
        let due = hedge_retry_timers(&h).first().map(|t| t.0).expect("retry timer");
        run_to(&mut h, due);
    }
    let m = broke.expect("Broken after the retries");
    let sr = decs(&h, m0)
        .iter()
        .filter(|d| role(d) == LinkRole::SecondRetry as u8)
        .count();
    assert_eq!(sr, 3, "hedge_retry_max SecondRetry");
    let hb = notes_of(&h, m, notes::HEDGE_BROKEN);
    assert_eq!(hb.len(), 1);
    assert_eq!(hb[0].level, hft_event::NoteLevel::P0);
    let caps = cap_sets(&h, m);
    assert!(
        caps.iter()
            .any(|c| c.source == CapSource::BROKEN && c.scope.group == G1)
    );
    assert!(caps.iter().any(|c| c.source == CapSource::HEDGE_FAILURE));
    let m = h.mark();
    let t = h.now() + 2 * SEC;
    run_to(&mut h, t);
    assert!(decs(&h, m).iter().all(|d| role(d) != LinkRole::SecondRetry as u8));
}

// ------------------------------------------------------------------------------------- B6

#[test]
fn b6_unit_error_hedge_vetoed_by_hedge_limit() {
    let mut h = steady();
    let first = open_first(&mut h);
    // b1 quoted 1 000× too high (healthy connection) between the first leg and its fill.
    h.quote(B1, "50010000", "50011000");
    h.run();
    let n = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let ds = decs(&h, m);
    let second = ds.iter().find(|d| d.inst == B1).expect("second leg decision");
    assert_eq!(
        (role(second), second.qty, second.reject),
        (LinkRole::Second as u8, q("0.01"), RejectBits::HEDGE_LIMIT.0)
    );
    assert!(frames(&h, n).iter().all(|f| f.1.inst != B1), "no b1 frame");
    let l = last_link(&h, AC1).expect("link");
    assert_eq!(l.state, LinkState::Broken);
    let hb = notes_of(&h, m, notes::HEDGE_BROKEN);
    assert_eq!(hb.len(), 1);
    assert_eq!(hb[0].level, hft_event::NoteLevel::P0);
    let caps = cap_sets(&h, m);
    assert!(
        caps.iter()
            .any(|c| c.source == CapSource::BROKEN && c.scope.account == AC1 && c.scope.group == G1)
    );
    assert!(
        caps.iter()
            .any(|c| c.source == CapSource::HEDGE_FAILURE && c.scope.account == AC1)
    );
    assert!(hedge_retry_timers(&h).is_empty(), "no HedgeRetry");
    let m = h.mark();
    let t = h.now() + 4 * SEC;
    run_to(&mut h, t);
    assert!(decs(&h, m).iter().all(|d| role(d) != LinkRole::SecondRetry as u8));
    let _ = LinkBroken::HedgeLimit;

    // B6.1: the first leg's fill reports cum_filled ×1 000 → the second leg is capped at the slot
    // quantity, then OmsOverfill (P0) and Broken{Overfill}; nothing above 0.0105 goes out.
    let mut h = steady();
    let first = open_first(&mut h);
    let n = h.io_ref().out.len();
    let m = h.mark();
    let big = Qty(first.qty.0 * 1_000);
    h.fill(&first, big, big, first.px);
    let ds = decs(&h, m);
    let second = ds.iter().find(|d| d.inst == B1).expect("second leg");
    assert_eq!((second.qty, second.reject), (q("0.01"), 0));
    assert_eq!(notes_of(&h, m, notes::OVERFILL).len(), 1);
    assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Broken));
    assert!(
        frames(&h, n)
            .iter()
            .filter(|f| f.1.inst == B1)
            .all(|f| f.1.qty.0 <= q("0.0105").0)
    );
}

// ------------------------------------------------------------------------------------- LK-1

fn md(v: hft_types::VenueId, st: ConnStateKind, rx: u64) -> hft_event::ConnState {
    conn(v, 0, st, ConnRole::Md, InstId(0), AccountId(0), rx)
}

#[test]
fn lk_1_unhealthy_second_leg_conn_rejects_open() {
    // Main: b1's only BBO connection Down → LINK_STALE (only that bit), no slot, no frame.
    for st in [ConnStateKind::Down, ConnStateKind::Resync] {
        let mut h = steady();
        let t = h.now();
        h.push(RingId::Rep(0), &md(B, st, t));
        h.run();
        let n = h.io_ref().out.len();
        let legs0 = h.s.engine().oms.legs(AC1, G1).view;
        let m = place_on_a1(&mut h, &[S1], link_open);
        let d = decs(&h, m);
        assert_eq!(d.len(), 1, "{st:?}: {d:?}");
        assert_eq!(
            (d[0].reject, d[0].slot.0),
            (RejectBits::LINK_STALE.0, 0),
            "{st:?}"
        );
        assert!(frames(&h, n).is_empty());
        assert_eq!(h.s.engine().oms.legs(AC1, G1).view, legs0);
    }
    // LK-1.2–1.4: last b1 frame at T, then silence; trigger at T+1 999 / T+2 000 / T+2 001 ms.
    for (dt_ms, ok) in [(1_999u64, true), (2_000, true), (2_001, false)] {
        let mut h = steady();
        h.quote(B1, "50010", "50011");
        h.run();
        let t = h.now();
        // a1's own connection stays alive (its Bbo is the trigger).
        h.at(t + dt_ms * MS);
        let n = h.io_ref().out.len();
        let m = place_on_a1(&mut h, &[S1], link_open);
        let d = decs(&h, m);
        assert_eq!(d.len(), 1, "+{dt_ms} ms: {d:?}");
        if ok {
            assert_eq!(d[0].reject, 0, "+{dt_ms} ms");
            assert_eq!(frames(&h, n).len(), 1);
        } else {
            assert_eq!(d[0].reject, RejectBits::LINK_STALE.0, "+{dt_ms} ms");
            assert!(frames(&h, n).is_empty());
        }
    }
    // LK-1.6: b1 unchanged for 10 s, its connection answering every 1 000 ms → allowed.
    let mut h = steady();
    h.quote(B1, "50010", "50011");
    h.run();
    for _ in 0..10 {
        h.adv(SEC);
        let t = h.now();
        h.push(RingId::Rep(0), &md(B, ConnStateKind::Alive, t));
        h.run();
    }
    h.adv(500 * MS);
    let m = place_on_a1(&mut h, &[S1], link_open);
    assert_eq!(decs(&h, m)[0].reject, 0);
}

/// LK-1.5: a `Hedging` link retrying b1 at the boundary judges the quote the same way as the
/// open: out at +1 999 / +2 000 ms, gated `StaleQuote` (`Note{OmsHedgePxStale}`) at +2 001 ms.
#[test]
fn lk_1_5_retry_at_boundary_matches_open() {
    for (dt_ms, ok) in [(1_999u64, true), (2_000, true), (2_001, false)] {
        let mut h = steady();
        let first = open_first(&mut h);
        h.fill(&first, first.qty, first.qty, first.px);
        let second = decs(&h, 0)
            .into_iter()
            .rev()
            .find(|d| d.inst == B1)
            .expect("second leg");
        h.ack(&second);
        // Last b1 frame now; the retry falls due at +dt.
        h.quote(B1, "50010", "50011");
        h.run();
        let t = h.now();
        expire(&mut h, &second);
        let due = hedge_retry_timers(&h)[0].0;
        assert!(due <= t + dt_ms * MS, "retry due before the probe time");
        h.at(t + dt_ms * MS);
        let m = h.mark();
        h.run();
        let retry = decs(&h, m)
            .into_iter()
            .find(|d| d.inst == B1 && role(d) == LinkRole::SecondRetry as u8);
        if ok {
            assert_eq!(retry.map(|d| d.reject), Some(0), "+{dt_ms} ms");
        } else {
            assert!(retry.is_none(), "+{dt_ms} ms: {retry:?}");
            assert_eq!(notes_of(&h, m, notes::HEDGE_PX_STALE).len(), 1, "+{dt_ms} ms");
        }
    }
}

// ------------------------------------------------------------------------------------- LK-2

/// Account 1's b1 (perp) second leg: margin needed = `link_notional / leverage × (1 +
/// funds_buffer)`; exactly enough passes, one micro-USDT less is `LINK_FUNDS` (LK-2.4/2.5); a new
/// leverage report changes the need (LK-2.6/2.7).
#[test]
fn lk_2_unaffordable_second_leg_rejects_open() {
    // link_notional = 0.010 × mid × (1 + rr) with b1 mid 50 010.5 (B0 quote 50 010 / 50 011).
    let probe = |avail_raw: i64, lev: Option<&str>| -> u32 {
        let mut h = steady();
        if let Some(l) = lev {
            let item = hft_event::LeverageItem {
                inst: B1,
                _pad: 0,
                leverage: r(l),
            };
            let hd = cfg_hdr(AC1, B, 1);
            let mut body = zerocopy::IntoBytes::as_bytes(&hd).to_vec();
            body.extend_from_slice(zerocopy::IntoBytes::as_bytes(&item));
            h.push_bytes(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, body);
            h.run();
        }
        let a = hft_types::Notional(avail_raw);
        let mut hd = <hft_event::MarginReportHdr as zerocopy::FromZeros>::new_zeroed();
        hd.venue = B;
        hd.account = AC1;
        hd.source = 1;
        hd.margin_balance = a;
        hd.maint_margin = hft_types::Notional(0);
        hd.avail_margin = a;
        hd.rx_tsc = hft_types::Tsc(h.now());
        h.push_bytes(
            RingId::Rep(0),
            EventKind::MARGIN_REPORT,
            zerocopy::IntoBytes::as_bytes(&hd).to_vec(),
        );
        h.run();
        let m = place_on_a1(&mut h, &[S1], link_open);
        decs(&h, m)[0].reject
    };
    let need = |lev: &str| -> i64 {
        let h = steady();
        let e = h.s.engine();
        let rr = e.risk.view_for(AC1, B1, &e.now).rr_max_cross;
        let mid = Px::mid_floor(px("50010"), px("50011"));
        let capx = mid.mul_rate(rr, RoundMode::Ceil);
        let exp = e.rd.asset(e.rd.inst(B1).asset).exp;
        let ln = hft_risk::Risk::notional(q("0.010"), capx, exp, hft_types::Rate(1_000_000_000));
        let lev = r(lev).0 as i128;
        let fb = i128::from(e.risk.params().accounts.as_slice()[0].funds_buffer.0);
        // ⌈link_notional × (1 + funds_buffer) / leverage⌉ (hft-risk #7b ②).
        let num = i128::from(ln.0) * (1_000_000_000 + fb);
        ((num + lev - 1) / lev) as i64
    };
    let n3 = need("3.00");
    assert_eq!(probe(n3, None), 0, "LK-2.4: exactly enough");
    assert_eq!(probe(n3 - 1, None), RejectBits::LINK_FUNDS.0, "LK-2.5");
    // LK-2.6/2.7: a lower leverage (2×) needs more; the 3× amount no longer suffices.
    let n2 = need("2.00");
    assert!(n2 > n3);
    assert_eq!(probe(n3, Some("2.00")), RejectBits::LINK_FUNDS.0, "LK-2.6");
    assert_eq!(probe(n2, Some("2.00")), 0, "LK-2.6 at the new need");
    assert_eq!(probe(n3, Some("3.00")), 0, "LK-2.7");
}

/// LK-2 main / .1 / .2 / .3 with a spot second leg: first leg b1 sell, second leg a1 buy; the
/// a1 side of account 1 must hold `link_notional × (1 + funds_buffer)` of free USDT net of its
/// in-flight spot buys.
#[test]
fn lk_2_spot_second_leg_needs_free_quote() {
    fn b1_first_a1_second(hx: &Hx<'_>) -> OrderIntent {
        let mut it = b1_sell_open(hx);
        it.qty = q("0.01");
        it.link = hft_strategy_sdk::LegLinkOpt::some(hft_strategy_sdk::LegLink {
            group: G1,
            role: LinkRole::First,
            _pad: [0; 3],
            tolerance: r("0.02"),
            hedge: onfill(hx.s.account, A1, Side::Buy),
        });
        it
    }
    // The trigger quote of `place_on_a1` is a1 50 000 / 50 003.
    let need = {
        let h = steady();
        let e = h.s.engine();
        let rr = e.risk.view_for(AC1, A1, &e.now).rr_max_cross;
        let capx = Px::mid_floor(px("50000"), px("50003")).mul_rate(rr, RoundMode::Ceil);
        let exp = e.rd.asset(e.rd.inst(A1).asset).exp;
        let ln = hft_risk::Risk::notional(q("0.010"), capx, exp, hft_types::Rate(1_000_000_000));
        let fb = i128::from(e.risk.params().accounts.as_slice()[0].funds_buffer.0);
        let num = i128::from(ln.0) * (1_000_000_000 + fb);
        ((num + 999_999_999) / 1_000_000_000) as i64
    };
    // `free` in USDT units with the quote asset's qty exponent.
    let free_qty = |h: &H, notional_raw: i64| -> Qty {
        let e = h.rd.asset(USDT).exp.qty_exp;
        Qty(notional_raw * 10i64.pow((-6 - i32::from(e)) as u32))
    };
    let probe = |extra: i64, inflight: bool| -> u32 {
        // The first balance of (A, account 1, USDT) initialises the ledger's cash (later reports
        // only reconcile), so it goes in before the steady-state bring-up.
        reset_script();
        let mut h = b0().boot();
        let br = hft_event::BalanceReport {
            venue: A,
            account: AC1,
            asset: USDT,
            _pad: 0,
            free: free_qty(&h, need + extra),
            locked: Qty(0),
            ex_ts_ns: 0,
            rx_tsc: hft_types::Tsc(h.now()),
        };
        h.push(RingId::Rep(0), &br);
        h.steady();
        if inflight {
            // An acknowledged a1 buy 0.001 IOC in flight (≈ 50 USDT of the free quote).
            let m = place_on_a1(&mut h, &[S1], |hx| {
                let mut it = open_buy(hx, A1);
                it.qty = q("0.001");
                it
            });
            let d = decs(&h, m)[0];
            assert_eq!(d.reject, 0);
            h.ack(&d);
        }
        let m = place_on_a1(&mut h, &[S1], b1_first_a1_second);
        decs(&h, m)
            .into_iter()
            .find(|d| d.inst == B1)
            .expect("first leg")
            .reject
    };
    assert_eq!(probe(-1, false), RejectBits::LINK_FUNDS.0, "LK-2 main");
    assert_eq!(probe(0, false), 0, "LK-2.1: exactly link_notional × 1.05");
    assert_eq!(probe(1, false), 0, "LK-2.2");
    assert_eq!(
        probe(30_000_000, true),
        RejectBits::LINK_FUNDS.0,
        "LK-2.3: in-flight buy counts"
    );
}

/// B2.1: a closing link (`HedgePlan{reduce_only = 1}`): the second leg and its `SecondRetry`
/// carry both `RISK_REDUCING` and `REDUCE_ONLY`.
#[test]
fn b2_1_closing_link_second_legs_reduce_only() {
    fn close_link(hx: &Hx<'_>) -> OrderIntent {
        let b = hx.v.bbo(A1);
        let mut it = intent(
            hx,
            A1,
            Side::Sell,
            hft_types::Tif::Ioc,
            hft_strategy_sdk::IntentClass::Reduce,
            b.bid_px,
            q("0.01"),
        );
        it.group = G1;
        let mut plan = onfill(hx.s.account, B1, Side::Buy);
        plan.reduce_only = 1;
        it.link = hft_strategy_sdk::LegLinkOpt::some(hft_strategy_sdk::LegLink {
            group: G1,
            role: LinkRole::First,
            _pad: [0; 3],
            tolerance: r("0.02"),
            hedge: plan,
        });
        it
    }
    let mut h = steady();
    hedged_position(&mut h);
    let m = place_on_a1(&mut h, &[S1], close_link);
    let first = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == A1)
        .expect("closing first leg");
    assert_eq!(first.reject, 0, "{first:?}");
    h.ack(&first);
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    assert_eq!((second.side, second.reject), (Side::Buy, 0));
    let both = OrderFlags::RISK_REDUCING.0 | OrderFlags::REDUCE_ONLY.0;
    assert_eq!(second.oflags & both, both);
    h.ack(&second);
    expire(&mut h, &second);
    let due = hedge_retry_timers(&h)[0].0;
    let m = h.mark();
    run_to(&mut h, due);
    let retry = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("SecondRetry");
    assert_eq!((role(&retry), retry.reject), (LinkRole::SecondRetry as u8, 0));
    assert_eq!(retry.oflags & both, both);
}

/// B6.2: with the first leg out, one-leg b1 opens use (B, a)'s 10 s window down to exactly its
/// reserve (0.20 × 100 = 20 left): the next b1 Open is `RATE`, the first leg's fill still sends
/// the risk-reducing second leg.
#[test]
fn b6_2_second_leg_uses_rate_reserve() {
    reset_script();
    let mut b0 = b0();
    b0.caps.max_timers = 8_192;
    b0.setup_edit.push(|s| s.oms.slots = hft_oms::PerTable::new(128));
    let mut h = b0.boot();
    h.steady();
    let first = open_first(&mut h);
    // 80 one-leg b1 opens (the linked first leg only checked B's bucket): 100 − 80 = 20 left.
    for k in 0..80 {
        let m = place_on_a1(&mut h, &[S1], b1_sell_open);
        let d = decs(&h, m)[0];
        assert_eq!(d.reject, 0, "b1 open #{k}");
        let ev = order_event(&d, OrderEventKind::Reject, h.epoch());
        h.push(RingId::Rep(0), &ev);
        h.run();
    }
    let m = place_on_a1(&mut h, &[S1], b1_sell_open);
    assert_eq!(decs(&h, m)[0].reject, RejectBits::RATE.0);
    let n = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    assert_eq!(second.reject, 0);
    assert!(second.oflags & OrderFlags::RISK_REDUCING.0 != 0);
    assert!(frames(&h, n).iter().any(|f| f.1.inst == B1));
}

// ------------------------------------------------------------------------------------- LK-3

/// `HedgePlan{px_mode Cross, px_param 0.002, max_slip 0.001}` on the a1/b1 link.
fn link_open_slip(hx: &Hx<'_>) -> OrderIntent {
    let mut it = open_buy(hx, A1);
    it.qty = q("0.01");
    let mut plan = onfill(hx.s.account, B1, Side::Sell);
    plan.px_param = r("0.002");
    plan.max_slip = r("0.001");
    it.link = hft_strategy_sdk::LegLinkOpt::some(hft_strategy_sdk::LegLink {
        group: it.group,
        role: LinkRole::First,
        _pad: [0; 3],
        tolerance: r("0.02"),
        hedge: plan,
    });
    it
}

fn sell_cap(h: &H, bid: &str, ask: &str) -> Px {
    let e = h.s.engine();
    let rr = e.risk.hedge_px_limits(AC1, B, B1).rr_max_cross;
    let tick = h.rd.inst(B1).tick;
    Px::mid_floor(px(bid), px(ask))
        .mul_rate(hft_types::Rate(-rr.0), RoundMode::Ceil)
        .round_to_tick(tick, RoundMode::Ceil)
}

#[test]
fn lk_3_slip_capped_beyond_cross_gated() {
    // Main: ref 50 010; b1 moves 0.4 % (> max_slip, < rr_max_cross of the mid) before the fill.
    let mut h = steady();
    h.quote(B1, "50010", "50010.5");
    h.run();
    let m = place_on_a1(&mut h, &[S1], link_open_slip);
    let first = decs(&h, m)[0];
    assert_eq!(first.reject, 0);
    h.ack(&first);
    h.quote(B1, "49810", "49810.5");
    h.run();
    let n = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    let cap = sell_cap(&h, "49810", "49810.5");
    assert_eq!(
        (second.reject, second.px, second.tif),
        (0, cap, hft_types::Tif::Ioc)
    );
    assert!(frames(&h, n).iter().any(|f| f.1.inst == B1 && f.1.px == cap));
    let sc = notes_of(&h, m, notes::HEDGE_SLIP_CAPPED);
    assert_eq!(sc.len(), 1);
    let l = last_link(&h, AC1).expect("link");
    assert_eq!(
        sc[0].v[..4],
        [
            u64::from(l.link_id),
            px("50010").0 as u64,
            px("49810").0 as u64,
            cap.0 as u64
        ]
    );
    assert_eq!(l.retries, 0);
    assert!(hedge_retry_timers(&h).is_empty());
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Done));

    // LK-3.1 / 3.2: bid below mid × (1 − rr) → gated BeyondCross (HedgeRetry, retries unchanged);
    // .1 the book recovers and the first retry goes out as SecondRetry, no Broken; .2 it never
    // recovers: no Broken before hedge_deadline_ms, Broken{Deadline} + OmsHedgeBroken after.
    for recover in [true, false] {
        let mut h = steady();
        let m = place_on_a1(&mut h, &[S1], link_open_slip);
        let first = decs(&h, m)[0];
        h.ack(&first);
        h.quote(B1, "48000", "52000");
        h.run();
        let m = h.mark();
        let t_fill = h.now();
        h.fill(&first, first.qty, first.qty, first.px);
        assert!(
            decs(&h, m).iter().all(|d| d.inst != B1),
            "gated: no second leg decision"
        );
        assert_eq!(notes_of(&h, m, notes::HEDGE_PX_GATED).len(), 1);
        assert_eq!(hedge_retry_timers(&h).len(), 1);
        assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(0));
        if recover {
            run_to(&mut h, t_fill + 800 * MS);
            h.quote(B1, "50010", "50011");
            h.run();
            let m = h.mark();
            run_to(&mut h, t_fill + 2_000 * MS);
            let r = decs(&h, m)
                .into_iter()
                .find(|d| d.inst == B1)
                .expect("retry after recovery");
            assert_eq!((role(&r), r.reject), (LinkRole::SecondRetry as u8, 0));
            assert_ne!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Broken));
        } else {
            let m = h.mark();
            run_to(&mut h, t_fill + 2_900 * MS);
            assert!(cap_sets(&h, m).is_empty(), "no Broken before the deadline");
            assert_ne!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Broken));
            run_to(&mut h, t_fill + 3_500 * MS);
            assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Broken));
            let hb = notes_of(&h, m, notes::HEDGE_BROKEN);
            assert_eq!(hb.len(), 1);
            assert_eq!(hb[0].v[1], LinkBroken::Deadline as u64, "{:?}", hb[0]);
        }
    }
}

// ------------------------------------------------------------------------------------- FP-1

fn send_report(d: &OrderDecision, op: hft_event::GwOp, seq: u64, t: u64) -> hft_event::SendReport {
    send_report_st(d, op, seq, t, hft_event::SendStatus::Written)
}

fn send_report_st(
    d: &OrderDecision,
    op: hft_event::GwOp,
    seq: u64,
    t: u64,
    status: hft_event::SendStatus,
) -> hft_event::SendReport {
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
        status,
        op,
        _pad: [0; 3],
        os_errno: 0,
        _pad2: 0,
    }
}

/// `ConnState` of (v, a)'s order connections `orders` and its user stream.
fn order_conns(h: &mut H, v: hft_types::VenueId, a: AccountId, st: ConnStateKind, orders: &[u8]) {
    let t = h.now();
    for k in orders {
        h.push(RingId::Rep(0), &conn(v, *k, st, ConnRole::Order, InstId(0), a, t));
    }
    h.push(
        RingId::Rep(0),
        &conn(v, 0, st, ConnRole::UserStream, InstId(0), a, t),
    );
    h.run();
}

fn slot_state(h: &H, d: &OrderDecision) -> Option<hft_oms::OrderState> {
    let r = hft_strategy_sdk::OrderRef::new(d.venue, d.account, d.slot.0, d.r#gen);
    h.s.engine().oms.slot(r).map(|s| s.state)
}

/// FP-1 preamble: the linked open is sent (`SendReport{Place, Written}`), the venue fills it
/// but every report is lost, then (A, a)'s order connections and user stream drop. Returns the
/// first leg and the time it was sent.
fn fp1_lost_reports(h: &mut H) -> (OrderDecision, u64) {
    let m = place_on_a1(h, &[S1], link_open);
    let (seq, first) = decisions(&h.since(m))[0];
    assert_eq!(first.reject, 0);
    let t0 = h.now();
    h.push(
        RingId::Rep(0),
        &send_report(&first, hft_event::GwOp::Place, seq, t0),
    );
    h.run();
    h.adv(3 * MS);
    order_conns(h, A, AC1, ConnStateKind::Down, &[0, 1]);
    (first, t0)
}

/// One `Query` per `ConnState{Order, Up}` call; returns its decision seq.
fn up_and_query(h: &mut H, first: &OrderDecision) -> u64 {
    let m = h.mark();
    order_conns(h, A, AC1, ConnStateKind::Up, &[0]);
    let qs: Vec<_> = decisions(&h.since(m))
        .into_iter()
        .filter(|d| d.1.action == OrderAction::Query)
        .collect();
    assert_eq!(qs.len(), 1, "{qs:?}");
    assert_eq!(qs[0].1.slot, first.slot);
    qs[0].0
}

#[test]
fn fp_1_conn_down_no_ack_converges_by_query() {
    let mut h = steady();
    let m = h.mark();
    let (first, t0) = fp1_lost_reports(&mut h);
    assert_eq!(slot_state(&h, &first), Some(hft_oms::OrderState::Unknown));
    let un = notes_of(&h, m, notes::UNKNOWN);
    assert_eq!(un.len(), 1);
    assert_eq!(un[0].level, hft_event::NoteLevel::P1);
    assert!(!h.s.engine().oms.venue_ready(A, AC1));
    let lv = h.s.engine().oms.legs(AC1, G1).view;
    assert!(lv.has_unknown);
    assert_eq!(lv.leg[0].inflight_immediate, first.qty);
    assert!(
        decs(&h, m).iter().all(|d| d.action != OrderAction::Query),
        "no Query while down"
    );
    // A new open while down → VENUE_NOT_READY (only bit), not out.
    h.at(t0 + SEC);
    let n = h.io_ref().out.len();
    let m = place_on_a1(&mut h, &[S1], link_open);
    let d = decs(&h, m);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].reject, RejectBits::VENUE_NOT_READY.0);
    assert!(frames(&h, n).is_empty());
    // Unknown queries fall due while down: nothing sent, the slot stays Unknown, no ledger.
    // (The market connections keep answering pings, as W1's BBO connections do.)
    let m = h.mark();
    keep_alive_to(&mut h, t0 + 3 * SEC);
    assert!(decs(&h, m).iter().all(|d| d.action != OrderAction::Query));
    assert_eq!(slot_state(&h, &first), Some(hft_oms::OrderState::Unknown));
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, Qty(0));
    // Reconnect: exactly one Query in the Up Input; its SendReport counts it.
    let qseq = up_and_query(&mut h, &first);
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &send_report(&first, hft_event::GwOp::Query, qseq, t),
    );
    h.run();
    // QueryResult Filled → Filled, second leg in the same Input, venue ready again.
    let mut ev = order_event(&first, OrderEventKind::QueryResult, h.epoch());
    ev.detail = 3;
    ev.cum_filled = first.qty;
    ev.avg_px = first.px;
    let m = h.mark();
    h.push(RingId::Rep(0), &ev);
    h.run();
    assert_eq!(slot_state(&h, &first), Some(hft_oms::OrderState::Filled));
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg in the QueryResult Input");
    assert_eq!(
        (second.side, second.qty, second.tif, role(&second), second.reject),
        (
            Side::Sell,
            first.qty,
            hft_types::Tif::Ioc,
            LinkRole::Second as u8,
            0
        )
    );
    assert!(second.oflags & OrderFlags::RISK_REDUCING.0 != 0);
    assert!(second.oflags & OrderFlags::REDUCE_ONLY.0 == 0);
    assert_eq!(notes_of(&h, m, notes::VENUE_READY).len(), 1);
    assert!(h.s.engine().oms.venue_ready(A, AC1));
    assert!(!h.s.engine().oms.legs(AC1, G1).view.has_unknown);
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, first.qty);
}

/// FP-1.1: the IOC expired unfilled → Cancelled, link Done, no second leg, ledger unchanged.
#[test]
fn fp_1_1_query_finds_expired_ioc() {
    let mut h = steady();
    let (first, t0) = fp1_lost_reports(&mut h);
    run_to(&mut h, t0 + 3 * SEC);
    let m = h.mark();
    let qseq = up_and_query(&mut h, &first);
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &send_report(&first, hft_event::GwOp::Query, qseq, t),
    );
    let mut ev = order_event(&first, OrderEventKind::QueryResult, h.epoch());
    ev.detail = 4;
    h.push(RingId::Rep(0), &ev);
    h.run();
    assert_eq!(slot_state(&h, &first), Some(hft_oms::OrderState::Cancelled));
    assert!(decs(&h, m).iter().all(|d| role(d) != LinkRole::Second as u8));
    assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Done));
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, Qty(0));
    assert_eq!(notes_of(&h, m, notes::VENUE_READY).len(), 1);
}

/// FP-1.2: down for 40 s → within [30.004 s, 31.004 s] a P0 `OmsUnknown` and the
/// `UnknownTimeout{a, A}` cap; after convergence the cap still blocks Opens on A until
/// `Resume{UnknownTimeout}` applies.
#[test]
fn fp_1_2_long_outage_sets_unknown_timeout_cap() {
    let mut h = steady();
    let (first, t0) = fp1_lost_reports(&mut h);
    let m = h.mark();
    run_to(&mut h, t0 + 31_004 * MS);
    let p0: Vec<_> = notes_of(&h, m, notes::UNKNOWN)
        .into_iter()
        .filter(|n| n.level == hft_event::NoteLevel::P0)
        .collect();
    assert_eq!(p0.len(), 1, "one P0 OmsUnknown");
    assert!(
        cap_sets(&h, m)
            .iter()
            .any(|c| c.source == CapSource::UNKNOWN_TIMEOUT && c.scope.account == AC1 && c.scope.venue == A)
    );
    run_to(&mut h, t0 + 40 * SEC);
    // Market connections went quiet during the outage; fresh evidence for the later checks.
    for v in [A, B] {
        let t = h.now();
        h.push(RingId::Rep(0), &md(v, ConnStateKind::Alive, t));
    }
    h.quote(B1, "50010", "50011");
    h.run();
    let qseq = up_and_query(&mut h, &first);
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &send_report(&first, hft_event::GwOp::Query, qseq, t),
    );
    let mut ev = order_event(&first, OrderEventKind::QueryResult, h.epoch());
    ev.detail = 3;
    ev.cum_filled = first.qty;
    ev.avg_px = first.px;
    h.push(RingId::Rep(0), &ev);
    h.run();
    assert_eq!(slot_state(&h, &first), Some(hft_oms::OrderState::Filled));
    // The cap persists: an a1 Open of account 1 is blocked.
    let m = place_on_a1(&mut h, &[S1], a1_buy_open);
    assert_eq!(decs(&h, m)[0].reject, RejectBits::CLOSE_ONLY.0);
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::Resume {
            scope: hft_event::CmdScope {
                strategy: StrategyId(0),
                venue: A,
                account: AC1,
                _pad: 0,
                group: hft_types::GROUP_ALL,
                inst: InstId(0),
                _pad2: 0,
            },
            source: CapSource::UNKNOWN_TIMEOUT,
        },
    );
    h.run();
    assert_eq!(
        h.res(id).0,
        hft_sequencer::codes::status::APPLIED,
        "{:?}",
        h.res(id)
    );
    assert!(!has_cap(&h, CapSource::UNKNOWN_TIMEOUT, AC1, None));
}

/// FP-1.3: queries never answered → one `Query` per `unknown_query_interval_ms`; after the
/// sixth `SendReport{Query, Written}` (`query_count` 6 > `unknown_max_queries` 5) exactly one
/// P0 `OmsUnknownStuck`; the slot stays Unknown.
#[test]
fn fp_1_3_unanswered_queries_report_stuck() {
    let mut h = steady();
    let (first, t0) = fp1_lost_reports(&mut h);
    run_to(&mut h, t0 + 3 * SEC);
    let m0 = h.mark();
    let mut sent = 0;
    let mut qseq = up_and_query(&mut h, &first);
    loop {
        let t = h.now();
        h.push(
            RingId::Rep(0),
            &send_report(&first, hft_event::GwOp::Query, qseq, t),
        );
        h.run();
        sent += 1;
        if sent == 6 {
            break;
        }
        let m = h.mark();
        let t = h.now();
        run_to(&mut h, t + 1_000 * MS);
        let qs: Vec<_> = decisions(&h.since(m))
            .into_iter()
            .filter(|d| d.1.action == OrderAction::Query)
            .collect();
        assert_eq!(qs.len(), 1, "one Query per interval: {qs:?}");
        qseq = qs[0].0;
    }
    let stuck = notes_of(&h, m0, notes::UNKNOWN_STUCK);
    assert_eq!(stuck.len(), 1);
    assert_eq!(stuck[0].level, hft_event::NoteLevel::P0);
    assert_eq!(slot_state(&h, &first), Some(hft_oms::OrderState::Unknown));
    let t = h.now();
    run_to(&mut h, t + 3 * SEC);
    assert_eq!(notes_of(&h, m0, notes::UNKNOWN_STUCK).len(), 1, "written once");
}

// ------------------------------------------------------------------------------------- FP-2

/// A Reduce of g1's a1 long (sell 0.005 IOC at the bid).
fn a1_reduce(hx: &Hx<'_>) -> OrderIntent {
    let b = hx.v.bbo(A1);
    let mut it = intent(
        hx,
        A1,
        Side::Sell,
        hft_types::Tif::Ioc,
        hft_strategy_sdk::IntentClass::Reduce,
        b.bid_px,
        q("0.005"),
    );
    it.group = G1;
    it
}

/// g1 holds a1 +0.01 / b1 −0.01 (a completed link).
fn hedged_position(h: &mut H) {
    let first = open_first(h);
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(h, m).into_iter().find(|d| d.inst == B1).expect("second leg");
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
}

fn rate_hits(h: &H, m: usize) -> usize {
    h.notes_since(m)
        .iter()
        .filter(|n| n.source == NoteSource::Risk && n.code & 0xFFFF == 1)
        .count()
}

#[test]
fn fp_2_rate_limit_drains_bucket_and_backs_off() {
    let mut h = steady();
    hedged_position(&mut h);
    let m = h.mark();
    let d = rate_limit_hit(&mut h, A1);
    assert_eq!(slot_state(&h, &d), Some(hft_oms::OrderState::Rejected));
    assert_eq!(rate_hits(&h, m), 1, "one RateLimitHit");
    let e = h.s.engine();
    let t0 = e.now.engine_ns;
    assert!(e.risk.view_for(AC1, A1, &e.now).rate_backoff);
    assert!(
        e.oms.venue_ready(A, AC1),
        "rate limit is not a connection failure"
    );
    assert_eq!(
        e.risk.backoff_until(A, AC1),
        t0 + 1_000_000_000,
        "k = 0: 1 000 ms"
    );
    let tsc0 = h.now();
    // +0.5 s, inside the backoff: the Open and the Reduce are both RATE_BACKOFF (hft-risk #8
    // checks the backoff before the buckets); neither goes out.
    keep_alive_to(&mut h, tsc0 + 500 * MS);
    let n = h.io_ref().out.len();
    let m = place_on_a1(&mut h, &[S1], a1_buy_open);
    assert_eq!(decs(&h, m)[0].reject, RejectBits::RATE_BACKOFF.0);
    let m = place_on_a1(&mut h, &[S1], a1_reduce);
    assert_eq!(decs(&h, m)[0].reject, RejectBits::RATE_BACKOFF.0);
    assert!(frames(&h, n).is_empty());
    // +1.5 s: backoff over; the Reduce (risk-reducing, may use the reserve) goes out, the Open is
    // still RATE (≈ 15 tokens refilled, not above the 20 % reserve).
    keep_alive_to(&mut h, tsc0 + 1_500 * MS);
    let e = h.s.engine();
    assert!(!e.risk.view_for(AC1, A1, &e.now).rate_backoff);
    let n = h.io_ref().out.len();
    let m = place_on_a1(&mut h, &[S1], a1_reduce);
    let red = decs(&h, m)[0];
    assert_eq!(red.reject, 0);
    assert_eq!(frames(&h, n).len(), 1);
    h.ack(&red);
    expire(&mut h, &red);
    let m = place_on_a1(&mut h, &[S1], a1_buy_open);
    assert_eq!(decs(&h, m)[0].reject, RejectBits::RATE.0);
    // +13 s: both windows above their reserve → the Open goes out.
    keep_alive_to(&mut h, tsc0 + 13 * SEC);
    let m = place_on_a1(&mut h, &[S1], a1_buy_open);
    assert_eq!(decs(&h, m)[0].reject, 0);
}

/// FP-2.1: a second hit 0.2 s after the first backoff ended → k = 1, 2 000 ms, another note.
#[test]
fn fp_2_1_second_hit_doubles_backoff() {
    let mut h = steady();
    // Two a1 IOCs out while the buckets are full; the venue rejects one now, one at +1.2 s.
    let m = place_on_a1(&mut h, &[S1], a1_buy_open);
    let d1 = decs(&h, m)[0];
    let m = place_on_a1(&mut h, &[S1], a1_buy_open);
    let d2 = decs(&h, m)[0];
    assert_eq!((d1.reject, d2.reject), (0, 0));
    venue_rate_reject(&mut h, &d1);
    let tsc0 = h.now();
    keep_alive_to(&mut h, tsc0 + 1_200 * MS);
    let m = h.mark();
    venue_rate_reject(&mut h, &d2);
    assert_eq!(rate_hits(&h, m), 1);
    let e = h.s.engine();
    assert_eq!(e.risk.backoff_until(A, AC1), e.now.engine_ns + 2_000_000_000);
}

/// FP-2.2: a link whose second leg (a1 buy) falls in A's backoff: `RATE_BACKOFF`, gated through
/// `hedge_gated` with the retry at max(fill + 100 ms, backoff end), `retries` 0; the
/// `SecondRetry` at the backoff end goes out.
#[test]
fn fp_2_2_second_leg_in_backoff_retries_at_backoff_end() {
    fn b1_first(hx: &Hx<'_>) -> OrderIntent {
        let mut it = b1_sell_open(hx);
        it.qty = q("0.01");
        it.link = hft_strategy_sdk::LegLinkOpt::some(hft_strategy_sdk::LegLink {
            group: G1,
            role: LinkRole::First,
            _pad: [0; 3],
            tolerance: r("0.02"),
            hedge: onfill(hx.s.account, A1, Side::Buy),
        });
        it
    }
    let mut h = steady();
    let m = place_on_a1(&mut h, &[S1], b1_first);
    let first = decs(&h, m).into_iter().find(|d| d.inst == B1).expect("first leg");
    assert_eq!(first.reject, 0, "{first:?}");
    h.ack(&first);
    rate_limit_hit(&mut h, A1);
    let e = h.s.engine();
    let until_tsc = e.now.tsc_of(e.risk.backoff_until(A, AC1)).0;
    h.adv(200 * MS);
    let m = h.mark();
    let t_fill = h.now();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == A1)
        .expect("second leg decision");
    assert_eq!(second.reject, RejectBits::RATE_BACKOFF.0);
    let timers = hedge_retry_timers(&h);
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0].0, until_tsc.max(t_fill + 100 * MS));
    assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(0));
    let m = h.mark();
    keep_alive_to(&mut h, timers[0].0);
    let r = decs(&h, m).into_iter().find(|d| d.inst == A1).expect("retry");
    assert_eq!((role(&r), r.reject), (LinkRole::SecondRetry as u8, 0));
}

/// FP-2.3: the gateway's own send budget answers `SendReport{Place, RateLimited}`: the slot is
/// `Rejected` (`reject_code` 3), no `RateLimitHit`, the buckets are not cleared; a second leg so
/// refused is gated (not counted).
#[test]
fn fp_2_3_gateway_rate_limited_send_is_local() {
    let mut h = steady();
    let m = place_on_a1(&mut h, &[S1], a1_buy_open);
    let (seq, d) = decisions(&h.since(m))[0];
    let t = h.now();
    let m = h.mark();
    h.push(
        RingId::Rep(0),
        &send_report_st(
            &d,
            hft_event::GwOp::Place,
            seq,
            t,
            hft_event::SendStatus::RateLimited,
        ),
    );
    h.run();
    assert_eq!(slot_state(&h, &d), Some(hft_oms::OrderState::Rejected));
    let r = hft_strategy_sdk::OrderRef::new(d.venue, d.account, d.slot.0, d.r#gen);
    assert_eq!(h.s.engine().oms.slot(r).map(|s| s.reject_code), Some(3));
    assert_eq!(rate_hits(&h, m), 0);
    let e = h.s.engine();
    assert!(!e.risk.view_for(AC1, A1, &e.now).rate_backoff);
    let m = place_on_a1(&mut h, &[S1], a1_buy_open);
    assert_eq!(decs(&h, m)[0].reject, 0, "buckets not cleared");
    // A second leg refused by the gateway budget → gated, retries unchanged.
    let mut h = steady();
    let first = open_first(&mut h);
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let (seq, second) = decisions(&h.since(m))
        .into_iter()
        .find(|d| d.1.inst == B1)
        .expect("second leg");
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &send_report_st(
            &second,
            hft_event::GwOp::Place,
            seq,
            t,
            hft_event::SendStatus::RateLimited,
        ),
    );
    h.run();
    assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(0));
    assert_eq!(hedge_retry_timers(&h).len(), 1);
}
