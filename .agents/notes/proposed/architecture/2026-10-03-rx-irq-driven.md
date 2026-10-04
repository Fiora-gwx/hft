# Agent Note: 收包只用中断驱动:busy poll 全关、交易网卡 IRQ 独占一核、sim 用独立网卡

Status: proposed

> **第八轮第五次修订(2026-10-03)**:收窄第 2、3 条——IRQ 核进 `rcu_nocbs`,hostprobe 另核对 managed IRQ 不落 IRQ 核、IRQ 核无任务;XPS 按发送热核分配;sim 连接不落 ENI-B 队列的校验由 hft-core 在首帧后自检(hostprobe 读不到别的进程的 socket);c32 预留第二 IRQ 核与 ENI-B2,见 [irq-split-reserve](2026-10-03-irq-split-reserve.md)。依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E4。

## Problem

规格里同时存在两种收包模型:T12 与 hft-net 写 IRQ/NAPI 在 hk 核、热线程只做非阻塞 `recvmsg`、`SO_BUSY_POLL` 默认不设;deploy 却全局设了 `busy_poll/busy_read = 25`、`napi_defer_hard_irqs = 2`、`gro_flush_timeout = 200000`。按内核行为,新 socket 的 busy-poll 时长继承 `busy_read`,热线程每次空读都会轮询整个 NAPI 队列,替同一 ENI-B 上的 sim 处理行情包;某个队列一时没人轮询时,包最多被推迟 200 µs。`kernel_rx` ≤ 4 µs 的预算又取自"忙轮询"那一列。核 0 同时承担全部 IRQ、housekeeping、persist 与系统服务([V5 评审](../../../../design/_work/review-2026-10-03-M0c-V5延迟与整体.md) P0-1、P1-3;[V4 评审](../../../../design/_work/review-2026-10-03-M0c-V4运行与交付.md) P1-5、P2-5)。

## Proposal

规范解法见 [round8d 简报](../../../../design/_work/round8d-spec-brief.md) L-P0-1。调优归 [deploy](../../../../deploy/README.md#运行期调优),线程与核号归 [hft-core 线程计划](../../../../apps/hft-core/README.md#线程计划),socket 选项归 [hft-net](../../../../crates/hft-net/README.md),预算归 [latency-budget](../../../../docs/latency-budget.md)。

1. `net.core.busy_poll = 0`、`net.core.busy_read = 0`;ENI-B 的 `napi_defer_hard_irqs = 0`、`gro_flush_timeout = 0`;保留 `adaptive-rx off rx-usecs 0`。conn-builder 对每个 socket 显式 `SO_BUSY_POLL = 0`。
2. ENI-B 全部队列 IRQ → IRQ 专用核(升级主机类核 1,不放线程);ENI-A → 核 0;`irqaffinity` = 核 0 与核 1。热线程只做非阻塞 `recvmsg`。
3. sim 用独立的 ENI-C(IRQ 钉在 sim 的一个核上,出口 = ENI-C 的私有 IP + EIP);conn-builder 在交接前经 `SO_INCOMING_NAPI_ID` 与 netdev netlink 自检 sim 连接不落在 ENI-B(hostprobe 不做连接级检查,规则归 [hft-core 主机资源隔离](../../../../apps/hft-core/README.md#主机资源隔离slice核与出口-ip))。
4. 上述参数与 IRQ 落点是 hostprobe 的 hard 项,不符 → hft-core 退出 3。
5. `kernel_rx` 预算按中断驱动重定(p50 ≤ 8 µs、p99 ≤ 20 µs,待 M1.5 实测);M7 用 sim 开/关对比 softnet 计数实证。dev2 只有一块网卡,不适用。

## Alternatives considered

**全面忙轮询(每个热线程 busy-poll 自己的队列)。** 需要连接按队列精确落到线程核(RSS 源端口预计算或 ntuple,ENA 在后挂网卡上的支持待核实),热线程还会替同队列的其他 socket 做协议栈处理;中断驱动的尾延迟更可预期,规格的其余部分本来就按它写。

**IRQ 留在 hk 核。** housekeeping 的段预创建(`MAP_POPULATE` 50–100 ms)、persist 与系统服务会让软中断转给 ksoftirqd,带来毫秒级尾部。

**sim 共用 ENI-B、只换一个私有 IP。** 网卡队列与中断仍共享,sim 的全宇宙行情会在 prod 的 IRQ 核上做软中断。

## Acceptance criteria

- 升级主机上 hostprobe 的四项参数与 IRQ 落点全部相符;改动任一项 → hft-core 拒绝启动(测试)。
- M7:sim 开/关两种状态下 IRQ 专用核的 `NET_RX` softirq 计数与 prod 的 `kernel_rx` p99 差异在 testing ⑩ 的同机干扰判据内。
- `kernel_rx` 7 天直方图满足 latency-budget §3 重定后的预算。

## Risks

- 中断驱动的 `kernel_rx` 中位数高于忙轮询;预算已按此上调,M1.5 实测不够时再评估忙轮询(须新 Note)。
- 单个 IRQ 核承载 ENI-B 全部队列的软中断,行情突发时可能成为瓶颈;按 M1.5 的队列数与软中断耗时实测决定队列数。
- 多一块 ENI 与 EIP 的费用与 IP 配额。
