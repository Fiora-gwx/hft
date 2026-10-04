# Agent Note: 跨重启连续的引擎时间 `Now.engine_ns`,持久状态不存 TSC

Status: proposed

## Problem

策略、ledger 与 risk 的持久状态里存的是 TSC(`ConfirmWindow.since`、`Throttle`、`reentry_block_until`、`round_start_tsc`、`MarginRow.deadline_tsc` 等)。TSC 的原点随开机变化,换主机后也不同;重启或换机后这些时刻失真——冷却期可能被跳过,也可能永远不结束([评审](../../../../design/_work/review-2026-10-03-M0b独立评审.md) P1-3)。[recovery-epoch-and-boot-rules](2026-10-01-recovery-epoch-and-boot-rules.md) 只为 OMS/exec 定义了 `recovery_end` 平移与按 `boot_id` 重建定时器,其他状态没有统一口径。

## Proposal

`Now` 归 [hft-strategy-sdk](../../../../crates/hft-strategy-sdk/README.md);引擎时间的累加归 [hft-sequencer](../../../../crates/hft-sequencer/README.md);各使用方([hft-strat-lib](../../../../crates/hft-strat-lib/README.md)、[hft-ledger](../../../../crates/hft-ledger/README.md)、[hft-risk](../../../../crates/hft-risk/README.md)、各策略)改存它。

1. `Now.engine_ns: u64`:跨重启连续、停机不计的引擎时间,由 journal 中各次运行的区间累加得到,回放确定。
2. 策略、strat-lib、ledger、risk 的持久状态一律存 `engine_ns`;TSC 只在单次启动内使用(延迟测量、本次运行内的计时)。
3. OMS/exec 的 `recovery_end` 平移保留;"对冲截止只计引擎运行时间"与本规则同一语义。
4. 停机期间冷却期、截止、节流都"暂停";需要墙钟的规则(资金费结算、日切、上架天数)用事件携带的 UTC,并在策略 README 里写明。

## Alternatives considered

**`restore(stop_tsc, now)`,每个组件自己平移(评审给出的另一修法)。** 每个组件各写一遍平移,新增状态容易漏;一个引擎时间让新状态默认正确。

**一律用 UTC 墙钟。** 墙钟会跳变(chrony 步进),停机时间被计入会让截止在重启瞬间集体到期。

**快照里存 TSC + `boot_id`,换 boot 时按 UTC 剩余时长重建。** 现有规则只覆盖定时器,不覆盖策略与 ledger 内部的时间戳字段。

## Acceptance criteria

- DST:冷却期中途 Drain、停机 1 h、换 `boot_id` 冷启动 → 冷却期剩余时长与停机前相同(按 `engine_ns`);同一 journal 回放两次,`engine_ns` 序列逐字节相同。
- 规格中策略、strat-lib、ledger、risk 的持久结构不再有跨启动使用的 `*_tsc` 字段(按评审清单逐项核对)。

## Risks

- 停机不计时不适合"上架后 N 天"之类的规则:这类规则必须显式用 UTC。
- 依赖 journal 运行区间完整:尾部丢失(`tail_lost`)时从最近快照的 `engine_ns` 起步,丢失的区间不计。
