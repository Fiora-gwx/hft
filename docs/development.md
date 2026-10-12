# 开发

本页是代码仓库、开发环境、日常流程、CI、构建剖面与 CI 资源规则的唯一归属;制品格式与服务器安装在 [deploy/](../deploy/README.md)。常驻规则在根 [AGENTS.md](../AGENTS.md);测试层次、累计账本规则与上线门槛在 [testing.md](testing.md);编码规则在 [hot-path-rules.md](hot-path-rules.md);xtask 各子命令与 `hft-verify` 的规格在 [tools/](../tools/README.md)。发布纪律与 CI 形态沿用现有 Trading 仓库(Trading/AGENTS.md(本仓库之外:`Trading/AGENTS.md`)、Trading `.github/workflows/`),本页只写 hft 的差异。

## 仓库与 CI

- **仓库**:本机目录 `E:\hft`,远端为负责人个人账户下的公开仓库 [Fiora-gwx/hft](https://github.com/Fiora-gwx/hft)。代码与设计证据 `design/` 一同维护;工作区独立于 Trading 仓库。
- **角色**:负责人(GitHub 登录名 `Fiora-gwx`)= 仓库所有者与唯一 Admin(`.github/hft-admins.txt`);协作者有写权限,公众可读。发布覆盖(`skip_nightly`、`skip_verify`)与解耦豁免只认 Admin 名单;生产安全依赖发布 `preflight` 与服务器只安装通过检查的制品。
- **受保护分支**:当前未配置;CI 红时不合并。发布工作流的 `preflight` 证实该 SHA 的 `ci.yml` 全绿且 `decoupling-gate` 通过,正式版另要求该 SHA 在 `main` 上([发布工作流](#发布工作流release-aarch64yml));服务器只安装经过 `preflight` 的制品。
- **CI 是唯一的构建与测试环境,只用 GitHub 托管的免费 runner**:PR 与 `main` 推送的质量门在 `ubuntu-latest`(x86_64)上跑——引擎的纯逻辑 crate 跨平台,ISA 专属代码另做 aarch64 交叉检查;aarch64 发布制品与验证包在 `ubuntu-24.04-arm` 上原生构建。**不用自托管 runner**;`dev2` 与任何交易主机都不注册为 runner,不在其上编译。CI 结论是"能编译、测试通过"的唯一证据。公开仓库使用标准托管 runner 的分钟免费;资源边界见[免费额度](#免费额度)。
- **长时间 fuzz 与 DST 在服务器上跑 CI 构建的验证包**:CI 构建 `hft-verify-<ver>-aarch64.tar.zst`([验证包工作流](#验证包工作流verify-bundleyml));服务器只安装并在批处理 slice 内以最低优先级运行它(`hft-verify.service`,[deploy](../deploy/README.md#验证包与-hft-verify)),不编译。上线门槛 ③④ 的累计量只来自这些运行(账本规则归 [testing §6.1](testing.md#61-服务器上的累计运行));CI 每夜只做回归冒烟,不计入。
- **本地只写代码**:Windows 工作站上编辑、提交、推送;不要求本地能编译或运行,也不用 WSL2。本地装了 Rust 工具链时可以 `cargo check` 或跑 `pure` crate 的测试作参考,结论以 CI 为准。
- **服务器只安装 CI 产出的制品**:正式版、开发构建与验证包(见 [deploy](../deploy/README.md#制品从哪里来)),不在服务器上编译、不装工具链。CI 不接触服务器:没有 SSH 部署作业,服务器自己拉取制品(server-pull)。
- **交易所联调只在服务器上**:本机连不上 Binance、OKX、Bitget 及其文档站;需要真实报文时在 `dev2` 上用开发构建运行(`sim` 实例的实时行情与模拟账户;联调窗口里只持测试网 key 的 `prod` 角色实例),或从 S3 中的 raw journal / collector WAL 抽取金样([fixtures/](../fixtures/README.md))。`dev2` 同时运行旧系统的正式账户,新框架只在 dev2 护栏内运行,且**不放正式实盘账户**([deploy](../deploy/README.md#dev2-护栏)、[launch](runbooks/launch.md))。

## 工具链

| 项 | 值 | 位置 | 说明 |
|---|---|---|---|
| stable | `1.97.1`(与 Trading 仓库一致,建仓时确认) | `rust-toolchain.toml` 的 `channel` | `profile = "minimal"`,`components = ["clippy", "rustfmt", "llvm-tools"]` |
| 目标三元组 | 发布:`aarch64-unknown-linux-gnu`;CI 质量门与测试:`x86_64-unknown-linux-gnu`(也是可选 `metal` 剖面的目标) | `rust-toolchain.toml` 的 `targets` | 见[目标 CPU](#目标-cpu) |
| nightly(仅 fuzz) | 固定日期 `nightly-YYYY-MM-DD`,建仓时选定后写进 `tools/xtask/toolchains.toml` | `cargo xtask fuzz` 包装;验证包的 fuzz 目标 | 只用于 fuzz,不用于发布二进制 |
| cargo 插件 | `cargo-deny`、`cargo-fuzz`、`cargo-nextest`(可选)按版本固定 | `tools/xtask/toolchains.toml` | CI 按表安装并缓存 |
| Python | 3.12(`hft_py` wheel 规格见 [hft-py](../crates/hft-py/README.md)) | `python/.python-version` | 生产每个版本一个 venv,冷进程经 `/opt/hft/current-cold/venv` 使用([deploy 单元组](../deploy/README.md#单元组与分组激活)、[python/AGENTS.md](../python/AGENTS.md)) |
| Foundry | 固定版本 | `contracts/foundry.toml` | 只用于 optional 的 `contracts/` |

升级工具链是一次独立改动:质量门 `rust` 作业在同一作业内给出 base 与 head 的相对基准(只报告),并须在参考机重跑 `bench-gate --reference` 并附直方图(规则 [B4](hot-path-rules.md#构建b),参考机见[参考机](#参考机))。

## 目标 CPU

本节是构建目标的**唯一归属**;其他文档(hot-path-rules B2、benchmarks、deploy、hft-rt 主机检查)链接这里,不复写。

| 剖面 | 发布目标三元组 | `target-cpu` | 运行在 | 说明 |
|---|---|---|---|---|
| 基准形态(core) | `aarch64-unknown-linux-gnu` | `neoverse-v1` | `dev2`(c7g,Graviton3)与 `c8`/`c16`/`c32`(c8g,Graviton4)——**同一个二进制** | Neoverse-V1 是两代 Graviton 的公共基线;与现有 Trading 仓库同为 ARM64 |
| 可选扩展:x86 / `metal` | `x86_64-unknown-linux-gnu` | 按 bake-off 选定的实例族 | `metal` 主机类 | 独立剖面、独立构建、独立基线;触发条件:实测证明 Graviton 上的 p99 不够,或需要裸金属 + Onload |

- `target-cpu` 写在 `.cargo/config.toml` 的发布目标段与 `RELEASE.json`;hft-core 启动时检查 CPU 特性是发布构建要求的超集,否则拒绝启动(规则 [B2](hot-path-rules.md#构建b))。
- CI 质量门在 x86_64 runner 上以 `target-cpu` 缺省值(`x86-64`)构建与测试;ISA 专属代码(`hft-rt` 的计时器读取、环与 journal 的原子序)由 `cargo check --target aarch64-unknown-linux-gnu` 保持可编译,由每夜的 `nightly-arm` 作业与正式版的 aarch64 一致性集在 arm runner 上原生测试(见[每夜任务](#每夜任务nightlyyml)、[发布工作流](#发布工作流release-aarch64yml))。内存序的正确性不依赖硬件:环与 journal 的 loom 模型检查在 x86 上跑,它按 C11 内存模型穷举弱序交错。
- **换架构 = 版本变化**:按恢复规则只能从 Drain 快照冷启动([recovery-epoch-and-boot-rules](../.agents/notes/proposed/architecture/2026-10-01-recovery-epoch-and-boot-rules.md))。
- 依据:[build-target-aarch64](../.agents/notes/proposed/process/2026-10-01-build-target-aarch64.md)、[ci-org-and-profiles](../.agents/notes/proposed/architecture/2026-10-03-ci-org-and-profiles.md)。

## 构建剖面

本节是 Cargo 剖面的**唯一归属**;[hot-path-rules B1](hot-path-rules.md#构建b) 只链接这里。

| 剖面 | 用途 | 设置 |
|---|---|---|
| `dev` | 本地参考 | 默认 |
| `release` | 正式版与开发构建的全部二进制(下一行的两个除外);开发构建只用它 | `lto = "thin"`、`codegen-units = 16`、`panic = "abort"`、`overflow-checks = false`、`debug = "line-tables-only"`;aarch64 + `neoverse-v1` |
| `release-hot` | **只用于正式版的 `hft-core` 与 `kill-switch` 两个二进制** | `inherits = "release"`;`lto = "fat"`、`codegen-units = 1` |
| `ci` | CI 的单元/属性测试、DST、回放验证、PR 的基准冒烟;验证包里的 `hft-dst` | `inherits = "release"`;`lto = false`、`codegen-units = 16`、`incremental = false`、`debug-assertions = true`、`overflow-checks = true`、**`panic = "unwind"`**(测试框架要求 unwind,显式写出以免依赖按两种 panic 策略各编一遍;`release`/`release-hot` 仍为 `abort`);`opt-level = 1`、`debug = false`,其中优化级别只作用于 workspace 成员:`[profile.ci.package."*"] opt-level = 3` 让全部依赖(含构建脚本与过程宏)保持 3(`"*"` 不匹配 workspace 成员);`tools/` 下的成员(`xtask`、`hft-cfg`、`hostprobe`、`latency-slice`,新增工具 crate 同样加一节)另以 `[profile.ci.package.<名>] opt-level = 0` 降到 0 级(其测试受进程与 I/O 限制,0 级与 1 级运行时长相同);叶子 crate `hft-ctl`、`hft-ctl-proto`、`venue-binance`、`hft-strategy-registry` 同样为 0 级(0 级下自身测试合计约 11 s,workspace 里还没有属性测试或 DST 运行它们的代码;一旦有就移回 1 级;`hft-exec` 已因 DST 驱动执行层移回 1 级)。原因:M2 起 workspace 有约 500 个测试/示例二进制,继承 `release` 的 3 级优化与行表时 PR 的测试树构建约 15 min;1 级 + 无调试信息使 workspace 部分的编译约降 40%,测试运行时长不变(0 级编译再快一倍,但属性测试运行慢约 3 倍,合计不划算);panic 信息仍带文件与行号,回溯无行号;x86_64 runner 上用缺省 `target-cpu` |
| `bench` | 只用于参考机基准制品 `hft-bench-<ver>-aarch64.tar.zst`(正式版发布作业);PR 的基准冒烟不用它 | 与 `release-hot` 相同(`inherits = "release-hot"`) |
| `release-pgo` | 可选:PGO 产物(实测收益 ≥ 5% 时启用) | `release-hot` + 以验证回放训练的 profile |

- **验证包里的 `hft-dst`/`hft-verify`** 以 `ci` 剖面构建,但构建步设环境变量 `CARGO_PROFILE_CI_OPT_LEVEL=3`、`CARGO_PROFILE_CI_DEBUG=line-tables-only`(服务器上累计 DST 的吞吐随优化级别,崩溃回溯要行号);PR 的基准冒烟沿用 `ci` 的 1 级,数值只报告。
- **开发构建**(`vX.Y.Z-dev.N`)全部用 `release`,只构建 dev2 运行的二进制(发布归档 `bin/` 中的 core 二进制与 Python wheel;不含基准制品与 optional 组件),跳过 aarch64 一致性集。
- **fuzz 目标**不用上表的剖面:由 `cargo fuzz build`(固定 nightly、AddressSanitizer、开 debug-assertions)构建,只出现在验证包与 CI 的 fuzz 冒烟里。
- `ubuntu-24.04-arm` runner 的 CPU 是否满足 `neoverse-v1` 的特性集**待核实**(方法:发布作业打印 `/proc/cpuinfo` 的 `Features` 与 `rustc --print cfg -C target-cpu=neoverse-v1` 比对)。不满足时,发布作业里的冒烟测试与 schema 导出改用 `target-cpu=generic` 的同源构建运行,打包的仍是 `neoverse-v1` 构建,其 CPU 特性检查在服务器安装时由 `hft-core --check` 完成。

## 开发机

- **Windows 工作站**:只编辑与提交。行尾:仓库 `.gitattributes` 设 `* text=auto eol=lf`,`*.sh`、`*.service`、`*.toml`、`*.py` 强制 `eol=lf`;提交时归一化。
- Git Bash 与 AI 代理的 Bash 工具对单条命令长度有限制(约 8 KB 会被截断);大文件用编辑器或写文件工具写,不用 heredoc。
- 工作站**连不上**交易所与其文档站,**可以** SSH 到 `dev2` 与采集服务器(只读查看日志、journal 与状态;不在其上编译、不在其上跑测试套件)。
- 不设单独的 Linux 开发主机:Linux 上的一切(编译、shellcheck、`systemd-analyze verify`、容器内的 dev2 护栏测试)都在 CI 里跑。

## 日常流程

1. 从 `main` 开分支:`feat/*`、`fix/*`、`ops/*`、`perf/*`、`recovery/*`、`strat/*`(只改 `strategies/*` 与其参数)。
2. 写代码前读目标目录的 README、`TESTS.md` 与它们链接的子系统页;组件以"`TESTS.md` 中里程碑已到的用例全部实现并通过"为完成。实现与规格不一致时按 [docs/tests §7](tests/README.md#7-修改流程设计基线-v1-之后):同一提交改 README、`TESTS.md` 与代码,并在 [changes.md](changes.md) 记一行(推翻原决定另写 Agent Note)。
3. 提交:Conventional Commits 主题(`feat(hft-oms): …`、`feat(portfolio-carry): …`);生成物(`gen-schema` 输出、策略 schema 金样 `fixtures/strategy-schemas/`、deploy 生成物)与生成器改动同一提交。
4. 推送分支 → PR → CI 全绿 → 合并(`--no-ff`;单人维护也先推分支让 CI 验证确切的提交)。CI 红时在本地改代码再推,不在服务器上排查编译问题。没有受保护分支,"红了不合并"是人的纪律,机器只在发布 `preflight` 处把关。
5. **开发构建**(需要在 `dev2` 联调时):对一个 `ci.yml` 已全绿的 PR 头提交,用 `release-aarch64.yml` 的 `workflow_dispatch`(输入 `sha`、`dev_n`)出 `vX.Y.Z-dev.N`,不必先合入 `main`;或在已通过 CI 的 `main` 提交上直接打该标签。再在服务器上按 [release-and-deploy](runbooks/release-and-deploy.md) 拉取并冷安装。合并本身不自动产出制品。
6. `main` 每夜只跑回归冒烟与 aarch64 专项([每夜任务](#每夜任务nightlyyml));前一日生产 journal 的回放验证与累计 fuzz/DST 在服务器上运行已安装的制品(`hft-replay-verify`、`hft-verify`,[deploy](../deploy/README.md#systemd-单元)),不占 CI 分钟。红了的 `main` 不能打正式版标签。

**策略改动**:只改 `strategies/<name>/`(含它的 pipeline 插件包 `strategies/<name>/pipeline/`)与注册表 `strategies/registry` 的一行;`decoupling-gate` 拒绝同一 PR 中对引擎 crate 的改动(需要改引擎时拆成单独的 PR 并说明理由)。参数改动不走本流程(控制命令,见 [pipeline](../python/pipeline/README.md#参数通道))。

不进 git:凭据、生产 journal、Parquet、捕获数据、验收 JSON、本地配置。大样本用 S3 指针([fixtures/](../fixtures/README.md))。

## xtask 命令

全部在 CI 中运行;本地有工具链时也可运行作参考。参数、输出与退出码见 [tools/README.md](../tools/README.md)。

| 命令 | 做什么 | CI 中何时 |
|---|---|---|
| `cargo xtask ci` | fmt、clippy(`-D warnings`)、`cargo deny check licenses bans`、`deps`、`hot-check`、单元/属性(含 loom)、金样、DST 冒烟、`doc-check` | 每次推送(质量门 `rust` 作业) |
| `cargo xtask deps` | 依赖方向与闭包检查(含"引擎 crate 不依赖 `strategies/*`、策略 crate 只依赖 SDK 与 `hft-strat-lib`") | `ci` 内 |
| `cargo xtask decoupling-gate` | PR 同时改 `strategies/*` 与引擎 crate/app 即失败;豁免核对标签与 Admin 批准 | 质量门的 `decoupling-gate` 作业与 `decoupling.yml` |
| `cargo xtask scaffold-gate` | 用模板生成临时测试策略、注册后构建并跑场景测试,断言引擎 crate、app 与 `python/` 的 diff 为零("新增策略零改动") | 按路径(改动 `crates/`、`apps/`、`strategies/`、`tools/`);`CARGO_TARGET_DIR` 指向本作业的主 target 目录,不另起一份全量编译 |
| `cargo xtask hot-check` | 热路径源码检查(`dyn`、浮点、`unsafe`、调用点白名单、lint 属性、profile) | `ci` 内 |
| `cargo xtask bench-gate [--crate <x>] [--path <p>] [--smoke \| --reference] [--syscall-audit]` | 基准冒烟(`--smoke`:编译与短跑,数值只报告)与参考机门槛(`--reference`);签名归 [tools](../tools/README.md#cargo-xtask-bench-gate) | 质量门按路径跑 `--smoke`,以 `ci` 剖面构建;参考机上用正式版附带的基准制品(`bench` 剖面)跑 `--reference`,M7 起阻断发布([参考机](#参考机)、上线门槛 ⑩) |
| `cargo xtask replay-verify <journal>` | 验证回放逐字节比对 | 策略/风控/OMS/执行层改动的 PR 跑 `fixtures/replay/` 样本;生产 journal 的每日 verify 由服务器上的 `replayer`(发布制品)运行,不在 CI |
| `cargo xtask dst …`(签名归 [tools](../tools/README.md#cargo-xtask-dst)) | 确定性模拟 | 质量门跑冒烟集并核对 `HashTrail` 金样(一致性集登记顺序的前 50 个种子);每夜与正式版标签核对一致性集全部种子(每夜另跑回归冒烟批量);复现单个种子 |
| `cargo xtask gen-schema [--check]` | 从 `hft-event` 生成 Arrow schema、Python dtype、事件目录文档;`--check` 只比对 | `ci` 用 `--check` |
| `cargo xtask doc-check` | 链接、README 章节结构、frontmatter、引擎 README 不出现策略族名;以 `scope` 生成并比对根 README 组件表 | `ci` 内;`docs` 作业 |
| `cargo xtask test-trace` | 测试规格(`TESTS.md`、`docs/tests/*.md`)与测试函数、场景、基准的追溯;规则归 [docs/tests §6](tests/README.md#6-与代码的对应) | `docs` 作业(每个 PR 与 `main` 推送) |
| `hft-cfg …` | 手写 TOML → refdata / `StrategyParams` / `RiskParams` / `EngineConfig` blob + 差异报告;读 `hft-core schemas` 导出的 schema JSON,编解码链接 hft-strategy-sdk 的 `params` | 见 [tools](../tools/README.md#hft-cfg) |
| `hft-core schemas --out <dir>` | 导出各策略的参数 schema JSON:`<kind_id>-<name>.schema.json` + `index.json`(规格归 [strategies/registry](../strategies/registry/README.md)) | 单元测试与 `fixtures/strategy-schemas/` 比对;发布作业导出进归档 `schemas/strategies/` |
| `cargo xtask gen-host [--check]` | 主机清单 + hft-core 线程计划 → deploy 生成物([deploy](../deploy/README.md#主机清单)) | 改动 `deploy/` 或 `apps/hft-core/` 的非 Markdown 文件时 `rust` 作业用 `--check` 核对生成物无差异;删除与改名也触发,不要求文件在当前检出中存在;同一判定供 deploy 容器测试使用 |
| `cargo xtask fixtures {fetch,extract,scrub,check}` | 金样下载校验、从 journal 抽金样、脱敏、敏感信息扫描 | `check` 在 `ci` 内;`fetch` 经 OIDC 读 S3([凭证](#凭证与权限)) |
| `cargo xtask fuzz <target> [--time T]` | 以固定 nightly 运行 `cargo fuzz` | 改了解码器的 PR 跑冒烟;每夜短跑轮转;验证包工作流构建全部 core 目标 |
| `cargo xtask new-strategy <name> --kind-id <n> [--with-model] [--with-pipeline]`(签名归 [tools](../tools/README.md#cargo-xtask-new-strategy)) | 新策略脚手架;`--with-pipeline` 同时生成 `strategies/<name>/pipeline/` 插件包 | 本地或 CI 均可;产物经 PR 合入 |

③④ 的累计账本不由 xtask 维护:服务器上的 `hft-verify ledger show|rebuild` 读写它([tools](../tools/README.md#hft-verify),规则归 [testing §6.1](testing.md#61-服务器上的累计运行))。

## CI 概要

**什么时候跑什么**(PR、每夜回归冒烟与 aarch64 专项、正式版的一致性集、服务器上的每日回放验证与累计 fuzz/DST、时长上限)的唯一定义是 [testing §6 CI 运行矩阵](testing.md#6-ci-运行矩阵);本节只定义工作流文件、触发、runner、权限、作业图与发布步骤。形状照搬 Trading 仓库的 `ci.yml` → `quality-gates.yml`(`workflow_call`)→ `release-arm64.yml`;hft 另有 `decoupling.yml`、`nightly.yml`、`verify-bundle.yml`、复现疑似失败的 `fuzz-repro.yml` 与 arm runner 不可用时用的 `release-x86-cross.yml`。

| 工作流 | 触发 | runner | 作业 |
|---|---|---|---|
| `ci.yml` | `pull_request`(`opened`、`synchronize`、`reopened`)与 `main` 推送;`concurrency: ci-${{ github.ref }}`,`cancel-in-progress: ${{ github.event_name == 'pull_request' }}`(只取消同一 PR 的旧运行;`main` 推送不取消,每个合并提交都有自己的结论) | — | 只调用 `quality-gates.yml`;按改动路径运行质量门,不设月度分钟门槛 |
| `quality-gates.yml` | `workflow_call` | `ubuntu-latest`;各作业的 `timeout-minutes` 取 [testing §6](testing.md#6-ci-运行矩阵) | `rust`、`ops-py`、`decoupling-gate`、`docs`,见[质量门](#质量门quality-gatesyml) |
| `decoupling.yml` | `pull_request_review`(`submitted`)、`pull_request`(`labeled`、`unlabeled`) | `ubuntu-latest`,`timeout-minutes: 5` | 只跑 `decoupling-gate`(不构建、不测试) |
| `nightly.yml` | 每日定时(UTC 18:30,北京时间 02:30)+ `workflow_dispatch`(可选输入 `sha`);`concurrency: nightly`(不取消) | `summary`:`ubuntu-latest`(`timeout-minutes: 2`);`nightly`:`ubuntu-latest`;`nightly-arm`:`ubuntu-24.04-arm` + `container: ubuntu:24.04`(两者的 `timeout-minutes` 取 [testing §6](testing.md#6-ci-运行矩阵)) | 见[每夜任务](#每夜任务nightlyyml) |
| `release-aarch64.yml` | `v*` 标签(正式版 `vX.Y.Z`、开发构建 `vX.Y.Z-dev.N`);`workflow_dispatch`(输入 `sha`、`dev_n` 出开发构建,或 `tag` 为已推送的正式版标签重跑;可选 `skip_nightly`、`skip_verify`,只认 Admin) | `preflight`、`conformance-x86`、`publish`:`ubuntu-latest`;`build`:`ubuntu-24.04-arm` + `container: ubuntu:24.04`(`timeout-minutes` 取 [testing §6](testing.md#6-ci-运行矩阵)) | 见[发布工作流](#发布工作流release-aarch64yml) |
| `verify-bundle.yml` | `workflow_call`(正式版由 `release-aarch64.yml` 调用);`workflow_dispatch`(输入 `sha`) | `ubuntu-24.04-arm` + `container: ubuntu:24.04`(`timeout-minutes` 取 [testing §6](testing.md#6-ci-运行矩阵)) | 见[验证包工作流](#验证包工作流verify-bundleyml) |
| `release-x86-cross.yml` | 只有 `workflow_dispatch`(输入 `sha`、`version`、`dev_n?`) | `ubuntu-latest`(`timeout-minutes: 90`) | arm runner 不可用时的路径,见[应急交叉编译](#应急交叉编译release-x86-crossyml) |
| `fuzz-repro.yml` | 只有 `workflow_dispatch`(输入 `version`、`target`、`crash_id`、`input_b64`) | `ubuntu-24.04-arm` + `container: ubuntu:24.04`(`timeout-minutes: 20`) | 用该版本验证包里已编译的 fuzz 二进制复现服务器上的疑似失败,见[复现疑似失败](#复现疑似失败fuzz-reproyml) |

### 凭证与权限

- 全部工作流默认 `permissions: contents: read`;只有下列作业提权:`preflight` 与每夜 `summary`(`actions: read`、`checks: read`)、`decoupling-gate`(`pull-requests: read`)、`verify-bundle.yml` 的 `resolve`(`actions: read`,`workflow_dispatch` 时核对 `ci.yml` 结论)、`publish` 与 `verify-bundle.yml` 的上传作业(`contents: write`)、需要 S3 fixtures 的作业(`id-token: write`:质量门 `rust`、发布 `build`)。调用可复用工作流的作业授予被调用作业所需权限的并集(`ci.yml` 的 `quality-gates`;发布的 `verify` 为 `contents: write` + `actions: read`,因为 `verify-bundle.yml` 也声明了只在 dispatch 时运行的作业)。
- **S3 fixtures 经 GitHub OIDC**:只有执行 `cargo xtask fixtures fetch` 的作业开 `id-token: write`,用 `aws-actions/configure-aws-credentials` 扮演只读 fixture 桶的角色(角色 ARN 与区域存仓库变量 `HFT_FIXTURES_ROLE_ARN`、`HFT_FIXTURES_AWS_REGION`;未设置而需要读 S3 指针时该步失败;信任策略只接受 `repo:Fiora-gwx/hft:*`;创建该角色是 `[需授权]` 的 IAM 变更)。不在仓库或 Actions secrets 里存任何长期 AWS 凭据。
- **告警群消息**(每夜摘要)经只发不读的 Telegram bot:仓库 secrets `HFT_ALERT_TG_TOKEN`、`HFT_ALERT_TG_CHAT`(`.github/scripts/notify.sh`);未配置时只写作业摘要并给出警告。这是 CI 唯一的 secret。
- 没有任何作业持有服务器凭据或 SSH key(服务器自己拉取制品;服务器上的只读拉取 token 的归属与轮换见 [deploy 发布归档](../deploy/README.md#发布归档与-releasectl))。

### 质量门(`quality-gates.yml`)

RPL-058–060、RPL-270–273 的现场 Model 回放文件、检查点、生成参数及独立进程报告保存为 `replay-model-evidence` artifact,保留 7 天。测试只导出自己生成的合成文件,不读取生产 journal;该制品用于核对与后续固定样本评审,不代替 `fixtures/replay` 的指针和哈希。

全部在 `ubuntu-latest`;固定工具链 + `Swatinem/rust-cache`;内容与时长上限见 [testing §6](testing.md#6-ci-运行矩阵)。作业划分(同 Trading 的 `rust` / `python` / `operations` 三分,其中后两者合并为 `ops-py` 共用环境准备):

| 作业 | 内容 |
|---|---|
| `rust` | `timeout-minutes: 30`(安全上限;取值与理由归 [testing §6](testing.md#6-ci-运行矩阵),目标 12 分钟)。条件 `!cancelled() && needs.decoupling-gate.outputs.docs_only != 'true'`;只有确认仅改文档时跳过,路径判定失败仍运行。作业开头 `.github/scripts/free-disk.sh` 在后台删除 runner 预装而作业不用的工具链(Android SDK、.NET、GHC、CodeQL 等;镜像 `/` 只余约 20 GB,`ci` 剖面测试树、aarch64 检查与基准链接会写满,链接器输出遇磁盘满以 SIGBUS 崩溃),基准冒烟前打印 `df -h /` 与 target 目录大小。先以单元/属性步的同一剖面与特性构建它的全部单元(`cargo build --locked --profile ci --workspace --all-features --tests --examples --keep-going`,与 `cargo test --no-run` 的单元相同;`--keep-going` 在 `cargo test` 上未稳定;随后按 `cargo xtask dst` 的同一命令构建 `hft-dst`(它按自己的特性集解析,依赖另编一份,放在这里使其进缓存);`rust-cache` 设 `cache-on-failure: true`,而一个缓存键的首次保存即定稿、之后命中的运行不再保存,所以依赖要在任何门能使作业停下之前构建完;测试自己起的嵌套 cargo 构建——trybuild 的编译失败/UI 测试在 `target/tests/trybuild`、hft-journal 特性门夹具在 `target/jnl-feature`,dev 剖面——另有一个 `actions/cache`(键 `x86-nested-<rust-toolchain.toml 哈希>-<Cargo.lock 哈希>`,前缀匹配恢复,只在 `cargo xtask ci` 成功且非精确命中时保存,即每个 `Cargo.lock` 一次),因为 `rust-cache` 的键常由测试前就停下的运行定稿);再 `cargo xtask ci --no-doc-check`(前一步失败也运行,使 fmt 与 clippy 照常报告;以 `ci` 剖面,clippy 也用 `ci` 剖面以与测试共用构建脚本与过程宏:fmt、clippy、`deny check licenses bans`、deps、hot-check、单元/属性、策略测试、金样、`gen-schema --check`、`fixtures check`、DST 冒烟——doc-check 由同一提交的 `docs` 作业运行,不在本作业重复——`dst --hashtrail check --first 50` 核对一致性集登记顺序前 50 个种子的 `HashTrail` 与金样 `fixtures/dst/hashtrail.toml` 逐字节相同);`cargo check --locked --target aarch64-unknown-linux-gnu --workspace`(ISA 专属代码保持可编译;前面的门失败也运行);按路径的步骤(规则归 [testing §6](testing.md#6-ci-运行矩阵)):`cargo xtask scaffold-gate`(复用本作业的 target 目录)、改动目标的 fuzz 冒烟、`fixtures/replay/` 样本 `replay-verify`、基准冒烟 `bench-gate --smoke`(`ci` 剖面,不做 fat LTO)、`rust-toolchain.toml` 改动时同一作业内 base 与 head 的相对基准(只报告,规则 [B4](hot-path-rules.md#构建b));`Cargo.lock` 改动时 `cargo deny check advisories`(`continue-on-error`,不阻塞);生成物核对(`gen-schema --check`、`gen-host --check`、策略 schema 与 `HashTrail` 金样)失败时,把重新生成的差异作为 `regen.patch` 上传(`retention-days: 7`),由人下载后 `git apply` 并在同一提交提交——本地不要求能编译 |
| `ops-py` | 条件同 `rust`。一个作业,按改动路径决定步骤:改了 `python/**`、`strategies/*/pipeline/**`、`crates/hft-py/**` 或 `tools/az-probe/**` → 以 `maturin build` 构建 x86_64 的 `hft_py`(`testing` 特性的一份装进作业 venv,不带的一份装进第二个 venv 供 [hft-py](../crates/hft-py/TESTS.md) 的公开名字与存根核对),构建 hft-journal 的测试写者与 hft-py 的夹具生成器,解除 `RLIMIT_MEMLOCK` 后跑 `crates/hft-py/tests/python`;再跑 `ruff`、`mypy --strict`、`pytest`(离线,fixtures 驱动;含各策略的 pipeline 插件包与 `tools/az-probe`);改了 `deploy/**`、`.github/**` 或 `tools/xtask/tests/test_ci_*.py` → CI 脚本回归用例(TOOL-187–TOOL-191)、`shellcheck deploy/**/*.sh`、`bats` 脚本测试、建测试用户与目录夹具后 `systemd-analyze verify` 全部单元、deploy 的非容器 pytest、`actionlint`(固定版本的 GitHub Releases 二进制,按工作流里写定的 SHA-256 校验后运行;不用 Docker Hub 镜像——匿名拉取会被限速);deploy 的 systemd 容器测试(`deploy/tests/container/`、`deploy/dev2/tests/container/`,含 dev2 护栏,[deploy 测试](../deploy/README.md#测试与验收))按上表 `gen-host` 的同一路径判定运行,其余 PR 跳过(每夜全量跑,见[每夜任务](#每夜任务nightlyyml));都没改 → 只打印"无相关改动"并成功 |
| `decoupling-gate` | `cargo xtask decoupling-gate --base … --head … --pr …`(PR 级解耦检查,上线门槛 ⑧ 的一部分,[tools](../tools/README.md#cargo-xtask-decoupling-gate));不构建、不测试;见下。另输出路径判定 `docs_only`,供 `rust` 与 `ops-py` 使用 |
| `docs` | `cargo xtask doc-check` 与 `cargo xtask test-trace`(每个 PR 与 `main` 推送都跑;只改 `*.md` 的 PR 只跑这一作业与 `decoupling-gate`——后者输出 `docs_only` 决定 `rust`、`ops-py` 是否运行,不构建) |

`rust` 的质量门失败且存在 DST 输出时,上传 `dst-failures` 制品,保留 7 天。内容为运行结果、复现命令与失败原因、故障计划和最后 200 条事件的 journal 片段,路径与格式归 [tools 的 DST 入口](../tools/README.md#cargo-xtask-dst)。

显式账本场景的逐种子证据以 `ledger-case-seeds` 制品保留 7 天,成功和失败都上传。内容归 [tools 的场景入口](../tools/README.md#cargo-xtask-dst);其通过结论只覆盖所列场景与种子,不替代完整 M2、服务器累计运行或生产来源 fixture 验收。

每夜日期派生种子的结果与场景证据以 `nightly-dst-seeds` 保留 7 天,包含种子计划、逐种子结果和失败片段,不记录服务器累计账本。

- **`decoupling-gate`**:PR 同时改了 `strategies/*` 与引擎 crate 或 app(范围取 `cargo xtask deps` 的分层表,[tools](../tools/README.md#cargo-xtask-deps))时失败。**豁免**只在两个条件同时成立时生效:PR 带 `engine-change` 标签,且有一位 Admin(名单 `.github/hft-admins.txt`,当前为负责人)的批准评审——作业用 `GITHUB_TOKEN`(`pull-requests: read`)读评审列表核对批准人;只有标签、或批准人不在名单里 → 仍失败。它在 `ci.yml` 的 PR 运行中执行一次;评审提交与标签变化只经 `decoupling.yml` 重跑这一个作业,不重跑构建与测试。`main` 推送时它对合并提交相对第一个父提交的差异与对应 PR 的标签、评审做同样判定。每次豁免写进作业摘要(`$GITHUB_STEP_SUMMARY`:PR 号、批准人、改动的引擎 crate 列表),并列入当夜每夜摘要的"解耦豁免"一节。
- **`HashTrail` 金样**:`fixtures/dst/hashtrail.toml`(种子 → 每个 `HashTrail`)随源码提交;改变 DST 轨迹的 PR 在同一提交里重新生成它(`cargo xtask dst` 输出,评审核对),核对范围:每个 PR 与 `main` 推送的 `rust` 作业在 x86_64 上核对一致性集登记顺序的前 50 个种子(`--hashtrail check --first 50`);每夜 `nightly` 与正式版标签核对一致性集的全部种子(种子数归 [testing §6](testing.md#6-ci-运行矩阵))。正式版的标签工作流在被发布的同一 SHA 上于 x86_64(`conformance-x86`)与 aarch64(`build` 的一致性集)各跑一次一致性集的固定种子,两侧都与该 SHA 的这份金样逐字节比较,所以比较对象就是被发布的 SHA,不依赖其他运行的 artifact;每夜不保存 `HashTrail`。
- **检出与守卫**:PR 运行检出 PR 头提交(不是 GitHub 默认的合并提交),使 `preflight` 按 `head_sha` 查到的结论对应被测内容;按路径的步骤由 `.github/scripts/changed-paths.py` 判定(PR 相对合并基点,`main` 推送相对第一个父提交;改名按删除与新增列出两端,避免移出受检目录时漏跑;改动的 fuzz 目标 = `fuzz-targets.toml` 中 crate 目录或 `paths` 含改动文件的目标)。依赖后续里程碑组件的步骤按"该 xtask 子命令或路径存在"执行:`.github/scripts/xtask-inventory.sh` 读 xtask 的 `help` 输出,里程碑已到(`test-trace.toml` 的 `current_milestone`,对照 tools/TESTS.md)而缺失的子命令使作业失败,未到的步骤跳过并写摘要。发布检查与每夜汇总共用 `.github/scripts/github_common.py` 的 GitHub 查询和 Admin 名单读取;API 错误使发布检查失败。
- **作业条件显式判定上游结果**:`if: needs.<job>.result == 'success'`(或与 `== 'skipped'` 的显式组合),不依赖隐式 `success()`([defensive-patterns](defensive-patterns.md#ci-条件显式判定上游结果));改了作业图条件后逐个核对每个作业的结论,而不是只看工作流结论。
- **不重复跑**:PR 验证功能提交,合并后 `main` 验证一次;打标签不再重跑普通套件,发布工作流的 `preflight` 只查证已有的结论(与 Trading 相同)。
- **`cargo deny` 分两半**:`licenses` 与 `bans` 阻塞;`advisories` 不阻塞——advisory 库每天更新,会在没有代码改动时把 `main` 变红;它在 `Cargo.lock` 改动的 PR 与每夜批次里跑,结果进每夜摘要;处理:评估影响 → 升级依赖,或在 `deny.toml` 以带到期日的忽略项写明理由。

### 发布工作流(`release-aarch64.yml`)

1. **`preflight`**(`ubuntu-latest`,`actions: read`、`checks: read`)确定 SHA:标签触发取标签指向的提交;`workflow_dispatch` 取输入 `sha`(必须是某个 PR 的头提交或 `main` 上的提交;判定为 `origin/main` 的祖先或某个已推送分支的头提交)或输入 `tag` 指向的提交。实现:`.github/scripts/release-preflight.py`(`release-x86-cross.yml` 共用)。下列任一不满足即失败,**不在发布工作流里补跑质量门**:
   1. `gh api …/actions/workflows/ci.yml/runs?head_sha=<SHA>&status=success` 有结果,且该次运行的 `rust` 作业结论为 `success`(被跳过的不算);
   2. 该 SHA 最近一次名为 `decoupling-gate` 的 check run 结论为 `success`(`ci.yml` 或 `decoupling.yml` 产生;`pull_request_review` 触发的运行与 PR 头 SHA 的关联方式待核实,方法:M1 第一周提交一次评审后用 `gh api …/commits/<SHA>/check-runs` 查看);
   3. 正式版 `vX.Y.Z`:该 SHA 是 `origin/main` 的祖先,且 `main` 上实际执行了的最近一次 `nightly.yml` 运行结论为 `success`;`nightly` 作业被跳过的运行不计入最近一次,也不当作通过;最近一次实际执行失败时不得回退查找更早的成功运行;
   4. 由 `workflow_dispatch` 创建开发构建时,标签 `vX.Y.Z-dev.<dev_n>` 尚不存在(`X.Y.Z` = 该 SHA 的工作区版本)。

   **Admin 覆盖**:`workflow_dispatch` 输入 `skip_nightly: true` 且 `github.actor` 在 `.github/hft-admins.txt` 中 → 只跳过第 3 条的每夜结论要求(SHA 仍须在 `main` 上,第 1、2 条照常);不在名单中 → 失败。覆盖写进作业摘要。preflight 把所依据的 `ci.yml` 运行、`decoupling-gate` check run、最近一次实际执行的每夜运行与 Admin 覆盖写进 `RELEASE.json.evidence`(字段归 [deploy](../deploy/README.md#发布归档与-releasectl)),运行记录被 GitHub 按保留期删除后仍可追溯;仓库的 Actions 保留期设为允许的上限(上限值待核实,方法:建仓时读仓库 Actions 设置页)。
2. **`conformance-x86`**(只在正式版;`ubuntu-latest`;条件 `needs.preflight.result == 'success'`,与 `build` 并行):检出同一 SHA,以 `ci` 剖面构建 `hft-dst`,对一致性集的固定种子(种子数与时长上限归 [testing §6](testing.md#6-ci-运行矩阵))运行 `cargo xtask dst`,输出的 `HashTrail` 与该 SHA 的 `fixtures/dst/hashtrail.toml` 逐字节比较;不一致即失败。
3. **`build`**(`ubuntu-24.04-arm`,`container: ubuntu:24.04`,与生产 Ubuntu 24.04/glibc 基线一致;`timeout-minutes` 取 [testing §6](testing.md#6-ci-运行矩阵);条件 `needs.preflight.result == 'success'`):
   1. 安装构建依赖与固定工具链;`rust-cache` 键 `arm64-release`(只给发布构建;7 天未用被逐出时冷构建);打印 `/proc/cpuinfo` 的 `Features`([构建剖面](#构建剖面)的待核实项,方法见该节)。
   2. 版本核对:`vX.Y.Z` 必须等于工作区版本;开发构建的 `X.Y.Z` 必须等于工作区版本。`channel` = 开发构建时 `dev`,否则 `release`。
   3. **正式版**跑 aarch64 一致性集(内容归 [testing §6](testing.md#6-ci-运行矩阵)):单元/金样、与 `conformance-x86` 相同的固定种子 DST,`HashTrail` 与 `fixtures/dst/hashtrail.toml` 逐字节比较、`fixtures/replay/` 样本 verify;不一致即不发布。开发构建跳过。
   4. 构建:正式版 `cargo build --locked --profile release-hot -p hft-core -p kill-switch --target aarch64-unknown-linux-gnu`,其余二进制 `cargo build --locked --release --target aarch64-unknown-linux-gnu`;开发构建全部 `--release`,只构建 dev2 运行的二进制([构建剖面](#构建剖面))。对每个二进制 `file … | grep 'ARM aarch64'`;`hft-core --check` 对样例配置通过(CPU 特性检查;样例配置的路径待 hft-core(M3)定,工作流暂取 `HFT_CORE_SAMPLE_CONFIG`,缺省 `apps/hft-core/config/sample.toml`,`apps/hft-core` 存在而文件缺失即失败)。
   5. `hft-core schemas --out dist/schemas/strategies`,与 `fixtures/strategy-schemas/` 逐字节相同,否则失败。
   6. 构建 Python wheel(各包 + `hft_py` aarch64 wheel)与离线 wheelhouse,合并 `requirements.lock`;以 `clang -O2 -target bpf` 把 `deploy/imds/hft-imds-connect.bpf.c` 编译为 `.bpf.o` 放进归档的 `deploy/imds/`([deploy IMDS 锁定](../deploy/README.md#imds-锁定与凭证代理));生成 SBOM(工具在建仓时选定)。
   7. `deploy/release/package-release.sh "$version" "$git_sha" dist`(`SOURCE_DATE_EPOCH` = 提交时间,可复现打包,[deploy](../deploy/README.md#发布归档与-releasectl));正式版另以 `bench` 剖面打包基准制品 `hft-bench-<ver>-aarch64.tar.zst`(不进生产归档,[benchmarks](../benchmarks/README.md));上传 workflow artifact,`retention-days: 1`。工作流交给打包脚本的输入:`dist/schemas/strategies/`、`dist/python/{wheels,wheelhouse,requirements.lock}`、`dist/deploy/imds/*.bpf.o`、二进制留在 `target/aarch64-unknown-linux-gnu/{release,release-hot}/`;环境变量 `HFT_RELEASE_EVIDENCE`(preflight 的 `evidence` JSON 文件,进 `RELEASE.json.evidence`)、`HFT_CI_RUN_ID`(进 `RELEASE.json.ci_run_id`)、`SOURCE_DATE_EPOCH`;脚本写出 `dist/hft-<tag>-aarch64.tar.zst` 与 `.sha256`。基准制品由工作流直接以同样的可复现 tar 参数打包(`benches/` 下的 `bench` 剖面可执行文件 + `bin/xtask`)。
4. **`verify`**(只在正式版):以 `workflow_call` 调用 `verify-bundle.yml`,输入为同一 SHA 与版本。
5. **`publish`**(`ubuntu-latest`,`contents: write`;条件 `needs.build.result == 'success'`,正式版另要求 `needs.conformance-x86.result == 'success'` 与 `needs.verify.result == 'success'`;Admin 以 dispatch 输入 `skip_verify: true` 跳过验证包时 `verify` 不运行、`publish` 不要求它——用于热修,之后以 `verify-bundle.yml` 的 `workflow_dispatch(sha)` 补出包,开正式实盘所用的版本仍须有验证包与补跑):`workflow_dispatch` 时先在 `sha` 上创建带注释的标签 `vX.Y.Z-dev.<dev_n>` 并推送(以 `GITHUB_TOKEN` 推送的标签不再触发工作流);`gh release create --verify-tag`,上传生产归档与 `.sha256`、正式版的基准制品与验证包(各带 `.sha256`);`channel = dev` 时加 `--prerelease`,并删除最近 10 个之外的开发构建 pre-release(标签不删、不移动)。正式版资产不可变。Release 资产不占 artifact 存储配额。

### 验证包工作流(`verify-bundle.yml`)

产出上线门槛 ③④ 在服务器上累计所用的验证包;包内容与安装归 [deploy](../deploy/README.md#验证包与-hft-verify),驱动 `hft-verify` 的规格归 [tools](../tools/README.md#hft-verify),`hft-dst` 归 [hft-testkit](../crates/hft-testkit/README.md),账本规则归 [testing §6.1](testing.md#61-服务器上的累计运行)。

- **何时**:正式版 `vX.Y.Z` 必跑(由发布工作流调用,随同一 Release 发布;Admin 以 `skip_verify` 跳过的热修除外,事后补出包);`workflow_dispatch(sha)` 可对任一 `ci.yml` 全绿、且已有发布标签(正式版或开发构建)指向它的提交出包,资产上传到该标签的 Release;SHA 上没有标签时失败并提示先出开发构建(有多个标签时取正式版,否则取 N 最大的开发构建)。开发构建不自动出包。
- **步骤**(`ubuntu-24.04-arm`,`container: ubuntu:24.04`;`timeout-minutes` 取 [testing §6](testing.md#6-ci-运行矩阵);`rust-cache` 键 `arm64-verify`):安装固定 nightly 与 `cargo-fuzz`;对 `tools/xtask/fuzz-targets.toml` 中每个 core 解码器目标 `cargo fuzz build`(AddressSanitizer);以 `ci` 剖面构建 `hft-dst` 与 `hft-verify`(优化级别与调试信息由环境变量恢复为 3 级与行表,见[构建剖面](#构建剖面));从仓库的种子语料与每夜 fuzz 的 `actions/cache` 语料合成初始语料 `corpus/<target>/`;计算 `path_digest`(见下)写进 `VERIFY.json`;`deploy/release/package-verify.sh` 打包为 `hft-verify-<ver>-aarch64.tar.zst` + `.sha256`(输入为环境变量 `HFT_VERIFY_STAGE` 指向的暂存目录:`fuzz/<target>`、`bin/{hft-dst,hft-verify}`、`corpus/<target>/`、`path-digest.json`(`cargo xtask path-digest --sha <SHA> --json` 的输出));以 `ASAN_OPTIONS=quarantine_size_mb=16` 对包内每个 fuzz 目标跑 10 s、`hft-dst` 跑 10 个固定种子作冒烟;上传 workflow artifact(`retention-days: 1`),由发布工作流的 `publish` 或本工作流的上传作业(`contents: write`)挂到 Release。
- **`path_digest`**(门槛 ③④ 跨版本累计的口径,规则归 testing §6.1):每个目标组一个,= 该组相关源路径在被构建 SHA 上的内容摘要——对 `git ls-tree -r <SHA> -- <路径…>` 的输出(按路径排序的 `路径 + blob 哈希`)求 SHA-256;每组的路径另含 `Cargo.lock`、`tools/xtask/toolchains.toml`(nightly 日期)与根 `Cargo.toml`(剖面设置),依赖、nightly 或剖面变化后旧的累计量不再计入。路径清单写在仓库里:③ 每个解码器目标在 `tools/xtask/fuzz-targets.toml` 的 `paths`(对应的 `venue-*`、`hft-venue-proto` 与 `hft-types` 的十进制解析模块),④ 在同一文件的 `[dst].paths`(引擎 crate 与 `hft-testkit`)。`hft-verify ledger show --candidate <ver>` 只累计 `path_digest` 与候选版本相同的运行([tools](../tools/README.md#hft-verify))。

### 复现疑似失败(`fuzz-repro.yml`)

服务器上 fuzz 的疑似失败(受限主机上的单输入超时与内存超限,口径归 [testing §6.1](testing.md#61-服务器上的累计运行))由本工作流以 libFuzzer 默认限额复现;结论经 `hft-verify crash resolve` 回到服务器账本([tools](../tools/README.md#hft-verify))。

- **触发**:只有 `workflow_dispatch`,输入 `version`(产生该疑似的验证包版本,即其 Release 标签)、`target`(fuzz 目标名)、`crash_id`(`hft-verify crash list` 的条目)、`input_b64`(最小化后的崩溃输入,base64)。输入超过 `workflow_dispatch` 的长度上限时的传递方式待核实(方法:M1 用一个 64 KiB 的输入试一次)。
- **步骤**(`ubuntu-24.04-arm` + `container: ubuntu:24.04`,与服务器同架构;`timeout-minutes: 20`):下载 `version` 对应 Release 的验证包并校验 `.sha256` 与 `VERIFY.json`;解码输入,其 SHA-1 与 `crash list` 记录的不一致 → 失败;以包内已编译的 `fuzz/<target>`(同一 nightly 与 ASan)按 libFuzzer 默认限额单独运行该输入,不编译(本地复现同样可用 `cargo xtask fuzz <target> --repro <input>`,[tools](../tools/README.md#其他-xtask-子命令));作业摘要写结论 `reproduced` / `not-reproduced`、输入 SHA-1、版本与运行 URL;复现时上传该输入(`retention-days: 7`),由人经 PR 写进回归金样。
- **结论回写**:运维在服务器上执行 `hft-verify crash resolve <crash-id> --reproduced|--not-reproduced --evidence <本次运行 URL>`;本工作流不持有服务器凭据,不直接改账本。复现与未复现的作业结论都是 `success`(结论在摘要);构建失败或输入不符为 `failure`,该疑似保持未决。

### 每夜任务(`nightly.yml`)

三个作业:`nightly` 与 `nightly-arm` 并行启动、互不依赖;最后 `summary`(≤ 2 min)无论上游成功、失败或被取消都汇总实际结论。各自跑什么与时长上限见 [testing §6](testing.md#6-ci-运行矩阵),资源规则见[免费额度](#免费额度)。每夜只做回归冒烟,结果不计入上线门槛 ③④(累计只在服务器上,见[仓库与 CI](#仓库与-ci))。

- **`nightly`**(`ubuntu-latest`,时长上限见 [testing §6](testing.md#6-ci-运行矩阵)):对当时的 `main` 头提交运行(`workflow_dispatch` 可指定 SHA);DST 一批新种子(≤ 6 min,批量写在 `tools/xtask/dst-nightly.toml` 的 `seeds`、`steps`;起点 `S` = UTC 日期 `YYYYMMDD` × 10^6)、fuzz 按目标短跑轮转(≤ 4 min,写在 `tools/xtask/fuzz-targets.toml`)、`tests/recover/` 进程内恢复用例、`dst --hashtrail check`(一致性集的全部种子,不一致即当夜失败)、deploy 检查全量运行(与 `quality-gates.yml` 的 "Deploy checks" 步骤相同的命令,含 PR 上按路径跳过的 systemd 容器测试;`deploy/` 存在才执行,M3 起)、`cargo deny check advisories`;作业在超时前自行收尾。fuzz 的最小化语料只存 `actions/cache`(全部目标一个缓存项,路径 `fuzz/corpus/<target>/`,键 `fuzz-corpus-<run_id>`,按前缀 `fuzz-corpus-` 恢复——`actions/cache` 的步骤是静态的,不能按目标数生成;被逐出时从仓库种子语料重来);不上传账本或语料 artifact。发现崩溃时上传最小化的崩溃输入(`retention-days: 7`),由人经 PR 写进对应 crate 的回归金样。
- **`nightly-arm`**(`ubuntu-24.04-arm`,时长上限见 [testing §6](testing.md#6-ci-运行矩阵)):只跑与架构有关的测试(`ci` 剖面);`rust-cache` 用自己的键 `arm64-ci`(与发布构建的剖面不同,共用一个键会让先保存者决定缓存内容);缓存未命中导致超时视为基础设施失败(摘要中标出),当夜由人在运行页重跑一次(工作流不持有 `actions: write`)。
- 门槛 ⑦ 中的"回放每日通过"读服务器上每日 `replayer verify` 的结果文件(`hft-replay-verify` 定时单元,[deploy](../deploy/README.md#systemd-单元)),不占 CI 分钟。
- 每夜失败不阻塞 PR,但阻止正式版标签(`preflight` 第 3 条;Admin 可覆盖,用于每夜恰好是红的热修);每夜摘要(一条消息推到告警群)列出各作业的结论、当日的解耦豁免。

### 应急交叉编译(`release-x86-cross.yml`)

`ubuntu-24.04-arm` 不可用(排队、故障)时,在标准 x86 runner 上交叉编译正式版或热修:

- 与 `release-aarch64.yml` 同一个 `preflight`;在 `ubuntu-latest` 上装 `gcc-aarch64-linux-gnu` 与同一固定工具链,以交叉链接器构建同样的 aarch64 二进制(剖面与 `target-cpu` 不变)、Python wheel(`hft_py` 经 `maturin --target aarch64-unknown-linux-gnu`)与归档,用同一个 `package-release.sh` 打包、同样上传 Release。
- 不跑 aarch64 一致性集与原生冒烟(x86 上跑不了 aarch64 二进制):它们改由服务器安装时的 `releasectl verify` 与 `hft-core --check` 承担;不产出验证包与基准制品(验证包 fuzz 目标交叉编译的可行性待核实,方法:季度演练中试一次 `cargo fuzz build --target aarch64-unknown-linux-gnu`)。开正式实盘所用的版本仍须按 testing §6.1 有验证包补跑,所以应急版本用于热修已上线的版本。
- 用本工作流出的版本在发布说明与 `RELEASE.json.ci_run_id` 上可辨认。标签 `v<version>`(或 `-dev.<dev_n>`)不存在时由 `publish` 在 `sha` 上创建;已存在时必须指向 `sha`。归档的 `schemas/strategies/` 取 `fixtures/strategy-schemas/` 的副本(aarch64 的 `hft-core` 在 x86 上不能运行;该 SHA 的绿 `ci.yml` 已证实导出与它逐字节相同)。
- **每季度演练一次**:由负责人对 `main` 的一个已有标签的提交出一个开发构建,在 dev2 上安装到 `sim` 组并跑第一分钟验收;用例与证据见 [DRL-110](tests/drills.md#drl-110-arm-runner-不可用路径的季度演练交叉编译的开发构建在-dev2-sim-组通过第一分钟验收)。

### 探针构建(`probe-build.yml`)

手动 `workflow_dispatch(sha)`:在 `ubuntu-24.04-arm`(容器 `ubuntu:24.04`)上以 `release` 剖面构建 M1.5 探针程序 `latency-slice` 与 `hostprobe`,打包 `probe-aarch64.tar.zst`(含 `SHA` 与 `SHA256SUMS`)作为 workflow artifact(保留 3 天),供按需租用的探针主机下载运行;不进任何 Release 与生产归档,`timeout-minutes: 40`。

### 免费额度

本节是 CI 资源规则的唯一归属。`Fiora-gwx/hft` 为公开仓库,GitHub 标准托管 runner(`ubuntu-latest`、`ubuntu-24.04-arm`)的运行分钟免费,不占 Trading 私有仓库的月度分钟额度。PR、每夜任务与开发构建不设月度分钟门槛,也不需要 `hotfix` 标签来启动完整质量门。规则依据 [GitHub Actions 计费](https://docs.github.com/en/billing/concepts/product-billing/github-actions)与[标准托管 runner](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)。

- **运行资源**:只用标准托管 runner,不启用 larger runner 或自托管 runner。各作业保留 [testing §6](testing.md#6-ci-运行矩阵) 的超时与冒烟批量,用于控制反馈时间和异常任务;同一 PR 的旧运行取消,只有文档改动时跳过构建测试。
- **费用边界**:公开仓库的标准 runner 分钟免费不代表所有服务免费。larger runner、超出免费范围的存储与缓存、AWS/S3 等按各自规则计费;本仓库不启用额外付费资源。按下列保留策略管理 artifact 与 cache。
- **artifact 与 cache**:发布与验证包的 workflow artifact `retention-days: 1`(发布物以 Release 资产为准);每夜只在崩溃时上传小文件;CI 不保存累计账本 artifact。cache 存 Rust 构建缓存(x86 `ci`(键 `x86-ci-v2`,另有测试嵌套构建的 `x86-nested-*`,见 `rust` 作业;`rust-cache` 的键不含剖面,改动依赖所用的剖面设置——`[profile.ci]` 的继承项、`package."*"`——时把键的版本号加一,否则完全命中的旧缓存不再保存,之后每次都重编依赖)、只构建 xtask 的 `docs`/`decoupling-gate`(共享键 `x86-xtask`)、arm 发布 `arm64-release`、`nightly-arm` 的 `arm64-ci`、arm 验证包 `arm64-verify`、应急交叉编译 `x86-cross`)与 fuzz 语料,被逐出时冷构建,超时的每夜视为基础设施失败。

决定与替代范围见 [公开仓库 CI](../.agents/notes/implemented/process/2026-10-10-public-repository-ci.md)。

## 参考机

- 参考机是按需按小时租用的同型 EC2 实例(不是 CI runner),从 M1.5 延迟探针起使用([11 实施路线图](../design/11-实施路线图.md)、[latency-probe-milestone](../.agents/notes/proposed/architecture/2026-10-03-latency-probe-milestone.md));开机、关机与费用是 `[需授权]` 的 AWS 操作,由负责人执行,实例只放测试网 key。租用实例不安装生产归档、不需要主机清单([deploy 制品](../deploy/README.md#制品从哪里来));开在 [az-probe](runbooks/az-probe.md) 选出的 AZ(AZ 探测在 M1.5 之前完成)。
- 每个里程碑结束时在参考机上运行该里程碑正式版附带的基准制品:`bench-gate --reference`(相对回归 + 单 crate 上限,[benchmarks](../benchmarks/README.md));结果 JSON 存 S3 `hft-bench/` 前缀并记入 ops-log。里程碑结束时没有正式版的,先打一个正式版标签(可以不部署)以产出基准制品与验证包。
- 参考机的实例类型:M1.5 与 M3 末满订阅探针用 `c8g.4xlarge`(M3 末探针按 c16 线程计划满订阅运行,用来选定主机类与是否启用 IRQ 拆分,[m3-end-probe](../.agents/notes/proposed/architecture/2026-10-03-m3-end-probe.md));升级主机类选定后用同一实例类型([hft-core 线程计划](../apps/hft-core/README.md#线程计划))。

## 分支与发布

1. 合并绿的分支 → 等 `main` CI 与当夜任务通过 → 在该提交上打带注释的 `vMAJOR.MINOR.PATCH` 标签。**标签不移动**,修复发新补丁版本。
2. **开发构建**:需要在 `dev2` 联调时,按[日常流程](#日常流程)第 5 步出 `vX.Y.Z-dev.N`(`N` 从 1 递增,`X.Y.Z` = 当前工作区版本);发布工作流产出 `channel = dev` 的 pre-release。开发构建只用于 `dev2`([deploy](../deploy/README.md#制品从哪里来))。
3. 发布工作流构建 aarch64 归档([目标 CPU](#目标-cpu)),写入 Git SHA、`ci_run_id`、schema 版本、目标三元组与 `target-cpu`、事件 kind 表哈希与各策略的 `(kind_id, version, schema_id, schema_ver)` 到 `RELEASE.json`;正式版同时发布验证包与基准制品。
4. 归档包含调优即代码与 dev2 护栏([deploy/](../deploy/README.md));激活时 [hostprobe](../tools/README.md#hostprobe) 校验实际生效值。
5. 服务器侧拉取(`fetch-release.sh` + 只读令牌)→ 校验 → 安装不激活 → 冷激活;验证包用 `releasectl install-verify` 单独安装,不影响生产版本激活。
6. 版本变化只在 journal 末尾是 Drain 快照时允许冷启动;新版本验证 = 回放 verify + 模拟账户运行([testing](testing.md));在升级主机上先以新版本激活 `sim` 实例观察,再激活 `prod`;改动 `venue-*`、`hft-venue`、`hft-net`、`hft-oms` 的版本先在 dev2 测试网窗口冒烟([release-and-deploy](runbooks/release-and-deploy.md))。新子账户首次上线按 [runbooks/launch](runbooks/launch.md)。
7. 保留当前版本与**恰好一个**已验证的回滚版本。

变更类别:代码、协议、内核参数、单元文件 → 完整 CI + 发布;策略参数、名单、模式 → 控制命令,不触发构建([10 §6](../design/10-语言分工与代码组织.md))。

## AI 代理协作

- 代理读根 [AGENTS.md](../AGENTS.md) 与目标目录 README 开始工作;代理写的热路径代码必须经人工评审([12 §3.2](../design/12-风险与未决问题.md))。
- 金样、解码器、Python 包、文档可由代理完成;每个金样的期望输出由人按交易所文档复核后才合入([fixtures/](../fixtures/README.md#更新流程))。
- 代理不部署、不启动交易或采集、不删除数据、不改外部系统(含 GitHub 仓库与协作者设置、Actions 设置与计费设置、AWS 资源),除非用户在当次会话中明确授权。
