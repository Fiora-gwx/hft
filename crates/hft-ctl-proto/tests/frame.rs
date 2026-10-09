//! CTP-002–CTP-004.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use hft_ctl_proto::{FrameErr, encode_frame, frame_len};
use proptest::prelude::*;

#[test]
fn ctp_002_frame_len_bounds() {
    let rows: [(&str, [u8; 4], Result<usize, FrameErr>); 7] = [
        ("CTP-002.1", [0, 0, 0, 0], Ok(0)),
        ("CTP-002.2", [0, 0, 0, 1], Ok(1)),
        ("CTP-002.3", [0, 0, 0x10, 0], Ok(4_096)),
        ("CTP-002.4", [0, 0x0F, 0xFF, 0xFF], Ok(1_048_575)),
        ("CTP-002.5", [0, 0x10, 0, 0], Ok(1_048_576)),
        ("CTP-002.6", [0, 0x10, 0, 1], Err(FrameErr::TooLong)),
        ("CTP-002.7", [0xFF, 0xFF, 0xFF, 0xFF], Err(FrameErr::TooLong)),
    ];
    for (row, hdr, want) in rows {
        assert_eq!(frame_len(&hdr), want, "{row}");
    }
}

#[test]
fn ctp_003_encode_frame_appends() {
    let mut out = vec![0x78];
    encode_frame(b"{}", &mut out);
    assert_eq!(out, [0x78, 0, 0, 0, 2, 0x7B, 0x7D]);
    encode_frame(b"", &mut out);
    assert_eq!(out, [0x78, 0, 0, 0, 2, 0x7B, 0x7D, 0, 0, 0, 0]);
}

/// Incremental reader of the test: header, then body, across arbitrary batches.
#[derive(Default)]
struct Reader {
    buf: Vec<u8>,
    out: Vec<Vec<u8>>,
}

impl Reader {
    fn feed(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
        loop {
            let Some(hdr) = self.buf.get(..4) else { return };
            let n = frame_len(&hdr.try_into().unwrap()).unwrap();
            if self.buf.len() < 4 + n {
                return;
            }
            self.out.push(self.buf[4..4 + n].to_vec());
            self.buf.drain(..4 + n);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]
    #[test]
    fn ctp_004_frame_roundtrip_any_split(
        frames in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..=65_536usize), 1..=8),
        cuts in proptest::collection::vec(any::<prop::sample::Index>(), 0..=16),
    ) {
        let mut wire = Vec::new();
        for f in &frames {
            let start = wire.len();
            encode_frame(f, &mut wire);
            let hdr: [u8; 4] = wire[start..start + 4].try_into().unwrap();
            prop_assert_eq!(frame_len(&hdr), Ok(f.len()));
            prop_assert_eq!(&wire[start + 4..], &f[..]);
        }
        let mut pts: Vec<usize> = cuts.iter().map(|c| c.index(wire.len() + 1)).collect();
        pts.push(0);
        pts.push(wire.len());
        pts.sort_unstable();
        let mut r = Reader::default();
        for w in pts.windows(2) {
            r.feed(&wire[w[0]..w[1]]);
        }
        prop_assert_eq!(r.out, frames);
    }
}
