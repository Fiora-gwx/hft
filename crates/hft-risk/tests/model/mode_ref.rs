//! Naive reference of the kill word over R0 (README "kill 字"): recomputed from the full list of
//! `Kill` records every time (RSK-270).

use hft_event::CapRecord;
use hft_types::{CapSource, KillLevel, VenueId};

/// Live accounts of each venue of R0, ascending (their index is the account's kill-bit index).
fn live_on(v: VenueId) -> &'static [u8] {
    match v.0 {
        2 | 3 => &[1, 2],
        5 => &[1],
        _ => &[],
    }
}

const GLOBAL: u32 = 1;
const VENUE: u32 = 2;
const HARD: u32 = 4;
const fn account(ix: usize) -> u32 {
    0x100 << ix
}
const fn account_hard(ix: usize) -> u32 {
    0x0010_0000 << ix
}

/// Kill words of the gws (gw k serves `gws[k]`).
pub fn kill_bits(caps: &[CapRecord], gws: &[VenueId]) -> Vec<u32> {
    gws.iter()
        .map(|&v| {
            let mut w = 0u32;
            for c in caps.iter().filter(|c| c.source == CapSource::KILL) {
                let hard = c.cflags & CapRecord::HARD != 0;
                let ix = live_on(v).iter().position(|a| *a == c.scope.account.0);
                match c.level {
                    l if l == KillLevel::Global as u8 => {
                        w |= GLOBAL;
                        if hard {
                            w |= HARD;
                        }
                    }
                    l if l == KillLevel::Account as u8 => {
                        if let Some(ix) = ix {
                            w |= account(ix);
                            if hard {
                                w |= account_hard(ix);
                            }
                        }
                    }
                    l if l == KillLevel::Venue as u8 && c.scope.venue == v => {
                        if c.scope.account.0 == 0 {
                            w |= VENUE;
                        } else if let Some(ix) = ix {
                            w |= account(ix);
                        }
                    }
                    _ => {}
                }
            }
            w
        })
        .collect()
}
