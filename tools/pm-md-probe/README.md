---
description: "Polymarket 行情接入验收探针 pm-md-probe:用生产行情路径(hft-net + hft-venue FeedRuntime + venue-polymarket)订阅线上 5 分钟盘,按事件重建每个代币的簿,逐事件输出最优价供与独立录制比对;只用于验收,不进生产"
kind: crate
tier: cold
scope: optional
status: impl
---

# pm-md-probe

## 概要

`pm-md-probe` 验收 [venue-polymarket](../../crates/venue-polymarket/README.md) 的行情接入:它不经 sequencer 与策略,直接用 hft-net 建连、hft-venue `FeedRuntime` 管订阅与解码,把 feed 任务发出的 `L2Snapshot`、`L2Batch`、`Trade` 重建成每个代币的簿,每个事件写一行 JSON(含重建后的最优买卖价)。配套脚本从 Gamma API 取即将开始的窗口的代币 id(`scripts/discover.py`),并把输出与独立录制(pmrec)的同一窗口逐条比对(`scripts/compare.py`)。它**只用于验收**:不下单、不读密钥、不进任何生产发布归档;构建走手动 `Probe build` 工作流。触发条件:Polymarket 接入提案的 P1 验收([提案](../../.agents/notes/proposed/architecture/2026-10-19-polymarket-venue.md))。

## 目录

- [使用本组件](#使用本组件)
  - [命令行](#命令行)
  - [产出](#产出)
  - [验收步骤](#验收步骤)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 命令行

```text
pm-md-probe --bindings FILE --seconds N --out FILE --dir DIR --src-ip IP --bind-device DEV [--host HOST]
```

| 参数 | 含义 |
|---|---|
| `--bindings` | `discover.py` 的输出:`[{"slot","token","start_ts","asset"}]`,按文件顺序成为 InstId 1..n |
| `--seconds` | 运行时长 |
| `--out` | JSON 行输出 |
| `--dir` | 运行时要求的 raw journal 目录(原始帧记录关闭,只有状态记录) |
| `--src-ip`、`--bind-device` | 本机源地址与网卡(hft-net 要求显式绑定) |
| `--host` | 默认 `ws-subscriptions-clob.polymarket.com` |

### 产出

每行一个事件:

| `k` | 字段 |
|---|---|
| `snap` | `slot`、`ex_ms`、`rx_ns`、`id`(`snap_id`)、`bb`、`ba` |
| `batch` | `slot`、`ex_ms`、`rx_ns`、`prev`、`last`、`bb`、`ba` |
| `trade` | `slot`、`ex_ms`、`rx_ns`、`px`、`qty`、`side` |
| `conn` | 连接状态事件 |

`bb`/`ba` 为重建簿的最优买/卖价(px_exp −3 的整数,空边为 −1)。结束时打印各类事件数与 feed 计数器。

### 验收步骤

1. `discover.py --out bindings.json --windows 2` 取下两个窗口 × 4 个资产的 16 个代币,打印第一个窗口的开始时间。
2. 在第一个窗口开始前启动,`--seconds` 覆盖两个窗口结束后 60 s。
3. `compare.py --probe out.jsonl --bindings bindings.json --pmrec <pmrec raw/日期目录>`:每个代币在两边都覆盖的时间段内按交易所时间戳(ms)分组比对,交易所 `price_change` 给出的 `best_bid`/`best_ask` 与探针每个 `batch` 之后的最优价在两边都有的每个时间戳上相等;只有一边有的时间戳(任一录制方的重连缺口,Polymarket 会以 1013 断开发送缓冲满的连接)不超过 1%(或 5 个)。

## 实现说明

<details><summary>实现要点</summary>

- refdata 在程序内构建:USDC 加每个绑定一个代币资产与一个现货 inst(tick 0.01、lot 0.000001,与 venue-polymarket 测试相同)。
- 线程:`warm`(conn-builder:`SessionDriver` 建连与交接)、`feed`(`FeedRuntime::poll_once`)、主线程消费 md 环与 bulk 环。启动时为每个 inst 发 `FeedCmd::Subscribe{BOOK | TRADE}`。
- `FeedCfg`:`routes = Md`、`depth = TopN`(100 档)、原始帧记录关闭。
- 单文件 `src/main.rs`;脚本在 `scripts/`。

</details>

## 延迟与资源契约

无延迟门槛(验收工具)。主线程在空闲时睡眠 200 µs;bulk 环 64 MiB。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

允许:`hft-types`、`hft-refdata`、`hft-event`、`hft-rt`、`hft-journal`(`writer`)、`hft-net`、`hft-venue-proto`、`hft-venue`、`venue-polymarket`;外部 `serde`、`serde_json`、`zerocopy`。不被任何 crate 依赖。

## 延伸阅读

- [venue-polymarket](../../crates/venue-polymarket/README.md)
- [接入提案](../../.agents/notes/proposed/architecture/2026-10-19-polymarket-venue.md)

## 已知限制与后续工作

- 不经 sequencer:hft-core 装配随提案 P1b(运行期绑定命令与市场发现)完成后,验收改用正式进程。
