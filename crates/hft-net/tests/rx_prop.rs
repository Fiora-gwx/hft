//! NET-092, NET-136: any frame sequence under any byte split decodes to the same frames, and each
//! frame's stamps follow the read-stamp model (README §7).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::support;

use hft_net::Opcode;
use hft_net::sock::RecvStep;
use proptest::prelude::*;
use support::mempeer::{cfg, pair_cfg};
use support::stamp_model::{self, Read};
use support::ws_ref::server_frame;
use support::xorshift::XorShift64Star;

struct Rng(XorShift64Star);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(XorShift64Star(seed | 1))
    }
    fn below(&mut self, n: u64) -> u64 {
        let hi = self.0.next_u32() as u64;
        let lo = self.0.next_u32() as u64;
        ((hi << 32) | lo) % n.max(1)
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo + 1)
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
    /// Payload size in 0..=20 000 (mostly small).
    fn size(&mut self) -> usize {
        match self.below(100) {
            0..=79 => self.range(0, 300) as usize,
            80..=94 => self.range(300, 4000) as usize,
            _ => self.range(4000, 20_000) as usize,
        }
    }
}

/// One server frame in the plaintext stream.
#[derive(Clone, Debug)]
struct Fr {
    start: usize,
    end: usize,
    ping: bool,
    /// Index of the message this data frame belongs to.
    msg: Option<usize>,
}

/// A generated stream: plaintext, frames, expected messages `(opcode, payload)` and record sizes.
struct Stream {
    plain: Vec<u8>,
    frames: Vec<Fr>,
    msgs: Vec<(Opcode, Vec<u8>)>,
    rec_sizes: Vec<usize>,
}

fn gen_stream(r: &mut Rng, n: usize) -> Stream {
    let mut plain = Vec::new();
    let mut frames = Vec::new();
    let mut msgs = Vec::new();
    let push = |plain: &mut Vec<u8>, frames: &mut Vec<Fr>, f: Vec<u8>, ping: bool, msg: Option<usize>| {
        let start = plain.len();
        plain.extend_from_slice(&f);
        frames.push(Fr {
            start,
            end: plain.len(),
            ping,
            msg,
        });
    };
    for _ in 0..n {
        if r.chance(8) {
            let len = r.range(0, 125) as usize;
            let p: Vec<u8> = (0..len).map(|_| r.below(256) as u8).collect();
            push(&mut plain, &mut frames, server_frame(true, 0x9, &p), true, None);
            continue;
        }
        let op = if r.chance(50) {
            Opcode::Text
        } else {
            Opcode::Binary
        };
        let len = r.size();
        let p: Vec<u8> = (0..len).map(|_| r.below(256) as u8).collect();
        let m = msgs.len();
        if r.chance(10) && len >= 2 {
            let parts = r.range(2, 4.min(len as u64)) as usize;
            let mut cuts: Vec<usize> = (0..parts - 1)
                .map(|_| r.range(1, len as u64 - 1) as usize)
                .collect();
            cuts.sort();
            cuts.dedup();
            let mut bounds = vec![0];
            bounds.extend(cuts);
            bounds.push(len);
            let k = bounds.len() - 1;
            for i in 0..k {
                let fin = i + 1 == k;
                let code = if i == 0 { op as u8 } else { 0 };
                push(
                    &mut plain,
                    &mut frames,
                    server_frame(fin, code, &p[bounds[i]..bounds[i + 1]]),
                    false,
                    Some(m),
                );
                if !fin && r.chance(30) {
                    push(
                        &mut plain,
                        &mut frames,
                        server_frame(true, 0x9, b"mid"),
                        true,
                        None,
                    );
                }
            }
        } else {
            push(
                &mut plain,
                &mut frames,
                server_frame(true, op as u8, &p),
                false,
                Some(m),
            );
        }
        msgs.push((op, p));
    }
    let mut rec_sizes = Vec::new();
    let mut left = plain.len();
    while left > 0 {
        let s = (r.range(1, 16_384) as usize).min(left);
        rec_sizes.push(s);
        left -= s;
    }
    Stream {
        plain,
        frames,
        msgs,
        rec_sizes,
    }
}

/// Record index holding plaintext offset `off`, and each record's ciphertext range.
fn record_map(sizes: &[usize]) -> (Vec<usize>, Vec<(u64, u64)>) {
    let mut pstarts = Vec::new();
    let mut cr = Vec::new();
    let (mut p, mut c) = (0usize, 0u64);
    for &s in sizes {
        pstarts.push(p);
        cr.push((c, c + s as u64 + 22));
        p += s;
        c += s as u64 + 22;
    }
    (pstarts, cr)
}

fn rec_of(pstarts: &[usize], off: usize) -> usize {
    match pstarts.binary_search(&off) {
        Ok(i) => i,
        Err(i) => i - 1,
    }
}

/// Pongs one round sends for `p` Pings (queue of 4; a 5th clears it).
fn pongs_for(p: usize) -> usize {
    if p == 0 { 0 } else { (p - 1) % 4 + 1 }
}

struct Out {
    msgs: Vec<(Opcode, Vec<u8>, hft_net::Timestamps)>,
    reads: Vec<Read>,
    pong_ok: bool,
}

/// Runs a stream through a connection with the given read plan (`None` = EAGAIN).
fn run(st: &Stream, mss: u32, plan: &mut dyn FnMut(usize) -> Option<usize>, r: &mut Rng) -> Out {
    let mut c = cfg();
    c.rx_buf = 65_536;
    c.max_msg = 65_536;
    let (mut conn, mut peer) = pair_cfg(c);
    conn.test_set_mss(mss);
    let cipher = peer.split(&st.plain, &st.rec_sizes);
    let (pstarts, crs) = record_map(&st.rec_sizes);
    assert_eq!(cipher.len() as u64, crs.last().map_or(0, |x| x.1));
    // Frame → the record holding its last byte (its completion).
    let done_rec: Vec<usize> = st.frames.iter().map(|f| rec_of(&pstarts, f.end - 1)).collect();
    let mut off = 0usize;
    let mut t = 1_000u64;
    let mut hw = 1_000_000i64;
    let mut reads = Vec::new();
    let mut msgs = Vec::new();
    let mut pong_ok = true;
    let mut pinged = 0usize;
    for _ in 0..1_000_000 {
        if off >= cipher.len() && conn.script().unwrap().recv.is_empty() {
            break;
        }
        if conn.script().unwrap().recv.is_empty() {
            let pre = t + r.range(1, 1000);
            let post = pre + r.range(1, 500);
            t = post;
            hw += r.range(1, 1000) as i64;
            match plan(cipher.len() - off) {
                None => conn.script().unwrap().push_recv(RecvStep::eagain(pre)),
                Some(n) => {
                    let n = n.min(cipher.len() - off);
                    conn.script().unwrap().push_recv(RecvStep::stamped(
                        pre,
                        post,
                        cipher[off..off + n].to_vec(),
                        hw,
                        hw + 7,
                    ));
                    off += n;
                }
            }
        }
        let step = conn.script().unwrap().recv.front().cloned().unwrap();
        let got = conn.poll_rx(0) as u64;
        match step.kind {
            hft_net::sock::RecvKind::Eagain => reads.push(Read {
                bytes: 0,
                pre: step.pre_tsc,
                post: step.post_tsc,
                hw: 0,
                sw: 0,
            }),
            hft_net::sock::RecvKind::Data { hw_ns, sw_ns, .. } => reads.push(Read {
                bytes: got,
                pre: step.pre_tsc,
                post: step.post_tsc,
                hw: hw_ns,
                sw: sw_ns,
            }),
            _ => unreachable!(),
        }
        while let Some(f) = conn.next_frame() {
            msgs.push((f.opcode, f.payload.to_vec(), f.ts));
        }
        conn.finish_poll(0);
        // Pings completed by the bytes received so far.
        let received: u64 = reads.iter().map(|x| x.bytes).sum();
        let done_now = st
            .frames
            .iter()
            .enumerate()
            .filter(|(i, f)| f.ping && crs[done_rec[*i]].1 <= received)
            .count();
        let want = pongs_for(done_now - pinged);
        pinged = done_now;
        peer.pump(&mut conn);
        let got = peer
            .client_frames()
            .into_iter()
            .filter(|f| f.opcode == 0xA)
            .count();
        pong_ok &= got == want;
        assert!(conn.err().is_empty(), "{:?}", conn.err());
    }
    Out { msgs, reads, pong_ok }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]
    #[test]
    fn net_092_any_split_same_frames(seed in any::<u64>(), n in 1usize..=200) {
        let mut r = Rng::new(seed);
        let st = gen_stream(&mut r, n);
        let mut r2 = Rng::new(seed ^ 0xA5A5);
        let mut plan = |left: usize| Some(r2.range(1, left as u64) as usize);
        let mut r3 = Rng::new(seed ^ 0x5A5A);
        let out = run(&st, 1448, &mut plan, &mut r3);
        let got: Vec<(Opcode, Vec<u8>)> = out.msgs.into_iter().map(|(o, p, _)| (o, p)).collect();
        prop_assert_eq!(got, st.msgs.clone());
        prop_assert!(out.pong_ok, "Pongs per round");
    }

    #[test]
    fn net_136_stamp_source_model(seed in any::<u64>(), n in 1usize..=200, mss_i in 0usize..3) {
        let mss = [536u32, 1448, 8948][mss_i];
        let mut r = Rng::new(seed);
        let st = gen_stream(&mut r, n);
        let mut r2 = Rng::new(seed ^ 0x1234);
        let mut plan = |_left: usize| if r2.chance(10) { None } else { Some(r2.range(1, 4000) as usize) };
        let mut r3 = Rng::new(seed ^ 0x4321);
        let out = run(&st, mss, &mut plan, &mut r3);
        let (pstarts, crs) = record_map(&st.rec_sizes);
        let rs = stamp_model::records(&out.reads, &crs, mss as u64);
        // Expected stamps per message: first frame's first record, last frame's last record.
        let mut first: Vec<Option<usize>> = vec![None; st.msgs.len()];
        let mut last = vec![0usize; st.msgs.len()];
        for f in &st.frames {
            if let Some(m) = f.msg {
                first[m].get_or_insert(rec_of(&pstarts, f.start));
                last[m] = rec_of(&pstarts, f.end - 1);
            }
        }
        prop_assert_eq!(out.msgs.len(), st.msgs.len());
        for (m, (_, _, ts)) in out.msgs.iter().enumerate() {
            let want = stamp_model::frame(&rs, first[m].unwrap(), last[m]);
            prop_assert_eq!(*ts, want, "message {}", m);
        }
    }
}
