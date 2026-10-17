# Agent Note: 16 核接收尾部归因与测量边界

Status: implemented

## Problem

2026-10-16 的 md-spin-01/02 不是同一市场窗口的 A/B。md-spin-02 的 Binance BBO `kernel_to_user` p99 从 15.387 升至 69.985 µs，不能据此撤销 #69 的 Timer 分片与快照工作区复用。证据归档为 `run-2026-10-16-latency/latency/evidence/<session>/latency/{raw_points,event_chains,journal_events,order_chains}.csv`、`sched.jsonl`、`tail-review.json`、`tail-timers.json` 与 reports 下的报告；修复后运行 SHA 为 `b551976deee3827f14595c5aac37a67778afc5b4`。

逐事件按线程、连接、完成 read TSC 回连原始 Read，用同一 ClockStatus REALTIME 插值拆分首末读取 SW 跨度和最后读取间隔。单位 µs；以下仍按 BBO 事件计权，独立读取批次数单列。

| 运行/venue | BBO n | 原口径 p99 / p99.9 / max | 最后读取 p99 / p99.9 / max | >100 µs 独立批次 |
|---|---:|---|---|---:|
| md-spin-01 / Binance | 49136 | 15.387 / 265.483 / 10137.943 | 6.949 / 23.809 / 10137.943 | 27 |
| md-spin-02 / Binance | 42067 | 69.985 / 6309.999 / 16225.022 | 6.926 / 6309.999 / 16225.022 | 59 |
| md-spin-01 / Bitget | 17016 | 7.487 / 70.434 / 135.208 | 5.472 / 7.793 / 21.721 | 2 |
| md-spin-02 / Bitget | 11364 | 67.793 / 262.612 / 263.173 | 6.515 / 8.662 / 14.012 | 10 |

md-spin-02 的 Binance 59 个批次中 46 个在 conn=1、13 个在 conn=0。两段所有 >100 µs BBO 接收区间与记录的 sequencer Timer 回调均无重合。最大 16.225 ms 尾部所在约 1.011 s 采样区间，feed/sequencer 的切换、迁移、runqueue wait 增量均为 0；persist on-CPU 113.710 ms、CPU1 NET_RX softirq 340 次。9.863 ms 尾部内同 feed 线程还完成 conn=0 的 5 次读取、conn=1 的 3 次较早读取。这排除了“整个 feed 线程一直被 sequencer Timer 阻塞”的解释，不能排除 TCP 队头阻塞或未采集的内核干扰。秒级 sched 统计不足以定位毫秒级 IRQ/persist 因果；没有逐包 TCP 序号或 OFO 证据，不能把历史探针约 260 µs 的 OFO 直接套用到本次毫秒尾部。

Snapshot 回调 p50 从 16464.703 降至 27.154 µs，md-spin-02 Roll 为 37.409 µs；shape 的 cross06 Snapshot p50/max 28.369/29.992 µs、carry Roll 29.610 µs，支持保留快照修复。不同策略、品种、窗口和小样本订单不作严格性能回归裁决。

orders-spin-02 的 192 个 Bitget `rep_to_order` 都由 **Binance** 私有 OrderEvent 触发。原 `feed.frame` 从 TLS record 解密到每个帧交付，后续帧包含前面消息的解码、journal、出环处理。Binance 回报首帧 n=333、p50/p99 0.109/0.233 µs；复用同一解密记录的后续帧 n=262、p50 11.143 µs。Bitget 回报首帧 n=349、p50/p99 0.129/0.222 µs；后续帧 n=35、p50 7.066 µs。这些是边界诊断，不是新基准或目标主机修复后数字。

## Decision

保留原始首包至可处理时间与所有负值。ReadTiming 记录已有 recvmsg pre/post、字节范围、最近空确认；FrameTiming 记录当前成功 next_frame 的独立边界；TxWrite 保留真正 sendmsg 前戳。离线将 record 复用等待、最终解帧、独立读取批次与纯 send syscall 分开。旧 trace 继续可读，缺少的边界不猜测。契约归 [latency-report](../../../../tools/latency-report/README.md)。

## Alternatives considered

**撤销 #69 或扩大接收缓冲**。没有 Timer 重合，最后 read p99 持平；单次几百字节的毫秒尾部也远未填满缓冲。暂不做没有证据支持的轮询、缓存或交易修复时序变更。

**将 11 µs 全归于 WS 长消息复制**。未分片帧已经借用固定 rx 缓冲，服务端帧不得掩码；主要差值是同 record 前序消息处理。优化私有 JSON 可减少这段真实排队，但不能把测量重命名当成主机加速。

**逐次记录空 recvmsg**。Spin 会产生大量诊断流量；成功读取携带最近空确认提供轮询节拍证据，同时避免空轮询逐次写环。

## Consequences

无资金、风险、事件 schema 或回放语义变更。启用 trace 增加每次成功读取/交帧一条固定记录及计数器读；默认关闭。下一次同主机测量须交错旧/新二进制，记录 trace-on/off、TCP OFO/重传、CPU1 IRQ/softirq 和 persist 的细粒度活动；只用新边界判断线程轮询与系统调用成本。`sent_tsc` 仍是用户态返回，不宣称 decision→wire 或内核 TX floor。
