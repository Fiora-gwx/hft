---
component: hft-ledger
prefix: LED
spec: README.md
---

# hft-ledger 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | LED-246、LED-247 |
| 目录 | 不适用:非规范 |
| 使用本组件 | LED-263、LED-003、LED-005、LED-074、LED-159、LED-231、LED-246 |
| 成交增量与净量 | LED-010–LED-020 |
| 权益数据(`PortfolioView` 的字段语义) | LED-041、LED-161、LED-179、LED-220–LED-226、LED-241、LED-242 |
| 配置(`LedgerConfig`,`EngineConfig` blob 的 `ledger` 段) | LED-001、LED-052、LED-098、LED-101、LED-120、LED-121、LED-136、LED-193、LED-194、LED-200、LED-211–LED-213、LED-221、LED-269 |
| 实现说明 | 不适用:见子节 |
| 数据结构 | LED-002–LED-004、LED-233、LED-237、LED-247 |
| 仓位与已实现(平均成本法) | LED-010、LED-021–LED-025、LED-030–LED-039、LED-240、LED-241、LED-243 |
| 余额初始化、校验与基线 | LED-039–LED-046、LED-260–LED-262 |
| 外部成交(`ExternalFill`) | LED-050–LED-081、LED-188、LED-240、LED-245 |
| 冷消费者(`JournalLedger`) | LED-263–LED-267 |
| 资金费 | LED-090–LED-104、LED-136–LED-138、LED-240、LED-245、LED-260 |
| 资金费时刻归一化 | LED-091、LED-110–LED-116 |
| 资金费对账校正 | LED-270、LED-268、LED-114、LED-120–LED-139、LED-244、LED-261 |
| 保证金数据 | LED-150–LED-161 |
| 人工干预的采纳(`AdoptExchange`) | LED-123、LED-170–LED-179、LED-243 |
| 划转与模拟账户 | LED-185–LED-188、LED-242、LED-260 |
| 未实现与当日 PnL(增量维护) | LED-160、LED-190–LED-194 |
| 轮次(round) | LED-176、LED-200–LED-203 |
| USDT 统一计价 | LED-158、LED-210–LED-215 |
| 链上腿(optional,feature `chain`) | 不适用:里程碑外 |
| 快照行格式(节 1、13、22;草案,实现时以代码与编译期断言为准) | LED-053、LED-061、LED-104、LED-138、LED-139、LED-230–LED-238 |
| 规划源文件 | 不适用:非规范 |
| 延迟与资源契约 | LED-004、LED-250–LED-258 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**夹具**(各用例的"前置"按名字引用;实现在 `crates/hft-ledger/tests/common/mod.rs`,参数一律显式赋值,不取 `Default`):

- `rd_lf()`:refdata。资产 `USDT`(Stable,`px_exp = −4`,`qty_exp = −6`)、`USDC`(Stable,`px_exp = −4`,`qty_exp = −6`)、`BTC`、`ETH`、`BNB`(三者 `px_exp = −4`,`qty_exp = −8`);venue `S1`(`VenueId::BINANCE_SPOT`,现货,交易所 E1)、`P2`(`VenueId::OKX_SWAP`,USDT 永续,交易所 E2,保证金资产 USDT)、`S3`(`VenueId::GATE_SPOT`,现货,交易所 E3)(refdata 按 VenueId 号校验市场,所以取这三个号);inst `BTC/S1`(`InstId(11)`,tick 0.01,lot 0.00001,`min_notional` 5)、`BNB/S1`(12)、`USDC/S1`(13,USDC 的参考 inst)、`BTC/P2`(21,lot 0.001,`funding_interval_s = 28 800`,`funding_anchor_s = 0`)、`ETH/P2`(22,`funding_interval_s = 28 800`,`funding_anchor_s = 0`)、`BTCUSDC/S3`(32,计价 USDC);账户 `A1`(Live,venues {S1, P2},绑定实例 1)、`A2`(Live,venues {S3, P2},实例 2)、`A9`(Sim,venues {S1, P2},实例 3,`initial_balances` = S1 USDT 10 000、P2 USDT 10 000)。inst 号从 1 起稠密:其余号为 `S3` 上已下架的填充 inst,其中 `InstId(1)` 状态为 `Halted`;每个 venue 一档手续费。资产 dust(测试 refdata 给定):USDT 0.01、USDC 0.01、BTC 0.00001、BNB 0.0001。
- 每个 `VenueCaps.fee_asset_rule` 与 `funding_settle` 在用到它的用例"前置"里写明。
- group(`A1` 绑定的实例,`on_groups_changed` 已调用,cell 均非 Shadow):`G1` = 腿 0 `BTC/S1`、腿 1 `BTC/P2`(对冲比 1/1);`G3` = 腿 0 `ETH/P2`;`G4` = 腿 0 `BNB/S1`(只在注明加入 `G4` 的用例里存在,其余用例中 BNB 不属于任何 group)。
- `cfg_lf()`:`LedgerConfig` 逐字段显式赋值——`par_assets = {USDT}`、`stable_par_fallback = {USDC}`、`rate_max_age_ms = 60 000`、`valuation = {spot: Bid, perp: Mark, include_baseline: false, include_unassigned: true}`、`round_dust_notional = 5`、`balance_check_quiet_ms = 2 000`、`funding_tol_abs = 0.05`、`funding_tol_rel = 0.005`、`funding_recon_lag_ms = 600 000`、`funding_recon_window_ms = 172 800 000`、`funding_hist_cap = 4 096`、`funding_push_seen_cap = 64`、`ext_dedup_cap = 1 024`;`Capacities.max_subs_per_inst = 4`。用例改动其中某项时在"前置"里写出新值。
- `boot_a1(t0)`:`on_run_start(t0)`;按序 `on_balance(quiet = false)`:`A1/S1` USDT free 100 000、`A1/S1` BTC free 0、`A1/S1` BNB free 10、`A1/P2` USDT free 50 000(locked 均 0,`ex_ts_ns` 均为 `D`,所以 `A1/P2` 的 `hist_from_ns = D`);`on_ref_px(BTC/S1, 59 999.99, 60 000.01)`、`on_ref_px(BNB/S1, 599.99, 600.01)`、`on_ref_px(USDC/S1, 0.9999, 1.0001)`;时刻均为 `t0`。此后 BTC 换算率 60 000.00、BNB 600.00、USDC 1.0000。
- 记法:时刻写成秒(`t = 10 s` 即 `engine_ns = 10 000 000 000`);UTC 时刻以 `D = 2026-10-01T00:00:00Z` 为原点。数量、价格与金额写十进制,raw = 十进制 × 10^(−exp)(资产指数见 `rd_lf`;`Notional` 指数 −6,`Rate` 指数 −9;`cost` 与 `realized_quote` 的指数为 `px_exp + qty_exp` = −12)。"成交" `F(side, inst, qty, px, fee, fee_asset, group, leg)` 是一条 `FillDelta`,`net_qty` 由 OMS 按净量规则填写(用例另说除外)。

### LED-001 LedgerConfig 各字段的默认值

- 规则:[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`LedgerConfig::default()`(本用例是默认值的唯一用例;其余用例一律用 `cfg_lf()` 显式赋值);`ref_inst` 用 `rd_lf()` 生成
- 实现:`crates/hft-ledger/tests/config.rs::led_001_config_defaults`

| 行 | 字段 | 期望 |
|---|---|---|
| LED-001.1 | `par_assets` | `{USDT}` |
| LED-001.2 | `stable_par_fallback` | `{USDC, USD1, FDUSD}` |
| LED-001.3 | `rate_max_age_ms` | 60 000 |
| LED-001.4 | `valuation` | `{spot: Bid, perp: Mark, include_baseline: false, include_unassigned: true}` |
| LED-001.5 | `round_dust_notional` | 5 USDT(raw 5 000 000) |
| LED-001.6 | `balance_check_quiet_ms` | 2 000 |
| LED-001.7 | `funding_tol_abs` / `funding_tol_rel` | 0.05 USDT(raw 50 000)/ 0.005(raw 5 000 000) |
| LED-001.8 | `funding_recon_lag_ms` | 600 000 |
| LED-001.9 | `funding_recon_window_ms` | 172 800 000 |
| LED-001.10 | `funding_hist_cap` | 4 096 |
| LED-001.11 | `funding_push_seen_cap` | 64 |
| LED-001.12 | `ext_dedup_cap` | 1 024 |
| LED-001.13 | `ref_inst[USDC]`(由 refdata 生成) | `USDC/S1` |

### LED-002 每 venue 资产与库存容量是编译期常量

- 规则:[数据结构](README.md#数据结构)
- 级别:unit
- 里程碑:M2
- 前置:无
- 步骤:
  1. 读常量 `MAX_ASSETS_PER_VENUE`、`MAX_INV_PER_VENUE`
- 期望:
  - `MAX_ASSETS_PER_VENUE == 64`,`MAX_INV_PER_VENUE == 128`;`LedgerConfig` 与 `EngineConfig` 的 `ledger` 段都没有同名字段
- 实现:`crates/hft-ledger/tests/config.rs::led_002_capacity_constants`

### LED-003 某 venue 的资产数超过 64 时 new 返回 LedgerConfigError

- 规则:[数据结构](README.md#数据结构)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M2
- 前置:`rd_lf()` 的变体,`S1` 上登记的资产数按行设定;`cfg_lf()`
- 实现:`crates/hft-ledger/tests/config.rs::led_003_assets_per_venue_limit`

| 行 | `S1` 资产数 | 期望 |
|---|---|---|
| LED-003.1 | 63 | `Ok(Ledger)` |
| LED-003.2 | 64 | `Ok(Ledger)` |
| LED-003.3 | 65 | `Err(LedgerConfigError::TooManyAssets { venue: S1, n: 65 })` |

### LED-004 new 之后热入口零分配

- 规则:[数据结构](README.md#数据结构)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M2
- 前置:`rd_lf()`、`cfg_lf()`、`boot_a1(0 s)`;测试进程装计数分配器(`hft-testkit` 的分配计数器)
- 步骤:
  1. `Ledger::new` 之后清零分配计数
  2. 依次调用 10 000 次:`apply_fill`、`apply_fee`、`apply_external`、`apply_funding`、`apply_funding_push`、`reconcile_funding`、`funding_window_sweep`、`on_ref_px`、`on_mark`、`on_margin`、`equity`(输入为随机合法值,去重环与资金费环各填满一次)
- 期望:
  - 每个入口的分配计数(`alloc`/`alloc_zeroed`/`realloc`,hft-testkit `CountingAlloc` 只计这些)都为 0
- 实现:`crates/hft-ledger/tests/config.rs::led_004_zero_alloc_after_new`

### LED-005 公开入口的时间参数只接受 engine_ns

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M2
- 前置:`trybuild` 编译失败用例集
- 步骤:
  1. 编译 `ledger.apply_fill(&f, Tsc(5))`
  2. 编译 `ledger.on_mark(inst, px, Tsc(5), &mut touched)`
- 期望:
  - 两段代码都编译失败,错误为类型不匹配(期望 `i64`);公开 API 文档导出中没有任何参数或返回值的类型是 `Tsc`
- 实现:`crates/hft-ledger/tests/api.rs::led_005_entries_take_engine_ns_not_tsc`

### LED-010 净量规则按扣费资产与方向逐行成立

- 规则:[成交增量与净量](README.md#成交增量与净量)、[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:table
- 里程碑:M2
- 前置:`rd_lf()`、`cfg_lf()`、`boot_a1(0 s)`;`S1` 的 `fee_asset_rule` 按行设定;卖出行之前先入账 `F(Buy, BTC/S1, 2.0, 60 000, 0, USDT, G1, 0)`;每行在 `t = 10 s` 入账一笔,`FillDelta.net_qty` 已按规则填写
- 实现:`crates/hft-ledger/tests/fill.rs::led_010_net_qty_rules`

| 行 | `fee_asset_rule` | 成交 | 期望 |
|---|---|---|---|
| LED-010.1 | `Quote` | `F(Buy, BTC/S1, 1.0, 60 000, 6, USDT)` | 复算 `net_qty` 1.0;`A1/S1` BTC `total` +1.0,`inv(BTC, G1)` +1.0;USDT `total` −60 006;`G1.pnl.fee_cex` −6;`net_adjust = None` |
| LED-010.2 | `Quote` | `F(Sell, BTC/S1, 1.0, 60 000, 6, USDT)` | `net_qty` 1.0;BTC `total` −1.0,`inv` −1.0;USDT `total` +59 994 |
| LED-010.3 | `ReceivedAsset` | `F(Buy, BTC/S1, 1.0, 60 000, 0.001, BTC)` | `net_qty` 0.999;BTC `total` +0.999,`inv` +0.999,`InstPos.qty` +0.999;USDT `total` −60 000;`fee_cex` −60(0.001 × 换算率 60 000) |
| LED-010.4 | `ReceivedAsset` | `F(Sell, BTC/S1, 1.0, 60 000, 6, USDT)` | `net_qty` 1.0;USDT `total` +59 994 |
| LED-010.5 | `ReceivedAssetUnlessDiscount{BNB}` | `F(Buy, BTC/S1, 1.0, 60 000, 0.01, BNB)` | `net_qty` 1.0;BTC `total` +1.0;BNB `unassigned` 10 → 9.99;`fee_cex` −6 |
| LED-010.6 | `ReceivedAssetUnlessDiscount{BNB}` | `F(Buy, BTC/S1, 1.0, 60 000, 0.001, BTC)` | `net_qty` 0.999 |
| LED-010.7 | `Configured(BNB)` | `F(Sell, BTC/S1, 1.0, 60 000, 0.01, BNB)` | `net_qty` 1.0;BNB `unassigned` −0.01 |
| LED-010.8 | `Configured(BNB)` | `F(Buy, BTC/S1, 1.0, 60 000, 0.001, BTC)`(回报的实际 `fee_asset` 是 BTC) | `net_qty` 0.999(回报的 `fee_asset` 优先于规则) |
| LED-010.9 | `Configured(BTC)` | `F(Sell, BTC/S1, 1.0, 60 000, 0.001, BTC)` | `net_qty` 1.001;BTC `total` −1.001,`inv` −1.001;USDT `total` +60 000 |
| LED-010.10 | `Quote` | `F(Buy, BTC/P2, 1.0, 60 000, 30, USDT, G1, 1)` | `net_qty` 1.0;`InstPos(A1, BTC/P2).qty` +1.0;`A1/P2` USDT `total` −30,`MarginState.wallet` −30 |

### LED-011 OMS 填写的净量与复算一致时不返回 NetAdjust

- 规则:[成交增量与净量](README.md#成交增量与净量)
- 级别:unit
- 里程碑:M2
- 前置:`rd_lf()`、`cfg_lf()`、`boot_a1(0 s)`;`S1` 的 `fee_asset_rule = ReceivedAsset`
- 步骤:
  1. `apply_fill(F(Buy, BTC/S1, 1.0, 60 000, 0.001, BTC, G1, 0) 且 net_qty = 0.999, t = 10 s)`
- 期望:
  - `FillApplied.net_adjust == None`;本次调用不产生 `Note{NetQtyMismatch}`;`InstPos.qty` = 0.999
- 实现:`crates/hft-ledger/tests/fill.rs::led_011_consistent_net_no_adjust`

### LED-012 买入净量不一致时以复算值记账并返回负的 NetAdjust

- 规则:[成交增量与净量](README.md#成交增量与净量)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-011
- 步骤:
  1. `apply_fill(F(Buy, BTC/S1, 1.0, 60 000, 0.001, BTC, G1, 0) 且 net_qty = 1.0, t = 10 s)`
- 期望:
  - `InstPos.qty` = 0.999,`inv(BTC, G1)` = 0.999,BTC `total` = 0.999
  - `FillApplied.net_adjust == Some(NetAdjust { account: A1, venue: S1, group: G1, inst: BTC/S1, delta: −0.001 })`
  - 恰好一条 `Note{NetQtyMismatch}`,level P1
- 实现:`crates/hft-ledger/tests/fill.rs::led_012_buy_net_mismatch`

### LED-013 卖出净量不一致时 delta 按有符号持仓给出

- 规则:[成交增量与净量](README.md#成交增量与净量)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-011,但 `fee_asset_rule = Configured(BTC)`;先入账 `F(Buy, BTC/S1, 2.0, 60 000, 0, USDT, G1, 0)`
- 步骤:
  1. `apply_fill(F(Sell, BTC/S1, 1.0, 60 000, 0.001, BTC, G1, 0) 且 net_qty = 1.0, t = 10 s)`
- 期望:
  - 复算 `net_qty` 1.001;`InstPos.qty` = 0.999;`net_adjust.delta == −0.001`(`(1.001 − 1.0) × (−1)`)
  - `Note{NetQtyMismatch}` 一条,P1
- 实现:`crates/hft-ledger/tests/fill.rs::led_013_sell_net_mismatch_signed`

### LED-014 回报未带费用时按毛量记净量且不预估费用

- 规则:[成交增量与净量](README.md#成交增量与净量)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-011
- 步骤:
  1. `apply_fill(F(Buy, BTC/S1, 1.0, 60 000, 0, ASSET_NONE, G1, 0) 且 net_qty = 1.0, t = 10 s)`
- 期望:
  - `InstPos.qty` = 1.0;`fee_cex` 不变(0);`net_adjust == None`;无 `Note{NetQtyMismatch}`
- 实现:`crates/hft-ledger/tests/fill.rs::led_014_fee_absent_uses_gross`

### LED-015 晚到的基础资产手续费(买入)补差并返回 NetAdjust

- 规则:[成交增量与净量](README.md#成交增量与净量)
- 级别:unit
- 里程碑:M2
- 前置:LED-014 之后的状态
- 步骤:
  1. `apply_fee(FeeDelta { account: A1, venue: S1, inst: BTC/S1, group: G1, side: Buy, fee: 0.001, fee_asset: BTC }, t = 11 s)`
- 期望:
  - `InstPos.qty` 1.0 → 0.999;`inv(BTC, G1)` 1.0 → 0.999;BTC `total` 1.0 → 0.999
  - `G1.pnl.fee_cex` −60;`FeeApplied.net_adjust == Some(NetAdjust { A1, S1, G1, BTC/S1, delta: −0.001 })`
- 实现:`crates/hft-ledger/tests/fill.rs::led_015_late_base_fee_buy`

### LED-016 晚到的基础资产手续费(卖出)同样是 delta = −fee

- 规则:[成交增量与净量](README.md#成交增量与净量)
- 级别:unit
- 里程碑:M2
- 前置:`fee_asset_rule = Configured(BTC)`;先入账 `F(Buy, BTC/S1, 2.0, 60 000, 0, USDT, G1, 0)`,再 `F(Sell, BTC/S1, 1.0, 60 000, 0, ASSET_NONE, G1, 0)`(`InstPos.qty` = 1.0)
- 步骤:
  1. `apply_fee(FeeDelta { A1, S1, BTC/S1, G1, side: Sell, fee: 0.001, fee_asset: BTC }, t = 11 s)`
- 期望:
  - `InstPos.qty` = 0.999;`inv(BTC, G1)` = 0.999;`net_adjust.delta == −0.001`
- 实现:`crates/hft-ledger/tests/fill.rs::led_016_late_base_fee_sell`

### LED-017 晚到的计价资产手续费不产生 NetAdjust

- 规则:[成交增量与净量](README.md#成交增量与净量)
- 级别:unit
- 里程碑:M2
- 前置:LED-014 之后的状态
- 步骤:
  1. `apply_fee(FeeDelta { A1, S1, BTC/S1, G1, Buy, fee: 6, fee_asset: USDT }, t = 11 s)`
- 期望:
  - `A1/S1` USDT `total` −6;`G1.pnl.fee_cex` −6;`InstPos.qty` 仍为 1.0;`net_adjust == None`
- 实现:`crates/hft-ledger/tests/fill.rs::led_017_late_quote_fee`

### LED-018 无归属的晚到费用只记账户级

- 规则:[成交增量与净量](README.md#成交增量与净量)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`
- 步骤:
  1. `apply_fee(FeeDelta { A1, S1, BTC/S1, group: GROUP_NONE, side: Buy, fee: 6, fee_asset: USDT }, t = 11 s)`
- 期望:
  - `account(A1).day.fee_cex` −6;USDT `total` −6;`group(A1, G1)`、`group(A1, G3)`、`group(A1, G4)` 的 `pnl` 与 `day` 逐字段不变;返回的 `PnlDelta.group == GROUP_NONE`
- 实现:`crates/hft-ledger/tests/fill.rs::led_018_unattributed_fee_account_only`

### LED-019 FillDelta 布局为 72 B 且无隐式填充

- 规则:[成交增量与净量](README.md#成交增量与净量)
- 级别:unit
- 里程碑:M2
- 前置:无
- 步骤:
  1. 断言 `size_of`、`align_of` 与字段偏移
- 期望:
  - `size_of::<FillDelta>() == 72`,`align_of == 8`;偏移:`account` 0、`venue` 1、`side` 2、`class` 3、`flags` 4、`leg_ix` 5、`inst` 8、`group` 12、`qty` 16、`net_qty` 24、`px` 32、`fee` 40、`fee_asset` 48、`slot` 52、`gen` 56、`seq` 64;各字段大小之和(含 `_pad*`)= 72
- 实现:`crates/hft-ledger/tests/layout.rs::led_019_filldelta_layout`

### LED-020 FillDelta 的 class 与 flags 码值往返

- 规则:[成交增量与净量](README.md#成交增量与净量)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-ledger/tests/layout.rs::led_020_filldelta_codes`

| 行 | 码 | 期望 |
|---|---|---|
| LED-020.1 | `class` Open / Reduce / Hedge / Flatten / Reversal / Adjust / External | 1 / 2 / 3 / 4 / 5 / 6 / 7,`u8 → enum → u8` 往返相等 |
| LED-020.2 | `class` 0 与 8 | 解码返回错误,不 panic |
| LED-020.3 | `flags` PX_EST / LATE / OVERFILL / EXT_DEDUP | 1 / 2 / 4 / 8 |

### LED-021 平台币扣费在 unassigned 足够时只扣 unassigned

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`(BNB `unassigned` 10、`baseline` 0);`S1` `fee_asset_rule = ReceivedAssetUnlessDiscount{BNB}`
- 步骤:
  1. `apply_fill(F(Buy, BTC/S1, 1.0, 60 000, 0.01, BNB, G1, 0), t = 10 s)`
- 期望:
  - BNB `unassigned` 9.99,`baseline` 0,`total` 9.99;各 group 的 `inv(BNB, ·)` 均为 0;`G1.pnl.fee_cex` −6;无 `Note{InvShortfall}`
- 实现:`crates/hft-ledger/tests/fill.rs::led_021_discount_fee_from_unassigned`

### LED-022 平台币 unassigned 不足时余下扣 baseline 并保持 NON_STRATEGY

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)` 但 `A1/S1` BNB 初始化为 5;`set_balance_baseline(A1, S1)`(BNB `baseline` 5、`bflags` 含 `NON_STRATEGY`);再入账 `ExternalFill{Manual, Buy, BNB/S1, 0.005, px 600}`(无候选,记 `unassigned`);此时 BNB `baseline = 5`、`unassigned = 0.005`、`total = 5.005`;`S1` `fee_asset_rule = ReceivedAssetUnlessDiscount{BNB}`
- 步骤:
  1. `apply_fill(F(Buy, BTC/S1, 1.0, 60 000, 0.01, BNB, G1, 0), t = 10 s)`
- 期望:
  - BNB `unassigned` 0,`baseline` 4.995,`total` 4.995;`bflags` 仍含 `NON_STRATEGY`;无 `Note{InvShortfall}`
- 实现:`crates/hft-ledger/tests/fill.rs::led_022_discount_fee_spills_to_baseline`

### LED-023 平台币 unassigned 与 baseline 都不足时 unassigned 为负并写 InvShortfall

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-022 的构造,但初始 BNB 为 0.003、外来买入 0.002(BNB `baseline = 0.003`、`unassigned = 0.002`)
- 步骤:
  1. `apply_fill(F(Buy, BTC/S1, 1.0, 60 000, 0.01, BNB, G1, 0), t = 10 s)`
- 期望:
  - BNB `baseline` 0,`unassigned` −0.005,`total` −0.005;恰好一条 `Note{InvShortfall}`,P1
- 实现:`crates/hft-ledger/tests/fill.rs::led_023_discount_fee_shortfall`

### LED-024 平台币扣费从不动其他 group 的库存

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)` 但 `A1/S1` BNB 初始化为 0;加入 `G4`;入账 `F(Buy, BNB/S1, 1.0, 600, 0, USDT, G4, 0)`(`inv(BNB, G4) = 1.0`,BNB `total` 1.0、`unassigned` 0、`baseline` 0);`S1` `fee_asset_rule = ReceivedAssetUnlessDiscount{BNB}`
- 步骤:
  1. `apply_fill(F(Buy, BTC/S1, 1.0, 60 000, 0.01, BNB, G1, 0), t = 10 s)`
- 期望:
  - `inv(BNB, G4)` 仍为 1.0;BNB `unassigned` −0.01;`Note{InvShortfall}` 一条;基础资产不变量 `total = baseline + unassigned + Σ inv` 成立(0.99 = 0 + (−0.01) + 1.0)
- 实现:`crates/hft-ledger/tests/fill.rs::led_024_fee_never_touches_other_group_inv`

### LED-025 现货卖出超过该 group 库存时差额扣 unassigned 并写 InvShortfall

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`fee_asset_rule = Quote`;`inv(BTC, G1) = 0.5`、BTC `unassigned = 0.2`(`total` 0.7)
- 步骤:
  1. `apply_fill(F(Sell, BTC/S1, 0.6, 60 000, 0, USDT, G1, 0), t = 10 s)`
- 期望:
  - `inv(BTC, G1)` 0,BTC `unassigned` 0.1,`total` 0.1;`Note{InvShortfall}` 一条,P1
- 实现:`crates/hft-ledger/tests/fill.rs::led_025_spot_sell_beyond_group_inv`

### LED-030 同向加仓累加数量与成本

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`P2` `fee_asset_rule = Quote`
- 步骤:
  1. `apply_fill(F(Buy, BTC/P2, 1.0, 60 000, 0, USDT, G1, 1), t = 10 s)`
  2. `apply_fill(F(Buy, BTC/P2, 1.0, 62 000, 0, USDT, G1, 1), t = 11 s)`
- 期望:
  - `pos(A1, BTC/P2)`:`qty` 2.0,`cost` 122 000(raw 122 000 × 10^12),`realized_quote` 0,`last_fill_ens` = 11 s
  - `PerpPosView.entry_px` = 61 000
- 实现:`crates/hft-ledger/tests/position.rs::led_030_add_same_side`

### LED-031 部分反向减仓按比例移出成本并实现盈亏

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:LED-030 之后的状态
- 步骤:
  1. `apply_fill(F(Sell, BTC/P2, 0.5, 63 000, 0, USDT, G1, 1), t = 12 s)`
- 期望:
  - `removed` = 30 500;`realized_quote` +1 000;`cost` 91 500;`qty` 1.5
  - `G1.pnl.price_realized` +1 000;`A1/P2` USDT `total` +1 000;`MarginState.wallet` +1 000
- 实现:`crates/hft-ledger/tests/position.rs::led_031_partial_reduce`

### LED-032 移出成本向零截断且全平时恰好清零

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`P2` `fee_asset_rule = Quote`
- 步骤:
  1. `apply_fill(F(Buy, BTC/P2, 0.01, 60 000.0001, 0, USDT, G1, 1))`,再 `F(Buy, BTC/P2, 0.02, 60 000.0000, …)`
  2. `apply_fill(F(Sell, BTC/P2, 0.01, 60 000.0000, …))`
  3. `apply_fill(F(Sell, BTC/P2, 0.02, 60 000.0000, …))`
- 期望:
  - 步骤 1 后 `cost` raw = 1 800 000 001 000 000
  - 步骤 2 后 `cost` raw = 1 200 000 000 666 667,`realized_quote` raw = −333 333(移出 600 000 000 333 333,`TowardZero`)
  - 步骤 3 后 `qty` 0,`cost` raw = 0,`realized_quote` raw = −1 000 000(= −0.0001 × 0.01 USDT);`G1.pnl.price_realized` raw = 0(两次换算 −0.333333、−0.666667 raw 各向零截断)
- 实现:`crates/hft-ledger/tests/position.rs::led_032_removed_cost_truncation`

### LED-033 穿越零拆成平仓加反向开仓

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;已入账 `F(Buy, BTC/P2, 1.0, 60 000, 0, USDT, G1, 1)`
- 步骤:
  1. `apply_fill(F(Sell, BTC/P2, 1.5, 61 000, 0, USDT, G1, 1), t = 10 s)`
- 期望:
  - `realized_quote` +1 000;`qty` −0.5;`cost` 30 500;`PerpPosView.entry_px` 61 000
- 实现:`crates/hft-ledger/tests/position.rs::led_033_cross_zero`

### LED-034 全平后数量与成本为零

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-033 的初始仓位
- 步骤:
  1. `apply_fill(F(Sell, BTC/P2, 1.0, 59 000, 0, USDT, G1, 1), t = 10 s)`
- 期望:
  - `qty` 0,`cost` 0,`realized_quote` −1 000,`last_fill_ens` = 10 s;`margin(A1, P2).mark_oldest_ens == i64::MAX`(无永续持仓)
- 实现:`crates/hft-ledger/tests/position.rs::led_034_full_close`

### LED-035 空头减仓的已实现符号

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`
- 步骤:
  1. `apply_fill(F(Sell, BTC/P2, 1.0, 60 000, 0, USDT, G1, 1))`
  2. `apply_fill(F(Buy, BTC/P2, 0.4, 58 000, 0, USDT, G1, 1))`
- 期望:
  - 步骤 1 后 `qty` −1.0,`cost` 60 000(存绝对值)
  - 步骤 2 后 `removed` 24 000,`realized_quote` +800,`cost` 36 000,`qty` −0.6
- 实现:`crates/hft-ledger/tests/position.rs::led_035_short_reduce`

### LED-036 现货买入成本按净量计,基础资产手续费只记 fee_cex

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`S1` `fee_asset_rule = ReceivedAsset`
- 步骤:
  1. `apply_fill(F(Buy, BTC/S1, 1.0, 60 000, 0.001, BTC, G1, 0), t = 10 s)`
- 期望:
  - `pos(A1, BTC/S1)`:`qty` 0.999,`cost` 59 940(净量 0.999 × 60 000);`G1.pnl.fee_cex` −60(费用不再计入成本);与"成交不带费用 + 晚到 `FeeOnly`"路径(LED-014、LED-015)的 `cost` 相同
- 实现:`crates/hft-ledger/tests/position.rs::led_036_spot_cost_net`

### LED-037 已实现与手续费同时进累计、当日、当前轮次与账户当日

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:LED-030 之后的状态
- 步骤:
  1. `apply_fill(F(Sell, BTC/P2, 0.5, 63 000, 31.5, USDT, G1, 1), t = 12 s)`
- 期望:
  - `G1.pnl`、`G1.day`、`G1.round.pnl` 三处各:`price_realized` +1 000,`fee_cex` −31.5
  - `account(A1).day`:`price_realized` +1 000,`fee_cex` −31.5;`A1/P2` USDT `total` +968.5
- 实现:`crates/hft-ledger/tests/position.rs::led_037_realized_into_three_books`

### LED-038 现货成交移动计价现金与该 group 的库存

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`S1` `fee_asset_rule = Quote`
- 步骤:
  1. `apply_fill(F(Buy, BTC/S1, 0.1, 60 000, 6, USDT, G1, 0), t = 10 s)`
  2. `apply_fill(F(Sell, BTC/S1, 0.1, 61 000, 6.1, USDT, G1, 0), t = 11 s)`
- 期望:
  - 步骤 1 后:`A1/S1` USDT `total` 93 994;BTC `total` 0.1;`inv(BTC, G1)` 0.1
  - 步骤 2 后:USDT `total` 100 087.9;BTC `total` 0;`inv(BTC, G1)` 0;`G1.pnl.price_realized` +100,`fee_cex` −12.1
- 实现:`crates/hft-ledger/tests/position.rs::led_038_spot_cash_and_inventory`

### LED-039 交易所余额报告在初始化之后不改持仓与库存

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)、[余额初始化、校验与基线](README.md#余额初始化校验与基线)
- 级别:unit
- 里程碑:M2
- 前置:LED-038 步骤 1 之后的状态
- 步骤:
  1. `on_balance(BalanceReport { A1, S1, BTC, free: 5, locked: 0 }, quiet = true, t = 20 s)`
- 期望:
  - 返回 `Drift { asset: BTC, diff: +4.9 }`;`pos(A1, BTC/S1).qty`、BTC `total`、`inv(BTC, G1)` 与调用前逐值相同;`rep_free` = 5,`rep_ens` = 20 s
- 实现:`crates/hft-ledger/tests/position.rs::led_039_reports_never_change_position`

### LED-040 首条余额报告初始化:计价资产为现金、其他资产进 unassigned

- 规则:[余额初始化、校验与基线](README.md#余额初始化校验与基线)
- 级别:unit
- 里程碑:M2
- 前置:`rd_lf()`、`cfg_lf()`;`on_run_start(0 s)`;无任何余额报告
- 步骤:
  1. `on_balance(A1/S1 USDT free 90 000 locked 10 000, quiet = false, 1 s)`
  2. `on_balance(A1/S1 BNB free 10 locked 0, quiet = false, 1 s)`
- 期望:
  - 两次都返回 `Initialized`,各写一条 `Note{BalanceInit}`
  - USDT `total` 100 000(free + locked),`unassigned` 0,`bflags` 不含 `UNINIT`;BNB `total` 10,`unassigned` 10,`baseline` 0
- 实现:`crates/hft-ledger/tests/balance.rs::led_040_first_report_initializes`

### LED-041 账户全部 venue 初始化之前 Equity.valid 为假

- 规则:[余额初始化、校验与基线](README.md#余额初始化校验与基线)、[权益数据](README.md#权益数据portfolioview-的字段语义)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-040 步骤 1–2 之后;`on_ref_px` 已按 `boot_a1` 给出换算率
- 步骤:
  1. 读 `equity(A1).valid`
  2. `on_balance(A1/P2 USDT free 50 000, quiet = false, 2 s)` 后再读
- 期望:
  - 步骤 1 为 `false`(`P2` 未初始化);步骤 2 为 `true`
- 实现:`crates/hft-ledger/tests/balance.rs::led_041_equity_valid_after_all_venues`

### LED-042 quiet 为假时只更新报告值不比较

- 规则:[余额初始化、校验与基线](README.md#余额初始化校验与基线)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`
- 步骤:
  1. `on_balance(A1/S1 USDT free 70 000 locked 0, quiet = false, 5 s)`
- 期望:
  - 返回 `Reported`;USDT `rep_free` 70 000、`rep_locked` 0、`rep_ens` 5 s;`total` 仍为 100 000
- 实现:`crates/hft-ledger/tests/balance.rs::led_042_not_quiet_no_check`

### LED-043 漂移判定以资产 dust 为界

- 规则:[余额初始化、校验与基线](README.md#余额初始化校验与基线)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;USDT dust 0.01;`A1/S1` USDT `total` 100 000;每行 `on_balance(A1/S1 USDT free = 报告值, locked 0, quiet = true, 5 s)`
- 实现:`crates/hft-ledger/tests/balance.rs::led_043_drift_dust_boundary`

| 行 | 报告值 | 期望 |
|---|---|---|
| LED-043.1 | 100 000.009999 | `Ok`(差 0.009999 < dust) |
| LED-043.2 | 100 000.01 | `Drift { asset: USDT, diff: +0.01 }` |
| LED-043.3 | 100 000.010001 | `Drift { asset: USDT, diff: +0.010001 }` |
| LED-043.4 | 99 999.99 | `Drift { asset: USDT, diff: −0.01 }` |
| LED-043.5 | 任一行之后 | `total` 仍为 100 000(本 crate 不改账) |

### LED-044 SetBalanceBaseline 把无 group 的非计价资产登记为基线

- 规则:[余额初始化、校验与基线](README.md#余额初始化校验与基线)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`(BNB `unassigned` 10);`on_balance(A1/S1 BNB free 10.5, quiet = false, 5 s)`
- 步骤:
  1. `set_balance_baseline(A1, S1, cmd_seq = 77, &mut out)`
- 期望:
  - 返回 `Set { n_assets: 1, notional: 6 300 }`;`out == [BNB]`
  - BNB:`baseline` 10.5,`unassigned` 0,`total` 10.5,`bflags` 含 `NON_STRATEGY`;`account(A1).day.adjust` +300(0.5 × 600)
  - 一条 `Note{BaselineSet}`,`v[0] = 1`,`v[1]` = 6 300 USDT(raw 6 300 000 000)
- 实现:`crates/hft-ledger/tests/balance.rs::led_044_set_baseline`

### LED-045 基线不动计价资产与属于 group 的资产

- 规则:[余额初始化、校验与基线](README.md#余额初始化校验与基线)
- 级别:unit
- 里程碑:M2
- 前置:LED-044 的前置,但 `A1/S1` BTC 初始化为 0.05(`unassigned = 0.05`;外来买入有唯一候选 `G1`,构造不出未归属量);入账 `F(Buy, BTC/S1, 0.1, 60 000, 0, USDT, G1, 0)`(`inv(BTC, G1) = 0.1`);报告 USDT 100 500、BTC 0.2
- 步骤:
  1. `set_balance_baseline(A1, S1, 77, &mut out)`
- 期望:
  - `out` 只含 BNB;USDT `total` 100 000 不变;BTC `baseline` 0、`unassigned` 0.05、`inv(BTC, G1)` 0.1 不变
- 实现:`crates/hft-ledger/tests/balance.rs::led_045_baseline_skips_quote_and_group_assets`

### LED-046 尚无余额报告的 venue 不能登记基线

- 规则:[余额初始化、校验与基线](README.md#余额初始化校验与基线)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`A2/S3` 从未收到 `BalanceReport`
- 步骤:
  1. `set_balance_baseline(A2, S3, 78, &mut out)`
- 期望:
  - 返回 `NoBalanceView`;`out` 为空;账本逐字节不变(三节快照与调用前相同);无 `Note{BaselineSet}`
- 实现:`crates/hft-ledger/tests/balance.rs::led_046_baseline_needs_balance_view`

### LED-050 同一 ExternalFill(同 inst、同 trade_id)第二次入账返回 None 且账本不变

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`P2` `fee_asset_rule = Quote`;`G1` 永续腿已入账 `F(Sell, BTC/P2, 1.0, 60 000, 0, USDT, G1, 1)`;`E1 = ExternalFill { A1, P2, cause: Liquidation, side: Buy, inst: BTC/P2, qty: 0.2, px: 61 000, fee: 0, fee_asset: USDT, trade_id: 900 }`
- 步骤:
  1. `apply_external(&E1, auto_recon = false, 10 s)`
  2. 记下三节快照字节,再 `apply_external(&E1, false, 11 s)`
- 期望:
  - 步骤 1 返回 `Some`;步骤 2 返回 `None`,三节快照字节与步骤 2 之前逐字节相同,步骤 2 没有写任何 `Note`
- 实现:`crates/hft-ledger/tests/external.rs::led_050_dup_trade_id`

### LED-051 去重键的取法与作用域

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:table
- 里程碑:M2
- 前置:同 LED-050,步骤 1 已入账 `E1`;每行从该状态起独立执行
- 实现:`crates/hft-ledger/tests/external.rs::led_051_dedup_key_forms`

| 行 | 输入 | 期望 |
|---|---|---|
| LED-051.1 | 同 `E1` 但 `inst = ETH/P2`(`trade_id` 900) | `Some`(键含 inst) |
| LED-051.2 | `trade_id = 0`、`exch_order_id = 55`、`fill_seq = 0.2`(raw 20 000 000);再喂一次同样的 | 第一次 `Some`,第二次 `None` |
| LED-051.3 | `trade_id = 0`、`exch_order_id = 55`、`fill_seq = 0.3` | `Some`(同一订单不同 `fill_seq`) |
| LED-051.4 | 与 `E1` 完全相同但 `account = A2` | `Some`(按 (account, venue) 分别去重) |

### LED-052 外来成交去重环满时淘汰最旧的键

- 规则:[外部成交](README.md#外部成交externalfill)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`cfg_lf()` 但 `ext_dedup_cap = 4`;键 k1…k5 为 `A1/P2` 上 `trade_id` 1…5 的 `ExternalFill{Liquidation, Buy, BTC/P2, 0.001}`,按行顺序依次执行
- 实现:`crates/hft-ledger/tests/external.rs::led_052_dedup_ring_capacity`

| 行 | 动作 | 期望 |
|---|---|---|
| LED-052.1 | 入账 k1…k4 后重放 k1 | `None`(环未满 4 条,k1 仍在) |
| LED-052.2 | 入账 k5 | `Some`;环内为 k2…k5 |
| LED-052.3 | 重放 k1 | `Some`(k1 已被淘汰,重复入账) |
| LED-052.4 | 重放 k5 | `None` |

### LED-053 去重环进快照,恢复后重放同一笔仍被挡住

- 规则:[外部成交](README.md#外部成交externalfill)、[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:LED-050 步骤 1 之后
- 步骤:
  1. `snapshot` → `restore`
  2. `apply_external(&E1, false, 20 s)`
- 期望:
  - 节 13 含一条 `ExtSeen { kind: TradeId(1), a: 900, b: 0 }`,行头 `account = A1`、`venue = P2`、`id = BTC/P2`;步骤 2 返回 `None`
- 实现:`crates/hft-ledger/tests/external.rs::led_053_ext_seen_survives_restore`

### LED-054 auto_external_fill 按引擎减交易所的差构造合成成交

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:table
- 里程碑:M2
- 前置:`a = A1`、`v = P2`、`inst = BTC/P2`、`mark_px = 60 500`、`report_id = 4242`
- 实现:`crates/hft-ledger/tests/external.rs::led_054_auto_external_fill_fields`

| 行 | `engine_minus_exch` | 期望 |
|---|---|---|
| LED-054.1 | −2.0(引擎空 10、交易所空 8) | `side = Buy`,`qty = 2.0` |
| LED-054.2 | +4.0 | `side = Sell`,`qty = 4.0` |
| LED-054.3 | +10.0(多 10 被全量 ADL 到 0) | `side = Sell`,`qty = 10.0` |
| LED-054.4 | 每行的其余字段 | `cause = Unknown`,`px = 60 500`,`fee = 0`,`fee_asset = ASSET_NONE`,`trade_id = 0`,`exch_order_id = 0`,`fill_seq = 4242`,`ex_ts_ns = 0`,`rx_tsc = 0`;同一输入调用两次结果逐字节相同 |

### LED-055 同一报告组的 AUTO_EXTERNAL 本体第二次入账返回 None

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`G1` 永续腿多 10.0(`F(Buy, BTC/P2, 10.0, 60 000, 0, USDT, G1, 1)`)
- 步骤:
  1. `apply_external(&auto_external_fill(A1, P2, BTC/P2, +4.0, 60 500, 4242), true, 10 s)`
  2. 同一调用再执行一次
- 期望:
  - 步骤 1 返回 `Some`,`InstPos.qty` 6.0;步骤 2 返回 `None`,`InstPos.qty` 仍为 6.0(键 `(BTC/P2, 0, 4242)`)
- 实现:`crates/hft-ledger/tests/external.rs::led_055_auto_body_once_per_report`

### LED-056 AUTO_RECON 本体入账后记 auto_held

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-055
- 步骤:
  1. LED-055 步骤 1
- 期望:
  - `auto_held(A1, BTC/P2, Sell)` = 4.0,`auto_held(A1, BTC/P2, Buy)` = 0;一条 `Note{ExternalFill}`,`v[0]` 的 `aflags` 含 `AUTO_RECON`(2)
- 实现:`crates/hft-ledger/tests/external.rs::led_056_auto_body_sets_auto_held`

### LED-057 补推的同向真实 ADL 被 auto_held 全部抵扣

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-056 之后;`E2 = ExternalFill { A1, P2, cause: Adl, side: Sell, inst: BTC/P2, qty: 4.0, px: 60 400, fee: 0, trade_id: 7001 }`
- 步骤:
  1. `apply_external(&E2, false, 20 s)`
- 期望:
  - 返回 `None`;`InstPos.qty` 仍为 6.0,`G1` 的 PnL、现金与钱包不变;`auto_held(Sell)` = 0
  - 一条 `Note{LedgerAutoHeldOffset}`,P0,`v = [5 | 1 << 8, 21, Sell 的码, 4.0, 4.0, 0]`(`P2` = 5)(qty 与 off 为 raw);`E2` 的去重键已记入
- 实现:`crates/hft-ledger/tests/external.rs::led_057_adl_fully_offset`

### LED-058 补推量大于 auto_held 时只入账超出部分,手续费按比例

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-056 之后;`E3` = 同 `E2` 但 `qty = 6.0`、`fee = 0.6`、`fee_asset = USDT`、`trade_id = 7002`
- 步骤:
  1. `apply_external(&E3, false, 20 s)`
- 期望:
  - 返回 `Some(ExternalApplied { group: G1, leg_ix: 1, net_qty: −2.0, none_qty: 0, … })`;`InstPos.qty` 6.0 → 4.0;入账手续费 0.2(`0.6 × 2 / 6`)
  - `auto_held(Sell)` = 0;`Note{LedgerAutoHeldOffset}` 的 `v[3]` = 6.0、`v[4]` = 4.0、`v[5]` = 0
- 实现:`crates/hft-ledger/tests/external.rs::led_058_adl_partially_offset`

### LED-059 不满足抵扣条件的外来成交照常全额入账

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:table
- 里程碑:M2
- 前置:LED-056 之后(`auto_held(A1, BTC/P2, Sell) = 4.0`);`G3` 的 `ETH/P2` 腿多 2.0;每行从该状态起独立执行,`trade_id` 各不相同
- 实现:`crates/hft-ledger/tests/external.rs::led_059_offset_conditions`

| 行 | 输入 | 期望 |
|---|---|---|
| LED-059.1 | `ExternalFill{Adl, Buy, BTC/P2, 1.0}` | 入账 1.0;`auto_held(Sell)` 4.0;无抵扣注记 |
| LED-059.2 | `ExternalFill{Manual, Sell, BTC/P2, 4.0}` | 入账 4.0;`auto_held` 4.0 |
| LED-059.3 | `ExternalFill{Delivery, Sell, BTC/P2, 4.0}` | 入账 4.0;`auto_held` 4.0 |
| LED-059.4 | `ExternalFill{Adl, Sell, ETH/P2, 1.0}` | 入账 1.0;`auto_held(BTC/P2, Sell)` 4.0 |
| LED-059.5 | `ExternalFill{Liquidation, Sell, BTC/P2, 1.0}` | 返回 `None`;`auto_held` 3.0;一条抵扣注记 |
| LED-059.6 | `ExternalFill{Unknown, Sell, BTC/P2, 1.0}`,`auto_recon = false` | 返回 `None`;`auto_held` 3.0 |
| LED-059.7 | `auto_external_fill(…, +3.0, …, report_id = 4243)`,`auto_recon = true` | 入账 3.0;`auto_held` 7.0(本体不抵扣) |

### LED-060 被全部抵扣的成交重放时不再写抵扣注记

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-057 之后
- 步骤:
  1. `apply_external(&E2, false, 30 s)`
- 期望:
  - 返回 `None`;本次调用不写任何 `Note`;`auto_held(Sell)` 仍为 0
- 实现:`crates/hft-ledger/tests/external.rs::led_060_offset_dedup_on_replay`

### LED-061 auto_held 进快照,恢复后照常抵扣

- 规则:[外部成交](README.md#外部成交externalfill)、[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:LED-056 之后
- 步骤:
  1. `snapshot` → `restore`
  2. `apply_external(&E2, false, 20 s)`
- 期望:
  - 节 13 含 `AutoHeld { buy: 0, sell: 4.0 }`,行头 `account = A1`、`venue = P2`、`id = BTC/P2`;`auto_held` 为 0 的 (account, venue, inst) 不写行
  - 步骤 2 返回 `None` 并写 `Note{LedgerAutoHeldOffset}`
- 实现:`crates/hft-ledger/tests/external.rs::led_061_auto_held_survives_restore`

### LED-062 恰好一个非 Shadow 候选时归属该 group 与腿

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-050 的前置(`G1` 永续腿 −1.0)
- 步骤:
  1. `apply_external(ExternalFill{Liquidation, Buy, BTC/P2, 0.4, px 61 000, fee 0.2 USDT, trade_id 901}, false, 10 s)`
- 期望:
  - `ExternalApplied { group: G1, leg_ix: 1, net_qty: +0.4, none_qty: 0, notional: 24 400 }`
  - `G1` 永续腿净量 −0.6;`InstPos.qty` −0.6;`G1.pnl.external` = −(0.4 × 1 000) − 0.2 = −400.2
- 实现:`crates/hft-ledger/tests/external.rs::led_062_single_candidate`

### LED-063 没有候选时记 GROUP_NONE

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`A2` 已初始化(`S3` USDC 10 000、`P2` USDT 10 000),`A2` 没有任何 group
- 实现:`crates/hft-ledger/tests/external.rs::led_063_no_candidate`

| 行 | 输入 | 期望 |
|---|---|---|
| LED-063.1 | `ExternalFill{A2, S3, Manual, Buy, BTCUSDC/S3, 0.1, px 60 000, fee 0}` | `group = GROUP_NONE`,`net_qty = 0`,`none_qty = +0.1`;`A2/S3` BTC `unassigned` +0.1;USDC `total` −6 000 |
| LED-063.2 | `ExternalFill{A2, P2, Liquidation, Sell, ETH/P2, 0.5, px 3 000}` | `none_qty = −0.5`;`pos(A2, ETH/P2).qty` −0.5(账户级无 group 持仓) |

### LED-064 同一 inst 被非 Shadow 与 Shadow cell 引用时归属非 Shadow 的

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-050 的前置;另加 `G5` = 腿 0 `BTC/P2`,`set_cell_shadow(A1, G5, true)`
- 步骤:
  1. `apply_external(ExternalFill{Liquidation, Buy, BTC/P2, 0.4, trade_id 902}, false, 10 s)`
- 期望:
  - `ExternalApplied.group == G1`;`G5` 的腿净量与 PnL 不变
- 实现:`crates/hft-ledger/tests/external.rs::led_064_shadow_cell_not_candidate`

### LED-065 唯一候选变为 Shadow 后归属 GROUP_NONE

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-064 的前置,再 `set_cell_shadow(A1, G1, true)`
- 步骤:
  1. `apply_external(ExternalFill{Liquidation, Buy, BTC/P2, 0.4, trade_id 903}, false, 10 s)`
- 期望:
  - `group == GROUP_NONE`,`none_qty = +0.4`;`G1`、`G5` 腿净量不变;账户级无 group 持仓 +0.4
- 实现:`crates/hft-ledger/tests/external.rs::led_065_all_candidates_shadow`

### LED-066 两个非 Shadow 候选时记 GROUP_NONE 并置 AMBIGUOUS

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-064 的前置,但 `G5` 非 Shadow(人为构造的违规状态)
- 步骤:
  1. `apply_external(ExternalFill{Liquidation, Buy, BTC/P2, 0.4, trade_id 904}, false, 10 s)`
- 期望:
  - `group == GROUP_NONE`;`Note{ExternalFill}` 的 `v[0] >> 24 & 0xFF` 含 `AMBIGUOUS`(1)
- 实现:`crates/hft-ledger/tests/external.rs::led_066_two_candidates_ambiguous`

### LED-067 归属以该腿减到 0 为限,超出部分记 GROUP_NONE

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-050 的前置(`G1` 永续腿 −1.0);`set_cell_shadow(A1, G1, true)` → 入账 `ExternalFill{Manual, Sell, BTC/P2, 0.004, trade_id 905}`(记 GROUP_NONE)→ `set_cell_shadow(A1, G1, false)`;此时 `InstPos.qty` −1.004,账户级无 group 持仓 −0.004
- 步骤:
  1. `apply_external(ExternalFill{Liquidation, Buy, BTC/P2, 1.004, px 61 000, trade_id 906}, false, 10 s)`
- 期望:
  - `ExternalApplied { group: G1, leg_ix: 1, net_qty: +1.0, none_qty: +0.004 }`;`G1` 永续腿 0(不越过零);`InstPos.qty` 0;账户级无 group 持仓 0
- 实现:`crates/hft-ledger/tests/external.rs::led_067_attribute_until_leg_zero`

### LED-068 使腿增大的外来成交整笔归属该 group

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-050 的前置(`G1` 永续腿 −1.0)
- 步骤:
  1. `apply_external(ExternalFill{Manual, Sell, BTC/P2, 0.3, trade_id 907}, false, 10 s)`
- 期望:
  - `net_qty = −0.3`,`none_qty = 0`;`G1` 永续腿 −1.3
- 实现:`crates/hft-ledger/tests/external.rs::led_068_leg_increase_fully_attributed`

### LED-069 AUTO_RECON 本体同样以腿减到 0 为限

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-067 的前置
- 步骤:
  1. `apply_external(&auto_external_fill(A1, P2, BTC/P2, −1.004, 61 000, 4244), true, 10 s)`
- 期望:
  - `net_qty = +1.0`,`none_qty = +0.004`;注记 `aflags` 含 `AUTO_RECON`;`auto_held(Buy)` = 1.004
- 实现:`crates/hft-ledger/tests/external.rs::led_069_auto_body_leg_limit`

### LED-070 外来成交的已实现与手续费记入 external 列

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-050 的前置(`G1` 永续腿 −1.0,成本 60 000)
- 步骤:
  1. `apply_external(ExternalFill{Liquidation, Buy, BTC/P2, 1.0, px 61 000, fee 30.5, fee_asset USDT, trade_id 908}, false, 10 s)`
- 期望:
  - `G1.pnl.external` −1 030.5;`G1.pnl.price_realized` 0;`G1.pnl.fee_cex` 0;`A1/P2` USDT `total` −1 030.5;`MarginState.wallet` −1 030.5;`InstPos.qty` 0
- 实现:`crates/hft-ledger/tests/external.rs::led_070_external_column`

### LED-071 ExternalFill 注记的级别按 cause

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:table
- 里程碑:M2
- 前置:LED-050 的前置;每行一笔 0.001 的外来成交,`trade_id` 各不相同
- 实现:`crates/hft-ledger/tests/external.rs::led_071_note_level_by_cause`

| 行 | `cause` | 期望 `Note{ExternalFill}.level` |
|---|---|---|
| LED-071.1 | `Liquidation` | P0 |
| LED-071.2 | `Adl` | P0 |
| LED-071.3 | `Manual` | P1 |
| LED-071.4 | `Delivery` | P0 |
| LED-071.5 | `Unknown` | P0 |

### LED-072 ExternalFill 注记的 v[] 布局与 v[0] 打包往返

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:table
- 里程碑:M2
- 前置:LED-050 的前置;`exch_order_id = 77`
- 实现:`crates/hft-ledger/tests/external.rs::led_072_note_v_layout`

| 行 | 输入 | 期望 |
|---|---|---|
| LED-072.1 | `ExternalFill{Adl, Sell, BTC/P2, 0.002, px 60 500, eoid 77}`,`auto_recon = false` | `v[0] = 0x0002_0105`(venue 5 = `P2`、account 1、cause 2、aflags 0);`v[1] = 21`;`v[2]` = Sell 的码;`v[3]` = 200 000;`v[4]` = 605 000 000;`v[5]` = 77;`v[6]` = G1;`v[7]` = 1 |
| LED-072.2 | `auto_external_fill(A1, P2, BTC/P2, +0.002, 60 500, 4245)`,`auto_recon = true` | `v[0] = 0x0205_0105`(cause 5、aflags `AUTO_RECON` 2);`v[5] = 0` |
| LED-072.3 | 对 `v[0]` 解包 | `(v0 & 0xFF, v0 >> 8 & 0xFF, v0 >> 16 & 0xFF, v0 >> 24 & 0xFF)` 等于输入的 `(venue, account, cause, aflags)` |

### LED-073 cause = Unknown 之后被认领的槽成交只算一次

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`S1` `fee_asset_rule = Quote`
- 步骤:
  1. `apply_external(ExternalFill{Unknown, Buy, BTC/S1, 0.4, px 60 000, eoid 88, trade_id 0, fill_seq 0.4}, false, 10 s)`
  2. `apply_fill(F(Buy, BTC/S1, 0.6, 60 000, 0, USDT, G1, 0) 且 flags = EXT_DEDUP, 11 s)`(OMS 已扣除外来入账的 0.4)
- 期望:
  - `pos(A1, BTC/S1).qty` = 1.0;`inv(BTC, G1)` = 1.0;USDT `total` −60 000;步骤 1 的 0.4 记在 `G1` 的 `external` 列(开仓,已实现 0),步骤 2 记在 `price_realized` 侧
- 实现:`crates/hft-ledger/tests/external.rs::led_073_ext_dedup_counted_once`

### LED-074 SetParams 移除 group 后候选表随之更新

- 规则:[外部成交](README.md#外部成交externalfill)、[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`(`G1` 无持仓)
- 步骤:
  1. `on_groups_changed(不含 G1 的 GroupTable)`
  2. `apply_external(ExternalFill{Manual, Buy, BTC/P2, 0.1, trade_id 909}, false, 10 s)`
- 期望:
  - `group == GROUP_NONE`,`none_qty = +0.1`
- 实现:`crates/hft-ledger/tests/external.rs::led_074_groups_changed_rebuilds_candidates`

### LED-075 冷读者不按普通 ExternalFill 注记再入账

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;冷读者测试驱动 `JournalLedger`(按"外部成交"第 6、7 步只调用本 crate 的公开入口);一个事件 = Input `ExternalFill{Liquidation, Buy, BTC/P2, 0.4, trade_id 910}` + 热进程写出的 `Note{ExternalFill}`(`aflags = 0`)+ `EVENT_END`
- 步骤:
  1. 冷读者消费该事件
- 期望:
  - `InstPos.qty` 变化恰为 +0.4(只按 Input 入账一次);三节快照与热进程对同一 Input 调 `apply_external` 的结果逐字节相同
- 实现:`crates/hft-ledger/tests/cold_reader.rs::led_075_plain_note_not_booked`

### LED-076 冷读者按 AUTO_RECON 注记调用同一合成函数

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-055 的初始仓位,两份相同的账本 H(热)与 C(冷)
- 步骤:
  1. H:`apply_external(&auto_external_fill(A1, P2, BTC/P2, +4.0, 60 500, 4242), true, 10 s)`,记下写出的 `Note{ExternalFill}`
  2. C:消费事件 [Input `ReconcileReport{report_id = 4242}`、步骤 1 的注记、`EVENT_END`]
- 期望:
  - C 由注记的 `venue`、`account`、`inst`、`side`、`qty`、`px` 与 Input 的 `report_id` 得到的 `ExternalFill` 与 H 的逐字节相同;H、C 三节快照逐字节相同,`auto_held(Sell)` 都是 4.0
- 实现:`crates/hft-ledger/tests/cold_reader.rs::led_076_auto_recon_note_consumed`

### LED-077 external_from_oms_note 按 hft-oms 的注记布局解出外来成交

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:table
- 里程碑:M2
- 前置:`Note{source = Oms, code = OmsForeignFill}`,`Note.inst = BTC/P2`,`v` 按 hft-oms"回报匹配与成交增量"第 8 条的布局填写:`venue = P2`、`account = A1`、`side = Buy`、`fee_asset = USDT`、`qty = 0.3`、`px = 60 100`、`fee = 0.09`、`exch_order_id = 99`、`cum_filled = 0.5`
- 实现:`crates/hft-ledger/tests/cold_reader.rs::led_077_external_from_oms_note`

| 行 | 输入差异 | 期望 |
|---|---|---|
| LED-077.1 | `path = 2`(StaleGen),`trade_id = 0`,`fill_seq` = 0.5 的 raw | `Some(ExternalFill { venue: P2, account: A1, side: Buy, cause: Unknown, inst: BTC/P2, qty: 0.3, px: 60 100, fee: 0.09, fee_asset: USDT, exch_order_id: 99, trade_id: 0, fill_seq: 50 000 000, ex_ts_ns: 0, rx_tsc: 0 })` |
| LED-077.2 | `path = 1`(防御路径) | 同上(`path` 不进 `ExternalFill`) |
| LED-077.3 | `trade_id = 12 345`,`fill_seq = 0` | `trade_id = 12 345`,`fill_seq = 0` |
| LED-077.4 | `code = OmsStaleGenFill` | `None` |
| LED-077.5 | `source = Ledger`,`code = ExternalFill` | `None` |

### LED-078 OMS 注记构造的外来成交与先前入账的同一笔去重

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;已入账 Input `ExternalFill{Unknown, Buy, BTC/P2, 0.3, eoid 99, trade_id 0, fill_seq 0.5}`
- 步骤:
  1. `apply_external(&external_from_oms_note(&LED-077.1 的注记).unwrap(), false, 10 s)`
- 期望:
  - 返回 `None`(键 `(BTC/P2, 99, 0.5)` 已在环内);账本不变
- 实现:`crates/hft-ledger/tests/cold_reader.rs::led_078_oms_note_dedup`

### LED-079 同一订单两笔累计量不同的防御路径成交都入账

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;两条 `Note{OmsForeignFill}`:`eoid 99`、`trade_id 0`,`qty` 0.2 / 0.3,`cum_filled` 0.2 / 0.5
- 步骤:
  1. 依次 `apply_external(external_from_oms_note(n), false, …)`
- 期望:
  - 两次都返回 `Some`;`InstPos(A1, BTC/P2).qty` 变化 +0.5;两条 `ExtSeen` 的 `b` 分别为 0.2 与 0.5 的 raw
- 实现:`crates/hft-ledger/tests/cold_reader.rs::led_079_oms_note_two_increments`

### LED-080 OMS 防御路径的外来成交:冷读者账本与热进程逐值相同

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:testkit 假交易所(hft-sim 撮合器),测试策略在 `A1` 上运行 PairOnFill 开平仓;冷读者 = 本 crate 的消费驱动读同一份 journal
- 故障:每个种子随机选 1–3 个时刻向 gw 注入 `StaleGen` 带成交的 `OrderEvent`(`exch_order_id ≠ 0`,`trade_id = 0` 与 `> 0` 各半)与 1 条 `SLOT_UNRESOLVED` 带成交的 `OrderEvent`;其中一半与之前 gw 已报的 `ExternalFill{Unknown}` 同一订单
- 种子:PR 200(提交派生)+ 回归集;每夜日期派生
- 断言:每个事件边界后,热进程 ledger 与冷读者 ledger 的节 1、13、22 快照字节相等;每条 `Note{OmsForeignFill}` 在冷读者中恰好产生一次 `apply_external` 调用;INV-1、INV-16
- 实现:待实现

### LED-081 AUTO_EXTERNAL:冷读者账本与 auto_held 与热进程逐值相同

- 规则:[外部成交](README.md#外部成交externalfill)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:同 LED-080;`ReconActor` 开启,`ExternalFill` 的"用户流缺口"形态
- 故障:用户流断开期间对 `A1` 永续仓位做部分或全量 ADL(不推送),恢复后一半种子由交易所补推同一笔真实 ADL
- 种子:PR 200 + 回归集;每夜日期派生
- 断言:合成成交入账的那个事件之后,热进程与冷读者的节 1、13、22 字节相等(含 `AutoHeld` 行);补推的 ADL 在两边都被抵扣、都写 `Note{LedgerAutoHeldOffset}`;INV-1、INV-16
- 实现:待实现

资金费用例另用夹具 `fund_a1(kind)`:`boot_a1(0 s)`;`P2` 的 `funding_settle = kind`;入账 `F(Sell, BTC/P2, 1.0, 60 000, 0, USDT, G1, 1)`(1 s);`on_mark(BTC/P2, 60 000, 1 s)`。`T1 = D + 8 h`,`T2 = D + 16 h`。推送写作 `Push(inst, amount, settle_ts)`,即 `FundingSettle { A1, P2, inst, amount, synth: 0, … }`。

### LED-090 PerSymbolPush 推送计入 group 资金费与保证金钱包

- 规则:[资金费](README.md#资金费)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(PerSymbolPush)`
- 步骤:
  1. `apply_funding(Push(BTC/P2, +6.0, T1), t = 10 s)`
- 期望:
  - 返回 `Some(PnlDelta { account: A1, group: G1, d.funding: +6.0, 其余 0 })`
  - `G1.pnl.funding` +6、`G1.day.funding` +6、`account(A1).day.funding` +6;`A1/P2` USDT `total` +6、`MarginState.wallet` +6
  - `FundHist` 新增 `{ inst: BTC/P2, group: G1, hflags: PUSHED, settle_ts_ns: T1, booked: 6.0, corrected: 0 }`
- 实现:`crates/hft-ledger/tests/funding.rs::led_090_per_symbol_push_booked`

### LED-091 PerSymbolPush 按 (inst, 归一化 settle_ts) 去重

- 规则:[资金费](README.md#资金费)、[资金费时刻归一化](README.md#资金费时刻归一化)
- 级别:table
- 里程碑:M2
- 前置:LED-090 之后;每行从该状态独立执行
- 实现:`crates/hft-ledger/tests/funding.rs::led_091_per_symbol_push_dedup`

| 行 | 输入 | 期望 |
|---|---|---|
| LED-091.1 | `Push(BTC/P2, +6.0, T1)` | `None`;钱包、`G1.pnl.funding` 不变 |
| LED-091.2 | `Push(BTC/P2, +6.0, T1 + 2 s)` | `None`(归一化到 `T1`) |
| LED-091.3 | `Push(BTC/P2, +5.0, T2)` | `Some`;`FundHist` 第二条 `settle_ts_ns = T2` |
| LED-091.4 | `Push(ETH/P2, +1.0, T1)` | `Some`(不同 inst) |

### LED-092 RateOnly 合成额计入 group 资金费与钱包

- 规则:[资金费](README.md#资金费)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(RateOnly)`
- 步骤:
  1. `s = synth_funding(A1, BTC/P2, mark 60 000, rate 0.0001, settle_ts_ns = T1 + 0.3 s)`
  2. `apply_funding(&s, t = 10 s)`
- 期望:
  - 步骤 1:`Some(FundingSettle { inst: BTC/P2, amount: +6.0, rate: 0.0001, pos_qty: −1.0, settle_ts_ns: T1, synth: 1 })`
  - 步骤 2:`G1.pnl.funding` +6;`A1/P2` USDT `total` 与 `wallet` +6;`FundHist` 记录 `booked` 6.0、`hflags` 不含 `PUSHED`
- 实现:`crates/hft-ledger/tests/funding.rs::led_092_rate_only_synth_booked`

### LED-093 合成资金费只取整一次:收入向下、支出向上

- 规则:[资金费](README.md#资金费)
- 级别:table
- 里程碑:M2
- 前置:`fund_a1(RateOnly)` 但 `BTC/P2` 持仓按行设定;`mark = 60 000.0001`(raw 600 000 001),`rate = 0.000123457`(raw 123 457)
- 实现:`crates/hft-ledger/tests/funding.rs::led_093_synth_rounding`

| 行 | 持仓 | 期望 `amount` |
|---|---|---|
| LED-093.1 | −0.001 | +0.007407(raw 7 407;精确值 0.0074074200…,`Floor`) |
| LED-093.2 | +0.001 | −0.007408(raw −7 408;按绝对值 `Ceil`) |

### LED-094 持仓为零时不合成

- 规则:[资金费](README.md#资金费)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`P2` `funding_settle = RateOnly`;`BTC/P2` 持仓按行设定;`mark = 60 000`,`rate = 0.0001`
- 实现:`crates/hft-ledger/tests/funding.rs::led_094_synth_zero_position`

| 行 | 持仓 | 期望 |
|---|---|---|
| LED-094.1 | 0 | `None` |
| LED-094.2 | −0.00000001(1 raw) | `Some`,`amount` raw 0(6e−8 USDT 向下取整) |
| LED-094.3 | +0.00000001 | `Some`,`amount` raw −1(按绝对值向上) |

### LED-095 BalanceOnlyPush 的合成额只计 PnL 归属不动钱包

- 规则:[资金费](README.md#资金费)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(BalanceOnlyPush)`
- 步骤:
  1. `apply_funding(&synth_funding(A1, BTC/P2, 60 000, 0.0001, T1).unwrap(), 10 s)`
- 期望:
  - `G1.pnl.funding` +6;`A1/P2` USDT `total` 与 `wallet` 不变;`account(A1).funding_residual` −6;待配对表一项 `{T1, synth_sum 6.0, pushed 0, state: SynthOnly}`
- 实现:`crates/hft-ledger/tests/funding.rs::led_095_balance_only_synth_no_wallet`

### LED-096 BalanceOnlyPush 的推送按实额入钱包并与合成配对

- 规则:[资金费](README.md#资金费)
- 级别:unit
- 里程碑:M2
- 前置:LED-095 之后
- 步骤:
  1. `apply_funding_push(Push(INST_NONE, +6.04, T1 + 1.5 s), 11 s)`
- 期望:
  - 返回 `Booked { check: Ok }`;`wallet` 与 USDT `total` +6.04;`FundHist` 中 `inst = INST_NONE`、`settle_ts_ns = T1` 的记录 `booked` 6.04;`funding_residual` +0.04;待配对表中 `T1` 项被移除
- 实现:`crates/hft-ledger/tests/funding.rs::led_096_balance_only_push_pairs`

### LED-097 推送与合成两种到达顺序的结果相同

- 规则:[资金费](README.md#资金费)
- 级别:table
- 里程碑:M2
- 前置:`fund_a1(BalanceOnlyPush)`;合成额 6.00(`synth_funding(…, 60 000, 0.0001, T1)`),推送 `Push(INST_NONE, +6.04, T1)`
- 实现:`crates/hft-ledger/tests/funding.rs::led_097_push_synth_order_independent`

| 行 | 顺序 | 期望 |
|---|---|---|
| LED-097.1 | 合成 → 推送 | 推送返回 `Booked { check: Ok }`;`funding_residual` +0.04;`wallet` +6.04;`G1.pnl.funding` +6.00 |
| LED-097.2 | 推送 → 合成 | 推送返回 `Booked { check: Ok }`(尚未配对);合成后 `funding_residual` +0.04、`wallet` +6.04、`G1.pnl.funding` +6.00;无 `Note{FundingMismatch}` |
| LED-097.3 | 两行的三节快照 | 逐字节相同 |

### LED-098 残差超出容差时报 Mismatch

- 规则:[资金费](README.md#资金费)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`fund_a1(BalanceOnlyPush)`;`funding_tol_abs = 0.05`,`funding_tol_rel = 0.005`;每行先入账合成额 `S`(构造的 `FundingSettle{synth = 1, amount = S}`),再推送 `Push(INST_NONE, P, T1)`(第 5 行顺序相反)
- 实现:`crates/hft-ledger/tests/funding.rs::led_098_mismatch_tolerance`

| 行 | `S` / `P` | 期望 |
|---|---|---|
| LED-098.1 | 6.00 / 6.05 | `Booked { check: Ok }`(残差 0.05 = 容差 max(0.05, 0.03025)) |
| LED-098.2 | 6.00 / 6.050001 | `Booked { check: Mismatch { residual: 0.050001 } }` |
| LED-098.3 | 19.9995 / 20.10 | `Ok`(残差 0.1005 = 0.005 × 20.10) |
| LED-098.4 | 19.999499 / 20.10 | `Mismatch { residual: 0.100501 }` |
| LED-098.5 | 先推送 6.10、后合成 6.00 | 推送返回 `Ok`;`apply_funding`(合成)时本 crate 写一条 `Note{FundingMismatch}`,P1 |

### LED-099 同一推送重放返回 Duplicate

- 规则:[资金费](README.md#资金费)
- 级别:unit
- 里程碑:M2
- 前置:LED-096 之后
- 步骤:
  1. `apply_funding_push(Push(INST_NONE, +6.04, T1), 12 s)`
- 期望:
  - 返回 `Duplicate`;`wallet`、USDT `total`、`INST_NONE` 记录的 `booked`(6.04)与 `funding_residual` 不变
- 实现:`crates/hft-ledger/tests/funding.rs::led_099_push_replay_duplicate`

### LED-100 同一结算时刻金额不同的推送都入账,已入钱包额是其和

- 规则:[资金费](README.md#资金费)
- 级别:unit
- 里程碑:M2
- 前置:LED-096 之后
- 步骤:
  1. `apply_funding_push(Push(INST_NONE, +0.5, T1), 12 s)`
- 期望:
  - 返回 `Booked`;`INST_NONE`、`T1` 记录的 `booked` = 6.54;`wallet` 再 +0.5
- 实现:`crates/hft-ledger/tests/funding.rs::led_100_push_same_ts_other_amount`

### LED-101 推送去重表按 funding_push_seen_cap 淘汰最旧项

- 规则:[资金费](README.md#资金费)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`fund_a1(BalanceOnlyPush)`;`funding_push_seen_cap = 2`;推送 p1 = `(T1, 1.0)`、p2 = `(T1, 2.0)`、p3 = `(T1, 3.0)`;按行顺序执行
- 实现:`crates/hft-ledger/tests/funding.rs::led_101_push_seen_capacity`

| 行 | 动作 | 期望 |
|---|---|---|
| LED-101.1 | 入账 p1、p2,重放 p1 | 重放返回 `Duplicate`(2 条未超容量) |
| LED-101.2 | 入账 p3 | `Booked`;去重表为 p2、p3 |
| LED-101.3 | 重放 p1 | `Booked`(p1 已淘汰,重复入账;`booked` 再 +1.0) |

### LED-102 待配对表满 16 项时淘汰结算时刻最早的一项

- 规则:[资金费](README.md#资金费)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(BalanceOnlyPush)`;依次入账 17 笔合成额,结算时刻 `D + k × 8 h`(k = 1…17),无推送
- 步骤:
  1. 读待配对表
  2. `apply_funding_push(Push(INST_NONE, +6.0, D + 8 h), …)`
- 期望:
  - 步骤 1:16 项,`settle_ts` 为 k = 2…17;k = 1 项不在表中,其合成额仍计入 `funding_residual`
  - 步骤 2:返回 `Booked { check: Ok }`;`wallet` +6.0;表仍为 16 项(新的 PushOnly 项是最早的一项,随即淘汰)
- 实现:`crates/hft-ledger/tests/funding.rs::led_102_pairing_table_capacity`

### LED-103 没有持有 group 的资金费只记账户级

- 规则:[资金费](README.md#资金费)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`A2` 已初始化;`A2` 无 group,`pos(A2, ETH/P2).qty = −0.5`(外来成交);`P2` `funding_settle = PerSymbolPush`
- 步骤:
  1. `apply_funding(FundingSettle { A2, P2, inst: ETH/P2, amount: +1.5, settle_ts_ns: T1, synth: 0 }, 10 s)`
- 期望:
  - 返回 `PnlDelta { account: A2, group: GROUP_NONE, d.funding: +1.5 }`;`account(A2).day.funding` +1.5;`A2/P2` 钱包 +1.5;`FundHist` 记录 `group = GROUP_NONE`
- 实现:`crates/hft-ledger/tests/funding.rs::led_103_funding_account_level`

### LED-104 推送去重记录进快照,恢复后重放仍是 Duplicate

- 规则:[资金费](README.md#资金费)、[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:LED-096 之后
- 步骤:
  1. `snapshot` → `restore`
  2. `apply_funding_push(Push(INST_NONE, +6.04, T1), 20 s)`
- 期望:
  - 节 13 含 `FundSeen { settle_ts_ns: T1, amount: 6.04 }` 与 `FundHist`(`inst = INST_NONE`)行;步骤 2 返回 `Duplicate`
- 实现:`crates/hft-ledger/tests/funding.rs::led_104_push_seen_survives_restore`

### LED-110 funding_grid 默认取 refdata 的周期与锚

- 规则:[资金费时刻归一化](README.md#资金费时刻归一化)
- 级别:unit
- 里程碑:M2
- 前置:`rd_lf()`、`cfg_lf()`;新建的 `Ledger`
- 步骤:
  1. `funding_grid(P2, BTC/P2)`、`funding_grid(P2, INST_NONE)`
- 期望:
  - 两者都是 `(28 800, 0)`
- 实现:`crates/hft-ledger/tests/funding_norm.rs::led_110_grid_defaults_from_refdata`

### LED-111 实际周期改为 4 h 后按 4 h 网格归一化

- 规则:[资金费时刻归一化](README.md#资金费时刻归一化)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(PerSymbolPush)`
- 步骤:
  1. `on_funding_interval(BTC/P2, 14 400)`
  2. `apply_funding(Push(BTC/P2, +3.0, D + 4 h + 0.3 s), 10 s)`
- 期望:
  - `funding_grid(P2, BTC/P2) == (14 400, 0)`;`FundHist` 记录 `settle_ts_ns = D + 4 h`(不并入 `D` 或 `D + 8 h`)
- 实现:`crates/hft-ledger/tests/funding_norm.rs::led_111_interval_change_regrids`

### LED-112 INST_NONE 取该 venue 永续 inst 中最短的周期

- 规则:[资金费时刻归一化](README.md#资金费时刻归一化)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(BalanceOnlyPush)`
- 步骤:
  1. `on_funding_interval(ETH/P2, 14 400)`(`BTC/P2` 仍为 28 800)
  2. `apply_funding_push(Push(INST_NONE, +1.0, D + 4 h + 1.5 s), 10 s)`
- 期望:
  - `funding_grid(P2, INST_NONE) == (14 400, 0)`;`INST_NONE` 记录的 `settle_ts_ns = D + 4 h`
- 实现:`crates/hft-ledger/tests/funding_norm.rs::led_112_inst_none_finest_grid`

### LED-113 推送晚于结算边界 2 s 仍与合成记录配对

- 规则:[资金费时刻归一化](README.md#资金费时刻归一化)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(BalanceOnlyPush)`;已入账合成额 6.00(`settle_ts = T1`)
- 步骤:
  1. `apply_funding_push(Push(INST_NONE, +6.00, T1 + 2 s), 10 s)`
- 期望:
  - `Booked { check: Ok }`;`funding_residual` 0;待配对表为空
- 实现:`crates/hft-ledger/tests/funding_norm.rs::led_113_late_push_pairs`

### LED-114 交易所流水时刻与推送差 1 s 时找到同一条记录

- 规则:[资金费时刻归一化](README.md#资金费时刻归一化)、[资金费对账校正](README.md#资金费对账校正)
- 级别:unit
- 里程碑:M2
- 前置:LED-090 之后(`FundHist` 一条,`T1`)
- 步骤:
  1. `reconcile_funding(FundingReconItem { A1, P2, BTC/P2, settle_ts_ns: T1 − 1 s, exch_amount: 6.0, engine_amount: 6.0, report_id: 1 }, now_ns = T1 + 1 h, 20 s)`
- 期望:
  - 返回 `Matched`;`FundHist` 仍只有一条记录
- 实现:`crates/hft-ledger/tests/funding_norm.rs::led_114_recon_item_matches_same_record`

### LED-115 归一化在入口内只取一次网格,已归一化的值再归一化不变

- 规则:[资金费时刻归一化](README.md#资金费时刻归一化)
- 级别:prop
- 里程碑:M2
- 性质:对任意 `ts`,经 `apply_funding`、`apply_funding_push`、`reconcile_funding`、`synth_funding` 任一入口写入 `FundHist`/`FundSeen`/`FundNoHist` 的 `settle_ts_ns` 等于 `funding_settle_norm_ns(ts, funding_grid(..))`;把这个值再交给任一入口,写入的值不变
- 生成:`ts` ∈ [D − 30 天, D + 30 天] 均匀;`interval_s` ∈ {3 600, 14 400, 28 800};`anchor_s` ∈ {0, 1 800}
- 参考:直接调用 hft-types `funding_settle_norm_ns` 的结果
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-ledger/tests/prop_funding.rs::led_115_normalization_idempotent`

### LED-116 结算周期表不进快照,恢复后按节 10 重放得到与连续运行相同的网格

- 规则:[资金费时刻归一化](README.md#资金费时刻归一化)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(PerSymbolPush)`;`on_funding_interval(BTC/P2, 14 400)`
- 步骤:
  1. `snapshot` → `restore`,读 `funding_grid(P2, BTC/P2)`
  2. `on_funding_interval(BTC/P2, 14 400)`(sequencer 按节 10 调用)后再读
- 期望:
  - 步骤 1 为 `(28 800, 0)`(refdata 默认);步骤 2 为 `(14 400, 0)`,与连续运行一致;节 13 中没有周期表的行
- 实现:`crates/hft-ledger/tests/funding_norm.rs::led_116_grid_rebuilt_after_restore`

对账校正用例写 `R(inst, settle, exch, engine)` 表示 `FundingReconItem { A1, P2, inst, settle_ts_ns: settle, exch_amount: exch, engine_amount: engine, report_id: 9 }`;参数 `funding_recon_lag_ms = 600 000`、`funding_recon_window_ms = 172 800 000`、USDT dust 0.01(`cfg_lf()` 与 `rd_lf()`),`hist_from_ns(A1, P2) = D`(`boot_a1`)。

### LED-120 结算后不足 funding_recon_lag_ms 的条目 TooRecent

- 规则:[资金费对账校正](README.md#资金费对账校正)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:LED-090 之后(`T1` 已入账 6.0);每行 `reconcile_funding(R(BTC/P2, T1, 9.0, 6.0), now_ns, 30 s)`,从同一状态独立执行
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_120_too_recent_boundary`

| 行 | `now_ns` | 期望 |
|---|---|---|
| LED-120.1 | `T1 + 599 s`(结算后 9 分 59 秒) | `TooRecent`;账本不变 |
| LED-120.2 | `T1 + 600 s − 1 ns` | `TooRecent` |
| LED-120.3 | `T1 + 600 s` | `Corrected`(diff 3.0) |
| LED-120.4 | `T1 + 600 s + 1 ns` | `Corrected` |

### LED-121 早于比对窗口的条目 OutsideWindow

- 规则:[资金费对账校正](README.md#资金费对账校正)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:同 LED-120
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_121_outside_window_boundary`

| 行 | `now_ns` | 期望 |
|---|---|---|
| LED-121.1 | `T1 + 48 h − 1 ns` | `Corrected` |
| LED-121.2 | `T1 + 48 h` | `Corrected` |
| LED-121.3 | `T1 + 48 h + 1 ns` | `OutsideWindow`;账本不变 |

### LED-122 差额小于结算资产 dust 时 Matched

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:table
- 里程碑:M2
- 前置:同 LED-120;`now_ns = T1 + 1 h`
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_122_matched_dust_boundary`

| 行 | `exch_amount` | 期望 |
|---|---|---|
| LED-122.1 | 6.009999 | `Matched` |
| LED-122.2 | 6.01 | `Corrected`,`adj.qty = 0.01` |
| LED-122.3 | 5.990001 | `Matched` |

### LED-123 PerSymbolPush 的差额校正钱包与 group 资金费

- 规则:[资金费对账校正](README.md#资金费对账校正)、[人工干预的采纳](README.md#人工干预的采纳adoptexchange)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-120
- 步骤:
  1. `reconcile_funding(R(BTC/P2, T1, 9.0, 6.0), T1 + 1 h, 30 s)`
- 期望:
  - 返回 `Corrected { adj: AdjustDelta { kind: Funding(3), px_src: 0, account: A1, venue: P2, inst: BTC/P2, asset: USDT, group: G1, qty: +3.0, px: 0, report_id: 9, cmd_seq: 0, settle_ts_ns: T1 }, pnl: PnlDelta { A1, G1, d.funding: +3.0 } }`
  - USDT `total` 与 `wallet` +3;`G1.pnl.funding` +3;`FundHist` 记录 `corrected` = 3.0
  - `Note{FundingCorrected}`,P1,`v = [5 | 1 << 8, 21, T1, 9.0, 6.0, 3.0, G1, 0]`(`P2` = 5)
  - `pos(A1, BTC/P2)`、库存与 `FillApplied`/`NetAdjust` 均无变化或输出
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_123_per_symbol_corrected`

### LED-124 RateOnly 的差额同样校正钱包与 group

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:unit
- 里程碑:M2
- 前置:LED-092 之后(合成额 6.0 已入账)
- 步骤:
  1. `reconcile_funding(R(BTC/P2, T1, 6.2, 6.0), T1 + 1 h, 30 s)`
- 期望:
  - `Corrected`,`adj.qty = +0.2`;`wallet` +0.2;`G1.pnl.funding` +0.2
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_124_rate_only_corrected`

### LED-125 BalanceOnlyPush 归属条目只把差额从残差移到 group

已删除:BalanceOnlyPush 所只有钱包条目,inst ≠ INST_NONE 的条目改为 NotApplicable,由 LED-270 替代。

### LED-126 BalanceOnlyPush 推送丢失时钱包条目按流水补足并新建记录

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:unit
- 里程碑:M2
- 前置:LED-095 之后(只有合成额,无推送;无 `INST_NONE` 记录)
- 步骤:
  1. `reconcile_funding(R(INST_NONE, T1, 6.10, 0), T1 + 1 h, 30 s)`
- 期望:
  - `Corrected`,`adj.inst = INST_NONE`、`adj.group = GROUP_NONE`、`adj.qty = +6.10`;`wallet` 与 USDT `total` +6.10;`funding_residual` −6.00 → +0.10
  - 新建 `INST_NONE` 记录 `booked` 0、`corrected` 6.10;`Note{FundingCorrected}` 的 `flags` = `NEW_RECORD`(2)
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_126_wallet_item_push_lost`

### LED-127 BalanceOnlyPush 推送被重复入账时钱包条目按流水扣回

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:unit
- 里程碑:M2
- 前置:LED-095 之后;推送 `(T1, 6.10)` 与 `(T1, 6.09)` 都已入账(`INST_NONE` 记录 `booked` 12.19,构造的误判)
- 步骤:
  1. `reconcile_funding(R(INST_NONE, T1, 6.10, 12.19), T1 + 1 h, 30 s)`
- 期望:
  - `Corrected`,`adj.qty = −6.09`;`wallet` −6.09;`funding_residual` −6.09
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_127_wallet_item_double_push`

### LED-128 同一条目报告两次只校正一次

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:unit
- 里程碑:M2
- 前置:LED-123 之后
- 步骤:
  1. 再次 `reconcile_funding(R(BTC/P2, T1, 9.0, 9.0), T1 + 2 h, 40 s)`
- 期望:
  - 返回 `Matched`(`booked + corrected` = 9.0);`wallet` 与 `G1.pnl.funding` 不变;不写注记
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_128_second_report_matched`

### LED-129 推送丢失(环中无记录)时全额校正并新建记录

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:table
- 里程碑:M2
- 前置:`fund_a1(PerSymbolPush)`,`T1` 没有任何推送入账;`now_ns = T1 + 1 h`
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_129_missing_record_full_correction`

| 行 | 条件 | 期望 |
|---|---|---|
| LED-129.1 | `G1` 非 Shadow,`R(BTC/P2, T1, 6.0, 0)` | `Corrected`,`adj.group = G1`,`qty` +6.0;新记录 `booked` 0、`corrected` 6.0;注记 `flags` = `NEW_RECORD` |
| LED-129.2 | `set_cell_shadow(A1, G1, true)` 之后同一条目 | `adj.group = GROUP_NONE`;`account(A1).day.funding` +6.0,`G1.pnl.funding` 不变 |

### LED-130 早于 hist_from_ns 的条目挂起、注记、不动账

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(PerSymbolPush)`(`hist_from_ns = D`)
- 步骤:
  1. `reconcile_funding(R(BTC/P2, D − 8 h, 5.0, 2.0), now_ns = D + 1 h, 30 s)`
- 期望:
  - 返回 `NoHistoryHeld`;三节快照中 `Cash`、`Margin`、`Group`、`Account` 行与调用前相同
  - 节 13 多一条 `FundNoHist { settle_ts_ns: D − 8 h, exch_amount: 5.0, engine_amount: 2.0, report_id: 9 }`;`Note{FundingCorrected}` 一条,P1,`flags` = `NO_HISTORY`(1)
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_130_no_history_held`

### LED-131 窗口内重复报告只更新挂起项的金额

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:unit
- 里程碑:M2
- 前置:LED-130 之后
- 步骤:
  1. `reconcile_funding(R(BTC/P2, D − 8 h, 5.5, 2.0) 且 report_id 10, D + 2 h, 40 s)`
- 期望:
  - `NoHistoryHeld`;`FundNoHist` 仍只有一行,`exch_amount` 5.5、`report_id` 10;账本其余不变
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_131_no_history_updated`

### LED-132 挂起项移出窗口后按交易所金额入账一次

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:table
- 里程碑:M2
- 前置:LED-130 之后(挂起 `exch 5.0`、`engine 2.0`);每行 `funding_window_sweep(A1, P2, now_ns, t, &mut out)`
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_132_window_sweep`

| 行 | `now_ns` | 期望 |
|---|---|---|
| LED-132.1 | `D − 8 h + 48 h` | `out` 为空;挂起项保留 |
| LED-132.2 | `D − 8 h + 48 h + 1 ns` | `out = [Corrected { adj.qty: +3.0, … }]`;`wallet` +3.0、`G1.pnl.funding` +3.0;注记 `flags` = `NO_HISTORY | WINDOW_END`(5);`FundNoHist` 行被删除 |
| LED-132.3 | 行 2 之后再调一次 | `out` 为空;账本不变 |
| LED-132.4 | 行 2 之后 `reconcile_funding(R(BTC/P2, D − 8 h, 5.0, 2.0), D + 41 h, …)` | `OutsideWindow` |

### LED-133 reconciler 也无记录时挂起项按全额入账

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(PerSymbolPush)`;`reconcile_funding(R(BTC/P2, D − 8 h, 4.0, 0), D + 1 h, …)` → `NoHistoryHeld`
- 步骤:
  1. `funding_window_sweep(A1, P2, D + 40 h + 1 ns, 50 s, &mut out)`
- 期望:
  - `out` 一项 `Corrected`,`adj.qty = +4.0`
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_133_sweep_engine_zero_full_amount`

### LED-134 挂起表满 64 项时最旧一项立即入账

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:unit
- 里程碑:M2
- 前置:`fund_a1(PerSymbolPush)`;`on_funding_interval(BTC/P2, 1 800)`;64 个早于 `hist_from_ns` 的条目已挂起(`settle_ts = D − k × 30 min`,k = 2…65,各 `exch 1.0`、`engine 0`,全部在比对窗口内)
- 步骤:
  1. `reconcile_funding(R(BTC/P2, D − 30 min, 1.0, 0), D + 1 h, 30 s)`
- 期望:
  - 返回 `Corrected`,对应 `settle_ts = D − 32.5 h` 的那一项(结算时刻最早),`adj.qty = +1.0`,注记 `flags` = 5;新条目已挂起并写一条 `flags = NO_HISTORY` 的注记;挂起表仍为 64 项
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_134_no_hist_table_full`

### LED-135 BalanceOnlyPush 的挂起项按条目类型入账

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:table
- 里程碑:M2
- 前置:`fund_a1(BalanceOnlyPush)`;依次提交 `R(BTC/P2, D − 8 h, 2.0, 0)` 与 `R(INST_NONE, D − 8 h, 2.0, 0)`(前者在第 0 步即 `NotApplicable`,不进挂起表)
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_135_balance_only_sweep_routes`

| 行 | 挂起项 | 期望(`funding_window_sweep(…, D + 40 h + 1 ns, …)` 后) |
|---|---|---|
| LED-135.1 | `inst ≠ INST_NONE` 的条目 | 不在挂起表;`G1.pnl.funding`、`funding_residual` 不变 |
| LED-135.2 | 钱包条目 | `wallet` +2.0,`funding_residual` +2.0 |

### LED-136 逐笔记录环满时淘汰最旧记录并推进水位线

- 规则:[资金费](README.md#资金费)、[资金费对账校正](README.md#资金费对账校正)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`fund_a1(PerSymbolPush)`;`funding_hist_cap = 4`;推送 `Push(BTC/P2, 1.0, D + k × 8 h)`,按行累计入账
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_136_hist_ring_capacity`

| 行 | 动作 | 期望 |
|---|---|---|
| LED-136.1 | k = 1…4 入账 | 环 4 条;`hist_from_ns` 仍为 `D` |
| LED-136.2 | k = 5 入账 | 环为 k = 2…5;`hist_from_ns = D + 8 h + 1 ns` |
| LED-136.3 | `R(BTC/P2, D + 8 h, 1.0, 1.0)`,`now = D + 41 h` | `NoHistoryHeld` |
| LED-136.4 | `R(BTC/P2, D + 16 h, 1.0, 1.0)`,`now = D + 41 h` | `Matched` |

### LED-137 hist_from_ns 在首条余额初始化时取报告的交易所时间

- 规则:[资金费](README.md#资金费)
- 级别:unit
- 里程碑:M2
- 前置:`rd_lf()`、`cfg_lf()`;新建 `Ledger`
- 步骤:
  1. `on_balance(A1/P2 USDT free 50 000, ex_ts_ns = D + 3 h, quiet = false, 1 s)`
  2. `reconcile_funding(R(BTC/P2, D + 0 h, 1.0, 0), D + 4 h, 2 s)`
- 期望:
  - 节 13 `FundMark { hist_from_ns: D + 3 h }`;步骤 2 返回 `NoHistoryHeld`(结算早于初始化,已含在初始化余额里)
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_137_hist_from_at_init`

### LED-138 从不含 FundMark 的快照恢复后以首次对账时刻为水位线

- 规则:[资金费](README.md#资金费)、[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:LED-090 的状态写出快照,删去节 13 中 `(A1, P2)` 的 `FundMark` 与其 `FundHist` 行后 `restore`
- 步骤:
  1. `reconcile_funding(R(BTC/P2, T1, 6.0, 6.0), now_ns = T1 + 2 h, 30 s)`
- 期望:
  - 返回 `NoHistoryHeld`;此后 `FundMark.hist_from_ns = T1 + 2 h`
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_138_hist_from_after_restore_without_mark`

### LED-139 挂起项进快照,恢复后窗口结束时照常入账

- 规则:[资金费对账校正](README.md#资金费对账校正)、[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:LED-130 之后
- 步骤:
  1. `snapshot` → `restore`
  2. `funding_window_sweep(A1, P2, D + 40 h + 1 ns, 60 s, &mut out)`
- 期望:
  - `out` 一项 `Corrected`,`adj.qty = +3.0`,与不经快照的 LED-132.2 结果逐字段相同
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_139_no_hist_survives_restore`

保证金用例另用夹具 `mg_a1`:`boot_a1(0 s)` 但 `A1/P2` USDT 为 18 000(`wallet` 18 000);入账 `F(Sell, BTC/P2, 1.0, 60 000, 0, USDT, G1, 1)`(1 s);`on_mark(BTC/P2, 60 000, 1 s)`;`on_margin(MarginReport { A1, P2, margin_balance: 18 000, maint_margin: 300, avail_margin: 6 000, margin_ratio: 0.016666667, items: [(BTC/P2, liq_px 77 600, adl 3)] }, 2 s)`。这就是单个空头、e = 0.30、维持保证金率 0.5% 的例子。

### LED-150 估计强平距离随 mark 变化,维持保证金只放大不缩小

- 规则:[保证金数据](README.md#保证金数据)
- 级别:table
- 里程碑:M2
- 前置:`mg_a1`;每行 `on_mark(BTC/P2, m, 3 s)`,从 `mg_a1` 独立执行;比率按 `HalfEven` 取到 1e−9
- 实现:`crates/hft-ledger/tests/margin.rs::led_150_est_liq_dist`

| 行 | `m`(上涨 y) | 期望 |
|---|---|---|
| LED-150.1 | 60 000(0) | `upnl` 0;`est_margin_balance` 18 000;`est_liq_dist` 0.293532338;`est_ratio` 0.016666667 |
| LED-150.2 | 66 000(0.10) | `upnl` −6 000;`est_margin_balance` 12 000;M = 330;`est_liq_dist` 0.175938489;`est_ratio` 0.0275 |
| LED-150.3 | 54 000(−0.10) | `est_margin_balance` 24 000;M = 300(不缩小);`est_liq_dist` 0.436464088;`est_ratio` 0.0125 |
| LED-150.4 | 77 610(0.2935) | M = 388.05;`est_liq_dist` 0.000025001 |
| LED-150.5 | 78 000(0.30) | `est_margin_balance` 0;`est_liq_dist` 0;`est_ratio` `Rate::MAX` |

### LED-151 按交易所强平价的距离:多空公式、越过与缺失

- 规则:[保证金数据](README.md#保证金数据)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`BTC/P2` 持仓与报告的 `liq_px` 按行设定;`on_mark(BTC/P2, 60 000, 3 s)`
- 实现:`crates/hft-ledger/tests/margin.rs::led_151_rep_liq_dist`

| 行 | 持仓 / `liq_px` | 期望 |
|---|---|---|
| LED-151.1 | 多 1.0 / 51 000 | `rep_liq_dist` 0.15 |
| LED-151.2 | 空 1.0 / 69 000 | 0.15 |
| LED-151.3 | 多 1.0 / 61 000(已越过) | 0 |
| LED-151.4 | 空 1.0 / 59 000(已越过) | 0 |
| LED-151.5 | 空 1.0 / 0(未给) | `Rate::MAX`;`lflags` 含 `LIQ_PX_MISSING` |
| LED-151.6 | `mg_a1`(空 1.0 / 77 600) | 0.293333333 |

### LED-152 多持仓取最小距离,缺 liq_px 的不参与

- 规则:[保证金数据](README.md#保证金数据)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;空 `BTC/P2` 1.0(mark 60 000)、空 `ETH/P2` 10.0(`G3`,mark 3 000)
- 实现:`crates/hft-ledger/tests/margin.rs::led_152_min_over_positions`

| 行 | 报告的 `liq_px`(BTC / ETH) | 期望 |
|---|---|---|
| LED-152.1 | 63 000 / 3 450 | `rep_liq_dist` 0.05;`LIQ_PX_MISSING` 不置位 |
| LED-152.2 | 0 / 3 450 | 0.15;`LIQ_PX_MISSING` 置位 |

### LED-153 没有永续持仓时距离为 Rate::MAX、最旧标记价时刻为 i64::MAX

- 规则:[保证金数据](README.md#保证金数据)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`on_margin(A1/P2 报告,items 为空, 2 s)`
- 步骤:
  1. 读 `margin(A1, P2)`
- 期望:
  - `rep_liq_dist == Rate::MAX`,`est_liq_dist == Rate::MAX`,`mark_oldest_ens == i64::MAX`
- 实现:`crates/hft-ledger/tests/margin.rs::led_153_no_perp_position`

### LED-154 没有保证金报告时不计算估计

- 规则:[保证金数据](README.md#保证金数据)
- 级别:unit
- 里程碑:M2
- 前置:`mg_a1` 但不调 `on_margin`
- 步骤:
  1. `on_mark(BTC/P2, 66 000, 3 s)` 后读 `margin(A1, P2)`
- 期望:
  - `has_report == false`;`est_ratio == 0`;`est_liq_dist == Rate::MAX`;`rep_liq_dist == Rate::MAX` 且 `LIQ_PX_MISSING` 置位;`upnl` −6 000 照常更新
- 实现:`crates/hft-ledger/tests/margin.rs::led_154_no_report_no_estimate`

### LED-155 最旧标记价时刻取各持仓最近标记价时刻的最小值

- 规则:[保证金数据](README.md#保证金数据)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;空 `BTC/P2` 1.0;按行顺序执行
- 实现:`crates/hft-ledger/tests/margin.rs::led_155_mark_oldest`

| 行 | 动作 | 期望 `mark_oldest_ens` |
|---|---|---|
| LED-155.1 | `on_mark(BTC/P2, …, 10 s)` | 10 s |
| LED-155.2 | 入账空 `ETH/P2` 1.0(该 inst 尚无标记价) | 0 |
| LED-155.3 | `on_mark(ETH/P2, …, 12 s)` | 10 s |
| LED-155.4 | `on_mark(BTC/P2, …, 13 s)` | 12 s |

### LED-156 on_mark 只把持有该 inst 的 (account, venue) 写进 touched

- 规则:[保证金数据](README.md#保证金数据)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`A1` 空 `ETH/P2` 1.0;`A2` 无 `ETH/P2` 持仓
- 步骤:
  1. `on_mark(ETH/P2, 3 000, 5 s, &mut touched)`
  2. `on_mark(BTC/P2, 60 000, 6 s, &mut touched2)`(无人持有)
- 期望:
  - `touched == [(A1, P2)]`;`touched2` 为空
- 实现:`crates/hft-ledger/tests/margin.rs::led_156_touched_only_holders`

### LED-157 增量维护的最小距离与全量重算逐值相同

- 规则:[保证金数据](README.md#保证金数据)
- 级别:prop
- 里程碑:M2
- 性质:任意 `on_mark`/`on_margin`/成交序列之后,`rep_liq_dist`、`est_liq_dist`、`est_ratio`、`mark_oldest_ens` 等于按"保证金数据"定义对当前全部持仓的全量重算
- 生成:`rd_lf` 的 2 个永续 inst,持仓由 0.1 的 1–20 倍的成交累积(含回到 0);0–200 次 `on_mark`(价格 ±40%)、0–10 次 `on_margin`(`liq_px` 含 0 与已越过值)、0–50 笔成交,任意交错
- 参考:测试内的全量重算函数(i128,`HalfEven`)
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-ledger/tests/prop_margin.rs::led_157_incremental_equals_full`

### LED-158 早于本次运行起点的换算率与保证金报告视为过期

- 规则:[保证金数据](README.md#保证金数据)、[USDT 统一计价](README.md#usdt-统一计价)
- 级别:unit
- 里程碑:M2
- 前置:`mg_a1`(换算率与报告的 `ens` 都 ≤ 2 s)
- 步骤:
  1. `on_run_start(100 s)`,读 `venue_book(A1, P2).summary.flags` 与 `venue_book(A1, S1).summary.flags`
  2. `on_ref_px(BTC/S1, 59 999.99, 60 000.01, 101 s)`、`on_ref_px(BNB/S1, …, 101 s)`、`on_ref_px(USDC/S1, …, 101 s)`
  3. `on_margin(新报告, 102 s)`
- 期望:
  - 步骤 1:`S1` 含 `RATE_STALE`;`P2` 含 `MARGIN_STALE`;`equity(A1).valid == false`
  - 步骤 2 后 `S1` 不含 `RATE_STALE`;步骤 3 后 `P2` 不含 `MARGIN_STALE`、`equity(A1).valid == true`
- 实现:`crates/hft-ledger/tests/margin.rs::led_158_stale_before_run_start`

### LED-159 restore_marks 后的派生量与连续运行逐值相同

- 规则:[保证金数据](README.md#保证金数据)、[使用本组件](README.md#使用本组件)
- 级别:prop
- 里程碑:M2
- 性质:任意事件序列在任意位置快照,`restore` 后以快照处每 inst 最新标记价与其 engine_ns 调 `restore_marks`,得到的 `MarginState` 派生量(`upnl`、`est_*`、两种强平距离、`mark_oldest_ens`)与持有者 `upnl` 缓存等于连续运行的值;此后继续同一序列,两边每一步的三节快照逐字节相同
- 生成:LED-157 的生成器,另在序列中随机选 1–3 个快照点
- 参考:不经快照的连续运行
- 例数:PR 1 000;每夜 50 000
- 实现:`crates/hft-ledger/tests/prop_margin.rs::led_159_restore_marks_matches_continuous`

### LED-160 restore_marks 不改任何 PnL 累计与当日未实现

- 规则:[保证金数据](README.md#保证金数据)、[未实现与当日 PnL](README.md#未实现与当日-pnl增量维护)
- 级别:unit
- 里程碑:M2
- 前置:`mg_a1` 之后 `on_mark(BTC/P2, 61 000, 5 s)`(`G1.day.unrealized` −1 000)
- 步骤:
  1. `snapshot` → `restore` → `restore_marks(&[(BTC/P2, 61 000, 5 s)])`
- 期望:
  - `G1.day.unrealized` 仍为 −1 000,`account(A1).upnl` −1 000;`G1.pnl`、`G1.day`、`account(A1).day` 逐字段与快照前相同;`restore_marks` 不返回也不写任何 `PnlDelta` 或 `Note`
- 实现:`crates/hft-ledger/tests/margin.rs::led_160_restore_marks_no_delta`

### LED-161 on_margin 保存报告值并记下报告时的永续总名义

- 规则:[保证金数据](README.md#保证金数据)、[权益数据](README.md#权益数据portfolioview-的字段语义)
- 级别:unit
- 里程碑:M2
- 前置:`mg_a1`
- 步骤:
  1. 读 `margin(A1, P2)` 与 `PerpPosView(A1, BTC/P2)`
- 期望:
  - `has_report`;`rep_margin_balance` 18 000、`rep_maint` 300、`rep_avail` 6 000、`rep_ratio` 0.016666667、`rep_ens` 2 s、`rep_perp_notional` 60 000;`items == [(BTC/P2, 77 600, 3)]`
  - `PerpPosView { qty: −1.0, entry_px: 60 000, upnl: 0, liq_px: 77 600, adl_quantile: 3 }`;另一个无条目的 inst 的 `liq_px` 与 `adl_quantile` 为 0
- 实现:`crates/hft-ledger/tests/margin.rs::led_161_on_margin_stores_report`

### LED-170 AdoptExchange 仓位条目按 Adjust 类成交记入

- 规则:[人工干预的采纳](README.md#人工干预的采纳adoptexchange)
- 级别:unit
- 里程碑:M2
- 前置:LED-050 的前置(`G1` 永续腿 −1.0,成本 60 000)
- 步骤:
  1. `apply_adjust(AdjustDelta { A1, P2, kind: Position(1), px_src: Cmd(1), inst: BTC/P2, asset: 0, group: G1, qty: −0.5, px: 61 000, report_id: 31, cmd_seq: 501, settle_ts_ns: 0 }, 10 s)`
- 期望:
  - `pos(A1, BTC/P2)`:`qty` −1.5,`cost` 90 500;`G1` 永续腿 −1.5;返回 `PnlDelta.d` 全 0
  - `Note{Adjusted}`:`v[0] = 1 | 5 << 8`(`P2` = 5),`v[1] = 21`,`v[2] = 31`,`v[3]` = −50 000 000,`v[4]` = 610 000 000,`v[5] = 0`
  - `GroupPosView(A1, G1).flags` 含 `ENTRY_ADOPTED`
- 实现:`crates/hft-ledger/tests/adjust.rs::led_170_adopt_position`

### LED-171 采纳产生的已实现记入 adjust 列

- 规则:[人工干预的采纳](README.md#人工干预的采纳adoptexchange)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-170
- 步骤:
  1. `apply_adjust(AdjustDelta { …, kind: Position, qty: +0.5, px: 59 000, report_id: 32, cmd_seq: 502 }, 10 s)`
- 期望:
  - `qty` −0.5;`G1.pnl.adjust` +500;`G1.pnl.price_realized` 0;`G1.round.pnl.price_realized` 不变;`Note{Adjusted}.v[5]` = 500 USDT
- 实现:`crates/hft-ledger/tests/adjust.rs::led_171_adopt_realized_to_adjust`

### LED-172 余额采纳到 group 时带入场价记成本

- 规则:[人工干预的采纳](README.md#人工干预的采纳adoptexchange)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`
- 步骤:
  1. `apply_adjust(AdjustDelta { A1, S1, kind: Balance(2), px_src: Cmd, inst: BTC/S1, asset: BTC, group: G1, qty: +0.5, px: 60 000, report_id: 33, cmd_seq: 503 }, 10 s)`
- 期望:
  - BTC `total` +0.5;`inv(BTC, G1)` +0.5;`pos(A1, BTC/S1)`:`qty` +0.5,`cost` 30 000;基础资产不变量成立
- 实现:`crates/hft-ledger/tests/adjust.rs::led_172_adopt_balance_to_group`

### LED-173 余额采纳到 GROUP_NONE 时进 unassigned 且不动持仓

- 规则:[人工干预的采纳](README.md#人工干预的采纳adoptexchange)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`
- 步骤:
  1. `apply_adjust(AdjustDelta { A1, S1, kind: Balance, px_src: Bid(3), inst: BTC/S1, asset: BTC, group: GROUP_NONE, qty: +0.5, px: 59 999.99, … }, 10 s)`
- 期望:
  - BTC `total` +0.5、`unassigned` +0.5;任何 group 的 `inv` 不变;`pos(A1, BTC/S1)` 不变
- 实现:`crates/hft-ledger/tests/adjust.rs::led_173_adopt_balance_unassigned`

### LED-174 核销永续腿残余只改归属

- 规则:[人工干预的采纳](README.md#人工干预的采纳adoptexchange)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`G1` 现货腿 +1.0(`F(Buy, BTC/S1, 1.0, 60 000, …)`)、永续腿 −1.004(`F(Sell, BTC/P2, 1.004, 60 000, …)`);`on_mark(BTC/P2, 60 000)`
- 步骤:
  1. `apply_adjust(AdjustDelta { A1, P2, kind: WriteOff(4), px_src: 0, inst: BTC/P2, asset: ASSET_NONE, group: G1, qty: −0.004, px: 0, report_id: 0, cmd_seq: 601 }, 10 s)`
- 期望:
  - `G1` 永续腿 −1.0;`pos(A1, BTC/P2).qty` 仍为 −1.004,`cost` 不变;账户级无 group 持仓 −0.004
  - 各资产 `total`、`wallet`、全部 `PnlBreakdown` 不变;返回 `PnlDelta.d` 全 0
  - `Note{ResidualWrittenOff}`,P1,`v = [1 | 5 << 8, G1, 21, −400 000, 240 USDT(raw 240 000 000,|qty| × mark), 601]`
- 实现:`crates/hft-ledger/tests/adjust.rs::led_174_write_off_perp_leg`

### LED-175 核销现货腿残余从 group 库存移到 unassigned

- 规则:[人工干预的采纳](README.md#人工干预的采纳adoptexchange)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`G1` 现货腿 +1.004、永续腿 −1.0
- 步骤:
  1. `apply_adjust(AdjustDelta { A1, S1, kind: WriteOff, inst: BTC/S1, asset: BTC, group: G1, qty: +0.004, cmd_seq: 602, … }, 10 s)`
- 期望:
  - `inv(BTC, G1)` 1.0;BTC `unassigned` +0.004;BTC `total` 1.004 不变;`pos(A1, BTC/S1)` 不变;基础资产不变量成立
- 实现:`crates/hft-ledger/tests/adjust.rs::led_175_write_off_spot_leg`

### LED-176 核销后各腿低于轮次线时轮次结束

- 规则:[人工干预的采纳](README.md#人工干预的采纳adoptexchange)、[轮次](README.md#轮次round)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`round_dust_notional = 5`;`G1` 现货腿 +0.0001(估值价 60 000,名义 6,轮次已开始),永续腿 0
- 步骤:
  1. `apply_adjust(AdjustDelta { kind: WriteOff, inst: BTC/S1, asset: BTC, group: G1, qty: +0.0001, cmd_seq: 603, … }, 10 s)`
- 期望:
  - 一条 `Note{RoundClosed}`;`G1.held_notional` 0;`G1.round` 回到未开始状态(各腿 `LegAgg` 为 0)
- 实现:`crates/hft-ledger/tests/adjust.rs::led_176_write_off_closes_round`

### LED-177 qty 为 0 的核销不改账只写注记

- 规则:[人工干预的采纳](README.md#人工干预的采纳adoptexchange)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`G1` 两腿 +1.0 / −1.0
- 步骤:
  1. 记下三节快照,`apply_adjust(AdjustDelta { kind: WriteOff, inst: BTC/P2, group: G1, qty: 0, cmd_seq: 604, … }, 10 s)`
- 期望:
  - 三节快照与调用前逐字节相同;一条 `Note{ResidualWrittenOff}`,`v[3] = 0`、`v[4] = 0`
- 实现:`crates/hft-ledger/tests/adjust.rs::led_177_write_off_zero_note_only`

### LED-178 AdjustDelta 布局为 56 B,kind 与 px_src 码值往返

- 规则:[人工干预的采纳](README.md#人工干预的采纳adoptexchange)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-ledger/tests/layout.rs::led_178_adjust_delta_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| LED-178.1 | `size_of::<AdjustDelta>()`、`align_of` | 56、8;字段偏移 `account` 0、`venue` 1、`kind` 2、`px_src` 3、`inst` 4、`asset` 8、`group` 12、`qty` 16、`px` 24、`report_id` 32、`cmd_seq` 40、`settle_ts_ns` 48 |
| LED-178.2 | `kind` Position / Balance / Funding / WriteOff | 1 / 2 / 3 / 4;0 与 5 解码返回错误 |
| LED-178.3 | `px_src` Cmd / Mark / Bid | 1 / 2 / 3;Funding 与 WriteOff 为 0 |

### LED-179 核销与采纳任意交错时基础资产不变量与持仓来源规则保持

- 规则:[人工干预的采纳](README.md#人工干预的采纳adoptexchange)、[权益数据](README.md#权益数据portfolioview-的字段语义)
- 级别:prop
- 里程碑:M2
- 性质:每一步之后 ② 每个 (account, venue, 基础资产) `total = baseline + unassigned + Σ inv`;④ `InstPos.qty` 的变化只来自成交、外来成交与 `Position`/`Balance` 采纳,`WriteOff` 前后 `InstPos` 与各 `total` 不变而 `Σ` 腿净量 + 账户级无 group 持仓 = `InstPos.qty`
- 生成:0–100 步,从 {现货/永续成交(两个 group)、`Position` 采纳(±1.0 内)、`Balance` 采纳(到 group 或 GROUP_NONE)、`WriteOff`(取当时一条腿的 0–100% 裸量,含 0)} 中抽取
- 参考:逐步断言上述等式
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-ledger/tests/prop_adjust.rs::led_179_adjust_writeoff_invariants`

### LED-185 RecordTransfer 只移动现金不计 PnL

- 规则:[划转与模拟账户](README.md#划转与模拟账户)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`
- 步骤:
  1. `apply_transfer(TransferDelta { A1, from_venue: S1, to_venue: P2, asset: USDT, amount: 1 000, kind: Record }, 10 s)`
- 期望:
  - `A1/S1` USDT `total` 99 000;`A1/P2` USDT `total` 51 000、`wallet` 51 000;全部 `PnlBreakdown` 不变;`equity(A1).total` 不变
- 实现:`crates/hft-ledger/tests/transfer.rs::led_185_record_transfer`

### LED-186 SimTransfer 同样只移动现金

- 规则:[划转与模拟账户](README.md#划转与模拟账户)
- 级别:unit
- 里程碑:M2
- 前置:`A9` 已按 `initial_balances` 初始化(S1 USDT 10 000、P2 USDT 10 000)
- 步骤:
  1. `apply_transfer(TransferDelta { A9, S1, P2, USDT, 2 500, kind: Sim }, 10 s)`
- 期望:
  - `A9/S1` 7 500,`A9/P2` 12 500;`A9` 的 PnL 不变;`A1` 的账本逐字节不变
- 实现:`crates/hft-ledger/tests/transfer.rs::led_186_sim_transfer`

### LED-187 sim_reset 清空模拟账户并按初始余额重置现金

- 规则:[划转与模拟账户](README.md#划转与模拟账户)
- 级别:unit
- 里程碑:M2
- 前置:`A9` 有现货与永续持仓、库存、PnL、进行中的轮次、`ExtSeen` 2 条、`FundSeen` 1 条、`auto_held` 1.0;`A1` 有持仓
- 步骤:
  1. `sim_reset(A9, &SimInit { S1 USDT 10 000, P2 USDT 10 000 }, 50 s)`
- 期望:
  - `A9` 全部 `InstPos` 为 0、`inv` 为空、`unassigned` 与 `baseline` 为 0,各 group 的 `pnl`、`day`、`round` 清零,`account(A9).day` 清零;`A9/S1` USDT 10 000、`A9/P2` USDT 10 000;节 13 中 `A9` 的 `ExtSeen`、`FundSeen`、`AutoHeld` 行为 0 条
  - 一条 `Note{SimReset}`;`A1` 的三节快照行逐字节不变
- 实现:`crates/hft-ledger/tests/transfer.rs::led_187_sim_reset`

### LED-188 sim_reset 之后先前被去重的外来成交再次入账

- 规则:[划转与模拟账户](README.md#划转与模拟账户)、[外部成交](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:LED-187 之后;其 `ExtSeen` 中的一个键 k
- 步骤:
  1. `apply_external(键为 k 的 ExternalFill, false, 60 s)`
- 期望:
  - 返回 `Some`
- 实现:`crates/hft-ledger/tests/transfer.rs::led_188_reset_clears_dedup`

### LED-190 标记价变化把未实现差量加到持有者的当日与账户

- 规则:[未实现与当日 PnL](README.md#未实现与当日-pnl增量维护)
- 级别:unit
- 里程碑:M2
- 前置:LED-050 的前置(`G1` 永续空 1.0,成本 60 000);`valuation.perp = Mark`
- 步骤:
  1. `on_mark(BTC/P2, 60 600, 5 s)`
  2. `on_mark(BTC/P2, 60 300, 6 s)`
- 期望:
  - 步骤 1 后 `G1.day.unrealized` −600,`account(A1).upnl` −600
  - 步骤 2 后两者都是 −300
- 实现:`crates/hft-ledger/tests/upnl.rs::led_190_mark_updates_holders`

### LED-191 行情只更新持有该 inst 的 group

- 规则:[未实现与当日 PnL](README.md#未实现与当日-pnl增量维护)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-190;`G3` 无持仓
- 步骤:
  1. `on_mark(BTC/P2, 61 000, 5 s)`;`on_mark(ETH/P2, 3 100, 6 s)`
- 期望:
  - `G3.day.unrealized` 0;`G1.day.unrealized` −1 000(`ETH/P2` 的标记价没有改动它)
- 实现:`crates/hft-ledger/tests/upnl.rs::led_191_only_holders_touched`

### LED-192 日切只重置当日基线,不改累计

- 规则:[未实现与当日 PnL](README.md#未实现与当日-pnl增量维护)
- 级别:unit
- 里程碑:M2
- 前置:LED-190 之后(`G1.day.unrealized` −300);`G1.pnl` 记下
- 步骤:
  1. `on_day_roll(7 s)`
  2. `on_mark(BTC/P2, 60 400, 8 s)`
- 期望:
  - 步骤 1 后 `G1.day` 与 `account(A1).day` 全部字段为 0;`G1.pnl` 逐字段不变
  - 步骤 2 后 `G1.day.unrealized` −100(以日切时的 −300 为基线)
- 实现:`crates/hft-ledger/tests/upnl.rs::led_192_day_roll_baseline`

### LED-193 现货未实现按 valuation.spot 取价

- 规则:[未实现与当日 PnL](README.md#未实现与当日-pnl增量维护)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`G1` 现货腿 1.0、成本 60 000;每行先设 `valuation.spot` 再 `on_ref_px(BTC/S1, 60 100, 60 300, 5 s)`
- 实现:`crates/hft-ledger/tests/upnl.rs::led_193_spot_valuation`

| 行 | `valuation.spot` | 期望 `G1.day.unrealized` |
|---|---|---|
| LED-193.1 | `Bid` | +100 |
| LED-193.2 | `Mid` | +200 |

### LED-194 永续未实现按 valuation.perp 取价

- 规则:[未实现与当日 PnL](README.md#未实现与当日-pnl增量维护)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`on_ref_px(BTC/P2, 60 500, 60 700, 5 s)`、`on_mark(BTC/P2, 60 600, 5 s)`;持仓按行
- 实现:`crates/hft-ledger/tests/upnl.rs::led_194_perp_valuation`

| 行 | 持仓 / `valuation.perp` | 期望 `upnl` |
|---|---|---|
| LED-194.1 | 空 1.0 / `Mark` | −600 |
| LED-194.2 | 空 1.0 / `ExitSide` | −700(按卖一) |
| LED-194.3 | 多 1.0 / `ExitSide` | +500(按买一) |

轮次用例的前置统一为:`boot_a1(0 s)`;`round_dust_notional = 5`;`valuation.spot = Bid`;`on_ref_px(BTC/S1, 50 000, 50 000.01, 0 s)`;成交价 50 000(另注明除外)。

### LED-200 任一腿名义到达 round_dust_notional 时轮次开始

- 规则:[轮次](README.md#轮次round)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:见上;每行是 `G1` 的第一笔成交 `F(Buy, BTC/S1, q, 50 000, 0, USDT, G1, 0)`,`t = 10 s`,`seq = 1 000`
- 实现:`crates/hft-ledger/tests/rounds.rs::led_200_round_start_boundary`

| 行 | `q`(腿名义) | 期望 |
|---|---|---|
| LED-200.1 | 0.00009999(4.9995) | 轮次未开始:`round_id` 0,`open_ens` 0 |
| LED-200.2 | 0.0001(5.0) | `round_id` 1,`open_ens` 10 s,`open_seq` 1 000 |
| LED-200.3 | 0.00010001(5.0005) | `round_id` 1 |

### LED-201 各腿都低于轮次线时轮次结束并写 RoundClosed

- 规则:[轮次](README.md#轮次round)
- 级别:unit
- 里程碑:M2
- 前置:LED-200.2 之后
- 步骤:
  1. `apply_fill(F(Sell, BTC/S1, 0.0001, 51 000, 0, USDT, G1, 0), 70 s)`
- 期望:
  - 一条 `Note{RoundClosed}`:`Note.group = G1`;腿 0 开仓量 0.0001、开仓名义 5.0、平仓量 0.0001、平仓名义 5.1,`PnlBreakdown.price_realized` 0.1,持仓时长 60 s;记录中没有策略 `tag` 字段
  - 之后 `G1.round` 各腿 `LegAgg` 为 0
- 实现:`crates/hft-ledger/tests/rounds.rs::led_201_round_close`

### LED-202 平仓到剩余名义恰为轮次线时轮次不结束

- 规则:[轮次](README.md#轮次round)
- 级别:table
- 里程碑:M2
- 前置:见上;`G1` 已买入 0.0002(名义 10.0,轮次已开始);每行一笔卖出,从该状态独立执行
- 实现:`crates/hft-ledger/tests/rounds.rs::led_202_round_close_boundary`

| 行 | 卖出量(剩余名义) | 期望 |
|---|---|---|
| LED-202.1 | 0.0001(5.0) | 无 `Note{RoundClosed}` |
| LED-202.2 | 0.00010001(4.9995) | 一条 `Note{RoundClosed}` |

### LED-203 轮次均价按当前轮次的配对成交计算

- 规则:[轮次](README.md#轮次round)
- 级别:unit
- 里程碑:M2
- 前置:见上
- 步骤:
  1. 依次入账 `F(Buy, 0.001, 50 000)`、`F(Buy, 0.001, 52 000)`、`F(Sell, 0.001, 53 000)`、`F(Sell, 0.001, 54 000)`(均为 `BTC/S1`、`G1` 腿 0)
- 期望:
  - `Note{RoundClosed}` 腿 0:开仓量 0.002、开仓名义 102(均价 51 000),平仓量 0.002、平仓名义 107(均价 53 500)
- 实现:`crates/hft-ledger/tests/rounds.rs::led_203_round_avg_from_paired_fills`

### LED-210 换算率取参考 inst 最近 Bbo 的向下取整中间价

- 规则:[USDT 统一计价](README.md#usdt-统一计价)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`A2` 已初始化(`S3` USDC 100 000、`P2` USDT 10 000);`S1` `fee_asset_rule = ReceivedAssetUnlessDiscount{BNB}`;每行先在 5 s 喂给定 Bbo
- 实现:`crates/hft-ledger/tests/rates.rs::led_210_rate_is_mid_floor`

| 行 | Bbo(参考 inst) | 期望 |
|---|---|---|
| LED-210.1 | `USDC/S1` 0.9999 / 1.0002 | `rate(USDC) == RateView { bid: 0.9999, ask: 1.0002, ens: 5 s, stale: false }`;`equity(A2)` 中 `S3` 的 USDC 现金计 100 000.0(中间价 1.00005 向下取整到 1.0000) |
| LED-210.2 | `BNB/S1` 599.99 / 600.02 | 入账 `F(Buy, BTC/S1, 0.01, 60 000, 1.0, BNB, G1, 0)` 后 `G1.pnl.fee_cex` −600.005 |

### LED-211 par 资产恒按 1.0 计价

- 规则:[USDT 统一计价](README.md#usdt-统一计价)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:unit
- 里程碑:M2
- 前置:`rd_lf()`、`cfg_lf()`;`on_run_start(1 000 s)`,没有任何 USDT 相关行情
- 步骤:
  1. `rate(USDT)`;入账一笔 6 USDT 的费用
- 期望:
  - `rate(USDT).stale == false`,换算为 1.0;`fee_cex` −6.0
- 实现:`crates/hft-ledger/tests/rates.rs::led_211_par_asset`

### LED-212 换算率超过 rate_max_age_ms 后过期

- 规则:[USDT 统一计价](README.md#usdt-统一计价)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`rate_max_age_ms = 60 000`;`on_ref_px(BNB/S1, 599.99, 600.01, 10 s)` 与其他参考 inst 同时刻更新;每行之后以给定 `t` 调一次 `on_mark(BTC/P2, 60 000, t)` 作为最近一次入口调用
- 实现:`crates/hft-ledger/tests/rates.rs::led_212_rate_age_boundary`

| 行 | `t` | 期望 `rate(BNB).stale` |
|---|---|---|
| LED-212.1 | 70 s − 1 ns | false |
| LED-212.2 | 70 s | false |
| LED-212.3 | 70 s + 1 ns | true;`S1` 的 `summary.flags` 含 `RATE_STALE` |

### LED-213 过期后稳定币按 1.0 回退,其他资产沿用旧值并标过期

- 规则:[USDT 统一计价](README.md#usdt-统一计价)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`A2` 已初始化(`S3` USDC 100 000、`P2` USDT 10 000);`stable_par_fallback = {USDC}`;最后的 Bbo:`USDC/S1` 0.9000 / 0.9002(10 s)、`BNB/S1` 599.99 / 600.01(10 s);当前 `t = 200 s`
- 实现:`crates/hft-ledger/tests/rates.rs::led_213_stale_fallback`

| 行 | 资产 | 期望 |
|---|---|---|
| LED-213.1 | USDC | 换算按 1.0(`equity(A2)` 中 `S3` 的 USDC 现金计 100 000.0,不按 0.9001);`stale == true` |
| LED-213.2 | BNB | 换算仍按 600.00;`stale == true` |
| LED-213.3 | 账户 | `equity(A1).valid == false` |

### LED-214 早于本次运行起点的行情即使未超龄也视为过期

- 规则:[USDT 统一计价](README.md#usdt-统一计价)
- 级别:unit
- 里程碑:M2
- 前置:`rd_lf()`、`cfg_lf()`;`on_ref_px(BNB/S1, 599.99, 600.01, 95 s)`
- 步骤:
  1. `on_run_start(100 s)`,读 `rate(BNB)`(最近入口 `t = 100 s`,年龄 5 s)
- 期望:
  - `stale == true`
- 实现:`crates/hft-ledger/tests/rates.rs::led_214_stale_before_run_start`

### LED-215 已实现按事件发生时的换算率入账,之后不重估

- 规则:[USDT 统一计价](README.md#usdt-统一计价)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`A2` 已初始化(`S3` USDC 100 000);`A2` 的实例有 `G7` = 腿 0 `BTCUSDC/S3`;`S3` `fee_asset_rule = Quote`;`on_ref_px(USDC/S1, 0.9989, 0.9991, 1 s)`(换算率 0.9990)
- 步骤:
  1. `F(Buy, BTCUSDC/S3, 0.1, 60 000, 0, USDC, G7, 0)`、`F(Sell, BTCUSDC/S3, 0.1, 61 000, 0, USDC, G7, 0)`(2 s、3 s)
  2. `on_ref_px(USDC/S1, 1.0009, 1.0011, 4 s)`(换算率 1.0010)
- 期望:
  - 步骤 1 后 `G7.pnl.price_realized` = 99.9 USDT(100 USDC × 0.9990)
  - 步骤 2 后仍为 99.9;`equity(A2)` 中 `S3` 的 USDC 现金按 1.0010 计价
- 实现:`crates/hft-ledger/tests/rates.rs::led_215_realized_not_revalued`

### LED-220 合计权益按估值规则汇总各 venue

- 规则:[权益数据](README.md#权益数据portfolioview-的字段语义)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`valuation = {spot: Bid, perp: Mark, include_baseline: false, include_unassigned: true}`
- 步骤:
  1. `equity(A1)`
- 期望:
  - `Equity { total: 155 999.9, by_venue: [(S1, 105 999.9), (P2, 50 000)], valid: true }`(BNB 10 × 买一 599.99)
- 实现:`crates/hft-ledger/tests/equity.rs::led_220_equity_total`

### LED-221 include_baseline 与 include_unassigned 开关

- 规则:[权益数据](README.md#权益数据portfolioview-的字段语义)、[配置](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)` 但 BNB 初始化为 4 → `set_balance_baseline(A1, S1)`(`baseline` 4)→ `ExternalFill{Manual, Buy, BNB/S1, 6.0, px 600}`(`unassigned` 6);每行设定开关后读 `equity(A1)` 中 BNB 的计入价值(总额减去不含 BNB 时的总额)
- 实现:`crates/hft-ledger/tests/equity.rs::led_221_include_switches`

| 行 | `include_baseline` / `include_unassigned` | 期望 BNB 计入价值 |
|---|---|---|
| LED-221.1 | false / true | 3 599.94 |
| LED-221.2 | true / true | 5 999.9 |
| LED-221.3 | false / false | 0 |
| LED-221.4 | true / false | 2 399.96 |

### LED-222 任一 venue 未初始化或所需换算率过期时 valid 为假

- 规则:[权益数据](README.md#权益数据portfolioview-的字段语义)
- 级别:table
- 里程碑:M2
- 前置:`rd_lf()`、`cfg_lf()`;按行构造
- 实现:`crates/hft-ledger/tests/equity.rs::led_222_equity_validity`

| 行 | 状态 | 期望 `valid` |
|---|---|---|
| LED-222.1 | `boot_a1` 但不发 `A1/P2` 的报告 | false |
| LED-222.2 | `boot_a1(0 s)`,最近入口 `t = 61 s`(BNB 换算率过期) | false |
| LED-222.3 | `boot_a1(0 s)`,最近入口 `t = 30 s` | true |

### LED-223 每 venue 汇总行字段与标志

- 规则:[权益数据](README.md#权益数据portfolioview-的字段语义)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`G1` 永续空 1.0(成本 60 000);`on_mark(BTC/P2, 60 600, 5 s)`
- 步骤:
  1. 读 `venue_book(A1, P2).summary` 与 `venue_book(A1, S1).summary`
  2. `on_balance(A1/S1 USDT free 99 000, quiet = true, 6 s)`(Drift)后读 `S1` 的 `flags`;再 `on_balance(A1/S1 USDT free 100 000, quiet = true, 7 s)`(Ok)后读
- 期望:
  - `P2`:`equity` 49 400、`quote_cash` 50 000、`spot_value` 0、`perp_upnl` −600、`margin_balance` 49 400
  - `S1`:`equity` 105 999.9、`quote_cash` 100 000、`spot_value` 5 999.9
  - 步骤 2:Drift 之后 `flags` 含 `BALANCE_UNRECONCILED`,Ok 之后不含
- 实现:`crates/hft-ledger/tests/equity.rs::led_223_venue_summary`

### LED-224 CashView 的 free 是总额减交易所冻结量且不为负

- 规则:[权益数据](README.md#权益数据portfolioview-的字段语义)
- 级别:table
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`A1/S1` USDC 条目 `total` 与最近报告的 `rep_locked` 按行设定
- 实现:`crates/hft-ledger/tests/equity.rs::led_224_cash_view`

| 行 | `total` / `rep_locked` | 期望 `CashView` |
|---|---|---|
| LED-224.1 | 100 / 30 | `{ free: 70, locked: 30 }` |
| LED-224.2 | 30 / 30 | `{ free: 0, locked: 30 }` |
| LED-224.3 | 20 / 30 | `{ free: 0, locked: 30 }` |

### LED-225 InventoryView 与 inventory_of

- 规则:[权益数据](README.md#权益数据portfolioview-的字段语义)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`inv(BTC, G1) = 0.3`、BTC `unassigned = 0.05`、`baseline = 0`
- 步骤:
  1. 读 BTC 的 `InventoryView`、`inventory(A1, S1, BTC, G1)`、`inventory_of(S1, G1)`
- 期望:
  - `InventoryView { total: 0.35, assigned: 0.3, unassigned: 0.05, baseline: 0 }`;`inventory(…) == 0.3`;`inventory_of(S1, G1) == 0.3`
- 实现:`crates/hft-ledger/tests/equity.rs::led_225_inventory_views`

### LED-226 GroupPosView 的腿、入场价、轮次起点与最近成交时刻

- 规则:[权益数据](README.md#权益数据portfolioview-的字段语义)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`round_dust_notional = 5`;`G1` 在 10 s 买现货 1.0 @ 60 000、11 s 卖永续 1.0 @ 60 100;`on_mark(BTC/P2, 60 100, 11 s)`;`on_ref_px(BTC/S1, 60 000, 60 000.01, 11 s)`
- 步骤:
  1. 读 `GroupPosView(A1, G1)`
- 期望:
  - 腿净量 [+1.0, −1.0];入场价 [60 000, 60 100];未实现 [0, 0];`held_notional` 120 100;`round_start_ens` 10 s;`last_fill_ens` 11 s;`flags` 不含 `ENTRY_ADOPTED`
- 实现:`crates/hft-ledger/tests/equity.rs::led_226_group_pos_view`

### LED-230 snapshot 按节 1、13、22 的顺序一次写完,行按键与种类排序

- 规则:[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`A1`、`A9` 各有现货与永续持仓、`ExtSeen`、`AutoHeld`、`FundHist`、`FundSeen`、`FundNoHist`、待配对项
- 步骤:
  1. `snapshot(&mut w)`,解析写出的 `SectionHdr` 与行
- 期望:
  - 节的出现顺序为 1、13、22,各节恰好一次
  - 节 1 行按 `(account, inst)` 升序;节 13 行的 `kind` 序列符合 Account* → Rate* → Px* → (Group 紧跟其 Pending)* → Margin* → FundPending* → (FundMark 紧跟其 FundHist)* → FundNoHist* → FundSeen* → ExtSeen* → AutoHeld*,同种类内按键升序(`FundSeen`、`ExtSeen` 按入账顺序);节 22 为 Cash* → Inv*
- 实现:`crates/hft-ledger/tests/snapshot.rs::led_230_section_and_row_order`

### LED-231 快照往返:restore 后再 snapshot 逐字节相同

- 规则:[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)、[使用本组件](README.md#使用本组件)
- 级别:prop
- 里程碑:M2
- 性质:任意事件序列之后 `restore(snapshot(L))` 再 `snapshot` 的字节与第一次相同;恢复后的账本与原账本对后续同一事件序列产出相同的 `FillApplied`/`ExternalApplied`/`FundingRecon`/`PnlDelta` 与三节快照
- 生成:LED-240 的生成器;快照点随机
- 参考:原账本(不经快照)
- 例数:PR 1 000;每夜 50 000
- 实现:`crates/hft-ledger/tests/prop_snapshot.rs::led_231_snapshot_roundtrip`

### LED-232 节 1 只写有非零字段的持仓行

- 规则:[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`BTC/P2` 开仓后全平且 `realized_quote = 0`、`fees_quote = 0`(同价开平、无费用);`BTC/S1` 有持仓
- 步骤:
  1. `snapshot`
- 期望:
  - 节 1 只有 `(A1, BTC/S1)` 一行;`(A1, BTC/P2)` 不出现
- 实现:`crates/hft-ledger/tests/snapshot.rs::led_232_positions_only_nonzero`

### LED-233 快照行与公开结构的 size_of 断言

- 规则:[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-ledger/tests/layout.rs::led_233_row_sizes`

| 行 | 结构 | 期望 `size_of`(行体不含 8 B 行头) |
|---|---|---|
| LED-233.1 | `PositionRow` | 64 |
| LED-233.2 | `LedgerRowHdr` | 8 |
| LED-233.3 | Account / Rate / Group 行 | 88 / 32 / 552 |
| LED-233.4 | Cash / Inv 行 | 64 / 16 |
| LED-233.5 | Margin 行 / 每项 | 64 / 16 |
| LED-233.6 | FundPending / FundMark / FundHist / FundNoHist / FundSeen | 32 / 16 / 32 / 32 / 16 |
| LED-233.7 | ExtSeen / AutoHeld / Px | 24 / 16 / 48 |
| LED-233.8 | `LegAgg` / `PnlBreakdown` | 48 / 64 |
| LED-233.9 | 每个上列结构的字段大小之和(含显式 `_pad`) | 等于其 `size_of`(无隐式填充) |

### LED-234 关键行的字段偏移

- 规则:[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-ledger/tests/layout.rs::led_234_row_offsets`

| 行 | 结构 | 期望偏移 |
|---|---|---|
| LED-234.1 | `PositionRow` | `account` 0、`inst` 4、`qty` 8、`cost` 16、`realized_quote` 32、`fees_quote` 48、`last_fill_ens` 56 |
| LED-234.2 | `LedgerRowHdr` | `kind` 0、`account` 1、`venue` 2、`id` 4 |
| LED-234.3 | FundHist 行 | `group` 0、`hflags` 4、`settle_ts_ns` 8、`booked` 16、`corrected` 24 |
| LED-234.4 | ExtSeen 行 | `kind` 0、`a` 8、`b` 16 |
| LED-234.5 | `LedgerRowHdr.kind` 码 | Account 1 … AutoHeld 14、Px 15,与 README 列表逐一相同;0 与 16 在 `restore` 中返回 `Err` |

### LED-235 行版本:写出当前版本,恢复拒绝更高版本

- 规则:[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:table
- 里程碑:M2
- 前置:LED-230 的状态写出的快照
- 实现:`crates/hft-ledger/tests/snapshot.rs::led_235_row_versions`

| 行 | 输入 | 期望 |
|---|---|---|
| LED-235.1 | 读三节 `SectionHdr` 的行版本 | 节 1 = 2,节 13 = 8,节 22 = 2 |
| LED-235.2 | 把节 13 的行版本改为 9 后 `restore` | `Err(RestoreError::VersionUnsupported)`,不 panic |
| LED-235.3 | 把节 1 的行版本改为 3 后 `restore` | 同上 |

### LED-236 i128 字段以两个小端 u64 存放

- 规则:[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:LED-032 之后(`realized_quote` raw −1 000 000)
- 步骤:
  1. `snapshot`,取 `(A1, BTC/P2)` 的 `PositionRow.realized_quote`
- 期望:
  - `[u64; 2] == [0xFFFF_FFFF_FFF0_BDC0, 0xFFFF_FFFF_FFFF_FFFF]`;`restore` 后读回 −1 000 000
- 实现:`crates/hft-ledger/tests/snapshot.rs::led_236_i128_little_endian`

### LED-237 时刻字段只存 engine_ns,恢复后原样有效

- 规则:[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)、[数据结构](README.md#数据结构)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;`last_fill_ens` 10 s、`open_ens` 10 s、`rep_ens` 12 s、换算率 `ens` 12 s
- 步骤:
  1. `snapshot` → `restore`;`on_run_start(500 s)`;入账一笔成交(`t = 501 s`)
- 期望:
  - 恢复后上述字段与快照前逐值相同(不平移);新成交后 `last_fill_ens` = 501 s,`open_ens` 仍为 10 s;三节中没有 TSC 类型的字段
- 实现:`crates/hft-ledger/tests/snapshot.rs::led_237_engine_ns_not_shifted`

### LED-238 任意字节的快照节交给 restore 不 panic

- 规则:[快照行格式](README.md#快照行格式节-11322草案实现时以代码与编译期断言为准)
- 级别:fuzz
- 里程碑:M2
- 目标:`ledger_restore`:把输入当作节 1、13、22 的字节交给 `Ledger::restore`
- 语料:LED-230 写出的快照与 `fixtures/` 中的回放快照节
- 判定:无 panic、无超时;返回 `Ok` 的输入再 `snapshot` 必须得到逐字节相同的输出;崩溃与超时口径见 [testing §3.3](../../docs/testing.md#33-fuzz)
- 实现:`crates/hft-ledger/fuzz/fuzz_targets/ledger_restore.rs::led_238_restore_fuzz`

### LED-240 任意事件序列下已实现加未实现等于现金流

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)、[外部成交](README.md#外部成交externalfill)、[资金费](README.md#资金费)
- 级别:prop
- 里程碑:M2
- 性质:① 价格不变时,每个 (account) Σ 已实现(含 `funding`、`fee_cex`、`external`、`adjust`,加 `funding_residual`)+ 未实现 = 持有资产的价值变化 − 余额采纳带入的价值 − Σ 即时基础资产手续费 × (换算率 − 成交价)(费用按换算率计费、所扣数量按成交价成交),误差 ≤ 已发生的换算次数 × 8 raw;生成器不含行情变化与晚到的基础资产手续费
- 生成:单账户 `A1`,0–300 步,从 {现货/永续成交(四种 `fee_asset_rule`)、`FeeDelta`、外来成交(五种 cause,含重复)、资金费(三种方式,含重复推送)、对账校正条目、划转、核销、采纳} 中抽取;价格在 [30 000, 90 000] 内,数量在 lot 的 1–1 000 倍
- 参考:测试内以 i128 逐笔累计的现金流模型
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-ledger/tests/prop_ledger.rs::led_240_pnl_equals_cashflow`

### LED-241 基础资产不变量在每一步成立

- 规则:[权益数据](README.md#权益数据portfolioview-的字段语义)、[仓位与已实现](README.md#仓位与已实现平均成本法)
- 级别:prop
- 里程碑:M2
- 性质:② 每步之后每个 (account, venue, 基础资产) `total = baseline + unassigned + Σ inv`;平台币扣费从不减少任何其他 group 的 `inv`
- 生成:LED-240 的生成器,另加平台币扣费与 `SetBalanceBaseline`
- 参考:逐步断言等式
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-ledger/tests/prop_ledger.rs::led_241_base_asset_invariant`

### LED-242 现金守恒:资产价值变化等于 PnL 变化加划转

- 规则:[权益数据](README.md#权益数据portfolioview-的字段语义)、[划转与模拟账户](README.md#划转与模拟账户)
- 级别:prop
- 里程碑:M2
- 性质:③ 每步 Σ 资产价值变化(各 venue 汇总行的 `equity`,`valuation.spot = Mid` 使现货估值价等于换算率,计入基线与未归属)= Σ `PnlBreakdown` 变化 + `funding_residual` 变化 + Σ 划转净额 + 即时基础资产手续费 × (换算率 − 成交价),误差 ≤ 16 raw;行情变化、余额采纳(带入外部资产)与晚到的基础资产手续费(按平均成本移出)那一步不判定
- 生成:LED-240 的生成器
- 参考:逐步断言等式
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-ledger/tests/prop_ledger.rs::led_242_cash_conservation`

### LED-243 持仓只由成交、外部成交与采纳改变

- 规则:[仓位与已实现](README.md#仓位与已实现平均成本法)、[人工干预的采纳](README.md#人工干预的采纳adoptexchange)
- 级别:prop
- 里程碑:M2
- 性质:④ `InstPos` 只在 `apply_fill`、`apply_fee`(扣基础资产时)、`apply_external`(返回 `Some`)与 `apply_adjust{Position | Balance}` 中变化;库存归属另可由 `apply_adjust{WriteOff}` 改变;`on_balance`、`on_margin`、资金费入口与 `reconcile_funding` 前后 `InstPos` 与 `inv` 逐字节不变
- 生成:LED-240 的生成器
- 参考:每次调用前后比较 `InstPos`/`inv` 的字节
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-ledger/tests/prop_ledger.rs::led_243_position_change_sources`

### LED-244 窗口内每笔资金费校正后等于最近报告的交易所金额

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:prop
- 里程碑:M2
- 性质:⑥ 对窗口内(`hist_from_ns` 之后、未 `OutsideWindow`)的每个 (inst 或 `INST_NONE`, settle_ts),最近一次 `reconcile_funding` 之后 `booked + corrected` 与该次 `exch_amount` 之差 < 结算资产 dust;同一条目任意重复报告不改变这一结果
- 生成:1–2 个永续 inst(`rd_lf` 的 `BTC/P2`、`ETH/P2`)× 1–20 个结算时刻;三种 `funding_settle`;推送随机丢失、重复、乱序;对账条目随机重复、`now_ns` 在 lag 两侧且单调不减(报告的事件时间按 journal 顺序);最后按结算顺序把每个条目再报告两次,只对仍在窗口内的条目断言
- 参考:按交易所流水的金额表
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-ledger/tests/prop_funding.rs::led_244_funding_converges_to_exchange`

### LED-245 重复投递任何外来成交或推送不改变账本

- 规则:[外部成交](README.md#外部成交externalfill)、[资金费](README.md#资金费)
- 级别:prop
- 里程碑:M2
- 性质:在不超过 `ext_dedup_cap`、`funding_push_seen_cap` 的重放距离内,把序列中任意外来成交与 `BalanceOnlyPush` 推送重复 1–3 次插入,最终三节快照与不重复的序列逐字节相同
- 生成:LED-240 的生成器;重放距离 0–(容量 − 1)
- 参考:无重复的同一序列
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-ledger/tests/prop_ledger.rs::led_245_replays_idempotent`

### LED-246 同一事件序列经 hft-py 调用的结果与 Rust 逐字节相同

- 规则:[冷消费者(`JournalLedger`)](README.md#冷消费者journalledger)、[使用本组件](README.md#使用本组件)
- 级别:integ
- 里程碑:M2
- 前置:hft-py 的夹具 `J_LED20`(`crates/hft-py/examples/pyfixture.rs` 的 `led` 子命令,20 个固定种子;生成器按 LED-240 的操作种类写成 journal 记录:下单与分次成交、外部成交、资金费推送、余额、BBO 与标记价)
- 步骤:
  1. Rust 侧(`pyfixture ledger-ref`)以 `JournalLedger` 按序消费该 journal,记下每个事件各 group 的 `PnlBreakdown` 增量与最终三节快照
  2. 经 `hft_py.ledger`(内部同一 `JournalLedger`)读同一 journal 重建
- 期望:
  - 每隔 10 个事件取一个时刻 t,`hft_py.ledger(…, t1 = t + 1 ns).breakdown()` 等于 Rust 侧 `proc_ns ≤ t` 的增量之和;最终三节快照(`hft_py._testing.ledger_snapshot`)逐字节相同
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_led_246_pyo3_matches_rust`

### LED-247 记账时刻跨重启连续

- 规则:[概要](README.md#概要)、[数据结构](README.md#数据结构)
- 级别:unit
- 里程碑:M2
- 前置:`boot_a1(0 s)`;10 s 入账一笔开仓;快照的 `SnapshotHdr.engine_ns` = 20 s;停机 1 h 后恢复,新运行 `on_run_start(20 s)`(引擎时间不计停机)
- 步骤:
  1. `restore`;`t = 25 s` 入账一笔加仓
- 期望:
  - `last_fill_ens` = 25 s;`GroupPosView.last_fill_ens` = 25 s;`round_start_ens` 仍为 10 s(两次成交的 engine_ns 差 15 s,不含停机的 1 h)
- 实现:`crates/hft-ledger/tests/snapshot.rs::led_247_engine_time_continuous`

### LED-250 apply_fill 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ledger/apply_fill`
- 实现:`crates/hft-ledger/benches/ledger.rs::led_250_apply_fill`

### LED-251 apply_external 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ledger/apply_external`
- 实现:`crates/hft-ledger/benches/ledger.rs::led_251_apply_external`

### LED-252 apply_funding_push 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ledger/apply_funding_push`
- 实现:`crates/hft-ledger/benches/ledger.rs::led_252_apply_funding_push`

### LED-253 reconcile_funding 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ledger/reconcile_funding`
- 实现:`crates/hft-ledger/benches/ledger.rs::led_253_reconcile_funding`

### LED-254 funding_window_sweep 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ledger/funding_window_sweep`
- 实现:`crates/hft-ledger/benches/ledger.rs::led_254_funding_window_sweep`

### LED-255 on_ref_px 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ledger/on_ref_px`
- 实现:`crates/hft-ledger/benches/ledger.rs::led_255_on_ref_px`

### LED-256 on_mark 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ledger/on_mark`
- 实现:`crates/hft-ledger/benches/ledger.rs::led_256_on_mark`

### LED-257 equity 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ledger/equity`
- 实现:`crates/hft-ledger/benches/ledger.rs::led_257_equity`

### LED-258 一天 journal 重建吞吐基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ledger/replay_day`
- 实现:`crates/hft-ledger/benches/ledger.rs::led_258_replay_day`

### LED-260 模拟账户连续运行时账本与模拟真值逐资产一致

- 规则:[余额初始化、校验与基线](README.md#余额初始化校验与基线)、[资金费](README.md#资金费)、[划转与模拟账户](README.md#划转与模拟账户)
- 级别:dst
- 里程碑:M4
- 门槛:⑦
- 前置:testkit `SimActor`(hft-sim `SimExchange`,`late_fee = true`)与测试策略在 `A9` 上运行 24 h 模拟时间,含 `SimTransfer`、`SimReset` 各一次与三种 `funding_settle`
- 故障:rep-sim 回报随机延迟 0–2 s;在结算前后 ±2 min 内随机插入行情空洞
- 种子:PR 50 + 回归集;每夜日期派生
- 断言:每个无在途时刻,`A9` 每 (venue, asset) 的 `CashEntry.total` 与 `SimExchange::account(A9)` 的余额相差 < 资产 dust,每个永续 inst 的 `InstPos.qty` 相等;每次结算合成额与模拟结算额之差 ≤ `max(funding_tol_abs, funding_tol_rel × |模拟额|)`;INV-16
- 实现:`crates/hft-testkit/src/scenarios/ledger.rs::led_260_sim_account_conservation`

### LED-261 测试网与模拟盘联调中账本逐笔与交易所一致

- 规则:[余额初始化、校验与基线](README.md#余额初始化校验与基线)、[资金费对账校正](README.md#资金费对账校正)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2 联调窗口内的 `prod` 角色实例,Binance、OKX、Bitget 现货与 USDT 永续的测试网/模拟盘账户
- 授权:无
- 步骤:
  1. 每所 ≥ 20 笔成交(含基础币扣费的现货买入与永续开平),每笔后 reconciler 一轮对账
  2. 持仓跨过至少一次资金费结算(该所测试网提供资金费流水时)
- 通过标准:每笔之后 ledger 的持仓差 < 1 lot、各资产余额差 < 资产 dust;资金费在结算后 `funding_recon_lag_ms` + 一轮对账之内与交易所流水一致(不提供流水的所记"未比对",待核实(方法:正式实盘早期按同一比对复核))
- 证据:每所一份比对记录(reconciler 轮次明细与逐笔比对结果),文件名含 `LED-261` 与 venue,按 [testing §8](../../docs/testing.md#8-上线门槛) 归入门槛 ⑥ 记录
- 实现:`tests/host/LED-261.sh`

### LED-262 sim 实例长跑中无 SimDrift

- 规则:[余额初始化、校验与基线](README.md#余额初始化校验与基线)
- 级别:host
- 里程碑:M6
- 门槛:⑦
- 环境:dev2 `sim` 实例,门槛 ⑦ 的 dev2 段(≥ 14 天)
- 授权:无
- 步骤:
  1. 模拟账户连续运行;watchtower `SimDrift` 规则开启
- 通过标准:运行期间 `SimDrift` 告警 0 条;每日 `replayer verify` 通过
- 证据:watchtower 日报中的 `SimDrift` 计数与 verify 报告,归档到门槛 ⑦ 记录(文件名含 `LED-262`)
- 实现:`tests/host/LED-262.sh`

### LED-263 JournalLedger 从快照起步,按 OrderDecision 建槽映射并把 OrderEvent 的成交增量入账

- 规则:[冷消费者(`JournalLedger`)](README.md#冷消费者journalledger)、[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M2
- 前置:`rd_lf()`、`cfg_lf()`;journal 片段:一条 `StateSnapshot`(`boot_a1(t0)` 之后的状态,OMS 节 2 无非空闲槽)、`Note{EpochStart, v[0] = 7}`、`OrderDecision{action New, reject 0, epoch 段内, slot 5, gen 1, account A1, venue S1, inst BTC/S1, side Buy, group G1, qty 0.01}`、`OrderEvent{PartialFill, epoch 7, slot 5, gen 1, cum_filled 0.004, avg_px 60 000.00, fee 0, fee_asset ASSET_NONE}`、`OrderEvent{Fill, cum_filled 0.01, avg_px 60 001.00}`;`S1.fee_asset_rule = Quote`
- 步骤:
  1. `JournalLedger::from_snapshot` 后按序 `apply` 每条记录
- 期望:
  - `ledger().pos(A1, BTC/S1)` 与 `inventory(A1, S1, BTC, G1)` 都为 +0.01;第二次增量 0.006、增量名义 = 60 001.00 × 0.01 − 60 000.00 × 0.004 = 360.01;USDT `total` = 100 000 − 600.01;`stats().unmapped_fills = 0`
- 实现:`crates/hft-ledger/tests/journal_ledger.rs::led_263_fills_from_journal`

### LED-264 JournalLedger 按 cum_filled 去重:重复与倒退的回报不入账

- 规则:[冷消费者(`JournalLedger`)](README.md#冷消费者journalledger)
- 级别:table
- 里程碑:M2
- 前置:同 LED-263 的快照、`EpochStart` 与 `OrderDecision`;之后依次 `apply` 所列 `OrderEvent`(epoch 7、slot 5、gen 1)
- 实现:`crates/hft-ledger/tests/journal_ledger.rs::led_264_cum_filled_dedup`

| 行 | `cum_filled` 序列 | 期望 `pos(A1, BTC/S1)` |
|---|---|---|
| LED-264.1 | 0.004、0.004 | 0.004(第二条不入账) |
| LED-264.2 | 0.006、0.004 | 0.006(倒退的不入账) |
| LED-264.3 | 0.004、0.01、0.01 | 0.01 |
| LED-264.4 | `Ack`(cum 0)、0.01 | 0.01 |

### LED-265 映射缺失的成交只计数,OMS 的 OmsForeignFill 注记照常入账

- 规则:[冷消费者(`JournalLedger`)](README.md#冷消费者journalledger)、[外部成交(`ExternalFill`)](README.md#外部成交externalfill)
- 级别:unit
- 里程碑:M2
- 前置:同 LED-263 的快照与 `EpochStart`;无 slot 9 的 `OrderDecision`
- 步骤:
  1. `apply(OrderEvent{Fill, epoch 7, slot 9, gen 3, cum_filled 0.002, exch_order_id 77})`
  2. `apply(Note{OmsForeignFill})`(同一事件内 OMS 写出,对应 BTC/S1 买 0.002)
- 期望:
  - 步骤 1 之后 `stats().unmapped_fills = 1`,`pos(A1, BTC/S1)` 不变
  - 步骤 2 之后 `pos(A1, BTC/S1)` +0.002,与 `apply_external(external_from_oms_note(n), false, t)` 的结果相同
- 实现:`crates/hft-ledger/tests/journal_ledger.rs::led_265_unmapped_and_foreign_note`

### LED-266 任意 journal 在每个快照点上 JournalLedger 与引擎 Ledger 的持仓、库存与余额相等

- 规则:[冷消费者(`JournalLedger`)](README.md#冷消费者journalledger)
- 级别:prop
- 里程碑:M2
- 性质:从第一条 `StateSnapshot` 起步的 `JournalLedger` 喂完到第 k 条 `StateSnapshot` 为止的全部记录后,其 `pos`、`inventory`、`venue_book`(余额)与从第 k 条快照 `restore` 的 Ledger 逐项相等
- 生成:测试内的 sequencer 替身(写出 sequencer 会写的 journal 记录并按同样的调用驱动引擎 Ledger;hft-testkit 的 sequencer 夹具落地后改用它)在 `rd_lf()` 上运行 1–200 条随机输入(下单、部分成交、重复与乱序回报、`FeeOnly`、`ExternalFill`、资金费推送、`AdoptExchange`、`StaleGen` 带成交),每 20 条写一次 `StateSnapshot`
- 参考:引擎 Ledger 在同一快照处的读数
- 例数:PR 500
- 实现:`crates/hft-ledger/tests/prop_journal_ledger.rs::led_266_cold_equals_engine_at_snapshots`

### LED-267 DST 中每个快照点 JournalLedger 等于引擎 Ledger

- 规则:[冷消费者(`JournalLedger`)](README.md#冷消费者journalledger)
- 级别:dst
- 里程碑:M2
- 故障:标准故障注入表(断线、回报重复与乱序、`StaleGen`、ADL 外来成交、资金费推送丢失)
- 种子:PR 200(提交派生)+ 回归集;每夜日期派生
- 断言:每条 `StateSnapshot` 处 JournalLedger 的 `pos`、`inventory`、`venue_book` 与引擎 Ledger 相等;`unmapped_fills` 的每一笔都在同一事件内有 `Note{OmsForeignFill}` 或属于 `Shadow` 扣留的单
- 实现:待实现

### LED-268 资金费逐笔差额以 AssetMeta.dust 为界判 Matched

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:table
- 里程碑:M2
- 前置:`fund_a1(PerSymbolPush)`;USDT `AssetMeta.dust = 0.01`;`T1` 已入账 6.00;`now_ns = T1 + 1 h`
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_268_dust_boundary`

| 行 | 交易所金额 | 期望 |
|---|---|---|
| LED-268.1 | 6.009999 | `Matched` |
| LED-268.2 | 6.01 | `Corrected`,`qty` +0.01 |
| LED-268.3 | 5.990001 | `Matched` |

### LED-269 ledger 段验证器对每个 ref_inst 调 check_ref_inst

- 规则:[配置(`LedgerConfig`,`EngineConfig` blob 的 `ledger` 段)](README.md#配置ledgerconfigengineconfig-blob-的-ledger-段)
- 级别:table
- 里程碑:M2
- 前置:`rd_lf()`;`cfg_lf()` 但 `ref_inst[USDC]` 如列;经 `EngineConfig::validate` 校验 `ledger` 段
- 实现:`crates/hft-ledger/tests/config.rs::led_269_ref_inst_validated`

| 行 | `ref_inst[USDC]` | 期望 |
|---|---|---|
| LED-269.1 | `USDC/S1`(`Trading`) | `Ok` |
| LED-269.2 | 不在 refdata 的 InstId 999 | `Err(LoadErrCode::RefInstNotSubscribed)` |
| LED-269.3 | 状态为 `Halted` 的 inst | `Err(LoadErrCode::RefInstNotSubscribed)` |

### LED-270 BalanceOnlyPush 所的 inst ≠ INST_NONE 条目不适用,不动账

- 规则:[资金费对账校正](README.md#资金费对账校正)
- 级别:unit
- 里程碑:M2
- 前置:LED-096 之后(合成 6.00、推送 6.04、`funding_residual` +0.04)
- 步骤:
  1. `reconcile_funding(R(BTC/P2, T1, 6.04, 6.00), T1 + 1 h, 30 s)`
- 期望:
  - `NotApplicable`;`G1.pnl.funding`、`funding_residual`、`wallet` 与 USDT `total` 都不变;挂起表为空;无 `Note{FundingCorrected}`
- 实现:`crates/hft-ledger/tests/funding_recon.rs::led_270_balance_only_inst_item_not_applicable`

## 验收

- 里程碑 M2:本文件中里程碑为 M2 的用例全部有同编号的测试函数、场景或基准且通过(追溯规则见 [docs/tests §6](../../docs/tests/README.md#6-与代码的对应))。
- 属性测试:LED-115、LED-157、LED-159、LED-179、LED-231、LED-240–LED-245 在发布前各跑 1 000 000 例无失败;PR 与每夜例数见各用例。
- DST:LED-080、LED-081 在 PR 冒烟与每夜通过,计入 [testing §8](../../docs/testing.md#8-上线门槛) 门槛 ④ 的累计;LED-260 在 M4 起同样运行。
- 基准:LED-250–LED-258 在参考机上满足 [benchmarks](../../benchmarks/README.md) 登记的上限(上限只在那里登记)。
- 上线门槛:⑥ 以 LED-261 的每所比对记录为证据;⑦ 以 LED-260 与 LED-262 为证据之一。
- 链上腿(feature `chain`)的用例在该可选扩展启用时补写。
