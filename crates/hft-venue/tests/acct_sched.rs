use crate::support::{rd::*, stub::StubProto};
use hft_refdata::MarginData;
use hft_types::MarketKind;
use hft_venue::{AcctSched, AcctState, GwCfg, RateCounters};
use hft_venue_proto::{AcctHint, AcctProtocol, AcctQueryKind};
fn sched(data: MarginData) -> AcctSched {
    AcctSched::new(A1, MarketKind::LinearPerp, data, &GwCfg::default(), |m| m)
}
fn due(s: &AcctSched, now: u64) -> Option<(hft_venue::AcctReq, u32)> {
    s.due(
        now,
        true,
        StubProto::acct_queries(AcctQueryKind::Config),
        StubProto::acct_queries(AcctQueryKind::Margin),
    )
}
fn finish(s: &mut AcctSched, now: u64) -> hft_venue::AcctReq {
    let (r, _) = due(s, now).unwrap();
    s.started(r);
    assert_eq!(s.done(r.seq_no, now), Some(false));
    r
}
#[test]
fn ven_250_startup_config_waits_for_readiness() {
    let mut s = sched(MarginData::Poll { interval_ms: 2000 });
    assert!(
        s.due(0, false, StubProto::acct_queries(AcctQueryKind::Config), &[])
            .is_none()
    );
    let (r, w) = due(&s, 0).unwrap();
    assert_eq!((r.kind, r.why, w), (AcctQueryKind::Config, 5, 6));
    s.started(r);
    assert!(due(&s, 999).is_none());
    assert_eq!(s.done(r.seq_no + 1, 999), None);
    assert_eq!(s.done(r.seq_no, 1000), Some(false));
    assert_eq!(finish(&mut s, 1000).kind, AcctQueryKind::Margin);
    assert!(due(&s, 2999).is_none());
}
#[test]
fn ven_251_poll_interval_from_completion() {
    let mut s = sched(MarginData::Poll { interval_ms: 2000 });
    finish(&mut s, 0);
    finish(&mut s, 250);
    assert!(due(&s, 2249).is_none());
    assert_eq!(finish(&mut s, 2250).poll_no, 1);
}
#[test]
fn ven_252_every_n_queries_weight_matches_selection() {
    let mut s = sched(MarginData::Poll { interval_ms: 2000 });
    finish(&mut s, 0);
    for (t, weight, poll) in [(0, 10, 0), (2000, 5, 1), (4000, 10, 2)] {
        let (r, w) = due(&s, t).unwrap();
        assert_eq!((w, r.poll_no), (weight, poll));
        s.started(r);
        s.done(r.seq_no, t);
    }
}
#[test]
fn ven_253_push_fallback_only_after_stale() {
    let mut s = sched(MarginData::Push);
    s.pushed(0);
    finish(&mut s, 0);
    assert!(due(&s, 4999).is_none());
    s.pushed(4000);
    assert!(due(&s, 8999).is_none());
    let r = finish(&mut s, 9000);
    assert_eq!(r.why, 4);
    assert!(due(&s, 13999).is_none());
}
#[test]
fn ven_254_priority_and_one_sequence_per_account() {
    let mut s = sched(MarginData::Poll { interval_ms: 2000 });
    s.hint(AcctHint::ConfigRecheck);
    s.hint(AcctHint::MarginNow);
    let r = due(&s, 0).unwrap().0;
    assert_eq!(r.why, 2);
    s.started(r);
    assert!(due(&s, u64::MAX).is_none());
    s.done(r.seq_no, 1);
    assert_eq!(finish(&mut s, 1).why, 3);
}
#[test]
fn ven_255_query_weight_reserve_and_urgent_hard_limit() {
    let limits = [hft_refdata::RateLimit {
        scope: hft_refdata::LimitScope::Account,
        bucket: hft_refdata::LimitBucket::Weight,
        window_ms: 1000,
        limit: 100,
        default_cost: 1,
    }];
    let mut r = RateCounters::account_rest(&limits, |m| m);
    assert!(r.weight_available(81, false, 0));
    r.charge_weight(81);
    assert!(!r.weight_available(1, false, 0));
    assert!(r.weight_available(19, true, 0));
    assert!(!r.weight_available(20, true, 0));
    assert_eq!(r.used(), 81);
    assert!(r.weight_available(100, true, 1000));
}
#[test]
fn ven_256_failure_backoff_degraded_once_and_recovery() {
    let mut s = sched(MarginData::NotApplicable);
    let mut now = 0;
    for (n, delay) in [1000, 2000, 4000, 8000, 16000, 30000, 30000]
        .into_iter()
        .enumerate()
    {
        let r = due(&s, now).unwrap().0;
        s.started(r);
        assert_eq!(s.failed(r.seq_no, now, 3), n == 2);
        assert!(due(&s, now + delay - 1).is_none());
        now += delay;
    }
    let r = due(&s, now).unwrap().0;
    s.started(r);
    assert_eq!(s.done(r.seq_no, now), Some(true));
    assert_eq!(s.state(), AcctState::Idle);
}
#[test]
fn ven_261_config_recheck_period_hint_and_session() {
    let mut s = sched(MarginData::NotApplicable);
    finish(&mut s, 0);
    assert!(due(&s, 599999).is_none());
    finish(&mut s, 600000);
    s.hint(AcctHint::ConfigRecheck);
    assert_eq!(finish(&mut s, 600001).why, 3);
    s.session(1, [b'U'; 24]);
    let r = finish(&mut s, 600002);
    assert_eq!((r.session, r.session_uid), (1, [b'U'; 24]));
}
#[test]
fn ven_262_spot_never_schedules_margin() {
    let mut s = AcctSched::new(A1, MarketKind::Spot, MarginData::Push, &GwCfg::default(), |m| m);
    finish(&mut s, 0);
    s.hint(AcctHint::MarginNow);
    assert!(due(&s, 5000).is_none());
    assert_eq!(finish(&mut s, 600000).kind, AcctQueryKind::Config);
}
#[test]
fn ven_390_effective_margin_interval() {
    let mut s = sched(MarginData::Poll { interval_ms: 7000 });
    finish(&mut s, 0);
    finish(&mut s, 0);
    assert!(due(&s, 6999).is_none());
    assert_eq!(finish(&mut s, 7000).kind, AcctQueryKind::Margin);
}
