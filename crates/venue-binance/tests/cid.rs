//! BIN-078, BIN-079: client id shape and round trip.

use crate::support;

use hft_types::{AccountId, ClientOrderId, VenueId};
use hft_venue_proto::VenueProtocol;
use proptest::prelude::*;
use support::cid;
use venue_binance::{Spot, Usdm};

#[test]
fn bin_078_cid_shape() {
    let c = cid(VenueId::BINANCE_USDM, 3, 42, 7);
    let mut buf = [0u8; 48];
    assert_eq!(Usdm::render_cid(&c, &mut buf), 24, "BIN-078.1");
    assert_eq!(&buf[..24], b"1000301020002a000000073d", "BIN-078.1");
    assert_eq!(Usdm::parse_cid(b"1000301020002a000000073d"), Some(c), "BIN-078.2");
    let none: [(&str, &[u8]); 7] = [
        ("BIN-078.3", b"1000301020002a000000073c"),
        ("BIN-078.4", b"1000301020002A000000073D"),
        ("BIN-078.5", b"1000301020002a000000073d0"),
        ("BIN-078.6", b"1800301020002a0000000735"),
        ("BIN-078.7", b"autoclose-1790000000000"),
        ("BIN-078.8", b"web_8fa3b1"),
        ("BIN-078.9", b""),
    ];
    for (row, t) in none {
        assert_eq!(Usdm::parse_cid(t), None, "{row}");
    }
}

fn charset_ok(s: &[u8]) -> bool {
    (1..=36).contains(&s.len())
        && s.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b':' | b'/' | b'_' | b'-'))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]
    #[test]
    fn bin_079_cid_roundtrip(
        epoch in 0u16..(1 << 15),
        venue in prop::sample::select(vec![VenueId::BINANCE_USDM, VenueId::BINANCE_SPOT]),
        account in any::<u8>(),
        slot in 0u32..(1 << 20),
        r#gen in any::<u32>(),
    ) {
        let x = ClientOrderId { epoch, venue, account: AccountId(account), slot, r#gen };
        let mut b = [0u8; 48];
        let n = Usdm::render_cid(&x, &mut b);
        prop_assert_eq!(n, 24);
        prop_assert!(charset_ok(&b[..n]));
        prop_assert_eq!(Usdm::parse_cid(&b[..n]), Some(x));
        let mut s = [0u8; 48];
        let m = Spot::render_cid(&x, &mut s);
        prop_assert_eq!(&s[..m], &b[..n]);
        prop_assert_eq!(Spot::parse_cid(&s[..m]), Some(x));
    }
}
