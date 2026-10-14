---
component: tools
prefix: TOOL
spec: README.md
---

# tools 测试规格

格式与规则见 [docs/tests](../docs/tests/README.md)。

SDK-004 的装配模板检查包含必填线程 nice 及 housekeeping 优先级，字段契约归 hft-core。SDK-005 的记录型执行器测试在 `xtask/tests/scaffold_gate.rs`；实际隔离副本进程测试模板在 `xtask/scaffold/sdk005.rs.tmpl`，CI Python 作业通过 `--generic-tools` 执行。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | TOOL-001–TOOL-002 |
| cargo xtask ci | TOOL-003–TOOL-011, TOOL-186–TOOL-191、TOOL-206–TOOL-207 |
| cargo xtask decoupling-gate | TOOL-012–TOOL-019 |
| cargo xtask scaffold-gate | TOOL-020–TOOL-029 |
| cargo xtask deps | TOOL-030–TOOL-031、TOOL-118 |
| cargo xtask hot-check | TOOL-032 |
| cargo xtask bench-gate | TOOL-033–TOOL-038、TOOL-185 |
| cargo xtask replay-verify | TOOL-039–TOOL-046、TOOL-192、TOOL-198–TOOL-204 |
| cargo xtask dst | TOOL-009、TOOL-047–TOOL-056、TOOL-205 |
| cargo xtask gen-schema | TOOL-057–TOOL-061、TOOL-179 |
| cargo xtask gen-host | TOOL-062–TOOL-065 |
| cargo xtask doc-check | TOOL-066–TOOL-068、TOOL-178 |
| cargo xtask new-strategy | TOOL-069–TOOL-071 |
| 其他 xtask 子命令 | TOOL-072–TOOL-091、TOOL-180、TOOL-193–TOOL-204 |
| az-probe | TOOL-092–TOOL-099 |
| hostprobe | TOOL-100–TOOL-113 |
| hft-secrets | TOOL-114–TOOL-118、TOOL-181 |
| hft-cfg | TOOL-119–TOOL-144、TOOL-182 |
| hft-verify | TOOL-145–TOOL-172、TOOL-178、TOOL-183–TOOL-184 |
| 实现说明 | TOOL-067、TOOL-103、TOOL-143、TOOL-163、TOOL-180 |
| 延迟与资源契约 | TOOL-099、TOOL-173–TOOL-177 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | TOOL-031、TOOL-118 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

约定:xtask 的用例在小型假仓库上运行(每个夹具一个最小 workspace + 文档树),夹具放 `tools/xtask/tests/fixtures/<名字>/`,或由测试代码按名字在临时目录中生成(如 `trace-clean`,见 `tools/xtask/tests/test_trace.rs`);外部命令(`cargo`、`git`、`gh`、`hft-core`、`bpftrace`)经 xtask 内部的命令执行器调用,测试替换为**记录型执行器**(记下每次调用的程序名、参数与设置的环境变量,按脚本返回退出码与输出,必要时写出脚本给定的文件以扮演产出文件的程序)。"退出码"指子命令进程的退出码;"JSON"指 `target/xtask/<subcommand>.json`。

### TOOL-001 每个 xtask 子命令按 0/1/2 约定退出

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M1
- 前置:夹具 `clean`(无违规的最小仓库)与各行指定的夹具;记录型执行器对外部命令一律返回 0(TOOL-001.7 例外:在只有一个提交的真实 git 仓库上运行,不替换执行器)
- 实现:`tools/xtask/tests/exit_codes.rs::tool_001_exit_code_convention`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-001.1 | `cargo xtask deps`,夹具 `clean` | 退出 0 |
| TOOL-001.2 | `cargo xtask deps`,夹具 `reverse-dep`(`hft-types` 依赖 `hft-refdata`) | 退出 1 |
| TOOL-001.3 | `cargo xtask deps`,夹具 `clean` 删去 `tools/xtask/deps.toml` | 退出 2 |
| TOOL-001.4 | `cargo xtask hot-check`,夹具 `dyn-in-hot` | 退出 1 |
| TOOL-001.5 | `cargo xtask doc-check`,夹具 `bad-anchor` | 退出 1 |
| TOOL-001.6 | `cargo xtask journal-verify /nonexistent` | 退出 2 |
| TOOL-001.7 | `cargo xtask decoupling-gate --base 0000000000000000000000000000000000000000 --head HEAD` | 退出 2 |
| TOOL-001.8 | `cargo xtask test-trace`,夹具 `clean` 删去 `tools/xtask/test-trace.toml` | 退出 2 |
| TOOL-001.9 | `cargo xtask gen-host --check --host nosuch` | 退出 2 |
| TOOL-001.10 | `cargo xtask replay-verify fixtures/replay/x.pointer.toml`,缓存文件 SHA-256 与指针不符 | 退出 2 |

### TOOL-002 子命令把机器可读结果写到 target/xtask 并把摘要写到 stderr

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M1
- 前置:夹具 `reverse-dep`;`target/xtask/` 不存在
- 步骤:
  1. 运行 `cargo xtask deps`
- 期望:
  - 退出 1;`target/xtask/deps.json` 存在且能解析为 JSON,其中违规列表恰有 1 项,字段含 `from = "hft-types"`、`to = "hft-refdata"`
  - stderr 至少一行含 `hft-types` 与 `hft-refdata`;stdout 为空
- 实现:`tools/xtask/tests/exit_codes.rs::tool_002_json_and_stderr_summary`

### TOOL-003 ci 按固定顺序执行且第一步失败即停

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)
- 级别:unit
- 里程碑:M1
- 前置:记录型执行器:`deps` 步返回 1,其余返回 0
- 步骤:
  1. 运行 `cargo xtask ci`
- 期望:
  - 退出 1;执行器记录的步骤序列恰为 `fmt --check`、`clippy --workspace --all-targets -D warnings`、`deny check licenses bans`、`deps`
  - 记录中没有 `hot-check` 及其后的任何步骤
- 实现:`tools/xtask/tests/ci.rs::tool_003_stops_at_first_failure`

### TOOL-004 ci 全部通过时依次执行规定的十一步

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)
- 级别:unit
- 里程碑:M1
- 前置:记录型执行器对每步返回 0
- 步骤:
  1. 运行 `cargo xtask ci`
- 期望:
  - 退出 0;步骤序列恰为:`fmt --check` → `clippy …` → `deny check licenses bans` → `deps` → `hot-check` → `fixtures check` → 单元/属性(`cargo test --profile ci`)→ 金样 → `dst` 冒烟 → `gen-schema --check` → `doc-check`
- 实现:`tools/xtask/tests/ci.rs::tool_004_runs_eleven_steps_in_order`

DST 冒烟步含基线、LED-080 与 LED-081 三次调用,三者均使用 200 个提交派生种子加完整回归集。HashTrail 金样核对步已移除(负责人决定 2026-10-14)。

### TOOL-005 ci --pure-only 只选 tier = pure 的 crate 与 Python 包

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)
- 级别:unit
- 里程碑:M1
- 前置:夹具 `tiers`:crate `p1`(`tier: pure`)、`h1`(`tier: hot`)、`c1`(`tier: cold`),Python 包 `py1`(`tier: pure`)、`py2`(`tier: cold`)
- 步骤:
  1. 运行 `cargo xtask ci --pure-only`
- 期望:
  - 单元/属性步的 `cargo test` 参数中 `-p` 只有 `p1`;Python 测试步只含 `py1`
  - 记录中没有对 `h1`、`c1`、`py2` 的任何测试调用
- 实现:`tools/xtask/tests/ci.rs::tool_005_pure_only_selects_pure`

### TOOL-006 ci --fast 跳过 DST 与金样

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)
- 级别:unit
- 里程碑:M1
- 前置:记录型执行器对每步返回 0
- 步骤:
  1. 运行 `cargo xtask ci --fast`
- 期望:
  - 退出 0;步骤序列中没有金样步与 `dst` 冒烟;其余九步顺序同 TOOL-004
- 实现:`tools/xtask/tests/ci.rs::tool_006_fast_skips_dst_and_golden`

### TOOL-007 ci 从不运行 cargo deny 的 advisories

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)
- 级别:unit
- 里程碑:M1
- 前置:夹具 `clean`,`Cargo.lock` 相对上一提交有改动
- 步骤:
  1. 运行 `cargo xtask ci`
- 期望:
  - 记录中 `cargo deny` 恰被调用一次,参数为 `check licenses bans`;任何调用的参数都不含 `advisories`
- 实现:`tools/xtask/tests/ci.rs::tool_007_never_runs_advisories`

### TOOL-008 ci 不运行按路径触发的步骤与 decoupling-gate

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)
- 级别:unit
- 里程碑:M1
- 前置:夹具 `clean`,工作区改动含 `crates/hft-oms/src/lib.rs`、`crates/venue-okx/src/decode.rs`、`benchmarks/README.md`
- 步骤:
  1. 运行 `cargo xtask ci`
- 期望:
  - 记录中没有 `scaffold-gate`、`fuzz`、`replay-verify`、`bench-gate`、`decoupling-gate` 的调用
- 实现:`tools/xtask/tests/ci.rs::tool_008_no_path_triggered_steps`

### TOOL-009 ci 的 DST 冒烟以 200 个提交派生种子加回归集运行

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)、[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:unit
- 里程碑:M2
- 前置:记录型执行器对每步返回 0;HEAD 提交 SHA 固定为 `a1b2…`(40 位)
- 步骤:
  1. 运行 `cargo xtask ci`
- 期望:
  - `dst` 冒烟步依次调用基线 `--seeds 200 --regressions` 以及同参数分别加 `--case LED-080`、`--case LED-081` 的专属场景,三次均不含 `--seed`;紧随其后的一步为 `gen-schema --check`(没有 HashTrail 核对)
- 实现:`tools/xtask/tests/ci.rs::tool_009_dst_smoke_args`

### TOOL-010 ci 在 optional 组件不存在时照常通过

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)
- 级别:unit
- 里程碑:M1
- 前置:`tools/xtask/scope.toml` 登记 optional 组件 `hft-amm`;workspace 中没有 `crates/hft-amm`
- 步骤:
  1. 运行 `cargo xtask ci`
- 期望:
  - 退出 0;stderr 与 JSON 中没有含 `hft-amm` 的错误项;单元/属性步的 `-p` 列表不含 `hft-amm`
- 实现:`tools/xtask/tests/ci.rs::tool_010_optional_absent_passes`

### TOOL-011 ci 在 optional 组件存在时纳入它

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)
- 级别:unit
- 里程碑:M1
- 前置:同 TOOL-010,但 workspace 含 `crates/hft-amm`
- 步骤:
  1. 运行 `cargo xtask ci`
- 期望:
  - 单元/属性步的 `cargo test` 覆盖 `hft-amm`(`-p hft-amm` 出现在参数中,或以 `--workspace` 运行且未排除它)
- 实现:`tools/xtask/tests/ci.rs::tool_011_optional_present_included`

### TOOL-012 decoupling-gate 按改动路径判定策略与引擎同改

- 规则:[cargo xtask decoupling-gate](README.md#cargo-xtask-decoupling-gate)
- 级别:table
- 里程碑:M1
- 前置:不带 `--pr`(无豁免);`deps.toml` 的分层表为夹具 `layers` 的版本;记录型执行器按各行返回 `git diff --name-only base..head` 的文件清单
- 实现:`tools/xtask/tests/decoupling_gate.rs::tool_012_classify_diff`

| 行 | diff 文件清单 | 期望 |
|---|---|---|
| TOOL-012.1 | `strategies/x/src/lib.rs` | 退出 0 |
| TOOL-012.2 | `strategies/x/src/lib.rs`、`crates/hft-oms/src/lib.rs` | 退出 1;JSON 的 `strategies` = [`strategies/x/src/lib.rs`],`engine` = [`crates/hft-oms/src/lib.rs`] |
| TOOL-012.3 | `strategies/registry/src/lib.rs`、`apps/hft-core/src/main.rs` | 退出 1 |
| TOOL-012.4 | `strategies/x/pipeline/x_plugin/plugin.py`、`apps/watchtower/src/main.rs` | 退出 1 |
| TOOL-012.5 | `strategies/x/src/lib.rs`、`crates/hft-testkit/src/lib.rs` | 退出 1 |
| TOOL-012.6 | `crates/hft-oms/src/lib.rs` | 退出 0 |
| TOOL-012.7 | `strategies/x/README.md`、`tools/xtask/src/deps.rs` | 退出 0 |
| TOOL-012.8 | `strategies/x/src/lib.rs`、`docs/testing.md` | 退出 0 |
| TOOL-012.9 | `strategies/x/src/lib.rs`、`python/pipeline/README.md` | 退出 0 |

### TOOL-013 engine-change 标签加 Admin 批准评审豁免并写出摘要

- 规则:[cargo xtask decoupling-gate](README.md#cargo-xtask-decoupling-gate)
- 级别:unit
- 里程碑:M1
- 前置:diff 同 TOOL-012.2;`--pr 17`;PR 17 的标签含 `engine-change`;评审列表含 `{user: "owner1", state: "APPROVED"}`;`.github/hft-admins.txt` 含一行 `owner1`;`GITHUB_STEP_SUMMARY` 指向临时文件
- 步骤:
  1. 运行 `cargo xtask decoupling-gate --base B --head H --pr 17`
- 期望:
  - 退出 0
  - JSON 的 `exemption` = `{pr: 17, approver: "owner1", engine: ["crates/hft-oms"]}`(另含 app 列表 `[]`)
  - 摘要文件含 `17`、`owner1`、`hft-oms`
- 实现:`tools/xtask/tests/decoupling_gate.rs::tool_013_admin_exemption`

### TOOL-014 只有 engine-change 标签没有评审仍失败

- 规则:[cargo xtask decoupling-gate](README.md#cargo-xtask-decoupling-gate)
- 级别:unit
- 里程碑:M1
- 前置:同 TOOL-013,但评审列表为空
- 步骤:
  1. 运行 `cargo xtask decoupling-gate --base B --head H --pr 17`
- 期望:
  - 退出 1;JSON 没有 `exemption`;摘要文件不含豁免条目
- 实现:`tools/xtask/tests/decoupling_gate.rs::tool_014_label_without_review_fails`

### TOOL-015 批准人不在 Admin 名单时不豁免

- 规则:[cargo xtask decoupling-gate](README.md#cargo-xtask-decoupling-gate)
- 级别:unit
- 里程碑:M1
- 前置:同 TOOL-013,但批准评审的 `user = "dev2user"`,`hft-admins.txt` 只含 `owner1`
- 步骤:
  1. 运行 `cargo xtask decoupling-gate --base B --head H --pr 17`
- 期望:
  - 退出 1;stderr 含 `dev2user` 与"不在 Admin 名单"的说明;JSON 没有 `exemption`
- 实现:`tools/xtask/tests/decoupling_gate.rs::tool_015_non_admin_approval_fails`

### TOOL-016 Admin 批准而没有 engine-change 标签时不豁免

- 规则:[cargo xtask decoupling-gate](README.md#cargo-xtask-decoupling-gate)
- 级别:unit
- 里程碑:M1
- 前置:同 TOOL-013,但 PR 17 的标签为空
- 步骤:
  1. 运行 `cargo xtask decoupling-gate --base B --head H --pr 17`
- 期望:
  - 退出 1;JSON 没有 `exemption`
- 实现:`tools/xtask/tests/decoupling_gate.rs::tool_016_approval_without_label_fails`

### TOOL-017 Admin 只留评论(非批准)时不豁免

- 规则:[cargo xtask decoupling-gate](README.md#cargo-xtask-decoupling-gate)
- 级别:unit
- 里程碑:M1
- 前置:同 TOOL-013,但 `owner1` 的评审 `state = "COMMENTED"`
- 步骤:
  1. 运行 `cargo xtask decoupling-gate --base B --head H --pr 17`
- 期望:
  - 退出 1;JSON 没有 `exemption`
- 实现:`tools/xtask/tests/decoupling_gate.rs::tool_017_comment_review_fails`

### TOOL-018 读不到 PR 评审时退出 2

- 规则:[cargo xtask decoupling-gate](README.md#cargo-xtask-decoupling-gate)
- 级别:table
- 里程碑:M1
- 前置:diff 同 TOOL-012.2;`--pr 17`
- 实现:`tools/xtask/tests/decoupling_gate.rs::tool_018_reviews_unreadable`

| 行 | 环境 | 期望 |
|---|---|---|
| TOOL-018.1 | 未设置 `GITHUB_TOKEN` | 退出 2;stderr 指明缺 token |
| TOOL-018.2 | 评审 API 返回 HTTP 500 | 退出 2 |
| TOOL-018.3 | 评审 API 返回 HTTP 404(PR 不存在) | 退出 2 |
| TOOL-018.4 | `--head` 指向不存在的提交 | 退出 2 |

### TOOL-019 decoupling-gate 不构建也不测试

- 规则:[cargo xtask decoupling-gate](README.md#cargo-xtask-decoupling-gate)
- 级别:unit
- 里程碑:M1
- 前置:diff 同 TOOL-012.2;豁免条件同 TOOL-013
- 步骤:
  1. 运行 `cargo xtask decoupling-gate --base B --head H --pr 17`
- 期望:
  - 记录型执行器中只有 `git` 与 GitHub API 调用;没有任何 `cargo build`、`cargo test`、`cargo check` 调用
- 实现:`tools/xtask/tests/decoupling_gate.rs::tool_019_never_builds`

### TOOL-020 scaffold-gate 以模板生成 NoModel 与 pipeline 插件的测试段探针

- 规则:[cargo xtask scaffold-gate](README.md#cargo-xtask-scaffold-gate)
- 级别:unit
- 里程碑:M2
- 前置:干净仓库;`--keep`;记录型执行器对构建、测试与 `hft-core --check` 返回 0
- 步骤:
  1. 运行 `cargo xtask scaffold-gate --keep`
- 期望:
  - `target/decoupling/strategies/ci-probe/` 存在,含 `src/lib.rs`、`type Model = NoModel<…>`、`tests/scenario.rs`、`pipeline/` 目录;不生成 Model 模块
  - 生成时调用的 `new-strategy` 参数含 `ci-probe`、`--with-pipeline`,不含 `--with-model`,`--kind-id` 的值 ∈ `0xFF00..=0xFFFF`
- 实现:`tools/xtask/tests/scaffold_gate.rs::tool_020_probe_generated_in_test_range`

### TOOL-021 scaffold-gate 在临时 registry 副本末尾加一行并以 patch 构建,复用主 target 目录

- 规则:[cargo xtask scaffold-gate](README.md#cargo-xtask-scaffold-gate)
- 级别:unit
- 里程碑:M2
- 前置:同 TOOL-020;主 target 目录为 `/w/target`
- 步骤:
  1. 运行 `cargo xtask scaffold-gate --keep`
- 期望:
  - `target/decoupling/registry/src/lib.rs` 的 `strategy_set!` 调用最后一项是 `CiProbe`,其余各行与仓库中的 registry 逐字节相同
  - 临时 workspace 清单 `target/decoupling/Cargo.toml` 的 `members` 含 `registry`、`strategies/ci-probe`、`apps/hft-core`、`apps/replayer`,其 `[workspace.dependencies]` 的 `hft-strategy-registry` 指向 `registry`;`apps/replayer` 清单副本中直接写路径的 registry 依赖改为 `../../registry`
  - 每次构建调用的环境 `CARGO_TARGET_DIR = /w/target`;运行结束后 `target/decoupling/` 下没有 `target/` 子目录
- 实现:`tools/xtask/tests/scaffold_gate.rs::tool_021_registry_copy_and_shared_target`

### TOOL-022 scaffold-gate 依次跑探针场景、DST 短集与一次装配检查

- 规则:[cargo xtask scaffold-gate](README.md#cargo-xtask-scaffold-gate)
- 级别:unit
- 里程碑:M2
- 前置:同 TOOL-020
- 步骤:
  1. 运行 `cargo xtask scaffold-gate`
- 期望:
  - 记录的顺序:`ci-probe` 的场景测试 → registry 副本 `tests/` 中含 `CiProbe` 的 DST 短集(100 个种子)→ 配置生成测试(真实四类 blob、SHA-256、生成 kind 与 Sim 账户绑定、零 Model 内存/检查点)→ 带 `bitget` feature 的副本 `hft-core --check --dry-run --config <生成的 sim 实例测试配置>`
  - 生成的测试配置把 `ci-probe` 绑定到一个 `label` 以 `sim-` 开头的 Sim 账户;退出 0
- 实现:`tools/xtask/tests/scaffold_gate.rs::tool_022_scenario_dst_check_order`

### TOOL-023 装配检查非零退出使 scaffold-gate 失败

- 规则:[cargo xtask scaffold-gate](README.md#cargo-xtask-scaffold-gate)
- 级别:unit
- 里程碑:M2
- 前置:同 TOOL-020,但 `hft-core --check` 返回 2
- 步骤:
  1. 运行 `cargo xtask scaffold-gate`
- 期望:
  - 退出 1;stderr 含 `hft-core --check` 与其退出码 2
- 实现:`tools/xtask/tests/scaffold_gate.rs::tool_023_check_failure_fails_gate`

### TOOL-024 探针的 Model 声明超过 sim 实例 advisor 预算时装配检查失败

- 规则:[cargo xtask scaffold-gate](README.md#cargo-xtask-scaffold-gate)
- 级别:integ
- 里程碑:M9
- 前置:模板生成后把 `ci-probe` 的 `Model::mem_bytes` 改为超过生成配置中 `advisor.mem_budget_mib` 的值(配置 `advisor.mem_budget_mib = 64`,探针声明 65 MiB/group × 1 group);真实 `hft-core` 构建
- 步骤:
  1. 运行 `cargo xtask scaffold-gate`
- 期望:
  - `hft-core --check` 退出 2;scaffold-gate 退出 1
- 实现:`tools/xtask/tests/scaffold_gate.rs::tool_024_model_budget_exceeded`

### TOOL-025 scaffold-gate 在引擎侧零改动时通过且不留补丁副本

- 规则:[cargo xtask scaffold-gate](README.md#cargo-xtask-scaffold-gate)
- 级别:unit
- 里程碑:M2
- 前置:干净仓库;不带 `--keep`
- 步骤:
  1. 运行 `cargo xtask scaffold-gate`
- 期望:
  - 退出 0;`git diff --stat -- crates apps tools/hft-cfg python` 输出为空;`git status --porcelain -- strategies/registry` 为空
  - `target/decoupling/` 不存在
- 实现:`tools/xtask/tests/scaffold_gate.rs::tool_025_clean_run_leaves_no_diff`

### TOOL-026 scaffold-gate --keep 保留生成目录

- 规则:[cargo xtask scaffold-gate](README.md#cargo-xtask-scaffold-gate)
- 级别:unit
- 里程碑:M2
- 前置:同 TOOL-025,但带 `--keep`
- 步骤:
  1. 运行 `cargo xtask scaffold-gate --keep`
- 期望:
  - 退出 0;`target/decoupling/strategies/ci-probe/` 与 `target/decoupling/registry/` 存在
- 实现:`tools/xtask/tests/scaffold_gate.rs::tool_026_keep_retains_dir`

### TOOL-027 探针生成过程改动引擎文件时失败并指出该文件

- 规则:[cargo xtask scaffold-gate](README.md#cargo-xtask-scaffold-gate)
- 级别:table
- 里程碑:M2
- 前置:探针生成过程的副作用以运行前改动工作区中各行的文件代替(未提交)
- 实现:`tools/xtask/tests/scaffold_gate.rs::tool_027_engine_diff_fails`

| 行 | 被额外修改的文件 | 期望 |
|---|---|---|
| TOOL-027.1 | `crates/hft-oms/src/lib.rs` | 退出 1;stderr 与 JSON 列出 `crates/hft-oms/src/lib.rs` |
| TOOL-027.2 | `apps/hft-core/src/main.rs` | 退出 1;列出该文件 |
| TOOL-027.3 | `tools/hft-cfg/src/strategy.rs` | 退出 1;列出该文件 |
| TOOL-027.4 | `python/pipeline/hft_pipeline/registry.py` | 退出 1;列出该文件 |
| TOOL-027.5 | `strategies/ci-probe/pipeline/README.md`(探针自身目录) | 退出 0 |

### TOOL-028 探针依赖 hft-oms 时 deps 步使 scaffold-gate 失败

- 规则:[cargo xtask scaffold-gate](README.md#cargo-xtask-scaffold-gate)、[cargo xtask deps](README.md#cargo-xtask-deps)
- 级别:unit
- 里程碑:M2
- 前置:模板的 `Cargo.toml` 额外加 `hft-oms` 依赖
- 步骤:
  1. 运行 `cargo xtask scaffold-gate`
- 期望:
  - 退出 1;stderr 含边 `ci-probe → hft-oms`
- 实现:`tools/xtask/tests/scaffold_gate.rs::tool_028_probe_depends_on_oms`

### TOOL-029 生成的探针不满足 NoModel 或缺 pipeline 插件时失败

- 规则:[cargo xtask scaffold-gate](README.md#cargo-xtask-scaffold-gate)
- 级别:table
- 里程碑:M2
- 前置:临时分支改模板,使生成物缺少各行所列部分
- 实现:`tools/xtask/tests/scaffold_gate.rs::tool_029_probe_shape_required`

| 行 | 生成物缺陷 | 期望 |
|---|---|---|
| TOOL-029.1 | 缺少 `NoModel` 关联类型 | 退出 1;stderr 指明必须使用 NoModel |
| TOOL-029.2 | `pipeline/` 插件包 | 退出 1;stderr 指明缺 pipeline 插件 |

### TOOL-030 deps 在无违规的仓库上通过

- 规则:[cargo xtask deps](README.md#cargo-xtask-deps)
- 级别:unit
- 里程碑:M1
- 前置:夹具 `clean`(全部依赖边都在 `deps.toml` 允许集合内,含 `strategies/registry → strategies/x`、`apps/hft-core → hft-strategy-registry`、`hft-py` 闭包含 `alloy-primitives`)
- 步骤:
  1. 运行 `cargo xtask deps`
- 期望:
  - 退出 0;JSON 违规列表为空
- 实现:`tools/xtask/tests/deps.rs::tool_030_clean_passes`

### TOOL-031 deps 对每条依赖规则的反例失败

- 规则:[cargo xtask deps](README.md#cargo-xtask-deps)、[依赖](README.md#依赖)
- 级别:table
- 里程碑:M1
- 前置:每行在夹具 `clean` 上只加该行的一条依赖(`[dependencies]`,另注明者除外)
- 实现:`tools/xtask/tests/deps.rs::tool_031_rule_counterexamples`

| 行 | 加入的依赖 | 期望 |
|---|---|---|
| TOOL-031.1 | `hft-book → hft-oms` | 退出 1;违规 `hft-book → hft-oms`(不在允许集合) |
| TOOL-031.2 | `hft-types → hft-refdata` | 退出 1(向上依赖) |
| TOOL-031.3 | `hft-oms → x`,`x → tokio` | 退出 1;报告闭包路径 `hft-oms → x → tokio` |
| TOOL-031.4 | `hft-oms →` `async-std` | 退出 1 |
| TOOL-031.5 | `hft-oms →` `mio` | 退出 1 |
| TOOL-031.6 | `hft-oms →` `futures-executor` | 退出 1 |
| TOOL-031.7 | `hft-oms →` `tracing-subscriber` | 退出 1 |
| TOOL-031.8 | `hft-oms →` `serde_json` | 退出 1 |
| TOOL-031.9 | `hft-oms →` `reqwest` | 退出 1 |
| TOOL-031.10 | `hft-oms →` `hyper` | 退出 1 |
| TOOL-031.11 | `hft-oms →` `pyo3` | 退出 1 |
| TOOL-031.12 | `hft-oms →` `rust_decimal` | 退出 1 |
| TOOL-031.13 | `hft-oms →` `bigdecimal` | 退出 1 |
| TOOL-031.14 | `hft-oms →` `num-bigint` | 退出 1 |
| TOOL-031.15 | `hft-types`(pure)`→` `serde_json` | 退出 1 |
| TOOL-031.16 | `venue-binance → hft-net` | 退出 1 |
| TOOL-031.17 | `venue-binance →` `simd-json`(白名单外) | 退出 1 |
| TOOL-031.18 | `venue-binance →` `sonic-rs`、`serde`、`memchr`、`zerocopy`、`ed25519-dalek`、`hmac`、`sha2`、`base64` | 退出 0 |
| TOOL-031.19 | `hft-oms →` `alloy-primitives` | 退出 1 |
| TOOL-031.20 | `apps/chain-core →` `revm` | 退出 0 |
| TOOL-031.21 | `hft-oms → hft-testkit`(`[dependencies]`) | 退出 1 |
| TOOL-031.22 | `hft-oms → hft-testkit`(`[dev-dependencies]`) | 退出 0 |
| TOOL-031.23 | `tools/xtask → hft-testkit` | 退出 0 |
| TOOL-031.24 | `hft-venue → venue-okx` | 退出 1 |
| TOOL-031.25 | `hft-exec → hft-rt` | 退出 1 |
| TOOL-031.26 | `venue-bitget → hft-rt` | 退出 1 |
| TOOL-031.27 | `strategies/x → hft-rt` | 退出 1 |
| TOOL-031.28 | `apps/watchtower → hft-journal` 启用 feature `writer` | 退出 1 |
| TOOL-031.29 | `apps/replayer → hft-journal` 启用 feature `writer` | 退出 0 |
| TOOL-031.30 | `hft-oms → strategies/x`(`[dev-dependencies]`) | 退出 1 |
| TOOL-031.31 | `strategies/x → hft-oms` | 退出 1 |
| TOOL-031.32 | `strategies/x → hft-types`(直接) | 退出 1 |
| TOOL-031.33 | `hft-strategy-sdk → hft-book` | 退出 1 |
| TOOL-031.34 | `hft-strat-lib → hft-oms` | 退出 1 |
| TOOL-031.35 | `hft-testkit → hft-strategy-registry`(`[dev-dependencies]`,禁止的边) | 退出 1 |
| TOOL-031.36 | `tools/hft-cfg → strategies/x` | 退出 1 |
| TOOL-031.37 | `hft-py → strategies/x`(`[dev-dependencies]`) | 退出 1 |
| TOOL-031.38 | `apps/watchtower → hft-strategy-registry` | 退出 1 |
| TOOL-031.39 | `tools/hft-verify → hft-types` | 退出 1 |
| TOOL-031.40 | `tools/hft-verify →` `reqwest` | 退出 1 |
| TOOL-031.41 | `tools/hft-verify →` `tokio` | 退出 1 |
| TOOL-031.42 | `tools/xtask → strategies/x` | 退出 1 |
| TOOL-031.43 | `tools/hostprobe → hft-rt` | 退出 1 |
| TOOL-031.44 | `tools/hft-cfg → hft-rt` | 退出 1 |
| TOOL-031.45 | `hft-oms → tools/hft-secrets` | 退出 1 |
| TOOL-031.46 | `hft-oms → hft-telemetry`(hot 依赖 cold) | 退出 1 |
| TOOL-031.47 | `strategies/x → hft-strategy-sdk`(`[dependencies]`,`features = ["testing"]`) | 退出 1;报告 `testing` 特性在非 dev 依赖中启用 |
| TOOL-031.48 | 同一边写在 `[dev-dependencies]`,`[dependencies]` 中的同一依赖不带该特性 | 退出 0 |
| TOOL-031.49 | `hft-py` 的 `[features] default = ["testing"]` | 退出 1 |

### TOOL-032 hot-check 对每项检查的反例失败、对合规写法通过

- 规则:[cargo xtask hot-check](README.md#cargo-xtask-hot-check)
- 级别:table
- 里程碑:M1
- 前置:每行在夹具 `clean` 上只做该行的一处改动;`tools/xtask/allow-list.toml` 为夹具版本
- 实现:`tools/xtask/tests/hot_check.rs::tool_032_check_counterexamples`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-032.1 | `crates/hft-oms/clippy.toml` 比 `tools/lint/clippy-hot.toml` 少一行 | 退出 1;报告文件 `crates/hft-oms/clippy.toml` |
| TOOL-032.2 | `crates/hft-oms/clippy.toml` 与模板逐字节相同 | 退出 0 |
| TOOL-032.3 | `crates/hft-oms/src/lib.rs` 的 lint 块删去 `clippy::unwrap_used` | 退出 1(C1) |
| TOOL-032.4 | `crates/hft-oms/src/lib.rs` 把 `#![deny(clippy::panic)]` 改为 `#![warn(clippy::panic)]` | 退出 1(削弱) |
| TOOL-032.5 | `crates/hft-oms/src/a.rs` 加 `#[allow(clippy::disallowed_methods)]`,未登记 | 退出 1 |
| TOOL-032.6 | 同上,`allow-list.toml` 登记了该文件与条目 | 退出 0 |
| TOOL-032.7 | `crates/hft-oms/src/a.rs` 加 `#[allow(clippy::unwrap_used)]`,未登记 | 退出 1 |
| TOOL-032.8 | `crates/hft-oms` 的结构字段类型为 `Box<dyn Fn(u64)>` | 退出 1(N6) |
| TOOL-032.9 | `apps/watchtower`(cold)中同样的字段 | 退出 0 |
| TOOL-032.10 | `crates/hft-oms` 的函数参数类型为 `f64` | 退出 1(N3) |
| TOOL-032.11 | `strategies/x/src/model/mod.rs` 的字段类型为 `f64` | 退出 0 |
| TOOL-032.12 | `strategies/x/src/lib.rs`(快速部分)的字段类型为 `f64` | 退出 1(S1) |
| TOOL-032.13 | `strategies/x/src/lib.rs` 出现 `&dyn Strategy` | 退出 1(S1) |
| TOOL-032.14 | `strategies/x/src/lib.rs` 调用 `hft_strategy_sdk::model_math::ln` | 退出 1(N3) |
| TOOL-032.15 | `strategies/x/src/model/fit.rs` 调用同一函数 | 退出 0 |
| TOOL-032.16 | `crates/hft-oms/src/a.rs` 出现 `unsafe {}` | 退出 1(B5) |
| TOOL-032.17 | `crates/hft-rt/src/a.rs` 的 `unsafe` 块前一行没有 `// SAFETY:` | 退出 1 |
| TOOL-032.18 | 同上,带 `// SAFETY: …` | 退出 0 |
| TOOL-032.19 | `crates/hft-oms/src/a.rs` 加 `#[repr(C)] struct R { a: u64 }`,crate 内没有 `size_of::<R>()` | 退出 1(M4) |
| TOOL-032.20 | `#[repr(C)] struct P { a: u8, b: u64 }` 带 `size_of::<P>() == 16` 断言 | 退出 1(M6);报告字段之间 7 B 隐式填充 |
| TOOL-032.21 | 同上,`a` 与 `b` 之间写出 `_pad: [u8; 7]` | 退出 0 |
| TOOL-032.22 | `crates/hft-oms/src/a.rs` 加 `struct SlotRow { deadline_tsc: u64 }`,未登记 | 退出 1(D6) |
| TOOL-032.23 | 同上,`allow-list.toml` 有 `[[tsc]] row = "SlotRow"`、`field = "deadline_tsc"`、`class = 1` | 退出 0 |
| TOOL-032.24 | `crates/hft-oms/src/a.rs` 调用 `std::thread::spawn` | 退出 1(T1) |
| TOOL-032.25 | `crates/hft-oms` 的字段类型为 `memmap2::MmapMut` | 退出 1(M3) |
| TOOL-032.26 | `crates/hft-oms` 的字段类型为 `AtomicU64` | 退出 1(T3) |
| TOOL-032.27 | 根 `Cargo.toml` 的 `[profile.release]` 写 `panic = "unwind"`(夹具 development 构建剖面表为 `abort`) | 退出 1(B1) |
| TOOL-032.28 | `.cargo/config.toml` 的发布目标段没有 `target-cpu` | 退出 1(B2) |
| TOOL-032.29 | `crates/hft-oms/src/a.rs` 引入 rustls 并调用 `WriteTraffic` 的 `.encrypt(…)` | 退出 1(T6) |
| TOOL-032.30 | 同一调用写在 `crates/hft-net/src/tls.rs` | `xtask::hot_check::scan_tls_encrypt` 返回空;与 TOOL-032.29 的改动同时存在时恰返回 `crates/hft-oms/src/a.rs` 一处 |
| TOOL-032.31 | `apps/watchtower/src/main.rs` 以 `mimalloc::MiMalloc` 为全局分配器,未设 `purge_delay` | 退出 1(M1) |
| TOOL-032.32 | 同上,另调用 `mi_option_set(mi_option_purge_delay, -1)` | 退出 0 |
| TOOL-032.19 | `crates/hft-venue/src/a.rs` 调用 `std::thread::spawn` | 退出 1(T1) |
| TOOL-032.20 | `apps/hft-core/src/main.rs` 调用 `std::thread::Builder::spawn` | 退出 0 |
| TOOL-032.21 | `crates/hft-net/src/conn.rs` 调用 rustls 记录加密入口 | 退出 1(T6) |
| TOOL-032.22 | `crates/hft-net/src/tls.rs` 调用同一入口 | 退出 0 |
| TOOL-032.23 | `crates/hft-ledger/src/a.rs` 出现 `memmap2::MmapMut` | 退出 1(M3) |
| TOOL-032.24 | `crates/hft-journal/src/a.rs` 出现 `memmap2::MmapMut` | 退出 0 |
| TOOL-032.25 | `crates/hft-oms/src/a.rs` 的结构字段类型为 `AtomicU32` | 退出 1(T3) |
| TOOL-032.26 | 根 `Cargo.toml` 的 `[profile.release]` 写 `panic = "unwind"` | 退出 1(B1、C1) |
| TOOL-032.27 | 根 `Cargo.toml` 的 `[profile.ci]` 写 `overflow-checks = false` | 退出 1(B1) |
| TOOL-032.28 | `.cargo/config.toml` 的发布目标段没有 `target-cpu` | 退出 1(B2) |
| TOOL-032.29 | `#[repr(C)] struct S { a: u64 }` 旁没有 `size_of` 断言 | 退出 1(M4) |
| TOOL-032.30 | `#[repr(C)] struct S { a: u8, b: u64 }` 带断言但没有显式 `_pad` | 退出 1(隐式填充,M4、M6) |
| TOOL-032.31 | 全局 mimalloc 选项缺 `purge_delay = -1` | 退出 1(M1) |
| TOOL-032.32 | `crates/hft-oms` 的快照行类型含 `deadline: Tsc` 字段,未登记 | 退出 1(D6) |
| TOOL-032.33 | 同上,`allow-list.toml` 登记为 D6 第 ① 类 | 退出 0 |
| TOOL-032.34 | `crates/hft-strat-lib/src/model/leadlag.rs` 的局部变量类型为 `f64` | 退出 0(N3 豁免) |
| TOOL-032.35 | `crates/hft-strat-lib/src/spread.rs` 的局部变量类型为 `f64` | 退出 1(N3) |
| TOOL-032.36 | `tools/xtask` 导出的 `scan_tls_encrypt` 对 TOOL-032.21 的树调用 | 返回 1 处违例,位置为 `crates/hft-net/src/conn.rs` 的那一行(与 `hot-check` 结论相同) |

### TOOL-033 bench-gate --smoke 以 ci 剖面构建且从不因数值阻断

- 规则:[cargo xtask bench-gate](README.md#cargo-xtask-bench-gate)
- 级别:unit
- 里程碑:M1.5
- 前置:记录型执行器:基准运行成功,输出的 p99 为 `benchmarks` 上限表中该基准上限的 10 倍
- 步骤:
  1. 运行 `cargo xtask bench-gate --smoke`
- 期望:
  - 退出 0;每个 `target/bench/<path>/<case>.json` 的 `dataset.mode = "smoke"`、`gate.kind = "smoke"`、`budget_*` 与 `baseline_p99_ns` 为空
  - 构建调用参数含 `--profile ci`,任何调用都不含 `bench` 或 `release-hot` 剖面
- 实现:`tools/xtask/tests/bench_gate.rs::tool_033_smoke_never_blocks`

### TOOL-034 bench-gate --smoke 在基准无法编译时失败

- 规则:[cargo xtask bench-gate](README.md#cargo-xtask-bench-gate)
- 级别:unit
- 里程碑:M1.5
- 前置:记录型执行器:基准构建返回 101
- 步骤:
  1. 运行 `cargo xtask bench-gate --smoke`
- 期望:
  - 退出 1;stderr 含构建失败的基准目录名
- 实现:`tools/xtask/tests/bench_gate.rs::tool_034_smoke_build_failure`

### TOOL-035 bench-gate --reference 先核对主机剖面,不符退出 2 且不运行基准

- 规则:[cargo xtask bench-gate](README.md#cargo-xtask-bench-gate)
- 级别:unit
- 里程碑:M1.5
- 前置:记录型执行器:`hostprobe verify --expected benchmarks/reference.toml` 返回 1
- 步骤:
  1. 运行 `cargo xtask bench-gate --reference`
- 期望:
  - 退出 2;记录中第一条调用是该 `hostprobe verify`;没有任何基准运行调用
- 实现:`tools/xtask/tests/bench_gate.rs::tool_035_reference_hostprobe_first`

### TOOL-036 bench-gate --reference 按 benchmarks 的阈值类别裁决

- 规则:[cargo xtask bench-gate](README.md#cargo-xtask-bench-gate)
- 级别:table
- 里程碑:M1.5
- 前置:`hostprobe verify` 返回 0;基线为最近 7 次通过运行的 p99 中位数(阈值类别与数值归 [benchmarks 回归阈值](../benchmarks/README.md#回归阈值));基准结果由各行给出
- 实现:`tools/xtask/tests/bench_gate.rs::tool_036_reference_verdicts`

| 行 | 结果 | 期望 |
|---|---|---|
| TOOL-036.1 | `seq-decide` p99 = 基线 × 1.100 | 退出 0(未超过 10%) |
| TOOL-036.2 | `seq-decide` p99 = 基线 × 1.101 | 退出 1;`gate.kind = "relative"`、`verdict = "fail"` |
| TOOL-036.3 | 单 crate 基准 p99 = 上限表值 | 退出 0 |
| TOOL-036.4 | 单 crate 基准 p99 = 上限表值 + 1 ns | 退出 1;`gate.kind = "absolute"` |
| TOOL-036.5 | `kernel_rx` 类基准 p99 = 基线 × 1.5,仍不超绝对门槛 | 退出 0(含内核的分段只判绝对门槛) |
| TOOL-036.6 | 标"分配 = 0"的基准 `allocs_after_warmup = 1` | 退出 1 |
| TOOL-036.7 | 基线只有 3 次通过运行 | 基线 = 这 3 次 p99 的中位数 |
| TOOL-036.8 | 本机 `HostProfile` 哈希与已存基线不同 | 开启新基线序列,旧序列不参与比较;本次只与新序列比较 |

### TOOL-037 bench-gate --syscall-audit 按允许表裁决

- 规则:[cargo xtask bench-gate](README.md#cargo-xtask-bench-gate)
- 级别:table
- 里程碑:M1.5
- 前置:mock 回环运行期间 bpftrace 的计数输出由测试替身给出(按 `comm` 与系统调用号);线程名到角色的映射取被测运行的 `[[threads]]`;允许表取 [hot-path-rules T7](../docs/hot-path-rules.md#线程t)
- 实现:`tools/xtask/tests/bench_gate.rs::tool_037_syscall_audit`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-037.1 | 进程无 root 且无 `CAP_BPF`+`CAP_PERFMON` | 退出 2;不启动 bpftrace |
| TOOL-037.2 | `sequencer` 线程只有 0 次系统调用,`gw` 线程只有 `recvmsg`、`writev` | 退出 0 |
| TOOL-037.3 | `sequencer` 线程出现 1 次 `futex` | 退出 1;JSON `syscalls_outside_allowlist` 列出 (`sequencer`, `futex`, 1) |
| TOOL-037.4 | warm 线程出现 `openat` | 退出 0(warm 线程不审计) |
| TOOL-037.5 | 任一情形 | 记录中没有 `strace` 调用 |

### TOOL-038 bench-gate --crate 只运行该 crate 的基准

- 规则:[cargo xtask bench-gate](README.md#cargo-xtask-bench-gate)
- 级别:unit
- 里程碑:M1.5
- 前置:workspace 中 `hft-book` 与 `hft-oms` 都有名为 `b` 的基准目标;记录型执行器:构建输出两者的基准可执行文件
- 步骤:
  1. 运行 `cargo xtask bench-gate --smoke --crate hft-book`
- 期望:
  - 构建调用带 `--workspace`、不带 `-p`;记录中的基准运行只有 `hft-book` 的可执行文件;`target/bench/` 下没有 `hft-oms` 的结果文件
- 实现:`tools/xtask/tests/bench_gate.rs::tool_038_crate_filter`

### TOOL-039 replay-verify 对未改动的样本报告一致

- 规则:[cargo xtask replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:replay
- 里程碑:M2
- 输入:真实 DST 在临时目录产出的物理 journal 样本(含 `OrderDecision`、`ExecDecision`、`CmdResult`、启动与按需 `StateSnapshot`、`LatencySample`);由真实 writer 重算 CRC 与段链
- 期望:
  - 退出 0;JSON `divergences = []`
- 实现:`apps/replayer/tests/toolcases.rs::tool_039_untouched_sample_passes`

### TOOL-040 篡改一个 Output 字节时报告第一个分歧的 seq

- 规则:[cargo xtask replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:unit
- 里程碑:M2
- 前置:TOOL-039 的样本复制一份;首条 `OrderDecision` 的 `px` 字段(体偏移 16,非测量字段)翻转一位,记录其 seq 为 S,并重算记录 CRC
- 步骤:
  1. 运行 `cargo xtask replay-verify <复制的目录>`
- 期望:
  - 退出 1;报告 `first_divergent_seq = S`
  - 报告含两侧记录的十六进制与解码视图;`journal_fragment` 路径指向的文件含 S 之前至多 200 条逻辑记录,保持顺序与原 seq,可用 `hft_event::decode_record` 逐条读取
  - `field_diff` 指向 `px`;快照组件诊断由 RPL-279/280 验收,不依赖 DST HashTrail 金样
- 实现:`apps/replayer/tests/toolcases.rs::tool_040_reports_first_divergent_seq`

### TOOL-041 只改测量字段或快照头 snap_tsc 不报分歧

- 规则:[cargo xtask replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:table
- 里程碑:M2
- 前置:TOOL-039 的样本复制一份,按行改动后重算 CRC
- 实现:`apps/replayer/tests/toolcases.rs::tool_041_masked_fields_ignored`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-041.1 | 一条 `OrderDecision` 中 `MEASURE_MASK` 覆盖的字段(`decided_tsc`) | 退出 0 |
| TOOL-041.2 | 一个 `StateSnapshot` 头的 `snap_tsc` | 退出 0 |
| TOOL-041.3 | 同一非起点 `StateSnapshot` 的 `engine_ns` 字段 | 退出 1,`first_divergent_seq` = 该快照的 seq |
| TOOL-041.4 | 一条 `LatencySample`(Meta)中的一个字节 | 退出 0 |

### TOOL-042 Keyframe 快照的 state_hash 由 replayer 侧计算

- 规则:[cargo xtask replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:unit
- 里程碑:M9
- 前置:样本中 `Keyframe` 快照头 `state_hash = 0`;其后的普通快照头带非 0 哈希
- 步骤:
  1. 运行 `cargo xtask replay-verify <样本>`
- 期望:
  - 退出 0;报告中该 Keyframe 的比较值为 replayer 计算的哈希(非 0),没有因头部为 0 产生的分歧
- 实现:`tools/xtask/tests/replay_verify.rs::tool_042_keyframe_hash_computed`

### TOOL-043 journal 需要外部配置才能回放时报告不自足

- 规则:[cargo xtask replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:unit
- 里程碑:M2
- 前置:将样本中所有 `ConfigBlob{Refdata}` 的 kind 改为其他 blob kind,保持 seq 连续并重建段链与 CRC,使 journal 内没有 Refdata
- 步骤:
  1. 运行 `cargo xtask replay-verify <样本>`
- 期望:
  - 退出 1;stderr 与 JSON 含"journal 不自足"及缺失的 blob 种类 `Refdata`;进程没有打开样本目录以外的任何配置文件
- 实现:`apps/replayer/tests/toolcases.rs::tool_043_not_self_contained`

### TOOL-044 --from-seq/--to-seq 之外的分歧不计

- 规则:[cargo xtask replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:unit
- 里程碑:M2
- 前置:同 TOOL-040(分歧在 seq S),首个可恢复快照 seq 为 B
- 步骤:
  1. 运行 `cargo xtask replay-verify <样本> --from-seq B --to-seq S-1`
  2. 运行 `cargo xtask replay-verify <样本> --from-seq S`
- 期望:
  - 步骤 1 退出 0
  - 步骤 2 退出 1,`first_divergent_seq = S`
- 实现:`apps/replayer/tests/toolcases.rs::tool_044_seq_range`

### TOOL-045 --record-decide-hist 输出 decide 分段直方图

- 规则:[cargo xtask replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:unit
- 里程碑:M2
- 前置:TOOL-039 的样本
- 步骤:
  1. 运行 `cargo xtask replay-verify <样本> --record-decide-hist`
- 期望:
  - 退出 0;JSON 含 `decide_hist`,其 `count` 等于样本中 `OrderDecision` 记录数,含 p50/p90/p99/p99.9/max
- 实现:`apps/replayer/tests/toolcases.rs::tool_045_decide_hist`

### TOOL-046 Sim 账户的 journal 同样逐字节验证

- 规则:[cargo xtask replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:replay
- 里程碑:M4
- 输入:`fixtures/replay/` 中一份 `sim` 实例的 journal 样本(只含 Sim 账户,模拟回报为 Input)
- 期望:
  - 退出 0;把其中一条模拟回报之后的第一条 `OrderDecision` 篡改一个字节后退出 1,`first_divergent_seq` 为该记录的 seq
- 实现:`tools/xtask/tests/replay_verify.rs::tool_046_sim_journal_verifiable`

### TOOL-047 dst 缺少 --seeds 时退出 2

- 规则:[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:table
- 里程碑:M2
- 前置:夹具中套件 `core`、`modes` 存在
- 实现:`tools/xtask/tests/dst.rs::tool_047_bad_args`

| 行 | 参数 | 期望 |
|---|---|---|
| TOOL-047.1 | (无参数) | 退出 2 |
| TOOL-047.2 | `--seeds 1 --suite nosuch` | 退出 2 |
| TOOL-047.3 | `--hashtrail check`(选项已移除) | 退出 2 |
| TOOL-047.4 | `--seeds 10 --first 2`(选项已移除) | 退出 2 |
| TOOL-047.5 | `--seeds 10 --hashtrail write` | 退出 2 |

### TOOL-048 dst 缺省起点由提交 SHA 派生

- 规则:[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:unit
- 里程碑:M2
- 前置:HEAD 为提交 A;另有提交 B ≠ A
- 步骤:
  1. 在 A 上运行 `cargo xtask dst --seeds 3` 两次
  2. 在 B 上运行 `cargo xtask dst --seeds 3`
- 期望:
  - 两次步骤 1 的 JSON `seeds` 相同且为 3 个连续值
  - 步骤 2 的起点与步骤 1 不同
- 实现:`tools/xtask/tests/dst.rs::tool_048_default_seed_from_sha`

### TOOL-049 --seeds 1 --seed S 只运行种子 S

- 规则:[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:unit
- 里程碑:M2
- 前置:无
- 步骤:
  1. 运行 `cargo xtask dst --seeds 1 --seed 0x9e3779b97f4a7c15 --steps 500`
- 期望:
  - JSON `seeds = ["0x9e3779b97f4a7c15"]`,每个种子 `steps ≤ 500`
- 实现:`tools/xtask/tests/dst.rs::tool_049_single_seed`

### TOOL-050 不带 --suite 时套件由种子派生,带 --suite 时全部种子在该套件运行

- 规则:[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:unit
- 里程碑:M2
- 前置:套件 `core`、`modes`、`storage`
- 步骤:
  1. `cargo xtask dst --seeds 50 --seed 1000` 运行两次
  2. `cargo xtask dst --seeds 50 --seed 1000 --suite modes`
- 期望:
  - 步骤 1 两次的 (种子 → 套件) 映射逐项相同,且与 `hft-dst run --seeds 1000..1049` 的映射相同
  - 步骤 2 的 50 个种子的套件全部是 `modes`
- 实现:`tools/xtask/tests/dst.rs::tool_050_suite_selection`

### TOOL-051 --regressions 在回归文件所示套件中追加运行回归种子

- 规则:[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:unit
- 里程碑:M2
- 前置:`fixtures/dst-seeds/core.txt` 两行(种子 7、9),`fixtures/dst-seeds/modes.txt` 一行(种子 11)
- 步骤:
  1. 运行 `cargo xtask dst --seeds 2 --seed 100 --regressions`
- 期望:
  - JSON 中共 5 个种子:100、101(派生套件)与 (7, core)、(9, core)、(11, modes)
- 实现:`tools/xtask/tests/dst.rs::tool_051_regressions_in_own_suite`

### TOOL-052 失败时输出种子、不变量、journal 片段与一行复现命令

- 规则:[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:unit
- 里程碑:M2
- 前置:hft-testkit 的植入缺陷替身(OMS 在 429 时把订单判为失败),已知在套件 `core` 的种子 S 第 N 步违反 I1
- 步骤:
  1. 运行 `cargo xtask dst --seeds 1 --seed S --suite core`
- 期望:
  - 退出 1;stderr 恰有一行匹配 `^DST FAILED seed=0x[0-9a-f]{16} step=N invariant=I1\([a-z_]+\) -> cargo xtask dst --seeds 1 --seed 0x[0-9a-f]{16} --suite core --steps N\+1 --trace$`(N+1 写成具体数值)
  - `target/dst/<seed>/` 含故障表、复现行与至多 200 条事件的 journal 记录片段(含失败事件);用 `hft_event::decode_record` 读完,按 EVENT_END 计数。不含完整引导块的片段用于诊断,重演使用复现命令
- 实现:`tools/xtask/tests/dst.rs::tool_052_failure_output`

### TOOL-053 dst 的任何运行都不写服务器账本

- 规则:[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:unit
- 里程碑:M2
- 前置:`HFT_VERIFY_STATE` 未设置;临时 HOME
- 步骤:
  1. 运行 `cargo xtask dst --seeds 5 --seed 1`(含一个失败种子)
- 期望:
  - 运行前后 `target/` 之外没有新文件;不存在任何 `ledger.json`、`runs/` 目录
- 实现:`tools/xtask/tests/dst.rs::tool_053_never_writes_ledger`

### TOOL-054 --hashtrail check 与金样一致时通过,篡改一个字节时报告第一处分歧

已删除:HashTrail 金样(`fixtures/dst/hashtrail.toml`)与 `dst --hashtrail check|write` 按负责人决定(2026-10-14)移除;DST 确定性改由进程内两次运行比较(crates/hft-testkit TK-181)。

### TOOL-055 --strategy K 运行 registry 中该策略的 DST,缺省只跑测试策略

- 规则:[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:unit
- 里程碑:M4
- 前置:`strategies/registry/tests/` 含策略 `strat-a` 的 DST 入口
- 步骤:
  1. 运行 `cargo xtask dst --seeds 2 --seed 1`
  2. 运行 `cargo xtask dst --seeds 2 --seed 1 --strategy strat-a`
- 期望:
  - 步骤 1 的 cargo 调用只涉及 `hft-testkit` 的测试目标,JSON `strategy_set = "TestStrategies"`
  - 步骤 2 的 cargo 调用为 `strategies/registry` 的测试目标,JSON `strategy_set = "strat-a"`
- 实现:`tools/xtask/tests/dst.rs::tool_055_strategy_flag`

### TOOL-056 --trace 打印逐步轨迹

- 规则:[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:unit
- 里程碑:M2
- 前置:无
- 步骤:
  1. 运行 `cargo xtask dst --seeds 1 --seed 5 --steps 20 --trace`
  2. 同参数不带 `--trace`
- 期望:
  - 步骤 1 的 stderr 恰有 20 行步级轨迹(每行含步号 0–19、虚拟时间与事件 kind)
  - 步骤 2 的 stderr 没有步级轨迹行
- 实现:`tools/xtask/tests/dst.rs::tool_056_trace_flag`

### TOOL-057 gen-schema 生成全部产物且重复运行逐字节相同

- 规则:[cargo xtask gen-schema](README.md#cargo-xtask-gen-schema)
- 级别:unit
- 里程碑:M1
- 前置:夹具 `events`(测试代码构造的 gen-schema 模型:3 个 kind、一张两项的 Note 登记表与 `hft-oms` 的 `NOTE_CODES` 表、22 个 `LoadErrCode`);输出目录为空
- 步骤:
  1. 运行 `cargo xtask gen-schema`
- 期望:
  - 退出 0;生成 `schemas/arrow/<kind>.json`(3 个)、`python/_generated/events.py`、`docs/generated/event-catalog.md`、`crates/hft-telemetry/src/notes_catalog.rs` 与 `schemas/event-layout.lock`;同一输入再运行一次(基线为第一次生成的 lock),不写任何文件,全部生成物逐字节相同
- 实现:`tools/xtask/tests/gen_schema.rs::tool_057_generates_four_outputs`

### TOOL-058 Note 登记表与 source crate 的 const 表不一一对应时失败

- 规则:[cargo xtask gen-schema](README.md#cargo-xtask-gen-schema)
- 级别:table
- 里程碑:M1
- 前置:夹具 `events`,按行改动
- 实现:`tools/xtask/tests/gen_schema.rs::tool_058_note_registry_mismatch`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-058.1 | 登记表有 `OmsFoo`,`hft-oms` 的 const 表没有 | 退出 1;报告 `OmsFoo` |
| TOOL-058.2 | `hft-oms` 的 const 表有 `OMS_BAR`,登记表没有 | 退出 1;报告 `OMS_BAR` |
| TOOL-058.3 | 两边同名但 source 不同(码值 `source << 24 \| n` 因此不同) | 退出 1;报告两个码值 |

### TOOL-059 导出的策略 schema 与已提交金样逐字节比较

- 规则:[cargo xtask gen-schema](README.md#cargo-xtask-gen-schema)
- 级别:table
- 里程碑:M2
- 前置:记录型执行器扮演已构建的 `hft-core schemas --out target/schemas`,按行输出
- 实现:`tools/xtask/tests/gen_schema.rs::tool_059_strategy_schema_compare`

| 行 | `hft-core schemas` 输出 | 期望 |
|---|---|---|
| TOOL-059.1 | 与 `fixtures/strategy-schemas/` 逐字节相同 | 退出 0 |
| TOOL-059.2 | `index.json` 多一个空格 | 退出 1;报告 `index.json` |
| TOOL-059.3 | 少一个 `<kind_id>-<name>.schema.json` | 退出 1;报告缺失文件名 |
| TOOL-059.4 | 找不到已构建的 `hft-core` 二进制 | 退出 2 |

### TOOL-060 gen-schema --check 不改仓库内容,有差异退出 1

- 规则:[cargo xtask gen-schema](README.md#cargo-xtask-gen-schema)
- 级别:unit
- 里程碑:M1
- 前置:夹具 `events` 已生成(基线为生成的 lock);随后在源码(夹具模型)给一个 kind 追加尾部字段并把 `schema_version` 加一,但不重新生成
- 步骤:
  1. 记录全部生成物的 SHA-256
  2. 运行 `cargo xtask gen-schema --check`
- 期望:
  - 退出 1;stderr 列出有差异的生成物路径
  - 步骤 2 后全部生成物的 SHA-256 与步骤 1 相同(只写临时目录)
- 实现:`tools/xtask/tests/gen_schema.rs::tool_060_check_is_read_only`

### TOOL-061 gen-schema --check 拒绝违反只追加核对的改动

- 规则:[cargo xtask gen-schema](README.md#cargo-xtask-gen-schema)
- 级别:table
- 里程碑:M1
- 前置:夹具 `events` 已生成 `schemas/event-layout.lock`(基线,测试直接以基线文本传入);各行改源码(改夹具模型)后**连同生成物与锁文件一起**重新生成,再以该基线运行 `--check`;失败条件的口径归 hft-event
- 实现:`tools/xtask/tests/gen_schema.rs::tool_061_append_only_violations`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-061.1 | 交换某 kind 两个字段的顺序 | 退出 1 |
| TOOL-061.2 | 删去 `LoadErrCode` 中值为 22 的变体 | 退出 1;报告编号 22 |
| TOOL-061.3 | 把 `_pad` 字节改成同宽的具名字段,大小不变 | 退出 0 |
| TOOL-061.4 | 同一 PR 尾部追加两次、版本累计加 2,对合并基点核对 | 退出 0;两次追加均保留旧字段布局 |
| TOOL-061.5 | 追加字段但不增加版本 | 退出 1;报告 schema_version |
| TOOL-061.6 | 布局不变但版本回退 | 退出 1;报告 schema_version |
| TOOL-061.7 | 版本加 2 同时改名旧字段 | 退出 1;版本递增不豁免布局保护 |

### TOOL-062 gen-host --check 在生成物无差异时通过

当前 dev2 prod/testnet 的入口与提交一致性由 `deploy/tests/test_gen.py::dep_250_pure_generator` 覆盖；本用例的 c32 全矩阵尚未验收。

- 规则:[cargo xtask gen-host](README.md#cargo-xtask-gen-host)
- 级别:golden
- 里程碑:M3
- 输入:夹具清单 `deploy/hosts/test-c32.toml` 与 hft-core 的 `c32` 档
- 期望文件:`tools/xtask/tests/fixtures/gen-host/expected/test-c32/`(GRUB 片段、各 slice 的 `AllowedCPUs`、`irq-plan.json`、两个实例配置的 `[host]`/`[[threads]]`/出口地址段、`egress.toml`、watchtower 隔离核线程白名单、hostprobe 期望剖面)
- 实现:`tools/xtask/tests/gen_host.rs::tool_062_golden_c32`

### TOOL-063 gen-host 对线程计划与生成物的不一致失败

- 规则:[cargo xtask gen-host](README.md#cargo-xtask-gen-host)
- 级别:table
- 里程碑:M3
- 前置:夹具 `test-c32` 的生成物已提交;各行只做该行改动
- 实现:`tools/xtask/tests/gen_host.rs::tool_063_check_failures`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-063.1 | 线程计划中 `gw:binance-usdm` 的核号 5 → 6,不重新生成 | `--check` 退出 1;列出差异文件 |
| TOOL-063.2 | 把 `feed-bbo:okx-swap` 与 `feed-md:okx-swap` 放进同一线程 | 退出 1(生成前的线程计划检查) |
| TOOL-063.3 | 两个线程登记同一核号 | 退出 1 |
| TOOL-063.4 | 隔离集合含 sim 实例的核 | 退出 1 |
| TOOL-063.5 | 隔离集合含批处理核 | 退出 1 |
| TOOL-063.6 | 手改已提交生成物中的 2 MiB 大页数 +1 | `--check` 退出 1;报告大页数与内存表求和不等 |
| TOOL-063.7 | 未改动 | `--check` 退出 0 |

### TOOL-064 生成物不含网卡 MAC

- 规则:[cargo xtask gen-host](README.md#cargo-xtask-gen-host)
- 级别:unit
- 里程碑:M3
- 前置:夹具 `test-c32`;清单中网卡按 `device_index` 描述
- 步骤:
  1. 运行 `cargo xtask gen-host --host test-c32`
- 期望:
  - 全部生成物中没有匹配 `([0-9a-f]{2}:){5}[0-9a-f]{2}` 的串
- 实现:`tools/xtask/tests/gen_host.rs::tool_064_no_mac_in_outputs`

### TOOL-065 不带 --host 时处理全部清单,带 --host 时只处理该清单

- 规则:[cargo xtask gen-host](README.md#cargo-xtask-gen-host)
- 级别:unit
- 里程碑:M3
- 前置:`deploy/hosts/` 下有 `test-c32.toml`、`test-dev2.toml`
- 步骤:
  1. 运行 `cargo xtask gen-host`
  2. 清空生成物后运行 `cargo xtask gen-host --host test-dev2`
- 期望:
  - 步骤 1 后两份清单的生成物都存在
  - 步骤 2 后只有 `test-dev2` 的生成物;`test-dev2` 的生成物没有 GRUB 片段与 IRQ 计划,有 dev2 护栏配置
- 实现:`tools/xtask/tests/gen_host.rs::tool_065_host_selection`

### TOOL-066 doc-check 对每项检查的反例失败、对合规写法通过

- 规则:[cargo xtask doc-check](README.md#cargo-xtask-doc-check)
- 级别:table
- 里程碑:M1
- 前置:每行在夹具 `docs-clean` 上只做该行改动;名单取夹具的 `tools/xtask/doc-check.toml`(含策略族名 `fooarb`、已删除组件 `old-comp`、历史叙述关键词 `以前`)
- 实现:`tools/xtask/tests/doc_check.rs::tool_066_check_counterexamples`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-066.1 | 某 README 含指向不存在文件 `../nosuch.md` 的相对链接 | 退出 1;报告文件与行号 |
| TOOL-066.2 | 某 README 的相对链接指向 `hft-oms/README.md` 中不存在的锚点 | 退出 1(锚点不存在) |
| TOOL-066.3 | 链接到存在的文件与锚点 | 退出 0 |
| TOOL-066.4 | 某 README frontmatter 缺 `status` | 退出 1 |
| TOOL-066.5 | `scope: experimental` | 退出 1 |
| TOOL-066.6 | `status: done` | 退出 1 |
| TOOL-066.7 | `## 依赖` 出现在 `## 测试与验收` 之前 | 退出 1(二级标题顺序) |
| TOOL-066.8 | hot crate README "延迟与资源契约"写基准名 `oms/alloc`,`benches/budgets.toml` 没有 | 退出 1 |
| TOOL-066.9 | 同名基准在 README 与 `budgets.toml` 的数值不同 | 退出 1 |
| TOOL-066.10 | crate 预算宽于 benchmarks 上限表 | 退出 1 |
| TOOL-066.11 | 正文出现 `` `kernel_rxx` ``(不在 latency-budget §2) | 退出 1 |
| TOOL-066.12 | 某 `EventKind` 没有任何 README 声明为产生者 | 退出 1 |
| TOOL-066.13 | event-model 编号表的分类与 `KindInfo.role` 不同 | 退出 1 |
| TOOL-066.14 | 正文出现 `Note` 后接花括号包住的未登记名 `OmsNoSuch` | 退出 1 |
| TOOL-066.15 | 同样写法,名字 `ProbeEdge` 登记在某策略的 `StrategyDesc.notes` 中 | 退出 0 |
| TOOL-066.16 | 正文出现 `Timer` 后接花括号包住的未登记名 `NoSuch` | 退出 1 |
| TOOL-066.17 | `#[repr(C)]` 结构的 `u8` 字段 `leg_kind` 注释既无 `=` 码值也无码表链接 | 退出 1 |
| TOOL-066.18 | `#[repr(C)]` 结构字段字节和 40,注释声明 48 B | 退出 1 |
| TOOL-066.19 | `crates/AGENTS.md` 依赖图多一条 `deps.toml` 没有的边 | 退出 1 |
| TOOL-066.20 | 某组件 frontmatter `scope: optional`,根 README 组件一览把它列在 core | 退出 1 |
| TOOL-066.21 | 正文出现排期措辞("阶段"后接数字,或"第"+数字+"周";夹具文本在 `tools/xtask/tests/fixtures/docs-clean/schedule.md`) | 退出 1 |
| TOOL-066.22 | `status: frozen` 组件的 README 链接已退役的名字 | 退出 0(跳过跨文档一致性) |
| TOOL-066.23 | 同一链接出现在非冻结组件 README | 退出 1 |
| TOOL-066.24 | `crates/hft-oms/README.md` 正文含 `fooarb` | 退出 1 |
| TOOL-066.25 | `crates/hft-oms/README.md` 正文含小写 `cdex` | 退出 1 |
| TOOL-066.26 | `python/pipeline/hft_pipeline/core.py` 含 `fooarb` | 退出 1 |
| TOOL-066.27 | `strategies/fooarb/README.md` 含 `fooarb` | 退出 0 |
| TOOL-066.28 | 组件 README 正文含"以前" | 退出 1 |
| TOOL-066.29 | 同一词只出现在该 README 的"延伸阅读"节 | 退出 0 |
| TOOL-066.30 | 同一词出现在 `.agents/notes/` 或 `docs/postmortem/` | 退出 0 |
| TOOL-066.31 | 某 README 链接已删除组件 `old-comp` | 退出 1 |
| TOOL-066.32 | Agent Note 的相对链接指向不存在的文件 | 退出 1 |
| TOOL-066.33 | Agent Note 放在类别目录 `misc/`(不在封闭集合) | 退出 1 |
| TOOL-066.34 | 根 `AGENTS.md` 计量 1,800 | 退出 0 |
| TOOL-066.35 | 根 `AGENTS.md` 计量 1,801 | 退出 1 |
| TOOL-066.36 | 子树 `AGENTS.md` 计量 901 | 退出 1 |
| TOOL-066.37 | `docs/architecture.md` 计量 3,001 | 退出 1 |
| TOOL-066.38 | hft-event 环拓扑的 `hk` 行元素类型写 `HkMsg`,hft-sequencer 环表写 `TimerMsg` | 退出 1;报告环 `hk` |
| TOOL-066.39 | hft-sequencer 环表多一个 hft-event 未登记的环 `foo` | 退出 1;报告环 `foo` |
| TOOL-066.40 | 两处只有 optional 的 `chain-in` 不一致 | 退出 0 |
| TOOL-066.41 | `scope: core` 的夹具 venue README 删去必备节"账户配置核验" | 退出 1;报告缺失的节名 |
| TOOL-066.42 | 夹具 venue 预算表 `classify` 只写一个数 | 退出 1;报告缺分位数 |
| TOOL-066.43 | `docs/runbooks/x.md`(`scope: core`)缺 `## 回退` | 退出 1;报告文件与缺失标题 |
| TOOL-066.44 | 同页 `## 验收` 出现在 `## 步骤` 之前 | 退出 1(顺序) |
| TOOL-066.45 | 同页六个二级标题齐全且顺序为 适用范围 → 前置条件 → 步骤 → 验收 → 回退 → 记录 | 退出 0 |

### TOOL-067 doc-check 按 GitHub 规则生成锚点

- 规则:[实现说明](README.md#实现说明)、[cargo xtask doc-check](README.md#cargo-xtask-doc-check)
- 级别:table
- 里程碑:M1
- 前置:无
- 实现:`tools/xtask/tests/doc_check.rs::tool_067_anchor_slug`

| 行 | 标题 | 期望锚点 |
|---|---|---|
| TOOL-067.1 | `### 1. 金样` | `1-金样` |
| TOOL-067.2 | `## 延迟与资源契约` | `延迟与资源契约` |
| TOOL-067.3 | `### 分段(\`StageSet\`)` | `分段stageset` |
| TOOL-067.4 | `### cargo xtask decoupling-gate` | `cargo-xtask-decoupling-gate` |
| TOOL-067.5 | `#### 质量门(\`quality-gates.yml\`)` | `质量门quality-gatesyml` |
| TOOL-067.6 | 同一文件中第二个 `### 示例` | `示例-1` |

### TOOL-068 doc-check 篇幅计量去掉代码块与链接 URL

- 规则:[cargo xtask doc-check](README.md#cargo-xtask-doc-check)
- 级别:table
- 里程碑:M1
- 前置:计量口径归 [docs/AGENTS.md 篇幅](../docs/AGENTS.md#篇幅)
- 实现:`tools/xtask/tests/doc_check.rs::tool_068_length_metric`

| 行 | 文本 | 期望计量 |
|---|---|---|
| TOOL-068.1 | `单写者规则` | 5 |
| TOOL-068.2 | `use hft-oms now` | 3 |
| TOOL-068.3 | `[链接](https://example.com/a/b)` | 2 |
| TOOL-068.4 | 围栏代码块内 100 个汉字 + 块外 `是` | 1 |
| TOOL-068.5 | `,。——` | 0 |

### TOOL-069 new-strategy 生成可构建的骨架并只在 registry 加一行

- 规则:[cargo xtask new-strategy](README.md#cargo-xtask-new-strategy)
- 级别:unit
- 里程碑:M2
- 前置:干净仓库;registry kind_id 登记表的真实策略行为 1、2、3、4(4 标"退役:old")
- 步骤:
  1. 运行 `cargo xtask new-strategy foo --kind-id 5`
- 期望:
  - 退出 0;`strategies/foo/` 含 `Cargo.toml`、`src/lib.rs`、`tests/scenario.rs`、`README.md`,没有 Model 骨架与 `pipeline/`
  - `strategies/foo/Cargo.toml` 的依赖恰为 `hft-strategy-sdk`、`hft-strat-lib`
  - `git status --porcelain --untracked-files=all` 恰为:`strategies/foo/**`、workspace 清单(一行新增)、`strategies/registry/src/lib.rs`(一行新增,`strategy_set!` 末尾)、`strategies/registry/Cargo.toml`(一行新增的依赖)与 kind_id 登记表的一行
  - 记录中运行了 `strategies/foo` 的单元测试与 `deps`,都返回 0
- 实现:`tools/xtask/tests/new_strategy.rs::tool_069_skeleton_builds`

### TOOL-070 new-strategy --with-model --with-pipeline 生成 Model 骨架与插件包

- 规则:[cargo xtask new-strategy](README.md#cargo-xtask-new-strategy)
- 级别:unit
- 里程碑:M2
- 前置:同 TOOL-069
- 步骤:
  1. 运行 `cargo xtask new-strategy foo --kind-id 0x0005 --with-model --with-pipeline`(十六进制写法)
- 期望:
  - Model 骨架声明 `mem_bytes` 与 `snapshot_bytes`;`strategies/foo/pipeline/` 是 Python 包骨架
  - `git status --porcelain --untracked-files=all` 中没有 `crates/`、`apps/`、`tools/hft-cfg/`、`python/` 下的文件
- 实现:`tools/xtask/tests/new_strategy.rs::tool_070_model_and_pipeline`

### TOOL-071 kind_id 已占用或目录已存在时退出 1 且不写文件

- 规则:[cargo xtask new-strategy](README.md#cargo-xtask-new-strategy)
- 级别:table
- 里程碑:M2
- 前置:registry kind_id 登记表的真实策略行为 1、2、3、4(4 标"退役:old");`strategies/bar/` 已存在
- 实现:`tools/xtask/tests/new_strategy.rs::tool_071_conflicts`

| 行 | 命令 | 期望 |
|---|---|---|
| TOOL-071.1 | `new-strategy foo --kind-id 3` | 退出 1;`git status --porcelain` 为空 |
| TOOL-071.2 | `new-strategy bar --kind-id 5` | 退出 1;`git status --porcelain` 为空 |
| TOOL-071.3 | `new-strategy foo`(缺 `--kind-id`) | 退出 2 |
| TOOL-071.4 | `new-strategy foo --kind-id 4`(退役行仍算占用) | 退出 1;`git status --porcelain` 为空 |
| TOOL-071.5 | `new-strategy foo --kind-id 6`(跳号,下一个应为 5) | 退出 1;报告期望的编号 5;`git status --porcelain` 为空 |

### TOOL-072 fixtures fetch 按指针下载并校验 SHA-256

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:table
- 里程碑:M3
- 前置:`fixtures/replay/a.pointer.toml` 指向替身 S3 对象,登记解压后大小与 SHA-256 = H;只有 S3 下载替身,压缩与解压运行真实 zstd;输入包含跨 64 KiB 缓冲边界的非 UTF-8 字节
- 实现:`tools/xtask/tests/fixtures_fetch.rs::tool_072_fetch_verifies_hash`

| 行 | 替身对象内容 | 期望 |
|---|---|---|
| TOOL-072.1 | SHA-256 = H | 退出 0;`target/fixtures-cache/` 下有该对象 |
| TOOL-072.2 | 内容被改一个字节 | 退出 2;`target/fixtures-cache/` 下没有该对象 |
| TOOL-072.3 | 对象不存在 | 退出 2 |

### TOOL-073 fixtures extract 从 raw journal 抽出报文为 .in

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:golden
- 里程碑:M3
- 输入:`tools/xtask/tests/fixtures/extract/raw-journal/`(一个 raw journal 段,含同一连接上 3 条 `RawRx`,其中一条响应拆成 2 块)
- 期望文件:`tools/xtask/tests/fixtures/extract/expected/*.in`(按连接拼接后的报文)
- 实现:`tools/xtask/tests/fixtures_cmd.rs::tool_073_extract_raw_journal`

### TOOL-074 fixtures check 发现敏感信息时失败

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:table
- 里程碑:M1
- 前置:扫描规则归 [fixtures](../fixtures/README.md)
- 实现:`tools/xtask/tests/fixtures_cmd.rs::tool_074_check_secrets`

| 行 | `fixtures/` 中加入 | 期望 |
|---|---|---|
| TOOL-074.1 | 无改动 | 退出 0 |
| TOOL-074.2 | 一个 `.in` 文件含 64 位十六进制 API key 形态的 `"apiKey":"…"` | 退出 1;报告文件与偏移 |
| TOOL-074.3 | 同一文件经 `fixtures scrub` 后 | 退出 0 |

### TOOL-075 fixtures corpus 以金样输入生成 fuzz 语料

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:unit
- 里程碑:M3
- 前置:`fixtures/binance-usdm/` 下有 5 个 `.in`
- 步骤:
  1. 运行 `cargo xtask fixtures corpus binance-usdm`
- 期望:
  - 语料目录恰有 5 个文件,内容的集合与 5 个 `.in` 的内容集合逐字节相同
- 实现:`tools/xtask/tests/fixtures_cmd.rs::tool_075_corpus_from_goldens`

### TOOL-076 fixtures bbo-diff 对一致与不一致的两组序列分别报告

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:table
- 里程碑:M3
- 前置:替身 WAL 与 journal 覆盖同一窗口;一致性规则归 [fixtures](../fixtures/README.md)
- 实现:`tools/xtask/tests/fixtures_cmd.rs::tool_076_bbo_diff`

| 行 | 两边序列 | 期望 |
|---|---|---|
| TOOL-076.1 | 逐条相同 | 退出 0 |
| TOOL-076.2 | journal 少一条 BBO | 退出 1;报告该条的交易所时间与 update_id |

### TOOL-077 fuzz 以固定日期的 nightly 运行并把崩溃输入最小化后写进回归目录

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:unit
- 里程碑:M3
- 前置:`tools/xtask/toolchains.toml` 的 nightly = `nightly-2026-09-01`;记录型执行器让目标 `binance-usdm` 报告一个崩溃输入 C(最小化后为 C')
- 步骤:
  1. 运行 `cargo xtask fuzz binance-usdm --time 60`
- 期望:
  - `cargo` 调用带 `+nightly-2026-09-01` 与 `-max_total_time=60`;退出 1
  - `fixtures/binance-usdm/regressions/` 新增恰一个文件,内容与 C' 逐字节相同
- 实现:`tools/xtask/tests/fuzz.rs::tool_077_pinned_nightly_and_regression`

### TOOL-078 fuzz --repro 以 libFuzzer 默认限额复现单个输入

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:unit
- 里程碑:M3
- 前置:输入文件 `in.bin`
- 步骤:
  1. 运行 `cargo xtask fuzz binance-usdm --repro in.bin`
- 期望:
  - 目标进程的参数含 `in.bin`,不含 `-rss_limit_mb`、`-malloc_limit_mb`、`-timeout`(使用默认限额);输出结论 `reproduced` 或 `not-reproduced`
- 实现:`tools/xtask/tests/fuzz.rs::tool_078_repro_default_limits`

### TOOL-079 fuzz --nightly 按轮转表短跑且不写账本与仓库语料

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:unit
- 里程碑:M3
- 前置:`fuzz-targets.toml` 登记 3 个目标,每夜每目标 1 分钟
- 步骤:
  1. 运行 `cargo xtask fuzz --nightly`
- 期望:
  - 3 个目标各运行一次,`-max_total_time = 60`
  - 运行后仓库内 `fixtures/**/corpus/` 无变化;不存在任何 `ledger.json`
- 实现:`tools/xtask/tests/fuzz.rs::tool_079_nightly_rotation`

### TOOL-080 path-digest 等于路径清单的 git ls-tree 输出的 SHA-256

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:unit
- 里程碑:M6
- 前置:夹具仓库提交 X;`fuzz-targets.toml` 中目标 `binance-usdm` 的 `paths = ["crates/venue-binance", "crates/hft-venue-proto"]`
- 步骤:
  1. 运行 `cargo xtask path-digest --sha X --group binance-usdm --json`
- 期望:
  - 输出值 = SHA-256(`git ls-tree -r X -- crates/venue-binance crates/hft-venue-proto Cargo.lock tools/xtask/toolchains.toml Cargo.toml` 按路径排序的输出);连续运行两次逐字节相同
- 实现:`tools/xtask/tests/path_digest.rs::tool_080_equals_ls_tree_hash`

### TOOL-081 path-digest 只随目标组路径内的改动变化

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:table
- 里程碑:M6
- 前置:同 TOOL-080;提交 X 的 digest 为 D0;各行在 X 上加一个提交 Y
- 实现:`tools/xtask/tests/path_digest.rs::tool_081_digest_sensitivity`

| 行 | Y 的改动 | 期望 |
|---|---|---|
| TOOL-081.1 | `crates/hft-oms/src/lib.rs` | `binance-usdm` 的 digest = D0 |
| TOOL-081.2 | `crates/venue-binance/src/lib.rs` | ≠ D0 |
| TOOL-081.3 | `Cargo.lock` | ≠ D0 |
| TOOL-081.4 | `tools/xtask/toolchains.toml` 的 nightly 日期 | ≠ D0 |
| TOOL-081.5 | 根 `Cargo.toml` 的 `[profile.ci]` | ≠ D0 |
| TOOL-081.6 | `crates/hft-oms/src/lib.rs`,组 `dst`(`[dst].paths` 含 `crates/hft-oms`) | `dst` 的 digest 变化 |

### TOOL-082 path-digest 对未知组或未知提交退出 2

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:table
- 里程碑:M6
- 前置:同 TOOL-080
- 实现:`tools/xtask/tests/path_digest.rs::tool_082_bad_inputs`

| 行 | 参数 | 期望 |
|---|---|---|
| TOOL-082.1 | `--group nosuch` | 退出 2 |
| TOOL-082.2 | `--sha 0000000000000000000000000000000000000000` | 退出 2 |
| TOOL-082.3 | 不带 `--group` | 输出每个 core 目标组与 `dst` 各一个 digest |

### TOOL-083 journal-verify 报告每类损坏的段号与偏移

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:table
- 里程碑:M1
- 前置:以 hft-journal 写者(堆内存热段、磁盘冷副本)写出的 4 段 seq journal 冷目录(段 1–3 已滚动并封存,段 4 未封存;每段引导块后 100 条 `Bbo`),各行改动一份副本(TOOL-083.5 把 `BootDone` 的滚动标记改写为 `Begin` 并重算该记录的 CRC);检查项口径归 [journal-format 校验与缺段检测](../docs/subsystems/journal-format.md#校验与缺段检测)
- 实现:`tools/xtask/tests/journal_verify.rs::tool_083_corruption_reports`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-083.1 | 无改动 | 退出 0 |
| TOOL-083.2 | 段 3 在偏移 8192 处截断(段已封存) | 退出 1;报告段 3、偏移 8192 |
| TOOL-083.3 | 段 2 偏移 4096 处记录(引导块 `Begin`;`seq` journal 中该 kind 不带 CRC)的 `len` 最低位翻转 | 退出 1;报告段 2、偏移 4096 |
| TOOL-083.4 | 删除段 2 | 退出 1;报告缺段 2 |
| TOOL-083.5 | 段 1 引导块去掉 `BootDone` | 退出 1;报告段 1 与引导块偏移 |
| TOOL-083.6 | 最后一段只有 `RECOVERY_GAP` 标记的尾部缺失 | 退出 0;报告"可能丢失的尾部" |

### TOOL-084 journal-verify --from-segment 与 --json

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:unit
- 里程碑:M1
- 前置:同 TOOL-083.3 的目录(段 2 损坏)
- 步骤:
  1. 运行 `cargo xtask journal-verify <dir> --from-segment 3 --json`
- 期望:
  - 退出 0;stdout 的 JSON 只含段 3、4 的结果
- 实现:`tools/xtask/tests/journal_verify.rs::tool_084_from_segment_json`

### TOOL-085 setup 按 toolchains.toml 安装固定版本且可重复运行

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:unit
- 里程碑:M1
- 前置:`toolchains.toml` 登记 `cargo-deny = "0.16.1"`、`cargo-fuzz = "0.12.0"`、nightly 日期 D
- 步骤:
  1. 运行 `cargo xtask setup` 两次
- 期望:
  - 第一次的调用含 `cargo install cargo-deny --version 0.16.1 --locked`、`cargo install cargo-fuzz --version 0.12.0 --locked` 与安装 nightly D
  - 第二次在版本已符时不再调用 `cargo install`;两次都退出 0
- 实现:`tools/xtask/tests/setup.rs::tool_085_pinned_and_idempotent`

### TOOL-086 test-trace 在规格与代码完全对应时通过

- 补充前置:同一夹具增加 `CARRY-001`(M4) 及对应测试,验证五字母前缀可解析且不会误报多余测试。
- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:unit
- 里程碑:M1
- 前置:夹具 `trace-clean`:`test-trace.toml` 的 `current_milestone = "M2"`;`crates/a/TESTS.md` 有 `AA-001`(unit,M1,实现 `crates/a/tests/x.rs::aa_001_ok`)、`AA-002`(dst,M2,实现 `crates/hft-testkit/src/scenarios/a.rs::aa_002_scn`)、`AA-003`(bench,M2,基准名 `a/hot`)、`AA-004`(host,M2);`docs/tests/scenarios.md` 有 `B-1`(dst,M2);代码中有函数 `aa_001_ok`、`Scenario::new("AA-002", …)`、`Scenario::new("B-1", …)`、Criterion id `a/hot`
- 步骤:
  1. 运行 `cargo xtask test-trace`
- 期望:
  - 退出 0;JSON 的 `unimplemented`、`extra`、`inconsistent` 三张清单都为空
- 实现:`tools/xtask/tests/test_trace.rs::tool_086_clean_passes`

### TOOL-087 里程碑已到的用例缺实现时列为未实现(只报告)

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:table
- 里程碑:M1
- 前置:夹具 `trace-clean`,按行改动
- 实现:`tools/xtask/tests/test_trace.rs::tool_087_unimplemented`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-087.1 | 删除函数 `aa_001_ok` | 退出 0;`unimplemented = ["AA-001"]` |
| TOOL-087.2 | `AA-001` 的实现字段改为 `待实现`,函数保留 | 退出 0;`unimplemented` 含 `AA-001` |
| TOOL-087.3 | 删除 `Scenario::new("B-1", …)` | 退出 0;`unimplemented = ["B-1"]` |
| TOOL-087.4 | 删除 Criterion id `a/hot` | 退出 0;`unimplemented = ["AA-003"]` |
| TOOL-087.5 | `AA-001` 的函数加 `#[ignore]` | 退出 0;`unimplemented = ["AA-001"]` |
| TOOL-087.6 | 同上,另给 `AA-001` 加 `状态:隔离:<Agent Note 链接>` | 退出 0;`unimplemented = []` |

### TOOL-088 里程碑未到或需要人工的用例不要求实现

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:table
- 里程碑:M1
- 前置:夹具 `trace-clean`,按行改动
- 实现:`tools/xtask/tests/test_trace.rs::tool_088_not_required`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-088.1 | 新增 `AA-005`(unit,M3,`待实现`),`current_milestone = "M2"` | 退出 0 |
| TOOL-088.2 | 同上,`current_milestone = "M3"` | 退出 0;`unimplemented = ["AA-005"]` |
| TOOL-088.3 | 新增 `AA-006`(unit,M1.5,`待实现`),`current_milestone = "M1"` | 退出 0(M1 < M1.5) |
| TOOL-088.4 | 删除 `AA-004` 对应的任何脚本(host 级) | 退出 0 |

### TOOL-089 代码中带编号而规格中没有的测试列为多余(只报告)

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:table
- 里程碑:M1
- 前置:夹具 `trace-clean`,按行在 `crates/a/tests/x.rs` 加函数
- 实现:`tools/xtask/tests/test_trace.rs::tool_089_extra`

| 行 | 加入的函数 | 期望 |
|---|---|---|
| TOOL-089.1 | `#[test] fn aa_099_orphan()` | 退出 0;`extra = ["aa_099_orphan"]` |
| TOOL-089.2 | `#[test] fn zz_001_unknown_prefix()` | 退出 0;`extra` 含该函数 |
| TOOL-089.3 | `proptest!` 中的 `fn aa_098_prop(..)` | 退出 0;`extra` 含该函数 |
| TOOL-089.4 | `#[test] fn helper_case()`(不匹配 `^[a-z]+_[0-9]{3}_`) | 退出 0 |
| TOOL-089.5 | 非测试函数 `fn aa_097_helper()`(无测试属性) | 退出 0 |

### TOOL-090 实现字段与找到的位置不一致时只报告,规格格式错误与重复编号才失败

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:table
- 里程碑:M1
- 前置:夹具 `trace-clean`,按行改动
- 实现:`tools/xtask/tests/test_trace.rs::tool_090_inconsistent`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-090.1 | 函数 `aa_001_ok` 移到 `crates/a/tests/y.rs`,规格不改 | 退出 0;`inconsistent` 含 (`AA-001`, 规格 `crates/a/tests/x.rs::aa_001_ok`, 实际 `crates/a/tests/y.rs::aa_001_ok`) |
| TOOL-090.2 | 函数改名为 `aa_001_renamed`,规格不改 | 退出 0;`AA-001` 在 `inconsistent` |
| TOOL-090.3 | 两处代码都有 `aa_001_ok` | 退出 0;`AA-001` 在 `inconsistent` |
| TOOL-090.4 | `AA-001` 的实现字段改为 `见 x.rs`(既不是 `待实现` 也不是 `<路径>::<函数>`) | 退出 1;`errors` 恰一项,含 `AA-001: 实现` |
| TOOL-090.5 | `crates/a/TESTS.md` 再写一个 `AA-001` | 退出 1;`errors` 恰一项,含 `AA-001: case id defined twice` |
| TOOL-090.6 | `AA-001` 的里程碑改为 `第二期` | 退出 1;`errors` 恰一项,含 `AA-001: unparsable 里程碑` |

### TOOL-091 test-trace 读 docs/tests 下的规格并输出三张清单与逐里程碑计数

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)
- 级别:unit
- 里程碑:M1
- 前置:夹具 `trace-clean`;另在 `docs/tests/invariants.md` 加 `INV-1`(prop,M1,`待实现`)
- 步骤:
  1. 运行 `cargo xtask test-trace`
- 期望:
  - 退出 0(三张清单只报告);`unimplemented = ["INV-1"]`;JSON 恰有 `unimplemented`、`extra`、`inconsistent`、`errors` 四个清单键与 `by_milestone` 对象,共 5 个键;`by_milestone` = `M1` {cases 2, implemented 1, unimplemented 1}、`M2` {cases 3, implemented 3, unimplemented 0}(只计自动级别、未删除的用例,不受 `current_milestone` 限制)
  - stderr 摘要列出三张清单的条目数(1、0、0)、`errors=0` 与逐里程碑计数
- 实现:`tools/xtask/tests/test_trace.rs::tool_091_reads_docs_tests`

### TOOL-092 az-probe run 多次解析端点并探测全部 A 记录

- 规则:[az-probe](README.md#az-probe)
- 级别:unit
- 里程碑:M1.5
- 前置:替身解析器对 `fstream.binance.com` 依次返回 {1.1.1.1, 1.1.1.2}、{1.1.1.2, 1.1.1.3}、{1.1.1.1};`targets.toml` 只含该端点;替身 TCP 连接每次耗时 300 µs
- 步骤:
  1. 运行 `az-probe run --targets targets.toml --out r.json --duration 1m`
- 期望:
  - `r.json` 中该端点恰有 3 个 IP 行(1.1.1.1、1.1.1.2、1.1.1.3),每行 TCP 握手样本数 = 9,`min_rtt_us = 300`
- 实现:`tools/az-probe/tests/test_run.py::test_tool_092_probes_all_a_records`

### TOOL-093 az-probe 按 ip-ranges.json 标注端点属性

- 规则:[az-probe](README.md#az-probe)
- 级别:table
- 里程碑:M1.5
- 前置:替身 `ip-ranges.json`:`3.112.0.0/14` 为 `EC2`(ap-northeast-1),`13.32.0.0/15` 为 `CLOUDFRONT`;Cloudflare 网段表含 `104.16.0.0/13`
- 实现:`tools/az-probe/tests/test_attribution.py::test_tool_093_endpoint_attribution`

| 行 | IP | 期望属性 |
|---|---|---|
| TOOL-093.1 | `3.112.5.6` | `ec2` |
| TOOL-093.2 | `13.32.1.1` | `cloudfront` |
| TOOL-093.3 | `104.16.2.2` | `cloudflare` |
| TOOL-093.4 | `8.8.8.8` | `other` |

### TOOL-094 无 PHC 同步时首帧样本标记 unsynced

- 规则:[az-probe](README.md#az-probe)
- 级别:unit
- 里程碑:M1.5
- 前置:替身环境没有 `/dev/ptp0`;chrony 未以 PHC 为参考;替身 WS 端点推 10 帧
- 步骤:
  1. 运行 `az-probe run … --ws-first-frame`
- 期望:
  - 首帧分布行 `clock = "unsynced"`,样本数 10;报告仍含全部 RTT 结果
- 实现:`tools/az-probe/tests/test_run.py::test_tool_094_first_frame_unsynced`

### TOOL-095 不带 --ws-first-frame 时只做握手与 ping,不订阅任何流

- 规则:[az-probe](README.md#az-probe)
- 级别:unit
- 里程碑:M1.5
- 前置:替身 WS 端点记录收到的帧
- 步骤:
  1. 运行 `az-probe run --targets targets.toml --out r.json --duration 1m`
- 期望:
  - 替身端点没有收到任何订阅帧;`r.json` 没有首帧分布行;进程没有读取任何 API key 文件或环境变量
- 实现:`tools/az-probe/tests/test_run.py::test_tool_095_handshake_only`

### TOOL-096 az-probe 输出每 (AZ, CPG, 端点, IP) 的统计

- 规则:[az-probe](README.md#az-probe)
- 级别:golden
- 里程碑:M1.5
- 输入:替身 RTT 序列(2 个端点 × 2 个 IP × 9 个样本,AZ `apne1-az4`,CPG `cpg-a`)
- 期望文件:`tools/az-probe/tests/fixtures/expected-run.json`(每行 `az`、`cpg`、`endpoint`、`ip`、`min_rtt_us`、`p50_us`、`p99_us`、`tls_handshake_us`、`n`)
- 实现:`tools/az-probe/tests/test_run.py::test_tool_096_output_rows`

### TOOL-097 compare 按 core 交易所握手 minRTT 之和升序排名

- 规则:[az-probe](README.md#az-probe)
- 级别:unit
- 里程碑:M1.5
- 前置:三份报告:(az1, cpg-x) 的 Binance/OKX/Bitget minRTT 为 900/700/800 µs;(az4, cpg-y) 为 300/900/800;(az4, cpg-z) 为 310/880/800;另含 Gate 端点 100 µs(optional)
- 步骤:
  1. 运行 `az-probe compare report-*.json`
- 期望:
  - 排名顺序恰为 (az4, cpg-z) 1990 µs、(az4, cpg-y) 2000 µs、(az1, cpg-x) 2400 µs;Gate 不计入和
- 实现:`tools/az-probe/tests/test_compare.py::test_tool_097_ranking`

### TOOL-098 首次 AZ 探测报告覆盖活跃时段并量化各 AZ 差异

- 规则:[az-probe](README.md#az-probe)
- 级别:host
- 里程碑:M1.5
- 环境:apne1-az1/az2/az4 各 3–5 台 Spot 探针,每个 AZ 一个 CPG
- 授权:`[需授权]`(开机与删机由负责人执行)
- 步骤:按 [runbooks/az-probe](../docs/runbooks/az-probe.md) 运行 `az-probe run` 与 `compare`
- 通过标准:运行窗口覆盖 ≥ 1 个 UTC 活跃交易时段;`az-report.md` 对 Binance、OKX、Bitget 各给出三个 AZ 的 minRTT 与 p99;每 (AZ, CPG, 端点) 的 TCP 样本数 ≥ 9
- 证据:`ops-log/<date>-az-probe.md`,文件名含 `TOOL-098`
- 实现:`tests/host/TOOL-098.sh`

### TOOL-099 周复测在生产主机上限定在冷核且不超时

- 规则:[az-probe](README.md#az-probe)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(生产 CPG)
- 授权:无
- 步骤:以普通用户在 housekeeping 以外的冷核 cgroup 中运行一次周复测 `az-probe run`(不带 `--ws-first-frame`)
- 通过标准:运行时长 ≤ 10 min;进程 CPU 使用 ≤ 1 核;`ps -o psr` 采样从未落在隔离核或 IRQ 专用核;没有订阅帧
- 证据:复测报告与 `pidstat` 输出,文件名含 `TOOL-099`
- 实现:`tests/host/TOOL-099.sh`

### TOOL-100 hostprobe collect 输出每类采集项并记录来源

- 规则:[hostprobe](README.md#hostprobe)
- 级别:table
- 里程碑:M1.5
- 前置:以夹具根目录 `tools/hostprobe/tests/fixtures/c32-root/` 替代 `/`(含伪造的 `/proc`、`/sys`、cgroup 与 nftables/BPF 查询替身);以 root 身份运行;IMDS 替身返回设备序号 0/1/2 的网卡
- 实现:`tools/hostprobe/tests/collect.rs::tool_100_categories_present`

| 行 | 类别 | 期望(`hostprofile.json` 中) |
|---|---|---|
| TOOL-100.1 | 实例 | 实例段的 `type = "c8g.8xlarge"`、`az_id = "apne1-az4"`,`source = "imdsv2"` |
| TOOL-100.2 | 内核 | `cmdline.isolcpus = "2-15"`,`source = "/proc/cmdline"` |
| TOOL-100.3 | CPU | `cpu.isolated = "2-15"`、`smt_active = false`、`clocksource = "arch_sys_counter"` |
| TOOL-100.4 | 内存 | `hugepages.2048kB.total = 1024`、`thp = "madvise"` |
| TOOL-100.5 | 网卡 | `nic.eni_b.ifname = "ens6"`(按 `device_index = 1` 识别)、`queues = 8`、`allowance_exceeded.bw_in = 0` |
| TOOL-100.6 | 中断 | ENI-B 每个队列 IRQ 的 `effective_affinity_list = "1"`;managed IRQ 单列 |
| TOOL-100.7 | 网络栈 | `net.core.busy_poll = 0`、`nf_conntrack_loaded = false` |
| TOOL-100.8 | 内核线程 | `workqueue_cpumask = "0001"` |
| TOOL-100.9 | resctrl | `mounted = false`(夹具未挂载)|
| TOOL-100.10 | 时钟 | `chrony.max_error_ns`、`chrony.refclock_phc_poll_hz` 取自 `chronyc` 替身输出 |
| TOOL-100.11 | 占用 | `irq_cores_tasks = []` |
| TOOL-100.12 | 资源护栏 | `hftbatch.slice.cpu_idle = 1`、`hftbatch.slice.parent = "/"` |
| TOOL-100.13 | IMDS 锁定 | `imds.bpf.connect4 = ["hft_imds4"]`、`imds.bpf.connect6 = ["hft_imds6"]` |
| TOOL-100.14 | 网卡(出口) | `/etc/hft/egress.toml` 的每个地址 `on_nic = true`、`eip = "<替身 EIP>"` |

### TOOL-101 hostprobe 输出规范化 JSON 并附内容哈希

- 规则:[hostprobe](README.md#hostprobe)
- 级别:unit
- 里程碑:M1.5
- 前置:同 TOOL-100
- 步骤:
  1. 运行 `hostprobe collect --out a.json` 两次(第二次写 `b.json`)
- 期望:
  - `a.json` 与 `b.json` 逐字节相同;每个对象的键按字典序排列
  - `host_profile_hash` = 去掉该字段后内容的 SHA-256(十六进制),两次相同
- 实现:`tools/hostprobe/tests/collect.rs::tool_101_canonical_json_hash`

### TOOL-102 读不到的项标 unreadable 而不省略

- 规则:[hostprobe](README.md#hostprobe)
- 级别:unit
- 里程碑:M1.5
- 前置:同 TOOL-100,但以非 root 身份运行,IMDS 替身对非 root 返回连接拒绝
- 步骤:
  1. 运行 `hostprobe collect --out a.json`
- 期望:
  - 退出 0;实例段的 `type`、`az_id` 等 IMDS 项的值为 `"unreadable"` 并带 `reason`;键集合与 root 运行时相同
- 实现:`tools/hostprobe/tests/collect.rs::tool_102_unreadable_marked`

### TOOL-103 hostprobe 只以 ioctl 读网卡,只启动 chronyc 一个外部程序

- 规则:[hostprobe](README.md#hostprobe)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1.5
- 前置:同 TOOL-100;进程启动替身记录每次 `exec`
- 步骤:
  1. 运行 `hostprobe collect --out a.json`
- 期望:
  - 记录的 `exec` 只有 `chronyc -c tracking` 与 `chronyc -c sources`;没有 `ethtool`、`nft`、`bpftool`;网卡信息来自 `SIOCETHTOOL` 替身的调用记录
- 实现:`tools/hostprobe/tests/collect.rs::tool_103_ioctl_and_chronyc_only`

### TOOL-104 hostprobe 不做连接级检查

- 规则:[hostprobe](README.md#hostprobe)
- 级别:unit
- 里程碑:M1.5
- 前置:同 TOOL-100;夹具根含 `/proc/net/tcp` 与一个属于别的进程的 socket
- 步骤:
  1. 运行 `hostprobe collect --out a.json`
- 期望:
  - 文件访问记录中没有 `/proc/net/tcp*` 与 `/proc/*/fd/*`;输出没有按连接的字段
- 实现:`tools/hostprobe/tests/collect.rs::tool_104_no_connection_checks`

### TOOL-105 IRQ 专用核上只有登记的 keepawake 线程算预期线程

- 规则:[hostprobe](README.md#hostprobe)
- 级别:table
- 里程碑:M1.5
- 前置:IRQ 专用核 = 1;期望剖面登记 keepawake 线程名 `hft-keepawake-1`;夹具 `/proc/*/stat` 按行放置可运行任务
- 实现:`tools/hostprobe/tests/occupancy.rs::tool_105_keepawake_whitelist`

| 行 | 核 1 上的可运行任务 | 期望 |
|---|---|---|
| TOOL-105.1 | `hft-keepawake-1` | `irq_cores_tasks = []`;`verify` 退出 0 |
| TOOL-105.2 | `spin-loop`(未登记的自旋线程) | `irq_cores_tasks` 含 (`1`, `spin-loop`);`verify` 退出 1 |
| TOOL-105.3 | `hft-keepawake-1` 与 `hft-keepawake-1b` | 后者列入占用;`verify` 退出 1 |
| TOOL-105.4 | 无任务,内核参数含 `cpuidle.off=1` | 输出列出 `cpuidle.off` 为未允许的保持唤醒手段;`verify` 退出 1 |
| TOOL-105.5 | 无任务,进程 `x` 经 `/dev/cpu_dma_latency` 持有 0 µs 约束(设备读出 0,低于默认值) | `keepawake_violations` 列出 `cpu_dma_latency=0`;`verify` 退出 1 |
| TOOL-105.6 | 无任务,内核参数含 `nohlt` | 不列为占用;`verify` 退出 0 |

### TOOL-106 verify 对每个 required 项的不符退出 1 并逐行列出

- 规则:[hostprobe](README.md#hostprobe)
- 级别:table
- 里程碑:M1.5
- 前置:期望剖面夹具 `tools/hostprobe/tests/fixtures/upgrade-c32.toml`(测试清单 `test-c32` 的期望剖面,按 [hostprobe 期望剖面格式](README.md#hostprobe) 手写,各项的 required/advisory 按 [deploy HostProfile](../deploy/README.md#hostprofile) 标定;deploy 生成器落地(M3)后改为生成);夹具根初始与期望全部相符(`verify` 退出 0);每行只改一项
- 实现:`tools/hostprobe/tests/verify.rs::tool_106_required_mismatch`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-106.1 | `cpu/isolated` = `2-14` | 退出 1;stderr 行 `cpu.isolated expected=2-15 actual=2-14 required` |
| TOOL-106.2 | cmdline 去掉 `nohz_full` | 退出 1 |
| TOOL-106.3 | cmdline 的 `rcu_nocbs` 不含 IRQ 专用核 1 | 退出 1 |
| TOOL-106.4 | 2 MiB 大页总数 1023 | 退出 1 |
| TOOL-106.5 | THP = `always` | 退出 1 |
| TOOL-106.6 | ENA 驱动版本不同 | 退出 1 |
| TOOL-106.7 | ENI-B 不在 `device_index = 1` | 退出 1 |
| TOOL-106.8 | `net.core.busy_read = 50` | 退出 1;stderr 行键为 `net.core.busy_read` |
| TOOL-106.9 | `net.core.busy_poll = 50` | 退出 1 |
| TOOL-106.10 | ENI-B 的 `napi_defer_hard_irqs` 改为非 0(夹具取 2) | 退出 1 |
| TOOL-106.11 | ENI-B 的 `gro_flush_timeout = 20000` | 退出 1 |
| TOOL-106.12 | ENI-B 一个队列的 IRQ 落在核 3 | 退出 1 |
| TOOL-106.13 | ENI-A 的 IRQ 落在核 2 | 退出 1 |
| TOOL-106.14 | 一个 NVMe managed IRQ 的 `effective_affinity` 含核 1 | 退出 1 |
| TOOL-106.15 | ENI-B 一个 TX 队列的 XPS 掩码与 `irq-plan.json` 不同 | 退出 1 |
| TOOL-106.16 | chrony 误差界 40 µs | 退出 1(要求 < 40 µs) |
| TOOL-106.17 | chrony 误差界 39.999 µs | 退出 0 |
| TOOL-106.18 | `hft-sim.slice` 的 `AllowedCPUs` 含核 2 | 退出 1 |
| TOOL-106.19 | `egress.toml` 的一个地址不在指定网卡上 | 退出 1 |
| TOOL-106.20 | 同一地址在网卡上但未关联 EIP | 退出 1 |
| TOOL-106.21 | 根 cgroup `connect4` 上没有 BPF 程序 | 退出 1 |
| TOOL-106.22 | `nf_conntrack` 已加载 | 退出 1 |
| TOOL-106.23 | `hftbatch.slice` 的 `cpu.idle = 0` | 退出 1 |
| TOOL-106.24 | chrony 与 hft-core 合计 PHC 读取频率超过期望剖面登记的上限(上限取值归 hft-rt;夹具上限 125/s,合计 126/s) | 退出 1 |
| TOOL-106.25 | 合计 125/s | 退出 0 |

### TOOL-107 dev2 剖面的 required 项不符时退出 1

- 规则:[hostprobe](README.md#hostprobe)
- 级别:table
- 里程碑:M3
- 前置:期望剖面夹具 `test-dev2.toml`;夹具根初始全部相符;每行只改一项
- 实现:`tools/hostprobe/tests/verify.rs::tool_107_dev2_required`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-107.1 | `hft-dev2-guard.service` inactive | 退出 1 |
| TOOL-107.2 | `hft-imds-dev2.nft` 的表未加载 | 退出 1 |
| TOOL-107.3 | `hft.slice` 的 `MemorySwapMax` 非 0 | 退出 1 |
| TOOL-107.4 | `hftbatch.slice` 设了 `CPUQuota` | 退出 1 |
| TOOL-107.5 | `hftbatch.slice` 不在根级(在 `hft.slice` 下) | 退出 1 |
| TOOL-107.6 | 隔离核集合与期望不同(dev2 上为 advisory) | 退出 0;stderr 行以 `advisory` 结尾 |

### TOOL-108 verify 在 required 项读不到时退出 1,输入错误时退出 2

- 规则:[hostprobe](README.md#hostprobe)
- 级别:table
- 里程碑:M1.5
- 前置:同 TOOL-106
- 实现:`tools/hostprobe/tests/verify.rs::tool_108_unreadable_and_input_errors`

| 行 | 条件 | 期望 |
|---|---|---|
| TOOL-108.1 | 非 root 运行,IMDS 项 `unreadable`,ENI 设备序号为 required | 退出 1;该键行 `actual=unreadable required` |
| TOOL-108.2 | 只有 advisory 项不符(MAC 与 IMDS 不一致) | 退出 0;stderr 列出该键,输出 `verify` 段含它 |
| TOOL-108.3 | `--expected` 文件不存在 | 退出 2 |
| TOOL-108.4 | `--expected` 不是合法 TOML | 退出 2 |
| TOOL-108.5 | `--out` 所在目录不可写 | 退出 2 |

### TOOL-109 pending_reboot 反映 GRUB 片段与 cmdline 的差异且不单独决定退出码

- 规则:[hostprobe](README.md#hostprobe)
- 级别:table
- 里程碑:M1.5
- 前置:同 TOOL-106
- 实现:`tools/hostprobe/tests/verify.rs::tool_109_pending_reboot`

| 行 | 条件 | 期望 |
|---|---|---|
| TOOL-109.1 | 发布内 GRUB 片段与 `/proc/cmdline` 的参数一致 | `pending_reboot = false`;退出 0 |
| TOOL-109.2 | GRUB 片段多一个 `idle=poll`,该参数不是 required 项 | `pending_reboot = true`;退出 0 |
| TOOL-109.3 | GRUB 片段的 `isolcpus` 改为 `2-14`(期望剖面的 `cpu.isolated` 随之为 `2-14`,同一发布),cmdline 未变 | `pending_reboot = true`;退出 1(`cpu.isolated` 不符) |

### TOOL-110 verify 写出与 collect 相同的剖面并附 verify 段

- 规则:[hostprobe](README.md#hostprobe)
- 级别:unit
- 里程碑:M1.5
- 前置:同 TOOL-106.8 的改动
- 步骤:
  1. 运行 `hostprobe collect --out c.json`
  2. 运行 `hostprobe verify --expected upgrade-c32.toml --out v.json`
- 期望:
  - `v.json` 去掉 `verify` 段后与 `c.json` 逐字节相同
  - `v.json.verify` 恰有 1 项:`{key: "net.core.busy_read", expected: 0, actual: 50, level: "required"}`
- 实现:`tools/hostprobe/tests/verify.rs::tool_110_verify_section`

### TOOL-111 osnoise 包装输出隔离核的 max 与 p99.9

- 规则:[hostprobe](README.md#hostprobe)
- 级别:unit
- 里程碑:M1.5
- 前置:`rtla osnoise` 替身输出 3 个核各 10,000 个采样(核 2 的最大值 18 µs,p99.9 4 µs)
- 步骤:
  1. 运行 `hostprobe osnoise --cpus 2-4 --duration 10s`
- 期望:
  - 输出每核的 `max_ns` 与 `p999_ns`;核 2 为 `18000` 与 `4000`;`rtla` 调用参数含 `-c 2-4`
- 实现:`tools/hostprobe/tests/osnoise.rs::tool_111_wrapper_output`

### TOOL-112 升级主机与参考机的隔离核噪声达标

- 规则:[hostprobe](README.md#hostprobe)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`c16` 或 `c32`)与同型参考机
- 授权:`[需授权]`(参考机租用)
- 步骤:按 [host-tuning](../docs/runbooks/host-tuning.md) 调优后运行 `hostprobe osnoise --cpus <hot> --duration 24h`
- 通过标准:每个隔离核 max < 20 µs、p99.9 < 5 µs
- 证据:osnoise 输出与 `hostprofile.json`,文件名含 `TOOL-112`
- 实现:`tests/host/TOOL-112.sh`

### TOOL-113 在参考机与生产 AMI 上 collect 覆盖全部采集项

- 规则:[hostprobe](README.md#hostprobe)
- 级别:host
- 里程碑:M1.5
- 环境:M1.5 参考机(`c8g.4xlarge`)与 dev2;M7 时在生产 AMI 实例上重做
- 授权:`[需授权]`(参考机租用)
- 步骤:以 root 运行 `hostprobe collect --out /run/hft/hostprofile.json`,再以非 root 运行一次
- 通过标准:root 运行的输出含采集表的每一类且无 `unreadable`(resctrl 不可用时值为 `mounted = false`,不算缺项);非 root 运行的 IMDS 项为 `unreadable`;`collect` 墙钟 ≤ 2 s
- 证据:两份 JSON 与计时,文件名含 `TOOL-113`
- 实现:`tests/host/TOOL-113.sh`

### TOOL-114 hft-secrets 全部取到时按清单写出 0400 文件且只打印名字与版本

- Unix 集成测试调用真实二进制与 SDK,本地 HTTP 替身提供 IMDSv2/Secrets Manager/KMS;SDK 传输子测试可跨平台运行,环境隔离在子进程中。属主断言使用测试进程用户,部署账户 hft 的属主操作由同一实现执行。

- 规则:[hft-secrets](README.md#hft-secrets)
- 级别:integ
- 里程碑:M3
- 前置:本地替身 Secrets Manager/KMS 端点;清单两项:`a`(`kind = "plain"`,`json_key = "api_key"`,秘密 JSON `{"api_key":"S3CR3T-A","other":"x"}`,VersionId `v1`)、`b`(`kind = "kms-ciphertext"`,密文解密后为 `S3CR3T-B`,VersionId `v7`);`--owner hft --mode 0400`
- 步骤:
  1. 运行 `hft-secrets fetch --manifest m.toml --out /tmp/t/secrets --owner hft --mode 0400`
- 期望:
  - 退出 0;`/tmp/t/secrets/` 为 `0700`、属主 `hft`;`a` 的内容恰为 `S3CR3T-A`,`b` 的内容恰为 `S3CR3T-B`,两文件 `0400`、属主 `hft`
  - stdout 恰两行,分别含 (`a`, `v1`)、(`b`, `v7`);stdout 与 stderr 全文不含 `S3CR3T`
- 实现:`tools/hft-secrets/tests/fetch.rs::tool_114_writes_files_and_names_only`

### TOOL-115 任一项取不到时删除已写出的部分并退出 1

- 规则:[hft-secrets](README.md#hft-secrets)
- 级别:integ
- 里程碑:M3
- 前置:同 TOOL-114,但替身对 `b` 返回 `AccessDeniedException`
- 步骤:
  1. 运行同一命令
- 期望:
  - 退出 1;`/tmp/t/secrets/` 为空(`a` 已删除);stderr 含 `b` 与错误类型,不含任何秘密值
- 实现:`tools/hft-secrets/tests/fetch.rs::tool_115_all_or_nothing`

### TOOL-116 环境错误退出 2

- 规则:[hft-secrets](README.md#hft-secrets)
- 级别:table
- 里程碑:M3
- 前置:清单同 TOOL-114
- 实现:`tools/hft-secrets/tests/fetch.rs::tool_116_env_errors`

| 行 | 环境 | 期望 |
|---|---|---|
| TOOL-116.1 | IMDS 替身不可达(无实例角色凭证) | 退出 2;输出目录不存在或为空 |
| TOOL-116.2 | IMDS 可达但实例没有角色 | 退出 2 |

### TOOL-117 清单出现未登记的键或重复名字时不取任何秘密

- 规则:[hft-secrets](README.md#hft-secrets)
- 级别:table
- 里程碑:M3
- 前置:替身端点记录请求
- 实现:`tools/hft-secrets/tests/manifest.rs::tool_117_manifest_rejects`

| 行 | 清单 | 期望 |
|---|---|---|
| TOOL-117.1 | 某项多一个键 `value = "S3CR3T"` | 退出 2;替身端点没有收到任何请求 |
| TOOL-117.2 | 某项多一个键 `secret = "x"` | 退出 2 |
| TOOL-117.3 | 两项 `name` 相同 | 退出 2 |
| TOOL-117.4 | `kind = "env"` | 退出 2 |
| TOOL-117.5 | 只含 `name`、`secret_id`、`json_key`、`kind` 四个键 | 继续取秘密(结果同 TOOL-114) |

### TOOL-118 Secrets Manager 与 KMS 的 SDK 只在 hft-secrets 的闭包里

- 规则:[hft-secrets](README.md#hft-secrets)、[cargo xtask deps](README.md#cargo-xtask-deps)
- 级别:table
- 里程碑:M3
- 前置:夹具 `clean`,按行加依赖
- 实现:`tools/xtask/tests/deps.rs::tool_118_secret_sdks_only_in_secrets`

| 行 | 加入的依赖 | 期望 |
|---|---|---|
| TOOL-118.1 | `apps/hft-core → aws-sdk-secretsmanager` | `deps` 退出 1 |
| TOOL-118.2 | `apps/kill-switch → aws-sdk-kms` | `deps` 退出 1 |
| TOOL-118.3 | `apps/journal-archiver → aws-sdk-s3` | `deps` 退出 0 |
| TOOL-118.4 | `hft-oms → aws-sdk-s3` | `deps` 退出 1(hot 闭包含 tokio) |
| TOOL-118.5 | `tools/hft-secrets → aws-sdk-secretsmanager`、`aws-sdk-kms` | `deps` 退出 0 |

### TOOL-119 strategy blob 的 TOML → blob → show 往返一致

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M2
- 前置:schema 目录夹具 `tools/hft-cfg/tests/fixtures/schemas/`(由 SDK `params` 对测试 schema 导出:`index.json` + `65280-probe-taker.schema.json`,`schema_version = 2`;字段按序 `spread_th: Rate[per_group, min 0, max 10_000_000, default 500_000, minor rel_pct:10]`、`max_qty: Qty[per_group]`、`cooldown_ms: DurationMs[min 0, max 60_000, default 1_000, minor abs:100]`、`bins: Count[per_group, structural, min 1, max 64, default 16]`、`whitelist: Count[min 0, max 1000, default 3, minor any]`、`limit_x: Count[min 0, max 100, default 7]`、`mode: Enum["a","b"][since 2]`;测试先由 SDK 重新导出并核对与已提交副本相同);refdata 夹具 `tools/hft-cfg/tests/fixtures/refdata-basic/`(资产 AIN `qty_exp = −2`,实例 id 1 `pc-main` kind `probe-taker`,账户 id 3 `pc-live` 绑定 `pc-main`,venues `binance-usdm`、`bitget-uta-spot`;由 `hft-cfg build --kind refdata` 构建为 `refdata.blob`);TOML `pc-main.pc-live.toml`:`version = 1`(第一版,不带 `--against`),`[defaults] spread_th = "0.0005"`,一个 group `n = 101`、两腿 `binance-usdm:AINUSDT`、`bitget-uta-spot:AINUSDT`,`[groups.overrides] max_qty = "5000"`
- 步骤:
  1. `hft-cfg build --kind strategy --schemas <夹具> --refdata <refdata.blob> --in pc-main.pc-live.toml --out a.blob`
  2. `hft-cfg show a.blob --schemas <夹具> --refdata <refdata.blob>`
- 期望:
  - 步骤 1 退出 0;`StrategyParamsHdr`:`strategy = 1`、`account = 3`、`kind_id = 65280`、`schema_id = 65280`、`schema_ver = 2`、`source = Manual(2)`、`n_groups = 1`、`version = 1`、`schema_digest` = 按 schema 文件重算、与 `index.json` 一致的摘要
  - `GroupSpec.id = 0x0100_0065`;`max_qty` 覆盖值为 5000(raw 500000,`qty_exp = −2`);`spread_th` 默认值 raw 500000
  - 步骤 2 的输出含 `spread_th = 0.05%`(SDK `format_value` 的 Rate 写法,= TOML 的 0.0005)、group `0x01000065` 的 `max_qty = 5000`、两腿 inst 名,与 TOML 的取值逐项相同
- 实现:`tools/hft-cfg/tests/strategy.rs::tool_119_toml_blob_show_roundtrip`

### TOOL-120 同一输入两次编码逐字节相同

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M2
- 前置:同 TOOL-119
- 步骤:
  1. 以同一输入运行步骤 1 两次,输出 `a.blob`、`b.blob`
- 期望:
  - `a.blob` 与 `b.blob` 逐字节相同(`generated_at_ns` 取自 TOML 或固定为输入派生值,不读系统时钟)
- 实现:`tools/hft-cfg/tests/strategy.rs::tool_120_deterministic_encoding`

### TOOL-121 同一份 hft-cfg 二进制对两个测试策略的 schema 通用

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M2
- 前置:schema 目录由 SDK `params` 对两个测试 schema 导出(与 `hft-core schemas --out` 的写法相同):`probe-taker` 同 TOOL-119;`probe-target` kind_id 65281、`schema_version = 1`,字段 `win: Count[per_group, structural, min 1, max 100, default 10]`、`thr: Rate[min 0, max 10_000_000, default 1_000_000, minor rel_pct:10]`、`advice_max_age_ms: DurationMs[min 0, max 600_000, default 5_000]`;hft-cfg 只构建一次;refdata 夹具 `refdata-basic` 另加实例 id 4 `tg-main`(`probe-target`)与绑定它的账户 id 5 `tg-live`(`is_prod = false`)
- 实现:`tools/hft-cfg/tests/strategy.rs::tool_121_generic_over_schemas`

| 行 | 输入 TOML | 期望 |
|---|---|---|
| TOOL-121.1 | `probe-taker` 实例的参数 | 退出 0;`kind_id` = 该策略的 kind_id;`show` 往返一致 |
| TOOL-121.2 | `probe-target` 实例的参数(`thr = "0.002"`,group `n = 7` 覆盖 `win = "20"`) | 退出 0;`kind_id` = 65281;`GroupSpec.id = 0x0400_0007`;`show` 含 `thr = 0.2%`、`win = 20` |
| TOOL-121.3 | 在 schema 目录加入第三个只有 schema 文件的策略 `probe-extra`(kind_id 65282,字段 `level: Count[min 0, max 9, default 1]`),refdata 加实例 id 5 `ex-main` 与账户 id 6 `ex-live`,编码 `level = "3"` | 退出 0;`kind_id` = 65282;`show` 含 `level = 3`;hft-cfg 未重新编译 |

### TOOL-122 Rust 与 Python 两条路径对参数金样逐字节相同

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:integ
- 里程碑:M9
- 前置:`fixtures/params/*.toml` 全部金样;hft-py 包装的 SDK `params` 编码器;同一 schema 与 refdata
- 步骤:
  1. 对每个金样运行 `hft-cfg build --kind strategy …`
  2. 对每个金样经 hft-py 编码
- 期望:
  - 每个金样两条路径的输出逐字节相同;且与 `fixtures/params/<name>.blob` 逐字节相同
- 实现:`tools/hft-cfg/tests/python_parity.rs::tool_122_rust_python_identical`

### TOOL-123 schema 目录的缺失与摘要不符分别退出 2 与 1

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M2
- 前置:同 TOOL-119,各行改 schema 目录
- 实现:`tools/hft-cfg/tests/strategy.rs::tool_123_schema_dir_errors`

| 行 | schema 目录 | 期望 |
|---|---|---|
| TOOL-123.1 | 删除 `index.json` | 退出 2;无输出 blob |
| TOOL-123.2 | `--schemas` 指向不存在的目录 | 退出 2 |
| TOOL-123.3 | `index.json` 中该项的 `schema_digest` 改一位(与按 schema 文件重算的摘要不同) | 退出 1;无输出 blob |
| TOOL-123.4 | schema 文件中 `cooldown_ms` 的 `max` 改为 60001,`index.json` 不变(重算的摘要与索引不同) | 退出 1;无输出 blob |
| TOOL-123.5 | `instances.toml` 的 `kind = "nosuch"` 在 `index.json` 中查不到 | 退出 1 |

### TOOL-124 strategy 参数校验的每条规则拒绝其反例

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M2
- 前置:同 TOOL-119;每行只改该行所列一处;"退出 1"均不产出 blob,stderr 含所列名字
- 实现:`tools/hft-cfg/tests/strategy.rs::tool_124_validation_counterexamples`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-124.1 | 账户 `pc-live` 的 `bound_instance` 未设置 | 退出 1;`AccountBound` |
| TOOL-124.2 | 账户绑定到另一实例 `pc-b` | 退出 1;`AccountBound` |
| TOOL-124.3 | TOML `schema = { id = 65281, version = 1 }`,实例 kind 为 65280 | 退出 1;`KindMismatch` |
| TOOL-124.4 | schema 声明 `non_prod_only = true`,账户 `is_prod = true` | 退出 1;`NonProdOnly` |
| TOOL-124.5 | 同上,账户 `is_prod = false` | 退出 0 |
| TOOL-124.6 | `[groups.overrides] cooldown_ms = "500"`(非 per_group 字段) | 退出 1;`NotPerGroup`(字段 `cooldown_ms`) |
| TOOL-124.7 | TOML `schema = { id = 65280, version = 1 }` 且写了 `mode = "b"`(`since = 2`) | 退出 1;字段 `mode` 在版本 1 不存在 |
| TOOL-124.8 | `[defaults] max_qty = "5000"`(Qty 字段写进 defaults) | 退出 1 |
| TOOL-124.9 | `[defaults] no_such = "1"` | 退出 1;未知字段 `no_such` |
| TOOL-124.10 | `cooldown_ms = "60000"`(= max) | 退出 0 |
| TOOL-124.11 | `cooldown_ms = "60001"`(max + 1) | 退出 1;`OutOfRange` |
| TOOL-124.12 | `spread_th = "0"`(= min) | 退出 0 |
| TOOL-124.13 | `spread_th = "-0.000000001"`(min − 1 raw) | 退出 1;`OutOfRange` |
| TOOL-124.14 | `mode = "c"` | 退出 1;`BadEnum` |
| TOOL-124.15 | `max_qty = "5000.001"`(多于 `qty_exp = −2` 的小数位) | 退出 1(`parse_exact` 精度错误) |
| TOOL-124.16 | 一条腿 `inst = "binance-usdm:NOSUCH"` | 退出 1;`LegNotTradable` |
| TOOL-124.17 | 一条腿的 inst 状态为 `Halted` | 退出 1;`LegNotTradable` |
| TOOL-124.18 | 一条腿的 inst 状态为 `ReduceOnly` | 退出 0 |
| TOOL-124.19 | 一条腿 `okx-swap:AIN-USDT-SWAP`(venue 不在账户内) | 退出 1;`LegVenueNotInAccount` |
| TOOL-124.20 | 两腿基础资产不同(AIN 与 BTC) | 退出 1;`LegAssetMismatch` |
| TOOL-124.21 | 两腿报价资产不同(USDT 与 USDC) | 退出 1;`QuoteMismatch` |
| TOOL-124.22 | 两腿 `binance-usdm:AINUSDT` 与 `binance-spot:AINUSDT`(同一交易所) | 退出 1;`LegVenueNotInAccount`(账户每个交易所只有一个 venue,`check_group` 的逐腿检查先于两腿之间的 `SameExchangeLegs`) |
| TOOL-124.23 | `n = 0` | 退出 1 |
| TOOL-124.24 | `n = 1` | 退出 0;`GroupId = 0x0100_0001` |
| TOOL-124.25 | `n = 0xFF_FFFE` | 退出 0;`GroupId = 0x01FF_FFFE` |
| TOOL-124.26 | `n = 0xFF_FFFF` | 退出 1 |
| TOOL-124.27 | 两个 group 都写 `n = 101` | 退出 1;重复 `GroupId 0x01000065` |
| TOOL-124.28 | 账户 `pc-live` 不存在于 refdata | 退出 1 |

### TOOL-125 带 --against 时新增 group 的 n 必须大于生效 blob 的水位且已有 id 的指纹不变

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M2
- 前置:同 TOOL-119;生效 blob `cur.blob`(`version = 7`)含 group n = 101、103、105(各自的资产与腿已知),`group_ctr_hwm = 105`;各行的 TOML 在保留这三个 group 不变的基础上做该行改动,`--against cur.blob`
- 实现:`tools/hft-cfg/tests/strategy.rs::tool_125_groupid_continuation`
- 备注:TOOL-125.7 待 hft-cfg 接入 hft-journal 的读取接口后实现;此前 `--against`/`--refdata` 给出目录时退出 2

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-125.1 | 新增 group `n = 106` | 退出 0 |
| TOOL-125.2 | 新增 group `n = 104`(小于水位 105) | 退出 1;报告 `n = 104` 不大于 H = 105 |
| TOOL-125.3 | 新增 group `n = 100` | 退出 1 |
| TOOL-125.4 | group 103 的一条腿换成另一 inst(指纹变化) | 退出 1;报告 `GroupId 0x01000067` 的指纹变化 |
| TOOL-125.5 | group 103 只把 `open_allowed` 改为 false | 退出 0(指纹只含资产与腿) |
| TOOL-125.6 | 移除 group 103,同时新增 `n = 103`、腿与原 103 不同 | 退出 1:同一份 TOML 中与"改 103 的腿"无法区分,报告 `GroupId 0x01000067` 的指纹变化;先单独移除 103 生效、再以新腿新增 `n = 103` 时报告 `n = 103` 不大于 H = 105 |
| TOOL-125.7 | `--against` 改为 journal 目录,其中该 (pc-main, pc-live) 最后生效的 `StrategyParams` 即 `cur.blob`;新增 `n = 104` | 退出 1(同 TOOL-125.2) |
| TOOL-125.8 | 不带 `--against`,TOML `version = 8`,新增 `n = 104` | 退出 2;不产出 blob |
| TOOL-125.9 | 生效 blob 只含 101、103(105 已在更早版本移除)、`group_ctr_hwm = 105`,新增 `n = 105` | 退出 1;报告 `n = 105` 不大于 H = 105(被移除的 n 不再使用) |

### TOOL-126 差异报告打印每个 group 两腿的 lot 与 pair_lot

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M2
- 前置:refdata 中 `binance-usdm:AINUSDT` 的 lot = 1(raw,`qty_exp = −2`,即 0.01),`bitget-uta-spot:AINUSDT` 的 lot = 10(0.1);`old.blob` 与 `new.blob` 都含 group 101
- 步骤:
  1. `hft-cfg diff --old old.blob --new new.blob`
- 期望:
  - 输出含 group `0x01000065` 一行:两腿 lot 0.01 与 0.1,`pair_lot` = 0.1(hft-types `pair_lot` 在 1/1 对冲比下的结果)
- 实现:`tools/hft-cfg/tests/diff.rs::tool_126_pair_lot_in_report`

### TOOL-127 差异报告按 minor_change 判定小改

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M2
- 前置:schema 同 TOOL-119;`old.blob` 中 `spread_th` 默认 = 1000(raw)、`cooldown_ms` = 1000、`bins` = 16;另有字段 `whitelist: Count[minor any]` = 3、`limit_x: Count[minor none]` = 7;group 101、103 都未覆盖 `bins`,group 105 覆盖 `bins = 32`;各行只改一处得到 `new.blob`
- 实现:`tools/hft-cfg/tests/diff.rs::tool_127_minor_change`

| 行 | 改动 | 期望(`--json` 的 `minor` 与 `structural`) |
|---|---|---|
| TOOL-127.1 | `spread_th` 1000 → 1100(+10%) | `minor = true` |
| TOOL-127.2 | `spread_th` 1000 → 1101 | `minor = false` |
| TOOL-127.3 | `spread_th` 1000 → 900(−10%) | `minor = true` |
| TOOL-127.4 | `old.blob` 的 `spread_th` = 0,`new.blob` 的 = 1 | `minor = false`(旧值为 0) |
| TOOL-127.5 | `cooldown_ms` 1000 → 1100(abs 100) | `minor = true` |
| TOOL-127.6 | `cooldown_ms` 1000 → 1101 | `minor = false` |
| TOOL-127.7 | `limit_x` 7 → 8(`None`) | `minor = false` |
| TOOL-127.8 | `whitelist` 3 → 300(`Any`) | `minor = true` |
| TOOL-127.9 | 默认 `bins` 16 → 20 | `structural = true`;受影响 group = [`0x01000065`, `0x01000067`](105 有覆盖,不受影响) |
| TOOL-127.10 | 只改 group 105 的 `bins` 覆盖 32 → 40 | `structural = true`;受影响 group = [`0x01000069`] |

### TOOL-128 diff 的人读文本与 --json 内容一致

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M2
- 前置:同 TOOL-127.2 与 TOOL-127.9 两处改动同时存在
- 步骤:
  1. `hft-cfg diff --old old.blob --new new.blob`
  2. 同参数加 `--json`
- 期望:
  - 两份输出列出的改动字段集合相同({`spread_th`, `bins`});文本输出把 `spread_th` 标为非小改、`bins` 标为结构字段并列出两个受影响 group;另有一行说明"升级/降级的权威判定在 sequencer"
- 实现:`tools/hft-cfg/tests/diff.rs::tool_128_text_and_json_agree`

### TOOL-129 refdata 的 TOML → blob 与 hft-core 加载结果逐字节一致并可 show 往返

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M3
- 前置:refdata TOML 夹具 `refdata-basic`(`assets.toml`、`venues.toml`、`insts-*.toml`、`caps.toml`、`venue_caps.toml`、`instances.toml`、`accounts.toml`、`fees.toml`、`limits.toml`、`endpoints.toml`、`maint.toml`;core `venue-*` 尚未接入,能力行的底取 `caps.toml` 的完整行)
- 步骤:
  1. `hft-cfg build --kind refdata --in params/refdata/ --out rd.blob`
  2. `RefData::load(rd.blob, None)` 后经 `dump` 与 `hft-cfg build` 以同一构建器重新编码
  3. `hft-cfg show rd.blob`
- 期望:
  - 步骤 1 退出 0;步骤 2 加载成功,重新编码的字节与 `rd.blob` 相同
  - 步骤 3 的输出中每个账户、实例、inst 与 TOML 的取值逐项相同
- 实现:`tools/hft-cfg/tests/refdata.rs::tool_129_roundtrip_and_load`

### TOOL-130 能力表以 default_caps 为底叠加覆盖,覆盖规则逐条生效

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M3
- 前置:同 TOOL-129;协议 `binance-usdm` 的 `default_caps()` 不支持 `AMEND` 以外的能力 X,`dead_man()` 为 `CountdownPerInst{refresh_ms = 10_000}`;每行只改 `venue_caps.toml` 一处
- 实现:`tools/hft-cfg/tests/refdata.rs::tool_130_caps_overrides`
- 补充断言:成功行另做 dump → build 逐字节比较,无覆盖行不含完整 `[caps.binance-usdm]`

| 行 | 覆盖 | 期望 |
|---|---|---|
| TOOL-130.1 | 无覆盖 | 退出 0;blob 中该行逐字段等于 `default_caps()` |
| TOOL-130.2 | 关闭一个默认开启的能力位,带 `reason` | 退出 0;该位为 0 |
| TOOL-130.3 | 开启协议不支持的能力位 X | 退出 1 |
| TOOL-130.4 | `required_margin_mode = "cross"` 不带 `reason` | 退出 1 |
| TOOL-130.5 | `ignorable_user_events` 删掉默认表中的一项 | 退出 1 |
| TOOL-130.6 | `ignorable_user_events` 追加一项 | 退出 0;blob 中为默认表 + 该项 |
| TOOL-130.7 | 死手 `timeout_ms = 19_999`,`refresh_ms = 10_000` | 退出 1;`DeadManRange` |
| TOOL-130.8 | 死手 `timeout_ms = 20_000`,`refresh_ms = 10_000` | 退出 0 |
| TOOL-130.9 | 现货行 `required_spot_borrow = true` | 退出 1;`SpotBorrowRequired` |
| TOOL-130.10 | 永续行写 `required_spot_borrow = false` | 退出 1;`CapsMarketMismatch` |
| TOOL-130.11 | `required_margin_mode = "mixed"` | 退出 1;`CapsMarketMismatch` |
| TOOL-130.12 | `margin_data = { kind = "poll", interval_ms = 0 }` | 退出 1 |

### TOOL-131 账户、模拟账户与实例的每条加载校验拒绝其反例

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M3
- 前置:同 TOOL-129;Admin 名单(`--admins`,缺省 `.github/hft-admins.txt`)只含 `owner1`;另有 Sim 账户 4 `sim-pc-a`(绑定实例 3 `pc-sim-a`);每行只改该行所列一处;"退出 1"均不产出 blob,stderr 含所列码名
- 实现:`tools/hft-cfg/tests/refdata.rs::tool_131_account_validation`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-131.1 | Sim 账户 `label = "pc-sim-a"` | 退出 1;`AccountNamePrefix` |
| TOOL-131.2 | Live 账户 `label = "sim-live"` | 退出 1;`AccountNamePrefix` |
| TOOL-131.3 | Live 账户 venues 含 `binance-spot` 与 `binance-usdm` | 退出 1;`SameExchangeMarkets` |
| TOOL-131.4 | Sim 账户 venues 含 `okx-spot` 与 `okx-swap` | 退出 1;`SameExchangeMarkets` |
| TOOL-131.5 | 两个账户 `bound_instance = "pc-main"` | 退出 1;`InstanceMultiBound` |
| TOOL-131.6 | Live 账户缺 `bitget-uta-spot` 的 UID | 退出 1;`MissingVenueUid` |
| TOOL-131.7 | `is_prod = true` 的 Live 账户写 `uid_policy = "unverifiable"` | 退出 1;`UidPolicyProd` |
| TOOL-131.8 | Sim 账户写 `uid_policy = "unverifiable"` | 退出 1;`UidPolicyProd` |
| TOOL-131.9 | `is_prod = false` 的 Live 账户:`uid_policy = "unverifiable"`、UID 为空串、`uid_policy_signoff = "owner1"` | 退出 0;差异报告列出签字人 `owner1` |
| TOOL-131.10 | 同上但没有 `uid_policy_signoff` | 退出 1 |
| TOOL-131.11 | 同上但 `uid_policy_signoff = "someone"`(不在名单) | 退出 1 |
| TOOL-131.12 | `uid_policy = "Unverifiable"`(大写) | 退出 1;取值只能是 `"verified"` 或 `"unverifiable"` |
| TOOL-131.13 | 不写 `uid_policy`,UID 为空串 | 退出 1;`MissingVenueUid`(缺省 `verified`) |
| TOOL-131.14 | 同一 venue 上 12 个 Live 账户 | 退出 0 |
| TOOL-131.15 | 同一 venue 上 13 个 Live 账户 | 退出 1;`TooManyAccountsPerVenue` |
| TOOL-131.16 | Sim 账户 `is_prod = true` | 退出 1;`SimIsProd` |
| TOOL-131.17 | Sim 账户写 `venue_uids` | 退出 1;`SimHasUid` |
| TOOL-131.18 | Sim 账户没有 `initial_balances` | 退出 1;`SimCfg` |
| TOOL-131.19 | Sim 账户余额的 venue 为 `okx-swap`(不在其 venues) | 退出 1;`SimCfg` |
| TOOL-131.20 | Sim 账户 `fee_tier = 9`,`fees.toml` 没有档 9 | 退出 1;`MissingFee` |
| TOOL-131.21 | `--sim-models core.toml`(2 个 latency 模型),`latency_model = 2` | 退出 1 |
| TOOL-131.22 | 同上,`latency_model = 1` | 退出 0 |
| TOOL-131.23 | 不带 `--sim-models`,`latency_model = 2` | 退出 0(留给 hft-core 启动时检查) |
| TOOL-131.24 | `bound_instance = "nosuch"` | 退出 1;`DanglingRef` |
| TOOL-131.25 | 两个实例 `name = "pc-main"` | 退出 1;`Strings` |
| TOOL-131.26 | 两个 `is_prod = true` 账户在同一交易所(`binance-usdm` 与 `binance-spot`)登记相同 UID | 退出 1;`DuplicateVenueUid` |
| TOOL-131.27 | 两个 `is_prod = false` 账户在同一交易所不同 `VenueId`(`binance-spot` 与 `binance-usdm`)上登记相同 UID | 退出 0;差异报告(文本与 `--json` 的 `warnings`)恰有一条 `DuplicateVenueUid`,列出两个账户与 UID |
| TOOL-131.32 | 两个 `is_prod = false` 账户在同一 `VenueId`(都为 `binance-usdm`)上登记相同 UID | 退出 1;`DuplicateVenueUid` |
| TOOL-131.33 | 一个 `is_prod = true`、一个 `is_prod = false` 账户在同一交易所不同 `VenueId` 上登记相同 UID | 退出 1;`DuplicateVenueUid` |
| TOOL-131.28 | `assets.toml` 中 AIN 的 `qty_exp = −2`,`insts-bitget-uta-spot.toml` 的 AIN 行另写 `qty_exp = −3` | 退出 1;报告资产 AIN |
| TOOL-131.29 | 某资产 `px_exp + qty_exp = −6` | 退出 0 |
| TOOL-131.30 | 某资产 `px_exp + qty_exp = −5` | 退出 1;`NotionalExp` |
| TOOL-131.31 | `accounts.toml` 某账户写 `leverage = 5` | 退出 1;未知键 `leverage`(永续杠杆不在 TOML) |

### TOOL-132 带 --against 时拒绝修改 refdata 不可变量

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M3
- 前置:`cur.blob` 由 `refdata-basic`(`version = 7`)构建;各行把 TOML 改一处、`version = 8`,`--against cur.blob`
- 实现:`tools/hft-cfg/tests/refdata.rs::tool_132_immutables`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-132.1 | 资产 AIN 的 `qty_exp` −2 → −3 | 退出 1;`AssetChanged` |
| TOOL-132.2 | 某 inst 的合约乘数 10 → 100 | 退出 1;`MultiplierChanged` |
| TOOL-132.3 | 账户 3 的 `kind` live → sim(label 同时改为 `sim-` 前缀并写 Sim 配置,否则先报 `AccountNamePrefix`) | 退出 1;`AccountKindChanged` |
| TOOL-132.4 | 账户 3 的 `is_prod` true → false | 退出 1;`AccountKindChanged` |
| TOOL-132.5 | 账户 3 的 UID 改变 | 退出 1;`UidChanged` |
| TOOL-132.6 | 实例 1 的 `kind` 改为另一种类 | 退出 1;`InstanceChanged` |
| TOOL-132.7 | 实例 1 的 `name` 改变 | 退出 1;`InstanceChanged` |
| TOOL-132.8 | 删除一个 inst 行 | 退出 1;`InstRemoved` |
| TOOL-132.9 | `version = 7`(未递增),其他不变 | 内容与 `cur.blob` 相同 → 退出 0(`AlreadyActive`,幂等) |
| TOOL-132.10 | `version = 7`,新增一个 inst | 退出 1;`VersionNotIncreasing` |
| TOOL-132.11 | 新增一个 inst,`version = 8` | 退出 0 |

### TOOL-133 refdata 差异报告列出增删、绑定变化与能力表逐字段变化

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M3
- 前置:`old.blob` 与 `new.blob`:新增 inst `okx-swap:AIN-USDT-SWAP`、删除账户 9、账户 3 的绑定从 `pc-main` 改为 `pc-b`、`binance-usdm` 能力行 `margin_data.interval_ms` 5000 → 2000、账户 4 的余额上限放宽
- 步骤:
  1. `hft-cfg diff --old old.blob --new new.blob --json`
- 期望:
  - `added.insts = ["okx-swap:AIN-USDT-SWAP"]`、`removed.accounts = [9]`
  - `binding_changes = [{account: 3, from: "pc-main", to: "pc-b"}]`,文本输出含"账户有仓位时 hft-core 会拒绝启动"
  - `caps_changes` 含 (`binance-usdm`, `margin_data.interval_ms`, 5000, 2000)
  - 账户 4 的该字段标为放宽
- 实现:`tools/hft-cfg/tests/diff.rs::tool_133_refdata_report`

### TOOL-134 refdata fetch 生成待审 TOML 并排除不支持的合约

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M3
- 前置:本地替身 HTTP 服务返回录制的 instruments 响应(`fixtures/` 中服务器上实拉的样本):含合约乘数 1000 的 `1000PEPEUSDT`、乘数 3 的合约、一个交割合约、普通永续 `AINUSDT`
- 步骤:
  1. `hft-cfg refdata fetch --venue binance-usdm --out insts-binance-usdm.toml`
- 期望:
  - 输出 TOML 含 `AINUSDT` 与 `1000PEPEUSDT`(`px_unit_shift = 3`),不含乘数 3 的合约与交割合约;同时给出资产指数建议
  - 没有生成任何 `.blob` 文件
- 实现:`tools/hft-cfg/tests/fetch.rs::tool_134_instruments_to_toml`

### TOOL-135 risk blob 往返并在 --against 下给出差异

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M2
- 前置:`params/risk.toml` 夹具(字段归 hft-risk);`cur.blob` 为上一版本
- 步骤:
  1. `hft-cfg build --kind risk --in params/risk.toml --refdata refdata.blob --out r.blob --against cur.blob`
  2. `hft-cfg show r.blob`
- 期望:
  - 退出 0;`r.blob` 与 hft-risk `RiskParams` 构建器对同一输入的输出逐字节相同;`show` 的取值与 TOML 逐项相同;差异报告按账户逐字段标出放宽/收紧
- 实现:`tools/hft-cfg/tests/risk.rs::tool_135_roundtrip_and_diff`

### TOOL-136 engine blob 从 hft-core 配置读三项并写进 ECF1 段 5

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M1
- 前置:`--core-config core.toml` 含 `advisor.mem_budget_mib = 256`、`advisor.checkpoint_max_mib = 64`、`startup.allow_missing_standby = false`;`params/engine.toml` 夹具
- 步骤:
  1. `hft-cfg build --kind engine --in params/engine.toml --core-config core.toml --out e.blob`
- 期望:
  - 退出 0;ECF1 段 5 的 `advisor_mem_budget_mib = 256`、`advisor_ckpt_max_mib = 64`、`allow_missing_standby = 0`
- 实现:`tools/hft-cfg/tests/engine.rs::tool_136_core_config_fields`

### TOOL-137 hft-core 配置缺三项之一时退出 2

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M1
- 前置:同 TOOL-136,各行从 `core.toml` 删去一键
- 实现:`tools/hft-cfg/tests/engine.rs::tool_137_core_config_missing_key`

| 行 | 删去的键 | 期望 |
|---|---|---|
| TOOL-137.1 | `advisor.mem_budget_mib` | 退出 2 |
| TOOL-137.2 | `advisor.checkpoint_max_mib` | 退出 2 |
| TOOL-137.3 | `startup.allow_missing_standby` | 退出 2 |

### TOOL-138 engine 容量变小时差异报告提示缩容规则

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M1
- 前置:`old.blob` 的 ECF1 段 3 `capacities.max_insts` = 4096,`new.blob` 为 2048(其余相同)
- 步骤:
  1. `hft-cfg diff --old old.blob --new new.blob`
- 期望:
  - 输出含该容量字段 4096 → 2048 与提示"缩容只经 Drain + 重启"及 `CapacityShrink`
- 实现:`tools/hft-cfg/tests/diff.rs::tool_138_capacity_shrink_note`

### TOOL-139 send 先预览、确认后带 confirmed 与 expect_state_version 重发

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:integ
- 里程碑:M4
- 前置:替身 ctl socket(按 control-plane 协议应答):首个 `SetParams` 返回 `CmdResult{preview, state_version = 9}`,预览首行为 `account=pc-live kind=Live mode=Shadow`;操作者在 stdin 输入确认
- 步骤:
  1. `hft-cfg send --kind strategy --blob a.blob --account pc-live --socket /tmp/ctl.sock`
- 期望:
  - 替身收到两条命令:第一条不带 `confirmed`;第二条带 `confirmed` 且 `expect_state_version = 9`;主体为 `cli:<当前登录名>`
  - stdout 第一行打印预览首行;最终退出 0
- 实现:`tools/hft-cfg/tests/send.rs::tool_139_preview_then_confirm`

### TOOL-140 send 遇到 StateChanged 时重新预览

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:integ
- 里程碑:M4
- 前置:同 TOOL-139,但替身对第一次确认返回 `StateChanged`,随后的预览返回 `state_version = 10`
- 步骤:
  1. 同 TOOL-139,操作者两次确认
- 期望:
  - 替身收到四条命令:预览、确认(`expect_state_version = 9`)、预览、确认(`expect_state_version = 10`);退出 0
- 实现:`tools/hft-cfg/tests/send.rs::tool_140_state_changed_repreview`

### TOOL-141 send 被拒时打印结果码与 detail 名并退出 1

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:integ
- 里程碑:M4
- 前置:替身对预览返回 `Rejected{RefdataRecheck}`
- 步骤:
  1. `hft-cfg send --kind refdata --blob rd.blob --account pc-live --socket /tmp/ctl.sock`
- 期望:
  - 退出 1;stderr 含 `Rejected` 与 `RefdataRecheck`;替身只收到一条命令
- 实现:`tools/hft-cfg/tests/send.rs::tool_141_rejected_prints_detail`

### TOOL-142 hft-cfg 的输入与环境错误退出 2

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M2
- 前置:同 TOOL-119
- 实现:`tools/hft-cfg/tests/cli.rs::tool_142_input_errors`

| 行 | 命令 | 期望 |
|---|---|---|
| TOOL-142.1 | `--in` 指向不存在的文件 | 退出 2 |
| TOOL-142.2 | `--refdata` 指向不存在的文件 | 退出 2 |
| TOOL-142.3 | `send --kind engine …` | 退出 2(`send` 不接受 engine) |
| TOOL-142.4 | `send …` 而 `--socket` 不存在 | 退出 2 |
| TOOL-142.5 | `--in` 不是合法 TOML | 退出 2 |

### TOOL-143 十进制换算不经 f64

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M2
- 前置:资产 X `qty_exp = −8`;TOML `max_qty = "0.30000001"`(经 f64 会变成 0.30000000999999999)
- 步骤:
  1. 构建 strategy blob
- 期望:
  - 覆盖值 raw = 30000001(0.30000001,`qty_exp = −8`)
- 实现:`tools/hft-cfg/tests/strategy.rs::tool_143_decimal_without_f64`

### TOOL-144 engine 的三项取值不能写进 params

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:unit
- 里程碑:M1
- 前置:同 TOOL-136,另在 `params/engine.toml` 写 `advisor_mem_budget_mib = 128`
- 步骤:
  1. `hft-cfg build --kind engine --in params/engine.toml --core-config core.toml --out e.blob`
- 期望:
  - 退出 1;stderr 指明该键只能来自 `--core-config`;不产出 `e.blob`
- 实现:`tools/hft-cfg/tests/engine.rs::tool_144_core_keys_not_in_params`

### TOOL-145 run 启动时校验验证包,不符退出 2 且不启动子进程

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:验证包夹具 `tools/hft-verify/tests/fixtures/bundle/`(2 个 core fuzz 目标 `binance-usdm`、`okx-swap` 与假 `bin/hft-dst`,`VERIFY.json` 完整);进程启动与 `wait4` 由测试替身提供;`--budget-min 5`
- 实现:`tools/hft-verify/tests/run.rs::tool_145_bundle_validation`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-145.1 | 无改动 | 退出 0;启动了子进程 |
| TOOL-145.2 | `fuzz/binance-usdm` 改一个字节 | 退出 2;没有启动任何子进程;没有新段记录 |
| TOOL-145.3 | `VERIFY.json` 的 `path_digest` 缺 `okx-swap` 组 | 退出 2 |
| TOOL-145.4 | `VERIFY.json` 的 `path_digest` 缺 `dst` 组 | 退出 2 |
| TOOL-145.5 | `VERIFY.json` 的 `files` 列出包内不存在的文件 | 退出 2 |
| TOOL-145.6 | `VERIFY.json` 的 `targets[]` 中 `core = true` 的目标没有二进制 | 退出 2 |

### TOOL-146 预算与状态目录的环境错误退出 2

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:同 TOOL-145.1
- 实现:`tools/hft-verify/tests/run.rs::tool_146_env_errors`

| 行 | 参数或环境 | 期望 |
|---|---|---|
| TOOL-146.1 | `--budget-min 4` | 退出 2 |
| TOOL-146.2 | `--budget-min 5` | 照常运行,退出 0 |
| TOOL-146.3 | `--state` 指向不可写目录 | 退出 2 |

### TOOL-147 同一时刻只有一个子进程

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:同 TOOL-145.1;`--budget-min 60`;替身记录每个子进程的启动与结束时刻(替身墙钟)
- 步骤:
  1. `hft-verify run --budget-min 60`
- 期望:
  - 任意两个子进程的 [启动, 结束] 区间不相交
- 实现:`tools/hft-verify/tests/run.rs::tool_147_serial_children`

### TOOL-148 fuzz 部分按累计 CPU 从少到多轮转并按时间片设 max_total_time

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:包含 3 个 core 目标 A、B、C;本机账本累计 CPU 秒 A = 100、B = 50、C = 0;`--budget-min 60 --fuzz-share 0.5 --slice-min 15`;替身让每个 fuzz 子进程跑满其时间片
- 步骤:
  1. `hft-verify run --budget-min 60 --fuzz-share 0.5 --slice-min 15`
- 期望:
  - fuzz 部分恰两个时间片,顺序 C、B,每个子进程参数 `-max_total_time=900`;之后进入 DST 部分;A 在本次运行中没有时间片
- 实现:`tools/hft-verify/tests/run.rs::tool_148_fuzz_rotation`

### TOOL-149 libFuzzer 的内存与超时参数和环境按规则设置

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:替身 cgroup 读数按行给出;每行启动一个 fuzz 时间片
- 实现:`tools/hft-verify/tests/run.rs::tool_149_libfuzzer_limits`

| 行 | slice `memory.max` / `memory.current` / 本单元当前用量 / 单元 `MemoryMax`(MiB) | 期望子进程参数 |
|---|---|---|
| TOOL-149.1 | 2048 / 1500 / 200 / 1024 | `-rss_limit_mb=684 -malloc_limit_mb=684`(min(748, 1024) − 64) |
| TOOL-149.2 | 4096 / 1000 / 100 / 1024 | `-rss_limit_mb=960 -malloc_limit_mb=960`(min(3196, 1024) − 64) |
| TOOL-149.3 | 任一行 | `-timeout=N` 且 N ≥ 600;环境 `ASAN_OPTIONS` 含 `quarantine_size_mb=16`;`HFT_FUZZ_TIMEOUT_DIR=<state>/crashes/<target>/suspect/` |

### TOOL-150 语料目录首次以包内初始语料起步,之后跨版本保留

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:包内 `corpus/binance-usdm/` 3 个文件;状态目录无 `corpus/`
- 步骤:
  1. 运行一次(替身 fuzz 子进程在语料目录新增 2 个文件)
  2. 换成新版本验证包(初始语料仍为 3 个文件中的 2 个)再运行一次
- 期望:
  - 步骤 1 后 `corpus/binance-usdm/` 有 5 个文件
  - 步骤 2 开始时该目录仍有 5 个文件(不随版本清空),子进程参数中的语料目录是它
- 实现:`tools/hft-verify/tests/run.rs::tool_150_corpus_persists`

### TOOL-151 累计按子进程 CPU 时间而不是墙钟

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:替身让一个 fuzz 时间片墙钟 900 s,`wait4` 返回的 `ru_utime + ru_stime` 按行
- 实现:`tools/hft-verify/tests/accounting.rs::tool_151_cpu_not_wall`

| 行 | `ru_utime` + `ru_stime` | 期望段记录 `cpu_seconds` / `wall_seconds` |
|---|---|---|
| TOOL-151.1 | 900 s + 0 s | 900 / 900 |
| TOOL-151.2 | 400 s + 50 s(一半 CPU) | 450 / 900 |
| TOOL-151.3 | 子进程 2 个线程各 600 s | 1200 / 900 |

### TOOL-152 子进程结果按分类表归为失败、疑似或中止

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:替身按行给出子进程结果;fuzz 目标 `binance-usdm`,DST 种子区间从 `0x80e3050000000000` 起;输入 SHA-1 记为 h
- 实现:`tools/hft-verify/tests/classify.rs::tool_152_classification`

| 行 | 子进程结果 | 期望分类与记录 |
|---|---|---|
| TOOL-152.1 | libFuzzer 写 `crash-h` 后退出 1 | 失败;`crashes/binance-usdm/h` 与 `h.min` 存在;最小化调用参数含 `-minimize_crash=1` 与 60 s 时限 |
| TOOL-152.2 | libFuzzer 写 `leak-h` | 失败;同上 |
| TOOL-152.3 | 输出含 `ERROR: AddressSanitizer` | 失败 |
| TOOL-152.4 | 以 SIGSEGV 结束 | 失败 |
| TOOL-152.5 | 以 SIGABRT 结束 | 失败 |
| TOOL-152.6 | 以 SIGBUS 结束 | 失败 |
| TOOL-152.7 | 以 SIGILL 结束 | 失败 |
| TOOL-152.8 | 以 SIGFPE 结束 | 失败 |
| TOOL-152.9 | 写 `oom-h` | 疑似;`crashes/binance-usdm/suspect/h` 与 `h.min` 存在 |
| TOOL-152.10 | 写 `timeout-h` | 疑似;同上 |
| TOOL-152.11 | harness 把输入 h 写进 `HFT_FUZZ_TIMEOUT_DIR` 后子进程以 0 结束 | 疑似;`suspect/h` 存在 |
| TOOL-152.12 | `hft-dst` 报告种子 S 违反 I3 | 失败;`crashes/dst/<S>.json` 含种子、`I3`、journal 片段路径 |
| TOOL-152.13 | `hft-dst` 以 SIGABRT 结束,`.inflight.json` 的种子为 S | 失败;失败种子 = S |
| TOOL-152.14 | `hft-dst` 以 SIGSEGV 结束 | 失败;种子取自 `.inflight.json` |
| TOOL-152.15 | fuzz 子进程被 SIGKILL | 中止;段记录 `exit = "killed"`;不清零 |
| TOOL-152.16 | fuzz 子进程被 SIGTERM | 中止 |
| TOOL-152.17 | fuzz 子进程被 SIGINT | 中止 |
| TOOL-152.18 | libFuzzer 以退出码 72 结束 | 中止;段记录 `exit = "interrupted"` |
| TOOL-152.19 | `hft-dst` 被 SIGKILL,`.inflight.json` 的种子为 S | 中止;S 不计入 `seeds`;段记录 `rerun_seed = S` |

### TOOL-153 运行的退出码由失败与疑似决定

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:替身按行安排本次运行中各段的结果
- 实现:`tools/hft-verify/tests/run.rs::tool_153_run_exit_code`

| 行 | 本次运行的段结果 | 期望 |
|---|---|---|
| TOOL-153.1 | 全部 ok | 退出 0;`RUN.json` `suspects = 0` |
| TOOL-153.2 | 一个疑似,其余 ok | 退出 0;`RUN.json` `suspects = 1` |
| TOOL-153.3 | 一个失败 | 退出 1;`RUN.json` 退出码 1 |
| TOOL-153.4 | 一个中止(SIGKILL),其余 ok | 退出 0;`RUN.json` `interrupted = true` |

### TOOL-154 fuzz 崩溃只使该目标清零并出现在 crash list

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:账本:A = 3600 CPU 秒、B = 1800;运行 R1(完成于 `2026-11-01T10:00:00Z`)中 A 的段崩溃(输入 h)、B 的段 900 CPU 秒
- 步骤:
  1. 运行 R1
  2. `hft-verify crash list --json`
- 期望:
  - 账本 A `cpu_seconds = 0`、`last_reset = {utc: "2026-11-01T10:00:00Z", run_id: R1, reason: "failure"}`;B = 2700
  - `crash list` 含 `{crash_id: "A/h", status: "failure", run_id: R1, repro: "cargo xtask fuzz A --repro <input>"}`,且该条目的最小化输入大小非 0
- 实现:`tools/hft-verify/tests/ledger.rs::tool_154_crash_resets_target`

### TOOL-155 疑似不清零、CPU 秒照常计入并列为未决

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:账本 A = 3600;运行 R2 中 A 的段 600 CPU 秒,结束时有 `oom-h`
- 步骤:
  1. 运行 R2
- 期望:
  - 退出 0;A `cpu_seconds = 4200`;`suspects` 含 `A/h`;`crash list --status suspect` 列出它
- 实现:`tools/hft-verify/tests/ledger.rs::tool_155_suspect_not_reset`

### TOOL-156 crash resolve --reproduced 后重新折叠按原运行完成时刻清零

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:接 TOOL-155(R2 完成于 `2026-11-02T10:00:00Z`);之后运行 R3 给 A 加 300 CPU 秒(完成于 `2026-11-03T10:00:00Z`)
- 步骤:
  1. `hft-verify crash resolve A/h --reproduced --evidence https://ci.example/run/1`
  2. `hft-verify ledger rebuild`
- 期望:
  - `resolutions/A_h.json` 存在,含证据 URL `https://ci.example/run/1` 与操作者
  - 重建后 A `cpu_seconds = 300`(R2 完成时清零,R3 之后累计)、`last_reset.utc = "2026-11-02T10:00:00Z"`;`suspects` 不再含 `A/h`;`crash list` 中该条目 `status = "failure"`
- 实现:`tools/hft-verify/tests/ledger.rs::tool_156_resolve_reproduced`

### TOOL-157 crash resolve --not-reproduced 标为 dismissed 且累计不变

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:接 TOOL-155 与 R3
- 步骤:
  1. `hft-verify crash resolve A/h --not-reproduced --evidence https://ci.example/run/2`
  2. `hft-verify ledger rebuild`
- 期望:
  - A `cpu_seconds = 4500`;`suspects` 不含 `A/h`;`crash list` 中该条目 `status = "dismissed"`
- 实现:`tools/hft-verify/tests/ledger.rs::tool_157_resolve_not_reproduced`

### TOOL-158 DST 违反或 hft-dst 致命信号使 DST 清零而区间保留

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:账本 DST `seeds = 50000`、`runs = 5`、`utc_days = 5`、`ranges = [[0x80e3050000000000, 0x80e305000000c34f]]`;本次运行 DST 段按行
- 实现:`tools/hft-verify/tests/ledger.rs::tool_158_dst_reset`

| 行 | 本次 DST 段 | 期望 |
|---|---|---|
| TOOL-158.1 | 种子 `0x80e305000000c350`..`+999` 中 `+10` 违反 I7 | `seeds = 0`、`runs = 0`、`utc_days = 0`;`ranges = [[0x80e3050000000000, 0x80e305000000c737]]`(旧区间与本次区间相接,合并);`last_reset.reason = "failure"` |
| TOOL-158.2 | `hft-dst` 以 SIGSEGV 结束,`.inflight.json` 种子 `+10` | 同上;`crashes/dst/` 有该种子的条目 |
| TOOL-158.3 | 1000 个种子零违反 | `seeds = 51000`、`runs = 6`;`ranges` 合并为一个区间 |

### TOOL-159 被中止的 DST 种子不计入并在下一次运行最先重跑

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:运行 R4 中 `hft-dst` 在种子 S 上被 SIGKILL(`.inflight.json` = S),此前已完成的 100 个种子写在段记录里
- 步骤:
  1. 运行 R4
  2. 运行 R5
- 期望:
  - R4 后 DST `seeds` 增加 100,不含 S;R4 最后一段 `rerun_seed = S`
  - R5 的 DST 部分第一个运行的种子是 S;R5 之后 S 计入一次
- 实现:`tools/hft-verify/tests/run.rs::tool_159_rerun_interrupted_seed`

### TOOL-160 收到 SIGTERM 时让子进程收尾并按实际 CPU 写最后一段后退出 0

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:fuzz 时间片进行到 300 s(子进程已用 280 CPU 秒)
- 步骤:
  1. 向 `hft-verify` 发 SIGTERM
- 期望:
  - 子进程收到 SIGINT,没有收到 SIGKILL;它在 3 s 内退出后,最后一段记录 `cpu_seconds = 280`、`exit = "interrupted"`;`hft-verify` 退出 0
- 实现:`tools/hft-verify/tests/run.rs::tool_160_sigterm_graceful`

### TOOL-161 被强杀时只计已写出的段

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:运行 R6 已写出段 0、1;段 2 的临时文件已写一半
- 步骤:
  1. 对 `hft-verify` 发 SIGKILL;再运行 `hft-verify ledger rebuild`
- 期望:
  - `runs/R6/` 只有 `0.json`、`1.json`,没有 `RUN.json` 与半个文件;重建的累计只含段 0、1 的量
- 实现:`tools/hft-verify/tests/run.rs::tool_161_sigkill_partial`

### TOOL-162 漏跑一次时账本不变

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:账本状态 L0;定时器本应触发的一次运行没有发生(无运行记录)
- 步骤:
  1. `hft-verify ledger rebuild`
- 期望:
  - 重建结果除 `rebuilt_from` 外与 L0 逐字节相同
- 实现:`tools/hft-verify/tests/ledger.rs::tool_162_missed_run_no_change`

### TOOL-163 删掉账本后重建得到逐字节相同的结果

- 规则:[hft-verify](README.md#hft-verify)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M6
- 前置:状态目录有 10 次运行与 2 个 resolution;`run` 结束时写出的 `ledger.json` 为 L1
- 步骤:
  1. 删除 `ledger/ledger.json`;运行 `hft-verify ledger rebuild`
- 期望:
  - 新 `ledger.json` 去掉 `rebuilt_from` 后与 L1 逐字节相同;`rebuilt_from` = `{hosts: [本机], records: 10 次运行的段数之和, earliest, latest}`;stderr 有一行告警
- 实现:`tools/hft-verify/tests/ledger.rs::tool_163_rebuild_identical`

### TOOL-164 两台主机的记录按完成时刻折叠且种子区间去重

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:主机 `dev2`(`host_tag = 0xe305`)与 `tokyo-cex-1`(`host_tag = 0xd103`)各有运行记录;`--runs-from` 给出另一台主机的记录目录
- 实现:`tools/hft-verify/tests/ledger.rs::tool_164_multi_host_fold`

| 行 | 记录 | 期望 |
|---|---|---|
| TOOL-164.1 | dev2 种子 `0x80e3050000000000`..+999,tokyo 种子 `0x80d1030000000000`..+999 | `seeds = 2000`;`ranges` 两个区间;`hosts = ["dev2", "tokyo-cex-1"]` |
| TOOL-164.2 | 同上,tokyo 的运行完成更早且含 DST 违反 | 清零发生在 tokyo 运行的完成时刻;其后完成的 dev2 运行照常累计(`seeds = 1000`) |
| TOOL-164.3 | 两台主机碰撞成同一 `host_tag`,区间各 0..999 与 500..1499 | 重叠的 500 个种子只计一次(`seeds = 1500`) |

### TOOL-165 运行与 UTC 日的计数口径

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:各行的运行都含 DST 段(另注明者除外)
- 实现:`tools/hft-verify/tests/ledger.rs::tool_165_runs_and_days`

| 行 | 运行 | 期望 |
|---|---|---|
| TOOL-165.1 | 三次运行,`RUN.json.end_utc` 都在 `2026-11-05` | `runs = 3`、`utc_days = 1` |
| TOOL-165.2 | 一次运行 `end_utc = 2026-11-05T23:59:59Z`,一次 `2026-11-06T00:00:00Z` | `utc_days = 2` |
| TOOL-165.3 | 一次运行只有 fuzz 段(没有新种子) | 不计入 `runs` 与 `utc_days` |
| TOOL-165.4 | 被强杀的运行没有 `RUN.json`,最后一段 `end_utc = 2026-11-07T00:10:00Z` | 运行的 UTC 日 = `2026-11-07`,折叠位置按该时刻 |

### TOOL-166 --candidate 只累计 path_digest 相同的段并单列候选版本自身的运行

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:版本 v1、v2 的验证包;`binance-usdm` 组的 digest:v1 = D1,v2 = D1(相同);`okx-swap` 组:v1 = E1,v2 = E2(不同);按完成时刻依次为:v1 的运行给两个目标各 3600 CPU 秒(最后一次 v1 运行中 `okx-swap` 的段崩溃,其 `binance-usdm` 段已计入这 3600),之后 v2 的运行给两个目标各 1800
- 实现:`tools/hft-verify/tests/ledger.rs::tool_166_candidate_view`

| 行 | 命令 | 期望 |
|---|---|---|
| TOOL-166.1 | `ledger show --candidate v2` | `binance-usdm` = 5400(两版 digest 相同,合并) |
| TOOL-166.2 | 同上 | `okx-swap` = 1800(只含 E2 的段;v1 的崩溃不使它清零) |
| TOOL-166.3 | 同上 | 单列 v2 自身的运行:每个目标 1800,零失败 |
| TOOL-166.4 | `ledger show`(不带候选) | `okx-swap` 顶层 `cpu_seconds = 1800`、`last_reset.reason = "failure"`;`binance-usdm` 顶层 = 5400 |

### TOOL-167 种子编码由主机名哈希与本机已用区间决定

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:替身 `gethostname` 返回各行的主机名
- 实现:`tools/hft-verify/tests/seeds.rs::tool_167_seed_encoding`

| 行 | 主机名与本机记录 | 期望本次 DST 区间起点 |
|---|---|---|
| TOOL-167.1 | `dev2`,无记录 | `0x80e3050000000000` |
| TOOL-167.2 | `dev2`,已用区间到 `0x80e30500000003e7` | `0x80e30500000003e8` |
| TOOL-167.3 | `tokyo-cex-1`,无记录 | `0x80d1030000000000` |
| TOOL-167.4 | 任一行 | 起点的第 63 位为 1,第 56–62 位为 0 |

### TOOL-168 段记录与 RUN.json 的字段与格式

- 规则:[hft-verify](README.md#hft-verify)
- 级别:golden
- 里程碑:M6
- 输入:替身脚本:一次运行含一个 fuzz 时间片(`binance-usdm`,600 CPU 秒,一个疑似)与一个 DST 批次(100 个种子零违反),主机 `dev2`,开始 `2026-11-05T01:00:00Z`
- 期望文件:`tools/hft-verify/tests/fixtures/expected/dev2-20261105T010000Z/{0.json,1.json,RUN.json}`(键排序、`format = 1`;fuzz 段含 `target`、`execs`、`crash_sha1 = []`、`suspect_sha1` 一项;DST 段含 `seed_from`、`seed_to`、`seeds = 100`、`steps`、`violations = 0`、`failed_seeds = []`、`rerun_seed = null`;两段与 `RUN.json` 均有 `skipped = null`;`RUN.json` 含段数 2、`interrupted = false`、`suspects = 1`、退出码 0)
- 实现:`tools/hft-verify/tests/record.rs::tool_168_record_format`

### TOOL-169 crash list 的字段、过滤与复现命令

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:`crashes/` 下一个 fuzz 失败 `binance-usdm/h1`、一个疑似 `okx-swap/h2`、一个 dismissed `okx-swap/h3`、一个 DST 失败 `dst/0x80e3050000000010`(套件 `core`,`steps = 10000`)
- 步骤:
  1. `hft-verify crash list --json`
  2. `hft-verify crash list --status suspect --json`
- 期望:
  - 步骤 1 输出 4 项,每项含 `crash_id`、`status`、首次出现 UTC、`run_id`、验证包版本、输入 SHA-1、最小化大小、复现命令;DST 项的复现命令为 `cargo xtask dst --seeds 1 --seed 0x80e3050000000010 --suite core --steps 10000`
  - 步骤 2 只输出 `okx-swap/h2`
- 实现:`tools/hft-verify/tests/crash.rs::tool_169_crash_list`

### TOOL-170 ledger show 输出累计、清零、疑似、分项与预计达成时刻

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:验证包 `VERIFY.json.gates`:fuzz 每目标 7200 CPU 秒、DST 200000 种子;目标 A 的最近 7 次运行各贡献 600 CPU 秒,最早开始 `2026-11-01T00:00:00Z`、最晚结束 `2026-11-08T00:00:00Z`(7 天),当前累计 4200
- 步骤:
  1. `hft-verify ledger show --json`
- 期望:
  - A 的吞吐 = 4200 秒 / 7 天 = 600 秒/天;剩余 3000 秒 → `eta.fuzz.A = "2026-11-13T00:00:00Z"`
  - 输出含 A 的 `cpu_seconds = 4200`、`last_reset`、`by_digest`、`by_version`、DST 的 `seeds`、`runs`、`utc_days`、未决疑似列表
  - 把 `VERIFY.json.gates` 中 fuzz 门槛改为 4200 后再运行:`eta.fuzz.A = null`(门槛取自验证包,不是硬编码)
- 实现:`tools/hft-verify/tests/ledger.rs::tool_170_ledger_show`

### TOOL-171 hft-verify 没有网络也能完成全部用例

- 规则:[hft-verify](README.md#hft-verify)
- 级别:proc
- 里程碑:M6
- 前置:CI 中以 `unshare -n`(无网络命名空间)运行 `tools/hft-verify` 的全部测试
- 步骤:
  1. `unshare -n cargo test -p hft-verify`
- 期望:
  - 全部测试通过;`deps` 中 `tools/hft-verify` 的闭包不含 `reqwest`、`hyper`、`tokio`(见 TOOL-031.39–TOOL-031.41)
- 实现:`tools/hft-verify/tests/netns.rs::tool_171_runs_without_network`

### TOOL-172 段记录的 path_digest 取该目标组在本验证包里的值

- 规则:[hft-verify](README.md#hft-verify)
- 级别:unit
- 里程碑:M6
- 前置:`VERIFY.json.path_digest = {binance-usdm: D1, okx-swap: E1, dst: F1}`
- 步骤:
  1. 运行一次(含 `okx-swap` 时间片与 DST 批次)
- 期望:
  - `okx-swap` 段 `path_digest = E1`;DST 段 `path_digest = F1`;每段 `bundle_version`、`bundle_sha256` 等于本包
- 实现:`tools/hft-verify/tests/record.rs::tool_172_segment_digest`

### TOOL-173 驱动自身内存低于 32 MiB

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M6
- 基准名:`hft-verify/driver_rss`
- 实现:`tools/hft-verify/benches/driver.rs::tool_173_driver_rss`

### TOOL-174 hostprobe collect 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`hostprobe/collect`
- 实现:`tools/hostprobe/benches/collect.rs::tool_174_collect_time`

### TOOL-175 hft-cfg 编码 500 个 group 的 strategy blob 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`hft-cfg/build_strategy_500`
- 实现:`tools/hft-cfg/benches/build.rs::tool_175_build_strategy_500`

### TOOL-176 decoupling-gate 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`xtask/decoupling_gate`
- 实现:`tools/xtask/benches/decoupling_gate.rs::tool_176_decoupling_gate_time`

### TOOL-177 hft-verify 在 hftbatch.slice 内运行且同一时刻只有一个子进程

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M6
- 环境:dev2,已安装验证包与 `hft-verify.service`
- 授权:无
- 步骤:`sudo systemctl start hft-verify.service`,运行期间每 10 s 采样一次单元的 cgroup 与进程树
- 通过标准:每次采样 `hft-verify` 的 cgroup 路径在 `/hftbatch.slice/` 下;`hft-verify` 的子进程数 ≤ 1;单元 `MemoryPeak` 不超过 hft-core 内存表的 `hft-verify` 行
- 证据:采样日志与 `systemctl show hft-verify.service -p MemoryPeak`,文件名含 `TOOL-177`
- 实现:`tests/host/TOOL-177.sh`

### TOOL-178 doc-check 核对 hft-verify 门槛表与 testing §8 一致

- 规则:[hft-verify](README.md#hft-verify)、[cargo xtask doc-check](README.md#cargo-xtask-doc-check)
- 级别:table
- 里程碑:M6
- 前置:夹具 `docs-clean` 的 `tools/hft-verify/gates.toml` 与 testing §8 的 ③④ 阈值一致(fuzz 每目标 7200 CPU 秒、DST 200000 种子、7 次运行、7 个 UTC 日)
- 实现:`tools/xtask/tests/doc_check.rs::tool_178_verify_gates_match_testing`

| 行 | 改动 | 期望 |
|---|---|---|
| TOOL-178.1 | 无改动 | `doc-check` 退出 0 |
| TOOL-178.2 | `gates.toml` 的 fuzz 阈值改为 3600 | 退出 1;报告 `gates.toml` 与 testing §8 ③ 不一致 |
| TOOL-178.3 | `gates.toml` 的 DST 运行天数改为 6 | 退出 1 |

### TOOL-179 gen-schema 拒绝策略 kind_id、Note 码、命令 op 与检查点节 tag 的消失或复用

- 规则:[cargo xtask gen-schema](README.md#cargo-xtask-gen-schema)
- 级别:table
- 里程碑:M2
- 前置:夹具 `events` 已生成并提交 `schemas/event-layout.lock`,其中策略种类 `1 → probe-a`,其 Note 码 `5 → ProbeEdge`、命令 `op 1 → set_thr`、检查点节 `tag 2 → path`;按行变化的是 `hft-core schemas --out` 导出读到的策略行(测试直接把它们注入 gen-schema 模型,不运行二进制);基线为该夹具第一次生成的锁文件
- 实现:`tools/xtask/tests/gen_schema.rs::tool_179_strategy_append_only`
- 备注:引擎侧编号(kind、`CmdKind`、`LoadErrCode`、引擎 Note 码)、基线选择与"违反不写"由 hft-event 的 EVT-174–EVT-180 覆盖,本用例只测经 `hft-core schemas` 导出读取的策略四项

| 行 | 导出的变化 | 期望 |
|---|---|---|
| TOOL-179.1 | 无变化 | 退出 0 |
| TOOL-179.2 | 追加种类 `2 → probe-b` | 退出 0;锁文件多一行 `2 → probe-b` |
| TOOL-179.3 | 种类 1 改名为 `probe-x` | 退出 1;报告 `kind_id 1` 被复用 |
| TOOL-179.4 | 种类 1 消失 | 退出 1;报告 `kind_id 1` 消失 |
| TOOL-179.5 | 种类 1 的 Note 码 5 改名为 `ProbeOther` | 退出 1;报告 `(1, Note 5)` |
| TOOL-179.6 | 种类 1 追加 Note 码 6 | 退出 0 |
| TOOL-179.7 | 种类 1 的命令 `op 1` 改名为 `set_win` | 退出 1;报告 `(1, op 1)` |
| TOOL-179.8 | 种类 1 的检查点节 `tag 2` 消失 | 退出 1;报告 `(1, tag 2)` |
| TOOL-179.9 | 种类 1 新增 Note 码 0 | 退出 1;报告码 0 |
| TOOL-179.10 | 第 3 至 9 行任一违反时 | `python/_generated/`、`docs/generated/` 与锁文件的 SHA-256 与运行前相同 |

### TOOL-180 test-trace 对可选扩展用例与 Python 测试函数的处理

- 规则:[其他 xtask 子命令](README.md#其他-xtask-子命令)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:夹具 `trace-clean`(见 TOOL-086),`current_milestone = "M2"`;另加 `crates/a/TESTS.md` 的 `AA-010`(unit,M2,`状态:可选扩展:incr-depth`,实现 `crates/a/tests/x.rs::aa_010_incr`)与 `python/p/TESTS.md` 的 `PP-001`(unit,M2,实现 `python/p/tests/test_x.py::test_pp_001_ok`);按行改动
- 实现:`tools/xtask/tests/test_trace.rs::tool_180_optional_and_python`

| 行 | `enabled_extensions` 与改动 | 期望 |
|---|---|---|
| TOOL-180.1 | `[]`;代码中没有 `aa_010_incr`;有 `test_pp_001_ok` | 退出 0;三张清单为空 |
| TOOL-180.2 | `["incr-depth"]`;代码中没有 `aa_010_incr` | 退出 0;`unimplemented = ["AA-010"]` |
| TOOL-180.3 | `["incr-depth"]`;代码中有 `aa_010_incr` | 退出 0 |
| TOOL-180.4 | `[]`;代码中有 `aa_010_incr` | 退出 0(规格中存在该编号,不算多余) |
| TOOL-180.5 | 删除 `test_pp_001_ok` | 退出 0;`unimplemented = ["PP-001"]` |
| TOOL-180.6 | 加 `def test_pp_099_orphan()` | 退出 0;`extra` 含 `test_pp_099_orphan` |
| TOOL-180.7 | 加 `def pp_098_helper()`(无 `test_` 前缀、不是 pytest 测试) | 退出 0 |
| TOOL-180.8 | `test_pp_001_ok` 移到 `python/p/tests/test_y.py`,规格不改 | 退出 0;`PP-001` 在 `inconsistent` |

### TOOL-181 hft-secrets --check 只校验清单,不访问网络、不写文件

- 规则:[hft-secrets](README.md#hft-secrets)
- 级别:table
- 里程碑:M3
- 前置:替身端点记录请求;没有实例角色(IMDS 替身不可达);输出目录 `/tmp/s` 不存在;每行一份清单,运行 `hft-secrets --check --manifest <清单>`
- 实现:`tools/hft-secrets/tests/manifest.rs::tool_181_check_mode`

| 行 | 清单 | 期望 |
|---|---|---|
| TOOL-181.1 | 两项,只含 `name`、`secret_id`(`arn:aws:secretsmanager:…`)、`json_key`、`kind = "plain"` | 退出 0;替身端点与 IMDS 替身没有收到任何请求;`/tmp/s` 不存在 |
| TOOL-181.2 | 某项多一个键 `value = "S3CR3T"` | 退出 2;stderr 不含 `S3CR3T` |
| TOOL-181.3 | 两项 `name` 相同 | 退出 2 |
| TOOL-181.4 | `kind = "env"` | 退出 2 |
| TOOL-181.5 | `secret_id = "hft/live/a"`(不是 ARN) | 退出 2 |
| TOOL-181.6 | `name = ""` | 退出 2 |
| TOOL-181.7 | 清单文件不存在 | 退出 2 |

### TOOL-182 strategy blob 的 group_ctr_hwm 只增不减,缺 --against 只接受第一版

- 规则:[hft-cfg](README.md#hft-cfg)
- 级别:table
- 里程碑:M2
- 前置:同 TOOL-119;生效 blob `cur.blob`(`version = 7`)含 n = 101、103,`group_ctr_hwm = 110`(n = 105–110 已在更早版本移除);各行 TOML 的 `version = 8` 并带 `--against cur.blob`,另注明者除外;`hft-cfg show` 读出新 blob 的头
- 实现:`tools/hft-cfg/tests/strategy.rs::tool_182_group_ctr_hwm`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-182.1 | 保留 101、103,不新增 | 退出 0;新 blob `group_ctr_hwm = 110` |
| TOOL-182.2 | 新增 `n = 110` | 退出 1;报告 `n = 110` 不大于 H = 110 |
| TOOL-182.3 | 新增 `n = 111` | 退出 0;`group_ctr_hwm = 111` |
| TOOL-182.4 | 新增 `n = 109` | 退出 1 |
| TOOL-182.5 | 移除 103,不新增 | 退出 0;`group_ctr_hwm = 110` |
| TOOL-182.6 | 生效 blob 的 `group_ctr_hwm` 改为 0,含 101、103;新增 `n = 102` | 退出 1;`--against` 不是合法 SPB1(`parse_spb1` 要求 `group_ctr_hwm` ≥ 每个 counter,`BadGroup`),所以 H 恒等于生效 blob 的 `group_ctr_hwm` |
| TOOL-182.7 | 不带 `--against`,`version = 1`,group n = 1、2 | 退出 0;`group_ctr_hwm = 2` |
| TOOL-182.8 | 不带 `--against`,`version = 2` | 退出 2;不产出 blob |
| TOOL-182.9 | 同一输入编码两次 | 两个 blob 逐字节相同(含 `group_ctr_hwm`) |

### TOOL-183 slice 剩余内存不足时跳过本次运行并记 insufficient_memory

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:替身 cgroup 读数按行给出(MiB:slice `memory.max` / `memory.current` / 本单元当前用量 / 单元 `MemoryMax` = 1024);余量 64 MiB;账本中目标 `binance-usdm` 累计 3600 CPU 秒;`--budget-min 30`
- 实现:`tools/hft-verify/tests/run.rs::tool_183_insufficient_memory`

| 行 | 读数 | 期望 |
|---|---|---|
| TOOL-183.1 | 2048 / 2048 / 64 / 1024(限额 = 64 − 64 = 0) | 没有启动任何子进程;段记录 `skipped = "insufficient_memory"`、`cpu_seconds = 0`;`RUN.json.skipped = "insufficient_memory"`;退出 0;账本 `binance-usdm` 仍为 3600 CPU 秒、`last_reset` 不变 |
| TOOL-183.2 | 2048 / 2048 / 0 / 1024(限额 = −64) | 同 TOOL-183.1 |
| TOOL-183.3 | 2048 / 2048 / 65 / 1024(限额 = 1) | 启动 fuzz 子进程,`-rss_limit_mb=1 -malloc_limit_mb=1`;段记录 `skipped = null` |
| TOOL-183.4 | TOOL-183.1 之后 `ledger show` | 该运行不计入 DST 的运行数与 UTC 日数 |

### TOOL-184 收到 SIGTERM 后子进程 10 s 内不退出则 SIGKILL

- 规则:[hft-verify](README.md#hft-verify)
- 级别:table
- 里程碑:M6
- 前置:fuzz 时间片进行中,子进程已用 280 CPU 秒;替身子进程按行在收到 SIGINT 后经过所列时长退出(或忽略 SIGINT);时钟可注入
- 实现:`tools/hft-verify/tests/run.rs::tool_184_sigterm_escalation`

| 行 | 子进程对 SIGINT 的反应 | 期望 |
|---|---|---|
| TOOL-184.1 | 9.999 s 后退出 | 没有发 SIGKILL;最后一段 `exit = "interrupted"`;退出 0 |
| TOOL-184.2 | 10 s 时退出 | 没有发 SIGKILL;同上 |
| TOOL-184.3 | 忽略 SIGINT | 在 SIGINT 后 10 s 发 SIGKILL;最后一段 `exit = "killed"`、按实际 CPU 秒计;退出 0;该段不分类为失败 |
| TOOL-184.4 | DST 批次中忽略 SIGINT | 同 TOOL-184.3;进行中标记里的种子记为 `rerun_seed` |

### TOOL-185 bench-gate 的预算缺失、report 类超预算与策略基准入口

- 规则:[cargo xtask bench-gate](README.md#cargo-xtask-bench-gate)
- 级别:table
- 里程碑:M1.5
- 前置:记录型执行器扮演基准运行,按行给出结果;夹具 `budgets.toml` 中 `a/x` 为 `gate.kind = "absolute"`、`a/y` 为 `gate.kind = "report"`;夹具策略 crate `strategies/s` 登记于 registry;以 `--reference` 运行时 `hostprobe verify` 替身返回 0
- 实现:`tools/xtask/tests/bench_gate.rs::tool_185_budget_kinds`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-185.1 | `--reference`;已登记基准 `a/z` 在 `budgets.toml` 没有预算 | 退出 1;报告 `a/z` 缺预算 |
| TOOL-185.2 | `--reference`;`a/y` 超出预算 | 退出 0;JSON 中 `a/y` 的 `verdict` 标为超出 |
| TOOL-185.3 | `--reference`;`a/x` 超出预算 | 退出 1 |
| TOOL-185.4 | `--smoke`;`a/x` 超出预算 | 退出 0(冒烟不读门槛) |
| TOOL-185.5 | `--smoke`;`strategies/s/benches/` 缺 `strategies/s/advice` | 无声明退出 1 并列出缺名;not-applicable.toml 提供非空理由时退出 0 并报告理由;必需入口、空理由、拼错名称或已有基准的排除声明均拒绝 |
| TOOL-185.6 | `--smoke`;`strategies/s/benches/` 提供 SDK 规定的全部入口 | 退出 0 |

### TOOL-186 ci --no-doc-check 跳过 doc-check 步

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)
- 级别:unit
- 里程碑:M1
- 前置:记录型执行器对每步返回 0
- 步骤:
  1. 运行 `cargo xtask ci --no-doc-check`
- 期望:
  - 退出 0;步骤序列为 TOOL-004 的前十一步(没有 `xtask doc-check` 调用);stderr 的摘要把 doc-check 步记为跳过
- 实现:`tools/xtask/tests/ci.rs::tool_186_no_doc_check_skips_doc_check`

### TOOL-187 开发构建要求实际通过的 CI,不读取分钟用量

- 规则:[发布检查](../docs/development.md#发布工作流release-aarch64yml)
- 级别:table
- 里程碑:M1
- 前置:候选 SHA 为已推送分支头;GitHub 替身只允许查询该 SHA 的 CI、解耦结论
- 实现:`tools/xtask/tests/test_ci_release.py::test_tool_187_dev_requires_executed_ci_without_usage_scan`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-187.1 | CI acceptance 成功,最近解耦检查成功 | 退出 0;输出两个证据 ID;不查询每夜与月度用量 |
| TOOL-187.2 | CI acceptance 跳过或失败 | 退出 1;不输出发布参数 |
| TOOL-187.3 | 最近解耦检查失败,之前成功 | 退出 1 |
| TOOL-187.4 | GitHub API 错误 | 退出 2 |

### TOOL-188 正式版 Admin 覆盖只影响指定检查

- 规则:[发布检查](../docs/development.md#发布工作流release-aarch64yml)
- 级别:table
- 里程碑:M1
- 前置:GitHub 结论替身;候选为正式版
- 实现:`tools/xtask/tests/test_ci_release.py::test_tool_188_admin_nightly_override_preserves_other_gates`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-188.1 | 无覆盖,各检查成功 | 退出 0;记录 nightly 运行 ID |
| TOOL-188.2 | 无覆盖,每夜失败 | 退出 1 |
| TOOL-188.3 | 非 Admin 请求 `skip_nightly` 或 `skip_verify` | 退出 1 |
| TOOL-188.4 | Admin 请求 `skip_nightly`,每夜失败,其他检查成功 | 退出 0;不查询每夜;记录覆盖名与操作人 |
| TOOL-188.5 | 同上,但 acceptance 跳过、最近解耦失败或 SHA 不在 main | 均退出 1 |
| TOOL-188.6 | Admin 请求 `skip_verify`,检查全通过 | 退出 0;输出 `skip_verify = true`,记录覆盖名与操作人 |

### TOOL-189 正式版读取最近实际执行的每夜结论

- 规则:[发布检查](../docs/development.md#发布工作流release-aarch64yml)
- 级别:table
- 里程碑:M1
- 前置:main 每夜列表按时间逆序;最新一次跳过,再上一次按行给出,更早一次成功
- 实现:`tools/xtask/tests/test_ci_release.py::test_tool_189_latest_executed_nightly_must_pass`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-189.1 | 最近实际执行成功 | 返回该次运行 ID |
| TOOL-189.2 | 最近实际执行失败或被取消 | 拒绝;不退回更早的成功记录 |
| TOOL-189.3 | 没有执行记录 | 拒绝 |

### TOOL-190 每夜摘要直接汇总作业结果

- 规则:[每夜任务](../docs/development.md#每夜任务nightlyyml)
- 级别:table
- 里程碑:M1
- 前置:无额度环境变量;x86 成功,ARM 失败且缓存未命中
- 实现:`tools/xtask/tests/test_ci_release.py::test_tool_190_nightly_summary_reports_jobs_without_quota_state`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-190.1 | 文件回放成功,解耦豁免查询成功 | 退出 0;列出三项作业结果、缓存提示和豁免;没有分钟额度字段 |
| TOOL-190.2 | 豁免查询 API 错误 | 退出 0;保留作业结果,豁免项标为 unavailable |
| TOOL-190.3 | 文件回放失败 | 退出 0;日期文件回放明确显示 failure |
| TOOL-190.4 | 文件回放取消 | 退出 0;日期文件回放明确显示 cancelled |
| TOOL-190.5 | 缺少文件回放结果 | 退出 0;日期文件回放显示 ?,不能报告成功 |

### TOOL-191 主机与基准检查按适用路径触发

- 规则:[质量门](../docs/development.md#质量门quality-gatesyml)
- 级别:table
- 里程碑:M1
- 前置:路径列表替身与真实临时 Git 仓库;运行 changed-paths 入口,读取工作流输出
- 期望:带 `benches/` 的 hot 包匹配 `deps.toml` 的精确/通配规则,策略源码或基准改动触发其实际包名;Markdown、模板、无基准目录与已删除包不触发基准。手动完整通道始终执行基准短跑,普通推送保留路径选择。
- 期望:子命令清单仅在 `deploy/gen` 已落地时启用 `gen-host`;M2 缺失时报告尚未落地,M3 缺失时失败。`gen_host` 路径判定与 deploy 容器测试不受该实现状态影响。
- 实现:`tools/xtask/tests/test_ci_paths.py::test_tool_191_host_checks_follow_code_and_config_changes`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-191.1 | deploy 的 Markdown + OMS 源码;或 hft-core 的 Markdown + CI 工作流 | `gen_host = false`;`docs_only = false`;OMS 的回放与脚手架检查仍触发 |
| TOOL-191.2 | 只改 deploy/hft-core 的 Markdown | `gen_host = false`;`docs_only = true` |
| TOOL-191.3 | deploy 主机 TOML、生成器、systemd 单元;或 hft-core 的 Rust/Cargo.toml | `gen_host = true`;`docs_only = false`;夹带 Markdown 不影响结果 |
| TOOL-191.4 | PR 与 push:把主机 TOML 从 deploy 移到 docs,同时删除 hft-core 源文件 | 路径列表含改名两端与被删源码;`gen_host = true`;不因旧路径已不存在漏跑 |

### TOOL-192 replay-verify 调用真实入口并映射退出码

- 规则:[cargo xtask replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:table
- 里程碑:M2
- 前置:本地样本路径与 ScriptExec
- 步骤:模拟 replayer 返回 0/1/2/3/4/进程异常;传入 from/to;另传入未支持的直方图选项
- 期望:通过 locked ci 剖面调用 replayer verify,原路径与窗口逐参数传递;0→0,1/3/4→1,2/异常→2;未支持选项返回 2 且不启动子进程
- 实现:`tools/xtask/tests/replay_verify.rs::tool_192_replay_wrapper_runs_binary_and_maps_exit_codes`

| 行 | 应用退出或参数 | 包装退出 |
|---|---|---|
| TOOL-192.1 | 0 | 0 |
| TOOL-192.2 | 1 | 1 |
| TOOL-192.3 | 2 | 2 |
| TOOL-192.4 | 3 | 1 |
| TOOL-192.5 | 4 | 1 |
| TOOL-192.6 | 进程异常 | 2 |
| TOOL-192.7 | --record-decide-hist | 2,无子进程 |

### TOOL-193 缓存每次使用前重新校验

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)
- 级别:integ
- 里程碑:M3
- 前置:一个带二进制内容的合成对象与对应指针
- 步骤:获取合格对象;清空远端替身再运行;分别将缓存改成同长度错误内容与截断内容,在远端不可读及恢复可读时重跑
- 期望:合格缓存不调用 AWS 或 zstd;坏缓存删除,失败退出 2 且不留下缓存;恢复后重新下载与真实解压,最终字节等于固定输入
- 实现:`tools/xtask/tests/fixtures_fetch.rs::tool_193_fetch_revalidates_cache_and_reuses_only_exact_bytes`

### TOOL-194 全部指针先预检再产生副作用

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)
- 级别:table
- 里程碑:M3
- 前置:首个合法指针有错误旧缓存,随后指针分别包含错误协议、空桶、错误前缀、父路径、空路径段、错误扩展、查询参数、空白、错误哈希字符/长度、负数/零/非整数/缺大小、非法 TOML;另测非法名称、空目录与额外 CLI 参数
- 期望:全部非法输入在任何下载和缓存删除之前退出 2,原缓存不变;非法名称不创建 target;空集合返回 0 且不创建 target;多余 CLI 参数退出 2
- 实现:`tools/xtask/tests/fixtures_fetch.rs::tool_194_fetch_preflights_all_pointers_before_side_effects`

| 行 | 独立输入 | 期望 |
|---|---|---|
| TOOL-194.1 | https 协议 | 退出 2,无副作用 |
| TOOL-194.2 | 空桶名 | 同上 |
| TOOL-194.3 | 非 hft-fixtures 前缀 | 同上 |
| TOOL-194.4 | 父路径分量 | 同上 |
| TOOL-194.5 | 空路径分量 | 同上 |
| TOOL-194.6 | 非 zst 后缀 | 同上 |
| TOOL-194.7 | 查询参数 | 同上 |
| TOOL-194.8 | 路径含空白 | 同上 |
| TOOL-194.9 | 哈希含非十六进制字符 | 同上 |
| TOOL-194.10 | 哈希长度错误 | 同上 |
| TOOL-194.11 | bytes 为负 | 同上 |
| TOOL-194.12 | bytes 为零 | 同上 |
| TOOL-194.13 | bytes 为字符串 | 同上 |
| TOOL-194.14 | 缺 bytes | 同上 |
| TOOL-194.15 | 非法 TOML | 同上 |
| TOOL-194.16 | 名称分别为隐藏名称、含点、含空格及空名 | 同上 |
| TOOL-194.17 | 空指针集合 | 退出 0,不创建 target |
| TOOL-194.18 | 多余 CLI 参数 | 退出 2 |

### TOOL-195 下载与解压失败不发布半成品

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)
- 级别:table
- 里程碑:M3
- 前置:指针指向三字节合成输入 abc,每行独立运行
- 步骤:替身依次返回非 zstd 内容、截断压缩帧、解压后少一个字节、多一个字节、AWS 程序缺失、成功但未产生文件
- 期望:六行均退出 2,缓存和本次临时目录均不存在;实际运行 zstd 识别损坏帧
- 实现:`tools/xtask/tests/fixtures_fetch.rs::tool_195_fetch_codec_size_and_transfer_failures_leave_no_partial_cache`

| 行 | 替身结果 | 期望 |
|---|---|---|
| TOOL-195.1 | 非 zstd 内容 | 退出 2,不留缓存或临时目录 |
| TOOL-195.2 | 截断压缩帧 | 同上 |
| TOOL-195.3 | 解压后少一个字节 | 同上 |
| TOOL-195.4 | 解压后多一个字节 | 同上 |
| TOOL-195.5 | AWS 程序无法启动 | 同上 |
| TOOL-195.6 | 下载返回成功但未产生文件 | 同上 |

### TOOL-196 单对象失败保留已验证缓存

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)
- 级别:integ
- 里程碑:M3
- 前置:三个固定合成对象,缓存为空
- 步骤:以乱序写入 a/b/z 三个指针,只提供 a/z;首次执行后补齐 b 并重跑
- 期望:首次按 a/b 顺序下载,退出 2,仅保留 a;第二次只下载 b/z,退出 0,三个缓存字节均通过校验
- 实现:`tools/xtask/tests/fixtures_fetch.rs::tool_196_fetch_failure_preserves_other_verified_objects`

### TOOL-197 缓存与指针符号链接拒绝

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)
- 级别:table
- 里程碑:M3
- 前置:Unix 文件系统;target、fixtures-cache、对象文件、指针文件分别为指向树外哨兵的链接
- 期望:各行退出 2,不执行 AWS 或 zstd,链接及树外哨兵保持原样
- 实现:`tools/xtask/tests/fixtures_fetch.rs::tool_197_fetch_rejects_links_without_touching_their_targets`

| 行 | 链接位置 | 期望 |
|---|---|---|
| TOOL-197.1 | target | 退出 2,链接和目标不变 |
| TOOL-197.2 | fixtures-cache | 同上 |
| TOOL-197.3 | 缓存对象 | 同上 |
| TOOL-197.4 | 指针文件 | 同上 |

### TOOL-198 整套获取与缓存复用

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)、[replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:integ
- 里程碑:M2
- 前置:两个合成 journal 段、两个 seal 和一个检查点,各对象经真实 zstd 压缩,只有 S3 下载以本地对象存储替换
- 步骤:获取整套,删除可用远端对象后重复获取
- 期望:首次下载五个对象,缓存路径与原始字节逐一相等,无锁或临时目录残留;第二次不运行 AWS/zstd,退出 0
- 实现:`tools/xtask/tests/fixtures_bundle.rs::tool_198_bundle_fetch_is_complete_and_exact_cache_is_reused`

### TOOL-199 整套失败不发布部分内容

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)、[replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:table
- 里程碑:M2
- 前置:空缓存与已损坏缓存各一行,清单最后一个对象不可取
- 步骤:先失败获取并尝试回放,再补齐对象重取
- 期望:两行首次退出 2;空缓存不发布部分目录,旧损坏缓存保留且回放拒绝;补齐后整套替换成功,三项检查均执行,临时目录与锁清理
- 实现:`tools/xtask/tests/fixtures_bundle.rs::tool_199_bundle_failure_never_publishes_partial_set_and_refreshes_corruption`

| 行 | 输入或扰动 | 期望 |
|---|---|---|
| TOOL-199.1 | 空缓存,最后对象下载失败 | 无部分缓存;回放拒绝;补齐后通过 |
| TOOL-199.2 | 损坏旧缓存,最后对象下载失败 | 旧内容保留且回放拒绝;补齐后整套替换 |


### TOOL-200 整套清单完整预检

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)、[replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:table
- 里程碑:M2
- 前置:排在前面的正确指针和排在后面的错误指针;缓存不存在
- 步骤:逐项改变版本、策略集、路径、字段、成员和检查矩阵
- 期望:每项退出 2,不创建 target、不调用外部程序;覆盖未知字段、空列表、缺 seal、重复与大小写别名、文件/目录重叠、设备名、路径越界、无效 URI/大小、无效检查点/窗口/Advice 和重复检查名
- 实现:`tools/xtask/tests/fixtures_bundle.rs::tool_200_bundle_manifests_preflight_before_any_side_effects`

| 行 | 输入或扰动 | 期望 |
|---|---|---|
| TOOL-200.1 | 未知格式版本 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.2 | 未知策略集 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.3 | journal 目录未声明 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.4 | 文件路径含 .. | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.5 | 绝对路径 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.6 | Windows 盘符 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.7 | 空路径段 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.8 | 路径含 . 段 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.9 | Windows 设备名 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.10 | 路径段以点结尾 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.11 | 文件与目录重叠 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.12 | 目录大小写别名 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.13 | 重复文件 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.14 | journal 缺 seal 配对 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.15 | 未知 Advice 模式 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.16 | 检查名大小写重复 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.17 | 检查点未声明 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.18 | 检查点为整数 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.19 | from_seq 为零 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.20 | from_seq 大于 to_seq | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.21 | from_seq 为字符串 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.22 | to_seq 为负数 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.23 | 大小为零 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.24 | 非 S3 URI | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.25 | format 为整数 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.26 | 外层未知 command 字段 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.27 | journal 模式带检查点 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.28 | 检查行未知字段 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.29 | 文件行未知字段 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.30 | 空 files 列表 | 获取与回放退出 2,无目录创建或外部调用 |
| TOOL-200.31 | 空 checks 列表 | 获取与回放退出 2,无目录创建或外部调用 |


### TOOL-201 回放复核精确成员集合

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)、[replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:table
- 里程碑:M2
- 前置:合格整套缓存
- 步骤:分别同长改内容、删除一个段、添加额外文件、添加空目录、删除检查点目录
- 期望:五行退出 2,均在启动回放子进程之前失败
- 实现:`tools/xtask/tests/fixtures_bundle.rs::tool_201_bundle_replay_rejects_changed_missing_and_extra_members`

| 行 | 输入或扰动 | 期望 |
|---|---|---|
| TOOL-201.1 | 同长改动 journal 字节 | 退出 2,不启动回放 |
| TOOL-201.2 | 缺少 journal 段 | 退出 2,不启动回放 |
| TOOL-201.3 | 多余普通文件 | 退出 2,不启动回放 |
| TOOL-201.4 | 多余空目录 | 退出 2,不启动回放 |
| TOOL-201.5 | 缺少检查点目录 | 退出 2,不启动回放 |


### TOOL-202 整套策略映射与检查矩阵

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)、[replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:table
- 里程碑:M2
- 前置:两种允许的策略集,普通回放/重算/检查点窗口三项检查
- 步骤:注入子进程退出 0/1/2/3/4/负值,逐项检查执行参数;额外传 CLI 窗口
- 期望:程序名及参数精确匹配封闭映射;0/1/3/4 执行三项,2/负值在首项停止,总码依次为 0/1/2/1/1/2;额外 CLI 参数退出 2 且不执行
- 实现:`tools/xtask/tests/fixtures_bundle.rs::tool_202_bundle_replay_dispatches_closed_strategy_and_all_declared_checks`

| 行 | 输入或扰动 | 期望 |
|---|---|---|
| TOOL-202.1 | 子进程 0 | 三项全执行,总码 0 |
| TOOL-202.2 | 子进程 1 | 三项全执行,总码 1 |
| TOOL-202.3 | 子进程 3 | 三项全执行,总码 1 |
| TOOL-202.4 | 子进程 4 | 三项全执行,总码 1 |
| TOOL-202.5 | 子进程 2 | 首项后停止,总码 2 |
| TOOL-202.6 | 子进程负值 | 首项后停止,总码 2 |
| TOOL-202.7 | 额外 CLI from_seq | 退出 2,无子进程 |


### TOOL-203 成功退出不能代替回放证据

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)、[replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:table
- 里程碑:M2
- 前置:子进程退出 0 的三项检查
- 步骤:注入空输出、非法 JSON、null、空对象、缺计数、verify 失败、非零 mismatch、零 Output/快照、重算 skipped 或零 Advice;另注入退出 1 却无报告
- 期望:每项退出 2 并停止;重算证据问题在第二项发现,其他问题在第一项发现
- 实现:`tools/xtask/tests/fixtures_bundle.rs::tool_203_bundle_success_requires_nonempty_consistent_report`

| 行 | 输入或扰动 | 期望 |
|---|---|---|
| TOOL-203.1 | 空 stdout | 总码 2,首项后停止 |
| TOOL-203.2 | 非法 JSON | 总码 2,首项后停止 |
| TOOL-203.3 | JSON null | 总码 2,首项后停止 |
| TOOL-203.4 | 空对象 | 总码 2,首项后停止 |
| TOOL-203.5 | 缺少比较数 | 总码 2,首项后停止 |
| TOOL-203.6 | verify 失败 | 总码 2,首项后停止 |
| TOOL-203.7 | mismatch 非零 | 总码 2,首项后停止 |
| TOOL-203.8 | Output 比较数为零 | 总码 2,首项后停止 |
| TOOL-203.9 | 快照比较数为零 | 总码 2,首项后停止 |
| TOOL-203.10 | recompute 为 skipped | 总码 2,第二项后停止 |
| TOOL-203.11 | Advice 比较数为零 | 总码 2,第二项后停止 |
| TOOL-203.12 | 子进程退出 1 却无报告 | 总码 2,首项后停止 |


### TOOL-204 整套独占锁与链接拒绝

- 规则:[大文件指针](../fixtures/README.md#大文件s3-指针)、[replay-verify](README.md#cargo-xtask-replay-verify)
- 级别:table
- 里程碑:M2
- 前置:已有整套缓存与独占锁;Unix 另设缓存根、子目录、成员文件为指向树外哨兵的符号链接
- 步骤:调用获取与回放
- 期望:已有锁令获取退出 2 且锁内容不变;各链接行两入口均退出 2,无外部调用,链接与树外哨兵不变
- 实现:`tools/xtask/tests/fixtures_bundle.rs::tool_204_bundle_lock_and_links_never_touch_external_targets`

| 行 | 输入或扰动 | 期望 |
|---|---|---|
| TOOL-204.1 | 已有独占锁 | 获取退出 2,原锁内容不变 |
| TOOL-204.2 | Unix 成员文件为符号链接 | 获取和回放退出 2,目标与链接不变 |
| TOOL-204.3 | Unix 子目录为符号链接 | 同上 |
| TOOL-204.4 | Unix 缓存根为符号链接 | 同上 |


### TOOL-205 显式 DST 场景保留回归种子与证据身份

- 规则:[cargo xtask dst](README.md#cargo-xtask-dst)
- 级别:table
- 里程碑:M2
- 前置:运行器清单包含固定 core 套件的 LED-081,core/storage 回归文件各一个种子
- 步骤:两个连续种子加全部回归种子运行 --case LED-081;复现时加 --trace
- 期望:四个种子都实际传递 case 和默认步数,场景固定套件应用于全部种子且原回归套件保留;回归子进程输出独立证据;报告和复现不丢失 case;未知编号、冲突套件及缺少 --seeds 的 case 退出 2,不运行任何种子
- 实现:`tools/xtask/tests/dst.rs::tool_205_case_selection_preserves_regressions_and_evidence_identity`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-205.1 | 两个区间种子加 core/storage 回归,case LED-081 | 四行均运行 core 场景,保留回归来源和独立证据 |
| TOOL-205.2 | case LED-081 加 trace | repro 子进程保留 case |
| TOOL-205.3 | 未知 case | 退出 2,不运行种子 |
| TOOL-205.4 | 固定 core 场景配 storage | 退出 2,不运行种子 |
| TOOL-205.5 | case 但缺 `--seeds` | 退出 2,不运行种子 |

### TOOL-206 PR 快速通道选出改动包及其反向依赖并保守回退

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)、[质量门](../docs/development.md#质量门quality-gatesyml)
- 级别:table
- 里程碑:M1
- 前置:临时 workspace:`alpha`、依赖 `alpha`(经 workspace 改名的 dev 依赖)且源码引用 `fixtures/foo` 的 `beta`、`xtask`;临时 Git 仓库含两次推送
- 步骤:运行包选择、计划与执行器,子进程用记录型替身
- 期望:选中改动包及传递反向依赖,改名两端均计入;差异总是完整 PR(合并基);共享输入选择全部包;只跑一条 nextest 命令。失败保存退出码并停止,成功才标记 complete
- 实现:`tools/xtask/tests/test_ci_feedback.py::test_tool_206_fast_lane_selects_affected_packages_and_reverse_deps`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-206.1 | 改 `crates/a`;只改 `crates/b` | `alpha`、`beta`(反向依赖);只有 `beta` |
| TOOL-206.2 | PR 的两次推送 | 完整 PR 差异(含第一次推送的改动),不按最近一次推送缩小 |
| TOOL-206.3 | 删除或改名 | 两个路径都参与包选择 |
| TOOL-206.4 | 根 Cargo、工具链或 .cargo 配置,根 `tests/`,无人引用的 `fixtures/<目录>`;有人引用的 `fixtures/foo` | 全部当前 workspace 包;引用者 `beta` |
| TOOL-206.5 | 仅文档或 Python | 无 Rust 包,明确空计划(`has_tests=false`) |
| TOOL-206.6 | CI 脚本或工作流 | xtask;政策回归另由静态作业先执行 |
| TOOL-206.7 | 计划与执行;`SHARD=2/2` | 一条 `cargo nextest run --locked --cargo-profile ci -p … --all-features --no-fail-fast -E 'not (binary(/^proc/) \| binary(compile_fail) \| test(/heavy::/) \| test(rpl_012_dst_journals_verify))'`,分片时另加 `--partition count:2/2`;步骤输出 `has_tests`、`packages`、`dst`(hft-testkit 受影响);失败留证并停止 |

### TOOL-207 每条通道的汇总拒绝缺失或跳过的适用检查

- 规则:[cargo xtask ci](README.md#cargo-xtask-ci)、[质量门](../docs/development.md#质量门quality-gatesyml)
- 级别:table
- 里程碑:M1
- 前置:两条通道(fast:PR;full:集成分支与 main 推送、每夜)的真实作业清单,路径分类输出与各作业结论的离线表
- 步骤:逐项替换状态并运行该通道的结论计算
- 期望:只有本通道全部适用检查通过才通过;另一通道的作业不参与,也不能代替本通道
- 实现:`tools/xtask/tests/test_ci_feedback.py::test_tool_207_acceptance_requires_every_applicable_job`

| 行 | 输入 | 期望 |
|---|---|---|
| TOOL-207.1 | 本通道全部 success | 通过 |
| TOOL-207.2 | 任一适用项失败、取消、skipped 或缺失 | 失败 |
| TOOL-207.3 | 完整通道,已确认无基准改动,benchmarks skipped | 通过;`bench_crates` 为空串或被 GitHub 省略(空输出不传递)同样通过;`benchmarks`/`toolchain` 分类字段缺失则失败 |
| TOOL-207.4 | 快速通道纯文档,代码作业 skipped | 通过;路径、静态或解耦失败仍失败 |
| TOOL-207.5 | 只完成另一通道的作业,本通道作业 skipped;未知通道 | 失败 |
| TOOL-207.6 | 完整通道由每夜(schedule)触发,decoupling-gate skipped | 通过;同样的跳过在 push 时失败 |
| TOOL-207.7 | 快速通道无受影响的 Rust 包或 hft-testkit 未受影响 | 对应的 fast-test/fast-clippy/fast-dst skipped 时通过;hft-testkit 受影响而 fast-dst skipped 时失败 |

## 验收

- 本组件完成 = 里程碑已到的全部用例有同编号的实现且通过(`cargo xtask test-trace`,规则见 [docs/tests §6](../docs/tests/README.md#6-与代码的对应));`host` 用例按 `证据` 字段归档。
- 上线门槛 ⑧ 中由本组件承担的部分:TOOL-012–TOOL-029 与 TOOL-030–TOOL-031 全部通过([testing §8](../docs/testing.md#8-上线门槛))。
- 上线门槛 ③④ 的累计口径由 TOOL-145–TOOL-172 保证;阈值与账本规则归 [testing §6.1](../docs/testing.md#61-服务器上的累计运行) 与 [testing §8](../docs/testing.md#8-上线门槛)。
- 基准用例(TOOL-173–TOOL-176)的上限只登记在 [benchmarks](../benchmarks/README.md)。
