---
component: latency-slice
prefix: SLC
spec: README.md
---

# latency-slice 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本文件的记号:

- `CFG0` = 最小配置:`slice.product = "spot"`,一个 `[[insts]]`:`symbol = "BTCUSDT"`、`base = "BTC"`、`tick = "0.01"`、`lot = "0.00001"`、`min_qty = "0.00001"`、`min_notional = "5"`;`net.src_ip = "10.0.0.5"`、`net.bind_device = "eth1"`;`orders.secret_env` 指向的环境变量为 RFC 8032 测试 1 私钥的 64 位十六进制种子;其余键取默认值。由 `CFG0` 生成的 refdata:BTC `px_exp = −2`、`qty_exp = −8`,inst 1 = BTCUSDT(tick raw 1,lot raw 1 000,min_qty raw 1 000,min_notional 5 USDT = `Notional` raw 5 000 000)。
- `SEQ0` = 由 `CFG0` 构建、`probe.probe_period_ms = 1000`、`probe.far_bp = 300`、`probe.size_margin_pct = 10`、`probe.max_spread_bp = 100`、`orders.max_open_orders = 2`、`orders.max_order_notional = "50"` 的简化 sequencer,时钟 1 tick = 1 ns,journal 在 hft-journal 的堆后端;gw 下单连接已 `Up`。
- `B(t, bid, ask)` = 时刻 `t`(ms,`rx_tsc = t × 10⁶`)inst 1 的 `Bbo{bid_px = bid, ask_px = ask}`(定点 raw,`px_exp = −2`),`hw_rx_ns = t × 10⁶ − 5 000`、`sw_rx_ns = t × 10⁶ − 3 000`、`rx_empty_gap_ns = 1 000`。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | SLC-040, SLC-100 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 命令行 | SLC-001, SLC-041, SLC-042 |
| 配置文件 | SLC-002, SLC-003, SLC-007 |
| 密钥 | SLC-005, SLC-006 |
| 安全约束 | SLC-004, SLC-016, SLC-018, SLC-019, SLC-021, SLC-022, SLC-042 |
| 线程布局 | SLC-040, SLC-100 |
| 简化 sequencer | SLC-010–SLC-025 |
| 产出 | SLC-030, SLC-031, SLC-040 |
| dry-run | SLC-040, SLC-041 |
| host 探测模式 | SLC-043, NET-371–NET-374 |
| 实现说明 | 不适用:见子节 |
| 源文件 | 不适用:非规范 |
| refdata 指数 | SLC-007 |
| 收尾顺序 | SLC-022, SLC-040 |
| 延迟与资源契约 | SLC-100, SLC-101, SLC-102 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

### SLC-001 命令行的模式与覆盖项

- 规则:[命令行](README.md#命令行)
- 级别:table
- 里程碑:M1.5
- 前置:环境变量 `HFT_SLICE_CONFIG` 未设置;每行解析一组参数,断言模式与覆盖项
- 实现:`tools/latency-slice/tests/cli.rs::slc_001_cli_parse`

| 行 | 参数 | 期望 |
|---|---|---|
| SLC-001.1 | (空) | 模式 Run;配置路径 `latency-slice.toml`;无覆盖项 |
| SLC-001.2 | `--minutes 60 --ts-expect-hw --print-conn-stats` | Run;`minutes = Some(60)`、`ts_expect_hw = true`、`print_conn_stats = true` |
| SLC-001.3 | `--config a.toml --ballast 0 --no-orders --out d` | Run;配置 `a.toml`;`ballast = Some(0)`、`no_orders = true`、`out = Some("d")` |
| SLC-001.4 | `--dry-run --seconds 3 --seed 7` | DryRun{seconds: 3, seed: 7} |
| SLC-001.5 | `--report r` | Report{root: "r"} |
| SLC-001.6 | `--build-only 20 --print-napi` | BuildOnly{n: 20, print_napi: true} |
| SLC-001.7 | `--idle-polls 1000000` | IdlePolls{n: 1000000} |
| SLC-001.8 | `--two-packet-stamp-probe 1000` | TwoPacket{n: 1000} |
| SLC-001.9 | `--loopback-stream-minutes 10` | LoopbackStream{minutes: 10} |
| SLC-001.10 | `--minutes` | `Err`(缺值),退出码 2 |
| SLC-001.11 | `--minutes x` | `Err`(非数字),退出码 2 |
| SLC-001.12 | `--bogus` | `Err`(未知参数),退出码 2 |
| SLC-001.13 | `--dry-run --report r` | `Err`(两个模式),退出码 2 |
| SLC-001.14 | 环境变量 `HFT_SLICE_CONFIG = e.toml`,无 `--config` | 配置路径 `e.toml` |

### SLC-002 配置的默认值

- 规则:[配置文件](README.md#配置文件)
- 级别:table
- 里程碑:M1.5
- 前置:`CFG0`;每行断言加载后的一个值
- 实现:`tools/latency-slice/tests/config.rs::slc_002_config_defaults`

| 行 | 键 | 期望 |
|---|---|---|
| SLC-002.1 | `slice.minutes` / `slice.out_dir` / `slice.epoch` | 60 / `./slice-out` / 0 |
| SLC-002.2 | `feed.host` / `feed.port` / `feed.bbo_conns` | `stream.binance.com` / 9443 / 2 |
| SLC-002.3 | `orders.enabled` / `orders.host` / `orders.port` | true / `demo-ws-api.binance.com` / 443 |
| SLC-002.4 | `orders.api_key_env` / `orders.secret_env` | `HFT_SLICE_API_KEY` / `HFT_SLICE_ED25519` |
| SLC-002.5 | `orders.max_open_orders` / `orders.max_order_notional` / `orders.max_orders` | 2 / 50 USDT(raw 50 000 000) / 20 000 |
| SLC-002.6 | `probe.probe_period_ms` / `burst_period_ms` / `far_bp` / `quote_side` | 1 000 / 0 / 300 / buy |
| SLC-002.7 | `probe.max_spread_bp` / `size_margin_pct` / `quote_rest_ms` / `max_consec_fail` | 100 / 10 / 2 000 / 5 |
| SLC-002.8 | `ballast.conns` / `journal.hot_dir` / `journal.segment_mib` / `journal.raw` | 0 / `/dev/shm/hft-slice` / 64 / on |
| SLC-002.9 | `net.ts_expect_hw` / `report.burst_gap_ns` / `report.cal_max_err_ns` | false / 20 000 / 1 000 |
| SLC-002.10 | `threads` 的核号:hk、persist、warm、seq、feed-bbo、feed-md、gw | 0、2、3、4、5、7、8 |
| SLC-002.11 | `slice.product = "usdm"` 时 `feed.host` / `feed.port` | `fstream.binance.com` / 443 |

### SLC-003 配置错误在加载时退出 2

- 规则:[配置文件](README.md#配置文件)
- 级别:table
- 里程碑:M1.5
- 前置:`CFG0` 做每行一处改动后加载
- 实现:`tools/latency-slice/tests/config.rs::slc_003_config_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| SLC-003.1 | 加 `slice.colour = 1`(未知键) | `Err`,消息含 `colour`,退出码 2 |
| SLC-003.2 | 删 `slice.product` | `Err`,退出码 2 |
| SLC-003.3 | `slice.product = "okx"` | `Err`,退出码 2 |
| SLC-003.4 | 删全部 `[[insts]]` | `Err`,退出码 2 |
| SLC-003.5 | `probe.burst_period_ms = 1001`(> `probe_period_ms` 1 000) | `Err`,退出码 2 |
| SLC-003.6 | `probe.burst_period_ms = 1000` | `Ok` |
| SLC-003.7 | `probe.quote_side = "sell"`(`spot`) | `Err`,退出码 2 |
| SLC-003.8 | `probe.far_bp = 49` / `50` / `1000` / `1001` | `Err` / `Ok` / `Ok` / `Err` |
| SLC-003.9 | inst `tick = "abc"` | `Err`,退出码 2 |
| SLC-003.10 | `slice.product = "usdm"`,不给 `orders.host` | `Err`(演示环境 ws-fapi 端点待核实(方法:SLC-102 前在 Binance 开发者文档查 USDⓈ-M demo WS API 主机名)),退出码 2 |
| SLC-003.11 | `slice.product = "usdm"`,`orders.enabled = false`,不给 `orders.host` | `Ok` |

### SLC-004 主网下单端点在加载时硬拒绝

- 规则:[安全约束](README.md#安全约束)
- 级别:table
- 里程碑:M1.5
- 前置:`CFG0`,每行只改 `orders.host`
- 实现:`tools/latency-slice/tests/config.rs::slc_004_mainnet_order_host_refused`

| 行 | `orders.host` | 期望 |
|---|---|---|
| SLC-004.1 | `ws-api.binance.com` | `Err(MainnetOrderHost)`,退出码 2 |
| SLC-004.2 | `ws-fapi.binance.com` | `Err(MainnetOrderHost)` |
| SLC-004.3 | `WS-API.Binance.com`(大小写) | `Err(MainnetOrderHost)` |
| SLC-004.4 | `api3.binance.com` | `Err(MainnetOrderHost)` |
| SLC-004.5 | `fstream.binance.com` | `Err(MainnetOrderHost)` |
| SLC-004.6 | `example.com`(不含 demo/testnet) | `Err(MainnetOrderHost)` |
| SLC-004.7 | `demo-ws-api.binance.com` | `Ok` |
| SLC-004.8 | `testnet.binancefuture.com` | `Ok` |
| SLC-004.9 | `ws-api.binance.com`,`orders.enabled = false` | `Ok`(不下单时不检查) |

### SLC-005 密钥只从环境变量或本地文件读

- 规则:[密钥](README.md#密钥)
- 级别:table
- 里程碑:M1.5
- 前置:`CFG0`;`HFT_SLICE_API_KEY = "TESTKEY"`;每行一处改动后读密钥
- 实现:`tools/latency-slice/tests/config.rs::slc_005_key_sources`

| 行 | 改动 | 期望 |
|---|---|---|
| SLC-005.1 | 无 | `Ok`:`api_key = "TESTKEY"`,种子 = RFC 8032 测试 1 私钥 |
| SLC-005.2 | 配置加 `orders.api_key = "x"` | `Err`(配置内含密钥键),退出码 2 |
| SLC-005.3 | 配置加 `orders.secret = "x"` | `Err`,退出码 2 |
| SLC-005.4 | 配置加 `orders.private_key = "x"` | `Err`,退出码 2 |
| SLC-005.5 | 取消 `HFT_SLICE_API_KEY` | `Err`(环境变量缺失),退出码 2 |
| SLC-005.6 | `secret_env` 与 `secret_file` 都给出 | `Err`,退出码 2 |
| SLC-005.7 | `secret_file` 指向权限 0600 的 PKCS#8 PEM 文件(同一私钥),`secret_env` 删除 | `Ok`,种子同 .1 |
| SLC-005.8 | 同 .7,文件权限 0644(仅 Linux) | `Err`(他人可读),退出码 2 |
| SLC-005.9 | `orders.enabled = false`,两个环境变量都不设 | `Ok`,不读密钥 |

### SLC-006 PKCS#8 PEM 私钥解析为种子且签名可验证

- 规则:[密钥](README.md#密钥)
- 级别:unit
- 前置:RFC 8032 测试 1 私钥的 PKCS#8 PEM 文本(`-----BEGIN PRIVATE KEY-----` 包裹 48 字节 DER 的 base64)
- 里程碑:M1.5
- 步骤:
  1. 解析 PEM 得到种子
  2. 以 `venue_binance::Spot::login(Order, …)` 用该种子与 `apiKey = "TESTKEY"`、`now_ms = 1790000000000` 生成登录帧
  3. 解析 `-----BEGIN PUBLIC KEY-----` 包裹的文本
- 期望:
  - 步骤 1 种子 = `9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60`
  - 步骤 2 帧的 `signature` = `kyKvl/kTaZKE+kUIQtZzLoF+xBBLbhAc+PoRvtG+MseTjjM1xPqXeIBiOYildYFAu7yqD9fn723B1gwti8VcCA==`
  - 步骤 3 返回 `Err`(不是 PKCS#8 私钥)
- 实现:`tools/latency-slice/tests/config.rs::slc_006_pem_seed`

### SLC-007 由配置生成的 refdata

- 规则:[配置文件](README.md#配置文件)、[refdata 指数](README.md#refdata-指数)
- 级别:table
- 里程碑:M1.5
- 前置:`CFG0` 另加 inst `ETHUSDT`(`base = "ETH"`、`tick = "0.01"`、`lot = "0.0001"`、`min_qty = "0.0001"`、`min_notional = "5"`)与 `ballast.symbols = ["SOLUSDT", "XRPUSDT"]`
- 实现:`tools/latency-slice/tests/rd.rs::slc_007_refdata_from_config`

| 行 | 断言 | 期望 |
|---|---|---|
| SLC-007.1 | inst id 与符号 | 1 = BTCUSDT、2 = ETHUSDT、3 = SOLUSDT、4 = XRPUSDT(压舱在后),都在 `BINANCE_SPOT` |
| SLC-007.2 | BTC 指数、tick、lot | `px_exp = −2`、`qty_exp = −8`;tick raw 1、lot raw 1 000 |
| SLC-007.3 | ETH | `px_exp = −2`、`qty_exp = −8`;lot raw 10 000 |
| SLC-007.4 | 压舱 inst 3(SOL) | `px_exp = −4`、`qty_exp = −8`;tick raw 1、lot raw 100 000 |
| SLC-007.5 | 账户 | 账户 1 为 Live、`is_prod = false`,只登记在 `BINANCE_SPOT` |
| SLC-007.6 | `caps(BINANCE_SPOT)` 与限速 | 等于 `Spot::default_caps` 与 `Spot::rate_rules()` |
| SLC-007.7 | inst 的 `min_notional` | `Notional` raw 5 000 000 |

### SLC-010 Bbo 到期时挂最小名义的 PostOnly

- 规则:[简化 sequencer](README.md#简化-sequencer)、[安全约束](README.md#安全约束)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0`
- 步骤:
  1. 投递 `B(1000, 6500000, 6500010)`(65000.00 / 65000.10)
- 期望:
  - 输出一条 `GwCmd{op: Place, inst: 1, side: Buy, tif: PostOnly, px: 6305009(63050.09 = floor_to_tick(65000.10 × 0.97)), qty: 9000(0.00009), slot: 1, gen: 1, account: 1, decided_tsc ≠ 0}`(0.00009 × 63050.09 = 5.67 ≥ 5 × 1.1;0.00008 时 5.04 < 5.5)
  - journal 依次有该 `Bbo`、`OrderDecision{action: New, trigger_seq = Bbo 的 seq, slot: 1, gen: 1, px, qty, decided_tsc = GwCmd.decided_tsc}`,后者带 `EVENT_END`
- 实现:`tools/latency-slice/tests/seq.rs::slc_010_bbo_places_postonly`

### SLC-011 同一 inst 两次挂单至少相隔 probe_period_ms

- 规则:[简化 sequencer](README.md#简化-sequencer)
- 级别:table
- 里程碑:M1.5
- 前置:`SEQ0`;`B(1000, …)` 挂出的单已 `CancelAck`;每行再投递一条 `Bbo`
- 实现:`tools/latency-slice/tests/seq.rs::slc_011_probe_period`

| 行 | 第二条 Bbo 的时刻 | 期望 |
|---|---|---|
| SLC-011.1 | 1999.999999 ms | 无 `GwCmd` |
| SLC-011.2 | 2000 ms | 一条 `GwCmd{Place}` |
| SLC-011.3 | 2000.000001 ms | 一条 `GwCmd{Place}` |
| SLC-011.4 | 2500 ms,但第一单仍未终态 | 无 `GwCmd`(该 inst 有在途) |

### SLC-012 突发层按 burst_period_ms 另给配额

- 规则:[简化 sequencer](README.md#简化-sequencer)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0`,`probe.burst_period_ms = 200`;1000 ms 挂出的单已终态
- 步骤:
  1. 投递 `rx_tsc` 不同的两条 Bbo:1300 ms、1301 ms
  2. 投递 `rx_tsc` 相同(1400 ms)的两条 Bbo
  3. 第二步挂出的单终态后,再投递 `rx_tsc` 相同(1500 ms)的两条 Bbo
- 期望:
  - 步骤 1 无 `GwCmd`
  - 步骤 2 第二条 Bbo 产生一条 `GwCmd{Place}`(1400 − 1000 ≥ 200)
  - 步骤 3 无 `GwCmd`(1500 − 1400 < 200)
- 实现:`tools/latency-slice/tests/seq.rs::slc_012_burst_quota`

### SLC-013 不满足挂单条件时不挂

- 规则:[简化 sequencer](README.md#简化-sequencer)
- 级别:table
- 里程碑:M1.5
- 前置:`SEQ0`;每行一处改动后投递 `B(1000, 6500000, 6500010)`
- 实现:`tools/latency-slice/tests/seq.rs::slc_013_no_probe_conditions`

| 行 | 改动 | 期望 |
|---|---|---|
| SLC-013.1 | ask = 6566001(价差 101 bp) | 无 `GwCmd`;`skipped_spread` = 1 |
| SLC-013.2 | ask = 6565000(价差 99.5 bp) | 一条 `GwCmd{Place}` |
| SLC-013.3 | bid = 0 | 无 `GwCmd` |
| SLC-013.4 | gw 下单连接未 `Up` | 无 `GwCmd` |
| SLC-013.5 | `orders.enabled = false` | 无 `GwCmd` |
| SLC-013.6 | `orders.max_open_orders = 1`,另一 inst 已有未终态单 | 无 `GwCmd` |
| SLC-013.7 | `orders.max_orders = 1`,已发过 1 张新单 | 无 `GwCmd` |
| SLC-013.8 | inst 不在配置中(inst 0) | 无 `GwCmd`,不报错 |

### SLC-014 Ack 写新单样本并在同一事件撤单

接收来源由环元素传入；不能以期望 HW 配置或接收戳数值相等推断。

- 规则:[简化 sequencer](README.md#简化-sequencer)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0`;`B(1000, …)` 挂出 slot 1 gen 1,`SendReport{slot: 1, gen: 1, op: Place, status: Written, sent_tsc: 1000.020 ms}` 已投递
- 步骤:
  1. 投递 `OrderEvent{what: Ack, slot: 1, gen: 1, hw_rx_ns: 1001.5 ms − 4 µs, rx_tsc: 1001.5 ms, exch_order_id: 77}`
- 期望:
  - journal:该 `OrderEvent`、`LatencySample{action: New, trigger_seq = Bbo 的 seq, trigger_kind = BBO, ack_seq = 本回报的 seq, hw_rx_ns = 1000 ms − 5 µs, rx_tsc = 1000 ms, sent_tsc = 1000.020 ms, ack_hw_rx_ns = 1001.5 ms − 4 µs, link_role: 0}`、`OrderDecision{action: Cancel, target_slot: 1}`;最后一条带 `EVENT_END`,前两条不带
  - 输出一条 `GwCmd{op: Cancel, target_slot: 1, inst: 1}`
- 实现:`tools/latency-slice/tests/seq.rs::slc_014_ack_cancels_same_event`

### SLC-015 撤单的首个回报写撤单样本,触发记录是 Ack

- 规则:[简化 sequencer](README.md#简化-sequencer)
- 级别:unit
- 里程碑:M1.5
- 前置:SLC-014 之后;撤单的 `SendReport{op: Cancel, target_slot: 1, sent_tsc: 1001.510 ms}` 已投递
- 步骤:
  1. 投递 `OrderEvent{what: CancelAck, slot: 1, gen: 1, hw_rx_ns: 1003 ms, rx_tsc: 1003 ms}`
  2. 再投递同一 `CancelAck`
- 期望:
  - 步骤 1 写一条 `LatencySample{action: Cancel, trigger_seq = Ack 记录的 seq, trigger_kind = ORDER_EVENT, hw_rx_ns = Ack 的 hw_rx_ns, rx_tsc = 1001.5 ms, proc_tsc = Ack 记录的 tsc, sent_tsc = 1001.510 ms, ack_hw_rx_ns = 1003 ms}`;slot 1 空闲
  - 步骤 2 只写该 `OrderEvent`,不写样本
- 实现:`tools/latency-slice/tests/seq.rs::slc_015_cancel_sample`

### SLC-016 拒单结束该单,连续拒单达到上限即安全停机

- 规则:[简化 sequencer](README.md#简化-sequencer)、[安全约束](README.md#安全约束)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0`,`probe.max_consec_fail = 2`
- 步骤:
  1. 挂单(1000 ms)→ `Reject{reject_code: 0xFFFFEC62}`
  2. 挂单(2000 ms)→ `Reject`
  3. 投递 `B(3000, …)`
- 期望:
  - 步骤 1 写 `LatencySample{action: New}`;slot 空闲;未停机
  - 步骤 2 后 `halted = ConsecFail`
  - 步骤 3 无 `GwCmd`
- 实现:`tools/latency-slice/tests/seq.rs::slc_016_reject_and_consec_fail`

### SLC-017 结果未知时立即撤单且不写样本

- 规则:[简化 sequencer](README.md#简化-sequencer)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0`;1000 ms 挂出 slot 1
- 步骤:
  1. 投递 `OrderEvent{what: OutcomeUnknown, slot: 1, gen: 1}`
- 期望:
  - 输出 `GwCmd{op: Cancel, target_slot: 1}`;不写 `LatencySample`
- 实现:`tools/latency-slice/tests/seq.rs::slc_017_outcome_unknown_cancels`

### SLC-018 单笔名义超限时安全停机且不发单

- 规则:[安全约束](README.md#安全约束)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0`,inst `min_notional = "60"`(> `max_order_notional` 50)
- 步骤:
  1. 投递 `B(1000, 6500000, 6500010)`
- 期望:
  - 无 `GwCmd`;`halted = NotionalCap`
- 实现:`tools/latency-slice/tests/seq.rs::slc_018_notional_cap_halts`

### SLC-019 成交类回报触发安全停机并撤掉其余在挂单

- 规则:[安全约束](README.md#安全约束)、[简化 sequencer](README.md#简化-sequencer)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0` 两个 inst(另一个为 `ETHUSDT`);slot 1(inst 1)已 `Ack`、撤单在途,slot 2(inst 2)只发出、未回报
- 步骤:
  1. 投递 `OrderEvent{what: Fill, slot: 1}`
- 期望:
  - `halted = Fill`;输出恰好一条 `GwCmd{Cancel, target_slot: 2}`;此后 `Bbo` 不再产生 `GwCmd{Place}`
- 实现:`tools/latency-slice/tests/seq.rs::slc_019_fill_halts`

### SLC-020 发送失败结束该单且不写样本

- 规则:[简化 sequencer](README.md#简化-sequencer)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0`;1000 ms 挂出 slot 1
- 步骤:
  1. 投递 `SendReport{slot: 1, gen: 1, op: Place, status: NoConn}`
- 期望:
  - slot 1 空闲;`send_fail = 1`;不写 `LatencySample`
- 实现:`tools/latency-slice/tests/seq.rs::slc_020_send_fail`

### SLC-021 超过 quote_rest_ms 未终态时重发撤单,第 3 次后安全停机

- 规则:[简化 sequencer](README.md#简化-sequencer)、[安全约束](README.md#安全约束)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0`,`probe.quote_rest_ms = 2000`;1000 ms 挂出 slot 1,无任何回报
- 步骤:
  1. 在 2999 ms、3000 ms、5000 ms、7000 ms、9000 ms 各推进一次空闲轮
- 期望:
  - 2999 ms 无输出;3000 ms、5000 ms、7000 ms 各一条 `GwCmd{Cancel, target_slot: 1}`;9000 ms `halted = Stuck`
- 实现:`tools/latency-slice/tests/seq.rs::slc_021_rest_timeout`

### SLC-022 收尾撤掉全部在挂单,全部终态后结束

- 规则:[安全约束](README.md#安全约束)、[收尾顺序](README.md#收尾顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0` 两个 inst;slot 1 已 `Ack`(撤单在途)、slot 2 只发出未回报
- 步骤:
  1. 调 `begin_drain(now)`
  2. 投递 slot 1、slot 2 的 `CancelAck`
- 期望:
  - 步骤 1 输出 `GwCmd{Cancel, target_slot: 2}`(slot 1 的撤单已在途,不重发);`drained() = false`
  - 步骤 2 后 `drained() = true`;此后 `Bbo` 不产生 `GwCmd`
- 实现:`tools/latency-slice/tests/seq.rs::slc_022_drain_cancels_all`

### SLC-023 Bbo 记录头的 HW_TS/SW_TS

- 规则:[简化 sequencer](README.md#简化-sequencer)
- 级别:table
- 里程碑:M1.5
- 前置:`SEQ0`;每行投递一条 Bbo,读回其记录头 `flags`
- 实现:`tools/latency-slice/tests/seq.rs::slc_023_bbo_ts_flags`

| 行 | `hw_rx_ns` / `sw_rx_ns`（环元素来源） | 期望 |
|---|---|---|
| SLC-023.1 | 1000 / 2000 或 2000 / 2000（HW_TS） | `HW_TS`,无 `SW_TS` |
| SLC-023.2 | 2000 / 2000（SW_TS） | `SW_TS`,无 `HW_TS` |
| SLC-023.3 | 0 / 2000（无来源） | 两者都无 |

### SLC-024 取环优先级 hk > rep > bbo,一条输入一个事件

- 规则:[简化 sequencer](README.md#简化-sequencer)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0`;三个输入环各放一条:`ClockStatus`、`ConnState{Up}`(rep)、Bbo
- 步骤:
  1. 连续调 `poll` 三次
- 期望:
  - journal 记录顺序为 ClockStatus、ConnState、Bbo(及其 `OrderDecision`);ConnState 与 Bbo 事件各以 `EVENT_END` 收尾;seq 连续递增
- 实现:`tools/latency-slice/tests/seq.rs::slc_024_input_priority_and_events`

### SLC-025 每段段首有 HostProfile,ClockStatus 为 Meta 记录

- 规则:[简化 sequencer](README.md#简化-sequencer)、[产出](README.md#产出)
- 级别:unit
- 里程碑:M1.5
- 前置:`SEQ0`,`journal.segment_mib` 取最小允许值;投递足够多的 Bbo 使 seq journal 至少滚动一次
- 步骤:
  1. 读回整个 journal
- 期望:
  - 每个段的启动块含一条 `HostProfile`,其 `tsc_hz_milli` = 时钟频率
  - `ClockStatus` 记录都在 journal 中且按投递顺序
- 实现:`tools/latency-slice/tests/seq.rs::slc_025_clock_and_host_profile`

### SLC-030 报告从 journal 算出各分段

- 规则:[产出](README.md#产出)
- 级别:integ
- 里程碑:M1.5
- 前置:`SEQ0` 驱动 1 200 次"Bbo → 挂单 → SendReport → Ack → 撤单 → SendReport → CancelAck"循环(每循环 1 s;`decided − proc` = 2 µs、`sent − decided` = 3 µs、Ack 的 `rx_tsc` 比 `sent` 晚 1.5 ms),hk 每 1 000 ms 一条 `ClockStatus`(TSC = REALTIME 恒等、误差界 100 ns);journal 关闭后对其运行 `--report`
- 步骤:
  1. 读 `report.json`
- 期望:
  - `samples` 中 `decide` 的 p50 与 p99 = 2 µs、`egress` = 3 µs(直方图分辨率内);`tick_to_trade`、`rep_to_order`、`order_rtt` 的 n 都 ≥ 1 000
  - `event_weighted` 有 `kernel_rx`、`ingest`、`kernel_rx_hi` 三类条目,n = Bbo 记录数
  - `stratified` 有一组 `tick_to_trade`
  - `journal` 节计数:`decisions_new` = `acks` = `cancel_acks` = 1 200
- 实现:`tools/latency-slice/tests/report.rs::slc_030_report_from_journal`

### SLC-031 HW 与 SW 样本分开统计

- 规则:[产出](README.md#产出)
- 级别:unit
- 里程碑:M1.5
- 前置:同 SLC-030,但一半 Bbo 的 `hw_rx_ns = sw_rx_ns`(SW 回退)
- 步骤:
  1. 运行报告
- 期望:
  - `samples` 中 `action = 0`(新单,行情触发)的 `kernel_rx` 有 `ts_src = Hw` 与 `Sw` 两组,n 各 600;`event_weighted` 的 `kernel_rx` 同样两组,n 各 600
- 实现:`tools/latency-slice/tests/report.rs::slc_031_hw_sw_separate`

### SLC-040 dry-run 跑通全部线程与直方图管线

- 规则:[dry-run](README.md#dry-run)、[线程布局](README.md#线程布局)、[产出](README.md#产出)、[收尾顺序](README.md#收尾顺序)、[概要](README.md#概要)
- 级别:integ
- 里程碑:M1.5
- 前置:`CFG0`(`spot`)与 `CFG0` 改 `slice.product = "usdm"`(inst `tick = "0.1"`、`lot = "0.001"`、`min_qty = "0.001"`、`min_notional = "100"`;`orders.host = "testnet.binancefuture.com"`、`orders.max_order_notional = "500"`)各一次;两者都设 `report.cal_max_err_ns = 1000000`(共享 runner 与虚机的时钟噪声大);`--seconds 3`、`--seed 1`;热段在堆后端
- 步骤:
  1. 以库函数运行 dry-run,直到返回
- 期望:
  - 返回码 0;未安全停机
  - 报告 `samples` 中 `ingest`、`decide`、`egress`、`order_rtt`、`rep_to_order`、`tick_to_trade` 的 n > 0;`event_weighted` 的 `kernel_rx` n > 0
  - `run` 计数:新单 > 0,Ack = 新单,撤单回报 = 新单(收尾后无未终态单)
- 实现:`tools/latency-slice/tests/dry.rs::slc_040_dry_run_pipeline`

### SLC-041 二进制的 dry-run 退出 0 并写出报告

- 规则:[命令行](README.md#命令行)、[dry-run](README.md#dry-run)
- 级别:proc
- 里程碑:M1.5
- 前置:Linux;`/dev/shm` 可写;`CFG0` 写入临时文件,`journal.hot_dir` 为 `/dev/shm` 下的临时目录,`report.cal_max_err_ns = 1000000`
- 步骤:
  1. 运行 `latency-slice --dry-run --seconds 2 --config <tmp> --out <tmp-out>`
- 期望:
  - 退出码 0;`<tmp-out>/report.json` 与 `report.txt` 存在,前者可解析且 `samples` 非空;`<tmp-out>/journal/prod/seq/` 有段文件
- 实现:`tools/latency-slice/tests/proc.rs::slc_041_binary_dry_run`

### SLC-042 --check-config 的退出码

- 规则:[命令行](README.md#命令行)、[安全约束](README.md#安全约束)
- 级别:proc
- 里程碑:M1.5
- 前置:Linux;`CFG0` 写入临时文件;子进程环境 `HFT_SLICE_API_KEY = "TESTKEY"`、`HFT_SLICE_ED25519` = RFC 8032 测试 1 私钥的十六进制种子
- 步骤:
  1. 运行 `latency-slice --check-config --config <tmp>`
  2. 把 `orders.host` 改为 `ws-api.binance.com` 再运行
- 期望:
  - 步骤 1 退出码 0;步骤 2 退出码 2,标准错误含 `mainnet`
- 实现:`tools/latency-slice/tests/proc.rs::slc_042_check_config_exit_codes`

### SLC-043 回环探测模式的输出格式

- 规则:[host 探测模式](README.md#host-探测模式)
- 级别:proc
- 里程碑:M1.5
- 前置:Linux
- 步骤:
  1. 运行 `latency-slice --two-packet-stamp-probe 5`
  2. 运行 `latency-slice --loopback-stream-minutes 0`
- 期望:
  - 步骤 1 退出码 0,输出 5 行 `i=<k> sw_rx_ns=…` 与末行 `within_50us_of_second=`
  - 步骤 2 退出码 0,末行以 `reads=` 开头
- 实现:`tools/latency-slice/tests/proc.rs::slc_043_loopback_probes`

### SLC-100 M1.5 主运行:主网 BBO + 测试网下单出第一份 Graviton 直方图

- 规则:[概要](README.md#概要)、[线程布局](README.md#线程布局)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:testnet
- 里程碑:M1.5
- 环境:M1.5 租用的 `c8g.4xlarge`(az-probe 选定的 AZ),按 [host-tuning C 段](../../docs/runbooks/host-tuning.md)调好(隔离核 4–11、ENI-B IRQ 在核 1);测试网/演示环境 key 已按[密钥](README.md#密钥)放好
- 授权:`[需授权]`(租用与删除实例由负责人执行)
- 步骤:
  1. `tests/host/SLC-100.sh`:记录 `ps -eLo tid,psr,comm` 的线程落点,运行 `latency-slice --minutes 60 --print-conn-stats`
  2. 检查 `report.json`
- 通过标准:退出码 0;各热线程落在配置的核上;`samples` 中 HW(或 HW 不可用时 SW)数据集的 `kernel_rx`、`ingest`、`decide`、`egress`、`tick_to_trade`、`rep_to_order`、`order_rtt` 各组 n ≥ 1 000,`event_weighted` 的 `kernel_rx` 两层都有样本;报告附 `tsc_hz_nominal` 与 `cal_err` 分布
- 证据:`SLC-100-<host>-<日期>/`(报告、线程落点、hostprofile.json),附在 M1.5 探针 Agent Note
- 实现:`tests/host/SLC-100.sh`

### SLC-101 压舱深度连接下的行情分段 A/B

- 规则:[线程布局](README.md#线程布局)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M1.5
- 环境:同 SLC-100
- 授权:`[需授权]`
- 步骤:
  1. `tests/host/SLC-101.sh`:`--ballast 0` 与 `--ballast 4`(`ballast.symbols` 为主网成交额前 200 的 USDT 交易对)各运行 30 min,交替两轮
- 通过标准:四份报告齐全;两种条件的 `event_weighted` `kernel_rx`(上下界)与 `nic_to_kernel` 的 p50/p99 按层并列写进结论
- 证据:`SLC-101-<host>-<日期>/`,附在 M1.5 探针 Agent Note
- 实现:`tests/host/SLC-101.sh`

### SLC-102 IRQ 核空闲与保持唤醒的 A/B

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M1.5
- 环境:同 SLC-100;IRQ 核"保持唤醒"只用 hostprobe 规则登记的允许形式([tools hostprobe](../README.md#hostprobe))
- 授权:`[需授权]`
- 步骤:
  1. `tests/host/SLC-102.sh`:IRQ 核空闲(默认)与保持唤醒(`KEEPAWAKE_CMD` 给出的允许形式)各运行 30 min(都带压舱)
- 通过标准:两份报告齐全;`kernel_rx`、`nic_to_kernel` 与 `tick_to_trade` 的对比写进结论
- 证据:`SLC-102-<host>-<日期>/`,附在 M1.5 探针 Agent Note
- 实现:`tests/host/SLC-102.sh`

## 验收

- 里程碑 ≤ 当前里程碑的 unit/table/integ/proc 用例全部有同编号实现且通过([docs/tests §6](../../docs/tests/README.md#6-与代码的对应));proc 用例只在 Linux 上运行。
- M1.5 探针:SLC-100–SLC-102 与 hft-net 的 NET-370–NET-374 在租用实例上完成,证据归档进 M1.5 探针 Agent Note。
