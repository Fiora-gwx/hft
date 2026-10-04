#!/usr/bin/env python3
"""Turn a decoupling-gate exemption into a `decoupling-exemption` notice annotation.

`cargo xtask decoupling-gate` writes each exemption (PR number, approver, changed engine crates and
apps) to $GITHUB_STEP_SUMMARY and target/xtask/decoupling-gate.json (tools/README.md). Step
summaries cannot be read back through the API, annotations can: nightly-summary.py collects these
notices for the nightly "decoupling exemptions" section. The JSON key is `exemption` (an object or
a list); nothing is printed when it is absent or empty.
"""

import json
import sys
from pathlib import Path

p = Path("target/xtask/decoupling-gate.json")
if not p.exists():
    sys.exit(0)
try:
    data = json.loads(p.read_text(encoding="utf-8"))
except ValueError as e:
    print(f"::warning::cannot parse {p}: {e}")
    sys.exit(0)
ex = data.get("exemption") if isinstance(data, dict) else None
if ex:
    text = json.dumps(ex, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    print(f"::notice title=decoupling-exemption::{text}")
