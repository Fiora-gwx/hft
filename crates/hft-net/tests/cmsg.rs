//! NET-120: `SCM_TIMESTAMPING` parsing on synthetic control buffers.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_net::TsFlags;
use hft_net::cmsg::{MSG_CTRUNC, RxStamp, build_timestamping, parse_timestamping};

const S: i64 = 1_700_000_000;

fn buf(ts: [(i64, i64); 3]) -> Vec<u8> {
    let mut b = vec![0u8; 128];
    let n = build_timestamping(&mut b, ts);
    b.truncate(n);
    b
}

#[test]
fn net_120_cmsg_parse() {
    let sw = 1_700_000_000_000_000_005i64;
    let hw = 1_700_000_000_000_000_003i64;
    let two_only = {
        // The header claims two timespecs only.
        let mut b = buf([(S, 5), (0, 0), (S, 3)]);
        b[..8].copy_from_slice(&(16u64 + 32).to_ne_bytes());
        b.truncate(48);
        b
    };
    let rows: Vec<(&str, Vec<u8>, i32, RxStamp)> = vec![
        (
            "NET-120.1",
            buf([(S, 5), (0, 0), (0, 0)]),
            0,
            RxStamp {
                sw_rx_ns: sw,
                hw_rx_ns: 0,
                flags: TsFlags::SW,
            },
        ),
        (
            "NET-120.2",
            buf([(0, 0), (0, 0), (S, 3)]),
            0,
            RxStamp {
                sw_rx_ns: 0,
                hw_rx_ns: hw,
                flags: TsFlags::HW,
            },
        ),
        (
            "NET-120.3",
            buf([(S, 5), (0, 0), (S, 3)]),
            0,
            RxStamp {
                sw_rx_ns: sw,
                hw_rx_ns: hw,
                flags: TsFlags::HW | TsFlags::SW,
            },
        ),
        ("NET-120.4", buf([(0, 0), (S, 9), (0, 0)]), 0, RxStamp::default()),
        ("NET-120.5", Vec::new(), 0, RxStamp::default()),
        (
            "NET-120.6",
            buf([(S, 5), (0, 0), (S, 3)]),
            MSG_CTRUNC,
            RxStamp::default(),
        ),
        ("NET-120.7", two_only, 0, RxStamp::default()),
    ];
    for (row, b, flags, want) in rows {
        assert_eq!(parse_timestamping(&b, flags), want, "{row}");
    }
}
