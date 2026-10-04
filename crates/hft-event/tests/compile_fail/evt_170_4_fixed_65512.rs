// EVT-170.4: fixed body above 65 536 - 24 - 8
use hft_event::{event_body, EventKind};

event_body! {
    pub struct Huge : fixed(EventKind(0x7E02), 1), FromBytes { pub a: [u8; 65512] }
}

fn main() {}
