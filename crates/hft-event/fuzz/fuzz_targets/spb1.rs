//! EVT-203: any bytes → `parse_spb1`; success implies `48 + 80 × n_groups + body_len == len`.
#![no_main]

use hft_event::*;
use libfuzzer_sys::fuzz_target;

fn evt_203_fuzz_spb1(data: &[u8]) {
    if let Ok(v) = parse_spb1(data) {
        assert_eq!(48 + 80 * usize::from(v.hdr.n_groups) + v.hdr.body_len as usize, data.len());
    }
}

fuzz_target!(|data: &[u8]| evt_203_fuzz_spb1(data));
