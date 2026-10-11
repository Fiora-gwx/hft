# Agent Note: 指定快照恢复记录实际跳过的范围

Status: implemented

## Problem

RPL-025 的完整时段验收需要核对恢复选择的快照与旧尾部。恢复入口原先只写 RecoverFromSnapshot 滚段原因,没有写 journal-format 规定的 Boot 注记。另外,文件重组后只保留首片 seq,把最后一个逻辑记录的 seq 当成提交末尾会漏掉它的后续物理片。

## Decision

读文件时保留跨片记录的末片 seq,按截去半事件后的完整边界计算审计范围。内存 journal 每条逻辑记录只有一个 seq,无需模拟文件分片。审计信息只在恢复启动阶段使用,不增加持久状态。具体字段和空区间语义归 [sequencer 冷启动恢复](../../../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)。

## Alternatives considered

- 用下一条 Input 定义范围。快照后的 Output、Meta 和其他分片也属于被丢弃的尾部,这样会漏记。
- 用读取到的最后一个逻辑 seq。遇到多片快照会把末片之前的位置误报成提交边界。
- 只延长旧回放测试的时间而不检查审计信息。这样不能覆盖原验收对恢复来源的要求。

## Consequences

审计 Note 属于 Meta,不参与输出哈希,不改变 journal schema 或快照布局。FROM_SNAPSHOT 与重放尾部互相矛盾的配置明确失败。新段仍从自己的恢复快照独立验证,不要求保留旧尾部才能验证新运行;这与需要旧日志证明的 RecoverMismatch 接受范围不同。

## Acceptance criteria

RPL-025 使用真实文件写者、20 min 的旧运行、第 10 min 快照以及恢复后的 20 min;篡改旧尾部 BBO 时保留原记录 CRC 策略并重算 seal。RPL-026 使用真实 Drain 与不同 build_git,再推进 10 min。SEQ-398 覆盖半事件、空尾部、矛盾配置及恢复后回放;SEQ-385 逐条核对文件重组后的物理末片位置。执行结果以本提交的 GitHub 托管 CI 为准,本地只做静态检查。
