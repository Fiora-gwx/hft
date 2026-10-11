//! The advisor (TESTS.md "advisor", "advisor 与 Model", "模型检查点": SEQ-030–SEQ-051, SEQ-363,
//! SEQ-367).

use std::collections::HashMap;

use hft_event::{AdviceHdr, EventKind, Note, NoteSource};
use hft_sequencer::advisor::{
    AdviceRing, Advisor, AdvisorCfg, CheckpointStore, CkptHdr, CkptIoErr, ckpt_checksum,
};
use hft_sequencer::driver::live::RingId;
use hft_sequencer::driver::replay::Log;
use hft_sequencer::journal::MemJournal;
use hft_strategy_sdk::{EvMask, EventRef, GroupResets, ModelParamEffect, Part, ResetCause};
use hft_types::{GroupId, InstId, StrategyId};
use zerocopy::FromBytes;

use crate::common::*;

// ------------------------------------------------------------------------------- doubles

/// In-memory `CheckpointStore`: one committed generation per instance, call log, faults.
#[derive(Debug, Default)]
pub struct MemCkpt {
    pub gens: HashMap<StrategyId, (CkptHdr, Vec<u8>)>,
    cur: Option<(StrategyId, CkptHdr, Vec<u8>)>,
    pub calls: Vec<&'static str>,
    pub chunks: u32,
    pub fail_commit: bool,
    pub fail_chunk_at: Option<u32>,
}

impl CheckpointStore for MemCkpt {
    fn load(&mut self, s: StrategyId, out: &mut Vec<u8>) -> Option<CkptHdr> {
        self.calls.push("load");
        let (h, b) = self.gens.get(&s)?;
        out.clear();
        out.extend_from_slice(b);
        Some(*h)
    }
    fn begin(&mut self, s: StrategyId, hdr: &CkptHdr) -> Result<(), CkptIoErr> {
        self.calls.push("begin");
        self.cur = Some((s, *hdr, Vec::new()));
        self.chunks = 0;
        Ok(())
    }
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), CkptIoErr> {
        self.calls.push("write_chunk");
        self.chunks += 1;
        if self.fail_chunk_at == Some(self.chunks) {
            self.cur = None;
            return Err(CkptIoErr);
        }
        if let Some(c) = self.cur.as_mut() {
            c.2.extend_from_slice(bytes);
        }
        Ok(())
    }
    fn commit(&mut self) -> Result<(), CkptIoErr> {
        self.calls.push("commit");
        if self.fail_commit {
            self.cur = None;
            return Err(CkptIoErr);
        }
        let Some((s, mut h, b)) = self.cur.take() else {
            return Err(CkptIoErr);
        };
        h.len = b.len() as u64;
        h.checksum = ckpt_checksum(&b);
        self.gens.insert(s, (h, b));
        Ok(())
    }
}

/// The advice ring: capacity in bytes, records kept.
#[derive(Debug, Default)]
pub struct Ring {
    pub cap: usize,
    pub used: usize,
    pub recs: Vec<(EventKind, Vec<u8>)>,
}

impl AdviceRing for Ring {
    fn free(&self) -> usize {
        self.cap.saturating_sub(self.used)
    }
    fn push(&mut self, kind: EventKind, bytes: &[u8]) -> bool {
        if self.free() < bytes.len() {
            return false;
        }
        self.used += bytes.len();
        self.recs.push((kind, bytes.to_vec()));
        true
    }
}

impl Ring {
    pub fn big() -> Ring {
        Ring {
            cap: 1 << 24,
            used: 0,
            recs: Vec::new(),
        }
    }
    pub fn advice(&self) -> Vec<(AdviceHdr, Vec<u8>)> {
        self.recs
            .iter()
            .filter(|r| r.0 == EventKind::ADVICE)
            .map(|r| {
                (
                    AdviceHdr::read_from_bytes(&r.1[..32]).unwrap(),
                    r.1[32..].to_vec(),
                )
            })
            .collect()
    }
    pub fn notes(&self) -> Vec<Note> {
        self.recs
            .iter()
            .filter(|r| r.0 == EventKind::NOTE)
            .map(|r| hft_sequencer::types::read_body::<Note>(&r.1).unwrap())
            .collect()
    }
}

/// The committed part of a journal.
pub fn committed(j: &MemJournal) -> Log {
    let mut l = Log::from_segments(&j.segments);
    l.recs.truncate(j.committed);
    l
}

type Adv = Advisor<TestSet, MemCkpt, Ring>;

fn start(h: &H, ckpt: MemCkpt, ring: Ring, cfg: AdvisorCfg) -> Adv {
    match Advisor::start(&committed(h.j()), &h.setup, cfg, ckpt, ring, hb()) {
        Ok(a) => a,
        Err(e) => panic!("advisor start: {e:?}"),
    }
}

fn cfg() -> AdvisorCfg {
    AdvisorCfg {
        batch: 64,
        checkpoint_ms: 0,
        chunk_bytes: 1 << 20,
        max_lag_ms: 5_000,
        shard: Some(vec![S2]),
    }
}

/// Polls until the mirror caught up with the committed journal and the outbox is empty.
fn catch_up(a: &mut Adv, h: &H) {
    let log = committed(h.j());
    for _ in 0..100_000 {
        let busy = a.poll_once(&log);
        if !busy && !a.checkpointing() {
            break;
        }
    }
}

fn model_log(s: StrategyId) -> Vec<Got> {
    log().into_iter().filter(|g| g.model && g.s == s).collect()
}

/// B0 with s2's Model subscribed to a1 BBO (`Part::Model`) and `model_own(ORDER)`.
fn b0_model() -> H {
    reset_script();
    set_subs(S2, vec![(A1, 0, EvMask::BBO, Part::Model)]);
    SCRIPT.with(|x| {
        x.borrow_mut()
            .model_own
            .push((S2, hft_strategy_sdk::OwnEvMask::ORDER.0))
    });
    let mut h = b0().boot();
    h.steady();
    h
}

// ------------------------------------------------------------------------------- tests

#[test]
fn seq_030_poll_once_bounded() {
    let mut b = {
        reset_script();
        b0()
    };
    let mut p = Spb::s2(1);
    p.groups = vec![
        gspec(G2),
        gspec(GroupId(0x0200_0002)),
        gspec(GroupId(0x0200_0003)),
    ];
    b.params = vec![Spb::s1(1), p, Spb::s9(1)];
    let mut h = b.boot();
    h.steady();
    for _ in 0..20 {
        h.quote(A1, "50000", "50002");
        h.run();
    }
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    a.begin_checkpoints();
    // First chunk (path block) → 3 left.
    let log = committed(h.j());
    a.poll_once(&log);
    let seq0 = a.applied_seq();
    let ev0 = model_log(S2).len();
    let n0 = a.ckpt.chunks;
    for k in 0..3 {
        // New records keep arriving.
        h.quote(A1, "50000", "50003");
        h.run();
        let log = committed(h.j());
        assert!(a.poll_once(&log));
        assert_eq!(a.ckpt.chunks, n0 + k + 1);
        assert_eq!(a.applied_seq(), seq0);
        assert_eq!(model_log(S2).len(), ev0);
    }
    assert_eq!(a.ckpt.calls.iter().filter(|c| **c == "commit").count(), 1);
    let calls = a.ckpt.calls.len();
    let log = committed(h.j());
    a.poll_once(&log);
    assert!(a.applied_seq() > seq0);
    assert_eq!(a.ckpt.calls.len(), calls);
}

#[test]
fn seq_031_outbox_keeps_latest() {
    let mut h = b0_model();
    let mut a = start(
        &h,
        MemCkpt::default(),
        Ring {
            cap: 0,
            used: 0,
            recs: Vec::new(),
        },
        cfg(),
    );
    catch_up(&mut a, &h);
    set_mhook(|hx, ev, out| {
        if let EventRef::Bbo { .. } = ev
            && hx.s.strategy == S2
        {
            let _ = out.emit_raw(G2, 7, b"P1");
            let _ = out.emit_raw(G2, 7, b"P2");
            let _ = out.emit_raw(G2, 7, b"P3");
            let _ = out.emit_raw(G2, 8, b"Q1");
        }
    });
    h.quote(A1, "50000", "50004");
    h.run();
    let c0 = a.advice_coalesced;
    a.poll_once(&committed(h.j()));
    assert_eq!(a.outbox.len(), 2);
    assert_eq!(&a.outbox[0].rec[32..], b"P3");
    assert_eq!(&a.outbox[1].rec[32..], b"Q1");
    assert_eq!(a.advice_coalesced, c0 + 2);
    assert!(a.ring.recs.iter().all(|r| r.0 != EventKind::ADVICE));
    a.ring.cap = 1 << 20;
    a.poll_once(&committed(h.j()));
    let adv = a.ring.advice();
    assert_eq!(adv.len(), 2);
    assert_eq!(adv[0].1, b"P3");
    assert_eq!(adv[1].1, b"Q1");
    assert!(a.outbox.is_empty());
    SCRIPT.with(|x| x.borrow_mut().mhook = None);
}

#[test]
fn seq_032_advice_ring_space() {
    for (free, pushed) in [(543usize, false), (544, true), (545, true)] {
        let mut h = b0_model();
        let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
        catch_up(&mut a, &h);
        a.ring.cap = a.ring.used;
        set_mhook(|hx, ev, out| {
            if let EventRef::Bbo { .. } = ev
                && hx.s.strategy == S2
            {
                let _ = out.emit_raw(G2, 7, &[0x5A; 512]);
            }
        });
        h.quote(A1, "50000", "50004");
        h.run();
        a.poll_once(&committed(h.j()));
        assert_eq!(a.outbox.len(), 1);
        a.ring.cap = a.ring.used + free;
        a.poll_once(&committed(h.j()));
        assert_eq!(a.ring.advice().len(), usize::from(pushed), "free {free}");
        assert_eq!(a.outbox.len(), usize::from(!pushed));
        SCRIPT.with(|x| x.borrow_mut().mhook = None);
    }
}

#[test]
fn seq_033_basis_seq_tsc() {
    let mut h = b0_model();
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    set_mhook(|hx, ev, out| {
        if let EventRef::Bbo { .. } = ev
            && hx.s.strategy == S2
        {
            let _ = out.emit_raw(G2, 7, b"basis");
        }
    });
    let m = h.mark();
    h.adv(77_000);
    h.quote(A1, "50000", "50004");
    h.run();
    let bbo = h.since(m).into_iter().find(|r| r.kind == EventKind::BBO).unwrap();
    catch_up(&mut a, &h);
    SCRIPT.with(|x| x.borrow_mut().mhook = None);
    let adv = a.ring.advice();
    let (hd, p) = adv.last().cloned().expect("advice");
    assert_eq!(hd.strategy, S2);
    assert_eq!(hd.basis_seq, bbo.seq);
    assert_eq!(hd.basis_tsc.0, bbo.tsc);
    // The sequencer delivers it with the same values.
    clear_log();
    let raw = a
        .ring
        .recs
        .iter()
        .rev()
        .find(|r| r.0 == EventKind::ADVICE)
        .unwrap()
        .1
        .clone();
    h.push_bytes(RingId::Advice, EventKind::ADVICE, raw);
    h.run();
    let got = log()
        .into_iter()
        .find(|g| g.s == S2 && g.kind == "Advice" && !g.model)
        .expect("delivered");
    assert!(
        got.text.contains(&format!("basis_seq: {}", bbo.seq)),
        "{}",
        got.text
    );
    assert_eq!(p, b"basis");
}

#[test]
fn seq_035_model_delivery_table() {
    let mut h = b0_model();
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    let row = |h: &mut H, a: &mut Adv, f: &dyn Fn(&mut H)| -> Vec<&'static str> {
        catch_up(a, h);
        clear_log();
        f(h);
        catch_up(a, h);
        model_log(S2).iter().map(|g| g.kind).collect()
    };
    // .1 subscribed a1 BBO.
    assert_eq!(
        row(&mut h, &mut a, &|h| {
            h.quote(A1, "50000", "50005");
            h.run();
        }),
        vec!["Bbo"]
    );
    // .2 a2 not subscribed.
    assert!(
        row(&mut h, &mut a, &|h| {
            h.quote(A2, "3000", "3001");
            h.run();
        })
        .is_empty()
    );
    // .7 ConnState Down of B: no Conn.
    assert!(
        row(&mut h, &mut a, &|h| {
            let t = h.now();
            h.push(
                RingId::Rep(0),
                &conn(
                    B,
                    0,
                    hft_event::ConnStateKind::Down,
                    hft_event::ConnRole::Md,
                    InstId(0),
                    hft_types::AccountId(0),
                    t,
                ),
            );
            h.run();
        })
        .iter()
        .all(|k| *k != "Conn")
    );
}

#[test]
fn seq_037_model_group_reset_notes() {
    reset_script();
    let mut b = b0();
    let g2b = GroupId(0x0200_0002);
    let mut p = Spb::s2(1);
    p.groups = vec![gspec(G2), gspec_eth(g2b)];
    b.params = vec![Spb::s1(1), p.clone(), Spb::s9(1)];
    let mut h = b.boot();
    h.steady();
    let mut r = GroupResets::new(8);
    r.set(0, ResetCause::Structural);
    r.set(1, ResetCause::Structural);
    SCRIPT.with(|x| {
        x.borrow_mut()
            .reset_effect
            .push((5, ModelParamEffect::ResetGroups(r)))
    });
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    let mut p5 = p.clone();
    p5.version = 5;
    p5.values[F_THR] = 5;
    let id = h.set_params(&p5);
    h.run();
    assert_eq!(h.result_of(id).map(|r| (r.status, r.detail)), Some((0, 0)));
    catch_up(&mut a, &h);
    eprintln!("notes {:?}", a.ring.notes());
    let notes: Vec<Note> = a
        .ring
        .notes()
        .into_iter()
        .filter(|n| n.code == hft_event::note_code(NoteSource::Advisor, 4))
        .collect();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].strategy, S2);
    assert_eq!(notes[0].group, G2);
    assert_eq!(notes[1].group, g2b);
    assert!(notes.iter().all(|n| n.v[0] == 1 && n.v[1] == 5));
}

#[test]
fn seq_038_reset_all_reason_4() {
    reset_script();
    let mut b = b0();
    let g2b = GroupId(0x0200_0002);
    let mut p = Spb::s2(1);
    p.groups = vec![gspec(G2), gspec_eth(g2b)];
    b.params = vec![Spb::s1(1), p.clone(), Spb::s9(1)];
    let mut h = b.boot();
    h.steady();
    SCRIPT.with(|x| x.borrow_mut().reset_effect.push((6, ModelParamEffect::ResetAll)));
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    let mut p6 = p.clone();
    p6.version = 6;
    p6.values[F_THR] = 6;
    h.set_params(&p6);
    h.run();
    catch_up(&mut a, &h);
    let notes: Vec<Note> = a
        .ring
        .notes()
        .into_iter()
        .filter(|n| n.code == hft_event::note_code(NoteSource::Advisor, 4))
        .collect();
    assert_eq!(notes.len(), 2);
    assert!(notes.iter().all(|n| n.v[0] == 4 && n.v[1] == 6));
}

#[test]
fn seq_040_warmup_to_model() {
    let mut h = b0_model();
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    clear_log();
    let mut ids = Vec::new();
    for (no, last) in [(0u32, 0u8), (1, 0), (2, 1)] {
        let c = hft_event::ControlCmd::InjectWarmup {
            instance: S2,
            last,
            local_kind: 3,
            chunk_no: no,
            len: 4,
            _pad: [0; 6],
            payload: b"warm",
        };
        ids.push(h.cmd(hft_types::AccountId(0), &c));
        h.run();
    }
    for id in ids {
        assert_eq!(h.result_of(id).map(|r| r.status), Some(0));
    }
    catch_up(&mut a, &h);
    let w: Vec<Got> = model_log(S2).into_iter().filter(|g| g.kind == "Warmup").collect();
    assert_eq!(w.len(), 3);
    assert!(w[0].text.contains("chunk_no: 0") && w[0].text.contains("last: false"));
    assert!(w[2].text.contains("chunk_no: 2") && w[2].text.contains("last: true"));
    assert!(!log().iter().any(|g| !g.model && g.kind == "Warmup"));
    clear_log();
    let c = hft_event::ControlCmd::InjectWarmup {
        instance: StrategyId(7),
        last: 1,
        local_kind: 3,
        chunk_no: 0,
        len: 4,
        _pad: [0; 6],
        payload: b"warm",
    };
    let id = h.cmd(hft_types::AccountId(0), &c);
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(21));
    catch_up(&mut a, &h);
    assert!(model_log(S2).iter().all(|g| g.kind != "Warmup"));
}

#[test]
fn seq_044_verify_does_not_run_model() {
    let mut h = b0_model();
    for k in 0..30u64 {
        let mut hd = <AdviceHdr as zerocopy::FromZeros>::new_zeroed();
        hd.strategy = S2;
        hd.account = AC2;
        hd.local_kind = 7;
        hd.group = G2;
        hd.len = 8;
        hd.basis_seq = k;
        let mut rec = zerocopy::IntoBytes::as_bytes(&hd).to_vec();
        rec.extend_from_slice(&k.to_le_bytes());
        h.push_bytes(RingId::Advice, EventKind::ADVICE, rec);
        h.run();
    }
    clear_log();
    let m = h.verify();
    assert!(m.is_empty(), "{m:?}");
    // Fast parts got the Advice again; no Model was called.
    assert!(log().iter().any(|g| g.kind == "Advice" && !g.model));
    assert!(!log().iter().any(|g| g.model));
}

#[test]
fn seq_045_chunked_checkpoint_pauses_advisor() {
    reset_script();
    let mut b = b0();
    let mut p = Spb::s2(1);
    p.groups = (1..=4).map(|k| gspec(GroupId(0x0200_0000 + k))).collect();
    b.params = vec![Spb::s1(1), p, Spb::s9(1)];
    let mut h = b.boot();
    h.steady();
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    let l = a.applied_seq();
    a.begin_checkpoints();
    let ev0 = model_log(S2).len();
    let mut polls = 0;
    while a.checkpointing() {
        h.quote(A1, "50000", "50003");
        h.run();
        a.poll_once(&committed(h.j()));
        polls += 1;
        assert_eq!(a.applied_seq(), l);
        assert_eq!(model_log(S2).len(), ev0);
    }
    // Path block + 4 groups.
    assert_eq!(a.ckpt.calls.iter().filter(|c| **c == "begin").count(), 1);
    assert_eq!(a.ckpt.calls.iter().filter(|c| **c == "write_chunk").count(), 5);
    assert_eq!(a.ckpt.calls.iter().filter(|c| **c == "commit").count(), 1);
    assert_eq!(polls, 5);
    assert_eq!(a.ckpt.gens.get(&S2).map(|g| g.0.last_seq), Some(l));
    a.poll_once(&committed(h.j()));
    assert!(a.applied_seq() > l);
}

/// A checkpoint of s2 taken now; returns the store.
fn checkpoint_now(a: &mut Adv) -> MemCkpt {
    a.begin_checkpoints();
    while a.checkpointing() {
        a.poll_once(&Log::default());
    }
    core::mem::take(&mut a.ckpt)
}

#[test]
fn seq_401_checkpoint_file_resume_matches_continuous_model() {
    use hft_sequencer::advisor::{checkpoint, ens_of_record};

    let mut h = b0_model();
    SCRIPT.with(|x| x.borrow_mut().path_version = 7);
    set_mhook(|hx, ev, out| {
        if hx.s.strategy == S2 && matches!(ev, EventRef::Bbo { .. }) {
            let _ = out.emit_raw(G2, 7, &hx.v.now().engine_ns.to_le_bytes());
        }
    });
    let count = |a: &Adv| {
        a.mirror
            .instances()
            .models
            .slots
            .iter()
            .filter_map(|s| s.model.as_ref())
            .find(|m| m.i.strategy == S2)
            .unwrap()
            .n_events
    };
    let mut continuous = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut continuous, &h);
    for _ in 0..8 {
        h.adv(10_000_000);
        h.quote(A1, "50000", "50003");
        h.run();
        catch_up(&mut continuous, &h);
    }
    let before = count(&continuous);
    assert!(before >= 8);
    let saved = checkpoint_now(&mut continuous);
    let (hdr, body) = saved.gens.get(&S2).unwrap();
    assert_eq!(hdr.layout_version, 1);
    assert_eq!(&body[..8], &[7, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&body[8..], &before.to_le_bytes());

    struct TempFile(std::path::PathBuf);
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let path = TempFile(std::env::temp_dir().join(format!("hft-seq-401-{}.ckpt", std::process::id())));
    let bytes = checkpoint::encode(hdr, body, 1024).unwrap();
    std::fs::write(&path.0, &bytes).unwrap();
    let loaded = std::fs::read(&path.0).unwrap();
    let decoded = checkpoint::decode(&loaded, 1024).unwrap();
    assert_eq!(decoded.samples, &before.to_le_bytes());
    // engine_ns belongs to the journal, not the file's reserved header bytes.
    let journal = committed(h.j());
    let ix = journal
        .recs
        .iter()
        .position(|r| r.seq == decoded.key.last_seq)
        .unwrap();
    let last_ens = ens_of_record(&journal, ix).unwrap();
    assert!(last_ens > 0);
    assert_eq!(last_ens, hdr.last_ens);
    let restored_store = MemCkpt {
        gens: HashMap::from([(S2, (decoded.header(last_ens), decoded.body.to_vec()))]),
        ..MemCkpt::default()
    };
    let mut resumed = start(&h, restored_store, Ring::big(), cfg());
    assert!(
        !resumed
            .notes
            .iter()
            .any(|n| n.strategy == S2 && n.code == hft_event::note_code(NoteSource::Advisor, 1))
    );
    catch_up(&mut resumed, &h);
    assert_eq!(
        count(&resumed),
        before,
        "do not replay the checkpoint prefix into the Model"
    );
    continuous.ring.recs.clear();
    continuous.ring.used = 0;
    resumed.ring.recs.clear();
    resumed.ring.used = 0;
    for _ in 0..20 {
        h.adv(2_000_000);
        h.quote(A1, "50000", "50004");
        h.run();
        catch_up(&mut continuous, &h);
        catch_up(&mut resumed, &h);
    }
    assert_eq!(count(&continuous), before + 20);
    assert_eq!(count(&resumed), count(&continuous));
    let advice_bytes = |a: &Adv| {
        a.ring
            .recs
            .iter()
            .filter(|r| r.0 == EventKind::ADVICE)
            .map(|r| r.1.clone())
            .collect::<Vec<_>>()
    };
    let expected = advice_bytes(&continuous);
    assert_eq!(expected.len(), 20);
    assert_eq!(advice_bytes(&resumed), expected, "complete AdviceHdr and payload");
    let a = checkpoint_now(&mut continuous);
    let b = checkpoint_now(&mut resumed);
    assert_eq!(a.gens.get(&S2), b.gens.get(&S2));
    assert_eq!(
        std::fs::read(&path.0).unwrap(),
        bytes,
        "restore only reads the checkpoint"
    );
    assert!(h.verify().is_empty());
}

#[test]
fn seq_402_each_checkpoint_releases_at_its_own_record_boundary() {
    reset_script();
    for s in [S1, S2] {
        set_subs(s, vec![(A1, 0, EvMask::BBO, Part::Model)]);
    }
    let mut h = b0().boot();
    h.steady();
    let set = |h: &mut H, s, version: u64, thr: i64| {
        let mut p = if s == S1 {
            Spb::s1(version)
        } else {
            Spb::s2(version)
        };
        p.values[F_THR] = thr;
        let id = h.set_params(&p);
        h.run();
        assert_eq!(h.result_of(id).unwrap().status, 0);
        committed(h.j())
            .recs
            .iter()
            .rev()
            .find(|r| r.kind == EventKind::CONTROL_CMD)
            .unwrap()
            .seq
    };
    let quote = |h: &mut H| {
        h.adv(1_000_000);
        h.quote(A1, "50000", "50003");
        h.run();
    };
    set(&mut h, S1, 7, 7);
    set(&mut h, S2, 17, 17);
    let c = AdvisorCfg {
        batch: u16::MAX,
        shard: Some(vec![S1, S2]),
        ..cfg()
    };
    let mut continuous = start(&h, MemCkpt::default(), Ring::big(), c.clone());
    catch_up(&mut continuous, &h);
    quote(&mut h);
    catch_up(&mut continuous, &h);
    let first = checkpoint_now(&mut continuous).gens.remove(&S1).unwrap();
    let l1 = first.0.last_seq;
    quote(&mut h); // S1 must have checkpoint parameters before this event.
    let change1 = set(&mut h, S1, 8, 8);
    quote(&mut h);
    catch_up(&mut continuous, &h);
    let mut second = checkpoint_now(&mut continuous).gens.remove(&S2).unwrap();
    let note = Note {
        source: NoteSource::Advisor,
        code: hft_event::note_code(
            NoteSource::Advisor,
            hft_sequencer::codes::advisor_note::ADVISOR_STATS,
        ),
        level: hft_event::NoteLevel::Info,
        strategy: S2,
        _pad: 0,
        group: hft_types::GROUP_NONE,
        inst: InstId(0),
        v: [0; 8],
    };
    h.push(RingId::Advice, &note);
    h.run();
    // A standalone Meta note is published with the following complete Input. Keep its
    // position as the checkpoint boundary, but wait for that publication before loading it.
    let pending = h
        .recs()
        .iter()
        .find(|r| r.kind == EventKind::NOTE && r.read::<Note>().is_some_and(|n| n.code == note.code))
        .expect("AdvisorStats note was recorded")
        .seq;
    assert!(committed(h.j()).recs.iter().all(|r| r.seq < pending));
    quote(&mut h); // S2 must have its checkpoint parameters before this following Input.
    // No Model event occurs between the saved state and this Meta record. A caller that
    // records the consumed journal boundary may legitimately put last_seq here.
    let l2 = committed(h.j())
        .recs
        .iter()
        .rev()
        .find(|r| r.kind == EventKind::NOTE && r.read::<Note>().is_some_and(|n| n.code == note.code))
        .expect("AdvisorStats note was published with the following BBO")
        .seq;
    assert_eq!(l2, pending);
    assert!(l1 < second.0.last_seq && second.0.last_seq < l2);
    second.0.last_seq = l2;
    let change2 = set(&mut h, S2, 18, 18);
    quote(&mut h);
    catch_up(&mut continuous, &h);
    let expected = checkpoint_now(&mut continuous);
    let store = MemCkpt {
        gens: HashMap::from([(S1, first), (S2, second)]),
        ..MemCkpt::default()
    };
    SCRIPT.with(|x| x.borrow_mut().model_params.clear());
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = seen.clone();
    set_mhook(move |hx, ev, _| {
        if matches!(ev, EventRef::Bbo { .. }) {
            let value = SCRIPT.with(|x| {
                x.borrow()
                    .model_params
                    .iter()
                    .rev()
                    .find(|p| p.0 == hx.s.strategy)
                    .map(|p| p.1)
            });
            observed.borrow_mut().push((hx.s.strategy, hx.v.now().seq, value));
        }
    });
    let mut resumed = start(&h, store, Ring::big(), c);
    catch_up(&mut resumed, &h);
    for (s, last, change, old, new) in [(S1, l1, change1, 7, 8), (S2, l2, change2, 17, 18)] {
        let params: Vec<_> = SCRIPT.with(|x| {
            x.borrow()
                .model_params
                .iter()
                .filter(|p| p.0 == s)
                .map(|p| p.1)
                .collect()
        });
        assert_eq!(params, vec![old, new]);
        let want: Vec<_> = committed(h.j())
            .recs
            .iter()
            .filter(|r| r.seq > last && r.kind == EventKind::BBO)
            .filter(|r| r.read::<hft_event::Bbo>().is_some_and(|b| b.h.inst == A1))
            .map(|r| (s, r.seq, Some(if r.seq < change { old } else { new })))
            .collect();
        assert!(want.iter().any(|r| r.2 == Some(old)) && want.iter().any(|r| r.2 == Some(new)));
        assert_eq!(
            seen.borrow()
                .iter()
                .filter(|r| r.0 == s)
                .copied()
                .collect::<Vec<_>>(),
            want
        );
    }
    let actual = checkpoint_now(&mut resumed);
    assert_eq!(actual.gens, expected.gens);
    assert!(h.verify().is_empty());
}

#[test]
fn seq_047_checkpoint_discard_reasons() {
    let disc = hft_event::note_code(NoteSource::Advisor, 1);
    let reason = |a: &Adv| -> Option<u64> {
        a.notes
            .iter()
            .chain(a.ring.notes().iter())
            .find(|n| n.code == disc && n.strategy == S2)
            .map(|n| n.v[1])
    };
    let mut h = b0_model();
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    // .1 missing (the first start has no checkpoint).
    assert_eq!(reason(&a), Some(1));
    catch_up(&mut a, &h);
    let store = checkpoint_now(&mut a);
    h.quote(A1, "50000", "50003");
    h.run();
    // A valid checkpoint: no discard.
    let mut s = MemCkpt {
        gens: store.gens.clone(),
        ..MemCkpt::default()
    };
    let a2 = start(&h, s, Ring::big(), cfg());
    assert_eq!(reason(&a2), None);
    // .2 corrupt.
    s = MemCkpt {
        gens: store.gens.clone(),
        ..MemCkpt::default()
    };
    if let Some(g) = s.gens.get_mut(&S2) {
        let n = g.1.len() - 1;
        g.1[n] ^= 0xFF;
    }
    assert_eq!(reason(&start(&h, s, Ring::big(), cfg())), Some(2));
    // .3 The version alone changes; the layout descriptor is exactly the same.
    assert_eq!(store.gens[&S2].0.layout_version, 1);
    SCRIPT.with(|x| x.borrow_mut().layout_version_delta = 1);
    let s3 = MemCkpt {
        gens: store.gens.clone(),
        ..MemCkpt::default()
    };
    assert_eq!(reason(&start(&h, s3, Ring::big(), cfg())), Some(3));
    SCRIPT.with(|x| x.borrow_mut().layout_version_delta = 0);
    // Descriptor text is not a checkpoint key. Its spelling must not invalidate raw samples.
    SCRIPT.with(|x| x.borrow_mut().layout_alt = true);
    let descriptor_only = MemCkpt {
        gens: store.gens.clone(),
        ..MemCkpt::default()
    };
    assert_eq!(reason(&start(&h, descriptor_only, Ring::big(), cfg())), None);
    SCRIPT.with(|x| x.borrow_mut().layout_alt = false);
    // .4 schema.
    let mut s4 = MemCkpt {
        gens: store.gens.clone(),
        ..MemCkpt::default()
    };
    s4.gens.get_mut(&S2).unwrap().0.schema_id ^= 1;
    assert_eq!(reason(&start(&h, s4, Ring::big(), cfg())), Some(3));
    // .5 kind and .6 instance.
    let mut s5 = MemCkpt {
        gens: store.gens.clone(),
        ..MemCkpt::default()
    };
    if let Some(g) = s5.gens.get_mut(&S2) {
        g.0.kind_id = TAKER;
    }
    assert_eq!(reason(&start(&h, s5, Ring::big(), cfg())), Some(3));
    let mut s6 = MemCkpt {
        gens: store.gens.clone(),
        ..MemCkpt::default()
    };
    if let Some(g) = s6.gens.get_mut(&S2) {
        g.0.strategy = S1;
    }
    assert_eq!(reason(&start(&h, s6, Ring::big(), cfg())), Some(3));
    // .7 last_seq not in the journal.
    let mut s7 = MemCkpt {
        gens: store.gens.clone(),
        ..MemCkpt::default()
    };
    if let Some(g) = s7.gens.get_mut(&S2) {
        g.0.last_seq = 1 << 40;
    }
    assert_eq!(reason(&start(&h, s7, Ring::big(), cfg())), Some(4));
    // .8 model_restore error.
    SCRIPT.with(|x| x.borrow_mut().fail_model_restore = true);
    let s8 = MemCkpt {
        gens: store.gens.clone(),
        ..MemCkpt::default()
    };
    assert_eq!(reason(&start(&h, s8, Ring::big(), cfg())), Some(5));
    SCRIPT.with(|x| x.borrow_mut().fail_model_restore = false);
}

#[test]
fn seq_049_path_section_reset() {
    let mut h = b0_model();
    SCRIPT.with(|x| x.borrow_mut().path_version = 1);
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    let store = checkpoint_now(&mut a);
    let last = store.gens.get(&S2).unwrap().0.last_seq;
    h.quote(A1, "50000", "50003");
    h.run();
    SCRIPT.with(|x| x.borrow_mut().path_version = 2);
    let a2 = start(
        &h,
        MemCkpt {
            gens: store.gens,
            ..MemCkpt::default()
        },
        Ring::big(),
        cfg(),
    );
    let pr = a2
        .notes
        .iter()
        .find(|n| n.code == hft_event::note_code(NoteSource::Advisor, 2))
        .expect("ModelPathReset");
    assert_eq!(pr.strategy, S2);
    assert_eq!(pr.v[0], 1);
    assert_eq!(pr.v[1], last);
    assert_eq!(pr.v[3], 1);
    assert_eq!(pr.v[4], 2);
    assert!(
        !a2.notes
            .iter()
            .any(|n| n.code == hft_event::note_code(NoteSource::Advisor, 1) && n.strategy == S2)
    );
}

#[test]
fn seq_050_restore_then_params_once() {
    let mut h = b0_model();
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    let store = checkpoint_now(&mut a);
    for _ in 0..5 {
        h.quote(A1, "50000", "50003");
        h.run();
    }
    SCRIPT.with(|x| x.borrow_mut().model_params.clear());
    clear_log();
    let mut a2 = start(
        &h,
        MemCkpt {
            gens: store.gens,
            ..MemCkpt::default()
        },
        Ring::big(),
        cfg(),
    );
    catch_up(&mut a2, &h);
    let calls: Vec<(StrategyId, i64)> = SCRIPT.with(|x| x.borrow().model_params.clone());
    assert_eq!(calls.iter().filter(|c| c.0 == S2).count(), 1, "{calls:?}");
    // The 5 Bbo after last_seq reached the Model; nothing before it.
    assert_eq!(model_log(S2).iter().filter(|g| g.kind == "Bbo").count(), 5);
}

#[test]
fn seq_051_checkpoint_write_failure() {
    let mut h = b0_model();
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    let c1 = checkpoint_now(&mut a);
    let gen1 = c1.gens.get(&S2).cloned().unwrap();
    h.quote(A1, "50000", "50003");
    h.run();
    // .1 commit fails.
    let mut a = start(
        &h,
        MemCkpt {
            gens: c1.gens.clone(),
            fail_commit: true,
            ..MemCkpt::default()
        },
        Ring::big(),
        cfg(),
    );
    catch_up(&mut a, &h);
    let s = checkpoint_now(&mut a);
    assert_eq!(s.gens.get(&S2), Some(&gen1));
    // .2 the 2nd chunk fails: no commit.
    let mut a = start(
        &h,
        MemCkpt {
            gens: c1.gens.clone(),
            fail_chunk_at: Some(2),
            ..MemCkpt::default()
        },
        Ring::big(),
        cfg(),
    );
    catch_up(&mut a, &h);
    let s = checkpoint_now(&mut a);
    assert_eq!(s.gens.get(&S2), Some(&gen1));
    assert!(!s.calls.contains(&"commit"));
}
