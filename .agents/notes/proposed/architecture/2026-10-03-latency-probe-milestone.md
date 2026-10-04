# Agent Note: M1.5 延迟探针:在写热路径之前拿到 Graviton 上的第一份延迟证据

Status: proposed

> **第八轮第五次修订(2026-10-03)**:收窄第 1 条——M1.5 只测量、不再选定主机档位(增测项与 M3 末满订阅探针见 [m3-end-probe](2026-10-03-m3-end-probe.md));第 3 条的探针策略:`pair_period_ms` 默认 10 000,`quote` 模式产生 `rep_to_order` 样本,另有 `target` 模式;⑩ 的硬线、目标与证据见 [latency-acceptance-evidence](../process/2026-10-03-latency-acceptance-evidence.md)。依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E2、E3、E14(d)。

## Problem

延迟是首要目标,但第一份全链路延迟证据要到 M7 统一升级后才出现:分段预算取自 c7i.metal 估算与"忙轮询"假设,而实际选的是 Graviton 虚拟机;wire 级 mock 是可选项;参考机要等升级之后,此前没有性能回归门槛;门槛 ⑩ 要求的样本量(n < 1,000 不报 p99)对分钟级调仓的组合携带在测试网上凑不出来;HW RX 时间戳可用性(A7)两份调研说法相反([V5 评审](../../../../design/_work/review-2026-10-03-M0c-V5延迟与整体.md) P1-1、§3.1、§3.5;[V4 评审](../../../../design/_work/review-2026-10-03-M0c-V4运行与交付.md) P1-4)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-10;[round8d 简报](../../../../design/_work/round8d-spec-brief.md) D3)。路线图归 [11 实施路线图](../../../../design/11-实施路线图.md),参考机的用法归 [development 参考机](../../../../docs/development.md#参考机)。

1. **里程碑 M1.5**(M1 与 M2 之间):按需按小时租 `c8g.4xlarge`(与 Binance 同 AZ;开关机是负责人执行的 `[需授权]` AWS 操作;只放测试网 key),跑纵切片:hft-net + 一个 venue 的 BBO feed(主网公共行情)+ 简化 sequencer + gw(测试网下单)。产出 `kernel_rx`、`ingest`、`decide`(简化)、`egress` 直方图(HW/SW 时间戳分开);核实 c8g 的 HW RX 时间戳与 PTP、ENA 队列数与 IRQ 布局。结论在 M3 结束前用于重定 latency-budget §3 的分配与选定主机档位([host-class-c32](2026-10-03-host-class-c32.md))。
2. **参考机从 M1.5 起按需租用**:每个里程碑结束时在同型租用实例上跑 `bench-gate --reference`(相对回归 + 单 crate 上限)。
3. **延迟探针策略** `latency-probe`:只能绑定非正式账户(`StrategyDesc.non_prod_only`),在测试网上按 BBO 事件限速挂远离对手价的最小名义 PostOnly 再撤单(`tick_to_trade`、`egress`、`order_rtt` 样本),另有两所两腿模式产生 `hedge_host`、`leg_gap` 样本;规格归该策略 README。

## Alternatives considered

**等到 M7 再测。** 收包模型、线程拆分与主机档位都依赖这些数字;M7 才发现不达标时,hft-net、hft-venue 与 sequencer 主循环都要返工。

**只用 CI runner 或 dev2 测。** 共享 runner 与 c7g.large 上的数字不代表 c8g 与隔离核;dev2 还与旧系统共机。

**wire 级 mock 回环。** 测得到主机内路径,但测不到 ENA、IRQ 与真实交易所的形态,且 mock 是可选组件。

## Acceptance criteria

- M1.5 结束时有一份 Agent Note 级别的结论:各分段 p50/p99(HW 与 SW 分开,每组样本 ≥ 10,000)、HW RX 时间戳与 PTP 是否可用、ENA 队列数与 IRQ 布局、对 latency-budget §3 的调整建议。
- M3 结束前 latency-budget §3 按实测重定,主机档位选定并记录。
- 此后每个里程碑结束时有一份参考机 `bench-gate --reference` 结果。

## Risks

- 租用实例的费用与开关机需要负责人的时间;忘记关机会产生持续费用(ops-log 记录开关时刻)。
- 纵切片不含完整 sequencer 与策略,`decide` 只是近似;最终验收仍以门槛 ⑩ 为准。
- 测试网下单的回执路径与正式端点可能不同,`egress` 之后的交易所侧分段只记录。
