//! SLC-007: refdata from the configuration.

mod support;

use hft_refdata::AccountKind;
use hft_types::{AccountId, InstId, Notional, VenueId};
use hft_venue_proto::VenueProtocol;
use latency_slice::config;
use latency_slice::rd;
use support::{CFG0, ETH};

#[test]
fn slc_007_refdata_from_config() {
    let cfg = config::parse(&format!(
        "{CFG0}\n{ETH}\n[ballast]\nsymbols = [\"SOLUSDT\", \"XRPUSDT\"]\n"
    ))
    .unwrap();
    let b = rd::build(&cfg).unwrap();
    let r = b.rd;
    let syms: Vec<(u32, String, VenueId)> = (1..=4u32)
        .map(|i| {
            (
                i,
                r.inst_meta(InstId(i)).symbol.to_string(),
                r.inst(InstId(i)).venue,
            )
        })
        .collect();
    let want: Vec<(u32, String, VenueId)> = ["BTCUSDT", "ETHUSDT", "SOLUSDT", "XRPUSDT"]
        .iter()
        .enumerate()
        .map(|(k, s)| (k as u32 + 1, s.to_string(), VenueId::BINANCE_SPOT))
        .collect();
    assert_eq!(syms, want, "SLC-007.1");
    assert_eq!(r.inst_count(), 4, "SLC-007.1");
    let i = |n| *r.inst(InstId(n));
    let (btc, eth, sol) = (i(1), i(2), i(3));
    assert_eq!(
        (btc.px_exp, btc.qty_exp, btc.tick, btc.lot),
        (-2, -8, 1, 1_000),
        "SLC-007.2"
    );
    assert_eq!((eth.px_exp, eth.qty_exp, eth.lot), (-2, -8, 10_000), "SLC-007.3");
    assert_eq!(
        (sol.px_exp, sol.qty_exp, sol.tick, sol.lot),
        (-4, -8, 1, 100_000),
        "SLC-007.4"
    );
    let a = r.account(AccountId(1));
    assert!(matches!(a.kind, AccountKind::Live), "SLC-007.5");
    assert!(!a.is_prod, "SLC-007.5");
    assert_eq!(a.venues, 1u64 << VenueId::BINANCE_SPOT.0, "SLC-007.5");
    let asset = |n: &str| {
        (1..=10u16)
            .map(hft_types::AssetId)
            .find(|x| &*r.asset(*x).name == n)
    };
    assert_eq!(
        *r.caps(VenueId::BINANCE_SPOT),
        venue_binance::Spot::default_caps(asset),
        "SLC-007.6"
    );
    let limits: Vec<_> = r
        .rate_limits(VenueId::BINANCE_SPOT)
        .iter()
        .map(|l| (l.scope, l.bucket, l.window_ms, l.limit))
        .collect();
    let want: Vec<_> = venue_binance::Spot::rate_rules()
        .iter()
        .map(|l| (l.scope, l.bucket, l.window_ms, l.limit))
        .collect();
    for w in &want {
        assert!(limits.contains(w), "SLC-007.6: {w:?}");
    }
    assert_eq!(btc.min_notional, Notional(5_000_000), "SLC-007.7");
}
