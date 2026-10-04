---
description: "wire 级模拟交易所:TLS + WebSocket + REST,复用 venue 协议 crate 反向编解码,回放捕获报文并注入故障,用于每夜全链路回环延迟、混沌与 kill-switch 演练"
kind: app
tier: dev
scope: optional
status: frozen
---

# mock-exchange

## 概要

**冻结:不随 core 规格同步;解冻(触发条件同下文"可选扩展"一段)时先按 core 规格复核。** 已知待复核:V3 #23(连接轮换周期以 hft-venue ECF1 段 6 为准)、V3 #110(故障名 `ClockSkew` 已退役,时钟不可信由 `Timer{ClockBad}` 表达);回环门槛数字(以 [latency-budget §3](../../docs/latency-budget.md#3-cex-路径预算) 的目标列为准,硬线与签字只属上线门槛 ⑩);W5 §4(core 行情范围:行情源与快照端点按 [hft-venue](../../crates/hft-venue/README.md) 的 core 行情与分档快照流复核,增量深度与 REST 深度快照只在该可选扩展启用时需要);W5 N-P1-5(回环报告除 `tick_to_trade` 外按 latency-budget 加报回报触发的 `rep_to_order`)。

`mock-exchange` 在 wire 层冒充交易所:对 hft-core 暴露与真实交易所相同的 TLS + WebSocket(行情、下单、用户流)与 REST(查单、快照、撤单),报文字节格式由各 `venue-*` crate 的 `mock` 特性反向提供(编码行情与回报、解码订单帧)。行情来自捕获的 raw journal 原样回放或合成生成器;订单按 [hft-sim](../../crates/hft-sim/README.md) 的撮合器成交(与模拟账户、DST 同一套代码);场景文件以种子驱动注入断连、序号缺口、延迟回报、灰色故障、限速拒绝、部分成交等故障。它服务三件事:统一升级后的本机全链路回环延迟([10 §5](../../design/10-语言分工与代码组织.md))、wire 级混沌测试、hft-core/reconciler/kill-switch 在测试网/模拟盘不覆盖某接口时的集成测试(作为上线门槛 ⑥ 的替代证据之一)。它只在 research 主机、CI 与参考机上不属于 hft-core 线程计划的核上运行,**生产配置出现 mock 端点即拒绝启动**([hft-core](../hft-core/README.md#配置文件字段) `endpoint_override`)。

**可选扩展**(触发条件:统一升级主机(或 `metal`)后做主机内 tick-to-trade 延迟回归、需要 wire 级混沌测试,或某个 core venue 的测试网/模拟盘缺少首个 core 策略要用的接口时)。core 的功能与正确性测试不依赖它:用**交易所测试网/模拟盘**(Binance 现货与 USD-M testnet、OKX 模拟盘、Bitget 模拟盘;可用性与差异逐所待核实,见各 `venue-*`)加 [hft-testkit](../../crates/hft-testkit/README.md) 的 `VenueProtocol` 级假交易所(撮合器即 hft-sim,同时支撑 DST)。下文是启用时的规格。

## 目录

- [使用本组件](#使用本组件)
  - [端点与协议](#端点与协议)
  - [行情源](#行情源)
  - [撮合与回报](#撮合与回报)
  - [故障注入](#故障注入)
  - [场景文件](#场景文件)
  - [回环延迟测量](#回环延迟测量)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
mock-exchange run --scenario scenarios/nightly-loopback.toml [--seed 42] [--report out/]
mock-exchange list-venues                       # 打印已编译进来的 venue 与其支持的端点
mock-exchange gen-ca --out /etc/hft/mock-ca/    # 生成测试 CA 与各 venue 主机名证书
```

**被测的是生产二进制**:回环与混沌用同一个 release 归档里的 hft-core,不存在"测试构建"。测试配置用 `venues.<v>.endpoint_override = "wss://mock-binance-usdm.test:9443"` 等指向 mock,并用 `test.extra_ca = /etc/hft/mock-ca/ca.pem` 让 hft-core 额外信任 mock CA;**额外 CA 只在配置含 `endpoint_override` 时加载**,生产配置禁止这两项(出现即退出 2,见 [hft-core 配置](../hft-core/README.md#配置文件字段))。

### 端点与协议

| venue | 行情 | 下单 | 用户流 | REST |
|---|---|---|---|---|
| Binance USD-M | `/public/ws`(`bookTicker`、`depth@100ms`)、`/market/ws`(`aggTrade`、`markPrice`) | `ws-fapi`:`session.logon`、`order.place/modify/cancel` | listenKey 流(`ORDER_TRADE_UPDATE`、`ACCOUNT_UPDATE`) | `depth` 快照、`openOrders`、`order`(查单)、`allOpenOrders`(撤)、`countdownCancelAll`、`positionRisk`、`balance`、`userTrades` |
| Binance 现货 | `bookTicker`、`depth` | WS API `order.place/cancel` | 用户数据流(`executionReport`、`outboundAccountPosition`) | `depth` 快照、`openOrders`、`order`、`account`、`myTrades` |
| OKX 永续/现货 | `bbo-tbt`、`books`、`trades`、`funding-rate`、`mark-price` | WS `order`/`amend-order`/`cancel-order`、`login` | `orders`、`positions`、`account` | `orders-pending`、`order`、`positions`、`balance`、`fills` |
| Bitget 永续/现货(UTA) | `books1`、`books`、`trade`、`ticker` | WS `place-order`/`modify-order`/`cancel-order`、`login` | `order`、`position`、`account` | UTA 查单、仓位、资产、成交 |

频道与端点名以各 `venue-*` README 为准(上表只示意覆盖面,逐所待核实)。

每接入一个可选的 venue,同时在该 `venue-*` 的 `mock` 特性里提供反向编解码并在此表加一行。端点路径、字段、错误码以 `venue-*` 的实现为准(mock 不另写一份协议知识)。

每个 venue 一个监听端口(`9443 + venue 序号`),按 SNI 区分主机名;TLS 1.3(rustls 服务端),支持会话票据(测 hft-core 的会话复用)。

### 行情源

| 模式 | 来源 | 用途 |
|---|---|---|
| `replay` | 捕获的 `raw/<venue>` journal(`RawRx` payload 原样),按原始 `rx_tsc` 间隔发送,可用 `speed` 倍速或 `max` 尽快 | 金样级真实性;延迟回环;解码回归 |
| `synthetic` | 种子驱动的随机游走簿(每 inst 的 tick/lot 来自 refdata),可配置每秒更新数与突发 | 压力、长时混沌、确定性 DST 集成 |
| `script` | 场景文件中逐条给出的事件(价格路径) | 策略触发、LegLink、特定故障的精确复现 |

`replay` 模式下,序号类字段(Binance `u`/`pu`、OKX `seqId`/`prevSeqId`、Bitget `seq`/`pseq`)保持原值;故障注入改写它们时同时改写后续报文以保持"只有注入点是缺口"。

### 撮合与回报

- 每个 (venue, inst) 一个由行情源驱动的 L2 视图;订单按 `hft-sim` 的撮合规则成交:IOC/FOK 立即对照当前簿、post-only 挂在簿上按 hft-sim 的挂单模型(默认 `TradeThrough`)成交、GTC 挂单被后续行情穿越时成交。
- 回报顺序默认 Ack → (PartialFill…) → Fill;场景可改为乱序或重复(见故障注入)。
- 账户状态(余额、仓位、挂单)按交易所规则维护,REST 查询与用户流从同一状态出,保证 reconciler 测试有真值。
- 死手:实现 Binance USD-M `countdownCancelAll` 与 OKX `cancel-all-after` 的语义(到期撤全部),其他 venue 不实现(与真实一致,供 kill-switch 测试)。
- 限速:按 `venue-*` 限速表维护令牌桶;超限返回该所真实的错误码与 HTTP 状态。

### 故障注入

| 故障 | 参数 | 期望 hft-core 行为(验收点) |
|---|---|---|
| `Disconnect` | 连接选择器、方式(`FIN`\|`RST`\|WS close code\|TLS `close_notify` 缺失)、时刻 | `ConnState{Down}`;备用连接接管;重连后重同步(快照 + 序号对齐) |
| `SeqGap` | 丢弃 N 条增量或跳号 | 检测缺口 → `Resync` → 快照 → `Live`;期间该 inst 不武装 |
| `DelayedAck` | 延迟分布(常数/对数正态/来自 journal 的 `ack − sent` 分布)、选择器 | 槽保持 `PendingNew`;超 `StaleOrderSweep` 阈值 → 查单 |
| `DropAck` | 概率 | 订单进 `Unknown`,查单收敛;期间该 group 禁开 |
| `GrayFailure` | 类型:`ping_ok_no_data`(心跳正常但无行情)、`slow_drip`(每帧拆成小段、段间延迟 X ms)、`stale_book`(行情继续但价格冻结)、`ack_blackhole`(下单连接活着但不回) | 前两类在连接层面是健康的(有回帧):主路无数据而备路有 → 切备(双路落差);全部连接都无数据 → watchtower `FeedSilent` P1,对冲仍按最新报价 + `rr_max_cross` 上限定价(IOC 未成交 → `HedgeRetry`);下单黑洞 → `Unknown` + 切备用下单连接 |
| `RateLimit` | 在第 N 条请求后返回 429/418/交易所错误码,持续 T | `RateLimitHit` → 令牌桶清零 + 退避;不重复轰炸 |
| `PartialFill` | 拆分比例序列 | LegLink 按实际成交量出第二腿;余数累计 |
| `Reject` | 错误码(余额不足、价格带外、reduce-only 冲突) | 槽 `Rejected`;策略不重试同一意图 |
| `FillBeforeAck` / `DuplicateReport` / `OutOfOrder` | 选择器 | OMS 幂等处理([hft-oms](../../crates/hft-oms/README.md) 属性测试的 wire 级对应) |
| `UnattributedFill` | 成交回报去掉 client id;查单响应延迟(短于或长于 `gw.unmatched_hold_ms`) | gw 暂存后归属到槽,或超时发 `ExternalFill{Unknown}` 后再归属时去重;第二腿数量不超过当前裸量,不重复对冲 |
| `QuietBook` | 某 venue 的 BBO 推送停止(盘口不变)T 秒(默认 10),连接照常回 pong | **必过**(testing QF-1):以该 venue 为第二腿的开仓放行;第一腿成交后第二腿按最新报价照常发出,不进 `HedgeRetry`、不 `Broken` |
| `ConnSilent` | 某 venue 的 BBO 连接停止发送任何帧(含 pong)T 秒(> `conn_liveness_ms`),TCP 不断开 | 以该 venue 为第二腿的开仓被拒(`LINK_STALE`);已在途的对冲门控拒绝、按 `HedgeRetry` 处理;hft-venue 判该连接失活并重连,恢复 Live 后对冲发出 |
| `ForcedRotate24h` | 连接年龄 | 在 `rotate_after_h` 之前主备已切换,无订单丢失 |
| `ClockSkew` | 交易所时间偏移 ± X ms | `recvWindow` 拒单 → `ClockSkew` 事件 → 停新单 |
| `RestOutage` | REST 5xx/超时比例 | reconciler 不误判;查单失败不改变槽状态 |

### 场景文件

```toml
schema = 1
seed = 42
duration_s = 3600
[venues.binance-usdm]
feed = { mode = "replay", journal = "fixtures/raw/binance-usdm/2026-10-01T03", speed = 1.0 }
accounts = [{ name = "test", balances = { USDT = "10000" } }]
[venues.okx-swap]
feed = { mode = "synthetic", insts = ["SOL-USDT-SWAP"], updates_per_s = 200, burst = { every_s = 60, x = 20, len_ms = 500 } }
[[faults]]
at_s = 600
kind = "Disconnect"
target = { venue = "okx-swap", conn = "feed-primary" }
how = "RST"
[[faults]]
at_s = 900
kind = "GrayFailure"
target = { venue = "binance-usdm", conn = "gw-primary" }
type = "ack_blackhole"
duration_s = 20
[[expect]]                      # 场景结束时由 mock 自检的断言,失败则退出 1
kind = "no_naked_exposure_over_usd"
value = 50
```

`[[expect]]` 只断言 mock 能看到的事实(收到的订单、账户状态、连接行为);hft-core 内部状态的断言由测试脚本读 hft-core 的 journal 完成。

### 回环延迟测量

- mock 与 hft-core 在同一主机、不同核(mock 用参考机上 hft-core 线程计划之外的核,或 research 主机的非热核),共享同一 TSC。
- mock 不改动报文内容:按连接记录"第 k 帧写出时的 `mock_tx_tsc`";收到订单帧最后一个字节时记录 `mock_rx_tsc`;经 hft-core journal 中 `OrderDecision` 的触发 seq 找到触发它的行情帧(`script` 行情时触发帧由场景确定)。
- 输出:wire-to-wire `mock_rx_tsc − mock_tx_tsc` 直方图,以及从 hft-core journal 读出的主机内分段(`sent_tsc − rx_tsc` 等);回环不经网卡,没有 `hw_rx`,分段按 `SW_TS` 口径报告,**不与生产 `sent_tsc − hw_rx` 混算**([latency-budget](../../docs/latency-budget.md))。

## 实现说明

<details><summary>实现要点</summary>

- 每 venue 一个监听线程 + 每连接一个线程(阻塞 I/O,连接数 < 50);行情发送用单独的发送线程按时间表发帧(忙等到时刻,保证发送时刻精度 < 5 µs)。mock 是测试工具,允许线程与阻塞,但**发送时刻与收帧时刻必须精确**,否则回环测量失真。
- 反向编解码:`enum MockVenue { BinanceUsdm, BinanceSpot, OkxSwap, OkxSpot, BitgetUsdtFutures, BitgetSpot, … }` 分派到 `venue_<x>::mock::{encode_market, decode_order_frame, encode_order_event, encode_rest_response}`;这些函数是 `venue-*` 的 `mock` 特性提供的纯函数(与正向解码共享结构体与字段映射)。
- 撮合与账户:调用 `hft-sim` 的纯核心 `SimExchange`;mock 只把 wire 报文转换为它的输入并把回报编码回去。
- 所有随机性来自场景种子(每个故障与行情生成器派生子种子);同一场景 + 种子 → 同一报文序列与时间表。
- 报告目录:`faults.jsonl`(每个注入点的时刻与目标)、`orders.jsonl`(收到的每个订单帧与回报)、`loopback.hdr`、`summary.json`。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/main.rs` | 子命令 |
| `src/scenario.rs` | 场景解析、种子派生、断言 |
| `src/tls.rs` | rustls 服务端、测试 CA |
| `src/ws.rs` / `src/rest.rs` | WS 与 REST 服务端外壳 |
| `src/venues.rs` | `MockVenue` 分派 |
| `src/feed.rs` | replay/synthetic/script 行情源与发送时间表 |
| `src/matching.rs` | `hft-sim` 装配、账户状态 |
| `src/faults.rs` | 故障注入器 |
| `src/measure.rs` | 回环 TSC 记录与直方图 |

</details>

## 延迟与资源契约

| 指标 | 目标 |
|---|---|
| 行情发送时刻误差 | < 5 µs p99(相对时间表) |
| mock 自身处理一条订单帧(收完 → 回报写出) | < 5 µs p99(不能成为回环的瓶颈) |
| 吞吐 | 单 venue 回放 ≥ 20 000 帧/s |
| 资源 | 2–3 个核(参考机上 hft-core 线程计划之外的核,或 research 主机) |

## 测试与验收

| 层 | 用例 |
|---|---|
| 协议一致性 | 对每个 venue:用真实交易所捕获的订单回报样本(fixtures)与 mock 编码结果逐字段比较;hft-core 的 `venue-*` 解码器解 mock 报文 = 解真实报文 |
| 确定性 | 同一场景 + 种子运行两次,`orders.jsonl` 与发送时间表逐字节相同 |
| 故障 | 故障表每一行一个场景,断言"期望 hft-core 行为"列(读 hft-core journal) |
| 回环(启用后每夜,只在统一升级后的升级主机类参考机上判门槛) | `nightly-loopback.toml`(Binance USD-M replay + OKX script,生产二进制中已注册的策略以场景参数对 mock 以 `Live` 运行;生产二进制不含 hft-testkit 的测试策略)1 h:主机内 `sent_tsc − rx_tsc` p50 ≤ 20 µs、p99 ≤ 60 µs([latency-budget](../../docs/latency-budget.md));超预算即红 |
| 混沌(每周) | `weekly-chaos.toml`:全部故障类型随机组合 4 h;零裸敞口 > $50、零 panic、所有 `Unknown` 在 10 s 内收敛 |
| kill-switch | mock 的 Binance Spot/Bitget REST 被 kill-switch 调用,挂单清零时间 ≤ 5 s |

## 依赖

- 内部:`venue-*`(`mock` 特性)、`hft-sim`、`hft-refdata`、`hft-journal`(只用 `JournalReader`,读捕获的 raw journal)、`hft-event`。
- 外部:`rustls`(服务端)、`rcgen`(测试 CA)、`tungstenite`、`tiny_http` 或同类阻塞 HTTP 服务端、`serde`/`toml`、`hdrhistogram`。
- 禁止:被任何生产 app 依赖;`tier: dev`,只进测试与基准。

## 延伸阅读

- [10 §5](../../design/10-语言分工与代码组织.md)(测试金字塔)、[05 §8](../../design/05-CEX执行层.md)(失败模式清单)、[04 §4](../../design/04-行情接入层.md)(重同步)、[11](../../design/11-实施路线图.md)
- [testing](../../docs/testing.md)、[hft-testkit](../../crates/hft-testkit/README.md)(DST 是事件级、进程内;mock-exchange 是 wire 级、跨进程,两者互补)、[fixtures](../../fixtures/README.md)
- 现有:Trading `tools/local_mock_exchange.py`(旧采集系统的本地 mock,只作参考)

## 已知限制与后续工作

- 回环不经过网卡与内核网络栈的真实路径(`lo` 不打硬件戳,也不走 ENA 队列),网卡 → 用户态分段只能在生产直方图里看。
- 交易所的撮合细节(自成交保护、价格带、RPI 订单不可见)只按 `hft-sim` 的简化模型实现。
- Binance Spot SBE 行情、FIX 下单:可选扩展(触发:接入 Binance Spot SBE/FIX 时)。

### 开发备忘

<details><summary>给实现者</summary>

- **需要 venue crate 配合**:每个 `venue-*` 增加 `mock` 特性(反向编码行情/回报/REST 响应、解码订单帧);这是 venue crate 规格需要覆盖的公开入口,先做 `venue-binance`,再 `venue-okx`、`venue-bitget`。
- **开放问题**:回环中"行情帧 → 订单"的关联在 `replay` 模式下依赖 hft-core journal 的 `OrderDecision` 记录其触发事件的 seq,再由 seq 找到 `rx_tsc` 与 raw 帧;确认 `OrderDecision` 带触发 seq(event-model 归属)。
- 实现顺序:TLS + WS 外壳 → Binance USD-M replay 行情 → 下单回报 → REST → 故障注入 → OKX、Bitget → 回环测量。

</details>
