# Agent Note: 不做同一交易所的现货 × 永续:一个账户在每个交易所只用一个市场

Status: proposed

## Problem

修订后的规格允许"同所现货 × 永续"只需新策略 + 配置即可接入([ccex-first-scope](2026-10-02-ccex-first-scope.md) 依据的 R8-1)。但"余额域"(同一交易所的现货与永续共用一个钱包)只在 refdata 与 reconciler 里有字段,ledger、SDK 的 `PortfolioView`、风控与 hft-sim 都按 venue 记现金:OKX 统一账户、Bitget UTA 下同一笔 USDT 会在两个 `VenueBook` 各记一份,策略按约 2 倍资金分配,第二腿余额不足被拒 → Broken → 裸腿;refdata 对未核实的所默认"各自独立"([独立评审](../../../../design/_work/review-2026-10-03-M0独立评审.md) P0-1)。用户决定不做同一交易所的现货 × 永续([round8-decisions](../../../../design/_work/round8-decisions.md) R8-8 第 1 条)。

## Proposal

规则归 [hft-refdata](../../../../crates/hft-refdata/README.md) 与 [control-plane](../../../../docs/subsystems/control-plane.md);操作归 [launch](../../../../docs/runbooks/launch.md):

1. **refdata 加载校验**:一个 `AccountId` 的 venue 集合中不得有两个 `exchange` 相同的 `VenueId`,否则 `SameExchangeMarkets`、拒绝加载;Sim 账户同样适用。
2. **参数校验**:同一 `GroupSpec` 的两条腿不得在同一交易所,`SetParams` 返回 `InvalidArgs{SameExchangeLegs}`;pipeline 在组装时先拦下,实例配置的两个角色 venue 同所即加载失败。
3. **删除余额域**:`balance_domain` 字段与相关逻辑从 refdata、reconciler、hft-sim 删除;hft-refdata 写明"同所现货与永续共用钱包的情形被本规则排除,因此不建模余额域"。
4. **文档**:架构与策略文档中"同所现货 × 永续可接入"改为"不做";组合携带 = 交易所 X 的永续 + 交易所 Y(≠ X)的现货。
5. **开户**:同一策略实例在一个交易所只开一个子账户、只开通一个市场。

## Alternatives considered

**按余额域建模(评审给的修法)。** ledger 按域记现金、`PortfolioView` 按域汇总、sim 按域建钱包、B1 按域查可负担性——改动横跨 ledger、SDK、风控、sim 四处,且依赖 OKX/Bitget 账户模式的大量未核实事实;首个策略是跨所组合携带,用不到。

**只拒绝"未核实"的所,已核实的允许。** 仍要先把余额域建模做完,收益同上;未核实项在本机无法核实。

**同一交易所开两个子账户(现货一个、永续一个)放进同一账户。** 钱包确实分开,但同所的费率档、子账户间划转与风控联动都未核实,用户的决定是整体不做;需要时另写 Note 取代本条。

## Acceptance criteria

- refdata 中某账户同时含 Binance 现货与 Binance 永续(Live 或 Sim)→ 加载失败 `SameExchangeMarkets`。
- `SetParams` 中某 group 两腿同一交易所 → `InvalidArgs{SameExchangeLegs}`;pipeline 实例配置两角色同所 → 启动失败。
- `check_hft.py`:规格中 `balance_domain` 出现 0 次。

## Risks

- 有意放弃:同所现货 × 永续的组合携带(无跨所划转、无跨所保证金错配);跨所划转更频繁,由 treasury 给建议、人工执行。
- 交易所把现货与合约强制合并为统一账户的所,其单一市场的行为仍需在 venue README 核实(账户模式、可用余额口径)。
