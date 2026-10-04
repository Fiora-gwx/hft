//! ByteSpsc matches a byte-occupancy model; an empty ring always accepts (RT-035).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

use std::collections::VecDeque;

use hft_rt::{Backing, byte_spsc};
use proptest::prelude::*;

#[derive(Clone, Debug)]
enum Op {
    Reserve(u32),
    Commit,
    Peek,
    Advance,
}

/// Reference: records plus the byte-position model (8 B header, 8 B alignment, wrap marker).
struct Model {
    cap: u64,
    head: u64,
    tail: u64,
    recs: VecDeque<(Vec<u8>, u64)>, // payload, bytes it occupies (incl. a wrap gap before it)
    pending: Option<(Vec<u8>, u64)>,
}

impl Model {
    fn reserve(&mut self, len: u32) -> bool {
        self.pending = None;
        if len as u64 > self.cap / 4 {
            return false;
        }
        let need = 8 + (len as u64).div_ceil(8) * 8;
        let pos = self.head % self.cap;
        let room = self.cap - pos;
        let adv = if need <= room { need } else { room + need };
        if self.head - self.tail + adv > self.cap {
            return false;
        }
        self.pending = Some((Vec::new(), adv));
        true
    }
}

fn op(cap: u32) -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..=cap / 4).prop_map(Op::Reserve),
        Just(Op::Commit),
        Just(Op::Peek),
        Just(Op::Advance),
    ]
}

fn case() -> impl Strategy<Value = (u32, Vec<Op>)> {
    prop::sample::select(vec![64u32, 256, 4096])
        .prop_flat_map(|cap| (Just(cap), prop::collection::vec(op(cap), 1..5000)))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]
    #[test]
    fn rt_035_bytes_match_model((cap, ops) in case()) {
        let (mut p, mut c) = byte_spsc(cap as usize, Backing::Heap).unwrap();
        let mut m = Model { cap: cap as u64, head: 0, tail: 0, recs: VecDeque::new(), pending: None };
        let mut seq = 0u8;
        for o in ops {
            match o {
                Op::Reserve(len) => {
                    let empty = m.recs.is_empty();
                    let want = m.reserve(len);
                    let got = p.reserve(len);
                    prop_assert_eq!(got.is_some(), want);
                    if empty {
                        prop_assert!(want, "empty ring must accept len {}", len);
                    }
                    if let Some(buf) = got {
                        prop_assert_eq!(buf.len(), len as usize);
                        for b in buf.iter_mut() {
                            seq = seq.wrapping_add(1);
                            *b = seq;
                        }
                        let payload = buf.to_vec();
                        if let Some(pd) = m.pending.as_mut() {
                            pd.0 = payload;
                        }
                    }
                }
                Op::Commit => {
                    p.commit();
                    if let Some((payload, adv)) = m.pending.take() {
                        m.head += adv;
                        m.recs.push_back((payload, adv));
                    }
                }
                Op::Peek => {
                    let got = c.peek().map(|s| s.to_vec());
                    prop_assert_eq!(got, m.recs.front().map(|r| r.0.clone()));
                }
                Op::Advance => {
                    c.advance();
                    if let Some((_, adv)) = m.recs.pop_front() {
                        m.tail += adv;
                    }
                }
            }
        }
    }
}
