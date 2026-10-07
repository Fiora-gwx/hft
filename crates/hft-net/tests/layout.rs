//! NET-001–NET-008, NET-011: value-type layouts and codes.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_net::*;
use static_assertions::{assert_impl_all, assert_not_impl_any};
use std::mem::{align_of, offset_of, size_of};

#[test]
fn net_001_timestamps_layout() {
    assert_eq!(size_of::<Timestamps>(), 32, "NET-001.1");
    assert_eq!(align_of::<Timestamps>(), 8, "NET-001.2");
    assert_eq!(
        (
            offset_of!(Timestamps, hw_rx_ns),
            offset_of!(Timestamps, sw_rx_ns),
            offset_of!(Timestamps, rx_tsc)
        ),
        (0, 8, 16),
        "NET-001.3"
    );
    assert_eq!(
        (
            offset_of!(Timestamps, flags),
            offset_of!(Timestamps, rx_empty_gap_ns)
        ),
        (24, 28),
        "NET-001.4"
    );
    let d = Timestamps::default();
    assert_eq!(
        (d.hw_rx_ns, d.sw_rx_ns, d.rx_tsc, d.flags, d.rx_empty_gap_ns),
        (0, 0, 0, 0, 0),
        "NET-001.5"
    );
}

#[test]
fn net_002_ts_flags_values() {
    assert_eq!(TsFlags::HW, 1, "NET-002.1");
    assert_eq!(TsFlags::SW, 2, "NET-002.2");
    assert_eq!(TsFlags::MULTI_SKB, 4, "NET-002.3");
    assert_eq!(TsFlags::HW & TsFlags::SW, 0, "NET-002.4");
    assert_eq!(TsFlags::HW & TsFlags::MULTI_SKB, 0, "NET-002.4");
    assert_eq!(TsFlags::SW & TsFlags::MULTI_SKB, 0, "NET-002.4");
}

#[test]
fn net_003_best_rx() {
    let rows = [
        ("NET-003.1", TsFlags::HW | TsFlags::SW, (1_000, false)),
        ("NET-003.2", TsFlags::HW, (1_000, false)),
        ("NET-003.3", TsFlags::SW, (900, true)),
        ("NET-003.4", TsFlags::SW | TsFlags::MULTI_SKB, (900, true)),
        ("NET-003.5", 0, (0, false)),
    ];
    for (row, flags, want) in rows {
        let t = Timestamps {
            hw_rx_ns: 1_000,
            sw_rx_ns: 900,
            rx_tsc: 0,
            flags,
            rx_empty_gap_ns: 0,
        };
        assert_eq!(t.best_rx(), want, "{row}");
    }
}

#[test]
fn net_004_conn_phase_codes() {
    let rows = [
        ("NET-004.1", ConnPhase::Free, 0u8),
        ("NET-004.2", ConnPhase::TcpConnecting, 1),
        ("NET-004.3", ConnPhase::TlsHandshake, 2),
        ("NET-004.4", ConnPhase::WsUpgrade, 3),
        ("NET-004.5", ConnPhase::Open, 4),
        ("NET-004.6", ConnPhase::Closing, 5),
        ("NET-004.7", ConnPhase::Dead, 6),
    ];
    for (row, p, v) in rows {
        assert_eq!(p as u8, v, "{row}");
    }
}

#[test]
fn net_005_opcode_codes() {
    assert_eq!(Opcode::Text as u8, 1, "NET-005.1");
    assert_eq!(Opcode::Binary as u8, 2, "NET-005.2");
    // NET-005.3: an exhaustive match over exactly two variants.
    let n = |o: Opcode| match o {
        Opcode::Text | Opcode::Binary => 1,
    };
    assert_eq!(n(Opcode::Text) + n(Opcode::Binary), 2, "NET-005.3");
}

#[test]
fn net_006_err_bits_values() {
    use ErrBits as E;
    let fatal = [
        ("NET-006.1", [E::PEER_EOF, E::RESET, E::IO, E::TLS_ALERT], 0u32),
        (
            "NET-006.2",
            [E::TLS_PROTO, E::WS_PROTO, E::WS_CLOSE, E::RX_FULL],
            4,
        ),
        (
            "NET-006.3",
            [E::TX_BACKLOG, E::PING_TIMEOUT, E::IDLE_TIMEOUT, E::HTTP_PROTO],
            8,
        ),
    ];
    for (row, bits, base) in fatal {
        for (k, b) in bits.iter().enumerate() {
            assert_eq!(b.bits(), 1 << (base + k as u32), "{row}");
            assert_eq!(*b & E::FATAL, *b, "NET-006.7");
        }
    }
    let soft = [
        (
            "NET-006.4",
            vec![E::TX_PARTIAL, E::TS_MISSING, E::WRONG_QUEUE],
            16u32,
        ),
        (
            "NET-006.5",
            vec![E::KEY_UPDATE, E::ROTATE_DUE, E::SERVER_HINT],
            19,
        ),
    ];
    for (row, bits, base) in soft {
        for (k, b) in bits.iter().enumerate() {
            assert_eq!(b.bits(), 1 << (base + k as u32), "{row}");
            assert!((*b & E::FATAL).is_empty(), "NET-006.8");
        }
    }
    assert_eq!(E::FATAL.bits(), 0x0000_FFFF, "NET-006.6");
}

#[test]
fn net_007_txts_layouts() {
    assert_eq!(size_of::<ConnKey>(), 4, "NET-007.1");
    assert_eq!(
        (
            offset_of!(ConnKey, venue),
            offset_of!(ConnKey, conn),
            offset_of!(ConnKey, _pad)
        ),
        (0, 1, 2),
        "NET-007.1"
    );
    assert_eq!(size_of::<TxStamp>(), 24, "NET-007.2");
    assert_eq!(
        (
            offset_of!(TxStamp, key),
            offset_of!(TxStamp, tskey),
            offset_of!(TxStamp, kind),
            offset_of!(TxStamp, ts_ns)
        ),
        (0, 4, 8, 16),
        "NET-007.2"
    );
    assert_eq!(size_of::<TxTag>(), 24, "NET-007.3");
    assert_eq!(
        (
            offset_of!(TxTag, key),
            offset_of!(TxTag, tskey_end),
            offset_of!(TxTag, slot),
            offset_of!(TxTag, r#gen),
            offset_of!(TxTag, sent_tsc)
        ),
        (0, 4, 8, 12, 16),
        "NET-007.3"
    );
}

#[test]
fn net_008_masked_frame_layout() {
    assert_eq!(align_of::<MaskedFrame<2048>>(), 64, "NET-008.1");
    assert_eq!(
        (size_of::<MaskedFrame<2048>>(), size_of::<MaskedFrame<8192>>()),
        (2112, 8256),
        "NET-008.2"
    );
    assert_eq!(
        hft_net::layout::masked_frame_offsets(),
        [0, 2, 4, 5, 6, 8],
        "NET-008.3"
    );
    let mut f = MaskedFrame::<2048>::new();
    assert_eq!(MaskedFrame::<2048>::HDR_MAX, 14, "NET-008.4");
    assert_eq!(f.body_mut().len(), 2034, "NET-008.4");
    assert_eq!(MaskedFrame::<2048>::new().sealed(), 0, "NET-008.5");
}

#[test]
fn net_011_conn_not_sync() {
    assert_not_impl_any!(Conn: Sync);
    assert_impl_all!(Handoff: Send);
    assert_impl_all!(RetireMsg: Send);
    assert_impl_all!(ReadyHandle: Send);
}
