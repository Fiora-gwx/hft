//! `kill_admit`: the gw kill decision (README §4 step 1.1); the gw and hft-core's startup self-check
//! call this one pure function. Bit layout: hft-types `KillBits`; semantics: hft-risk.

use hft_event::GwOp;
use hft_event::OrderFlags;
use hft_types::KillBits;

/// `word` = the loaded kill word, `mask` = `GLOBAL | VENUE | HARD`, `acct_bits` =
/// `account(ix) | account_hard(ix)`. Only `Place`/`Amend` can be blocked: any `HARD` or
/// `account_hard(ix)` bit blocks them, any other bit blocks unless `oflags` has `RISK_REDUCING`.
#[inline]
pub fn kill_admit(word: u32, mask: u32, acct_bits: u32, ix: u8, op: GwOp, oflags: u16) -> bool {
    if !matches!(op, GwOp::Place | GwOp::Amend) {
        return true;
    }
    let k = word & (mask | acct_bits);
    if k & (KillBits::HARD.0 | KillBits::account_hard(ix).0) != 0 {
        return false;
    }
    k == 0 || oflags & OrderFlags::RISK_REDUCING.0 != 0
}
