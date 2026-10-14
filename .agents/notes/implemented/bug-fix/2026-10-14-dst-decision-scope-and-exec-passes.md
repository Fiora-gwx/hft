# Agent Note: 决策时刻的 Unknown 与执行续推轮次

Status: implemented

## Problem

core 种子 `0x61aaa3c81c37eb19` 在第 7110 步触发 I4。实际决策是 SecondRetry,观察适配器却把整个 group 的 Unknown 标记当成该链接的 Unknown。同组其他订单未收敛不等于当前链接不能偿还已确定的对冲量。

[CI 38522574515](https://github.com/Fiora-gwx/hft/actions/runs/38522574515) 的 EXE-024 种子 `5bf7fb15af4ed9f8d98cff7f381029ea91e3e5b8acf9eeb64cf7fea51ff1da7c` 则是真实执行状态问题:小容量下第一次输出回填把已访问 cell 再次置 pending,Continue 从头扫描后提前处理它,大容量运行没有这个额外推进。

## Decision

I4 使用决策时刻的 group 与 link 两个独立事实。链接事实直接检查 OMS 原始槽位,不复用引擎维护的链接 Unknown 计数。测试改变诊断副本验证本链接 Unknown、group Open 与输入内状态转换仍被正确检查。

执行层预分配当前轮和下一轮位图,当前轮剩余工作优先完成;回填只登记下一轮。强制位随各自工作集合保存,容量中断不提升强制位。快照节 21 写 v2 并保留 v1 读取,格式归 [hft-exec](../../../../crates/hft-exec/README.md#快照与恢复)。

## Alternatives considered

仅在补腿入口增加退避特判不能消除其他状态迁移的提前重访。一个扫描游标能跳过已访问 cell,但不能独立保存下一轮新增工作和两轮不同的 wake 门控,因此选择两个固定容量集合。

## Consequences

执行层的资金行为修复列入 [changes](../../../../docs/changes.md) 待评审。没有更改订单分类、风险放行规则或生产诊断开关;HashTrail 金样不重生成。

## Testing

EXE-024 保留 CI 种子与 13 步定向回归,比较普通、限容量与每次 Continue 前恢复三种运行。EXE-459 覆盖 v1 pending 恢复和 v2 非法强制位。TK-107 与 core 回归种子覆盖 I4 误报和诊断反例。
