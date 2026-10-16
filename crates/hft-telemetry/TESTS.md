---
component: hft-telemetry
prefix: TEL
spec: README.md
---

# hft-telemetry 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。分段名、公式、插值与误差界归 [latency-budget](../../docs/latency-budget.md);分层、上下界、加权合成与同机干扰的阈值归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法);用例在 `前置` 中显式给出所用的取值。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | TEL-001、TEL-020、TEL-046、TEL-082 |
| 输入:时间点 | TEL-002–TEL-003 |
| 分段(`StageSet`) | TEL-002–TEL-018、TEL-080 |
| 按事件加权的离线统计(`event_weighted`) | TEL-025–TEL-038、TEL-084 |
| 账户报告(`account_report`) | TEL-039–TEL-045、TEL-083 |
| 策略报告(`strategy_report`) | TEL-046–TEL-052 |
| 告警判定所需的派生量 | TEL-053–TEL-059 |
| 保证金与账户指标 | TEL-060 |
| 实现说明 | 不适用:见子节 |
| 直方图 | TEL-009、TEL-020–TEL-024 |
| 时钟校准序列 | TEL-011–TEL-016、TEL-019 |
| 计数器(由事件派生,冷侧计算) | TEL-061 |
| Note 翻译 | TEL-062–TEL-065 |
| 规则判定函数 | TEL-066–TEL-079、TEL-091–TEL-092 |
| 规划源文件 | TEL-024 |
| 延迟与资源契约 | TEL-085–TEL-090 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | TEL-081 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

约定:"标准校准"指两条 `ClockStatus`:a = (tsc 1 000 000 000, realtime/phc 5 000 000 000 000 ns),b = (tsc 2 000 000 000, realtime/phc 5 000 001 000 000 ns),`tsc_hz = 1 000 000 000`,两端 `calib_err_ns = 100`、`max_error_ns = 1 000`、`state = Synced`,前一区间速率与本区间相同。记 T0 = 1 000 000 000(tsc),P0 = 5 000 000 000 000(ns)。"标准样本"指行情触发的 `LatencySample`(`trigger_kind = Bbo`、`action = New`、`link_role = 0`):`ex_ts = P0 − 2 000 000`、`ex_send = P0 − 1 000 000`、`hw_rx = P0 + 400 000`(HW)、`sw_rx = P0 + 405 000`、`rx_tsc = T0 + 412 000`、`proc_tsc = T0 + 415 000`、`decided_tsc = T0 + 420 000`、`sent_tsc = T0 + 428 000`、`ack_hw_rx = P0 + 1 428 000`。

### TEL-001 TelemetryConfig 的缺省值

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M1.5
- 前置:`TelemetryConfig::default()`(`max_series`、`cal_max_err_ns`、`naked_usd` 由调用方给,不在缺省中)
- 步骤:
  1. 读取各字段
- 期望:
  - `window_s = 10`、`sig_digits = 3`、`min_ns = 1`、`max_ns = 10_000_000_000`、`clock_untrusted_offset_ns = 200_000`
- 实现:`crates/hft-telemetry/tests/config.rs::tel_001_defaults`

### TEL-002 标准样本的每个分段等于公式值

- 规则:[分段(`StageSet`)](README.md#分段stageset)、[输入:时间点](README.md#输入时间点)
- 级别:table
- 里程碑:M1.5
- 前置:标准校准;标准样本;`cal_max_err_ns = 1000`;`Telemetry::stages(&sample, &cal)`
- 实现:`crates/hft-telemetry/tests/stages.rs::tel_002_formula_values`

| 行 | 分段 | 期望(ns) |
|---|---|---|
| TEL-002.1 | `ex_queue` | 1 000 000 |
| TEL-002.2 | `wire_in` | 1 400 000 |
| TEL-002.3 | `feed_oneway` | 2 400 000 |
| TEL-002.4 | `kernel_rx` | 12 000 |
| TEL-002.5 | `nic_to_kernel` | 5 000 |
| TEL-002.6 | `kernel_to_user` | 7 000 |
| TEL-002.7 | `ingest` | 3 000 |
| TEL-002.8 | `feed_host` | 15 000 |
| TEL-002.9 | `decide` | 5 000 |
| TEL-002.10 | `egress` | 8 000 |
| TEL-002.11 | `tick_to_trade` | 28 000(= `kernel_rx` + (`sent_tsc` − `rx_tsc`)) |
| TEL-002.12 | `order_rtt` | 1 000 000 |
| TEL-002.13 | `tick_to_ack` | 1 028 000 |
| TEL-002.14 | `err_bound_ns`、`cal_ok` | 100、true |

### TEL-003 按触发事件分类产出分段

- 规则:[分段(`StageSet`)](README.md#分段stageset)、[输入:时间点](README.md#输入时间点)
- 级别:table
- 里程碑:M1.5
- 前置:标准校准;以标准样本为底,按行改触发字段;v2 记录的 `trigger_seq` 指向的记录头 kind 由夹具 journal 给出
- 实现:`crates/hft-telemetry/tests/stages.rs::tel_003_trigger_classification`

| 行 | 触发 | 期望 |
|---|---|---|
| TEL-003.1 | `trigger_kind = Bbo` | 有 `tick_to_trade`;无 `rep_to_order`、`hedge_host` |
| TEL-003.2 | `trigger_kind = OrderEvent`,`link_role = 0` | 有 `rep_to_order` = 28 000;无 `tick_to_trade`、`hedge_host` |
| TEL-003.3 | `trigger_kind = OrderEvent`,`link_role = Second(2)` | 有 `rep_to_order` 与 `hedge_host`,两者都是 28 000 |
| TEL-003.4 | `trigger_kind = OrderEvent`,`link_role = SecondRetry(3)` | 有 `rep_to_order`;无 `hedge_host` |
| TEL-003.5 | `trigger_kind = Timer` | 只有 `decide`、`egress`、`order_rtt`;所有含 `hw_rx` 的分段为 `None` |
| TEL-003.6 | v2 记录:`trigger_kind = 0`,`trigger_seq` 指向 `Bbo` 记录 | 结果与 TEL-003.1 逐字段相同 |
| TEL-003.7 | v2 记录:`trigger_kind = 0`,`trigger_seq` 指向 `OrderEvent` 记录 | 结果与 TEL-003.2 逐字段相同 |

### TEL-004 order_rtt 只取新单,rep_to_order 含三种 action

- 规则:[分段(`StageSet`)](README.md#分段stageset)
- 级别:table
- 里程碑:M1.5
- 前置:标准校准;同一订单三条样本,均为回报触发(`trigger_kind = OrderEvent`),`action` 分别为 0/1/2
- 实现:`crates/hft-telemetry/tests/stages.rs::tel_004_action_sampling`

| 行 | `action` | 期望 |
|---|---|---|
| TEL-004.1 | 0(New) | 有 `order_rtt` 与 `rep_to_order` |
| TEL-004.2 | 1(Cancel) | 无 `order_rtt`;有 `rep_to_order` |
| TEL-004.3 | 2(Amend) | 无 `order_rtt`;有 `rep_to_order` |
| TEL-004.4 | 三条喂入 `Telemetry` 后的窗口 | `rep_to_order` 的 count = 3,`order_rtt` 的 count = 1;`HistKey.action` 有三个不同取值 |

### TEL-005 rep_to_order 按回报种类分组

- 规则:[分段(`StageSet`)](README.md#分段stageset)
- 级别:unit
- 里程碑:M1.5
- 前置:标准校准;回报触发样本三条,触发回报种类分别为 Ack、Fill、Cancel
- 步骤:
  1. 喂入后取窗口报告
- 期望:
  - `rep_to_order` 出现在 3 个不同的 `HistKey` 中,各 count = 1
- 实现:`crates/hft-telemetry/tests/stages.rs::tel_005_rep_to_order_by_report_kind`

### TEL-006 sw_rx 拆分的两段之和等于 kernel_rx,缺 sw_rx 时计 missing

- 规则:[分段(`StageSet`)](README.md#分段stageset)
- 级别:table
- 里程碑:M1.5
- 前置:标准校准;标准样本
- 实现:`crates/hft-telemetry/tests/stages.rs::tel_006_sw_rx_split`

| 行 | `sw_rx` | 期望 |
|---|---|---|
| TEL-006.1 | P0 + 405 000 | `nic_to_kernel + kernel_to_user = kernel_rx`(5 000 + 7 000 = 12 000) |
| TEL-006.2 | 0 | 两个诊断分段为 `None`;窗口计数器 `missing`(stage = `nic_to_kernel`)+1、(stage = `kernel_to_user`)+1;不填 0 |

### TEL-007 tx_sw 未打开时 kernel_tx 不产出

- 规则:[分段(`StageSet`)](README.md#分段stageset)
- 级别:unit
- 里程碑:M1.5
- 前置:标准校准;标准样本;journal 中没有 housekeeping 读回的 `tx_sw` 记录
- 步骤:
  1. `stages(&sample, &cal)`
- 期望:
  - `kernel_tx = None`,且不计入 `missing`
- 实现:`crates/hft-telemetry/tests/stages.rs::tel_007_kernel_tx_off`

### TEL-008 leg_gap 把第二腿样本与第一腿成交记录的 proc_tsc 配对

- 规则:[分段(`StageSet`)](README.md#分段stageset)
- 级别:unit
- 里程碑:M1.5
- 前置:第一腿成交 `OrderEvent` 记录头 `tsc = T0 + 500 000`;第二腿 `LatencySample`:`trigger_seq` 指向该记录、`sent_tsc = T0 + 530 000`、`link_role = Second`
- 步骤:
  1. 喂入 `Telemetry`
- 期望:
  - `leg_gap` 样本 = 30 000 ns
- 实现:`crates/hft-telemetry/tests/stages.rs::tel_008_leg_gap_pairing`

### TEL-009 端点缺失计 missing、负值计 negative、零值照常记录

- 规则:[分段(`StageSet`)](README.md#分段stageset)、[直方图](README.md#直方图)
- 级别:table
- 里程碑:M1.5
- 前置:标准校准;以标准样本为底按行改
- 实现:`crates/hft-telemetry/tests/stages.rs::tel_009_missing_negative_zero`

| 行 | 改动 | 期望 |
|---|---|---|
| TEL-009.1 | `hw_rx` 的来源位 = 缺失(值 0) | `kernel_rx`、`feed_host`、`tick_to_trade` 为 `None`;各自 `missing` +1 |
| TEL-009.2 | `proc_tsc = T0 + 411 999`(早于 `rx_tsc` 1 ns) | `ingest = −1`:不进直方图,`negative` +1 |
| TEL-009.3 | `proc_tsc = T0 + 412 000` | `ingest = 0`:进直方图,count +1 |

### TEL-010 Sim 账户的样本不进延迟直方图,单独成数据集

- 规则:[分段(`StageSet`)](README.md#分段stageset)
- 级别:unit
- 里程碑:M4
- 前置:标准校准;两条标准样本,账户分别为 Live 账户 3 与 Sim 账户 4(`AccountMeta.kind = Sim`)
- 步骤:
  1. 喂入后取窗口报告
- 期望:
  - `account_kind = Live` 的 `order_rtt` count = 1;Sim 的样本只出现在 `account_kind = Sim` 的数据集,不在 Live 数据集的任何 `HistKey` 中
- 实现:`crates/hft-telemetry/tests/stages.rs::tel_010_sim_samples_separate`

### TEL-011 按前后两条校准插值,而阶梯取值会有半微秒误差

- 规则:[时钟校准序列](README.md#时钟校准序列)、[分段(`StageSet`)](README.md#分段stageset)
- 级别:unit
- 里程碑:M1.5
- 前置:合成校准序列:TSC 相对 PHC 快 0.5 ppm,`ClockStatus` 每 1 000 ms 一条(读数无噪声);真值 `kernel_rx` = 12 000 ns 的样本放在两条校准正中间
- 步骤:
  1. `stages` 计算 `kernel_rx`
  2. 测试内以"最近一条 `ClockStatus`"做阶梯换算对照
- 期望:
  - 步骤 1 与真值之差 < 50 ns
  - 步骤 2 与真值之差 ≥ 200 ns(区间正中 0.5 ppm × 0.5 s = 250 ns)
- 实现:`crates/hft-telemetry/tests/clock.rs::tel_011_interpolation_vs_step`

### TEL-012 逐样本误差界按 latency-budget §2.1 的公式

- 规则:[时钟校准序列](README.md#时钟校准序列)
- 级别:unit
- 里程碑:M1.5
- 前置:三条 `ClockStatus` prev、a、b 间隔各 1 s;prev→a 区间速率 1.0000000、a→b 区间速率 1.0000005;a 的 `calib_err_ns = 100`,b 的 = 300;`cal_max_err_ns = 1000`
- 步骤:
  1. 对落在 [a, b] 内的样本计算误差界
- 期望:
  - `err_bound_ns = 550`(max(100, 300) + 0.5 ppm × 1 s / 2);`cal_ok = true`
- 实现:`crates/hft-telemetry/tests/clock.rs::tel_012_error_bound_formula`

### TEL-013 误差界超过阈值的样本计入 unsynced_cal,误差仍进分布

- 规则:[时钟校准序列](README.md#时钟校准序列)、[分段(`StageSet`)](README.md#分段stageset)
- 级别:table
- 里程碑:M1.5
- 前置:标准校准但按行改 a 的 `calib_err_ns`(b = 0,两区间速率相同);`cal_max_err_ns = 1000`
- 实现:`crates/hft-telemetry/tests/clock.rs::tel_013_cal_threshold`

| 行 | a 的 `calib_err_ns` | 期望 |
|---|---|---|
| TEL-013.1 | 999 | `cal_ok = true`;进分位数 |
| TEL-013.2 | 1000 | `cal_ok = true`;进分位数 |
| TEL-013.3 | 1001 | `cal_ok = false`;含 PHC 端点的分段不进分位数,`unsynced_cal` +1;误差界分布直方图 count +1 |
| TEL-013.4 | 1500(README 示例) | 同 TEL-013.3 |

### TEL-014 缺少后一条 ClockStatus 的样本计入 unsynced_cal

- 规则:[时钟校准序列](README.md#时钟校准序列)
- 级别:unit
- 里程碑:M1.5
- 前置:只有 a;一条落在 a 之后的标准样本;随后 journal 结束(进程退出)
- 步骤:
  1. 喂入后调用 `finish()`(journal 结束),再调用 `day_close`
- 期望:
  - 该样本在 b 到达前不出数;结束时计入 `unsynced_cal`,不进任何含 PHC 端点的分位数
- 实现:`crates/hft-telemetry/tests/clock.rs::tel_014_missing_next_clockstatus`

### TEL-015 区间端点为 Stepped 的样本计入 unsynced_cal

- 规则:[时钟校准序列](README.md#时钟校准序列)
- 级别:unit
- 里程碑:M1.5
- 前置:标准校准,但 b 的 `state = Stepped`
- 步骤:
  1. 计算标准样本
- 期望:
  - `cal_ok = false`;`unsynced_cal` +1;纯 TSC 分段(`ingest`、`decide`、`egress`)照常进分位数
- 实现:`crates/hft-telemetry/tests/clock.rs::tel_015_stepped_endpoint`

### TEL-016 SW_TS 样本以 sw_rx 为锚并落入 SW 数据集

- 规则:[时钟校准序列](README.md#时钟校准序列)、[分段(`StageSet`)](README.md#分段stageset)
- 级别:unit
- 里程碑:M1.5
- 前置:标准校准;标准样本的 `src` 中 `hw_rx` 为 SW(2,记录带 `SW_TS`),`hw_rx = P0 + 405 000`(内核软件戳),`sw_rx = P0 + 405 000`
- 步骤:
  1. 喂入后取窗口报告
- 期望:
  - `kernel_rx` = `rx_tsc` − `sw_rx` = 7 000 ns(以 TSC↔REALTIME 插值);该样本的 `HistKey.ts_src = Sw`;HW 数据集中没有它
- 实现:`crates/hft-telemetry/tests/clock.rs::tel_016_sw_anchor`

### TEL-017 交易所时钟偏移界超过 200 µs 时含交易所时钟的分段计 unsynced

- 规则:[分段(`StageSet`)](README.md#分段stageset)
- 级别:table
- 里程碑:M1.5
- 前置:标准校准,按行改 a 与 b 的 `max_error_ns`;`clock_untrusted_offset_ns = 200_000`
- 实现:`crates/hft-telemetry/tests/clock.rs::tel_017_exchange_clock_trust`

| 行 | `max_error_ns`(a, b) | 期望 |
|---|---|---|
| TEL-017.1 | 200 000, 200 000 | `ex_queue`、`wire_in`、`feed_oneway` 进分位数;窗口 `clock_trust` 可信 |
| TEL-017.2 | 200 001, 200 000 | 三个分段不进分位数,`unsynced` +1 各一次;窗口 `clock_trust` 不可信;`kernel_rx` 等主机内分段照常 |

### TEL-018 TSC 到 ns 的换算误差小于 1 ns/ms

- 规则:[分段(`StageSet`)](README.md#分段stageset)
- 级别:prop
- 里程碑:M1.5
- 性质:纯 TSC 分段的换算值与 i128 精确换算 `Δtsc × 1e9 / tsc_hz` 之差 < 1 ns/ms(按分段长度计)
- 生成:`tsc_hz` ∈ [1e8, 5e9](含非整除值如 2 999 999 999),Δtsc ∈ [0, 10 s × tsc_hz]
- 参考:i128 精确计算
- 例数:PR 10 000
- 实现:`crates/hft-telemetry/tests/clock.rs::tel_018_tsc_conversion_precision`

### TEL-019 校准队列不超过 4 条

- 规则:[时钟校准序列](README.md#时钟校准序列)
- 级别:unit
- 里程碑:M1.5
- 前置:只喂 `ClockStatus`(10 条),不喂任何样本
- 步骤:
  1. 读校准队列长度
- 期望:
  - 队列长度 ≤ 4,保留的是最后几条
- 实现:`crates/hft-telemetry/tests/clock.rs::tel_019_queue_bound`

### TEL-020 窗口按 UTC 对齐并在终点返回

- 规则:[直方图](README.md#直方图)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M1.5
- 前置:`window_s = 10`;第一次 `ingest` 后首次调用 `window(now)` 时 `now = 1_700_000_003_000_000_000`
- 实现:`crates/hft-telemetry/tests/window.rs::tel_020_window_alignment`

| 行 | 调用 | 期望 |
|---|---|---|
| TEL-020.1 | `window(1_700_000_003_000_000_000)` | `None`(当前窗口 [1_700_000_000, 1_700_000_010) s) |
| TEL-020.2 | `window(1_700_000_009_999_999_999)` | `None` |
| TEL-020.3 | `window(1_700_000_010_000_000_000)` | `Some`:`start_ns = 1_700_000_000_000_000_000`、`end_ns = 1_700_000_010_000_000_000` |
| TEL-020.4 | 紧接着再次 `window(1_700_000_010_000_000_000)` | `None`;新窗口的直方图为空 |

### TEL-021 窗口清零而按日累计保留

- 规则:[直方图](README.md#直方图)
- 级别:unit
- 里程碑:M1.5
- 前置:三个窗口各喂 10 个 `tick_to_trade` 样本
- 步骤:
  1. 三次窗口滚动后 `day_close(19_675)`
- 期望:
  - 每个 `WindowReport` 的该键 count = 10;按日累计的 count = 30
- 实现:`crates/hft-telemetry/tests/window.rs::tel_021_daily_accumulates`

### TEL-022 维度键超过 max_series 时并入 other 并计数

- 规则:[直方图](README.md#直方图)
- 级别:unit
- 里程碑:M1.5
- 前置:`max_series = 3`;四个不同 `HistKey` 的样本依次到达
- 步骤:
  1. 喂入后取窗口报告
- 期望:
  - 前 3 个键各自成序列;第 4 个键的样本计入 `other`;计数器 `series_overflow` = 1;`HistKey` 中没有 inst 维度
- 实现:`crates/hft-telemetry/tests/hist.rs::tel_022_series_overflow`

### TEL-023 不同数据集不合并

- 规则:[直方图](README.md#直方图)
- 级别:table
- 里程碑:M1.5
- 前置:同一分段同一 venue 的样本,按行的维度不同
- 实现:`crates/hft-telemetry/tests/hist.rs::tel_023_datasets_separate`

| 行 | 两组样本的差别 | 期望 |
|---|---|---|
| TEL-023.1 | `ts_src` HW 与 SW | 两个序列,各自 count |
| TEL-023.2 | `dataset` 实盘与验证回放 | 两个序列 |
| TEL-023.3 | `dataset` 实盘与按事件加权的离线统计 | 两个序列 |
| TEL-023.4 | `account_kind` Live 与 Sim | 两个序列 |

### TEL-024 WindowReport 导出为 Prometheus 文本

- 规则:[规划源文件](README.md#规划源文件)、[直方图](README.md#直方图)
- 级别:golden
- 里程碑:M5
- 输入:一份固定的 `WindowReport`(2 个直方图键、`unsynced = 3`、一个 gauge `margin_ratio`)
- 期望文件:`crates/hft-telemetry/tests/fixtures/export/window.prom`
- 实现:`crates/hft-telemetry/tests/export.rs::tel_024_prometheus_text`

### TEL-025 一次读出的多条行情记录各计一个样本并都属突发层

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:unit
- 里程碑:M1.5
- 前置:夹具 seq journal:标准校准;同一连接 (venue 1, conn 0) 上 3 条 `Bbo` 共享 `hw_rx = P0 + 400 000`、`rx_tsc = T0 + 412 000`、`rx_empty_gap_ns = 20 000`,记录头 `tsc` 分别为 T0 + 415 000、+416 000、+417 000;`burst_gap_ns = 50 000`;`cal_max_err_ns = 1000`
- 步骤:
  1. `event_weighted(&mut reader, &rd, &cfg)`
- 期望:
  - 返回 1 份报告;`n = 3`;`kernel_rx` 键(`stratum = Burst`)count = 3,三个样本都是 12 000 ns;`ingest` 三个样本 3 000、4 000、5 000 ns;`Calm` 层没有样本
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_025_one_read_three_samples`

### TEL-026 stratum 的分层边界

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:table
- 里程碑:M1.5
- 前置:`burst_gap_ns = 50_000`;判定规则归 testing §8.2 第 9 条
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_026_stratum_boundaries`

| 行 | `frames_in_read`, `rx_empty_gap_ns` | 期望 |
|---|---|---|
| TEL-026.1 | 1, 50 000 | `Calm` |
| TEL-026.2 | 1, 49 999 | `Calm` |
| TEL-026.3 | 1, 50 001 | `Burst` |
| TEL-026.4 | 2, 0 | `Burst` |
| TEL-026.5 | 1, 0 | `Calm` |
| TEL-026.6 | 3, 10 | `Burst` |

### TEL-027 kernel_rx 上界等于空队列间隔加网卡到软中断一段

非零接收戳无来源标志或同时带 HW/SW 时，不进入 HW/SW 分段；独立的 ingest/kernel_to_user 保留在 Missing 数据集。

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:table
- 里程碑:M1.5
- 前置:标准校准;单条 `Bbo`:`hw_rx = P0 + 400 000`、`sw_rx = P0 + 405 000`、`rx_tsc = T0 + 412 000`;`rx_empty_gap_ns` 按行;公式归 testing §8.2 第 9 条
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_027_kernel_rx_hi`

| 行 | 条件 | 期望 |
|---|---|---|
| TEL-027.1 | HW,`rx_empty_gap_ns = 30 000` | `kernel_rx_hi` 样本 = 35 000(30 000 + 5 000);≥ 同一记录 `kernel_rx` 12 000 |
| TEL-027.2 | HW,`rx_empty_gap_ns = 7 000` | `kernel_rx_hi` = 12 000(等于 `kernel_rx`) |
| TEL-027.3 | SW(`SW_TS`),`rx_empty_gap_ns = 30 000` | `kernel_rx_hi` 样本 = 30 000,键为 SW 数据集,报告注明不含网卡到软中断一段 |
| TEL-027.4 | 任一行 | `kernel_rx_hi` 的每个键 `stage = kernel_rx` |

### TEL-028 空队列间隔为 0 的记录不出上界并计入 gap_missing

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:unit
- 里程碑:M1.5
- 前置:同 TEL-027,`rx_empty_gap_ns = 0`
- 步骤:
  1. `event_weighted`
- 期望:
  - `gap_missing = 1`;`kernel_rx_hi` 中没有样本;`kernel_rx` 照常 = 12 000
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_028_gap_missing`

### TEL-029 结果与逐条手算一致

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:unit
- 里程碑:M1.5
- 前置:标准校准;5 条行情记录(2 条 `Bbo`、1 条 `Trade`、1 条 `Mark`、1 条 `Funding`),各自的四个时间戳已知
- 步骤:
  1. `event_weighted`
  2. 测试内逐条按 latency-budget §2 的公式手算 `kernel_rx`、`ingest`、`feed_host`
- 期望:
  - 每个 (kind, stage) 键的样本集合与手算逐值相同;`n = 5`
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_029_matches_manual`

### TEL-030 段首 HostProfile 变化时分成两个数据集

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:unit
- 里程碑:M1.5
- 前置:journal 两段,段 1 首部 `HostProfile` 哈希 H1(10 条行情),段 2 首部 H2(7 条行情)
- 步骤:
  1. `event_weighted`
- 期望:
  - 返回 2 份报告,`host_profile` 依次为 H1、H2,`n` 依次为 10、7;没有合并的直方图
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_030_hostprofile_split`

### TEL-031 同一 journal 跑两次结果逐位相同

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:unit
- 里程碑:M1.5
- 前置:TEL-029 的 journal
- 步骤:
  1. 运行 `event_weighted` 两次并序列化
- 期望:
  - 两份序列化字节相同
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_031_deterministic`

### TEL-032 只读 journal,不写任何文件

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:unit
- 里程碑:M1.5
- 前置:TEL-029 的 journal 放在只读目录;文件系统调用记录替身
- 步骤:
  1. `event_weighted`
- 期望:
  - 返回 `Ok`;没有任何以写方式打开的文件;journal 文件的 SHA-256 不变
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_032_read_only`

### TEL-033 只统计带 MarketHdr 的行情记录

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:table
- 里程碑:M1.5
- 前置:journal 含各行的记录各 1 条
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_033_record_kinds`

| 行 | 记录 | 期望 |
|---|---|---|
| TEL-033.1 | `Bbo`、`Trade`、`Mark`、`Index`、`Funding`、`L2Snapshot` | 各计入 `n`,各自 kind 的键 |
| TEL-033.2 | `OrderEvent`、`Timer`、`LatencySample` | 不计入 `n` |

### TEL-034 按层给出 unsynced_cal 与总数

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:unit
- 里程碑:M1.5
- 前置:10 条记录:Burst 4 条(其中 2 条的校准误差界 > `cal_max_err_ns`)、Calm 6 条(其中 1 条超界);`cal_max_err_ns = 1000`
- 步骤:
  1. `event_weighted`
- 期望:
  - `unsynced_cal = 3`;`unsynced_cal_by_stratum = [(Burst, 2, 4), (Calm, 1, 6)]`;`cal_err` 分布 count = 10
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_034_unsynced_by_stratum`

### TEL-035 只统计 from_seq 到 to_seq 之间的记录

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:unit
- 里程碑:M1.5
- 前置:TEL-029 的 journal,行情记录 seq 为 10、11、12、13、14
- 步骤:
  1. `cfg.from_seq = 11`、`cfg.to_seq = 13`
- 期望:
  - `n = 3`
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_035_seq_range`

### TEL-036 两层样本按事件加权合成分位数

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:unit
- 里程碑:M1.5
- 前置:同一窗口 `event_weighted` 的记录占比 Burst 0.2、Calm 0.8;行情触发的探针样本:Calm 层 10 000 个,`tick_to_trade` 取值 1 000、2 000、…、10 000 000 ns(等差);Burst 层 10 000 个,取值 10 001 000、…、20 000 000 ns;合成规则归 testing §8.2 第 9 条
- 步骤:
  1. `stratified_probe(&mut reader, &rd, &cfg, &ew)`
- 期望:
  - 该组 `weight = [0.2, 0.8]`;`composite_p50_p99 = (6 250 000, 19 500 000)`(各在 hdrhistogram 3 位有效数字的等价范围内,即相对误差 ≤ 0.1%)
  - `by_stratum` 两层各有 p50/p99
- 实现:`crates/hft-telemetry/tests/stratified.rs::tel_036_composite`

### TEL-037 某层样本不足时不出合成值

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:table
- 里程碑:M1.5
- 前置:同 TEL-036,按行改 Burst 层样本数;样本量下限归 latency-budget §5(p99 需 n ≥ 1 000)
- 实现:`crates/hft-telemetry/tests/event_weighted.rs::tel_037_insufficient_stratum`

| 行 | Burst 层样本数 | 期望 |
|---|---|---|
| TEL-037.1 | 1 000 | `composite_p50_p99` 有值 |
| TEL-037.2 | 999 | `composite_p50_p99 = None`;Burst 层的 p99 为 `None`(n < 1 000),Calm 层照常给出 |

### TEL-038 行情触发的探针样本取其触发记录的层

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)
- 级别:unit
- 里程碑:M1.5
- 前置:两条行情记录 R1(单帧、`rx_empty_gap_ns = 10`,Calm)、R2(与另一条共享 `rx_tsc`,Burst);两条 `LatencySample` 的 `trigger_seq` 分别指向 R1、R2;`burst_gap_ns = 50 000`
- 步骤:
  1. `stratified_probe`
- 期望:
  - 指向 R1 的样本计入 Calm 层(`n = [1(Burst), 1(Calm)]` 中的 Calm),指向 R2 的计入 Burst 层
- 实现:`crates/hft-telemetry/tests/stratified.rs::tel_038_trigger_stratum`

### TEL-039 账户报告手算小例

- 规则:[账户报告(`account_report`)](README.md#账户报告account_report)
- 级别:unit
- 里程碑:M4
- 前置:Live 账户 3,group G(现货腿 S、永续腿 P,资产 AIN);`naked_usd = 5`;事件流(UTC 日 D):t = 0 `OrderDecision` 买 S 10 AIN(IOC,下单量 10),此刻 S 中间价 1.000;t = 0.1 s 成交 10 @ 1.001,费用 0.01 AIN(基础币扣费);t = 0.1 s `OrderDecision` 卖 P 10 AIN(IOC,下单量 10),此刻 P 中间价 1.000;t = 2.1 s P 成交 10 @ 0.999;t = 3 600 s `FundingSettle` −0.50 USDT;t = 7 200 s 一笔强平 `ExternalFill`;t = 7 300 s 一次 `AdjustDelta::Funding` +0.10 USDT
- 步骤:
  1. 喂入后 `day_close(D)`
- 期望:
  - `slippage_cost` = 0.010 + 0.010 = 0.020 USDT
  - `fill_rate` = 20 / 20 = 1.0;`turnover` = 10 × 1.001 + 10 × 0.999 = 20.000 USDT
  - `naked_over_s` = 2.0(t = 0.1 s 到 2.1 s 两腿差 10 AIN,名义 > 5);`naked_max` 等于 hft-ledger 对同一事件流的每 group 名义差最大值,且在 [9.99, 10.01] USDT 内
  - `pnl_fee` = −0.01001 USDT(0.01 AIN × 1.001);`pnl_total`、`pnl_funding`、`pnl_basis`、`pnl_directional` 与以 hft-ledger 记账代码对同一事件流重建的 `PnlBreakdown` 逐字段相等
- 实现:`crates/hft-telemetry/tests/account_report.rs::tel_039_hand_example`

### TEL-040 Sim 与 Live 同口径

- 规则:[账户报告(`account_report`)](README.md#账户报告account_report)
- 级别:unit
- 里程碑:M4
- 前置:TEL-039 的事件流,分别以账户种类 Live 与 Sim 各喂一次(Sim 的 `leg_gap` 来源按 Sim 规则另给成交与 `SendReport`)
- 步骤:
  1. 两次 `day_close(D)`
- 期望:
  - 两份 `AccountDaily` 除 `account_kind`、`leg_gap_*` 与 Sim 专有计数外逐字段相等
- 实现:`crates/hft-telemetry/tests/account_report.rs::tel_040_sim_live_same`

### TEL-041 margin_* 取 MarginReport 与 ledger 估算

- 规则:[账户报告(`account_report`)](README.md#账户报告account_report)
- 级别:unit
- 里程碑:M4
- 前置:当日三条 `MarginReport`:保证金率 0.10、0.25、0.15;初始保证金 / 保证金余额依次 0.2、0.4、0.3(各持续相同时长);最小到强平距离按"保证金与账户指标"的 `liq_dist` 口径为 0.12
- 步骤:
  1. `day_close(D)`
- 期望:
  - `margin_ratio_max = 0.25`;`margin_usage_avg = 0.3`;报告中的最小 `liq_dist = 0.12`
- 实现:`crates/hft-telemetry/tests/account_report.rs::tel_041_margin_fields`

### TEL-042 leg_gap 实盘取 LatencySample,Sim 取成交与第二腿 SendReport

- 规则:[账户报告(`account_report`)](README.md#账户报告account_report)
- 级别:table
- 里程碑:M4
- 前置:当日两组两腿事件
- 实现:`crates/hft-telemetry/tests/account_report.rs::tel_042_leg_gap_sources`

| 行 | 账户与数据 | 期望 `leg_gap_p50/p99` 来源 |
|---|---|---|
| TEL-042.1 | Live:两条第二腿 `LatencySample`(`leg_gap` 30 µs、50 µs) | p50 = 30 µs、p99 = 50 µs(n < 1 000 时 p99 取最大值,按 latency-budget §5 标注) |
| TEL-042.2 | Sim:第一腿成交 → 第二腿 `SendReport` 间隔 1 ms、3 ms | p50 = 1 ms、p99 = 3 ms;不读任何 `LatencySample` |

### TEL-043 Sim 专有计数直接累加 sim-gw 的计数类 Note

- 规则:[账户报告(`account_report`)](README.md#账户报告account_report)
- 级别:unit
- 里程碑:M4
- 前置:Sim 账户当日 sim-gw 写 3 条 `top_only_fills` 计数 Note(增量 2、5、1)
- 步骤:
  1. `day_close(D)`
- 期望:
  - `top_only_fills = 8`
- 实现:`crates/hft-telemetry/tests/account_report.rs::tel_043_sim_counters`

### TEL-044 calib_pairs 差异表用同一函数逐字段相减

- 规则:[账户报告(`account_report`)](README.md#账户报告account_report)
- 级别:unit
- 里程碑:M4
- 前置:正式实盘账户 L 与同参数 Sim 账户 S 组成一对 `calib_pairs`;两者当日 `slippage_cost` 为 0.020 与 0.015
- 步骤:
  1. 生成差异表
- 期望:
  - 该对的 `slippage_cost` 差 = 0.005;差异表每个字段都等于两份 `AccountDaily` 的同名字段之差
- 实现:`crates/hft-telemetry/tests/account_report.rs::tel_044_calib_pairs`

### TEL-045 跨 group 共用库存不计入任何 group 的已对冲部分

- 规则:[账户报告(`account_report`)](README.md#账户报告account_report)
- 级别:unit
- 里程碑:M4
- 前置:账户持 unassigned 现货 3 AIN 与 group G 的两腿各 10 AIN
- 步骤:
  1. `day_close(D)`
- 期望:
  - G 的 `pnl_basis` 只按 10 AIN 计;3 AIN 的价格 PnL 在单列的 unassigned 行,不进任何 group 的 `pnl_basis`
- 实现:`crates/hft-telemetry/tests/account_report.rs::tel_045_unassigned_inventory`

### TEL-046 StrategyCatalog 加载时校验摘要

- 规则:[策略报告(`strategy_report`)](README.md#策略报告strategy_report)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M4
- 前置:hft-testkit 测试策略经 `hft-core schemas --out` 导出的目录
- 实现:`crates/hft-telemetry/tests/strategy_report.rs::tel_046_catalog_load`

| 行 | 目录 | 期望 |
|---|---|---|
| TEL-046.1 | 原样 | `Ok`;`note(kind_id, code)` 对每个登记的码返回描述符 |
| TEL-046.2 | `index.json` 中某项的摘要被改 | `Err(CatalogError::DigestMismatch)` |
| TEL-046.3 | `StrategyCatalog::empty()` | `note(任意, 任意) = None` |

### TEL-047 kind_id 取自 journal 的 StrategyParams 头

- 规则:[策略报告(`strategy_report`)](README.md#策略报告strategy_report)
- 级别:unit
- 里程碑:M4
- 前置:实例 pc-main 当日先后两份 `StrategyParams`(版本 41、42,`kind_id` 都是 65280);每份之后各有 2 条该实例的策略 Note
- 步骤:
  1. `day_close(D)`
- 期望:
  - 该实例的全部行 `kind_id = 65280`;Note 计数 = 4(不因参数更换拆行)
- 实现:`crates/hft-telemetry/tests/strategy_report.rs::tel_047_kind_id_from_params`

### TEL-048 策略 Note 按 (实例, group) 计数,未登记码计入 unregistered

- 规则:[策略报告(`strategy_report`)](README.md#策略报告strategy_report)
- 级别:unit
- 里程碑:M4
- 前置:描述符登记码 1 `edge`;当日 Note:code 1 × 3(group G1 两条、G2 一条)、code 99 × 1
- 步骤:
  1. `day_close(D)`
- 期望:
  - `item = Note`、`code = 1` 的行:G1 `value = 2`、G2 `value = 1`
  - 一行 `item = Unregistered`、`code = 99`、`value = 1`
- 实现:`crates/hft-telemetry/tests/strategy_report.rs::tel_048_note_counts`

### TEL-049 report 字段的五种聚合与 scope

- 规则:[策略报告(`strategy_report`)](README.md#策略报告strategy_report)
- 级别:table
- 里程碑:M4
- 前置:描述符:码 1 `edge`(`v[0]` = edge);report 字段 `edges`(Count, Instance)、`edge_sum`(Sum, Group, v_ix 0)、`edge_last`(Last, Instance)、`edge_max`(Max, Group)、`edge_p99`(P99, Instance);当日 Note(按 seq):seq 100 G1 v0 = 10、seq 101 G2 v0 = 5、seq 102 G1 v0 = 20
- 实现:`crates/hft-telemetry/tests/strategy_report.rs::tel_049_aggregations`

| 行 | 字段或条件 | 期望 |
|---|---|---|
| TEL-049.1 | `edges` | 一行,`value = 3` |
| TEL-049.2 | `edge_sum` | G1 行 30,G2 行 5 |
| TEL-049.3 | `edge_last` | 一行,`value = 20`(seq 102) |
| TEL-049.4 | `edge_max` | G1 行 20,G2 行 5 |
| TEL-049.5 | `edge_p99`(3 个样本) | `value = 20`,`flags` 含 `low_n` |
| TEL-049.6 | `edge_p99`,当日 1 000 个样本取值 1..=1000 | `value = 990`,无 `low_n` |
| TEL-049.7 | `edge_p99`,99 个样本 | 取最大值并带 `low_n` |
| TEL-049.8 | `edge_p99`,100 个样本 1..=100 | `value = 99`,无 `low_n` |
| TEL-049.9 | `edge_sum`,同一 group 两条 `v0 = i64::MAX` | `value = i64::MAX`,`flags` 含 `saturated` |
| TEL-049.10 | `edge_sum`,两条 `v0` 的位模式为 `i64::MIN` | `value = i64::MIN`,`flags` 含 `saturated` |
| TEL-049.11 | `v[0] = 0xFFFF_FFFF_FFFF_FFFF` 的一条 | 按 i64 取值 −1 |
| TEL-049.12 | 同样的 Note 分属两个 UTC 日 | 每日各一组行 |

### TEL-050 Sim 账户与 Live 账户分开出行

- 规则:[策略报告(`strategy_report`)](README.md#策略报告strategy_report)
- 级别:unit
- 里程碑:M4
- 前置:两个实例同一种类,分别绑 Live 账户与 Sim 账户,各写 2 条码 1 的 Note
- 步骤:
  1. `day_close(D)`
- 期望:
  - 两行码 1 的 Note 计数,`account_kind` 分别为 Live 与 Sim,各 `value = 2`;没有合并行
- 实现:`crates/hft-telemetry/tests/strategy_report.rs::tel_050_sim_live_rows`

### TEL-051 目录中新增只有 schema 文件的策略即可出报

- 规则:[策略报告(`strategy_report`)](README.md#策略报告strategy_report)
- 级别:unit
- 里程碑:M4
- 前置:在 TEL-046 的目录中加入新种类 `probe-extra`(kind_id 65300)的 schema 文件并更新 `index.json`;journal 中有该种类实例的 Note
- 步骤:
  1. 以同一份已编译的 hft-telemetry 加载目录并 `day_close(D)`
- 期望:
  - 该种类的 Note 与 report 字段按描述符出行;没有重新编译
- 实现:`crates/hft-telemetry/tests/strategy_report.rs::tel_051_new_strategy_zero_change`

### TEL-052 note 查询按 (kind_id, code) 返回描述符

- 规则:[策略报告(`strategy_report`)](README.md#策略报告strategy_report)
- 级别:table
- 里程碑:M4
- 前置:TEL-046 的目录;`probe-taker` 登记码 1、2
- 实现:`crates/hft-telemetry/tests/strategy_report.rs::tel_052_note_lookup`

| 行 | 查询 | 期望 |
|---|---|---|
| TEL-052.1 | (65280, 1) | `Some`,`name`、`level`、`v` 与 schema 中的描述符相同 |
| TEL-052.2 | (65280, 3) | `None` |
| TEL-052.3 | (65281, 1)(另一种类的同码) | 返回 `probe-target` 的描述符,不是 `probe-taker` 的 |

### TEL-053 advisor_lag_ms 取窗口内 AdvisorLag Note 的最大值,无 Note 时为 0

- 规则:[告警判定所需的派生量](README.md#告警判定所需的派生量)
- 级别:table
- 里程碑:M5
- 前置:实例 pc-main;`window_s = 10`
- 实现:`crates/hft-telemetry/tests/derived.rs::tel_053_advisor_lag`

| 行 | 窗口内的 `Note{source = Advisor, code = AdvisorLag}` | 期望 gauge `advisor_lag_ms` |
|---|---|---|
| TEL-053.1 | 两条,`v[1]` = 1 500、2 500 | 2 500 |
| TEL-053.2 | 无 | 0 |
| TEL-053.3 | 同一 journal 计算两次 | 两次结果相同 |

### TEL-054 FundingDiff 判定在阈值两侧各差一个最小单位

- 规则:[告警判定所需的派生量](README.md#告警判定所需的派生量)、[规则判定函数](README.md#规则判定函数)
- 级别:table
- 里程碑:M5
- 前置:`RuleParams`:`funding_diff_abs = 0.500000 USDT`、`funding_diff_rel = 1%`;`ReconcileReport{item = Funding}` 的 `exch_amount = 10.000000`;阈值 = max(0.5, 1% × 10) = 0.5;金额最小单位 1e-6 USDT
- 实现:`crates/hft-telemetry/tests/rules.rs::tel_054_funding_diff_boundary`

| 行 | `engine_amount` | 期望 |
|---|---|---|
| TEL-054.1 | 9.500001(差 0.499999) | 不告警 |
| TEL-054.2 | 9.500000(差 0.500000) | 不告警 |
| TEL-054.3 | 9.499999(差 0.500001) | 告警 |
| TEL-054.4 | `exch_amount = 100`、`engine_amount` 差 1.000001 | 告警(阈值 = max(0.5, 1.0) = 1.0) |

### TEL-055 资金费校正的计数与金额

- 规则:[告警判定所需的派生量](README.md#告警判定所需的派生量)
- 级别:unit
- 里程碑:M5
- 前置:当日两条 `Note{FundingCorrected}`(source `Ledger`),金额 +0.10 与 −0.03 USDT
- 步骤:
  1. `day_close(D)`
- 期望:
  - 日报 `funding` 节:校正次数 2,金额合计 +0.07 USDT
- 实现:`crates/hft-telemetry/tests/derived.rs::tel_055_funding_corrected`

### TEL-056 断腿自动平仓结果取自 ExecAutoFixDone 与 ExecAutoFixFailed

- 规则:[告警判定所需的派生量](README.md#告警判定所需的派生量)
- 级别:table
- 里程碑:M5
- 前置:group G
- 实现:`crates/hft-telemetry/tests/derived.rs::tel_056_autofix_outcomes`

| 行 | Note | 期望派生量 |
|---|---|---|
| TEL-056.1 | `Note{ExecAutoFixDone}` | G 的结果 = done,含平掉的腿与数量 |
| TEL-056.2 | `Note{ExecAutoFixFailed}` | G 的结果 = failed,含失败原因码 |

### TEL-057 sim_tail_lag_ms 只记录,不进任何规则

- 规则:[告警判定所需的派生量](README.md#告警判定所需的派生量)
- 级别:unit
- 里程碑:M5
- 前置:sim-gw 的测量输出 `sim_tail_lag_ms = 9 999`
- 步骤:
  1. 取窗口报告;以该窗口运行全部规则判定函数
- 期望:
  - gauge 中有 `sim_tail_lag_ms = 9999`;日报 `latency` 节有它;任何 `RuleInput` 都不含该量,所有规则的结论与它为 0 时相同
- 实现:`crates/hft-telemetry/tests/derived.rs::tel_057_sim_tail_lag_record_only`

### TEL-058 跨所偏离以全部 venue 最新值的中位数为参照

- 规则:[告警判定所需的派生量](README.md#告警判定所需的派生量)
- 级别:table
- 里程碑:M5
- 前置:资产 AIN;按行的各 venue 最新值(永续用 `Mark`,现货用 BBO 中间价)
- 实现:`crates/hft-telemetry/tests/derived.rs::tel_058_mark_divergence`

| 行 | 各 venue 最新值 | 期望偏离(bp) |
|---|---|---|
| TEL-058.1 | A 100.00(静止)、B 101.00、C 101.00 | A = −99.01(相对中位数 101.00,四舍五入到 0.01 bp),B = 0,C = 0 |
| TEL-058.2 | 随后 B、C 继续升到 102.00,A 不变 | A = −196.08(偏离增大),B = 0,C = 0 |
| TEL-058.3 | 只有 A 订阅 AIN | 不输出 |
| TEL-058.4 | 现货 venue D 的 BBO 99.98/100.02、永续 A 的 Mark 100.00 | D 的取值为中间价 100.00;两者偏离都是 0 |

### TEL-059 按小时直方图只含 cal_ok 样本,sim 状态取自心跳采样

- 规则:[告警判定所需的派生量](README.md#告警判定所需的派生量)
- 级别:table
- 里程碑:M5
- 前置:一个 UTC 小时内 100 个 `tick_to_trade` 样本,其中 10 个 `cal_ok = false`;`HeartbeatBlock` 对 `sim` 实例的采样按行
- 实现:`crates/hft-telemetry/tests/derived.rs::tel_059_hourly_stats`

| 行 | `sim` 采样 | 期望 `HourStat` |
|---|---|---|
| TEL-059.1 | 整小时 Running | `sim = On`,`n = 90` |
| TEL-059.2 | 整小时停止 | `sim = Off` |
| TEL-059.3 | 前 30 分 Running、后 30 分停止 | `sim = Mixed` |

### TEL-060 保证金与账户指标按来源与维度派生

- 规则:[保证金与账户指标](README.md#保证金与账户指标)
- 级别:table
- 里程碑:M5
- 前置:账户 3、venue `binance-usdm`、inst P;每行的输入事件
- 实现:`crates/hft-telemetry/tests/gauges.rs::tel_060_margin_metrics`

| 行 | 输入 | 期望 |
|---|---|---|
| TEL-060.1 | 最新 `MarginReport` 保证金率 0.15、余额 1 000、可用 600 | `margin_ratio = 0.15`、`margin_balance = 1000`、`avail_margin = 600`,维度 (3, `binance-usdm`) |
| TEL-060.2 | `MarginReport` 时刻 1 000 ms,当前 6 000 ms | `margin_data_age_ms = 5000` |
| TEL-060.3 | `Note{CapSet}` 的 `MarginStress` cap 为 L2 | `margin_stress_level = 2`;随后 `Note{CapCleared}` → 0 |
| TEL-060.4 | 多头,mark 100,交易所 `liq_px` 80,ledger 估算 85 | `liq_dist = 0.15`(取更靠近 mark 的 85) |
| TEL-060.5 | 多头,mark 100,交易所 `liq_px` 90,ledger 估算 85 | `liq_dist = 0.10`(取 90) |
| TEL-060.6 | 空仓 | 不输出 `liq_dist` |
| TEL-060.7 | 两笔 `ExternalFill`(cause `Liquidation`、`Adl`) | `external_fills_total{cause=Liquidation} = 1`、`{cause=Adl} = 1` |
| TEL-060.8 | `AccountConfigReport.mismatches = 0b101` | `account_cfg_mismatch = 5` |
| TEL-060.9 | inst P 最近 `Funding` 在 400 s 前,下次结算时刻已过 70 s 未更新 | `funding_data_age_s = 400`;"结算后未更新"计数 +1 |
| TEL-060.10 | 三条 `ExecDecision`(算法 `TakerSlice` 两条、`PairOnFill` 一条) | `exec_decisions_total` 按 (账户, 算法, 原因) 分别为 2 与 1 |

### TEL-061 计数器按来源事件派生

- 规则:[计数器(由事件派生,冷侧计算)](README.md#计数器由事件派生冷侧计算)
- 级别:table
- 里程碑:M5
- 前置:夹具 journal,各行所列事件
- 实现:`crates/hft-telemetry/tests/counters.rs::tel_061_counters`

| 行 | 输入 | 期望计数 |
|---|---|---|
| TEL-061.1 | 5 条 `Bbo`、2 条 `Trade`(venue 1) | 事件数 (kind `Bbo`, venue 1) = 5、(`Trade`, 1) = 2 |
| TEL-061.2 | 4 条 `OrderDecision`:放行 2、Shadow 1、拒绝 1(`RejectBits` 两位置位) | 放行 2、Shadow 1、拒绝 1;两个被置位的拒绝位各 +1 |
| TEL-061.3 | 3 条 `ExecDecision` | 按 (strategy, group, 算法, 原因) 计 |
| TEL-061.4 | 两腿成交与第二腿 `SendReport` | 成交数 +2,双腿间隔一个样本 |
| TEL-061.5 | `Note{RateLimitHit}` × 2,`OrderEvent.reject_code = −1003` × 1 | `RateLimitHit` = 2;reject 码 −1003 = 1 |
| TEL-061.6 | `Note{Conflated}` × 7(venue 1) | BBO 合并丢弃 = 7 |
| TEL-061.7 | `Note{OmsTransition}` 进入 Unknown(t = 10 s)、离开(t = 13 s) | Unknown 持续时长分布 count 1,值 3 s |
| TEL-061.8 | 两条 `ConnState`(切换一次、重同步一次) | 连接切换 1、重同步 1 |
| TEL-061.9 | 未知用户流事件:可忽略表内 3 条、表外 1 条 | 两个计数分别为 3 与 1 |
| TEL-061.10 | `CtxFull` Note 1 条、advice 过期 Note 2 条 | Ctx 溢出 1、advice 过期 2 |

### TEL-062 生成的 notes_catalog 覆盖全部拥有者导出的码

- 规则:[Note 翻译](README.md#note-翻译)
- 级别:unit
- 里程碑:M5
- 前置:`cargo xtask gen-schema` 生成的 `notes_catalog.rs`;各 source crate 导出的 `const` 码表
- 步骤:
  1. 收集全部 (source, code) 与目录条目比较
- 期望:
  - 两个集合相等;任一 const 码表多出的码 → 测试失败并列出 (source, code)
- 实现:`crates/hft-telemetry/tests/notes.rs::tel_062_catalog_complete`

### TEL-063 note_text 输出结构化字段与一行中文摘要

- 规则:[Note 翻译](README.md#note-翻译)
- 级别:unit
- 里程碑:M5
- 前置:一条引擎 Note:`source = Exec`、`code = ExecGateHeldNaked`、`level = P0`、`group = 0x01000065`、`v` 按登记表
- 步骤:
  1. `Telemetry::note_text(&note)`
- 期望:
  - 结构化字段含 `code = ExecGateHeldNaked`、`level = P0`、`source = Exec`、`group = 0x01000065` 与登记表定义的 `v[]` 名;摘要为一行中文(不含换行)
- 实现:`crates/hft-telemetry/tests/notes.rs::tel_063_note_text_fields`

### TEL-064 策略 Note 经目录翻译,查不到时输出十六进制

- 规则:[Note 翻译](README.md#note-翻译)
- 级别:table
- 里程碑:M5
- 前置:TEL-046 的目录;`probe-taker` 登记码 1
- 实现:`crates/hft-telemetry/tests/notes.rs::tel_064_strategy_note_text`

| 行 | Note | 期望 |
|---|---|---|
| TEL-064.1 | `source = Strategy`、实例种类 65280、code 1 | 名与 `v[]` 含义取自描述符 |
| TEL-064.2 | 同上 code 99 | 输出码的十六进制;`unregistered` +1 |

### TEL-065 未知 kind 输出十六进制并计数

- 规则:[Note 翻译](README.md#note-翻译)
- 级别:unit
- 里程碑:M5
- 前置:一条 `source = 99`(未登记)的 Note
- 步骤:
  1. `note_text`
- 期望:
  - 输出含 source 与 code 的十六进制;未知计数 +1;不 panic
- 实现:`crates/hft-telemetry/tests/notes.rs::tel_065_unknown_source`

### TEL-066 规则判定函数在阈值边界两侧各差一个单位

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:table
- 里程碑:M5
- 前置:每行的 `RuleParams` 在行内给出(取值即 watchtower 配置的首版缺省值,[watchtower 健康规则](../../apps/watchtower/README.md#健康规则));规则表与级别归 watchtower,本用例只验证判定函数的边界
- 实现:`crates/hft-telemetry/tests/rules.rs::tel_066_rule_boundaries`

| 行 | 规则与参数 | 输入 | 期望 `RuleVerdict` |
|---|---|---|---|
| TEL-066.1 | `FeedSilent`,`silent_ms = 2000` | 最近行情年龄 2 000 ms | 不触发 |
| TEL-066.2 | 同上 | 2 001 ms | 触发 P1 |
| TEL-066.3 | `UnknownAge`,阈值 10 000 ms | 10 000 ms | 不触发 |
| TEL-066.4 | 同上 | 10 001 ms | 触发 P0 |
| TEL-066.5 | `ReconcileMismatch`,`recon_warn_usd = 50`、`recon_p0_usd = 500` | `Warn` 条目名义 49.999999 | 不触发 |
| TEL-066.6 | 同上 | 50.000000 | P1 |
| TEL-066.7 | 同上 | 500.000000 | P0 |
| TEL-066.8 | `NakedExposure`,`naked_usd = 5`,持续 30 s | 名义差 6,持续 30 000 ms | 不触发 |
| TEL-066.9 | 同上 | 持续 30 001 ms | P0 |
| TEL-066.10 | `MarginDataStale`,`margin_stale_ms = 10000` | 年龄 10 001 ms | P1;10 000 ms 不触发 |
| TEL-066.11 | `FundingStale`,`funding_stale_s = 300` | 年龄 300 s / 301 s | 不触发 / P1 |
| TEL-066.12 | `PersistLag` | 60 s 窗口 p99 = 50 ms / 50 ms + 1 ns | 不触发 / P2 |
| TEL-066.13 | 同上 | p99 = 1 s / 1 s + 1 ns | P2 / P1 |
| TEL-066.14 | 同上 | p99 = 10 s / 10 s + 1 ns | P1 / P0 |
| TEL-066.15 | `SequencerStall`,`hb_stale_ms = 3000`,实例有 `is_prod = true` 的 Live 账户 | 心跳 3 001 ms 未前进,`state = Running` | P0 |
| TEL-066.16 | 同上,实例只有 Sim 账户 | 同上 | P1 |
| TEL-066.17 | `SimDrift`,`dust` 由 refdata 给出 | 连续 2 次报告差异超过 `dust` / 只有 1 次 | P1 / 不触发 |
| TEL-066.18 | `MarkDivergence`,`mark_div_bp = 50` | 偏离 50 bp / 50 bp + 1(Rate raw) | 不触发 / 触发 |

### TEL-067 同机干扰按 UTC 对齐分块并标记资金费块与排除块

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:table
- 里程碑:M5
- 前置:`block_hours = 2`、`funding_hours_utc = [0, 8, 16]`、`aa = false`;小时数据从 `2026-11-01T01:00Z` 起 50 小时;按行设置个别小时的 `sim` 状态
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_067_blocks`

| 行 | 条件 | 期望 `BlockRow` |
|---|---|---|
| TEL-067.1 | 块 [00:00, 02:00) 只有 01:00 一个小时 | `excluded = true` |
| TEL-067.2 | 块 [08:00, 10:00) 一开一关 | `funding = true`、`excluded = false` |
| TEL-067.3 | 块 [02:00, 04:00) 一开一关,先开 | `funding = false`、`first_on = true` |
| TEL-067.4 | 块 [04:00, 06:00) 两小时都开 | `excluded = true` |
| TEL-067.5 | 块 [06:00, 08:00) 含一个 `Mixed` 小时 | `excluded = true`,仍列在 `blocks` 中 |
| TEL-067.6 | 全部未排除块 | `balance` = [(true, 资金费块先开数, 先关数), (false, 普通块先开数, 先关数)],与按 `first_on` 手数的结果相同 |

### TEL-068 真实比值 1.00 且按资金费块配平时通过

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:unit
- 里程碑:M5
- 前置:合成 7 天 84 块:普通小时样本在 [90 000, 100 000] ns 均匀分布,资金费整点所在小时在 [99 000, 110 000] ns 均匀分布;每小时 600 个样本(每侧 84 小时 × 600 = 50 400);"开"小时不放大(真实比值 1.00);资金费块 21 个中 11 个先开、10 个先关,普通块 63 个中 32 个先开、31 个先关;`InterferenceParams`:`min_n_each_side = 10_000`、`max_ratio_ub = 1.05`、`max_halfwidth = 0.03`、`conf = 0.95`、`resamples = 2000`、`seed = 42`、`hard_stages = [tick_to_trade, rep_to_order]`
- 步骤:
  1. `interference(&hours, &p)`
- 期望:
  - 该组 `ci.1 ≤ 1.05`、`verdict = Pass`;`pass = true`、`extend = false`
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_068_balanced_pass`

### TEL-069 先开固定在第一小时时比值偏离 1,证明配平的必要

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:unit
- 里程碑:M5
- 前置:同 TEL-068,但全部块先开(资金费整点都落在"开"侧)
- 步骤:
  1. `interference(&hours, &p)`
- 期望:
  - `ratio > 1.05`;`ci.0 > 1.00`(真实比值为 1 却判出偏差);`balance` 中资金费块先关数 = 0
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_069_unbalanced_bias`

### TEL-070 真实比值 1.08 时判不通过

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:unit
- 里程碑:M5
- 前置:同 TEL-068,"开"小时的样本 × 1.08
- 步骤:
  1. `interference(&hours, &p)`
- 期望:
  - `ci.1 > 1.05`、`halfwidth ≤ 0.03`、`verdict = Fail`;`pass = false`、`extend = false`
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_070_ratio_fail`

### TEL-071 区间过宽且上界越线时判 Extend

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:unit
- 里程碑:M5
- 前置:同 TEL-068,但每个小时的样本再乘以由夹具种子决定、取值在 [0.6, 1.4] 的小时因子,"开"小时 × 1.06;夹具构造使 `halfwidth > 0.03`
- 步骤:
  1. `interference(&hours, &p)`
- 期望:
  - `ci.1 > 1.05`、`halfwidth > 0.03`、`verdict = Extend`;`pass = false`、`extend = true`
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_071_wide_extend`

### TEL-072 每侧样本不足时只记录

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:unit
- 里程碑:M5
- 前置:同 TEL-068,每小时 60 个样本(每侧 5 040 < `min_n_each_side = 10_000`)
- 步骤:
  1. `interference(&hours, &p)`
- 期望:
  - 该组 `verdict = RecordOnly`,`ratio`、`ci` 照常给出;没有可硬判定的组 → `pass = false`,报告注明"无可硬判定的组"
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_072_low_n_record_only`

### TEL-073 不在 hard_stages 中的分段只记录

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:unit
- 里程碑:M5
- 前置:同 TEL-070(比值 1.08),但该组 `stage = hedge_host`,不在 `hard_stages`
- 步骤:
  1. `interference(&hours, &p)`
- 期望:
  - 该组 `verdict = RecordOnly`;它不影响 `pass`
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_073_non_hard_stage`

### TEL-074 A/A 模式全部只记录且区间包含 1

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:table
- 里程碑:M5
- 前置:两天 24 块,`sim` 全部 Off;样本分布同 TEL-068(无放大);`aa = true`、`seed = 7`
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_074_aa_mode`

| 行 | 条件 | 期望 |
|---|---|---|
| TEL-074.1 | 原样 | 每组 `verdict = RecordOnly`;`ci.0 ≤ 1 ≤ ci.1`;`pass = false` |
| TEL-074.2 | 原样 | 各块"开"标签由 `seed` 按配平规则贴出:资金费块与普通块各自先开与先关之差 ≤ 1;`ratio`、`ci`、`halfwidth` 与测试内朴素参考实现(合并原始样本、同一 `ChaCha8Rng` 序列抽块)逐位相同 |
| TEL-074.3 | 某块有一小时 On | 该块 `excluded = true` |

### TEL-075 同一种子结果逐位相同,换种子只有重抽样量级的变化

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:unit
- 里程碑:M5
- 前置:TEL-068 的数据
- 步骤:
  1. `seed = 42` 运行两次;`seed = 43` 运行一次
- 期望:
  - 两次 `seed = 42` 的 `InterferenceReport` 序列化后逐字节相同
  - `seed = 43` 与 `seed = 42` 的 `ci.1` 之差 ≤ 0.01;`verdict` 相同
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_075_seed_determinism`

### TEL-076 置信区间取重抽样分布的 (1 − conf)/2 与 (1 + conf)/2 分位

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:unit
- 里程碑:M5
- 前置:TEL-068 的数据;`conf = 0.95`、`resamples = 2000`
- 步骤:
  1. `interference`;测试内以同一 `ChaCha8Rng::seed_from_u64(42)` 序列重算 2 000 个比值
- 期望:
  - `ci.0` = 重算分布的 2.5% 分位,`ci.1` = 97.5% 分位;`halfwidth = (ci.1 − ci.0) / 2`
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_076_ci_quantiles`

### TEL-077 组按 HistKey 排序处理,输入顺序不影响结果

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:prop
- 里程碑:M5
- 性质:把 `hours` 任意置换后 `interference` 的输出逐字节相同
- 生成:TEL-068 的数据的随机置换
- 参考:未置换输入的输出
- 例数:PR 50
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_077_order_independent`

### TEL-078 pass 与 extend 由各组判定汇总

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:table
- 里程碑:M5
- 前置:直接构造各组的判定(两组硬判定)
- 实现:`crates/hft-telemetry/tests/interference.rs::tel_078_aggregate`

| 行 | 两组判定 | 期望 `pass`、`extend` |
|---|---|---|
| TEL-078.1 | Pass、Pass | true、false |
| TEL-078.2 | Pass、Fail | false、false |
| TEL-078.3 | Pass、Extend | false、true |
| TEL-078.4 | Fail、Extend | false、false |
| TEL-078.5 | RecordOnly、RecordOnly | false、false(无可硬判定的组) |

### TEL-079 规则判定函数是纯函数

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:unit
- 里程碑:M5
- 前置:TEL-066 每行的输入
- 步骤:
  1. 每个输入调用两次判定函数;在两次之间改变系统时间与环境变量
- 期望:
  - 两次结论逐字段相同
- 实现:`crates/hft-telemetry/tests/rules.rs::tel_079_pure_functions`

### TEL-080 StageSet 的分段名集合与 latency-budget §2 一致

- 规则:[分段(`StageSet`)](README.md#分段stageset)
- 级别:unit
- 里程碑:M1.5
- 前置:`StageSet` 的分段名列表;`docs/latency-budget.md` §2 表的分段列(由 `doc-check` 解析)
- 步骤:
  1. 比较两个集合(链上分段按可选扩展单列)
- 期望:
  - 生产分段与诊断分段的集合相等;`ws_rtt`(连接级,不是事件分段)不在 `StageSet` 中
- 实现:`crates/hft-telemetry/tests/stages.rs::tel_080_names_match_latency_budget`

### TEL-081 依赖方向:只被冷进程使用,不依赖策略

- 规则:[依赖](README.md#依赖)
- 级别:table
- 里程碑:M1.5
- 前置:夹具 `clean`,按行加依赖后运行 `cargo xtask deps`
- 实现:`crates/hft-telemetry/tests/deps.rs::tel_081_dependency_rules`

| 行 | 加入的依赖 | 期望 |
|---|---|---|
| TEL-081.1 | `hft-oms → hft-telemetry` | 退出 1 |
| TEL-081.2 | `apps/hft-core → hft-telemetry` | 退出 1 |
| TEL-081.3 | `hft-telemetry → strategies/x` | 退出 1 |
| TEL-081.4 | `hft-telemetry → hft-strategy-registry` | 退出 1 |
| TEL-081.5 | `apps/watchtower → hft-telemetry` | 退出 0 |

### TEL-082 金样 journal 产出期望的窗口报告、加权统计、账户日报与 Note 文本

- 规则:[使用本组件](README.md#使用本组件)、[分段(`StageSet`)](README.md#分段stageset)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/telemetry/day1/`:一段 seq journal,含行情触发与回报触发的 `LatencySample` 各 20 条、`ClockStatus` 每 1 000 ms 一条、`MarginReport`、引擎与策略 `Note`
- 期望文件:`fixtures/telemetry/day1/expected/{window.json, event_weighted.json, account_daily.json, notes.txt}`
- 实现:`crates/hft-telemetry/tests/golden.rs::tel_082_day1`

### TEL-083 模拟账户长跑期间日报与离线重算逐字段相等

- 规则:[账户报告(`account_report`)](README.md#账户报告account_report)
- 级别:host
- 里程碑:M6
- 环境:dev2 的 `sim` 实例(门槛 ⑦ 的 dev2 段)
- 授权:无
- 步骤:每日以 `replayer` 对前一 UTC 日的 journal 离线运行 `account_report`,与 watchtower 当日写出的 `AccountDaily` 比较
- 通过标准:⑦ 计时期间每一天两份 `AccountDaily` 逐字段相等
- 证据:每日比较结果(文件名含 `TEL-083` 与日期),归档到门槛 ⑦ 记录
- 实现:`tests/host/TEL-083.sh`

### TEL-084 门槛 ⑩ 的延迟与干扰报告由本 crate 产出

- 规则:[按事件加权的离线统计(`event_weighted`)](README.md#按事件加权的离线统计event_weighted)、[规则判定函数](README.md#规则判定函数)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,`prod` 与 `sim` 同机,门槛 ⑩ 的测量窗口
- 授权:无
- 步骤:窗口结束后运行 `replayer latency`(`event_weighted`、`stratified_probe`)与 `watchtower interference`(含窗口前的 `--aa`)
- 通过标准:两份报告的每组数值与本 crate 对同一 journal 的离线调用逐位相同;报告含 testing §8.2 要求的全部列(块表、配平计数、A/A 结果、种子、每组 n/R/区间/半宽,`kernel_rx` 上下界与分层)
- 证据:两份报告与离线调用输出,文件名含 `TEL-084`
- 实现:`tests/host/TEL-084.sh`

### TEL-085 ingest 吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`telemetry/ingest`
- 实现:`crates/hft-telemetry/benches/telemetry.rs::tel_085_ingest`

### TEL-086 stages 单样本耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`telemetry/stages`
- 实现:`crates/hft-telemetry/benches/telemetry.rs::tel_086_stages`

### TEL-087 event_weighted 吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`telemetry/event_weighted`
- 实现:`crates/hft-telemetry/benches/telemetry.rs::tel_087_event_weighted`

### TEL-088 interference 耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M5
- 基准名:`telemetry/interference`
- 实现:`crates/hft-telemetry/benches/telemetry.rs::tel_088_interference`

### TEL-089 day_close 耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`telemetry/day_close`
- 实现:`crates/hft-telemetry/benches/telemetry.rs::tel_089_day_close`

### TEL-090 每序列内存

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`telemetry/series_bytes`
- 实现:`crates/hft-telemetry/benches/telemetry.rs::tel_090_series_bytes`

### TEL-091 外部告警行的直通判定按行内 level 定级并给出指纹字段

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:table
- 里程碑:M5
- 前置:`rules::archiver_alert` 的输入为 journal-archiver `alerts.jsonl` 的一行;`rules::pipeline_alert` 的输入为实例 `pc-a` 的 `status.json` 中 `alerts` 段的一项;每行一条输入
- 实现:`crates/hft-telemetry/tests/rules.rs::tel_091_passthrough_alerts`

| 行 | 函数与输入 | 期望 `RuleVerdict` |
|---|---|---|
| TEL-091.1 | `archiver_alert`:`{level: "P0", kind: "ChainBreak", prefix: "hft", instance: "prod", journal: "seq", hour: "2026-11-05T01", …}` | 级别 P0;指纹字段 = (`ChainBreak`, `hft`, `prod`, `seq`, `2026-11-05T01`) |
| TEL-091.2 | `archiver_alert`:同上但 `level: "P1"`、`kind: "Quarantine"` | 级别 P1 |
| TEL-091.3 | `archiver_alert`:`level: "P7"` | 级别 P1,`malformed = true` |
| TEL-091.4 | `archiver_alert`:不是合法 JSON | 级别 P1,`malformed = true`,指纹字段为空 |
| TEL-091.5 | `pipeline_alert`:`{level: "P1", code: "PublishRejected", group: 16777317, detail: "…", at: …}` | 级别 P1;指纹字段 = (`pc-a`, `PublishRejected`, 16777317) |
| TEL-091.6 | `pipeline_alert`:`{level: "P2", code: "WriteOffNeeded"}`(无 `group`) | 级别 P2;指纹字段 = (`pc-a`, `WriteOffNeeded`, 无) |
| TEL-091.7 | `pipeline_alert`:缺 `code` | 级别 P1,`malformed = true` |

### TEL-092 规则注册表含 ArchiverAlert 与 PipelineAlert 两个直通规则

- 规则:[规则判定函数](README.md#规则判定函数)
- 级别:unit
- 里程碑:M5
- 前置:`rules::REGISTRY`
- 步骤:
  1. 按名字查 `ArchiverAlert` 与 `PipelineAlert`
- 期望:
  - 两项都存在,类别为事件型,判定函数分别为 `archiver_alert` 与 `pipeline_alert`;注册表中名字不重复
- 实现:`crates/hft-telemetry/tests/rules.rs::tel_092_registry_has_passthrough`

## 验收

- 本组件完成 = 里程碑已到的全部用例有同编号的实现且通过(`cargo xtask test-trace`)。
- 门槛 ⑦:TEL-083 在计时期间每日成立([testing §8](../../docs/testing.md#8-上线门槛))。
- 门槛 ⑩:报告由本 crate 产出(TEL-084);判定口径与阈值归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法)。
- 基准用例(TEL-085–TEL-090)的数值目标见本 crate README 的延迟与资源契约,登记归 [benchmarks](../../benchmarks/README.md)。
