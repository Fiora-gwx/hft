// EVT-170.3: KINDS registers Bbo as Fixed{size: 96}; this body is 88 B
use hft_event::{event_body, EventKind};

event_body! {
    pub struct NotBbo : fixed(EventKind::BBO, 1), FromBytes { pub a: [u8; 88] }
}

fn main() {}
