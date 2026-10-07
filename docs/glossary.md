# 术语表

每个术语一行定义,并指向归属地。定义以归属地为准。

## 范围与组织

| 术语 | 定义 | 归属 |
|---|---|---|
| ccex / cdex | CEX-CEX 策略族(新框架首要服务的对象)/ CEX-DEX 策略族(链上腿;在旧框架运行,迁移到新框架是可选扩展) | [07 策略层](../design/07-策略层.md) |
| scope(core / optional) | README frontmatter 字段:`core` = 首次上线(新开交易所子账户上线 ccex 策略)必须具备、必须通过上线门槛;`optional` = 可选扩展,正文写触发条件,不写周次 | [crates/AGENTS.md](../crates/AGENTS.md#readme-模板) |
| status(spec / impl / frozen) | README frontmatter 字段:`spec` = 只有规格;`impl` = 已合入实现;`frozen` = 冻结的可选组件,不随 core 规格同步,解冻时先按 core 规格复核 | [crates/AGENTS.md](../crates/AGENTS.md#readme-模板) |
| 引擎 crate | `crates/*` 中的组件;不依赖任何策略包与 `hft-strategy-registry`(含 dev 依赖),README 与代码不出现策略族名或某个策略的规则数字 | [crates/AGENTS.md](../crates/AGENTS.md#依赖方向) |
| 策略包 | 一个策略一个 crate(`strategies/<name>`)= 快速部分(`Strategy`)+ 慢速部分(`Model`)+ 参数 schema,需要时另带 Python pipeline 插件(`strategies/<name>/pipeline/`);只依赖 `hft-strategy-sdk` 与 `hft-strat-lib`,不出现交易所名;静态编译进同一二进制,在 `strategies/registry` 清单里一行注册 | [strategies/AGENTS.md](../strategies/AGENTS.md) |
| 策略 SDK | `hft-strategy-sdk`:策略唯一依赖的稳定接口(trait、视图、意图、参数 schema、装配宏) | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| `StrategySet` / `strategy_set!` | 宏生成的静态分派策略枚举;唯一调用在 `strategies/registry`(`hft-strategy-registry`),hft-core 与 replayer 经它取得同一份;hft-testkit 不依赖它(需要真实策略的集成测试放 `strategies/registry/tests/` 或 `apps/hft-core/tests/`) | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| 交付路径 | `dev2`(开发测试、交易所测试网/模拟盘联调、模拟账户;不放正式实盘账户)→ 统一升级主机(`prod` 与 `sim` 同机,延迟验收)→ 正式账户零成交冒烟与正式实例 Shadow → 正式实盘;没有小额实盘阶段 | [runbooks/launch](runbooks/launch.md)、[11 实施路线图](../design/11-实施路线图.md) |
| M1.5 延迟探针 | M1 与 M2 之间的里程碑:按需租用 `c8g.4xlarge` 跑 hft-net + 单 venue BBO feed + 简化 sequencer + gw 的纵切片,拿到第一份 Graviton 上的延迟直方图,并做 IRQ 与时钟增测;不选档;参考机也从 M1.5 起按需租用 | [latency-budget §6](latency-budget.md#6-验收方法)、[11 实施路线图](../design/11-实施路线图.md)(增测清单) |
| M3 末(满订阅)探针 | M3 末的里程碑项:按需租用 `c8g.4xlarge`,按 `c16` 线程计划对 6 个产品线满订阅(另加压舱深度),开 raw journal 与 persist,测试网探针单;据此重定分段预算、选定统一升级主机类(`c16`/`c32`)并决定是否启用 IRQ 拆分(ENI-B2) | [11 实施路线图](../design/11-实施路线图.md)、[latency-budget §6、§7](latency-budget.md#7-主机类与验收口径) |
| 上线 | 新开交易所子账户的流程:开通与核验(无提现权限的 key、IP 白名单、`venue_uid` 登记、账户配置核验)→ 模拟账户运行 → 统一升级主机上的零成交冒烟(每个正式账户 × venue 一张冒烟单 `SmokeOrder`)与正式实例 Shadow 3–7 天 → 切 Live(风险上限由运营在 `RiskParams` 设定,初始上限与第一次放宽的条件写进上线签字记录);回退 = Drain | [runbooks/launch](runbooks/launch.md) |
| 上线门槛 | core 组件开始正式实盘前必须通过的测试与运行门槛(含三所测试网/模拟盘联调、模拟账户 ≥ 14 天与压力用例、升级主机上的延迟验收、解耦门槛;fuzz 与 DST 按服务器上运行验证包得到的累计量计,只卡正式实盘、不卡模拟) | [testing §8](testing.md#8-上线门槛) |
| 测试网/模拟盘联调 | 在 dev2 上起一个指向交易所测试网端点、只持测试网 key、refdata 只含测试网账户的 `prod` 角色实例,只在联调窗口运行;门槛 ⑥ 的行情也取测试网(`feed_env = "testnet"`);三所为 Binance 测试网、OKX 模拟盘、Bitget 模拟盘(可用性与 API 覆盖逐所待核实,方法:各所测试网联调,见 testing §8) | [testing §8](testing.md#8-上线门槛)、[hft-core](../apps/hft-core/README.md) |
| CI | 负责人个人账户下的私有仓库 `Fiora-gwx/hft`(免费每月分钟额度,与 Trading 仓库共享)上的托管 runner:PR 与 `main` 推送的质量门,解耦门单独的小工作流,按 tag 或手动触发在 `ubuntu-24.04-arm` 产出 aarch64 发布构建与验证包;不用自托管 runner;是唯一构建环境,服务器不编译 | [development](development.md)(工作流、剖面与分钟预算的唯一归属) |
| 验证包(`hft-verify`) | CI 在 aarch64 上构建的 `hft-verify-<ver>-aarch64.tar.zst`(fuzz 二进制、DST 运行器 `hft-dst`、驱动 `hft-verify`、初始语料),装到服务器上以最低优先级在预算内运行,累计门槛 ③④ 的账本在服务器上 | [tools](../tools/README.md)、[deploy](../deploy/README.md)、[testing](testing.md) |
| cdex 迁移 | 可选扩展:把 cdex 从旧框架切换到新框架(链上组件随之启用);切换而非并行 | [runbooks/migration](runbooks/migration.md) |
| collector | 现有的独立行情采集系统(`Trading` 仓库),服务研究;不并入新框架 | [journal-archiver](../apps/journal-archiver/README.md)(范围说明) |

## 标的、账户与能力

| 术语 | 定义 | 归属 |
|---|---|---|
| venue | 一个交易所的一个产品线(如 Binance USDT-M、Binance Spot、OKX 永续),有自己的连接、协议与限速;`VenueId(u8)` | [hft-types](../crates/hft-types/README.md) |
| 外部 venue | 不在 hft-core 进程内、经协议接入 sequencer 的执行场所(chain-core;optional) | [external-venue-protocol](subsystems/external-venue-protocol.md) |
| inst | 某 venue 上的一个可交易合约;`InstId(u32)` 全局唯一 | [hft-refdata](../crates/hft-refdata/README.md) |
| asset / 资产 | 跨 venue 的同一标的;决定定点指数 `AssetExp` | [hft-refdata](../crates/hft-refdata/README.md) |
| group | 一个策略实例管理的一组同资产的腿(至多 4 腿,如某所永续 + 某所现货);`GroupId(u32)` = `(StrategyId << 24) \| 实例内计数器`,唯一性由构造保证、永不复用(构造归 hft-types,续号与 `group_ctr_hwm` 归 tools hft-cfg,pipeline 链接);由策略参数里的 `GroupSpec` 声明,引擎据此做订阅、账户绑定、LegTracker 与记账归属 | [hft-types](../crates/hft-types/README.md)、[hft-event](../crates/hft-event/README.md)(`GroupSpec`)、[hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| `GROUP_NONE` / `GROUP_ALL` | `GROUP_NONE = GroupId(0)`:无组(外来成交不可归属、ledger 的 unassigned、定时器不适用、实例级或账户级载荷);`GROUP_ALL = GroupId(u32::MAX)`:命令作用域的"全部 group"(`*`);`Advice` 与 `StrategyCmd` 的 group 不得为 `GROUP_ALL` | [hft-types](../crates/hft-types/README.md)、[hft-event](../crates/hft-event/README.md) |
| account | 一个交易所(子)账户、模拟账户或链上钱包的执行身份;`AccountId(u8)` | [hft-types](../crates/hft-types/README.md)、[hft-refdata](../crates/hft-refdata/README.md)(`AccountMeta`) |
| 一账户一策略实例 | `AccountMeta.bound_instance`:一个账户只绑定一个策略实例;`SetParams` 对未绑定的账户返回 `InvalidArgs{AccountBound}` | [hft-refdata](../crates/hft-refdata/README.md)、[control-plane](subsystems/control-plane.md) |
| 同所单市场 | 一个账户在同一交易所只用一个市场(现货或永续):refdata 拒绝同一 `AccountId` 含两个同交易所的 `VenueId`(`SameExchangeMarkets`,Sim 账户同样);`SetParams` 拒绝两腿同交易所的 `GroupSpec`(`InvalidArgs{SameExchangeLegs}`);因此不建模余额域 | [hft-refdata](../crates/hft-refdata/README.md)、[defensive-patterns](defensive-patterns.md#同一交易所不同时做现货与永续) |
| `venue_uid` | 交易所账户 UID;启动闸门 `IDENTITY` 对每把交易 key(Ed25519 会话 key 与 HMAC REST key 分别)查询 UID 并与 `AccountMeta` 比对;kill-switch 自检与 reconciler 启动同样核验,不符 → 退出 2 + P0 | [hft-refdata](../crates/hft-refdata/README.md)、[hft-risk](../crates/hft-risk/README.md) |
| VenueCaps | 按 `VenueId` 的交易所能力表(一个交易所的现货与 USDT 永续是两个 `VenueId`,`VenueMeta.product` 给出 `MarketKind`):手续费扣币规则、reduceOnly、要求的持仓/保证金模式、资金费推送方式、保证金数据来源、自身强平识别、可忽略用户事件、能力位、测试网;`venue-*` 给默认值,refdata 可覆盖 | [hft-refdata](../crates/hft-refdata/README.md) |
| 模拟账户 | `AccountKind::Sim(SimAccountCfg)`:绑定某个真实 venue 的行情,订单由 sim-gw 模拟撮合,回报经 `rep-sim` 环进入 journal;策略以正常 `RunMode` 运行;不持交易所 key,reconciler 与 kill-switch 跳过,kill 与实盘一样经检查链生效。只在 `sim` 实例(dev2 与升级主机相同),每个研究中的策略预置 `sim_pool` 个模拟实例;与正式实盘同参数并行时用实盘结果校准模拟。模拟不是 `RunMode`,是账户类型 | [hft-sim](../crates/hft-sim/README.md) |
| 权益视图(`PortfolioView`) | 每账户每 venue 的现金(free/locked)、现货库存、永续持仓、保证金余额、维持保证金、可用保证金、保证金率、合计权益;引擎不计算"可用资金"或杠杆公式 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md)(视图)、[hft-ledger](../crates/hft-ledger/README.md)(数据) |
| 现货库存 | ledger 的一等对象:资产 → 数量,按 group 归属或 unassigned | [hft-ledger](../crates/hft-ledger/README.md) |
| 净量(`net_qty`) | 手续费以基础资产扣除时的成交净量;仓位、库存、对冲量一律按净量 | [hft-ledger](../crates/hft-ledger/README.md)(`FillDelta`) |
| 余额基线 | `SetBalanceBaseline` 登记的非策略资产;对账不把它当作差异 | [hft-ledger](../crates/hft-ledger/README.md)、[control-plane](subsystems/control-plane.md) |
| 构建目标 | 唯一发布目标 `aarch64-unknown-linux-gnu` + `target-cpu=neoverse-v1`(dev2 与升级主机同一二进制);x86 只作为可选的 `metal` 剖面 | [development](development.md#目标-cpu) |

## 线程与数据面

| 术语 | 定义 | 归属 |
|---|---|---|
| seq | sequencer 赋予每条事件的全局单调序号;回放顺序的唯一依据 | [event-model](subsystems/event-model.md) |
| 输入环优先级 | sequencer 取环顺序 `ctl > rep(实盘回报)> bbo > advice > md > rep-sim`;md、bbo 与 advice 逐条处理,每条 md 之后查看 ctl、实盘 rep、bbo 的环头,每条 bbo 或 advice 之后查看 ctl 与实盘 rep 的环头,非空先处理它们;每环每轮有上限(md 另有时间片,`rep-sim` 为 `sim_max_per_round`);同一事件派发给多个策略实例时,绑定实盘账户的实例在前 | [hft-sequencer](../crates/hft-sequencer/README.md)、[architecture §3](architecture.md#3-hft-core-线程) |
| hft-core 实例(`instance`) | 同一二进制的两种实例:`prod`(只含交易所账户,不含 Sim 账户;dev2 上只在测试网联调窗口以指向测试网、只持测试网 key 的形态运行)/ `sim`(只含 Sim 账户、不加载交易所 key、自己的行情连接与 journal,不交易任何交易所账户);journal 实例名只允许 `"prod"`、`"sim"` | [hft-core](../apps/hft-core/README.md)、[hft-journal](../crates/hft-journal/README.md) |
| sim 与 prod 同机 | 统一升级主机上两个实例同机运行:sim 只用非隔离核,永不占 prod 的热核;独立 `hft-sim.slice`(CPU、内存上限);独立 journal 与冷盘目录;独立网卡 ENI-C(IRQ 钉在 sim 的核上)与出口 IP(ENI-C 私有 IP + EIP),不占 prod 的收包队列与按 IP 限速 | [hft-core](../apps/hft-core/README.md)、[deploy](../deploy/README.md) |
| tsc | CPU 时间戳计数器读数(aarch64 为 `CNTVCT_EL0`,x86 为 invariant TSC);热路径唯一的时间来源;**只在单次启动内可比较**,持久状态不存它(例外:hot-path-rules D6 登记的行,恢复时按 `recovery_end` 平移或作废) | [hft-rt](../crates/hft-rt/README.md)、[defensive-patterns](defensive-patterns.md#tsc-不跨开机比较) |
| 引擎时间(`engine_ns`) | SDK `Now.engine_ns`:跨重启连续、停机不计的时间,sequencer 由 journal 记录的运行区间累加得到(回放确定);策略、`hft-strat-lib`、ledger、risk 持久状态里的时刻与期限一律存它 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md)、[hft-sequencer](../crates/hft-sequencer/README.md)、[hot-path-rules D6](hot-path-rules.md#确定性d) |
| `boot_seq` | SDK `Now.boot_seq`:本次启动 `EpochStart` 的 seq;`basis_seq < boot_seq` 的 Advice 由策略视为跨启动过期 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| 五个时间戳 | `ex_ts`、`ex_send`、`hw_rx`、`rx_tsc`、`proc_tsc`(= 记录头 `tsc`);另有常态记录的 `sw_rx` | [latency-budget](latency-budget.md) |
| `rep_to_order` | 回报路径的主机内分段:由某条回报帧(Ack/Fill/Cancel 等)触发的下一笔订单操作的 `sent_tsc` − 该回报帧的 `hw_rx`;上线门槛 ⑩ 的硬项之一 | [latency-budget §2](latency-budget.md#2-分段名全仓唯一) |
| `hedge_host` | `rep_to_order` 中 LegLink 第二腿(`link_role = Second`,不含 `SecondRetry`)的样本:第二腿 `sent_tsc` − 触发它的第一腿成交帧的 `hw_rx`;只记录 | [latency-budget §2](latency-budget.md#2-分段名全仓唯一) |
| feed 任务 / gw 任务 | 每 venue 的行情任务 / 下单任务。feed 按路由拆成 `feed-bbo:<venue>`(只含 BBO 路由)与 `feed-md:<venue>`(成交、标记价/指数价、资金费、分档深度 TopN;可选的增量深度也在这里),升级主机类上两者不同线程,`feed-bbo` 可跨 venue 合并;`dev2`/`c4` 可配成一个 `routes = All` 的任务。每个下单 venue 的 gw 在所有主机类上独占线程(升级主机上另独占隔离核) | [hft-venue](../crates/hft-venue/README.md)、[hot-path-rules T8](hot-path-rules.md#线程t) |
| `snap-fetch` | warm 任务(每实例一个):行情 REST 的请求、读取、解密与解析——BBO 补帧(经 `snap-<venue>-bbo` 环交回 `feed-bbo`),启用增量深度时另有 L2 快照(经 `snap-<venue>` 环交回 `feed-md`);热线程不做行情 REST 的系统调用与解析 | [hft-venue](../crates/hft-venue/README.md) |
| 报价可用(连接健康) | 该 inst 的 BBO 成员连接中至少一条 Up、未在重同步、`now − conn_live_tsc ≤ conn_liveness_ms`;`BboView.conn_live_tsc` = Up 且未在重同步的成员连接的 `live_tsc`(该连接最近一次活性证据的时刻;哪些记录算活性证据归 [event-model](subsystems/event-model.md#关键事件体),维护规则归 hft-sequencer 连接活性)的最大值,hft-venue 每 `bbo_ping_ms` 发 WS ping 保证有回帧;报价取最新一条 Bbo,盘口不变不算过期;对冲腿、风险递减单、自动平仓、L3 减仓与第二腿检查都用它 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md)(`BboView`)、[hft-sequencer](../crates/hft-sequencer/README.md)、[defensive-patterns](defensive-patterns.md#安静--过期报价可用性看连接健康) |
| TopN(分档快照流) | 交易所按固定节拍推送的前 N 档快照流(如 Binance `depth{5,10,20}@100ms`、OKX `books5`、Bitget `books5/books15`;各所待核实,方法:文档 + 金样):路由 `RouteKind::DepthTopN`,产出 `L2Snapshot{origin = TopN}`,簿整替换、无序号链、无 REST 重同步;首次上线需要深度时优先用它,全量增量深度是可选扩展 | [hft-venue](../crates/hft-venue/README.md)、[hft-book](../crates/hft-book/README.md) |
| `acct-fetch` | warm 任务(每个有 `gw:` 的实例一个):账户查询(保证金轮询、账户配置核验)的签名 REST 请求、读取与解码;gw 经 `acctreq-<venue>` 环提交请求,结果经 `acct-<venue>` 环交回 gw,gw 线程不做大块解码 | [hft-venue](../crates/hft-venue/README.md)、[hft-core 线程计划](../apps/hft-core/README.md#线程计划) |
| VenueProtocol | 纯交易所协议接口(分派、解码、帧模板、登录、重同步规则、限速表);`venue-<x>` 实现它 | [hft-venue-proto](../crates/hft-venue-proto/README.md) |
| ext 任务 | hft-core 内与外部 venue(chain-core)会话的任务;optional | [external-venue-protocol](subsystems/external-venue-protocol.md) |
| conn-builder | warm 任务,负责握手/登录(建连时显式关闭 socket 的 busy poll),把就绪连接交给 feed/gw | [hft-net](../crates/hft-net/README.md) |
| advisor | warm 线程,运行各策略的 `Model`(慢速部分),输出 `Advice` 事件;按 `advisor.model_checkpoint_ms` 分块写 Model 检查点;内存按各策略声明之和受 `advisor.mem_budget_mib` 约束;滞后(按 `engine_ns` 差)超过阈值写 `AdvisorLag`(定义归 hft-event) | [hft-core 线程计划](../apps/hft-core/README.md#线程计划)、[hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| sim-gw | warm 任务(只在 sim 实例),处理模拟账户的 `OutMsg`:按 hft-sim 撮合,回报写回报环;不占热核 | [hft-core 线程计划](../apps/hft-core/README.md#线程计划)、[hft-sim](../crates/hft-sim/README.md) |
| ctl-gateway | 冷进程命令的唯一入口(warm `poll_once` 任务):传输、鉴权、角色上限后写控制环;不复制 sequencer 状态,确认与状态版本由 sequencer 复核 | [hft-ctl](../crates/hft-ctl/README.md);帧与权限表在 [hft-ctl-proto](../crates/hft-ctl-proto/README.md) |
| raw journal / seq journal / chain journal | 原始报文日志(feed/gw 写)/ 归一化事件与决策日志(sequencer 写)/ 链上日志(chain-core 写;optional) | [journal-format](subsystems/journal-format.md) |
| 热段 / 冷副本 | journal 在 RAM(hugetlbfs 或 tmpfs)中的当前段 / persist 线程复制到盘上的文件;段预创建与退役也在 persist 线程;段大小按主机类由 hft-core 给出;启动空间检查扣除本实例已有的段 | [hft-journal](../crates/hft-journal/README.md)、[hft-core](../apps/hft-core/README.md) |
| 线程计划 / `[[threads]]` | 配置驱动的线程布局:每个线程 = `{name, core, idle, tasks}`,同一线程按固定顺序轮转多个任务的 `poll_once`/`step`;deploy 的隔离核列表由它生成 | [hft-core](../apps/hft-core/README.md#线程计划) |
| 主机类(`host.class`) | `dev2`(现有 2 核,与 cdex 正式账户共机,在资源护栏内运行,只做功能、正确性、测试网联调与模拟账户,不放正式实盘账户)/ `c4`(联调档)/ `c8`(只承载 prod)/ `c16`(条件候选,首批下单 venue ≤ 4)/ `c32`(`c8g.8xlarge`,prod + sim 同机的默认候选)/ `metal`(optional);统一升级主机类按 M3 末满订阅探针选定(默认 `c32`;`c16` 只在探针达标时保留),延迟验收与正式实盘在那里;升级只换配置与调优,不重新构建 | [hft-core](../apps/hft-core/README.md#线程计划)、[latency-budget §7](latency-budget.md#7-主机类与验收口径) |
| 专用 IRQ 核 | 升级主机类(`c8` 及以上)上独占交易网卡(ENI-B)全部队列 IRQ 与软中断的核(核 1,列入 `rcu_nocbs`),不放任何线程;收包只用中断驱动,busy poll 全关,热线程只做非阻塞 `recvmsg`;`c32` 另预留核 28 与第二块交易网卡 ENI-B2 承载批量流(默认不启用) | [hot-path-rules T12](hot-path-rules.md#线程t)、[deploy](../deploy/README.md) |
| dev2 护栏 | dev2 上 hft 进程的资源上限(CPU、内存、禁 swap、优先级、不忙等、journal 段上限):hft-core 在 `hft-core.slice`,批处理在根级 `hftbatch.slice`(`CPUWeight=idle`);守护按资源信号(读旧系统所在的 `system.slice`)判断,只在 hft 自身用量明显时停 hft,否则只告警(信号、阈值与 sim 的自动重启归 hft-core),cdex 单元重启或状态文件过期只告警;cdex 优先 | [hft-core](../apps/hft-core/README.md)、[deploy](../deploy/README.md#dev2-护栏) |
| `IdlePolicy` | 线程空闲后的退让策略:`Spin` / `SpinThenPark{spin_us, park_us}` / `Park{park_us}`;只在全部任务连续空闲后生效;`Spin` 生产者不唤醒 park 的消费者 | [hft-rt](../crates/hft-rt/README.md#线程计划的执行任务轮转与空闲策略) |
| kill 字(`KillWord`) | 每个 gw 一个 `CachePadded<AtomicU32>`,位布局 `KillBits` 归 hft-types;sequencer 写、gw 发送前读,判定含按账户 HARD 位;启动自检经真实 store/load;唯一允许的跨线程共享可变量 | [hft-types](../crates/hft-types/README.md)、[hft-rt](../crates/hft-rt/README.md)、[hot-path-rules T3](hot-path-rules.md#线程t) |
| EngineConfig | 引擎容量与行为配置,按主机类给出,启动时记为 `ConfigBlob{Engine}`;类型在 hft-refdata 的 `engine` 模块,但不属于 refdata blob,变更需重启 | [architecture §10](architecture.md#10-核心类型归属)、[hft-core](../apps/hft-core/README.md) |
| 容量名 | `max_instances`(策略实例数上限)、`max_groups_per_instance`(SDK,每个策略实例的 group 上限)、`max_cells_total`(sequencer 与执行层,全进程 (account, group) cell 总数;`ModeOutcome` 按它预分配,不会溢出);取值唯一归属 hft-core(按主机类),其他处链接;投递给策略的通知超出 sequencer 通知队列时不丢弃,经续推定时器在后续轮次处理 | [hft-core](../apps/hft-core/README.md)、[hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| hft-cfg | 手写 TOML → refdata / 策略参数(按导出的 schema JSON,经 SDK `params` 编码)/ `RiskParams` / `EngineConfig` blob + 差异报告;refdata 的唯一来源 | [tools](../tools/README.md#hft-cfg) |
| schema 导出 | `hft-core schemas --out <dir>` → 每个策略一份 `<kind_id>-<name>.schema.json`(参数字段、Note 与报表描述符)+ `index.json`(数组,格式归 registry),随发布制品附带;hft-cfg、hft-py、pipeline 只读这些 JSON,不链接策略包 | [registry](../strategies/registry/README.md)、[tools](../tools/README.md) |

## 策略、意图与执行

| 术语 | 定义 | 归属 |
|---|---|---|
| 快速部分(`Strategy`) | 策略在 sequencer 线程内的状态机;唯一热路径入口 `on_event`,不分配、不阻塞、不读系统时钟、无 f64 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| 慢速部分(`Model`) | 同一策略包在 advisor 线程运行的部分(打分、分配、分位、资金费统计等);允许 f64 与分配,必须确定(只用事件时间,无 I/O、无系统时钟、无 HashMap 迭代);输出 `Advice`;声明自己的内存与快照上界 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| Model 检查点 | advisor 为每个策略实例定期写的 Model 状态文件(冷盘,journal 之外),只用于重启免预热;分两节:路径状态节(先写)与原始样本节(布局由 `ModelLayoutDesc` + `LAYOUT_VERSION` 管);键只含 `kind_id`、`Model::LAYOUT_VERSION`、`schema.id`、`strategy_instance`、`last_seq`,不随参数、refdata、发版失效;缺失或不符 → Model 从空起步 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md)、[hft-core](../apps/hft-core/README.md) |
| 路径状态节 | Model 检查点里路径依赖的派生状态(分位带及其时刻、平滑窗、上次发送的载荷摘要等),自带 `Model::PATH_VERSION`,经 `Model::snapshot_path`/`restore_path` 读写;版本不符只丢这一节、按原始样本重算并写 `Note{ModelPathReset}`;有它时"检查点 → 恢复 → 续算"与连续运行逐字节相同 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| 预热注入(`InjectWarmup`) | 控制命令 `InjectWarmup{instance, local_kind, chunk_no, last, payload ≤ 4096 B}`(主体 Trader 或 pipeline),作为 Input 记 journal,路由到该实例的 Model(`EventRef::Warmup`);载荷由策略定义,每块自身完整,生产者是该策略的 pipeline 插件;回放确定 | [control-plane](subsystems/control-plane.md)、[hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| `StrategyCmd` | 通用的策略人工命令 `StrategyCmd{instance, group, payload ≤ 256 B}`(SDK `EventRef::Manual`),载荷含义由策略定义;发送方可声明 `reduce_only`(中性命令,处理期间该实例的 Open 意图与目标增加被拒);引擎不为某个策略加命令 | [control-plane](subsystems/control-plane.md)、[hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| Note 与报表描述符 | `StrategyDesc.notes`(`NoteDesc`:码、名、级别、`v[]` 含义)与 `report`(`ReportFieldDesc`:名、单位、聚合方式);随 schema JSON 导出,watchtower 与 journal-archiver 据此通用汇总策略 Note、生成实盘与模拟日报 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| Advice | 策略 `Model` 输出的变长不透明载荷(`AdviceHdr` + ≤ 4096 B),带 `basis_seq`/`basis_tsc`(所依据的最后输入),按 strategy 路由,进 journal;group 只能是 `GROUP_NONE` 或该实例当前参数里的 group;过期由策略判定——同一次启动内按 `basis_tsc`,跨启动按 `basis_seq < Now.boot_seq`,sequencer 不加标志;引擎不解释,回放读日志不重算 | [hft-event](../crates/hft-event/README.md) |
| 参数 schema | 每个策略自带的参数描述(`ParamSchemaDesc`:字段、单位、范围、默认值、说明);配置工具、控制协议、ops_bot 按 schema 通用处理,新增策略不改它们;规范编解码只有 SDK `params` 一份(hft-cfg、hft-py 包装,Python 不另写编码器) | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md);线上头在 [hft-event](../crates/hft-event/README.md) |
| 人工覆盖项(overrides) | pipeline 合并进策略参数的人工修改,按实例分键 `overrides/<instance>.toml`;校准写共享覆盖项必须显式 `scope = pair`(默认 `instance`),模拟实例上的试验不流到实盘实例;pipeline 主体的 `SetParams`(含新增 group、`open_allowed` 0→1)在 Live 账户上也是中性命令;ops_bot `/params <实例> set` 只写该实例,作用于 Live 实例时需确认 | [pipeline](../python/pipeline/README.md)、[control-plane](subsystems/control-plane.md)、[defensive-patterns](defensive-patterns.md#模拟上的试验不得流到实盘覆盖项按实例分键) |
| intent / 意图 | 策略经 `Ctx` 输出的订单意图(`OrderIntent`)或目标意图(`TargetIntent`);经执行层、风控与 OMS 才成为订单 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| 目标意图(`TargetIntent`) | 策略给出某 (account, group) 各腿的目标净量 + 原因(含 `Exit`、`MarginStress` 等)、紧急度、不交易带与执行方式(`ExecSpec`);在被替换或清除前一直有效 | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| 单 group 强制退出 | 策略层的退出通道(首个策略 portfolio-carry 提供):per-group 参数 `force_exit` 或 `StrategyCmd` 命令 `exit`(`reduce_only = true`)→ 该 group 目标设 0、`reason = Exit`;实例级 `exit_all` 对全部 group 执行同一动作;策略的成本门只约束 `Rebalance`,`Exit`/`Hard`/`MarginStress` 不受它约束 | [strategies/portfolio-carry](../strategies/portfolio-carry/README.md) |
| 执行层 | `hft-exec`:把目标变成订单(吃单切片、两腿 OnFill、Simultaneous;MakerFirst 预留),按退出原因选腿顺序,两腿数量按 lot 对齐,并在 MarginStress L3 或外部成交未处理时默认减仓(人工外来成交除外)、对 Broken 做一次有界平仓;L3 与 flatten 在另一腿报价不可用时走串行的 `SerialFallback`;引擎单限价按 `cap_cross_px` 钳到风控上限;每次决定写 `ExecDecision` | [hft-exec](../crates/hft-exec/README.md) |
| lot 对齐 | 两腿链接的切片与第一腿数量按两腿 lot 的较粗者(一方整除另一方时)取整,使第二腿数量 ≥ 其 `min_qty` 且为其 lot 的整数倍;不能整除时取最小公倍数,其名义 > `max_chunk_notional` → `SetParams` 被拒(`InvalidArgs{LotMismatch}`) | [hft-exec](../crates/hft-exec/README.md)、[control-plane](subsystems/control-plane.md)、[defensive-patterns](defensive-patterns.md#两腿数量按-lot-对齐残余不能对冲就不开) |
| `flatten_lock` | `Kill{Account, flatten}` 触发 `engine_flatten` 时置的账户级锁:直到 `Resume{Kill}` 前,该账户只接受比当前更接近 0 的目标与 Reduce/Flatten 意图 | [hft-exec](../crates/hft-exec/README.md)、[hft-risk](../crates/hft-risk/README.md)、[defensive-patterns](defensive-patterns.md#清仓锁flatten-之后只接受更接近-0-的目标) |
| slot / 订单槽 | OMS 预分配数组中的一个订单记录;`(epoch, slot, gen)` 编进 client order id | [hft-oms](../crates/hft-oms/README.md) |
| LegLink | 把多腿绑成一组、让 OMS 在第一腿成交事件内发出第二腿的描述 | [hft-oms](../crates/hft-oms/README.md) |
| LegTracker | 每 (account, group) 的腿状态:各腿净量、在途、残余与 naked;在途分为立即类(`inflight_immediate`:IOC/FOK、Ack 前的 GTC/PostOnly、Unknown)与挂单类(`inflight_resting`:已 Ack 的 GTC/PostOnly),划分以 hft-oms 为准;裸量与补腿抑制只用立即类 | [hft-oms](../crates/hft-oms/README.md) |
| 第二腿检查 | 两腿链接在第一腿发出前对第二腿的检查:第二腿 venue 的报价按连接健康可用(否则 `LINK_STALE`);可用资金或保证金 ≥ 所需 × (1 + `funds_buffer`),永续腿按 `AccountConfigReport` 读到的账户实际杠杆折算(否则 `LINK_FUNDS`);第二腿数量不超过当前裸量 | [hft-risk](../crates/hft-risk/README.md)、[defensive-patterns](defensive-patterns.md#第二腿先检查再开第一腿) |
| Unknown | 订单或链上交易的显式状态:发出后结果未知;只能由证据收敛,不能由传输错误推断 | [defensive-patterns](defensive-patterns.md) |
| ExternalFill | 非本系统下单产生的成交(强平、ADL、人工、交割、未知):入 ledger、更新 LegTracker、告警、通知策略;未处理时执行层默认平掉多出的一腿;`Manual`(人工网页操作)不做默认处理,并使该 cell 进入 `manual_hold` | [hft-event](../crates/hft-event/README.md)、[hft-exec](../crates/hft-exec/README.md) |
| `manual_hold` | 执行层 cell 标志:收到 `ExternalFill{Manual}` 置位(例外归 hft-exec),写 `Note{ExecManualHold}`;置位期间抑制补腿(HedgeFix)、ResidualTrim、外来成交默认处理与自动有界平仓的计时;解除条件归 hft-exec(策略 `clear_target` 不解除) | [hft-exec](../crates/hft-exec/README.md) |
| MarginReport | venue gw 按 `VenueCaps.margin_data` 推送或轮询得到的保证金余额、维持保证金、保证金率、强平价、ADL 档 | [hft-event](../crates/hft-event/README.md) |
| chain-first | CEX-DEX 执行顺序:先上链,拿到包含证据再在 CEX 对冲(optional) | [strategies/cdex](../strategies/cdex/README.md) |
| L0 / L1(链上定价) | 可选扩展的链上定价两级:L0 内存池状态闭式计算;L1 revm 精确模拟 | [hft-chain-evm](../crates/hft-chain-evm/README.md) |
| profile(链上) | 链上发送方式:`free`(免费公共 RPC)/ `builder`(付费);均为 optional | [hft-chain-evm](../crates/hft-chain-evm/README.md)、[chain-core](../apps/chain-core/README.md) |

## 模式、风控与人工干预

| 术语 | 定义 | 归属 |
|---|---|---|
| RunMode | `Shadow` / `CloseOnly` / `Live`,按策略 × group × 账户设置;`Live` 遵守 `open_allowed`;枚举与线上编码在 hft-types,模式计算在 `hft-risk::mode`,策略意图在各模式下的处理表在 SDK | [hft-types](../crates/hft-types/README.md)、[hft-risk](../crates/hft-risk/README.md)、[hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| 期望模式 | 操作者要的 `RunMode`,存在模式表里,只由 `SetMode` 写;不被任何自动降级覆写 | [hft-risk](../crates/hft-risk/README.md) |
| cap | 压低有效模式的独立记录 `(source, scope, set_seq, params)`:来源包括 Kill(+HARD)、`DRAINING`、Recovery、DailyLoss、Broken、HedgeFailure、UnknownTimeout、ClockBad、MarginStress、Storage、Delisting 等(完整清单以 control-plane 层级表为准);互不覆盖,`Resume{source, scope}` 只删完全匹配的一条 | [hft-risk](../crates/hft-risk/README.md);层级表在 [control-plane](subsystems/control-plane.md) |
| 有效模式 | 实际生效的模式:期望为 `Shadow` 时取 `Shadow`,否则取 min(期望, 全部生效 cap);`RISK_REDUCING` 单只受 HARD 约束 | [hft-risk](../crates/hft-risk/README.md) |
| MarginStress | 按 (account, venue) 的保证金分档 cap,度量是到强平的距离 `liq_dist`(永续持仓 `\|liq_px − mark\| / mark` 的最小值,交易所 `liq_px` 与 ledger 估算取更保守者):L1 `liq_dist ≤ liq_dist_block_open` 禁开;L2 `≤ liq_dist_reduce` 通知策略在截止前减仓;L3 `≤ liq_dist_force` 或截止已过 → 执行层按步减永续(含 `GROUP_NONE` 的永续持仓);回退带 `liq_dist_hyst`;保证金数据过期视为 L1;维持/保证金比率只作交叉校验(超过校验阈值至少 L2);实际杠杆超过 `max_perp_leverage_x100` → `ACCOUNT_CFG` 不符;默认值归 hft-risk(待标定) | [hft-risk](../crates/hft-risk/README.md)、[defensive-patterns](defensive-patterns.md#保证金分档降级截止后引擎减仓) |
| 启动闸门 | 冷启动后放行交易前必须全部满足的位(如 `RECONCILED`、`ACCOUNT_CFG`、`IDENTITY`);资金费差异不进闸门 | [hft-risk](../crates/hft-risk/README.md) |
| ShadowDeferred | `SetMode→Shadow` 时该 cell 仍有仓位/在途/LegTracker 残余:有效模式先为 `CloseOnly`(平仓照常),归零后自动落到 `Shadow` | [control-plane](subsystems/control-plane.md)、[defensive-patterns](defensive-patterns.md#shadow-不能冻结出场) |
| Recovery(cap) | 冷启动恢复后加在每个账户上的锁存 cap;闸门放行后由 `Resume{Recovery, account}` 解除;干净 Drain 重启在闸门通过后自动解除,出现 `tail_lost` 或恢复不一致时不自动解除;只含 Sim 账户的实例另有自动解除条件(归 hft-risk);任何 Recovery 解除都不判 Broken | [hft-sequencer](../crates/hft-sequencer/README.md)、[hft-risk](../crates/hft-risk/README.md) |
| 干净 Drain | 同版本、journal 尾部为空、无 `Unknown`/Broken 的 Drain 重启;Drain 在 `exec.drain_ready()`(全部链接结束、IOC 终态、重试计时器清空,且无未到达的 MarginStress/Hard 引擎目标;受 `drain_timeout_ms` 约束)满足后才退出,条件归 hft-sequencer 与 hft-exec | [hft-sequencer](../crates/hft-sequencer/README.md) |
| kill 级别 | `Account` / `Venue` / `Global`,另有 HARD 位(可按账户或全局;连 `RISK_REDUCING` 单也拦;解除 HARD 时裸量超过补腿阈值的 group 按 Broken 处置(`resolve_broken`,判定归 hft-exec):人工 `HedgeFix`,或到期后的自动有界平仓);策略层面的停止用 `SetMode` | [hft-risk](../crates/hft-risk/README.md)、[hft-exec](../crates/hft-exec/README.md) |
| 冒烟单(`SmokeOrder`) | Admin 专用控制命令(需确认):在某 (account, venue, inst) 上挂一张远离对手价的最小名义 PostOnly 单,到 `ttl_ms` 由 OMS 自动撤单;Shadow/CloseOnly 下允许、HARD 下拒绝,走完整风控链,不计入任何策略;用于上线零成交冒烟与每月正式账户冒烟 | [control-plane](subsystems/control-plane.md)、[hft-oms](../crates/hft-oms/README.md) |
| AdoptExchange | 升级类命令:把一次对账差(`report_id`)写成 ledger 调整(class = `Adjust`),可采纳仓位与余额(现货库存)条目并可带入场价;用于人工在交易所网页操作之后,使人工动作进入 journal | [control-plane](subsystems/control-plane.md)、[hft-ledger](../crates/hft-ledger/README.md) |
| 死手(dead man's switch) | 交易所侧定时撤单;没有该接口的所由 kill-switch 兜底 | [kill-switch](../apps/kill-switch/README.md) |
| kill-switch | 独立的最小进程:hft-core 心跳消失时,用自己的 REST 凭证对每个 Live 账户在其交易的每个 venue 撤全部挂单(有交易所死手的 venue 也并行再撤一次);另有只接受 ops_bot uid 的本地触发套接字(撤单,或 `stop_core`:撤单、写持久停机标记后经 polkit 停 `hft-core@prod`);只撤单、不平仓 | [kill-switch](../apps/kill-switch/README.md) |
| MirrorRestore | 恢复并 `SetEpoch` 之后,sequencer 为每个非终态槽向 gw 下发的命令,让 gw 重建私有映射并查询;无法编码查询的槽保持 `Unknown` | [hft-sequencer](../crates/hft-sequencer/README.md) |
| FeeOnly | 只带手续费的回报种类:任何状态下只把费用交给 ledger(净量补差),不迁移槽状态、不请求对账 | [hft-oms](../crates/hft-oms/README.md) |
| HedgeRetry | OMS 定时器:第二腿每次失败后按 max(最小间隔, `backoff_until`) 重试;门控拒绝不计次但累计时长,超过截止 → Broken cap + P0 | [hft-oms](../crates/hft-oms/README.md) |
| Broken(断腿) | 对冲到 `hedge_deadline_ms`(只按引擎运行时间累计)仍未完成 → Broken cap + P0;持续 `broken_auto_fix_ms` 且无人工 `HedgeFix` → 执行层自动做**一次可执行的尝试**,有上限地"平掉多出的一腿"(何时开始、何时重试、何时判失败归 hft-exec;`Note{ExecAutoFixDone}` P1 / `ExecAutoFixFailed` P0);解除按**已成交**裸量不高于尘埃线且无 Pending*/Unknown/在途判定;残余低于最小下单量无法平掉时由 Admin 以 `WriteOffResidual` 核销;账户级 `HedgeFailure` 只有在该账户已无 Broken 时才能 `Resume`;MarginStress L3 与 flatten 不跳过 Broken;HARD kill 下不自动处理 | [hft-exec](../crates/hft-exec/README.md)、[hft-risk](../crates/hft-risk/README.md)、[defensive-patterns](defensive-patterns.md#裸腿有界断腿自动平仓l3-与-flatten-不跳过) |
| 资金费对账 | 交易所资金费流水与账本的差异只告警(watchtower,超过 `funding_diff_alert` → P1),不进启动闸门、不触发 cap;分级前余额差先扣除同组报告的资金费条目;ledger 对超过 `funding_recon_lag_ms` 的差额自动入 `AdjustDelta::Funding` 并写 `Note{FundingCorrected}`(P1),`NoHistory` 的条目在窗口结束后按交易所金额入账 | [hft-ledger](../crates/hft-ledger/README.md)、[reconciler](../apps/reconciler/README.md) |
| ops_bot 角色 | `Admin` / `Trader` / `Viewer`,按 Telegram `from.id` 分配;老板为 `Viewer`:只读,唯一写权限是紧急停止 `/stop [account]` | [control-plane](subsystems/control-plane.md) |
| 紧急停止(`/stop`) | 非 HARD、不带 flatten 的 `Kill{Global \| Account}`:禁止新开仓、撤销挂单,已成交第一腿的对冲与平仓照常(`RISK_REDUCING` 放行);Viewer 的确认绑定预览的 `cmd_hash` + TTL 60 s,不比对 `ctl_state_version`;不能 `Resume`;hft-core 心跳过期或 ctl-gateway 不可达时 ops_bot 经 kill-switch 本地触发套接字兜底(条件、二次确认与告警归 ops_bot):`stop_core` 撤单并 Drain `hft-core@prod`,停机前写持久停机标记,hft-core 下次启动据此写一条 `Kill` Input,由交易员 `/resume kill` 解除;任一账户 MarginStress ≥ L2、有 Broken 或在途链接时兜底只撤单,停引擎需再次确认 | [control-plane](subsystems/control-plane.md)、[ops_bot](../python/ops_bot/README.md)、[defensive-patterns](defensive-patterns.md#紧急停止在引擎不健康时也要可达) |
| 机外看门狗 | dev2 上的 `hft-offbox-watchdog.service`:每 15 s 查升级主机 watchtower 的 `/healthz`,连续 60 s 不可达或不健康 → 独立 Telegram token 与电话类通道告警 | [watchtower](../apps/watchtower/README.md)、[deploy](../deploy/README.md) |

## 测试与回放

| 术语 | 定义 | 归属 |
|---|---|---|
| 验证回放 | 喂 seq journal 的全部 Input,要求 Output 与实盘逐字节一致;用于正确性,不评估收益 | [replayer](../apps/replayer/README.md) |
| 模拟重演 | `replayer sim`:把录下的 journal 行情喂给模拟网关与同一策略,用于排障与复现,不评估收益;默认 `--advice recompute`(按重演状态重算 Advice);`--ab` 只输出两组参数在同一段 journal 上的相对差值(默认关闭,不是收益估计,不得用于定仓) | [replayer](../apps/replayer/README.md)、[hft-sim](../crates/hft-sim/README.md) |
| DST | 确定性模拟测试:种子驱动的假交易所 + 故障注入跑 sequencer/执行层/OMS/风控;CI 每夜只跑回归冒烟,正式实盘前的门槛按服务器上验证包运行的累计新种子数计(只累计相关源路径与候选版本相同的运行);只有违反使计数清零,漏跑不清零 | [testing §6.1](testing.md#61-服务器上的累计运行) |
| fuzz | 解码器模糊测试:PR 上冒烟,CI 每夜短跑轮转;正式实盘前的门槛按服务器上验证包运行的每个 core 解码器累计 CPU 时间计(只累计相关源路径与候选版本相同的运行);崩溃使计数清零,受限主机上的超时与 OOM 先记疑似、复现后才清零 | [testing §6.1](testing.md#61-服务器上的累计运行) |
| 解耦门槛 | CI 中新增第二个测试策略时,引擎 crate 的 diff 必须为零 | [testing §8](testing.md#8-上线门槛) |
| 延迟探针策略(`latency-probe`) | 只能绑定非正式账户的策略(`StrategyDesc.non_prod_only`):在主网行情负载下用测试网账户限速挂远离盘口的最小名义 PostOnly、Ack 后同一事件内撤单(`quote`,产生 `tick_to_trade` 与 `rep_to_order` 样本),两所最小名义 IOC 配对并平掉(`pair`,`hedge_host` 小样本确认),或走 `TargetIntent` → 执行层切片(`target`),为延迟验收提供订单类样本 | [strategies/latency-probe](../strategies/latency-probe/README.md) |
| 金样 | 真实交易所报文 → 期望归一化事件的固定样本 | [fixtures](../fixtures/README.md) |
| 测试规格(`TESTS.md`) | 组件的验收合同:每个用例有稳定编号(`<前缀>-<三位数>`)、级别、里程碑、确定的输入与期望、实现函数名;覆盖表把 README 每个规范性小节对到用例;跨组件场景、不变量 `INV-n` 与演练 `DRL-n` 在 `docs/tests/` | [docs/tests](tests/README.md) |
| `test-trace` | `cargo xtask test-trace`:里程碑已到的用例必须有同编号的测试函数(或场景、基准),测试函数的编号必须存在于测试规格 | [docs/tests §6](tests/README.md#6-与代码的对应) |
| 设计基线 v1 | 进入实现(M1)时冻结的规格(README + `TESTS.md`);此后每次行为变更同一提交改 README、`TESTS.md` 与代码,并在变更记录记一行 | [docs/changes.md](changes.md)、[docs/tests §7](tests/README.md#7-修改流程设计基线-v1-之后) |
