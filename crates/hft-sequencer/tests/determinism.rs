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
        arrivals in prop::collection::vec(prop::collection::vec(0u64..=5 * MS, 0..=300), 8),
        timers in prop::collection::vec(0u64..=5 * MS, 0..=20),
        work in 0u64..20_001,
    ) {
        reset_script();
        let mut b = b0();
        b.seq.conflate_backlog = u16::MAX;
        let mut h = b.boot();
        h.steady();
        add_hook(move |hx, ev, ctx| {
            if hx.s.strategy == S1
                && let hft_strategy_sdk::EventRef::Bbo { b, .. } = ev
                && let Some(batch) = ["50004", "50006", "50008"].iter().position(|p| b.ask_px == px(p))
            {
                // The strategy Ctx permits eight timer operations per delivery.
                for (tag, &delay) in timers.iter().enumerate().skip(batch * 8).take(8) {
                    ctx.timer_at(hft_types::Tsc(hx.v.now().tsc.0 + delay), tag as u32, G1).unwrap();
                }
            }
        });
        for ask in ["50004", "50006", "50008"] {
            h.quote(A1, "50000", ask);
            h.run();
        }
        h.io().tsc_per_take = work;
        h.io().popped = Some(Vec::new());
        let rings = [RingId::Ctl, RingId::Rep(0), RingId::Rep(1), RingId::Bbo(0),
            RingId::Bbo(1), RingId::Advice, RingId::Md(0), RingId::RepSim];
        let mut expected: [VecDeque<Elem>; 8] = std::array::from_fn(|_| VecDeque::new());
        let mark = h.mark();
        let taken = h.s.driver().taken_log.len();
        let start = h.now();
        let mut scheduled: Vec<_> = arrivals.iter().enumerate()
            .flat_map(|(ri, times)| times.iter().map(move |&at| (at, ri))).collect();
        scheduled.sort_unstable();
        let mut arrived = 0;
        // A round observes arrivals due at its boundary; work advances the clock so later
        // arrivals can accumulate into a burst. Each ring independently has 0..=300 entries.
        while arrived < scheduled.len() {
            h.at(h.now().max(start + scheduled[arrived].0));
            while arrived < scheduled.len() && start + scheduled[arrived].0 <= h.now() {
                let id = arrived;
                let ri = scheduled[arrived].1;
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
                arrived += 1;
            }
            h.step();
        }
        h.at(h.now().max(start + 5 * MS));
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
    lint_counterexamples(root, &cfg);
}

/// Run the real Clippy driver serially, without recursively starting Cargo. The fixture uses
/// the production config and crate lint attributes; an external rand stub supplies only the
/// forbidden symbol, so the test needs neither a network fetch nor a new engine dependency.
fn lint_counterexamples(root: &std::path::Path, cfg: &str) {
    use std::process::Command;
    let tool = |name: &str| {
        let p = std::path::Path::new(env!("CARGO"))
            .with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        if p.is_file() {
            p
        } else {
            let out = Command::new("rustup").args(["which", name]).output().unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            std::path::PathBuf::from(String::from_utf8(out.stdout).unwrap().trim())
        }
    };
    let dir = root.join(format!("../../target/seq-310-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("clippy.toml"), cfg).unwrap();
    std::fs::write(dir.join("rand.rs"), "pub fn thread_rng() -> u64 { 1 }\n").unwrap();
    let out = Command::new(tool("rustc"))
        .args([
            "--crate-name",
            "rand",
            "--crate-type",
            "rlib",
            "rand.rs",
            "-o",
            "librand.rlib",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rows = [
        (
            "pub fn tick() { let _ = std::time::Instant::now(); }",
            "disallowed_methods",
        ),
        (
            "pub fn tick() { let _ = std::time::SystemTime::now(); }",
            "disallowed_methods",
        ),
        (
            "pub fn tick(m: &std::collections::HashMap<u8,u8>) { for _ in m.iter() {} }",
            "disallowed_types",
        ),
        (
            "std::thread_local! { pub static X: u64 = const { 0 }; }",
            "disallowed_macros",
        ),
        (
            "static mut X: u64 = 0; pub fn tick() -> u64 { unsafe { X } }",
            "unsafe_code",
        ),
        (
            "pub fn tick() { let _ = rand::thread_rng(); }",
            "disallowed_methods",
        ),
        ("pub fn tick(x: f64) -> f64 { x + 1.0 }", "float_arithmetic"),
        (
            "pub trait Io { fn tsc(&self) -> u64; } pub fn tick(io: &impl Io) -> u64 { io.tsc() }",
            "",
        ),
    ];
    for (row, (body, diagnostic)) in rows.iter().enumerate() {
        std::fs::write(
            dir.join("probe.rs"),
            format!("#![forbid(unsafe_code)]\n#![deny(clippy::float_arithmetic)]\n{body}\n"),
        )
        .unwrap();
        let out = Command::new(tool("clippy-driver"))
            .args([
                "--edition=2024",
                "--crate-name",
                "probe",
                "--crate-type",
                "lib",
                "--emit=metadata",
                "probe.rs",
                "--extern",
                "rand=librand.rlib",
                "-D",
                "warnings",
            ])
            .env("CLIPPY_CONF_DIR", &dir)
            .current_dir(&dir)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            out.status.success(),
            diagnostic.is_empty(),
            "row {}: {err}",
            row + 1
        );
        if !diagnostic.is_empty() {
            assert!(err.contains(diagnostic), "row {}: {err}", row + 1);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
