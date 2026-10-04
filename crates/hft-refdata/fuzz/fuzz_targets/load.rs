//! REF-080: any bytes → `RefData::load` (without and with a current snapshot) and
//! `EngineConfig::load` never panic; a successful load has `version() > 0`.
//! Corpus seeds: F0, E0 and the failing inputs of REF-005 – REF-009.
#![no_main]

#[path = "../../tests/common/f0.rs"]
mod f0;

use std::sync::OnceLock;

use hft_refdata::{EngineConfig, RefData};
use libfuzzer_sys::fuzz_target;

fn current() -> &'static RefData {
    static CUR: OnceLock<RefData> = OnceLock::new();
    CUR.get_or_init(|| RefData::load(&f0::f0(), None).unwrap_or_else(|r| panic!("F0: {:?}", r.errors)))
}

fn ref_080_fuzz_load(data: &[u8]) {
    if let Ok(rd) = RefData::load(data, None) {
        assert!(rd.version() > 0);
    }
    let _ = RefData::load(data, Some(current()));
    let _ = EngineConfig::load(data, None);
}

fuzz_target!(|data: &[u8]| ref_080_fuzz_load(data));
