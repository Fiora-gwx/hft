//! Fee resolution: per-inst override of the account > per-venue override of the account >
//! `Fees (venue, tier)` with tier = 0 for Live and the Sim account's `fee_tier`.

use crate::blob::{FeeOverrideRow, FeeRow};
use crate::types::FeePair;
use hft_types::Rate;

/// Number of `VenueId` slots kept in per-venue arrays (venue bitmaps are 64 bits).
pub(crate) const VENUE_SLOTS: usize = 64;

#[inline]
fn pair(maker: i64, taker: i64) -> FeePair {
    FeePair {
        maker: Rate(maker),
        taker: Rate(taker),
    }
}

/// Sorted views of the Fees and FeeOverrides rows (first row wins on duplicate keys).
#[derive(Clone, Debug, Default)]
pub(crate) struct FeeRows {
    tiers: Vec<((u8, u8), FeePair)>,
    overrides: Vec<((u8, u8, u32), FeePair)>,
}

impl FeeRows {
    pub(crate) fn new(fees: &[FeeRow], overrides: &[FeeOverrideRow]) -> FeeRows {
        let mut tiers: Vec<((u8, u8), FeePair)> = fees
            .iter()
            .map(|f| ((f.venue, f.tier), pair(f.maker, f.taker)))
            .collect();
        tiers.sort_by_key(|(k, _)| *k);
        tiers.dedup_by_key(|(k, _)| *k);
        let mut ov: Vec<((u8, u8, u32), FeePair)> = overrides
            .iter()
            .map(|o| ((o.account, o.venue, o.inst), pair(o.maker, o.taker)))
            .collect();
        ov.sort_by_key(|(k, _)| *k);
        ov.dedup_by_key(|(k, _)| *k);
        FeeRows { tiers, overrides: ov }
    }

    fn find_ov(&self, k: (u8, u8, u32)) -> Option<FeePair> {
        let i = self.overrides.binary_search_by_key(&k, |(k, _)| *k).ok()?;
        self.overrides.get(i).map(|(_, p)| *p)
    }

    /// Effective fee of `account` (fee tier `tier`) for `inst` on `venue` (`inst = 0` = venue-wide).
    pub(crate) fn resolve(&self, account: u8, tier: u8, venue: u8, inst: u32) -> Option<FeePair> {
        if inst != 0
            && let Some(p) = self.find_ov((account, venue, inst))
        {
            return Some(p);
        }
        self.find_ov((account, venue, 0)).or_else(|| {
            let i = self
                .tiers
                .binary_search_by_key(&(venue, tier), |(k, _)| *k)
                .ok()?;
            self.tiers.get(i).map(|(_, p)| *p)
        })
    }
}

/// Resolved fee tables of a loaded `RefData`.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct FeeTable {
    /// `(account × 64 + venue)` → venue-wide fee (override, else tier row).
    default: Box<[Option<FeePair>]>,
    /// `(account, inst)` → per-inst override, ascending.
    per_inst: Box<[(u8, u32, FeePair)]>,
}

impl FeeTable {
    /// `tiers[a]` = fee tier of account `a` (index 0 unused).
    pub(crate) fn build(rows: &FeeRows, tiers: &[u8]) -> FeeTable {
        let mut default = vec![None; tiers.len() * VENUE_SLOTS];
        for (a, &tier) in tiers.iter().enumerate().skip(1) {
            for v in 0..VENUE_SLOTS {
                if let Some(slot) = default.get_mut(a * VENUE_SLOTS + v) {
                    *slot = rows.resolve(a as u8, tier, v as u8, 0);
                }
            }
        }
        let per_inst: Vec<(u8, u32, FeePair)> = rows
            .overrides
            .iter()
            .filter(|((_, _, i), _)| *i != 0)
            .map(|((a, _, i), p)| (*a, *i, *p))
            .collect();
        let mut per_inst = per_inst;
        per_inst.sort_by_key(|(a, i, _)| (*a, *i));
        per_inst.dedup_by_key(|(a, i, _)| (*a, *i));
        FeeTable {
            default: default.into(),
            per_inst: per_inst.into(),
        }
    }

    #[inline]
    pub(crate) fn get(&self, account: u8, venue: u8, inst: u32) -> Option<FeePair> {
        if usize::from(venue) >= VENUE_SLOTS {
            return None;
        }
        if !self.per_inst.is_empty()
            && let Ok(i) = self
                .per_inst
                .binary_search_by_key(&(account, inst), |(a, i, _)| (*a, *i))
        {
            return self.per_inst.get(i).map(|(_, _, p)| *p);
        }
        let ix = usize::from(account) * VENUE_SLOTS + usize::from(venue);
        self.default.get(ix).copied().flatten()
    }
}
