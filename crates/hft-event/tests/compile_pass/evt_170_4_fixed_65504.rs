// EVT-170.4: fixed body at the 65 504 B limit
use hft_event::{event_body, EventKind};

event_body! {
    pub struct Max : fixed(EventKind(0x7E02), 1), FromBytes { pub a: [u8; 65504] }
}

fn main() {}
