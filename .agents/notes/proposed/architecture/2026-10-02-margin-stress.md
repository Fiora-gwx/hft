# Agent Note: 账户级保证金分档(MarginStress):禁开 → 通知减仓 → 引擎默认减永续

Status: proposed

> **第八轮补充(2026-10-03)**:收窄——L3 默认减仓覆盖 `Broken` 的 group(永续腿 reduce-only 先减,再处理现货腿);见 [broken-auto-fix](2026-10-03-broken-auto-fix.md)。

> **第八轮第三次修订(2026-10-03)**:部分取代第 2 条——分档度量改为到强平价的距离 `liq_dist`(L1 ≤ 0.25 禁开、L2 ≤ 0.15 通知减仓、L3 ≤ 0.10 或截止已过强制减仓,滞回 0.02,数据过期 → L1),"维持/保证金余额"比率只做交叉校验(≥ 0.8 → 至少 L2);`LINK_FUNDS` 按 `AccountConfigReport` 读到的实际杠杆;hft-sim"单边 +30%"压力用例为门槛 ⑦ 必过项;见 [margin-liq-distance](2026-10-03-margin-liq-distance.md)。

> **第八轮第四次修订(2026-10-03)**:风险一节"模拟账户与小额实盘期间按实测调整"中的小额实盘已不存在,阈值在模拟账户与正式实盘中标定;见 [no-small-live](2026-10-03-no-small-live.md)。

> **第八轮第六次修订(2026-10-03)**:补充——用户流缺口后的自动入账(`AUTO_EXTERNAL`)只在用户流稳定时成立,之后补推的同方向强平/ADL 由 ledger 抵扣;见 [auto-external-guard](2026-10-03-auto-external-guard.md)。

## Problem

组合携带是一个所做空永续、另一个所持有现货。按雏形默认参数估算,持仓币普涨 50% 时永续所权益只剩总资金约 10%,普涨 60% 时约 4%,接近维持保证金;现货浮盈在另一个所,补不过来([评审 §2.2](../../../../design/_work/review-2026-10-02-主策略适配与执行正确性.md))。修订前的规格没有保证金率、强平价、ADL 档的数据来源;自身被强平/ADL 的成交被当作 `ForeignOrder` 只记 Note,仓位不更新,LegTracker 以为仍对冲(B3);强平距离硬动作是按单个 group 写的 cdex 小仓口径;treasury 是 optional(A5)。

## Proposal

档位与 cap 归 [`hft-risk`](../../../../crates/hft-risk/README.md);事件体归 [`hft-event`](../../../../crates/hft-event/README.md);默认减仓归 [`hft-exec`](../../../../crates/hft-exec/README.md);监控与建议归 [treasury](../../../../python/treasury/README.md)。

1. **数据**:venue gw 按 `VenueCaps.margin_data` 轮询或接收 → `MarginReport`(保证金余额、维持保证金、可用保证金、统一口径的保证金率 = 维持保证金 / 保证金余额、每 inst 强平价与 ADL 档);自身强平、ADL、人工、交割成交 → `ExternalFill`:ledger 入账、LegTracker 更新、报 P0、策略收到 `External`。
2. **按账户的 `MarginStress` cap**:L1 `margin_ratio ≥ margin_block_open`(默认 0.60)禁开;L2 `≥ margin_reduce`(0.70)向策略发 `RiskEvent::MarginStress`,`deleverage_deadline_ms`(30 000)内须减仓;L3 `≥ margin_force`(0.80)或截止已过 → 执行层按比例减永续敞口(每步 `force_step` 20%,`PerpFirst`,reason = MarginStress);数据过期(`margin_stale_ms`)视为 L1。默认值待标定。
3. **watchtower** 对 L2/L3 与 `ExternalFill` 报 P0;**treasury 转 core**:跨所余额与保证金监控、普涨情景下的保证金余量、划转建议;不自动划转,交易 key 不带提现权限(R8-4)。

## Alternatives considered

**按 group 的强平距离硬动作(原 cdex ClosePolicy 口径)。** 全仓下保证金是账户共享的,单 group 的"距离"没有意义,输入也没有来源。

**自动跨所划转补保证金。** 需要在交易主机上放带提现权限的 key,安全面不可接受;跨所到账以分钟到小时计,救不了急;用户决定不自动划转([round8-decisions](../../../../design/_work/round8-decisions.md) R8-4)。

**只告警(雏形的做法)。** 逼空时人的反应太慢;引擎必须在策略失灵时也能保证账户安全(R8-3 第 4 条)。

**全部交给策略处理。** 风控边界要求保证金率由引擎负责;策略可以更早、更聪明地减仓,引擎只兜底。

## Acceptance criteria

- DST:保证金率从 0.5 升到 0.85,依次出现禁开、`MarginStress` 事件、截止后每步减永续 20%(先永续),全程不增加敞口;`MarginReport` 超过 `margin_stale_ms` 未更新 → L1。
- `ExternalFill{cause = Liquidation}` 在同一事件内更新 ledger 与 LegTracker、报 P0;策略未在 `external_deadline_ms` 内处理时执行层平掉多出的一腿。
- treasury 对评审 §2.2 的样例账户算出的"普涨多少到 L2"与手算一致(误差 ≤ 1 个百分点)。

## Risks

- 阈值未标定:默认值标"待标定",模拟账户与小额实盘期间按实测调整。
- 各所保证金率口径不同:换算写在各 venue README,未核实前标待核实。
- 强制减仓可能在不利价格成交:这是安全的代价;解除与滞回条件由 hft-risk 定义,避免在阈值附近反复触发。
- 保证金轮询消耗限速:轮询周期是 `margin_data` 参数,计入令牌桶。
