//! Offline writer for the replayer (JNL-101).

mod common;

use common::harness::*;
use hft_journal::*;
use hft_types::Tsc;

#[test]
fn jnl_101_open_offline() {
    let hot = linux_hot();
    let cold = cold_dir();
    let cfg = c0(&hot, &cold);
    let out = disk_dir("offline");
    let mut w = JournalWriter::open_offline(&cfg, &out).expect("JNL-101.1");
    let info = RollInfo {
        utc_ns: UTC0,
        calib: calib0(),
        tsc_hz_milli: 0,
        configs: &[],
    };
    w.roll(RollReason::Startup, &info).unwrap().finish().unwrap();
    for i in 0..1000u64 {
        w.append(F0, Tsc(i), &tin(i as u8)).unwrap();
        w.end_event();
        if w.needs_roll(Tsc(0)).is_some() {
            w.publish();
            w.roll(RollReason::Size, &info).unwrap().finish().unwrap();
        }
    }
    w.publish();
    w.seal_and_close().unwrap();
    let jdir = out.join("prod").join("seq");
    let names: Vec<String> = std::fs::read_dir(&jdir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|n| n.len() == 14 && n.ends_with(".jrn")),
        "JNL-101.1: {names:?}"
    );
    let last = std::fs::read(jdir.join(names.iter().max().unwrap())).unwrap();
    assert_eq!(
        roll_body(&last, last.len() - 72).mark,
        1,
        "JNL-101.1: ends with End"
    );
    let mut c = ReaderConfig::new(out.clone(), out.join("no-cold"), InstanceName::Prod, "seq".into());
    c.end = EndAt::LastCompleteEvent;
    let mut r = JournalReader::open(&c).unwrap();
    let got = drain(&mut r);
    assert_eq!(got.iter().filter(|x| x.1 == 0x7F01).count(), 1000, "JNL-101.1");
    assert!(matches!(r.poll(), Next::End), "JNL-101.1");

    #[cfg(target_os = "linux")]
    let shm_out = {
        let d = shm_hot().join("out");
        std::fs::create_dir_all(&d).unwrap();
        d
    };
    #[cfg(not(target_os = "linux"))]
    let shm_out = mem_hot().join("out");
    assert_eq!(
        JournalWriter::open_offline(&cfg, &shm_out).err(),
        Some(OpenErr::OfflineTarget),
        "JNL-101.2"
    );
    assert_eq!(
        JournalWriter::open_offline(&cfg, &cold.join("replay")).err(),
        Some(OpenErr::OfflineTarget),
        "JNL-101.3"
    );
    #[cfg(target_os = "linux")]
    assert_eq!(
        JournalWriter::open_offline(&cfg, std::path::Path::new("/dev/shm/hft/x")).err(),
        Some(OpenErr::OfflineTarget),
        "JNL-101.4"
    );
}
