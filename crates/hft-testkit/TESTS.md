---
component: hft-testkit
prefix: TK
spec: README.md
---

# hft-testkit 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本文件验证测试基建自身:框架给出的"通过"必须可信,"失败"必须可复现。被测业务组件的用例在各自的 `TESTS.md` 与 `docs/tests/scenarios.md`。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 1. 金样 | TK-001–TK-009 |
| 2. proptest 生成器 | TK-010–TK-021 |
| 3. 假时钟 | TK-022–TK-024 |
| 4. 策略测试桩 | TK-025–TK-035 |
| 5. 解耦门槛夹具 | TK-036–TK-042、TK-094–TK-096、TK-117–TK-119、TK-141、TOOL-024、TOOL-027–TOOL-029 |
| 6. DST 框架 | TK-043–TK-070、TK-100–TK-116、TK-120–TK-125、TK-147–TK-154 |
| 7. 状态哈希对比 | TK-071–TK-074 |
| 8. hft-dst 运行器 | TK-075–TK-088、TK-143、TK-146 |
| 9. 分配计数与基准驱动 | TK-089–TK-091 |
| 10. SeqRig | PY-106–PY-109、PY-111、PY-112 |
| 实现说明 | 不适用:见子节 |
| DST 的世界模型 | TK-045–TK-048、TK-055–TK-064、TK-097–TK-099、TK-126–TK-140、TK-142、TK-144–TK-145、TK-155–TK-159 |
| 金样运行器 | TK-001、TK-005 |
| 源文件 | TK-066 |
| 延迟与资源契约 | TK-092–TK-093 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | TK-036、TOOL-031 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

约定:"桩协议"指本 crate 测试里实现 `VenueProtocol` 的最小解码器(逐字节把 `.in` 文本 `BBO,<bid>,<ask>` 解成一条 `Bbo`);"植入缺陷替身"指以 feature `planted-<名>` 编译、在被测 crate 的调试钩子里注入错误行为的构建(只在本 crate 的测试中使用)。

### TK-001 golden_cases 按 fixtures 布局读出金样

- 规则:[1. 金样](README.md#1-金样)、[金样运行器](README.md#金样运行器)
- 级别:unit
- 里程碑:M3
- 前置:夹具 `crates/hft-testkit/tests/fixtures/golden/stub-venue/bbo/` 下 2 个用例目录,各含 `meta.toml`(venue `stub-venue`、通道 `bbo`、协议版本 `1`、捕获时间)、`*.in` 与期望事件文件
- 步骤:
  1. `golden_cases("stub-venue", "bbo")` 收集全部用例
- 期望:
  - 恰得到 2 个 `GoldenCase`,`venue`、`channel` 与 `meta` 字段取自各自的 `meta.toml`;`input` 的帧数与 `.in` 行数相同;`expected` 的事件数与期望文件条数相同
- 实现:`crates/hft-testkit/tests/golden.rs::tk_001_loads_cases_by_layout`

### TK-002 解码输出与期望逐事件相等时 run_decoder_case 返回 Ok

- 规则:[1. 金样](README.md#1-金样)
- 级别:unit
- 里程碑:M3
- 前置:同 TK-001 的第 1 个用例;桩协议
- 步骤:
  1. `run_decoder_case(&mut 桩协议, &case)`
- 期望:
  - 返回 `Ok(())`
- 实现:`crates/hft-testkit/tests/golden.rs::tk_002_matching_case_ok`

### TK-003 篡改期望的一个字段时报告该帧、该事件与该字段

- 规则:[1. 金样](README.md#1-金样)
- 级别:unit
- 里程碑:M3
- 前置:同 TK-002;把期望中第 2 帧第 1 个事件的 `bid_px` 由 raw 6512345 改为 6512346(该 inst `px_exp = −2`)
- 步骤:
  1. `run_decoder_case(&mut 桩协议, &case)`
- 期望:
  - 返回 `Err(GoldenDiff)`:帧序号 1(0 起)、事件序号 0、字段 `bid_px`、期望 raw 6512346(65123.46)、实际 raw 6512345(65123.45)
- 实现:`crates/hft-testkit/tests/golden.rs::tk_003_field_diff_reported`

### TK-004 事件条数不同时报告第一处多出或缺少的事件

- 规则:[1. 金样](README.md#1-金样)
- 级别:table
- 里程碑:M3
- 前置:同 TK-002,按行改期望文件
- 实现:`crates/hft-testkit/tests/golden.rs::tk_004_event_count_mismatch`

| 行 | 改动 | 期望 |
|---|---|---|
| TK-004.1 | 期望删去第 2 帧的事件 | `GoldenDiff`:帧 1 多出 1 条实际事件 |
| TK-004.2 | 期望在第 1 帧多加一条事件 | `GoldenDiff`:帧 0 缺少 1 条实际事件 |

### TK-005 本地接收字段不参与比较

- 规则:[1. 金样](README.md#1-金样)、[金样运行器](README.md#金样运行器)
- 级别:unit
- 里程碑:M3
- 前置:同 TK-002;期望文件中每个事件的 `rx_tsc` 与 `hw_rx_ns` 改为任意不同值
- 步骤:
  1. `run_decoder_case(&mut 桩协议, &case)`
- 期望:
  - 返回 `Ok(())`
- 实现:`crates/hft-testkit/tests/golden.rs::tk_005_local_fields_ignored`

### TK-006 没有用当前输出覆盖期望文件的代码路径

- 规则:[1. 金样](README.md#1-金样)
- 级别:unit
- 里程碑:M3
- 前置:TK-003 的失败用例;环境变量 `UPDATE_EXPECT=1`、`BLESS=1`、`INSTA_UPDATE=always` 都已设置
- 步骤:
  1. 记录期望文件的 SHA-256;运行 `run_decoder_case`
- 期望:
  - 返回 `Err(GoldenDiff)`;期望文件的 SHA-256 不变;`src/golden.rs` 中没有对 `fixtures/` 路径的写文件调用(源码扫描:无 `fs::write`、`File::create`、`OpenOptions::write`)
- 实现:`crates/hft-testkit/tests/golden.rs::tk_006_never_overwrites_expected`

### TK-007 golden_suite! 为每个金样用例生成一个测试

- 规则:[1. 金样](README.md#1-金样)
- 级别:unit
- 里程碑:M3
- 前置:同 TK-001(2 个用例);测试文件写一行 `golden_suite!(StubVenue, "stub-venue")`
- 步骤:
  1. `cargo test -- --list`
- 期望:
  - 列出恰 2 个由该宏生成的测试,名字含各自的用例 id
- 实现:`crates/hft-testkit/tests/golden.rs::tk_007_suite_macro_per_case`

### TK-008 core 六个协议的金样目录都能读出用例

- 规则:[1. 金样](README.md#1-金样)
- 级别:table
- 里程碑:M3
- 前置:仓库 `fixtures/`(金样由各 venue 维护,覆盖要求归 [fixtures](../../fixtures/README.md))
- 实现:`crates/hft-testkit/tests/golden.rs::tk_008_core_suites_present`

| 行 | venue | 期望 |
|---|---|---|
| TK-008.1 | `binance-usdm` | `golden_cases` 至少 1 个用例 |
| TK-008.2 | `binance-spot` | 同上 |
| TK-008.3 | `okx-swap` | 同上 |
| TK-008.4 | `okx-spot` | 同上 |
| TK-008.5 | `bitget-usdt-futures` | 同上 |
| TK-008.6 | `bitget-spot` | 同上 |

### TK-009 run_sim_case 把记录与指令脚本跑成回报序列并逐条比较

- 规则:[1. 金样](README.md#1-金样)
- 级别:table
- 里程碑:M2
- 前置:`fixtures/sim/` 夹具用例 `ioc-cross`:一段行情记录 + 指令脚本(一张 IOC 买单);期望回报序列(Ack、Fill、终态)
- 实现:待实现

| 行 | 期望文件 | 期望 |
|---|---|---|
| TK-009.1 | 原样 | `Ok(())` |
| TK-009.2 | Fill 的 `qty` 改大 1 lot | `GoldenDiff` 指向该回报的 `qty` |

### TK-010 market_stream 只产出合法的 core 行情序列

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:prop
- 里程碑:M1
- 性质:每条 `Bbo` 的 `bid_px < ask_px`、两侧数量 > 0;每条 `L2Snapshot` 的 `origin = TopN`,档位价格严格单调;同一连接上事件的 `rx_tsc` 不减;`Funding` 的结算周期变化时其后的 `next_funding` 与新周期一致
- 生成:1–4 个 inst,`n` ∈ 1..=5000,任意 `GapPolicy` 与 `QuietPolicy`
- 参考:逐条断言上述条件
- 例数:PR 256
- 实现:`crates/hft-testkit/tests/gen.rs::tk_010_market_stream_valid`

### TK-011 QuietPolicy 让 BBO 长时间不变而心跳照常,或让连接完全静默

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:table
- 里程碑:M1
- 前置:`market_stream_with`:`insts = [X]`,`bbo_ping_ms = 1000`,`GapPolicy::Contiguous`,n = 3000,步间隔 5–20 ms,固定种子;窗口 [1 000, 12 000) ms
- 实现:`crates/hft-testkit/tests/gen.rs::tk_011_quiet_policy`

| 行 | 策略 | 期望 |
|---|---|---|
| TK-011.1 | `Still{X}`:盘口不变、连接健康 | 窗口内没有 X 的 `Bbo`;BBO 连接的 `ConnState{Alive}` 相邻两条间隔 1000 ms,窗口内 ≥ 10 条;窗口后 X 再有 `Bbo` |
| TK-011.2 | `Silent`:连接停止回帧 | 窗口内 BBO 连接没有任何事件;该连接在窗口后的前 5 条依次为 `Degraded{BBO_SILENT}`、`Down{inst = 0}`、`Up{inst = 0}`、`Up{X}`、`Resync{X, BBO_REFRESH}`,`rx_tsc` 均为 12 000 ms;之后才有 `Bbo` |

### TK-012 order_lifecycle 产出某 venue 语义下合法的回报序列

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:prop
- 里程碑:M2
- 性质:每个序列以终态(`Fill` 全部成交、`CancelAck`、`Expired`)结束一次;累计成交量 ≤ 订单量;在 10 000 个样本中 fill-before-ack、cancel-reject-because-filled、amend 竞态、只带费用的回报各至少出现一次
- 生成:venue 规则取三个 core venue 的桩规则;订单量 1–1000 lot
- 参考:按 venue 规则的朴素状态机重放
- 例数:PR 10 000
- 实现:`crates/hft-testkit/tests/gen.rs::tk_012_order_lifecycle_valid`

### TK-013 perturb 只在窗口内乱序,并按概率重复与丢弃

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:prop
- 里程碑:M2
- 性质:输出中每个元素相对原位置的位移 ≤ `reorder_window`;输出去掉重复后是输入的子序列的一个窗口内置换;`dup_p = drop_p = 0`、`reorder_window = 0`、`delay = 0` 时输出与输入逐项相同
- 生成:输入长度 0–500;`reorder_window` 0–16;`dup_p`、`drop_p` ∈ [0, 0.5]
- 参考:逐项断言上述条件
- 例数:PR 1 000
- 实现:`crates/hft-testkit/tests/gen.rs::tk_013_perturb_bounds`

### TK-014 control_cmds 只产出允许集合内的命令且覆盖登记的变体

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:prop
- 里程碑:M2
- 性质:每条命令的变体 ∈ `allowed`;`allowed` 为全部时,10 000 个样本中每个列出的变体至少一次:按账户 `Kill{hard}`、Viewer 主体的紧急停止、闸门未放行与 `DRAINING` 期间的 `Kill{flatten}`、两腿同所的 `SetParams`、静止的非 Broken group 上的 `WriteOffResidual`、`SmokeOrder`、`InjectWarmup`、`SimReset`、`SimTransfer`
- 生成:`allowed` 为全部变体的任意非空子集
- 参考:变体判别与计数
- 例数:PR 10 000
- 实现:`crates/hft-testkit/tests/gen.rs::tk_014_control_cmds_allowed`

### TK-015 fills_with_fee_asset 按 fee_asset_rule 产出成交与晚到费用

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:table
- 里程碑:M2
- 前置:成交毛量 10 lot,费率 0.1%
- 实现:`crates/hft-testkit/tests/gen.rs::tk_015_fee_asset_rules`

| 行 | `fee_asset_rule` | 期望 |
|---|---|---|
| TK-015.1 | 基础币扣费 | 费用资产 = 基础资产,费用 = 毛量 × 0.1% |
| TK-015.2 | 报价币扣费 | 费用资产 = 报价资产 |
| TK-015.3 | 晚到费用变体 | 先有一条无费用的成交,之后一条只带费用的回报,两者的费用之和 = 毛量 × 价格 × 0.1%(报价币) |

### TK-016 margin_path 穿越各档阈值与滞回两侧并带不一致与过期段

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:prop
- 里程碑:M2
- 性质:在 1 000 条路径中:`liq_dist` 穿越 `liq_dist_block_open`、`liq_dist_reduce`、`liq_dist_force` 各自的阈值(含 `liq_dist_hyst` 两侧)至少一次;交易所 `liq_px` 与 ledger 估算不一致的段、维持/保证金比率 ≥ 0.8 而 `liq_dist` 宽松的段、数据过期段各至少一次
- 生成:前置的阈值取 `liq_dist_block_open = 0.30`、`liq_dist_reduce = 0.20`、`liq_dist_force = 0.10`、`liq_dist_hyst = 0.02`(Rate);路径长度 100–2 000 步
- 参考:逐路径统计
- 例数:PR 1 000
- 实现:`crates/hft-testkit/tests/gen.rs::tk_016_margin_path_coverage`

### TK-017 one_sided_move 按形状在 over_ms 内单边移动且只作用于给定 inst

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:table
- 里程碑:M2
- 前置:inst X 起始 mark 与 BBO 中间价 100.00(tick 0.01);另一 inst Y 同时存在
- 实现:`crates/hft-testkit/tests/gen.rs::tk_017_one_sided_move`

| 行 | 参数 | 期望 |
|---|---|---|
| TK-017.1 | `pct = 30%`、`over_ms = 3_600_000`、`Linear` | t = 3 600 000 ms 时 X 的 mark = 130.00;t = 1 800 000 ms 时 = 115.00;Y 的 mark 全程不变 |
| TK-017.2 | 同上、`Steps{every_ms = 600_000}` | 恰 6 次跳变,每次 +5.00 |
| TK-017.3 | 同上、`Gap{at_ms = 1_000_000, gap_pct = 10%}` | t = 1 000 000 ms 处 mark 一次跳变 10.00(起始价 × 10%);t = 3 600 000 ms 时 = 130.00 |
| TK-017.4 | `pct = −30%`、`Linear` | 终值 70.00 |

### TK-018 external_fills 的三种形态

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:table
- 里程碑:M2
- 前置:假交易所账户持永续多头 10 lot
- 实现:`crates/hft-testkit/tests/gen.rs::tk_018_external_fill_shapes`

| 行 | 形态 | 期望 |
|---|---|---|
| TK-018.1 | 强平,推送 | 用户流有一条带该 venue 自身强平标记的成交;真值仓位减 10 lot |
| TK-018.2 | ADL,用户流缺口 | 真值仓位变化,用户流没有对应成交 |
| TK-018.3 | ADL,缺口后重连补推 | 先同 TK-018.2;重连后用户流补推同一笔成交(同去重键) |

### TK-019 account_config 产出一致、启动不一致与运行中变化三类序列

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:table
- 里程碑:M2
- 前置:`VenueCaps` 要求单向持仓、全仓、现货不借币
- 实现:`crates/hft-testkit/tests/gen.rs::tk_019_account_config_shapes`

| 行 | 类别 | 期望 |
|---|---|---|
| TK-019.1 | 一致 | 每条 `AccountConfigReport.mismatches = 0` |
| TK-019.2 | 启动不一致 | 第一条 `mismatches ≠ 0` |
| TK-019.3 | 运行中变化(杠杆) | 前若干条为 0,之后一条实际杠杆改变 |
| TK-019.4 | 运行中变化(现货借币开启) | 之后一条的 `AUTO_BORROW` 位置位 |

### TK-020 funding_flows 产出配对、差额、缺笔、多笔与窗口内结算

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:prop
- 里程碑:M2
- 性质:1 000 个样本中五种形态(完全配对、金额差、交易所有而本地无、本地有而交易所无、结算时刻落在 `funding_recon_lag_ms` 内)各至少一次;完全配对的笔两边金额相同
- 生成:`funding_recon_lag_ms = 600_000`;1–50 笔
- 参考:按形态分类计数
- 例数:PR 1 000
- 实现:`crates/hft-testkit/tests/gen.rs::tk_020_funding_flows_shapes`

### TK-021 生成器不读系统时钟与全局随机源

- 规则:[2. proptest 生成器](README.md#2-proptest-生成器)
- 级别:unit
- 里程碑:M1
- 前置:同一 proptest 种子
- 步骤:
  1. 以同一种子分别在两个进程中调用 `market_stream` 与 `perturb` 各生成一次
- 期望:
  - 两个进程的输出逐字节相同
- 实现:`crates/hft-testkit/tests/gen.rs::tk_021_generators_deterministic`

### TK-022 ManualTsc 按固定频率推进

- 规则:[3. 假时钟](README.md#3-假时钟)
- 级别:table
- 里程碑:M1
- 前置:`ManualTsc::new(tsc_hz = 3_000_000_000, start = 1000)`
- 实现:`crates/hft-testkit/tests/clock.rs::tk_022_manual_tsc`

| 行 | 操作 | 期望读数 |
|---|---|---|
| TK-022.1 | 无 | 1000 |
| TK-022.2 | `advance_ns(1000)` | 4000 |
| TK-022.3 | 再 `advance_ns(1)` | 4003 |
| TK-022.4 | `set(10)` | 10 |

### TK-023 ManualUtc 按固定校准对把 TSC 换算为 UTC

- 规则:[3. 假时钟](README.md#3-假时钟)
- 级别:unit
- 里程碑:M1
- 前置:校准对 (tsc = 1_000_000_000, utc_ns = 1_700_000_000_000_000_000),`tsc_hz = 1_000_000_000`
- 步骤:
  1. 换算 tsc = 1_000_000_500
- 期望:
  - UTC = 1_700_000_000_000_000_500 ns
- 实现:`crates/hft-testkit/tests/clock.rs::tk_023_manual_utc`

### TK-024 策略与 sequencer 的测试不需要时钟替身

- 规则:[3. 假时钟](README.md#3-假时钟)
- 级别:unit
- 里程碑:M2
- 前置:`StrategyHarness` 驱动测试策略 `probe-taker`
- 步骤:
  1. 喂 100 条事件
- 期望:
  - `ManualTsc` 的访问计数为 0;每次 `on_event` 拿到的 `Now` 等于事件携带的时间
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_024_no_clock_double_needed`

### TK-025 MockViews 的写入方法在 Views 读出处生效

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:table
- 里程碑:M2
- 前置:`MockViews::new(RefFixture)`,夹具为 hft-strategy-sdk 测试的 GT3 refdata(`tests/sdk_rd.rs`:账户 A1、inst I1(现货)/I2(永续)、group G1 = `0x01000065`);写入方法即 SDK `testing::MockViews` 的方法
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_025_setters_visible`

| 行 | 写入 | 期望经 `Views` 读出 |
|---|---|---|
| TK-025.1 | `at_engine_ns(5_000)` | `now().engine_ns = 5000` |
| TK-025.2 | `bbo(I1, (1.000000, 10), (1.000100, 7))` | `bbo(I1)` 两侧价量与写入相同 |
| TK-025.3 | `book(I1, [0.999900 × 10], [1.000200 × 7])` | `book(I1)` 的最优买、卖档价量与写入相同 |
| TK-025.4 | `funding(I2, rate 0.0001)` | `funding(I2).rate = 0.0001` |
| TK-025.5 | `portfolio(A1, cash …)` | `portfolio(A1)` 可读 |
| TK-025.6 | `legs(A1, G1, naked_qty = 3)` | `legs(A1, G1).naked_qty = 3` |
| TK-025.7 | 已删除:`VenueCaps` 来自 refdata 夹具(`RefFixture`),SDK 的 `MockViews` 没有写入方法 | — |
| TK-025.8 | `mode(A1, G1, Shadow)` | `mode(A1, G1) = Shadow` |
| TK-025.9 | `risk(A1, conn_liveness_ms = 1234)` | `risk(A1).conn_liveness_ms = 1234` |

### TK-026 StrategyHarness 按发生顺序捕获全部意图

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:unit
- 里程碑:M2
- 前置:测试策略 `probe-taker` 参数使下一条 BBO 触发开仓;其逻辑在同一事件内依次发 `OrderIntent`(第一腿,带 `LegLink`)、一个定时器、一条 `Note`
- 步骤:
  1. `harness.feed(&bbo_event)`
- 期望:
  - `CtxCapture` 恰 3 项,顺序为 `OrderIntent`、`Timer`、`Note`,字段与策略写出的相同
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_026_captures_in_order`

### TK-027 Ctx 满时记录 CtxFull

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:unit
- 里程碑:M2
- 前置:测试用策略在一个事件内发 `Ctx` 容量 + 1 个意图(容量取 SDK 常量)
- 步骤:
  1. `harness.feed(&ev)`
- 期望:
  - 捕获的意图数 = 容量;`CtxCapture` 在第(容量 + 1)个意图处记 `CtxFull`,测试可断言它
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_027_ctx_full_recorded`

### TK-028 on_event 中的一次分配使零分配断言失败

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:unit
- 里程碑:M2
- 前置:测试策略的 `on_event` 里插入一次 `Vec::push`(超出预留容量)
- 步骤:
  1. `harness.feed(&ev)`
- 期望:
  - 断言失败(测试以 panic 结束),信息含"分配次数 1"
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_028_alloc_fails_assertion`

### TK-029 on_event 读时钟使断言失败

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:unit
- 里程碑:M2
- 前置:测试策略在 `on_event` 中经 hft-rt 的 TSC 读取 trait 读一次(harness 把它绑定到 `ManualTsc`)
- 步骤:
  1. `harness.feed(&ev)`
- 期望:
  - 断言失败,信息含"读取了时钟"
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_029_clock_read_fails_assertion`

### TK-030 on_event panic 被报告为失败并指明事件

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:unit
- 里程碑:M2
- 前置:测试策略在第 3 个事件 `panic!`
- 步骤:
  1. 依次 `feed` 3 个事件
- 期望:
  - 第 3 次 `feed` 的断言失败信息含事件序号 2(0 起)与 panic 信息;前 2 次的捕获不受影响
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_030_panic_reported`

### TK-031 on_params 校验失败时策略状态不变

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:unit
- 里程碑:M2
- 前置:`harness` 已喂 10 个事件;准备一份 `validate` 必然失败的参数
- 步骤:
  1. 记录 `snapshot` 字节 A;以失败参数调用 `on_params`;记录 `snapshot` 字节 B
- 期望:
  - `on_params` 返回 `Err(ParamError::Invalid{..})`;A 与 B 逐字节相同
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_031_on_params_failure_keeps_state`

### TK-032 snapshot_roundtrip 检出恢复丢失字段

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:table
- 里程碑:M2
- 前置:`probe-taker` 已喂 10 个事件
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_032_snapshot_roundtrip`

| 行 | 策略 | 期望 |
|---|---|---|
| TK-032.1 | 原样 | `snapshot_roundtrip() = Ok(())` |
| TK-032.2 | `restore` 漏读一个字段的植入缺陷版本 | `Err(RestoreError)` 或断言失败,信息含两次快照首个不同字节的偏移 |

### TK-033 ModelHarness 捕获 Advice

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:unit
- 里程碑:M2
- 前置:`probe-target` 的 `Model`,参数使每 5 个事件产出一条 `Advice`
- 步骤:
  1. `model_harness.feed` 10 个事件
- 期望:
  - 共捕获 2 条 `Advice`,载荷不透明字节与 Model 写出的相同,长度 ≤ 4096 B
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_033_model_harness_advice`

### TK-034 参数不合法时 StrategyHarness 的构造返回 ParamError

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:unit
- 里程碑:M2
- 前置:`probe-taker` 的参数中某 per_group 字段超出 `max`
- 步骤:
  1. `StrategyHarness::from_body(&body, 1, &groups, rd, account)`(`body` 为该参数的 SPB1 体,经 `ParamSet::decode`)
- 期望:
  - 返回 `Err(ParamError::OutOfRange{..})`
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_034_bad_params_rejected`

### TK-035 SDK testing feature 提供同名 mock 时只有一份实现

- 规则:[4. 策略测试桩](README.md#4-策略测试桩)
- 级别:unit
- 里程碑:M2
- 前置:启用 `hft-strategy-sdk/testing`
- 步骤:
  1. 比较 `TypeId::of::<hft_testkit::MockViews>()` 与 `TypeId::of::<hft_strategy_sdk::testing::MockViews>()`(SDK 提供时)
- 期望:
  - SDK 提供该类型时两者相等;SDK 不提供时 `hft_testkit::MockViews` 是本 crate 的定义,且 `src/sdk_mock.rs` 只有一处 `struct MockViews`
- 实现:`crates/hft-testkit/tests/sdk_mock.rs::tk_035_single_mockviews`

### TK-036 测试策略只依赖 SDK 与 hft-strat-lib,本 crate 不依赖 registry

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)、[依赖](README.md#依赖)
- 级别:table
- 里程碑:M2
- 前置:测试策略模块 `src/strategies/probe_taker.rs`、`probe_target.rs`、`replay_probe.rs` 的源文本;本 crate 与各引擎 crate 的 `Cargo.toml`;`tools/xtask/deps.toml`
- 实现:`crates/hft-testkit/tests/deps.rs::tk_036_dependency_rules`

| 行 | 检查 | 期望 |
|---|---|---|
| TK-036.1 | 三个策略模块引用的 `hft_*` crate | ⊆ {`hft_strategy_sdk`, `hft_strat_lib`} |
| TK-036.2 | 在 `probe-taker` 源文本后加一行 `use hft_oms::Oms;` | 检查报出 `hft_oms` |
| TK-036.3 | 本 crate `Cargo.toml` | 不含 `hft-strategy-registry` |
| TK-036.4 | 本 crate `Cargo.toml` | 没有 `strategies/` 路径依赖 |
| TK-036.5 | `deps.toml` 的 `no_strategy_closure` | 含 `crates/hft-testkit`(`cargo xtask deps` 规则 9 覆盖本 crate) |
| TK-036.6 | 每个引擎 crate 的 `[dependencies]`/`[build-dependencies]` | 不含 `hft-testkit`(只在 `[dev-dependencies]`) |

### TK-037 TestStrategies 恰含三个测试策略种类且 kind_id 在测试段

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:unit
- 里程碑:M2
- 前置:无
- 步骤:
  1. 枚举 `TestStrategies` 的 `StrategyDesc`
- 期望:
  - 恰 3 项,名字依次为 `probe-taker`、`probe-target`、`probe-np`;三个 `kind_id` ∈ `0xFF00..=0xFFFF` 且互不相同
- 实现:`crates/hft-testkit/tests/probes.rs::tk_037_test_strategy_set`

### TK-038 probe-taker 在价差越过阈值时才发两腿意图

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:table
- 里程碑:M2
- 前置:参数 `spread_th = 10 bp`(Rate raw 1_000_000);两腿 inst A、B 同资产;每行喂一对 BBO 后检查 `CtxCapture`
- 实现:`crates/hft-testkit/tests/probes.rs::tk_038_threshold`

| 行 | A 买一与 B 卖一的价差 | 期望 |
|---|---|---|
| TK-038.1 | 10 bp − 1 raw | 无 `OrderIntent` |
| TK-038.2 | 10 bp | 无 `OrderIntent` |
| TK-038.3 | 10 bp + 1 raw | 恰 1 条第一腿 `OrderIntent`,带 `LegLink`(第二腿 inst B、方向相反) |

### TK-039 probe-taker 收到执行、外部与风控事件时按参数平仓

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:table
- 里程碑:M2
- 前置:`probe-taker` 已持一组两腿仓位;参数 `close_on = [Exec, External, Risk]`
- 实现:`crates/hft-testkit/tests/probes.rs::tk_039_close_on_events`

| 行 | 事件 | 期望 |
|---|---|---|
| TK-039.1 | `Exec`(该 group Broken) | 发平仓意图,目标为 0 |
| TK-039.2 | `External`(外来成交) | 发平仓意图 |
| TK-039.3 | `Risk`(cap 置位) | 发平仓意图 |
| TK-039.4 | 参数 `close_on = []`,同样三类事件 | 不发任何意图 |

### TK-040 probe-target 按时间表发三种算法的 TargetIntent

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:unit
- 里程碑:M2
- 前置:参数时间表:t = 1 s 发 `TakerSlice`、t = 2 s 发 `PairOnFill`、t = 3 s 发 `Simultaneous`
- 步骤:
  1. 以 `engine_ns` 0 到 4 s 每 100 ms 一个定时器事件喂入
- 期望:
  - 恰 3 条 `TargetIntent`,分别在 `engine_ns` = 1e9、2e9、3e9 的事件中,算法依次为上述三种
- 实现:`crates/hft-testkit/tests/probes.rs::tk_040_schedule`

### TK-041 probe-target 的 Model 产出不透明 Advice,参数经 schema 解码

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:unit
- 里程碑:M2
- 前置:`probe-target` 参数 blob 由 `hft-cfg` 按导出的 schema 编码
- 步骤:
  1. 以该 blob 构造 `ModelHarness` 并喂 10 个事件
- 期望:
  - 参数解码成功;至少一条 `Advice`;`Model::mem_bytes` 与 `snapshot_bytes` 有非 0 声明
- 实现:`crates/hft-testkit/tests/probes.rs::tk_041_model_advice`

### TK-042 模板生成的 ci-probe 带超过 1 MiB/group 的 Model 与 pipeline 插件

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:unit
- 里程碑:M2
- 前置:`cargo xtask new-strategy ci-probe --kind-id 0xFF10 --with-model --with-pipeline` 生成到临时目录
- 步骤:
  1. 构建生成物并读取其 `Model::mem_bytes`(1 个 group)
- 期望:
  - `mem_bytes > 1_048_576`;`pipeline/` 目录是可导入的插件包(声明 entry point)
- 实现:待实现

### TK-043 同一种子运行两次得到相同的结果与状态哈希轨迹

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 故障:套件 `core` 的全部故障类别,按其 `DstSuite` 的速率
- 种子:PR 200(提交派生);每夜日期派生
- 断言:每个种子两次 `run` 的 `DstOutcome` 相等、`HashTrail` 逐字节相同、意图流逐字节相同;INV-9
- 不变量:INV-9
- 实现:`crates/hft-testkit/src/scenarios/framework.rs::tk_043_same_seed_same_result`

### TK-044 失败结果给出种子、步、不变量、轨迹与故障表

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:植入缺陷替身 `planted-oms-429-fails`;已知种子 S 在第 N 步违反 I1
- 步骤:
  1. `run(&cfg{seed = S})`
- 期望:
  - `DstOutcome::Fail{seed = S, step = N, invariant = I1, trace, fault_schedule}`;`trace` 的最后一项是第 N 步的事件;`fault_schedule` 与 `cfg` 由 S 生成的故障表相同
- 实现:`crates/hft-testkit/tests/dst_world.rs::tk_044_fail_outcome_fields`

### TK-045 故障表在运行开始时由种子一次性生成并按虚拟时间排序

- 规则:[6. DST 框架](README.md#6-dst-框架)、[DST 的世界模型](README.md#dst-的世界模型)
- 级别:prop
- 里程碑:M2
- 性质:同一种子生成的故障表逐项相同;表按 `virtual_ns` 不减排序;运行过程中不新增故障项(运行结束时已注入的故障 ⊆ 初始表)
- 生成:任意 u64 种子;任意 `FaultRates`
- 参考:直接比较
- 例数:PR 500
- 实现:`crates/hft-testkit/tests/dst_world.rs::tk_045_fault_plan_upfront`

### TK-046 事件堆按 (virtual_ns, gen_seq) 全序弹出

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:向 `World` 压入三个事件:(100, gen 2)、(100, gen 1)、(50, gen 3)
- 步骤:
  1. 弹出三次
- 期望:
  - 顺序为 (50, 3)、(100, 1)、(100, 2)
- 实现:`crates/hft-testkit/tests/dst_world.rs::tk_046_heap_total_order`

### TK-047 每个 actor 有独立的派生随机流

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:种子 S;配置 1 含 `AdvisorActor`,配置 2 去掉它
- 步骤:
  1. 两种配置各运行 1 000 步
- 期望:
  - 两次运行中 `MarketActor` 产出的行情序列逐项相同
- 实现:`crates/hft-testkit/tests/dst_world.rs::tk_047_actor_rng_substreams`

### TK-048 被测代码拿不到随机数源,时间来自虚拟时间

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:`DstDriver`;虚拟时间 1 000 000 ns,`tsc_hz = 1_000_000_000`,起点 tsc 0
- 步骤:
  1. 调 `stamp()`
- 期望:
  - 返回 tsc = 1 000 000;`DstDriver` 的源文件(`src/dst/driver.rs`)不出现 `Rng`、`rand_`、`rand::`、`ChaCha`、`SeedableRng`(时间只来自虚拟时间,输入只来自 `offer`,输出只进 `emit`)
- 实现:`crates/hft-testkit/tests/dst_world.rs::tk_048_driver_has_no_rng`

### TK-049 失败时只打印一行可复制的复现命令

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:同 TK-044,种子 S 在套件 `core` 中运行,第 48213 步违反 I3
- 步骤:
  1. 经 `cargo xtask dst --seeds 1 --seed S` 运行并截取 stderr
- 期望:
  - 恰一行以 `DST FAILED` 开头:`DST FAILED seed=<S 的 16 位十六进制> step=48213 invariant=I3(<名>) -> cargo xtask dst --seeds 1 --seed <同一种子> --suite core --steps 48214 --trace`
  - 按该行命令再运行,得到同一行输出
- 实现:`crates/hft-testkit/tests/dst_report.rs::tk_049_single_repro_line`

### TK-050 失败的种子写出可回放的 journal 片段与故障表

- 规则:[6. DST 框架](README.md#6-dst-框架)、[DST 的世界模型](README.md#dst-的世界模型)
- 级别:table
- 里程碑:M2
- 前置:植入缺陷替身;各行的违反所在步不同
- 实现:`crates/hft-testkit/tests/dst_report.rs::tk_050_failure_artifacts`

| 行 | 违反所在步 | 期望 `target/dst/<seed>/` |
|---|---|---|
| TK-050.1 | 第 5 000 步 | journal 片段恰含违反前 200 条事件;故障表文件;以片段运行 `replayer` 退出 0 |
| TK-050.2 | 第 50 步 | 片段含全部 50 条事件 |

### TK-051 shrink 把植入缺陷的故障表缩到不超过 5 项且仍违反同一不变量

- 规则:[6. DST 框架](README.md#6-dst-框架)、[DST 的世界模型](README.md#dst-的世界模型)
- 级别:table
- 里程碑:M2
- 前置:`budget_s = 120`;每行一个植入缺陷与其已知失败种子(故障表 ≥ 30 项)
- 实现:`crates/hft-testkit/tests/shrink.rs::tk_051_shrink_to_five`

| 行 | 植入缺陷 | 期望 |
|---|---|---|
| TK-051.1 | `planted-oms-429-fails` | 返回的故障表 ≤ 5 项;以它重跑仍违反 I1 |
| TK-051.2 | `planted-hedge-gross-qty` | ≤ 5 项;仍违反 I8 |
| TK-051.3 | `planted-double-hedge-no-cid` | ≤ 5 项;仍违反 I19 |

### TK-052 shrink 预算为 0 时原样返回

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:同 TK-051.1 的失败
- 步骤:
  1. `shrink(&cfg, &fail, 0)`
- 期望:
  - 返回的失败与输入的故障表逐项相同
- 实现:`crates/hft-testkit/tests/shrink.rs::tk_052_zero_budget_identity`

### TK-053 每个不变量都有植入缺陷用例被 DST 在预算内找到

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:table
- 里程碑:M2
- 前置:每行以该植入缺陷替身构建,从种子 1 起最多跑 2 000 个种子(套件按种子派生);在 sequencer 适配器落地之前,植入缺陷在框架自测引擎 `ToyEngine` 中(`toy::PLANTED`;每行开启能触发它的故障类别,TK-053.9 关闭 I6——它在同一进程重放、会先报同一非确定性),TK-053.21–TK-053.23 随适配器与 QF-1、MS-2 场景运行;期望列为第一个失败种子所报的不变量或场景断言;I10 属可选扩展(`chain` feature),启用该 feature 时另加一行;不变量定义归 [testing §4](../../docs/testing.md#4-必须在-dst-与属性测试中持续成立的不变量)
- 实现:`crates/hft-testkit/tests/planted.rs::tk_053_planted_defects_found`

| 行 | 植入缺陷 | 期望 |
|---|---|---|
| TK-053.1 | OMS 在 429 时把订单判为失败,ledger 据此改仓位 | 2 000 个种子内失败;`invariant = I1` |
| TK-053.2 | OMS 静默丢弃未知 client id 的回报;从时间 0 配对注入 FillBeforeAck 与 NoClientIdFill,固定回报延迟且等待限为零,保留原随机 TwoLeg 故障 | `I2`;去掉植入缺陷后同一输入计划通过,且实际交付未解析身份的正量成交 |
| TK-053.3 | 风控不检查每资产未对冲量上限 | `I3` |
| TK-053.4 | `Unknown` 期间同一 group 放行开仓 | `I4` |
| TK-053.5 | kill 后放行不带 `RISK_REDUCING` 的 `Place` | `I5` |
| TK-053.6 | 某组件一个状态字段不进快照 | `I6` |
| TK-053.7 | `CancelAck` 后不释放槽 | `I7` |
| TK-053.8 | 第二腿按毛量对冲(不扣基础币手续费) | `I8` |
| TK-053.9 | sequencer 按 `HashMap` 迭代序处理 | `I9`(抽样重跑的意图流不同) |
| TK-053.10 | cap 解除时有效模式增量维护出错 | `I11` |
| TK-053.11 | `DRAINING` 拦截 `RISK_REDUCING` 单 | `I12` |
| TK-053.12 | `SetMode` 降级时发出开仓单 | `I13` |
| TK-053.13 | `KillBlocked` 计入 `consec_fail` | `I14` |
| TK-053.14 | 对冲单数量 = 基数 + 2 lot | `I15` |
| TK-053.15 | ledger 忽略晚到费用 | `I16` |
| TK-053.16 | L3 减仓单越过 0 | `I17` |
| TK-053.17 | `Drain` 在 HedgeRetry 定时器未清时返回 `Done` | `I18` |
| TK-053.18 | 无 client id 成交先按外来成交对冲、匹配到槽后再对冲一次 | `I19` |
| TK-053.19 | Broken 后从不自动平仓 | `I20` |
| TK-053.20 | Broken 后反复自动平仓 | `I20` |
| TK-053.21 | 对冲定价按"盘口最后变化时刻"判过期 | QF-1 场景断言失败(安静盘口下第二腿未发出) |
| TK-053.22 | MarginStress 只看比率 | MS-2(测试策略版)断言失败(出现强平) |
| TK-053.23 | 对冲重试单带 `reduceOnly`(被假交易所拒绝,裸腿得不到对冲) | 2 000 个种子内失败;报告的不变量 ∈ {`I3`, `I20`} |

### TK-054 一致性集在 x86_64 与 aarch64 上的 HashTrail 与金样相同

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 故障:`fixtures/dst/hashtrail.toml` 登记的套件与故障率
- 种子:`fixtures/dst/hashtrail.toml` 登记的全部种子(版本标签作业;同一提交;数目归 testing §6)
- 断言:`cargo xtask dst --hashtrail check` 在 `ubuntu-latest` 与 `ubuntu-24.04-arm` 上都退出 0;INV-9
- 不变量:INV-9
- 实现:`crates/hft-testkit/src/scenarios/framework.rs::tk_054_cross_arch_conformance`

### TK-055 CrashFault 中途丢弃 sequencer 后由 RecoverDriver 重建

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:dst
- 里程碑:M2
- 故障:`CrashFault{at_step = 由种子在 [1000, 5000] 中选, repeat = 1}`,其余故障按套件 `storage`
- 种子:PR 200;每夜日期派生
- 断言:崩溃后新实例只读内存段 journal 重建;重建后的状态哈希与"同一 journal 从最近快照重放"相同(INV-6);之后的运行继续到步数上限
- 不变量:INV-6
- 实现:`crates/hft-testkit/src/scenarios/framework.rs::tk_055_crash_and_recover`

### TK-056 CrashFault repeat = 2 让同一输入再次崩溃

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:dst
- 里程碑:M2
- 故障:`CrashFault{at_step = K, repeat = 2}`
- 种子:PR 50
- 断言:第一次重建后,`RecoverDriver` 处理到同一输入(同一 `seq`)时再次崩溃;第三个实例启动时 journal 中出现 `TailSkipped{Poison}`(处置规则归 hft-core 与 hft-sequencer);INV-6
- 不变量:INV-6
- 实现:`crates/hft-testkit/src/scenarios/framework.rs::tk_056_poison_repeat`

### TK-057 MockVenue 行为层按 VenueCaps 与规则表产生交易所行为

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:table
- 里程碑:M2
- 前置:`MockVenueCfg`:令牌桶 10 单/秒;拒单码表含 (`-2019`, 保证金不足);死手倒计时超时 30 000 ms;`bbo_ping_ms = 1000`;自身强平订单标记为 client id 前缀 `autoclose-`
- 实现:`crates/hft-testkit/tests/actors.rs::tk_057_behavior_layer`

| 行 | 输入 | 期望 |
|---|---|---|
| TK-057.1 | 同一秒内第 11 张新单 | 回该 venue 的限速拒绝(429 形态),前 10 张被接受 |
| TK-057.2 | 下单时账户可用保证金不足 | 回拒单码 `-2019` |
| TK-057.3 | 30 000 ms 内无死手刷新 | 该账户全部挂单被撤,推送撤单回报 |
| TK-057.4 | 29 999 ms 时刷新一次 | 无撤单 |
| TK-057.5 | 行情连接空闲 3 500 ms | 收到 3 条 pong 形态的连接健康输入(间隔 1000 ms) |
| TK-057.6 | 真值触发强平 | 成交回报的 client id 以 `autoclose-` 开头 |

### TK-058 Sim 账户经 SimActor,不受故障注入影响

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:一个 Live 账户与一个 Sim 账户;全部回报类故障速率 1.0
- 步骤:
  1. 运行 2 000 步
- 期望:
  - Live 账户的回报出现乱序、重复与丢失;Sim 账户的回报序列与无故障时同一种子的序列逐项相同
- 实现:`crates/hft-testkit/tests/actors.rs::tk_058_sim_account_unfaulted`

### TK-059 AdvisorActor 可注入落后,Advice 以旧依据成批到达

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:`probe-target` 的 Model;落后故障在 t = 10 s 起持续 5 s
- 步骤:
  1. 运行到 t = 20 s
- 期望:
  - t ∈ [10, 15) s 内没有 Advice 交付;依据在 [10, 15) s 的 Advice 自 t = 15 s 起交付,其 `basis_seq` 小于交付时 sequencer 的当前 seq
- 实现:`crates/hft-testkit/tests/actors.rs::tk_059_advisor_lag`

### TK-060 CtlActor 注入带主体与角色的命令

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:`control_cmds` 允许全部变体;主体集合 {Admin, Trader, Viewer}
- 步骤:
  1. 运行 5 000 步
- 期望:
  - journal 中每条 `ControlCmd` 都有主体与角色字段;Viewer 主体的命令至少一条
- 实现:`crates/hft-testkit/tests/actors.rs::tk_060_ctl_principals`

### TK-061 ReconActor 按 reconciler 的置位条件填写 AUTO_EXTERNAL 与 sflags

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:table
- 里程碑:M2
- 前置:真值仓位比本地多 1 lot;对账周期 30 s;置位条件归 [reconciler](../../apps/reconciler/README.md#reconcilereport)
- 实现:`crates/hft-testkit/tests/actors.rs::tk_061_recon_actor_flags`

| 行 | 用户流状态 | 期望后一份 `ReconcileReport` |
|---|---|---|
| TK-061.1 | 前后两份报告都没有 `USERSTREAM_DOWN`,用户流稳定满一个周期 | 该仓位条目带 `AUTO_EXTERNAL` |
| TK-061.2 | 前一份报告有 `USERSTREAM_DOWN` | 不带 `AUTO_EXTERNAL`;`sflags` 含 `USERSTREAM_DOWN` |
| TK-061.3 | 用户流在前一份报告的视图点之后才恢复 | 不带 `AUTO_EXTERNAL` |
| TK-061.4 | 注入资金费差异(`funding_flows`) | 报告含逐笔资金费条目 |

### TK-062 HkActor 注入时钟与存储状态

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:故障:t = 5 s 起 persist 滞后 2 s;t = 8 s 磁盘低水位;`ClockStatus` 每 1 000 ms 一条
- 步骤:
  1. 运行到 t = 10 s
- 期望:
  - journal 中有 10 条 `ClockStatus`;t ≥ 5 s 后出现 `Timer{PersistLag}`,t ≥ 8 s 后出现 `Timer{DiskLow}`
- 实现:`crates/hft-testkit/tests/actors.rs::tk_062_hk_injection`

### TK-063 MarketActor 回放记录的行情片段

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:`MarketSource::Recorded(片段)`,片段含 500 条行情记录
- 步骤:
  1. 运行到片段结束
- 期望:
  - 进入 sequencer 的行情事件体与片段中的记录体逐字节相同、顺序相同
- 实现:`crates/hft-testkit/tests/actors.rs::tk_063_recorded_market`

### TK-064 只含 Sim 账户的实例非干净重启后模拟真值由快照与 journal 恢复

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:dst
- 里程碑:M4
- 故障:只含 Sim 账户的配置;`CrashFault` 在有挂单与持仓时发生
- 种子:PR 100;每夜日期派生
- 断言:重建后 `SimActor` 的余额、仓位、挂单与"同一 journal 不崩溃连续运行"在同一 seq 时相同;无 `RecoverMismatch`;无在途时 I16 成立
- 不变量:INV-16
- 实现:`crates/hft-testkit/src/scenarios/framework.rs::tk_064_sim_restart_truth_restored`

### TK-065 故障目录的每个类别可单独开关

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:table
- 里程碑:M2
- 前置:每行只开该类别、速率 1.0,其余类别关;同一种子运行 5 000 步;再以该类别关闭重跑
- 实现:`crates/hft-testkit/tests/faults.rs::tk_065_category_switches`

| 行 | 类别 | 期望 |
|---|---|---|
| TK-065.1 | 连接 | 开:故障表含 ≥ 1 项断开/重连;关:0 项 |
| TK-065.2 | 回报 | 开:≥ 1 项乱序或重复;关:0 |
| TK-065.3 | 拒绝 | 开:≥ 1 项 429 或超时;关:0 |
| TK-065.4 | 两腿与断腿 | 开:≥ 1 项第二腿被拒或盘口不变;关:0 |
| TK-065.5 | 行情 | 开:≥ 1 项交易所时间戳乱序或资金费周期变化;关:0 |
| TK-065.6 | 账户与保证金 | 开:≥ 1 项;关:0 |
| TK-065.7 | 用户流 | 开:≥ 1 项未知事件;关:0 |
| TK-065.8 | 控制 | 开:≥ 1 项命令乱序或过期;关:0 |
| TK-065.9 | 模型与模拟 | 开:≥ 1 项 advisor 落后或模拟回报洪峰;关:0 |
| TK-065.10 | 存储与进程 | 开:≥ 1 项 persist 滞后或崩溃;关:0 |

### TK-066 tests/dst 为每个套件一个入口,三个入口跑同一组套件

- 规则:[6. DST 框架](README.md#6-dst-框架)、[源文件](README.md#源文件)
- 级别:unit
- 里程碑:M2
- 前置:`src/scenarios/` 定义的套件集合 T
- 步骤:
  1. 列出仓库根 `tests/dst/` 的 `#[test]`;运行 `hft-dst list`;读 `cargo xtask dst` 的套件表
- 期望:
  - 三者的套件名集合都等于 T;`tests/dst/` 中每个套件恰一个 `#[test]`
- 实现:`crates/hft-testkit/tests/suites.rs::tk_066_entry_points_agree`

### TK-067 不指定套件时按种子派生选套件

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:table
- 里程碑:M2
- 前置:套件按名字排序为 [`core`, `modes`, `storage`](3 个)
- 实现:`crates/hft-testkit/tests/suites.rs::tk_067_suite_from_seed`

| 行 | 种子 | 期望套件 |
|---|---|---|
| TK-067.1 | 任一种子 s | 下标 = `xxh3_64(s.to_le_bytes()) mod 3` 处的套件 |
| TK-067.2 | 同一种子在 `cargo xtask dst` 与 `hft-dst run` | 相同的套件 |

### TK-068 InvariantSet 可单独关闭某个不变量

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:植入缺陷 `planted-slot-leak`(违反 I7)及其失败种子 S
- 步骤:
  1. 以全部不变量运行 S
  2. 关闭 I7 后运行 S
- 期望:
  - 步骤 1 `Fail{invariant = I7}`;步骤 2 `Pass`
- 实现:`crates/hft-testkit/tests/dst_world.rs::tk_068_invariant_toggle`

### TK-069 步数缺省值为 10 000

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:不带 `--steps`
- 步骤:
  1. `cargo xtask dst --seeds 1 --seed 1`;`hft-dst run --seeds 1..1 --out r.json --crash-dir d`
- 期望:
  - 两者 JSON 中 `steps = 10000`
- 实现:`crates/hft-testkit/tests/suites.rs::tk_069_default_steps`

### TK-070 每步只读访问调试视图

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:种子 S;ToyEngine 与真实 SeqSut 各运行一行;配置 A 启用全部不变量检查器,配置 B 全部关闭
- 步骤:
  1. 两种配置各运行 S
- 期望:
  - 两次的意图流与 `HashTrail` 逐字节相同(检查器不改变被测状态)
- 实现:`crates/hft-testkit/tests/dst_world.rs::tk_070_checkers_read_only`

### TK-071 diff_trails 对相同轨迹返回 None,对不同轨迹给出第一处分歧

- 规则:[7. 状态哈希对比](README.md#7-状态哈希对比)
- 级别:table
- 里程碑:M2
- 前置:轨迹 a:`every_n = 100`,点 seq 100、200、300;各行构造轨迹 b
- 实现:`crates/hft-testkit/tests/hash.rs::tk_071_diff_trails`

| 行 | b 与 a 的差别 | 期望 |
|---|---|---|
| TK-071.1 | 无 | `None` |
| TK-071.2 | seq 200 的 `oms` 不同,其后各点的 `oms` 与 `all` 也不同 | `Some(Divergence{seq = 200, component = oms})` |
| TK-071.3 | seq 200 的 `ledger` 与 `exec` 都不同 | `component` = 按 sequencer 固定段顺序排在前面的那个 |
| TK-071.4 | b 只有前两个点 | `Some(Divergence{seq = 300, ..})`(长度不同在第一个缺失点报告) |

### TK-072 bisect_divergence 在两个采样点之间定位到确切的 seq

- 规则:[7. 状态哈希对比](README.md#7-状态哈希对比)
- 级别:unit
- 里程碑:M2
- 前置:`run(seq)` 返回两侧在 seq 处的哈希,seq < 1234 时两侧相同,≥ 1234 时 `oms` 不同
- 步骤:
  1. `bisect_divergence(run, 1000, 2000)`
- 期望:
  - 返回 1234;`run` 的调用次数 ≤ 12
- 实现:`crates/hft-testkit/tests/hash.rs::tk_072_bisect`

### TK-073 组件哈希取自快照的分组件段,all 等于快照头的 state_hash

- 规则:[7. 状态哈希对比](README.md#7-状态哈希对比)
- 级别:unit
- 里程碑:M2
- 前置:hft-sequencer 写出的一个普通(非 Keyframe)快照
- 步骤:
  1. 由该快照计算 `ComponentHashes`
- 期望:
  - `book`、`oms`、`risk`、`ledger`、`exec`、`strategies` 各等于对应段字节的 xxh3-64;`all` 等于快照头 `state_hash`
- 实现:`crates/hft-testkit/tests/hash.rs::tk_073_component_hashes`

### TK-074 HashTrail 按 every_n 采样

- 规则:[7. 状态哈希对比](README.md#7-状态哈希对比)
- 级别:unit
- 里程碑:M2
- 前置:`every_n = 50`;运行到 seq 230
- 步骤:
  1. 取运行的 `HashTrail`
- 期望:
  - 点的 seq 恰为 50、100、150、200
- 实现:`crates/hft-testkit/tests/hash.rs::tk_074_sampling`

### TK-075 hft-dst run 按升序运行区间并写出结果

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:unit
- 里程碑:M6
- 前置:无违反的构建
- 步骤:
  1. `hft-dst run --seeds 100..109 --out r.json --crash-dir d`
- 期望:
  - 退出 0;`r.json` 含 `bin_version`、`git_sha`、`profile = "ci"`、`suites`、`seed_from = 100`、`seed_to_done = 109`、`seeds_run = 10`、`steps = 10000`、`violations = []`、`elapsed_s`;字段按规范顺序;运行顺序记录为 100、101、…、109
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_075_run_result`

### TK-076 --deadline-s 到期只截断区间,不改变已完成种子的结果

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:unit
- 里程碑:M6
- 前置:区间 100..199,其中种子 150 违反;替身墙钟使截止落在种子 160 运行期间
- 步骤:
  1. `hft-dst run --seeds 100..199 --deadline-s N …`
  2. `hft-dst run --seeds 100..160 …`(无截止)
- 期望:
  - 步骤 1 `seed_to_done = 160`(当前种子跑完才停),`violations` 只有种子 150 一项
  - 步骤 1 与步骤 2 中种子 100–160 的结果与 `HashTrail` 相同
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_076_deadline_truncates`

### TK-077 每完成 checkpoint_seeds 个种子原子地落盘一次

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:table
- 里程碑:M6
- 前置:`--checkpoint-seeds 10`;区间 0..34;文件系统替身记录每次写与改名
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_077_checkpoints`

| 行 | 条件 | 期望 |
|---|---|---|
| TK-077.1 | 跑完 | `--out` 被写 4 次(第 10、20、30 个种子与结束),每次先写 `r.json.new` 再改名 |
| TK-077.2 | 第 25 个种子运行中被 SIGKILL | `r.json` 为第 20 个种子时的完整结果(`seed_to_done = 19`),能解析 |
| TK-077.3 | 不带 `--checkpoint-seeds` | 每 1 000 个种子落盘一次(缺省值) |

### TK-078 收到 SIGINT 或 SIGTERM 时跑完当前种子、写出结果后退出

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:table
- 里程碑:M6
- 前置:区间 0..99 无违反;信号在种子 42 运行中到达
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_078_graceful_signals`

| 行 | 信号 | 期望 |
|---|---|---|
| TK-078.1 | SIGINT | `seed_to_done = 42`;退出 0;`.inflight.json` 不存在 |
| TK-078.2 | SIGTERM | 同上 |

### TK-079 每个种子开始前原子地写进行中标记,正常退出后删除

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:unit
- 里程碑:M6
- 前置:文件系统替身记录调用;区间 0..2,套件按种子派生
- 步骤:
  1. `hft-dst run --seeds 0..2 --out r.json --crash-dir d`
- 期望:
  - 每个种子开始前依次出现:写 `d/.inflight.json.new`、`fsync`、改名为 `d/.inflight.json`;内容为 `{seed, suite, steps, started_utc}`,`seed` 等于将要运行的种子
  - 退出后 `d/.inflight.json` 不存在
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_079_inflight_marker`

### TK-080 种子内 abort 时进程以 SIGABRT 结束且标记指明该种子

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:unit
- 里程碑:M6
- 前置:植入缺陷:种子 7 的第 100 步调用 `abort()`;区间 0..20
- 步骤:
  1. `hft-dst run --seeds 0..20 …`
  2. `hft-dst repro --seed 7 --suite <标记中的套件> --steps 10000 --crash-dir d2`
- 期望:
  - 步骤 1 进程以 SIGABRT 结束;`d/.inflight.json` 的 `seed = 7`;`r.json` 为最近一次落盘的结果
  - 步骤 2 同样以 SIGABRT 结束
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_080_abort_marker`

### TK-081 种子内的普通 panic 记为 panic 违反,其余种子照常完成

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:unit
- 里程碑:M6
- 前置:植入缺陷:种子 7 中 `panic!`;区间 0..20
- 步骤:
  1. `hft-dst run --seeds 0..20 …`
- 期望:
  - 退出 1;`violations = [{seed: 7, suite, step, invariant: "panic"}]`;`seeds_run = 21`;`seed_to_done = 20`
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_081_panic_violation`

### TK-082 违反的种子写出可复现的崩溃目录

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:unit
- 里程碑:M6
- 前置:植入缺陷 `planted-slot-leak`;已知种子 S 违反 I7
- 步骤:
  1. `hft-dst run --seeds S..S …`
  2. 执行 `d/<S>/` 中记录的复现命令(`hft-dst repro …`)
- 期望:
  - `d/<S>/` 含故障表、违反前 200 条事件的 journal 片段与一行复现命令
  - 步骤 2 报告同一种子、同一步、同一不变量 I7
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_082_crash_dir_reproducible`

### TK-083 退出码 0/1/2

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:table
- 里程碑:M6
- 前置:无违反的构建与 `planted-slot-leak` 构建
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_083_exit_codes`

| 行 | 运行 | 期望 |
|---|---|---|
| TK-083.1 | 无违反构建,`--seeds 0..9` | 退出 0 |
| TK-083.2 | 植入缺陷构建,区间含失败种子 | 退出 1 |
| TK-083.3 | `--seeds 10..5` | 退出 2 |
| TK-083.4 | `--out` 所在目录不可写 | 退出 2 |
| TK-083.5 | `--suite nosuch` | 退出 2 |

### TK-084 hft-dst 与 cargo xtask dst 对同一种子结果逐字节相同

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:dst
- 里程碑:M6
- 门槛:④
- 故障:按种子派生套件的故障率
- 种子:固定 100 个(版本标签作业中与一致性集同一构建)
- 断言:每个种子在 `hft-dst run` 与 `cargo xtask dst --seeds 1 --seed S` 下的违反列表与 `HashTrail` 逐字节相同;INV-9
- 不变量:INV-9
- 实现:`crates/hft-testkit/src/scenarios/framework.rs::tk_084_runner_parity`

### TK-085 hft-dst 不读系统时钟、环境变量与网络

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:proc
- 里程碑:M6
- 前置:同一二进制与区间 0..49
- 步骤:
  1. 正常环境运行一次
  2. 设置 `TZ=Asia/Tokyo`、`HFT_X=1`、系统时间偏移 +1 天(`faketime`),在 `unshare -n` 中再运行一次
- 期望:
  - 两次的 `violations` 与每个种子的 `HashTrail` 逐字节相同(`elapsed_s` 除外);第二次退出码与第一次相同
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_085_hermetic`

### TK-086 hft-dst list 列出套件、必过用例与故障率

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:unit
- 里程碑:M6
- 前置:无
- 步骤:
  1. `hft-dst list`
- 期望:
  - 每个套件一行组,含名字、必过用例编号(至少 QF-1、MS-2、MS-3 出现在某个套件)与各故障类别的速率;套件名集合与 TK-066 的 T 相同
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_086_list`

### TK-087 hft-dst 只含测试策略集

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:unit
- 里程碑:M6
- 前置:以 feature `dst-bin` 构建的 `hft-dst`
- 步骤:
  1. 读 `hft-dst list` 与 `cargo tree -p hft-testkit --features dst-bin`
- 期望:
  - 策略集名为 `TestStrategies`;依赖树中没有 `hft-strategy-registry` 与任何 `strategies/*`
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_087_test_strategies_only`

### TK-088 hft-dst repro 复现单个种子

- 规则:[8. hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:unit
- 里程碑:M6
- 前置:植入缺陷 `planted-slot-leak`;`run` 报告种子 S、套件 X、步 N 违反 I7
- 步骤:
  1. `hft-dst repro --seed S --suite X --steps N+1 --trace --crash-dir d2`
- 期望:
  - 退出 1;报告同一 (S, X, N, I7);`--trace` 输出 N+1 行步级轨迹
- 实现:`crates/hft-testkit/tests/hft_dst.rs::tk_088_repro`

### TK-089 CountingAlloc 只计本线程自 scope 起的分配

- 规则:[9. 分配计数与基准驱动](README.md#9-分配计数与基准驱动)
- 级别:table
- 里程碑:M1
- 前置:测试二进制以 `#[global_allocator]` 安装 `CountingAlloc`
- 实现:`crates/hft-testkit/tests/alloc.rs::tk_089_counting_alloc`

| 行 | scope 内的操作 | 期望 `allocs()` |
|---|---|---|
| TK-089.1 | 无 | 0 |
| TK-089.2 | `Vec::with_capacity(8)` 一次 | 1 |
| TK-089.3 | 容量 8 的 `Vec` 内 push 第 9 个元素 | 1(realloc 计一次) |
| TK-089.4 | 另一线程分配 10 次 | 0 |
| TK-089.5 | `drop` 一个 `Vec` | 0(dealloc 不计) |

### TK-090 HistBench 冒烟模式热身 1 000 次并记录每次迭代

- 规则:[9. 分配计数与基准驱动](README.md#9-分配计数与基准驱动)
- 级别:unit
- 里程碑:M1.5
- 前置:冒烟模式;被测闭包每次迭代把计数器加 1;迭代 5 000 次
- 步骤:
  1. 运行 `HistBench`
- 期望:
  - 闭包共被调用 6 000 次;直方图 `count = 5000`;输出 `allocs_after_warmup = 0`;冒烟模式下不绑核
- 实现:`crates/hft-testkit/tests/bench.rs::tk_090_histbench_warmup`

### TK-091 HistBench 记录热身后的分配次数

- 规则:[9. 分配计数与基准驱动](README.md#9-分配计数与基准驱动)
- 级别:unit
- 里程碑:M1.5
- 前置:被测闭包在第 3 000 次调用时分配一次;冒烟模式
- 步骤:
  1. 运行 `HistBench`(热身 1 000、迭代 5 000)
- 期望:
  - 输出 `allocs_after_warmup = 1`
- 实现:`crates/hft-testkit/tests/bench.rs::tk_091_histbench_allocs`

### TK-092 DST 单步开销

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`testkit/dst_step`
- 实现:`crates/hft-testkit/benches/dst.rs::tk_092_dst_step`

### TK-093 全部金样的运行时长

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`testkit/golden_all`
- 实现:`crates/hft-testkit/benches/golden.rs::tk_093_golden_all`

### TK-094 测试策略提供场景用到的固定字段、命令与 non_prod_only 变体

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:table
- 里程碑:M2
- 前置:`TestStrategies` 的导出 schema(`StrategySet::export_schemas()`);每行断言一项
- 实现:`crates/hft-testkit/tests/probes.rs::tk_094_probe_fixed_capabilities`

| 行 | 检查 | 期望 |
|---|---|---|
| TK-094.1 | `probe-target` 字段 `win` | 存在,`structural = true` |
| TK-094.2 | `probe-target` 字段 `thr` | 存在,`structural = false` |
| TK-094.3 | `probe-target` 字段 `advice_max_age_ms` | 存在 |
| TK-094.4 | `probe-target` 的 `cmds` | 恰含 `op 1`(`set_thr`)、`op 2`(`exit_all`)、`op 3`(`pair_lots`)与 `op 4`(`pair_exit`) |
| TK-094.5 | `probe-np` 的 `StrategyDesc` | `non_prod_only = true`;kind_id 与 `probe-taker`、`probe-target` 不同,且在 `0xFF00–0xFFFF` |
| TK-094.6 | `probe-taker`、`probe-target` 的 `StrategyDesc` | `non_prod_only = false` |

### TK-095 ProbeBuild 注入的版本、布局与 Model 大小进入 journal 与检查点

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:unit
- 里程碑:M2
- 前置:两组装配参数 A = `ProbeBuild{build_git = "aaaa", desc_version = 3, model_layout_version = 1, model_path_version = 1, model_mem_bytes = 2 MiB, model_ckpt_bytes = 64 KiB}`,B 与 A 只差 `build_git = "bbbb"`、`desc_version = 4`、`model_layout_version = 2`
- 步骤:
  1. 以 A 跑 1 000 步并写一份 Model 检查点
  2. 以 B 跑 1 000 步并写一份 Model 检查点
- 期望:
  - 步骤 1 的引导块 `WriterEnv.build_git = "aaaa"`,`probe-taker` 的 `StrategyDesc.version = 3`;步骤 2 分别为 `"bbbb"`、4
  - 两份检查点头的布局版本分别为 1 与 2;`probe-target` 的 `mem_bytes` 声明为 2 MiB
- 实现:待实现

### TK-096 probe-target 在缓存中没有未过期 Advice 时跳过目标点

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:table
- 里程碑:M2
- 前置:`StrategyHarness` 驱动 `probe-target`;`advice_max_age_ms = 1000`;目标点在 t = 10.0 s;最近一条 Advice 的依据时刻按行给出
- 实现:`crates/hft-testkit/tests/probes.rs::tk_096_advice_max_age`

| 行 | 最近 Advice 的依据时刻 | 期望 |
|---|---|---|
| TK-096.1 | t = 9.001 s(年龄 999 ms) | 在 t = 10.0 s 发出该点的 `TargetIntent` |
| TK-096.2 | t = 9.000 s(年龄 1000 ms) | 发出(`engine_ns` 等于 `valid_until_ens` 时 `Expiring::get` 仍为 `Fresh`) |
| TK-096.3 | t = 8.999 s(年龄 1001 ms) | 不发出;没有 `TargetIntent` |
| TK-096.4 | 没有任何 Advice | 不发出 |

### TK-097 AdvisorLagFault 只在指定引擎时间区间内使 Advice 推迟交付

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:`probe-target` 的 Model 每 100 ms 产出一条 Advice;`AdvisorLagFault{from_ens = 10 s, to_ens = 13 s, lag_ms = 3000}`;不注入其他故障
- 步骤:
  1. 运行到引擎时间 20 s
- 期望:
  - 依据时刻在 [10 s, 13 s) 的 Advice 交付时刻 = 依据时刻 + 3 000 ms(±0,虚拟时间)
  - 依据时刻 < 10 s 或 ≥ 13 s 的 Advice 交付不推迟
  - 同一种子运行两次,交付序列逐条相同
- 实现:`crates/hft-testkit/tests/actors.rs::tk_097_advisor_lag_by_engine_time`

### TK-098 CrashFault 的各崩溃位置按 hft-sim 的重启恢复步骤重建

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:table
- 里程碑:M4
- 前置:只含 Sim 账户 `A9` 的配置;测试策略在 `A9` 上做 IOC 开平仓;参考 = 同一 journal 不崩溃连续运行;每行在所列位置注入一次 `CrashFault`
- 实现:`crates/hft-testkit/tests/dst_faults.rs::tk_098_crash_points`
- 备注:各位置恢复后的业务期望由 SIM-167–SIM-169 覆盖;本用例只测注入位置本身

| 行 | `point` | 期望 |
|---|---|---|
| TK-098.1 | `SeqInput` | `SimActor` 内存对象保留(同一实例);sequencer 由 `RecoverDriver` 重建 |
| TK-098.2 | `SimMatchedUndelivered` | 崩溃时刻 sim-gw 待交付堆非空;`SimActor` 与 sequencer 都是新实例 |
| TK-098.3 | `SimRepQueued` | 崩溃时刻 `rep-sim` 环非空、sequencer 尚未取走;两者都是新实例 |
| TK-098.4 | `BeforeFirstReport` | 崩溃时刻 `A9` 的第一条余额报告尚未被处理;两者都是新实例 |

### TK-099 CleanRestartFault 做一次干净 Drain 重启

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M4
- 前置:同 TK-098,`A9` 有一张 GTC 挂单;`CleanRestartFault{at_step = 500}`
- 步骤:
  1. 运行到 1 000 步
- 期望:
  - 第 500 步后 journal 末尾为 Drain 快照,随后是新运行的引导块;没有 `RecoverMismatch`
  - 重启后该 GTC 挂单仍在 `SimActor` 的订单表中
- 实现:`crates/hft-testkit/tests/dst_faults.rs::tk_099_clean_restart`

### TK-100 不变量检查器与 invariants.md 的编号一一对应

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:读取 `docs/tests/invariants.md` 的 `### INV-n` 标题集合 E 与 `InvariantSet::all()` 的检查器集合
- 步骤:
  1. 比较两个集合与 `src/dst/invariants.rs` 中 `inv_<n>_` 开头的函数
- 期望:
  - 每个 `I<n>` 恰对应 E 中的 `INV-n` 与一个 `inv_<n>_…` 函数;E 中除可选扩展 INV-10(`chain` feature 未启用时)外每一项都有检查器
- 实现:`crates/hft-testkit/tests/invariants_map.rs::tk_100_checkers_match_invariants_md`

### TK-101 真实引擎的 OMS 观察结果包含最后一个槽

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:`SeqSut<TestStrategies>`;`core` 套件;种子 `0xbe413439dca24fad`;运行 780 步,包含最高有效槽被占用的前缀
- 步骤:
  1. 运行该种子,每步检查全部不变量
  2. 检查最终 `Obs` 中每张表的槽号与计数
- 期望:
  - 780 步完成且没有不变量违反
  - 至少一张表的最高有效槽 `capacity` 出现在 `Obs.slots`
  - 每张表的槽号均在 `1..=capacity`,占用数加空闲数等于容量
- 实现:`crates/hft-testkit/tests/seq_sut.rs::tk_101_observes_last_oms_slot`

### TK-102 回收槽的当前代数墓碑参与迟到回报匹配

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:TK-101 的真实引擎、core 套件与固定种子;完整运行 10000 步,包含第 1171 步的迟到回报
- 步骤:
  1. 每步检查全部不变量,保留最终 `Obs`
- 期望:10000 步完成且没有不变量违反;`Obs.tombs` 包含至少一个无对应占用槽的墓碑,其代数等于 journal 中该槽最近一次成功 New 的代数
- 实现:`crates/hft-testkit/tests/seq_sut.rs::tk_102_observes_recycled_slot_tombs`

### TK-103 晚到手续费故障实际交付且只扣费一次

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:W1、`MockVenue<ToyMatcher>`;逐行测试报价币费用、基础币费用、与 ReportDup 组合、clean 模式;故障延迟 6000 ms
- 步骤:排入一次 `LateFeeOnly`;先发只会 Ack 的挂单,再发两次可成交订单
- 期望:Ack 不消费故障;首次成交费用为零,恰好一条 FeeOnly 在原成交交付后 6000 ms 到达且身份、累计量与费用资产正确;重复成交仍无费用;真值只扣一次费且未交付标记覆盖 FeeOnly;第二次成交正常携带费用;clean 模式不拆费
- 实现:`crates/hft-testkit/tests/actors.rs::tk_103_late_fee_delivery`

### TK-104 FeeOnly 检查器拒绝漏费、重复费和错误匹配

- 规则:[INV-2](../../docs/tests/invariants.md#inv-2-每条订单回报都匹配一个槽或墓碑否则产生告警并进入对账从不被静默丢弃)
- 级别:unit
- 里程碑:M2
- 前置:当前槽、墓碑、旧代数、墓碑 epoch 不同、UNRESOLVED 五条路径 × 基础币/报价币 × 正费用/返佣/零费用
- 步骤:构造独立的 ledger 前后观察行,运行 INV-2;再分别删去入账效果、重复扣费、删去未知订单告警、改动当前槽累计量
- 期望:正确入账全部通过;非零费用漏记或重复扣除均失败;未知归属无告警失败;FeeOnly 改动累计量失败;epoch 不同不算命中墓碑
- 实现:`crates/hft-testkit/tests/fees.rs::tk_104_fee_only_checker`

### TK-105 真实引擎消费晚到报价币手续费

- 规则:[DST 的世界模型](README.md#dst-的世界模型)、[INV-2](../../docs/tests/invariants.md#inv-2-每条订单回报都匹配一个槽或墓碑否则产生告警并进入对账从不被静默丢弃)
- 级别:dst
- 里程碑:M2
- 前置:`SeqSut<TestStrategies>` 与真实 SimExchange;W1、报价币 USDT 费用;50/6000 ms 延迟两行;每行脚本在时间 0 注入一次故障
- 故障:`LateFeeOnly{venue = VS, delay_ms}`;两行的延迟如前置
- 种子:固定种子 1
- 步骤:每行运行 10000 步,每步检查全部不变量
- 断言:两行都完成全部步数;每行 journal 都有且仅有一条非零 FeeOnly,资产等于 USDT;没有只排故障却不生成回报的空过
- 实现:`crates/hft-testkit/tests/seq_sut.rs::tk_105_real_engine_late_fee`

### TK-106 真实引擎的基础币费用配置与对冲余数观察

- 规则:[DST 的世界模型](README.md#dst-的世界模型)、[INV-8](../../docs/tests/invariants.md#inv-8-onfill-第二腿的对冲数量之和等于触发它的第一腿累计净成交量)
- 级别:dst
- 里程碑:M2
- 前置:`SeqSut<FeeStrategies>` 与真实 SimExchange;W1 的 VS 配置 `fee_in_base = true`;测试夹具每 100 ms 发一次现货第一腿 OnFill,每单 2 个永续 lot,组现货净量达到 20 单时转卖,行情保持初始价差;50/6000 ms 延迟两行,包含槽回收与链接复用
- 故障:`LateFeeOnly{venue = VS, delay_ms}`
- 种子:固定种子 1
- 断言:配置实际进入 refdata 的 VenueCaps,FeeOnly 的资产为 BTC;每行运行 10000 步且全部不变量通过;INV-8 观察包含晚到手续费补差与链接复用前的已完成累计量,不能固定 `fee_adjust = 0`;反例漏记或重复费用补差、改错余数、减少历史成交量或删除全部历史桶时相应检查失败
- 实现:`crates/hft-testkit/tests/seq_sut.rs::tk_106_real_base_fee_and_retired_links`

### TK-107 真实引擎逐事件观察与反例

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:真实 `SeqSut<TestStrategies>`、W1、种子 1、2000 步
- 步骤:读取内部 Timer 与 New 决策的逐 Input 观察,对副本分别改错 Kill、有效模式、对冲数量及强制减仓方向,删去模式/保证金/执行单元/在途/决策观察,制造溢出标记和 Input 漏记/重复
- 期望:正常运行全部不变量通过;实际存在内部 Timer 和成功对冲;所有反例被相应检查拒绝,不修改被测引擎
- 实现:`crates/hft-testkit/tests/observations.rs::tk_107_real_observations_and_counterexamples`

### TK-108 观察检查点只覆盖已发布 journal

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:有基础币费用与退休链接累计量的观察检查点
- 步骤:追加超出已发布 journal 边界的费用与历史量,尝试提交后连续恢复两次;再发布新边界并恢复;最后传入早于检查点的边界
- 期望:未提交量回滚,已提交量不漏记也不重复;边界早于保存的检查点时明确报错
- 实现:`crates/hft-testkit/src/dst/seq/audit.rs::tk_108_observation_commit_boundary`

### TK-109 真实保证金强制目标独立重算

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:W1、种子 1、2000 步;账户有永续仓位
- 步骤:记录 MarkPrice 与强平价距标记价 5% 的 MarginReport,触发真实 L3;读取执行层实际保存的目标,与原仓位、旧目标、原比例独立重算值比较
- 期望:全部不变量通过且强制目标非空;翻转目标方向、删除全部目标与比例观察时 INV-17 失败
- 实现:`crates/hft-testkit/tests/observations.rs::tk_109_real_force_target_reference`

### TK-110 真实引擎重启后手续费与链接历史累计量守恒

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:dst
- 里程碑:M2
- 前置:TK-106 的基础币费用夹具,种子 1、6000 ms 延迟、10000 步
- 故障:第 1000、3000 条外部 Input 后各连续非干净重启两次
- 种子:固定种子 1
- 断言:全部不变量通过且确实重启四次;每次恢复前后的累计费用、历史加当前链接的累计净成交和请求量相等;退休历史桶不丢失;实际覆盖撤单回报先记成交量、迟到成交回报再补基础币费用的路径
- 实现:`crates/hft-testkit/tests/observations.rs::tk_110_real_fee_history_across_restarts`

### TK-111 传输 Input 内的主动撤单必须有决策证据

- 规则:[INV-1](../../docs/tests/invariants.md#inv-1-仓位与库存的每一次变化都能追溯到成交证据传输错误从不改变仓位)
- 级别:unit
- 里程碑:M2
- 前置:真实 storage 套件,固定种子 0xcef145ccd5e68192,10000 步
- 步骤:定位传输或定时器 Input 内 PendingNew 到 PendingCancel 的观察;在副本删除 Cancel 决策
- 期望:原始事件通过 INV-1;没有成功撤单决策的相同状态迁移失败,仓位与库存零变化规则不变
- 实现:`crates/hft-testkit/tests/observations.rs::tk_111_transport_cancel_needs_decision_evidence`

### TK-112 不同世界的 journal 重建互不串用配置

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:replay
- 里程碑:M2
- 输入:真实 W1 引擎 A;引擎 B 修改 conn_liveness_ms 与场景墓碑时长,在同一线程随后启动
- 步骤:交错调用 A/B 的 INV-6 重建;丢弃两台引擎后把 A 的 journal 送入新线程,仅从快照和此前的配置记录装配 VerifyDriver
- 期望:启动解码保留 B 的墓碑时长;两份 INV-6 状态与各自运行状态完全相同;新线程逐条 Output 与最终组件哈希一致,没有遗漏输出;不需要 WorldSpec 或线程局部 RiskParams
- 实现:`crates/hft-testkit/tests/journal_config.rs::tk_112_worlds_do_not_share_risk_replay_state`

### TK-113 journal 重建不可用时不得跳过 INV-6

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:真实 W1 引擎,固定种子 1/2、4 步,仅开启 INV-6;包装器仅把重建结果替换为空值或带序号的错误
- 步骤:分别设置每步重建与关闭定期重建,运行到首次失败或场景结束
- 期望:四行均以 INV-6 失败,空值诊断指出 journal 重建不可用,错误保留原始诊断与序号;不能因没有哈希结果通过检查
- 实现:`crates/hft-testkit/tests/journal_config.rs::tk_113_missing_rebuild_fails_instead_of_skipping`

### TK-114 storage 重启中的轮次注记逐字节回放

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:replay
- 里程碑:M2
- 输入:storage 套件,固定种子 0x00cc7b25194f1b2c、0x00cc7b25194f1b3a、0x00cc7b25194f1b4b、0x00cc7b25194f1b4d、0x00cc7b25194f1bc8、0x00cc7b25194f1be0
- 步骤:各运行完整 10,000 步,使用套件原有故障率和全部不变量;逐条检查 RoundClosed 的所属 Input
- 期望:六行均实际结束轮次,注记属于 Fill、PartialFill 或 FeeOnly,不遗留到对账;含定期和结束 INV-6 的全部检查通过;相同种子保留在 storage 回归集
- 实现:`crates/hft-testkit/tests/journal_config.rs::tk_114_round_notes_replay_across_storage_restarts`

### TK-115 每条快照在其完成边界检查 INV-6

- 规则:[6. DST 框架](README.md#6-dst-框架)、[INV-6](../../docs/tests/invariants.md#inv-6-任一时刻从最近的快照加后续事件重建的状态与运行中状态逐组件相同)
- 级别:replay
- 里程碑:M2
- 输入:真实 W1 引擎,4 个世界步,关闭定期重建,每步采样 HashTrail
- 步骤:同一测试入口逐行运行正例与九种反例。首次世界步在外部行情之前依次处理关键帧、按需快照与滚段 Timer,Drain 干净重启后连续崩溃恢复两次;另一行关闭全部检查器;反例行分别删除首条/全部观察、重复首条、改错边界、跨过后续 Input、改错首条风险哈希、标记溢出、返回重建错误、删除首条快照的完整账本视图
- 期望:正常行实际包含五种目的的至少八条快照,检查数等于 journal 快照数;每个真实快照同时经过 LED-267 冷账本核对;开关检查器前后的完整 journal 和非空 HashTrail 逐字节相同;九种反例均以 INV-6 在第 0 步失败,诊断指出原因,不能被后来的正确快照遮盖
- 实现:`crates/hft-testkit/tests/snapshot_checks.rs::tk_115_every_snapshot_checked_with_periodic_rebuild_off`

### TK-116 快照观察只保留已提交边界且缓冲有界

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:两个完成边界的快照观察,仅第一条已发布
- 步骤:连续恢复两次,连续交付两次;再向已满的 4096 条输出观察追加一条
- 期望:只交付第一条一次,未提交观察丢弃且扫描游标回到已提交边界;溢出显式报错,输出不超过容量
- 实现:`crates/hft-testkit/src/dst/seq/snapshots.rs::tk_116_snapshot_observations_follow_commit_boundary_and_capacity`

### TK-117 回放 Model 的路径恢复与 120 秒收敛

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:unit
- 里程碑:M2
- 前置:ReplayModel 路径版本 1 连续处理 10–60 秒的六条 BBO,保存原始样本与路径;一个实例恢复两节,另一个路径版本 2 实例仅恢复原始样本
- 步骤:逐条投递 70–240 秒 BBO;同时检查原始节布局,截断路径恢复反例
- 期望:完整恢复始终逐字节一致;仅恢复原始样本时计数与时间一致、180 秒前路径相关载荷不同,180 秒起完整载荷一致;截断恢复失败且原有路径不变
- 实现:`crates/hft-testkit/tests/replay_probe.rs::tk_117_replay_model_path_restore_and_convergence`

### TK-118 回放 Model 的样本布局与路径金样

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:golden
- 里程碑:M2
- 前置:按规格独立写出的 sdk-replay 金样,原始布局与路径版本均为 1
- 输入:引擎时间 1 s/2 s 的两条 BBO,bid 为 101/102
- 期望文件:`fixtures/strategies/sdk-replay/ckpt/layout.json`、`path.bin`
- 步骤:1 秒、2 秒的 bid 分别为 101、102;比较规范布局 JSON、check_layout 和路径字节;恢复后重写,再向桶 1 注入属于桶 2 的时刻
- 期望:规范布局及 1920 字节路径匹配;两次路径字节一致;错误桶时刻恢复失败且状态不变
- 实现:`crates/hft-testkit/tests/replay_probe.rs::tk_118_replay_model_layout_and_path_goldens`

### TK-119 预热测试 Model 的计数、顺序与恢复可独立检查

- 规则:[5. 解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:table
- 里程碑:M2
- 前置:WarmupModel 空状态,local_kind 9,chunk_no 0/1/2 对应 payload 11/23/47,仅 chunk_no 2 的 last 为真;无 BBO
- 实现:`crates/hft-testkit/tests/replay_probe.rs::tk_119_warmup_model_observes_chunks_and_restores_transactionally`

| 行 | 投递顺序 | 最终 Advice 与原始检查点的五个 u64 |
|---|---|---|
| TK-119.1 | 0、1、2 | `[3,66051,81,4,0]` |
| TK-119.2 | 0、2 | `[2,259,58,4,0]` |
| TK-119.3 | 0、1、1、2 | `[4,16908803,104,4,0]` |
| TK-119.4 | 2、1、0 | `[3,197121,81,4,0]` |

每行通过 check_layout,原始检查点恰 40 字节;完整恢复重写逐字节一致,截断到 39 字节的恢复失败且不污染原状态。

### TK-120 一致性集的 Drain 重启保留恢复闸门

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:dst
- 里程碑:M2
- 前置:真实 `SeqSut<TestStrategies>`、原始 core 套件,不改故障率、时限或采样间隔
- 故障:套件原有故障计划,实际触发 Drain 和自动重启
- 种子:固定种子 7、28、34,每行 2000 步
- 断言:每行实际完成至少一次 Drain 和对应恢复;Drain 快照含 Draining,其他 cap 非空,Recovery 快照不含旧 Draining,其余 cap 原样保留,全部账户仍有 Recovery 且闸门未放行;每条快照通过 INV-6,检查数与 journal 快照数相同;全部不变量通过,强制复跑的 Output 和非空 HashTrail 逐字节一致
- 恢复后 New 意图:种子 28 的两条决策均以 CloseOnly 拒绝且没有分配槽,其余两行没有 New 决策;订单查询照常处理
- 实现:`crates/hft-testkit/tests/journal_config.rs::tk_120_drain_restart_conformance_keeps_recovery_gates`

### TK-121 对冲 place 失败必须有同一链接实例的原始证据

- 规则:[6. DST 框架](README.md#6-dst-框架)、[INV-14](../../docs/tests/invariants.md#inv-14-门控拒绝不计入执行失败计数只有交易所执行结果才计入)
- 级别:dst
- 里程碑:M2
- 前置:真实 `SeqSut<TestStrategies>` 与原始 core 套件,不改故障率与时限
- 故障:套件原有计划
- 种子:固定种子 `0x1af231bb1d6440e2`,完整 10000 步
- 断言:全部不变量与强制复跑通过;Input 1572 为对冲重试 Timer,链接 17 调用前空闲槽为 0,place 返回 NoFreeSlot,重试从 1 到 2,没有 OrderDecision;删除/重复观察、改错 Input/epoch/slot/gen/创建事件/链接号/账户/组、非满表、错误失败种类、错误计数水位、多加计数和缺链接身份共 17 行反例失败;未增加计数的失败也不能漏观察。四种门控 SendReport 不能单独解释计数增加;链接号复用后不能借旧实例证据。关闭检查器后完整 journal 和非空 HashTrail 逐字节相同
- 实现:`crates/hft-testkit/tests/observations.rs::tk_121_hedge_place_failure_requires_exact_evidence`
- 对账覆盖:两次运行均保持原始对账 actor,确认 journal 包含 ReconcileReport;关闭检查器仍刷新 actor 所需仓位,逐记录比较数量、序号、种类、TSC、标志及全部体字节,失败仅输出首个差异位置
- 历史定位:Input 1572 指追加 BalanceCheck 之前的序号;定位时仅扣除此前新增的该种 Note,观察的 input_seq 仍须等于当前记录的原始序号

### TK-122 恢复链接同事件内复用仍保留 place 时的身份

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:Input 前保存链接 17 的 epoch/slot/gen/创建事件,快照恢复后的原始诊断 epoch 与创建事件为零;该 Input 完成时相同链接号已被新一代订单复用
- 步骤:分别解析旧调用与新调用的身份,再逐行给旧调用指定不同的 epoch、gen 和创建事件
- 期望:旧调用准确取 Input 前的完整身份,新调用保留新身份;已知原始字段不被旧检查点覆盖,不改变快照格式
- 实现:`crates/hft-testkit/src/dst/seq/audit.rs::tk_122_place_identity_survives_restored_link_reuse`

### TK-123 原始撮合顺序证据不受回报延迟影响

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:integ
- 里程碑:M2
- 前置:modes 固定种子 `0x7dfee4207b81bfbd`,显式关闭永续现金能力保留旧记录语义,先生成原 10000 步故障表并固定,截取前 316 步核对证据;回报中的 Ack 数量不作为成交
- 步骤:从模拟回报生成序号取得单 5/6/7 的完整身份、数量与价格,与 journal 交付顺序及原始下单方向比较;关闭检查器重跑相同步数。核对该账户全部成交集合,从完整订单身份关联到原始 GwCmd 和已提交 OrderDecision,分别独立计算两种顺序的钱包与费用,不读实际成本或余额变化作为期望
- 期望:生成顺序为 5/6/7,交付顺序为 6/5/7,旧能力模式的 INV-16 在第 315 步以精确 1 单位差额失败;诊断不改变 journal、HashTrail 或回报。此用例验证故障证据,不代表该余额差通过验收;常规回归集中的原种子仍在默认能力下完整运行 10000 步并接受严格核对
- 实现:`crates/hft-testkit/tests/fill_order.rs::tk_123_raw_matching_order_survives_report_jitter`
- 反例:实际余额 ±1、输入费用 ±1、错误 epoch、重复证据、错误生成顺序均不能满足独立演算。参考仅接受本例的单笔全成,其他回报形式明确拒绝
- 历史定位:交付序号 544/555/597 仅扣除此前新增的 BalanceCheck 记录数后核对;成交身份、金额、完整 journal 与 HashTrail 的比较不作归一化

### TK-124 两种成交顺序的独立现金演算覆盖未平仓与正负取整

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:从零仓开始,按 TK-123 原始两笔开仓价建仓,三笔后续交易分别按 5/6/7 和 6/5/7 顺序应用
- 步骤:在第二笔与第三笔后比较有符号成本和钱包;多空镜像重算;另做买 2、卖 3、买 1 的穿零序列
- 期望:未平仓时成本与已实现不同,全平后的钱包分别为 −11800/−11799,多空镜像为 +11800/+11799,均无残留成本;穿零序列最终钱包为 5、仓位与成本为零。全程独立整数计算,不调用生产账本或模拟器的记账函数
- 实现:`crates/hft-testkit/tests/fill_order.rs::tk_124_cash_order_math_keeps_open_cost_and_signed_rounding`

### TK-125 交易所归零和缺少真值不能让 INV-16 跳过检查

- 规则:[6. DST 框架](README.md#6-dst-框架)、[INV-16](../../docs/tests/invariants.md#inv-16-无在途时账本的仓位现货库存与余额等于模拟真值)
- 级别:unit
- 里程碑:M2
- 前置:无在途的账户,永续持仓、库存及额外余额在两边均为空,计价余额为 100
- 步骤:双向分别植入 ±1 的仓位、库存、余额;植入重复的各类键、重复账户/venue、缺少整行真值及元数据。逐步检查和结束检查使用相同输入
- 期望:12 行非零差异和 11 行缺失/重复反例均失败;显式零与缺省零相等,未交付回报仍延后金额比较;现货品种归属不冒充总库存,既有 ADL 排除边界保留
- 实现:`crates/hft-testkit/tests/truth.rs::tk_125_truth_comparison_includes_zero_and_missing_rows`

### TK-126 外来成交回报携带交易所结算时返回的现金

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:table
- 里程碑:M2
- 前置:W1 的 `SimMatcher`,初始回报已交付,原始五笔成交同 [SIM-226](../hft-sim/TESTS.md#sim-226-外来成交直接返回撮合现金无效输入不结算)
- 步骤:每笔先设置成交价,调用 external,在 1 ms 交付边界前后取回报;同种子分别开启和关闭推送
- 共同期望:每笔现金按原始成交的独立常量核对,覆盖量等于该笔数量;钱包、仓位逐笔精确相等。推送开关只改变回报交付,模拟交易所完整状态哈希相等;回报保留账户、venue、品种、方向、数量、价格、trade_id 和 client id
- 实现:`crates/hft-testkit/tests/actors.rs::tk_126_external_reports_carry_exchange_cash_facts`

| 行 | 输入 | 期望 |
|---|---|---|
| TK-126.1 | 原方向五笔,推送开启/关闭 | 现金原始单位 0、0、−11700、0、−100;开启时边界前零笔、边界处恰好一笔,关闭时无回报 |
| TK-126.2 | 多空镜像五笔,推送开启/关闭 | 现金 0、0、+11700、0、+100,相同交付规则 |
| TK-126.3 | 现货买 0.001 BTC@60000 | 两现金字段为零,库存加 0.001 BTC、钱包减 60 USDT |
| TK-126.4 | 未知账户、跨 venue 品种、零数量三行 | DST 明确失败,模拟状态哈希不变,不产生 ExternalFill |

### TK-127 原乱序反例使用实际现金后钱包逐单位一致

- 规则:[DST 的世界模型](README.md#dst-的世界模型)、[INV-16](../../docs/tests/invariants.md#inv-16-无在途时账本的仓位现货库存与余额等于模拟真值)
- 级别:dst
- 里程碑:M2
- 前置:modes 原种子 `0x7dfee4207b81bfbd`、原 10000 步故障计划,W1 仅为永续 VP 默认启用 `VenueSpec.perp_cash`;引擎与模拟器能力位一致,现货拒绝该开关
- 故障:原始 modes 故障表,不调整延迟、故障率或检查间隔
- 种子:前 316 步诊断窗口和完整 10000 步分别运行,均强制重跑
- 断言:诊断窗口保留原交付乱序,从原始成交及完整订单身份独立计算每笔现金,逐笔与现金字段相等;独立按交付顺序估算仍多 1 单位,实际两边钱包均为 49999683575。关闭检查器后完整 journal、非空 HashTrail、撮合证据及真值逐字节相同;完整 10000 步全部不变量及快照重建通过。前缀与全程分别从首个、中间和最后一个仍有成交后缀的快照交给 JournalLedger 冷读,全部钱包、仓位和库存与热引擎逐单位相同
- 实现:`crates/hft-testkit/tests/fill_order.rs::tk_127_explicit_cash_closes_the_original_wallet_difference`
- 边界:原 TK-123 显式关闭能力以保留旧协议反例,不能因新用例通过而删除失败回归种子;默认世界仍须通过完整 CI

### TK-128 无 client id 副本按已到达的同账户 Ack 匹配并保留现金

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:真实 SimMatcher、开启永续现金,两个账户先分别外来开空;之后按不同 slot/gen/epoch 平仓,价格按世界的永续 tick 对齐,每次下单前刷新报价。两账户的交易所订单号相同,每笔实际现金非零
- 输入:原回报与 NoClientIdFill 的六行对照,`within_hold = false/true` × 第二账户 Ack 同时先排入队列/晚到但在等待限内/晚到且等待限为零;固定行为层延迟以明确交付边界
- 步骤:两套相同匹配器分别走原回报及无 client id 副本路径,逐账户发平仓单,继续推进到实际回报到达,收集 Ack、原成交和故障副本并比较真值;FillBeforeAck 不得在空轮询中被提前消费
- 期望:每次故障实际多交付一份成交副本;已有 Ack 时完整身份、数量、费用与现金逐字节等于原成交。限内晚 Ack 行的副本只在 Ack 到达时交付,到达前真值仍标在途;等待限为零则副本保持零 epoch/gen 与 UNRESOLVED,不能借未来 Ack 或另一账户的同号订单。原成交仍原样交付,两边模拟状态哈希和真值相同
- 边界:仅覆盖已排队 Ack 的等待;未排队 Ack、超时转外来成交、终态映射过期和完整 LK-4 仍不计通过
- 实现:`crates/hft-testkit/tests/actors.rs::tk_128_no_client_id_uses_the_accounts_ack_identity_and_keeps_cash`

### TK-129 已有 Ack 的无 client id 现金成交通过完整故障计划

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:dst
- 里程碑:M2
- 前置:W1 显式启用永续现金,全部检查器开启
- 故障:原 core 故障计划,不调整延迟或故障率
- 种子:`0xd05efd09f013189a`、`0xd05efd09f01318f6`、43,各 10000 步,强制复跑
- 断言:全部不变量、强制复跑及非空 HashTrail 通过,实际发生快照重建;原第 591/597/590 步的已有或限内到达 Ack 不会错误进入无法匹配的防御路径
- 实现:`crates/hft-testkit/tests/seq_sut.rs::tk_129_acknowledged_no_client_id_cash_reports_survive_the_full_fault_plan`

### TK-130 外来成交历史保留原始现金且满表原子失败

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:真实 SimMatcher,多空镜像先开 200000 原始数量,价格从 600189000 变到 600275000 后平 100000
- 步骤:不推送的开平仓均保留原始凭据,反复读取并比较全部字节;另一账户产生下一笔成交。另将 4096 行历史填满,尝试第 4097 笔
- 期望:平仓现金分别为 +8600/−8600,历史含完整身份、client id 与现金覆盖量,读取和修改取出的副本不影响原始事实、交易所哈希或交付;满表不淘汰、不结算、不生成回报
- 实现:`crates/hft-testkit/tests/external_receipts.rs::tk_130_external_history_keeps_original_cash_and_fails_before_overflow`

### TK-131 ADL 重连补报复用原始成交且不重复结算

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:table
- 里程碑:M2
- 前置:真实 SimMatcher,与 TK-130 相同的非零现金成交,行为层延迟固定以明确交付边界
- 共同期望:所有交付副本除 cause 由 venue 标记为 Adl 外,逐字节等于原始凭据;副本不增加历史、不改变交易所哈希、仓位或钱包。补报结束后不再标在途,对应品种未加入永久排除
- 实现:`crates/hft-testkit/tests/external_receipts.rs::tk_131_adl_replay_waits_for_stream_and_never_settles_twice`

| 行 | 输入 | 期望 |
|---|---|---|
| TK-131.1 | 多空 × gap 开关 × replay 开关 × 用户流正常/断连,共 16 行 | 原始推送数为 !gap,补报数为 replay;补报最早在 1 ms 或恢复边界的较晚者到达,边界前不交付 |
| TK-131.2 | 两笔断流 ADL,第二次断连延长恢复时刻 | 两笔分别保留 trade_id 2/3 与现金 −8600/−4300;旧恢复边界不补报,新边界各交付一次 |

### TK-132 ADL 原始补报经真实引擎和恢复后钱包一致

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:integ
- 里程碑:M2
- 前置:真实 SeqSut 与 SimMatcher,多空镜像 × 首次补报前是否重启,共四行
- 步骤:先将原始开仓送入引擎;断流 ADL 仅改变交易所真值,用户流恢复后送入保存的成交。首笔入账后重启,再交付两份相同副本
- 期望:钱包精确为初始值加一次 +8600/−8600,仓位保留原方向的 100000;每个入账边界的独立 JournalLedger 与热账本一致,验证回放的 Output 和组件哈希一致,恢复后不重复入账
- 边界:此例只覆盖补报去重和恢复,AUTO_EXTERNAL 合成与 auto_held 抵扣归 LED-081
- 实现:`crates/hft-testkit/tests/external_receipts.rs::tk_132_replayed_adl_keeps_real_engine_wallet_after_recovery`

### TK-133 原始凭据补账后重启再确认

- 规则:[实现规格](README.md#dst-的世界模型)
- 级别:integ
- 里程碑:M2
- 前置:真实 SeqSut 与 SimMatcher,多空 × 100000/200000 缺口四行
- 步骤:仅交易所记入原始成交,由 ReconActor 打包补账;重启后交付 ADL 补报和重复副本;逐边界核对冷热账本及验证回放
- 期望:原始历史经 ReconActor 打包补账,钱包等于真值;重启后的 ADL 补报及重复回报均不重复结算,auto_held 清零,每个边界独立账本和验证回放一致
- 实现:`crates/hft-testkit/tests/external_receipts.rs::tk_133_raw_receipts_reconcile_gap_then_confirm_after_restart`

### TK-134 已提交冷视图自动选择原始补账凭据

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:integ
- 里程碑:M2
- 前置:真实 SeqSut 与 SimMatcher,多空 × 部分/全量 ADL 四行,每次缺口包含不同现金额的两笔成交
- 步骤:从完整交易所历史自动排除已入账成交,按账户、venue、inst 与原始身份选择凭据;补账后重启,倒序补报并重复交付
- 期望:全量归零仍产生 Position;每个账户/venue 独立 Summary;第二个稳定轮次选中且只选中两笔原件;相同分页副本不重复,相同 trade_id 的其他账户不混入;补账与确认后 I16 不带排除项且实际执行,独立冷账本与验证回放一致
- 反例:遗漏原件、冲突副本、缺少 cash_qty、反向成交、把虚拟时间当 view_seq、错误引擎数量、不完整身份、未知费用资产、历史容量溢出与未知品种都失败
- 实现:`crates/hft-testkit/tests/reconcile_actor.rs::tk_134_select_original_receipts_from_committed_history`

### TK-135 真实对账轮次必须稳定且观察完整

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:integ
- 里程碑:M2
- 前置:真实引擎存在全量 ADL 缺口,首轮 Position 已提交
- 步骤:分别注入本账户 Down、Down/Up、引擎重启、已入账成交的重复推送、变化的交易所数量、缺失另一账户真值及另一账户断流
- 期望:前六种不得自动补账,缺失真值直接失败;另一账户断流不改变本账户资格;同一提交边界重复轮询不能充当第二轮;缺少原始历史直接失败;连续三轮在途强制报告仍不自动补账,连续两轮扩仓或反向仓位也不自动补账
- 边界:本用例不替代 LED-081 的 PairOnFill、标准种子集及节 1/13/22 逐事件验收
- 实现:`crates/hft-testkit/tests/reconcile_actor.rs::tk_135_committed_round_requires_stable_complete_observations`

### TK-136 普通去重环回收后仍能识别已入账原件

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:integ
- 里程碑:M2
- 前置:真实引擎存在两笔未推送的 ADL;再依次入账 `ext_dedup_cap + 1` 笔独立成交
- 步骤:确认最早成交身份已被热账本 FIFO 淘汰;保留完整历史,执行两个稳定对账轮次
- 期望:已入账身份由原始 journal 的完整事件边界保留;仅选中两笔漏记原件,不把淘汰的旧成交再算进缺口;I16 和独立冷账本/验证回放通过
- 实现:`crates/hft-testkit/tests/reconcile_actor.rs::tk_136_receipt_selection_survives_hot_dedup_eviction`

### TK-137 对账 actor 迁移须复现旧金样并保持检查器透明

- 规则:[DST 的世界模型](README.md#dst-的世界模型)、[HashTrail](README.md#7-状态哈希对比)
- 级别:integ
- 里程碑:M2
- 前置:`fixtures/dst/recon-before.toml` 固定 CI 212 的 50 个种子、套件、每行 2000 步及摘要
- 步骤:仅用测试适配器重现旧报告与旧输入环,旧轮次时刻按调度周期累加;PartialFills 时长置零并保留原计划条目以复现历史空操作;同一配置运行真实已提交对账 actor;再次关闭检查器运行新 actor
- 期望:旧 actor 重现 50 个旧摘要;首条 journal 差异为虚拟 30 s 的 ReconcileReport,此前逐字节相同;新 actor 开关检查器的全部 journal、网关意图、完整 HashTrail 与最终交易所真值逐字节相同;三个运行均达到原步数且不变量通过
- 证据:CI 保存 `target/dst/reconcile-migration.json`,列出每行的新旧摘要、首次差异序号/时刻和记录数;当前金样保持原种子、套件、步数及哈希范围
- 实现:`crates/hft-testkit/tests/reconcile_migration.rs::tk_137_recon_migration_preserves_prefix_and_checker_transparency`

### TK-138 补账提交后才解除 ADL 缺口并补推原件

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:integ
- 里程碑:M2
- 前置:真实引擎与 SimExchange,多空 × 部分/全量 ADL × 有无补推 × 有无重启共 16 行,原件现金非零
- 步骤:断流期间注入 ReconciledAdl,恢复后执行两个真实冷对账轮次,提交批次后确认入账,按行补推及重复交付
- 期望:只结算一次,生成但未提交的批次不能解除缺口;重启后同样成立;补推抵扣一次,无补推保留待确认量;按脚本成交价和数量独立计算现金及最终钱包;结束时无在途/排除项/活动订单,严格 I16 与冷重建通过;保存 16 行 CI 证据
- 反例:被拒绝批次不解除缺口,其他账户和其他成交键不能充当凭据;未断流或零仓位注入明确失败且交易所事实不变
- 边界:直接开仓只验证测试基建,不替代 LED-081
- 实现:`crates/hft-testkit/tests/reconcile_actor.rs::tk_138_reconciled_adl_waits_for_commit_then_restores_full_truth`

### TK-139 外部结算事件的账本字节与观察边界必须完整

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:integ
- 里程碑:M2
- 前置:真实原件补账事件,热端在事件完成时独立记录节 1/13/22
- 步骤:核对冷账本同一边界;分别删除观察、提前/延后边界、翻转一个字节、截短和清空字节
- 期望:原始观察逐字节相等,所有反例失败;字节差异锁存,之后补回原始数据仍失败
- 实现:`crates/hft-testkit/tests/reconcile_actor.rs::tk_139_ledger_event_bytes_and_boundaries_cannot_be_skipped`

### TK-140 整个 DST 循环中的补账交付不受检查器开关影响

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:integ
- 里程碑:M2
- 前置:种子 1,probe-target 用 PairOnFill 实际开仓,场景墓碑时长 10 s 记录进配置;10 s 断流,11 s 部分/全量 ADL,15 s 恢复;有无延后补报共四行
- 步骤:每 100 ms 的固定报价片段持续至 89.9 s,保留全部 actor 与定时器,运行至 90.5 s 或 10,000 步,让 90 s 的周期报告实际交付;再次关闭全部检查器运行;四行独立执行,汇总失败组合
- 期望:实际产生一组原件补账,每次补账与确认事件均有账本字节核对;结束时严格真值比较不跳过;两次 journal、出环意图、交付流、HashTrail 全字节及真值相同
- 反例:保持默认一小时墓碑时长时,实际报告在途阻塞,本场景期间不补账,缺口仍在途
- 边界:本用例覆盖循环交付与检查器透明性,不替代 LED-081 的开平仓和标准种子集
- 实现:`crates/hft-testkit/tests/reconcile_world.rs::tk_140_committed_gap_delivery_is_independent_of_checkers`

### TK-141 PairOnFill 命令支持独立手数与退出

- 规则:[解耦门槛夹具](README.md#5-解耦门槛夹具)
- 级别:table
- 里程碑:M2
- 前置:ProbeTarget 的 SDK 测试桩,定时目标关闭
- 输入:关闭定时目标,调用 pair_lots 的 1、2、6 手及 0、负数、超界值,随后 pair_exit
- 期望:合法手数只发一条 PairOnFill,两腿量为正负相应 lot 倍数;非法值无意图;退出目标为零且算法仍是 PairOnFill;既有调度和参数默认值不变
- 实现:`crates/hft-testkit/tests/probes.rs::tk_141_pair_commands_open_lots_and_exit_without_changing_schedule`

| 行 | 命令 | 期望 |
|---|---|---|
| TK-141.1 | pair_lots 1、2、6 手 | 一条 PairOnFill,两腿正负对应 lot 倍数 |
| TK-141.2 | pair_lots 0、负数或超界 | 无意图 |
| TK-141.3 | pair_exit | PairOnFill 零目标,Exit 原因 |

### TK-142 完整结算场景不能遗漏执行或观察

- 规则:[DST 框架](README.md#6-dst-框架)、[LED-081](../hft-ledger/TESTS.md#led-081-auto_external冷读者账本与-auto_held-与热进程逐值相同)
- 级别:integ
- 里程碑:M2
- 前置:LED-081 的种子 0–3,覆盖部分/全量 ADL 与有无补推
- 步骤:开启检查器运行,逐条确认四个控制命令实际应用;关闭检查器重跑;分别删除执行证据、账本检查和抵扣注记,设置最终在途,扰动钱包 1 单位,截短运行,关闭控制 actor 或改变确认命令来源
- 期望:完整场景通过,两次 journal、交付、意图、HashTrail 与真值逐字节相同;所有反例失败,不能从套件基线的 pass 推断场景已通过
- 实现:`crates/hft-testkit/tests/ledger_case.rs::tk_142_confirmed_case_executes_and_rejects_missing_evidence`

### TK-143 CLI 场景选择与复现必须执行专属断言

- 规则:[hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:proc
- 里程碑:M2
- 前置:带 dst-bin 的 hft-dst
- 步骤:list 后运行 LED-081;把步数限制为 1 并复现;运行完整种子 2,比较 run 与 repro 的单种子证据
- 期望:截短场景退出 1 且复现保留 --case,完整运行退出 0 且含实际补账、抵扣和无跳过真值;未知编号、套件冲突、hashtrail 带 case、超过 4096 的显式场景批次退出 2
- 实现:`crates/hft-testkit/tests/dst_cli.rs::tk_143_cli_case_selection_and_repro_are_not_baseline_runs`

### TK-144 外来订单的交付副本不得再次结算

- 规则:[DST 世界模型](README.md#dst-的世界模型)
- 级别:table
- 里程碑:M2
- 前置:真实 SimMatcher 的启动报告已交付且无在途,永续开仓后独立报价变化,再平一半
- 输入:买/卖、正/零成交 ID、有/无先发原件共八行;另两行缺少已确认槽或用户流断开
- 期望:每行结算一次,现金分别为正负 8600,历史原件与全部交付的身份和金融字段相同;按实际响应期限排空报价与仓位变化触发的保证金报告,在最后交付时点之后检查无在途;交付后仓位、库存与钱包真值不变,匹配器时钟正常前进;前置缺失在改变真值前失败
- 实现:`crates/hft-testkit/tests/foreign_ledger.rs::tk_144_foreign_order_delivery_retains_original_settlement_and_identity`

| 行 | 输入 | 期望 |
|---|---|---|
| TK-144.1 | 买/卖 × 正/零 ID × 有/无原件先发,八行 | 原始现金与累计副本相同,交付不再次结算 |
| TK-144.2 | 旧身份但没有已确认槽 | 故障失败,撮合状态与历史不变 |
| TK-144.3 | 用户流断开 | 故障失败,不生成成交 |

### TK-145 防御成交场景的执行与逐事件证据不可省略

- 规则:[DST 世界模型](README.md#dst-的世界模型)、[LED-080](../hft-ledger/TESTS.md#led-080-oms-防御路径的外来成交冷读者账本与热进程逐值相同)
- 级别:integ
- 里程碑:M2
- 前置:LED-080 种子 0–7,独立组合方向、成交 ID 和原件先发方式
- 步骤:完整运行并逐原件检查;删除账本边界、原件、配对执行和分支注记,篡改原件现金和最终钱包,留下在途、重复边界、插入外来往返期间接受的新单或截短运行;关闭全部检查器重跑
- 期望:完整场景有真实开平仓、非零结算现金和全部冷热核对,外来往返期间没有引擎新单;注记按完整 `Note.code` 识别,删除反例必须实际删除匹配记录;所有反例失败;检查器开关前后 journal、交付流、意图、HashTrail 与真值相同
- 实现:`crates/hft-testkit/tests/foreign_ledger.rs::tk_145_foreign_case_requires_every_execution_and_exact_ledger_boundary`

### TK-146 CLI 防御成交场景保留原始证据与复现身份

- 规则:[hft-dst 运行器](README.md#8-hft-dst-运行器)
- 级别:proc
- 里程碑:M2
- 前置:带 dst-bin 的 hft-dst
- 步骤:list 后运行 LED-080 种子 0,先限制为 1 步,再完整运行与复现
- 期望:短运行失败且复现命令保留 case;完整运行与复现的逐种子证据相同,包含非空原件、外来订单、账本边界、完整 journal 验证计数及无跳过真值
- 实现:`crates/hft-testkit/tests/dst_cli.rs::tk_146_cli_foreign_case_keeps_settlement_evidence_on_run_and_repro`

### TK-147 外来成交检查器保留完整去重作用域与键分支

- 规则:[DST 框架](README.md#6-dst-框架)、[INV-19](../../docs/tests/invariants.md#inv-19-对冲不越过裸量减仓不越过仓位同一笔成交不被对冲两次)
- 级别:table
- 里程碑:M2
- 前置:真实适配器的 InputRec/InputSlot 观察及 I2、跨步 I19 检查器
- 输入与期望见下表;合法项随后重复一次必须失败,不能通过换方向或金额绕过去重
- 实现:`crates/hft-testkit/tests/foreign_ledger.rs::tk_147_external_dedup_observation_keeps_scope_and_key_domain`

| 行 | 输入 | 期望 |
|---|---|---|
| TK-147.1 | 正 ID 相同,改变订单号与累计量 | 仍观察到同一 Trade 键 |
| TK-147.2 | 零 ID,两个订单累计量相同;同订单的后续累计量 | 分别为独立 OrderSeq 键,允许首次入账 |
| TK-147.3 | 正 ID 与零 ID 的累计量数字相同 | 两个键分支独立,允许各入账一次 |
| TK-147.4 | 两种键分别只改变账户、venue 或 inst | 六个独立作用域,不互相误判重复 |
| TK-147.5 | 同键在同事件或跨步重复入账,含改变方向与金额 | I19 失败 |
| TK-147.6 | 零 ID 且缺订单号或累计量;已入账 External 缺身份 | 观察无有效键,I19 拒绝缺失身份 |
| TK-147.7 | 其他 kind 的字节碰巧可解为 ExternalFill | 不产生外来成交身份 |
| TK-147.8 | I2 的保留键与当前未记账输入分别差账户、venue、inst、键分支、订单号或累计量 | 六种差异均不能充当去重证据,完整同键才通过 |

### TK-148 完整 journal 验证早期输出与每次独立运行

- 规则:[DST 框架](README.md#6-dst-框架)
- 级别:table
- 里程碑:M2
- 前置:真实 W1 引擎产生关键帧、按需快照、普通滚段、Drain 重启和两次崩溃恢复,共五段、四次独立运行;验证器只取得原始 journal 和段时钟
- 期望:正例的 Input、Output、快照及全部覆盖计数由原始记录独立统计,交给新线程仍完全相同;下表各反例失败,前三行从最新快照开始的局部验证仍通过;CmdResult 体和标志分歧保留两侧有界字节、标志、最近 Input 和邻近记录,消息少于 4096 字节
- 实现:`crates/hft-testkit/tests/journal_replay.rs::tk_148_full_journal_checks_every_run_and_earlier_divergences`

| 行 | 改动 | 期望 |
|---|---|---|
| TK-148.1 | 较早 CmdResult 体翻转一位 | 精确定位原 seq |
| TK-148.2 | 较早 CmdResult 的 SHADOW 标志翻转 | 精确定位原 seq |
| TK-148.3 | 较早 Keyframe 的 engine_ns 翻转一位 | 精确定位快照 seq |
| TK-148.4 | 最后一条 Input 之后额外追加 Output | EOF 仍报告额外 Output 的 seq |
| TK-148.5 | 仅保留完整的后续段 | 拒绝缺少初始 Startup |
| TK-148.6 | 空 journal | 明确失败 |
| TK-148.7 | 滚段索引加一 | RunBoundary |

### TK-149 DST 结束必须取得完整回放结果且不改变执行

- 规则:[DST 框架](README.md#6-dst-框架)
- 级别:table
- 里程碑:M2
- 前置:真实 W1 引擎运行 64 步,关闭定期重建,每步采样 HashTrail
- 步骤:开关检查器各跑一次;随后分别返回空验证结果、验证错误和篡改旧 CmdResult 的真实验证错误
- 期望:正例的已验证末 seq 与记录数等于完整 journal,Input 和 Output 比较数非零;关闭时结果为空,完整 journal、交付、出环与非空 HashTrail 逐字节相同;三行反例在最后一步以 INV-6 失败,不保存成功证据
- 实现:`crates/hft-testkit/tests/journal_replay.rs::tk_149_end_guard_requires_full_replay_and_leaves_the_run_unchanged`

| 行 | 完整回放结果 | 期望 |
|---|---|---|
| TK-149.1 | 真实验证 | 非空成功计数 |
| TK-149.2 | 关闭检查器 | 无验证结果,执行字节与第一行相同 |
| TK-149.3 | 空结果 | INV-6,unavailable |
| TK-149.4 | 返回错误 | INV-6,保留错误信息 |
| TK-149.5 | 旧 CmdResult 被篡改的真实回放 | INV-6,保留首错序号 |

### TK-150 物理编号的真实世界逐运行回放与观察边界

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:W1 开启 physical_journal,按 TK-148 产生五段、四次运行;另在默认无故障世界运行 128 步
- 步骤:冷验证完整历史并按体长度核对每条记录跨度;开关检查器分别运行同一种子
- 期望:每条 Multi 的占号按体长度独立计算,相邻记录连续,完整回放计数覆盖五段四次运行;世界循环实际核对快照与资金事件且保留完整回放结果;检查器开关后的完整 journal、出环、交付与 HashTrail 相同
- 实现:`crates/hft-testkit/tests/journal_replay.rs::tk_150_physical_journal_replays_each_run_and_snapshot_boundary`

### TK-151 快照前缀与观察必须等到末片

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:物理编号的三片快照,末片带 EVENT_END,已经发布
- 步骤:分别按三片的序号选择前缀和设置观察边界
- 期望:发布边界是末片;截到第一片或第二片的前缀不包含快照和分片索引,对应观察均拒绝;末片前缀保留完整逻辑体及首末片索引,观察通过
- 实现:`crates/hft-testkit/src/dst/seq/snapshots.rs::tk_151_snapshot_prefix_requires_the_final_physical_part`

### TK-152 真实 Model 从创建处重算并跨重启连续

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:物理编号 W1,第二实例为 probe-target,启用真实 Advisor,无检查点
- 步骤:四轮各交付 16 条 BBO,逐次把真实 Advisor 输出交回引擎;轮间依次 clean restart、两次 CrashFault 恢复,每轮保存快照并从创建处独立重算
- 期望:真实 Advisor 的空起步注记在时刻 0 且早于首条行情;Advice 为真实 16 B 载荷,导出保留已发布的完整事件及其尾部测量 Meta,四段全部 Input 参与重算,每条已记录 Advice 都比较且无分歧;篡改首条载荷或 basis_seq 均失败
- 实现:`crates/hft-testkit/tests/model_journal.rs::tk_152_real_model_replays_across_restarts_and_rejects_changed_advice`

### TK-153 Model 世界不依赖检查器并拒绝缺少适配器

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:Live taker 与 Sim target,真实 Model、物理编号,600 步,第 120 步崩溃恢复
- 步骤:开关全部检查器各执行一次,独立重算 Model;另用不支持 Model/物理段的 ToyEngine 分别请求两种模式
- 期望:启动快照的观察由首步 INV-6 核对,真实世界非空 Advice 全量重算成功,完整 journal、出环、交付和非空 HashTrail 逐字节相同;缺少适配器明确失败,无合成回退或空列表跳过
- 实现:`crates/hft-testkit/tests/model_journal.rs::tk_153_model_world_keeps_checker_independence_and_requires_adapters`

### TK-154 Model 输出缓冲与已读前缀受检查

- 规则:[6. DST 框架](README.md#6-dst-框架)
- 级别:unit
- 里程碑:M2
- 前置:真实 Advisor 与有界输出环
- 步骤:分别填满条数与字节容量,继续写入;改变已读首条记录;让 outbox 在满环后仍有待发记录
- 期望:超额写入返回失败且已存数据不变,前缀变化与未排空输出都明确失败
- 实现:`crates/hft-testkit/src/dst/seq/advisor.rs::tk_154_model_buffers_and_prefix_changes_fail_explicitly`

### TK-155 部分成交由真实撮合产生且窗口有界

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:table
- 里程碑:M2
- 前置:W1 的 MockVenue 与真实 SimExchange,现货计价币/基础币费用、永续费用,买卖两个方向
- 步骤:限制撮合可见量为一个 lot,向 IOC/FOK 提交十个 lot
- 期望:IOC 恰成交一个 lot 后 Expired,完整身份保留;费用、仓位、库存、钱包与永续现金覆盖按输入独立算出并逐单位相等;FOK 零成交且资金不变。窗口与异常配置分别归 TK-158/159
- 实现:`crates/hft-testkit/tests/partial_fills.rs::tk_155_partial_fills_use_matcher_fees_cash_and_ioc_fok_rules`

| 行 | 输入 | 期望 |
|---|---|---|
| TK-155.1 | 现货计价币手续费,买/卖 × IOC/FOK | IOC 一个 lot 的原始成交、费用与钱包逐单位相等;FOK 零成交 |
| TK-155.2 | 现货基础币手续费,买/卖 × IOC/FOK | IOC 一个 lot,Configured(BTC) 按输入 BBO 中间价换算一次向上取整,买扣 2、卖扣 1 个最小单位;FOK 不扣费 |
| TK-155.3 | 永续计价币手续费,买/卖 × IOC/FOK | IOC 一个 lot 的仓位、费用与零开仓现金相等;FOK 零成交 |

### TK-156 部分成交与晚到基础币手续费的真实引擎验收

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:integ
- 里程碑:M2
- 前置:固定种子 1,每行 10,000 步,基础币手续费延迟 50/6000 ms,现货部分成交窗口 5000 ms
- 步骤:真实引擎与 SimExchange 运行,开关检查器分别执行同一世界
- 期望:实际出现一个 lot 的 PartialFill 与非零 BTC FeeOnly,窗口后实际恢复大于一个 lot 的完整成交;全部不变量通过且 INV-16 保持钱包严格相等,快照与完整 journal 回放实际执行;检查器开关后的完整 journal、交付、意图、HashTrail 和最终交易所真值逐字节相同
- 实现:`crates/hft-testkit/tests/partial_fills.rs::tk_156_real_partial_fee_windows_keep_wallets_replay_and_checkers_equal`

### TK-157 部分成交接通前后保留完整金样迁移证据

- 规则:[DST 的世界模型](README.md#dst-的世界模型)、[HashTrail](README.md#7-状态哈希对比)
- 级别:integ
- 里程碑:M2
- 前置:`fixtures/dst/partial-before.toml` 固定提交 4244872 的 50 个种子、套件、每行 2000 步及摘要
- 步骤:只将 PartialFills 时长置零,保留计划条目、顺序与随机流,重现旧摘要;再运行正常故障并关闭全部检查器复跑
- 期望:旧摘要逐行相等,至少一行轨迹变化且首条差异不早于首次部分成交故障,此前 journal 前缀逐字节相同;全部运行达到原步数并通过不变量,检查器开关的完整 journal、交付、意图、HashTrail 与交易所真值相同
- 证据:CI 保存 `target/dst/partial-fill-migration.json` 的原提交、新旧摘要、差异边界、实际部分成交数量及逐行比较结果,核对后才更新当前金样
- 实现:`crates/hft-testkit/tests/partial_migration.rs::tk_157_partial_migration_reproduces_prior_goldens_and_preserves_prefix`

### TK-158 部分成交窗口按最新盘口和实际到所时刻结束

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:W1 真实 SimExchange,入所延迟 1 ms
- 步骤:100 ms 窗口叠加从 50 ms 开始的 150 ms 窗口;期间两次更新 BBO;199.5 ms 发单,结束后不再提供行情
- 期望:120 ms 仍只成交一个 lot,窗口在 200 ms 才结束;引擎收到原深度,199.5 ms 收集不提前撮合,200.5 ms 到所的订单按最近报价全成
- 实现:`crates/hft-testkit/tests/partial_fills.rs::tk_158_partial_window_restores_latest_book_and_respects_arrival_boundary`

### TK-159 部分成交空窗口与非法配置明确处理

- 规则:[DST 的世界模型](README.md#dst-的世界模型)
- 级别:unit
- 里程碑:M2
- 前置:W1 的 MockVenue,买入 IOC 十个 lot
- 步骤:分别使用 clean、零时长、非正 lot 与结束时刻溢出配置
- 期望:clean 与零时长不产生 PartialFill,原订单全成;非正 lot 和时刻溢出分别返回明确错误
- 实现:`crates/hft-testkit/tests/partial_fills.rs::tk_159_partial_clean_zero_and_invalid_windows_are_explicit`

## 验收

- 本组件完成 = 里程碑已到的全部用例有同编号的实现且通过(`cargo xtask test-trace`)。
- 框架可信度:TK-053 的每一行都在 2 000 个种子内被找到,TK-051 的每一行都缩到 ≤ 5 项;任一行未达成即框架验收不通过。
- 跨架构一致:版本标签作业的 TK-054 通过(不变量 9,[testing §6](../../docs/testing.md#6-ci-运行矩阵))。
- 运行时间预算(PR 冒烟、每夜、版本标签、`scaffold-gate`、全部金样、单步开销)以 [延迟与资源契约](README.md#延迟与资源契约) 的表为准,由 CI 作业时长与 TK-092、TK-093 的基准输出给出证据;服务器累计的门槛 ④ 由 hft-verify 账本给出([testing §6.1](../../docs/testing.md#61-服务器上的累计运行))。
