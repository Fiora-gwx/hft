//! Warmup injection (SDK-181, SDK-182, SDK-184).

use hft_event::{CmdHdr, ControlCmd, EventKind};
use hft_sequencer::codes::status;
use hft_types::{AccountId, StrategyId};

use super::*;
use crate::common::*;

fn warm(instance: StrategyId, chunk_no: u32, last: bool, payload: &[u8]) -> ControlCmd<'_> {
    ControlCmd::InjectWarmup {
        instance,
        last: u8::from(last),
        local_kind: 1,
        chunk_no,
        len: payload.len() as u16,
        _pad: [0; 6],
        payload,
    }
}

/// Model deliveries of s1 as `(kind, seq, text)`.
fn model_calls_log() -> Vec<(&'static str, u64, String)> {
    model(S1)
        .into_iter()
        .map(|g| (g.kind, g.now.seq, g.text))
        .collect()
}

#[test]
fn sdk_181_warmup_via_journal() {
    let mut h = rig();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    let mut seqs = Vec::new();
    for k in 0..3u32 {
        let payload = vec![k as u8 + 1; 40];
        let m = h.mark();
        let id = h.cmd(AccountId(0), &warm(S1, k, k == 2, &payload));
        h.run();
        assert_eq!(h.res(id).0, status::APPLIED);
        seqs.push(
            h.kinds_since(m, EventKind::CONTROL_CMD)
                .last()
                .map(|r| r.seq)
                .expect("cmd"),
        );
    }
    clear_log();
    catch_up(&mut a, &h);
    let w: Vec<Got> = model(S1).into_iter().filter(|g| g.kind == "Warmup").collect();
    assert_eq!(w.len(), 3);
    for (k, g) in w.iter().enumerate() {
        assert_eq!(g.now.seq, seqs[k]);
        assert!(g.text.contains(&format!("chunk_no: {k}")));
        assert!(g.text.contains(&format!("last: {}", k == 2)));
    }
    let run1 = model_calls_log();
    let ck1 = checkpoint_now(&mut a);
    // A second advisor replays the same journal from the start (no checkpoint).
    clear_log();
    let mut a2 = adv(&h, MemCkpt::default());
    catch_up(&mut a2, &h);
    let run2 = model_calls_log();
    let ck2 = checkpoint_now(&mut a2);
    assert_eq!(run1, run2);
    assert_eq!(
        ck1.gens.get(&S1).map(|g| g.1.clone()),
        ck2.gens.get(&S1).map(|g| g.1.clone())
    );
}

#[test]
fn sdk_182_inject_warmup_checks() {
    let mut h = rig();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    // .1 4 096 B.
    let big = vec![7u8; 4_096];
    let id = h.cmd(AccountId(0), &warm(S1, 0, false, &big));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED, "SDK-182.1");
    // .2 4 097 B: the command does not encode (nothing reaches the ctl ring).
    let over = vec![7u8; 4_097];
    let mut hd = CmdHdr::zeroed();
    hd.cmd_ver = 1;
    let mut buf = vec![0u8; 8_192];
    assert!(
        warm(S1, 1, false, &over).encode(&hd, &mut buf).is_err(),
        "SDK-182.2"
    );
    // .3 an instance that does not exist.
    let id = h.cmd(AccountId(0), &warm(StrategyId(7), 0, true, &[1]));
    h.run();
    assert_eq!(h.res(id).0, status::UNKNOWN_TARGET, "SDK-182.3");
    // .4 chunk 5 without 0–4 before it.
    let id = h.cmd(AccountId(0), &warm(S1, 5, true, &[1, 2, 3]));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED, "SDK-182.4");
    clear_log();
    catch_up(&mut a, &h);
    let w: Vec<Got> = model(S1).into_iter().filter(|g| g.kind == "Warmup").collect();
    let chunks: Vec<bool> = [0u32, 5]
        .iter()
        .map(|k| w.iter().any(|g| g.text.contains(&format!("chunk_no: {k},"))))
        .collect();
    assert_eq!(chunks, vec![true, true]);
    assert_eq!(w.len(), 2, "no delivery for the unknown instance");
}

#[test]
fn sdk_184_warmup_no_intents() {
    let b = b_gt3();
    set_warmup_both(S1);
    let mut h = b.boot();
    h.steady();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    clear_log();
    let m = h.mark();
    let id = h.cmd(AccountId(0), &warm(S1, 0, true, &[9; 48]));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    assert!(fast(S1).iter().any(|g| g.kind == "Warmup"));
    let recs = h.since(m);
    assert!(decs(&recs).is_empty());
    assert!(execs(&recs).is_empty());
    catch_up(&mut a, &h);
    assert!(model(S1).iter().any(|g| g.kind == "Warmup"));
}
