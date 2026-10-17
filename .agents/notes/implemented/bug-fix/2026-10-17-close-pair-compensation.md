# Agent Note: 平仓同发的补偿方向

Status: implemented

## Problem

平仓一腿已成交、另一腿零成交时,反向已平腿会重开仓。round4 中该 Reverse 被 REDUCE_INVALID 连续拒绝,真实剩余敞口仍需修复。

## Decision

按负责人默认决定,平仓 Simultaneous 的失败策略完成少成交的平仓腿。沿用 SecondRetry 和既有补偿阶段位,用当前健康报价产生受越价上限约束的 IOC,经过相同减仓检查、限速和次数/时间上限。Unknown 先收敛,失败交 Broken。开仓 Reverse 保持原义。

## Alternatives considered

放宽 reduce-only 会允许平仓失败后主动重开风险。无限重试会占用回报与定时器预算。另加策略分支会把引擎补偿机制泄漏到策略,且无法覆盖恢复后的链接。

## Consequences

补第一腿时更新第一腿累计净量,补第二腿时更新第二腿累计净量。原单和补偿单必须有终态证据后才能再次补偿。验证覆盖双向非对称、部分成交、限速、次数耗尽、两次恢复与真实引擎逐字节回放。资金语义记入 [changes](../../../../docs/changes.md),状态待评审、负责人默认接受。
