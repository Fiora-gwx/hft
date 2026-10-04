# 延迟预算

本页是**延迟的唯一定义**:时间点、分段名、tick-to-trade 的两种口径、CEX 与链上路径的分段预算(目标与硬线)、数据集与分组规则、测量方法。任何 README、基准、看板、直方图、Agent Note 提到延迟时只用本页的分段名;§3 的预算数字只在本页出现,别处链接。设计总预算的出处是 [architecture-v2/00](../design/00-目标与延迟预算.md)、[03 §2](../design/03-网络与传输层.md)、[09 §1](../design/09-可观测性、风控与控制面.md);本页标"分配"的数字是把总预算拆到分段的结果,M1.5 延迟探针与 M3 末满订阅探针([11 实施路线图](../design/11-实施路线图.md))实测后重定。字段布局在 [event-model](subsystems/event-model.md#时间戳语义) 与 [`hft-event`](../crates/hft-event/README.md);直方图与离线统计的实现在 [`hft-telemetry`](../crates/hft-telemetry/README.md) 与 [`watchtower`](../apps/watchtower/README.md);基准门槛在 [benchmarks/](../benchmarks/README.md);上线门槛 ⑩ 的判定在 [testing §8.2](testing.md#82-延迟验收的测法)。

## 1. 时间点

### 1.1 每个事件的五个时间戳

| 名称 | 含义 | 时钟域 | 来源 | 存放 |
|---|---|---|---|---|
| `ex_ts` | 交易所事件时间(撮合/成交,如 Binance `T`) | 交易所 UTC | 报文 | 事件体 `ex_ts_ns` |
| `ex_send` | 交易所推送时间(如 Binance `E`);无则 0 | 交易所 UTC | 报文 | 事件体 `ex_send_ns` |
| `hw_rx` | 网卡硬件接收时间;不可用时为内核软件戳并置 `SW_TS`;都没有为 0 | PHC(换算规则见 §2.1) | `SO_TIMESTAMPING` cmsg | 事件体 `hw_rx_ns` |
| `rx_tsc` | feed/gw 线程 `recvmsg` 返回、读完承载该帧最后一个字节的 TCP 段时的 TSC | 本机 TSC | `hft-rt` | 事件体 `rx_tsc`;raw journal 记录头 `tsc` |
| `proc_tsc` | sequencer 从输入环取出该事件时的 TSC | 本机 TSC | `hft-rt` | seq journal 记录头 `tsc` |

`hw_rx` 对应的是 TCP 段而不是 WS 消息;一次 `recvmsg` 读到多个段时取哪个段的戳、取不到时怎样标记,归 [hft-net 时间戳](../crates/hft-net/README.md#7-时间戳);一帧跨多次读取时 `hw_rx`/`sw_rx` 与 `rx_tsc` 取自不同的读取,`kernel_rx` 与 `kernel_to_user` 因此含等待该帧收全的时间。一次 `recvmsg` 解出多条消息时,它们共享同一对 `hw_rx/rx_tsc`,突发时 `kernel_rx` 会低估排队等待;门槛 ⑩ 的报告并列 `kernel_rx` 的上下界(上界借助 `MarketHdr.rx_empty_gap_ns`,字段归 [hft-event](../crates/hft-event/README.md)),口径归 [testing §8.2](testing.md#82-延迟验收的测法)。c8g 上 HW RX 时间戳与 PTP 的可用性待核实(方法:M1.5 探针上 `ethtool -T` 与 `SO_TIMESTAMPING` 实测,[12 A7](../design/12-风险与未决问题.md));不可用时全部分段落在 SW 数据集(§5),换算以 `sw_rx` 为锚(§2.1)。

### 1.2 每笔订单再加三个

| 名称 | 含义 | 时钟域 | 来源 |
|---|---|---|---|
| `decided_tsc` | sequencer 把该订单的 `OrderDecision` 写入 seq journal 的时刻(策略 + 风控 + OMS 分配槽与 client id 完成;紧随其后投影出的 `OutMsg`(`GwCmd`)入 out 环) | TSC | sequencer,写进 `OrderDecision` |
| `sent_tsc` | gw 线程 `writev` 返回的时刻 | TSC | gw,写进 `SendReport` |
| `ack_hw_rx` | 该订单第一条回报(`Ack`/`Reject`/`Fill`,取最早)的 `hw_rx` | PHC | gw 解码回报,写进 `OrderEvent.hw_rx_ns` |

五个事件时间戳(取触发该订单的那条事件)+ 三个订单时间点 = **每订单八个时间点**,编号固定为:

```
0 ex_ts   1 ex_send   2 hw_rx   3 rx_tsc   4 proc_tsc   5 decided_tsc   6 sent_tsc   7 ack_hw_rx
```

sequencer 线程内的 OMS 对每次新单、撤单与改单,在该操作的首个交易所响应时把八个点合成一条 `LatencySample`(0x0505;`action` 字段区分三者,撤单与改单的点 5–7 取该操作自己的决定、写出与首个响应;写入时机归 [hft-oms](../crates/hft-oms/README.md))写进 seq journal。`LatencySample` 是 **Meta 类**记录:占 `seq`,不进入状态、不参与验证回放的比对([hot-path-rules D5](hot-path-rules.md#确定性d))。字节布局由 [`hft-event`](../crates/hft-event/README.md) 定义,分段计算由 [`hft-telemetry`](../crates/hft-telemetry/README.md) 实现;时间点与分段的**名称和语义以本页为准**。样本另带触发事件的维度(触发事件的 kind——行情事件或回报帧,§2 据此区分 `tick_to_trade` 与 `rep_to_order`;`link_role`;`venue`、`account`、`strategy`、`slot`、触发与回报的 `seq`)、每个点的来源标志(HW/SW/缺失)与尾部追加的 `sw_rx`(§1.3,不改变 0–7 的编号)。触发事件是回报帧时,点 0–4 取该回报帧的时间戳。

### 1.3 辅助时间点

| 名称 | 含义 | 用途 |
|---|---|---|
| `sw_rx` | 内核软件接收戳(`RX_SOFTWARE`),与 `hw_rx` 取自同一 cmsg | **常态记录**:追加在 `MarketHdr` 与 `LatencySample` 尾部(布局归 [hft-event](../crates/hft-event/README.md));把 `kernel_rx` 拆成诊断分段 `nic_to_kernel`、`kernel_to_user`(§2),进生产直方图,不作门槛;HW 戳不可用时作换算锚点(§2.1) |
| `tx_sw` | 内核 TX 软件戳(`TX_SOFTWARE`/`TX_SCHED`,经 `MSG_ERRQUEUE` 由 housekeeping 线程读回,`OPT_ID` 对回订单)。**默认关闭**;可选扩展:当生产直方图显示内核 TX 路径值得单独测量时打开 | `kernel_tx` 分段(只在打开时存在);ENA **没有 TX 硬件戳**,不存在 `hw_tx` |
| `ping_rtt` | 每条连接每 5 s 一次 WS ping/pong 的往返;BBO 连接按 `bbo_ping_ms`(1 000)发 ping,回帧同时刷新连接健康 `conn_live_tsc`([hft-venue](../crates/hft-venue/README.md)) | `ws_rtt` 指标([03 §3.5](../design/03-网络与传输层.md)) |

## 2. 分段名(全仓唯一)

一律用 `snake_case` 名称。AWS 十点分解的 D2–D5 只作对照,不作为仓库内的指标名。

| 分段 | 定义 | 含义 | 对照 |
|---|---|---|---|
| `ex_queue` | `ex_send − ex_ts` | 交易所内部:撮合 → 推送 | — |
| `wire_in` | `hw_rx − ex_send` | 交易所推送 → 我们网卡(网络单向 + 交易所扇出);需 PHC 同步 | — |
| `feed_oneway` | `hw_rx − ex_ts` | 行情单向(00 §3 的定义);交易所戳为 ms 级,只看大样本分布 | — |
| `kernel_rx` | `rx_tsc − hw_rx` | 网卡 → 软中断(专用 IRQ 核)→ socket 队列 → 热线程下一次非阻塞 `recvmsg` 读完;含应用轮询节拍的等待 | D2+D3 |
| `nic_to_kernel` | `sw_rx − hw_rx` | `kernel_rx` 的前半:网卡 → 内核协议栈打戳;诊断分段,进生产直方图,不作门槛 | D2 |
| `kernel_to_user` | `rx_tsc − sw_rx` | `kernel_rx` 的后半:内核 → 用户态读完(含轮询节拍等待);诊断分段,同上 | D3 |
| `ingest` | `proc_tsc − rx_tsc` | TLS 解密 + WS 解帧 + 解码 + 定点归一化 + 入环(`bbo` 环或 `md` 环,见 §2.3)+ 在环中排队 + sequencer 取出 | — |
| `feed_host` | `proc_tsc − hw_rx` = `kernel_rx + ingest` | 行情主机内部分 | — |
| `decide` | `decided_tsc − proc_tsc` | 赋 seq + 写 seq journal + 簿更新 + 策略快速部分 + 执行层 + 风控 + OMS 分配槽与 client id + `OrderDecision` 写 journal。**唯一的纯计算分段**;组成部分的分配见 [hft-sequencer](../crates/hft-sequencer/README.md)、[hft-strategy-sdk](../crates/hft-strategy-sdk/README.md)、[hft-exec](../crates/hft-exec/README.md) | D4 的一部分 |
| `egress` | `sent_tsc − decided_tsc` | `OutMsg`(`GwCmd`)入 out 环 + 环跳 + gw 取 `OutMsg` + **gw 填帧模板** + WS 掩码 + TLS 加密 + `writev` 返回 | D4 的一部分 |
| `kernel_tx` | `tx_sw − sent_tsc` | 内核 TX 路径(仅在 TX 时间戳打开时) | D5 |
| `tick_to_trade` | `sent_tsc − hw_rx` = `kernel_rx + ingest + decide + egress` | **主机内 tick-to-trade**:触发事件是行情事件的订单样本 | — |
| `rep_to_order` | 由某条回报帧(`OrderEvent`:Ack/Fill/Cancel 等)触发的下一笔订单操作(新单、撤单或改单,三者都计入)的 `sent_tsc` − 该回报帧的 `hw_rx`;计算同 `tick_to_trade`:`kernel_rx + (sent_tsc − rx_tsc)` | **回报路径主机内部分**:回报帧收包、gw 解码、rep 环与 gw 内等待、sequencer 处理、下一单出环与写出——对冲路径走的就是这条路。样本 = 触发事件是回报帧的订单样本,不计入 `tick_to_trade`;主网行情负载下由 latency-probe `quote` 模式(Ack 后同一事件内撤单)产生 | — |
| `hedge_host` | `rep_to_order` 中 `link_role = Second` 的样本:LegLink 第二腿 `sent_tsc` − 触发它的第一腿成交帧的 `hw_rx`(不含 `SecondRetry`:重试由定时器驱动,不是回报直接触发) | **对冲路径主机内部分**,`rep_to_order` 的子集 | — |
| `order_rtt` | `ack_hw_rx − sent_tsc` | 网络 + 交易所网关/撮合 + 回报;只统计新单(`LatencySample.action = New`) | — |
| `tick_to_ack` | `ack_hw_rx − hw_rx`(触发事件) | **端到端 tick-to-ack**,用于业务评估,不作发布门槛 | — |
| `leg_gap` | 第二腿 `sent_tsc` − 触发它的事件(第一腿 `Fill` 或链上 `TxLifecycle{IncludedOk}`)的 `proc_tsc` | 双腿间隔的 sequencer 之后部分;`LegLink` 在同一事件内出环。成交在 rep 环与 gw 里的等待看 `hedge_host` | — |
| `ws_rtt` | 连接级 ping/pong 往返 | 到交易所的网络健康;不是事件分段 | — |

### 2.1 跨时钟域换算

- 纯 TSC 分段(`ingest`、`decide`、`egress`、`leg_gap`)直接相减再按段头 `tsc_hz` 换算为 ns,没有同步误差。
- **校准记录**:housekeeping 每个校准节拍(取值归 [hft-core 线程计划](../apps/hft-core/README.md#线程计划))写一条 `ClockStatus`(Meta;含该次校准对的 TSC、REALTIME 与 PHC 读数及误差界 `calib_err_ns`,布局归 [hft-event](../crates/hft-event/README.md),采样方法归 [hft-rt](../crates/hft-rt/README.md))。
- **含 PHC 端点的分段**(`kernel_rx`、`tick_to_trade`、`rep_to_order`、`hedge_host`、`order_rtt`、`tick_to_ack`):TSC 端点按样本**前后两条** `ClockStatus` 的校准对线性插值换算到 PHC 域,不取阶梯值。逐样本误差界 = 两端 `calib_err_ns` 的较大者 + 相邻两个校准区间的速率差 × 区间长度的一半(速率在区间内突变的保守上界)。`tick_to_trade`、`rep_to_order` 与 `hedge_host` 按 `kernel_rx + (sent_tsc − rx_tsc)` 计算,把跨域误差限制在 `kernel_rx` 一段。
- **误差过滤**:误差界 > 1 µs 的样本计入 `unsynced_cal` 计数,不进分位数;⑩ 的报告附误差界分布(p50/p99/max)与 `unsynced_cal` 占比。缺少后一条 `ClockStatus`(进程刚退出)或区间两端之一为 `Stepped` 的样本同样计入 `unsynced_cal`。
- **HW 戳不可用**(`SW_TS`):以 `sw_rx` 为锚,用同一对 `ClockStatus` 的 TSC ↔ REALTIME 端插值(夹逼窗口约 50 ns,误差界同样逐样本给出);这些样本属 SW 数据集(§5),不与 HW 样本合并;门槛 ⑩ 何时改用 SW 锚数据集判定归 [testing §8.2](testing.md#82-延迟验收的测法)。
- 含 `sw_rx`(系统时钟域)的诊断分段(`nic_to_kernel`、`kernel_to_user`)用同一插值的 REALTIME 端换算,误差界规则相同。
- 含交易所时钟的分段(`ex_queue`、`wire_in`、`feed_oneway`)只在 `ClockStatus` 偏移界 ≤ 200 µs 的窗口内计入;超出的样本计入 `unsynced` 计数,不进分位数([09 §3](../design/09-可观测性、风控与控制面.md))。

### 2.2 链上路径时间点与分段

可选扩展(CEX-DEX 策略从旧框架迁移时启用)。chain-core 有自己的 TSC 与 chain journal,与 hft-core 同机(外部 venue 协议走共享内存环);可选的 builder profile 可放在另一台主机,跨主机的端点用 UTC(chrony,链上分段是 ms 级以上,误差可忽略)。发送方式称 **profile**([hft-chain-evm](../crates/hft-chain-evm/README.md)):**free**(免费公共 RPC)与 **builder**(付费);同名分段在不同 profile 下分别统计,不混算(§5)。

| 时间点 | 含义 | 记录处 |
|---|---|---|
| `signal` | hft-core 产生 `SwapDecision` 时的 `decided_tsc` | seq journal |
| `intent_rx` | chain-core 从外部 venue 协议收到 `SwapIntent` | chain journal |
| `bundle_sent` | 第一个发送端点的写出返回:free profile = 第一个公共 RPC 的 `eth_sendRawTransaction` 写出,builder profile = 第一家 builder 的投递写出 | chain journal |
| `block_ts` | 包含该 bundle 的区块时间戳 | 链 |
| `include_seen` | 满足 `IncludedOk` 证据规则的时刻(free profile:第二个不同运营商的回执到达;builder profile:块流或本机节点)= `TxLifecycle.observed_ts_ns` | chain journal → seq journal |
| `hedge_fill` | CEX 对冲腿 `Fill` 的 `ex_ts` | seq journal |

| 分段 | 定义 | 含义 |
|---|---|---|
| `chain_decide` | `bundle_sent − intent_rx` | 预检 + 构造 + 签名 + 投递;预检在 free profile 是公共 RPC `eth_call`,在 builder profile 是本机节点 `eth_call`(可选扩展:L0 复核 + L1 revm) |
| `sol_decide` | Solana 两路写出返回 − `intent_rx` | `chain-core@solana` 的对应段:构造 + 签名 + 两路写出([hft-chain-solana](../crates/hft-chain-solana/README.md));Solana 为 optional,不设门槛 |
| `chain_signal_to_send` | `bundle_sent − signal` | 信号 → bundle 发出(含 hft-core → chain-core 传输) |
| `chain_include_blocks` | 包含区块号 − 发送时最新区块号 | 以块计的包含延迟;另记块内排位 `chain_block_pos` |
| `chain_include_seen` | `include_seen − block_ts` | 看到包含证据的滞后 |
| `leg_gap` | 同 §2 | 对冲腿主机内部分 |
| `exposure` | `hedge_fill − block_ts` | 库存暴露时长 |
| `relay_hop`(optional) | chain-core 读到帧的 `rx_tsc` − `RelayFrameHdr.rx_tsc`(同机,同一 TSC 域) | [chain-grpc-relay](../apps/chain-grpc-relay/README.md#延迟与资源契约) 一跳:解码 + 写环 + chain-core 读到 |
| `evidence_to_emit` | `TxLifecycle` 写入 chain-link 的 TSC − 触发它的证据 `rx_tsc` | chain-core 内部:证据 → 生命周期迁移 → 出链接([hft-chain-evm](../crates/hft-chain-evm/README.md#延迟与资源契约)) |

### 2.3 BBO 与深度的通道

- **连接与任务**:每个 venue 的 BBO 流(bookTicker 类)与其他行情流走**不同连接**。升级主机类(`c8` 及以上)上 BBO 连接在 `feed-bbo:<venue>` 任务(只含 `RouteKind::Bbo` 路由,可跨 venue 合并到同一 `feed-bbo-*` 线程),成交、标记价/指数价、资金费与分档深度(`RouteKind::DepthTopN`)连接在 `feed-md:<venue>` 任务,两者不同线程([hot-path-rules T8](hot-path-rules.md#线程t));全量增量深度是可选扩展(触发条件:某个策略需要全深度),启用时同在 `feed-md`。`dev2`/`c4` 可把同一 venue 配成一个 `routes = All` 的 feed 任务。feed 把 `Bbo` 写 `bbo` 环,其余写 `md` 环。行情 REST(BBO 补帧;启用增量深度时另有 L2 快照)由 warm 任务 `snap-fetch` 请求、读取、解密并解析,经 `snap-<venue>-bbo` / `snap-<venue>` 环交回 `feed-bbo` / `feed-md`,**热线程不做行情 REST 的系统调用与解析**([hft-venue](../crates/hft-venue/README.md))。
- **sequencer 取环**:优先级 `ctl > rep(实盘回报)> bbo > advice > md > rep-sim`([architecture.md §3](architecture.md#3-hft-core-线程))。md、bbo 与 advice 都**逐条处理**:每处理完一条 md 消息即查看 ctl、实盘 rep、bbo 的环头,每处理完一条 bbo 或 advice 即查看 ctl 与实盘 rep 的环头(各一次 Acquire 读),非空则先取并处理它们;同时比较一次最近的定时器到期,到期即让位。md 每轮另有时间片限额。限额参数、环头检查的预算(按环数)与定时器让位归 [hft-sequencer](../crates/hft-sequencer/README.md)。取出顺序照常写进 journal,确定性不变。
- **深度进入方式**:分档快照(`L2Snapshot{origin = TopN}`)每条整替换该簿(无序号链、无 REST 重同步,[hft-book](../crates/hft-book/README.md));可选的增量深度以 `L2Batch` 进入,一条交易所消息一条记录。两种都整条应用后才更新 mid、调用策略,策略不会看到半条消息。由深度记录触发的订单,其 `proc_tsc` 是该条被取出的时刻,`decide` 包含整条的应用时间。
- **深度与其他 md 突发对 BBO 的影响**只有两处,各自有界:
  1. **sequencer 侧**:BBO 最多等**正在应用的那一条** md 消息 + 一次环头检查。附加等待上界 = 该条档数 × 簿更新上限(< 100 ns/档,[benchmarks 上限表](../benchmarks/README.md#单-crate-基准上限表));分档快照每侧 ≤ 20 档;增量深度的 REST 快照按 `L2SnapshotPart` 分片(每片档数 `snapshot_part_levels` 归 [hft-venue](../crates/hft-venue/README.md))。单条增量消息的档数分布待核实(方法:M3 前用现有采集数据统计各所 `L2Batch` 档数的 p99/max)。
  2. **feed 侧**:升级主机类上 BBO 连接不与其他行情流、快照同线程。同一 `feed-bbo-*` 线程上的多条 BBO 连接之间按帧轮转、每连接每次 poll 至多 `feed.bbo_frames_per_poll` 帧;承载 ≥ 3 个 socket 的 `Spin` 线程用就绪探测(`epoll_pwait(0)`)只读就绪 socket,轮询节拍不随空闲 socket 数增长([hot-path-rules T7](hot-path-rules.md#线程t))。等待上界的口径归 [hft-venue](../crates/hft-venue/README.md)("BBO 在 feed 侧的等待上界")。`routes = All` 只用于无延迟验收的形态,在路由之间简单轮转。
- **预算**:深度或其他 md 流 10× 常速突发期间,`Bbo` 的 `ingest` p99 仍 ≤ 10 µs;由 `Bbo` 触发的 `tick_to_trade` p99 相对平静期上涨 < 10%。验收基准是单进程、不依赖 mock 的 `seq/l2burst`(M2;断言与参数归 [benchmarks](../benchmarks/README.md#路径基准与门槛))。生产上用 `(venue, Bbo, ingest)` 直方图按"同 venue `md` 环积压 > 0"切成两个数据集比较,不合并。

## 3. CEX 路径预算

交易所 BBO → 下单 → 回执(首批 Binance、OKX、Bitget)。现状列来自 00 §4(c7g.large @ apne1-az2)。**基准形态** = AWS 同 AZ、选定的统一升级主机类(§7)+ 调优内核 + **中断驱动收包**(交易网卡的 IRQ 与软中断在专用 IRQ 核,热线程只做非阻塞 `recvmsg`,busy poll 全关,[hot-path-rules T12](hot-path-rules.md#线程t))。

- **目标**列是基准形态下的分配,全部待 M1.5 与 M3 末满订阅探针实测:探针结论写 Agent Note,M3 末探针之后据此重定本表并选定主机类。
- **硬线**列是上线门槛 ⑩ 的验收下限,只有 `tick_to_trade` 与 `rep_to_order` 两个分段有硬线;`rep_to_order` 的硬线可在 M3 末探针后重定。目标与硬线的关系(落在两者之间如何通过、谁签字)只在 [testing §8.2](testing.md#82-延迟验收的测法) 判定;本表只给数字。
- 没有硬线的分段只用于告警、记录与分配校核(生产直方图,§6),不是 ⑩ 的硬项。p99.9 只记录(§3.1)。
- **可选 `metal` 列只是参考估算**:`metal` 的预算由 bake-off 实测决定,不按 Onload 估算验收(触发条件:交易所提供 AWS 之外的接入,或基准形态证明 AWS fabric 抖动主导 p99)。

| 分段 | 现状 | 目标 p50 | 目标 p99 | 硬线 p50 | 硬线 p99 | 可选 metal 参考 p50 | 可选 metal 参考 p99 | 出处 |
|---|---|---|---|---|---|---|---|---|
| `kernel_rx`(`c32`) | 20–50 µs,p99 200 µs | ≤ 8 µs | ≤ 20 µs | — | — | ≤ 1.5 µs | ≤ 4 µs | 分配;03 §2 中断驱动 5–15 µs;Onload 列 03 §2 ≈ 1.3 µs |
| `kernel_rx`(`c16`) | — | 待 M3 末探针 | 待 M3 末探针 | — | — | — | — | 见下文"按主机类" |
| `ingest`(bookTicker ≈ 300 B) | 5–15 µs | ≤ 3 µs | ≤ 10 µs | — | — | ≤ 2 µs | ≤ 6 µs | 00 §4 "0.5–3 µs" + 环 0.1–0.3 µs;p99 为分配 |
| `ingest`(深度 ≈ 2 KB:分档快照;全量增量深度为可选扩展) | — | ≤ 4 µs | ≤ 12 µs | — | — | ≤ 3.5 µs | ≤ 8 µs | 03 §2 "1.5–3 µs";p99 为分配 |
| `decide` | Python 数 ms | ≤ 5 µs | ≤ 15 µs | — | — | ≤ 3 µs | ≤ 8 µs | 00 §4 "1–5 µs";p99 为分配 |
| `egress` | 10–20 µs | ≤ 5 µs | ≤ 15 µs | — | — | ≤ 1.5 µs | ≤ 5 µs | 00 §4 "2–5 µs"、03 §2 Onload 0.5–1 µs |
| **`tick_to_trade`** | 50–100 µs,p99 > 200 µs | **≤ 22 µs** | **≤ 75 µs** | **≤ 30 µs** | **≤ 100 µs** | ≤ 8 µs | ≤ 25 µs | 目标:分配(由 00 §1 的 20/60 按中断驱动重定,理由与校核见 [00 §4.1](../design/00-目标与延迟预算.md));硬线:00 §1 |
| **`rep_to_order`** | — | **≤ 25 µs** | **≤ 80 µs** | **≤ 35 µs** | **≤ 110 µs** | — | — | 分配;含 rep 环与 gw 内等待;硬线 M3 末探针后可重定 |
| `hedge_host`(`rep_to_order` 的子集) | — | ≤ 25 µs | ≤ 80 µs | — | — | — | — | 分配;只记录,由 `rep_to_order` 的硬线覆盖 |
| `feed_host` | — | ≤ 12 µs | — | — | — | — | — | 分配(00 §1 "行情单向主机内 ≤ 10 µs" 按中断驱动重定) |
| `kernel_tx`(可选) | — | — | < 5 µs | — | — | — | — | 02 §5(D5) |
| `leg_gap` | 数百 ms | ≤ 20 µs | — | — | — | 同 | — | 00 §1 |
| `ws_rtt`(到 Binance) | 1.0–1.5 ms | ≤ 0.3 ms;p99 − p50 < 100 µs | — | — | — | 同 | — | 00 §1 |
| `order_rtt` | 1.2–4 ms | 0.3–3 ms(交易所内部不可控) | — | — | — | 同 | — | 00 §4 |
| `tick_to_ack` | 2–6 ms | ≈ 0.5–3.5 ms | — | — | — | 同 | — | 00 §4;业务评估,不作门槛 |

- 分段的 p99 不可相加;分配原则是各段目标 p99 之和 ≤ 总目标 p99 的 85%,给排队与跨段相关性留余量:`kernel_rx` 20 + `ingest` 10 + `decide` 15 + `egress` 15 = 60 ≤ 0.85 × 75。任何分段的分配调整须同一改动更新本表,并写明依据(基准或直方图、主机类、是否与 sim 同机)。
- **`kernel_rx` 按主机类**:它含应用轮询节拍的等待,随 `feed-bbo` 线程承载的 socket 数变化。`c32` 每所一个 `feed-bbo` 线程,目标为表中数字;`c16` 的 `feed-bbo` 线程更少、每线程承载多个 venue 的 BBO socket(分组归 [hft-core `c16` 节](../apps/hft-core/README.md#线程计划)),目标待 M3 末满订阅探针(启用 `epoll_pwait(0)`,T7)实测后写入本表——写入之前 `c16` 不能被选为统一升级主机类(§7)。
- 可选 metal 列按 00 §1 与 03 §2 的 Onload 估算(p50 ≤ 8、p99 ≤ 25)给出,只作参考;立项时以 bake-off 实测重写该列。

### 3.1 p99.9 记录项

p99.9 在固定节拍突发与周期性停顿下只记录、不作门槛,报告里按来源注明。已知来源(各自的上界归括号里的组件):

| 来源 | 形态 | 上界或频率 |
|---|---|---|
| 交易所固定节拍推送(Binance `@markPrice@1s`、分档快照 `@100ms`;启用增量深度时另有 `@depth@100ms`) | md 突发;BBO 等待按 §2.3 有界;各 inst 的推送是否对齐到同一时刻(订阅多时在 IRQ 核上形成整点突发)待核实(方法:M1.5 压舱深度连接按 `hw_rx` 统计各 inst 推送时刻的分布) | 每 100 ms / 1 s |
| 同一批推送在 IRQ 核上的软中断突发 | BBO 包排在同队列的批量包之后(软中断串行处理) | 待 M1.5 压舱深度连接与 M3 末探针实测;显著时在 `c32` 启用预留的第二 IRQ 核与 ENI-B2(§7) |
| 段滚动的引导块 | sequencer 在两个事件之间写;滚动时机(整点、两级大小安全阀)归 [hft-journal](../crates/hft-journal/README.md#写者) 与 hft-sequencer | ≤ 1 ms/次([hft-journal](../crates/hft-journal/README.md#延迟与资源契约));每段一次 |
| 关键帧快照 | sequencer **一次写完**(不分片),触发与最大推迟归 [hft-sequencer](../crates/hft-sequencer/README.md);实盘不计算 `state_hash`([journal-format](subsystems/journal-format.md#快照记录结构));每个 Live 簿的 `L2Snapshot{Keyframe}` 均摊在同一周期内 | ≤ 300 µs/次([hft-sequencer](../crates/hft-sequencer/README.md));频率由其触发条件决定。可选扩展:由 advisor 镜像在旁路生成关键帧(触发条件:实测关键帧停顿超出本行上界) |
| 热段退役的 `munmap` | persist 线程内;aarch64 为广播 TLBI(不打断热核,热核只承担 DVM 同步),可选 x86 剖面为 IPI shootdown | 每段退役一次;对热核的实际影响待 M3 末满订阅探针实测(M1.5 纵切片没有段退役) |
| `InjectWarmup` 预热块、`NotifyContinue` 续推 | ctl 环 / 驱动 Input | 单条 ≤ 5 µs / ≤ 30 µs([hft-sequencer](../crates/hft-sequencer/README.md)) |
| `ReconcileReport` 整组处理 | ctl 优先级上处理一组对账条目 | 上界或分批规则归 [hft-sequencer](../crates/hft-sequencer/README.md) |

## 4. 链上路径预算(BSC)

可选扩展(CEX-DEX 策略从旧框架迁移时启用)。按 profile 分两张表;两种 profile 的数据不合并。分段定义见 §2.2。

### 4.1 free profile

下列各段只报告分布(`dev2` 主机不做延迟验收),用于与旧框架 Python 链上引擎的同期日志离线对照,不作门槛。

| 分段 | 现状(旧框架 Python) | free profile | 说明 |
|---|---|---|---|
| `chain_decide` | 60 ms 复核 | 报告;由公共 RPC `eth_call` 往返主导(现有实测单次约 11 ms,AIN 复盘(本仓库之外:`analysis/DEX加速与AIN插针复盘.md`)) | 不设门槛 |
| `chain_include_blocks` | 常多等 1–3 块 | **下一块到数块,不承诺**;只报告分布 | 公共 mempool,与现状同类;改善需要 builder profile |
| `chain_block_pos`、被抢跑率 | 排位 20–40,约 80% 被抢跑 | 只报告,**不设目标**;预期与现状相近 | 同上 |
| `chain_include_seen` | 0.5 s 轮询,中位 1.3 s 才看到成交 | 报告;WS `newHeads` 可用时预期为"块到达后一次回执往返 × 2 个运营商" | 事件驱动替代轮询,是 free profile 的改进项之一 |
| 链上成交 → CEX 对冲发出 | 中位 146 ms、p90 30 s(旧框架实测) | `leg_gap`:收到 `IncludedOk` 的同一事件内出环(主机内 ≤ 20 µs,升级主机) | — |
| 确认窗口(ConfirmWindow,故意保留) | 600–900 ms,2 次采样 | 跨 ≥ 2 块(≈ 0.9 s),每事件评估 | 00 §5;**不是延迟目标,不要优化它** |

### 4.2 可选扩展:builder profile(付费;触发条件:包含率与被抢跑成为主要损失;统一升级主机及以上)

| 分段 | 目标 | 出处 |
|---|---|---|
| L0 池状态报价(`hft-chain-evm` 内部) | < 50 µs | 00 §5、[06 §2.3](../design/06-链上执行层.md) |
| `chain_decide`(本机节点 `eth_call` over CPG) | p99 ≤ 5 ms(分配) | 06 §2.3 |
| `chain_decide`(revm) | < 1.5 ms(L1 revm < 1 ms + 签名 < 0.1 ms) | 00 §5、06 §3 |
| `chain_signal_to_send` | ≤ 2 ms | 00 §1 |
| `chain_include_blocks` / `chain_block_pos` | 下一块包含率 ≥ 70%,排位 < 10(12 A4 的假设目标,未经证实) | 00 §1、[12 A4](../design/12-风险与未决问题.md) |
| `chain_include_seen` | +50–150 ms(块流) | 00 §5 |
| `leg_gap`(chain-first 对冲) | ≤ 20 µs | 00 §5 |
| `exposure` | 0.5–1.2 s | 00 §5 |

欧洲发送点、Solana 法兰克福等可选扩展的链上目标在立项时补入本节;在那之前 Solana 分段不设门槛。

## 5. 分组与"不同数据集不混算"

**分组键**:每个分位数至少按 `(venue, 事件 kind, 分段)` 分组([00 §3](../design/00-目标与延迟预算.md));订单类分段(`decide`、`egress`、`tick_to_trade`、`rep_to_order`、`hedge_host`、`order_rtt`、`tick_to_ack`)的事件 kind 指**触发事件**的 kind(bookTicker 触发与深度触发分开;`rep_to_order` 按回报种类 Ack/Fill/Cancel 分开)。可以再细分(`inst` 类别——由 refdata 给出,[09 §2](../design/09-可观测性、风控与控制面.md);`account`;`strategy`),不能更粗。`watchtower` 的直方图键与每日报表都满足这一条。

**数据集**:下列任一维度不同,即为不同数据集,**不合并直方图、不合并分位数、不在同一张表里并列成一个数**:

| 维度 | 取值 | 从哪里读 |
|---|---|---|
| 时间戳来源 | `hw_rx` 为 HW / SW / 缺失 | 记录 `flags` 的 `HW_TS`/`SW_TS` |
| 运行环境 | 实盘账户 / 主网行情 + 交易所测试网或模拟盘下单回路(`order_rtt`、`tick_to_ack` 是测试网的,不代表生产)/ 测试网行情(`feed_env = testnet`,只作 `pair` 模式的小样本确认)/ 模拟账户(`sim` 实例;`egress`、`order_rtt`、`tick_to_ack` 来自 sim-gw 与模拟延迟,只作模拟报告,不进延迟统计)/ Shadow 策略(只有 `decide`)/ mock 回环 / 验证回放 / 模拟重演 / M1.5 探针纵切片 / M3 末探针 | journal 段头的实例名、`HostProfile`、`AccountMeta.kind` 与 venue 的测试网标志、策略 `RunMode` |
| 统计口径 | 订单样本(`LatencySample`)/ 按事件加权的离线统计(§6) | 报告来源 |
| 链上 profile | free / builder | chain journal 的 `ConfigBlob`(`send.profile`) |
| 主机剖面 | 主机类 `host.class`(§7)与 `HostProfile` 哈希(实例族、内核、调优参数、target-cpu) | 段首 `HostProfile` 事件 |
| 版本 | 发布版本 / Git SHA | `HostProfile` 与 `RELEASE.json` |
| 时钟状态 | `ClockStatus` 偏移界 ≤ 200 µs / 超出;逐样本误差界 ≤ 1 µs / `unsynced_cal`(§2.1) | `ClockStatus` 事件 |
| 行情流 | bookTicker / 分档快照 / 增量深度 / trade / SBE vs JSON | 事件 kind 与 venue 通道 |

每份报告同时给出样本数 `n`、缺失数、负值数、未同步数(`unsynced`、`unsynced_cal`)(沿用 Trading 仓库的验收纪律(本仓库之外:`Trading/AGENTS.md`))。`n < 1,000` 不报 p99,`n < 10,000` 不报 p99.9(写 `n/a`),防止小样本冒充尾部数字。订单类样本由延迟探针策略 [`latency-probe`](../strategies/latency-probe/README.md) 在主网行情负载下保证(只绑定非正式账户、测试网下单):`quote` 模式产生 `tick_to_trade`、`egress`、`order_rtt` 样本,并在 Ack 后同一事件内撤单产生 `rep_to_order` 样本;`pair` 模式产生 `hedge_host`、`leg_gap` 的小样本确认;`target` 模式走 `TargetIntent` → hft-exec 切片路径,样本按触发事件 kind 照常分组(模式与周期归 latency-probe)。`kernel_rx`、`ingest`、`feed_host` 不用探针样本,用按事件加权的离线统计(§6)。

## 6. 验收方法

| 检查 | 数据集 | 频率 | 通过条件 | 执行者 |
|---|---|---|---|---|
| M1.5 延迟探针 | 按需租用的 `c8g.4xlarge`(开在 az-probe 选定的 AZ;只放测试网 key)上的纵切片:hft-net + 一个 venue 的 BBO feed(主网公共行情)+ 简化 sequencer + gw(测试网下单);HW 与 SW 时间戳分开 | M1.5 一次,可重复 | 不作门槛、**不选档**:产出 `kernel_rx`、`ingest`、`decide`(简化)、`egress` 直方图,并做 IRQ 与时钟增测(压舱深度连接、IRQ 核空闲 A/B、`calib_err_ns` 等,清单归 [11](../design/11-实施路线图.md));结论写 Agent Note | 人工评审 |
| M3 末满订阅探针 | 按需租用的 `c8g.4xlarge`,按 `c16` 线程计划,6 个产品线满订阅(BBO、成交、标记价、资金费,另加压舱深度),打开 raw journal 与 persist,测试网探针单(配置归 [11](../design/11-实施路线图.md#32-m3-末满订阅探针)) | M3 末一次 | 不作门槛:据此重定 §3、选定主机类(`c16`/`c32`,§7)并决定是否在 `c32` 启用 IRQ 拆分(ENI-B2);结论写 Agent Note | 人工评审 |
| 生产直方图 | 生产,HW 时间戳,选定的统一升级主机类 / `metal`(§7) | 每 10 s 导出,每日报表 | 任一分段 p99 超 §3 目标 → 告警;**只告警,不降级**(降级只由 hft-risk 的 cap 规则产生) | [`watchtower`](../apps/watchtower/README.md) |
| 延迟验收(门槛 ⑩) | 统一升级主机上一个连续 7 天的主网实时行情窗口 + 测试网/模拟盘下单回路(latency-probe),prod 与 sim 两个实例同机运行;只评主机内分段:订单类分段用探针样本,`kernel_rx`/`ingest`/`feed_host` 用按事件加权的离线统计 | 连续 7 天 | 硬线、目标与签字规则、探针的"突发/平静"分层抽样、`kernel_rx` 上下界、HW 与 SW 锚数据集的选用、同机干扰的测法与判定、`pair` 模式确认窗口都归 [testing §8.2](testing.md#82-延迟验收的测法);本页给分段定义、数字与数据集规则 | 人工评审 |
| 可选:mock 回环全链路 | wire 级 mock 回环(参考机) | 每个里程碑 | `tick_to_trade` 的 p50/p99 ≤ §3 目标,p99.9 记录 | [benchmarks/tick-to-trade](../benchmarks/README.md) |
| 回归(参考机) | 微基准与路径基准;验证回放(同一份 journal) | 每个里程碑结束时(M1.5 起) | 相对回归与单 crate 上限([benchmarks 回归阈值](../benchmarks/README.md#回归阈值));`decide` 的 p99 相对基线上涨 > 10% 阻断发布(**M7 起**;此前只记录,[09 §2](../design/09-可观测性、风控与控制面.md)) | `cargo xtask bench-gate --reference` |
| 生产回归 | 生产 | 每日 | 任一 p99 目标连续 3 日被突破视为回归,阻断发布([10 §6](../design/10-语言分工与代码组织.md)) | `watchtower` 日报 |
| 交易所侧 | 生产 | 每周复测并存档 | `ws_rtt`、`wire_in` 分布;AZ/spine 漂移时触发迁移评估 | [tools/az-probe](../tools/README.md) |
| 链上 | 生产 chain journal | 每日报表 | free profile:§4.1 只报告;builder profile:§4.2 目标,包含率与抢跑率按 builder 分组 | `watchtower` |

**按事件加权的离线统计**:探针按时间触发,即使按"突发/平静"分层抽样(规则归 testing §8.2)也代表不了每条行情,突发时段(尾部与真实策略触发都集中在那里)会被低估。所以行情侧分段从 seq journal 对**每条** `Bbo` 与深度记录离线计算:`kernel_rx`、`ingest`、`feed_host` 取记录头 `tsc`(= `proc_tsc`)与 `MarketHdr` 的 `hw_rx`/`sw_rx`/`rx_tsc`,换算按 §2.1,热路径零额外成本。实现是 [hft-telemetry](../crates/hft-telemetry/README.md) 的离线模块 `event_weighted()`,命令行入口是 [`replayer latency`](../apps/replayer/README.md);门槛 ⑩ 的 `ingest`/`kernel_rx` 读这份统计,探针样本只用于订单类分段。

**参考机**从 M1.5 起按需租用:每个里程碑结束时在同型租用实例上跑 `bench-gate --reference`(选档前为 `c8g.4xlarge`,选档后为选定的主机类);它不是 CI runner,CI 托管 runner 上不判任何时间门槛([benchmarks 参考机与 CI](../benchmarks/README.md#参考机与-ci))。

直方图参数:`hdrhistogram` 3 位有效数字、范围 1 ns–10 s;导出 p50/p90/p99/p99.9/max 与计数([09 §2](../design/09-可观测性、风控与控制面.md))。热路径只读时间戳(≈ 20 ns/次),不做统计。任何性能声明附直方图或基准输出,并写明数据集的全部维度(§5)与主机类(§7)。

## 7. 主机类与验收口径

主机类由 hft-core 配置 `host.class` 指定(线程计划见 [hft-core](../apps/hft-core/README.md#线程计划),编码见 [hft-rt](../crates/hft-rt/README.md));**主机类是数据集维度**(§5 的"主机剖面"),不同主机类的直方图不合并、不互相比较。

| 主机类 | 线程模型 | 延迟验收 | 记录什么 |
|---|---|---|---|
| `dev2`(现有 c7g.large,2 vCPU,与旧系统共机;不放正式实盘账户) | 不出现 `Spin`(`SpinThenPark`/`Park`);一个 venue 一个 `routes = All` 的 feed 任务;每个下单 venue 的 gw 独占线程(T8);不隔离核、无大页;运行在资源护栏内 | **无**:§3/§6 的延迟目标不适用,`bench-gate` 的时间门槛不在 dev2 上执行 | 生产直方图照常导出(标 `host.class = dev2`),只作信息 |
| `c4`(4 物理核,联调档,可选) | 只承载 `prod`,至多一个下单 venue([hft-core `c4` 节](../apps/hft-core/README.md#c44-物理核联调可选));`routes = All` 的 feed 任务;每个下单 venue 的 gw 独占线程(T8);无 IRQ 专核;隔离核与大页由 deploy 生成,但启动自检不作 hard 项([hft-rt `HostCheck`](../crates/hft-rt/README.md)) | **无** | 同上,标 `c4` |
| `c8`(`c8g.2xlarge`) | 只承载 `prod`,不承载 `sim` | **无**:延迟验收要求 prod 与 sim 同机 | 同上,标 `c8` |
| `c16`(`c8g.4xlarge`,16 vCPU / 32 GiB) | **条件候选**(首批下单 venue ≤ 4):只在 M3 末满订阅探针达标、`feed-bbo` 线程启用 `epoll_pwait(0)`(T7)且 §3 已写入其 `kernel_rx` 目标时保留;prod 的热线程各自独占隔离核、`Spin`;交易网卡 IRQ 在专用 IRQ 核;sim 只用非隔离核、独立 slice 与网卡 | 选定为统一升级主机类时在这一档:§6 各项,在 prod 与 sim 同机运行时测 | §3 全部分段 |
| `c32`(`c8g.8xlarge`,32 vCPU / 64 GiB) | prod + sim 同机的**默认候选**;线程原则同上,`feed-bbo-*` 与 `feed-md-*` 各自独占核;预留第二 IRQ 核与第二块交易网卡 ENI-B2(默认不启用;启用条件与流量划分归 [hft-core 线程计划](../apps/hft-core/README.md#线程计划)) | 同上 | 同上 |
| M1.5 探针主机(按需租用 `c8g.4xlarge`) | 纵切片(§6),热线程 `Spin`,IRQ 在专用核 | **无**:只记录,不选档 | 探针分段,标探针数据集 |
| M3 末探针主机(按需租用 `c8g.4xlarge`) | 按 `c16` 线程计划满订阅(§6) | **无**:据此重定 §3、选档并决定 IRQ 拆分 | 探针分段与按事件加权统计,标探针数据集 |
| `metal`(c7i.metal-24xl,optional) | 同统一升级主机类,线程更多 | bake-off 实测后定预算(§3 metal 列只是参考) | 同上 |

- **统一升级主机类按 M3 末满订阅探针选定**(默认 `c32`),宁可上更大档位,不砍首批下单 venue;线程计划与核号表归 [hft-core](../apps/hft-core/README.md#线程计划)。
- **延迟验收只在选定的统一升级主机上进行,而且在正式实盘之前**:主网实时行情 + 测试网/模拟盘下单回路测主机内分段,prod 与 sim 同机运行时测;在那之前开发与测试在本地(只写代码)、CI 与 `dev2` 上进行,`dev2` 只记录直方图。
- 修改 §3 某分段的预算须同一改动写明实测依据(主机类、是否与 sim 同机),不得把 dev2/c4/c8 的数字带入。
