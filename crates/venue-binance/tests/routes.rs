//! BIN-002, BIN-003: route tables.

use hft_venue_proto::{RouteKind, RouteSpec, VenueProtocol};
use venue_binance::{Spot, Usdm};

fn find(rs: &[RouteSpec], name: &str) -> RouteSpec {
    *rs.iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no route {name}"))
}

fn distinct_ids(rs: &[RouteSpec]) {
    for (i, a) in rs.iter().enumerate() {
        for b in &rs[i + 1..] {
            assert_ne!(a.id, b.id, "{} / {}", a.name, b.name);
        }
    }
}

#[test]
fn bin_002_usdm_routes() {
    let rs = Usdm::routes();
    assert_eq!(rs.len(), 4);
    distinct_ids(rs);
    let rows: [(&str, RouteKind, bool, &[u8]); 4] = [
        ("public-bbo", RouteKind::Bbo, true, &[]),
        ("public-topn", RouteKind::DepthTopN, false, &[5, 10, 20]),
        ("market", RouteKind::Md, false, &[]),
        ("public-depth", RouteKind::Md, false, &[]),
    ];
    for (k, (name, kind, race, topn)) in rows.into_iter().enumerate() {
        let r = find(rs, name);
        assert_eq!(
            (r.kind, r.race, r.max_streams, r.max_in_msgs_per_s, r.topn_levels),
            (kind, race, 1024, 10, topn),
            "BIN-002.{}",
            k + 1
        );
    }
    // BIN-002.5
    assert!(rs.iter().filter(|r| r.kind != RouteKind::Bbo).all(|r| !r.race));
}

#[test]
fn bin_003_spot_routes() {
    let rs = Spot::routes();
    assert_eq!(rs.len(), 3);
    distinct_ids(rs);
    let rows: [(&str, RouteKind, bool, &[u8]); 3] = [
        ("spot-bbo", RouteKind::Bbo, true, &[]),
        ("spot-topn", RouteKind::DepthTopN, false, &[5, 10, 20]),
        ("spot-md", RouteKind::Md, false, &[]),
    ];
    for (k, (name, kind, race, topn)) in rows.into_iter().enumerate() {
        let r = find(rs, name);
        assert_eq!(
            (r.kind, r.race, r.max_streams, r.max_in_msgs_per_s, r.topn_levels),
            (kind, race, 1024, 5, topn),
            "BIN-003.{}",
            k + 1
        );
    }
}
