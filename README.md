# hft

我们自己的低延迟交易系统,首要服务 **CEX-CEX(ccex)策略**:首个策略是组合携带(一所永续空 × 另一所现货多,赚资金费与基差;同一交易所不同时做现货与永续),首批交易所 **Binance、OKX、Bitget**(各接现货 + USDT 本位永续,每个账户在一个交易所只用其中一个市场);跨所价差吃单、挂单 + 吃单对冲预留为可选策略包。链上执行(cdex)是可选扩展:cdex 继续在旧框架运行,迁移不急。交易主机上运行同一个 Rust 热进程二进制的两种实例——`prod`(交易所账户)与 `sim`(只含模拟账户)——按主机类的线程计划运行(升级主机上 prod 绑核忙等、sim 只用非隔离核;现有 2 核服务器上 feed 任务合并 + 空闲退让;单写者 sequencer + journal 即真相),慢路径与运维是 Python 冷进程。策略与引擎解耦:每个策略是 `strategies/` 下的独立 crate,只依赖策略 SDK,新增策略不改引擎。

**仓库状态:设计基线 v1 + 实现(M1 起)**:每个目录的 `README.md` 是该组件的实现规格,同目录的 `TESTS.md` 是它的验收用例(格式与修改流程见 [docs/tests](docs/tests/README.md),基线之后的每次行为变更记在 [docs/changes.md](docs/changes.md));M1 起代码按里程碑逐个组件落地。交付路径是 **dev2 → 统一升级主机 → 正式实盘**:core 组件在现有服务器(`dev2`)上完成开发测试、交易所测试网/模拟盘联调与模拟账户长跑(dev2 不放正式实盘账户),然后统一升级主机(`prod` 与 `sim` 两个实例同机,在那里做延迟验收;主机档位按 M3 末满订阅探针的实测选定,[latency-budget §7](docs/latency-budget.md#7-主机类与验收口径)),通过[上线门槛](docs/testing.md#8-上线门槛)、正式账户零成交冒烟与正式实例 Shadow 之后在新开的交易所子账户上开始正式实盘;没有小额实盘阶段,正式实盘的风险上限由运营在 `RiskParams` 设定(步骤见 [runbooks/launch](docs/runbooks/launch.md);里程碑见 [实施路线图](design/11-实施路线图.md))。现有系统(cdex、collector、现有 Telegram bot)照常运行、不做适配;cdex 迁移是可选扩展([runbooks/migration](docs/runbooks/migration.md))。策略评估用模拟账户(实时行情 + 模拟撮合),不做历史回测。开发与发布:本地只写代码,CI(负责人个人私有仓库 `Fiora-gwx/hft` 的免费托管 runner;aarch64 发布构建与验证包)是唯一构建环境;服务器只安装 CI 产出的制品、不编译,累计 fuzz 与 DST 在服务器上运行验证包([development](docs/development.md),含分钟预算)。

## 从哪读起

| 你要做什么 | 读 |
|---|---|
| 了解整体 | [docs/architecture.md](docs/architecture.md) → [docs/glossary.md](docs/glossary.md) |
| 写引擎代码 | [AGENTS.md](AGENTS.md)(常驻规则)→ [docs/hot-path-rules.md](docs/hot-path-rules.md) → 目标目录的 README |
| 写或改策略 | [strategies/AGENTS.md](strategies/AGENTS.md) → [hft-strategy-sdk](crates/hft-strategy-sdk/README.md) → [cookbook/adding-a-strategy](docs/cookbook/adding-a-strategy.md) |
| 理解事件与日志 | [docs/subsystems/event-model.md](docs/subsystems/event-model.md) → [journal-format.md](docs/subsystems/journal-format.md) |
| 接新交易所 / 新命令 / 新 crate | [docs/cookbook/](docs/cookbook) |
| 上线、部署与调优 | [runbooks/launch](docs/runbooks/launch.md)、[docs/runbooks/](docs/runbooks/README.md)、[deploy/](deploy/README.md) |
| 为什么这样设计 | [.agents/notes/](.agents/notes/README.md)、[../architecture-v2/](design/README.md) |
| 延迟目标与测量 | [docs/latency-budget.md](docs/latency-budget.md) |
| 测试与上线门槛 | [docs/testing.md](docs/testing.md) |
| 测试规格与改设计 | [docs/tests](docs/tests/README.md) → 目标目录的 `TESTS.md`;改规格按 [§7](docs/tests/README.md#7-修改流程设计基线-v1-之后) 并记 [docs/changes.md](docs/changes.md) |

## 组件一览

"范围"列取自各 README frontmatter 的 `scope`(唯一来源,`cargo xtask doc-check` 据此生成并比对本表):`core` = 首次上线(新子账户上线 ccex 策略)必须具备、必须通过上线门槛;`optional` = 可选扩展,README 写明触发条件。标"冻结"的组件 frontmatter 为 `status: frozen`,不随 core 规格同步。构建目标见 [docs/development.md#目标-cpu](docs/development.md#目标-cpu)。

| 目录 | tier | 范围 | 一句话 |
|---|---|---|---|
| [crates/hft-types](crates/hft-types/README.md) | pure | core | 定点(按资产指数)、ID、枚举、`RunMode`/`KillLevel`/`RejectBits` |
| [crates/hft-refdata](crates/hft-refdata/README.md) | pure | core | 合约/资产/费率/限速/符号映射、`VenueCaps` 交易所能力表、`AccountMeta`(账户类型、交易所 UID、绑定的策略实例);`EngineConfig` 模块 |
| [crates/hft-event](crates/hft-event/README.md) | pure | core | 记录头、事件体(含不透明 `Advice`、策略参数头、`ExternalFill`、`MarginReport`)、控制命令;schema 导出 |
| [crates/hft-ctl-proto](crates/hft-ctl-proto/README.md) | pure | core | ctl 帧编码与命令权限表 |
| [crates/hft-strategy-sdk](crates/hft-strategy-sdk/README.md) | pure | core | 策略唯一依赖的稳定接口:`Strategy`(快速部分)、`Model`(慢速部分)、参数 schema、只读视图、订单意图与目标意图、`strategy_set!` |
| [crates/hft-rt](crates/hft-rt/README.md) | hot | core | SPSC 环(进程内/共享内存)、TSC、绑核、大页、任务轮转与空闲策略 |
| [crates/hft-journal](crates/hft-journal/README.md) | hot | core | RAM 热段写者、读者、快照滚动、persist 复制(含段预创建与退役) |
| [crates/hft-book](crates/hft-book/README.md) | hot | core | 稠密数组 L2 簿(分档快照整替换;增量深度可选)+ BBO |
| [crates/hft-net](crates/hft-net/README.md) | hot | core | 非阻塞 TCP + rustls + WS + 时间戳 |
| [crates/hft-venue-proto](crates/hft-venue-proto/README.md) | pure | core | `VenueProtocol` trait 与纯协议类型 |
| [crates/hft-venue](crates/hft-venue/README.md) | hot | core | feed(BBO 与其他行情分任务)/gw 运行时、行情 REST 获取(`snap-fetch`)、账户查询(`acct-fetch`)、重同步、会话驱动 |
| [crates/venue-binance](crates/venue-binance/README.md) · [venue-okx](crates/venue-okx/README.md) · [venue-bitget](crates/venue-bitget/README.md) | pure | core | 首批三所的纯协议:现货 + USDT 永续(Binance 现货 SBE 为可选) |
| [crates/venue-gate](crates/venue-gate/README.md) · [bybit](crates/venue-bybit/README.md) · [kucoin](crates/venue-kucoin/README.md) | pure | optional | 其余 CEX(冻结) |
| [crates/venue-hyperliquid](crates/venue-hyperliquid/README.md) | pure | optional | 链上永续场所(冻结) |
| [crates/hft-oms](crates/hft-oms/README.md) | hot | core | 订单槽、状态机、LegLink、LegTracker |
| [crates/hft-risk](crates/hft-risk/README.md) | hot | core | 预交易检查、`classify`、`hft-risk::mode`(期望模式、cap、有效模式、MarginStress 分档、启动闸门、kill 投影) |
| [crates/hft-ledger](crates/hft-ledger/README.md) | pure | core | 仓位、现货库存、净量记账、权益视图数据、余额基线、PnL(热进程与 Python 同码) |
| [crates/hft-exec](crates/hft-exec/README.md) | hot | core | 执行层:目标跟踪、吃单切片、两腿 OnFill、Simultaneous;引擎默认减仓 |
| [crates/hft-strat-lib](crates/hft-strat-lib/README.md) | pure | core | 通用策略构件:价差、确认窗口、脏数据过滤、节流、分位统计、资金费统计、冲击估算 |
| [crates/hft-sequencer](crates/hft-sequencer/README.md) | hot | core | 主循环(对 `StrategySet` 泛型)、优先级、定时器、快照/恢复 |
| [crates/hft-ctl](crates/hft-ctl/README.md) | warm | core | ctl-gateway |
| [crates/hft-telemetry](crates/hft-telemetry/README.md) | cold | core | 延迟样本、直方图、按事件加权的离线延迟统计 |
| [crates/hft-sim](crates/hft-sim/README.md) | warm | core | 模拟撮合与模拟账户(实时行情纸面交易;journal 重演排障);队列模型为可选 feature |
| [crates/hft-py](crates/hft-py/README.md) | cold | core | PyO3 绑定(journal reader + ledger + SDK `params` 编解码,aarch64 wheel) |
| [crates/hft-testkit](crates/hft-testkit/README.md) | dev | core | 金样、属性测试、DST、共用撮合器、假交易所、测试策略 |
| [crates/hft-ext-proto](crates/hft-ext-proto/README.md) | pure | optional | 外部 venue(chain-core ↔ hft-core)帧;链上策略迁移时启用(冻结) |
| [crates/hft-chain-evm](crates/hft-chain-evm/README.md) | cold | optional | 链上执行(free profile、SwapGuard、证据规则);链上策略迁移时启用(冻结) |
| [crates/hft-amm](crates/hft-amm/README.md) | pure | optional | V2/V3 数学与路由(冻结) |
| [crates/hft-chain-solana](crates/hft-chain-solana/README.md) | hot | optional | Solana 执行(冻结) |
| [strategies/registry](strategies/registry/README.md) | hot | core | 唯一策略清单(`strategy_set!` 调用、`export_schemas()`);新增策略只改这里一行 |
| [strategies/portfolio-carry](strategies/portfolio-carry/README.md) | hot | core | 首个 ccex 策略:组合携带(打分 → 分配 → 目标 → 调仓,公式参数化) |
| [strategies/latency-probe](strategies/latency-probe/README.md) | hot | core | 延迟探针:只绑定非正式账户,在主网行情负载下以测试网下单产生延迟验收的订单类样本 |
| [strategies/cdex](strategies/cdex/README.md) · [cross-taker](strategies/cross-taker/README.md) · [maker-taker](strategies/maker-taker/README.md) | hot | optional | CEX-DEX;跨所价差吃单;挂单 + 吃单对冲 |
| [apps/](apps/AGENTS.md) | — | core / optional | core:hft-core、journal-archiver、watchtower、reconciler、kill-switch、replayer(verify + 模拟重演);optional(冻结):chain-core、chain-grpc-relay、mock-exchange |
| [python/pipeline](python/pipeline/README.md) · [ops_bot](python/ops_bot/README.md) · [treasury](python/treasury/README.md) | cold | core | pipeline 框架(各策略的插件在 `strategies/<name>/pipeline/`)→ 策略参数(经 hft-py)与预热数据,人工覆盖项按实例分键;Telegram 运维面(按 `from.id` 分角色,老板只读 + 紧急停止 `/stop`);跨所资金监控 + 划转建议 |
| [python/research](python/research/README.md) | cold | optional | 研究工具、模拟结果分析(不做回测) |
| [contracts/](contracts/README.md) | — | optional | SwapGuard 合约(链上策略迁移时启用;冻结) |
| [deploy/](deploy/README.md) · [benchmarks/](benchmarks/README.md) · [fixtures/](fixtures/README.md) · [tools/](tools/README.md) | — | core | 部署、资源护栏与调优;基准;金样;工具(含 `hft-cfg`、`hft-verify`) |
