//! Advice routing (TESTS.md "Advice 路由": SEQ-169, SEQ-188–SEQ-191).

use hft_event::{AdviceHdr, EventKind};
use hft_sequencer::driver::live::RingId;
use hft_types::{AccountId, GroupId, StrategyId, Tsc};
use zerocopy::IntoBytes;

use crate::common::*;

fn advice(s: StrategyId, a: AccountId, g: GroupId, len: usize, basis_seq: u64) -> Vec<u8> {
    let mut hd = <AdviceHdr as zerocopy::FromZeros>::new_zeroed();
    hd.strategy = s;
    hd.account = a;
    hd.local_kind = 7;
    hd.group = g;
    hd.len = len as u16;
    hd.basis_seq = basis_seq;
    hd.basis_tsc = Tsc(123);
    let mut v = hd.as_bytes().to_vec();
    v.extend(std::iter::repeat_n(0xA5u8, len.min(5_000)));
    v
}

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

const MISROUTED: u32 = 5;
const STALE: u32 = 6;

fn send(h: &mut H, rec: Vec<u8>) -> (Vec<Got>, Vec<hft_event::Note>) {
    clear_log();
    let m = h.mark();
    h.push_bytes(RingId::Advice, EventKind::ADVICE, rec);
    h.run();
    (
        log()
            .into_iter()
            .filter(|g| g.kind == "Advice" && !g.model)
            .collect(),
        h.notes_since(m),
    )
}

#[test]
fn seq_169_advice_input_routed() {
    let mut h = steady();
    let rec = advice(S2, AC2, G2, 16, 1_000_000);
    let m = h.mark();
    let (got, _) = send(&mut h, rec.clone());
    let logged = h
        .since(m)
        .into_iter()
        .find(|r| r.kind == EventKind::ADVICE)
        .unwrap();
    assert_eq!(logged.body, rec);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].s, S2);
    assert_eq!(got[0].gix, 0);
}

#[test]
fn seq_188_advice_routing_checks() {
    let mut h = steady();
    // Give g2 a non-zero spec_seq (fingerprint change by a SetParams).
    let mut p = Spb::s2(2);
    p.groups = vec![gspec_eth(G2)];
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.result_of(id).map(|r| (r.status, r.detail)), Some((0, 0)));
    let spec = h.s.instances().get(S2).unwrap().spec_seq_of(G2).unwrap();
    let code = |n: u32| hft_sequencer::codes::seq_note(n);
    let cases: Vec<(Vec<u8>, Option<u32>)> = vec![
        (
            advice(StrategyId(5), AccountId(0), GroupId(0), 8, spec),
            Some(MISROUTED),
        ),
        (advice(S2, AccountId(0), G2, 4_097, spec), Some(MISROUTED)),
        (advice(S2, AccountId(0), G2, 4_096, spec), None),
        (
            advice(S2, AccountId(0), hft_types::GROUP_ALL, 8, spec),
            Some(MISROUTED),
        ),
        (
            advice(S2, AccountId(0), GroupId(0x0100_0001), 8, spec),
            Some(MISROUTED),
        ),
        (advice(S2, AC1, G2, 8, spec), Some(MISROUTED)),
        (advice(S2, AccountId(0), G2, 8, spec), None),
        (
            advice(S2, AccountId(0), GroupId(0x0200_0009), 8, spec),
            Some(STALE),
        ),
        (
            advice(S2, AccountId(0), G2, 8, spec.saturating_sub(1)),
            Some(STALE),
        ),
        (advice(S2, AccountId(0), hft_types::GROUP_NONE, 8, 1), None),
    ];
    for (k, (rec, want)) in cases.into_iter().enumerate() {
        let (got, notes) = send(&mut h, rec);
        match want {
            None => {
                assert_eq!(got.len(), 1, "row {}", k + 1);
                assert!(got.iter().all(|g| g.s == S2));
            }
            Some(n) => {
                assert!(got.is_empty(), "row {}", k + 1);
                assert!(
                    notes.iter().any(|x| x.code == code(n)),
                    "row {}: {notes:?}",
                    k + 1
                );
            }
        }
    }
}

#[test]
fn seq_189_spec_seq_updates() {
    let mut h = steady();
    let s0 = h.s.instances().get(S2).unwrap().spec_seq_of(G2).unwrap();
    let mut p = Spb::s2(2);
    p.values[F_THR] = 9;
    let m = h.mark();
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0));
    assert_eq!(h.s.instances().get(S2).unwrap().spec_seq_of(G2), Some(s0));
    let mut p = Spb::s2(3);
    p.values[F_THR] = 9;
    let g2b = GroupId(0x0200_0002);
    p.groups = vec![gspec(G2), gspec_eth(g2b)];
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0));
    let cmd_seq = h
        .since(m)
        .into_iter()
        .rfind(|r| r.kind == EventKind::CONTROL_CMD)
        .unwrap()
        .seq;
    assert_eq!(h.s.instances().get(S2).unwrap().spec_seq_of(g2b), Some(cmd_seq));
    assert_eq!(h.s.instances().get(S2).unwrap().spec_seq_of(G2), Some(s0));
}

#[test]
fn seq_190_spec_seq_in_snapshot() {
    let mut h = steady();
    let kf = h.keyframe();
    let (_, secs) = snap(&kf);
    let (_, rows, _) = secs.strategies.iter().find(|x| x.0.strategy == S2).unwrap();
    let spec = h.s.instances().get(S2).unwrap().spec_seq_of(G2).unwrap();
    assert_eq!(
        rows.iter().find(|r| r.group == G2).map(|r| r.spec_seq),
        Some(spec)
    );
    for b in [spec.saturating_sub(1), spec.saturating_sub(1), spec + 5, spec + 6] {
        send(&mut h, advice(S2, AccountId(0), G2, 8, b));
    }
    let m = h.verify_from(Some(kf.seq));
    assert!(m.is_empty(), "{m:?}");
}

#[test]
fn seq_191_no_engine_staleness() {
    let mut h = steady();
    h.adv(60_000_000_000);
    let spec = h.s.instances().get(S2).unwrap().spec_seq_of(G2).unwrap();
    let (got, notes) = send(&mut h, advice(S2, AccountId(0), G2, 8, spec));
    assert_eq!(got.len(), 1);
    assert!(got[0].text.contains("basis_tsc: Tsc(123)"), "{}", got[0].text);
    let adv = [
        hft_sequencer::codes::seq_note(MISROUTED),
        hft_sequencer::codes::seq_note(STALE),
    ];
    assert!(notes.iter().all(|n| !adv.contains(&n.code)), "{notes:?}");
}
