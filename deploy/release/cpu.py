#!/usr/bin/python3
"""Derive the userspace Linux feature envelope from Cargo's actual compiler flags."""
import json
from pathlib import Path
import re
import subprocess
import tomllib

TARGET = "aarch64-unknown-linux-gnu"
# Rust/LLVM feature names are not Linux HWCAP names. sm4 also enables SM3 in LLVM;
# it is deliberately absent from this contract. V2 does not implement either.
HWCAP = {
    "aes": "aes pmull", "bf16": "bf16 svebf16", "crc": "crc32",
    "dit": "dit", "dotprod": "asimddp", "dpb": "dcpop", "dpb2": "dcpodp",
    "fcma": "fcma", "fhm": "asimdfhm", "flagm": "flagm",
    "fp16": "fphp asimdhp", "i8mm": "i8mm svei8mm", "jsconv": "jscvt",
    "lse": "atomics", "neon": "fp asimd", "paca": "paca", "pacg": "pacg",
    "rand": "rng", "rcpc": "lrcpc", "rcpc2": "ilrcpc", "rdm": "asimdrdm",
    "sha2": "sha1 sha2", "sha3": "sha3 sha512", "sve": "sve",
}
# These target flags describe privileged/system facilities, not additional EL0 opcodes.
# SSBS uses a backwards-compatible architectural hint; it has no required HWCAP here.
SYSTEM_FEATURES = {"lor", "pan", "pmuv3", "ras", "spe", "ssbs", "vh"}


def envelope(repo, runner=subprocess.check_output):
    flags = tomllib.loads((repo / ".cargo/config.toml").read_text(encoding="utf-8"))["target"][TARGET]["rustflags"]
    expected = ["-C", "target-cpu=neoverse-v1", "-C", "target-feature=-sm4,-rand,-dit"]
    if flags != expected:
        raise ValueError("unreviewed aarch64 build flags")
    cfg = runner(["rustc", "--target", TARGET, "--print", "cfg", *flags], cwd=repo).decode()
    enabled = set(re.findall(r'^target_feature="([^"]+)"$', cfg, re.MULTILINE))
    if not enabled or enabled - HWCAP.keys() - SYSTEM_FEATURES:
        raise ValueError("unmapped compiler CPU feature")
    required = sorted({f for name in enabled for f in HWCAP.get(name, "").split()})
    return {"target_cpu": "neoverse-v1", "target_features": "-sm4,-rand,-dit",
            "rustflags": flags, "compiler_cpu_features": sorted(enabled), "cpu_features": required}


def compatible(manifest, cpuinfo):
    # Every exposed CPU must satisfy the envelope, not the union of heterogeneous CPUs.
    rows = [set(line.split(":", 1)[1].split()) for line in cpuinfo.splitlines()
            if line.split(":", 1)[0].strip() == "Features"]
    required = set(manifest["cpu_features"])
    return bool(required) and bool(rows) and all(required <= row for row in rows)


if __name__ == "__main__":
    repo = Path(__file__).resolve().parents[2]
    contract = envelope(repo)
    if not compatible(contract, Path("/proc/cpuinfo").read_text()):
        raise SystemExit("CI CPU does not satisfy compiled feature envelope")
    print(json.dumps(contract, sort_keys=True))
