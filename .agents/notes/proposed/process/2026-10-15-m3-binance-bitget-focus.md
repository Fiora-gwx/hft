# Agent Note: M3 聚焦两家交易所

Status: proposed

## Problem

owner 的当前目标是 Binance 与 Bitget 的跨交易所套利、资金费策略及测试网接入。把其他交易所的未实现用例留在 M3，会使本轮接入范围与进度报告不一致。账户 key 尚未提供，本轮验证只使用 mocks 与 fixtures。

## Proposal

[OKX 测试规格](../../../../crates/venue-okx/TESTS.md) 中未实现的 M3 用例统一标为 M9，沿用 [M2-lite](../../implemented/architecture/2026-10-14-m2-lite.md) 已使用的后续里程碑。共 175 项，其中自动级别 173 项、人工 testnet 项 2 项；不删除用例、不声称实现，也不修改协议契约。

Bybit、Gate、Hyperliquid、KuCoin 及 DEX/chain 仍在本轮之外；其规格当前没有 M3 用例，无需重标。Binance、Bitget、通用网络/venue 运行时、账户参考数据、hft-core 与 latency-probe 保留原里程碑。

## Alternatives considered

**保留 OKX 的 M3 标签**会把 owner 已明确排除的接入工作计入本轮剩余项。**新建里程碑名称**没有必要，现有 [test-trace 配置](../../../../tools/xtask/test-trace.toml) 支持数字里程碑，M9 已有一致用法。

## Acceptance criteria

用临时 `current_milestone = "M3"` 运行 test-trace，随后恢复配置。基线为 1,096 个自动 M3 用例，其中 1,001 个未实现；仅此重标后为 923 / 828，减少的 173 项全部转入 M9。main 的 current_milestone 不变。测试网事实、正式上线门槛均不因本提案豁免。

## Risks

这只是 owner 当前聚焦范围的排期提案，须随 draft PR 评审。M3 报告减少不代表实现增加；恢复其他交易所接入时仍须完整验收其冻结规格。
