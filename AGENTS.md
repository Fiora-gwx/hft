# AGENTS.md

`hft` 是我们自己的低延迟交易系统的 Rust + Python 仓库:首要服务 CEX-CEX(ccex)策略,首批交易所 Binance、OKX、Bitget(各接现货 + USDT 永续);链上执行(cdex)是可选扩展。改 `crates/`、`strategies/` 或 `apps/` 之前先读 [docs/architecture.md](docs/architecture.md);写文档遵循 [docs/AGENTS.md](docs/AGENTS.md)。设计证据在 [../architecture-v2/](design/README.md)。

## 仓库状态:设计基线 v1 + 实现(M1 起)

规格已冻结为**设计基线 v1**:每个目录的 `README.md` 是该组件的**实现规格**,同目录的 `TESTS.md` 是它的**验收用例**;M1 起代码按[里程碑](design/11-实施路线图.md#3-里程碑按依赖)逐个组件落地。写代码前先读目标 README、`TESTS.md` 与它们链接的子系统页;组件以"`TESTS.md` 中里程碑已到的用例全部实现并通过"为完成([docs/tests](docs/tests/README.md#1-地位))。实现与规格不一致时按[修改流程](docs/tests/README.md#7-修改流程设计基线-v1-之后):**同一提交**改 README、`TESTS.md` 与代码,并在 [docs/changes.md](docs/changes.md) 记一行(推翻原决定另写 Agent Note),不允许分叉。frontmatter 的 `status`、`scope` 取值见 [README 模板](crates/AGENTS.md#readme-模板):`frozen` 组件不随 core 规格同步;`core` 组件必须通过[上线门槛](docs/testing.md#8-上线门槛)。

## 仓库布局

```
crates/      引擎 Rust 库,规则见 crates/AGENTS.md
  hft-types hft-event hft-refdata             纯基础:定点、ID、事件与命令、参考数据与 VenueCaps
  hft-strategy-sdk                            策略唯一依赖的稳定接口(Strategy/Model/参数 schema/视图/意图)
  hft-rt hft-journal hft-net hft-venue         热路径运行时:环/时钟/绑核、journal、网络、venue 运行时
  hft-venue-proto venue-<exchange>             VenueProtocol trait;纯交易所协议
  hft-book hft-oms hft-risk hft-ledger hft-exec 簿、订单、风控、记账、执行层
  hft-strat-lib hft-sequencer                  通用策略构件、主循环
  hft-ctl hft-telemetry hft-sim hft-py hft-testkit   控制面、观测、模拟撮合与模拟账户、PyO3、测试基建
  hft-ext-proto hft-amm hft-chain-evm hft-chain-solana   链上(optional,冻结)
strategies/  策略包,一策略一 crate(其 pipeline 插件在 strategies/<name>/pipeline/),规则见 strategies/AGENTS.md
apps/        进程(只装配),规则见 apps/AGENTS.md
python/      慢路径与运维框架(pipeline、ops_bot、treasury、research),规则见 python/AGENTS.md
contracts/   Solidity SwapGuard(optional,冻结)
deploy/      systemd、资源护栏、调优即代码、发布接入
benchmarks/  跨组件延迟门槛
fixtures/    解码金样与回放样本
tools/       xtask(门槛与生成器)、hft-cfg、hft-verify、az-probe、hostprobe
docs/        文档(docs/AGENTS.md)
.agents/     Agent Notes(决策记录)
```

## 命令(规划,由 `tools/xtask` 提供,在 CI 上运行)

```sh
cargo xtask ci              # fmt + clippy + deny + 依赖方向 + 单元/属性 + 金样 + 文档门槛
cargo xtask bench-gate      # 基准:CI 上 --smoke(ci 剖面)只报告;参考机上 --reference 按单 crate 上限与相对回归阻断,主机类选定后加路径绝对门槛(benchmarks/README.md)
cargo xtask replay-verify <journal>   # 验证回放:意图流与状态哈希逐字节一致
cargo xtask dst --seeds N   # 确定性模拟测试
cargo xtask gen-schema      # 从 hft-event 生成 Arrow schema / Python dtype / 事件表文档
cargo xtask doc-check       # 链接、预算、README 章节结构、名字越界检查(Markdown、Rust、Python)
cargo xtask test-trace      # 测试规格(TESTS.md、docs/tests)与测试函数一一对应(docs/tests §6)
cargo xtask deps            # 依赖方向(tools/xtask/deps.toml)
cargo xtask hot-check       # 热路径可机检规则(hot-path-rules.md)
cargo xtask fixtures        # 金样拉取/校验(fixtures/README.md)
cargo xtask fuzz <target>   # 解码器 fuzz(固定的 nightly 工具链,见 tools/README.md)
```

## 常驻规则

- **单写者**:每个状态只有一个写线程;业务状态只在 sequencer 线程;跨线程只走 SPSC 环([理由](docs/architecture.md#5-状态所有权))。
- **决策可见 ⟺ 已记录**:能影响 sequencer 决策的输入(行情、回报、命令、定时器、advice、预热数据、配置内容)先成为 journal 事件;回放只读 journal 就能逐字节复现意图流([事件模型](docs/subsystems/event-model.md))。
- **热路径规则强制**:无分配、无锁、任务内无阻塞与睡眠、无系统时钟、无 panic、无 `dyn`、下单路径无 f64;策略快速部分同样适用([hot-path-rules](docs/hot-path-rules.md))。
- **持久状态不存 TSC**:进快照或检查点的时刻与期限存 `engine_ns`(跨重启连续、停机不计)或 UTC 日历量;唯一例外是 [D6](docs/hot-path-rules.md#确定性d) 登记的 TSC 行,恢复时的处置(平移、作废或不读)按 D6。
- **传输错误 ≠ 业务失败**:超时、403/429/5xx、断连只能把订单或交易推入 `Unknown`;只有交易所回报或链上证据能改变仓位([defensive-patterns](docs/defensive-patterns.md))。
- **安静 ≠ 过期**:报价可用性按该 inst 的 BBO 成员连接健康判定,盘口长时间不变不算过期([defensive-patterns](docs/defensive-patterns.md#安静--过期报价可用性看连接健康))。
- **裸腿有界**:第一腿发出前检查第二腿报价可用与可负担,两腿按 lot 对齐;Broken 到期后执行层自动做一次可执行的有界平仓尝试;L3 与 flatten 不跳过 Broken、不依赖另一腿报价([defensive-patterns](docs/defensive-patterns.md#裸腿有界断腿自动平仓l3-与-flatten-不跳过))。
- **定点按资产指数**:同一资产在所有 venue 用同一 `px_exp/qty_exp`;乘积用 i128([hft-types](crates/hft-types/README.md))。
- **依赖只能向下**:见 [crates/AGENTS.md](crates/AGENTS.md#依赖方向);`venue-*` 是纯协议;`apps/*` 只装配。
- **策略与引擎解耦**:引擎 crate(含 dev 依赖)不依赖 `strategies/*` 与 `hft-strategy-registry`;策略 crate 只依赖 `hft-strategy-sdk` 与 `hft-strat-lib`;新增策略 = `strategies/<name>` + registry 清单一行,引擎、app 与 `python/` 框架零改动([strategies/AGENTS.md](strategies/AGENTS.md),`cargo xtask deps` 机检)。
- **名字不越界**:引擎 crate、app 与 `python/` 框架包不出现策略族名(含小写 `cdex`)或某个策略的规则数字;策略 crate 不出现交易所名,交易所差异进 `VenueCaps` 与 `venue-*`(`doc-check` 机检)。
- **一个账户一个策略实例**:一个交易所(子)账户只绑定一个策略实例,同一时刻只由一个系统交易([hft-refdata](crates/hft-refdata/README.md))。
- **不做同一交易所的现货 × 永续**:一个账户在同一交易所只用一个市场;refdata 与 `SetParams` 都拒绝([hft-refdata](crates/hft-refdata/README.md))。
- **事件 schema 只追加**:新增字段追加在体尾,改语义用新 kind([cookbook](docs/cookbook/adding-an-event-kind.md))。
- **无硬编码可调参数**:阈值、限额、超时、名单进 refdata、策略参数或 `RiskParams`,经控制环替换并被记录;账户事实(如永续杠杆)从交易所读取;协议常量与安全不变量除外。
- **错误配置在加载时失败**:缺引用、指数不一致、未知 venue、账户未绑定一律拒绝启动或拒绝该命令,不静默跳过。
- **新行为走扩展点**:新交易所 = `VenueProtocol` 实现 + `VenueCaps` 默认值;新策略 = 新策略包;新命令 = `ControlCmd` 变体。改 sequencer 主循环必须同时更新 [architecture.md](docs/architecture.md)。
- **测量先于优化**:性能声明附直方图或基准输出;延迟只用 [latency-budget](docs/latency-budget.md) 的分段名;延迟验收只在选定的统一升级主机上、prod 与 sim 同机时做,`dev2` 只记录。
- **测试描述行为**:每个测试对应 `TESTS.md` 或 `docs/tests/` 里的一个用例,函数名以用例编号开头(如 `oms_014_…`,[docs/tests §6](docs/tests/README.md#6-与代码的对应));金样不对就修 fixture 来源,不迁就归一化逻辑;策略、执行层、风控、OMS 改动必须过 `replay-verify` 与 DST([testing.md](docs/testing.md))。
- **交付路径**:`dev2`(不放正式实盘账户)→ 统一升级主机(`prod` 与 `sim` 同机)→ 通过上线门槛、零成交冒烟与正式实例 Shadow 后正式实盘;没有小额实盘阶段([launch](docs/runbooks/launch.md))。
- **评估用模拟账户,不用回测**:策略表现在模拟账户(实时行情 + 模拟撮合)上观察,只在 `instance.name = "sim"` 的实例运行;`prod` 实例不含 Sim 账户;回放只用于确定性验证与排障([hft-sim](crates/hft-sim/README.md))。
- **sim 与 prod 同机隔离**:sim 只用非隔离核,独立 slice、journal、冷盘目录、网卡与出口 IP;数值归 [hft-core](apps/hft-core/README.md)。
- **CI 是唯一构建环境**:本地只写代码并推送;GitHub 托管 runner(不用自托管 runner)跑质量门并产出 aarch64 发布构建与验证包;服务器只安装 CI 产出的制品,**不编译**;累计 fuzz 与 DST 在服务器上运行验证包([development.md](docs/development.md))。
- **发布纪律**:冷生产、不可变归档 + SHA-256 + `RELEASE.json`、保留一个已验证回滚版本;内核参数、资源护栏与调优脚本随发布归档([deploy/](deploy/README.md))。
- **共机护栏**:`dev2` 与 cdex 正式账户共机,hft 进程只在资源护栏内运行,cdex 优先;数值与守护规则归 [hft-core](apps/hft-core/README.md)。
- **秘密不入库、不入 journal**:API key 与私钥启动期从 Secrets Manager/KMS 载入;交易 key 不带提现权限;journal 不记录 key、签名原文或登录帧。
- **文档随代码**:同一改动更新 README、`TESTS.md` 与子系统页,行为变更记 [docs/changes.md](docs/changes.md);一件事实只有一个归属地,其余处链接;写当前状态与契约,不写历史与推理过程。
- **持久的决策理由写 Agent Note**:放 `.agents/notes/{proposed,implemented,rejected}/{class}/`,格式见 [.agents/notes/README.md](.agents/notes/README.md);机械改动不需要。

## 编辑本文件

`CLAUDE.md` 只是指向本文件的一行;改这里。每条规则自足、一到三行、链接其归属地;篇幅上限与计量口径见 [docs/AGENTS.md](docs/AGENTS.md#篇幅)。
