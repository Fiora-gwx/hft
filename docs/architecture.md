# hft 架构地图

改 `crates/`、`strategies/` 或 `apps/` 前读本页。类型归各 crate README 与 [subsystems/](subsystems/README.md),理由归 [Agent Notes](../.agents/notes/README.md) 与 [../../architecture-v2/](../design/README.md)。

## 1. 一句话

交易主机上运行同一个 Rust 热进程二进制的实例:**feed 任务 → SPSC 环 → 单写者 sequencer(序号、journal、簿、策略快速部分、执行层、风控、OMS)→ SPSC 环 → gw 任务**;任务到线程与核的映射是按主机类配置的线程计划。策略是静态编译进同一二进制的策略包(只依赖策略 SDK);策略的慢速部分在 advisor 线程运行,结果以 `Advice` 事件经 journal 回到 sequencer。慢的一切是只读 journal 的冷进程,只能经 ctl-gateway 写入一条被记录的控制环。状态 = f(journal)。

## 2. 部署单元

| 主机 | 进程 | 说明 |
|---|---|---|
| `tokyo-cex`(主机类是部署配置,[latency-budget §7](latency-budget.md#7-主机类与验收口径)):`dev2` = 现有 2 核主机,开发测试与模拟账户,**不放正式实盘账户**;统一升级主机(默认候选 `c32`)上 prod 与 sim 同机,延迟验收与正式实盘在这里 | [`hft-core`](../apps/hft-core/README.md) | 同一二进制两种实例:`prod` 只含交易所账户;`sim` 只含模拟账户、不加载交易所 key。dev2 上 `prod` 角色实例只在测试网联调窗口运行 |
| | [`journal-archiver`](../apps/journal-archiver/README.md)、[`watchtower`](../apps/watchtower/README.md)、[`reconciler`](../apps/reconciler/README.md)、[`kill-switch`](../apps/kill-switch/README.md) | Rust 冷进程;reconciler 与 kill-switch 跳过模拟账户 |
| | [`ops_bot`](../python/ops_bot/README.md)、[`pipeline`](../python/pipeline/README.md)、[`treasury`](../python/treasury/README.md) | Python 冷进程 |
| `dev2`(对统一升级主机) | `hft-offbox-watchdog`([deploy](../deploy/README.md)) | 机外看门狗:升级主机失联即经独立通道告警 |
| 可选扩展(CEX-DEX 迁移时) | [`chain-core`](../apps/chain-core/README.md)、[`chain-grpc-relay`](../apps/chain-grpc-relay/README.md) | 冻结;外部 venue 协议与事件编号段保留 |
| 现有采集主机 | collector(`Trading` 仓库) | 保持独立,服务研究 |
| `research` | [`replayer`](../apps/replayer/README.md);可选 [`research`](../python/research/README.md) | 离线 |

- **dev2 共机**:与 cdex 正式账户共机,hft 进程在资源护栏内运行,cdex 优先([hft-core dev2 护栏](../apps/hft-core/README.md#dev2-护栏))。
- **sim 与 prod 同机**:sim 只用非隔离核,独立 slice、journal、冷盘目录、网卡与出口 IP([hft-core 主机资源隔离](../apps/hft-core/README.md#主机资源隔离slice核与出口-ip))。
- **交付**:dev2 → 统一升级主机 → 上线门槛、零成交冒烟与正式实例 Shadow → 正式实盘;没有小额实盘阶段([launch](runbooks/launch.md))。CI 是唯一构建环境,服务器只安装发布制品([development](development.md))。

## 3. hft-core 线程

线程计划由配置 `[[threads]]` 按主机类给出,唯一归属 [hft-core 线程计划](../apps/hft-core/README.md#线程计划):同一线程按固定顺序轮转多个任务(`poll_once`/`step` 非阻塞、每轮有上限),全部任务连续空闲后按 `IdlePolicy` 退让。所有主机类上 `sequencer`、`persist` 与每个下单 venue 的 `gw:<venue>` 各自独占线程(T8,核不够就换更大的主机类)。

升级主机类(`c8` 及以上)的布局原则:核 0 承担 housekeeping、系统服务与管理网卡 IRQ;核 1 独占交易网卡的 IRQ 与软中断,不放线程;persist 独占一个非隔离核;warm 任务与 advisor 占其余非隔离核(`c8`/`c16` 核数不够时的合并例外归 hft-core);隔离热核上 `Spin` 运行 sequencer、`feed-bbo-*`、`feed-md-*` 与各 gw,`feed-bbo` 不与 `feed-md` 同线程;sim 实例用自己的非隔离核与独立网卡。`c32` 另预留核 28 与第二块交易网卡 ENI-B2,用于把批量行情流移出核 1(默认不启用)。`dev2`/`c4` 一个 venue 一个 `routes = All` 的 feed 任务;`dev2` 不隔离核、不忙等。

```
热任务(tick-to-trade 路径)
  feed-bbo:<venue>  BBO 连接 → 解码 → bbo 环;按 bbo_ping_ms 发 ping;BBO 补帧来自 snap-fetch(可跨 venue 合并到一个线程)
  feed-md:<venue>   成交/标记价/资金费/分档深度(TopN)连接 → 解码 → md 环;可选的增量深度及其 L2 重同步也在这里
  gw:<venue>        每个下单 venue 独占;out 环(OutMsg)→ 填帧模板/掩码/加密/write;回报 → rep 环
  sequencer         按优先级取环 → 赋 seq → 写 seq journal → 簿 → 策略快速部分 → 执行层 → 风控 → OMS → out 环
  ext:<chain>       可选:与 chain-core 的外部 venue 会话
warm 任务(不在 tick-to-trade 路径,不占热核)
  snap-fetch        行情 REST 的请求、读取、解析:BBO 补帧 → snap-<venue>-bbo 环 → feed-bbo;(增量深度启用时)L2 快照 → snap-<venue> 环 → feed-md
  acct-fetch        账户查询(保证金轮询、账户配置核验)的签名 REST 请求、读取与解码:gw → acctreq-<venue> 环 → acct-fetch → acct-<venue> 环 → gw(gw 线程不做大块解码)
  conn-builder      TCP/TLS 握手、WS 升级与登录;就绪连接经交接环交给 feed/gw
  ctl-gateway       Unix socket → 鉴权 → ControlCmd → ctl 环
  advisor           尾随 seq journal → 各策略 Model → Advice → advice 环;分块写 Model 检查点
  sim-gw            只在 sim 实例:out-sim 环 → hft-sim 撮合 → rep-sim 环
  housekeeping      心跳 → sd_notify、时钟校准(ClockStatus)、persist 滞后与磁盘余量 → Timer 输入
  persist           热段 → 冷副本复制与 fdatasync;段预创建与退役(会阻塞,独占线程)
```

**取环**:每个 venue 的 BBO 流与其他行情流走不同连接、不同环。sequencer 优先级 `ctl > rep(实盘回报)> bbo > advice > md > rep-sim`;md、bbo 与 advice **逐条处理**:每条 md 之后查看 ctl、实盘 rep、bbo 的环头,每条 bbo 或 advice 之后查看 ctl 与实盘 rep 的环头,非空先处理它们;md 每轮另有时间片限额,`rep-sim` 每轮至多 `sim_max_per_round`(参数归 [hft-sequencer](../crates/hft-sequencer/README.md))。取出顺序写进 journal,确定性不变。同一 inst 的 BBO 积压允许合并,写进记录 `flags`。分档深度(TopN)每条快照整替换该簿;可选的增量深度按交易所消息整条成 `L2Batch`;都整条应用后才调用策略。同一事件派发给多个策略实例时,绑定实盘账户的实例在前。模拟账户的 `OutMsg` 路由到 sim 出环,不进任何 gw。

**收包**只用中断驱动:交易网卡 IRQ 在专用 IRQ 核,热线程只做非阻塞 `recvmsg`,busy poll 全关([T12](hot-path-rules.md#线程t))。kill 字是启动期静态分配的 `&'static [KillWord]`(每 gw 一个),sequencer 写、gw 读。线程与核的装配只在 `apps/hft-core`;库 crate 不创建线程。

## 4. 一次 tick-to-trade

| 步 | 线程 | 做什么 | 所属 crate |
|---|---|---|---|
| t0 | 网卡 | HW RX 时间戳;IRQ 与软中断在专用核 | — |
| t1 | feed-bbo / feed-md | 非阻塞 `recvmsg` → rustls 解密 → WS 解帧 → 分派 → 解码 → 定点归一化 → bbo/md 环;raw journal 追加 | `hft-net`、`hft-venue`、`venue-*` |
| t2 | sequencer | 取出 → 赋 `seq` → 追加 seq journal → 更新簿、校验结算、记账并写账本注记 | `hft-sequencer`、`hft-journal`、`hft-book`、`hft-ledger` |
| t3 | sequencer | 策略 `on_event` → `OrderIntent` 或 `TargetIntent` → 执行层产出订单意图(写 `ExecDecision`)→ 风控(含第二腿报价的连接健康与可负担性)→ OMS 分配槽与 client id → `OrderDecision` 写 journal → `OutMsg` 入 out 环 | `strategies/*`、`hft-exec`、`hft-risk`、`hft-oms` |
| t4 | gw | 取 `OutMsg` → 填帧模板 → WS 掩码 → TLS 加密 → `writev` → `sent_tsc`(`SendReport` 回 rep 环) | `hft-venue`、`venue-*`、`hft-net` |
| t5 | gw → sequencer | 回报 → `OrderEvent` → OMS 身份与结算校验 → 槽迁移 → `LegLink` 第二腿出环(量 ≤ 裸量);同事件记账,外来结算核对原件与增量;无成交的槽更新和基础币费用调整也刷新风控敞口 | `hft-oms`、`hft-exec`、`hft-ledger`、`hft-risk` |

分段名与主机内预算(`tick_to_trade`、回报触发下一单的 `rep_to_order` 及其对冲子集 `hedge_host`)唯一归属 [latency-budget](latency-budget.md)。不做"每 tick 空跑下单路径",**不对真实连接预加密**([T5、T6](hot-path-rules.md#线程t))。

## 5. 状态所有权

| 状态 | 唯一写者 | 其他线程/进程如何看到 |
|---|---|---|
| 簿、BBO、BBO 成员连接健康、最新成交、资金费、mark | sequencer | 冷进程读 journal 重建 |
| 仓位、现货库存、权益视图数据、挂单槽、LegTracker、风控计数与 cap、执行层目标、策略快速部分状态、引擎时间 `engine_ns` | sequencer | 同上;`StateSnapshot` 每段首与周期关键帧;快照里的时刻存 `engine_ns`,只有 [D6](hot-path-rules.md#确定性d) 登记的 TSC 行例外,恢复时的处置按 D6 |
| 策略 Model 状态 | advisor 线程(只读 journal 输入,确定性) | 输出 `Advice` 进 journal,回放读日志不重算;检查点只用于重启免预热 |
| 模拟账户的交易所侧真值 | sim 实例的 sim-gw([hft-sim](../crates/hft-sim/README.md)) | 回报经回报环进入并记 journal;账本侧由 sequencer 记账 |
| socket、TLS 会话、WS 缓冲、raw journal | 对应 feed/gw 线程 | raw journal |
| 行情 REST(BBO 补帧、可选的 L2 快照)的连接与解析 | `snap-fetch` | 解析结果经环交给对应行情任务,随 `RawRx` 记录 |
| 交易所字符串 id → slot 映射 | 对应 gw 线程 | 解码时写进 `OrderEvent.slot` |
| refdata、策略参数、`RiskParams` | sequencer(经 `ConfigBlob`/`ControlCmd` 整体替换) | journal |
| kill 字 | sequencer 写,每个 gw 读自己的那一个 | 唯一允许的跨线程共享可变量(位布局 `KillBits` 归 [hft-types](../crates/hft-types/README.md)) |
| 交易所真值 | reconciler | `ReconcileReport` / `ReconcileBatch` 经控制环进入,处理归 [sequencer](../crates/hft-sequencer/README.md#每条-input-的处理顺序process) |
| 保证金率、强平价、ADL 档、账户配置(含实际杠杆) | venue gw | `MarginReport`、`AccountConfigReport` 事件 |
| 链上池状态、nonce、tx 生命周期(optional) | chain-core | 经外部 venue 协议进入 |

## 6. 事件域

记录头全系统唯一(24 B:`len, kind, flags, seq, tsc`)。五个域:行情(`Bbo`、`L2Batch`、`L2Snapshot`、`Trade`、`Funding`、`Mark`…)、执行(`OrderDecision`、`SendReport`、`OrderEvent`、`ExternalFill`、`MarginReport`、`AccountConfigReport`、`ExecDecision`…)、链上(optional,编号段保留)、控制(`ControlCmd`、`CmdResult`、`ConfigBlob`、`ReconcileReport`)、系统(`Timer`、`ConnState`、`Advice`、`StateSnapshot`、`LatencySample`、`Note`、`HostProfile`)。模拟账户的回报与实盘回报是同一组事件,按 `AccountMeta.kind` 区分。字段、编号段与演进规则见 [event-model](subsystems/event-model.md);文件格式见 [journal-format](subsystems/journal-format.md)。

## 7. 组件分层

```
L0 纯基础    hft-types ← hft-refdata(含 VenueCaps、AccountMeta)← hft-event(链上 kind 在独立编号段)
L0 策略接口  hft-strategy-sdk(只依赖 L0)
L1 纯领域    hft-venue-proto ← venue-<x>;hft-ctl-proto;hft-book  hft-oms  hft-risk(含 hft-risk::mode)  hft-ledger  hft-exec
             (oms/risk/ledger/exec 可依赖 hft-strategy-sdk 的视图与意图类型;book 只用其快照读写类型)
L2 策略构件  hft-strat-lib(L0 + hft-strategy-sdk)
L3 运行时    hft-rt  hft-journal  hft-net  hft-venue(依赖 hft-venue-proto,不依赖具体 venue)
L4 装配核    hft-sequencer(hft-strategy-sdk + L1 + hft-journal + hft-rt;对 StrategySet 泛型,不依赖任何策略包)
L5 冷/工具   hft-ctl  hft-telemetry  hft-sim  hft-py(reader + ledger + SDK params 编解码)  hft-testkit(共用 hft-sim 撮合器;不依赖 registry)
测试装配    hft-ctl 的真实引擎 E2E 测试 dev 依赖:hft-sequencer + hft-oms + hft-ledger + hft-exec
optional     hft-ext-proto  hft-amm  hft-chain-evm  hft-chain-solana(冻结)
strategies   strategies/<name>:只依赖 hft-strategy-sdk + hft-strat-lib(Python pipeline 插件随包在 strategies/<name>/pipeline/);strategies/registry(hft-strategy-registry):唯一策略清单,依赖各策略包
apps         hft-core、replayer(二者经 hft-strategy-registry 取得同一 StrategySet)journal-archiver watchtower reconciler kill-switch;optional:chain-core chain-grpc-relay mock-exchange
tools        hft-cfg(按 schema 通用编码) xtask hft-verify hostprobe az-probe hft-secrets latency-slice(M1.5 探针专用)
python       框架包 ops_bot  pipeline  treasury;optional:research
```

tier(frontmatter 标注):`pure` 无 I/O 无线程;`hot` 在 tick-to-trade 路径上;`warm` 在热进程内但不在该路径上;`cold` 独立进程;`dev` 只用于测试与基准。依赖方向与 tier 规则见 [crates/AGENTS.md](../crates/AGENTS.md)。**解耦由机检保证**:`cargo xtask deps` 查依赖方向,`doc-check` 查名字越界,解耦门槛要求新增第二个测试策略时引擎 crate diff 为零([testing §8](testing.md#8-上线门槛))。

## 8. 新行为放哪

| 目标 | 放在 |
|---|---|
| 新策略 | `strategies/<name>` 新 crate(`Strategy` + `Model` + 参数 schema,需要时加 `pipeline/` 插件)+ registry 清单一行;引擎、app 与 `python/` 框架零改动([cookbook](cookbook/adding-a-strategy.md)) |
| 策略的重计算(打分、分配、分位、资金费统计、冲击估算) | 该策略的 `Model`,输出不透明 `Advice`;不进 sequencer |
| 新执行算法 | `hft-exec` 的 `ExecSpec` 变体(通用、由参数驱动) |
| 接入新交易所 | 新建 `venue-<x>` 实现 [`VenueProtocol`](../crates/hft-venue-proto/README.md) 并给出 `VenueCaps` 默认值;在 hft-core 装配 feed/gw([cookbook](cookbook/adding-a-venue.md)) |
| 交易所行为差异 | `VenueCaps` 字段;策略与执行层不按交易所分支 |
| 新事件字段/新事件 | `hft-event`,只追加([cookbook](cookbook/adding-an-event-kind.md)) |
| 新运维命令 | `ControlCmd` 变体 + 权限表 + ops_bot 映射([cookbook](cookbook/adding-a-control-command.md)) |
| 新的风控检查 | `hft-risk` 检查链,按廉价 → 昂贵排序 |
| 新的派生指标/告警 | `watchtower` 规则(冷,读 journal;只告警) |
| 策略报表、Note 码、人工操作、预热数据与名单 | 策略的描述符与 `StrategyCmd`;该策略的 pipeline 插件经 `InjectWarmup` 或 `SetParams` 下发 |
| 交易 journal 的新 Parquet 视图 | `journal-archiver` 的映射 + `gen-schema` |
| 改已有行为(设计基线 v1 之后) | 同一提交改归属 README、`TESTS.md` 与代码,[changes.md](changes.md) 记一行([docs/tests §7](tests/README.md#7-修改流程设计基线-v1-之后)) |
| 新链/新 DEX(optional) | `hft-amm` + `hft-chain-evm`;对 sequencer 仍是 `PoolQuote`/`TxLifecycle` |

## 9. 回放、恢复与确定性

DST 的只读观察仅在 `testkit` 构建启用。策略定时器先预留配额,ID 随快照恢复。ClockStatus 记录后只更新冷读者校准行,不进决策状态与哈希。

`hft-sequencer` 的三种驱动:实盘读环;恢复从快照重放完整事件、对账后放行,停机标记以 Kill Input 保留;验证回放自检快照、预热窗口并逐字节核对 Output 与哈希,普通滚段延续状态,重启或版本变化从新引导快照独立运行([驱动契约](../crates/hft-sequencer/README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用))。Advice 是 Input,回放读日志;Model 按合法来源续算,恢复缺口中断冷验证来源([规则](../apps/replayer/README.md#verify))。跨重启时间用 engine_ns,停机不计;跨启动 Advice 按 `basis_seq < Now.boot_seq` 判过期。

**模拟重演**(`replayer sim`)把录下的 journal 行情喂给模拟网关与同一策略,用于排障与复现,**不评估收益**;策略表现只在模拟账户上观察([hft-sim](../crates/hft-sim/README.md))。没有历史回测。

Drain 退出前的发布边界由 [sequencer 主循环](../crates/hft-sequencer/README.md#主循环step) 保证,退出前最后一个完整事件及 Drain 快照对恢复读者可见;封段与关闭由装配层执行。

恢复核容量、结束旧 Drain 状态并接替 Draining,新容量入引导配置;快照保留停止点 engine_ns,分歧入输出哈希。

journal 记录分三类:**Input**(进入决策的一切,含实盘驱动当场做出的环境相关决定,记为 `Timer`)、**Output**(决策、意图与执行层决定)、**Meta**(测量,不参与验证比对与状态)。确定性前提:时钟、随机数、配置内容、定时器全部来自 Input 记录;无 `HashMap` 迭代依赖;无全局可变状态。细节见 [`hft-sequencer`](../crates/hft-sequencer/README.md) 与 [`replayer`](../apps/replayer/README.md)。

## 10. 核心类型归属

| 类型 | 归属 |
|---|---|
| `Px`、`Qty`、`Notional`、ID 类型(含 `GroupId` 的 `GROUP_NONE`/`GROUP_ALL`)、`Side`、`Tif`、`Tsc`、`KillBits`、`ClientOrderId` 编解码、`RunMode`/`KillLevel`、`RejectBits`、`cap_cross_px`、`HostClass` | [hft-types](../crates/hft-types/README.md) |
| `RecordHeader`、`EventKind`、各事件体、`OrderFlags`、`ControlCmd`、`CmdResult`、`ConfigBlob`、环元素(`FeedMsg`、`RepMsg`、`OutMsg` = `GwCmd`、`FeedCmd`) | [hft-event](../crates/hft-event/README.md) |
| ctl 帧与命令权限表;命令语义与角色 | [hft-ctl-proto](../crates/hft-ctl-proto/README.md);[control-plane](subsystems/control-plane.md) |
| `RefData`、`InstMeta`、`AssetMeta`、`AccountMeta`、`VenueMeta`、`VenueCaps`、`FeeSchedule`、`RateLimitTable`;`EngineConfig` 类型(取值按主机类由 hft-core 给出) | [hft-refdata](../crates/hft-refdata/README.md) |
| `Strategy`、`Model`、`StrategyDesc`、参数 schema 与规范编解码 `params`、`Views`/`ModelViews` 与视图类型(含 `BboView`)、`Now`、`Ctx`、`OrderIntent`、`TargetIntent`、`ExecSpec`、`StrategySet`/`strategy_set!` | [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) |
| 执行算法、腿顺序、两腿 lot 对齐、对冲量上限、引擎默认减仓、`flatten_lock`、`manual_hold`、Broken 有界平仓、TSC 行的平移公式 | [hft-exec](../crates/hft-exec/README.md) |
| `Spsc`、`ShmSpsc`、`ByteSpsc`、`TscClock`、`CpuSet`、`KillWord`、`HeartbeatBlock`、`TaskLoop`/`IdlePolicy` | [hft-rt](../crates/hft-rt/README.md) |
| `JournalWriter`、`JournalReader`、`SegmentHeader`、`Persister` | [hft-journal](../crates/hft-journal/README.md) |
| `L2Book`、`BookSet`、`Top`、快照节 6/7 的行(`BookMetaRow`、`TopRow`) | [hft-book](../crates/hft-book/README.md) |
| `Conn`、`Timestamps`;`VenueProtocol`;`FeedRuntime`、`GwRuntime`、`SnapFetchTask` | [hft-net](../crates/hft-net/README.md);[hft-venue-proto](../crates/hft-venue-proto/README.md);[hft-venue](../crates/hft-venue/README.md) |
| `OrderSlot`、`LegLink`、LegTracker、client order id 的分配 | [hft-oms](../crates/hft-oms/README.md) |
| `RiskParams`、检查链、`classify`、`hft-risk::mode`(cap、有效模式、MarginStress、启动闸门) | [hft-risk](../crates/hft-risk/README.md) |
| `Position`、`FillDelta`、`Ledger`、`PnlBreakdown`、journal 冷消费者 `JournalLedger`;模拟撮合器与模拟账户 | [hft-ledger](../crates/hft-ledger/README.md);[hft-sim](../crates/hft-sim/README.md) |
| `LiveDriver`、`RecoverDriver`、`VerifyDriver`、`engine_ns` 的累加、恢复结束时刻 `recovery_end` 与平移的调度 | [hft-sequencer](../crates/hft-sequencer/README.md) |
| 外部 venue 帧、tx 生命周期与链上 profile(optional) | [hft-ext-proto](../crates/hft-ext-proto/README.md)、[hft-chain-evm](../crates/hft-chain-evm/README.md) |
