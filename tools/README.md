---
description: "仓库工具:cargo xtask(CI 门槛、解耦检查、基准门槛、回放验证、DST、schema 与主机配置生成、journal 校验、文档与依赖检查、新策略脚手架)、az-probe(AZ 探测与周复测)、hostprobe(读取实际生效的主机参数并输出 HostProfile)、hft-secrets(启动前从 Secrets Manager/KMS 预取秘密)、hft-cfg(TOML → refdata/RiskParams/EngineConfig blob,以及按策略 schema 通用的 TOML → StrategyParams blob;账户、模拟账户、能力表与绑定校验)、hft-verify(服务器上运行验证包、累计上线门槛 ③④ 的 fuzz 与 DST)"
kind: dir
tier: cold
scope: core
status: spec
---

# tools

## 概要

六个工具:**`tools/xtask`** 是全部门槛、生成器与脚手架的唯一入口(开发者与 CI 都只调它);**`tools/az-probe`** 在 M1.5 延迟探针之前选定部署 AZ 时探测各 AZ/CPG 到各交易所端点的延迟,之后每周在生产主机上复测;**`tools/hostprobe`** 读取主机上实际生效的内核、网卡、IRQ、hugepages、resctrl、时钟参数,输出 `HostProfile` 的完整内容(`/run/hft/hostprofile.json`),并可与期望剖面比对;**`tools/hft-secrets`** 在服务启动前(`ExecStartPre`)从 Secrets Manager/KMS 取秘密写到 tmpfs,让 hft-core 不必链接 AWS SDK;**`tools/hft-cfg`** 把受版本控制的 TOML 编译成 refdata、`RiskParams`、`EngineConfig` 与 `StrategyParams` blob 并经 CLI 发送——策略参数按发布制品里导出的 **schema JSON 通用处理**,hft-cfg 只链接 SDK 的参数编解码器,**不链接任何策略 crate,新增策略不改 hft-cfg**;**`tools/hft-verify`** 是验证包的驱动:在服务器上按预算运行 CI 构建好的 fuzz 目标与 DST 运行器,写运行记录并维护上线门槛 ③④ 的累计账本。xtask 只在开发机与 CI 运行(CI 是唯一的构建与测试环境);az-probe 在探针实例与生产主机(只读)运行,不在 dev2 运行;hostprobe 在每台生产主机(hft-core 启动前、部署激活时)与参考机上运行;hft-verify 只在服务器上以 `hft-verify.service` 运行(不联网、不编译)。六者都不进 hot 进程的依赖树。M1.5 延迟探针的纵切片程序 [`tools/latency-slice`](latency-slice/README.md) 有自己的 README 与 TESTS.md(只用于探针,不进生产发布归档)。

## 目录

- [使用本组件](#使用本组件)
  - [cargo xtask ci](#cargo-xtask-ci)
  - [cargo xtask decoupling-gate](#cargo-xtask-decoupling-gate)
  - [cargo xtask scaffold-gate](#cargo-xtask-scaffold-gate)
  - [cargo xtask deps](#cargo-xtask-deps)
  - [cargo xtask hot-check](#cargo-xtask-hot-check)
  - [cargo xtask bench-gate](#cargo-xtask-bench-gate)
  - [cargo xtask replay-verify](#cargo-xtask-replay-verify)
  - [cargo xtask dst](#cargo-xtask-dst)
  - [cargo xtask gen-schema](#cargo-xtask-gen-schema)
  - [cargo xtask gen-host](#cargo-xtask-gen-host)
  - [cargo xtask doc-check](#cargo-xtask-doc-check)
  - [cargo xtask new-strategy](#cargo-xtask-new-strategy)
  - [其他 xtask 子命令](#其他-xtask-子命令)
  - [az-probe](#az-probe)
  - [hostprobe](#hostprobe)
  - [hft-secrets](#hft-secrets)
  - [hft-cfg](#hft-cfg)
  - [hft-verify](#hft-verify)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

所有 xtask 子命令的退出码约定:**0** 通过;**1** 检查失败(门槛突破、不一致、违规);**2** 环境或输入错误(缺文件、哈希不符、不在要求的主机上)。输出:人读摘要到 stderr,机器可读 JSON 到 `target/xtask/<subcommand>.json`。

### cargo xtask ci

```sh
cargo xtask ci [--pure-only] [--fast] [--no-doc-check]
```

按顺序运行,任一步失败即停止:`fmt --check` → `clippy --workspace --all-targets -D warnings` → `deny check licenses bans` → `deps` → `hot-check` → `fixtures check` → 单元/属性测试(`--profile ci`,剖面定义归 [development 构建剖面](../docs/development.md#构建剖面))→ 金样 → `dst` 冒烟(种子数与回归集归 [testing §2](../docs/testing.md#2-金字塔))→ `dst --hashtrail check --first K`(K = PR 核对的一致性种子数,取值归 [testing §6](../docs/testing.md#6-ci-运行矩阵))→ `gen-schema --check` → `doc-check`。`--pure-only` 只选 frontmatter `tier: pure` 的 crate 与 Python 包;`--fast` 跳过 DST 与金样(本地迭代,CI 不用);`--no-doc-check` 跳过 doc-check 步(只供 CI 的 `rust` 作业:同一提交的 `docs` 作业每次都运行 `doc-check`,不在两个作业里各跑一遍)。`cargo deny check advisories` 不在 `ci` 内:它是独立的非阻塞任务([development](../docs/development.md))。

**按路径触发的步骤不在 `ci` 内**:`scaffold-gate`、改动目标的 fuzz 冒烟、`fixtures/replay/` 样本的 `replay-verify`、基准短跑由工作流按 [testing §6](../docs/testing.md#6-ci-运行矩阵) 的路径规则分别调用对应子命令;`decoupling-gate` 是独立作业(下节)。工作流文件、触发、作业图与资源规则归 [development](../docs/development.md#ci-概要);本地(Windows)只写代码并推送。

发布检查与夜间汇总的辅助脚本在 `.github/scripts/`;契约归 [development 发布检查](../docs/development.md#发布工作流release-aarch64yml)与[每夜任务](../docs/development.md#每夜任务nightlyyml)。`ops-py` 对相关改动运行 `tools/xtask/tests/test_ci_*.py`(TOOL-187–TOOL-191),用离线 GitHub 替身与临时 Git 仓库验证发布证据、Admin 覆盖、汇总结论与主机检查的路径判定。

**每夜**只做回归冒烟:`dst --hashtrail check`(不带 `--first`,核对全部登记种子);DST 以 `dst --seeds N --seed S --steps M` 跑一批由 UTC 日期派生起点 `S` 的新种子,fuzz 以 `fuzz --nightly` 按 `fuzz-targets.toml` 轮转(N、M 读 `tools/xtask/dst-nightly.toml`,时长上限归 [testing §6](../docs/testing.md#6-ci-运行矩阵)),**不计入上线门槛 ③④**,也不保存任何累计账本;③④ 的累计在服务器上由 [hft-verify](#hft-verify) 运行。

**基准门槛**:`bench-gate` 只在按需租用的参考机上按回归阈值阻断(从 M1.5 起每个里程碑结束时运行,[development 参考机](../docs/development.md#参考机)、[benchmarks](../benchmarks/README.md#参考机与-ci));CI 托管 runner 与 `dev2` 上只报告。

**范围**:`ci` 里的金样、fuzz 目标、DST 场景覆盖 frontmatter `scope: core` 的全部组件——`venue-binance`、`venue-okx`、`venue-bitget`(现货 + USDT 永续)、hft-core、hft-exec、hft-sim 的共用撮合器(即 hft-testkit 的撮合器)、控制面命令集(唯一清单在 [control-plane](../docs/subsystems/control-plane.md))、pipeline 与 ops_bot 的 Python 测试;`scope: optional` 的组件(链上、其余 venue、optional 策略)只在其 crate 存在时纳入,不让 core 的 CI 变红(`tools/xtask/scope.toml`);frontmatter `status: frozen` 的组件不进跨文档一致性检查。

**尚未进入 workspace 的组件**:某步依赖的组件不在 workspace 时该步跳过,跳过原因写进摘要(stderr)与 `target/xtask/ci.json` 的该步 `note`,不算失败:金样步 = 一条 `cargo test --profile ci --workspace --all-features --test golden`(带 `--pure-only` 时 `--workspace` 换成与单元/属性步相同的 `-p` 列表;包选择与特性同单元/属性步,复用其构建,不因按包解析特性而重编),所选成员都没有 `tests/golden.rs` 时跳过;两个 DST 步在 hft-testkit 的 DST 框架(`crates/hft-testkit/src/dst/`)存在时才运行;单元/属性步另以 `python -m pytest` 运行有 `tests/` 目录的 Python 包(`python/<包>/` 与 `strategies/<名>/pipeline/`,按 README frontmatter 的 `tier` 参与 `--pure-only` 选择),没有时跳过。`scope.toml` 是 `optional = [<目录>…]`(与各 README frontmatter 的 `scope: optional` 一致),列出的组件是 workspace 成员时纳入,否则摘要写"不在 workspace,跳过"。单元/属性步不带 `--pure-only` 时为 `cargo test --profile ci --workspace --all-features --no-fail-fast`(报告全部失败而不是停在第一个失败的测试二进制),带时为每个 `tier: pure` 成员一个 `-p`;clippy 步同样,且同为 `--profile ci`、`--all-features`(`cargo clippy --profile ci --workspace --all-targets --all-features -- -D warnings`:构建脚本——含 aws-lc-sys 的 C 构建——与过程宏只为两步编译一次;lint 结果与剖面无关)。任一步的命令退出 2 时 `ci` 退出 2,其余非 0 退出 1。

### cargo xtask decoupling-gate

```sh
cargo xtask decoupling-gate --base <sha> --head <sha> [--pr <n>]
```

PR 级的解耦检查,上线门槛 ⑧ 的一部分([testing §8](../docs/testing.md#8-上线门槛))。**不构建、不测试**,只读 git 与 GitHub:

1. 按 `tools/xtask/deps.toml` 的分层表把 `git diff --name-only <base>..<head>` 的文件归类;同时改了 `strategies/*`(含 `strategies/registry`)与任何引擎 crate(`crates/*`)或 app(`apps/*`)→ 失败,列出两边的文件。
2. **豁免**:`--pr` 给出的 PR 带 `engine-change` 标签,**且**有一位 Admin(名单 `.github/hft-admins.txt`)的批准评审——经 `GITHUB_TOKEN`(`pull-requests: read`)读评审列表核对批准人;只有标签、或批准人不在名单里 → 仍失败。
3. 每次豁免写进 `$GITHUB_STEP_SUMMARY` 与 `target/xtask/decoupling-gate.json`(PR 号、批准人、改动的引擎 crate 与 app 列表),供每夜摘要汇总。
4. 退出 2:读不到 PR 评审(无 token、API 失败)、`base`/`head` 不存在。

口径:提交存在 = `git rev-parse --verify --quiet <sha>^{commit}` 成功;分层取 `deps.toml` 的 `[layers]`(`strategy`、`engine`、`app`;`crates/*` = `crates/<x>/` 下的任何文件,`tool` 与其余路径中立);只有判定失败且给了 `--pr` 时才读 GitHub——`gh api repos/<GITHUB_REPOSITORY>/pulls/<n>`(标签)与 `…/pulls/<n>/reviews`,token 取 `GITHUB_TOKEN`(或 `GH_TOKEN`);每位评审人以其最后一条 `APPROVED`/`CHANGES_REQUESTED`/`DISMISSED` 评审为准(`COMMENTED` 不算);批准人不在名单时 stderr 写明"不在 Admin 名单";Admin 名单文件缺失退出 2。JSON 含 `strategies`、`engine`(`crates/*` 的文件)、`apps` 与 `result`,豁免时另有 `exemption = {pr, approver, engine, apps}`(组件目录列表)。

调用它的作业与触发(`ci.yml` 的 PR 运行、`decoupling.yml` 的评审与标签触发、`main` 推送时对合并提交的判定)以及发布 `preflight` 对其结论的核对归 [development](../docs/development.md#质量门quality-gatesyml)。

### cargo xtask scaffold-gate

```sh
cargo xtask scaffold-gate [--keep]
```

"新增策略零改动"的检查,上线门槛 ⑧ 的另一部分:

1. 用 [`new-strategy`](#cargo-xtask-new-strategy) 的模板以 `--with-model --with-pipeline` 生成临时策略 `ci-probe`(kind_id `0xFFF0`,取测试段 `0xFF00–0xFFFF`;带 `Model` 与 pipeline 插件,探针形态归 [hft-testkit §5](../crates/hft-testkit/README.md#5-解耦门槛夹具))到 `target/decoupling/strategies/ci-probe/`,并核对形态(`src/model/` 中声明 `mem_bytes` 的 Model 骨架、`pipeline/pyproject.toml`)与直接依赖(只许 `deps.toml` 给 `strategies/*` 的两项,违规报告边 `ci-probe → <包>`);
2. 在 `target/decoupling/` 生成临时 workspace 清单 `Cargo.toml`(沿用根清单的 `[workspace]` 设置与 `[profile]`,依赖路径改写到新位置)、`strategies/registry` 的临时副本 `registry/`(`strategy_set!` 末尾加一行 `ci_probe::CiProbe`、加对探针的依赖、按 `tools/xtask/scaffold/registry_dst.rs.tmpl` 生成 DST 短集 `tests/ci_probe_dst.rs`),以及 `apps/hft-core`、`apps/replayer` 的清单副本(源码仍取仓库里的文件,registry 依赖指向副本;Cargo 的 `[patch]` 不能替换 path 依赖,所以改向写在清单副本里);每次构建与测试都以环境 `CARGO_TARGET_DIR` 指向主 target 目录(未设置时为 `target/`),不另起一份全量编译;
3. 依次跑 `ci-probe` 的场景测试 `--test scenario`、registry 副本的 DST 短集(100 个种子;夹具与两个测试策略归 [hft-testkit §5](../crates/hft-testkit/README.md#5-解耦门槛夹具))、构建副本的 `hft-core`,再以 `hft-core --check --config <生成的 sim 实例测试配置>` 做一次装配检查(配置按 `tools/xtask/scaffold/sim-check.toml` 生成,`ci-probe` 绑定一个 `sim-` 开头的 Sim 账户;含 Model 内存与检查点预算;任一步退出非 0 即失败,报告该步与退出码);
4. 断言 `git status --porcelain --untracked-files=all` 在 `crates/`(引擎 crate)、`apps/`、`tools/hft-cfg`、`python/` 下为空,非空时 stderr 与 JSON `engine_changes` 列出这些文件(策略的 pipeline 插件随策略放在 `strategies/<name>/pipeline/`,不进 `python/`);补丁副本不提交,结束时删除 `target/decoupling/`,`--keep` 只为排障保留它。

模板(`strategies/_template/`)、`strategies/registry` 或 `apps/hft-core` 尚不在仓库时跳过:退出 0,原因写进摘要与 JSON(`result = "skipped"`)。JSON 另含 `new_strategy_args`、`dst_seeds`、`sim_account`(生成配置中的 Sim 账户)、`steps`(每步命令与退出码)与 `violations`。

按路径触发(改动 `crates/`、`apps/`、`strategies/`、`tools/` 的 PR,[testing §6](../docs/testing.md#6-ci-运行矩阵))。

### cargo xtask deps

读取 `tools/xtask/deps.toml`(每个 crate 的 tier 与允许的直接依赖;人读视图在 [crates/AGENTS.md](../crates/AGENTS.md))与 `cargo metadata`,检查:

1. 每条直接依赖边在 `deps.toml` 允许集合内;依赖只向下。
2. `hot`/`pure` crate 的完整依赖闭包不含:`tokio`、`async-std`、`mio`、`futures-executor`、`tracing-subscriber`、`serde_json`、`reqwest`、`hyper`、`pyo3`、`rust_decimal`、`bigdecimal`、`num-bigint`。
3. `venue-*` 只依赖 `hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto` 与编解码白名单(`sonic-rs`、`serde`(只为 sonic-rs 专用结构体的 `derive`)、`memchr`、`zerocopy`、`ed25519-dalek`、`hmac`、`sha2`、`base64`)。
4. `alloy-*`/`revm` 只出现在 optional 链上组件(`hft-amm`、`hft-chain-evm`、`apps/chain-core`)与 `hft-py` 的闭包。
5. `dev` tier crate 只出现在 `[dev-dependencies]`。
6. `hft-venue` 不依赖任何 `venue-*`。
7. L1 与策略层 crate(`hft-venue-proto`、`hft-book`、`hft-oms`、`hft-risk`、`hft-ledger`、`hft-exec`、`hft-strategy-sdk`、`hft-strat-lib`、`hft-amm`、`venue-*`、全部 `strategies/*`)不依赖 `hft-rt`(规则 [T4](../docs/hot-path-rules.md))。
8. `hft-journal/writer` 只出现在 apps/hft-core、apps/replayer(与 optional 的 apps/chain-core)的依赖闭包里。
9. **策略解耦**:`crates/*` 中任何 crate 都不依赖 `strategies/*`;`strategies/<x>`(registry 除外)只依赖 `hft-strategy-sdk` 与 `hft-strat-lib`(L0 类型经 SDK 取得,规则归 [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md));`hft-strategy-sdk` 只依赖 L0;`hft-strat-lib` 只依赖 L0 + SDK;**只有 `strategies/registry`(`hft-strategy-registry`)依赖各策略 crate**,而它只被 apps(hft-core、replayer)依赖;`hft-testkit`、`tools/hft-cfg` 与 `hft-py` 的闭包(含 `[dev-dependencies]`)里没有 registry 与任何 `strategies/*`——需要真实策略的集成测试放在 `strategies/registry/tests/` 或 `apps/hft-core/tests/`。
10. `tools/hft-verify` 的闭包不含任何 hft crate 与网络库(`reqwest`、`hyper`、`tokio`)。
11. `aws-sdk-secretsmanager` 与 `aws-sdk-kms` 只出现在 `tools/hft-secrets` 的闭包。
12. 名为 `testing` 的 feature(SDK 的[测试桩](../crates/hft-strategy-sdk/README.md#测试桩testing-特性)、hft-py 的测试钩子)只在 `[dev-dependencies]` 中启用;`[dependencies]` 或默认 feature 中启用 → 失败。

口径:workspace 成员按目录识别(`crates/hft-oms`、`apps/hft-core`;`deps.toml` 的 `strategies/*` 一项覆盖 registry 以外的全部策略 crate),外部 crate 按包名;没有 `deps.toml` 条目的成员 → 失败。闭包沿 `[dependencies]` 与 `[build-dependencies]` 传递(dev 依赖不传递;只在测试用 `--cfg` 构建中出现的依赖,如 `cfg(loom)`,按 dev 依赖处理,清单在 `deps.toml` 的 `test_cfgs`)。规则 1 对 `[dev-dependencies]` 另允许该包的 `dev` 列表与全部 dev tier 包;规则 2 针对 `crates/*` 与 `strategies/*` 的库(app 的闭包按其 README 依赖节);规则 5 的唯一例外是只在开发机与 CI 运行的 `tools/xtask`(`dev_tier_ok`,依据 [hft-testkit 依赖](../crates/hft-testkit/README.md#依赖));规则 8 检查只读 journal 的包,即其余 app 与 hft-py([crates/AGENTS.md](../crates/AGENTS.md#依赖方向)),启用 `writer` 的方式包括直接声明与经某个 crate feature 转发。违规逐条写进 `target/xtask/deps.json`(规则号、包、详情,闭包违规带路径)。

### cargo xtask hot-check

基于 `syn` 的源码与配置检查,规则编号见 [hot-path-rules](../docs/hot-path-rules.md):

| 检查 | 规则 |
|---|---|
| hot/pure crate 的 `clippy.toml` 与 `tools/lint/clippy-{hot,pure}.toml` 逐字节一致 | 检查机制 |
| `lib.rs` 的 lint 属性块与 `tools/lint/attrs-<tier>.rs` 一致且未削弱 | C1、N3、B5 |
| `#[allow(clippy::disallowed_*)]` 与 `#[allow(clippy::unwrap_used …)]` 只出现在 `tools/xtask/allow-list.toml` 登记处 | 检查机制 |
| hot/pure crate 类型位置出现 trait object | N6 |
| N3 列出的 crate 类型位置出现 `f32`/`f64`(策略 crate 的 `Model` 模块按 SDK 规则豁免;`crates/hft-strat-lib/src/model/` 同样豁免) | N3 |
| `unsafe` 出现在四个允许的 crate 之外;允许的 crate 里 `unsafe` 块缺 `// SAFETY:` | B5 |
| `std::thread::spawn`/`Builder::spawn` 出现在 `apps/*` 之外 | T1 |
| pure crate 的 `src/`(测试代码除外)引用 `std::time`(报 T4)、`std::fs` 或 `std::net`(报 `pure`;tier 规则见 [crates/AGENTS.md](../crates/AGENTS.md#tier)) | T4、pure |
| rustls 记录加密入口的调用点不在 `crates/hft-net/src/tls.rs` | T6 |
| `memmap2::MmapMut` 出现在 `hft-journal` 之外 | M3 |
| `Atomic*` 类型定义出现在 `hft-rt`、`hft-journal` 之外 | T3 |
| 根 `Cargo.toml` 的 `[profile.release]`、`[profile.release-hot]`、`[profile.ci]`、`[profile.bench]` 与 [development 构建剖面](../docs/development.md#构建剖面) 一致;`.cargo/config.toml` 的发布 `target-cpu` 已设置 | B1、B2 |
| `#[repr(C)]` 结构缺 `size_of`/`offset_of` 编译期断言、或有隐式填充 | M4、M6 |
| 全局 mimalloc 的选项缺 `purge_delay = -1` | M1 |
| `strategies/*` 的快速部分(`src/model/` 以外)违反上列 hot 规则集(trait object、`f32`/`f64`、未登记的 allow、`unsafe`) | S1 |
| SDK `model_math` 模块被策略 `src/model/` 以外的代码引用 | N3 |
| 快照节的行类型中出现 `Tsc` 字段而未登记在 `allow-list.toml` | D6 |

机检口径:

- **范围**:库 crate = `crates/` 与 `strategies/` 下有 `Cargo.toml` 的目录(`_` 开头的除外),tier 取 `deps.toml`;"hot/pure crate" 只指其中 tier 为 `hot`/`pure` 的库(apps 是二进制,不在内)。源码规则读 `<crate>/src/**/*.rs` 并跳过 `#[cfg(test)]` 项与只经 `#[cfg(test)] mod x;` 引入的文件;`#[allow]` 规则读 hot/pure crate 的全部 `.rs`(含 `tests/`、`benches/`、`examples/`、`build.rs`,clippy 配置同样作用于它们);嵌套包(`fuzz/`)不读。策略 crate 的快速部分 = `src/` 去掉 `src/model/`。T1 读 `crates/*`、`strategies/*` 全部 tier 的 `src/`;M3、M1、T6 另读 `apps/*`。
- **lint 模板**:`clippy.toml` 与 `tools/lint/clippy-<tier>.toml` 逐字节相同。属性块:`tools/lint/attrs-<tier>.rs` 中每条 lint 在 `lib.rs` 的 crate 级属性里以不低于模板的级别出现(`forbid` ≥ `deny`;同一 lint 以最后一条属性为准,后出现的 `warn`/`allow` 即削弱);B5 的四个 crate 以 `attrs-unsafe.rs` 代替 `forbid(unsafe_code)`(保留 `forbid(unsafe_code)` 也可);N3 列出的 crate 与策略 crate 另要求 `deny(clippy::float_arithmetic)`。
- **allow-list**:`allow-list.toml` 的 `[[allow]]` 为 `file`(glob,`*` 一段、`**` 任意层)+ `lints` + `reason`;需登记的 lint 为 `clippy::disallowed_{methods,types,macros}` 与 C1 的 `unwrap_used`、`expect_used`、`panic`、`unreachable`、`todo`、`unimplemented`、`indexing_slicing`(含 `#[expect]` 与 `cfg_attr` 内的写法)。`[[tsc]]` 为 `row` + `field` + `class`(D6 的三类,1–3)+ `reason`。
- **类型位置**:trait object = 任一类型中的 `dyn`;浮点 = 类型路径恰为 `f32`/`f64`(含 `as` 转换的目标类型);`Atomic*` = 末段以 `Atomic` 开头的类型路径;`MmapMut` = 任一路径段;线程 = 调用 `…thread::spawn`/`…Builder::spawn` 或接收者含 `Builder` 的 `.spawn(`;T6 = `WriteTraffic::encrypt` 路径或含 `rustls`/`WriteTraffic` 的文件中的 `.encrypt(` 调用;`model_math` 或 `hft_strat_lib::model` 路径(含 `use`)在 SDK/strat-lib 自身与策略 `src/model/` 之外即违规。
- **`unsafe`**:B5 四个 crate 之外的 hot/pure crate 出现 `unsafe` 块、`unsafe impl`、`unsafe fn`、`unsafe trait` 即违规;四个 crate 内的 `unsafe` 块与 `unsafe impl` 要在同一行、紧邻的注释行或所属语句之上的注释里有 `SAFETY:`。
- **布局**:hot/pure crate 中每个 `#[repr(C)]` 结构在本 crate `src/` 里有 `size_of::<名>()`(可带泛型实参)出现;未 derive zerocopy `IntoBytes` 的非泛型结构按 repr(C) 规则计算(原语、原子类型、数组(长度为字面量或同树 `const`)、`repr(transparent)` 新类型、`repr(uN)` 枚举、别名与嵌套结构;解析不了的跳过),字段之间有填充、或无 `align(N)` 而尾部有填充即违规(M6)。D6 的"快照节行类型" = 名字以 `Row` 结尾的结构,`Tsc` 字段 = 类型末段为 `Tsc` 或字段名为 `tsc`/以 `_tsc` 结尾。
- **剖面**:`docs/development.md` 构建剖面表 `release`、`release-hot`、`ci`、`bench` 行的 `` `键 = 值` `` 沿 `inherits` 展开(Cargo 内置的 `bench` → `release`),与根 `Cargo.toml` 同样展开后的取值逐键相等;目标 CPU 表第一行(core)的三元组在 `.cargo/config.toml` 的 `[target.<三元组>] rustflags` 中有 `-C target-cpu=<值>`。M1:有 `#[global_allocator]` 且用 `MiMalloc` 的包,其源码里要有 `purge_delay … -1`。

### cargo xtask bench-gate

```sh
cargo xtask bench-gate [--crate <x>] [--path <p>] [--smoke | --reference] [--syscall-audit]
```

门槛、参考机、伸缩系数与回归阈值的规格在 [benchmarks/](../benchmarks/README.md);退出码(含 `--reference` 时已登记基准缺预算退出 1、`gate.kind = "report"` 的基准超预算不影响退出码只在 `verdict` 中标出)归 [benchmarks 使用本组件](../benchmarks/README.md#使用本组件)。每个已注册策略 crate 必须有 SDK 规定的基准入口([hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约)):`--smoke` 与 `--reference` 先核对每个 `strategies/<kind>/benches/` 都提供这些基准名,缺任一个 → 退出 1 并列出缺的名字。`--smoke`:CI 托管 runner 上以 `ci` 剖面构建(不用 `bench`/`release-hot`,剖面归 [development 构建剖面](../docs/development.md#构建剖面))并只报告数字,从不阻断;`--reference`:在按需租用的参考机上(同型实例,从 M1.5 起每个里程碑结束时;租用与时机归 [development 参考机](../docs/development.md#参考机))运行正式版附带的基准制品,按回归阈值与单 crate 上限阻断。`--reference` 先调用 `hostprobe verify --expected benchmarks/reference.toml`,不符退出 2。`--syscall-audit`:在 mock 回环运行期间启动 bpftrace,挂 `tracepoint:raw_syscalls:sys_enter`,按 `comm`(线程名)与系统调用号计数,结束后与 [hot-path-rules T7](../docs/hot-path-rules.md) 的允许表比对;需要 root 或 `CAP_BPF`+`CAP_PERFMON`,只在参考机上运行。不用 strace(ptrace 改变被测时序)。

实现口径:`--smoke` 对每个带 `benches/` 的 workspace 成员先 `cargo bench --profile ci --workspace --all-features --no-run --message-format=json-render-diagnostics --bench <该成员的每个基准目标名>`(按 workspace 统一特性解析,复用单元/属性步 `ci` 剖面已编译的库;按包 `-p` 会另行解析特性而重编依赖;构建失败报告该目录、继续其余包,结果退出 1),再从构建输出的 `compiler-artifact` 中取该成员自己的基准可执行文件(别的成员可能有同名基准目标),逐个以 `--bench` 加短跑参数运行;`--reference` 用 `--bench-dir <dir>`(基准制品中的可执行文件)运行,缺省时 `cargo bench --profile bench`(只供本地排查)。结果取自基准在 stdout 输出的 `hft-bench: {json}` 行(HistBench),Criterion 基准取本次运行写出的 `<target>/criterion/**/new/{benchmark,sample}.json`(按样本均值求分位);每个基准写 `target/bench/<path>/<case>.json`(单 crate 基准的 `<path>` = 包名、`<case>` = 基准名),汇总写 `target/xtask/bench-gate.json`。`budgets.toml` 表在 [doc-check 预算口径](#cargo-xtask-doc-check) 之外另可写 `gate = "absolute" | "report"`(缺省:有数值即 `absolute`)与 `allocs = 0`(预热后分配必须为 0);表在而无数值视为没有预算。路径基准的预算在 `benchmarks/budgets.toml`,`benchmarks/reference.toml` 写 `path_absolute = true` 之前只报告。基线为 `--baseline-dir`(缺省 `target/bench-baseline/`)下 `<host_profile_hash>/<基准名>.jsonl` 的通过记录,哈希取 `hostprobe verify` 写出的 `target/bench/hostprofile.json`;与 S3 `hft-bench/` 前缀的同步由参考机 runbook 完成。`--syscall-audit` 读 `benchmarks/syscall-audit.toml`(`run` = 被测运行的命令行数组,`threads` = 该运行的 `[[threads]]` 文件),以 `bpftrace -e … -c <run>` 计数,线程角色按 `tasks`(`sequencer`、`gw:*`、`feed*`;其余为 warm,不审计),非 `Spin` 线程另允许 park 边界的 `futex`/`epoll_wait`/`epoll_pwait`/`write`;单独给 `--syscall-audit` 时只做审计(仍先核对主机剖面)。

### cargo xtask replay-verify

```sh
cargo xtask replay-verify <journal-file | journal-dir | fixtures/replay/<name>.pointer.toml> [--from-seq A] [--to-seq B] [--record-decide-hist]
```

以验证模式运行 [`replayer`](../apps/replayer/README.md):喂入 journal 的全部 Input 事件,逐字节比较每个 Output 记录(`OrderDecision`、`ExecDecision`、`CmdResult`…,屏蔽 hft-event `MEASURE_MASK` 的测量字段;`StateSnapshot` 同样逐字节比较),并比较快照头的状态哈希(`Keyframe` 快照的 `state_hash` 恒为 0,由 replayer 侧计算,规则归 [journal-format](../docs/subsystems/journal-format.md#快照记录结构))。失败输出第一个分歧的 `seq`、两侧记录的十六进制与解码视图、分歧前 200 条事件的 journal 片段路径。回放只读 journal 内容;若需要任何外部配置文件才能运行,以退出码 1 报告"journal 不自足"。`--record-decide-hist` 同时输出 `decide` 分段直方图,供 bench-gate 的纯计算回归基线使用。模拟账户的 journal 段与实盘一样可验证。

### cargo xtask dst

```sh
cargo xtask dst --seeds N [--steps M] [--seed S] [--suite X] [--regressions] [--trace] [--strategy K]
cargo xtask dst --hashtrail check|write [--first K] [--strategy K]
```

**本节是该子命令签名的唯一归属**(hft-testkit、testing 与 DST 打印的复现命令都用这一组参数)。在 [`hft-testkit`](../crates/hft-testkit/README.md) 的 DST 框架里运行 `tests/dst/` 的场景(与服务器上验证包里的 `hft-dst` 是同一个运行器):

| 参数 | 含义 |
|---|---|
| `--seeds N` | 运行从起点开始的 N 个连续种子(必填) |
| `--steps M` | 每个种子的步数上限(缺省值归 [hft-testkit](../crates/hft-testkit/README.md)) |
| `--seed S` | 起点种子;缺省由提交 SHA 派生(PR 冒烟)。每夜作业以 UTC 日期派生 `S`;复现单个种子(含 `hft-verify crash list` 报告的服务器种子)= `--seeds 1 --seed S` |
| `--suite X` | 每个种子都在套件 X 中运行;缺省时每个种子在由种子派生选出的一个套件中运行(派生与 `hft-dst run` 相同,归 hft-testkit);X 不存在 → 退出 2 |
| `--regressions` | 追加运行 `fixtures/dst-seeds/<suite>.txt` 的全部种子,每个种子在其文件名所示的套件中运行 |
| `--trace` | 打印逐步轨迹(复现时用) |
| `--strategy K` | 跑 `strategies/registry/tests/` 中该策略的 DST(真实策略集);不带时只跑 hft-testkit 的 `TestStrategies` |
| `--hashtrail check\|write` | 一致性金样:以 `fixtures/dst/hashtrail.toml` 登记的种子、套件与步数运行(不能与 `--seeds`/`--seed`/`--suite`/`--regressions` 同用,同用退出 2);`check` 逐种子把 `HashTrail` 的摘要与文件比较,任一不同退出 1 并输出第一个不同的种子(定位到 `seq` 与组件用 `--seeds 1 --seed S --suite X --trace` 复现);`write` 只重写各种子的 `HashTrail`(种子集不变) |
| `--first K` | 只与 `--hashtrail check` 同用:只核对登记顺序的前 K 个种子;K = 0、K 大于登记数或与 `write` 同用 → 退出 2 |

失败输出:种子、套件、违反的不变量([testing §4](../docs/testing.md))、违反前 200 条事件的 journal 片段(可直接喂 `replayer`),以及一行可复制的复现命令 `cargo xtask dst --seeds 1 --seed S --suite X --steps M --trace`(格式归 [hft-testkit](../crates/hft-testkit/README.md#6-dst-框架))。CI 与本地运行都不写服务器账本、不计入门槛 ④。

实现口径:缺省起点 = HEAD 提交 SHA 的前 16 位十六进制(作为 u64);种子写作 `0x` + 16 位小写十六进制,也接受十进制。`dst` 以 `ci` 剖面构建 hft-testkit 的 [`hft-dst`](../crates/hft-testkit/README.md#8-hft-dst-运行器)(`cargo build --profile ci -p hft-testkit --features dst-bin --bin hft-dst`)并调用它:套件表取 `hft-dst list` 中以 `suite <名称>` 开头的行,按名字排序后用 hft-testkit 的 `seed::suite_index` 派生种子的套件;区间种子以 `hft-dst run --seeds <起点>..<终点>`(十进制、含两端)运行,带 `--trace` 时改为逐个 `hft-dst repro --seed S --suite X --steps M --trace --crash-dir target/dst`;回归种子逐个 `hft-dst repro --seed S --suite <文件名所示套件> --steps M --crash-dir target/dst`;崩溃目录与运行结果都在 `target/dst/` 下。`fixtures/dst-seeds/<suite>.txt` 每行取第一个字段为种子,`#` 开头的行跳过。JSON `target/xtask/dst.json`:`seeds`(区间在前、回归在后)、`runs`(每项 `seed`、`suite`、`source = range | regression`)、`steps`、`suite`、`strategy_set`(不带 `--strategy` 时 `"TestStrategies"`)、`result`。`--hashtrail`:`fixtures/dst/hashtrail.toml` 是按登记顺序排列的 `[[seed]]` 表(`seed = "0x<16 位十六进制>"`、`suite`、`steps`、`trail = "<32 位十六进制>"`);对每个种子运行 `hft-dst hashtrail --seed S --suite X --steps N`(stdout 一行 `trail <HashTrail 字节的 xxh3_128 十六进制> points=<n>`,格式归 [hft-testkit](../crates/hft-testkit/README.md#8-hft-dst-运行器)),`check` 在第一个摘要不同的种子处停下并输出种子、套件与两侧摘要(退出 1),`write` 只改写各 `[[seed]]` 的 `trail` 值;JSON 含 `seeds` 与 `first_divergence`。hft-testkit 的 DST 框架(`crates/hft-testkit/src/dst/`)不在仓库时退出 2。

### cargo xtask gen-schema

```sh
cargo xtask gen-schema [--check]
```

从 [`hft-event`](../crates/hft-event/README.md) 的 Rust 定义生成:Arrow schema、Python dtype 模块(`python/_generated/`)、事件目录文档(`docs/generated/event-catalog.md`)、hft-telemetry 的 Note 码表 `notes_catalog.rs`、hft-venue 的 `ConnState.reason` 码表 `schemas/conn-reason.json`、只追加基线 `schemas/event-layout.lock`(生成物清单与格式归 [hft-event 实现说明](../crates/hft-event/README.md#实现说明));核对 hft-event 的 Note 码登记表与各 source crate 的 `const` 表一一对应;`apps/hft-core` 在 workspace 中时运行已构建的 `hft-core schemas --out target/schemas`(二进制取 `HFT_CORE_BIN`,否则 `target/{debug,release,ci}/hft-core`;`apps/hft-core` 尚不在 workspace 时跳过策略部分并在输出中注明),把导出的 `<kind_id>-<name>.schema.json` 与 `index.json` 与已提交的 [`fixtures/strategy-schemas/`](../fixtures/README.md) 逐字节比较(格式归 [strategies/registry](../strategies/registry/README.md))。`--check` 只生成到临时目录并与仓库内容比较,有差异退出 1。每次运行(带不带 `--check`)都先做只追加核对:基线是 `schemas/event-layout.lock`——CI 的 PR 运行(`GITHUB_EVENT_NAME = pull_request`)取 PR 目标分支(`GITHUB_BASE_REF`,先找 `origin/<分支>` 再找本地分支;检出时须带上目标分支)与 HEAD 的合并基点上的该文件,取不到合并基点退出 2,其余情形取工作区 HEAD 已提交的版本,基线中还没有该文件时只生成、不核对;核对范围含事件布局、kind、`CmdKind`、`LoadErrCode`、引擎 Note 码,以及经 `hft-core schemas` 导出读取的各策略 `kind_id`、策略 Note 码、命令 `op` 与检查点节 `tag` 只追加、不复用的核对;失败条件的唯一定义在 [hft-event 实现说明](../crates/hft-event/README.md#实现说明),违反时不改写任何生成物、退出 1;找不到已构建的 `hft-core` 二进制退出 2。

### cargo xtask gen-host

```sh
cargo xtask gen-host [--host <name>] [--check]
```

主机配置生成器的入口:读 `deploy/hosts/<name>.toml`(主机清单:主机类、网卡 ENI-A/B/C 与预留的 ENI-B2、出口地址、存储;字段归 [deploy](../deploy/README.md))与 [apps/hft-core](../apps/hft-core/README.md) 给出的该主机类档(线程计划、内存表、slice 与 tmpfs 数值、dev2 护栏数值),调用 `deploy/gen/` 的生成器写出生成物:GRUB 片段、各 slice 的 `AllowedCPUs`/cpuset、IRQ 计划(各 ENI 队列 → 核)、两个实例配置的 `[host]`/`[[threads]]`/出口地址段、`/etc/hft/egress.toml`、watchtower 的隔离核线程白名单、hostprobe 的期望剖面(生成物清单与格式归 [deploy](../deploy/README.md))。生成前先用 hft-core 加载校验的同一实现检查线程计划(核号冲突、隔离集合与 sim/批处理核相交、`feed-bbo:*` 与 `feed-md:*` 同线程等),不通过退出 1。生成物不含网卡 MAC(网卡按设备序号识别,MAC 只由 hostprobe 在运行期经 IMDS 读出并核对,规则归 deploy),因此换 AZ 重建或租用探针机不需要先发版。不带 `--host` 时处理全部清单;`--check` 生成到临时目录并与已提交的生成物比较,有差异退出 1(CI 触发口径归 [development](../docs/development.md#ci-概要));它同时核对生成的 2 MiB/1 GiB 大页数等于 hft-core 内存表按实际 journal 列表求和的结果。

### cargo xtask doc-check

| 检查 | 失败条件 |
|---|---|
| 链接 | 相对链接目标不存在;锚点不存在(按 GitHub 规则生成锚点) |
| README 结构 | frontmatter 缺 `description/kind/tier/scope/status` 或值非法(`scope` 只能是 `core`/`optional`;`status` 只能是 `spec`/`impl`/`frozen`);二级标题不按 [README 模板](../crates/AGENTS.md#readme-模板) 顺序 |
| 预算 | hot crate README "延迟与资源契约"里的基准名在 `benches/budgets.toml` 不存在或数值不同;crate 预算宽于 [benchmarks 上限表](../benchmarks/README.md) |
| 分段名 | README/文档里出现不在 [latency-budget §2](../docs/latency-budget.md) 的分段名(按 `` `name` `` 形式匹配已知前缀) |
| 事件 | 每个 `EventKind` 至少被一个 README 声明为产生者;[event-model](../docs/subsystems/event-model.md#kind-编号段) 编号表的分类与 `KindInfo.role` 一致;正文出现的 `Note{X}`、`Timer{X}` 名在 hft-event 的登记表或某策略的 `StrategyDesc.notes` 中 |
| 码值与布局 | `#[repr(C)]` 结构里名为 `*kind`/`*level`/`*purpose`/`*role`/`*mode`/`*status`/`*reason`/`*code` 的整型字段,注释含码值(`=`)或码表链接;结构字段字节和与声明字节数一致 |
| 依赖图 | `crates/AGENTS.md` 依赖方向图与 `deps.toml` 不一致 |
| 环名 | [hft-event 环拓扑](../crates/hft-event/README.md#环元素类型) 与 [hft-sequencer](../crates/hft-sequencer/README.md) 环表的环名与元素类型不一一对应(optional 的 `chain-in`/`chain-out` 不参与) |
| 权限表 | [hft-ctl-proto 权限表](../crates/hft-ctl-proto/README.md#权限表)与 `crates/hft-ctl-proto/src/perms.rs` 的 `PERM_ROWS`、`OPTIONAL_ROWS` 渲染出的行不逐行相同(CTP-093) |
| venue README | `scope: core` 的 `venue-*` README 缺 [hft-venue-proto 必备节](../crates/hft-venue-proto/README.md#每个-venue-readme-的必备节) 中的任一节或某节缺现货/永续之一;venue 预算表某项未写 p50 与 p99,或任一分位宽于 [hft-venue-proto 统一门槛表](../crates/hft-venue-proto/README.md#延迟与资源契约);门槛表与 hft-net 预算、[latency-budget](../docs/latency-budget.md) `ingest` 行的核对规则(归 hft-venue-proto)不成立 |
| runbook 结构 | `docs/runbooks/` 下 frontmatter `scope: core` 的页缺 [写法规则](../docs/runbooks/README.md#写法规则) 规定的二级标题(适用范围、前置条件、步骤、验收、回退、记录)或顺序不同 |
| 范围 | 每个组件 README frontmatter 的 `scope` 是范围的**唯一来源**;`doc-check` 据此生成根 [README](../README.md) 的组件一览,不一致即失败;正文出现"阶段 N"或"第 N 周"式措辞即失败——optional 内容写成"可选扩展:……(触发条件)";`status: frozen` 的组件跳过跨文档一致性检查 |
| 策略族名 | `crates/*` 与 `apps/*` 的 README,以及 `python/` 下框架包的 README 与 `.py` 源文件,出现策略族名或策略专用术语(名单在 `tools/xtask/doc-check.toml`,与 `architecture-v2/_work/check_hft.py` 同步)即失败;策略规则与策略的 pipeline 插件只在 `strategies/*` |
| 写当前状态 | 正文出现历史叙述关键词或跨文档同步队列的标题与条目(名单在 `doc-check.toml`;`.agents/notes/`、`docs/postmortem/` 与"延伸阅读"节除外) |
| 残留 | 链接或提及已删除的组件、已退役的模式名与实例角色、已退役的参数与常量、已取消的上线阶段、非 32 位的 kill 字或掩码、已废弃的调优取值与命令(名单都在 `doc-check.toml`,与 `check_hft.py` 同步;本页不重复列出名字,免得自身命中)即失败 |
| Agent Notes | 相对链接可达、类别目录属于封闭集合;其余格式检查**冻结**(由评审把关,见 [.agents/notes/README.md](../.agents/notes/README.md)) |
| 篇幅 | 根 `AGENTS.md` > 1,800、子树 `AGENTS.md` > 900、`architecture.md` > 3,000(计量口径归 [docs/AGENTS.md](../docs/AGENTS.md#篇幅)) |

上表各项由 xtask 原生检查实现(全部为硬错误),之后运行 `doc-check.toml` 的 `scripts` 列出的 `tools/doccheck/check_hft.py` 与 `check_tests.py`;`doc-check.toml` 的 `exclude`(含 xtask 测试夹具 `tools/xtask/tests/fixtures/`)不扫描,`evidence`(`design/`)只查链接,`status: frozen` 的组件跳过内容检查。表中未细化之处的机检口径:

- **预算**:只查已有 `Cargo.toml` 的 hot crate。README 预算行里的基准名 = 最后一列中已在 [benchmarks 基准名登记](../benchmarks/README.md#基准名登记) 的名字;预算格中 `[pNN] ≤|< 数值 单位` 是预算值,紧跟 `+` 的(相对值)与带 `/档`、`/条` 的(引用的上限)不算;表头写 `p50 / p99` 时一格的前两个值依次是 p50、p99,否则一行有带分位的值就只取它们,没有时不带分位的值按基准名顺序一一对应(个数不等只核对基准名存在)。`benches/budgets.toml` 每个基准一张表 `[<基准名>]`,键 `pNN_ns`(带分位的值,`p99.9` 写 `p999_ns`)或 `budget_ns`(不带分位的值),与 README 必须相同;可选 `bound`(上限表"原语"列原文)、`case`(该格中以 `标签:` 区分的情形)、`levels`(按档的上限除以的档数)指明比对的上限行,`<` 的上限不得相等。
- **分段名**:反引号内的 snake_case 名以 latency-budget §2 的某个分段名开头、紧接字母或数字(不是 `_`),且本身不是 latency-budget 表第一列的名字 → 失败。
- **事件**:"声明为产生者" = `crates/*`、`apps/*`、`strategies/*` 的 README(hft-event 自身除外)有一行同时含反引号包住的该 kind 名(可带 `{…}`)与产生动词(产生、产出、写入、写出、写、发出、输出、生成、推入、`→`);测试 kind 除外。编号表"分类"列:`按体…` = `ByBody`,`raw journal` 与 `Meta` = `Meta`;每个登记的 kind 都要有一行。登记表 = hft-event README 表格行中的标识符与 `名字 = 数字` 写法;`StrategyDesc.notes` = `strategies/*`(registry 除外)README 表格行中的标识符。
- **码值与布局**(硬错误):码表链接 = 注释含 `#`、`§`、Markdown 链接、"码表/码值/登记/见/自定义"、值列表(`0 无 / 1 First`)、`同 X` 或某个 `#[repr(uN)]` 枚举类型名;声明字节数取结构头行或尾行注释的 `N B`(`≤ N B` 与 `N B + …` 不算)或 `size_of` 断言,与按 repr(C) 规则(对齐、`align(N)`、`packed`)算出的大小比对;类型大小在全部 Markdown 代码块中解析(原语、数组、ID newtype、别名、`#[repr(uN)]` 枚举、嵌套结构),解析不了的结构跳过。
- **依赖图**:`依赖方向` 代码块每行按 `;` 分句,括号内文字与 `:` 之前的文字忽略;`A ← B` = B 依赖 A,`A → X + Y` = A 依赖 X、Y;`L0`/`L1` = 以 `(L0,…)`/`(L1,…)` 标注的那一行的 crate,`venue-<x>` = 全部 `crates/venue-*`,`strategies/<name>` = `deps.toml` 的 `strategies/*`;每条边必须在 `deps.toml` 该包的 `deps` 内,图中的包名必须在 `deps.toml` 中。
- **venue README**:必备节 = hft-venue-proto 必备节表第一列 `(` 之前的文字,匹配以它开头的标题;一节提到一侧市场(`现货` 或 `MARKET = Spot` 的产品线类型名;`永续` 或 `MARKET = LinearPerp` 的类型名)而没提另一侧,且没写"两者/两个产品线" → 失败。预算表(表头 `p50 / p99`)每行要有两个值;行按入口与消息类别对到统一门槛表的行(`classify`;`decode_md` 按 BBO/分档快照,增量深度不比;`encode`;`decode_report` 按成交/可忽略与未知/保证金推送;`decode_snapshot`;`acct_decode`),门槛列不是数值的行不比。`ingest` 核对:`net_rx_300b_mem` + `classify` + BBO `decode_md` 门槛 ≤ latency-budget §3 `ingest`(bookTicker)目标 p50 的 1/3、p99 的 1/5;`net_rx_2k_mem` + `classify` + 分档快照 `decode_md` 门槛 ≤ `ingest`(深度)目标 p50 的 1/2、p99 的 1/3。
- **runbook 结构**:六个标题按 [写法规则](../docs/runbooks/README.md#写法规则) 的"每页结构"取,标题文字相同或以它加 `(` 开头即算;其间可以有别的二级标题。
- **范围**:根 README 组件表(有 `tier`、`范围` 两列的表)中链接的 README,其 frontmatter 的 `tier`、`scope` 与表一致;每个有 frontmatter 的 `crates/*`、`strategies/*` README 都在表中。
- **策略族名、写当前状态、残留、排期措辞**:`doc-check.toml` 的 `[[rule]]`(正则;各自的文件范围、豁免路径、按 frontmatter `scope` 选文件、豁免的小节与行、先删去的文字);已删除组件(`[deleted]`)按词匹配,链接目标也算。
- **Agent Notes**:生命周期目录下直接放的 Note(没有类别目录)同样失败。

### cargo xtask new-strategy

```sh
cargo xtask new-strategy <name> --kind-id <n> [--with-model] [--with-pipeline]
```

从 `strategies/_template/` 生成新策略 crate 的骨架(模板内容归 [strategies/AGENTS.md](../strategies/AGENTS.md) 与 [adding-a-strategy](../docs/cookbook/adding-a-strategy.md)):

1. 创建 `strategies/<name>/`:`Cargo.toml`(只依赖 `hft-strategy-sdk`、`hft-strat-lib`)、`src/lib.rs`(`Strategy` 骨架、`StrategyDesc`、空的 `ParamSet` 与 schema)、`--with-model` 时加 `Model` 骨架(含 `mem_bytes`/`snapshot_bytes` 声明)、`tests/scenario.rs`(SDK mock 视图上的场景测试)、`README.md`(按 strategies/AGENTS.md 的模板);`--with-pipeline` 时另建 `strategies/<name>/pipeline/`(该策略的 pipeline 插件 Python 包骨架,插件接口归 [python/pipeline](../python/pipeline/README.md))。模板目录的布局:`base/` 总是生成,`model/`(`--with-model`)与 `pipeline/`(`--with-pipeline`)是叠加在同一 crate 根上的层(同路径文件以叠加层为准);路径与 UTF-8 内容中的 `{{name}}`(目录名与 crate 名)、`{{crate}}`(snake_case)、`{{Type}}`(CamelCase)、`{{kind_id}}`(十进制)被替换。
2. 校验 `--kind-id`(十进制或 `0x` 十六进制)等于 [strategies/registry 的 kind_id 登记表](../strategies/registry/README.md) 中真实策略行的最大值 + 1(退役行仍算占用;测试段 `0xFF00–0xFFFF` 不经本命令分配),在登记表最后一个真实策略行之后加一行(kind_id、指向 `strategies/<name>/README.md` 的相对链接、scope `core`、Cargo feature `默认`),把该 crate 加进 workspace 的 `members`。kind_id 不等于下一个编号(已被占用或跳号,报告期望的编号)或 `strategies/<name>/` 已存在 → 退出 1,不写任何文件;缺 `--kind-id`、名字不是 `[a-z][a-z0-9-]*` 或模板缺所需的层 → 退出 2。
3. 在 `strategies/registry` 的 `strategy_set!` 调用末尾加一行 `<crate>::<Type>,`,并在 registry 的 `Cargo.toml` `[dependencies]` 加 `<name> = { path = "../<name>" }`(这两行是新增策略在装配处的**唯一**改动)。
4. 运行该 crate 的单元测试(`cargo test -p <name>`)与 `deps`;退出 0 表示骨架可构建,任一失败退出 1。

不改动任何引擎 crate、`tools/hft-cfg`、`python/`;[`scaffold-gate`](#cargo-xtask-scaffold-gate) 在 CI 里跑这一步并检查 diff。

### 其他 xtask 子命令

| 子命令 | 做什么 |
|---|---|
| `fixtures fetch` | 经 AWS CLI 与 zstd 获取 `fixtures/replay/*.pointer.toml` 的对象;预检、缓存复用、大小/哈希与失败清理的契约归 [fixtures 大文件指针](../fixtures/README.md#大文件s3-指针),CI 经 [OIDC](../docs/development.md#凭证与权限)只读 S3 |
| `fixtures extract --wal <s3-key> \| --journal <path> …` | 从旧 WAL v2 或 raw journal 抽取报文为 `.in` |
| `fixtures scrub [<路径>…]` / `fixtures check` / `fixtures corpus <venue>` | 清洗、敏感信息扫描、生成 fuzz 语料(见 [fixtures/](../fixtures/README.md));`check` 逐条报告文件与字节偏移,JSON 在 `target/xtask/fixtures.json` |
| `fixtures bbo-diff --wal <s3-key> --journal <path> --venue <v> --window <a..b>` | 离线对照:同一时段两边(现有 collector 与新框架)的 BBO 序列一致性,规则见 [fixtures](../fixtures/README.md) |
| `fuzz <target> [--time T] [--repro <input>]` / `fuzz --nightly` | 以 `tools/xtask/toolchains.toml` 里**固定日期的 nightly** 运行 `cargo fuzz`;崩溃输入最小化后写进 `fixtures/<venue>/regressions/`。`--repro <input>`:以 libFuzzer 默认限额对单个输入复现(服务器上的疑似 OOM/超时由 CI 用它复核,见 [hft-verify](#hft-verify))。`--nightly`:CI 每夜回归冒烟,按 `fuzz-targets.toml` 轮转、每目标短跑(合计时长归 testing §6),语料只存 CI cache,不计入门槛 ③。目标清单 `fuzz-targets.toml` 标出哪些是 core 解码器目标(验证包只打包这些,[development 验证包工作流](../docs/development.md#验证包工作流verify-bundleyml)) |
| `path-digest [--sha <SHA>] [--group <g>] [--json]` | 按 [development 验证包工作流](../docs/development.md#验证包工作流verify-bundleyml) 定义的求法计算验证包各目标组的 `path_digest`(路径清单读 `tools/xtask/fuzz-targets.toml` 的 `paths` 与 `[dst].paths`);验证包工作流用它填 `VERIFY.json`([hft-verify](#hft-verify)) |
| `journal-verify <dir> [--from-segment N] [--json]` | 逐段校验 journal 目录(段头、段链、记录结构与 CRC、多段组、引导块、封存、缺段汇总;检查项的唯一定义在 [journal-format 校验与缺段检测](../docs/subsystems/journal-format.md#校验与缺段检测),读取实现在 [hft-journal](../crates/hft-journal/README.md));`RECOVERY_GAP` 报告为"可能丢失的尾部",不算失败;任一损坏退出 1 并报告段号与偏移;已封存段的文件短于 `.seal` 记录的长度时,偏移报告为截断点(文件末尾);缺段区间报告为"缺段";`--from-segment N` 只保留段号 ≥ N 的结果,`--json` 另把报告(每段 `segment_no`、`ok`、`issues[]{offset, item, what}`、`recovery_gap`、`sealed`,以及 `missing`、`recovery_gaps`、`unsealed`)打印到 stdout。CI 对 `fixtures/replay/` 样本与 DST 产出的 journal 运行 |
| `setup` | 按 `tools/xtask/toolchains.toml`(`nightly = "nightly-YYYY-MM-DD"`;`[plugins]` 插件名 = 版本)安装固定版本的 cargo 插件(`cargo install <插件> --version <v> --locked`)与 nightly(`rustup toolchain install <nightly> --profile minimal`);`cargo <插件名去掉 cargo-> --version` 已输出该版本、`rustup toolchain list` 已列出该 nightly 时跳过,可重复运行 |
| `test-trace` | 测试规格(`TESTS.md`、`docs/tests/*.md`)与测试代码的追溯检查;检查项、输出与运行时机唯一归属 [docs/tests §6](../docs/tests/README.md#6-与代码的对应) |

### az-probe

选定部署 AZ/spine 时的探测。运行地点:首次在 M1.5 延迟探针之前、于候选 AZ 的 Spot 探针上运行(M1.5 探针与之后租用的实例都开在选定 AZ;[12 A1](../design/12-风险与未决问题.md));之后每周在生产 AZ(生产 CPG 的探针与生产主机)上只读复测;任何一次都不在 dev2 上运行。流程、时机与判据归 [runbooks/az-probe](../docs/runbooks/az-probe.md);本节只规定工具。

```sh
az-probe run --targets tools/az-probe/targets.toml --out report-<az>-<ts>.json [--duration 60m] [--ws-first-frame]
az-probe compare report-*.json > az-report.md
```

- 部署:apne1-az1/az2/az4 各 3–5 台 Spot 实例,每个 AZ 一个 CPG;覆盖至少一个活跃交易时段。
- 测量(每个端点主机名解析出的**每个 A 记录**,多次解析收集完整轮换):TCP 握手 RTT(每 IP 取 9 次的最小值与分布,沿用 `architecture-v2/research/_src/probe2.py` 的方法);TLS 握手耗时;`--ws-first-frame`:订阅公共行情,记录 `hw_rx − E`(需要 PHC 与 chrony 同步;无 PHC 的实例标记为 `unsynced`);端点属性:按 AWS `ip-ranges.json` 标注直连 EC2 / CloudFront / Cloudflare([03 §3.5](../design/03-网络与传输层.md))。
- 目标端点(`targets.toml`):core 交易所 Binance(`fstream`、`ws-fapi`、`fapi`、`stream`、`ws-api`、`api1–api4` 等)、OKX、Bitget 的公共 WS 与 REST;optional 交易所(Gate、Bybit、KuCoin、Hyperliquid)与对照端点按需加入。
- 输出:每 (AZ, CPG, 端点, IP) 的 minRTT/p50/p99、握手耗时、首帧单向分布与样本数;`compare` 按 (AZ, CPG) 的 core 交易所握手 minRTT 之和升序生成排名(取舍判据归 [runbooks/az-probe](../docs/runbooks/az-probe.md))。
- 周复测:生产 CPG 的探针与生产主机(统一升级主机)上以普通用户运行,只做 TCP/TLS 握手与 ping,不订阅私有流,不占隔离核(cgroup 限定在 housekeeping 以外的冷核)。**不在 dev2 上运行**(dev2 不在生产 AZ,结果不代表生产主机)。结果存档并由 `watchtower` 比对漂移。

### hostprobe

读取主机**实际生效**的参数(不是配置文件里写的),输出 `HostProfile`,并可与期望剖面比对。

```sh
hostprobe collect --out /run/hft/hostprofile.json
hostprobe verify --expected <release>/deploy/tuning/<profile>.toml --out /run/hft/hostprofile.json
hostprobe osnoise --cpus <hot> --duration 24h      # 主机基线与调优 A/B
```

采集项(每项记录"读自哪里"):

| 类别 | 项 | 来源 |
|---|---|---|
| 实例 | 实例类型、AZ ID、放置组、AMI | IMDSv2(以 root 运行;IMDS 锁定的主机上非 root 读不到,该项标 `unreadable`) |
| 内核 | 版本、`/proc/cmdline`(`isolcpus`、`nohz_full`、`rcu_nocbs`、`idle=`、`irqaffinity`、`mitigations`、`nosmt`) | `/proc/sys/kernel/osrelease`(即 `uname -r`)、`/proc/cmdline` |
| CPU | 型号、特性标志(与发布 `target-cpu` 要求比对)、隔离集合、`nohz_full` 集合、SMT 状态、cpuidle 驱动、时钟源 | `/proc/cpuinfo`、`/sys/devices/system/cpu/{isolated,nohz_full,smt/active}`、`/sys/devices/system/clocksource/*/current_clocksource` |
| 内存 | 各尺寸 hugepages 总数/空闲、THP 模式、`numa_balancing`、swap、`vm.*` 关键 sysctl、KSM | `/sys/kernel/mm/hugepages/*`、`/sys/kernel/mm/transparent_hugepage/enabled`、`/proc/sys/vm/*`、`/proc/swaps` |
| 网卡 | 网卡角色(ENI-A 管理、ENI-B 交易、ENI-C sim、启用时的 ENI-B2):按主机清单的设备序号(`device_index`,字段归 deploy)经 IMDS 识别网卡,得出接口名与私有 IP;MAC 只在运行期核对(IMDS 读出的 MAC 与接口的 `address` 一致,写进输出,`advisory`)、ENA 驱动版本、队列数、RSS 设置、合并参数(含 `adaptive-rx`、`rx-usecs`)、卸载、环大小、ntuple 规则、每个 TX 队列的 XPS 掩码、硬件时间戳能力、PHC 设备、ENA 整形计数(`*_allowance_exceeded`,采集时刻的快照;持续采集归 watchtower)、`/etc/hft/egress.toml` 中每个出口地址是否配在清单指定的网卡上并关联 EIP | IMDSv2(按设备序号;EIP 关联读 `network/interfaces/macs/<mac>/ipv4-associations/`)、`ethtool -i/-l/-x/-c/-k/-g/-n/-T/-S`(经 ioctl 读取)、`/dev/ptp*`、`/sys/class/net/*/address`、`/sys/class/net/<if>/queues/tx-*/xps_cpus` |
| 中断 | 每个 IRQ 的 `smp_affinity_list` 与 `effective_affinity_list`(按网卡角色与队列归类;NVMe 等 managed IRQ 单列,供核对它们不落在 IRQ 专用核)、irqbalance 是否运行 | `/proc/irq/*/smp_affinity_list`、`/proc/irq/*/effective_affinity_list`、`/proc/interrupts`、systemd |
| 网络栈 | `net.core.busy_poll`、`net.core.busy_read`、`tcp_autocorking`、`tcp_slow_start_after_idle`、`default_qdisc`;每接口 `napi_defer_hard_irqs`、`gro_flush_timeout`、`threaded`;`nf_conntrack` 模块是否加载 | `/proc/sys/net/*`、`/sys/class/net/<if>/*`、`/proc/modules` |
| 内核线程 | workqueue cpumask、`timer_migration`、`stat_interval` | `/sys/devices/virtual/workqueue/cpumask`、`/proc/sys/kernel/*`、`/proc/sys/vm/stat_interval` |
| resctrl | 是否挂载、L3 CAT 与 MBA 可用性、CLOS 数、热核组的 schemata | `/sys/fs/resctrl/info/*`、`/sys/fs/resctrl/<group>/schemata` |
| 时钟 | chrony 参考源、偏移、误差界;chrony 是否以 PHC 为 refclock 及其读取频率,与 hft-core 校准的 PHC 读取频率之和对照 ENA PHC 的读取上限(上限与 hft-core 的读取口径归 [hft-rt](../crates/hft-rt/README.md)) | `chronyc -c tracking`、`chronyc -c sources`、chrony 配置的 `refclock` 行 |
| 占用 | 隔离核与 IRQ 专用核上除预期线程外的可运行任务(登记的 keepawake 线程算预期线程,内核线程不计,见下);不允许的保持唤醒手段 | `/proc/*/task/*/stat` 的 `state`、`flags` 与 `processor` 字段;`/proc/cmdline`、cpuidle 驱动、`/dev/cpu_dma_latency` 的读出值 |
| 资源护栏 | `hft.slice`、根级 `hftbatch.slice` 与各子 slice 的 `CPUQuota`(设了的)、`CPUWeight`(含 `cpu.idle`)、`AllowedCPUs`、`MemoryMax`、`MemorySwapMax`、`IOReadBandwidthMax`/`IOWriteBandwidthMax` 实际值;`cgroup.subtree_control`;`system.slice` 的 `cpu.pressure`/`memory.pressure`;dev2 护栏守护 `hft-dev2-guard.service` 是否 active | `/sys/fs/cgroup/{hft.slice,hftbatch.slice}/**`、`/sys/fs/cgroup/cgroup.subtree_control`、`/sys/fs/cgroup/system.slice/*.pressure`、systemd |
| IMDS 锁定 | 根 cgroup 的 `connect4`/`connect6` 上挂载的 BPF 程序(名字与 id;dev2 以外的主机类);`imds/hft-imds-dev2.nft` 定义的表与规则是否已加载(dev2);机制与期望归 [deploy](../deploy/README.md#imds-锁定与凭证代理) | `bpf(BPF_PROG_QUERY)`(根 cgroup)与 `/sys/fs/bpf/hft-imds/`;nftables netlink 查询 |

- **不做连接级检查**:某条连接落在哪块网卡(NAPI 归属)由 conn-builder 在交接前经 netdev netlink 自检([hft-net](../crates/hft-net/README.md) `Connector::napi_ifindex`,规则与处置归 [hft-core 主机资源隔离](../apps/hft-core/README.md#主机资源隔离slice核与出口-ip));hostprobe 在 hft-core 建连之前运行,读不到其他进程的 socket。
- **IRQ 专用核"保持唤醒"的允许形式**(本节是唯一登记处;[hot-path-rules T12](../docs/hot-path-rules.md) 与 [hft-rt `TASK_ON_HOT`](../crates/hft-rt/README.md) 链接这里):只有两种——内核参数 `nohlt`(arm64 上是否支持、能否改变 WFI:待核实(方法:M1.5 探针上对比 IRQ 核的 cpuidle 状态计数与 `nic_to_kernel`)),或期望剖面里按线程名登记的 keepawake 线程(每个 IRQ 专用核至多一条,不做 I/O;线程名、单元与是否启用归 [deploy](../deploy/README.md))。hostprobe 把登记的 keepawake 线程算作预期线程;其他手段(`cpuidle.off`、haltpoll、`/dev/cpu_dma_latency` 的生效值低于默认值——有进程持有约束、未登记的自旋线程)都按"占用"报告(`occupancy.keepawake_violations`;hostprobe 不读 `/proc/*/fd`,所以报告生效值而不是持有者)。是否保持唤醒由 M1.5 的 A/B 决定([11 §3](../design/11-实施路线图.md))。
- 输出是规范化 JSON(键排序),附内容的 SHA-256(`host_profile_hash`);另含 `pending_reboot`(发布内 GRUB 片段的参数与当前 `/proc/cmdline` 不同时为 `true`,判定条件归 deploy)。读不到的项写 `unreadable`(附原因),不省略。**本节是采集项清单与输出格式的归属地**;哪些项是 `required`、期望值是什么由 [deploy](../deploy/README.md) 生成(收包模型的四项参数——`busy_poll`、`busy_read`、ENI-B 的 `napi_defer_hard_irqs` 与 `gro_flush_timeout`——与 ENI-B/ENI-A 的 IRQ 落点在升级主机类上是 hard 比较项,规则归 [hft-net](../crates/hft-net/README.md) 与 hft-core)。
- hft-core 启动时读取该文件,自己与 `[host.expect]` 逐项比较(比较项与不符时的退出码归 [hft-core](../apps/hft-core/README.md)),把摘要与比较结果位写成 `HostProfile` 事件,把 JSON 完整内容写成 `ConfigBlob{HostManifest}`——journal 因此自足([13 R6](../design/13-设计评审与优化.md))。
- `verify`:给没有 hft-core 的场合用——部署激活检查、参考机(`bench-gate --reference`)、探针机。先照 `collect` 写出 `--out`,再逐项与期望剖面比较:每个不符项向 stderr 打印一行 `<键> expected=<期望> actual=<实际> required|advisory`,并写进输出 JSON 的 `verify` 段;退出 0 = 全部 `required` 项相符(`advisory` 不符只列出);1 = 任一 `required` 项不符或为 `unreadable`;2 = 期望剖面缺失或无法解析、`--out` 不可写。`pending_reboot` 不单独影响退出码。期望剖面随发布归档([deploy/](../deploy/README.md))。
- **期望剖面的格式**(`verify --expected` 读取;取值由 deploy 生成,本节只定格式):TOML,`[[expect]]` 每项 `key`(输出 JSON 的点分键,对象键本身可含点,如 `guard.slices.hftbatch.slice.cpu_idle`;`[*]` 展开数组)、恰好一个比较符(`eq = <值>`、`lt`、`le`(可带 `add`:实际值加它再比,用于"chrony 与 hft-core 合计的 PHC 读取频率")、`contains_cpus`、`disjoint_cpus`(CPU 列表)、`empty = true`、`present = true`)与 `level = "required" | "advisory"`;可选 `grub`(发布内 GRUB 片段的路径,相对本文件)、`[roles]`(网卡角色 → 设备序号,缺省 `eni_a = 0`、`eni_b = 1`、`eni_c = 2`、`eni_b2 = 3`)、`[occupancy]`(`irq_cpus` IRQ 专用核,缺省为 ENI-B IRQ 实际所在核;`keepawake` 登记的线程名);其他顶层键忽略(`benchmarks/reference.toml` 另记 `host_class`、`path_absolute`)。`collect` 经 `/etc/hft/host-local.toml` 的 `manifest` 读 `/opt/hft/current-core/deploy/tuning/<清单名>.toml` 取同样的输入,没有时用缺省值。
- `--root <dir>` 以夹具根目录代替 `/`(unit/table 用例在任意平台运行;非文件的来源——IMDS、`chronyc`/`rtla` 输出、ioctl、BPF 与 netlink 查询、`getifaddrs`——的替身放在 `<dir>/.hostprobe/`,布局见 `tools/hostprobe/src/host.rs`);读真实主机只在 Linux 上。
- `osnoise`:包装 `rtla osnoise`/`timerlat`,输出隔离核的 max 与 p99.9;升级主机类(`c16`/`c32`)与参考机的验收:max < 20 µs,p99.9 < 5 µs([02 §6](../design/02-硬件与托管方案.md))。

### hft-secrets

在服务启动前把该服务需要的秘密(交易所 API key、Ed25519 私钥;optional 链上组件的钱包私钥)从 AWS Secrets Manager / KMS 取出,写成 tmpfs 上的文件交给服务读取。**hft-core 与其他 Rust 服务都不链接 `aws-sdk-secretsmanager` 与 `aws-sdk-kms`**(hot crate 的闭包不含任何 AWS SDK:它依赖 tokio,规则 [B6](../docs/hot-path-rules.md);冷进程访问 S3 另有各自的依赖节);取秘密这件事只发生在这个冷工具里。模拟账户不持有交易所 key,清单里没有它们的条目。

```sh
hft-secrets fetch --manifest /etc/hft/secrets/<unit>.toml --out /run/hft/<unit-dir>/secrets --owner <user> --mode 0400
hft-secrets --check --manifest /etc/hft/secrets/<unit>.toml      # 只校验清单(部署预检用)
```

systemd 用法(以 `hft-core@prod` 为例,单元与运行目录布局归 [deploy/](../deploy/README.md)):

```ini
RuntimeDirectory=hft/core-%i          # /run/hft/core-<instance>/:本单元独占的运行期目录(tmpfs;模式与属组归 deploy);停止时 systemd 删除
# 只在 prod 的 drop-in;路径是本单元所在组的链接 current-core(deploy)
ExecStartPre=+/opt/hft/current-core/bin/hft-secrets fetch --manifest /etc/hft/secrets/hft-core.toml --out /run/hft/core-%i/secrets --owner hft --mode 0400
```

- **清单** `/etc/hft/secrets/<unit>.toml`(随发布归档,只含引用不含值):每项 `name`(输出文件名,如 `binance-usdm.<account>.ed25519`)、`secret_id`(ARN)、`json_key`(可选)、`kind = "plain" | "kms-ciphertext"`;出现其他键(包括任何形式的明文值字段)或 `name` 重复 → 退出 2,不取任何秘密。`--check`:只解析并按同一实现校验清单(键集合、`name` 非空且不重复、`kind` 取值、`secret_id` 以 `arn:aws:secretsmanager:` 开头),不访问网络、不读实例角色、不写任何文件;合法退出 0,不合法或文件不可读退出 2。交易 key 不带提现权限,并绑定 IP 白名单([runbooks/launch](../docs/runbooks/launch.md))。
- **输出**:每项一个文件,文件 `0400`、属主为服务用户;`secrets/` 子目录 `0700`。服务启动时逐个读入锁定内存(`mlock`,缓冲标记 `MADV_DONTDUMP`),**读完立即 `unlink`**;服务停止时 systemd 删除整个 `RuntimeDirectory`。
- **全有或全无**:任一项取不到 → 删除已写出的部分,退出 1,`ExecStartPre` 失败,服务不启动;退出 2 = 环境错误(无实例角色、IMDS 不可达)。
- **日志只记名字与版本**:stdout 输出每项的 `name` 与 Secrets Manager `VersionId`,从不输出值。
- 每个单元一份清单、一个运行期目录:hft-core、kill-switch(与 optional 的 chain-core)各取各的,不共享目录(见 [defensive-patterns](../docs/defensive-patterns.md))。`hft-core@sim` 没有清单,单元里也没有本命令。

**威胁模型**

| 防住什么 | 怎么防 |
|---|---|
| 秘密进 git、配置文件、journal、环境变量、core dump | 清单只有引用;值只存在于 tmpfs 文件(秒级寿命)与服务进程的锁定内存;`LimitCORE=0`、`PR_SET_DUMPABLE=0`;journal 不记录 key、签名原文与登录帧([event-model](../docs/subsystems/event-model.md)) |
| 秘密落到持久盘 | 只写 tmpfs;生产 `swapoff`;缓冲 `mlock` |
| 同机其他用户或冷进程(ops_bot、watchtower、pipeline)读到秘密 | 文件 0400 + 目录 0700 + 属主为服务用户;ops_bot 进程不持有任何 key;正式实盘主机(`formal_live = true`)上 IMDS 只对 uid 0 开放,dev2 上只对 hft 各 uid 丢弃 IMDS;两处的 hft 冷进程都只拿凭证代理发放的、只覆盖各自 S3 前缀的短期凭证(nftables 规则与 `hft-cred-broker.service` 归 [deploy](../deploy/README.md)) |
| 一个服务拿到别的服务或别的账户的 key | IAM 策略按秘密 ARN 授权;清单按单元拆分;每个账户是不同的秘密;启动闸门 `IDENTITY` 核对 key 对应的交易所 UID 与 refdata 登记一致 |
| 事后追查谁取了什么 | CloudTrail 记录每次 `GetSecretValue`/`Decrypt` |

**不防**:主机 root 被攻破;服务进程自身被利用。缓解在秘密的**权限**上:交易 key 禁提现 + IP 白名单(EIP);kill-switch 优先使用只有撤单/查询权限的 key(各所能否限定到撤单权限待核实(方法:各所 API key 权限设置与测试网实测),[12 §3.3](../design/12-风险与未决问题.md))。

### hft-cfg

refdata、`RiskParams`、`EngineConfig` 的**唯一来源**,也是 `StrategyParams` 的人工通道:把受版本控制的 TOML(`params/` 目录,不含秘密)编译成 blob 并输出差异报告;可选地经 ctl 把 `SetParams`/`SetRisk`/`ReplaceRefdata` 发给 hft-core。Rust CLI,cold tier,**复用各归属 crate 的构建器与加载期校验**,每种 blob 只有一个编码器:refdata/ECF1 在 [hft-refdata](../crates/hft-refdata/README.md)(`RdbBuilder`/`EcfBuilder`),SPB1 头在 [hft-event](../crates/hft-event/README.md#strategyparamsspb1),参数 body 在 [hft-strategy-sdk 的 `params` 模块](../crates/hft-strategy-sdk/README.md),`RiskParams` 在 [hft-risk](../crates/hft-risk/README.md)。

```sh
hft-cfg build  --kind refdata  --in params/refdata/ --out refdata.blob [--against <当前 blob 或 journal 目录>] [--sim-models <hft-core 配置>] \n               [--schemas <dir>] [--admins <名单文件>]
hft-cfg build  --kind strategy --schemas <release>/schemas/strategies --refdata <refdata.blob | journal 目录> \
               --in params/strategies/<instance>.<account>.toml --out <file>.blob [--against <当前 blob 或 journal 目录>]
hft-cfg build  --kind risk   --in params/<file>.toml --out <file>.blob [--against …]
hft-cfg build  --kind engine --in params/<file>.toml --core-config <hft-core 实例配置> --out <file>.blob [--against …]
hft-cfg refdata fetch --venue <venue> --out params/refdata/insts-<venue>.toml   # 拉 instruments 接口,生成待审 TOML
hft-cfg diff   --old <a.blob> --new <b.blob> [--schemas <dir>] [--refdata <refdata.blob>] [--json]   # strategy blob 需要 --schemas
hft-cfg show   <file.blob> [--schemas <dir>] [--refdata <refdata.blob>]   # strategy blob 按 schema 与 refdata 渲染字段名、人读单位、资产与 inst 名
hft-cfg send   --kind strategy|risk|refdata --blob <file.blob> --account <name> [--socket /run/hft/core-<instance>/ctl.sock]
```

**策略参数:按 schema 通用的 TOML → blob**

- **schema 来源**:发布制品里由 `hft-core schemas --out <dir>` 导出的目录 `schemas/strategies/`(本地开发用已提交的金样 `fixtures/strategy-schemas/`),这是 schema 导出的**唯一**命令;文件命名与 `index.json` 的格式唯一归属 [strategies/registry](../strategies/registry/README.md):每个编译进二进制的策略种类一个 `<kind_id>-<name>.schema.json`(内容——参数字段与 `notes`、`report`、`cmds` 描述符——由 SDK `params` 产生,格式归 [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md);摘要不在文件内,由 SDK 按文件内容重算),`index.json` 是按 `kind_id` 升序的数组,每项 `{kind_id, name, file, schema_id, schema_version, schema_digest}`(`schema_id = kind_id`)。

  hft-cfg 先读 `index.json`,按 `instances.toml` 的种类名或 `kind_id` 定位文件,并核对按文件重算的摘要(`OwnedSchema::schema_digest`)与索引一致;hft-py、pipeline、watchtower、journal-archiver 用同一目录。hft-cfg **不链接任何策略 crate**,只链接 SDK 的 `params` 编解码器;新增策略、给策略加字段都不改 hft-cfg。
- **TOML 形状**(字段名来自 schema,未知字段即报错;下例用 core 策略的两个字段):

  ```toml
  [strategy]
  instance = "pc-main"          # refdata Instances 中的实例名 → StrategyId 与 kind_id
  account  = "pc-live"          # 必须绑定到该实例
  schema   = { id = 1, version = 1 }   # id = kind_id(registry 约定)
  version  = 42                 # 每 (instance, account) 单调递增
  source   = "manual"

  [defaults]                    # 非 per_group 字段与 per_group 字段的默认值;人读单位的十进制字符串
  slice_max_chunk = "500"

  [[groups]]
  n = 101                       # 实例内计数器;hft-cfg 合成 GroupId = (StrategyId << 24) | n(构造与"永不复用"归 hft-types)
  asset = "AIN"
  open_allowed = true
  legs = [ { inst = "binance-usdm:AINUSDT" }, { inst = "bitget-uta-spot:AINUSDT" } ]
  [groups.overrides]            # 只允许 per_group 字段;Qty/Px 按本 group 资产的指数换算
  coin_abs_cap = "5000"
  ```

- **编码**:解析 TOML → 按 schema 校验字段名、单位、`min/max`、`per_group`、`since`(字段在该 schema 版本是否存在)→ 调 SDK `params` 编码 body(规范编码与 body 布局的唯一定义在 SDK)→ 用 refdata 解析实例、账户与 inst,写 `StrategyParamsHdr`(含 schema 摘要)与 `GroupSpec[]`。同一输入两次编码逐字节相同。
- **校验**(失败退出 1,不产出 blob):schema 摘要与所读 schema JSON 一致;**账户绑定**(`AccountMeta.bound_instance == 实例`,否则 `AccountBound`)与实例种类一致;该种类 `non_prod_only` 时账户不得 `is_prod`(`NonProdOnly`);每个 group 的 refdata 一致性(腿 inst 存在且可交易、venue 在账户内、同基础资产、同报价资产、任两腿不在同一交易所 → `SameExchangeLegs`);`n ∈ 1..=0xFF_FFFE`,合成的 `GroupId` 在 blob 内唯一;**GroupId 续号与水位**(全仓唯一的分配规则,hft-cfg 与 pipeline 相同,pipeline 链接这里;构造与取值范围归 [hft-types](../crates/hft-types/README.md),头字段 `group_ctr_hwm` 的布局归 [hft-event SPB1](../crates/hft-event/README.md#strategyparamsspb1)):`--against` 给出当前生效的 blob(或 journal 目录中该 (instance, account) 最后生效的 `StrategyParams`),记 H = max(生效 blob 的 `group_ctr_hwm`, 生效 blob 中本实例的最大 `n`);其中没有的 `GroupId` 为新增 group,新增 group 的 `n` 必须 > H(被移除的 group 的 `n` 因此永不再用);其中已有的 `GroupId` 的 `GroupSpec::fingerprint()`(基础资产 + 各腿)必须与生效 blob 相同(同一 id 永不指向第二个指纹);新 blob 的 `group_ctr_hwm` = max(H, 新 blob 中本实例的最大 `n`)(只增不减)。不带 `--against` 时只接受该 (instance, account) 的第一版(`version = 1`,H = 0);`version > 1` 而缺 `--against` → 退出 2。策略自己的 `validate`(跨字段约束)与 Model 内存预算不在 hft-cfg 里跑:`send` 的第一步预览由 gateway 调策略代码校验并返回结果;两腿 lot 能否对齐(`InvalidArgs{LotMismatch}`)同样在预览时由 sequencer 判定,hft-cfg 只在 `diff` 报告里打印每个 group 两腿的 lot 与 1/1 对冲比下的 `pair_lot`(hft-types),供人提前发现。
- **Python**:pipeline 与研究脚本读同一份 schema 目录了解字段,经 [hft-py](../crates/hft-py/README.md) 包装的同一个 SDK `params` 编解码器编码 body(或直接调用 `hft-cfg build`);不存在纯 Python 编码器或任何第二个编码实现。金样 `fixtures/params/*.toml` → blob 在 Rust 与 Python 两条路径上逐字节相同。

**refdata:账户、模拟账户、能力表与绑定**

`params/refdata/` 下的 TOML 分文件维护:`assets.toml`、`venues.toml`、`insts-<venue>.toml`(可由 `refdata fetch` 生成后人工审阅)、`venue_caps.toml`、`instances.toml`、`accounts.toml`、`fees.toml`、`limits.toml`、`endpoints.toml`、`maint.toml`。

```toml
# instances.toml —— StrategyInstanceId 的唯一分配处(id 永久绑定 kind 与 name,只追加)
[[instance]]
id = 1
name = "pc-main"
kind = "portfolio-carry"      # 按 schema 目录的 index.json 解析为 kind_id

# accounts.toml
[[account]]
id = 3
label = "pc-live"
kind = "live"
is_prod = true
bound_instance = "pc-main"
venues = ["binance-usdm", "bitget-uta-spot"]   # 每个交易所至多一个市场(同所现货 + 永续 → SameExchangeMarkets)
venue_uids = { binance-usdm = "123456789", bitget-uta-spot = "987654321" }

[[account]]
id = 4
label = "sim-pc-a"            # sim 实例的账户名必须以 "sim-" 开头(AccountNamePrefix)
kind = "sim"
bound_instance = "pc-sim-a"   # 每个模拟账户也只绑定一个实例;一个实例也只被一个账户绑定
venues = ["binance-usdm", "bitget-uta-spot"]
[account.sim]
fee_tier = 0                  # fees.toml 中 (venue, tier) 的档号;本账户若有覆盖行以覆盖行为准
latency_model = 0             # hft-core 配置 [[sim.latency_models]] 的下标(模型定义归 hft-sim)
margin_model = 0              # hft-core 配置 [[sim.margin_models]] 的下标
initial_balances = [ { venue = "binance-usdm", asset = "USDT", qty = "10000" },
                     { venue = "bitget-uta-spot", asset = "USDT", qty = "10000" } ]

# venue_caps.toml —— 只写覆盖;默认值来自各 venue-* 的 VenueProtocol::default_caps()
[venue_caps.binance-usdm]
fee_asset_rule = { kind = "quote" }
required_position_mode = "one-way"
required_margin_mode = "cross"
required_multi_asset = false
margin_data = { kind = "poll", interval_ms = 5000 }
reason = "单向持仓 + 全仓;保证金率 5 s 轮询"

[venue_caps.bitget-uta-spot]
required_spot_borrow = false  # 现货杠杆/借币必须关闭(AccountConfigReport 的 AUTO_BORROW 位核验;默认值本就如此,写出来只为审阅)
reason = "只做多现货,从不借币"
```

联调用的交易所测试网/模拟盘账户写 `kind = "live"`、`is_prod = false`,只放进联调实例使用的那份 refdata(与正式账户的 refdata 分开构建);读不到 UID 的测试网账户另写 `uid_policy = "unverifiable"`(取值 `"verified" | "unverifiable"`,缺省 `"verified"`,对应 `UidPolicy::{Verified, Unverifiable}`)与 `uid_policy_signoff = "<Admin 的 GitHub 登录名>"`,签字人必须在 `.github/hft-admins.txt` 名单内(否则退出 1),差异报告列出签字人(字段语义与限制归 [hft-refdata](../crates/hft-refdata/README.md#数据结构));永续杠杆不在任何 TOML 里——它在交易所账户上设置,运行期由 `AccountConfigReport` 读回([hft-refdata](../crates/hft-refdata/README.md#交易所能力表venuecaps))。

- **TOML 写法**:目录内各 `*.toml` 按文件名合并(表逐键合并,顶层数组串接,其余重复键退出 1),也可给单个文件。表与键即 `hft_refdata::dump` 的输出:`[assets.<名>]`、`[venues.<venue>]`(含 `rate_limits`、`endpoints` 数组)、`[caps.<venue>]`、`[insts."<venue>:<符号>"]`、`[instances.<名>]`、`[accounts.<label>]`(`uids`、`sim.balances`)与 `fees`/`fee_overrides`/`aliases`/`maint`/`tokens` 数组,所以转储可原样再构建;实例与账户另可写成上例的 `[[instance]]`/`[[account]]`(`venue_uids`、`initial_balances` 即 `uids`、`sim.balances`)。枚举取值小写(`one_way` 与 `one-way` 同义),venue 名写 `binance-usdm` 或 `BINANCE_USDM`;定点量写 `<键>_raw`(原始整数)或 `<键>`(十进制字符串,按资产指数经 `parse_exact`);inst 的 tick/lot/min_qty 只写交易所原文时按线格式指数换算。实例的 `kind = "<种类名>"` 经 `--schemas` 的 `index.json` 解析(也可写 `kind_id`);签字人名单缺省 `.github/hft-admins.txt`,`--admins` 可改。构建器按 id 等规范顺序调用,同一内容恒得同一字节;构建打印报告(签字人、`warnings()`,带 `--against` 时另有差异报告,`--json` 同内容)。
- **能力表**:hft-cfg 链接各 core `venue-*` crate(纯协议,无 I/O)取 `default_caps()` 与 `dead_man()`,以它为底叠加 `venue_caps.toml`,把最终整行写进 blob(该 venue 的协议 crate 未链接时,底行取 `[caps.<venue>]` 的完整行;已链接时只允许写覆盖);覆盖规则(能力位只能关、`ignorable_user_events` 只能追加、`required_*`/`fee_asset_rule`/轮询间隔/死手的超时与刷新可改且必须写 `reason`)与字段语义归 [hft-refdata VenueCaps](../crates/hft-refdata/README.md#交易所能力表venuecaps)。
- **账户与模拟账户校验**(调用 hft-refdata 的加载校验,失败退出 1):账户名前缀(`AccountNamePrefix`:Sim 账户的 `label` 必须以 `sim-` 开头、Live 账户的不得以它开头——一份 refdata 只服务一个实例角色,规则归 [apps/hft-core](../apps/hft-core/README.md));每个账户在同一交易所至多一个 venue(`SameExchangeMarkets`);每个实例至多被一个账户绑定(`InstanceMultiBound`);Live 账户每个 venue 都有 UID(`uid_policy = "unverifiable"` 的账户可留空;它只允许 `is_prod = false`)、每 venue 实盘账户 ≤ 12;同一 (交易所, UID) 登记在两个账户上是 `DuplicateVenueUid` 错误,唯一例外是两个 `is_prod = false` 账户在同一交易所的不同 `VenueId` 上——加载成功,该条进 `RefData::warnings()` 并列在差异报告中(规则归 [hft-refdata](../crates/hft-refdata/README.md));Sim 账户 `is_prod = false`、有初始余额、无 UID、余额的 venue ⊆ `venues`、`fee_tier` 档在 `fees.toml` 中存在;`latency_model`/`margin_model` 下标是否在 hft-core 配置的表内由 hft-core 启动时检查(hft-cfg 给出 `--sim-models <hft-core 配置>` 时预检);`bound_instance` 指向已登记的实例;实例名唯一;`--against` 时再检查不可变量(资产指数、合约乘数、账户 id/类型/`is_prod`/UID、实例的 kind 与名字)。
- **差异报告**:新增/删除的 inst、账户与实例,绑定变化(提示"账户有仓位时 hft-core 会拒绝启动"),能力表逐字段变化,按账户逐字段的放宽/收紧;strategy blob 逐字段的变化按 schema 的 `FieldDesc.minor_change`(`None`/`RelPct`/`Abs`/`Any`,属性定义归 [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md))判定是否属于小改,并标出改动了的结构字段(`structural`)与受影响的 group——门槛 ⑦ 的重新计时只读这份判定([testing §8](../docs/testing.md#8-上线门槛));`engine` blob 的容量变小时提示"缩容只经 Drain + 重启,启动时当前占用超过新容量 hft-core 会拒绝启动(`CapacityShrink`)";人读文本 + `--json`。分类只是预览:**升级/降级的权威判定在 sequencer**。

**发送与其余**

- **发送**:`send` 以 `cli:<login>` 身份连本实例的 ctl socket(路径 = hft-core 配置 `paths.ctl_socket`,位于 `/run/hft/core-<instance>/`;运维账户访问 socket 的组成员资格归 [deploy](../deploy/README.md);strategy 需 Trader 或 Admin;risk、refdata 需 Admin,refdata 只在维护窗口),按 [control-plane 的二次确认协议](../docs/subsystems/control-plane.md#版本与确认) 先发预览、打印 sequencer 返回的 `CmdResult{preview}`(第一行是目标账户、账户类型与对照账户的有效模式),人工确认后带 `confirmed` 与 `expect_state_version` 重发;`StateChanged` 时重新预览;被拒时打印结果码与 detail 名(如 `Rejected{RefdataRecheck}`)。Telegram 不直接发 `SetParams`/`SetRisk`;ops_bot 的参数编辑命令改的是 pipeline 的策略文件,由 pipeline 产出 blob 后以自己的身份下发。
- **`EngineConfig`**:按主机类给出(oms/ledger/容量/批量上限/每事件出环上限 K/环容量),随 hft-core 配置部署,启动时记为 `ConfigBlob{Engine}`;变更需要重启,`send` 不接受 `engine`。ECF1 段 5 中取值归 hft-core 实例配置的三项由 `--core-config` 读入,不在 `params/` 里另写:`advisor.mem_budget_mib` → `advisor_mem_budget_mib`、`advisor.checkpoint_max_mib` → `advisor_ckpt_max_mib`、`startup.allow_missing_standby` → `allow_missing_standby`(段 5 字段归 [hft-sequencer](../crates/hft-sequencer/README.md),hft-core 启动时核对两边相等);配置缺其中任一键 → 退出 2;三键出现在 params 文件任何位置 → 退出 1。params 文件写 `version`、`host_class` 与各段表:`[capacities]`(段 3)、`[rings]`(段 4,键为 `RingCapKind` 名的 snake_case)、`[sequencer]`(段 5 其余字段,`rings.<RingClass 名>.{per_ring, total}`)、`[conn.rotate_margin_s]`(段 6,按 venue);段 1、2、7 在归属 crate 定义行格式后加入(未知表退出 1)。ECF1 与 refdata 之间的交叉校验(含段 2 每个 `ref_inst` 的 `RefInstNotSubscribed`,规则归 [hft-refdata](../crates/hft-refdata/README.md))只在 hft-core 启动时由 `EngineConfig::load` 的验证器进行,hft-cfg 不预检。
- **`refdata fetch`**:冷路径联网工具,按 venue 拉 instruments/exchangeInfo,按 [hft-refdata 的指数规则](../crates/hft-refdata/README.md#实现说明)生成 inst 行与资产指数建议,排除非 10 的幂合约乘数与交割合约;输出供人审阅的 TOML,不直接产出 blob。
- 退出码:0 成功;1 校验或差异被拒;2 输入/环境错误(文件缺失、socket 不可达、schema 目录缺失)。

### hft-verify

服务器上的验证包驱动:在预算内运行 CI 构建好的 core 解码器 fuzz 目标与 DST 运行器,累计上线门槛 ③ fuzz 与 ④ DST。**账本规则(只有失败清零、疑似须复核、漏跑不清零、按 UTC 日计次、种子不重叠、按 `path_digest` 跨版本累计、重建、候选版本补跑)唯一归属 [testing §6.1](../docs/testing.md#61-服务器上的累计运行),阈值在 [testing §8](../docs/testing.md#8-上线门槛)**;验证包的打包、安装与单元(`hft-verify.service`/`.timer`、用户 `hft-verify`、slice 与调度、`ExecCondition`)归 [deploy](../deploy/README.md#验证包与-hft-verify),`path_digest` 的求法归 [development 验证包工作流](../docs/development.md#验证包工作流verify-bundleyml),`hft-dst` 归 [hft-testkit](../crates/hft-testkit/README.md)。本节是命令、轮转、计量、失败分类、`VERIFY.json` 字段、运行记录、账本文件与状态目录的归属地。

```sh
hft-verify run --budget-min N [--bundle /opt/hft/verify/current] [--state /var/lib/hft/verify] [--fuzz-share 0.5] [--slice-min 15]
hft-verify ledger show [--json] [--candidate <ver> [--candidate-bundle <dir>]] [--runs-from <dir>]...
hft-verify ledger rebuild [--runs-from <dir>]...
hft-verify crash list [--json] [--status failure|suspect|dismissed]
hft-verify crash resolve <crash-id> (--reproduced | --not-reproduced) --evidence <CI 运行 URL>
```

- **计量**:全部累计量按**子进程 CPU 时间**计(子进程结束时 `wait4` 返回的 `ru_utime + ru_stime`,含其全部线程),不按墙钟。`--budget-min` 与 `--slice-min` 是墙钟上限,只决定何时停下;批处理所在的 `hftbatch.slice` 是 `CPUWeight=idle`,墙钟与 CPU 时间之比随主机负载变化,不影响累计口径。
- **`run`**:启动时校验验证包(`VERIFY.json` 的逐文件 SHA-256、目标清单与各目标组的 `path_digest`;不符退出 2),在 `N` 分钟内**串行**运行(同一时刻只有一个子进程):
  1. fuzz 部分(预算 × `--fuzz-share`):按本机账本中各 core 目标的累计 CPU 秒数从少到多轮转,每个目标一个时间片(≤ `--slice-min`)。子进程为包内 `fuzz/<target>`(libFuzzer):`-max_total_time` = 时间片;`-rss_limit_mb` 在每个时间片开始时按所在 slice 的剩余内存计算(slice 的 `memory.max` − `memory.current` + 本单元当前用量,再与单元 `MemoryMax` 取小)减去余量 64 MiB(驱动自身 < 32 MiB 与 ASan 隔离区 16 MiB 之和向上取整),按 MiB 向下取整;`-malloc_limit_mb` 取同一值;算出的值 ≤ 0 → 不启动子进程,写一条 `skipped = "insufficient_memory"`、`cpu_seconds = 0` 的段记录,本次运行结束并退出 0(`RUN.json` 的 `skipped` 同值;账本按漏跑处理,规则归 testing §6.1);`-timeout` ≥ 600 s,只作挂死兜底(libFuzzer 按墙钟判定);环境 `ASAN_OPTIONS=quarantine_size_mb=16`(其余 ASan 选项随验证包固定)与 `HFT_FUZZ_TIMEOUT_DIR=crashes/<target>/suspect/`。**单输入超时**的判定与存法(harness 写进 `HFT_FUZZ_TIMEOUT_DIR` 后照常返回)唯一归属 [testing §3.3](../docs/testing.md#33-fuzz);本工具只设置该目录并在段结束时把其中的输入归为疑似。语料目录 `corpus/<target>/`(首次运行某目标时以包内初始语料为种子,之后在本机累积,不随版本清空;超过上限(待标定)时该目标的下一个时间片先以 `-merge=1` 收缩)。
  2. DST 部分(其余预算):调用包内 `bin/hft-dst run`,按截止时间运行一段**本主机连续的新种子区间**,批次 ≤ 5 min;`--crash-dir` 为 `crashes/dst/`,`hft-dst` 在每个种子开始前把该种子写进其中的进行中标记 `.inflight.json`(格式归 [hft-testkit](../crates/hft-testkit/README.md#8-hft-dst-运行器)),子进程异常结束时据此确定当时的种子;上一次运行留下的待重跑种子(见下表)在本次 DST 部分最先运行。种子编码 `seed = 1 << 63 | host_tag << 40 | n`(`host_tag` = 主机名(`gethostname`,UTF-8)的 SHA-256 最后两个字节按大端解释;`n` 每主机单调,取本机 `runs/` 与账本中本主机已用区间的最大 `n` + 1,无记录时为 0),与 CI 由 SHA 或日期派生的种子不重叠,`host_tag` 不同的主机之间也不重叠(`host_tag` 碰撞时重叠部分按 testing §6.1 规则 5 只计一次)。

  每个 fuzz 时间片或 DST 批次结束时写一条**段记录**(临时文件 + `fsync` + 原子改名),全部段写完后更新本机账本缓存;被强杀时只计已写出的段。有失败时该运行以退出码 1 结束(告警途径归 deploy);只有疑似时退出 0,`RUN.json` 记 `suspects > 0`。
- **失败分类**(每段结束时判定;各类怎样影响账本归 testing §6.1):

  | 子进程结果 | 分类 | 记录 |
  |---|---|---|
  | fuzz:libFuzzer 报告崩溃(`crash-*`、`leak-*`、ASan/UBSan 报错、断言),或进程因 SIGABRT、SIGSEGV、SIGBUS、SIGILL、SIGFPE 结束 | 失败 | 输入移入 `crashes/<target>/<sha1>`,以 `-minimize_crash=1`(限时 60 s)生成 `<sha1>.min` |
  | fuzz:内存超限(`oom-*`)或单输入超时(libFuzzer 的 `timeout-*`,以及 harness 写进 `HFT_FUZZ_TIMEOUT_DIR` 的输入) | 疑似 | 输入在 `crashes/<target>/suspect/<sha1>`(同样最小化),等复核 |
  | DST:`hft-dst` 报告不变量违反 | 失败 | `crashes/dst/<seed>.json`(种子、违反的不变量、journal 片段路径) |
  | DST:`hft-dst` 因上列致命信号结束 | 失败 | 种子取自进行中标记,同上 |
  | 任一子进程被 SIGKILL、SIGTERM 或 SIGINT 终止(cgroup OOM、护栏、单元停止),或 libFuzzer 以退出码 72(被中断)结束 | 中止,不是失败 | 只计已写出的段;DST 进行中标记里的种子不计入,记进段记录的 `rerun_seed`,由本主机下一次运行重跑 |
  | 本进程收到 `SIGTERM` | 中止 | 向子进程发 `SIGINT`、等其退出 ≤ 10 s,仍未退出 → `SIGKILL` 并等其结束;按实际 CPU 时间与已完成的种子写最后一段后退出 0 |

- **疑似的复核**(只有 fuzz 有疑似):疑似输入随 `crashes/` 备份到 S3;运维在 CI 上以默认限额复现(`cargo xtask fuzz <target> --repro <input>`;工作流归 [development](../docs/development.md)),再在服务器上执行 `crash resolve`:`--reproduced` → 该条目转为失败(清零时刻的规则归 testing §6.1),`--not-reproduced` → 标 `dismissed`。每次 resolve 写一条不可变的 `resolutions/<crash-id>.json`(含证据 URL 与操作者),与运行记录一起参与折叠。`crash-id` = `<target>/<sha1>`(失败的 DST 条目为 `dst/<seed>`)。
- **`ledger show`**:按完成时刻折叠本机 `runs/`、`resolutions/` 与 `--runs-from` 给出的记录(运维把 S3 `hft/verify/` 同步到本地后传入),输出每个 core 目标的累计 CPU 秒、DST 累计新种子数、贡献新种子的运行数与不同 UTC 日数、最近一次清零的时刻与原因、未决的疑似条目、按 `path_digest` 与按验证包版本的分项,以及按最近 7 次运行的吞吐外推的预计达成日期(吞吐 = 这 7 次运行贡献的累计量之和 ÷ 其中最早开始到最晚结束的墙钟时长;预计时刻 = 最晚结束时刻 + 距门槛的剩余量 ÷ 吞吐;已达门槛为 null,不足 7 次时用全部运行)。`--candidate <ver>`:从该版本验证包的 `VERIFY.json`(`--candidate-bundle`,缺省 `/opt/hft/verify/<ver>/`)取各目标组的 `path_digest`,**只累计段记录的 `path_digest` 与之相同的运行**(不论运行它们的是哪个版本),并单列该版本自身的运行(候选版本补跑)。门槛取值不在本工具硬编码:读验证包 `VERIFY.json` 的 `gates` 段(由 `tools/hft-verify/gates.toml` 写入,`doc-check` 核对与 testing §8 一致)。`path_digest` 的计算见 [`cargo xtask path-digest`](#其他-xtask-子命令)。
- **`ledger rebuild`**:同样折叠全部可得的记录,重写 `ledger/ledger.json` 并标 `rebuilt_from`(参与折叠的主机、记录数、最早与最晚运行)、写一条告警到 stderr 与 journald;规则按 testing §6.1。
- **`crash list`**:列出 `crashes/` 下的条目(`crash-id`、状态 `failure`/`suspect`/`dismissed`、首次出现的 UTC、`run_id`、验证包版本、输入 SHA-1、最小化后的大小、复现命令:`cargo xtask fuzz <target> --repro <input>` 或 `cargo xtask dst --seeds 1 --seed <S> --suite <X> --steps <M>`);失败输入由人带回仓库写进 `fixtures/<venue>/regressions/` 或 `fixtures/dst-seeds/`。
- **不联网**:本工具没有任何网络代码(单元另设 `PrivateNetwork=yes`);S3 备份由 journal-archiver 的 `kind = files` source 上传 `ledger/`、`runs/`、`resolutions/`、`crashes/` 到前缀 `hft/verify/`(对象键带主机段;键模板与排除规则归 [journal-archiver](../apps/journal-archiver/README.md)),`corpus/` 不上传(可再生),`crashes/dst/.inflight.json` 不上传(每个种子改写一次)。
- 退出码:0 完成(含被中止、只有疑似);1 发现失败;2 环境或输入错误(验证包哈希或 `path_digest` 不符、状态目录不可写、`--budget-min` < 5)。

**状态目录** `/var/lib/hft/verify/`:

```
ledger/ledger.json                       本机账本缓存(可由 runs 与 resolutions 重建;门槛只认折叠全部主机记录的结果)
runs/<run_id>/<k>.json                   段记录,不可变;run_id = <host>-<开始 UTC,yyyymmddThhmmssZ>
runs/<run_id>/RUN.json                   运行结束时写:段数、结束 UTC、interrupted、suspects、skipped(无则 null)、退出码
resolutions/<crash-id>.json              疑似条目的复核结论,不可变(crash-id 中的 / 写成 _)
corpus/<target>/                         libFuzzer 工作语料
crashes/<target>/<sha1>[.min]、crashes/<target>/suspect/<sha1>[.min]
crashes/dst/<seed>.json、crashes/dst/.inflight.json
```

**段记录**(JSON,键排序;`format = 1`):`run_id`、`k`、`host`、`bundle_version`、`bundle_sha256`、`path_digest`(该段所属目标组在本验证包里的值)、`start_utc`、`end_utc`、`cpu_seconds`、`wall_seconds`、`kind = "fuzz" | "dst"`、`exit`(`ok`、`signal:<名>`、`killed`、`interrupted`)、`skipped`(`"insufficient_memory"` 或 null;非 null 时没有子进程、其余计量为 0);fuzz 段另有 `target`、`execs`、`crash_sha1[]`、`suspect_sha1[]`;DST 段另有 `seed_from`、`seed_to`(含)、`seeds`、`steps`、`violations`、`failed_seeds[]`、`rerun_seed`(被 SIGKILL 打断、待重跑的种子;无则 null)。

**`VERIFY.json`**(验证包根目录;JSON,键排序;`format = 1`):`version`、`git_sha`、`nightly_date`(fuzz 工具链)、`targets[]`(每项 `{name, core, binary, corpus}`;`core = true` 才计入门槛 ③)、`path_digest: {<目标组>: <hex>}`(目标组 = 每个 core 解码器目标名与 `dst`)、`gates`(由 `tools/hft-verify/gates.toml` 写入)、`files: {<相对路径>: <sha256>}`(包内全部文件)。`run` 启动时逐项核对。

**账本**(`ledger/ledger.json`,`format = 1`):`fuzz.<target> = {cpu_seconds, since_utc, last_reset: {utc, run_id, reason}, by_digest: {<path_digest>: cpu_seconds}, by_version: {<ver>: cpu_seconds}}`;`dst = {seeds, runs, utc_days, ranges: [[from, to]…], since_utc, last_reset, by_digest: {<path_digest>: {seeds, runs, utc_days}}, by_version: {<ver>: seeds}}`;`suspects[]`(未决的疑似条目)、`last_run_id`、`hosts[]`、`rebuilt_from`(无则 null)、`eta: {fuzz, dst}`。

**折叠的取值口径**(规则本身归 testing §6.1,这里只定字段怎样算):运行按完成时刻排序,完成时刻 = `RUN.json.end_utc`,没有 `RUN.json`(被强杀)的运行取其最后一段的 `end_utc`;运行的 UTC 日取同一时刻。疑似段的 CPU 秒照常计入。清零把该目标(或 DST)的计数量(`cpu_seconds` 或 `seeds`、`runs`、`utc_days`、`by_version`)归零并更新 `since_utc` 与 `last_reset`;DST 的 `ranges` 保留全部历史区间,作为规则 5 去重的依据。`by_digest.<d>` 只折叠 `path_digest = d` 的段,也只被这些段中的失败清零;`--candidate` 的输出就是候选版本各目标组 digest 对应的这一项。

## 实现说明

<details><summary>实现要点</summary>

| 路径 | 职责 |
|---|---|
| `tools/xtask/` | `xtask` 二进制 crate(`.cargo/config.toml` 的 `alias.xtask = "run -p xtask --"`) |
| `tools/xtask/deps.toml` | 依赖方向的机器可读表(含策略解耦规则与 `decoupling-gate` 的分层) |
| `tools/xtask/doc-check.toml` | 策略族名、已删除组件、历史叙述关键词、残留与排期措辞名单(`[[rule]]`),`scope`/`status` 合法值,组件 README 的路径范围,Agent Note 类别,不扫描与只查链接的目录,外部脚本清单 |
| `crates/<x>/benches/budgets.toml` | hot crate 的基准预算(源常量;格式见 [doc-check](#cargo-xtask-doc-check) 的"预算"口径) |
| `tools/xtask/allow-list.toml` | 允许的 `#[allow]` 登记(文件、lint、理由) |
| `tools/xtask/toolchains.toml` | nightly 日期、cargo 插件版本 |
| `tools/xtask/fuzz-targets.toml` | 全部 fuzz 目标、是否 core 解码器目标、CI 每夜冒烟的每目标分钟数;每个 core 目标与 `[dst]` 参与 `path_digest` 的源路径 `paths` |
| `tools/xtask/test-trace.toml` | `test-trace` 的 `current_milestone` 与 `enabled_extensions`(已启用的可选扩展名;语义归 [docs/tests §6](../docs/tests/README.md#6-与代码的对应)) |
| `tools/xtask/dst-nightly.toml` | CI 每夜 DST 冒烟的种子数 N 与步数 M(每夜作业据此调用 `dst --seeds N --seed S --steps M`) |
| `tools/xtask/src/{ci,decoupling_gate,scaffold_gate,deps,hot_check,bench_gate,replay_verify,dst,gen_schema,gen_host,doc_check,new_strategy,fixtures,fuzz,journal_verify,path_digest,setup,test_trace}.rs` | 各子命令 |
| `tools/xtask/src/hot_check/scan.rs` | hot-check 的 `syn` 扫描(带行号;`#[cfg(test)]` 标记) |
| `tools/xtask/src/exec.rs` | 外部命令执行器(`cargo`、`git`、`gh`、`rustup`、python 与 xtask 自身的子命令都经它调用);环境变量 `XTASK_EXEC_SCRIPT=<json>` 换成记录型执行器(按命令行前缀返回脚本里的退出码与输出,不启动进程),`XTASK_EXEC_LOG=<文件>` 逐行记下每次调用的程序与参数(测试用,tools/TESTS.md 用例约定) |
| `tools/xtask/src/workspace.rs` | workspace 成员、包名与 README frontmatter 的读取 |
| `tools/xtask/scope.toml` | `ci` 的 optional 组件清单(`optional = [<目录>…]`,与 frontmatter `scope: optional` 一致) |
| `deny.toml` | `cargo deny` 配置:许可证白名单(私有 crate 不查)、`bans`(N4 的 `rust_decimal`、`bigdecimal`、`num-bigint`) |
| `tools/xtask/src/doc_check/{config,md,native,layout,budget}.rs` | doc-check 的配置、Markdown 解析与各项原生检查 |
| `tools/lint/clippy-hot.toml`、`clippy-pure.toml`、`attrs-{hot,pure,unsafe}.rs` | lint 模板 |
| `tools/az-probe/` | Python 3.12 包(由 `probe2.py` 演化);`targets.toml` |
| `tools/hostprobe/` | Rust 二进制(cold tier;只读 `/proc`、`/sys`、ioctl、`bpf(BPF_PROG_QUERY)` 与 nftables netlink 查询;不链接任何 hot crate) |
| `tools/hft-cfg/` | Rust 二进制(cold tier;依赖 `hft-refdata`、`hft-event`、`hft-strategy-sdk`(只用 `params`)、`hft-risk` 的构建器与校验、core `venue-*`(只用 `default_caps`/`dead_man`)、`hft-ctl-proto` 的帧;不依赖任何 `strategies/*` 与 hot 运行时 crate) |
| `tools/hft-cfg/src/{lib,refdata,strategy,engine,diff,util}.rs` | CLI 与各子命令;`strategy.rs` 只做 TOML ↔ schema 字段映射与单位换算,编码调用 SDK;`risk`、`fetch`、`send` 随 hft-risk 构建器、`refdata fetch` 与 hft-ctl-proto 帧加入 |
| `tools/hft-secrets/` | Rust 二进制(cold tier;`aws-sdk-secretsmanager`、`aws-sdk-kms`、tokio;不链接任何 hot crate);清单模板在 `deploy/secrets/` |
| `tools/hft-verify/` | Rust 二进制(cold tier;不链接任何 hft crate、无网络代码);`src/{run,rotate,classify,record,ledger,crash,resolve}.rs`;`gates.toml`(门槛取值,与 testing §8 比对) |

- `hot-check` 的 T6 扫描(rustls 记录加密入口的调用点)以库函数 `xtask::hot_check::scan_tls_encrypt(root: &Path) -> Vec<Violation>` 导出,组件测试(如 hft-net)以同一实现扫描自己的源码树。
- xtask 是 cold tier:可用 `serde_json`、`syn`、`walkdir`、`tokio`(仅 fixtures fetch)。`gen-schema` 调用已构建的 `hft-core` 二进制导出策略 schema,xtask 自身不链接策略 crate。
- `doc-check` 的锚点生成按 GitHub 规则:小写、去掉标点(含全角标点)、空格变 `-`,保留中日韩字符。
- hostprobe 的 `ethtool` 信息经 `SIOCETHTOOL` ioctl 读取,不启动外部进程;chrony 信息需调用 `chronyc`(启动期,允许)。
- hft-cfg 的十进制 → 定点换算用 hft-types 的 `parse_exact`(单位换算按 schema 与 refdata 资产指数),不经 f64。
- hft-verify 的账本折叠是纯函数(输入:运行记录集合;输出:账本),`run` 结束时与 `ledger rebuild` 调用同一实现;子进程管理只用 `fork`/`exec`/`waitpid` 与信号,不用异步运行时。

</details>

## 延迟与资源契约

无热路径约束。`cargo xtask ci` 在 CI 上的目标时长归 [testing §6](../docs/testing.md#6-ci-运行矩阵) 的运行矩阵;`decoupling-gate` 不构建,≤ 1 min;hostprobe `collect` ≤ 2 s(在 hft-core 启动前运行,计入冷启动 < 30 s 预算,[00 §1](../design/00-目标与延迟预算.md));az-probe 周复测对生产主机的影响限定在冷核、≤ 1 核、≤ 10 min;`hft-cfg build --kind strategy`(500 group)≤ 1 s。hft-verify 在根级 `hftbatch.slice`(`CPUWeight=idle`)内运行(slice、`Nice` 与 I/O 带宽上限的单元设置归 deploy)、同一时刻只有一个子进程;内存上限取 [hft-core 内存表](../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)的 `hft-verify` 行,单元设置归 deploy;驱动自身 < 32 MiB。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- xtask:`cargo_metadata`、`syn`(`full`、`visit`)与 `proc-macro2`(`span-locations`,hot-check 的行号)、`serde`、`toml`、`serde_json`、`walkdir`、`regex`、`pulldown-cmark`、`sha2`(fixtures 与回放指针的哈希);内部链接 `hft-event` 与 `hft-refdata`(`gen-schema` 读它们的 schema 导出)、`hft-journal`(`journal-verify`,不启用 `writer`;其测试以 dev 依赖启用 `writer` 与 `mem-backend` 写夹具 journal),`deps.toml` 另允许 `hft-testkit`(`dst` 运行器,[hft-testkit](../crates/hft-testkit/README.md#依赖));调用 `cargo`、`cargo-deny`、`cargo-fuzz`、`bpftrace`、`gh`(只在 `decoupling-gate`)、已构建的 `hft-core`(外部二进制)。
- hostprobe:`libc`、`serde_json`、`toml`、`sha2`;禁止依赖任何 hot crate(它读主机,不读交易)。`unsafe` 只在 `src/sys.rs`(ioctl、`bpf`、netlink、`getifaddrs`),每处带 `// SAFETY:`。
- az-probe:Python 标准库 + `websockets`;不依赖 hft crate。
- hft-cfg:见实现说明表;外部 `toml`、`serde_json`(读 schema JSON)、`reqwest`(仅 `refdata fetch`)。禁止依赖 `strategies/*` 与 `hft-strategy-registry`。
- hft-secrets:`aws-sdk-secretsmanager`、`aws-sdk-kms`、`tokio`、`toml`;不被任何 crate 依赖(只以二进制在 `ExecStartPre` 调用)。
- hft-verify:`serde`、`serde_json`、`toml`、`sha2`、`nix`(信号与进程);禁止任何 hft crate、网络库与异步运行时。

## 延伸阅读

- [development.md](../docs/development.md)、[testing.md](../docs/testing.md)、[hot-path-rules.md](../docs/hot-path-rules.md)、[benchmarks/](../benchmarks/README.md)、[fixtures/](../fixtures/README.md)、[strategies/AGENTS.md](../strategies/AGENTS.md)、[cookbook/adding-a-strategy](../docs/cookbook/adding-a-strategy.md)
- [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md)(`params` 编解码与 schema JSON)、[hft-refdata](../crates/hft-refdata/README.md)(账户、实例、能力表)、[control-plane](../docs/subsystems/control-plane.md)、[deploy](../deploy/README.md)(验证包、主机生成物)
- [architecture-v2/02 §4–§6](../design/02-硬件与托管方案.md)(调优清单与验收指标)、[03 §7](../design/03-网络与传输层.md)、[10 §6](../design/10-语言分工与代码组织.md)(调优即代码)
- 决策出处:[round8-decisions](../design/_work/round8-decisions.md)(参数 schema 驱动与机检解耦;累计门槛在服务器上运行);决策记录 [verify-on-server](../.agents/notes/proposed/process/2026-10-03-verify-on-server.md)、[ci-org-and-profiles](../.agents/notes/proposed/architecture/2026-10-03-ci-org-and-profiles.md);`architecture-v2/research/_src/probe2.py`、[research/05](../design/research/05-hardware-hosting-os-tuning.md)

## 已知限制与后续工作

- hostprobe 只读当前状态,不修改;调优的施加在 [deploy/](../deploy/README.md) 的 oneshot 单元里。
- hostprobe 的 `osnoise` 只包装 `rtla osnoise hist`(`timerlat` 尚未包装);nftables 只列出已加载的表(dev2 的"表含两条规则"在 M3 随 dev2 剖面补上);ntuple 规则数在 ENA 上报 `unsupported`。
- bench-gate 的 Criterion 结果按样本均值求分位(不是逐次迭代),逐次分位要等 HistBench(hft-testkit)落地;发布工作流的基准制品目前只打包 `hft-benchmarks` 的基准,单 crate 基准的参考机运行先用 `cargo bench --profile bench`(参考机上编译)或在制品中补打包。
- az-probe 的首帧单向分布依赖 PHC;探针实例若无 PHC,只能给 RTT 结论。
- `doc-check` 的分段名与策略族名检查只能匹配名单内的写法,自然语言里的错误用法靠评审。
- hft-cfg 不跑策略的跨字段 `validate`(它不链接策略代码);离线发现跨字段错误要靠 `send` 的预览或 CI 里对 `fixtures/params/*` 的 gateway 级测试。
- xtask 尚未实现(随各自的里程碑加入):`fuzz`、`path-digest`(未知子命令,退出 2);`strategies/_template/` 尚未建立,`new-strategy` 在真实仓库中退出 2、`scaffold-gate` 跳过;`dst` 的 `--strategy` 退出 2(registry 尚无策略 DST 入口);`--hashtrail check` 只比较每个种子的 `HashTrail` 摘要,不报告分歧的 `seq` 与组件;`fixtures extract/corpus/bbo-diff` 退出 2;`replay-verify` 经 locked ci 剖面执行本地 `replayer verify`,指针仍先校验缓存 SHA-256;应用退出 1/3/4 映射为检查失败 1,应用参数/环境错误映射为 2。Model 重算、恢复区间、压缩输入、分歧前片段导出与 `--record-decide-hist` 尚未接入,不支持的选项退出 2;`gen-host` 只做输入核对(`--host` 的清单存在),`deploy/gen/` 与 hft-core 主机类档不在仓库时退出 2。
- hot-check 尚未覆盖 [hot-path-rules](../docs/hot-path-rules.md) 中写给 hot-check 而本节检查表未列的项:T5 warm 开关默认值、T7 rustls 握手/`connect` 与 REST 快照调用点只在 `conn-builder`/`snap-fetch` 模块、M6 含枚举字段的事件体 derive `FromBytes`(相关 crate 进入 workspace 时随表加入)。
- hft-cfg 尚未接入:`build --kind risk`(hft-risk 构建器)、`refdata fetch`、`send`(hft-ctl-proto 帧)、从 journal 目录读取生效 blob(`--against`/`--refdata` 给目录)与 core `venue-*` 的 `default_caps()`(能力行的底取 `[caps.<venue>]`);前四项当前退出 2。
- hft-verify 的吞吐受服务器空闲 CPU 限制(批处理所在 slice 为 `CPUWeight=idle`,dev2 上的吞吐随旧系统负载变化;达成时间估算见 testing §6.1);它不并行,也不在主机间协调——同时有两台主机运行时靠种子编码与按完成时刻折叠保证结果正确,不靠锁。

### 开发备忘

<details><summary>给实现者</summary>

- 待核实:clippy 从 crate 目录向上查找 `clippy.toml`(依赖此行为实现按 tier 的配置)——方法:在两个 crate 放不同的 `disallowed-methods`,确认各自生效。
- 待核实:`/sys/fs/resctrl` 在 Graviton(c8g)上是否可用(Arm MPAM 支持情况)([12 A12](../design/12-风险与未决问题.md))——方法:hostprobe 在部署镜像上首次运行即给出答案。
- 待核实:ENA PHC 在 c8g 上的设备名与 `ethtool -T` 报告的能力([12 A7](../design/12-风险与未决问题.md);方法:M1.5 延迟探针时在租用实例上读取)。
- 待核实:core 交易所 instruments 接口的字段与分页(`refdata fetch`;本机连不上官方文档站,方法:在服务器上实拉并存为 fixtures)。
- 待核实:`ubuntu-24.04-arm` 上以 ASan 构建的 libFuzzer 二进制在服务器(Ubuntu 24.04、glibc 相同)上直接运行,以及 `quarantine_size_mb=16` 下 `-rss_limit_mb`/`-malloc_limit_mb` 与 cgroup `MemoryMax` 的配合(不出现假 OOM)——方法:验证包工作流的冒烟步骤 + dev2 上首次 `hft-verify run --budget-min 5`,读单元的 `MemoryPeak`。
- 待核实:`pull_request_review` 触发的运行与 PR 头 SHA 的 check run 关联方式(影响 `decoupling-gate` 结论能否被发布 `preflight` 找到)——方法:M1 第一周在 `Fiora-gwx/hft` 实测。
- 实现顺序:`deps` → `hot-check` → `doc-check` → `ci` 骨架(建仓时让空壳 CI 变绿)→ `decoupling-gate` → `gen-schema` → hft-cfg(refdata → engine → risk → strategy)→ `new-strategy` → `scaffold-gate` → `bench-gate` → `replay-verify` → `dst` → `journal-verify` → `gen-host` → hft-verify;hostprobe 与 az-probe 先于 xtask 完成(选 AZ 与做主机基线需要它们)。

</details>
