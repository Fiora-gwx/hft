//! `GroupTable` (the `GroupSpec`s of one instance and parameter version) and the group
//! fingerprint `GroupSpecExt::fingerprint` (README "Strategy 与 Model", "参数变化时的 group 重置").

use hft_event::{GroupSpec, LegDef};
use hft_types::{AssetId, GroupId, InstId, VenueId};

const ZERO_LEG: LegDef = LegDef {
    inst: InstId(0),
    venue: VenueId(0),
    kind: 0,
    _pad: 0,
    route: 0,
    _pad2: 0,
};
/// Returned by `GroupTable::get` for an out-of-range gix (all zero, `id = GROUP_NONE`).
static ZERO_SPEC: GroupSpec = GroupSpec {
    id: GroupId(0),
    asset: AssetId(0),
    n_legs: 0,
    open_allowed: 0,
    legs: [ZERO_LEG; 4],
    _rsv: 0,
};

/// The `GroupSpec` list of one instance for one parameter version; gix = index in the SetParams
/// blob. Built on the command path (allocates once); every query is allocation-free.
#[derive(Clone, Debug)]
pub struct GroupTable {
    specs: Box<[GroupSpec]>,
    /// gix sorted by `GroupId` (binary search for `gix_of`).
    by_id: Box<[u16]>,
    cap: u16,
}

impl GroupTable {
    /// Builds the table (command path). `capacity` = `max_groups_per_instance` (ECF1
    /// `Capacities`); a table longer than its capacity is representable so that
    /// `ParamSet::decode` can reject it with `TooManyGroups`. Specs beyond `u16::MAX` are dropped.
    /// Duplicate `GroupId`s are not checked here (the gateway and the sequencer reject them);
    /// `gix_of` then returns one of them.
    pub fn new(specs: &[GroupSpec], capacity: u16) -> GroupTable {
        let specs = specs.get(..specs.len().min(usize::from(u16::MAX))).unwrap_or(&[]);
        let specs: Box<[GroupSpec]> = specs.into();
        let mut by_id: Vec<u16> = (0..specs.len()).filter_map(|i| u16::try_from(i).ok()).collect();
        by_id.sort_by_key(|&g| specs.get(usize::from(g)).map_or(0, |s| s.id.0));
        GroupTable {
            specs,
            by_id: by_id.into_boxed_slice(),
            cap: capacity,
        }
    }
    /// Number of groups.
    pub fn len(&self) -> u16 {
        u16::try_from(self.specs.len()).unwrap_or(u16::MAX)
    }
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }
    /// `max_groups_per_instance`; strategies preallocate per-group state to it.
    pub fn capacity(&self) -> u16 {
        self.cap
    }
    /// Spec at `gix`; out of range → an all-zero spec (`id = GROUP_NONE`, `n_legs = 0`).
    pub fn get(&self, gix: u16) -> &GroupSpec {
        self.specs.get(usize::from(gix)).unwrap_or(&ZERO_SPEC)
    }
    /// gix of `g` in this version (binary search).
    pub fn gix_of(&self, g: GroupId) -> Option<u16> {
        let i = self
            .by_id
            .binary_search_by_key(&g.0, |&x| self.specs.get(usize::from(x)).map_or(0, |s| s.id.0))
            .ok()?;
        self.by_id.get(i).copied()
    }
    /// `(gix, spec)` in gix order.
    pub fn iter(&self) -> impl Iterator<Item = (u16, &GroupSpec)> + '_ {
        self.specs
            .iter()
            .enumerate()
            .filter_map(|(i, s)| u16::try_from(i).ok().map(|g| (g, s)))
    }
    /// All specs in gix order.
    pub fn specs(&self) -> &[GroupSpec] {
        &self.specs
    }
}

/// Group fingerprint (FNV-1a 64 over asset, n_legs and the first n_legs legs'
/// (inst, venue, kind, route), excluding `open_allowed`); Python and pipeline hash the same bytes.
pub trait GroupSpecExt {
    fn fingerprint(&self) -> u64;
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[inline]
fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

impl GroupSpecExt for GroupSpec {
    fn fingerprint(&self) -> u64 {
        let mut h = fnv(FNV_OFFSET, &self.asset.0.to_le_bytes());
        h = fnv(h, &[self.n_legs]);
        let n = usize::from(self.n_legs).min(4);
        for l in self.legs.iter().take(n) {
            h = fnv(h, &l.inst.0.to_le_bytes());
            h = fnv(h, &[l.venue.0, l.kind]);
            h = fnv(h, &l.route.to_le_bytes());
        }
        h
    }
}
