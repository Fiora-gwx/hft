// SDK-187.2: two kinds with kind_id 0xFF01
#[path = "support/mod.rs"]
mod support;
use support::*;

ui_strategy!(EchoProbe, P1, desc(0xFF01, "echo_probe", &S1));
ui_strategy!(TargetProbe, P2, desc(0xFF01, "target_probe", &S2));

hft_strategy_sdk::strategy_set! {
    pub struct ProbeSet {
        EchoProbe,
        TargetProbe,
    }
}

fn main() {
    let _ = ProbeSet::KINDS;
}
