# Agent Note: 账户是一等维度,测试与正式是同一进程的两个账户

Status: proposed

> **第八轮(2026-10-02)**:部分取代——"测试与正式是同一策略的两个账户"改为"一个账户只绑定一个策略实例",测试的角色由模拟账户与小额实盘承担;第 3 条"同实例 Shadow 运行"改为模拟账户;按账户 HARD 可用。见 [one-account-per-strategy-instance](2026-10-02-one-account-per-strategy-instance.md)、[sim-accounts-not-backtest](2026-10-02-sim-accounts-not-backtest.md)。账户是一等维度、单实例的决定不变。

> **第八轮补充(2026-10-03)**:补充——统一升级后主机上另有一个只含模拟账户、不持交易所 key 的 `sim` 实例;交易实盘账户的仍是单一 `prod` 实例;见 [sim-only-instance](2026-10-03-sim-only-instance.md)。

> **第八轮第四次修订(2026-10-03)**:收窄第八轮状态行——"测试的角色由模拟账户与小额实盘承担"中的小额实盘已不存在:测试由测试网账户与模拟账户承担,切 Live 前另有零成交冒烟与正式实例 Shadow;见 [no-small-live](2026-10-03-no-small-live.md)、[shadow-before-live](../process/2026-10-03-shadow-before-live.md)。

## Problem

现有 cdex 用两个进程实例区分测试与正式账户(`~/Trading/cdex-arb` 与 `~/cdex-prod`):两份代码、两套连接、补丁后要跑 `sync-code.sh` 同步,代码漂移与"改了测试忘了正式"反复发生;Telegram 侧靠 `/account` 切换和消息前缀区分([research/00 §2、§6.2](../../../../design/research/00-existing-codebase-inventory.md))。新栈如果照搬"一账户一进程",连接数与核数翻倍,且多进程各持仓位的老问题回来([defensive-patterns](../../../../docs/defensive-patterns.md#状态只有一个写者不复制仓位))。

## Proposal

[13 O3](../../../../design/13-设计评审与优化.md)、[09 §5.3](../../../../design/09-可观测性、风控与控制面.md):

1. `AccountId(u8)` 定义在 [`hft-types`](../../../../crates/hft-types/README.md),进入 OMS 订单槽、风控限额与 kill、ledger 仓位、事件体(`OrderDecision.account`、`OrderEvent.account`、`TxLifecycle.account`;行情 `MarketHdr` 不带账户,行情按 venue 共享)。
2. 测试与正式是同一 hft-core 里的两个 `AccountId`:各自 API key、各自下单连接、各自 `RiskParams` 与 kill 开关;行情连接共享。
3. **单实例**:没有第二个 hft-core 实例;新版本验证 = 回放 verify + 同实例 Shadow 运行([build-test-migrate](../process/2026-10-01-build-test-migrate.md))。`ClientOrderId` 的实例位是恒 0 的保留位,不承载任何逻辑。
4. ops_bot 的 `/account` 切换的是默认视图;`[🧪测试]/[💰正式]` 前缀保留;命令显式带账户。

5. **一个账户一个系统**:同一交易所账户同一时刻只由一个系统交易;迁移是切换,不并行——旧引擎停止开仓并收尾后,hft-core 接管同一批账户([one-exchange-account-one-system](2026-10-01-one-exchange-account-one-system.md))。
6. **命令的账户语义**(第五轮 M4):降级类命令不带账户参数时作用于全部账户;升级与中性命令必须显式写账户名,缺省即拒;`CmdHdr.account == CmdScope.account`;确认预览显示目标账户与另一账户的当前有效模式,令牌绑定 `ctl_state_version`。ops_bot 的视图账户只决定"看什么",不决定命令落在哪个账户。日亏熔断、对账锁存、`Recovery` 都按账户成立。

## Alternatives considered

**一账户一进程(现状)。** 连接与核数翻倍;两份状态;两份代码漂移;对账复杂。

**账户作为配置 profile,运行时二选一。** 无法同时运行测试与正式;测试账户验证新策略时正式账户必须停。

**账户在策略层区分(策略实例绑定账户),OMS/风控不感知。** 风控限额与 kill 必须按账户生效,否则测试账户的异常会触发正式账户的全局熔断,反之亦然。

## Acceptance criteria

- DST:测试账户触发 venue 级 kill 时,正式账户的订单流不受影响;反之亦然。
- 风控属性测试:每账户每资产的敞口限额独立生效。
- ops_bot 每条推送带账户标签;每个交易类命令在 journal 里带 `AccountId`。

## Risks

- 共享进程意味着共享爆炸半径:一个账户的解码 panic 会停掉两个账户;由 fuzz、禁 panic 与死手覆盖([single-writer-sequencer-and-journal](2026-09-28-single-writer-sequencer-and-journal.md))。
- 老板习惯的"双实例"运维方式改变,需要老板拍板([13 §7](../../../../design/13-设计评审与优化.md));迁移是一次性切换,没有两套并行交易的时期。
