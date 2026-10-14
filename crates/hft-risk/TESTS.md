---
component: hft-risk
prefix: RSK
spec: README.md
---

# hft-risk 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 热路径:分类与检查 | RSK-003、RSK-004、RSK-020、RSK-031、RSK-039、RSK-040、RSK-041、RSK-051、RSK-162、RSK-180、RSK-181、RSK-230、RSK-231、RSK-240、RSK-272 |
| 保证金分档的输入与输出 | RSK-161、RSK-172 |
| mode 模块:唯一的模式状态 | RSK-009、RSK-043、RSK-053、RSK-060、RSK-062、RSK-072、RSK-076–RSK-080、RSK-141、RSK-192、RSK-221、RSK-270 |
| 实现说明 | 不适用:见子节 |
| 分类 | RSK-010–RSK-013 |
| 检查链(按廉价 → 昂贵,短路;适用范围以本表为唯一定义) | RSK-020–RSK-045、RSK-271 |
| 断腿(Broken)、自动有界平仓、L3 与 flatten | RSK-022、RSK-050–RSK-056 |
| 有效模式 | RSK-024、RSK-060、RSK-061、RSK-270 |
| cap 记录 | RSK-009、RSK-025、RSK-050、RSK-052–RSK-054、RSK-063、RSK-070–RSK-077、RSK-104、RSK-105、RSK-131、RSK-202、RSK-210、RSK-220、RSK-223 |
| 干净 Drain 重启自动解除 Recovery | RSK-090–RSK-094 |
| 启动闸门 | RSK-077、RSK-100–RSK-109、RSK-271、RSK-272 |
| 对账闸门与 HARD 解除的严格条件 | RSK-079、RSK-130、RSK-131 |
| kill 字(唯一的跨线程共享可变量) | RSK-140、RSK-230、RSK-270 |
| Kill | RSK-077、RSK-140–RSK-142 |
| 保证金分档(MarginStress) | RSK-283、RSK-001、RSK-002、RSK-160–RSK-172、RSK-280 |
| 永续杠杆上限 | RSK-101、RSK-180、RSK-181 |
| 当日亏损(锁存) | RSK-001、RSK-002、RSK-190–RSK-193 |
| 对账分级(全系统唯一一次) | RSK-282、RSK-109、RSK-200–RSK-203 |
| 时钟 | RSK-023、RSK-210 |
| 令牌桶 | RSK-036–RSK-040、RSK-272 |
| 存储降级(Storage) | RSK-220–RSK-222、RSK-281 |
| 下架与公告(VenueNotice) | RSK-141、RSK-223 |
| 配置(`RiskParams`,版本化,经 `SetRisk` 替换并作为 `ConfigBlob` 写入 journal) | RSK-001、RSK-003–RSK-006、RSK-028、RSK-032、RSK-284–RSK-286 |
| 引擎单与执行配置的一致性(`ExecRiskMismatch`) | RSK-007 |
| 快照行格式(节 4、17、18、23;草案,实现时以代码与编译期断言为准) | RSK-008、RSK-009、RSK-230 |
| 规划源文件 | RSK-001–RSK-013、RSK-020、RSK-036、RSK-060、RSK-100、RSK-130、RSK-160、RSK-180、RSK-190、RSK-200、RSK-270 |
| 延迟与资源契约 | RSK-241、RSK-250–RSK-264 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**基线 `R0`**(`crates/hft-risk/tests/common/mod.rs::r0()`;用例 `前置` 写"R0"时下列取值全部显式生效,用例另列的值覆盖它们;不依赖任何默认值):

- **定点**:BTC、ETH 计价 USDT,Px 指数 −2,Qty 指数 −8;`Notional` 1e-6 USDT;`Rate` 1e-9。下文写十进制值:`50 000.00` = Px(5 000 000, −2),`0.01` BTC = Qty(1 000 000, −8),`500` USDT = Notional(500 000 000),`0.03` = Rate(30 000 000)。
- **refdata**(id 稠密,venue 用真实 `VenueId` 以通过 refdata 加载校验):venue A = `VenueId 2`(`BINANCE_SPOT`,现货,交易所 X,`VenueCaps.reduce_only = false`)、B = `VenueId 3`(`GATE_USDT_FUT`,USDT 永续,交易所 Y,`reduce_only = true`,`margin_data = Push`)、C = `VenueId 5`(`OKX_SWAP`,USDT 永续,交易所 Z,`reduce_only = true`,`margin_data = Poll{interval_ms = 2 000}`);inst `a1` = `InstId 1`(A,BTC/USDT,tick 0.01,lot 0.0001)、`a2` = 2(A,ETH/USDT,tick 0.01,lot 0.001)、`b1` = 3(B,BTC 永续,tick 0.10,lot 0.001)、`b2` = 4(B,ETH 永续,tick 0.01,lot 0.01)、`c1` = 5(C,BTC 永续,tick 0.10,lot 0.001),状态都是 `Trading`;账户 1(`Live`,`is_prod = false`,`uid_policy = Verified`,venues {A, B, C},绑定实例 s1 = `StrategyId 1`)、账户 2(`Live`,venues {A, B},绑定 s2 = 2)、账户 3(`Sim`,venues {A, B},绑定 s3 = 3);`venue_account_ix`:A、B 上账户 1 → 0、账户 2 → 1,C 上账户 1 → 0;gw 0 = A、gw 1 = B、gw 2 = C;group `g1 = 0x0100_0001`(s1,账户 1,腿 a1 + b1)、`g3 = 0x0100_0002`(s1,账户 1,腿 a2 + b2)、`g2 = 0x0200_0001`(s2,账户 2,腿 a1 + b1)、`gs = 0x0300_0001`(s3,账户 3,腿 a1 + b1);`RateLimitTable` 每 (venue, account):10 s 窗口 100 单、1 min 窗口 600 单。
- **`RiskParams` P0**:`rate_headroom = 0`、`conn_liveness_ms = 2 000`、`max_perp_leverage_x100 = 300`、`l3_max_cross = 0`;`classes[0] = {max_px_dev 0.03, ioc_max_cross 0.005, rr_max_cross 0.01}`、`classes[1] = {0.05, 0.01, 0.02}`;`risk_class`:a1、b1、c1 → 0,a2、b2 → 1;账户 1、2、3 的 `AcctLimits` 相同:`max_order_notional 10 000`、`default_group {max_group_notional 50 000, max_pos_qty 1, naked_gate 0.5, naked_gate_min 100}`、`max_account_notional 100 000`、`account_notional_from_equity 1.0`、`max_hedge_notional 20 000`、`max_reduce_notional 10 000`、`daily_loss_kill 0.05`、`daily_loss_abs 5 000`、`daily_loss_max_resumes 1`、`recon_closeonly_notional 100`、`unknown_blocks_open = true`、`funds_buffer 0.05`、`margin {block_open 0.25, reduce 0.15, force 0.10, hyst 0.02, crosscheck 0.80, ratio_hyst 0.05, deadline 30 000 ms, force_step 0.20, force_step_interval 5 000 ms, margin_stale 15 000 ms, mark_stale 10 000 ms}`;`groups`:(1, g1)、(2, g2)、(3, gs) = `{max_group_notional 50 000, max_pos_qty 1(BTC), naked_gate 0.5, naked_gate_min 100}`,(1, g3) = `{50 000, 20(ETH), 0.5, 100}`;`venues`:每 (venue, account) `{max_venue_notional 80 000, inst_bucket_per_s 1 000, backoff_base_ms 1 000, backoff_max_ms 8 000, rr_reserve 0.20}`。
- **`ExecBounds`**(账户 1、2、3):`force_max_cross 0.01`、`force_max_chunk 10 000`、`dust_notional 10`。
- **时间**:`Now` 由 testkit `now_at(ens)` 构造,`tsc_per_ms = 1 000 000`(1 tick = 1 ns),本次运行 `tsc = engine_ns`;当前时刻 `T0 = 1 000 000 000 000`(engine_ns);`set_run_start(0)`。"t0 + 5 s"等写法都是 engine_ns。
- **视图**(`MockRiskView`):仓位、在途、已挂减仓量全为 0;`venue_ready = true`;`group_has_unknown = false`;`equity = {200 000, valid}`;`held_notional`、`open_notional` 为 0;`avail_quote(·, A, USDT) = 100 000`;`avail_margin(·, B) = avail_margin(·, C) = 100 000`;BBO 全部 `conn_up`、`resync = false`、`conn_live_tsc = 当前 tsc`:a1 49 999.00 / 50 001.00(mid 50 000.00)、a2 2 499.50 / 2 500.50(mid 2 500.00)、b1 49 995.0 / 50 005.0(mid 50 000.0)、b2 2 499.0 / 2 501.0(mid 2 500.0)、c1 49 995.0 / 50 005.0;USDT 的 `quote_rate = 1`。
- **账户配置**:每个 Live (account, venue) 已经过 `on_account_cfg` 与 `apply(Gate(AccountCfg{hdr{mismatches = 0, uid_keys = SESSION|REST, 三种模式与 VenueCaps 一致}, keys_required = SESSION|REST}))`;永续 venue 的报告对 b1、b2、c1 各给 `leverage = 3.00`。
- **模式**:全部账户启动闸门已放行(各 venue 位图齐全、kill 自检已通过、`Gate(Clock{ok: true})`),全部 cell 期望模式 Live,无 cap,`ctl_state_version = V`(下文写 V + n);`ModeView` 适配器:`cell_flat = true`、`naked_cleared = true`、`recon_resumable = true`、`daily_loss_resumable = true`、`unknown_count = 0`、`engine_reduce_pending = false`。
- **保证金**:每个 (account, 永续 venue) 已有 `MarginSample{rep_liq_dist = Rate::MAX, est_liq_dist = Rate::MAX, reported_ratio = 0.10, est_ratio = 0.10, report_ens = T0, mark_ens = i64::MAX, has_report = true, truncated = false}`,档位 L0,堆中无 `Timer{Risk, …}`。

**意图简写**:`Open(a, g, inst, side, qty @ px, tif)` = `origin = Strategy`、`class = Open`、`strategy` = 账户 a 绑定的实例、`link_venue = VENUE_NONE` 的意图,`notional = Risk::notional(qty, px, 1)`;`Hedge(…)` = `origin = Oms`、`link_role = Second`、`class = Hedge`、`flags` 含 `RISK_REDUCING`(另写 `hedge_cap_qty`);`Reduce(…)`、`Flatten(…)` 按 `classify` 的结果填 `class` 与 `flags`(`origin` 另写)。`LinkOpen(…; link = (acct, venue, inst, side, link_qty, link_notional))` = 带链接腿的第一腿 Open。"放行" = `evaluate` 返回 `Ok`;"拒 `X`" = 返回 `Err(RejectBits::X)`,且恰有这一位。

### RSK-001 RiskParams 每个字段的默认值

- 规则:[配置](README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)、[保证金分档](README.md#保证金分档marginstress)、[当日亏损](README.md#当日亏损锁存)
- 级别:table
- 里程碑:M2
- 前置:R0 的 refdata,另加账户 4(`Live`,venues {C});本 crate 的参数构建器(`src/params.rs`,hft-cfg 复用)对账户 1、2、4 只给 `max_order_notional = 10 000` 与 `classes[0].ioc_max_cross = 0.005`,其余字段缺省
- 实现:`crates/hft-risk/tests/params.rs::rsk_001_risk_params_defaults`

| 行 | 字段 | 期望 |
|---|---|---|
| RSK-001.1 | `conn_liveness_ms` | 2 000 |
| RSK-001.2 | `max_perp_leverage_x100` | 300 |
| RSK-001.3 | `l3_max_cross` | 0 |
| RSK-001.4 | `rr_reserve` | 0.20 |
| RSK-001.5 | `classes[0].rr_max_cross` | 0.01(= 2 × 0.005) |
| RSK-001.6 | `max_hedge_notional` | 20 000 |
| RSK-001.7 | `max_reduce_notional` | 10 000 |
| RSK-001.8 | `daily_loss_max_resumes` | 1 |
| RSK-001.9 | `funds_buffer` | 0.05 |
| RSK-001.10 | `margin.liq_dist_block_open` / `liq_dist_reduce` / `liq_dist_force` | 0.25 / 0.15 / 0.10 |
| RSK-001.11 | `margin.liq_dist_hyst` / `margin_ratio_crosscheck` / `margin_ratio_hyst` | 0.02 / 0.80 / 0.05 |
| RSK-001.12 | `margin.deleverage_deadline_ms` / `force_step` / `force_step_interval_ms` | 30 000 / 0.20 / 5 000 |
| RSK-001.13 | `margin.mark_stale_ms` | 10 000 |
| RSK-001.14 | 账户 2(永续 venue 只有 B,`Push`)的 `margin.margin_stale_ms` | 15 000 |
| RSK-001.15 | 账户 4(永续 venue 只有 C,`Poll{2 000}`)的 `margin_stale_ms` | 6 000 |
| RSK-001.16 | 账户 1(B 与 C)的 `margin_stale_ms` | 15 000(两者取大) |
| RSK-001.17 | `unknown_blocks_open` | true |

### RSK-002 参数校验在 new 时拒绝非法的保证金阈值与日亏额度

- 规则:[保证金分档](README.md#保证金分档marginstress)(参数校验)、[当日亏损](README.md#当日亏损锁存)(`daily_loss_abs` 必须 > 0)
- 级别:table
- 里程碑:M2
- 前置:R0;每行只改账户 1 的所列字段,调 `Risk::new`
- 实现:`crates/hft-risk/tests/params.rs::rsk_002_param_validation`

| 行 | 改动 | 期望 |
|---|---|---|
| RSK-002.1 | 无 | `Ok` |
| RSK-002.2 | `liq_dist_block_open = 0.15`(= reduce) | `Err(RiskParamError::Invalid)` |
| RSK-002.3 | `liq_dist_block_open = 0.150000001` | `Ok` |
| RSK-002.4 | `liq_dist_reduce = 0.10`(= force) | `Err(Invalid)` |
| RSK-002.5 | `liq_dist_force = 0` | `Err(Invalid)` |
| RSK-002.6 | `liq_dist_force = 0.000000001` | `Ok` |
| RSK-002.7 | `liq_dist_hyst = 0.05`(= reduce − force) | `Err(Invalid)` |
| RSK-002.8 | `liq_dist_hyst = 0.049999999` | `Ok` |
| RSK-002.9 | `daily_loss_abs = 0` | `Err(Invalid)` |
| RSK-002.10 | `daily_loss_abs = 0.000001` | `Ok` |
| RSK-002.11 | `daily_loss_abs = −1` | `Err(Invalid)` |

### RSK-003 SetRisk 参数非法时返回 Invalid 且旧参数继续生效

- 规则:[配置](README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)、[热路径:分类与检查](README.md#热路径分类与检查)(`set_params`)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. `set_params(P0 改 liq_dist_block_open = 0.15、max_order_notional = 20 000, scope = Account(1))`
  2. `on_margin(1, B, liq_dist = 0.25 的样本)`
  3. `evaluate(Open(1, g1, a1, Buy, 0.3 @ 50 000.00, Gtc))`(名义 15 000)
- 期望:
  - 步骤 1 返回 `Err(RiskParamError::Invalid)`
  - 步骤 2 后账户 1 在 B 为 L1(阈值仍为 0.25)
  - 步骤 3 拒 `NOTIONAL_ORDER`(`max_order_notional` 仍为 10 000)
- 实现:`crates/hft-risk/tests/params.rs::rsk_003_invalid_setrisk_keeps_old`

### RSK-004 RiskParams 各向量按 refdata 与 Capacities 一次分配,超出即 Capacity

- 规则:[配置](README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)(容量)、[热路径:分类与检查](README.md#热路径分类与检查)(`set_params`)
- 级别:table
- 里程碑:M2
- 前置:refdata 有 20 个 Live 账户(账户 1–10 只在 A、11–20 只在 B;每 venue 的 Live 账户数受 `KillBits::MAX_ACCOUNTS_PER_GW` 限制)、`Capacities{max_insts = 64, max_cells_total = 32}`;`Risk::new` 用 20 行账户参数成功;此后 testkit `CountingAlloc` 计数清零
- 实现:`crates/hft-risk/tests/params.rs::rsk_004_capacity_rows`

| 行 | `set_params(scope = *)` 的输入 | 期望 |
|---|---|---|
| RSK-004.1 | `accounts` 20 行 | `Ok`;分配次数 0 |
| RSK-004.2 | `accounts` 21 行 | `Err(RiskParamError::Capacity)`;此后 `view(1).max_order_notional` 等于旧值 |
| RSK-004.3 | `groups` 32 行 | `Ok` |
| RSK-004.4 | `groups` 33 行 | `Err(Capacity)` |
| RSK-004.5 | `risk_class` 64 行 | `Ok` |
| RSK-004.6 | `risk_class` 65 行 | `Err(Capacity)` |
| RSK-004.7 | `venues` 21 行((account, venue) 数为 20) | `Err(Capacity)` |

### RSK-005 账户作用域的 SetRisk 只替换该账户的字段

- 规则:[配置](README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)(账户作用域)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. `set_params(P1, scope = Account(1))`,P1 = P0 改账户 1、账户 2 的 `max_order_notional` 都为 15 000(不低于 `force_max_chunk`,否则是 RSK-007.7 的 `ExecRiskMismatch`),`classes[0].rr_max_cross = 0.02`、`l3_max_cross = 0.03`
- 期望:
  - 返回 `Ok`
  - `view(1).max_order_notional = 15 000`,`view(2).max_order_notional = 10 000`
  - `view_for(1, a1).rr_max_cross = 0.01`、`view_for(1, a1).l3_max_cross = 0`(全局字段只随 `scope = *` 替换)
- 实现:`crates/hft-risk/tests/params.rs::rsk_005_account_scope_replace`

### RSK-006 loosens 按字段方向判定放宽

- 规则:[配置](README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)(放宽判定)
- 级别:table
- 里程碑:M2
- 前置:R0;每行在 P0 上只改所列字段,`scope` 见行(未写为 `Account(1)`,改的是账户 1 的行)
- 实现:`crates/hft-risk/tests/params.rs::rsk_006_loosens_by_field`

| 行 | 改动 | `loosens` |
|---|---|---|
| RSK-006.1 | 无改动 | false |
| RSK-006.2 | `max_order_notional` 10 000 → 10 000.000001 | true |
| RSK-006.3 | `max_order_notional` 10 000 → 9 999.999999 | false |
| RSK-006.4 | `max_hedge_notional`、`max_reduce_notional`、`max_account_notional`、`max_group_notional`(g1)、`max_venue_notional`((A, 1))、`max_pos_qty`(g1)各自 +1 raw(六个子例) | 每个都 true |
| RSK-006.5 | `account_notional_from_equity` 1.0 → 1.000000001 | true |
| RSK-006.6 | `daily_loss_abs` +1 raw;`daily_loss_kill` +1 raw;`daily_loss_max_resumes` 1 → 2 | 每个都 true |
| RSK-006.7 | `recon_closeonly_notional`、`naked_gate`、`naked_gate_min`、`inst_bucket_per_s` 各自变大 | 每个都 true |
| RSK-006.8 | `liq_dist_block_open` 0.25 → 0.249999999 | true |
| RSK-006.9 | `liq_dist_block_open` 0.25 → 0.250000001 | false |
| RSK-006.10 | `liq_dist_reduce`、`liq_dist_force`、`liq_dist_hyst`、`margin_ratio_hyst`、`force_step` 各自 −1 raw | 每个都 true |
| RSK-006.11 | `margin_ratio_crosscheck`、`deleverage_deadline_ms`、`force_step_interval_ms`、`margin_stale_ms`、`mark_stale_ms` 各自变大 | 每个都 true |
| RSK-006.12 | `funds_buffer` 0.05 → 0.049999999 | true |
| RSK-006.13 | `funds_buffer` 0.05 → 0.050000001 | false |
| RSK-006.14 | `unknown_blocks_open` true → false | true |
| RSK-006.15 | `unknown_blocks_open` false → true(前置先为 false) | false |
| RSK-006.16 | `rr_reserve`((A, 1))0.20 → 0.19;`backoff_base_ms` 1 000 → 999;`backoff_max_ms` 8 000 → 7 999 | 每个都 true |
| RSK-006.17 | `scope = *`:`rate_headroom` 0 → 0.1 | false |
| RSK-006.18 | `scope = *`:`rate_headroom` 0.1 → 0(前置先为 0.1) | true |
| RSK-006.19 | `scope = *`:`conn_liveness_ms` 2 000 → 2 001 | true |
| RSK-006.20 | `scope = *`:`max_perp_leverage_x100` 300 → 301 | true |
| RSK-006.21 | `scope = *`:`l3_max_cross` 0 → 0.000000001 | true |
| RSK-006.22 | `scope = *`:`l3_max_cross` 0.02 → 0.03(前置先为 0.02) | true |
| RSK-006.23 | `scope = *`:`l3_max_cross` 0.02 → 0.01;0.02 → 0 | 两者都 false |
| RSK-006.24 | `scope = *`:`classes[0].max_px_dev`、`ioc_max_cross`、`rr_max_cross` 各自 +1 raw | 每个都 true |
| RSK-006.25 | `scope = *`:`risk_class` 把 a1 从类别 0 改到类别 1(类别 1 三项都更大) | true |
| RSK-006.26 | `scope = *`:`risk_class` 把 a2 从类别 1 改到类别 0 | false |
| RSK-006.27 | `scope = Account(1)`:账户 2 的 `max_order_notional` 变大、账户 1 不变 | false(作用域外不计) |

### RSK-007 执行配置越出风控上限时 new 与 SetRisk 都返回 ExecRiskMismatch

- 规则:[引擎单与执行配置的一致性](README.md#引擎单与执行配置的一致性execriskmismatch)
- 级别:table
- 里程碑:M2
- 前置:R0;账户 1 可交易 inst 为 a1、b1、c1(类别 0,`rr_max_cross 0.01`)与 a2、b2(类别 1,0.02),最小值 0.01
- 实现:`crates/hft-risk/tests/params.rs::rsk_007_exec_risk_mismatch`

| 行 | 输入 | 期望 |
|---|---|---|
| RSK-007.1 | `new`,`ExecBounds(1).force_max_cross = 0.01` | `Ok` |
| RSK-007.2 | `new`,`force_max_cross = 0.010000001` | `Err(RiskParamError::ExecRiskMismatch)` |
| RSK-007.3 | `new`,`force_max_cross = 0.009999999` | `Ok` |
| RSK-007.4 | `new`,`force_max_chunk = 10 000`(= `max_order_notional`) | `Ok` |
| RSK-007.5 | `new`,`force_max_chunk = 10 000.000001` | `Err(ExecRiskMismatch)` |
| RSK-007.6 | R0 后 `set_params(scope = *, classes[0].rr_max_cross = 0.009999999)` | `Err(ExecRiskMismatch)`;随后 `view_for(1, a1).rr_max_cross = 0.01` |
| RSK-007.7 | R0 后 `set_params(scope = Account(1), max_order_notional = 9 999.999999)` | `Err(ExecRiskMismatch)`;随后 `view(1).max_order_notional = 10 000` |
| RSK-007.8 | `new`,账户 3(Sim)`force_max_cross = 0.010000001` | `Err(ExecRiskMismatch)`(模拟账户同一规则) |
| RSK-007.9 | `new`,账户 2(只有 A、B)`force_max_cross = 0.01`、`classes[1].rr_max_cross = 0.005`(a2、b2 属类别 1) | `Err(ExecRiskMismatch)`(最小值变为 0.005) |

### RSK-008 快照行结构的大小与字段偏移

- 规则:[快照行格式](README.md#快照行格式节-4171823草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:无运行时状态
- 步骤:
  1. 取各结构的 `size_of`、`align_of` 与 `offset_of`
- 期望:
  - `RiskRowHdr` 8 B,对齐 4:`kind` 0、`account` 1、`venue` 2、`_pad` 3、`inst` 4
  - `RiskAcctRow` 40 B:`day_pnl_base` 0、`loss_trigger` 8、`loss_quota` 16、`equity_sod` 24、`resumes_today` 32、`eq_flags` 33、`_pad` 34
  - `RiskBucketRow` 40 B:`tokens_10s` 0、`tokens_min` 8、`last_ens` 16、`backoff_until_ens` 24、`backoff_k` 32、`_pad` 36
  - `RiskLevRow` 8 B:`leverage` 0
  - `ReconRow` 16 B:`ignore_streak` 0、`warn_streak` 1、`_pad` 2、`last_diff` 8
  - `ReconGateRowHdr` 32 B:`report_id` 0、`disposed_mask` 8、`n_items` 16、`flags` 18、`_pad` 20、`view_seq` 24;其后每个 `ReconItemKey` 8 B
  - `RiskUsRow` 24 B:`us_up_seq` 0、`prev_view_seq` 8、`uflags` 16、`_pad` 17
  - `MarginRow` 96 B(行版本 4):`account` 0、`venue` 1、`level` 2、`mflags` 3、`_pad` 4、`report_seq` 8、`report_ens` 16、`reported_ratio` 24、`est_ratio` 32、`rep_liq_dist` 40、`est_liq_dist` 48、`l2_since_ens` 56、`deadline_ens` 64、`next_force_ens` 72、`mark_ens` 80、`stale_due_ens` 88
  - 以上结构的字段之和等于 `size_of`(无隐式填充)
- 实现:`crates/hft-risk/tests/layout.rs::rsk_008_snapshot_row_layouts`

### RSK-009 本 crate 定义的码值

- 规则:[快照行格式](README.md#快照行格式节-4171823草案实现时以代码与编译期断言为准)、[cap 记录](README.md#cap-记录)、[mode 模块](README.md#mode-模块唯一的模式状态)、[热路径:分类与检查](README.md#热路径分类与检查)
- 级别:table
- 里程碑:M2
- 前置:无运行时状态;各类型的 `as u8`/`from_u8`、位常量与 `RiskTimer` → `Timer{Risk, timer_id}` 的编码函数
- 实现:`crates/hft-risk/tests/layout.rs::rsk_009_code_values`

| 行 | 类型 | 期望 |
|---|---|---|
| RSK-009.1 | `RiskRowHdr.kind` | `Acct = 1`、`Bucket = 2`、`Recon = 3`、`ReconGate = 6`、`Lev = 7`、`UserStream = 8`;4、5 未分配 |
| RSK-009.2 | `MarginRow.level` / `mflags` | 0..3 = L0..L3;`TRUNCATED = 1`、`RATIO_L2 = 2`、`DEADLINE_L3 = 4`、`HAS_REPORT = 8` |
| RSK-009.3 | `RiskAcctRow.eq_flags` / `RiskUsRow.uflags` | `SOD_VALID = 1` / `PREV_US_DOWN = 1` |
| RSK-009.4 | `ReconGateRowHdr.flags` | `TRUNCATED = 1`、`INFLIGHT = 2`、`USERSTREAM_DOWN = 4`、`OVERFLOW = 8` |
| RSK-009.5 | `Origin` | `Strategy = 1`、`Exec = 2`、`Oms = 3`、`Command = 4` |
| RSK-009.6 | `CapLevel` | `None = 0`、`CloseOnly = 1`、`Hard = 3` |
| RSK-009.7 | `AccountCfg` cap 的 `params[0]` / `Kill` cap 的 `params[0]` | `MISMATCH = 1`、`LEVERAGE = 2` / `CANCEL_RESTING = 1`、`FLATTEN = 2` |
| RSK-009.8 | `RecoveryFlags` | `TAIL_LOST = 1`、`RECOVER_MISMATCH = 2`、`POISON = 4`、`FROM_SNAPSHOT = 8`、`CLEAN_DRAIN = 16` |
| RSK-009.9 | `RiskTimer::MarginStale{a: 1, v: 2}` / `MarginDeadline{1, 2}` / `MarginForce{1, 2}` | `timer_id` = 131 329 / 131 330 / 131 331(`kind \| account << 8 \| venue << 16`) |
| RSK-009.10 | `restore` 读到 `RiskRowHdr.kind = 4`、`kind = 9` 或 `MarginRow.level = 4` 的行 | `Err(RestoreError)` |

### RSK-010 分类表每一行

- Simultaneous 第二腿减仓提示按 Reduce 检查可减量;永续带 reduceOnly,现货不带;数量超可减量不标风险递减。由同一分类表测试覆盖。

- 规则:[分类](README.md#分类)
- 级别:table
- 里程碑:M2
- 前置:R0;`classify(draft, origin, pos, caps)`;"B" 行用 b1(`VenueCaps.reduce_only = true`),"A" 行用 a1(false);`pos` 写该 (account, inst) 的仓位(现货为该 group 库存)与已挂同向减仓量
- 实现:`crates/hft-risk/tests/classify.rs::rsk_010_classification_table`

| 行 | 意图 | `origin` | venue / 仓位 | 期望 `(class, risk_reducing, reduce_only)` |
|---|---|---|---|---|
| RSK-010.1 | `role = Second`,`HedgePlan.reduce_only = 0` | Oms | B | `(Hedge, true, false)` |
| RSK-010.2 | `role = SecondRetry`,`reduce_only = 0` | Oms | B | `(Hedge, true, false)` |
| RSK-010.3 | `role = Second`,`reduce_only = 1` | Oms | B | `(Hedge, true, true)` |
| RSK-010.4 | `role = Second`,`reduce_only = 1` | Oms | A | `(Hedge, true, false)` |
| RSK-010.5 | `role = SecondRetry`,`reduce_only = 1` | Oms | B | `(Hedge, true, true)` |
| RSK-010.6 | Simultaneous 第二腿(与第一腿同一事件) | Oms | B | `(Open, false, false)` |
| RSK-010.7 | `role = Reverse` | Oms | B / A | `(Hedge, true, true)` / `(Hedge, true, false)` |
| RSK-010.8 | `role = HedgeFix`(自动补腿、外来成交默认处理、残余修剪、Broken 自动有界平仓、`HedgeFix` 命令,五个子例) | Exec | B | 每个 `(Hedge, true, true)` |
| RSK-010.9 | 策略硬动作 `Flatten` | Strategy | B / A | `(Flatten, true, true)` / `(Flatten, true, false)` |
| RSK-010.10 | 执行层默认减仓 `reason = MarginStress`、`reason = Hard`(含 `SerialFallback` 腿)、`Kill{Account, flatten}` 清仓(三个子例) | Exec | B | 每个 `(Flatten, true, true)` |
| RSK-010.11 | `Reduce` Buy 0.3 | Strategy | B,pos −0.5,已挂同向减仓 0.1 | `(Reduce, true, true)` |
| RSK-010.12 | `Reduce` Buy 0.4(边界:= 0.5 − 0.1) | Strategy | 同上 | `(Reduce, true, true)` |
| RSK-010.13 | `Reduce` Buy 0.401(+1 lot) | Strategy | 同上 | `(Reduce, false, true)` |
| RSK-010.14 | 目标意图的减仓切片 Buy 0.4 | Exec | 同上 | `(Reduce, true, true)` |
| RSK-010.15 | `Reduce` Sell 0.5(现货,group 库存 0.5,已挂 0) | Strategy | A | `(Reduce, true, false)` |
| RSK-010.16 | `Reduce` Sell 0.5001(+1 lot) | Strategy | A,库存 0.5 | `(Reduce, false, false)` |
| RSK-010.17 | 冒烟单(`flags` 含 `SMOKE`) | Command | B | `(Open, false, false)` |
| RSK-010.18 | 普通开仓 | Strategy | B | `(Open, false, false)` |
| RSK-010.19 | 目标意图的增仓切片 | Exec | B | `(Open, false, false)` |
| RSK-010.20 | 执行层订单的 `class` 提示为 Open,`role = HedgeFix` | Exec | B | `(Hedge, true, true)`(提示不生效) |

- 补充负责人决定 SDK-164.4:覆盖组净量正负两向、归零边界、同向在途耗尽、反向/过量/平组拒绝、本腿 reduceOnly 与非策略来源不借用策略分类。实现:`crates/hft-risk/tests/classify.rs::rsk_010_unlinked_strategy_hedge_reduces_group`。

### RSK-011 SecondRetry 与 Second 的分类逐字段相同

- 规则:[分类](README.md#分类)
- 级别:prop
- 里程碑:M2
- 性质:任意草稿 d 中 `role = SecondRetry` 的 `classify` 结果与把 `role` 改为 `Second` 的结果逐字段相等
- 生成:`HedgePlan.reduce_only ∈ {0, 1}`、side、qty 1..10 000 lot、venue ∈ {A, B}、`OrderFlags` 任意子集(不含 `SMOKE`)、仓位 −10..10 BTC
- 参考:同一输入调两次 `classify` 比较
- 例数:PR 2 000
- 实现:`crates/hft-risk/tests/classify.rs::rsk_011_second_retry_equals_second`

### RSK-012 Amend 沿用原单分类,放大数量即失去风险递减

- 规则:[分类](README.md#分类)
- 级别:table
- 里程碑:M2
- 前置:R0;原单在 b1 上,数量 0.3
- 实现:`crates/hft-risk/tests/classify.rs::rsk_012_amend_classification`

| 行 | 原单分类 | 新数量 | 期望 |
|---|---|---|---|
| RSK-012.1 | `(Hedge, true, true)` | 0.3 | `(Hedge, true, true)` |
| RSK-012.2 | `(Hedge, true, true)` | 0.2 | `(Hedge, true, true)` |
| RSK-012.3 | `(Hedge, true, true)` | 0.301 | `(Hedge, false, true)` |
| RSK-012.4 | `(Open, false, false)` | 0.2 | `(Open, false, false)` |
| RSK-012.5 | `(Reduce, true, true)` | 0.3 | `(Reduce, true, true)` |

### RSK-013 分类的全局性质

- 规则:[分类](README.md#分类)
- 级别:prop
- 里程碑:M2
- 性质:`class = Open` ⇒ `risk_reducing = false`;`class ∈ {Hedge, Flatten}` ⇒ `risk_reducing = true`;`reduce_only = true` ⇒ 该 venue `VenueCaps.reduce_only = true`;`class = Reduce` 时 `risk_reducing ⇔ qty ≤ |pos| − 已挂同向减仓量`;同一输入两次调用结果相同
- 生成:分类表全部意图种类 × `origin` × venue ∈ {A, B} × qty 1..10 000 lot × 仓位 −10..10 × 已挂减仓 0..10
- 参考:分类表的逐行实现(`tests/model/classify_ref.rs`)
- 例数:PR 5 000
- 实现:`crates/hft-risk/tests/classify.rs::rsk_013_classification_properties`

### RSK-020 单腿 Open 在 R0 下通过全部检查,evaluate 不改状态

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[热路径:分类与检查](README.md#热路径分类与检查)(`evaluate` 只读)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. `evaluate(Open(1, g1, a1, Buy, 0.01 @ 50 000.00, Gtc))` 两次
  2. 对第二次的 `Admission` 调 `commit`
- 期望:
  - 两次都 `Ok`,两个 `Admission` 逐字段相等
  - 步骤 2 后快照节 17 中 (A, 1) 的 `RiskBucketRow.tokens_10s`、`tokens_min` 各比步骤 1 前少 1 个令牌,(1, a1) 的 inst 桶少 1 个;其余桶不变
- 实现:`crates/hft-risk/tests/chain.rs::rsk_020_single_open_passes`

### RSK-021 #0 结构、可交易与账户绑定

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#0)
- 级别:table
- 里程碑:M2
- 前置:R0;未另写的意图为 `Open(1, g1, a1, Buy, 0.01 @ 50 000.00, Gtc)`
- 实现:`crates/hft-risk/tests/chain.rs::rsk_021_structure_and_binding`

| 行 | 输入 | 期望 |
|---|---|---|
| RSK-021.1 | `qty = 0` | 拒 `INVALID` |
| RSK-021.2 | a1 `qty = 0.00015`(不是 lot 0.0001 的整数倍) | 拒 `INVALID` |
| RSK-021.3 | `Open(1, g1, b1, Buy, 0.01 @ 50 000.05, Gtc)`(不在 tick 0.10 上) | 拒 `INVALID` |
| RSK-021.4 | a1 状态 `ReduceOnly`,Open | 拒 `NOT_TRADABLE` |
| RSK-021.5 | a1 状态 `ReduceOnly`,`Reduce(1, g1, a1, Sell, 0.01)`(库存 0.01,风险递减) | 放行 |
| RSK-021.6 | a1 状态 `Halted`,执行层 `Flatten`(风险递减) | 拒 `NOT_TRADABLE` |
| RSK-021.7 | a1 状态 `Delisted`,Open | 拒 `NOT_TRADABLE` |
| RSK-021.8 | 账户 1 的 Open,`strategy = 2` | 拒 `NOT_BOUND` |
| RSK-021.9 | `origin = Exec`、`group = g1`、`role = HedgeFix` 的单(账户 1) | 放行 |
| RSK-021.10 | `origin = Exec`、`group = g2`、账户 1 | 拒 `NOT_BOUND` |
| RSK-021.11 | 冒烟单(`origin = Command`、`SMOKE`、PostOnly)账户 1、`strategy = 1`、a1 Buy 0.0002 @ 47 500.00 | 放行 |
| RSK-021.12 | 冒烟单账户 3(Sim)、`strategy = 3` | 拒 `INVALID` |
| RSK-021.13 | `qty = 0` 且 `strategy = 2` | 拒 `INVALID`(同一检查内按列出顺序) |
| RSK-021.14 | `qty = 0`,存在 `Kill{Global, hard}` | 拒 `INVALID`(#0 先于 #1) |

### RSK-022 #1 HARD 拦全部类别,账户 HARD 只拦该账户与以它为链接腿的单

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#1)、[断腿](README.md#断腿broken自动有界平仓l3-与-flatten)第 1 条
- 级别:table
- 里程碑:M2
- 前置:R0;账户 1 在 b1 持仓 −0.02(供 Reduce/Flatten);Hedge 的 `hedge_cap_qty = 0.01`;"执行层 `Flatten`"指 `Flatten(a, g, b1, Buy, 0.01 @ 50 005.0, origin = Exec, reason = MarginStress)`(账户 3 用 gs 且同样持仓 −0.02)
- 实现:`crates/hft-risk/tests/chain.rs::rsk_022_hard_blocks_all`

| 行 | cap | 意图 | 期望 |
|---|---|---|---|
| RSK-022.1 | `Kill{Global, hard}` | `Open(1, g1, b1, Buy, 0.01 @ 50 000.0)`;`Hedge(1, g1, b1, Sell, 0.01 @ 49 995.0)`;`Reduce(1, g1, b1, Buy, 0.01 @ 50 005.0)`;执行层 `Flatten`(同前)(四个子例) | 每个拒 `HARD` |
| RSK-022.2 | `Kill{Account(1), hard}` | 账户 1 的执行层 `Flatten` | 拒 `HARD` |
| RSK-022.3 | `Kill{Account(1), hard}` | `Open(2, g2, a1, Buy, 0.01 @ 50 000.00)` | 放行 |
| RSK-022.4 | `Kill{Account(1), hard}` | `LinkOpen(2, g2, a1, Buy, 0.01 @ 50 000.00; link = (1, B, b1, Sell, 0.01, 495))` | 拒 `HARD` |
| RSK-022.5 | `Kill{Account(1)}`(非 HARD) | 账户 1 的执行层 `Flatten` | 放行 |
| RSK-022.6 | `Kill{Global, hard}` | 账户 3(Sim)的执行层 `Flatten` | 拒 `HARD` |

### RSK-023 #2 ClockBad 只拦 Open

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#2)、[时钟](README.md#时钟)
- 级别:table
- 里程碑:M2
- 前置:R0;`apply(Clock{ok: false})` 之后;账户 1 在 b1 持仓 −0.02;策略 `Flatten` 与执行层 L3 `Flatten` 都是 b1 Buy 0.01 @ 50 005.0
- 实现:`crates/hft-risk/tests/chain.rs::rsk_023_clock_bad_blocks_open`

| 行 | 输入 | 期望 |
|---|---|---|
| RSK-023.1 | `Open(1, g1, a1, Buy, 0.01 @ 50 000.00)` | 拒 `CLOCK` |
| RSK-023.2 | `Hedge(1, g1, b1, Sell, 0.01 @ 49 995.0)`、`Reduce(1, g1, b1, Buy, 0.01 @ 50 005.0)`、策略 `Flatten`、执行层 L3 `Flatten`(四个子例) | 每个放行 |
| RSK-023.3 | 另有 `Kill{Account(1)}`,Open | 拒 `CLOCK`(#2 先于 #3) |
| RSK-023.4 | 再 `apply(Clock{ok: true})`,Open | 放行 |

### RSK-024 #3 按挡住意图的来源给出唯一的拒绝位

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#3 与"拒绝只带一位")、[cap 记录](README.md#cap-记录)、[有效模式](README.md#有效模式)
- 级别:table
- 里程碑:M2
- 前置:R0;每行先经 `apply` 建立所列 cap 或期望模式;未另写的意图为 `Open(1, g1, a1, Buy, 0.01 @ 50 000.00, Gtc)`
- 实现:`crates/hft-risk/tests/chain.rs::rsk_024_cap_reject_bits`

| 行 | 状态 | 意图 | 期望 |
|---|---|---|---|
| RSK-024.1 | `SetMode{(1, 1, g1), CloseOnly}` | 默认 | 拒 `CLOSE_ONLY` |
| RSK-024.2 | `Kill{Global}` | 默认 | 拒 `KILLED` |
| RSK-024.3 | `Kill{Account(1)}` | 默认;`Open(2, g2, a1, …)` | 拒 `KILLED`;放行 |
| RSK-024.4 | `Kill{Venue(B) × 1}` | 默认(a1);`Open(1, g1, b1, Buy, 0.01 @ 50 000.0)` | 放行;拒 `KILLED` |
| RSK-024.5 | `Draining` | 默认 | 拒 `DRAINING` |
| RSK-024.6 | `DailyLoss{1}` | 默认 | 拒 `DAILY_LOSS` |
| RSK-024.7 | `Recon{1, B}` | b1 Open;a1 Open | 拒 `RECON_DEVIATION`;放行 |
| RSK-024.8 | `UnknownTimeout{1, B}` | b1 Open | 拒 `CLOSE_ONLY` |
| RSK-024.9 | `Auto(Broken{1, g1})`(同时设 `HedgeFailure{1}`) | g1 的 a1 Open;g3 的 `Open(1, g3, a2, Buy, 0.1 @ 2 500.00)`;`Open(2, g2, a1, …)` | 拒 `CLOSE_ONLY`;拒 `CLOSE_ONLY`;放行 |
| RSK-024.10 | `MarginStress{1}`(L1) | 默认 | 拒 `CLOSE_ONLY` |
| RSK-024.11 | `Storage`(档位 1) | 默认 | 拒 `CLOSE_ONLY` |
| RSK-024.12 | `AccountCfg{1, B}` | b1 Open;a1 Open | 拒 `CLOSE_ONLY`;放行 |
| RSK-024.13 | `Recovery{1}` | 默认 | 拒 `CLOSE_ONLY` |
| RSK-024.14 | `Halted{s1, 1, g1}` | g1 Open;g3 Open | 拒 `CLOSE_ONLY`;放行 |
| RSK-024.15 | `Delisting{B, b1}` | b1 Open;`Open(1, g3, b2, Buy, 0.1 @ 2 500.0)` | 拒 `CLOSE_ONLY`;放行 |
| RSK-024.16 | `Delisting{B, inst = 0}` | b2 Open | 拒 `CLOSE_ONLY` |
| RSK-024.17 | `Kill{Account(1), flatten}`(清仓锁) | 默认 | 拒 `KILLED` |
| RSK-024.18 | `Draining` + `Kill{Global}` + `DailyLoss{1}` | 默认 | 拒 `DRAINING` |
| RSK-024.19 | `Kill{Account(1)}` + `DailyLoss{1}` | 默认 | 拒 `KILLED` |
| RSK-024.20 | `DailyLoss{1}` + `Recon{1, B}` | b1 Open | 拒 `DAILY_LOSS` |
| RSK-024.21 | `Recon{1, B}` + `MarginStress{1}` | b1 Open | 拒 `RECON_DEVIATION` |
| RSK-024.22 | `SetMode{(1, 1, g1), Shadow}`(cell flat,有效模式 Shadow) | 默认 | 放行(Shadow 由 sequencer 的模式门处理,#3 只查 CloseOnly) |
| RSK-024.23 | `MarginStress{1}`,b1 持仓 −0.5、已挂同向减仓 0.1 | 策略 `Reduce(1, g1, b1, Buy, 0.401 @ 50 005.0)`(超量,`risk_reducing = false`) | 拒 `CLOSE_ONLY`(#3 先于 #7) |

### RSK-025 任何非 HARD 的 cap 与条件门都不拦风险递减单

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#2–#4 只拦 Open 与无 `RISK_REDUCING` 的单)、[cap 记录](README.md#cap-记录)(除 HARD 外所有 cap 只拦 Open)
- 级别:table
- 里程碑:M2
- 前置:R0;账户 1 在 b1 持仓 −0.02;每行先建立所列状态,再对三个意图各调一次 `evaluate`:`Hedge(1, g1, b1, Sell, 0.01 @ 49 995.0, hedge_cap_qty 0.01)`、`Reduce(1, g1, b1, Buy, 0.01 @ 50 005.0)`、执行层 `Flatten(1, g1, b1, Buy, 0.01 @ 50 005.0, reason = MarginStress)`
- 实现:`crates/hft-risk/tests/chain.rs::rsk_025_risk_reducing_passes_caps`

| 行 | 状态 | 期望(三个意图) |
|---|---|---|
| RSK-025.1 | `Kill{Global}` | 都放行 |
| RSK-025.2 | `Kill{Account(1), flatten}` | 都放行 |
| RSK-025.3 | `Kill{Venue(B) × 1}` | 都放行 |
| RSK-025.4 | `Draining` | 都放行 |
| RSK-025.5 | `ClockBad` | 都放行 |
| RSK-025.6 | `DailyLoss{1}`、`Recon{1, B}`、`UnknownTimeout{1, B}`、`Recovery{1}`、`Halted{s1, 1, g1}`(五个子例) | 都放行 |
| RSK-025.7 | `Broken{1, g1}` + `HedgeFailure{1}` | 都放行 |
| RSK-025.8 | `MarginStress{1}`(L3)、`Storage`(档位 2)、`AccountCfg{1, B}`(三个子例) | 都放行 |
| RSK-025.9 | `Delisting{B, b1}` | 都放行 |
| RSK-025.10 | 期望模式 CloseOnly | 都放行 |
| RSK-025.11 | `venue_ready(B, 1) = false`、`group_has_unknown(1, g1) = true`、`on_legs` 报 `naked_ratio 0.9, naked_notional 1 000`(同时) | 都放行 |

### RSK-026 #4 条件门只拦 Open

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#4)
- 级别:table
- 里程碑:M2
- 前置:R0
- 实现:`crates/hft-risk/tests/chain.rs::rsk_026_condition_gates`

| 行 | 状态 | 意图 | 期望 |
|---|---|---|---|
| RSK-026.1 | `venue_ready(A, 1) = false` | `Open(1, g1, a1, Buy, 0.01 @ 50 000.00)` | 拒 `VENUE_NOT_READY` |
| RSK-026.2 | 同上,a1 库存 0.01 | `Reduce(1, g1, a1, Sell, 0.01 @ 49 999.00)`、`Hedge(1, g1, a1, Buy, 0.01 @ 50 001.00)`、执行层 `Flatten(1, g1, a1, Sell, 0.01 @ 49 999.00)`(三个子例) | 每个放行 |
| RSK-026.3 | `group_has_unknown(1, g1) = true`,`unknown_blocks_open = true` | 同 .1 | 拒 `UNKNOWN_PENDING` |
| RSK-026.4 | 同上,`unknown_blocks_open = false` | 同 .1 | 放行 |
| RSK-026.5 | `group_has_unknown(1, g1) = true` | `Open(1, g3, a2, Buy, 0.1 @ 2 500.00)` | 放行 |
| RSK-026.6 | `venue_ready(B, 1) = false` | `LinkOpen(1, g1, a1, Buy, 0.01 @ 50 000.00; link = (1, B, b1, Sell, 0.01, 495))` | 拒 `VENUE_NOT_READY` |
| RSK-026.7 | `venue_ready(A, 1) = false` 且 `group_has_unknown(1, g1) = true` | 同 .1 | 拒 `VENUE_NOT_READY` |

### RSK-027 #5 报价健康按连接判定,盘口安静不算过期

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#5、报价新鲜度 = 连接健康)
- 级别:table
- 里程碑:M2
- 前置:R0(`conn_liveness_ms = 2 000`,`tsc_per_ms = 1 000 000`);未另写的意图为 `Open(1, g1, a1, Buy, 0.01 @ 50 000.00, Gtc)`;b1 持仓 −0.02
- 实现:`crates/hft-risk/tests/chain.rs::rsk_027_quote_health`

| 行 | a1 / b1 的 BBO | 意图 | 期望 |
|---|---|---|---|
| RSK-027.1 | a1 买一为空 | 默认 | 拒 `NO_MID` |
| RSK-027.2 | a1 `conn_up = false` | 默认 | 拒 `STALE_MID` |
| RSK-027.3 | a1 `resync = true` | 默认 | 拒 `STALE_MID` |
| RSK-027.4 | a1 `now.tsc − conn_live_tsc = 2 000 000 000` | 默认 | 放行 |
| RSK-027.5 | 同上 `= 2 000 000 001` | 默认 | 拒 `STALE_MID` |
| RSK-027.6 | 同上 `= 1 999 999 999` | 默认 | 放行 |
| RSK-027.7 | a1 `rx_tsc = now − 10 s`(盘口 10 s 未变),`conn_live_tsc = now − 500 ms` | 默认 | 放行 |
| RSK-027.8 | b1 `conn_up = false` | `Hedge(1, g1, b1, Sell, 0.01 @ 49 995.0)` | 拒 `STALE_MID` |
| RSK-027.9 | b1 `resync = true` | 执行层 L3 `Flatten(1, g1, b1, Buy, 0.01 @ 50 005.0)` | 拒 `STALE_MID` |
| RSK-027.10 | a1 买一为空且 `conn_up = false` | 默认 | 拒 `NO_MID` |
| RSK-027.11 | `conn_liveness_ms = 1 000`(`set_params`),a1 静默 1 500 ms | 默认 | 拒 `STALE_MID` |

### RSK-028 #5 价格带与越价上限按 inst 所属类别取值

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#5)、[配置](README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)(`l3_max_cross`)
- 级别:table
- 里程碑:M2
- 前置:R0;b1 持仓 −0.02、a1 库存 0.02(供 Reduce/Flatten);Hedge 的 `hedge_cap_qty` 等于其数量
- 实现:`crates/hft-risk/tests/chain.rs::rsk_028_price_band_per_inst`

| 行 | 参数 | 意图 | 期望 |
|---|---|---|---|
| RSK-028.1 | — | `Open(1, g1, a1, Buy, 0.01 @ 51 500.00, Gtc)`(偏离 = 0.03 × mid) | 放行 |
| RSK-028.2 | — | 同上 @ 51 500.01 | 拒 `PX_BAND` |
| RSK-028.3 | — | `Open(…, a1, Sell, 0.01 @ 48 500.00)`;@ 48 499.99 | 放行;拒 `PX_BAND` |
| RSK-028.4 | — | `Open(…, a1, Buy, 0.01 @ 50 251.00, Ioc)`(越过 ask 50 001.00 的比例 0.0049999) | 放行 |
| RSK-028.5 | — | 同上 @ 50 251.01(0.0050001) | 拒 `PX_BAND` |
| RSK-028.6 | — | `Open(…, a1, Sell, 0.01 @ 49 749.01, Ioc)`;@ 49 749.00 | 放行;拒 `PX_BAND` |
| RSK-028.7 | — | `Open(…, a1, Buy, 0.01 @ 50 300.00, Gtc)`(越过 ask 0.006,GTC 不查 `ioc_max_cross`) | 放行 |
| RSK-028.8 | — | `Hedge(1, g1, a1, Buy, 0.01 @ 50 500.00)`(越过 mid 0.01) | 放行 |
| RSK-028.9 | — | 同上 @ 50 500.01 | 拒 `PX_BAND` |
| RSK-028.10 | — | `Hedge(…, b1, Sell, 0.01 @ 49 500.0)`;@ 49 499.9 | 放行;拒 `PX_BAND` |
| RSK-028.11 | — | `Hedge(…, a1, Buy, 0.01 @ 40 000.00)`(有利方向,不查 `max_px_dev`) | 放行 |
| RSK-028.12 | — | `Hedge(1, g3, a2, Buy, 0.1 @ 2 550.00)`(类别 1,`rr_max_cross 0.02`);@ 2 550.01 | 放行;拒 `PX_BAND` |
| RSK-028.13 | — | `Open(1, g3, a2, Buy, 0.1 @ 2 625.00, Gtc)`(类别 1,`max_px_dev 0.05`);@ 2 625.01 | 放行;拒 `PX_BAND` |
| RSK-028.14 | `l3_max_cross = 0` | 执行层 `Flatten(1, g1, b1, Buy, 0.01 @ 50 750.0, reason = MarginStress)`(越过 mid 0.015) | 拒 `PX_BAND` |
| RSK-028.15 | `l3_max_cross = 0.02` | 同上 | 放行 |
| RSK-028.16 | `l3_max_cross = 0.02` | 同上 @ 51 000.0(0.02);@ 51 000.1 | 放行;拒 `PX_BAND` |
| RSK-028.17 | `l3_max_cross = 0.005` | 同 .14(上限取 max(0.01, 0.005) = 0.01) | 拒 `PX_BAND` |
| RSK-028.18 | `l3_max_cross = 0.02` | 策略 `Flatten(1, g1, b1, Buy, 0.01 @ 50 750.0)` | 拒 `PX_BAND` |
| RSK-028.19 | `l3_max_cross = 0.02` | 执行层 `role = HedgeFix` 的 Hedge,b1 Buy 0.01 @ 50 750.0 | 拒 `PX_BAND` |
| RSK-028.20 | — | 策略 `Reduce(1, g1, b1, Buy, 0.01 @ 50 600.0)`(越过 mid 0.012) | 拒 `PX_BAND` |

### RSK-029 带链接腿的 Open 另查第二腿报价健康

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#5 `LINK_STALE`、链接腿规则)
- 级别:table
- 里程碑:M2
- 前置:R0;意图 L = `LinkOpen(1, g1, a1, Buy, 0.01 @ 50 000.00, Gtc; link = (1, B, b1, Sell, 0.01, 495))`
- 实现:`crates/hft-risk/tests/chain.rs::rsk_029_link_quote_health`

| 行 | BBO 状态 | 意图 | 期望 |
|---|---|---|---|
| RSK-029.1 | R0 | L | 放行 |
| RSK-029.2 | b1 `conn_up = false` | L | 拒 `LINK_STALE` |
| RSK-029.3 | b1 `resync = true` | L | 拒 `LINK_STALE` |
| RSK-029.4 | b1 静默 2 000 ms;2 000 ms + 1 ns | L | 放行;拒 `LINK_STALE` |
| RSK-029.5 | b1 `rx_tsc = now − 10 s`,`conn_live_tsc = now − 500 ms` | L | 放行 |
| RSK-029.6 | a1 与 b1 都 `conn_up = false` | L | 拒 `STALE_MID`(先查本 inst) |
| RSK-029.7 | b1 买一为空、连接健康 | L | 放行(第二腿只查 `quote_live`) |
| RSK-029.8 | a1 `conn_up = false` | Simultaneous 第二腿 `LinkOpen(1, g1, b1, Sell, 0.01 @ 49 995.0; link = (1, A, a1, Buy, 0.01, 505))` | 拒 `LINK_STALE` |

### RSK-030 hedge_px_limits 与 #5 同值同源

- 规则:[热路径:分类与检查](README.md#热路径分类与检查)(`hedge_px_limits`)、[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(引擎单的限价与名义)
- 级别:prop
- 里程碑:M2
- 性质:对任意 (account, venue, inst):`hedge_px_limits(a, v, i) == HedgePxLimits{conn_liveness_ms, rr_max_cross: view_for(a, i).rr_max_cross}`;报价健康时 `Hedge` 买单在 #5 放行 ⇔ `(px − mid) / mid ≤` 该值,卖单 ⇔ `(mid − px) / mid ≤` 该值
- 生成:`conn_liveness_ms` 1 000..10 000;`classes` 1..8 个,三项各 0.0001..0.1;`risk_class` 随机映射 R0 的五个 inst;mid 1.00..100 000.00;px = mid × (1 ± 0..0.2) 取到 tick
- 参考:按 README 公式的参考判定 `tests/model/band_ref.rs`
- 例数:PR 2 000
- 实现:`crates/hft-risk/tests/chain_props.rs::rsk_030_hedge_px_limits_same_source`

### RSK-031 名义 = qty × 限价 × 换算率,向上取整到 1e-6

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#6 名义口径)、[热路径:分类与检查](README.md#热路径分类与检查)(`Risk::notional`)
- 级别:table
- 里程碑:M2
- 前置:`Risk::notional(qty, px, quote_rate)`,qty 指数 −8、px 指数 −2
- 实现:`crates/hft-risk/tests/chain.rs::rsk_031_notional_definition`

| 行 | `(qty, px, quote_rate)` | 期望 |
|---|---|---|
| RSK-031.1 | `(0.01, 50 000.00, 1)` | 500.000000 |
| RSK-031.2 | `(0.0001, 50 000.01, 1)` | 5.000001 |
| RSK-031.3 | `(0.00000001, 0.01, 1)`(精确值 1e-10) | 0.000001(向上取整) |
| RSK-031.4 | `(0.01, 50 000.00, 0.999)` | 499.500000 |
| RSK-031.5 | `(1, 1.00, 0.333333333)` | 0.333334 |
| RSK-031.6 | `(1, 50 000.01, 1)` 与 `(1, 50 000.00, 1)` | 50 000.010000 与 50 000.000000(限价更高名义更高) |
| RSK-031.7 | `(1 000 000, 1 000 000.00, 1)` | 1 000 000 000 000.000000(i128 中间积不溢出) |

### RSK-032 Open 的四级名义上限含在途

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#6)、[配置](README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)(`max_account_notional` 与权益)
- 级别:table
- 里程碑:M2
- 前置:R0;意图为账户 1、g1、a1 的 Open,`IntentView.notional` 直接取行中的值;`held_notional`/`open_notional` 按 `Scope::{Group(g1), Account, Venue(A)}` 给行中的值,未写为 0
- 实现:`crates/hft-risk/tests/chain.rs::rsk_032_open_notional_levels`

| 行 | 视图 | `notional` | 期望 |
|---|---|---|---|
| RSK-032.1 | — | 10 000 | 放行 |
| RSK-032.2 | — | 10 000.000001 | 拒 `NOTIONAL_ORDER` |
| RSK-032.3 | group 持有 45 000 | 5 000 | 放行 |
| RSK-032.4 | group 持有 45 000、group 在途 0.000001 | 5 000 | 拒 `NOTIONAL_GROUP` |
| RSK-032.5 | 账户持有 95 000 | 5 000 | 放行 |
| RSK-032.6 | 账户持有 95 000.000001 | 5 000 | 拒 `NOTIONAL_ACCOUNT` |
| RSK-032.7 | `equity = {90 000, valid}`,账户持有 85 000;85 000.000001 | 5 000 | 放行;拒 `NOTIONAL_ACCOUNT` |
| RSK-032.8 | `equity = {1 000, valid = false}`,账户持有 95 000 | 5 000 | 放行(只用绝对值 100 000) |
| RSK-032.9 | venue A 持有 75 000;75 000.000001 | 5 000 | 放行;拒 `NOTIONAL_VENUE` |
| RSK-032.10 | group 持有 45 000 | 10 000.000001 | 拒 `NOTIONAL_ORDER`(四级按列出顺序) |

### RSK-033 Hedge 的数量与名义上限、Reduce/Flatten 的单笔上限

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#6、`HEDGE_LIMIT` 是终局拒绝)
- 级别:table
- 里程碑:M2
- 前置:R0;Reduce 与 Flatten 行 b1 持仓 −1,Hedge 行 b1 持仓 0(持仓 −1 时卖出对冲会先超 #7 的 `max_pos_qty 1`);`IntentView.notional` 未写时按 `qty × px`
- 实现:`crates/hft-risk/tests/chain.rs::rsk_033_hedge_and_reduce_limits`

| 行 | 意图 | 期望 |
|---|---|---|
| RSK-033.1 | `Hedge(1, g1, b1, Sell, 0.01 @ 49 995.0, hedge_cap_qty 0.01)` | 放行 |
| RSK-033.2 | 同上 qty 0.011(+1 lot) | 拒 `HEDGE_LIMIT` |
| RSK-033.3 | `Hedge(…, Sell, 0.4 @ 50 000.0, hedge_cap_qty 0.4)`(名义 20 000) | 放行 |
| RSK-033.4 | 同上,`notional = 20 000.000001` | 拒 `HEDGE_LIMIT` |
| RSK-033.5 | `Hedge(…, Sell, 0.3 @ 50 000.0, hedge_cap_qty 0.3)`(名义 15 000 > `max_order_notional`) | 放行 |
| RSK-033.6 | 执行层 `role = HedgeFix` 的 Hedge,qty 0.011、`hedge_cap_qty 0.01` | 拒 `HEDGE_LIMIT` |
| RSK-033.7 | `Reduce(1, g1, b1, Buy, 0.2 @ 50 000.0)`(名义 10 000) | 放行 |
| RSK-033.8 | 同上,`notional = 10 000.000001` | 拒 `NOTIONAL_ORDER` |
| RSK-033.9 | 执行层 `Flatten`,`notional = 10 000.000001` | 拒 `NOTIONAL_ORDER` |
| RSK-033.10 | `Reduce(…, Buy, 0.2 @ 50 000.0)`,账户持有 200 000(超过账户上限) | 放行(Reduce 只查单笔) |
| RSK-033.11 | 同 .2 连续 `evaluate` 两次 | 两次都拒 `HEDGE_LIMIT`(结果确定;不再试由 sequencer 规则保证,见 hft-sequencer 意图管线) |

### RSK-034 #7 仓位:Open 含全部在途,Hedge 与 Flatten 只扣即时在途

- Simultaneous 整对准入补充:任一 Reduce/Flatten 腿方向与实际持仓同向时拒 REDUCE_INVALID,两腿正确反向时按可减量校验;实现并入 `rsk_034_position_inflight`。

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#7、#7 只扣即时在途的理由)
- 级别:table
- 里程碑:M2
- 前置:R0(g1 `max_pos_qty = 1`),账户 1 的 `max_order_notional`、`max_reduce_notional` 与 `max_hedge_notional` 改为 50 000(否则 0.3、0.4 的行先被 #6 拒);视图按行设置 `pos`、`open_qty`(即时 + 挂单)、`open_qty_immediate`、`pending_reduce`(即时 + 挂单)、`pending_reduce_immediate`,未写为 0;未写限价的 b1 意图:买 @ 50 005.0、卖 @ 49 995.0(都在价格带与越价上限之内)
- 实现:`crates/hft-risk/tests/chain.rs::rsk_034_position_inflight`

| 行 | 视图(b1,账户 1) | 意图 | 期望 |
|---|---|---|---|
| RSK-034.1 | pos 0.5,`open_qty(Buy)` 0.4 | `Open(1, g1, b1, Buy, 0.1 @ 50 000.0)` | 放行(0.5 + 0.4 + 0.1 = 1) |
| RSK-034.2 | 同上 | 同上 qty 0.101 | 拒 `POSITION` |
| RSK-034.3 | pos −0.5,`open_qty(Sell)` 0.4 | `Open(…, b1, Sell, 0.1)`;qty 0.101 | 放行;拒 `POSITION` |
| RSK-034.4 | pos 0.5 | `Open(…, b1, Sell, 0.3)` | 放行(0 + 0.3 − 0.5 < 1) |
| RSK-034.5 | pos 0.5,`open_qty_immediate(Buy)` 0.2,`open_qty(Buy)` 0.5 | `Hedge(…, b1, Buy, 0.3, HedgePlan.reduce_only = 0, hedge_cap_qty 0.3)` | 放行(只计即时:1.0;含挂单会是 1.3) |
| RSK-034.6 | 同上 | 同上 qty 0.301 | 拒 `POSITION` |
| RSK-034.7 | pos 0.5,`pending_reduce_immediate(Sell)` 0.1,`pending_reduce(Sell)` 0.5 | reduceOnly 的 `Hedge(…, b1, Sell, 0.4, reduce_only = 1)` | 放行 |
| RSK-034.8 | 同上 | 同上 qty 0.401 | 拒 `REDUCE_INVALID` |
| RSK-034.9 | 同上 | 执行层 `Flatten(…, b1, Sell, 0.4)`;qty 0.401 | 放行;拒 `REDUCE_INVALID` |
| RSK-034.10 | 同上(策略同向 reduce-only GTC 0.4 已 Ack) | 策略 `Reduce(…, b1, Sell, 0.001)` | 拒 `REDUCE_INVALID`(含挂单:0.001 > 0.5 − 0.5) |
| RSK-034.11 | pos 0.5,`pending_reduce(Sell)` 0.1 | 策略 `Reduce(…, b1, Sell, 0.4)` | 放行 |
| RSK-034.12 | 账户 1 b1 pos −0.9,`open_qty(Sell)` 0.05 | `LinkOpen(1, g1, a1, Buy, 0.05 @ 50 000.00; link = (1, B, b1, Sell, 0.05, 2 475))`;`link_qty` 0.051 | 放行;拒 `POSITION` |
| RSK-034.13 | `default_group.max_pos_qty = 0.001`;`= 0.0001` | 冒烟单 a1 Buy 0.0002 @ 47 500.00 | 放行;拒 `POSITION` |

### RSK-035 #7b 第二腿名义与可负担性

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#7b、链接腿规则)、[热路径:分类与检查](README.md#热路径分类与检查)(`on_account_cfg`、`leverage`)
- 级别:table
- 里程碑:M2
- 前置:R0(`funds_buffer = 0.05`,b1 杠杆 3.00);现货买入腿用 `LinkOpen(1, g1, b1, Sell, 0.01 @ 50 000.0; link = (1, A, a1, Buy, 0.01, 505))`;现货卖出腿用 `LinkOpen(1, g1, b1, Buy, 0.01 @ 50 000.0; link = (1, A, a1, Sell, 0.01, 495))`;永续腿用 `LinkOpen(1, g1, a1, Buy, 0.01 @ 50 000.00; link = (1, B, b1, Sell, 0.01, 495))`
- 备注:OMS 在途买单名义与在途开仓保证金占用的扣减发生在视图适配器(hft-sequencer)与 OMS `InflightBook`,由其测试规格验证;本用例直接给出扣减后的 `avail_quote`/`avail_margin`
- 实现:`crates/hft-risk/tests/chain.rs::rsk_035_link_affordability`

| 行 | 状态 | 意图 | 期望 |
|---|---|---|---|
| RSK-035.1 | `avail_quote(1, A, USDT) = 530.25`(= 505 × 1.05) | 现货买入腿 | 放行 |
| RSK-035.2 | `avail_quote = 530.249999` | 现货买入腿 | 拒 `LINK_FUNDS` |
| RSK-035.3 | `avail_quote = None` | 现货买入腿 | 拒 `LINK_FUNDS` |
| RSK-035.4 | `link_notional = 20 000` | 现货买入腿 | 放行 |
| RSK-035.5 | `link_notional = 20 000.000001` | 现货买入腿 | 拒 `HEDGE_LIMIT` |
| RSK-035.6 | `funds_buffer = 0`,`avail_quote = 505`;504.999999 | 现货买入腿 | 放行;拒 `LINK_FUNDS` |
| RSK-035.7 | `pos(1, a1) = 0.01`,`pending_reduce(1, a1, Sell) = 0` | 现货卖出腿 | 放行 |
| RSK-035.8 | `pos = 0.01`,`pending_reduce = 0.0001` | 现货卖出腿 | 拒 `LINK_FUNDS` |
| RSK-035.9 | `avail_margin(1, B) = 173.25`(= 495 / 3 × 1.05) | 永续腿 | 放行 |
| RSK-035.10 | `avail_margin = 173.249999` | 永续腿 | 拒 `LINK_FUNDS` |
| RSK-035.11 | 最近报告给 b1 `leverage = 0` | 永续腿 | 拒 `LINK_FUNDS` |
| RSK-035.12 | 最近一条非 `PARTIAL` 报告只列 b2 | 永续腿 | 拒 `LINK_FUNDS` |
| RSK-035.13 | 最近报告带 `LEV_TRUNCATED` 且未列 b1 | 永续腿 | 拒 `LINK_FUNDS` |
| RSK-035.14 | `avail_margin = None` | 永续腿 | 拒 `LINK_FUNDS` |
| RSK-035.15 | refdata `InstMeta.max_leverage(b1) = 20`,`avail_margin = 173.25`;`= 26.0` | 永续腿 | 放行;拒 `LINK_FUNDS`(只用账户实际杠杆 3.00) |
| RSK-035.16 | R0 之后一条 `PARTIAL` 报告只给 b2 `leverage = 5.00`,`avail_margin = 173.25` | 永续腿 | 放行(b1 仍为 3.00) |
| RSK-035.17 | `avail_margin(1, B) = None` | `Hedge(1, g1, b1, Sell, 0.01 @ 49 995.0)` | 放行(#7b 只对链接 Open) |

### RSK-036 #8 令牌:非风险递减单只用预留以上的令牌

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#8)、[令牌桶](README.md#令牌桶)
- 级别:table
- 里程碑:M2
- 前置:R0((A, 1) 两窗口容量 100 / 600,`rr_reserve = 0.20` → 预留 20 / 120;inst 桶 1 000);"k 次提交"= 在同一 engine_ns 对 a1 的 Open 连续 `evaluate` + `commit` k 次;非风险递减单 N = `Open(1, g1, a1, Buy, 0.0001 @ 50 000.00)`,风险递减单 H = `Hedge(1, g1, a1, Buy, 0.0001 @ 50 001.00, hedge_cap_qty 0.0001)`
- 实现:`crates/hft-risk/tests/bucket.rs::rsk_036_rr_reserve`

| 行 | 前置操作 | 意图 | 期望 |
|---|---|---|---|
| RSK-036.1 | 79 次提交(10 s 桶余 21) | N | 放行 |
| RSK-036.2 | 80 次提交(余 20) | N;H | 拒 `RATE`;放行 |
| RSK-036.3 | 99 次提交(余 1) | H | 放行 |
| RSK-036.4 | 100 次提交(余 0) | H | 拒 `RATE` |
| RSK-036.5 | (A, 1) 10 s 限额改为 1 000;479 次提交(1 min 桶余 121);480 次 | N | 放行;拒 `RATE` |
| RSK-036.6 | `inst_bucket_per_s = 5`;对 a1 5 次提交 | H(a1);`Open(1, g3, a2, Buy, 0.001 @ 2 500.00)` | 拒 `RATE`;放行(inst 桶按 (account, inst),无预留) |
| RSK-036.7 | `on_rate_limit_hit(A, 1)`,1 ms 后 | N;H | 拒 `RATE_BACKOFF`;拒 `RATE_BACKOFF` |
| RSK-036.8 | `on_rate_limit_hit(B, 1)` | `LinkOpen(1, g1, a1, Buy, 0.01 @ 50 000.00; link = (1, B, b1, Sell, 0.01, 495))` | 拒 `RATE_BACKOFF` |
| RSK-036.9 | (B, 1) 上 79 次提交(余 21) | 同 .8 的链接 Open,`evaluate` + `commit` 后再 `evaluate` 一次 | 两次都放行((B, 1) 只查不扣,仍余 21) |
| RSK-036.10 | (B, 1) 上 80 次提交(余 20) | 同 .8 | 拒 `RATE` |
| RSK-036.11 | 账户 3(Sim)在 (A, 3) 上 80 次提交 | `Open(3, gs, a1, Buy, 0.0001 @ 50 000.00)` | 拒 `RATE`(模拟账户用同一限额) |
| RSK-036.12 | (A, 1) 上 80 次提交(余 20),另 `on_rate_limit_hit(B, 1)` | 同 .8 | 拒 `RATE`(本 venue 先于链接腿 venue) |
| RSK-036.13 | (B, 1) 上 80 次提交(余 20),另 `on_rate_limit_hit(B, 1)` | 同 .8 | 拒 `RATE_BACKOFF`(同一 venue 退避先于令牌) |
| RSK-036.14 | (A, 1) 上 `on_rate_limit_hit(A, 1)`,另 (B, 1) 上 80 次提交 | 同 .8 | 拒 `RATE_BACKOFF` |

### RSK-037 令牌按 engine_ns 惰性补充、不超过容量,容量扣除 rate_headroom

- 规则:[令牌桶](README.md#令牌桶)
- 级别:table
- 里程碑:M2
- 前置:R0;H 同 RSK-036;"耗尽"= 在 t 时刻对 a1 连续提交 H 直到被拒 `RATE`
- 实现:`crates/hft-risk/tests/bucket.rs::rsk_037_lazy_refill`

| 行 | 操作 | 期望 |
|---|---|---|
| RSK-037.1 | t = T0 耗尽(100 次);在 T0 + 99 999 999 ns `evaluate(H)` | 拒 `RATE`(补充 0.99999999 个) |
| RSK-037.2 | 同上,在 T0 + 100 000 000 ns | 放行(补充 1 个) |
| RSK-037.3 | T0 耗尽,T0 + 1 h 在同一时刻连续提交 H | 前 100 次放行,第 101 次拒 `RATE` |
| RSK-037.4 | `set_params(scope = *, rate_headroom = 0.1)`,T0 连续提交 H | 前 90 次放行,第 91 次拒 `RATE` |
| RSK-037.5 | (A, 1) 10 s 限额 1 000、1 min 限额 600;T0 提交 600 次;T0 + 99 999 999 ns;T0 + 100 000 000 ns | 第 601 次拒 `RATE`;拒 `RATE`;放行(1 min 桶每 100 ms 补 1 个) |

### RSK-038 限速命中后清零两个窗口桶并指数退避

- 规则:[令牌桶](README.md#令牌桶)(退避时长与 k 的规则)
- 级别:unit
- 里程碑:M2
- 前置:R0(`backoff_base_ms = 1 000`、`backoff_max_ms = 8 000`);(A, 1) 从未退避
- 步骤:
  1. `on_rate_limit_hit(A, 1)` 于 T0
  2. `evaluate(H)` 于 T0 + 999 999 999 ns;`view_for(1, a1)`、`view_for(2, a1)`、`view_for(1, b1)`
  3. 于 T0 + 1 s:`evaluate(H)`、`evaluate(N)`(H、N 同 RSK-036)
  4. 依次于 T0 + 1.5 s、T0 + 4 s、T0 + 9 s、T0 + 18 s 各命中一次
  5. 于 T0 + 34 s 命中一次(距上次退避结束 T0 + 26 s 恰为 8 000 ms)
  6. 于 T0 + 35 s + 7 999 999 999 ns 命中一次
- 期望:
  - 步骤 1:`backoff_until_ens = T0 + 1 000 ms`,`backoff_k = 0`,(A, 1) 两个窗口桶为 0;返回 `Note{RateLimitHit}`(P2,`v[0] = 2 | 1 << 8 = 258`,`v[1] = 0`,`v[2] = T0 + 1 000 ms`)
  - 步骤 2:拒 `RATE_BACKOFF`;`rate_backoff` 分别为 true、false、false
  - 步骤 3:H 放行(1 s 内补充 10 个);N 拒 `RATE`(10 ≤ 预留 20)
  - 步骤 4:`backoff_k` 依次 1、2、3、4,退避结束依次为 T0 + 3.5 s、T0 + 8 s、T0 + 17 s、T0 + 26 s(第四次时长 min(16 000, 8 000) = 8 000 ms)
  - 步骤 5:`backoff_k = 0`,退避结束 T0 + 35 s
  - 步骤 6:`backoff_k = 1`,退避结束 T0 + 35 s + 7 999 999 999 ns + 2 000 ms
  - 全程 (A, 2)、(B, 1) 的桶与退避不变
- 实现:`crates/hft-risk/tests/bucket.rs::rsk_038_backoff_exponential`

### RSK-039 只有 commit 扣令牌

- 规则:[热路径:分类与检查](README.md#热路径分类与检查)(`evaluate` 只读、`commit`;Shadow 与被拒意图从不调用 `commit`)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. 在 T0 对 H 调 `evaluate` 1 000 次,不调 `commit`
  2. 在 T0 对 H 连续 `evaluate` + `commit`
- 期望:
  - 步骤 1 全部放行
  - 步骤 2 前 100 次放行、第 101 次拒 `RATE`(步骤 1 未扣任何令牌)
- 实现:`crates/hft-risk/tests/bucket.rs::rsk_039_only_commit_consumes`

### RSK-040 桶与退避存 engine_ns,重启后原样有效、停机不补充

- 规则:[令牌桶](README.md#令牌桶)、[热路径:分类与检查](README.md#热路径分类与检查)(时间)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. T0 耗尽 (A, 1)(100 次 H);`on_rate_limit_hit(B, 1)` 于 T0;`snapshot`
  2. 新进程语义:`restore`(新运行的 TSC 从 0 起,`tsc_hz` 相同),`set_run_start(T0 + 50 ms)`(停机 10 min 不计)
  3. 于 engine_ns T0 + 50 ms `evaluate(H)`;于 T0 + 100 ms `evaluate(H)`
  4. 于 T0 + 999 ms 对 b1 的 Hedge `evaluate`;于 T0 + 1 000 ms 再一次
- 期望:
  - 步骤 3:拒 `RATE`;放行
  - 步骤 4:拒 `RATE_BACKOFF`;放行
- 实现:`crates/hft-risk/tests/bucket.rs::rsk_040_bucket_survives_restart`

### RSK-041 #9 敞口门按最近一次 on_legs

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#9)、[热路径:分类与检查](README.md#热路径分类与检查)(`on_legs`)
- 级别:table
- 里程碑:M2
- 前置:R0(g1 `naked_gate 0.5`、`naked_gate_min 100`);每行先 `on_legs(1, g1, …)`
- 实现:`crates/hft-risk/tests/chain.rs::rsk_041_exposure_gate`

| 行 | `on_legs` 的 `(naked_ratio, naked_notional)` | 意图 | 期望 |
|---|---|---|---|
| RSK-041.1 | (0.5, 1 000) | g1 的 a1 Open | 放行 |
| RSK-041.2 | (0.500000001, 100) | 同上 | 拒 `EXPOSURE_GATE` |
| RSK-041.3 | (0.500000001, 99.999999) | 同上 | 放行 |
| RSK-041.4 | (0.9, 1 000) | `Hedge(1, g1, b1, Sell, 0.01 @ 49 995.0)`;`Open(1, g3, a2, Buy, 0.1 @ 2 500.00)` | 放行;放行 |
| RSK-041.5 | 先 (0.9, 1 000),再 (0.1, 1 000) | g1 的 a1 Open | 放行(按最近一次) |
| RSK-041.6 | g1 从未调用 `on_legs` | g1 的 a1 Open | 放行 |

### RSK-042 冒烟单按 Open 走全链,但不经模式门与条件 cap、不查价格带

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(冒烟单)
- 级别:table
- 里程碑:M2
- 前置:R0;冒烟单 S = `origin = Command`、`flags = SMOKE`、`tif = PostOnly`、账户 1、`strategy = 1`、`group = GROUP_NONE`、a1 Buy 0.0002 @ 47 500.00(离 mid 5%,`max_px_dev 0.03`)
- 实现:`crates/hft-risk/tests/chain.rs::rsk_042_smoke_order_scope`

| 行 | 状态 | 期望 |
|---|---|---|
| RSK-042.1 | R0 | 放行 |
| RSK-042.2 | 账户 1 全部 cell 期望 Shadow;期望 CloseOnly | 放行;放行 |
| RSK-042.3 | `Recovery{1}`、`MarginStress{1}`、`Broken{1, g1}` + `HedgeFailure{1}`、`DailyLoss{1}`、`Recon{1, A}`、`AccountCfg{1, A}`、`UnknownTimeout{1, A}`、`Storage`、`Halted{s1, 1, g1}`(九个子例) | 每个放行 |
| RSK-042.4 | `Kill{Account(1)}` | 拒 `KILLED` |
| RSK-042.5 | `Kill{Venue(A) × 1}`;`Kill{Venue(B) × 1}` | 拒 `KILLED`;放行 |
| RSK-042.6 | `Kill{Global}` | 拒 `KILLED` |
| RSK-042.7 | `Kill{Account(1), hard}` | 拒 `HARD` |
| RSK-042.8 | `Draining` | 拒 `DRAINING` |
| RSK-042.9 | `Delisting{A, a1}` | 拒 `CLOSE_ONLY` |
| RSK-042.10 | a1 `conn_up = false`;a1 买一为空 | 拒 `STALE_MID`;拒 `NO_MID` |
| RSK-042.11 | `ClockBad` | 拒 `CLOCK` |
| RSK-042.12 | `venue_ready(A, 1) = false` | 拒 `VENUE_NOT_READY` |
| RSK-042.13 | (A, 1) 已 80 次提交(余 20) | 拒 `RATE`(冒烟单不是风险递减单) |

### RSK-043 链接 Open 对第二腿 venue 同时检查,任一失败第一腿不发

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(链接腿规则)、[mode 模块](README.md#mode-模块唯一的模式状态)(`venue_level`)
- 级别:table
- 里程碑:M2
- 前置:R0;L = `LinkOpen(1, g1, a1, Buy, 0.01 @ 50 000.00, Gtc; link = (1, B, b1, Sell, 0.01, 495))`;L2 = `LinkOpen(2, g2, a1, Buy, 0.01 @ 50 000.00; link = (1, B, b1, Sell, 0.01, 495))`(链接腿在另一账户)
- 实现:`crates/hft-risk/tests/chain.rs::rsk_043_linked_open_both_venues`

| 行 | 状态 | 意图 | 期望 |
|---|---|---|---|
| RSK-043.1 | R0 | L | 放行 |
| RSK-043.2 | `Kill{Venue(B) × 1}` | L | 拒 `KILLED` |
| RSK-043.3 | `Recon{1, B}` | L | 拒 `RECON_DEVIATION` |
| RSK-043.4 | `AccountCfg{1, B}`;`UnknownTimeout{1, B}`;`Delisting{B, b1}` | L | 每个拒 `CLOSE_ONLY` |
| RSK-043.5 | `venue_ready(B, 1) = false` | L | 拒 `VENUE_NOT_READY` |
| RSK-043.6 | `MarginStress{1}` | L2 | 拒 `CLOSE_ONLY`(链接账户的账户级 cap 计入 `venue_level`) |
| RSK-043.7 | `Kill{Account(1)}` | L2 | 拒 `KILLED` |
| RSK-043.8 | `Halted{s1, 1, g1}` | L2 | 放行(cell 级 cap 不计入 `venue_level`) |
| RSK-043.9 | `Kill{Venue(A) × 1}` | Simultaneous 第二腿 `LinkOpen(1, g1, b1, Sell, 0.01 @ 49 995.0; link = (1, A, a1, Buy, 0.01, 505))` | 拒 `KILLED` |

### RSK-044 以 cap_cross_px 定价、按限价折算数量的引擎单不被 #5 与 #6 拒绝

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(引擎单的限价与名义)
- 级别:prop
- 里程碑:M2
- 性质:执行层 `Flatten`(`reason = MarginStress`)的限价 = hft-types `cap_cross_px(side, touch, mid, rr_max_cross, …)`,数量 = ⌊`max_reduce_notional` / 限价⌋ 取到 lot;报价健康时 `evaluate` 的结果不是 `PX_BAND` 也不是 `NOTIONAL_ORDER`;对冲腿(`max_hedge_notional`)与 Open 切片(`ioc_max_cross`、`max_order_notional`)同理
- 生成:mid 100.00..100 000.00;对手价离 mid 0..3 × `rr_max_cross`;side;`rr_max_cross` 0.001..0.05;`l3_max_cross` ∈ {0, rr..2 rr};仓位足够大
- 参考:`evaluate` 的返回位 ∉ {`PX_BAND`, `NOTIONAL_ORDER`, `HEDGE_LIMIT`}
- 例数:PR 2 000
- 实现:`crates/hft-risk/tests/chain_props.rs::rsk_044_engine_orders_pass_band_and_notional`

### RSK-045 拒绝恰有一位,且等于参考模型的第一个失败检查

- 规则:[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(拒绝只带一位、适用范围表)
- 级别:prop
- 里程碑:M2
- 性质:任意意图与状态下 `evaluate` 的 `Err` 恰有一位;该位等于参考模型按 #0–#9 顺序与"失败位"列顺序找到的第一个失败位;风险递减单从不得到 `CLOCK`、`CLOSE_ONLY`、`KILLED`、`DRAINING`、`DAILY_LOSS`、`RECON_DEVIATION`、`VENUE_NOT_READY`、`UNKNOWN_PENDING`、`EXPOSURE_GATE`、`LINK_STALE`、`LINK_FUNDS`
- 生成:R0 的账户与 inst;意图种类 × 类别 × 是否带链接腿;cap 集合为 15 种来源的任意子集(含 HARD 与否);视图字段在边界 ±1 raw 附近随机
- 参考:`tests/model/chain_ref.rs`(按 README 表格逐列实现,不共享产品代码)
- 例数:PR 5 000;每夜 200 000(见验收)
- 实现:`crates/hft-risk/tests/chain_props.rs::rsk_045_single_bit_matches_reference`

**基线 `G0`**(`crates/hft-risk/tests/common/mod.rs::g0()`,启动闸门用例用):R0,`ModeCfg{auto_resume_after_clean_drain: true, allow_missing_standby: false}`;随后对账户 1 依次 `apply(Auto(Recovery{1}))`、`apply(RecoveryFlags{1, flags = 0})`(本次运行的闸门复位:账户 1 的 `gate_bits = 0`、各 venue 位图为空、`released = false`)。"放行序列"指依次投递:`Gate(Feeds{1, true})`;A、B、C 各一条 `Gate(OrderConns{1, v, primary_up: true, standby_up: true})`;`Gate(Unknown{1, 0})`;A、B、C 各一组全部 `Ignore` 的对账组(`Gate(Recon)`,`view_seq` 递增);`Gate(Clock{ok: true})`;A、B、C 各一条 `Gate(AccountCfg{hdr{mismatches 0, uid_keys SESSION|REST, 模式一致}, keys_required SESSION|REST})`(B、C 之前先 `on_account_cfg` 给 b1、b2、c1 杠杆 3.00);`Gate(KillSelfTest{1})`。

### RSK-050 设置 Broken 同时设 HedgeFailure 并产出对应输出

- 规则:[断腿](README.md#断腿broken自动有界平仓l3-与-flatten)第 1 条、[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(链接腿规则末句)、[cap 记录](README.md#cap-记录)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. `apply(Auto(Broken{account 1, group g1}), seq = 1 000)`
- 期望:
  - cap 集合新增 `Broken{scope = (account 1, group g1)}` 与 `HedgeFailure{scope = (account 1, group GROUP_ALL)}`,两条 `set_seq = 1 000`
  - `cells` 恰两项:`((1, 1, g1), Live, CloseOnly{false})`、`((1, 1, g3), Live, CloseOnly{false})`
  - `cancels` 恰一项:范围为账户 1(group 通配),`open_only = true`
  - `risk_events` 恰三项,都给账户 1:`HedgeBroken{g1}`、`CapSet{Broken}`、`CapSet{HedgeFailure}`
  - `notes` 恰两条 `CapSet`(级别 P1)
  - `ctl_state_version = V + 1`;`kill_changed = false`;`resolve_broken = false`;`drain = None`;`released`、`hedge_gate_open`、`venue_gate_open`、`gate_changes` 为空
- 实现:`crates/hft-risk/tests/broken.rs::rsk_050_broken_sets_hedge_failure`

### RSK-051 on_legs 在已静止且已成交裸量不超过尘埃线时返回 AutoClear{Broken}

- 规则:[断腿](README.md#断腿broken自动有界平仓l3-与-flatten)第 3、7 条、[热路径:分类与检查](README.md#热路径分类与检查)(`on_legs`)
- 级别:table
- 里程碑:M2
- 前置:R0(`dust_notional = 10`);`apply(Auto(Broken{1, g1}))` 之后
- 实现:`crates/hft-risk/tests/broken.rs::rsk_051_on_legs_auto_clear`

| 行 | `on_legs` 的 `LegSummary` | 期望 |
|---|---|---|
| RSK-051.1 | (1, g1),`quiescent = true`,`naked_filled_notional = 10.000000` | `Some(AutoClear{Broken, (1, g1)})` |
| RSK-051.2 | 同上,`naked_filled_notional = 10.000001` | `None` |
| RSK-051.3 | `quiescent = false`(自动平仓 IOC 已发出未终态),`naked_filled_notional = 0` | `None` |
| RSK-051.4 | `quiescent = false`(有 Unknown 槽),0 | `None` |
| RSK-051.5 | `quiescent = true`(只有挂着的 GTC,挂单在途不算),`naked_filled_notional = 0` | `Some(AutoClear{Broken, (1, g1)})` |
| RSK-051.6 | (1, g3)(无 Broken),`quiescent = true`,0 | `None` |
| RSK-051.7 | 先 .3 再 `quiescent = true`、`naked_filled_notional = 12`(IOC 零成交,裸露照旧) | `None`(仍 Broken) |
| RSK-051.8 | 12(高于尘埃线、低于最小下单量);随后核销后报 `naked_filled_notional = 0`、`quiescent = true` | `None`;`Some(AutoClear{Broken, (1, g1)})` |
| RSK-051.9 | `dust_notional = 0`:0;0.000001 | `Some(AutoClear…)`;`None` |

### RSK-052 AutoClear{Broken} 只删 Broken,HedgeFailure 保留

- 规则:[断腿](README.md#断腿broken自动有界平仓l3-与-flatten)第 3 条、[cap 记录](README.md#cap-记录)(HedgeFailure 行)
- 级别:unit
- 里程碑:M2
- 前置:R0;`apply(Auto(Broken{1, g1}))`,`ctl_state_version = V + 1`
- 步骤:
  1. `on_legs(1, g1, {quiescent: true, naked_filled_notional: 5})` 得到的 `AutoClear{Broken, (1, g1)}` 交 `apply`
  2. `evaluate(Open(1, g1, a1, Buy, 0.01 @ 50 000.00))`
- 期望:
  - 步骤 1:`Broken{1, g1}` 删除、`HedgeFailure{1}` 仍在;`notes` 恰一条 `CapCleared`(Info);`risk_events` 恰一项 `(1, CapCleared{Broken})`;`cells` 为空(两个 cell 仍因 HedgeFailure 为 CloseOnly);`ctl_state_version = V + 2`;`resolve_broken = false`
  - 步骤 2:拒 `CLOSE_ONLY`
- 实现:`crates/hft-risk/tests/broken.rs::rsk_052_auto_clear_keeps_hedge_failure`

### RSK-053 Resume{Broken} 要求已成交裸量已在尘埃线以下

- 规则:[cap 记录](README.md#cap-记录)(Broken 行)、[mode 模块](README.md#mode-模块唯一的模式状态)(`ModeView::naked_cleared`、拒绝顺序)
- 级别:unit
- 里程碑:M2
- 前置:R0;`apply(Auto(Broken{1, g1}))`;`ModeView.naked_cleared(1, g1) = false`
- 步骤:
  1. `apply(Resume{Broken, scope (1, g1)})`
  2. 置 `naked_cleared(1, g1) = true`,再 `apply(Resume{Broken, scope (1, g1)})`
- 期望:
  - 步骤 1:`Err(ModeErr::NakedNotCleared)`;cap 不变;`ctl_state_version` 不变;`ModeOutcome` 各向量为空
  - 步骤 2:`Ok`;`Broken{1, g1}` 删除、`HedgeFailure{1}` 仍在;`resolve_broken = false`
- 实现:`crates/hft-risk/tests/broken.rs::rsk_053_resume_broken_requires_cleared`

### RSK-054 Resume{HedgeFailure} 在账户仍有 Broken 时返回 BrokenRemaining

- 规则:[cap 记录](README.md#cap-记录)(HedgeFailure 行)、[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(链接腿规则末句)
- 级别:unit
- 里程碑:M2
- 前置:R0;`apply(Auto(Broken{1, g1}))`、`apply(Auto(Broken{1, g3}))`
- 步骤:
  1. `apply(Resume{HedgeFailure, account 1})`
  2. `apply(AutoClear{Broken, (1, g1)})`;再 `apply(Resume{HedgeFailure, account 1})`
  3. `apply(AutoClear{Broken, (1, g3)})`;再 `apply(Resume{HedgeFailure, account 1})`
  4. `evaluate(Open(1, g1, a1, Buy, 0.01 @ 50 000.00))`
- 期望:
  - 步骤 1 与步骤 2 的 Resume:`Err(ModeErr::BrokenRemaining)`
  - 步骤 3 的 Resume:`Ok`;`cells` 恰两项 `((1, 1, g1), CloseOnly{false}, Live)`、`((1, 1, g3), CloseOnly{false}, Live)`;`notes` 一条 `CapCleared`
  - 步骤 4:放行
- 实现:`crates/hft-risk/tests/broken.rs::rsk_054_resume_hedge_failure_after_all_broken`

### RSK-055 只有删除带 HARD 的 Kill 记录才产出 resolve_broken

- 规则:[断腿](README.md#断腿broken自动有界平仓l3-与-flatten)第 6 条、[mode 模块](README.md#mode-模块唯一的模式状态)(`ModeOutcome.resolve_broken`)
- 级别:table
- 里程碑:M2
- 前置:R0;HARD 解除所需的对账组:A、B、C 各一组 `view_seq` 晚于 Kill 的全部 `Ignore` 报告(账户 2 的 A、B 同样)
- 实现:`crates/hft-risk/tests/broken.rs::rsk_055_resolve_broken_only_on_hard_release`

| 行 | 建立 | 解除输入 | 该 apply 的 `resolve_broken` |
|---|---|---|---|
| RSK-055.1 | `Kill{Global, hard}` | `Resume{Kill, Global}` | true |
| RSK-055.2 | `Kill{Account(1), hard}` | `Resume{Kill, Account(1)}` | true |
| RSK-055.3 | `Kill{Account(1)}` 后再 `Kill{Account(1), hard}`(合并为一条带 HARD) | `Resume{Kill, Account(1)}` | true |
| RSK-055.4 | `Kill{Account(1)}` | `Resume{Kill, Account(1)}` | false |
| RSK-055.5 | `DailyLoss{1}` | `Resume{DailyLoss, 1}` | false |
| RSK-055.6 | G0,`RecoveryFlags{1, CLEAN_DRAIN}` | 放行序列(自动删除 Recovery) | false |
| RSK-055.7 | `Recovery{1}`,闸门已放行 | `Resume{Recovery, 1}` | false |
| RSK-055.8 | `Broken{1, g1}` | `AutoClear{Broken, (1, g1)}` | false |

### RSK-056 Broken 期间引擎减仓与自动平仓照常受 #5–#8 约束

- 规则:[断腿](README.md#断腿broken自动有界平仓l3-与-flatten)第 2、4 条
- 级别:table
- 里程碑:M2
- 前置:R0;`apply(Auto(Broken{1, g1}))`;b1 持仓 −0.5
- 实现:`crates/hft-risk/tests/broken.rs::rsk_056_broken_engine_orders`

| 行 | 另加状态 | 意图 | 期望 |
|---|---|---|---|
| RSK-056.1 | — | 执行层 `role = HedgeFix` 的 Hedge,b1 Buy 0.01 @ 50 005.0,`hedge_cap_qty 0.01` | 放行 |
| RSK-056.2 | — | 同上 qty 0.011 | 拒 `HEDGE_LIMIT` |
| RSK-056.3 | — | 同上 qty 0.4 @ 50 000.0、`hedge_cap_qty 0.5`,`notional = 20 000.000001` | 拒 `HEDGE_LIMIT` |
| RSK-056.4 | `Kill{Account(1), hard}` | .1 的意图 | 拒 `HARD` |
| RSK-056.5 | `MarginStress{1}`(L3)+ `Kill{Account(1), flatten}` | 执行层 `Flatten(1, g1, b1, Buy, 0.2 @ 50 000.0, reason = MarginStress)`(名义恰为 `max_reduce_notional`) | 放行 |
| RSK-056.6 | `Kill{Account(1), flatten}` | 执行层 `Flatten(…, reason = Hard)`,`notional = 10 000.000001` | 拒 `NOTIONAL_ORDER` |

### RSK-060 有效模式由期望模式与覆盖 cell 的 cap 决定

- 规则:[有效模式](README.md#有效模式)、[mode 模块](README.md#mode-模块唯一的模式状态)(`effective`、`venue_level`、`inst_blocked`)
- 级别:table
- 里程碑:M2
- 前置:R0;未写的 cell 为 (1, 1, g1);每行从 R0 开始
- 实现:`crates/hft-risk/tests/mode.rs::rsk_060_effective_mode`

| 行 | 输入 | 期望 |
|---|---|---|
| RSK-060.1 | 无 | `effective = Live` |
| RSK-060.2 | `SetMode{(1, 1, g1), CloseOnly}` | `CloseOnly{deferred_shadow: false}` |
| RSK-060.3 | `cell_flat = true`,`SetMode{(1, 1, g1), Shadow}` | `Shadow`;`cells` 恰一项 `(cell, Live, Shadow)` |
| RSK-060.4 | `cell_flat = false`,`SetMode{(1, 1, g1), Shadow}` | `CloseOnly{deferred_shadow: true}`;`cells` 恰一项 `(cell, Live, CloseOnly{true})` |
| RSK-060.5 | .4 之后 `CellFlat{cell, flat: true}` | `Shadow`;`cells` 恰一项 `(cell, CloseOnly{true}, Shadow)`;`ctl_state_version` 不变 |
| RSK-060.6 | R0 下 `CellFlat{cell, flat: true}`;`CellFlat{cell, flat: false}` | 都无输出,`effective` 仍 Live |
| RSK-060.7 | `DailyLoss{1}` | `CloseOnly{false}` |
| RSK-060.8 | `cell_flat = true`、`SetMode{Shadow}`,再 `DailyLoss{1}` | `Shadow`(cap 的增删不改 Shadow) |
| RSK-060.9 | `Recon{1, B}` | `effective = Live`;`venue_level(1, B) = CloseOnly`;`venue_level(1, A) = None` |
| RSK-060.10 | `Kill{Venue(B) × 1}` | `effective = Live`;`venue_level(1, B) = CloseOnly` |
| RSK-060.11 | `Kill{Account(1), hard}` | `CloseOnly{false}`;`venue_level(1, A) = Hard` |
| RSK-060.12 | `Halted{s1, 1, g1}` | (1, 1, g1) `CloseOnly{false}`;(1, 1, g3) Live |
| RSK-060.13 | `Halted{s1, 1, GROUP_NONE}`(作用域 group = GROUP_NONE) | (1, 1, g1)、(1, 1, g3) 都 Live(GROUP_NONE 不是通配) |
| RSK-060.14 | `Kill{Global}` | 四个 cell(含 Sim 账户的 (3, 3, gs))都 `CloseOnly{false}`;`cells` 四项 |
| RSK-060.15 | `AccountCfg{1, B}`、`UnknownTimeout{1, B}`、`Delisting{B, b1}`(三个子例) | `effective = Live`;Delisting 子例 `inst_blocked(B, b1) = true`、`inst_blocked(B, b2) = false` |
| RSK-060.16 | `Delisting{B, inst = 0}` | `inst_blocked(B, b1) = inst_blocked(B, b2) = true`;`inst_blocked(A, a1) = false` |
| RSK-060.17 | `ClockBad`;`Storage`(档位 1);`Draining`(三个子例) | 四个 cell 都 `CloseOnly{false}` |

### RSK-061 SetMode 只写期望值、不清 cap,cap 的增删不改期望值

- 规则:[有效模式](README.md#有效模式)(末段)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. `apply(SetMode{(1, 1, *), CloseOnly})`
  2. `apply(Auto(DailyLoss{1}))`
  3. `apply(SetMode{(1, 1, *), Live})`
  4. `apply(Resume{DailyLoss, 1})`
- 期望:
  - 步骤 2 后 cap 集合含 `DailyLoss{1}`,ModeTable 中 (1, 1, g1)、(1, 1, g3) 仍为 CloseOnly
  - 步骤 3 后 ModeTable 为 Live,`DailyLoss{1}` 仍在,`effective` 仍 `CloseOnly{false}`,`cells` 为空
  - 步骤 4 后 ModeTable 仍为 Live,`effective = Live`,`cells` 两项 `(cell, CloseOnly{false}, Live)`
- 实现:`crates/hft-risk/tests/mode.rs::rsk_061_setmode_and_caps_independent`

### RSK-062 SetMode{group = *} 写 default_mode,新增 group 取它

- 规则:[mode 模块](README.md#mode-模块唯一的模式状态)(`ModeTable` 的 default_mode、`GroupsChanged`)
- 级别:unit
- 里程碑:M2
- 前置:R0;`cell_flat = true`
- 步骤:
  1. `apply(SetMode{(1, 1, *), Shadow})`
  2. `apply(GroupsChanged{added: [(1, 1, g4 = 0x0100_0003)], removed: [(1, 1, g3)]})`
- 期望:
  - 步骤 2 后 `effective((1, 1, g4)) = Shadow`;ModeTable 无 (1, 1, g3) 行;节 14 快照含 `ModeRow{strategy 1, account 1, mode Shadow, flags DEFAULT, group GROUP_NONE}`
- 实现:`crates/hft-risk/tests/mode.rs::rsk_062_default_mode_for_new_group`

### RSK-063 level_of:只有带 HARD 的 Kill 是 Hard,其余都是 CloseOnly

- 规则:[cap 记录](README.md#cap-记录)(`CapLevel`、`level_of`)
- 级别:table
- 里程碑:M2
- 前置:无运行时状态;对构造的 `CapRecord` 调 `level_of`
- 实现:`crates/hft-risk/tests/mode.rs::rsk_063_level_of`

| 行 | 记录 | `level_of` |
|---|---|---|
| RSK-063.1 | `Kill`,`cflags = HARD` | `Hard`(3) |
| RSK-063.2 | `Kill`,`cflags = 0`(三种 `KillLevel` 各一) | `CloseOnly`(1) |
| RSK-063.3 | `MarginStress`,`level = 3` | `CloseOnly` |
| RSK-063.4 | `Draining`、`ClockBad`、`DailyLoss`、`Recon`、`UnknownTimeout`、`Broken`、`Reorg`、`Recovery`、`Halted`、`HedgeFailure`、`Storage`、`Delisting`、`AccountCfg`(十三个子例) | 每个 `CloseOnly` |

### RSK-070 每种来源的 cap 记录编码

- 规则:[cap 记录](README.md#cap-记录)(cap 表、`cflags` 与 `level`)
- 级别:table
- 里程碑:M2
- 前置:R0;每行从 R0 用所列输入设置一条 cap(`seq = 500`),读该 apply 的 `CapSet` 注记 `v[0..6]` 解出的 `CapRecord`;scope 未写的字段为 0、group 未写为 `GROUP_ALL`
- 实现:`crates/hft-risk/tests/mode.rs::rsk_070_cap_record_encoding`

| 行 | 输入 | 期望 `{source, level, cflags, scope, set_seq, params}` |
|---|---|---|
| RSK-070.1 | `Kill{Account(1), cancel_resting: true}` | `{1, 3, 0, {account 1}, 500, [1, 0]}` |
| RSK-070.2 | `Kill{Global, hard, cancel_resting: true}` | `{1, 4, HARD = 1, {}, 500, [1, 0]}` |
| RSK-070.3 | `Kill{Venue(B) × 1, cancel_resting: false}` | `{1, 2, 0, {venue 3, account 1}, 500, [0, 0]}` |
| RSK-070.4 | `Kill{Account(1), cancel_resting: true, flatten: true}` | `{1, 3, 0, {account 1}, 500, [3, 0]}` |
| RSK-070.5 | `on_pnl(1, −5 000.000001, …)` 返回的 `Auto(DailyLoss)` | `{2, 0, 0, {account 1}, 500, [(−5 000 000 000) as u64, 0]}` |
| RSK-070.6 | `Recon{1, B}`;`UnknownTimeout{1, B}` | `{3, 0, 0, {account 1, venue 3}, …}`;`{4, 0, 0, {account 1, venue 3}, …}` |
| RSK-070.7 | `Broken{1, g1}` | `{5, 0, 0, {account 1, group g1}, …}`(同一 apply 的第二条为 `{11, 0, 0, {account 1}, …}`) |
| RSK-070.8 | `Recovery{1}`;`Halted{s1, 1, g1}` | `{7, 0, 0, {account 1}, …}`;`{8, 0, 0, {strategy 1, account 1, group g1}, …}` |
| RSK-070.9 | `Draining`(`cancel_resting = 1`) | `{9, 0, NON_CLEARABLE = 4, {}, 500, [1, 0]}` |
| RSK-070.10 | `Clock{ok: false}` | `{10, 0, CONDITIONAL = 2, {}, 500, [0, 0]}` |
| RSK-070.11 | 账户 1 在 B 进入 L2 的 `Auto(MarginStress)` | `{12, 2, 2, {account 1}, 500, [8, 0]}`(venue 位图 1 << 3) |
| RSK-070.12 | `Storage{PersistLag, 1}` | `{13, 0, 2, {}, 500, [1, 0]}` |
| RSK-070.13 | `Notice(Delisting{venue B, inst b1, at_ns = 1 700 000 000 000 000 000})` | `{14, 0, 0, {venue 3, inst 3}, 500, [1 700 000 000 000 000 000, 0]}` |
| RSK-070.14 | 放行后 b1 杠杆 3.01 的 `Auto(AccountCfg{1, B})` | `{15, 0, 2, {account 1, venue 3}, 500, [LEVERAGE = 2, 0]}` |

### RSK-071 同一 (source, scope) 再次设置按来源合并取更严

- 规则:[cap 记录](README.md#cap-记录)(合并)
- 级别:table
- 里程碑:M2
- 前置:R0;每行依次 apply 所列输入,首条 `seq = 500`、第二条 `seq = 600`
- 实现:`crates/hft-risk/tests/mode.rs::rsk_071_cap_merge`

| 行 | 输入序列 | 期望(第二条输入的 apply) |
|---|---|---|
| RSK-071.1 | `Kill{Account(1), cancel_resting}`;`Kill{Account(1), cancel_resting, flatten}` | cap 仍一条,`params[0] = 3`、`set_seq = 500`;`notes` 一条 `CapSet`(合并后的记录);`ctl_state_version` +1 |
| RSK-071.2 | .1 之后再 `Kill{Account(1), cancel_resting}` | 记录不变(`FLATTEN` 位保留);无任何输出;`ctl_state_version` 不变 |
| RSK-071.3 | `Kill{Global}`;`Kill{Global, hard}` | 一条,`cflags = HARD`;`kill_changed = true` |
| RSK-071.4 | `Auto(DailyLoss{1, params[0] = −5 000})`;`Auto(DailyLoss{1, params[0] = −4 000})` | `params[0] = −4 000`;一条 `CapSet` |
| RSK-071.5 | .4 之后 `Auto(DailyLoss{1, params[0] = −6 000})` | 记录不变;无输出 |
| RSK-071.6 | `Auto(AccountCfg{1, B, MISMATCH})`;`Auto(AccountCfg{1, B, LEVERAGE})` | `params[0] = 3` |
| RSK-071.7 | MarginStress L1(B);L2(B) | `level = 2`;一条 `CapSet`、`risk_events` 含 `(1, CapSet{MarginStress})` |
| RSK-071.8 | .7 之后按保证金分档规则降到 L1 | `level = 1`(不取更严);一条 `CapSet` |

### RSK-072 Resume 只删除 (source, scope) 完全匹配的一条

- 规则:[cap 记录](README.md#cap-记录)、[mode 模块](README.md#mode-模块唯一的模式状态)(`NoMatchingCap`)
- 级别:unit
- 里程碑:M2
- 前置:R0;`apply(Kill{Account(1)})`、`apply(Kill{Global})`、`apply(Auto(Halted{s1, 1, g1}))`
- 步骤:
  1. `apply(Resume{Kill, Account(1)})`
  2. `apply(Resume{Kill, Account(2)})`
  3. `apply(Resume{Halted, (s1, 1, GROUP_ALL)})`
  4. `apply(Resume{Kill, Global})`
- 期望:
  - 步骤 1:`Ok`;`Kill{Global}` 仍在;(1, 1, g1) 仍 CloseOnly;`cells` 为空
  - 步骤 2:`Err(ModeErr::NoMatchingCap)`
  - 步骤 3:`Err(NoMatchingCap)`(cap 的 group 为 g1)
  - 步骤 4:`Ok`;(1, 1, g3)、(2, 2, g2)、(3, 3, gs) 变为 Live,(1, 1, g1) 仍 CloseOnly(Halted)
- 实现:`crates/hft-risk/tests/mode.rs::rsk_072_resume_exact_match`

### RSK-073 不可清除的来源对 Resume 返回 NotResumable,可清除来源无记录时 NoMatchingCap

- 规则:[cap 记录](README.md#cap-记录)(`resumable()` 一条)
- 级别:table
- 里程碑:M2
- 前置:R0;"有记录"指先设置该来源的一条 cap
- 实现:`crates/hft-risk/tests/mode.rs::rsk_073_resumable_sources`

| 行 | 输入 | 期望 |
|---|---|---|
| RSK-073.1 | 有记录时 `Resume{Draining}`、`Resume{ClockBad}`、`Resume{MarginStress, 1}`、`Resume{Storage}`、`Resume{AccountCfg, (1, B)}`(五个子例) | 每个 `Err(ModeErr::NotResumable)`,cap 仍在 |
| RSK-073.2 | 无记录时对 Kill、DailyLoss、Recon、UnknownTimeout、Broken、Recovery、Halted、HedgeFailure、Delisting 各发一次 `Resume`(作用域取该来源的形状,账户 1) | 每个 `Err(NoMatchingCap)` |
| RSK-073.3 | 有记录时 `Resume{Delisting, (B, b1)}` | `Ok`;`inst_blocked(B, b1) = false` |
| RSK-073.4 | 有记录时 `Resume{Halted, (s1, 1, g1)}` | `Ok` |

### RSK-074 CapSet 与 CapCleared 注记的实际级别

- 规则:[cap 记录](README.md#cap-记录)(实际级别)
- 级别:table
- 里程碑:M2
- 前置:R0;每行设置所列 cap 后再按其解除方式删除,读两次 apply 的 `notes`;注记的 `source = Risk`、`code = Risk << 24 | n`(n 取本 crate 的码表)、`v[0..6]` = 该条 `CapRecord` 的 48 字节
- 实现:`crates/hft-risk/tests/mode.rs::rsk_074_cap_note_levels`

| 行 | cap(设置方式) | `CapSet` 级别 | `CapCleared` 级别 |
|---|---|---|---|
| RSK-074.1 | `Kill{Account(1)}`(命令) | Info | Info |
| RSK-074.2 | `Delisting{B, b1}`(命令) | Info | Info |
| RSK-074.3 | `Recovery{1}`(冷启动) | Info | Info |
| RSK-074.4 | `Draining` | Info | —(不可清除) |
| RSK-074.5 | `DailyLoss`、`Recon`、`UnknownTimeout`、`Broken`、`HedgeFailure`、`MarginStress`、`Storage`、`AccountCfg`、`ClockBad`、`Halted`(自动设置,十个子例) | 每个 P1 | 每个 Info |
| RSK-074.6 | UID 不符自动设置的 `Kill{Account(1), hard}` | P1 | Info |

### RSK-075 cap 增删按受影响账户产出 RiskEvent

- 规则:[cap 记录](README.md#cap-记录)(`risk_events`)
- 级别:table
- 里程碑:M2
- 前置:R0;读该 apply 的 `risk_events`(顺序按账户升序)
- 实现:`crates/hft-risk/tests/mode.rs::rsk_075_risk_events_per_account`

| 行 | 输入 | 期望 `risk_events` |
|---|---|---|
| RSK-075.1 | `Kill{Global}` | `(1, CapSet{Kill})`、`(2, CapSet{Kill})`、`(3, CapSet{Kill})` |
| RSK-075.2 | `Recon{1, B}` | `(1, CapSet{Recon})` |
| RSK-075.3 | `Storage{PersistLag, 1}` | 账户 1、2、3 各一项 `CapSet{Storage}` |
| RSK-075.4 | `Kill{Venue(B) × 1}` | `(1, CapSet{Kill})` |
| RSK-075.5 | `Notice(Delisting{B, b1})` | 账户 1、2、3(`venues` 含 B)各一项 `CapSet{Delisting}` |
| RSK-075.7 | `Kill{Venue(C)}`(account = *) | 只有 `(1, CapSet{Kill})`(只有账户 1 的 `venues` 含 C) |
| RSK-075.6 | .1 之后 `Resume{Kill, Global}` | 三项 `CapCleared{Kill}` |

### RSK-076 ctl_state_version 只在 ModeTable 或 cap 集合变化时恰好加 1

- 规则:[cap 记录](README.md#cap-记录)(`ctl_state_version`)、[mode 模块](README.md#mode-模块唯一的模式状态)
- 级别:table
- 里程碑:M2
- 前置:R0(`ctl_state_version = V`);每行从 R0 开始,读最后一次 apply 后的 `ModeOutcome.ctl_state_version`
- 实现:`crates/hft-risk/tests/mode.rs::rsk_076_state_version`

| 行 | 输入序列 | 期望 |
|---|---|---|
| RSK-076.1 | `SetMode{(1, 1, g1), CloseOnly}` | V + 1 |
| RSK-076.2 | 同一 `SetMode` 两次 | V + 1(第二次无变化) |
| RSK-076.3 | `Auto(Broken{1, g1})`(增两条 cap) | V + 1 |
| RSK-076.4 | `Gate(OrderConns{1, B, false, true})` | V |
| RSK-076.5 | `cell_flat = false`、`SetMode{Shadow}`,再 `CellFlat{flat: true}` | V + 1 |
| RSK-076.6 | `Resume{DailyLoss, 1}`(无记录,被拒) | V |
| RSK-076.7 | `Clock{ok: false}`、`Clock{ok: true}` | V + 2 |
| RSK-076.8 | `Kill{Account(1)}`、`Resume{Kill, Account(1)}` | V + 2 |

### RSK-077 apply 的拒绝顺序,被拒时不改任何状态

- 规则:[mode 模块](README.md#mode-模块唯一的模式状态)(`ModeErr` 与拒绝顺序)、[启动闸门](README.md#启动闸门)(Kill 从不因闸门或 Draining 被拒)、[Kill](README.md#kill)
- 级别:table
- 里程碑:M2
- 前置:R0;"未放行"指账户 1 经 G0 的复位、未投递放行序列;每行比较 apply 前后的 `snapshot` 字节
- 实现:`crates/hft-risk/tests/mode.rs::rsk_077_mode_err_order`

| 行 | 状态 | 输入 | 期望 |
|---|---|---|---|
| RSK-077.1 | — | `Kill{Venue(B), hard}`;`Kill{Global, flatten}`;`Kill{Account(1), flatten, hard}` | 每个 `Err(InvalidScope)`,快照字节不变 |
| RSK-077.2 | `Draining` | `Resume{Draining}` | `Err(NotResumable)` |
| RSK-077.3 | `Draining`,账户 1 未放行 | `Resume{DailyLoss, 1}` | `Err(Draining)` |
| RSK-077.4 | 账户 1 未放行、无 DailyLoss | `Resume{DailyLoss, 1}` | `Err(GateNotReleased)` |
| RSK-077.5 | 已放行、无 DailyLoss | `Resume{DailyLoss, 1}` | `Err(NoMatchingCap)` |
| RSK-077.6 | 账户 1 未放行 | `SetMode{(1, 1, g1), Live}`;`SetMode{(1, 1, g1), CloseOnly}` | `Err(GateNotReleased)`;`Ok` |
| RSK-077.7 | 账户 1 未放行 + `Draining` | `Kill{Account(1)}`;`Kill{Account(1), flatten}` | 两个都 `Ok`;后者之后 `view(1).flatten_lock = true` |
| RSK-077.8 | 账户 1 未放行、账户 2 已放行 | `SetMode{(*, *, *), Live}`(`account = *`) | `Err(GateNotReleased)`,账户 2 的 cell 也不变 |
| RSK-077.9 | 全部已放行 | 同 .8 | `Ok` |
| RSK-077.10 | 账户 1 有 `UnknownTimeout{1, B}`,`unknown_count(1, B) = 1`;再置 0 | `Resume{UnknownTimeout, (1, B)}` 两次 | `Err(UnknownPending)`;`Ok` |
| RSK-077.11 | 任一被拒的 apply | — | `ModeOutcome` 各向量为空,`ctl_state_version` 不变 |

### RSK-078 is_upgrade 按目标值判定

- 规则:[mode 模块](README.md#mode-模块唯一的模式状态)(`is_upgrade`)
- 级别:table
- 里程碑:M2
- 前置:R0
- 实现:`crates/hft-risk/tests/mode.rs::rsk_078_is_upgrade`

| 行 | 输入 | `is_upgrade` |
|---|---|---|
| RSK-078.1 | `SetMode{…, Live}` | true |
| RSK-078.2 | `SetMode{…, CloseOnly}`;`SetMode{…, Shadow}` | false;false |
| RSK-078.3 | `Resume{Kill, …}`;`Resume{DailyLoss, …}`;`Resume{Recovery, …}` | 都 true |
| RSK-078.4 | `Kill{Account(1), flatten}` | true |
| RSK-078.5 | `Kill{Account(1)}`;`Kill{Global, hard}`;`Notice(Delisting)` | 都 false |

### RSK-079 preview 不改状态并列出未处置的对账条目

- 规则:[mode 模块](README.md#mode-模块唯一的模式状态)(`preview`)、[对账闸门与 HARD 解除的严格条件](README.md#对账闸门与-hard-解除的严格条件)(PREVIEW)
- 级别:unit
- 里程碑:M2
- 前置:G0 投递放行序列,但 B 的对账组(`report_id 7`)有两条 Warn 仓位条目 b1(差额 −0.003)与 b2(差额 0.02),未处置
- 步骤:
  1. `preview(Resume{Recovery, 1}, …)`
  2. `preview(Kill{Global})`
- 期望:
  - 步骤 1:`Preview` 列出账户 1 venue B 未处置 2 条:`ReconItemKey(Position, b1)` 差额 −0.003、`ReconItemKey(Position, b2)` 差额 0.02;A、C 为 0 条
  - 两步前后 `snapshot` 字节相同,`ctl_state_version` 不变
- 实现:`crates/hft-risk/tests/mode.rs::rsk_079_preview_lists_undisposed`

### RSK-080 一次 apply 改变全部 cell 时 ModeOutcome 各向量不溢出

- 规则:[mode 模块](README.md#mode-模块唯一的模式状态)(`ModeOutcome` 容量取构造上界)
- 级别:unit
- 里程碑:M2
- 前置:refdata 20 个 Live 账户,每账户 2 个 venue(奇数账户 {A, B}、偶数账户 {C, D = `VenueId 10` `BITGET_UTA_USDT_FUT`},使每 venue 的 Live 账户数不超过 `KillBits::MAX_ACCOUNTS_PER_GW`);`max_cells_total = 32`,32 个 cell 分属这 20 个账户;`ModeOutcome::with_capacity(32, 20, 40)`;全部已放行,期望 Live
- 步骤:
  1. `apply(Kill{Global, hard})`
  2. 20 个账户各经 G0 的复位并投递除 `Clock` 以外的放行序列,然后 `apply(Gate(Clock{ok: true}))`
- 期望:
  - 步骤 1:`cells` 32 项,`risk_events` 20 项,`notes` 1 条,无 panic、无截断
  - 步骤 2:`released` 20 项、`gate_changes` 20 项,每个账户恰出现一次
  - 全程 `CountingAlloc` 分配次数 0
- 实现:`crates/hft-risk/tests/mode.rs::rsk_080_outcome_capacity_bound`

### RSK-090 干净 Drain 重启在闸门放行时自动删除 Recovery

- 规则:[干净 Drain 重启自动解除 Recovery](README.md#干净-drain-重启自动解除-recovery)
- 级别:unit
- 里程碑:M2
- 前置:G0,但 `RecoveryFlags{1, flags = CLEAN_DRAIN}`;`auto_resume_after_clean_drain = true`
- 步骤:
  1. 投递放行序列
- 期望:
  - 放行序列最后一条(`KillSelfTest`)的 apply:`released = [1]`;`Recovery{1}` 删除;`notes` 含一条 `CapCleared`(Info);`risk_events` 含 `(1, CapCleared{Recovery})`;`cells` 含 (1, 1, g1)、(1, 1, g3) 由 `CloseOnly{false}` 变 Live;`ctl_state_version` +1;`resolve_broken = false`
  - 之前各步 Recovery 都在
- 实现:`crates/hft-risk/tests/mode.rs::rsk_090_clean_drain_auto_resume`

### RSK-091 任一干净条件不满足时 Recovery 保留

- 规则:[干净 Drain 重启自动解除 Recovery](README.md#干净-drain-重启自动解除-recovery)
- 级别:table
- 里程碑:M2
- 前置:G0(R0 含 Live 账户 1、2 与 Sim 账户 3,不是只含 Sim 账户的实例);每行按所列 `RecoveryFlags` 与开关,然后投递放行序列
- 实现:`crates/hft-risk/tests/mode.rs::rsk_091_unclean_keeps_recovery`

| 行 | `flags` / 开关 | 期望(放行的那次 apply) |
|---|---|---|
| RSK-091.1 | 0(尾部非空、有 Unknown、有 Broken/HedgeFailure、有未结束链接或版本变化时 sequencer 都不置 `CLEAN_DRAIN`) | `released = [1]`;`Recovery{1}` 仍在;无 `CapCleared`;无 `RecoveryHeldSim` |
| RSK-091.2 | `CLEAN_DRAIN \| TAIL_LOST` | 同上 |
| RSK-091.3 | `CLEAN_DRAIN \| RECOVER_MISMATCH` | 同上 |
| RSK-091.4 | `CLEAN_DRAIN \| POISON` | 同上 |
| RSK-091.5 | `CLEAN_DRAIN \| FROM_SNAPSHOT` | 同上 |
| RSK-091.6 | `CLEAN_DRAIN`,`auto_resume_after_clean_drain = false` | 同上 |
| RSK-091.7 | .1 之后 `Resume{Recovery, 1}` | `Ok`;Recovery 删除 |

### RSK-092 停机标记产生的 Kill 在 Recovery 自动解除后仍挡 Open

- 规则:[干净 Drain 重启自动解除 Recovery](README.md#干净-drain-重启自动解除-recovery)(自动解除只删 Recovery)
- 级别:unit
- 里程碑:M2
- 前置:G0,`RecoveryFlags{1, CLEAN_DRAIN}`;放行序列之前 `apply(Kill{Account(1), cancel_resting: true})`(`cli:systemd` 的停机标记 Kill)
- 步骤:
  1. 投递放行序列
  2. `evaluate(Open(1, g1, a1, Buy, 0.01 @ 50 000.00))`
- 期望:
  - 步骤 1 后 `Recovery{1}` 删除、`Kill{Account(1)}` 仍在
  - 步骤 2 拒 `KILLED`
- 实现:`crates/hft-risk/tests/mode.rs::rsk_092_stop_marker_kill_survives`

### RSK-093 只含 Sim 账户的实例在非干净重启后的 Recovery 解除

- 规则:[干净 Drain 重启自动解除 Recovery](README.md#干净-drain-重启自动解除-recovery)(只含 Sim 账户的实例)
- 级别:table
- 里程碑:M2
- 前置:refdata 只有一个账户 1(Sim,venues {A, B},绑定 s1,group g1;id 稠密);对账户 1 做 G0 的复位(`Auto(Recovery{1})`、`RecoveryFlags{1, flags}`),再投递账户 1 的放行序列(Sim 账户的 `RECONCILED`、`ACCOUNT_CFG`、`IDENTITY` 已在复位时置位;`OrderConns` 的 `standby_up = true`)
- 实现:`crates/hft-risk/tests/mode.rs::rsk_093_sim_instance_recovery`

| 行 | `flags` / 开关 | 期望(放行的那次 apply) |
|---|---|---|
| RSK-093.1 | 0,开关开 | `Recovery{1}` 删除,`CapCleared` |
| RSK-093.2 | `TAIL_LOST`,开关开 | 删除 |
| RSK-093.3 | `FROM_SNAPSHOT \| TAIL_LOST`,开关开 | 删除 |
| RSK-093.4 | `RECOVER_MISMATCH`,开关开 | 保留;`notes` 含一条 `RecoveryHeldSim`(P1,`v[0] = 1`,`v[1] = 2`) |
| RSK-093.5 | `POISON \| TAIL_LOST`,开关开 | 保留;`RecoveryHeldSim` 的 `v[1] = 5` |
| RSK-093.6 | 0,开关关 | 保留;无 `RecoveryHeldSim` |
| RSK-093.7 | `CLEAN_DRAIN`,开关开 | 删除 |

### RSK-094 含 Live 账户的实例同样的崩溃重启保留 Recovery 且不写 RecoveryHeldSim

- 规则:[干净 Drain 重启自动解除 Recovery](README.md#干净-drain-重启自动解除-recovery)
- 级别:unit
- 里程碑:M2
- 前置:R0(账户 1、2 Live,账户 3 Sim);账户 3 与账户 1 都做复位,`RecoveryFlags{·, flags = TAIL_LOST}`,开关开
- 步骤:
  1. 投递账户 3 的放行序列
  2. 投递账户 1 的放行序列
- 期望:
  - 两次放行的 apply 中 `Recovery{3}`、`Recovery{1}` 都保留;`notes` 中无 `RecoveryHeldSim`
- 实现:`crates/hft-risk/tests/mode.rs::rsk_094_mixed_instance_no_sim_rule`

### RSK-100 放行序列逐步推进 gate_bits,全部置位的那次 apply 放行

- 规则:[启动闸门](README.md#启动闸门)(位表、`gate_changes`、`released`、对冲闸门子集、恢复期按 venue 放行)
- 级别:unit
- 里程碑:M2
- 前置:G0
- 步骤:
  1. `Gate(Feeds{1, true})`
  2. A、B、C 的 `OrderConns`(每条一次 apply)
  3. `Gate(Unknown{1, 0})`
  4. A、B、C 的对账组
  5. `Gate(Clock{ok: true})`
  6. A、B、C 的 `AccountCfg`
  7. `Gate(KillSelfTest{1})`
- 期望:
  - 步骤 1:`gate_changes = [{1, old 0, new 1, released false}]`
  - 步骤 2:A、B 两次 apply 的 `gate_changes` 为空;C 的 apply 为 `[{1, 1, 3, false}]`
  - 步骤 3:`[{1, 3, 7, false}]`;步骤 4:前两组为空、C 组 `[{1, 7, 15, false}]`;步骤 5:`[{1, 15, 31, false}]`(账户 2、3 的位不变,不列出)
  - 步骤 6:A、B 为空,C 为 `[{1, 31, 63, false}]`;`hedge_gate_open`、`venue_gate_open` 为空(kill 自检未到)
  - 步骤 7:`gate_changes = [{1, 63, 127, true}]`;`released = [1]`(只列一次);`hedge_gate_open` 与 `venue_gate_open` 都恰为 `[(1, A), (1, B), (1, C)]`;`Recovery{1}` 仍在(`flags = 0`)
  - 全程 `ctl_state_version` 不变
- 实现:`crates/hft-risk/tests/gates.rs::rsk_100_release_sequence`

### RSK-101 每个闸门位的置位条件

- 规则:[启动闸门](README.md#启动闸门)(位表)、[身份与配置不符](README.md#启动闸门)
- 级别:table
- 里程碑:M2
- 前置:G0;每行投递放行序列,但把所列一条替换为行中的输入(或追加在序列末尾);读账户 1 的 `gate_bits` 与 `released`
- 实现:`crates/hft-risk/tests/gates.rs::rsk_101_gate_bit_conditions`

| 行 | 替换或追加 | 期望 |
|---|---|---|
| RSK-101.1 | `Feeds{1, false}` | `FEEDS` 未置位,`released = false` |
| RSK-101.2 | C 的 `OrderConns{primary_up: false, standby_up: true}` | `ORDER_CONNS` 未置位 |
| RSK-101.3 | C 的 `OrderConns{primary_up: true, standby_up: false}`,`allow_missing_standby = false` | `ORDER_CONNS` 未置位 |
| RSK-101.4 | 同上,`allow_missing_standby = true` | `ORDER_CONNS` 置位,`released = true` |
| RSK-101.5 | `Unknown{1, 1}`;再追加 `Unknown{1, 0}` | `NO_UNKNOWN` 未置位;随后置位 |
| RSK-101.6 | C 的对账组含一条 Warn 条目 | `RECONCILED` 未置位 |
| RSK-101.7 | `Clock{ok: false}` | `CLOCK` 未置位 |
| RSK-101.8 | B 的报告 `mismatches = POS_MODE`;`MARGIN_MODE`;`MULTI_ASSET`(三个子例) | `ACCOUNT_CFG` 未置位 |
| RSK-101.9 | A 的报告 `mismatches = AUTO_BORROW`;`FEE_RULE`;`ACCT_TYPE`;`KEY_PERM`;`UNREADABLE`(五个子例) | `ACCOUNT_CFG` 未置位 |
| RSK-101.10 | B 的报告 `mismatches = 0` 但 `position_mode = Hedge`(VenueCaps 要求 OneWay) | `ACCOUNT_CFG` 未置位(按 refdata 复算后取并集) |
| RSK-101.11 | 账户 1 另有交易 c1 的 group g4 = 0x0100_0003(腿 a1 + c1,经 `on_groups_changed`);C 之前 `on_account_cfg` 给 c1 `leverage = 3.01` | `ACCOUNT_CFG` 未置位;`cfg_ok_venues` 含 C |
| RSK-101.12 | 同 .11 的 g4;C 的杠杆报告不列 c1 | `ACCOUNT_CFG` 未置位 |
| RSK-101.13 | C 的报告 `uid_keys = SESSION`(缺 REST) | `IDENTITY` 未置位;`drain = None`;无 Kill cap |
| RSK-101.14 | C 的报告 `mismatches = UID \| UNREADABLE` | `IDENTITY` 未置位;`drain = None`;无 Kill cap |
| RSK-101.15 | 不投递 `KillSelfTest{1}` | `IDENTITY` 未置位 |
| RSK-101.16 | Sim 账户 3 复位后只投递 `Feeds`、sim-gw 的 `OrderConns`(A、B,`standby_up = true`)、`Unknown`、`Clock`,不投递 `KillSelfTest` | 复位那次 apply 的 `gate_changes` 为 `{3, 127, 104, false}`(`RECONCILED`、`ACCOUNT_CFG`、`IDENTITY` 直接置位);最后一次 apply `released = [3]` |
| RSK-101.17 | Sim 账户 3 复位后只投递 A 的 `OrderConns{true, true}` | 该 apply 的 `hedge_gate_open` 与 `venue_gate_open` 都为 `[(3, A)]`(模拟账户的子集只看连接) |

### RSK-102 venue 位图反映最近一次输入,放行后不再变假

- 规则:[启动闸门](README.md#启动闸门)(每次运行的复位、venue 位图、`released` 锁存)
- 级别:unit
- 里程碑:M2
- 前置:G0
- 步骤:
  1. 放行序列中把 C 的 `OrderConns` 先 `primary_up: true`、再 `primary_up: false`,其余照常
  2. 再投递 C 的 `OrderConns{true, true}`
  3. 放行之后投递 C 的 `OrderConns{false, true}`
- 期望:
  - 步骤 1 后 `ORDER_CONNS` 未置位,`released = false`
  - 步骤 2 的 apply:`released = [1]`
  - 步骤 3 的 apply:`gate_changes = [{1, 127, 125, released: true}]`;`mode.released(1)` 仍为 true;`hedge_gate_ok(1, C) = false`
- 实现:`crates/hft-risk/tests/gates.rs::rsk_102_bitmaps_track_latest`

### RSK-103 uid_policy = Unverifiable 的账户按报告与 kill 自检放行 IDENTITY 并写 IdentityUnverified

- 规则:[启动闸门](README.md#启动闸门)(`IDENTITY` 行、身份与配置不符)
- 级别:table
- 里程碑:M2
- 前置:G0,账户 1 `uid_policy = Unverifiable`(`is_prod = false`);放行序列中各 venue 的报告按行替换;读 `IDENTITY` 的 venue 位图与 `notes`
- 实现:`crates/hft-risk/tests/gates.rs::rsk_103_uid_unverifiable`

| 行 | 输入 | 期望 |
|---|---|---|
| RSK-103.1 | A 的报告 `mismatches = UNREADABLE`、`uid_keys = 0`,kill 自检已通过 | A 进入 `identity_ok_venues`;该 apply 的 `notes` 含一条 `IdentityUnverified`(P1,`v[0] = 258`,即 venue 2、account 1 << 8) |
| RSK-103.2 | .1 之后同样的报告再来一条 | 无新的 `IdentityUnverified` |
| RSK-103.3 | C 从未收到报告 | C 不在 `identity_ok_venues`,`IDENTITY` 未置位 |
| RSK-103.4 | C 的报告 `mismatches = 0`、`uid_keys = SESSION \| REST` | C 置位,`notes` 含 `IdentityUnverified`(`v[0] = 261`) |
| RSK-103.5 | B 的报告 `mismatches = UID`(无 `UNREADABLE`) | 按 UID 不符:`Auto(Kill{Account(1), hard})`、`drain = Some(IdentityMismatch{1, B})`,B 不置位 |
| RSK-103.6 | 报告都到、kill 自检未通过;随后 `KillSelfTest{1}` | 无 venue 置位、无注记;`KillSelfTest` 的 apply 中三个 venue 置位、三条 `IdentityUnverified` |

### RSK-104 UID 不符在放行前后都设账户 HARD 并要求 Drain

- 规则:[启动闸门](README.md#启动闸门)(身份与配置不符)、[cap 记录](README.md#cap-记录)(Kill 行)
- 级别:table
- 里程碑:M2
- 前置:读 UID 不符那次 apply 的 `ModeOutcome`
- 实现:`crates/hft-risk/tests/gates.rs::rsk_104_uid_mismatch_hard_and_drain`

| 行 | 时机 | 期望 |
|---|---|---|
| RSK-104.1 | G0 放行序列中 C 的报告 `mismatches = UID` | cap 集合新增 `Kill{Account(1)}` 带 `cflags.HARD`(`CapSet` P1);`kill_changed = true`,gw0、gw1、gw2 的 `kill_bits` 含 `account(0)` 与 `account_hard(0)`;`cancels` 恰一项账户 1、`open_only = false`;`drain = Some(IdentityMismatch{account 1, venue C})`;C 不在 `identity_ok_venues` |
| RSK-104.2 | R0(已放行)中 C 的报告 `mismatches = UID` | 同 .1;此外 `gate_changes = [{1, 127, 63, released: true}]` |
| RSK-104.3 | R0 中 C 的报告 `mismatches = UID \| UNREADABLE` | 无 Kill、`drain = None`;C 移出 `identity_ok_venues` |

### RSK-105 放行后配置不符设 AccountCfg cap,恢复一致后自动删除

- 规则:[启动闸门](README.md#启动闸门)(配置不符)、[cap 记录](README.md#cap-记录)(AccountCfg 行)
- 级别:unit
- 里程碑:M2
- 前置:R0(已放行)
- 步骤:
  1. B 的报告 `mismatches = MARGIN_MODE`
  2. B 的报告 `mismatches = 0`
  3. A 的报告 `mismatches = AUTO_BORROW`
- 期望:
  - 步骤 1:`AccountCfg{1, B}` 设置,`params[0] = MISMATCH`,`CapSet`(P1);`gate_changes = [{1, 127, 95, released: true}]`;`Open(1, g1, b1, Buy, 0.01 @ 50 000.0)` 拒 `CLOSE_ONLY`、a1 的 Open 放行
  - 步骤 2:`AccountCfg{1, B}` 删除,`CapCleared`(Info)
  - 步骤 3:`AccountCfg{1, A}` 设置(`MISMATCH`)
- 实现:`crates/hft-risk/tests/gates.rs::rsk_105_account_cfg_after_release`

### RSK-106 对冲闸门子集不要求行情、对账、杠杆与热备

- 规则:[启动闸门](README.md#启动闸门)(对冲闸门子集)
- 级别:table
- 里程碑:M2
- 前置:G0;不投递 `Feeds` 与任何对账组;每行投递所列输入,读最后一次 apply 的 `hedge_gate_open` 与 `hedge_gate_ok`
- 实现:`crates/hft-risk/tests/gates.rs::rsk_106_hedge_gate_subset`

| 行 | 输入 | 期望 |
|---|---|---|
| RSK-106.1 | `KillSelfTest{1}`、A 的 `AccountCfg`(一致)、A 的 `OrderConns{true, true}` | 最后一次 apply 的 `hedge_gate_open = [(1, A)]`;`released` 为空 |
| RSK-106.2 | 同上,但 A 的 `OrderConns{true, standby_up: false}` | `[(1, A)]`;`ORDER_CONNS` 未置位 |
| RSK-106.3 | `KillSelfTest`、B 的 `on_account_cfg`(b1 杠杆 3.01)与 `AccountCfg`(一致)、B 的 `OrderConns{true, true}` | `[(1, B)]`;`ACCOUNT_CFG` 未置位 |
| RSK-106.4 | 只 A 满足,C 的 `OrderConns{false, true}` | `hedge_gate_ok(1, C) = false`,C 从未列出 |
| RSK-106.5 | .1 之后 A 的 `OrderConns{false, true}`,再 `{true, true}` | 断开时不列、`hedge_gate_ok(1, A) = false`;恢复那次 apply 再列 `[(1, A)]` |
| RSK-106.6 | 不投递 `KillSelfTest`,其余同 .1 | 不列;`hedge_gate_ok(1, A) = false` |
| RSK-106.7 | A 的报告 `mismatches = POS_MODE` | 不列 |
| RSK-106.8 | A 的报告 `mismatches = UID \| UNREADABLE` | 不列(`identity_ok_venues` 不含 A) |

### RSK-107 恢复期按 venue 放行 = 对冲闸门子集 ∧ 该 venue 已对账

- 规则:[启动闸门](README.md#启动闸门)(恢复期按 venue 放行)
- 级别:table
- 里程碑:M2
- 前置:G0;先投递 `KillSelfTest{1}` 与 A、B、C 的 `AccountCfg`(一致)与 `OrderConns{true, true}`(三个 venue 的 `hedge_gate_ok` 为真);不投递 `Feeds`
- 实现:`crates/hft-risk/tests/gates.rs::rsk_107_venue_gate`

| 行 | 输入 | 期望 |
|---|---|---|
| RSK-107.1 | 无 | `venue_gate_ok(1, B) = false`;`venue_gate_open` 从未列出 B |
| RSK-107.2 | B 的对账组全部 Ignore | 该 apply 的 `venue_gate_open = [(1, B)]`;`released` 为空(账户未放行) |
| RSK-107.3 | .2 之后 C 的 `OrderConns{false, true}` | `venue_gate_ok(1, B)` 仍为 true;无新列出 |
| RSK-107.4 | .2 之后 B 的 `OrderConns{false, true}`,再 `{true, true}` | 断开时 `venue_gate_ok(1, B) = false`;恢复那次 apply `venue_gate_open = [(1, B)]` |
| RSK-107.5 | B 的对账组含一条未处置 Warn | 不列;处置该条目那次 apply `venue_gate_open = [(1, B)]` |

### RSK-108 一次 Clock 输入同时放行多个账户时每个账户只列一次

- 规则:[启动闸门](README.md#启动闸门)(`gate_changes`)、[mode 模块](README.md#mode-模块唯一的模式状态)(容量)
- 级别:unit
- 里程碑:M2
- 前置:R0;账户 1 与 2 都做 G0 的复位,各自投递除 `Clock` 外的放行序列
- 步骤:
  1. `apply(Gate(Clock{ok: true}))`
- 期望:
  - `released = [1, 2]`;`gate_changes` 恰两项 `{1, 111, 127, true}`、`{2, 111, 127, true}`;账户 3 不列出(位不变)
- 实现:`crates/hft-risk/tests/gates.rs::rsk_108_clock_releases_many`

### RSK-109 闸门复位清空本次运行的闸门状态

- 规则:[启动闸门](README.md#启动闸门)(每次运行的复位)、[对账分级](README.md#对账分级全系统唯一一次)第 7 条(`us_up_seq`)
- 级别:unit
- 里程碑:M2
- 前置:R0(账户 1 已放行;B 的对账组记录 `report_id 5`;`us_up_seq(1, B) = 40`)
- 步骤:
  1. `apply(RecoveryFlags{1, flags = 0})`
  2. `apply(Gate(Disposed{1, B, report_id 5, …, Adopted}))`
- 期望:
  - 步骤 1:`gate_changes = [{1, 127, 0, released: false}]`;`hedge_gate_ok(1, ·)` 与 `venue_gate_ok(1, ·)` 都为 false;节 17 中账户 1 的 `ReconGate` 行不存在,账户 1 在 B 的 `RiskUsRow.us_up_seq = 0`;账户 2 的位与行不变
  - 步骤 2:`RECONCILED` 不置位(旧组已清空)
- 实现:`crates/hft-risk/tests/gates.rs::rsk_109_per_run_reset`

### RSK-130 对账组满足严格条件时 RECONCILED 逐 venue 置位

- 规则:[对账闸门与 HARD 解除的严格条件](README.md#对账闸门与-hard-解除的严格条件)
- 级别:table
- 里程碑:M2
- 前置:G0 投递除 B 的对账组以外的全部放行序列;每行投递 B 的一组报告(`report_id 7`)与所列处置,读 `reconciled_venues` 是否含 B
- 实现:`crates/hft-risk/tests/recon_gate.rs::rsk_130_reconciled_strict`

| 行 | B 的报告组与处置 | 期望 |
|---|---|---|
| RSK-130.1 | `Summary{sflags 0, skipped_inflight 0}`,其余条目全部 Ignore | 含 B;`released = [1]` |
| RSK-130.2 | 一条 Warn 仓位条目 b1 | 不含;`Disposed{1, B, 7, (Position, b1), Adopted}` 后含 |
| RSK-130.3 | 一条 Warn 余额条目 USDT | 不含;`Disposed{…, (Balance, USDT), Baselined}` 后含 |
| RSK-130.4 | 一条带 `AUTO_EXTERNAL` 的仓位条目 b1 | `Disposed{…, AutoExternal}` 后含 |
| RSK-130.5 | 两条 Warn(b1、b2),只处置 b1 | 不含;再处置 b2 后含 |
| RSK-130.6 | 一条 Warn b1,处置引用 `report_id 6` | 不含 |
| RSK-130.7 | `Summary.sflags = TRUNCATED`,其余 Ignore | 不含 |
| RSK-130.8 | `Summary.sflags = USERSTREAM_DOWN`,其余 Ignore | 不含 |
| RSK-130.9 | `Summary.skipped_inflight = 1`,其余 Ignore | 不含 |
| RSK-130.10 | 一条孤儿挂单条目(无处置方式);下一组(`report_id 8`)全部 Ignore | 不含;下一组后含 |
| RSK-130.11 | 只有 `Funding` 条目(含 `inst = INST_NONE` 的钱包条目)差异 | 含 |
| RSK-130.12 | .1 之后新一组 `report_id 8` 含一条 Warn | B 移出;此前对 `report_id 7` 的处置作废 |
| RSK-130.13 | 64 条 Warn,全部处置 | 含 |
| RSK-130.14 | 65 条 Warn(`OVERFLOW`),处置其中 64 条 | 不含;下一组全部 Ignore 后含 |

### RSK-131 HARD 解除要求 kill 之后每个 venue 的最近一组报告满足严格条件

- 规则:[对账闸门与 HARD 解除的严格条件](README.md#对账闸门与-hard-解除的严格条件)(HARD 解除)、[cap 记录](README.md#cap-记录)(Kill 行)
- 级别:table
- 里程碑:M2
- 前置:R0;`apply(Kill{Account(1), hard}, seq = S = 10 000)`;每行再投递所列对账组,然后 `apply(Resume{Kill, Account(1)})`
- 实现:`crates/hft-risk/tests/recon_gate.rs::rsk_131_hard_release_recon`

| 行 | A、B、C 的最近一组 | 期望 |
|---|---|---|
| RSK-131.1 | kill 之后没有新报告 | `Err(ReconNotClean)`;Kill 仍在 |
| RSK-131.2 | 三组 `view_seq = 10 001`、全部 Ignore | `Ok`;`resolve_broken = true`;`kill_changed = true`,三个 gw 的账户 1 位清零 |
| RSK-131.3 | A、B 同 .2,C 的最近一组 `view_seq = 9 999` | `Err(ReconNotClean)` |
| RSK-131.4 | C 的 `view_seq = 10 000`;`= 10 001` | `Err(ReconNotClean)`;`Ok` |
| RSK-131.5 | C 一组含 Warn b1,已 `Adopted` 处置 | `Ok` |
| RSK-131.6 | C 一组只有 `Funding` 差异 | `Ok` |
| RSK-131.7 | C 先一组 Ignore(`view_seq 10 001`),再一组含未处置 Warn(`view_seq 10 050`) | `Err(ReconNotClean)`(以新组为准) |
| RSK-131.8 | `Kill{Global, hard}`;账户 1 的 A、B、C 与账户 2 的 A、B 都有合格组,账户 3(Sim)无报告 | `Resume{Kill, Global}` 返回 `Ok` |
| RSK-131.9 | 非 HARD 的 `Kill{Account(1)}`,无任何报告 | `Resume{Kill, Account(1)}` 返回 `Ok` |

### RSK-140 kill 字由剩余 Kill 记录重算

- 规则:[kill 字](README.md#kill-字唯一的跨线程共享可变量)、[Kill](README.md#kill)(kill 字位列)
- 级别:table
- 里程碑:M2
- 前置:R0(gw0 = A、gw1 = B、gw2 = C;A、B 上账户 1 → ix 0、账户 2 → ix 1;C 上账户 1 → ix 0);每行从 R0 apply 所列输入,读 `kill_bits()`(十六进制)
- 实现:`crates/hft-risk/tests/mode.rs::rsk_140_kill_bits_projection`

| 行 | 输入 | gw0 / gw1 / gw2 | `kill_changed` |
|---|---|---|---|
| RSK-140.1 | 无 | 0 / 0 / 0 | — |
| RSK-140.2 | `Kill{Global}` | 0x1 / 0x1 / 0x1 | true |
| RSK-140.3 | `Kill{Global, hard}` | 0x5 / 0x5 / 0x5 | true |
| RSK-140.4 | `Kill{Account(1)}` | 0x100 / 0x100 / 0x100 | true |
| RSK-140.5 | `Kill{Account(2)}` | 0x200 / 0x200 / 0 | true |
| RSK-140.6 | `Kill{Account(1), hard}` | 0x0010_0100 / 0x0010_0100 / 0x0010_0100 | true |
| RSK-140.7 | `Kill{Account(2), hard}` | 0x0020_0200 / 0x0020_0200 / 0 | true |
| RSK-140.8 | `Kill{Venue(B)}`(account = *) | 0 / 0x2 / 0 | true |
| RSK-140.9 | `Kill{Venue(B) × 2}` | 0 / 0x200 / 0 | true |
| RSK-140.10 | `Kill{Account(3)}`(Sim) | 0 / 0 / 0 | false |
| RSK-140.11 | `Kill{Global}` + `Kill{Account(1), hard}` | 0x0010_0101 / 0x0010_0101 / 0x0010_0101 | — |
| RSK-140.12 | .11 之后 `Resume{Kill, Global}` | 0x0010_0100 / 0x0010_0100 / 0x0010_0100 | true |
| RSK-140.13 | `DailyLoss{1}`、`MarginStress{1}`、`Storage`(非 Kill 来源) | 0 / 0 / 0 | false |

### RSK-141 Kill 的级别、撤单与参数组合

- 规则:[Kill](README.md#kill)、[mode 模块](README.md#mode-模块唯一的模式状态)(`ModeCancel.open_only`)
- 级别:table
- 里程碑:M2
- 前置:R0;读该 apply 的 `cancels`
- 实现:`crates/hft-risk/tests/mode.rs::rsk_141_kill_levels_and_cancels`

| 行 | 输入 | 期望 |
|---|---|---|
| RSK-141.1 | `Kill{Venue(B) × 1, cancel_resting: true}` | `cancels` 恰一项:(venue B, account 1),`open_only = true`;`cells` 为空 |
| RSK-141.2 | `Kill{Venue(B) × 1, cancel_resting: false}` | `cancels` 为空 |
| RSK-141.3 | `Kill{Venue(B) × 1, hard}` | `Err(InvalidScope)` |
| RSK-141.4 | `Kill{Account(1), cancel_resting: true}` | 恰一项:账户 1,`open_only = true`;`cells` 两项 → CloseOnly |
| RSK-141.5 | `Kill{Account(1), hard, cancel_resting: true}` | 恰一项:账户 1,`open_only = false` |
| RSK-141.6 | `Kill{Global, cancel_resting: true}`;`Kill{Global, hard, cancel_resting: true}` | 恰一项全局,`open_only = true`;恰一项全局,`open_only = false` |
| RSK-141.7 | `Kill{Global, flatten}` | `Err(InvalidScope)` |
| RSK-141.8 | `SetMode{(1, 1, g1), CloseOnly}` | `cancels` 恰一项:cell (1, 1, g1),`open_only = true` |
| RSK-141.9 | `Notice(Delisting{B, b1})` | `cancels` 恰一项:(venue B, inst b1),`open_only = true`;`cells` 为空 |
| RSK-141.10 | `Auto(Recon{1, B})`、`Auto(AccountCfg{1, B})`(两个子例) | `cancels` 为空 |

### RSK-142 清仓锁到删除该记录的 Resume{Kill} 为止

- 规则:[清仓锁](README.md#清仓锁flatten_lock)、[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#3)
- 级别:unit
- 里程碑:M2
- 前置:R0;b1 持仓 −0.02
- 步骤:
  1. `apply(Kill{Account(1), cancel_resting: true, flatten: true})`
  2. `apply(Kill{Account(1), cancel_resting: true})`
  3. 对账户 1:`evaluate` `Open(1, g1, a1, Buy, 0.01 @ 50 000.00)`、`Reduce(1, g1, b1, Buy, 0.01 @ 50 005.0)`、执行层 `Flatten`(同前,`reason = Hard`)、OMS 第二腿 `Hedge(1, g1, b1, Sell, 0.01 @ 49 995.0)`、执行层 `role = HedgeFix` 的 Hedge、L3 `Flatten(reason = MarginStress)`;对账户 2:`Open(2, g2, a1, Buy, 0.01 @ 50 000.00)`
  4. `apply(Resume{Kill, Account(1)})`
- 期望:
  - 步骤 1 后 `view(1).flatten_lock = true`,`view(2).flatten_lock = false`
  - 步骤 2 后 `view(1).flatten_lock = true`(合并保留 `FLATTEN` 位)
  - 步骤 3:账户 1 的 Open 拒 `KILLED`,其余五个放行;账户 2 的 Open 放行
  - 步骤 4:`Ok`;`view(1).flatten_lock = false`;`risk_events` 含 `(1, CapCleared{Kill})`;`notes` 含一条 `CapCleared`
- 实现:`crates/hft-risk/tests/mode.rs::rsk_142_flatten_lock`

**保证金样本简写**:`ms(rep, est, rr, er)` = `MarginSample{rep_liq_dist = rep, est_liq_dist = est, reported_ratio = rr, est_ratio = er, report_ens = now.engine_ns, mark_ens = now.engine_ns, has_report = true, truncated = false}`(有永续持仓、数据新鲜);`liq` 单写一个值时 `rep = est = liq`、`rr = er = 0.10`。保证金用例都对 (账户 1, venue B) 调 `on_margin`,读 `MarginOut` 与账户 1 在 B 的 `MarginRow`;阈值为 R0 的值(0.25 / 0.15 / 0.10,滞回 0.02,比率 0.80 / 0.05)。

### RSK-160 档位按强平距离与比率交叉校验上行

- 规则:[保证金分档](README.md#保证金分档marginstress)(度量、交叉校验、档位表)
- 级别:table
- 里程碑:M2
- 前置:R0;每行从 L0 调一次 `on_margin(1, B, 样本, now = T0)`
- 实现:`crates/hft-risk/tests/margin.rs::rsk_160_level_by_distance`

| 行 | 样本 | 期望档位 |
|---|---|---|
| RSK-160.1 | liq 0.250000001 | L0 |
| RSK-160.2 | liq 0.25 | L1 |
| RSK-160.3 | liq 0.249999999 | L1 |
| RSK-160.4 | liq 0.150000001 | L1 |
| RSK-160.5 | liq 0.15 | L2 |
| RSK-160.6 | liq 0.100000001 | L2 |
| RSK-160.7 | liq 0.10 | L3 |
| RSK-160.8 | liq 0.099999999 | L3 |
| RSK-160.9 | `ms(0.30, 0.12, 0.10, 0.10)` | L2(取更保守者) |
| RSK-160.10 | `ms(0.12, 0.30, 0.10, 0.10)` | L2 |
| RSK-160.11 | `ms(Rate::MAX, 0.20, 0.10, 0.10)`(该持仓无 `liq_px`) | L1(只用估计) |
| RSK-160.12 | `ms(Rate::MAX, Rate::MAX, 0, 0)`、`mark_ens = i64::MAX`(无永续持仓) | L0 |
| RSK-160.13 | `ms(0.30, 0.30, 0.80, 0.10)` | L2(比率触发,`mflags` 含 `RATIO_L2`) |
| RSK-160.14 | `ms(0.30, 0.30, 0.799999999, 0.80)` | L2(取两比率的大者) |
| RSK-160.15 | `ms(0.30, 0.30, 0.799999999, 0.799999999)` | L0 |
| RSK-160.16 | `ms(0.30, 0.30, Rate::MAX, 0.10)`(保证金余额 ≤ 0) | L2 |

### RSK-161 进入 L1、L2、L3 时的输出

- 规则:[保证金分档](README.md#保证金分档marginstress)(档位表、进入 L2、`Note{MarginLevel}`、账户档位与 cap)、[保证金分档的输入与输出](README.md#保证金分档的输入与输出)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. `on_margin(1, B, liq 0.25)` 于 T0
  2. `on_margin(1, B, liq 0.15)` 于 T0 + 1 s
  3. `on_margin(1, B, liq 0.10)` 于 T0 + 2 s
- 期望:
  - 步骤 1:`mode = [Auto(MarginStress{account 1, level 1, params[0] = 8})]`;`events = [(1, MarginStress{venue B, level L1, liq_dist 0.25, ratio 0.10, …})]`;`force = None`;`notes` 为空
  - 步骤 2:`mode = [Auto(MarginStress{level 2, params[0] = 8})]`;`events = [(1, MarginStress{B, L2, 0.15, 0.10, deadline_tsc = tsc_of(T0 + 31 s)})]`;`timers` 含 `(MarginDeadline{1, B}, T0 + 31 s)`;`notes = [MarginLevel(P1, v = [769, 2, 0.15, 0.10, T0 + 31 s])]`(769 = 账户 1、venue 3 << 8);`MarginRow.deadline_ens = T0 + 31 s`、`l2_since_ens = T0 + 1 s`
  - 步骤 3:`mode` 含 `level 3`;`events` 的 level 为 L3;`force = Some(ForceDeleverage{account 1, venue B, frac 0.20})`;`timers` 含 `(MarginForce{1, B}, T0 + 7 s)`;`notes = [MarginLevel(P0, level 3)]`;`MarginRow.next_force_ens = T0 + 7 s`
- 实现:`crates/hft-risk/tests/margin.rs::rsk_161_level_entry_outputs`

### RSK-162 L3 每 force_step_interval_ms 仍为 L3 则再出一步,离开 L3 即停

- 规则:[保证金分档](README.md#保证金分档marginstress)(L3 行)、[热路径:分类与检查](README.md#热路径分类与检查)(`on_timer`)
- 级别:unit
- 里程碑:M2
- 前置:R0;`on_margin(1, B, liq 0.10)` 于 T(进入 L3,下一步 T + 5 s)
- 步骤:
  1. `on_timer(MarginForce{1, B})` 于 T + 5 s
  2. `on_margin(1, B, liq 0.13)` 于 T + 8 s
  3. `on_timer(MarginForce{1, B})` 于 T + 10 s
- 期望:
  - 步骤 1:`force = Some(ForceDeleverage{1, B, 0.20})`;`timers` 含 `(MarginForce{1, B}, T + 10 s)`
  - 步骤 2:档位 L2(0.13 > 0.12);`deadline_ens = T + 38 s`;`timers` 含 `(MarginDeadline{1, B}, T + 38 s)`;`force = None`
  - 步骤 3:无 `force`、无新 `timers`、档位不变
- 实现:`crates/hft-risk/tests/margin.rs::rsk_162_force_steps`

### RSK-163 下行需数据新鲜且越过阈值加滞回,逐档下降

- 规则:[保证金分档](README.md#保证金分档marginstress)(滞回)
- 级别:table
- 里程碑:M2
- 前置:R0;每行先用第一列的样本把档位推到起点,再于 1 s 后调一次 `on_margin`
- 实现:`crates/hft-risk/tests/margin.rs::rsk_163_hysteresis`

| 行 | 起点 | 新样本 | 期望档位 |
|---|---|---|---|
| RSK-163.1 | L1(liq 0.25) | liq 0.27;liq 0.270000001 | L1;L0 |
| RSK-163.2 | L2(liq 0.15) | liq 0.17;liq 0.170000001 | L2;L1 |
| RSK-163.3 | L3(liq 0.10) | liq 0.12;liq 0.120000001 | L3;L2 |
| RSK-163.4 | L3(liq 0.10) | liq 0.18 | L1 |
| RSK-163.5 | L3(liq 0.10) | liq 0.30 | L0 |
| RSK-163.6 | L2(`ms(0.30, 0.30, 0.80, 0.10)`,比率触发) | `ms(0.30, 0.30, 0.75, 0.10)`;`ms(0.30, 0.30, 0.749999999, 0.10)` | L2;L0 |
| RSK-163.7 | L2(比率触发) | `ms(0.26, 0.26, 0.70, 0.10)` | L1 |
| RSK-163.8 | L1(liq 0.25) | liq 0.50,`report_ens = now − 15 001 ms` | L1(数据过期不降) |
| RSK-163.9 | L1(liq 0.25) | liq 0.10 | L3(上行即时;不经 L2,`deadline_ens` 不设) |

### RSK-164 L2 截止过期进入 L3,截止进入的 L3 要越过 reduce + hyst 才降

- 规则:[保证金分档](README.md#保证金分档marginstress)(L3 行、进入 L2、滞回)
- 级别:unit
- 里程碑:M2
- 前置:R0;`on_margin(1, B, liq 0.14)` 于 T(L2,`deadline_ens = T + 30 s`)
- 步骤:
  1. `on_timer(MarginDeadline{1, B})` 于 T + 29 999 999 999 ns(早到)
  2. `on_timer(MarginDeadline{1, B})` 于 T + 30 s
  3. `on_margin(1, B, liq 0.16)` 于 T + 31 s
  4. `on_margin(1, B, liq 0.170000001)` 于 T + 32 s
- 期望:
  - 步骤 1:档位 L2;`timers = [(MarginDeadline{1, B}, T + 30 s)]`;无 `force`
  - 步骤 2:档位 L3,`mflags` 含 `DEADLINE_L3`;`force = Some(…frac 0.20)`;`timers` 含 `(MarginForce{1, B}, T + 35 s)`;`notes` 含 `MarginLevel`(P0)
  - 步骤 3:仍 L3(0.16 ≤ 0.17)
  - 步骤 4:L1(离开 L3 与 L2,0.170000001 ≤ 0.27 停在 L1)
- 实现:`crates/hft-risk/tests/margin.rs::rsk_164_deadline_l3`

### RSK-165 on_margin 发现 L2 截止已过同样进入 L3

- 规则:[保证金分档](README.md#保证金分档marginstress)(进入 L2 一条)
- 级别:unit
- 里程碑:M2
- 前置:R0;`on_margin(1, B, liq 0.14)` 于 T
- 步骤:
  1. `on_margin(1, B, liq 0.14)` 于 T + 30 s − 1 ns
  2. `on_margin(1, B, liq 0.14)` 于 T + 30 s
- 期望:
  - 步骤 1:L2,无 `force`
  - 步骤 2:L3(`DEADLINE_L3`),`force = Some(…)`
- 实现:`crates/hft-risk/tests/margin.rs::rsk_165_deadline_on_margin`

### RSK-166 数据过期或不完整至少 L1

- 规则:[保证金分档](README.md#保证金分档marginstress)(数据过期或不完整)
- 级别:table
- 里程碑:M2
- 前置:R0(`margin_stale_ms = 15 000`、`mark_stale_ms = 10 000`);每行从 L0 调一次 `on_margin(1, B, …)`,`now = T0`;未写的字段同 `ms(0.50, 0.50, 0.10, 0.10)`
- 实现:`crates/hft-risk/tests/margin.rs::rsk_166_stale_data_l1`

| 行 | 样本 | 期望档位 |
|---|---|---|
| RSK-166.1 | `has_report = false` | L1 |
| RSK-166.2 | `set_run_start(T0 − 10 s)`,`report_ens = T0 − 10 s − 1 ns` | L1 |
| RSK-166.3 | `report_ens = T0 − 15 000 ms` | L0 |
| RSK-166.4 | `report_ens = T0 − 15 000 ms − 1 ns` | L1 |
| RSK-166.5 | `truncated = true` | L1 |
| RSK-166.6 | `mark_ens = T0 − 10 000 ms`;`T0 − 10 000 ms − 1 ns` | L0;L1 |
| RSK-166.7 | `set_run_start(T0 − 1 s)`,`mark_ens = T0 − 1 s − 1 ns` | L1 |
| RSK-166.8 | `rep = est = Rate::MAX`,`mark_ens = i64::MAX`(无永续持仓),报告新鲜 | L0 |
| RSK-166.9 | `truncated = true` 且 liq 0.10 | L3 |

### RSK-167 MarginStale 定时器判定停止到达的数据

- 规则:[保证金分档](README.md#保证金分档marginstress)(数据过期:`Timer{Risk, MarginStale}`)
- 级别:unit
- 里程碑:M2
- 前置:R0;(1, B) 堆中无 `MarginStale`
- 步骤:
  1. `on_margin(1, B, ms(0.50, 0.50, 0.10, 0.10) 但 mark_ens = T − 5 s)` 于 T
  2. 于 T + 1 s 再调一次同样内容的 `on_margin`(`report_ens = T + 1 s`,`mark_ens = T − 5 s`)
  3. `on_timer(MarginStale{1, B})` 于 T + 5 s + 1 ns
  4. `on_margin(1, B, liq 0.50,mark_ens = T + 6 s)` 于 T + 6 s
- 期望:
  - 步骤 1:`timers = [(MarginStale{1, B}, T + 5 s + 1 ns)]`(min(T + 15 s, T + 5 s) + 1 ns)
  - 步骤 2:`timers` 为空(每 (account, venue) 至多一个在堆)
  - 步骤 3:档位 L1(标记价断流);`mode = [Auto(MarginStress{level 1})]`;`events` 一项 L1
  - 步骤 4:档位 L0;`mode = [AutoClear{MarginStress, account 1}]`;`events` 一项 `MarginStress{B, None, …}`;`timers` 含新的 `(MarginStale{1, B}, T + 16 s + 1 ns)`(min(T + 21 s, T + 16 s) + 1 ns)
- 实现:`crates/hft-risk/tests/margin.rs::rsk_167_stale_timer`

### RSK-168 账户档位取各 venue 最大值,cap 记录 venue 位图,回到 L0 删除 cap

- 规则:[保证金分档](README.md#保证金分档marginstress)(账户档位与 cap、只对永续 venue 判定)、[热路径:分类与检查](README.md#热路径分类与检查)(`view`)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. `on_margin(1, B, liq 0.15)`;`on_margin(1, C, liq 0.25)`
  2. `on_margin(1, B, liq 0.50)`
  3. `on_margin(1, C, liq 0.50)`
  4. `on_margin(1, A, …)`(A 为现货 venue)
- 期望:
  - 步骤 1 后 cap `MarginStress{1}` 的 `level = 2`、`params[0] = 40`(venue 3 与 5 的位);`view(1)`:`margin_level = L2`、`liq_dist = 0.15`、`deadline_tsc = tsc_of(B 的 deadline_ens)`、`open_blocked = true`
  - 步骤 2:`mode = [Auto(MarginStress{level 1, params[0] = 32})]`;`events = [(1, MarginStress{B, None, …})]`
  - 步骤 3:`mode = [AutoClear{MarginStress, account 1}]`;`events = [(1, MarginStress{C, None, …})]`;随后 apply 的 `risk_events` 含 `(1, CapCleared{MarginStress})`
  - 步骤 4:`MarginOut` 全空,无 (1, A) 的 `MarginRow`
- 实现:`crates/hft-risk/tests/margin.rs::rsk_168_account_level_and_bitmap`

### RSK-169 重启跨越 L2 截止:engine_ns 连续、早到的定时器重登记、L3 出步等本次运行数据

- 规则:[保证金分档](README.md#保证金分档marginstress)(判定只读 engine_ns、重启后的 L3 出步要本次运行的数据确认)
- 级别:unit
- 里程碑:M2
- 前置:R0;`on_margin(1, B, liq 0.14)` 于 T(L2,`deadline_ens = T + 30 s`)
- 步骤:
  1. 于 T + 10 s `snapshot`;`restore`(新运行 TSC 从 0 计,停机 10 min 不计入 engine_ns),`set_run_start(T + 10 s)`
  2. `on_timer(MarginDeadline{1, B})` 于 T + 29 999 999 999 ns
  3. `on_timer(MarginDeadline{1, B})` 于 T + 30 s
  4. `on_margin(1, B, liq 0.14,report_ens = T + 31 s,mark_ens = T + 31 s)` 于 T + 31 s
- 期望:
  - 步骤 1 后账户 1 在 B 的 `MarginRow` 中 `level = 2`、`deadline_ens = T + 30 s`(截止剩余 20 s,与停机前相同)
  - 步骤 2:`timers = [(MarginDeadline{1, B}, T + 30 s)]`,档位 L2
  - 步骤 3:档位 L3(`DEADLINE_L3`);`force = None`、无 `MarginForce`(报告与标记价都早于本次运行起点)
  - 步骤 4:仍 L3;`force = Some(ForceDeleverage{1, B, 0.20})`;`timers` 含 `(MarginForce{1, B}, T + 36 s)`
- 实现:`crates/hft-risk/tests/margin.rs::rsk_169_restart_across_deadline`

### RSK-170 快照里的 L3 在本次运行的数据确认前不出步

- 规则:[保证金分档](README.md#保证金分档marginstress)(重启后的 L3 出步要本次运行的数据确认)
- 级别:table
- 里程碑:M2
- 前置:R0;`on_margin(1, B, liq 0.10)` 于 T(L3,下一步 T + 5 s);于 T + 1 s 快照并 `restore`,`set_run_start(T + 1 s)`
- 实现:`crates/hft-risk/tests/margin.rs::rsk_170_restart_in_l3`

| 行 | 之后的输入 | 期望 |
|---|---|---|
| RSK-170.1 | `on_timer(MarginForce{1, B})` 于 T + 5 s | `force = None`;`timers` 为空;档位 L3 |
| RSK-170.2 | .1 之后 `on_margin(liq 0.10,report_ens = mark_ens = T + 6 s)` 于 T + 6 s | `force = Some(…0.20)`;`timers` 含 `(MarginForce, T + 11 s)` |
| RSK-170.3 | .1 之后 `on_margin(liq 0.13,数据新鲜)` 于 T + 6 s | L2;`force = None`;`deadline_ens = T + 36 s` |
| RSK-170.4 | .1 之后 `on_margin(liq 0.10,report_ens = T − 1 s,mark_ens = T + 6 s)` 于 T + 6 s | `force = Some(…)`(新标记价即确认) |
| RSK-170.5 | .1 之后 `on_margin(liq 0.30,数据新鲜)` 于 T + 6 s | L0;`force = None` |

### RSK-171 按距离分档示例:单个永续空头 e = 0.30、m = 0.5%

- 规则:[保证金分档](README.md#保证金分档marginstress)(为什么按距离)
- 级别:table
- 里程碑:M2
- 前置:R0;`est_liq_dist` 由 `tests/model/ledger_liq.rs` 按 `(0.295 − 1.005·y) / (1.005·(1 + y))` 算出(Rate,向零取整),`rep_liq_dist = Rate::MAX`,比率 0.10;每行从 L0 调一次 `on_margin`
- 实现:`crates/hft-risk/tests/margin.rs::rsk_171_distance_example`

| 行 | 单边涨幅 y | `est_liq_dist`(约) | 期望档位 |
|---|---|---|---|
| RSK-171.1 | 0.034 | 0.2510 | L0 |
| RSK-171.2 | 0.035 | 0.2498 | L1 |
| RSK-171.3 | 0.124 | 0.1508 | L1 |
| RSK-171.4 | 0.125 | 0.1498 | L2 |
| RSK-171.5 | 0.175 | 0.1009 | L2 |
| RSK-171.6 | 0.176 | 0.0999 | L3 |

### RSK-172 保证金判定可回放,MarginOut 不超过构造容量

- 规则:[保证金分档](README.md#保证金分档marginstress)(判定只读 Input 推出的 engine_ns)、[保证金分档的输入与输出](README.md#保证金分档的输入与输出)(容量)
- 级别:prop
- 里程碑:M2
- 性质:同一 `on_margin`/`on_timer` 序列在两个独立的 `Risk` 上产生逐字节相同的 `MarginOut` 序列与 `MarginRow`;在任一中间点 `snapshot` → `restore`(`set_run_start` 用同一值)后继续,结果与不中断时相同;任何一次调用的 `mode`、`events`、`timers`、`notes` 各不超过 4 项
- 生成:200 步,每步为 `on_margin`(两种距离 0.05..0.40 或 `Rate::MAX`、两种比率 0..1、`report_ens`/`mark_ens` 在 now − 20 s..now、`truncated` 5%、`has_report` 95%)或到期的 `on_timer`;now 单调递增 0..10 s 步长;venue ∈ {B, C}
- 参考:两实例比较 + 容量断言
- 例数:PR 1 000
- 实现:`crates/hft-risk/tests/margin.rs::rsk_172_margin_replay_deterministic`

### RSK-180 交易中的永续 inst 杠杆未知或超限即禁开该 (account, venue)

- 规则:[永续杠杆上限](README.md#永续杠杆上限)、[热路径:分类与检查](README.md#热路径分类与检查)(`on_account_cfg`、`leverage`、`lev_bad`)
- 级别:table
- 里程碑:M2
- 前置:R0(已放行;账户 1 交易中的永续 inst:B 上 b1、b2,C 上无);每行对 (1, B) 或 (1, C) 先 `on_account_cfg(hdr, 行中杠杆)` 再 `apply(Gate(AccountCfg{hdr, …}))`,`hdr.mismatches = 0`
- 实现:`crates/hft-risk/tests/leverage.rs::rsk_180_leverage_cap`

| 行 | 报告 | 期望 |
|---|---|---|
| RSK-180.1 | B:b1 3.00、b2 3.00 | `lev_bad(1, B) = false`;无 cap |
| RSK-180.2 | B:b1 3.01、b2 3.00 | `lev_bad = true`;`Auto(AccountCfg{1, B})`,`params[0] = LEVERAGE`,`CapSet`(P1);`Open(1, g1, b1, …)` 与 `Open(1, g3, b2, …)` 拒 `CLOSE_ONLY`,a1 的 Open 放行 |
| RSK-180.3 | B:b1 `leverage = 0`、b2 3.00 | 同 .2 |
| RSK-180.4 | B(非 `PARTIAL`):只列 b2 | 同 .2 |
| RSK-180.5 | B:`LEV_TRUNCATED`,只列 b2 | 同 .2 |
| RSK-180.6 | C:c1 20.00(c1 不被任何 group 引用) | `lev_bad(1, C) = false`;无 cap |
| RSK-180.7 | C:不列任何 inst | 无 cap |
| RSK-180.8 | .2 之后 B:b1 3.00、b2 3.00 | `AccountCfg{1, B}` 删除,`CapCleared` |
| RSK-180.9 | .2 之后 B 的报告 `PARTIAL`,只给 b1 3.00 | cap 删除(b2 保持 3.00) |
| RSK-180.10 | B:`PARTIAL`,只给 b2 5.00(b1 保持 3.00) | `lev_bad = true`,cap 设置 |
| RSK-180.11 | B:b1 3.01,且 `mismatches = MARGIN_MODE` | cap `params[0] = 3`(`MISMATCH` 与 `LEVERAGE`);只修好杠杆的下一条报告后 cap 仍在;两项都一致的报告后删除 |
| RSK-180.12 | B:b1 3.00、b2 3.00;`leverage(1, B, b1)`、`leverage(1, B, c1)` | `Some(3.00)`;`None` |

### RSK-181 SetParams 引入新的永续 inst 时立即重判杠杆

- 规则:[永续杠杆上限](README.md#永续杠杆上限)(交易中的永续 inst、判定)、[热路径:分类与检查](README.md#热路径分类与检查)(`on_groups_changed`)
- 级别:unit
- 里程碑:M2
- 前置:R0;最近的 C 报告给 c1 20.00
- 步骤:
  1. `on_groups_changed(1, 新 group 表:g1、g3 与 g5 = 0x0100_0003(腿 a1 + c1))`
  2. `apply(GroupsChanged{added: [(1, 1, g5)], removed: []})`
  3. `on_groups_changed(1, 只含 g1、g3 的表)`,`apply(GroupsChanged{removed: [(1, 1, g5)]})`
- 期望:
  - 步骤 1 后 `lev_bad(1, C) = true`
  - 步骤 2:`Auto` 设置 `AccountCfg{1, C}`(`LEVERAGE`),`CapSet`
  - 步骤 3:`lev_bad(1, C) = false`;`AccountCfg{1, C}` 删除
- 实现:`crates/hft-risk/tests/leverage.rs::rsk_181_groups_changed_rechecks`

### RSK-190 日亏额度取绝对值与权益比例的较小者,越线锁存

- 规则:[当日亏损](README.md#当日亏损锁存)(权益来源、额度、触发线)
- 级别:table
- 里程碑:M2
- 前置:R0(`daily_loss_abs = 5 000`、`daily_loss_kill = 0.05`);每行从 `on_day_roll(day = 100, …)` 开始,`EquityView` 按行给出;读 `on_pnl(1, day_pnl, eq)` 的返回
- 实现:`crates/hft-risk/tests/pnl.rs::rsk_190_daily_loss_quota`

| 行 | 日切时权益 | `on_pnl` 序列 | 期望 |
|---|---|---|---|
| RSK-190.1 | {200 000, valid} | −5 000;−5 000.000001 | `None`;`Some(Auto(DailyLoss{1, params[0] = −5 000}))` |
| RSK-190.2 | {80 000, valid}(额度 4 000) | −4 000;−4 000.000001 | `None`;`Some(…params[0] = −4 000)` |
| RSK-190.3 | {0, valid = false} | −5 000.000001(eq 仍无效) | `Some(…−5 000)`(额度取绝对值) |
| RSK-190.4 | {0, valid = false} | −3 000(eq = {80 000, valid},补取 `equity_sod`);−4 000;−4 000.000001 | `None`;`None`;`Some(…−4 000)` |
| RSK-190.5 | {200 000, valid} | −5 000.000001;−6 000;−7 000 | `Some`;`None`;`None`(锁存期间不再返回) |
| RSK-190.6 | {200 000, valid} | +1 000 | `None` |

### RSK-191 日切重取权益与额度、清零次数,但不清锁存

- 规则:[当日亏损](README.md#当日亏损锁存)(权益来源、额度、日切)
- 级别:unit
- 里程碑:M2
- 前置:R0;day 100:账户 1 越线锁存(`DailyLoss{1}` 已 apply);账户 2 当日已 `Resume{DailyLoss}` 一次,触发线 −7 000
- 步骤:
  1. `on_day_roll(101, 权益:账户 1 = 200 000、账户 2 = 60 000,都 valid)`
  2. `on_pnl(1, −1, …)`;`on_pnl(2, −3 000, …)`;`on_pnl(2, −3 000.000001, …)`
  3. `daily_loss_resumable(1)`、`daily_loss_resumable(2)`
- 期望:
  - 步骤 1 后 `DailyLoss{1}` 仍在;账户 2 的额度 = min(5 000, 3 000) = 3 000、触发线 −3 000
  - 步骤 2:`None`(账户 1 仍锁存);`None`;`Some(…params[0] = −3 000)`
  - 步骤 3:都为 true(`resumes_today = 0`)
- 备注:`on_day_roll` 只在 `day > last_day_rolled` 时被调用,重复与倒序的 `DayRoll` 由 hft-sequencer 过滤(其 TESTS.md 的 `Sys{DayRoll}` 用例)
- 实现:`crates/hft-risk/tests/pnl.rs::rsk_191_day_roll_keeps_latch`

### RSK-192 Resume{DailyLoss} 重设触发线为当前 day_pnl − 额度,当日次数用完即 ResumeLimit

- 规则:[当日亏损](README.md#当日亏损锁存)(解除)、[mode 模块](README.md#mode-模块唯一的模式状态)(`daily_loss_resumed`)
- 级别:unit
- 里程碑:M2
- 前置:R0;day 100 越线:`on_pnl(1, −5 500)` 返回的 `Auto(DailyLoss)` 已 apply;`daily_loss_max_resumes = 1`;ModeView 的 `daily_loss_resumable` 转调 `Risk::daily_loss_resumable`
- 步骤:
  1. `apply(Resume{DailyLoss, 1})`
  2. `on_daily_loss_resumed(1, −5 500)`
  3. `on_pnl(1, −10 500)`;`on_pnl(1, −10 500.000001)`,把后者的 `Auto` 交 `apply`
  4. `apply(Resume{DailyLoss, 1})`
  5. `on_day_roll(101, …)`,再 `apply(Resume{DailyLoss, 1})`
- 期望:
  - 步骤 1:`Ok`;`ModeOutcome.daily_loss_resumed = [1]`;cap 删除
  - 步骤 3:`None`;`Some(Auto(DailyLoss{1, params[0] = −10 500}))`
  - 步骤 4:`Err(ModeErr::ResumeLimit)`;cap 仍在
  - 步骤 5:`Ok`(次数清零)
- 实现:`crates/hft-risk/tests/pnl.rs::rsk_192_resume_daily_loss`

### RSK-193 daily_loss_resumable 的边界

- 规则:[当日亏损](README.md#当日亏损锁存)(解除)
- 级别:table
- 里程碑:M2
- 前置:R0;按行设 `daily_loss_max_resumes` 与当日已解除次数
- 实现:`crates/hft-risk/tests/pnl.rs::rsk_193_resumable_boundary`

| 行 | `daily_loss_max_resumes` / 已解除次数 | `daily_loss_resumable(1)` |
|---|---|---|
| RSK-193.1 | 1 / 0 | true |
| RSK-193.2 | 1 / 1 | false |
| RSK-193.3 | 2 / 1 | true |
| RSK-193.4 | 2 / 2 | false |
| RSK-193.5 | 0 / 0 | false |

### RSK-200 对账条目的分级

- 规则:[对账分级](README.md#对账分级全系统唯一一次)(第 3 条)
- 级别:table
- 里程碑:M2
- 前置:R0(b1 lot 0.001;USDT `AssetMeta.dust = 0.01`;`recon_closeonly_notional = 100`);每行对 (1, B) 依次调 `on_reconcile`(每组 `Summary{sflags 0, skipped_inflight 0}` 加所列条目),读最后一组的 `ReconVerdicts`;条目写 `(键, exch_qty − engine_qty, |diff|)`
- 实现:`crates/hft-risk/tests/recon.rs::rsk_200_grading`

| 行 | 各轮条目 | 期望(最后一轮) |
|---|---|---|
| RSK-200.1 | (b1, 0.0009, 45) | `n_warn = 0`;`recon = None`;`gate` 无条目 |
| RSK-200.2 | (b1, 0.001, 50) | `n_warn = 1`;`recon = None`;`gate` 含 `(Position, b1)` |
| RSK-200.3 | 余额 (USDT, 0.009999, 0.009999);(USDT, 0.01, 0.01) 两个子例 | `n_warn = 0`;`n_warn = 1` |
| RSK-200.4 | 两轮 (b1, 0.003, 150) | `recon = Some(Auto(Recon{1, B}))` |
| RSK-200.5 | (b1, 0.003, 150) 然后 (b1, 0.0035, 175) | `recon = Some(…)`(变化 0.0005 < lot) |
| RSK-200.6 | (b1, 0.003, 150) 然后 (b1, 0.004, 200) | `recon = None`,`n_warn = 1`(变化 = lot) |
| RSK-200.7 | (b1, 0.003, 150) 然后 (b1, −0.003, 150) | `recon = None` |
| RSK-200.8 | 两轮 (b1, 0.002, 99.999999) | `recon = None` |
| RSK-200.9 | 两轮 (b1, 0.002, 100) | `recon = Some(…)` |
| RSK-200.10 | 两轮余额 (USDT, 150, 150) | `recon = Some(…)` |
| RSK-200.11 | 两轮带 `AUTO_EXTERNAL` 的 (b1, −0.003, 150) | `recon = Some(…)`(同一规则) |
| RSK-200.12 | (b1, 0.003, 150),中间一轮全部 Ignore,再 (b1, 0.003, 150) | `recon = None`(不连续) |

### RSK-201 Funding 条目不分级、不进闸门、不打断连续 Ignore,余额条目不再二次修正

- 规则:[对账分级](README.md#对账分级全系统唯一一次)(第 2、6 条)
- 级别:table
- 里程碑:M2
- 前置:R0;(1, B) 上 `Recon{1, B}` 已设置且已有一轮全部 Ignore
- 实现:`crates/hft-risk/tests/recon.rs::rsk_201_funding_not_graded`

| 行 | 这一组报告 | 期望 |
|---|---|---|
| RSK-201.1 | 只有 `Funding` 条目(b1 与 `inst = INST_NONE` 钱包条目,`diff` 各 12 USDT) | `n_warn = 0`;`gate` 无条目;`recon_resumable(1, B) = true`(Ignore 连续两轮) |
| RSK-201.2 | 余额条目 USDT 的 `engine_qty` 已含同组 Funding 差额,`exch_qty − engine_qty = 0`,另有 Funding 条目 `diff = 12` | 余额条目 Ignore(本模块不再减去 Funding 的 `diff`);`recon = None` |
| RSK-201.3 | 余额条目差 12 USDT(reconciler 未修正的部分),Funding 条目 `diff = 12` | 余额条目 Warn(按报告值分级,不抵扣) |

### RSK-202 对账恢复条件:连续两轮 Ignore,在途轮不计入不清零,用户流断开清零

- 规则:[对账分级](README.md#对账分级全系统唯一一次)(第 4 条)、[cap 记录](README.md#cap-记录)(Recon 行)
- 级别:table
- 里程碑:M2
- 前置:R0;(1, B) 已有两轮相同差异、`Recon{1, B}` 已设置;每行之后调 `recon_resumable(1, B)`,最后 `apply(Resume{Recon, (1, B)})`
- 实现:`crates/hft-risk/tests/recon.rs::rsk_202_recon_recovery`

| 行 | 之后的轮次 | `recon_resumable` / Resume |
|---|---|---|
| RSK-202.1 | Ignore | false / `Err(ReconNotClean)` |
| RSK-202.2 | Ignore、Ignore | true / `Ok` |
| RSK-202.3 | Ignore、`skipped_inflight = 1` 的轮、Ignore | true / `Ok` |
| RSK-202.4 | Ignore、`skipped_inflight = 1` 的轮 | false / `Err(ReconNotClean)` |
| RSK-202.5 | Ignore、Warn、Ignore | false |
| RSK-202.6 | Ignore、`on_user_stream_down(1, B)`、Ignore | false;再一轮 Ignore 后 true |
| RSK-202.7 | Ignore、`Summary.sflags = USERSTREAM_DOWN` 的轮 | false |

### RSK-203 auto_external_ok 要求两组报告无用户流断开且用户流 Up 早于上一组视图点

- 规则:[对账分级](README.md#对账分级全系统唯一一次)(第 7 条)
- 级别:table
- 里程碑:M2
- 前置:R0;(1, B):seq 100 的 `Gate(OrderConns{1, B, primary_up: true})`(由假变真,`us_up_seq = 100`);组 G1(`view_seq 150`、`sflags 0`)经 `on_reconcile`;检查下一组 G2 时调 `auto_external_ok(1, B, G2.sflags)`
- 实现:`crates/hft-risk/tests/recon.rs::rsk_203_auto_external_ok`

| 行 | 改动 | 期望 |
|---|---|---|
| RSK-203.1 | 无,G2 `sflags = 0` | true |
| RSK-203.2 | G2 `sflags = USERSTREAM_DOWN` | false |
| RSK-203.3 | G1 `sflags = USERSTREAM_DOWN`(`PREV_US_DOWN` 置位) | false |
| RSK-203.4 | G1 之后 seq 160 `primary_up: false`、seq 170 `primary_up: true` | false(`us_up_seq = 170 > 150`) |
| RSK-203.5 | G1 `view_seq = 100` | false(相等不算早于) |
| RSK-203.6 | 从未 Up(无 seq 100 的输入) | false |
| RSK-203.7 | G1 之后 `RecoveryFlags{1, 0}`(闸门复位,`us_up_seq = 0`),seq 300 `primary_up: true`;先查一次;再处理 G3(`view_seq 350`)后查 | false;true |
| RSK-203.8 | G1 之后 `snapshot` → `restore` → `set_run_start` | true(与连续运行相同) |
| RSK-203.9 | 快照中没有 (1, B) 的 `RiskUsRow` | false |

### RSK-210 Clock{ok: false} 设条件型 ClockBad,Clock{ok: true} 自动删除

- 规则:[时钟](README.md#时钟)、[cap 记录](README.md#cap-记录)(ClockBad 行)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. `apply(Clock{ok: false})`
  2. `apply(Clock{ok: false})`
  3. `apply(Clock{ok: true})`
  4. `apply(Clock{ok: true})`
- 期望:
  - 步骤 1:cap `ClockBad`(`cflags = CONDITIONAL`);`CapSet`(P1);`cells` 四项 → CloseOnly;`gate_bits` 不变(`CLOCK` 位只由 `Gate(Clock)` 推进)
  - 步骤 2、4:无输出,`ctl_state_version` 不变
  - 步骤 3:`ClockBad` 删除,`CapCleared`(Info),`cells` 四项回 Live
- 实现:`crates/hft-risk/tests/mode.rs::rsk_210_clock_bad_cap`

### RSK-220 存储档位 1 禁开、档位 2 产出 StorageCritical,StorageOk 删除

- 规则:[存储降级](README.md#存储降级storage)、[cap 记录](README.md#cap-记录)(Storage 行)
- 级别:table
- 里程碑:M2
- 前置:R0(`engine_reduce_pending = false`);每行从 R0 依次 apply
- 实现:`crates/hft-risk/tests/storage.rs::rsk_220_storage_levels`

| 行 | 输入序列 | 期望(最后一次 apply) |
|---|---|---|
| RSK-220.1 | `Storage{PersistLag, 1}` | cap `Storage{params [1, 0]}`;`drain = None`;`Open(1, g1, a1, …)` 拒 `CLOSE_ONLY`,Hedge 放行 |
| RSK-220.2 | `Storage{PersistLag, 1}`、`Storage{DiskLow, 1}` | 记录变为 `params [1, 1]`,一条 `CapSet` |
| RSK-220.3 | `Storage{DiskLow, 2}` | `params [0, 2]`;`drain = Some(StorageCritical)` |
| RSK-220.4 | `Storage{PersistLag, 1}`、`Storage{Both, 0}`(`StorageOk`) | cap 删除,`CapCleared` |
| RSK-220.5 | 已有 `Draining` cap,再 `Storage{PersistLag, 2}` | `drain = None` |

### RSK-221 存在未到达的引擎减仓目标时推迟 StorageCritical,目标到达后的同一 Input 内产出一次

- 规则:[存储降级](README.md#存储降级storage)(推迟)、[mode 模块](README.md#mode-模块唯一的模式状态)(`StorageRecheck`、`drain_deferred`、`ModeView::engine_reduce_pending`)
- 级别:unit
- 里程碑:M2
- 前置:R0;`apply(Kill{Account(1), flatten})`(清仓锁);`engine_reduce_pending = true`
- 步骤:
  1. `apply(Storage{PersistLag, 2})`
  2. `apply(StorageRecheck)`
  3. 置 `engine_reduce_pending = false`(清仓目标已到达,清仓锁仍在),`apply(StorageRecheck)`
  4. `apply(Auto(Draining{cancel_resting 1}))`,再 `apply(StorageRecheck)`
- 期望:
  - 步骤 1:`drain = None`;`drain_deferred() = true`;`Storage` cap 在(禁开)
  - 步骤 2:`drain = None`;无 cap 变化,`ctl_state_version` 不变
  - 步骤 3:`drain = Some(StorageCritical)`
  - 步骤 4:`drain_deferred() = false`;`StorageRecheck` 的 `drain = None`(只产出一次)
- 实现:`crates/hft-risk/tests/storage.rs::rsk_221_deferred_until_targets_reached`

### RSK-222 推迟只看引擎目标,不看清仓锁与 MarginStress 档位

- 规则:[存储降级](README.md#存储降级storage)(推迟)
- 级别:table
- 里程碑:M2
- 前置:R0;每行建立所列状态后 `apply(Storage{DiskLow, 2})`
- 实现:`crates/hft-risk/tests/storage.rs::rsk_222_defer_condition_is_targets`

| 行 | 状态 | 期望 `drain` |
|---|---|---|
| RSK-222.1 | 清仓锁在,`engine_reduce_pending = false` | `Some(StorageCritical)` |
| RSK-222.2 | `MarginStress{1}` L3,`engine_reduce_pending = false` | `Some(StorageCritical)` |
| RSK-222.3 | 无锁、无 L3,`engine_reduce_pending = true`(HardAlign 对齐目标未到达) | `None` |
| RSK-222.4 | .3 之后一条无关的 `apply(Gate(OrderConns{2, A, true, true}))`,此时 `engine_reduce_pending = false` | 该 apply 的 `drain = Some(StorageCritical)` |

### RSK-223 Delisting 只挡该 inst 的 Open,公告撤回后解除,inst 退出 refdata 后 restore 删除

- 规则:[下架与公告](README.md#下架与公告venuenotice)、[cap 记录](README.md#cap-记录)(Delisting 行)
- 级别:unit
- 里程碑:M2
- 前置:R0
- 步骤:
  1. `apply(Notice(Delisting{B, b1, at_ns}))`
  2. `apply(Notice(FundingInterval{B, b1, …}))`;`apply(Notice(Maintenance{B, 0, …}))`
  3. `apply(Resume{Delisting, (B, b1)})`
  4. 重新设置 Delisting{B, b1},`snapshot`,用不含 b1 的 refdata(只留 a1、a2,id 保持稠密)`restore`
- 期望:
  - 步骤 1:`CapSet`(Info);`inst_blocked(B, b1) = true`;`cancels` 恰一项 (B, b1)、`open_only = true`;`cells` 为空
  - 步骤 2:两次 apply 都无输出、无 cap、`ctl_state_version` 不变
  - 步骤 3:`Ok`;`inst_blocked(B, b1) = false`;`CapCleared`
  - 步骤 4:restore 后 cap 集合无 Delisting
- 实现:`crates/hft-risk/tests/mode.rs::rsk_223_delisting`

### RSK-230 快照往返:一次写完节 4、14、17、18、23,restore 后再快照逐字节相同

- 规则:[快照行格式](README.md#快照行格式节-4171823草案实现时以代码与编译期断言为准)、[热路径:分类与检查](README.md#热路径分类与检查)(`snapshot`、`restore`)
- 级别:unit
- 里程碑:M2
- 前置:R0;再建立:`Kill{Account(1), flatten}`、`DailyLoss{1}`、`Broken{1, g1}`、(A, 1) 已消耗 30 个令牌且在退避、(1, B) 有 `Recon` 的连续差异计数与一组未处置的对账组、账户 1 在 B 的 `RiskUsRow = {40, 150, 0}`、(1, B) 处于 L2、b1 杠杆 3.00、账户 2 未放行
- 步骤:
  1. `snapshot(w)`
  2. `restore(r, …)`,按生效参数调 `on_groups_changed`
  3. 再 `snapshot`
- 期望:
  - 步骤 1 写出节的顺序为 4、14、17、18、23,各写一次;节 4 每条生效 cap 一行(48 B `CapRecord`);节 17 含 `Acct`、`Bucket`、`Recon`、`ReconGate`、`Lev`、`UserStream` 行
  - 步骤 3 的字节与步骤 1 相同
  - 步骤 2 后:`kill_bits()` 与快照前相同(由 cap 重算,节中无 kill 位);`effective`、`released`、`view(1)`、`hedge_gate_ok`、`venue_gate_ok`、`auto_external_ok`、`lev_bad` 与快照前逐项相同;快照中没有任何 TSC 字段
- 实现:`crates/hft-risk/tests/snapshot.rs::rsk_230_snapshot_roundtrip`

### RSK-231 restore 拒绝超出容量的行

- 规则:[热路径:分类与检查](README.md#热路径分类与检查)(`restore`)
- 级别:unit
- 里程碑:M2
- 前置:按 R0 写出一个含 3 条 cap 的快照;restore 用 `Capacities` 使 cap 容量为 2(其余 R0)
- 步骤:
  1. `restore`
- 期望:
  - `Err(RestoreError)`
- 实现:`crates/hft-risk/tests/snapshot.rs::rsk_231_restore_capacity`

### RSK-240 view 与 view_for 读同一状态

- 规则:[热路径:分类与检查](README.md#热路径分类与检查)(`view`、`view_for`、`hedge_px_limits`、`leverage`)
- 级别:unit
- 里程碑:M2
- 前置:R0;`Kill{Account(1), flatten}`;(1, B) L2(liq 0.15,`deadline_ens = D`);`on_rate_limit_hit(B, 1)`;`l3_max_cross = 0.02`
- 步骤:
  1. `view(1, now)`;`view_for(1, b1, now)`;`view_for(1, a2, now)`;`hedge_px_limits(1, B, b1)`
- 期望:
  - `view(1)`:`margin_level = L2`、`liq_dist = 0.15`、`deadline_tsc = now.tsc_of(D)`、`open_blocked = true`、`hard = false`、`flatten_lock = true`、`max_order_notional = 10 000`、`max_hedge_notional = 20 000`、`max_reduce_notional = 10 000`、`conn_liveness_ms = 2 000`
  - `view_for(1, b1)`:`max_px_dev 0.03`、`ioc_max_cross 0.005`、`rr_max_cross 0.01`、`l3_max_cross 0.02`、`rate_backoff = true`
  - `view_for(1, a2)`:`0.05`、`0.01`、`0.02`、`l3_max_cross 0.02`、`rate_backoff = false`
  - `hedge_px_limits(1, B, b1) = {conn_liveness_ms 2 000, rr_max_cross 0.01}`
- 实现:`crates/hft-risk/tests/chain.rs::rsk_240_views`

### RSK-241 热路径入口零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)(零分配)
- 级别:unit
- 里程碑:M2
- 前置:R0;`new` 之后 testkit `CountingAlloc` 计数清零
- 步骤:
  1. 各调 10 000 次:`classify`、`evaluate` + `commit`(单腿与链接 Open、Hedge)、`check_mode` + `mode.apply`(Kill 增删)、`effective`、`on_pnl`、`on_legs`、`on_margin`、`on_timer`、`on_account_cfg`(1 024 条杠杆)、`hedge_px_limits`、`leverage`、`view`、`view_for`、`on_reconcile`
- 期望:
  - 分配次数 0
- 实现:`crates/hft-risk/tests/alloc.rs::rsk_241_zero_alloc`

### RSK-250 基准 classify

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/classify`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_250_classify`

### RSK-251 基准单腿 Open 全部通过 + commit

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/check_pass_open`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_251_check_pass_open`

### RSK-252 基准链接与同时双腿 Open 全部通过 + commit

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/check_pass_linked_open`、`risk/check_pair`
- 前置:R0,预检查与每次采样的 mock 连接活跃时刻和 Now 同步;引擎时间按令牌补充速率推进。
- 期望:预检查及每次评估均放行,链接单提交一次、同时双腿提交两次;任何拒绝使基准失败。
- 实现:`crates/hft-risk/benches/risk.rs::rsk_252_check_pass_linked_open`

### RSK-253 基准第 1 层即拒(HARD)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/check_reject_kill`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_253_check_reject_kill`

### RSK-254 基准 Hedge 类 evaluate + commit

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/check_pass_hedge`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_254_check_pass_hedge`

### RSK-255 基准 mode.apply(256 条 cap,含 kill 投影)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/mode_apply`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_255_mode_apply`

### RSK-256 基准 mode.effective

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/effective`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_256_effective`

### RSK-257 基准 on_pnl

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/on_pnl`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_257_on_pnl`

### RSK-258 基准 on_legs

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/on_legs`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_258_on_legs`

### RSK-259 基准 on_margin(档位不变)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/on_margin`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_259_on_margin`

### RSK-260 基准 hedge_px_limits

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/hedge_px_limits`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_260_hedge_px_limits`

### RSK-261 基准 leverage

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/leverage`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_261_leverage`

### RSK-262 基准 view_for

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/view_for`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_262_view_for`

### RSK-263 基准 on_account_cfg(1 024 条杠杆)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`risk/on_account_cfg`
- 实现:`crates/hft-risk/benches/risk.rs::rsk_263_on_account_cfg`

### RSK-264 基准 check_mode 只读预览

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 输入:R0,账户 2 已有 Kill cap,反复检查 Resume{Kill, Account(2)},不执行 apply
- 基准名:`risk/check_mode`
- 通过标准:参考机 p50 ≤ 2 µs、p99 ≤ 5 µs,零分配
- 实现:`crates/hft-risk/benches/risk.rs::rsk_264_check_mode`

### RSK-270 mode 在任意输入序列下保持不变量

- 规则:[mode 模块](README.md#mode-模块唯一的模式状态)、[有效模式](README.md#有效模式)、[cap 记录](README.md#cap-记录)、[kill 字](README.md#kill-字唯一的跨线程共享可变量)、[Kill](README.md#kill)、[启动闸门](README.md#启动闸门)
- 级别:prop
- 里程碑:M2
- 性质:每步之后(INV-11):① ModeTable 只在 `SetMode`(与 `GroupsChanged` 增删 cell)时变化;② `Resume` 成功时恰删除一条且其 (source, scope) 与输入完全相同;③ `kill_bits()` 等于参考模型从剩余 `Kill` 记录重算的结果,`account_hard(ix)` 只出现在带 HARD 的该账户记录服务的 gw 上;④ `hard` 与 Venue、`flatten` 与 Global、`flatten` 与 `hard` 的 Kill 都 `Err(InvalidScope)`;⑤ 合并结果是两条记录逐字段取更严(`MarginStress` 例外,取最新档位,见 RSK-071.8);⑥ `ctl_state_version` 的增量 = 1(ModeTable 或 cap 集合有变化)或 0;⑦ 无 HARD 时任何 cap 集合下 Reduce/Flatten/Hedge 的 `evaluate` 不返回 #1–#4 的位;⑧ `cancels` 的 `open_only` 按合并后的 Kill 记录:非 HARD 为 true、HARD 为 false;⑨ `notes` 只含 `CapSet`、`CapCleared`、`RecoveryHeldSim`、`IdentityUnverified`,级别符合 RSK-074;⑩ `gate_changes` 对每个 `gate_bits` 或 `released` 有变化的账户恰一项,无变化的账户不出现;⑪ 被拒的 apply 不改快照字节;⑫ 任何向量不溢出
- 生成:300 步,每步从 {SetMode(三种模式 × 作用域)、Kill(三级 × hard × flatten × cancel_resting)、Resume(15 种来源 × 随机作用域)、Auto(各来源)、AutoClear、Clock、Storage、StorageRecheck、Notice、Gate(各变体)、CellFlat、GroupsChanged、RecoveryFlags} 均匀抽取;R0 的账户与 cell;`ModeView` 各读数随机
- 参考:`tests/model/mode_ref.rs`(按 README 表格实现的朴素模型,`Vec<CapRecord>` 全量重算)
- 例数:PR 1 000;每夜 50 000(见验收)
- 实现:`crates/hft-risk/tests/mode_props.rs::rsk_270_mode_invariants`

### RSK-271 模拟账户与实盘账户在 kill 与 cap 下得到相同的检查链结果

- 规则:[启动闸门](README.md#启动闸门)(模拟账户段)、[令牌桶](README.md#令牌桶)(模拟账户用同一限额)
- 级别:prop
- 性质:把同一意图分别投给账户 1(Live)与账户 3(Sim,相同参数与 venue),在相同 cap 组合下(`Kill{Account(a)}`、`Kill{Account(a), hard}`、`DailyLoss{a}`、`MarginStress{a}`、`Kill{Global}` 等)`evaluate` 的结果相同;只有 `kill_bits()` 不同(Sim 账户不占位)
- 里程碑:M2
- 生成:意图种类 × 类别 × cap 子集 × 令牌消耗 0..100
- 参考:两次调用结果相等
- 例数:PR 1 000
- 实现:`crates/hft-risk/tests/chain_props.rs::rsk_271_sim_same_as_live`

### RSK-280 L3 时策略挂满同向 reduce-only 平仓单,引擎仍按步减到不再是 L3

- 规则:[保证金分档](README.md#保证金分档marginstress)(L3 行)、[检查链](README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)(#7 只扣即时在途)
- 级别:dst
- 里程碑:M4
- 前置:testkit 两腿世界:venue A 现货、B 永续;账户 1 的 g1 现货多 2 BTC、永续空 2 BTC;`force_step = 0.20`、`force_step_interval_ms = 5 000`;策略在 b1 挂满同向 reduce-only GTC(总量 = |仓位|,已 Ack)
- 故障:t = 0 起 b1 标记价线性上涨,使 `liq_dist` 在 t = 10 s 落到 0.10 以下并保持;A 的 BBO 连接健康
- 种子:PR 200(提交派生)+ 回归集;每夜日期派生
- 断言:进入 L3 后每 5 s 一条 `ForceDeleverage`,对应的执行层 `Flatten` 单无一被 `REDUCE_INVALID` 拒绝;永续空头按步减少直到档位降到 L3 以下;INV-6、INV-9、INV-12、INV-17、INV-19 每步成立
- 实现:`crates/hft-testkit/src/scenarios/risk.rs::rsk_280_l3_with_full_reduce_only_book`

### RSK-281 Storage 危急期间清仓按步完成,目标到达的同一 Input 内发起 Drain

- 规则:[存储降级](README.md#存储降级storage)(推迟)
- 级别:dst
- 里程碑:M5
- 前置:testkit 两腿世界;账户 1 有仓位;`Kill{Account(1), flatten}` 在 t = 0 生效
- 故障:t = 1 s 注入 `Timer{Sys, PersistLag}`(档位 2);b1 每个 IOC 切片只成交一半
- 种子:PR 200 + 回归集;每夜日期派生
- 断言:清仓目标到达之前 journal 中无 `Draining` cap 与 Drain 记录;目标到达的那条 Input 内出现 `ModeOutcome.drain = StorageCritical` 与 Drain;清仓锁直到进程结束仍在;期间所有 Open 被拒;INV-12、INV-18 成立
- 实现:`crates/hft-testkit/src/scenarios/risk.rs::rsk_281_storage_critical_waits_flatten`

### RSK-272 从任意快照起步回放,输出与连续运行逐字节相同

- 规则:[热路径:分类与检查](README.md#热路径分类与检查)(时间:持久状态存 engine_ns、`snapshot`、`restore`)、[令牌桶](README.md#令牌桶)、[启动闸门](README.md#启动闸门)(从快照起步不复位)
- 级别:prop
- 里程碑:M2
- 性质:任意输入序列在一个 `Risk` 上连续执行,与在第 k 步 `snapshot` → `restore` → `set_run_start(同一值)` → 按生效参数 `on_groups_changed` 并对每个 group 重放最近一次 `on_legs` 后继续执行,两者第 k 步之后的全部返回值(`evaluate`、`ModeOutcome`、`MarginOut`、`ReconVerdicts`、`auto_external_ok`、`Note`)与最终快照字节逐一相同
- 生成:400 步,输入从 `evaluate`+`commit`、`on_rate_limit_hit`、`mode.apply`(各变体)、`on_margin`、`on_timer`、`on_pnl`、`on_day_roll`、`on_legs`、`on_account_cfg`、`on_reconcile` 中抽取;engine_ns 单调递增;k 在 1..400 均匀
- 参考:两条执行路径的输出比较
- 例数:PR 500;每夜 20 000(见验收)
- 实现:`crates/hft-risk/tests/snapshot.rs::rsk_272_replay_from_any_snapshot`

### RSK-282 不含 Summary 的报告组不分级、不产生闸门输入、不计作一轮

- 规则:[对账分级(全系统唯一一次)](README.md#对账分级全系统唯一一次)(第 8 条)、[对账闸门与 HARD 解除的严格条件](README.md#对账闸门与-hard-解除的严格条件)
- 级别:table
- 里程碑:M2
- 前置:R0(b1 lot 0.001;`recon_closeonly_notional = 100`);对 (1, B) 依次调 `on_reconcile`;"S 组"= `Summary{sflags 0, skipped_inflight 0}` 加所列条目;"U 组"= 只含一条 `UnknownResolved` 条目、无 `Summary`
- 实现:`crates/hft-risk/tests/recon.rs::rsk_282_group_without_summary`

| 行 | 依次提交 | 期望 |
|---|---|---|
| RSK-282.1 | U 组 | 该次 `ReconVerdicts{recon: None, gate: None, n_warn: 0}`;无 `CapSet`、无注记 |
| RSK-282.2 | S 组 (b1, 0.003, 150);U 组;S 组 (b1, 0.003, 150) | 第 3 组 `recon = Some(Auto(Recon{1, B}))`(U 组不打断"连续两轮") |
| RSK-282.3 | `Recon{1, B}` 已设;S 组全部 Ignore;U 组;S 组全部 Ignore;`Resume{Recon, 1, B}` | `Ok`(两轮 Ignore 连续,U 组不计入也不清零) |
| RSK-282.4 | `Recon{1, B}` 已设;S 组全部 Ignore;U 组;`Resume{Recon, 1, B}` | `Err(ModeErr::ReconNotClean)`(只有一轮) |
| RSK-282.5 | 启动闸门 `RECONCILED` 未放行;U 组 | `gate = None`;`RECONCILED(1, B)` 仍未放行 |

### RSK-283 MarginStale 截止按严格大于判定:截止前 1 ns 与恰好截止时不过期

- 规则:[保证金分档(MarginStress)](README.md#保证金分档marginstress)(数据过期)
- 级别:table
- 里程碑:M2
- 前置:R0(账户 2,`margin_stale_ms = 15 000`、`mark_stale_ms = 10 000`);于 T 调 `on_margin(2, B, liq 0.50, report_ens = T, mark_ens = T)`,得截止 `T + 10 s + 1 ns`;每行在所列时刻调 `on_timer(MarginStale{2, B})`
- 实现:`crates/hft-risk/tests/margin.rs::rsk_283_stale_boundary`

| 行 | `on_timer` 时刻(engine_ns) | 期望 |
|---|---|---|
| RSK-283.1 | T + 10 s − 1 ns | 早到:档位 L0 不变;`timers = [(MarginStale{2, B}, T + 10 s + 1 ns)]`;无 `mode` |
| RSK-283.2 | T + 10 s(`now − mark_ens = mark_stale_ms`) | 同 .1(未过期) |
| RSK-283.3 | T + 10 s + 1 ns | 档位 L1;`mode = [Auto(MarginStress{level 1})]` |

### RSK-284 RKP1 完整字段往返与规范编码

- 规则:[配置](README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)
- 级别:unit
- 里程碑:M2
- 前置:R0 的 P0,非零版本、负 generated_at_ns 与 16 B author
- 步骤:编码后在只持有 refdata 与字节的路径解码;扩大原参数向量的容量后再次编码
- 期望:头部的 magic、版本、时间与 author 字节符合 RKP1;全部全局字段、类别、账户、venue 与 group 行逐字段相同;编码不受向量容量影响,解码后重编码逐字节相同
- 实现:`crates/hft-risk/tests/blob.rs::rsk_284_risk_blob_roundtrip`

### RSK-285 RKP1 拒绝不完整、歧义或缺引用的配置

- 规则:[配置](README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)
- 级别:unit
- 里程碑:M2
- 前置:RSK-284 的有效 blob
- 步骤:分别枚举每个截断点,破坏 magic/version/u32 字段/count/bool,追加尾字节;重复账户、删账户、指定未知 inst
- 期望:全部返回错误;缺失限额不由默认配置补齐,不存在窄整数截断
- 实现:`crates/hft-risk/tests/blob.rs::rsk_285_risk_blob_rejects_malformed`

### RSK-286 两份 RKP1 配置可在新线程独立解码

- 规则:[配置](README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)
- 级别:unit
- 里程碑:M2
- 前置:同一 refdata 的两份参数,单笔名义上限与 conn_liveness_ms 不同
- 步骤:四个新线程交替解码两份 blob,不传当前 RiskParams 或配置回调
- 期望:每个线程重编码等于其输入,两份参数没有交叉污染
- 实现:`crates/hft-risk/tests/blob.rs::rsk_286_risk_blob_no_process_state`

## 验收

- **里程碑 M2 完成标准**:本文件全部用例(级别 `unit`/`table`/`prop`/`bench`/`dst`)都有同编号的实现且通过,`cargo xtask test-trace` 无未实现、多余与不一致项([docs/tests §6](../../docs/tests/README.md#6-与代码的对应))。
- **属性测试大例数**(每夜运行;PR 例数见各用例):RSK-045 200 000、RSK-270 50 000、RSK-272 20 000,RSK-011、RSK-013、RSK-030、RSK-044、RSK-172、RSK-271 各 50 000;任一失败的最小化输入加入回归集。
- **DST**:RSK-280、RSK-281 计入上线门槛 ④ 的累计量,零违反;累计量与种子规则只在 [testing §8](../../docs/testing.md#8-上线门槛)。
- **基准**:RSK-250–RSK-264 在参考机上满足 [benchmarks 单 crate 基准上限表](../../benchmarks/README.md#单-crate-基准上限表) 的"风控链"一行与本 crate `benches/budgets.toml`(不宽于上限表);RSK-241 零分配在 PR 中阻断。
- **跨组件**:sequencer 驱动下的模式、kill、恢复与断腿场景(紧急停止、多个 cap 叠加、日亏 Resume、Drain 期间、冷启动、停机标记等)由 [testing §4.1](../../docs/testing.md#41-模式账户与人工干预) 登记的跨组件场景验证,本文件不重复。
