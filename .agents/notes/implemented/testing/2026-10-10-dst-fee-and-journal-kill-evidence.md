# Agent Note: 故障必须实际触发,验证结果才有意义

Status: implemented

## Problem

DST 的 LateFeeOnly 只排入队列,没有消费方,测试能在未产生晚到手续费的情况下通过。JNL-072 的随机事件上界超过测试段容量,子进程提前 panic 后父进程仍接受 EOF,恢复出来的短前缀也可能通过。

## Decision

MockVenue 从真实撮合费用中拆出一次延迟回报,不另改撮合真值。INV-2 对 FeeOnly 读取独立的 ledger 前后观察量,同时保留 OMS 组件对 FeeOut 条数与归属的断言。负例覆盖漏费、重复扣费、缺告警与错误 epoch 匹配。与 ReportDup 组合只复制成交回报,无幂等键的增量 FeeOnly 不复制;这一限制写入组件规格。

JNL-072 在开始事件前按未发布游标预留整事件上界,父进程核验杀点事件数和 SIGKILL 退出信号。每个种子的目录随其工作线程释放,避免完整执行后积累所有 journal。

## Alternatives considered

**把 FeeOnly 当作无状态变化而直接放行。** 这会漏掉未入账的费用。DST 检查实际资金变化,组件测试检查内部输出,两者互补。

**让验证包启用 sequencer 的 testing trace。** testing 只从 dev-dependencies 启用,正式验证包不能靠绕开依赖规则获得观察能力。本次使用已有的 ledger 公开视图。

**只增大 journal 段。** 未发布写游标仍可能越过安全阀。容量检查必须发生在事件前,并给滚动尾部留空间。

## Consequences

故障开始真实投递后,受影响种子的 HashTrail 会变化;只接受 CI 生成、逐行核对种子与步数不变的基线更新。测试用例与限制分别归 [hft-testkit](../../../../crates/hft-testkit/TESTS.md) 和 [hft-journal](../../../../crates/hft-journal/TESTS.md)。生产事件格式、下单规则与延迟验收门槛不变。完整随机崩溃测试比提前退出更耗时,执行证据以 CI 为准。

真实引擎的世界配置尚未传递基础币费用规则,INV-8 的费用补差观察也缺失。该路径独立登记为 TK-106 待实现,不把仅修改 WorldSpec 却未改变实际费用资产的运行算作覆盖。
