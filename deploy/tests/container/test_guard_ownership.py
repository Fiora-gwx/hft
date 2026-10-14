"""Real setuid/file ownership tests in disposable CI containers, never on a developer host."""
import os
from pathlib import Path
import shutil
import subprocess

import pytest

pytestmark = pytest.mark.skipif(os.environ.get("CI") != "true" or not shutil.which("docker"), reason="requires disposable hosted-CI Docker container")


def container(script):
    repo = Path(__file__).resolve().parents[3]
    result = subprocess.run(["docker", "run", "--rm", "-i", "--network=none", "-v", str(repo) + ":/repo:ro", "python:3.12-slim", "python", "-"], input=script, text=True, capture_output=True, timeout=120)
    assert result.returncode == 0, result.stdout + result.stderr


SETUP = r'''
import json, os, pathlib, pwd, struct, subprocess, sys
subprocess.run(["useradd", "--system", "--no-create-home", "hft-guard"], check=True)
user = pwd.getpwnam("hft-guard")
state = pathlib.Path("/state")
state.mkdir(mode=0o750)
os.chown(state, user.pw_uid, user.pw_gid)
sys.path.insert(0, "/repo/deploy/dev2")
from guard import Effects
def as_guard(action):
    pid = os.fork()
    if pid == 0:
        os.setgroups([])
        os.setgid(user.pw_gid)
        os.setuid(user.pw_uid)
        action()
        os._exit(0)
    assert os.waitpid(pid, 0)[1] == 0
'''


def dep_081_clear_no_root_files():
    container(SETUP + r'''
cfg = pathlib.Path("/config.toml")
cfg.write_text(pathlib.Path("/repo/deploy/dev2/dev2-guard.toml.tmpl").read_text().replace('/var/lib/hft/guard', '/state'))
for existing in (True, False):
    (state / "events.jsonl").unlink(missing_ok=True)
    def setup():
        fx = Effects(state)
        fx.trip({"fixture": True})
        if existing:
            fx.event({"event": 1})
            fx.event({"event": 2})
    as_guard(setup)
    subprocess.run([sys.executable, "/repo/deploy/dev2/hft-dev2-guard", "clear", "--config", str(cfg)], check=True)
    assert not (state / "tripped").exists()
    lines = [json.loads(line) for line in (state / "events.jsonl").read_text().splitlines()]
    assert len(lines) == (3 if existing else 1)
    assert all(p.stat().st_uid == user.pw_uid for p in state.iterdir())
''')


def dep_082_tripped_file():
    container(SETUP + r'''
as_guard(lambda: Effects(state).trip({"first": True}))
path = state / "tripped"
before = (path.stat().st_ino, path.read_bytes())
as_guard(lambda: Effects(state).trip({"second": True}))
assert (path.stat().st_ino, path.read_bytes()) == before
assert path.stat().st_uid == user.pw_uid
assert path.stat().st_mode & 0o777 == 0o644
''')


def dep_084_shm_release_event():
    container(SETUP + r'''
hot = pathlib.Path("/hot/sim")
hot.mkdir(parents=True)
for at, status in enumerate((5, 4)):
    header = bytearray(4096)
    header[128:136] = b"HFTHB\x00\x01\x00"
    struct.pack_into("<I", header, 136, 1)
    header[144], header[146] = 2, status
    (hot / "heartbeat").write_bytes(header)
    for number in range(2):
        (hot / (f"{number:010}.jrn")).write_bytes(b"fixture")
    subprocess.run([sys.executable, "/repo/deploy/bin/hft-shm-release", "sim", "--root", "/hot", "--state-dir", str(state)], check=True)
    rows = [json.loads(line) for line in (state / "events.jsonl").read_text().splitlines()]
    assert len(rows) == at + 1 and rows[-1]["state"] == status
    assert rows[-1]["deleted"] == (status == 5)
    assert len(list(hot.glob("*.jrn"))) == (0 if status == 5 else 2)
    assert (hot / "heartbeat").exists()
    assert all(p.stat().st_uid == user.pw_uid for p in state.iterdir())
''')
