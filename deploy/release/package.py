#!/usr/bin/python3
"""Package prebuilt CI output; never compile or access credentials on the target host."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile

from release import ReleaseError, sha256, verify_tree, version


def package_tree(source, output, metadata, epoch, compress=True):
    """A complete staging tree + CI metadata produces an immutable, reproducible archive."""
    source, output = source.resolve(), output.resolve()
    if output.is_relative_to(source):
        raise ReleaseError("archive output must be outside its staging tree")
    version(metadata["version"])
    if not re.fullmatch(r"[0-9a-f]{40}", metadata["git_sha"]) or epoch < 0:
        raise ReleaseError("invalid provenance")
    if metadata["target"] != "aarch64-unknown-linux-gnu":
        raise ReleaseError("unsupported release target")
    name = "hft-" + metadata["version"] + "-aarch64"
    output.mkdir(parents=True, exist_ok=True)
    final = output / (name + (".tar.zst" if compress else ".tar"))
    if final.exists():
        raise ReleaseError("release archive already exists")
    with tempfile.TemporaryDirectory(dir=output) as temp:
        tree = Path(temp) / name
        shutil.copytree(source, tree, symlinks=True)
        entries = sorted(tree.rglob("*"), key=lambda p: p.relative_to(tree).as_posix())
        if any(p.is_symlink() or not (p.is_file() or p.is_dir()) for p in entries):
            raise ReleaseError("staging contains a link or special file")
        if (tree / "venv").exists() or (tree / "RELEASE.json").exists():
            raise ReleaseError("staging contains installed state")
        manifest = dict(metadata)
        manifest["channel"] = "dev" if "-dev." in metadata["version"] else "release"
        manifest["host_expect"] = {p.stem: sha256(tree / "deploy/tuning" / p.name) for p in sorted((tree / "deploy/hosts").glob("*.toml"))}
        if not manifest["host_expect"]:
            raise ReleaseError("release has no host expectations")
        manifest["files"] = [{"path": p.relative_to(tree).as_posix(), "bytes": p.stat().st_size, "sha256": sha256(p)} for p in entries if p.is_file()]
        (tree / "RELEASE.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        verify_tree(tree)
        tarpath = Path(temp) / (name + ".tar")
        with tarfile.open(tarpath, "w", format=tarfile.USTAR_FORMAT) as tar:
            for p in [tree, *sorted(tree.rglob("*"), key=lambda p: p.relative_to(tree).as_posix())]:
                info = tar.gettarinfo(str(p), arcname=p.relative_to(tree.parent).as_posix())
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                info.mtime = epoch
                info.mode = 0o755 if p.is_dir() or p.stat().st_mode & 0o111 else 0o644
                if p.is_file():
                    with p.open("rb") as stream:
                        tar.addfile(info, stream)
                else:
                    tar.addfile(info)
        packed = tarpath
        if compress:
            packed = tarpath.with_suffix(".tar.zst")
            subprocess.run(["zstd", "-19", "-T4", "-q", "-o", str(packed), str(tarpath)], check=True, timeout=300)
        packed.replace(final)
        (output / (final.name + ".sha256")).write_text(sha256(final) + "  " + final.name + "\n", encoding="ascii")
    return final


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("git_sha")
    parser.add_argument("dist", type=Path)
    parser.add_argument("--stage", type=Path, help="complete CI staging tree (default: dist/stage)")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    try:
        # Provenance is supplied by CI, never invented from a developer's machine.
        metadata = json.loads((args.dist / "release-metadata.json").read_text())
        if metadata["version"] != args.version or metadata["git_sha"] != args.git_sha:
            raise ReleaseError("CI provenance differs from requested release")
        for key in ("ci_run_id", "target_cpu", "cpu_features", "rust_version", "profiles", "strategies", "schemas", "evidence", "sbom_sha256"):
            if not metadata.get(key):
                raise ReleaseError("missing CI provenance: " + key)
        source = args.stage or args.dist / "stage"
        if sha256(source / "sbom.json") != metadata["sbom_sha256"]:
            raise ReleaseError("SBOM differs from CI provenance")
        if not args.dry_run:
            print(package_tree(source, args.dist, metadata, int(os.environ.get("SOURCE_DATE_EPOCH", "0"))))
        return 0
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print("package-release failed: " + type(error).__name__)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
