//! `strategy_set!` rows gated by `#[cfg]` (SDK-188). CI runs this file twice: without and with
//! `--features probe-target`.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_strategy_sdk::StrategySet;

hft_strategy_sdk::strategy_set! {
    pub struct CfgSet {
        EchoProbe,
        #[cfg(feature = "probe-target")] TargetProbe,
    }
}

#[test]
fn sdk_188_cfg_gated_rows() {
    let k: Vec<u16> = CfgSet::kinds().iter().map(|d| d.kind_id).collect();
    if cfg!(feature = "probe-target") {
        assert_eq!(k, vec![0xFF01, 0xFF02], "step 2: probe-target on");
    } else {
        assert_eq!(k, vec![0xFF01], "step 1: probe-target off");
    }
}
