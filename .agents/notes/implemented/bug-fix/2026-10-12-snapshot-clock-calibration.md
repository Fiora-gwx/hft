# Agent Note: 快照校准元数据不参与交易状态比对

Status: implemented

## Problem

[SEQ-249](../../../../crates/hft-sequencer/TESTS.md#seq-249-state_hash-不计节-16out_hash-屏蔽测量字段) 要求最近的 ClockStatus 反映到快照校准行,同时不影响状态哈希。实现只在恢复时读回该行,运行期没有更新;引导快照也早于本段校准记录构造。回放比较完整快照体,会把校准差异误报成交易状态分歧。

## Decision

保留原有节 16 的唯一哈希豁免,实际记录 ClockStatus 后复制校准事实和来源序号。引导块先获得记录序号再采集快照;回放按日志处理 Meta,不增加 Input 计数,冷恢复同时处理已消费的前缀。校准只给冷读者使用,业务时间仍来自 EngineClock 和段头。

输出比较屏蔽校准行体,保留节头结构与所有业务字段的比较。该决定落实[自足 journal](../../proposed/architecture/2026-09-30-self-contained-journal.md) 中取证数据与决策数据的边界,不扩大哈希豁免。字段契约归 [sequencer](../../../../crates/hft-sequencer/README.md#记录分类)。

## Alternatives considered

**保持校准行全零**可维持旧快照字节,但冷读者拿不到最近校准事实。**只比较快照哈希**会同时忽略节头损坏及其他未入哈希的头部字段,也削弱现有逐字节检查。**让 Meta 消耗回放 Input 配额**会改变逐 Input 冷验证的进度契约。保留来源与独立屏蔽行体能满足已有边界。

## Consequences

快照与事件布局不变,已有 HashTrail 种子和步数保持。真实 hk、引导滚段、完整/窗口回放和恢复尾部覆盖校准来源;数量篡改并重算日志摘要、节 16 行数损坏必须报错。未提交 Meta 尾部依旧丢弃。开发版本写出的全零校准行仍可恢复,不能将这些零值当作有效 UTC 校准证据。
