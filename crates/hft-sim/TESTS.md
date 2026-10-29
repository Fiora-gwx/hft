---
component: hft-sim
prefix: SIM
spec: README.md
---

# hft-sim 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | SIM-005、SIM-114 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 模拟账户是什么 | SIM-005、SIM-114、SIM-167 |
| 纯核心 API | SIM-002、SIM-003、SIM-012、SIM-113、SIM-188、SIM-220、SIM-226、SIM-231 |
| journal 衔接(`glue` 模块) | SIM-230 |
| 场所行为选项(`VenueModel`) | SIM-232–SIM-239、SIM-240 |
| sim-gw 任务(`task` feature) | SIM-214–SIM-217、SIM-105、SIM-140–SIM-148、SIM-205 |
| 重启恢复 | SIM-160–SIM-169、SIM-218、SIM-224 |
| 配置 | SIM-001、SIM-003、SIM-004、SIM-033、SIM-034、SIM-052、SIM-053、SIM-077、SIM-082、SIM-111、SIM-144 |
| 产生的事件 | SIM-062、SIM-075–SIM-077、SIM-083、SIM-091、SIM-100–SIM-104、SIM-110–SIM-114 |
| 拒单规则 | SIM-214、SIM-219、SIM-023、SIM-090、SIM-091、SIM-101、SIM-202 |
| 报告字段 | SIM-021、SIM-200 |
| 预置模拟实例池 | SIM-201 |
| 压力用例(上线门槛必过项) | SIM-180–SIM-196 |
| 与正式实盘并行校准 | SIM-206 |
| 实现说明 | 不适用:见子节 |
| 1. 时间 | SIM-010–SIM-012 |
| 2. 簿副本 | SIM-020–SIM-023 |
| 3. 吃单撮合 | SIM-030–SIM-036、SIM-125 |
| 4. 挂单(预留给 MakerFirst) | SIM-228、SIM-229、SIM-040、SIM-041、SIM-162 |
| 5. 手续费与扣费资产 | SIM-050–SIM-053、SIM-125、SIM-223 |
| 6. 资金费 | SIM-060–SIM-063、SIM-125、SIM-166、SIM-203 |
| 7. 永续全仓保证金与强平 | SIM-070–SIM-075、SIM-125、SIM-221、SIM-225 |
| 8. 现货余额与划转 | SIM-036、SIM-080–SIM-083、SIM-227 |
| 9. 确定性 | SIM-004、SIM-120–SIM-123、SIM-165、SIM-205 |
| 10. 不变量 | SIM-124、SIM-204、SIM-222 |
| 11. 源文件 | 不适用:非规范 |
| 延迟与资源契约 | SIM-002、SIM-006、SIM-104、SIM-210–SIM-213 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**夹具**(各用例的"前置"按名字引用;实现在 `crates/hft-sim/tests/common/mod.rs`,参数一律显式赋值):

- `rd_sim()`:refdata(以 `RdbBuilder` 构建;资产、inst、账户 id 稠密)。资产 `USDT`(`AssetId(1)`,`qty_exp = −6`)、`BTC`(2)、`ETH`(3)、`BNB`(4)(三者 `px_exp = −4`,`qty_exp = −8`);venue `S1`(`VenueId(2)` = `BINANCE_SPOT`,现货,交易所 E1 = Binance,`reduce_only = false`)、`P2`(`VenueId(5)` = `OKX_SWAP`,USDT 永续,交易所 E2 = OKX,`required_margin_mode = Cross`,`reduce_only = true`,`margin_data = Push`,`funding_settle = PerSymbolPush`)、`S4`(`VenueId(6)` = `OKX_SPOT`,现货,交易所 E2,只用于同所两 venue 的用例);inst `BTC/S1`(`InstId(11)`,tick 0.01,lot 0.00001,`min_qty` 0.00001,`min_notional` 5,`Trading`,`book_window` 4096)、`BNB/S1`(12,tick 0.01,lot 0.001)、`BTC/P2`(21,tick 0.1,lot 0.001,`min_qty` 0.001,`min_notional` 5)、`ETH/P2`(22,tick 0.01,lot 0.01),inst 1–10、13–20 为 `S1` 上 `Delisted` 的占位行;账户 1–9 都是 venues {S1, P2} 的 Sim 账户;`Fees`:(S1, 档 1)maker 0.0002、taker 0.0004,(S1, 档 2)maker 0.0002、taker 0.0003,(P2, 档 1)与(P2, 档 2)maker 0.0002、taker 0.0005,(S4, 档 1)同 (S1, 档 1);各 venue 的 `fee_asset_rule` 与 `funding_settle` 在用例"前置"里写明。
- `rd_binary()`:`rd_sim()` 加 venue `B5`(`VenueId(15)` = `POLYMARKET_CLOB`,现货,交易所 Polymarket)与两个结果代币 `UP_B5`(`InstId(23)`,symbol `btc-up-0`,资产 BTC)、`DN_B5`(24,`btc-dn-0`,资产 ETH),两者 `px_exp = −4`(1.0 = 10 000 raw)、tick 0.01、lot 1、`min_notional` 0.000001;B5 费率 0;账户 1–9 的 venues 加上 B5。
- 账户:`A9`(Sim,venues {S1, P2},`initial_balances` = S1 USDT 100 000、S1 BNB 1、P2 USDT 18 000,`latency_model = 0`、`fee_tier = 1`、`margin_model = 0`);`A8`(Sim,venues {S1, P2},`initial_balances` = S1 USDT 50 000、P2 USDT 10 000,其余同 `A9`;只在注明加入的用例中存在)。
- `setup_sim()`:`SimSetup { tuning: SimTuning { seed: 42, consume_decay_ms: 1 000, take_avail_bp: 10 000, quiet_ms: 50, report_interval_s: 60, transfer_delay_ms: 300 000, rate_limit: false, late_fee: false, maker_fill: TradeThrough }, tsc_hz_milli: 1 000 000 000 000(1 GHz,1 TSC = 1 ns), latency_models: [Fixed { entry_us: 2 000, resp_us: 2 000 }], margin_models: [MarginModel { mmr: Flat(0.005), leverage: 5, liq_fee: 0.005, liq_slippage_bp: 50 }], accounts: [A9] }`;用例改动其中某项时在"前置"里写出新值。
- 记法:事件时间写成 ms(1 GHz TSC);"`@t`"表示记录头 `tsc = t`,行情记录的 `ex_ts_ns` 默认等于 `t` 的 ns 值;`Bbo(inst, bid×量, ask×量) @t`;`TopN(inst, asks[…], bids[…]) @t` 是 `L2Snapshot{origin = TopN}`;`Place(inst, side, qty, px, tif) decided @t` 是一条 `op = Place` 的 `GwCmd` 经 `on_command(cmd, decided_tsc = t)`,slot 按出现顺序为 1、2、…,`gen = 1`;"撮合"由下一条 `tsc` 更大的记录或 `advance` 触发。数量、价格写十进制(raw = 十进制 × 10^(−exp));`Rate` 指数 −9。
- `seed9(…)`:`seed_account_from(A9, SimSeedState{…})`,未列出的部分为空;用于直接构造持仓。

### SIM-001 SimTuning 各键的默认值

- 规则:[配置](README.md#配置)
- 级别:table
- 里程碑:M2
- 前置:只给 `seed = 7`、其余键取默认值的 `SimTuning`(本用例是这些默认值的唯一用例)
- 实现:`crates/hft-sim/tests/config.rs::sim_001_tuning_defaults`

| 行 | 键 | 期望 |
|---|---|---|
| SIM-001.1 | `consume_decay_ms` | 1 000 |
| SIM-001.2 | `take_avail_bp` | 10 000 |
| SIM-001.3 | `quiet_ms` | 50 |
| SIM-001.4 | `report_interval_s` | 60 |
| SIM-001.5 | `transfer_delay_ms` | 300 000 |
| SIM-001.6 | `rate_limit` | true |
| SIM-001.7 | `late_fee` | false |
| SIM-001.8 | `maker_fill` | `TradeThrough` |
| SIM-001.9 | `seed` | 无默认值:不给 `seed` 时构造失败(hft-core 据此拒绝启动) |

### SIM-002 常量取值

- 规则:[纯核心 API](README.md#纯核心-api)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M2
- 前置:无
- 步骤:
  1. 读常量
- 期望:
  - `MAX_SIM_ACCOUNTS == 16`,`MAX_LATENCY_MODELS == 16`,`MAX_MARGIN_MODELS == 16`,`SIM_CODE_BASE == 0x1000_0000`;`CMDS_PER_POLL == 256`,`REPLY_RESERVE == 4 096`
- 实现:`crates/hft-sim/tests/config.rs::sim_002_constants`

### SIM-003 SimExchange::new 对每个配置错误返回对应的 SimCfgErr

- 规则:[纯核心 API](README.md#纯核心-api)、[配置](README.md#配置)
- 级别:table
- 里程碑:M2
- 前置:`rd_sim()`、`setup_sim()`,每行只改一处
- 实现:`crates/hft-sim/tests/config.rs::sim_003_config_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| SIM-003.1 | `A9.initial_balances` 含 venue `VenueId(9)`(refdata 没有) | `Err(UnknownVenue)` |
| SIM-003.2 | `initial_balances` 含 refdata 没有的资产 | `Err(UnknownAsset)` |
| SIM-003.3 | `fee_tier = 9`(没有 (S1, 9) 行) | `Err(FeeTier)` |
| SIM-003.4 | `latency_model = 1`(表长 1) | `Err(LatencyModelIndex)` |
| SIM-003.5 | `latency_model = 0`(表长 1) | `Ok` |
| SIM-003.6 | `margin_model = 1`(表长 1) | `Err(MarginModelIndex)` |
| SIM-003.7 | `latency_models` 为空而有 Sim 账户 | `Err(LatencyModelIndex)` |
| SIM-003.8 | `margin_models` 为空而有 Sim 账户 | `Err(MarginModelIndex)` |
| SIM-003.9 | `Empirical` 的 `q_bp` 为 [0, 5 000, 5 000, 10 000] | `Err(BadDist)` |
| SIM-003.10 | `q_bp` 首项为 100 | `Err(BadDist)` |
| SIM-003.11 | `q_bp` 末项为 9 999 | `Err(BadDist)` |
| SIM-003.12 | `us` 为 [100, 200, 150, 300] | `Err(BadDist)` |
| SIM-003.13 | `n = 64`、分位点合法 | `Ok` |
| SIM-003.14 | `n = 65` | `Err(BadDist)` |
| SIM-003.15 | `P2.required_margin_mode = Isolated` | `Err(MarginMode)` |
| SIM-003.16 | `A9.initial_balances` 另含 `(S4, USDT, 1)`(venue 集合 {S1, P2, S4} 中 P2 与 S4 同为交易所 E2) | `Err(SameExchangeMarkets)` |
| SIM-003.17 | 16 个 Sim 账户 | `Ok` |
| SIM-003.18 | 17 个 Sim 账户 | `Err(TooManyAccounts)` |

### SIM-004 两个账户引用同一延迟模型时抽样子流互不影响

- 规则:[配置](README.md#配置)、[9. 确定性](README.md#9-确定性)
- 级别:unit
- 里程碑:M2
- 前置:`setup_sim()` 但 `latency_models = [Empirical { entry: [0 → 1 000 µs, 10 000 → 5 000 µs], resp: 同 }]`;运行 R1 只有 `A9`,运行 R2 另有 `A8`;两次运行 `A9` 的行情与指令相同,R2 中 `A8` 另有 50 条 `Place`
- 步骤:
  1. 两次运行各自记下 `A9` 每条回报的 `due_tsc`
- 期望:
  - 两次运行 `A9` 的 `due_tsc` 序列逐项相等
- 实现:`crates/hft-sim/tests/config.rs::sim_004_substreams_independent`

### SIM-005 账户之间余额、仓位与记忆互不影响

- 规则:[模拟账户是什么](README.md#模拟账户是什么)
- 级别:unit
- 里程碑:M2
- 前置:`setup_sim()` 加入 `A8`;`S1` `fee_asset_rule = Quote`;`TopN(BTC/S1, asks [60 000×0.1]) @100 ms`
- 步骤:
  1. `A9` `Place(BTC/S1, Buy, 0.1, 60 000, IOC) decided @100 ms`;`A8` 同样的单 decided @100 ms
  2. 推进到 110 ms,读两个账户的 `SimAccountView`
- 期望:
  - 两个账户各成交 0.1(记忆按账户独立);`A9` USDT 减少 6 002.4,`A8` USDT 减少 6 002.4;`A9` 的指令不改变 `A8` 的任何余额
- 实现:`crates/hft-sim/tests/account.rs::sim_005_accounts_isolated`

### SIM-006 new 之后零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M2
- 前置:`rd_sim()`、`setup_sim()`;计数分配器
- 步骤:
  1. `SimExchange::new` 之后清零计数;喂 100 000 条行情记录与 10 000 条指令,`drain_due` 全部回报
- 期望:
  - 分配计数 0
- 实现:`crates/hft-sim/tests/account.rs::sim_006_zero_alloc`

### SIM-010 指令在到达时刻之前的行情全部应用之后撮合

- 规则:[1. 时间](README.md#1-时间)
- 级别:unit
- 里程碑:M2
- 前置:`rd_sim()`、`setup_sim()`(entry 2 ms、resp 2 ms);`S1` `fee_asset_rule = Quote`;`Bbo(BTC/S1, 59 990×1, 60 000×1) @100 ms`
- 步骤:
  1. `Place(BTC/S1, Buy, 0.1, 60 030, IOC) decided @100 ms`(到达 102 ms)
  2. `Bbo(…, ask 60 010×1) @102 ms`;`Bbo(…, ask 60 020×1) @102 ms + 1 ns`
  3. `drain_due(104 ms − 1 ns)`,再 `drain_due(104 ms)`
- 期望:
  - 成交价 60 010(`tsc = 102 ms` 的记录已应用,`102 ms + 1 ns` 的未应用);成交回报的 `ex_ts_ns` = 102 ms 记录的交易所时间
  - 第一次 `drain_due` 无回报;第二次依次为 `Ack`、`Fill{cum_filled 0.1, avg_px 60 010}`,`due_tsc` 都是 104 ms
- 实现:`crates/hft-sim/tests/time.rs::sim_010_match_after_arrive`

### SIM-011 同一账户的指令按 (arrive_tsc, gen_seq) 全序执行

- 规则:[1. 时间](README.md#1-时间)
- 级别:unit
- 里程碑:M2
- 前置:同 SIM-010 的前置;`TopN(BTC/S1, asks [60 000×0.1]) @100 ms`
- 步骤:
  1. `Place(…, Buy, 0.1, 60 000, IOC)` 两条,decided 同为 @100 ms(slot 1、2)
- 期望:
  - slot 1 成交 0.1;slot 2 `Expired`、无成交(slot 1 先执行,记忆已扣去该档)
- 实现:`crates/hft-sim/tests/time.rs::sim_011_total_order`

### SIM-012 回报按 (due_tsc, gen_seq) 升序交付,next_due 给出最早的待交付时刻

- 规则:[纯核心 API](README.md#纯核心-api)、[1. 时间](README.md#1-时间)
- 级别:unit
- 里程碑:M2
- 前置:`setup_sim()` 但 `latency_models = [Fixed{2 000, 2 000}, Fixed{1 000, 1 000}]`,加入 `A8` 并令 `A8.latency_model = 1`;两账户各 decided @100 ms 下一单
- 步骤:
  1. 推进到 110 ms,读 `next_due()`;`drain_due(110 ms)`
- 期望:
  - `next_due() == Some(101 ms)`(空仓启动保证金报告);全部回报按 due 升序,订单回报仍先交付 A8(102 ms),再交付 A9(104 ms);全部交付后 `next_due() == None`
- 实现:`crates/hft-sim/tests/time.rs::sim_012_drain_order`

### SIM-020 TopN 快照整替换后只能吃到快照中的档位

- 规则:[2. 簿副本](README.md#2-簿副本)
- 级别:unit
- 里程碑:M2
- 前置:`rd_sim()`、`setup_sim()`;`S1` `fee_asset_rule = Quote`
- 步骤:
  1. `TopN(BTC/S1, asks [60 000×0.1, 60 010×0.1, 60 020×0.1]) @100 ms`;`Place(Buy, 0.5, 61 000, IOC) decided @100 ms`;推进到 110 ms
  2. `TopN(BTC/S1, asks [60 100×1]) @200 ms`;`Place(Buy, 0.1, 60 050, IOC) decided @200 ms`;推进到 210 ms
- 期望:
  - 步骤 1:三条成交共 0.3,`Expired` 余量 0.2(吃穿第 3 档的部分不成交)
  - 步骤 2:无成交,`Expired` 0.1(旧档位已被整替换)
- 实现:`crates/hft-sim/tests/book.rs::sim_020_topn_full_replace`

### SIM-021 只订阅 BBO 的 inst 只有一档可吃并计 top_only_fills

- 规则:[2. 簿副本](README.md#2-簿副本)、[报告字段](README.md#报告字段)
- 级别:unit
- 里程碑:M2
- 前置:同 SIM-020;`BTC/S1` 只有 Bbo 流
- 步骤:
  1. `Bbo(BTC/S1, 59 990×1, 60 000×0.05) @100 ms`;`Place(Buy, 0.1, 61 000, IOC) decided @100 ms`;推进到 110 ms
- 期望:
  - 成交 0.05 @ 60 000,`Expired` 0.05;`top_only_fills` 计数 +1
- 实现:`crates/hft-sim/tests/book.rs::sim_021_bbo_only_one_level`

### SIM-022 撮合视图的顶档取 BBO 与簿顶档中交易所时间更新者

- 规则:[2. 簿副本](README.md#2-簿副本)
- 级别:table
- 里程碑:M2
- 前置:同 SIM-020;`TopN(BTC/S1, asks [60 000×0.1, 60 010×0.1, 60 020×0.1])`,`ex_ts_ns` = 100 ms;每行再给一条 `Bbo` 后下 `Place(Buy, 0.3, 61 000, IOC)`
- 实现:`crates/hft-sim/tests/book.rs::sim_022_top_from_newer_source`

| 行 | Bbo(卖一 × 量,`ex_ts_ns`) | 期望成交 |
|---|---|---|
| SIM-022.1 | 60 005 × 0.1,101 ms | 0.1 @ 60 005、0.1 @ 60 010、0.1 @ 60 020(簿中优于顶档的 60 000 不可吃) |
| SIM-022.2 | 60 005 × 0.1,99 ms | 0.1 @ 60 000、0.1 @ 60 010、0.1 @ 60 020(簿顶档更新) |

### SIM-023 簿不在 Live 时以 NotTrading 拒单并计数

- 规则:[2. 簿副本](README.md#2-簿副本)、[拒单规则](README.md#拒单规则)
- 级别:unit
- 里程碑:M2
- 前置:同 SIM-020;`BTC/S1` 的簿处于重同步(收到序号缺口后的 `L2Batch`,增量深度启用)
- 步骤:
  1. `Place(Buy, 0.1, 61 000, IOC) decided @100 ms`;推进到 110 ms
- 期望:
  - 回报 `Reject`,`reject_code = 0x1000_0001`;无 `Ack`、无成交;簿未 Live 拒单计数 +1
- 实现:`crates/hft-sim/tests/book.rs::sim_023_book_not_live_rejects`

### SIM-030 逐档吃单直到限价

- 规则:[3. 吃单撮合](README.md#3-吃单撮合)
- 级别:unit
- 里程碑:M2
- 前置:同 SIM-020;`TopN(BTC/S1, asks [60 000×0.1, 60 010×0.2, 60 020×0.3]) @100 ms`
- 步骤:
  1. `Place(Buy, 0.25, 60 015, IOC) decided @100 ms`;推进到 110 ms
- 期望:
  - 回报依次:`Ack`;`PartialFill{cum_filled 0.1, avg_px 60 000, fee 2.4, fee_asset USDT}`;`Fill{cum_filled 0.25, avg_px 60 006, fee 3.6006}`;无 `Expired`
- 实现:`crates/hft-sim/tests/taker.rs::sim_030_walk_levels_to_limit`

### SIM-031 IOC 余量 Expired

- 规则:[3. 吃单撮合](README.md#3-吃单撮合)
- 级别:unit
- 里程碑:M2
- 前置:同 SIM-030
- 步骤:
  1. `Place(Buy, 0.7, 60 015, IOC) decided @100 ms`;推进到 110 ms
- 期望:
  - 成交 0.1 @ 60 000 与 0.2 @ 60 010;最后一条 `Expired`,`cum_filled` 0.3
- 实现:`crates/hft-sim/tests/taker.rs::sim_031_ioc_remainder_expired`

### SIM-032 FOK 总可得不足时整单无成交

- 规则:[3. 吃单撮合](README.md#3-吃单撮合)
- 级别:table
- 里程碑:M2
- 前置:同 SIM-030;每行独立
- 实现:`crates/hft-sim/tests/taker.rs::sim_032_fok`

| 行 | 指令 | 期望 |
|---|---|---|
| SIM-032.1 | `Place(Buy, 0.7, 61 000, FOK)`(可得 0.6) | `Ack` 后 `Expired`,`cum_filled` 0;余额不变 |
| SIM-032.2 | `Place(Buy, 0.6, 61 000, FOK)` | 三档全部成交,`Fill{cum_filled 0.6}` |

### SIM-033 take_avail_bp 按比例折减每档可得量并按 lot 向下取整

- 规则:[3. 吃单撮合](README.md#3-吃单撮合)、[配置](README.md#配置)
- 级别:table
- 里程碑:M2
- 前置:同 SIM-030;每行设定 `take_avail_bp` 后下 `Place(Buy, 0.25, 60 015, IOC)`
- 实现:`crates/hft-sim/tests/taker.rs::sim_033_take_avail_bp`

| 行 | `take_avail_bp` | 期望成交 |
|---|---|---|
| SIM-033.1 | 5 000 | 0.05 @ 60 000、0.1 @ 60 010;`Expired` 0.1 |
| SIM-033.2 | 10 000 | 0.1、0.15 |
| SIM-033.3 | 3 333 | 0.03333 @ 60 000(0.1 × 0.3333 向下取整到 0.00001)、0.06666 @ 60 010 |

### SIM-034 已吃流动性记忆按 consume_decay_ms 线性衰减

- 规则:[3. 吃单撮合](README.md#3-吃单撮合)、[配置](README.md#配置)
- 级别:table
- 里程碑:M2
- 前置:同 SIM-020;`consume_decay_ms = 1 000`;`TopN(BTC/S1, asks [60 000×0.1]) @100 ms`;第一单 `Place(Buy, 0.1, 60 000, IOC) decided @100 ms`(到达 102 ms,吃掉 0.1);之后同一快照再发一次;第二单 `Place(Buy, 0.1, 60 000, IOC)` 的到达时刻按行
- 实现:`crates/hft-sim/tests/taker.rs::sim_034_memory_decay`

| 行 | 第二单到达 − 102 ms | 期望第二单成交 |
|---|---|---|
| SIM-034.1 | 0 | 0(记忆剩余 0.1) |
| SIM-034.2 | 500 ms | 0.05 |
| SIM-034.3 | 999 ms | 0.0999 |
| SIM-034.4 | 1 000 ms | 0.1(衰减完) |
| SIM-034.5 | 第二单在 500 ms 到达(吃 0.05),第三单在 600 ms 到达 | 第三单成交 0.015(两条记忆剩余 0.04 + 0.045) |

### SIM-035 记忆环满且无过期项时覆盖最早的一项

- 规则:[3. 吃单撮合](README.md#3-吃单撮合)
- 级别:unit
- 里程碑:M2
- 前置:同 SIM-020;`consume_decay_ms = 60 000`;64 个不同价位各被吃过一次(t0 依次递增,全部未过期)
- 步骤:
  1. 吃第 65 个价位;读记忆
  2. 再对第 1 个价位(t0 最早)下单
- 期望:
  - 步骤 1:环为 64 项;第 1 个价位的记忆已被覆盖,第 2 个与第 65 个价位的记忆在
  - 步骤 2:该价位按全部可见量成交;环仍为 64 项
- 实现:`crates/hft-sim/tests/taker.rs::sim_035_memory_ring_overwrite`

### SIM-036 可成交的 GTC 限价单余量转为挂单

- 规则:[3. 吃单撮合](README.md#3-吃单撮合)、[8. 现货余额与划转](README.md#8-现货余额与划转)
- 级别:unit
- 里程碑:M2
- 前置:同 SIM-030
- 步骤:
  1. `Place(Buy, 0.5, 60 010, GTC) decided @100 ms`;推进到 110 ms
- 期望:
  - 成交 0.1 @ 60 000、0.2 @ 60 010;无 `Expired`;余量 0.2 成为挂单(订单表状态 PartiallyFilled),USDT 锁定 0.2 × 60 010 + 0.2 × 60 010 × 0.0004 = 12 006.8008
- 实现:`crates/hft-sim/tests/taker.rs::sim_036_gtc_rests_remainder`

### SIM-040 TradeThrough:只有穿价成交或卖一低于挂价时买挂单成交

- 规则:[4. 挂单(预留给 MakerFirst)](README.md#4-挂单预留给-makerfirst)
- 级别:table
- 里程碑:M2
- 前置:同 SIM-020;`maker_fill = TradeThrough`;`A9` 有挂单 `Place(Buy, 0.1, 59 000, GTC)`(未成交);每行之后推进 10 ms
- 实现:`crates/hft-sim/tests/maker.rs::sim_040_trade_through`

| 行 | 行情 | 期望 |
|---|---|---|
| SIM-040.1 | `Trade(BTC/S1, 59 000 × 1)` | 无成交(价等于挂价不算穿价) |
| SIM-040.2 | `Trade(BTC/S1, 58 990 × 0.04)` | 成交 0.04 @ 59 000,maker 费 0.472(0.04 × 59 000 × 0.0002) |
| SIM-040.3 | `Bbo(…, ask 58 999.99 × 1)` | 成交 0.1 @ 59 000 |
| SIM-040.4 | `Bbo(…, ask 59 000 × 1)` | 无成交 |

### SIM-041 挂卖单的穿价规则与买对称

- 规则:[4. 挂单(预留给 MakerFirst)](README.md#4-挂单预留给-makerfirst)
- 级别:unit
- 里程碑:M2
- 前置:同 SIM-040,但挂单为 `Place(Sell, 0.1, 61 000, GTC)`,`A9` 先持有 BTC 0.1
- 步骤:
  1. `Trade(BTC/S1, 61 000.01 × 0.2)`
- 期望:
  - 成交 0.1 @ 61 000
- 实现:`crates/hft-sim/tests/maker.rs::sim_041_trade_through_sell`

### SIM-050 四种扣费规则下的费用资产、金额与到账量

- 规则:[5. 手续费与扣费资产](README.md#5-手续费与扣费资产)
- 级别:table
- 里程碑:M2
- 前置:同 SIM-020(`fee_tier = 1`,taker 0.0004);买入行 `TopN(BTC/S1, asks [60 000×1]) @100 ms`,卖出行 `TopN(BTC/S1, bids [60 000×1]) @100 ms`(同价两侧的快照是交叉簿,hft-book 拒绝);`Bbo(BNB/S1, 599.99×10, 600.01×10) @100 ms`(中间价 600.00,第 7 行除外);卖出行之前 `A9` 持有 BTC 0.1;每行一笔 IOC,`S1` 的 `fee_asset_rule` 按行
- 实现:`crates/hft-sim/tests/fees.rs::sim_050_fee_asset_rules`

| 行 | 规则 / 方向 / 量 @ 价 | 期望 |
|---|---|---|
| SIM-050.1 | `Quote` / Buy / 0.1 @ 60 000 | `fee` 2.4,`fee_asset` USDT;USDT −6 002.4,BTC +0.1 |
| SIM-050.2 | `Quote` / Sell / 0.1 @ 60 000 | `fee` 2.4 USDT;USDT +5 997.6 |
| SIM-050.3 | `ReceivedAsset` / Buy / 0.1 | `fee` 0.00004 BTC;BTC +0.09996,USDT −6 000 |
| SIM-050.4 | `ReceivedAsset` / Sell / 0.1 | `fee` 2.4 USDT |
| SIM-050.5 | `ReceivedAssetUnlessDiscount{BNB}` / Buy / 0.1,BNB free 1 | `fee` 0.004 BNB(2.4 ÷ 600);BNB 0.996,BTC +0.1 |
| SIM-050.6 | 同上,BNB free 0.003 | `fee` 0.00004 BTC(BNB 不足,按 `ReceivedAsset`) |
| SIM-050.7 | 同上,BNB free 1,没有 `BNB/S1` 行情 | `fee` 0.00004 BTC |
| SIM-050.8 | `ReceivedAssetUnlessDiscount{BNB}` / Sell / 0.1,BNB free 0.003 | `fee` 2.4 USDT(卖出回退扣计价资产) |
| SIM-050.9 | `Configured(BNB)` / Sell / 0.1 | `fee` 0.004 BNB |
| SIM-050.10 | `Quote` / Buy / 0.00001 @ 60 000.01(本行 `BTC/S1` 的 `min_notional` 改为 0.5,卖一 60 000.01) | `fee` 0.000241 USDT(0.00024000004 按支出向上取整) |

### SIM-051 挂单成交按 maker 费率计费

- 规则:[5. 手续费与扣费资产](README.md#5-手续费与扣费资产)
- 级别:unit
- 里程碑:M2
- 前置:SIM-040 的前置;`S1` `fee_asset_rule = Quote`
- 步骤:
  1. `Trade(BTC/S1, 58 990 × 1)`
- 期望:
  - `Fill{cum_filled 0.1, avg_px 59 000, fee 1.18, fee_asset USDT}`(0.1 × 59 000 × 0.0002)
- 实现:`crates/hft-sim/tests/fees.rs::sim_051_maker_fee`

### SIM-052 late_fee 打开时成交回报不带费用、另发 FeeOnly

- 规则:[5. 手续费与扣费资产](README.md#5-手续费与扣费资产)、[配置](README.md#配置)
- 级别:unit
- 里程碑:M2
- 前置:SIM-050 第 1 行的前置,`late_fee = true`
- 步骤:
  1. `Place(Buy, 0.1, 60 000, IOC) decided @100 ms`;`drain_due(200 ms)`
- 期望:
  - `Fill` 的 `fee = 0`;另有一条 `FeeOnly{fee 2.4, fee_asset USDT}`,`due_tsc` = 成交回报的 `due_tsc` + 50 ms(154 ms);USDT 在 `FeeOnly` 时刻之后才扣 2.4(总额最终同第 1 行)
- 实现:`crates/hft-sim/tests/fees.rs::sim_052_late_fee`

### SIM-053 fee_tier 选择费率行

- 规则:[配置](README.md#配置)、[5. 手续费与扣费资产](README.md#5-手续费与扣费资产)
- 级别:table
- 里程碑:M2
- 前置:SIM-050 第 1 行的前置;`A9.fee_tier` 按行
- 实现:`crates/hft-sim/tests/fees.rs::sim_053_fee_tier_row`

| 行 | `fee_tier` | 期望 |
|---|---|---|
| SIM-053.1 | 1 | `fee` 2.4 |
| SIM-053.2 | 2 | `fee` 1.8(档 2 taker 0.0003;平台币折扣经费率档表达) |

### SIM-060 结算在该 venue 第一条交易所时间不早于结算时刻的记录被消费时发生

- 规则:[6. 资金费](README.md#6-资金费)
- 级别:table
- 里程碑:M2
- 前置:`rd_sim()`、`setup_sim()`;`P2` `funding_settle = PerSymbolPush`;`seed9(P2 USDT 18 000;BTC/P2 −1.0 @ 60 000)`;`Funding(BTC/P2, rate 0.0001, next_funding = T)`(`T` = 8 h)@ `T − 1 h`;`Mark(BTC/P2, 60 000)` @ `T − 1 s`;`Mark(BTC/P2, 61 000)` 的 `ex_ts_ns` 按行
- 实现:`crates/hft-sim/tests/funding.rs::sim_060_settle_timing`

| 行 | 下一条 P2 记录的 `ex_ts_ns` | 期望 |
|---|---|---|
| SIM-060.1 | `T − 1 ns` | 不结算;`wallet` 18 000 |
| SIM-060.2 | `T` | 结算:`FundingSettle{inst BTC/P2, amount +6.0, rate 0.0001, pos_qty −1.0, settle_ts_ns T, synth 0}`;`wallet` 18 006(按 `T` 之前最后一条 Mark 60 000) |
| SIM-060.3 | `T + 1 s` | 同第 2 行(只结算一次;再来一条 `ex_ts ≥ T` 的记录不重复结算) |

### SIM-061 资金费金额取整一次,仓位为 0 不结算

- 规则:[6. 资金费](README.md#6-资金费)
- 级别:table
- 里程碑:M2
- 前置:同 SIM-060 但仓位、`mark`、`rate` 按行,`mark = 60 000.0001`、`rate = 0.000123457`
- 实现:`crates/hft-sim/tests/funding.rs::sim_061_funding_amount_rounding`

| 行 | 仓位 | 期望 |
|---|---|---|
| SIM-061.1 | −0.001 | `amount` +0.007407(收入向下) |
| SIM-061.2 | +0.001 | `amount` −0.007408(USDT raw −7 408;支出按绝对值向上) |
| SIM-061.3 | 0 | 不结算、无推送 |

### SIM-062 推送形态按 funding_settle,余额变化在任何形态下都进入真值

- 规则:[6. 资金费](README.md#6-资金费)、[产生的事件](README.md#产生的事件)
- 级别:table
- 里程碑:M2
- 前置:同 SIM-060,另持 `ETH/P2` −10.0(mark 3 000、费率 0.0001,同一结算时刻);`P2` 的 `funding_settle` 按行;结算记录 `ex_ts_ns = T`
- 实现:`crates/hft-sim/tests/funding.rs::sim_062_push_forms`

| 行 | `funding_settle` | 期望 |
|---|---|---|
| SIM-062.1 | `PerSymbolPush` | 两条 `FundingSettle`:`BTC/P2` +6.0、`ETH/P2` +3.0;`wallet` +9.0 |
| SIM-062.2 | `BalanceOnlyPush` | 一条 `FundingSettle{inst INST_NONE, amount +9.0, rate 0, pos_qty 0}`;`wallet` +9.0 |
| SIM-062.3 | `RateOnly` | 无 `FundingSettle`;`wallet` +9.0 |

### SIM-063 结算时刻跟随最近的 Funding 事件

- 规则:[6. 资金费](README.md#6-资金费)
- 级别:unit
- 里程碑:M2
- 前置:同 SIM-060
- 步骤:
  1. `T − 30 min` 再来一条 `Funding(BTC/P2, rate 0.0002, next_funding = T + 4 h)`;再依次喂 `Mark(BTC/P2, 60 000)`,`ex_ts_ns` 为 `T + 1 s` 与 `T + 4 h`
- 期望:
  - `T` 处不结算;`T + 4 h` 结算一次,`amount` +12.0(按最近 `Funding` 的费率 0.0002 与 `T + 4 h` 之前最后一条 Mark)
- 实现:`crates/hft-sim/tests/funding.rs::sim_063_settle_time_follows_funding`

保证金用例另用:`seed9(P2 USDT 18 000;BTC/P2 −1.0 @ 60 000)`,`Mark(BTC/P2, 60 000)`;`MarginModel = {Flat(0.005), leverage 5, liq_fee 0.005, liq_slippage_bp 50}`。

### SIM-070 保证金余额、维持、初始、可用与比率

- 规则:[7. 永续全仓保证金与强平](README.md#7-永续全仓保证金与强平)
- 级别:unit
- 里程碑:M2
- 前置:见上
- 步骤:
  1. 读 `account(A9)` 的 `P2` 保证金视图
- 期望:
  - `unrealized` 0,`margin_balance` 18 000,`maint` 300,`initial` 12 000,`avail` 6 000,`margin_ratio` 0.016666667(raw 16 666 667)
- 实现:`crates/hft-sim/tests/margin.rs::sim_070_margin_numbers`

### SIM-071 强平价公式:多空、多 inst、RefdataTiers 跨档

- 规则:[7. 永续全仓保证金与强平](README.md#7-永续全仓保证金与强平)
- 级别:table
- 里程碑:M2
- 前置:见上,仓位与钱包按行;`ETH/P2` 的 mark 3 000;第 4 行的 refdata 给 `BTC/P2` 分档维持保证金率(名义 ≤ 50 000:0.004;(50 000, 200 000]:0.005;> 200 000:0.01;分档表字段归 hft-refdata)且 `mmr = RefdataTiers`
- 实现:`crates/hft-sim/tests/margin.rs::sim_071_liq_px`

| 行 | 钱包 / 仓位 | 期望 `liq_px(BTC/P2)` |
|---|---|---|
| SIM-071.1 | 18 000 / 空 1.0 @ 60 000 | 77 611.9403(78 000 ÷ 1.005 = 77 611.94029…,空头向上取整) |
| SIM-071.2 | 18 000 / 多 1.0 @ 60 000 | 42 211.0552(42 000 ÷ 0.995 = 42 211.05527…,多头向下取整) |
| SIM-071.3 | 18 000 / 空 BTC 1.0 + 空 ETH 10.0 @ 3 000 | 77 462.6866((18 000 − 150 + 60 000) ÷ 1.005) |
| SIM-071.4 | 54 000 / 空 3.0 @ 60 000 | 77 227.7228(首轮 0.005 得名义 232 836 进入 0.01 档,重算 234 000 ÷ 3.03) |
| SIM-071.5 | 60 000 / 多 1.0 @ 60 000 | 0(分子为 0,无强平价) |
| SIM-071.6 | 每行 | 与测试内 i128 参考解逐值相同 |

### SIM-072 单持仓时 mark 恰到强平价触发强平,差一个价格单位不触发

- 规则:[7. 永续全仓保证金与强平](README.md#7-永续全仓保证金与强平)
- 级别:table
- 里程碑:M2
- 前置:见上(`liq_px` 77 611.9403);每行一条 `Mark(BTC/P2, m)`
- 实现:`crates/hft-sim/tests/margin.rs::sim_072_liquidation_boundary`

| 行 | `m` | 期望 |
|---|---|---|
| SIM-072.1 | 77 611.9402 | 无 `ExternalFill`;`maint` < `margin_balance` |
| SIM-072.2 | 77 611.9403 | 一条 `ExternalFill{cause Liquidation, side Buy, inst BTC/P2, qty 1.0, px 78 000.0001, fee 390.000001, fee_asset USDT}`;仓位 0;`wallet` −390.000101;其后一条 `MarginReport`;`liquidations` 计数 +1 |

### SIM-073 单持仓的强平恰在 mark 到达 liq_px 的那条 Mark 触发

- 规则:[7. 永续全仓保证金与强平](README.md#7-永续全仓保证金与强平)
- 级别:prop
- 里程碑:M2
- 性质:单个永续持仓(多或空)时,由 `MarginReport` 给出的 `liq_px` 为 L(≠ 0):`mark = L` 的那条 Mark 触发强平,`mark` 比 L 少走 1 个价格最小单位(空头 L − 1 raw、多头 L + 1 raw)不触发
- 生成:钱包 [1 000, 100 000];仓位 ±[0.001, 10];入场价 [10 000, 100 000];`mmr` Flat [0.001, 0.05] 与三档 RefdataTiers
- 参考:i128 精确比较 `maint ≥ margin_balance`
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-sim/tests/prop_margin.rs::sim_073_liq_px_triggers_exactly`

### SIM-074 触发时该 venue 全部永续仓位按滑点价平仓,每 inst 一条强平成交

- 规则:[7. 永续全仓保证金与强平](README.md#7-永续全仓保证金与强平)
- 级别:unit
- 里程碑:M2
- 前置:SIM-071 第 3 行的仓位
- 步骤:
  1. `Mark(BTC/P2, 77 462.6866)`
- 期望:
  - 两条 `ExternalFill{Liquidation}`:`BTC/P2` Buy 1.0 @ 77 850.0001(77 462.6866 × 1.005 = 77 850.000033,向上取整)、`ETH/P2` Buy 10.0 @ 3 015.0000(3 000 × 1.005);两个仓位都为 0;各自的 `fee` = 成交名义 × 0.005 向上取整
- 实现:`crates/hft-sim/tests/margin.rs::sim_074_liquidate_all_positions`

### SIM-075 MarginReport 的强平价与触发同一口径,ADL 档恒 0

- 规则:[7. 永续全仓保证金与强平](README.md#7-永续全仓保证金与强平)、[产生的事件](README.md#产生的事件)
- 级别:unit
- 里程碑:M2
- 前置:SIM-071 第 3 行的仓位
- 步骤:
  1. 推进 1 s 后取最近一条 `MarginReport`
- 期望:
  - `margin_balance` 18 000、`maint_margin` 450、`avail_margin` 0(初始 12 000 + 6 000)、`margin_ratio` 0.025;条目 `(BTC/P2, liq_px 77 462.6866, adl_quantile 0)`、`(ETH/P2, liq_px 4 746.2687, adl_quantile 0)`(47 700 ÷ 10.05,空头向上取整)
- 实现:`crates/hft-sim/tests/margin.rs::sim_075_margin_report_fields`

### SIM-076 MarginReport 的推送节拍

同一节拍覆盖空仓账户的启动报告与持续心跳,条目数与保证金比例均为零。

- 规则:[产生的事件](README.md#产生的事件)
- 级别:table
- 里程碑:M2
- 前置:见上;上一条报告在 0 ms,比率 r0;按行用 Mark 把比率调到给定值,时刻为相对上一条报告
- 实现:`crates/hft-sim/tests/margin.rs::sim_076_margin_report_cadence`

| 行 | `margin_data` / 比率 / 时刻 | 期望 |
|---|---|---|
| SIM-076.1 | `Push` / r0 + 0.009999 / 500 ms | 不发报告 |
| SIM-076.2 | `Push` / r0 + 0.01 / 500 ms | 发一条 |
| SIM-076.3 | `Push` / r0 / 999 ms | 不发 |
| SIM-076.4 | `Push` / r0 / 1 000 ms | 发一条 |
| SIM-076.5 | `Poll{5 000}` / r0 + 0.05 / 4 999 ms | 不发 |
| SIM-076.6 | `Poll{5 000}` / r0 / 5 000 ms | 发一条 |

### SIM-077 AccountConfigReport 恒与 VenueCaps 一致且带杠杆条目

- 规则:[产生的事件](README.md#产生的事件)、[配置](README.md#配置)
- 级别:unit
- 里程碑:M2
- 前置:`rd_sim()`、`setup_sim()`
- 步骤:
  1. 新建后 `drain_due` 启动回报
- 期望:
  - 每个 venue 一条 `AccountConfigReport`:`mismatches = 0`,`uid_keys = 0`;`P2` 的条目为每个可交易永续 inst 一条 `LeverageItem{leverage 5.0(Rate raw 5 000 000 000)}`(`BTC/P2`、`ETH/P2`);`S1` 的 `n_lev = 0`
- 实现:`crates/hft-sim/tests/events.rs::sim_077_account_config_report`

### SIM-080 挂单锁定额并在撤单后释放

- 规则:[8. 现货余额与划转](README.md#8-现货余额与划转)
- 级别:unit
- 里程碑:M2
- 前置:同 SIM-020;`S1` `fee_asset_rule = Quote`;卖一 60 000
- 步骤:
  1. `Place(Buy, 0.1, 59 000, GTC) decided @100 ms`;推进到 110 ms,读 USDT
  2. `Cancel(target slot 1) decided @200 ms`;推进到 210 ms
- 期望:
  - 步骤 1:USDT `locked` 5 902.36(5 900 + taker 费 2.36),`free` 94 097.64
  - 步骤 2:`CancelAck`;`locked` 0,`free` 100 000
- 实现:`crates/hft-sim/tests/spot.rs::sim_080_resting_lock`

### SIM-081 挂单部分成交按比例释放锁定并结算

- 规则:[8. 现货余额与划转](README.md#8-现货余额与划转)
- 级别:unit
- 里程碑:M2
- 前置:SIM-080 步骤 1 之后
- 步骤:
  1. `Trade(BTC/S1, 58 990 × 0.04)`
- 期望:
  - 成交 0.04 @ 59 000,maker 费 0.472;USDT `total` 97 639.528(100 000 − 2 360 − 0.472),`locked` 3 541.416(释放 40%),`free` 94 098.112;BTC +0.04
- 实现:`crates/hft-sim/tests/spot.rs::sim_081_partial_fill_releases_lock`

### SIM-082 SimTransfer 源端即时扣减、目标端延迟到账

- 规则:[8. 现货余额与划转](README.md#8-现货余额与划转)、[配置](README.md#配置)
- 级别:table
- 里程碑:M2
- 前置:`rd_sim()`、`setup_sim()`(`transfer_delay_ms = 300 000`);最后一条记录 `@1 000 ms`;`on_command(sim_transfer(A9, S1, P2, USDT, 1 000))`
- 实现:`crates/hft-sim/tests/spot.rs::sim_082_transfer_delay`

| 行 | 推进到 | 期望 |
|---|---|---|
| SIM-082.1 | 1 000 ms | `S1` USDT 99 000;`P2` USDT 18 000;一条 `S1` 的 `BalanceReport` |
| SIM-082.2 | 301 000 ms − 1 ns | `P2` 仍为 18 000 |
| SIM-082.3 | 301 000 ms | `P2` 19 000;一条 `P2` 的 `BalanceReport` |

### SIM-083 SimReset 撤单、恢复初始余额、清零仓位与记忆并发真值报告

- 规则:[8. 现货余额与划转](README.md#8-现货余额与划转)、[产生的事件](README.md#产生的事件)
- 级别:unit
- 里程碑:M2
- 前置:`A9` 有挂单 2 张、现货与永续持仓、记忆条目、余额已变化
- 步骤:
  1. `on_command(sim_reset(A9))`;`drain_due`
- 期望:
  - 两条 `CancelAck`;余额 = `initial_balances`(S1 USDT 100 000、S1 BNB 1、P2 USDT 18 000);全部仓位 0;记忆为空;回报含每资产一条 `BalanceReport`、每永续 inst 一条 `PositionReport` 与每 venue 一条 `AccountConfigReport`
  - 此后对重置前订单的 `Trade` 穿价不再产生成交
- 实现:`crates/hft-sim/tests/spot.rs::sim_083_sim_reset`

### SIM-090 每条拒单规则一正一反,首个不满足的规则决定原因

- 规则:[拒单规则](README.md#拒单规则)
- 级别:table
- 里程碑:M2
- 前置:`rd_sim()`、`setup_sim()`;`S1` `fee_asset_rule = Quote`;`TopN(BTC/S1, asks [60 000×1], bids [59 990×1])`;`Bbo(BTC/P2, 59 999.9×10, 60 000.0×10)`、`Mark(BTC/P2, 60 000)`;永续行先 `seed9(P2 USDT 18 000;BTC/P2 −1.0 @ 60 000)`(第 13 行无仓位);每行一条指令,期望列的"拒"为 `Reject{reject_code = 0x1000_0000 + n}` 且无 `Ack`,"过"为 `Ack`
- 实现:`crates/hft-sim/tests/rules.rs::sim_090_reject_rules`

| 行 | 条件 / 指令 | 期望 |
|---|---|---|
| SIM-090.1 | `BTC/S1` 状态 `Halted`;`Place(BTC/S1, Buy, 0.1, 60 000, IOC)` | 拒,n = 1 |
| SIM-090.2 | 状态 `Trading`;同上 | 过 |
| SIM-090.3 | `VenueNotice{Delisting}` 使 `BTC/P2` 只减仓;`Place(BTC/P2, Sell, 0.1, 60 000, IOC)`(增空) | 拒,n = 1 |
| SIM-090.4 | 同上;`Place(BTC/P2, Buy, 0.1, 60 000, IOC)`(减空) | 过 |
| SIM-090.5 | `Place(BTC/S1, Buy, 0.000015, 60 000, IOC)` | 拒,n = 2 |
| SIM-090.6 | `BTC/S1` 的 `min_qty` 改为 0.0001;`Place(…, 0.00005, …)` | 拒,n = 2 |
| SIM-090.7 | 同上;`Place(…, 0.0001, 60 000, IOC)` | 过 |
| SIM-090.8 | `Place(BTC/S1, Buy, 0.1, 60 000.005, IOC)` | 拒,n = 3 |
| SIM-090.9 | `Place(BTC/S1, Buy, 0.00008, 60 000, IOC)`(名义 4.8) | 拒,n = 4 |
| SIM-090.10 | `Place(BTC/S1, Buy, 0.0001, 50 000, IOC)`(名义恰为 5.0) | 过(随后 `Expired`) |
| SIM-090.11 | refdata 无价格限制字段;远离市价的 `Place(BTC/S1, Buy, 1.0, 10.00, IOC)`(名义 10) | 第 5 条跳过:过(随后 `Expired`) |
| SIM-090.12 | `Place(BTC/S1, Sell, 0.1, 60 000, IOC)` 带 `REDUCE_ONLY`(`S1.reduce_only = false`) | 拒,n = 6 |
| SIM-090.13 | 无永续仓位;`Place(BTC/P2, Buy, 0.1, 60 000, IOC)` 带 `REDUCE_ONLY` | 拒,n = 7 |
| SIM-090.14 | 空 1.0;`REDUCE_ONLY` 的 `Buy 0.5` | 过 |
| SIM-090.15 | 空 1.0;`REDUCE_ONLY` 的 `Buy 1.5` | 拒,n = 7 |
| SIM-090.16 | `Place(BTC/S1, Buy, 0.1, 60 000, PostOnly)`(会立即成交) | 拒,n = 8 |
| SIM-090.17 | `Place(BTC/S1, Buy, 0.1, 59 995, PostOnly)` | 过,成为挂单 |
| SIM-090.18 | `S1` USDT 初始 6 002.39;`Place(BTC/S1, Buy, 0.1, 60 000, IOC)`(需 6 000 + 2.4) | 拒,n = 9 |
| SIM-090.19 | `S1` USDT 初始 6 002.40;同上 | 过 |
| SIM-090.20 | `A9` BTC 0.09999;`Place(BTC/S1, Sell, 0.1, 59 990, IOC)` | 拒,n = 9 |
| SIM-090.21 | 空 1.0(可用 6 000);`Place(BTC/P2, Sell, 0.5, 60 000, IOC)`(新增初始 6 000) | 过 |
| SIM-090.22 | 同上;`Sell 0.501`(新增初始 6 012) | 拒,n = 10 |
| SIM-090.23 | `rate_limit = true`,`RateLimitTable` (S1, Account, Orders, 10 / 1 000 ms);1 s 内第 10 条 `Place` | 过 |
| SIM-090.24 | 同上;第 11 条 | 拒,n = 11 |
| SIM-090.25 | `rate_limit = false`;1 s 内第 11 条 | 过 |
| SIM-090.26 | `Place(BTC/S1, Buy, 0.000015, 60 000.005, IOC)`(同时违反第 2、3 条) | 拒,n = 2 |

### SIM-091 拒单不改余额与仓位

- 规则:[拒单规则](README.md#拒单规则)、[产生的事件](README.md#产生的事件)
- 级别:unit
- 里程碑:M2
- 前置:SIM-090 第 9 行
- 步骤:
  1. 记下 `account(A9)`,下单并推进 10 ms
- 期望:
  - 唯一回报为 `Reject`(slot 1、gen 1,`reject_code = 0x1000_0004`);`account(A9)` 与下单前逐字段相同;订单表中该单为终态 Rejected
- 实现:`crates/hft-sim/tests/rules.rs::sim_091_reject_no_side_effects`

### SIM-100 撤单:挂单回 CancelAck,其余回 CancelReject

- 规则:[产生的事件](README.md#产生的事件)
- 级别:table
- 里程碑:M2
- 前置:SIM-080 步骤 1 之后(slot 1 为挂单);slot 2 为已成交的 IOC
- 实现:`crates/hft-sim/tests/commands.rs::sim_100_cancel`
- 身份回归:`sim_100_cancel_targets_order_identity`;slot 复用后撤旧代仍只读旧单,不存在的代数或品种回 `CancelReject` 且保留请求身份、成交与现金字段为零;当前挂单及锁定余额不变,当前代仍可撤销。固定 DST seed 见 [core.txt](../../fixtures/dst-seeds/core.txt)。

| 行 | 指令 | 期望 |
|---|---|---|
| SIM-100.1 | `Cancel(target slot 1)` | `CancelAck`;锁定释放 |
| SIM-100.2 | `Cancel(target slot 2)`(已终态) | `CancelReject`;余额不变 |
| SIM-100.3 | `Cancel(target slot 99)`(订单表中没有) | `CancelReject` |

### SIM-101 改单:挂单且新价量通过规则时 AmendAck,否则 AmendReject

- 规则:[产生的事件](README.md#产生的事件)、[拒单规则](README.md#拒单规则)
- 级别:table
- 里程碑:M2
- 前置:SIM-080 步骤 1 之后
- 实现:`crates/hft-sim/tests/commands.rs::sim_101_amend`
- 身份回归:`sim_101_amend_targets_order_identity`;同 SIM-100 的旧代、未知代与错误品种,只拒绝对应目标,不修改当前挂单与锁定余额;当前代仍可改单。

| 行 | 指令 | 期望 |
|---|---|---|
| SIM-101.1 | `Amend(slot 1, px 59 500, qty 0.1)` | `AmendAck`;`locked` 5 952.38(5 950 + 2.38) |
| SIM-101.2 | `Amend(slot 1, px 59 500.005, qty 0.1)` | `AmendReject`;挂单仍为 59 000、`locked` 5 902.36 |
| SIM-101.3 | slot 1 改为以 `PostOnly` 挂出的同一单;`Amend(slot 1, px 60 000, qty 0.1)`(会立即成交) | `AmendReject`(第 7 条) |
| SIM-101.4 | `Amend` 指向已终态的 slot 2 | `AmendReject` |

### SIM-102 CancelAll 按 inst 或整个账户撤挂单

- 规则:[产生的事件](README.md#产生的事件)
- 级别:table
- 里程碑:M2
- 前置:`A9` 有三张挂单:两张 `BTC/S1`、一张 `BNB/S1`
- 实现:`crates/hft-sim/tests/commands.rs::sim_102_cancel_all`

| 行 | 指令 | 期望 |
|---|---|---|
| SIM-102.1 | `CancelAll{inst = BTC/S1}` | 两条 `CancelAck`(BTC 两张);`BNB/S1` 挂单仍在 |
| SIM-102.2 | `CancelAll{inst = 0}` | 三条 `CancelAck` |
| SIM-102.3 | 无挂单时 `CancelAll{inst = 0}` | 无 `OrderEvent` |

### SIM-103 查单按订单表回 QueryResult

- 规则:[产生的事件](README.md#产生的事件)
- 级别:table
- 里程碑:M2
- 前置:`A9` 订单表:slot 1 挂单(0.1 中已成交 0.04 @ 59 000)、slot 2 IOC 已全部成交 0.1 @ 60 000、slot 3 IOC `Expired`(成交 0.03 @ 60 000)、slot 4 被拒
- 实现:`crates/hft-sim/tests/commands.rs::sim_103_query`
- 身份回归:`sim_103_query_targets_order_identity`;slot 复用后查询保留的旧代返回该代累计成交,未知代或错误品种返回 `NotFound`、请求身份与零成交/现金,当前代仍返回当前挂单。

| 行 | `Query(target)` | 期望 `QueryResult` |
|---|---|---|
| SIM-103.1 | slot 1 | `order_status` PartiallyFilled(2),`cum_filled` 0.04,`avg_px` 59 000 |
| SIM-103.2 | slot 2 | Filled(3),`cum_filled` 0.1,`avg_px` 60 000 |
| SIM-103.3 | slot 3 | Expired(6),`cum_filled` 0.03,`avg_px` 60 000 |
| SIM-103.4 | slot 4 | Rejected(5),`cum_filled` 0,`avg_px` 0 |
| SIM-103.5 | slot 99(从未出现) | NotFound(7) |
| SIM-103.6 | 挂单 slot 5 无成交 | New(1),`cum_filled` 0 |

### SIM-104 订单表满 1 024 项时淘汰最早终结的记录

- 规则:[产生的事件](README.md#产生的事件)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M2
- 前置:`A9` 依次完成 1 024 张 IOC(全部终态)
- 步骤:
  1. 再下第 1 025 张;`Query` 第 1 张;`Query` 第 2 张
- 期望:
  - 第 1 张 → NotFound;第 2 张 → 其终态;订单表仍为 1 024 项
- 实现:`crates/hft-sim/tests/commands.rs::sim_104_order_table_capacity`

### SIM-105 维护类指令被丢弃且不回报

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:table
- 里程碑:M4
- 前置:`SimGwTask` 用测试环与假 journal;`out-sim` 中依次放入 `DeadManRefresh`、`Rotate`、`ResetRate`、`Resubscribe`、`SetEpoch`、`SessionKeepalive`、`MirrorRestore` 各一条
- 实现:`crates/hft-sim/tests/task.rs::sim_105_maintenance_ignored`

| 行 | 断言 | 期望 |
|---|---|---|
| SIM-105.1 | `rep-sim` 中的元素 | 0 条(无 `SendReport`、无 `ConnState`) |
| SIM-105.2 | `maint_ignored` 计数 | 7 |
| SIM-105.3 | `SimExchange::state_hash()` | 与处理前相同 |

### SIM-110 订单回报序列与字段

- 规则:[产生的事件](README.md#产生的事件)
- 级别:unit
- 里程碑:M2
- 前置:SIM-030 的前置;`TopN` 记录的 `ex_ts_ns` = 99 ms(与记录头 100 ms 不同)
- 步骤:
  1. `Place(Buy, 0.25, 60 015, IOC) decided @100 ms`,`drain_due` 全部回报
- 期望:
  - 依次 `Ack`、`PartialFill`、`Fill`,三条的 `slot = 1`、`gen = 1`;两条成交的 `ex_ts_ns` = 99 ms(撮合所用记录的交易所时间);`cum_filled` 单调,最后一条 = 0.25
- 实现:`crates/hft-sim/tests/events.rs::sim_110_order_event_sequence`

### SIM-111 真值报告按 report_interval_s 周期发出

- 规则:[产生的事件](README.md#产生的事件)、[配置](README.md#配置)
- 级别:table
- 里程碑:M2
- 前置:`rd_sim()`、`setup_sim()`(`report_interval_s = 60`);`seed9(S1 USDT 100 000、BTC 0.1;P2 USDT 18 000;BTC/P2 −1.0 @ 60 000)`,上一轮真值报告在 0 s
- 实现:`crates/hft-sim/tests/events.rs::sim_111_truth_report_interval`

| 行 | 推进到 | 期望 |
|---|---|---|
| SIM-111.1 | 60 s − 1 ns | 无新的 `BalanceReport`/`PositionReport` |
| SIM-111.2 | 60 s | 每 (venue, asset) 一条 `BalanceReport`(`S1` USDT free 100 000、`S1` BTC 0.1、`P2` USDT 18 000)与 `PositionReport{BTC/P2, qty −1.0, entry_px 60 000}`,数值等于 `account(A9)` |

### SIM-112 启动时每个 Sim 账户一次 ConnState{Up}

- 规则:[产生的事件](README.md#产生的事件)
- 级别:unit
- 里程碑:M2
- 前置:`setup_sim()` 加入 `A8`
- 步骤:
  1. 新建后 `drain_due` 启动回报;推进 10 min 再 `drain_due`
- 期望:
  - 启动回报中 `ConnState{Up}` 共 2 条(每账户一条,`conn = 0`);之后的 10 min 内没有 `ConnState`
- 实现:`crates/hft-sim/tests/events.rs::sim_112_conn_up_once`

### SIM-113 set_order_conn 置 Down 期间的订单按下单连接 Down 处理

- 规则:[纯核心 API](README.md#纯核心-api)
- 级别:unit
- 里程碑:M2
- 前置:SIM-030 的前置
- 步骤:
  1. `set_order_conn(A9, S1, up = false, 200 ms)`;`Place(Buy, 0.1, 60 015, IOC) decided @201 ms`;`Cancel` 与 `Query` 各一条
  2. `set_order_conn(A9, S1, up = true, 300 ms)`;`Place(Buy, 0.1, 60 015, IOC) decided @301 ms`
- 期望:
  - 步骤 1:`ConnState{Down}` 一条;`Place` 得 `SendReport{status: NoConn}`,无 `Ack`、无成交;`Cancel`、`Query` 同样得 `NoConn`
  - 步骤 2:`ConnState{Up}` 一条;第二张单得 `Ack` 并成交 0.1
- 实现:`crates/hft-sim/tests/events.rs::sim_113_order_conn_down`

### SIM-220 external_fill 按无手续费的成交结算仓位、已实现盈亏与现货余额,不产出回报

- 规则:[纯核心 API](README.md#纯核心-api)
- 级别:unit
- 里程碑:M2
- 前置:SIM-072 的账户 A9(USDT 18 000);`seed` 使 A9 在 BTC_P2 持仓 −1.0、均价 60 000
- 步骤:
  1. `external_fill(A9, BTC_P2, Buy, 0.4, 59 000)`
  2. `external_fill(A9, BTC_S1, Buy, 0.1, 60 000)`
- 期望:
  - 步骤 1 返回 `Some(400 USDT)`;BTC_P2 仓位 −0.6,USDT 余额 18 400(已实现 +400,无手续费)
  - 步骤 2 返回 `Some(0)`;S1 的 BTC 余额 +0.1、USDT −6 000
  - 两步都没有新的回报(`drain_due` 无输出)
- 实现:`crates/hft-sim/tests/events.rs::sim_220_external_fill`

### SIM-221 部分减仓的移出成本与已实现盈亏向零取整

- 规则:[7. 永续全仓保证金与强平](README.md#7-永续全仓保证金与强平)
- 级别:unit
- 里程碑:M2
- 前置:账户 A9 永续钱包 USDT 18 000,无持仓
- 步骤:
  1. `external_fill(A9, BTC_P2, Sell, 0.1, 60 000)`;`external_fill(A9, BTC_P2, Sell, 0.2, 60 000.0001)`
  2. `external_fill(A9, BTC_P2, Buy, 0.1, 60 000.0002)`
- 期望:
  - 两次开仓返回 `Some(0)`;步骤 2 返回 `Some(−13)`(USDT 最小单位),仓位 −0.2;移出成本 = 18 000 000 002 × 10⁶ / 3 向零取整 = 6 000 000 006 666 666(10⁻¹²);已实现 −13 333 334(10⁻¹²)向零取整到 USDT 最小单位 = −0.000013,钱包 = 17 999.999987
- 实现:`crates/hft-sim/tests/events.rs::sim_221_partial_close_rounds_toward_zero`

### SIM-120 同一输入与种子两次运行的回报序列与 state_hash 轨迹逐项相等

- 规则:[9. 确定性](README.md#9-确定性)
- 级别:prop
- 里程碑:M2
- 性质:同一 `(SimSetup, refdata, 记录与指令序列)` 运行两次,`drain_due` 交付的回报序列逐字节相等,每处理一条输入后的 `state_hash()` 序列逐项相等
- 生成:1–4 个账户、1–4 个 inst(`rd_sim()` 的可交易 inst);0–400 个输入,行情记录(Bbo、TopN、L2Batch、Trade、Funding、Mark)与指令(下单、撤、改、查、CancelAll、`SimTransfer`、`SimReset`)混合;`Empirical` 与 `Fixed` 延迟模型各半
- 参考:第一次运行
- 例数:PR 500;每夜 20 000
- 实现:`crates/hft-sim/tests/prop_determinism.rs::sim_120_same_seed_same_trace`

### SIM-121 换种子只改变延迟抽样

- 规则:[9. 确定性](README.md#9-确定性)
- 级别:prop
- 里程碑:M2
- 性质:`Fixed` 延迟模型下任意两个种子的回报序列逐字节相等;`Empirical` 下 `due_tsc` 可不同,而每张单的成交量之和与 §10 不变量照常成立
- 生成:同 SIM-120;种子取两个随机 u64
- 参考:同一输入的另一种子运行
- 例数:PR 500;每夜 20 000
- 实现:`crates/hft-sim/tests/prop_determinism.rs::sim_121_seed_only_affects_sampling`

### SIM-122 新增账户不扰动已有账户的子流

- 规则:[9. 确定性](README.md#9-确定性)
- 级别:unit
- 里程碑:M2
- 前置:SIM-004 的前置,但 R2 的 `A8` 的 `AccountId` 小于 `A9`
- 步骤:
  1. 比较两次运行 `A9` 的回报
- 期望:
  - `A9` 的回报序列(含 `due_tsc`)逐字节相等
- 实现:`crates/hft-sim/tests/prop_determinism.rs::sim_122_new_account_no_perturbation`

### SIM-123 固定输入的回报序列在 x86_64 与 aarch64 上逐字节一致

- 规则:[9. 确定性](README.md#9-确定性)
- 级别:golden
- 里程碑:M2
- 输入:`fixtures/sim/sim-123-mixed/` 的 `records.jsonl`、`commands.jsonl`、`setup.toml`(含 `Empirical` 延迟、强平、资金费、`RefdataTiers`)
- 期望文件:`fixtures/sim/sim-123-mixed/expected-replies.bin`(逐条回报字节)与 `expected-state-hash.txt`(每个输入之后的 `state_hash`)
- 实现:`crates/hft-sim/tests/golden.rs::sim_123_cross_arch_golden`

- 空仓保证金:强平后仍输出零仓项目的债务保证金报告;maint 为零,负钱包与饱和风险率保留,相应回报队列状态参与 state_hash。

### SIM-124 资金守恒、仓位与成交量上界、回报时序三组不变量

- 规则:[10. 不变量](README.md#10-不变量)
- 级别:prop
- 里程碑:M2
- 性质:每处理一条输入并交付全部待交付回报后:① 每 (账户, venue, asset) 余额变化 = Σ 成交 ± 手续费 ± 资金费 ± 划转 ± 强平费,逐笔可追溯;② 每个永续仓位 = Σ 成交(含强平);每档成交量 ≤ 可见量 × `take_avail_bp` / 10 000 − 记忆剩余量;③ 每条回报 `due_tsc ≥` 其指令的 `decided_tsc`,同一订单回报的 `ex_ts_ns` 单调不减,`cum_filled ≤` 下单量
- 生成:同 SIM-120,但 0–150 个输入
- 参考:测试内逐笔记账模型,每批回报完整交付后按 `gen_seq` 核算成交与资金费,每条回报的时序检查仍保留原始交付顺序;Transfer/Reset 前先核算其前已产生的回报
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-sim/tests/prop_invariants.rs::sim_124_invariants`

### SIM-125 金样:一段行情与指令脚本的期望回报序列

- 规则:[3. 吃单撮合](README.md#3-吃单撮合)、[5. 手续费与扣费资产](README.md#5-手续费与扣费资产)、[6. 资金费](README.md#6-资金费)、[7. 永续全仓保证金与强平](README.md#7-永续全仓保证金与强平)
- 级别:golden
- 里程碑:M2
- 输入:`fixtures/sim/sim-125-<case>/`(`taker-walk`、`fee-rules`、`funding-push-forms`、`liquidation`、`transfer-reset` 五组;每组 `records.jsonl`、`commands.jsonl`、`setup.toml`)
- 期望文件:各组目录下的 `expected.jsonl`(人工核对后入库)
- 实现:`crates/hft-sim/tests/golden.rs::sim_125_replay_fixtures`

- 空仓报告同样按 Push 周期发出,划转/重置后的钱包金额来自该脚本;正钱包的零仓比例为零,负钱包仍保留饱和风险率。

### SIM-114 回报只用实盘同一组事件类型

- 规则:[模拟账户是什么](README.md#模拟账户是什么)、[产生的事件](README.md#产生的事件)
- 级别:prop
- 里程碑:M2
- 性质:任意输入下 `drain_due` 交付的每个 `SimReply::Rep(m)` 的 `m` 都是 hft-event `RepMsg` 已登记的变体(`OrderEvent`、`SendReport`、`FundingSettle`、`MarginReport`、`ExternalFill`、`AccountConfigReport`、`PositionReport`、`BalanceReport`、`ConnState`),且按 hft-event 的编码器编码成功
- 生成:同 SIM-120
- 参考:hft-event 的 `RepMsg` 变体表与编码器
- 例数:PR 500;每夜 20 000
- 实现:`crates/hft-sim/tests/prop_determinism.rs::sim_114_replies_are_standard_events`

### SIM-140 poll_once 按固定顺序:尾随 journal、取指令、推进、交付

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:unit
- 里程碑:M4
- 前置:`SimGwTask` 用测试环与内存 journal;journal 末尾为 `Bbo(BTC/S1, …) @100 ms` 与 `A9` slot 1 的 `OrderDecision`;`out-sim` 中有对应的 `GwCmd{Place, slot 1}`;`SimExchange` 换成记录调用顺序的测试替身
- 步骤:
  1. `poll_once(now)` 一次
- 期望:
  - 调用顺序:`on_record`(该 Bbo)→ `on_command`(Place)→ `advance(100 ms)` → `drain_due(now)`;返回 `true`
- 实现:`crates/hft-sim/tests/task.rs::sim_140_poll_order`

### SIM-141 订单类指令的 decided_tsc 取 journal 中同 (account, slot, gen) 的 OrderDecision

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:unit
- 里程碑:M4
- 前置:同 SIM-140,但 `OrderDecision.decided_tsc` = 100 ms,而 `GwCmd.decided_tsc` = 999 ms
- 步骤:
  1. `poll_once`,推进到 110 ms 并交付
- 期望:
  - 成交回报的 `due_tsc` = 104 ms(按 100 ms + 2 ms + 2 ms)
- 实现:`crates/hft-sim/tests/task.rs::sim_141_decided_tsc_from_journal`

### SIM-142 模拟命令以已消费的最后一条记录的 tsc 为时刻

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:unit
- 里程碑:M4
- 前置:同 SIM-140;已消费的最后一条记录 `@5 000 ms`;`out-sim` 中一条 `sim_transfer(A9, S1, P2, USDT, 1 000)`
- 步骤:
  1. `poll_once`;再喂记录到 `305 000 ms`
- 期望:
  - `S1` 在 5 000 ms 扣减;`P2` 在 305 000 ms 入账
- 实现:`crates/hft-sim/tests/task.rs::sim_142_sim_commands_at_last_record`

### SIM-143 每次调用至多尾随 4 096 条记录

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:table
- 里程碑:M4
- 前置:journal 中有 10 000 条未消费的行情记录
- 实现:`crates/hft-sim/tests/task.rs::sim_143_tail_batch_limit`

| 行 | 调用 | 期望本次 `on_record` 次数 |
|---|---|---|
| SIM-143.1 | 第 1 次 `poll_once` | 4 096 |
| SIM-143.2 | 第 2 次 | 4 096 |
| SIM-143.3 | 第 3 次 | 1 808 |
| SIM-143.4 | 第 4 次 | 0,返回 `false` |

### SIM-144 journal 静默 quiet_ms 后以本地 TSC 推进并计数

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)、[配置](README.md#配置)
- 级别:table
- 里程碑:M4
- 前置:`quiet_ms = 50`;最后一条记录 `@100 ms`;一条待撮合指令(到达 102 ms);假时钟给出 `now`
- 实现:`crates/hft-sim/tests/task.rs::sim_144_quiet_advance`

| 行 | 自最后一条记录起无新记录的时长 | 期望 |
|---|---|---|
| SIM-144.1 | 49 ms | 不推进;`quiet_advance` 0 |
| SIM-144.2 | 50 ms | 以本地 TSC 推进,该单撮合;`quiet_advance` 1 |
| SIM-144.3 | 50 ms 但没有待撮合指令 | 不推进;`quiet_advance` 0 |

### SIM-145 rep-sim 环满时回报留在待交付堆、按序重试、从不丢弃

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:unit
- 里程碑:M4
- 前置:`rep-sim` 容量 8;20 条回报同时到期;sequencer 侧不取
- 步骤:
  1. `poll_once` 三次,每次之间测试从环中取走全部元素
- 期望:
  - 三次推入 8、8、4 条;取出的 20 条按 `(due_tsc, gen_seq)` 升序且与无容量限制时的序列逐条相同
- 实现:`crates/hft-sim/tests/task.rs::sim_145_ring_full_no_drop`

### SIM-146 回报洪峰时停止取指令与推进,只交付

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:table
- 里程碑:M4
- 前置:`rep-sim` 不被取走(sequencer 被阻塞);待交付堆按行预先填到给定剩余容量;`out-sim` 中有 10 条 `Place`,journal 有新行情
- 实现:`crates/hft-sim/tests/task.rs::sim_146_reply_backpressure`

| 行 | 待交付堆剩余容量 | 期望本次 `poll_once` |
|---|---|---|
| SIM-146.1 | 4 096 | 取指令并推进;`reply_backpressure` 不变 |
| SIM-146.2 | 4 095 | 不取指令(`out-sim` 仍 10 条)、不调 `advance`;只尾随 journal 与交付;`reply_backpressure` +1 |
| SIM-146.3 | 行 2 之后测试取走环中元素使剩余回到 ≥ 4 096,再 `poll_once` | 10 条指令被处理;全部回报的内容与 `due_tsc` 与无洪峰的运行逐条相同 |

### SIM-147 每次调用至多取 256 条指令

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:unit
- 里程碑:M4
- 前置:`out-sim` 中 300 条订单类指令,journal 中有对应的 `OrderDecision`
- 步骤:
  1. `poll_once` 两次
- 期望:
  - 第一次 `on_command` 256 次,第二次 44 次
- 实现:`crates/hft-sim/tests/task.rs::sim_147_cmds_per_poll`

### SIM-148 next_deadline 给出最早的待交付回报时刻,空闲时不阻塞

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:unit
- 里程碑:M4
- 前置:同 SIM-140
- 步骤:
  1. 处理指令后读 `next_deadline()`;交付全部后再读;无任何输入时调 `poll_once`
- 期望:
  - 第一次为 `Some(104 ms)`;第二次 `None`;空闲的 `poll_once` 返回 `false` 且不调用任何阻塞或睡眠的系统调用(seccomp 测试过滤器下运行不被拦截)
- 实现:`crates/hft-sim/tests/task.rs::sim_148_next_deadline_and_no_block`

### SIM-160 seed_account_from 按恢复后的引擎状态建立模拟真值

- 规则:[重启恢复](README.md#重启恢复)
- 级别:unit
- 里程碑:M4
- 前置:`rd_sim()`、`setup_sim()`;`S1` `fee_asset_rule = Quote`
- 步骤:
  1. `seed_account_from(A9, SimSeedState { balances: [(S1, USDT, 99 000), (S1, BTC, 0.5), (P2, USDT, 17 500)], positions: [(BTC/P2, −1.0, 60 000)], orders: [{S1, BTC/S1, slot 7, gen 3, Buy, GTC, prev_state Live, qty 0.1, filled 0.04, px 59 000, avg_px 59 000, exch_order_id 70}], funding: [] })`
- 期望:
  - `S1` USDT `total` 99 000,`locked` 3 541.416(0.06 × 59 000 + 0.06 × 59 000 × 0.0004),`free` 95 458.584;BTC 0.5;`P2` `wallet` 17 500;`BTC/P2` 仓位 −1.0、入场价 60 000
  - 订单表 slot 7 / gen 3 为挂单,剩余 0.06
- 实现:`crates/hft-sim/tests/recovery.rs::sim_160_seed_mapping`

### SIM-161 非挂单的非终态槽恢复为终态记录,查单不出现 NotFound

- 规则:[重启恢复](README.md#重启恢复)
- 级别:table
- 里程碑:M4
- 前置:同 SIM-160;每行一个 `SimSeedOrder`,seed 后对该 slot 发 `Query`
- 实现:`crates/hft-sim/tests/recovery.rs::sim_161_seeded_orders_terminal`

| 行 | `tif` / `prev_state` / `qty` / `filled` | 期望 `QueryResult` |
|---|---|---|
| SIM-161.1 | IOC / PendingNew / 0.1 / 0 | Expired(6),`cum_filled` 0,`avg_px` 0 |
| SIM-161.2 | IOC / PendingNew / 0.1 / 0.03(`avg_px` 60 000) | Expired(6),`cum_filled` 0.03,`avg_px` 60 000 |
| SIM-161.3 | IOC / PendingNew / 0.1 / 0.1 | Filled(3),`cum_filled` 0.1 |
| SIM-161.4 | FOK / PendingNew / 0.1 / 0 | Expired(6) |
| SIM-161.5 | GTC / PendingNew / 0.1 / 0 | Expired(6)(`PendingNew` 不恢复为挂单) |
| SIM-161.6 | GTC / PendingCancel / 0.1 / 0 | New(1)(恢复为挂单) |
| SIM-161.7 | 任一行 | 回报中没有 `order_status = NotFound` 的 `QueryResult`;终态记录不产生成交 |

### SIM-162 恢复出的挂单之后照常按穿价成交

- 规则:[重启恢复](README.md#重启恢复)、[4. 挂单(预留给 MakerFirst)](README.md#4-挂单预留给-makerfirst)
- 级别:unit
- 里程碑:M4
- 前置:SIM-160 之后
- 步骤:
  1. `Trade(BTC/S1, 58 990 × 1)`
- 期望:
  - slot 7 / gen 3 成交 0.06 @ 59 000,`Fill{cum_filled 0.1}`(累计含恢复前的 0.04),`exch_order_id` 70
- 实现:`crates/hft-sim/tests/recovery.rs::sim_162_seeded_resting_fills`

### SIM-163 seed_account_from 的错误

- 规则:[重启恢复](README.md#重启恢复)
- 级别:table
- 里程碑:M4
- 前置:同 SIM-160
- 实现:`crates/hft-sim/tests/recovery.rs::sim_163_seed_errors`

| 行 | `SimSeedState` | 期望 |
|---|---|---|
| SIM-163.1 | `balances` 含 `(S4, USDT, 1)`(不在 `A9` 的 venue 集合) | `Err(UnknownVenue)`,账户状态不变 |
| SIM-163.2 | `balances` 含 refdata 没有的资产 | `Err(UnknownAsset)` |
| SIM-163.3 | `positions` 含 `S4` 上的 inst | `Err(UnknownVenue)` |

### SIM-164 seed 之后记忆、待撮合与待交付为空,在途划转不恢复,子流回到初始状态

- 规则:[重启恢复](README.md#重启恢复)
- 级别:unit
- 里程碑:M4
- 前置:`setup_sim()` 但 `latency_models = [Empirical{…}]`;`A9` 有记忆条目、一条在途 `SimTransfer`、待交付回报 5 条、已抽样 20 次
- 步骤:
  1. `seed_account_from(A9, SimSeedState{ balances: [(S1, USDT, 100 000), (P2, USDT, 18 000)] })`
  2. 下一单并读其 `due_tsc − decided_tsc`
- 期望:
  - 步骤 1 后 `next_due() == None`;记忆为空;划转的目标端此后不入账
  - 步骤 2 的延迟抽样等于全新 `A9` 的第一次抽样
- 实现:`crates/hft-sim/tests/recovery.rs::sim_164_seed_resets_transients`

### SIM-165 同一 SimSeedState 得到同一 state_hash

- 规则:[重启恢复](README.md#重启恢复)、[9. 确定性](README.md#9-确定性)
- 级别:unit
- 里程碑:M4
- 前置:两个新建的 `SimExchange` E1、E2
- 步骤:
  1. 对 E1、E2 用 SIM-160 的 `SimSeedState` 各 seed 一次;对 E1 再 seed 一次同一状态
- 期望:
  - 三次之后 E1 与 E2 的 `state_hash()` 相等
- 实现:`crates/hft-sim/tests/recovery.rs::sim_165_seed_deterministic`

### SIM-166 恢复的结算时刻、费率与 mark 在新数据到达前生效

- 规则:[重启恢复](README.md#重启恢复)、[6. 资金费](README.md#6-资金费)
- 级别:unit
- 里程碑:M4
- 前置:`P2` `funding_settle = PerSymbolPush`;seed:`P2` USDT 18 000、`BTC/P2` −1.0 @ 60 000、`funding: [(BTC/P2, T, 0.0001, 60 000)]`;恢复后没有新的 `Funding`/`Mark`
- 步骤:
  1. 喂一条 `ex_ts_ns = T` 的 `P2` 行情记录
- 期望:
  - 结算 `FundingSettle{BTC/P2, amount +6.0, settle_ts_ns T}`;`wallet` 18 006
- 实现:`crates/hft-sim/tests/recovery.rs::sim_166_seeded_funding`

### SIM-167 sim 实例崩溃于"已撮合未交付"之后,恢复的真值与账本一致且槽收敛

- 规则:[重启恢复](README.md#重启恢复)
- 级别:dst
- 里程碑:M4
- 前置:testkit 跑真实 sequencer + `SimActor`(sim-gw 同一用法),测试策略在 `A9` 上做 IOC 开平仓;恢复按"重启恢复"的步骤
- 故障:在 sim-gw 已撮合某张 IOC(成交已进模拟真值)而回报尚在待交付堆时整进程崩溃;另一半种子崩溃在回报已入 rep-sim 环、sequencer 尚未取走时;恢复走非干净重启
- 种子:PR 50 + 回归集;每夜日期派生
- 断言:恢复后该成交不在 ledger 中,也不在 `SimExchange::account(A9)` 中;OMS 对该槽的 `Query` 得到 `QueryResult{Expired, cum_filled = 恢复前的已入账量}`,槽在一个查询往返内终结;整个运行中没有 `order_status = NotFound` 的 `QueryResult`;恢复后第一组真值报告与 ledger 一致(无 `SimDrift` 条件);对同一份 journal 恢复两次得到的 `state_hash()` 相等;INV-6、INV-7、INV-16
- 实现:`crates/hft-testkit/src/scenarios/sim.rs::sim_167_crash_after_match_recovers`

### SIM-168 干净 Drain 重启后挂单仍在并照常成交

- 规则:[重启恢复](README.md#重启恢复)
- 级别:dst
- 里程碑:M4
- 前置:同 SIM-167,测试策略在 `A9` 上挂一张 GTC 买单
- 故障:挂单存续期间做干净 Drain 重启;重启后在挂价下方注入穿价成交
- 种子:PR 50 + 回归集;每夜日期派生
- 断言:恢复后订单表中该单为挂单、`locked` 与 ledger 推算一致;穿价后成交一次,ledger 与真值的余额与仓位一致;INV-7、INV-16
- 实现:`crates/hft-testkit/src/scenarios/sim.rs::sim_168_clean_drain_keeps_resting`

### SIM-169 首条真值报告交付前崩溃时不 seed,按 initial_balances 起步

- 规则:[重启恢复](README.md#重启恢复)
- 级别:dst
- 里程碑:M4
- 前置:同 SIM-167,新建的 Sim 账户
- 故障:sim 实例首次启动后、`A9` 的第一条 `BalanceReport` 被 sequencer 处理之前崩溃
- 种子:PR 50 + 回归集;每夜日期派生
- 断言:恢复时不调 `seed_account_from`(`A9` 在 ledger 中未初始化);`SimExchange::account(A9)` 等于 `initial_balances`;随后第一条真值报告使 ledger 初始化为同一余额;INV-16
- 实现:`crates/hft-testkit/src/scenarios/sim.rs::sim_169_crash_before_first_report`

### SIM-180 单边路径按 f(t) 变换价格并按方向取整到 tick

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:table
- 里程碑:M4
- 前置:`StressSpec{case OneSided30, path OneSided{3 000, ramp 3 600 000, hold 1 800 000}, scope AllInsts, qty_scale_bp 10 000, faults [], leverage Operating}`,`t0 = 0`,`step_ms = 1 000`;输入 `Bbo(BTC/P2, 60 000.0, 60 000.1)`,记录时刻按行
- 实现:`crates/hft-sim/tests/stress.rs::sim_180_one_sided_transform`

| 行 | 记录时刻 | 期望输出 Bbo |
|---|---|---|
| SIM-180.1 | 0 | 60 000.0 / 60 000.1(f = 1) |
| SIM-180.2 | 30 min | 69 000.0 / 69 000.2(f = 1.15;卖价 69 000.115 向上取到 tick) |
| SIM-180.3 | 60 min | 78 000.0 / 78 000.2(f = 1.30) |
| SIM-180.4 | 90 min(hold 内) | 78 000.0 / 78 000.2 |

### SIM-181 变换后的买卖价始终 bid < ask

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:prop
- 里程碑:M4
- 性质:任意 Bbo、TopN 快照与 f(t) 下,输出的每个 inst `bid < ask`;TopN 每一侧各档价格严格有序
- 生成:价格 [0.0001, 1e6] 内任意 tick 倍数,价差 1–100 tick;`pct_bp` ∈ [−5 000, 10 000];`extra_bp` ∈ [−1 000, 1 000];时刻覆盖 ramp 前、中、后
- 参考:逐条检查输出
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-sim/tests/prop_stress.rs::sim_181_bid_below_ask`

### SIM-182 StressScope 决定哪些记录变价,范围外的记录逐字节不变

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:table
- 里程碑:M4
- 前置:同 SIM-180,时刻 60 min;输入 `BTC/S1`、`BTC/P2`、`ETH/P2` 的 Bbo 各一条
- 实现:`crates/hft-sim/tests/stress.rs::sim_182_scope`

| 行 | `scope` | 期望 |
|---|---|---|
| SIM-182.1 | `AllInsts` | 三条都乘 1.30 |
| SIM-182.2 | `Asset(BTC)` | `BTC/S1`、`BTC/P2` 变价;`ETH/P2` 记录与输入逐字节相同 |
| SIM-182.3 | `Venue(P2)` | `BTC/P2`、`ETH/P2` 变价;`BTC/S1` 逐字节相同 |

### SIM-183 BasisWiden 下永续与现货的价比按 extra_bp 单调扩大

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:table
- 里程碑:M4
- 前置:`path = BasisWiden{3 000, 300, ramp 3 600 000, hold 1 800 000}`;输入 `BTC/S1` 与 `BTC/P2` 的 Bbo,中间价都为 60 000
- 实现:`crates/hft-sim/tests/stress.rs::sim_183_basis_widen`

| 行 | 时刻 | 期望 永续中间价 / 现货中间价 |
|---|---|---|
| SIM-183.1 | 0 | 1.000000 |
| SIM-183.2 | 30 min | 1.165 / 1.15 = 1.013043(±1 tick 的取整误差) |
| SIM-183.3 | 60 min | 1.33 / 1.30 = 1.023077 |
| SIM-183.4 | 行 1–3 | 单调不减 |

### SIM-184 qty_scale_bp 缩放盘口量并按 lot 向下取整

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:table
- 里程碑:M4
- 前置:同 SIM-180 但 `qty_scale_bp = 2 000`;`BTC/P2` lot 0.001
- 实现:`crates/hft-sim/tests/stress.rs::sim_184_qty_scale`

| 行 | 输入 | 期望 |
|---|---|---|
| SIM-184.1 | Bbo 买卖量各 1.0 | 0.2 / 0.2 |
| SIM-184.2 | Bbo 买量 0.004 | 0.001(0.0008 不足 1 lot,Bbo 至少保留 1 lot) |
| SIM-184.3 | L2 一档 0.004 | 该档被删除 |
| SIM-184.4 | L2 一档 1.0 | 0.2 |

### SIM-185 增量深度下每步开头输出变换后的快照,步内增量同一 f

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:unit
- 里程碑:M4
- 前置:同 SIM-180;增量深度启用;`step_ms = 1 000`;两个已订阅 inst
- 步骤:
  1. 输入 1 000 ms 至 2 999 ms 的 `L2Batch` 与 Bbo
- 期望:
  - 在 1 000 ms 与 2 000 ms 处各输出两条 `L2Snapshot`(每 inst 一条),档位为当时 f 变换后的原簿;步内 `L2Batch` 的价格按该步开头的 f 变换;任一时刻变换后簿的顶档与变换后 Bbo 相同(原簿顶档与原 Bbo 相同的前提下)
- 实现:`crates/hft-sim/tests/stress.rs::sim_185_step_snapshots`

### SIM-186 TopN 快照逐档按同一 f 变换

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:unit
- 里程碑:M4
- 前置:同 SIM-180,时刻 30 min
- 步骤:
  1. 输入 `TopN(BTC/P2, asks [60 000.1×1, 60 000.2×1], bids [60 000.0×1, 59 999.9×1])`
- 期望:
  - asks [69 000.2, 69 000.3],bids [69 000.0, 68 999.8](买价向下、卖价向上取到 tick);bid < ask
- 实现:`crates/hft-sim/tests/stress.rs::sim_186_topn_transform`

### SIM-187 资金费率与交易所时间不变,其余价格字段变换

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:table
- 里程碑:M4
- 前置:同 SIM-180,时刻 60 min
- 实现:`crates/hft-sim/tests/stress.rs::sim_187_fields_transformed`

| 行 | 输入记录 | 期望 |
|---|---|---|
| SIM-187.1 | `Trade(BTC/P2, 60 000 × 1)` | 价 78 000,量不变 |
| SIM-187.2 | `MarkPrice` 60 000 / `IndexPrice` 60 000 | 各 78 000 |
| SIM-187.3 | `Funding(rate 0.0001, next_funding T)` | 逐字节不变 |
| SIM-187.4 | 每条输出 | `ex_ts_ns` 与输入相同 |

### SIM-188 被测永续 inst 缺分档维持保证金率时拒绝运行

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)、[纯核心 API](README.md#纯核心-api)
- 级别:table
- 里程碑:M4
- 前置:`StressTransform::new(spec, t0, 1 000, rd, tested)`
- 实现:`crates/hft-sim/tests/stress.rs::sim_188_mmr_tiers_missing`

| 行 | refdata / `tested` | 期望 |
|---|---|---|
| SIM-188.1 | `BTC/P2` 无分档 / [BTC/S1, BTC/P2] | `Err(MmrTiersMissing)` |
| SIM-188.2 | `BTC/P2` 有分档,`ETH/P2` 无 / [BTC/S1, BTC/P2] | `Ok`(`ETH/P2` 不被测) |
| SIM-188.3 | 两者都有 / [BTC/P2, ETH/P2] | `Ok` |

### SIM-189 Venue 范围指向不存在的 venue 时报 UnknownVenue

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:unit
- 里程碑:M4
- 前置:`scope = Venue(VenueId(9))`(refdata 没有)
- 步骤:
  1. `StressTransform::new`
- 期望:
  - `Err(SimCfgErr::UnknownVenue)`
- 实现:`crates/hft-sim/tests/stress.rs::sim_189_unknown_venue_scope`

### SIM-190 每个命名压力用例的规格常量

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:table
- 里程碑:M4
- 前置:按 `--stress` 名取 `StressSpec`
- 实现:`crates/hft-sim/tests/stress.rs::sim_190_named_specs`

| 行 | 名字 | 期望 |
|---|---|---|
| SIM-190.1 | `one-sided-30` | 门槛;`OneSided{3 000, 3 600 000, 1 800 000}`;`AllInsts`;量 10 000;无故障;`Operating` |
| SIM-190.2 | `l3-path` | 门槛;同上但 `RiskCap`;升级运行为 `Operating`、`OneSided{4 500, 3 600 000, 1 800 000}` |
| SIM-190.3 | `basis-widen` | 非门槛;`BasisWiden{3 000, 300, 3 600 000, 1 800 000}` |
| SIM-190.4 | `fast-30` | 非门槛;`OneSided{3 000, 900 000, 1 800 000}` |
| SIM-190.5 | `gw-down` | 非门槛;`GwDown{venue None, at_ms 1 200 000, dur_ms 600 000}` |
| SIM-190.6 | `thin-book` | 非门槛;`qty_scale_bp = 2 000` |
| SIM-190.7 | `core-restart` | 非门槛;`CoreRestart{at_ms 1 800 000, down_ms 60 000}` |
| SIM-190.8 | `step_ms` 未给时 | 1 000(默认值用例) |

### SIM-191 RiskCap 杠杆按允许上限向下取整到整数倍

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:table
- 里程碑:M4
- 前置:`leverage = RiskCap`,`RiskParams.max_perp_leverage_x100` 按行
- 实现:`crates/hft-sim/tests/stress.rs::sim_191_riskcap_leverage`

| 行 | `max_perp_leverage_x100` | 期望被测账户 `MarginModel.leverage` |
|---|---|---|
| SIM-191.1 | 550 | 5 |
| SIM-191.2 | 500 | 5 |
| SIM-191.3 | 499 | 4 |

### SIM-192 判定:门槛用例只看零强平与 L3 路径,非门槛用例恒通过

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:table
- 里程碑:M4
- 前置:判定函数的输入为各次运行的 `liquidations` 与 `first_level_tsc`
- 实现:`crates/hft-sim/tests/stress.rs::sim_192_verdict_rules`

| 行 | 用例 / 运行结果 | 期望 `StressVerdict` |
|---|---|---|
| SIM-192.1 | `one-sided-30` / 强平 0 | `gate = true`,`passed = true` |
| SIM-192.2 | `one-sided-30` / 强平 1 | `passed = false` |
| SIM-192.3 | `l3-path` / 第一次运行 L1 < L2 < L3,强平 0 | `passed = true`,`escalated = false` |
| SIM-192.4 | `l3-path` / 第一次未进 L3、强平 0;第二次 L1 < L2 < L3、强平 0 | `passed = true`,`escalated = true` |
| SIM-192.5 | `l3-path` / 第一次未进 L3;第二次进 L3 但强平 1 | `passed = false` |
| SIM-192.6 | `l3-path` / 两次都未进 L3 | `passed = false` |
| SIM-192.7 | `l3-path` / L2 首次时刻早于 L1 | `passed = false` |
| SIM-192.8 | `basis-widen` / 强平 3 | `gate = false`,`passed = true`,`liquidations = 3` |
| SIM-192.9 | `Custom` / 强平 1 | `gate = false`,`passed = true` |

### SIM-193 同一输入与种子两次运行的 StressVerdict 逐字段相等

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:integ
- 里程碑:M4
- 前置:测试策略的满仓起点 journal 夹具;`replayer sim --stress all` 同一种子
- 步骤:
  1. 运行两次
- 期望:
  - 每个用例的 `stress-<case>.json` 两次逐字节相同
- 实现:`apps/replayer/tests/stress.rs::sim_193_verdict_deterministic`

### SIM-194 门槛用例对测试策略零强平,把 liq_dist_force 调到 0 时出现强平

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:integ
- 里程碑:M4
- 前置:同 SIM-193;测试策略的风控参数按 hft-risk 的默认分档
- 步骤:
  1. `--stress gate`
  2. 把 `RiskParams.liq_dist_force` 改为 0 的同一 journal 再跑
- 期望:
  - 步骤 1:两个门槛用例 `liquidations = 0`;`l3-path` 的 `first_level_tsc` 依次递增,或出现 `escalated` 运行;退出 0
  - 步骤 2:`one-sided-30` 的 `liquidations > 0`、`passed = false`,replayer 退出 1
- 实现:`apps/replayer/tests/stress.rs::sim_194_gate_cases_can_fail`

### SIM-195 GwDown 故障:venue 未给时取起点永续名义最大的 venue

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:integ
- 里程碑:M4
- 前置:起点快照中被测账户在两个永续 venue 上的名义为 30 000 与 50 000;`gw-down`
- 步骤:
  1. 运行 `gw-down`
- 期望:
  - `t0 + 20 min` 到 `t0 + 30 min` 之间对名义 50 000 的 venue 的订单得 `SendReport{NoConn}`,另一 venue 的订单得 `SendReport{Written}` 与 `Ack`;两端各一条 `ConnState`(Down、Up)
- 实现:`apps/replayer/tests/stress.rs::sim_195_gw_down_picks_largest_venue`

### SIM-196 CoreRestart 期间引擎无输入而模拟真值照常推进

- 规则:[压力用例(上线门槛必过项)](README.md#压力用例上线门槛必过项)
- 级别:integ
- 里程碑:M4
- 前置:同 SIM-193;`core-restart`
- 步骤:
  1. 运行 `core-restart`
- 期望:
  - `t0 + 30 min` 起 60 s 内 sequencer 不处理任何记录,`SimExchange` 照常处理行情(`MarginReport` 与资金费照常产生,满足条件时强平);恢复后引擎从最近快照起步,`Recovery` 的解除与 `sim` 实例的规则相同
- 实现:`apps/replayer/tests/stress.rs::sim_196_core_restart_sim_continues`

### SIM-200 Sim 专有报告计数

- 规则:[报告字段](README.md#报告字段)
- 级别:table
- 里程碑:M2
- 前置:同 SIM-020
- 实现:`crates/hft-sim/tests/events.rs::sim_200_sim_counters`

| 行 | 动作 | 期望计数 |
|---|---|---|
| SIM-200.1 | SIM-021 的成交 | `top_only_fills` +1 |
| SIM-200.2 | SIM-034.2 的第二单(可得量被记忆削减) | `liquidity_memory_hits` +1 |
| SIM-200.3 | SIM-072.2 的强平 | `liquidations` +1 |
| SIM-200.4 | 由 SIM-144.2 覆盖(`quiet_advance` 是 sim-gw 任务的计数,M4) | — |

### SIM-201 预置模拟实例池:换参数与清空重来都不重启

- 规则:[预置模拟实例池](README.md#预置模拟实例池)
- 级别:host
- 里程碑:M4
- 门槛:⑦
- 环境:dev2 `sim` 实例,refdata 为研究中的策略预置 `sim_pool = 2` 个模拟实例
- 授权:无
- 步骤:
  1. 对其中一个实例发 `SetParams`;对它的 Sim 账户发 `SimReset{account}`(经 ops_bot 或 `hft-ctl`)
- 通过标准:两条命令都 `Applied`;`hft-core@sim` 进程未重启(PID 与启动时刻不变);重置后该账户余额等于 `initial_balances`
- 证据:命令回执与 `systemctl show hft-core@sim` 的启动时刻,文件名含 `SIM-201`,归入门槛 ⑦ 记录
- 实现:`tests/host/SIM-201.sh`

### SIM-202 与交易所一致:拒单原因按 lot、tick、最小名义、价格限制、reduceOnly 边界与真实交易所相同

- 规则:[拒单规则](README.md#拒单规则)
- 级别:testnet
- 里程碑:M6
- 门槛:⑦
- 环境:Binance、OKX、Bitget 的现货与 USDT 永续测试网/模拟盘账户;同一 refdata 的 Sim 账户
- 授权:无
- 步骤:
  1. 对每个 venue 按 lot、tick、最小名义、价格限制(refdata 有该字段时)、reduceOnly 各下"边界、−1、+1"三张单,同样的单交给 `SimExchange`
- 通过标准:每张单真实拒单原因(经 venue 解码)与模拟的原因(n)一一对应;测试网与实盘规则不同的项逐条记录在证据中
- 证据:每 venue 一份对照表,文件名含 `SIM-202` 与 venue,归入门槛 ⑦ 记录;正式实盘开始后按 raw journal 复核
- 实现:`tests/host/SIM-202.sh`

### SIM-203 与交易所一致:资金费结算与交易所流水相对误差不超过 1e−6

- 规则:[6. 资金费](README.md#6-资金费)
- 级别:testnet
- 里程碑:M6
- 门槛:⑦
- 环境:提供资金费流水的测试网/模拟盘永续账户,持仓跨过至少一次结算
- 授权:无
- 步骤:
  1. 对同一仓位,用同一时刻 journal 中的 `Funding` 与 `Mark` 让 `SimExchange` 结算,与交易所流水比对
- 通过标准:每笔 |模拟 − 流水| / |流水| ≤ 1e−6
- 证据:比对表,文件名含 `SIM-203`,归入门槛 ⑦ 记录
- 实现:`tests/host/SIM-203.sh`

### SIM-204 模拟账户连续运行中资金守恒零违反

- 规则:[10. 不变量](README.md#10-不变量)
- 级别:host
- 里程碑:M6
- 门槛:⑦
- 环境:dev2 `sim` 实例,门槛 ⑦ 的 dev2 段
- 授权:无
- 步骤:
  1. 连续运行,每日对 `SimExchange` 的逐笔记账执行 §10 资金守恒检查(sim-gw 的检查计数随日报输出)
- 通过标准:每日违反计数 0
- 证据:日报中的守恒检查计数,文件名含 `SIM-204`
- 实现:`tests/host/SIM-204.sh`

### SIM-205 重演一致:无静默推进的区间逐笔复现

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)、[9. 确定性](README.md#9-确定性)
- 级别:replay
- 里程碑:M4
- 输入:`fixtures/replay/sim-205`(实时模拟账户从 `SimReset` 起录下的 1 h journal 指针,区间内 `quiet_advance = 0`)
- 期望:`replayer sim --check-fills` 同种子 → 每笔成交的价、量、`ex_ts_ns` 与 journal 逐笔相同,退出 0
- 实现:`apps/replayer/tests/replay.rs::sim_205_sim_replay_matches_live`

### SIM-206 与正式实盘并行校准:日报给出各指标差异,未达标时只作方向性参考

- 规则:[与正式实盘并行校准](README.md#与正式实盘并行校准)
- 级别:host
- 里程碑:M8
- 环境:升级主机,`prod` 实盘账户与 `sim` 实例中同参数、同 venue 的 Sim 账户(watchtower `calib_pairs`)
- 授权:无
- 步骤:
  1. 并行运行 ≥ 7 天,读每日校准表
- 通过标准:日报每天含 IOC 成交率、相对到达价滑点(中位、p90)、下单 → 回报延迟分位、两腿间隔 p50 比值、资金费相对误差五项的实盘值、模拟值与差异;差异超出目标的项列出对应的回写参数(`take_avail_bp`、`consume_decay_ms`、`[sim.latency_models]` 或结算规则)
- 证据:watchtower 校准日报,文件名含 `SIM-206`
- 实现:`tests/host/SIM-206.sh`

### SIM-210 on_record(L2Batch) 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`sim/on_l2`
- 实现:`crates/hft-sim/benches/sim.rs::sim_210_on_l2`

### SIM-211 Place 加 10 档吃单撮合基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`sim/place_match`
- 实现:`crates/hft-sim/benches/sim.rs::sim_211_place_match`

### SIM-212 SimGwTask::poll_once 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`sim/poll_once`
- 实现:`crates/hft-sim/benches/sim.rs::sim_212_poll_once`

### SIM-213 replayer sim 一天吞吐基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`sim/replay_day`
- 实现:`crates/hft-sim/benches/sim.rs::sim_213_replay_day`

### SIM-214 SimFault{RejectNew} 使该 (account, venue) 的新单以 Injected 拒绝直到 Off,撤单与其他 venue 不受影响

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)、[拒单规则](README.md#拒单规则)
- 级别:table
- 里程碑:M4
- 前置:`rd_sim()`、`setup_sim()`;`SimGwTask` 用测试环与内存 journal;已消费的最后一条记录 `@1 000 ms`;`out-sim` 先有 `GwCmd::sim_fault(A9, S1, RejectNew, 0)`,其后是所列指令(各自的 `OrderDecision` 已在 journal);`Bbo(BTC/S1, 59 999.99×2, 60 000.00×2)`
- 实现:`crates/hft-sim/tests/task.rs::sim_214_fault_reject`

| 行 | 之后的指令 | 期望 |
|---|---|---|
| SIM-214.1 | 两条 `Place(BTC/S1, Buy, 0.01, 60 000.00, Ioc)` | 两条 `OrderEvent{Reject, reject_code = 0x1000_000C}`(`SIM_CODE_BASE + 12`);余额不变 |
| SIM-214.2 | `GwCmd::sim_fault(A9, S1, Off, 0)`,再一条同样的 `Place` | 照常撮合:`Ack` + `Fill` |
| SIM-214.3 | 第 1 条为 `Cancel`(目标为 S1 上已有的 GTC 挂单),第 2 条为 `Place` | `CancelAck`;`Place` 为 `Reject`(撤单不受影响) |
| SIM-214.4 | 账户 `A8` 的 `Place(BTC/S1, …)` | 照常撮合(只对 `A9`) |
| SIM-214.5 | `A9` 在 `P2` 上的 `Place` | 照常撮合(只对 `S1`) |

### SIM-215 SimFault{Disconnect} 期间该 venue 的指令回 NoConn,Off 时推 Up 并交付积压回报

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:unit
- 里程碑:M4
- 前置:同 SIM-214 的环境(初始无故障);`A9` 在 `S1` 有一条已撮合、`due_tsc = 1 004 ms` 未交付的 `Fill`;`out-sim`:`GwCmd::sim_fault(A9, S1, Disconnect, 0)`(已消费最后一条记录 `@1 000 ms`),随后 `@2 000 ms` 一条 `S1` 上的 `Place` 与一条 `P2` 上的 `Place`,`@6 000 ms` 一条 `GwCmd::sim_fault(A9, S1, Off, 0)`
- 步骤:
  1. `poll_once`,事件时间推进到 2 000 ms
  2. 事件时间推进到 6 000 ms
- 期望:
  - 步骤 1:`rep-sim` 依次有 `ConnState{S1, Down, role Order}`、`ConnState{S1, Down, role UserStream}`,没有 `P2` 的 `ConnState`;`S1` 的 `Place` 回 `SendReport{status = NoConn}`,无 `Ack`;`P2` 的 `Place` 照常撮合;已撮合的 `Fill` 未交付
  - 步骤 2:6 000 ms 时推 `S1` 的 2 条 `ConnState{Up}`,随后交付积压的 `Fill`
- 实现:`crates/hft-sim/tests/task.rs::sim_215_fault_disconnect`

### SIM-216 SimFault{DelayReports} 只在故障期间给该 venue 的回报额外延迟

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:table
- 里程碑:M4
- 前置:同 SIM-214 的环境(初始无故障),`latency_model` 回报延迟固定 2 ms;`GwCmd::sim_fault(A9, S1, DelayReports, 3 000)` 于 `@1 000 ms` 生效,`GwCmd::sim_fault(A9, S1, Off, 0)` 于 `@11 000 ms` 生效;每行一条 `A9` 在 `S1` 上的 `Place`
- 实现:`crates/hft-sim/tests/task.rs::sim_216_fault_delay_reports`

| 行 | `Place` 的 `decided_tsc` | 期望 `Ack` 的 `due_tsc` |
|---|---|---|
| SIM-216.1 | 2 000 ms | 5 002 ms(2 000 + 2 + 3 000) |
| SIM-216.2 | 10 999 ms | 14 001 ms |
| SIM-216.3 | 11 001 ms(`Off` 之后) | 11 003 ms |

### SIM-217 越界参数、非 Sim 账户与不属于该账户的 venue 被忽略并计数;新故障替换旧故障

- 规则:[sim-gw 任务(`task` feature)](README.md#sim-gw-任务task-feature)
- 级别:table
- 里程碑:M4
- 前置:同 SIM-214 的环境(初始无故障);每行 `out-sim` 一条或两条所列 `GwCmd::sim_fault`,随后一条 `A9` 在 `S1` 上的 `Place`
- 实现:`crates/hft-sim/tests/task.rs::sim_217_fault_ignored`

| 行 | 指令 | 期望 |
|---|---|---|
| SIM-217.1 | `(A9, S1, RejectNew, delay_ms = 5)` | `sim_fault_ignored = 1`;`Place` 照常撮合 |
| SIM-217.2 | `(A9, S1, DelayReports, delay_ms = 0)` | 同 .1 |
| SIM-217.3 | `(A9, S1, Disconnect, delay_ms = 1)` | 同 .1 |
| SIM-217.4 | `(A9, S1, DelayReports, delay_ms = 60 001)` | 同 .1 |
| SIM-217.5 | `RejectNew`,账户为 refdata 中的 Live 账户 | 同 .1 |
| SIM-217.6 | `(A9, S1, RejectNew, 0)` 生效后再来 `(A9, S1, DelayReports, 3 000)` | `sim_fault_ignored = 0`;`Place` 不再被拒,`Ack` 的 `due_tsc` 另加 3 000 ms |
| SIM-217.7 | `(A9, S4, RejectNew, 0)`(`S4` 不在 `A9` 内) | 同 .1 |

### SIM-218 sim-gw 重启后受控故障清除

- 规则:[重启恢复](README.md#重启恢复)
- 级别:unit
- 里程碑:M4
- 前置:同 SIM-214;`GwCmd::sim_fault(A9, S1, RejectNew, 0)` 生效、已拒 1 条
- 步骤:
  1. 按"重启恢复"重建 `SimExchange`(`seed_account_from`)
  2. 一条 `A9` 在 `S1` 上的 `Place`
- 期望:
  - 照常撮合(无 `Injected` 拒单);`sim_fault_ignored = 0`
- 实现:`crates/hft-sim/tests/restore.rs::sim_218_fault_cleared_on_restart`

### SIM-219 价格限制带未配置时跳过该项,配置后越带以 PriceLimit 拒绝

- 规则:[拒单规则](README.md#拒单规则)
- 级别:table
- 里程碑:M4
- 前置:`rd_sim()`,`BTC/P2` 的 `InstMeta` 价格限制带如列;mark 60 000.0;`Place(BTC/P2, Buy, 0.001, px, Gtc)`
- 实现:`crates/hft-sim/tests/reject.rs::sim_219_price_limit_band`

| 行 | 价格限制带 | `px` | 期望 |
|---|---|---|---|
| SIM-219.1 | 未配置 | 90 000.0 | `Ack`(跳过第 5 项) |
| SIM-219.2 | 带上沿 63 000.0、下沿 57 000.0 | 63 000.0 | `Ack`(恰在带边) |
| SIM-219.3 | 同 .2 | 63 000.1 | `Reject`,`reject_code = SIM_CODE_BASE + 5` |

### SIM-222 回报乱序不改变独立资金核算

- 规则:[10. 不变量](README.md#10-不变量)
- 级别:table
- 里程碑:M2
- 前置:SIM-124 的逐笔模型,4 账户、4 inst、Empirical 延迟,种子 11815488002174123349
- 输入:账户下标 2、inst 下标 2,连续发 GTC 卖 1 lot/价偏移 0、卖 4 lot/偏移 −1、买 1 lot/偏移 4,随后 Amend 最近一单/偏移 0;17 ms 后 TopN 偏移 2、单档量 5 lot
- 共同期望:确实出现成交回报产生顺序与交付顺序不一致;回报时序检查始终使用原始交付顺序
- 实现:`crates/hft-sim/tests/prop_invariants.rs::sim_222_settlement_order_is_not_reply_order`

| 行 | 模型输入 | 期望 |
|---|---|---|
| SIM-222.1 | 按 `gen_seq` 核算 | 与模拟真值逐单位相等 |
| SIM-222.2 | 错误地按交付顺序核算 | 固定原种子及其后 31 个种子中至少一次实际乱序导致独立核算与模拟真值不同;按 gen_seq 核算的全部运行仍相等 |
| SIM-222.3 | 按 `gen_seq`,额外记入 1 单位手续费 | 资金守恒检查失败 |

### SIM-223 永续回报提供累计结算事实,费用与查询不改变它

- 规则:[手续费与扣费资产](README.md#5-手续费与扣费资产)、[hft-event 执行事件](../hft-event/README.md#执行0x02xx)
- 级别:table
- 里程碑:M2
- 前置:`rd_sim`、`setup_sim`,P2 钱包 18000 USDT,开仓 0.2 BTC @60000
- 输入:两档各 0.1 BTC 平仓,每行分别即时/晚到手续费;结束后同订单查两次
- 共同期望:开仓为覆盖量 0.2、现金零;每笔覆盖量等于累计成交,钱包 = 初始 + 结算 − 独立费用和;两次查询重复相同累计事实
- 实现:`crates/hft-sim/tests/settlement.rs::sim_223_order_cash_is_independent_of_fees_and_queries`

| 行 | 输入 | 累计现金 USDT |
|---|---|---|
| SIM-223.1 | 多仓,卖 62000/61000 | 200、300 |
| SIM-223.2 | 多仓,卖 59000/58000 | −100、−300 |
| SIM-223.3 | 空仓,买 58000/59000 | 200、300 |
| SIM-223.4 | 空仓,买 61000/62000 | −100、−300 |
| SIM-223.5 | S1 现货成交 | 两现金字段均为零,不声明永续结算 |

### SIM-224 恢复订单保留结算覆盖范围,无效事实原子拒绝

- 规则:[重启恢复](README.md#重启恢复)
- 级别:table
- 里程碑:M2
- 前置:P2 余额 18100 USDT,多仓 0.1 BTC @60000;恢复卖单共 0.2、已成交 0.1 @61000,余量挂单
- 输入:先查询恢复单,再以 61001 的 Trade 穿过挂单价 61000
- 共同期望:查询保留种子原值;后续只增加本笔 0.1 BTC 的现金 100 USDT;非法种子返回 SettlementFacts,完整状态哈希不变
- 实现:`crates/hft-sim/tests/settlement.rs::sim_224_recovered_cash_coverage_is_not_inferred_from_wallet`

| 行 | 原结算事实或错误 | 期望 |
|---|---|---|
| SIM-224.1 | 覆盖 0.1、金额 100 | 最终覆盖 0.2、金额 200 |
| SIM-224.2 | 两字段均为 0 | 最终只覆盖 0.1、金额 100,不从余额补造旧事实 |
| SIM-224.3 | 覆盖量为负 | 原子拒绝 |
| SIM-224.4 | 覆盖量超过已成交量 | 原子拒绝 |
| SIM-224.5 | 零覆盖量带非零金额 | 原子拒绝 |
| SIM-224.6 | 现货订单带永续结算事实 | 原子拒绝 |

### SIM-225 强平回报将本笔现金与费用分开

- 规则:[永续全仓保证金与强平](README.md#7-永续全仓保证金与强平)
- 级别:unit
- 里程碑:M2
- 前置:P2 余额 18000 USDT,空仓 1 BTC @60000
- 输入:mark = 77611.9403,触发与 SIM-072.2 相同的强平
- 步骤:恢复余额与空仓,输入 mark,推进到 10 ms 后收集 ExternalFill 与钱包
- 期望:恰好一笔 ExternalFill,覆盖量 1 BTC、实际现金 −18000.0001 USDT、费用 390.000001 USDT;钱包逐单位等于初始加现金减费用
- 实现:`crates/hft-sim/tests/settlement.rs::sim_225_liquidation_reports_exact_cash_separately_from_fee`

### SIM-226 外来成交直接返回撮合现金,无效输入不结算

- 规则:[纯核心 API](README.md#纯核心-api)
- 级别:table
- 里程碑:M2
- 前置:`rd_sim`、`setup_sim`,P2 钱包 18000 USDT、零仓;回报堆已清空
- 输入:依次卖 0.001@60018.9、卖 0.001@60024.4、买 0.002@60027.5、卖 0.001@60027.4、买 0.001@60027.5;再镜像所有方向
- 共同期望:逐笔返回值和钱包分别按原始成交的独立常量核对,仓位按带方向数量累计,不产生回报;无效输入返回 None 且完整状态哈希不变
- 实现:`crates/hft-sim/tests/settlement.rs::sim_226_external_cash_is_returned_at_settlement`

| 行 | 输入 | 期望 |
|---|---|---|
| SIM-226.1 | 原方向五笔 | 现金原始单位 0、0、−11700、0、−100,最终零仓 |
| SIM-226.2 | 多空镜像五笔 | 现金 0、0、+11700、0、+100,最终零仓 |
| SIM-226.3 | 未装配账户 A8 | 原子拒绝 |
| SIM-226.4 | 未知品种 999 | 原子拒绝 |
| SIM-226.5 | 数量零 | 原子拒绝 |
| SIM-226.6 | 数量 −1 | 原子拒绝 |
| SIM-226.7 | 价格零 | 原子拒绝 |
| SIM-226.8 | 价格 −1 | 原子拒绝 |
| SIM-226.9 | A8 绑定 P2,A9 仅绑定 S1;A9 在 P2 成交 | 原子拒绝,不能因其他账户追踪该品种而结算 |

## 验收

- 里程碑 M2(共用撮合器):里程碑为 M2 的用例全部有同编号的测试函数且通过;M4(sim 实例、sim-gw、重启恢复、压力用例):里程碑为 M4 的用例同样全部通过(追溯规则见 [docs/tests §6](../../docs/tests/README.md#6-与代码的对应))。
- 属性测试与 DST:PR 与每夜例数、种子见各用例;SIM-167–SIM-169 计入 [testing §8](../../docs/testing.md#8-上线门槛) 门槛 ④ 的累计。
- 基准:SIM-210–SIM-213 在参考机上满足 [benchmarks](../../benchmarks/README.md) 登记的上限,回归阈值 10%。
- 上线门槛 ⑦:与交易所一致(SIM-202、SIM-203)、资金守恒(SIM-204)与实例池(SIM-201)的证据;门槛压力用例以每个上线策略的最终参数运行,用例在 `strategies/registry/tests/`,变换、故障与判定由 SIM-180–SIM-196 验证。
- 校准(M8):SIM-206 的日报;未达目标差异时模拟结论只作方向性参考(规则见[与正式实盘并行校准](README.md#与正式实盘并行校准))。
- `queue-hftbacktest` feature(挂单排队模型)与链上腿模拟的用例在各自的触发条件满足、启用时补写。

### SIM-227 交割:撤单并把库存按兑付价卖出

- 规则:[8. 现货余额与划转](README.md#8-现货余额与划转)
- 级别:unit
- 里程碑:M3
- 前置:`Sim::std()`;账户 9 以 IOC 买入 `BTC_S1` 0.1(卖一 60000),再挂卖单 0.05 @ 70000(GTC)
- 步骤:
  1. `GwCmd::sim_settle(A9, S1, BTC_S1, 65000, 1792425600)`,推进到 300 ms
  2. 同一命令再发一次,推进到 500 ms
- 期望:
  - 步骤 1:挂单 `CancelAck`;恰好一条 `ExternalFill{Delivery, Sell, BTC_S1, qty = 买入后的 BTC 余额, px 65000, fee 0, exch_order_id 1792425600, fill_seq = qty}`;BTC 余额 0;USDT 余额增加 qty × 65000
  - 步骤 2:没有 `ExternalFill`
- 实现:`crates/hft-sim/tests/spot.rs::sim_227_settle_delivers_inventory`

### SIM-228 `maker_fill = Queue`:挂价处的排队位置

- 规则:[4. 挂单(预留给 MakerFirst)](README.md#4-挂单预留给-makerfirst)
- 级别:table
- 里程碑:M3
- 前置:`setup_sim()` 且 `maker_fill = Queue`;`BTC_S1` 买一 59000 显示 1 BTC;账户 9 挂买 0.1 @ 59000(GTC)
- 实现:`crates/hft-sim/tests/maker.rs::sim_228_queue_position`

| 行 | 行情序列 | 期望 |
|---|---|---|
| SIM-228.1 | 买一 59000 显示 0.6,随后卖方吃单成交 0.4 @ 59000;显示 0.1,随后成交 0.5;再成交 0.15 | 前两次不成交;第三次成交 0.05 @ 59000 |
| SIM-228.2 | 显示 0.5(无成交);600 ms 后再次显示 0.5;随后成交 0.55 | 成交 0.05(前方数量截到 0.5) |
| SIM-228.3 | 买方吃单成交 5 @ 59000 | 不成交 |
| SIM-228.4 | `TradeThrough`(`Sim::std()`),卖方吃单成交 5 @ 59000 | 不成交 |

### SIM-229 二元结果代币的互补成交

- 规则:[4. 挂单(预留给 MakerFirst)](README.md#4-挂单预留给-makerfirst)
- 级别:table
- 里程碑:M3
- 前置:`setup_sim()` 且 `maker_fill = Queue`,`A9` 的 `initial_balances` = B5 USDT 1 000;`rd_binary()`;`UP_B5` 买一 0.45 显示 50、卖一 0.47;`A9` 挂买 100 `UP_B5` @ 0.45(GTC)
- 步骤:依次执行下表各行(每行从前置重新开始)
- 实现:`crates/hft-sim/tests/maker.rs::sim_229_binary_complement_prints`

| 行 | 行情序列 | 期望 |
|---|---|---|
| SIM-229.1 | `DN_B5` 买方吃单 30 @ 0.55;再 40 @ 0.55 | 第一次不成交;第二次成交 20 @ 0.45 |
| SIM-229.2 | `DN_B5` 卖方吃单 500 @ 0.55 | 不成交(镜像后吃单方与挂单同侧) |
| SIM-229.3 | `DN_B5` 买方吃单 10 @ 0.60 | 穿价:成交 10 @ 0.45(镜像价 0.40 < 0.45) |
| SIM-229.4 | `UP_B5` 卖方吃单 60 @ 0.45 | 同代币规则不变:成交 10 |

### SIM-230 journal 衔接:记录分派、Written 与回报展开

- 规则:[journal 衔接](README.md#journal-衔接glue-模块)
- 级别:table
- 里程碑:M3
- 前置:`Sim::std()`
- 实现:`crates/hft-sim/tests/glue.rs::sim_230_journal_glue`

| 行 | 操作 | 期望 |
|---|---|---|
| SIM-230.1 | `feed_record` 送 BTC_S1 的 `Bbo` 记录体(买一 59 000、卖一 60 000),再送去掉末字段的同一记录体;随后买 0.1 @ 60 000 IOC | 两次都是 `Fed::Market`;成交价 60 000 |
| SIM-230.2 | `origin = KEYFRAME` 的 `L2Snapshot`;`Note` 记录 | `Fed::Skipped`;`Fed::Clock` |
| SIM-230.3 | `written` 对 `Place{slot 4, gen 3}`、`Cancel{target 4}`、`SimReset` | `Written`、`slot 4`、`gen 3`、`sent_tsc` = now;撤单的 `target_slot`/`target_gen` = 4/3;`SimReset` → `None` |
| SIM-230.4 | `reply_parts` 对 A9 的账户级 `ConnState{venue 0}`;对一条 `OrderEvent`,epoch 5 | 依次 S1、P2 各一条 `ConnState`;`OrderEvent.epoch` = 5 |

### SIM-231 `reseed_account` 使两个交易所的后续延迟相同

- 规则:[纯核心 API](README.md#纯核心-api)、[9. 确定性](README.md#9-确定性)
- 级别:table
- 里程碑:M3
- 前置:`setup_sim()` 的延迟模型换成 `Empirical{entry = resp = [(0, 1 000), (10 000, 90 000)]}`;两个交易所 a、b;a 先下 3 张、b 先下 1 张不成交的 GTC 买单
- 实现:`crates/hft-sim/tests/glue.rs::sim_231_reseed_restarts_the_account_stream`

| 行 | 操作 | 期望 |
|---|---|---|
| SIM-231.1 | 不重播种,各下一张 | 两者的 Ack 延迟序列不全相同 |
| SIM-231.2 | 两者都 `reseed_account(A9, 777)`,再各下 4 张(同一决策时刻) | 每张 Ack 延迟相等 |
| SIM-231.3 | `reseed_account(AccountId(77), 1)` | 无动作、不出错 |

### SIM-232 `QueueCap`:排队截断、穿价与越价成交全部剩余

- 规则:[场所行为选项](README.md#场所行为选项venuemodel)
- 级别:table
- 里程碑:M3
- 前置:`rd_binary()`;`A9` 在 B5 有 1 000 USDT;Up 簿买 0.45 × 50、卖 0.47 × 50;`A9` 挂买 100 Up @ 0.45(GTC),每行从前置重新开始
- 实现:`crates/hft-sim/tests/venue_model.rs::sim_232_queue_cap`

| 行 | 行情 | 期望 |
|---|---|---|
| SIM-232.1 | 买 0.45 档降到 20;卖方成交 30 @ 0.45 | 成交 10 |
| SIM-232.2 | 档位降到 20 再升到 80;卖方成交 25 @ 0.45 | 成交 5(`ahead` 不随档位回升) |
| SIM-232.3 | 卖方成交 1 @ 0.44 | 成交 100 |
| SIM-232.4 | 卖一降到 0.44 × 1 | 成交 100 |
| SIM-232.5 | `Queue`,卖方成交 1 @ 0.44 | 成交 1(原规则不变) |

### SIM-233 撤单竞速与墓碑

- 规则:[场所行为选项](README.md#场所行为选项venuemodel)
- 级别:table
- 里程碑:M3
- 前置:同 SIM-232 的簿;`tombstone`,`cancel_us = (0.5 ms, 1 ms)`;下单延迟 2 ms
- 实现:`crates/hft-sim/tests/venue_model.rs::sim_233_cancel_race_and_tombstone`

| 行 | 操作 | 期望 |
|---|---|---|
| SIM-233.1 | 100 ms 下买 10 @ 0.45,100.5 ms 撤 | 只有 `CancelAck`;`tombstone_refusals` = 1;没有挂单 |
| SIM-233.2 | 100 ms 下单,103 ms 撤 | `Ack`、`CancelAck`;后者 due = 103.5 ms + 1 ms |
| SIM-233.3 | 挂单后 120 ms 撤,120.1 ms 卖方成交 1 @ 0.44 | 撤单在途时成交 10 |
| SIM-233.4 | 无 `tombstone`,同 .1 | `CancelReject`、`Ack`(撤单找不到,订单照挂) |

### SIM-234 吃单撮合延迟

- 规则:[场所行为选项](README.md#场所行为选项venuemodel)
- 级别:table
- 里程碑:M3
- 前置:同 SIM-232 的簿;`taker = {base 195 ms, slow 1.25 s, resp 15 ms}`
- 实现:`crates/hft-sim/tests/venue_model.rs::sim_234_taker_delay`

| 行 | 操作 | 期望 |
|---|---|---|
| SIM-234.1 | 100 ms IOC 买 30 @ 0.50;250 ms 卖档变为 0.49 × 10 | 成交 10 @ 0.49,其余 `Expired`;首条回报 due = 102 + 195 + 15 ms |
| SIM-234.2 | 不设 `taker`,同上 | 到达时成交 30 @ 0.47 |
| SIM-234.3 | `slow_ppm` = 10⁶ | 首条回报 due = 102 ms + 1.25 s + 15 ms |
| SIM-234.4 | PostOnly 买 10 @ 0.44 | 到达即 `Ack`,不延迟 |

### SIM-235 二元代币吃单费

- 规则:[场所行为选项](README.md#场所行为选项venuemodel)
- 级别:table
- 里程碑:M3
- 前置:同 SIM-232 的簿;`fee = BinaryTaker{k_e9 = 7 × 10⁷}`
- 实现:`crates/hft-sim/tests/venue_model.rs::sim_235_binary_taker_fee`

| 行 | 操作 | 期望 |
|---|---|---|
| SIM-235.1 | IOC 买 10 @ 0.50(成交 0.47) | 费 0.17437 USDT |
| SIM-235.2 | 挂单被穿价成交 | 费 0 |

### SIM-236 GTD 到期

- 规则:[场所行为选项](README.md#场所行为选项venuemodel)
- 级别:table
- 里程碑:M3
- 前置:同 SIM-232 的挂单;`gtd_ms = 300 000`
- 实现:`crates/hft-sim/tests/venue_model.rs::sim_236_gtd_expiry`

| 行 | 操作 | 期望 |
|---|---|---|
| SIM-236.1 | 推进到到达 + 300 s 前 1 ns;再到 + 10 ms | 前者只有 `Ack`;后者 `Ack`、`Expired`;`gtd_expired` = 1 |
| SIM-236.2 | 到期前被成交 | 不出 `Expired`;`gtd_expired` = 0 |

### SIM-237 条件延迟池

- 规则:[场所行为选项](README.md#场所行为选项venuemodel)
- 级别:table
- 里程碑:M3
- 前置:同 SIM-232 的簿;`split_us` 5 000、`min_n` 20;Place 池 (1, 0, 0) = 20 × 10 ms、(2, *, *) = 20 × 30 ms、(0, 0, 0) = 19 × 99 ms;账户模型 2 ms / 2 ms
- 实现:`crates/hft-sim/tests/venue_model.rs::sim_237_conditional_pools`

| 行 | 条件 | 期望(100 ms 下 PostOnly 买单的 Ack due) |
|---|---|---|
| SIM-237.1 | (1, 0, 0) | 105 ms + 2 ms |
| SIM-237.2 | (2, 1, 1) | 125 ms + 2 ms(退到 (leg, load)) |
| SIM-237.3 | (0, 0, 0) | 102 ms + 2 ms(样本不足且无退路:账户模型) |

### SIM-238 自有挂单叠加层

- 规则:[场所行为选项](README.md#场所行为选项venuemodel)
- 级别:table
- 里程碑:M3
- 前置:同 SIM-232 的簿,`QueueCap`
- 实现:`crates/hft-sim/tests/venue_model.rs::sim_238_own_order_overlay`

| 行 | 操作 | 期望 |
|---|---|---|
| SIM-238.1 | `hide_own(Up, 买, 0.45, 30)` 后挂买 100 @ 0.45;卖方成交 30 @ 0.45 | 成交 10 |
| SIM-238.2 | `hide_own(Up, 卖, 0.47, 40)`;IOC 买 30 @ 0.47 | 成交 10 |
| SIM-238.3 | 再 `hide_own(…, −40)` | 成交 30 |

### SIM-239 默认的场所行为选项与不设置相同

- 规则:[场所行为选项](README.md#场所行为选项venuemodel)
- 级别:table
- 里程碑:M3
- 前置:同 SIM-232 的簿,`Queue`
- 实现:`crates/hft-sim/tests/venue_model.rs::sim_239_default_model_is_the_plain_exchange`

| 行 | 操作 | 期望 |
|---|---|---|
| SIM-239.1 | 同一脚本(挂单、撤单、IOC、穿价成交)分别不调用与调用 `set_venue_model(VenueModel::default())` | 回报序列与 `state_hash` 相同 |

### SIM-240 条件跟踪的三个分量

- 规则:[条件跟踪与校准表](README.md#条件跟踪与校准表)
- 级别:table
- 里程碑:M3
- 前置:`CondCfg{load_thr_ns = [1 ms, 5 ms], window_s = 300, phase_s = 5}`;跟踪 venue 1,venue 2 承载参考报价;TSC 1 GHz;每秒 10 个延迟样本
- 实现:`crates/hft-sim/tests/cond.rs::sim_240_condition_buckets`

| 行 | 输入 | 期望 |
|---|---|---|
| SIM-240.1 | 无样本;分钟 0 延迟 10 ms;分钟 1 延迟 0.5 ms;分钟 2 延迟 3 ms;venue 2 的 50 ms 样本 | `load` 依次为 1、1(分钟 0 未结束)、2、0;未跟踪 venue 的样本不改变 `load` |
| SIM-240.2 | 参考 inst 9 的报价(bid + ask 2 000 000)在 0.5 s 内变 0.2 bp、0.8 bp、2 bp;另一 inst 8 的报价;1.85 s 时只剩最后一条在 1 s 内 | `mv` 依次 0、1、2;inst 8 不改变结果;最后为 0 |
| SIM-240.3 | `utc_ns` 在窗口第 4 s、第 5 s | `phase` 0、1 |
