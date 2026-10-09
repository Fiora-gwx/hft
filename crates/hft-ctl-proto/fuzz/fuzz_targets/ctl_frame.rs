//! CTP-170: any byte stream → frames (`frame_len` + body) → schema parse and error rendering.
#![no_main]

use hft_ctl_proto::{CmdResponse, EngineInfo, TransportStatus, Uuid, frame_len, parse_request};
use libfuzzer_sys::fuzz_target;

fn ctp_170_ctl_frame(mut data: &[u8]) {
    while let Some((hdr, rest)) = data.split_first_chunk::<4>() {
        let Ok(n) = frame_len(hdr) else { return };
        let Some(body) = rest.get(..n) else { return };
        if let Err(e) = parse_request(body) {
            let mut r = CmdResponse::new(
                e.cmd_id.unwrap_or(Uuid::nil()),
                TransportStatus::InvalidRequest,
                None,
                EngineInfo::default(),
            );
            r.detail = e.detail.map(hft_ctl_proto::codes::Detail::code);
            let _ = r.to_json();
        }
        data = &rest[n..];
    }
}

fuzz_target!(|data: &[u8]| ctp_170_ctl_frame(data));
