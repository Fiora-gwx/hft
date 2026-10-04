r"""Mechanical checks for the hft spec tree (read-only; never writes any file; prints findings only).

Severity
  HARD errors   are counted in `problems=` (the number to drive to 0).
  WARNINGS      are heuristic checks with known false positives; listed separately, counted in `warnings=`,
                NOT in problems. Hard: every category below unless marked [W].

Frozen components (frontmatter `status: frozen`; the whole directory of that README): only format checks run
(MISSING NOFM FMKEY SCOPE STATUS PHASEKEY NOSEC ORDER TABLE BADLINK BADANCHOR STRATLINK). Every content /
cross-document consistency check skips them; their would-be hits are tallied on the `frozen:` info line.

Categories (format):
  MISSING    expected file absent
  NOFM/FMKEY component README frontmatter missing or incomplete
  SCOPE      frontmatter scope not in {core, optional}
  STATUS     frontmatter status not in {spec, impl, frozen}
  PHASEKEY   frontmatter still has a phase: key
  NOSEC/ORDER README template sections missing or out of order
  TABLE      root README component table disagrees with frontmatter (tier/scope) or misses a component
  BADLINK    relative link target does not exist
  BADANCHOR  #anchor not found in the target markdown file (GitHub slug rules)
  STRATLINK  link to the deleted crates/hft-strategies
Categories (content):
  STRATREF   text mention of hft-strategies (outside its own dir and .agents/notes)
  STRATNAME  strategy family name in an engine crate README (crates/*)
  VENUENAME  exchange name in a strategies/* README
  CANARY     canary / Primary-Canary role / second-instance ("shadow" instance) residue
  WEEK       "第 N 周" schedule wording
  PHASE      "阶段 N" wording
  OLDTERM    superseded terms (迁移前门槛, HypoDriver, 假设回放, Armed)
  CDEXCORE   lowercase "cdex" in a scope:core engine README (crates/* or apps/*)
  BALDOMAIN  balance_domain residue (same-exchange spot x perp is not done; no balance domains)
  ACKCMD     dangling reconcile "Ack" command (entries are settled only by AdoptExchange / SetBalanceBaseline)
  GROUPALL0  GROUP_ALL defined as 0 (GROUP_NONE = GroupId(0), GROUP_ALL = GroupId(u32::MAX))
  MANUALCMD  ManualOpen / ManualClose in an engine README (replaced by StrategyCmd; lines marked 退役/retired skipped)
  OLDPRIO    sequencer ring priority without rep-sim last (ctl > rep > bbo > advice > md > rep-sim)
  SCHEMAEXP  old schema export command (hft-cfg schema export / export-schema; use hft-core schemas --out <dir>)
  ONEINST    "only one hft-core per host / no second instance" residue (prod + sim instances exist)
  -- round 8c (R8-9 + M0-b review); .agents/notes and history files are skipped for these --
  SMALLLIVE  small-live stage residue (小额实盘 / 新子账户小额 / 小额实单 ...); negation lines skipped
  SIMINPROD  "sim accounts inside prod during dev2" / "sim host undecided" residue (sim always runs in the sim instance)
  PERPLEV    constant perp_leverage residue (leverage is read from AccountConfigReport); 删除/不再/退役 lines skipped
  MIDAGE     max_mid_age_ms / rr_max_mid_age_ms residue (quote freshness = BBO connection health)
  QUOTEAGE   (hedge_)quote_max_age_ms residue (BBO-age freshness thresholds; informational, same retirement)
  KILLU8     8-bit KillWord residue (KillWord = CachePadded<AtomicU32>, kill_mask: u32)
  REGDEP     an engine crate (incl. hft-testkit, incl. dev-deps) said to depend on hft-strategy-registry
  PYSTRAT    strategy family name in python/ framework packages (plugins live in strategies/<name>/pipeline/;
             cdex mentions on optional lines, in optional sections, or about the legacy cdex system are allowed)
  -- round 8d (M0-d brief §5); hft tree only, .agents/notes/ and docs/postmortem/ skipped --
  HISTORY    history narrative in spec text (link URLs ignored; "延伸阅读" sections skipped): 修订后 / round N /
             R<d>-<n> / 本轮 + 新增|修订|改|删除|评审|决定|裁决|补|调整|同步|变更 / 第[一..十]+轮 or 第 N 轮
             followed by 裁决|修订|评审|决定|决策|第|新增|改|讨论|方案|问题|简报|定稿
  HISTWEAK   [W] bare 本轮 / 第一轮 / 第二轮 … (also mean a loop round: sequencer delivery rounds, a reconcile
             round, a ledger round trip)
  SYNCQ      cross-document sync queue: 同步项 / 需要其他组件
  SYNCREG    [W] "需要 … 登记" within one clause, not preceded by 不 (often a normative rule, sometimes a queue item)
  LENGTH     size limit: root AGENTS.md <= 1800, docs/architecture.md <= 3000, every other AGENTS.md <= 900.
             Measure (docs/AGENTS.md): drop fenced code blocks and Markdown link URLs, then
             CJK ideographs + tokens of [A-Za-z0-9_.-]+ (a token must contain a letter or digit, so bare
             list bullets "-" and table rules "---" do not count)
  CODEVAL    [W] integer field named *kind/*level/*purpose/*role/*mode/*status/*reason/*code in a layout struct
             (#[repr(C)] code block, declared "N B", or size_of assertion) whose comment (same-line // or the
             field's /* */, or comment-only lines right above it) has neither code values ("=", or a value list
             "0 无 / 1 First") nor a pointer to a code table (#anchor, §, link, 码表/码值/登记/见/自定义,
             "同 Type.field", or a known enum name)
  LAYOUT     declared struct size ("N B" in the struct-line or closing-brace comment, not "≤ N B" or "N B + …";
             or size_of::<T>() == N) differs from the repr(C) layout computed from the fields (primitives, arrays,
             newtypes from code or from "| `Name` | u32 |" table rows, aliases, enums with repr(uN) or explicit
             discriminants < 256, other parsed structs, align(N)/packed; structs with an unresolvable field type
             are skipped and counted on the `layout:` info line)
  IMPLPAD    [W] declared size equals the repr(C) layout but the plain field sum is smaller (implicit padding;
             hft-types requires explicit _pad for #[repr(C)] wire structs)
  NOTEREG    Note{X} / Timer{X} name not found in crates/hft-event/README.md (table rows or Name=N lists) nor in
             any strategies/<name>/README.md table (strategy Note codes are registered by the strategy's
             StrategyDesc.notes); forms handled: Note{X}, Note{Source, X}, Note{source = S, code = X}, A|B, A/B;
             struct literals ("tag: …") and single-letter placeholders skipped
  RESID8D    round-8d residue: busy_poll=25 / busy_read=25, napi_defer_hard_irqs=2, Conflicts=hft-replay-verify,
             "c8 或更大/更高", kernel_rx "≤ 4 µs" baseline, `xtask ledger` (rebuild), "dev2 不跑 fuzz/DST",
             pull_request_review triggering the whole quality gate (line without decoupling); negation lines skipped
             (hft tree except .agents/notes and docs/postmortem; design chapters except history files)
  -- round 8e (M0-e brief §4 = review W3 §4 items 1-12); hft spec files (no .agents/notes, docs/postmortem) --
  Grading: a check is HARD when its extraction is structural (tables, signatures, code blocks, registry rows) and
  a hit is a real disagreement between two places; it is a WARNING [W] when it needs prose heuristics (CFGREF,
  CLIREQ, LINKTGT) or flags a style rule that many legacy lines break (TBDFMT), or restates a value consistently
  (BENCHCOPY). Measured on the pre-M0-e backup (_backup/round8e-pre via `--base`): every hard hit was a W3 §3
  instance or a true gap of the same kind.
  ① IDREG     user/group (uid `x`, `x` 用户/组, 组 `x`, User=/Group=/SupplementaryGroups=, owner:group, table columns
             uid/用户/属主) not defined in deploy/README.md (unit table 用户 column, 系统用户 row, AMI 创建用户与组 list)
     IDOPT   [W] the same on a line about optional / chain components
     PATHREG  /opt/hft/…, /run/hft/…, /var/lib/hft/… path (hft tree + design chapters) not matching a path written in
             deploy/README.md (incl. RuntimeDirectory=/StateDirectory=); placeholders <x>/%i/{a,b} match any value;
             a deeper path under a registered directory is fine; an unregistered top directory is reported once
     S3CRED   a journal-archiver prefix (s3.prefix default, prefix = "…", files prefix 缺省) not covered by the
             `hft-archive`(S3 写 …) consumer credential prefixes in deploy/README.md
     S3REF   [W] S3/secret prefix `hft…/…` (lines about S3/前缀/prefix/对象键) not in deploy/README.md
  ② CFGDUP    the same configuration key gets a default value in two config-table rows (tables with a 默认 column;
             namespace: hft-core's TOML = hft-core + hft-venue (under venues.<v>.) + hft-rt + hft-sim, every other
             file is its own namespace)
     CFGPATH  an hft-venue key whose TOML table venues.<v>.<seg> has no row in hft-core 配置文件字段
     CFGREF  [W] `x = …` assigning a value to an hft-core table on a line about hft-core (`instance = "sim"` vs
             instance.name), or a dotted hft-core key reference (instance./paths./host./venues./…) not in the tables
  ③ CLIFLAG   `--flag` used with `cargo xtask <sub>` / `hft-verify <sub>` / `hft-dst <sub>` / `replayer <sub>` (inline
             code or code-block line, hft tree + design chapters) not in that subcommand's signature (code-block
             lines in tools/README.md, crates/hft-testkit/README.md, apps/replayer/README.md, incl. continuation
             lines and the 其他 xtask 子命令 table; a signature ending in … is open)
     CLIREQ  [W] a full invocation (code block, or ≥ 2 flags and no …) missing a required (unbracketed) flag
     CLISUB  [W] `cargo xtask <word>` whose subcommand is not in tools/README.md
  ④ RINGCAP   ring named in the hft-sequencer ring table, hft-venue §1 or hft-event 环拓扑 (non-optional lines)
             without a row in hft-core's ring-capacity table (| 环 | … prod … |; "(及拆分时的 `-bbo`)" adds -bbo)
     RINGHOME a config default (any table with 默认) that gives the capacity of a ring of that table: second home
     JRNLMEM  core journal of journal-format 三类 journal without a row in hft-core's journal-segment table or its
             memory-budget table (`raw-feed-*` covers raw-feed-<venue>, not raw-feed-<venue>-bbo)
  ⑤ PAD       a struct with its own #[repr(C …)] (declared size or not) whose natural layout has implicit padding
             (tail padding added only by align(N) is allowed; packed skipped; unresolvable field types counted on
             the `round8e:` info line). IMPLPAD [W] remains for declared-size structs without their own repr(C).
  ⑥ SIZECOPY "`X` N B" / "X 按当前字段 N B" / "X(N B" anywhere (prose or comment) where X is a struct whose layout
             is computable and N differs (bounds ≤/<, "N B + …" and record sizes with header skipped)
  ⑦ ENUMVAL   a unit variant without explicit discriminant in an enum with #[repr(u*/i*)], or one whose comment or
             a prose sentence names it with 逐值相同 / 同名同值
  ⑧ NOTESRC   a crate README (hft-sequencer/risk/oms/ledger/exec/venue/ctl, apps/hft-core) that writes Note{X}
             ("写/产出/产生/发出/输出 [P0] Note{X}", or the `notes: PreVec<Note>` comment list) while X's source in
             the hft-event Note 码登记表 belongs to another crate (clauses naming another actor are skipped)
  ⑨ BENCHDUP  one benchmark name (`prefix/name` in the last cell of a row with ns/µs budgets) with different budgets
     BENCHOWN a benchmark name defined only outside the file that owns its prefix (most definitions of `prefix/`)
     BENCHCOPY [W] the same name and budget restated in another file
  ⑩ LINKTGT  [W] "规则归 / 唯一归属 / 唯一定义 / 只在 [X](…#a)": section #a contains none of the backticked
             identifiers of the clause before the link
  ⑪ TBDFMT   [W] per file: 待核实 without 方法 on the same line or a 开发备忘 pointer, and other wordings (待确认,
             待选型, 待研究, 未核实, 待验证, 待调研, 待评估); 待标定 / 待实测 are the allowed variants
  ⑫ BRIEFNAME a name of the brief's naming table (## … 命名总表, `--brief`) found in none of the owner files named
             in its 归属 cell (Note{X} → X, Type::V / Type.f → both parts, F(…) / S{…} → head, x = v → x = v);
             a repo file named there (`docs/changes.md`) must exist
  -- round 8f (M0-f brief §4 = X3 §4 items 1-12); hft spec files, frozen components tallied --
  Measured on the pre-M0-f backup (`--base E:\trading\_backup\round8f-pre --prev E:\trading\_backup\round8e-pre\hft`;
  BRIEFDRIFT with `--brief round8e-spec-brief.md`): every hard hit is an X3 §3 entry or a true gap of the same kind.
  Deploy model (home deploy/README.md): unit table (用户 / 关键设置 / dev2 覆盖; template drop-ins per instance;
  "a / b" rows split by "a:`…`" segments), user table (primary group = user name + 附加组), path table (属主:组 / 模式,
  ACL `g:G:rx`, 建立者, files with "(`o:g` 0660)" in 用途), 组: list.
  ① PERM      permission closure. Claims: ReadWritePaths= (traverse), path-table directories owned by a unit user
             (write), component config defaults that are paths (own → write, socket → write, other → read), explicit
             actor prose ("`hft-replay-verify` 从这里读", "ops_bot 读 `/var/lib/hft/treasury/`"), group claims ("对
             `G` 组只读", "`G` 组可读", "对该组只读" + 热段/冷副本/心跳块 aliases from the path table 用途 column), and
             direct statements in a component README ("删除本地冷副本", "尾随热段": verb right before the path/alias,
             no other actor in the clause). Each is checked (x on every registered ancestor, then r/w) against owner,
             group, mode and ACL. Also: a file/socket `o:g` created by process o needs o ∈ g or a setgid parent with
             group g (path table and README "创建 … `o:g`"); a README's stated owner:group/mode of a registered path
             equals the path table.                                      (X3 P0-6, P0-7, P0-8, P0-9)
     PERMRW   ProtectSystem=strict: a write outside /dev needs the unit's RuntimeDirectory/StateDirectory (or a path the
             path table says StateDirectory=/RuntimeDirectory= creates for it) or ReadWritePaths=  (X3 P0-9, P1-27)
     PERMOPT [W] the same on optional users / paths / lines (hft-chain …)
     PERMIMP [W] claims inferred from a component README clause with a verb near (not right before) a path/alias
  ② GROUPMEM  unit SupplementaryGroups= ≠ user table; "`G` 组只有 A 与 B" / "组成员只有 …" ≠ the user table's members;
             "A 加入 `G`" / "X 以附加组 `G`" without the membership; "A 不在 `G` 组" / "不在该组" with it (X3 P0-7, P2-54)
  ③ S3KEY     an S3 key template (`<prefix>/…`, `hft/…`, `hft-testnet/…`; lines about S3/前缀/对象键/归档) whose segment
             after the prefix is an instance (`<instance>`, prod, sim): the host segment is missing     (X3 P0-10)
  ④ IAMSET    per host class (dev2 / upgrade) the secrets prefixes granted in a runbook ≠ deploy's grant line; a class
             deploy calls "只有 S3 写" while the archiver HEADs objects or a runbook uses its S3 read; an S3 consumer
             that writes / names an SSE-KMS (private) prefix without a KMS grant in deploy  (X3 P0-11, P1-29, P1-31)
  ⑤ UNITFORM  the same identifier pair (`_ns/_us/_ms/_s` stripped, dotted prefix dropped) in two `a OP b`
             inequalities whose boundaries differ after unit normalisation (formulas read as raw numbers)  (X3 P0-4)
     UNITMIX [W] one such inequality comparing different units with no 1 000-power factor
  ⑥ WIRECODE  "arg = `X`" / "码 = `X`" where X has no numeric table (enum with explicit discriminants, `Name=N` list,
             const bit table, table with a code column); a Timer that carries a key (`Timer{O, N{key}}`, or the kinds
             of a `*TimerReq {kind /* A | B */, id}`) whose hft-event timer row gives `N=n` without a
             `timer_id = n | … << 8` encoding                                                   (X3 P0-2, P1-8)
  ⑦ VERBATIM  a clause "X … 原样 / 布局原样" where X (or a field type inside it) is a #[repr(C, u8)] data enum or a
             type documented "不以原始字节写进 journal"                                          (X3 P0-1)
  ⑧ APPENDONLY a wire record (0xNNNN kind in its header, or any struct of hft-event / event-model / journal-format) or
             a snapshot-section row (节 N in its header, a "// 节 N" comment block, and their field types) compared with
             the same struct in `--prev` (same file): a named field moved / retyped / removed (mid-body insertion), a new
             field inside the old body (reserved bytes consumed — allowed only when the event-model 演进规则 permits
             "占用保留字节"), or a size / field change without a higher schema_version / 行版本 (节 N = v, 节 N vV,
             行版本 V)                                                                         (X3 P1-9, P1-31)
  ⑨ ALLHOSTS  a "(全部主机类)" clause outside deploy (dev2 clauses skipped): every unit pair it names must be linked in the
             deploy main unit table (关键设置 column or a "见[…]" section) — not only in the dev2 override column / dev2
             section; a non-numeric `Key=Value` directive for its units (or 批处理单元) must be in their rows
                                                                                              (X3 P0-5, P2-55)
  ⑩ LISTCOPY [W] lists after 只有/只含/白名单/只限/允许表 with ≥ 3 backticked names (unit words expanded: 两个 hft-core
             实例, 批处理单元): two lists with Jaccard ≥ 0.6 that differ, a list spelled differently from a longer one
             (recv ~ recvmsg), an "只有" list naming most but not all variants of an enum   (X3 P1-21, P2-17, P2-36)
  ⑪ BUDGET    a crate budget row (延迟与资源契约) wider than its benchmarks 单 crate 基准上限表 bound (matched by
             primitive keyword / entry name, message class, case label; warm, optional, command-path rows skipped);
             a bound row that delegates to a crate's table ("…归 [crate 延迟与资源契约]") uses that table's rows (X3 P1-20)
     BUDGETPCT [W] the bound states no percentile and only a higher percentile of the crate row exceeds it
  ⑫ BRIEFDRIFT [W] brief (`--brief`) and merge-fixes (`--fixes`) vs spec: absolute paths absent from the spec, ADR files
             absent under .agents/notes, `f(a, b)` arity ≠ the spec's fn, `name: type` ≠ the spec's field type,
             "`key`(N …)" ≠ the spec default, "`A` 保持 T … 与 `B` 一致" with B typed otherwise
                                                       (round8e brief: E10 path, E13(a) cancel_resting_on, R8)
     FROZENREF [W] a backticked name in a frozen README that the non-frozen spec no longer uses but `--prev` did
             (frozen components only fix references to renamed identifiers)
"""
import os
import re
import sys
from collections import defaultdict

try:
    sys.stdout.reconfigure(encoding="utf-8")
except AttributeError:
    pass

# usage: check_hft.py [LIMIT] [--base DIR] [--brief FILE] [--fixes FILE] [--prev DIR]
#   LIMIT  max items printed per category (default 40)
#   --base DIR   tree root holding hft/ and architecture-v2/ (default E:\trading; e.g. E:\trading\_backup\round8e-pre)
#   --brief FILE the brief whose naming table (## N. 命名总表) BRIEFNAME reads and whose names/paths/signatures
#                BRIEFDRIFT compares with the spec (default round8f-spec-brief.md)
#   --fixes FILE merge-fixes file also read by BRIEFDRIFT (default: the brief's sibling *-merge-fixes.md, if any)
#   --prev DIR   the previous hft tree for APPENDONLY / FROZENREF (default E:\trading\_backup\round8g-pre\hft;
#                skipped when it is the tree being checked)
BASE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))   # repo root
BRIEF = os.path.join(BASE, "design", "_work", "round8f-spec-brief.md")
FIXES = None
PREV = os.environ.get("HFT_DOCCHECK_PREV", os.path.join(BASE, "target", "doccheck-prev"))   # previous tree for APPENDONLY (CI: merge-base checkout)
LIMIT = 40
_args = sys.argv[1:]
while _args:
    _a = _args.pop(0)
    if _a == "--base" and _args:
        BASE = _args.pop(0)
    elif _a == "--brief" and _args:
        BRIEF = _args.pop(0)
    elif _a == "--fixes" and _args:
        FIXES = _args.pop(0)
    elif _a == "--prev" and _args:
        PREV = _args.pop(0)
    elif _a.isdigit():
        LIMIT = int(_a)
if FIXES is None:
    _f = BRIEF.replace("spec-brief", "merge-fixes")
    FIXES = _f if _f != BRIEF and os.path.exists(_f) else ""
ROOT = BASE
DESIGN = os.path.join(BASE, "design")


def in_spec(p):
    """Files of the spec tree (repo minus design/ and build output)."""
    return p.startswith(ROOT) and not p.startswith(DESIGN + os.sep) and not p.startswith(os.path.join(ROOT, "target"))

CRATES = """hft-types hft-refdata hft-event hft-strategy-sdk hft-rt hft-journal hft-book hft-net hft-venue hft-venue-proto
venue-binance venue-gate venue-okx venue-bybit venue-bitget venue-kucoin venue-hyperliquid
hft-oms hft-risk hft-ledger hft-exec hft-strat-lib hft-sequencer hft-ctl hft-telemetry
hft-amm hft-chain-evm hft-chain-solana hft-sim hft-py hft-testkit hft-ctl-proto hft-ext-proto""".split()
STRATS = "registry portfolio-carry cdex cross-taker maker-taker".split()
APPS = "hft-core chain-core journal-archiver watchtower reconciler kill-switch replayer mock-exchange chain-grpc-relay".split()
PY = "pipeline ops_bot research treasury".split()
PIPELINE_PLUGINS = ["strategies/portfolio-carry/pipeline/README.md", "strategies/cdex/pipeline/README.md"]

COMPONENTS = ([f"crates/{c}/README.md" for c in CRATES] + [f"strategies/{s}/README.md" for s in STRATS]
              + [f"apps/{a}/README.md" for a in APPS] + [f"python/{p}/README.md" for p in PY] + PIPELINE_PLUGINS)
EXPECTED = COMPONENTS + """AGENTS.md CLAUDE.md README.md crates/AGENTS.md strategies/AGENTS.md apps/AGENTS.md python/AGENTS.md
deploy/AGENTS.md deploy/README.md benchmarks/AGENTS.md benchmarks/README.md fixtures/README.md tools/README.md
contracts/README.md docs/AGENTS.md docs/architecture.md docs/glossary.md docs/latency-budget.md docs/hot-path-rules.md
docs/testing.md docs/defensive-patterns.md docs/development.md
docs/subsystems/README.md docs/subsystems/event-model.md docs/subsystems/journal-format.md
docs/subsystems/control-plane.md docs/subsystems/external-venue-protocol.md
docs/cookbook/adding-a-venue.md docs/cookbook/adding-a-strategy.md docs/cookbook/adding-a-control-command.md
docs/cookbook/adding-an-event-kind.md docs/cookbook/adding-a-crate.md
docs/runbooks/README.md docs/runbooks/launch.md docs/runbooks/migration.md docs/postmortem/README.md
.agents/notes/README.md .agents/notes/AGENTS.md""".split()

SECTIONS = ["## 概要", "## 目录", "## 使用本组件", "## 实现说明", "## 延迟与资源契约",
            "## 测试与验收", "## 依赖", "## 延伸阅读", "## 已知限制与后续工作", "### 开发备忘"]
FM_KEYS = ["description", "kind", "tier", "scope", "status"]
STATUS_VALUES = ("spec", "impl", "frozen")

problems = defaultdict(list)
warnings = defaultdict(list)
suppressed = defaultdict(int)   # content-check hits inside frozen components (not reported)
WARN_CATS = {"HISTWEAK", "SYNCREG", "CODEVAL", "IMPLPAD",
             "IDOPT", "S3REF", "CFGREF", "CLIREQ", "CLISUB", "BENCHCOPY", "LINKTGT", "TBDFMT",
             "PERMOPT", "PERMIMP", "UNITMIX", "LISTCOPY", "BUDGETPCT", "BRIEFDRIFT", "FROZENREF"}


def rel(p):
    return os.path.relpath(p, BASE)


def rroot(p):
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
        m = re.match(r"^([A-Za-z_]+):\s*(.*?)\s*(#.*)?$", line)
        if m:
            fm[m.group(1)] = m.group(2).strip().strip('"')
    return fm


def strip_code(s):
    s = re.sub(r"```.*?```", "", s, flags=re.S)
    return re.sub(r"`[^`\n]*`", "", s)


# ---------- files that should exist
for r in EXPECTED:
    if not os.path.exists(os.path.join(ROOT, r)):
        problems["MISSING"].append(r)

# ---------- all markdown files
md_files = []
for base in (ROOT, DESIGN):
    for dp, dn, fn in os.walk(base):
        if any(x in dp for x in ("_src", "_dsh", os.sep + "_work")):
            continue
        for f in fn:
            if f.endswith(".md"):
                md_files.append(os.path.join(dp, f))
texts = {p: read(p) for p in md_files}

# ---------- frontmatter, scope, status, sections
fms = {}
for p, s in texts.items():
    if not in_spec(p):
        continue
    fm = frontmatter(s)
    if fm is None:
        continue
    r = rroot(p)
    fms[r] = fm
    if "scope" in fm and fm["scope"] not in ("core", "optional"):
        problems["SCOPE"].append(f"{r}: scope={fm['scope']!r}")
    if "status" in fm and fm["status"] not in STATUS_VALUES:
        problems["STATUS"].append(f"{r}: status={fm['status']!r} (allowed: {', '.join(STATUS_VALUES)})")
    if "phase" in fm:
        problems["PHASEKEY"].append(r)

# frozen components: the directory of every README whose frontmatter says status: frozen
FROZEN_DIRS = sorted(os.path.dirname(os.path.join(ROOT, r.replace("/", os.sep)))
                     for r, fm in fms.items() if fm.get("status") == "frozen" and r.endswith("README.md"))


def is_frozen(p):
    return any(p == d or p.startswith(d + os.sep) for d in FROZEN_DIRS)


def add(cat, p, msg):
    """Record a content-check finding; frozen components are tallied, not reported."""
    if p and is_frozen(p):
        suppressed[cat] += 1
    elif cat in WARN_CATS:
        warnings[cat].append(msg)
    else:
        problems[cat].append(msg)


for r in COMPONENTS:
    p = os.path.join(ROOT, r)
    if not os.path.exists(p):
        continue
    s = texts.get(p) or read(p)
    fm = fms.get(r)
    if fm is None:
        problems["NOFM"].append(r)
    else:
        for k in FM_KEYS:
            if k not in fm:
                problems["FMKEY"].append(f"{r}: {k}")
    last = -1
    for sec in SECTIONS:
        i = s.find("\n" + sec)
        if i < 0:
            problems["NOSEC"].append(f"{r}: {sec}")
        elif i < last:
            problems["ORDER"].append(f"{r}: {sec}")
        else:
            last = i

# ---------- root README component table vs frontmatter
readme = texts.get(os.path.join(ROOT, "README.md"), "")
in_table = set()
for line in readme.splitlines():
    if not line.startswith("| [") or line.count("|") < 5:
        continue
    cells = [c.strip() for c in line.strip().strip("|").split("|")]
    if len(cells) < 4:
        continue
    tier, scope = cells[1], cells[2]
    for t in re.findall(r"\]\(([^)#\s]+)\)", cells[0]):
        r = os.path.normpath(t).replace(os.sep, "/")
        in_table.add(r)
        fm = fms.get(r)
        if fm is None:
            continue
        if tier not in ("—", "-") and fm.get("tier") and fm["tier"] != tier:
            problems["TABLE"].append(f"{r}: table tier={tier} frontmatter tier={fm['tier']}")
        if scope in ("core", "optional") and fm.get("scope") and fm["scope"] != scope:
            problems["TABLE"].append(f"{r}: table scope={scope} frontmatter scope={fm['scope']}")
for r in [f"crates/{c}/README.md" for c in CRATES] + [f"strategies/{s}/README.md" for s in STRATS]:
    if r not in in_table:
        problems["TABLE"].append(f"{r}: not in root README component table")

# ---------- links and anchors


def slug(h):
    h = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", h.strip()).lower()
    h = re.sub(r"[^\w\- ]", "", h)
    return h.replace(" ", "-")


anchor_cache = {}


def anchors(p):
    if p in anchor_cache:
        return anchor_cache[p]
    s = texts.get(p)
    if s is None:
        s = read(p)
    out, seen = set(), defaultdict(int)
    in_code = False
    for line in s.splitlines():
        if line.lstrip().startswith("```"):
            in_code = not in_code
            continue
        if in_code:
            continue
        m = re.match(r"^(#{1,6})\s+(.*?)\s*#*\s*$", line)
        if m:
            b = slug(m.group(2))
            n = seen[b]
            seen[b] += 1
            out.add(b if n == 0 else f"{b}-{n}")
    out.update(re.findall(r'<a\s+(?:id|name)="([^"]+)"', s))
    anchor_cache[p] = out
    return out


link_re = re.compile(r"\[[^\]]*\]\(([^)\s]+)\)")
nlinks = 0
for p, s in texts.items():
    body = re.sub(r"```.*?```", "", s, flags=re.S)
    for m in link_re.finditer(body):
        t = m.group(1)
        if t.startswith(("http://", "https://", "mailto:")):
            continue
        path, _, frag = t.partition("#")
        nlinks += 1
        tp = os.path.normpath(os.path.join(os.path.dirname(p), path)) if path else p
        if path and "hft-strategies" in path.replace("\\", "/"):
            problems["STRATLINK"].append(f"{rel(p)}: {t}")
        if not os.path.exists(tp):
            problems["BADLINK"].append(f"{rel(p)}: {t}")
            continue
        if frag and tp.endswith(".md") and os.path.isfile(tp):
            if frag.lower() not in {a.lower() for a in anchors(tp)}:
                problems["BADANCHOR"].append(f"{rel(p)}: {t}")

# ---------- text checks


def skip_history(p):
    return (os.sep + "research" + os.sep in p or p.endswith("13-设计评审与优化.md")
            or os.sep + "rejected" + os.sep in p or os.sep + "measurements" + os.sep in p)


def count(p, rx, s=None):
    return len(rx.findall(s if s is not None else texts[p]))


RX_PHASE = re.compile(r"阶段\s*[0-4一二三四]'?|^phase:\s*\S", re.M)
RX_WEEK = re.compile(r"第\s*\d+(?:\s*[–\-~至]\s*\d+)?\s*周")
RX_CANARY = re.compile(r"[Cc]anary|金丝雀|Primary\s*=\s*1|\"shadow\"|'shadow'")
RX_STRATNAME = re.compile(r"PortfolioCarry|Cdex|CrossTaker|MakerTaker|StockToken|(?<![A-Za-z0-9_])Carry(?![A-Za-z0-9_])")
RX_VENUE = re.compile(r"(?<![A-Za-z0-9_\-])(Binance|OKX|Bitget|Gate(?:\.io)?|Bybit|KuCoin|Kucoin|Hyperliquid|binance|okx|bitget|bybit|kucoin|hyperliquid)(?![A-Za-z0-9_])")
RX_OLD = re.compile(r"迁移前门槛|HypoDriver|假设回放|(?<![A-Za-z])Armed(?![A-Za-z])")
RX_CDEX = re.compile(r"cdex")  # lowercase only; Cdex is covered by STRATNAME
RX_BALDOM = re.compile(r"balance_domain")
RX_ACKCMD = re.compile(r"Adopt(?:Exchange)?`?\s*[/、]\s*`?Ack(?![A-Za-z])"
                       r"|(?<![A-Za-z])Ack`?\s*[/、]\s*`?(?:SetBalanceBaseline|Adopt)"
                       r"|ControlCmd::Ack(?![A-Za-z])|(?<![A-Za-z/])Ack\s*\{\s*report_id"
                       r"|(?<![A-Za-z])Ack 后再一轮")  # alert acknowledgement (/ack in ops_bot) is not this command
RX_GROUPALL0 = re.compile(r"GROUP_ALL\s*(?::\s*GroupId\s*)?=\s*(?:GroupId\s*\(\s*)?0(?![0-9xX.])|GROUP_ALL\s*[(（]\s*0\s*[)）]")
RX_MANUAL = re.compile(r"Manual(?:Open|Close)(?![A-Za-z])")
RX_PRIO = re.compile(r"ctl\s*>\s*(?:回报|rep)[^\n]{0,40}?>\s*md(?!\s*>\s*rep-sim)")
RX_SCHEMAEXP = re.compile(r"hft-cfg\s+(?:schema\s+export|export-schema)")
RX_ONEINST = re.compile(r"没有第二个\s*(?:hft-core\s*)?(?:进程)?实例|每台主机(?:只有)?一个\s*hft-core|instance_id\s*固定\s*1")

# ---- round 8c
RX_SMALL = re.compile(r"小额实盘|小额实单|新子账户小额|小额上线|小额阶段|小额放量|→\s*小额(?!真实|往返|报价)")
RX_SMALL_NEG = re.compile(r"不设|没有|删除|取消|不跑|不再|无小额|不含|否决|取代|不做|只有模拟")
RX_SIMINPROD = re.compile(r"dev2\s*阶段[^\n。;]{0,30}(?:`?prod`?\s*(?:实例)?内|也(?:承载|含)模拟账户|同在)"
                          r"|Sim 账户与 Live 账户同在|模拟账户在 dev2 阶段"
                          r"|sim 实例(?:运行)?在哪台主机|sim 实例放哪台主机|sim 实例的(?:主机|位置)[^\n。]{0,12}(?:待定|未定)"
                          r"|统一升级(?:主机)?后(?:只|全部)?在\s*`?sim`?\s*实例")
RX_PERPLEV = re.compile(r"(?<![A-Za-z0-9_])perp_leverage(?![A-Za-z0-9_])")  # not RiskParams.max_perp_leverage_x100
RX_MIDAGE = re.compile(r"max_mid_age_ms")
RX_QUOTEAGE = re.compile(r"(?<![A-Za-z_])(?:hedge_)?quote_max_age_ms")
RX_KILLU8 = re.compile(r"KillWord[^\n]{0,80}AtomicU8|AtomicU8[^\n]{0,40}KillWord|kill_mask\s*:\s*u8\b"
                       r"|KillBits\s*\(\s*(?:pub\s+)?u8\s*\)|KillBits\s*:\s*u8\b")
RX_RETIRED_ANY = re.compile(r"退役|retired|Retired|删除|不再|取消|旧规格|旧的|取代|superseded|不设常量")
RX_REG = re.compile(r"hft-strategy-registry|strategies/registry|(?<![A-Za-z_-])registry(?![A-Za-z_-])")
RX_REG_NEG = re.compile(r"不依赖|不得|禁止|不引入|不链接|不经|不能|也不|不例外|不在|没有|无需")
RX_DEPVERB = re.compile(r"依赖|经|取得|dev-dependencies|dev 依赖|链接")
RX_REG_INJECT = re.compile(r"泛型参数|由 `?apps/|装配处|注入")  # the app passes the set in; not a crate dependency
RX_PYSTRAT = re.compile(r"PortfolioCarry|Cdex|CrossTaker|MakerTaker|StockToken|(?<![A-Za-z0-9_])Carry(?![A-Za-z0-9_])"
                        r"|(?<![A-Za-z0-9_])cdex(?![A-Za-z0-9])|portfolio[-_]carry|cross[-_]taker|maker[-_]taker"
                        r"|stock[-_]?token")
RX_OPTIONAL = re.compile(r"optional|可选", re.I)
RX_LEGACY = re.compile(r"旧框架|现有 cdex|旧 bot")


def scope_core(s):
    fm = frontmatter(s)
    return bool(fm) and fm.get("scope") == "core"


def line_hits(s, rx, skip=None):
    n = 0
    for line in s.splitlines():
        if skip and skip.search(line):
            continue
        n += len(rx.findall(line))
    return n


RX_RETIRED = re.compile(r"退役|retired|Retired")

for p, s in texts.items():
    r = rel(p)
    if not skip_history(p):
        for cat, rx in (("PHASE", RX_PHASE), ("WEEK", RX_WEEK), ("CANARY", RX_CANARY)):
            n = count(p, rx)
            if n:
                add(cat, p, f"{r}: {n}")
    if not in_spec(p):
        if not skip_history(p):  # design chapters: only the round-8b residue checks that apply to prose
            for cat, rx in (("BALDOMAIN", RX_BALDOM), ("GROUPALL0", RX_GROUPALL0), ("OLDPRIO", RX_PRIO),
                            ("SCHEMAEXP", RX_SCHEMAEXP), ("ONEINST", RX_ONEINST)):
                n = count(p, rx)
                if n:
                    add(cat, p, f"{r}: {n}")
        continue
    rr = rroot(p)
    notes = rr.startswith(".agents/notes/")
    if not notes and not rr.startswith("crates/hft-strategies/"):
        n = s.count("hft-strategies")
        if n:
            add("STRATREF", p, f"{rr}: {n}")
        n = count(p, RX_OLD)
        if n:
            add("OLDTERM", p, f"{rr}: {n} ({', '.join(sorted(set(RX_OLD.findall(s))))})")
    if re.match(r"crates/[^/]+/README\.md$", rr) and not rr.startswith("crates/hft-strategies/"):
        hits = RX_STRATNAME.findall(s)
        if hits:
            add("STRATNAME", p, f"{rr}: {len(hits)} ({', '.join(sorted(set(hits)))})")
    if re.match(r"strategies/[^/]+/README\.md$", rr):
        hits = RX_VENUE.findall(s)
        if hits:
            add("VENUENAME", p, f"{rr}: {len(hits)} ({', '.join(sorted(set(hits)))})")
    engine_readme = re.match(r"(?:crates|apps)/[^/]+/README\.md$", rr) and not rr.startswith("crates/hft-strategies/")
    if engine_readme and scope_core(s):
        n = len(RX_CDEX.findall(s))
        if n:
            add("CDEXCORE", p, f"{rr}: {n}")
    if engine_readme:
        n = line_hits(s, RX_MANUAL, RX_RETIRED)
        if n:
            add("MANUALCMD", p, f"{rr}: {n}")
    if not notes:
        for cat, rx in (("BALDOMAIN", RX_BALDOM), ("ACKCMD", RX_ACKCMD), ("GROUPALL0", RX_GROUPALL0),
                        ("OLDPRIO", RX_PRIO), ("SCHEMAEXP", RX_SCHEMAEXP), ("ONEINST", RX_ONEINST)):
            n = count(p, rx)
            if n:
                add(cat, p, f"{rr}: {n}")

# ---------- round 8c residue checks (hft tree except .agents/notes; design chapters except history)
for p, s in texts.items():
    if skip_history(p):
        continue
    r = rel(p)
    if in_spec(p) and rroot(p).startswith(".agents/notes/"):
        continue  # decision records keep their history
    for cat, rx, neg in (("SMALLLIVE", RX_SMALL, RX_SMALL_NEG), ("SIMINPROD", RX_SIMINPROD, RX_SMALL_NEG),
                         ("PERPLEV", RX_PERPLEV, RX_RETIRED_ANY), ("MIDAGE", RX_MIDAGE, RX_RETIRED_ANY),
                         ("QUOTEAGE", RX_QUOTEAGE, RX_RETIRED_ANY), ("KILLU8", RX_KILLU8, RX_RETIRED_ANY)):
        n = line_hits(s, rx, neg)
        if n:
            add(cat, p, f"{r}: {n}")
    # REGDEP (b): prose saying hft-testkit depends on / goes through the registry
    for i, line in enumerate(s.splitlines(), 1):
        if ("hft-testkit" in line and RX_REG.search(line) and RX_DEPVERB.search(line) and not RX_REG_NEG.search(line)
                and "registry/tests" not in line):  # tests living in strategies/registry/tests/ use hft-testkit (allowed)
            add("REGDEP", p, f"{r}:{i}: hft-testkit + registry dependency wording")

# REGDEP (a): an engine crate README's 依赖 section (or a Cargo/use snippet) lists the registry
for c in CRATES:
    p = os.path.join(ROOT, "crates", c, "README.md")
    if not os.path.exists(p):
        continue
    s = texts.get(p) or read(p)
    for i, line in enumerate(s.splitlines(), 1):
        if re.search(r"hft-strategy-registry\s*=|hft_strategy_registry::", line):
            add("REGDEP", p, f"crates/{c}/README.md:{i}: Cargo/use of hft-strategy-registry")
    m = re.search(r"\n## 依赖\n(.*?)(?=\n## )", s, flags=re.S)
    if m:
        for line in m.group(1).splitlines():
            if RX_REG.search(line) and not RX_REG_NEG.search(line) and not RX_REG_INJECT.search(line):
                add("REGDEP", p, f"crates/{c}/README.md: 依赖节列出 registry: {line.strip()[:90]}")

# PYSTRAT: strategy family names in python/ framework packages (md, py, toml)
PY_ROOT = os.path.join(ROOT, "python")
for dp, dn, fn in os.walk(PY_ROOT):
    for f in fn:
        if not f.endswith((".md", ".py", ".toml")):
            continue
        p = os.path.join(dp, f)
        s = texts.get(p) or read(p)
        rr = rroot(p)
        optional_section = False
        hits = []
        for i, line in enumerate(s.splitlines(), 1):
            hm = re.match(r"^(#{1,6})\s+(.*)$", line)
            if hm:
                optional_section = bool(RX_OPTIONAL.search(hm.group(2)))
            t = re.sub(r"\]\([^)]*\)", "]", line)                          # link targets are paths, not names
            t = re.sub(r"strategies/[A-Za-z0-9_\-]+", "strategies/<x>", t)  # pointers to strategies/<name>/...
            found = RX_PYSTRAT.findall(t)
            if optional_section or RX_OPTIONAL.search(line) or RX_LEGACY.search(line):
                found = [x for x in found if x.lower() != "cdex"]  # optional cdex plugin docs / the legacy cdex system
            if found:
                hits.append((i, found))
        if hits:
            names = sorted({x for _, xs in hits for x in xs})
            add("PYSTRAT", p, f"{rr}: {sum(len(xs) for _, xs in hits)} on {len(hits)} lines "
                              f"(first line {hits[0][0]}; {', '.join(names)})")

# =====================================================================================================
# round 8d checks (M0-d brief §5)
# =====================================================================================================
RX_HEAD = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")


def walk_lines(s):
    """Yield (lineno, line, in_code, headings) — in_code is True for fence lines and lines inside fences."""
    heads = [None] * 7
    in_code = False
    for i, line in enumerate(s.splitlines(), 1):
        st = line.lstrip()
        if st.startswith("```") or st.startswith("~~~"):
            in_code = not in_code
            yield i, line, True, tuple(h for h in heads[1:] if h)
            continue
        if not in_code:
            m = RX_HEAD.match(line)
            if m:
                lv = len(m.group(1))
                heads[lv] = m.group(2)
                for k in range(lv + 1, 7):
                    heads[k] = None
        yield i, line, in_code, tuple(h for h in heads[1:] if h)


def spec_file(p):
    """hft spec text subject to the round-8d content checks."""
    if not in_spec(p):
        return False
    rr = rroot(p)
    return not (rr.startswith(".agents/notes/") or rr.startswith("docs/postmortem/"))


def fmt_lines(ls, k=6):
    return ", ".join(str(x) for x in ls[:k]) + (" …" if len(ls) > k else "")


# ---------- ① HISTORY / HISTWEAK
# "第一轮/第二轮" and bare "本轮" also mean a loop round (sequencer delivery rounds, a reconcile round, a ledger
# round trip), so only the review-round forms are hard; the bare forms are HISTWEAK.
RX_HIST_ROUND_CTX = r"(?=裁决|修订|评审|决定|决策|第|新增|改|讨论|方案|问题|简报|定稿)"
RX_HIST = re.compile(r"修订后|第[一二三四五六七八九十]+轮" + RX_HIST_ROUND_CTX + r"|第\s*\d+\s*轮" + RX_HIST_ROUND_CTX
                     + r"|(?<![A-Za-z])[Rr]ound\s*\d+|(?<![A-Za-z0-9_])R\d+-\d+(?![\d])"
                     r"|本轮(?:新增|修订|改|删除|评审|决定|裁决|补|调整|同步|变更)")
RX_HISTWEAK = re.compile(r"本轮|第[一二三四五六七八九十]+轮|第\s*\d+\s*轮")
# ---------- ② SYNCQ / SYNCREG
RX_SYNCQ = re.compile(r"同步项|需要其他组件")
RX_SYNCREG = re.compile(r"(?<!不)需要[^。;;\n]{0,30}?登记")

for p, s in texts.items():
    if not spec_file(p):
        continue
    rr = rroot(p)
    hist, weak, syncq, syncreg = [], [], [], []
    for i, line, in_code, heads in walk_lines(s):
        if any("延伸阅读" in h for h in heads):
            continue
        line = re.sub(r"\]\([^)\s]*\)", "]", line)   # link URLs repeat the target's heading; BADANCHOR covers them
        line = line.replace("策略研究后定(R8-12)", "策略研究后定")   # deferral marker (R8-12), not history
        line = re.sub(r"\[(?:负责人决定|用户决定)\s*R\d+-\d+\]", "[决定]", line)   # decision citations
        if RX_HIST.search(line):
            hist.append(i)
        elif RX_HISTWEAK.search(line):
            weak.append(i)
        if RX_SYNCQ.search(line):
            syncq.append(i)
        elif RX_SYNCREG.search(line):
            syncreg.append(i)
    for cat, ls in (("HISTORY", hist), ("HISTWEAK", weak), ("SYNCQ", syncq), ("SYNCREG", syncreg)):
        if ls:
            add(cat, p, f"{rr}: {len(ls)} (lines {fmt_lines(ls)})")

# ---------- ③ LENGTH
RX_CJK = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]")
RX_WORD = re.compile(r"[A-Za-z0-9_.-]+")


def doc_length(s):
    t = "\n".join(line for _, line, in_code, _ in walk_lines(s) if not in_code)
    t = re.sub(r"\]\([^)\s]*\)", "]", t)          # Markdown link URLs
    return len(RX_CJK.findall(t)) + sum(1 for w in RX_WORD.findall(t) if re.search(r"[A-Za-z0-9]", w))


length_info = []
for dp, dn, fn in os.walk(ROOT):
    for f in fn:
        if f != "AGENTS.md":
            continue
        p = os.path.join(dp, f)
        rr = rroot(p)
        lim = 1800 if rr == "AGENTS.md" else 900
        n = doc_length(texts.get(p) or read(p))
        length_info.append(f"{rr}={n}/{lim}")
        if n > lim:
            add("LENGTH", p, f"{rr}: {n} > {lim}")
p = os.path.join(ROOT, "docs", "architecture.md")
if os.path.exists(p):
    n = doc_length(texts.get(p) or read(p))
    length_info.append(f"docs/architecture.md={n}/3000")
    if n > 3000:
        add("LENGTH", p, f"docs/architecture.md: {n} > 3000")
length_info.sort(key=lambda x: (x.count("/") > 2, x))

# ---------- ④ CODEVAL / ⑤ LAYOUT / IMPLPAD: parse Rust code blocks


def code_blocks(s):
    """[(first_line_no_of_content, [lines])] for fenced blocks."""
    out, cur, start, in_code = [], None, 0, False
    for i, line in enumerate(s.splitlines(), 1):
        st = line.lstrip()
        if st.startswith("```") or st.startswith("~~~"):
            if in_code:
                out.append((start, cur))
                in_code = False
            else:
                in_code, cur, start = True, [], i + 1
            continue
        if in_code:
            cur.append(line)
    return out


def scan_comments(lines):
    """Per line: (code_with_placeholders, [block comments], line_comment, code_blanked).
    code_blanked has every comment replaced by spaces (same length) for brace matching."""
    res, in_block = [], False
    for line in lines:
        code, blanked, blocks, lc = [], [], [], ""
        j, n = 0, len(line)
        cur_block = []
        while j < n:
            if in_block:
                k = line.find("*/", j)
                if k < 0:
                    cur_block.append(line[j:])
                    blanked.append(" " * (n - j))
                    j = n
                else:
                    cur_block.append(line[j:k])
                    blanked.append(" " * (k + 2 - j))
                    blocks.append(" ".join(cur_block))
                    code.append(f"\x00{len(blocks) - 1}\x00")
                    cur_block = []
                    in_block = False
                    j = k + 2
                continue
            if line.startswith("//", j):
                lc = line[j + 2:]
                blanked.append(" " * (n - j))
                break
            if line.startswith("/*", j):
                in_block = True
                blanked.append("  ")
                j += 2
                continue
            code.append(line[j])
            blanked.append(line[j])
            j += 1
        if in_block and cur_block:
            blocks.append(" ".join(cur_block))
        res.append(("".join(code), blocks, lc, "".join(blanked)))
    return res


PRIM = {"u8": (1, 1), "i8": (1, 1), "bool": (1, 1), "u16": (2, 2), "i16": (2, 2), "u32": (4, 4), "i32": (4, 4),
        "f32": (4, 4), "char": (4, 4), "u64": (8, 8), "i64": (8, 8), "f64": (8, 8), "usize": (8, 8),
        "isize": (8, 8), "u128": (16, 16), "i128": (16, 16), "AtomicU8": (1, 1), "AtomicBool": (1, 1),
        "AtomicU16": (2, 2), "AtomicU32": (4, 4), "AtomicI32": (4, 4), "AtomicU64": (8, 8), "AtomicI64": (8, 8),
        "NonZeroU8": (1, 1), "NonZeroU16": (2, 2), "NonZeroU32": (4, 4), "NonZeroU64": (8, 8)}
INT_TYPES = {"u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "u128", "i128"}
RX_TYPE = r"(?:\[(?:[^\[\]]|\[[^\[\]]*\])*\]|[A-Za-z_][\w:]*(?:<(?:[^<>]|<[^<>]*>)*>)?)"
RX_FIELD = re.compile(r"^\s*(?:\x00\d+\x00\s*)*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?([A-Za-z_]\w*)\s*:\s*(" + RX_TYPE + r")\s*((?:\x00\d+\x00\s*)*)$")
RX_STRUCT = re.compile(r"\bstruct\s+([A-Z]\w*)\s*(<[^>{]*>)?\s*\{")
RX_NEWTYPE = re.compile(r"\bstruct\s+([A-Z]\w*)\s*\(\s*(?:pub(?:\([^)]*\))?\s+)?(" + RX_TYPE + r")\s*\)\s*;")
RX_ALIAS = re.compile(r"\btype\s+([A-Z]\w*)\s*=\s*(" + RX_TYPE + r")\s*;")
RX_ENUM_DATA = re.compile(r"#\[repr\(\s*((?:C\s*,\s*)?(?:u8|u16|u32|u64|i8|i16|i32|i64)(?:\s*,\s*C)?)\s*\)\]\s*"
                          r"(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Z]\w*)\s*\{((?:[^{}]|\{[^{}]*\})*)\}")
RX_ENUM = re.compile(r"#\[repr\((u8|u16|u32|u64|i8|i16|i32|i64)\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Z]\w*)")
RX_CONST = re.compile(r"\bconst\s+([A-Z][A-Z0-9_]*)\s*:\s*\w+\s*=\s*([\d_]+)\s*;")
RX_SIZEOF = re.compile(r"size_of::<\s*([A-Z]\w*)\s*>\(\)\s*==\s*(\d[\d_]*)")
RX_DECL = re.compile(r"(?<![\w.])(\d+)\s*B(?![A-Za-z])(?!\s*\+)")   # "16 B + 消息" is a variable-length body


def decl_sizes(c):
    """Declared sizes in a comment; bounds ("≤ 4096 B", "< 64 B") are not sizes."""
    return {int(m.group(1)) for m in RX_DECL.finditer(c) if not re.search(r"[≤<>≥]\s*$", c[max(0, m.start() - 3):m.start()])}
RX_REPR = re.compile(r"#\[repr\(((?:[^()]|\([^()]*\))*)\)\]")
# wire enums written without a repr attribute (hft-types: "线格式枚举全部 #[repr(u8)]"): explicit discriminants < 256
RX_ENUM_PLAIN = re.compile(r"(?<!\])\s(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Z]\w*)\s*\{([^{}]*)\}")
# ID newtypes declared in a table row: | `InstId` | u32 | ...
RX_TABLE_NEWTYPE = re.compile(r"^\|\s*`([A-Z]\w*)`\s*\|\s*`?(u8|u16|u32|u64|u128|i8|i16|i32|i64|i128)`?\s*\|", re.M)


def split_top(s, sep=","):
    out, depth, cur = [], 0, []
    for ch in s:
        if ch in "[(<{":
            depth += 1
        elif ch in "])>}":
            depth -= 1
        if ch == sep and depth == 0:
            out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    out.append("".join(cur))
    return out


class Defs:
    def __init__(self):
        self.local = defaultdict(dict)        # file -> name -> (size, align)
        self.glob = defaultdict(set)          # name -> {(size, align)}
        self.consts_local = defaultdict(dict)
        self.consts_glob = defaultdict(set)
        self.enums = set()

    def put(self, f, name, sa):
        if sa is None:
            return False
        if self.local[f].get(name) == sa:
            return False
        self.local[f][name] = sa
        self.glob[name].add(sa)
        return True

    def get(self, f, name):
        if name in PRIM:
            return PRIM[name]
        if name in self.local[f]:
            return self.local[f][name]
        g = self.glob.get(name)
        if g and len(g) == 1:
            return next(iter(g))
        return None

    def const(self, f, name):
        if name in self.consts_local[f]:
            return self.consts_local[f][name]
        g = self.consts_glob.get(name)
        if g and len(g) == 1:
            return next(iter(g))
        return None


D = Defs()


def eval_len(expr, f):
    expr = expr.strip().replace("_", "") if re.fullmatch(r"[\d_]+", expr.strip()) else expr.strip()
    if re.fullmatch(r"\d+", expr):
        return int(expr)
    if re.fullmatch(r"[A-Z][A-Z0-9_]*", expr):
        return D.const(f, expr)
    m = re.fullmatch(r"(\w+)\s*([*+])\s*(\w+)", expr)
    if m:
        a, b = eval_len(m.group(1), f), eval_len(m.group(3), f)
        if a is not None and b is not None:
            return a * b if m.group(2) == "*" else a + b
    return None


def type_size(t, f):
    t = t.strip()
    if t.startswith("[") and t.endswith("]"):
        parts = split_top(t[1:-1], ";")
        if len(parts) != 2:
            return None
        inner, n = type_size(parts[0], f), eval_len(parts[1], f)
        if inner is None or n is None:
            return None
        return (inner[0] * n, inner[1])
    if "<" in t or "&" in t or "*" in t:
        return None
    return D.get(f, t.split("::")[-1])


# parse every code block once (parse_code is reused for the --prev tree by the round-8f APPENDONLY check)
def parse_code(tmap, root):
    """(struct_defs, Defs) of the markdown files of one tree; the global D is swapped while parsing."""
    global D
    saved, D = D, Defs()
    try:
        sdefs = _parse_code(tmap, root)
        return sdefs, D
    finally:
        D = saved


def _parse_code(tmap, root):
  struct_defs = []   # dicts with file, line, name, fields, repr, decl, block_has_repr
  for p, s in tmap.items():
    if not p.startswith(root):
        continue
    for m in RX_CONST.finditer(s):
        v = int(m.group(2).replace("_", ""))
        D.consts_local[p][m.group(1)] = v
        D.consts_glob[m.group(1)].add(v)
    asserts = defaultdict(set)
    for m in RX_SIZEOF.finditer(s):
        asserts[m.group(1)].add(int(m.group(2).replace("_", "")))
    for m in RX_TABLE_NEWTYPE.finditer(s):
        D.put(p, m.group(1), PRIM[m.group(2)])
    for start, lines in code_blocks(s):
        sc = scan_comments(lines)
        blank = "\n".join(x[3] for x in sc)
        offs = [0]
        for x in sc:
            offs.append(offs[-1] + len(x[3]) + 1)

        def line_of(pos):
            lo = 0
            while lo + 1 < len(offs) and offs[lo + 1] <= pos:
                lo += 1
            return lo

        block_has_repr = "#[repr(C" in blank
        data_enums = set()
        for m in RX_ENUM_DATA.finditer(blank):   # #[repr(C, u8)] / #[repr(u8)] enums with tuple payloads
            payloads = re.findall(r"\b[A-Z]\w*\s*\(([^()]*)\)", m.group(3))
            if payloads:
                data_enums.add(m.group(2))
                D.enums.add(m.group(2))
                struct_defs.append({"file": p, "line": start + line_of(m.start()), "name": m.group(2),
                                    "enum_data": (re.search(r"[ui]\d+", m.group(1)).group(0),
                                                  [[x.strip() for x in pl.split(",") if x.strip()] for pl in payloads])})
        for m in RX_ENUM.finditer(blank):
            if m.group(2) in data_enums:
                continue
            D.enums.add(m.group(2))
            D.put(p, m.group(2), PRIM[m.group(1)])
        for m in RX_ENUM_PLAIN.finditer("\n" + blank):
            vals = [int(v) for v in re.findall(r"=\s*(\d+)", m.group(2))]
            if vals and max(vals) < 256 and m.group(1) not in D.local[p] and m.group(1) not in data_enums:
                D.enums.add(m.group(1))
                D.put(p, m.group(1), (1, 1))
        for m in RX_STRUCT.finditer(blank):
            if m.group(2):          # generic struct
                continue
            ob = m.end() - 1
            depth, cb = 0, None
            for k in range(ob, len(blank)):
                if blank[k] == "{":
                    depth += 1
                elif blank[k] == "}":
                    depth -= 1
                    if depth == 0:
                        cb = k
                        break
            if cb is None:
                continue
            # attributes: text between the previous item end and the struct keyword
            prev_end = max(blank.rfind(";", 0, m.start()), blank.rfind("}", 0, m.start()))
            attrs = " ".join(RX_REPR.findall(blank[prev_end + 1:m.start()]))
            l0, l1 = line_of(ob), line_of(cb)
            hdr_c = sc[l0][2] + ("" if l0 == l1 else " " + " ".join(sc[l0][1]))
            close_c = sc[l1][2]
            decl = decl_sizes(hdr_c) | decl_sizes(close_c)
            decl |= asserts.get(m.group(1), set())
            fields, pending = [], []
            for li in range(l0, l1 + 1):
                code, blocks, lc, _ = sc[li]
                a = 0
                if li == l0:   # body starts after the struct's opening brace
                    hm = re.search(r"\bstruct\s+" + m.group(1) + r"\b[^{]*\{", code)
                    a = hm.end() if hm else code.find("{") + 1
                b = len(code)
                if li == l1:   # body ends at the brace that closes it
                    depth = 1
                    for k in range(a, len(code)):
                        if code[k] == "{":
                            depth += 1
                        elif code[k] == "}":
                            depth -= 1
                            if depth == 0:
                                b = k
                                break
                seg = code[a:b]
                # a line comment describes the fields only on interior lines (first = header, last = closing)
                line_comment = lc if l0 < li < l1 else ""
                if not seg.replace("\x00", "").strip(" \t,0123456789"):
                    if lc.strip() and l0 < li < l1:
                        pending.append(lc)   # comment-only line above a field
                    continue
                got = False
                for piece in split_top(seg):
                    fm_ = RX_FIELD.match(piece)
                    if not fm_:
                        continue
                    got = True
                    bc = " ".join(blocks[int(x)] for x in re.findall(r"\x00(\d+)\x00", piece) if int(x) < len(blocks))
                    fields.append({"name": fm_.group(1), "type": fm_.group(2).strip(), "line": start + li,
                                   "comment": " ".join([bc, line_comment] + pending).strip()})
                if got:
                    pending = []
            struct_defs.append({"file": p, "line": start + l0, "end": start + l1, "name": m.group(1), "fields": fields,
                                "repr": attrs, "decl": decl, "block_has_repr": block_has_repr})
        for m in RX_NEWTYPE.finditer(blank):
            struct_defs.append({"file": p, "line": start + line_of(m.start()), "name": m.group(1),
                                "newtype": m.group(2)})
        for m in RX_ALIAS.finditer(blank):
            struct_defs.append({"file": p, "line": start + line_of(m.start()), "name": m.group(1),
                                "alias": m.group(2)})
  for _ in range(10):  # fixpoint over newtypes / aliases / nested structs / data enums
    changed = False
    for sd in struct_defs:
        if "newtype" in sd:
            changed |= D.put(sd["file"], sd["name"], type_size(sd["newtype"], sd["file"]))
        elif "alias" in sd:
            changed |= D.put(sd["file"], sd["name"], type_size(sd["alias"], sd["file"]))
        elif "enum_data" in sd:
            changed |= D.put(sd["file"], sd["name"], enum_layout(sd))
        else:
            lay = layout(sd)
            if lay:
                changed |= D.put(sd["file"], sd["name"], (lay[0], lay[1]))
    if not changed:
        break
  return struct_defs


def layout(sd):
    """(size, align, plain_sum) under repr(C) rules, or None if a field type is unresolved."""
    packed = "packed" in sd["repr"]
    am = re.search(r"align\((\d+)\)", sd["repr"])
    off, mx, plain = 0, 1, 0
    if not sd["fields"]:
        return None
    for fd in sd["fields"]:
        sa = type_size(fd["type"], sd["file"])
        if sa is None:
            return None
        sz, al = sa
        if packed:
            al = 1
        off = (off + al - 1) // al * al + sz
        mx = max(mx, al)
        plain += sz
    if am:
        mx = max(mx, int(am.group(1)))
    size = (off + mx - 1) // mx * mx
    return size, mx, plain


def enum_layout(sd):
    """repr(C, uN) / repr(uN) enum with payloads: tag, then a union of the payload tuples (repr(C) rules)."""
    tag, pls = sd["enum_data"]
    ts, ta = PRIM[tag]
    umax, ual = 0, 1
    for pl in pls:
        off, al = 0, 1
        for t in pl:
            sa = type_size(t, sd["file"])
            if sa is None:
                return None
            off = (off + sa[1] - 1) // sa[1] * sa[1] + sa[0]
            al = max(al, sa[1])
        umax, ual = max(umax, (off + al - 1) // al * al), max(ual, al)
    al = max(ta, ual)
    return (((ts + ual - 1) // ual * ual + umax + al - 1) // al * al, al)


struct_defs, D = parse_code(texts, ROOT)

RX_CODE_FIELD = re.compile(r"(kind|level|purpose|role|mode|status|reason|code)$")
RX_CODE_PTR = re.compile(r"=|#|§|\]\(|码表|码值|登记|见|自定义"           # values, anchors, owner pointers
                         r"|\d+\s*[A-Za-z一-鿿]\w*\s*/\s*\d+"    # value list "0 无 / 1 First / 2 Second"
                         r"|同\s*`?[A-Z]\w*")                            # "同 SegmentRoll.reason"
def pad_gaps(sd):
    """Where implicit padding sits in a natural repr(C) layout: ["after f: n B", "tail: n B"]."""
    off, mx, out = 0, 1, []
    prev = None
    for fd in sd["fields"]:
        sz, al = type_size(fd["type"], sd["file"])
        start = (off + al - 1) // al * al
        if start > off and prev:
            out.append(f"after {prev}: {start - off} B")
        off, mx, prev = start + sz, max(mx, al), fd["name"]
    end = (off + mx - 1) // mx * mx
    if end > off:
        out.append(f"tail: {end - off} B")
    return out


layout_checked = layout_skipped = codeval_checked = pad_checked = pad_skipped = 0
for sd in struct_defs:
    if "fields" not in sd or not spec_file(sd["file"]):
        continue
    rr = rroot(sd["file"])
    own_c = "C" in re.split(r"[,\s]+", sd["repr"])
    # ⑤ PAD (round 8e): every struct with its own #[repr(C …)] (declared size or not) must have no implicit
    # padding in its natural layout; tail padding that only align(N) adds (cache-line alignment) is allowed.
    if own_c and "packed" not in sd["repr"]:
        nat = layout(dict(sd, repr="C"))
        if nat is None:
            pad_skipped += 0 if is_frozen(sd["file"]) else 1
        else:
            pad_checked += 0 if is_frozen(sd["file"]) else 1
            if nat[2] != nat[0]:
                add("PAD", sd["file"], f"{rr}:{sd['line']}: {sd['name']} fields sum {nat[2]} B, repr(C) {nat[0]} B"
                                       f" — implicit padding {nat[0] - nat[2]} B ({'; '.join(pad_gaps(sd))})")
    is_layout = bool(own_c or sd["block_has_repr"] or sd["decl"])
    if is_layout:
        for fd in sd["fields"]:
            if fd["name"].startswith("_") or fd["type"] not in INT_TYPES or not RX_CODE_FIELD.search(fd["name"]):
                continue
            codeval_checked += 1
            c = fd["comment"]
            if RX_CODE_PTR.search(c) or any(e in c for e in D.enums):
                continue
            add("CODEVAL", sd["file"], f"{rr}:{fd['line']}: {sd['name']}.{fd['name']}: {fd['type']}"
                                       f" — comment: {c.strip()[:60]!r}")
    if not sd["decl"]:
        continue
    lay = layout(sd)
    if lay is None:
        if not is_frozen(sd["file"]):
            layout_skipped += 1
        continue
    if not is_frozen(sd["file"]):
        layout_checked += 1
    size, al, plain = lay
    if size not in sd["decl"]:
        add("LAYOUT", sd["file"], f"{rr}:{sd['line']}: {sd['name']} declared {'/'.join(map(str, sorted(sd['decl'])))} B,"
                                  f" fields sum {plain} B, repr(C) layout {size} B (align {al})")
    elif plain != size and not own_c:   # own #[repr(C)] structs are covered by the hard PAD check
        add("IMPLPAD", sd["file"], f"{rr}:{sd['line']}: {sd['name']} {size} B but fields sum {plain} B"
                                   f" (implicit padding {size - plain} B)")

# ---------- ⑥ NOTEREG: Note{X} / Timer{X} names vs the hft-event registry
EVENT = os.path.join(ROOT, "crates", "hft-event", "README.md")
RX_IDENT = re.compile(r"(?<![A-Za-z0-9_])([A-Z][A-Za-z0-9_]*)")


def registry_names(p):
    s = texts.get(p) or (read(p) if os.path.exists(p) else "")
    names = set()
    for _, line, in_code, _ in walk_lines(s):
        if not in_code and line.lstrip().startswith("|"):
            names.update(RX_IDENT.findall(line.replace("`", " ")))
    names.update(re.findall(r"(?<![A-Za-z0-9_])([A-Z][A-Za-z0-9_]*)\s*=\s*\d", s))
    return names


REG = registry_names(EVENT)
HEADNAMES = set(re.findall(r"^\|\s*`(\w+)`\s*\|\s*\d+\s*\|", texts.get(EVENT, ""), flags=re.M))
# strategy Note codes live in each strategy's StrategyDesc.notes (its README tables), not in hft-event
STRAT_REG = set()
for _s in STRATS:
    _p = os.path.join(ROOT, "strategies", _s, "README.md")
    if os.path.exists(_p) and _s != "registry":
        STRAT_REG |= registry_names(_p)
RX_NOTE = re.compile(r"(?<![A-Za-z0-9_])(Note|Timer)\{([^{}\n]*)")
for p, s in texts.items():
    if not spec_file(p) or not REG:
        continue
    rr = rroot(p)
    extra = STRAT_REG
    bad = defaultdict(list)
    for i, line in enumerate(s.splitlines(), 1):
        for m in RX_NOTE.finditer(line):
            kind, body = m.group(1), m.group(2).replace("\\|", "|")
            if ":" in body:          # struct literal in code (Timer{tag: …})
                continue
            toks = []
            if "=" in body:
                toks = re.findall(r"(?:code|source|owner)\s*=\s*([A-Za-z_][\w|/]*)", body)
            else:
                parts = [x.strip() for x in body.split(",")]
                toks = [parts[0]]
                if parts[0] in HEADNAMES and len(parts) > 1:
                    toks.append(parts[1])
            for t in toks:
                for name in re.split(r"[|/]", t):
                    name = name.strip()
                    if not re.fullmatch(r"[A-Z][A-Za-z0-9_]+", name):   # single letters are placeholders (Note{X})
                        continue
                    if name not in REG and name not in extra:
                        bad[f"{kind}{{{name}}}"].append(i)
    if bad and p != EVENT:
        for k, ls in sorted(bad.items()):
            add("NOTEREG", p, f"{rr}:{ls[0]}: {k}" + (f" (+{len(ls) - 1} more)" if len(ls) > 1 else ""))
    elif bad:
        for k, ls in sorted(bad.items()):
            add("NOTEREG", p, f"{rr}:{ls[0]}: {k} used in the registry file itself but not in its tables")

# ---------- ⑧ RESID8D
RESID8D = [
    ("busy_poll=25", re.compile(r"busy_(?:poll|read)(?:\s*/\s*busy_read)?\s*=\s*25(?!\d)")),
    ("napi_defer_hard_irqs=2", re.compile(r"napi_defer_hard_irqs\s*=\s*2(?!\d)")),
    ("Conflicts=hft-replay-verify", re.compile(r"Conflicts\s*=\s*`?hft-replay-verify")),
    ("c8 或更大/更高", re.compile(r"c8`?\s*或更(?:大|高)")),
    ("kernel_rx ≤ 4 µs", re.compile(r"kernel_rx[^\n]{0,40}?≤\s*4\s*(?:µs|us)(?![\w.])")),
    ("xtask ledger", re.compile(r"xtask\s+ledger(?:\s+rebuild)?")),
    ("dev2 不跑 fuzz", re.compile(r"dev2[^\n。;;]{0,8}不(?:跑|运行)[^\n。;;]{0,8}(?:fuzz|DST)")),
    ("pull_request_review", re.compile(r"pull_request_review")),
]
RX_R8D_NEG = re.compile(r"删除|不再|取代|退役|否决|旧规格|去掉|不是|而非|不设|原先|原来|曾经|过去")
RX_PRR_OK = re.compile(r"decoupling|不(?:由|触发|跑|重跑)|只跑")
for p, s in texts.items():
    if in_spec(p):
        if not spec_file(p):
            continue
        r = rroot(p)
    else:
        if skip_history(p):
            continue
        r = rel(p)
    for i, line in enumerate(s.splitlines(), 1):
        for label, rx in RESID8D:
            if not rx.search(line) or RX_R8D_NEG.search(line):
                continue
            if label == "pull_request_review" and RX_PRR_OK.search(line):
                continue
            add("RESID8D", p, f"{r}:{i}: [{label}] {line.strip()[:80]}")

# =====================================================================================================
# round 8e checks (M0-e brief §4 = W3 §4 items ①–⑫); hft spec files (spec_file), design chapters only where noted
# =====================================================================================================
RX_CELL_SPLIT = re.compile(r"(?<!\\)\|")
RX_TABLE_RULE = re.compile(r"^\|?\s*:?-{2,}:?\s*(?:\|\s*:?-{2,}:?\s*)*\|?\s*$")
RX_BT = re.compile(r"`([^`\n]+)`")


def split_row(line):
    t = line.strip()
    if t.startswith("|"):
        t = t[1:]
    if t.endswith("|") and not t.endswith("\\|"):
        t = t[:-1]
    return [c.strip().replace("\\|", "|") for c in RX_CELL_SPLIT.split(t)]


_tables_cache = {}


def md_tables(p):
    """[(header_line_no, header_cells, [(line_no, cells)])] for the pipe tables of file p (outside fences)."""
    if p in _tables_cache:
        return _tables_cache[p]
    s = texts.get(p) or (read(p) if os.path.exists(p) else "")
    lines, out, in_code, i = s.splitlines(), [], False, 0
    while i < len(lines):
        st = lines[i].lstrip()
        if st.startswith("```") or st.startswith("~~~"):
            in_code = not in_code
        elif not in_code and st.startswith("|") and i + 1 < len(lines) and RX_TABLE_RULE.match(lines[i + 1].strip()):
            hdr, rows, j = split_row(lines[i]), [], i + 2
            while j < len(lines) and lines[j].lstrip().startswith("|"):
                rows.append((j + 1, split_row(lines[j])))
                j += 1
            out.append((i + 1, hdr, rows))
            i = j
            continue
        i += 1
    _tables_cache[p] = out
    return out


def hft(r):
    return os.path.join(ROOT, *r.split("/"))


def spec_items():
    """(path, display name) of the hft spec files."""
    return [(p, rroot(p)) for p in texts if spec_file(p)]


def design_items():
    """(path, display name) of the current-state design chapters (history files skipped)."""
    return [(p, rel(p)) for p in texts if not in_spec(p) and not skip_history(p)]


def prose_lines(s):
    """(lineno, line, in_code) with fence lines themselves dropped."""
    for i, line, in_code, _ in walk_lines(s):
        st = line.lstrip()
        if st.startswith("```") or st.startswith("~~~"):
            continue
        yield i, line, in_code


# ---------- ① IDREG / PATHREG / S3CRED / S3REF[W] / IDOPT[W]: identity and path registry (home: deploy/README.md)
DEPLOY_MD = hft("deploy/README.md")
ARCHIVER_MD = hft("apps/journal-archiver/README.md")
TOK = r"hft(?:-[a-z0-9]+)*"
TOK_END = r"(?![\w@.-])"      # not a unit/instance name (hft-core@prod, hft-verify.service) nor a longer token
RX_IDREF = [
    re.compile(r"(?:\buid|\bUID|User=|Group=|runuser\s+-u|subject\.user\s*==)\s*[`\"]?(" + TOK + ")" + TOK_END),
    re.compile(r"(?<![元活分按本各每该两组])组\s*`(" + TOK + ")`"),
    re.compile(r"用户\s*`?(" + TOK + ")" + TOK_END),
    re.compile(r"`(" + TOK + r")`\s*[((]?\s*(?:用户|\buid|\bUID|组(?![件合成激链内别]))"),
    re.compile(r"`(?:" + TOK + r"|root|<[^>`]*>):(" + TOK + r")`"),          # owner:group
    re.compile(r"`(" + TOK + r"):(?:" + TOK + r"|root)`"),                     # owner of owner:group
]
RX_SUPP = re.compile(r"SupplementaryGroups=([A-Za-z0-9 _-]+)")
RX_OPTLINE = re.compile(r"可选|optional|链上|chain")


def id_refs(line):
    toks = set()
    for rx in RX_IDREF:
        toks.update(m.group(1) for m in rx.finditer(line))
    for m in RX_SUPP.finditer(line):
        toks.update(t for t in m.group(1).split() if re.fullmatch(TOK, t))
    return toks


RX_IDCOL = re.compile(r"^(?:uid|UID|用户|系统用户|用户名|属主|属组|附加组)$")


def id_table_refs(p):
    """{line_no: {tokens}} from table columns headed uid / 用户 / 属主 … and rows whose first cell is 系统用户."""
    out = defaultdict(set)
    for _hl, hdr, rows in md_tables(p):
        cols = [k for k, c in enumerate(hdr) if RX_IDCOL.match(c)]
        for ln, cells in rows:
            cs = list(cols) if not (cells and cells[0] in ("用户", "系统用户", "uid")) else range(1, len(cells))
            for k in cs:
                if k < len(cells):
                    out[ln].update(t for t in RX_BT.findall(cells[k]) if re.fullmatch(TOK, t))
    return out


ID_REG = set()
if os.path.exists(DEPLOY_MD):
    _ds = texts.get(DEPLOY_MD) or read(DEPLOY_MD)
    for _line in _ds.splitlines():
        ID_REG |= id_refs(_line)
        _m = re.search(r"创建用户与组[((]含([^))]*)[))]", _line)
        if _m:
            ID_REG.update(t for t in RX_BT.findall(_m.group(1)) if re.fullmatch(TOK, t))
    for _ts in id_table_refs(DEPLOY_MD).values():
        ID_REG |= _ts

RX_HFTPATH = re.compile(r"(?<![\w./-])/(opt|run|var/lib)/hft((?:/(?:\{[^}\s`]*\}|[^\s/`'\"()(),,;;::。、|\[\]{}])+)*)")


def path_comps(base, tail):
    comps = [c for c in tail.split("/") if c]
    while comps and comps[-1] in ("…", "...", "*"):
        comps.pop()
    if comps:
        comps[-1] = comps[-1].rstrip(".")
    return (base, tuple(c.replace("%i", "<instance>") for c in comps if c))


def comp_rx(c):
    out = ""
    for part in re.split(r"(<[^>]*>|%[a-zA-Z]|\{[^}]*\}|\*|…)", c):
        if not part:
            continue
        if part[0] in "<%*…":
            out += r"[^/]+"
        elif part.startswith("{"):
            out += "(?:" + "|".join(re.escape(x) for x in part[1:-1].split(",")) + ")"
        else:
            out += re.escape(part)
    return out


def comp_match(a, b):
    if a == b:
        return True
    if a.startswith("{") and b.startswith("{"):
        sa, sb = set(a[1:-1].split(",")), set(b[1:-1].split(","))
        return sa <= sb or sb <= sa
    return bool(re.fullmatch(comp_rx(a), b) or re.fullmatch(comp_rx(b), a))


def comps_match(r, p):
    n = min(len(r), len(p))
    return n >= 1 and all(comp_match(r[k], p[k]) for k in range(n))


def is_rule(c):
    """A first component that is only a placeholder (`<unit>`, `%i`, `…`) is a naming rule, not a registration."""
    return not c or re.fullmatch(r"(?:<[^>]*>|%[a-zA-Z]|…|\.\.\.|\*)+", c[0]) is not None


PATH_REG = set()
for _m in RX_HFTPATH.finditer(texts.get(DEPLOY_MD, "")):
    _b, _c = path_comps(_m.group(1), _m.group(2))
    if not is_rule(_c):
        PATH_REG.add((_b, _c))
for _m in re.finditer(r"(Runtime|State)Directory=hft/([^\s`、,;;)()]+)", texts.get(DEPLOY_MD, "")):
    _b, _c = path_comps("run" if _m.group(1) == "Runtime" else "var/lib", _m.group(2))
    if not is_rule(_c):
        PATH_REG.add((_b, _c))

# S3: the archiver's prefixes must all be covered by the hft-archive consumer's credential prefixes
RX_S3TOK = re.compile(r"^(hft(?:-[a-z]+)?)/[^\s]*$")
RX_S3CTX = re.compile(r"S3|s3://|前缀|prefix|对象键|bucket|桶")
S3_REG, ARCH_CRED, ARCH_PREFIX = set(), set(), {}


def s3norm(t):
    t = t.strip().strip("/").replace("…", "").rstrip("/")
    return t + "/"


for _i, _line in enumerate(texts.get(DEPLOY_MD, "").splitlines(), 1):
    _m = re.search(r"`hft-archive`\s*[((]([^))]*)[))]", _line)
    if _m and ("S3" in _m.group(1) or "前缀" in _m.group(1)):
        ARCH_CRED.update(s3norm(t) for t in RX_BT.findall(_m.group(1)) if "/" in t or re.fullmatch(r"hft(?:-[a-z]+)?", t))
    if RX_S3CTX.search(_line):
        for t in RX_BT.findall(_line):
            if RX_S3TOK.match(t):
                S3_REG.add(path_comps(t.split("/")[0], "/".join(t.split("/")[1:])))
        for t in re.findall(r"prefix\s*=\s*\"([^\"]+)\"", _line):
            S3_REG.add((t.split("/")[0], tuple(c for c in t.split("/")[1:] if c)))
            ARCH_PREFIX.setdefault(s3norm(t), f"deploy/README.md:{_i}")
for _i, _line in enumerate(texts.get(ARCHIVER_MD, "").splitlines(), 1):
    for t in re.findall(r"prefix\s*=\s*\"([^\"]+)\"", _line):
        ARCH_PREFIX.setdefault(s3norm(t), f"apps/journal-archiver/README.md:{_i}")
    for t in re.findall(r"prefix`?\s*缺省\s*(?:=\s*)?`([^`]+)`", _line):
        if not t.startswith("s3."):
            ARCH_PREFIX.setdefault(s3norm(t), f"apps/journal-archiver/README.md:{_i}")
    _m = re.match(r"^\|\s*`s3\.prefix`\s*\|[^|]*\|\s*`([^`]+)`", _line)
    if _m:
        ARCH_PREFIX.setdefault(s3norm(_m.group(1)), f"apps/journal-archiver/README.md:{_i}")
if ARCH_PREFIX and not ARCH_CRED:
    add("S3CRED", DEPLOY_MD, "deploy/README.md: no `hft-archive`(S3 … `<prefix>/` …) consumer entry found")
for _a, _where in sorted(ARCH_PREFIX.items()):
    if ARCH_CRED and not any(_a.startswith(c) for c in ARCH_CRED):
        add("S3CRED", DEPLOY_MD, f"archiver prefix `{_a}` ({_where}) not covered by the hft-archive consumer"
                                 f" credential prefixes {sorted(ARCH_CRED)} (deploy/README.md)")

id_ref_n = path_ref_n = s3_ref_n = 0
path_top_bad = defaultdict(list)    # (base, first component) never registered -> [(path, "file:line")]
for p, r in spec_items() + design_items():
    in_hft = in_spec(p)
    tab_ids = id_table_refs(p) if in_hft else {}
    for i, line, in_code in prose_lines(texts[p]):
        if in_hft and p != DEPLOY_MD:
            for t in sorted(id_refs(line) | tab_ids.get(i, set())):
                id_ref_n += 1
                if t not in ID_REG:
                    cat = "IDOPT" if RX_OPTLINE.search(line) else "IDREG"
                    add(cat, p, f"{r}:{i}: user/group `{t}` not defined in deploy/README.md")
        if p != DEPLOY_MD:
            for m in RX_HFTPATH.finditer(line):
                b, c = path_comps(m.group(1), m.group(2))
                if not c or c[0] in ("…", "..."):
                    continue
                path_ref_n += 1
                if any(b == pb and comps_match(c, pc) for pb, pc in PATH_REG):
                    continue
                if not any(b == pb and comp_match(c[0], pc[0]) for pb, pc in PATH_REG):
                    path_top_bad[(b, c[0])].append((p, f"{r}:{i}"))
                else:
                    add("PATHREG", p, f"{r}:{i}: /{b}/hft/{'/'.join(c)} not in the deploy/README.md path registry")
            if in_hft and RX_S3CTX.search(line) and p != ARCHIVER_MD:
                for t in RX_BT.findall(line):
                    if not RX_S3TOK.match(t) or t.startswith("hft/live/_"):
                        continue
                    b, c = path_comps(t.split("/")[0], "/".join(t.split("/")[1:]))
                    if not c:
                        continue
                    s3_ref_n += 1
                    if not any(b == pb and (not pc or comps_match(c, pc)) for pb, pc in S3_REG):
                        add("S3REF", p, f"{r}:{i}: S3/secret prefix `{t}` not in the deploy/README.md registry")
for (b, c0), refs in sorted(path_top_bad.items()):
    live = [w for q, w in refs if not is_frozen(q)]
    if not live:
        suppressed["PATHREG"] += len(refs)
        continue
    problems["PATHREG"].append(f"/{b}/hft/{c0}/… not in the deploy/README.md path registry ({len(live)} refs: "
                               f"{', '.join(live[:6])}{' …' if len(live) > 6 else ''})")

# ---------- ② CFGDUP / CFGPATH / CFGREF[W]: configuration-key registry
# namespace of each config table: hft-core's TOML = hft-core + hft-venue (under venues.<v>.) + hft-rt + hft-sim [sim];
# every other file is its own namespace (app TOML, strategy params, ECF1 sections)
CFG_NS = {"apps/hft-core/README.md": "", "crates/hft-venue/README.md": "venues.<v>.",
          "crates/hft-rt/README.md": "", "crates/hft-sim/README.md": "sim."}
RX_DEF_PTR = re.compile(r"^(?:见|按|取|由|同|—|–|-|无|必填|deploy|生成|来自|随|与|=|n/a|N/A|该|各|空\b)")
CORE_TOP = ("instance", "paths", "blobs", "host", "advisor", "startup", "shutdown", "watchdog", "venues")


def cfg_keys(cell):
    keys, last = [], None
    for t in RX_BT.findall(cell):
        t = t.strip()
        if t.startswith(".") and last and "." in last:      # `a.b.c` / `.d` and `venues.<v>.gw.x` / `.gw.y`
            t = ".".join(last.split(".")[:-t.count(".")]) + t
        if not re.fullmatch(r"(?:\[\[?)?[A-Za-z_][\w<>*\[\].-]*(?:\]\]?)?", t) or t.endswith("."):
            continue
        keys.append(re.sub(r"<venue>", "<v>", t))
        last = t
    return keys


def cfg_norm(k):
    k = re.sub(r"^venues\.(?!<v>)[^.]+\.", "venues.<v>.", k)
    return k


cfg_defs = defaultdict(list)       # (ns, key) -> [(file, line, default)]
CORE_KEYS = set()                  # every key of hft-core's TOML namespace (full path)
CORE_FILE_KEYS = set()             # the keys written in hft-core's own 配置文件字段 table
venue_keys = []                    # (line, key) from hft-venue
cfg_tables_n = 0
for p, r in spec_items():
    for hl, hdr, rows in md_tables(p):
        dcol = next((k for k, c in enumerate(hdr) if "默认" in c), None)
        if dcol is None or dcol == 0 or not re.search(r"键|名称|配置|参数|字段|项", hdr[0]):
            continue
        cfg_tables_n += 1
        pre = CFG_NS.get(r)
        for ln, cells in rows:
            if len(cells) <= dcol:
                continue
            keys = cfg_keys(cells[0])
            dv = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", cells[dcol]).strip().strip("`").strip()
            has_def = bool(dv) and not RX_DEF_PTR.match(dv)
            for k in keys:
                full = cfg_norm(k if pre is None or k.startswith(pre) or k.startswith(("rt.", "[")) else pre + k)
                ns = "core" if pre is not None else r
                if pre is not None:
                    CORE_KEYS.add(full)
                if r == "apps/hft-core/README.md":
                    CORE_FILE_KEYS.add(full)
                if r == "crates/hft-venue/README.md":
                    venue_keys.append((ln, full))
                if has_def:
                    cfg_defs[(ns, full)].append((r, ln, dv[:40]))
for (ns, k), defs in sorted(cfg_defs.items()):
    if len({(f, ln) for f, ln, _ in defs}) > 1:
        where = "; ".join(f"{f}:{ln} = {v!r}" for f, ln, v in defs)
        add("CFGDUP", hft(defs[0][0]), f"{ns}: `{k}` has a default in {len(defs)} places: {where}")


def core_key_known(k):
    for K in CORE_KEYS:
        if K == k or K.startswith(k + ".") or k.startswith(K + ".") or k.startswith(K + "["):
            return True
        if "*" in K and re.fullmatch(re.escape(K).replace(r"\*", r"[^\s]+"), k):
            return True
    return False


_seen = set()
for ln, k in venue_keys:
    seg = ".".join(k.split(".")[:3])          # venues.<v>.<first segment>
    if seg in _seen:
        continue
    _seen.add(seg)
    if not any(K == seg or K.startswith(seg + ".") or K.startswith(seg + "[") for K in CORE_FILE_KEYS):
        add("CFGPATH", hft("crates/hft-venue/README.md"),
            f"crates/hft-venue/README.md:{ln}: `{k}` — TOML table `{seg}` has no row in hft-core 配置文件字段")

cfg_ref_n = 0
RX_OWN_TOML = re.compile(r"^(?:apps/(?!hft-core/)|python/|strategies/)")   # other apps / packages have their own TOML
for p, r in spec_items():
    own_toml = bool(RX_OWN_TOML.match(r))
    for i, line, in_code in prose_lines(texts[p]):
        if own_toml and "hft-core" not in line:
            continue
        for t in RX_BT.findall(line):
            m = re.fullmatch(r"([a-z_]+)\s*=\s*\S.*", t)
            if m and "hft-core" in line and r != "apps/hft-core/README.md":
                name = m.group(1)
                if name not in CORE_KEYS and any(K.startswith(name + ".") for K in CORE_KEYS):
                    add("CFGREF", p, f"{r}:{i}: `{t}` assigns a value to hft-core table `{name}` (keys: "
                                     f"{', '.join(sorted(K for K in CORE_KEYS if K.startswith(name + '.'))[:3])} …)")
                continue
            if (re.fullmatch(r"(?:" + "|".join(CORE_TOP) + r")\.[\w<>*\[\].-]*[\w>*\]]", t)
                    and not re.search(r"\.(?:rs|toml|json|md|py|sh|service)$", t)):
                cfg_ref_n += 1
                k = cfg_norm(re.sub(r"<venue>", "<v>", t))
                if not core_key_known(k):
                    add("CFGREF", p, f"{r}:{i}: `{t}` not a key of hft-core's TOML (hft-core / hft-venue / hft-rt /"
                                     f" hft-sim config tables)")

# ---------- ③ CLIFLAG / CLIREQ[W] / CLISUB[W]: CLI signatures (homes: tools = cargo xtask + hft-verify,
# hft-testkit = hft-dst, apps/replayer = replayer) vs every `--flag` used with that command anywhere
CLI_OWNERS = {"cargo xtask": "tools/README.md", "hft-verify": "tools/README.md",
              "hft-dst": "crates/hft-testkit/README.md", "replayer": "apps/replayer/README.md"}
RX_PROG = re.compile(r"(?<![\w/.-])(cargo xtask|hft-verify|hft-dst|replayer)(?=\s+[a-z-])")
RX_FLAG = re.compile(r"(?<![\w-])--([a-z][a-z0-9-]*)")
CLI_SIGS = defaultdict(lambda: defaultdict(list))   # prog -> sub(tuple) -> [(all flags, required flags)]
CLI_KNOWN_SUB = defaultdict(set)                     # prog -> {sub tuple} (incl. heading-only subcommands)
CLI_OPEN = set()                                     # (prog, sub) whose signature ends in "…" (more flags allowed)
CLI_GLOBAL = defaultdict(set)                        # prog -> flags documented in a section headed by the flag
cli_def_lines = set()


def cli_split(rest):
    """(sub words, all flags, required flags) of the text after the program name."""
    words = rest.split()
    sub = []
    for w in words:
        if not re.fullmatch(r"[a-z][a-z0-9-]*", w):
            break
        sub.append(w)
    allf, req, depth = set(), set(), 0
    for m in re.finditer(r"\[|\]|--[a-z][a-z0-9-]*", rest):
        x = m.group(0)
        if x == "[":
            depth += 1
        elif x == "]":
            depth = max(0, depth - 1)
        else:
            allf.add(x[2:])
            if depth == 0:
                req.add(x[2:])
    # `--a <x> | --b <y>` at top level: alternatives, neither alone is required
    for m in re.finditer(r"--([a-z][a-z0-9-]*)(?:\s+<[^>]*>)?\s*\|\s*--([a-z][a-z0-9-]*)", rest):
        req.discard(m.group(1))
        req.discard(m.group(2))
    return tuple(sub), allf, req


def cli_add_sig(prog, rest, where):
    rest = re.split(r"\s#", rest, maxsplit=1)[0]
    sub, allf, req = cli_split(rest)
    if sub:
        CLI_SIGS[prog][sub].append((allf, req))
        CLI_KNOWN_SUB[prog].add(sub)
        cli_def_lines.add(where)
        if "…" in rest.split(sub[-1], 1)[-1] or "..." in rest:
            CLI_OPEN.add((prog, sub))
    return sub


for prog, owner in CLI_OWNERS.items():
    op = hft(owner)
    if op not in texts:
        continue
    last_sig, last_i = None, -9
    for i, line, in_code in prose_lines(texts[op]):
        m = re.match(r"^\s*" + re.escape(prog) + r"\s+(.*)$", line)
        if in_code and m:
            last_sig, last_i = cli_add_sig(prog, m.group(1), (op, i)), i
        elif in_code and last_sig and i == last_i + 1 and re.match(r"^\s+[\[-]", line):   # continuation line
            _, allf, req = cli_split("x " + re.split(r"\s#", line, maxsplit=1)[0])
            a0, r0 = CLI_SIGS[prog][last_sig][-1]
            CLI_SIGS[prog][last_sig][-1] = (a0 | allf, r0 | req)
            cli_def_lines.add((op, i))
            last_i = i
        fm = re.match(r"^#{2,6}\s+`--([a-z][a-z0-9-]*)`", line)
        if fm:
            CLI_GLOBAL[prog].add(fm.group(1))
        hm = re.match(r"^#{2,6}\s+`?" + re.escape(prog) + r"\s+([a-z][a-z0-9 -]*)`?\s*$", line)
        if hm:
            CLI_KNOWN_SUB[prog].add(tuple(hm.group(1).split()))
    if prog == "cargo xtask":    # the 其他 xtask 子命令 table: each backticked first-cell token is a signature
        for hl, hdr, rows in md_tables(op):
            if hdr and hdr[0] == "子命令":
                for ln, cells in rows:
                    for t in RX_BT.findall(cells[0]):
                        cli_add_sig(prog, t, (op, ln))
    if prog == "replayer":       # replayer headings name subcommands: ### verify / ### bench(可选扩展)
        for i, line, in_code in prose_lines(texts[op]):
            hm = re.match(r"^###\s+([a-z][a-z-]*)(?:\s|[((]|$)", line)
            if hm:
                CLI_KNOWN_SUB[prog].add((hm.group(1),))


def cli_spans(s):
    """(lineno, span, in_code) — inline code spans and code-block lines (comment after ' #' dropped)."""
    for i, line, in_code in prose_lines(s):
        if in_code:
            yield i, re.split(r"\s#\s", line, maxsplit=1)[0], True
        else:
            for t in RX_BT.findall(line):
                yield i, t, False


cli_use_n = 0
for p, r in spec_items() + design_items():
    for i, span, in_block in cli_spans(texts[p]):
        if (p, i) in cli_def_lines:
            continue
        ms = list(RX_PROG.finditer(span))
        for k, m in enumerate(ms):
            prog = m.group(1)
            if not CLI_KNOWN_SUB.get(prog):
                continue
            rest = span[m.end():ms[k + 1].start()] if k + 1 < len(ms) else span[m.end():]
            rest = re.split(r"&&|;|\|\|", rest, maxsplit=1)[0]
            words, flags, _ = cli_split(rest)
            sub = max((s_ for s_ in CLI_KNOWN_SUB[prog] if words[:len(s_)] == s_), key=len, default=None)
            if sub is None:
                if prog == "cargo xtask" and words and not any(s_[0] == words[0] for s_ in CLI_KNOWN_SUB[prog]):
                    add("CLISUB", p, f"{r}:{i}: `cargo xtask {' '.join(words[:2])}` — subcommand not in tools/README.md")
                continue
            sigs = CLI_SIGS[prog].get(sub)
            if not sigs:
                continue
            used = set(RX_FLAG.findall(rest))
            cli_use_n += 1
            known = set().union(*(a for a, _ in sigs)) | CLI_GLOBAL[prog]
            bad = sorted(used - known) if (prog, sub) not in CLI_OPEN else []
            if bad:
                add("CLIFLAG", p, f"{r}:{i}: `{prog} {' '.join(sub)}` --{', --'.join(bad)} not in its signature "
                                  f"({CLI_OWNERS[prog]}: {' '.join('--' + x for x in sorted(known)) or 'no flags'})")
            elif (used and not any(req <= used for _, req in sigs)
                  and (in_block or (len(used) >= 2 and "…" not in rest and "..." not in rest))):
                need = min((req - used for _, req in sigs), key=len)
                add("CLIREQ", p, f"{r}:{i}: `{prog} {' '.join(sub)}` without required --{', --'.join(sorted(need))}")

# ---------- ④ RINGCAP / RINGHOME / JRNLMEM: ring and journal name closure
# ring names in the hft-sequencer ring table, hft-venue §1 and hft-event 环拓扑 must each have a row in hft-core's
# ring-capacity table (ECF1 段 4); a ring capacity given as a config default anywhere else is a second home.
# core journals (journal-format 三类 journal) must have a row in hft-core's journal-segment and memory tables
# (`*` in `raw-feed-*` stands for one name segment: `raw-feed-<venue>-bbo` needs its own mention).
CORE_MD = hft("apps/hft-core/README.md")
RING_SRC = ["crates/hft-sequencer/README.md", "crates/hft-venue/README.md", "crates/hft-event/README.md"]
RX_RINGVAR = re.compile(r"(?<![\w:<>/.-])((?!raw-)[a-z]+(?:-[a-z]+)*-<(?:venue|v)>(?:-[a-z]+)?)(?![\w:>-])")
RING_FIXED = {"ctl", "hk", "advice", "rep-sim", "out-sim", "chain-in", "chain-out"}
RX_RING_OPT = re.compile(r"optional|可选")


def ring_norm(t):
    return t.replace("<v>", "<venue>")


ring_src = {}            # name -> "file:line"
for rr_ in RING_SRC:
    p = hft(rr_)
    if p not in texts:
        continue
    for hl, hdr, rows in md_tables(p):
        if hdr and hdr[0] == "环":
            for ln, cells in rows:
                if RX_RING_OPT.search(" ".join(cells)):
                    continue
                for t in RX_BT.findall(cells[0]):
                    t = ring_norm(t.split("×")[0].strip())
                    if t in RING_FIXED or RX_RINGVAR.fullmatch(t):
                        ring_src.setdefault(t, f"{rr_}:{ln}")
    for i, line, in_code in prose_lines(texts[p]):
        if RX_RING_OPT.search(line) or not (in_code or "环" in line):
            continue
        if in_code and not re.search(r"──|▶|◀|sequencer", line):
            continue
        for t in RX_RINGVAR.findall(line):
            if line[line.find(t) - 1:line.find(t)] == ":":
                continue
            ring_src.setdefault(ring_norm(t), f"{rr_}:{i}")
ring_cap = set()
cap_table_seen = False
for hl, hdr, rows in md_tables(CORE_MD):
    if hdr and hdr[0] == "环" and any("prod" in c or "dev2" in c for c in hdr[1:]):
        cap_table_seen = True
        for ln, cells in rows:
            names = [ring_norm(t) for t in RX_BT.findall(cells[0]) if not t.startswith("-")]
            sufs = [t for t in RX_BT.findall(cells[0]) if t.startswith("-")]
            for n in names:
                ring_cap.add(n)
                for sfx in sufs:
                    if "<venue>" in n:
                        ring_cap.add(n + sfx)
if CORE_MD in texts and not cap_table_seen:
    add("RINGCAP", CORE_MD, "apps/hft-core/README.md: ring-capacity table (| 环 | … prod … |) not found")
for n, where in sorted(ring_src.items()):
    if cap_table_seen and n not in ring_cap:
        add("RINGCAP", hft(where.split(":")[0]), f"ring `{n}` ({where}) has no row in hft-core's ring-capacity table")
for p, r in spec_items():
    if p == CORE_MD:
        continue
    for hl, hdr, rows in md_tables(p):
        dcol = next((k for k, c in enumerate(hdr) if "默认" in c), None)
        if dcol is None or dcol == 0:
            continue
        for ln, cells in rows:
            if len(cells) <= dcol:
                continue
            dv = cells[dcol].strip().strip("`")
            txt = " ".join(cells[1:])
            rings = [ring_norm(t) for t in RX_BT.findall(txt) if ring_norm(t) in ring_cap]
            if rings and "容量" in txt and dv and re.match(r"\d", dv):
                add("RINGHOME", p, f"{r}:{ln}: {', '.join(cfg_keys(cells[0]))} = {dv} gives the capacity of "
                                   f"`{rings[0]}`, whose only home is hft-core's ring-capacity table (ECF1 段 4)")

JF_MD = hft("docs/subsystems/journal-format.md")
journals = {}
for hl, hdr, rows in md_tables(JF_MD):
    if hdr and hdr[0].startswith("journal"):
        for ln, cells in rows:
            if len(cells) > 1 and "optional" in cells[1]:
                continue
            for t in RX_BT.findall(cells[0]):
                if t == "seq" or t.startswith("raw-"):
                    journals.setdefault(ring_norm(t), f"docs/subsystems/journal-format.md:{ln}")


def journal_rows(pred):
    out = []
    for hl, hdr, rows in md_tables(CORE_MD):
        if hdr and pred(hdr, rows):
            out.append(rows)
    return out


def journal_named(name, rows_list):
    for rows in rows_list:
        for ln, cells in rows:
            c0 = cells[0]
            for t in RX_BT.findall(c0):
                t = ring_norm(t)
                if t == name:
                    return True
                if "*" in t and re.fullmatch(re.escape(t).replace(r"\*", r"[^-\s]+"), name.replace("<venue>", "x")):
                    return True
                if t.startswith("-") and name.endswith(t) and any(name.startswith(ring_norm(u).rstrip("*").rstrip("-"))
                                                                for u in RX_BT.findall(c0) if not u.startswith("-")):
                    return True
    return False


seg_tables = journal_rows(lambda h, rows: h[0] == "journal")
mem_tables = journal_rows(lambda h, rows: h[0] == "部分" and any("热段" in c[0] for _, c in rows))
for n, where in sorted(journals.items()):
    for label, tl in (("journal-segment", seg_tables), ("memory-budget", mem_tables)):
        if tl and not journal_named(n, tl):
            add("JRNLMEM", CORE_MD, f"journal `{n}` ({where}) has no row in hft-core's {label} table")

# ---------- ⑥ SIZECOPY: "`X` N B" restated anywhere (prose or comment) vs the repr(C) layout computed at X's home
struct_size = defaultdict(set)
struct_hdr_lines = set()
for sd in struct_defs:
    if "fields" in sd:
        struct_hdr_lines.add((sd["file"], sd["line"]))
        struct_hdr_lines.add((sd["file"], sd["end"]))
        lay = layout(sd)
        if lay:
            struct_size[sd["name"]].add(lay[0])
RX_SIZECOPY = re.compile(r"(?<![\w.:`])`?([A-Z][A-Za-z0-9]+)`?\s*(?:按当前字段|原样|是|为)?\s*[((]?\s*(\d[\d,]*)\s*B(?![A-Za-z])")
size_copy_n = 0
for p, r in spec_items():
    for i, line, in_code in prose_lines(texts[p]):
        if (p, i) in struct_hdr_lines or re.search(r"记录字节|记录头|头\s*\+", line):
            continue
        for m in RX_SIZECOPY.finditer(line):
            name, n = m.group(1), int(m.group(2).replace(",", ""))
            sizes = struct_size.get(name)
            if not sizes or len(sizes) != 1 or re.search(r"\bstruct\s+" + name + r"\b", line):
                continue
            tail, head = line[m.end():m.end() + 12], line[max(0, m.start() - 8):m.start()]
            if re.match(r"\s*[+×*]", tail) or re.search(r"头|含|记录", tail) or re.search(r"记录|[≤<≥>]\s*$", head):
                continue
            size_copy_n += 1
            (size,) = sizes
            if n != size:
                add("SIZECOPY", p, f"{r}:{i}: `{name}` stated {n} B, its repr(C) layout is {size} B")

# ---------- ⑦ ENUMVAL: #[repr(u*/i*)] enums, and enums declared "逐值相同"/"同名同值" with another type,
# must give every unit variant an explicit discriminant
RX_ENUMDEF = re.compile(r"\benum\s+([A-Z]\w*)\s*\{")
RX_SAMEVAL = re.compile(r"逐值相同|同名同值")
enum_defs = []      # {file, line, name, repr_int, sameval, variants: [(name, has_value, has_data)]}
for p, s in texts.items():
    if not in_spec(p):
        continue
    for start, lines in code_blocks(s):
        sc = scan_comments(lines)
        blank = "\n".join(x[3] for x in sc)
        offs = [0]
        for x in sc:
            offs.append(offs[-1] + len(x[3]) + 1)

        def _line_of(pos):
            lo = 0
            while lo + 1 < len(offs) and offs[lo + 1] <= pos:
                lo += 1
            return lo

        for m in RX_ENUMDEF.finditer(blank):
            ob = m.end() - 1
            depth, cb = 0, None
            for k in range(ob, len(blank)):
                if blank[k] == "{":
                    depth += 1
                elif blank[k] == "}":
                    depth -= 1
                    if depth == 0:
                        cb = k
                        break
            if cb is None:
                continue
            prev_end = max(blank.rfind(";", 0, m.start()), blank.rfind("}", 0, m.start()))
            attrs = blank[prev_end + 1:m.start()]
            rm = re.search(r"#\[repr\(\s*([ui](?:8|16|32|64|size))\s*\)\]", attrs)
            l0, l1 = _line_of(prev_end + 1 if prev_end >= 0 else 0), _line_of(cb)
            com = " ".join(sc[k][2] + " " + " ".join(sc[k][1]) for k in range(l0, l1 + 1))
            variants = []
            for v in split_top(blank[ob + 1:cb]):
                v = v.strip()
                vm = re.match(r"(?:#\[[^\]]*\]\s*)*([A-Z]\w*)", v)
                if vm:
                    rest_ = v[vm.end():].strip()
                    variants.append((vm.group(1), rest_.startswith("="), rest_[:1] in ("(", "{")))
            enum_defs.append({"file": p, "line": start + _line_of(m.start()), "name": m.group(1),
                              "repr_int": rm.group(1) if rm else None, "sameval": bool(RX_SAMEVAL.search(com)),
                              "variants": variants})
sameval_names = defaultdict(list)    # enum name -> ["file:line"] of prose sentences saying 逐值相同 / 同名同值
for p, r in spec_items():
    for i, line, in_code in prose_lines(texts[p]):
        if in_code or not RX_SAMEVAL.search(line):
            continue
        for sent in re.split(r"[。;;]", line):
            if RX_SAMEVAL.search(sent):
                for t in RX_BT.findall(sent):
                    if re.fullmatch(r"[A-Z]\w*", t):
                        sameval_names[t].append(f"{r}:{i}")
enum_checked = 0
for ed in enum_defs:
    why = ([f"#[repr({ed['repr_int']})]"] if ed["repr_int"] else []) + (["comment 逐值相同"] if ed["sameval"] else []) \
        + ([f"said 逐值相同 at {sameval_names[ed['name']][0]}"] if ed["name"] in sameval_names else [])
    if not why or not spec_file(ed["file"]):
        continue
    enum_checked += 0 if is_frozen(ed["file"]) else 1
    missing = [v for v, has_val, has_data in ed["variants"] if not has_val and not has_data]
    if missing:
        add("ENUMVAL", ed["file"], f"{rroot(ed['file'])}:{ed['line']}: enum {ed['name']} ({'; '.join(why)}): "
                                   f"{len(missing)} variant(s) without explicit value ({', '.join(missing[:5])}"
                                   f"{' …' if len(missing) > 5 else ''})")

# ---------- ⑧ NOTESRC: a crate README that writes Note{X} must be the crate of X's registered source
NOTE_SRC_CRATE = {"Sequencer": {"crates/hft-sequencer"}, "Measure": {"crates/hft-sequencer"},
                  "Advisor": {"crates/hft-sequencer"}, "Risk": {"crates/hft-risk"}, "Oms": {"crates/hft-oms"},
                  "Ledger": {"crates/hft-ledger"}, "Exec": {"crates/hft-exec"}, "Venue": {"crates/hft-venue"},
                  "CtlAudit": {"crates/hft-ctl"}, "Housekeeping": {"apps/hft-core"}, "Boot": {"apps/hft-core"}}
NOTE_SRC = {}
for hl, hdr, rows in md_tables(EVENT):
    if len(hdr) > 1 and hdr[0] == "码名" and hdr[1].strip("`") == "source":
        for ln, cells in rows:
            src = cells[1].strip().strip("`")
            for t in RX_BT.findall(cells[0]):
                if re.fullmatch(r"[A-Z]\w*", t):
                    NOTE_SRC[t] = src
RX_NOTE_PROD = re.compile(r"(写|产出|产生|发出|输出|发)\s*(?:一条\s*|一次\s*)?(?:(?:P[0-3]|Info)\s*(?:级\s*)?)?(?:的\s*)?`?"
                          r"Note\{\s*(?:(?:source\s*=\s*)?(\w+)\s*,\s*(?:code\s*=\s*)?)?(\w+)\s*\}")
RX_OTHER_ACTOR = re.compile(r"sequencer|OMS|oms|ledger|执行层|风控|gw|feed|housekeeping|advisor|reconciler|watchtower|"
                            r"hft-core|ctl-gateway|gateway|策略|SDK|kill-switch|驱动|driver|hft-[a-z]+")
CRATE_ACTOR = {"crates/hft-sequencer": r"sequencer|advisor|驱动|driver", "crates/hft-oms": r"OMS|oms",
               "crates/hft-ledger": r"ledger", "crates/hft-exec": r"执行层|exec", "crates/hft-risk": r"风控|risk",
               "crates/hft-venue": r"gw|feed", "apps/hft-core": r"hft-core|housekeeping|驱动|driver",
               "crates/hft-ctl": r"gateway|ctl"}
SRC_ACTOR = {"Sequencer": r"[Ss]equencer|RecoverDriver|LiveDriver|advisor", "Advisor": r"advisor|Advisor",
             "Measure": r"[Ss]equencer", "Risk": r"风控|hft-risk", "Oms": r"OMS|hft-oms", "Ledger": r"ledger|Ledger",
             "Exec": r"执行层|hft-exec", "Venue": r"\bgw\b|feed|hft-venue", "Boot": r"hft-core|启动步",
             "Housekeeping": r"housekeeping", "CtlAudit": r"gateway"}
note_src_n = 0
for p, r in spec_items():
    comp = "/".join(r.split("/")[:2])
    if comp not in CRATE_ACTOR:
        continue
    for i, line, in_code in prose_lines(texts[p]):
        found = []
        for m in RX_NOTE_PROD.finditer(line):
            pre = re.split(r"[。;;]", line[max(0, m.start() - 80):m.start()])[-1]
            others = [x for x in RX_OTHER_ACTOR.findall(pre) if not re.fullmatch(CRATE_ACTOR[comp], x)
                      and x != r.split("/")[1]]
            if others:
                continue
            reg_ = NOTE_SRC.get(m.group(3))
            if reg_ in SRC_ACTOR and re.search(SRC_ACTOR[reg_], line[:m.start()]):
                continue          # the line describes the registered producer's behaviour (e.g. "advisor: … 写 Note{…}")
            found.append((m.group(3), m.group(2)))
        nm = re.search(r"notes\s*:\s*PreVec<Note>[^/]*//(.*)$", line)
        if nm:
            found += [(x, None) for x in re.findall(r"[A-Z]\w+", nm.group(1)) if x in NOTE_SRC]
        for code, src in found:
            reg = NOTE_SRC.get(code)
            if not reg:
                continue
            note_src_n += 1
            ok = NOTE_SRC_CRATE.get(reg, set())
            if src and src in NOTE_SRC_CRATE and src != reg:
                add("NOTESRC", p, f"{r}:{i}: Note{{{src}, {code}}} but {code} is registered with source {reg}")
            elif ok and comp not in ok:
                add("NOTESRC", p, f"{r}:{i}: {comp} writes Note{{{code}}}, registered with source {reg} "
                                  f"(produced by {', '.join(sorted(ok))}; hft-event Note 码登记表)")

# ---------- ⑨ BENCHDUP / BENCHOWN / BENCHCOPY[W]: benchmark names (`prefix/name` in the last cell of a budget row)
RX_BENCH = re.compile(r"`([a-z][a-z0-9_-]*/[a-z0-9_]+)`")
RX_BUDGET = re.compile(r"(\d+(?:\.\d+)?(?:\s*/\s*\d+(?:\.\d+)?)*)\s*(ns|µs|us|ms)(?![A-Za-z])")
UNIT_NS = {"ns": 1, "µs": 1000, "us": 1000, "ms": 1000000}
bench_defs = defaultdict(list)     # name -> [(file, rr, line, budget tuple)]
for p, r in spec_items():
    for hl, hdr, rows in md_tables(p):
        for ln, cells in rows:
            if len(cells) < 3:
                continue
            names = RX_BENCH.findall(cells[-1])
            if not names:
                continue
            bud = tuple(float(x) * UNIT_NS[u] for c in cells[1:-1] for g, u in RX_BUDGET.findall(c)
                        for x in re.split(r"\s*/\s*", g))
            if not bud:
                continue
            for n in names:
                bench_defs[n].append((p, r, ln, bud))
prefix_files = defaultdict(lambda: defaultdict(int))
for n, defs in bench_defs.items():
    for p, r, ln, bud in defs:
        prefix_files[n.split("/")[0]][r] += 1
prefix_owner = {}
for pre, fc in prefix_files.items():
    best = sorted(fc.items(), key=lambda x: -x[1])
    if len(best) == 1 or best[0][1] > best[1][1]:
        prefix_owner[pre] = best[0][0]
for n, defs in sorted(bench_defs.items()):
    buds = {d[3] for d in defs}
    files = {d[1] for d in defs}
    owner = prefix_owner.get(n.split("/")[0])
    where = "; ".join(f"{d[1]}:{d[2]} {'/'.join(f'{x:g}' for x in d[3])} ns" for d in defs)
    if len(buds) > 1:
        add("BENCHDUP", defs[0][0], f"`{n}` has different budgets: {where}")
    elif len(files) > 1:
        add("BENCHCOPY", defs[0][0], f"`{n}` budget restated in {len(files)} files: {where}")
    if owner and owner not in files:
        add("BENCHOWN", defs[0][0], f"`{n}` defined only outside {owner} (owner of `{n.split('/')[0]}/`): {where}")

# ---------- ⑩ LINKTGT[W]: "规则归 / 唯一归属 / 唯一定义 [X](…#a)" — the target section must contain at least one of
# the identifiers written in backticks earlier in the same clause
RX_MDLINK = re.compile(r"\[([^\]]*)\]\(([^)\s#]*)#([^)\s]+)\)")
RX_OWNKW = re.compile(r"(?:规则归|唯一归属|唯一定义|只在)[^\[。;;]{0,6}$")
_sec_cache = {}


def section_text(tp, frag):
    key = (tp, frag.lower())
    if key in _sec_cache:
        return _sec_cache[key]
    s = texts.get(tp) or (read(tp) if os.path.exists(tp) else "")
    lines, seen, out, lvl = s.splitlines(), defaultdict(int), None, None
    in_code = False
    for k, line in enumerate(lines):
        if line.lstrip().startswith("```"):
            in_code = not in_code
            if out is not None:
                out.append(line)
            continue
        hm = None if in_code else re.match(r"^(#{1,6})\s+(.*?)\s*#*\s*$", line)
        if hm:
            b = slug(hm.group(2))
            nb = seen[b]
            seen[b] += 1
            sl = b if nb == 0 else f"{b}-{nb}"
            if out is not None and len(hm.group(1)) <= lvl:
                break
            if out is None and sl.lower() == frag.lower():
                out, lvl = [], len(hm.group(1))
                continue
        if out is not None:
            out.append(line)
    if out is None and f'id="{frag}"' in s:
        out = s.split(f'id="{frag}"', 1)[1].splitlines()[:80]
    _sec_cache[key] = "\n".join(out) if out is not None else None
    return _sec_cache[key]


link_tgt_n = 0
for p, r in spec_items():
    for i, line, in_code in prose_lines(texts[p]):
        if in_code:
            continue
        for m in RX_MDLINK.finditer(line):
            before = line[:m.start()]
            if not RX_OWNKW.search(before):
                continue
            clause = re.split(r"[。;;]|\]\([^)]*\)", before)[-1]
            ids = [re.split(r"[\s{(=<\[]", t)[0].strip(".,") for t in RX_BT.findall(clause)]
            ids = [x for x in ids if len(x) >= 3 and not x.startswith(("/", "#"))]
            if not ids:
                continue
            tp = os.path.normpath(os.path.join(os.path.dirname(p), m.group(2))) if m.group(2) else p
            sec = section_text(tp, m.group(3))
            if sec is None:
                continue
            link_tgt_n += 1
            if not any(x in sec for x in ids):
                add("LINKTGT", p, f"{r}:{i}: [{m.group(1)}](…#{m.group(3)}) contains none of "
                                  f"{', '.join('`' + x + '`' for x in ids[:4])}")

# ---------- ⑪ TBDFMT[W]: unverified items are written 待核实(方法:…) or point to a 开发备忘 entry;
# 待标定 / 待实测 are the defined variants (docs/AGENTS.md); any other wording, or a bare 待核实, is a warning
RX_TBD = re.compile(r"待核实|待确认|待选型|待研究|未核实(?!的)|待验证|待调研|待评估")
tbd_count = defaultdict(int)
for p, r in spec_items():
    bad_tbd = []
    for i, line, in_code in prose_lines(texts[p]):
        for w in ("待标定", "待实测"):
            tbd_count[w] += line.count(w)
        for m in RX_TBD.finditer(line):
            w = m.group(0)
            tbd_count[w] += 1
            after = line[m.end():]
            if w == "待核实" and (re.search(r"方法", after) or "开发备忘" in line or re.search(r"#开发备忘", line)):
                continue
            bad_tbd.append((i, w))
    if bad_tbd:
        kinds = defaultdict(int)
        for _, w in bad_tbd:
            kinds["bare 待核实" if w == "待核实" else w] += 1
        add("TBDFMT", p, f"{r}: {len(bad_tbd)} ({', '.join(f'{k}×{v}' for k, v in sorted(kinds.items()))};"
                         f" lines {fmt_lines(sorted({i for i, _ in bad_tbd}))})")

# ---------- ⑫ BRIEFNAME: every name of the brief's naming table appears in (one of) its declared owner files
BRIEF_OWNER = {"SDK": "crates/hft-strategy-sdk/README.md", "pc": "strategies/portfolio-carry/README.md",
               "latency-budget": "docs/latency-budget.md", "development": "docs/development.md",
               "testing": "docs/testing.md", "control-plane": "docs/subsystems/control-plane.md",
               "glossary": "docs/glossary.md", "deploy": "deploy/README.md", "tools": "tools/README.md",
               "journal-format": "docs/subsystems/journal-format.md", "event-model": "docs/subsystems/event-model.md",
               "hot-path-rules": "docs/hot-path-rules.md", "launch": "docs/runbooks/launch.md",
               "benchmarks": "benchmarks/README.md", "host-upgrade": "docs/runbooks/host-upgrade.md",
               "incident-response": "docs/runbooks/incident-response.md",
               "defensive-patterns": "docs/defensive-patterns.md"}
for _d in ("crates", "apps", "strategies", "python"):
    _dp = os.path.join(ROOT, _d)
    if os.path.isdir(_dp):
        for _n in os.listdir(_dp):
            if os.path.exists(os.path.join(_dp, _n, "README.md")):
                BRIEF_OWNER.setdefault(_n, f"{_d}/{_n}/README.md")


def brief_owner_files(cell):
    files = []
    for t in re.findall(r"architecture-v2/\d+|[A-Za-z][\w-]*", cell):
        if t.startswith("architecture-v2/"):
            files += [os.path.join(DESIGN, f) for f in sorted(os.listdir(DESIGN))
                      if f.startswith(t.split("/")[1].zfill(2) + "-") and f.endswith(".md")] if os.path.isdir(DESIGN) else []
        elif t in BRIEF_OWNER:
            files.append(hft(BRIEF_OWNER[t]))
    return list(dict.fromkeys(files))


def brief_name_rx(t):
    """Candidate regexes for a naming-table entry: the literal first, then its distinctive part."""
    t = t.replace("\\|", "|").strip()
    cands = [re.escape(t)]
    m = re.fullmatch(r"Note\{\s*(\w+)\s*\}", t)
    if m:
        return [re.escape(m.group(1))]
    m = re.fullmatch(r"[\w:.]+\{\s*\w+\s*=\s*(\w+)\s*\}", t)          # L2Snapshot{origin = TopN}
    if m:
        cands.append(r"\b" + m.group(1) + r"\b")
    m = re.fullmatch(r"([\w:.]+)\s*[{(].*[})]", t)                     # SmokeOrder{…}, event_weighted(), F(a, b)
    if m:
        cands.append(re.escape(m.group(1)) + r"\b")
        t = m.group(1)
    m = re.fullmatch(r"([A-Z]\w*)(?:::|\.)(\w+)", t)                   # Type::Variant, Type.field
    if m:
        cands.append(re.escape(m.group(1)) + r"[^\n]{0,200}\b" + re.escape(m.group(2)) + r"\b")
    m = re.fullmatch(r"([\w.]+)\s*=\s*(.+)", t)                          # verify = true|false
    if m:
        cands.append(re.escape(m.group(1)) + r"\s*=\s*" + re.escape(m.group(2).split("|")[0].strip()))
    if " " in t and not re.search(r"[=<{(]", t):                        # replayer latency, hft-maint start
        w = t.split()
        cands.append(re.escape(w[0]) + r"[^\n]{0,80}" + re.escape(w[-1]))
    if "<" in t:
        cands.append(re.escape(t.split("<")[0]))
    return cands


brief_names_n = 0
if os.path.exists(BRIEF):
    _bs = read(BRIEF)
    _bm = re.search(r"\n## [^\n]*命名(?:总)?表[^\n]*\n(.*?)(?=\n## |\Z)", _bs, re.S)
    if not _bm:
        problems["BRIEFNAME"].append(f"{BRIEF}: no '## … 命名总表' section")
    else:
        _tmp = os.path.join(os.path.dirname(BRIEF), "\x00brief-section")
        texts[_tmp] = _bm.group(1)
        for hl, hdr, rows in md_tables(_tmp):
            for ln, cells in rows:
                if len(cells) < 3:
                    continue
                files = brief_owner_files(cells[2])
                toks = RX_BT.findall(cells[0]) or [x.strip() for x in re.split(r"[、,]", re.sub(r"[((][^))]*[))]", "", cells[0]))
                                                  if x.strip()]
                for t in toks:
                    brief_names_n += 1
                    if re.fullmatch(r"(?:\.agents|docs|crates|apps|python|strategies|deploy|tools|benchmarks|fixtures)"
                                    r"/[\w./-]+\.md", t):          # a repo file named by the brief: it must exist
                        if not os.path.exists(hft(t)):
                            problems["BRIEFNAME"].append(f"brief §7 `{t}`: file does not exist in the hft tree")
                        continue
                    if not files:
                        problems["BRIEFNAME"].append(f"brief §7 `{t}`: owner '{cells[2]}' maps to no file")
                        continue
                    body = "\n".join(texts.get(f) or (read(f) if os.path.exists(f) else "") for f in files)
                    if not any(re.search(c, body) for c in brief_name_rx(t)):
                        problems["BRIEFNAME"].append(
                            f"brief §7 `{t}` not found in its owner file(s): "
                            f"{', '.join(rroot(f) if in_spec(f) else rel(f) for f in files)}")
        del texts[_tmp]

# =====================================================================================================
# round 8f checks (M0-f brief §4 = X3 §4 items 1–12); hft spec files (spec_file); frozen components tallied
# =====================================================================================================
RX_ANYPATH = re.compile(r"(?<![\w./<>@-])((?:/(?:dev|run|var/lib|opt|etc|usr/local/sbin)|<cold_root>|<journal_cold>)"
                        r"(?:/(?:\{[^}\s`]*\}|[^\s/`'\"()(),,;;。、|\[\]{}])+)*/?)")
RX_OG = re.compile(r"`?((?:[a-z][\w-]*|<[^<>`:\s]+>)):([a-z][\w-]*)`?")
RX_MODE = re.compile(r"(?<![\d.])([0-7]?[0-7]{3})(?![\d.])")
INSTANCES = ("prod", "sim")


def unit_norm(t):
    t = t.strip().strip("`").strip()
    t = re.sub(r"\.(?:service|timer|mount|target)$", "", t)
    return t


def brace_expand(t):
    m = re.search(r"\{([^{}]*)\}", t)
    if not m or "," not in m.group(1):
        return [t]
    return [x for alt in m.group(1).split(",") for x in brace_expand(t[:m.start()] + alt.strip() + t[m.end():])]


def ptuple(t):
    """(components, is_dir) of a path string; `%i` → `<instance>`, `<journal_cold>` → `<cold_root>`."""
    t = t.strip().rstrip("。.,,;;:")
    is_dir = t.endswith("/")
    comps = [c for c in t.split("/") if c]
    while comps and comps[-1] in ("…", "...", "*", "**"):
        comps.pop()
        is_dir = True
    comps = [c.replace("%i", "<instance>").replace("<journal_cold>", "<cold_root>").rstrip("…") for c in comps]
    comps = [c for c in comps if c]
    if comps and "." not in comps[-1] and not comps[-1].endswith(("sock", "json", "jsonl", "toml")):
        is_dir = True if not t.endswith(("until", "active", "tripped", "credentials", "heartbeat")) else is_dir
    return tuple(comps), is_dir


def pstr(comps):
    return ("" if comps and comps[0].startswith("<") else "/") + "/".join(comps)


def split_depth0(s, seps="。;;"):
    out, depth, cur = [], 0, []
    for ch in s:
        if ch in "((":
            depth += 1
        elif ch in "))":
            depth = max(0, depth - 1)
        if ch in seps and depth == 0:
            out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    out.append("".join(cur))
    return out


def inst_variants(comps):
    """[(instance or None, comps)] with `<instance>` replaced by each instance."""
    if not any("<instance>" in c for c in comps):
        return [(None, comps)]
    return [(i, tuple(c.replace("<instance>", i) for c in comps)) for i in INSTANCES]


# ---------- deploy model: unit table, user table, path table (home: deploy/README.md)
DEP_UNITS = {}     # unit -> {user, main, dev2, line, supp(set|None), runtime[], state[], rwpaths[], optional}
DEP_USERS = {}     # user -> {unit, groups(set), line}
DEP_GROUPS = set()
_dep_s = texts.get(DEPLOY_MD, "")


def _dep_section(anchor):
    return section_text(DEPLOY_MD, anchor) or ""


for hl, hdr, rows in md_tables(DEPLOY_MD):
    if hdr and hdr[0] == "单元" and "用户" in hdr and any("关键设置" in c for c in hdr):
        ucol, mcol = hdr.index("用户"), next(k for k, c in enumerate(hdr) if "关键设置" in c)
        dcol = next((k for k, c in enumerate(hdr) if "dev2" in c), None)
        for ln, cells in rows:
            if len(cells) <= mcol:
                continue
            names = [unit_norm(t) for t in RX_BT.findall(cells[0]) if re.match(r"(?:hft-|dev-shm|bsc-)", t)]
            if not names:
                continue
            ucell, main = cells[ucol], cells[mcol]
            dev2 = cells[dcol] if dcol is not None and dcol < len(cells) else ""
            # follow "见[section](#anchor)" in the main cell (the row delegates its settings to that section)
            ext = " ".join(_dep_section(a) for a in re.findall(r"见\s*\[[^\]]*\]\(#([^)\s]+)\)", main))
            opt = bool(re.search(r"可选|optional", cells[0]))
            pairs = re.findall(r"`%i\s*=\s*(\w+)`\s*[::]\s*`([\w-]+)`", ucell)
            if pairs and names and names[0].endswith("@"):          # template unit: one entry per instance
                parts = re.split(r"\*\*(\w+) drop-in\*\*", main)
                base, drop = parts[0], dict(zip(parts[1::2], parts[2::2]))
                for inst, user in pairs:
                    DEP_UNITS[names[0] + inst] = {"user": user, "main": (base + " " + drop.get(inst, "")).replace("%i", inst),
                                                  "dev2": dev2, "line": ln, "ext": ext, "optional": opt}
                continue
            users = [t for t in RX_BT.findall(ucell) if re.fullmatch(TOK, t)]
            if not users and re.search(r"\broot\b", ucell):
                users = ["root"]
            labels = {}
            if len(names) > 1:
                for lm in re.finditer(r"(?:^|[;;])\s*([a-z_-]+)\s*[::]", main):
                    if any(lm.group(1).replace("_", "-") in x for x in names):
                        labels[lm.start()] = lm.group(1).replace("_", "-")
            for k, n in enumerate(names):
                u = users[k] if len(users) == len(names) else (users[0] if users else None)
                m_main = main
                if labels:                                           # keep the shared text and this unit's segments
                    cuts = sorted(labels) + [len(main)]
                    m_main = main[:cuts[0]] + "".join(main[a:b] for a, b in zip(cuts, cuts[1:])
                                                      if labels[a] in n)
                DEP_UNITS[n] = {"user": u, "main": m_main, "dev2": dev2, "line": ln, "ext": ext, "optional": opt}
    if hdr and hdr[0] == "用户" and any("附加组" in c for c in hdr):
        gcol = next(k for k, c in enumerate(hdr) if "附加组" in c)
        for ln, cells in rows:
            us = [t for t in RX_BT.findall(cells[0]) if re.fullmatch(TOK, t)] or (["root"] if "root" in cells[0] else [])
            if not us or len(cells) <= gcol:
                continue
            gs = {t for t in RX_BT.findall(cells[gcol]) if re.fullmatch(TOK, t)}
            DEP_USERS[us[0]] = {"unit": cells[1] if len(cells) > 1 else "", "groups": gs, "line": ln,
                                "optional": bool(re.search(r"可选|optional", " ".join(cells)))}
for _line in _dep_s.splitlines():
    _m = re.match(r"^组[::](.*)$", _line)
    if _m:
        DEP_GROUPS.update(t for t in RX_BT.findall(_m.group(1)) if re.fullmatch(TOK, t))
DEP_GROUPS |= set(DEP_USERS)
for _u in DEP_USERS.values():
    DEP_GROUPS |= _u["groups"]

for n, info in DEP_UNITS.items():
    txt = info["main"]
    info["runtime"] = [ptuple("/run/hft/" + x)[0] for x in re.findall(r"RuntimeDirectory=hft/([\w%<>@-]+)", txt)]
    info["state"] = [ptuple("/var/lib/hft/" + x)[0] for x in re.findall(r"StateDirectory=hft/([\w%<>@-]+)", txt)]
    info["rwpaths"] = [ptuple(x.lstrip("-"))[0] for x in re.findall(r"ReadWritePaths=([^\s`;;,,()()]+)", txt)]
    sms = re.findall(r"SupplementaryGroups=([A-Za-z0-9 _-]+)", txt)
    if sms:
        sup = {t for sm in sms for t in sm.split() if re.fullmatch(TOK, t)}
        for word, body in re.findall(r"[((]\s*([a-z_-]+)\s*另加\s*([^))]*)[))]", txt):   # "(pipeline 另加 `hft-ctl`)"
            if word.replace("_", "-") in n:
                sup |= {g for t in RX_BT.findall(body) for g in t.split() if re.fullmatch(TOK, g)}
        info["supp"] = sup
    else:
        info["supp"] = None
    info["protect_off"] = bool(re.search(r"ProtectSystem=(?!strict)", txt))

INST_USER = {i: DEP_UNITS.get("hft-core@" + i, {}).get("user") for i in INSTANCES}
if DEPLOY_MD in texts:      # the round-8f checks are vacuous without these tables: say so instead of passing silently
    if not DEP_UNITS:
        problems["PERM"].append("deploy/README.md: systemd unit table (| 单元 | … | 用户 | 关键设置 | …) not found")
    if not DEP_USERS:
        problems["PERM"].append("deploy/README.md: user table (| 用户 | 单元 | …附加组 |) not found")


def dir_unit(d):
    """The unit that owns directory name d under /run/hft or /var/lib/hft."""
    for u, info in DEP_UNITS.items():
        for c in info.get("runtime", []) + info.get("state", []):
            if c and c[-1] == d:
                return u
    d2 = d.replace("_", "-")
    for u in DEP_UNITS:
        if u in ("hft-" + d2, d2) or u.endswith("-" + d2):
            return u
    return None


def unit_user(u):
    info = DEP_UNITS.get(u)
    return info["user"] if info else None


def groups_of(u):
    return {u} | (DEP_USERS[u]["groups"] if u in DEP_USERS else set())


PATHS = []   # {comps, is_dir, owner, group, mode, creator, line, optional, acl}


def _parse_owner(cell):
    acl, aclm = [], re.search(r"ACL(.*)$", cell)
    if aclm:
        acl = [(k, nm, bits) for k, nm, bits in re.findall(r"\b([ug]):([\w-]+):([rwx-]{1,3})", aclm.group(1))]
        if not acl:
            acl = "unknown"
        cell = cell[:aclm.start()]
    og = RX_OG.search(cell)
    mm = RX_MODE.search(cell)
    owner = group = None
    if og:
        owner, group = og.group(1), og.group(2)
    elif re.search(r"各单元用户|该用户", cell.split("(")[0].split("(")[0]):
        owner = "<unit>"
    else:
        bt = [t for t in RX_BT.findall(cell) if re.fullmatch(TOK + r"|<[^>]+>", t)]
        if bt:
            owner = bt[0]
        elif re.search(r"\broot\b", cell):
            owner = "root"
    return owner, group, (mm.group(1) if mm else None), acl


_only = None


def _add_path(comps, is_dir, owner, group, mode, creator, ln, opt, acl, inst=None):
    if owner == "<实例用户>" and inst:
        owner = INST_USER.get(inst)
    if owner == "<unit>":
        u = dir_unit(comps[-1]) if comps else None
        owner = unit_user(u) if u else None
    if owner and group is None:
        group = owner if owner != "root" else "root"
    if mode is None and creator in ("systemd", "root") and is_dir:
        mode = "0755"
    if mode and not int(mode[-3]) & 1:
        is_dir = False
    PATHS.append({"comps": comps, "is_dir": is_dir, "owner": owner, "group": group, "mode": mode,
                  "creator": creator, "line": ln, "optional": opt, "acl": acl or [], "only": _only})


for hl, hdr, rows in md_tables(DEPLOY_MD):
    if not (hdr and hdr[0] == "路径" and len(hdr) >= 3 and "属主" in hdr[1]):
        continue
    ccol = next((k for k, c in enumerate(hdr) if "建立" in c), None)
    ucol = next((k for k, c in enumerate(hdr) if "用途" in c), None)
    for ln, cells in rows:
        if "符号链接" in cells[1]:
            continue
        owner, group, mode, acl = _parse_owner(cells[1])
        gover = dict(re.findall(r"`([\w-]+)`\s*组为\s*`([\w-]+)`", cells[1]))
        ccell = cells[ccol] if ccol is not None and ccol < len(cells) else ""
        creator = ("root" if re.search(r"ExecStartPre=\+|挂载|mount|releasectl|人|\broot\b|cred-broker|组激活|hft-window"
                                       r"|hft-maint|hostprobe|tune", ccell)
                   else "systemd" if re.search(r"RuntimeDirectory|StateDirectory", ccell) else None)
        opt = bool(re.search(r"可选|optional", " ".join(cells)))
        _uc = cells[ucol] if ucol is not None and ucol < len(cells) else ""
        _only = next((i_ for i_ in INSTANCES if re.match(r"\s*`?" + i_ + r"`?\s*的", _uc)), None)
        bases = []
        for t in RX_BT.findall(cells[0]):
            if not (t.startswith("/") or t.startswith("<cold_root>")):
                continue
            for x in brace_expand(t):
                comps, is_dir = ptuple(x)
                for inst, cv in inst_variants(comps):
                    g = gover.get(cv[-1], group) if cv else group
                    _add_path(cv, is_dir, owner, g, mode, creator, ln, opt, acl, inst)
                    bases.append((inst, cv))
        ucell = cells[ucol] if ucol is not None and ucol < len(cells) else ""
        for m in re.finditer(r"`([\w.<>-]+/?)`\s*(?:标志)?\s*[((]([^()()]*)[))]", ucell):
            name, inner = m.group(1), m.group(2)
            fo, fg, fm, _ = _parse_owner(inner)
            if not (fo and fg) and not fm:
                continue
            fcre = "root" if re.search(r"ExecStartPre=\+`?\s*建立\s*`" + re.escape(name.rstrip("/")) + "`", _dep_s) else "proc"
            for inst, bc in bases:
                parent = next((e for e in PATHS if e["comps"] == bc), None)
                o = fo if fo and fg else (parent["owner"] if parent else None)
                _add_path(bc + (name.rstrip("/"),), name.endswith("/"), o, fg if fo and fg else None, fm, fcre, ln,
                          opt, [], inst)
        _only = None


if DEPLOY_MD in texts and not PATHS:
    problems["PERM"].append("deploy/README.md: path table (| 路径 | 属主:组 / 模式 | 建立者 | 用途 |) not found")


def find_entry(comps):
    best = None
    for e in PATHS:
        ec = e["comps"]
        if len(ec) == len(comps) and all(comp_match(a, b) for a, b in zip(ec, comps)):
            score = sum(1 for c in ec if "<" not in c and "{" not in c)
            if best is None or score > best[0]:
                best = (score, e)
    return best[1] if best else None


def can(u, e, need):
    if u == "root" or e["mode"] is None or e["acl"] == "unknown":
        return True
    m = e["mode"][-3:]
    gs = groups_of(u)
    if u == e["owner"]:
        d = int(m[0])
    else:
        named = [b for k, nm, b in e["acl"] if (k == "u" and nm == u)]
        grp = [b for k, nm, b in e["acl"] if k == "g" and nm in gs]
        if named:
            d = sum({"r": 4, "w": 2, "x": 1}.get(ch, 0) for ch in set("".join(named)))
        elif e["group"] in gs or grp:
            d = (int(m[1]) if e["group"] in gs else 0) | sum({"r": 4, "w": 2, "x": 1}.get(ch, 0)
                                                              for ch in set("".join(grp)))
        else:
            d = int(m[2])
    return d & need == need


def access_fail(u, comps, op):
    """None when user u can reach comps for op ('r'/'w'); (entry, need) for the first blocking entry; 'unreg' when
    no component of the path is registered in deploy."""
    chain = [(k, find_entry(comps[:k])) for k in range(1, len(comps) + 1)]
    chain = [(k, e) for k, e in chain if e]
    if not chain:
        return "unreg"
    for k, e in chain:
        last = k == len(comps)
        parent_of_file = (k == len(comps) - 1 and "." in comps[-1] and not find_entry(comps))
        if op == "t":
            need = 1 if e["is_dir"] else 0
        elif last and not e["is_dir"]:
            need = 4 if op == "r" else 2
        elif (last or parent_of_file) and op == "w":
            need = 3
        else:
            need = 1
        if not can(u, e, need):
            return e, need
    return None


def need_word(n):
    return {1: "traverse (x)", 3: "write (wx)", 4: "read (r)", 2: "write (w)"}.get(n, str(n))


def entry_desc(e):
    return f"{pstr(e['comps'])} = {e['owner']}:{e['group']} {e['mode']} (deploy/README.md:{e['line']})"


# ---------- path aliases (热段 / 冷副本 / 心跳块) from the deploy path table's 用途 column and journal-format
ALIAS = defaultdict(set)
for hl, hdr, rows in md_tables(DEPLOY_MD):
    if hdr and hdr[0] == "路径":
        ucol = next((k for k, c in enumerate(hdr) if "用途" in c), None)
        for ln, cells in rows:
            use = cells[ucol] if ucol is not None and ucol < len(cells) else ""
            for word in ("热段", "冷副本", "心跳块"):
                if word in use:
                    for t in RX_BT.findall(cells[0]):
                        if t.startswith("/") or t.startswith("<cold_root>"):
                            for x in brace_expand(t):
                                ALIAS[word].add(ptuple(x)[0])
for _line in texts.get(hft("docs/subsystems/journal-format.md"), "").splitlines():
    if "热段" in _line:
        for m in RX_ANYPATH.finditer(_line):
            c = ptuple(m.group(1))[0]
            if "<instance>" in c:
                ALIAS["热段"].add(c[:c.index("<instance>") + 1])

# ---------- config keys whose default is a path (per file; `a.b` / `c.d` multi-key rows paired by index)
CFG_PATHDEF = defaultdict(dict)    # rr -> key -> (comps, is_dir, desc, line)
for p, r in spec_items():
    for hl, hdr, rows in md_tables(p):
        dcol = next((k for k, c in enumerate(hdr) if "默认" in c), None)
        if dcol is None or dcol == 0:
            continue
        for ln, cells in rows:
            if len(cells) <= dcol:
                continue
            keys = cfg_keys(cells[0])
            dv = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", cells[dcol])
            if not keys:
                continue
            parts = re.split(r"\s+/\s+", dv) if len(keys) > 1 else [dv]
            if len(parts) != len(keys):
                parts = [dv] if len(keys) == 1 else []
            desc = " ".join(cells[1:dcol]) if len(keys) == 1 else ""
            for k, part in zip(keys, parts):
                pm = RX_ANYPATH.search(part.replace("`", " "))
                if not pm:
                    continue
                comps, is_dir = ptuple(pm.group(1))
                CFG_PATHDEF[r][k] = (comps, is_dir, desc, ln)


def cfg_path(key, r):
    if key in CFG_PATHDEF.get(r, {}):
        return CFG_PATHDEF[r][key]
    hits = {v[0] for rr_, d in CFG_PATHDEF.items() for k, v in d.items() if k == key}
    if len(hits) == 1:
        h = next(iter(hits))
        return next(v for rr_, d in CFG_PATHDEF.items() for k, v in d.items() if k == key and v[0] == h)
    return None


def comp_units(r):
    """Units of the component that owns spec file r (apps/<x>, python/<x>)."""
    m = re.match(r"(?:apps|python)/([^/]+)/", r)
    if not m:
        return []
    x = m.group(1)
    if x == "hft-core":
        return [u for u in DEP_UNITS if u.startswith("hft-core@")]
    if x == "replayer":
        return [u for u in DEP_UNITS if u == "hft-replay-verify"]
    u = dir_unit(x)
    return [u] if u else []


ACTOR_WORDS = {"watchtower": "hft-watchtower", "archiver": "hft-journal-archiver",
               "journal-archiver": "hft-journal-archiver", "ops_bot": "hft-ops-bot", "ops-bot": "hft-ops-bot",
               "pipeline": "hft-pipeline", "treasury": "hft-treasury", "reconciler": "hft-reconciler",
               "kill-switch": "hft-kill-switch", "replayer": "hft-replay-verify"}


def resolve_user(tok):
    t = unit_norm(tok)
    if t in DEP_USERS:
        return t
    if t in DEP_UNITS:
        return DEP_UNITS[t]["user"]
    w = ACTOR_WORDS.get(t)
    if w and w in DEP_UNITS:
        return DEP_UNITS[w]["user"]
    return None


def group_members(g):
    return {u for u, info in DEP_USERS.items() if g in info["groups"] or u == g}

# ---------- ① PERM / PERMRW / PERMOPT[W] / PERMIMP[W]: permission closure
# Claims (who needs which access to which path) come from: deploy unit rows (ReadWritePaths=), config defaults
# in component READMEs (a key whose default is a path: own path → write, another unit's path → read, socket →
# write), prose with an explicit actor ("`hft-replay-verify` 从这里读", "`G` 组可读", "对该组只读") and — warning
# level — prose in a component README with no other actor ("只读映射段头", "删除本地冷副本"). Each claim is
# evaluated against the deploy path table (owner:group / mode, ACL entries g:G:rx) and the user table (primary
# group = user name + 附加组): traverse (x) on every registered ancestor, then r / w on the target.
# PERM also checks: a file or socket `o:g` created by process o (not root, not inherited from a setgid parent
# with group g) needs o ∈ g; a README's stated owner/group/mode of a registered path equals the path table.
# PERMRW: ProtectSystem=strict — a write outside /dev needs the unit's own RuntimeDirectory/StateDirectory or a
# ReadWritePaths= entry.
RX_RV = r"(?:从这里读|从中读|可读|只读映射|只读|读取|读|映射|尾随)"
RX_WV = r"(?:追加写|写入|写到|写|删除|改名|rename|建立|创建)"
RX_VERB = re.compile(RX_RV + "|" + RX_WV)
RX_OTHER_PROC = re.compile(r"hft-core|sequencer|persist|\bgw\b|feed|watchtower|archiver|reconciler|kill-switch|ops_bot|"
                           r"ops-bot|pipeline|treasury|replayer|hft-verify|守护|护栏|看门狗|运维|负责人|\b人\b|systemd"
                           r"|ExecStartPre|releasectl|hft-secrets|hft-maint|hft-window|hft-shm-release|cred-broker|凭证代理|hostprobe"
                           r"|研究|hft-py|用例|测试|策略|deploy|挂载单元|生成器|tmpfs 挂载")
claims = []        # {user, unit, comps, is_dir, op, src, kind, optional, p}


def add_claim(user, unit, comps, is_dir, op, src, kind, p, optional=False):
    optional = bool(optional or DEP_USERS.get(user, {}).get("optional"))
    if user and comps:
        claims.append({"user": user, "unit": unit, "comps": comps, "is_dir": is_dir, "op": op, "src": src,
                       "kind": kind, "optional": optional, "p": p})


# deploy rows: ReadWritePaths= entries are write claims of the unit's user
for n, info in DEP_UNITS.items():
    for rw in info["rwpaths"]:
        add_claim(info["user"], n, rw, "." not in rw[-1], "t", f"deploy/README.md:{info['line']}", "rwpaths",
                  DEPLOY_MD, info["optional"])

# path-table directories owned by a unit's user are written by that unit (instance directories by that instance)
for e in PATHS:
    o = e["owner"]
    if not e["is_dir"] or not o or o == "root" or o.startswith("<"):
        continue
    for u, info in DEP_UNITS.items():
        if info["user"] != o:
            continue
        if u.startswith("hft-core@") and any(i_ in "/".join(e["comps"]) for i_ in INSTANCES) \
                and u[len("hft-core@"):] not in e["comps"] and not any(c.endswith("-" + u[len("hft-core@"):])
                                                                       for c in e["comps"]):
            continue
        add_claim(o, u, e["comps"], True, "w", f"deploy/README.md:{e['line']} (owner `{o}`)", "owner", DEPLOY_MD,
                  e["optional"])

# config defaults of component READMEs
for r, d in CFG_PATHDEF.items():
    us = comp_units(r)
    if not us:
        continue
    for key, (comps, is_dir, desc, ln) in d.items():
        for u in us:
            user = unit_user(u)
            for inst, cv in inst_variants(comps):
                if inst and u.startswith("hft-core@") and not u.endswith(inst):
                    continue
                own = False
                info = DEP_UNITS[u]
                for oc in info["runtime"] + info["state"]:
                    if cv[:len(oc)] == tuple(oc) or all(comp_match(a, b) for a, b in zip(oc, cv)) and len(cv) >= len(oc):
                        own = True
                e = find_entry(cv)
                if e and e["owner"] == user:
                    own = True
                is_sock = bool(cv and cv[-1].endswith(".sock"))
                if re.search(r"sock|套接字|socket", key + " " + desc) or is_sock:
                    op = "w"
                elif own and not re.search(r"只读|读取", desc):
                    op = "w"
                else:
                    op = "r"
                add_claim(user, u, cv, is_dir, op, f"{r}:{ln} `{key}`", "config", hft(r))
                if is_sock and claims:
                    claims[-1]["sock"] = True


def line_path_refs(line, r):
    """[(pos, comps, is_dir)] literal paths and `config.key[/…]` with a known path default."""
    refs = []
    for m in RX_ANYPATH.finditer(line):
        c, d = ptuple(m.group(1))
        if c:
            refs.append((m.start(), c, d))
    for m in re.finditer(r"`([a-z_]+(?:\.[a-z_]+)+)((?:/[^`]*)?)`", line):
        cp = cfg_path(m.group(1), r)
        if cp:
            extra = ptuple("/x" + m.group(2))[0][1:] if m.group(2) else ()
            refs.append((m.start(), tuple(cp[0]) + tuple(extra), cp[1] if not extra else ptuple(m.group(2))[1]))
    return refs


def alias_refs(text):
    out = []
    for word, paths in ALIAS.items():
        if word in text:
            out += [(text.find(word), c, True) for c in sorted(paths)]
    return out


RX_ACTOR = re.compile(r"(?:`(hft[\w@.-]*)`|(?<![\w`/-])(" + "|".join(re.escape(w) for w in ACTOR_WORDS) + r")(?![\w`-]))"
                      r"\s*(组)?\s*(?:的运行用户)?\s*(?:也|再|会|可以|只|才|另)?\s*(?:从这里|从中|从其中)?\s*"
                      r"(" + RX_RV + "|" + RX_WV + r")")
RX_GROUPCLAIM = re.compile(r"(?:对\s*`(hft[\w-]*)`\s*组\s*(只读|可读|可穿越)|`(hft[\w-]*)`\s*组\s*(可读|只读|可穿越)"
                           r"|对该组\s*(只读|可读))")
RX_SUPPREAD = re.compile(r"([A-Za-z_-]+)\s*以附加组\s*`(hft[\w-]*)`\s*(" + RX_RV + ")")
RX_SKIPSEC = re.compile(r"测试与验收|延迟与资源契约|开发备忘|规划源文件|已知限制")
for p, r in spec_items():
    own_units = comp_units(r)
    for i, line, in_code, heads in walk_lines(texts[p]):
        if in_code or line.lstrip().startswith(("```", "~~~")):
            continue
        opt = bool(re.search(r"可选|optional|链上组件", line))
        refs = line_path_refs(line, r)
        # group claims: "对 `G` 组只读" / "`G` 组可读" / "对该组只读"
        for m in RX_GROUPCLAIM.finditer(line):
            g = m.group(1) or m.group(3)
            if not g:
                prev_g = [t for t in RX_BT.findall(line[:m.start()]) if t in DEP_GROUPS]
                g = prev_g[-1] if prev_g else None
            if not g or g not in DEP_GROUPS:
                continue
            sent = re.split(r"[。;;]", line[:m.start()])[-1]
            pre = [x for x in refs if x[0] < m.start()]
            tg = [pre[-1]] if pre else alias_refs(sent)
            for _, c, d in tg:
                for u in sorted(group_members(g)):
                    add_claim(u, None, c, d, "r", f"{r}:{i} (`{g}` 组读)", "group", p, opt)
        # explicit actor claims
        for m in RX_ACTOR.finditer(line):
            tok, isgrp, verb = m.group(1) or m.group(2), m.group(3), m.group(4)
            if isgrp:
                continue              # "`G` 组 …" is a group claim (above)
            pre_txt = line[max(0, m.start() - 6):m.start()]
            if "以附加组" in pre_txt or re.search(r"[A-Za-z_]$", pre_txt.strip()) and "附加组" in pre_txt:
                continue
            user = resolve_user(tok)
            if not user or re.match(r"\s*(?:不到|不了|不能|不可)", line[m.end():m.end() + 4]) \
                    or re.search(r"不能|无法|不可|不得|拒绝|禁止", line[m.start():m.end()]):
                continue
            op = "w" if re.fullmatch(RX_WV, verb) else "r"
            cstart = max(line.rfind(c_, 0, m.start()) for c_ in "。;;") + 1
            depth_ok = [x for x in refs if x[0] < m.start() and len(split_depth0(line[x[0]:m.start()])) == 1]
            pre = [x for x in refs if x[0] < m.start() and x in depth_ok]
            post = [x for x in refs if m.end() <= x[0] <= m.end() + 3]
            tg = [post[0]] if post else ([pre[-1]] if pre and m.group(1) else [])
            unit = unit_norm(tok) if unit_norm(tok) in DEP_UNITS else None
            for _, c, d in tg:
                add_claim(user, unit, c, d, op, f"{r}:{i} (`{tok}` {verb})", "prose", p, opt)
        # "X 以附加组 `G` 读取" — X reads the nearest preceding path
        for m in RX_SUPPREAD.finditer(line):
            user = resolve_user(m.group(1))
            pre = [x for x in refs if x[0] < m.start()]
            if user and pre:
                add_claim(user, None, pre[-1][1], pre[-1][2], "r", f"{r}:{i} ({m.group(1)} 以附加组 `{m.group(2)}` 读)",
                          "prose", p, opt)
        # implicit actor: a clause of a component README naming a path or alias with a verb and no other actor
        own_w = r.split("/")[1].replace("_", "-") if own_units else ""
        if own_units and not r.endswith("AGENTS.md") and not any(RX_SKIPSEC.search(h) for h in heads) \
                and not any(x.replace("_", "-") not in own_w and not own_w.endswith(x.replace("_", "-"))
                            for h in heads[1:] for x in RX_OTHER_PROC.findall(h)):
            pos = 0
            for clause in re.split(r"[。;;|]", line):
                start, pos = pos, pos + len(clause) + 1
                vm = RX_VERB.search(re.sub(r"\]\([^)]*\)", "]", clause))
                if not vm or RX_ACTOR.search(clause) or RX_GROUPCLAIM.search(clause) or re.search(r"禁止|不得", clause):
                    continue
                own_words = "|".join(re.escape(x) for x in (r.split("/")[1], r.split("/")[1].replace("-", "_"),
                                                            "archiver" if "archiver" in r else "§§"))
                others = [x for x in RX_OTHER_PROC.findall(re.sub(r"\]\([^)]*\)", "]", clause))
                          if not re.fullmatch(own_words, x)]
                if others:
                    continue
                crefs = [(x[0] - start, x[1], x[2]) for x in refs if start <= x[0] < pos] or alias_refs(clause)
                if not crefs:
                    continue
                for u in own_units:
                    for cpos, c, d in crefs:
                        near_w = any((0 <= cpos - wm.end() <= 8 or 0 <= wm.start() - cpos - 3 <= 2)
                                     and not re.match(r"入?者", clause[wm.end():wm.end() + 2])
                                     and not re.search(r"(?:不|没有|无需|不再)\s*$", clause[max(0, wm.start() - 3):wm.start()])
                                     for wm in re.finditer(RX_WV, clause))
                        near_r = any(abs(rm.start() - cpos) <= 40 for rm in re.finditer(RX_RV, clause))
                        strong_w = any(0 <= cpos - wm.end() <= 2 and not re.search(
                            r"(?:不|没有|无需|不再|由)\s*$", clause[max(0, wm.start() - 3):wm.start()])
                            for wm in re.finditer(RX_WV, clause))
                        strong_r = any(0 <= cpos - rm.end() <= 2 for rm in re.finditer(RX_RV, clause))
                        if near_w and not re.search(r"只读", clause):
                            op, strong = "w", strong_w
                        elif near_r:
                            op, strong = "r", strong_r
                        else:
                            continue
                        # a verb right before the path/alias ("删除本地冷副本", "尾随热段") is a direct statement: problem level
                        add_claim(unit_user(u), u, c, d, op, f"{r}:{i} ({'direct' if strong else 'implicit'} "
                                  f"{r.split('/')[1]})", "direct" if strong else "implicit", p,
                                  bool(re.search(r"可选|optional|链上组件", clause)))

perm_n = len(claims)
_perm_seen = defaultdict(lambda: {"users": set(), "kind": None, "p": None, "opt": False})
_rw_seen = {}
for cl in claims:
    for inst, cv in inst_variants(cl["comps"]):
        if inst and cl["unit"] and cl["unit"].startswith("hft-core@") and not cl["unit"].endswith(inst):
            continue
        inst_u = cl["unit"][len("hft-core@"):] if cl["unit"] and cl["unit"].startswith("hft-core@") else None
        if inst_u and any(e_ and e_["only"] and e_["only"] != inst_u
                          for e_ in (find_entry(cv[:k]) for k in range(1, len(cv) + 1))):
            continue
        res = access_fail(cl["user"], cv, cl["op"])
        if res and res != "unreg":
            e, need = res
            o_ = bool(cl["optional"] or e["optional"])
            key = (cl["src"], pstr(cl["comps"]), id(e), need, o_)
            if any(k_[0] == cl["src"] and k_[2] == id(e) and k_[3] == need and k_[4] == o_ for k_ in _perm_seen):
                key = next(k_ for k_ in _perm_seen
                           if k_[0] == cl["src"] and k_[2] == id(e) and k_[3] == need and k_[4] == o_)
            rec = _perm_seen[key]
            rec["users"].add(cl["user"])
            rec["e"], rec["need"], rec["kind"], rec["p"] = e, need, cl["kind"], cl["p"]
            rec["opt"] = rec["opt"] or cl["optional"] or e["optional"]
        # PERMRW: ProtectSystem=strict
        if cl["op"] == "w" and cl["user"] != "root" and cv and cv[0] != "dev" and not cl.get("sock"):
            units = [cl["unit"]] if cl["unit"] else [u for u, info in DEP_UNITS.items() if info["user"] == cl["user"]]
            for u in units:
                info = DEP_UNITS.get(u)
                if not info or info["protect_off"]:
                    continue
                allowed = [tuple(x) for x in info["runtime"] + info["state"] + info["rwpaths"]]
                for e in PATHS:      # a directory the path table says is created by StateDirectory=/RuntimeDirectory=
                    if e["creator"] == "systemd" and e["is_dir"] and dir_unit(e["comps"][-1]) == u:
                        allowed.append(e["comps"])
                if any(len(cv) >= len(a) and all(comp_match(x, y) for x, y in zip(a, cv)) for a in allowed):
                    continue
                k = (u, tuple(cl["comps"]))
                if k not in _rw_seen or (_rw_seen[k][0]["kind"] == "implicit" and cl["kind"] != "implicit"):
                    _rw_seen[k] = (cl, inst)
for key, rec in sorted(_perm_seen.items(), key=lambda kv: kv[0][0]):
    src, target, _, need, _o = key
    e = rec["e"]
    cat = "PERMOPT" if rec["opt"] else ("PERMIMP" if rec["kind"] == "implicit" else "PERM")
    us = sorted(rec["users"])
    add(cat, rec["p"], f"{src}: {', '.join(us[:6])}{' …' if len(us) > 6 else ''} cannot {need_word(need)} "
                       f"{entry_desc(e)} on the way to {target}")
for (u, cv), (cl, inst) in sorted(_rw_seen.items(), key=lambda kv: (kv[0][0], kv[0][1])):
    cat = "PERMOPT" if cl["optional"] else ("PERMIMP" if cl["kind"] == "implicit" else "PERMRW")
    add(cat, cl["p"], f"{cl['src']}: unit `{u}` writes {pstr(cl['comps'])} but has no RuntimeDirectory/StateDirectory/"
                      f"ReadWritePaths= covering it (ProtectSystem=strict; deploy/README.md:{DEP_UNITS[u]['line']})")

# creation rule: file/socket o:g created by process o needs o ∈ g (or a setgid parent with group g)
create_n = 0
for e in PATHS:
    if e["creator"] != "proc" or not e["owner"] or e["owner"] == "root" or not e["group"]:
        continue
    create_n += 1
    if e["group"] in groups_of(e["owner"]):
        continue
    par = find_entry(e["comps"][:-1])
    if par and par["mode"] and len(par["mode"]) == 4 and par["mode"][0] in "2367" and par["group"] == e["group"]:
        continue
    add("PERMOPT" if e["optional"] else "PERM", DEPLOY_MD,
        f"deploy/README.md:{e['line']}: {pstr(e['comps'])} is {e['owner']}:{e['group']} {e['mode']}, created by "
        f"{e['owner']} (not root) — `{e['owner']}` is not in group `{e['group']}` (user table) and the parent is not "
        f"setgid {e['group']}")
# README-stated owner/group/mode of a registered path (and files the process creates with a group it lacks)
RX_STATED = re.compile(r"(`?(?:/(?:dev|run|var/lib)|<cold_root>)[^\s`'\"()(),,;;。、]*`?)[^。;;]{0,40}?"
                       r"(?:属主\s*)?`?((?:[0-7]{4}|[0-7]{3})\s+)?`?((?:[a-z][\w-]*|<[^<>`:\s]+>):[a-z][\w-]*)`?"
                       r"(?:[、,,\s]*`?([0-7]{3,4})`?)?")
for p, r in spec_items():
    if p == DEPLOY_MD:
        continue
    for i, line, in_code in prose_lines(texts[p]):
        for m in RX_STATED.finditer(line):
            c, d = ptuple(m.group(1).strip("`"))
            o, g = m.group(3).split(":")
            mode = (m.group(2) or m.group(4) or "").strip() or None
            for inst, cv in inst_variants(c):
                e = find_entry(cv)
                if not e or e["comps"] != cv and any("<" in x for x in e["comps"]):
                    pass
                if not e:
                    if "创建" in line and o not in ("root",) and not o.startswith("<") and g not in groups_of(o):
                        par = find_entry(cv[:-1])
                        if not (par and par["mode"] and len(par["mode"]) == 4 and par["mode"][0] in "2367"
                                and par["group"] == g):
                            add("PERM", p, f"{r}:{i}: {pstr(cv)} created as {o}:{g} — `{o}` is not in group `{g}`"
                                           f" and the parent ({entry_desc(par) if par else 'unregistered'}) is not setgid {g}")
                    break
                eo = e["owner"]
                if o.startswith("<") or eo is None:
                    continue
                bad = []
                if o != eo:
                    bad.append(f"owner {o} ≠ {eo}")
                if g != e["group"]:
                    bad.append(f"group {g} ≠ {e['group']}")
                if mode and e["mode"] and mode.lstrip("0") != e["mode"].lstrip("0"):
                    bad.append(f"mode {mode} ≠ {e['mode']}")
                if bad:
                    add("PERM", p, f"{r}:{i}: {pstr(cv)} stated {o}:{g} {mode or ''} — {'; '.join(bad)} "
                                   f"(deploy/README.md:{e['line']})")
                break

# ---------- ② GROUPMEM: user table × unit SupplementaryGroups= × "G 组只有 A 与 B" / "A 加入 G" / "A 不在 G 组"
groupmem_n = 0
for n, info in sorted(DEP_UNITS.items()):
    if info["supp"] is None or not info["user"] or info["user"] == "root":
        continue
    groupmem_n += 1
    ut = DEP_USERS.get(info["user"])
    if ut is None:
        add("GROUPMEM", DEPLOY_MD, f"deploy/README.md:{info['line']}: unit `{n}` runs as `{info['user']}`, which has no row"
                                   f" in the user table")
        continue
    if info["supp"] != ut["groups"]:
        miss, extra = sorted(ut["groups"] - info["supp"]), sorted(info["supp"] - ut["groups"])
        add("GROUPMEM", DEPLOY_MD, f"deploy/README.md:{info['line']}: `{n}` SupplementaryGroups= "
                                   f"{{{' '.join(sorted(info['supp']))}}} ≠ user table `{info['user']}` "
                                   f"{{{' '.join(sorted(ut['groups']))}}} (deploy/README.md:{ut['line']})"
                                   + (f"; missing {', '.join(miss)}" if miss else "") + (f"; extra {', '.join(extra)}" if extra else ""))


def member_tokens(txt):
    """Users named in a member list: `user`, `unit` (→ its user), `unit` 的运行用户."""
    out = []
    for m in re.finditer(r"`([\w@.-]+)`(\s*的运行用户)?", txt):
        u = resolve_user(m.group(1))
        if u:
            out.append(u)
    return out


RX_ONLY = re.compile(r"(?:`(hft[\w-]*)`\s*组(?:的)?(?:成员)?|组成员)\s*只有\s*([^。;;::((]*)")
RX_JOIN = re.compile(r"加入\s*`(hft[\w-]*)`")
RX_NOTIN = re.compile(r"`(hft[\w@-]*)`[^`。;;]{0,12}?不在\s*((?:`hft[\w-]*`\s*[、与和]?\s*)+)组")
RX_NOTIN_THIS = re.compile(r"`(hft[\w@-]*)`[^`。;;]{0,6}?不在该组")
for p, r in spec_items():
    for i, line, in_code in prose_lines(texts[p]):
        if in_code:
            continue
        for m in RX_ONLY.finditer(line):
            g = m.group(1)
            if not g:
                prev_g = [t.split(":")[-1] for t in RX_BT.findall(line[:m.start()])]
                prev_g = [t for t in prev_g if t in DEP_GROUPS and t not in DEP_USERS]
                g = prev_g[-1] if prev_g else None
            if not g:
                continue
            claimed = set(member_tokens(m.group(2)))
            if not claimed:
                continue
            groupmem_n += 1
            actual = group_members(g)
            if claimed != actual:
                add("GROUPMEM", p, f"{r}:{i}: `{g}` 组只有 {{{', '.join(sorted(claimed))}}} but the deploy user table gives "
                                   f"{{{', '.join(sorted(actual)) or '∅'}}}")
        for m in RX_JOIN.finditer(line):
            g = m.group(1)
            seg = re.split(r"[。;;]", line[:m.start()])[-1][-120:]
            for u in member_tokens(seg):
                groupmem_n += 1
                if u not in group_members(g):
                    add("GROUPMEM", p, f"{r}:{i}: `{u}` 加入 `{g}` but the deploy user table does not list it")
        for m in RX_NOTIN.finditer(line):
            u = resolve_user(m.group(1))
            for g in RX_BT.findall(m.group(2)):
                groupmem_n += 1
                if u and u in group_members(g) and u != g:
                    add("GROUPMEM", p, f"{r}:{i}: `{u}` 不在 `{g}` 组 but the deploy user table lists it")
        for m in RX_NOTIN_THIS.finditer(line):
            u = resolve_user(m.group(1))
            prev_g = [t for t in RX_BT.findall(line[:m.start()]) if t in DEP_GROUPS and t != m.group(1)]
            if u and prev_g:
                groupmem_n += 1
                if u in group_members(prev_g[-1]):
                    add("GROUPMEM", p, f"{r}:{i}: `{u}` 不在该组 (`{prev_g[-1]}`) but the deploy user table lists it")
        for m in RX_SUPPREAD.finditer(line):
            u = resolve_user(m.group(1))
            if u:
                groupmem_n += 1
                if m.group(2) not in groups_of(u):
                    add("GROUPMEM", p, f"{r}:{i}: {m.group(1)} 以附加组 `{m.group(2)}` — `{u}` lacks it in the user table")

# ---------- ③ S3KEY: S3 object-key templates are `<prefix>/<host>/<instance>/…` — an instance segment (`<instance>`,
# `prod`, `sim`, `%i`) directly after the prefix (`<prefix>`, `hft`, `hft-testnet`, `<hft 或 hft-testnet>`) is a key
# without the host segment (old `hft/sim/` style); secret prefixes hft/{live,testnet,ops}/ and hft/verify/ are not keys
RX_S3KEY = re.compile(r"`((?:<prefix>|hft|hft-testnet|<hft 或 hft-testnet>)/([^/`\s]+)(?:/[^`]*)?)`")
s3key_n = 0
for p, r in spec_items() + design_items():
    for i, line, in_code in prose_lines(texts[p]):
        if not re.search(r"S3|s3://|前缀|prefix|对象键|bucket|桶|归档|上传|manifest|Parquet",
                         re.sub(r"\]\([^)]*\)", "]", line)):
            continue
        for m in RX_S3KEY.finditer(line):
            seg = m.group(2)
            if m.group(1).startswith(("hft/live/", "hft/testnet/", "hft/ops/", "hft/verify")):
                continue
            s3key_n += 1
            if seg in ("<instance>", "prod", "sim", "%i", "<inst>"):
                add("S3KEY", p, f"{r}:{i}: S3 key `{m.group(1)}` has no `<host>` segment after the prefix "
                                f"(template `<prefix>/<host>/<instance>/…`, deploy/README.md S3 前缀)")

# ---------- ④ IAMSET: secrets / S3 / KMS grants per host class — deploy (home) vs runbooks; KMS covers SSE-KMS keys
RUNBOOKS = hft("docs/runbooks")
RX_SECPFX = re.compile(r"`(hft/(?:live|testnet|ops)(?:/[^`\s]*)?)`")


def sec_norm(t):
    return t.rstrip("*").rstrip("…").rstrip("/") + "/"


def host_class(line, heads, r):
    ctx = line + " " + " ".join(heads[-2:])
    if re.search(r"formal_live\s*=\s*false|\bdev2\b", line):
        return "dev2"
    if re.search(r"升级主机|formal_live\s*=\s*true", line):
        return "upgrade"
    if r.endswith("host-upgrade.md"):
        return "upgrade"
    if re.search(r"\bdev2\b", ctx):
        return "dev2"
    if re.search(r"升级主机", ctx):
        return "upgrade"
    return None


IAM_DEP = {}      # class -> (set of secret prefixes, line, text)
_iam_sec = section_text(DEPLOY_MD, "秘密与实例角色") or ""
_iam_off = None
for i, line, in_code, heads in walk_lines(_dep_s):
    if in_code or not re.search(r"GetSecretValue|实例角色授权|授权\s*`hft/", line):
        continue
    cls = host_class(line, list(heads), "deploy/README.md")
    pf = {sec_norm(t) for t in RX_SECPFX.findall(line) if t.endswith("*") or t.endswith("/*")}
    if cls and pf:
        IAM_DEP.setdefault(cls, (pf, i, line))
iam_n = 0
for p, r in spec_items():
    if not r.startswith("docs/runbooks/"):
        continue
    for i, line, in_code, heads in walk_lines(texts[p]):
        if in_code:
            continue
        if not re.search(r"secretsmanager|GetSecretValue|实例角色|IAM|授权|能取", line):
            continue
        pf = {sec_norm(t) for t in RX_SECPFX.findall(line) if t.endswith("*")}
        if not pf:
            continue
        cls = host_class(line, list(heads), r)
        if cls not in IAM_DEP:
            continue
        iam_n += 1
        dep_pf, dl, _ = IAM_DEP[cls]
        if pf != dep_pf:
            add("IAMSET", p, f"{r}:{i}: {cls} secrets grant {{{', '.join(sorted(x + '*' for x in pf))}}} ≠ deploy "
                             f"{{{', '.join(sorted(x + '*' for x in dep_pf))}}} (deploy/README.md:{dl})")
# S3 read: a class whose role deploy says is "只有 S3 写" while a runbook needs S3 读 / the archiver HEADs objects
for cls, (pf, dl, dline) in IAM_DEP.items():
    if not re.search(r"只有\s*S3\s*写|只\s*S3\s*写|S3\s*只写", dline):
        continue
    iam_n += 1
    if re.search(r"`HEAD`|HeadObject|GetObject", texts.get(ARCHIVER_MD, "")):
        add("IAMSET", DEPLOY_MD, f"deploy/README.md:{dl}: {cls} instance role is S3 write-only but journal-archiver "
                                 f"HEADs objects before PUT (s3:GetObject/ListBucket needed)")
    for p, r in spec_items():
        if not r.startswith("docs/runbooks/"):
            continue
        for i, line, in_code, heads in walk_lines(texts[p]):
            if not in_code and re.search(r"S3\s*(?:读|Get|List)", line) and host_class(line, list(heads), r) == cls:
                add("IAMSET", p, f"{r}:{i}: uses the {cls} role's S3 read permission; deploy/README.md:{dl} gives it "
                                 f"S3 write only")
# KMS: every consumer that writes (or names) an SSE-KMS prefix needs a KMS grant in deploy
kms_pfx = set()
for p, r in spec_items():
    for i, line, in_code in prose_lines(texts[p]):
        if "SSE-KMS" in line:
            for t in RX_BT.findall(line):
                if "private" in t and "/" in t:
                    kms_pfx.add(t)
if kms_pfx:
    _cons = re.search(r"\*\*消费者\*\*(.*)", _dep_s)
    entries = []
    if _cons:
        for m in re.finditer(r"`(hft-[\w-]+)`\s*[((]((?:[^()()]|[((][^()()]*[))])*)[))]", _cons.group(1)):
            entries.append((m.group(1), m.group(2)))
    for u, body in entries:
        writes = bool(re.search(r"写", body))
        covers = "private" in body or (writes and re.search(r"`hft/`|`hft-testnet/`|`hft/\*`", body))
        if not covers:
            continue
        iam_n += 1
        if not re.search(r"KMS|kms:", body) and not any(u in ln_ and re.search(r"KMS|kms:", ln_)
                                                         for ln_ in _dep_s.splitlines()):
            add("IAMSET", DEPLOY_MD, f"deploy/README.md: consumer `{u}` reads/writes SSE-KMS objects "
                                     f"({', '.join(sorted(kms_pfx))[:80]}) but deploy grants it no KMS permission")
    if not entries and not re.search(r"KMS|kms:", _iam_sec + (section_text(DEPLOY_MD, "imds-锁定与凭证代理") or "")):
        add("IAMSET", DEPLOY_MD, "deploy/README.md 秘密与实例角色 / 凭证代理: no KMS grant anywhere, yet "
                                 f"{', '.join(sorted(kms_pfx))[:90]} are SSE-KMS")

# ---------- ⑤ UNITFORM: the same identifier pair in two inequalities (units _ns/_us/_ms/_s normalised) must agree;
# UNITMIX[W]: one inequality comparing different units with no 1 000-power factor
UNIT_F = {"ns": 1, "us": 1e3, "ms": 1e6, "s": 1e9}
RX_INEQ = re.compile(r"^\s*(.+?)\s*(<=|>=|≤|≥|<|>)\s*(.+?)\s*$")
RX_IDU = re.compile(r"(?:[a-z_][\w]*\.)*([a-z][a-z0-9_]*?)_(ns|us|ms|s)$")


def ineq_side(t):
    """(coef, base, unit) of `[N ×] ident [× N | / N]`, numbers with spaces/commas/underscores; None otherwise."""
    t = t.strip().strip("()")
    t = re.sub(r"(?<=\d)[ ,_](?=\d{3}\b)", "", t)
    toks = re.split(r"\s*([×*/])\s*", t)
    coef, ident, op = 1.0, None, "*"
    for tk in toks:
        if tk in ("×", "*", "/"):
            op = tk
            continue
        if re.fullmatch(r"\d+(?:\.\d+)?", tk):
            v = float(tk)
            coef = coef / v if op == "/" else coef * v
        elif RX_IDU.fullmatch(tk) and ident is None and op != "/":
            ident = tk
        else:
            return None
    if not ident:
        return None
    m = RX_IDU.fullmatch(ident)
    return coef, m.group(1), m.group(2)


def kilo(c):
    """c (or 1/c) is a multiple of 1 000, 1 000 000 or 1 000 000 000."""
    for v in (c, 1 / c if c else 0):
        if v >= 1000 and any(abs(v / 10 ** e - round(v / 10 ** e)) < 1e-9 for e in (3, 6, 9)):
            return True
    return False


ineqs = defaultdict(list)     # (base_a, base_b) -> [(k, rr:line, text)]
unitform_n = 0
for p, r in spec_items():
    for i, line, in_code in prose_lines(texts[p]):
        for t in RX_BT.findall(line):
            m = RX_INEQ.match(t)
            if not m or "&&" in t or "max(" in t or "min(" in t:
                continue
            a, b = ineq_side(m.group(1)), ineq_side(m.group(3))
            if not a or not b or a[1] == b[1]:
                continue
            unitform_n += 1
            # raw: ca·A_raw OP cb·B_raw ; physical A = A_raw·ua → A OP (cb·ua)/(ca·ub)·B
            k = (b[0] * UNIT_F[a[2]]) / (a[0] * UNIT_F[b[2]])
            key = (a[1], b[1]) if a[1] < b[1] else (b[1], a[1])
            kk = k if a[1] < b[1] else 1 / k
            ineqs[key].append((kk, f"{r}:{i}", t, p))
            if a[2] != b[2] and not any(kilo(c) for c in (a[0], b[0])):
                add("UNITMIX", p, f"{r}:{i}: `{t}` compares _{a[2]} with _{b[2]} without a 1 000-power factor")
for key, lst in sorted(ineqs.items()):
    ks = {round(x[0], 9) for x in lst}
    if len(ks) > 1:
        where = "; ".join(f"{w} `{t}` (⇒ {key[1]} = {1 / x:g} × {key[0]} in common units)" for x, w, t, _ in lst)
        add("UNITFORM", lst[0][3], f"{key[0]} / {key[1]}: inequalities disagree after unit normalisation: {where}")

# ---------- ⑥ WIRECODE: a wire code named by type ("arg = `X`", "码 = `X`", "arg = hft-refdata `X`") must have numeric
# values (enum with explicit discriminants, `Name=N` list, const bit table); a Timer that carries a key (`Name{key}`,
# or a *TimerReq/kind+id struct) needs an encoding formula (`timer_id = N | … << 8`) in the hft-event timer table
enum_vals = {}            # enum name -> (all unit variants valued, file:line)
for ed in enum_defs:
    unit = [v for v in ed["variants"] if not v[2]]
    if unit:
        enum_vals[ed["name"]] = (all(v[1] for v in unit), f"{rroot(ed['file'])}:{ed['line']}")


def has_numeric_table(x):
    if x in enum_vals and enum_vals[x][0]:
        return True
    for p, r in spec_items():
        s = texts[p]
        if x not in s:
            continue
        for i, line, in_code in prose_lines(s):
            if x not in line:
                continue
            if len(re.findall(r"(?<![\w])[A-Z]\w*\s*=\s*(?:\d|1\s*<<)", line)) >= 2 and re.search(
                    r"(?<![\w`])`?" + re.escape(x) + r"`?(?![\w])", line):
                return True
        m = re.search(r"struct\s+" + re.escape(x) + r"\s*\(", s)
        if m and re.search(r"impl\s+" + re.escape(x) + r"\s*\{[^}]*=\s*1\s*<<", s, re.S):
            return True
        for hl, hdr, rows in md_tables(p):
            cap = "\n".join(s.splitlines()[max(0, hl - 3):hl - 1])
            if (x in " ".join(hdr) or re.search(r"`" + re.escape(x) + r"`", cap)) and any(
                    re.search(r"码|值|code|位", c) for c in hdr):
                if sum(1 for _, cells in rows if re.search(r"\b\d+\b", " ".join(cells))) >= 2:
                    return True
    return False


wirecode_n = 0
_wc_uses = defaultdict(list)
RX_ARG = re.compile(r"(?:arg|码|code)\s*=\s*(?:[\w-]+\s+)?`([A-Z]\w+)`")
for p, r in spec_items():
    for i, line, in_code in prose_lines(texts[p]):
        for m in RX_ARG.finditer(line):
            wirecode_n += 1
            _wc_uses[m.group(1)].append((p, f"{r}:{i}"))
for x, uses in sorted(_wc_uses.items()):
    if not has_numeric_table(x):
        add("WIRECODE", uses[0][0], f"`{x}` is used as a wire code (arg/码 = `{x}`) at {', '.join(w for _, w in uses[:5])}"
                                    f" but has no numeric code table (enum with explicit discriminants or Name=N list)"
                                    + (f"; enum at {enum_vals[x][1]} has unvalued variants" if x in enum_vals else ""))
# timers: the hft-event timer table rows → per owner, names with value and whether an encoding is attached
TIMER_ENC = {}            # (owner, name) -> has encoding
for hl, hdr, rows in md_tables(EVENT):
    if hdr and hdr[0].strip("`") == "owner" and any("timer_id" in c for c in hdr):
        tcol = next(k for k, c in enumerate(hdr) if "timer_id" in c)
        for ln, cells in rows:
            owner = cells[0].strip("`")
            cell = cells[tcol] if tcol < len(cells) else ""
            row_enc = bool(re.match(r"\s*`?timer_id\s*=\s*[^`]*<<", cell))
            for m in re.finditer(r"`?([A-Z]\w*)\s*=\s*(\d+)`?((?:\s*[((][^()()]*[))])?)", cell):
                TIMER_ENC[(owner, m.group(1))] = row_enc or "<<" in m.group(3)
keyed = defaultdict(list)     # (owner, name) -> [where]
for p, r in spec_items():
    s = texts[p]
    for i, line, in_code in prose_lines(s):
        for m in re.finditer(r"Timer\{\s*(\w+)\s*,\s*(\w+)\s*\{\s*(\w+)", line):
            keyed[(m.group(1), m.group(2))].append(f"{r}:{i}")
    for m in re.finditer(r"struct\s+(\w*TimerReq)\s*\{(.*?)\}", s, re.S):
        body = m.group(2)
        km = re.search(r"kind\s*:\s*(\w+)\s*/\*\s*([^*]*)\*/", body)
        if km and re.search(r"\bid\s*:", body):
            owner = re.sub(r"TimerReq$", "", m.group(1))
            ln = s[:m.start()].count("\n") + 1
            for nm in re.findall(r"[A-Z]\w+", km.group(2)):
                keyed[(owner, nm)].append(f"{r}:{ln} ({m.group(1)}.id)")
for (owner, nm), where in sorted(keyed.items()):
    wirecode_n += 1
    if (owner, nm) in TIMER_ENC and not TIMER_ENC[(owner, nm)]:
        add("WIRECODE", EVENT, f"Timer{{{owner}, {nm}}} carries a key ({where[0]}) but the hft-event timer table gives only "
                               f"`{nm}=N`, no `timer_id = N | … << 8` encoding")

# ---------- ⑦ VERBATIM: a line that embeds a type "原样 / 布局原样" (journal rows, snapshot rows, records) must not
# embed a #[repr(C, u8)] data enum or a type documented "不以原始字节写进 journal" (directly or in a field)
NORAW = set()
for p, s in texts.items():
    if not in_spec(p):
        continue
    for start, lines in code_blocks(s):
        doc = []
        for line in lines:
            st = line.strip()
            if st.startswith("//"):
                doc.append(st)
                continue
            m = re.search(r"\b(?:struct|enum)\s+([A-Z]\w*)", st)
            if m and "不以原始字节" in " ".join(doc) + st:
                NORAW.add(m.group(1))
            if st and not st.startswith("#["):
                doc = []
DATA_ENUMS = {sd["name"] for sd in struct_defs if "enum_data" in sd}
FIELDS_OF = defaultdict(list)
for sd in struct_defs:
    if "fields" in sd:
        FIELDS_OF[sd["name"]].append(sd)


def bad_embedded(t, seen=None):
    """Chain of type names from t to a data enum / NORAW type, or None."""
    seen = seen or set()
    if t in seen:
        return None
    seen.add(t)
    if t in DATA_ENUMS or t in NORAW:
        return [t]
    for sd in FIELDS_OF.get(t, [])[:1]:
        for fd in sd["fields"]:
            for nm in re.findall(r"[A-Z]\w*", fd["type"]):
                ch = bad_embedded(nm, seen)
                if ch:
                    return [t + "." + fd["name"]] + ch
    return None


verbatim_n = 0
for p, r in spec_items():
    for i, line, in_code in prose_lines(texts[p]):
        if "原样" not in line:
            continue
        for clause in re.split(r"[。;;]", line):
            if "原样" not in clause:
                continue
            pos = clause.find("原样")
            names = [m.group(1) for m in re.finditer(r"(?<![\w.:])([A-Z][A-Za-z0-9]+)(?![\w])", clause[:pos])]
            names = [n for n in names if n in FIELDS_OF or n in DATA_ENUMS or n in NORAW]
            if not names:
                continue
            verbatim_n += 1
            ch = bad_embedded(names[-1])
            if ch:
                add("VERBATIM", p, f"{r}:{i}: `{names[-1]}` written 原样 (raw bytes) but embeds {' → '.join(ch)}"
                                   f" ({'#[repr(C, u8)] data enum' if ch[-1] in DATA_ENUMS else '不以原始字节写进 journal'})")

# ---------- ⑧ APPENDONLY: wire records and snapshot rows vs the --prev tree (same file, same struct name).
# A named field that moved / changed type / vanished, a new field inside the old body that consumes reserved bytes
# (`_rsv`/`_pad`; allowed only when the event-model evolution rules permit "保留字节改为具名字段"), or a changed
# layout whose schema_version / 行版本 did not increase.
def field_offsets(sd):
    packed = "packed" in sd["repr"]
    off, out = 0, []
    for fd in sd["fields"]:
        sa = type_size(fd["type"], sd["file"])
        if sa is None:
            return None
        sz, al = sa
        al = 1 if packed else al
        st = (off + al - 1) // al * al
        out.append((fd["name"], re.sub(r"\s+", "", fd["type"]), st, sz))
        off = st + sz
    return out


def struct_hdr(tmap, sd):
    lines = tmap[sd["file"]].splitlines()
    k = sd["line"] - 1
    out = [lines[k]] if 0 <= k < len(lines) else []
    j = k - 1
    while j >= 0 and j >= k - 4 and lines[j].strip().startswith(("//", "#[")):
        out.insert(0, lines[j])
        j -= 1
    return " ".join(out)


def section_versions(tmap, root_):
    """{(rel file, section no): version} from 行版本 registries ("节 2 = 1", "节 1 v2", "节 6 `X`,行版本 1")."""
    out = {}
    for p, s in tmap.items():
        if not p.startswith(root_):
            continue
        rr_ = os.path.relpath(p, root_).replace(os.sep, "/")
        for line in s.splitlines():
            if "行版本" not in line and not re.search(r"节\s*\d+\s*v\d", line):
                continue
            for a, b in re.findall(r"节\s*(\d+)\s*(?:=\s*|v)(\d+)", line):
                out[(rr_, int(a))] = int(b)
            for a, b in re.findall(r"节\s*(\d+)[^,,;;。\n]{0,40}?行版本\s*(\d+)", line):
                out[(rr_, int(a))] = int(b)
    return out


def section_members(tmap, root_, sdefs):
    """{(rel file, struct name): section no} — structs whose header says 节 N, types named in a '// 节 N' comment block,
    and (transitively) the struct types of their fields."""
    names = {sd["name"] for sd in sdefs if "fields" in sd}
    byname = defaultdict(list)
    for sd in sdefs:
        if "fields" in sd:
            byname[sd["name"]].append(sd)
    out = {}
    for sd in sdefs:
        if "fields" not in sd:
            continue
        m = re.search(r"节\s*(\d+)", struct_hdr(tmap, sd))
        if m:
            out[(sd["file"], sd["name"])] = int(m.group(1))
    for p, s in tmap.items():
        if not p.startswith(root_):
            continue
        for start, lines in code_blocks(s):
            cur = None
            for line in lines:
                st = line.strip()
                m = re.match(r"//\s*节\s*(\d+)", st)
                if m:
                    cur = int(m.group(1))
                elif not st.startswith("//"):
                    if not st.startswith(("#[", "const _")):
                        cur = None
                    continue
                if cur is not None:
                    for nm in re.findall(r"[A-Z]\w+", st):
                        if nm in names:
                            out.setdefault((p, nm), cur)
    changed = True
    while changed:
        changed = False
        for (p, nm), sec in list(out.items()):
            for sd in byname.get(nm, [])[:1]:
                for fd in sd["fields"]:
                    for t in re.findall(r"[A-Z]\w+", fd["type"]):
                        for sd2 in byname.get(t, [])[:1]:
                            if (sd2["file"], t) not in out:
                                out[(sd2["file"], t)] = sec
                                changed = True
    return out


def struct_version(hdr):
    m = re.search(r"schema_version\s*(\d+)|行版本\s*(\d+)", hdr)
    return int(m.group(1) or m.group(2)) if m else None


appendonly_n = 0
prev_ok = bool(PREV) and os.path.isdir(PREV) and \
    os.path.normcase(os.path.abspath(PREV)) != os.path.normcase(os.path.abspath(ROOT))
prev_texts = {}
if prev_ok:
    for dp, dn, fn in os.walk(PREV):
        for f in fn:
            if f.endswith(".md"):
                prev_texts[os.path.join(dp, f)] = read(os.path.join(dp, f))
    prev_structs, PREV_D = parse_code(prev_texts, PREV)
    _ev_txt = texts.get(hft("docs/subsystems/event-model.md"), "") + texts.get(EVENT, "")
    RESERVED_OK = bool(re.search(r"保留(?:字节|位|字段)[^。\n]{0,40}(?:具名|改为|占用)|(?:具名字段|占用)[^。\n]{0,30}保留(?:字节|位|字段)",
                                 _ev_txt))
    cur_sv, prev_sv = section_versions(texts, ROOT), section_versions(prev_texts, PREV)
    cur_mem = section_members(texts, ROOT, struct_defs)
    prev_mem = section_members(prev_texts, PREV, prev_structs)
    prev_by = {}
    for sd in prev_structs:
        if "fields" in sd:
            prev_by.setdefault((os.path.relpath(sd["file"], PREV).replace(os.sep, "/"), sd["name"]), sd)
    _saved_D = D
    for sd in struct_defs:
        if "fields" not in sd or not spec_file(sd["file"]):
            continue
        rr = rroot(sd["file"])
        old = prev_by.get((rr, sd["name"]))
        if not old:
            continue
        hdr_c, hdr_o = struct_hdr(texts, sd), struct_hdr(prev_texts, old)
        kind_code = re.search(r"0x0[0-9A-Fa-f]{3}", hdr_c)
        sec = cur_mem.get((sd["file"], sd["name"]))
        if not kind_code and sec is None and rr not in ("crates/hft-event/README.md", "docs/subsystems/event-model.md",
                                                         "docs/subsystems/journal-format.md"):
            continue
        if not kind_code and sec is None and "repr(C" not in sd["repr"] and not sd["block_has_repr"] \
                and rr not in ("crates/hft-event/README.md", "docs/subsystems/event-model.md",
                               "docs/subsystems/journal-format.md"):
            continue
        fc = field_offsets(sd)
        D = PREV_D
        fo = field_offsets(old)
        lo = layout(old)
        D = _saved_D
        lc = layout(sd)
        if fc is None or fo is None or lc is None or lo is None:
            continue
        appendonly_n += 1
        if fc == fo and lc[0] == lo[0]:
            continue
        what = []
        cur_named = {n: (t, o, z) for n, t, o, z in fc}
        for n, t, o, z in fo:
            if n.startswith("_"):
                continue
            if n not in cur_named:
                what.append(f"field `{n}` removed or renamed")
            elif cur_named[n][:2] != (t, o):
                what.append(f"field `{n}` {t}@{o} → {cur_named[n][0]}@{cur_named[n][1]}")
        old_named = {n for n, t, o, z in fo if not n.startswith("_")}
        consumed = [n for n, t, o, z in fc if not n.startswith("_") and n not in old_named and o < lo[0]]
        if consumed and not RESERVED_OK:
            what.append(f"new field(s) {', '.join('`' + x + '`' for x in consumed)} inside the old {lo[0]} B body "
                        f"(reserved bytes consumed; event-model 演进规则 only allows appending at the tail)")
        if sec is not None:
            vc = cur_sv.get((rr, sec))
            vo = prev_sv.get((os.path.relpath(old["file"], PREV).replace(os.sep, "/"), prev_mem.get((old["file"], sd["name"]), sec)))
            vlabel = f"节 {sec} 行版本"
        else:
            vc, vo, vlabel = struct_version(hdr_c), struct_version(hdr_o), "schema_version"
        vc = vc if vc is not None else struct_version(hdr_c)
        vo = vo if vo is not None else struct_version(hdr_o)
        if lc[0] != lo[0] or [x for x in what if "→" in x or "removed" in x]:
            if vc is None and vo is None:
                if kind_code or sec is not None:
                    what.append(f"layout {lo[0]} B → {lc[0]} B with no {vlabel} recorded")
            elif (vc or 1) <= (vo or 1):
                what.append(f"layout {lo[0]} B → {lc[0]} B but {vlabel} stays {vc or 1}")
        if what:
            add("APPENDONLY", sd["file"], f"{rr}:{sd['line']}: {sd['name']} vs {os.path.basename(os.path.dirname(PREV)) or PREV}: "
                                          + "; ".join(what))
    D = _saved_D

# ---------- ⑨ ALLHOSTS: a "(全部主机类)" unit rule outside deploy must be in deploy's main unit table (关键设置 column,
# or a deploy section it links with 见[…]) — not only in the dev2 override column / dev2 section
BATCH = []
for hl, hdr, rows in md_tables(DEPLOY_MD):
    if hdr and hdr[0] == "slice":
        for ln, cells in rows:
            if "batch" in cells[0] and len(cells) > 1:
                BATCH = [unit_norm(t) for t in RX_BT.findall(cells[1]) if t.startswith("hft-")]
NUMERIC_KEYS = set()
_nm = re.search(r"资源数值[((]([^))]*)[))]", _dep_s)
if _nm:
    NUMERIC_KEYS = {t.rstrip("=") for t in RX_BT.findall(_nm.group(1))}
NUMERIC_KEYS |= {"Nice", "MemoryMax", "CPUQuota", "OOMScoreAdjust", "CPUWeight", "IOReadBandwidthMax",
                 "IOWriteBandwidthMax", "AllowedCPUs", "CPUAffinity"}


def main_text(u):
    info = DEP_UNITS.get(u)
    return (info["main"] + " " + info.get("ext", "")) if info else ""


def mentions(txt, u):
    return bool(re.search(r"(?<![\w-])" + re.escape(u) + r"(?:\.service)?(?![\w-])", txt))


allhosts_n = 0
for p, r in spec_items():
    if p == DEPLOY_MD:
        continue
    for i, line, in_code in prose_lines(texts[p]):
        if in_code or "全部主机类" not in line:
            continue
        for clause in split_depth0(line):
            if re.search(r"\bdev2\b", re.sub(r"\([^)]*\)|([^)]*)", "", clause)):
                continue
            units = []
            for t in RX_BT.findall(clause):
                u = unit_norm(t)
                if u in DEP_UNITS and u not in units:
                    units.append(u)
            for a in units[1:]:
                allhosts_n += 1
                s0 = units[0]
                if not (mentions(main_text(s0), a) or mentions(main_text(a), s0)):
                    add("ALLHOSTS", p, f"{r}:{i}: (全部主机类) `{s0}` ↔ `{a}` — neither unit's row in the deploy main table "
                                       f"(关键设置, deploy/README.md:{DEP_UNITS[s0]['line']}/{DEP_UNITS[a]['line']}) "
                                       f"names the other (only a dev2 override or section?)")
            subj = units or (BATCH if "批处理" in clause else [])
            for t in RX_BT.findall(clause):
                m = re.fullmatch(r"([A-Z][A-Za-z]+)=(\S+)", t)
                if not m or m.group(1) in NUMERIC_KEYS:
                    continue
                for u in subj:
                    allhosts_n += 1
                    if t not in main_text(u):
                        add("ALLHOSTS", p, f"{r}:{i}: (全部主机类) `{t}` for `{u}` is not in its deploy main-table row "
                                           f"(deploy/README.md:{DEP_UNITS[u]['line']})")

# ---------- ⑩ LISTCOPY[W]: "只有/只含/白名单/只限/允许表 …" lists of ≥ 3 backticked identifiers compared across lines
RX_LISTKW = re.compile(r"只有|只含|白名单|只限|只允许|允许表|仅含|仅有")
PROSE_ENUMS = {}
for p, r in spec_items():
    for i, line, in_code in prose_lines(texts[p]):
        for m in re.finditer(r"`((?:[A-Z]\w*\s*=\s*\d+[\s,、]*){3,})`", line):
            prev_t = [t for t in RX_BT.findall(line[:m.start()]) if re.fullmatch(r"[A-Z]\w+", t)]
            if prev_t:
                PROSE_ENUMS.setdefault(prev_t[-1], (set(re.findall(r"([A-Z]\w*)\s*=", m.group(1))), f"{r}:{i}"))
for ed in enum_defs:
    if len(ed["variants"]) >= 3:
        PROSE_ENUMS.setdefault(ed["name"], ({v[0] for v in ed["variants"]}, f"{rroot(ed['file'])}:{ed['line']}"))


def list_ids(seg):
    seg = re.sub(r"[((]\s*(?:不含|不包括|除)[^))]*[))]", "", seg)
    ids = []
    for t in RX_BT.findall(seg):
        for x in re.split(r"\s*/\s*", t):
            x = unit_norm(re.split(r"[({\s]", x.strip())[0])
            if len(x) >= 3 and re.fullmatch(r"[\w@.:-]+", x) and not re.fullmatch(r"\d+", x):
                ids.append(x)
    if re.search(r"两个\s*hft-core\s*实例", seg):
        ids += ["hft-core@prod", "hft-core@sim"]
    if re.search(r"批处理单元", seg):
        ids += BATCH
    return set(ids)


lists = []
for p, r in spec_items():
    for i, line, in_code in prose_lines(texts[p]):
        if in_code:
            continue
        kws = list(RX_LISTKW.finditer(line))
        for k_, m in enumerate(kws):
            rest = line[m.end():kws[k_ + 1].start()] if k_ + 1 < len(kws) else line[m.end():]
            seg = split_depth0(rest, "。;;,,")[0][:300]          # a list ends at the next keyword or clause / comma
            if len(list_ids(seg)) < 3 and line.lstrip().startswith("|"):
                seg = re.split(r"(?<!\\)\|", line[m.end():])[0]   # table cell holding a <br> list (T7)
            ids = list_ids(seg)
            if len(ids) >= 3:
                lists.append((p, r, i, ids, m.group(0)))


def fuzzy(a, b):
    if a == b:
        return 2
    if re.fullmatch(r"[a-z]\w*", a) and re.fullmatch(r"[a-z]\w*", b) and min(len(a), len(b)) >= 4:
        s, l_ = (a, b) if len(a) < len(b) else (b, a)
        if l_.startswith(s) and len(l_) - len(s) <= 3:
            return 1
    return 0


listcopy_n = len(lists)
_lc_seen = set()
for x in range(len(lists)):
    p1, r1, i1, s1, k1 = lists[x]
    for y in range(len(lists)):
        if x == y:
            continue
        p2, r2, i2, s2, k2 = lists[y]
        if (r1, i1) == (r2, i2) or s1 == s2:
            continue
        key = tuple(sorted([(r1, i1), (r2, i2)]))
        exact = s1 & s2
        fz = {a: [b for b in s2 if fuzzy(a, b) == 1] for a in s1 - exact}
        if len(s1) <= len(s2) and all(fz[a] for a in s1 - exact) and s1 - exact and key not in _lc_seen:
            _lc_seen.add(key)
            add("LISTCOPY", p1, f"{r1}:{i1}: list {{{', '.join(sorted(s1))}}} spells names differently from {r2}:{i2}: "
                                f"{', '.join(a + '~' + fz[a][0] for a in sorted(s1 - exact))}")
            continue
        jac = len(exact) / len(s1 | s2)
        if jac >= 0.6 and key not in _lc_seen and x < y:
            _lc_seen.add(key)
            add("LISTCOPY", p1, f"{r1}:{i1} vs {r2}:{i2}: '{k1}'-lists differ — only here {{{', '.join(sorted(s1 - s2))}}}, "
                                f"only there {{{', '.join(sorted(s2 - s1))}}}")
    if k1 in ("只有", "只含", "仅有", "仅含"):
        for en, (vs, where) in PROSE_ENUMS.items():
            inter = s1 & vs
            if len(inter) >= 3 and len(inter) >= len(s1) - 1 and len(inter) >= 0.4 * len(vs) and vs - s1 \
                    and (r1, i1, en) not in _lc_seen:
                _lc_seen.add((r1, i1, en))
                add("LISTCOPY", p1, f"{r1}:{i1}: '{k1}' list names {len(inter)} of {en}'s {len(vs)} variants ({where});"
                                    f" missing {', '.join(sorted(vs - s1))}")

# ---------- ⑪ BUDGET / BUDGETPCT[W]: a crate budget row (延迟与资源契约 tables) is not wider than the benchmarks
# 单 crate 基准上限表 row of its primitive. A bound row may delegate to a crate's table ("…门槛表归 [crate 延迟与资源契约]"):
# then each row of that table is the bound for the same entry (`decode_md`, `encode`, …) and message class in the listed
# crates. Matching: primitive keyword / entry name, message class, case label; warm / optional / command-path rows are
# skipped. A bound without percentile is compared with every percentile of the crate row: the lowest percentile
# exceeding it is a problem, a higher one only BUDGETPCT[W] (the bound should say which percentile it is).
BENCH_MD = hft("benchmarks/README.md")
PRIM_KW = [("解码", r"decode"), ("环跳", r"单跳|hop"), ("journal 追加", r"\bappend"), ("簿更新", r"\bapply"),
           ("风控链", r"evaluate"), ("OMS 分配槽", r"\bplace\b"), ("填帧", r"填帧|\bfill\b|帧模板"), ("TSC 读取", r"tsc_now")]
MSG_CLASS = [("bbo", r"BBO|bookTicker|bbo-tbt|books1\b|→ `Bbo`"), ("topn", r"分档|TopN|books\d+|depth20|20 档"),
             ("incr", r"增量|L2Batch|depthUpdate"), ("ignore", r"Ignorable|可忽略|未知事件|Unknown"),
             ("margin", r"保证金|MarginReport|account 推送"), ("positions", r"positions|持仓"),
             ("trade", r"成交|ORDER_TRADE_UPDATE|orders 推送|订单推送"), ("snapshot", r"decode_snapshot|1000 档"),
             ("acct", r"acct_decode")]
RX_BV = re.compile(r"(p\d+(?:\.\d+)?)?\s*[::]?\s*(<|≤|<=)\s*(\d+(?:\.\d+)?)\s*(ns|µs|us|ms)(\s*/\s*[条档])?")
RX_SKIPROW = re.compile(r"warm|可选|optional|不在热路径|命令路径|冷路径|不在 tick-to-trade|snap-fetch|acct-fetch"
                        r"|acct_decode|decode_snapshot")


def msg_class(t):
    return next((c for c, rx in MSG_CLASS if re.search(rx, t)), None)


def bound_cases(cell):
    """[(label or '', [(pct, value_ns, per)])] — 'case: p50 ≤ … 、p99 ≤ …; case2: …'."""
    out = []
    for part in re.split(r"[;;]", re.sub(r"[((][^))]*分配[^))]*[))]", "", cell)):
        lab = ""
        lm = re.match(r"\s*([^::]{1,40})[::]\s*p\d", part)
        if lm:
            lab = lm.group(1)
        vals = [(m.group(1), float(m.group(3)) * UNIT_NS[m.group(4)], (m.group(5) or "").strip().lstrip("/").strip())
                for m in RX_BV.finditer(part)]
        if vals:
            out.append((lab, vals))
    return out


def crate_budget_rows(p):
    """[(line, first cell, [(pct or None, value_ns)], whole row)] of the tables in 延迟与资源契约."""
    s = texts.get(p, "")
    sec = re.search(r"\n## 延迟与资源契约\n(.*?)(?=\n## |\Z)", s, re.S)
    if not sec:
        return []
    off = s[:sec.start(1)].count("\n")
    tmp = p + "\x00budget"
    texts[tmp] = sec.group(1)
    rows_out = []
    for hl, hdr, rows in md_tables(tmp):
        labels = [re.findall(r"p\d+(?:\.\d+)?", c) for c in hdr]
        for ln, cells in rows:
            vals = []
            for k, c in enumerate(cells[1:], 1):
                hl_ = labels[k] if k < len(labels) else []
                for j, m in enumerate(RX_BV.finditer(c)):
                    pct = m.group(1) or (hl_[j] if j < len(hl_) else (hl_[0] if len(hl_) == 1 else None))
                    vals.append((pct, float(m.group(3)) * UNIT_NS[m.group(4)]))
            if vals:
                rows_out.append((ln + off, cells[0], vals, " ".join(cells)))
    del texts[tmp]
    _tables_cache.pop(tmp, None)
    return rows_out


def pct_rank(pc):
    return float(pc[1:]) if pc else -1


budget_n = 0
bounds = []          # (where, entry regex, class or None, case label, bvals, [crate files], description)
for hl, hdr, rows in md_tables(BENCH_MD):
    if not (hdr and hdr[0] == "原语" and any("上限" in c for c in hdr)):
        continue
    bcol = next(k for k, c in enumerate(hdr) if "上限" in c)
    for ln, cells in rows:
        prim = cells[0]
        crates_ = [os.path.normpath(os.path.join(os.path.dirname(BENCH_MD), t))
                   for t in re.findall(r"\]\(([^)#\s]+README\.md)\)", cells[1])]
        if "等" in cells[1]:
            fams = {re.sub(r"-[^-]+$", "-", os.path.basename(os.path.dirname(c))) for c in crates_}
            crates_ += [hft(f"crates/{c}/README.md") for c in CRATES if any(c.startswith(f) for f in fams)
                        or ("venue-" in fams and c == "hft-venue-proto")]   # the generic venue contract
        crates_ = [c for c in dict.fromkeys(crates_) if c in texts and not is_frozen(c)]
        bcell = cells[bcol] if bcol < len(cells) else ""
        dm = re.search(r"\]\(([^)#\s]+README\.md)#延迟与资源契约\)", bcell)
        if dm and not RX_BV.search(bcell):              # delegated: the linked crate's table rows are the bounds
            home = os.path.normpath(os.path.join(os.path.dirname(BENCH_MD), dm.group(1)))
            for hln, first, hvals, whole in crate_budget_rows(home):
                ent = next((t for t in RX_BT.findall(first) if re.fullmatch(r"[a-z_]+", t)), None)
                if not ent or RX_SKIPROW.search(whole):
                    continue
                bounds.append((f"{rroot(home)}:{hln}", r"`" + re.escape(ent) + r"`", msg_class(first), "",
                               [(pc, v, "") for pc, v in hvals], [c for c in crates_ if c != home], first[:40]))
            continue
        kw = next((rx for word, rx in PRIM_KW if word in prim), None)
        if not kw:
            continue
        for lab, bvals in bound_cases(bcell):
            bounds.append((f"benchmarks/README.md:{ln}", kw, msg_class(prim), lab, bvals, crates_, prim[:30]))
for where, ent_rx, cls, lab, bvals, crates_, desc in bounds:
    lab_kw = re.findall(r"[一-鿿]{2,}|[A-Za-z]{3,}", lab)[:1]
    for cp in crates_:
        for cl, first, cvals, whole in crate_budget_rows(cp):
            if not re.search(ent_rx, first) or RX_SKIPROW.search(whole):
                continue
            if cls != msg_class(first) and (cls or where.startswith(("crates/",))):
                continue
            if lab_kw and lab_kw[0] not in first:
                continue
            budget_n += 1
            nlev = re.search(r"(\d+)\s*档", first)
            lowest = min((pct_rank(pc) for pc, _ in cvals), default=-1)
            bmax = max(v for _, v, _ in bvals)
            for bp, bv, per in bvals:
                for cp_, cv in cvals:
                    if bp and cp_ and bp != cp_:
                        continue
                    if bp and not cp_ and bv < bmax:
                        continue
                    if per == "档":
                        if not nlev and "delta" not in first:
                            continue
                        cv = cv / int(nlev.group(1)) if nlev else cv
                    if cv <= bv + 1e-9:
                        continue
                    cat = "BUDGETPCT" if (not bp and cp_ and pct_rank(cp_) > lowest) else "BUDGET"
                    add(cat, cp, f"{rroot(cp)}:{cl}: {first[:50]} {cp_ or ''} {cv:g} ns > {where} bound "
                                 f"{bp or '(no percentile)'} {bv:g} ns{'/' + per if per else ''} ({desc}{'; ' + lab if lab else ''})")

# ---------- ⑫ BRIEFDRIFT[W]: the brief (and merge-fixes) vs the spec — paths, ADR files, signatures, typed fields,
# parameter values, "X 保持 T,与 `Y` 一致" type claims
briefdrift_n = 0
ALL_SPEC = "\n".join(texts[p] for p, r in spec_items())


def spec_fn_arity(name):
    out = set()
    for m in re.finditer(r"\bfn\s+" + re.escape(name) + r"\s*(?:<[^>]*>)?\s*\(([^)]*)\)", ALL_SPEC):
        ps = [x for x in split_top(m.group(1)) if x.strip() and not re.match(r"\s*&?\s*(?:mut\s+)?(?:'\w+\s+)?self\b", x)]
        out.add(len(ps))
    return out


def spec_types(name):
    out = set()
    for sd in struct_defs:
        for fd in sd.get("fields", []):
            if fd["name"] == name:
                out.add(re.sub(r"\s+", "", fd["type"]))
    for p, r in spec_items():
        for hl, hdr, rows in md_tables(p):
            if len(hdr) < 2 or not re.search(r"类型|type|单位", hdr[1], re.I):
                continue
            for ln, cells in rows:
                if len(cells) > 1 and name in [k.split(".")[-1] for k in cfg_keys(cells[0])]:
                    out.add(cells[1].strip("`").strip())
    return out


def spec_defaults(name):
    out = set()
    for (ns, k), defs in cfg_defs.items():
        if k == name or k.endswith("." + name) or name.endswith("." + k.split(".")[-1]) and k.split(".")[-1] == name.split(".")[-1]:
            for f, ln, v in defs:
                m = re.match(r"\s*([\d][\d _,]*)", v.replace("`", ""))
                if m:
                    out.add(int(re.sub(r"[ _,]", "", m.group(1))))
    return out


def brief_files():
    out = []
    for f in (BRIEF, FIXES):
        if f and os.path.exists(f):
            out.append((os.path.basename(f), read(f)))
    return out


NOTES_DIR = hft(".agents/notes")
for bname, bs in brief_files():
    for i, line in enumerate(bs.splitlines(), 1):
        if re.search(r"改名为|→\s*`|旧|取代|不再|删除|作废", line) and not re.search(r"^\|", line):
            old_tokens = set(re.findall(r"`([^`]+)`\s*(?:改名为|→)", line))
        else:
            old_tokens = set()
        for t in RX_BT.findall(line):
            if t in old_tokens:
                continue
            # absolute paths
            if re.match(r"/(?:var/lib|run|etc|opt|dev)/", t):
                briefdrift_n += 1
                if t.rstrip("…/") not in ALL_SPEC:
                    add("BRIEFDRIFT", None, f"{bname}:{i}: path `{t}` appears nowhere in the spec")
                continue
            # ADR / repo files
            if re.fullmatch(r"(?:architecture|process)/[\w.-]+\.md", t):
                briefdrift_n += 1
                hit = any(f == os.path.basename(t) for dp, dn, fn in os.walk(NOTES_DIR) for f in fn) \
                    if os.path.isdir(NOTES_DIR) else False
                if not hit:
                    add("BRIEFDRIFT", None, f"{bname}:{i}: ADR `{t}` not found under .agents/notes/")
                continue
            # signatures name(a, b) / obj.name()
            m = re.fullmatch(r"(?:\w+\.)?([a-z_][a-z0-9_]*)\(([^()]*)\)", t)
            if m and not m.group(2).strip():
                continue                      # name() is a reference, not a signature
            if m:
                ar = spec_fn_arity(m.group(1))
                if ar:
                    briefdrift_n += 1
                    n = len([x for x in m.group(2).split(",") if x.strip() and x.strip() not in ("…", "...")])
                    if n not in ar and not ("…" in m.group(2) or "..." in m.group(2)):
                        add("BRIEFDRIFT", None, f"{bname}:{i}: `{t}` has {n} argument(s); the spec's fn {m.group(1)} "
                                                f"takes {'/'.join(map(str, sorted(ar)))}")
                continue
            # typed fields  name: type
            m = re.fullmatch(r"([a-z_]\w*)\s*:\s*([\w\[\]; ]+)", t)
            if m:
                ts = spec_types(m.group(1))
                if ts:
                    briefdrift_n += 1
                    if re.sub(r"\s+", "", m.group(2)) not in ts:
                        add("BRIEFDRIFT", None, f"{bname}:{i}: `{t}` but the spec types {m.group(1)} as "
                                                f"{', '.join(sorted(ts))}")
                continue
        # parameter values: `key`(300 000 …) / `key` 300000
        for m in re.finditer(r"`([a-z_][\w.]*)`\s*[((]?\s*(\d[\d ,_]*\d|\d)(?=\s*[,,))]|\s*$|\s*\|)", line):
            ds = spec_defaults(m.group(1))
            if ds:
                briefdrift_n += 1
                v = int(re.sub(r"[ ,_]", "", m.group(2)))
                if v not in ds:
                    add("BRIEFDRIFT", None, f"{bname}:{i}: `{m.group(1)}` = {v} but the spec default is "
                                            f"{', '.join(map(str, sorted(ds)))}")
        # "`A` 保持 T … 与 … `B` 一致": B's spec type must be T
        m = re.search(r"`(\w+)`\s*(?:保持|为|是)\s*`?(u8|u16|u32|u64|i64|Rate|Px|Qty|Notional)`?[^。]*?与[^。]*?`(\w+)`\s*一致", line)
        if m:
            ts = spec_types(m.group(3))
            if ts:
                briefdrift_n += 1
                if m.group(2) not in ts:
                    add("BRIEFDRIFT", None, f"{bname}:{i}: says `{m.group(1)}` {m.group(2)} 与 `{m.group(3)}` 一致, but the "
                                            f"spec types {m.group(3)} as {', '.join(sorted(ts))}")

# ---------- FROZENREF[W]: a backticked name in a frozen README that the non-frozen spec no longer uses but the --prev
# tree's non-frozen spec did (renamed or removed elsewhere; frozen READMEs only fix such references)
frozenref_n = 0
if prev_ok:
    prev_live = "\n".join(s for p, s in prev_texts.items()
                          if not any(os.path.relpath(p, PREV).replace(os.sep, "/").startswith(
                              os.path.relpath(d, ROOT).replace(os.sep, "/") + "/") for d in FROZEN_DIRS)
                          and not os.path.relpath(p, PREV).replace(os.sep, "/").startswith(".agents/"))
    cur_live = "\n".join(s for p, s in texts.items() if in_spec(p) and not is_frozen(p)
                         and not rroot(p).startswith(".agents/"))
    for p, s in texts.items():
        if not (in_spec(p) and is_frozen(p) and p.endswith("README.md")):
            continue
        seen = set()
        for i, line in enumerate(s.splitlines(), 1):
            for t in RX_BT.findall(line):
                t = t.strip()
                if len(t) < 5 or t in seen or not re.search(r"[a-z]", t) or " " in t:
                    continue
                seen.add(t)
                frozenref_n += 1
                if t not in cur_live and t in prev_live:
                    add_w = warnings["FROZENREF"]
                    add_w.append(f"{rroot(p)}:{i}: `{t}` no longer appears in the non-frozen spec (it did in --prev)")

# ---------- report
present = [r for r in EXPECTED if os.path.exists(os.path.join(ROOT, r))]
sizes = sorted((os.path.getsize(os.path.join(ROOT, r)), r) for r in present)
print(f"files expected={len(EXPECTED)} present={len(present)} md scanned={len(md_files)} links checked={nlinks}")
print("smallest:", ", ".join(f"{r}={n}" for n, r in sizes[:8]))
print(f"frozen: {len(FROZEN_DIRS)} components ({', '.join(rroot(d) for d in FROZEN_DIRS)}); content hits skipped: "
      + (", ".join(f"{c}={n}" for c, n in sorted(suppressed.items())) or "none"))
print("length:", ", ".join(length_info))
print(f"layout: structs with declared size checked={layout_checked}, skipped (unresolved field types)={layout_skipped};"
      f" code-like integer fields checked={codeval_checked}; Note/Timer registry names={len(REG)}")
print(f"round8e: ids registered={len(ID_REG)} refs={id_ref_n}; paths registered={len(PATH_REG)} refs={path_ref_n};"
      f" s3 refs={s3_ref_n}; archiver prefixes={sorted(ARCH_PREFIX)} cred={sorted(ARCH_CRED)}; config tables="
      f"{cfg_tables_n} keys with default={len(cfg_defs)} hft-core keys={len(CORE_KEYS)} refs={cfg_ref_n};"
      f" cli signatures={sum(len(v) for v in CLI_SIGS.values())} uses={cli_use_n}; rings={len(ring_src)}"
      f" capacity rows={len(ring_cap)} journals={len(journals)}; repr(C) padding checked={pad_checked}"
      f" skipped={pad_skipped}; size restatements={size_copy_n}; enums checked={enum_checked}; Note producers="
      f"{note_src_n}; benchmarks={len(bench_defs)}; owner links={link_tgt_n}; brief names={brief_names_n}")
print(f"round8f: units={len(DEP_UNITS)} users={len(DEP_USERS)} groups={len(DEP_GROUPS)} path entries={len(PATHS)}"
      f" access claims={perm_n} created files={create_n} membership checks={groupmem_n}; s3 keys={s3key_n};"
      f" iam grants={sorted(IAM_DEP)} checks={iam_n} kms prefixes={len(kms_pfx)}; inequalities={unitform_n};"
      f" wire codes={wirecode_n}; verbatim embeds={verbatim_n}; append-only structs={appendonly_n}"
      f" (prev={'yes' if prev_ok else 'no'}); all-host rules={allhosts_n}; lists={listcopy_n}; budget rows={budget_n};"
      f" brief items={briefdrift_n}; frozen refs={frozenref_n}")
print("round8e: unverified markers: " + ", ".join(f"{k}={v}" for k, v in sorted(tbd_count.items(), key=lambda x: -x[1])))
ORDER = ["MISSING", "NOFM", "FMKEY", "SCOPE", "STATUS", "PHASEKEY", "NOSEC", "ORDER", "TABLE", "BADLINK", "BADANCHOR",
         "STRATLINK", "STRATREF", "STRATNAME", "VENUENAME", "CANARY", "WEEK", "PHASE", "OLDTERM",
         "CDEXCORE", "BALDOMAIN", "ACKCMD", "GROUPALL0", "MANUALCMD", "OLDPRIO", "SCHEMAEXP", "ONEINST",
         "SMALLLIVE", "SIMINPROD", "PERPLEV", "MIDAGE", "QUOTEAGE", "KILLU8", "REGDEP", "PYSTRAT",
         "HISTORY", "SYNCQ", "LENGTH", "LAYOUT", "NOTEREG", "RESID8D",
         "IDREG", "PATHREG", "S3CRED", "CFGDUP", "CFGPATH", "CLIFLAG", "RINGCAP", "RINGHOME", "JRNLMEM", "PAD",
         "SIZECOPY", "ENUMVAL", "NOTESRC", "BENCHDUP", "BENCHOWN", "BRIEFNAME",
         "PERM", "PERMRW", "GROUPMEM", "S3KEY", "IAMSET", "UNITFORM", "WIRECODE", "VERBATIM", "APPENDONLY",
         "ALLHOSTS", "BUDGET"]
WORDER = ["HISTWEAK", "SYNCREG", "CODEVAL", "IMPLPAD",
          "IDOPT", "S3REF", "CFGREF", "CLIREQ", "CLISUB", "BENCHCOPY", "LINKTGT", "TBDFMT",
          "PERMOPT", "PERMIMP", "UNITMIX", "LISTCOPY", "BUDGETPCT", "BRIEFDRIFT", "FROZENREF"]


def dump(d, order):
    total = 0
    for cat in order:
        items = d.get(cat, [])
        total += len(items)
        if not items:
            continue
        print(f"\n[{cat}] {len(items)}")
        for x in items[:LIMIT]:
            print(f"  {x}")
        if len(items) > LIMIT:
            print(f"  ... {len(items) - LIMIT} more")
    return total


total = dump(problems, ORDER)
print("\n-- warnings (heuristic; not counted in problems) --")
wtotal = dump(warnings, WORDER)
print("\nsummary: " + ", ".join(f"{c}={len(problems.get(c, []))}" for c in ORDER))
print("warnings: " + ", ".join(f"{c}={len(warnings.get(c, []))}" for c in WORDER))
print(f"problems={total} warnings={wtotal}")
