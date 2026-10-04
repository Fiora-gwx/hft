# Agent Note: 冷启动用 RecoverDriver,逐字节验证只做离线工具

Status: proposed

## Problem

"状态 = f(journal)"要求崩溃后能从 journal 恢复。早期规格让冷启动走验证回放:重放尾部,输出必须与日志逐字节一致,否则致命。红队评审指出三处不确定:崩溃点可能落在一个输入事件的输出写到一半;快照、滚动、采样这类依赖运行环境的决定在回放时会被重新判断,与实盘不同;新版本二进制读旧 journal 时输出本来就可能不同。把逐字节一致作为启动条件,会让一次部署或一个非确定性小缺陷变成"起不来"。

## Proposal

裁决见 第三轮裁决(2026-10-01 红队评审后,主控记录) B3、B4;实现归 [`hft-sequencer`](../../../../crates/hft-sequencer/README.md):

1. **事件边界**:每个输入事件及其全部输出写完后打 `EventEnd` 标记(优先用记录头 `flags` 位,不新增记录)。
2. **记录分类**:Input / Output / Meta。依赖运行环境的决定由 live driver 当场写成 Input(`Timer{Snapshot|Roll|Keyframe}`);Meta(`LatencySample`、`Note{Measure}`、`ClockStatus`、`HostProfile`)不进状态、不比对([hot-path-rules D5](../../../../docs/hot-path-rules.md#确定性d))。
3. **`RecoverDriver`**:截断到最后一个完整事件 → 从最近快照起照常应用输入 → 重新生成的输出与日志不一致**只告警、不致命**,新输出写入标 `Recovery` 的新段。
4. **版本变化**:从最近快照起步,**不重放尾部**;快照之后的订单全部经对账收敛。
5. 恢复不写模式格,只给每个账户加 `Recovery(account)` cap(第五轮 M6);闸门(含每个有 gw 的 venue 各一组 ≤ Warn 的对账)放行后,由 `Resume{Recovery, account}`(Trader + 确认)解除;只有干净 Drain 重启在闸门通过后自动解除,崩溃恢复无自动武装。
6. 逐字节验证回放只作为离线夜跑与 CI 工具(`cargo xtask replay-verify`)。

## Alternatives considered

**冷启动即验证回放,不一致即致命。** 见 Problem;把确定性缺陷的发现从夜跑搬到生产启动,代价是停机。

**不重放尾部,一律从快照起步。** 快照间隔(60 s)内的输入全部丢给对账收敛,即使版本未变、尾部完全可用;恢复更慢、`Unknown` 更多。

**每个输出后 fsync 作为提交点。** 违背热 journal 只写 RAM 段([hot-journal-in-ram](2026-09-30-hot-journal-in-ram.md));`EventEnd` 标记在 RAM 段内即可界定完整事件。

## Acceptance criteria

- `tests/recover/` 全部用例通过([testing §3.6](../../../../docs/testing.md#36-崩溃恢复与冷启动)):写到一半的事件、滚动中途崩溃、尾部在途订单、版本变化、不一致告警、对账不通过。
- 混沌:任意时刻 `kill -9` 后冷启动 < 30 s 到"对账中"状态,且交易所侧无重复订单(client id 相同)。
- 夜间验证回放对 Meta 差异注入不敏感。

## Risks

- 恢复时"不一致只告警"可能掩盖真正的非确定性缺陷:告警为 P1,当夜验证回放必须复现并定位。
- `EventEnd` 用 flags 位意味着读者必须理解它;journal 读者与 `hft-py` 同步更新。
- 版本变化跳过尾部会增加对账负担;发布前演练"版本变化冷启动"。
