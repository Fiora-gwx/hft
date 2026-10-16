use crate::support::{gwh::*, rd::*, stub::StubProto};
use hft_event::RepMsg;
use hft_refdata::RefData;
use hft_types::{Notional, Tsc};
use hft_venue::{FundingPoll, FundingPollCfg, FundingPollErr};
use hft_venue_proto::{
    Credential, DecodeErr, FundingBill, FundingHistoryProtocol, FundingPage, FundingQuery, ProtoErr,
    RestReqBuf, VenueCfg, VenueEnv, WallClock,
};

impl FundingHistoryProtocol for StubProto {
    fn funding_request(
        _: &VenueCfg,
        _: &Credential,
        q: &FundingQuery,
        _: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        assert!(q.end_ms > q.start_ms, "venue rejects an empty funding window");
        out.redacted = true;
        Ok(())
    }
    fn funding_decode(
        _: &RefData,
        q: &FundingQuery,
        status: u16,
        body: &[u8],
    ) -> Result<FundingPage, DecodeErr> {
        if status != 200 {
            return Err(DecodeErr::Shape);
        }
        let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| DecodeErr::Shape)?;
        let n = value["n"].as_i64().ok_or(DecodeErr::Shape)?;
        let id = value["id"].as_u64().ok_or(DecodeErr::Shape)?;
        Ok(FundingPage {
            bills: vec![FundingBill {
                id,
                inst: S1,
                time_ns: 28_800_000_000_000,
                amount: Notional(n),
            }],
            next: value["more"].as_bool().unwrap_or(false).then(|| FundingQuery {
                page: q.page + 1,
                ..q.clone()
            }),
            skipped_foreign: 0,
        })
    }
}

#[test]
fn ven_258_first_funding_window_waits_past_settle_lag_boundary() {
    let start = 1_792_000_000_000;
    let mut poll = FundingPoll::<StubProto>::new(
        A1,
        start,
        FundingPollCfg {
            poll_ms: 5000,
            lookback_ms: 172_800_000,
            settle_lag_ms: 600_000,
            max_settlement_skew_ms: 1000,
            max_pages: 64,
            max_bills: 4096,
        },
    )
    .unwrap();
    let venue = VenueCfg {
        env: VenueEnv::Testnet,
        stale_guard_ms: 1500,
    };
    for elapsed in [0, 599_999, 600_000] {
        let clock = WallClock {
            unix_ms: start + elapsed,
            unix_us: 0,
        };
        assert!(
            poll.request(&venue, &Credential::default(), &clock)
                .unwrap()
                .is_none()
        );
    }
    let clock = WallClock {
        unix_ms: start + 600_001,
        unix_us: 0,
    };
    assert!(
        poll.request(&venue, &Credential::default(), &clock)
            .unwrap()
            .is_some()
    );
}

#[test]
fn ven_258_funding_pages_atomic_retry_dedup_and_gateway_ingress() {
    let cfg = FundingPollCfg {
        poll_ms: 1000,
        lookback_ms: 86_400_000,
        settle_lag_ms: 10_000,
        max_settlement_skew_ms: 1000,
        max_pages: 5,
        max_bills: 10,
    };
    let mut p = FundingPoll::<StubProto>::new(A1, 0, cfg).unwrap();
    let vc = VenueCfg {
        env: VenueEnv::Prod,
        stale_guard_ms: 5000,
    };
    let mut clock = WallClock {
        unix_ms: 30_000_000,
        unix_us: 30_000_000_000,
    };
    let key = Credential::default();
    let rd = rd_stub();
    p.request(&vc, &key, &clock).unwrap().unwrap();
    assert!(p.request(&vc, &key, &clock).unwrap().is_none());
    p.response(rd, 200, br#"{"n":10,"id":1,"more":true}"#, clock.unix_ms, Tsc(5))
        .unwrap();
    assert!(p.pending().is_empty());
    clock.unix_ms += 1000;
    p.request(&vc, &key, &clock).unwrap().unwrap();
    assert_eq!(
        p.response(rd, 429, b"bad", clock.unix_ms, Tsc(6)),
        Err(FundingPollErr::Decode)
    );
    assert!(p.pending().is_empty());
    clock.unix_ms += 1000;
    p.request(&vc, &key, &clock).unwrap().unwrap();
    // Repeated ID across pages does not double-book it.
    p.response(rd, 200, br#"{"n":10,"id":1}"#, clock.unix_ms, Tsc(7))
        .unwrap();
    assert_eq!(p.pending().len(), 1);
    assert_eq!(p.pending()[0].amount, Notional(10));
    let mut h = gw::<StubProto>();
    h.reps();
    assert!(p.publish_one(&mut h.acct));
    assert!(!p.publish_one(&mut h.acct));
    h.poll(3_000_000);
    let settlements: Vec<_> = h
        .reps()
        .into_iter()
        .filter_map(|m| match m {
            RepMsg::FundingSettle(f) => Some(f),
            _ => None,
        })
        .collect();
    assert_eq!(settlements.len(), 1);
    assert_eq!(
        (settlements[0].account, settlements[0].inst, settlements[0].amount),
        (A1, S1, Notional(10))
    );
}
