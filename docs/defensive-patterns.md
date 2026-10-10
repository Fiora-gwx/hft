# 防御性模式

每一节是一个**缺陷类别**,写成防止它发生或复发的规则:标"事故"的节真实出过事(经过只在 [postmortem/](postmortem/README.md) 里叙述);标"来源"的节是评审中发现、尚未发生的资金或状态风险,postmortem 索引不收录。写执行、状态机、链上、重试、部署或 CI 代码之前读本页。本页只写规则、它在结构上落在哪里、怎么验证。设计侧的对照表见 [architecture-v2/12 §1](../design/12-风险与未决问题.md) 与 [research/00 §5.3](../design/research/00-existing-codebase-inventory.md)。

## 传输错误 ≠ 业务失败

**事故**:2026-09-24 交易节点返回 HTTP 403,已经上链的 swap 被当作失败,引擎按失败处理 → 裸多([索引](postmortem/README.md))。

**规则**:超时、403/429/5xx、断连、解析失败只说明"我们不知道结果",不说明"没发生"。它们只能把订单槽或链上交易推入 **`Unknown`**。`Unknown` 是显式状态,语义是"已发出,结果未知":

1. 只有证据能让它离开:交易所回报(`OrderEvent`)、查询结果(`QueryResult`)、链上回执(配置 ≥ 3 个独立运营商的来源——按封禁/限流域划分,不按 URL;≥ 2 个独立运营商一致才算;可用独立来源 < 2 持续 N 块 → P0 转人工,不推断)、`ReconcileReport`。
2. 停留期间:同一 group 不开新仓,不做任何反向操作(不回滚另一腿、不重发)。
3. 有界:CEX 侧由重连后查询与对账收敛;链上由**交易自身的链上过期**(包装合约内 `block.number <= maxBlock`)保证"过了 `maxBlock` 且终局后必知"——bundle 有效期不够,见[下一节](#bundle-有效期--交易有效期)([06 §2.7](../design/06-链上执行层.md))。
4. 超过收敛时限仍为 `Unknown` → P0 告警,不自动处理。

**落点**:[`hft-oms`](../crates/hft-oms/README.md) 的 `OrderState::Unknown`;链上(optional):[`hft-chain-evm`](../crates/hft-chain-evm/README.md) 的 `TxState::Unknown`;发送节点与回执节点分离、多源回执。

**验证**:OMS 与 tx 生命周期属性测试——任意传输错误序列不产生仓位变化;DST 不变量 1、4([testing §4](testing.md#4-必须在-dst-与属性测试中持续成立的不变量))。

## bundle 有效期 ≠ 交易有效期

**来源**:2026-10-01 红队评审(资金风险)。

**缺陷**:把 bundle 的 `maxBlockNumber` 当成"这笔交易过期了"。bundle 过期只说明 builder 不再按这个 bundle 打包;里面那笔**已签名交易在其 nonce 被消耗之前一直有效**——泄露到公共 mempool、被 builder 留存、或重组后被回灌,都可能在我们判定 `Dropped`、甚至已经为它做了反向处理之后上链。这会复制 9/24 的形态:我们以为没成交,链上却成交了。

**规则**:

1. 每笔链上交易都带**链上过期**:经最小包装合约执行,合约内 `require(block.number <= maxBlock)` + `minOut` 校验,`maxBlock` = bundle 的 `maxBlockNumber`;走 Router 的路径同时设 `deadline`。迟到上链只会 revert(只花 gas)。
2. `Dropped` 只在**终局块号 > `maxBlock` 且该 nonce 未被本交易消耗**时成立;在此之前一律 `Sent`/`Unknown`。
3. 给 builder 的出价一律在合约内转出(InTx),不用独立的 N+1 转账——否则出价交易可以脱离主交易单独上链。
4. 重来 = 新意图、新 nonce 规划,旧交易在链上仍被 `maxBlock` 约束。

**落点**:[`hft-chain-evm`](../crates/hft-chain-evm/README.md) 的生命周期状态机(按"nonce 槽 → 候选交易集合"设计);[contracts/](../contracts/README.md) 的 SwapGuard 包装合约;[tx-onchain-expiry](../.agents/notes/proposed/architecture/2026-10-01-tx-onchain-expiry.md)。

**验证**:DST 链上故障"泄露/重组回灌后迟到上链"([testing §3.4](testing.md#34-dst));合约 Foundry 测试:`block.number > maxBlock` 必 revert、`minOut` 不满足必 revert。

## 未登记的错误码默认进 Unknown,不默认 Reject

**来源**:2026-10-01 红队评审。

**缺陷**:解码回报时把"不认识的错误码"归入 `Reject`。交易所的很多错误码表示"系统忙/超时/结果未知"(如 OKX 50001/50004/50013、Bybit 10000/10016),订单可能已进入撮合;按 `Reject` 处理会释放槽与敞口额度、允许重发——一旦原单其实成交,就是双倍敞口或裸腿。这是"传输错误 ≠ 业务失败"在应用层错误码上的同一类缺陷。

**规则**:

1. 每个 `venue-<x>` 的 `codes.rs` 显式列出三类错误码:确定拒绝(`Reject`)、结果未知(`Unknown`)、限速(`RateLimited`)。**不在表里的码一律 `Unknown`** 并告警,由查询或对账收敛。
2. 没有过期保护的所(Bitget、KuCoin;待核实,方法:各所下单接口文档与测试网实测)查单返回 NotFound 时也不能按超时定终态,只能由对账收敛;带 `recvWindow` 的所(Binance)查单 NotFound 同样不定终态,按 `Unknown` 处理并由成交历史确认。**对冲腿**查单返回 NotFound 一律保持 `Unknown`,直到成交历史确认;槽回收后保留墓碑(最近 `tombstone_gens` 代),迟到成交照常入 ledger 与 LegTracker 并报 P0,不再触发第二次对冲([`hft-oms`](../crates/hft-oms/README.md))。
3. 这是 [`hft-venue-proto`](../crates/hft-venue-proto/README.md) 对实现者的契约,新 venue 接入时逐码核对交易所文档。

**落点**:`hft-venue-proto` 的实现者契约;各 `venue-<x>` 的 `codes.rs`;`hft-oms` 的 `Unknown` 处理。

**验证**:每个 venue 的金样覆盖每个"结果未知"码与一个未登记码(期望 `Unknown`);DST 故障"未登记错误码"。

## 成功要有正面证据,报价不是成交

**事故**:2026-09-23 Solana 交易被确认但 `meta.err != null`,余额变化为 0,代码退回报价当成交,随即平掉币安空单 → ARC 裸多。

**规则**:

1. 成功 = 执行结果无错误(EVM `status == 1`;Solana `err == null`)**且**目标事件存在(Swap 日志)**且**余额差/内部指令符合预期。三者缺一即不是成功。
2. 成交数量与价格只能来自交易所回报或链上事件/余额差;任何代码路径不得用报价、意图或"预期值"填充成交。
3. "被打包"、"被确认"是包含证据,不是成功证据。

**落点**:`hft-chain-evm` 的生命周期状态机(`IncludedOk` 需要结果校验通过);`hft-chain-solana` 同一规则;`OrderEvent` 的 `px/qty/cum_filled` 只由 gw 解码回报写入。

**验证**:属性测试"执行失败但被包含"、"成功但 Swap 日志缺失"、"余额差为 0"三类输入都不得产生对冲意图;金样覆盖失败回执。

## 采样粒度匹配执行粒度,重试是新意图

**事故**:2026-09-20 AIN 插针——1 s 单点采样触发平仓,revert 后盲重试,没有重新判断信号;另有 JCT 重试沿用旧价。

**规则**:

1. 慢信号的确认窗口按执行粒度设:链上跨 ≥ 2 块、≥ 2 次链上报价,**每个事件都评估**,不是隔固定时间采一次([07 §3](../design/07-策略层.md))。
2. 意图携带的价格只在生成它的那个事件里有效。重试 = 新意图:必须在最新状态上重新通过全部检查(链上是 L0 + L1,CEX 是风控链),不达标就放弃这次机会。
3. 决策、模拟账户与研究用同一数据源与同一粒度;研究里的 1 h 收盘价不能当作执行里的秒级价格。

**落点**:[`hft-strat-lib`](../crates/hft-strat-lib/README.md) 的 `ConfirmWindow`;OMS 不提供"原样重发"接口,`HedgeFix` 生成新意图。

**验证**:DST 场景"单块尖峰后恢复"不得触发;"第一次 revert"后若最新状态不满足条件,不得有第二次发送。

## 价格只有一个来源:信号事件

**事故**:2026-09-25 NAORIS——`min_out` 用发送时的新报价计算,滑点保护只防"发出 → 上链",从不检查触发价;高费率开的仓在 −0.3 平掉。

**规则**:

1. 限价(CEX 的 `px`、链上的 `min_out`)由信号事件与 L1 模拟共同决定,写进 `SwapDecision`/`OrderDecision` 事件,之后不可放宽。
2. 发送前只允许在最新块上**再次校验**;校验失败放弃,不允许重新取价、按新价放宽限价。
3. 一个仓位只属于一个策略实例的一个 group,出场规则跟着它走;同一账户不混放两个策略的仓位([一个账户只给一个策略实例](#一个账户只给一个策略实例身份与配置先核验))。

**落点**:[`hft-strategy-sdk`](../crates/hft-strategy-sdk/README.md) 的意图类型(限价随意图一次给定);[`hft-exec`](../crates/hft-exec/README.md) 的切片不放宽限价;`hft-chain-evm`(optional)的 bundle 构造只读 `SwapIntent.min_out`,没有"刷新报价"入口。

**验证**:回放断言每个链上发送的 `min_out` 等于对应 `SwapDecision` 记录中的值;属性测试"发送前价格变差"只能导致放弃。

## 所有循环有界,看门狗看的是进展

**事故**:2026-09-23 `recommend()` 从 0.5% 起按 0.1% 步长扫到序列最大值,同名假币算出 10⁵ % 价差,循环几十亿次,烧 2 核 7.5 h;流水线与扫描器 8 小时没写状态也没人发现。另:采集系统的健康检查只看磁盘与目录哈希,分片 crash-loop 时仍报健康。

**规则**:

1. 每个循环的最坏迭代次数由数据结构容量或显式上限界定;扫描类算法按输入范围先截断(脏数据阈值,如 `|spread| > 30%` 直接拒绝)再计算。
2. 看门狗检查**进展**(心跳计数前进、seq 前进、状态文件时间戳前进),不是"进程还活着"或"端口还开着"。
3. 热进程:sequencer 递增心跳计数,housekeeping 每 1 s 确认前进后 `sd_notify`;systemd `WatchdogSec` 在停滞时重启。冷进程:cgroup CPU 配额,禁入隔离核。整机:交易主机之外由 dev2 上的 `hft-offbox-watchdog.service` 每 15 s 查 watchtower 的 `/healthz`(hft-core 心跳年龄、kill-switch 存活、最近一次对账时刻),连续 60 s 不可达或不健康 → 经独立通道告警;本机维护静默期间只发 Telegram、不打电话([watchtower](../apps/watchtower/README.md)、[deploy](../deploy/README.md))。
4. 告警不是降级:persist 滞后与磁盘余量由 housekeeping 写成 `Timer{PersistLag|DiskLow}` 输入,sequencer 据此设条件型 cap `Storage`(禁开;超阈值 Drain;存在未到达的引擎减仓或清仓目标时推迟,推迟条件归 [hft-risk 存储降级](../crates/hft-risk/README.md#存储降级storage),硬阈值是 journal 本身),不能只靠人看告警([`hft-risk`](../crates/hft-risk/README.md)、[`hft-sequencer`](../crates/hft-sequencer/README.md))。

**落点**:[hot-path-rules T10、T7](hot-path-rules.md#线程t);`apps/hft-core` housekeeping;`watchtower` 的静默检测;[`deploy/`](../deploy/README.md) 的 cgroup 与 `WatchdogSec`。

**验证**:DST 极端输入(10⁵ % 价差、空簿、满槽、万级名单)下单事件处理时间有上界;混沌测试"sequencer 停滞"必须在 `WatchdogSec` 内被重启;"冷盘写满 / persist 停滞"→ `Storage` cap 生效、不再开仓。

## 外部端点有速率治理,权威源自建

**事故**:2026-09-23/24 两台引擎向 4 个公共 BSC 节点并行 multicall,每秒几百次 swap 模拟,全部 dataseed 节点对本机返回 403 五小时,约一半报价分块失败。

**规则**:

1. 每个外部端点(交易所 REST/WS、RPC、builder、块流)一个令牌桶,容量取官方限额的 80%(留 20% 余量);超额请求在本地排队或丢弃,不发出去。
2. 任何单一外部源都不作为唯一证据。链上(optional)的状态源与发送路径按 profile 规定:free profile 经多个独立运营商的公共 RPC 发送与取回执,以 ≥ 2 个独立运营商一致为证据;builder profile 以自建节点为权威、付费 RPC 只做回执备份([`hft-chain-evm`](../crates/hft-chain-evm/README.md))。
3. 限速错误触发退避与 `RateLimitHit` 事件,由策略侧 `Throttle` 联动;不以并发换速度。
4. 开仓单不能耗尽平仓与对冲的额度:本地令牌桶为 `RISK_REDUCING` 单预留份额 `rr_reserve`([`hft-risk`](../crates/hft-risk/README.md))。

**落点**:[`hft-refdata`](../crates/hft-refdata/README.md) 的 `RateLimitTable`;`hft-venue` 发送侧计数;`hft-risk` 检查链 #8;`hft-chain-evm` 的端点预算。

**验证**:mock 交易所限速拒绝场景;单元测试"令牌耗尽时不产生发送";"开仓打满令牌后对冲与平仓仍能发出"。

## 权限按人与角色,运行模式是被记录的命令

**事故**:cdex 部署审计(2026-09)——Telegram bot 只按 `chat.id` 白名单,群里任何成员都能交易,没有只读角色;Rust 启动器把 `dry_run = false` 写死;Ed25519 key 缺失时 Rust 引擎 fatal 退出,而 Python 侧一切正常,看起来"系统在跑"。

**规则**:

1. 鉴权按发送者(`from.id` / socket 对端 uid + 签发者),ops_bot 角色 = Admin / Trader / Viewer;群聊只能发只读命令与**默认参数**的降级命令(`/kill`、`/mode … closeonly`),其余命令只在私聊。老板为 Viewer:只读,**唯一的写权限是紧急停止** `/stop [account]`(语义、确认与不健康时的路径见[紧急停止在引擎不健康时也要可达](#紧急停止在引擎不健康时也要可达));Viewer 不能 `Resume`,不能发任何其他写命令,审计照常。角色与命令权限的唯一归属是 [control-plane](subsystems/control-plane.md)(权限表由 [`hft-ctl-proto`](../crates/hft-ctl-proto/README.md) 代码生成)。
2. 降级类命令永远允许;升级类(`SetMode → Live`、放宽限额、`Resume`,以及人工主体把 `open_allowed` 从 0 改为 1)需 Trader 角色且二次确认(PREVIEW + `expect_state_version`);`SetMode` 的类别按目标值判定(`→ Live` 升级,`→ CloseOnly`/`→ Shadow` 降级),分类表唯一归属 [control-plane](subsystems/control-plane.md);pipeline 主体发的 `SetParams` 是中性命令(在 Live 账户上新增 group、`open_allowed` 0→1 同样不需确认),人工覆盖项按实例分键([模拟上的试验不得流到实盘](#模拟上的试验不得流到实盘覆盖项按实例分键));`Kill` 的 `flatten` 需 Trader + 确认(启动闸门未放行与 DRAINING 期间同样允许,是升级类的例外),`hard` 需 Trader。
3. 运行模式(`Shadow`/`CloseOnly`/`Live`)只能经 `SetMode` 命令改变,命令与结果写进 journal;代码与配置里没有"默认实盘"。冷启动不改期望模式,而是加 `Recovery(account)` cap,有效模式不高于 `CloseOnly`,直到对账通过后的 `Resume{Recovery}`(Trader + 确认);崩溃恢复后不自动解除 `Recovery`(干净 Drain 重启在闸门通过后自动解除 `Recovery`,出现 `tail_lost` 或恢复不一致时不自动解除;只含 Sim 账户的实例另有自动解除条件,归 [`hft-risk`](../crates/hft-risk/README.md))。
4. 启动缺任何凭据或引用即拒绝启动并告警,不部分启动。

**落点**:[control-plane](subsystems/control-plane.md) 的角色与权限;[`hft-ctl-proto`](../crates/hft-ctl-proto/README.md) 权限表;[`ops_bot`](../python/ops_bot/README.md)(`/stop`);`hft-risk::mode` 的期望模式与 `Recovery` cap;根 AGENTS.md "错误配置在加载时失败";Viewer 紧急停止的理由见 [viewer-emergency-stop](../.agents/notes/proposed/architecture/2026-10-03-viewer-emergency-stop.md)。

**验证**:ctl-gateway 单元测试覆盖每个命令 × 每个角色(含"Viewer 发 HARD 或带 flatten 的 Kill、`Resume` 或任何其他写命令 → 拒绝"、"Viewer 的 `/stop` 未确认 → 不发出");启动测试"缺 key 退出码非 0 且产生告警"。

## 紧急停止在引擎不健康时也要可达

**来源**:2026-10-03 M0-b 与 M0-d 评审。

**缺陷**:老板的紧急停止按普通升级命令确认——预览后比对 `ctl_state_version`。事故中 cap 频繁变化,版本每秒都在变,确认反复 `ConfirmInvalid`,最需要停的时候停不下来;停止失败或超时没有任何通知;hft-core 卡死或 ctl-gateway 不可达时,唯一的停止入口跟着失效。

**规则**:

1. `/stop [account]` = 非 HARD、不带 flatten 的 `Kill{Global | Account}`:禁止新开仓、撤销挂单;已成交第一腿的对冲与平仓照常(`RISK_REDUCING` 放行)。
2. Viewer 的 Kill 确认只绑定预览的 `cmd_hash` + TTL(60 s),由发起预览的同一 `from.id` 按下;**不比对 `ctl_state_version`**(其他命令仍比对)。
3. hft-core 不健康(心跳过期或 ctl-gateway 不可达,判定条件与时限归 [ops_bot](../python/ops_bot/README.md))时,ops_bot 经 kill-switch 的本地触发套接字(只接受 ops_bot uid)兜底,同时 P0 告警给值班交易员;`/stop` 失败或无结果同样 P0。兜底默认是 `stop_core`:kill-switch 撤单后经 polkit **只允许** `systemctl stop hft-core@prod`(SIGTERM → Drain,不平仓)。**停引擎会同时停掉 L3 减仓与在途对冲的续接**:任一账户 MarginStress ≥ L2、有 Broken 或在途链接时,兜底只撤单 + P0,停引擎需要第二次确认(`/stop … core`)。
4. **停机要跨过重启**:兜底停止不经 journal,所以 kill-switch 在 `systemctl stop` 之前写持久停机标记 `/var/lib/hft/kill-switch/stop-marker.json`(格式归 kill-switch);hft-core 启动时读到标记,写一条 Input `ControlCmd{Kill}`(该范围、非 HARD、不带 flatten;主体与放行规则归 hft-core 与 control-plane)并消费标记。干净重启因此不会自动恢复开仓,Kill cap 只由交易员 `/resume kill` 解除。`stop_core` 停机失败时 kill-switch 把标记改名作废(`.aborted`)并 P0,hft-core 只读未作废的标记,是否另发 Kill 由人决定(规则归 [`kill-switch`](../apps/kill-switch/README.md))。
5. kill-switch 仍只撤单、不平仓;平仓留给恢复后的引擎或人工。

**落点**:[control-plane](subsystems/control-plane.md) 的 Viewer 确认规则与 `cli:systemd` 主体;[`ops_bot`](../python/ops_bot/README.md) 的 `/stop`、回退路径与二次确认;[`kill-switch`](../apps/kill-switch/README.md) 的本地触发套接字、`stop_core` 与停机标记;[`apps/hft-core`](../apps/hft-core/README.md) 启动时读停机标记;[deploy](../deploy/README.md) 的 polkit 规则;理由见 [viewer-emergency-stop](../.agents/notes/proposed/architecture/2026-10-03-viewer-emergency-stop.md)。

**验证**:ctl 测试"预览后 `ctl_state_version` 变化,Viewer 的 Kill 确认仍生效;TTL 过期 → 拒绝;另一 `from.id` 按确认 → 拒绝";演练"停掉 hft-core 心跳 → `/stop` 经 kill-switch 撤单 + P0";"ctl-gateway 不可达 → `stop_core` 撤单并停 `hft-core@prod`;重启后带 Kill cap、不开仓,`/resume kill` 后才恢复";"有 Broken 或在途链接时兜底只撤单,`/stop … core` 二次确认后才停引擎";"`/stop` 无结果 → P0";kill-switch 测试"非 ops_bot uid 的本地触发 → 拒绝"、"polkit 拒绝 `hft-core@prod` 之外的任何单元操作"。

## 段滚动必须带快照

**事故**:2026-09-07/08 24 h 采集中 84% 的 L2 段(104 个中 87 个)被隔离——写者在 500 MiB 自行滚动,新段以增量开头、没有快照,无法独立回放;约 250 个隔离包至今需要多段 seed 才能恢复。

**规则**:

1. 任何段的开头必须自足:所有活跃簿的快照 + 全部状态快照 + 当前 `ConfigBlob`。读者从任意段开头都能重建。
2. 滚动是 sequencer 的事件,在两个事件之间完成,不是写者的私事;快照从本地镜像导出,不发 REST、不重连。
3. 大小只做安全阀:先在空闲轮滚动,超过硬上限才在事件边界强制滚动(两级阈值归 [`hft-journal`](../crates/hft-journal/README.md#写者));两种都走"快照先行"的滚动。
4. 段头链(`prev_header_sha256`)让离线校验发现缺段。

**落点**:[`hft-journal`](../crates/hft-journal/README.md) 的段格式;[`hft-sequencer`](../crates/hft-sequencer/README.md) 的整点滚动与周期关键帧(一次写完,触发条件归该 crate);[journal-format](subsystems/journal-format.md)。

**验证**:测试"从任意段开头回放得到与连续回放相同的状态哈希";触到大小上限的滚动同样满足。

## 状态只有一个写者,不复制仓位

**事故**:cdex 测试/正式两个实例、多个 Python 进程各持一份仓位副本;补丁要靠 `sync-code.sh` 同步两套代码,对账变成三方猜测。

**规则**:

1. 业务状态(簿、仓位、订单槽、风控计数、策略状态)只在 sequencer 线程;其他线程与进程只读 journal 重建。
2. 交易所账户只在 `instance.name = "prod"` 的 hft-core 实例里,各自限额、各自 kill;`prod` 不含 Sim 账户。模拟账户只在独立的 `instance.name = "sim"` 实例——同一二进制,refdata 只含 Sim 账户、不加载任何交易所 key、有自己的行情连接与 journal,不交易任何交易所账户,所以不违反下一条;dev2 长期运行 `sim`,测试网联调窗口另起只持测试网 key 的 `prod` 角色实例(dev2 不放正式实盘账户);统一升级主机上 `prod` 与 `sim` 同机,按核、slice、目录、网卡(ENI-C)与出口 IP 隔离([sim-only-instance](../.agents/notes/proposed/architecture/2026-10-03-sim-only-instance.md)、[sim-colocated-with-prod](../.agents/notes/proposed/architecture/2026-10-03-sim-colocated-with-prod.md))。
3. 同一交易所账户同一时刻只由一个系统交易:ccex 策略用新开的子账户上线,不与旧框架共用账户;cdex 迁移(optional)是切换,不并行([runbooks/migration](runbooks/migration.md))。

**落点**:[architecture.md §5](architecture.md#5-状态所有权);`hft-risk`/`hft-oms`/`hft-ledger` 以 `AccountId` 为一等维度。

**验证**:DST 不变量 6(状态 = f(journal));`deps` 检查冷进程不链接可写的状态结构。

## 文件系统所有权:逐级创建、单一属主、以服务用户演练

**事故**:2026-08-31 定时 24 h 采集在 T+1 s 因 EACCES 失败、零数据,一天后才发现——`install -d -o USER deep/path` 只把属主给最深一级,隐式创建的父目录属于 root;当天所有验证都以 root 运行,问题不可见;`Requires=` 拉起坏服务后在 `Restart=always` 下 crash-loop 57,915 次。另:2026-09-06/07 两个 systemd 单元都声明 `RuntimeDirectory=trading`,每次启动递归改属主,互相抢 socket。

**规则**:

1. 部署脚本逐级显式创建每一层目录并设属主与权限,不依赖 `install -d`/`mkdir -p` 的隐式父目录。
2. 每个运行期目录只有一个单元声明所有权;其他单元只"存在即用、不存在则按所有者属性创建"。
3. 任何无人值守运行之前,以**服务用户**跑一次完全相同的单元命令(`sudo -u <user> … --run-for 8s`);以 root 通过的验证不算数。
4. "依赖作业失败"是根因线索,立即追到失败单元的日志;调度不是交付,验证过的第一分钟才是。

**落点**:[`deploy/`](../deploy/README.md) 的 provision 脚本与 systemd 单元;[runbooks](runbooks/README.md) 的上线检查单。

**验证**:部署 CI 在容器里以服务用户执行 provision + 单元 dry-run;`systemd-analyze verify`;检查脚本中不出现 `install -d` 多级路径。

## CI 条件显式判定上游结果

**事故**:2026-08-30 前后 v0.2.4——GitHub Actions 工作流显示 success,但所有有意义的作业都被跳过,发布资产没有生成。作业条件里隐式的 `success()` 遍历整条 `needs` 传递链,一个有意跳过的上游作业让下游全部静默跳过。

**规则**:

1. 作业条件显式写 `needs.<job>.result == 'success'`(或列举允许的结果),不依赖隐式 `success()`。
2. 改动作业图条件后,验证**每个作业**的结论,不看工作流总结论。
3. 发布工作流的最后一步断言资产存在且哈希匹配;缺资产即失败。
4. 公开仓库的标准托管 runner 分钟免费;作业仍有超时、冒烟批量与制品保留期限。arm runner 故障时保留定期演练的 x86 交叉编译发布路径;资源规则归 [development.md](development.md#免费额度)。

**落点**:`.github/workflows/`;[development.md](development.md#ci-概要)。

**验证**:`actionlint`;CI 自检作业读取本次运行全部作业结论,任何被跳过的必需作业使运行失败。

## 参考数据有版本,刷新不能静默丢标的

**事故**:2026-09-07 目录刷新定时器从未运行,OKX `CXMT-USDT-SWAP` 的 `price_tick` 停在 0.01(交易所已改 0.001),对应分片 crash-loop;2026-09-15 24 h 采集中 BTCUSDC 5 分钟后丢失——周期刷新只接受 USDT 计价,`live = refreshed ∩ planned` 把计划内标的永久剔除。

**规则**:

1. refdata 是带版本的完整 blob,作为 `ConfigBlob` 记录;不在热进程内做部分刷新。由 [`hft-cfg`](../tools/README.md#hft-cfg) 从手写 TOML 生成(唯一编码器;pipeline 产出名单与 `StrategyParams`,不编码 refdata)。refdata 变更 = `ReplaceRefdata`(Ops、维护窗口、确认)写入 blob 后 Drain + 重启(冷启动 < 30 s),refdata 预装交易宇宙超集;已存在 `AssetId` 的指数、`InstId` 的合约乘数变化、容量缩小一律拒绝,名单与阈值在可热替换的 `StrategyParams` 里([refdata-change-is-restart](../.agents/notes/proposed/architecture/2026-10-01-refdata-change-is-restart.md))。
2. 新 blob 与当前 blob 比较:计划内标的消失、tick/lot/指数变化必须显式列出并需要确认,不静默生效。
3. refdata 过期(超过刷新周期未更新)是告警,不是继续用旧值的理由。

**落点**:[`hft-refdata`](../crates/hft-refdata/README.md);[`hft-cfg`](../tools/README.md#hft-cfg) 的差异报告。

**验证**:单元测试"新 blob 缺少计划内标的 → 拒绝并返回差异";`watchtower` 的 refdata 年龄告警。

## 一个状态格不能同时存期望与降级

**来源**:2026-10-01 多模式多账户专项评审。

**缺陷**:模式表的一格同时存"操作者要的模式"和"系统因故压低到的模式"。自动降级覆写原值后,恢复时不知道该回到哪里;两个降级来源(日亏 + 对账偏差)叠在同一格,解除其中一个就把另一个也解除了;`SetMode` 一写就把未解除的降级抹掉。恢复因此不精确:要么回不去,要么回过头。

**规则**:

1. 模式表只存**期望模式**,只由 `SetMode` 写。kill、自动降级、恢复锁存、条件门都是**独立的 cap 记录** `(source, scope, set_seq, params)`,互不覆盖。
2. **有效模式** = 期望为 `Shadow` 时取 `Shadow`;否则取 min(期望, 全部生效 cap)。
3. `SetMode` 不清任何 cap;`Resume{source, scope}` 只删 (source, scope) 完全匹配的那一条;kill 字由剩余 cap 重新计算。
4. 每条 cap 与期望模式都进快照,回放得到同一组 cap。

**落点**:[`hft-risk`](../crates/hft-risk/README.md) 维护 cap 集合与有效模式;层级与放行表的唯一定义在 [control-plane](subsystems/control-plane.md);理由见 [desired-mode-and-caps](../.agents/notes/proposed/architecture/2026-10-01-desired-mode-and-caps.md)。

**验证**:DST"多个 cap 叠加时 `Resume` 只删匹配的一条";快照含 cap 记录且回放一致([testing §4.1](testing.md#41-模式账户与人工干预))。

## Shadow 不能冻结出场

**来源**:2026-10-01 多模式多账户专项评审。

**缺陷**:有仓位时把策略切到 `Shadow`,策略意图只记录不出环——平仓、止损、资金费转负的退出全部被冻结,仓位变成没人管的裸敞口。降级命令反而制造了风险。

**规则**:

1. `SetMode→Shadow` 只在该 (strategy, account, group) 的净仓、在途订单、LegTracker 残余**全为 0** 时生效;否则有效模式为 `CloseOnly`,`CmdResult.detail = ShadowDeferred`,归零后自动落到 `Shadow` 并写 Note。
2. `SetMode` 的命令类别按目标值判定([control-plane](subsystems/control-plane.md)):`→ Live` 是升级(需确认);`Shadow → CloseOnly` 是降级,但从此该 cell 的减仓意图会真实发出,命令预览写明这一点。
3. 有效模式每次变化都向策略投递 Mode 事件;降到 `CloseOnly`/`Shadow` 时撤销该 cell 的 Open 类挂单,平仓单照常。
4. 同一原则适用于全部门控:`ClockBad`、`Halted` 等只拦 Open;Flatten 与硬动作绕过 cooldown 与 in_flight 限制([`hft-risk`](../crates/hft-risk/README.md)、[`hft-strat-lib`](../crates/hft-strat-lib/README.md) 的 `Throttle`)。

**落点**:[`hft-strategy-sdk`](../crates/hft-strategy-sdk/README.md) 的运行模式意图行为表;`hft-risk` 的有效模式计算。

**验证**:DST"Shadow 有仓位时 `ShadowDeferred` 且平仓照常";"`ClockBad` 期间策略 Flatten 仍放行"。

## 命令不默认落在视图账户

**来源**:2026-10-01 多模式多账户专项评审。

**缺陷**:ops_bot 有"当前视图账户"(`/account` 切换)。升级命令缺省账户参数时落在视图账户上——操作者以为在测试账户上 `Live`,实际改的是正式账户;或者两步确认之间别人切了视图或改了模式,确认的不再是预览过的状态。

**规则**:

1. 降级类命令不带账户参数时作用于**全部账户**;升级与中性命令必须显式写账户名,缺省即拒。
2. gateway 要求 `CmdHdr.account == CmdScope.account`,否则 `InvalidArgs`;`cmd_hash` 覆盖二者与完整 `CmdScope`。
3. 确认预览第一行显示目标账户标签与其他账户的当前有效模式;升级命令携带 PREVIEW 返回的 `ctl_state_version` 作为 `expect_state_version`(每次 SetMode/Kill/Resume/cap 变化 +1),sequencer 复核,版本变了即拒([control-plane](subsystems/control-plane.md));唯一例外是 Viewer 的紧急停止(只绑 `cmd_hash` + TTL,[上文](#紧急停止在引擎不健康时也要可达))。
4. 审计记录 `view_account` 与 `chat_type`。

**落点**:[`hft-ctl-proto`](../crates/hft-ctl-proto/README.md) 的命令信封与权限表;确认与 `ctl_state_version` 校验在 sequencer(`hft-risk::mode`),gateway 不复制状态;[`ops_bot`](../python/ops_bot/README.md);[control-plane](subsystems/control-plane.md)。

**验证**:ctl-gateway 测试"命令账户字段不一致被拒";sequencer 测试"`expect_state_version` 与当前 `ctl_state_version` 不符被拒"。

## 模拟上的试验不得流到实盘:覆盖项按实例分键

**来源**:2026-10-03 M0-b 独立评审。

**缺陷**:pipeline 校准时 `sim` 与 `prod` 两个实例共享同一份人工覆盖项。有人在模拟实例上试一个激进参数,下一轮 pipeline 就把它合进实盘实例的参数;pipeline 主体的 `SetParams` 又是中性命令、不需确认——没人按过确认键,实盘就换了参数。

**规则**:

1. 人工覆盖项按实例分键:`overrides/<instance>.toml`;pipeline 为某实例生成参数时只合并该实例的覆盖项。
2. 校准对共享覆盖项的写入必须显式 `scope = pair`(默认 `instance`);没有显式声明的写入只作用于发起它的实例。
3. pipeline 主体的 `SetParams`(含新增 group、`open_allowed` 0→1)在 Live 账户上仍是中性命令;ops_bot `/params <实例> set` 只写该实例的覆盖项,作用于 Live 实例的人工修改照常需要确认。

**落点**:[`pipeline`](../python/pipeline/README.md) 的覆盖项合并;[`ops_bot`](../python/ops_bot/README.md) 的 `/params`;[control-plane](subsystems/control-plane.md) 的中性/升级分类。

**验证**:pipeline 测试"只写 `overrides/sim.toml` → prod 实例的参数 blob 逐字节不变";"`scope = pair` 的校准写入 → 两个实例都变,且审计记录写明 scope";ops_bot 测试"`/params prod set` 未确认 → 不下发"。

## 人工干预必须入账,解除不自动对冲

**来源**:2026-10-01 多模式多账户专项评审;对应现有 `/hedgefix` 与"交易所网页手工平仓"的工作流。

**缺陷**:操作者在交易所网页上手工平仓或补腿,journal 里没有这件事。系统按自己的账本仍认为有未对冲量,解除 kill 或锁存时自动 `HedgeFix` 再补一次——重复对冲,手工修好的敞口被反向打开。

**规则**:

1. 解除任何 kill 或锁存**本身都不触发自动 `HedgeFix`**。只有解除 HARD 时,裸量超过补腿阈值的 group 标为 Broken(`resolve_broken`;阈值与跳过 `manual_hold` 的判定归 [`hft-exec`](../crates/hft-exec/README.md),调用点归 hft-sequencer),由 `HedgeFix` 命令处理(预览基于最新对账),持续 `broken_auto_fix_ms` 无人处理时才由执行层做一次有界平仓([裸腿有界](#裸腿有界断腿自动平仓l3-与-flatten-不跳过))。非 HARD 的解除与 Recovery 的解除不判 Broken:这些锁存期间风险递减单本来照常放行,补腿与外来成交默认处理按放行后的新窗口继续。
2. 人工在交易所操作之后,用升级类命令 `AdoptExchange` 把对账差写成 ledger 调整(class = `Adjust`;可采纳仓位与余额条目,可带入场价)——人工动作因此进入 journal,账本与交易所一致后才能继续。非策略资产用 `SetBalanceBaseline` 登记为基线,不成为永久差异。`Annotate` 同样提供。
3. 对账恢复条件只有一个定义:同一 (account, venue) 连续两轮 Ignore,且期间该 venue 无 UserStream Down。启动闸门 `RECONCILED` 与 HARD 解除更严:最新一轮对账全部 Ignore,或每项差异已经 `AdoptExchange` 或 `SetBalanceBaseline` 处置;任何量级的差异都不按 Warn 放行([`hft-risk`](../crates/hft-risk/README.md))。资金费条目不进该闸门([资金费差异只告警](#资金费差异只告警账本按流水校正))。

**落点**:[`hft-ledger`](../crates/hft-ledger/README.md) 的 `Adjust` 类;[control-plane](subsystems/control-plane.md);理由见 [adopt-exchange-manual-intervention](../.agents/notes/proposed/architecture/2026-10-01-adopt-exchange-manual-intervention.md)。

**验证**:DST"手工平仓后 `AdoptExchange`,随后 `Resume`,不产生任何对冲单";"干净重启或解除非 HARD 的 Kill 时 group 有容差内残余 → 无 Broken、无 `HedgeFailure`";属性测试:`Resume` 从不生成 `OutMsg`。

## 撤单/查单的发送失败不是目标单的失败

**来源**:2026-10-01 全局评审(场景评审 B-1、B-12、B-13)。

**缺陷**:gw 回 `SendReport{非 Written}` 时,OMS 不看是哪一种操作,一律把槽迁到 `Rejected`。撤单帧没发出去,却把一张仍挂在交易所的单标成"已拒绝"、释放了额度——之后这张单成交,就是账外仓位。同类:只带手续费的回报被当成状态回报去迁移槽;回报解码失败被静默丢掉,用户流里出现看不见的缺口。

**规则**:

1. 发送结果只作用于它所指的那一个操作:`SendReport` 带 `op`、`target_slot`、`target_gen`;非 Written 只对 `op = Place` 把槽迁到 `Rejected`;`Cancel`/`Amend`/`Query`/`CancelAll` 失败只清 `CANCEL_SENT`/`AMEND_SENT` 并写 Note,目标槽状态不变。
2. 只有费用的回报是独立种类 `FeeOnly`:任何状态下只把费用交给 ledger,不迁移状态、不请求对账;代数不符时按 (venue, account, exch_order_id) 入账。
3. 回报解码失败按用户流缺口处理:该 (venue, account) 的非终态槽置 VERIFY 并查询;连续 N 次按 UserStream Down。`VenueCaps.ignorable_user_events` 列出的事件与未知事件类型只计数,不算解码失败——否则一个新增的推送类型就会引发查询风暴并断流。

**落点**:[`hft-oms`](../crates/hft-oms/README.md) 的槽状态机;[`hft-event`](../crates/hft-event/README.md) 的 `SendReport`、`OrderEventKind::FeeOnly`;[`hft-venue`](../crates/hft-venue/README.md) 的回报解码。

**验证**:[testing §4.2](testing.md#42-恢复oms-与对账场景) 的 B-1、B-12 用例;OMS 属性测试"任何非 Place 的发送失败都不改变目标槽状态"。

## 自动补腿要把在途算进去

**来源**:2026-10-01 全局评审(场景评审 B-4)。

**缺陷**:LegTracker 只拿"已成交"算裸敞口。第二腿已经发出、还没回报时,定时检查看到敞口仍在,又发一笔 `HedgeFix`——两笔都成交,对冲过量,反向打开一条新腿。ClosePolicy 的 `hedge_fix` 动作有同样的问题。

**规则**:

1. 裸敞口 = 已成交 + 在途:`hedge_sent − hedge_filled`,以及 Pending/Unknown 槽的剩余量。
2. group 内存在 Open/Hedging 的 LinkState、Pending/Unknown 槽、或 Broken/Reorg cap 时,或该 cell 处于 `manual_hold`(置位与解除归 [`hft-exec`](../crates/hft-exec/README.md),见[自己的强平与 ADL 也是成交](#自己的强平与-adl-也是成交) 第 2 条)时,不发任何自动 `HedgeFix`(包括策略包自己的补腿动作);Broken 的唯一自动动作是[有界平仓](#裸腿有界断腿自动平仓l3-与-flatten-不跳过)。
3. 解除 kill 或锁存不触发自动补腿([人工干预必须入账](#人工干预必须入账解除不自动对冲))。
4. 自动补腿有界:复用对冲重试的计次、冷却与截止;单笔补腿量 ≤ 触发成交净量 × ratio + 1 lot、名义 ≤ `max_hedge_notional`,超限即 Broken + P0、不发单;HARD 下抑制。

**落点**:[`hft-oms`](../crates/hft-oms/README.md) 的 LegTracker;[`hft-exec`](../crates/hft-exec/README.md) 的补腿与对冲量上限;策略包自己的补腿动作同样受本节约束。

**验证**:DST"第二腿在途时定时补腿检查触发 → 不发单";"门控拒绝下补腿不自激循环";"解码放大 1000 倍的成交 → 对冲被上限拦下并 Broken";testing 不变量 8(第二腿数量 = 实际第一腿成交)。

## 失败重试要有时间维度

**来源**:2026-10-01 全局评审(场景评审 B-5、B-10)。

**缺陷**:第二腿失败后只在"下一个事件"时重试:行情安静时永不重试,行情汹涌时每个 tick 都重试(打满限速);重试只计次数不计时长,门控拒绝(kill、cap)让它无限期挂着,却没有任何告警。同类:kill 与 Drain 的撤单只在生效那一刻发一次,之后重连或 `Unknown` 收敛冒出来的挂单没人撤。

**规则**:

1. 每次失败(风控拒、place 出错、本地发送失败、交易所拒)登记 OMS 定时器 `HedgeRetry`,间隔 max(100 ms, `backoff_until`);重试由定时器驱动,不依赖行情事件。
2. 门控拒绝不计入次数,但**累计时长**;超过 `hedge_deadline_ms` 仍未对冲 → Broken cap + P0。该时长只按引擎运行时间累计,停机不计([裸腿有界](#裸腿有界断腿自动平仓l3-与-flatten-不跳过) 第 4 条)。
3. Kill cap 与 `DRAINING` cap 带 `cancel_resting = true` 生效期间,撤单是**持续义务**:每次 `ConnState` Up、每次 `Unknown` 收敛为 Live,都重发撤单。
4. Drain 不带着裸腿退出:等全部 Open/Hedging 链接结束、全部 IOC 到终态、`HedgeRetry` 计时器清空,并且没有未到达的 `MarginStress`/`Hard` 引擎目标(`exec.drain_ready()`,受 `drain_timeout_ms` 约束)后才写 Drain 快照;Drain 期间执行层只停增仓切片。

**落点**:[`hft-oms`](../crates/hft-oms/README.md)(`HedgeRetry`);[`hft-risk`](../crates/hft-risk/README.md)(cap 生效期间的撤单义务);[`hft-exec`](../crates/hft-exec/README.md)(`drain_ready`);[`hft-sequencer`](../crates/hft-sequencer/README.md)(Drain 等待条件与超时)。

**验证**:DST"第二腿被拒后行情静止 → 仍按定时器重试";"kill 期间断线重连 → 重发撤单";"门控持续超过期限 → Broken + P0";"Drain 时第二腿处于重试间隙 → 等其终态后才退出";"Drain 中发 `Kill{flatten}` → 清仓到达目标后才退出,或 `drain_timeout_ms` 到期 P0"。

## 安静 ≠ 过期:报价可用性看连接健康

**来源**:2026-10-03 M0-b 独立评审。

**缺陷**:报价新鲜度按"该 inst 最后一次变化"判定。行情安静时第二腿盘口 0.5 s 不变 → 对冲定价判为报价过期 → 数秒后 Broken → 自动平仓的每一片也要求报价"新" → 失败。越是平静的市场越平不掉仓;而盘口不变本来就说明最新报价仍然有效。

**规则**:

1. 报价可用 = 该 inst 的 BBO 成员连接中至少一条 Up、未在重同步(`BboView.resync = false`:成员连接、inst 与簿都不在重同步)、`now − conn_live_tsc ≤ conn_liveness_ms`;`conn_live_tsc` 取 Up 成员连接最近一次收到**任何**帧(数据、pong、心跳)的时刻的最大值。报价取最新一条 Bbo,盘口无变化即有效。成员维护与聚合规则归 SDK `BboView` 与 [`hft-sequencer`](../crates/hft-sequencer/README.md)(赛马胜出的连接退役不使安静 inst 失效)。
2. hft-venue 对 BBO 连接每 `bbo_ping_ms` 发 WS ping,保证安静时也有回帧;收不到回帧就是连接问题,按断连与重同步处理。
3. 对冲腿定价、风险递减单、Broken 自动平仓、MarginStress L3 减仓、链接腿的第二腿检查一律用此判定;不按 inst 最后变化时刻判过期(`max_mid_age_ms` 一类阈值退役)。
4. 判定所需的连接健康信息作为 Input 进入 sequencer,回放得到同一判定。

**落点**:[`hft-strategy-sdk`](../crates/hft-strategy-sdk/README.md) 的 `BboView`;[`hft-venue`](../crates/hft-venue/README.md) 的 BBO 连接保活;[`hft-risk`](../crates/hft-risk/README.md) 的链接腿检查与价格带;[`hft-oms`](../crates/hft-oms/README.md) 的 `price_hedge`;[`hft-exec`](../crates/hft-exec/README.md) 的切片定价;[`hft-strat-lib`](../crates/hft-strat-lib/README.md)("安静不算过期");理由见 [quote-freshness-conn-health](../.agents/notes/proposed/architecture/2026-10-03-quote-freshness-conn-health.md)。

**验证**:DST 必过"第二腿盘口 10 s 不变"——开仓、对冲与自动平仓照常;"BBO 连接 `conn_liveness_ms` 内无任何帧 → `LINK_STALE`,对冲等待且不计失败次数";"连接重同步期间 → 不可用";金样覆盖各所 BBO 连接的 pong 帧。

## 第二腿先检查再开第一腿

**来源**:2026-10-03 M0 独立评审。

**缺陷**:两腿链接发第一腿之前只检查第一腿。第二腿的报价不可用、或第二腿账户的钱不够,第一腿照样发出——成交后第二腿必然被拒或超滑点,几秒内落到 Broken,只剩人工处理。同类:对冲腿偏离超过 `max_slip` 就算执行失败,越重试越接近截止;没有 client id 的成交先按外来成交处理、随后又匹配到槽,同一笔成交被对冲两次。

**规则**:

1. 链接腿检查覆盖第二腿,并在第一腿发出之前判定:第二腿 venue 的报价按连接健康可用([上一节](#安静--过期报价可用性看连接健康)),否则 `LINK_STALE`;第二腿可负担——现货买入腿的可用计价资产 ≥ `link_notional × (1 + funds_buffer)`,永续腿的可用保证金 ≥ `link_notional / 杠杆 × (1 + funds_buffer)`,杠杆取 `AccountConfigReport` 读到的该账户实际杠杆(不设常量),否则 `LINK_FUNDS`。
2. 对冲腿对手价偏离超过 `max_slip` 时,`price_hedge` 按 `rr_max_cross` 上限定价照常发出(仍是 IOC);只有超过 `rr_max_cross` 才返回门控拒绝,不计执行失败次数(按[失败重试要有时间维度](#失败重试要有时间维度)累计时长)。
3. 第二腿数量 = min(按第一腿成交净量算出的量, LegTracker 该方向的当前裸量):对冲量永远不超过实际裸露的量。
4. gw 把无法归属到槽的成交暂存 `gw.unmatched_hold_ms`,超时才发 `ExternalFill{cause: Unknown}`;之后查到归属时,OMS 与 ledger 按成交去重键(契约 22:`(inst, trade_id)`,成交 id 缺失或 ≤ 0 时 `(inst, exch_order_id, fill_seq)`,[hft-venue-proto](../crates/hft-venue-proto/README.md))去重,不重复入账、不重复对冲。

**落点**:[`hft-risk`](../crates/hft-risk/README.md) 的链接腿检查与 `RiskParams`;[`hft-types`](../crates/hft-types/README.md) 的 `RejectBits`(`LINK_STALE`、`LINK_FUNDS`);[`hft-oms`](../crates/hft-oms/README.md) 的 `price_hedge` 与 LegTracker;[`hft-exec`](../crates/hft-exec/README.md) 的两腿 OnFill;[`hft-venue`](../crates/hft-venue/README.md) 的未匹配成交暂存。

**验证**:DST"第二腿 BBO 连接不健康 → 第一腿不发,`LINK_STALE`";"第二腿余额不足 → `LINK_FUNDS`";"账户实际杠杆变小 → `LINK_FUNDS` 按新杠杆判定";"对冲腿偏离介于 `max_slip` 与 `rr_max_cross` 之间 → 按上限价发出、不计失败";"无 client id 的成交在暂存期内匹配到槽 → 只对冲一次";属性测试"第二腿数量 ≤ 裸量"。

## 两腿数量按 lot 对齐,残余不能对冲就不开

**来源**:2026-10-03 M0-b 独立评审。

**缺陷**:两腿的 lot 与最小量不一样(例:现货一片 0.004 BTC,对面永续一张 0.01 BTC)。第一腿按自己的 lot 切片,第二腿按自己的 lot 取整后要么是 0、要么不足 `min_qty`——每一片都留下一截无法对冲的残余,切得越细残余越多。

**规则**:

1. 切片与第一腿数量按两腿 lot 的较粗者取整(一方的 lot 整除另一方时),保证第二腿数量 ≥ 其 `min_qty` 且为其 lot 的整数倍。
2. 两个 lot 互不整除时取最小公倍数;最小公倍数的名义 > `max_chunk_notional` → `SetParams` 拒绝该 `GroupSpec`(`InvalidArgs{LotMismatch}`,sequencer 通用校验),不在运行中才发现。
3. 第二腿实际数量仍受"≤ 当前裸量"约束(上一节第 3 条)。

**落点**:[`hft-exec`](../crates/hft-exec/README.md) 的切片;[`hft-sequencer`](../crates/hft-sequencer/README.md) 的 `SetParams` 通用校验;[control-plane](subsystems/control-plane.md) 的 `LotMismatch` detail 码;[`hft-oms`](../crates/hft-oms/README.md) 的第二腿数量。

**验证**:属性测试"任意 lot 组合下每片第二腿数量是其 lot 的整数倍且 ≥ `min_qty`";ctl 测试"最小公倍数名义超过 `max_chunk_notional` → `InvalidArgs{LotMismatch}`";DST"现货 0.004 对永续 0.01 的组合运行 1,000 片 → 无因 lot 不齐产生的残余"。

## 裸腿有界:断腿自动平仓,L3 与 flatten 不跳过

**来源**:2026-10-03 M0 与 M0-d 评审。

**缺陷**:对冲到截止仍失败后 group 进入 Broken,之后只能等人发 `HedgeFix`——夜里可能裸几个小时。保证金 L3 的默认减仓与 `Kill{flatten}` 又恰好跳过 Broken 的 group,最危险的裸永续反而不减。冷启动更糟:恢复加对账就超过 `hedge_deadline_ms`,重启本身把在途链接判成 Broken。

**规则**:

1. 某 (account, group) 处于 Broken 持续 `broken_auto_fix_ms`(`ExecConfig`)且期间无人工 `HedgeFix`、不在 `manual_hold`:执行层自动做**一次可执行的尝试**"平掉多出的一腿"——永续腿 reduce-only,现货腿卖出量取 min(库存, 可用余额);按 `max_reduce_notional` 切片,总量 ≤ 当前裸量。"一次"按可执行的尝试计:开始前的等待(报价可用、限速退避、越价上限等)不算失败;开始后门控拒绝与零成交按退避重试;只有 hft-exec 列出的失败原因判失败(等待条件、失败原因码与参数都归 hft-exec)。成功的判定(每片终态后判断):**已成交**裸量的名义不高于尘埃线 `dust_notional`(定义归 hft-exec),且该 group 无 Pending*/Unknown 槽、无在途单——在途的平仓单不算已对冲。成功 → `Note{ExecAutoFixDone}`(P1),cell 回到 Idle;账户级 `HedgeFailure` cap 仍需人工 `Resume`,且前置条件是该账户已无 Broken cell(否则 `CmdResult{Rejected, detail = BrokenRemaining}`)。失败 → `Note{ExecAutoFixFailed}`(P0),保持 Broken 等人工。HARD kill 下不执行。
   - **残余小到下不了单不能卡死**:hft-core 装配时校验 `dust_notional` ≥ 各交易腿最小名义的最大值(否则退出 2,`DustBelowMinNotional`);仍有低于最小下单量、平不掉的残余时,Admin 用升级类命令 `WriteOffResidual{account, group}` 核销:适用于静止的 group(Broken 与否均可),把多出那一腿的已成交裸量移到 unassigned(ledger `AdjustDelta::WriteOff`)并清零该 group 全部腿的残余,Broken 随之可解除(语义归 [control-plane](subsystems/control-plane.md),静止的定义归 [`hft-oms`](../crates/hft-oms/README.md))。
2. MarginStress L3 的 `force_deleverage` 与 `engine_flatten`(`Kill{flatten}`)对 Broken cell 同样生效:先对永续腿 reduce-only 减仓(`PerpFirst`,受 `max_reduce_notional`),再处理现货腿;自动平仓、L3 与 flatten 的每一片按连接健康判定报价可用([安静 ≠ 过期](#安静--过期报价可用性看连接健康))。**策略挂单不能挡住引擎减仓**:设 `reason ∈ {MarginStress, Hard}` 的引擎目标时,先经 OMS `cancel_resting_on` 撤该 inst 上本账户全部非 EXEC 挂单(否则策略的 reduce-only 挂单会让引擎的平仓单被风控按"超过可减仓量"拒掉);撤单确认前切片量按 `|pos| − 即时在途` 截取。L3 越价上限按 inst 所属价格带类别取值,不取账户内最小值([`hft-risk`](../crates/hft-risk/README.md))。
3. **L3 与 flatten 不依赖另一腿报价**:对非 Broken 的 cell,`reason ∈ {MarginStress, Hard}` 只要求永续腿(第一腿)报价可用;第二腿不可执行(报价不可用、venue 未就绪或下单连接断开;恢复后在途链接仍在续接等待时同样按不可执行处理,判定归 hft-exec)时退回与 Broken 相同的串行路径 `SerialFallback`(永续 reduce-only 先减,现货腿在其报价恢复后再减),写 `Note{ExecSerialFallback}`(P0),进入后直到该 cell 到达引擎目标不切回配对执行——接受短时现货裸多,换取永续保证金不被拖到强平。L3 与 flatten 同样作用于该账户 `GROUP_NONE` 的永续持仓。
4. `hedge_deadline_ms` 只按引擎运行时间累计,停机时间不计;对仍未对冲的链接先做一次 `SecondRetry`,再按正常截止判断。恢复后,补腿、残余修剪、外来成交默认处理与自动有界平仓按**多出那一腿所在 venue** 开始(该 venue 的对冲闸门子集满足且已对账,不等整个账户的启动闸门),条件不满足期间 `broken_auto_fix_ms` 与外来成交截止不计时;账户启动闸门长期未放行而仍有 Broken 或超阈值裸量 → P0(`Note{ExecGateHeldNaked}`)。条件与时限归 hft-exec。
5. 解除 kill 或锁存本身不触发任何对冲或平仓([人工干预必须入账](#人工干预必须入账解除不自动对冲));Broken 的自动动作只有第 1 条这一次(加上第 2、3 条的引擎减仓)。

**落点**:[`hft-exec`](../crates/hft-exec/README.md)(`broken_auto_fix_ms`、自动平仓的尝试与失败判定、L3 与 flatten 对 Broken 的处理、`SerialFallback` 条件);[`hft-oms`](../crates/hft-oms/README.md)(截止按运行时间累计、`SecondRetry`、`cancel_resting_on`);[`hft-ledger`](../crates/hft-ledger/README.md)(`AdjustDelta::WriteOff`);[`hft-risk`](../crates/hft-risk/README.md)(Broken 与 `HedgeFailure` cap);[runbooks](runbooks/README.md);理由见 [broken-auto-fix](../.agents/notes/proposed/architecture/2026-10-03-broken-auto-fix.md)。

**验证**:DST"Broken 持续 `broken_auto_fix_ms` 无人处理 → 平掉多出的一腿,总量 ≤ 裸量,`ExecAutoFixDone`";"自动平仓遇门控拒绝、报价不可用或零成交 → 等待或重试,不判失败;交易所拒单 → `ExecAutoFixFailed`(P0),保持 Broken";"残余低于最小下单量 → `WriteOffResidual` 后 Broken 可解除";"L3 时策略挂满 reduce-only 平仓单 → 先撤单,仍按步减";"HARD kill 下的 Broken → 不发单";"L3 时有 Broken group → 永续腿先 reduce-only 减仓";"`Kill{flatten}` 覆盖 Broken";必过用例 MS-3"L3 + 另一所 BBO 断 30 s → 永续仍按步减"([testing §4.4](testing.md#44-断腿第二腿保证金资金费与建议新鲜度用例));"自动平仓 IOC 零成交 → 不判成功,保持 Broken";"停机跨过 `hedge_deadline_ms` 后恢复 → 先 `SecondRetry`,不立即 Broken"。

## 清仓锁:flatten 之后只接受更接近 0 的目标

**来源**:2026-10-03 M0-b 独立评审。

**缺陷**:`Kill{Account, flatten}` 让执行层按引擎默认减仓把该账户清到 0,但它不改变策略的目标意图。策略在下一个事件里重新给出加仓目标,执行层就把刚平掉的仓位又开回来——操作者以为在清仓,实际在来回交易。

**规则**:

1. `engine_flatten` 置账户级 `flatten_lock`;直到 `Resume{Kill}` 前,该账户只接受比当前持仓更接近 0 的目标与 Reduce/Flatten 意图,其他目标与 Open 意图被拒并写 Note。
2. `flatten_lock` 进快照,恢复后仍然生效;只有 `Resume{Kill}` 解除它,解除本身不触发任何下单([人工干预必须入账](#人工干预必须入账解除不自动对冲))。

**落点**:[`hft-exec`](../crates/hft-exec/README.md) 的 `engine_flatten` 与目标接受规则;[`hft-risk`](../crates/hft-risk/README.md) 的 Kill cap 与 `Resume`。

**验证**:DST"`Kill{flatten}` 后策略给出新的加仓目标 → 被拒,持仓单调趋向 0";"`flatten_lock` 期间的减仓目标 → 照常执行";"重启后 `flatten_lock` 仍在";"`Resume{Kill}` 后不产生任何 `OutMsg`"。

## TSC 不跨开机比较

**来源**:2026-10-01 全局评审(场景评审 B-3、B-8、B-17)。

**缺陷**:快照里存了 TSC 形式的定时器到期点、退避截止、令牌桶时刻。重启(换机、换架构、主机重启)后 TSC 从另一个原点开始,恢复出来的定时器要么立刻全部到期,要么几天后才到期;令牌桶以为刚刚用过或很久没用。策略、ledger、risk 的持久状态同样如此(确认窗口起点、节流、再入场冷却、保证金截止):存下来的 TSC 截止在新的一次启动里没有意义。同类:日切只在"跨过零点那一刻"写一次,恰好停机的那一天永远不切;订单的代数只在内存里递增,重启后与交易所上的旧单撞号。

**规则**:

0. **持久状态不存 TSC**:策略、`hft-strat-lib`、ledger、risk 持久状态里的时刻与期限一律存 `engine_ns`(SDK `Now.engine_ns`:跨重启连续、停机不计,sequencer 由 journal 记录的运行区间累加,回放确定)或 UTC 日历量;TSC 只在单次启动内使用([hot-path-rules D6](hot-path-rules.md#确定性d);理由见 [engine-time](../.agents/notes/proposed/architecture/2026-10-03-engine-time.md))。
1. **唯一例外**:登记在 [hot-path-rules D6](hot-path-rules.md#确定性d) allow-list 的 TSC 行可以进快照(清单与处置归 D6)。OMS、exec 与定时器堆的行(定时器到期点、退避截止、`LinkEntry.first_fill_tsc`、`Tomb.freed_tsc` 等)恢复时按 `recovery_end` 平移(D6 登记的例外除外):停机时间不计,`recovery_end` 的当前时刻取本次运行的 `TscClock` 并写进 Recovery 快照头(归 [`hft-sequencer`](../crates/hft-sequencer/README.md)),平移公式(含 `tsc_hz` 换算)唯一定义在 [`hft-exec` 快照与恢复](../crates/hft-exec/README.md#快照与恢复)。不按 `boot_id` 或 UTC 剩余时长重建;令牌桶时刻存 `engine_ns`;周期性系统定时器每次启动重新登记。
2. 日切按日历而非按"跨过那一刻":快照存 `last_day_rolled`;时钟可信且 UTC 日 > 该值时(含启动后首轮)写 `Timer{DayRoll{day}}`;day ≤ 该值的 DayRoll 忽略。
3. `engine_epoch` 在发出第一笔订单前持久化到冷副本并 fsync,启动时取严格大于快照值与持久值的新值(取值规则归 [`hft-core` 启动顺序](../apps/hft-core/README.md#启动顺序));OMS 按 (epoch, slot, gen) 匹配回报。
4. 版本(含架构)变化的冷启动只在 journal 末尾是 Drain 快照时允许;否则先用旧二进制恢复并 Drain。

**落点**:[`hft-sequencer`](../crates/hft-sequencer/README.md) 的恢复;[`hft-oms`](../crates/hft-oms/README.md) 的 epoch;理由见 [recovery-epoch-and-boot-rules](../.agents/notes/proposed/architecture/2026-10-01-recovery-epoch-and-boot-rules.md)。

**验证**:[testing §4.2](testing.md#42-恢复oms-与对账场景) 的 B-3、B-8、B-17 用例;DST"停机 N 小时(含换机)后恢复 → 策略确认窗口、节流、再入场冷却与保证金截止按停机前剩余的 `engine_ns` 继续,OMS/exec 定时器按停机前剩余的运行时间继续,不立即到期也不延长";`hot-check`:快照节类型中出现 `Tsc` 字段须在 allow-list 登记(只允许 D6 列出的行)。

## 对账条目要带代数与视图时刻

**来源**:2026-10-01 全局评审(场景评审 B-14、B-15、B-16)。

**缺陷**:对账报告说"这张单已收敛"或"这个差额请采纳",却不说它指的是哪一代的槽、是在哪一刻看到的交易所视图。报告在路上的这几百毫秒里,槽已被复用给新订单,或者那个 inst 又成交了一笔——报告被应用到错误的对象上,或把刚刚发生的成交当成差额抹掉。

**规则**:

1. `UnknownResolved`/`OpenOrder` 条目带 `gen` 与 `epoch`;只有完全匹配且槽仍为 Unknown/VERIFY 时才应用。
2. `ReconcileReport` 带 `view_seq`;`AdoptExchange` 的目标 inst 自 `view_seq` 以来有成交 → `PreconditionFailed{StaleReport}`;同一命令同量调整 LegTracker。
3. 对账窗口:S0 有非终态槽或 (S0 − user_stream_lag, S1] 内有事件的 inst 视为在途;单轮差异只给 Warn,CloseOnly 需同一差异连续两轮;`skipped_inflight > 0` 的轮次不计入恢复所需的 Ignore 轮数。分级只在 hft-risk 做一次,reconciler 只报差异。

**落点**:[`reconciler`](../apps/reconciler/README.md);[`hft-risk`](../crates/hft-risk/README.md) 的对账分级;[control-plane](subsystems/control-plane.md) 的 `AdoptExchange`。

**验证**:[testing §4.2](testing.md#42-恢复oms-与对账场景) 的 B-14 用例;属性测试"代数不符的对账条目从不改变槽状态"。

## 资金费差异只告警,账本按流水校正

**来源**:2026-10-03 M0 独立评审。

**缺陷**:资金费的对账差异(交易所只给费率不给逐笔、结算时刻与推送不一致)每期都可能出现。如果它进启动闸门 `RECONCILED` 或触发 `Recon` cap,一条资金费 Warn 就能长期卡住启动或压低模式;账本又没有能写资金费差额的调整类型,只能人工逐笔处理。

**规则**:

1. 资金费条目不进 `RECONCILED` 闸门、不触发 `Recon` cap;差额超过 `funding_diff_alert` 时 watchtower 报 P1。
2. reconciler 的 `Funding` 条目带交易所逐笔资金费(account、venue、inst、settle_ts、amount);ledger 对结算时刻早于 `funding_recon_lag_ms` 的差额自动入 `AdjustDelta::Funding`,写 `Note{FundingCorrected}`(P1);比对窗口为最近 48 h。账本侧没有对应记录(`NoHistory`)的条目在窗口结束后按交易所金额入账并写 `Note{FundingCorrected}`(P1),不要求人工。
3. 资金费会改变结算资产的余额:分级之前,结算资产的余额差先用同一份报告里的 `Funding` 条目修正,资金费差额不经"余额"条目进入 `Recon` cap 或启动闸门。
4. 仓位、余额与挂单的差异仍按上一节与启动闸门处理,不因本条放宽。

**落点**:[`reconciler`](../apps/reconciler/README.md) 的 `Funding` 条目;[`hft-ledger`](../crates/hft-ledger/README.md) 的 `AdjustDelta::Funding`;[`hft-risk`](../crates/hft-risk/README.md) 的启动闸门;[`watchtower`](../apps/watchtower/README.md) 的告警规则;理由见 [funding-recon-alert-only](../.agents/notes/proposed/architecture/2026-10-03-funding-recon-alert-only.md)。

**验证**:DST"资金费差异持续存在 → 闸门照常放行、无 cap、P1 告警";"只有资金费造成的结算资产余额差 → 分级为 Ignore";"`NoHistory` 条目窗口结束 → 自动入账 + `FundingCorrected`";属性测试"自动校正后,滞后窗口之外的账本资金费合计 = 交易所流水合计"。

## 手续费扣基础币时按净量记账

**来源**:2026-10-02 主策略适配评审。

**缺陷**:现货买入的手续费以基础资产扣除时,到账数量小于成交数量。仓位、对冲量、平仓量按毛量算:永续空头比现货多一截;平仓按毛量卖现货被交易所以余额不足拒绝,硬动作"先卖现货"卖不掉,永续空头在行情急涨时也平不掉;对账每轮都报同一笔差异。

**规则**:

1. 每个 (venue, 市场类型) 的扣费规则写在 `VenueCaps.fee_asset_rule`(`Quote` / `ReceivedAsset` / `ReceivedAssetUnlessDiscount{token}` / `Configured(asset)`);回报解码携带手续费资产。
2. `FillDelta.net_qty` 是成交净量;仓位、现货库存、LegTracker、对冲量一律按净量。
3. 费用晚到(`FeeOnly`)时 ledger 与 LegTracker 按差额补正。
4. 现货卖出量取 min(库存, 交易所可用余额)。

**落点**:[`hft-refdata`](../crates/hft-refdata/README.md) 的 `VenueCaps`;[`hft-ledger`](../crates/hft-ledger/README.md) 的 `FillDelta`;[`hft-oms`](../crates/hft-oms/README.md) 的 LegTracker;[`hft-exec`](../crates/hft-exec/README.md) 的对冲与平仓量;各 `venue-*` 的回报解码。

**验证**:属性测试"任意扣费规则与费用到达顺序下,仓位 = 净量之和";DST"扣基础币后全平 → 不被拒、无残余";金样覆盖每个 core venue 回报里的手续费资产字段。

## 自己的强平与 ADL 也是成交

**来源**:2026-10-02 主策略适配评审;2026-10-03 M0-d 评审。

**缺陷**:交易所强平、ADL、人工在网页上的操作产生的成交,client id 不是我们的格式。只记一条外来订单 Note:仓位不更新,LegTracker 以为两腿仍对平,另一腿实际已经裸露,要等对账两轮加人工采纳才发现。同时没有保证金率、强平价、ADL 档的输入,风控看不到正在逼近的强平。

**规则**:

1. venue gw 按 `VenueCaps.own_liquidation_marker` 识别自身强平/ADL,与人工、交割一起解码为 `ExternalFill{cause}`;识别不了来源的外来成交 `cause = Unknown`,同样入账。
   - **用户流缺口里漏掉的 ADL**:reconciler 连续两轮对账都见到"永续同号缩小、无在途、无 Unknown"的差异,且两轮都没有用户流断开、该 (account, venue) 用户流自最近一次 Up 起已稳定至少一个对账周期时,该条目带 `AUTO_EXTERNAL` 标志(置位条件归 [`reconciler`](../apps/reconciler/README.md),sequencer 复核)。sequencer 在这条 `ReconcileReport` Input 内确定地合成一笔 `ExternalFill{cause = Unknown}` 并 P0,**不另写记录**(另写会在回放时被重复应用;规则归 [`hft-sequencer`](../crates/hft-sequencer/README.md#每条-input-的处理顺序process)),按本节第 2 条处理;合成函数在 hft-ledger,冷读者经同一函数看到这笔成交。之后交易所补推的同 inst、同方向强平/ADL 先抵扣 `auto_held`,只入账超出部分,不重复入账([`hft-ledger`](../crates/hft-ledger/README.md#外部成交externalfill))。现货差异与反向差异仍走人工采纳。
2. `ExternalFill` 入 ledger、更新 LegTracker、告警、经 `EventRef::External` 通知策略;策略在 `external_deadline_ms` 内没有处理 → 执行层按"平掉多出的一腿"默认处理。**例外**:`ExternalFill{Manual}`(人工在交易所网页操作)使该 cell 进入 `manual_hold`(写 `Note{ExecManualHold}`,P1):置位期间引擎不做默认处理,也不自动补腿、不做残余修剪、不计自动有界平仓的时间——交易员可能正在网页另一侧补对冲,引擎再动就是重复对冲。置位的例外与解除条件归 [`hft-exec`](../crates/hft-exec/README.md)(策略的 `clear_target` 不解除)。
3. 保证金率、强平价、ADL 档按 `VenueCaps.margin_data` 接收或轮询,写成 `MarginReport` Input;过期数据不当作安全([下一节](#保证金分档降级截止后引擎减仓))。
4. 成交去重键是 `(inst, trade_id)`(交易所的成交 id 按交易对编号,跨品种可能撞号);强平/ADL 回报里为 0 或负的成交 id 不进去重环,改用 `(inst, exch_order_id, fill_seq)`——否则一笔强平会被当成重复成交丢掉。

**落点**:各 `venue-*`(识别方法待核实,方法:各所文档与真实强平/ADL 回报样本,见各 venue README 的开发备忘);[`hft-event`](../crates/hft-event/README.md) 的 `ExternalFill`、`MarginReport`;[`hft-ledger`](../crates/hft-ledger/README.md);[`hft-oms`](../crates/hft-oms/README.md);[`hft-exec`](../crates/hft-exec/README.md) 的默认处理;[`watchtower`](../apps/watchtower/README.md) 的 P0 规则。

**验证**:DST"一腿被强平 → 截止内另一腿被平掉,无裸腿";"外来成交 `cause = Unknown` → 入账 + P0";"人工外来成交 → 不自动平、不自动补腿(`manual_hold`),满足 hft-exec 的解除条件后才恢复;策略 `clear_target` 不解除";"用户流断线期间发生 ADL → 用户流恢复并稳定一个对账周期后,两轮对账 → `AUTO_EXTERNAL` → 合成的 `ExternalFill{Unknown}` 入账(journal 中没有另一条成交记录,冷读者入账一致)并按默认处理平掉多出的一腿;之后补推的真实 ADL 被 `auto_held` 抵扣,不重复入账";"两轮之一带用户流断开 → 不置 `AUTO_EXTERNAL`";金样覆盖各所强平/ADL 回报(拿到真实样本前标待核实,方法见各 venue README 的开发备忘)。

## 保证金分档降级,截止后引擎减仓

**来源**:2026-10-02 主策略适配评审;2026-10-03 M0-b 独立评审。

**缺陷**:永续空 × 现货多的组合在标的普涨时,永续账户的保证金率快速上升,而现货浮盈在另一个交易所,补不过来(评审按雏形默认参数估算:普涨 50–60% 即接近维持保证金)。只有告警没有动作,或者只按单个 group 判断,账户级的强平风险没人管。按"维持保证金 / 保证金余额"比率分档同样不行:比率随价格是 `m(1+X)/(e−X)`,从 L1(0.6)到强平(1.0)只剩约 `0.67·m` 的价格空间——`e = 0.30`、`m = 0.5%` 时 L1 在 +28.93%、L3 在 +29.19%、强平在 +29.35%,引擎几乎没有减仓时间。

**规则**:

1. 度量是到强平的距离:每个 (account, venue) 的 `liq_dist = min over 永续持仓 |liq_px − mark| / mark`(空头即 `(liq_px − mark) / mark`);`liq_px` 取 `MarginReport` 给出的值与 ledger 按持仓和账户实际杠杆的估算中**更保守者**。
2. `MarginStress` cap 分档:L1 `liq_dist ≤ liq_dist_block_open` → 禁开;L2 `≤ liq_dist_reduce` → `RiskEvent::MarginStress` 通知策略,须在 `deleverage_deadline_ms` 内减仓;L3 `≤ liq_dist_force` 或截止已过 → 执行层按步减永续敞口(每步 `force_step`,`PerpFirst`,reason = `MarginStress`),处于 Broken 的 group、`GROUP_NONE` 的永续持仓同样减,另一腿报价不可用时走 `SerialFallback`([裸腿有界](#裸腿有界断腿自动平仓l3-与-flatten-不跳过));降档带回差 `liq_dist_hyst`。
3. "维持/保证金余额"比率保留为交叉校验:比率超过校验阈值时至少按 L2 处理。
4. 保证金数据过期(`margin_stale_ms`)按 L1 处理——看不见不等于安全。引擎订阅按持仓驱动:有账户级持仓或库存的 inst 都由引擎订阅(规则归 [`hft-sequencer`](../crates/hft-sequencer/README.md)),group 之外的持仓同样有标记价与报价,L3 减得动。
5. 永续杠杆按账户实际配置读取(`AccountConfigReport`),不设常量;链接腿的 `LINK_FUNDS` 与 `liq_px` 估算都用它。
6. 保证金驱动与硬动作的退出先减永续(释放保证金);常规退出按流动性选腿顺序。
7. 跨所资金只告警并给划转建议,不自动划转;交易 key 不带提现权限。
8. 档位与默认值的唯一归属是 [`hft-risk`](../crates/hft-risk/README.md)(默认值待标定),引擎不写某个策略的数字。

**落点**:[`hft-risk`](../crates/hft-risk/README.md) 的 MarginStress cap 与 `LINK_FUNDS`;[`hft-ledger`](../crates/hft-ledger/README.md) 的强平价估算;[`hft-exec`](../crates/hft-exec/README.md) 的默认减仓与腿顺序;[`hft-sim`](../crates/hft-sim/README.md) 的压力用例;[`watchtower`](../apps/watchtower/README.md) 的保证金 P0 规则;[`treasury`](../python/treasury/README.md) 的划转建议;理由见 [margin-liq-distance](../.agents/notes/proposed/architecture/2026-10-03-margin-liq-distance.md)。

**验证**:DST"普涨路径:L1 禁开 → L2 通知 → 截止 → L3 按步减仓,永续先于现货";"`MarginReport` 断流超过 `margin_stale_ms` → L1";"交易所 `liq_px` 比 ledger 估算乐观 → 按 ledger 估算分档";hft-sim 压力用例:[上线门槛](testing.md#8-上线门槛) ⑦ 要求零强平,另有必过的 L3 路径变体(依次出现 L1/L2/L3 且无强平),用例定义归 [hft-sim](../crates/hft-sim/README.md);模拟账户上的保证金计算与交易所口径对照。

## 一个账户只给一个策略实例,身份与配置先核验

**来源**:cdex 测试/正式双实例的教训;2026-10-02 主策略适配评审。

**缺陷**:两个策略共用一个交易所账户:单向持仓下同一个币只有一个净仓,资金费与盈亏无法按策略拆分,共享的保证金让一个策略的分配不知道另一个占了多少。同类:测试与正式的 key 配反就会串单;账户是双向持仓或逐仓时,reduceOnly 与保证金假设全部失效,下单被拒。

**规则**:

1. 一个交易所(子)账户只绑定一个策略实例(`AccountMeta.bound_instance`);`SetParams` 校验账户已绑定到该实例,否则 `InvalidArgs{AccountBound}`。新策略上线开新子账户,ccex 与 cdex 不共用账户。
2. 启动闸门 `IDENTITY`:对**每一把**交易 key(Ed25519 会话 key 与 HMAC REST key 分别)向交易所查询账户 UID,与 `AccountMeta.venue_uid` 比对;kill-switch 自检与 reconciler 启动同样核验自己的 key;任一不符 → 退出 2 + P0(读不到 UID 的测试网账户按 `uid_policy = "unverifiable"` 放行,只限非正式账户,规则归 [hft-refdata](../crates/hft-refdata/README.md) 与 [hft-risk](../crates/hft-risk/README.md));逐账户自检 kill 位(经真实 store/load)。
3. 启动闸门 `ACCOUNT_CFG`:`AccountConfigReport`(持仓模式、保证金模式、多资产模式、实际杠杆、现货借币开关)与 `VenueCaps` 的要求一致才放行;现货账户必须关闭借币(如 OKX `enableSpotBorrow = false`、Bitget 现货杠杆关闭;逐所待核实,方法:测试网账户配置接口实测),否则 `AUTO_BORROW`;运行中配置变化 → cap。
4. 交易 key 不带提现权限并绑定 IP 白名单;开通步骤见 [runbooks/launch](runbooks/launch.md)。

**落点**:[`hft-refdata`](../crates/hft-refdata/README.md) 的 `AccountMeta`;[`hft-risk`](../crates/hft-risk/README.md) 的启动闸门;[control-plane](subsystems/control-plane.md) 的 `SetParams` 校验;[`apps/hft-core`](../apps/hft-core/README.md) 的启动顺序;理由见 [one-account-per-strategy-instance](../.agents/notes/proposed/architecture/2026-10-02-one-account-per-strategy-instance.md)。

**验证**:ctl 测试"未绑定账户的 `SetParams` 被拒";启动测试"任一把 key(Ed25519、HMAC、kill-switch、reconciler)的 UID 不符 → 退出 2 且 P0";"账户为双向持仓 → `ACCOUNT_CFG` 闸门不过";"现货账户开着借币 → `AUTO_BORROW`,闸门不过";DST"运行中收到配置变化报告 → cap 生效"。

## 同一交易所不同时做现货与永续

**来源**:2026-10-03 M0 独立评审。

**缺陷**:统一账户(现货与永续共用一个钱包)下,同一笔计价资产在两个 venue 的账本里各记一份:策略按约两倍的资金分配,第二腿下单时余额不足被拒 → Broken → 裸腿。引擎按 venue 记现金,看不出两个 venue 背后是同一个钱包。

**规则**:

1. 一个账户在同一交易所只用一个市场(现货或永续):refdata 加载时,一个 `AccountId` 的 venue 集合中不得有两个 `exchange` 相同的 `VenueId`,否则拒绝加载(`SameExchangeMarkets`);Sim 账户同样适用。
2. `SetParams` 校验同一 `GroupSpec` 的两条腿不在同一交易所,否则 `InvalidArgs{SameExchangeLegs}`。
3. 因此不建模"余额域":账本、权益视图与模拟账户都按 venue 记现金。要做同所现货 × 永续,先用 Agent Note 推翻本条,再给账本、视图与模拟加余额域。

**落点**:[`hft-refdata`](../crates/hft-refdata/README.md) 的加载校验;[control-plane](subsystems/control-plane.md) 的 `SetParams` 校验;[`hft-ledger`](../crates/hft-ledger/README.md);理由见 [no-same-exchange-spot-perp](../.agents/notes/proposed/architecture/2026-10-03-no-same-exchange-spot-perp.md)。

**验证**:refdata 单元测试"同一账户含两个同交易所 venue → `SameExchangeMarkets`"(含 Sim 账户);ctl 测试"两腿同交易所的 `GroupSpec` → `InvalidArgs{SameExchangeLegs}`"。
