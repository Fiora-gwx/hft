#!/usr/bin/python3
"""Read-only preflight for the assembled dev2 prod/testnet profile.

Each check fails closed. Host operations are injectable, but the CLI uses real commands.
No credentials or command output are printed.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib

# The release is checksummed as an exact file set, including when preflight runs as root.
# Importing its helpers must not create __pycache__ inside that immutable tree.
sys.dont_write_bytecode = True
from release import selected_host, verify_tree
from cpu import compatible
from local_secrets import entries as local_entries


class Host:
    def __init__(self, root):
        self.root = Path(root)

    def path(self, name):
        return self.root / name.lstrip("/")

    def text(self, name):
        return self.path(name).read_text(encoding="utf-8")

    def command(self, *argv):
        return subprocess.run(argv, check=True, capture_output=True, text=True, timeout=60).stdout.strip()

    def available(self, name):
        return shutil.which(name) is not None

    def space(self, name):
        usage = shutil.disk_usage(self.path(name))
        return usage.free, usage.total

    def service_uids(self):
        sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "imds"))
        from dev2_lock import service_uids
        return service_uids()

    def live_secret_denied(self):
        result = subprocess.run(["aws", "secretsmanager", "describe-secret", "--secret-id", "hft/live/_sentinel"], capture_output=True, text=True, timeout=60)
        return result.returncode != 0 and "AccessDenied" in result.stderr


def check(host):
    results = {}

    def probe(name, operation):
        try:
            results[name] = bool(operation())
        except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
            results[name] = False
        return results[name]

    tree = host.path("/opt/hft/current-core")
    try:
        manifest = verify_tree(tree, installed=True)
        name, inventory = selected_host(host.root, tree, manifest)
        config = tomllib.loads(host.text("/etc/hft/hft-core-prod.toml"))
        policy = tomllib.loads(host.text("/etc/hft/preflight.toml"))
        cold = config["paths"]["journal_cold"]
    except (OSError, ValueError, KeyError, TypeError):
        return {"configuration-and-release": False}
    probe("assembled-profile", lambda: inventory["class"] == "dev2" and inventory["instances"] == ["prod"] and inventory["formal_live"] is False)
    probe("architecture", lambda: host.command("uname", "-m") == manifest["target"].split("-")[0])
    probe("cpu-features", lambda: compatible(manifest, host.text("/proc/cpuinfo")))
    probe("cold-volume", lambda: host.command("findmnt", "-n", "-o", "SOURCE", "-T", cold) != host.command("findmnt", "-n", "-o", "SOURCE", "-T", "/"))

    def disk():
        free, total = host.space(cold)
        limit = policy["disk_free_block_pct"]
        return type(limit) is int and 0 < limit < 100 and total > 0 and free * 100 >= total * limit
    probe("disk-free", disk)

    def clock():
        if host.command("systemctl", "is-active", "chrony.service") != "active":
            return False
        tracking = host.command("chronyc", "tracking")
        leap = re.search(r"Leap status\s*:\s*([^\n]+)", tracking)
        offset = re.search(r"Last offset\s*:\s*([+-]?[\d.]+)", tracking)
        return leap is not None and leap[1].strip() == "Normal" and offset is not None and abs(float(offset[1])) <= 0.005
    probe("chrony", clock)
    probe("no-compiler", lambda: not any(host.available(c) for c in ("cargo", "rustc")))
    probe("version-local-layout", lambda: not any(host.path(p).exists() or host.path(p).is_symlink() for p in ("/opt/hft/current", "/opt/hft/venv")))
    probe("testnet-only", lambda: config["instance"]["name"] == "prod" and config["instance"]["order_env"] == "testnet")
    probe("guard-active", lambda: host.command("systemctl", "is-active", "hft-dev2-guard.service") == "active")
    probe("batch-idle", lambda: host.text("/sys/fs/cgroup/hftbatch.slice/cpu.idle").strip() == "1" and not host.path("/sys/fs/cgroup/hft.slice/hftbatch.slice").exists())
    results.update(resource_checks(host, policy, cold))
    probe("guard-polkit", lambda: host.path("/etc/polkit-1/rules.d/50-hft-dev2-guard.rules").read_bytes() == (tree / "deploy/polkit/50-hft-dev2-guard.rules").read_bytes())
    for unit in ("hft.slice", "hft-core.slice", "hftbatch.slice"):
        probe("unit:" + unit, lambda unit=unit: host.path("/etc/systemd/system/" + unit).read_bytes() == (tree / "deploy/generated" / name / unit).read_bytes())

    def profile():
        value = json.loads(host.text("/run/hft/hostprofile.json"))
        if value.get("pending_reboot") is not False:
            return False
        # Recollect and verify; a previously passed profile is not evidence about this boot.
        host.command(str(tree / "bin/hostprobe"), "verify", "--expected", str(tree / "deploy/tuning" / (name + ".toml")), "--out", str(host.path("/run/hft/hostprofile.json")))
        return True
    probe("host-profile", profile)

    probe("imds-rules", lambda: valid_imds(json.loads(host.command("nft", "-j", "list", "table", "inet", "hft_imds")), host.service_uids()))
    local = config["instance"].get("secrets_mode", "aws_secrets_manager") == "local_demo"
    probe("secret-mode", lambda: config["instance"].get("secrets_mode", "aws_secrets_manager") in ("aws_secrets_manager", "local_demo"))
    if not local:
        probe("live-secret-denied", host.live_secret_denied)
    # These commands validate local references without returning secret values.
    secrets = sorted(host.path("/etc/hft/secrets").glob("*.toml"))
    probe("secrets-manifests", lambda: bool(secrets))
    for secret in secrets:
        if local:
            probe("local-secrets:" + secret.name, lambda secret=secret: secret.name == "hft-core.toml" and bool(local_entries(config, tomllib.loads(secret.read_text(encoding="utf-8")), host.root)))
            continue
        def allowed_prefix(secret=secret):
            entries = tomllib.loads(secret.read_text())["secret"]
            return bool(entries) and all(entry["secret_id"].split(":secret:", 1)[-1].startswith(("hft/testnet/", "hft/ops/")) for entry in entries)
        probe("secret-prefix:" + secret.name, allowed_prefix)
        probe("secrets:" + secret.name, lambda secret=secret: host.command(str(tree / "bin/hft-secrets"), "--check", "--manifest", str(secret)) is not None)
    probe("core-config", lambda: host.command(str(tree / "bin/hft-core"), "--check", "--config", str(host.path("/etc/hft/hft-core-prod.toml"))) is not None)
    return results


def resource_checks(host, policy, cold):
    """Check installed kernel limits, not just the text of a unit file. Never stress the host."""
    results = {}

    def probe(name, fn):
        try:
            results[name] = bool(fn())
        except (OSError, ValueError, KeyError, subprocess.SubprocessError):
            results[name] = False

    def read(name):
        return host.text("/sys/fs/cgroup/" + name).strip()

    probe("cgroup-controllers", lambda: {"cpu", "memory", "io"} <= set(read("cgroup.subtree_control").split()))
    probe("dev2-no-isolated-cpus", lambda: host.text("/sys/devices/system/cpu/isolated").strip() == "")
    probe("core-oom-score", lambda: host.command("systemctl", "show", "--property=OOMScoreAdjust", "--value", "hft-core@prod.service") == "100")
    for directory, budget in (("hft.slice", "parent_memory"), ("hftbatch.slice", "batch_memory"), ("hft.slice/hft-core.slice/hft-core@prod.service", "memory")):
        # The service cgroup does not exist yet in ExecStartPre? systemd creates it before pre-start.
        probe("memory:" + directory, lambda directory=directory, budget=budget: int(read(directory + "/memory.max")) == policy[budget] * 1024 * 1024 and read(directory + "/memory.swap.max") == "0")
    probe("parent-cpu-limit", lambda: read("hft.slice/cpu.max") == f"{policy['quota'] * 1000} 100000" and int(read("hft.slice/cpu.weight")) == policy["weight"])
    probe("batch-cpu-limit", lambda: read("hftbatch.slice/cpu.max") == "max 100000" and read("hftbatch.slice/cpu.idle") == "1")

    def io_limit():
        device = host.command("findmnt", "-n", "-o", "MAJ:MIN", "-T", cold)
        rows = [line.split() for line in read("hftbatch.slice/io.max").splitlines()]
        row = next((parts[1:] for parts in rows if parts[0] == device), [])
        limits = dict(part.split("=") for part in row)
        return limits.get("rbps") == str(policy["read"] * 1_000_000) and limits.get("wbps") == str(policy["write"] * 1_000_000)
    probe("batch-io-limit", io_limit)
    return results


def valid_imds(value, uids):
    sets = [row["set"] for row in value["nftables"] if "set" in row and row["set"].get("name") == "hft_uids"]
    if len(sets) != 1 or not uids or not uids <= set(sets[0].get("elem", [])):
        return False
    chains = [row["chain"] for row in value["nftables"] if "chain" in row and row["chain"].get("name") == "out"]
    if len(chains) != 1 or chains[0].get("hook") != "output" or chains[0].get("type") != "filter":
        return False
    rules = [row["rule"] for row in value["nftables"] if "rule" in row]
    # Both destination families must be dropped for the installed hft uid set.
    families = set()
    for rule in rules:
        if rule.get("chain") != "out":
            continue
        uid = destination = dropped = False
        family = None
        for expr in rule.get("expr", []):
            match = expr.get("match", {})
            if match.get("op") == "==" and match.get("left") == {"meta": {"key": "skuid"}} and match.get("right") == "@hft_uids":
                uid = True
            payload = match.get("left", {}).get("payload", {})
            if match.get("op") == "==" and payload.get("field") == "daddr" and (payload.get("protocol"), match.get("right")) in (("ip", "169.254.169.254"), ("ip6", "fd00:ec2::254")):
                destination, family = True, payload["protocol"]
            dropped |= "drop" in expr
        if uid and destination and dropped:
            families.add(family)
    return families == {"ip", "ip6"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("/"))
    parser.add_argument("--dry-run", action="store_true", help="preflight is always read-only except hostprofile refresh")
    args = parser.parse_args()
    results = check(Host(args.root))
    for name, passed in results.items():
        print(("PASS " if passed else "FAIL ") + name)
    return 0 if all(results.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
