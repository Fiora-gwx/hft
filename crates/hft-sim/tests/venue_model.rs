//! Venue-behaviour options (README "场所行为选项"): SIM-232–SIM-239.
use crate::common::*;
use hft_event::{OrderEventKind, RepMsg};
use hft_sim::{Cond, FeeModel, LatPools, MakerFill, Pool, PoolLeg, TakerDelay, VenueModel};
use hft_types::Side::{Buy, Sell};
use hft_types::{Side, Tif};

/// `rd_binary()`: Up shows 50 @ 0.45 bid / 50 @ 0.47 ask; account 9 has 1 000 USDT on B5.
fn binary(fill: MakerFill, vm: VenueModel) -> Sim {
    let mut setup = setup_sim();
    setup.tuning.maker_fill = fill;
    let mut a9 = cfg_a9();
    a9.initial_balances = vec![bal(B5, USDT, usdt("1000"))].into();
    setup.accounts = vec![(A9, a9)];
    let mut s = Sim::with(&setup, &rd_binary());
    s.ex.set_venue_model(vm);
    s.topn(ms(100), UP_B5, &[("0.47", "50")], &[("0.45", "50")]);
    s
}

/// A resting buy of 100 Up @ 0.45 behind the 50 shown.
fn resting(fill: MakerFill, vm: VenueModel) -> (Sim, hft_types::SlotId) {
    let mut s = binary(fill, vm);
    let o = s.place(A9, UP_B5, Buy, "100", "0.45", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    (s, o)
}

#[test]
fn sim_232_queue_cap() {
    let d = VenueModel::default;
    // .1 the level shrinks to 20: the queue ahead is capped at 20; a 30-share sell at the price
    // consumes it and fills 10.
    let (mut s, o) = resting(MakerFill::QueueCap, d());
    s.topn(ms(150), UP_B5, &[("0.47", "50")], &[("0.45", "20")]);
    s.trade_by(ms(200), UP_B5, "0.45", "30", Side::Sell);
    s.run_to(ms(210));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.45"), q("10"))], "SIM-232.1");
    // .2 a later increase of the level does not raise the queue ahead again.
    let (mut s, o) = resting(MakerFill::QueueCap, d());
    s.topn(ms(150), UP_B5, &[("0.47", "50")], &[("0.45", "20")]);
    s.topn(ms(160), UP_B5, &[("0.47", "50")], &[("0.45", "80")]);
    s.trade_by(ms(200), UP_B5, "0.45", "25", Side::Sell);
    s.run_to(ms(210));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.45"), q("5"))], "SIM-232.2");
    // .3 one share traded through the price fills the whole remainder.
    let (mut s, o) = resting(MakerFill::QueueCap, d());
    s.trade_by(ms(200), UP_B5, "0.44", "1", Side::Sell);
    s.run_to(ms(210));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.45"), q("100"))], "SIM-232.3");
    // .4 the ask crossing below the price fills the whole remainder.
    let (mut s, o) = resting(MakerFill::QueueCap, d());
    s.topn(ms(200), UP_B5, &[("0.44", "1")], &[("0.43", "5")]);
    s.run_to(ms(210));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.45"), q("100"))], "SIM-232.4");
    // .5 `Queue` keeps its rules: the through trade fills only its own quantity.
    let (mut s, o) = resting(MakerFill::Queue, d());
    s.trade_by(ms(200), UP_B5, "0.44", "1", Side::Sell);
    s.run_to(ms(210));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.45"), q("1"))], "SIM-232.5");
}

fn race() -> VenueModel {
    VenueModel {
        tombstone: true,
        cancel_us: Some((500, 1_000)),
        ..VenueModel::default()
    }
}

#[test]
fn sim_233_cancel_race_and_tombstone() {
    // .1 the cancel (0.5 ms) reaches the venue before the order (2 ms): the order is refused on
    // arrival with CancelAck only; it never rests.
    let mut s = binary(MakerFill::QueueCap, race());
    let o = s.place(A9, UP_B5, Buy, "10", "0.45", Tif::Gtc, ms(100));
    s.op(
        A9,
        hft_event::GwOp::Cancel,
        UP_B5,
        o,
        q("10"),
        px("0.45"),
        ms(100) + 500_000,
    );
    s.run_to(ms(200));
    assert_eq!(s.whats(A9, o), vec![OrderEventKind::CancelAck], "SIM-233.1");
    assert_eq!(s.ex.counters().tombstone_refusals, 1, "SIM-233.1");
    assert_eq!(s.view(A9).resting_count(), 0, "SIM-233.1");
    // .2 a cancel arriving after the order: Ack, then CancelAck 1 ms after the cancel arrives.
    let mut s = binary(MakerFill::QueueCap, race());
    let o = s.place(A9, UP_B5, Buy, "10", "0.45", Tif::Gtc, ms(100));
    s.op(
        A9,
        hft_event::GwOp::Cancel,
        UP_B5,
        o,
        q("10"),
        px("0.45"),
        ms(103),
    );
    s.run_to(ms(200));
    assert_eq!(
        s.whats(A9, o),
        vec![OrderEventKind::Ack, OrderEventKind::CancelAck],
        "SIM-233.2"
    );
    let due = s
        .order_events(A9)
        .into_iter()
        .find(|(_, e)| e.what == OrderEventKind::CancelAck)
        .map(|(d, _)| d);
    assert_eq!(due, Some(ms(103) + 500_000 + 1_000_000), "SIM-233.2");
    // .3 fills still happen while the cancel is in flight.
    let mut s = binary(MakerFill::QueueCap, race());
    let o = s.place(A9, UP_B5, Buy, "10", "0.45", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    s.op(
        A9,
        hft_event::GwOp::Cancel,
        UP_B5,
        o,
        q("10"),
        px("0.45"),
        ms(120),
    );
    s.trade_by(ms(120) + 100_000, UP_B5, "0.44", "1", Side::Sell);
    s.run_to(ms(200));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.45"), q("10"))], "SIM-233.3");
    // .4 without `tombstone` the early cancel is rejected (GONE) and the order rests.
    let mut s = binary(
        MakerFill::QueueCap,
        VenueModel {
            cancel_us: Some((500, 1_000)),
            ..VenueModel::default()
        },
    );
    let o = s.place(A9, UP_B5, Buy, "10", "0.45", Tif::Gtc, ms(100));
    s.op(
        A9,
        hft_event::GwOp::Cancel,
        UP_B5,
        o,
        q("10"),
        px("0.45"),
        ms(100) + 500_000,
    );
    s.run_to(ms(200));
    assert_eq!(
        s.whats(A9, o),
        vec![OrderEventKind::CancelReject, OrderEventKind::Ack],
        "SIM-233.4"
    );
}

fn delayed(slow_ppm: u32) -> VenueModel {
    VenueModel {
        taker: Some(TakerDelay {
            base_us: 195_000,
            slow_us: 1_250_000,
            slow_ppm,
            resp_us: 15_000,
        }),
        ..VenueModel::default()
    }
}

#[test]
fn sim_234_taker_delay() {
    // .1 an IOC buy arriving at 102 ms is matched 195 ms later against the book of that time
    // (the 0.47 level is gone, 10 @ 0.49 left): fills 10 @ 0.49, the rest expires; reported 15 ms
    // after the match.
    let mut s = binary(MakerFill::QueueCap, delayed(0));
    let o = s.place(A9, UP_B5, Buy, "30", "0.50", Tif::Ioc, ms(100));
    s.topn(ms(250), UP_B5, &[("0.49", "10")], &[("0.45", "50")]);
    s.run_to(ms(400));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.49"), q("10"))], "SIM-234.1");
    assert_eq!(
        s.whats(A9, o),
        vec![
            OrderEventKind::Ack,
            OrderEventKind::PartialFill,
            OrderEventKind::Expired
        ],
        "SIM-234.1"
    );
    let first = s.order_events(A9).first().map(|x| x.0);
    assert_eq!(first, Some(ms(102) + ms(195) + ms(15)), "SIM-234.1");
    // .2 without the option the same order fills at arrival against 0.47.
    let mut s = binary(MakerFill::QueueCap, VenueModel::default());
    let o = s.place(A9, UP_B5, Buy, "30", "0.50", Tif::Ioc, ms(100));
    s.topn(ms(250), UP_B5, &[("0.49", "10")], &[("0.45", "50")]);
    s.run_to(ms(400));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.47"), q("30"))], "SIM-234.2");
    // .3 the slow tail (probability 1): matched 1.25 s after arrival.
    let mut s = binary(MakerFill::QueueCap, delayed(1_000_000));
    let o = s.place(A9, UP_B5, Buy, "30", "0.50", Tif::Ioc, ms(100));
    s.run_to(ms(2_000));
    let first = s
        .order_events(A9)
        .into_iter()
        .find(|(_, e)| e.slot == o)
        .map(|x| x.0);
    assert_eq!(first, Some(ms(102) + ms(1_250) + ms(15)), "SIM-234.3");
    // .4 a post-only order is not delayed.
    let mut s = binary(MakerFill::QueueCap, delayed(0));
    let o = s.place(A9, UP_B5, Buy, "10", "0.44", Tif::PostOnly, ms(100));
    s.run_to(ms(110));
    assert_eq!(s.whats(A9, o), vec![OrderEventKind::Ack], "SIM-234.4");
}

#[test]
fn sim_235_binary_taker_fee() {
    let vm = VenueModel {
        fee: FeeModel::BinaryTaker { k_e9: 70_000_000 },
        ..VenueModel::default()
    };
    // .1 a taker buy of 10 @ 0.47: fee = 0.07 × 0.47 × 0.53 × 10 = 0.17437 USDT.
    let mut s = binary(MakerFill::QueueCap, vm.clone());
    let o = s.place(A9, UP_B5, Buy, "10", "0.50", Tif::Ioc, ms(100));
    s.run_to(ms(200));
    let fee: Vec<_> = s
        .events_of(A9, o)
        .into_iter()
        .filter(|e| e.what == OrderEventKind::Fill)
        .map(|e| e.fee)
        .collect();
    assert_eq!(fee, vec![usdt("0.17437")], "SIM-235.1");
    // .2 a maker fill pays nothing.
    let (mut s, o) = resting(MakerFill::QueueCap, vm);
    s.trade_by(ms(200), UP_B5, "0.44", "1", Side::Sell);
    s.run_to(ms(210));
    let fee: Vec<_> = s
        .events_of(A9, o)
        .into_iter()
        .filter(|e| e.what == OrderEventKind::Fill)
        .map(|e| e.fee)
        .collect();
    assert_eq!(fee, vec![usdt("0")], "SIM-235.2");
}

#[test]
fn sim_236_gtd_expiry() {
    let vm = VenueModel {
        gtd_ms: 300_000,
        ..VenueModel::default()
    };
    // .1 a resting order expires 300 s after it reached the venue.
    let (mut s, o) = resting(MakerFill::QueueCap, vm);
    s.run_to(ms(102) + ms(300_000) - 1);
    assert_eq!(s.whats(A9, o), vec![OrderEventKind::Ack], "SIM-236.1");
    s.run_to(ms(102) + ms(300_000) + ms(10));
    assert_eq!(
        s.whats(A9, o),
        vec![OrderEventKind::Ack, OrderEventKind::Expired],
        "SIM-236.1"
    );
    assert_eq!(s.ex.counters().gtd_expired, 1, "SIM-236.1");
    // .2 an order filled first does not expire.
    let vm = VenueModel {
        gtd_ms: 300_000,
        ..VenueModel::default()
    };
    let (mut s, o) = resting(MakerFill::QueueCap, vm);
    s.trade_by(ms(200), UP_B5, "0.44", "1", Side::Sell);
    s.run_to(ms(400_000));
    assert_eq!(s.ex.counters().gtd_expired, 0, "SIM-236.2");
    assert!(!s.whats(A9, o).contains(&OrderEventKind::Expired), "SIM-236.2");
}

fn pool(leg: PoolLeg, load: Option<u8>, mv: Option<u8>, phase: Option<u8>, us: u32) -> Pool {
    Pool {
        leg,
        load,
        mv,
        phase,
        us: vec![us; 20],
    }
}

#[test]
fn sim_237_conditional_pools() {
    let vm = VenueModel {
        pools: Some(LatPools {
            split_us: 5_000,
            min_n: 20,
            pools: vec![
                pool(PoolLeg::Place, Some(1), Some(0), Some(0), 10_000),
                pool(PoolLeg::Place, Some(2), None, None, 30_000),
                Pool {
                    us: vec![99_000; 19],
                    ..pool(PoolLeg::Place, Some(0), Some(0), Some(0), 0)
                },
            ],
        }),
        ..VenueModel::default()
    };
    let ack = |s: &Sim, o| {
        s.order_events(A9)
            .into_iter()
            .find(|(_, e)| e.slot == o)
            .map(|x| x.0)
    };
    // .1 the exact condition: 10 ms − 5 ms split to the venue, then the 2 ms response.
    let mut s = binary(MakerFill::QueueCap, vm.clone());
    s.ex.set_condition(Cond {
        load: 1,
        mv: 0,
        phase: 0,
    });
    let o = s.place(A9, UP_B5, Buy, "10", "0.44", Tif::PostOnly, ms(100));
    s.run_to(ms(200));
    assert_eq!(ack(&s, o), Some(ms(105) + ms(2)), "SIM-237.1");
    // .2 no exact pool: the (leg, load) pool.
    let mut s = binary(MakerFill::QueueCap, vm.clone());
    s.ex.set_condition(Cond {
        load: 2,
        mv: 1,
        phase: 1,
    });
    let o = s.place(A9, UP_B5, Buy, "10", "0.44", Tif::PostOnly, ms(100));
    s.run_to(ms(200));
    assert_eq!(ack(&s, o), Some(ms(125) + ms(2)), "SIM-237.2");
    // .3 a pool with fewer than 20 samples and no fallback: the account model (2 ms entry).
    let mut s = binary(MakerFill::QueueCap, vm);
    s.ex.set_condition(Cond {
        load: 0,
        mv: 0,
        phase: 0,
    });
    let o = s.place(A9, UP_B5, Buy, "10", "0.44", Tif::PostOnly, ms(100));
    s.run_to(ms(200));
    assert_eq!(ack(&s, o), Some(ms(102) + ms(2)), "SIM-237.3");
}

#[test]
fn sim_238_own_order_overlay() {
    // .1 30 of the 50 shown at the bid are another run's own order: 20 ahead; a 30-share sell
    // fills 10.
    let mut s = binary(MakerFill::QueueCap, VenueModel::default());
    s.ex.hide_own(UP_B5, Buy, px("0.45"), q("30"));
    let o = s.place(A9, UP_B5, Buy, "100", "0.45", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    s.trade_by(ms(200), UP_B5, "0.45", "30", Side::Sell);
    s.run_to(ms(210));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.45"), q("10"))], "SIM-238.1");
    // .2 hidden ask volume is not taken: 40 of the 50 at 0.47 hidden, a 30 IOC buy fills 10.
    let mut s = binary(MakerFill::QueueCap, VenueModel::default());
    s.ex.hide_own(UP_B5, Sell, px("0.47"), q("40"));
    let o = s.place(A9, UP_B5, Buy, "30", "0.47", Tif::Ioc, ms(100));
    s.run_to(ms(200));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.47"), q("10"))], "SIM-238.2");
    // .3 removing the hidden quantity restores the level.
    let mut s = binary(MakerFill::QueueCap, VenueModel::default());
    s.ex.hide_own(UP_B5, Sell, px("0.47"), q("40"));
    s.ex.hide_own(UP_B5, Sell, px("0.47"), hft_types::Qty(-q("40").0));
    let o = s.place(A9, UP_B5, Buy, "30", "0.47", Tif::Ioc, ms(100));
    s.run_to(ms(200));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.47"), q("30"))], "SIM-238.3");
}

#[test]
fn sim_239_default_model_is_the_plain_exchange() {
    // The same script with and without `set_venue_model(VenueModel::default())`: same replies and
    // state hash.
    let run = |set: bool| {
        let mut s = binary(MakerFill::Queue, VenueModel::default());
        if set {
            s.ex.set_venue_model(VenueModel::default());
        }
        let o = s.place(A9, UP_B5, Buy, "100", "0.45", Tif::Gtc, ms(100));
        s.op(
            A9,
            hft_event::GwOp::Cancel,
            UP_B5,
            o,
            q("100"),
            px("0.45"),
            ms(101),
        );
        let p = s.place(A9, UP_B5, Buy, "10", "0.50", Tif::Ioc, ms(120));
        s.trade_by(ms(150), UP_B5, "0.44", "3", Side::Sell);
        s.run_to(ms(400));
        let reps: Vec<String> = s
            .out
            .iter()
            .map(|r| match &r.msg {
                RepMsg::OrderEvent(e, _) => format!("{} {:?} {:?} {}", r.due, e.what, e.slot, e.qty.0),
                m => format!("{} {m:?}", r.due),
            })
            .collect();
        let _ = p;
        (reps, s.ex.state_hash())
    };
    assert_eq!(run(false), run(true), "SIM-239.1");
}
