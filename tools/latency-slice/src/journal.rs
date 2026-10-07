//! The three journals (`seq`, `raw-feed-<venue>-bbo`, `raw-gw-<venue>`): writers, segment
//! preparation, persist and retirement on the `persist` thread, and readers (README "产出").

use std::path::{Path, PathBuf};

use hft_event::{BlobHdr, BlobKind, BlobRef, CmdScope, EventKind, HostProfile};
use hft_journal::{
    HotRetirer, InstanceName, JournalConfig, JournalKind, JournalReader, JournalWriter, PageSize,
    PersistConfig, Persister, PrepEnv, PreparedSegment, ReaderConfig, RetiredSegment, RollInfo, RollReason,
    SegmentPreparer, WriterEnv,
};
use hft_rt::{Backing, CalibPair, Consumer, Producer, spsc};
use hft_types::{Tsc, VenueId};
use sha2::{Digest, Sha256};
use zerocopy::{FromZeros, IntoBytes};

/// The persist-side half of one journal.
pub struct JSide {
    pub cfg: JournalConfig,
    prep: SegmentPreparer,
    retirer: HotRetirer,
}

impl std::fmt::Debug for JSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JSide").field("name", &self.cfg.name).finish()
    }
}

impl JSide {
    /// Prepares the next segment (the persist thread does this every round).
    pub fn prep_poll(&mut self) {
        let _ = self.prep.poll();
    }
}

/// Configuration of one journal under `hot`/`cold`.
pub fn jcfg(
    name: &str,
    kind: JournalKind,
    venue: VenueId,
    hot: &Path,
    cold: &Path,
    seg_mib: u32,
) -> JournalConfig {
    JournalConfig {
        name: name.into(),
        kind,
        venue,
        instance: InstanceName::Prod,
        hot_dir: hot.to_path_buf(),
        cold_root: cold.to_path_buf(),
        segment_bytes: u64::from(seg_mib) * 1_048_576,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: 2,
    }
}

/// Opens a writer and its persist-side half; the first segment is prepared (not yet rolled).
pub fn open(cfg: JournalConfig, profile: [u8; 32]) -> Result<(JournalWriter, JSide), String> {
    if !cfg.validate() {
        return Err(format!("journal {} configuration invalid", cfg.name.as_str()));
    }
    std::fs::create_dir_all(cfg.hot_journal_dir()).map_err(|e| format!("{}: {e}", cfg.hot_dir.display()))?;
    std::fs::create_dir_all(&cfg.cold_root).map_err(|e| format!("{}: {e}", cfg.cold_root.display()))?;
    let (ptx, prx): (Producer<PreparedSegment>, Consumer<PreparedSegment>) =
        spsc(4, Backing::Heap).map_err(|e| format!("{e:?}"))?;
    let (rtx, rrx): (Producer<RetiredSegment>, Consumer<RetiredSegment>) =
        spsc(256, Backing::Heap).map_err(|e| format!("{e:?}"))?;
    let mut prep = SegmentPreparer::new(
        &cfg,
        PrepEnv {
            writer_epoch: 1,
            build_git: [0; 20],
            host_profile_sha256: profile,
            to_writer: ptx,
        },
    );
    let (w, _) = JournalWriter::open(
        &cfg,
        WriterEnv {
            writer_epoch: 1,
            build_git: [0; 20],
            host_profile_sha256: profile,
            prepared: prx,
            retired: rtx,
        },
    )
    .map_err(|e| format!("journal {}: {e:?}", cfg.name.as_str()))?;
    prep.poll();
    let retirer = HotRetirer::new(&cfg, rrx);
    Ok((w, JSide { cfg, prep, retirer }))
}

/// `RollInfo` at `tsc`/`utc_ns`.
pub fn roll_info(tsc: u64, utc_ns: i64, hz_milli: u64, configs: &[BlobRef]) -> RollInfo<'_> {
    RollInfo {
        utc_ns,
        calib: CalibPair {
            tsc: Tsc(tsc),
            realtime_ns: utc_ns,
            phc_ns: 0,
            err_ns: 0,
        },
        tsc_hz_milli: hz_milli,
        configs,
    }
}

/// First roll of a raw journal (empty boot block).
pub fn first_roll_raw(w: &mut JournalWriter, side: &mut JSide, info: &RollInfo<'_>) -> Result<(), String> {
    side.prep.poll();
    let b = w
        .roll(RollReason::Startup, info)
        .map_err(|e| format!("roll {}: {e:?}", side.cfg.name.as_str()))?;
    b.finish().map_err(|e| format!("{e:?}"))?;
    w.publish();
    Ok(())
}

/// What the seq journal's boot block restates: the host profile and the refdata blob.
#[derive(Clone, Debug)]
pub struct Boot {
    pub profile: HostProfile,
    pub blob: Vec<u8>,
    pub blob_ref: BlobRef,
}

impl Boot {
    pub fn new(profile: HostProfile, blob: Vec<u8>) -> Boot {
        let blob_ref = BlobRef {
            kind: BlobKind::Refdata,
            encoding: BlobRef::ENC_RDB1,
            len: blob.len() as u32,
            version: 1,
            sha256: Sha256::digest(&blob).into(),
        };
        Boot {
            profile,
            blob,
            blob_ref,
        }
    }

    /// Rolls `w` and writes the boot block (HostProfile, ConfigBlob(refdata)).
    pub fn roll(&self, w: &mut JournalWriter, reason: RollReason, info: &RollInfo<'_>) -> Result<(), String> {
        let mut b = w.roll(reason, info).map_err(|e| format!("roll seq: {e:?}"))?;
        let tsc = Tsc(info.calib.tsc.0);
        b.append(tsc, &self.profile).map_err(|e| format!("{e:?}"))?;
        let h = BlobHdr {
            blob: self.blob_ref,
            scope: CmdScope::new_zeroed(),
            boot: 1,
            _pad: [0; 7],
        };
        b.append_var(EventKind::CONFIG_BLOB, tsc, &[h.as_bytes(), &self.blob])
            .map_err(|e| format!("{e:?}"))?;
        b.finish().map_err(|e| format!("{e:?}"))?;
        w.publish();
        Ok(())
    }
}

/// The persist thread's work: preparers, persister and retirers of every journal.
#[derive(Debug)]
pub struct Persist {
    sides: Vec<JSide>,
    pers: PersisterBox,
}

/// `Persister` wrapper (no `Debug` upstream).
struct PersisterBox(Persister);

impl std::fmt::Debug for PersisterBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Persister")
    }
}

impl Persist {
    pub fn new(sides: Vec<JSide>) -> Persist {
        let mut pers = Persister::new(PersistConfig::default());
        for s in &sides {
            pers.add(&s.cfg);
        }
        Persist {
            sides,
            pers: PersisterBox(pers),
        }
    }

    /// One round; returns whether anything was copied.
    pub fn poll(&mut self, mono_ns: u64, utc_ns: i64) -> bool {
        for s in &mut self.sides {
            let _ = s.prep.poll();
        }
        let p = self.pers.0.poll(mono_ns, utc_ns);
        for s in &mut self.sides {
            s.retirer.poll(utc_ns);
        }
        p.bytes_copied > 0 || p.sealed > 0
    }

    /// Copies everything that is committed (after the writers were sealed).
    pub fn flush(&mut self, mut mono_ns: u64, utc_ns: i64) {
        for _ in 0..100_000 {
            mono_ns += 50_000_000;
            let busy = self.poll(mono_ns, utc_ns);
            if !busy && self.pers.0.lag().iter().all(|l| l.bytes == 0) {
                let p = self.pers.0.poll(mono_ns + 50_000_000, utc_ns);
                if p.bytes_copied == 0 && p.sealed == 0 {
                    return;
                }
            }
        }
    }
}

/// A reader of journal `name` from the oldest record to the current end.
pub fn reader(hot: &Path, cold: &Path, name: &str) -> Result<JournalReader, String> {
    let mut c = ReaderConfig::new(
        hot.to_path_buf(),
        cold.to_path_buf(),
        InstanceName::Prod,
        name.into(),
    );
    c.tail_poll_us = 0;
    JournalReader::open(&c).map_err(|e| format!("open {name}: {e:?}"))
}

/// `<root>/prod/<name>` exists.
pub fn exists(root: &Path, name: &str) -> bool {
    root.join("prod").join(name).is_dir()
}

/// The empty hot directory used when reading cold copies only.
pub fn no_hot(root: &Path) -> PathBuf {
    root.join("__no_hot__")
}
