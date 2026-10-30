//! Parameter variants (README "参数变体"): a name and raw schema values, applied after boot as a
//! `ConfigBlob` + `SetParams` pair built from the strategy's active parameter blob.

use hft_event::{BlobKind, BlobRef, StrategyParamsHdr};
use hft_strategy_sdk::StrategySet;
use hft_strategy_sdk::params::{SchemaView, decode_body, encode_body, field_ix, schema_digest};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use zerocopy::{IntoBytes, TryFromBytes};

/// One parameter variant: `sets` are `(field name, raw schema value)`, applied in order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Variant {
    pub name: String,
    #[serde(default)]
    pub set: std::collections::BTreeMap<String, i64>,
}

#[derive(Deserialize)]
struct VariantFile {
    variant: Vec<Variant>,
}

/// `NAME=RAW[,NAME=RAW…]` (raw schema units, as `pm_sim_config --param`).
pub fn parse_sets(s: &str) -> Result<std::collections::BTreeMap<String, i64>, String> {
    let mut m = std::collections::BTreeMap::new();
    for kv in s.split(',').map(str::trim).filter(|x| !x.is_empty()) {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| format!("--set {kv}: expected NAME=RAW"))?;
        let v: i64 = v
            .trim()
            .parse()
            .map_err(|_| format!("--set {kv}: RAW must be an integer (schema raw units)"))?;
        m.insert(k.trim().to_string(), v);
    }
    Ok(m)
}

/// A TOML file of `[[variant]] name = "…" set = { field = raw, … }`; names must be unique.
pub fn parse_file(text: &str) -> Result<Vec<Variant>, String> {
    let f: VariantFile = toml::from_str(text).map_err(|e| format!("variants: {e}"))?;
    let mut seen = std::collections::BTreeSet::new();
    for v in &f.variant {
        if v.name.is_empty() || !seen.insert(v.name.clone()) {
            return Err(format!("variants: empty or duplicate name {:?}", v.name));
        }
    }
    Ok(f.variant)
}

/// The parameter names of the strategy kind of `base` (an SPB1 StrategyParams blob).
pub fn fields_of<S: StrategySet>(base: &[u8]) -> Result<Vec<&'static str>, String> {
    let (h, _) = StrategyParamsHdr::try_read_from_prefix(base).map_err(|_| "params header".to_string())?;
    let desc = S::kinds()
        .iter()
        .find(|d| d.kind_id == h.kind_id)
        .ok_or_else(|| format!("strategy kind {} is not registered", h.kind_id))?;
    let s = desc.schema;
    Ok((0..s.n_fields())
        .filter_map(|i| s.field(i).map(|f| f.name))
        .collect())
}

/// The variant's blob: `base` (an SPB1 StrategyParams blob) decoded at its schema version,
/// the `sets` applied (unknown field or a value outside `[min, max]` → error), re-encoded at the
/// running schema version with `version + 1`; the reference keeps `base_ref`'s encoding.
/// Returns the blob and its reference.
pub fn variant_blob<S: StrategySet>(
    base: &[u8],
    base_ref: &BlobRef,
    v: &Variant,
) -> Result<(Vec<u8>, BlobRef), String> {
    let (mut h, _) =
        StrategyParamsHdr::try_read_from_prefix(base).map_err(|_| "params header".to_string())?;
    let desc = S::kinds()
        .iter()
        .find(|d| d.kind_id == h.kind_id)
        .ok_or_else(|| format!("strategy kind {} is not registered", h.kind_id))?;
    let schema = desc.schema;
    let body_start = base
        .len()
        .checked_sub(h.body_len as usize)
        .ok_or("params body length")?;
    let mut raw = decode_body(schema, h.schema_ver, h.n_groups, &base[body_start..])
        .map_err(|e| format!("params body: {e:?}"))?;
    for (name, value) in &v.set {
        let ix = field_ix(schema, name).ok_or_else(|| {
            let names: Vec<&str> = (0..schema.n_fields())
                .filter_map(|i| schema.field(i).map(|f| f.name))
                .collect();
            format!(
                "{}: unknown parameter {name} of {} (fields: {})",
                v.name,
                desc.name,
                names.join(", ")
            )
        })?;
        let f = schema
            .field(ix)
            .ok_or_else(|| format!("{}: unknown parameter {name}", v.name))?;
        if *value < f.min || *value > f.max {
            return Err(format!(
                "{}: {name}={value} outside [{}, {}]",
                v.name, f.min, f.max
            ));
        }
        if let Some(d) = raw.defaults.get_mut(usize::from(ix)) {
            *d = *value;
        }
    }
    let body = encode_body(schema, h.n_groups, &raw).map_err(|e| format!("{}: {e:?}", v.name))?;
    let hl = h.as_bytes().len();
    h.version = h.version.wrapping_add(1);
    h.schema_ver = schema.version();
    h.schema_digest = schema_digest(desc, h.schema_ver);
    h.body_len = u32::try_from(body.len()).map_err(|_| "params body length")?;
    let mut out = h.as_bytes().to_vec();
    out.extend_from_slice(&base[hl..body_start]);
    out.extend_from_slice(&body);
    let sha: [u8; 32] = Sha256::digest(&out).into();
    let r = BlobRef {
        kind: BlobKind::StrategyParams,
        encoding: base_ref.encoding,
        len: u32::try_from(out.len()).map_err(|_| "params blob length")?,
        version: h.version,
        sha256: sha,
    };
    Ok((out, r))
}

/// Hex of a SHA-256.
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
