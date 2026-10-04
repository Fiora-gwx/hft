# Agent Note: 断腿(Broken)两分钟后自动有界平仓;L3 与 flatten 覆盖 Broken

Status: proposed

> **第八轮第三次修订(2026-10-03)**:收窄第 4 条——第二腿与风险递减单的"报价可用"改按 BBO 连接健康判定(连接 Live、非重同步、`conn_live_tsc` 在 `conn_liveness_ms` 内),不再按最后一次变化的年龄;见 [quote-freshness-conn-health](2026-10-03-quote-freshness-conn-health.md)。人工 `HedgeFix` 与自动平仓一样只平多出的一腿。

> **第八轮第六次修订(2026-10-03)**:收窄——Acceptance criteria 第 2 条的失败口径以 [hft-exec](../../../../crates/hft-exec/README.md#broken-处置) 为准(风控门控与零成交按等待与重试处理,不判失败);`resolve_broken` 只在解除 HARD 时产生,恢复期的等待按多出那一腿所在 venue 的闸门判定且不计时;见 [resolve-broken-hard-only](2026-10-03-resolve-broken-hard-only.md)。

## Problem

修订后的规格里,一个 (account, group) 进入 `Broken` 后只有人工 `HedgeFix` 能处理;MarginStress L3 的默认减仓与 `Kill{Account, flatten}` 明确跳过 Broken 的 cell——最危险的裸永续空单反而不减。对冲腿在退化条件下几乎必然 Broken:B1 两腿检查不含报价新鲜度,对冲腿报价 > 500 ms 或滑点 > 50 bp 时 3 s 内即 Broken;冷启动 30 s 就超过 3 s 的对冲截止;夜间无人值守时裸腿可能挂几个小时([独立评审](../../../../design/_work/review-2026-10-03-M0独立评审.md) P0-2、P0-3)。用户决定按推荐处理([round8-decisions](../../../../design/_work/round8-decisions.md) R8-8 第 3 条)。

## Proposal

规则归 [hft-exec](../../../../crates/hft-exec/README.md)(Broken 处置、默认减仓),检查与 cap 归 [hft-risk](../../../../crates/hft-risk/README.md),处置步骤归 [incident-response A](../../../../docs/runbooks/incident-response.md):

1. **自动有界平仓**:Broken 持续 `broken_auto_fix_ms`(ExecConfig,默认 120 000,只计引擎运行时间)且期间无人工 `HedgeFix`,执行层自动发起**一次**"平掉多出的一腿":永续腿 reduce-only,现货腿卖出 min(库存, 可用余额);按 `max_reduce_notional` 切片、IOC,总量 ≤ 当前裸量。group 有 Pending/Unknown 槽或未结束的链接、闸门未放行、正被引擎减仓时等待(不算失败)。HARD 下暂停,解除后重给完整窗口。成功 → `Note{ExecAutoFixDone}`(P1),Broken 解除,账户 `HedgeFailure` 仍需人工 `Resume`;失败 → `Note{ExecAutoFixFailed}`(P0),保持 Broken 等人工,本次不再尝试。
2. **L3 与 flatten 覆盖 Broken**:`force_deleverage` 与 `engine_flatten` 对 Broken cell 同样生效,先永续腿 reduce-only(`PerpFirst`,受 `max_reduce_notional`),再现货腿。
3. **冷启动不立刻判 Broken**:`hedge_deadline_ms` 只按引擎运行时间累计;恢复后闸门放行时,仍未对冲的链接先做一次 `SecondRetry`。
4. **少进 Broken**(同一轮的上游修正,归 hft-risk/hft-oms/hft-exec):B1 加第二腿报价年龄与可负担性检查(`LINK_STALE`、`LINK_FUNDS`);对冲超滑点按 `rr_max_cross` 改价而不判失败;第二腿数量 ≤ 当前裸量。

## Alternatives considered

**只允许人工 HedgeFix(修订前)。** 夜间裸腿可挂数小时,而裸永续空单在逼空时最危险;评审 P0-3。

**到期后自动补齐缺失的一腿。** 缺的那腿是因报价过期、资金不足或价格越界才失败的,再开仓会扩大敞口;只减不增是兜底动作的唯一安全方向。补齐仍可由人在窗口内 `/hedgefix` 选择。

**进入 Broken 立即自动平仓(不留窗口)。** 人来不及选择"补齐"或发现数量错误;2 分钟是夜间无人时的上限,也给值班看到 P0 的时间。

**失败后反复重试。** 拒单原因(最小下单量、门控、无流动性)通常不会自己消失,重试制造订单风暴;一次终态后交给人。

**L3 继续跳过 Broken。** 正是评审 P0-2 指出的缺口。

## Acceptance criteria

- DST:第二腿被拒 → Broken → 无人工时 120 s(引擎时间)后恰好一次自动平仓,裸量 < 尘埃线,`ExecAutoFixDone`,`HedgeFailure` 仍在;窗口内人工 `HedgeFix` → 不再自动;HARD 期间零订单,解除后 120 s 再开始。
- 自动平仓失败的各支(拒单、IOC 零成交、门控、库存不足)→ `ExecAutoFixFailed` P0,无第二次尝试。
- 保证金率升到 L3 时,含 Broken group 的账户先减永续腿;`Kill{Account, flatten}` 同样处理 Broken。
- `kill -9` 后重启:停机前在途的链接不在第一分钟被判 Broken,先 `SecondRetry`。

## Risks

- 自动平仓可能在不利价格成交:IOC 限价受 `max_cross` 约束,总量 ≤ 裸量;代价是确定的小损失换掉不确定的裸敞口。
- 与人工网页操作重叠:runbook 要求人工操作前 HARD(暂停自动平仓)。
- 数量来源错误时自动平仓按错误的裸量动作:受对冲量上限与裸量上限约束;怀疑数量错时人用 HARD 拦住。
- 有意放弃:无人时"补齐一腿保住价差"的机会。
