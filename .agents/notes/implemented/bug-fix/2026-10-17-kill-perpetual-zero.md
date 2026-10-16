# Agent Note: Kill 永续尾仓严格归零

Status: implemented

## Problem

2026-10-16 shape run 中,Kill 被执行层 dust 完成条件截断,留下 Binance USDⓈ-M ETH 空仓 0.001。人工精确量 reduce-only 买单可以平掉该仓。负责人于 2026-10-17 确认清仓的永续完成条件必须严格为零;现货不可交易尾量仍作为库存记录。

## Decision

清仓锁下的永续零目标不使用 dust 容忍带;能力位选择精确量 reduce-only MARKET 或品种整仓端点。订单仍经过原风险链,只对经过身份、能力、类别与标志校验的执行层尾仓平仓放宽 lot 整数倍。成交证据更新持仓,请求受理不能证明归零。无组四腿窗口完成后继续扫描,同场所多个品种按订单槽处理成交。

协议行为归 [Binance](../../../../crates/venue-binance/README.md)、[Bitget](../../../../crates/venue-bitget/README.md#uta-永续尾仓平仓)。Binance 官方不支持普通 MARKET + closePosition,采用文档明确豁免最小名义的精确量 reduce-only。Bitget 全仓端点的服务端订单号通过已捕获的请求身份绑定,先到成交沿用暂存机制。

## Alternatives considered

降低 dust 阈值仍会留下 lot 舍入残量。把本地 ledger 写零没有交易证据。Binance 条件 closePosition 会引入触发价格、条件单生命周期与停机等待,不能等同即时 MARKET。Bitget 在单个 group 数量不等于账户品种总仓时直接 flash-close 会把其他 group 的仓位错误记给该腿,因此该情况保持门控并明确未完成。

## Consequences

MARKET 平仓的实际价格由场所决定,风险链的估值价不能保证成交限价。Binance 低于 minQty/step 的接受能力、Bitget 整仓端点的真实小额 fixture 仍需 testnet 采集;合成编码与回报测试仅证明本地行为。无能力、拒绝、Unknown、共享品种归属不一致均保持未完成,不能宣称账户已清零。旧 refdata 必须重新生成才能启用新能力位。

真实引擎 SEQ-185 覆盖部分成交尾仓、Kill、Drain、实际成交到零与自足 journal replay。EXE-361 覆盖正负仓、配对、现货残留以及跨批无组尾仓;VEN-228 覆盖 REST 回执绑定早到 fill。资金/风险变更在 [changes](../../../../docs/changes.md) 标为待评审,负责人方向已确认。
