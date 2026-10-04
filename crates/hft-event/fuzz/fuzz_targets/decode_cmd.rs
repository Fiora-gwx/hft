//! EVT-201: any bytes as a logical body → `ControlCmd::decode`; a decoded command at its current
//! `cmd_ver` re-encodes to the same `128 + payload_len` bytes.
#![no_main]

use hft_event::*;
use libfuzzer_sys::fuzz_target;

fn evt_201_fuzz_decode_cmd(data: &[u8]) {
    let Ok((h, c)) = ControlCmd::decode(data) else { return };
    if h.cmd_ver != c.version() {
        return;
    }
    let mut out = vec![0u8; data.len() + 8];
    let n = c.encode(&h, &mut out).expect("re-encode");
    assert_eq!(n, 128 + h.payload_len as usize);
    assert_eq!(&out[..n], &data[..n], "round trip differs");
}

fuzz_target!(|data: &[u8]| evt_201_fuzz_decode_cmd(data));
