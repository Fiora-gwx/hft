// SDK-057: match ExecUpdateKind without a wildcard arm
use hft_strategy_sdk::ExecUpdateKind;

fn kind(k: ExecUpdateKind) -> u8 {
    match k {
        ExecUpdateKind::Progress { .. } => 0,
        ExecUpdateKind::Done { .. } => 1,
        ExecUpdateKind::Stuck(_) => 2,
        ExecUpdateKind::Broken(_) => 3,
    }
}

fn main() {
    let _ = kind;
}
