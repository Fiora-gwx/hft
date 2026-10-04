"""Read-only: list headings that existed in a backup snapshot but no longer exist in the current tree.
Usage: python heading_audit.py <backup_root>   (e.g. E:\\trading\\_backup\\round8b-pre)"""
import io, os, re, sys

BACKUP = sys.argv[1]
CUR = r"E:\trading"
SUBDIRS = ["hft", "architecture-v2"]
SKIP = ("_src", "_dsh", "_work")
rx = re.compile(r"^(#{2,4})\s+(.+?)\s*$")

def norm(h):
    h = re.sub(r"[`*_]", "", h)
    h = re.sub(r"[((].*?[))]", "", h)   # drop parentheticals, often reworded
    h = re.sub(r"\s+", "", h)
    return h.lower()

def heads(p):
    out = []
    in_code = False
    for line in io.open(p, encoding="utf-8", errors="replace"):
        if line.startswith("```"):
            in_code = not in_code
            continue
        if in_code:
            continue
        m = rx.match(line)
        if m:
            out.append(m.group(2))
    return out

total = 0
for sd in SUBDIRS:
    for dp, dn, fn in os.walk(os.path.join(BACKUP, sd)):
        if any(s in dp for s in SKIP):
            continue
        for f in fn:
            if not f.endswith(".md"):
                continue
            bp = os.path.join(dp, f)
            rel = os.path.relpath(bp, BACKUP)
            cp = os.path.join(CUR, rel)
            if not os.path.exists(cp):
                print(f"[GONE FILE] {rel}")
                continue
            old = heads(bp)
            new_norm = {norm(h) for h in heads(cp)}
            lost = [h for h in old if norm(h) not in new_norm]
            if lost:
                total += len(lost)
                print(f"[{len(lost)}] {rel}")
                for h in lost:
                    print(f"      - {h}")
print("lost headings:", total)
