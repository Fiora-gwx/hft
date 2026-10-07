#!/usr/bin/env python3
"""Local sanity check of .github/ (no network, no new global software).

1. Every workflow YAML parses (PyYAML when importable; otherwise this check is reported as skipped
   and only the text checks run).
2. Structure (YAML mode): top-level `permissions: {contents: read}`; every job has
   `timeout-minutes` (jobs that call a reusable workflow excepted) and runs on ubuntu-latest or
   ubuntu-24.04-arm (arm jobs inside `container: ubuntu:24.04`); `needs` name existing jobs; local
   `uses: ./.github/workflows/x.yml` targets exist and declare `workflow_call`;
   `steps.<id>.outputs` references name a step id of the same job.
3. Every `cargo xtask <sub>` called (workflows and .github/scripts) and every
   `steps.xtask.outputs.<sub>` guard names a subcommand registered in tools/README.md.
4. Every `.github/scripts/<file>` referenced exists; `bash -n` on each `run:` block (expressions
   replaced) and each .sh script; `py_compile` on each .py script.

Usage: python .github/scripts/check-workflows.py   (from the repository root; exit 1 on findings)
"""

from __future__ import annotations

import py_compile
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WF = ROOT / ".github" / "workflows"
SCRIPTS = ROOT / ".github" / "scripts"
RUNNERS = {"ubuntu-latest", "ubuntu-24.04-arm"}

try:
    import yaml  # type: ignore
except ImportError:  # pragma: no cover
    yaml = None

problems: list[str] = []


def bad(msg: str) -> None:
    problems.append(msg)


def registered_xtask() -> set[str]:
    text = (ROOT / "tools" / "README.md").read_text(encoding="utf-8")
    names = set(re.findall(r"^### cargo xtask ([a-z][a-z-]*)", text, re.M))
    m = re.search(r"^### 其他 xtask 子命令\n(.*?)(?=^### )", text, re.M | re.S)
    if m:
        for row in re.findall(r"^\| `([a-z][a-z-]*)", m.group(1), re.M):
            names.add(row)
    names.add("help")  # built into xtask's argument parser
    return names


def xtask_uses(text: str) -> set[str]:
    subs = set(re.findall(r"cargo xtask ([a-z][a-z-]*)", text))
    subs |= set(re.findall(r"steps\.xtask\.outputs\.([a-z][a-z-]*)", text))
    return subs


def check_bash(src: str, where: str) -> None:
    bash = shutil.which("bash")
    if not bash:
        return
    body = re.sub(r"\$\{\{.*?\}\}", "X", src, flags=re.S)
    with tempfile.NamedTemporaryFile("w", suffix=".sh", delete=False, encoding="utf-8", newline="\n") as f:
        f.write(body)
        name = f.name
    try:
        r = subprocess.run([bash, "-n", name], capture_output=True, text=True)
        if r.returncode != 0:
            bad(f"{where}: bash -n: {r.stderr.strip()}")
    finally:
        Path(name).unlink(missing_ok=True)


def check_workflow(path: Path, doc: dict, all_docs: dict[str, dict]) -> None:
    rel = path.relative_to(ROOT).as_posix()
    on = doc.get("on", doc.get(True))  # PyYAML reads the key `on` as boolean True
    if on is None:
        bad(f"{rel}: no `on:`")
    if doc.get("permissions") != {"contents": "read"}:
        bad(f"{rel}: top-level permissions must be exactly contents: read")
    jobs = doc.get("jobs") or {}
    for jid, job in jobs.items():
        where = f"{rel}: job {jid}"
        needs = job.get("needs", [])
        for n in [needs] if isinstance(needs, str) else needs:
            if n not in jobs:
                bad(f"{where}: needs unknown job {n}")
        if "uses" in job:
            target = job["uses"]
            if target.startswith("./.github/workflows/"):
                name = Path(target).name
                called = all_docs.get(name)
                if called is None:
                    bad(f"{where}: uses missing {target}")
                else:
                    con = called.get("on", called.get(True)) or {}
                    if not (isinstance(con, dict) and "workflow_call" in con):
                        bad(f"{where}: {target} has no workflow_call trigger")
            continue
        if "timeout-minutes" not in job:
            bad(f"{where}: no timeout-minutes")
        runs_on = job.get("runs-on")
        if runs_on not in RUNNERS:
            bad(f"{where}: runs-on {runs_on!r} not in {sorted(RUNNERS)}")
        container = job.get("container")
        if isinstance(container, dict):  # long form `container: {image: …, options: …}`
            container = container.get("image")
        if runs_on == "ubuntu-24.04-arm" and container != "ubuntu:24.04":
            bad(f"{where}: arm job without container: ubuntu:24.04")
        ids = {s.get("id") for s in job.get("steps", []) if s.get("id")}
        job_text = yaml.safe_dump(job, allow_unicode=True)
        for ref in set(re.findall(r"steps\.([A-Za-z_][\w-]*)\.(?:outputs|outcome|conclusion)", job_text)):
            if ref not in ids:
                bad(f"{where}: references unknown step id {ref}")
        for i, step in enumerate(job.get("steps", [])):
            if "run" in step and step.get("shell", "bash") == "bash":
                check_bash(step["run"], f"{where} step {step.get('name', i)}")


def main() -> int:
    files = sorted(WF.glob("*.yml")) + sorted(WF.glob("*.yaml"))
    if not files:
        bad("no workflow files")
    texts = {p.name: p.read_text(encoding="utf-8") for p in files}
    docs: dict[str, dict] = {}
    if yaml is None:
        print("PyYAML not available: YAML parse and structure checks skipped (text checks only)")
    else:
        for p in files:
            try:
                docs[p.name] = yaml.safe_load(texts[p.name])
            except yaml.YAMLError as e:
                bad(f"{p.name}: YAML parse error: {e}")
        for p in files:
            if p.name in docs:
                check_workflow(p, docs[p.name], docs)

    script_texts = {p.name: p.read_text(encoding="utf-8") for p in sorted(SCRIPTS.iterdir()) if p.is_file()}
    known = registered_xtask()
    for name, text in {**texts, **script_texts}.items():
        if name == "check-workflows.py":
            continue
        for sub in sorted(xtask_uses(text) - known):
            bad(f"{name}: cargo xtask {sub} is not registered in tools/README.md")
        for ref in sorted(set(re.findall(r"\.github/scripts/([\w.-]+)", text))):
            if not (SCRIPTS / ref).exists():
                bad(f"{name}: references missing .github/scripts/{ref}")
    for name in re.findall(r"\[([a-z][a-z-]*)\]=M", script_texts.get("xtask-inventory.sh", "")):
        if name not in known:
            bad(f"xtask-inventory.sh: milestone map names unregistered subcommand {name}")

    for p in sorted(SCRIPTS.iterdir()):
        if p.suffix == ".sh":
            check_bash(p.read_text(encoding="utf-8"), p.name)
        elif p.suffix == ".py":
            try:
                py_compile.compile(str(p), doraise=True, cfile=tempfile.mktemp())
            except py_compile.PyCompileError as e:
                bad(f"{p.name}: {e.msg}")

    for line in problems:
        print(f"FAIL {line}")
    used = sorted(set().union(*(xtask_uses(t) for t in texts.values())))
    print(f"{len(files)} workflow(s), {len(script_texts)} script(s); xtask subcommands used: {', '.join(used)}")
    print("OK" if not problems else f"{len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
