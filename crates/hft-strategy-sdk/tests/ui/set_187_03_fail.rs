// SDK-187.3: two kinds with the same name
#[path = "support/mod.rs"]
mod support;
use support::*;

ui_strategy!(EchoProbe, P1, desc(0xFF01, "echo_probe", &S1));
ui_strategy!(TargetProbe, P2, desc(0xFF02, "echo_probe", &S2));

hft_strategy_sdk::strategy_set! {
    pub struct ProbeSet {
        EchoProbe,
        TargetProbe,
    }
}

fn main() {
    let _ = ProbeSet::KINDS;
}
