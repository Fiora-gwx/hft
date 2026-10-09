"""hft_py.params: schema loading, encode/decode, commands, value text (TESTS.md PY-120–PY-134)."""

from __future__ import annotations

import json
import random
import shutil
import struct
from decimal import Decimal
from pathlib import Path

import pytest

import hft_py
from conftest import D, REPO

P = hft_py.params
SDK_TP = REPO / "fixtures" / "params" / "sdk-tp"
GOLDEN = SDK_TP / "schema.golden.json"
LEGS = [{"inst": 101, "venue": 2, "kind": "Cex"}, {"inst": 102, "venue": 5, "kind": "Cex"}]


def _gt3(n: int = 3, exps: bool = True) -> list[dict]:
    out = []
    for i in range(n):
        g = {"id": 0x0300_0001 + i, "asset": 1, "open_allowed": True, "legs": LEGS}
        if exps:
            g.update(qty_exp=-8, px_exp=-2)
        out.append(g)
    return out


HDR = {"strategy": 3, "account": 9, "version": 7, "source": "Pipeline", "generated_at_ns": D}


def _write_dir(dst: Path, extra_target: bool = False) -> Path:
    """``S_TP`` (and ``S_TP2`` with ``extra_target``) schema directory."""
    dst.mkdir(parents=True, exist_ok=True)
    s = P.load_schema(str(GOLDEN))
    shutil.copy(GOLDEN, dst / f"65281-{s.name}.schema.json")
    idx = [{"kind_id": 65281, "name": s.name, "file": f"65281-{s.name}.schema.json", "schema_id": 65281, "schema_version": 2, "schema_digest": s.schema_digest}]
    if extra_target:
        t = json.loads(GOLDEN.read_text())
        t.update(name="target_probe", kind_id=65282, schema_id=65282, cmds=[], notes=[], report=[])
        (dst / "65282-target_probe.schema.json").write_text(json.dumps(t))
        ts = P.load_schema(str(dst / "65282-target_probe.schema.json"))
        idx.append({"kind_id": 65282, "name": "target_probe", "file": "65282-target_probe.schema.json", "schema_id": 65282, "schema_version": 2, "schema_digest": ts.schema_digest})
    (dst / "index.json").write_text(json.dumps(idx))
    return dst


@pytest.fixture()
def s_tp(tmp_path: Path) -> Path:
    return _write_dir(tmp_path / "s_tp")


@pytest.fixture()
def s_tp2(tmp_path: Path) -> Path:
    return _write_dir(tmp_path / "s_tp2", extra_target=True)


def _encode_ok(s: object, **kw: object) -> bytes:
    args = dict(header=HDR, groups=_gt3(), defaults={"open_th": "0.6%"}, overrides=[{"group_ix": 1, "field": "max_qty", "value": "0.5"}])
    args.update(kw)
    blob, errs = P.encode(s, **args)
    assert errs == [] and blob is not None, errs
    return blob


def test_py_120_load_schemas_golden() -> None:
    d = REPO / "fixtures" / "strategy-schemas"
    m = P.load_schemas(str(d))
    idx = json.loads((d / "index.json").read_text())
    assert list(m.keys()) == sorted(e["kind_id"] for e in idx)
    for e in idx:
        s = m[e["kind_id"]]
        assert (s.kind_id, s.name, s.schema_id, s.schema_version, s.schema_digest) == (
            e["kind_id"], e["name"], e["schema_id"], e["schema_version"], e["schema_digest"],
        )
        names = [f["name"] for f in json.loads((d / e["file"]).read_text())["fields"]]
        assert [f["name"] for f in s.fields] == names


@pytest.mark.parametrize("row", [f"PY-121.{i}" for i in range(1, 8)])
def test_py_121_load_schemas_errors(tmp_path: Path, row: str) -> None:
    d = _write_dir(tmp_path / row)
    idx = d / "index.json"
    data = json.loads(idx.read_text())
    sfile = d / data[0]["file"]
    if row == "PY-121.1":
        idx.write_text(json.dumps({"items": data}))
    elif row == "PY-121.2":
        sfile.unlink()
    elif row == "PY-121.3":
        data[0]["schema_id"] = 65282
        idx.write_text(json.dumps(data))
    elif row == "PY-121.4":
        dg = data[0]["schema_digest"]
        data[0]["schema_digest"] = dg[:-1] + ("0" if dg[-1] != "0" else "1")
        idx.write_text(json.dumps(data))
    elif row == "PY-121.5":
        idx.unlink()
    elif row == "PY-121.6":
        idx.write_text(idx.read_text().rstrip().rstrip("]"))
    if row == "PY-121.7":
        assert 65281 in P.load_schemas(str(d))
    else:
        with pytest.raises(hft_py.ParamsError):
            P.load_schemas(str(d))


def test_py_122_load_schema_single(s_tp: Path, tmp_path: Path) -> None:
    f = next(s_tp.glob("65281-*.schema.json"))
    s = P.load_schema(str(f))
    assert s.kind_id == 65281 and s.schema_version == 2
    assert [x["name"] for x in s.fields] == ["open_th", "cooldown_ms", "pct_bin", "max_qty", "enabled", "mode"]
    notjson = tmp_path / "x.json"
    notjson.write_text("not json")
    nofields = tmp_path / "y.json"
    j = json.loads(f.read_text())
    del j["fields"]
    nofields.write_text(json.dumps(j))
    for p in (notjson, nofields, tmp_path / "missing.json"):
        with pytest.raises(hft_py.ParamsError):
            P.load_schema(str(p))


def _hdr_fields(blob: bytes) -> dict:
    keys = ["strategy", "account", "kind_id", "schema_id", "schema_ver", "source", "_pad", "n_groups", "_pad2", "version", "generated_at_ns", "body_len", "group_ctr_hwm", "schema_digest"]
    return dict(zip(keys, struct.unpack_from("<BBHIHBBHHQqIIQ", blob, 0)))


def test_py_123_encode_ok(s_tp: Path) -> None:
    s = P.load_schemas(str(s_tp))[65281]
    blob = _encode_ok(s)
    h = _hdr_fields(blob)
    assert (h["strategy"], h["account"], h["kind_id"], h["schema_id"], h["schema_ver"]) == (3, 9, 65281, 65281, 2)
    assert (h["source"], h["n_groups"], h["version"], h["generated_at_ns"]) == (1, 3, 7, D)
    assert f"{h['schema_digest']:016x}" == json.loads((s_tp / "index.json").read_text())[0]["schema_digest"]
    body = blob[48 + 80 * 3 :]
    n_fields, _, n_ov = struct.unpack_from("<HHI", body, 0)
    defaults = struct.unpack_from(f"<{n_fields}q", body, 8)
    want = [f["default"] for f in s.fields]
    want[0] = 6_000_000
    assert list(defaults) == want
    assert n_ov == 1
    assert struct.unpack_from("<HHIq", body, 8 + 8 * n_fields) == (1, 3, 0, 50_000_000)


@pytest.mark.parametrize(
    ("row", "kw", "want"),
    [
        ("PY-124.1", {"defaults": {"open_th": "10.0000001%"}}, [("OutOfRange", "open_th", None)]),
        ("PY-124.2", {"defaults": {"open_th": "10%"}}, []),
        ("PY-124.3", {"overrides": [{"group_ix": 0, "field": "cooldown_ms", "value": "5000"}]}, [("NotPerGroup", "cooldown_ms", 0)]),
        ("PY-124.4", {"defaults": {"mode": "z"}}, [("BadEnum", "mode", None)]),
        ("PY-124.5", {"groups": _gt3(exps=False)}, [("BadText", "max_qty", 1)]),
        ("PY-124.6", {"defaults": {"nope": "1"}}, [("UnknownField", "nope", None)]),
        ("PY-124.7", {"overrides": [{"group_ix": 3, "field": "open_th", "value": "1%"}]}, [("GroupIxOutOfRange", "open_th", 3)]),
        ("PY-124.8", {"overrides": [{"group_ix": 0, "field": "open_th", "value": "1%"}, {"group_ix": 0, "field": "open_th", "value": "2%"}]}, [("OverrideOrder", "open_th", 0)]),
        ("PY-124.9", {"defaults": {"open_th": "10.0000001%", "nope": "1"}}, [("UnknownField", "nope", None), ("OutOfRange", "open_th", None)]),
    ],
)
def test_py_124_encode_errors(s_tp: Path, row: str, kw: dict, want: list[tuple]) -> None:
    s = P.load_schemas(str(s_tp))[65281]
    args = dict(header=HDR, groups=_gt3(), defaults={"open_th": "0.6%"}, overrides=[{"group_ix": 1, "field": "max_qty", "value": "0.5"}])
    args.update(kw)
    blob, errs = P.encode(s, **args)
    got = [(e.variant, e.field, e.group_ix) for e in errs]
    if not want:
        assert blob is not None and errs == [], row
    else:
        assert blob is None, row
        assert sorted(got, key=str) == sorted(want, key=str), (row, got)


def test_py_125_value_types(s_tp: Path) -> None:
    s = P.load_schemas(str(s_tp))[65281]
    with pytest.raises(TypeError):
        P.encode(s, header=HDR, groups=_gt3(), defaults={"open_th": 0.006})
    b2 = _encode_ok(s, defaults={"open_th": Decimal("0.006")})
    b3 = _encode_ok(s, defaults={"open_th": 6_000_000})
    assert b2 == b3
    body = b2[48 + 80 * 3 :]
    assert struct.unpack_from("<q", body, 8)[0] == 6_000_000
    with pytest.raises(TypeError):
        P.encode_cmd(s, "open", {"notional": 1000.0})
    assert P.encode_cmd(s, "open", {"notional": Decimal("1000")}) == P.encode_cmd(s, "open", {"notional": "1000"})


def _random_x(rng: random.Random, s: object) -> dict:
    fields = list(s.fields)  # type: ignore[attr-defined]
    n = rng.randint(0, 8)
    groups = [{"id": 0x0300_0001 + i, "asset": 1, "open_allowed": rng.random() < 0.5, "legs": LEGS} for i in range(n)]
    defaults = {f["name"]: rng.choice([f["min"], f["max"], rng.randint(f["min"], f["max"])]) for f in fields}
    per_group = [f for f in fields if f["per_group"]]
    keys = {(g, f["name"]) for g in range(n) for f in per_group}
    picked = rng.sample(sorted(keys), min(len(keys), rng.randint(0, 12)))
    by_name = {f["name"]: f for f in fields}
    overrides = [{"group_ix": g, "field": name, "value": rng.randint(by_name[name]["min"], by_name[name]["max"])} for g, name in picked]
    header = {"strategy": 3, "account": 9, "version": rng.randint(1, 2**63 - 1), "source": rng.choice(["Pipeline", "Manual"]), "generated_at_ns": D}
    return {"header": header, "groups": groups, "defaults": defaults, "overrides": overrides}


def test_py_126_roundtrip(s_tp: Path) -> None:
    m = P.load_schemas(str(s_tp))
    s = m[65281]
    n = int(__import__("os").environ.get("HFT_PROP_CASES", "2000"))
    rng = random.Random(0x126)
    for _ in range(n):
        x = _random_x(rng, s)
        blob, errs = P.encode(s, **x)
        assert errs == [] and blob is not None, (x, errs)
        d = P.decode(m, blob)
        for k, v in x["header"].items():
            assert d["header"][k] == v
        assert d["groups"] == x["groups"]
        assert d["defaults"] == x["defaults"]
        key = lambda o: (o["group_ix"], o["field"])  # noqa: E731
        assert sorted(d["overrides"], key=key) == sorted(x["overrides"], key=key)


def test_py_127_decode_schema_selection(s_tp2: Path) -> None:
    m2 = P.load_schemas(str(s_tp2))
    b = _encode_ok(m2[65281])
    assert P.decode(m2, b)["header"]["kind_id"] == 65281, "PY-127.1"
    with pytest.raises(hft_py.ParamsError):
        P.decode({k: v for k, v in m2.items() if k != 65281}, b)
    assert P.decode(m2[65281], b) == P.decode(m2, b), "PY-127.3"
    with pytest.raises(hft_py.ParamsError):
        P.decode(m2[65282], b)
    b5 = bytearray(b)
    struct.pack_into("<I", b5, 4, 65282)
    with pytest.raises(hft_py.ParamsError):
        P.decode(m2, bytes(b5))


@pytest.mark.parametrize("row", [f"PY-128.{i}" for i in range(1, 7)])
def test_py_128_decode_errors(s_tp: Path, row: str) -> None:
    m = P.load_schemas(str(s_tp))
    b = bytearray(_encode_ok(m[65281]))
    if row == "PY-128.1":
        b = b[:40]
    elif row == "PY-128.2":
        struct.pack_into("<I", b, 32, struct.unpack_from("<I", b, 32)[0] + 4)
    elif row == "PY-128.3":
        struct.pack_into("<H", b, 8, 3)
    elif row == "PY-128.4":
        struct.pack_into("<H", b, 48 + 80 * 3, 5)
    elif row == "PY-128.5":
        b[48 + 72] = 1
    if row == "PY-128.6":
        assert P.decode(m, bytes(b))["header"]["kind_id"] == 65281
    else:
        with pytest.raises(hft_py.ParamsError):
            P.decode(m, bytes(b))


def test_py_129_old_body_defaults(s_tp: Path) -> None:
    m = P.load_schemas(str(s_tp))
    p = P.decode(m, (SDK_TP / "v1_old.blob").read_bytes())
    assert p["header"]["schema_ver"] == 1
    assert len(p["defaults"]) == 6 and p["defaults"]["mode"] == 0
    # v1_old.input.toml: open_th 0.6%, cooldown_ms 25s, pct_bin 0.01%, enabled false, max_qty unset.
    want = {"open_th": 6_000_000, "cooldown_ms": 25_000, "pct_bin": 100_000, "max_qty": 0, "enabled": 0}
    assert {k: p["defaults"][k] for k in want} == want


@pytest.mark.parametrize(
    ("row", "op", "values", "want"),
    [
        ("PY-130.1", "open", {"notional": "1000"}, bytes([1, 1, 1, 0, 0, 0, 0, 0]) + struct.pack("<q", 1_000_000_000)),
        ("PY-130.2", "close", {}, bytes([1, 2, 0, 0, 0, 0, 0, 0])),
        ("PY-130.3", "set_mode", {"m": "y"}, bytes([1, 3, 1, 0, 0, 0, 0, 0]) + struct.pack("<q", 1)),
        ("PY-130.4", "open", {"notional": 10_000_000_000}, bytes([1, 1, 1, 0, 0, 0, 0, 0]) + struct.pack("<q", 10_000_000_000)),
        ("PY-130.5", "open", {"notional": 0}, bytes([1, 1, 1, 0, 0, 0, 0, 0]) + struct.pack("<q", 0)),
    ],
)
def test_py_130_encode_cmd(s_tp: Path, row: str, op: str, values: dict, want: bytes) -> None:
    s = P.load_schemas(str(s_tp))[65281]
    assert P.encode_cmd(s, op, values) == want, row


@pytest.mark.parametrize(
    ("row", "which", "op", "values"),
    [
        ("PY-131.1", "s", "flip", {}),
        ("PY-131.2", "s", "open", {}),
        ("PY-131.3", "s", "close", {"x": "1"}),
        ("PY-131.4", "s", "open", {"notional": 10_000_000_001}),
        ("PY-131.5", "s", "open", {"notional": -1}),
        ("PY-131.6", "s", "set_mode", {"m": "z"}),
        ("PY-131.7", "t", "open", {"notional": "1"}),
    ],
)
def test_py_131_encode_cmd_errors(s_tp2: Path, row: str, which: str, op: str, values: dict) -> None:
    m = P.load_schemas(str(s_tp2))
    s = m[65281] if which == "s" else m[65282]
    with pytest.raises(hft_py.ParamsError):
        P.encode_cmd(s, op, values)


def test_py_133_descriptors_read_only(s_tp: Path) -> None:
    s = P.load_schemas(str(s_tp))[65281]
    notes = s.notes
    assert [(n["code"], n["name"], n["level"]) for n in notes] == [(1, "EchoFire", "info"), (2, "EchoOverflow", "p2"), (3, "EchoNoFeed", "p2")]
    rep = s.report
    assert len(rep) == 1 and (rep[0]["name"], rep[0]["agg"], rep[0]["scope"], rep[0]["note"], rep[0]["v_ix"]) == ("fires", "count", "group", 1, 0)
    cmds = s.cmds
    assert [(c["op"], c["name"]) for c in cmds] == [(1, "open"), (2, "close"), (3, "set_mode")]
    assert cmds[1]["reduce_only"] is True
    with pytest.raises(TypeError):
        s.notes[0]["name"] = "x"  # type: ignore[index]
    assert s.notes[0]["name"] == "EchoFire"


@pytest.mark.parametrize(
    ("row", "field", "text", "exp", "want"),
    [
        ("PY-134.1", "open_th", "50bp", None, 5_000_000),
        ("PY-134.2", "open_th", "0.5%", None, 5_000_000),
        ("PY-134.3", "open_th", "10%", None, 100_000_000),
        ("PY-134.4", "open_th", "10.0000001%", None, "OutOfRange"),
        ("PY-134.5", "open_th", "-1bp", None, "OutOfRange"),
        ("PY-134.6", "cooldown_ms", "5m", None, 300_000),
        ("PY-134.7", "cooldown_ms", "600001", None, "OutOfRange"),
        ("PY-134.8", "mode", "b", None, 1),
        ("PY-134.9", "enabled", "true", None, 1),
        ("PY-134.10", "max_qty", "0.001", -8, 100_000),
        ("PY-134.11", "max_qty", "0.001", None, "BadText"),
        ("PY-134.12", "open_th", "abc", None, "BadText"),
        ("PY-134.13", "nosuch", "1", None, "UnknownField"),
    ],
)
def test_py_134_parse_format_value(s_tp: Path, row: str, field: str, text: str, exp: int | None, want: object) -> None:
    s = P.load_schemas(str(s_tp))[65281]
    if isinstance(want, str):
        with pytest.raises(hft_py.ParamsError) as e:
            P.parse_value(s, field, text, exp=exp)
        assert e.value.variant == want, row
    else:
        assert P.parse_value(s, field, text, exp=exp) == want, row
    if row == "PY-134.1":
        t = P.format_value(s, "open_th", 5_000_000)
        assert t == "0.5%"
        assert P.parse_value(s, "open_th", t) == 5_000_000, "PY-134.14"
        assert P.format_value(s, "mode", 2) == "c", "PY-134.15"
