# Agent Note: 一个交易所(子)账户只绑定一个策略实例

Status: proposed

> **重构修订(2026-10-29)**:放宽第 1 条——一个账户仍只被一个实例交易,但一个实例可交易多个账户(账户集合,重构 M4 落地);第 3–6 条保留;见 [account-set-per-instance](2026-10-29-account-set-per-instance.md)。

> **第八轮补充(2026-10-03)**:补充——一个账户在每个交易所只用一个市场(现货或永续),refdata 以 `SameExchangeMarkets` 拒绝;见 [no-same-exchange-spot-perp](2026-10-03-no-same-exchange-spot-perp.md)。

> **第八轮第四次修订(2026-10-03)**:第 5 条"原测试账户的角色由模拟账户与小额实盘承担"改为由测试网账户与模拟账户承担(没有小额实盘阶段);见 [no-small-live](2026-10-03-no-small-live.md)。

> **第八轮第六次修订(2026-10-03)**:补充——读不到 UID 的测试网账户可用 `uid_policy = "unverifiable"`(只限 `is_prod = false`,Admin 签字,`IDENTITY` 由 kill 自检与一次签名只读请求放行);见 [testnet-uid-unverifiable](../process/2026-10-03-testnet-uid-unverifiable.md)。

## Problem

修订前的规格让多个策略(C、D、E、A)共用同一对测试/正式账户,只用"一个 (account, inst) 至多被一个非 Shadow cell 引用"(`InstOwned`)来防冲突([accounts-first-class](2026-09-30-accounts-first-class.md))。评审 §2.5 指出([review](../../../../design/_work/review-2026-10-02-主策略适配与执行正确性.md)):组合携带的币池与 cdex 名单有重叠,单向持仓下同一币只能有一个净仓;资金费与盈亏无法按策略拆分;两个策略共享保证金,而分配器的"总资金"不知道另一个策略占了多少。另外 HARD 只能全局,测试账户失控只能连正式账户一起封(C8);API key 与 `AccountId` 之间没有交易所身份核验,key 配反就会串单(C9)。

## Proposal

字段归 [`hft-refdata`](../../../../crates/hft-refdata/README.md) 的 `AccountMeta`;命令归 [control-plane](../../../../docs/subsystems/control-plane.md);闸门与 cap 归 [`hft-risk`](../../../../crates/hft-risk/README.md)。

1. **绑定**:`AccountMeta.bound_instance: StrategyInstanceId`——一个账户(及其在各 venue 的子账户)只给一个策略实例使用;`SetParams` 校验账户已绑定到该实例,否则 `InvalidArgs{AccountBound}`。ccex 与 cdex 用不同的子账户([round8-decisions](../../../../design/_work/round8-decisions.md) R8-4)。
2. **新子账户上线**:ccex 策略用新开的子账户直接上线,不接管旧仓位([runbooks/launch](../../../../docs/runbooks/launch.md))。
3. **身份核验**:`AccountMeta.venue_uid` 登记交易所账户 UID;启动闸门 `IDENTITY` 按 key 查 UID 并比对;逐账户自检 kill 位。
4. **爆炸半径按账户**:kill 与 HARD 可按账户(`Kill{Account(a), hard}`);DailyLoss、MarginStress、HedgeFailure 等 cap 都按账户成立。一个实例出问题,只封它自己的账户。
5. **模拟账户同样是账户**:`AccountKind::Sim` 也只绑定一个实例;"不同参数"= 不同模拟账户([sim-accounts-not-backtest](2026-10-02-sim-accounts-not-backtest.md))。原"测试账户"的角色由模拟账户与小额实盘承担。
6. "同一交易所账户同一时刻只由一个系统交易"([one-exchange-account-one-system](2026-10-01-one-exchange-account-one-system.md))继续成立,是本规则的特例。

## Alternatives considered

**多个策略共用账户 + `InstOwned`。** 单向持仓下重叠币无法共存;保证金与资金费不可拆分;一个策略的风控事件(保证金、HARD)牵连另一个(评审 §2.5)。

**一个分配器统一管理账户内多个策略。** 把策略耦合到同一个分配器里,违背策略与引擎解耦([strategy-sdk-decoupling](2026-10-02-strategy-sdk-decoupling.md))。

**每个账户一个 hft-core 进程。** 连接与核数成倍增加,状态多份;单写者单进程的理由不变([accounts-first-class](2026-09-30-accounts-first-class.md))。

**HARD 仍只允许全局。** 单进程多账户时,一个账户的问题会停掉全部账户;按账户 HARD 让爆炸半径与账户一致。

## Acceptance criteria

- 向已绑定到其他实例的账户发 `SetParams` → `InvalidArgs{AccountBound}`。
- 用 UID 不符的 key 启动 → 该账户 `IDENTITY` 位为 0,零订单离开 gw。
- DST:对账户 a 发 `Kill{Account(a), hard}`,其他账户的订单流、对冲与平仓不受影响。
- reconciler 与 kill-switch 的目标表按账户列出,模拟账户不在其中。

## Risks

- 子账户与 key 数量增加:Secrets、IP 白名单、开通核验写进 launch runbook;各所子账户数量上限待核实。
- 资金分散在多个账户、多个所:划转更频繁,由 treasury 给建议、人工执行。
- 有意放弃:跨策略共享保证金带来的资金效率。
