# Agent Note: 首次上线的 core 行情不含全量增量深度;需要深度时优先分档快照流

Status: proposed

## Problem

首个上线策略不订阅 L2,但规格把全量增量深度与 REST 快照重同步列为 core:它带来序号链、重同步状态机、snap-fetch 上的深度快照、深度突发对 BBO 的软中断与轮询干扰,以及内存表里的深度环与 raw 段([W5 评审](../../../../design/_work/review-2026-10-03-M0d-W5延迟与整体.md) §6-6)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-11;规范见 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E5)。路由与任务归 [hft-venue](../../../../crates/hft-venue/README.md),事件归 [hft-event](../../../../crates/hft-event/README.md),内存表归 [hft-core](../../../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)。

1. **core 行情** = BBO、逐笔成交、标记价/指数价、资金费,以及 BBO 补帧(REST)。全量增量深度与 REST 快照重同步改为**可选扩展**(触发条件:某个策略需要全深度);规格保留但标为可选。
2. 需要深度时优先用**分档快照流**(Binance `depth{5,10,20}@100ms`、OKX `books5`、Bitget `books5/books15`,各所待核实):路由 `RouteKind::DepthTopN`,产出 `L2Snapshot{origin = TopN}`,簿按"整替换"应用,无序号链、无 REST 重同步。
3. snap-fetch 只保留 BBO 补帧(增量深度启用时才承担深度快照);`feed-md:` 任务承载成交/标记价/资金费/TopN;执行层切片的可见深度有 TopN 订阅时读簿,否则用 BBO 顶档。

## Alternatives considered

**保留全量增量深度为 core。** 首个策略用不到,却要先实现并验证最复杂的一条行情路径,且它是 BBO 尾部的主要干扰源。

**完全不支持深度。** 执行层切片与第二个策略可能需要前几档深度;TopN 快照流以很小代价提供它,且没有序号链。

## Acceptance criteria

- 首次上线版本的 venue 适配与金样只覆盖 core 行情与 TopN 路由;增量深度相关的规格节标为可选扩展并写明触发条件。
- 内存表、环容量与线程计划按 core 行情范围计算(深度只计 TopN)。
- TopN 簿的"整替换"应用在 DST 与金样中覆盖(无序号链时不触发重同步)。

## Risks

- 各所分档快照流的推送频率与档数待核实;档数不够的所只能等可选扩展。
- 分档快照的体量随档数增长,仍会在固定节拍集中推送;对 BBO 的干扰由 M3 末探针一并测量([m3-end-probe](2026-10-03-m3-end-probe.md))。
