//! The ledger's Note codes (`NOTE_CODES`: the numeric `n` of every `Ledger`-source name in the
//! hft-event registry) and the Note codes of other sources the journal cold consumer reads.

use hft_event::{Note, NoteLevel, NoteSource, note_code};
use hft_types::{GroupId, InstId, StrategyId};

/// `(source, n, name)` of every `Ledger`-source Note (hft-event "引擎 Note 码登记表").
pub static NOTE_CODES: &[(NoteSource, u32, &str)] = &[
    (NoteSource::Ledger, 1, "RoundClosed"),
    (NoteSource::Ledger, 2, "Adjusted"),
    (NoteSource::Ledger, 3, "BaselineSet"),
    (NoteSource::Ledger, 4, "BalanceInit"),
    (NoteSource::Ledger, 5, "SimReset"),
    (NoteSource::Ledger, 6, "ResidualWrittenOff"),
    (NoteSource::Ledger, 7, "ExternalFill"),
    (NoteSource::Ledger, 8, "LedgerAutoHeldOffset"),
    (NoteSource::Ledger, 9, "BalanceDrift"),
    (NoteSource::Ledger, 10, "InvShortfall"),
    (NoteSource::Ledger, 11, "NetQtyMismatch"),
    (NoteSource::Ledger, 12, "FundingMismatch"),
    (NoteSource::Ledger, 13, "FundingCorrected"),
    (NoteSource::Ledger, 14, "ChainReorg"),
    (NoteSource::Ledger, 15, "ChainReorgNaked"),
    (NoteSource::Ledger, 16, "BalanceCheck"),
];

/// `Note.code` values of the ledger's Notes.
pub mod code {
    use super::*;
    pub const ROUND_CLOSED: u32 = note_code(NoteSource::Ledger, 1);
    pub const ADJUSTED: u32 = note_code(NoteSource::Ledger, 2);
    pub const BASELINE_SET: u32 = note_code(NoteSource::Ledger, 3);
    pub const BALANCE_INIT: u32 = note_code(NoteSource::Ledger, 4);
    pub const SIM_RESET: u32 = note_code(NoteSource::Ledger, 5);
    pub const RESIDUAL_WRITTEN_OFF: u32 = note_code(NoteSource::Ledger, 6);
    pub const EXTERNAL_FILL: u32 = note_code(NoteSource::Ledger, 7);
    pub const AUTO_HELD_OFFSET: u32 = note_code(NoteSource::Ledger, 8);
    pub const BALANCE_DRIFT: u32 = note_code(NoteSource::Ledger, 9);
    pub const INV_SHORTFALL: u32 = note_code(NoteSource::Ledger, 10);
    pub const NET_QTY_MISMATCH: u32 = note_code(NoteSource::Ledger, 11);
    pub const FUNDING_MISMATCH: u32 = note_code(NoteSource::Ledger, 12);
    pub const FUNDING_CORRECTED: u32 = note_code(NoteSource::Ledger, 13);
    pub const BALANCE_CHECK: u32 = note_code(NoteSource::Ledger, 16);

    /// hft-sequencer `EpochStart` (registry order within the `Sequencer` source; the
    /// authoritative `n` belongs to hft-sequencer's `NOTE_CODES`).
    pub const EPOCH_START: u32 = note_code(NoteSource::Sequencer, 1);
    /// hft-oms `OmsStaleGenFill` (registry order within the `Oms` source; owned by hft-oms).
    pub const OMS_STALE_GEN_FILL: u32 = note_code(NoteSource::Oms, 5);
    /// hft-oms `OmsForeignFill` (registry order within the `Oms` source; owned by hft-oms).
    pub const OMS_FOREIGN_FILL: u32 = note_code(NoteSource::Oms, 11);
}

/// A `Ledger`-source Note.
pub(crate) fn note(code: u32, level: NoteLevel, group: GroupId, inst: InstId, v: [u64; 8]) -> Note {
    Note {
        code,
        level,
        source: NoteSource::Ledger,
        strategy: StrategyId(0),
        _pad: 0,
        group,
        inst,
        v,
    }
}
