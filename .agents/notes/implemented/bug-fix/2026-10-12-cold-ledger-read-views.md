# Agent Note: 冷账本还原完整读视图

Status: implemented — PR #2 内待 CI 与资金、恢复评审

## Problem

[LED-267](../../../../crates/hft-ledger/TESTS.md#led-267-dst-中每个快照点-journalledger-等于引擎-ledger) 接入完整 VenueBook 后,[CI 201](https://github.com/Fiora-gwx/hft/actions/runs/38248143933) 在恢复边界发现估值更新时间不同,基础币费用场景还发现 BALANCE_UNRECONCILED 不同。该处钱包金额相等,不能据此跳过完整视图检查。

## Decision

冷消费者用自身已入账事实保留一个最新检查点副本,清空其进程内缓存,从下一条 Input 开始独立消费后缀。Recovery 只采用该副本重算的缓存生命周期;主账本的仓位、钱包及订单去重水位不被后续快照替换。实现契约归 [ledger 冷消费者](../../../../crates/hft-ledger/README.md#冷消费者journalledger)。

热账本实际执行余额校验时,通过现有 Note 载体发布 BalanceCheck。该业务 Output 只证明 quiet 闸门通过,并关联同事件原始报告,不提供账本金额或校验结论。冷读者据自己的钱包计算漂移。Note 码按追加规则登记,不增加事件字段或快照行;完整回放校验热引擎是否应当产出它。

## Alternatives considered

**在 Recovery 处全部清零。** [定向 CI](https://github.com/Fiora-gwx/hft/actions/runs/38249970693) 通过了空后缀单测,但真实恢复会重放检查点之后的 Input,部分 venue 已有非零更新时间。统一清零不能描述这一过程。

**冷读者一直以 quiet=false 消费。** 这能保留金额,但丢失 Drift 的置位和清除,与完整读视图验收冲突。余额报告自身没有 OMS 的在途状态。

**在 ledger 再实现一套 OMS。** 生命周期、撤改单、重启和槽回收都需跟随,且账本依赖 OMS 违反组件边界。采用已有业务输出的事实模式,与冷读者使用 OrderDecision 确定订单归属一致。

**复制热账本标志或放宽比较。** 会把参考结果当成期望来源。反例要求同一校验事实在不同冷钱包上得出不同结果,钱包逐单位一致的契约不变。

## Consequences

冷读者多保留一份有界状态副本。BalanceCheck 会增加业务 Output,HashTrail 如有变化必须在托管 CI 中审查来源,不修改既有种子、步数或哈希范围。生产引擎的余额金额计算和 quiet 判定保持原行为;此记录服务账本恢复,不属于 testkit 诊断缓冲。LED-288/289、SEQ-414 与完整 DST/回放共同验证,通过前不计入 M2 验收。

## Validation

[专项 CI](https://github.com/Fiora-gwx/hft/actions/runs/38253273817) 验证提交 `a193404` 的 193 项账本测试、4 项结算回放测试、93 项 testkit 测试及 50 个固定种子均通过。生成的新金样与关闭检查器的完整摘要 50/50 相同。另在临时构建中只移除 `on_balance` 新增的 BalanceCheck 输出,同一 50 个种子完整复现旧摘要,未改序号、状态字段或哈希范围;两轮共记录 400/0 条该输出。正式实现保留全部记录与检查器。

据此更新固定集的摘要和生成的 Note 登记文件,种子、套件、步数均不变。专项制品 ZIP 的 SHA-256 为 `e7f546020c8e51cbce11c237b33880ad5140970dc47e66b5e2b4148f7f6840c6`;完整 PR CI 和负责人评审另行执行。
