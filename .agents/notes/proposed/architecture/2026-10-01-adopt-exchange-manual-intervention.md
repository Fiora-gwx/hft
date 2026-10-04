# Agent Note: 人工干预经 AdoptExchange 入账,解除 kill 不自动对冲

Status: proposed

> **第八轮(2026-10-02)**:补充——`AdoptExchange` 可采纳余额条目(现货库存)与可选入场价;新增 `SetBalanceBaseline` 登记非策略资产(命令归 control-plane);"迁移时剩余仓位入账"只在 optional 的 cdex 迁移中使用;见 [ccex-first-scope](2026-10-02-ccex-first-scope.md)。

> **第八轮补充(2026-10-03)**:收窄第 1 条——`Broken` 的未对冲量在无人工 `HedgeFix` 时于 `broken_auto_fix_ms` 后自动有界平仓(解除 kill 本身仍不触发任何对冲);资金费差异不经 `AdoptExchange`,由账本按交易所流水自动校正;见 [broken-auto-fix](2026-10-03-broken-auto-fix.md)、[funding-recon-alert-only](2026-10-03-funding-recon-alert-only.md)。

## Problem

现有工作流里,操作者经常直接在交易所网页上平仓、补腿,或者用 `/hedgefix` 人工确认补腿([research/00 §6.2](../../../../design/research/00-existing-codebase-inventory.md))。在 journal 即真相的新栈里,网页上的人工操作不会出现在 journal 中:系统的账本仍认为存在未对冲量。早期规格在解除 kill 或锁存时会自动 `HedgeFix`——于是系统把操作者刚修好的敞口再补一次,反向打开一条新腿。专项评审把它列为三条误交易路径之一。

## Proposal

裁决见 第五轮裁决(2026-10-01 模式/账户统一模型,主控记录) M5、M6;命令规格归 [control-plane](../../../../docs/subsystems/control-plane.md),账目归 [`hft-ledger`](../../../../crates/hft-ledger/README.md):

1. 解除任何 kill 或锁存**都不触发自动 `HedgeFix`**;解除时存在的未对冲量标为 Broken,只能由 `HedgeFix` 命令处理(预览基于最新对账)。
2. 新增升级类命令 `AdoptExchange{account, venue, inst, report_id}`:把指定对账报告中的差额写成 ledger 调整(class = `Adjust`),用于人工在交易所操作之后。命令与结果进 journal,账本与交易所一致后才能 `Resume`。迁移时旧引擎留下的剩余仓位也用它入账([runbooks/migration](../../../../docs/runbooks/migration.md))。
3. `Annotate`(给 journal 写人工备注)与之一起提供,人工操作的原因与 `AdoptExchange` 关联。
4. 对账恢复条件的唯一定义:同一 (account, venue) 连续两轮 Ignore,且期间该 venue 无 UserStream Down。

## Alternatives considered

**解除 kill 时自动 `HedgeFix`(原方案)。** 见 Problem:无法区分"系统该补的腿"和"人已经补过的腿"。

**禁止人工在交易所操作。** 不现实:交易所故障、我们的系统故障时,网页是最后的手段;规则应让人工操作可记账,而不是假装它不会发生。

**reconciler 发现差异后自动采纳交易所值。** 会把真正的缺陷(漏解码的成交、错误的对冲)静默吞掉;采纳必须是人看过对账报告之后的显式决定。

## Acceptance criteria

- DST:交易所侧手工平仓 → 对账出差 → `AdoptExchange` → `Resume`,全过程不产生对冲单;不 Adopt 直接 `Resume` → 未对冲量标 Broken,只接受 `HedgeFix` 命令([testing §4.1](../../../../docs/testing.md#41-模式账户与人工干预))。
- 属性测试:`Resume` 从不产生 `OutMsg` 中的开仓或对冲单。
- ledger:`Adjust` 类调整在 PnL 分解中单独成项,回放可重建。

## Risks

- `AdoptExchange` 是升级类命令,可能被误用来"抹平"真实缺陷:需 Trader + 确认,预览显示差额与最近成交;watchtower 统计 Adopt 次数,异常增多即告警。
- 操作者忘记 Adopt 时,该账户停在 Broken/CloseOnly:这是有意的保守默认,`/status` 明确提示原因。
