//! `CostLine` and `fee_round_trip` (README "SpreadCalc 与 CostLine"): cost items with purpose bits,
//! the three totals precomputed on parameter changes, and the two-leg fee round trip from refdata.

use core::mem::{align_of, offset_of, size_of};

use hft_strategy_sdk::RefView;
use hft_types::{AccountId, InstId, Rate};

/// Purpose bits of a [`CostItem`].
pub mod cost_flags {
    /// Counts in the round-trip cost (compared with `rt_floor`).
    pub const ROUND_TRIP: u8 = 1;
    /// Fixed add-on outside the round trip (execution cost, margin); exclusive with `ROUND_TRIP`.
    pub const FIXED: u8 = 2;
    /// Cost not contained in either quoted price and still paid when closing (break-even line).
    pub const OUT_OF_QUOTE: u8 = 4;
    /// Every defined bit.
    pub const ALL: u8 = ROUND_TRIP | FIXED | OUT_OF_QUOTE;
}

/// One cost item (16 B): a rate and its purpose bits. The strategy chooses the items and values;
/// this library only sums by bit.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CostItem {
    pub rate: Rate,
    /// Strategy-defined; only used in Notes and dashboards.
    pub label: u16,
    /// [`cost_flags`] bits.
    pub flags: u8,
    pub _pad: [u8; 5],
}
const _: () = assert!(size_of::<CostItem>() == 16);
const _: () = assert!(align_of::<CostItem>() == 8);
const _: () = assert!(offset_of!(CostItem, rate) == 0);
const _: () = assert!(offset_of!(CostItem, label) == 8);
const _: () = assert!(offset_of!(CostItem, flags) == 10);
const _: () = assert!(offset_of!(CostItem, _pad) == 11);

impl Default for CostItem {
    fn default() -> Self {
        CostItem::ZERO
    }
}

impl CostItem {
    pub const ZERO: CostItem = CostItem {
        rate: Rate(0),
        label: 0,
        flags: 0,
        _pad: [0; 5],
    };

    pub const fn new(rate: Rate, label: u16, flags: u8) -> CostItem {
        CostItem {
            rate,
            label,
            flags,
            _pad: [0; 5],
        }
    }
}

/// Up to 8 cost items with the totals precomputed by [`CostLine::set`] (168 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CostLine {
    pub items: [CostItem; 8],
    pub n: u8,
    pub _pad: [u8; 7],
    pub rt_floor: Rate,
    rt: Rate,
    open_floor: Rate,
    out_of_quote: Rate,
}
const _: () = assert!(size_of::<CostLine>() == 168);
const _: () = assert!(align_of::<CostLine>() == 8);
const _: () = assert!(offset_of!(CostLine, items) == 0);
const _: () = assert!(offset_of!(CostLine, n) == 128);
const _: () = assert!(offset_of!(CostLine, _pad) == 129);
const _: () = assert!(offset_of!(CostLine, rt_floor) == 136);
const _: () = assert!(offset_of!(CostLine, rt) == 144);
const _: () = assert!(offset_of!(CostLine, open_floor) == 152);
const _: () = assert!(offset_of!(CostLine, out_of_quote) == 160);

/// `CostLine::set` failure (the line is unchanged).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum CostErr {
    /// More than 8 items.
    TooMany,
    /// An undefined bit, or `ROUND_TRIP` together with `FIXED`.
    BadFlags,
    /// A total does not fit i64.
    Overflow,
}

impl Default for CostLine {
    fn default() -> Self {
        CostLine::EMPTY
    }
}

impl CostLine {
    /// No items, all totals 0.
    pub const EMPTY: CostLine = CostLine {
        items: [CostItem::ZERO; 8],
        n: 0,
        _pad: [0; 7],
        rt_floor: Rate(0),
        rt: Rate(0),
        open_floor: Rate(0),
        out_of_quote: Rate(0),
    };

    /// Replaces the items and `rt_floor` and precomputes the totals (parameter changes only).
    /// On `Err` the line is unchanged.
    pub fn set(&mut self, items: &[CostItem], rt_floor: Rate) -> Result<(), CostErr> {
        if items.len() > 8 {
            return Err(CostErr::TooMany);
        }
        let (mut rt, mut fixed, mut ooq) = (0i128, 0i128, 0i128);
        for it in items {
            let f = it.flags;
            if f & !cost_flags::ALL != 0 || (f & cost_flags::ROUND_TRIP != 0 && f & cost_flags::FIXED != 0) {
                return Err(CostErr::BadFlags);
            }
            let r = i128::from(it.rate.0);
            if f & cost_flags::ROUND_TRIP != 0 {
                rt = rt.saturating_add(r);
            }
            if f & cost_flags::FIXED != 0 {
                fixed = fixed.saturating_add(r);
            }
            if f & cost_flags::OUT_OF_QUOTE != 0 {
                ooq = ooq.saturating_add(r);
            }
        }
        let round_trip = rt.max(i128::from(rt_floor.0));
        let open_floor = round_trip.saturating_add(fixed);
        let fit = |v: i128| i64::try_from(v).map_err(|_| CostErr::Overflow);
        let (rt_v, of_v, ooq_v) = (fit(round_trip)?, fit(open_floor)?, fit(ooq)?);
        let mut arr = [CostItem::ZERO; 8];
        for (d, s) in arr.iter_mut().zip(items) {
            *d = *s;
        }
        self.items = arr;
        self.n = items.len() as u8;
        self._pad = [0; 7];
        self.rt_floor = rt_floor;
        self.rt = Rate(rt_v);
        self.open_floor = Rate(of_v);
        self.out_of_quote = Rate(ooq_v);
        Ok(())
    }

    /// `max(rt_floor, Σ ROUND_TRIP items)`.
    #[inline]
    pub fn round_trip(&self) -> Rate {
        self.rt
    }

    /// `round_trip() + Σ FIXED items`.
    #[inline]
    pub fn open_floor(&self) -> Rate {
        self.open_floor
    }

    /// `Σ OUT_OF_QUOTE items`.
    #[inline]
    pub fn out_of_quote(&self) -> Rate {
        self.out_of_quote
    }
}

/// Maker or taker fee of a leg.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Liquidity {
    Taker,
    Maker,
}

/// One leg of [`fee_round_trip`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FeeLeg {
    pub inst: InstId,
    pub account: AccountId,
    pub liq: Liquidity,
}

/// `fee_round_trip` failure (decided by the first failing leg).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum FeeErr {
    /// The inst is not in refdata.
    UnknownInst,
    /// No fee for (account, inst): the account is not the bound one, or the inst's venue is outside
    /// the account's venues.
    NoFeeTier,
}

/// Two-leg fee round trip `2 × Σ fee(leg)` from the refdata fee schedule (taker or maker; maker may
/// be negative = rebate). Parameter changes only; summed in i128 and saturated to `±i64::MAX`.
/// `VenueCaps.fee_asset_rule` is not read (it changes the net fill, not the rate).
pub fn fee_round_trip(rd: &RefView<'_>, legs: &[FeeLeg]) -> Result<Rate, FeeErr> {
    let mut sum = 0i128;
    for l in legs {
        if rd.try_inst(l.inst).is_none() {
            return Err(FeeErr::UnknownInst);
        }
        let f = rd.try_fee(l.account, l.inst).ok_or(FeeErr::NoFeeTier)?;
        let r = match l.liq {
            Liquidity::Taker => f.taker,
            Liquidity::Maker => f.maker,
        };
        sum = sum.saturating_add(i128::from(r.0));
    }
    let v = sum.saturating_mul(2);
    let lim = i128::from(i64::MAX);
    Ok(Rate(v.clamp(-lim, lim) as i64))
}
