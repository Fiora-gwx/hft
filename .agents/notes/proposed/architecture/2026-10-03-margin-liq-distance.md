# Agent Note: MarginStress 按到强平价的距离分档,保证金率只做交叉校验

Status: proposed

> **第八轮第六次修订(2026-10-03)**:补充——账户中有持仓或库存的 inst 与每个 `ref_inst` 都进引擎订阅,group 之外的永续持仓也有标记价与强平距离;见 [position-driven-subscription](2026-10-03-position-driven-subscription.md)。

## Problem

[margin-stress](2026-10-02-margin-stress.md) 用"维持保证金 / 保证金余额"比率分档(0.60 / 0.70 / 0.80)。对一个永续空头,比率 r = m(1+X)/(e−X)(e = 保证金占名义比例,m = 维持保证金率,X = 价格涨幅)。e = 0.30、m = 0.5% 时 L1 在 +28.93%、L3 在 +29.19%、强平在 +29.35%:从禁开到强平只有约 0.42 个百分点的价格空间,L2 的 30 s 截止与 L3 每步 20% 都来不及([评审](../../../../design/_work/review-2026-10-03-M0b独立评审.md) P0-3,主控复算)。另外 hft-risk 的 `LINK_FUNDS` 按常量 `perp_leverage`(默认 1)折算保证金,与首个策略 `perp_margin_frac` 0.30 冲突(评审 P2)。

## Proposal

规则、参数名与默认值归 [hft-risk](../../../../crates/hft-risk/README.md);`MarginReport`/`AccountConfigReport` 归 [hft-event](../../../../crates/hft-event/README.md);压力用例归 [hft-sim](../../../../crates/hft-sim/README.md) 与 [testing §8](../../../../docs/testing.md#8-上线门槛);杠杆设置步骤归 [launch](../../../../docs/runbooks/launch.md);情景余量归 [treasury](../../../../python/treasury/README.md)。

1. **度量**:每 (account, venue) `liq_dist = min over 永续持仓 |liq_px − mark| / mark`;`liq_px` 取交易所 `MarginReport` 与 ledger 估算中更保守(更近)者。
2. **分档**(默认待标定):L1 `liq_dist ≤ liq_dist_block_open`(0.25)禁开;L2 `≤ liq_dist_reduce`(0.15)通知策略减仓,截止 30 s;L3 `≤ liq_dist_force`(0.10)或截止已过 → 引擎强制减仓(永续先,每步 20%);回落带滞回 `liq_dist_hyst`(0.02);数据过期 → L1。比率 ≥ 0.8 → 至少 L2(交叉校验)。
3. **同一例子**:强平在 +29.35% 时,按"统一涨幅、强平价不变"近似,L_k 的涨幅 = (1 + 0.2935)/(1 + d_k) − 1:L1 +3.48%、L2 +12.48%、L3 +17.59%;L3 到强平之间留约 11.8 个百分点。
4. **杠杆**:`LINK_FUNDS` 用 `AccountConfigReport` 读到的实际杠杆,删除常量 `perp_leverage`;策略的 `perp_margin_frac` 默认与运营设定的交易所杠杆一致。
5. **压力用例**:hft-sim 增加"单边 +30%(1 h 内)",作为门槛 ⑦ 的必过项:不得出现强平。

## Alternatives considered

**保留比率,把阈值调低。** 比率与价格空间是非线性关系,随 e 与 m 变化;同一组阈值在不同杠杆下给出的价格空间差别极大,运维也无法直观理解。

**按"权益 / 维持保证金"的倍数分档(评审列出的另一度量)。** 与价格距离的换算依赖持仓结构;距离直接回答"还能承受多大的价格波动",与压力用例(+30%)同一量纲,告警与日报也更直观。

**只信交易所给的 `liq_px`。** 部分所只在推送里给,统一账户与多资产模式下口径不同(待核实);与 ledger 估算取更保守者,防止单一来源出错。

## Acceptance criteria

- DST:mark 单边上行到强平前,依次出现 L1、L2(事件 + 30 s 截止)、L3(每步 20%,永续先),全程不增加敞口;滞回生效;`MarginReport` 过期 → L1;比率 ≥ 0.8 而距离 > 0.15 → L2。
- hft-sim"单边 +30%(1 h 内)"在默认参数下零强平。
- treasury 对上面例子算出的 L1/L2/L3 涨幅与手算一致(误差 ≤ 0.1 个百分点)。

## Risks

- 默认档位让 L1 在约 +3.5% 就禁开,资金利用率下降;档位待在模拟账户上标定。
- 各所 `liq_px` 口径与维持保证金阶梯待核实;ledger 估算依赖这些数据。
- 强制减仓在逼空中成交不利;这是安全的代价。
