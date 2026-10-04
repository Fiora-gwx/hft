# Agent Note: 同一交易所账户同一时刻只由一个系统交易;迁移是切换,不并行

Status: proposed

> **第八轮(2026-10-02)**:部分取代——"迁移是切换"不再是首要交付(ccex 用新子账户上线、不接管旧仓位;cdex 迁移转 optional);规则加强为"一个账户只绑定一个策略实例";见 [one-account-per-strategy-instance](2026-10-02-one-account-per-strategy-instance.md)、[ccex-first-scope](2026-10-02-ccex-first-scope.md)。"同一交易所账户同一时刻只由一个系统交易"继续有效。

> **第八轮补充(2026-10-03)**:补充——统一升级后的 `sim` 实例不交易任何交易所账户,不违反本规则;见 [sim-only-instance](2026-10-03-sim-only-instance.md)。

> **第八轮第四次修订(2026-10-03)**:第 4 条与风险中的"小额实盘"不再适用(没有小额实盘阶段);迁移前的验证改为测试网联调、模拟账户与切 Live 前的零成交冒烟/Shadow;见 [no-small-live](2026-10-03-no-small-live.md)、[shadow-before-live](../process/2026-10-03-shadow-before-live.md)。

## Problem

两个下单系统同时在一个交易所账户上交易,会打穿几乎所有隔离:一方的账户级 `CancelAll`、账户级死手(Binance `countdownCancelAll`)与 kill-switch 会撤掉另一方的单;一方会把另一方的仓位当成自己的去平;reconciler 看到的是两系统之和,谁都对不上——"多进程各持仓位副本"([defensive-patterns](../../../../docs/defensive-patterns.md#状态只有一个写者不复制仓位))换了一种形式重现。第五轮曾为"新旧系统并行期"设计了独立子账户、ownership 表、`NOT_OWNED` 拒绝与 kill-switch 的"只撤本系统单"过滤;第七轮用户决定不再有并行期,这些机制随之失去存在理由。

## Proposal

第七轮裁决(2026-10-01 用户决定,主控记录)R7-1、R7-4;规则归 [defensive-patterns](../../../../docs/defensive-patterns.md#状态只有一个写者不复制仓位):

1. **同一交易所账户同一时刻只由一个系统交易。** 迁移是一次性切换:旧引擎停止开仓并收尾 → 停旧引擎 → hft-core 启动、对账、接管同一批账户;剩余仓位用 `AdoptExchange` 入账([runbooks/migration](../../../../docs/runbooks/migration.md))。回滚是反方向的切换(hft-core Drain → 重启旧引擎),同样不并行。
2. 没有 ownership 表、`NOT_OWNED`、`[[accounts]].exclusive`、`foreign_insts`;kill-switch 在本系统停摆时撤**该账户全部**挂单。
3. 没有第二个 hft-core 实例;`ClientOrderId` 的实例位是恒 0 的保留位。新版本验证 = 回放 verify + 同实例 Shadow 运行。
4. 小额实盘验证在迁移前进行:在同一账户上旧引擎暂停时,或用独立小额账户([testing §8](../../../../docs/testing.md#8-上线门槛))。

## Alternatives considered

**新旧并行交易期 + 按 SymbolGroup 分配开仓权(第三轮方案)。** 只约束了开仓,约束不了撤单、死手与平仓;并行期越长,共享账户的事故面越大。

**并行期 + 独立子账户 + ownership 表(第五轮方案)。** 能隔离,但要为一个临时时期实现并测试一整套机制(ownership 检查、过滤后的 kill-switch、分账对账、资金划转);用户决定不需要并行期,这些全部是一次性成本。

**灰度第二实例在同一账户上做真实交易。** 与共享账户同一类问题;新版本的风险由回放 verify 与 Shadow 运行覆盖。

## Acceptance criteria

- 迁移演练:旧引擎停止 → hft-core 启动 → 对账 → `AdoptExchange` 入账剩余仓位 → 解除 Recovery,全程同一时刻只有一个系统持有该账户的交易 key 与连接。
- kill-switch 在 hft-core 心跳停止时撤掉该账户全部挂单(混沌测试)。
- 仓库中不存在 ownership、`NOT_OWNED`、`exclusive` 相关的配置或代码路径(`doc-check` 与评审)。

## Risks

- 切换窗口内两边都不交易:窗口长度由迁移 runbook 控制(收尾与对账),作为迁移计划的一项度量。
- 回滚同样需要切换窗口;回滚前新系统必须 Drain 到无在途。
- 小额实盘若在同一账户上进行,旧引擎必须真正暂停(连接断开、无挂单),由迁移前检查单确认。
