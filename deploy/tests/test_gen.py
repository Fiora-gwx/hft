"""Acceptance coverage for the executable prod/testnet subset of the host generator."""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

import pytest

REPO = Path(__file__).parents[2]
spec = importlib.util.spec_from_file_location("generator", REPO / "deploy/gen/generate.py")
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)


def generated():
    inventory = tomllib.loads((REPO / "deploy/hosts/test-dev2-prod.toml").read_text())
    return generator.render(inventory, (REPO / "apps/hft-core/README.md").read_text(encoding="utf-8"), (REPO / "deploy/dev2/hft-dev2-guard.service").read_text())


def dep_121_explicit_user_slice():
    files, _ = generated()
    for name, text in files.items():
        if name.endswith(".service"):
            assert "\nUser=" in text and "\nSlice=" in text
    core = files["hft-core@prod.service"]
    assert "User=hft\n" in core and "Slice=hft-core.slice\n" in core


def dep_122_runtime_dir_unique():
    files, _ = generated()
    directories = [line.split("=", 1)[1] for text in files.values() for line in text.splitlines() if line.startswith("RuntimeDirectory=")]
    assert len(directories) == len(set(directories))
    assert "hft/core-prod" in directories


def dep_123_no_hard_deps():
    files, _ = generated()
    for text in files.values():
        for line in text.splitlines():
            if line.startswith(("Requires=", "Requisite=", "BindsTo=", "PartOf=")):
                assert not any(x.startswith("hft-") and x.endswith(".service") for x in line.split("=", 1)[1].split())


def dep_124_restart_policy():
    files, _ = generated()
    core = files["hft-core@prod.service"].splitlines()
    assert {"Restart=on-failure", "RestartSec=1", "StartLimitBurst=3", "StartLimitIntervalSec=600", "RestartPreventExitStatus=2 3 4", "RestartForceExitStatus=75"} <= set(core)
    assert "Restart=always" in files["hft-dev2-guard.service"].splitlines()


def dep_125_core_template():
    files, _ = generated()
    lines = files["hft-core@prod.service"].splitlines()
    assert {"Type=notify", "WatchdogSec=10", "TimeoutStartSec=45", "TimeoutStopSec=20", "LimitMEMLOCK=infinity", "LimitNOFILE=65536", "RuntimeDirectoryMode=0751", "SupplementaryGroups=hft-ctl hft-stopmark"} <= set(lines)
    assert "[Install]" not in lines
    assert any(line.startswith("ExecStartPre=+") and "preflight.sh" in line for line in lines)
    assert "ExecStopPost=+/opt/hft/current-core/deploy/bin/hft-shm-release prod" in lines


def dep_247_dev2_profile():
    files, profile = generated()
    expected = {row["key"]: row for row in tomllib.loads(profile)["expect"]}
    assert all(row["level"] == "required" for row in expected.values())
    assert expected["guard.slices.hftbatch.slice.parent"]["eq"] == "/"
    assert expected["guard.slices.hftbatch.slice.cpu_idle"]["eq"] == 1
    assert expected["guard.slices.hftbatch.slice.cpu_max"]["eq"].startswith("max ")
    assert expected["guard.slices.hft.slice.cpu_weight"]["eq"] == 50
    assert expected["guard.slices.hft.slice.memory_max"]["eq"] == str(1376 * 1024 * 1024)
    assert expected["nic.egress[*].on_nic"]["eq"] is True
    assert "prod" in tomllib.loads(files["egress.toml"])


@pytest.mark.skipif(os.environ.get("CI") != "true" or os.name != "posix", reason="isolated hosted-CI network namespace")
def dep_247_real_nft_uid_isolation():
    # No host network mutation: unshare creates a fresh network namespace before nft runs.
    script = r'''
import json, pathlib, subprocess, sys
repo = pathlib.Path.cwd()
sys.path.insert(0, str(repo / "deploy/imds"))
sys.path.insert(0, str(repo / "deploy/release"))
from dev2_lock import rules
from preflight import valid_imds
subprocess.run(["nft", "add", "table", "inet", "fixture_other"], check=True)
for replace in (False, True):
    subprocess.run(["nft", "-f", "-"], input=rules({1001, 1002}, replace), text=True, check=True)
    actual = json.loads(subprocess.check_output(["nft", "-j", "list", "table", "inet", "hft_imds"]))
    assert valid_imds(actual, {1001, 1002}), actual
    assert not valid_imds(actual, {1001, 1002, 1003})
    tables = json.loads(subprocess.check_output(["nft", "-j", "list", "tables"]))
    assert any(row.get("table", {}).get("name") == "fixture_other" for row in tables["nftables"])
'''
    result = subprocess.run(["sudo", "-n", "unshare", "--net", "/usr/bin/python3", "-B", "-"], cwd=REPO, input=script, capture_output=True, text=True, timeout=30)
    assert result.returncode == 0, result.stdout + result.stderr


def dep_250_pure_generator(tmp_path, monkeypatch):
    for path in ("apps/hft-core/README.md", "deploy/dev2/hft-dev2-guard.service", "deploy/hosts/test-dev2-prod.toml"):
        target = tmp_path / path
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(REPO / path, target)
    monkeypatch.setenv("TZ", "UTC")
    first = generator.generate(tmp_path, "test-dev2-prod")
    monkeypatch.setenv("TZ", "Asia/Tokyo")
    monkeypatch.chdir(tmp_path)
    assert first == generator.generate(tmp_path, "test-dev2-prod", check=True)
    for path in first:
        assert path.read_bytes() == (REPO / path.relative_to(tmp_path)).read_bytes()
    output = next(iter(first))
    output.write_bytes(b"drift")
    with pytest.raises(ValueError, match="differ"):
        generator.generate(tmp_path, "test-dev2-prod", check=True)
