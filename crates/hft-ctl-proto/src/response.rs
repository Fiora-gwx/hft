//! Response schema and its rendering from `CmdResult` (detail names, preview body).

use std::collections::BTreeMap;

use hft_event::{CmdPreview, CmdResult};
use hft_refdata::RefData;
use hft_types::{AccountId, VenueId, write_fixed};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;
use uuid::Uuid;

use crate::codes::{Detail, ResultCode, TransportStatus, render_detail};
use crate::names;
use crate::{PROTO_VERSION, VIEWER_CONFIRM_TTL_MS};

/// `CmdResult.rflags` bits.
pub const RFLAG_PREVIEW: u32 = 1;
pub const RFLAG_NEEDS_CONFIRM: u32 = 2;
pub const RFLAG_PREVIEW_TTL: u32 = 4;

/// 32 bytes rendered as 64 lowercase hex characters.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Hex32(pub [u8; 32]);

impl Hex32 {
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
    pub fn from_hex(s: &str) -> Option<Hex32> {
        if s.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, o) in out.iter_mut().enumerate() {
            *o = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
        }
        Some(Hex32(out))
    }
}

impl Serialize for Hex32 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Hex32 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Hex32, D::Error> {
        let s = String::deserialize(d)?;
        Hex32::from_hex(&s).ok_or_else(|| serde::de::Error::custom("cmd_hash: not 64 hex characters"))
    }
}

/// Engine identity and configuration versions carried by every response.
#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct EngineInfo {
    pub version: String,
    pub epoch: u32,
    /// `"sha256:<hex>"` of the active refdata.
    pub refdata: String,
    /// `"<instance>/<account>"` → params version.
    pub params: BTreeMap<String, u64>,
    pub risk: u64,
}

/// A response frame body. Key order is the field order; absent options are omitted.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CmdResponse {
    pub v: u8,
    pub cmd_id: Uuid,
    pub status: TransportStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<ResultCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Detail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmd_hash: Option<Hex32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctl_state_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<Box<RawValue>>,
    pub engine: EngineInfo,
    /// Passed through byte for byte (`GetSchema`: the SDK schema JSON).
    pub body: Box<RawValue>,
}

/// `{}`.
pub fn empty_body() -> Box<RawValue> {
    serde_json::value::to_raw_value(&serde_json::Map::new()).unwrap_or_default()
}

/// Raw JSON from a value (serialized compactly with `serde_json`).
pub fn raw_of(v: &serde_json::Value) -> Box<RawValue> {
    serde_json::value::to_raw_value(v).unwrap_or_else(|_| empty_body())
}

impl CmdResponse {
    /// A response with a status; `code` = the status' implied code (`ok` takes `code`).
    pub fn new(
        cmd_id: Uuid,
        status: TransportStatus,
        code: Option<ResultCode>,
        engine: EngineInfo,
    ) -> CmdResponse {
        let code = match status {
            TransportStatus::Ok => code,
            TransportStatus::Accepted => None,
            s => s.implied_code(),
        };
        CmdResponse {
            v: PROTO_VERSION,
            cmd_id,
            status,
            code,
            detail: None,
            seq: None,
            cmd_hash: None,
            ctl_state_version: None,
            preview: None,
            engine,
            body: empty_body(),
        }
    }

    /// `ok` with a gateway result code and optional raw detail (`code | arg << 16`).
    pub fn gateway(cmd_id: Uuid, code: ResultCode, detail: u32, engine: EngineInfo) -> CmdResponse {
        let mut r = CmdResponse::new(cmd_id, TransportStatus::Ok, Some(code), engine);
        r.detail = render_detail(detail);
        r
    }

    /// `ok` rendered from a sequencer `CmdResult`.
    pub fn from_result(
        res: &CmdResult,
        cmd_hash: Option<[u8; 32]>,
        rd: &RefData,
        engine: EngineInfo,
    ) -> CmdResponse {
        let code = ResultCode::from_u8(res.status).unwrap_or(ResultCode::Internal);
        let mut r = CmdResponse::new(
            Uuid::from_bytes(res.cmd_id),
            TransportStatus::Ok,
            Some(code),
            engine,
        );
        r.detail = render_detail(res.detail);
        r.seq = Some(res.cmd_seq);
        r.cmd_hash = cmd_hash.map(Hex32);
        r.ctl_state_version = Some(res.ctl_state_version);
        r.preview = render_preview(&res.preview, res.rflags, rd);
        r
    }

    /// Compact JSON.
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Parses a response frame body.
    pub fn from_json(b: &[u8]) -> Result<CmdResponse, serde_json::Error> {
        serde_json::from_slice(b)
    }
}

fn account_name(rd: &RefData, a: AccountId) -> String {
    let l = names::account_label(rd, a);
    if l.is_empty() {
        a.0.to_string()
    } else {
        l.to_owned()
    }
}

fn venue_name(v: VenueId) -> String {
    names::venue_name(v).map_or_else(|| v.0.to_string(), str::to_owned)
}

/// Renders the preview block (`rflags.PREVIEW` set) as JSON: keys = `CmdPreview` field names;
/// accounts and venues by name, insts as `<venue>:<symbol>`, numbers as decimal strings; the
/// `wo_*` keys only when `wo_venue ≠ 0`; `"confirm_within_s": 60` under `PREVIEW_TTL`.
pub fn render_preview(p: &CmdPreview, rflags: u32, rd: &RefData) -> Option<Box<RawValue>> {
    if rflags & RFLAG_PREVIEW == 0 {
        return None;
    }
    let s = |v: String| serde_json::Value::String(v);
    let mode = |v: u8| {
        hft_event::codes::RUN_MODE
            .items
            .iter()
            .find(|i| i.value == u32::from(v))
            .map_or_else(|| v.to_string(), |i| i.name.to_owned())
    };
    let kind = hft_event::codes::ACCOUNT_KIND
        .items
        .iter()
        .find(|i| i.value == u32::from(p.target_kind))
        .map_or_else(|| p.target_kind.to_string(), |i| i.name.to_owned());
    // Insertion order is the rendered order (see `to_ordered`).
    let mut keys: Vec<(&str, serde_json::Value)> = vec![
        ("target_account", s(account_name(rd, p.target_account))),
        ("target_kind", s(kind)),
        ("target_mode", s(mode(p.target_mode))),
    ];
    if p.other_account.0 != 0 {
        keys.push(("other_account", s(account_name(rd, p.other_account))));
        keys.push(("other_mode", s(mode(p.other_mode))));
    }
    keys.extend([
        ("affected_cells", s(p.affected_cells.to_string())),
        ("open_orders", s(p.open_orders.to_string())),
        ("active_caps", s(p.active_caps.to_string())),
        ("cap_sources", s(p.cap_sources.to_string())),
        ("reset_groups", s(p.reset_groups.to_string())),
        ("no_open_ms", s(p.no_open_ms.to_string())),
    ]);
    if p.wo_venue.0 != 0 {
        let exp = rd.asset(rd.inst(p.wo_inst).asset).exp.qty_exp;
        let mut buf = [0u8; 40];
        let n = write_fixed(p.wo_qty.0, exp, &mut buf);
        let qty = String::from_utf8_lossy(buf.get(..n).unwrap_or(&[])).into_owned();
        keys.extend([
            ("wo_venue", s(venue_name(p.wo_venue))),
            ("wo_leg_ix", s(p.wo_leg_ix.to_string())),
            ("wo_inst", s(names::inst_name(rd, p.wo_inst))),
            ("wo_qty", s(qty)),
        ]);
    }
    if rflags & RFLAG_PREVIEW_TTL != 0 {
        keys.push((
            "confirm_within_s",
            serde_json::Value::from(VIEWER_CONFIRM_TTL_MS / 1000),
        ));
    }
    let mut out = String::from("{");
    for (i, (k, v)) in keys.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&serde_json::to_string(k).unwrap_or_default());
        out.push(':');
        out.push_str(&serde_json::to_string(&v).unwrap_or_default());
    }
    out.push('}');
    RawValue::from_string(out).ok()
}
