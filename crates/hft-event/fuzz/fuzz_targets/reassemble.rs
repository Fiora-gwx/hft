//! EVT-202: bytes cut into records → `decode_record` → `Reassembler::push` (1 MiB buffer); complete
//! logical bodies go to `multi_view` and `ControlCmd::decode`.
#![no_main]

use hft_event::*;
use libfuzzer_sys::fuzz_target;

fn evt_202_fuzz_reassemble(data: &[u8]) {
    let mut buf = vec![0u8; 1 << 20];
    let mut r = Reassembler::new(&mut buf);
    let mut off = 0;
    while off < data.len() {
        let Ok(v) = decode_record(&data[off..]) else { break };
        off += v.hdr.len as usize;
        if let Ok(Some(l)) = r.push(&v) {
            let _ = multi_view::<BlobHdr>(l);
            let _ = multi_view::<SnapshotHdr>(l);
            let _ = multi_view::<L2SnapHdr>(l);
            let _ = ControlCmd::decode(l);
        }
    }
}

fuzz_target!(|data: &[u8]| evt_202_fuzz_reassemble(data));
