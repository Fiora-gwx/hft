---
description: "M1.5 延迟探针的纵切片程序 latency-slice:主网公共 BBO(一个 venue 的 feed-bbo 任务)+ 简化 sequencer(按 BBO 事件限速触发 quote 动作)+ gw(测试网 WS API 下单与撤单),写 seq 与 raw journal,离线出 kernel_rx/ingest/decide/egress/order_rtt/rep_to_order 等直方图;另有 hft-net host 用例的探测模式与无网络的 --dry-run;只用于探针,不进生产"
kind: crate
tier: cold
scope: core
status: impl
---

# latency-slice

## 概要

`latency-slice` 是 [M1.5 延迟探针](../../design/11-实施路线图.md#31-m15-延迟探针)的纵切片程序:在按需租用的 `c8g.4xlarge` 上,把 [hft-net](../../crates/hft-net/README.md) + 一个 venue 的 BBO feed([hft-venue](../../crates/hft-venue/README.md) `FeedRuntime`,主网公共行情)+ **简化 sequencer**(本程序自带)+ gw(`GwRuntime`,测试网/演示环境 WS API 下单)按 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划) `c16` 的子集绑核运行,下单负载按 [latency-probe](../../strategies/latency-probe/README.md) 的 `quote` 口径(远离对手价的最小名义 PostOnly,Ack 后同一事件撤单)。来源标志直接取环元素，与 hft-core 的 [接收来源契约](../../docs/subsystems/event-model.md#时间戳语义) 一致。它写与 hft-core 同格式的 seq journal(行情记录、回报、`OrderDecision`、`LatencySample`、`ClockStatus`)与 raw journal,运行结束后用 [hft-telemetry](../../crates/hft-telemetry/README.md) 对 seq journal 出分段直方图(HW 与 SW 时间戳分开)、按事件加权统计(突发/平静分层、`kernel_rx` 上下界)与探针样本分层。它还提供 hft-net host 用例 NET-370–NET-374 需要的探测模式,以及不连网、用 [hft-testkit](../../crates/hft-testkit/README.md) 生成行情驱动全部线程的 `--dry-run`(CI 验证线程与直方图管线)。它**只用于探针**:不承载策略、OMS、风控与账本,不进任何生产发布归档,只允许测试网下单端点,主网下单端点在加载配置时硬拒绝。M3 末满订阅探针与门槛 ⑩ 用正式的 hft-core,不用本程序。

## 目录

- [使用本组件](#使用本组件)
  - [命令行](#命令行)
  - [配置文件](#配置文件)
  - [密钥](#密钥)
  - [安全约束](#安全约束)
  - [线程布局](#线程布局)
  - [简化 sequencer](#简化-sequencer)
  - [产出](#产出)
  - [dry-run](#dry-run)
  - [host 探测模式](#host-探测模式)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 命令行

```text
latency-slice [--config <toml>] [--minutes <n>] [--ts-expect-hw] [--print-conn-stats] [--ballast <n>] [--no-orders] [--out <dir>]
latency-slice --dry-run [--config <toml>] [--seconds <n>] [--seed <u64>] [--out <dir>]
latency-slice --report <journal-root> [--out <dir>]
latency-slice --check-config [--config <toml>]
latency-slice --build-only <n> [--print-napi] [--config <toml>]      # NET-371
latency-slice --idle-polls <n> [--config <toml>]                     # NET-372
latency-slice --two-packet-stamp-probe <n>                           # NET-373
latency-slice --loopback-stream-minutes <n>                          # NET-374
```

- 配置文件:`--config`,否则环境变量 `HFT_SLICE_CONFIG`,否则当前目录的 `latency-slice.toml`;`--two-packet-stamp-probe` 与 `--loopback-stream-minutes` 不读配置。
- 命令行覆盖配置:`--minutes` → `slice.minutes`;`--ts-expect-hw` → `net.ts_expect_hw = true`;`--ballast <n>` → `ballast.conns`(0 = 无压舱);`--no-orders` → `orders.enabled = false`(只测行情分段);`--out` → `slice.out_dir`。
- 主运行结束后自动对本次 journal 执行 `--report`;`--report` 也可单独对已有 journal 根目录运行(离线)。
- `--print-conn-stats`:结束时把每条 BBO 连接的 `ConnStats`(`frames`、`ts_reads`、`hw_ts_reads`、`multi_skb`,以及 `ts_missing_reads = ts_reads − hw_ts_reads`,即期望 HW 戳而缺失的读取数)与 feed 任务的计数器写到标准输出(`conn_stats {…}` 行)与报告。
- 退出码:**0** 正常结束;**1** 运行中触发安全停机(见[安全约束](#安全约束)),报告照常生成;**2** 配置、密钥或参数错误(含主网下单端点);**3** 运行时初始化失败(绑核、journal、建连器、`TscClock` 校准)。

### 配置文件

TOML;未列出的键取默认值,未知键 → 退出 2。

| 键 | 类型 | 默认 | 含义 |
|---|---|---|---|
| `slice.product` | `usdm \| spot` | 必填 | 探针的产品线(feed 与 gw 同一 venue,[venue-binance](../../crates/venue-binance/README.md)) |
| `slice.minutes` | u32 | 60 | 主运行时长;到时进入收尾(撤单、等回报、关 journal、出报告) |
| `slice.out_dir` | 路径 | `./slice-out` | 报告与证据输出目录 |
| `slice.epoch` | u16 | 0 | gw 生成 client id 的 epoch(`ClientOrderId.epoch`,bit 15 恒 0) |
| `feed.host` / `feed.port` | 字符串 / u16 | `usdm`:`fstream.binance.com`/443;`spot`:`stream.binance.com`/9443 | 主网公共行情主机;路径取 `VenueProtocol::endpoint` |
| `feed.ips` | IPv4 列表 | 空(解析主机名) | 钉住的行情地址 |
| `feed.bbo_conns` | u8 | 2 | BBO 赛马连接数(hft-venue `feed.bbo_conns`) |
| `orders.enabled` | bool | true | 是否下单 |
| `orders.host` / `orders.port` | 字符串 / u16 | `spot`:`demo-ws-api.binance.com`/443;`usdm`:无默认(演示环境 ws-fapi 端点待核实,venue-binance 开发备忘第 14 条) | 测试网/演示环境 WS API 主机;只允许[安全约束](#安全约束)认可的主机 |
| `orders.ips` | IPv4 列表 | 空 | 钉住的下单地址 |
| `orders.api_key_env` | 字符串 | `HFT_SLICE_API_KEY` | 存放 API key 的环境变量名 |
| `orders.secret_env` / `orders.secret_file` | 字符串 / 路径 | `HFT_SLICE_ED25519` / 空 | Ed25519 私钥的来源(见[密钥](#密钥));两者恰好给一个 |
| `orders.max_open_orders` | u16 | 2 | 同时未终态的探测单上限 |
| `orders.max_order_notional` | 十进制字符串 | `"50"` | 单笔名义上限(报价资产);算出的数量超过它 → 安全停机 |
| `orders.max_orders` | u32 | 20 000 | 本次运行新单总数上限;达到即停止探测 |
| `net.src_ip` / `net.bind_device` | IPv4 / 字符串 | 必填(主运行与 NET-371/372) | 交易网卡(ENI-B)上的源地址与网卡名(hft-net `src_ips`/`bind_device`) |
| `net.ts_expect_hw` | bool | false | hft-net `ts_expect_hw` |
| `[[insts]]` | 表数组 | 至少 1 个 | 探测的 inst:`symbol`、`base`(基础资产名)、`tick`、`lot`、`min_qty`、`min_notional`(交易所过滤器原文,十进制字符串,取主网与演示环境中较严的值) |
| `probe.probe_period_ms` | u32 | 1 000 | 同一 inst 两次挂单的最小间隔(同 latency-probe `probe_period_ms`) |
| `probe.burst_period_ms` | u32 | 0 | 突发层触发的最小间隔(0 = 不另设;非 0 时须 ≤ `probe_period_ms`) |
| `probe.far_bp` | u32 | 300 | PostOnly 价格离对手价的距离(bp,50–1 000) |
| `probe.quote_side` | `buy \| sell \| alternate` | `buy` | 挂单方向;`spot` 一律只挂买单 |
| `probe.max_spread_bp` | u32 | 100 | (ask − bid)/mid 超过它不探测 |
| `probe.size_margin_pct` | u32 | 10 | 单笔名义余量(%) |
| `probe.quote_rest_ms` | u32 | 2 000 | 探测单未终态的时限;到时重发撤单 |
| `probe.max_consec_fail` | u32 | 5 | 连续交易所拒单达到即停止探测 |
| `ballast.conns` | u8 | 0 | 压舱深度连接数(只读不解码) |
| `ballast.symbols` | 字符串列表 | 空 | 压舱订阅的符号(每连接全部订阅,`DepthTopN` 20 档) |
| `journal.hot_dir` | 路径 | `/dev/shm/hft-slice` | 热段目录(必须在 tmpfs) |
| `journal.cold_dir` | 路径 | `<out_dir>/journal` | 冷副本根目录(报告读这里) |
| `journal.segment_mib` | u32 | 64 | 段大小 |
| `journal.raw` | `on \| off` | `on` | raw-feed 数据帧是否写(hft-venue `feed.raw.bbo`);raw-gw 始终全写 |
| `clock.phc` | 路径 | 空(主运行自动探测 `/dev/ptp_ena`、`/dev/ptp0`) | PHC 设备;有 PHC 时 `ClockStatus` 带 PHC 读数,没有时 `phc_ns` 取同一对的 REALTIME 读数(此时只有 SW 数据集) |
| `report.burst_gap_ns` | u32 | 20 000 | 突发/平静分层阈值([testing §8.2](../../docs/testing.md#82-延迟验收的测法) 第 9 条;初值 = [latency-budget §3](../../docs/latency-budget.md#3-cex-路径预算) `kernel_rx`(`c32`)的 p99 目标,待本探针标定) |
| `report.cal_max_err_ns` | u32 | 1 000 | 逐样本误差界阈值([latency-budget §2.1](../../docs/latency-budget.md#21-跨时钟域换算)) |
| `threads.<name>` | `{core, idle}` | 见[线程布局](#线程布局) | 每个线程的核号(`core = -1` 不绑核)与空闲策略(`spin` 或 `park`) |

### 密钥

- API key 只从环境变量(`orders.api_key_env`)读;Ed25519 私钥只从环境变量(`orders.secret_env`,PKCS#8 PEM 正文或 64 位十六进制种子)或本地文件(`orders.secret_file`,PKCS#8 PEM)读。配置文件中出现 `api_key`、`secret`、`private_key` 等键 → 退出 2;Linux 上 `secret_file` 的权限对组或其他用户可读 → 退出 2。
- 密钥只在 conn-builder 线程的 `session.logon` 中借用(hft-venue `SessionDriver`);不写 journal、报告与日志;raw-gw 中登录帧只记元数据(hft-venue)。
- 测试网/演示环境的 key 在该环境的 API 管理页创建,只开交易权限,IP 白名单加入探针实例的出口地址,用完撤销。

### 安全约束

1. **只允许测试网下单端点**:`orders.host` 的小写形式在主网主机表中(`ws-fapi.binance.com`、`ws-api.binance.com`、`fapi.binance.com`、`api.binance.com`、`api1`–`api4.binance.com`、`stream.binance.com`、`fstream.binance.com`,以及任何以 `.binance.com` 结尾且不含 `demo`、`testnet` 的主机)→ 退出 2;不含 `demo` 或 `testnet` → 退出 2。gw 以 `VenueEnv::Testnet` 运行。
2. **挂单上限**:同时未终态的探测单 ≤ `orders.max_open_orders`;本次运行新单 ≤ `orders.max_orders`;单笔名义 > `orders.max_order_notional` → 安全停机。
3. **最小名义**:单笔数量取满足 `qty × px ≥ min_notional × (1 + size_margin_pct/100)`、`qty ≥ min_qty`、`qty` 为 lot 整数倍的最小值。
4. **安全停机**:触发后不再发新单,撤掉全部在挂探测单,等回报至多 5 s,写报告,退出 1。触发条件:单笔名义超限;连续拒单达到 `probe.max_consec_fail`;同一探测单撤单重发 3 次后仍未终态;收到任何成交类回报(`PartialFill`/`Fill`)。
5. 收到 SIGINT/SIGTERM:按正常收尾处理(撤单、等回报、出报告),退出 0。

### 线程布局

`c16` 计划([hft-core 线程计划](../../apps/hft-core/README.md#线程计划))的子集;核号可由 `threads.<name>.core` 覆盖。

| 线程 | 默认核 | 空闲 | 任务 |
|---|---|---|---|
| `hk` | 0 | park | 每 1 000 ms 一条 `ClockStatus`(hft-rt `sample_pair`:TSC ↔ REALTIME,有 PHC 时含 PHC;`max_error_ns` 取 `adjtimex` 的 `maxerror`;`state` 按 hft-rt `DriftTracker`)、运行时长计时、信号 |
| `persist` | 2 | park | 三个 journal 的段预创建、热段 → 冷副本复制、热段退役 |
| `warm` | 3 | park | conn-builder:`Connector` + `SessionDriver`(握手、登录、交接、回收) |
| `seq` | 4 | spin | 简化 sequencer(下节) |
| `feed-bbo` | 5 | spin | `FeedRuntime<P>`(`routes = Bbo`) |
| `feed-md` | 7 | spin | 压舱深度连接(`ballast.conns > 0` 时) |
| `gw` | 8 | spin | `GwRuntime<P>`(一个测试网账户) |

核 1 是 ENI-B 的 IRQ 专核,不放线程。环:`feedctl`(seq → feed-bbo)、`bbo`(feed-bbo → seq)、`out`(seq → gw)、`rep`(gw → seq)、`hk`(hk → seq)、`build`/`handoff`/`retire`(各热任务 ↔ warm)。

### 简化 sequencer

单线程、单写者,取环优先级 `hk > rep > bbo`,每次取一条,一条输入 = 一个事件(该事件的全部记录以 `EVENT_END` 收尾);每条输入在取出时读 TSC 作为 `proc_tsc`(记录头 `tsc`)。

| 输入 | 处理 |
|---|---|
| `ClockStatus` | 写 Meta 记录 |
| `ConnState` | 写记录;gw 下单连接 `Up` 之后才开始探测,`Down` 期间不探测 |
| `Bbo` | 写记录（接收来源标志直接取 `FeedMsg`）;更新该 inst 的最优价;按下面的条件决定是否挂单 |
| `SendReport` | 写记录;记该单该操作的 `sent_tsc`;状态不是 `Written` → 该单结束(计 `send_fail`,不产生样本) |
| `OrderEvent` | 写记录;按下表迁移该单 |

**挂单条件**(依次,全部满足才挂;与 latency-probe [quote 探测](../../strategies/latency-probe/README.md#quote-探测)第 1 步同口径的子集):`orders.enabled`;未安全停机;gw 下单连接已 `Up`;到期——`now ≥ last_probe + probe_period_ms`,或突发触发(`burst_period_ms ≠ 0`、本 `Bbo` 的 `rx_tsc ≠ 0` 且等于上一条 `Bbo` 的 `rx_tsc`、`now ≥ last_probe + burst_period_ms`);该 inst 没有未终态的探测单;未终态探测单总数 < `max_open_orders`;新单总数 < `max_orders`;`bid > 0`、`ask > 0` 且 (ask − bid) × 10⁴ / mid ≤ `max_spread_bp`(mid = `Px::mid_floor`,i128;否则计 `skipped_spread`)。到期判定只在满足其余条件时更新 `last_probe`。

**价量**:买 `px = floor_to_tick(ask × (1 − far_bp/10⁴))`、卖 `px = ceil_to_tick(bid × (1 + far_bp/10⁴))`;`qty` 按[安全约束](#安全约束)第 3 条;`tif = PostOnly`。写一条 `OrderDecision{action: New}`,随后读 TSC 作为 `decided_tsc`(写进该记录),再把 `GwCmd{Place}` 推入 `out` 环。

| 该单状态 / 回报 | 动作 |
|---|---|
| 新单 / `Ack` | 写 `LatencySample{action: New}`(触发 = 挂单的 `Bbo` 记录,`ack` = 本回报);**同一事件**写 `OrderDecision{action: Cancel}` 并发 `GwCmd{Cancel}`(触发 = 本回报记录) |
| 新单 / `Reject` | 写 `LatencySample{action: New}`(`ack` = 本回报);该单结束;连续拒单计数 +1(任何 `Ack` 清零) |
| 新单 / `OutcomeUnknown` | 立即发撤单(同上,触发 = 本回报;不写样本) |
| 撤单中 / `CancelAck`、`Expired`、`CancelReject` | 撤单的首个回报写 `LatencySample{action: Cancel}`(触发 = 引发撤单的回报记录);`CancelAck`、`Expired` 与 `CancelReject{GONE}` 使该单结束;其他 `CancelReject` 保持撤单中 |
| 任意 / `PartialFill`、`Fill` | 安全停机 |
| 未终态超过 `quote_rest_ms` | 重发撤单(触发 = 无,样本照常按首个回报写);第 3 次重发后仍未终态 → 安全停机 |

`LatencySample` 的八个时间点按 [latency-budget §1.2](../../docs/latency-budget.md#12-每笔订单再加三个) 填:点 0–4 取触发记录(行情触发取 `MarketHdr`;回报触发取 `OrderEvent` 的 `ex_ts_ns`、`hw_rx_ns`、`rx_tsc` 与该记录的 `proc_tsc`),点 5 取该操作的 `decided_tsc`,点 6 取其 `SendReport.sent_tsc`,点 7 取首个回报的 `hw_rx_ns`;`src` 中点 2 与点 7 的 HW/SW 取行情与回报环元素的来源标志，缺失不推断；SW 回报触发时 `sw_rx_ns` 取其软件接收戳,点 3–6 为 TSC;`trigger_kind` = 触发记录的 kind;`link_role = 0`;`strategy = 0`。

### 产出

运行目录 `<out_dir>/`:

| 文件 | 内容 |
|---|---|
| `journal/` | `prod/seq`、`prod/raw-feed-<venue>-bbo`、`prod/raw-gw-<venue>` 三个 journal 的冷副本(hft-journal 格式;`replayer`、hft-py 可读) |
| `report.json` | 见下 |
| `report.txt` | 同一内容的人读摘要 |

`report.json` 的节:`run`(产品线、inst、主机名、内核、`tsc_hz_nominal()`(CNTVCT 频率)、起止 UTC、是否压舱、`ts_expect_hw`、计数:Bbo 记录、新单、Ack、拒单、撤单、send_fail、skipped_spread、安全停机原因);`samples`(hft-telemetry `Telemetry` 对全部 `LatencySample` 的按日累计:每个 `HistKey` 的 `kernel_rx`、`ingest`、`decide`、`egress`、`tick_to_trade`、`rep_to_order`、`order_rtt`、`tick_to_ack` 等分段的 n/p50/p99/p99.9/max,HW 与 SW 分开,`cal_err` 误差界分布与 `unsynced_cal` 等计数);`event_weighted`(`event_weighted()`:每条行情记录的 `kernel_rx`、`ingest`、`feed_host`、`nic_to_kernel`、`kernel_to_user`,按突发/平静分层,`kernel_rx_hi` = `rx_empty_gap_ns` 上界);`stratified`(`stratified_probe()`:行情触发样本按触发记录的层);`conn_stats`(`--print-conn-stats` 时)。分层与误差界阈值取 `report.*`。

### dry-run

`--dry-run` 不连网、不需要密钥、不写 raw journal:`feed-bbo` 线程改为回放任务,先推一条 BBO 连接 `ConnState{Up}`,再用 hft-testkit `market_stream`(种子 `--seed`)生成 `Bbo` 序列,按序列的时间间隔把每条渲染成该产品线的 `bookTicker` 报文,经 `P::classify`/`peek`/`decode_md` 解码后入 `bbo` 环(同一时刻的几条共用一个 `rx_tsc`,取解码前的 TSC;`hw_rx_ns = sw_rx_ns` = 当时的 REALTIME 减 50 µs,即 SW 数据集);`gw` 线程改为回环任务,先推下单连接 `ConnState{Up}`,对 `GwCmd` 调 `P::encode`(帧丢弃)、推 `SendReport`,在 200 µs 后把交易所应答报文(下单 ACK、撤单成功)经 `P::classify_report`/`decode_report` 解码入 `rep` 环。`seq`、`hk`、`persist` 与报告和主运行相同;线程不绑核、空闲策略为 park。时长 `--seconds`(默认 5)。

### host 探测模式

| 模式 | 用例 | 做什么 | 标准输出 |
|---|---|---|---|
| `--build-only <n> --print-napi` | NET-371 | 用配置的行情端点建 n 条 `Md` 连接(不订阅),记录 `BuildEvent::Ready.napi_id` 与 `Connector::napi_ifindex` | 每条一行 `conn=<id> napi_id=<u32> ifindex=<u32\|none>`,末行 `built=<n> failed=<k>` |
| `--idle-polls <n>` | NET-372 | 建 1 条行情连接(不订阅),对它 `poll_rx` n 次 | `polls=<n> nonempty=<k> ns_per_poll=<x> recvmsg=<syscalls>` |
| `--two-packet-stamp-probe <n>` | NET-373 | 回环 TCP:对端每次发两包(各 200 B,间隔 1 ms),本端在第二包发出 3 ms 后用一次 `recvmsg` 读出并取 `SO_TIMESTAMPING` 软件戳;重复 n 次;第二包的到达时刻取其发送时刻加环境变量 `HFT_SLICE_NETEM_US`(默认 1 000 µs,与 NET-373 的 netem 延迟一致) | 每次一行 `i=<k> sw_rx_ns=<v> t1_ns=<v> t2_ns=<v> bytes=<v>`,末行 `within_50us_of_second=<pct>` |
| `--loopback-stream-minutes <n>` | NET-374 | 回环 TCP:对端持续写入,本端以固定 64 KiB 缓冲做非阻塞 `recvmsg` n 分钟(n = 0 时 200 ms) | 末行 `reads=<v> short_reads=<v> bytes=<v>` |

## 实现说明

<details><summary>实现要点</summary>

### 源文件

| 文件 | 职责 |
|---|---|
| `src/main.rs` | 参数解析、模式分派、退出码 |
| `src/cli.rs` | 命令行 |
| `src/config.rs` | TOML 配置、默认值、安全约束第 1 条、密钥读取与 PEM 解析 |
| `src/rd.rs` | 由配置生成 refdata(`RdbBuilder`):资产、venue、`VenueCaps`(`P::default_caps`)、限速(`P::rate_rules`)、inst(探测 inst 在前,压舱符号在后,id 从 1 稠密)、一个 Live 非正式账户 |
| `src/seq.rs` | 简化 sequencer(纯逻辑,输入/输出经 trait,测试直接驱动) |
| `src/journal.rs` | 三个 journal 的写者、预创建、persist、退役与读者 |
| `src/live.rs` | 主运行:网络配置、线程、环与收尾 |
| `src/dry.rs` | dry-run 的回放 feed 与回环 gw |
| `src/ballast.rs` | 压舱连接(建连、`DepthTopN` 订阅、只读不解码) |
| `src/hk.rs` | `ClockStatus` 采样(`hft_rt::sample_pair`) |
| `src/report.rs` | `--report`:hft-telemetry 的三类统计与 JSON/文本输出 |
| `src/probes.rs` | NET-371–NET-374 的探测模式(后两者只在 Linux) |
| `src/threads.rs` | 线程启动、绑核、空闲循环 |

### refdata 指数

基础资产 `px_exp = −(tick 的小数位数)`,`qty_exp = min(−8, −(lot 的小数位数))`(`px_exp` 不大于 2,即满足 hft-refdata 的 `px_exp + qty_exp ≤ −6`);报价资产(USDT)`px_exp = −6`、`qty_exp = −8`。压舱符号取 `tick = 0.0001`、`lot = 0.001`(只订阅、不解码),基础资产名 = 符号去掉报价资产后缀;与探测 inst 同名的压舱符号复用该 inst。

### 收尾顺序

到时或收到信号 → `hk` 置停止位 → `seq` 停止探测、对每个未终态探测单发撤单、等全部终态或 5 s → `seq`、`feed-bbo`、`feed-md`、`gw` 退出(各自 `retire_all`)→ `warm` 回收连接后退出 → `seq` 写完并 `seal_and_close` seq journal,feed/gw 线程 `seal_and_close` 各自的 raw journal → `persist` 复制完退出 → 报告。

</details>

## 延迟与资源契约

本程序是测量工具,不设自身预算;它测的分段与预算归 [latency-budget](../../docs/latency-budget.md)。热线程(`seq`、`feed-bbo`、`feed-md`、`gw`)在稳态不分配、不做阻塞系统调用:`seq` 的订单表与环都是启动时预分配的定长数组,journal 追加零分配(hft-journal);`feed-md` 的压舱读取只做 `poll_rx` 与丢帧。结果只在 [M1.5 探针 Agent Note](../../.agents/notes/proposed/architecture/2026-10-03-latency-probe-milestone.md) 中解读,不作门槛。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

| 依赖 | 理由 |
|---|---|
| `hft-types`、`hft-refdata`、`hft-event`、`hft-rt`、`hft-journal`(`writer`)、`hft-net`、`hft-venue-proto`、`hft-venue` | 与 hft-core 相同的运行时部件 |
| `venue-binance` | M1.5 的 venue |
| `hft-telemetry` | 报告 |
| `hft-testkit` | `--dry-run` 的行情生成器(开发构建制品才含本程序) |
| `toml`、`serde`、`serde_json` | 配置与报告(冷路径) |
| `base64` | PEM 解码 |
| `libc` | 信号、NET-373/374 的回环 socket |

本程序不被任何 crate 或 app 依赖;不进生产发布归档(只在开发构建制品中)。

## 延伸阅读

- [architecture-v2/11 §3.1 M1.5 延迟探针](../../design/11-实施路线图.md#31-m15-延迟探针)、[M1.5 探针 Agent Note](../../.agents/notes/proposed/architecture/2026-10-03-latency-probe-milestone.md)
- [latency-budget](../../docs/latency-budget.md)、[testing §8.2](../../docs/testing.md#82-延迟验收的测法)、[latency-probe](../../strategies/latency-probe/README.md)
- [hft-net host 用例](../../crates/hft-net/TESTS.md)(NET-370–NET-374)、[deploy 探针清单](../../deploy/README.md)

## 已知限制与后续工作

- 回报只取 WS API 响应(下单 ACK、撤单与改单应答);USDT-M 的 listenKey 用户流不建(conn-builder 不处理 `UserStream` 的建连请求,gw 的该请求一直挂起),现货不订阅用户流,因此看不到成交推送:PostOnly 远离对手价且 Ack 后立即撤单,成交概率极低,一旦从应答中看到成交即安全停机。
- USDT-M 演示环境的 ws-fapi 端点待核实(venue-binance 开发备忘第 14 条);核实前 `usdm` 产品线必须显式给出 `orders.host`。
- BBO 补帧(snap-fetch REST)不运行:hft-venue 的 snap-fetch REST 随 M3;连接恢复后等交易所自然推送。

### 开发备忘

<details><summary>给实现者</summary>

- 测试驱动 `seq.rs` 时不起线程:`Seq` 对输入环与输出端泛型(测试用 `Vec` 实现),journal 用 hft-journal 的 `mem-backend`(Windows 与 Linux 同一套测试)。
- 主运行与 dry-run 共用 `seq`、`hk`、`persist` 与报告;差别只在 `feed-bbo` 与 `gw` 两个线程装的任务。

</details>
