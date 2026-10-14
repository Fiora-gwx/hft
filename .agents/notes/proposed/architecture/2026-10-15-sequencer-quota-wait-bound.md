# Agent Note: 每轮额度耗尽时的高优先级等待上界

Status: proposed

## Problem

[SEQ-063](../../../../crates/hft-sequencer/TESTS.md#seq-063-让位取出的输入计入同一轮的上限) 要求 ctl 每轮取满 4 条后,本轮仍处理剩余 BBO;[SEQ-077](../../../../crates/hft-sequencer/TESTS.md#seq-077-实盘回报与-bbo-的等待上界) 要求每条 ctl/实盘回报到达后最多等待一条 BBO、Advice 或 md。Poisson 到达流可以在同一轮到达第五条 ctl,两条要求无法同时成立。

真实 LiveDriver、B0、2,000 条 L2Batch、128 条 BBO(含合并)、16 条 Advice,以及独立均值 200 µs 的 BBO/回报/ctl 到达流下,种子 `0x784b9e117907309c` 产生 ctl 等待 12 条低优先级 Input。消费者每次交出 Input 推进虚拟时间 5,000 TSC(1 GHz);观察者以 journal 的 Input 时间戳与原始命令体检查位置,不读取调度器内部额度作为预期。

## Proposal

待 owner 选择额度耗尽后的行为。如果等待上界优先,当 ctl/实盘 rep 环非空且本轮额度不足时结束本轮,由下一次 step 重置额度后先处理高优先级输入;同步修改 SEQ-063、主循环规格与 architecture。也需要明确 BBO 自身额度耗尽及高优先级同环积压时的上界口径。

当前保留冻结的 SEQ-063 行为,不修改生产主循环。`tests/preemption_dst.rs::seq_077_preempt_wait_bounds` 显式 ignored,首个种子固定为上述反例,不计 SEQ-077 完成。不放宽断言、不限制 Poisson 生成器来避开额度耗尽。

## Alternatives considered

- 提高每轮 ctl 额度只能降低反例概率,不能保证任意到达序列下的等待上界。
- 在测试里删去额度耗尽的样本会改变 SEQ-077 的冻结前置。
- 用 wall-clock 测量不能消除取出顺序上的矛盾;参考机基准仍需单独运行。

## Acceptance criteria

- 先裁决额度耗尽时是否结束本轮,以及等待上界是否排除自身环排队时间。
- 冻结用例同步后运行 SEQ-055–078 全组、SEQ-076/077 的提交派生种子与回归集,两种处理耗时分布的 journal 各自 VerifyDriver 通过。
- SEQ-319 另在参考机验证逐样本时间上界,不能由本虚拟时间测试替代。
