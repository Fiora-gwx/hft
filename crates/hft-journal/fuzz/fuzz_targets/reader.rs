//! JNL-118: the input is one segment file (zero-padded to 4 096 B): `JournalReader` from `Oldest`
//! to `End`/`Err`, `verify`, and the recovery tail scan (`JournalWriter::open`) must not panic,
//! loop forever or exceed `reassembly_cap`.
#![no_main]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hft_journal::*;
use hft_rt::{Backing, spsc};
use libfuzzer_sys::fuzz_target;

static N: AtomicU64 = AtomicU64::new(0);

fn jnl_118_fuzz_reader(data: &[u8]) {
    let n = N.fetch_add(1, Ordering::Relaxed);
    let hot = std::env::temp_dir().join(format!("jnl118-hot-{}-{n}", std::process::id()));
    let cold = std::env::temp_dir().join(format!("jnl118-cold-{}-{n}", std::process::id()));
    testing::mem_root(&hot);
    let jdir = hot.join("prod").join("seq");
    let mut seg = data.to_vec();
    if seg.len() < 4096 {
        seg.resize(4096, 0);
    }
    let path: PathBuf = jdir.join("0000000001.jrn");
    let _ = testing::create(&path, &seg);
    // Reader to End/Err (bounded).
    let mut rc = ReaderConfig::new(hot.clone(), cold.clone(), InstanceName::Prod, "seq".into());
    rc.end = EndAt::LastCompleteEvent;
    rc.reassembly_cap = 1 << 20;
    if let Ok(mut r) = JournalReader::open(&rc) {
        for _ in 0..100_000 {
            match r.poll() {
                Next::Event(e) if e.body.len() > 1 << 20 => panic!("reassembly over cap"),
                Next::Record(_) | Next::Event(_) => {}
                _ => break,
            }
        }
        let _ = r.last_complete_event();
    }
    let _ = verify(&jdir, &VerifyOpts::default());
    // Recovery tail scan.
    let cfg = JournalConfig {
        name: "seq".into(),
        kind: JournalKind::Seq,
        venue: hft_types::VenueId(0),
        instance: InstanceName::Prod,
        hot_dir: hot.clone(),
        cold_root: cold.clone(),
        segment_bytes: 1 << 20,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: 1,
    };
    if let (Ok((_, prx)), Ok((rtx, _))) = (spsc(4, Backing::Heap), spsc(64, Backing::Heap)) {
        let env = WriterEnv {
            writer_epoch: 1,
            build_git: [0; 20],
            host_profile_sha256: [0; 32],
            prepared: prx,
            retired: rtx,
        };
        let _ = JournalWriter::open(&cfg, env);
    }
    testing::mem_purge(&hot);
    let _ = std::fs::remove_dir_all(&cold);
}

fuzz_target!(|data: &[u8]| jnl_118_fuzz_reader(data));
