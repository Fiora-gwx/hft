//! `OwnedSchema`: a parameter schema (and its command descriptors) restored from the canonical
//! schema JSON, for tools that do not link strategy crates (hft-cfg, hft-ctl, hft-py).

use super::json::{self, Json};
use super::schema_json::digest_of;
use super::{EnumNames, FieldRef, MinorChange, ParamError, Relax, SchemaView, UnitKind};
use crate::desc::CmdSchema;

/// Most parameter fields in one schema.
const MAX_FIELDS: usize = 256;
/// Most fields in one command payload.
const MAX_CMD_FIELDS: usize = 31;

/// One field restored from JSON (owned form of `FieldDesc`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedField {
    pub name: String,
    pub unit: UnitKind,
    pub enum_names: Vec<String>,
    pub min: i64,
    pub max: i64,
    pub default: i64,
    pub per_group: bool,
    pub structural: bool,
    pub manual_only: bool,
    pub prod_min: Option<i64>,
    pub relax: Relax,
    pub minor_change: MinorChange,
    pub since: u16,
    pub doc: String,
}

impl OwnedField {
    pub fn as_ref(&self, ix: u16) -> FieldRef<'_> {
        FieldRef {
            ix,
            name: &self.name,
            unit: self.unit,
            enum_names: EnumNames::Owned(&self.enum_names),
            min: self.min,
            max: self.max,
            default: self.default,
            per_group: self.per_group,
            structural: self.structural,
            manual_only: self.manual_only,
            prod_min: self.prod_min,
            relax: self.relax,
            minor_change: self.minor_change,
            since: self.since,
            doc: &self.doc,
        }
    }
}

/// One command descriptor restored from JSON.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedCmd {
    pub op: u8,
    pub name: String,
    pub reduce_only: bool,
    pub fields: Vec<OwnedField>,
}

impl CmdSchema for OwnedCmd {
    fn op(&self) -> u8 {
        self.op
    }
    fn n_fields(&self) -> u16 {
        u16::try_from(self.fields.len()).unwrap_or(u16::MAX)
    }
    fn field(&self, ix: u16) -> Option<FieldRef<'_>> {
        self.fields.get(usize::from(ix)).map(|f| f.as_ref(ix))
    }
}

/// A schema restored from canonical schema JSON (`params::schema_json` output); encodes and
/// decodes exactly like the compiled-in `ParamSchemaDesc`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedSchema {
    name: String,
    kind_id: u16,
    id: u32,
    version: u16,
    fields: Vec<OwnedField>,
    cmds: Vec<OwnedCmd>,
}

/// Any rejection of the JSON (`from_json` reports only that it is not a valid schema).
const BAD: ParamError = ParamError::SchemaMismatch { want: 0, got: 0 };

fn int<T: TryFrom<i128>>(v: &Json, key: &str) -> Result<T, ParamError> {
    v.get(key)
        .and_then(Json::as_int)
        .and_then(|x| T::try_from(x).ok())
        .ok_or(BAD)
}

fn boolean(v: &Json, key: &str) -> Result<bool, ParamError> {
    v.get(key).and_then(Json::as_bool).ok_or(BAD)
}

fn text(v: &Json, key: &str) -> Result<String, ParamError> {
    v.get(key).and_then(Json::as_str).map(str::to_string).ok_or(BAD)
}

fn relax(s: &str) -> Option<Relax> {
    match s {
        "up" => Some(Relax::Up),
        "down" => Some(Relax::Down),
        "none" => Some(Relax::None),
        _ => None,
    }
}

fn minor(s: &str) -> Option<MinorChange> {
    match s {
        "none" => Some(MinorChange::None),
        "any" => Some(MinorChange::Any),
        _ => {
            if let Some(n) = s.strip_prefix("rel_pct:") {
                n.parse().ok().map(MinorChange::RelPct)
            } else if let Some(n) = s.strip_prefix("abs:") {
                n.parse().ok().map(MinorChange::Abs)
            } else {
                None
            }
        }
    }
}

/// One `fields` element, checked against the schema rules (README "schema 编译期校验").
fn field(v: &Json, ix: usize) -> Result<OwnedField, ParamError> {
    if int::<usize>(v, "ix")? != ix {
        return Err(BAD);
    }
    let unit = v
        .get("unit")
        .and_then(Json::as_str)
        .and_then(UnitKind::from_json_name)
        .ok_or(BAD)?;
    let enum_names = match (unit, v.get("enum")) {
        (UnitKind::Enum, Some(e)) => e
            .as_arr()
            .ok_or(BAD)?
            .iter()
            .map(|n| n.as_str().map(str::to_string).ok_or(BAD))
            .collect::<Result<Vec<_>, _>>()?,
        (UnitKind::Enum, None) | (_, Some(_)) => return Err(BAD),
        (_, None) => Vec::new(),
    };
    let f = OwnedField {
        name: text(v, "name")?,
        unit,
        enum_names,
        min: int(v, "min")?,
        max: int(v, "max")?,
        default: int(v, "default")?,
        per_group: boolean(v, "per_group")?,
        structural: boolean(v, "structural")?,
        manual_only: boolean(v, "manual_only")?,
        prod_min: v.get("prod_min").map(|_| int(v, "prod_min")).transpose()?,
        relax: v.get("relax").and_then(Json::as_str).and_then(relax).ok_or(BAD)?,
        minor_change: v
            .get("minor_change")
            .and_then(Json::as_str)
            .and_then(minor)
            .ok_or(BAD)?,
        since: int(v, "since")?,
        doc: text(v, "doc")?,
    };
    let range_ok = f.min <= f.default && f.default <= f.max;
    let prod_ok = f.prod_min.is_none_or(|m| f.min <= m && m <= f.max);
    let enum_ok = f.unit != UnitKind::Enum
        || usize::try_from(f.default).is_ok_and(|d| d < f.enum_names.len()) && !f.enum_names.is_empty();
    let asset_ok = !matches!(f.unit, UnitKind::Qty | UnitKind::Px) || f.per_group;
    let minor_ok = match f.minor_change {
        MinorChange::None | MinorChange::Any => true,
        MinorChange::RelPct(n) => !f.structural && (1..=100).contains(&n),
        MinorChange::Abs(_) => !f.structural,
    } && (!matches!(f.unit, UnitKind::Bool | UnitKind::Enum)
        || matches!(f.minor_change, MinorChange::None | MinorChange::Any))
        && (!f.structural || f.minor_change == MinorChange::None);
    if f.name.is_empty() || !range_ok || !prod_ok || !enum_ok || !asset_ok || !minor_ok {
        return Err(BAD);
    }
    Ok(f)
}

fn unique_names(fields: &[OwnedField]) -> bool {
    fields
        .iter()
        .enumerate()
        .all(|(i, f)| fields.iter().skip(i + 1).all(|g| g.name != f.name))
}

impl OwnedSchema {
    /// Restores a schema from canonical schema JSON. Rejected (`Err`): malformed JSON, a missing
    /// or mistyped key, an unknown unit/relax/minor_change text, `ix` out of sequence, duplicate
    /// names, `min ≤ default ≤ max` violated, an illegal enum default, `Qty`/`Px` without
    /// `per_group`, an illegal `minor_change` for the field, `since` decreasing, 0, or above
    /// `schema_version`, more than 256 fields; command fields
    /// additionally `per_group`/`structural` true, `minor_change ≠ none` or more than 31.
    /// `notes`, `report`, `sections` and `display` are not read.
    pub fn from_json(s: &str) -> Result<OwnedSchema, ParamError> {
        let v = json::parse(s).ok_or(BAD)?;
        let version: u16 = int(&v, "schema_version")?;
        let fields_json = v.get("fields").and_then(Json::as_arr).ok_or(BAD)?;
        if fields_json.len() > MAX_FIELDS {
            return Err(BAD);
        }
        let fields = fields_json
            .iter()
            .enumerate()
            .map(|(i, f)| field(f, i))
            .collect::<Result<Vec<_>, _>>()?;
        let since_ok = fields.iter().all(|f| (1..=version).contains(&f.since))
            && fields
                .windows(2)
                .all(|w| w.first().map(|a| a.since) <= w.get(1).map(|b| b.since))
            && version > 0;
        if !since_ok || !unique_names(&fields) {
            return Err(BAD);
        }
        let mut cmds = Vec::new();
        for c in v.get("cmds").and_then(Json::as_arr).ok_or(BAD)? {
            let fj = c.get("fields").and_then(Json::as_arr).ok_or(BAD)?;
            if fj.len() > MAX_CMD_FIELDS {
                return Err(BAD);
            }
            let cf = fj
                .iter()
                .enumerate()
                .map(|(i, f)| field(f, i))
                .collect::<Result<Vec<_>, _>>()?;
            if cf.iter().any(|f| {
                f.per_group || f.structural || f.prod_min.is_some() || f.minor_change != MinorChange::None
            }) || !unique_names(&cf)
            {
                return Err(BAD);
            }
            let op: u8 = int(c, "op")?;
            let name = text(c, "name")?;
            if op == 0 || cmds.iter().any(|o: &OwnedCmd| o.op == op || o.name == name) {
                return Err(BAD);
            }
            cmds.push(OwnedCmd {
                op,
                name,
                reduce_only: boolean(c, "reduce_only")?,
                fields: cf,
            });
        }
        Ok(OwnedSchema {
            name: text(&v, "name")?,
            kind_id: int(&v, "kind_id")?,
            id: int(&v, "schema_id")?,
            version,
            fields,
            cmds,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn kind_id(&self) -> u16 {
        self.kind_id
    }
    pub fn fields(&self) -> &[OwnedField] {
        &self.fields
    }
    /// Command descriptors (`decode_cmd(s.cmds(), payload)`, `encode_cmd(&s.cmds()[i], vals)`).
    pub fn cmds(&self) -> &[OwnedCmd] {
        &self.cmds
    }
    /// = `params::schema_digest` of the strategy the JSON was exported from.
    pub fn schema_digest(&self, ver: u16) -> u64 {
        digest_of(&self.name, self.kind_id, self, ver)
    }
}

impl SchemaView for OwnedSchema {
    fn id(&self) -> u32 {
        self.id
    }
    fn version(&self) -> u16 {
        self.version
    }
    fn n_fields(&self) -> u16 {
        u16::try_from(self.fields.len()).unwrap_or(u16::MAX)
    }
    fn field(&self, ix: u16) -> Option<FieldRef<'_>> {
        self.fields.get(usize::from(ix)).map(|f| f.as_ref(ix))
    }
}
