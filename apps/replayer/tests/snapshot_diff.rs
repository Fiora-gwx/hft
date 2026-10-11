//! A complete generated hour, written by the live sequencer/advisor/SimExchange fixture.
//! Mutations keep transport checks valid so the independent replay, not CRC, detects them.

use hft_event::{EventKind, PartHdr, RecordFlags, RecordHeader, Role, SnapshotHdr};
use hft_strategy_sdk::SnapshotReader;
use serde_json::{Value, json};
use zerocopy::FromBytes;

use super::{fixture, long_model};

fn section(body: &[u8], id: u16) -> (usize, u32, usize) {
    let mut r = SnapshotReader::new(&body[96..]);
    loop {
        let at = 96 + r.pos();
        let s = r.next_section().unwrap().unwrap();
        if s.section == id {
            return (at, s.count, s.body.len());
        }
    }
}

fn change(original: &[u8], seq: u64, changes: &[(usize, u8)]) -> Vec<u8> {
    let mut bytes = original.to_vec();
    let mut at = 4096;
    loop {
        let (h, _) = RecordHeader::read_from_prefix(&bytes[at..]).unwrap();
        assert!(h.len > 0);
        if h.seq == seq {
            let begin = at + 24;
            let end = at + h.len as usize;
            let body = if h.kind == EventKind::STATE_SNAPSHOT.0 {
                let (part, _) = PartHdr::read_from_prefix(&bytes[begin..]).unwrap();
                assert_eq!((part.part, part.parts), (0, 1));
                assert_eq!(part.total_len, part.chunk_len);
                begin + 16
            } else {
                begin
            };
            for &(offset, value) in changes {
                assert!(body + offset < end - 4);
                bytes[body + offset] = value;
            }
            assert!(RecordFlags(h.flags).contains(RecordFlags::CRC));
            let crc = crc32c::crc32c(&bytes[at..end - 4]);
            bytes[end - 4..end].copy_from_slice(&crc.to_le_bytes());
            hft_event::decode_record(&bytes[at..end]).unwrap();
            return bytes;
        }
        at += h.len as usize;
    }
}

fn execute(f: &fixture::Fixture, label: &str, extra: &[String]) -> (i32, Value, String) {
    let mut a = vec![
        "--report".into(),
        f.root.join(format!("{label}.json")).display().to_string(),
    ];
    a.extend_from_slice(extra);
    let out = long_model::execute(f, false, &a);
    if out.0 == 0 || out.0 == 1 {
        let summary: Value = serde_json::from_slice(&std::fs::read(&a[1]).unwrap()).unwrap();
        assert_eq!(summary, out.1["summary"]);
        let side: Value =
            serde_json::from_slice(&std::fs::read(f.root.join(format!("{label}.divergence.json"))).unwrap())
                .unwrap();
        assert_eq!(side["divergences"], out.1["divergences"]);
    }
    out
}

fn replace(f: &fixture::Fixture, bytes: &[u8]) {
    std::fs::write(&f.file, bytes).unwrap();
    fixture::seal(&f.file);
}

#[test]
fn rpl_279_generated_hour_snapshot_component_and_field_diagnostics() {
    let f = fixture::path_model(1);
    let original = std::fs::read(&f.file).unwrap();
    let log = fixture::read(&f);
    let keyframe = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT && r.read::<SnapshotHdr>().unwrap().purpose == 2)
        .nth(1)
        .unwrap();
    let end = log.recs[log.seg_start[1]].seq;
    assert!(keyframe.seq < end);
    let base = execute(&f, "snapshot-baseline", &[]);
    assert_eq!(base.0, 0, "{base:?}");
    assert_eq!(base.1["summary"]["component"], Value::Null);
    assert!(base.1["summary"]["outputs_compared"].as_u64().unwrap() > 100);
    let boot = base.1["summary"]["start_snapshot_seq"].as_u64().unwrap();
    let prefix_outputs = log
        .recs
        .iter()
        .filter(|r| r.seq > boot && r.seq <= keyframe.seq && r.role() == Role::Output)
        .count();
    let mut evidence = Vec::new();
    for (id, offset, owner, field, width) in [
        (4, 24, Some("hft-risk"), "sections[4].rows[0].set_seq", 8),
        (4, 10, Some("hft-risk"), "sections[4].rows[0].scope.account", 1),
        (13, 0, Some("hft-ledger"), "sections[13].body[0]", 1),
        (3, 0, Some("hft-oms"), "sections[3].body[0]", 1),
        (5, 0, Some("strategies"), "sections[5].body[0]", 1),
        (7, 0, Some("hft-book"), "sections[7].body[0]", 1),
        (21, 0, Some("hft-exec"), "sections[21].body[0]", 1),
        (25, 0, Some("hft-sequencer"), "sections[25].body[0]", 1),
        (16, 0, None, "sections[16].body[0]", 1),
    ] {
        let (start, count, len) = section(&keyframe.body, id);
        assert!(count > 0 && offset + width <= len);
        let at = start + 16 + offset;
        let byte = keyframe.body[at].wrapping_add(1);
        let changed = change(&original, keyframe.seq, &[(at, byte)]);
        replace(&f, &changed);
        // Independently read the changed logical record through the formal reader. A
        // mistaken physical offset must not turn this into an unrelated metadata test.
        let reread = fixture::read(&f);
        let actual = &reread.recs.iter().find(|r| r.seq == keyframe.seq).unwrap().body;
        let mut wanted = keyframe.body.clone();
        wanted[at] = byte;
        assert_eq!(*actual, wanted);
        let label = format!("snapshot-{id}-{offset}");
        let out = execute(&f, &label, &[]);
        assert_eq!(out.0, 1, "{out:?}");
        assert_eq!(out.1["summary"]["verify"], "fail");
        assert_eq!(out.1["summary"]["component"], json!(owner));
        assert_eq!(out.1["summary"]["first_divergent_seq"], keyframe.seq);
        assert_eq!(out.1["summary"]["outputs_compared"], prefix_outputs);
        assert_eq!(out.1["divergences"].as_array().unwrap().len(), 1);
        let d = &out.1["divergences"][0];
        assert_eq!(d["kind"], EventKind::STATE_SNAPSHOT.0);
        assert_eq!(d["snapshot"]["first"]["field"], field);
        assert_eq!(d["snapshot"]["first"]["expected_offset"], at);
        assert_eq!(d["snapshot"]["first"]["actual_offset"], at);
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(
            d["snapshot"]["first"]["expected_hex"],
            hex(&wanted[at..at + width])
        );
        assert_eq!(
            d["snapshot"]["first"]["actual_hex"],
            hex(&keyframe.body[at..at + width])
        );
        assert_eq!(d["snapshot"]["components"].as_array().unwrap().len(), 6);
        assert!(
            d["field_diff"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["field"] == field)
        );
        assert_eq!(std::fs::read(&f.file).unwrap(), changed);
        evidence.push(
            json!({"label": label, "seq": keyframe.seq, "offset": at, "value": byte,
            "component": owner, "field": field, "prefix_outputs": prefix_outputs}),
        );
    }
    // A later risk snapshot must not relabel an earlier ordinary Output mismatch.
    let risk = section(&keyframe.body, 4).0 + 16 + 24;
    let changed = change(&original, keyframe.seq, &[(risk, keyframe.body[risk] ^ 1)]);
    let cmd = log
        .recs
        .iter()
        .find(|r| r.seq > boot && r.seq < keyframe.seq && r.kind == EventKind::CMD_RESULT)
        .unwrap();
    let changed = change(&changed, cmd.seq, &[(0, cmd.body[0] ^ 1)]);
    replace(&f, &changed);
    let out = execute(&f, "snapshot-earlier-output", &["--keep-going".into()]);
    assert_eq!(out.0, 1, "{out:?}");
    assert_eq!(out.1["summary"]["first_divergent_seq"], cmd.seq);
    assert_eq!(out.1["summary"]["component"], Value::Null);
    assert!(
        out.1["divergences"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["component"] == "hft-risk")
    );
    replace(&f, &original);
    assert_eq!(execute(&f, "snapshot-restored", &[]).0, 0);
    if let Some(root) = std::env::var_os("HFT_REPLAY_EVIDENCE_DIR") {
        let path = std::path::PathBuf::from(root)
            .join(f.root.file_name().unwrap())
            .join("snapshot-mutations.json");
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}

#[test]
fn rpl_280_malformed_snapshot_diagnostics_do_not_invent_components() {
    let f = fixture::path_model(1);
    let original = std::fs::read(&f.file).unwrap();
    let log = fixture::read(&f);
    let k = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT && r.read::<SnapshotHdr>().unwrap().purpose == 2)
        .unwrap();
    let section = section(&k.body, 4).0;
    for (label, changes, reason, offset) in [
        ("reserved", vec![(section + 12, 1)], "SectionFraming", section),
        ("length", vec![(section + 8, 1)], "SectionFraming", section),
        ("duplicate", vec![(section, 13)], "DuplicateSection", section),
        ("count", vec![(4, k.body[4] + 1)], "SectionCount", 4),
    ] {
        let changed = change(&original, k.seq, &changes);
        replace(&f, &changed);
        let out = execute(&f, &format!("snapshot-bad-{label}"), &[]);
        assert_eq!(out.0, 1, "{out:?}");
        assert_eq!(out.1["summary"]["component"], Value::Null);
        assert_eq!(out.1["summary"]["first_divergent_seq"], k.seq);
        let d = &out.1["divergences"][0]["snapshot"];
        assert_eq!(d["error"]["side"], "Logged");
        assert_eq!(d["error"]["reason"], reason);
        assert_eq!(d["error"]["offset"], offset);
        let from = vec!["--from-seq".into(), k.seq.to_string()];
        let out = execute(&f, &format!("snapshot-start-bad-{label}"), &from);
        assert_eq!(out.0, 3, "{out:?}");
        assert_eq!(std::fs::read(&f.file).unwrap(), changed);
    }
}
