# Agent Note: 资金费差异只告警、不进启动闸门,账本按交易所流水自动校正

Status: proposed

> **第八轮第三次修订(2026-10-03)**:收窄第 3 条——reconciler 分级前用同组报告的资金费条目修正结算资产的余额差;ledger 的 `NoHistory` 在比对窗口结束后按交易所金额入账并写 `Note{FundingCorrected}`(P1),不再要求人工处置。依据[评审](../../../../design/_work/review-2026-10-03-M0b独立评审.md) P1-7;规格归 [hft-ledger](../../../../crates/hft-ledger/README.md)、[reconciler](../../../../apps/reconciler/README.md)。

## Problem

对账条目的处置只有 `AdoptExchange`(仓位、余额)与 `SetBalanceBaseline`;`AdjustDelta` 只有 Position/Balance。资金费有三种推送形态(每 inst 推送、只推余额、只给费率),合成值与交易所流水在只给费率的所(Bitget `RateOnly`)每期都可能有小差额;严格的 `RECONCILED` 闸门可能被一条资金费 Warn 永久卡住,人每 8 小时处理一次不现实([独立评审](../../../../design/_work/review-2026-10-03-M0独立评审.md) P1-5)。用户决定按推荐处理:只告警、不进闸门,账本自动校正([round8-decisions](../../../../design/_work/round8-decisions.md) R8-8 第 3 条)。

## Proposal

条目归 [reconciler](../../../../apps/reconciler/README.md);校正归 [hft-ledger](../../../../crates/hft-ledger/README.md);闸门归 [hft-risk](../../../../crates/hft-risk/README.md);告警规则归 [watchtower](../../../../apps/watchtower/README.md);处置归 [incident-response B3](../../../../docs/runbooks/incident-response.md)。

1. 资金费条目**不进** `RECONCILED` 闸门、**不触发** `Recon` cap。
2. reconciler 的 `Funding` 条目带交易所逐笔资金费(account、venue、inst、settle_ts、amount),作为 Input 进 journal。
3. ledger 对结算时刻早于 `funding_recon_lag_ms`(默认 600 000)的差额自动记 `AdjustDelta::Funding = 3`,写 `Note{FundingCorrected}`;比对窗口为最近 48 h,更早的差异出现在对账报告中由人以余额条目处置。
4. watchtower:差额 > `funding_diff_alert`(默认 1 USDT 或 1%)→ P1。资金费以交易所流水为准。

## Alternatives considered

**资金费照常进闸门,每期人工 `AdoptExchange`。** `RateOnly` 的所每期都有差额,闸门会被反复卡住,人工处理不可持续。

**完全忽略资金费差异。** 账本与 `PortfolioView` 漂移,策略按错误的权益分配;系统性的解析缺陷也被隐藏。

**自动校正但不告警。** 同样会隐藏系统性缺陷;保留 P1 告警与 `Note{FundingCorrected}` 记录,每周可汇总复核。

**结算时刻立即校正。** 交易所流水晚到,立即比对会反复出现假差额;等 `funding_recon_lag_ms` 再比。

## Acceptance criteria

- 集成场景:合成资金费与流水差 0.5 USDT → 无 `Recon` cap、闸门放行,10 min 后 ledger 记 `AdjustDelta::Funding` 与 `FundingCorrected`;差 > 1 USDT → P1 告警。
- 差额早于 48 h 窗口 → 不自动校正,出现在对账报告中。
- 校正来自 journal 中的 Input,`replayer verify` 逐字节一致。

## Risks

- 小的系统性错误被自动吸收:告警阈值与每周 `FundingCorrected` 汇总复核;同一 venue 连续同向差额按缺陷处理。
- 交易所流水本身有误的罕见情况:以结算方为准,人工核对后可按余额条目调整。
