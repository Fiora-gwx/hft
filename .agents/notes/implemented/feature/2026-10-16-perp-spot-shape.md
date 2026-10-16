# Agent Note: 两策略限定空永续与多现货

Status: implemented

## Problem

双向跨所价差与双永续资金费选择不符合负责人当前产品范围，现货反向开仓还需要借币与做空能力。2026-10-16 负责人指定同资产 USDT 计价的 Binance USDⓈ-M 永续与 Bitget SPOT。

## Decision

第 0 腿固定协议 VenueId 1，第 1 腿固定 VenueId 11。两策略参数检查在默认禁用状态也拒绝其他市场、交易所、腿顺序与缺引用；热路径不按交易所分支。cross-taker 只卖永续/买现货开仓，反向净边达到 close_edge 只减持有仓位，沿用非 baseline 库存限制。portfolio-carry 只持有空永续/多现货，资金费扣往返费用、滑点、借贷/机会成本与 expected_basis_cost 后做进入和衰减退出。

两腿同事件 IOC、Reverse、Unknown 与引擎风险边界保持。参数 schema 升到 version 2，新增字段只追加；旧版本的合法形态可由 schema 默认值升级，旧双永续形态被拒绝。具体阈值与整数口径归各策略 README。

最大持有时间和相对入场基差止损已考虑，当前保持关闭且没有自动强制亏损退出参数。它们需要持有期与基差样本标定；S3 数据访问尚缺，不凭空设期限。close_edge 可由人工明确配置负退出阈值。

## Alternatives considered

**保留双永续与双向自动择优。** 负责人收窄[跨所价差](2026-10-14-cross-taker-executable-spread.md)与[资金费差](2026-10-14-bounded-funding-carry.md)的范围，不作为可启用路径；carry 的纯差分计算函数可保留供对照，不绕过组校验。

**只修改测试网配置。** 无法阻止启动或 SetParams 重新引入错误组，故策略参数验证也强制产品形态。

## Consequences

交易策略受部署 allowlist 限制，扩展交易所或恢复其他形态需要新的明确决定。现货资金费恒为零；校准需要双方 BBO 与永续预测/实际资金费，不能要求现货有资金费记录。真实引擎开平仓、Reverse 与 replay/DST 回归覆盖这条路径；M3-lite 接受的旧双永续测试网运行不算新形态的实盘验收。
