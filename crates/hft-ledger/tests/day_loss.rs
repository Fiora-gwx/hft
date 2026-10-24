//! LED-306–LED-308: a quote asset without a USDT rate source is refused at load; outcome tokens
//! delivered at 0 book their cost as a realized day loss in USDC at par; the day loss carried
//! from earlier journals of the same UTC day.

use crate::common;

use common::journal::*;
use common::*;
use hft_event::{ExternalCause, GroupSpec, NoteLevel, NoteSource};
use hft_ledger::{AssetName, FillClass, FillDelta, FixedVec, Ledger, LedgerConfig, LedgerConfigError};
use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, Exchange, FeeAssetRule, FeePair, FeeSpec, InstKind,
    InstSpec, InstStatus, InstanceSpec, RdbBuilder, RefData,
};
use hft_types::{
    ASSET_NONE, AccountId, AssetExp, AssetId, GROUP_NONE, InstId, Notional, Px, Qty, Rate, Side, StrategyId,
    Tsc, VenueId,
};

const PM: VenueId = VenueId::POLYMARKET_CLOB;
const PUSDC: AssetId = AssetId(1);
const UP: AssetId = AssetId(2);
const DN: AssetId = AssetId(3);
const UP_I: InstId = InstId(1);
const DN_I: InstId = InstId(2);
const PA: AccountId = AccountId(1);
const PI: StrategyId = StrategyId(1);
const PG: hft_types::GroupId = gid(PI, 1);

/// A Polymarket-style refdata: quote USDC (`px_exp −6`, `qty_exp −6`), two outcome tokens
/// (`px_exp −3`, `qty_exp −6`, tick 0.01, lot 0.01 share) quoted in USDC, one live account.
fn rd_pm() -> &'static RefData {
    static RD: std::sync::OnceLock<&'static RefData> = std::sync::OnceLock::new();
    RD.get_or_init(|| {
        let mut b = RdbBuilder::new(1);
        b.created_utc_ns(D);
        b.asset(AssetSpec {
            id: PUSDC,
            name: "USDC".into(),
            class: AssetClass::Stable,
            exp: AssetExp {
                px_exp: -6,
                qty_exp: -6,
            },
            px_max: Px(1_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
        for (id, name) in [(UP, "PMUP"), (DN, "PMDN")] {
            b.asset(AssetSpec {
                id,
                name: name.into(),
                class: AssetClass::Crypto,
                exp: AssetExp {
                    px_exp: -3,
                    qty_exp: -6,
                },
                px_max: Px(1_000),
                qty_max: Qty(1_000_000_000_000_000),
                dust: Qty(1),
            });
        }
        b.venue(venue_spec(PM, Exchange::Polymarket, InstKind::Spot));
        b.caps(spot_caps(PM, FeeAssetRule::Quote));
        for (id, asset, sym) in [(UP_I, UP, "btc-up-0"), (DN_I, DN, "btc-dn-0")] {
            b.inst(InstSpec {
                id,
                venue: PM,
                symbol: sym.into(),
                status: InstStatus::Trading,
                kind: InstKind::Spot,
                asset,
                quote: PUSDC,
                tick: 10,
                lot: 10_000,
                min_qty: Qty(10_000),
                max_qty: Qty(1_000_000_000_000),
                min_notional: Notional(1),
                book_window: 64,
                l2_depth: 20,
                crossed_tolerance: 0,
                tick_str: raw_str(10, -3),
                lot_str: raw_str(10_000, -6),
                min_qty_str: raw_str(10_000, -6),
                ..InstSpec::default()
            });
        }
        b.fee(FeeSpec {
            venue: PM,
            tier: 0,
            fee: FeePair {
                maker: Rate(0),
                taker: Rate(0),
            },
        });
        b.instance(InstanceSpec {
            id: PI,
            kind_id: 0xFF00,
            name: "pm-maker".into(),
        });
        b.account(AccountSpec {
            id: PA,
            sim: false,
            bound_instance: PI,
            label: "pm-live".into(),
            venues: 1u64 << PM.0,
            ..AccountSpec::default()
        });
        b.account_venue(AccountVenueSpec {
            account: PA,
            venue: PM,
            uid: "pm".into(),
        });
        let blob = b.finish();
        match RefData::load(&blob, None) {
            Ok(r) => Box::leak(Box::new(r)),
            Err(e) => panic!("rd_pm rejected: {e:?}"),
        }
    })
}

fn names(v: &[&str]) -> FixedVec<AssetName, 8> {
    let mut f = FixedVec::new();
    for n in v {
        f.push(AssetName::new(n));
    }
    f
}

fn cfg_with(par: &[&str], fallback: &[&str]) -> LedgerConfig {
    let mut c = cfg_lf();
    c.par_assets = names(par);
    c.stable_par_fallback = names(fallback);
    c
}

#[test]
fn led_306_quote_asset_without_usdt_rate_refused() {
    let new = |rd: &RefData, c: &LedgerConfig| Ledger::new(rd, c, &caps_lf()).err();
    assert_eq!(
        new(rd_pm(), &cfg_with(&["USDT"], &[])),
        Some(LedgerConfigError::NoUsdtRate { asset: PUSDC }),
        "LED-306.1"
    );
    assert_eq!(new(rd_pm(), &cfg_with(&["USDT", "USDC"], &[])), None, "LED-306.2");
    assert_eq!(new(rd_pm(), &cfg_with(&["USDT"], &["USDC"])), None, "LED-306.3");
    // rd_lf: USDC has the reference inst USDCUSDT.
    assert_eq!(new(rd_lf(), &cfg_with(&["USDT"], &[])), None, "LED-306.4");
}

fn pm_group() -> GroupSpec {
    spec(PG, UP, &[(UP_I, PM), (DN_I, PM)])
}

/// `rd_pm` with USDC at par, 82 USDC deposited at `t = 0`.
fn pm_ledger() -> Ledger {
    let mut l = Ledger::new(rd_pm(), &cfg_with(&["USDT", "USDC"], &[]), &caps_lf()).expect("new");
    l.on_groups_changed(PI, &table(&[pm_group()]));
    l.on_run_start(0);
    for (a, q) in [(PUSDC, 82_000_000), (UP, 0), (DN, 0)] {
        l.on_balance(&bal(PA, PM, a, Qty(q), Qty(0), D), true, 0);
    }
    l
}

fn pm_buy(inst: InstId, leg: u8, shares: i64, px_milli: i64) -> FillDelta {
    FillDelta {
        account: PA,
        venue: PM,
        side: Side::Buy,
        class: FillClass::Open as u8,
        flags: 0,
        leg_ix: leg,
        _pad: [0; 2],
        inst,
        group: PG,
        qty: Qty(shares * 1_000_000),
        net_qty: Qty(shares * 1_000_000),
        px: Px(px_milli),
        fee: 0,
        fee_asset: ASSET_NONE,
        _pad2: 0,
        slot: 0,
        r#gen: 0,
        _pad3: 0,
        seq: 0,
    }
}

fn delivery(inst: InstId, shares: i64, payout_milli: i64) -> hft_event::ExternalFill {
    hft_event::ExternalFill {
        venue: PM,
        account: PA,
        side: Side::Sell,
        cause: ExternalCause::Delivery,
        inst,
        qty: Qty(shares * 1_000_000),
        px: Px(payout_milli),
        fee: Qty(0),
        fee_asset: ASSET_NONE,
        _pad: [0; 6],
        exch_order_id: 0,
        trade_id: 0x5e7 + inst.0 as u64,
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
        fill_seq: 0,
        cash_qty: Qty(0),
        cash_amount: Qty(0),
    }
}

#[test]
fn led_307_token_delivered_at_zero_is_a_realized_day_loss() {
    let mut l = pm_ledger();
    // .1 10 Up at 0.600 and 10 Down at 0.450; the window resolves Down.
    l.apply_fill(&pm_buy(UP_I, 0, 10, 600), s(1));
    l.apply_fill(&pm_buy(DN_I, 1, 10, 450), s(1));
    assert_eq!(
        l.account(PA).day.net(),
        Notional(0),
        "LED-307.1 bought at the last price"
    );
    assert_eq!(
        l.account(PA).held_notional,
        Notional(10_500_000),
        "LED-307.1 held in USDC"
    );
    // .2 the loser delivered at 0: its whole cost (6 USDC) is a realized loss, in `external`.
    l.apply_external(&delivery(UP_I, 10, 0), false, s(2))
        .expect("booked");
    assert_eq!(l.account(PA).day.external, Notional(-6_000_000), "LED-307.2");
    assert_eq!(l.account(PA).day.net(), Notional(-6_000_000), "LED-307.2");
    assert_eq!(l.pos(PA, UP_I).qty, Qty(0), "LED-307.2");
    // .3 the winner delivered at 1.000: +5.5 USDC; day −0.5 USDC; cash 82 − 10.5 + 10 = 81.5.
    l.apply_external(&delivery(DN_I, 10, 1_000), false, s(3))
        .expect("booked");
    assert_eq!(l.account(PA).day.net(), Notional(-500_000), "LED-307.3");
    // Both token books quote (the tokens' reference insts are their own USDC books).
    l.on_ref_px(UP_I, Px(10), Px(20), s(3));
    l.on_ref_px(DN_I, Px(980), Px(990), s(3));
    let e = l.equity(PA);
    assert!(e.valid, "LED-307.3 USDC at par: equity valid");
    assert_eq!(e.total, Notional(81_500_000), "LED-307.3");
}

#[test]
fn led_308_day_pnl_carry() {
    // .1 a carried loss goes to the account's day `adjust` only, with one Note.
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "0.001", "60000", "0", USDT, G1, 0), s(1));
    let g_day = l.group(A1, G1).day;
    let before = l.account(A1).day.net();
    l.clear_notes();
    let d = l.carry_day_pnl(A1, n("-28.51"), s(2));
    assert_eq!(
        (d.account, d.group, d.d.adjust),
        (A1, GROUP_NONE, n("-28.51")),
        "LED-308.1"
    );
    assert_eq!(l.account(A1).day.adjust, n("-28.51"), "LED-308.1");
    assert_eq!(
        l.account(A1).day.net(),
        Notional(before.0 - 28_510_000),
        "LED-308.1"
    );
    assert_eq!(l.group(A1, G1).day, g_day, "LED-308.1 group day unchanged");
    let notes: Vec<_> = l.notes().to_vec();
    assert_eq!(notes.len(), 1, "LED-308.1");
    assert_eq!(
        (notes[0].source, notes[0].code, notes[0].level),
        (
            NoteSource::Ledger,
            hft_ledger::notes::code::DAY_PNL_CARRIED,
            NoteLevel::Info
        ),
        "LED-308.1"
    );
    assert_eq!(
        notes[0].v,
        [1, (-28_510_000i64) as u64, 0, 0, 0, 0, 0, 0],
        "LED-308.1"
    );
    // .2 zero, a profit or an unknown account: nothing booked, no Note.
    l.clear_notes();
    for (a, p) in [(A1, "0"), (A1, "5"), (AccountId(0), "-1")] {
        let d = l.carry_day_pnl(a, n(p), s(3));
        assert_eq!(d.d, hft_ledger::PnlBreakdown::zero(), "LED-308.2 {a:?} {p}");
    }
    assert!(l.notes().is_empty(), "LED-308.2");
    assert_eq!(l.account(A1).day.adjust, n("-28.51"), "LED-308.2");
    // .3 the snapshot keeps it.
    let r = restore_with(&snap(&l), rd_lf()).expect("restore");
    assert_eq!(r.account(A1).day, l.account(A1).day, "LED-308.3");
    // .4 the cold consumer applies the Note to the same account.
    let mut cold = cold_from(&booted());
    clock(&mut cold, s(2));
    let before = cold.ledger().account(A1).day.adjust;
    feed(&mut cold, &notes[0], true);
    assert_eq!(
        cold.ledger().account(A1).day.adjust.0,
        before.0 - 28_510_000,
        "LED-308.4"
    );
    // .5 the next day roll clears it with the rest of the day book.
    l.on_day_roll(s(4));
    assert_eq!(l.account(A1).day, hft_ledger::PnlBreakdown::zero(), "LED-308.5");
}
