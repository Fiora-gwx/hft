//! Start-up gates (README "启动闸门"): `gate_bits`, venue bitmaps, `released`, the per-run
//! reset, the hedge-gate subset, the per-venue recovery release, identity and configuration
//! checks of `AccountConfigReport`, clean-restart `Recovery` release.

use hft_event::{AccountConfigReportHdr, CapRecord, NoteLevel};
use hft_types::{AccountId, CapSource, KillLevel, VenueId};

use super::caps::{Origin, record, scope_of};
use super::{
    DrainCause, GateEvent, Mode, ModeOutcome, ModeView, RecoveryFlags, cfg_params, gate_bits as gb,
    kill_params,
};
use crate::types::{N_IDENTITY_UNVERIFIED, N_RECOVERY_HELD_SIM, note};

/// `CfgMismatch` bits (hft-event codes).
pub mod mismatch {
    pub const POS_MODE: u32 = 1;
    pub const MARGIN_MODE: u32 = 2;
    pub const MULTI_ASSET: u32 = 4;
    pub const FEE_RULE: u32 = 8;
    pub const ACCT_TYPE: u32 = 16;
    pub const AUTO_BORROW: u32 = 32;
    pub const KEY_PERM: u32 = 64;
    pub const UID: u32 = 128;
    pub const UNREADABLE: u32 = 256;
}

/// Gate state of one account (= `GateRow` fields).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct GateState {
    pub bits: u32,
    pub released: bool,
    pub clean_restart: bool,
    pub recovery_flags: u8,
    pub reconciled: u64,
    pub cfg_ok: u64,
    pub identity_ok: u64,
    pub conns_ok: u64,
    pub standby_ok: u64,
    pub selftest: bool,
}

impl GateState {
    /// Venues of the hedge-gate subset (kill self-test ∧ identity ∧ mode/borrow config ∧ primary).
    pub fn hedge_bits(&self) -> u64 {
        if self.selftest {
            self.identity_ok & self.cfg_ok & self.conns_ok
        } else {
            0
        }
    }
    /// Venues released during recovery (hedge subset ∧ reconciled).
    pub fn venue_bits(&self) -> u64 {
        self.hedge_bits() & self.reconciled
    }
}

fn set_bit(m: &mut u64, v: VenueId, on: bool) {
    let b = 1u64 << (v.0 & 63);
    if on {
        *m |= b;
    } else {
        *m &= !b;
    }
}

impl Mode {
    /// Per-run reset of an account (`RecoveryFlags`; also the initial state).
    pub(crate) fn reset_gate_state(&mut self, a: AccountId, flags: u8) {
        let ix = usize::from(a.0);
        let venues = self.f.venues(a);
        let sim = self.f.is_sim(a);
        let clean = flags & RecoveryFlags::CLEAN_DRAIN != 0
            && flags
                & (RecoveryFlags::TAIL_LOST
                    | RecoveryFlags::RECOVER_MISMATCH
                    | RecoveryFlags::POISON
                    | RecoveryFlags::FROM_SNAPSHOT)
                == 0;
        if let Some(g) = self.gates.get_mut(ix) {
            *g = GateState {
                recovery_flags: flags,
                clean_restart: clean,
                ..GateState::default()
            };
            if sim {
                g.bits = gb::RECONCILED | gb::ACCOUNT_CFG | gb::IDENTITY;
                g.reconciled = venues;
                g.cfg_ok = venues;
                g.identity_ok = venues;
                g.selftest = true;
            }
        }
        for v in 0..64 {
            if let Some(r) = self.recon.get_mut(ix * 64 + v) {
                *r = super::ReconGateRow::default();
            }
            if let Some(u) = self.us.get_mut(ix * 64 + v) {
                u.us_up_seq = 0;
            }
        }
    }

    /// Recomputes `gate_bits` of one account from its states (ACCOUNT_CFG reads `lev_bad`).
    fn recompute_bits(&mut self, a: AccountId, v: &impl ModeView) {
        let ix = usize::from(a.0);
        let venues = self.f.venues(a);
        let sim = self.f.is_sim(a);
        let perp = self.f.perp & venues;
        let mut lev_ok = true;
        if !sim {
            for vv in 0u8..64 {
                if perp & (1u64 << vv) != 0 && v.lev_bad(a, VenueId(vv)) {
                    lev_ok = false;
                }
            }
        }
        let allow_missing = self.cfg.allow_missing_standby;
        let Some(g) = self.gates.get_mut(ix) else { return };
        let mut bits = g.bits & (gb::FEEDS | gb::NO_UNKNOWN | gb::CLOCK);
        // A simulated venue has no standby connection: `standby_up` counts as true for Sim accounts
        // (README "启动闸门", 模拟账户), whatever `allow_missing_standby` says.
        if venues & !g.conns_ok == 0 && (allow_missing || sim || venues & !g.standby_ok == 0) {
            bits |= gb::ORDER_CONNS;
        }
        if sim {
            bits |= gb::RECONCILED | gb::ACCOUNT_CFG | gb::IDENTITY;
        } else {
            if venues & !g.reconciled == 0 {
                bits |= gb::RECONCILED;
            }
            if venues & !g.cfg_ok == 0 && lev_ok {
                bits |= gb::ACCOUNT_CFG;
            }
            if venues & !g.identity_ok == 0 && g.selftest {
                bits |= gb::IDENTITY;
            }
        }
        g.bits = bits;
    }

    fn set_flag_bit(&mut self, a: AccountId, bit: u32, on: bool) {
        if let Some(g) = self.gates.get_mut(usize::from(a.0)) {
            if on {
                g.bits |= bit;
            } else {
                g.bits &= !bit;
            }
        }
    }

    /// One gate input; returns whether the cap set changed.
    pub(crate) fn gate(
        &mut self,
        e: GateEvent<'_>,
        seq: u64,
        v: &impl ModeView,
        out: &mut ModeOutcome,
    ) -> bool {
        let mut changed = false;
        let mut touched: Option<AccountId> = None;
        let mut all = false;
        // Unverifiable identity notes: identity bits that count before the input.
        let who = match e {
            GateEvent::Feeds { account, .. }
            | GateEvent::OrderConns { account, .. }
            | GateEvent::Unknown { account, .. }
            | GateEvent::Disposed { account, .. }
            | GateEvent::KillSelfTest { account } => Some(account),
            GateEvent::Recon(g) => Some(g.account),
            GateEvent::AccountCfg { hdr, .. } => Some(hdr.account),
            GateEvent::Clock { .. } => None,
        };
        let ident_before = who.map_or(0, |a| self.ident_counted(a));
        match e {
            GateEvent::Feeds { account, ok } => {
                self.set_flag_bit(account, gb::FEEDS, ok);
                touched = Some(account);
            }
            GateEvent::OrderConns {
                account,
                venue,
                primary_up,
                standby_up,
            } => {
                let prev = self
                    .gates
                    .get(usize::from(account.0))
                    .is_some_and(|g| g.conns_ok & (1u64 << (venue.0 & 63)) != 0);
                if let Some(g) = self.gates.get_mut(usize::from(account.0)) {
                    set_bit(&mut g.conns_ok, venue, primary_up);
                    set_bit(&mut g.standby_ok, venue, standby_up);
                }
                if let Some(u) = self.us_row_mut(account, venue) {
                    if primary_up && !prev {
                        u.us_up_seq = seq;
                    } else if !primary_up {
                        u.us_up_seq = 0;
                    }
                }
                touched = Some(account);
            }
            GateEvent::Unknown { account, count } => {
                self.set_flag_bit(account, gb::NO_UNKNOWN, count == 0);
                touched = Some(account);
            }
            GateEvent::Clock { ok } => {
                for a in 1..=self.f.n_accounts {
                    self.set_flag_bit(AccountId(a as u8), gb::CLOCK, ok);
                }
                all = true;
            }
            GateEvent::Recon(g) => {
                if !self.f.is_sim(g.account) {
                    self.store_recon(g);
                    let ok = self.recon_row(g.account, g.venue).is_some_and(|r| r.satisfied());
                    if let Some(s) = self.gates.get_mut(usize::from(g.account.0)) {
                        set_bit(&mut s.reconciled, g.venue, ok);
                    }
                }
                touched = Some(g.account);
            }
            GateEvent::Disposed {
                account,
                venue,
                report_id,
                item,
                how,
            } => {
                if !self.f.is_sim(account) {
                    self.dispose(account, venue, report_id, item, how);
                    let ok = self.recon_row(account, venue).is_some_and(|r| r.satisfied());
                    if let Some(s) = self.gates.get_mut(usize::from(account.0)) {
                        set_bit(&mut s.reconciled, venue, ok);
                    }
                }
                touched = Some(account);
            }
            GateEvent::AccountCfg { hdr, keys_required } => {
                changed |= self.account_cfg(hdr, keys_required, seq, v, out);
                touched = Some(hdr.account);
            }
            GateEvent::KillSelfTest { account } => {
                if !self.f.is_sim(account)
                    && let Some(g) = self.gates.get_mut(usize::from(account.0))
                {
                    g.selftest = true;
                }
                touched = Some(account);
            }
        }
        if let Some(a) = who
            && self
                .f
                .unverifiable
                .get(usize::from(a.0))
                .copied()
                .unwrap_or(false)
        {
            let now = self.ident_counted(a);
            let rise = now & !ident_before;
            for vv in 0u8..64 {
                if rise & (1u64 << vv) != 0 {
                    out.notes.push(note(
                        N_IDENTITY_UNVERIFIED,
                        NoteLevel::P1,
                        [u64::from(vv) | u64::from(a.0) << 8, 0, 0, 0, 0, 0, 0, 0],
                    ));
                }
            }
        }
        for ai in 1..=self.f.n_accounts {
            let a = AccountId(ai as u8);
            if !all && touched != Some(a) {
                continue;
            }
            self.recompute_bits(a, v);
            changed |= self.maybe_release(a, out);
        }
        changed
    }

    /// Identity venues counted for `IdentityUnverified` (identity bit and self-test passed).
    fn ident_counted(&self, a: AccountId) -> u64 {
        self.gates
            .get(usize::from(a.0))
            .map_or(0, |g| if g.selftest { g.identity_ok } else { 0 })
    }

    /// Latches `released` when every bit is set; clean-restart / Sim-only `Recovery` release.
    fn maybe_release(&mut self, a: AccountId, out: &mut ModeOutcome) -> bool {
        let ix = usize::from(a.0);
        let Some(g) = self.gates.get_mut(ix) else {
            return false;
        };
        if g.released || g.bits != gb::ALL {
            return false;
        }
        g.released = true;
        let (clean, flags) = (g.clean_restart, g.recovery_flags);
        out.released.push(a);
        let scope = super::account_scope(a);
        if self.find_cap(CapSource::RECOVERY, &scope).is_none() || !self.cfg.auto_resume_after_clean_drain {
            return false;
        }
        if clean {
            return self.remove_cap(CapSource::RECOVERY, &scope, out);
        }
        if self.f.sim_only {
            if flags & (RecoveryFlags::RECOVER_MISMATCH | RecoveryFlags::POISON) == 0 {
                return self.remove_cap(CapSource::RECOVERY, &scope, out);
            }
            out.notes.push(note(
                N_RECOVERY_HELD_SIM,
                NoteLevel::P1,
                [u64::from(a.0), u64::from(flags), 0, 0, 0, 0, 0, 0],
            ));
        }
        false
    }

    /// `AccountConfigReport` gate input (identity, configuration, UID mismatch, AccountCfg cap).
    fn account_cfg(
        &mut self,
        hdr: &AccountConfigReportHdr,
        keys_required: u8,
        seq: u64,
        v: &impl ModeView,
        out: &mut ModeOutcome,
    ) -> bool {
        let (a, vv) = (hdr.account, hdr.venue);
        if self.f.is_sim(a) || usize::from(a.0) > self.f.n_accounts || a.0 == 0 {
            return false;
        }
        let (rp, rm, rma) = self
            .f
            .req
            .get(usize::from(vv.0 & 63))
            .copied()
            .unwrap_or((0, 0, 0));
        let mut mism = hdr.mismatches;
        if rp != 0 && hdr.position_mode != rp {
            mism |= mismatch::POS_MODE;
        }
        if rm != 0 && hdr.margin_mode != rm {
            mism |= mismatch::MARGIN_MODE;
        }
        if rma != 0 && hdr.multi_asset != rma {
            mism |= mismatch::MULTI_ASSET;
        }
        let uid_bad = mism & mismatch::UID != 0 && mism & mismatch::UNREADABLE == 0;
        let cfg_ok = mism & !mismatch::UID == 0;
        let unverifiable = self
            .f
            .unverifiable
            .get(usize::from(a.0))
            .copied()
            .unwrap_or(false);
        let ident_ok = if uid_bad {
            false
        } else if unverifiable {
            true
        } else {
            mism & (mismatch::UID | mismatch::UNREADABLE) == 0
                && hdr.uid_keys & keys_required == keys_required
        };
        if let Some(g) = self.gates.get_mut(usize::from(a.0)) {
            set_bit(&mut g.cfg_ok, vv, cfg_ok);
            set_bit(&mut g.identity_ok, vv, ident_ok);
        }
        let mut changed = false;
        if uid_bad {
            let mut s = scope_of(&[]);
            s.account = a;
            let r = record(
                CapSource::KILL,
                KillLevel::Account as u8,
                CapRecord::HARD,
                s,
                [kill_params::CANCEL_RESTING, 0],
            );
            changed |= self.set_cap(r, seq, Origin::Auto, out);
            if out.drain.is_none() {
                out.drain = Some(DrainCause::IdentityMismatch {
                    account: a,
                    venue: vv,
                });
            }
        }
        if self.released(a) {
            changed |= self.recheck_cfg_venue(a, vv, seq, v, out);
        }
        changed
    }

    /// `AccountCfg` cap of (a, v) after release: set (reasons OR-merged) or remove.
    fn recheck_cfg_venue(
        &mut self,
        a: AccountId,
        vv: VenueId,
        seq: u64,
        v: &impl ModeView,
        out: &mut ModeOutcome,
    ) -> bool {
        let cfg_ok = self
            .gates
            .get(usize::from(a.0))
            .is_some_and(|g| g.cfg_ok & (1u64 << (vv.0 & 63)) != 0);
        let perp = self.f.perp & (1u64 << (vv.0 & 63)) != 0;
        let mut reasons = 0u64;
        if !cfg_ok {
            reasons |= cfg_params::MISMATCH;
        }
        if perp && v.lev_bad(a, vv) {
            reasons |= cfg_params::LEVERAGE;
        }
        let scope = super::account_venue_scope(a, vv);
        if reasons == 0 {
            self.remove_cap(CapSource::ACCOUNT_CFG, &scope, out)
        } else {
            let r = record(
                CapSource::ACCOUNT_CFG,
                0,
                CapRecord::CONDITIONAL,
                scope,
                [reasons, 0],
            );
            self.set_cap(r, seq, Origin::Auto, out)
        }
    }

    /// Leverage re-check of every venue of an account (`GroupsChanged`).
    pub(crate) fn recheck_account_cfg(
        &mut self,
        a: AccountId,
        seq: u64,
        v: &impl ModeView,
        out: &mut ModeOutcome,
    ) -> bool {
        if self.f.is_sim(a) {
            return false;
        }
        let mut changed = false;
        if self.released(a) {
            let venues = self.f.venues(a);
            for vv in 0u8..64 {
                if venues & (1u64 << vv) != 0 {
                    changed |= self.recheck_cfg_venue(a, VenueId(vv), seq, v, out);
                }
            }
        }
        self.recompute_bits(a, v);
        changed |= self.maybe_release(a, out);
        changed
    }
}
