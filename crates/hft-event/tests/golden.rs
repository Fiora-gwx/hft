//! Canonical records against the golden files (EVT-173). `HFT_REGEN_GOLDEN=1` rewrites
//! `fixtures/hft-event/EVT-173/` (only together with a layout change, see TESTS.md 验收).

mod common;

use common::canonical::{file_name, records};
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/hft-event/EVT-173")
}

#[test]
fn evt_173_canonical_records() {
    let recs = records();
    assert_eq!(recs.len(), 39);
    let regen = std::env::var_os("HFT_REGEN_GOLDEN").is_some();
    if regen {
        std::fs::create_dir_all(dir()).unwrap();
    }
    for (k, bytes) in recs {
        let path = dir().join(file_name(k));
        if regen {
            std::fs::write(&path, &bytes).unwrap();
        }
        let want = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(bytes, want, "{}", path.display());
        hft_event::decode_record(&bytes).unwrap();
    }
}
