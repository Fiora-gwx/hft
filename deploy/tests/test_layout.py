"""Executable archive and local host checks; privileged measurements remain host evidence."""
import ast
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

import pytest
from test_release import load, release, preflight_host

REPO = Path(__file__).resolve().parents[2]


def dep_001_ci_stage_to_install(tmp_path):
    stager = load("stage")
    repo, dist = tmp_path / "repo", tmp_path / "dist"
    for path in ("deploy", "schemas", "apps/hft-core/config"):
        stager.copy_tree(REPO / path, repo / path)
    stager.copy_tree(REPO / ".cargo", repo / ".cargo")
    shutil.copyfile(REPO / "Cargo.lock", repo / "Cargo.lock")
    stager.copy_tree(REPO / "fixtures/strategy-schemas", dist / "schemas/strategies")
    (dist / "python/wheelhouse").mkdir(parents=True)
    (dist / "python/wheels").mkdir()
    (dist / "python/wheels/hft_example-1.0-py3-none-any.whl").write_bytes(b"fixture")
    (dist / "python/requirements.lock").write_text("")
    elf = bytearray(64)
    elf[:6], elf[18:20] = b"\x7fELF\x02\x01", (183).to_bytes(2, "little")
    names = ["hft-core", "hostprobe", "hft-cfg", "hft-secrets"]
    for name in names:
        path = repo / "target/aarch64-unknown-linux-gnu/release" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(elf)
    metadata = dict(workspace_members=names, packages=[dict(id=n, name=n, targets=[dict(name=n, kind=["bin"])]) for n in names])
    def run(argv, **_kwargs):
        if argv[0] == "cargo":
            return json.dumps(metadata).encode()
        if argv[0] == "rustc":
            if "cfg" in argv:
                return b'target_feature="aes"\ntarget_feature="neon"\ntarget_feature="sve"\n'
            return b"rustc fixture"
        if argv[0] == "cat":
            return b"Features : fp aes pmull asimd sve sm3 sm4 svesm4\n"
        return b"kind_id=1 portfolio-carry version=1\nkind_id=3 cross-taker version=1\nkind_id=5 latency-probe version=1\nkind_id=6 book-observer version=1\n"
    tree, info = stager.stage(repo, dist, "v1.2.3-dev.4", "a" * 40, "123", dict(ci_run_id=122, decoupling_check_run_id=121), run)
    archive = load("package").package_tree(tree, tmp_path / "packed", info, 0, compress=False)
    extracted = tmp_path / "unpacked"
    extracted.mkdir()
    actual = release.verify_tree(release.unpack(archive, extracted))
    assert actual["profiles"] == {n: "release" for n in names}
    assert actual["cpu_features"] == ["aes", "asimd", "fp", "pmull", "sve"]
    assert actual["target_features"] == "-sm4,-rand,-dit"
    assert len(actual["strategies"]) == 3
    assert actual["evidence"]["ci_run_id"] == 122
    assert actual["sbom_sha256"] == release.sha256(tree / "sbom.json")
    for path in ("bin/hft-core", "bin/hostprobe", "bin/hft-secrets", "deploy/dev2/hft-window", "deploy/release/preflight.py", "deploy/generated/test-dev2-prod/install.json", "schemas/strategies/index.json", "config-templates/dual-testnet.toml"):
        assert path in {p["path"] for p in actual["files"]}
    assert "--hash=sha256:" in (tree / "python/requirements.lock").read_text()
    assert (tree / "python/wheelhouse/hft_example-1.0-py3-none-any.whl").is_file()
    with pytest.raises(ValueError, match="already exists"):
        stager.stage(repo, dist, "v1.2.3-dev.4", "a" * 40, "123", info["evidence"], run)


def dep_042_shellcheck():
    scripts = [p for p in (REPO / "deploy").rglob("*") if p.is_file() and p.read_bytes().startswith(b"#!/usr/bin/env bash")]
    assert scripts
    for path in scripts:
        text = path.read_text().replace("\r", "")
        statement = next(line for line in text.splitlines() if line.strip() and not line.startswith("#"))
        assert statement == "set -euo pipefail", path
        if shutil.which("shellcheck"):
            subprocess.run(["shellcheck", "-S", "warning", "-"], input=text, text=True, check=True, timeout=30)
        else:
            assert os.environ.get("CI") != "true", "shellcheck must be installed in CI"


def dep_251_stdlib_only():
    # Follow local helper modules too; a stdlib-only wrapper proves nothing about its imports.
    paths = [p for directory in ("dev2", "bin", "imds", "release", "gen") for p in (REPO / "deploy" / directory).glob("*") if p.is_file() and (p.suffix == ".py" or p.read_bytes().startswith(b"#!/usr/bin/python3"))]
    local = {p.stem for p in paths}
    for path in paths:
        for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"))):
            modules = [n.name for n in node.names] if isinstance(node, ast.Import) else [node.module] if isinstance(node, ast.ImportFrom) and node.module else []
            assert all(m.split(".")[0] in sys.stdlib_module_names | local for m in modules), path


@pytest.mark.parametrize("field,value,failed", [
    ("cgroup.subtree_control", "cpu memory", "cgroup-controllers"),
    ("hftbatch.slice/cpu.idle", "0", "batch-cpu-limit"),
    ("hftbatch.slice/cpu.max", "100000 100000", "batch-cpu-limit"),
    ("hftbatch.slice/memory.max", "max", "memory:hftbatch.slice"),
    ("hftbatch.slice/memory.swap.max", "4096", "memory:hftbatch.slice"),
    ("hft.slice/memory.max", "max", "memory:hft.slice"),
    ("hftbatch.slice/io.max", "259:2 rbps=40000000 wbps=20000000", "batch-io-limit"),
    ("hftbatch.slice/io.max", "259:1 rbps=max wbps=20000000", "batch-io-limit"),
])
def test_resource_limits_fail_closed(tmp_path, monkeypatch, field, value, failed):
    preflight, host = preflight_host(tmp_path, monkeypatch)
    assert all(preflight.check(host).values())
    host.path("/sys/fs/cgroup/" + field).write_text(value)
    assert not preflight.check(host)[failed]
