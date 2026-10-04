# Agent Note: 预留第二个 IRQ 核与第二块交易网卡;多 socket 行情线程允许 epoll_pwait(0)

Status: proposed

## Problem

收包改为中断驱动后,交易网卡 ENI-B 的全部队列都在一个 IRQ 核上处理。深度与标记价按固定节拍集中推送,BBO 与它们哈希到同一队列时排在全部批量包之后;IRQ 核还没有 `rcu_nocbs`,NVMe 的 managed IRQ 可能落到它上面。c16 上一个行情线程承载约 12 个 socket,每圈空读一遍,`kernel_rx` 预算放不下;BBO 路由每连接每次最多 64 帧,另一 venue 突发时 BBO 最多等约 96 µs;每次读到 EAGAIN 还要多一次空 `recvmsg`([W5 评审](../../../../design/_work/review-2026-10-03-M0d-W5延迟与整体.md) N-P1-1、N-P1-2、P2-1、P2-7)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-11;规范见 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E4)。核号归 [hft-core 线程计划](../../../../apps/hft-core/README.md#线程计划),清单与调优归 [deploy](../../../../deploy/README.md#主机清单),轮询规则归 [hot-path-rules](../../../../docs/hot-path-rules.md) T7 与 [hft-venue](../../../../crates/hft-venue/README.md),预算归 [latency-budget](../../../../docs/latency-budget.md)。

1. **预留,默认不启用**:c32 线程计划预留核 28 为第二 IRQ 核,deploy 清单预留 `[eni_b2]`。启用后延迟敏感流(BBO、下单、用户流)留在 ENI-B,批量流(深度、成交、标记价、REST)走 ENI-B2;ENI-B2 的 EIP 不进任何 key 白名单。启用条件:M3 末满订阅探针中 BBO 的 `nic_to_kernel` p99 在突发窗口显著上升(阈值由探针的 Agent Note 定,[m3-end-probe](2026-10-03-m3-end-probe.md))。
2. **IRQ 核卫生**:IRQ 核进 `rcu_nocbs`;hostprobe 核对 managed IRQ 的 `effective_affinity` 不含 IRQ 核、IRQ 核上没有可运行任务;XPS 按发送热核分配。
3. **轮询**:承载 ≥ 3 个 socket 的 `Spin` 行情线程每圈可调用一次 `epoll_pwait(timeout = 0)`,只对就绪的 socket 调 `recvmsg`;BBO 路由每连接每次至多 `feed.bbo_frames_per_poll`(默认 8)帧并在连接之间轮转;TCP 短读即停。`kernel_rx` 预算按主机类分写。
4. **c16** 只在 M3 末探针达标且启用 `epoll_pwait(0)` 时保留为候选;默认 c32。

## Alternatives considered

**现在就启用两块交易网卡。** 多一块 ENI、一个 EIP、一套 IRQ 计划与校验,在没有证据显示队头阻塞之前是纯成本;预留核与清单字段使启用只是换清单与生成物。

**启用可选的 RSS 源端口预计算把 BBO 固定到独立队列。** 同一个 IRQ 核仍串行处理全部队列的软中断,只能缓解队列内的排队。

**保持"每圈对每个 socket 空读"并删掉 c16。** c32 上同样多 4 个 socket 的空读成本;`epoll_pwait(0)` 一次约 0.5 µs,低于多次空 `recvmsg`。

## Acceptance criteria

- hostprobe 的 `required` 含 `rcu_nocbs` 覆盖 IRQ 核、managed IRQ 落点、IRQ 核无任务与 XPS 映射;改坏任一项 → hft-core 退出 3。
- 基准:另一 venue 突发时 BBO 在行情线程上的等待 p99 ≤ `bbo_frames_per_poll` 帧的处理时间;`--syscall-audit` 中 `epoll_pwait` 只出现在承载 ≥ 3 socket 的行情线程。
- 启用 ENI-B2 的决定(或不启用)与依据写进 M3 末探针的 Agent Note。

## Risks

- `epoll_pwait` 是新增的热线程系统调用,白名单与审计要同步;收益与成本以参考机实测为准。
- ENI-B2 启用后每个 venue 的连接分到两个出口 IP,交易所按 IP 计的限额与连接数需要逐所核对。
