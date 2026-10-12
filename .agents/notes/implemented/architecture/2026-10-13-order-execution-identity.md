# Agent Note: 订单回报保留单笔成交身份

Status: implemented

## Problem

LED-080 要覆盖原始外来成交与同订单累计回报重叠,同时保留钱包逐单位一致。OrderEvent v2 只有订单身份、累计量与累计现金,不能证明它的末笔成交与正 trade_id 的 ExternalFill 相同。数量与金额碰巧相等也可能来自不同成交,据此抵扣会掩盖漏账。

## Decision

在 OrderEvent 的体尾记录本笔成交的正 ID,旧字段与旧记录保持兼容。0 明确表示没有正 ID 事实,不从 slot、gen、订单号或本地计数生成交易所 ID。字段只提供原始身份,不改变 OMS、ledger 的去重与结算规则。布局与缺省语义归 [hft-event](../../../../crates/hft-event/README.md)。

跨来源结算仍需原件、费用与累计区间的独立证明。不能把一个 ID 绑定到跨过多笔成交的累计差额,也不能因这个字段存在就放行现有 ExternalOverlap。LED-080 的资金闭环单独验收。

## Alternatives considered

用累计量相等推断重复无法区分旧回报与同量的新成交。把 ExternalFill.fill_seq 的正 ID 分支改作累计位置会改变已有字段语义,且影响 ReconcileBatch 与持久原件。单独保留 OrderEvent 的逐笔 ID 不改这些格式。

## Acceptance criteria

EVT-071 保留 v1/v2 所有原字段并将缺失 ID 补零;EVT-205 覆盖 0、正 ID、u64 高位与最大值的完整编码和 CRC 保护。EVT-050/173 与 gen-schema 核对布局和生成物。既有 DST 种子、50 个 HashTrail 与完整/窗口文件回放保持原验证强度。

## Risks

已记录 ID 不等于已实现跨来源资金核验。缺少区间或现金证明仍应失败,不能把事件扩展算作 LED-080 通过。资金与恢复改动保留在草稿 PR,交负责人评审后再合并。
