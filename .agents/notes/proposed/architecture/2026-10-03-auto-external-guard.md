# Agent Note: AUTO_EXTERNAL 只在用户流稳定时成立;合成成交不写记录;ledger 以 auto_held 抵扣补推

Status: proposed

## Problem

reconciler 的 `AUTO_EXTERNAL` 置位条件不排除带 `USERSTREAM_DOWN` 的轮次,sequencer 复核也不看:用户流仍断着时就合成一笔 `ExternalFill{Unknown}`,交易所随后补推真实的强平/ADL,两笔键不同、都会入账,策略与执行层的默认处理会按"被减了两次"卖出现货,形成真实裸空永续。合成成交要不要另写一条记录,sequencer、reconciler、defensive-patterns 与 testing 四处说法不一;冷读者(reconciler 的引擎视图、hft-py)只经 hft-ledger 消费 journal 事件,看不到 sequencer 内合成的那笔([X1 评审](../../../../design/_work/review-2026-10-03-M0e-X1资金正确性.md) P1-C;[X3 评审](../../../../design/_work/review-2026-10-03-M0e-X3一致性.md) P0-3)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-12;规范见 [round8f 简报](../../../../design/_work/round8f-spec-brief.md) F1(c))。置位条件归 [reconciler](../../../../apps/reconciler/README.md),合成与复核归 [hft-sequencer](../../../../crates/hft-sequencer/README.md),合成函数与 `auto_held` 归 [hft-ledger](../../../../crates/hft-ledger/README.md)。

1. `AUTO_EXTERNAL` 另要求:两轮对账都没有 `USERSTREAM_DOWN`,且该 (account, venue) 用户流自最近一次 Up 起已稳定 ≥ 1 个对账周期;reconciler 置位与 sequencer 复核都查。
2. 合成成交在 `ReconcileReport` 这条 Input 内确定地合成,不另写记录;合成逻辑是 hft-ledger 的一个函数,sequencer 与冷读者都经 hft-ledger 的 journal 消费路径调用它,冷读者因此看得到这笔成交。
3. hft-ledger 对 `AUTO_RECON` 入账量按 (account, inst, 方向) 记 `auto_held`(进快照);之后同 inst、同方向的 `ExternalFill{Adl | Liquidation | Unknown}` 先抵扣,只入账超出部分,并写 `Note{LedgerAutoHeldOffset}`(P0)。
4. 待核实:三所私有流断开期间的强平/ADL 在重连后是否补推(方法:测试网断流演练)。

## Alternatives considered

**合成成交另写一条 Input 记录。** 回放时这条记录与 `ReconcileReport` 内的合成会重复应用;不写记录、在同一 Input 内确定地重算,回放与冷读者都只有一条路径。

**用户流断开期间一律不自动入账,全部交人工 `AdoptExchange`。** 夜间无人时 ADL 造成的单边仓位长期不被引擎认识;稳定一个周期后再入账,配合 `auto_held` 抵扣,既不双计也不长期悬空。

**按 trade_id 去重。** 合成成交没有交易所成交号,去重键对不上;按方向与量抵扣不依赖成交号。

## Acceptance criteria

- DST:用户流断开期间同号缩小 → 不置位;恢复并稳定一个周期后第二轮仍缩小 → 合成一次;随后补推同方向 ADL → 只入账超出部分并写 `LedgerAutoHeldOffset`,账本与交易所仓位一致。
- 回放:含 `AUTO_EXTERNAL` 的 journal `replayer verify` 逐字节一致;hft-py 读出的仓位与 sequencer 相同。

## Risks

- 交易所补推的成交方向或量与合成值不同(部分成交、多笔合并)时,抵扣后仍有差额,由下一轮对账报出并人工处理。
- 稳定周期把自动入账推迟至少一个对账周期,期间账户按对账差异禁开。
