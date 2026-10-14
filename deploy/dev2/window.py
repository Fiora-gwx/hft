#!/usr/bin/python3
"""Explicit testnet window; failures leave the marker until prod has stopped."""
import argparse
import os
from pathlib import Path
import subprocess

BATCH = ("hft-verify.service", "hft-replay-verify.service", "hft-journal-archiver.service", "hft-pipeline.service", "hft-treasury.service")
PROD = "hft-core@prod.service"


def command(*argv):
    return subprocess.run(argv, check=True, capture_output=True, timeout=60).stdout


def used_bytes(path):
    # Count mapped-but-unlinked pages as well; directory enumeration cannot see them.
    stat = os.statvfs(path)
    return (stat.f_blocks - stat.f_bfree) * stat.f_frsize


def window(action, root=Path("/"), run=command, dry_run=False, usage=used_bytes):
    marker = root / "run/hft/window/active"
    if action == "start" and (root / "var/lib/hft/guard/tripped").exists():
        raise ValueError("guard is tripped")
    if dry_run:
        return
    if action == "start":
        run("systemctl", "is-active", "--quiet", "hft-dev2-guard.service")
        marker.parent.mkdir(parents=True, exist_ok=True, mode=0o755)
        # Block new batch starts before stopping any running batch unit.
        marker.touch(mode=0o644, exist_ok=True)
        for unit in BATCH:
            # A reduced testnet archive intentionally does not install all cold services.
            if run("systemctl", "show", "--property=LoadState", "--value", unit).strip() != b"not-found":
                run("systemctl", "stop", unit)
        run("systemctl", "start", PROD)
    else:
        run("systemctl", "stop", PROD)
        if run("systemctl", "show", "--property=ActiveState", "--value", PROD).strip() not in (b"inactive", b"failed"):
            raise ValueError("prod has not stopped")
        if usage(root / "dev/shm/hft/prod") > 4096:
            raise ValueError("prod hot pages remain; retain window until recovery and clean Drain")
        marker.unlink(missing_ok=True)
        # Batch timers resume naturally; the window never starts a batch itself.


def main():
    p = argparse.ArgumentParser()
    p.add_argument("action", choices=("start", "stop"))
    p.add_argument("--dry-run", action="store_true")
    args = p.parse_args()
    try:
        if not args.dry_run and os.geteuid() != 0:
            raise ValueError("window requires root")
        import fcntl
        lock = Path("/run/hft/window")
        if args.dry_run:
            window(args.action, dry_run=True)
        else:
            lock.mkdir(parents=True, exist_ok=True, mode=0o755)
            with (lock / "lock").open("a") as stream:
                fcntl.flock(stream, fcntl.LOCK_EX)
                window(args.action)
        return 0
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("hft-window failed: " + type(error).__name__)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
