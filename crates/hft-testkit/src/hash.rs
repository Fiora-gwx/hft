//! State-hash comparison (README "7. 状态哈希对比"): `HashTrail`, `ComponentHashes`, `diff_trails`,
//! `bisect_divergence` and the per-component hashes of a `StateSnapshot`.
//!
//! Component hashes are taken from the snapshot's sections (journal-format "快照记录结构"): every
//! component's hash is `xxh3_64` over the bytes (`SectionHdr` + body) of its sections in write
//! order; `all` is the snapshot header's `state_hash` (for `purpose = Keyframe`, whose header
//! carries 0, the reader computes `xxh3_128` over every section except 16 in write order).

use hft_event::{SectionHdr, SnapshotHdr};
use xxhash_rust::xxh3::{Xxh3, xxh3_64};
use zerocopy::FromBytes;

/// The components a trail point distinguishes, in the sequencer's section write order (the order
/// of each component's first section number).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Component {
    Ledger,
    Oms,
    Risk,
    Strategies,
    Book,
    Exec,
    /// Only `all` differs (a section outside the six components).
    All,
}

impl Component {
    pub const fn name(self) -> &'static str {
        match self {
            Component::Ledger => "ledger",
            Component::Oms => "oms",
            Component::Risk => "risk",
            Component::Strategies => "strategies",
            Component::Book => "book",
            Component::Exec => "exec",
            Component::All => "all",
        }
    }
}

/// Section number → component (journal-format section table). Sections not listed (timers,
/// active config, market last, command dedup, recon pending, day, notices, engine clock, conn
/// liveness) only enter `all`; section 16 enters nothing.
pub const SECTION_COMPONENT: &[(u16, Component)] = &[
    (1, Component::Ledger),
    (2, Component::Oms),
    (3, Component::Oms),
    (4, Component::Risk),
    (5, Component::Strategies),
    (6, Component::Book),
    (7, Component::Book),
    (12, Component::Oms),
    (13, Component::Ledger),
    (14, Component::Risk),
    (17, Component::Risk),
    (18, Component::Risk),
    (21, Component::Exec),
    (22, Component::Ledger),
    (23, Component::Risk),
];

/// Section excluded from `state_hash` (ClockCal).
pub const SECTION_CLOCK_CAL: u16 = 16;
/// `SnapshotPurpose::Keyframe`.
pub const PURPOSE_KEYFRAME: u8 = 2;

/// Hashes of one state.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct ComponentHashes {
    pub book: u64,
    pub oms: u64,
    pub risk: u64,
    pub ledger: u64,
    pub exec: u64,
    pub strategies: u64,
    /// The snapshot header's `state_hash` (xxh3-128).
    pub all: u128,
}

impl ComponentHashes {
    /// First differing component in write order; `All` when only `all` differs; `None` if equal.
    pub fn first_diff(&self, o: &ComponentHashes) -> Option<Component> {
        let pairs = [
            (Component::Ledger, self.ledger, o.ledger),
            (Component::Oms, self.oms, o.oms),
            (Component::Risk, self.risk, o.risk),
            (Component::Strategies, self.strategies, o.strategies),
            (Component::Book, self.book, o.book),
            (Component::Exec, self.exec, o.exec),
        ];
        pairs
            .iter()
            .find(|(_, a, b)| a != b)
            .map(|(c, _, _)| *c)
            .or((self.all != o.all).then_some(Component::All))
    }
    fn get_mut(&mut self, c: Component) -> Option<&mut u64> {
        match c {
            Component::Ledger => Some(&mut self.ledger),
            Component::Oms => Some(&mut self.oms),
            Component::Risk => Some(&mut self.risk),
            Component::Strategies => Some(&mut self.strategies),
            Component::Book => Some(&mut self.book),
            Component::Exec => Some(&mut self.exec),
            Component::All => None,
        }
    }
}

/// Sampled hashes of one run: a point every `every_n` (DST: Input ordinal; replay: journal seq).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct HashTrail {
    pub every_n: u64,
    pub points: Vec<(u64, ComponentHashes)>,
}

impl HashTrail {
    pub fn new(every_n: u64) -> HashTrail {
        HashTrail {
            every_n,
            points: Vec::new(),
        }
    }
    /// Records `(seq, h)` when `seq` is a positive multiple of `every_n`.
    pub fn sample(&mut self, seq: u64, h: ComponentHashes) {
        if self.every_n != 0 && seq != 0 && seq.is_multiple_of(self.every_n) {
            self.points.push((seq, h));
        }
    }
    /// Canonical bytes (`every_n`, then per point seq and the seven hashes, little-endian).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(8 + self.points.len() * 72);
        b.extend_from_slice(&self.every_n.to_le_bytes());
        for (s, h) in &self.points {
            b.extend_from_slice(&s.to_le_bytes());
            for v in [h.book, h.oms, h.risk, h.ledger, h.exec, h.strategies] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b.extend_from_slice(&h.all.to_le_bytes());
        }
        b
    }
}

/// First divergence of two trails.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Divergence {
    pub seq: u64,
    /// `None` = the point exists in only one trail.
    pub component: Option<Component>,
}

/// First point where the trails differ (component = first in write order), or where one trail has
/// a point the other lacks; `None` if identical.
pub fn diff_trails(a: &HashTrail, b: &HashTrail) -> Option<Divergence> {
    let n = a.points.len().max(b.points.len());
    for i in 0..n {
        match (a.points.get(i), b.points.get(i)) {
            (Some((sa, ha)), Some((sb, hb))) => {
                if sa != sb {
                    return Some(Divergence {
                        seq: (*sa).min(*sb),
                        component: None,
                    });
                }
                if let Some(c) = ha.first_diff(hb) {
                    return Some(Divergence {
                        seq: *sa,
                        component: Some(c),
                    });
                }
            }
            (Some((s, _)), None) | (None, Some((s, _))) => {
                return Some(Divergence {
                    seq: *s,
                    component: None,
                });
            }
            (None, None) => return None,
        }
    }
    None
}

/// Smallest seq in `(lo, hi]` at which the two sides differ, given that they agree at `lo` and
/// differ at `hi`. `run(seq)` replays both sides to `seq` and returns `(a, b)`; it is called
/// ⌈log2(hi − lo)⌉ times.
pub fn bisect_divergence(
    mut run: impl FnMut(u64) -> (ComponentHashes, ComponentHashes),
    lo: u64,
    hi: u64,
) -> u64 {
    let (mut good, mut bad) = (lo, hi);
    while bad - good > 1 {
        let mid = good + (bad - good) / 2;
        let (a, b) = run(mid);
        if a == b {
            good = mid;
        } else {
            bad = mid;
        }
    }
    bad
}

/// Snapshot parse error.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SnapErr {
    Short,
    /// A section runs past the body.
    Section(u16),
}

/// Sections of a snapshot: number and full bytes (`SectionHdr` + body), in write order.
pub type Sections<'a> = Vec<(u16, &'a [u8])>;

/// The header and the sections of a snapshot.
pub fn sections(logical: &[u8]) -> Result<(SnapshotHdr, Sections<'_>), SnapErr> {
    let (hdr, mut rest) = SnapshotHdr::read_from_prefix(logical).map_err(|_| SnapErr::Short)?;
    let mut out = Vec::with_capacity(hdr.n_sections as usize);
    for _ in 0..hdr.n_sections {
        let (sh, _) = SectionHdr::read_from_prefix(rest).map_err(|_| SnapErr::Short)?;
        let total = 16 + (sh.len as usize).div_ceil(8) * 8;
        let bytes = rest.get(..total).ok_or(SnapErr::Section(sh.section))?;
        out.push((sh.section, bytes));
        rest = &rest[total..];
    }
    Ok((hdr, out))
}

/// `xxh3_128` over every section except 16, in write order (the reader-side keyframe hash).
pub fn state_hash_of(secs: &[(u16, &[u8])]) -> u128 {
    let mut h = Xxh3::new();
    for (n, b) in secs {
        if *n != SECTION_CLOCK_CAL {
            h.update(b);
        }
    }
    h.digest128()
}

/// Component hashes of a `StateSnapshot` logical body.
pub fn component_hashes(logical: &[u8]) -> Result<ComponentHashes, SnapErr> {
    let (hdr, secs) = sections(logical)?;
    let mut acc: Vec<(Component, Vec<u8>)> = Vec::new();
    for (n, b) in &secs {
        if let Some((_, c)) = SECTION_COMPONENT.iter().find(|(s, _)| s == n) {
            match acc.iter_mut().find(|(k, _)| k == c) {
                Some((_, v)) => v.extend_from_slice(b),
                None => acc.push((*c, b.to_vec())),
            }
        }
    }
    let mut h = ComponentHashes::default();
    for (c, bytes) in acc {
        if let Some(slot) = h.get_mut(c) {
            *slot = xxh3_64(&bytes);
        }
    }
    h.all = if hdr.purpose == PURPOSE_KEYFRAME {
        state_hash_of(&secs)
    } else {
        u128::from_le_bytes(hdr.state_hash)
    };
    Ok(h)
}
