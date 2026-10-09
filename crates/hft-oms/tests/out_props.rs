#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{ConnRole, ConnStateKind};
use hft_oms::notes;
use hft_oms::*;
use hft_types::{AccountId, GROUP_ALL, GROUP_NONE, Side, StrategyId, Tif, VenueId};
use proptest::prelude::*;

const TABLES: [(VenueId, AccountId); 4] = [(VS, A1), (VX, A1), (VG, A1), (VS, A2)];

#[derive(Copy, Clone, Debug)]
enum Batch {
    CancelAll(u8, bool),
    CancelOpen,
    CancelResting(u8),
    Reissue(u8, bool),
    ConnDown(u8),
    ConnUp(u8),
    UserDown(u8),
    ForceUnknown(u8),
    GateOpen(u8),
    Sweep,
}

fn batch(nt: u8) -> impl Strategy<Value = Batch> {
    prop_oneof![
        (0..nt, any::<bool>()).prop_map(|(t, o)| Batch::CancelAll(t, o)),
        Just(Batch::CancelOpen),
        (0..nt).prop_map(Batch::CancelResting),
        (0..nt, any::<bool>()).prop_map(|(t, o)| Batch::Reissue(t, o)),
        (0..nt).prop_map(Batch::ConnDown),
        (0..nt).prop_map(Batch::ConnUp),
        (0..nt).prop_map(Batch::UserDown),
        (0..nt).prop_map(Batch::ForceUnknown),
        (0..nt).prop_map(Batch::GateOpen),
        Just(Batch::Sweep),
    ]
}

#[derive(Default, Debug, PartialEq)]
struct Streams {
    notes: Vec<hft_event::Note>,
    updates: Vec<hft_strategy_sdk::OrderUpdate>,
    timers: Vec<OmsTimerReq>,
    decisions: Vec<hft_event::OrderDecision>,
    hedges: Vec<HedgeRequest>,
    links: Vec<LinkUpdate>,
}

fn take(s: &mut Streams, o: &OmsOut) {
    let tc = notes::code_of(notes::TRANSITION);
    let uc = notes::code_of(notes::UNKNOWN);
    s.notes
        .extend(o.notes.iter().filter(|n| n.code == tc || n.code == uc).copied());
    s.updates.extend(o.updates.iter().copied());
    s.timers.extend(o.timers.iter().copied());
    s.decisions.extend(o.decisions.iter().copied());
    s.hedges.extend(o.hedges.iter().copied());
    s.links.extend(o.links.iter().copied());
}

fn inst(v: VenueId) -> hft_types::InstId {
    if v == VS {
        X
    } else if v == VX {
        Y
    } else {
        Z
    }
}

fn build(k: u16, quota: u16, sizes: &[u16], unbounded: bool) -> H {
    let mut c = p0();
    c.max_out_per_event = k;
    c.out_quota = PerTable::new(Some(quota));
    for (v, a) in TABLES {
        c.slots.set(v, a, 600);
    }
    let mut h = H::from_cfg(&c, rd());
    for (i, n) in sizes.iter().enumerate() {
        let (v, a) = TABLES[i];
        for j in 0..*n {
            let mut it = intent(
                v,
                a,
                inst(v),
                Side::Sell,
                if j % 5 == 0 { Tif::Ioc } else { Tif::Gtc },
                p("600.0"),
                q("0.001"),
                GROUP_NONE,
            );
            it.risk_reducing = j % 7 == 0;
            let r = h.place_ok(&it, 0);
            if j % 2 == 0 {
                h.ack(r, u64::from(j) + 1, 0);
            }
        }
    }
    while h.o.wants_continue() {
        h.cont();
    }
    if unbounded {
        h.out = OmsOut::unbounded();
    }
    h.out.clear();
    h
}

fn apply(h: &mut H, b: Batch, t: u64) {
    h.out.clear();
    let tab = |i: u8| TABLES[usize::from(i) % TABLES.len()];
    let now = ms(t);
    match b {
        Batch::CancelAll(i, oo) => {
            let (v, a) = tab(i);
            h.o.cancel_all(CancelScope::VenueAccount(v, a), oo, now, t, &mut h.out)
        }
        Batch::CancelOpen => h.o.cancel_open(
            CellKey {
                strategy: StrategyId(0),
                account: A1,
                group: GROUP_ALL,
            },
            now,
            t,
            &mut h.out,
        ),
        Batch::CancelResting(i) => {
            let (v, a) = tab(i);
            h.o.cancel_resting_on(a, inst(v), now, t, &mut h.out)
        }
        Batch::Reissue(i, oo) => {
            let (_, a) = tab(i);
            h.o.reissue_cancels(CancelScope::Account(a), oo, now, t, &mut h.out)
        }
        // Connection events are delivered by the test itself (they need a record header).
        Batch::ConnDown(_) | Batch::ConnUp(_) | Batch::UserDown(_) => {}
        Batch::ForceUnknown(i) => {
            let (v, a) = tab(i);
            h.o.force_unknown(v, a, now, t, &mut h.out);
        }
        Batch::GateOpen(i) => {
            let (v, a) = tab(i);
            h.o.on_hedge_gate_open(a, v, now, t, &mut h.out)
        }
        Batch::Sweep => h.o.on_sweep(now, &mut h.out),
    }
}

fn states(h: &H, sizes: &[u16]) -> Vec<(u8, u16)> {
    let mut v = Vec::new();
    for (i, n) in sizes.iter().enumerate() {
        let (ve, a) = TABLES[i];
        for s in 1..=u32::from(*n) {
            let (sl, _, _) = h.o.raw_slot(ve, a, s).unwrap();
            v.push((sl.state as u8, sl.sflags.0 & !SlotFlags::QUEUED));
        }
    }
    v
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn oms_388_fanout_matches_unbounded(
        k in 1u16..=32,
        quota in 1u16..=64,
        sizes in prop::collection::vec(0u16..=600, 2..=4),
        ops in prop::collection::vec((batch(4), 0u8..=3), 1..=6),
    ) {
        let mut a = build(k, quota, &sizes, false);
        let mut r = build(k, quota, &sizes, true);
        let (mut sa, mut sr) = (Streams::default(), Streams::default());
        for (n, (b, conts)) in ops.iter().enumerate() {
            let t = 10 + n as u64;
            // Connection events go through the harness (fresh vector); route them directly instead.
            match b {
                Batch::ConnDown(_) | Batch::ConnUp(_) | Batch::UserDown(_) => {
                    let i = match b { Batch::ConnDown(i) | Batch::ConnUp(i) | Batch::UserDown(i) => *i, _ => 0 };
                    let (v, acc) = TABLES[usize::from(i) % TABLES.len()];
                    let (role, st) = match b {
                        Batch::ConnDown(_) => (ConnRole::Order, ConnStateKind::Down),
                        Batch::ConnUp(_) => (ConnRole::Order, ConnStateKind::Up),
                        _ => (ConnRole::UserStream, ConnStateKind::Down),
                    };
                    for (h, s) in [(&mut a, &mut sa), (&mut r, &mut sr)] {
                        h.out.clear();
                        let hd = h.hdr(t);
                        let ev = hft_event::ConnState { venue: v, conn: 0, state: st, role, inst: hft_types::InstId(0), reason: 0, account: acc, _pad2: [0; 3], rx_tsc: 0 };
                        h.o.on_conn_state(&ev, &hd, &mut h.out);
                        take(s, &h.out);
                    }
                }
                _ => {
                    apply(&mut a, *b, t);
                    take(&mut sa, &a.out);
                    apply(&mut r, *b, t);
                    take(&mut sr, &r.out);
                }
            }
            prop_assert_eq!(states(&a, &sizes), states(&r, &sizes));
            for _ in 0..*conts {
                a.cont();
                take(&mut sa, &a.out);
                r.cont();
                take(&mut sr, &r.out);
            }
        }
        while a.o.wants_continue() || r.o.wants_continue() {
            a.cont();
            take(&mut sa, &a.out);
            r.cont();
            take(&mut sr, &r.out);
        }
        prop_assert_eq!(sa, sr);
    }
}
