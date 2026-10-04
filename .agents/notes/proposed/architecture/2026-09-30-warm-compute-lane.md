# Agent Note: 慢计算走 warm-compute 车道,以 Advice 事件进入 sequencer

Status: proposed

> **第八轮(2026-10-02)**:部分取代——`Advice{kind, …, v[6]}` 与全局 `AdviceKind` 登记改为按策略的不透明载荷;慢计算属于策略包的 `Model`,在 advisor 线程运行;见 [strategy-sdk-decoupling](2026-10-02-strategy-sdk-decoupling.md)。advisor 车道与"写进 journal 后才对策略可见"不变。

> **第八轮补充(2026-10-03)**:收窄第 2、4 条——输入环优先级为 `ctl > rep(实盘) > bbo > advice > md > rep-sim`;Advice 的新鲜度按其 `basis_tsc` 判定(规格归 hft-event 与 hft-strategy-sdk);见 [sim-only-instance](2026-10-03-sim-only-instance.md)。

## Problem

早期设计把所有策略计算都放在 sequencer 线程,包括每分钟的 ClosePolicy 分位数、LeadLag 互相关、20 档 VWAP 与冲击曲线([13 R10](../../../../design/13-设计评审与优化.md))。这些计算量大、周期性,会在 A 类触发的同一时刻占住唯一的状态写者,把 µs 级的决策拖到 ms 级。

## Proposal

[01 §3.3](../../../../design/01-总体架构.md)、[07 §2](../../../../design/07-策略层.md)、[architecture.md §3](../../../../docs/architecture.md#3-hft-core-线程):

1. warm 核上的 **advisor 线程**(1–2 个)尾随 journal,做慢计算,输出 `Advice{kind, group, valid_until_tsc, v[6]}` 事件到 advice 环。
2. sequencer 以优先级 `ctl > 回报 > bbo > advice > md` 取 advice 环,**把 `Advice` 写进 journal 后**才让策略看到——回放时从 journal 读 `Advice`,不重算,结果仍逐字节一致。
3. sequencer 里只留 O(1) 的比较与状态迁移。A 类用"预计算触发价":每次慢所报价变化时 advisor 算出快所需要到达的价位,tick 到来时 sequencer 只做一次整数比较。
4. `Advice` 过期(`valid_until_tsc` 早于当前事件)即视为不存在,策略退回保守行为。

规则见 [hot-path-rules T9](../../../../docs/hot-path-rules.md#线程t)。

## Alternatives considered

**在 sequencer 里增量计算。** 分位数与互相关的增量算法仍非 O(1),且策略代码复杂度上升;对 20 档 VWAP 这类需要遍历的计算无效。

**把慢计算放进冷进程(Python/独立进程),经控制环回来。** 控制环是运维命令通道,按命令鉴权与审计;高频的派生量不属于那里,且跨进程多一次调度延迟。

**advisor 直接写策略状态(共享内存)。** 破坏单写者;回放时无法重现 advisor 写入的时机。

## Acceptance criteria

- `seq-decide` 基准:`decide` p99 ≤ 15 µs,在 advisor 满负荷运行时同样成立。
- 验证回放不启动 advisor 也能逐字节一致(证明 `Advice` 全部来自 journal)。
- advisor 线程停止时,策略在 `valid_until_tsc` 后进入保守行为,DST 覆盖。

## Risks

- `Advice` 是"过去某刻"的计算结果,新鲜度有界(`valid_until_tsc`);策略必须把过期当作一等状态处理。
- advisor 与 sequencer 共享 L3;advisor 放 warm 核并限制其内存足迹(预分配工作集)。
