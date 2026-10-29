# Agent Note: 不做回测评估,做模拟账户(实时行情纸面交易)

Status: proposed

> **重构修订(2026-10-29)**:收窄第 6 条与"历史回测 / 假设回放 / 参数扫描"的否决——回放(`replayer research`/`parity`)是研究与参数筛选工具,输出带校准表哈希、对账误差与随机种子方差;第 7 条门槛 ⑦ 与模拟账户的其余规则不变;见 [replay-as-research-tool](2026-10-29-replay-as-research-tool.md)。

> **第八轮补充(2026-10-03)**:收窄第 1 条——模拟账户的回报环 `rep-sim` 优先级最低、每轮处理量 ≤ `sim_max_per_round`、同一事件先派发实盘;统一升级后模拟只在只含模拟账户、不持 key 的 `instance = "sim"` 实例运行(同一二进制;本 Note 否决的"独立的模拟进程"指另一套代码);见 [sim-only-instance](2026-10-03-sim-only-instance.md)。

> **第八轮第三次修订(2026-10-03)**:收窄第 5 条——没有小额实盘阶段:同参数模拟实例在 sim 实例上与**正式实盘**并行校准;见 [no-small-live](2026-10-03-no-small-live.md)、[sim-colocated-with-prod](2026-10-03-sim-colocated-with-prod.md)。

## Problem

用户判断历史回测一定不准,不建回测系统用于评估策略,但要有办法在不花钱的情况下观察策略表现并改进([round8-decisions](../../../../design/_work/round8-decisions.md) R8-5、R8-6 第 1 条)。修订前的规格只有 Shadow(只记录意图、不模拟成交):目标跟踪策略在 Shadow 下实际仓位不变,每个 tick 发同一意图,无法评估([评审 §1](../../../../design/_work/review-2026-10-02-主策略适配与执行正确性.md) 表 11);`hft-sim` 是 hftbacktest 队列模型的回测内核;上线门槛要求"Shadow ≥ 14 天 + 与旧引擎信号离线对照",而 ccex 策略没有旧引擎可对照。

## Proposal

规格归 [`hft-sim`](../../../../crates/hft-sim/README.md);装配归 [`hft-core`](../../../../apps/hft-core/README.md);重演归 [`replayer`](../../../../apps/replayer/README.md)。

1. **模拟是账户类型,不是 RunMode**:`AccountKind::Sim(SimAccountCfg)` 绑定某个真实 venue 的行情;策略在模拟账户上以正常 `RunMode` 运行,代码与实盘完全相同。sequencer 把该账户的 `OutMsg` 路由到 sim 出环,由 sim-gw 任务(warm 线程,不占热核)撮合;回报走同一回报环、记 journal(Input),回放仍确定。模拟账户不持有 key,reconciler 与 kill-switch 跳过它们。
2. **撮合**:吃单在"下单时刻 + 按实测分布抽样的延迟"对实时 L2/BBO 逐档成交;已吃流动性在 `consume_decay_ms` 内从可见量中扣除;部分成交;按 refdata 与 `VenueCaps` 拒单;手续费与扣费资产;资金费按真实费率在结算时刻结算;永续全仓保证金与强平;现货余额;跨所划转用 `SimTransfer`。
3. **一套撮合器**:`hft-testkit` 的最小撮合器就是它;hftbacktest 队列模型降为可选 feature。
4. **并行与报告**:可同时开多个模拟账户跑不同参数;PnL 分解、成交率、两腿间隔、裸敞口、保证金占用由 journal-archiver/watchtower 产出、ops_bot 标注"模拟"展示。
5. **校准**:小额实盘开始后,同参数模拟账户与实盘并行,量化滑点、成交率、延迟差异并回写模拟参数。
6. **重演只排障**:`replayer sim` 把录下的 journal 行情喂给模拟网关 + 同一策略,用于复现与排障,不用于评估收益;`replayer verify` 不变。
7. **上线门槛**:"模拟账户在实时行情上连续运行 ≥ 14 天"取代"Shadow ≥ 14 天 + 离线对照"([testing §8](../../../../docs/testing.md#8-上线门槛) ⑦)。

## Alternatives considered

**历史回测 / 假设回放 / 参数扫描。** 用户判断不准:我们自己的成交不在历史里,冲击、排队、资金费与基差的路径依赖都只能假设;雏形的夜间网格寻优在同一段数据上选参又打分(评审 §6)。降为 optional。

**Shadow(只记录意图)。** 没有成交就没有仓位与 PnL,目标跟踪策略无法评估。

**只用交易所测试网。** 测试网的流动性与价格不真实,且不是每家都有(待核实);保留为门槛 ⑥ 的冒烟。

**独立的模拟进程。** 两套状态、两套代码路径;在同一进程内用同一策略代码与同一回报路径,才能保证"模拟 = 实盘减去交易所"。

**只做小额实盘。** 花钱,且小额验证不了 $500 级切片与分配(评审 §1)。

## Acceptance criteria

- 模拟撮合:资金守恒;结算时刻资金费 = 持仓 × mark × 真实费率;拒单规则与交易所金样一致。
- 含模拟账户的 journal `replayer verify` 逐字节一致。
- 开启与关闭模拟账户时,`c8` 上 sequencer `decide` p99 差 ≤ 5%(sim-gw 不在热线程)。
- 小额实盘期间产出模拟 vs 实盘的滑点、成交率、延迟差异报告,并完成至少一次参数回写。

## Risks

- 模拟失真(我们不改变真实盘口):已吃流动性记忆与实盘校准缓解;报告始终标注"模拟"。
- 模拟账户占用 sequencer 预算:数量受 EngineConfig 容量约束。
- 有意放弃:历史"如果当时"分析;排障重演不出收益结论。
