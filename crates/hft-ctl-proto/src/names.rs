//! Name ↔ dense id resolution against refdata. Names are exact (no case folding); the only other
//! accepted spelling is the decimal id string.

use hft_refdata::{AccountMeta, InstStatus, RefData};
use hft_types::{AccountId, AssetId, InstId, StrategyId, VenueId};

/// Venue protocol names (the `venues.<v>.protocol` names of apps/hft-core) by `VenueId`.
pub const VENUE_NAMES: &[(VenueId, &str)] = &[
    (VenueId::BINANCE_USDM, "binance-usdm"),
    (VenueId::BINANCE_SPOT, "binance-spot"),
    (VenueId::GATE_USDT_FUT, "gate-futures"),
    (VenueId::GATE_SPOT, "gate-spot"),
    (VenueId::OKX_SWAP, "okx-swap"),
    (VenueId::OKX_SPOT, "okx-spot"),
    (VenueId::BYBIT_LINEAR, "bybit-linear"),
    (VenueId::BYBIT_SPOT, "bybit-spot"),
    (VenueId::BITGET_UTA_USDT_FUT, "bitget-usdt-futures"),
    (VenueId::BITGET_UTA_SPOT, "bitget-spot"),
    (VenueId::KUCOIN_FUT, "kucoin-futures"),
    (VenueId::KUCOIN_SPOT, "kucoin-spot"),
    (VenueId::HYPERLIQUID_PERP, "hyperliquid-perp"),
    (VenueId::POLYMARKET_CLOB, "polymarket-clob"),
];

/// Protocol name of a venue id; unassigned → `None`.
pub fn venue_name(v: VenueId) -> Option<&'static str> {
    VENUE_NAMES.iter().find(|(id, _)| *id == v).map(|(_, n)| *n)
}

fn dec_id(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// A venue present in refdata, by name or id string.
pub fn venue(rd: &RefData, s: &str) -> Option<VenueId> {
    let v = match dec_id(s) {
        Some(n) => VenueId(u8::try_from(n).ok()?),
        None => VENUE_NAMES.iter().find(|(_, n)| *n == s).map(|(id, _)| *id)?,
    };
    (v.0 != 0 && rd.venue_row(v).is_some()).then_some(v)
}

/// A strategy instance by name or id string.
pub fn instance(rd: &RefData, s: &str) -> Option<StrategyId> {
    if let Some(n) = dec_id(s) {
        let id = StrategyId(u8::try_from(n).ok()?);
        return (id.0 != 0 && rd.instance(id).is_some()).then_some(id);
    }
    rd.instances_in_order()
        .iter()
        .find(|i| &*i.name == s)
        .map(|i| i.id)
}

fn account_exists(m: &AccountMeta) -> bool {
    m.id.0 != 0
}

/// An account by label or id string.
pub fn account(rd: &RefData, s: &str) -> Option<AccountId> {
    if let Some(n) = dec_id(s) {
        let id = AccountId(u8::try_from(n).ok()?);
        return (id.0 != 0 && account_exists(rd.account(id))).then_some(id);
    }
    (1..=u8::MAX)
        .map(AccountId)
        .find(|&a| account_exists(rd.account(a)) && &*rd.account(a).label == s)
}

/// Label of an account (empty for unknown ids).
pub fn account_label(rd: &RefData, a: AccountId) -> &str {
    &rd.account(a).label
}

/// An asset by name or id string (asset ids are dense from 1).
pub fn asset(rd: &RefData, s: &str) -> Option<AssetId> {
    if let Some(n) = dec_id(s) {
        let id = AssetId(u16::try_from(n).ok()?);
        return (id.0 != 0 && rd.asset(id).id == id).then_some(id);
    }
    (1..=u16::MAX)
        .map(AssetId)
        .take_while(|&a| rd.asset(a).id == a)
        .find(|&a| &*rd.asset(a).name == s)
}

fn inst_exists(rd: &RefData, id: InstId) -> bool {
    id.0 != 0 && rd.inst(id).status != InstStatus::Invalid
}

/// An inst by `<venue>:<symbol>` or id string.
pub fn inst(rd: &RefData, s: &str) -> Option<InstId> {
    if let Some(n) = dec_id(s) {
        let id = InstId(u32::try_from(n).ok()?);
        return inst_exists(rd, id).then_some(id);
    }
    let (v, sym) = s.split_once(':')?;
    let v = venue(rd, v)?;
    rd.insts_of_venue(v)
        .iter()
        .copied()
        .find(|&i| &*rd.inst_meta(i).symbol == sym)
}

/// `<venue>:<symbol>` of an inst; unknown → its id as a decimal string.
pub fn inst_name(rd: &RefData, id: InstId) -> String {
    if !inst_exists(rd, id) {
        return id.0.to_string();
    }
    let v = rd.inst(id).venue;
    match venue_name(v) {
        Some(n) => format!("{n}:{}", rd.inst_meta(id).symbol),
        None => id.0.to_string(),
    }
}
