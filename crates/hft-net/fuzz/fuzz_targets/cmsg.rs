//! NET-337: any bytes and any `msg_flags` as the `recvmsg` control buffer: the parser never panics,
//! never reads out of bounds, and reports only the `HW`/`SW` flags.
#![no_main]

use hft_net::TsFlags;
use hft_net::cmsg::parse_timestamping;
use libfuzzer_sys::fuzz_target;

fn net_337_fuzz_cmsg(data: &[u8]) {
    let (flags, buf) = match data {
        [a, b, c, d, rest @ ..] => (i32::from_ne_bytes([*a, *b, *c, *d]), rest),
        _ => (0, data),
    };
    let s = parse_timestamping(buf, flags);
    assert_eq!(s.flags & !(TsFlags::HW | TsFlags::SW), 0);
    assert_eq!(s.flags & TsFlags::HW != 0, s.hw_rx_ns != 0);
    assert_eq!(s.flags & TsFlags::SW != 0, s.sw_rx_ns != 0);
}

fuzz_target!(|data: &[u8]| net_337_fuzz_cmsg(data));
