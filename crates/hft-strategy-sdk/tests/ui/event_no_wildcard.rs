// SDK-057: match EventRef without a wildcard arm
use hft_strategy_sdk::EventRef;

fn kind(e: &EventRef<'_>) -> u8 {
    match e {
        EventRef::Bbo { .. } => 0,
        EventRef::Book { .. } => 1,
        EventRef::Trade { .. } => 2,
        EventRef::Funding { .. } => 3,
        EventRef::Mark { .. } => 4,
        EventRef::Index { .. } => 5,
        EventRef::Liquidation { .. } => 6,
        EventRef::Order { .. } => 7,
        EventRef::Exec { .. } => 8,
        EventRef::External { .. } => 9,
        EventRef::Risk { .. } => 10,
        EventRef::Notice { .. } => 11,
        EventRef::Timer { .. } => 12,
        EventRef::Advice { .. } => 13,
        EventRef::Mode { .. } => 14,
        EventRef::Conn { .. } => 15,
        EventRef::Manual { .. } => 16,
        EventRef::Warmup { .. } => 17,
    }
}

fn main() {
    let _ = kind;
}
