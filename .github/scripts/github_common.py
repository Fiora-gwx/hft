"""GitHub REST reads and the release Admin allowlist; no usage or billing policy."""
from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ADMINS = Path(__file__).resolve().parents[2] / ".github" / "hft-admins.txt"


class ApiError(RuntimeError):
    pass


def api(path: str, params: dict[str, str] | None = None) -> dict:
    """GET one GitHub REST resource (JSON), with a few retries on transient errors."""
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if not token:
        raise ApiError("GITHUB_TOKEN/GH_TOKEN is not set")
    base = os.environ.get("GITHUB_API_URL", "https://api.github.com")
    url = f"{base}{path}"
    if params:
        url += "?" + urllib.parse.urlencode(params)
    req = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    last: Exception | None = None
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            last = e
            if e.code < 500 and e.code != 429:
                raise ApiError(f"GET {path}: HTTP {e.code} {e.reason}") from e
        except (urllib.error.URLError, TimeoutError) as e:
            last = e
        time.sleep(2**attempt)
    raise ApiError(f"GET {path}: {last}")


def paged(path: str, key: str, params: dict[str, str] | None = None, limit: int = 5000):
    """Iterate the items under `key` of a paginated list endpoint."""
    page = 1
    seen = 0
    while True:
        p = dict(params or {})
        p.update({"per_page": "100", "page": str(page)})
        data = api(path, p)
        items = data.get(key, [])
        for it in items:
            yield it
            seen += 1
            if seen >= limit:
                return
        if len(items) < 100:
            return
        page += 1


def admins() -> set[str]:
    out: set[str] = set()
    if not ADMINS.exists():
        return out
    for line in ADMINS.read_text(encoding="utf-8").splitlines():
        login = line.split("#", 1)[0].strip()
        if login:
            out.add(login.lower())
    return out


def is_admin(actor: str) -> bool:
    return bool(actor) and actor.lower() in admins()
