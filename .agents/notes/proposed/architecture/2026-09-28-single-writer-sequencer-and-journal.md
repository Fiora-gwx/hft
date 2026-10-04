# Agent Note: 单写者 sequencer 与 journal 即真相(LMAX 形态)

Status: proposed

> **第八轮(2026-10-02)**:第 5 条的"假设回放"驱动不再是 core;策略评估改用模拟账户,journal 行情重演只用于排障;见 [sim-accounts-not-backtest](2026-10-02-sim-accounts-not-backtest.md)。单写者与 journal 即真相不变。

## Problem

现有雏形的状态散落在 `logs/*.json`、CSV 与多个 Python 进程的内存里,测试/正式两个实例各持一份仓位;事故无法复现,对账靠三方猜测,补丁要靠 `sync-code.sh` 同步两套代码([research/00 §2](../../../../design/research/00-existing-codebase-inventory.md)、[12 §1](../../../../design/12-风险与未决问题.md))。重建需要一个核心形态,同时满足:主机内 µs 级延迟、状态可证明、任何一天可逐字节回放。

## Proposal

每台机器一个热进程,内部是 LMAX 形态([01 §1–§4](../../../../design/01-总体架构.md)、[architecture.md §1](../../../../docs/architecture.md#1-一句话)):

1. 绑核忙等的 `feed-<venue>`/`gw-<venue>` 线程 → SPSC 环 → **唯一的 sequencer 线程**:赋全局 `seq`、写 seq journal、更新簿、调用策略、风控、OMS → SPSC 环 → gw 线程。
2. 业务状态(簿、仓位、订单槽、风控计数、策略状态)只有 sequencer 一个写者;跨线程只走 SPSC 环。
3. **决策可见 ⟺ 已记录**:行情、回报、命令、定时器、advice、配置内容都先成为 journal 事件;决策本身(`OrderDecision`/`SwapDecision`)也是事件。状态 = f(journal)。
4. 冷进程只读 journal 尾部;对热进程的唯一写入口是经 ctl-gateway 的控制环,命令与结果同样入 journal。
5. 同一 [`hft-sequencer`](../../../../crates/hft-sequencer/README.md) 代码在实盘、验证回放、假设回放三种驱动下运行。

证据:LMAX 6M orders/s 单线程 + journaler、Chronicle Queue "record everything"、hftbacktest 实盘拓扑([research/04 §5](../../../../design/research/04-oss-frameworks-and-patterns.md)、[research/06 §0、§4.1](../../../../design/research/06-data-plane-replay-observability.md))。

6. 逐字节验证回放是**离线**工具(夜跑与 CI);生产冷启动走 `RecoverDriver`,输出不一致只告警、不致命,对账通过且人工 `Resume{Recovery}` 后才可开仓([recover-driver](2026-10-01-recover-driver.md))。

## Alternatives considered

**多进程/微服务。** 跨进程一跳(iceoryx2 ≈ 100–250 ns)本身可忽略,但每多一个进程就多一份状态、一套时钟、一套故障模式;现有系统的对账问题正来自这里([01 §3.1](../../../../design/01-总体架构.md))。只保留两个例外:链节点(重 I/O、有 GC)与跨地域,它们以外部 venue 协议接入。

**以现成框架(nautilus_trader、hftbacktest live、barter)为热路径。** 它们绑定自身引擎与 tokio 运行时,sequencer 是本系统的核心资产,必须完全掌控([research/04 §1、§8](../../../../design/research/04-oss-frameworks-and-patterns.md))。

**多写者 + 锁。** 锁与缓存行争用进入尾部延迟;交错顺序不可记录,回放无法逐字节一致。

**只记录输入、不记录决策。** 可以重算,但无法对实盘意图流做逐字节比对,代码改动是否改变行为无从证明。

## Acceptance criteria

- `cargo xtask replay-verify` 对昨日生产 seq journal 每夜逐字节一致。
- 任一时刻从最近 `StateSnapshot` + 后续事件重建的状态哈希与运行中相等(DST 不变量)。
- mock 回环 `tick_to_trade` p50 ≤ 20 µs、p99 ≤ 60 µs([latency-budget](../../../../docs/latency-budget.md))。

## Risks

- **单热进程爆炸半径**:任一解码器 panic 即全停。对策:fuzz、`panic = "abort"` 下禁 panic、交易所死手 + kill-switch 进程、冷启动 < 30 s([13 §8](../../../../design/13-设计评审与优化.md))。
- **sequencer 成为吞吐瓶颈**:慢计算移出到 advisor([warm-compute-lane](2026-09-30-warm-compute-lane.md)),输入环有批量上限与优先级。
- **journal 写入进入热路径**:只写 RAM 段([hot-journal-in-ram](2026-09-30-hot-journal-in-ram.md))。
