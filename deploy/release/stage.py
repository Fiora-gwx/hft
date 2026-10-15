#!/usr/bin/python3
"""Join CI build outputs into the packager's input tree. No build or host mutation."""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import tomllib

from release import ReleaseError, check_binaries, sha256, version
from cpu import compatible, envelope


def copy_tree(source, target):
    shutil.copytree(source, target, ignore=shutil.ignore_patterns("__pycache__", "*.pyc", ".pytest_cache"))


def stage(repo, dist, tag, sha, run_id, evidence, runner=subprocess.check_output):
    version(tag)
    if not re.fullmatch(r"[0-9a-f]{40}", sha) or not str(run_id).isdigit():
        raise ReleaseError("invalid CI identity")
    if not evidence.get("ci_run_id") or not evidence.get("decoupling_check_run_id"):
        raise ReleaseError("missing quality gate evidence")
    root = dist / "stage"
    if root.exists():
        raise ReleaseError("staging already exists")
    root.mkdir(parents=True)
    target = "aarch64-unknown-linux-gnu"
    profiles = {}
    # Cargo metadata identifies executable targets; never glob build scripts or stale outputs.
    metadata = json.loads(runner(["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=repo))
    hot = {"hft-core", "kill-switch"} if "-dev." not in tag else set()
    for package in metadata["packages"]:
        if package["id"] not in metadata["workspace_members"] or package["name"] == "xtask":
            continue
        for item in package["targets"]:
            if item["kind"] != ["bin"]:
                continue
            name = item["name"]
            profile = "release-hot" if package["name"] in hot else "release"
            source = repo / "target" / target / profile / name
            destination = root / "bin" / name
            destination.parent.mkdir(exist_ok=True)
            shutil.copyfile(source, destination)
            destination.chmod(0o755)
            profiles[name] = profile
    if not {"hft-core", "hft-cfg", "hft-secrets", "hostprobe"} <= profiles.keys():
        raise ReleaseError("missing testnet executables")
    check_binaries(root, lambda argv: runner([str(x) for x in argv], cwd=repo))
    copy_tree(repo / "deploy", root / "deploy")
    copy_tree(repo / "schemas", root / "schemas")
    copy_tree(dist / "schemas/strategies", root / "schemas/strategies")
    copy_tree(repo / "apps/hft-core/config", root / "config-templates")
    copy_tree(dist / "python", root / "python")
    # Local wheels must be installed too, with exact hashes, not just copied into an unused dir.
    lock = root / "python/requirements.lock"
    entries = lock.read_text(encoding="utf-8").rstrip()
    for wheel in sorted((root / "python/wheels").glob("*.whl")):
        name, ver, *_ = wheel.name.split("-")
        shutil.copyfile(wheel, root / "python/wheelhouse" / wheel.name)
        entries += f"\n{name}=={ver} --hash=sha256:{sha256(wheel)}"
    lock.write_text(entries.lstrip() + "\n", encoding="utf-8")
    for source, destination in [(root / "deploy/polkit", root / "polkit")]:
        copy_tree(source, destination)
    units = root / "systemd"
    units.mkdir()
    for host in sorted((root / "deploy/generated").iterdir()):
        if host.is_dir():
            for unit in host.iterdir():
                if unit.suffix in (".service", ".slice", ".mount", ".target"):
                    dest = units / host.name / unit.name
                    dest.parent.mkdir(exist_ok=True)
                    shutil.copyfile(unit, dest)
    # Deterministic CycloneDX inventory from the locked Rust dependency set and shipped wheels.
    packages = tomllib.loads((repo / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    components = []
    for p in packages:
        row = {"type": "library", "name": p["name"], "version": p["version"], "purl": f"pkg:cargo/{p['name']}@{p['version']}"}
        if "checksum" in p:
            row["hashes"] = [{"alg": "SHA-256", "content": p["checksum"]}]
        components.append(row)
    for wheel in sorted((root / "python/wheelhouse").glob("*.whl")):
        name, ver, *_ = wheel.name.split("-")
        components.append({"type": "library", "name": name, "version": ver, "purl": f"pkg:pypi/{name}@{ver}", "hashes": [{"alg": "SHA-256", "content": sha256(wheel)}]})
    sbom = {"bomFormat": "CycloneDX", "specVersion": "1.5", "version": 1, "components": components}
    (root / "sbom.json").write_text(json.dumps(sbom, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    index = json.loads((root / "schemas/strategies/index.json").read_text())
    exported = runner([str(root / "bin/hft-core"), "--version"], cwd=repo).decode()
    versions = {int(k): int(v) for k, v in re.findall(r"kind_id=(\d+) \S+ version=(\d+)", exported)}
    strategies = [{"name": row["name"], "kind_id": row["kind_id"], "version": versions[row["kind_id"]], "schema_id": row["schema_id"], "schema_ver": row["schema_version"]} for row in index]
    cpu_contract = envelope(repo, runner)
    if not compatible(cpu_contract, runner(["cat", "/proc/cpuinfo"], cwd=repo).decode()):
        raise ReleaseError("native CI CPU does not satisfy compiled feature envelope")
    schemas = {p.relative_to(root / "schemas").as_posix(): sha256(p) for p in sorted((root / "schemas").rglob("*")) if p.is_file()}
    result = dict(version=tag, git_sha=sha, ci_run_id=str(run_id), target=target, **cpu_contract, rust_version=runner(["rustc", "--version"], cwd=repo).decode().strip(), profiles=profiles, strategies=strategies, schemas=schemas, evidence=evidence, sbom_sha256=sha256(root / "sbom.json"))
    (dist / "release-metadata.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return root, result


def main():
    p = argparse.ArgumentParser()
    p.add_argument("tag")
    p.add_argument("sha")
    p.add_argument("dist", type=Path)
    p.add_argument("--run-id", required=True)
    args = p.parse_args()
    try:
        evidence = json.loads((args.dist / "evidence.json").read_text())
        stage(Path(__file__).resolve().parents[2], args.dist.resolve(), args.tag, args.sha, args.run_id, evidence)
        return 0
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print("stage-release failed: " + type(error).__name__)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
