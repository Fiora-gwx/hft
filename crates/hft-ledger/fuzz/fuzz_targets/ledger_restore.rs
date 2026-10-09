//! LED-238: any bytes as sections 1, 13 and 22 → `Ledger::restore` never panics; an accepted
//! input writes back byte for byte. Corpus: LED-230 snapshots and fixture snapshot sections.
#![no_main]
#![allow(dead_code)]

use hft_ledger::Ledger;
use hft_ledger::snapshot::{SECTION_LEDGER_GROUPS, SECTION_LEDGER_INVENTORY, SECTION_POSITIONS};
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
use libfuzzer_sys::fuzz_target;

#[path = "../../tests/common/mod.rs"]
mod common;

fn led_238_restore_fuzz(data: &[u8]) {
    let mut r = SnapshotReader::new(data);
    let Ok(l) = Ledger::restore(&mut r, common::rd_lf(), &common::cfg_lf(), &common::caps_lf()) else {
        return;
    };
    let mut w = SnapshotWriter::new(64 << 20);
    l.snapshot(&mut w);
    let out = SnapshotReader::new(w.as_bytes());
    let inp = SnapshotReader::new(data);
    for sec in [SECTION_POSITIONS, SECTION_LEDGER_GROUPS, SECTION_LEDGER_INVENTORY] {
        let (Ok(a), Ok(b)) = (inp.find_section(sec), out.find_section(sec)) else {
            panic!("framing of section {sec}");
        };
        match (a, b) {
            (Some(a), Some(b)) => assert!(a.count == b.count && a.body == b.body, "section {sec} differs"),
            (None, Some(b)) => assert_eq!(b.count, 0, "section {sec} invented rows"),
            _ => panic!("section {sec} missing on write-back"),
        }
    }
}

fuzz_target!(|data: &[u8]| led_238_restore_fuzz(data));
