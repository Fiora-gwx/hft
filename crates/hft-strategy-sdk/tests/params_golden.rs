//! Parameter encoding goldens `fixtures/params/sdk-tp/<case>.{input.toml,blob,meta.toml}`
//! (SDK-120): the hft-cfg-shaped input is turned into `RawParams` with `field_ix` +
//! `parse_value`, encoded with `encode_body` inside the SPB1 container and compared byte for
//! byte with the hand-computed `.blob`; `decode_body` of the blob's body gives the input back.

mod common;

use std::path::PathBuf;

use common::*;
use hft_event::{GroupSpec, LegDef, ParamSource, StrategyParamsHdr, parse_spb1};
use hft_strategy_sdk::params::*;
use hft_types::{AccountId, AssetExp, AssetId, GroupId, InstId, StrategyId, VenueId};
use zerocopy::IntoBytes;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/params/sdk-tp")
}

/// The refdata names used by the inputs (GT3 of TESTS.md).
fn instance(name: &str) -> StrategyId {
    assert_eq!(name, "sdk-probe");
    StrategyId(3)
}
fn account(name: &str) -> AccountId {
    assert_eq!(name, "sdk-a1");
    AccountId(1)
}
fn asset(name: &str) -> (AssetId, AssetExp) {
    assert_eq!(name, "BTC");
    (AssetId(1), BTC)
}
fn inst(name: &str) -> (InstId, VenueId) {
    match name {
        "v1:BTC-SPOT" => (InstId(101), VenueId(1)),
        "v2:BTC-PERP" => (InstId(102), VenueId(2)),
        _ => panic!("unknown inst {name}"),
    }
}

struct Input {
    ver: u16,
    strategy: StrategyId,
    account: AccountId,
    version: u64,
    source: ParamSource,
    groups: Vec<GroupSpec>,
    raw: RawParams,
}

fn text(v: &toml::Value) -> &str {
    v.as_str().expect("values are text")
}

/// hft-cfg TOML → container fields and `RawParams` (schema `s`, missing defaults = schema default).
fn read_input(t: &toml::Table, s: &ParamSchemaDesc) -> Result<Input, ParamError> {
    let st = t["strategy"].as_table().unwrap();
    let ver = st["schema"]["version"].as_integer().unwrap() as u16;
    let mut defaults: Vec<i64> = s.fields.iter().map(|f| f.default).collect();
    for (k, v) in t["defaults"].as_table().unwrap() {
        let ix = field_ix(s, k).expect("known field");
        defaults[usize::from(ix)] = parse_value(s.field(ix).unwrap(), text(v), None)?;
    }
    let mut groups = Vec::new();
    let mut overrides = Vec::new();
    for (gix, g) in t["groups"].as_array().unwrap().iter().enumerate() {
        let (aid, exp) = asset(g["asset"].as_str().unwrap());
        let mut legs = [LegDef {
            inst: InstId(0),
            venue: VenueId(0),
            kind: 0,
            _pad: 0,
            route: 0,
            _pad2: 0,
        }; 4];
        let lj = g["legs"].as_array().unwrap();
        for (l, j) in legs.iter_mut().zip(lj) {
            let (i, v) = inst(j["inst"].as_str().unwrap());
            *l = LegDef {
                inst: i,
                venue: v,
                kind: LegDef::CEX,
                ..*l
            };
        }
        groups.push(GroupSpec {
            id: GroupId::compose(
                instance(st["instance"].as_str().unwrap()),
                g["n"].as_integer().unwrap() as u32,
            )
            .unwrap(),
            asset: aid,
            n_legs: lj.len() as u8,
            open_allowed: u8::from(g["open_allowed"].as_bool().unwrap()),
            legs,
            _rsv: 0,
        });
        if let Some(ov) = g.get("overrides").and_then(toml::Value::as_table) {
            for (k, v) in ov {
                let ix = field_ix(s, k).expect("known field");
                let val = parse_value(s.field(ix).unwrap(), text(v), Some(exp))?;
                overrides.push(ParamOverride::new(gix as u16, ix, val));
            }
        }
    }
    Ok(Input {
        ver,
        strategy: instance(st["instance"].as_str().unwrap()),
        account: account(st["account"].as_str().unwrap()),
        version: st["version"].as_integer().unwrap() as u64,
        source: match st["source"].as_str().unwrap() {
            "manual" => ParamSource::Manual,
            _ => ParamSource::Pipeline,
        },
        groups,
        raw: RawParams { defaults, overrides },
    })
}

/// Full SPB1 blob: header, `GroupSpec`s, body.
fn assemble(inp: &Input, body: &[u8]) -> Vec<u8> {
    let hdr = StrategyParamsHdr {
        strategy: inp.strategy,
        account: inp.account,
        kind_id: EchoProbe::DESC.kind_id,
        schema_id: TP.id,
        schema_ver: inp.ver,
        source: inp.source,
        _pad: 0,
        n_groups: inp.groups.len() as u16,
        _pad2: 0,
        version: inp.version,
        generated_at_ns: 0,
        body_len: body.len() as u32,
        group_ctr_hwm: inp.groups.iter().map(|g| g.id.0 & 0xFF_FFFF).max().unwrap_or(0),
        schema_digest: schema_digest(&EchoProbe::DESC, inp.ver),
    };
    let mut b = hdr.as_bytes().to_vec();
    for g in &inp.groups {
        b.extend_from_slice(g.as_bytes());
    }
    b.extend_from_slice(body);
    b
}

fn run_case(name: &str) {
    let path = |ext: &str| dir().join(format!("{name}.{ext}"));
    let meta: toml::Table = std::fs::read_to_string(path("meta.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let input: toml::Table = std::fs::read_to_string(path("input.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let ver = input["strategy"]["schema"]["version"].as_integer().unwrap() as u16;
    assert_eq!(
        meta["schema_version"].as_integer(),
        Some(i64::from(ver)),
        "{name}: meta schema_version"
    );
    // The encoder that wrote a version-`ver` body is the schema as it was at `ver`.
    let s: &ParamSchemaDesc = if ver == 1 { &TP_V1 } else { &TP };
    let expect = meta["expect"].as_str().unwrap();
    let encoded = read_input(&input, s).and_then(|inp| {
        let body = encode_body(s, inp.groups.len() as u16, &inp.raw)?;
        Ok((inp, body))
    });
    if expect != "Ok" {
        let err = encoded
            .err()
            .unwrap_or_else(|| panic!("{name}: expected {expect}"));
        assert_eq!(
            format!("ParamError::{err:?}").split([' ', '{']).next(),
            Some(expect),
            "{name}"
        );
        assert!(!path("blob").exists(), "{name}: error cases have no blob");
        return;
    }
    let (inp, body) = encoded.unwrap_or_else(|e| panic!("{name}: {e:?}"));
    let want = std::fs::read(path("blob")).unwrap();
    assert_eq!(assemble(&inp, &body), want, "{name}: blob bytes");

    let al = aligned(&want);
    let spb = parse_spb1(bytes_of(&al, want.len())).unwrap();
    assert_eq!(spb.hdr.schema_ver, ver, "{name}");
    let back = decode_body(&TP, ver, spb.hdr.n_groups, spb.body).unwrap();
    let mut sorted = inp.raw.overrides.clone();
    sorted.sort_by_key(|o| (o.group_ix, o.field_ix));
    let mut defaults = inp.raw.defaults.clone();
    defaults.extend(TP.fields[defaults.len()..].iter().map(|f| f.default));
    assert_eq!(
        back,
        RawParams {
            defaults,
            overrides: sorted
        },
        "{name}: decode round trip"
    );
}

#[test]
fn sdk_120_params_golden() {
    let mut cases: Vec<String> = std::fs::read_dir(dir())
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().into_string().unwrap();
            n.strip_suffix(".input.toml").map(str::to_string)
        })
        .collect();
    cases.sort();
    assert!(cases.len() >= 4, "cases: {cases:?}");
    for c in &cases {
        run_case(c);
    }
}
