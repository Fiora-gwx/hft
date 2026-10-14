//! Journal preparation, copying and durable close run outside the sequencer.
use crate::{Error, config::Config};
use hft_journal::*;
use hft_rt::{Backing, spsc};
use hft_types::VenueId;

pub struct Side {
    pub cfg: JournalConfig,
    prep: SegmentPreparer,
    retirer: HotRetirer,
}
impl std::fmt::Debug for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Side").field("cfg", &self.cfg).finish()
    }
}
pub fn instance(c: &Config) -> InstanceName {
    if c.instance.name == "sim" {
        InstanceName::Sim
    } else {
        InstanceName::Prod
    }
}
pub fn config(c: &Config, name: &str, kind: JournalKind, venue: VenueId) -> JournalConfig {
    JournalConfig {
        name: name.into(),
        kind,
        venue,
        instance: instance(c),
        hot_dir: c.paths.journal_ram.clone(),
        cold_root: c.paths.journal_cold.clone(),
        segment_bytes: c.journal.segment_bytes,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: c.journal.hot_keep,
    }
}
pub fn open(cfg: JournalConfig, epoch: u32) -> Result<(JournalWriter, Side), Error> {
    if !cfg.validate() {
        return Err(Error::config("JournalConfig", cfg.name.as_str()));
    }
    std::fs::create_dir_all(cfg.hot_journal_dir())
        .map_err(|e| Error::runtime("Journal", e.to_string(), 3))?;
    let (ptx, prx) = spsc(4, Backing::Heap).map_err(|e| Error::runtime("Ring", format!("{e:?}"), 3))?;
    let (rtx, rrx) = spsc(256, Backing::Heap).map_err(|e| Error::runtime("Ring", format!("{e:?}"), 3))?;
    let (writer, _) = JournalWriter::open(
        &cfg,
        WriterEnv {
            writer_epoch: epoch,
            build_git: [0; 20],
            host_profile_sha256: [0; 32],
            prepared: prx,
            retired: rtx,
        },
    )
    .map_err(|e| Error::runtime("Journal", format!("{e:?}"), 4))?;
    let mut prep = SegmentPreparer::new(
        &cfg,
        PrepEnv {
            writer_epoch: epoch,
            build_git: [0; 20],
            host_profile_sha256: [0; 32],
            to_writer: ptx,
        },
    );
    if prep.poll() == PrepState::Failed {
        return Err(Error::runtime("JournalPrepare", cfg.name.as_str(), 3));
    }
    let retirer = HotRetirer::new(&cfg, rrx);
    Ok((writer, Side { cfg, prep, retirer }))
}
pub fn reader(c: &Config) -> Result<JournalReader, Error> {
    let mut r = ReaderConfig::new(
        c.paths.journal_ram.clone(),
        c.paths.journal_cold.clone(),
        instance(c),
        "seq".into(),
    );
    r.tail_poll_us = 0;
    JournalReader::open(&r).map_err(|e| Error::runtime("JournalReader", format!("{e:?}"), 4))
}
pub struct Persist {
    sides: Vec<Side>,
    persister: Persister,
}
impl std::fmt::Debug for Persist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Persist").finish_non_exhaustive()
    }
}
impl Persist {
    pub fn new(sides: Vec<Side>) -> Self {
        let mut p = Persister::new(PersistConfig::default());
        for s in &sides {
            p.add(&s.cfg);
        }
        Self { sides, persister: p }
    }
    pub fn poll(&mut self, mono_ns: u64, utc_ns: i64) -> Result<bool, Error> {
        for s in &mut self.sides {
            if s.prep.poll() == PrepState::Failed {
                return Err(Error::runtime("JournalPrepare", s.cfg.name.as_str(), 3));
            }
        }
        let p = self.persister.poll(mono_ns, utc_ns);
        if p.errors != 0 {
            return Err(Error::runtime("Persist", "copy/sync failed", 3));
        }
        for s in &mut self.sides {
            s.retirer.poll(utc_ns);
        }
        Ok(p.bytes_copied != 0 || p.sealed != 0)
    }
    pub fn flush(&mut self, mono: u64, utc: i64) -> Result<(), Error> {
        for n in 1..=4096 {
            let p = self.persister.poll(mono.saturating_add(n * 50_000_000), utc);
            if p.errors != 0 {
                return Err(Error::runtime("Persist", "final sync failed", 3));
            }
            if p.bytes_copied == 0 && p.sealed == 0 && self.persister.lag().iter().all(|l| l.bytes == 0) {
                return Ok(());
            }
        }
        Err(Error::runtime("Persist", "final sync did not converge", 3))
    }
}
