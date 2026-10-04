# Agent Note: 策略输出两层:目标仓位由执行层跟踪,订单意图直通

Status: proposed

> **第八轮补充(2026-10-03)**:收窄第 5 条——引擎默认减仓(L3 与 `engine_flatten`)对 `Broken` 的 group 同样生效,先对永续腿 reduce-only 减仓;`Broken` 持续 `broken_auto_fix_ms` 且无人工 `HedgeFix` 时,执行层自动有界平掉多出的一腿一次;见 [broken-auto-fix](2026-10-03-broken-auto-fix.md)。

## Problem

组合携带是"目标 − 实际 → 切片调仓"的截面策略([评审 §1](../../../../design/_work/review-2026-10-02-主策略适配与执行正确性.md) 表 2):没有目标意图与执行层,每个策略都要自己写切片、两腿协调、对冲与补腿,雏形的缺陷(超时按原数量重下、两腿取 min 记账,评审 §6)会被一再照搬。同一条路径上还有一组执行正确性漏洞:开仓链接的对冲重试被生成为 `HedgeFix` 并被判 reduceOnly(B2)、按毛量对冲(B4)、对冲与 Flatten 无数量与名义上限(B6)、自动 HedgeFix 无次数与冷却(C4)、平仓腿顺序固定为"先现货"(C14)。OMS 里还写着 cdex 的补腿阈值与残余规则。

## Proposal

规格归 [`hft-exec`](../../../../crates/hft-exec/README.md);意图类型归 [`hft-strategy-sdk`](../../../../crates/hft-strategy-sdk/README.md)。

1. **两层输出**:策略可以输出 `TargetIntent`(按 (account, group) 的各腿目标净量、原因、紧急度、不交易带、`ExecSpec`),由执行层跟踪;也可以直接输出 `OrderIntent`。
2. **执行算法是引擎模块**:`TakerSlice`、`PairOnFill`、`Simultaneous`;`MakerFirst` 预留(optional)。目标在被替换或清除前一直有效,每个相关事件推进一次,O(腿数);每次决定写 Output 记录 `ExecDecision`,可逐字节回放。
3. **腿顺序**:`reason ∈ {MarginStress, Hard}` 强制 `PerpFirst`;`Rebalance/Exit` 开仓默认 `ByLiquidity`,可由参数覆盖。
4. **对冲正确性**:按 `FillDelta.net_qty` 对冲;对冲量 ≤ 触发成交净量 × ratio + 1 lot,名义 ≤ `max_hedge_notional`,超限 → `Broken` + P0、不发单;开仓链接的重试保留 `role = Second`(`SecondRetry` 只用于计数),`reduce_only` 继承计划;`HedgeFix` 只用于平掉多出的一腿,复用重试计次、冷却与截止,HARD 下抑制。
5. **引擎默认减仓**:MarginStress L3([margin-stress](2026-10-02-margin-stress.md))与策略在 `external_deadline_ms` 内未处理的 `ExternalFill` 由执行层按默认规则处理。
6. **OMS 只管机制**:订单槽状态机与 LegLink;补腿阈值、残余/尘埃规则、重试次数与截止全部是参数。

## Alternatives considered

**每个策略自己实现执行(雏形的做法)。** 重复代码、重复缺陷;评审 §6 的 P0 全部出在这一层。

**把执行并入 OMS。** OMS 会长出策略语义(何时切片、按什么顺序);执行层单独成 crate 才能独立做属性测试与 DST,OMS 保持"订单槽 + 链接"的最小职责。

**执行层放在独立线程或进程。** 多一跳延迟且破坏单写者;它在 sequencer 线程内同步执行,预算计入 `decide`。

**平仓腿顺序固定"先现货"(原 C 规格)。** 保证金压力下现货卖不掉时永续空头也平不掉(评审 §2.1);按原因选顺序。

## Acceptance criteria

- DST:目标在切片中途被替换,不超调、不留裸腿;解码单位错使成交放大 1000 倍时对冲被上限拦下(`Broken` + P0,零 `OutMsg`);开仓链接的重试从不带 reduceOnly;`reason = MarginStress` 的退出先减永续。
- 含 `ExecDecision` 的 journal `replayer verify` 逐字节一致。
- 执行层每个公开入口有预算与基准(见 hft-exec README),`decide` p99 预算不因执行层上调。

## Risks

- 执行层缺陷影响所有策略:共用一套 DST 不变量与场景;策略仍可退回直接下单。
- 引擎默认减仓可能与策略意图冲突:策略在截止前自行处理即可覆盖;默认动作只减不增。
- `MakerFirst` 的逆向选择未评估:预留,先在模拟账户上量化再启用。
