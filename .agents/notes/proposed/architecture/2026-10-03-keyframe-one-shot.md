# Agent Note: 关键帧在空闲轮一次写完,删除按节分片与 journal 写者的 mark/rollback

Status: proposed

> **第八轮第六次修订(2026-10-03)**:收窄第 1、2 条——关键帧另加事件数触发 `keyframe_max_events`(1,500,000)与最大推迟(到期后最多推迟 2 个 `keyframe_period_ms`,之后在下一个事件边界强制写);实盘写关键帧不计算 `state_hash`(头部该字段写 0,由 replayer 与读者侧计算);Risks 两条因此都有上界。规则归 [hft-sequencer](../../../../crates/hft-sequencer/README.md) 与 [journal-format](../../../../docs/subsystems/journal-format.md);依据 [round8f 简报](../../../../design/_work/round8f-spec-brief.md) F3(b)。

## Problem

上一轮把关键帧 `StateSnapshot{purpose=Keyframe}` 拆成按节分片,有输入到达就作废重来(hft-journal `JournalWriter::mark()/rollback()`)。持续有输入时分片几乎从不完成,5 s 后仍要一次写完;5/60 的时间里新到事件多等半片,`ingest` p99 约 +13 µs。分片还让每个状态 crate 多出 `snapshot_section`、让 journal 写者多出可回滚的状态([W5 评审](../../../../design/_work/review-2026-10-03-M0d-W5延迟与整体.md) N-P0-1)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-11;规范见 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E1)。主循环与参数归 [hft-sequencer](../../../../crates/hft-sequencer/README.md),写者 API 归 [hft-journal](../../../../crates/hft-journal/README.md),预算归 [latency-budget §3.1](../../../../docs/latency-budget.md)。

1. `Timer{Sys, Snapshot}` 到期后的**第一个空闲轮**一次写完关键帧,预算 ≤ 300 µs,计入 p99.9 项。
2. 周期 `keyframe_period_ms` 默认 300 000(ECF1 段 5);每个 Live 簿的 `L2Snapshot{Keyframe}` 均摊到同一周期。
3. 删除 `JournalWriter::mark()/rollback()`、`WriteMark`/`AppendMark`、段 5 的 `snapshot_slice_bytes`/`snapshot_max_defer_ms`、各 crate 的 `snapshot_section`、基准 `seq/snapshot_slice` 与测试 `keyframe_slice`;各组件只保留 `snapshot(&self, w)` 一次写本 crate 的全部节(hft-book 为 `BookSet::snapshot`,`Top.rx_tsc` 置 0)。
4. 可选扩展:advisor 镜像在旁路生成关键帧;触发条件:实测关键帧停顿超出 p99.9 记录目标。

## Alternatives considered

**保留分片,只把"有输入即作废"改成"有输入即暂停"。** 每片之间状态继续变化,拼起来的快照不对应任何一个时刻;要么记录增量补丁,要么回到作废重来。

**旁路(advisor 镜像)生成关键帧作为默认。** 去掉主循环停顿,但镜像与 sequencer 的状态一致性要单独证明,实现量大;留作可选扩展。

**缩短周期、每次只写一部分 crate。** 恢复时要拼多个周期的节,恢复规则变复杂,且总停顿不变。

## Acceptance criteria

- 参考机基准:关键帧一次写出 p99 ≤ 300 µs(c32 prod 规模的状态);`ingest` p99 在有无关键帧的分钟之间差 ≤ 1 µs(按事件加权统计)。
- 回放:同一 journal 在新写法下 `replayer verify` 逐字节一致;从任一关键帧起步的恢复与从 Drain 快照起步的结果相同。
- 全仓没有 `mark`/`rollback`/`snapshot_section`/`snapshot_slice` 残留(`doc-check`)。

## Risks

- 空闲轮长时间不出现(持续突发)时关键帧被推迟;周期是上界的近似,推迟次数与时长进 `Note`,超出记录目标即启用可选扩展。
- 300 s 周期使崩溃恢复的尾部更长(最多 5 min 的重放);恢复预算按此复核(hft-core 启动步 5)。
