import importlib.util
from pathlib import Path
import struct
import pytest

spec = importlib.util.spec_from_file_location("shm_release", Path(__file__).resolve().parents[1] / "bin/shm_release.py")
shm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shm)


def instance(root, state, magic=b"HFTHB\0\x01\0"):
    base = root / "sim"
    (base / "seq").mkdir(parents=True)
    data = bytearray(4096)
    data[128:136] = magic
    struct.pack_into("<I", data, 136, 1)
    data[144] = 2
    data[146] = state
    (base / "heartbeat").write_bytes(data)
    for i in range(3):
        (base / "seq" / f"{i + 1:010}.jrn").write_bytes(b"journal")
    (base / "seq" / "evidence.txt").write_bytes(b"preserve")
    return base


def core_195_release_only_when_stopped(tmp_path):
    for state in (5, 4, 3, 1, 0, 6):
        root = tmp_path / str(state)
        base = instance(root, state)
        if state == 0:
            (base / "heartbeat").unlink()
        if state == 6:
            (base / "heartbeat").write_bytes(b"corrupt")
        result = shm.release(root, "sim")
        assert result["deleted"] == (state == 5)
        assert len(list(base.rglob("*.jrn"))) == (0 if state == 5 else 3)
        assert (base / "seq/evidence.txt").read_bytes() == b"preserve"
        if state != 0:
            assert (base / "heartbeat").exists()


def test_invalid_instance_and_dry_run(tmp_path):
    base = instance(tmp_path, 5)
    with pytest.raises(ValueError):
        shm.release(tmp_path, "../prod")
    assert shm.release(tmp_path, "sim", True)["files"] == 3
    assert len(list(base.rglob("*.jrn"))) == 3


def test_symlink_never_escapes_instance(tmp_path):
    base = instance(tmp_path, 5)
    other = tmp_path / "prod"
    other.mkdir()
    (other / "0000000001.jrn").write_bytes(b"other instance")
    try:
        (base / "alias").symlink_to(other, target_is_directory=True)
    except OSError:
        pytest.skip("symlink privilege unavailable")
    with pytest.raises(ValueError):
        shm.release(tmp_path, "sim")
    assert len(list((base / "seq").glob("*.jrn"))) == 3
    assert (other / "0000000001.jrn").read_bytes() == b"other instance"
