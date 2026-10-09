//! Result codes, detail codes and their JSON names (the code tables themselves are defined by
//! control-plane "结果码"; this module renders them).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Transport-level status of a response.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportStatus {
    Ok,
    Accepted,
    InvalidRequest,
    Unauthorized,
    Busy,
    InternalError,
}

impl TransportStatus {
    /// The result code a non-`ok` status implies (`accepted` → none).
    pub fn implied_code(self) -> Option<ResultCode> {
        match self {
            TransportStatus::Ok | TransportStatus::Accepted => None,
            TransportStatus::InvalidRequest => Some(ResultCode::InvalidArgs),
            TransportStatus::Unauthorized => Some(ResultCode::Unauthorized),
            TransportStatus::Busy => Some(ResultCode::Busy),
            TransportStatus::InternalError => Some(ResultCode::Internal),
        }
    }
}

macro_rules! result_codes {
    ($($name:ident = $v:literal),* $(,)?) => {
        /// control-plane result code (`CmdResult.status` and gateway codes).
        #[repr(u8)]
        #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
        pub enum ResultCode { $($name = $v),* }

        impl ResultCode {
            /// Every code in value order.
            pub const ALL: &'static [ResultCode] = &[$(ResultCode::$name),*];

            /// JSON name.
            pub fn name(self) -> &'static str {
                match self { $(ResultCode::$name => stringify!($name)),* }
            }

            /// Code of a value; unknown → `None`.
            pub fn from_u8(v: u8) -> Option<ResultCode> {
                match v { $($v => Some(ResultCode::$name),)* _ => None }
            }

            /// Code of a JSON name; unknown → `None`.
            pub fn from_name(s: &str) -> Option<ResultCode> {
                match s { $(stringify!($name) => Some(ResultCode::$name),)* _ => None }
            }
        }
    };
}

result_codes! {
    Applied = 0,
    NoOp = 1,
    Duplicate = 2,
    Expired = 3,
    Unauthorized = 10,
    Forbidden = 11,
    NeedsConfirm = 12,
    ConfirmInvalid = 13,
    InvalidArgs = 20,
    UnknownTarget = 21,
    VersionConflict = 22,
    StaleVersion = 23,
    CapacityExceeded = 24,
    PreconditionFailed = 25,
    RoleInsufficient = 26,
    Rejected = 27,
    RateLimited = 30,
    Busy = 31,
    Internal = 40,
}

impl Serialize for ResultCode {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for ResultCode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<ResultCode, D::Error> {
        let s = String::deserialize(d)?;
        ResultCode::from_name(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown result code {s}")))
    }
}

/// Detail codes (`CmdResult.detail = code | arg << 16`); names = control-plane detail table.
pub mod detail {
    pub const SHADOW_DEFERRED: u16 = 1;
    pub const UNKNOWN_DISCRIMINANT: u16 = 100;
    pub const ACCOUNT_REQUIRED: u16 = 101;
    pub const ACCOUNT_NOT_ALLOWED: u16 = 102;
    pub const ACCOUNT_MISMATCH: u16 = 103;
    pub const OUT_OF_SCOPE: u16 = 104;
    pub const ACCOUNT_BOUND: u16 = 105;
    pub const KIND_MISMATCH: u16 = 106;
    pub const SCOPE_MISMATCH: u16 = 107;
    pub const SCHEMA_UNSUPPORTED: u16 = 108;
    pub const SCHEMA_MISMATCH: u16 = 109;
    pub const PARAM_INVALID: u16 = 110;
    pub const GROUP_ID_DUP: u16 = 111;
    pub const BAD_GROUP: u16 = 112;
    pub const SAME_EXCHANGE_LEGS: u16 = 113;
    pub const LOT_MISMATCH: u16 = 114;
    pub const UNKNOWN_GROUP: u16 = 115;
    pub const TOO_LONG: u16 = 116;
    pub const NOT_SIM: u16 = 117;
    pub const NOT_LIVE: u16 = 118;
    pub const UNKNOWN_NAME: u16 = 119;
    pub const INST_OWNED: u16 = 120;
    pub const EXEC_RISK_MISMATCH: u16 = 121;
    pub const BAD_COMBINATION: u16 = 122;
    pub const NOT_RESUMABLE: u16 = 123;
    pub const GROUP_CHECK: u16 = 124;
    pub const BAD_VALUE: u16 = 125;
    pub const DUST_BELOW_MIN_NOTIONAL: u16 = 126;
    pub const STATE_CHANGED: u16 = 200;
    pub const NO_VALID_PREVIEW: u16 = 201;
    pub const GATE_NOT_RELEASED: u16 = 300;
    pub const DRAINING: u16 = 301;
    pub const RECON_NOT_CLEAN: u16 = 302;
    pub const RESUME_LIMIT: u16 = 303;
    pub const SIM_NOT_IDLE: u16 = 304;
    pub const INSUFFICIENT_BALANCE: u16 = 305;
    pub const UNKNOWN_PRESENT: u16 = 306;
    pub const NAKED_NOT_CLEARED: u16 = 307;
    pub const GROUP_HAS_POSITION: u16 = 308;
    pub const NO_MATCHING_CAP: u16 = 309;
    pub const STALE_REPORT: u16 = 310;
    pub const NO_BALANCE_VIEW: u16 = 311;
    pub const CONFIG_SOURCE_NOT_JOURNAL: u16 = 312;
    pub const SMOKE_ACTIVE: u16 = 313;
    pub const SMOKE_NO_PRICE: u16 = 314;
    pub const NOT_QUIESCENT: u16 = 316;
    pub const HARD: u16 = 317;
    pub const NOTHING_TO_FIX: u16 = 318;
    pub const QUOTE_UNAVAILABLE: u16 = 319;
    pub const CHAIN_LEG: u16 = 320;
    pub const BROKEN_REMAINING: u16 = 400;
    pub const REFDATA_RECHECK: u16 = 401;
    pub const REFDATA_ASSEMBLY: u16 = 402;
    pub const SMOKE_RISK_REJECT: u16 = 403;

    /// `(code, name)` of every assigned detail code (315 retired).
    pub const NAMES: &[(u16, &str)] = &[
        (SHADOW_DEFERRED, "ShadowDeferred"),
        (UNKNOWN_DISCRIMINANT, "UnknownDiscriminant"),
        (ACCOUNT_REQUIRED, "AccountRequired"),
        (ACCOUNT_NOT_ALLOWED, "AccountNotAllowed"),
        (ACCOUNT_MISMATCH, "AccountMismatch"),
        (OUT_OF_SCOPE, "OutOfScope"),
        (ACCOUNT_BOUND, "AccountBound"),
        (KIND_MISMATCH, "KindMismatch"),
        (SCOPE_MISMATCH, "ScopeMismatch"),
        (SCHEMA_UNSUPPORTED, "SchemaUnsupported"),
        (SCHEMA_MISMATCH, "SchemaMismatch"),
        (PARAM_INVALID, "ParamInvalid"),
        (GROUP_ID_DUP, "GroupIdDup"),
        (BAD_GROUP, "BadGroup"),
        (SAME_EXCHANGE_LEGS, "SameExchangeLegs"),
        (LOT_MISMATCH, "LotMismatch"),
        (UNKNOWN_GROUP, "UnknownGroup"),
        (TOO_LONG, "TooLong"),
        (NOT_SIM, "NotSim"),
        (NOT_LIVE, "NotLive"),
        (UNKNOWN_NAME, "UnknownName"),
        (INST_OWNED, "InstOwned"),
        (EXEC_RISK_MISMATCH, "ExecRiskMismatch"),
        (BAD_COMBINATION, "BadCombination"),
        (NOT_RESUMABLE, "NotResumable"),
        (GROUP_CHECK, "GroupCheck"),
        (BAD_VALUE, "BadValue"),
        (DUST_BELOW_MIN_NOTIONAL, "DustBelowMinNotional"),
        (STATE_CHANGED, "StateChanged"),
        (NO_VALID_PREVIEW, "NoValidPreview"),
        (GATE_NOT_RELEASED, "GateNotReleased"),
        (DRAINING, "Draining"),
        (RECON_NOT_CLEAN, "ReconNotClean"),
        (RESUME_LIMIT, "ResumeLimit"),
        (SIM_NOT_IDLE, "SimNotIdle"),
        (INSUFFICIENT_BALANCE, "InsufficientBalance"),
        (UNKNOWN_PRESENT, "UnknownPresent"),
        (NAKED_NOT_CLEARED, "NakedNotCleared"),
        (GROUP_HAS_POSITION, "GroupHasPosition"),
        (NO_MATCHING_CAP, "NoMatchingCap"),
        (STALE_REPORT, "StaleReport"),
        (NO_BALANCE_VIEW, "NoBalanceView"),
        (CONFIG_SOURCE_NOT_JOURNAL, "ConfigSourceNotJournal"),
        (SMOKE_ACTIVE, "SmokeActive"),
        (SMOKE_NO_PRICE, "SmokeNoPrice"),
        (NOT_QUIESCENT, "NotQuiescent"),
        (HARD, "Hard"),
        (NOTHING_TO_FIX, "NothingToFix"),
        (QUOTE_UNAVAILABLE, "QuoteUnavailable"),
        (CHAIN_LEG, "ChainLeg"),
        (BROKEN_REMAINING, "BrokenRemaining"),
        (REFDATA_RECHECK, "RefdataRecheck"),
        (REFDATA_ASSEMBLY, "RefdataAssembly"),
        (SMOKE_RISK_REJECT, "SmokeRiskReject"),
    ];

    /// `ParamInvalid` arg kinds (`arg = kind | field << 4`).
    pub const PARAM_KIND_STRATEGY: u16 = 1;
    pub const PARAM_KIND_MODEL_MEM_BUDGET: u16 = 2;
    pub const PARAM_KIND_NON_PROD_ONLY: u16 = 3;
    /// `ParamInvalid` field "not specified".
    pub const PARAM_FIELD_NONE: u16 = 0xFFF;

    /// `ParamInvalid` arg.
    pub const fn param_arg(kind: u16, field: u16) -> u16 {
        (kind & 0xF) | ((field & 0xFFF) << 4)
    }

    /// `RefdataAssembly` arg names (1-based).
    pub const ASSEMBLY_ITEMS: &[&str] = &[
        "AccountNamePrefix",
        "MissingCredential",
        "SimConfig",
        "ThreadPlan",
        "DustBelowMinNotional",
    ];

    /// Name of a detail code; unknown → `None`.
    pub fn name(code: u16) -> Option<&'static str> {
        NAMES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
    }

    /// Code of a detail name; unknown → `None`.
    pub fn code(name: &str) -> Option<u16> {
        NAMES.iter().find(|(_, n)| *n == name).map(|(c, _)| *c)
    }
}

/// `RejectBits` bit names by bit number (hft-types constants; 21 retired).
pub const REJECT_BIT_NAMES: &[(u8, &str)] = &[
    (0, "INVALID"),
    (1, "NOT_TRADABLE"),
    (2, "VENUE_NOT_READY"),
    (3, "KILLED"),
    (4, "CLOSE_ONLY"),
    (5, "HARD"),
    (6, "UNKNOWN_PENDING"),
    (7, "NO_MID"),
    (8, "STALE_MID"),
    (9, "PX_BAND"),
    (10, "NOTIONAL_ORDER"),
    (11, "NOTIONAL_GROUP"),
    (12, "NOTIONAL_ACCOUNT"),
    (13, "NOTIONAL_VENUE"),
    (14, "POSITION"),
    (15, "REDUCE_INVALID"),
    (16, "RATE"),
    (17, "RATE_BACKOFF"),
    (18, "DAILY_LOSS"),
    (19, "EXPOSURE_GATE"),
    (20, "RECON_DEVIATION"),
    (22, "CLOCK"),
    (23, "DRAINING"),
    (24, "MODE_SHADOW"),
    (25, "LOCAL_SEND_FAIL"),
    (26, "HEDGE_LIMIT"),
    (27, "NOT_BOUND"),
    (28, "LINK_STALE"),
    (29, "LINK_FUNDS"),
];

/// A rendered detail: name and optional arg.
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct Detail {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg: Option<String>,
}

impl Detail {
    /// A detail without arg.
    pub fn code(code: u16) -> Detail {
        render_detail(u32::from(code)).unwrap_or(Detail {
            name: code.to_string(),
            arg: None,
        })
    }
}

fn load_err_name(v: u16) -> Option<&'static str> {
    hft_refdata::schema::load_err_codes()
        .find(|(_, c)| *c == v)
        .map(|(n, _)| n)
}

fn arg_name(code: u16, arg: u16) -> Option<String> {
    match code {
        detail::PARAM_INVALID => {
            let kind = arg & 0xF;
            let field = arg >> 4;
            Some(match kind {
                detail::PARAM_KIND_STRATEGY if field == detail::PARAM_FIELD_NONE => "Strategy".to_owned(),
                detail::PARAM_KIND_STRATEGY => format!("Strategy:{field}"),
                detail::PARAM_KIND_MODEL_MEM_BUDGET => "ModelMemBudget".to_owned(),
                detail::PARAM_KIND_NON_PROD_ONLY => "NonProdOnly".to_owned(),
                _ => return None,
            })
        }
        detail::REFDATA_ASSEMBLY => detail::ASSEMBLY_ITEMS
            .get(usize::from(arg).checked_sub(1)?)
            .map(|s| (*s).to_owned()),
        detail::GROUP_CHECK | detail::REFDATA_RECHECK => load_err_name(arg).map(str::to_owned),
        detail::SMOKE_RISK_REJECT => REJECT_BIT_NAMES
            .iter()
            .find(|(b, _)| u16::from(*b) == arg)
            .map(|(_, n)| (*n).to_owned()),
        _ => None,
    }
}

/// Renders `CmdResult.detail` (`code | arg << 16`); 0 → `None` (key omitted).
pub fn render_detail(raw: u32) -> Option<Detail> {
    if raw == 0 {
        return None;
    }
    let code = (raw & 0xFFFF) as u16;
    let arg = (raw >> 16) as u16;
    let name = detail::name(code).map_or_else(|| code.to_string(), str::to_owned);
    let arg = match arg_name(code, arg) {
        Some(n) => Some(n),
        None if arg == 0 => None,
        None => Some(arg.to_string()),
    };
    Some(Detail { name, arg })
}
