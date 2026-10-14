#!/usr/bin/python3
"""Deterministic testnet service assembly. Resource numbers come from hft-core's specification."""
import argparse
import ipaddress
import json
from pathlib import Path
import re
import sys
import tomllib


def validate(host):
    allowed = {"schema", "host", "instance_type", "class", "instances", "formal_live", "verify", "replay_verify", "eni_b", "egress"}
    if set(host) != allowed or host["schema"] != 4:
        raise ValueError("host fields/schema")
    # This executable assembly is intentionally bounded to the current prod/testnet runtime.
    if host["class"] != "dev2" or host["formal_live"] is not False or host["instances"] != ["prod"]:
        raise ValueError("generator requires dev2 prod/testnet; other runtime profiles are not assembled")
    if host["verify"] is not False or host["replay_verify"] is not False:
        raise ValueError("verification services require the complete release generator")
    if not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]*", host["host"]):
        raise ValueError("host name")
    if set(host["eni_b"]) != {"name"} or not re.fullmatch(r"[a-zA-Z0-9_.-]+", host["eni_b"]["name"]):
        raise ValueError("interface name")
    if set(host["egress"]) != {"prod"} or not host["egress"]["prod"]:
        raise ValueError("prod egress required")
    for item in host["egress"]["prod"]:
        if set(item) != {"eni", "ip"} or item["eni"] != "b":
            raise ValueError("egress fields")
        if not ipaddress.IPv4Address(item["ip"]).is_private:
            raise ValueError("egress must be a local private IPv4 address")


def resources(specification):
    rows = specification.splitlines()
    row = next(r for r in rows if r.startswith("| **单元 `MemoryMax`"))
    columns = [c.strip().replace("*", "").replace(",", "") for c in row.split("|")[2:-1]]
    memory = int(columns[1])
    parent = re.search(r"\*\*`hft.slice`\(父\)\*\*:dev2 上 `MemoryMax=(\d+)M`、`MemorySwapMax=0`、`CPUQuota=(\d+)%`.*?、`CPUWeight=(\d+)`", specification)
    if parent is None:
        raise ValueError("missing hft-core parent resource specification")
    batch = next(r for r in rows if r.startswith("| `hftbatch.slice`(根级)"))
    batch_memory = int(re.search(r"`MemoryMax=(\d+)M`", batch)[1])
    tmpfs = int(re.search(r"dev2 `prod` `size=(\d+)M`", specification)[1])
    io = re.search(r"\| `hftbatch.slice` \| 读 (\d+) MB/s、写 (\d+) MB/s", specification)
    return dict(memory=memory, parent_memory=int(parent[1]), quota=int(parent[2]), weight=int(parent[3]), batch_memory=batch_memory, tmpfs=tmpfs, read=int(io[1]), write=int(io[2]))


def render(host, core_spec, guard_unit):
    validate(host)
    r = resources(core_spec)
    result = {}
    result["hft.slice"] = f"[Slice]\nMemoryMax={r['parent_memory']}M\nMemorySwapMax=0\nCPUQuota={r['quota']}%\nCPUWeight={r['weight']}\n"
    result["hft-core.slice"] = f"[Slice]\nCPUWeight=100\nMemoryMax={r['parent_memory']}M\nMemorySwapMax=0\n"
    result["hftbatch.slice"] = f"[Slice]\nCPUWeight=idle\nMemoryMax={r['batch_memory']}M\nMemorySwapMax=0\nIOReadBandwidthMax=/data {r['read']}M\nIOWriteBandwidthMax=/data {r['write']}M\n"
    result["dev-shm-hft-prod.mount"] = f"[Unit]\nDescription=hft prod journal tmpfs\nBefore=hft-core@prod.service\n\n[Mount]\nWhat=tmpfs\nWhere=/dev/shm/hft/prod\nType=tmpfs\nOptions=size={r['tmpfs']}M,uid=hft,gid=hft-journal-ro,mode=2750\n"
    result["hft-core@prod.service"] = f"""[Unit]
Description=hft testnet core
After=network-online.target hft-dev2-guard.service hft-imds-lock.service
RequiresMountsFor=/dev/shm/hft/prod /data/prod
StartLimitBurst=3
StartLimitIntervalSec=600

[Service]
Type=notify
User=hft
Slice=hft-core.slice
SupplementaryGroups=hft-ctl hft-stopmark
RuntimeDirectory=hft/core-prod
RuntimeDirectoryMode=0751
ExecStartPre=/usr/bin/systemctl is-active --quiet hft-dev2-guard.service
ExecStartPre=/usr/bin/systemctl is-active --quiet hft-imds-lock.service
ExecStartPre=+/opt/hft/current-core/deploy/release/preflight.sh
ExecStartPre=+/usr/bin/install -d -o hft -g hft-journal-ro -m 2750 /data/prod
ExecStartPre=+/opt/hft/current-core/bin/hft-secrets fetch --manifest /etc/hft/secrets/hft-core.toml --out /run/hft/core-prod/secrets --owner hft --mode 0400
ExecStart=/opt/hft/current-core/bin/hft-core --config /etc/hft/hft-core-prod.toml
ExecStopPost=+/opt/hft/current-core/deploy/bin/hft-shm-release prod
WatchdogSec=10
TimeoutStartSec=45
TimeoutStopSec=20
Restart=on-failure
RestartSec=1
RestartPreventExitStatus=2 3 4
RestartForceExitStatus=75
LimitMEMLOCK=infinity
LimitNOFILE=65536
MemoryMax={r['memory']}M
MemorySwapMax=0
OOMScoreAdjust=100
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/data/prod /dev/shm/hft/prod -/var/lib/hft/kill-switch
"""
    result["hft-dev2-guard.service"] = guard_unit
    result["hft-imds-lock.service"] = "[Unit]\nDescription=hft dev2 IMDS uid isolation\nBefore=hft-core@prod.service\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nUser=root\nSlice=system.slice\nExecStart=/opt/hft/current-core/deploy/imds/hft-imds-lock.sh load\n\n[Install]\nWantedBy=multi-user.target\n"
    result["hft-core.target"] = "[Unit]\nDescription=hft core application group\nWants=hft-core@prod.service\n"
    result["egress.toml"] = "prod = [" + ", ".join('{ eni = "b", ip = "' + x["ip"] + '" }' for x in host["egress"]["prod"]) + "]\n"
    storage_row = next(line for line in core_spec.splitlines() if line.startswith("| `storage.disk_free_block_pct`"))
    disk_limit = re.search(r"\| (\d+) / \d+", storage_row)[1]
    result["preflight.toml"] = "disk_free_block_pct = " + disk_limit + "\n"
    expectation = [("guard.dev2_guard_active", True)]
    for name, memory, parent in [("hft.slice", r["parent_memory"], "/"), ("hft-core.slice", r["parent_memory"], "/hft.slice"), ("hftbatch.slice", r["batch_memory"], "/")]:
        for key, value in [("present", True), ("parent", parent), ("memory_max", str(memory * 1024 * 1024)), ("memory_swap_max", "0")]:
            expectation.append(("guard.slices." + name + "." + key, value))
    expectation += [("guard.slices.hft.slice.cpu_max", str(r["quota"] * 1000) + " 100000"), ("guard.slices.hft.slice.cpu_weight", r["weight"]), ("guard.slices.hft-core.slice.cpu_weight", 100), ("guard.slices.hftbatch.slice.cpu_idle", 1), ("guard.slices.hftbatch.slice.cpu_max", "max 100000")]
    expectation += [("nic.egress[*].nic", host["eni_b"]["name"]), ("nic.egress[*].on_nic", True)]
    profile = 'manifest = "' + host["host"] + '"\nclass = "dev2"\n'
    for key, value in expectation:
        profile += '\n[[expect]]\nkey = ' + json.dumps(key) + '\neq = ' + json.dumps(value) + '\nlevel = "required"\n'
    return result, profile


def generate(repo, manifest, check=False):
    path = repo / "deploy/hosts" / (manifest + ".toml")
    host = tomllib.loads(path.read_text(encoding="utf-8"))
    generated, profile = render(host, (repo / "apps/hft-core/README.md").read_text(encoding="utf-8"), (repo / "deploy/dev2/hft-dev2-guard.service").read_text(encoding="utf-8"))
    prefix = "deploy/generated/" + manifest + "/"
    group_files, units = [], []
    for name in generated:
        if name.endswith((".service", ".slice", ".target", ".mount")):
            destination = "etc/systemd/system/" + name
            if name in ("hft-core@prod.service", "hft-core.target"):
                units.append(name)
        else:
            destination = "etc/hft/" + name
        group_files.append({"source": prefix + name, "destination": destination})
    group_files.extend([
        {"source": "deploy/dev2/dev2-guard.toml.tmpl", "destination": "etc/hft/dev2-guard.toml"},
        {"source": "deploy/polkit/50-hft-dev2-guard.rules", "destination": "etc/polkit-1/rules.d/50-hft-dev2-guard.rules"},
    ])
    generated["install.json"] = json.dumps({"groups": {"core": group_files, "cold": [], "sim": []}, "units": {"core": units, "cold": [], "sim": []}}, sort_keys=True, indent=2) + "\n"
    outputs = {repo / prefix / name: text for name, text in generated.items()}
    outputs[repo / "deploy/tuning" / (manifest + ".toml")] = profile
    if check:
        if any(not p.is_file() or p.read_bytes() != text.encode() for p, text in outputs.items()):
            raise ValueError("generated files differ")
    else:
        for path, text in outputs.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(text.encode())
    return outputs


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--host")
    p.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    p.add_argument("--check", action="store_true")
    args = p.parse_args()
    try:
        if args.host is not None and not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", args.host):
            raise ValueError("manifest name")
        names = [args.host] if args.host else sorted(p.stem for p in (args.root / "deploy/hosts").glob("*.toml"))
        if not names:
            raise ValueError("no host manifests")
        for name in names:
            generate(args.root, name, args.check)
        return 0
    except (OSError, ValueError, KeyError, StopIteration) as error:
        print("gen-host: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
