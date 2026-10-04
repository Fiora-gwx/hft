// EVT-170.2: u8 followed by u64 without _pad (implicit padding)
use hft_event::{event_body, EventKind};

event_body! {
    pub struct Gap : fixed(EventKind(0x7E01), 1), FromBytes { pub a: u8, pub b: u64 }
}

fn main() {}
