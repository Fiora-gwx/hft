#!/usr/bin/python3
"""Remove only journal hot segments of a stopped instance, preserving heartbeat/evidence."""
import os
from pathlib import Path
import re
import struct


def release(root, instance, dry_run=False):
    if instance not in {"prod", "sim"}:
        raise ValueError("unknown instance")
    root = Path(root).resolve(strict=True)
    directory = root / instance
    if directory.is_symlink() or not directory.is_dir():
        return {"instance": instance, "state": None, "deleted": False, "files": 0}
    heartbeat = directory / "heartbeat"
    state = None
    if heartbeat.is_file() and not heartbeat.is_symlink():
        with heartbeat.open("rb") as stream:
            header = stream.read(4097)
        if (len(header) == 4096 and header[128:136] == b"HFTHB\0\x01\0"
                and struct.unpack_from("<I", header, 136)[0] == 1
                and header[144] == (1 if instance == "prod" else 2)):
            state = header[146]
    result = {"instance": instance, "state": state, "deleted": False, "files": 0}
    if state != 5:
        return result
    files = []
    for current, directories, names in os.walk(directory, followlinks=False):
        current = Path(current)
        # Refuse the entire operation before deleting anything if the tree contains links.
        if any((current / name).is_symlink() for name in directories + names):
            raise ValueError("symlink in instance journal tree")
        for name in names:
            if re.fullmatch(r"[0-9]{10,20}\.jrn(?:\.preparing)?", name):
                path = current / name
                if not path.resolve().is_relative_to(directory):
                    raise ValueError("segment outside instance directory")
                files.append(path)
    for path in files:
        if not dry_run:
            path.unlink()
    result.update(deleted=not dry_run, files=len(files))
    return result
