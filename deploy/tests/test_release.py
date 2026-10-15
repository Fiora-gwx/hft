"""The actual installer runs against isolated files and an observable command boundary."""
import hashlib
import importlib.util
import io
import json
import os
import subprocess
from pathlib import Path
import tarfile
import sys

import pytest

spec = importlib.util.spec_from_file_location("hft_release", Path(__file__).parents[1] / "release/release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
sys.modules["release"] = release
sys.path.insert(0, str(Path(__file__).parents[1] / "release"))


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).parents[1] / "release" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def fixture(tmp_path, channel="release", host_class="dev2", mutation=None):
    root = tmp_path / "host"
    local = root / "etc/hft"
    local.mkdir(parents=True, exist_ok=True)
    (local / "host-local.toml").write_text('manifest = "test-host"\n')
    ver = "v1.2.3-dev.4" if channel == "dev" else "v1.2.3"
    elf = bytearray(64)
    elf[:6] = b"\x7fELF\x02\x01"
    elf[18:20] = (183).to_bytes(2, "little")
    files = {
        "bin/hft-core": bytes(elf), "bin/hostprobe": bytes(elf),
        "python/requirements.lock": b"", "python/wheelhouse/.keep": b"",
        "deploy/hosts/test-host.toml": ('class = "' + host_class + '"\n').encode(),
        "deploy/tuning/test-host.toml": b"schema = 1\n",
        "deploy/generated/test-host/hft-core@prod.service": b"[Service]\nUser=hft\nSlice=hft-core.slice\n",
    }
    files["deploy/generated/test-host/install.json"] = json.dumps({
        "groups": {"core": [{"source": "deploy/generated/test-host/hft-core@prod.service", "destination": "etc/systemd/system/hft-core@prod.service"}], "sim": [], "cold": []},
        "units": {"core": ["hft-core@prod.service"], "sim": ["hft-core@sim.service"], "cold": ["hft-watchtower.service"]},
    }).encode()
    manifest = {"version": ver, "channel": channel, "target": "aarch64-unknown-linux-gnu", "git_sha": "a" * 40,
                "host_expect": {"test-host": hashlib.sha256(files["deploy/tuning/test-host.toml"]).hexdigest()},
                "files": [{"path": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()} for name, data in sorted(files.items())]}
    if mutation:
        mutation(files, manifest)
    files["RELEASE.json"] = json.dumps(manifest).encode()
    archive = tmp_path / "fixture.tar"
    with tarfile.open(archive, "w") as tar:
        for name, data in files.items():
            info = tarfile.TarInfo("hft-" + ver + "-aarch64/" + name)
            info.size, info.mode = len(data), 0o755 if name.startswith("bin/") else 0o644
            tar.addfile(info, io.BytesIO(data))
    checksum = tmp_path / "fixture.sha256"
    checksum.write_text(release.sha256(archive) + "  fixture.tar\n")
    return root, archive, checksum


class Runner:
    def __init__(self):
        self.calls = []

    def __call__(self, argv):
        argv = [str(x) for x in argv]
        self.calls.append(argv)
        if argv[:2] == ["systemctl", "show"]:
            return b"LoadState=loaded\nActiveState=inactive\n"
        if argv[1:3] == ["-m", "venv"]:
            Path(argv[-1]).mkdir()


def dep_004_dev_build_dev2_only(tmp_path):
    for cls in ["dev2", "c4", "c32"]:
        directory = tmp_path / cls
        directory.mkdir()
        root, archive, checksum = fixture(directory, "dev", cls)
        runner = Runner()
        if cls == "dev2":
            release.install(archive, checksum, root, runner)
            assert (root / "opt/hft/releases/v1.2.3-dev.4").is_dir()
        else:
            with pytest.raises(release.ReleaseError, match="requires dev2"):
                release.install(archive, checksum, root, runner)
            assert not (root / "opt/hft/releases").exists()
            assert runner.calls == []


def dep_014_manifest_selection(tmp_path):
    root, archive, checksum = fixture(tmp_path)
    (root / "etc/hft/host-local.toml").write_text('manifest = "missing"')
    with pytest.raises((release.ReleaseError, OSError)):
        release.install(archive, checksum, root, Runner())
    assert not (root / "opt/hft/releases").exists()


def dep_223_install_ok(tmp_path):
    root, archive, checksum = fixture(tmp_path)
    base = root / "opt/hft"
    old = base / "releases/v1.2.2"
    old.mkdir(parents=True)
    (old / "unchanged").write_bytes(b"immutable")
    # Symlink operations are verified on Linux, the deployment target.
    if os.name == "posix":
        for group in ("core", "sim", "cold"):
            (base / ("current-" + group)).symlink_to("releases/v1.2.2")
    runner = Runner()
    release.install(archive, checksum, root, runner)
    assert (base / "releases/v1.2.3/venv").is_dir()
    assert (old / "unchanged").read_bytes() == b"immutable"
    assert all("systemctl" not in c for c in runner.calls)
    pip = next(c for c in runner.calls if "pip" in c)
    assert "--no-index" in pip and "--require-hashes" in pip
    record = json.loads((base / "installations/v1.2.3.json").read_text())
    assert record["archive_sha256"] == release.sha256(archive)
    if os.name == "posix":
        for path in (base / "releases/v1.2.3").rglob("*"):
            assert not path.stat().st_mode & 0o022
        assert all(os.readlink(base / ("current-" + group)) == "releases/v1.2.2" for group in ("core", "cold", "sim"))
    with pytest.raises(release.ReleaseError, match="already installed"):
        release.install(archive, checksum, root, Runner())


def dep_224_install_rejects(tmp_path):
    def alter_profile(files, manifest):
        name = "deploy/tuning/test-host.toml"
        files[name] = b"tampered\n"
        row = next(r for r in manifest["files"] if r["path"] == name)
        row.update(bytes=len(files[name]), sha256=hashlib.sha256(files[name]).hexdigest())
    mutations = [
        lambda f, m: f.update({"extra": b"extra"}),
        lambda f, m: f.pop("bin/hostprobe"),
        lambda f, m: f.update({"bin/hft-core": b"wrong"}),
        lambda f, m: m.update(target="x86_64-unknown-linux-gnu"),
        alter_profile,
    ]
    for index, mutation in enumerate(mutations):
        directory = tmp_path / str(index)
        directory.mkdir()
        root, archive, checksum = fixture(directory, mutation=mutation)
        with pytest.raises((release.ReleaseError, OSError)):
            release.install(archive, checksum, root, Runner())
        assert not (root / "opt/hft/releases").exists()
    directory = tmp_path / "failed-version"
    directory.mkdir()
    root, archive, checksum = fixture(directory)
    def fail_version(argv):
        raise OSError("failed version probe")
    with pytest.raises(OSError):
        release.install(archive, checksum, root, fail_version)
    assert not (root / "opt/hft/releases").exists()
    def fail_venv(argv):
        if "venv" in argv:
            raise OSError("failed offline environment")
    with pytest.raises(OSError):
        release.install(archive, checksum, root, fail_venv)
    assert not (root / "opt/hft/releases/v1.2.3").exists()
    assert len(list((root / "opt/hft/releases").glob("v1.2.3.failed.*"))) == 1
    for index, kind in enumerate(("parent", "absolute", "symlink", "device", "roots", "duplicate")):
        directory = tmp_path / ("tar" + str(index))
        directory.mkdir()
        root, archive, checksum = fixture(directory)
        with tarfile.open(archive, "a") as tar:
            name = {"parent": "../evil", "absolute": "/etc/evil", "roots": "other/file", "duplicate": "hft-v1.2.3-aarch64/bin/hft-core"}.get(kind, "hft-v1.2.3-aarch64/evil")
            info = tarfile.TarInfo(name)
            if kind == "symlink":
                info.type, info.linkname = tarfile.SYMTYPE, "/etc"
            if kind == "device":
                info.type = tarfile.CHRTYPE
            tar.addfile(info, io.BytesIO())
        checksum.write_text(release.sha256(archive))
        with pytest.raises(release.ReleaseError):
            release.install(archive, checksum, root, Runner())
        assert not (root / "opt/hft/releases").exists()


@pytest.mark.skipif(os.name != "posix", reason="Linux activation uses POSIX symlinks")
def dep_225_activate_ok(tmp_path):
    root, archive, checksum = fixture(tmp_path)
    runner = Runner()
    release.install(archive, checksum, root, runner)
    base = root / "opt/hft"
    for group in ("core", "cold", "sim"):
        (base / ("current-" + group)).symlink_to("releases/v1.2.2")
    runner.calls.clear()
    def first_boot(argv):
        output = runner(argv)
        args = [str(x) for x in argv]
        if args[0] == "install":
            assert args[1:] == ["-d", "-o", "hft", "-g", "hft-journal-ro", "-m", "2750", str(root / "data/prod")]
            assert ["systemctl", "disable", "--now", "hft-core@prod.service"] in runner.calls
            Path(args[-1]).mkdir(parents=True)
        if "verify" in args and args[0].endswith("hostprobe"):
            assert (root / "data/prod").is_dir(), "first activation must prepare the cold path"
        return output
    record = release.activate("v1.2.3", "core", root, first_boot)
    assert record["acceptance"] == "pending" and record["hostprobe"] == "passed"
    assert os.readlink(base / "current-core") == "releases/v1.2.3"
    assert os.readlink(base / "previous-core") == "releases/v1.2.2"
    assert os.readlink(base / "current-sim") == "releases/v1.2.2"
    assert os.readlink(base / "current-cold") == "releases/v1.2.2"
    assert runner.calls[-1][:2] == [str(base / "current-core/bin/hostprobe"), "verify"]
    assert "--expected" in runner.calls[-1] and "--out" in runner.calls[-1]
    assert [c[1] for c in runner.calls if c[0] == "systemctl"] == ["show", "disable", "daemon-reload", "disable"]
    assert (root / "etc/systemd/system/hft-core@prod.service").is_file()


@pytest.mark.skipif(os.name != "posix", reason="Linux activation uses POSIX symlinks")
def dep_225_first_activation_absent_unit(tmp_path):
    # deploy/conftest.py collects dep_* without parametrize support: loop explicitly.
    for active in ("inactive", "active"):
        _first_activation_absent_unit(tmp_path / active, active)


def _first_activation_absent_unit(tmp_path, active):
    tmp_path.mkdir()
    root, archive, checksum = fixture(tmp_path)
    runner = Runner()
    release.install(archive, checksum, root, runner)
    base = root / "opt/hft"
    def missing(argv):
        result = runner(argv)
        if list(argv)[:2] == ["systemctl", "show"]:
            return ("LoadState=not-found\nActiveState=" + active + "\n").encode()
        if list(argv)[:2] == ["systemctl", "disable"]:
            assert (root / "etc/systemd/system/hft-core@prod.service").is_file()
            assert ["systemctl", "daemon-reload"] in runner.calls
        return result
    if active == "active":
        with pytest.raises(release.ReleaseError, match="missing unit is not inactive"):
            release.activate("v1.2.3", "core", root, missing)
        assert not (base / "current-core").exists()
    else:
        release.activate("v1.2.3", "core", root, missing)
        assert [c[1] for c in runner.calls if c[0] == "systemctl"] == ["show", "daemon-reload", "disable"]


def dep_222_fetch(tmp_path, monkeypatch):
    fetcher = load("fetch")
    token = tmp_path / "etc/hft/github-release-token"
    token.parent.mkdir(parents=True)
    token.write_text("fixture-token")
    token.chmod(0o600)
    # Exercise rejection with the real unprivileged owner before supplying a root-owned fixture.
    if os.name == "posix" and os.getuid() != 0:
        with pytest.raises(release.ReleaseError, match="root-owned"):
            fetcher.fetch("v1.2.3", False, "production", tmp_path, dry_run=True)
        original = Path.lstat
        def fixture_stat(path, *args, **kwargs):
            info = original(path, *args, **kwargs)
            if path == token:
                fields = list(info)
                fields[4] = 0
                return os.stat_result(fields)
            return info
        monkeypatch.setattr(Path, "lstat", fixture_stat)
    for ver, prerelease, asset in [("v1.2.3", False, "production"), ("v1.2.3-dev.4", True, "production"), ("v1.2.3", False, "verify")]:
        name = ("hft-verify-" if asset == "verify" else "hft-") + ver + "-aarch64.tar.zst"
        calls = []
        def download(url, dest, supplied_token, binary=False):
            calls.append(url)
            assert supplied_token == "fixture-token"
            if not binary:
                dest.write_text(json.dumps(dict(tag_name=ver, draft=False, prerelease=prerelease, assets=[dict(name=name, id=1), dict(name=name + ".sha256", id=2)])))
            elif url.endswith("/1"):
                dest.write_bytes(b"fixture archive")
            else:
                dest.write_text(hashlib.sha256(b"fixture archive").hexdigest())
        result = fetcher.fetch(ver, prerelease, asset, tmp_path, download)
        assert result.name == name and result.read_bytes() == b"fixture archive"
        assert len(calls) == 3
    def missing(*args):
        raise OSError("404")
    with pytest.raises(OSError):
        fetcher.fetch("v9.0.0", False, "production", tmp_path, missing)


def dep_003_artifact_names(tmp_path):
    for channel in ("release", "dev"):
        directory = tmp_path / channel
        directory.mkdir()
        packager = load("package")
        root, archive, checksum = fixture(directory, channel=channel)
        extracted = directory / "extracted"
        extracted.mkdir()
        tree = release.unpack(archive, extracted)
        metadata = json.loads((tree / "RELEASE.json").read_text())
        (tree / "RELEASE.json").unlink()
        with pytest.raises(release.ReleaseError, match="outside its staging tree"):
            packager.package_tree(tree, tree / "nested-output", metadata, 0, compress=False)
        first = packager.package_tree(tree, directory / "one", metadata, 0, compress=False)
        second = packager.package_tree(tree, directory / "two", metadata, 0, compress=False)
        assert first.read_bytes() == second.read_bytes()
        assert first.name == "hft-" + metadata["version"] + "-aarch64.tar"
        assert (first.parent / (first.name + ".sha256")).is_file()
        with tarfile.open(first) as tar:
            assert all(m.uid == m.gid == m.mtime == 0 for m in tar.getmembers())
            assert tar.getnames() == sorted(tar.getnames())
        output = directory / "verify"
        output.mkdir()
        checked = release.verify_tree(release.unpack(first, output))
        assert checked["git_sha"] == metadata["git_sha"] and checked["channel"] == channel


def preflight_host(tmp_path, monkeypatch):
    preflight = load("preflight")
    root, archive, checksum = fixture(tmp_path)
    extracted = tmp_path / "extracted"
    extracted.mkdir()
    tree = release.unpack(archive, extracted)
    manifest = release.verify_tree(tree)
    manifest["cpu_features"] = ["aes", "atomics"]
    monkeypatch.setattr(preflight, "verify_tree", lambda *args, **kw: manifest)
    monkeypatch.setattr(preflight, "selected_host", lambda *args: ("test-host", dict(**{"class": "dev2"}, instances=["prod"], formal_live=False)))
    for path, text in {
        "etc/hft/hft-core-prod.toml": '[instance]\nname="prod"\norder_env="testnet"\n[paths]\njournal_cold="/data"\n',
        "etc/hft/preflight.toml": "disk_free_block_pct=15\nmemory=768\nparent_memory=1376\nbatch_memory=512\nquota=100\nweight=50\nread=40\nwrite=20\n",
        "proc/cpuinfo": "Features: aes atomics\n",
        "sys/fs/cgroup/hftbatch.slice/cpu.idle": "1",
        "sys/fs/cgroup/cgroup.subtree_control": "cpu memory io",
        "sys/devices/system/cpu/isolated": "",
        "sys/fs/cgroup/hft.slice/cpu.max": "100000 100000",
        "sys/fs/cgroup/hft.slice/cpu.weight": "50",
        "sys/fs/cgroup/hftbatch.slice/cpu.max": "max 100000",
        "sys/fs/cgroup/hftbatch.slice/io.max": "259:1 rbps=40000000 wbps=20000000",
        "run/hft/hostprofile.json": '{"pending_reboot":false}',
        "etc/hft/secrets/hft-core.toml": '[[secret]]\nsecret_id="arn:aws:secretsmanager:ap-northeast-1:123456789012:secret:hft/testnet/fixture"\n',
    }.items():
        target = root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
    for directory, limit in [("hft.slice", 1376), ("hftbatch.slice", 512), ("hft.slice/hft-core.slice/hft-core@prod.service", 768)]:
        for field, value in [("memory.max", str(limit * 1024 * 1024)), ("memory.swap.max", "0")]:
            target = root / "sys/fs/cgroup" / directory / field
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(value)
    for unit in ("hft.slice", "hft-core.slice", "hftbatch.slice"):
        for target in (tree / "deploy/generated/test-host" / unit, root / "etc/systemd/system" / unit):
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text("[Slice]\n")
    for target in (root / "etc/polkit-1/rules.d/50-hft-dev2-guard.rules", tree / "deploy/polkit/50-hft-dev2-guard.rules"):
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("fixture policy")
    rules = [{"set": {"name": "hft_uids", "elem": [1001, 1002]}}, {"chain": {"name": "out", "type": "filter", "hook": "output"}}]
    for protocol, address in (("ip", "169.254.169.254"), ("ip6", "fd00:ec2::254")):
        rules.append({"rule": {"chain": "out", "expr": [{"match": {"op": "==", "left": {"meta": {"key": "skuid"}}, "right": "@hft_uids"}}, {"match": {"op": "==", "left": {"payload": {"protocol": protocol, "field": "daddr"}}, "right": address}}, {"drop": None}]}})
    class Host(preflight.Host):
        free = 15000
        compilers = set()
        chrony = "Leap status : Normal\nLast offset : 0.005000 seconds"
        active = True
        machine = "aarch64"
        denied = True
        def path(self, path):
            return tree if path == "/opt/hft/current-core" else super().path(path)
        def command(self, *argv):
            if argv[0] == "uname":
                return self.machine
            if argv[0] == "findmnt":
                return "259:1" if "MAJ:MIN" in argv else argv[-1]
            if argv[0] == "chronyc":
                return self.chrony
            if argv[0] == "systemctl":
                if "--property=OOMScoreAdjust" in argv:
                    return "100"
                return "active" if self.active else "inactive"
            if argv[0] == "nft":
                return json.dumps({"nftables": rules})
            if Path(argv[0]).name in ("hostprobe", "hft-secrets", "hft-core"):
                return ""
            raise AssertionError(argv)
        def available(self, name):
            return name in self.compilers
        def space(self, path):
            return self.free, 100000
        def service_uids(self):
            return {1001, 1002}
        def live_secret_denied(self):
            return self.denied
    return preflight, Host(root)


def dep_235_common_items(tmp_path, monkeypatch):
    # Run the shipped entrypoint in a clean process: importing a sibling must not add files
    # to an immutable tree before verify_tree checks its exact file set.
    scripts = tmp_path / "shipped-preflight"
    scripts.mkdir()
    for name in ("preflight.py", "fetch.py", "release.py", "cpu.py", "local_secrets.py"):
        (scripts / name).write_bytes((Path(__file__).parents[1] / "release" / name).read_bytes())
    before = {p.name: p.read_bytes() for p in scripts.iterdir()}
    env = {k: v for k, v in os.environ.items() if k not in ("PYTHONDONTWRITEBYTECODE", "PYTHONPYCACHEPREFIX")}
    for entrypoint in ("preflight.py", "fetch.py"):
        result = subprocess.run([sys.executable, str(scripts / entrypoint), "--help"], env=env, capture_output=True, text=True)
        assert result.returncode == 0, result.stderr
        assert sorted(p.name for p in scripts.iterdir()) == sorted(before), "entrypoint import polluted the release"
    assert all((scripts / name).read_bytes() == content for name, content in before.items())
    preflight, host = preflight_host(tmp_path, monkeypatch)
    assert all(preflight.check(host).values())
    for field, bad, failed in [("free", 14999, "disk-free"), ("machine", "x86_64", "architecture"), ("chrony", "Leap status : Normal\nLast offset : -0.005001 seconds", "chrony"), ("chrony", "Leap status : Not synchronised\nLast offset : 0.0 seconds", "chrony"), ("compilers", {"cargo"}, "no-compiler")]:
        old = getattr(host, field)
        setattr(host, field, bad)
        assert not preflight.check(host)[failed]
        setattr(host, field, old)
    host.path("/run/hft/hostprofile.json").unlink()
    assert not preflight.check(host)["host-profile"]


def dep_235_lifecycle_entrypoints_preserve_release(tmp_path):
    import shutil
    for n, entry in enumerate(("dev2/hft-window", "dev2/hft-dev2-guard", "bin/hft-shm-release")):
        scripts = tmp_path / str(n) / "deploy"
        shutil.copytree(Path(__file__).parents[1], scripts, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        before = {str(p.relative_to(scripts)): p.read_bytes() for p in scripts.rglob("*") if p.is_file()}
        env = {k: v for k, v in os.environ.items() if k not in ("PYTHONDONTWRITEBYTECODE", "PYTHONPYCACHEPREFIX")}
        result = subprocess.run([sys.executable, str(scripts / entry), "--help"], env=env, capture_output=True, text=True)
        assert result.returncode == 0, result.stderr
        after = {str(p.relative_to(scripts)): p.read_bytes() for p in scripts.rglob("*") if p.is_file()}
        assert after == before, "lifecycle entrypoint import polluted the immutable release: " + entry


def dep_237_dev2_items(tmp_path, monkeypatch):
    preflight, host = preflight_host(tmp_path, monkeypatch)
    host.compilers = {"gcc", "cc"}
    assert all(preflight.check(host).values())
    host.compilers = {"rustc"}
    assert not preflight.check(host)["no-compiler"]
    host.compilers = set()
    host.active = False
    assert not preflight.check(host)["guard-active"]
    host.active = True
    host.denied = False
    assert not preflight.check(host)["live-secret-denied"]
    host.denied = True
    host.path("/etc/polkit-1/rules.d/50-hft-dev2-guard.rules").write_text("allow foreign service")
    assert not preflight.check(host)["guard-polkit"]
    host.path("/sys/fs/cgroup/hftbatch.slice/cpu.idle").write_text("0")
    assert not preflight.check(host)["batch-idle"]
