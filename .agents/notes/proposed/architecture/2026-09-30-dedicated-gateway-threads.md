# Agent Note: 下单连接独占 gw 线程,行情在 feed 线程

Status: proposed

## Problem

早期设计里一个 venue 线程同时处理该所的行情与下单([13 R3](../../../../design/13-设计评审与优化.md),原 01 §3.2)。这会队头阻塞:Gate 行情突发时 venue-gate 线程正在解密/解析一批帧,出环里的 Gate 订单要等这一批处理完。A 类策略的触发恰恰发生在行情突发的时刻,于是 p99 被"对端所的行情量"决定,而不是被我们的代码决定。

## Proposal

[01 §3.2–§3.3](../../../../design/01-总体架构.md)、[architecture.md §3](../../../../docs/architecture.md#3-hft-core-线程):

1. 每个会下单的 venue 一个 `gw-<venue>` 线程:只轮询该 venue 的出环与下单连接(主 + 热备)的回报,循环极短;拥有帧模板、发送侧限速计数、交易所字符串 id → slot 的私有表。
2. 行情在 `feed-<venue>` 线程:行情主/备连接、raw journal 写入。
3. 只看行情不下单的 venue 只有 feed 线程。
4. 类型上分开:`GwRuntime` 只接受 `OrderConn`,`FeedRuntime` 只接受 `FeedConn`([`hft-venue`](../../../../crates/hft-venue/README.md))。

核预算:6 所 × (feed + gw) + sequencer + advisor + 余量 ≈ 16 个隔离核;c7i.metal-24xl 48 核充裕([01 §3.2](../../../../design/01-总体架构.md))。

5. 同一 venue 的 BBO 流与深度流用不同连接;feed 分别写 `bbo` 环(高优先级)与 `md` 环([l2-batch-and-bbo-lane](2026-10-01-l2-batch-and-bbo-lane.md))。
6. feed/gw 循环每 poll 完一个 socket 就检查一次出环,standby/REST/UserStream 每 N 圈才 poll 一次([hot-path-rules T13](../../../../docs/hot-path-rules.md#线程t));基准 `gw-egress/idle8`。

## Alternatives considered

**一个 venue 一个线程,内部优先轮询下单连接。** 优先级只在轮询间隙生效;一旦开始解密/解析一批行情帧(Binance 合约全量 bookTicker 峰值数千条/秒,每条 2–5 µs),订单仍要等这批完成。批量上限能缩小但不能消除等待。

**所有 venue 的下单共用一个 gw 线程。** 一个所的慢回报或重连会拖住其他所的发送;且该线程要持有多所的 TLS 会话,缓存足迹变大。

**sequencer 直接写 socket。** sequencer 会承担 TLS 加密与系统调用,违反"sequencer 无 I/O",并把网络抖动引入唯一的状态写者。

## Acceptance criteria

- mock 回环在对端所行情突发(10× 正常速率)下,`egress` p99 与无突发时相比上涨 < 10%。
- `tick_to_trade` p99 ≤ 60 µs 在行情峰值时段同样成立(按时段分组的生产直方图)。

## Risks

- 线程与核数翻倍:核够用;若将来 venue 增多到核不足,只看行情的 venue 可以合并 feed 线程,下单线程不合并。
- 行情与回报走不同线程后,同一所的行情与回报到达 sequencer 的相对顺序由各自环决定;sequencer 的输入优先级(回报 > 行情)使这一点确定且被记录。
