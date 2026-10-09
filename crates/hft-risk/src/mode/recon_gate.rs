//! Reconciliation gate rows (the latest group of each (account, venue) with its non-Ignore item
//! keys and dispositions) and the user-stream rows of the `AUTO_EXTERNAL` precondition
//! (README "对账闸门与 HARD 解除的严格条件", "对账分级" item 7).
// zerocopy's derive helpers of `ReconItemKey` carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_event::CapRecord;
use hft_types::{AccountId, CapSource, VenueId};

use super::{Disposition, Mode};
use crate::vec::FixedVec;

/// `ReconGateRowHdr.flags` / `ReconGateInput.flags` bits.
pub mod recon_flags {
    pub const TRUNCATED: u16 = 1;
    pub const INFLIGHT: u16 = 2;
    pub const USERSTREAM_DOWN: u16 = 4;
    /// More than 64 non-Ignore items.
    pub const OVERFLOW: u16 = 8;
}

/// Item key: `ReconcileReport.item` and the inst (position, open order) or asset (balance).
#[repr(C)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    Default,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct ReconItemKey {
    pub item: u8,
    pub _pad: [u8; 3],
    /// `InstId` or `AssetId` (widened).
    pub id: u32,
}
const _: () = assert!(size_of::<ReconItemKey>() == 8);
const _: () = assert!(align_of::<ReconItemKey>() == 4);
const _: () = assert!(core::mem::offset_of!(ReconItemKey, item) == 0);
const _: () = assert!(core::mem::offset_of!(ReconItemKey, _pad) == 1);
const _: () = assert!(core::mem::offset_of!(ReconItemKey, id) == 4);

impl ReconItemKey {
    pub const POSITION: u8 = 1;
    pub const BALANCE: u8 = 2;
    pub const OPEN_ORDER: u8 = 3;

    pub const fn new(item: u8, id: u32) -> ReconItemKey {
        ReconItemKey {
            item,
            _pad: [0; 3],
            id,
        }
    }
}

/// Gate input of one reconciliation group with a `Summary` (from `Risk::on_reconcile`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ReconGateInput {
    pub account: AccountId,
    pub venue: VenueId,
    pub report_id: u64,
    pub view_seq: u64,
    /// Non-Ignore item keys (at most 64; `OVERFLOW` beyond).
    pub keys: FixedVec<ReconItemKey, 64>,
    /// Number of non-Ignore items (may exceed 64).
    pub n_items: u16,
    pub flags: u16,
}

/// Latest group of one (account, venue).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct ReconGateRow {
    pub present: bool,
    pub report_id: u64,
    pub disposed_mask: u64,
    pub n_items: u16,
    pub flags: u16,
    pub view_seq: u64,
    pub keys: [ReconItemKey; 64],
}

impl Default for ReconGateRow {
    fn default() -> Self {
        ReconGateRow {
            present: false,
            report_id: 0,
            disposed_mask: 0,
            n_items: 0,
            flags: 0,
            view_seq: 0,
            keys: [ReconItemKey::default(); 64],
        }
    }
}

impl ReconGateRow {
    /// The strict condition (README items 1 and 2).
    pub fn satisfied(&self) -> bool {
        let bad = recon_flags::TRUNCATED
            | recon_flags::INFLIGHT
            | recon_flags::USERSTREAM_DOWN
            | recon_flags::OVERFLOW;
        if !self.present || self.flags & bad != 0 || self.n_items > 64 {
            return false;
        }
        let all = if self.n_items == 64 {
            u64::MAX
        } else {
            (1u64 << self.n_items) - 1
        };
        self.disposed_mask & all == all
    }
}

/// User-stream row of one (account, venue).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct UsRow {
    pub us_up_seq: u64,
    pub prev_view_seq: u64,
    pub uflags: u8,
}

/// `RiskUsRow.uflags`.
pub(crate) const PREV_US_DOWN: u8 = 1;

impl Mode {
    pub(crate) fn recon_row(&self, a: AccountId, v: VenueId) -> Option<&ReconGateRow> {
        self.recon.get(usize::from(a.0) * 64 + usize::from(v.0 & 63))
    }

    pub(crate) fn us_row_mut(&mut self, a: AccountId, v: VenueId) -> Option<&mut UsRow> {
        self.us.get_mut(usize::from(a.0) * 64 + usize::from(v.0 & 63))
    }

    pub(crate) fn us_row(&self, a: AccountId, v: VenueId) -> UsRow {
        self.us
            .get(usize::from(a.0) * 64 + usize::from(v.0 & 63))
            .copied()
            .unwrap_or_default()
    }

    /// Stores a new group (the previous one and its dispositions are void).
    pub(crate) fn store_recon(&mut self, g: &ReconGateInput) {
        if let Some(r) = self
            .recon
            .get_mut(usize::from(g.account.0) * 64 + usize::from(g.venue.0 & 63))
        {
            *r = ReconGateRow::default();
            r.present = true;
            r.report_id = g.report_id;
            r.n_items = g.n_items;
            r.flags = g.flags;
            r.view_seq = g.view_seq;
            for (d, s) in r.keys.iter_mut().zip(g.keys.iter()) {
                *d = *s;
            }
        }
    }

    /// Marks one item of the stored group as disposed (wrong report or unknown key → ignored).
    pub(crate) fn dispose(
        &mut self,
        a: AccountId,
        v: VenueId,
        report_id: u64,
        key: ReconItemKey,
        how: Disposition,
    ) {
        let ok_kind = match how {
            Disposition::Adopted => key.item == ReconItemKey::POSITION || key.item == ReconItemKey::BALANCE,
            Disposition::Baselined => key.item == ReconItemKey::BALANCE,
            Disposition::AutoExternal => key.item == ReconItemKey::POSITION,
        };
        if !ok_kind {
            return;
        }
        if let Some(r) = self.recon.get_mut(usize::from(a.0) * 64 + usize::from(v.0 & 63))
            && r.present
            && r.report_id == report_id
        {
            let n = usize::from(r.n_items).min(64);
            if let Some(k) = r.keys.iter().take(n).position(|k| *k == key) {
                r.disposed_mask |= 1u64 << k;
            }
        }
    }

    /// HARD release: every Live affected account, every venue: latest group later than the
    /// cap's `set_seq` and strict.
    pub(crate) fn hard_release_clean(&self, r: &CapRecord) -> bool {
        debug_assert!(r.source == CapSource::KILL);
        for a in self.f.accounts() {
            if (r.scope.account.0 != 0 && r.scope.account != a) || self.f.is_sim(a) {
                continue;
            }
            for v in 0u8..64 {
                if self.f.venues(a) & (1u64 << v) == 0 {
                    continue;
                }
                match self.recon_row(a, VenueId(v)) {
                    Some(row) if row.present && row.view_seq > r.set_seq && row.satisfied() => {}
                    _ => return false,
                }
            }
        }
        true
    }
}
