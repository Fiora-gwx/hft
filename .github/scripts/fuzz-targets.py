#!/usr/bin/env python3
"""Read tools/xtask/fuzz-targets.toml for the workflows.

The file lands with `cargo xtask fuzz` (M3; contents: tools/README.md 实现说明). Until its exact
shape is fixed there, both `[[target]]` arrays and `[target.<name>]` tables are accepted, with
`name`, `crate` or `dir` (the crate directory that holds `fuzz/`), `core` (bool, core decoder target
packed into the verification bundle) and `paths` (extra source paths).

CLI: fuzz-targets.py --core   prints "<name> <crate dir>" per core target (nothing if no file).
"""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

CFG = Path("tools/xtask/fuzz-targets.toml")


def entries() -> list[dict]:
    if not CFG.exists():
        return []
    with CFG.open("rb") as f:
        t = tomllib.load(f)
    raw = t.get("target", t.get("targets", []))
    items = [dict(v, name=k) for k, v in raw.items()] if isinstance(raw, dict) else list(raw)
    out = []
    for e in items:
        if not e.get("name"):
            continue
        out.append(
            {
                "name": str(e["name"]),
                "dir": str(e.get("crate") or e.get("dir") or "").rstrip("/"),
                "core": bool(e.get("core", False)),
                "paths": [str(p).rstrip("/") for p in e.get("paths", [])],
            }
        )
    return out


def main(argv: list[str]) -> int:
    if argv == ["--core"]:
        for e in entries():
            if e["core"]:
                if not e["dir"]:
                    print(f"fuzz target {e['name']} has no crate dir", file=sys.stderr)
                    return 2
                print(e["name"], e["dir"])
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
