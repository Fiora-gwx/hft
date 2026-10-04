---
description: "首个 ccex 策略(组合携带):每个 group = 永续空腿 + 现货多腿,Model 在 advisor 上做资金费统计、双侧分位、打分与截面分配,快速部分按闸门把分配结果变成目标仓位交给执行层;单 group 与实例级强制退出(被打断后继续)、默认生效的负资金费退出、超配减仓与强制逐步减仓、与杠杆无关的永续硬上限;Model 检查点含原始样本与路径状态两节;规则在模拟账户上研究迭代"
kind: crate
tier: hot
scope: core
status: spec
---

# portfolio-carry

## 概要

`portfolio-carry` 是新框架的第一个 ccex 策略:对一组资产同时持有"永续空 + 现货多"的对冲组合,收资金费与基差回归。每个 group 是一个资产的两腿,永续腿与现货腿在**不同交易所**(不做同所现货 × 永续),具体 venue 由 `GroupSpec` 给出,代码与本文不出现交易所名字,交易所差异只经 `VenueCaps` 等视图读取。结构取自老板的雏形 `Trading/portfolio-arb`:**全部 group 打分 → 截面分配 → 目标仓位 → 带闸门的目标跟踪调仓**。慢速部分(Model,advisor 线程)维护资金费统计、开仓侧与平仓侧各自的 168 h 分位、打分与分配,输出 Advice;快速部分(sequencer 线程)读 Advice 与 `PortfolioView`,过闸门后输出 `TargetIntent`,切片、两腿协调与补腿由 [hft-exec](../../crates/hft-exec/README.md) 完成。它只依赖 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) 与 [hft-strat-lib](../../crates/hft-strat-lib/README.md);名单、流动性分与冷启动预热样本由同目录的 [pipeline 插件包](pipeline/README.md)生产。只做"空永续、多现货"一个方向。

**策略规则尚未确定,在模拟账户上研究迭代。** 全部公式参数化、打分模型可替换;参数默认值取雏形值并标"雏形默认值,待在模拟账户上研究",雏形没有的参数标"暂定";雏形的已知缺陷不照搬([开发备忘](#开发备忘))。

## 目录

- [使用本组件](#使用本组件)
  - [身份](#身份)
  - [group 形态](#group-形态)
  - [资金口径](#资金口径)
  - [订阅](#订阅)
  - [Advice 载荷](#advice-载荷)
  - [预热载荷](#预热载荷)
  - [输出:目标意图](#输出目标意图)
  - [快速部分的闸门](#快速部分的闸门)
  - [强制退出](#强制退出)
  - [负资金费退出](#负资金费退出)
  - [强制逐步减仓](#强制逐步减仓)
  - [保证金压力](#保证金压力)
  - [事件处理表](#事件处理表)
  - [策略命令](#策略命令)
  - [Note 码表与报表字段](#note-码表与报表字段)
  - [参数表](#参数表)
  - [字段属性](#字段属性)
  - [小改属性](#小改属性)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 身份

| 项 | 值 |
|---|---|
| `StrategyDesc.name` | `portfolio-carry` |
| `kind_id` | 1(登记表见 [strategies/registry](../registry/README.md)) |
| `version` | 1 |
| `non_prod_only` | false |
| `schema.id` / `schema.version` | 1 / 1 |
| 类型 | `PortfolioCarry: Strategy<Params = CarryParams, Model = CarryModel>` |

### group 形态

| 项 | 规定 | 校验失败(`ParamError::GroupShape{gix, code}`) |
|---|---|---|
| 腿数 | 2 | code 1 |
| leg 0 | 永续(`InstMeta` 市场类型 `LinearPerp`),目标量 ≤ 0(空) | code 2 |
| leg 1 | 现货(`Spot`),目标量 ≥ 0(多) | code 3 |
| 资产 | 两腿为同一资产(`GroupSpec.asset`),同一 `Qty` 指数 | code 4 |
| 计价 | 两腿计价资产都是账户估值资产(USDT) | code 5 |
| venue | 两腿都属于本实例绑定的账户,且**永续 venue 与现货 venue 属于不同交易所**(`exchange` 不同)。同一账户内 refdata 已保证不出现同所两个市场(`SameExchangeMarkets`),`SetParams` 另有通用校验 `InvalidArgs{SameExchangeLegs}`;本项校验两腿 venue 属于绑定账户 | code 6 |
| inst 唯一 | 同一实例内一个 inst 只出现在一个 group | code 7 |
| `open_allowed` | `false` = 不参与分配,目标为 0,按常规平仓闸门逐步退出(模式门同时挡增仓);持续 `close_only_rounds` 轮仍有仓位时改按[强制逐步减仓](#强制逐步减仓) | — |
| `force_exit`(per-group 参数) | `true` = 该 group 立即按 `Exit` 退出到 0/0,不经分位、成本门与结算前保护,保持为 0 直到改回 `false`(见[强制退出](#强制退出)) | — |

记号:`perp`、`spot` 指两腿的 inst;`q_perp`(有符号,空为负)、`q_spot` 取自 `v.group_pos(a, g)`(净量,已扣除以基础币计的手续费);**对冲量** `h = min(|q_perp|, q_spot)`,**实际名义** `actual = h × spot_mid`。两腿之差由引擎的 LegTracker 与执行层处理,策略只在 `External` 之后主动对齐(见事件处理表)。

**有仓位**(本文凡写"有仓位/无仓位"处都用这个定义):至少一腿同时满足 ① 名义 `|q| × 参考价`(永续 mark、现货 mid)高于尘埃线 `dust_notional`(唯一定义在 [hft-exec](../../crates/hft-exec/README.md),策略经 SDK 视图读取);② `|q|` 不低于该腿的最小下单量(`InstMeta.min_qty`,且 `|q| × 价格 ≥ min_notional`);并且该 group 没有 `MINORDER_RESIDUAL` 标志。0/0 退出以执行层无法再下单的残余结束时(`Stuck{MinOrder}`,或 `Done` 时仍满足 ①②)置该标志并记下两腿净量,任一腿净量变化即清除——所以低于最小下单量的残余不会让强制退出、负资金费退出或 `exit_all` 反复重设目标。

**group 身份**:`GroupId = (StrategyId << 24) | 实例内计数器`,由 pipeline 框架(或 hft-cfg)按同一规则分配,单调、永不复用、全局唯一由构造保证(语义归 [hft-types](../../crates/hft-types/README.md));快速部分与 Model 都以 `GroupId` 为键保存状态。`on_params` 比对每个 group 的 `GroupSpec::fingerprint()`(资产 + 各腿)与已存状态里的指纹:不一致即重置该 group(快速部分回到 `Idle`,清除确认窗口、触发价、封锁、待完成退出、被保留的目标与各标志;Model 见[参数应用与重置](#参数应用与重置))。sequencer 丢弃依据早于该 group 规格变化的 Advice(`spec_seq`,归 [hft-sequencer](../../crates/hft-sequencer/README.md)),所以旧 group 的 Advice 不会作用到新 group 上。

### 资金口径

引擎不计算"可用资金";本策略按自己的参数从 `PortfolioView` 计算(雏形口径 = 永续所保证金余额 + 现货所 USDT + 现货代币按 bid 估值)。两腿在不同交易所,永续 venue 的保证金与现货 venue 的现金是两个独立钱包,按 venue 读取不会重复计算:

| 量 | 定义 | 谁算 |
|---|---|---|
| `E` | `portfolio.equity_total()`(现货按 bid、永续按 mark,保证金余额 = wallet + Σ未实现) | Model、快速部分 |
| `D`(可部署) | `E × deployable_frac` | Model |
| 现货池 `A_s` | 对每个现货 venue `s`:`max(0, cash(s, USDT).free + venue(s).spot_value − spot_cash_reserve)`;若 `Σ A_s > D`,全部按 `D / Σ A_s` 等比缩小 | Model |
| 单币相对上限 | `A_s × max_share`(在池内分配时封顶) | Model |
| 单币上限 `cap_i` | `min(D × max_share, A_s, coin_abs_cap_i)`;受限 group 为 `restricted_cap` | Model |
| 已承诺增量 `committed_spot[s]`、`committed_perp[p]`、`committed_margin[p]` | 不保存,每次增仓评估时从 `v.target(a, g)`(`TargetView`)推导:对本实例除正在评估的 group 以外每个 `origin = Strategy`、`reason = Rebalance`、状态为 `Working`/`Stuck` 的目标,剩余增量 `r_g = max(0, 目标现货量 − legs.leg[1].net)`;`committed_spot[s] = Σ r_g × spot_mid`(现货在 `s` 的 group),`committed_perp[p] = Σ r_g × mark(perp_g)`、`committed_margin[p] = Σ r_g × mark(perp_g) × m_g`(永续在 `p` 的 group,`m_g` 见下行) | 快速部分 |
| 开仓现金约束 | 新增量的名义 ≤ `cash(s, USDT).free − spot_cash_reserve − committed_spot[s]` | 快速部分 |
| 开仓保证金约束 | 新增量的名义 × `m_g` ≤ `margin(p).avail_margin − committed_margin[p]`;`m_g = max(perp_margin_frac, 1 / L_g)`,`L_g` = `Views::leverage(a, perp_g)`(该 group 永续 inst 的实际杠杆,百分之一倍);读不到 → 不开仓并写 `Note{MarginShort}`(v3 = 2) | 快速部分 |
| **永续硬上限**(与杠杆无关) | 每个永续 venue `p`:`Σ \|q_perp_i\| × mark_i + committed_perp[p] + 新增量的名义 ≤ margin(p).margin_balance / perp_min_equity_ratio`;求和覆盖本实例永续腿在 `p` 的全部 inst 的净持仓(`v.portfolio(a).perp(p, i)`,含该 inst 上不归属 group 的持仓),并计入其他 group 已承诺未成交的增量,多个 group 同时开仓也不会短时越过上限 | 快速部分 |
| 软告警 | `venue(p).margin_balance / E < perp_min_share`;任一 group `actual / E > concentration_alert_share` | 快速部分(`Note`) |

资金不足或触到硬上限时把可增量截断;截断后 < `slice_min_chunk` → 不设目标、写 `Note{CashShort|MarginShort|PerpEquityCap}`(限频)。划转建议由 treasury 给出,策略不划转。`committed_*` 由目标视图推导,不进快照,不会因漏减而永久挡开仓。

**永续腿的安全由什么决定**:开仓保证金约束按 `avail_margin` 的增量判断,而交易所对每单位永续名义只占用 `1/L` 的初始保证金;反复开仓时可增量一直为正,直到永续名义 ≈ `L × 保证金余额`,即开满时 `e = 保证金余额 / 永续名义 ≈ 1/L`。所以 `perp_margin_frac` 只影响每一次可增量的估算(偏小 → 第一腿或链接腿被风控 `LINK_FUNDS` 挡,偏大 → 少开),**不决定开满后的安全程度**;决定安全的是实际杠杆 `L` 与永续 venue 上的资金量。为此有两道与 `perp_margin_frac` 无关的约束:

1. **硬上限**(上表):开满时 `e ≥ perp_min_equity_ratio`(默认 0.40,待标定),不论交易所杠杆设成多少;
2. **实际杠杆上限**:[hft-risk](../../crates/hft-risk/README.md) 用 `AccountConfigReport` 读到的实际杠杆核验 `RiskParams.max_perp_leverage_x100`(默认 300 = 3x),任一交易中的永续 inst 超过 → `ACCOUNT_CFG` 不符(禁开)+ P1;本策略经 `Views::leverage` 读同一值。

推荐的交易所杠杆是 2x(设置步骤在 [launch](../../docs/runbooks/launch.md)):开满时 `e ≈ 0.5`,由交易所占用决定,硬上限不起作用;杠杆为 3x 时开满由硬上限限在 `e = 0.40`。两种情形下的保证金分档估算见[保证金压力](#保证金压力)。

### 订阅

| 内容 | `EvMask` | `Part` |
|---|---|---|
| `perp` | `BBO \| FUNDING \| MARK` | `Both` |
| `spot` | `BBO` | `Both` |
| 模型 tick | `model_tick(alloc_period_ms)` | Model |
| 预热注入 | 不调用 `warmup`(默认只投递给 Model) | Model |

本策略不订阅深度(快速部分与 Model 都不订阅;首次上线的 core 行情不含深度,范围归 [hft-venue](../../crates/hft-venue/README.md))。切片的可见深度由执行层读:该 inst 有 TopN 深度簿(由其他订阅者维持)时读簿,否则用 BBO 顶档;`slice_depth_levels` 默认 1,只看顶档(规则归 [hft-exec](../../crates/hft-exec/README.md))。打分与分配不使用深度;按深度估算冲击是可选扩展:由 Model 订阅 TopN 深度,触发条件是模拟账户报告显示按顶档估算的冲击明显偏离实际成交。

### Advice 载荷

载荷首字节为版本号;全部字段为整数定点。记号:`F` = `FundingSummary.realized_daily`(`funding_window_ms` 窗口内已结算费率折成每日,[hft-strat-lib](../../crates/hft-strat-lib/README.md));`T` = 同一窗口内已结算费率之和(`realized_sum`)。凡是"每日"的量一律用 `F`,不把窗口合计当每日。

**`local_kind = 1`:`GroupAdvice`(每 group,`ver = 1`)**

| 字段 | 类型 | 含义 |
|---|---|---|
| `flags` | u16 | `READY`(分位与资金费统计都就绪)、`PCT_NOT_READY`、`FUNDING_STALE`、`ENTRY_BLOCKED`(平滑后的开仓侧基差 < `min_entry_basis`)、`RESTRICTED`、`HOLD`(未就绪但有仓位:维持现状)、`INTERVAL_CHANGED`、`WARM_APPROX`(窗口内仍有以成交价 K 线近似的预热样本)、`FUNDING_TRUNC`(`FundingSummary.flags` 含 `TRUNCATED`:窗口内的结算没有全部留在资金费环里,见 Model 第 2 步)、`NEG_FUNDING`(负资金费退出条件成立) |
| `target` | Notional | 平滑并经池预算缩放后的目标名义(≥ 0) |
| `weight` | Rate | 分配份额(占所在现货池) |
| `cap` | Notional | 该 group 的单币上限 `cap_i`(快速部分的超配判定用它) |
| `kick` | Notional | 基差加成部分 |
| `open_th` | Rate | 开仓侧分布在 `p_req` 处的基差值 |
| `close_th` | Rate | 平仓侧分布在 `p_exit_used` 处的基差值 |
| `bonus_close_th` | Rate | 平仓侧分布在 `bonus_close_pct` 处的基差值 |
| `p_req`、`p_exit_used` | Rate | 实际使用的入场分位(含资金费下沉)与退出分位 |
| `funding_now_norm` | Rate | 当前费率折算到 `funding_norm_ms` 周期 |
| `funding_daily`、`funding_min` | Rate | `F`;窗口内单期最差费率 |
| `pos_periods`、`total_periods` | u16 | 窗口内正费率期数、总期数 |
| `interval_s` | u32 | 实际结算周期 |
| `open_daily` | Rate | 开仓机会(每日)= `max(F_i, 0)` |
| `close_gain_daily` | Rate | 平仓收益(每日)= `max(0, max(best_other_F_i, 0) − F_i)`;`best_other_F_i` = **同一现货池**活跃集中其他 group 的 `F` 最大值,池内除本 group 外活跃集为空时 = 0。第一项是同池换仓机会,第二项是平掉后不再支付的负资金费 |
| `exit_score` | Rate | 退出分(`exit_score_min = 0` 时恒为 1.0) |
| `funding_score`、`basis_score`、`liq_score`、`entry_score` | Rate | 打分分项(诊断与研究) |
| `n_open`、`n_close` | u32 | 两侧分位样本数 |

**`local_kind = 2`:`PortfolioAdvice`(`GROUP_NONE`,`ver = 1`)**:`equity`(E)、`deployable`(D)、`n_ready`、`n_active`、`n_warmed`(已应用过预热样本的 group 数)、`sum_target`、`pools: [(venue, A_s); 4]`、`pool_scale`(Rate)、`flags`(`CONCENTRATION`、`PERP_SHARE_LOW`、`WARMUP_DONE`(收到过 `last = true` 的基差与资金费预热))。

发送策略(每个模型 tick 计算一次):

1. **变化即发**:与该 group 上次发送的摘要(`target`、`open_th`、`close_th`、`bonus_close_th`、`flags`)相比,`target` 相对变化 ≥ `advice_min_change`,或其余任一项变化 → 发送该 group;`PortfolioAdvice` 的 `flags` 或 `n_*` 变化 → 发送。
2. **心跳(UTC 对齐)**:`now.utc_ns` 越过 `advice_refresh_ms` 的整数倍边界(自 UTC 纪元起)后的第一个模型 tick,发全部 group 与 `PortfolioAdvice`。
3. **重置后**:group 被重置(新加入、指纹变化、结构参数)或检查点路径节被丢弃后的第一个模型 tick,发这些 group。

上次发送的摘要与心跳时刻在 Model 检查点的路径状态节里,所以从检查点恢复后的发送序列与连续运行逐字节相同([Model 检查点](#model-检查点))。快速部分的过期判定([SDK Advice 载荷](../../crates/hft-strategy-sdk/README.md#advice-载荷)):`basis_seq < v.now().boot_seq`(依据上次启动的数据)→ 过期;否则 `v.now().tsc − basis_tsc > advice_max_age_ms` → 过期。advisor 落后或重启追赶期间的 Advice 因此不会被当成新的。

### 预热载荷

Model 的分位(168 h)与资金费统计依赖历史样本。group 没有可用的统计时(实例首次启动、`LAYOUT_VERSION` 提升、检查点被丢弃、新加入的 group、被重置的 group,见[参数应用与重置](#参数应用与重置)),由本策略的 [pipeline 插件包](pipeline/README.md)经 `InjectWarmup`([SDK 预热注入](../../crates/hft-strategy-sdk/README.md#预热注入))送入历史样本,不必等实时样本攒满。载荷首字节为版本号,小端,全部整数定点;本 crate(`src/warmup.rs`)与插件共用金样 `fixtures/strategies/portfolio-carry/warmup/`。

**`local_kind = 1`:`BasisWarmup`(每块一个 group,`ver = 1`)**

| 偏移 | 字段 | 类型 | 含义 |
|---|---|---|---|
| 0 | `ver` | u8 | 1 |
| 1 | `src` | u8 | 1 = 两侧报价(collector BBO 或 journal 归档,按 `pct_sample_ms` 采样);2 = 成交价 K 线(两侧取同一价,近似) |
| 2 | `_pad` | u16 | 0 |
| 4 | `group` | `GroupId`(u32) | 目标 group;不在当前参数中 → 整块忽略并计数 |
| 8 | `start_ns` | i64 | 第一个样本的 UTC 时刻,按 `step_ms` 对齐 |
| 16 | `step_ms` | u32 | 样本间隔:`pct_sample_ms` 的整数倍且 ≤ `warmup_max_step_ms`,否则整块忽略 |
| 20 | `n` | u16 | 样本数,≤ 509 |
| 22 | `_pad2` | u16 | 0 |
| 24 | `rows[n]` | `{open: i32, close: i32}` | 开仓侧基差 `(perp_bid − spot_ask)/spot_ask` 与平仓侧基差 `(perp_ask − spot_bid)/spot_bid`,`Rate`(1e-9)存为 i32(覆盖 ±214%);`i32::MIN` = 缺失 |

**`local_kind = 2`:`FundingWarmup`(每块一个 group,`ver = 1`)**

| 偏移 | 字段 | 类型 | 含义 |
|---|---|---|---|
| 0 | `ver` | u8 | 1 |
| 1 | `_pad` | [u8; 3] | 0 |
| 4 | `group` | `GroupId`(u32) | 目标 group(永续腿的资金费) |
| 8 | `n` | u16 | 记录数,≤ 170 |
| 10 | `_pad2` | [u8; 6] | 0 |
| 16 | `rows[n]` | `{at_ns: i64, rate: i64 (Rate), interval_s: u32, _pad: u32}` | 交易所资金费历史的逐期结算记录 |

- **应用**(Model,幂等、与先后无关):`BasisWarmup` → `pct.prefill(行, fill_ms = step_ms, now.utc_ns)`,只补空的采样点、实时样本优先;`src = 2` 的块把 `kline_until_ns` 推到该块末尾,窗口内仍有这类样本时 `GroupAdvice` 置 `WARM_APPROX`。`FundingWarmup` → `fund.prefill_settled(行)`(每行取 `at_ns`、`rate` 构成 `SettleRec`;`interval_s` 不进资金费环,周期只由实时 `Funding` 更新,见 hft-strat-lib `FundingStats` 预热)。收到 `last = true` 后 `PortfolioAdvice` 置 `WARMUP_DONE`。基差平滑窗与目标平滑窗很短,不预热。
- **数据量**:满窗(168 h / 5 s)每 group 120 960 个样本 ≈ 238 块;64 group ≈ 1.5 万块、约 59.5 MiB(62.4 MB)journal Input 载荷。只为达到就绪覆盖(`pct_min_coverage` 0.25,即 42 h)≈ 3 800 块、约 15 MiB;1 分钟 K 线满窗每 group 20 块。只为新加入或被重置的 group 预热时按 group 数等比减少。注入量由插件按来源与覆盖目标决定。
- **效果**:注入覆盖 ≥ `pct_min_coverage` 的样本且资金费历史到达后,group 在下一个模型 tick 即可就绪,无需约 42 h 的实时积累;此后的检查点包含预热结果,普通重启不必再注入。
- **触发**:实例首次启动、`Note{ModelCheckpointDiscarded}`、`Note{ModelGroupReset}`(列出的 group,含新加入的 group)之后自动,或人工;只丢路径节的 `Note{ModelPathReset}` 不需要预热(原始样本仍在)。触发规则归 [pipeline 框架](../../python/pipeline/README.md),数据来源选择归插件。

### 输出:目标意图

目标两腿始终为 `legs = [(0, −q), (1, +q)]`,`q` = 目标名义按 `spot_mid` 折算的资产量(hft-types 名义↔数量换算,向零取整);`TargetIntent.band = band`(执行层的带分母下限用 hft-exec 的 `band_floor_notional`;本策略的 `band_floor` 只用于下文闸门 5 与目标重设判定)。`PairParams`:`slice` 取 `slice_*` 参数(`depth_levels = slice_depth_levels`),`hedge_px_mode = Cross`、`hedge_cross`、`hedge_max_slip`,`ratio = 1/1`,`fix_tolerance`、`residual` 取同名参数。`tag = 场景码 << 8 | 序号`:场景码见下表,序号是该 group 的 `tag_seq`(u8,回绕;见"目标的设与留")。

| 场景码 | 场景 | `reason` | `urgency` | `exec` | 适用闸门 |
|---|---|---|---|---|---|
| 1 | 常规增仓(desired > actual) | `Rebalance` | `Normal` | `PairOnFill{first_leg = open_first_leg}`,切片参数见 `slice_*` | 全部开仓闸门(含开仓成本门) |
| 2 | 常规减仓(desired < actual) | `Rebalance` | `Normal` | `PairOnFill{first_leg = close_first_leg}` | 全部平仓闸门(含平仓成本门) |
| 3 | 超配减仓(只减到 `cap`) | `Rebalance` | `Normal` | `PairOnFill{close_first_leg}` | 只有平仓成本门 |
| 4 | [强制逐步减仓](#强制逐步减仓) | `Rebalance` | `Normal` | `PairOnFill{close_first_leg}` | 持续 `close_only_rounds` 轮的条件;不经成本门 |
| 5 | 下架退出 | `Exit` | 距下架 > `delist_urgent_before_ms` 为 `Normal`,否则 `Urgent` | `PairOnFill{close_first_leg}` | 无 |
| 6 | 强制退出(`force_exit`、`exit`、`exit_all`) | `Exit` | 参数:`Normal`;命令:按命令字段 `urgent` | `PairOnFill{close_first_leg}` | 无 |
| 7 | 负资金费退出 | `Exit` | `Normal` | `PairOnFill{close_first_leg}` | 无 |
| 8 | 外部成交后的处理 | `Hard` | `Urgent` | `PairOnFill`(执行层强制 `PerpFirst`);只需改一腿时 `TakerSlice` | 无 |
| 9 | 保证金压力减仓 | `MarginStress` | `Urgent` | `PairOnFill`(执行层强制 `PerpFirst`) | 无 |

增仓目标的 `q` 先受[资金口径](#资金口径)的现金、保证金与永续硬上限截断:可增量 < `slice_min_chunk` 时不设目标。**成本门只对场景 1–3 生效**;强制逐步减仓与 `Exit`、`Hard`、`MarginStress` 不经过分位、成本门与结算前保护,所以风险退出、规则退出、人工退出与"只平不开"不会被行情条件无限期挡住。报价是否可用由执行层按连接健康判定([SDK 报价健康](../../crates/hft-strategy-sdk/README.md#视图类型)),策略不为这几类目标另设行情门。两腿 lot 对齐失败(`LotMismatch`)只拒增仓方向的配对目标;减仓方向与 `Exit`/`Hard`/`MarginStress` 按较粗的 lot 取整执行,残余交执行层的"残余与尘埃"规则([hft-exec](../../crates/hft-exec/README.md))。

### 快速部分的闸门

每次评估一个 group(触发:该 group 任一腿的 `Bbo`、该 group 的 `GroupAdvice`、永续腿的 `Funding`、`Exec{Done|Stuck}`、`Mode`、定时器轮询),按以下顺序,任一不通过即停止。

**目标的设与留**:执行层每次接受目标都会替换旧目标、清零 `filled` 并写 `ExecDecision`,逐字段相同的目标视为已接受、不替换([hft-exec 目标的接受与替换](../../crates/hft-exec/README.md#目标的接受与替换)),所以快速部分只在三种情况下调用 `ctx.target`:该 group 没有本策略的生效目标;新目标的 `reason` 或场景码与生效目标不同;新目标量与生效目标量之差超出不交易带(`|q_new − q_cur| × spot_mid / max(|q_cur| × spot_mid, band_floor) > band`)。已有同方向的常规 `Rebalance` 目标时,每次评估用**即时条件**复查(分位条件不再要求 `persist_ms` 持续确认,其余闸门照常):复查不通过 → `clear_target`;通过且目标量在带内 → 保留,不重设。**重发**:被保留的 `Exit`/`Hard`/`MarginStress` 目标在执行受阻后重发时 `GroupState.tag_seq += 1`(回绕,从不清零),重发的目标因此至少 `tag` 与生效目标不同,执行层按替换处理并重置失败计数(hft-exec "重置失败计数必须改字段");其余情况沿用当前序号。只有 `urgency` 变化(L2 与下架临近的升级)不在上述三种情况内,是否据此重设,策略研究后定。

1. **状态与退出**:
   1. `force_exit = true` 且阶段不是 `Exit`/`Broken` → 进入 `Exit{Param}`(阶段为 `Trim`、`Stress`、`Fix`(含 `Fix{Manual}`)、`Overridden` 时同样适用;见[强制退出](#强制退出))。
   2. 负资金费退出条件成立(`neg_funding_exit` 为真且 `neg_streak ≥ neg_funding_exit_periods`)、有仓位且阶段不是 `Exit`/`Broken`/`Fix{Manual}` → 进入 `Exit{NegFunding}`(见[负资金费退出](#负资金费退出))。
   3. 阶段 `Trim`/`Exit`/`Fix`/`Stress`/`Overridden`/`Broken` → 不做常规调仓(由对应流程处理)。
2. **Advice**:有 `GroupAdvice` 且未过期(`basis_seq ≥ now.boot_seq` 且 `now.tsc − basis_tsc ≤ advice_max_age_ms`,见 [Advice 载荷](#advice-载荷));`flags` 无 `READY`:有 `HOLD` → 不设常规目标(只做第 4 步的超配减仓),否则 desired = 0 但**不开仓**。
3. **数据健康**:两腿报价健康——`v.quote_ok(a, perp)` 与 `v.quote_ok(a, spot)` 为真(SDK 的唯一判定,引擎阈值 `RiskStateView.conn_liveness_ms`:连接 Live、非重同步、连接静默不超过阈值;**盘口长时间不变不算过期**);两腿所需一侧价格存在;`funding(perp).upd_tsc` 距今 ≤ `funding_max_age_ms`(资金费新鲜度门)。
4. **超配**:`actual > cap × (1 + overweight_tol)`(`cap` 取自 `GroupAdvice`)→ 本次评估标记"超配",`desired ← min(desired, cap)`(`HOLD` 时 desired 取 `cap`)。
5. **不交易带**:`delta = desired − actual`;`|delta| / max(desired, band_floor) ≤ band` 且本次未标记超配 → 不动(超配已有自己的容差 `overweight_tol`,不再叠加不交易带)。
6. **方向 = 开仓(delta > 0)**(`HOLD` 时不开仓):
   1. `v.mode(a, g) == Live` 且 `open_allowed` 且 `!force_exit`;无 `ENTRY_BLOCKED`;`now.engine_ns ≥ reentry_block_until_ens` 且 `≥ fail_until_ens`;负资金费退出条件不成立;`now.utc_ns` 不在维护屏蔽窗口内(`now.clock_ok = false` 且有维护窗口登记时视为在窗口内)。
   2. **负基差底线**:实时开仓侧基差 `ob = (perp_bid − spot_ask) / spot_ask ≥ min_entry_basis`。
   3. **入场分位**:`ob ≥ open_th`;设新目标时还须经 `ConfirmWindow{min_ms = persist_ms}` 持续确认,复查已有目标时用即时条件(只有分位条件需要持续,其余条件即时判断)。
   4. **结算前保护**:距下次结算 `< settle_protect_ms` 且 `funding.rate < 0` → 等结算后再开(`now.clock_ok = false` 时视为在窗口内)。
   5. **开仓成本门**(见下式),`gain_daily = open_daily`。
   6. **资金**:按[资金口径](#资金口径)截断可增量(现金、保证金、永续硬上限),< `slice_min_chunk` → 停止并 `Note`。
7. **方向 = 平仓(delta < 0)**:
   1. **常规减仓**(`HOLD` 时跳过):`now.engine_ns ≥ fail_until_ens`;**退出分位**:实时平仓侧基差 `cb = (perp_ask − spot_bid) / spot_bid ≤ close_th`(设新目标时经 `ConfirmWindow{persist_ms}` 持续确认,复查时即时);**退出分**:`exit_score ≥ exit_score_min`(为 0 时关闭);**结算前保护**:距下次结算 `< settle_protect_ms` 且 `funding.rate > settle_protect_rate` → 先收这期资金费;**平仓成本门**,`gain_daily = close_gain_daily`(含负资金费项:本 group 资金费为负时,即使同池没有可换的币,平仓收益也 ≥ 每日所付资金费);若 `cb ≤ bonus_close_th` 且 `group_pos.flags` 无 `ENTRY_ADOPTED`,`one_shot = ((perp_entry − perp_ask) + (spot_bid − spot_entry)) / spot_mid`(> 0 才计,否则 0)。全部通过 → 目标 = desired。
   2. **超配减仓**(常规减仓未通过且本次标记超配):`now.engine_ns ≥ fail_until_ens`;只过**平仓成本门**(`gain_daily`、`one_shot` 同上;不经退出分位、退出分与结算前保护)→ 目标 = `cap`(只减掉超配部分)。
8. 通过 → 按"目标的设与留"决定是否 `ctx.target(...)`(`reason = Rebalance`);同一定时器轮询中先处理减仓 group、后处理增仓 group(平仓先于开仓,先释放资金)。

**成本门**(只用于场景 1–3;全部为 `Rate`,i128 中间量;`1天` = 86 400 000 ms):

```
spread_x   = (ask_x − bid_x) / mid_x                                       // x ∈ {perp, spot}
open_cost  = 2 × (fee(perp).taker + fee(spot).taker)                      // 开仓:开 + 平两腿手续费
           + (spread_perp + spread_spot) / 2
close_cost = fee(perp).taker + fee(spot).taker                            // 平仓:只算平仓两腿(开仓费已沉没)
           + (spread_perp + spread_spot) / 2
开仓通过 ⇔ open_cost × 1天 ≤ max_break_even_ms × gain_daily                // 回本时间 ≤ 上限;两边量纲同为 Rate·ms
        且 ¬(gain_daily × cost_multiplier < open_cost 且 gain_daily < open_cost)   // 雏形的倍数条件,只用于开仓
平仓通过 ⇔ max(0, close_cost − one_shot) × 1天 ≤ max_break_even_ms × gain_daily   // 一次性收益先抵成本;无倍数条件
```

倍数条件里 `gain_daily × cost_multiplier` 读作"`cost_multiplier` 天的收益",与 `open_cost` 同为 `Rate`;`cost_multiplier ≥ 1` 时它等价于"回本 ≤ `cost_multiplier` 天",与第一条取更严者。本门**没有参数开关可绕过**:新开仓、目标降为 0 的常规减仓(含 `open_allowed = false` 的逐步退出)、超配减仓同样要过;不经过它的只有 `Exit`/`Hard`/`MarginStress` 目标,以及条件持续 `close_only_rounds` 轮之后的[强制逐步减仓](#强制逐步减仓)。按雏形默认值(`cost_multiplier = 2`、回本 ≤ 5 天),开仓实际受倍数条件约束(每日资金费 ≥ 往返成本的一半),这一点留待模拟账户研究。换仓时目标币的开仓成本由该币自己的开仓成本门覆盖,不计入平仓方向。

### 强制退出

退出的目标一律是 0/0、`reason = Exit`、不经分位、成本门与结算前保护,阶段 `Exit{src}`,直到执行层报 `Exec{Done}`(或剩余一腿低于交易所最小下单量,见事件处理表 `Stuck(MinOrder)`)。

| 入口 | 范围 | 类别 | 持续 | 完成后 |
|---|---|---|---|---|
| per-group 参数 `force_exit = true`(`SetParams`,人工专用字段,按实例分开) | group | 参数 | 只要为 `true` 就保持 0/0;再次有仓位(外部原因)时再设 0 | 改回 `false` 后恢复常规评估(不另加封锁) |
| 策略命令 `exit`(`reduce_only = true`,中性命令,DRAINING、CloseOnly、闸门未放行时也可发) | group | 命令 | 一次性:退出到 0/0 | `reentry_block_until_ens = now.engine_ns + reentry_block_ms`(命令字段,默认 24 h),期间开仓闸门 6.1 不放行;`clear_block` 命令可提前解除 |
| 策略命令 `exit_all`(同上,`group = GROUP_NONE`) | 实例全部 group | 命令 | 一次性:每个有仓位的 group 退出到 0/0;超出 `Ctx` 目标容量(64)的部分由 `Timer{SWEEP}` 续发;`Broken` 的 group 只设封锁(执行层拒绝 `CellBroken`;是否同时记待完成退出,策略研究后定) | 每个 group 同 `exit` |
| 负资金费退出(默认生效,见下节) | group | 规则 | 一次性 | 封锁 `reentry_block_ms`(参数);负资金费条件复位前开仓闸门 6.1 不放行 |
| 下架通知 `Notice{Delisting}` | group | 规则 | 一次性 | 置 `DELIST_DONE`,该 group 不再开仓(直到参数移除或下架撤销) |

- 命令处理期间产生的只有减仓目标(两腿每腿更接近 0),满足 SDK `reduce_only` 契约;无仓位的 group 只设封锁。
- **待完成退出**(`GroupState.pend_exit`,进快照):进入 `Exit{src}` 时记下来源、`urgent` 与封锁时长(命令字段或参数),**只在退出完成或来源撤销时清除**(`force_exit` 改回 `false`、`Risk{CapCleared{Delisting}}`);`clear_block` 不清它。**任何阶段回到 `Idle` 之前先查它**:有待完成退出且有仓位 → 回到 `Exit{src}`,按保存的 `urgent` 重设 0/0;已无仓位 → 按上表"完成后"一列结束(设封锁、`DELIST_DONE` 等)再回 `Idle`。所以被 L2 减仓、人工冻结、L3 接管或 Broken 打断的退出(含下架退出)在打断结束后继续;人工冻结期间被人在交易所平掉的仓位按退出完成处理并设封锁,不会被策略重新开回。
- **L2 期间**:有待完成退出的 group 不进入 `Stress`,保持 0/0 `Exit`,只把 `urgency` 升为 `Urgent`(升级后保持到完成)。
- **CloseOnly 下持有**:有效模式降为 `CloseOnly`(人工 `SetMode`、cap 等)时,本策略不开新仓、清除进行中的增仓 `Rebalance` 目标,**不主动退出**:已有仓位保持,常规减仓、超配减仓、强制逐步减仓、风险类与 `Exit` 照常。需要全部退出时用 `exit_all`。
- **退出目标不因执行受阻而丢失**:`Exit`(以及 `Hard`、`MarginStress`)目标在执行层 `Stuck` 时保留,`fail_cooldown_ms` 后以保存的字段与新的 `tag` 序号重发(见"目标的设与留")并写 `Note{ExitRetry}`(P1),见事件处理表。

### 负资金费退出

默认生效的规则退出:`neg_funding_exit`(默认 `true`)为真时,永续腿连续 `neg_funding_exit_periods`(默认 3)次结算的**实收资金费率** < 0(空永续在费率 < 0 时付资金费)→ 该 group 目标 0/0、`reason = Exit`、不经成本门,`Note{NegFundingExit}`。N 的取值在模拟阶段研究。

- **计数**(快速部分,`neg_streak`):每条永续腿的 `Funding` 事件按与 [hft-strat-lib `FundingStats`](../../crates/hft-strat-lib/README.md) 相同的结算检测规则判断是否发生了一次结算(`next_settle_ns` 前移;该期费率取跳变前最后一次观测的费率;跳变 ≥ 2 个周期只计一次);该期费率 < 0 → `neg_streak += 1`,≥ 0 → `neg_streak = 0`。`neg_streak`、最近一次观测的费率与 `next_settle_ns`(UTC)进快照,停机期间漏掉的结算在重启后的第一条 `Funding` 上按一次计。`neg_funding_exit = false` 时照常计数,只是不触发退出。
- **退出**:条件成立且有仓位 → `Exit{NegFunding}`(闸门 1.2;`Funding` 事件与 `Timer{SWEEP}` 都会复查,所以重启后照常触发);`Exec{Done}` 后 `reentry_block_until_ens = now.engine_ns + reentry_block_ms`。条件成立期间开仓闸门 6.1 不放行,直到出现一期非负结算;`clear_block` 只清封锁,不清 `neg_streak`。
- **Model 侧**:Model 按同一检测规则从资金费环得到 `FundingSummary.neg_streak`,条件成立的 group 置 `NEG_FUNDING`、不进入活跃集(不分配资金)。退出由快速部分执行,不依赖 Advice 是否新鲜。
- 已实现费率来源的核实见开发备忘 3;该来源改变时本节的计数口径随之改变。

### 强制逐步减仓

超配减仓与 `open_allowed = false` 的逐步退出都只经平仓成本门,资金费在本池最高、基差又未回落的币(`close_gain_daily = 0`,没有一次性收益)会一直减不下来。下列情形持续 `close_only_rounds` 轮之后,快速部分改为不受成本门约束的逐步减仓(阶段 `Trim{cause}`,场景码 4):

| `cause` | 条件(每轮检查) | 每步目标 | 结束 |
|---|---|---|---|
| `OpenDisallowed` | `open_allowed = false` 且有仓位 | 当前对冲量 `h × (1 − stress_reduce_frac)`;剩余名义 < `max(min_target, slice_min_chunk)` 时取 0/0 | 无仓位,或 `open_allowed` 改回 `true` |
| `Overweight` | 有未过期的 `GroupAdvice` 且 `actual > cap × (1 + 2 × overweight_tol)` | `max(cap, actual × (1 − stress_reduce_frac))` 折成数量 | `actual ≤ cap × (1 + overweight_tol)`,或 Advice 过期/缺失 |

- **轮**:每 `stress_step_ms` 一轮,由 `Timer{SWEEP}` 在该 group 的 `trim.next_ens` 到期后检查。阶段为 `Idle`/`Rebalancing` 时条件成立 → `trim.rounds += 1`,不成立 → 清零;`trim.rounds ≥ close_only_rounds` → 进入 `Trim{cause}` 并发第一步,此后每轮一步(基准为当时的实际量,与 L2 减仓同一节奏)。两种条件同时成立取 `OpenDisallowed`。
- **目标**:`reason = Rebalance`、`Normal`、`PairOnFill{close_first_leg}`;不经分位、退出分、结算前保护与成本门;仍受 `fail_until_ens` 退避,报价可用与否由执行层判定。已有本策略更接近 0 的减仓目标时不替换。每步写 `Note{TrimStep}`。
- 阶段 `Trim` 中不做常规调仓;`force_exit`、负资金费退出、L2 减仓、外部成交与 Broken 照常优先(转移同其他阶段)。结束条件成立 → 回 `Idle`(先查待完成退出),`trim.rounds` 清零。
- 默认参数(`close_only_rounds` 3、`stress_step_ms` 10 s、`stress_reduce_frac` 0.25)下,条件持续 30 s 后开始,约 11 步(≈ 2 min)减到原对冲量的 5% 以下;代价是平仓成本可能高于收益。节奏与 L2 共用参数,是否另设见开发备忘 10。已知不一致:每步目标与生效目标之差可能落在不交易带 `band` 内("目标的设与留")而不重设,默认参数下第一步之后可能停住;`open_allowed = false` 由 pipeline 一轮过滤即可置位,与这里 30 s 的开始节奏不匹配。两者策略研究后定。

### 保证金压力

档位由引擎按每 (account, venue) 的到强平距离 `liq_dist` 判定(阈值、滞回与比率交叉校验归 [hft-risk](../../crates/hft-risk/README.md)):L1 禁开,L2 通知策略减仓并给截止,L3 或截止已过由引擎强制减仓(永续先、按步),数据过期至少 L1。本策略的响应只针对**永续腿在该 venue 上**的 group(现货 venue 没有强平):

| 档位 | 本策略 |
|---|---|
| L1 | 清除本实例全部增仓目标(账户的 Open 已被 cap 挡);不新设增仓 |
| L2 | 阶段 `Stress`:按 `entry_score` 从低到高,每 group 目标 = 当前对冲量 × (1 − `stress_reduce_frac`)(`MarginStress`、`Urgent`);此后每 `stress_step_ms` 再减一轮,直到该 venue 降到 L2 以下或升到 L3;首轮须在 `deadline_tsc` 前发完(超过 `Ctx` 目标容量的部分用定时器续发)。有待完成退出的 group 不进入 `Stress`,保持 0/0 `Exit` 并升为 `Urgent`(见[强制退出](#强制退出));`Fix{Manual}` 冻结的 group 不发目标。按默认参数每 group 一轮 ≤ `0.25 × coin_abs_cap` = 2 500 USDT ≈ 5 片,能否在 `deleverage_deadline_ms` 内成交由压力用例核实 |
| L3 | 引擎接管:各 group 收到 `Exec{Stuck(EngineOverride)}` → 阶段 `Overridden`,只允许每腿更接近 0 的目标;有待完成退出的 group 立即重设 0/0 `Exit`(执行层接受更接近 0 的目标并替换引擎目标) |
| 回落 | 该 venue 的 `MarginStress{level}` 降到 L2 以下 → 永续腿在该 venue 的 group 退出 `Stress`(停止续减,回 `Idle` 前先查待完成退出);`CapCleared{MarginStress}`(账户回到 L0)→ 清全部 venue 的压力状态;`Timer{SWEEP}` 另以 `v.risk(a).margin_level < L2` 兜底退出 `Stress`(不依赖每 venue 回到 L0 的事件) |

**估算**(`liq_dist ≈ (e − m − (1 + m)·y) / ((1 + m)(1 + y))`,`e` = 保证金余额 / 永续名义,`m` = 维持保证金率 0.5%–2.5%,`y` = 单边涨幅;以 hft-sim 压力用例为准):

| 开满时 | L1(≤ 0.25) | L2(≤ 0.15) | L3(≤ 0.10) | 不减仓时强平 |
|---|---|---|---|---|
| `e ≈ 0.5`(交易所杠杆 2x) | +17%–19% | +27%–30% | +33%–36% | +46%–49% |
| `e = 0.40`(3x,由硬上限限住) | +9%–11% | +19%–21% | +24%–27% | +37%–39% |

**压力用例**(定义与行情变换归 [hft-sim 压力用例](../../crates/hft-sim/README.md#压力用例上线门槛必过项),门槛与计时规则归 [testing §8 ⑦](../../docs/testing.md#8-上线门槛)),一律以拟上线的最终参数运行,交易 inst 的分档维持保证金率必须在 refdata 中(缺失 → 用例拒绝运行):

| 用例 | 性质 | 要求 |
|---|---|---|
| 单边 +30%(1 h 内),交易所杠杆 2x、`deployable_frac` 0.6 开满([MS-2a](../../docs/tests/scenarios.md#ms-2a-每个上线策略以最终参数运行-one-sided-30-时零强平)) | 门槛 | 零强平;本策略在 L2 期间的减仓目标先于引擎 L3 发出 |
| L3 路径变体:实际杠杆取 `max_perp_leverage_x100` 上限(`e` 由硬上限限在 0.40)下单边 +30%,或 2x 下 1 h 内 +45%(testing MS-4) | 必过 | 依次出现 L1、L2、L3,零强平;L3 期间本策略只发每腿更接近 0 的目标 |
| 基差扩大、15 min 内 +30%、上涨中 gw 断开 10 min、盘口量 ×0.2、上涨中 hft-core 重启、只有永续所单独暴涨(`StressScope::Venue`) | 非门槛 | 结果写进模拟报告,不过时说明原因 |

### 事件处理表

| 事件 | 动作 |
|---|---|
| `Advice{GroupAdvice}` | 解码、缓存(`Expiring::put_advice`,记 `basis_seq`/`basis_tsc`);评估该 group |
| `Advice{PortfolioAdvice}` | 缓存;`CONCENTRATION`/`PERP_SHARE_LOW` → `Note`(限频);`WARMUP_DONE` 或 `n_warmed` 变化 → `Note{WarmupStatus}` |
| `Bbo`(任一腿) | 重算另一侧触发价与本侧比较(`TriggerPx` 思路);推进开/平确认窗口;评估该 group(闸门 1–8);`force_exit` 或 `Exit` 阶段的 group 若有仓位且无生效目标,重设 0/0 |
| `Funding`(永续腿) | 更新新鲜度;结算检测与 `neg_streak`([负资金费退出](#负资金费退出));条件成立 → 评估(闸门 1.2);`interval_s ≠ prev_interval_s` → `Note{FundingInterval}`(统计口径由 Model 处理) |
| `Exec{Progress}` | 不改阶段(含 Broken 阶段以 `origin = Engine(..)` 到达的引擎成交);有成交时 `held.retries` 清零(`tag_seq` 不清) |
| `Exec{Done}` | 按阶段:增减仓 → `Idle`;`Trim` 且条件仍成立 → 保持 `Trim`(下一步按轮),否则回 `Idle`;`Exit{src}` → 退出完成:清待完成退出,按[强制退出](#强制退出)表"完成后"一列处理(`Exit{Param}` 且 `force_exit` 仍为真 → 保持 `Exit`,待完成退出保留),仍有仓位(带内残余)时置 `MINORDER_RESIDUAL`;`Stress` 且该 venue 仍 ≥ L2 → 保持 `Stress`(下一轮按 `stress_step_ms`),否则回 `Idle`;`Fix` → 回 `Idle`。凡回 `Idle` 都先查待完成退出 |
| `Exec{Done{cleared_by_engine}}` | 阶段为 `Exit` 且仍有仓位 → 保持 `Exit` 并重设 0/0;其余同 `Exec{Done}` 的按阶段规则(引擎清除时下次评估重新给目标) |
| `Exec{Stuck(r)}` | **按目标原因分两类**。生效目标为 `Rebalance`(场景 1–4):`Gated(bits)` 含模式位(`CLOSE_ONLY`/`MODE_SHADOW`)→ 清除增仓目标;只含 `LINK_STALE`(第二腿报价不健康)→ 保留(执行层在连接恢复后重试),计入 `GateStats`;含 `LINK_FUNDS` → `clear_target` + `Note{LinkFunds}`,`fail_until_ens = now.engine_ns + fail_cooldown_ms`;含其他风控位 → `clear_target`,`fail_until_ens` 同上;`SliceFails` → `clear_target` + `Note{SliceFails}`,`fail_until_ens` 同上。生效目标为 `Exit`/`Hard`/`MarginStress`:一律**保留**;`Gated`(非 `LINK_STALE`)与 `SliceFails` 时 `fail_until_ens = now.engine_ns + fail_cooldown_ms`,到期后以保存的字段重发、`tag_seq += 1`(与生效目标不同,执行层替换并重置失败计数),`held.retries += 1`,写 `Note{ExitRetry}`(P1),`SliceFails` 另写 `Note{SliceFails}`。两类共同:`NoLiquidity`/`StaleQuote` → 保留(执行层自行等待);`Shadow` → 保留;`External` → 走 `External` 行;`EngineOverride` → 阶段 `Overridden`;`MinOrder` → `Note{MinOrder}`,目标为 0/0 的 `Exit` 视为完成(`clear_target`,置 `MINORDER_RESIDUAL` 并记下两腿净量,按 `Exec{Done}` 的 `Exit` 规则结束;残余由执行层报 P1 交人工);`Fix` 阶段的目标遇 `MinOrder` 怎样结束、`Fix` 截止时做什么,策略研究后定 |
| `Exec{Broken(_)}`、`Risk{HedgeBroken}` | 阶段 `Broken`:`clear_target`(执行层冻结的旧目标在解除后不再推进),不发任何新目标;待完成退出保留。Broken **不是终态**:执行层可能自动有界平掉多出的一腿、执行人工 `HedgeFix`,或在 L3 / `Kill{flatten}` 下先减永续腿;这些成交以 `Exec{Progress}`(`origin = Engine(..)`)到达,只更新记录,不改阶段 |
| 阶段 `Broken` 中收到 `Exec{Done{cleared_by_engine}}` | `Note{BrokenCleared}`(v 带 `origin`);回 `Idle`(先查待完成退出),按 `group_pos`/`legs` 的**实际**仓位重新评估(两腿可能已被引擎改变,不沿用 Broken 前的状态);账户级 `HedgeFailure` cap 仍在时(`v.risk(a).open_blocked`)只走平仓闸门,直到人工 `Resume` 后的 `Risk{CapCleared}` |
| 阶段 `Overridden`;目标被拒 `TargetRejected(ForcedReduceActive \| FlattenLocked)`(L3 默认减仓或账户清仓锁期间) | 引擎接管:不发常规目标;只允许发"每腿更接近 0"的目标(`Exit` 的 0/0 目标照常被接受;有待完成退出的 group 进入本阶段时即重设 0/0);直到 `Done`、`Risk{CapCleared}` 或 `Resume{Kill}` 后的 cap 解除,之后回 `Idle`(先查待完成退出) |
| 目标被拒 `TargetRejected(LotMismatch)`(两腿 lot 在 `slice_max_chunk` 内对齐不了;只对增仓方向的配对目标) | 置 `LOT_MISMATCH` 并记下该 group 当时的 `slice_max_chunk` 生效值:此后不再设增仓 `Rebalance` 目标(减仓方向与 `Exit`/`Hard`/`MarginStress` 不受影响),直到该 group 的 `slice_max_chunk` 生效值或指纹变化;`Note{LotMismatch}`(P1:调高该 group 的 `slice_max_chunk` 或移出名单),每次置位一条 |
| `External{Liquidation \| Adl}`(永续腿) | 阶段 `Fix{Liquidation}`,在 `deadline_tsc` 前(保存为 `now.ens_of(deadline_tsc)`):`on_liquidation = exit_group` → 目标 0/0(`Hard`);`= align` → 现货腿降到与永续剩余量一致(`Hard`,`TakerSlice` 单腿);并设 `reentry_block_until_ens = now.engine_ns + liq_reentry_block_ms` |
| `External{Manual}` | **冻结**:阶段 `Fix{Manual}`,`clear_target`,此后不设任何策略目标(常规、`Trim`、`Exit`、`Stress` 减仓都不设;引擎的 L3 与 flatten 照常);待完成退出保留;`Note{ManualExternal}`(P1)。执行层对该 cell 的 `manual_hold`(置位条件、期间抑制的动作与解除条件)只在 [hft-exec 人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制)定义;本策略的 `clear_target` 不解除它。**解除冻结**:① `clear_block` 命令(两腿差 > `fix_tolerance` 时先按较小一腿对齐,`Hard`);② 两腿重新对齐(`legs(a, g).naked_ratio ≤ fix_tolerance` 且无在途、无 Pending/Unknown,每个 `External` 与 `Timer{SWEEP}` 检查);③ `exit`/`exit_all` 命令或 `force_exit = true` → 进入 `Exit`。①② 之后回 `Idle` 前先查待完成退出(冻结前在退出中:仍有仓位 → 继续 `Exit{src}`,已无仓位 → 按退出完成设封锁),否则 `Idle` 并重新评估 |
| `External{Unknown \| Delivery}` | 阶段 `Fix{Other}`:两腿对齐到较小一腿(`Hard`);`Note{ExternalFix}` |
| `External`(`gix = GIX_NONE`) | `Note{ExternalUnattributed}`(不可归属的外部成交由对账与人工处理) |
| `Risk{MarginStress{venue, level, liq_dist, ..}}` | 按[保证金压力](#保证金压力)表处理;记 `stress[venue] = (level, deadline_ens)`;level < L2 → 永续腿在该 venue 的 group 退出 `Stress`;每发一个减仓目标写 `Note{StressReduce}` |
| `Risk{CapCleared{MarginStress}}` | 清全部 venue 的压力状态,退出 `Stress`(回 `Idle` 前先查待完成退出),恢复常规评估 |
| `Risk{CapCleared{Delisting}}`(下架公告撤回,scope 覆盖该 group 任一腿 inst;投递范围归 hft-sequencer) | 清 `Delist` 来源的待完成退出;阶段 `Exit{Delist}` → `clear_target`、`Idle`、清下架标记并重新评估;已完成的下架退出解除"不再开仓" |
| `Notice{Delisting{at_ns}}`(任一腿) | 阶段 `Exit{Delist}`(记待完成退出):目标 0/0(`Exit`),`at_ns − now.utc_ns < delist_urgent_before_ms` 时升为 `Urgent`;到 `at_ns − delist_flat_margin_ms` 仍未完成 → `Note{DelistLate}` P1 |
| `Notice{FundingInterval}` | `Note{FundingInterval}`;Model 按新周期归一化 |
| `Notice{Maintenance{from, to}}`(任一腿的 venue) | 记 UTC 窗口 `[from − maint_block_ms, to]`;窗口内不设增仓目标,并清除进行中的增仓目标 |
| `Mode{old, new}` | 重置两个确认窗口;降到 `CloseOnly`/`Shadow` → 清除增仓 `Rebalance` 目标,持有(不主动退出,见[强制退出](#强制退出));回到 `Live` 重新评估 |
| `Conn` | 不单独处理(闸门 3 的报价健康覆盖) |
| `Manual` | 见[策略命令](#策略命令) |
| `Warmup` | 快速部分不订阅(只投递给 Model,见[预热载荷](#预热载荷)) |
| `Timer{SWEEP}` | 每 `sweep_ms` 轮询至多 `sweep_groups` 个 group(按 gix 轮转):advice 过期、待完成退出的 `Urgent` 升级(下架)、`Stress` 续发(每 `stress_step_ms`)与兜底退出、强制逐步减仓的轮检查与续步、`Fix` 截止、`Fix{Manual}` 的对齐检查、被保留目标的重发(`fail_until_ens` 到期)、`force_exit` 与负资金费条件复查、`exit_all` 与 L2 减仓超出 `Ctx` 容量部分的续发;距上次输出 ≥ `gate_stats_ms` 且计数非零的 group 输出 `GateStats` 与 `TrackErr`(每次至多 8 个 group,受 `Ctx` Note 容量 16 约束) |

运行模式对意图的处理以 [SDK 行为表](../../crates/hft-strategy-sdk/README.md#运行模式下意图的行为表)为准。

### 策略命令

即 `DESC.cmds`([SDK 策略描述符](../../crates/hft-strategy-sdk/README.md#策略描述符note报表策略命令)),ops_bot 与 hft-ctl 按导出的 schema JSON 通用编码。group 级命令带 `group = GROUP_NONE` → 不执行并写 `Note{CmdRejected}`(v1 = 1);实例级命令带具体 group → 同上(v1 = 3)。

| op | name | `reduce_only` | 字段(按顺序) | 范围 | 含义 |
|---|---|---|---|---|---|
| 1 | `exit` | true(中性类) | `urgent: Bool`(默认 false)、`reentry_block_ms: DurationMs`(默认 86 400 000,范围 0–30 d) | group | 该 group 强制退出到 0/0(`reason = Exit`,`urgent` 为真时 `Urgent`),完成后封锁再开仓 `reentry_block_ms`(见[强制退出](#强制退出));对 `Fix{Manual}` 冻结的 group 同时解除冻结 |
| 2 | `clear_block` | false(升级类,二次确认) | 无 | group | 清除该 group 的再开仓封锁(`exit`/`exit_all` 命令、负资金费退出或强平/ADL 设下的)并解除 `Fix{Manual}` 冻结;不改 `force_exit`、`open_allowed`,不清 `neg_streak` 与待完成退出 |
| 3 | `exit_all` | true(中性类) | 同 `exit` | 实例(`group = GROUP_NONE`) | 对本实例全部 group 执行 `exit`(见[强制退出](#强制退出)) |

### Note 码表与报表字段

即 `DESC.notes` 与 `DESC.report`,随 schema JSON 导出;watchtower 与 journal-archiver 据此通用汇总,实盘与模拟账户各出日报。"级别"取 `NoteLevel`(码表归 [hft-event](../../crates/hft-event/README.md);策略 Note 不用 P0)。

| code | name | 级别 | v[] | 产生时机 |
|---|---|---|---|---|
| 1 | `CashShort` | P2 | v0 = 现货 venue,v1 = 所需 Notional,v2 = 可用 Notional | 资金口径截断后可增量 < `slice_min_chunk`(每 group 限频 `gate_stats_ms`) |
| 2 | `MarginShort` | P2 | v0 = 永续 venue,v1 = 所需保证金,v2 = 可用保证金,v3 = 原因(1 可用不足、2 实际杠杆未知) | 同上(保证金约束) |
| 3 | `PerpShareLow` | P1 | v0 = 永续 venue,v1 = 占比(Rate) | `PortfolioAdvice.flags` 置 `PERP_SHARE_LOW`(限频) |
| 4 | `Concentration` | P1 | v0 = group,v1 = 占 E 的比例(Rate) | `CONCENTRATION`(限频) |
| 5 | `GateStats` | Info | v0..v7 = 自上次以来各闸门拦截次数:未就绪/HOLD、Advice 过期、报价不健康或资金费过期(含 `LINK_STALE`)、入场分位与确认、结算前保护、成本门、资金与上限(含 `LINK_FUNDS`、永续硬上限)、退出分位与退出分 | `Timer{SWEEP}`,每 group 每 `gate_stats_ms` 至多一条 |
| 6 | `TrackErr` | Info | v0 = \|desired − actual\| Notional,v1 = desired,v2 = actual | 同上 |
| 7 | `SliceFails` | P1 | v0 = 目标 tag,v1 = 失败退避剩余 ms | `Exec{Stuck(SliceFails)}` |
| 8 | `MinOrder` | P2 | v0 = leg_ix,v1 = 数量 | `Exec{Stuck(MinOrder)}`(0/0 退出时同时置 `MINORDER_RESIDUAL`,仓位变化前同一残余不再触发) |
| 9 | `LinkFunds` | P2 | v0 = `RejectBits` | `Stuck(Gated)` 含 `LINK_FUNDS` |
| 10 | `ExternalFix` | P1 | v0 = `ExternalCause`,v1 = leg_ix,v2 = 数量 | `External`(强平/ADL/Unknown/Delivery)进入 `Fix` 阶段 |
| 11 | `ExternalUnattributed` | P1 | v0 = venue,v1 = inst,v2 = 数量 | `External` 且 `gix = GIX_NONE` |
| 12 | `DelistLate` | P1 | v0 = 下架时刻 ns,v1 = 剩余名义 | 到 `at_ns − delist_flat_margin_ms` 仍未清仓 |
| 13 | `FundingInterval` | P2 | v0 = 旧周期 s,v1 = 新周期 s | `Funding` 周期变化或 `Notice{FundingInterval}` |
| 14 | `StressReduce` | Info | v0 = 档位,v1 = 新目标名义,v2 = 永续 venue,v3 = `liq_dist`(Rate) | MarginStress L2 每发一个减仓目标(同一事件超出 `Ctx` Note 容量 16 的部分被丢弃,`stress_steps` 会少计;处理方式策略研究后定) |
| 15 | `WarmupStatus` | Info | v0 = `n_warmed`,v1 = `n_ready`,v2 = group 数,v3 = `WARMUP_DONE` | `PortfolioAdvice` 中这些值变化 |
| 16 | `BrokenCleared` | P2 | v0 = `TargetOrigin`(引擎来源),v1 = 解除后实际名义 | Broken 阶段收到 `Done{cleared_by_engine}` |
| 17 | `ForceExit` | P2 | v0 = 来源(1 参数 `force_exit`、2 命令 `exit`、3 命令 `exit_all`),v1 = 退出前实际名义(`exit_all` 为合计),v2 = 再开仓封锁 ms(参数来源为 0),v3 = 命令 seq(参数来源为 0) | 进入强制退出(`exit_all` 每条命令一条,`group = GROUP_NONE`) |
| 18 | `CmdRejected` | P2 | v0 = 命令 seq,v1 = 原因(1 group 级命令带 `GROUP_NONE`、2 group 不在当前参数中、3 实例级命令带了 group) | 不执行的策略命令 |
| 19 | `EntryUnblocked` | Info | v0 = 命令 seq,v1 = 被清除的封锁剩余 ms,v2 = 是否解除了 `Fix{Manual}` 冻结 | `clear_block` 生效 |
| 20 | `LotMismatch` | P1 | v0 = group,v1 = 两腿 lot 的配对单位名义(Notional),v2 = `slice_max_chunk` | 增仓目标被拒 `LotMismatch` 并置 `LOT_MISMATCH`(每次置位一条;标志保持到该 group 的 `slice_max_chunk` 生效值或指纹变化,其他参数版本不清) |
| 21 | `ManualExternal` | P1 | v0 = leg_ix,v1 = 数量,v2 = 冻结前的阶段 | `External{Manual}`,该 group 进入 `Fix{Manual}` 冻结 |
| 22 | `PerpEquityCap` | P2 | v0 = 永续 venue,v1 = Σ 永续名义,v2 = 上限(`margin_balance / perp_min_equity_ratio`) | 永续硬上限截断后可增量 < `slice_min_chunk`(每 group 限频 `gate_stats_ms`) |
| 23 | `ExitRetry` | P1 | v0 = `TargetReason`,v1 = 连续重发次数(`held.retries`),v2 = 触发的 `StuckReason`,v3 = `RejectBits`(Gated 时),v4 = 新 `tag` | 被保留的 `Exit`/`Hard`/`MarginStress` 目标重发 |
| 24 | `NegFundingExit` | P2 | v0 = `neg_streak`,v1 = 退出前实际名义,v2 = 最近一期费率(Rate) | 负资金费退出触发 |
| 25 | `TrimStep` | P2 | v0 = 原因(1 `open_allowed = false`、2 超配),v1 = 新目标名义,v2 = 实际名义,v3 = `cap`(原因 2;否则 0),v4 = 已持续轮数 | [强制逐步减仓](#强制逐步减仓)每发一步 |

日报的策略专项字段(通用项——PnL 分解、换手、手续费/资金费比、成交率、两腿间隔、裸敞口、保证金占用——由 journal-archiver 从引擎事件算,不在此列):

| 字段 | 单位 | 聚合 | 范围 | 来源 |
|---|---|---|---|---|
| `gate_not_ready` … `gate_exit`(8 个,与 `GateStats` v0..v7 一一对应) | Count | Sum | Group | `GateStats` v0..v7 |
| `track_err_max` / `track_err_p99` | Notional | Max / P99 | Group | `TrackErr` v0 |
| `cash_short` / `margin_short` / `perp_cap_hits` | Count | Count | Instance | `CashShort` / `MarginShort` / `PerpEquityCap` |
| `slice_fails` | Count | Count | Group | `SliceFails` |
| `external_fix` | Count | Count | Group | `ExternalFix` |
| `manual_freezes` | Count | Count | Group | `ManualExternal` |
| `stress_steps` | Count | Count | Instance | `StressReduce` |
| `broken_cleared` | Count | Count | Group | `BrokenCleared` |
| `force_exits` | Count | Count | Group | `ForceExit` |
| `neg_funding_exits` | Count | Count | Group | `NegFundingExit` |
| `exit_retries` | Count | Count | Group | `ExitRetry` |
| `trim_steps` | Count | Count | Group | `TrimStep` |

### 参数表

单位见 SDK `Unit`;表中数值按可读形式书写(编码为原始定点:`Rate` 1e-9,`Notional` USDT 1e-6)。"来源":**雏形** = 雏形默认值,待在模拟账户上研究;**暂定** = 雏形无此参数的暂定值,同样在模拟账户上研究。"relax" 列 = `FieldDesc.relax`,"⑦" 列 = `FieldDesc.minor_change`(取值的写法见[小改属性](#小改属性)),两列的 `—` 都是 `None`;结构参数与人工专用字段见[字段属性](#字段属性)。

**Model:资金费与打分**

| 名称 | 单位 | 默认 | 范围 | per-group | relax | ⑦ | 来源 | 说明 |
|---|---|---|---|---|---|---|---|---|
| `score_model` | Enum[`proto_v1`] | `proto_v1` | — | 否 | — | — | — | 打分模型选择;新模型追加枚举值 |
| `w_funding` | Rate | 0.6 | 0–10 | 否 | — | — | 雏形 | 资金费分项权重 |
| `w_basis` | Rate | 0.3 | 0–10 | 否 | — | — | 雏形 | 基差分项权重 |
| `w_liquidity` | Rate | 0.1 | 0–10 | 否 | — | — | 雏形 | 流动性分项权重 |
| `funding_w_24h` | Rate | 0.7 | 0–1 | 否 | — | — | 雏形 | 资金费信号中窗口均值(`funding_window_ms`,默认 24 h)的权重(当前费率权重 = 1 − 它) |
| `funding_disagree` | Rate | 0.5 | 0–1 | 否 | — | — | 雏形 | 当前费率与窗口合计异号时的折扣 |
| `consistency_min_periods` | Count | 5 | 1–100 | 否 | — | — | 雏形 | 期数少于它时一致性取 `consistency_default` |
| `consistency_default` | Rate | 0.5 | 0–1 | 否 | — | — | 雏形 | — |
| `funding_risk_floor` | Rate | 0.5 bp | 0–100 bp | 否 | — | — | 雏形 | 资金费风险项分母下限 |
| `funding_norm_ms` | DurationMs | 14 400 000(4 h) | 1 h–24 h | 否 | — | — | 雏形 | 跨币可比的归一周期 |
| `funding_window_ms` | DurationMs | 86 400 000(24 h) | 8 h–48 h | 否 | — | — | 雏形 | 已实现资金费统计窗口。下限 8 h:窗口须覆盖最长的结算周期,否则窗口内可能没有已结算的一期、该 group 周期性不就绪(core venue 永续的最长结算周期待核实,方法:hft-cfg 生成 refdata 时取各 core venue 永续结算周期的最大值,超过 8 h 则同步调高下限)。上限是研究范围(窗口越长,资金费信号越滞后),不受资金费环约束:hft-strat-lib 的 `FUNDING_RING`(256 期)在 1 h 结算周期下覆盖约 10 天;窗口内的结算多于环所能保留的期数时 `summary` 置 `TRUNCATED`,本策略置 `FUNDING_TRUNC` 并按不就绪处理(Model 第 2 步) |
| `basis_vol_clamp` | Rate | 5.0 | 0–50 | 否 | — | — | 雏形 | 基差分项的波动因子上限 |
| `liq_score` | Rate | 0 | 0–3 | 是 | — | `Any` | 雏形公式 | 流动性分,由 [pipeline 插件](pipeline/README.md)按持仓量与成交额给出(分钟级以上) |
| `exit_score_min` | Rate | 0.4 | 0–10 | 否 | — | ±20% | 雏形 | 平仓所需退出分;0 = 关 |
| `exit_cost_ref` | Rate | 100 bp | 1–1000 bp | 否 | — | — | 雏形 | 退出分的平仓成本参考 |
| `exit_hold_ref_ms` | DurationMs | 604 800 000(168 h) | 1 h–30 d | 否 | — | — | 雏形 | 退出分的持仓时间参考 |

**Model:分位与分配**

| 名称 | 单位 | 默认 | 范围 | per-group | relax | ⑦ | 来源 | 说明 |
|---|---|---|---|---|---|---|---|---|
| `pct_window_ms` | DurationMs | 604 800 000(168 h) | 24 h–30 d | 否 | — | — | 雏形 | 分位窗口(两侧各一份);结构参数;也是插件的预热回看窗口 |
| `pct_sample_ms` | DurationMs | 5 000 | 1 000–60 000 | 否 | — | — | 雏形 | 采样间隔;结构参数 |
| `pct_range` | Rate | 30% | 1%–100% | 否 | — | — | 暂定 | 分位直方图范围 ±它(越界样本夹到端箱并计数);结构参数 |
| `pct_bin` | Rate | 1 bp | 0.1–100 bp | 否 | — | — | 暂定 | 箱宽(决定内存与查询耗时,误差 ≤ 半个箱宽);结构参数 |
| `pct_fill_max_ms` | DurationMs | 15 000 | 0–600 000 | 否 | — | — | 暂定 | 相邻观测间隔不超过它时空采样点用上一值补齐,否则记缺口 |
| `pct_min_coverage` | Rate | 0.25 | 0.05–1 | 否 | — | — | 雏形 | 样本数 / 满窗样本数达到它才就绪;也是插件预热的覆盖下限 |
| `basis_smooth_ms` | DurationMs | 60 000 | 0–600 000 | 否 | — | — | 雏形 | 打分所用基差的滑窗均值(1 s 桶;进检查点路径节) |
| `basis_smooth_min_samples` | Count | 5 | 1–100 | 否 | — | — | 雏形 | 滑窗样本不足时用瞬时值 |
| `p_entry` | Rate | 0.80 | 0.5–1 | 是 | `Down` | ±0.05 | 雏形 | 入场分位基准(也是基差加成起点) |
| `p_exit` | Rate | 0.40 | 0–0.5 | 是 | — | ±0.05 | 雏形 | 退出分位基准 |
| `band_min_gap` | Rate | 14 bp | 0–200 bp | 否 | — | ±20% | 雏形 | 入场值与退出值之差不足时加宽分位带 |
| `band_p_entry_cap` | Rate | 0.90 | 0.5–1 | 否 | — | — | 雏形 | 加宽上限 |
| `band_p_exit_floor` | Rate | 0.10 | 0–0.5 | 否 | — | — | 雏形 | 加宽下限 |
| `band_bonus_low_gap` / `band_bonus_high_gap` | Rate | 0 / 28 bp | 0–500 bp | 否 | — | — | 雏形 | 基差加成系数的线性映射区间 |
| `band_bonus_min` / `band_bonus_max` | Rate | 0.96 / 1.04 | 0–2 | 否 | — | — | 雏形 | 基差分项乘数范围 |
| `band_update_ms` | DurationMs | 1 800 000 | 60 000–86 400 000 | 否 | — | — | 雏形 | 分位带重算周期;在 UTC 的整数倍边界之后的第一个模型 tick 重算(Model 第 4 步) |
| `fa_bp_per_point` | Rate | 5 bp | 0.1–100 bp | 否 | — | ±20% | 雏形 | 资金费下沉:每日已实现资金费 `F` 每这么多,入场分位下降 1 个百分点 |
| `fa_drop_max` | Rate | 0.30 | 0–0.5 | 否 | `Up` | — | 雏形 | 下沉上限 |
| `min_entry_basis` | Rate | −200 bp | −1000–0 bp | 是 | `Down` | ±50 bp | 雏形 | 负基差底线(Model 用于排除,快速部分实时复查) |
| `deployable_frac` | Rate | 0.6 | 0–1 | 否 | `Up` | — | 雏形 | 可部署比例 |
| `max_share` | Rate | 0.20 | 0–1 | 否 | `Up` | — | 雏形 | 单币份额上限 |
| `k_power` | Rate | 1.3 | 0.1–5 | 否 | — | — | 雏形 | 幂律指数 |
| `coin_abs_cap` | Notional | 10 000 | 0–1 000 000 | 是 | `Up` | — | 雏形 | 单币绝对上限 |
| `restricted` | Bool | false | — | 是 | `Down` | `Any` | 雏形(名单) | 受限 group(插件拥有;true → false 为放宽) |
| `restricted_cap` | Notional | 1 500 | 0–1 000 000 | 否 | `Up` | — | 雏形 | 受限 group 的上限(不享受基差加成) |
| `kick_factor` | Rate | 1.0 | 0–5 | 否 | `Up` | — | 雏形 | 基差加成满量系数 |
| `kick_funding_floor` | Rate | 0.25 | 0–1 | 否 | — | — | 雏形 | 资金费因子下限 |
| `kick_funding_full` | Rate | 30 bp | 1–1000 bp | 否 | — | — | 雏形 | 每日已实现资金费 `F` 达到它时资金费因子 = 1 |
| `min_target` | Notional | 100 | 0–100 000 | 否 | — | ±20% | 雏形 | 目标与实际都低于它时目标 = 0 |
| `target_smooth_ms` | DurationMs | 60 000 | 0–600 000 | 否 | — | — | 雏形 | 目标滑窗平滑(进检查点路径节) |
| `spot_cash_reserve` | Notional | 2 000 | 0–1 000 000 | 否 | `Down` | — | 雏形 | 每个现货 venue 保留的 USDT |
| `alloc_period_ms` | DurationMs | 1 000 | 1 000–60 000,1 000 的整数倍 | 否 | — | — | 雏形 | 模型 tick 周期(模型 tick 以 1 s 一条的 `AdvisorTick` 为刻度,`check()` 拒绝非整数倍) |
| `advice_refresh_ms` | DurationMs | 10 000 | 1 000–300 000 | 否 | — | — | 暂定 | Advice 心跳周期:UTC 整数倍边界之后的第一个模型 tick 发全部 group([发送策略](#advice-载荷)) |
| `advice_min_change` | Rate | 0.01 | 0–1 | 否 | — | ±20% | 暂定 | 目标相对变化达到它才发送 |
| `warmup_max_step_ms` | DurationMs | 60 000 | 5 000–3 600 000 | 否 | — | — | 暂定 | 预热基差样本的最大间隔(更粗的块忽略);默认接受 1 分钟 K 线 |

**快速部分:闸门与资金**

| 名称 | 单位 | 默认 | 范围 | per-group | relax | ⑦ | 来源 | 说明 |
|---|---|---|---|---|---|---|---|---|
| `band` | Rate | 0.15 | 0–1 | 否 | — | ±20% | 雏形 | 不交易带(也用于目标重设判定) |
| `band_floor` | Notional | 50 | 0–100 000 | 否 | — | — | 雏形 | 不交易带分母下限 |
| `persist_ms` | DurationMs | 1 000 | 0–60 000 | 否 | — | ±20% | 雏形 | 设新目标时分位条件的持续确认 |
| `advice_max_age_ms` | DurationMs | 30 000 | 1 000–600 000 | 否 | — | — | 暂定 | 同一启动内 Advice 依据时刻(`basis_tsc`)距今超过它即过期,不可调仓 |
| `funding_max_age_ms` | DurationMs | 60 000 | 1 000–3 600 000 | 否 | — | — | 暂定 | 资金费新鲜度门 |
| `max_break_even_ms` | DurationMs | 432 000 000(5 d) | 1 h–60 d | 否 | `Up` | ±20% | 雏形 | 回本时间上限(开仓与平仓成本门) |
| `cost_multiplier` | Rate | 2.0 | 0–20 | 否 | `Up` | ±20% | 雏形 | 倍数条件(只用于开仓) |
| `bonus_close_pct` | Rate | 0.20 | 0–0.5 | 否 | — | ±0.05 | 雏形 | 平仓一次性收益的分位条件 |
| `settle_protect_ms` | DurationMs | 300 000 | 0–3 600 000 | 否 | — | ±20% | 雏形 | 结算前保护窗口 |
| `settle_protect_rate` | Rate | 1 bp | 0–100 bp | 否 | — | ±20% | 雏形 | 窗口内费率高于它时不平 |
| `overweight_tol` | Rate | 0.10 | 0–1 | 否 | `Up` | — | 暂定 | 超配容差:`actual > cap × (1 + 它)` 时超出 `cap` 的部分按超配减仓处理;超过 `cap × (1 + 2 × 它)` 持续 `close_only_rounds` 轮 → [强制逐步减仓](#强制逐步减仓) |
| `perp_margin_frac` | Rate | 0.50 | 0.05–1 | 否 | `Down` | — | 暂定 | 增仓名义的保证金占用估算下限;实际按 `max(它, 1 / 实际杠杆)`(见[资金口径](#资金口径)),不决定开满后的安全程度 |
| `perp_min_equity_ratio` | Rate | 0.40 | 0.05–1 | 否 | `Down` | — | 暂定(待标定) | 永续硬上限:每个永续 venue 开仓后 `Σ 永续名义 + 已承诺增量 ≤ margin_balance / 它`,与交易所杠杆无关 |
| `perp_min_share` | Rate | 0.40 | 0–1 | 否 | — | — | 雏形 | 永续 venue 保证金余额占比软告警 |
| `concentration_alert_share` | Rate | 0.25 | 0–1 | 否 | — | — | 雏形 | 单币集中度告警 |
| `fail_cooldown_ms` | DurationMs | 3 000 | 0–600 000 | 否 | — | — | 雏形 | 风控拒绝或执行失败后的退避;被保留的退出与风险目标按它重发 |
| `force_exit` | Bool | false | — | 是 | `Down` | `Any` | 暂定 | 强制退出该 group(见[强制退出](#强制退出));人工专用字段,不由插件填写;true → false 为放宽 |
| `sweep_ms` / `sweep_groups` | DurationMs / Count | 1 000 / 16 | 100–60 000 / 1–64 | 否 | — | — | 暂定 | 定时器轮询 |
| `gate_stats_ms` | DurationMs | 600 000 | 60 000–86 400 000 | 否 | — | — | 暂定 | `GateStats`/`TrackErr` 与资金不足 Note 的每 group 输出间隔 |

**执行选择(写入 `ExecSpec`)**

| 名称 | 单位 | 默认 | 范围 | per-group | relax | ⑦ | 来源 | 说明 |
|---|---|---|---|---|---|---|---|---|
| `slice_depth_frac` | Rate | 0.3333 | 0–1 | 否 | — | — | 雏形 | 单笔 ≤ 弱侧可见深度 × 它 |
| `slice_depth_levels` | Count | 1 | 1–10 | 否 | — | — | 雏形(只看顶档) | 写入 `depth_levels`(范围与 hft-exec 相同):计入可见深度的档数。本策略不订阅深度:该 inst 有 TopN 深度簿时执行层读簿,否则只用 BBO 顶档(见[订阅](#订阅)) |
| `slice_max_chunk` | Notional | 500 | 5–100 000 | 是 | `Up` | — | 雏形 | 单笔上限(也是两腿 lot 对齐的判定阈值) |
| `slice_min_chunk` | Notional | 5 | 0–10 000 | 否 | — | — | 雏形 | 单笔下限(仍受交易所最小名义约束) |
| `slice_cooldown_ms` | DurationMs | 1 000 | 0–600 000 | 否 | — | — | 雏形 | 同 group 两笔间隔 |
| `slice_max_cross` | Rate | 5 bp | 0–500 bp | 否 | — | — | 暂定 | 第一腿 IOC 越价上限(也是可见深度的价格范围);执行时按 `Views::risk_for` 的风控越价上限截取(增仓与减仓切片各取哪一项归 [hft-exec 限价与单笔名义](../../crates/hft-exec/README.md#限价与单笔名义)) |
| `hedge_cross` | Rate | 20 bp | 0–500 bp | 否 | — | — | 暂定 | 第二腿 IOC 越过对手价的比率(雏形两腿同发,无此参数) |
| `hedge_max_slip` | Rate | 50 bp | 0–1000 bp | 否 | — | — | 暂定 | 第二腿相对链接建立时对手价的滑点软阈值:超过时第二腿不失败,按风控 `rr_max_cross` 上限定价照发(语义归 hft-oms) |
| `fix_tolerance` | Rate | 0.02 | 0–0.5 | 否 | — | — | 暂定 | 补腿阈值(写入 `LegLink.tolerance`);也是 `Fix{Manual}` 冻结解除的对齐判据 |
| `residual` | Enum[`defer`, `trim_excess`, `hold`] | `defer` | — | 否 | — | — | 暂定 | 残余处理(语义见 hft-exec) |
| `open_first_leg` | Enum[`by_liquidity`, `perp_first`, `spot_first`] | `by_liquidity` | — | 是 | — | — | 暂定 | 增仓先下哪腿 |
| `close_first_leg` | 同上 | `by_liquidity` | — | 是 | — | — | 暂定 | 常规减仓、强制逐步减仓与 `Exit` 先下哪腿(`Hard`/`MarginStress` 由执行层强制永续先) |

**风险响应**

| 名称 | 单位 | 默认 | 范围 | per-group | relax | ⑦ | 来源 | 说明 |
|---|---|---|---|---|---|---|---|---|
| `stress_reduce_frac` | Rate | 0.25 | 0.05–1 | 否 | `Down` | — | 暂定 | MarginStress L2 与强制逐步减仓每步减掉的对冲量比例 |
| `stress_step_ms` | DurationMs | 10 000 | 1 000–600 000 | 否 | `Up` | — | 暂定 | L2 持续时两轮减仓的间隔(首轮在截止前发完);也是强制逐步减仓的轮长与步长 |
| `close_only_rounds` | Count | 3 | 1–100 | 否 | `Up` | — | 暂定 | 超配超过 `2 × overweight_tol` 或 `open_allowed = false` 持续这么多轮(每轮 `stress_step_ms`)后开始[强制逐步减仓](#强制逐步减仓) |
| `on_liquidation` | Enum[`exit_group`, `align`] | `exit_group` | — | 否 | — | — | 暂定 | 永续腿被强平/ADL 后的处理 |
| `liq_reentry_block_ms` | DurationMs | 86 400 000 | 0–30 d | 否 | `Down` | — | 暂定 | 强平/ADL 后禁止再开仓的时长(engine_ns,停机不计) |
| `neg_funding_exit` | Bool | true | — | 否 | `Down` | — | 暂定 | 负资金费退出开关(true → false 为放宽) |
| `neg_funding_exit_periods` | Count(u16) | 3 | 1–24 | 否 | `Up` | — | 暂定(模拟阶段研究) | 负资金费退出:`neg_funding_exit` 为真时,连续这么多次结算的实收费率 < 0 → 退出 |
| `reentry_block_ms` | DurationMs | 86 400 000 | 0–30 d | 否 | `Down` | — | 暂定 | 负资金费退出完成后禁止再开仓的时长(engine_ns,停机不计) |
| `delist_urgent_before_ms` | DurationMs | 21 600 000(6 h) | 0–7 d | 否 | — | — | 暂定 | 距下架多久升为 Urgent |
| `delist_flat_margin_ms` | DurationMs | 3 600 000(1 h) | 0–1 d | 否 | — | — | 暂定 | 须在下架前多久清仓 |
| `maint_block_ms` | DurationMs | 600 000 | 0–1 d | 否 | — | — | 暂定 | 维护开始前多久停止增仓 |

### 字段属性

由 SDK `FieldDesc` 的 `structural`、`manual_only`、`relax`、`minor_change` 表达(语义归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md#参数-schema-与-paramset)),随 schema JSON 导出;pipeline、ops_bot、ctl-gateway 与 ⑦ 的判定工具按属性通用处理。

- **结构参数**(`FieldDesc.structural = true`):`pct_window_ms`、`pct_sample_ms`、`pct_range`、`pct_bin`——它们决定分位环长度与箱号映射。允许热更:Model 按新 `PctConfig` 重建全部 group 的分位环(资金费环保留),`on_params` 返回 `ResetAll`,advisor 写 `Note{ModelGroupReset}`,pipeline 自动为这些 group 预热([参数应用与重置](#参数应用与重置))。在 Live 实例上改结构参数是升级类命令(需确认,预览给出受影响的 group 数与预计不能开仓的时长,时长按 SDK `ResetPreview` 规则估算(本策略 `WARMUP = true`,即 min(`cold_start_ms`, `path_settle_ms`));规则归 [control-plane](../../docs/subsystems/control-plane.md))。插件不拥有任何结构字段。其余字段都不是结构参数:改它们(包括每轮由插件更新的 `liq_score`)不清任何统计。
- **人工专用**(`manual_only = true`,插件不得输出):`force_exit`。插件拥有的字段:`liq_score`、`restricted`(插件的 `owned_fields`)。
- **放宽方向**(`relax`,见参数表 relax 列):pipeline 在必需输入失败的一轮只下发不放宽的改动,ops_bot 的覆盖项预览据此标出"放宽"([SDK 字段属性的使用方](../../crates/hft-strategy-sdk/README.md#参数-schema-与-paramset));它**不决定**人工 `SetParams` 的命令类别(body 字段不参与放宽判定,类别归 [control-plane](../../docs/subsystems/control-plane.md))。标了方向的字段在整个范围内单调:开关与期数分成两个字段(`neg_funding_exit`、`neg_funding_exit_periods`),不用"0 = 关闭"。

### 小改属性

[上线门槛 ⑦](../../docs/testing.md#8-上线门槛)(模拟账户 ≥ 14 天)的证据绑定 `(发布版本, StrategyParams SHA, RiskParams SHA)`;参数改动是否使 ⑦ 重新计时,由工具按每个字段的 `FieldDesc.minor_change` 与 ⑦ 计时起点生效的参数比较判定(计时与判定规则归 testing §8)。本策略在参数表"⑦"列给出每个字段的取值:

| 写法 | `MinorChange` | 含义 |
|---|---|---|
| `Any` | `Any` | 任意改动不重新计时:插件拥有的 `liq_score`、`restricted` 与人工专用的 `force_exit` |
| ±20% | `RelPct(20)` | 相对计时起点的值不超过 ±20%(多次修改按与起点比较;per-group 覆盖同样计):阈值类字段 |
| ±0.05、±50 bp | `Abs(…)`(按字段单位编码) | 与计时起点的绝对差不超过它:分位类字段(`p_entry`、`p_exit`、`bonus_close_pct`,相对比例对它们太宽)与可取负值的 `min_entry_basis` |
| — | `None` | 任何改动都重新计时:结构参数、资金与风险上限、执行参数、打分权重与模型选择、风险响应参数 |

group 的增删与 `open_allowed` 属于参数容器(`GroupSpec`),不是 schema 字段,按 testing ⑦ 的名单规则处理;`RiskParams` 的改动按 testing ⑦ 处理。

## 实现说明

<details><summary>实现要点</summary>

### Model(advisor)

每 group 状态:`fund: FundingStats`;`pct: PercentileStats`(一个两侧实例,`PctConfig{sides: 2, windows_ms: [pct_window_ms], sample_ms: pct_sample_ms, lo: −pct_range, hi: +pct_range, bin: pct_bin, fill_max_ms: pct_fill_max_ms}`;`PctSide::Buy` 侧样本 = 开仓侧基差 `(perp_bid − spot_ask)/spot_ask`,`PctSide::Sell` 侧样本 = 平仓侧基差 `(perp_ask − spot_bid)/spot_bid`);两侧基差的 `basis_smooth_ms` 滑窗(1 s 桶,按范围上限 600 s 预分配);分位带状态 `{p_entry_used, p_exit_used, natural_gap, bonus, band_at_ns}`(`band_at_ns = 0` = 尚无有效分位带);目标滑窗(每 tick 一项,按 `target_smooth_ms` 上限 / `alloc_period_ms` 下限预分配);上次发送的摘要(`target`、`open_th`、`close_th`、`bonus_close_th`、`flags`);`GroupSpec` 指纹、`warmed` 标志与 `kline_until_ns`。分位带、两个平滑窗与上次发送的摘要是路径状态,进检查点的路径节([Model 检查点](#model-检查点))。两种统计构件来自 [hft-strat-lib](../../crates/hft-strat-lib/README.md)。Model 状态里的时刻全部是 UTC ns(样本与结算时刻)或 engine_ns(持仓时长),不存 TSC。下文 `open.value(p)` = `pct.value(Buy, 0, p)`、`open.rank(x)` = `pct.rank(Buy, 0, x)`,`close.*` 同理取 `Sell` 侧(分位以 ppm 传递,文中写成小数)。

1. **采样**(两腿任一 `Bbo`):两腿报价都健康时(`quote_ok`,同闸门 3)`pct.observe(now.utc_ns, 开仓侧基差, 平仓侧基差)`(采样点划分与补齐规则在 PercentileStats);同时更新两侧基差滑窗。两侧分别采样、分别查询,不用 mid 代替。
2. **资金费**(`Funding`):`fund.observe(f, now.utc_ns)`;`Settled` 记一期已实现费率(= 结算前最后一次观测的费率,各 venue 是否与结算值一致待核实,见开发备忘);`IntervalChanged` 置载荷 `INTERVAL_CHANGED`(同一事件既结算又改周期时返回 `SettledChanged`,两件都做)。窗口统计 `s = fund.summary(funding_window_ms × 10⁶, now.utc_ns, funding_max_age_ms × 10⁶)`(两个参数换算为 ns,与函数签名一致):`F = s.realized_daily`、`T = s.realized_sum`、`m = s.min_rate`、`neg_streak = s.neg_streak`。`s.flags` 含 `TRUNCATED`(环已满且最旧记录晚于窗口起点,判据归 hft-strat-lib)→ 置 `FUNDING_TRUNC`、该 group 不就绪。
3. **就绪**:`pct.coverage(0).samples ≥ pct_min_coverage × expected`(预热注入的样本计入),且 `summary` 至少 1 期、无 `STALE`/`NO_DATA`、无 `FUNDING_TRUNC` → `READY`;否则不参与分配,有仓位置 `HOLD`、无仓位 target = 0。普通重启从 Model 检查点恢复;没有可用统计时靠[预热载荷](#预热载荷)就绪,两者都没有时样本不足、该币不可开仓(见"已知限制")。
   - **预热**(`Warmup`):按[预热载荷](#预热载荷)的应用规则调用 `pct.prefill` / `fund.prefill_settled`,置该 group 的 `warmed` 并更新 `n_warmed` 与 `kline_until_ns`;每块 ≤ 509 个样本,处理在模型 tick 的 1 ms 时间片内。
4. **分位带**(UTC 对齐):`now.utc_ns` 越过 `band_update_ms` 的整数倍边界(自 UTC 纪元起)后的第一个模型 tick,重算全部就绪 group,`band_at_ns` 记该边界;尚无有效分位带的就绪 group(新加入、被重置、路径节被丢弃)在它就绪后的第一个模型 tick 先算一次(`band_at_ns` 记该 tick 时刻)。所以边界上的重算只取决于 UTC 与样本,不取决于实例何时启动。算法:从 `(p_entry, p_exit)` 出发,`gap = open.value(p_e) − close.value(p_x)`;`gap < band_min_gap` 时 `p_e` 每步 +0.01、`p_x` 每步 −0.01,直到满足或同时到达 `band_p_entry_cap`/`band_p_exit_floor`(`gap` 随步数单调不减,实现按步数二分求最小满足步数,结果与逐步加宽相同,每 group ≤ 12 次分位查询);`bonus` = `natural_gap` 在 `[band_bonus_low_gap, band_bonus_high_gap]` 上线性映射到 `[band_bonus_min, band_bonus_max]`。
5. **打分**(`proto_v1`;`r` = 当前费率,`n = funding_norm_ms / (interval_s × 1000)`,`w = funding_window_ms`):
   - `now_n = r × n`;`avg_n = T × funding_norm_ms / w`
   - `agree = (sign(r) == sign(T) 或 T == 0) ? 1 : funding_disagree`;`cons = total_periods < consistency_min_periods ? consistency_default : pos/total`
   - `signal = (avg_n × funding_w_24h + now_n × (1 − funding_w_24h)) × agree × cons`;`risk = max(0, −m × n)`
   - `funding_score = max(signal, 0) / max(risk, funding_risk_floor)`
   - `basis_score = open.rank(平滑开仓侧基差) × clamp((open.value(1.0) − open.value(0)) / max(|open.value(0.5)|, 1 bp), 0, basis_vol_clamp)`(雏形用均值绝对值作分母,这里用开仓侧中位数;分母的选择在模拟账户上研究)
   - `entry_score = w_funding × funding_score + w_basis × basis_score × bonus + w_liquidity × liq_score`
   - 打分模型是 crate 内的 `enum ScoreModel`,按 `score_model` 参数选择;新增模型 = 新变体 + 新枚举值,不改其他步骤。
6. **分配**(每个现货池 `s` 独立;按 gix 升序):
   1. 计算 `E`、`D`、`A_s` 与缩放([资金口径](#资金口径))。
   2. `ENTRY_BLOCKED` 的 group 目标 = 实际;`A_s' = max(0, A_s − Σ 这些 group 的实际名义)`(被阻塞仓位超过池额时其余 group 得 0,不出现负额)。
   3. 活跃集 = 就绪、未阻塞、`open_allowed`、`!force_exit`、无下架通知(Model 从哪里得到下架状态与撤回、该状态归检查点哪一节,策略研究后定)、无 `NEG_FUNDING`、`F_i > 0`(`open_daily > 0`;开仓成本门要求每日收益为正,`F ≤ 0` 的 group 分到资金也开不了仓)、`entry_score > 0`。
   4. 幂律:`w_i = s_i^k / Σ s_j^k`(`k = k_power`)。
   5. 迭代封顶(≤ 20 轮):超过 `max_share` 的截到 `max_share`,超出部分按未封顶者的原权重比例再分配;`base_i = w_i × A_s'`。
   6. 受限两遍:第一遍含受限 group,受限者 `base = min(base, restricted_cap)` 并从 `A_s'` 扣除;第二遍把剩余 `A_s'` 分给非受限者。
   7. 基差加成:`pct = open.rank(平滑开仓侧基差)`;`pct ≥ p_entry` 时 `frac = (pct − p_entry) / (1 − p_entry)`,资金费因子 `= F < 0 ? 0 : clamp(kick_funding_floor + (1 − kick_funding_floor) × F / kick_funding_full, kick_funding_floor, 1)`,`kick = min(frac × cap_i × kick_factor × 因子, cap_i)`;受限者无加成。
   8. 同池超额再分配(≤ 20 轮):`raw_i = base_i + kick_i` 超过 `cap_i` 的部分按其余 group 的剩余空间(`cap_j − raw_j`)比例再分配,最后截到 `cap_i`。
   9. **平滑**:`sm_i` = 最近 `target_smooth_ms` 内各 tick `raw_i` 的均值,再截到当前 `cap_i`。
   10. **池预算**(在平滑之后,保证输出满足预算):若 `Σ_{i∈s, 非阻塞} sm_i > A_s'`,非阻塞 group 等比缩到 `A_s'`;全部池合计再保证 ≤ `D`(雏形各币目标之和可超过可部署资金,这里不照搬)。
   11. `sm_i < min_target` 且 `actual_i < min_target` → 0;`force_exit` 的 group → 0;结果即 `target_i`。
   - Model 目前不推断快速部分的命令封锁、强平后封锁、`LotMismatch` 与 `Fix{Manual}` 冻结,这些 group 仍可能分到份额;该份额在封锁期间闲置,属于资金利用率损失,不是风险(SDK 已允许 Model 订阅本实例的 `Exec`/`Order`/`Manual`,是否采用见开发备忘 9)。
7. **机会与平仓收益**:`open_daily_i = max(F_i, 0)`;`best_other_F_i = max_{j ≠ i, j ∈ 活跃集, spot(j) = spot(i)} F_j`(只看同一现货池:释放的现金只能在本池再部署),池内除 i 外活跃集为空时 `= 0`;`close_gain_daily_i = max(0, max(best_other_F_i, 0) − F_i)`(对所有有仓位的 group 计算,包括不在活跃集里的)。
8. **阈值**:`p_req = p_entry_used − clamp(max(F, 0) / fa_bp_per_point × 0.01, 0, fa_drop_max)`;`open_th = open.value(p_req)`、`close_th = close.value(p_exit_used)`、`bonus_close_th = close.value(bonus_close_pct)`。
9. **退出分**:`ccs = 1 − clamp(max(实时平仓侧基差, 0) / exit_cost_ref, 0, 1)`;`fp = signal < 0 ? min(1 + |signal| / 100 bp, 3) : 1`;`tp = clamp(持仓时长 / exit_hold_ref_ms, 0.5, 2)`(持仓时长 = `now.engine_ns − group_pos.round_start_ens`,停机不计);`hold = clamp((pct − 0.5) / 0.5, 0, 1)`;`exit_score = ccs × fp × tp × (1 − hold)`。
10. **发送**:按[发送策略](#advice-载荷)输出 `GroupAdvice` 与 `PortfolioAdvice`(变化即发、UTC 对齐的心跳、重置后补发),发送后更新上次发送的摘要;f64 只在编码前经 `model_math` 转为定点。

#### 参数应用与重置

`Model::on_params(p, groups, rd) -> ModelParamEffect`(通用规则见 [SDK 参数变化时的 group 重置](../../crates/hft-strategy-sdk/README.md#参数变化时的-group-重置);advisor 尾随到 Applied 的 `SetParams`/`ReplaceRefdata` 后调用,检查点恢复后也以重放起点的参数调用一次),幂等,按 `GroupId` 对齐。本策略的取值:

| 情况 | 动作 | 返回 |
|---|---|---|
| 只有非结构参数变化(含 `liq_score`、阈值、权重) | 更新参数副本,统计与路径状态全部保留 | `None` |
| 新加入的 `GroupId` | 空状态起步(未就绪) | `ResetGroups`,原因 `Fingerprint`(pipeline 据此为它们预热) |
| 被移除的 `GroupId` | 丢弃状态 | 不计入 |
| 已有 `GroupId` 的 `GroupSpec::fingerprint()` 与已存指纹不同 | 该 group 全部状态(含路径状态)重置 | `ResetGroups`,原因 `Fingerprint` |
| 检查点里某 group 的 `PctConfig` 与当前参数不一致(停机期间改了结构参数) | 该 group 的分位环按当前配置重建(资金费环保留),路径状态清空 | `ResetGroups`,原因 `PctConfig` |
| 结构参数变化(本策略的结构参数都不是 per-group,改动影响全部 group) | 全部 group 的分位环按新 `PctConfig` 重建,资金费环保留,路径状态清空 | `ResetAll` |

多种情况同时出现时合并为一个返回值(有 `ResetAll` 取 `ResetAll`,否则各 `ResetGroups` 取并集)。被重置的 group 由 advisor 写 `Note{ModelGroupReset}`,之后按"未就绪"处理(有仓位置 `HOLD`),pipeline 对它们自动预热;重置不影响快速部分已有的仓位与阶段。

#### Model 检查点

检查点的键、两节结构、分块与丢弃规则归 [SDK Model 检查点](../../crates/hft-strategy-sdk/README.md#model-检查点);本策略的取值:

- **版本**:`CarryModel::LAYOUT_VERSION = 1`(原始样本节)、`CarryModel::PATH_VERSION = 1`(路径状态节);`CarryModel::WARMUP = true`(本策略有预热载荷,见[预热载荷](#预热载荷))。
- **原始样本节**(`snapshot_chunk`/`restore`):实例头(group 数、标志);每 group 一节,以 `GroupId` 为键,含 `GroupSpec` 指纹、`warmed` 标志、`kline_until_ns`、`PercentileStats` 快照(hft-strat-lib 写入 `PctConfig`、末次观测与箱号环)、`FundingStats` 快照(`FUNDING_RING` = 256 期的结算环与检测状态)。布局以 `CarryModel::LAYOUT`(`ModelLayoutDesc`:实例级节 1 个,group 级节依次为指纹与来源标志、`PercentileStats`、`FundingStats`)描述,布局金样 = 它的规范 JSON `fixtures/strategies/portfolio-carry/ckpt/layout.json`;金样变化而 `LAYOUT_VERSION` 未升 → CI 失败,`testing::check_layout` 断言实际写出与描述一致。
- **路径状态节**(`snapshot_path`/`restore_path`,检查点的第一块):只放不能由原始样本重算、又决定此后输出的派生状态。

  | 范围 | 内容 | 上界 |
  |---|---|---|
  | 实例 | 最近一次心跳边界 `hb_at_ns`;上次发送的 `PortfolioAdvice` 摘要(`flags`、`n_ready`、`n_active`、`n_warmed`) | 64 B |
  | 每 group(以 `GroupId` 为键) | 分位带 `{p_entry_used, p_exit_used, natural_gap, bonus, band_at_ns}`;两侧基差平滑窗的 1 s 桶(只写窗内的桶);目标平滑窗(每模型 tick 一项,只写窗内的项);上次发送的 `GroupAdvice` 摘要(`target`、`open_th`、`close_th`、`bonus_close_th`、`flags`) | `P_g`(公式见[延迟与资源契约](#延迟与资源契约)) |

  f64 值按位写出(`F64Bits`),恢复后逐位相同。打分分项、分配结果、阈值(`open_th` 等)与 `best_other_F` 每个模型 tick 由原始样本与路径状态重算,不进任何一节。
- **恢复**:两节都可用 → 续算与连续运行逐字节相同(含分位带边界前后、平滑窗未满与心跳时刻;模型 tick 按 UTC 对齐,见 SDK `SubsBuilder::model_tick`)。路径节不可用(`PATH_VERSION` 不符或 `restore_path` 出错)→ 只丢路径节(advisor 写 `Note{ModelPathReset}`),原始样本保留,`restore` 按原始样本给出派生状态的近似值:第一个模型 tick 为每个就绪 group 计算分位带、平滑窗从空开始(样本不足时按 `basis_smooth_min_samples` 用瞬时值)并发出全部 group 的 Advice。此后在下一个 UTC 分位带边界重算分位带,平滑窗再经 `max(basis_smooth_ms, target_smooth_ms)` 填满,再过一个心跳边界上次发送的摘要也一致,输出重新与连续运行逐字节相同。所需的最长事件时间即 `Model::path_settle_ms`(公式见[延迟与资源契约](#延迟与资源契约),默认约 31 min);当日 recompute 跳过检查点之后这一段(规则归 [SDK](../../crates/hft-strategy-sdk/README.md#model-检查点) 与 [testing](../../docs/testing.md))。
- **版本纪律**:原始样本的布局或含义变化 → 升 `LAYOUT_VERSION`(丢整份检查点,用预热缩短)。路径节的布局或计算逻辑变化 → 升 `PATH_VERSION`:分位带算法、打分与分配(它们决定目标平滑窗里的值与上次发送的摘要)、平滑窗桶宽、摘要字段都算,代价只是下次重启丢一次路径节,原始样本与 168 h 统计不受影响。路径金样 `fixtures/strategies/portfolio-carry/ckpt/path.bin`(固定事件脚本跑完后 `snapshot_path` 的字节)变化而 `PATH_VERSION` 未升 → CI 失败。快速部分的闸门逻辑不在检查点里,改它两者都不升。
- **分块写出**:`Model::snapshot_chunk(&self, from_gix, w, max_bytes) -> SnapshotProgress` 只写原始样本节:从 `from_gix` 起按 gix 升序写完整的 group 节(`from_gix = 0` 时先写实例头),再写一个 group 会超过 advisor 给出的 `max_bytes` 时停并返回 `More(next_gix)`,写完返回 `Done`;一块至少含一个完整 group(默认参数下单 group ≈ 0.47 MiB)。同一份检查点的各块之间**整个 advisor 暂停**、不消费 journal(规则归 [hft-sequencer](../../crates/hft-sequencer/README.md)),所以两节与各块对应同一个 `last_seq`;分块结果与一次写出逐字节相同。
- 上界 `Model::snapshot_bytes(p, n_groups)`(两节之和)与 `Model::mem_bytes(p, n_groups)` 的公式见[延迟与资源契约](#延迟与资源契约)。

### 快速部分

```rust
struct GroupState {                       // ≤ 512 B,按 GroupTable::capacity() 预分配
    gid: GroupId, fp: u64,                // fp = GroupSpec::fingerprint();on_params 比对,不同即整行重置
    perp: InstId, spot: InstId, perp_venue: VenueId, spot_venue: VenueId,
    adv: Expiring<GroupAdviceCache>,      // basis_seq + engine_ns 截止(hft-strat-lib)
    cw_open: ConfirmWindow, cw_close: ConfirmWindow,   // hft-strat-lib,engine_ns
    trig: TriggerPx,                      // 由另一腿价格与 open_th/close_th 预计算的触发价
    phase: Phase,                         // Idle | Rebalancing{dir} | Trim{cause: OpenDisallowed|Overweight}
                                          //  | Exit{src: Delist|Param|Cmd|All|NegFunding}
                                          //  | Fix{cause: Liquidation|Manual|Other, deadline_ens} | Stress | Overridden | Broken
    pend_exit: PendingExit,               // 待完成退出:src(None = 无)、urgent、block_ms、cmd_seq;只在完成或来源撤销时清除
    held: HeldTarget,                     // 被保留的 Exit/Hard/MarginStress 目标:reason、urgency、两腿目标量、场景码(算法按场景码与当前参数重建),
                                          //  retries(连续重发次数,有成交清零)
    tag_seq: u8,                          // 目标 tag 的低 8 位:重发时 +1、回绕、从不清零
    trim: TrimTrack,                      // rounds: u8, next_ens: i64(强制逐步减仓的轮计数与下一轮时刻)
    fund: FundTrack,                      // last_rate: Rate, last_next_ns: i64(UTC), neg_streak: u16
    resid_pos: [Qty; 2],                  // MINORDER_RESIDUAL 置位时的两腿净量
    lot_chunk: Notional,                  // LOT_MISMATCH 置位时该 group 的 slice_max_chunk 生效值
    fail_until_ens: i64, reentry_block_until_ens: i64,          // engine_ns
    maint_from_ns: i64, maint_to_ns: i64, delist_at_ns: i64,    // UTC ns(来自 Notice)
    flags: u16,                           // DELIST_DONE | LOT_MISMATCH | MINORDER_RESIDUAL
}
struct StressState { level: MarginLevel, deadline_ens: i64, next_step_ens: i64, liq_dist: Rate }
struct Instance { params: CarryParams,    // 覆盖后的参数(p.at(gix) O(1) 读取;on_params 时替换)
                  groups: Box<[GroupState]>, stress: [StressState; MAX_VENUES],
                  sweep_cursor: u16, cont: Continuation,  // exit_all 与 L2 减仓超出 Ctx 容量部分的续发位置
                  portfolio_adv: Expiring<PortfolioAdviceCache> }
```

- 闸门实现全部为整数:基差用 `Px::ratio`,触发价用 `Px::mul_rate` 预计算(价格越过阈值的方向取更难触发),成本门用 i128,`1 / L` 由 `Views::leverage` 的百分之一倍整数换成 `Rate`。
- 每个事件只处理一个 group;`Risk`、`Mode`、`exit_all` 等账户级事件按 gix 升序处理,超出 `Ctx` 目标容量(64)的部分记在 `cont` 并在定时器事件里继续(`max_groups_per_instance` = 64 时只在同一事件已占用部分目标容量时出现)。
- 增仓评估的资金扫描(`committed_*` 推导与永续硬上限的求和)一次遍历本实例 group,只在开仓方向通过前 5 道闸门后执行,O(group 数)。
- **时间**:跨事件保存的时刻全部是 engine_ns(`*_ens`)或 UTC ns(Notice 给出的窗口、资金费结算时刻);视图里的 `deadline_tsc` 保存前用 `now.ens_of` 换算,登记定时器时用 `now.tsc_of`。
- 快照:`Instance`(不含 `params`,参数随引导块与 `SetParams` 恢复)与每个 `GroupState`(以 `GroupId` 为键),`version = 1`;快照里没有 TSC,也没有 `committed_*`。`restore` 时 `reset` 全部确认窗口、`invalidate` 全部触发价(依据的是重启前的行情),阶段、待完成退出、被保留的目标、`tag_seq`、`trim`、`neg_streak`、各标志、封锁、冷却与截止按 engine_ns / UTC 原样继续(停机不计)。
- `on_params`:按 `GroupId` 对齐;新 group 取空状态;指纹不同的 group 整行重置(见 [group 身份](#group-形态));`LOT_MISMATCH` 只在该 group 的 `slice_max_chunk` 生效值不等于 `lot_chunk` 时清除(其他参数版本不清,pipeline 每轮改 `liq_score` 不会重复报 P1);`force_exit` 由 true 改为 false 的 group 清除 `Param` 来源的待完成退出,从 `Exit{Param}` 回到 `Idle`;`open_allowed` 改回 true → `Trim{OpenDisallowed}` 回 `Idle`、`trim.rounds` 清零。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `PortfolioCarry`、`DESC`、`Strategy` 实现 |
| `src/params.rs` | `define_params!`(上方参数表,含字段属性与 `minor_change`)与 `check`(group 形态、`alloc_period_ms` 整秒) |
| `src/advice.rs` | `GroupAdvice`、`PortfolioAdvice` 载荷 |
| `src/desc.rs` | Note 码表、报表字段、`cmds`(`exit`、`clear_block`、`exit_all`) |
| `src/warmup.rs` | `BasisWarmup`、`FundingWarmup` 解码 |
| `src/fast/{gates.rs,events.rs,risk.rs,capital.rs,exit.rs,trim.rs,funding.rs}` | 快速部分(`#![deny(clippy::float_arithmetic)]`;`exit.rs` = 强制退出、待完成退出、`exit_all`、被保留目标的重发;`trim.rs` = 强制逐步减仓;`funding.rs` = 结算检测与 `neg_streak`) |
| `src/model/{mod.rs,score.rs,alloc.rs,band.rs,funding.rs,warmup.rs,ckpt.rs}` | 慢速部分(`warmup.rs` 应用预热块;`ckpt.rs` = 参数应用与重置、原始样本节分块、路径状态节、`mem_bytes`/`snapshot_bytes`) |
| `tests/{model_golden.rs,alloc_props.rs,fast_scenarios.rs,determinism.rs,warmup.rs,ckpt_layout.rs,restart.rs}` | 见测试(SDK `testing` 特性) |
| `benches/carry.rs`、`benches/budgets.toml` | 基准 |
| `pipeline/` | pipeline 插件包([pipeline/README.md](pipeline/README.md)) |
| `../registry/tests/portfolio_carry_{scenarios,dst,recovery}.rs` | 引擎级场景、DST 与恢复测试(hft-testkit 驱动) |

</details>

## 延迟与资源契约

快速部分在 `decided_tsc − proc_tsc` 分段内;Model 在 advisor 线程,不计入 tick-to-trade。入口按 [SDK 规定的模板](../../crates/hft-strategy-sdk/README.md#延迟与资源契约)登记:

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `on_event(Bbo)`,未越过触发价且确认窗口空闲 | ≤ 15 ns | ≤ 40 ns | `strategies/portfolio-carry/tick_miss` |
| `on_event(Bbo)`,另一腿触发价重算 | ≤ 60 ns | ≤ 150 ns | `strategies/portfolio-carry/slow_update` |
| `on_event(Advice)` 解码并缓存 | ≤ 100 ns | ≤ 300 ns | `strategies/portfolio-carry/advice` |
| 闸门全部通过 → `TargetIntent`(不含资金扫描) | ≤ 250 ns | ≤ 800 ns | `strategies/portfolio-carry/target_set` |
| 增仓资金扫描(64 group:`committed_*` 推导 + 永续硬上限求和) | ≤ 400 ns | ≤ 1 µs | `strategies/portfolio-carry/capital_scan` |
| `on_event(Funding)`,结算检测与 `neg_streak` | ≤ 30 ns | ≤ 80 ns | `strategies/portfolio-carry/funding_update` |
| `on_event(Exec)` | ≤ 80 ns | ≤ 250 ns | `strategies/portfolio-carry/exec_update` |
| `on_event(Risk)`,每个受影响 group | ≤ 150 ns | ≤ 400 ns | `strategies/portfolio-carry/risk_event` |
| `on_event(Manual)` `exit` → 退出目标 | ≤ 250 ns | ≤ 800 ns | `strategies/portfolio-carry/cmd_exit` |
| `on_event(Manual)` `exit_all`(一批 64 group) | ≤ 16 µs | ≤ 50 µs | `strategies/portfolio-carry/cmd_exit_all` |
| `CarryModel` 采样(每个 `Bbo`) | ≤ 1 µs | ≤ 5 µs | `model/portfolio-carry/sample` |
| `CarryModel` 模型 tick(64 group 全量,不在分位带边界) | ≤ 300 µs | ≤ 1 ms | `model/portfolio-carry/tick` |
| `CarryModel` 分位带边界的模型 tick(64 group 全量 + 分位带重算) | ≤ 700 µs | ≤ 2 ms | `model/portfolio-carry/band_tick` |
| `CarryModel` 应用一块 `BasisWarmup`(509 样本) | ≤ 30 µs | ≤ 80 µs | `model/portfolio-carry/warmup_chunk` |
| `CarryModel::snapshot_path`(64 group,默认参数,≈ 160 KiB) | ≤ 150 µs | ≤ 400 µs | `model/portfolio-carry/snapshot_path` |
| `CarryModel::snapshot_chunk`(一块,advisor 给出的 `max_bytes`) | ≤ 400 µs | ≤ 1 ms | `model/portfolio-carry/snapshot_chunk` |

- 本策略不直接发订单意图,没有 `tick_fire`。
- 模型 tick 每 group 7 次分位查询(`basis_score` 的 1 次 rank 与 3 次 value,`open_th`/`close_th`/`bonus_close_th` 各 1 次;基差加成与退出分复用同一个 rank);按默认箱宽(1 bp,±30% 范围,6 001 箱)每次 ≤ 0.5 µs p50 / 1.5 µs p99(hft-strat-lib `stratlib/pct_query_small`),64 group 合计 ≤ 0.23 ms p50 / 0.68 ms p99。分位带边界的 tick 每 group 另加 ≤ 12 次(按步数二分),每 `band_update_ms` 一次,单列预算 `band_tick`;其 p99 超过 SDK 模型 tick 的 1 ms 时间片,是否分摊到边界后的几个 tick,策略研究后定。
- **快速部分内存**:≤ 512 B × 64 group ≈ 32 KiB。
- **group 数**:正式实例 ≤ `max_groups_per_instance`(取值归 [apps/hft-core 容量表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属));**模拟实例(`sim`)默认 ≤ 32 group**(pipeline policy `max_groups` 的默认值;dev2 `sim` 的 advisor 预算与 cell 数按"2 个研究策略 × 2 个池实例 × 32 group"定,取值归 apps/hft-core)。研究中加大 `pct_*` 的改动先按下式核对预算。
- **Advice 量**:`GroupAdvice` 每条约 230 B(含记录头);心跳每 `advice_refresh_ms` 发全部 group,64 group 约 130 MB/日 journal Input,变化即发的条数另计(待实测:模拟账户运行 7 天统计每日 Advice 字节数)。
- **Model 内存与快照上界** `Model::mem_bytes(p, n)`、`Model::snapshot_bytes(p, n)`(使用方与校验见 [SDK Model 内存与快照声明](../../crates/hft-strategy-sdk/README.md#model-内存与快照声明):hft-core 对照 `advisor.mem_budget_mib`,`SetParams` 超预算 → `ParamInvalid{ModelMemBudget}`):

  ```
  L   = ⌈pct_window_ms / pct_sample_ms⌉                 // 分位环长度
  B   = 2 × pct_range / pct_bin + 1                     // 箱数
  w   = 2(B < 65 535,u16 箱号),否则 4
  S_b = ⌈basis_smooth_ms / 1 000⌉ + 1                  // 基差平滑窗的 1 s 桶数
  S_t = ⌈target_smooth_ms / alloc_period_ms⌉           // 目标平滑窗项数
  P_g = 128 + 32 × S_b + 8 × S_t                        // 每 group 的路径状态节
  mem_bytes(p, n)      = 65 536 + n × (8 × L + 8 × (B + ⌈B / 256⌉) + 32 768)
  snapshot_bytes(p, n) = 320 + n × (w × 2 × L + 8 192 + P_g)   // 两节之和;320 = 原始样本节实例头 256 + 路径节实例部分 64
  path_settle_ms(p)    = band_update_ms + max(basis_smooth_ms, target_smooth_ms) + advice_refresh_ms + alloc_period_ms
  cold_start_ms(p, g)  = max(⌈pct_min_coverage × pct_window_ms⌉, funding_window_ms)
  ```

  内存每 group 的 `8 × L` 是两侧 i32 箱号环,`8 × (B + ⌈B/256⌉)` 是两侧两级直方图;32 768 B 是固定部分的上界:`FundingStats`(`FUNDING_RING` 256 期 × `SettleRec` 24 B + 观测状态 ≤ 64 B = 6 208 B)、基差平滑窗(按范围上限预分配 601 桶 × 2 侧 × 16 B = 19 232 B)、目标平滑窗(600 × 8 B = 4 800 B)、分位带、上次发送摘要与指纹(≤ 2 048 B),合计 32 288 B。快照原始样本节每 group 的 `w × 2 × L` 是箱号环,8 192 B 覆盖节头与指纹(≤ 128 B)、`PctConfig` 与末次观测(≤ 128 B)与 `FundingStats` 快照(6 208 B);`P_g` 是路径状态节(两侧平滑窗每桶 16 B、目标平滑窗每项 8 B、分位带与摘要 ≤ 128 B)。默认参数(L = 120 960,B = 6 001,S_b = 61,S_t = 60,`P_g` = 2 560 B):`mem_bytes` ≈ 1.00 MiB/group,64 group ≈ 64.1 MiB,32 group ≈ 32.1 MiB;`snapshot_bytes` ≈ 0.47 MiB/group,64 group ≈ 30.2 MiB,32 group ≈ 15.1 MiB(其中路径节 64 group ≈ 160 KiB)。例:`pct_window_ms` 改为 30 d → 每 group 约 4.0 MiB 内存、2.0 MiB 快照,64 group 约 258 MiB / 127 MiB,超出预算的修改在 `SetParams` 时被拒,不会在运行中 OOM。检查点的磁盘占用与保留代数归 apps/hft-core。

  `path_settle_ms`:检查点到下一个 UTC 分位带边界至多 `band_update_ms`,之后两个平滑窗填满、再过一个心跳边界,外加一个模型 tick 的取整([Model 检查点](#model-检查点));默认 1 871 000 ms ≈ 31 min。`band_update_ms` 小于平滑窗的参数角点上该式偏小(分位带一致之后平滑窗才开始一致),修正策略研究后定。`cold_start_ms`:从空状态到就绪需要分位覆盖 ≥ `pct_min_coverage` 且资金费窗口内至少一期已结算;默认 42 h,预热注入会缩短它(结构参数改动的预览用它,SDK)。

## 测试与验收

- **Model 金样**(定点结果与雏形 f64 参考值比较,相对误差 ≤ 1e-6):雏形 `score.rs` 的再分配用例(无超额不变、单币超额同池再分配、级联、多轮饱和、受限 group 用自身上限)按同样输入移植;单币打分(同号/异号、期数 < 5、最差期为负)、幂律分配 + 迭代封顶、基差加成资金费因子(每日 `F` 3 bp → 0.325;46 bp → 1.0)、分位带加宽到上限;按步数二分与逐步加宽的分位带逐例相同。
- **Model 属性**:平滑与池预算之后每池 `Σ target ≤ A_s'`、全部 `≤ D`;`0 ≤ target_i ≤ cap_i`;被阻塞仓位超过池额时其余 group 为 0、无负值;`F_i ≤ 0` 或 `NEG_FUNDING` 的 group 不在活跃集、target 为 0;未就绪 group 的 target 为 0 或 `HOLD`;`force_exit` 的 group target 为 0;`best_other_F_i` 只取同一现货池;相同输入两次运行 Advice 字节相同;`close_gain_daily_i ≥ max(0, −F_i)` 恒成立;`FundingSummary.flags` 含 `TRUNCATED` → `FUNDING_TRUNC` 且不就绪(构造 10 min 周期的结算序列:48 h 窗口内 288 期 > `FUNDING_RING`),1 h 周期、48 h 窗口、环已满 256 期 → 不置。
- **分位带与发送(UTC 对齐)**:分位带只在 `band_update_ms` 的 UTC 整数倍边界之后的第一个模型 tick 重算;在不同时刻启动的两次运行,喂入同一段样本后,边界之后的分位带逐位相同;心跳只在 `advice_refresh_ms` 的 UTC 边界之后发全部 group;`target` 相对变化 < `advice_min_change` 且阈值与标志不变 → 不发。
- **成本门**(费率取 refdata 金样中 core venue 的默认档费率;下例按永续 taker 5 bp、现货 taker 10 bp、两腿价差各 2 bp:`open_cost` = 32 bp、`close_cost` = 17 bp):开仓 `F` = 16 bp/日 → 回本 2 天、倍数条件 32 ≥ 32 → 放行,15.9 bp/日 → 倍数条件拒;平仓 `F_i = −4 bp/日`、同池无其他活跃 group → `close_gain_daily` = 4 bp/日,17 × 1天 ≤ 5 d × 4 → 放行(倍数条件不适用于平仓),`F_i = −3 bp/日` → 拒(由负资金费退出兜底);`gain_daily = 0`、`one_shot ≥ close_cost` → 放行,`one_shot < close_cost` → 拒;`one_shot = 0` 时平仓门与 `close_cost × 1天 ≤ max_break_even_ms × gain_daily` 逐例一致(属性测试);i128 中间量在 `max_break_even_ms` 取上限 60 d 时不溢出。
- **平仓收益(负资金费)**:同池活跃集为空、`F_i = −10 bp/日` → `close_gain_daily = 10 bp/日`;`F_i = −10 bp`、同池 `best_other_F = 30 bp` → 40 bp;另一池的 `F = 50 bp` 不计入;`F_i = 20 bp`、同池 `best_other_F = 15 bp` → 0,只有 `one_shot` 能放行。
- **负资金费退出**:`N = 3` 时连续 3 次负费率结算 → 目标 0/0、`reason = Exit`,成本门、分位与结算前保护都不通过时照样发出,`Note{NegFundingExit}`;负、负、正 → 不触发且 `neg_streak` 复位;`neg_funding_exit = false` → 从不触发(计数照常);完成后 `reentry_block_ms` 内与 `neg_streak ≥ N` 期间开仓闸门拒绝,一期非负结算后放行;跳变 ≥ 2 个周期只计一次;第 2 次负结算后重启 → `neg_streak` 从快照恢复,第 3 次触发;Advice 过期时照样触发;`clear_block` 不清 `neg_streak`;`neg_funding_exit_periods` 3 → 4 与 `neg_funding_exit` true → false 在 schema JSON 中都判为放宽(`relax`)。
- **有仓位**:只剩一腿且其名义 ≤ 尘埃线,或低于最小下单量 → 视为无仓位:`force_exit`、负资金费条件与 `exit_all` 都不设目标;`MINORDER_RESIDUAL` 置位后 1 000 个 `Bbo` 与 `Timer{SWEEP}` 内 0 条 `ExecDecision`、0 条 Note,任一腿净量变化后标志清除。
- **强制退出与命令**:`force_exit` 0 → 1 → 目标 0/0、`reason = Exit`,成本门、分位与结算前保护都不通过时照样发出;阶段为 `Trim`、`Stress`、`Fix{Manual}`、`Overridden` 时同样进入 `Exit`;外部成交使仓位重新出现 → 再次设 0;1 → 0 → 恢复常规评估。`exit` 与 `exit_all`(`reduce_only`)在 CloseOnly 与 DRAINING 下照常生效,产生的目标每腿都更接近 0;`exit_all` 在 64 个 group(`max_groups_per_instance`)时同一事件发完,同一事件的 Ctx 目标容量已被部分占用时余下的经 `Timer{SWEEP}` 续发;`Broken` 的 group 只设封锁;完成后在 `reentry_block_ms` 内开仓闸门拒绝,`clear_block`(升级类)后放行;`exit`/`clear_block` 带 `GROUP_NONE` 与 `exit_all` 带 group → `Note{CmdRejected}`(v1 = 1 / 3);命令载荷经 `params::encode_cmd` 构造(`urgent` 两种取值各一例)。
- **待完成退出**:`exit` 进行中进入 L2 → 不进 `Stress`,保持 0/0 并升为 `Urgent`,完成后封锁照设;`exit` 进行中 `External{Manual}` → 冻结;人工把两腿都平掉 → 自动解冻时按 `exit` 完成规则设封锁,之后不开仓;仍有仓位 → 回到 `Exit{Cmd}` 继续;`Exit{Delist}` 被 L3 接管 → 立即重设的 0/0 被执行层接受,L3 回落后仍有仓位 → 继续 `Exit{Delist}` 并按时升级 `Urgent`;`Exit{NegFunding}` 进行中 Broken → 解除后继续;`force_exit` 改回 false → `Param` 来源清除;`clear_block` 不清待完成退出;重启后待完成退出从快照恢复。
- **CloseOnly 持有**:有仓位时 `SetMode → CloseOnly` → 增仓目标被清、无新增仓、**不出现 `Exit` 目标**;常规减仓、超配减仓与强制逐步减仓照常;回到 `Live` 后恢复开仓。
- **超配**:`actual = cap × 1.15`(`overweight_tol` 0.10)、退出分位与退出分都不通过 → 设目标 = `cap`,只受平仓成本门;`actual = cap × 1.05` → 不动;`HOLD` 时超配照常减到 `cap`;平仓成本门不通过 → 不减并计入 `GateStats` v5。
- **强制逐步减仓**:`open_allowed = false`、平仓成本门不通过 → `close_only_rounds` 轮(每轮 `stress_step_ms`)之后第一步目标 = 0.75 h、`reason = Rebalance`、`Note{TrimStep}`,此后每轮一步,剩余 < `max(min_target, slice_min_chunk)` 时 0/0;条件中断一轮 → 计数清零;中途 `open_allowed` 改回 true → 停止、回 `Idle`;`actual = cap × 1.25`(> 1 + 2 × 0.10)且平仓成本门不通过 → 3 轮后逐步减到 `cap`,回到 `cap × 1.10` 以内停止;`actual = cap × 1.15` → 只走常规超配减仓(受成本门);L2 期间不出 `Trim` 步;`force_exit` 优先;已有更接近 0 的减仓目标时不替换。
- **永续硬上限与杠杆**:实际杠杆 5x(仍在风控上限内的测试配置)时反复开仓,永续名义停在 `margin_balance / 0.40`、`Note{PerpEquityCap}`;两个 group 同一事件内先后开仓、第一个目标未成交时,第二个的可增量扣除 `committed_perp`,合计不越过上限;2x 时停在交易所占用(e ≈ 0.5)、硬上限不触发;`Views::leverage` 为 `None` → 不开仓、`Note{MarginShort}` v3 = 2;`perp_margin_frac` 0.2 而实际杠杆 2x → 按 0.5 估算。
- **被保留的退出目标**:`Exit`/`Hard`/`MarginStress` 目标收到 `Stuck(SliceFails)` 或 `Stuck(Gated(非 LINK_STALE))` → 不清除,`fail_cooldown_ms` 后重发:`tag` 序号 +1、其余字段相同,执行层按替换处理(`ExecDecision{TargetReplaced}`)并清零失败计数,之后的切片照常发出,`Note{ExitRetry}`;`Exec{Progress}` 清 `retries` 不清 `tag_seq`,连续重发的 `tag` 都不同于当时的生效目标(含序号回绕);`exit` 命令目标重发时保留 `urgent` 与封锁时长;`Rebalance` 目标同样情况 → 清除并退避;`Stuck(MinOrder)` 在 0/0 退出目标上 → 退出视为完成、封锁照设、置 `MINORDER_RESIDUAL`。
- **LotMismatch**:增仓配对目标被拒 → 停止该 group 的增仓并 `Note{LotMismatch}`;此后 pipeline 连续 10 个参数版本只改 `liq_score` → 不再重复该 Note;改该 group 的 `slice_max_chunk` → 标志清除;同一 group 的减仓、`Exit`、`Hard`、`MarginStress` 目标被执行层接受(执行层用例归 hft-exec)。
- **外部成交**:`External{Manual}` → `Fix{Manual}` 冻结、`clear_target`、`Note{ManualExternal}`,之后 `Bbo`、`Risk{MarginStress L2}`、负资金费条件、强制逐步减仓都不产生策略目标;执行层置 `manual_hold`,冻结期间不出补腿与外来成交默认处理单(本策略的 `clear_target` 不解除它);两腿被人工对齐后自动解冻;`clear_block` 解冻(两腿差 > `fix_tolerance` 时先按较小一腿对齐);`exit` 命令直接进入 `Exit`;`External{Liquidation}` → 截止前对齐或退出、再开仓封锁;`External{Unknown}` → 对齐到较小一腿。
- **目标的设与留**:同方向 `Rebalance` 目标生效期间 1 000 个 BBO、目标量在带内 → 0 条新的 `ExecDecision{TargetSet|TargetReplaced}`;目标量超出带、原因或场景码变化 → 恰好一次重设;确认窗口 `Confirmed` 后重置不引起设/清抖动(复查用即时条件)。
- **状态机**:L2 期间 `Exec{Done}` → 保持 `Stress`,`stress_step_ms` 后下一轮;某 venue 降到 L1 → 该 venue 的 group 退出 `Stress`;无 `CapCleared` 时 `margin_level < L2` 由 `Timer{SWEEP}` 退出;`CapCleared{Delisting}` → `Exit{Delist}` 撤销、恢复评估;`fail_until_ens` 未到 → 开仓、常规减仓与强制逐步减仓都不设目标;每个回到 `Idle` 的转移都先查待完成退出。
- **资金口径**:多个 group 争用同一现货池时 `committed_spot`(由 `TargetView` 推导)生效;目标被清除或完成后已承诺额随之消失(无残留);重启后不依赖快照中的承诺额。
- **报价健康**:两腿盘口 10 s 不变、连接存活帧每 1 s 到达 → 闸门 3 通过、目标照常推进(DST 同一必过用例的策略侧);连接静默超过 `RiskStateView.conn_liveness_ms`、`resync` 或连接非 Live → 闸门 3 拒绝并计入 `GateStats` v2。
- **重启与时间轴**(SDK `MockViews::restart`):冷却(`fail_until_ens`)、强平后的再开仓封锁、命令封锁、`Stress` 与 `Trim` 的续发时刻、`Fix`/`Exit` 截止、被保留目标的重发时刻在停机 Δ 后按 engine_ns 继续(停机不计),TSC 重新计数不影响;维护窗口按 UTC 判定;重启前的 Advice(`basis_seq < boot_seq`)一律过期,确认窗口已 `reset`、触发价已失效;快照字节中不出现 TSC。
- **保证金压力**:L1 → 清全部增仓目标;L2(`venue` = 永续 venue)→ 只对永续腿在该 venue 的 group 按 `entry_score` 升序减仓、`reason = MarginStress`、首轮在 `deadline_tsc` 前发完,此后每 `stress_step_ms` 一轮直到回落;现货 venue 的档位事件不触发减仓;L3 → 收到 `Stuck(EngineOverride)` 后只发每腿更接近 0 的目标;64 个 group 的 L2 首轮在同一事件内发完(Ctx 目标容量被占用时经定时器续发,仍在截止前发完)。
- **预热**:金样载荷(`fixtures/strategies/portfolio-carry/warmup/`,与插件包共用)解码;42 h 的 `BasisWarmup`(`src = 1`)+ `FundingWarmup` 注入、并收到一条实时 `Funding` 后,下一个模型 tick 即 `READY`;同一批块乱序、重复、与实时样本交错注入,Advice 字节相同;`step_ms` 非 `pct_sample_ms` 整数倍或超过 `warmup_max_step_ms` → 整块忽略;未知 group → 忽略并计数;`src = 2` → `WARM_APPROX` 直到这些样本滑出窗口。
- **参数应用与重置**:改 `liq_score`、阈值与权重 → `None`,统计与路径状态逐位不变;改 `pct_bin` → `ResetAll`,资金费环保留、分位环按新配置重建;新增 group → `ResetGroups` 含它;同一 `GroupId` 指纹变化 → 只重置该 group;检查点中 `PctConfig` 与当前参数不一致 → 该 group 重置;重复调用同一参数结果不变(幂等)。
- **检查点**:原始样本节的布局描述金样(`ckpt/layout.json`,`ModelLayoutDesc`)与实现一致;路径金样(`ckpt/path.bin`)与固定事件脚本跑出的 `snapshot_path` 字节相同,路径节写出的元素与[路径状态节](#model-检查点)表一致(`ckpt_layout.rs`);两节都可用时,在任意时刻(分位带边界前后、平滑窗未满、心跳边界前后)检查点 → 恢复 → 续算的 Advice 与 Model 状态都与连续运行逐字节相同(SDK 确定性夹具 `keep_path = true`);`keep_path = false` → `Note{ModelPathReset}`、原始样本保留、第一个模型 tick 发全部 group,依据时刻在检查点之后 `path_settle_ms` 以外的 Advice 与连续运行逐字节相同(含检查点恰在分位带边界之后的最坏情形);只改打分或分配逻辑后旧检查点的原始样本节被接受(路径节随 `PATH_VERSION` 提升丢弃一次);改参数值或 `schema.version` 后重启 → 接受、不重新等待;`snapshot_chunk` 以任意 `max_bytes`(含小于单 group 大小)分块写出的字节与一次写出相同;实际分配与两节写出大小之和 ≤ `mem_bytes`/`snapshot_bytes`(属性测试,覆盖参数范围的角点,含资金费环写满 256 期、平滑窗取上限)。
- **快速部分场景**(SDK `MockViews`):每个闸门的通过与失败;Advice 的 `basis_tsc` 超过 `advice_max_age_ms`(到达时刻是新的)→ 不调仓;**新开仓与目标降为 0 的常规减仓都要过成本门**;Broken → 收到 `Exec{Progress}`(`origin = Engine(AutoFix)`)不改阶段 → `Done{cleared_by_engine}` 回到 `Idle` 并按新的实际仓位评估,`open_blocked` 时只走平仓;`Stuck(Gated(LINK_STALE))` 保留目标、`Gated(LINK_FUNDS)` 清 `Rebalance` 目标并 `Note{LinkFunds}`;`TargetRejected(ForcedReduceActive | FlattenLocked)` → `Overridden`、`Exit` 目标仍被接受;每个登记的 Note 码至少触发一次(`CtxHarness` 校验码与级别);结算前窗口两个方向;资金费过期、Advice 过期、分位未就绪 → 不开仓;`HOLD` 不做常规调仓;资金不足截断与 `Note`;下架 → `Exit` 并按时升级 `Urgent`;维护窗口;`Mode` 降级与恢复;`Exec{Stuck}` 各原因 × 目标原因。
- **引擎场景**(`strategies/registry/tests/portfolio_carry_scenarios.rs`,hft-testkit 驱动,共用撮合器):开仓 → 两腿成交 → 目标下降减仓 → 退出;`exit` 与 `exit_all` 命令 → 两腿清零;手续费以基础币扣除时两腿按净量一致;两腿差不超过容差持续超过对冲时延;第二腿盘口 10 s 不变时对冲腿照常发出;连续负资金费结算 → 自动退出;退出目标连续被拒 `max_slice_fails` 次后重发 → 执行层替换目标并继续出切片。
- **DST**:`cargo xtask dst --strategy portfolio-carry`(目标在 `strategies/registry/tests/`),故障注入(断连、Unknown、限速、强平/ADL、人工外部成交(之后引擎不补腿)、保证金数据过期、下架命令与撤回、退出中途的 L2 与重启、重启与换机)。
- **模拟账户**(`sim` 实例,默认 ≤ 32 group):按[上线门槛](../../docs/testing.md#8-上线门槛) ⑦,含[保证金压力](#保证金压力)一节的压力用例(门槛零强平、L3 路径变体必过、非门槛变体记录);参数变化是否重新计时按参数表的 ⑦ 列([小改属性](#小改属性));每日 `--advice recompute` 从当日归档的 Model 检查点起算(`replayer verify --model-checkpoint`,规则归 testing;路径节被丢弃的当日按 [Model 检查点](#model-检查点)给出的起点比较);每日报告 = 通用项(由引擎事件算出:每 group 日换手、手续费/资金费比、最大保证金率、两腿间隔等)+ [报表字段](#note-码表与报表字段)。**验收不设收益门槛**;报告用于研究规则。代码改动后,同参数模拟账户 ≥ 3 天无 P0、无未解释 P1 才发布到实盘账户(暂定)。
- **正式实盘**:全部上线门槛通过后,在统一升级主机上的新子账户开始([launch](../../docs/runbooks/launch.md));风险上限由运营在 `RiskParams` 设定(建议从保守值起步,不是门槛);永续 venue 的实际杠杆 ≤ `RiskParams.max_perp_leverage_x100`(引擎核验),推荐 2x(见[资金口径](#资金口径));同参数模拟账户与正式实盘并行校准(滑点、成交率、延迟)。

## 依赖

- 内部:`hft-strategy-sdk`(含 `model_math`)、`hft-strat-lib`(`PercentileStats`、`FundingStats`、`ConfirmWindow`、`TriggerPx`、`Expiring`)。
- 外部:无。
- 禁止:任何引擎 crate、其他策略 crate、I/O、系统时钟;快速部分禁 f64。插件包的依赖见 [pipeline/README.md](pipeline/README.md)。

## 延伸阅读

- 雏形:`E:\trading\Trading\portfolio-arb\docs\strategy_spec.md`、`src\score.rs`(打分与分配)、`src\reconciler.rs`(闸门链)、`src\params.rs`(默认值)、`src\band_update.rs`(分位带)、`src\funding.rs`(资金费统计)
- [评审 §1、§2、§6](../../design/_work/review-2026-10-02-主策略适配与执行正确性.md)、[第二次独立评审](../../design/_work/review-2026-10-03-M0b独立评审.md)、[解耦与策略评审](../../design/_work/review-2026-10-03-M0c-V2解耦与策略.md)、[第四轮解耦与策略评审](../../design/_work/review-2026-10-03-M0d-W2解耦与策略.md)、[第八轮裁决](../../design/_work/round8-decisions.md)(其中 R8-12:本文标"策略研究后定"的各处是第五轮评审 X2 中暂缓到策略研究的已知不一致)、[第五轮解耦与策略评审](../../design/_work/review-2026-10-03-M0e-X2解耦与策略.md)、[07 策略层](../../design/07-策略层.md)
- Agent Note:[保证金按到强平距离分档](../../.agents/notes/proposed/architecture/2026-10-03-margin-liq-distance.md)、[报价新鲜度 = 连接健康](../../.agents/notes/proposed/architecture/2026-10-03-quote-freshness-conn-health.md)、[引擎时间 engine_ns](../../.agents/notes/proposed/architecture/2026-10-03-engine-time.md)、[Model 检查点的路径状态节](../../.agents/notes/proposed/architecture/2026-10-03-model-path-section.md)、[人工持有与停机标记](../../.agents/notes/proposed/architecture/2026-10-03-manual-hold-and-stop-marker.md)
- [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md)、[hft-exec](../../crates/hft-exec/README.md)、[hft-strat-lib](../../crates/hft-strat-lib/README.md)、[hft-risk](../../crates/hft-risk/README.md)(MarginStress 档位、实际杠杆上限)、[hft-ledger](../../crates/hft-ledger/README.md)(权益与净量)、[hft-refdata](../../crates/hft-refdata/README.md)(`VenueCaps`)、[hft-sim](../../crates/hft-sim/README.md)(模拟账户与压力用例)
- [pipeline 插件包](pipeline/README.md)、[cookbook:新增策略](../../docs/cookbook/adding-a-strategy.md)、[launch](../../docs/runbooks/launch.md)

## 已知限制与后续工作

- 只做"空永续、多现货";反向(需要现货借币)不做。
- 流动性分来自插件包的 per-group 参数(分钟级以上),不在热进程内计算。
- 跨 venue 资金不自动划转:资金不足只告警,划转建议由 treasury 给出。
- 一腿挂单(先挂现货、成交后吃永续)可降低成本但有逆向选择:可选扩展,触发条件 = 模拟账户报告显示 taker 成本占收益比例过高,且 `MakerFirst` 已在 hft-exec 规格化。
- 负资金费的处理:持续为负的 group 由默认生效的负资金费退出(N 期)平掉,短于 N 期的负资金费经平仓收益(`close_gain_daily`)推动常规平仓;人工可用 `force_exit`、`exit` 或 `exit_all`。N 与是否按负资金费幅度加权在模拟阶段研究。
- 超配在 `cap × (1 + 2 × overweight_tol)` 以内只受平仓成本门约束:资金费在本池最高且基差未回落(无一次性收益)的币可能长期停在这个区间。超出它或 `open_allowed = false` 持续 `close_only_rounds` 轮后按[强制逐步减仓](#强制逐步减仓)不计成本地减,代价是可能付出高于收益的平仓成本;需要立即降下来时用 `exit`。
- 永续硬上限只统计本实例 group 中的永续 inst;同一 venue 上不在任何 group 里的永续持仓(新子账户上不应出现)不计入,由对账与引擎 L3 覆盖。
- Model 目前不推断快速部分的命令封锁、`LotMismatch` 与人工冻结,这些 group 的份额在封锁期间闲置(开发备忘 9)。
- 不接管旧仓位:新子账户上线。
- 普通重启、改非结构参数、换 refdata、发新版都不丢 168 h 统计:advisor 从 [Model 检查点](../../crates/hft-strategy-sdk/README.md#model-检查点)恢复 `CarryModel` 并从检查点的 `last_seq` 重放 journal 追上(检查点键不含参数、refdata 与二进制版本;原始样本节不含派生状态,改策略逻辑不使它失效;路径状态节使续算与连续运行一致,`PATH_VERSION` 提升只丢这一节)。只有**首次启动**、`LAYOUT_VERSION` 提升、检查点缺失或损坏时全部从空状态起步;结构参数热更、新加入或指纹变化的 group 按 group 从空起步;这些情况都由插件包注入[预热载荷](#预热载荷),注入完成即可就绪。既无检查点又未注入时,分位未就绪期间(默认 `pct_min_coverage` 下约 42 h)不可开仓,已有仓位按 `HOLD` 维持,风险类与规则退出不受影响。

### 开发备忘

<details><summary>给实现者</summary>

**开放问题**

1. **预热数据来源的取舍**(通道与载荷已定,见[预热载荷](#预热载荷)):`src = 1` 来自 collector 或 journal 归档的两侧报价(只覆盖录过的 venue 与时段);`src = 2` 来自交易所 1 分钟 K 线(只有成交价,开/平两侧相同,雏形把每根展开 12 次即这种偏差;这里同样按 `step_ms` 覆盖 12 个采样点,并以 `WARM_APPROX` 标出)。`WARM_APPROX` 期间是否允许开仓、K 线样本是否加半个价差修正,在模拟账户上比较两种来源后定。
2. **平仓收益与负资金费**:`close_gain_daily = max(0, max(best_other_F, 0) − F_i)` = 同池换仓后可得的资金费(不低于 0)加上本 group 停付的负资金费,|F_i| 只计一次;负资金费退出的 N(默认 3)、是否按费率幅度提前退出,在模拟账户上观察后定。
3. **已实现资金费的来源**(待核实,每个 core venue 分别核实):"结算前最后一次观测到的费率 = 该期已实现费率"是否成立。核实方法:模拟账户运行 ≥ 7 天,把 Model 记录的每期费率与该交易所资金费历史接口逐期比对;不一致的 venue 改用结算事件或历史接口(经 venue 层成为 journal 事件),负资金费退出的计数随之改用同一来源。
4. 开仓成本门的两个条件中,按雏形默认值起作用的是倍数条件(相当于回本 ≤ 2 天),是否保留两条、如何取值在模拟账户上研究。
5. `band_min_gap` 是常数(雏形按一组固定费率取 14 bp);是否改为按两腿实际费率计算在模拟账户上研究。
6. **杠杆与硬上限的标定**:`perp_min_equity_ratio`(默认 0.40)、`max_perp_leverage_x100`(默认 3x)与 `deployable_frac`、分档阈值的组合在模拟账户上复核;维持保证金率按分档取值后重算[保证金压力](#保证金压力)的估算表。
7. `exit_score` 是否保留(雏形为修特定币的问题逐步加入);`hedge_cross`、`hedge_max_slip`、`slice_max_cross`、`stress_reduce_frac`、`stress_step_ms` 用模拟账户与正式实盘的并行校准数据标定。
8. **待核实**:各 core venue 资金费推送是否只在费率变化时推送;若是,`funding_max_age_ms` 会在费率不变时误判过期,需改为按该连接健康判定(同报价健康)。核实方法:服务器上录 24 h 资金费推送,统计相邻推送间隔与费率是否变化。
9. **Model 感知封锁**:命令封锁、`LotMismatch`、人工冻结只在快速部分。SDK 允许 Model 订阅本实例的 `Exec`/`Order`/`Manual`(只读、来自 journal、确定);是否用它推断这些封锁并把份额让给其他 group,视模拟账户的资金利用率再定(推断规则须与快速部分逐条一致,否则两部分分叉)。
10. **强制逐步减仓的节奏**:与 L2 共用 `stress_step_ms`、`stress_reduce_frac`,开始前的持续轮数为 `close_only_rounds`;是否给它独立的步长与比例、`open_allowed = false` 是否应比超配更慢,视模拟账户中 `TrimStep` 的频率与成本再定。

**从雏形中不照搬的缺陷**(评审 §6)

| 雏形缺陷 | 本策略与框架的处理 |
|---|---|
| 下单超时/发送失败当普通错误按原数量重下;两腿都报错当两腿都未成交 | 策略不直接下单;订单的 Unknown 语义与重试归 OMS/执行层 |
| 两腿数量不同时取 min 记账;现货按 USDT 金额买入、价格走动即少买;对账只改记账不补腿 | 目标按资产数量给两腿;ledger 按净量记账;两腿差由 LegTracker 与执行层补腿;对账走 reconciler |
| 没有保证金/强平/ADL/下架保护;平仓必须等分位条件,风险退出可能被无限期挡住 | 响应 `MarginStress`(按到强平距离分档)、`External`、`Notice`;风险类与 `Exit` 目标不经过分位与成本门;人工可强制退出单个 group 或整个实例 |
| drift_override 使成本-机会门几乎失效(新开仓与清零全部绕过) | 成本门没有绕过开关;只有 `Exit`/`Hard`/`MarginStress` 原因不经过 |
| 回本天数 = 成本 ÷(每日收益 + 一次性收益),量纲混用;平仓也按开 + 平往返成本算 | `cost_left × 1天 ≤ max_break_even_ms × gain_daily`,一次性收益先抵成本;平仓只算平仓两腿;倍数条件只用于开仓 |
| 总资金口径混用(推送更新不含未实现,快照含未实现;代币市值 5 min 更新) | 统一读 `PortfolioView`(保证金余额 = wallet + 未实现,现货按 bid 实时估值) |
| 各币目标之和可超过可部署资金;总名义上限配置未被读取 | 平滑之后按池与 `D` 缩放,并有属性测试 |
| 调参工具与实盘公式不一致、同一数据选参又打分、热加载不做范围校验直接取第一名 | 不做自动寻优与热加载;参数经 `SetParams` + schema 范围校验;研究在模拟账户上做 |
| 分位分布按 mid 采样却用买/卖两侧价差查询;预热用 1 分钟收盘价展开 | 开仓侧与平仓侧分别采样、分别查询;预热经通用注入通道进入 journal(可回放),载荷区分两侧报价与 K 线近似并以 `WARM_APPROX` 标出 |
| 负资金费时平仓收益只算换仓机会,没有其他币时为 0,平仓门可能永不放行 | `close_gain_daily` 计入停付的负资金费;另有默认生效的负资金费退出 |
| shadow 模式未实现;资金费数据无新鲜度门 | 模拟账户替代 shadow 评估;`funding_max_age_ms` 新鲜度门 |
| 资金费高时入场分位最多下沉 30 点、−200 bp 底线写死 | 两者都是参数(默认保留雏形值,在模拟账户上研究) |
| 受限名单与单币上限写死在代码里 | per-group 参数 `restricted`、`coin_abs_cap` |
| 100 ms 轮询与墙钟持续性 | 事件驱动,持续性按事件时间计算(确定、可回放) |

**实现顺序**:参数与 group 校验 → Advice 载荷 → Model(资金费统计 → 分位与带 → 打分 → 分配 → 发送)+ 金样 → 参数应用与重置、检查点两节(原始样本节布局与分块、路径状态节) → 快速部分闸门 → 风险响应 → 强制退出与待完成退出、负资金费退出、强制逐步减仓与策略命令 → 场景测试 → 基准。
</details>
