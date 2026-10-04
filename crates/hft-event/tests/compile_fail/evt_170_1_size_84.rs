// EVT-170.1: body size not a multiple of 8
use hft_event::{event_body, EventKind};

event_body! {
    pub struct Odd : fixed(EventKind(0x7E01), 1), FromBytes { pub a: [u8; 84] }
}

fn main() {}
