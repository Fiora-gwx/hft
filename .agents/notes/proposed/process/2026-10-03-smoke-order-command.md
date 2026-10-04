# Agent Note: 正式端点冒烟改用 Admin 专用的 SmokeOrder 命令

Status: proposed

> **第八轮第六次修订(2026-10-03)**:事实同步——Risks 第 2 条的待核实已定:`Drain{cancel_resting = false}` 时 SMOKE 槽保留到进程退出,不按 `ttl_ms` 撤销(规则归 [hft-oms 冒烟单](../../../../crates/hft-oms/README.md#冒烟单smokeorder))。

> **第八轮第七次修订(M0-g,2026-10-04)**:收窄第 2 条——删除"且在交易所价格带内":OMS 不检查交易所价格保护带(refdata 没有这项),被交易所拒单时如实回报;规则归 [hft-oms 冒烟单](../../../../crates/hft-oms/README.md#冒烟单smokeorder)。依据 [round8g 简报](../../../../design/_work/round8g-tests-brief.md) §4。

## Problem

切 Live 前的零成交冒烟 D-smoke 原本由人在交易所网页挂单:OMS 的槽镜像里没有这张单,实例又处于 Shadow,交易所死手不会对该 inst 设置倒计时,"停掉 hft-core 后死手撤单"这一步测不到;网页挂单还被 reconciler 报为外来挂单,每月的正式账户冒烟都会触发 `Recon` cap 与 P0([W4 评审](../../../../design/_work/review-2026-10-03-M0d-W4运行与交付.md) P1-9、§5-4;[shadow-before-live](2026-10-03-shadow-before-live.md) 的 Risks 第 1 条)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-11;规范见 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E8)。命令与结果码归 [control-plane](../../../../docs/subsystems/control-plane.md),槽与自动撤单归 [hft-oms](../../../../crates/hft-oms/README.md),标志位归 [hft-event](../../../../crates/hft-event/README.md),操作步骤归 [launch D-smoke](../../../../docs/runbooks/launch.md#d-正式实盘m8) 与 [incident-response kill 演练](../../../../docs/runbooks/incident-response.md#kill-演练),ops_bot 入口 `/smoke` 归 [ops_bot](../../../../python/ops_bot/README.md#命令表)。

1. 新控制命令 `SmokeOrder{account, venue, inst, side, ttl_ms}`:只允许 Admin(运维 CLI,或 ops_bot 私聊中的 Admin),需确认。
2. OMS 以 `origin = Command`、`OrderFlags::SMOKE` 建槽:只能 PostOnly,价格离对手价 ≥ `smoke_far_bp`(默认 300 bp),数量 = 最小名义 × 1.1 向上取整到 lot;`ttl_ms` 默认 10 000、上限 60 000,到期自动撤单;每 (account, venue) 同时至多一张。
3. Shadow/CloseOnly 下允许,HARD 下拒绝;走完整风控链(不经模式门);不计入任何策略、不改目标。槽在 OMS 里,死手刷新因此覆盖该 inst,reconciler 也不把它当外来挂单。
4. launch D-smoke 与每月正式账户冒烟都用它;删除"网页手工挂单"与"Recon 降 P1"。

## Alternatives considered

**保留网页挂单,签字接受"正式端点死手未验证"。** 死手是 hft-core 宕机时的第一道撤单,切 Live 前不验证就把它留到第一次事故。

**用延迟探针策略在正式账户上挂单。** 探针策略只能绑定非正式账户(`non_prod_only`),放开它等于引入一个能在正式账户下单的测试策略。

**冒烟期间把 `Recon` 降为 P1。** 只是压告警,死手仍测不到。

## Acceptance criteria

- 每个(正式账户, venue)的 D-smoke 记录:冒烟单订单号、OMS 自动撤单时刻、死手撤单时刻与原因(有死手的 venue)、kill-switch 核验为 0、reconciler 两轮 `Ignore`,冒烟期间没有 `Recon` cap。
- 权限测试:Trader 与群聊发 `SmokeOrder` 被拒;HARD 下被拒;同一 (account, venue) 第二张被拒。

## Risks

- 冒烟单是真实挂单:`smoke_far_bp` 或最小名义的取值错了可能成交;交易所价格保护带不接受该距离时被交易所拒单(OMS 不检查交易所价格带,如实回报);价格离对手价 ≥ 300 bp 且 PostOnly,成交的概率很低,成交时按外来仓位处理并写 postmortem。
- 验证死手需要在 `ttl_ms` 到期前让 hft-core 不撤单地退出;Drain 期间 SMOKE 槽是否保留待核实(方法:测试网账户上先演练一次)。
