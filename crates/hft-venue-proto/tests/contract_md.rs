//! Market-data contract checks on the built-in fake protocol (VPR-066).

#[path = "fake_venue.rs"]
mod fake_venue;

use fake_venue::{FakeFeedCtx, FakePerp, depth20_payload, fake_rd};
use hft_types::{AccountId, ConnId};
use hft_venue_proto::testkit::{MdRec, MdSample, check_md_basics, decode_samples};
use hft_venue_proto::{MsgOp, RouteId, RxMeta};

const BBO: &[u8] =
    br#"{"e":"bbo","i":201,"u":7,"b":"100.5","B":"1.25","a":"100.6","A":"2","T":1700000000000}"#;
const TRADE: &[u8] = br#"{"e":"trade","i":201,"t":99,"p":"100.5","q":"0.1","m":1,"T":1700000000000}"#;
const MARK: &[u8] = br#"{"e":"mark","i":201,"p":"100.55","T":1700000000000}"#;

fn samples<'a>(rx: RxMeta, depth: &'a [u8]) -> Vec<MdSample<'a>> {
    [("bbo", BBO), ("trade", TRADE), ("mark", MARK), ("depth20", depth)]
        .into_iter()
        .map(|(name, payload)| MdSample {
            name,
            route: RouteId(0),
            op: MsgOp::Text,
            payload,
            rx,
        })
        .collect()
}

#[test]
fn vpr_066_rx_meta_copied() {
    let rd = fake_rd();
    let depth = depth20_payload(201);
    let rx = RxMeta {
        rx_tsc: 0x1111,
        hw_rx_ns: 0x2222,
        sw_rx_ns: 0x3333,
        prev_override: 0,
        conn: ConnId(7),
        account: AccountId(0),
        ts_flags: 2,
        rx_empty_gap_ns: 0x4444,
        ..RxMeta::default()
    };
    let mut ctx = FakeFeedCtx;
    let s1 = samples(rx, depth.as_bytes());
    let recs = decode_samples::<FakePerp>(&s1, &rd, &mut ctx).unwrap();
    assert_eq!(recs.len(), 4);
    for (s, r) in s1.iter().zip(&recs) {
        assert_eq!(r.len(), 1, "{}: one event", s.name);
        let h = r[0].market_hdr().unwrap();
        assert_eq!(
            (h.rx_tsc, h.hw_rx_ns, h.sw_rx_ns, h.rx_empty_gap_ns, h.conn),
            (0x1111, 0x2222, 0x3333, 0x4444, ConnId(7)),
            "{}",
            s.name
        );
    }
    assert!(
        matches!(&recs[3][0], MdRec::L2Snapshot { bids, asks, .. } if bids.len() == 20 && asks.len() == 20)
    );
    assert_eq!(check_md_basics::<FakePerp>(&s1, &rd, &mut ctx), Ok(()));

    // Second group: no HW stamp; the implementation does not substitute sw_rx_ns.
    let rx2 = RxMeta {
        hw_rx_ns: 0,
        ts_flags: 0,
        ..rx
    };
    let s2 = samples(rx2, depth.as_bytes());
    let recs = decode_samples::<FakePerp>(&s2, &rd, &mut ctx).unwrap();
    for (s, r) in s2.iter().zip(&recs) {
        let h = r[0].market_hdr().unwrap();
        assert_eq!(h.hw_rx_ns, 0, "{}", s.name);
        assert_eq!(h.sw_rx_ns, 0x3333, "{}", s.name);
    }
    assert_eq!(check_md_basics::<FakePerp>(&s2, &rd, &mut ctx), Ok(()));
}
