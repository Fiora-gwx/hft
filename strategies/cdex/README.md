---
description: "可选策略包:CEX-DEX chain-first 携带策略 Cdex(加密币档 = 07 中的 D 类,股票代币档 = E 类)及其专用构件(Sizer、ClosePolicy、FundingGate、SwapPicker、carry 线、价差样本统计与允许开仓规则);cdex 从旧框架迁移到新框架时启用"
kind: crate
tier: hot
scope: optional
status: spec
---

# cdex

## 概要

本 crate 是现有 Python cdex 引擎(`spread_engine.py`,9/24 起正式 LIVE)的规格化:**规则与数字是 9/30 现行值,经两个月实盘校准,原样保留,只改接口与归属**。它提供一个策略类型 `Cdex`:每个 group = CEX USDT 永续空 + 链上池现货多,链上腿先成交(chain-first),CEX 腿按链上实际成交量对冲;group 的 `profile` 区分加密币(D 类)与股票代币(E 类)——两者共用同一个 CEX 账户与同一个链上钱包,而一个账户只绑定一个策略实例,所以是同一策略的两个档位。快速部分 `Cdex` 在 sequencer 线程做价差触发、确认、三条开仓通道、下单、平仓、硬动作、残余清理与手动任务;慢速部分 `CdexModel` 在 advisor 线程做价差样本统计、平仓线(ClosePolicy)、硬动作价位、冲击探测与换仓候选,输出本策略自定的 Advice 载荷。它**不拥有**第二腿与订单状态机([hft-oms](../../crates/hft-oms/README.md) 的 LegLink、[hft-exec](../../crates/hft-exec/README.md))、风控、账本、链上交易生命周期([chain-core](../../apps/chain-core/README.md)、[hft-chain-evm](../../crates/hft-chain-evm/README.md))与名单/池子/成本线的生成(同目录的 [pipeline 插件包](pipeline/README.md),由 [pipeline 框架](../../python/pipeline/README.md)调度)。**触发条件**:cdex 从旧框架迁移到新框架时(cdex 现在旧框架照常运行,迁移不急)。

## 目录

- [使用本组件](#使用本组件)
  - [触发条件与启用前提](#触发条件与启用前提)
  - [装配与策略接口](#装配与策略接口)
  - [组与订阅](#组与订阅)
  - [事件处理](#事件处理)
  - [意图](#意图)
  - [参数 schema(CdexParams)](#参数-schemacdexparams)
  - [Advice 载荷](#advice-载荷)
  - [对引擎事件的响应](#对引擎事件的响应)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 触发条件与启用前提

| 项 | 内容 |
|---|---|
| 触发条件 | 决定把 cdex(CEX-DEX 携带 + 股票代币)从旧 Python 框架迁移到新框架 |
| 前提组件(均为 optional,同一触发条件启用) | [chain-core](../../apps/chain-core/README.md)、[hft-chain-evm](../../crates/hft-chain-evm/README.md)、[hft-ext-proto](../../crates/hft-ext-proto/README.md)、[hft-amm](../../crates/hft-amm/README.md)、[contracts](../../contracts/README.md)(SwapGuard)、[external-venue-protocol](../../docs/subsystems/external-venue-protocol.md) |
| SDK | [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) 的 `chain` 特性(`PoolView`/`EventRef::Pool`/`SubsBuilder::pool`、`SwapIntent`、`SwapUpdate`、`EventRef::Swap`、`Ctx::swap`)与通用接口 `Ctx::request_halt`、`EventRef::Manual`(StrategyCmd)、`EventRef::Warmup`(预热注入)、`PerpPosView.liq_px/adl_quantile` |
| 慢路径 | 本策略的 [pipeline 插件包](pipeline/README.md)(名单、池子核验、成本线、`open_th`、`armed`、规模因子、资金费结构性条件;冷启动时的价差样本预热) |
| 运维 | [runbooks/migration](../../docs/runbooks/migration.md)(cdex 迁移手册:旧引擎停开收尾 → 新实例对账接管) |
| 账户 | 一个 CEX(子)账户 + 一个链上钱包,只绑定本策略的一个实例([hft-refdata AccountMeta](../../crates/hft-refdata/README.md) 的 `bound_instance`);测试/正式是两个实例 |

### 装配与策略接口

登记在 [strategies/registry](../registry/README.md) 的列表里(已有一行,`#[cfg(feature = "cdex")]` 门控);启用 = 发布构建打开 `cdex` feature,app 与引擎 crate 零改动:

```rust
#[cfg(feature = "cdex")] cdex::Cdex,     // strategies/registry 的 strategy_set! 列表中的一行
```

```rust
pub struct Cdex {
    inst: StrategyInstance,
    groups: Box<[CdexGroup]>,          // 按参数里的 group 数在 new/on_params 时分配(命令路径);gix 为下标
    acct: AcctAgg,                     // 账户级增量汇总:held_sum、inflight_sum、capital_in_use、utilization
    p: CdexParamsNs,                   // 时长参数已换算为 ns(engine_ns);DirtyFilter 的两项另换算为 tsc
    swap_gate_until_ens: i64,          // 换仓:10 min 内最多一次(engine_ns)
}
impl Strategy for Cdex {
    const DESC: StrategyDesc = StrategyDesc { name: "cdex", kind_id: 2, version: 1, non_prod_only: false,
                                              schema: &CdexParams::SCHEMA, notes: &NOTES, report: &REPORT, cmds: &CMDS };   // src/desc.rs
    type Params = CdexParams;
    type Model = CdexModel;
    fn new(inst: StrategyInstance, p: &CdexParams, groups: &GroupTable, rd: &RefView<'_>) -> Result<Self, ParamError>;
    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder);
    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>);
    fn on_params(&mut self, p: &CdexParams, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError>;
    fn snapshot(&self, w: &mut SnapshotWriter);
    fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}

pub struct CdexModel {
    per_group: Box<[ModelGroup]>,      // PercentileStats、HourlyMax、ClosePolicy 输入缓存、ImpactCurve 状态
    funding: Box<[FundingStats]>,      // 每个 CEX 永续 inst 一个(hft-strat-lib)
    swap: SwapPicker,
}
impl Model for CdexModel {
    type Params = CdexParams; const LAYOUT_VERSION: u32 = 1;
    const LAYOUT: &'static ModelLayoutDesc = &LAYOUT;   // 原始样本节。实例级节:FundingStats;Group 节:GroupId、指纹、PercentileStats::LAYOUT_ELEMS、ImpactCurve
    const PATH_VERSION: u32 = 1;                        // 路径状态节:每 group 上次看到的 last_fill_ens 与 MarginView.src_ens
    const WARMUP: bool = true;                          // 有预热载荷(价差样本),ResetPreview 按预热估算不能开仓时长
    /* mem_bytes / snapshot_bytes / path_settle_ms / cold_start_ms / new / on_params / on_event / snapshot_path / restore_path /
       snapshot_chunk / restore:签名见 SDK;取值见"价差样本统计" */
}
```

`kind_id = 2`、`schema.id = 2`,登记于 [strategies/registry](../registry/README.md)(只追加)。`CdexParams` 用 SDK 的 `define_params!` 声明(下文参数表即其字段),跨字段校验写在 `check` 里挂入 `validate`。

### 组与订阅

每个 group 由 `StrategyParams` 的 `GroupSpec` 定义(引擎理解的部分,[hft-event](../../crates/hft-event/README.md)):

| 腿 | `LegDef` | 说明 |
|---|---|---|
| `leg 0` Fast | CEX USDT 本位永续 inst | 空头腿;价差的快侧 |
| `leg 1` Chain | 链上 venue + `route_id`(refdata `routes` 段下标,pipeline 核验过的路由) | 现货多头腿;价差的慢侧 |

- `GroupSpec.open_allowed` 是**引擎门**:false 时任何通道都不开(坏池、待移出名单)。是否"已验证"(现 ✅/🟡)是本策略参数 `armed`:未验证的 group 只能走 spike 与 carry 通道。pipeline 对仍可能走 spike/carry 的 👁 币下发 `open_allowed = true, armed = false`。
- 快速部分订阅(`SubsBuilder::market`):每个 group 的 Fast 腿 `BBO | BOOK | MARK | FUNDING`,其中 `BBO | MARK | FUNDING` 用 `Part::Both`(Model 也要);Chain 腿的链上报价用 `SubsBuilder::pool(gix, Part::Both)`(`EventRef::Pool`,按规模档的 `PoolView`)。本实例的 `Order`/`Swap`/`Exec`/`External`/`Risk`/`Mode`/`Advice`/`Manual`/`Timer` 按归属投递,`Notice` 与 `Conn` 按订阅的 inst/venue 投递;`Warmup` 只投递给 Model(不调用 `SubsBuilder::warmup`)。
- `CdexModel`:收上述 `Part::Both` 行情与链上报价、预热注入;`SubsBuilder::model_tick(model_tick_ms)`(10 s)作为时间刻度。Model 不订阅本实例的 `Order`/`Exec`/`Manual`(不调用 SDK `SubsBuilder::model_own`),`Swap` 不投递给 Model:成交与仓位变化从 `ModelViews::group_pos`(`last_fill_ens`、各腿入场价、已收资金费)读,保证金率从 `ModelViews::portfolio(..).margin(v)` 读,强平价与 ADL 档从 `portfolio(..).perp(v, i)` 的 `liq_px`/`adl_quantile` 读。
- 两腿天然不在同一交易所(CEX 永续 + 链上池),满足"不做同所两腿"的规则。

### 事件处理

| `EventRef` | 快速部分 `Cdex` 的处理 |
|---|---|
| `Bbo`(Fast) | `TriggerPx::hit_buy/hit_sell` 不命中即返回;命中 → 开/平评估(`from_slow = false`) |
| 链上报价(Chain) | 重算 `TriggerPx`;同样驱动一次开/平评估(`from_slow = true`,块号进 `ConfirmWindow`):链上报价也可能是"最后一个达标条件" |
| `Mark` | 与 `HardTriggers` 价位整数比较 → 硬动作;更新该 group 的 `held_usd` 与账户汇总 |
| `Funding` | 记实时费率与实际结算周期(`FundingView`),供 `FundingGate` |
| `Book` | 不评估;VWAP 只在开火前读 `Views::book` |
| `Order` / `Swap` / `Exec` | 推进 `in_flight`、`Throttle::on_outcome`、`ManualJob` 进度、`entry_mode` 与 `round_start`;`Rollback` → 进入并行回退期 |
| `External` | 自身强平/ADL/人工成交:见[对引擎事件的响应](#对引擎事件的响应) |
| `Risk` | `MarginStress`、`HedgeBroken`、`CapSet/CapCleared{Halted}`:同上 |
| `Notice` | 下架、结算周期变化、维护:同上 |
| `Timer` | 手动任务分笔、下架退出、保证金压力分步减仓 |
| `Advice` | 按 `local_kind` 解码进 `CdexGroup` 的 `Expiring<_>` 缓存(`Expiring::put_advice`:跨启动按 `basis_seq < boot_seq` 过期,启动内有效期从 `basis_tsc` 起算),重算受影响的 `TriggerPx` |
| `Manual` | `StrategyCmd` 载荷(`CmdView::parse`):`open` / `close` → `ManualJob`,见[手动任务](#手动任务);未知 `op` 不会到达(gateway 已按 `cmds` 校验) |
| `Warmup` | 快速部分不收(只投递给 Model) |
| `Mode` | 重置确认窗口与开仓状态(不清 `in_flight`、`consec_fail`);降到 CloseOnly/Shadow 时 Open 挂单由 sequencer 撤 |
| `Conn` | Fast 连接断 → `TriggerPx::invalidate`;恢复后等下一次链上报价重建。Fast 腿报价是否可用只看连接健康(SDK `BboView::quote_live`),盘口长时间不变不算过期 |

**同一事件对同一 group 最多产生一组开/平意图**。

### 意图

| 动作 | 意图 | 类别 | 说明 |
|---|---|---|---|
| 开仓(chain-first,默认) | `SwapIntent{side = Buy, exact = ExactIn, amount_in = 单笔, min_out 由 limit_px 反推}`,`link = OnFill → Fast 腿 IOC 卖,数量 = IncludedOk.amount_out 对齐 lot,px = bid × (1 − cex_slip)` | Open | CEX 对冲腿**不带 reduceOnly**(chain-first 的对冲在 CEX 上是开仓;引擎仍标 `RISK_REDUCING`,分类以 [hft-risk](../../crates/hft-risk/README.md) 为准) |
| 开仓(carry 并行,可选) | 同一事件内 `SwapIntent` + Fast 腿 IOC 卖,`link = Simultaneous{on_fail = Reverse}` | Open | 链上未成 → 引擎买回 CEX 腿;发生回滚后 `parallel_fallback_ms` 内该 group 回到 chain-first |
| 平仓 | `SwapIntent{side = Sell, amount_in = 平仓量, min_out 由 close_th + limit_tol 反推}`,`link = OnFill → Fast 腿 IOC 买,reduce_only = 1` | Reduce | 平仓链接 |
| 硬动作 `reduce_half` / `close_all` | 不设价差限制;腿顺序按 `hard_leg_order`(见实现说明) | Flatten | Shadow 下只记录 |
| 链上多出的一腿(补腿) | `SwapIntent{Sell, 多出的代币量}` | Flatten | CEX 多出的一腿由执行层自动补腿(阈值 = 本策略写进 `LegLink.tolerance` 的 `naked_fix_tol`) |
| 残余清理 | Fast 腿 IOC 买 reduce-only 平掉差额 | Flatten | 两腿都 < `residual_sweep_max` 且 CEX 空多于链上币 |

- `SwapIntent` 的字段与 `valid_blocks` 默认值以 SDK `chain` 模块与 [hft-chain-evm](../../crates/hft-chain-evm/README.md)(`free.max_blocks`,唯一归属)为准;`min_out` 由**信号价**反推,chain-core 只能收紧不能放宽(NAORIS 约束,[06 §2.4](../../design/06-链上执行层.md))。
- swap 未包含 = 零成本放弃,**不重试同一意图**;重试必须重新判断(AIN 复盘)。
- `tag`(u32,策略自定):bit 0–7 = `entry_mode`(1 normal、2 spike、3 carry、4 manual),bit 8–15 = 动作(open、close、reduce_half、close_all、chain_fix、residual、manual_open、manual_close),bit 16–31 = 手动任务序号。

### 参数 schema(CdexParams)

字段按 SDK `FieldDesc` 描述;**默认值 = 现行值**;"组"列 ✓ = `per_group`(pipeline 按币覆盖)。单位:Rate 1e-9(表中写百分比)、Notional USDT(表中写美元)、DurationMs(表中写可读时长)。来源列是现 Python 配置名或复盘日期。

**通道与阈值**

| 字段 | 单位 | 默认 | 组 | 含义 / 来源 |
|---|---|---|---|---|
| `profile` | Enum[crypto, stock] | crypto | ✓ | stock = 股票代币档(E 类) |
| `armed` | Bool | false | ✓ | 已验证(✅/🟡);pipeline `hist_verdict` |
| `open_th` | Rate | 0 | ✓ | pipeline 推荐阈值(历史推荐、配置阈值取大) |
| `open_th_floor` | Rate | 0.5% | | 有效开仓线的固定下限 |
| `min_open_th` | Rate | 1.8% | | 全局下限 |
| `spike_open` | Rate | 3.0% | | 0 = 关 spike 通道;`spike_open_pct` |
| `spike_cost_margin` | Rate | 1.5% | | spike 线 ≥ 成本线 + 1.5% |
| `spike_jump` | Rate | 1.5% | | buy − buy_med_1h 下限;`spike_jump_pct` |
| `spike_size_frac` | Rate | 50% | | spike 单笔 = max(min_chunk, chunk × 50%),当前轮次只此一笔 |
| `calm_med_max` | Rate | 1.5% | | \|buy_med_1h\| 上限(spike、carry 共用) |
| `carry_enabled` | Bool | true | ✓ | stock 档强制 false |
| `carry_line` | Enum[norm, cost] | norm | | 9/30 老板做法 |
| `carry_norm_h` | DurationMs | 6 h | | 常态 = 最近 6 h 买价差中位 |
| `carry_norm_min_samples` | Count | 360 | | 不足 → carry 线无效(不开) |
| `carry_norm_margin` | Rate | 0.1% | | |
| `carry_floor_abs` | Rate | 0.3% | | norm 线下限 |
| `carry_min_th_abs` | Rate | 0.6% | | cost 线下限 |
| `carry_min_daily` | Rate | 0.15% | | 9/29 规则 |
| `carry_min_daily_relaxed` | Rate | 0.08% | | 资金占用 < `relax_util` 时 |
| `relax_util` | Rate | 60% | | |
| `funding_structural_ok` | Bool | false | ✓ | pipeline 每 30 min 算(近 3 期每期折日化 ≥ 0.10%、30 天占比 ≥ 25%;低占用放宽 0.05% / 10%) |
| `carry_min_sample_h` | Count | 6 | | 小时 |
| `carry_min_cap` | Notional | $12 | | |
| `carry_size_frac` | Rate | 50% | | carry 总量 ≤ cap × 50%,单笔 = chunk × 50% |
| `carry_open_tol` | Rate | 0.15% | | carry 开仓限价容差 |
| `carry_drift` | Rate | 0.15% | | carry 出场容差;carry 口径的信号→成交漂移成本 |
| `carry_parallel` | Bool | true | | |
| `parallel_fallback_ms` | DurationMs | 30 min | | |

**确认与数据门**

| 字段 | 单位 | 默认 | 组 | 含义 / 来源 |
|---|---|---|---|---|
| `confirm_ms` | DurationMs | 450 ms | | 价差通道开/平;旧引擎现行值(9/25 限价保护上线后) |
| `confirm_slow_updates` | Count | 1 | | 期间链上报价达标次数;`min_chain_quotes` 2 → 1 |
| `confirm_min_updates` | Count | 1 | | |
| `confirm_min_blocks` | Count | 2 | | 跨块数(新栈规则,[06 §3](../../design/06-链上执行层.md)) |
| `carry_confirm_ms` | DurationMs | 450 ms | | |
| `carry_confirm_slow_updates` | Count | 2 | | |
| `stale_fast_ms` | DurationMs | 3 s | | Fast 腿连接静默上限:该 BBO 连接最近一次收到任何帧(数据、pong、心跳)距今不超过它即可用,盘口不变不算过期(SDK `BboView::quote_live`);生效值 = min(它, `RiskStateView.conn_liveness_ms`)——比引擎阈值宽的部分不生效;`stale_bbo_s` |
| `stale_slow_ms` | DurationMs | 1.5 s | | 最近一条链上报价的最大年龄(链上报价按块产出,没有新报价即过期);`stale_chain_s` |
| `max_abs_spread` | Rate | 30% | ✓ | stock 档由 pipeline 按标的给更严值 |
| `junk_after` | Count | 5 | | 连续报不出价 |
| `bad_pool_med` | Rate | 5% | | 1 h 中位 \|buy\| 超此 → `BAD_POOL`(只看不开) |

**节流、执行与 halt**

| 字段 | 单位 | 默认 | 组 | 含义 / 来源 |
|---|---|---|---|---|
| `cooldown_ms` | DurationMs | 20 s | ✓ | `signal_cooldown_s`;每次开/平后 |
| `max_consec_fail` | Count | 3 | | 连续执行失败 → 请求 Halted |
| `cex_fail_rounds` | Count | 2 | | CEX 腿连续 2 轮失败 → 请求 Halted |
| `hard_rearm_ms` | DurationMs | 60 s | | 同一 group 两次 reduce_half/close_all 的最小间隔(与旧引擎每分钟评估一次一致) |
| `chain_fix_rearm_ms` | DurationMs | 15 min | | 链上侧补腿最小间隔(旧引擎 hedge_fix 900 s) |
| `limit_tol` | Rate | 0.3% | | `limit_tol_pct` |
| `cex_slip` | Rate | 0 = 未设(`check` 拒绝) | | 第二腿 IOC 越价;待标定(开发备忘) |
| `hard_leg_order` | Enum[perp_first, chain_first] | perp_first | | 硬动作腿顺序 |
| `stress_step_ms` | DurationMs | 5 s(待标定) | | 保证金压力下分步减仓间隔 |
| `delist_lead_ms` | DurationMs | 0 = 未设(`check` 拒绝,迁移前定) | | 下架前多久全平 |

**规模**

| 字段 | 单位 | 默认 | 组 | 含义 / 来源 |
|---|---|---|---|---|
| `slow_cap` | Notional | $0 | ✓ | pipeline 因子 min(单币硬上限、池深、DEX 成交、CEX OI、CEX 成交);`sizing.compute` |
| `per_coin_cap` | Notional | $150 | ✓ | 安全阀 |
| `chunk_usd` | Notional | $40 | | |
| `max_share` | Rate | 25% | | 单币占 `capital_in_use` 上限 |
| `leverage` | Rate | 150% | | `bn_target_leverage` |
| `deployable_frac` | Rate | 100% | | |
| `capital_base` | Enum[wallet, margin_balance] | wallet | | `capital_in_use` 的基数(现行 = CEX 永续所 USDT 钱包) |
| `min_chunk` | Notional | $6 | | CEX 最小名义($5)上加余量 |
| `book_frac` | Rate | 50% | | 顶档 ≥ 2 × 单笔才算 Top;`cex_depth` |
| `book_levels` | Count | 20 | | |
| `cex_max_impact` | Rate | 0.15% | | `cex_max_impact_pct` |
| `probe_base` | Notional | $5 | | 链上冲击探测基准额 |
| `probe_max_impact` | Rate | 0.3% | | |
| `probe_max_halvings` | Count | 4 | | |
| `chain_reserve` | Notional | $1 | | `chain_usdt_reserve` |
| `chain_balance_margin` | Rate | 3% | | 链上计价币需 ≥ 单笔 × 1.03 |
| `dust_sweep` | Notional | $6 | | 平仓剩余 < 此值则本笔一并平;`dust_sweep_usd` |
| `residual_sweep_max` | Notional | $5 | | 残余清理判定线(现 `<$5` 规则);执行层尘埃线 `dust_notional` 的唯一归属在 [hft-exec](../../crates/hft-exec/README.md),两者现值相同 |
| `naked_fix_tol` | Rate | 5% | | 两腿差补腿阈值(`naked_fix_pct`);写进本策略每个链接的 `LegLink.tolerance`,执行层的自动补腿按它判定 |

**成本线**(插件按池子费率算,见[插件包的成本线](pipeline/README.md#成本线))

| 字段 | 单位 | 默认 | 组 | 含义 |
|---|---|---|---|---|
| `cost_pool_fee_rt` | Rate | 0 | ✓ | 2 × 池子费率(一跳路由再加中转池双边);费率未知按整体估计(BSC 0.8%、Solana 0.59%,pipeline 填) |
| `cost_gas_usd` | Notional | $0 | ✓ | 两笔 swap 的 gas(USDT);运行时 `gas_rt = cost_gas_usd / 单笔` |
| `cost_slip_margin` | Rate | 0.10% | | |
| `cost_exec` | Rate | 0.30% | | `EXEC_COST_PCT` |
| `cost_margin` | Rate | 0.20% | | `MARGIN_PCT` |
| `cost_rt_floor` | Rate | 0.4% | | 往返部分下限 |
| `cost_bribe` | Rate | 0 | ✓ | chain-core 出价折算(公共 RPC 为 0) |

CEX 腿手续费往返不是参数:`on_params` 时用 [`hft-strat-lib::fee_round_trip`](../../crates/hft-strat-lib/README.md) 从 refdata 的账户费率档求(现值 taker 往返 0.10%)。

**平仓(ClosePolicy)、换仓、手动**

| 字段 | 单位 | 默认 | 含义 / 来源 |
|---|---|---|---|
| `profit_min` | Rate | 0.5% | 基础目标:入场 − 成本线 − 这个 |
| `th_min` / `th_max` | Rate | −1% / 0.3% | 平仓线上下限;硬动作时平仓线 = `th_max` |
| `opt_pctile` | Rate | 20% | 乐观目标取 72 h 卖价差的哪个分位 |
| `funding_min_daily` | Rate | 0.05% | 普通仓"资金费没了" |
| `carry_fund_low_div` | Count | 2 | carry 仓"资金费低" = `carry_min_daily / 2` |
| `funding_neg_settles` | Count | 2 | 连续负费率几期按负处理 |
| `decay_h` | DurationMs | 48 h | 乐观目标线性抬到保本 |
| `decay_low_div` / `decay_neg_div` | Count | 2 / 4 | 资金费低 / 为负时抬线加速倍数 |
| `neg_be_after_h` | DurationMs | 12 h | 资金费为负且持仓 ≥ 12 h → 直接保本 |
| `hold_p20_h` / `hold_p40_h` | DurationMs | 48 h / 96 h | 持仓超过后不低于 24 h 卖价差 p20 / p40 |
| `max_giveback` | Rate | 0 | 普通仓上限 = 保本 + 这个 |
| `liq_reduce` / `liq_close` | Rate | 15% / 8% | 强平距离硬动作 |
| `adl_reduce` | Count | 4 | ADL 档 ≥ 4 → 减半 |
| `runaway` | Rate | 80% | 币价较入场涨幅 → 全平 |
| `carry_profit` | Rate | 1.0% | carry 仓资金费仍在收时,额外利润下限 |
| `carry_hold_days` | Count | 2 | 额外利润 ≥ 日化 × 2 天 |
| `carry_th_floor` | Rate | −8% | carry 仓持有期平仓线下限(不被 −1% 截断) |
| `carry_giveback` | Rate | 0 | carry 仓资金费低 ≥ `carry_stale_h` 后允许亏多少(9/29 用户:必须保本 → 0) |
| `carry_stale_h` | DurationMs | 12 h | |
| `patience_util_lo` / `patience_util_hi` | Rate | 60% / 80% | 资金占用分档 |
| `patience_lo` / `patience_lo_h` | Rate / DurationMs | 0.15% / 24 h | 占用 < 60%:多要 0.15%,24 h 内线性退到保本 |
| `patience_mid` / `patience_mid_h` | Rate / DurationMs | 0.05% / 12 h | 占用 60–80% |
| `carry_close_mode` | Enum[profit, manual, policy] | profit | 9/30 |
| `carry_min_profit` | Rate | 0.05% | profit 模式:平仓线 ≤ 保本 − 0.05% |
| `be_include_funding` | Bool | false | 保本线是否加已收资金费 |
| `swap_min_edge` | Rate | 0.5% | 换仓:新信号须比开仓线高 0.5% |
| `swap_exit_over_be` | Rate | 0.1% | 被换仓位按保本 + 0.1% 出 |
| `swap_hold_ms` / `swap_min_interval_ms` | DurationMs | 10 min / 10 min | |
| `manual_open_max_fires` / `manual_close_max_fires` | Count | 15 / 20 | |
| `manual_max_ms` | DurationMs | 30 min | |

**Model 周期与 advice 有效期**

| 字段 | 单位 | 默认 | 含义 |
|---|---|---|---|
| `model_tick_ms` | DurationMs | 10 s | Model 时间刻度与价差采样间隔(= 旧引擎 `engine_samples` 10 s 采样);结构参数 |
| `stats_period_ms` / `stats_valid_ms` | DurationMs | 60 s / 180 s | `SpreadStats`、`SampleStats` |
| `close_period_ms` / `close_valid_ms` | DurationMs | 60 s / 120 s | `CloseTarget`、`HardTriggers` |
| `impact_period_ms` / `impact_valid_ms` | DurationMs | 30 min / 45 min | `ImpactCurve` |
| `swap_period_ms` / `swap_valid_ms` | DurationMs | 10 s / 20 s | `SwapCandidate` |
| `pct_bin` / `pct_range` | Rate | 0.001% / 30% | 价差分位直方图箱宽与范围(±);结构参数 |
| `pct_fill_max_ms` | DurationMs | 30 s | 采样补齐上限 |

`CdexParams::validate`:`n_legs == 2`、leg 0 为 `LinearPerp`、leg 1 为链上 venue 且 `route_id` 在 refdata 中存在;`profile = stock` 时 `carry_enabled` 必须为 false;`cex_slip`、`delist_lead_ms` 非 0;`chunk_usd ≥ min_chunk`;所有 Rate 在 schema 的 `[min, max]` 内。

### Advice 载荷

`CdexModel` 用 SDK `AdviceOut::emit` 输出;Advice 头由引擎填 `strategy`、`account`、`group`、`basis_seq`、`basis_tsc`;`local_kind` 与载荷布局由本策略定义(没有全局登记;`src/advice.rs` 为每种载荷实现 `AdvicePayload`)。载荷首字节为该 kind 的版本号(当前 1),其余为整数定点字段。Advice 记录没有有效期字段:快速部分以 `EventRef::Advice.basis_tsc`(Model 依据的数据时刻,不是到达时刻)加下表的有效期参数存入 `Expiring<_>`;依据上次启动数据的 Advice(`basis_seq < Now.boot_seq`)一律过期(跨启动的唯一判据)。

| `local_kind` | 名称 | 周期(参数) | 有效期(参数) | 载荷字段(整数) |
|---|---|---|---|---|
| 1 | `SpreadStats` | `stats_period_ms` 60 s | `stats_valid_ms` 180 s | `buy_med_1h`、`buy_med_6h`、`sell_med_24h`、`sell_p20_24h`、`sell_p40_24h`、`sell_p20_72h`(Rate;样本不足 = `i64::MIN`) |
| 2 | `SampleStats` | 同上 | 同上 | `sample_hours: u16`、`hit_hours: u16`、`flags: u32`(`BAD_POOL`)、`sell_min_6h`、`hit_rate_24h`(ppm)、`max_buy_24h`、`carry_line`(Rate) |
| 3 | `CloseTarget` | `close_period_ms` 60 s;另在发现 `last_fill_ens` 变化后的下一个采样事件立即重算 | `close_valid_ms` 120 s | `close_th`、`breakeven`、`entry_spread`(Rate)、`hard_action: u8`、`reason: u16`、`fund_low_since_s: u32` |
| 4 | `HardTriggers` | `close_period_ms`;另在保证金数据(`MarginView.src_ens`)或 `last_fill_ens` 变化后的下一个采样事件立即重算 | `close_valid_ms` 120 s | `liq_reduce_px`、`liq_close_px`、`runaway_px`、`ref_mark`(Px)、`adl_level: u8` |
| 5 | `ImpactCurve` | `impact_period_ms` 30 min | `impact_valid_ms` 45 min | `cap_after_probe`(Notional)、`impact_at_cap`(Rate)、`probes: u8`、`base_px`、`cap_px`(Px) |
| 6 | `SwapCandidate` | `swap_period_ms` 10 s | `swap_valid_ms` 20 s | `release_group: GroupId`、`exit_th`(Rate);`group = GROUP_NONE` |

Model 计算按 gix 升序分片:每个模型 tick 只处理 ⌈G × model_tick_ms / stats_period_ms⌉ 个 group 的统计与 ClosePolicy(40 group 时每 tick 7 个),一个周期内覆盖全部 group,使单个 tick 不超过 SDK 的 1 ms 上限;第 k 片 = ⌊该模型 tick 的 UTC 时刻 / `model_tick_ms`⌋ 对分片数取模(模型 tick 按 UTC 边界对齐,所以分片位置与重启无关,不进状态);"立即重算"的 group 在采样事件里单独处理。

**过期即保守**:所需 advice 过期(`Expiring::get` 返回 `Stale`/`Missing`)时该 group 不开仓;已持仓的平仓线沿用最后一次有效值并计数 `ADVICE_STALE`;advisor 停摆由 watchtower 告警。spike 通道依赖 `SpreadStats.buy_med_1h`,过期即关闭。

### 对引擎事件的响应

- **运行模式**:各模式下意图怎么处理以 [SDK 运行模式行为表](../../crates/hft-strategy-sdk/README.md) 为准(Shadow 只记录、CloseOnly 拒 Open、Live 遵守 `open_allowed`);硬动作在 Shadow 下只记录。
- **`Risk{CapSet/CapCleared{Halted}}`**:`Throttle::set_halted` 镜像;halted 只拦开仓,平仓、硬动作照常。
- **`Risk{MarginStress{venue, level, liq_dist, deadline_tsc}}`**(每 (account, venue) 按到强平距离分档,[hft-risk](../../crates/hft-risk/README.md)):只有 `venue` = 本实例 CEX 永续 venue 的事件触发减仓。L1 只禁开(引擎 cap);L2 起本策略按 CEX 腿名义从大到小,每 `stress_step_ms` 对一个 group 发 `reduce_half`(`hard_leg_order`),直到该 venue 回落或截止;截止后由引擎强制减 CEX 永续腿([hft-exec](../../crates/hft-exec/README.md)),链上腿留给本策略随后的链上补腿处理。本策略自己的强平距离硬动作(`liq_reduce` 15%、`liq_close` 8%,ClosePolicy)与引擎分档并存,两者关系见开发备忘。
- **`Risk{HedgeBroken{group}}` / `Exec{Broken}`**:不自行补腿(含链上补腿);停该 group 的开平评估并写 `GroupBroken` Note。Broken 不是终态:执行层在 Broken 持续 `broken_auto_fix_ms`(开始条件与计时归 hft-exec)后可能自动有界平掉多出的**交易所腿**(CEX 空单多出时 reduce-only 买回),L3 与 `Kill{flatten}` 也会先减 CEX 永续腿;多出的是链上腿时执行层不能动作,只写 Note,仍等人工 `HedgeFix` 或运营处理。cell 回到 Idle(`Exec{Done{cleared_by_engine}}`)后,本策略按 `LegView`/`group_pos` 的实际两腿重新计算 `held_usd` 与两腿差,两腿差 ≥ `naked_fix_tol` 且多出的是链上腿时按[两腿差规则](#平仓残余与硬动作)发链上补腿;账户级 `HedgeFailure` cap 解除前开仓仍被挡。
- **`External{cause = Liquidation | Adl | Unknown}`**(CEX 腿被强平/ADL,或对账自动补记的漏推送成交):在 `ExternalFillView.deadline_tsc`(执行层 `external_deadline_ms`)之前,对链上多出的代币量发链上补腿(`SwapIntent{Sell}`,Flatten);CEX 腿多出时交给执行层截止后的默认处理。执行层的默认处理只能下 CEX 订单,链上腿只能由本策略处理(见开发备忘)。写 `ExternalChainFix` Note(P1;引擎侧的外来成交 Note 另有 P0)。
- **`External{cause = Manual}`**(人工在交易所网页改了 CEX 腿):执行层不做默认处理,并可对该 cell 置 `manual_hold`(不补腿、不修剪残余;置位与解除条件只在 [hft-exec](../../crates/hft-exec/README.md#人工保持与恢复期抑制) 定义);本策略**不依赖**执行层处理任何一腿:停该 group 的开平评估与两腿差处理(不发链上补腿),写 `ManualExternal` Note(P1),由人工决定后续——两腿差回到 `naked_fix_tol` 以内(人工在交易所自行补齐)后自动恢复评估;人工发本策略的 `close` 命令([手动任务](#手动任务))时照常执行平仓任务并恢复两腿差处理(这些意图对 `manual_hold` 的作用按 [hft-exec 人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制))。
- **`Notice{Delisting{at_ns}}`**:引擎已对该 inst 设 CloseOnly;本策略在 `at_ns − delist_lead_ms` 发 `close_all`。
- **`Notice{FundingInterval{new_s}}`** 与 `FundingStats` 检测到的周期变化:日化口径立即改用新周期;写 Note。
- **`Notice{Maintenance}`**:维护窗口内不开仓(平仓照常尝试,失败按执行失败计)。

### Note 码表与报表字段

即 `DESC.notes` / `DESC.report`(`src/desc.rs`,[SDK 策略描述符](../../crates/hft-strategy-sdk/README.md#策略描述符note报表策略命令));ops_bot 面板与日报只按它们通用渲染。策略命令见[手动任务](#手动任务)。

| code | name | 级别 | v[] | 产生时机 |
|---|---|---|---|---|
| 1 | `HaltRequested` | P1 | v0 = 原因(1 链上余额过期、2 CEX 腿连续失败、3 连续执行失败),v1 = 计数 | `request_halt` |
| 2 | `GroupBroken` | P1 | v0 = `BrokenReason` | `Exec{Broken}` / `Risk{HedgeBroken}` |
| 3 | `ExternalChainFix` | P1 | v0 = `ExternalCause`,v1 = 链上多出量 | `External` 后发链上补腿 |
| 4 | `HardAction` | P1 | v0 = 动作(1 reduce_half、2 close_all),v1 = 触发(1 强平距离、2 runaway、3 ADL),v2 = mark | 硬动作开火 |
| 5 | `ChainFix` | P1 | v0 = 多出代币量,v1 = 差额名义 | 两腿差规则下的链上补腿 |
| 6 | `ResidualSweep` | Info | v0 = 平掉的数量 | 残余清理 |
| 7 | `ParallelFallback` | P2 | v0 = 回退期 ms | 并行回滚 |
| 8 | `OpenFired` | Info | v0..v3 = 通道指示(normal / spike / carry / manual,其一为 1),v4 = 开仓价差(Rate),v5 = 单笔名义 | 每次开仓开火 |
| 9 | `AdviceStale` | Info | v0..v5 = 各 `local_kind` 自上次以来过期次数 | 每 group 每 10 min 至多一条(计数非零) |
| 10 | `SizeBinding` | Info | v0 = 卡住的因子,v1 = cap,v2 = chunk | 因子变化时(面板数据) |
| 11 | `SwapSelected` | Info | v0 = `release_group`,v1 = `exit_th` | 换仓生效 |
| 12 | `ManualDone` | Info | v0 = `cmd_seq`,v1 = 笔数,v2 = 完成名义,v3 = 结束原因(1 完成、2 超次数、3 超时、4 被替换) | 手动任务结束 |
| 13 | `ManualRejected` | P2 | v0 = `cmd_seq`,v1 = 原因(1 实例级 `GROUP_NONE`、2 group 只看不开、3 `reduce_only` 与 `op` 不符) | 不执行的策略命令 |
| 14 | `DelistClose` | P1 | v0 = 下架时刻 ns | 下架前 `close_all` |
| 15 | `FundingInterval` | P2 | v0 = 旧周期 s,v1 = 新周期 s | 结算周期变化 |
| 16 | `ManualExternal` | P1 | v0 = `Side`,v1 = 成交量,v2 = 两腿差名义 | `External{Manual}` 后冻结该 group |

| 报表字段 | 单位 | 聚合 | 范围 | 来源 |
|---|---|---|---|---|
| `opens_normal` / `opens_spike` / `opens_carry` / `opens_manual` | Count | Sum | Group | `OpenFired` v0 / v1 / v2 / v3 |
| `open_notional` | Notional | Sum | Group | `OpenFired` v5 |
| `open_spread_p99` | Rate | P99 | Instance | `OpenFired` v4 |
| `halts` / `broken` / `hard_actions` / `chain_fixes` | Count | Count | Group | `HaltRequested` / `GroupBroken` / `HardAction` / `ChainFix` |
| `manual_jobs` | Count | Count | Instance | `ManualDone` |

## 实现说明

<details><summary>实现要点</summary>

### 状态(`CdexGroup`,≤ 512 B)

`trig: TriggerPx`、`cw_open`、`cw_close`、`cw_carry: ConfirmWindow`、`thr: Throttle`、`entry_mode: u8`(None/Normal/Spike/Carry/Manual)、`eff_open_th: Rate`、`close: Expiring<CloseTarget>`、`hard: Expiring<HardTriggers>`、`stats: Expiring<SpreadStats>`、`sample: Expiring<SampleStats>`、`impact: Expiring<ImpactCurve>`、`held_usd: Notional`、`cap/chunk/room: Notional`、`binding: SizeFactor`、`swap_until_ens: i64`、`round_start_seq: u64`、`manual: Option<ManualJob>`(分笔与超时截止为 engine_ns)、`manual_frozen: bool`(`External{Manual}` 后的冻结)、`cex_fail_rounds: u8`、`no_quote_streak: u16`、`chain_fix_until_ens: i64`、`buy/sell: Rate`(最近价差)。跨事件持有的引用(定时器、手动任务、advice)以 `GroupId` 为键,`gix` 只在一个参数版本内有效;`on_params` 中新 GroupId 或 `GroupSpec::fingerprint()` 变化(换池、换路由)的 group 状态从空起步([SDK](../../crates/hft-strategy-sdk/README.md#参数变化时的-group-重置))。跨事件保存的时刻全部是 engine_ns(含 hft-strat-lib 构件内部),快照里没有 TSC;`restore` 时 `reset` 确认窗口、`invalidate` 触发价,冷却、回退期、换仓与补腿间隔按 engine_ns 继续(停机不计)。

账户汇总 `AcctAgg`:`held_sum = Σ held_usd`,`inflight_sum = Σ 在途开仓单笔`,`capital_in_use`,`utilization = (held_sum + inflight_sum) / capital_in_use`。只在相关 group 的 `Mark`/成交/在途变化时增量更新,O(1)。

### on_event 骨架

① 取 `groups[gix]`;② 事件分派;③ 慢侧(链上报价、参数、advice)→ 重算 `TriggerPx`;④ 快侧 → `hit_*` 不命中即返回;⑤ `DirtyFilter` → 通道判定 → `ConfirmWindow` → `Throttle::can_fire` → 开仓前门 → `Sizer` → `ImpactModel::vwap` 复核 → 构造意图;⑥ `Order`/`Swap`/`Exec` → 状态推进;⑦ `Advice` → 缓存并重算。

### 价差与触发价

- 价差(老板定义,[hft-strat-lib SpreadCalc](../../crates/hft-strat-lib/README.md)):`buy = (fast_bid − pool_buy_px) / pool_buy_px`,`sell = (fast_ask − pool_sell_px) / pool_sell_px`。链上 `buy_px/sell_px` 取链上报价中"按单笔金额(买)/持仓量(卖)报出的均价",池子费与冲击已在其中。
- `TriggerPx`:开 `fast_bid ≥ pool_buy × (1 + θ_open)`,平 `fast_ask ≤ pool_sell × (1 + close_th)`;θ_open = 当前通道的有效开仓线。

### 三条开仓通道(互斥,优先级依次)

先算 `held_carry = held_usd ≥ 5 且 entry_mode = carry`,`held_other = held_usd ≥ 5 且 entry_mode ∉ {None, carry}`,`held_spike = held_usd ≥ 5 且 entry_mode = spike`。

1. **armed**:`!held_carry` 且 `armed_eff` 且 `buy ≥ eff_open_th`,其中 `eff_open_th = max(open_th, open_th_floor 0.5%, 成本线 open_floor, min_open_th 1.8%)`,`armed_eff = armed 或(sample_hours ≥ 24 且 hit_hours ≥ 3)`,且无 `BAD_POOL`、`cap ≥ 6`。
2. **spike**:`!held_carry` 且 `!held_spike` 且 armed 不成立,`spike_open > 0`,`buy ≥ spike_th = max(spike_open 3.0%, 成本线 + spike_cost_margin 1.5%)`,无坏池、`no_quote_streak < 5`、`cap ≥ 6`,`|buy_med_1h| ≤ 1.5%` 且 `buy − buy_med_1h ≥ spike_jump 1.5%`。单笔 = max(6, chunk × 50%),当前轮次只此一笔。
3. **carry**:`!held_other` 且前两者都不成立,`carry_enabled`,无坏池、`no_quote_streak < 5`、`cap ≥ carry_min_cap 12`,`buy ≥ carry_line`,`|buy_med_1h| ≤ 1.5%`,`sample_hours ≥ 6`,`funding_structural_ok` 且 24 h 卖价差中位可用,`FundingGate` 通过。`room = min(room, cap × carry_size_frac − held_usd)`,单笔 = max(6, chunk × 50%),限价容差 `min(limit_tol, carry_open_tol 0.15%)`。

通道约束:carry 仓不用价差/spike 通道加仓,价差仓不用 carry 加仓。`entry_mode` 在当前轮次第一笔开仓时确定并随轮次保持(写入意图 `tag` 与快照)。确认:armed/spike 用 `confirm_ms 450 / confirm_slow_updates 1 / confirm_min_blocks 2`;只有 carry 成立时用 `carry_confirm_ms 450 / carry_confirm_slow_updates 2 / confirm_min_blocks 2`。

### 开仓前门(全部通过才开)

1. `DirtyFilter`(`stale_fast_ms 3000`(生效值见参数表)、`stale_slow_ms 1500`、`max_abs_spread 30%`、`junk_after 5`):Fast 腿 `LegFresh::from_bbo`(连接健康,盘口不变不算过期),Chain 腿 `LegFresh::from_quote`(最近链上报价时刻)。
2. `Throttle::can_fire(Open)`:20 s 冷却、`in_flight`、halted、限速退避。
3. `room = cap − held_usd ≥ 6`。
4. 链上计价币:`chain_quote_cash − Σ 其他 group 在途单笔 − chain_reserve 1.0 ≥ 单笔 × 1.03`;链上计价币现金 = `PortfolioView::cash(链上 venue, 计价资产).free`。
5. 账户敞口:`held_sum + inflight_sum(不含本 group)+ 单笔 ≤ capital_in_use`;不满足 → 尝试换仓(SwapPicker)后本次不开。
6. CEX 深度复核:`ImpactModel::vwap(book, Sell, 单笔数量, book_frac 50%, 20 档)`;`Insufficient` 或 `impact > cex_max_impact 0.15%` → 不开,并把下一次单笔减半;`Walked` 时用 `(vwap − pool_buy) / pool_buy` 重算价差,仍须 ≥ 开仓线。

### 执行

- chain-first:`limit_px = fast_bid / (1 + (θ − limit_tol))`(carry 通道 `limit_tol = 0.15%`),由此反推 `min_out`;`link = OnFill`,CEX 腿数量 = `IncludedOk.amount_out` 对齐 lot,`px = bid × (1 − cex_slip)`。
- carry 并行:`carry_parallel` 且 `!thr.in_fallback(now.engine_ns)` 且本笔为 carry 通道(含手动 carry)→ `Simultaneous{on_fail = Reverse}`;`Order/Swap` 结果为 `Rollback` → 回退期截止 = `now.engine_ns + 30 min`。
- 每次开/平开火后 `cooldown 20 s`;流程终态时 `Throttle::on_outcome`。

### 平仓、残余与硬动作

- **常规平仓**:`held_usd ≥ 5`、`close` advice 有效、`sell ≤ close_th`、`|sell| ≤ max_abs_spread`、`Throttle::can_fire(Close)` → `ConfirmWindow`(同 armed 参数)→ 平仓量 = `min(chunk, held)`;若 `held − 平仓量 < dust_sweep 6` → 本笔平全部(`held × 1.05` 扫尾)。carry 仓出场限价容差 `min(limit_tol, carry_drift 0.15%)`。换仓覆盖期内 `close_th = max(close_th, breakeven + swap_exit_over_be)`。
- **残余**:平仓完成后两腿名义都 < `residual_sweep_max $5` 且 CEX 空单量 > 链上币量 → Fast 腿 IOC 买 reduce-only,数量 = `min(cex, ceil_to_lot(cex − dex))`(Flatten)。
- **硬动作**(`HardTriggers` 在每个 `Mark` 上整数比较;ADL 档随 advice 到达即判):`mark ≥ liq_close_px` 或 `mark ≥ runaway_px` → `close_all`(平仓量 = held × 1.05);`mark ≥ liq_reduce_px` 或 `adl_level ≥ 4` → `reduce_half`(平仓量 = max(held / 2, 6))。`Throttle::can_fire(Hard)` 只受 `hard_rearm_ms` 限制(C5)。腿顺序 `hard_leg_order`:
  - `perp_first`(默认,与执行层"保证金/硬动作先减永续"的规则一致):Fast 腿 IOC 买 reduce-only,`link = OnFill → SwapIntent{Sell}`;
  - `chain_first`(旧引擎做法):与常规平仓同一路径,不设价差限制。
- **两腿差**:`naked = |cex_qty − dex_qty| / max(两腿)`(取 SDK `LegView`,含在途)≥ `naked_fix_tol 5%` 时:CEX 多出 → 由执行层自动补腿(阈值来自本策略链接的 `LegLink.tolerance`;尘埃线、计次、冷却、截止归 [hft-exec](../../crates/hft-exec/README.md));链上多出且差额名义 ≥ $1 → 本策略发 `SwapIntent{Sell, 差额}`(Flatten),间隔 ≥ `chain_fix_rearm_ms`。存在 Open/Hedging 链接、Pending/Unknown 槽或 Broken/Reorg cap 时不发(与执行层的抑制条件一致)。

### halt

无法自动处理的状态 → 请求 `Halted{strategy, account, group}` cap(只有 `Resume{Halted}` 解除,现 `/engine unhalt`):链上余额读不到(`PortfolioView` 链上 venue 余额标记过期)、CEX 腿连续 `cex_fail_rounds 2` 轮失败、`Throttle` 连续 `max_consec_fail 3` 次**执行失败**。门控拒绝(hft-types `RejectBits` 的门控位,如 `KILLED`、`CLOSE_ONLY`、`VENUE_NOT_READY`、`RATE`、`LINK_STALE`、`LINK_FUNDS`)记为 `Gated`,不计数。halted 期间只拦开仓;常规平仓与硬动作继续(与旧引擎"停开平"不同,见开发备忘)。

### 手动任务

人工开/平(现 `/engine open|close`)经通用命令 `StrategyCmd{instance, group, payload}`([control-plane](../../docs/subsystems/control-plane.md))到达,载荷由本策略的 `cmds` 描述符定义(SDK 载荷格式 `{ver, op, n, vals[n]}`,ops_bot/hft-ctl 按导出的 schema JSON 通用编码),引擎不解释:

| op | name | `reduce_only` | 字段(按顺序) | 含义 |
|---|---|---|---|---|
| 1 | `open` | false(升级类,二次确认) | `usd: Notional`(> 0)、`min_spread: Rate`(默认 0)、`mode: Enum[carry, normal]`(默认 carry) | 开仓任务;`mode` = 当前轮次 `entry_mode`(carry = 吃资金费轮次,可走并行;normal = 价差轮次) |
| 2 | `close` | true(中性类;本策略处理它时只产生 Reduce/Flatten 意图,含后续分笔) | `usd: Notional`(0 = 全部)、`max_spread: Rate`(0 = 缺省 `th_max`) | 平仓任务 |

→ `ManualJob`:
- 开:每笔 = max(6, min(chunk, 剩余));每笔仍要求 `buy ≥ min_spread` 与两腿限价;`entry_mode = mode`;carry 模式可走并行。≤ `manual_open_max_fires 15` 次、≤ `manual_max_ms 30 min`;账户敞口门照旧。
- 平:剩余 ≥ held − 5 → 本笔平全部(held × 1.05);否则每笔 = max(6, min(chunk, 剩余));`sell ≤ max_spread`;≤ 20 次、≤ 30 min。
- 分笔间隔由 `Timer` 驱动(≥ 冷却);超次数或超时 → 任务结束并写 `ManualDone` Note(v 带 `cmd_seq`)。同一 group 已有任务时新命令替换旧任务并写 Note。`group = GROUP_NONE`(实例级)的命令不支持(写 `ManualRejected` Note 后忽略)。`reduce_only = true` 却是 `open` 的命令同样拒绝(写 Note)。

### 成本线(用 `hft-strat-lib::CostLine`)

| 项 | Rate | 位 |
|---|---|---|
| CEX taker 往返(refdata,现 0.10%) | `fee_round_trip` | `ROUND_TRIP \| OUT_OF_QUOTE` |
| 池子费往返 | `cost_pool_fee_rt` | `ROUND_TRIP`(已在链上报价里,不进保本线) |
| gas | `cost_gas_usd / 单笔` | `ROUND_TRIP \| OUT_OF_QUOTE` |
| 滑点余量 | 0.10% | `ROUND_TRIP` |
| 出价折算 | `cost_bribe` | `ROUND_TRIP \| OUT_OF_QUOTE` |
| 执行成本 | 0.30% | `FIXED` |
| 余量 | 0.20% | `FIXED` |
| carry 漂移 | 0.15% | `OUT_OF_QUOTE`(只用于 carry 口径) |

- 开仓成本线 `open_floor = max(0.4%, cex_taker_rt + pool_fee_rt + gas_rt + slip_margin) + exec + margin`;1% 费率池 ≈ 3.2%,与 `token_check.cost_floor` 一致(Python 最后按 0.1% 取整,Rust 保留 `Rate` 精度,对照测试取整后比较)。
- 价差外成本 `fees_out = cex_taker_rt + gas_rt + bribe (+ drift,carry 口径)`,用于保本线与 carry 的 cost 线;池子费已在两个价差里,不重复算。
- gas 随单笔变化:`gas_rt` 在单笔(chunk)变化时重算,`CostLine::set` 属于参数/规模变化路径,不在每个 tick 上。

### Sizer(规模)

上限 `cap = min(各因子)`:

| 因子 | 计算 | 谁算 | 现值 |
|---|---|---|---|
| 单币硬上限 | `per_coin_cap` | 参数 | $150(9/26 分析认为保守;是安全阀) |
| 账户占比 | `capital_in_use × max_share` | 快速部分(实时) | 25% |
| 池子深度 | 计价币一侧 × 档位比例(< $100k:0.6 × base;> $500k:1.5 × base;base 0.5%)→ 0.3% / 0.5% / 0.75% | pipeline(进 `slow_cap`) | — |
| DEX 成交量 | 池 24 h 成交 × 1% | pipeline | — |
| CEX OI | OI × 0.05% | pipeline | — |
| CEX 成交量 | 24 h 成交 × 0.02% | pipeline | — |
| 冲击探测 | `ImpactCurve.cap_after_probe` | Model | 0.3%,≤ 4 次减半 |

- `capital_in_use = base × leverage(1.5) × deployable_frac(1.0)`,`base` = CEX 永续所的钱包余额 `venue(v).margin_balance − venue(v).perp_upnl`(`capital_base = wallet`,现行口径)或保证金余额 `venue(v).margin_balance`(`capital_base = margin_balance`),取自 SDK `PortfolioView`。这是本策略的资金口径;引擎不提供"可用资金"。
- 单笔:`chunk = cap < 12 ? cap : max(6, cap / 2)`,再 `min(chunk_usd 40)`,再 `min(顶档名义 × 50%)`(仅当该值 ≥ $6),最后 `cap ≥ 6 ? clamp(chunk, 6, cap) : 0`。`room = cap − held_usd`,`room < 6` 不开。carry 通道 `cap × carry_size_frac(0.5)`。`binding` 记录卡在哪个因子(ops_bot 面板数据,经 `Ctx::note`)。

### FundingGate(carry 通道)

`daily = FundingStats::daily(rate, interval_s)`,`rate` 与 `interval_s` 取 SDK `FundingView` 的实时费率与**实际结算周期**(不取 refdata 默认周期);门槛 `utilization < relax_util(60%) ? carry_min_daily_relaxed(0.08%) : carry_min_daily(0.15%)`(开仓时实时判断:放宽进场把占用推过 60% 后,新 carry 自动回到 0.15%);`funding_structural_ok`(pipeline 参数)必须为 true。尚无资金费推送(`FundingView.upd_tsc = 0`)→ 不通过(旧引擎:费率缺失即不开)。stock 档不参与(无资金费项)。

### 价差样本统计(72 h 口径)、允许开仓规则与 carry 线(Model)

- 每 group 一个 [`PercentileStats`](../../crates/hft-strat-lib/README.md):`sample_ms = model_tick_ms`(10 s,与旧引擎 `engine_samples` 同频)、窗口 {1 h, 6 h, 24 h, 72 h}、范围 ±`pct_range`(30%)、箱宽 `pct_bin`(0.001%)、`fill_max_ms = pct_fill_max_ms`(30 s)、`sides = 2`(买卖两侧分别采样;查询用 `value`/`rank`)。另有 `HourlyMax`(72 个整点小时桶,每桶记录是否有样本与该小时最大 buy)。
- 输出 `SpreadStats`:`buy_med_1h`、`buy_med_6h`、`sell_med_24h`、`sell_p20_24h`、`sell_p40_24h`、`sell_p20_72h`;`SampleStats`:`sample_hours`(72 h 内有样本的小时数)、`hit_hours`(其中最大 buy ≥ 开仓成本线的小时数)、`sell_min_6h`、`hit_rate_24h`(24 h 内 buy ≥ 有效开仓线的样本比例)、`max_buy_24h`、`BAD_POOL`(1 h 中位 \|buy\| > 5%)、`carry_line`。
- **允许开仓规则**(`refresh_params`):pipeline 历史 ✅/🟡(`armed`),或 `sample_hours ≥ 24 且 hit_hours ≥ 3`;`BAD_POOL` 或 `cap < 6` → 只看不开。
- **carry 线**:`carry_line = norm` → `max(carry_floor_abs 0.3%, buy_med_6h + carry_norm_margin 0.1%)`,6 h 样本 < 360 个 → 无效;`carry_line = cost` → `max(carry_min_th_abs 0.6%, sell_med_24h + fees_out(carry 口径))`,`sell_med_24h` 不可用 → 无效。
- 冷启动(无可用 Model 检查点):本策略的 [pipeline 插件包](pipeline/README.md)经 `InjectWarmup`([SDK 预热注入](../../crates/hft-strategy-sdk/README.md#预热注入))注入 72 h 价差样本,Model 用 `PercentileStats::prefill`(只补空点、实时优先)与 `HourlyMax` 合并;注入完成前相关 advice 标"样本不足"。载荷 **`local_kind = 1`:`SpreadWarmup`(`ver = 1`)**:`{ver: u8, _pad: [u8; 3], group: GroupId, start_ns: i64, step_ms: u32 /* = 10 000 的整数倍 */, n: u16 /* ≤ 509 */, _pad2: u16}` 24 B + `n × {buy: i32, sell: i32}`(`Rate`,`i32::MIN` = 缺失);来源为旧引擎的 `engine_samples`(10 s 采样)或新栈 journal 归档,与插件共用金样 `fixtures/strategies/cdex/warmup/`。72 h / 10 s 每 group 25 920 个样本 ≈ 51 块,40 group ≈ 2 040 块、约 8 MiB journal Input。
- 内存:每 group 直方图 8 个 × 60,001 箱 × 4 B ≈ 1.9 MiB + 环 25,920 点 × 8 B ≈ 200 KiB ≈ 2.1 MiB;40 个 group ≈ 85 MiB(Model 启动时一次分配)。`CdexModel::mem_bytes(p, n)` = n × (`PercentileStats::mem_bytes(cfg)` + `HourlyMax` 与缓存的定长部分)+ 永续 inst 数 × `FundingStats` 大小,按参数实算(箱宽、范围变化时随之变化)。
- **检查点**([SDK 两节](../../crates/hft-strategy-sdk/README.md#model-检查点)):原始样本节 = 每 group 的 `PercentileStats`(含 `PctConfig`)与 `ImpactCurve` 最近一次探测结果、每个永续 inst 的 `FundingStats`;路径状态节(`PATH_VERSION = 1`)= 每 group 上次看到的 `last_fill_ens` 与 `MarginView.src_ens`(决定"立即重算"是否触发,16 B/group)。`HourlyMax`、ClosePolicy 输入缓存与 SwapPicker 候选可由样本与视图精确重算,`restore` 后重算;分片位置按模型 tick 的 UTC 时刻取模,不进状态。`path_settle_ms` = `close_period_ms`(路径节被丢弃后,下一次周期重算即与连续运行一致);`cold_start_ms` = 24 h(允许开仓规则要求 `sample_hours ≥ 24`)。`snapshot_bytes(p, n)` ≈ n × (52 KiB + 16 B)(环按 u16 编码)。
- **结构参数**:`pct_bin`、`pct_range`、`model_tick_ms`(决定采样点与箱号的解释)在 schema 中标 `structural`;改它们 → `on_params` 对受影响的 group 重建 `PercentileStats` 并报告重置,pipeline 随即对这些 group 预热注入。`pct_fill_max_ms` 不是结构参数。

### ClosePolicy(Model,每 `close_period_ms` + 发现成交后立即)

输入(每持仓的 (account, group)):入场价差(当前轮次配对成交均价,来自账本视图)、开仓成本线 `floor`、`fees_out`、持仓小时、实时日化资金费、连续负费率期数(`FundingStats.neg_streak`)、已收资金费(账本)、`SpreadStats`、强平距离与 ADL 档(`PortfolioView::perp` 的强平价与 ADL 档,数据来自 `MarginReport`)、两腿差、币价较入场涨幅、`entry_mode`、资金占用。输出 `(close_th, breakeven, hard_action, reason)` 与 `HardTriggers`。步骤(`close_policy.effective_close_threshold` 9/29 版 + 引擎覆盖):

1. **硬动作优先**(依次):强平距离 < 8% → `close_all`;币价较入场 ≥ +80% → `close_all`;两腿差 ≥ 5% → `hedge_fix`(按[上文两腿差规则](#平仓残余与硬动作)执行);强平距离 < 15% 或 ADL ≥ 4 → `reduce_half`。硬动作时 `close_th = th_max`。同时输出 `HardTriggers` 价位:空头强平距离 = `(liq_px − mark) / mark`,故 `liq_close_px = liq_px / (1 + 8%)`、`liq_reduce_px = liq_px / (1 + 15%)`、`runaway_px = entry_px × (1 + 80%)`;快速部分在每个 `Mark` 上比较,不等下一分钟。
2. **保本线** `be = entry − fees_out (+ 已收资金费,仅当 be_include_funding;现为 false)`;`fees_out` 缺失时取 `floor − 0.2%`。
3. **carry 仓**(`entry_mode = carry`),`low_h` = 资金费低于门槛已持续的小时数:
   - 资金费为负或连续 ≥ 2 期负 → `th = be + give`(`give = carry_giveback`,仅当 `low_h ≥ carry_stale_h 12`,否则 0);
   - 日化 < `carry_min_daily / carry_fund_low_div`(= 0.075%)→ 耐心:占用 < 60% 多要 0.15%、24 h 内线性退到保本;60–80% 多要 0.05%、12 h 内退到保本;≥ 80% 立即保本:`th = be − prem + give`,`prem = p0 × max(0, 1 − low_h / dh)`,`give` 仅当 `low_h ≥ carry_stale_h + dh`;下限 `th_min −1%`;
   - 资金费仍在收 → `th = be − max(carry_profit 1.0%, 日化 × carry_hold_days 2)`,下限 `carry_th_floor −8%`(不被 −1% 截断:截了就等于按不到 2 天资金费的利润平掉)。
   - 再按 `carry_close_mode`:`profit` → `th = min(th, be − carry_min_profit 0.05%)`(只在盈利时自动平);`manual` → 不设线(只保留硬动作);`policy` → 原样。
4. **普通仓**:`base = entry − floor − profit_min 0.5%`;乐观目标 `opt = min(base, sell_p20_72h)`(样本不足用 base),不低于 −1%;抬线时长 `decay = 48 h`(资金费为负或连续负 ÷ 4,资金费 < `funding_min_daily 0.05%` ÷ 2);`th = opt + (be − opt) × min(1, hold_h / decay)`(`be ≤ opt` 时直接 be);资金费为负且持仓 ≥ 12 h → `th = be`;持仓 ≥ 96 h 且有 `sell_p40_24h` → `th = max(th, p40)`,否则持仓 ≥ 48 h 且有 `sell_p20_24h` → `th = max(th, p20)`;上限 `be + max_giveback(0)`;下限 −1%。
5. **换仓覆盖**:快速部分在 `swap_until` 前对被选中的 group 取 `th = max(th, be + swap_exit_over_be 0.1%)`(仅 `carry_close_mode = policy`)。

### SwapPicker(换仓,`_maybe_swap`)

- Model 每 10 s 计算候选:在持仓 group 中选 `entry_mode = carry`、无在途、`held_usd ≥ 5`、资金费已低(`fund_low_since` 已设)、保本线已知、`sell ≤ be + 0.1%` 的仓位,取 `sell − be` 最小者 → `SwapCandidate{release_group, exit_th = be + 0.1%}`。
- 快速部分在开仓前门 5 失败时检查:`carry_close_mode = policy`、本 group `armed_eff` 且非只看、`buy ≥ eff_open_th + swap_min_edge 0.5%`、距上次换仓 ≥ 10 min、候选 advice 有效 → 对 `release_group` 设 `swap_until_ens = now.engine_ns + 10 min`(平仓线按上节第 5 步覆盖)。10 min 内最多换一个。本次信号不开(资金释放后由后续信号开)。

### ImpactCurve(冲击探测,Model,每 30 min)

用链上报价(chain-core 按路由表为每条路由给出小额探测额与 Sizer 目标额两个规模的报价;可选 L0 池镜像可给更多点,[hft-chain-evm](../../crates/hft-chain-evm/README.md)):上限金额报价相对 `probe_base $5` 报价的冲击 > 0.3% → 上限减半,最多 4 次;缺少某规模报价时用不小于它的最近规模点保守判定。输出 `ImpactCurve`。

### StockToken 档(`profile = stock`,E 类)

`Cdex` 核心 + 三条规则([07](../../design/07-策略层.md)):① 无资金费项(carry 通道关闭,`FundingGate` 不参与);② 更严的 `max_abs_spread` 与更小的 `slow_cap`(pipeline 按标的给);③ 市场时段门:refdata 的交易日历/维护窗口判定美股休市时段,休市禁 Open(持仓的 Reduce/Hedge/硬动作照常)。Fast 腿为 CEX 股票永续(SNDK/MU/…),Chain 腿为链上 bStocks/xStocks/Ondo(别名 `{SYM}ON/{SYM}X/{SYM}B` 由 pipeline 解析)。

### 与引擎的分工(本策略专有的规则)

| 规则 | 位置 |
|---|---|
| 资金公式 `wallet × leverage × deployable` | 本策略 `capital_in_use`(参数 `leverage`、`deployable_frac`、`capital_base`) |
| 两腿差补腿阈值 5% | 本策略参数 `naked_fix_tol`,经 `LegLink.tolerance` 交给执行层;尘埃线 $5 是执行层 `dust_notional`(唯一归属 hft-exec) |
| 残余清理(两腿都 < $5 且 CEX 空多于链上 → reduce-only 平差额) | 本策略[残余](#平仓残余与硬动作) |
| 并行回滚后 30 min 回到 chain-first | 本策略 `parallel_fallback_ms`(`Throttle` 回退期) |
| 轮次的 `entry_mode`(normal/spike/carry/manual) | 本策略状态与意图 `tag`;账本是否按轮次归档以 [hft-ledger](../../crates/hft-ledger/README.md) 为准 |

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `Cdex`、`CdexModel`、`DESC` |
| `src/params.rs` | `define_params!` 声明 `CdexParams`;`check` 跨字段校验;换算为 `CdexParamsNs` |
| `src/fast/{group,acct}.rs` | `CdexGroup`、`AcctAgg`(`#![deny(clippy::float_arithmetic)]`) |
| `src/fast/{channels,gates,sizer,cost}.rs` | 三条通道、开仓前门与 `FundingGate`、Sizer、成本线组装 |
| `src/fast/{exec,close,hard,manual}.rs` | 意图构造、平仓/残余、硬动作与补腿、手动任务 |
| `src/advice.rs` | 六种载荷(`AdvicePayload`,两部分共用) |
| `src/desc.rs` | Note 码表、报表字段、`cmds`(`open`/`close`) |
| `src/warmup.rs` | `SpreadWarmup` 解码 |
| `src/model/{stats,close_policy,impact_curve,swap_picker}.rs` | Model 各部分 |
| `tests/scenarios/*.rs` | SDK 场景测试(事件脚本 → 期望意图) |
| `tests/py_parity/` | 与现 Python 实现的对照 |

</details>

## 延迟与资源契约

快速部分开销计入分段 `decided_tsc − proc_tsc` 内的策略份额(SDK 规定的策略预算,[hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md));Model 在 advisor 线程。测量:Criterion,在[参考机](../../benchmarks/README.md#参考机与-ci)上判定;dev2 只做功能验证。

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `on_event` 快侧未命中(绝大多数 tick) | ≤ 15 ns | ≤ 40 ns | `strategies/cdex/tick_miss` |
| `on_event` 命中 → 订单/swap 意图(含 20 档 VWAP) | ≤ 300 ns | ≤ 1 µs | `strategies/cdex/tick_fire` |
| `on_event` 慢侧更新(链上报价 → 重算触发价) | ≤ 60 ns | ≤ 150 ns | `strategies/cdex/slow_update` |
| `on_event(Advice)` 解码并缓存 | ≤ 100 ns | ≤ 300 ns | `strategies/cdex/advice` |
| `on_event` → 构造 `TargetIntent` | 不适用(本策略不发目标意图) | — | — |
| `on_event(Exec)` | ≤ 80 ns | ≤ 250 ns | `strategies/cdex/exec_update` |
| `on_event(Risk)`,每个受影响 group | ≤ 150 ns | ≤ 400 ns | `strategies/cdex/risk_event` |
| `on_event(Mark)` 硬动作比较(本策略额外登记) | ≤ 10 ns | ≤ 30 ns | `strategies/cdex/mark` |
| `Model::on_event` 行情采样(advisor) | ≤ 1 µs | ≤ 5 µs | `model/cdex/sample` |
| `Model::on_event` 模型 tick(分片后) | ≤ 300 µs | ≤ 1 ms | `model/cdex/tick` |
| 其中:ClosePolicy 每 group / 分位查询每 group / SwapPicker 40 group | ≤ 2 / 20 / 5 µs | ≤ 10 / 60 / 20 µs | `model/cdex/close_policy`、`model/cdex/stats_query`、`model/cdex/swap_picker` |

- 快速部分零分配(`groups` 在 `new`/`on_params` 改变 group 集合时于命令路径重建)、无系统调用、无 `dyn`、无 f64。
- 内存:`CdexGroup` ≤ 512 B,40 group × 2 实例 ≈ 40 KiB;Model ≈ 2.1 MiB/group(40 group ≈ 85 MiB/实例),经 `CdexModel::mem_bytes` 声明,hft-core 按声明装配(超出 `advisor.mem_budget_mib` 时由运维调大,不改引擎规格)。

## 测试与验收

- **单测与场景测试**(SDK `testing` 特性的 `MockViews`/`CtxHarness`/`ModelHarness`;需要成交与故障的引擎级场景放在 `strategies/registry/tests/cdex_*.rs`,用 hft-testkit 场景与 mock 链上 venue):正常开/平;Fast 腿盘口 10 s 不变而连接存活帧持续 → 不判过期、照常开平;Fast 连接静默超过 `stale_fast_ms` → `Stale`;重启(`MockViews::restart`)后冷却、回退期、换仓与链上补腿间隔按 engine_ns 继续,重启前的 advice(`basis_seq < boot_seq`)全部过期;确认窗口被一次不达标打断;过期报价;三条通道互斥与优先级;carry 仓不被价差通道加仓、价差仓不被 carry 加仓;spike 每轮只一笔;chain-first 包含 → 对冲、未包含 → 零成本放弃且不重试同一意图;并行回滚后 30 min 回到 chain-first;硬动作在 Shadow 下只记录;硬动作两种腿顺序;halted 时平仓与硬动作照常、开仓被拒;残余清理;链上侧补腿与执行层 CEX 侧补腿不重复;halt 与 `Resume{Halted}`;手动开/平分笔与次数/时长上限(载荷经 `params::encode_cmd` 构造,`mode = carry/normal` 各一例,`group = GROUP_NONE` → `ManualRejected`;`close` 带 `reduce_only` 时全部意图为 Reduce/Flatten);advice 的 `basis_tsc` 过期而到达时刻新 → 不开;Broken → `Done{cleared_by_engine}` 后按实际两腿重算、链上多出时发链上补腿;`SpreadWarmup` 金样解码、乱序与重复注入结果相同、注入后 `sample_hours ≥ 24`;每个登记的 Note 码至少触发一次;换仓(10 min 只一次、仅 policy 模式);advice 过期 → 不开;有效模式变化 `Mode` 事件后开仓状态重置;门控拒绝不计 `consec_fail`;`MarginStress` L2 分步减半;`External{Liquidation}` 后链上补腿;`External{Manual}` → 冻结该 group(无开平、无链上补腿、`ManualExternal`),两腿差回到 `naked_fix_tol` 以内后恢复,人工 `close` 命令照常执行;`Notice{Delisting}` 到时 close_all;结算周期 8 h → 4 h 后日化口径改变;stock 档休市禁开。
- **与现 Python 的对照(py_parity)**:对 `close_policy.effective_close_threshold`、`sizing.compute`(去掉网络部分)、`cex_depth.check`、`token_check.cost_floor/round_trip_cost`、`spread_engine` 的 `evaluate()` 确认逻辑与通道判定、`carry_th`、`_carry_ok`、`_maybe_swap`,用生成的输入网格(≥ 10,000 组)两边求值,按 Python 侧显示精度取整后比较(价差类差 ≤ 0.0001%;分位类差 ≤ 半个箱宽)。Python 快照放 `fixtures/strategies/cdex/py/`,来源 `E:\trading\analysis\util0929\src`(9/29–9/30 版本)。
- **Model 确定性**(SDK 夹具):同一事件脚本跑两遍 Advice 字节相同;随机位置检查点(两节)→ 恢复 → 继续,输出与 Model 状态都与连续运行逐字节相同;丢弃路径节时 `close_period_ms` 之后的输出与连续运行相同;在不同位置重启,分片位置不变;路径金样(`fixtures/strategies/cdex/ckpt/path.bin`)。
- **金样**:AIN 9/20 插针区块序列(`analysis/DEX加速与AIN插针复盘.md` 的块号与价格)喂 `ConfirmWindow{min_blocks = 2}` → 不触发;NAORIS 9/25 四笔 → `min_out` 由信号价反推。
- **离线信号对照**(cdex 迁移验收,不属于首次上线门槛):正式账户 9/24–9/30 的 `signals.jsonl` 对应时段(BBO + 链上报价样本)转为事件序列,开/平信号时刻与旧引擎一致率 ≥ 95%,差异逐条解释(主要来自采样粒度变化)。
- **DST**:本策略 + 风控 + OMS + 执行层 + mock 交易所/链,故障注入(抢跑 revert、Unknown、断连、限速、重组),断言裸敞口不超过执行层阈值持续 > 1 块 + 对冲时延。
- **回放**:`replayer verify` 逐字节一致(Meta 除外);Advice 从 journal 读取。
- **验收**(迁移时):以上全过;CEX 腿在模拟账户与交易所测试网上、链上腿在 mock 链上 venue 与链上测试网上(链上测试网可用性:待核实(方法:迁移前在目标链测试网部署 SwapGuard 并跑一次开平))跑通开平、对冲、补腿与对账;迁移切换即正式实盘(旧框架先停开收尾,新实例对账接管),按 [migration runbook](../../docs/runbooks/migration.md) 执行,切换后首批真实成交逐笔核对两腿间隔、对冲、记账与交易所/链上一致、无人工补腿。

## 依赖

- 内部:[`hft-strategy-sdk`](../../crates/hft-strategy-sdk/README.md)(启用 `chain` feature)、[`hft-strat-lib`](../../crates/hft-strat-lib/README.md)。经 SDK 间接使用 L0 类型(`hft-types`、`hft-refdata`、`hft-event`)。
- 外部:无;dev 依赖 `proptest`、`criterion`。py_parity 的期望值由 Python 离线生成为 fixture 文件,Rust 测试只读文件,不依赖 `pyo3`。
- 禁止:任何引擎 crate(`hft-sequencer`、`hft-oms`、`hft-risk`、`hft-ledger`、`hft-exec`、`hft-journal`、`hft-venue*`、`venue-*`、`hft-chain-evm`)、其他 `strategies/*`;I/O、`std::time`、`HashMap` 迭代、快速部分 f64、`dyn Trait`。

## 延伸阅读

- [07 策略层](../../design/07-策略层.md)、[06 §2–4 链上执行与 chain-first](../../design/06-链上执行层.md)、[05 §2.3 LegLink](../../design/05-CEX执行层.md)
- [research/00 §2.5–2.6 价差引擎、sizing、close_policy、watchlist](../../design/research/00-existing-codebase-inventory.md)
- 现代码:`E:\trading\analysis\util0929\src\monitoring\{spread_engine,sizing,close_policy,watchlist}.py`、`strat0926/new/core/cex_depth.py`
- 复盘:`E:\trading\analysis\DEX加速与AIN插针复盘.md`、`cdex-正式账户复盘-2026-09-25.md`、`cdex-资金利用率与高资金费持有策略复盘-2026-09-29.md`
- [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md)、[hft-strat-lib](../../crates/hft-strat-lib/README.md)、[hft-exec](../../crates/hft-exec/README.md)、[strategies/AGENTS.md](../AGENTS.md)、[control-plane](../../docs/subsystems/control-plane.md)
- 第八轮裁决:[round8-decisions](../../design/_work/round8-decisions.md)

## 已知限制与后续工作

- F 类(链上 backrun)不在本 crate:由 chain-core 独立模块承担([07](../../design/07-策略层.md))。
- Solana 链上腿是可选扩展(触发条件:有明确的 Solana 机会与预算);接入时 `SwapIntent` 字段(`valid_blocks` → blockhash 有效期)按 [hft-chain-solana](../../crates/hft-chain-solana/README.md) 确定。
- 名单增删是 pipeline 的事:新币必须已在当日 refdata 宇宙里,pipeline 只下发新的 `StrategyParams`(热替换);宇宙之外的新币要等次日 refdata。本 crate 不做找币。
- 模拟账户([hft-sim](../../crates/hft-sim/README.md))只模拟 CEX 腿;本策略的链上腿在正式实盘前只能用 mock 链上 venue 与链上测试网验证。
- 波动闸门(60 s ≤ 0.8% / 5 min ≤ 2%)在旧引擎默认关闭(9/28 用户:由两腿限价把关),本策略不实现。

### 开发备忘

<details><summary>给实现者</summary>

- **SDK 接口**:本策略需要的接口 SDK 均已提供(`EventRef::Pool`/`SubsBuilder::pool`、`Ctx::request_halt`、`EventRef::Manual` + `cmds` 描述符、`EventRef::Warmup`、`PerpPosView.liq_px/adl_quantile`);链上报价没有"按规模取最新报价"的视图,快速部分在 `CdexGroup` 里缓存最近一条 `PoolView`。执行层遇到"多出的一腿是链上腿"时不动作、只写 Note(自动补腿、残余修剪、外来成交默认处理、Broken 自动有界平仓同此),以 [hft-exec](../../crates/hft-exec/README.md) 为准。
- `cex_slip`(第二腿 IOC 越价):待标定(方法:用 9/24 以来 CEX 腿漂移数据标定;06 §4:1.4 s 内漂移中位 0、p99 0.1%;旧 Python 用"CEX 限价 IOC 最多等 3 s",新栈为纯 IOC)。`stress_step_ms`、`delist_lead_ms` 迁移前定。
- **与旧引擎的行为差异**(只有这三处,迁移前与运营确认):① halted 只拦开仓(旧引擎停开平);② 硬动作默认 `perp_first`(旧引擎与常规平仓同路径 = `chain_first`,可用参数切回);③ 硬动作绕过冷却与在途,只受 `hard_rearm_ms` 限制(旧引擎在途时不发硬动作)。
- **引擎保证金分档与本策略强平距离硬动作**:引擎默认 L2 = `liq_dist ≤ 15%`(通知减仓)、L3 = `≤ 10%`(强制减永续腿);本策略 `liq_reduce` 15% 与引擎 L2 同线、`liq_close` 8% 在引擎 L3 之后才会触发。迁移前与运营确认:保留本策略的 `close_all`(会同时卖出链上腿,引擎 L3 只减 CEX 腿)还是把 `liq_close` 提到 10% 以上,使两腿同时退出先于引擎单腿减仓。
- **72 h 预热**:迁移首日、`LAYOUT_VERSION` 提升后与 group 被重置(`ModelGroupReset`)后经 `SpreadWarmup` 预热注入(见[价差样本统计](#价差样本统计72-h-口径允许开仓规则与-carry-线model));之后靠 Model 检查点(键不含参数、refdata、二进制版本),改非结构参数与发版不需重新预热。迁移手册写明注入步骤与核对方法(注入后 `SampleStats.sample_hours` 应 ≥ 24)。
- 9/29 复盘建议的"小仓试跑老板规则"(资金费 ≥ 0.15%、价差 ≥ 0.5% 持续 60 s、不低于保本出场、资金费没了最多等 24 h)只需参数表达:`carry_line = cost` → 改为固定开仓线 0.5%、`carry_confirm_ms = 60000`、ClosePolicy 增 `max_wait_after_funding_h`(待老板在 A/B 中选择,A = 一直拿着 = 现 `profit` 模式)。
- 实现顺序:params/schema → group/acct → 通道与前门 → exec/close/hard → advice 编解码 → Model(stats → close_policy → impact → swap)→ py_parity → 场景测试 → 手动任务。
</details>
