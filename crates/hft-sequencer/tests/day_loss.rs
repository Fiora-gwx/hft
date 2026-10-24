//! SEQ-440–SEQ-442: a Delivery booked at 0 crosses the daily loss (DailyLoss cap, opens
//! rejected); the ECF1 day-loss carry rows; the carry booked at the day roll of a fresh journal,
//! not again after a restart from a snapshot.

use hft_event::{ExternalCause, NoteSource};
use hft_refdata::{EcfBuilder, EngineConfig, EngineSection};
use hft_sequencer::driver::live::RingId;
use hft_sequencer::{EngineSetup, SnapshotSel};
use hft_strategy_sdk::IntentClass;
use hft_types::{AccountId, CapSource, HostClass, RejectBits, Side};

use crate::common::risk_kit::*;
use crate::common::*;

/// UTC day of the B0 clock.
const DAY0: u32 = (UTC0 / 86_400_000_000_000) as u32;

/// Account 1: daily loss 50 USDT, quota not limited by equity (`daily_loss_kill` 1.0).
fn loss_50(b: &mut B0) {
    let rd = b.rd.load();
    let mut p = b.risk.clone().unwrap_or_else(|| b0_risk(&rd));
    for x in p.accounts.as_mut_slice() {
        if x.account == AC1 {
            x.daily_loss_abs = n("50");
            x.daily_loss_kill = hft_types::Rate(1_000_000_000);
        }
    }
    b.risk = Some(p);
}

fn day_adjust(h: &H) -> i64 {
    h.s.engine().ledger.account(AC1).day.adjust.0
}

fn open_reject(h: &mut H) -> RejectBits {
    let d = place_in(h, S1, G1, A1, Side::Buy, q("0.001"), IntentClass::Open);
    RejectBits(d.reject)
}

#[test]
fn seq_440_delivery_at_zero_crosses_daily_loss() {
    reset_script();
    let mut b = b0();
    loss_50(&mut b);
    let mut h = b.boot();
    h.steady();
    // .1 0.002 BTC bought at the touch (≈ 100 USDT): no cap.
    h.open_position(AC1, A1, Side::Buy, "0.002");
    assert!(!cap_on(&h, CapSource::DAILY_LOSS, AC1, None), "SEQ-440.1");
    // .2 the venue delivers the inventory at 0 (ExternalFill cause Delivery, SELL px 0).
    let m = h.mark();
    let mut x = external_fill(&h, AC1, A1, Side::Sell, "0.002", ExternalCause::Delivery);
    x.px = hft_types::Px(0);
    h.push(RingId::Rep(0), &x);
    h.run();
    let e = h.s.engine();
    assert_eq!(e.ledger.pos(AC1, A1).qty.0, 0, "SEQ-440.2");
    let day = e.ledger.account(AC1).day;
    assert!(day.external.0 <= -100_000_000, "SEQ-440.2 {day:?}");
    assert!(day.net().0 < -50_000_000, "SEQ-440.2 {day:?}");
    let set: Vec<_> = cap_notes(&h, m, true)
        .into_iter()
        .filter(|c| c.source == CapSource::DAILY_LOSS && c.scope.account == AC1)
        .collect();
    assert_eq!(set.len(), 1, "SEQ-440.2 CapSet{{DailyLoss}}");
    assert_eq!(set[0].params[0] as i64, -50_000_000, "SEQ-440.2 trigger");
    // .3 an Open is rejected DAILY_LOSS.
    assert_eq!(open_reject(&mut h), RejectBits::DAILY_LOSS, "SEQ-440.3");
}

fn with_carry(day: u32, pnl: &'static str) -> impl Fn(&mut EngineSetup) {
    move |s: &mut EngineSetup| s.day_pnl_carry = vec![(AC1, day, n(pnl))]
}

#[test]
fn seq_441_day_loss_carry_rows() {
    let rd = b0_rd().load();
    let mut s = b0_setup(&rd);
    // .1 round trip.
    with_carry(DAY0, "-28.51")(&mut s);
    s.day_pnl_carry.push((AC2, DAY0, n("-1")));
    let b = s.encode_blob(1, HostClass::Dev2, UTC0);
    let q = EngineSetup::decode_blob(&b, &rd, s.clock, s.oms.engine_epoch, s.base_engine_ns).unwrap();
    assert_eq!(q.day_pnl_carry, s.day_pnl_carry, "SEQ-441.1");
    assert_eq!(b, q.encode_blob(1, HostClass::Dev2, UTC0), "SEQ-441.1");
    // .2 a profit, zero, day 0, an unknown account: refused.
    for (i, (a, day, pnl)) in [
        (AC1, DAY0, "5"),
        (AC1, DAY0, "0"),
        (AC1, 0, "-1"),
        (AccountId(200), DAY0, "-1"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut x = s.clone();
        x.day_pnl_carry = vec![(a, day, n(pnl))];
        let b = x.encode_blob(1, HostClass::Dev2, UTC0);
        assert!(
            EngineSetup::decode_blob(&b, &rd, x.clock, 1, 0).is_err(),
            "SEQ-441.2 row {i}"
        );
    }
    // .3 a day row without its pnl row (and the reverse): refused.
    let ec = EngineConfig::load(&s.encode_blob(1, HostClass::Dev2, UTC0), None).unwrap();
    for drop_field in [7u16, 8] {
        let mut b = EcfBuilder::new(1, HostClass::Dev2);
        for tag in EngineSection::ALL {
            let Some(v) = ec.section(tag) else { continue };
            let mut bytes = v.bytes.to_vec();
            let mut count = v.count;
            if tag == EngineSection::Assembly {
                bytes = v
                    .bytes
                    .chunks_exact(16)
                    .filter(|r| !(u16::from_le_bytes([r[0], r[1]]) == drop_field && r[4] == AC2.0))
                    .flatten()
                    .copied()
                    .collect();
                count = (bytes.len() / 16) as u32;
            }
            b.section(tag, v.row_ver, v.row_size, count, bytes);
        }
        assert!(
            EngineSetup::decode_blob(&b.finish(), &rd, s.clock, 1, 0).is_err(),
            "SEQ-441.3 without field {drop_field}"
        );
    }
}

fn carry_60(s: &mut EngineSetup) {
    s.day_pnl_carry = vec![(AC1, DAY0, n("-60"))];
}
fn carry_20(s: &mut EngineSetup) {
    s.day_pnl_carry = vec![(AC1, DAY0, n("-20"))];
}
fn carry_tomorrow(s: &mut EngineSetup) {
    s.day_pnl_carry = vec![(AC1, DAY0 + 1, n("-60"))];
}

/// B0 with [`loss_50`] and the carry `edit`, booted with the complete ECF1 in the journal.
fn boot_carry(edit: fn(&mut EngineSetup)) -> H {
    reset_script();
    let mut b = b0();
    loss_50(&mut b);
    b.setup_edit.push(edit);
    let mut h = b.boot_self_contained();
    h.steady();
    h
}

fn carried_notes(h: &H) -> Vec<[u64; 8]> {
    h.notes_since(0)
        .iter()
        .filter(|n| n.source == NoteSource::Ledger && n.code == hft_ledger::notes::code::DAY_PNL_CARRIED)
        .map(|n| n.v)
        .collect()
}

#[test]
fn seq_442_carry_booked_at_day_roll_once() {
    // .1 −60 carried into a 50 limit: day −60, DailyLoss at the day roll, opens rejected.
    let mut h = boot_carry(carry_60);
    assert_eq!(h.s.engine().last_day_rolled, DAY0, "SEQ-442.1");
    assert_eq!(day_adjust(&h), -60_000_000, "SEQ-442.1");
    assert_eq!(
        carried_notes(&h),
        vec![[1, (-60_000_000i64) as u64, 0, 0, 0, 0, 0, 0]],
        "SEQ-442.1"
    );
    assert!(cap_on(&h, CapSource::DAILY_LOSS, AC1, None), "SEQ-442.1");
    // .4 replay from the journal reproduces it (the carry is read from the journaled ECF1).
    assert!(h.verify().is_empty(), "SEQ-442.4 {:?}", h.verify());
    assert_eq!(open_reject(&mut h), RejectBits::DAILY_LOSS, "SEQ-442.1");
    // .5 a restart from the latest snapshot the same day: not booked again, cap still latched.
    h.keyframe();
    let mut h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
    h2.bring_up(false);
    assert_eq!(day_adjust(&h2), -60_000_000, "SEQ-442.5");
    assert!(cap_on(&h2, CapSource::DAILY_LOSS, AC1, None), "SEQ-442.5");
    // .2 −20 carried: day −20, no cap, opens accepted.
    let mut h = boot_carry(carry_20);
    assert_eq!(day_adjust(&h), -20_000_000, "SEQ-442.2");
    assert!(!cap_on(&h, CapSource::DAILY_LOSS, AC1, None), "SEQ-442.2");
    assert_eq!(open_reject(&mut h), RejectBits(0), "SEQ-442.2");
    // .3 a carry of another day: nothing booked.
    let h = boot_carry(carry_tomorrow);
    assert_eq!(day_adjust(&h), 0, "SEQ-442.3");
    assert!(carried_notes(&h).is_empty(), "SEQ-442.3");
    assert!(!cap_on(&h, CapSource::DAILY_LOSS, AC1, None), "SEQ-442.3");
}
