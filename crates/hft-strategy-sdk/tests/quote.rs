//! Quote health (SDK-122, SDK-123).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_strategy_sdk::testing::MockViews;
use hft_strategy_sdk::*;
use hft_types::{Px, Qty, Tsc};
use proptest::prelude::*;

const NOW: u64 = 100_000_000_000;
const SILENCE: u64 = 6_000_000_000;

#[test]
fn sdk_122_quote_live_boundaries() {
    let base = BboView {
        conn_up: true,
        resync: false,
        conn_live_tsc: Tsc(NOW),
        rx_tsc: Tsc(NOW),
        ..BboView::EMPTY
    };
    let rows: [(&str, BboView, bool); 9] = [
        ("SDK-122.1", base, true),
        (
            "SDK-122.2",
            BboView {
                conn_live_tsc: Tsc(NOW - 6_000_000_000),
                ..base
            },
            true,
        ),
        (
            "SDK-122.3",
            BboView {
                conn_live_tsc: Tsc(NOW - 6_000_000_001),
                ..base
            },
            false,
        ),
        (
            "SDK-122.4",
            BboView {
                conn_live_tsc: Tsc(NOW - 5_999_999_999),
                ..base
            },
            true,
        ),
        (
            "SDK-122.5",
            BboView {
                conn_up: false,
                ..base
            },
            false,
        ),
        ("SDK-122.6", BboView { resync: true, ..base }, false),
        (
            "SDK-122.7",
            BboView {
                rx_tsc: Tsc(NOW - 30_000_000_000),
                ..base
            },
            true,
        ),
        (
            "SDK-122.8",
            BboView {
                conn_live_tsc: Tsc(NOW - 7_000_000_000),
                ..base
            },
            false,
        ),
        (
            "SDK-122.9",
            BboView {
                conn_live_tsc: Tsc(0),
                ..base
            },
            false,
        ),
    ];
    for (row, b, want) in rows {
        assert_eq!(b.quote_live(Tsc(NOW), SILENCE), want, "{row}");
    }
}

fn views(liveness_ms: u32, tsc_per_ms: u64, now: u64, live: u64, up: bool, resync: bool) -> MockViews {
    let mut v = gt3_views();
    let clk = EngineClockAnchor {
        base_engine_ns: 0,
        base_tsc: Tsc(0),
        tsc_hz_milli: tsc_per_ms * 1_000_000,
        utc_tsc: Tsc(0),
        utc_ns: 0,
    };
    v.at(Now::new(Tsc(now), 1, 1, true, clk));
    // Setters through `&mut MockViews`: on a plain `MockViews` receiver the `&self` getters of
    // `Views` win method lookup.
    let m = &mut v;
    m.risk(
        A1,
        RiskStateView {
            conn_liveness_ms: liveness_ms,
            ..RiskStateView::EMPTY
        },
    );
    m.bbo(I1, (Px(100), Qty(1)), (Px(101), Qty(1)));
    m.conn(I1, Tsc(live), up, resync);
    v
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn sdk_123_quote_ok_is_engine_threshold(
        liveness in 1u32..=10_000,
        tsc_per_ms in 1_000_000u64..=4_000_000,
        now in 1u64..=1_000_000_000_000_000,
        frac in 0u64..=1_000,
        edge in -1i64..=1,
        use_edge in any::<bool>(),
        up in any::<bool>(),
        resync in any::<bool>(),
    ) {
        let silence = u64::from(liveness) * tsc_per_ms;
        let live = if use_edge {
            (now.saturating_sub(silence) as i64 + edge).clamp(0, now as i64) as u64
        } else {
            now / 1_000 * frac
        };
        let v = views(liveness, tsc_per_ms, now, live, up, resync);
        let n = Views::now(&v);
        let expect = Views::bbo(&v, I1).quote_live(n.tsc, u64::from(Views::risk(&v, A1).conn_liveness_ms) * n.tsc_per_ms);
        prop_assert_eq!(v.quote_ok(A1, I1), expect);
    }
}
