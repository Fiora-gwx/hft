#!/usr/bin/python3
"""Only the dev2 nftables foundation; never modify another table or attach cgroup BPF."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tomllib

SERVICE_USERS = frozenset(("hft", "hft-sim", "hft-ks", "hft-archive", "hft-replay", "hft-verify", "hft-watch", "hft-recon", "hft-opsbot", "hft-pipeline", "hft-treasury", "hft-guard", "hft-offbox", "hft-chain"))


def service_uids():
    import pwd
    rows = {row.pw_name: row.pw_uid for row in pwd.getpwall() if row.pw_name in SERVICE_USERS}
    if not {"hft", "hft-guard"} <= rows.keys() or any(uid == 0 for uid in rows.values()):
        raise ValueError("required service identities missing or privileged")
    return set(rows.values())


def rules(uids, replace):
    if not uids or any(type(uid) is not int or uid <= 0 for uid in uids):
        raise ValueError("invalid service uid set")
    prefix = "delete table inet hft_imds\n" if replace else ""
    return prefix + """table inet hft_imds {
  set hft_uids { type uid; elements = { %s }; }
  chain out {
    type filter hook output priority 0; policy accept;
    meta skuid @hft_uids ip daddr 169.254.169.254 counter drop
    meta skuid @hft_uids ip6 daddr fd00:ec2::254 counter drop
  }
}
""" % ", ".join(str(uid) for uid in sorted(uids))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("load", "status"))
    parser.add_argument("--root", type=Path, default=Path("/"))
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    try:
        name = tomllib.loads((args.root / "etc/hft/host-local.toml").read_text())["manifest"]
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", name):
            raise ValueError("invalid inventory name")
        inventory = tomllib.loads((args.root / "opt/hft/current-core/deploy/hosts" / (name + ".toml")).read_text())
        if inventory["class"] != "dev2":
            raise ValueError("only dev2 nftables is assembled")
        listed = subprocess.run(["nft", "-j", "list", "tables"], check=True, capture_output=True, text=True, timeout=10)
        existing = any(row.get("table", {}).get("name") == "hft_imds" and row["table"].get("family") == "inet" for row in json.loads(listed.stdout)["nftables"])
        if args.action == "status":
            return subprocess.run(["nft", "list", "table", "inet", "hft_imds"], timeout=10).returncode
        text = rules(service_uids(), existing)
        if args.dry_run:
            print(text, end="")
        else:
            # One nft transaction: no interval with the old table deleted and no replacement.
            subprocess.run(["nft", "-f", "-"], input=text, text=True, check=True, timeout=10)
        return 0
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print("hft-imds-lock failed: " + type(error).__name__)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
