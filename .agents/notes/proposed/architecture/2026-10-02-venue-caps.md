# Agent Note: 交易所差异写进能力表 VenueCaps,策略与执行层不出现交易所名

Status: proposed

> **第八轮补充(2026-10-03)**:补充——同一交易所的现货与永续共用钱包的情形被 [no-same-exchange-spot-perp](2026-10-03-no-same-exchange-spot-perp.md) 排除,`VenueCaps` 与 refdata 不建模余额域。

## Problem

修订前的 C 族写成"Binance 永续 + Gate 现货",交易所差异散落在策略、ledger 与各 venue README 的叙述里([评审](../../../../design/_work/review-2026-10-02-主策略适配与执行正确性.md) P0-4、P1-6/7/8):现货手续费按基础币扣除时仓位与对冲按毛量计(B4);账户持仓/保证金模式启动不核验(C6);用户流未知事件被当作解码失败、引发查询风暴(C12);全仓资金费推送可能不带 symbol(C13,待核实);没有保证金与 ADL 数据来源。用户要求交易所不固定、策略代码中不出现交易所名([round8-decisions](../../../../design/_work/round8-decisions.md) R8-1)。

## Proposal

类型归 [`hft-refdata`](../../../../crates/hft-refdata/README.md);默认值与"待核实"归各 `venue-*`(首批 [Binance](../../../../crates/venue-binance/README.md)、[OKX](../../../../crates/venue-okx/README.md)、[Bitget](../../../../crates/venue-bitget/README.md))。

1. **`VenueCaps` 按 `(venue, MarketKind ∈ {Spot, LinearPerp})`**:`fee_asset_rule`、`reduce_only`、`required_position_mode`/`required_margin_mode`、`funding_settle`(`PerSymbolPush | BalanceOnlyPush | RateOnly`)、`margin_data`(`Push | Poll`)、`own_liquidation_marker`、`ignorable_user_events`、能力位(`amend`、`post_only`、`ioc`、`batch_cancel`、`dms`)、`testnet`。venue crate 提供默认值,refdata 可覆盖。
2. **引擎按能力表行事**:ledger 与 LegTracker 用 `FillDelta.net_qty`;执行层现货卖出量取 min(库存, 可用余额);`BalanceOnlyPush`/`RateOnly` 由 sequencer 按"结算时刻持仓 × mark × 费率"合成每 inst 的 `FundingSettle{synth=1}`,推送与流水只做校验;gw 按 `margin_data` 产出 `MarginReport`;启动闸门 `ACCOUNT_CFG` 核验账户配置;`ignorable_user_events` 只计数。
3. **策略经 `Views::caps` 读取**;策略与执行层代码中不出现交易所名(doc-check)。
4. **没有官方出处的行为一律"待核实"**,写在对应 venue README 的开发备忘里并给核实方法(本机连不上官方文档站,在服务器上用真实报文金样核实)。

## Alternatives considered

**策略里按交易所分支(`if venue == X`)。** 策略与交易所耦合;交易所不固定,每换一家都要改策略。

**全部在 venue crate 里抹平(例如 venue 层自己合成每币资金费)。** venue crate 是纯协议,看不到仓位与 mark;合成需要 sequencer 状态,只能由引擎按能力表决定。

**强制统一账户配置(例如开平台币抵扣让手续费都扣报价币)。** 不是每家都支持,配置仍可能被人改;仍需启动核验与运行中检测,能力表 + `ACCOUNT_CFG` 一并解决。

## Acceptance criteria

- 每个首批 venue × 市场类型的能力表字段都有金样或待核实项与核实方法。
- 场景:手续费扣基础币时 ledger 净量 = 成交量 − 手续费,现货卖出量 ≤ 可用余额;`BalanceOnlyPush` 的所合成的每 inst 资金费与 reconciler 拉到的带 symbol 流水逐笔一致。
- 未知用户事件类型:计数增加、不触发重同步与查询。
- 账户配置与能力表不符 → `ACCOUNT_CFG` 位为 0,该账户不放行开仓。
- doc-check:`strategies/*` 与 `hft-exec` README 中交易所名出现 0 次。

## Risks

- 能力表事实错误:M3 在服务器上用真实报文核实,金样随交易所变更更新。
- 字段会随新交易所增长:只追加,默认值保守。
- refdata 覆盖与 venue 默认值漂移:覆盖项在 hft-cfg 差异报告中单列。
