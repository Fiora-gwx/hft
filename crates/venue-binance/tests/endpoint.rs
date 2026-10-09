//! BIN-004, BIN-005, BIN-007: endpoints and the listenKey bootstrap.

use crate::support;

use hft_venue_proto::{EndpointSpec, Method, ProtoErr, Role, RouteId, VenueProtocol};
use support::{cfg, ed1, hm1};
use venue_binance::{Spot, Usdm};

const NOW: i64 = 1_790_000_000_000;

fn route<P: VenueProtocol>(name: &str) -> RouteId {
    P::routes().iter().find(|r| r.name == name).unwrap().id
}

fn path<P: VenueProtocol>(role: Role, route: RouteId, boot: Option<&[u8]>) -> String {
    let mut out = EndpointSpec::default();
    let cred = ed1();
    let c = (role != Role::Md).then_some(&cred);
    P::endpoint(role, route, &cfg(1500), c, boot, NOW, &mut out).unwrap();
    String::from_utf8(out.path_query.as_slice().to_vec()).unwrap()
}

#[test]
fn bin_004_endpoint_paths() {
    let rows: [(&str, String, &str); 9] = [
        (
            "BIN-004.1",
            path::<Usdm>(Role::Md, route::<Usdm>("public-bbo"), None),
            "/public/ws",
        ),
        (
            "BIN-004.2",
            path::<Usdm>(Role::Md, route::<Usdm>("public-topn"), None),
            "/public/ws",
        ),
        (
            "BIN-004.3",
            path::<Usdm>(Role::Md, route::<Usdm>("market"), None),
            "/market/ws",
        ),
        (
            "BIN-004.4",
            path::<Usdm>(Role::UserStream, RouteId(0), Some(br#"{"listenKey":"lk0123"}"#)),
            "/private/ws/lk0123",
        ),
        (
            "BIN-004.5",
            path::<Usdm>(Role::Order, RouteId(0), None),
            "/ws-fapi/v1",
        ),
        (
            "BIN-004.6",
            path::<Spot>(Role::Md, route::<Spot>("spot-bbo"), None),
            "/ws?timeUnit=MICROSECOND",
        ),
        (
            "BIN-004.7",
            path::<Spot>(Role::Md, route::<Spot>("spot-md"), None),
            "/ws?timeUnit=MICROSECOND",
        ),
        (
            "BIN-004.8",
            path::<Spot>(Role::Md, route::<Spot>("spot-topn"), None),
            "/stream?timeUnit=MICROSECOND",
        ),
        (
            "BIN-004.9",
            path::<Spot>(Role::Order, RouteId(0), None),
            "/ws-api/v3?timeUnit=MICROSECOND",
        ),
    ];
    for (row, got, want) in rows {
        assert_eq!(got, want, "{row}");
    }
}

#[test]
fn bin_005_spot_no_user_stream_role() {
    let mut out = EndpointSpec::default();
    let cred = ed1();
    let r = Spot::endpoint(
        Role::UserStream,
        RouteId(0),
        &cfg(1500),
        Some(&cred),
        None,
        NOW,
        &mut out,
    );
    assert_eq!(r, Err(ProtoErr::Unsupported));
    assert_eq!(out.path_query.len(), 0);
}

#[test]
fn bin_007_bootstrap() {
    let c = cfg(1500);
    let hm = hm1();
    // BIN-007.1
    let b = Usdm::bootstrap(Role::UserStream, &c, Some(&hm)).expect("BIN-007.1");
    assert_eq!(b.method, Method::Post);
    assert_eq!(b.endpoint, 0);
    assert_eq!(b.path_query.as_slice(), b"/fapi/v1/listenKey");
    let h = String::from_utf8(b.headers.as_slice().to_vec()).unwrap();
    assert!(h.contains("X-MBX-APIKEY: TESTKEY"), "{h}");
    assert!(!b.path_query.as_slice().contains(&b'?'));
    assert!(!b.redacted);
    // BIN-007.2 – .5
    assert!(Usdm::bootstrap(Role::Md, &c, None).is_none(), "BIN-007.2");
    assert!(Usdm::bootstrap(Role::Order, &c, Some(&hm)).is_none(), "BIN-007.3");
    assert!(Spot::bootstrap(Role::Order, &c, Some(&hm)).is_none(), "BIN-007.4");
    assert!(Spot::bootstrap(Role::Md, &c, None).is_none(), "BIN-007.5");
}
