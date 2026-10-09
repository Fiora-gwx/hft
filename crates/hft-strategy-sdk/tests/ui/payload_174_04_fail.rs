// SDK-174.4: LOCAL_KIND = 0xFFFF
#[path = "support/mod.rs"]
mod support;
use support::*;

struct P;

impl AdvicePayload for P {
    const LOCAL_KIND: u16 = 0xFFFF;
    const MAX_LEN: usize = 16;
    fn encode(&self, w: &mut PayloadWriter<'_>) -> Result<(), PayloadFull> {
        w.put_u8(1)
    }
    fn decode(_: &[u8]) -> Result<Self, PayloadError> {
        Ok(P)
    }
}

fn main() {
    let mut b = AdviceBufs::new();
    b.begin();
    let mut out = b.out();
    let _ = out.emit(GroupId(1), &P);
}
