//! LED-050–LED-074: external fills — dedup, AUTO_EXTERNAL and auto_held, attribution, notes.

mod common;

use common::*;
use hft_event::{ExternalCause, ExternalFill, NoteLevel};
use hft_ledger::notes::code;
use hft_ledger::snapshot::{AutoHeldRow, ExtSeenRow, LedgerRowHdr, SECTION_LEDGER_GROUPS, row_kind};
use hft_ledger::{ExternalApplied, KEY_TRADE_ID, Ledger, aflags};
use hft_types::{ASSET_NONE, AccountId, GROUP_NONE, INST_NONE, Notional, Qty, Side, Tsc};
use zerocopy::IntoBytes;

use ExternalCause::{Adl, Delivery, Liquidation, Manual, Unknown};

/// LED-050's state before `E1`: `G1` perp leg −1.0 at 60 000.
fn pre_050() -> Ledger {
    let mut l = booted();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l.clear_notes();
    l
}

fn e1() -> ExternalFill {
    let mut e = ext_tid(Liquidation, Buy, BTC_P2, "0.2", "61000", 900);
    e.fee_asset = USDT;
    e
}

fn applied(l: &mut Ledger, e: &ExternalFill, auto: bool, t: i64) -> Option<ExternalApplied> {
    l.apply_external(e, auto, t)
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn row_bytes(h: LedgerRowHdr, body: &[u8]) -> Vec<u8> {
    let mut v = h.as_bytes().to_vec();
    v.extend_from_slice(body);
    v
}

#[test]
fn led_050_dup_trade_id() {
    let mut l = pre_050();
    assert!(applied(&mut l, &e1(), false, s(10)).is_some());
    let before = snap(&l);
    l.clear_notes();
    assert!(applied(&mut l, &e1(), false, s(11)).is_none());
    assert_eq!(snap(&l), before);
    assert!(l.notes().is_empty());
}

#[test]
fn led_051_dedup_key_forms() {
    let base = || {
        let mut l = pre_050();
        assert!(applied(&mut l, &e1(), false, s(10)).is_some());
        l
    };
    // .1
    let mut l = base();
    let mut e = e1();
    e.inst = ETH_P2;
    assert!(applied(&mut l, &e, false, s(11)).is_some(), "LED-051.1");
    // .2
    let mut l = base();
    let mut e = e1();
    e.trade_id = 0;
    e.exch_order_id = 55;
    e.fill_seq = 20_000_000;
    assert!(applied(&mut l, &e, false, s(11)).is_some(), "LED-051.2 first");
    assert!(applied(&mut l, &e, false, s(12)).is_none(), "LED-051.2 second");
    // .3
    let mut l = base();
    e.fill_seq = 30_000_000;
    assert!(applied(&mut l, &e, false, s(11)).is_some(), "LED-051.3");
    // .4
    let mut l = base();
    boot_a2(&mut l, 0, "10000", "10000");
    let mut e = e1();
    e.account = A2;
    assert!(applied(&mut l, &e, false, s(11)).is_some(), "LED-051.4");
}

#[test]
fn led_052_dedup_ring_capacity() {
    let mut cfg = cfg_lf();
    cfg.ext_dedup_cap = 4;
    let mut l = ledger_with(rd_lf(), &cfg);
    boot_a1(&mut l, 0);
    let k = |i: u64| ext_tid(Liquidation, Buy, BTC_P2, "0.001", "60000", i);
    for i in 1..=4 {
        assert!(applied(&mut l, &k(i), false, s(i as i64)).is_some());
    }
    assert!(applied(&mut l, &k(1), false, s(10)).is_none(), "LED-052.1");
    assert!(applied(&mut l, &k(5), false, s(11)).is_some(), "LED-052.2");
    let ring: Vec<u64> = l.ext_seen(A1, P2).iter().map(|x| x.a).collect();
    assert_eq!(ring, [2, 3, 4, 5], "LED-052.2");
    assert!(applied(&mut l, &k(1), false, s(12)).is_some(), "LED-052.3");
    assert!(applied(&mut l, &k(5), false, s(13)).is_none(), "LED-052.4");
}

#[test]
fn led_053_ext_seen_survives_restore() {
    let mut l = pre_050();
    applied(&mut l, &e1(), false, s(10));
    let bytes = snap(&l);
    let want = row_bytes(
        LedgerRowHdr {
            kind: row_kind::EXT_SEEN,
            account: A1,
            venue: P2,
            _pad: 0,
            id: BTC_P2.0,
        },
        ExtSeenRow {
            kind: KEY_TRADE_ID,
            _pad: [0; 7],
            a: 900,
            b: 0,
        }
        .as_bytes(),
    );
    assert!(contains(section(&bytes, SECTION_LEDGER_GROUPS).body, &want));
    let mut r = restore(&bytes).expect("restore");
    assert!(applied(&mut r, &e1(), false, s(20)).is_none());
}

#[test]
fn led_054_auto_external_fill_fields() {
    let mk = |d: &str| Ledger::auto_external_fill(A1, P2, BTC_P2, btc(d), px("60500"), 4242);
    for (k, (d, side, q)) in [("-2.0", Buy, "2.0"), ("4.0", Sell, "4.0"), ("10.0", Sell, "10.0")]
        .iter()
        .enumerate()
    {
        let e = mk(d);
        assert_eq!((e.side, e.qty), (*side, btc(q)), "LED-054.{}", k + 1);
        assert_eq!(e.cause, Unknown, "LED-054.4");
        assert_eq!(e.px, px("60500"), "LED-054.4");
        assert_eq!((e.fee, e.fee_asset), (Qty(0), ASSET_NONE), "LED-054.4");
        assert_eq!(
            (e.trade_id, e.exch_order_id, e.fill_seq),
            (0, 0, 4242),
            "LED-054.4"
        );
        assert_eq!((e.ex_ts_ns, e.rx_tsc), (0, Tsc(0)), "LED-054.4");
        assert_eq!((e.account, e.venue, e.inst), (A1, P2, BTC_P2), "LED-054.4");
        assert_eq!(e.as_bytes(), mk(d).as_bytes(), "LED-054.4 deterministic");
    }
}

/// LED-055's state: `G1` perp leg long 10.0.
fn long_10() -> Ledger {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_P2, "10.0", "60000", "0", USDT, G1, 1), s(1));
    l.clear_notes();
    l
}

fn auto_fill(d: &str, p: &str, rid: u64) -> ExternalFill {
    Ledger::auto_external_fill(A1, P2, BTC_P2, btc(d), px(p), rid)
}

#[test]
fn led_055_auto_body_once_per_report() {
    let mut l = long_10();
    assert!(applied(&mut l, &auto_fill("4.0", "60500", 4242), true, s(10)).is_some());
    assert_eq!(l.pos(A1, BTC_P2).qty, btc("6.0"));
    assert!(applied(&mut l, &auto_fill("4.0", "60500", 4242), true, s(10)).is_none());
    assert_eq!(l.pos(A1, BTC_P2).qty, btc("6.0"));
}

/// LED-056's state.
fn after_056() -> Ledger {
    let mut l = long_10();
    applied(&mut l, &auto_fill("4.0", "60500", 4242), true, s(10));
    l
}

#[test]
fn led_056_auto_body_sets_auto_held() {
    let l = after_056();
    assert_eq!(l.auto_held(A1, BTC_P2, Sell), btc("4.0"));
    assert_eq!(l.auto_held(A1, BTC_P2, Buy), Qty(0));
    let ns = notes_of(&l, code::EXTERNAL_FILL);
    assert_eq!(ns.len(), 1);
    assert!((ns[0].v[0] >> 24) & 0xFF & aflags::AUTO_RECON != 0);
}

fn e2() -> ExternalFill {
    ext_tid(Adl, Sell, BTC_P2, "4.0", "60400", 7001)
}

#[test]
fn led_057_adl_fully_offset() {
    let mut l = after_056();
    l.clear_notes();
    let before = (
        *l.group(A1, G1),
        cash(&l, A1, P2, USDT).total,
        l.margin(A1, P2).wallet,
    );
    assert!(applied(&mut l, &e2(), false, s(20)).is_none());
    assert_eq!(l.pos(A1, BTC_P2).qty, btc("6.0"));
    assert_eq!(
        (
            *l.group(A1, G1),
            cash(&l, A1, P2, USDT).total,
            l.margin(A1, P2).wallet
        ),
        before
    );
    assert_eq!(l.auto_held(A1, BTC_P2, Sell), Qty(0));
    let ns = notes_of(&l, code::AUTO_HELD_OFFSET);
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].level, NoteLevel::P0);
    let v0 = u64::from(P2.0) | u64::from(A1.0) << 8;
    assert_eq!(
        ns[0].v[..6],
        [
            v0,
            u64::from(BTC_P2.0),
            Side::Sell as u64,
            btc("4.0").0 as u64,
            btc("4.0").0 as u64,
            0
        ]
    );
    assert!(
        l.ext_seen(A1, P2)
            .iter()
            .any(|k| k.kind == KEY_TRADE_ID && k.a == 7001)
    );
}

#[test]
fn led_058_adl_partially_offset() {
    let mut l = after_056();
    l.clear_notes();
    let mut e3 = e2();
    e3.qty = btc("6.0");
    e3.fee = usdt("0.6");
    e3.fee_asset = USDT;
    e3.trade_id = 7002;
    let ext0 = l.group(A1, G1).pnl.external;
    let r = applied(&mut l, &e3, false, s(20)).expect("Some");
    assert_eq!(
        (r.group, r.leg_ix, r.net_qty, r.none_qty),
        (G1, 1, btc("-2.0"), Qty(0))
    );
    assert_eq!(l.pos(A1, BTC_P2).qty, btc("4.0"));
    // Realized 2.0 × (60 400 − 60 000) = 800, fee 0.6 × 2 / 6 = 0.2.
    assert_eq!(l.group(A1, G1).pnl.external.0 - ext0.0, n("799.8").0);
    assert_eq!(l.auto_held(A1, BTC_P2, Sell), Qty(0));
    let ns = notes_of(&l, code::AUTO_HELD_OFFSET);
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].v[3..6], [btc("6.0").0 as u64, btc("4.0").0 as u64, 0]);
}

#[test]
fn led_059_offset_conditions() {
    let base = || {
        let mut l = after_056();
        l.apply_fill(&f(Buy, ETH_P2, "2.0", "3000", "0", USDT, G3, 0), s(11));
        l.clear_notes();
        l
    };
    let held = |l: &Ledger| l.auto_held(A1, BTC_P2, Sell);
    let offsets = |l: &Ledger| notes_of(l, code::AUTO_HELD_OFFSET).len();
    // .1
    let mut l = base();
    let q0 = l.pos(A1, BTC_P2).qty.0;
    assert!(
        applied(
            &mut l,
            &ext_tid(Adl, Buy, BTC_P2, "1.0", "60000", 8001),
            false,
            s(20)
        )
        .is_some()
    );
    assert_eq!(l.pos(A1, BTC_P2).qty.0 - q0, btc("1.0").0, "LED-059.1");
    assert_eq!((held(&l), offsets(&l)), (btc("4.0"), 0), "LED-059.1");
    // .2, .3
    for (k, cause) in [(2, Manual), (3, Delivery)] {
        let mut l = base();
        let q0 = l.pos(A1, BTC_P2).qty.0;
        assert!(
            applied(
                &mut l,
                &ext_tid(cause, Sell, BTC_P2, "4.0", "60000", 8000 + k),
                false,
                s(20)
            )
            .is_some()
        );
        assert_eq!(q0 - l.pos(A1, BTC_P2).qty.0, btc("4.0").0, "LED-059.{k}");
        assert_eq!(held(&l), btc("4.0"), "LED-059.{k}");
    }
    // .4
    let mut l = base();
    let q0 = l.pos(A1, ETH_P2).qty.0;
    assert!(
        applied(
            &mut l,
            &ext_tid(Adl, Sell, ETH_P2, "1.0", "3000", 8004),
            false,
            s(20)
        )
        .is_some()
    );
    assert_eq!(q0 - l.pos(A1, ETH_P2).qty.0, qa(ETH, "1.0").0, "LED-059.4");
    assert_eq!(held(&l), btc("4.0"), "LED-059.4");
    // .5
    let mut l = base();
    assert!(
        applied(
            &mut l,
            &ext_tid(Liquidation, Sell, BTC_P2, "1.0", "60000", 8005),
            false,
            s(20)
        )
        .is_none()
    );
    assert_eq!((held(&l), offsets(&l)), (btc("3.0"), 1), "LED-059.5");
    // .6
    let mut l = base();
    assert!(
        applied(
            &mut l,
            &ext_tid(Unknown, Sell, BTC_P2, "1.0", "60000", 8006),
            false,
            s(20)
        )
        .is_none()
    );
    assert_eq!(held(&l), btc("3.0"), "LED-059.6");
    // .7
    let mut l = base();
    let q0 = l.pos(A1, BTC_P2).qty.0;
    assert!(applied(&mut l, &auto_fill("3.0", "60500", 4243), true, s(20)).is_some());
    assert_eq!(q0 - l.pos(A1, BTC_P2).qty.0, btc("3.0").0, "LED-059.7");
    assert_eq!(held(&l), btc("7.0"), "LED-059.7");
}

#[test]
fn led_060_offset_dedup_on_replay() {
    let mut l = after_056();
    applied(&mut l, &e2(), false, s(20));
    l.clear_notes();
    assert!(applied(&mut l, &e2(), false, s(30)).is_none());
    assert!(l.notes().is_empty());
    assert_eq!(l.auto_held(A1, BTC_P2, Sell), Qty(0));
}

#[test]
fn led_061_auto_held_survives_restore() {
    let l = after_056();
    let bytes = snap(&l);
    let want = row_bytes(
        LedgerRowHdr {
            kind: row_kind::AUTO_HELD,
            account: A1,
            venue: P2,
            _pad: 0,
            id: BTC_P2.0,
        },
        AutoHeldRow {
            buy: Qty(0),
            sell: btc("4.0"),
        }
        .as_bytes(),
    );
    let body = section(&bytes, SECTION_LEDGER_GROUPS).body;
    assert!(contains(body, &want));
    // Only one AutoHeld row (zero entries are not written).
    let hdr_only = LedgerRowHdr {
        kind: row_kind::AUTO_HELD,
        account: A1,
        venue: P2,
        _pad: 0,
        id: ETH_P2.0,
    };
    assert!(!contains(body, hdr_only.as_bytes()));
    let mut r = restore(&bytes).expect("restore");
    assert!(applied(&mut r, &e2(), false, s(20)).is_none());
    assert_eq!(notes_of(&r, code::AUTO_HELD_OFFSET).len(), 1);
}

#[test]
fn led_062_single_candidate() {
    let mut l = pre_050();
    let mut e = ext_tid(Liquidation, Buy, BTC_P2, "0.4", "61000", 901);
    e.fee = usdt("0.2");
    e.fee_asset = USDT;
    let r = applied(&mut l, &e, false, s(10)).expect("Some");
    assert_eq!(
        (r.group, r.leg_ix, r.net_qty, r.none_qty, r.notional),
        (G1, 1, btc("0.4"), Qty(0), n("24400"))
    );
    assert_eq!(l.leg_net(A1, G1, 1), btc("-0.6"));
    assert_eq!(l.pos(A1, BTC_P2).qty, btc("-0.6"));
    assert_eq!(l.group(A1, G1).pnl.external, n("-400.2"));
}

#[test]
fn led_063_no_candidate() {
    let mut l = booted();
    boot_a2(&mut l, 0, "10000", "10000");
    // .1
    let mut e = ext(Manual, Buy, BTCUSDC_S3, "0.1", "60000");
    e.account = A2;
    let (b0, u0) = (cash(&l, A2, S3, BTC).unassigned, cash(&l, A2, S3, USDC).total);
    let r = applied(&mut l, &e, false, s(10)).expect("Some");
    assert_eq!(
        (r.group, r.net_qty, r.none_qty),
        (GROUP_NONE, Qty(0), btc("0.1")),
        "LED-063.1"
    );
    assert_eq!(
        cash(&l, A2, S3, BTC).unassigned.0 - b0.0,
        btc("0.1").0,
        "LED-063.1"
    );
    assert_eq!(
        cash(&l, A2, S3, USDC).total.0 - u0.0,
        -qa(USDC, "6000").0,
        "LED-063.1"
    );
    // .2
    let mut e = ext(Liquidation, Sell, ETH_P2, "0.5", "3000");
    e.account = A2;
    let r = applied(&mut l, &e, false, s(11)).expect("Some");
    assert_eq!(r.none_qty, qa(ETH, "-0.5"), "LED-063.2");
    assert_eq!(l.pos(A2, ETH_P2).qty, qa(ETH, "-0.5"), "LED-063.2");
    assert_eq!(l.ungrouped_qty(A2, ETH_P2), qa(ETH, "-0.5"), "LED-063.2");
}

fn g5() -> hft_event::GroupSpec {
    spec(G5, BTC, &[(BTC_P2, P2)])
}

/// LED-064's state: LED-050 + `G5` (Shadow) on `BTC/P2`.
fn pre_064(g5_shadow: bool) -> Ledger {
    let mut l = ledger_lf();
    set_groups(&mut l, &[g5()]);
    boot_a1(&mut l, 0);
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l.set_cell_shadow(A1, G5, g5_shadow);
    l.clear_notes();
    l
}

#[test]
fn led_064_shadow_cell_not_candidate() {
    let mut l = pre_064(true);
    let g5_before = (l.leg_net(A1, G5, 0), *l.group(A1, G5));
    let r = applied(
        &mut l,
        &ext_tid(Liquidation, Buy, BTC_P2, "0.4", "61000", 902),
        false,
        s(10),
    )
    .expect("Some");
    assert_eq!(r.group, G1);
    assert_eq!((l.leg_net(A1, G5, 0), *l.group(A1, G5)), g5_before);
}

#[test]
fn led_065_all_candidates_shadow() {
    let mut l = pre_064(true);
    l.set_cell_shadow(A1, G1, true);
    let legs = (l.leg_net(A1, G1, 1), l.leg_net(A1, G5, 0));
    let u0 = l.ungrouped_qty(A1, BTC_P2);
    let r = applied(
        &mut l,
        &ext_tid(Liquidation, Buy, BTC_P2, "0.4", "61000", 903),
        false,
        s(10),
    )
    .expect("Some");
    assert_eq!((r.group, r.none_qty), (GROUP_NONE, btc("0.4")));
    assert_eq!((l.leg_net(A1, G1, 1), l.leg_net(A1, G5, 0)), legs);
    assert_eq!(l.ungrouped_qty(A1, BTC_P2).0 - u0.0, btc("0.4").0);
}

#[test]
fn led_066_two_candidates_ambiguous() {
    let mut l = pre_064(false);
    let r = applied(
        &mut l,
        &ext_tid(Liquidation, Buy, BTC_P2, "0.4", "61000", 904),
        false,
        s(10),
    )
    .expect("Some");
    assert_eq!(r.group, GROUP_NONE);
    let ns = notes_of(&l, code::EXTERNAL_FILL);
    assert_eq!(ns.len(), 1);
    assert!((ns[0].v[0] >> 24) & 0xFF & aflags::AMBIGUOUS != 0);
}

/// LED-067's state: `G1` perp leg −1.0, account-level −0.004.
fn pre_067() -> Ledger {
    let mut l = pre_050();
    l.set_cell_shadow(A1, G1, true);
    applied(
        &mut l,
        &ext_tid(Manual, Sell, BTC_P2, "0.004", "60000", 905),
        false,
        s(2),
    );
    l.set_cell_shadow(A1, G1, false);
    assert_eq!(l.pos(A1, BTC_P2).qty, btc("-1.004"));
    assert_eq!(l.ungrouped_qty(A1, BTC_P2), btc("-0.004"));
    l.clear_notes();
    l
}

#[test]
fn led_067_attribute_until_leg_zero() {
    let mut l = pre_067();
    let r = applied(
        &mut l,
        &ext_tid(Liquidation, Buy, BTC_P2, "1.004", "61000", 906),
        false,
        s(10),
    )
    .expect("Some");
    assert_eq!(
        (r.group, r.leg_ix, r.net_qty, r.none_qty),
        (G1, 1, btc("1.0"), btc("0.004"))
    );
    assert_eq!(l.leg_net(A1, G1, 1), Qty(0));
    assert_eq!(l.pos(A1, BTC_P2).qty, Qty(0));
    assert_eq!(l.ungrouped_qty(A1, BTC_P2), Qty(0));
}

#[test]
fn led_068_leg_increase_fully_attributed() {
    let mut l = pre_050();
    let r = applied(
        &mut l,
        &ext_tid(Manual, Sell, BTC_P2, "0.3", "60000", 907),
        false,
        s(10),
    )
    .expect("Some");
    assert_eq!((r.net_qty, r.none_qty), (btc("-0.3"), Qty(0)));
    assert_eq!(l.leg_net(A1, G1, 1), btc("-1.3"));
}

#[test]
fn led_069_auto_body_leg_limit() {
    let mut l = pre_067();
    let r = applied(&mut l, &auto_fill("-1.004", "61000", 4244), true, s(10)).expect("Some");
    assert_eq!((r.net_qty, r.none_qty), (btc("1.0"), btc("0.004")));
    let ns = notes_of(&l, code::EXTERNAL_FILL);
    assert!((ns[0].v[0] >> 24) & 0xFF & aflags::AUTO_RECON != 0);
    assert_eq!(l.auto_held(A1, BTC_P2, Buy), btc("1.004"));
}

#[test]
fn led_070_external_column() {
    let mut l = pre_050();
    let (u0, w0) = (cash(&l, A1, P2, USDT).total, l.margin(A1, P2).wallet);
    let mut e = ext_tid(Liquidation, Buy, BTC_P2, "1.0", "61000", 908);
    e.fee = usdt("30.5");
    e.fee_asset = USDT;
    applied(&mut l, &e, false, s(10));
    let g = l.group(A1, G1).pnl;
    assert_eq!(
        (g.external, g.price_realized, g.fee_cex),
        (n("-1030.5"), Notional(0), Notional(0))
    );
    assert_eq!(cash(&l, A1, P2, USDT).total.0 - u0.0, usdt("-1030.5").0);
    assert_eq!(l.margin(A1, P2).wallet.0 - w0.0, n("-1030.5").0);
    assert_eq!(l.pos(A1, BTC_P2).qty, Qty(0));
}

#[test]
fn led_071_note_level_by_cause() {
    let rows = [
        (Liquidation, NoteLevel::P0),
        (Adl, NoteLevel::P0),
        (Manual, NoteLevel::P1),
        (Delivery, NoteLevel::P0),
        (Unknown, NoteLevel::P0),
    ];
    for (k, (cause, level)) in rows.iter().enumerate() {
        let mut l = pre_050();
        applied(
            &mut l,
            &ext_tid(*cause, Sell, BTC_P2, "0.001", "60000", 950 + k as u64),
            false,
            s(10),
        );
        let ns = notes_of(&l, code::EXTERNAL_FILL);
        assert_eq!(ns.len(), 1, "LED-071.{}", k + 1);
        assert_eq!(ns[0].level, *level, "LED-071.{}", k + 1);
    }
}

fn unpack(v0: u64) -> (u64, u64, u64, u64) {
    (v0 & 0xFF, v0 >> 8 & 0xFF, v0 >> 16 & 0xFF, v0 >> 24 & 0xFF)
}

#[test]
fn led_072_note_v_layout() {
    let venue = u64::from(P2.0);
    let acc = u64::from(A1.0);
    // .1
    let mut l = pre_050();
    let mut e = ext_tid(Adl, Sell, BTC_P2, "0.002", "60500", 0);
    e.exch_order_id = 77;
    e.fill_seq = 1;
    applied(&mut l, &e, false, s(10));
    let v = notes_of(&l, code::EXTERNAL_FILL)[0].v;
    assert_eq!(v[0], venue | acc << 8 | (Adl as u64) << 16, "LED-072.1");
    assert_eq!(
        v[1..8],
        [
            21,
            Side::Sell as u64,
            200_000,
            605_000_000,
            77,
            u64::from(G1.0),
            1
        ],
        "LED-072.1"
    );
    assert_eq!(unpack(v[0]), (venue, acc, Adl as u64, 0), "LED-072.3");
    // .2
    let mut l = pre_050();
    applied(&mut l, &auto_fill("0.002", "60500", 4245), true, s(10));
    let v = notes_of(&l, code::EXTERNAL_FILL)[0].v;
    assert_eq!(
        v[0],
        venue | acc << 8 | (Unknown as u64) << 16 | aflags::AUTO_RECON << 24,
        "LED-072.2"
    );
    assert_eq!(v[5], 0, "LED-072.2");
    assert_eq!(
        unpack(v[0]),
        (venue, acc, Unknown as u64, aflags::AUTO_RECON),
        "LED-072.3"
    );
}

#[test]
fn led_073_ext_dedup_counted_once() {
    let mut l = booted();
    let u0 = cash(&l, A1, S1, USDT).total;
    let mut e = ext(Unknown, Buy, BTC_S1, "0.4", "60000");
    e.exch_order_id = 88;
    e.fill_seq = btc("0.4").0 as u64;
    applied(&mut l, &e, false, s(10));
    let mut fl = f(Buy, BTC_S1, "0.6", "60000", "0", USDT, G1, 0);
    fl.flags = hft_ledger::fill_flags::EXT_DEDUP;
    l.apply_fill(&fl, s(11));
    assert_eq!(l.pos(A1, BTC_S1).qty, btc("1.0"));
    assert_eq!(l.inventory(A1, S1, BTC, G1), btc("1.0"));
    assert_eq!(cash(&l, A1, S1, USDT).total.0 - u0.0, -usdt("60000").0);
    let g = l.group(A1, G1).pnl;
    assert_eq!((g.external, g.price_realized), (Notional(0), Notional(0)));
}

#[test]
fn led_074_groups_changed_rebuilds_candidates() {
    let mut l = booted();
    l.on_groups_changed(I1, &table(&[g3()]));
    let r = applied(
        &mut l,
        &ext_tid(Manual, Buy, BTC_P2, "0.1", "60000", 909),
        false,
        s(10),
    )
    .expect("Some");
    assert_eq!((r.group, r.none_qty), (GROUP_NONE, btc("0.1")));
    let _ = (INST_NONE, AccountId(0));
}
