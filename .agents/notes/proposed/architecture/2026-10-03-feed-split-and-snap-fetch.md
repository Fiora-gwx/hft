# Agent Note: feed 线程按 BBO 与深度拆开,REST 快照移到 warm 任务

Status: proposed

> **第八轮第五次修订(2026-10-03)**:收窄第 2 条——全量增量深度与 REST 深度快照改为可选扩展,snap-fetch 在 core 中只承担 BBO 补帧;需要深度时用分档快照流(`RouteKind::DepthTopN`),见 [core-market-data-scope](2026-10-03-core-market-data-scope.md)。BBO 路由每连接每次至多 `feed.bbo_frames_per_poll` 帧、连接间轮转,多 socket 行情线程可 `epoll_pwait(0)`,见 [irq-split-reserve](2026-10-03-irq-split-reserve.md)。依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E4、E5。

## Problem

同一个 `FeedRuntime` 按"Bbo 路由 → 其他路由"的固定顺序轮询,每条连接每次最多 64 帧;REST 快照的请求与解析(40–80 µs,T7 上限 100 µs)在 feed 线程上做,期间其他 inst 的帧全部延后;升级主机上每个 feed 线程承载两个产品线。突发时一条 BBO 最多等 64 × 5 µs + 100 µs 快照 + 解密,单个 runtime 约 0.4 ms;即使空闲,约 10 个 socket 的空 `recvmsg` 一圈就吃掉 `kernel_rx` 的 p50 预算。latency-budget 引用的"每条深度后先 poll BBO"在 T13 与 hft-venue 里都没有([V5 评审](../../../../design/_work/review-2026-10-03-M0c-V5延迟与整体.md) P0-3)。

## Proposal

规范解法见 [round8d 简报](../../../../design/_work/round8d-spec-brief.md) L-P0-3。任务与环归 [hft-venue](../../../../crates/hft-venue/README.md),线程计划归 [hft-core](../../../../apps/hft-core/README.md#线程计划),规则 T7/T13 归 [hot-path-rules](../../../../docs/hot-path-rules.md)。

1. 同一 venue 拆成两个任务:`feed-bbo:<venue>`(只含 BBO 路由)与 `feed-md:<venue>`(深度/成交/标记价路由),都是 `FeedRuntime<P>`,配置 `routes = Bbo | Md | All`;升级主机上 `feed-bbo:*` 不得与 `feed-md:*` 同线程(加载校验退出 2 `BboSharesWithMd`),BBO 线程可以跨 venue 合并;dev2/c4 可配 `All`,此时每处理完一条非 Bbo 帧即 poll 一次全部 BBO 连接。
2. REST 快照移到 warm 任务 `snap-fetch`(`SnapFetchTask<P>`,每实例一个):拥有快照用的 REST 连接,负责请求、读取、解密、解析;经 `snapreq-<venue>` 与 `snap-<venue>` 两个环与 `feed-md` 交互,`feed-md` 收到后写 `RawRx` 并驱动重同步状态机。T7 中 feed 线程的 REST 快照系统调用与"单次解析 ≤ 100 µs"条款删除,改为 snap-fetch 的 warm 预算。

## Alternatives considered

**保持单个 feed 任务,只在每条深度帧后 poll BBO。** 仍然要在 feed 线程上做快照解析,且每条深度帧的处理本身就在 BBO 前面;只能缓解,不能给出"BBO 最多等一条帧"的界。

**每个 venue 一个线程(BBO 与深度同线程)。** 线程多但队头阻塞仍在;BBO 与深度分开后,BBO 线程可以跨 venue 合并,核数更省。

**快照解析留在 feed 线程、分片解析。** 解析可以分片,REST 的读与解密仍在热线程上,且分片状态让重同步状态机更复杂。

## Acceptance criteria

- 基准:深度突发与快照进行中时,BBO 从 `kernel_rx` 到进入 bbo 环的等待 p99 不随深度负载上升(与无深度负载对照,差 ≤ 一条帧的处理时间)。
- 加载校验:升级主机上 `feed-bbo:*` 与 `feed-md:*` 同线程 → 退出 2 `BboSharesWithMd`。
- 重同步演练:快照在 snap-fetch 上完成,feed 线程上没有 REST 系统调用(`--syscall-audit`)。

## Risks

- 每个 venue 多一个线程与两个环;核数由主机档位吸收([host-class-c32](2026-10-03-host-class-c32.md))。
- 快照响应从 warm 线程经环回传,重同步延迟多一次环跳;对 L2 的影响在毫秒级以下,可接受。
- BBO 与深度来自不同连接时,两者的相对顺序只由各自的接收时间戳与 sequencer 的取环顺序决定,回放照 journal 顺序确定。
