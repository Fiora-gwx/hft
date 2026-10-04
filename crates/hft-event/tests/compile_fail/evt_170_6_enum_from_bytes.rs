// EVT-170.6: a body with a Side field must use TryFromBytes
use hft_event::{event_body, EventKind};
use hft_types::Side;

event_body! {
    pub struct S : fixed(EventKind(0x7E04), 1), FromBytes { pub side: Side, pub _pad: [u8; 7] }
}

fn main() {}
