"""DEP-168 local demo sources remain closed to production and unsafe file layouts."""
from pathlib import Path
from types import SimpleNamespace
import stat
import pytest
from test_release import load

local = load("local_secrets")


def fixture():
    config = {"instance": {"name": "prod", "order_env": "testnet", "secrets_mode": "local_demo"},
              "venues": {"demo": {"accounts": [{"api_key_path": str(local.OUTPUT / "api")}]}}}
    manifest = {"mode": "local_demo", "secret": [{"name": "api", "path": "/run/secrets/demo/api"}]}
    def metadata(path):
        directory = Path(path).name != "api"
        return SimpleNamespace(st_uid=0, st_mode=(stat.S_IFDIR | 0o700) if directory else (stat.S_IFREG | 0o600), st_size=32)
    return config, manifest, metadata


def dep_168_demo_secret_source():
    config, manifest, metadata = fixture()
    assert local.entries(config, manifest, lstat=metadata) == {str(local.OUTPUT / "api"): Path("/run/secrets/demo/api")}
    for field, value in (("name", "sim"), ("order_env", "prod"), ("secrets_mode", "aws_secrets_manager")):
        original = config["instance"][field]
        config["instance"][field] = value
        with pytest.raises(ValueError, match="demo/testnet"):
            local.entries(config, manifest, lstat=metadata)
        config["instance"][field] = original
    for path in ("/etc/api", "/run/secrets/api", "/run/secrets/demo/../../../etc/api"):
        manifest["secret"][0]["path"] = path
        with pytest.raises(ValueError):
            local.entries(config, manifest, lstat=metadata)


def dep_168_demo_secret_permissions():
    config, manifest, metadata = fixture()
    for uid, kind, mode in [(1000, stat.S_IFREG, 0o600), (0, stat.S_IFREG, 0o644), (0, stat.S_IFLNK, 0o600), (0, stat.S_IFDIR, 0o755)]:
        def bad(path):
            info = metadata(path)
            if Path(path).name == ("demo" if kind == stat.S_IFDIR else "api"):
                info.st_uid, info.st_mode = uid, kind | mode
            return info
        with pytest.raises(ValueError, match="owner/mode"):
            local.entries(config, manifest, lstat=bad)


def dep_168_local_secrets_exact_references():
    config, manifest, metadata = fixture()
    config["venues"]["demo"]["accounts"][0]["rest_api_key_path"] = "/run/secrets/other/api"
    with pytest.raises(ValueError, match="match all"):
        local.entries(config, manifest, lstat=metadata)
