//! LED-170–LED-177: AdoptExchange position/balance entries and residual write-offs.

use crate::common;

use common::*;
use hft_event::NoteLevel;
use hft_ledger::notes::code;
use hft_ledger::views::GroupPosView;
use hft_ledger::{AdjustDelta, AdjustKind, Ledger, LegAgg, PnlBreakdown, PxSrc};
use hft_types::{ASSET_NONE, AccountId, AssetId, GROUP_NONE, GroupId, InstId, Notional, Px, Qty, VenueId};

#[allow(clippy::too_many_arguments)]
fn adj(
    v: VenueId,
    kind: AdjustKind,
    px_src: u8,
    inst: InstId,
    asset: AssetId,
    group: GroupId,
    qty: Qty,
    p: Px,
    report_id: u64,
    cmd_seq: u64,
) -> AdjustDelta {
    AdjustDelta {
        account: A1,
        venue: v,
        kind: kind as u8,
        px_src,
        inst,
        asset,
        _pad: 0,
        group,
        qty,
        px: p,
        report_id,
        cmd_seq,
        settle_ts_ns: 0,
    }
}

/// LED-050's pre-state: `G1` perp leg −1.0 at 60 000.
fn short_1() -> Ledger {
    let mut l = booted();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l.clear_notes();
    l
}

fn av(a: AccountId, v: VenueId) -> u64 {
    u64::from(a.0) | u64::from(v.0) << 8
}

/// Base-asset invariant `total = baseline + unassigned + Σ inv`.
fn invariant(l: &Ledger, v: VenueId, asset: AssetId) {
    let c = cash(l, A1, v, asset);
    let inv: i64 = l
        .venue_book(A1, v)
        .inv
        .iter()
        .filter(|e| e.asset == asset)
        .map(|e| e.qty.0)
        .sum();
    assert_eq!(
        c.total.0,
        c.baseline.0 + c.unassigned.0 + inv,
        "invariant {asset:?}"
    );
}

#[test]
fn led_170_adopt_position() {
    let mut l = short_1();
    let a = adj(
        P2,
        AdjustKind::Position,
        PxSrc::Cmd as u8,
        BTC_P2,
        AssetId(0),
        G1,
        btc("-0.5"),
        px("61000"),
        31,
        501,
    );
    let d = l.apply_adjust(&a, s(10));
    let p = l.pos(A1, BTC_P2);
    assert_eq!((p.qty, p.cost), (btc("-1.5"), cq("90500")));
    assert_eq!(l.leg_net(A1, G1, 1), btc("-1.5"));
    assert_eq!(d.d, PnlBreakdown::zero());
    let ns = notes_of(&l, code::ADJUSTED);
    assert_eq!(ns.len(), 1);
    assert_eq!(
        ns[0].v[..6],
        [av(A1, P2), 21, 31, (-50_000_000i64) as u64, 610_000_000, 0]
    );
    assert!(l.group_pos(A1, G1).flags & GroupPosView::ENTRY_ADOPTED != 0);
}

#[test]
fn led_171_adopt_realized_to_adjust() {
    let mut l = short_1();
    let round0 = l.group(A1, G1).round.pnl.price_realized;
    l.clear_notes();
    let b = adj(
        P2,
        AdjustKind::Position,
        PxSrc::Cmd as u8,
        BTC_P2,
        AssetId(0),
        G1,
        btc("0.5"),
        px("59000"),
        32,
        502,
    );
    l.apply_adjust(&b, s(10));
    assert_eq!(l.pos(A1, BTC_P2).qty, btc("-0.5"));
    let g = l.group(A1, G1);
    assert_eq!((g.pnl.adjust, g.pnl.price_realized), (n("500"), Notional(0)));
    assert_eq!(g.round.pnl.price_realized, round0);
    assert_eq!(notes_of(&l, code::ADJUSTED)[0].v[5], n("500").0 as u64);
}

#[test]
fn led_172_adopt_balance_to_group() {
    let mut l = booted();
    let (t0, i0) = (cash(&l, A1, S1, BTC).total, l.inventory(A1, S1, BTC, G1));
    let a = adj(
        S1,
        AdjustKind::Balance,
        PxSrc::Cmd as u8,
        BTC_S1,
        BTC,
        G1,
        btc("0.5"),
        px("60000"),
        33,
        503,
    );
    l.apply_adjust(&a, s(10));
    assert_eq!(cash(&l, A1, S1, BTC).total.0 - t0.0, btc("0.5").0);
    assert_eq!(l.inventory(A1, S1, BTC, G1).0 - i0.0, btc("0.5").0);
    let p = l.pos(A1, BTC_S1);
    assert_eq!((p.qty, p.cost), (btc("0.5"), cq("30000")));
    invariant(&l, S1, BTC);
}

#[test]
fn led_173_adopt_balance_unassigned() {
    let mut l = booted();
    let c0 = cash(&l, A1, S1, BTC);
    let inv0 = l.venue_book(A1, S1).inv.iter().copied().collect::<Vec<_>>();
    let p0 = *l.pos(A1, BTC_S1);
    let a = adj(
        S1,
        AdjustKind::Balance,
        PxSrc::Bid as u8,
        BTC_S1,
        BTC,
        GROUP_NONE,
        btc("0.5"),
        px("59999.99"),
        34,
        504,
    );
    l.apply_adjust(&a, s(10));
    let c = cash(&l, A1, S1, BTC);
    assert_eq!(
        (c.total.0 - c0.total.0, c.unassigned.0 - c0.unassigned.0),
        (btc("0.5").0, btc("0.5").0)
    );
    assert_eq!(l.venue_book(A1, S1).inv.iter().copied().collect::<Vec<_>>(), inv0);
    assert_eq!(*l.pos(A1, BTC_S1), p0);
}

#[test]
fn led_174_write_off_perp_leg() {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0", USDT, G1, 0), s(1));
    l.apply_fill(&f(Sell, BTC_P2, "1.004", "60000", "0", USDT, G1, 1), s(1));
    mark(&mut l, BTC_P2, "60000", s(1));
    l.clear_notes();
    let p0 = *l.pos(A1, BTC_P2);
    let cash0: Vec<_> = [S1, P2]
        .iter()
        .flat_map(|v| {
            l.venue_book(A1, *v)
                .cash
                .iter()
                .map(|e| e.total)
                .collect::<Vec<_>>()
        })
        .collect();
    let (w0, g0, a0) = (l.margin(A1, P2).wallet, *l.group(A1, G1), l.account(A1).day);
    let a = adj(
        P2,
        AdjustKind::WriteOff,
        0,
        BTC_P2,
        ASSET_NONE,
        G1,
        btc("-0.004"),
        Px(0),
        0,
        601,
    );
    let d = l.apply_adjust(&a, s(10));
    assert_eq!(l.leg_net(A1, G1, 1), btc("-1.0"));
    assert_eq!((l.pos(A1, BTC_P2).qty, l.pos(A1, BTC_P2).cost), (p0.qty, p0.cost));
    assert_eq!(l.ungrouped_qty(A1, BTC_P2), btc("-0.004"));
    let cash1: Vec<_> = [S1, P2]
        .iter()
        .flat_map(|v| {
            l.venue_book(A1, *v)
                .cash
                .iter()
                .map(|e| e.total)
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(cash1, cash0);
    assert_eq!(l.margin(A1, P2).wallet, w0);
    let g = l.group(A1, G1);
    assert_eq!(
        (g.pnl, g.day.price_realized, g.day.funding, g.day.fee_cex),
        (g0.pnl, g0.day.price_realized, g0.day.funding, g0.day.fee_cex)
    );
    assert_eq!(l.account(A1).day.net_realized(), a0.net_realized());
    assert_eq!(d.d, PnlBreakdown::zero());
    let ns = notes_of(&l, code::RESIDUAL_WRITTEN_OFF);
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].level, NoteLevel::P1);
    assert_eq!(
        ns[0].v[..6],
        [
            av(A1, P2),
            u64::from(G1.0),
            21,
            (-400_000i64) as u64,
            240_000_000,
            601
        ]
    );
}

#[test]
fn led_175_write_off_spot_leg() {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "1.004", "60000", "0", USDT, G1, 0), s(1));
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    let (u0, p0) = (cash(&l, A1, S1, BTC).unassigned, *l.pos(A1, BTC_S1));
    let a = adj(
        S1,
        AdjustKind::WriteOff,
        0,
        BTC_S1,
        BTC,
        G1,
        btc("0.004"),
        Px(0),
        0,
        602,
    );
    l.apply_adjust(&a, s(10));
    assert_eq!(l.inventory(A1, S1, BTC, G1), btc("1.0"));
    assert_eq!(cash(&l, A1, S1, BTC).unassigned.0 - u0.0, btc("0.004").0);
    assert_eq!(cash(&l, A1, S1, BTC).total, btc("1.004"));
    assert_eq!(*l.pos(A1, BTC_S1), p0);
    invariant(&l, S1, BTC);
}

#[test]
fn led_176_write_off_closes_round() {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "0.0001", "60000", "0", USDT, G1, 0), s(1));
    assert!(
        l.group(A1, G1).round.round_id != 0 || l.group(A1, G1).held_notional.0 > 0,
        "round open"
    );
    l.clear_notes();
    let a = adj(
        S1,
        AdjustKind::WriteOff,
        0,
        BTC_S1,
        BTC,
        G1,
        btc("0.0001"),
        Px(0),
        0,
        603,
    );
    l.apply_adjust(&a, s(10));
    assert_eq!(notes_of(&l, code::ROUND_CLOSED).len(), 1);
    let g = l.group(A1, G1);
    assert_eq!(g.held_notional, Notional(0));
    assert_eq!(g.round.legs, [LegAgg::ZERO; 4]);
}

#[test]
fn led_177_write_off_zero_note_only() {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0", USDT, G1, 0), s(1));
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l.clear_notes();
    let before = snap(&l);
    let a = adj(
        P2,
        AdjustKind::WriteOff,
        0,
        BTC_P2,
        ASSET_NONE,
        G1,
        Qty(0),
        Px(0),
        0,
        604,
    );
    l.apply_adjust(&a, s(10));
    assert_eq!(snap(&l), before);
    let ns = notes_of(&l, code::RESIDUAL_WRITTEN_OFF);
    assert_eq!(ns.len(), 1);
    assert_eq!((ns[0].v[3], ns[0].v[4]), (0, 0));
}
