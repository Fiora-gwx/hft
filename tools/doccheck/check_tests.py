#!/usr/bin/env python3
"""Machine checks for hft test specs (TESTS.md + docs/tests/*.md).

Rules come from hft/docs/tests/README.md (format, numbering, coverage). Categories:

  MISSINGTESTS  core, non-frozen component without TESTS.md
  FRONT         TESTS.md frontmatter missing/wrong (component, prefix, spec)
  BADID         case heading id malformed or wrong prefix for this file
  DUPID         id defined twice (repo-wide)
  FIELD         required field missing for the case's level
  LEVEL         unknown level
  MILESTONE     unknown milestone
  IMPL          实现 field malformed or function name does not start with the id
  COVMISS       README heading (outside code) missing from the 覆盖 table
  COVEXTRA      覆盖 row that is not a README heading
  COVREF        覆盖 references an id that does not exist
  COVREASON     不适用 reason not in the allowed set
  UNCOVERED     case not referenced by any 覆盖 row (warning)
  VAGUE         vague word in 期望/通过标准 (warning)
  TABLEROW      table-level case without a row table or rows not numbered <id>.<k>
  NOCASES       TESTS.md has no cases
  STATUS        状态 not '隔离:<link>' or '可选扩展:<name>'

Usage: PYTHONIOENCODING=utf-8 python check_tests.py [--base E:\\trading\\hft] [--allow-missing] [-v]
"""
import argparse
import os
import re
import sys
from collections import defaultdict

ap = argparse.ArgumentParser()
ap.add_argument("--base", default=r"E:\trading\hft")
ap.add_argument("--allow-missing", action="store_true", help="missing TESTS.md is a warning, not a problem")
ap.add_argument("-v", "--verbose", action="store_true")
args = ap.parse_args()
ROOT = os.path.abspath(args.base)

problems = defaultdict(list)
warnings = defaultdict(list)

LEVELS = {"unit", "table", "prop", "golden", "fuzz", "bench", "dst", "integ", "proc", "replay", "host", "testnet", "drill"}
MILESTONES = {"M1", "M1.5", "M2", "M3", "M4", "M5", "M6", "M7", "M8"}
COMMON = ["规则", "级别", "里程碑", "实现"]
BY_LEVEL = {
    "unit": ["前置", "步骤", "期望"],
    "integ": ["前置", "步骤", "期望"],
    "proc": ["前置", "步骤", "期望"],
    "table": ["前置"],
    "prop": ["性质", "生成", "参考", "例数"],
    "golden": ["输入", "期望文件"],
    "fuzz": ["目标", "语料", "判定"],
    "bench": ["基准名"],
    "dst": ["故障", "种子", "断言"],
    "replay": ["输入", "期望"],
    "host": ["环境", "授权", "步骤", "通过标准", "证据"],
    "testnet": ["环境", "授权", "步骤", "通过标准", "证据"],
    "drill": ["环境", "授权", "步骤", "通过标准", "证据"],
}
VAGUE = ["正确", "正常", "合理", "适当", "符合预期", "等等", "之类", "按预期"]
ALLOWED_REASON = re.compile(r"^不适用[::]\s*(非规范|见子节|由\s*.+\s*覆盖|里程碑外)")
NONNORMATIVE = {"概要", "目录", "测试与验收", "依赖", "延伸阅读", "已知限制与后续工作", "开发备忘"}
SCEN_ID = re.compile(r"^(A\d+|B\d+|C\d+|[A-Z]{1,4}-\d+[a-z]?|INV-\d+|DRL-\d+)$")
COMP_ID = re.compile(r"^([A-Z]{2,4})-(\d{3})$")
RANGE_SEP = "–"


def rel(p):
    return os.path.relpath(p, ROOT).replace(os.sep, "/")


def read(p):
    with open(p, encoding="utf-8") as f:
        return f.read()


def frontmatter(s):
    if not s.startswith("---"):
        return None
    parts = s.split("---", 2)
    if len(parts) < 3:
        return None
    fm = {}
    for line in parts[1].splitlines():
        m = re.match(r"^([A-Za-z_]+):\s*(.*?)\s*$", line)
        if m:
            fm[m.group(1)] = m.group(2).strip().strip('"')
    return fm


def heading_levels(s):
    out, in_code = [], False
    for line in s.splitlines():
        if line.lstrip().startswith("```"):
            in_code = not in_code
            continue
        if in_code:
            continue
        m = re.match(r"^(#{1,6})\s+(.*?)\s*#*\s*$", line)
        if m:
            out.append((len(m.group(1)), m.group(2).strip()))
    return out


def headings(s, levels=(2, 3)):
    out, in_code = [], False
    for line in s.splitlines():
        if line.lstrip().startswith("```"):
            in_code = not in_code
            continue
        if in_code:
            continue
        m = re.match(r"^(#{1,6})\s+(.*?)\s*#*\s*$", line)
        if m and len(m.group(1)) in levels:
            out.append(m.group(2).strip())
    return out


def split_row(line):
    line = line.strip()
    if line.startswith("|"):
        line = line[1:]
    if line.endswith("|"):
        line = line[:-1]
    cells, cur, esc = [], "", False
    for ch in line:
        if esc:
            cur += ch
            esc = False
        elif ch == "\\":
            cur += ch
            esc = True
        elif ch == "|":
            cells.append(cur.strip())
            cur = ""
        else:
            cur += ch
    cells.append(cur.strip())
    return cells


# ---------- prefix registry from docs/tests/README.md
reg_path = os.path.join(ROOT, "docs", "tests", "README.md")
prefix_of = {}
if os.path.exists(reg_path):
    sec = read(reg_path).split("## 3. 编号", 1)[-1].split("## 4.", 1)[0]
    for line in sec.splitlines():
        if line.startswith("|") and "---" not in line:
            cells = split_row(line)
            for i in range(0, len(cells) - 1, 2):
                c, p = cells[i], cells[i + 1]
                if c and p and re.fullmatch(r"[A-Z]{2,4}", p):
                    prefix_of[c] = p
else:
    problems["FRONT"].append("docs/tests/README.md missing (prefix registry)")

# ---------- components that need TESTS.md
EXEMPT = {"strategies/portfolio-carry", "strategies/portfolio-carry/pipeline"}
need = {}
for top in ("crates", "apps", "strategies", "python", "tools"):
    d = os.path.join(ROOT, top)
    if not os.path.isdir(d):
        continue
    for name in sorted(os.listdir(d)):
        rd = os.path.join(d, name, "README.md")
        if not os.path.isfile(rd):
            continue
        fm = frontmatter(read(rd)) or {}
        if fm.get("scope") != "core" or fm.get("status") == "frozen":
            continue
        if fm.get("kind") not in ("crate", "app", "python-package"):
            continue
        r = f"{top}/{name}"
        if r in EXEMPT:
            continue
        need[r] = name
for r, name in (("tools", "tools"), ("deploy", "deploy")):
    if os.path.isfile(os.path.join(ROOT, r, "README.md")):
        need[r] = name

for r, name in need.items():
    if name not in prefix_of:
        problems["FRONT"].append(f"{r}: component '{name}' has no prefix in docs/tests/README.md §3")
    if not os.path.isfile(os.path.join(ROOT, r, "TESTS.md")):
        (warnings if args.allow_missing else problems)["MISSINGTESTS"].append(r)

# ---------- parse cases


def parse_cases(p, s):
    """Return list of dicts: id, title, line, fields{name: text}, body."""
    lines = s.splitlines()
    cases, cur, in_code, field = [], None, False, None
    for i, line in enumerate(lines, 1):
        if line.lstrip().startswith("```"):
            in_code = not in_code
        if not in_code:
            m = re.match(r"^###\s+(\S+)\s+(.*)$", line)
            if m and (COMP_ID.match(m.group(1)) or SCEN_ID.match(m.group(1))):
                cur = {"id": m.group(1), "title": m.group(2), "line": i, "fields": {}, "body": []}
                cases.append(cur)
                field = None
                continue
            if re.match(r"^#{1,3}\s", line):
                cur, field = None, None
                continue
        if cur is None:
            continue
        cur["body"].append(line)
        fm_ = re.match(r"^- ([^\s::]{1,6})[::]\s*(.*)$", line)
        if fm_ and not in_code:
            field = fm_.group(1)
            cur["fields"][field] = fm_.group(2)
        elif field and (line.startswith("  ") or line.strip() == "") and not re.match(r"^- ", line):
            cur["fields"][field] += "\n" + line
    return cases


all_ids = {}
id_file = {}
files = []
for r in need:
    t = os.path.join(ROOT, r, "TESTS.md")
    if os.path.isfile(t):
        files.append((t, r))
dt = os.path.join(ROOT, "docs", "tests")
if os.path.isdir(dt):
    for f in sorted(os.listdir(dt)):
        if f.endswith(".md") and f != "README.md":
            files.append((os.path.join(dt, f), None))

parsed = {}
for t, comp in files:
    s = read(t)
    cases = parse_cases(t, s)
    parsed[t] = (comp, s, cases)
    for c in cases:
        if c["id"] in all_ids:
            problems["DUPID"].append(f"{rel(t)}:{c['line']} {c['id']} (also {all_ids[c['id']]})")
        else:
            all_ids[c["id"]] = f"{rel(t)}:{c['line']}"
            id_file[c["id"]] = t


def expand(entry):
    """Expand 'OMS-010–OMS-024, OMS-030' into ids; return (ids, bad_tokens)."""
    ids, bad = [], []
    for tok in re.split(r"[,、,\s]+", entry):
        tok = tok.strip().strip("`")
        if not tok:
            continue
        if RANGE_SEP in tok:
            a, _, b = tok.partition(RANGE_SEP)
            ma, mb = COMP_ID.match(a), COMP_ID.match(b)
            if ma and mb and ma.group(1) == mb.group(1) and int(ma.group(2)) <= int(mb.group(2)):
                for n in range(int(ma.group(2)), int(mb.group(2)) + 1):
                    ids.append((f"{ma.group(1)}-{n:03d}", n in (int(ma.group(2)), int(mb.group(2)))))
            else:
                bad.append(tok)
        elif COMP_ID.match(tok) or SCEN_ID.match(tok):
            ids.append((tok, True))
        elif re.match(r"^[A-Z]{2,4}-\d{3}\.\d+$", tok):
            ids.append((tok.split(".")[0], True))
        else:
            bad.append(tok)
    return ids, bad


referenced = set()
for t, (comp, s, cases) in parsed.items():
    rt = rel(t)
    if comp is not None:
        fm = frontmatter(s) or {}
        name = need[comp]
        want = prefix_of.get(name)
        if fm.get("component") != name:
            problems["FRONT"].append(f"{rt}: component '{fm.get('component')}' != '{name}'")
        if fm.get("prefix") != want:
            problems["FRONT"].append(f"{rt}: prefix '{fm.get('prefix')}' != '{want}'")
        if fm.get("spec") != "README.md":
            problems["FRONT"].append(f"{rt}: spec must be README.md")
        if not cases:
            problems["NOCASES"].append(rt)
    for c in cases:
        cid, f = c["id"], c["fields"]
        where = f"{rt}:{c['line']} {cid}"
        if comp is not None:
            m = COMP_ID.match(cid)
            if not m or m.group(1) != prefix_of.get(need[comp]):
                problems["BADID"].append(where)
        body = "\n".join(c["body"])
        if "已删除" in body and len(f) <= 1:
            continue
        lvl = f.get("级别", "").strip().strip("`")
        if lvl not in LEVELS:
            problems["LEVEL"].append(f"{where}: '{lvl}'")
        ms = f.get("里程碑", "").strip()
        if ms not in MILESTONES:
            problems["MILESTONE"].append(f"{where}: '{ms}'")
        for k in COMMON + BY_LEVEL.get(lvl, []):
            if not f.get(k, "").strip():
                problems["FIELD"].append(f"{where}: missing {k}")
        st = f.get("状态", "").strip()
        if st and not re.match(r"^(隔离[::]\s*\[.+\]\(.+\)|可选扩展[::]\s*\S+)", st):
            problems["STATUS"].append(f"{where}: '{st[:40]}'")
        impl = f.get("实现", "").strip()
        if impl and impl != "待实现":
            mm = re.match(r"^`?([^`\s]+)::([A-Za-z0-9_]+)`?$", impl)
            stem = cid.lower().replace("-", "_").replace(".", "_")
            if lvl in ("host", "testnet", "drill"):
                pass
            elif not mm:
                problems["IMPL"].append(f"{where}: '{impl}'")
            elif mm.group(1).endswith(".py") and (mm.group(2).startswith("test_" + stem + "_") or mm.group(2) == "test_" + stem):
                pass   # pytest requires the test_ prefix (docs/tests §6)
            elif not mm.group(2).startswith(stem + "_") and mm.group(2) != stem:
                problems["IMPL"].append(f"{where}: fn '{mm.group(2)}' must start with '{stem}_'")
        if lvl == "table":
            rows = [ln for ln in c["body"] if ln.startswith("|") and "---" not in ln]
            nums = [split_row(ln)[0].strip("`") for ln in rows[1:]] if rows else []
            if not nums or any(not re.fullmatch(re.escape(cid) + r"\.\d+", n) for n in nums):
                problems["TABLEROW"].append(where)
        for k in ("期望", "通过标准", "表"):
            if k == "表" and lvl != "table":
                continue
            txt = "\n".join(ln for ln in c["body"] if ln.startswith("|")) if k == "表" else f.get(k, "")
            for w in VAGUE:
                if w in re.sub(r"`[^`]*`", "", txt):
                    warnings["VAGUE"].append(f"{where}: '{w}' in {k}")
                    break

    if comp is None:
        continue
    # coverage table
    cov = s.split("## 覆盖", 1)
    if len(cov) < 2:
        problems["COVMISS"].append(f"{rt}: no '## 覆盖' section")
        continue
    sec = re.split(r"\n## ", cov[1], maxsplit=1)[0]
    rows = {}
    for line in sec.splitlines():
        if line.startswith("|") and "---" not in line:
            cells = split_row(line)
            if len(cells) >= 2 and cells[0] != "README 小节":
                rows[cells[0]] = cells[1]
    readme = read(os.path.join(ROOT, comp, "README.md"))
    hs = headings(readme)
    hl = heading_levels(readme)
    has_child = set()
    for i, (lv, h) in enumerate(hl):
        if i + 1 < len(hl) and hl[i + 1][0] > lv:
            has_child.add(h)
    for h in hs:
        if h not in rows:
            problems["COVMISS"].append(f"{rt}: README heading '{h}'")
    for h, entry in rows.items():
        if h not in hs:
            problems["COVEXTRA"].append(f"{rt}: '{h}'")
        e = entry.strip()
        if e.startswith("不适用"):
            if not ALLOWED_REASON.match(e):
                problems["COVREASON"].append(f"{rt}: '{h}' → '{e}'")
            mby = re.search(r"由\s*(.+?)\s*覆盖", e)
            if mby:
                ids, bad = expand(mby.group(1))
                for i, _ in ids:
                    referenced.add(i)
                    if i not in all_ids:
                        problems["COVREF"].append(f"{rt}: '{h}' → {i}")
            if "见子节" in e and h not in has_child:
                problems["COVREASON"].append(f"{rt}: '{h}' 见子节 but heading has no subsections")
            if "非规范" in e and h not in NONNORMATIVE:
                warnings["COVNONNORM"].append(f"{rt}: '{h}' marked 非规范")
            continue
        ids, bad = expand(e)
        if bad:
            problems["COVREF"].append(f"{rt}: '{h}' unparsable {bad}")
        if not ids:
            problems["COVREF"].append(f"{rt}: '{h}' has no ids")
        for i, must in ids:
            referenced.add(i)
            if must and i not in all_ids:
                problems["COVREF"].append(f"{rt}: '{h}' → {i}")

for t, (comp, s, cases) in parsed.items():
    if comp is None:
        continue
    for c in cases:
        if c["id"] not in referenced:
            warnings["UNCOVERED"].append(f"{rel(t)}:{c['line']} {c['id']}")

# ---------- report
np_ = sum(len(v) for v in problems.values())
nw = sum(len(v) for v in warnings.values())
ncases = sum(len(v[2]) for v in parsed.values())
print(f"files={len(parsed)} cases={ncases} components_needing_tests={len(need)}")
for title, d in (("PROBLEMS", problems), ("WARNINGS", warnings)):
    for k in sorted(d):
        print(f"[{title[:-1]}] {k}: {len(d[k])}")
        for x in d[k][: (10**6 if args.verbose else 12)]:
            print("   ", x)
        if not args.verbose and len(d[k]) > 12:
            print(f"    ... {len(d[k]) - 12} more")
print(f"problems={np_} warnings={nw}")
sys.exit(1 if np_ else 0)
