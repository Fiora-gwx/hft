// EVT-170.5: Var prefix 80 + max 65 425 > 65 504
use hft_event::{event_body, EventKind};

event_body! {
    pub struct P : var(EventKind(0x7E03), 1, 65_425), FromBytes { pub a: [u8; 80] }
}

fn main() {}
