//! Model checkpoints (SDK-065–SDK-067, SDK-069, SDK-074).

use hft_event::{EventKind, Note, NoteSource};
use hft_sequencer::advisor::{Advisor, ckpt_checksum};
use hft_sequencer::codes::{advisor_note, status};
use hft_sequencer::driver::live::RingId;
use hft_sequencer::{SnapshotSel, StepOutcome};
use hft_strategy_sdk::{EvMask, EventRef, Part};
use hft_types::{AccountId, GroupId, StrategyId};

use super::*;
use crate::common::*;

/// A Meta Note of the advisor (pending or on the ring) of s1 with code `n`.
fn adv_notes(a_notes: &[Note], ring: &Ring, n: u32) -> Vec<Note> {
    a_notes
        .iter()
        .copied()
        .chain(ring.notes())
        .filter(|x| x.source == NoteSource::Advisor && x.code == hft_event::note_code(NoteSource::Advisor, n))
        .filter(|x| x.strategy == S1)
        .collect()
}

/// The `ModelCheckpointDiscarded` reason of s1 after a start (`None` = none written).
fn discard_reason<T: hft_strategy_sdk::StrategySet>(a: &Advisor<T, MemCkpt, Ring>) -> Option<(u64, u64)> {
    adv_notes(&a.notes, &a.ring, advisor_note::MODEL_CHECKPOINT_DISCARDED)
        .first()
        .map(|n| (n.v[0], n.v[1]))
}

fn path_reset<T: hft_strategy_sdk::StrategySet>(a: &Advisor<T, MemCkpt, Ring>) -> Option<[u64; 8]> {
    adv_notes(&a.notes, &a.ring, advisor_note::MODEL_PATH_RESET)
        .first()
        .map(|n| n.v)
}

/// The s1 Model calls recorded since `from`.
fn model_calls(from: usize) -> Vec<&'static str> {
    calls_of(S1, from)
        .into_iter()
        .filter(|c| c.starts_with("model") || *c == "create_model")
        .collect()
}

/// `SeqRig` + `AdvRig` with a checkpoint of s1's Model: s1 runs version 7 (`thr = 7`), its Model
/// gets a1 Bbos; the checkpoint (`last_seq`) is taken after a few Bbos, more Bbos follow.
struct Ck {
    h: H,
    store: MemCkpt,
    last: u64,
}

fn ck_script() {
    set_subs(S1, vec![(A1, 0, EvMask::BBO, Part::Model)]);
    SCRIPT.with(|x| x.borrow_mut().path_version = 1);
}

fn ck() -> Ck {
    let b = b_gt3();
    ck_script();
    let mut h = b.boot();
    h.steady();
    let mut p = gt3(7);
    p.values[F_THR] = 7;
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    for k in 0..3 {
        h.quote(A1, "50000", &format!("{}", 50_002 + k));
        h.run();
    }
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    let store = checkpoint_now(&mut a);
    let last = store
        .gens
        .get(&S1)
        .map(|g| g.0.last_seq)
        .expect("checkpoint of s1");
    for k in 0..3 {
        h.quote(A1, "50000", &format!("{}", 50_010 + k));
        h.run();
    }
    Ck { h, store, last }
}

fn store_of(c: &Ck, f: impl FnOnce(&mut MemCkpt)) -> MemCkpt {
    let mut s = MemCkpt::default();
    s.gens = c.store.gens.clone();
    f(&mut s);
    s
}

fn hdr_edit(c: &Ck, f: impl FnOnce(&mut hft_sequencer::advisor::CkptHdr, &mut Vec<u8>)) -> MemCkpt {
    store_of(c, |s| {
        if let Some(g) = s.gens.get_mut(&S1) {
            f(&mut g.0, &mut g.1);
        }
    })
}

/// Restarts the advisor with `store` and catches up; returns it and the call mark.
fn restart(c: &Ck, store: MemCkpt) -> (Adv, usize) {
    let c0 = calls().len();
    let mut a = adv(&c.h, store);
    catch_up(&mut a, &c.h);
    (a, c0)
}

fn accepted(a: &Adv, c0: usize, row: &str) {
    assert_eq!(discard_reason(a), None, "{row}");
    let m = model_calls(c0);
    assert!(m.contains(&"model_restore"), "{row}: {m:?}");
}

fn discarded(a: &Adv, c0: usize, reason: u64, row: &str) {
    assert_eq!(discard_reason(a), Some((1, reason)), "{row}");
    let m = model_calls(c0);
    assert!(m.contains(&"create_model"), "{row}: {m:?}");
    if reason != hft_sequencer::codes::discard::RESTORE_ERR {
        assert!(!m.contains(&"model_restore"), "{row}: {m:?}");
    }
}

#[test]
fn sdk_065_checkpoint_key_matrix() {
    use hft_sequencer::codes::discard;
    let c = ck();
    // .1 unchanged.
    let (a, c0) = restart(&c, store_of(&c, |_| {}));
    accepted(&a, c0, "SDK-065.1");
    let m = model_calls(c0);
    assert_eq!(m.iter().filter(|x| **x == "model_restore").count(), 1);
    assert_eq!(m.iter().filter(|x| **x == "model_restore_path").count(), 1);
    assert_eq!(path_reset(&a), None);
    // .2 kind_id in the header.
    let (a, c0) = restart(&c, hdr_edit(&c, |h, _| h.kind_id = NP));
    discarded(&a, c0, discard::KEY_MISMATCH, "SDK-065.2");
    // .3 the binary's layout.
    SCRIPT.with(|x| x.borrow_mut().layout_alt = true);
    let (a, c0) = restart(&c, store_of(&c, |_| {}));
    SCRIPT.with(|x| x.borrow_mut().layout_alt = false);
    discarded(&a, c0, discard::KEY_MISMATCH, "SDK-065.3");
    // .4 schema id (header vs binary).
    let (a, c0) = restart(&c, hdr_edit(&c, |h, _| h.schema_id = 0xFF05));
    discarded(&a, c0, discard::KEY_MISMATCH, "SDK-065.4");
    // .5 strategy, .6 account.
    let (a, c0) = restart(&c, hdr_edit(&c, |h, _| h.strategy = StrategyId(4)));
    discarded(&a, c0, discard::KEY_MISMATCH, "SDK-065.5");
    let (a, c0) = restart(&c, hdr_edit(&c, |h, _| h.account = AC2));
    discarded(&a, c0, discard::KEY_MISMATCH, "SDK-065.6");
    // .7 no checkpoint.
    let (a, c0) = restart(&c, MemCkpt::default());
    discarded(&a, c0, discard::MISSING, "SDK-065.7");
    // .8 header length disagrees with the body.
    let (a, c0) = restart(&c, hdr_edit(&c, |h, _| h.len += 1));
    discarded(&a, c0, discard::CORRUPT, "SDK-065.8");
    // .9 one bit of the raw-sample section flipped (checksum mismatch).
    let (a, c0) = restart(
        &c,
        hdr_edit(&c, |_, b| {
            let n = b.len() - 1;
            b[n] ^= 1;
        }),
    );
    discarded(&a, c0, discard::CORRUPT, "SDK-065.9");
    // .10 path_len past the body (checksum recomputed).
    let (a, c0) = restart(
        &c,
        hdr_edit(&c, |h, b| {
            let big = (b.len() as u32).to_le_bytes();
            b[..4].copy_from_slice(&big);
            h.checksum = ckpt_checksum(b);
        }),
    );
    discarded(&a, c0, discard::CORRUPT, "SDK-065.10");
    // .11 last_seq not in the readable journal.
    let (a, c0) = restart(&c, hdr_edit(&c, |h, _| h.last_seq = 1 << 40));
    discarded(&a, c0, discard::SEQ_NOT_IN_JOURNAL, "SDK-065.11");
    // .12 restore fails.
    SCRIPT.with(|x| x.borrow_mut().fail_model_restore = true);
    let (a, c0) = restart(&c, store_of(&c, |_| {}));
    SCRIPT.with(|x| x.borrow_mut().fail_model_restore = false);
    discarded(&a, c0, discard::RESTORE_ERR, "SDK-065.12");
    let m = model_calls(c0);
    let r = m.iter().position(|x| *x == "model_restore").expect("restore");
    assert!(m[r..].contains(&"create_model"), "{m:?}");
    // .13 a non-structural SetParams after last_seq.
    {
        let mut c13 = ck();
        let mut p = gt3(8);
        p.values[F_THR] = 8;
        let id = c13.h.set_params(&p);
        c13.h.run();
        assert_eq!(c13.h.res(id), (status::APPLIED, 0));
        SCRIPT.with(|x| x.borrow_mut().model_params.clear());
        let st = store_of(&c13, |_| {});
        let (a, c0) = restart(&c13, st);
        accepted(&a, c0, "SDK-065.13");
        let mp: Vec<i64> = SCRIPT.with(|x| {
            x.borrow()
                .model_params
                .iter()
                .filter(|p| p.0 == S1)
                .map(|p| p.1)
                .collect()
        });
        assert_eq!(mp, vec![7, 8], "params at last_seq, then the replayed SetParams");
    }
    // .14 another binary (`DESC.version` 4).
    {
        let c0 = calls().len();
        let mut a: Advisor<TestSetV4, MemCkpt, Ring> = match Advisor::start(
            &committed(c.h.j()),
            &c.h.setup,
            adv_cfg(),
            store_of(&c, |_| {}),
            Ring::big(),
            hb(),
        ) {
            Ok(a) => a,
            Err(e) => panic!("start: {e:?}"),
        };
        let log = committed(c.h.j());
        for _ in 0..10_000 {
            if !a.poll_once(&log) {
                break;
            }
        }
        assert_eq!(discard_reason(&a), None, "SDK-065.14");
        assert!(model_calls(c0).contains(&"model_restore"));
    }
    // .15 ReplaceRefdata (refdata version 2) after last_seq.
    {
        let mut c15 = ck();
        let mut rd2 = b_gt3().rd;
        ck_script();
        rd2.version = 2;
        let blob = rd2.blob();
        let r = hft_event::BlobRef {
            kind: hft_event::BlobKind::Refdata,
            encoding: 1,
            len: blob.len() as u32,
            version: 2,
            sha256: sha(&blob),
        };
        let parsed = hft_refdata::RefData::load(&blob, Some(&c15.h.s.engine().rd));
        c15.h.io().mailbox.push(hft_sequencer::ParsedConfig::Refdata {
            sha256: r.sha256,
            result: Box::new(parsed),
        });
        c15.h
            .blob_input(r, scope(StrategyId(0), AccountId(0), GroupId(0)), &blob);
        let id = c15
            .h
            .cmd_confirmed(AccountId(0), &hft_event::ControlCmd::ReplaceRefdata { blob: r });
        c15.h.run();
        assert_eq!(c15.h.res(id).0, status::APPLIED);
        let st = store_of(&c15, |_| {});
        let (a, c0) = restart(&c15, st);
        accepted(&a, c0, "SDK-065.15");
    }
    // .16 hft-core restarted by another build (HostProfile.build_git differs).
    {
        // A new build starts only from a finished Drain (hft-sequencer SEQ-273).
        let mut c16 = ck();
        let st = store_of(&c16, |_| {});
        c16.h.cmd_confirmed(
            AccountId(0),
            &hft_event::ControlCmd::Drain { cancel_resting: true },
        );
        let mut done = false;
        for _ in 0..200 {
            match c16.h.step() {
                StepOutcome::Done { .. } => {
                    done = true;
                    break;
                }
                _ => c16.h.adv(10_000_000),
            }
        }
        assert!(done, "drain finished");
        let rr = Rerun {
            build: [0x5A; 20],
            ..Rerun::default()
        };
        let j = c16.h.j().clone();
        let mut h2 = H::<TestSet>::restart_over(
            j,
            c16.h.setup.clone(),
            plan(SnapshotSel::DrainTail, false, 1, 0),
            rr,
        )
        .expect("restart");
        h2.run();
        let c0 = calls().len();
        let mut a = adv(&h2, st);
        catch_up(&mut a, &h2);
        accepted(&a, c0, "SDK-065.16");
    }
    // .17 the binary's PATH_VERSION 2.
    SCRIPT.with(|x| x.borrow_mut().path_version = 2);
    let (a, c0) = restart(&c, store_of(&c, |_| {}));
    SCRIPT.with(|x| x.borrow_mut().path_version = 1);
    accepted(&a, c0, "SDK-065.17");
    assert!(!model_calls(c0).contains(&"model_restore_path"));
    let v = path_reset(&a).expect("ModelPathReset");
    assert_eq!(&v[..5], &[1, c.last, 0, 1, 2]);
    let _ = (StepOutcome::Idle, RingId::Ctl, EventKind::NOTE);
}

#[test]
fn sdk_066_restore_order() {
    let mut c = ck();
    let mut p = gt3(8);
    p.values[F_THR] = 8;
    let m8 = c.h.mark();
    let id = c.h.set_params(&p);
    c.h.run();
    assert_eq!(c.h.res(id), (status::APPLIED, 0));
    let seq8 =
        c.h.kinds_since(m8, EventKind::CONTROL_CMD)
            .last()
            .map(|r| r.seq)
            .expect("SetParams");
    c.h.quote(A1, "50000", "50020");
    c.h.run();
    SCRIPT.with(|x| x.borrow_mut().model_params.clear());
    clear_log();
    let st = store_of(&c, |_| {});
    let (_a, c0) = restart(&c, st);
    let calls = model_calls(c0);
    assert_eq!(
        calls,
        vec![
            "create_model",
            "model_restore",
            "model_restore_path",
            "model_on_params",
            "model_on_params"
        ]
    );
    let mp: Vec<i64> = SCRIPT.with(|x| {
        x.borrow()
            .model_params
            .iter()
            .filter(|p| p.0 == S1)
            .map(|p| p.1)
            .collect()
    });
    assert_eq!(mp, vec![7, 8]);
    // Only records after last_seq reach the Model: the a1 Bbos after it, in order.
    let bbos: Vec<u64> = committed(c.h.j())
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::BBO && r.seq > c.last)
        .filter(|r| r.read::<hft_event::Bbo>().is_some_and(|b| b.h.inst == A1))
        .map(|r| r.seq)
        .collect();
    let got: Vec<u64> = model(S1)
        .iter()
        .filter(|g| g.kind == "Bbo")
        .map(|g| g.now.seq)
        .collect();
    assert_eq!(got, bbos);
    assert!(got.iter().all(|s| *s > c.last));
    assert!(got.iter().any(|s| *s > seq8));
}

#[test]
fn sdk_067_catchup_advice_predates_boot() {
    let c = ck();
    let st = store_of(&c, |_| {});
    let mut h2 =
        H::crash_restart(c.h, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).expect("restart");
    h2.run();
    let boot = committed(h2.j())
        .recs
        .iter()
        .rev()
        .find(|r| {
            r.kind == EventKind::NOTE
                && r.read::<Note>().is_some_and(|n| {
                    n.code == hft_sequencer::codes::seq_note(hft_sequencer::codes::note::EPOCH_START)
                })
        })
        .map(|r| r.seq)
        .expect("EpochStart");
    // The Model emits one Advice per a1 Bbo.
    set_mhook(|hx, ev, out| {
        if let EventRef::Bbo { .. } = ev
            && hx.s.strategy == S1
        {
            let _ = out.emit_raw(GT1, 1, &hx.v.now().seq.to_le_bytes());
        }
    });
    let mut a = adv(&h2, st);
    catch_up(&mut a, &h2);
    SCRIPT.with(|x| x.borrow_mut().mhook = None);
    let adv_recs = a.ring.advice();
    assert!(!adv_recs.is_empty());
    for (hd, _) in &adv_recs {
        assert!(hd.basis_seq > c.last && hd.basis_seq < boot, "{hd:?} boot {boot}");
    }
    clear_log();
    feed_advice(&a, &mut h2, 0);
    let got: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Advice").collect();
    assert_eq!(got.len(), adv_recs.len());
    assert!(got.iter().all(|g| g.now.boot_seq == boot));
}

#[test]
fn sdk_069_restore_path_err_note() {
    let c = ck();
    SCRIPT.with(|x| {
        let mut x = x.borrow_mut();
        x.fail_restore_path = true;
        x.model_params.clear();
    });
    let (a, c0) = restart(&c, store_of(&c, |_| {}));
    SCRIPT.with(|x| x.borrow_mut().fail_restore_path = false);
    let v = path_reset(&a).expect("ModelPathReset");
    assert_eq!(&v[..3], &[2, c.last, 0]);
    assert_eq!(discard_reason(&a), None);
    assert!(model_calls(c0).contains(&"model_on_params"));
    let mp: Vec<i64> = SCRIPT.with(|x| {
        x.borrow()
            .model_params
            .iter()
            .filter(|p| p.0 == S1)
            .map(|p| p.1)
            .collect()
    });
    assert_eq!(mp, vec![7]);
    assert!(model(S1).iter().any(|g| g.kind == "Bbo" && g.now.seq > c.last));
}

#[test]
fn sdk_074_oversize_generation_discarded() {
    // s1 declares snapshot_bytes 10 000 (`ckpt` field); its Model writes 8 B per group.
    let mut b = b_gt3();
    ck_script();
    if let Some(p) = b.params.iter_mut().find(|p| p.strategy == S1) {
        p.values[F_CKPT] = 10_000;
    }
    let mut h = b.boot();
    h.steady();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    a.begin_checkpoints();
    while a.checkpointing() {
        a.poll_once(&hft_sequencer::driver::replay::Log::default());
    }
    let gen1 = a.ckpt.gens.get(&S1).map(|g| g.0.last_seq).expect("generation 1");
    // The declaration drops to 16 B: the next generation (24 B of samples) is over it.
    let mut p = gt3(2);
    p.values[F_CKPT] = 16;
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    h.quote(A1, "50000", "50009");
    h.run();
    catch_up(&mut a, &h);
    let commits = a.commits;
    a.begin_checkpoints();
    while a.checkpointing() {
        a.poll_once(&hft_sequencer::driver::replay::Log::default());
    }
    assert_eq!(a.commits, commits, "no commit of the oversize generation");
    assert_eq!(a.ckpt.gens.get(&S1).map(|g| g.0.last_seq), Some(gen1));
    let sk = adv_notes(&a.notes, &a.ring, advisor_note::MODEL_CKPT_SKIPPED);
    assert_eq!(sk.len(), 1, "{sk:?}");
    assert_eq!(sk[0].level, hft_strategy_sdk::NoteLevel::P1);
    assert_eq!(sk[0].v[2], 16);
    // A restart restores generation 1.
    let mut st = MemCkpt::default();
    st.gens = a.ckpt.gens.clone();
    let c0 = calls().len();
    let a2 = adv(&h, st);
    assert_eq!(discard_reason(&a2), None);
    assert!(model_calls(c0).contains(&"model_restore"));
    assert_eq!(a2.ckpt.gens.get(&S1).map(|g| g.0.last_seq), Some(gen1));
}
