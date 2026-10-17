#!/usr/bin/env python3
"""Release preflight shared by release-aarch64.yml and release-x86-cross.yml.

Rules: docs/development.md#发布工作流release-aarch64yml step 1. It never re-runs quality gates; it
only proves existing verdicts for the SHA being released:
  1. a successful ci.yml run for the SHA whose lane-aware `acceptance` job concluded `success`;
  2. the latest non-skipped check run named `decoupling-gate` on the SHA concluded `success`;
  3. formal vX.Y.Z: the SHA is an ancestor of origin/main and the latest nightly.yml run on main
     that actually executed (its `nightly` job was not skipped) succeeded;
  4. a dispatched dev build needs the tag vX.Y.Z-dev.<dev_n> not to exist yet.
Admin override (`skip_nightly`, only logins of .github/hft-admins.txt): only 3's nightly condition
is not evaluated; 1 and 2 always are. A non-Admin override fails.

verify-bundle.yml's workflow_dispatch(sha) uses WORKFLOW=verify-bundle: green ci.yml + an existing
release tag on the SHA (outputs sha, tag).

Environment (all strings, empty when absent):
  WORKFLOW        aarch64 | x86-cross | verify-bundle
  EVENT           github.event_name
  REF_TYPE, REF_NAME   github.ref_type / github.ref_name (tag pushes)
  IN_SHA, IN_DEV_N, IN_TAG, IN_VERSION, IN_SKIP_NIGHTLY, IN_SKIP_VERIFY   dispatch inputs
  ACTOR           github.actor
  GITHUB_TOKEN, GITHUB_REPOSITORY
Needs a checkout with full history and tags (fetch-depth: 0).
Outputs: sha, tag, version (X.Y.Z), channel (release|dev), create_tag, skip_verify, evidence (JSON).
Exit: 0 pass; 1 a condition failed; 2 input/environment/API error.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tomllib
import github_common as gh

FORMAL = re.compile(r"^v(\d+\.\d+\.\d+)$")
DEV = re.compile(r"^v(\d+\.\d+\.\d+)-dev\.([1-9]\d*)$")


class Fail(Exception):
    """A preflight condition is not met (exit 1)."""


class Bad(Exception):
    """Bad input or environment (exit 2)."""


def env(name: str) -> str:
    return os.environ.get(name, "").strip()


def git(*args: str, check: bool = True) -> str:
    r = subprocess.run(["git", *args], capture_output=True, text=True)
    if check and r.returncode != 0:
        raise Bad(f"git {' '.join(args)}: {r.stderr.strip()}")
    return r.stdout.strip()


def resolve_commit(rev: str) -> str:
    sha = git("rev-parse", "--verify", "--quiet", f"{rev}^{{commit}}", check=False)
    if not sha:
        raise Bad(f"{rev!r} is not a commit in this repository")
    return sha


def tag_exists(tag: str) -> bool:
    if git("rev-parse", "--verify", "--quiet", f"refs/tags/{tag}", check=False):
        return True
    return bool(git("ls-remote", "--tags", "origin", f"refs/tags/{tag}", check=False))


def workspace_version(sha: str) -> str:
    text = git("show", f"{sha}:Cargo.toml")
    v = tomllib.loads(text).get("workspace", {}).get("package", {}).get("version")
    if not v:
        raise Bad("no [workspace.package] version in Cargo.toml")
    return v


def is_ancestor_of_main(sha: str) -> bool:
    r = subprocess.run(["git", "merge-base", "--is-ancestor", sha, "origin/main"])
    return r.returncode == 0


def is_branch_head(sha: str) -> bool:
    return bool(git("for-each-ref", "--points-at", sha, "refs/remotes/origin", check=False))


def job_named(jobs: list[dict], name: str) -> dict | None:
    """A job by name; jobs of a called workflow are named `<caller job> / <job>`."""
    for j in jobs:
        n = j.get("name", "")
        if n == name or n.endswith(f" / {name}"):
            return j
    return None


def check_ci(repo: str, sha: str) -> int:
    runs = gh.paged(
        f"/repos/{repo}/actions/workflows/ci.yml/runs",
        "workflow_runs",
        {"head_sha": sha, "status": "success"},
        limit=50,
    )
    for run in runs:
        jobs = list(gh.paged(f"/repos/{repo}/actions/runs/{run['id']}/jobs", "jobs"))
        acceptance = job_named(jobs, "acceptance")
        if acceptance and acceptance.get("conclusion") == "success":
            return int(run["id"])
    raise Fail(f"1: no successful ci.yml run with a successful `acceptance` job for {sha}")


def check_decoupling(repo: str, sha: str) -> int:
    runs = [
        c
        for c in gh.paged(f"/repos/{repo}/commits/{sha}/check-runs", "check_runs", limit=1000)
        if (c.get("name") == "decoupling-gate" or c.get("name", "").endswith(" / decoupling-gate"))
        and c.get("conclusion") != "skipped"
    ]
    if not runs:
        raise Fail(f"2: no non-skipped `decoupling-gate` check run for {sha}")
    latest = max(runs, key=lambda c: c.get("started_at") or "")
    if latest.get("conclusion") != "success":
        raise Fail(
            f"2: latest `decoupling-gate` check run {latest['id']} is "
            f"{latest.get('status')}/{latest.get('conclusion')}"
        )
    return int(latest["id"])


def check_nightly(repo: str) -> int:
    runs = gh.paged(
        f"/repos/{repo}/actions/workflows/nightly.yml/runs",
        "workflow_runs",
        {"branch": "main", "status": "completed"},
        limit=60,
    )
    for run in runs:
        jobs = list(gh.paged(f"/repos/{repo}/actions/runs/{run['id']}/jobs", "jobs"))
        nightly = job_named(jobs, "nightly")
        if nightly is None or nightly.get("conclusion") == "skipped":
            continue  # An unexecuted job provides no nightly verdict.
        if run.get("conclusion") != "success":
            raise Fail(f"3: latest executed nightly run {run['id']} concluded {run.get('conclusion')}")
        return int(run["id"])
    raise Fail("3: no executed nightly.yml run on main")


def out(**kv: object) -> None:
    path = os.environ.get("GITHUB_OUTPUT")
    lines = [f"{k}={str(v).lower() if isinstance(v, bool) else v}" for k, v in kv.items()]
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    print("\n".join(lines))


def summary(text: str) -> None:
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write(text + "\n")
    print(text, file=sys.stderr)


def decide() -> dict:
    wf, event = env("WORKFLOW"), env("EVENT")
    in_sha, in_dev, in_tag, in_ver = env("IN_SHA"), env("IN_DEV_N"), env("IN_TAG"), env("IN_VERSION")
    create_tag = False
    if wf == "aarch64" and event == "push":
        if env("REF_TYPE") != "tag":
            raise Bad("release-aarch64 push trigger must be a tag")
        tag = env("REF_NAME")
        sha = resolve_commit(f"refs/tags/{tag}")
    elif wf == "aarch64" and event == "workflow_dispatch":
        if in_tag and not (in_sha or in_dev):
            tag = in_tag
            if not FORMAL.match(tag):
                raise Bad(f"input tag {tag!r} is not a formal vX.Y.Z tag")
            if not tag_exists(tag):
                raise Bad(f"tag {tag} has not been pushed")
            sha = resolve_commit(f"refs/tags/{tag}")
        elif in_sha and in_dev and not in_tag:
            sha = resolve_commit(in_sha)
            if not re.fullmatch(r"[1-9]\d*", in_dev):
                raise Bad(f"dev_n {in_dev!r} must be a positive integer")
            tag = f"v{workspace_version(sha)}-dev.{in_dev}"
            create_tag = True
        else:
            raise Bad("dispatch needs either `tag`, or `sha` and `dev_n`")
    elif wf == "x86-cross" and event == "workflow_dispatch":
        if not (in_sha and in_ver):
            raise Bad("release-x86-cross needs `sha` and `version`")
        sha = resolve_commit(in_sha)
        ver = in_ver.removeprefix("v")
        tag = f"v{ver}-dev.{in_dev}" if in_dev else f"v{ver}"
        if tag_exists(tag):
            if resolve_commit(f"refs/tags/{tag}") != sha:
                raise Bad(f"tag {tag} exists and points elsewhere")
        else:
            create_tag = True
    else:
        raise Bad(f"unsupported trigger {wf}/{event}")

    m_formal, m_dev = FORMAL.match(tag), DEV.match(tag)
    if not (m_formal or m_dev):
        raise Bad(f"{tag!r} is neither vX.Y.Z nor vX.Y.Z-dev.N")
    version = (m_formal or m_dev).group(1)
    ws = workspace_version(sha)
    if version != ws:
        raise Fail(f"tag version {version} != workspace version {ws} at {sha}")
    if create_tag and tag_exists(tag):
        raise Fail(f"4: tag {tag} already exists")
    return {
        "sha": sha,
        "tag": tag,
        "version": version,
        "channel": "release" if m_formal else "dev",
        "create_tag": create_tag,
    }


def verify_bundle_dispatch() -> int:
    """verify-bundle.yml workflow_dispatch(sha): the SHA must have a green ci.yml run and a release
    tag (formal preferred, else the newest dev build) pointing at it; outputs sha and tag."""
    try:
        repo = env("GITHUB_REPOSITORY")
        sha = resolve_commit(env("IN_SHA"))
        tags = git("tag", "--points-at", sha).split()
        formal = sorted(t for t in tags if FORMAL.match(t))
        dev = sorted((t for t in tags if DEV.match(t)), key=lambda t: int(DEV.match(t).group(2)))
        if not (formal or dev):
            raise Fail(f"no release tag points at {sha}; make a development build of it first")
        tag = formal[-1] if formal else dev[-1]
        ci_run = check_ci(repo, sha)
    except Fail as e:
        summary(f"### Verification bundle: refused\n\n{e}")
        print(f"::error::{e}")
        return 1
    except (Bad, gh.ApiError) as e:
        print(f"::error::{e}")
        return 2
    out(sha=sha, tag=tag)
    summary(f"### Verification bundle for {tag}\n\n- SHA `{sha}`\n- ci.yml run {ci_run}")
    return 0


def main() -> int:
    if env("WORKFLOW") == "verify-bundle":
        return verify_bundle_dispatch()
    try:
        repo = env("GITHUB_REPOSITORY")
        actor = env("ACTOR")
        skip_nightly = env("IN_SKIP_NIGHTLY") == "true"
        skip_verify = env("IN_SKIP_VERIFY") == "true"
        d = decide()
        admin = gh.is_admin(actor)
        overrides = []
        for flag, used in (("skip_nightly", skip_nightly), ("skip_verify", skip_verify)):
            if used and not admin:
                raise Fail(f"`{flag}` by `{actor}`, who is not in .github/hft-admins.txt")
            if used:
                overrides.append({"input": flag, "actor": actor})
        if skip_verify and d["channel"] != "release":
            skip_verify = False  # dev builds never make a verification bundle anyway
        sha = d["sha"]
        if env("EVENT") == "workflow_dispatch" and d["channel"] == "dev":
            if not (is_ancestor_of_main(sha) or is_branch_head(sha)):
                raise Fail(f"{sha} is neither on main nor the head of a pushed branch (PR head)")
        ci_run = check_ci(repo, sha)
        dg_run = check_decoupling(repo, sha)
        nightly_run = None
        if d["channel"] == "release":
            if not is_ancestor_of_main(sha):
                raise Fail(f"3: {sha} is not an ancestor of origin/main")
            if not skip_nightly:
                nightly_run = check_nightly(repo)
        evidence = {
            "ci_run_id": ci_run,
            "decoupling_check_run_id": dg_run,
            "nightly_run_id": nightly_run,
            "overrides": overrides,
        }
    except Fail as e:
        summary(f"### Release preflight: failed\n\n{e}")
        print(f"::error::preflight {e}")
        return 1
    except (Bad, gh.ApiError) as e:
        summary(f"### Release preflight: error\n\n{e}")
        print(f"::error::preflight {e}")
        return 2
    out(
        sha=sha,
        tag=d["tag"],
        version=d["version"],
        channel=d["channel"],
        create_tag=d["create_tag"],
        skip_verify=skip_verify,
        evidence=json.dumps(evidence, sort_keys=True, separators=(",", ":")),
    )
    lines = [
        f"### Release preflight: {d['tag']} ({d['channel']})",
        "",
        f"- SHA `{sha}`",
        f"- ci.yml run {ci_run}; decoupling-gate check run {dg_run}",
        f"- nightly run: {nightly_run if nightly_run else 'not evaluated'}",
    ]
    for o in overrides:
        lines.append(f"- Admin override `{o['input']}` by `{o['actor']}`")
    summary("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
