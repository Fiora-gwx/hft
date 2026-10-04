# Agent Note: 深度按整条消息成批进入,BBO 走独立连接与高优先级环

Status: proposed

> **第八轮(2026-10-02)**:风险一节"迁移范围内只有 Binance 与 Gate"改为首批 core venue(Binance、OKX、Bitget);见 [ccex-first-scope](2026-10-02-ccex-first-scope.md)。

> **第八轮第四次修订(2026-10-03)**:补充——sequencer 对 md 逐条处理,每条之后先看 ctl、实盘 rep、bbo 环头;feed 按 BBO 与深度拆成两个任务,REST 快照移到 warm 任务 `snap-fetch`;见 [sequencer-md-preempt](2026-10-03-sequencer-md-preempt.md)、[feed-split-and-snap-fetch](2026-10-03-feed-split-and-snap-fetch.md)。

> **第八轮第五次修订(2026-10-03)**:收窄——core 行情不含全量增量深度,需要深度时用分档快照流 TopN,snap-fetch 只做 BBO 补帧;见 [core-market-data-scope](2026-10-03-core-market-data-scope.md)。

## Problem

早期事件模型把一条交易所深度消息拆成 N 条 `L2Delta`(末条置 `last`),sequencer 逐条应用。红队评审指出两个问题:

1. 策略可能在**半条消息**上被调用(看到一个中间态的簿),而且每条 `L2Delta` 各占一次环跳、一次 journal 追加与一次记录头,深度突发时开销按档数放大。
2. BBO 与深度共用连接与输入环时,深度突发(恰恰是 A 类触发的时刻)会把 BBO 排在一批深度帧之后——与 [dedicated-gateway-threads](2026-09-30-dedicated-gateway-threads.md) 解决的队头阻塞是同一类问题,只是发生在输入侧。

## Proposal

裁决见 第三轮裁决(2026-10-01 红队评审后,主控记录) B6、B7;布局归 [`hft-event`](../../../../crates/hft-event/README.md),通道与预算见 [latency-budget §2.3](../../../../docs/latency-budget.md#23-bbo-与深度的通道):

1. 新增变长 kind `L2Batch`:一条交易所消息一条记录(头 + N 个 `{px, qty, side}`);`L2Delta` 保留给需要逐档的读者,feed 默认产出 `L2Batch`;快照按 `L2SnapshotPart` 分片。
2. sequencer 一次应用整条 `L2Batch`,整批应用后才更新 mid、调用策略。
3. 每个 venue 的 BBO 流与深度流走不同连接;feed 写 `bbo` 环与 `md` 环;sequencer 优先级 `ctl > 回报 > bbo > advice > md`。
4. feed 每解完一条深度消息先 poll 一次 BBO 连接([hot-path-rules T13](../../../../docs/hot-path-rules.md#线程t))。

## Alternatives considered

**保留逐档 `L2Delta` + `last` 标记,策略只在 `last` 时调用。** 解决半条消息问题,但不解决按档数放大的环跳与 journal 开销;回放与 Parquet 映射也要多一层拼接。

**BBO 由深度簿导出,不单独订阅 bookTicker。** bookTicker 常早于 depth 数十 ms,且两者序号空间不同([research/06 §4.6](../../../../design/research/06-data-plane-replay-observability.md));用深度重建的 BBO 抓不到 bookTicker 能抓的机会。

**BBO 与深度同一连接、同一环,只加优先级标志。** 同一 socket 上的字节顺序无法重排,feed 解完前面的深度帧才能读到后面的 BBO。

## Acceptance criteria

- `tick-to-trade/l2burst`:深度 10× 突发时,`Bbo` 触发的 `tick_to_trade` p99 相对平静段上涨 < 10%,`Bbo` 的 `ingest` p99 ≤ 10 µs([benchmarks](../../../../benchmarks/README.md#路径基准与门槛))。
- 验证回放与 DST:策略从未在一条 `L2Batch` 应用到一半时被调用;sequencer 从未在 `bbo` 环非空时取 `md` 环。
- 金样:深度消息期望一条 `L2Batch`([fixtures](../../../../fixtures/README.md))。

## Risks

- 连接数翻倍(每所多一条行情连接):受交易所每 IP 连接限额约束,多 EIP 缓解;迁移范围内只有 Binance 与 Gate。
- 大批次(上千档)让 BBO 等待变长:快照分片 `snapshot_part_levels`(默认 64)约束最坏等待。
- 变长记录增加解码复杂度:`hft-event` 的变长编码与 fuzz 目标覆盖 `L2Batch`。
