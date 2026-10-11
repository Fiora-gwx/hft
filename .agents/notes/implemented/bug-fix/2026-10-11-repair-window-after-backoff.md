# Agent Note: 容量续推不提前启动补腿窗口

Status: implemented

## Problem

[CI 38140865614](https://github.com/Fiora-gwx/hft/actions/runs/38140865614) 的 EXE-024 属性测试找到同一组输入在两种输出容量下产生不同决定的反例。清仓单被 RATE 拒绝后,小容量的 Continue 会提早重新推进同一个 cell,启动补腿计时;大容量等待后续输入。之后小容量分支多出一次 FixExhausted。

反例及快照插入位置固定在 [EXE-516](../../../../crates/hft-exec/TESTS.md#exe-516-容量续推与恢复不在退避期提前启动补腿窗口),随机种子也保留在 proptest 默认读取的位置。

## Decision

补腿窗口的启动遵守 cell 已有的退避期限,具体契约归 [hft-exec 补腿](../../../../crates/hft-exec/README.md#补腿hedgefixc4)。在补腿入口判定,使普通回填、容量 Continue 和恢复后重新推进得到同一行为。已经开始的窗口继续计时,原先的超时保护保留。

## Alternatives considered

- 将回填后的 pending 改成非强制。容量停顿与快照恢复都会把待推进 cell 作为强制推进处理,仅改回填不能覆盖这些入口。
- 提前跳过全部处于退避中的补腿检查。这会让已经开始的补腿在持续门控期间越过原截止,因此只约束尚未开始的窗口。

## Consequences

没有增加热路径存储,也没有改变 journal 或快照格式。EXE-517 验证已开始的截止仍生效;SEQ-397 用真实引擎从拒单前、退避中的快照验证输出和状态。该行为属于执行风险控制,合并前仍需负责人评审,文件回放样本集的其余门槛不由这几个回归用例替代。
