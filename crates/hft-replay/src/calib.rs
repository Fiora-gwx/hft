//! The calibration table (README "校准表"): parsed by hft-sim's `calib` (shared with the sim-gw);
//! this module adds the file hash.

use sha2::{Digest, Sha256};

use crate::venue::SimParams;

/// Applies the calibration table `text` to `p`; returns its SHA-256 (hex).
pub fn apply(text: &[u8], p: &mut SimParams) -> Result<String, String> {
    hft_sim::calib::apply(text, &mut p.maker_fill, &mut p.venue, &mut p.cond)?;
    Ok(crate::variants::hex(&Sha256::digest(text)))
}
