//! Cross-component risk / kill / Broken / mode scenarios on the real engine (docs/tests/scenarios.md
//! §4, §5, §7: MD-2–MD-4, B-10, BK-1–BK-3, BK-7, FL-1), run as deterministic step sequences on
//! the B0 rig (the same approach as `scenarios.rs`; helpers in `common/risk_kit.rs`).
//!
//! Mapping to world W1: venue A ≙ `VS` (spot), B ≙ `VP` (USDT perp), a1 ≙ `S1`, b1 ≙ `P1`,
//! account 1 / instance s1 / g1 ≙ `a` / instance 1 / `g1`, account 2 / s2 / g2 ≙ `b` / 2 / `g2`.
//! B0 prices replace W1's (a1 50 000.00 / 50 001.00, b1 50 010 / 50 011); limits and sizes are
//! read back from the engine where a case computes them. `probe-taker`'s once-a-second Open is
//! represented by an Open at each instant the case asserts on. Times are relative to the
//! scenario start `t0` (TSC = ns in B0).

use hft_event::{ControlCmd, OrderFlags};
use hft_sequencer::codes::{detail, status};
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, IntentClass, LinkRole};
use hft_types::{AccountId, CapSource, GroupId, Notional, Px, Qty, RejectBits, RunMode, Side, StrategyId};

use crate::common::risk_kit::*;
use crate::common::*;

fn boot_with(f: impl FnOnce(&mut B0)) -> H {
    reset_script();
    let mut b = b0();
    f(&mut b);
    let mut h = b.boot();
    h.steady();
    h
}

/// B0 risk parameters with account `a`'s limits edited by `f`.
fn risk_acct(b: &mut B0, a: AccountId, f: impl Fn(&mut hft_risk::AcctLimits)) {
    let rd = b.rd.load();
    let mut p = b.risk.clone().unwrap_or_else(|| b0_risk(&rd));
    for x in p.accounts.as_mut_slice() {
        if x.account == a {
            f(x);
        }
    }
    b.risk = Some(p);
}

/// Steps to `t` (absolute TSC) in ≤ 500 ms steps with the market connections alive.
/// Every second b1's mark (unchanged) and both accounts' margin reports arrive, as W1's
/// `margin_data = Push` venue sends them.
fn live_to(h: &mut H, t: u64) {
    while h.now() < t {
        let d = (t - h.now()).min(500 * MS);
        h.adv(d);
        md_alive(h);
        if h.now() / SEC != (h.now() - d) / SEC {
            let mark = h.s.engine().market.row(B1).map_or(Px(0), |r| r.mark_px);
            if mark.0 > 0 {
                let f = format!("{}.{:02}", mark.0 / 100, mark.0 % 100);
                h.mark_md(B1, &f);
            }
            for a in [AC1, AC2] {
                let liq = LIQ_PX.with(std::cell::Cell::get);
                if a == AC1 && liq > 0 {
                    let l = format!("{}.{:02}", liq / 100, liq % 100);
                    margin_with(h, a, "0", Some(&l));
                } else {
                    h.margin_report(a, "100000", "0", "100000");
                }
            }
            h.run();
        }
    }
}

/// The day PnL the risk module sees (ledger day: realized + fees + funding + unrealized).
fn day(h: &H, a: AccountId) -> i64 {
    h.s.engine().ledger.account(a).day.net().0
}

/// Moves the a1 quote (both sides, whole ticks) so that account `a`'s day PnL is exactly
/// `target` (raw 1e-6 USDT); panics when the valuation step does not divide the distance.
fn set_day_pnl(h: &mut H, a: AccountId, target: i64) {
    let top = *h.s.engine().books.top(A1);
    let quote = |h: &mut H, k: i64| {
        let (b, s) = (Px(top.bid_px.0 + k), Px(top.ask_px.0 + k));
        let f = |p: Px| format!("{}.{:02}", p.0 / 100, p.0 % 100);
        h.quote(A1, &f(b), &f(s));
        h.run();
    };
    let d0 = day(h, a);
    quote(h, 1);
    let step = day(h, a) - d0;
    assert!(step > 0, "valuation step {step}");
    assert_eq!((target - d0) % step, 0, "target {target} from {d0}: step {step}");
    quote(h, (target - d0) / step);
    assert_eq!(day(h, a), target, "day PnL");
}

/// Moves both sides of the a1 quote by `ticks` (0.01).
fn shift_a1(h: &mut H, ticks: i64) {
    let top = *h.s.engine().books.top(A1);
    let f = |p: Px| format!("{}.{:02}", p.0 / 100, p.0 % 100);
    h.quote(A1, &f(Px(top.bid_px.0 + ticks)), &f(Px(top.ask_px.0 + ticks)));
    h.run();
}

/// Instance `s` opens a1 +0.1 / b1 −0.1 in its first group with one-leg IOCs; the second part of
/// the a1 fill pays a USDT fee of `y` × 1e-6 so that the day PnL can be placed on any 1e-6 USDT
/// (the a1 valuation alone moves in steps of 0.1 × 0.01). b1 is marked at 50 010.
fn exact_position(h: &mut H, s: StrategyId, y: i64) {
    let g = if s == S1 { G1 } else { G2 };
    h.mark_md(B1, "50010");
    let a1 = place_in(h, s, g, A1, Side::Buy, q("0.1"), IntentClass::Open);
    h.ack(&a1);
    let b1 = place_in(h, s, g, B1, Side::Sell, q("0.1"), IntentClass::Open);
    h.ack(&b1);
    assert_eq!((a1.reject, b1.reject), (0, 0));
    h.fill(&a1, q("0.0999"), q("0.0999"), a1.px);
    // The last part carries a USDT fee of `y` × 1e-6 USDT (USDT qty_exp −8).
    let mut ev = order_event(&a1, hft_event::OrderEventKind::Fill, h.epoch());
    ev.qty = q("0.0001");
    ev.cum_filled = q("0.1");
    ev.px = a1.px;
    ev.avg_px = a1.px;
    ev.fee = Qty(y * 100);
    ev.fee_asset = USDT;
    h.push(RingId::Rep(0), &ev);
    h.run();
    h.fill(&b1, b1.qty, b1.qty, b1.px);
    assert_eq!(h.s.engine().ledger.pos(account_of(h, s), A1).qty, q("0.1"));
}

/// `y` (fee, 1e-6 USDT) for [`exact_position`] such that the day PnL is ≡ `target` modulo the
/// a1 valuation step.
fn residue_for(edit: fn(&mut B0), target: i64) -> i64 {
    let mut h = boot_with(edit);
    exact_position(&mut h, S1, 0);
    let d = day(&h, AC1);
    let top = *h.s.engine().books.top(A1);
    let f = |p: Px| format!("{}.{:02}", p.0 / 100, p.0 % 100);
    h.quote(A1, &f(Px(top.bid_px.0 + 1)), &f(Px(top.ask_px.0 + 1)));
    h.run();
    let step = day(&h, AC1) - d;
    (d - target).rem_euclid(step)
}

/// An Open link of s1 (a1 buy 0.001, b1 sell link); returns its first-leg decision and whether
/// a frame went out.
fn open_link(h: &mut H) -> (hft_event::OrderDecision, bool) {
    let n = h.io_ref().out.len();
    let m = place_with(h, S1, |hx| pair_first(hx, q("0.001")));
    let d = decs_since(h, m)
        .into_iter()
        .find(|d| d.account == AC1 && d.inst == A1)
        .expect("Open link decision");
    let out = h.io_ref().out[n..]
        .iter()
        .any(|f| f.1.account == AC1 && f.1.inst == A1);
    (d, out)
}

fn caps_of(h: &H, a: AccountId) -> Vec<(CapSource, hft_types::VenueId, GroupId)> {
    h.s.engine()
        .risk
        .mode
        .caps()
        .iter()
        .filter(|c| c.scope.account == a)
        .map(|c| (c.source, c.scope.venue, c.scope.group))
        .collect()
}

fn mode_events(s: StrategyId) -> Vec<String> {
    log()
        .into_iter()
        .filter(|g| g.kind == "Mode" && g.s == s && !g.model)
        .map(|g| g.text)
        .collect()
}

fn set_mode(h: &mut H, mode: RunMode) -> (u8, u32) {
    run_cmd(
        h,
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode,
        },
    )
}

fn is_close_only(h: &H) -> bool {
    matches!(h.mode(S1, AC1, G1), hft_risk::mode::Effective::CloseOnly { .. })
}

/// A reconciliation group of (a, B): an optional b1 position difference (`exch − engine`,
/// notional `dn`) and its `Summary`, viewed at the current seq.
fn recon_b(h: &mut H, a: AccountId, id: u64, diff: Option<(&str, &str)>) {
    let view = h.s.engine().cur_seq;
    let count = if diff.is_some() { 2 } else { 1 };
    if let Some((d, dn)) = diff {
        let mut x = recon_summary(a, B, id, view);
        x.item = 1;
        x.inst = B1;
        x.idx = 0;
        x.count = count;
        x.engine_qty = h.s.engine().ledger.pos(a, B1).qty;
        x.exch_qty = Qty(x.engine_qty.0 + q(d).0);
        x.diff = n(dn);
        h.push(RingId::Rep(0), &x);
    }
    let mut s = recon_summary(a, B, id, view);
    s.idx = count - 1;
    s.count = count;
    h.push(RingId::Rep(0), &s);
    h.run();
}

// ===================================================================================== MD-2

fn md2_edit(b: &mut B0) {
    risk_acct(b, AC1, |x| {
        x.daily_loss_abs = n("50");
        x.daily_loss_kill = r("0.05");
        x.daily_loss_max_resumes = 1;
        x.recon_closeonly_notional = n("100");
    });
}

/// MD-2 up to t = 100 s (before the first Resume), with `alt_kill` = MD-2.1's account Kill in
/// place of the second reconciliation round. Returns (harness, t0).
fn md2_until_100(alt_kill: bool) -> (H, u64) {
    let mut h = boot_with(md2_edit);
    exact_position(&mut h, S1, 0);
    clear_log();
    let t0 = h.now();
    // t = 10 s: a1 marked down → day PnL −60 USDT → DailyLoss{a}, trigger −50.
    live_to(&mut h, t0 + 10 * SEC);
    let m = h.mark();
    shift_a1(&mut h, -60_000);
    let dl: Vec<_> = cap_notes(&h, m, true)
        .into_iter()
        .filter(|c| c.source == CapSource::DAILY_LOSS && c.scope.account == AC1)
        .collect();
    assert_eq!(dl.len(), 1, "t = 10 s CapSet{{DailyLoss}}");
    assert_eq!(dl[0].params[0] as i64, -50_000_000, "params[0] = trigger");
    assert!(is_close_only(&h), "Live → CloseOnly");
    assert!(
        mode_events(S1)
            .iter()
            .any(|t| t.contains("old: Live") && t.contains("new: CloseOnly")),
        "{:?}",
        mode_events(S1)
    );
    // t = 20 s, 80 s: two rounds with the same b1 difference +0.010 (≈ 500 USDT) → Recon{a, B}.
    live_to(&mut h, t0 + 20 * SEC);
    recon_b(&mut h, AC1, 7_001, Some(("0.010", "500")));
    // t = 30 s: SetMode CloseOnly.
    live_to(&mut h, t0 + 30 * SEC);
    assert_eq!(set_mode(&mut h, RunMode::CloseOnly).0, status::APPLIED);
    live_to(&mut h, t0 + 80 * SEC);
    if alt_kill {
        let r = run_cmd(&mut h, AC1, &kill_account(AC1, false, false, true));
        assert_eq!(r.0, status::APPLIED);
    } else {
        let m = h.mark();
        recon_b(&mut h, AC1, 7_002, Some(("0.010", "500")));
        let rc: Vec<_> = cap_notes(&h, m, true)
            .into_iter()
            .filter(|c| c.source == CapSource::RECON)
            .collect();
        assert_eq!(rc.len(), 1, "t = 80 s CapSet{{Recon}}");
        assert_eq!((rc[0].scope.account, rc[0].scope.venue), (AC1, B));
        assert!(is_close_only(&h), "venue-level cap leaves the cell mode");
    }
    live_to(&mut h, t0 + 100 * SEC);
    (h, t0)
}

fn resume_recon(a: AccountId, v: hft_types::VenueId) -> ControlCmd<'static> {
    ControlCmd::Resume {
        scope: scope_av(a, v),
        source: CapSource::RECON,
    }
}

#[test]
fn md_2_resume_removes_exact_cap_only() {
    let (mut h, t0) = md2_until_100(false);
    // t = 100 s: Resume{DailyLoss} → only Recon{a, B} left; desired CloseOnly → still CloseOnly.
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::DAILY_LOSS));
    assert_eq!(r.0, status::APPLIED, "{r:?}");
    assert_eq!(
        caps_of(&h, AC1),
        vec![(CapSource::RECON, B, hft_types::GROUP_ALL)]
    );
    assert!(is_close_only(&h));
    // t = 110 s: SetMode Live → Applied, caps unchanged, CloseOnly → Live.
    live_to(&mut h, t0 + 110 * SEC);
    clear_log();
    assert_eq!(set_mode(&mut h, RunMode::Live).0, status::APPLIED);
    assert_eq!(
        caps_of(&h, AC1),
        vec![(CapSource::RECON, B, hft_types::GROUP_ALL)]
    );
    assert!(
        mode_events(S1)
            .iter()
            .any(|t| t.contains("old: CloseOnly") && t.contains("new: Live")),
        "{:?}",
        mode_events(S1)
    );
    // t ∈ [110 s, 210 s): every Open link RECON_DEVIATION (#3 on the link leg's venue), not out.
    let deviation = |h: &mut H, at: u64| {
        live_to(h, t0 + at * SEC);
        let (d, out) = open_link(h);
        assert_eq!(
            (d.reject, out),
            (RejectBits::RECON_DEVIATION.0, false),
            "t = {at} s"
        );
    };
    deviation(&mut h, 111);
    // t = 140 s: first Ignore round. t = 150 s: Resume refused (one round only), nothing changed.
    live_to(&mut h, t0 + 140 * SEC);
    recon_b(&mut h, AC1, 7_003, None);
    live_to(&mut h, t0 + 150 * SEC);
    let v = h.s.engine().risk.mode.ctl_state_version();
    let r = run_cmd(&mut h, AC1, &resume_recon(AC1, B));
    assert_eq!(
        r,
        (status::PRECONDITION_FAILED, u32::from(detail::RECON_NOT_CLEAN)),
        "t = 150 s"
    );
    assert_eq!(h.s.engine().risk.mode.ctl_state_version(), v);
    assert_eq!(
        caps_of(&h, AC1),
        vec![(CapSource::RECON, B, hft_types::GROUP_ALL)]
    );
    deviation(&mut h, 151);
    // t = 200 s: second Ignore round; t = 210 s Resume applies; t = 211 s the Open goes out.
    live_to(&mut h, t0 + 200 * SEC);
    recon_b(&mut h, AC1, 7_004, None);
    deviation(&mut h, 209);
    live_to(&mut h, t0 + 210 * SEC);
    let r = run_cmd(&mut h, AC1, &resume_recon(AC1, B));
    assert_eq!(r.0, status::APPLIED, "t = 210 s {r:?}");
    assert!(caps_of(&h, AC1).is_empty(), "{:?}", caps_of(&h, AC1));
    live_to(&mut h, t0 + 211 * SEC);
    let (d, out) = open_link(&mut h);
    assert_eq!((d.reject, out), (0, true), "t = 211 s");

    // MD-2.1: an account Kill instead of the second round; SetMode Live leaves CloseOnly, Opens
    // KILLED; Resume{Kill} at 210 s → CloseOnly → Live, Open out.
    let (mut h, t0) = md2_until_100(true);
    run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::DAILY_LOSS));
    live_to(&mut h, t0 + 110 * SEC);
    clear_log();
    assert_eq!(set_mode(&mut h, RunMode::Live).0, status::APPLIED);
    assert!(is_close_only(&h), "MD-2.1 still CloseOnly");
    assert!(mode_events(S1).is_empty(), "MD-2.1 no Mode event");
    live_to(&mut h, t0 + 111 * SEC);
    let (d, out) = open_link(&mut h);
    assert_eq!((d.reject, out), (RejectBits::KILLED.0, false), "MD-2.1");
    live_to(&mut h, t0 + 210 * SEC);
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::KILL));
    assert_eq!(r.0, status::APPLIED, "MD-2.1 {r:?}");
    assert!(
        mode_events(S1)
            .iter()
            .any(|t| t.contains("old: CloseOnly") && t.contains("new: Live"))
    );
    live_to(&mut h, t0 + 211 * SEC);
    let (d, out) = open_link(&mut h);
    assert_eq!((d.reject, out), (0, true), "MD-2.1 t = 211 s");

    // MD-2.2: a second Resume{DailyLoss} at 120 s → NoMatchingCap, version unchanged.
    let (mut h, t0) = md2_until_100(false);
    run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::DAILY_LOSS));
    live_to(&mut h, t0 + 120 * SEC);
    let v = h.s.engine().risk.mode.ctl_state_version();
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::DAILY_LOSS));
    assert_eq!(
        r,
        (status::PRECONDITION_FAILED, u32::from(detail::NO_MATCHING_CAP)),
        "MD-2.2"
    );
    assert_eq!(h.s.engine().risk.mode.ctl_state_version(), v);
    // MD-2.3: Resume{Recon, a, A} (scope mismatch) → NoMatchingCap, Recon{a, B} kept.
    live_to(&mut h, t0 + 210 * SEC);
    let r = run_cmd(&mut h, AC1, &resume_recon(AC1, A));
    assert_eq!(
        r,
        (status::PRECONDITION_FAILED, u32::from(detail::NO_MATCHING_CAP)),
        "MD-2.3"
    );
    assert!(cap_on(&h, CapSource::RECON, AC1, None));
    // MD-2.4: Resume{MarginStress, a} → InvalidArgs{NotResumable}.
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::MARGIN_STRESS));
    assert_eq!(
        r,
        (status::INVALID_ARGS, u32::from(detail::NOT_RESUMABLE)),
        "MD-2.4"
    );
}

// ===================================================================================== MD-3

fn daily_caps(h: &H, m: usize) -> Vec<hft_event::CapRecord> {
    cap_notes(h, m, true)
        .into_iter()
        .filter(|c| c.source == CapSource::DAILY_LOSS && c.scope.account == AC1)
        .collect()
}

/// MD-3's world with the day PnL placeable on `target` (see [`exact_position`]).
fn md3_world(target: i64) -> (H, u64) {
    let y = residue_for(md2_edit, target);
    let mut h = boot_with(md2_edit);
    exact_position(&mut h, S1, y);
    let t0 = h.now();
    (h, t0)
}

#[test]
fn md_3_daily_loss_resume_resets_trigger() {
    let (mut h, t0) = md3_world(-60_000_000);
    // t = 10 s: −60.00 → CapSet{DailyLoss}, params[0] = −50.000000 USDT.
    live_to(&mut h, t0 + 10 * SEC);
    let m = h.mark();
    set_day_pnl(&mut h, AC1, -60_000_000);
    let c = daily_caps(&h, m);
    assert_eq!(c.len(), 1, "t = 10 s");
    assert_eq!(c[0].params[0] as i64, -50_000_000);
    // t = 100 s: Resume → trigger = −60 − 50 = −110, resumes_today = 1.
    live_to(&mut h, t0 + 100 * SEC);
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::DAILY_LOSS));
    assert_eq!(r.0, status::APPLIED, "{r:?}");
    let st = h.s.engine().risk.daily_loss_state(AC1).expect("row");
    assert_eq!((st.0, st.1), (Notional(-110_000_000), 1), "t = 100 s");
    // t = 150 s: −109.99 > −110 → nothing.
    live_to(&mut h, t0 + 150 * SEC);
    let m = h.mark();
    set_day_pnl(&mut h, AC1, -109_990_000);
    assert!(daily_caps(&h, m).is_empty(), "t = 150 s");
    // t = 200 s: −110.01 → CapSet, params[0] = −110 USDT.
    live_to(&mut h, t0 + 200 * SEC);
    let m = h.mark();
    set_day_pnl(&mut h, AC1, -110_010_000);
    let c = daily_caps(&h, m);
    assert_eq!(c.len(), 1, "t = 200 s");
    assert_eq!(c[0].params[0] as i64, -110_000_000);
    // t = 300 s: the second Resume of the day → ResumeLimit, cap kept.
    live_to(&mut h, t0 + 300 * SEC);
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::DAILY_LOSS));
    assert_eq!(
        r,
        (status::PRECONDITION_FAILED, u32::from(detail::RESUME_LIMIT)),
        "t = 300 s"
    );
    assert!(cap_on(&h, CapSource::DAILY_LOSS, AC1, None));
    // UTC day boundary: Timer{Sys, DayRoll} → baseline 0, resumes 0, latch and CloseOnly kept.
    let next_day = h.s.engine().last_day_rolled + 1;
    let t = h.now();
    h.push(RingId::Hk, &sys_timer(3 | (next_day << 8), t));
    h.run();
    assert_eq!(h.s.engine().last_day_rolled, next_day, "day rolled");
    assert_eq!(day(&h, AC1), 0, "day PnL baseline 0");
    let st = h.s.engine().risk.daily_loss_state(AC1).expect("row");
    assert_eq!(st.1, 0, "resumes_today = 0");
    assert!(
        cap_on(&h, CapSource::DAILY_LOSS, AC1, None),
        "latch kept across the day roll"
    );
    assert!(is_close_only(&h));
    // 10 s later: Resume applies, cap removed, trigger = 0 − 50.
    live_to(&mut h, t + 10 * SEC);
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::DAILY_LOSS));
    assert_eq!(r.0, status::APPLIED, "{r:?}");
    assert!(!cap_on(&h, CapSource::DAILY_LOSS, AC1, None));
    let st = h.s.engine().risk.daily_loss_state(AC1).expect("row");
    assert_eq!(st.0, Notional(-50_000_000), "new trigger");

    // MD-3.1: exactly −50.000000 → no CapSet (strictly less).
    let (mut h, t0) = md3_world(-50_000_000);
    live_to(&mut h, t0 + 10 * SEC);
    let m = h.mark();
    set_day_pnl(&mut h, AC1, -50_000_000);
    assert!(daily_caps(&h, m).is_empty(), "MD-3.1");
    // MD-3.2: −50.000001 (1 raw lower) → CapSet, params[0] = −50 USDT.
    let (mut h, t0) = md3_world(-50_000_001);
    live_to(&mut h, t0 + 10 * SEC);
    let m = h.mark();
    set_day_pnl(&mut h, AC1, -50_000_001);
    let c = daily_caps(&h, m);
    assert_eq!(c.len(), 1, "MD-3.2");
    assert_eq!(c[0].params[0] as i64, -50_000_000);
}

// ===================================================================================== MD-4

thread_local! {
    /// b1 liquidation price of account 1's margin reports in `live_to` (0 = none).
    static LIQ_PX: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

fn set_liq(h: &mut H, liq: Option<&str>) {
    LIQ_PX.with(|c| c.set(liq.map_or(0, |l| px(l).0)));
    margin_with(h, AC1, "0", liq);
}

/// A one-leg a1 Open of s1 (buy 0.001 IOC at the ask): decision and whether it went out.
fn open_a1(h: &mut H) -> (hft_event::OrderDecision, bool) {
    let n = h.io_ref().out.len();
    let d = place_in(h, S1, G1, A1, Side::Buy, q("0.001"), IntentClass::Open);
    let out = h.io_ref().out[n..]
        .iter()
        .any(|f| f.1.account == AC1 && f.1.inst == A1);
    (d, out)
}

/// The closing link of g1: a1 sell `qty` (Reduce) with an OnFill reduce-only b1 buy.
fn close_link(hx: &Hx<'_>) -> hft_strategy_sdk::OrderIntent {
    let b = hx.v.bbo(A1);
    let mut it = intent(
        hx,
        A1,
        Side::Sell,
        hft_types::Tif::Ioc,
        IntentClass::Reduce,
        b.bid_px,
        q("0.05"),
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

#[test]
fn md_4_clockbad_halted_block_open_only() {
    let mut h = boot_with(|_| {});
    exact_position(&mut h, S1, 0);
    h.mark_md(B1, "50000");
    set_liq(&mut h, Some("70000"));
    let t0 = h.now();
    // t = 5 s: Timer{ClockBad} → global conditional ClockBad cap, version + 1.
    live_to(&mut h, t0 + 5 * SEC);
    let v0 = h.s.engine().risk.mode.ctl_state_version();
    let m = h.mark();
    let t = h.now();
    h.push(RingId::Hk, &sys_timer(9, t));
    h.run();
    let cb: Vec<_> = cap_notes(&h, m, true)
        .into_iter()
        .filter(|c| c.source == CapSource::CLOCK_BAD)
        .collect();
    assert_eq!(cb.len(), 1, "CapSet{{ClockBad}}");
    assert!(
        cb[0].cflags & hft_event::CapRecord::CONDITIONAL != 0,
        "conditional"
    );
    assert_eq!(h.s.engine().risk.mode.ctl_state_version(), v0 + 1);
    // t = 6 s: an Open → CLOCK, not out.
    live_to(&mut h, t0 + 6 * SEC);
    let (d, out) = open_a1(&mut h);
    assert_eq!((d.reject, out), (RejectBits::CLOCK.0, false), "t = 6 s");
    // t = 7 s: the closing link's first leg (Reduce) goes out; its fill sends the hedge leg
    // (risk-reducing) in the same Input.
    live_to(&mut h, t0 + 7 * SEC);
    let n = h.io_ref().out.len();
    let m = place_with(&mut h, S1, close_link);
    let first = decs_since(&h, m)
        .into_iter()
        .find(|d| d.inst == A1)
        .expect("first leg");
    assert_eq!(first.reject, 0, "{first:?}");
    assert!(h.io_ref().out[n..].iter().any(|f| f.1.inst == A1));
    h.ack(&first);
    let n = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs_since(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("hedge leg");
    assert_eq!((second.side, second.reject), (Side::Buy, 0));
    assert!(second.oflags & OrderFlags::RISK_REDUCING.0 != 0);
    assert!(
        h.io_ref().out[n..].iter().any(|f| f.1.inst == B1),
        "hedge out in the fill's Input"
    );
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    // t ∈ [8 s, 15 s): L3 (liq_dist 0.09) → ForceDeleverage engine orders, perp first, reject 0.
    live_to(&mut h, t0 + 8 * SEC);
    let m = h.mark();
    let n = h.io_ref().out.len();
    set_liq(&mut h, Some("54500"));
    let mut seen = Vec::new();
    while h.now() < t0 + 15 * SEC {
        for d in decs_since(&h, m) {
            if !seen.iter().any(|x: &hft_event::OrderDecision| {
                x.venue == d.venue && x.slot == d.slot && x.r#gen == d.r#gen
            }) {
                seen.push(d);
                if d.reject == 0 && d.action == hft_event::OrderAction::New {
                    h.ack(&d);
                    h.fill(&d, d.qty, d.qty, d.px);
                }
            }
        }
        let t = h.now() + 500 * MS;
        live_to(&mut h, t);
    }
    let fx: Vec<_> = exec_decs_since(&h, m)
        .into_iter()
        .filter(|x| x.action == hft_exec::decision::action::FORCE_STEP)
        .collect();
    assert!(!fx.is_empty(), "ForceStep");
    let eng: Vec<_> = seen.iter().filter(|d| d.account == AC1).collect();
    assert!(!eng.is_empty(), "engine orders");
    assert!(eng.iter().all(|d| d.reject == 0), "{eng:?}");
    assert_eq!(eng[0].inst, B1, "perp leg first: {eng:?}");
    assert!(eng[0].oflags & OrderFlags::REDUCE_ONLY.0 != 0);
    assert!(
        h.io_ref().out[n..].iter().any(|f| f.1.inst == B1),
        "engine order out"
    );
    // t = 15 s: liq_dist 0.40 → back to L0.
    set_liq(&mut h, Some("70000"));
    // MD-4.2 (t = 10 s in the table; any time under ClockBad): Resume{ClockBad} → NotResumable.
    let r = run_cmd(
        &mut h,
        AC1,
        &ControlCmd::Resume {
            scope: scope(StrategyId(0), AccountId(0), GroupId(0)),
            source: CapSource::CLOCK_BAD,
        },
    );
    assert_eq!(
        r,
        (status::INVALID_ARGS, u32::from(detail::NOT_RESUMABLE)),
        "MD-4.2"
    );
    assert!(
        h.s.engine()
            .risk
            .mode
            .caps()
            .iter()
            .any(|c| c.source == CapSource::CLOCK_BAD)
    );
    // t = 20 s: Timer{ClockOk} → CapCleared{ClockBad}; t = 21 s the Open passes.
    live_to(&mut h, t0 + 20 * SEC);
    let m = h.mark();
    let t = h.now();
    h.push(RingId::Hk, &sys_timer(8, t));
    h.run();
    assert!(
        cap_notes(&h, m, false)
            .iter()
            .any(|c| c.source == CapSource::CLOCK_BAD),
        "CapCleared{{ClockBad}}"
    );
    live_to(&mut h, t0 + 21 * SEC);
    let (d, out) = open_a1(&mut h);
    assert_eq!((d.reject, out), (0, true), "t = 21 s: {:?}", caps_of(&h, AC1));
}

/// MD-4.1: three venue rejects of s1's Opens → `Ctx::request_halt(g1)` → `CapSet{Halted}` of
/// (1, a, g1) in the third reject's Input, Live → CloseOnly; Opens CLOSE_ONLY, a Flatten goes
/// out; `Resume{Halted}` → CapCleared, CloseOnly → Live.
#[test]
fn md_4_1_consecutive_rejects_request_halt() {
    let mut h = boot_with(|_| {});
    exact_position(&mut h, S1, 0);
    clear_log();
    // probe-taker's max_consec_fail = 3.
    let fails = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let mut m3 = 0;
    for k in 0..3 {
        let (d, out) = open_a1(&mut h);
        assert_eq!((d.reject, out), (0, true), "Open {k}");
        let f2 = fails.clone();
        set_hook(move |hx, ev, ctx| {
            if hx.s.strategy == S1
                && let EventRef::Order { u, .. } = ev
                && matches!(u.kind, hft_strategy_sdk::OrderUpdateKind::Rejected { .. })
            {
                f2.set(f2.get() + 1);
                if f2.get() == 3 {
                    let _ = ctx.request_halt(G1, 1);
                }
            }
        });
        m3 = h.mark();
        h.reject(&d);
        clear_hook();
    }
    assert_eq!(fails.get(), 3);
    let hc: Vec<_> = cap_notes(&h, m3, true)
        .into_iter()
        .filter(|c| c.source == CapSource::HALTED)
        .collect();
    assert_eq!(hc.len(), 1, "CapSet{{Halted}} in the third reject's Input");
    assert_eq!(
        (hc[0].scope.strategy, hc[0].scope.account, hc[0].scope.group),
        (S1, AC1, G1)
    );
    assert!(is_close_only(&h));
    assert!(
        mode_events(S1)
            .iter()
            .any(|t| t.contains("old: Live") && t.contains("new: CloseOnly"))
    );
    let (d, out) = open_a1(&mut h);
    assert_eq!((d.reject, out), (RejectBits::CLOSE_ONLY.0, false));
    // The strategy's Flatten (a1 sell 0.1) goes out.
    let n = h.io_ref().out.len();
    let d = place_in(&mut h, S1, G1, A1, Side::Sell, q("0.1"), IntentClass::Flatten);
    assert_eq!(d.reject, 0, "{d:?}");
    assert!(
        h.io_ref().out[n..]
            .iter()
            .any(|f| f.1.inst == A1 && f.1.side == Side::Sell)
    );
    clear_log();
    let r = run_cmd(
        &mut h,
        AC1,
        &ControlCmd::Resume {
            scope: scope(S1, AC1, G1),
            source: CapSource::HALTED,
        },
    );
    assert_eq!(r.0, status::APPLIED, "{r:?}");
    assert!(!cap_on(&h, CapSource::HALTED, AC1, Some(G1)));
    assert!(
        mode_events(S1)
            .iter()
            .any(|t| t.contains("old: CloseOnly") && t.contains("new: Live"))
    );
}

// ===================================================================================== B-10

/// An Admin `SmokeOrder` of account 1 on `v` (`inst` buy, `ttl_ms` 60 000), acknowledged.
fn smoke(h: &mut H, v: hft_types::VenueId, inst: hft_types::InstId) -> hft_event::OrderDecision {
    let m = h.mark();
    let r = run_cmd(
        h,
        AC1,
        &ControlCmd::SmokeOrder {
            account: AC1,
            venue: v,
            _pad: 0,
            inst,
            side: Side::Buy,
            _pad2: [0; 3],
            ttl_ms: 60_000,
        },
    );
    assert_eq!(r.0, status::APPLIED, "{r:?}");
    let d = decs_since(h, m)
        .into_iter()
        .find(|d| d.inst == inst && d.action == hft_event::OrderAction::New)
        .expect("smoke order");
    assert!(d.oflags & OrderFlags::SMOKE.0 != 0);
    h.ack(&d);
    d
}

/// `(seq, decision)` of the Cancel decisions of slot `d` since `m`.
fn cancels_of(h: &H, m: usize, d: &hft_event::OrderDecision) -> Vec<(u64, hft_event::OrderDecision)> {
    decisions(&h.since(m))
        .into_iter()
        .filter(|x| {
            x.1.action == hft_event::OrderAction::Cancel && x.1.venue == d.venue && x.1.slot == d.slot
        })
        .collect()
}

/// (v, 1)'s order connections: Down drops both, Up brings back the primary (one Up Input).
fn order_conn(h: &mut H, v: hft_types::VenueId, st: hft_event::ConnStateKind) {
    let t = h.now();
    let ks: &[u8] = if st == hft_event::ConnStateKind::Up {
        &[0]
    } else {
        &[0, 1]
    };
    for &k in ks {
        h.push(
            RingId::Rep(0),
            &conn(v, k, st, hft_event::ConnRole::Order, hft_types::InstId(0), AC1, t),
        );
    }
    h.run();
}

fn slot_of(h: &H, d: &hft_event::OrderDecision) -> Option<hft_oms::OrderState> {
    let r = hft_strategy_sdk::OrderRef::new(d.venue, d.account, d.slot.0, d.r#gen);
    h.s.engine().oms.slot(r).map(|s| s.state)
}

/// Steps (100 ms, market connections alive) until a round ends the Drain (`StepOutcome::Done`)
/// or `until`; true when it ended.
fn wait_drain_done(h: &mut H, until: u64) -> bool {
    use hft_event::{ConnRole, ConnStateKind};
    loop {
        if matches!(h.run(), hft_sequencer::StepOutcome::Done { .. }) {
            return true;
        }
        if h.now() >= until {
            return false;
        }
        h.adv(100 * MS);
        let t = h.now();
        for v in [A, B] {
            h.push(
                RingId::Rep(0),
                &conn(
                    v,
                    0,
                    ConnStateKind::Alive,
                    ConnRole::Md,
                    hft_types::InstId(0),
                    AccountId(0),
                    t,
                ),
            );
        }
    }
}

fn report(h: &mut H, d: &hft_event::OrderDecision, what: hft_event::OrderEventKind, detail: u8) {
    let mut ev = order_event(d, what, h.epoch());
    ev.detail = detail;
    h.push(RingId::Rep(0), &ev);
    h.run();
}

/// B-10 / B-10.1 common run. `drain`: B-10.1 (Drain instead of Kill, plus a hedge IOC in
/// flight).
fn b10(drain: bool) {
    use hft_event::{ConnStateKind as K, OrderEventKind as E};
    use hft_oms::OrderState as S;
    let mut h = boot_with(|_| {});
    let hedge = if drain {
        // A link's second leg in flight (PendingNew, RISK_REDUCING).
        let m = place_with(&mut h, S1, |hx| pair_first(hx, q("0.01")));
        let first = decs_since(&h, m)
            .into_iter()
            .find(|d| d.inst == A1)
            .expect("first");
        h.ack(&first);
        let m = h.mark();
        h.fill(&first, first.qty, first.qty, first.px);
        let s = decs_since(&h, m)
            .into_iter()
            .find(|d| d.inst == B1)
            .expect("second");
        assert!(s.oflags & OrderFlags::RISK_REDUCING.0 != 0);
        Some(s)
    } else {
        None
    };
    let m1 = smoke(&mut h, A, A1);
    let m2 = smoke(&mut h, B, B1);
    assert_eq!(
        (slot_of(&h, &m1), slot_of(&h, &m2)),
        (Some(S::Live), Some(S::Live))
    );
    let t = h.now() + SEC;
    live_to(&mut h, t);
    let t0 = h.now();
    // t = 0: Kill{Account(a), cancel_resting} (B-10.1: Drain{cancel_resting}) → one Cancel each.
    let m = h.mark();
    if drain {
        h.cmd(AccountId(0), &ControlCmd::Drain { cancel_resting: true });
        h.run();
    } else {
        let r = run_cmd(&mut h, AC1, &kill_account(AC1, false, false, true));
        assert_eq!(r.0, status::APPLIED);
    }
    let c1 = cancels_of(&h, m, &m1);
    let c2 = cancels_of(&h, m, &m2);
    assert_eq!((c1.len(), c2.len()), (1, 1), "t = 0 one Cancel per slot");
    if let Some(s) = hedge {
        assert_eq!(
            cancels_of(&h, m, &s).len(),
            1,
            "B-10.1: the hedge IOC is cancelled too"
        );
    }
    // m1: the Cancel is sent; at 0.01 s A's order connections drop → Unknown.
    h.push(
        RingId::Rep(0),
        &sent(&c1[0].1, hft_event::GwOp::Cancel, c1[0].0, h.now()),
    );
    h.run();
    h.at(t0 + 10 * MS);
    order_conn(&mut h, A, K::Down);
    assert_eq!(slot_of(&h, &m1), Some(S::Unknown), "m1 PendingCancel → Unknown");
    // m2: its Cancel is sent and answered CancelReject (not "gone") → Live.
    h.push(
        RingId::Rep(0),
        &sent(&c2[0].1, hft_event::GwOp::Cancel, c2[0].0, h.now()),
    );
    h.adv(MS);
    report(&mut h, &m2, E::CancelReject, 0);
    assert_eq!(slot_of(&h, &m2), Some(S::Live), "m2 Live after CancelReject");
    // t = 3 s: A up → Query in the Up Input; the query finds it Live → Cancel again in that
    // Input; CancelAck → Cancelled.
    live_to(&mut h, t0 + 3 * SEC);
    let m = h.mark();
    let mk3 = m;
    order_conn(&mut h, A, K::Up);
    let q1: Vec<_> = decisions(&h.since(m))
        .into_iter()
        .filter(|x| x.1.action == hft_event::OrderAction::Query && x.1.slot == m1.slot)
        .collect();
    assert_eq!(q1.len(), 1, "Query on Up");
    h.push(
        RingId::Rep(0),
        &sent(&q1[0].1, hft_event::GwOp::Query, q1[0].0, h.now()),
    );
    h.run();
    let m = h.mark();
    report(&mut h, &m1, E::QueryResult, 1);
    // Converged to Live and cancelled again within the same Input.
    assert_eq!(slot_of(&h, &m1), Some(S::PendingCancel));
    let again = cancels_of(&h, m, &m1);
    assert_eq!(again.len(), 1, "Cancel re-issued in the QueryResult Input");
    h.push(
        RingId::Rep(0),
        &sent(&again[0].1, hft_event::GwOp::Cancel, again[0].0, h.now()),
    );
    report(&mut h, &m1, E::CancelAck, 0);
    assert_eq!(slot_of(&h, &m1), Some(S::Cancelled), "m1 Cancelled");
    // The obligation is re-issued account-wide: A's Up at 3 s already re-cancelled m2.
    let r2 = cancels_of(&h, mk3, &m2);
    assert_eq!(r2.len(), 1, "m2 re-cancelled by the 3 s Up");
    h.push(
        RingId::Rep(0),
        &sent(&r2[0].1, hft_event::GwOp::Cancel, r2[0].0, h.now()),
    );
    h.run();
    // t = 5 s / 6 s: B down (PendingCancel → Unknown) / up → Query; the query finds it Live → the
    // Cancel again in that Input; then Cancelled.
    live_to(&mut h, t0 + 5 * SEC);
    order_conn(&mut h, B, K::Down);
    assert_eq!(slot_of(&h, &m2), Some(S::Unknown));
    live_to(&mut h, t0 + 6 * SEC);
    let m = h.mark();
    order_conn(&mut h, B, K::Up);
    let q2: Vec<_> = decisions(&h.since(m))
        .into_iter()
        .filter(|x| x.1.action == hft_event::OrderAction::Query && x.1.slot == m2.slot && x.1.venue == B)
        .collect();
    assert_eq!(q2.len(), 1, "Query on B's Up");
    h.push(
        RingId::Rep(0),
        &sent(&q2[0].1, hft_event::GwOp::Query, q2[0].0, h.now()),
    );
    h.run();
    let m = h.mark();
    report(&mut h, &m2, E::QueryResult, 1);
    let again = cancels_of(&h, m, &m2);
    assert_eq!(
        again.len(),
        1,
        "m2 Cancel re-issued on convergence: {:?}",
        slot_of(&h, &m2)
    );
    h.push(
        RingId::Rep(0),
        &sent(&again[0].1, hft_event::GwOp::Cancel, again[0].0, h.now()),
    );
    report(&mut h, &m2, E::CancelAck, 0);
    assert_eq!(slot_of(&h, &m2), Some(S::Cancelled), "m2 Cancelled");
    if let Some(s) = hedge {
        // The hedge IOC ends (its Cancel acknowledged); every slot is terminal → Drain Done.
        let now = h.now();
        assert!(
            !wait_drain_done(&mut h, now),
            "not before the hedge IOC is terminal"
        );
        let mut ev = order_event(&s, E::CancelAck, h.epoch());
        ev.detail = 0;
        h.push(RingId::Rep(0), &ev);
        let until = h.now() + 5 * SEC;
        assert!(
            wait_drain_done(&mut h, until),
            "B-10.1 Drain ends with Done: {:?}",
            h.s.engine().drain
        );
        assert!(!h.s.engine().drain.timed_out);
        assert!(h.s.engine().exec.drain_ready());
        return;
    }
    // t = 10 s: Resume{Kill}; t = 11 s a new smoke m3 on A; A down at 12 s, up at 13 s: no Cancel
    // for m3, which stays Live until its ttl.
    live_to(&mut h, t0 + 10 * SEC);
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::KILL));
    assert_eq!(r.0, status::APPLIED);
    live_to(&mut h, t0 + 11 * SEC);
    let m3 = smoke(&mut h, A, A1);
    let mk = h.mark();
    live_to(&mut h, t0 + 12 * SEC);
    order_conn(&mut h, A, K::Down);
    live_to(&mut h, t0 + 13 * SEC);
    order_conn(&mut h, A, K::Up);
    assert!(
        cancels_of(&h, mk, &m3).is_empty(),
        "no Cancel for m3 after the Resume"
    );
    assert_eq!(slot_of(&h, &m3), Some(S::Live));
    live_to(&mut h, t0 + 70 * SEC);
    assert_eq!(
        slot_of(&h, &m3),
        Some(S::Live),
        "m3 Live until its ttl (11 s + 60 s)"
    );
    live_to(&mut h, t0 + 72 * SEC);
    assert_eq!(cancels_of(&h, mk, &m3).len(), 1, "m3 cancelled at its ttl");
}

#[test]
fn b_10_cancel_obligation_reissued_on_reconnect() {
    b10(false);
    // B-10.1: Drain{cancel_resting} (open_only = false) with a hedge IOC in flight.
    b10(true);
}

// ===================================================================================== BK-1–BK-3

use hft_exec::decision::{action as xa, autofix, table as xt};

/// BK-1/BK-2 parameters: `max_reduce_notional` 2 000 USDT, `force_slice.max_chunk_notional`
/// 10 000 USDT (min 0, cooldown 200 ms); the rest is B0 / ExecParams default
/// (`broken_auto_fix_ms` 120 000, `fix_deadline_ms` 10 000, `max_nofill` 5, `stuck_after_ms`
/// 2 000, `gated_backoff_ms` 500, `dust_notional` 5 USDT).
fn bk_edit(b: &mut B0) {
    risk_acct(b, AC1, |x| {
        x.max_reduce_notional = n("2000");
        x.max_hedge_notional = n("10000");
        x.max_order_notional = n("10000");
    });
    b.setup_edit.push(|s| {
        s.exec.default.force_slice.max_chunk_notional = n("10000");
        s.exec.default.force_slice.min_chunk_notional = Notional(0);
        s.exec.default.force_slice.cooldown_ms = 200;
    });
}

/// `(tsc, decision)` of the `OrderDecision`s since `m`.
fn decs_t(h: &H, m: usize) -> Vec<(u64, hft_event::OrderDecision)> {
    h.since(m)
        .iter()
        .filter(|r| r.kind == hft_event::EventKind::ORDER_DECISION)
        .filter_map(|r| {
            hft_sequencer::types::read_body::<hft_event::OrderDecision>(&r.body).map(|d| (r.tsc, d))
        })
        .collect()
}

fn autofix_decs(h: &H, m: usize, code: u8) -> Vec<hft_event::ExecDecision> {
    exec_decs_since(h, m)
        .into_iter()
        .filter(|x| x.action == xa::AUTO_FIX && exec_detail(x).0 == xt::AUTO_FIX && exec_detail(x).1 == code)
        .collect()
}

fn autofix_orders(h: &H, m: usize) -> Vec<(u64, hft_event::OrderDecision)> {
    decs_t(h, m)
        .into_iter()
        .filter(|d| d.1.reason == REASON_AUTO_FIX && d.1.action == hft_event::OrderAction::New)
        .collect()
}

fn same(a: &hft_event::OrderDecision, b: &hft_event::OrderDecision) -> bool {
    a.venue == b.venue && a.slot == b.slot && a.r#gen == b.r#gen
}

/// s1's link opened by `f` (first leg `first`, filled at once); each hedge order (inst `hedge`)
/// is answered by `answer` until the link is Broken. Returns (mark before the breaking Input,
/// its TSC).
fn break_by(
    h: &mut H,
    f: fn(&Hx<'_>) -> hft_strategy_sdk::OrderIntent,
    first: hft_types::InstId,
    hedge: hft_types::InstId,
    answer: &mut dyn FnMut(&mut H, &hft_event::OrderDecision, usize),
) -> (usize, u64) {
    let m = place_with(h, S1, f);
    let d = decs_since(h, m)
        .into_iter()
        .find(|d| d.inst == first && d.account == AC1)
        .expect("first leg");
    assert_eq!(d.reject, 0, "{d:?}");
    h.ack(&d);
    let m0 = h.mark();
    h.fill(&d, d.qty, d.qty, d.px);
    let mut done: Vec<hft_event::OrderDecision> = Vec::new();
    for k in 0..12 {
        let open = decs_since(h, m0).into_iter().find(|x| {
            x.inst == hedge
                && x.reject == 0
                && x.action == hft_event::OrderAction::New
                && !done.iter().any(|y| same(x, y))
        });
        if let Some(open) = open {
            done.push(open);
            let m = h.mark();
            answer(h, &open, k);
            if cap_notes(h, m, true)
                .iter()
                .any(|c| c.source == CapSource::BROKEN)
            {
                return (m, h.now());
            }
        }
        let due = retry_timers(h).first().map(|t| t.0).expect("retry timer");
        while h.now() < due {
            let m = h.mark();
            let d = (due - h.now()).min(50 * MS);
            h.adv(d);
            md_alive(h);
            if cap_notes(h, m, true)
                .iter()
                .any(|c| c.source == CapSource::BROKEN)
            {
                return (m, h.now());
            }
        }
    }
    panic!("link not Broken");
}

/// Answers every new AutoFix slice with `answer` while stepping 50 ms to `until` (or until `stop`
/// says so); returns the slices with their decision TSC.
fn drive_autofix(
    h: &mut H,
    m: usize,
    until: u64,
    answer: &mut dyn FnMut(&mut H, &hft_event::OrderDecision),
    stop: &dyn Fn(&H) -> bool,
) -> Vec<(u64, hft_event::OrderDecision)> {
    let mut done: Vec<(u64, hft_event::OrderDecision)> = Vec::new();
    loop {
        for x in autofix_orders(h, m) {
            if !done.iter().any(|y| same(&x.1, &y.1)) {
                done.push(x);
                answer(h, &x.1);
            }
        }
        if stop(h) || h.now() >= until {
            return done;
        }
        h.adv(50 * MS);
        md_alive(h);
        if h.now() / SEC != (h.now() - 50 * MS) / SEC {
            h.mark_md(B1, "50010");
            h.margin_report(AC1, "100000", "0", "100000");
        }
    }
}

fn ack_fill(h: &mut H, d: &hft_event::OrderDecision) {
    h.ack(d);
    h.fill(d, d.qty, d.qty, d.px);
}

fn ceil_notional(d: &hft_event::OrderDecision) -> i64 {
    // qty (1e-8) × px (1e-2) → 1e-10 USDT; ceiling to 1e-6.
    let v = i128::from(d.qty.0) * i128::from(d.px.0);
    ((v + 9_999) / 10_000) as i64
}

fn bk1_world() -> (H, usize, u64) {
    let mut h = boot_with(bk_edit);
    h.mark_md(B1, "50010");
    let t0 = h.now();
    let (m, t_b) = break_by(&mut h, |hx| pair_first(hx, q("0.1")), A1, B1, &mut |h, d, _| {
        h.reject(d)
    });
    assert!(t_b - t0 <= 3_050 * MS, "t_b = {} ms", (t_b - t0) / MS);
    (h, m, t_b)
}

#[test]
fn bk_1_spot_excess_auto_fix_once() {
    let (mut h, mb, t_b) = bk1_world();
    // The breaking Input: OmsHedgeBroken (P0), CapSet{Broken(a, g1)}, CapSet{HedgeFailure(a)},
    // ExecDecision{State → Broken, Broken/LinkBroken, arg RetryExhausted}.
    let hb = h
        .notes_since(mb)
        .into_iter()
        .filter(|n| n.code == hft_oms::notes::code_of(hft_oms::notes::HEDGE_BROKEN))
        .collect::<Vec<_>>();
    assert_eq!(hb.len(), 1);
    assert_eq!(hb[0].level, hft_event::NoteLevel::P0);
    let caps = cap_notes(&h, mb, true);
    assert!(
        caps.iter()
            .any(|c| c.source == CapSource::BROKEN && c.scope.account == AC1 && c.scope.group == G1)
    );
    assert!(
        caps.iter()
            .any(|c| c.source == CapSource::HEDGE_FAILURE && c.scope.account == AC1)
    );
    let st: Vec<_> = exec_decs_since(&h, mb)
        .into_iter()
        .filter(|x| x.action == xa::STATE && x.state_to == 4)
        .collect();
    assert_eq!(st.len(), 1, "State → Broken");
    assert_eq!(
        exec_detail(&st[0]),
        (xt::BROKEN, 1, 1),
        "Broken/LinkBroken, RetryExhausted"
    );
    // Nothing until t_b + 119 999 ms.
    live_to(&mut h, t_b + 119_999 * MS);
    assert!(autofix_decs(&h, mb, autofix::START).is_empty());
    assert!(autofix_orders(&h, mb).is_empty());
    // t_b + 120 000 ms: AutoFix/Start, target 0.1; three a1 sell slices, each ≤ 2 000 USDT at
    // the capped limit bid × (1 − 0.005) = 49 750.00, ≥ 200 ms apart; the last fill's Input
    // closes it: AutoFix/Done, ExecAutoFixDone (P1), CapCleared{Broken} in this order.
    let m = h.mark();
    h.at(t_b + 120_000 * MS);
    md_alive(&mut h);
    let s = autofix_decs(&h, m, autofix::START);
    assert_eq!(s.len(), 1, "AutoFix/Start at t_b + 120 s");
    assert_eq!(s[0].target_qty, q("0.1"));
    let mut last_m = m;
    let slices = drive_autofix(
        &mut h,
        m,
        t_b + 130 * SEC,
        &mut |h, d| {
            h.ack(d);
            last_m = h.mark();
            h.fill(d, d.qty, d.qty, d.px);
        },
        &|h| !cap_on(h, CapSource::BROKEN, AC1, Some(G1)),
    );
    let qs: Vec<Qty> = slices.iter().map(|s| s.1.qty).collect();
    assert_eq!(qs, vec![q("0.0402"), q("0.0402"), q("0.0196")], "{slices:?}");
    for (k, (t, d)) in slices.iter().enumerate() {
        assert_eq!((d.inst, d.side, d.px), (A1, Side::Sell, px("49750")), "slice {k}");
        assert!(is_role(d, LinkRole::HedgeFix));
        assert!(d.oflags & OrderFlags::RISK_REDUCING.0 != 0 && d.oflags & OrderFlags::REDUCE_ONLY.0 == 0);
        assert!(ceil_notional(d) <= 2_000_000_000, "slice {k} notional");
        if k > 0 {
            assert!(t - slices[k - 1].0 >= 200 * MS, "cooldown between slices");
        }
    }
    let recs = h.since(last_m);
    let done_at = recs.iter().position(|r| {
        r.kind == hft_event::EventKind::EXEC_DECISION
            && hft_sequencer::types::read_body::<hft_event::ExecDecision>(&r.body)
                .is_some_and(|x| x.action == xa::AUTO_FIX && exec_detail(&x).1 == autofix::DONE)
    });
    let note_at = note_pos(&recs, |n| {
        n.code == hft_exec::decision::note_code(hft_exec::decision::note::EXEC_AUTO_FIX_DONE)
    });
    let clr_at = note_pos(&recs, |n| is_cap_note(n, false, CapSource::BROKEN));
    assert!(
        done_at.is_some() && note_at.is_some() && clr_at.is_some(),
        "{done_at:?} {note_at:?} {clr_at:?}"
    );
    assert!(done_at < note_at && note_at < clr_at, "order");
    assert_eq!(
        input_of(&recs, done_at.unwrap_or(0)),
        input_of(&recs, clr_at.unwrap_or(0)),
        "same Input"
    );
    let nd = exec_notes(&h, last_m, hft_exec::decision::note::EXEC_AUTO_FIX_DONE);
    assert_eq!(nd[0].level, hft_event::NoteLevel::P1);
    assert_eq!(
        nd[0].v[..6],
        [
            hft_exec::decision::acct_group(AC1.0, G1.0),
            0,
            q("0.1").0 as u64,
            q("0.1").0 as u64,
            0,
            3
        ]
    );
    assert!(
        cap_on(&h, CapSource::HEDGE_FAILURE, AC1, None),
        "HedgeFailure kept"
    );
    assert_eq!(autofix_decs(&h, mb, autofix::START).len(), 1);
    assert_eq!(
        autofix_decs(&h, mb, autofix::DONE).len() + autofix_decs(&h, mb, autofix::FAILED).len(),
        1
    );

    // BK-1.2: a1 quoted 49 000 / 51 000 (bid < mid × 0.99) from t_b + 119 s to t_b + 130 s: no
    // start, no failure note; the Bbo restoring 50 000 / 50 001 starts it in its Input.
    let (mut h, mb, t_b) = bk1_world();
    live_to(&mut h, t_b + 119 * SEC);
    h.quote(A1, "49000", "51000");
    h.run();
    live_to(&mut h, t_b + 130 * SEC);
    assert!(
        autofix_decs(&h, mb, autofix::START).is_empty(),
        "BK-1.2 wide spread"
    );
    assert!(exec_notes(&h, mb, hft_exec::decision::note::EXEC_AUTO_FIX_FAILED).is_empty());
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    assert_eq!(
        autofix_decs(&h, m, autofix::START).len(),
        1,
        "BK-1.2 start on the restoring Bbo"
    );

    // BK-1.3: a1's BBO connection silent until t_b + 130 s (b1's alive): no start; the
    // connection's return starts it in that Input. The silence starts at t_b + 117 s (the table
    // says 119 s): the quote is only unusable once the silence exceeds conn_liveness_ms (2 s),
    // so it must begin ≥ 2 s before the due time for "no start at t_b + 120 s" to hold.
    let (mut h, mb, t_b) = bk1_world();
    live_to(&mut h, t_b + 117 * SEC);
    while h.now() < t_b + 130 * SEC {
        h.adv(250 * MS);
        let t = h.now();
        h.push(
            RingId::Rep(0),
            &conn(
                B,
                0,
                hft_event::ConnStateKind::Alive,
                hft_event::ConnRole::Md,
                hft_types::InstId(0),
                AccountId(0),
                t,
            ),
        );
        h.run();
    }
    assert!(
        autofix_decs(&h, mb, autofix::START).is_empty(),
        "BK-1.3 stale quote"
    );
    assert!(exec_notes(&h, mb, hft_exec::decision::note::EXEC_AUTO_FIX_FAILED).is_empty());
    // The connection returns with a fresh a1 frame: AutoFix/Start in that Bbo's Input.
    let m = h.mark();
    requote(&mut h, A1);
    assert_eq!(
        autofix_decs(&h, m, autofix::START).len(),
        1,
        "BK-1.3 start on the connection's return"
    );

    // BK-1.5: every slice fills nothing → after Start + 10 000 ms the first terminal slice ends
    // it: ExecAutoFixFailed (P0, Deadline) and AutoFix/Failed arg 7; Broken kept; no new slice.
    let (mut h, _mb, t_b) = bk1_world();
    live_to(&mut h, t_b + 120 * SEC - 250 * MS);
    let m = h.mark();
    let t_s = t_b + 120 * SEC;
    let _ = drive_autofix(
        &mut h,
        m,
        t_s + 20 * SEC,
        &mut |h, d| {
            h.ack(d);
            expire_order(h, d);
        },
        &|h| !exec_notes(h, 0, hft_exec::decision::note::EXEC_AUTO_FIX_FAILED).is_empty(),
    );
    let f = exec_notes(&h, m, hft_exec::decision::note::EXEC_AUTO_FIX_FAILED);
    assert_eq!(f.len(), 1, "BK-1.5");
    assert_eq!(f[0].level, hft_event::NoteLevel::P0);
    assert_eq!(
        (f[0].v[2], f[0].v[4], f[0].v[5], f[0].v[6]),
        (7, 0, q("0.1").0 as u64, q("0.1").0 as u64)
    );
    let fd = autofix_decs(&h, m, autofix::FAILED);
    assert_eq!(fd.len(), 1);
    assert_eq!(exec_detail(&fd[0]).2, 7);
    assert!(h.now() >= t_s + 10 * SEC, "not before the executable window");
    assert!(cap_on(&h, CapSource::BROKEN, AC1, Some(G1)));
    let n_orders = autofix_orders(&h, m).len();
    live_to(&mut h, t_s + 40 * SEC);
    assert_eq!(
        autofix_orders(&h, m).len(),
        n_orders,
        "BK-1.5 no new AutoFix order"
    );

    // BK-1.6: the first slice rejected by A → failed (Rejected, 4) in that Input; no new start.
    let (mut h, _mb, t_b) = bk1_world();
    live_to(&mut h, t_b + 120 * SEC - 250 * MS);
    let m = h.mark();
    let mut rej_m = 0;
    let _ = drive_autofix(
        &mut h,
        m,
        t_b + 125 * SEC,
        &mut |h, d| {
            rej_m = h.mark();
            h.reject(d);
        },
        &|h| !exec_notes(h, 0, hft_exec::decision::note::EXEC_AUTO_FIX_FAILED).is_empty(),
    );
    let f = exec_notes(&h, rej_m, hft_exec::decision::note::EXEC_AUTO_FIX_FAILED);
    assert_eq!(f.len(), 1, "BK-1.6 in the reject's Input");
    assert_eq!(f[0].v[2], 4);
    assert_eq!(exec_detail(&autofix_decs(&h, rej_m, autofix::FAILED)[0]).2, 4);
    assert!(cap_on(&h, CapSource::BROKEN, AC1, Some(G1)));
    live_to(&mut h, t_b + 300 * SEC);
    assert_eq!(
        autofix_decs(&h, m, autofix::START).len(),
        1,
        "BK-1.6 no second start"
    );

    // BK-1.7: Trader HedgeFix at t_b + 60 s → CmdHedgeFix and an a1 sell 0.1 IOC (reason 9);
    // no AutoFix/Start afterwards.
    let (mut h, mb, t_b) = bk1_world();
    live_to(&mut h, t_b + 60 * SEC);
    let m = h.mark();
    let v = h.s.engine().risk.mode.ctl_state_version();
    let id = h.cmd_with(
        "cli:trader1",
        TRADER,
        AC1,
        &ControlCmd::HedgeFix {
            scope: scope(S1, AC1, G1),
        },
        |c| {
            c.confirmed = 1;
            c.expect_state_version = v;
        },
    );
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED, "{:?}", h.res(id));
    assert!(
        exec_decs_since(&h, m)
            .iter()
            .any(|x| x.action == xa::CMD_HEDGE_FIX)
    );
    let fix = decs_since(&h, m)
        .into_iter()
        .find(|d| d.reason == REASON_CMD_HEDGE_FIX)
        .expect("HedgeFix order");
    assert_eq!((fix.inst, fix.side, fix.qty), (A1, Side::Sell, q("0.1")));
    h.ack(&fix);
    expire_order(&mut h, &fix);
    live_to(&mut h, t_b + 200 * SEC);
    assert!(
        autofix_decs(&h, mb, autofix::START).is_empty(),
        "BK-1.7 no AutoFix/Start"
    );

    // BK-1.8: HARD Kill at t_b + 60 s, clean rounds every 10 s, Resume at t_b + 90 s: no AutoFix
    // order meanwhile; the full window restarts at the release (start at t_b + 210 s).
    let (mut h, mb, t_b) = bk1_world();
    live_to(&mut h, t_b + 60 * SEC);
    let r = run_cmd(&mut h, AC1, &kill_account(AC1, true, false, false));
    assert_eq!(r.0, status::APPLIED);
    for k in 1..=3 {
        live_to(&mut h, t_b + (60 + 10 * k) * SEC);
        clean_recon(&mut h, AC1);
    }
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::KILL));
    assert_eq!(r.0, status::APPLIED, "{r:?}");
    let t_r = h.now();
    assert!(autofix_orders(&h, mb).is_empty(), "BK-1.8 none under HARD");
    live_to(&mut h, t_r + 119_999 * MS);
    assert!(
        autofix_decs(&h, mb, autofix::START).is_empty(),
        "BK-1.8 before the new window"
    );
    let m = h.mark();
    h.at(t_r + 120 * SEC);
    md_alive(&mut h);
    assert_eq!(
        autofix_decs(&h, m, autofix::START).len(),
        1,
        "BK-1.8 start at release + 120 s"
    );
}

/// The link with b1 (perp) as first leg: b1 sell 0.1 IOC with an OnFill a1 buy.
fn b1_first_link(hx: &Hx<'_>) -> hft_strategy_sdk::OrderIntent {
    let b = hx.v.bbo(B1);
    let mut it = intent(
        hx,
        B1,
        Side::Sell,
        hft_types::Tif::Ioc,
        IntentClass::Open,
        b.bid_px,
        q("0.1"),
    );
    it.group = G1;
    it.link = hft_strategy_sdk::LegLinkOpt::some(hft_strategy_sdk::LegLink {
        group: G1,
        role: LinkRole::First,
        _pad: [0; 3],
        tolerance: r("0.02"),
        hedge: onfill(hx.s.account, A1, Side::Buy),
    });
    it
}

#[test]
fn bk_2_perp_excess_reduce_only_buy() {
    let mut h = boot_with(bk_edit);
    h.mark_md(B1, "50010");
    let (mb, t_b) = break_by(&mut h, b1_first_link, B1, A1, &mut |h, d, _| h.reject(d));
    live_to(&mut h, t_b + 119_999 * MS);
    assert!(autofix_orders(&h, mb).is_empty() && autofix_decs(&h, mb, autofix::START).is_empty());
    let m = h.mark();
    h.at(t_b + 120 * SEC);
    md_alive(&mut h);
    let s = autofix_decs(&h, m, autofix::START);
    assert_eq!(s.len(), 1, "AutoFix/Start at t_b + 120 s");
    assert_eq!(s[0].target_qty, q("0.1"));
    let mut last_m = m;
    let slices = drive_autofix(
        &mut h,
        m,
        t_b + 130 * SEC,
        &mut |h, d| {
            h.ack(d);
            last_m = h.mark();
            h.fill(d, d.qty, d.qty, d.px);
        },
        &|h| !cap_on(h, CapSource::BROKEN, AC1, Some(G1)),
    );
    // Limit = ask × (1 + 0.005) floored to the 0.1 tick = 50 261.0; ⌈qty × px⌉ ≤ 2 000 USDT.
    let qs: Vec<Qty> = slices.iter().map(|s| s.1.qty).collect();
    assert_eq!(qs, vec![q("0.039"), q("0.039"), q("0.022")], "{slices:?}");
    let both = OrderFlags::RISK_REDUCING.0 | OrderFlags::REDUCE_ONLY.0;
    for (k, (_, d)) in slices.iter().enumerate() {
        assert_eq!(
            (d.inst, d.side, d.px),
            (B1, Side::Buy, px("50261.0")),
            "slice {k}"
        );
        assert_eq!(d.oflags & both, both, "slice {k}");
        assert!(is_role(d, LinkRole::HedgeFix));
        assert!(ceil_notional(d) <= 2_000_000_000);
    }
    let nd = exec_notes(&h, last_m, hft_exec::decision::note::EXEC_AUTO_FIX_DONE);
    assert_eq!(nd.len(), 1);
    assert_eq!(
        nd[0].v[..6],
        [
            hft_exec::decision::acct_group(AC1.0, G1.0),
            1,
            q("0.1").0 as u64,
            q("0.1").0 as u64,
            0,
            3
        ]
    );
    assert!(
        cap_notes(&h, last_m, false)
            .iter()
            .any(|c| c.source == CapSource::BROKEN)
    );
    assert!(cap_on(&h, CapSource::HEDGE_FAILURE, AC1, None));
    assert!(
        autofix_orders(&h, mb).iter().all(|d| d.1.inst == B1),
        "no a1 AutoFix order"
    );
}

/// BK-3 parameters: L3 thresholds (B0 defaults 0.25 / 0.15 / 0.10, hysteresis 0.02, step 0.20
/// every 5 s), `max_reduce_notional` and `force_slice.max_chunk_notional` 30 000 USDT, orders
/// up to 30 000 USDT.
fn bk3_edit(b: &mut B0) {
    // `force_slice.max_chunk_notional ≤ max_order_notional` holds for every account.
    for a in 1..=9u8 {
        risk_acct(b, AccountId(a), |x| {
            x.max_reduce_notional = n("30000");
            x.max_hedge_notional = n("30000");
            x.max_order_notional = n("30000");
        });
    }
    b.setup_edit.push(|s| {
        s.exec.default.force_slice.max_chunk_notional = n("30000");
        s.exec.default.force_slice.min_chunk_notional = Notional(0);
    });
}

/// b1 −0.4 first; the a1 hedge fills 0.3, the rest is rejected until Broken.
fn bk3_world() -> (H, u64) {
    fn first(hx: &Hx<'_>) -> hft_strategy_sdk::OrderIntent {
        let mut it = b1_first_link(hx);
        it.qty = q("0.4");
        it
    }
    let mut h = boot_with(bk3_edit);
    h.mark_md(B1, "50000");
    set_liq(&mut h, Some("70000"));
    let (_, t_b) = break_by(&mut h, first, B1, A1, &mut |h, d, k| {
        if k == 0 {
            h.ack(d);
            h.fill(d, q("0.3"), q("0.3"), d.px);
            expire_order(h, d);
        } else {
            h.reject(d);
        }
    });
    let lv = h.s.engine().oms.legs(AC1, G1).view;
    assert_eq!((lv.leg[0].net, lv.leg[1].net), (q("0.3"), q("-0.4")));
    (h, t_b)
}

/// Fills every engine order as it appears while stepping to `until`; returns them in order.
fn drive_engine(h: &mut H, m: usize, until: u64) -> Vec<(u64, hft_event::OrderDecision)> {
    let mut done: Vec<(u64, hft_event::OrderDecision)> = Vec::new();
    while h.now() < until {
        for x in decs_t(h, m) {
            if x.1.account == AC1
                && x.1.action == hft_event::OrderAction::New
                && x.1.reject == 0
                && !done.iter().any(|y| same(&x.1, &y.1))
            {
                done.push(x);
                ack_fill(h, &x.1);
            }
        }
        let t = h.now() + 100 * MS;
        live_to(h, t);
    }
    done
}

#[test]
fn bk_3_l3_and_flatten_cover_broken() {
    let (mut h, t_b) = bk3_world();
    live_to(&mut h, t_b + 10 * SEC);
    let m = h.mark();
    set_liq(&mut h, Some("54500"));
    let notes = h.notes_since(m);
    assert!(
        notes.iter().any(|n| n.source == hft_event::NoteSource::Risk
            && n.code & 0xFFFF == 4
            && n.level == hft_event::NoteLevel::P0),
        "MarginLevel L3"
    );
    let fs: Vec<_> = exec_decs_since(&h, m)
        .into_iter()
        .filter(|x| x.action == xa::FORCE_STEP)
        .collect();
    assert!(
        fs.iter()
            .any(|x| exec_detail(x) == (xt::ENGINE, hft_exec::decision::engine::CANCEL_RESTING, 3)),
        "{fs:?}"
    );
    assert!(
        fs.iter()
            .any(|x| exec_detail(x).0 == xt::ENGINE && exec_detail(x).1 == hft_exec::decision::engine::STEP)
    );
    assert!(cap_on(&h, CapSource::BROKEN, AC1, Some(G1)), "still Broken");
    // Perp first: b1 buy 0.08 reduce-only (Engine), then — after b1 reached −0.32 — a1 sell 0.06.
    let orders = drive_engine(&mut h, m, t_b + 14 * SEC);
    let o: Vec<_> = orders.iter().map(|x| (x.1.inst, x.1.side, x.1.qty)).collect();
    assert!(o.len() >= 2, "{o:?}");
    assert_eq!(o[0], (B1, Side::Buy, q("0.08")), "{o:?}");
    assert!(orders[0].1.oflags & OrderFlags::REDUCE_ONLY.0 != 0);
    assert_eq!(o[1], (A1, Side::Sell, q("0.06")), "{o:?}");
    let lv = h.s.engine().oms.legs(AC1, G1).view;
    assert!(lv.leg[0].net.0 >= 0 && lv.leg[1].net.0 <= 0, "never crosses 0");
    // t_b + 120 s: the engine target holds, no AutoFix/Start.
    set_liq(&mut h, Some("54500"));
    let _ = drive_engine(&mut h, m, t_b + 121 * SEC);
    assert!(
        autofix_decs(&h, m, autofix::START).is_empty(),
        "no AutoFix/Start under an engine target"
    );

    // BK-3.1: no L3; Kill{Account(a), flatten} at t_b + 10 s → b1 buy 0.4 reduce-only, then a1
    // sell 0.3; both at 0 → CapCleared{Broken} in that Input; the flatten lock stays.
    let (mut h, t_b) = bk3_world();
    live_to(&mut h, t_b + 10 * SEC);
    let m = h.mark();
    let r = run_cmd(&mut h, AC1, &kill_account(AC1, false, true, true));
    assert_eq!(r.0, status::APPLIED, "{r:?}");
    let mut seen: Vec<hft_event::OrderDecision> = Vec::new();
    let mut cleared_in = None;
    while h.now() < t_b + 20 * SEC && cleared_in.is_none() {
        for x in decs_since(&h, m) {
            if x.account == AC1
                && x.action == hft_event::OrderAction::New
                && x.reject == 0
                && !seen.iter().any(|y| same(&x, y))
            {
                seen.push(x);
                h.ack(&x);
                let mf = h.mark();
                h.fill(&x, x.qty, x.qty, x.px);
                if cap_notes(&h, mf, false)
                    .iter()
                    .any(|c| c.source == CapSource::BROKEN)
                {
                    cleared_in = Some(mf);
                }
            }
        }
        let t = h.now() + 100 * MS;
        live_to(&mut h, t);
    }
    // b1 is bought back first (reduce-only; B0's top is 1 BTC, so force_slice.depth_frac 0.3
    // caps a slice at 0.3), a1 only after b1 reached 0.
    let o: Vec<_> = seen.iter().map(|x| (x.inst, x.side, x.qty)).collect();
    let k = seen.iter().position(|x| x.inst == A1).expect("a1 leg");
    assert!(
        seen[..k]
            .iter()
            .all(|x| x.inst == B1 && x.side == Side::Buy && x.oflags & OrderFlags::REDUCE_ONLY.0 != 0),
        "BK-3.1 {o:?}"
    );
    assert_eq!(
        seen[..k].iter().map(|x| x.qty.0).sum::<i64>(),
        q("0.4").0,
        "BK-3.1 {o:?}"
    );
    assert_eq!(&o[k..], &[(A1, Side::Sell, q("0.3"))], "BK-3.1 {o:?}");
    assert!(cleared_in.is_some(), "CapCleared{{Broken}} with the last fill");
    let lv = h.s.engine().oms.legs(AC1, G1).view;
    assert_eq!((lv.leg[0].net, lv.leg[1].net), (Qty(0), Qty(0)));
    assert!(h.s.engine().risk.mode.flatten_lock(AC1), "flatten lock kept");
}

// ===================================================================================== FL-1

thread_local! {
    static TGT: std::cell::Cell<Option<hft_strategy_sdk::TargetIntent>> = const { std::cell::Cell::new(None) };
}

/// Instance `s` sets target `t` on the next a1 Bbo; returns the mark before it.
fn set_target(h: &mut H, s: StrategyId, t: hft_strategy_sdk::TargetIntent) -> usize {
    TGT.with(|c| c.set(Some(t)));
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == s
            && let Some(t) = TGT.with(std::cell::Cell::take)
        {
            let _ = ctx.target(t);
        }
    });
    let m = h.mark();
    requote(h, A1);
    clear_hook();
    m
}

fn tgt(
    a: AccountId,
    g: GroupId,
    x: &str,
    reason: hft_strategy_sdk::TargetReason,
) -> hft_strategy_sdk::TargetIntent {
    let mut t = target(
        a,
        g,
        &[(0, q(x)), (1, Qty(-q(x).0))],
        hft_strategy_sdk::ExecSpec::PairOnFill(pair0()),
    );
    t.reason = reason;
    t
}

/// The executor's current target of (a, g): `(legs, origin)`.
fn tview(
    h: &H,
    a: AccountId,
    g: GroupId,
) -> Option<([hft_strategy_sdk::LegTarget; 4], hft_strategy_sdk::TargetOrigin)> {
    let e = h.s.engine();
    if !e.exec.cell(a, g).is_some_and(|c| c.has_target) {
        return None;
    }
    e.exec.view(a, g).map(|v| (v.intent.legs, v.origin))
}

fn target_decs(h: &H, m: usize, a: AccountId) -> Vec<hft_event::ExecDecision> {
    exec_decs_since(h, m)
        .into_iter()
        .filter(|x| {
            x.account == a
                && matches!(
                    x.action,
                    xa::TARGET_SET | xa::TARGET_REPLACED | xa::TARGET_REJECTED
                )
        })
        .collect()
}

#[test]
fn fl_1_flatten_lock_rejects_away_from_zero() {
    use hft_strategy_sdk::TargetReason as R;
    // Engine slices up to 10 000 USDT (B0's ExecParams default is 1 000): one flatten IOC.
    let mut h = boot_with(|b| {
        b.setup_edit
            .push(|s| s.exec.default.force_slice.max_chunk_notional = n("10000"))
    });
    exact_position(&mut h, S1, 0);
    exact_position(&mut h, S2, 0);
    let t0 = h.now();
    // t = 1.000 s: Kill{Account(a), flatten} (Trader, confirmed) → b1 buy 0.1 IOC out in the
    // same Input (perp first); it fills at 1.050 s.
    live_to(&mut h, t0 + SEC);
    let n = h.io_ref().out.len();
    let m = h.mark();
    let v = h.s.engine().risk.mode.ctl_state_version();
    let id = h.cmd_with(
        "cli:trader1",
        TRADER,
        AC1,
        &kill_account(AC1, false, true, true),
        |c| {
            c.confirmed = 1;
            c.expect_state_version = v;
        },
    );
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED, "{:?}", h.res(id));
    let fl = decs_since(&h, m)
        .into_iter()
        .find(|d| d.account == AC1 && d.inst == B1 && d.action == hft_event::OrderAction::New)
        .expect("flatten perp leg");
    assert_eq!((fl.side, fl.qty, fl.reject), (Side::Buy, q("0.1"), 0));
    assert!(
        h.io_ref().out[n..]
            .iter()
            .any(|f| f.1.inst == B1 && f.1.account == AC1)
    );
    assert!(h.s.engine().risk.mode.flatten_lock(AC1));
    // t = 1.010 s: the Open is KILLED, the Reduce goes out.
    h.at(t0 + 1_010 * MS);
    let (d, out) = open_a1(&mut h);
    assert_eq!((d.reject, out), (RejectBits::KILLED.0, false), "t = 1.010 s Open");
    let n = h.io_ref().out.len();
    let red = place_in(&mut h, S1, G1, A1, Side::Sell, q("0.01"), IntentClass::Reduce);
    assert_eq!(red.reject, 0, "t = 1.010 s Reduce");
    assert!(
        h.io_ref().out[n..]
            .iter()
            .any(|f| f.1.inst == A1 && f.1.side == Side::Sell)
    );
    h.ack(&red);
    expire_order(&mut h, &red);
    // t = 1.020 s: a target away from 0 → TargetRejected{FlattenLocked}; the engine target stays.
    h.at(t0 + 1_020 * MS);
    let before = tview(&h, AC1, G1);
    assert!(before.is_some(), "engine target (0, 0) in place");
    let m = set_target(&mut h, S1, tgt(AC1, G1, "0.2", R::Rebalance));
    let td = target_decs(&h, m, AC1);
    assert_eq!(td.len(), 1, "{td:?}");
    assert_eq!(td[0].action, xa::TARGET_REJECTED);
    assert_eq!(exec_detail(&td[0]), (xt::TARGET_REJECT, 9, 0), "FlattenLocked");
    assert_eq!(tview(&h, AC1, G1), before, "engine target unchanged");
    // t = 1.030 s: target (0, 0) Exit → accepted and replaces the engine target: perp leg first,
    // Urgent. It is a strategy target (hft-exec "目标的接受与替换": an accepted strategy target is
    // not an engine target; the table's "origin = Engine" is read as "executor orders", see the
    // owner note), and the lock keeps it from being cleared.
    h.at(t0 + 1_030 * MS);
    let m = set_target(&mut h, S1, tgt(AC1, G1, "0", R::Exit));
    let td = target_decs(&h, m, AC1);
    assert_eq!(td.len(), 1, "{td:?}");
    assert!(
        matches!(td[0].action, xa::TARGET_SET | xa::TARGET_REPLACED),
        "{td:?}"
    );
    assert_eq!(td[0].urgency, 3, "Urgent");
    let tv = tview(&h, AC1, G1).expect("target");
    assert!(tv.0.iter().all(|l| l.qty.0 == 0), "(0, 0)");
    let m_perp = m;
    // t = 1.040 s: account b is not locked: its target and Open are accepted.
    h.at(t0 + 1_040 * MS);
    let m = set_target(&mut h, S2, tgt(AC2, G2, "0.2", R::Rebalance));
    let td = target_decs(&h, m, AC2);
    assert!(
        td.iter()
            .any(|x| matches!(x.action, xa::TARGET_SET | xa::TARGET_REPLACED)),
        "{td:?}"
    );
    let o2 = place_in(&mut h, S2, G2, A1, Side::Buy, q("0.01"), IntentClass::Open);
    assert_eq!(o2.reject, 0, "account b Open");
    // t = 1.050 s: the flatten leg fills; only then does the spot leg follow (perp first).
    assert!(
        decs_since(&h, m_perp).iter().all(|d| !(d.account == AC1
            && d.inst == A1
            && d.action == hft_event::OrderAction::New
            && d.reject == 0)),
        "no a1 engine order before b1 is flat"
    );
    h.at(t0 + 1_050 * MS);
    h.ack(&fl);
    let mf = h.mark();
    h.fill(&fl, fl.qty, fl.qty, fl.px);
    let t = h.now() + SEC;
    live_to(&mut h, t);
    let spot = decs_since(&h, mf)
        .into_iter()
        .find(|d| d.account == AC1 && d.inst == A1 && d.side == Side::Sell && d.reject == 0)
        .expect("spot leg after the perp leg");
    ack_fill(&mut h, &spot);
    // t = 10 s: Resume{Kill} → CapCleared{Kill}, lock released; t = 10.5 s the target is
    // accepted without FlattenLocked.
    live_to(&mut h, t0 + 10 * SEC);
    let m = h.mark();
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::KILL));
    assert_eq!(r.0, status::APPLIED, "{r:?}");
    assert!(
        cap_notes(&h, m, false)
            .iter()
            .any(|c| c.source == CapSource::KILL)
    );
    assert!(!h.s.engine().risk.mode.flatten_lock(AC1), "lock released");
    h.at(t0 + 10_500 * MS);
    let m = set_target(&mut h, S1, tgt(AC1, G1, "0.2", R::Rebalance));
    let td = target_decs(&h, m, AC1);
    assert!(
        td.iter()
            .any(|x| matches!(x.action, xa::TARGET_SET | xa::TARGET_REPLACED)),
        "t = 10.5 s accepted: {td:?}"
    );
    assert!(td.iter().all(|x| x.action != xa::TARGET_REJECTED));
}

#[test]
fn fl_1_kill_flattens_with_full_links_and_strategy_timer_quota() {
    let mut h = boot_with(|b| {
        b.caps.max_timers = 1024;
        b.setup_edit.push(|s| {
            s.oms.links_per_account = 16;
            s.oms.slots = hft_oms::PerTable::new(64);
            s.exec.default.force_slice.max_chunk_notional = n("10000");
        });
    });
    exact_position(&mut h, S1, 0);
    let mut blockers = Vec::new();
    for _ in 0..16 {
        let m = crate::scenarios::place_on_a1(&mut h, &[S1], |hx| pair_first(hx, q("0.001")));
        let d = decs_since(&h, m).into_iter().find(|d| d.link_role == 1).unwrap();
        assert_eq!(d.reject, 0, "{d:?}");
        h.ack(&d);
        blockers.push(d);
    }
    assert_eq!(h.s.engine().oms.legs(AC1, G1).view.open_links, 16);
    let quota = h.s.engine().caps.max_timers - h.s.engine().engine_timer_demand;
    set_hook(|hx, ev, ctx| {
        if hx.s.strategy == S1 && matches!(ev, EventRef::Bbo { inst, .. } if *inst == A1) {
            for _ in 0..8 {
                let _ = ctx.timer_at(hft_types::Tsc(hx.v.now().tsc.0 + 120 * SEC), 901, G1);
            }
        }
    });
    for _ in 0..quota.div_ceil(8) + 1 {
        h.quote(A1, "50000", "50003");
        h.run();
    }
    clear_hook();
    assert_eq!(h.s.engine().strat_timers, quota);
    let start = h.keyframe().seq;
    let m = h.mark();
    let id = h.cmd_confirmed(AC1, &kill_account(AC1, false, true, true));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    let perp = decs_since(&h, m)
        .into_iter()
        .find(|d| d.action == hft_event::OrderAction::New && d.inst == B1)
        .expect("Kill perp order");
    assert_eq!((perp.reject, perp.link_role, perp.qty), (0, 0, q("0.1")));
    assert_ne!(perp.oflags & OrderFlags::REDUCE_ONLY.0, 0);
    assert_eq!(h.s.engine().oms.legs(AC1, G1).view.open_links, 16);
    for d in blockers {
        let ev = order_event(&d, hft_event::OrderEventKind::CancelAck, h.epoch());
        h.push(RingId::Rep(0), &ev);
        h.run();
    }
    h.ack(&perp);
    h.fill(&perp, perp.qty, perp.qty, perp.px);
    for _ in 0..20 {
        let t = h.now() + 100 * MS;
        live_to(&mut h, t);
        let orders: Vec<_> = decs_since(&h, m)
            .into_iter()
            .filter(|d| {
                d.action == hft_event::OrderAction::New
                    && d.reject == 0
                    && d.inst == A1
                    && h.s
                        .engine()
                        .oms
                        .slot(hft_strategy_sdk::OrderRef::new(
                            d.venue, d.account, d.slot.0, d.r#gen,
                        ))
                        .is_some_and(|s| s.state.open())
            })
            .collect();
        for d in orders {
            h.ack(&d);
            h.fill(&d, d.qty, d.qty, d.px);
        }
    }
    let lv = &h.s.engine().oms.legs(AC1, G1).view;
    assert_eq!((lv.leg[0].net, lv.leg[1].net), (Qty(0), Qty(0)));
    assert!(h.verify_from(Some(start)).is_empty());
}

// ===================================================================================== BK-7

/// After a restart: venue `v`'s market and order connections, account configuration, balances
/// and a clean reconciliation round for accounts 1 and 2 (the per-venue half of `bring_up`);
/// `first` also journals ClockOk and the kill self-tests.
fn bring_up_venue(h: &mut H, v: hft_types::VenueId, first: bool) {
    use hft_event::{ConnRole, ConnStateKind};
    use zerocopy::IntoBytes;
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &conn(
            v,
            0,
            ConnStateKind::Up,
            ConnRole::Md,
            hft_types::InstId(0),
            AccountId(0),
            t,
        ),
    );
    for i in h.steady_insts() {
        if h.rd.inst(i).venue == v {
            h.push(
                RingId::Rep(0),
                &conn(v, 0, ConnStateKind::Up, ConnRole::Md, i, AccountId(0), t),
            );
        }
    }
    for a in [AC1, AC2, AC9] {
        let ring = if a == AC9 { RingId::RepSim } else { RingId::Rep(0) };
        for (k, role) in [
            (0u8, ConnRole::Order),
            (1, ConnRole::Order),
            (0, ConnRole::UserStream),
        ] {
            h.push(
                ring,
                &conn(v, k, ConnStateKind::Up, role, hft_types::InstId(0), a, t),
            );
        }
    }
    h.run();
    if first {
        h.push(RingId::Hk, &sys_timer(8, t));
        for a in [AC1, AC2] {
            h.push(RingId::Hk, &sys_timer(16 | (u32::from(a.0) << 16), t));
        }
        h.run();
    }
    for i in h.steady_insts() {
        if h.rd.inst(i).venue == v {
            let (b, s) = match i {
                A1 => ("50000", "50001"),
                B1 => ("50010", "50011"),
                A2 => ("3000", "3000.5"),
                B2 => ("3000.1", "3000.6"),
                _ => ("10", "10.01"),
            };
            h.quote(i, b, s);
        }
    }
    h.run();
    for a in [AC1, AC2] {
        let levs: Vec<hft_event::LeverageItem> = if v == B {
            h.steady_insts()
                .into_iter()
                .filter(|i| h.rd.inst(*i).venue == B)
                .map(|inst| hft_event::LeverageItem {
                    inst,
                    _pad: 0,
                    leverage: r("3.00"),
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut body = cfg_hdr(a, v, levs.len() as u16).as_bytes().to_vec();
        for l in &levs {
            body.extend_from_slice(l.as_bytes());
        }
        h.push_bytes(RingId::Rep(0), hft_event::EventKind::ACCOUNT_CONFIG_REPORT, body);
        let br = hft_event::BalanceReport {
            venue: v,
            account: a,
            asset: USDT,
            _pad: 0,
            free: q("100000"),
            locked: Qty(0),
            ex_ts_ns: 0,
            rx_tsc: hft_types::Tsc(h.now()),
        };
        h.push(RingId::Rep(0), &br);
        if v == B {
            h.margin_report(a, "100000", "0", "100000");
        }
    }
    h.run();
    for a in [AC1, AC2] {
        let view = h.s.engine().cur_seq;
        let id = 90_000 + u64::from(a.0) * 10 + u64::from(v.0);
        h.push(RingId::Rep(0), &recon_summary(a, v, id, view));
    }
    h.run();
}

/// Keeps the venues already brought up alive (and marks / margin on B once it is up) up to `t`.
fn alive_venues_to(h: &mut H, t: u64, b_up: bool) {
    use hft_event::{ConnRole, ConnStateKind};
    while h.now() < t {
        let d = (t - h.now()).min(250 * MS);
        h.adv(d);
        let now = h.now();
        h.push(
            RingId::Rep(0),
            &conn(
                A,
                0,
                ConnStateKind::Alive,
                ConnRole::Md,
                hft_types::InstId(0),
                AccountId(0),
                now,
            ),
        );
        if b_up {
            h.push(
                RingId::Rep(0),
                &conn(
                    B,
                    0,
                    ConnStateKind::Alive,
                    ConnRole::Md,
                    hft_types::InstId(0),
                    AccountId(0),
                    now,
                ),
            );
            if now / SEC != (now - d) / SEC {
                h.mark_md(B1, "50010");
                for a in [AC1, AC2] {
                    h.margin_report(a, "100000", "0", "100000");
                }
            }
        }
        h.run();
    }
}

/// The link of s1 with tolerance 0.02 and `on_fail = Hold` (a1 buy 0.1 / b1 sell).
fn hold_link(hx: &Hx<'_>) -> hft_strategy_sdk::OrderIntent {
    let mut it = pair_first(hx, q("0.1"));
    if let Some(l) = it.link.get().copied() {
        let mut l = l;
        l.hedge.on_fail = hft_strategy_sdk::OnFail::Hold;
        it.link = hft_strategy_sdk::LegLinkOpt::some(l);
    }
    it
}

/// BK-7's world up to t = 2 s: g1 a1 +0.1 / b1 −0.099 (within tolerance), g2 a1 +0.1 / b1 −0.1
/// with an Unknown external a1 sell 0.05 at t = 1 s.
fn bk7_world() -> (H, u64) {
    // Engine slices up to 10 000 USDT (B0's ExecParams default is 1 000): one default IOC.
    let mut h = boot_with(|b| {
        b.setup_edit
            .push(|s| s.exec.default.force_slice.max_chunk_notional = n("10000"))
    });
    h.mark_md(B1, "50010");
    // g2 hedged first.
    let a = place_in(&mut h, S2, G2, A1, Side::Buy, q("0.1"), IntentClass::Open);
    h.ack(&a);
    let b = place_in(&mut h, S2, G2, B1, Side::Sell, q("0.1"), IntentClass::Open);
    h.ack(&b);
    h.fill(&a, a.qty, a.qty, a.px);
    h.fill(&b, b.qty, b.qty, b.px);
    let t0 = h.now();
    // t = 0: a's link; the second leg IOC fills 0.099 of 0.100 and expires (Hold).
    let m = place_with(&mut h, S1, hold_link);
    let first = decs_since(&h, m)
        .into_iter()
        .find(|d| d.inst == A1)
        .expect("first leg");
    h.ack(&first);
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs_since(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    h.ack(&second);
    h.fill(&second, q("0.099"), q("0.099"), second.px);
    expire_order(&mut h, &second);
    let lv = h.s.engine().oms.legs(AC1, G1).view;
    assert_eq!(
        (lv.leg[0].net, lv.leg[1].net, lv.open_links),
        (q("0.1"), q("-0.099"), 0)
    );
    // t = 1 s: account b's external a1 sell 0.05 (cause Unknown).
    live_to(&mut h, t0 + SEC);
    let x = external_fill(&h, AC2, A1, Side::Sell, "0.05", hft_event::ExternalCause::Unknown);
    h.push(RingId::Rep(0), &x);
    h.run();
    live_to(&mut h, t0 + 2 * SEC);
    (h, t0)
}

#[test]
fn bk_7_resolve_broken_only_on_hard_release() {
    let (mut h, t0) = bk7_world();
    // t = 2 s: Drain. Its completion waits for g2's external-fill default (hft-sequencer "Drain"
    // step 3 / hft-exec `drain_ready_acct`: no pending default handling), so g2 is handled before
    // the restart: ExternalDefault at 1 s + external_deadline_ms (6 s), a reduce-only b1 buy 0.05;
    // then Drain ends with Done (not DrainTimeout). (The table's "new window after the restart"
    // for g2 cannot coexist with a Done at 2 s; see the owner note.)
    h.cmd(AccountId(0), &ControlCmd::Drain { cancel_resting: true });
    let m = h.mark();
    assert!(
        !wait_drain_done(&mut h, t0 + 5_900 * MS),
        "Drain waits for the default handling"
    );
    let mut done = false;
    let mut handled = false;
    while h.now() < t0 + 10 * SEC && !done {
        if !handled
            && let Some(d) = decs_since(&h, m)
                .into_iter()
                .find(|d| d.account == AC2 && d.inst == B1 && d.action == hft_event::OrderAction::New)
        {
            handled = true;
            assert_eq!((d.side, d.qty), (Side::Buy, q("0.05")));
            assert!(d.oflags & OrderFlags::REDUCE_ONLY.0 != 0);
            assert!(h.now() >= t0 + 6 * SEC, "ExternalDefault at 1 s + 5 s");
            let r = h.rep_ring(d.account);
            h.push(r, &order_event(&d, hft_event::OrderEventKind::Ack, h.epoch()));
            let mut ev = order_event(&d, hft_event::OrderEventKind::Fill, h.epoch());
            ev.qty = d.qty;
            ev.cum_filled = d.qty;
            h.push(r, &ev);
        }
        let until = h.now() + 100 * MS;
        done = wait_drain_done(&mut h, until);
    }
    assert!(done, "Drain Done: {:?}", h.s.engine().drain);
    assert!(
        exec_decs_since(&h, m)
            .iter()
            .any(|x| x.action == xa::EXTERNAL_DEFAULT && x.account == AC2)
    );
    // Same-version restart over an empty tail; A released at t_r + 15 s, B (and the account gate)
    // at t_r + 18 s.
    let mut h = H::crash_restart(
        h,
        plan(hft_sequencer::SnapshotSel::Latest, true, 2, 0),
        Rerun::default(),
    )
    .expect("restart");
    assert!(cap_on(&h, CapSource::RECOVERY, AC1, None));
    h.trace_on();
    let m0 = h.mark();
    let t_r = h.now();
    alive_venues_to(&mut h, t_r + 15 * SEC, false);
    bring_up_venue(&mut h, A, true);
    alive_venues_to(&mut h, t_r + 18 * SEC, false);
    let mg = h.mark();
    bring_up_venue(&mut h, B, false);
    let cleared: Vec<_> = cap_notes(&h, mg, false)
        .into_iter()
        .filter(|c| c.source == CapSource::RECOVERY && c.scope.account == AC1)
        .collect();
    assert_eq!(
        cleared.len(),
        1,
        "CapCleared{{Recovery}} (auto) at the account gate release"
    );
    assert!(!cap_on(&h, CapSource::RECOVERY, AC1, None));
    // To the end: no resolve_broken, no Broken / HedgeFailure, no repair of g1 (within
    // tolerance), nothing more for g2.
    let t_g = h.now();
    alive_venues_to(&mut h, t_g + 30 * SEC, true);
    assert_eq!(h.s.engine().trace.count("exec.resolve_broken"), 0);
    assert!(
        cap_notes(&h, m0, true)
            .iter()
            .all(|c| c.source != CapSource::BROKEN && c.source != CapSource::HEDGE_FAILURE)
    );
    assert!(
        decs_since(&h, m0)
            .iter()
            .all(|d| d.account != AC1 && d.account != AC2),
        "no order: {:?}",
        decs_since(&h, m0)
    );

    // BK-7.1: no Drain; non-HARD Kill{Account(a)} at 5 s, Resume at 10 s → only CapCleared{Kill}.
    let (mut h, t0) = bk7_world();
    live_to(&mut h, t0 + 5 * SEC);
    let r = run_cmd(&mut h, AC1, &kill_account(AC1, false, false, true));
    assert_eq!(r.0, status::APPLIED);
    live_to(&mut h, t0 + 10 * SEC);
    let m = h.mark();
    let r = run_cmd(&mut h, AC1, &resume_account(AC1, CapSource::KILL));
    assert_eq!(r.0, status::APPLIED);
    let cl = cap_notes(&h, m, false);
    assert_eq!(
        cl.iter().map(|c| c.source).collect::<Vec<_>>(),
        vec![CapSource::KILL],
        "BK-7.1"
    );
    assert!(
        cap_notes(&h, m, true)
            .iter()
            .all(|c| c.source != CapSource::BROKEN)
    );
    assert!(decs_since(&h, m).is_empty(), "BK-7.1 no order");
}
