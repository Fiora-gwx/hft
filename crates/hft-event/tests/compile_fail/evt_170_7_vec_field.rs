// EVT-170.7: no Vec or reference fields
use hft_event::{event_body, EventKind};

event_body! {
    pub struct V : fixed(EventKind(0x7E05), 1), FromBytes { pub v: Vec<u8> }
}

fn main() {}
