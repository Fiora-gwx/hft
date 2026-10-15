#!/usr/bin/python3
"""Explicit demo-only local credential source. Validation never reads credential contents."""
import os
from pathlib import Path, PurePosixPath
import re
import stat

OUTPUT = PurePosixPath("/run/hft/core-prod/secrets")


def entries(config, manifest, root=Path("/"), lstat=os.lstat):
    instance = config["instance"]
    if instance.get("secrets_mode") != "local_demo" or instance.get("order_env") != "testnet" or instance.get("name") != "prod":
        raise ValueError("local secrets require explicit demo/testnet mode")
    if set(manifest) != {"mode", "secret"} or manifest["mode"] != "local_demo":
        raise ValueError("local secret manifest mode")
    result = {}
    for item in manifest["secret"]:
        if set(item) != {"name", "path"} or not re.fullmatch(r"[a-zA-Z0-9_-]+", item["name"]):
            raise ValueError("local secret entry")
        path = PurePosixPath(item["path"])
        if not path.is_relative_to("/run/secrets") or ".." in path.parts or len(path.parts) < 5:
            raise ValueError("local secret must be inside a private /run/secrets directory")
        for directory in reversed(path.parents):
            if directory == PurePosixPath("/run/secrets") or directory.is_relative_to("/run/secrets"):
                meta = lstat(root / str(directory).lstrip("/"))
                if meta.st_uid != 0 or not stat.S_ISDIR(meta.st_mode) or stat.S_IMODE(meta.st_mode) != 0o700:
                    raise ValueError("local secret directory owner/mode")
        source = root / str(path).lstrip("/")
        meta = lstat(source)
        if meta.st_uid != 0 or not stat.S_ISREG(meta.st_mode) or stat.S_IMODE(meta.st_mode) not in (0o400, 0o600) or not 0 < meta.st_size <= 4096:
            raise ValueError("local secret file owner/mode/size")
        destination = str(OUTPUT / item["name"])
        if destination in result:
            raise ValueError("duplicate local secret destination")
        result[destination] = source
    references = set()
    for venue in config["venues"].values():
        for keys in venue.get("accounts", []):
            for key in ("api_key_path", "session_seed_path", "session_secret_path", "rest_api_key_path", "rest_secret_path", "passphrase_path"):
                if key in keys:
                    references.add(keys[key])
    if not result or references != result.keys():
        raise ValueError("local secret destinations must match all configured credential paths")
    return result
