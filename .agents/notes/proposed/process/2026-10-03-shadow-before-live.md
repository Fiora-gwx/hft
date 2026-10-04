# Agent Note: 切 Live 之前做零成交冒烟与正式实例 Shadow 3–7 天

Status: proposed

> **第八轮第五次修订(2026-10-03)**:收窄第 1、4 条与 Risks 第 1 条——零成交冒烟改用 Admin 专用的 `SmokeOrder` 命令(OMS 的槽,死手覆盖该 inst,不触发 `Recon`),网页手工挂单作废,见 [smoke-order-command](2026-10-03-smoke-order-command.md);D-shadow 期间"决策不变的修复"只重计 3 天(规则归 testing §8)。依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E8、E9。

## Problem

不设小额实盘之后([no-small-live](../architecture/2026-10-03-no-small-live.md)),门槛只补偿了软件正确性:⑥ 只在测试网,⑦ 的模拟未经校准,⑩ 只测主机内分段,⑪ 的 kill 演练也在测试网。正式端点上的撤单、交易所死手、真实 `MarginReport`、资金费流水与扣费资产,在切 Live 之前一次都没用过;风险上限"保守起步"只是建议([V4 评审](../../../../design/_work/review-2026-10-03-M0c-V4运行与交付.md) P1-1、§4-3/4)。

## Proposal

用户决定按推荐执行,且这两步不产生仓位、不算小额实盘([round8-decisions](../../../../design/_work/round8-decisions.md) R8-10;[round8d 简报](../../../../design/_work/round8d-spec-brief.md) D5)。步骤归 [launch D](../../../../docs/runbooks/launch.md#d-正式实盘m8),比对工具归 [replayer](../../../../apps/replayer/README.md)。

1. **D-smoke 零成交冒烟**:每个(正式账户, venue)挂一张远离市价的最小名义 PostOnly 单再撤;验证交易所死手的刷新与到期、kill-switch 对正式端点撤单、reconciler 连续两轮 `Ignore`、每把 key 的 UID 与 `ACCOUNT_CFG`。
2. **D-shadow**:正式 `prod` 实例全部策略实例 `RunMode = Shadow` 运行 3–7 天(决策照算、订单不发),打通真实保证金、资金费流水、用户流、对账与告警;与同参数的 sim 实例用 `replayer compare` 逐决策比对,差异逐条解释。
3. 初始 `RiskParams` 上限与"第一次放宽"的条件写成上线签字记录里的**必填数值**(不是门槛)。
4. 正式账户的每月 kill 演练只做同样的零成交撤单冒烟(维护窗口、CloseOnly),不对正式实例 SIGSTOP([incident-response kill 演练](../../../../docs/runbooks/incident-response.md#kill-演练))。

## Alternatives considered

**恢复小额实盘阶段。** 用户已否决(护栏与实盘可用性互相牵制、最小名义放大 lot 残余)。

**只做 Shadow、不做冒烟。** Shadow 不发单,覆盖不到正式端点的撤单与死手。

**在正式账户上做完整 kill 演练(含 SIGSTOP)。** 冻结有仓位时的对冲与保证金管理;完整演练留在测试网账户上。

## Acceptance criteria

- 每个(正式账户, venue)的 D-smoke 记录:订单号、死手撤销时刻(有死手的 venue)、kill-switch 核验为 0、两轮 `Ignore`。
- D-shadow ≥ 3 天:无 P0、P1 都有解释、reconciler 全部 `Ignore`、资金费流水与账本一致、`replayer compare` 的差异全部有原因。
- 上线签字记录的初始上限与放宽条件均为具体数值。

## Risks

- 零成交冒烟的挂单在规格里没有经 hft-core 下单的通用入口(延迟探针策略只能绑定非正式账户),目前用交易所网页挂单,会被 reconciler 报为外来挂单;需要时以单独 Note 增加演练下单入口。
- Shadow 期间行情与资金费照常,但没有成交:成交路径(第二腿、扣费、用户流成交推送)仍要到切 Live 当天才首次走正式端点。
- Shadow 推迟正式实盘 3–7 天。
