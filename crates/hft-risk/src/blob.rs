//! Cold-path RKP1 codec. A journal contains the complete parameters, never a process-local key.
//! Integer slots are little-endian 64-bit words; collection capacities are not serialized.

use hft_refdata::RefData;
use hft_types::{AccountId, GroupId, InstId, Notional, Qty, Rate, VenueId};

use crate::{
    AcctLimits, FixedVec, GroupLimits, GroupLimitsRow, InstClassRow, MarginLimits, PreVec, PxBand,
    RiskParams, VenueLimits,
};

/// The format version is part of the magic; incompatible layouts require a new version.
pub const RISK_BLOB_MAGIC: [u8; 8] = *b"HFTRKP01";

/// Invalid encoding or an unresolved reference. No partially decoded parameters are returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiskBlobError {
    Format,
    Truncated,
    Trailing,
    Value,
    Rows,
    Order,
    Reference(&'static str),
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], RiskBlobError> {
        let b = self.bytes.get(..n).ok_or(RiskBlobError::Truncated)?;
        self.bytes = self.bytes.get(n..).ok_or(RiskBlobError::Truncated)?;
        Ok(b)
    }
    fn read<T: Wire>(&mut self) -> Result<T, RiskBlobError> {
        T::read(self)
    }
    fn rows<T: Wire>(&mut self) -> Result<Vec<T>, RiskBlobError> {
        let n = usize::try_from(self.read::<u64>()?).map_err(|_| RiskBlobError::Rows)?;
        let len = n.checked_mul(T::SIZE).ok_or(RiskBlobError::Rows)?;
        if len > self.bytes.len() {
            return Err(RiskBlobError::Truncated);
        }
        let mut rows = Vec::with_capacity(n);
        for _ in 0..n {
            rows.push(self.read()?);
        }
        Ok(rows)
    }
}

trait Wire: Sized {
    const SIZE: usize;
    fn write(&self, out: &mut Vec<u8>);
    fn read(r: &mut Reader<'_>) -> Result<Self, RiskBlobError>;
}

impl Wire for u64 {
    const SIZE: usize = 8;
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn read(r: &mut Reader<'_>) -> Result<Self, RiskBlobError> {
        let b = r.take(8)?.try_into().map_err(|_| RiskBlobError::Truncated)?;
        Ok(u64::from_le_bytes(b))
    }
}

impl Wire for i64 {
    const SIZE: usize = 8;
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn read(r: &mut Reader<'_>) -> Result<Self, RiskBlobError> {
        let b = r.take(8)?.try_into().map_err(|_| RiskBlobError::Truncated)?;
        Ok(i64::from_le_bytes(b))
    }
}

macro_rules! narrow {
    ($($t:ty),+ $(,)?) => { $(impl Wire for $t {
        const SIZE: usize = 8;
        fn write(&self, out: &mut Vec<u8>) { u64::from(*self).write(out); }
        fn read(r: &mut Reader<'_>) -> Result<Self, RiskBlobError> {
            Self::try_from(r.read::<u64>()?).map_err(|_| RiskBlobError::Value)
        }
    })+ };
}
narrow!(u8, u32);

impl Wire for bool {
    const SIZE: usize = 8;
    fn write(&self, out: &mut Vec<u8>) {
        u64::from(*self).write(out);
    }
    fn read(r: &mut Reader<'_>) -> Result<Self, RiskBlobError> {
        match r.read::<u64>()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(RiskBlobError::Value),
        }
    }
}

macro_rules! scalar {
    ($($t:ident : $inner:ty),+ $(,)?) => { $(impl Wire for $t {
        const SIZE: usize = 8;
        fn write(&self, out: &mut Vec<u8>) { self.0.write(out); }
        fn read(r: &mut Reader<'_>) -> Result<Self, RiskBlobError> {
            Ok(Self(r.read::<$inner>()?))
        }
    })+ };
}
scalar!(AccountId: u8, VenueId: u8, InstId: u32, GroupId: u32, Notional: i64, Qty: i64, Rate: i64);

macro_rules! row {
    ($t:ident { $($f:ident : $ft:ty),+ $(,)? }) => {
        impl Wire for $t {
            const SIZE: usize = 0 $(+ <$ft as Wire>::SIZE)+;
            fn write(&self, out: &mut Vec<u8>) { $(self.$f.write(out);)+ }
            fn read(r: &mut Reader<'_>) -> Result<Self, RiskBlobError> {
                Ok(Self { $($f: r.read::<$ft>()?,)+ })
            }
        }
    };
}
row!(PxBand {
    max_px_dev: Rate,
    ioc_max_cross: Rate,
    rr_max_cross: Rate
});
row!(InstClassRow {
    inst: InstId,
    class: u8
});
row!(GroupLimits {
    max_group_notional: Notional,
    max_pos_qty: Qty,
    naked_gate: Rate,
    naked_gate_min: Notional
});
row!(GroupLimitsRow {
    account: AccountId,
    group: GroupId,
    l: GroupLimits
});
row!(MarginLimits {
    liq_dist_block_open: Rate,
    liq_dist_reduce: Rate,
    liq_dist_force: Rate,
    liq_dist_hyst: Rate,
    margin_ratio_crosscheck: Rate,
    margin_ratio_hyst: Rate,
    deleverage_deadline_ms: u32,
    force_step: Rate,
    force_step_interval_ms: u32,
    margin_stale_ms: u32,
    mark_stale_ms: u32,
});
row!(AcctLimits {
    account: AccountId,
    max_order_notional: Notional,
    default_group: GroupLimits,
    max_account_notional: Notional,
    account_notional_from_equity: Rate,
    max_hedge_notional: Notional,
    max_reduce_notional: Notional,
    daily_loss_kill: Rate,
    daily_loss_abs: Notional,
    daily_loss_max_resumes: u8,
    recon_closeonly_notional: Notional,
    unknown_blocks_open: bool,
    funds_buffer: Rate,
    margin: MarginLimits,
});
row!(VenueLimits {
    venue: VenueId,
    account: AccountId,
    max_venue_notional: Notional,
    inst_bucket_per_s: u32,
    backoff_base_ms: u32,
    backoff_max_ms: u32,
    rr_reserve: Rate,
});

fn rows<T: Wire>(out: &mut Vec<u8>, rows: &[T]) {
    (rows.len() as u64).write(out);
    for row in rows {
        row.write(out);
    }
}

fn ordered<T, K: Ord>(rows: &[T], key: impl Fn(&T) -> K) -> bool {
    rows.windows(2).all(|w| match w {
        [a, b] => key(a) < key(b),
        _ => false,
    })
}

fn format(p: &RiskParams) -> Result<(), RiskBlobError> {
    if p.version == 0 || p.classes.is_empty() || p.accounts.is_empty() || p.accounts.len() > 255 {
        return Err(RiskBlobError::Value);
    }
    if !ordered(p.risk_class.as_slice(), |r| r.inst)
        || !ordered(p.accounts.as_slice(), |r| r.account)
        || !ordered(p.venues.as_slice(), |r| (r.account, r.venue))
        || !ordered(p.groups.as_slice(), |r| (r.account, r.group))
    {
        return Err(RiskBlobError::Order);
    }
    if p.risk_class
        .iter()
        .any(|r| r.inst.0 == 0 || usize::from(r.class) >= p.classes.len())
        || p.accounts.iter().any(|r| r.account.0 == 0)
        || p.venues
            .iter()
            .any(|r| r.account.0 == 0 || !(1..64).contains(&r.venue.0))
        || p.groups
            .iter()
            .any(|r| r.account.0 == 0 || r.group.0 == 0 || r.group.0 == u32::MAX)
    {
        return Err(RiskBlobError::Value);
    }
    Ok(())
}

impl RiskParams {
    /// Complete, canonical RKP1 configuration. Cold path; keys must be strictly increasing.
    /// Risk limits retain their exact integers; `Risk::new` / `set_params` enforce their semantics.
    pub fn encode_blob(&self) -> Result<Vec<u8>, RiskBlobError> {
        format(self)?;
        let mut b = RISK_BLOB_MAGIC.to_vec();
        self.version.write(&mut b);
        self.generated_at_ns.write(&mut b);
        b.extend_from_slice(&self.author);
        self.rate_headroom.write(&mut b);
        self.conn_liveness_ms.write(&mut b);
        self.max_perp_leverage_x100.write(&mut b);
        self.l3_max_cross.write(&mut b);
        rows(&mut b, &self.classes.to_vec());
        rows(&mut b, self.risk_class.as_slice());
        rows(&mut b, self.accounts.as_slice());
        rows(&mut b, self.venues.as_slice());
        rows(&mut b, self.groups.as_slice());
        Ok(b)
    }

    /// Decode only from these bytes and `rd`; no current parameters, files or global state.
    /// Rejects non-canonical order, duplicates, narrowing, truncation, trailing bytes and missing
    /// account / venue rows. Row counts are checked against the available bytes before allocation.
    pub fn decode_blob(b: &[u8], rd: &RefData) -> Result<Self, RiskBlobError> {
        let mut r = Reader { bytes: b };
        if r.take(8)? != RISK_BLOB_MAGIC {
            return Err(RiskBlobError::Format);
        }
        let version = r.read()?;
        let generated_at_ns = r.read()?;
        let author = r.take(16)?.try_into().map_err(|_| RiskBlobError::Truncated)?;
        let rate_headroom = r.read()?;
        let conn_liveness_ms = r.read()?;
        let max_perp_leverage_x100 = r.read()?;
        let l3_max_cross = r.read()?;
        let bands: Vec<PxBand> = r.rows()?;
        let mut classes = FixedVec::new();
        for v in bands {
            if !classes.push(v) {
                return Err(RiskBlobError::Rows);
            }
        }
        let p = Self {
            version,
            generated_at_ns,
            author,
            rate_headroom,
            conn_liveness_ms,
            max_perp_leverage_x100,
            l3_max_cross,
            classes,
            risk_class: PreVec::from_vec(r.rows()?),
            accounts: PreVec::from_vec(r.rows()?),
            venues: PreVec::from_vec(r.rows()?),
            groups: PreVec::from_vec(r.rows()?),
        };
        if !r.bytes.is_empty() {
            return Err(RiskBlobError::Trailing);
        }
        format(&p)?;
        for id in 1..=u8::MAX {
            let a = rd.account(AccountId(id));
            if a.id.0 == 0 {
                continue;
            }
            if !p.accounts.iter().any(|x| x.account == a.id) {
                return Err(RiskBlobError::Reference("accounts.missing"));
            }
            for v in 1..64u8 {
                if a.venues & (1u64 << v) != 0
                    && !p
                        .venues
                        .iter()
                        .any(|x| x.account == a.id && x.venue == VenueId(v))
                {
                    return Err(RiskBlobError::Reference("venues.missing"));
                }
            }
        }
        if p.accounts.iter().any(|a| rd.account(a.account).id != a.account) {
            return Err(RiskBlobError::Reference("accounts.account"));
        }
        if p.venues
            .iter()
            .any(|v| rd.account(v.account).venues & (1u64 << v.venue.0) == 0)
        {
            return Err(RiskBlobError::Reference("venues.venue"));
        }
        if p.risk_class.iter().any(|i| rd.inst_meta(i.inst).id != i.inst) {
            return Err(RiskBlobError::Reference("risk_class.inst"));
        }
        if p.groups.iter().any(|g| rd.account(g.account).id != g.account) {
            return Err(RiskBlobError::Reference("groups.account"));
        }
        Ok(p)
    }
}
