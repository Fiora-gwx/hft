//! CTL-080–CTL-086.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use common::*;
use hft_ctl::ParsedConfig;
use hft_event::{ControlCmd, EventKind};
use hft_refdata::{AccountSpec, AccountVenueSpec, InstStatus, InstanceSpec, UidPolicy};
use hft_types::{AccountId, StrategyId, VenueId};
use serde_json::{Value, json};

thread_local! {
    static FREES: Cell<u64> = const { Cell::new(0) };
}

/// Counts deallocations per thread (CTL-086).
struct CountFrees;

// SAFETY: forwards to the system allocator; the counter is a thread-local `Cell`.
unsafe impl GlobalAlloc for CountFrees {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        let _ = FREES.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: CountFrees = CountFrees;

fn sp(blob: &[u8]) -> Value {
    json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(blob)})
}

fn send(g: &mut G0, issuer: &str, account: &str, cmd: Value) -> (hft_ctl_proto::CmdResponse, Vec<RingRec>) {
    let n = g.seen.len();
    let r = g.send(issuer, Some(account), cmd);
    g.take_ring();
    (r, g.seen[n..].to_vec())
}

fn take_params(g: &mut G0) -> Option<Box<hft_ctl::ParsedParams<CtlTestSet>>> {
    match g.mb_take.try_take() {
        Some(ParsedConfig::StrategyParams(p)) => Some(p),
        Some(o) => panic!("{o:?}"),
        None => None,
    }
}

#[test]
fn ctl_080_container_and_binding() {
    let base = Spb::s1(42);
    let mut g = G0::new();
    g.paused = true;
    let (_, recs) = send(&mut g, "cli:tom", "acct-a", sp(&base.bytes()));
    assert!(recs.iter().any(RingRec::is_cmd), "CTL-080.1");
    assert!(take_params(&mut g).is_some(), "CTL-080.1");
    let mut rows: Vec<(&str, Vec<u8>, &str, &str)> = Vec::new();
    let mut s = base.clone();
    s.groups[0].n_legs = 0;
    rows.push(("CTL-080.2", s.bytes(), "BadGroup", ""));
    let mut s = base.clone();
    s.groups[0].id = hft_types::GroupId((2 << 24) | 101);
    rows.push(("CTL-080.3", s.bytes(), "BadGroup", ""));
    let mut s = base.clone();
    s.kind_id = 0xFF02;
    s.digest = set::digest(0xFF02);
    rows.push(("CTL-080.4", s.bytes(), "KindMismatch", ""));
    let mut s = base.clone();
    s.groups[0] = group(G1, &[leg(1, 1), leg(2, 2)]);
    rows.push(("CTL-080.5", s.bytes(), "GroupCheck", "LegVenueNotInAccount"));
    let mut s = base.clone();
    s.groups[0] = group(G1, &[leg(1, 1), leg(1, 1)]);
    rows.push(("CTL-080.6", s.bytes(), "SameExchangeLegs", ""));
    let mut s = base.clone();
    s.groups.push(g_s1(G1));
    rows.push(("CTL-080.8", s.bytes(), "GroupIdDup", ""));
    let mut s = base.clone();
    s.kind_id = 0xFF7F;
    rows.push(("CTL-080.9", s.bytes(), "SchemaUnsupported", ""));
    let mut s = base.clone();
    s.schema_ver = 2;
    rows.push(("CTL-080.10", s.bytes(), "SchemaUnsupported", ""));
    let mut s = base.clone();
    s.digest ^= 1;
    rows.push(("CTL-080.11", s.bytes(), "SchemaMismatch", ""));
    for (row, blob, name, arg) in rows {
        let mut g = G0::new();
        g.paused = true;
        let (r, recs) = send(&mut g, "cli:tom", "acct-a", sp(&blob));
        assert_eq!(
            (code(&r), detail_name(&r).as_str(), detail_arg(&r).as_str()),
            ("InvalidArgs", name, arg),
            "{row}"
        );
        // CTL-080.12
        assert!(recs.iter().all(RingRec::is_note), "CTL-080.12 {row}");
        assert_eq!(only_audit(&recs).result_code, 20, "CTL-080.12 {row}");
        assert!(take_params(&mut g).is_none(), "CTL-080.12 {row}");
    }
    // CTL-080.7: first leg's inst delisted.
    let mut spec = p0::p0_spec();
    spec.insts[0].status = InstStatus::Delisted;
    let mut g = G0::with(spec.load(), 262_144, g0_active());
    let (r, recs) = send(&mut g, "cli:tom", "acct-a", sp(&base.bytes()));
    assert_eq!(
        (detail_name(&r).as_str(), detail_arg(&r).as_str()),
        ("GroupCheck", "LegNotTradable"),
        "CTL-080.7"
    );
    assert_eq!(only_audit(&recs).result_code, 20, "CTL-080.12 CTL-080.7");
}

/// P0 plus instance `s3-np` (id 4, kind 0xFF03) bound to account `acct-p` (id 4).
fn rd_np(is_prod: bool) -> hft_refdata::RefData {
    let mut spec = p0::p0_spec();
    spec.instances.push(InstanceSpec {
        id: StrategyId(4),
        kind_id: 0xFF03,
        name: "s3-np".into(),
    });
    spec.accounts.push(AccountSpec {
        id: AccountId(4),
        sim: false,
        bound_instance: StrategyId(4),
        is_prod,
        venues: p0::vbits(&[VenueId::BINANCE_USDM, VenueId::BITGET_UTA_SPOT]),
        label: "acct-p".into(),
        uid_policy: UidPolicy::Verified,
    });
    for (v, uid) in [
        (VenueId::BINANCE_USDM, "100000004"),
        (VenueId::BITGET_UTA_SPOT, "200000004"),
    ] {
        spec.account_venues.push(AccountVenueSpec {
            account: AccountId(4),
            venue: v,
            uid: uid.into(),
        });
    }
    spec.load()
}

#[test]
fn ctl_081_body_and_budget() {
    let base = Spb::s1(42);
    let run = |g: &mut G0, account: &str, strategy: &str, blob: Vec<u8>| {
        send(
            g,
            "cli:tom",
            account,
            json!({"type":"SetParams","strategy":strategy,"blob_b64":b64(&blob)}),
        )
    };
    // .1 body one field short (container length updated).
    let mut b = base.bytes();
    b.truncate(b.len() - 8);
    let bl = u32::from_le_bytes(b[32..36].try_into().unwrap()) - 8;
    b[32..36].copy_from_slice(&bl.to_le_bytes());
    let mut g = G0::new();
    let (r, _) = run(&mut g, "acct-a", "s1-main", b);
    assert_eq!(
        (detail_name(&r).as_str(), detail_arg(&r).as_str()),
        ("ParamInvalid", "Strategy"),
        "CTL-081.1"
    );
    let mut s = base.clone();
    s.values[F_BAD] = 1;
    let (r, _) = run(&mut g, "acct-a", "s1-main", s.bytes());
    assert_eq!(detail_arg(&r), "Strategy:2", "CTL-081.2");
    for (row, n, ok) in [("CTL-081.3", 9u32, false), ("CTL-081.4", 8, true)] {
        let mut g = G0::new();
        let mut s = base.clone();
        s.groups = (0..n).map(|k| g_s1((1 << 24) | (101 + k))).collect();
        let (r, recs) = run(&mut g, "acct-a", "s1-main", s.bytes());
        if ok {
            assert!(recs.iter().any(RingRec::is_cmd), "{row}: {r:?}");
        } else {
            assert_eq!(
                (detail_name(&r).as_str(), detail_arg(&r).as_str()),
                ("ParamInvalid", "Strategy"),
                "{row}"
            );
        }
    }
    for (row, prod, ok) in [("CTL-081.5", true, false), ("CTL-081.6", false, true)] {
        let mut g = G0::with(rd_np(prod), 262_144, g0_active());
        let mut s = base.clone();
        s.strategy = 4;
        s.account = 4;
        s.kind_id = 0xFF03;
        s.digest = set::digest(0xFF03);
        s.groups = vec![g_s1((4 << 24) | 101)];
        let (r, recs) = run(&mut g, "acct-p", "s3-np", s.bytes());
        if ok {
            assert!(recs.iter().any(RingRec::is_cmd), "{row}: {r:?}");
        } else {
            assert_eq!(
                (detail_name(&r).as_str(), detail_arg(&r).as_str()),
                ("ParamInvalid", "NonProdOnly"),
                "{row}"
            );
        }
    }
    let budget = |f: usize, v: i64| {
        let mut g = G0::new();
        g.paused = true;
        let mut s = base.clone();
        s.values[f] = v;
        let (r, recs) = run(&mut g, "acct-a", "s1-main", s.bytes());
        (r, recs.iter().any(RingRec::is_cmd), take_params(&mut g))
    };
    let (_, w, p) = budget(F_MEM, 268_435_456);
    assert!(w, "CTL-081.7");
    assert_eq!(p.unwrap().model_mem, 268_435_456, "CTL-081.7");
    let (r, w, _) = budget(F_MEM, 268_435_457);
    assert!(!w && detail_arg(&r) == "ModelMemBudget", "CTL-081.8 {r:?}");
    assert!(budget(F_MEM, 268_435_455).1, "CTL-081.9");
    assert!(budget(F_CKPT, 536_870_912).1, "CTL-081.10");
    let (r, w, _) = budget(F_CKPT, 536_870_913);
    assert!(!w && detail_arg(&r) == "ModelMemBudget", "CTL-081.11 {r:?}");
}

#[test]
fn ctl_082_write_order_and_mailbox() {
    let s = big_spb();
    let blob = s.bytes();
    assert!(blob.len() >= 180_000);
    let mut g = G0::big();
    g.paused = true;
    let cmd = json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&blob),"base_version":"41"});
    let (_, recs) = send(&mut g, "cli:tom", "acct-a", cmd);
    let at = recs.iter().position(RingRec::is_cmd).unwrap();
    let mut logical = Vec::new();
    for r in &recs[..at] {
        assert_eq!(r.kind, EventKind::CONFIG_BLOB);
        assert!(8 + r.body.len() <= 65_536);
        logical.extend_from_slice(&r.body[16..]);
    }
    assert!(at >= 3, "several PART segments");
    assert_eq!(&logical[72..], &blob[..]);
    let (h, c) = recs[at].cmd();
    let sha: [u8; 32] = <sha2::Sha256 as sha2::Digest>::digest(&blob).into();
    match c {
        ControlCmd::SetParams { blob: b, .. } => assert_eq!(b.sha256, sha),
        c => panic!("{c:?}"),
    }
    assert_eq!(h.base_version, 41);
    assert_eq!(only_audit(&recs).result_code, 0);
    let p = take_params(&mut g).unwrap();
    assert_eq!(p.sha256, sha);
    assert!(p.groups.gix_of(hft_types::GroupId(G1)).is_some());
    assert_eq!((p.model_mem, p.model_ckpt), (1_000, 1_000));
    let _ = (p.structural_changed, p.reset_preview);
}

#[test]
fn ctl_083_structural_changed() {
    let rows: [(&str, i64, i64, bool, u32); 3] = [
        ("CTL-083.1", 10, 6, false, 0),
        ("CTL-083.2", 11, 5, true, 1),
        ("CTL-083.3", 10, 7, false, 0),
    ];
    for (row, win, thr, structural, reset) in rows {
        let mut g = G0::new();
        g.paused = true;
        let mut s = Spb::s1(42);
        s.values[F_WIN] = win;
        s.values[F_THR] = thr;
        send(&mut g, "cli:tom", "acct-a", sp(&s.bytes()));
        let p = take_params(&mut g).unwrap();
        assert_eq!(
            (p.structural_changed, p.reset_preview.reset_groups),
            (structural, reset),
            "{row}"
        );
    }
}

#[test]
fn ctl_084_baseline_tracks_applied() {
    let mut g = G0::new();
    g.paused = true;
    let spb = |v: u64, win: i64, thr: i64| {
        let mut s = Spb::s1(v);
        s.values[F_WIN] = win;
        s.values[F_THR] = thr;
        s
    };
    // #1 v42 changes win.
    let (_, recs) = send(&mut g, "cli:tom", "acct-a", sp(&spb(42, 11, 5).bytes()));
    assert!(recs.iter().any(RingRec::is_cmd));
    assert!(take_params(&mut g).unwrap().structural_changed);
    g.step();
    // #2 while #1's result is outstanding.
    let (r, recs) = send(&mut g, "cli:tom", "acct-a", sp(&spb(43, 11, 6).bytes()));
    assert_eq!(status(&r), "busy", "step 2");
    assert!(!recs.iter().any(RingRec::is_cmd), "step 2");
    // #3 another (instance, account).
    let mut s3 = Spb::s1(8);
    s3.strategy = 2;
    s3.account = 2;
    s3.kind_id = 0xFF02;
    s3.digest = set::digest(0xFF02);
    s3.groups = vec![group(G2, &[leg(2, 2), leg(3, 10)])];
    let (_, recs) = send(
        &mut g,
        "cli:tom",
        "acct-b",
        json!({"type":"SetParams","strategy":"s2-main","blob_b64":b64(&s3.bytes())}),
    );
    assert!(recs.iter().any(RingRec::is_cmd), "step 3");
    take_params(&mut g).unwrap();
    g.step();
    // #1 Applied, then #4 keeps #1's win.
    g.answer_all();
    let (_, recs) = send(&mut g, "cli:tom", "acct-a", sp(&spb(43, 11, 6).bytes()));
    assert!(recs.iter().any(RingRec::is_cmd), "step 4");
    assert!(!take_params(&mut g).unwrap().structural_changed, "step 4");
    g.step();
    g.answer_all();
    // #5 changes win but is rejected; #6 keeps #4's win.
    let (_, recs) = send(&mut g, "cli:tom", "acct-a", sp(&spb(44, 12, 6).bytes()));
    assert!(recs.iter().any(RingRec::is_cmd), "step 5");
    take_params(&mut g).unwrap();
    g.step();
    let (id, seq) = g.unanswered.remove(0);
    g.result(
        id,
        seq,
        Outcome {
            status: 20,
            ..Outcome::APPLIED
        },
    );
    let (_, recs) = send(&mut g, "cli:tom", "acct-a", sp(&spb(44, 11, 7).bytes()));
    assert!(recs.iter().any(RingRec::is_cmd), "step 5 #6");
    assert!(!take_params(&mut g).unwrap().structural_changed, "step 5 #6");
}

#[test]
fn ctl_085_mailbox_single_slot() {
    let mut g = G0::new();
    g.paused = true;
    let (_, recs) = send(&mut g, "cli:tom", "acct-a", sp(&Spb::s1(42).bytes()));
    assert!(recs.iter().any(RingRec::is_cmd), "CTL-085.1");
    let mut s2 = Spb::s1(8);
    s2.strategy = 2;
    s2.account = 2;
    s2.kind_id = 0xFF02;
    s2.digest = set::digest(0xFF02);
    s2.groups = vec![group(G2, &[leg(2, 2), leg(3, 10)])];
    let v2 = g.req(
        "cli:tom",
        Some("acct-b"),
        json!({"type":"SetParams","strategy":"s2-main","blob_b64":b64(&s2.bytes())}),
    );
    let n = g.seen.len();
    let r = g.send_and_wait(&v2);
    g.take_ring();
    assert_eq!(status(&r), "busy", "CTL-085.2");
    assert_eq!(only_audit(&g.seen[n..]).result_code, 31, "CTL-085.2");
    let (r, _) = send(
        &mut g,
        "cli:alice",
        "acct-b",
        json!({"type":"SetRisk","blob_b64":b64(&risk_blob(0))}),
    );
    assert_eq!(status(&r), "busy", "CTL-085.3");
    let (_, recs) = send(
        &mut g,
        "cli:tom",
        "acct-a",
        json!({"type":"HedgeFix","group":"16777317"}),
    );
    assert!(recs.iter().any(RingRec::is_cmd), "CTL-085.4");
    take_params(&mut g).unwrap();
    let n = g.seen.len();
    g.send_and_wait(&v2);
    g.take_ring();
    assert!(g.seen[n..].iter().any(RingRec::is_cmd), "CTL-085.5");
}

#[test]
fn ctl_086_retired_freed_in_gateway() {
    let mut g = G0::new();
    let mut drained = 0;
    let mut freed_by_seq = 0u64;
    // Three objects allocated here (the gateway thread), handed back one at a time by a
    // "sequencer" thread through the single-slot Mailbox, freed by `drain_retired`.
    for o in g0_active() {
        let send = &mut g.retired_send;
        freed_by_seq += std::thread::scope(|sc| {
            sc.spawn(move || {
                let before = FREES.with(Cell::get);
                assert!(send.try_send(o).is_ok());
                FREES.with(Cell::get) - before
            })
            .join()
            .unwrap()
        });
        let before = FREES.with(Cell::get);
        drained += g.core.drain_retired();
        assert!(FREES.with(Cell::get) > before, "freed on the gateway thread");
    }
    assert_eq!(drained, 3);
    assert_eq!(freed_by_seq, 0, "the sequencer thread frees nothing");
}
