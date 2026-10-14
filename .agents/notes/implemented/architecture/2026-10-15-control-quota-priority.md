# Agent Note: 控制命令不排在耗尽额度后的低优先级输入后

Status: implemented

## Problem

SEQ-063 的继续消费 BBO 行为会使已经到达的第五条控制命令等待多条低优先级输入,与 SEQ-077 的控制等待上界冲突。[原提案](../../proposed/architecture/2026-10-15-sequencer-quota-wait-bound.md) 保存固定反例。

## Decision

按负责人本次任务中的裁决,LiveDriver 在非背压轮 b–g 每个输入边界先检查 ctl。剩余额度足够就交出控制输入;额度耗尽则结束本轮,下一轮恢复额度后先取 ctl。Kill、CancelAll、Drain 与其他控制命令遵循同一规则。非控制输入仍计入原有每轮额度;通知背压轮仍只续推通知和有容量的实盘回报。

SEQ-077 不再忽略,固定反例、提交派生种子和回归种子均运行。等待上界约束低优先级输入的阻塞,不消除同控制环排队、定时器、关键帧和通知背压造成的等待。

## Alternatives considered

**继续低优先级输入**违反负责人要求,固定种子可复现多条输入阻塞。

**单纯提高额度**只能降低出现概率,不能给出与低优先级积压无关的上界。

**无限绕过额度**使单次 step 无界;结束本轮可保留 publish 与 heartbeat 边界。

## Consequences

持续控制流可以推迟 housekeeping 和行情;每轮仍有控制条数与字节上限。记录顺序变化由 journal 真实保存,VerifyDriver 按记录重放。SEQ-319 的参考机测量仍须独立完成,确定性虚拟时间测试不能替代它。
