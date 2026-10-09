// SDK-174.1: LOCAL_KIND = 1, MAX_LEN = 4_096
#[path = "support/mod.rs"]
mod support;
use support::*;

struct P;

impl AdvicePayload for P {
    const LOCAL_KIND: u16 = 1;
    const MAX_LEN: usize = 4_096;
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
