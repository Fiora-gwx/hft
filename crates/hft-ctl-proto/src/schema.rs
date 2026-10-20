//! Request schema: the typed view of a request frame (`CmdRequest`/`CmdBody`), its strict parser
//! (schema-layer errors → `invalid_request`) and its canonical compact JSON writer.
//!
//! Numbers inside commands are decimal strings: the parser only checks their syntax; ranges,
//! exponents and names are checked by `normalize`.

use hft_types::{CapSource, KillLevel, RunMode, Side};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::codes::detail;
use crate::{MAX_EXPIRY_NS, PROTO_VERSION};

/// Maximum issuer length (`CmdHdr.issuer`).
pub const ISSUER_MAX: usize = 24;

/// Envelope account selection.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum AccountSel {
    /// Key absent.
    Missing,
    /// `"*"`.
    All,
    /// An account label or a decimal `AccountId` string.
    Name(String),
}

/// Chat type of a Telegram issuer.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChatType {
    Private,
    Group,
}

impl ChatType {
    pub fn name(self) -> &'static str {
        match self {
            ChatType::Private => "private",
            ChatType::Group => "group",
        }
    }
}

/// `Annotate.kind`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AnnotateKind {
    OpsNote = 1,
    LocalWrite = 2,
    Incident = 3,
}

/// `AdoptExchange.item`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AdoptItem {
    Position = 0,
    Balance = 1,
}

/// `SimFault.fault`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SimFaultKind {
    Off = 1,
    RejectNew = 2,
    Disconnect = 3,
    DelayReports = 4,
}

/// `VenueNotice.kind` with its own fields (decimal strings).
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum NoticeKind {
    Delisting { at_ns: String },
    FundingInterval { new_s: String },
    Maintenance { from_ns: String, to_ns: String },
}

/// One `ReconcileReport` item (names and code names; numbers as decimal strings).
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct ReconItem {
    pub venue: Option<String>,
    pub account: Option<String>,
    /// `codes::RECON_ITEM` value.
    pub item: u8,
    /// `codes::RECON_SEVERITY` value; absent = 0.
    pub severity: Option<u8>,
    pub inst: Option<String>,
    pub asset: Option<String>,
    pub engine_qty: Option<String>,
    pub exch_qty: Option<String>,
    pub diff: Option<String>,
    pub exch_ts_ns: Option<String>,
    pub slot: Option<String>,
    /// `codes::RECON_ORDER_STATUS` value.
    pub order_status: Option<u8>,
    pub history_checked: Option<bool>,
    pub view_seq: Option<String>,
    pub r#gen: Option<String>,
    pub epoch: Option<String>,
    pub exch_amount: Option<String>,
    pub engine_amount: Option<String>,
    /// `codes::RECON_SFLAGS` bit names, in request order.
    pub sflags: Option<Vec<String>>,
    pub skipped_inflight: Option<String>,
}

/// Command body: one shape per `ControlCmd` variant, plus the read-only `GetSchema` and the
/// non-command `ReconcileReport`. Optional scope fields absent = `"*"`; `account` here is the
/// optional in-body account (must equal the envelope's when both are given).
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum CmdBody {
    SetMode {
        strategy: Option<String>,
        group: Option<String>,
        account: Option<String>,
        mode: RunMode,
    },
    SetParams {
        strategy: String,
        account: Option<String>,
        blob_b64: String,
        base_version: Option<String>,
    },
    SetRisk {
        account: Option<String>,
        blob_b64: String,
        base_version: Option<String>,
    },
    Kill {
        level: KillLevel,
        venue: Option<String>,
        account: Option<String>,
        cancel_resting: Option<bool>,
        flatten: Option<bool>,
        hard: Option<bool>,
    },
    Resume {
        source: CapSource,
        strategy: Option<String>,
        venue: Option<String>,
        group: Option<String>,
        inst: Option<String>,
        account: Option<String>,
    },
    CancelAll {
        venue: Option<String>,
        inst: Option<String>,
        account: Option<String>,
    },
    Drain {
        cancel_resting: Option<bool>,
        account: Option<String>,
    },
    HedgeFix {
        group: Option<String>,
        account: Option<String>,
    },
    WriteOffResidual {
        group: Option<String>,
        account: Option<String>,
    },
    SetBalanceBaseline {
        venue: String,
        account: Option<String>,
    },
    SimReset {
        account: Option<String>,
    },
    SimTransfer {
        from_venue: String,
        to_venue: String,
        asset: String,
        amount: String,
        account: Option<String>,
    },
    SimFault {
        venue: String,
        fault: SimFaultKind,
        delay_ms: Option<String>,
        account: Option<String>,
    },
    RecordTransfer {
        from: String,
        to: String,
        asset: String,
        amount: String,
        ext_ref: String,
        account: Option<String>,
    },
    Annotate {
        kind: AnnotateKind,
        text: String,
        digest: Option<String>,
        account: Option<String>,
    },
    AdoptExchange {
        venue: String,
        report_id: String,
        item: AdoptItem,
        inst: Option<String>,
        asset: Option<String>,
        group: Option<String>,
        entry_px: Option<String>,
        account: Option<String>,
    },
    VenueNotice {
        venue: String,
        inst: Option<String>,
        kind: NoticeKind,
        account: Option<String>,
    },
    SmokeOrder {
        venue: String,
        inst: String,
        side: Side,
        ttl_ms: Option<String>,
        account: Option<String>,
    },
    /// `wire_id`: the exchange id as a decimal uint256 (`"0"` = unbind).
    BindSlot {
        venue: String,
        inst: String,
        start_s: String,
        end_s: String,
        wire_id: String,
        account: Option<String>,
    },
    StrategyCmd {
        strategy: String,
        group: Option<String>,
        reduce_only: Option<bool>,
        payload_b64: String,
        account: Option<String>,
    },
    InjectWarmup {
        strategy: String,
        local_kind: String,
        chunk_no: String,
        last: Option<bool>,
        payload_b64: String,
        account: Option<String>,
    },
    ReplaceRefdata {
        blob_b64: String,
        version: String,
        account: Option<String>,
    },
    GetSchema {
        strategy_kind: Option<String>,
        kind_id: Option<String>,
    },
    ReconcileReport {
        report_id: String,
        items: Vec<ReconItem>,
        /// Optional original ExternalFill v3 bodies (96 B each), base64 encoded.
        receipts_b64: Option<String>,
    },
    RotateConn {
        venue: String,
        account: Option<String>,
    },
    Resubscribe {
        venue: String,
        inst: Option<String>,
        account: Option<String>,
    },
    SnapshotNow {
        account: Option<String>,
    },
}

impl CmdBody {
    /// The JSON `type`.
    pub fn type_name(&self) -> &'static str {
        match self {
            CmdBody::SetMode { .. } => "SetMode",
            CmdBody::SetParams { .. } => "SetParams",
            CmdBody::SetRisk { .. } => "SetRisk",
            CmdBody::Kill { .. } => "Kill",
            CmdBody::Resume { .. } => "Resume",
            CmdBody::CancelAll { .. } => "CancelAll",
            CmdBody::Drain { .. } => "Drain",
            CmdBody::HedgeFix { .. } => "HedgeFix",
            CmdBody::WriteOffResidual { .. } => "WriteOffResidual",
            CmdBody::SetBalanceBaseline { .. } => "SetBalanceBaseline",
            CmdBody::SimReset { .. } => "SimReset",
            CmdBody::SimTransfer { .. } => "SimTransfer",
            CmdBody::SimFault { .. } => "SimFault",
            CmdBody::RecordTransfer { .. } => "RecordTransfer",
            CmdBody::Annotate { .. } => "Annotate",
            CmdBody::AdoptExchange { .. } => "AdoptExchange",
            CmdBody::VenueNotice { .. } => "VenueNotice",
            CmdBody::SmokeOrder { .. } => "SmokeOrder",
            CmdBody::BindSlot { .. } => "BindSlot",
            CmdBody::StrategyCmd { .. } => "StrategyCmd",
            CmdBody::InjectWarmup { .. } => "InjectWarmup",
            CmdBody::ReplaceRefdata { .. } => "ReplaceRefdata",
            CmdBody::GetSchema { .. } => "GetSchema",
            CmdBody::ReconcileReport { .. } => "ReconcileReport",
            CmdBody::RotateConn { .. } => "RotateConn",
            CmdBody::Resubscribe { .. } => "Resubscribe",
            CmdBody::SnapshotNow { .. } => "SnapshotNow",
        }
    }

    /// `ControlCmd` number of the command (`GetSchema`/`ReconcileReport` → 0).
    pub fn cmd_kind(&self) -> u16 {
        match self {
            CmdBody::SetMode { .. } => 0x0001,
            CmdBody::SetParams { .. } => 0x0002,
            CmdBody::SetRisk { .. } => 0x0003,
            CmdBody::Kill { .. } => 0x0004,
            CmdBody::RotateConn { .. } => 0x0006,
            CmdBody::Resubscribe { .. } => 0x0007,
            CmdBody::SnapshotNow { .. } => 0x0008,
            CmdBody::ReplaceRefdata { .. } => 0x0009,
            CmdBody::Resume { .. } => 0x000A,
            CmdBody::HedgeFix { .. } => 0x000E,
            CmdBody::CancelAll { .. } => 0x000F,
            CmdBody::Drain { .. } => 0x0010,
            CmdBody::RecordTransfer { .. } => 0x0011,
            CmdBody::Annotate { .. } => 0x0012,
            CmdBody::AdoptExchange { .. } => 0x0013,
            CmdBody::SimReset { .. } => 0x0014,
            CmdBody::SimTransfer { .. } => 0x0015,
            CmdBody::VenueNotice { .. } => 0x0016,
            CmdBody::SetBalanceBaseline { .. } => 0x0017,
            CmdBody::StrategyCmd { .. } => 0x0018,
            CmdBody::InjectWarmup { .. } => 0x0019,
            CmdBody::SmokeOrder { .. } => 0x001A,
            CmdBody::WriteOffResidual { .. } => 0x001B,
            CmdBody::SimFault { .. } => 0x001C,
            CmdBody::BindSlot { .. } => 0x001D,
            CmdBody::GetSchema { .. } | CmdBody::ReconcileReport { .. } => 0,
        }
    }

    /// The in-body account, when the shape has one and it is given.
    pub fn body_account(&self) -> Option<&str> {
        let a = match self {
            CmdBody::SetMode { account, .. }
            | CmdBody::SetParams { account, .. }
            | CmdBody::SetRisk { account, .. }
            | CmdBody::Kill { account, .. }
            | CmdBody::Resume { account, .. }
            | CmdBody::CancelAll { account, .. }
            | CmdBody::Drain { account, .. }
            | CmdBody::HedgeFix { account, .. }
            | CmdBody::WriteOffResidual { account, .. }
            | CmdBody::SetBalanceBaseline { account, .. }
            | CmdBody::SimReset { account, .. }
            | CmdBody::SimTransfer { account, .. }
            | CmdBody::SimFault { account, .. }
            | CmdBody::RecordTransfer { account, .. }
            | CmdBody::Annotate { account, .. }
            | CmdBody::AdoptExchange { account, .. }
            | CmdBody::VenueNotice { account, .. }
            | CmdBody::SmokeOrder { account, .. }
            | CmdBody::BindSlot { account, .. }
            | CmdBody::StrategyCmd { account, .. }
            | CmdBody::InjectWarmup { account, .. }
            | CmdBody::ReplaceRefdata { account, .. }
            | CmdBody::RotateConn { account, .. }
            | CmdBody::Resubscribe { account, .. }
            | CmdBody::SnapshotNow { account } => account,
            CmdBody::GetSchema { .. } | CmdBody::ReconcileReport { .. } => &None,
        };
        a.as_deref()
    }
}

/// A parsed, schema-valid request.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct CmdRequest {
    pub v: u8,
    pub cmd_id: Uuid,
    /// ASCII, ≤ 24 bytes; its form (`tg:`, `cli:`, process names) is checked by the gateway.
    pub issuer: String,
    pub issued_at_ns: i64,
    pub expires_at_ns: i64,
    pub account: AccountSel,
    pub preview: bool,
    pub confirmed: bool,
    pub expect_state_version: Option<u64>,
    pub view_account: Option<String>,
    pub chat_type: Option<ChatType>,
    pub cmd: CmdBody,
}

/// Top-level keys of a request, in canonical order (no token field exists).
pub const REQUEST_FIELDS: &[&str] = &[
    "v",
    "cmd_id",
    "issuer",
    "issued_at_ns",
    "expires_at_ns",
    "account",
    "preview",
    "confirmed",
    "expect_state_version",
    "view_account",
    "chat_type",
    "cmd",
];

/// A schema-layer error: rendered as `status = invalid_request`, `code = InvalidArgs`,
/// `detail` = `UnknownDiscriminant` for an unknown `type` or enum name, otherwise absent.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct SchemaErr {
    /// Detail code (`UnknownDiscriminant`) or `None`.
    pub detail: Option<u16>,
    /// The request's `cmd_id` when it could be read, for the response.
    pub cmd_id: Option<Uuid>,
    /// Human-readable reason (not on the wire).
    pub why: String,
}

impl SchemaErr {
    fn new(why: impl Into<String>) -> SchemaErr {
        SchemaErr {
            detail: None,
            cmd_id: None,
            why: why.into(),
        }
    }
    fn unknown(why: impl Into<String>) -> SchemaErr {
        SchemaErr {
            detail: Some(detail::UNKNOWN_DISCRIMINANT),
            cmd_id: None,
            why: why.into(),
        }
    }
}

type R<T> = Result<T, SchemaErr>;

/// `-?[0-9]+(\.[0-9]+)?`.
pub fn is_decimal(s: &str) -> bool {
    let b = s.as_bytes();
    let b = b.strip_prefix(b"-").unwrap_or(b);
    let (int, frac) = match b.iter().position(|&c| c == b'.') {
        Some(p) => (b.get(..p).unwrap_or(&[]), Some(b.get(p + 1..).unwrap_or(&[]))),
        None => (b, None),
    };
    let digits = |x: &[u8]| !x.is_empty() && x.iter().all(u8::is_ascii_digit);
    digits(int) && frac.is_none_or(digits)
}

fn enum_value(d: &hft_event::EnumDesc, name: &str) -> Option<u32> {
    d.items.iter().find(|i| i.name == name).map(|i| i.value)
}

fn enum_name(d: &hft_event::EnumDesc, v: u32) -> &'static str {
    d.items.iter().find(|i| i.value == v).map_or("", |i| i.name)
}

/// Key-tracking reader over a JSON object.
struct Obj<'a> {
    m: &'a Map<String, Value>,
    used: Vec<&'a str>,
    what: &'static str,
}

impl<'a> Obj<'a> {
    fn new(v: &'a Value, what: &'static str) -> R<Obj<'a>> {
        match v {
            Value::Object(m) => Ok(Obj {
                m,
                used: Vec::new(),
                what,
            }),
            _ => Err(SchemaErr::new(format!("{what}: not an object"))),
        }
    }

    fn get(&mut self, k: &'static str) -> Option<&'a Value> {
        let (key, v) = self.m.get_key_value(k)?;
        self.used.push(key.as_str());
        Some(v)
    }

    fn str_opt(&mut self, k: &'static str) -> R<Option<String>> {
        match self.get(k) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(SchemaErr::new(format!("{}.{k}: not a string", self.what))),
        }
    }

    fn str_req(&mut self, k: &'static str) -> R<String> {
        self.str_opt(k)?
            .ok_or_else(|| SchemaErr::new(format!("{}.{k}: missing", self.what)))
    }

    fn dec_opt(&mut self, k: &'static str) -> R<Option<String>> {
        match self.str_opt(k)? {
            Some(s) if !is_decimal(&s) => {
                Err(SchemaErr::new(format!("{}.{k}: not a decimal string", self.what)))
            }
            x => Ok(x),
        }
    }

    fn dec_req(&mut self, k: &'static str) -> R<String> {
        self.dec_opt(k)?
            .ok_or_else(|| SchemaErr::new(format!("{}.{k}: missing", self.what)))
    }

    /// `"*"`, `"none"` or a decimal string.
    fn group_opt(&mut self, k: &'static str) -> R<Option<String>> {
        match self.str_opt(k)? {
            Some(s) if s != "*" && s != "none" && !is_decimal(&s) => {
                Err(SchemaErr::new(format!("{}.{k}: not a group", self.what)))
            }
            x => Ok(x),
        }
    }

    fn bool_opt(&mut self, k: &'static str) -> R<Option<bool>> {
        match self.get(k) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(SchemaErr::new(format!("{}.{k}: not a boolean", self.what))),
        }
    }

    fn enum_opt(&mut self, k: &'static str, d: &'static hft_event::EnumDesc) -> R<Option<u32>> {
        match self.get(k) {
            None => Ok(None),
            Some(Value::String(s)) => enum_value(d, s)
                .map(Some)
                .ok_or_else(|| SchemaErr::unknown(format!("{}.{k}: unknown {} {s:?}", self.what, d.name))),
            Some(_) => Err(SchemaErr::new(format!("{}.{k}: not a string", self.what))),
        }
    }

    fn enum_req(&mut self, k: &'static str, d: &'static hft_event::EnumDesc) -> R<u32> {
        self.enum_opt(k, d)?
            .ok_or_else(|| SchemaErr::new(format!("{}.{k}: missing", self.what)))
    }

    fn finish(self) -> R<()> {
        match self.m.keys().find(|k| !self.used.contains(&k.as_str())) {
            Some(k) => Err(SchemaErr::new(format!("{}: unknown field {k:?}", self.what))),
            None => Ok(()),
        }
    }
}

fn run_mode(v: u32) -> RunMode {
    match v {
        1 => RunMode::Shadow,
        3 => RunMode::CloseOnly,
        _ => RunMode::Live,
    }
}

fn kill_level(v: u32) -> KillLevel {
    match v {
        2 => KillLevel::Venue,
        3 => KillLevel::Account,
        _ => KillLevel::Global,
    }
}

fn parse_items(v: Option<&Value>) -> R<Vec<ReconItem>> {
    let Some(v) = v else {
        return Err(SchemaErr::new("ReconcileReport.items: missing"));
    };
    let Value::Array(a) = v else {
        return Err(SchemaErr::new("ReconcileReport.items: not an array"));
    };
    if a.is_empty() {
        return Err(SchemaErr::new("ReconcileReport.items: empty"));
    }
    let mut out = Vec::with_capacity(a.len());
    for it in a {
        let mut o = Obj::new(it, "item")?;
        let sflags = match o.get("sflags") {
            None => None,
            Some(Value::Array(names)) => {
                let mut s = Vec::with_capacity(names.len());
                for n in names {
                    let Value::String(n) = n else {
                        return Err(SchemaErr::new("item.sflags: not a string"));
                    };
                    if enum_value(&hft_event::codes::RECON_SFLAGS, n).is_none() {
                        return Err(SchemaErr::unknown(format!("item.sflags: unknown flag {n:?}")));
                    }
                    s.push(n.clone());
                }
                Some(s)
            }
            Some(_) => return Err(SchemaErr::new("item.sflags: not an array")),
        };
        let item = ReconItem {
            venue: o.str_opt("venue")?,
            account: o.str_opt("account")?,
            item: o.enum_req("item", &hft_event::codes::RECON_ITEM)? as u8,
            severity: o
                .enum_opt("severity", &hft_event::codes::RECON_SEVERITY)?
                .map(|v| v as u8),
            inst: o.str_opt("inst")?,
            asset: o.str_opt("asset")?,
            engine_qty: o.dec_opt("engine_qty")?,
            exch_qty: o.dec_opt("exch_qty")?,
            diff: o.dec_opt("diff")?,
            exch_ts_ns: o.dec_opt("exch_ts_ns")?,
            slot: o.dec_opt("slot")?,
            order_status: o
                .enum_opt("order_status", &hft_event::codes::RECON_ORDER_STATUS)?
                .map(|v| v as u8),
            history_checked: o.bool_opt("history_checked")?,
            view_seq: o.dec_opt("view_seq")?,
            r#gen: o.dec_opt("gen")?,
            epoch: o.dec_opt("epoch")?,
            exch_amount: o.dec_opt("exch_amount")?,
            engine_amount: o.dec_opt("engine_amount")?,
            sflags,
            skipped_inflight: o.dec_opt("skipped_inflight")?,
        };
        o.finish()?;
        out.push(item);
    }
    Ok(out)
}

fn parse_body(v: &Value) -> R<CmdBody> {
    use hft_event::codes as c;
    let mut o = Obj::new(v, "cmd")?;
    let ty = o.str_req("type")?;
    let body = match ty.as_str() {
        "SetMode" => CmdBody::SetMode {
            strategy: o.str_opt("strategy")?,
            group: o.group_opt("group")?,
            account: o.str_opt("account")?,
            mode: run_mode(o.enum_req("mode", &c::RUN_MODE)?),
        },
        "SetParams" => CmdBody::SetParams {
            strategy: o.str_req("strategy")?,
            account: o.str_opt("account")?,
            blob_b64: o.str_req("blob_b64")?,
            base_version: o.dec_opt("base_version")?,
        },
        "SetRisk" => CmdBody::SetRisk {
            account: o.str_opt("account")?,
            blob_b64: o.str_req("blob_b64")?,
            base_version: o.dec_opt("base_version")?,
        },
        "Kill" => CmdBody::Kill {
            level: kill_level(o.enum_req("level", &c::KILL_LEVEL)?),
            venue: o.str_opt("venue")?,
            account: o.str_opt("account")?,
            cancel_resting: o.bool_opt("cancel_resting")?,
            flatten: o.bool_opt("flatten")?,
            hard: o.bool_opt("hard")?,
        },
        "Resume" => CmdBody::Resume {
            source: CapSource(o.enum_req("source", &c::CAP_SOURCE)? as u8),
            strategy: o.str_opt("strategy")?,
            venue: o.str_opt("venue")?,
            group: o.group_opt("group")?,
            inst: o.str_opt("inst")?,
            account: o.str_opt("account")?,
        },
        "CancelAll" => CmdBody::CancelAll {
            venue: o.str_opt("venue")?,
            inst: o.str_opt("inst")?,
            account: o.str_opt("account")?,
        },
        "Drain" => CmdBody::Drain {
            cancel_resting: o.bool_opt("cancel_resting")?,
            account: o.str_opt("account")?,
        },
        "HedgeFix" => CmdBody::HedgeFix {
            group: o.group_opt("group")?,
            account: o.str_opt("account")?,
        },
        "WriteOffResidual" => CmdBody::WriteOffResidual {
            group: o.group_opt("group")?,
            account: o.str_opt("account")?,
        },
        "SetBalanceBaseline" => CmdBody::SetBalanceBaseline {
            venue: o.str_req("venue")?,
            account: o.str_opt("account")?,
        },
        "SimReset" => CmdBody::SimReset {
            account: o.str_opt("account")?,
        },
        "SimTransfer" => CmdBody::SimTransfer {
            from_venue: o.str_req("from_venue")?,
            to_venue: o.str_req("to_venue")?,
            asset: o.str_req("asset")?,
            amount: o.dec_req("amount")?,
            account: o.str_opt("account")?,
        },
        "SimFault" => {
            let venue = o.str_req("venue")?;
            let fault = match o.enum_req("fault", &c::SIM_FAULT)? {
                1 => SimFaultKind::Off,
                2 => SimFaultKind::RejectNew,
                3 => SimFaultKind::Disconnect,
                _ => SimFaultKind::DelayReports,
            };
            CmdBody::SimFault {
                venue,
                fault,
                delay_ms: o.dec_opt("delay_ms")?,
                account: o.str_opt("account")?,
            }
        }
        "RecordTransfer" => CmdBody::RecordTransfer {
            from: o.str_req("from")?,
            to: o.str_req("to")?,
            asset: o.str_req("asset")?,
            amount: o.dec_req("amount")?,
            ext_ref: o.str_req("ext_ref")?,
            account: o.str_opt("account")?,
        },
        "Annotate" => {
            let kind = match o.enum_req("kind", &c::ANNOTATE_KIND)? {
                1 => AnnotateKind::OpsNote,
                2 => AnnotateKind::LocalWrite,
                _ => AnnotateKind::Incident,
            };
            CmdBody::Annotate {
                kind,
                text: o.str_req("text")?,
                digest: o.str_opt("digest")?,
                account: o.str_opt("account")?,
            }
        }
        "AdoptExchange" => {
            let venue = o.str_req("venue")?;
            let report_id = o.dec_req("report_id")?;
            let item = if o.enum_req("item", &c::ADOPT_ITEM)? == 0 {
                AdoptItem::Position
            } else {
                AdoptItem::Balance
            };
            let inst = o.str_opt("inst")?;
            let asset = o.str_opt("asset")?;
            match item {
                AdoptItem::Position if inst.is_none() || asset.is_some() => {
                    return Err(SchemaErr::new("AdoptExchange: Position needs inst and no asset"));
                }
                AdoptItem::Balance if asset.is_none() || inst.is_some() => {
                    return Err(SchemaErr::new("AdoptExchange: Balance needs asset and no inst"));
                }
                _ => {}
            }
            let group = o.group_opt("group")?;
            if item == AdoptItem::Position && group.is_some() {
                return Err(SchemaErr::new("AdoptExchange: group only with Balance"));
            }
            CmdBody::AdoptExchange {
                venue,
                report_id,
                item,
                inst,
                asset,
                group,
                entry_px: o.dec_opt("entry_px")?,
                account: o.str_opt("account")?,
            }
        }
        "VenueNotice" => {
            let venue = o.str_req("venue")?;
            let inst = o.str_opt("inst")?;
            let kind = match o.enum_req("kind", &c::VENUE_NOTICE_KIND)? {
                1 => NoticeKind::Delisting {
                    at_ns: o.dec_req("at_ns")?,
                },
                2 => NoticeKind::FundingInterval {
                    new_s: o.dec_req("new_s")?,
                },
                _ => NoticeKind::Maintenance {
                    from_ns: o.dec_req("from_ns")?,
                    to_ns: o.dec_req("to_ns")?,
                },
            };
            CmdBody::VenueNotice {
                venue,
                inst,
                kind,
                account: o.str_opt("account")?,
            }
        }
        "SmokeOrder" => CmdBody::SmokeOrder {
            venue: o.str_req("venue")?,
            inst: o.str_req("inst")?,
            side: if o.enum_req("side", &c::SIDE)? == 1 {
                Side::Buy
            } else {
                Side::Sell
            },
            ttl_ms: o.dec_opt("ttl_ms")?,
            account: o.str_opt("account")?,
        },
        "BindSlot" => CmdBody::BindSlot {
            venue: o.str_req("venue")?,
            inst: o.str_req("inst")?,
            start_s: o.dec_req("start_s")?,
            end_s: o.dec_req("end_s")?,
            wire_id: o.dec_req("wire_id")?,
            account: o.str_opt("account")?,
        },
        "StrategyCmd" => CmdBody::StrategyCmd {
            strategy: o.str_req("strategy")?,
            group: o.group_opt("group")?,
            reduce_only: o.bool_opt("reduce_only")?,
            payload_b64: o.str_req("payload_b64")?,
            account: o.str_opt("account")?,
        },
        "InjectWarmup" => CmdBody::InjectWarmup {
            strategy: o.str_req("strategy")?,
            local_kind: o.dec_req("local_kind")?,
            chunk_no: o.dec_req("chunk_no")?,
            last: o.bool_opt("last")?,
            payload_b64: o.str_req("payload_b64")?,
            account: o.str_opt("account")?,
        },
        "ReplaceRefdata" => CmdBody::ReplaceRefdata {
            blob_b64: o.str_req("blob_b64")?,
            version: o.dec_req("version")?,
            account: o.str_opt("account")?,
        },
        "GetSchema" => {
            let strategy_kind = o.str_opt("strategy_kind")?;
            let kind_id = o.dec_opt("kind_id")?;
            if strategy_kind.is_some() == kind_id.is_some() {
                return Err(SchemaErr::new("GetSchema: exactly one of strategy_kind, kind_id"));
            }
            CmdBody::GetSchema {
                strategy_kind,
                kind_id,
            }
        }
        "ReconcileReport" => {
            let report_id = o.dec_req("report_id")?;
            let items = parse_items(o.get("items"))?;
            let receipts_b64 = o.str_opt("receipts_b64")?;
            CmdBody::ReconcileReport {
                report_id,
                items,
                receipts_b64,
            }
        }
        "RotateConn" => CmdBody::RotateConn {
            venue: o.str_req("venue")?,
            account: o.str_opt("account")?,
        },
        "Resubscribe" => CmdBody::Resubscribe {
            venue: o.str_req("venue")?,
            inst: o.str_opt("inst")?,
            account: o.str_opt("account")?,
        },
        "SnapshotNow" => CmdBody::SnapshotNow {
            account: o.str_opt("account")?,
        },
        other => return Err(SchemaErr::unknown(format!("cmd.type: unknown {other:?}"))),
    };
    o.finish()?;
    Ok(body)
}

fn int_field<T: TryFrom<i128>>(v: Option<&Value>, k: &str) -> R<Option<T>> {
    match v {
        None => Ok(None),
        Some(Value::Number(n)) => {
            let x: Option<i128> = n.as_i64().map(i128::from).or_else(|| n.as_u64().map(i128::from));
            x.and_then(|x| T::try_from(x).ok())
                .map(Some)
                .ok_or_else(|| SchemaErr::new(format!("{k}: not an integer of its type")))
        }
        Some(_) => Err(SchemaErr::new(format!("{k}: not an integer"))),
    }
}

fn read_cmd_id(m: &Map<String, Value>) -> Option<Uuid> {
    match m.get("cmd_id") {
        Some(Value::String(s)) if s.len() == 36 => Uuid::try_parse(s).ok(),
        _ => None,
    }
}

/// Parses a frame body. Every failure is a schema-layer error (`invalid_request`).
pub fn parse_request(bytes: &[u8]) -> Result<CmdRequest, SchemaErr> {
    let Ok(text) = core::str::from_utf8(bytes) else {
        return Err(SchemaErr::new("frame body is not UTF-8"));
    };
    let v: Value = serde_json::from_str(text).map_err(|e| SchemaErr::new(format!("JSON: {e}")))?;
    let Value::Object(m) = &v else {
        return Err(SchemaErr::new("request is not an object"));
    };
    let cmd_id = read_cmd_id(m);
    parse_object(&v).map_err(|mut e| {
        e.cmd_id = cmd_id;
        e
    })
}

fn parse_object(v: &Value) -> R<CmdRequest> {
    let mut o = Obj::new(v, "request")?;
    let ver: u8 = int_field(o.get("v"), "v")?.ok_or_else(|| SchemaErr::new("v: missing"))?;
    if ver != PROTO_VERSION {
        return Err(SchemaErr::new(format!("v: {ver} is not {PROTO_VERSION}")));
    }
    let cmd_id = o.str_req("cmd_id")?;
    let cmd_id = (cmd_id.len() == 36)
        .then(|| Uuid::try_parse(&cmd_id).ok())
        .flatten()
        .ok_or_else(|| SchemaErr::new("cmd_id: not a UUID"))?;
    let issuer = o.str_req("issuer")?;
    if issuer.is_empty() || issuer.len() > ISSUER_MAX || !issuer.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(SchemaErr::new(
            "issuer: empty, longer than 24 bytes or not printable ASCII",
        ));
    }
    let issued_at_ns: i64 = int_field(o.get("issued_at_ns"), "issued_at_ns")?
        .ok_or_else(|| SchemaErr::new("issued_at_ns: missing"))?;
    let expires_at_ns: i64 = int_field(o.get("expires_at_ns"), "expires_at_ns")?
        .ok_or_else(|| SchemaErr::new("expires_at_ns: missing"))?;
    let account = match o.str_opt("account")? {
        None => AccountSel::Missing,
        Some(s) if s == "*" => AccountSel::All,
        Some(s) => AccountSel::Name(s),
    };
    let preview = o.bool_opt("preview")?.unwrap_or(false);
    let confirmed = o.bool_opt("confirmed")?.unwrap_or(false);
    let expect_state_version: Option<u64> = int_field(o.get("expect_state_version"), "expect_state_version")?;
    let view_account = o.str_opt("view_account")?;
    let chat_type = match o.str_opt("chat_type")? {
        None => None,
        Some(s) if s == "private" => Some(ChatType::Private),
        Some(s) if s == "group" => Some(ChatType::Group),
        Some(s) => return Err(SchemaErr::unknown(format!("chat_type: unknown {s:?}"))),
    };
    let cmd = parse_body(o.get("cmd").ok_or_else(|| SchemaErr::new("cmd: missing"))?)?;
    o.finish()?;
    if preview && confirmed {
        return Err(SchemaErr::new("preview and confirmed are both true"));
    }
    let span = i128::from(expires_at_ns) - i128::from(issued_at_ns);
    if span <= 0 || span > i128::from(MAX_EXPIRY_NS) {
        return Err(SchemaErr::new(
            "expires_at_ns not in (issued_at_ns, issued_at_ns + 300 s]",
        ));
    }
    if issuer.starts_with("tg:") && chat_type.is_none() {
        return Err(SchemaErr::new("chat_type is required for tg:* issuers"));
    }
    Ok(CmdRequest {
        v: ver,
        cmd_id,
        issuer,
        issued_at_ns,
        expires_at_ns,
        account,
        preview,
        confirmed,
        expect_state_version,
        view_account,
        chat_type,
        cmd,
    })
}

/// Compact JSON writer with caller-chosen key order.
struct Out {
    s: String,
    first: Vec<bool>,
}

impl Out {
    fn new() -> Out {
        Out {
            s: String::new(),
            first: Vec::new(),
        }
    }
    fn open(&mut self) {
        self.s.push('{');
        self.first.push(true);
    }
    fn close(&mut self) {
        self.s.push('}');
        self.first.pop();
    }
    fn key(&mut self, k: &str) {
        if let Some(f) = self.first.last_mut() {
            if !*f {
                self.s.push(',');
            }
            *f = false;
        }
        self.quoted(k);
        self.s.push(':');
    }
    fn quoted(&mut self, v: &str) {
        self.s.push_str(&serde_json::to_string(v).unwrap_or_default());
    }
    fn str(&mut self, k: &str, v: &str) {
        self.key(k);
        self.quoted(v);
    }
    fn str_opt(&mut self, k: &str, v: &Option<String>) {
        if let Some(v) = v {
            self.str(k, v);
        }
    }
    fn raw(&mut self, k: &str, v: &str) {
        self.key(k);
        self.s.push_str(v);
    }
    fn bool_opt(&mut self, k: &str, v: Option<bool>) {
        if let Some(b) = v {
            self.raw(k, if b { "true" } else { "false" });
        }
    }
}

fn write_item(o: &mut Out, it: &ReconItem) {
    use hft_event::codes as c;
    o.open();
    o.str_opt("venue", &it.venue);
    o.str_opt("account", &it.account);
    o.str("item", enum_name(&c::RECON_ITEM, u32::from(it.item)));
    if let Some(s) = it.severity {
        o.str("severity", enum_name(&c::RECON_SEVERITY, u32::from(s)));
    }
    o.str_opt("inst", &it.inst);
    o.str_opt("asset", &it.asset);
    o.str_opt("engine_qty", &it.engine_qty);
    o.str_opt("exch_qty", &it.exch_qty);
    o.str_opt("diff", &it.diff);
    o.str_opt("exch_ts_ns", &it.exch_ts_ns);
    o.str_opt("slot", &it.slot);
    if let Some(s) = it.order_status {
        o.str("order_status", enum_name(&c::RECON_ORDER_STATUS, u32::from(s)));
    }
    o.bool_opt("history_checked", it.history_checked);
    o.str_opt("view_seq", &it.view_seq);
    o.str_opt("gen", &it.r#gen);
    o.str_opt("epoch", &it.epoch);
    o.str_opt("exch_amount", &it.exch_amount);
    o.str_opt("engine_amount", &it.engine_amount);
    if let Some(f) = &it.sflags {
        let names: Vec<String> = f
            .iter()
            .map(|n| serde_json::to_string(n).unwrap_or_default())
            .collect();
        o.raw("sflags", &format!("[{}]", names.join(",")));
    }
    o.str_opt("skipped_inflight", &it.skipped_inflight);
    o.close();
}

fn write_body(o: &mut Out, b: &CmdBody) {
    use hft_event::codes as c;
    o.open();
    o.str("type", b.type_name());
    match b {
        CmdBody::SetMode {
            strategy,
            group,
            account,
            mode,
        } => {
            o.str_opt("account", account);
            o.str_opt("strategy", strategy);
            o.str_opt("group", group);
            o.str("mode", enum_name(&c::RUN_MODE, *mode as u32));
        }
        CmdBody::SetParams {
            strategy,
            account,
            blob_b64,
            base_version,
        } => {
            o.str("strategy", strategy);
            o.str_opt("account", account);
            o.str("blob_b64", blob_b64);
            o.str_opt("base_version", base_version);
        }
        CmdBody::SetRisk {
            account,
            blob_b64,
            base_version,
        } => {
            o.str_opt("account", account);
            o.str("blob_b64", blob_b64);
            o.str_opt("base_version", base_version);
        }
        CmdBody::Kill {
            level,
            venue,
            account,
            cancel_resting,
            flatten,
            hard,
        } => {
            o.str("level", enum_name(&c::KILL_LEVEL, *level as u32));
            o.str_opt("account", account);
            o.str_opt("venue", venue);
            o.bool_opt("cancel_resting", *cancel_resting);
            o.bool_opt("flatten", *flatten);
            o.bool_opt("hard", *hard);
        }
        CmdBody::Resume {
            source,
            strategy,
            venue,
            group,
            inst,
            account,
        } => {
            o.str("source", enum_name(&c::CAP_SOURCE, u32::from(source.0)));
            o.str_opt("account", account);
            o.str_opt("strategy", strategy);
            o.str_opt("venue", venue);
            o.str_opt("group", group);
            o.str_opt("inst", inst);
        }
        CmdBody::CancelAll { venue, inst, account } => {
            o.str_opt("account", account);
            o.str_opt("venue", venue);
            o.str_opt("inst", inst);
        }
        CmdBody::Drain {
            cancel_resting,
            account,
        } => {
            o.str_opt("account", account);
            o.bool_opt("cancel_resting", *cancel_resting);
        }
        CmdBody::HedgeFix { group, account } | CmdBody::WriteOffResidual { group, account } => {
            o.str_opt("account", account);
            o.str_opt("group", group);
        }
        CmdBody::SetBalanceBaseline { venue, account } | CmdBody::RotateConn { venue, account } => {
            o.str_opt("account", account);
            o.str("venue", venue);
        }
        CmdBody::SimReset { account } | CmdBody::SnapshotNow { account } => {
            o.str_opt("account", account);
        }
        CmdBody::SimTransfer {
            from_venue,
            to_venue,
            asset,
            amount,
            account,
        } => {
            o.str_opt("account", account);
            o.str("from_venue", from_venue);
            o.str("to_venue", to_venue);
            o.str("asset", asset);
            o.str("amount", amount);
        }
        CmdBody::SimFault {
            venue,
            fault,
            delay_ms,
            account,
        } => {
            o.str_opt("account", account);
            o.str("venue", venue);
            o.str("fault", enum_name(&c::SIM_FAULT, *fault as u32));
            o.str_opt("delay_ms", delay_ms);
        }
        CmdBody::RecordTransfer {
            from,
            to,
            asset,
            amount,
            ext_ref,
            account,
        } => {
            o.str_opt("account", account);
            o.str("from", from);
            o.str("to", to);
            o.str("asset", asset);
            o.str("amount", amount);
            o.str("ext_ref", ext_ref);
        }
        CmdBody::Annotate {
            kind,
            text,
            digest,
            account,
        } => {
            o.str_opt("account", account);
            o.str("kind", enum_name(&c::ANNOTATE_KIND, *kind as u32));
            o.str("text", text);
            o.str_opt("digest", digest);
        }
        CmdBody::AdoptExchange {
            venue,
            report_id,
            item,
            inst,
            asset,
            group,
            entry_px,
            account,
        } => {
            o.str_opt("account", account);
            o.str("venue", venue);
            o.str("report_id", report_id);
            o.str("item", enum_name(&c::ADOPT_ITEM, *item as u32));
            o.str_opt("inst", inst);
            o.str_opt("asset", asset);
            o.str_opt("group", group);
            o.str_opt("entry_px", entry_px);
        }
        CmdBody::VenueNotice {
            venue,
            inst,
            kind,
            account,
        } => {
            o.str_opt("account", account);
            o.str("venue", venue);
            o.str_opt("inst", inst);
            match kind {
                NoticeKind::Delisting { at_ns } => {
                    o.str("kind", "Delisting");
                    o.str("at_ns", at_ns);
                }
                NoticeKind::FundingInterval { new_s } => {
                    o.str("kind", "FundingInterval");
                    o.str("new_s", new_s);
                }
                NoticeKind::Maintenance { from_ns, to_ns } => {
                    o.str("kind", "Maintenance");
                    o.str("from_ns", from_ns);
                    o.str("to_ns", to_ns);
                }
            }
        }
        CmdBody::SmokeOrder {
            venue,
            inst,
            side,
            ttl_ms,
            account,
        } => {
            o.str_opt("account", account);
            o.str("venue", venue);
            o.str("inst", inst);
            o.str("side", enum_name(&c::SIDE, *side as u32));
            o.str_opt("ttl_ms", ttl_ms);
        }
        CmdBody::BindSlot {
            venue,
            inst,
            start_s,
            end_s,
            wire_id,
            account,
        } => {
            o.str_opt("account", account);
            o.str("venue", venue);
            o.str("inst", inst);
            o.str("start_s", start_s);
            o.str("end_s", end_s);
            o.str("wire_id", wire_id);
        }
        CmdBody::StrategyCmd {
            strategy,
            group,
            reduce_only,
            payload_b64,
            account,
        } => {
            o.str("strategy", strategy);
            o.str_opt("account", account);
            o.str_opt("group", group);
            o.bool_opt("reduce_only", *reduce_only);
            o.str("payload_b64", payload_b64);
        }
        CmdBody::InjectWarmup {
            strategy,
            local_kind,
            chunk_no,
            last,
            payload_b64,
            account,
        } => {
            o.str("strategy", strategy);
            o.str_opt("account", account);
            o.str("local_kind", local_kind);
            o.str("chunk_no", chunk_no);
            o.bool_opt("last", *last);
            o.str("payload_b64", payload_b64);
        }
        CmdBody::ReplaceRefdata {
            blob_b64,
            version,
            account,
        } => {
            o.str_opt("account", account);
            o.str("blob_b64", blob_b64);
            o.str("version", version);
        }
        CmdBody::GetSchema {
            strategy_kind,
            kind_id,
        } => {
            o.str_opt("strategy_kind", strategy_kind);
            o.str_opt("kind_id", kind_id);
        }
        CmdBody::ReconcileReport {
            report_id,
            items,
            receipts_b64,
        } => {
            o.str("report_id", report_id);
            o.key("items");
            o.s.push('[');
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    o.s.push(',');
                }
                write_item(o, it);
            }
            o.s.push(']');
            o.str_opt("receipts_b64", receipts_b64);
        }
        CmdBody::Resubscribe { venue, inst, account } => {
            o.str_opt("account", account);
            o.str("venue", venue);
            o.str_opt("inst", inst);
        }
    }
    o.close();
}

impl CmdBody {
    /// Canonical compact JSON of the command object.
    pub fn to_json(&self) -> String {
        let mut o = Out::new();
        write_body(&mut o, self);
        o.s
    }
}

impl CmdRequest {
    /// Canonical compact JSON (key order = `REQUEST_FIELDS`; `preview`/`confirmed` only when true,
    /// absent options omitted). `parse_request(to_json(r)) == r`.
    pub fn to_json(&self) -> Vec<u8> {
        let mut o = Out::new();
        o.open();
        o.raw("v", &self.v.to_string());
        o.str("cmd_id", &self.cmd_id.hyphenated().to_string());
        o.str("issuer", &self.issuer);
        o.raw("issued_at_ns", &self.issued_at_ns.to_string());
        o.raw("expires_at_ns", &self.expires_at_ns.to_string());
        match &self.account {
            AccountSel::Missing => {}
            AccountSel::All => o.str("account", "*"),
            AccountSel::Name(n) => o.str("account", n),
        }
        if self.preview {
            o.raw("preview", "true");
        }
        if self.confirmed {
            o.raw("confirmed", "true");
        }
        if let Some(v) = self.expect_state_version {
            o.raw("expect_state_version", &v.to_string());
        }
        o.str_opt("view_account", &self.view_account);
        if let Some(c) = self.chat_type {
            o.str("chat_type", c.name());
        }
        o.key("cmd");
        write_body(&mut o, &self.cmd);
        o.close();
        o.s.into_bytes()
    }
}
