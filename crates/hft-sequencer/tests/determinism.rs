//! Journal ordering and deterministic source constraints (SEQ-012/310).
use crate::common::*;
use hft_event::Role;
use hft_sequencer::driver::live::RingId;
use proptest::prelude::*;
use std::collections::VecDeque;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    #[test]
    fn seq_012_take_order_is_journal_order(
        arrivals in prop::collection::vec((0usize..8, 0u8..8), 1..100),
        work in 0u64..20_001,
    ) {
        reset_script();
        let mut b = b0();
        b.seq.conflate_backlog = u16::MAX;
        let mut h = b.boot();
        h.steady();
        h.io().tsc_per_take = work;
        h.io().popped = Some(Vec::new());
        let rings = [RingId::Ctl, RingId::Rep(0), RingId::Rep(1), RingId::Bbo(0),
            RingId::Bbo(1), RingId::Advice, RingId::Md(0), RingId::RepSim];
        let mut expected: [VecDeque<Elem>; 8] = std::array::from_fn(|_| VecDeque::new());
        let mark = h.mark();
        let taken = h.s.driver().taken_log.len();
        // Arrivals between rounds, with per-take processing time and all driver timers active.
        for batch in 0..8 {
            for (id, &(ri, at)) in arrivals.iter().enumerate() {
                if at != batch { continue; }
                let e = match ri {
                    0 => h.cmd_elem("cli:test", 4, AC1, &hft_event::ControlCmd::SnapshotNow, |_| {}).0,
                    3 | 4 => elem(&bbo(if ri == 3 { A1 } else { B1 }, if ri == 3 { A } else { B },
                        0, "50000", "50002", id as u64 + 1000, h.now())),
                    6 => elem(&trade(A1, id as u64 + 1000)),
                    _ => elem(&conn(A, 1, hft_event::ConnStateKind::Alive,
                        hft_event::ConnRole::Md, hft_types::InstId(0), AC1, id as u64 + 1000)),
                };
                h.push_e(rings[ri], e.clone());
                expected[ri].push_back(e);
            }
            h.step();
        }
        h.run();
        let records = h.since(mark);
        let inputs: Vec<_> = records.iter().filter(|r| r.role() == Role::Input).collect();
        let takes = &h.s.driver().taken_log[taken..];
        let mut pops = h.io_ref().popped.as_ref().unwrap().iter();
        prop_assert_eq!(inputs.len(), takes.len());
        for (record, (kind, ring)) in inputs.iter().zip(takes) {
            prop_assert_eq!(record.kind, *kind);
            if ring.is_some() {
                // Driver hand-over diagnostics identify the ring class; the IO boundary
                // independently records the actual round-robin member.
                let (ring, popped) = pops.next().unwrap();
                let ri = rings.iter().position(|r| r == ring).unwrap();
                let next = expected[ri].pop_front().expect("each ring element taken at most once");
                prop_assert_eq!(&popped.body, &next.body);
                prop_assert_eq!(&record.body, &next.body);
            }
        }
        prop_assert!(pops.next().is_none());
        prop_assert!(inputs.windows(2).all(|w| w[0].seq < w[1].seq));
        for (ri, pending) in expected.iter().enumerate() {
            prop_assert_eq!(pending.len(), h.io().ring(rings[ri]).len());
        }
        prop_assert!(h.verify().is_empty());
    }
}

#[test]
fn seq_310_determinism_lints() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let cfg = std::fs::read_to_string(root.join("clippy.toml")).unwrap();
    let template = std::fs::read_to_string(root.join("../../tools/lint/clippy-hot.toml")).unwrap();
    assert_eq!(cfg.replace("\r\n", "\n"), template.replace("\r\n", "\n"));
    for forbidden in [
        "std::time::Instant::now",
        "std::time::SystemTime::now",
        "std::collections::HashMap",
        "std::thread_local",
        "rand::thread_rng",
    ] {
        assert!(cfg.contains(&format!("path = \"{forbidden}\"")), "{forbidden}");
    }
    let lib = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert!(lib.contains("#![forbid(unsafe_code)]"));
    assert!(lib.contains("clippy::float_arithmetic"));
    // The CI clippy/hot-check gates enforce these declarations. Independently scan the
    // decision modules so unused floating-point declarations are caught as well.
    for module in [
        "intents.rs",
        "process.rs",
        "strategies.rs",
        "exec_glue.rs",
        "mode_glue.rs",
    ] {
        let src = std::fs::read_to_string(root.join("src").join(module)).unwrap();
        for line in src.lines().map(|l| l.split("//").next().unwrap()) {
            let words: Vec<_> = line.split(|c: char| !c.is_alphanumeric() && c != '_').collect();
            for bad in ["f64", "f32", "HashMap", "thread_local", "SystemTime", "Instant"] {
                assert!(!words.contains(&bad), "{module}: {line}");
            }
            assert!(!line.contains("static mut"));
        }
    }
    let live = std::fs::read_to_string(root.join("src/driver/live.rs")).unwrap();
    assert!(live.contains("self.io.tsc()"));
}
