//! TK-107: real per-Input observations, with counterexamples applied only to diagnostic copies.
use std::cell::RefCell;

use hft_event::{EventKind, OrderAction, OrderDecision, RecordFlags};
use hft_testkit::dst::invariants::{InvId, InvState, InvariantSet, StepView, check_step};
use hft_testkit::dst::obs::{DecClass, Obs};
use hft_testkit::dst::sut::{BootCtx, BootFail, CrashPoint, EventStep, InputRec, JRec, StepOut, Sut};
use hft_testkit::dst::{SeqSut, run_record};
use hft_testkit::hash::ComponentHashes;
use hft_testkit::strategies::TestStrategies;
use hft_types::{AccountId, Side};
use zerocopy::IntoBytes;

thread_local! {
    static SAMPLES: RefCell<Vec<EventStep>> = const { RefCell::new(Vec::new()) };
    static TIMERS: RefCell<u64> = const { RefCell::new(0) };
    static RESTARTS: RefCell<u64> = const { RefCell::new(0) };
    static LATE_FILL_FEES: RefCell<u64> = const { RefCell::new(0) };
}
struct Observed<
    const FORCE: bool = false,
    S: hft_strategy_sdk::StrategySet = TestStrategies,
    const RECOVER: bool = false,
>(SeqSut<S>, bool, u64);

fn capture(events: &[EventStep]) {
    for e in events {
        if e.records.first().is_some_and(|r| {
            r.try_read::<hft_event::OrderEvent>().is_some_and(|ev| {
                matches!(
                    ev.what,
                    hft_event::OrderEventKind::Fill | hft_event::OrderEventKind::PartialFill
                )
            })
        }) && e.after.hedge_balances.iter().any(|h| {
            e.before.hedge_balances.iter().any(|b| {
                (b.account, b.group, b.inst) == (h.account, h.group, h.inst) && b.fee_adjust != h.fee_adjust
            })
        }) {
            LATE_FILL_FEES.with(|n| *n.borrow_mut() += 1);
        }
        if e.records.first().is_some_and(|r| r.kind == EventKind::TIMER) {
            TIMERS.with(|n| *n.borrow_mut() += 1);
        }
        if !e.info.decisions.is_empty()
            || !e.info.force_targets.is_empty()
            || e.records.iter().any(|r| {
                r.try_read::<OrderDecision>()
                    .is_some_and(|d| d.action == OrderAction::Cancel)
            })
        {
            SAMPLES.with(|s| {
                let mut s = s.borrow_mut();
                // Only this test's selected counterexample copies are sampled; the run checks all Inputs.
                if s.len() < 128 {
                    s.push(e.clone());
                }
            });
        }
    }
}

#[test]
fn tk_111_transport_cancel_needs_decision_evidence() {
    let suite = hft_testkit::scenarios::suites()
        .into_iter()
        .find(|s| s.name == "storage")
        .unwrap();
    let c = suite.config(0xcef1_45cc_d5e6_8192);
    let r = run_record::<Observed>(&c);
    assert!(r.outcome.is_pass(), "{}", crate::seq_sut::show(&r));
    let samples = SAMPLES.with(|s| s.take());
    let e = samples
        .iter()
        .find(|e| {
            e.records.first().is_some_and(|r| {
                matches!(
                    r.kind,
                    EventKind::TIMER | EventKind::CONN_STATE | EventKind::SEND_REPORT
                )
            }) && e.before.slots.iter().any(|b| {
                b.state == hft_testkit::dst::obs::SlotState::PendingNew
                    && e.after
                        .slot(b.venue, b.account, b.slot)
                        .is_some_and(|a| a.state == hft_testkit::dst::obs::SlotState::PendingCancel)
            })
        })
        .expect("transport-triggered cancel not exercised");
    assert!(check(InvId::I1, e).is_ok());
    let mut bad = e.clone();
    bad.records.retain(|r| {
        !r.try_read::<OrderDecision>()
            .is_some_and(|d| d.action == OrderAction::Cancel)
    });
    assert!(
        check(InvId::I1, &bad).is_err(),
        "cancel without decision evidence accepted"
    );
}

impl<const FORCE: bool, S: hft_strategy_sdk::StrategySet, const RECOVER: bool> Sut
    for Observed<FORCE, S, RECOVER>
{
    type Matcher = <SeqSut<S> as Sut>::Matcher;
    fn boot(c: &BootCtx<'_>) -> Result<Self, BootFail> {
        SAMPLES.with(|s| s.borrow_mut().clear());
        TIMERS.with(|n| *n.borrow_mut() = 0);
        RESTARTS.with(|n| *n.borrow_mut() = 0);
        LATE_FILL_FEES.with(|n| *n.borrow_mut() = 0);
        SeqSut::<S>::boot(c).map(|e| Self(e, false, 0))
    }
    fn process(&mut self, n: u64, t: u64, i: &InputRec, o: &mut StepOut) {
        self.0.process(n, t, i, o);
        self.2 += 1;
        if RECOVER && [1_000, 3_000].contains(&self.2) {
            let mut before = Obs::default();
            self.0.observe(&mut before);
            let fees = |o: &Obs| {
                o.hedge_balances
                    .iter()
                    .map(|h| (h.account, h.group, h.inst, h.fee_adjust))
                    .collect::<Vec<_>>()
            };
            let totals = |o: &Obs| {
                let mut m = std::collections::BTreeMap::<(u8, u32, u32), (i128, i128)>::new();
                for h in &o.hedge_history {
                    let r = m.entry((h.account.0, h.group.0, h.inst.0)).or_default();
                    r.0 += h.first_net;
                    r.1 += h.hedge_req;
                }
                for l in o
                    .links
                    .iter()
                    .filter(|l| l.plan == hft_testkit::dst::obs::PlanKind::OnFill && !l.reversed)
                {
                    let r = m.entry((l.account.0, l.group.0, l.hedge_inst.0)).or_default();
                    r.0 += l.first_net;
                    r.1 += l.hedge_req;
                }
                m
            };
            for _ in 0..2 {
                self.0
                    .crash_and_recover(CrashPoint::SeqInput, None)
                    .expect("recover fee observation");
                self.0.drain_observations(o);
                let mut after = Obs::default();
                self.0.observe(&mut after);
                assert_eq!(fees(&before), fees(&after), "fees lost or repeated on restart");
                assert_eq!(
                    totals(&before),
                    totals(&after),
                    "link totals lost or repeated on restart"
                );
                for old in &before.hedge_history {
                    let h = after
                        .hedge_history
                        .iter()
                        .find(|h| {
                            (h.account, h.group, h.inst, h.ratio_num, h.ratio_den)
                                == (old.account, old.group, old.inst, old.ratio_num, old.ratio_den)
                        })
                        .expect("lost historical bucket");
                    assert!(
                        h.links >= old.links && h.first_net >= old.first_net && h.hedge_req >= old.hedge_req,
                        "lost retired link totals"
                    );
                }
                RESTARTS.with(|n| *n.borrow_mut() += 1);
            }
        }
        if FORCE && !self.1 && n >= 2_000_000_000 {
            use hft_testkit::dst::world::{ACCT_A, P1, VP};
            use hft_types::{Notional, Px};
            use zerocopy::FromZeros;
            let pos = self.0.sequencer().engine().ledger.pos(ACCT_A, P1).qty.0;
            if pos != 0 {
                self.1 = true;
                let price = self.0.world().inst(P1).unwrap().bid;
                let mut mh = hft_event::MarketHdr::new_zeroed();
                mh.venue = VP;
                mh.inst = P1;
                let mark = hft_event::MarkPrice { h: mh, px: Px(price) };
                let mut hdr = hft_event::MarginReportHdr::new_zeroed();
                hdr.venue = VP;
                hdr.account = ACCT_A;
                hdr.source = 1;
                hdr.n = 1;
                hdr.margin_balance = Notional(100_000_000_000);
                hdr.maint_margin = Notional(10_000_000);
                hdr.avail_margin = Notional(99_000_000_000);
                let item = hft_event::MarginItem {
                    inst: P1,
                    adl_quantile: 255,
                    _pad: [0; 3],
                    liq_px: Px(if pos < 0 {
                        price + price / 20
                    } else {
                        price - price / 20
                    }),
                };
                let src = hft_testkit::dst::sut::Source::Venue(VP);
                let mut margin = InputRec::of(&hdr, src);
                margin.body.extend_from_slice(item.as_bytes());
                for input in [InputRec::of(&mark, src), margin] {
                    let mut extra = StepOut::default();
                    self.0.process(n, t, &input, &mut extra);
                    o.records.extend(extra.records);
                    o.frames.extend(extra.frames);
                    o.events.extend(extra.events);
                }
            }
        }
        capture(&o.events);
    }
    fn fire_timers(&mut self, n: u64, t: u64, o: &mut StepOut) -> bool {
        let fired = self.0.fire_timers(n, t, o);
        capture(&o.events);
        fired
    }
    fn drain_observations(&mut self, o: &mut StepOut) {
        let n = o.events.len();
        self.0.drain_observations(o);
        capture(&o.events[n..]);
    }
    fn next_deadline(&self) -> Option<u64> {
        self.0.next_deadline()
    }
    fn observe(&self, o: &mut Obs) {
        self.0.observe(o);
    }
    fn hashes(&self) -> ComponentHashes {
        self.0.hashes()
    }
    fn rebuilt_hashes(&self) -> Option<ComponentHashes> {
        self.0.rebuilt_hashes()
    }
    fn seq(&self) -> u64 {
        self.0.seq()
    }
    fn journal(&self) -> &[JRec] {
        self.0.journal()
    }
    fn crash_and_recover(&mut self, p: CrashPoint, i: Option<&InputRec>) -> Result<(), BootFail> {
        self.0.crash_and_recover(p, i)
    }
    fn clean_restart(&mut self) -> Result<(), BootFail> {
        self.0.clean_restart()
    }
    fn is_sim(&self, a: AccountId) -> bool {
        self.0.is_sim(a)
    }
}

#[test]
fn tk_110_real_fee_history_across_restarts() {
    let c = crate::fee_cycle::config(6_000);
    let r = run_record::<Observed<false, crate::fee_cycle::FeeStrategies, true>>(&c);
    assert!(r.outcome.is_pass(), "{}", crate::seq_sut::show(&r));
    assert_eq!(r.steps_run, 10_000);
    assert_eq!(RESTARTS.with(|n| *n.borrow()), 4);
    assert!(
        LATE_FILL_FEES.with(|n| *n.borrow() > 0),
        "late fill fee not exercised"
    );
    assert!(r.obs.hedge_history.iter().any(|h| h.links > 0));
    assert!(r.obs.hedge_balances.iter().any(|h| h.fee_adjust != 0));
}

#[test]
fn tk_109_real_force_target_reference() {
    let c = crate::support::cfg(1, 2_000, &[], 0, &[]);
    let r = run_record::<Observed<true>>(&c);
    assert!(r.outcome.is_pass(), "{}", crate::seq_sut::show(&r));
    let samples = SAMPLES.with(|s| s.take());
    let e = samples
        .iter()
        .find(|e| e.info.force_targets.iter().any(|f| f.base != 0))
        .expect("no force target observed");
    assert!(check(InvId::I17, e).is_ok());
    let mut bad = e.clone();
    let f = bad.info.force_targets.iter_mut().find(|f| f.base != 0).unwrap();
    f.target = -f.base;
    assert!(check(InvId::I17, &bad).is_err(), "wrong force target accepted");
    let mut bad = e.clone();
    bad.info.force_targets.clear();
    bad.info.force_fractions.clear();
    assert!(
        check(InvId::I17, &bad).is_err(),
        "missing all force targets accepted"
    );
}

fn check(id: InvId, e: &EventStep) -> Result<(), (InvId, String)> {
    check_step(
        InvariantSet::only(id),
        &mut InvState::default(),
        &StepView {
            step: 0,
            now_ns: e.after.engine_ns.unwrap_or(0) as u64,
            records: &e.records,
            delivered: None,
            frames: &e.frames,
            info: &e.info,
            before: &e.before,
            after: &e.after,
            truth: &[],
            rebuild: None,
        },
    )
}

#[test]
fn tk_107_real_observations_and_counterexamples() {
    let c = crate::support::cfg(1, 2_000, &[], 0, &[]);
    let r = run_record::<Observed>(&c);
    assert!(r.outcome.is_pass(), "{}", crate::seq_sut::show(&r));
    assert!(
        TIMERS.with(|n| *n.borrow() > 0),
        "internal Timer was not observed"
    );
    let samples = SAMPLES.with(|s| s.take());
    let e = samples
        .iter()
        .find(|e| !e.info.decisions.is_empty())
        .expect("no decision observation");
    assert!(check(InvId::I11, e).is_ok());
    let mut bad = e.clone();
    bad.after.kill[0].global = !bad.after.kill[0].global;
    assert!(check(InvId::I11, &bad).is_err(), "wrong Kill accepted");
    let mut bad = e.clone();
    let cell = &mut bad.after.cells[0];
    cell.effective = if cell.effective == hft_types::RunMode::Live {
        hft_types::RunMode::Shadow
    } else {
        hft_types::RunMode::Live
    };
    assert!(check(InvId::I11, &bad).is_err(), "wrong effective mode accepted");
    let mut bad = e.clone();
    bad.after.cells.clear();
    assert!(check(InvId::I11, &bad).is_err(), "missing modes accepted");
    let mut bad = e.clone();
    bad.after.margin.clear();
    assert!(check(InvId::I17, &bad).is_err(), "missing margin accepted");
    let mut bad = e.clone();
    bad.after.exec.clear();
    assert!(check(InvId::I18, &bad).is_err(), "missing exec accepted");
    let mut bad = e.clone();
    bad.after.inflight.clear();
    assert!(check(InvId::I19, &bad).is_err(), "missing in-flight accepted");
    let mut bad = e.clone();
    bad.info.decisions.clear();
    assert!(check(InvId::I15, &bad).is_err(), "missing decision accepted");
    let mut bad = e.clone();
    bad.info.decision_inflight.clear();
    assert!(
        check(InvId::I19, &bad).is_err(),
        "missing decision-time in-flight accepted"
    );
    let mut bad = e.clone();
    bad.info.decision_limits.clear();
    assert!(
        check(InvId::I15, &bad).is_err(),
        "missing decision-time limits accepted"
    );
    let mut bad = e.clone();
    bad.info.decision_perp_pending.clear();
    assert!(check(InvId::I17, &bad).is_err(), "missing perp target accepted");
    let mut bad = e.clone();
    bad.after.observation_error = Some("diagnostic overflow".into());
    assert!(check(InvId::I11, &bad).is_err(), "overflow accepted");
    let mut out = StepOut {
        records: e.records.clone(),
        events: vec![e.clone()],
        ..StepOut::default()
    };
    assert!(out.verify_observations().is_ok());
    out.events.clear();
    assert!(out.verify_observations().is_err());
    out.events = vec![e.clone(), e.clone()];
    assert!(out.verify_observations().is_err());

    let (e, seq) = samples
        .iter()
        .find_map(|e| {
            e.info
                .decisions
                .iter()
                .find(|i| i.class == DecClass::Hedge)
                .filter(|i| {
                    e.records.iter().any(|r| {
                        r.seq == i.seq
                            && r.flags.0 & RecordFlags::SHADOW.0 == 0
                            && r.try_read::<OrderDecision>()
                                .is_some_and(|d| d.action == OrderAction::New && d.reject == 0)
                    })
                })
                .map(|i| (e, i.seq))
        })
        .expect("no admitted hedge to test");
    assert!(check(InvId::I15, e).is_ok());
    let mut bad = e.clone();
    let info = *bad.info.decisions.iter().find(|i| i.seq == seq).unwrap();
    let rec = bad.records.iter_mut().find(|r| r.seq == seq).unwrap();
    let mut d = rec.try_read::<OrderDecision>().unwrap();
    let lot = bad.after.inst(d.inst).unwrap().lot;
    d.qty.0 = (info.basis + 2 * lot) as i64;
    rec.body = d.as_bytes().to_vec();
    assert!(check(InvId::I15, &bad).is_err(), "oversized hedge accepted");
    let mut bad = e.clone();
    let i = bad.info.decisions.iter_mut().find(|i| i.seq == seq).unwrap();
    i.class = DecClass::Flatten;
    i.margin_stress = true;
    i.pos = d.qty.0 as i128;
    let rec = bad.records.iter_mut().find(|r| r.seq == seq).unwrap();
    let mut d = rec.try_read::<OrderDecision>().unwrap();
    d.side = Side::Buy;
    rec.body = d.as_bytes().to_vec();
    assert!(
        check(InvId::I17, &bad).is_err(),
        "wrong reduction direction accepted"
    );
}
