use hft_strat_lib::{FundingObs, FundingStats, funding_flags};
use hft_strategy_sdk::FundingView;
use hft_types::Rate;
const H: i64 = 3_600_000_000_000;
fn view(rate: i64, next: i64, interval: u32) -> FundingView {
    FundingView {
        rate: Rate(rate),
        next_settle_ns: next,
        interval_s: interval,
        ..FundingView::EMPTY
    }
}
#[test]
fn slb_068_live_settlement_period_change_and_regression() {
    let mut s = FundingStats::new();
    assert_eq!(s.summary(24 * H, 1, H).flags, funding_flags::NO_DATA);
    assert_eq!(
        s.observe(&view(100_000, 16 * H, 28_800), 15 * H),
        FundingObs::Normal
    );
    s.observe(&view(120_000, 16 * H, 28_800), 15 * H + H / 2);
    assert!(
        matches!(s.observe(&view(-50_000, 24*H, 28_800), 16*H+1), FundingObs::Settled(rec) if rec.rate == Rate(120_000) && rec.at_ns == 16*H)
    );
    assert_eq!(s.summary(24 * H, 16 * H + 1, H).realized_sum, Rate(120_000));
    assert_eq!(
        s.observe(&view(-50_000, 24 * H, 3_600), 17 * H),
        FundingObs::IntervalChanged {
            old_s: 28_800,
            new_s: 3_600
        }
    );
    let sum = s.summary(24 * H, 17 * H, H);
    assert_eq!(sum.predicted_daily, Rate(-1_200_000));
    assert_eq!(sum.flags, funding_flags::INTERVAL_CHANGED);
    assert_eq!(s.observe(&view(999, 23 * H, 14_400), 17 * H), FundingObs::Normal);
    assert_eq!(s.summary(24 * H, 17 * H, H), sum);
    assert!(
        matches!(s.observe(&view(3, 26*H, 3_600), 24*H+1), FundingObs::Settled(rec) if rec.flags == funding_flags::GAP)
    );
    let sum = s.summary(24 * H, 24 * H + 1, H);
    assert_eq!(sum.realized_sum, Rate(70_000));
    assert_eq!((sum.n_positive, sum.n_negative, sum.neg_streak), (1, 1, 1));
    assert_ne!(sum.flags & funding_flags::GAP, 0);
}
#[test]
fn slb_079_daily_half_even_saturation_and_unknown_period() {
    for (raw, interval, expected) in [
        (1, 172_800, 0),
        (3, 172_800, 2),
        (-3, 172_800, -2),
        (i64::MAX, 1, i64::MAX),
        (i64::MIN, 1, -i64::MAX),
        (9, 0, 0),
    ] {
        assert_eq!(FundingStats::daily(Rate(raw), interval), Rate(expected));
    }
}
