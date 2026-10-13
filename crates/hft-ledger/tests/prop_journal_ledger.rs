//! LED-266: at every snapshot point the journal cold consumer equals the engine ledger restored
//! from that snapshot (positions, inventories, balances). The test plays the sequencer: each
//! input becomes journal records plus the engine's ledger calls.

use crate::common;

use common::journal::*;
use common::opgen::cases;
use common::*;
use hft_event::{Body, EngineClock, ExternalCause, OrderEventKind, RecordFlags, decode_record, encode_fixed};
use hft_ledger::notes::code;
use hft_ledger::{AdjustDelta, AdjustKind, FeeDelta, FillClass, FillDelta, JournalLedger, Ledger};
use hft_strategy_sdk::SnapshotReader;
use hft_types::{ASSET_NONE, AssetId, InstId, Notional, Px, Qty, Side};
use proptest::prelude::*;
use zerocopy::FromZeros;

const T0: i64 = 1_000_000_000;
const EPOCH: u16 = 7;

#[derive(Clone, Debug)]
enum In {
    New {
        perp: bool,
        buy: bool,
        lots: i64,
    },
    /// Fill `pct`% of the remaining quantity of order `o` at `px`.
    Fill {
        o: usize,
        pct: i64,
        px: i64,
    },
    /// Re-send the last report of order `o`, or an older cumulative quantity.
    Replay {
        o: usize,
        older: bool,
    },
    FeeOnly {
        o: usize,
        base: bool,
    },
    External {
        buy: bool,
        perp: bool,
        lots: i64,
        tid: u64,
    },
    Funding {
        k: i64,
        amount: i64,
    },
    Adopt {
        lots: i64,
        px: i64,
    },
    StaleGen {
        buy: bool,
        lots: i64,
        eoid: u64,
    },
}

fn input() -> impl Strategy<Value = In> {
    let px = (5_900i64..=6_100).prop_map(|k| k * 100_000);
    prop_oneof![
        3 => (any::<bool>(), any::<bool>(), 1i64..=100).prop_map(|(perp, buy, lots)| In::New { perp, buy, lots }),
        5 => (0usize..16, 1i64..=100, px.clone()).prop_map(|(o, pct, px)| In::Fill { o, pct, px }),
        2 => (0usize..16, any::<bool>()).prop_map(|(o, older)| In::Replay { o, older }),
        1 => (0usize..16, any::<bool>()).prop_map(|(o, base)| In::FeeOnly { o, base }),
        1 => (any::<bool>(), any::<bool>(), 1i64..=50, 1u64..=10).prop_map(|(buy, perp, lots, tid)| In::External { buy, perp, lots, tid }),
        1 => (1i64..=10, -5_000_000i64..=5_000_000).prop_map(|(k, amount)| In::Funding { k, amount }),
        1 => (-50i64..=50, px.clone()).prop_map(|(lots, px)| In::Adopt { lots, px }),
        1 => (any::<bool>(), 1i64..=50, 1u64..=1_000).prop_map(|(buy, lots, eoid)| In::StaleGen { buy, lots, eoid }),
    ]
}

struct Order {
    slot: u32,
    inst: InstId,
    side: Side,
    qty: i64,
    cum: i64,
    notional: i128,
    last: Option<hft_event::OrderEvent>,
    prev_cum: i64,
}

/// The test's sequencer: engine ledger + journal bytes + snapshot points.
struct Seq {
    e: Ledger,
    records: Vec<Vec<u8>>,
    /// (number of records before the snapshot, snapshot bytes).
    snaps: Vec<(usize, Vec<u8>)>,
    orders: Vec<Order>,
    t: i64,
}

impl Seq {
    fn rec<B: Body>(&mut self, b: &B, end: bool) {
        let mut buf = vec![0u8; 1024];
        let flags = if end {
            RecordFlags::EVENT_END
        } else {
            RecordFlags(0)
        };
        let n = encode_fixed(
            &mut buf,
            self.records.len() as u64,
            (self.t - T0) as u64,
            flags,
            b,
        ) as usize;
        buf.truncate(n);
        self.records.push(buf);
    }

    fn input(&mut self, x: &In) {
        self.t += s(1);
        let t = self.t;
        match *x {
            In::New { perp, buy, lots } => {
                let (inst, venue) = if perp { (BTC_P2, P2) } else { (BTC_S1, S1) };
                let side = if buy { Buy } else { Sell };
                let qty = lots * if perp { 100_000 } else { 1_000 };
                let slot = 1 + self.orders.len() as u32;
                self.rec(&decision_new(slot, 1, A1, venue, inst, side, G1, Qty(qty)), true);
                self.orders.push(Order {
                    slot,
                    inst,
                    side,
                    qty,
                    cum: 0,
                    notional: 0,
                    last: None,
                    prev_cum: 0,
                });
            }
            In::Fill { o, pct, px } => {
                let Some(ord) = self.orders.get(o) else { return };
                let rest = ord.qty - ord.cum;
                let inc = (rest * pct / 100).max(if rest > 0 { 1 } else { 0 });
                if inc == 0 {
                    return;
                }
                let cum = ord.cum + inc;
                let total = ord.notional + i128::from(px) * i128::from(inc);
                // The exchange reports a truncated average price.
                let avg = Px((total / i128::from(cum)) as i64);
                // Engine: the OMS fill delta (incremental notional over the incremental quantity).
                let total_rep = i128::from(avg.0) * i128::from(cum);
                let inc_notional = total_rep - ord.notional;
                let dpx = Px(half_even(inc_notional, i128::from(inc)) as i64);
                let what = if cum == ord.qty {
                    OrderEventKind::Fill
                } else {
                    OrderEventKind::PartialFill
                };
                let (inst, side, slot) = (ord.inst, ord.side, ord.slot);
                let ev = order_event(what, EPOCH, slot, 1, A1, venue_of(inst), inst, Qty(cum), avg);
                self.rec(&ev, true);
                let leg = if inst == BTC_P2 { 1 } else { 0 };
                let fd = FillDelta {
                    account: A1,
                    venue: venue_of(inst),
                    side,
                    class: FillClass::Open as u8,
                    flags: 0,
                    leg_ix: leg,
                    _pad: [0; 2],
                    inst,
                    group: G1,
                    qty: Qty(inc),
                    net_qty: Qty(inc),
                    px: dpx,
                    fee: 0,
                    fee_asset: ASSET_NONE,
                    _pad2: 0,
                    slot,
                    r#gen: 1,
                    _pad3: 0,
                    seq: 0,
                };
                if inst == BTC_S1 {
                    self.e
                        .apply_spot_fill(&fd, inc_notional, t)
                        .expect("spot increment");
                } else {
                    self.e.apply_fill(&fd, t);
                }
                let ord = &mut self.orders[o];
                ord.prev_cum = ord.cum;
                ord.cum = cum;
                ord.notional = total_rep;
                ord.last = Some(ev);
            }
            In::Replay { o, older } => {
                let Some(ord) = self.orders.get(o) else { return };
                let Some(mut ev) = ord.last else { return };
                if older {
                    ev.cum_filled = Qty(ord.prev_cum);
                    ev.what = OrderEventKind::PartialFill;
                }
                self.rec(&ev, true);
            }
            In::FeeOnly { o, base } => {
                let Some(ord) = self.orders.get(o) else { return };
                if ord.cum == 0 {
                    return;
                }
                let (fee, asset) = if base && ord.inst == BTC_S1 {
                    (1i64, BTC)
                } else {
                    (1_000i64, USDT)
                };
                let mut ev = order_event(
                    OrderEventKind::FeeOnly,
                    EPOCH,
                    ord.slot,
                    1,
                    A1,
                    venue_of(ord.inst),
                    ord.inst,
                    Qty(ord.cum),
                    Px(0),
                );
                ev.fee = Qty(fee);
                ev.fee_asset = asset;
                let fd = FeeDelta {
                    account: A1,
                    venue: venue_of(ord.inst),
                    inst: ord.inst,
                    group: G1,
                    side: ord.side,
                    fee,
                    fee_asset: asset,
                    seq: 0,
                };
                self.rec(&ev, true);
                self.e.apply_fee(&fd, t);
            }
            In::External { buy, perp, lots, tid } => {
                let inst = if perp { BTC_P2 } else { BTC_S1 };
                let mut x = ext(
                    ExternalCause::Manual,
                    if buy { Buy } else { Sell },
                    inst,
                    "0",
                    "60000",
                );
                x.qty = Qty(lots * if perp { 100_000 } else { 1_000 });
                x.trade_id = tid;
                self.rec(&x, true);
                self.e.apply_external(&x, false, t);
            }
            In::Funding { k, amount } => {
                let mut x = push(BTC_P2, "0", D + k * 8 * H);
                x.amount = Notional(amount);
                self.rec(&x, true);
                self.e.apply_funding(&x, t);
            }
            In::Adopt { lots, px } => {
                if lots == 0 {
                    return;
                }
                let a = AdjustDelta {
                    account: A1,
                    venue: P2,
                    kind: AdjustKind::Position as u8,
                    px_src: 1,
                    inst: BTC_P2,
                    asset: AssetId(0),
                    _pad: 0,
                    group: G1,
                    qty: Qty(lots * 100_000),
                    px: Px(px),
                    report_id: 5,
                    cmd_seq: 9,
                    settle_ts_ns: 0,
                };
                self.e.clear_notes();
                self.e.apply_adjust(&a, t);
                let n = notes_of(&self.e, code::ADJUSTED)[0];
                self.rec(&n, true);
            }
            In::StaleGen { buy, lots, eoid } => {
                // A report with a stale generation: the OMS writes OmsForeignFill and the sequencer
                // books it as an Unknown external fill.
                let Some(ord) = self.orders.first() else { return };
                let qty = lots * 1_000;
                let mut ev = order_event(
                    OrderEventKind::Fill,
                    EPOCH,
                    ord.slot,
                    2,
                    A1,
                    S1,
                    BTC_S1,
                    Qty(qty),
                    px("60000"),
                );
                ev.exch_order_id = eoid;
                let side = if buy { Buy } else { Sell };
                let n = oms_foreign_fill(
                    S1,
                    A1,
                    BTC_S1,
                    side,
                    2,
                    ASSET_NONE,
                    Qty(qty),
                    px("60000"),
                    Qty(0),
                    eoid,
                    0,
                    Qty(qty),
                );
                self.rec(&ev, false);
                self.rec(&n, true);
                let x = Ledger::external_from_oms_note(&n).expect("note");
                self.e.apply_external(&x, false, t);
            }
        }
        self.e.clear_notes();
    }
}

fn half_even(a: i128, b: i128) -> i128 {
    let (q, r) = (a.div_euclid(b), a.rem_euclid(b));
    match (2 * r).cmp(&b) {
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Equal => q + (q & 1),
        std::cmp::Ordering::Less => q,
    }
}

fn readings(
    l: &Ledger,
) -> (
    Vec<hft_ledger::InstPos>,
    Vec<hft_ledger::CashEntry>,
    Vec<hft_ledger::InvEntry>,
) {
    // An all-zero position is not a snapshot row (LED-232), so its last fill time is not state.
    let pos = [BTC_S1, BTC_P2]
        .iter()
        .map(|i| {
            let p = *l.pos(A1, *i);
            let empty = p.qty.0 == 0 && p.cost == 0 && p.realized_quote == 0 && p.fees_quote == 0;
            hft_ledger::InstPos {
                last_fill_ens: if empty { 0 } else { p.last_fill_ens },
                ..p
            }
        })
        .collect();
    let mut cash = Vec::new();
    let mut inv = Vec::new();
    for v in [S1, P2] {
        let b = l.venue_book(A1, v);
        cash.extend(b.cash.iter().map(|e| hft_ledger::CashEntry { rep_ens: 0, ..*e }));
        inv.extend(b.inv.iter().copied());
    }
    (pos, cash, inv)
}

fn started() -> (Seq, Vec<u8>) {
    let mut e = booted();
    e.on_run_start(T0);
    let mut q = Seq {
        e,
        records: Vec::new(),
        snaps: Vec::new(),
        orders: Vec::new(),
        t: T0,
    };
    let first = snap(&q.e);
    let mut c = EngineClock::new_zeroed();
    c.base_engine_ns = T0;
    c.tsc_hz_milli = CLOCK_HZ_MILLI;
    q.rec(&c, true);
    q.rec(&epoch_start(u64::from(EPOCH)), true);
    (q, first)
}

#[test]
fn led_266_fractional_report_notional_survives_the_hot_driver_and_cold_snapshot() {
    for buy in [true, false] {
        let (mut q, first) = started();
        let mut r = SnapshotReader::new(&first);
        let mut cold = JournalLedger::from_snapshot(&mut r, rd_lf(), &cfg_lf(), &caps_lf()).unwrap();
        q.input(&In::New {
            perp: false,
            buy,
            lots: 1,
        });
        let mut fed = 0;
        // The CI counterexample's reported cumulative amount is 404373.999960 quote
        // units. First cash is exactly 35784; second cash rounds to 368590/368589.
        for (pct, price, cumulative_cash) in [
            (6, 596_400_000, 35_784),
            (66, 594_500_000, if buy { 404_374 } else { 404_373 }),
        ] {
            q.input(&In::Fill { o: 0, pct, px: price });
            while fed < q.records.len() {
                cold.apply(&decode_record(&q.records[fed]).unwrap()).unwrap();
                fed += 1;
            }
            let expected = 100_000_000_000 + if buy { -cumulative_cash } else { cumulative_cash };
            assert_eq!(cash(&q.e, A1, S1, USDT).total, Qty(expected));
            assert_eq!(cash(cold.ledger(), A1, S1, USDT).total, Qty(expected));
            assert_eq!(readings(cold.ledger()), readings(&restore(&snap(&q.e)).unwrap()));
        }
        let last = q.orders[0].last.unwrap();
        assert_eq!((last.cum_filled, last.avg_px), (Qty(680), Px(594_667_647)));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases(500)))]

    #[test]
    fn led_266_cold_equals_engine_at_snapshots(inputs in prop::collection::vec(input(), 1..=200)) {
        let (mut q, first) = started();
        for (k, x) in inputs.iter().enumerate() {
            q.input(x);
            if (k + 1) % 20 == 0 || k + 1 == inputs.len() {
                let b = snap(&q.e);
                q.snaps.push((q.records.len(), b));
            }
        }
        let mut r = SnapshotReader::new(&first);
        let mut j = JournalLedger::from_snapshot(&mut r, rd_lf(), &cfg_lf(), &caps_lf()).expect("from_snapshot");
        let mut fed = 0;
        for (n, bytes) in &q.snaps {
            while fed < *n {
                let rv = decode_record(&q.records[fed]).expect("decode");
                j.apply(&rv).expect("valid journal");
                fed += 1;
            }
            let restored = restore(bytes).expect("restore");
            prop_assert_eq!(readings(j.ledger()), readings(&restored), "at record {}", n);
        }
    }
}
