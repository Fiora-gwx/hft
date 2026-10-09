//! Read-only journal access (README "1. journal → Arrow"): path resolution, segment listing, the
//! record scan over hft-journal's `JournalReader` (TSC → UTC, segment tracking, error mapping),
//! `events`, `tail` and `info`. Nothing here writes a journal file.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hft_event::{
    AccountConfigReportHdr, AdviceHdr, EventKind, KindInfo, L2BatchHdr, Layout, MarginReportHdr, RecordFlags,
    kind_info,
};
use hft_journal::{
    EndAt, InstanceName, JournalReader, Next, OnGap, OpenErr, ReaderConfig, SEGMENT_MAGIC,
    SegmentHeaderFixed, StartAt,
};
use hft_refdata::{AccountKind, RefData};
use hft_types::{AccountId, AssetId, InstId};
use zerocopy::FromBytes;

use crate::arrow_map::{self, Batch, Builder, Exps, KindMap, Plan, Row};
use crate::error::{Error, Res};
use crate::time::Clock;

/// Immutable part of a segment header (`SegmentHeaderFixed`).
const HEADER_FIXED_LEN: usize = size_of::<SegmentHeaderFixed>();

/// Journal location parsed from the `open_journal` path.
#[derive(Clone, Debug)]
pub struct Spec {
    /// The path as given (error reports).
    pub path: String,
    pub cold_root: PathBuf,
    pub hot_root: Option<PathBuf>,
    pub instance: InstanceName,
    pub name: String,
    /// A single segment file was given: only this segment is read.
    pub single: Option<u64>,
}

fn parse_cold_name(name: &str) -> Option<(u64, u64)> {
    let d = name.strip_suffix(".jrn")?;
    let (a, b) = d.split_once('-')?;
    if a.len() != 10 || b.len() != 20 || !a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn parse_hot_name(name: &str) -> Option<u64> {
    let d = name.strip_suffix(".jrn")?;
    (d.len() == 10 && d.bytes().all(|c| c.is_ascii_digit())).then(|| d.parse().ok())?
}

fn name_of(p: &Path) -> Option<String> {
    p.file_name().and_then(|s| s.to_str()).map(str::to_string)
}

fn instance_of(s: &str) -> Res<InstanceName> {
    s.parse::<InstanceName>().map_err(|_| {
        Error::value(format!(
            "instance `{s}`: the path must be <cold_root>/<prod|sim>/<journal>"
        ))
    })
}

/// `open_journal(path, hot_root)`: `<cold_root>/<instance>/<journal>` or one segment file.
pub fn parse_spec(path: &str, hot_root: Option<PathBuf>) -> Res<Spec> {
    let p = PathBuf::from(path);
    let jerr = |msg: &str| Error::journal(format!("{path}: {msg}"), Some(path.to_string()), None);
    if p.extension().is_some_and(|e| e == "jrn") {
        let fname = name_of(&p).unwrap_or_default();
        let (no, _) = parse_cold_name(&fname).ok_or_else(|| jerr("not a cold segment file name"))?;
        let jdir = p
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| jerr("no journal directory"))?;
        let idir = jdir.parent().ok_or_else(|| jerr("no instance directory"))?;
        let instance = instance_of(&name_of(idir).unwrap_or_default())?;
        if !p.is_file() {
            return Err(jerr("segment file not found"));
        }
        return Ok(Spec {
            path: path.to_string(),
            cold_root: idir.parent().map(Path::to_path_buf).unwrap_or_default(),
            hot_root,
            instance,
            name: name_of(jdir).unwrap_or_default(),
            single: Some(no),
        });
    }
    let p = if path.ends_with('/') || path.ends_with('\\') {
        PathBuf::from(path.trim_end_matches(['/', '\\']))
    } else {
        p
    };
    let name = name_of(&p).ok_or_else(|| jerr("no journal name"))?;
    let idir = p.parent().ok_or_else(|| jerr("no instance directory"))?;
    let instance = instance_of(&name_of(idir).unwrap_or_default())?;
    if !p.is_dir() {
        return Err(jerr("journal directory not found"));
    }
    Ok(Spec {
        path: path.to_string(),
        cold_root: idir.parent().map(Path::to_path_buf).unwrap_or_default(),
        hot_root,
        instance,
        name,
        single: None,
    })
}

/// One segment on disk.
#[derive(Clone, Debug)]
pub struct SegFile {
    pub no: u64,
    pub first_seq: u64,
    pub hot: Option<PathBuf>,
    pub cold: Option<PathBuf>,
    pub sealed: bool,
    pub header: Option<SegmentHeaderFixed>,
}

impl SegFile {
    /// Hot path when present, else the cold copy.
    pub fn path(&self) -> Option<&Path> {
        self.hot.as_deref().or(self.cold.as_deref())
    }
}

fn read_header(p: &Path) -> Option<SegmentHeaderFixed> {
    use std::io::Read;
    let mut f = std::fs::File::open(p).ok()?;
    let mut b = vec![0u8; HEADER_FIXED_LEN];
    f.read_exact(&mut b).ok()?;
    SegmentHeaderFixed::read_from_bytes(&b).ok()
}

/// `yyyymmdd` of UTC ns (cold copy date directory).
pub fn utc_yyyymmdd(ns: i64) -> String {
    let days = ns.div_euclid(86_400_000_000_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}")
}

/// Hot and cold segment files of a journal, by segment number.
pub fn list_segments(hot_jdir: Option<&Path>, cold_jdir: &Path) -> BTreeMap<u64, SegFile> {
    let mut m: BTreeMap<u64, SegFile> = BTreeMap::new();
    let entry = |m: &mut BTreeMap<u64, SegFile>, no: u64| -> SegFile {
        m.remove(&no).unwrap_or(SegFile {
            no,
            first_seq: 0,
            hot: None,
            cold: None,
            sealed: false,
            header: None,
        })
    };
    if let Ok(days) = std::fs::read_dir(cold_jdir) {
        for d in days.flatten() {
            let dp = d.path();
            if !dp.is_dir() {
                continue;
            }
            let Ok(files) = std::fs::read_dir(&dp) else {
                continue;
            };
            for f in files.flatten() {
                let p = f.path();
                let Some((no, first)) = name_of(&p).as_deref().and_then(parse_cold_name) else {
                    continue;
                };
                let mut s = entry(&mut m, no);
                s.first_seq = first;
                s.sealed = p.with_extension("seal").exists();
                s.cold = Some(p);
                m.insert(no, s);
            }
        }
    }
    if let Some(h) = hot_jdir
        && let Ok(rd) = std::fs::read_dir(h)
    {
        for f in rd.flatten() {
            let p = f.path();
            let Some(no) = name_of(&p).as_deref().and_then(parse_hot_name) else {
                continue;
            };
            let Some(hdr) = read_header(&p).filter(|h| h.magic == SEGMENT_MAGIC) else {
                continue;
            };
            // Preparing segments (first_seq not yet written) are not part of the journal.
            if hdr.first_seq == 0 {
                continue;
            }
            let mut s = entry(&mut m, no);
            s.first_seq = hdr.first_seq;
            s.hot = Some(p);
            m.insert(no, s);
        }
    }
    for s in m.values_mut() {
        s.header = s.path().and_then(read_header);
    }
    m
}

/// One difference between a segment-header schema table and this build (README "Arrow 映射").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaDiff {
    pub kind: u16,
    pub name: &'static str,
    pub journal_version: u16,
    pub journal_size: u32,
    pub reader_version: u16,
    pub reader_size: u32,
    pub segments: Vec<u64>,
}

/// Body (Fixed) or prefix (Var/Multi) size of this build.
pub fn layout_size(info: &KindInfo) -> u32 {
    match info.layout {
        Layout::Fixed { size } => u32::from(size),
        Layout::Var { prefix, .. } | Layout::Multi { prefix } => u32::from(prefix),
    }
}

/// A `ConfigBlob` of a boot block.
#[derive(Clone, Debug)]
pub struct BootBlob {
    pub seq: u64,
    /// The 72-byte `BlobHdr`.
    pub hdr: Vec<u8>,
    pub content: Vec<u8>,
}

impl BootBlob {
    pub fn kind(&self) -> u16 {
        u16::from_le_bytes([
            self.hdr.first().copied().unwrap_or(0),
            self.hdr.get(1).copied().unwrap_or(0),
        ])
    }
}

/// Contents of one segment's boot block.
#[derive(Clone, Debug, Default)]
pub struct Boot {
    pub blobs: Vec<BootBlob>,
    pub host_profile: Option<Vec<u8>>,
    pub boot_done: Option<u64>,
}

/// Shared, immutable state of an opened journal.
#[derive(Debug)]
pub struct Journal {
    pub spec: Spec,
    pub hot_jdir: Option<PathBuf>,
    pub cold_jdir: PathBuf,
    pub segs: BTreeMap<u64, SegFile>,
    pub diffs: Vec<SchemaDiff>,
    /// Kinds written somewhere with a shorter body/prefix than this build's (`_missing` column).
    pub older: BTreeSet<u16>,
    /// Refdata of the newest boot block that carries one.
    pub refdata: Option<Arc<RefData>>,
    pub boot_newest: Boot,
}

const BLOB_HDR: usize = 72;
/// `BlobKind::Refdata`.
pub const BLOB_REFDATA: u16 = 1;

impl Journal {
    pub fn open(path: &str, hot_root: Option<PathBuf>) -> Res<Journal> {
        let spec = parse_spec(path, hot_root)?;
        let cold_jdir = spec.cold_root.join(spec.instance.as_str()).join(&spec.name);
        let hot_jdir = spec
            .hot_root
            .as_ref()
            .map(|h| h.join(spec.instance.as_str()).join(&spec.name));
        let mut segs = list_segments(hot_jdir.as_deref(), &cold_jdir);
        if let Some(no) = spec.single {
            segs.retain(|k, _| *k == no);
        }
        if segs.is_empty() {
            return Err(Error::journal(
                format!("{}: no journal segment", spec.path),
                Some(spec.path.clone()),
                None,
            ));
        }
        let (diffs, older) = schema_check(&segs)?;
        let mut j = Journal {
            spec,
            hot_jdir,
            cold_jdir,
            segs,
            diffs,
            older,
            refdata: None,
            boot_newest: Boot::default(),
        };
        let nos: Vec<u64> = j.segs.keys().rev().copied().collect();
        for (i, no) in nos.iter().enumerate() {
            let Ok(b) = j.read_boot(*no) else { continue };
            if i == 0 {
                j.boot_newest = b.clone();
            }
            if let Some(blob) = b.blobs.iter().find(|x| x.kind() == BLOB_REFDATA)
                && let Ok(rd) = RefData::load(&blob.content, None)
            {
                j.refdata = Some(Arc::new(rd));
                break;
            }
        }
        Ok(j)
    }

    /// Reader configuration at `start` (hot segments only with `hot_root`).
    pub fn reader(&self, start: StartAt) -> Res<JournalReader> {
        let hot = self
            .spec
            .hot_root
            .clone()
            .unwrap_or_else(|| self.spec.cold_root.join(".hft-py-no-hot"));
        let mut rc = ReaderConfig::new(
            hot,
            self.spec.cold_root.clone(),
            self.spec.instance,
            self.spec.name.as_str().into(),
        );
        rc.start = start;
        rc.end = EndAt::Follow;
        rc.on_gap = OnGap::Error;
        JournalReader::open(&rc).map_err(|e| match e {
            OpenErr::SeqUnavailable => Error::value("from_seq is outside the readable range of the journal"),
            other => Error::journal(
                format!("{}: cannot open reader: {other:?}", self.spec.path),
                Some(self.spec.path.clone()),
                None,
            ),
        })
    }

    /// Start position of a full read (`Oldest`, or the single segment).
    pub fn start(&self) -> StartAt {
        match self.spec.single {
            Some(no) => StartAt::SegmentBoot(no),
            None => StartAt::Oldest,
        }
    }

    /// A scan from `start`.
    pub fn scan(&self, start: StartAt) -> Res<Scan> {
        Scan::new(self, start)
    }

    /// The boot block of segment `no`.
    pub fn read_boot(&self, no: u64) -> Res<Boot> {
        let mut sc = Scan::new(self, StartAt::SegmentBoot(no))?;
        let mut b = Boot::default();
        let mut began = false;
        for _ in 0..100_000 {
            match sc.step()? {
                Step::Rec(rec) => {
                    if rec.kind == EventKind::SEGMENT_ROLL.0 {
                        let mark = rec.body.get(32).copied().unwrap_or(0);
                        if mark == 2 {
                            if began {
                                break;
                            }
                            began = true;
                        } else if mark == 3 {
                            b.boot_done = Some(rec.seq);
                            break;
                        } else if mark == 1 {
                            break;
                        }
                    } else if rec.kind == EventKind::CONFIG_BLOB.0 && rec.body.len() >= BLOB_HDR {
                        let hdr = rec.body.get(..BLOB_HDR).unwrap_or(&[]).to_vec();
                        let len = u32::from_le_bytes([
                            hdr.get(4).copied().unwrap_or(0),
                            hdr.get(5).copied().unwrap_or(0),
                            hdr.get(6).copied().unwrap_or(0),
                            hdr.get(7).copied().unwrap_or(0),
                        ]) as usize;
                        let content = rec.body.get(BLOB_HDR..BLOB_HDR + len).unwrap_or(&[]).to_vec();
                        b.blobs.push(BootBlob {
                            seq: rec.seq,
                            hdr,
                            content,
                        });
                    } else if rec.kind == EventKind::HOST_PROFILE.0 {
                        b.host_profile = Some(rec.body.to_vec());
                    }
                }
                Step::End | Step::Pending => break,
            }
        }
        Ok(b)
    }

    /// Exponents and account kinds for a query (`insts` = the query's inst filter).
    pub fn exps(&self, insts: Option<&[u32]>) -> Exps {
        let mut e = Exps::default();
        let Some(rd) = &self.refdata else { return e };
        // Without an inst filter: every inst of the refdata that is not delisted.
        for v in rd.venues_in_order() {
            for i in rd.insts_of_venue(*v) {
                let h = rd.inst(*i);
                if insts.is_none() && h.status != hft_refdata::InstStatus::Delisted {
                    e.inst.insert(i.0, (h.px_exp, h.qty_exp));
                }
            }
        }
        if let Some(f) = insts {
            for i in f {
                let h = rd.inst(InstId(*i));
                if h.venue.0 != 0 {
                    e.inst.insert(*i, (h.px_exp, h.qty_exp));
                }
            }
        }
        for a in 1..=u16::MAX {
            let m = rd.asset(AssetId(a));
            if m.id.0 == 0 {
                if a > 4096 {
                    break;
                }
                continue;
            }
            e.asset.insert(a, m.exp.qty_exp);
        }
        for a in 1..=254u8 {
            let m = rd.account(AccountId(a));
            if m.id == AccountId(a) {
                e.account_kind.insert(
                    a,
                    match m.kind {
                        AccountKind::Live => "Live",
                        AccountKind::Sim(_) => "Sim",
                    },
                );
            }
        }
        e
    }

    /// Account id of a label or id (refdata of the journal).
    pub fn account_id(&self, label: Option<&str>, id: Option<i64>) -> Res<u8> {
        let rd = self.refdata.as_ref().ok_or_else(|| {
            Error::value("the journal has no refdata boot blob: accounts cannot be resolved")
        })?;
        match (label, id) {
            (Some(l), _) => (1..=254u8)
                .find(|a| {
                    let m = rd.account(AccountId(*a));
                    m.id == AccountId(*a) && &*m.label == l
                })
                .ok_or_else(|| Error::value(format!("account `{l}` is not in the journal's refdata"))),
            (None, Some(i)) => {
                let a =
                    u8::try_from(i).map_err(|_| Error::value(format!("account {i}: not an AccountId")))?;
                if a != 0 && rd.account(AccountId(a)).id == AccountId(a) {
                    Ok(a)
                } else {
                    Err(Error::value(format!(
                        "account {i} is not in the journal's refdata"
                    )))
                }
            }
            (None, None) => Err(Error::value("account missing")),
        }
    }
}

/// Schema tables of the segment headers vs this build: same version with another size →
/// `SchemaError`; other versions are listed.
fn schema_check(segs: &BTreeMap<u64, SegFile>) -> Res<(Vec<SchemaDiff>, BTreeSet<u16>)> {
    let mut diffs: Vec<SchemaDiff> = Vec::new();
    let mut older = BTreeSet::new();
    for s in segs.values() {
        let Some(h) = &s.header else { continue };
        if h.magic != SEGMENT_MAGIC {
            continue;
        }
        let n = usize::from(h.n_schema).min(h.schema.len());
        for e in h.schema.iter().take(n) {
            let Some(info) = kind_info(EventKind(e.kind)) else {
                continue;
            };
            let size = layout_size(info);
            if e.version == info.schema_version {
                if e.size != size {
                    return Err(Error::Schema(format!(
                        "segment {}: kind {} version {} has size {} here and {} in the journal (append-only rule broken)",
                        s.no, info.name, e.version, size, e.size
                    )));
                }
                continue;
            }
            if e.size < size {
                older.insert(e.kind);
            }
            match diffs
                .iter_mut()
                .find(|d| d.kind == e.kind && d.journal_version == e.version && d.journal_size == e.size)
            {
                Some(d) => d.segments.push(s.no),
                None => diffs.push(SchemaDiff {
                    kind: e.kind,
                    name: info.name,
                    journal_version: e.version,
                    journal_size: e.size,
                    reader_version: info.schema_version,
                    reader_size: size,
                    segments: vec![s.no],
                }),
            }
        }
    }
    Ok((diffs, older))
}

/// One delivered record.
#[derive(Copy, Clone, Debug)]
pub struct Rec<'a> {
    pub seq: u64,
    pub kind: u16,
    pub tsc: u64,
    pub flags: u16,
    /// Logical body (Var records trimmed to prefix + tail).
    pub body: &'a [u8],
    pub proc_ns: i64,
    pub segment: u64,
    /// Prefix size the writer used (segment-header schema table; this build's when absent).
    pub prefix: usize,
    /// Inside a boot block (`Begin` … `BootDone`).
    pub in_boot: bool,
}

impl Rec<'_> {
    /// `SegmentRoll` mark (1 End, 2 Begin, 3 BootDone) or 0.
    pub fn roll_mark(&self) -> u8 {
        if self.kind == EventKind::SEGMENT_ROLL.0 {
            self.body.get(32).copied().unwrap_or(0)
        } else {
            0
        }
    }
}

/// `Scan::step` result.
#[derive(Debug)]
pub enum Step<'a> {
    Rec(Rec<'a>),
    /// No more records now (tail: wait and retry).
    Pending,
    /// Single-segment read finished.
    End,
}

/// Scan state apart from the reader (so a delivered body can borrow the reader).
#[derive(Debug)]
struct ScanState {
    hot_jdir: Option<PathBuf>,
    cold_jdir: PathBuf,
    single: Option<u64>,
    seg: u64,
    clock: Clock,
    segs: BTreeMap<u64, SegFile>,
    done: bool,
    in_boot: bool,
}

/// Record scan over a `JournalReader`.
#[derive(Debug)]
pub struct Scan {
    r: JournalReader,
    st: ScanState,
}

/// Exact logical length of a Var record (prefix + tail, without the record padding).
fn exact_var(kind: EventKind, v: &hft_event::RecordView<'_>) -> Option<usize> {
    match kind {
        EventKind::ADVICE => v.var::<AdviceHdr>().ok().map(|(_, t)| 32 + t.len()),
        EventKind::MARGIN_REPORT => v.var::<MarginReportHdr>().ok().map(|(_, t)| 64 + t.len()),
        EventKind::ACCOUNT_CONFIG_REPORT => v.var::<AccountConfigReportHdr>().ok().map(|(_, t)| 64 + t.len()),
        EventKind::L2_BATCH => v.var::<L2BatchHdr>().ok().map(|(_, t)| 80 + t.len()),
        _ => None,
    }
}

/// Path of segment `no` for error reports (the expected cold copy path when it is gone).
fn seg_path(
    hot_jdir: Option<&Path>,
    cold_jdir: &Path,
    known: &BTreeMap<u64, SegFile>,
    no: u64,
    expected_first: Option<u64>,
) -> String {
    let segs = list_segments(hot_jdir, cold_jdir);
    if let Some(p) = segs.get(&no).and_then(SegFile::path) {
        return p.display().to_string();
    }
    // Gone: the cold copy path it would have (date of its own header, else of the segment before).
    let date = known
        .get(&no)
        .and_then(|s| s.header)
        .or_else(|| known.range(..no).next_back().and_then(|(_, s)| s.header))
        .map_or_else(|| "*".to_string(), |h| utc_yyyymmdd(h.created_utc_ns));
    let first = expected_first
        .or_else(|| known.get(&no).map(|s| s.first_seq))
        .unwrap_or(0);
    cold_jdir
        .join(date)
        .join(format!("{no:010}-{first:020}.jrn"))
        .display()
        .to_string()
}

impl ScanState {
    /// Segment `no` became current: calibration from its header.
    fn enter(&mut self, no: u64) {
        self.seg = no;
        if !self.segs.contains_key(&no) {
            self.segs = list_segments(self.hot_jdir.as_deref(), &self.cold_jdir);
        }
        if let Some(h) = self.segs.get(&no).and_then(|s| s.header) {
            self.clock = Clock {
                tsc: h.calib_tsc,
                realtime_ns: h.calib_realtime_ns,
                hz_milli: h.tsc_hz_milli,
            };
        }
    }

    fn writer_prefix(&self, kind: u16, info: Option<&KindInfo>) -> usize {
        let own = info.map_or(0, layout_size) as usize;
        self.segs
            .get(&self.seg)
            .and_then(|s| s.header)
            .and_then(|h| {
                let n = usize::from(h.n_schema).min(h.schema.len());
                h.schema
                    .iter()
                    .take(n)
                    .find(|e| e.kind == kind)
                    .map(|e| e.size as usize)
            })
            .unwrap_or(own)
    }

    fn deliver<'b>(&mut self, seq: u64, kind: u16, tsc: u64, flags: u16, body: &'b [u8]) -> Step<'b> {
        let k = EventKind(kind);
        let u64_at = |o: usize| {
            u64::from_le_bytes(
                body.get(o..o + 8)
                    .and_then(|b| b.try_into().ok())
                    .unwrap_or([0; 8]),
            )
        };
        if k == EventKind::SEGMENT_ROLL && body.get(32).copied() == Some(2) {
            let no = u64_at(0);
            if let Some(single) = self.single
                && no != single
            {
                self.done = true;
                return Step::End;
            }
            if no != self.seg {
                self.enter(no);
            }
            self.in_boot = true;
        } else if k == EventKind::CLOCK_STATUS && body.len() >= 32 {
            let hz = u64_at(24);
            if hz > 0 {
                self.clock = Clock {
                    tsc: u64_at(0),
                    realtime_ns: u64_at(8) as i64,
                    hz_milli: hz,
                };
            }
        }
        let rec = Rec {
            seq,
            kind,
            tsc,
            flags,
            body,
            proc_ns: self.clock.proc_ns(tsc),
            segment: self.seg,
            prefix: self.writer_prefix(kind, kind_info(k)),
            in_boot: self.in_boot,
        };
        if k == EventKind::SEGMENT_ROLL && body.get(32).copied() == Some(3) {
            self.in_boot = false;
        }
        Step::Rec(rec)
    }
}

impl Scan {
    fn new(j: &Journal, start: StartAt) -> Res<Scan> {
        let r = j.reader(start)?;
        let seg = r.position().segment_no;
        let mut st = ScanState {
            hot_jdir: j.hot_jdir.clone(),
            cold_jdir: j.cold_jdir.clone(),
            single: j.spec.single,
            seg,
            clock: Clock::default(),
            segs: j.segs.clone(),
            done: false,
            in_boot: false,
        };
        st.enter(seg);
        Ok(Scan { r, st })
    }

    /// Seq of the last record delivered.
    pub fn last_seq(&self) -> u64 {
        self.r.position().seq
    }

    /// Current segment number.
    pub fn segment(&self) -> u64 {
        self.st.seg
    }

    /// The next record, `Pending` (nothing more now) or `End` (single segment done).
    pub fn step(&mut self) -> Res<Step<'_>> {
        if self.st.done {
            return Ok(Step::End);
        }
        // A failed poll does not move the position: the failing record is the next one.
        let before = self.r.position();
        let fail = match self.r.poll() {
            Next::Record(v) => {
                let k = EventKind(v.hdr.kind);
                let body: &[u8] = match exact_var(k, &v) {
                    Some(n) => v.body.get(..n).unwrap_or(v.body),
                    None => v.body,
                };
                return Ok(self
                    .st
                    .deliver(v.hdr.seq, v.hdr.kind, v.hdr.tsc, v.hdr.flags, body));
            }
            Next::Event(e) => return Ok(self.st.deliver(e.seq, e.kind.0, e.tsc, e.flags.0, e.body)),
            Next::Pending => return Ok(Step::Pending),
            Next::End => return Ok(Step::End),
            Next::Gap(g) => Ok(g),
            Next::Err(e) => Err(e),
        };
        match fail {
            Ok(g) => {
                if let Some(single) = self.st.single
                    && g.at.segment_no >= single
                {
                    self.st.done = true;
                    return Ok(Step::End);
                }
                let no = g.at.segment_no + 1;
                Err(Error::journal(
                    format!("journal gap ({:?}) after segment {}", g.reason, g.at.segment_no),
                    Some(seg_path(
                        self.st.hot_jdir.as_deref(),
                        &self.st.cold_jdir,
                        &self.st.segs,
                        no,
                        Some(g.expected_seq),
                    )),
                    Some(g.expected_seq),
                ))
            }
            Err(e) => {
                let seq = before.seq.saturating_add(1);
                let segs = list_segments(self.st.hot_jdir.as_deref(), &self.st.cold_jdir);
                let no = segs
                    .values()
                    .chain(self.st.segs.values())
                    .filter(|s| s.first_seq <= seq && s.first_seq != 0)
                    .map(|s| s.no)
                    .max()
                    .unwrap_or(self.st.seg);
                Err(Error::journal(
                    format!("read error {e:?} in segment {no}"),
                    Some(seg_path(
                        self.st.hot_jdir.as_deref(),
                        &self.st.cold_jdir,
                        &self.st.segs,
                        no,
                        None,
                    )),
                    Some(seq),
                ))
            }
        }
    }
}

/// Which time a window filters on.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TimeSrc {
    Proc,
    Ex,
    Hw,
}

/// A validated `events` query.
#[derive(Clone, Debug)]
pub struct Query {
    pub map: Arc<KindMap>,
    pub insts: Option<Vec<u32>>,
    pub t0: Option<i64>,
    pub t1: Option<i64>,
    pub time: TimeSrc,
    pub account: Option<u8>,
    pub plan: Plan,
    pub batch_rows: usize,
}

/// Kind by name or number (unknown → `ValueError`).
pub fn kind_by(name: Option<&str>, num: Option<i64>) -> Res<&'static KindInfo> {
    let found = match (name, num) {
        (Some(n), _) => hft_event::KINDS.iter().find(|k| k.name == n),
        (None, Some(x)) => u16::try_from(x).ok().and_then(|k| kind_info(EventKind(k))),
        _ => None,
    };
    found
        .filter(|k| k.kind.0 >> 8 != 0x7F)
        .ok_or_else(|| match (name, num) {
            (Some(n), _) => Error::value(format!("unknown event kind `{n}`")),
            (_, Some(x)) => Error::value(format!("unknown event kind {x:#x}")),
            _ => Error::value("kind missing"),
        })
}

/// Options of `events` before validation against the kind.
#[derive(Clone, Debug, Default)]
pub struct QueryArgs {
    pub insts: Option<Vec<u32>>,
    pub t0: Option<i64>,
    pub t1: Option<i64>,
    pub time: Option<String>,
    pub account_label: Option<String>,
    pub account_id: Option<i64>,
    pub columns: Option<Vec<String>>,
    pub decimals: bool,
    pub batch_rows: Option<i64>,
}

/// Default `batch_rows`.
pub const DEFAULT_BATCH_ROWS: usize = 1_000_000;

impl Journal {
    /// Validates `events` arguments for `info`.
    pub fn query(&self, info: &'static KindInfo, a: &QueryArgs) -> Res<Query> {
        let map = Arc::new(arrow_map::kind_map(info));
        if a.insts.is_some() && map.inst_leaf.is_none() {
            return Err(Error::value(format!("kind {} has no inst field", info.name)));
        }
        let time = match a.time.as_deref().unwrap_or("proc") {
            "proc" => TimeSrc::Proc,
            "ex" => TimeSrc::Ex,
            "hw" => TimeSrc::Hw,
            t => {
                return Err(Error::value(format!(
                    "time `{t}`: expected \"proc\", \"ex\" or \"hw\""
                )));
            }
        };
        if (time == TimeSrc::Ex && map.ex_leaf.is_none()) || (time == TimeSrc::Hw && map.hw_leaf.is_none()) {
            return Err(Error::value(format!(
                "kind {} has no {} time field",
                info.name,
                if time == TimeSrc::Ex {
                    "ex_ts_ns"
                } else {
                    "hw_rx_ns"
                }
            )));
        }
        if let (Some(t0), Some(t1)) = (a.t0, a.t1)
            && t0 > t1
        {
            return Err(Error::value(format!("t0 {t0} > t1 {t1}")));
        }
        let account = if a.account_label.is_some() || a.account_id.is_some() {
            if map.account_leaf.is_none() {
                return Err(Error::value(format!("kind {} has no account field", info.name)));
            }
            Some(self.account_id(a.account_label.as_deref(), a.account_id)?)
        } else {
            None
        };
        let batch_rows = match a.batch_rows {
            None => DEFAULT_BATCH_ROWS,
            Some(n) if n >= 1 => usize::try_from(n).map_err(|_| Error::value("batch_rows too large"))?,
            Some(n) => return Err(Error::value(format!("batch_rows {n} < 1"))),
        };
        let exps = Arc::new(self.exps(a.insts.as_deref()));
        let plan = arrow_map::plan(
            map.clone(),
            a.decimals,
            self.older.contains(&info.kind.0),
            a.columns.as_deref(),
            exps,
        )
        .map_err(Error::Value)?;
        Ok(Query {
            map,
            insts: a.insts.clone(),
            t0: a.t0,
            t1: a.t1,
            time,
            account,
            plan,
            batch_rows,
        })
    }
}

impl Query {
    fn matches(&self, rec: &Rec<'_>) -> bool {
        if rec.kind != self.map.info.kind.0 {
            return false;
        }
        let m = &self.map;
        let pre = rec.body;
        if let Some(f) = &self.insts {
            let inst = m
                .inst_leaf
                .and_then(|i| m.leaves.get(i))
                .map_or(0, |l| l.read_int(pre) as u32);
            if !f.contains(&inst) {
                return false;
            }
        }
        if let Some(a) = self.account {
            let acc = m
                .account_leaf
                .and_then(|i| m.leaves.get(i))
                .map_or(0, |l| l.read_int(pre) as u8);
            if acc != a {
                return false;
            }
        }
        if self.t0.is_some() || self.t1.is_some() || self.time != TimeSrc::Proc {
            let t = match self.time {
                TimeSrc::Proc => Some(rec.proc_ns),
                TimeSrc::Ex => m
                    .ex_leaf
                    .and_then(|i| m.leaves.get(i))
                    .map(|l| l.read_int(pre) as i64),
                TimeSrc::Hw => m
                    .hw_leaf
                    .and_then(|i| m.leaves.get(i))
                    .map(|l| l.read_int(pre) as i64),
            };
            let Some(t) = t.filter(|t| *t != 0 || self.time == TimeSrc::Proc) else {
                return false;
            };
            if self.t0.is_some_and(|t0| t < t0) || self.t1.is_some_and(|t1| t >= t1) {
                return false;
            }
        }
        true
    }

    pub fn row<'a>(rec: &Rec<'a>) -> Row<'a> {
        Row {
            seq: rec.seq,
            tsc: rec.tsc,
            proc_ns: rec.proc_ns,
            flags: rec.flags,
            body: rec.body,
            prefix: rec.prefix,
        }
    }
}

/// Lazy batch iterator of `events()`.
#[derive(Debug)]
pub struct Events {
    scan: Option<Scan>,
    q: Query,
    b: Builder,
    err: Option<Error>,
    done: bool,
    j: Arc<Journal>,
}

impl Events {
    pub fn new(j: &Arc<Journal>, q: Query) -> Events {
        let b = Builder::new(q.plan.clone(), q.batch_rows);
        Events {
            scan: None,
            q,
            b,
            err: None,
            done: false,
            j: j.clone(),
        }
    }

    pub fn schema(&self) -> Arc<arrow_map::SchemaSpec> {
        self.q.plan.schema.clone()
    }

    /// The next batch, `None` at the end.
    pub fn next_batch(&mut self) -> Res<Option<Batch>> {
        if let Some(e) = self.err.take() {
            self.done = true;
            return Err(e);
        }
        if self.done {
            return Ok(None);
        }
        if self.scan.is_none() {
            match self.j.scan(self.j.start()) {
                Ok(s) => self.scan = Some(s),
                Err(e) => {
                    self.done = true;
                    return Err(e);
                }
            }
        }
        let Some(scan) = self.scan.as_mut() else {
            return Ok(None);
        };
        loop {
            let step = match scan.step() {
                Ok(s) => s,
                Err(e) => {
                    if self.b.is_empty() {
                        self.done = true;
                        return Err(e);
                    }
                    self.err = Some(e);
                    return self.flush();
                }
            };
            match step {
                Step::Rec(rec) => {
                    if self.q.matches(&rec) {
                        self.b.push(&Query::row(&rec));
                        if self.b.is_full() {
                            return self.flush();
                        }
                    }
                    if rec.roll_mark() == 1 && !self.b.is_empty() {
                        return self.flush();
                    }
                }
                Step::Pending | Step::End => {
                    self.done = true;
                    self.scan = None;
                    return if self.b.is_empty() { Ok(None) } else { self.flush() };
                }
            }
        }
    }

    fn flush(&mut self) -> Res<Option<Batch>> {
        let b = self.b.finish().map_err(|e| Error::journal(e, None, None))?;
        Ok(Some(b))
    }
}

/// One owned record of the event being assembled by `tail`.
#[derive(Clone, Debug)]
struct Owned {
    seq: u64,
    kind: u16,
    tsc: u64,
    flags: u16,
    body: Vec<u8>,
    proc_ns: i64,
    prefix: usize,
}

/// `tail()`: runs of same-kind records of complete events, newer than the start point.
#[derive(Debug)]
pub struct Tail {
    scan: Scan,
    kinds: Vec<(u16, Plan)>,
    cur: Option<(u16, Builder)>,
    event: Vec<Owned>,
    out: VecDeque<Batch>,
}

impl Tail {
    pub fn new(j: &Arc<Journal>, kinds: &[&'static KindInfo], from_seq: Option<u64>) -> Res<Tail> {
        let start = match from_seq {
            None => StartAt::Tail,
            Some(n) => StartAt::Seq(n),
        };
        let mut plans = Vec::new();
        for k in kinds {
            if plans.iter().any(|(n, _)| *n == k.kind.0) {
                continue;
            }
            let map = Arc::new(arrow_map::kind_map(k));
            let p = arrow_map::plan(
                map,
                false,
                j.older.contains(&k.kind.0),
                None,
                Arc::new(j.exps(None)),
            )
            .map_err(Error::Value)?;
            plans.push((k.kind.0, p));
        }
        Ok(Tail {
            scan: j.scan(start)?,
            kinds: plans,
            cur: None,
            event: Vec::new(),
            out: VecDeque::new(),
        })
    }

    fn commit(&mut self) {
        for o in std::mem::take(&mut self.event) {
            let Some((_, plan)) = self.kinds.iter().find(|(k, _)| *k == o.kind) else {
                continue;
            };
            let same = self.cur.as_ref().is_some_and(|(k, _)| *k == o.kind);
            if !same {
                if let Some((_, mut b)) = self.cur.take()
                    && !b.is_empty()
                    && let Ok(batch) = b.finish()
                {
                    self.out.push_back(batch);
                }
                self.cur = Some((o.kind, Builder::new(plan.clone(), DEFAULT_BATCH_ROWS)));
            }
            if let Some((_, b)) = self.cur.as_mut() {
                b.push(&Row {
                    seq: o.seq,
                    tsc: o.tsc,
                    proc_ns: o.proc_ns,
                    flags: o.flags,
                    body: &o.body,
                    prefix: o.prefix,
                });
            }
        }
    }

    /// One check of the journal: the next batch if any record became visible.
    pub fn poll_once(&mut self) -> Res<Option<Batch>> {
        crate::testing::tail_poll();
        if let Some(b) = self.out.pop_front() {
            return Ok(Some(b));
        }
        while let Step::Rec(rec) = self.scan.step()? {
            if self.kinds.iter().any(|(k, _)| *k == rec.kind) {
                self.event.push(Owned {
                    seq: rec.seq,
                    kind: rec.kind,
                    tsc: rec.tsc,
                    flags: rec.flags,
                    body: rec.body.to_vec(),
                    proc_ns: rec.proc_ns,
                    prefix: rec.prefix,
                });
            }
            if RecordFlags(rec.flags).contains(RecordFlags::EVENT_END) {
                self.commit();
            }
        }
        if let Some((_, b)) = self.cur.as_mut()
            && !b.is_empty()
        {
            let batch = b.finish().map_err(|e| Error::journal(e, None, None))?;
            self.out.push_back(batch);
        }
        Ok(self.out.pop_front())
    }
}

/// `info()["segments"]` item.
#[derive(Clone, Debug)]
pub struct SegInfo {
    pub no: u64,
    pub first_seq: u64,
    pub cold: Option<String>,
    pub hot: Option<String>,
    pub sealed: bool,
}

/// `info()`.
#[derive(Clone, Debug)]
pub struct Info {
    pub segments: Vec<SegInfo>,
    pub first_seq: u64,
    pub last_seq: u64,
}

impl Journal {
    pub fn info(self: &Arc<Self>) -> Res<Info> {
        let segments: Vec<SegInfo> = self
            .segs
            .values()
            .map(|s| SegInfo {
                no: s.no,
                first_seq: s.first_seq,
                cold: s.cold.as_ref().map(|p| p.display().to_string()),
                hot: s.hot.as_ref().map(|p| p.display().to_string()),
                sealed: s.sealed,
            })
            .collect();
        let first_seq = self.segs.values().next().map_or(0, |s| s.first_seq);
        let newest = self.segs.keys().next_back().copied().unwrap_or(0);
        let mut sc = self.scan(StartAt::SegmentBoot(newest))?;
        let mut last = 0;
        while let Step::Rec(r) = sc.step()? {
            last = r.seq;
        }
        let last_seq = last.max(sc.last_seq());
        Ok(Info {
            segments,
            first_seq,
            last_seq,
        })
    }
}
