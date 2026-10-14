#!/usr/bin/python3
"""Fetch exact release assets without exposing the GitHub token to child processes."""
import argparse
import json
import os
from pathlib import Path
import stat
import tempfile
import urllib.parse
import urllib.request

from release import ReleaseError, sha256, version


def download(url, destination, token, binary=False):
    # Never forward credentials to release-asset redirects on a different host.
    class Redirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, request, fp, code, msg, headers, newurl):
            redirected = super().redirect_request(request, fp, code, msg, headers, newurl)
            if urllib.parse.urlparse(newurl).netloc != "api.github.com":
                redirected.remove_header("Authorization")
            return redirected
    request = urllib.request.Request(url, headers={
        "Authorization": "Bearer " + token,
        "Accept": "application/octet-stream" if binary else "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
    })
    with urllib.request.build_opener(Redirect()).open(request, timeout=60) as response, destination.open("xb") as stream:
        while data := response.read(1024 * 1024):
            stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def fetch(ver, prerelease, asset, root, downloader=download, dry_run=False):
    version(ver)
    if ("-dev." in ver) != prerelease or asset not in ("production", "verify"):
        raise ReleaseError("release selection mismatch")
    root = Path(root)
    token_path = root / "etc/hft/github-release-token"
    info = token_path.lstat()
    if not stat.S_ISREG(info.st_mode) or (os.name == "posix" and (info.st_uid != 0 or stat.S_IMODE(info.st_mode) != 0o600)):
        raise ReleaseError("token file must be root-owned mode 0600")
    token = token_path.read_text().strip()
    if not token or any(c.isspace() for c in token):
        raise ReleaseError("invalid token file")
    name = ("hft-verify-" if asset == "verify" else "hft-") + ver + "-aarch64.tar.zst"
    staging = root / "opt/hft/staging"
    if dry_run:
        return staging / name
    staging.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=staging) as directory:
        temporary = Path(directory)
        metadata = temporary / "metadata.json"
        base = "https://api.github.com/repos/Fiora-gwx/hft"
        downloader(base + "/releases/tags/" + ver, metadata, token)
        release = json.loads(metadata.read_text())
        if release.get("tag_name") != ver or release.get("draft") or release.get("prerelease") != prerelease:
            raise ReleaseError("GitHub release metadata mismatch")
        for filename in (name, name + ".sha256"):
            matches = [a for a in release["assets"] if a["name"] == filename]
            if len(matches) != 1 or type(matches[0]["id"]) is not int:
                raise ReleaseError("release asset missing or ambiguous")
            downloader(base + "/releases/assets/" + str(matches[0]["id"]), temporary / filename, token, True)
        expected = (temporary / (name + ".sha256")).read_text().split()[0].lower()
        if sha256(temporary / name) != expected:
            raise ReleaseError("downloaded archive checksum mismatch")
        # Publish only a complete, verified pair; installation verifies it again.
        for filename in (name, name + ".sha256"):
            target = staging / filename
            if target.exists() and sha256(target) != sha256(temporary / filename):
                raise ReleaseError("immutable staging asset differs")
        for filename in (name, name + ".sha256"):
            (temporary / filename).replace(staging / filename)
    return staging / name


def main():
    parser = argparse.ArgumentParser()
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument("--version")
    selection.add_argument("--prerelease")
    parser.add_argument("--asset", choices=("production", "verify"), default="production")
    parser.add_argument("--root", type=Path, default=Path("/"))
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    try:
        print(fetch(args.version or args.prerelease, args.prerelease is not None, args.asset, args.root, dry_run=args.dry_run))
        return 0
    except Exception as error:
        print("fetch-release failed: " + type(error).__name__)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
