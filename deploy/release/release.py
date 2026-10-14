#!/usr/bin/python3
"""Immutable CI-artifact installation and cold, per-group activation.

Filesystem and command boundaries are injectable for container tests. The CLI never builds.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib


class ReleaseError(ValueError):
    pass


def sha256(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(argv):
    # No shell, no environment dumps, no credentials in command arguments.
    subprocess.run([str(x) for x in argv], check=True, timeout=300)


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temp = Path(stream.name)
        stream.write((json.dumps(value, sort_keys=True, indent=2) + "\n").encode())
        stream.flush()
        os.fsync(stream.fileno())
    try:
        temp.replace(path)
        sync_dir(path.parent)
    finally:
        temp.unlink(missing_ok=True)


def sync_dir(path):
    if os.name == "posix":
        fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)


def relative(name):
    if not isinstance(name, str) or not name or "\\" in name or "\0" in name:
        raise ReleaseError("invalid archive path")
    path = PurePosixPath(name)
    if path.is_absolute() or any(p in ("", ".", "..") for p in name.split("/")) or ":" in name:
        raise ReleaseError("unsafe archive path")
    return path


def version(value):
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+(?:-dev\.[0-9]+)?", value):
        raise ReleaseError("invalid version")
    return value


def unpack(archive, destination):
    """Validate the entire member set before creating any extracted file."""
    with tempfile.TemporaryDirectory(dir=destination.parent) as temp:
        source = archive
        if archive.name.endswith(".zst"):
            source = Path(temp) / "archive.tar"
            run(["zstd", "-q", "-d", "-o", source, "--", archive])
        with tarfile.open(source, "r:*") as tar:
            members = tar.getmembers()
            seen, roots = set(), set()
            for member in members:
                path = relative(member.name.rstrip("/") if member.isdir() else member.name)
                if str(path) in seen or not (member.isfile() or member.isdir()):
                    raise ReleaseError("duplicate, linked or special archive member")
                seen.add(str(path))
                roots.add(path.parts[0])
            if len(roots) != 1:
                raise ReleaseError("archive must have exactly one root")
            for member in members:
                path = relative(member.name.rstrip("/") if member.isdir() else member.name)
                target = destination.joinpath(*path.parts)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    with tar.extractfile(member) as src, target.open("xb") as dst:
                        shutil.copyfileobj(src, dst)
                    target.chmod(0o755 if member.mode & 0o111 else 0o644)
            return destination / roots.pop()


def verify_tree(tree, installed=False):
    manifest = json.loads((tree / "RELEASE.json").read_text(encoding="utf-8"))
    ver = version(manifest["version"])
    channel = "dev" if "-dev." in ver else "release"
    if manifest["channel"] != channel or manifest["target"] != "aarch64-unknown-linux-gnu":
        raise ReleaseError("release channel/target mismatch")
    expected = {}
    for row in manifest["files"]:
        name = str(relative(row["path"]))
        if name in expected or name == "RELEASE.json" or name.split("/")[0] == "venv":
            raise ReleaseError("invalid manifest file set")
        expected[name] = row
    actual = set()
    for path in tree.rglob("*"):
        rel = path.relative_to(tree).as_posix()
        if installed and rel.split("/")[0] == "venv":
            continue
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ReleaseError("non-regular installed member")
        if path.is_file() and rel != "RELEASE.json":
            actual.add(rel)
    if actual != set(expected):
        raise ReleaseError("manifest file set mismatch")
    for name, row in expected.items():
        path = tree / name
        if path.stat().st_size != row["bytes"] or sha256(path) != row["sha256"]:
            raise ReleaseError("manifest file checksum mismatch")
    return manifest


def selected_host(root, tree, manifest):
    local = tomllib.loads((root / "etc/hft/host-local.toml").read_text(encoding="utf-8"))
    name = local["manifest"]
    if not isinstance(name, str) or not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_.-]*", name):
        raise ReleaseError("invalid host manifest name")
    profile = tree / "deploy/tuning" / (name + ".toml")
    if manifest["host_expect"].get(name) != sha256(profile):
        raise ReleaseError("host expectation mismatch")
    host = tomllib.loads((tree / "deploy/hosts" / (name + ".toml")).read_text(encoding="utf-8"))
    if manifest["channel"] == "dev" and host["class"] != "dev2":
        raise ReleaseError("development release requires dev2")
    return name, host


def check_binaries(tree, runner):
    binaries = sorted((tree / "bin").iterdir())
    if not binaries or not (tree / "bin/hft-core").is_file():
        raise ReleaseError("missing production binaries")
    for path in binaries:
        with path.open("rb") as binary:
            header = binary.read(20)
        if len(header) < 20 or header[:6] != b"\x7fELF\x02\x01" or int.from_bytes(header[18:20], "little") != 183:
            raise ReleaseError("binary is not aarch64 ELF64")
        if path.name == "hft-core":
            runner([path, "--version"])


def identities(runner):
    """Only the identities used by the assembled dev2 profile; never touch legacy users."""
    for group in ("hft", "hft-guard", "hft-ctl", "hft-stopmark", "hft-journal-ro"):
        runner(["groupadd", "--system", "--force", group])
    for user in ("hft", "hft-guard"):
        try:
            runner(["id", "-u", user])
        except subprocess.CalledProcessError:
            runner(["useradd", "--system", "--no-create-home", "--gid", user, "--shell", "/usr/sbin/nologin", user])
    runner(["usermod", "--append", "--groups", "hft-ctl,hft-stopmark,hft-journal-ro", "hft"])


def install(archive, checksum, root, runner=run, dry_run=False):
    archive, checksum, root = Path(archive), Path(checksum), Path(root)
    words = checksum.read_text(encoding="ascii").strip().split()
    if not words or not re.fullmatch(r"[0-9a-fA-F]{64}", words[0]) or sha256(archive) != words[0].lower():
        raise ReleaseError("archive checksum mismatch")
    base = root / "opt/hft"
    base.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".install-", dir=base) as temporary:
        tree = unpack(archive, Path(temporary))
        manifest = verify_tree(tree)
        selected_host(root, tree, manifest)
        destination = base / "releases" / manifest["version"]
        if destination.exists() or destination.is_symlink():
            raise ReleaseError("version already installed")
        check_binaries(tree, runner)
        requirements = tree / "python/requirements.lock"
        if not requirements.is_file() or not (tree / "python/wheelhouse").is_dir():
            raise ReleaseError("missing offline Python environment")
        if dry_run:
            return manifest
        destination.parent.mkdir(parents=True, exist_ok=True)
        tree.replace(destination)
        try:
            runner([sys.executable, "-m", "venv", "--copies", destination / "venv"])
            runner([destination / "venv/bin/python", "-m", "pip", "install", "--no-index", "--no-deps", "--require-hashes", "--find-links", destination / "python/wheelhouse", "-r", destination / "python/requirements.lock"])
            for path in destination.rglob("*"):
                if path.is_symlink():
                    # venv may create lib64 -> lib; it never enters the archive or manifest.
                    continue
                path.chmod(path.stat().st_mode & ~0o022)
            destination.chmod(0o755)
            atomic_json(base / "installations" / (manifest["version"] + ".json"), {
                "version": manifest["version"], "archive_sha256": words[0].lower(),
                "manifest_sha256": sha256(destination / "RELEASE.json"),
            })
        except Exception:
            destination.replace(destination.with_name(destination.name + ".failed." + str(int(time.time()))))
            raise
        return manifest


def activate(ver, group, root, runner=run, dry_run=False):
    ver = version(ver)
    if group not in ("core", "cold", "sim"):
        raise ReleaseError("invalid activation group")
    root = Path(root)
    base = root / "opt/hft"
    tree = base / "releases" / ver
    manifest = verify_tree(tree, installed=True)
    installed = json.loads((base / "installations" / (ver + ".json")).read_text())
    if installed["manifest_sha256"] != sha256(tree / "RELEASE.json"):
        raise ReleaseError("installed manifest was modified")
    name, _host = selected_host(root, tree, manifest)
    plan = json.loads((tree / "deploy/generated" / name / "install.json").read_text())
    files = plan["groups"][group]
    units = plan["units"][group]
    if not files or not units:
        raise ReleaseError("activation group is not assembled")
    if any(not re.fullmatch(r"hft-[A-Za-z0-9@_.-]+\.(service|timer|target)", u) for u in units):
        raise ReleaseError("activation contains non-hft unit")
    group_units = {
        "core": {"hft-core@prod.service", "hft-core.target"},
        "sim": {"hft-core@sim.service", "hft-sim.target"},
        "cold": {"hft-cold.target", *("hft-" + name + ".service" for name in ("kill-switch", "watchtower", "reconciler", "journal-archiver", "replay-verify", "ops-bot", "pipeline", "treasury")), "hft-replay-verify.timer"},
    }
    if not set(units) <= group_units[group]:
        raise ReleaseError("activation contains another group's unit")
    copies = []
    for item in files:
        src = tree / str(relative(item["source"]))
        dst = str(relative(item["destination"]))
        if not dst.startswith(("etc/systemd/system/", "etc/hft/", "etc/polkit-1/rules.d/", "etc/udev/rules.d/", "etc/sysctl.d/")):
            raise ReleaseError("activation destination outside hft installation")
        if not dst.startswith("etc/hft/") and "hft" not in Path(dst).name:
            raise ReleaseError("activation contains a non-hft destination")
        if not src.is_file():
            raise ReleaseError("activation source missing")
        copies.append((src, root / dst))
    current = base / ("current-" + group)
    previous = base / ("previous-" + group)
    old = os.readlink(current) if current.is_symlink() else None
    if current.exists() and old is None:
        raise ReleaseError("activation pointer is not a symlink")
    if dry_run:
        return {"version": ver, "group": group, "units": units, "files": files}
    identities(runner)
    # Stop the old process before replacing any pointer or unit content it might execute.
    runner(["systemctl", "disable", "--now", *units])
    if group == "core":
        # First boot needs this before systemd constructs ReadWritePaths and preflight
        # probes the cold filesystem. ExecStartPre still repairs its mode on every start.
        runner(["install", "-d", "-o", "hft", "-g", "hft-journal-ro", "-m", "2750", root / "data/prod"])
    def switch(link, target):
        temp = link.with_name(link.name + ".next")
        temp.unlink(missing_ok=True)
        temp.symlink_to(target, target_is_directory=True)
        temp.replace(link)
        sync_dir(base)
    if old is not None and old != "releases/" + ver:
        switch(previous, old)
    switch(current, "releases/" + ver)
    # No start, restart, mask, or cross-group operation is permitted here.
    unit_sources = [s for s, d in copies if d.suffix in (".service", ".timer", ".slice", ".target")]
    if unit_sources:
        runner(["systemd-analyze", "verify", *unit_sources])
    for src, dst in copies:
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dst)
        dst.chmod(0o644)
    runner(["systemctl", "daemon-reload"])
    record = dict(version=ver, group=group, manifest=name, channel=manifest["channel"], git_sha=manifest["git_sha"], archive_sha256=installed["archive_sha256"], current="releases/" + ver, previous=old, acceptance="pending", host_expect_sha256=manifest["host_expect"][name])
    try:
        output = root / "run/hft/hostprofile.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        runner([base / "current-core/bin/hostprobe", "verify", "--expected", base / "current-core/deploy/tuning" / (name + ".toml"), "--out", output])
        record["hostprobe"] = "passed"
    except Exception:
        record["hostprobe"] = "failed"
        raise
    finally:
        atomic_json(base / "deployments" / (str(time.time_ns()) + "-" + ver + ".json"), record)
    return record


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("install", "activate"))
    parser.add_argument("--root", type=Path, default=Path("/"))
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--sha256-file", type=Path)
    parser.add_argument("--version")
    parser.add_argument("--group", choices=("core", "cold", "sim"))
    args = parser.parse_args()
    try:
        if args.action == "install":
            if args.archive is None or args.sha256_file is None:
                parser.error("install requires --archive and --sha256-file")
            result = install(args.archive, args.sha256_file, args.root, dry_run=args.dry_run)
        else:
            if args.version is None or args.group is None:
                parser.error("activate requires --version and --group")
            result = activate(args.version, args.group, args.root, dry_run=args.dry_run)
        print(json.dumps({"version": result["version"], "status": "checked" if args.dry_run else args.action + "ed"}))
        return 0
    except (OSError, ValueError, KeyError, subprocess.SubprocessError, tarfile.TarError) as error:
        # Never echo subprocess arguments or external file contents.
        print("release: validation/action failed (" + type(error).__name__ + ")", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
