use hft_refdata::{LimitBucket, LimitScope, RateLimit};
use hft_venue::RateCounters;
use hft_venue_proto::RateObs;
#[test]
fn ven_224_uid_and_multiple_windows() {
    let rule = |ms, limit| RateLimit {
        scope: LimitScope::Uid,
        bucket: LimitBucket::Orders,
        window_ms: ms,
        limit,
        default_cost: 1,
    };
    let mut c = RateCounters::new(&[rule(1000, 2), rule(10000, 3)], |ms| ms);
    assert!(c.take(0));
    assert!(c.take(0));
    assert!(!c.take(0));
    assert!(c.take(1000));
    assert!(!c.take(1000));
    assert!(!c.take(2000)); // Refusal did not charge the short window or release the long one.
    assert!(c.take(10000));
    c.observe_window(
        RateObs {
            bucket: LimitBucket::Orders,
            used: 2,
            limit: 2,
            window_ms: 1000,
        },
        10000,
    );
    c.observe_window(
        RateObs {
            bucket: LimitBucket::Orders,
            used: 0,
            limit: 2,
            window_ms: 1000,
        },
        10000,
    );
    assert!(!c.take(10000)); // A delayed zero count cannot erase charged orders.
    c.reset();
    assert!(c.take(10000));
}
