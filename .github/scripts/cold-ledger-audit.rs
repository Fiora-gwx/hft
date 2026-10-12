//! Read-only audit of the next M2 cold-reader gap against real engine snapshots.
use hft_event::{EventKind, RecordHeader, RecordView};
use hft_ledger::{JournalLedger, Ledger};
use hft_strategy_sdk::SnapshotReader;
use hft_testkit::dst::seq::build;
use hft_testkit::dst::world::WorldSpec;
use hft_testkit::dst::{DstConfig, RunRecord, run_record};
use hft_testkit::scenarios::{DstEngine, suites};
use std::path::Path;

fn money(l: &Ledger, w: &WorldSpec) -> Vec<((u8, u8, u32, u8), i128)> {
    let mut rows = Vec::new();
    for a in &w.accounts {
        for i in &w.insts {
            if a.venues.contains(&i.venue) {
                let p = l.pos(a.id, i.id);
                rows.push(((a.id.0, i.venue.0, i.id.0, 0), i128::from(p.qty.0)));
                rows.push(((a.id.0, i.venue.0, i.id.0, 1), p.cost));
                rows.push(((a.id.0, i.venue.0, i.id.0, 2), p.realized_quote));
                rows.push(((a.id.0, i.venue.0, i.id.0, 3), p.fees_quote));
            }
        }
        for v in &a.venues {
            let b = l.venue_book(a.id, *v);
            for x in b.cash {
                rows.push(((a.id.0, v.0, u32::from(x.asset.0), 4), i128::from(x.total.0)));
            }
            for x in b.inv {
                rows.push(((a.id.0, v.0, u32::from(x.asset.0), 5), i128::from(x.total.0)));
            }
        }
    }
    rows.sort_by_key(|r| r.0);
    rows
}

fn cold_check(c: &DstConfig, r: &RunRecord) -> Result<usize, String> {
    let rd = build::load_refdata(&build::refdata_blob(&c.world, false).unwrap()).unwrap();
    let setup = build::engine_setup(&c.world, c.tsc_hz, 1);
    let caps = setup.caps.to_ledger();
    let start = r
        .journal
        .iter()
        .position(|j| j.kind == EventKind::STATE_SNAPSHOT)
        .ok_or("no initial snapshot")?;
    let mut cold = JournalLedger::from_snapshot(
        &mut SnapshotReader::new(&r.journal[start].body[96..]),
        &rd,
        &setup.ledger,
        &caps,
    )
    .map_err(|e| format!("initial snapshot: {e:?}"))?;
    let mut checks = 0;
    for j in &r.journal[start + 1..] {
        if j.kind == EventKind::STATE_SNAPSHOT {
            let expected = JournalLedger::from_snapshot(
                &mut SnapshotReader::new(&j.body[96..]),
                &rd,
                &setup.ledger,
                &caps,
            )
            .map_err(|e| format!("snapshot {}: {e:?}", j.seq))?;
            let actual = money(cold.ledger(), &c.world);
            let expected = money(expected.ledger(), &c.world);
            if actual != expected {
                return Err(format!(
                    "snapshot {} money mismatch\nactual={actual:?}\nexpected={expected:?}",
                    j.seq
                ));
            }
            checks += 1;
        }
        cold.apply(&RecordView {
            hdr: RecordHeader {
                len: (24 + j.body.len()) as u32,
                kind: j.kind.0,
                flags: j.flags.0,
                seq: j.seq,
                tsc: j.tsc,
            },
            body: &j.body,
        })
        .map_err(|e| format!("apply seq {} kind {}: {e:?}", j.seq, j.kind.0))?;
    }
    Ok(checks)
}

fn main() {
    let all = suites();
    let mut planned = Vec::new();
    for seed in 0xd05e_fd09_f013_185cu64..=0xd05e_fd09_f013_185c + 199 {
        planned.push((seed, hft_testkit::seed::suite_index(seed, all.len())));
    }
    let mut files: Vec<_> = std::fs::read_dir("fixtures/dst-seeds")
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    files.sort();
    for path in files {
        let ix = all
            .iter()
            .position(|s| s.name == path.file_stem().unwrap().to_str().unwrap())
            .unwrap();
        for line in std::fs::read_to_string(path).unwrap().lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let token = line
                .split(|c: char| c.is_whitespace() || c == ',')
                .next()
                .unwrap();
            let seed = match token.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16).unwrap(),
                None => token.parse().unwrap(),
            };
            planned.push((seed, ix));
        }
    }
    assert_eq!(planned.len(), 223);
    let mut failed = 0;
    for (seed, ix) in planned {
        let mut c = all[ix].config(seed);
        c.steps = 10_000;
        let r = run_record::<DstEngine>(&c);
        let result = if r.outcome.is_pass() {
            cold_check(&c, &r)
        } else {
            Err(format!(
                "original DST failed: {:?}",
                r.outcome.failure().map(|f| (&f.step, &f.message))
            ))
        };
        match result {
            Ok(checks) => println!("{seed:#018x}\t{}\tPASS\t{checks}", c.suite),
            Err(message) => {
                failed += 1;
                let dir = Path::new("audit/cold-failures")
                    .join(c.suite)
                    .join(format!("{seed:016x}"));
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("detail.txt"), &message).unwrap();
                std::fs::write(
                    dir.join("full-journal.bin"),
                    hft_testkit::dst::driver::encode_records(&r.journal),
                )
                .unwrap();
                println!(
                    "{seed:#018x}\t{}\tFAIL\t{}",
                    c.suite,
                    message.lines().next().unwrap()
                );
            }
        }
    }
    eprintln!("cold snapshot audit: {failed}/223 failures");
    assert_eq!(failed, 0, "cold-reader gap evidence saved; no acceptance waived");
}
