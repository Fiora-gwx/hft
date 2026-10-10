# Agent Note: 非成交回报先到时保留迟到手续费

Status: implemented

## Problem

真实 sequencer 的 DST 中,CancelReject 或查询回报先带来了累计成交量。随后的成交回报携带手续费,却因累计量没有增加被当成重复消息丢弃。模拟账户真值与引擎账本因此出现费用差额。只恢复已成交数量不能判断该数量的费用是否已经记账。

## Decision

OMS 的 SlotExt 保留成交类回报的累计量水位 fee_cum。数量已经入账、费用水位尚未覆盖的 PartialFill 或 Fill 只产生 FeeOut。冷账本按同一规则生成 FeeDelta,保留该 OrderEvent 的序号。

持久化归属仍在 [OMS README](../../../../crates/hft-oms/README.md) 的快照行格式一节。节 2 使用行版本 2,旧版行按 filled 初始化水位。旧快照没有足够信息补出当时遗漏的费用,按已入账处理可避免恢复时重复扣费。冷读者与 OMS 都拒绝未知行版本。

## Alternatives considered

**继续只比较 cum_filled** 会重现 OMS-460 与 LED-271 的漏费。数量和费用可能来自两条不同类型的回报,必须分别记录进度。

**恢复旧快照时把水位置零** 无法区分费用已入账与尚未入账,重复推送成交回报可能再次扣费。兼容读者采用保守水位,费用历史缺口由原有对账流程处理。

## Consequences

SlotExt 占用与节 2 行长增加,具体数值与资源预算放在 OMS README。OMS-461 与 LED-272 检查旧版兼容、新版保留未入账费用、重复回报去重与未知版本拒绝。新旧冷读者不能混用新版快照,发布仍须遵守仓库的版本与回滚纪律。
