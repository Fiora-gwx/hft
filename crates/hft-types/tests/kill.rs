//! Kill-word bit layout (TYP-100 – TYP-105).

use hft_types::KillBits;

#[test]
fn typ_100_kill_constants() {
    assert_eq!(KillBits::GLOBAL.0, 0x0000_0001, "TYP-100.1");
    assert_eq!(KillBits::VENUE.0, 0x0000_0002, "TYP-100.2");
    assert_eq!(KillBits::HARD.0, 0x0000_0004, "TYP-100.3");
    assert_eq!(KillBits::ACCOUNT0.0, 0x0000_0100, "TYP-100.4");
    assert_eq!(KillBits::ACCOUNT_HARD0.0, 0x0010_0000, "TYP-100.5");
    assert_eq!(KillBits::ACCOUNT_MASK.0, 0x000F_FF00, "TYP-100.6");
    assert_eq!(KillBits::ACCOUNT_HARD_MASK.0, 0xFFF0_0000, "TYP-100.7");
    assert_eq!(KillBits::MAX_ACCOUNTS_PER_GW, 12, "TYP-100.8");
    assert_eq!(
        (size_of::<KillBits>(), align_of::<KillBits>()),
        (4, 4),
        "TYP-100.9"
    );
}

#[test]
fn typ_101_account_bits() {
    let rows: [(&str, KillBits, u32); 11] = [
        ("TYP-101.1", KillBits::account(0), 0x0000_0100),
        ("TYP-101.2", KillBits::account(1), 0x0000_0200),
        ("TYP-101.3", KillBits::account(11), 0x0008_0000),
        ("TYP-101.4", KillBits::account_hard(0), 0x0010_0000),
        ("TYP-101.5", KillBits::account_hard(1), 0x0020_0000),
        ("TYP-101.6", KillBits::account_hard(11), 0x8000_0000),
        ("TYP-101.7", KillBits::gw_mask(0), 0x0010_0107),
        ("TYP-101.8", KillBits::gw_mask(5), 0x0200_2007),
        ("TYP-101.9", KillBits::gw_mask(11), 0x8008_0007),
        ("TYP-101.10", KillBits::hard_mask(0), 0x0010_0004),
        ("TYP-101.11", KillBits::hard_mask(11), 0x8000_0004),
    ];
    for (row, got, want) in rows {
        assert_eq!(got, KillBits(want), "{row}");
    }
}

#[test]
fn typ_102_account_bits_partition() {
    let mut vals = Vec::new();
    let (mut acc_or, mut hard_or) = (0u32, 0u32);
    for ix in 0..12u8 {
        let a = KillBits::account(ix).0;
        let h = KillBits::account_hard(ix).0;
        assert_eq!(a.count_ones(), 1, "account({ix})");
        assert_eq!(h.count_ones(), 1, "account_hard({ix})");
        assert_eq!(a & KillBits::ACCOUNT_MASK.0, a, "account({ix}) in mask");
        assert_eq!(h & KillBits::ACCOUNT_HARD_MASK.0, h, "account_hard({ix}) in mask");
        acc_or |= a;
        hard_or |= h;
        vals.push(a);
        vals.push(h);
    }
    let reserved = KillBits::GLOBAL.0 | KillBits::VENUE.0 | KillBits::HARD.0 | 0xF8;
    for (i, a) in vals.iter().enumerate() {
        assert_eq!(a & reserved, 0, "value {i} touches bits 0..7");
        for b in &vals[i + 1..] {
            assert_eq!(a & b, 0, "overlap {a:#x} {b:#x}");
        }
    }
    assert_eq!(acc_or, 0x000F_FF00);
    assert_eq!(hard_or, 0xFFF0_0000);
}

#[test]
fn typ_103_cross_account_isolation() {
    for ix in 0..12u8 {
        for jx in (0..12u8).filter(|j| *j != ix) {
            let gw = KillBits::gw_mask(ix).0;
            assert_eq!(KillBits::account(jx).0 & gw, 0, "account({jx}) & gw_mask({ix})");
            assert_eq!(
                KillBits::account_hard(jx).0 & gw,
                0,
                "account_hard({jx}) & gw_mask({ix})"
            );
            assert_eq!(
                KillBits::account_hard(jx).0 & KillBits::hard_mask(ix).0,
                0,
                "account_hard({jx}) & hard_mask({ix})"
            );
        }
    }
    let w = KillBits::account(3).0 | KillBits::account_hard(7).0;
    assert_eq!(w & KillBits::gw_mask(4).0, 0);
    assert_eq!(w & KillBits::gw_mask(3).0, KillBits::account(3).0);
    assert_eq!(KillBits::account(3).0, 0x0000_0800);
    assert_eq!(w & KillBits::hard_mask(3).0, 0);
    assert_eq!(w & KillBits::hard_mask(7).0, KillBits::account_hard(7).0);
    assert_eq!(KillBits::account_hard(7).0, 0x0800_0000);
}

#[test]
#[cfg_attr(debug_assertions, should_panic)]
fn typ_104_account_index_12() {
    let a = KillBits::account(12);
    let h = KillBits::account_hard(12);
    let g = KillBits::gw_mask(12);
    assert_eq!((a, h, g), (KillBits(0), KillBits(0), KillBits(0x0000_0007)));
}

#[test]
fn typ_105_const_masks() {
    const M: KillBits = KillBits::gw_mask(3);
    const H: KillBits = KillBits::hard_mask(3);
    assert_eq!(M, KillBits::gw_mask(3));
    assert_eq!(M, KillBits(0x0080_0807));
    assert_eq!(H, KillBits::hard_mask(3));
    assert_eq!(H, KillBits(0x0080_0004));
}
