# Agent Note: 恢复快照持久化停止点的引擎时间

Status: implemented

## Problem

CI 38149555201 的 RPL-025/026 在完整时段恢复后发现引导快照的 engine_ns 为 0。恢复入口已经把停止点保存到 last_ens,但快照生成取的是当前事件上下文;不重放尾部时尚无新事件,这个上下文没有初始化。进程再次在新 EngineClock 之前崩溃时,只能从错误快照取到 0。

## Decision

Recovery 快照头取已提交停止点的引擎时间,与下一条 EngineClock 的 base_engine_ns 同源。普通事件内快照继续取该事件时间。节 25 保留旧运行的时钟基准,仍由记录下来的 EngineClock 切换,不在快照生成中提前切换时钟。契约归 [引擎时间](../../../../crates/hft-sequencer/README.md#引擎时间engine_ns)。

## Alternatives considered

- 放宽长时测试的快照时间断言。这样会漏掉连续崩溃后时间归零的恢复缺陷。
- 先合成新运行的完整 Now。它会改变恢复阶段的模式回调和 TSC 换算,超出持久化停止点所需的修改,且没有对应的已记录新 Input。

## Consequences

修正现有快照字段的值,不改布局、schema 或正常事件处理。恢复相关变更需要负责人评审。

## Acceptance criteria

SEQ-108 验证有尾部、无尾部和指定旧快照三种恢复,随后在 EngineClock 之前再崩溃一次,核对新频率下时间连续与逐字节回放。RPL-025/026 使用真实文件和正式 CLI 验证完整时段。GitHub 托管 CI 执行完整 DST 与 HashTrail,本地仅静态检查。
