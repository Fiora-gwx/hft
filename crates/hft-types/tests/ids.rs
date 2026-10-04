//! ID newtypes, named constants, `GroupId` and the `VenueId` table (TYP-080 – TYP-083, TYP-085).

use std::any::TypeId;

use hft_types::*;

#[test]
fn typ_080_id_widths() {
    assert_eq!(size_of::<InstId>(), 4, "TYP-080.1");
    assert_eq!(size_of::<AssetId>(), 2, "TYP-080.2");
    assert_eq!(size_of::<VenueId>(), 1, "TYP-080.3");
    assert_eq!(size_of::<AccountId>(), 1, "TYP-080.4");
    assert_eq!(size_of::<StrategyId>(), 1, "TYP-080.5");
    assert_eq!(size_of::<GroupId>(), 4, "TYP-080.6");
    assert_eq!(size_of::<SlotId>(), 4, "TYP-080.7");
    assert_eq!(size_of::<ConnId>(), 1, "TYP-080.8");
    assert_eq!(size_of::<Seq>(), 8, "TYP-080.9");
    assert_eq!(size_of::<Tsc>(), 8, "TYP-080.10");
    assert_eq!(
        TypeId::of::<StrategyInstanceId>(),
        TypeId::of::<StrategyId>(),
        "TYP-080.11"
    );
}

#[test]
fn typ_081_named_constants() {
    assert_eq!(INST_NONE, InstId(0), "TYP-081.1");
    assert_eq!(INST_ALL, InstId(0), "TYP-081.2");
    assert_eq!(ASSET_NONE, AssetId(0), "TYP-081.3");
    assert_eq!(GROUP_NONE, GroupId(0), "TYP-081.4");
    assert_eq!(GROUP_ALL, GroupId(4_294_967_295), "TYP-081.5");
    assert_eq!(ACCOUNT_ALL, AccountId(0), "TYP-081.6");
    assert_eq!(SlotId::UNRESOLVED, SlotId(0), "TYP-081.7");
    assert_eq!(MAX_BBO_CONN_SLOTS, 8u8, "TYP-081.8");
    assert_eq!(GroupId::N_MAX, 16_777_214, "TYP-081.9");
}

#[test]
fn typ_082_group_id_compose() {
    let rows: [(&str, u8, u32, Option<u32>); 12] = [
        ("TYP-082.1", 1, 1, Some(0x0100_0001)),
        ("TYP-082.2", 254, 0xFF_FFFE, Some(0xFEFF_FFFE)),
        ("TYP-082.3", 1, 0xFF_FFFE, Some(0x01FF_FFFE)),
        ("TYP-082.4", 254, 1, Some(0xFE00_0001)),
        ("TYP-082.5", 0, 1, None),
        ("TYP-082.6", 255, 1, None),
        ("TYP-082.7", 1, 0, None),
        ("TYP-082.8", 1, 0xFF_FFFF, None),
        ("TYP-082.9", 2, 0x100_0000, None),
        ("TYP-082.10", 1, u32::MAX, None),
        ("TYP-082.11", 255, 0xFF_FFFF, None),
        ("TYP-082.12", 0, 0, None),
    ];
    for (row, s, n, want) in rows {
        assert_eq!(GroupId::compose(StrategyId(s), n), want.map(GroupId), "{row}");
    }
}

#[test]
fn typ_083_group_id_parts() {
    let rows: [(&str, GroupId, (u8, u32)); 4] = [
        ("TYP-083.1", GroupId(0x0300_0007), (3, 7)),
        ("TYP-083.2", GroupId(0xFEFF_FFFE), (254, 0xFF_FFFE)),
        ("TYP-083.3", GROUP_NONE, (0, 0)),
        ("TYP-083.4", GROUP_ALL, (255, 0xFF_FFFF)),
    ];
    for (row, g, (s, n)) in rows {
        assert_eq!((g.strategy(), g.counter()), (StrategyId(s), n), "{row}");
    }
}

#[test]
fn typ_085_venue_table() {
    use MarketKind::{LinearPerp, Spot};
    let rows: [(&str, u8, Option<MarketKind>, Option<&str>); 21] = [
        ("TYP-085.1", 0, None, None),
        ("TYP-085.2", 1, Some(LinearPerp), Some("BINANCE_USDM")),
        ("TYP-085.3", 2, Some(Spot), Some("BINANCE_SPOT")),
        ("TYP-085.4", 3, Some(LinearPerp), Some("GATE_USDT_FUT")),
        ("TYP-085.5", 4, Some(Spot), Some("GATE_SPOT")),
        ("TYP-085.6", 5, Some(LinearPerp), Some("OKX_SWAP")),
        ("TYP-085.7", 6, Some(Spot), Some("OKX_SPOT")),
        ("TYP-085.8", 7, None, None),
        ("TYP-085.9", 8, Some(LinearPerp), Some("BYBIT_LINEAR")),
        ("TYP-085.10", 9, Some(Spot), Some("BYBIT_SPOT")),
        ("TYP-085.11", 10, Some(LinearPerp), Some("BITGET_UTA_USDT_FUT")),
        ("TYP-085.12", 11, Some(Spot), Some("BITGET_UTA_SPOT")),
        ("TYP-085.13", 12, Some(LinearPerp), Some("KUCOIN_FUT")),
        ("TYP-085.14", 13, Some(Spot), Some("KUCOIN_SPOT")),
        ("TYP-085.15", 14, Some(LinearPerp), Some("HYPERLIQUID_PERP")),
        ("TYP-085.16", 15, None, None),
        ("TYP-085.17", 31, None, None),
        ("TYP-085.18", 32, None, Some("CHAIN_BSC")),
        ("TYP-085.19", 33, None, Some("CHAIN_SOLANA")),
        ("TYP-085.20", 34, None, None),
        ("TYP-085.21", 255, None, None),
    ];
    for (row, v, market, name) in rows {
        assert_eq!(VenueId(v).market(), market, "{row} market");
        assert_eq!(VenueId(v).name(), name, "{row} name");
    }
    let consts = [
        (VenueId::BINANCE_USDM, "BINANCE_USDM"),
        (VenueId::BINANCE_SPOT, "BINANCE_SPOT"),
        (VenueId::GATE_USDT_FUT, "GATE_USDT_FUT"),
        (VenueId::GATE_SPOT, "GATE_SPOT"),
        (VenueId::OKX_SWAP, "OKX_SWAP"),
        (VenueId::OKX_SPOT, "OKX_SPOT"),
        (VenueId::BYBIT_LINEAR, "BYBIT_LINEAR"),
        (VenueId::BYBIT_SPOT, "BYBIT_SPOT"),
        (VenueId::BITGET_UTA_USDT_FUT, "BITGET_UTA_USDT_FUT"),
        (VenueId::BITGET_UTA_SPOT, "BITGET_UTA_SPOT"),
        (VenueId::KUCOIN_FUT, "KUCOIN_FUT"),
        (VenueId::KUCOIN_SPOT, "KUCOIN_SPOT"),
        (VenueId::HYPERLIQUID_PERP, "HYPERLIQUID_PERP"),
        (VenueId::CHAIN_BSC, "CHAIN_BSC"),
        (VenueId::CHAIN_SOLANA, "CHAIN_SOLANA"),
    ];
    for (c, name) in consts {
        let (row, v, _, _) = rows.iter().find(|r| r.3 == Some(name)).unwrap();
        assert_eq!(c, VenueId(*v), "{row} constant {name}");
    }
}
