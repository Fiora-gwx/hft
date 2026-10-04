---
component: treasury
prefix: TRS
spec: README.md
---

# treasury 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | TRS-004、TRS-096、TRS-110 |
| 目录 | 不适用:非规范 |
| 使用本组件 | TRS-097、TRS-099、TRS-102 |
| 输入 | TRS-006、TRS-013–TRS-025、TRS-042、TRS-081、TRS-131 |
| 计算 | TRS-010–TRS-012、TRS-030–TRS-043、TRS-050–TRS-054、TRS-060–TRS-071、TRS-080、TRS-090、TRS-130 |
| 划转通道表 | TRS-060、TRS-064、TRS-066 |
| 输出与登记 | TRS-014、TRS-016、TRS-082–TRS-085、TRS-091–TRS-101、TRS-130、TRS-141、TRS-142 |
| 模拟账户 | TRS-105 |
| 链上钱包(optional) | 不适用:里程碑外 |
| 配置文件字段 | TRS-001–TRS-006、TRS-040、TRS-070、TRS-071、TRS-082、TRS-100、TRS-140 |
| 实现说明 | TRS-005、TRS-014、TRS-035、TRS-042、TRS-090、TRS-140 |
| 延迟与资源契约 | TRS-120 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | TRS-096、TRS-110–TRS-112 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**夹具**(实现在 `python/treasury/tests/conftest.py`;各用例的"前置"按名字引用,参数一律显式赋值,不取默认值;默认值只在 TRS-001 出现):

- 记法:金额是 USDT,raw = 十进制 × 10^6(`Notional`,指数 −6);比例 raw = 十进制 × 10^9(`Rate`,指数 −9);例:5 214 USDT(raw 5 214 000 000,−6)、0.20(raw 200 000 000,−9),之后只写十进制。时刻以 `T0 = 2026-10-01T00:00:00Z` 为原点,`T0 + 60 s` 写作 `t = 60 s`。"误差 ≤ 0.1 个百分点"即 `|输出 − 期望| ≤ 1 000 000`(raw)。
- `rd_t`:测试 refdata。资产 `USDT`、`USDC`(计价资产)、`BTC`、`ETH`;venue `SP1`(`VenueId(1)`,现货,交易所 E1)、`PP2`(`VenueId(2)`,USDT 永续,交易所 E2)、`PP3`(`VenueId(3)`,USDT 永续,交易所 E3)、`SP4`(`VenueId(4)`,现货,交易所 E4);inst `BTC/SP1`、`BTC/SP4`、`BTC/PP2`、`ETH/PP2`、`BTC/PP3`;账户 `A1`(Live,{SP1, PP2})、`A2`(Live,{PP2, PP3})、`A3`(Live,{SP1, PP2, SP4})、`A4`(Live,{SP1, PP2})、`S9`(Sim,{SP1, PP2},只在 `sim` 实例)。
- `risk_t`:`RiskParams` 显式赋值——`liq_dist_block_open = 0.25`、`liq_dist_reduce = 0.15`、`liq_dist_force = 0.10`、`liq_dist_hyst = 0.02`、`margin_ratio_crosscheck = 0.80`、`margin_ratio_hyst = 0.05`、`margin_stale_ms`(PP2、PP3)= 15 000、`max_perp_leverage_x100 = 500`。
- `pol_t`:`cycle_s = 60`;policy `target_headroom = 0.20`、`min_headroom = 0.10`、`stress_max_move = 3.0`、`reserve_usd = {spot: 500, perp: 1 000}`、`min_transfer_usd = 10`、`max_cost_ratio_bp = 100`、`renotify_min = 30`、`channels = {internal: {fixed_usd: 0, ratio_bp: 0, latency_s: 5}, cross: {fixed_usd: 1, ratio_bp: 0, latency_s: 600}, sub: {fixed_usd: 0, ratio_bp: 0, latency_s: 5}}`;部署项 `hft.instances = [prod, sim]`(路径指向测试 journal 夹具)、`reconciler.state` 指向夹具目录。
- `short1(mb, maint, avail, liq_px, ratio)`:`A1/PP2` 永续空头 `BTC/PP2` 1.0(入场 60 000,mark 60 000,`N` = 60 000);ledger 保证金钱包使 `est_margin_balance = mb`;`MarginReport{rep_margin_balance = mb, rep_maint = maint, rep_avail = avail, rep_ratio = ratio, items = [{BTC/PP2, liq_px}]}`,接收时刻 `T0 − 5 s`。缺省实参写作 `short1()` = `(18 000, 300, 6 000, 77 611.94, 300 / 18 000)`(保证金占名义 e = 0.30、维持保证金率 m = 0.5 %)。
- `long1(mb, maint, avail, liq_px, ratio)`:同上,但为多头 1.0;`long1()` = `(18 000, 100, 6 000, 42 211.06, 100 / 18 000)`。
- `spot1(free_usdt)`:`A1/SP1` 现金 `USDT` free = `free_usdt`、locked 0,`BTC` 0;`BalanceReport` 与 ledger 一致,接收时刻 `T0 − 5 s`。
- `round_at(t)`:以 `round_ts = t` 调纯函数 `plan(inputs, policy)`(`src/hft_treasury/plan.py`),`inputs` 由 `inputs.py` 从上述夹具组装;`run_round(t)` 是带 I/O 的一轮(读 `fixtures/replay/treasury/` 下由 hft-journal 测试写者 `examples/testwriter.rs` 生成的 journal 夹具、写 `tmp_path` 状态目录)。
- 无 `MarginStress` cap(`engine_level = 0`)、无 `RecordTransfer`、无 `ReconcileReport` 差异、`AccountConfigReport` 给出 `BTC/PP2`、`BTC/PP3` 杠杆 3.00×,除非用例另写。

### TRS-001 配置各键的默认值

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:只含必填键的最小配置:`hft.instances`、`reconciler.state`、`policy.target_headroom = 0.20`、`policy.min_headroom = 0.10`、`policy.reserve_usd = {spot: 500, perp: 1 000}`、`policy.channels` 同 `pol_t`(本用例是默认值的唯一用例)
- 实现:`python/treasury/tests/test_config.py::test_trs_001_defaults`

| 行 | 键 | 期望 |
|---|---|---|
| TRS-001.1 | `cycle_s` | 60 |
| TRS-001.2 | `policy.stress_max_move` | 3.0(raw 3 000 000 000) |
| TRS-001.3 | `policy.min_transfer_usd` | 10(raw 10 000 000) |
| TRS-001.4 | `policy.max_cost_ratio_bp` | 100 |
| TRS-001.5 | `policy.renotify_min` | 30 |

### TRS-002 缺少必填键时启动失败

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`pol_t` 的完整配置,每行删去一个键;以 `hft-treasury run --config <文件>` 启动
- 实现:`python/treasury/tests/test_config.py::test_trs_002_missing_required`

| 行 | 删去的键 | 期望 |
|---|---|---|
| TRS-002.1 | `hft.instances` | 退出码 2;stderr 含 `hft.instances`;状态目录无 `latest.json` |
| TRS-002.2 | `reconciler.state` | 退出码 2;stderr 含 `reconciler.state` |
| TRS-002.3 | `policy.target_headroom` | 退出码 2;stderr 含 `target_headroom` |
| TRS-002.4 | `policy.min_headroom` | 退出码 2;stderr 含 `min_headroom` |
| TRS-002.5 | `policy.reserve_usd` | 退出码 2;stderr 含 `reserve_usd` |
| TRS-002.6 | `policy.channels` | 退出码 2;stderr 含 `channels` |
| TRS-002.7 | 不删(对照) | 进入运行并写出 `latest.json` |

### TRS-003 余量阈值的次序与 `cycle_s` 在加载时校验

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`,每行只改所列的键;`stress_max_move = 3.0`
- 实现:`python/treasury/tests/test_config.py::test_trs_003_threshold_order`

| 行 | 改动 | 期望 |
|---|---|---|
| TRS-003.1 | `min_headroom = 0` | 退出码 2 |
| TRS-003.2 | `min_headroom = 0.000000001`(raw 1) | 加载成功 |
| TRS-003.3 | `min_headroom = 0.20`(= `target_headroom`) | 加载成功 |
| TRS-003.4 | `min_headroom = 0.200000001`(`target_headroom` + 1 raw) | 退出码 2 |
| TRS-003.5 | `target_headroom = 3.0`(= `stress_max_move`) | 加载成功 |
| TRS-003.6 | `target_headroom = 3.000000001` | 退出码 2 |
| TRS-003.7 | `cycle_s = 0` | 退出码 2 |
| TRS-003.8 | `cycle_s = 1` | 加载成功 |

### TRS-004 配置中出现像 key 的键时启动失败

- 规则:[配置文件字段](README.md#配置文件字段)、[概要](README.md#概要)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`,每行在部署项或 policy 中加一个键(值为任意字符串)
- 实现:`python/treasury/tests/test_config.py::test_trs_004_key_fields_rejected`

| 行 | 加入的键 | 期望 |
|---|---|---|
| TRS-004.1 | 部署项 `exchange.api_key` | 退出码 2;stderr 含 `api_key` |
| TRS-004.2 | policy `Secret` | 退出码 2 |
| TRS-004.3 | 部署项 `okx.PASSPHRASE` | 退出码 2 |
| TRS-004.4 | policy `channels.cross.withdraw_key` | 退出码 2 |
| TRS-004.5 | 部署项 `reconciler.state`(已有键,对照) | 加载成功 |

### TRS-005 运行模式每 `cycle_s` 秒一轮

- 规则:[实现说明](README.md#实现说明)、[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`(`cycle_s = 60`);假时钟从 `t = 0` 起;`run` 的调度器注入假时钟
- 步骤:
  1. 推进到 `t = 59.999 s`
  2. 推进到 `t = 60.000 s`
  3. 推进到 `t = 120.000 s`
- 期望:
  - 步骤 1 后 `plan` 被调用 1 次(`round_ts = T0`)
  - 步骤 2 后被调用 2 次,第 2 次 `round_ts = T0 + 60 s`
  - 步骤 3 后被调用 3 次;`latest.json` 的 `round_ts = T0 + 120 s`
- 实现:`python/treasury/tests/test_run.py::test_trs_005_cycle_schedule`

### TRS-006 两个实例的账户都参与;一个实例读不到时只影响它的账户

- 规则:[配置文件字段](README.md#配置文件字段)、[输入](README.md#输入)
- 级别:integ
- 里程碑:M5
- 前置:`pol_t`;`prod` 夹具含 `A1`(`short1()`、`spot1(20 000)`),`sim` 夹具含 `S9`(同样的数据)
- 步骤:
  1. `run_round(T0)`
  2. 把 `sim` 实例的 `journal_cold` 指向不存在的目录,`run_round(T0 + 60 s)`
- 期望:
  - 步骤 1 后 `latest.json.accounts` 恰有 `A1`(`kind = Live`)与 `S9`(`kind = Sim`)两项,`status` 都是 `ok`
  - 步骤 2 后 `A1.status = ok`;`S9` 只有 `account`、`kind`、`status = incomplete`、`notes` 四个键,`notes` 含 `missing:journal`;进程未退出
- 实现:`python/treasury/tests/test_run.py::test_trs_006_two_instances`

### TRS-010 位置:合约钱包与现货钱包输出,待转入位置不输出

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`、`spot1(20 000)`
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `A1.locations` 的键恰为 `SP1`、`PP2` 两个(无 E2 资金/现货钱包)
  - `explain` 输出中 E2 的待转入位置只出现在配对说明里,`B`、`S` 字段不存在
- 实现:`python/treasury/tests/test_plan.py::test_trs_010_locations`

### TRS-011 现货钱包的可用现金是各计价资产 free 折 USDT 之和

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`A1/SP1` 现金 `USDT` free 20 000、locked 300,`USDC` free 1 000;ledger 的 `USDC` 换算率 0.9999;`BTC` 0.5(基础资产,不计入)
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `A1.locations.SP1.balance` = 20 999.9(raw 20 999 900 000);`reserve` = 500;`surplus` = 20 499.9
- 实现:`python/treasury/tests/test_plan.py::test_trs_011_spot_balance`

### TRS-012 合约钱包的可用现金取 MarginReport 的可用保证金

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1(mb = 18 000, maint = 300, avail = 6 000, liq_px = 77 611.94, ratio = 300 / 18 000)`;ledger `est_margin_balance` = 18 000
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `A1.locations.PP2.balance` = 6 000;`margin_ratio` = 0.016666667(raw 16 666 667);`data_age_ms` = 5 000
- 实现:`python/treasury/tests/test_plan.py::test_trs_012_margin_balance`

### TRS-013 MarginReport 年龄超过 `margin_stale_ms` 才算过期

- 规则:[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`(`margin_stale_ms = 15 000`);`short1()`、`spot1(20 000)`;每行改 `MarginReport` 的接收时刻
- 实现:`python/treasury/tests/test_inputs.py::test_trs_013_stale_boundary`

| 行 | 这一轮的时刻 − 接收时刻 | 期望 |
|---|---|---|
| TRS-013.1 | 14 999 ms | `status = ok`;`locations.PP2.data_age_ms = 14 999` |
| TRS-013.2 | 15 000 ms | `status = ok` |
| TRS-013.3 | 15 001 ms | `status = stale`;无 `suggestions` 键 |

### TRS-014 数据过期的账户只输出状态,不给建议、不更新当日最低值

- 规则:[输入](README.md#输入)、[实现说明](README.md#实现说明)、[输出与登记](README.md#输出与登记)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`、`spot1(20 000)`;第 1 次计算正常;第 2 次计算前把 `MarginReport` 接收时刻改为 `t − 20 s`(过期),并把 mark 改为 70 000
- 步骤:
  1. `round_at(T0)`
  2. `round_at(T0 + 60 s)`
  3. 新的 `MarginReport` 于 `T0 + 115 s` 到达、mark 回到 60 000,`round_at(T0 + 120 s)`
- 期望:
  - 步骤 2 的 `A1` 恰有 `account`、`kind`、`status`、`notes` 四个键,`status = stale`;`notes` 含 `stale:MarginReport:PP2`
  - 步骤 3 后 `status = ok`,`day_min_l2.up` = 0.124810725(误差 ≤ 0.1 个百分点;步骤 2 按 mark 70 000 本会得到 0,未计入)
- 实现:`python/treasury/tests/test_plan.py::test_trs_014_stale_outputs_status_only`

### TRS-015 必需输入缺失时状态为 incomplete

- 规则:[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`、`spot1(20 000)`;每行去掉一项输入
- 实现:`python/treasury/tests/test_inputs.py::test_trs_015_required_missing`

| 行 | 去掉的输入 | 期望 |
|---|---|---|
| TRS-015.1 | `SP1` 的余额(ledger 中该 venue 未初始化) | `A1.status = incomplete`;`notes` 含 `missing:balance:SP1`;无 `suggestions` 键 |
| TRS-015.2 | `PP2` 的 `MarginReport`(从未收到) | `status = incomplete`;`notes` 含 `missing:MarginReport:PP2` |
| TRS-015.3 | `BTC/PP2` 的 mark | `status = incomplete`;`notes` 含 `missing:mark:BTC/PP2` |
| TRS-015.4 | `RiskParams`(journal 中无 `ConfigBlob{RiskParams}`) | 全部账户 `status = incomplete`;`notes` 含 `missing:RiskParams` |

### TRS-016 过期与缺失同时出现时取 incomplete

- 规则:[输出与登记](README.md#输出与登记)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`(`margin_stale_ms = 15 000`);`short1()` 的 `MarginReport` 接收时刻 `T0 − 20 s`;`SP1` 余额缺失
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `A1.status = incomplete`;`notes` 同时含 `missing:balance:SP1` 与 `stale:MarginReport:PP2`
- 实现:`python/treasury/tests/test_plan.py::test_trs_016_incomplete_wins`

### TRS-017 可选输入缺失只记 notes,照常计算

- 规则:[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`、`spot1(20 000)`
- 实现:`python/treasury/tests/test_inputs.py::test_trs_017_optional_missing`

| 行 | 缺失的输入 | 期望 |
|---|---|---|
| TRS-017.1 | `AccountConfigReport` | `status = ok`;`notes` 含 `missing:AccountConfigReport`;`suggestions` 与 TRS-060 相同 |
| TRS-017.2 | `ReconcileReport`(`reconciler.state` 目录为空) | `status = ok`;`notes` 含 `missing:ReconcileReport`;`suggestions` 与 TRS-060 相同 |

### TRS-018 交易中的永续 inst 杠杆未知或超限时标出

- 规则:[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`(`max_perp_leverage_x100 = 500`);`short1()`、`spot1(20 000)`;`A1` 当前生效参数有一个 group,腿为 `BTC/SP1` 与 `BTC/PP2`;`A1/PP2` 另持 `ETH/PP2` 空头 0.1(不在任何 group 的腿中)
- 实现:`python/treasury/tests/test_inputs.py::test_trs_018_leverage_flags`

| 行 | `AccountConfigReport` 中 `BTC/PP2` / `ETH/PP2` 的杠杆 | 期望 `notes` |
|---|---|---|
| TRS-018.1 | 5.00× / 20.00× | 不含任何 `lev:` 项(`ETH/PP2` 不是交易中的 inst) |
| TRS-018.2 | 5.01× / 3.00× | 含 `lev:PP2:BTC/PP2` |
| TRS-018.3 | 未列出 / 3.00× | 含 `lev:PP2:BTC/PP2` |
| TRS-018.4 | 0 / 3.00× | 含 `lev:PP2:BTC/PP2` |
| TRS-018.5 | 4.99× / 3.00× | 不含 `lev:` 项 |

### TRS-019 杠杆核对不改变余量与建议

- 规则:[输入](README.md#输入)
- 级别:unit
- 里程碑:M5
- 前置:同 TRS-018,两次计算分别用 TRS-018.1 与 TRS-018.2 的杠杆
- 步骤:
  1. 两次 `round_at(T0)`
- 期望:
  - 两次输出除 `notes` 外逐字节相同(`headroom`、`locations`、`suggestions` 相同)
- 实现:`python/treasury/tests/test_inputs.py::test_trs_019_leverage_explain_only`

### TRS-020 对账的余额条目以交易所值为准并标注

- 规则:[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`;`spot1(20 000)`;最近一轮 `ReconcileReport` 含条目 `{item = Balance, venue = SP1, asset = USDT, 交易所值 = 19 000}`,每行改 `severity`
- 实现:`python/treasury/tests/test_inputs.py::test_trs_020_recon_override`

| 行 | `severity` | 期望 |
|---|---|---|
| TRS-020.1 | `Warn` | `locations.SP1.balance = 19 000`;`notes` 含 `recon:SP1:USDT` |
| TRS-020.2 | `Error` | `balance = 19 000`;`notes` 含 `recon:SP1:USDT` |
| TRS-020.3 | `Ignore` | `balance = 20 000`;`notes` 无 `recon:` 项 |

### TRS-021 强平价取交易所值与账本估计中更保守者

- 规则:[输入](README.md#输入)、[计算](README.md#计算)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1(mb = 18 000, maint = 300, avail = 6 000, liq_px = 每行, ratio = 300 / 18 000)`(账本估计 `est_liq_dist` = 0.293532338);`spot1(20 000)`
- 实现:`python/treasury/tests/test_scenario.py::test_trs_021_conservative_liq`

| 行 | 交易所 `liq_px` | 期望 `explain` 的上涨方向 `s_liq` 来源与值 | 期望 `headroom.up.L2` |
|---|---|---|---|
| TRS-021.1 | 75 000(距离 0.25) | `exchange`,0.25 | 0.086956522(误差 ≤ 0.1 个百分点) |
| TRS-021.2 | 80 000(距离 0.333333) | `ledger`,0.293532338 | 0.124810729(误差 ≤ 0.1 个百分点) |

### TRS-022 数据年龄是这一轮的时刻减接收时刻

- 规则:[输入](README.md#输入)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`(接收 `T0 − 5 s`);`spot1(20 000)` 的 `BalanceReport` 接收于 `T0 − 61.5 s`
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `locations.PP2.data_age_ms = 5 000`;`locations.SP1.data_age_ms = 61 500`
- 实现:`python/treasury/tests/test_inputs.py::test_trs_022_data_age`

### TRS-023 已登记的划转计为在途,不被重复建议,也不计入情景余量

- 规则:[输入](README.md#输入)、[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`(报告 `rep_margin_balance = 18 000`、`rep_avail = 6 000`);journal 在 `T0 − 120 s` 有 `RecordTransfer{A1, from = SP1, to = PP2, USDT, 5 214}`,ledger 因此 `PP2` 的 `est_margin_balance` = 23 214、`SP1` 的 USDT free = 14 786;`SP1` 的 `BalanceReport` 为 14 786
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `locations.PP2.in_transit = 5 214`;`locations.PP2.demand = 0`;`locations.PP2.surplus = 0`
  - `suggestions` 中没有 `to = PP2` 的项
  - `headroom.up.L2` = 0.124810725(误差 ≤ 0.1 个百分点;在途的 5 214 不计)
  - `locations.SP1.in_transit = 0`;`locations.SP1.surplus = 14 286`
- 实现:`python/treasury/tests/test_transit.py::test_trs_023_in_transit_not_resuggested`

### TRS-024 目的位置的报告追上账本后视为到账,之后不再回到在途

- 规则:[输入](README.md#输入)
- 级别:unit
- 里程碑:M5
- 前置:同 TRS-023;之后依次到达 `PP2` 的 `MarginReport`(`rep_margin_balance` 见步骤),ledger 值保持 23 214
- 步骤:
  1. 报告 20 607(差 2 607 = 登记额一半),`round_at(T0 + 60 s)`
  2. 报告 20 607.000001(差 2 606.999999),`round_at(T0 + 120 s)`
  3. 报告 18 000(差 5 214),`round_at(T0 + 180 s)`
- 期望:
  - 步骤 1 后 `in_transit = 2 607`
  - 步骤 2 后 `in_transit = 0`
  - 步骤 3 后 `in_transit = 0`(该笔已到账,不再计入)
- 实现:`python/treasury/tests/test_transit.py::test_trs_024_arrival_boundary`

### TRS-025 在途额不超过尚未到账的登记额之和

- 规则:[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()` 的报告 `rep_margin_balance = 18 000`;journal 中 `RecordTransfer` 见各行(到 `PP2`,USDT)
- 实现:`python/treasury/tests/test_transit.py::test_trs_025_transit_cap`

| 行 | 登记 | ledger 值 − 报告值 | 期望 `in_transit` |
|---|---|---|---|
| TRS-025.1 | 一笔 5 214 | 6 000 | 5 214 |
| TRS-025.2 | 无登记 | 6 000 | 0 |
| TRS-025.3 | 两笔 1 000、4 000 | 1 500 | 1 000(4 000 那笔已到账:1 500 < 2 000;1 000 那笔未到账:1 500 ≥ 500) |
| TRS-025.4 | 两笔 1 000、4 000 | 400 | 0(两笔都已到账) |

### TRS-030 单所永续空头的情景金样

- 规则:[计算](README.md#计算)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`;`spot1(20 000)`;`s_liq` = 77 611.94 / 60 000 − 1 = 0.293532333(账本估计 0.293532338,交易所值更保守);比率交叉校验的变动 = 0.291925466(大于距离换算的 L2,不起作用)
- 实现:`python/treasury/tests/test_scenario.py::test_trs_030_short_golden`

| 行 | 输出项 | 期望(误差 ≤ 0.1 个百分点) |
|---|---|---|
| TRS-030.1 | `headroom.up.L1` | 0.034825867(+3.48 %) |
| TRS-030.2 | `headroom.up.L2` | 0.124810725(+12.48 %) |
| TRS-030.3 | `headroom.up.L3` | 0.175938485(+17.59 %) |
| TRS-030.4 | `headroom.up.approximate` / `capped` | `false` / `false` |
| TRS-030.5 | `headroom.down` | L1、L2、L3 均为 `null`(无多头) |
| TRS-030.6 | `explain` 的上涨方向 `s_liq` | 来源 `exchange`,0.293532333 |

### TRS-031 单所永续多头的情景金样(交易所强平价起作用)

- 规则:[计算](README.md#计算)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`long1()`(`rep_maint = 100`,账本估计 0.297836938 不如交易所值保守);`spot1(20 000)`;`s_liq` = 42 211.06 / 60 000 − 1 = −0.296482333(即 (m − e)/(1 − m),m = 0.5 %、e = 0.30)
- 实现:`python/treasury/tests/test_scenario.py::test_trs_031_long_golden`

| 行 | 输出项 | 期望(误差 ≤ 0.1 个百分点) |
|---|---|---|
| TRS-031.1 | `headroom.down.L1` | −0.061976444(−6.20 %) |
| TRS-031.2 | `headroom.down.L2` | −0.172332157(−17.23 %) |
| TRS-031.3 | `headroom.down.L3` | −0.218313704(−21.83 %) |
| TRS-031.4 | `headroom.up` | L1、L2、L3 均为 `null` |
| TRS-031.5 | `explain` 的下跌方向 `s_liq` | 来源 `exchange`,−0.296482333 |

### TRS-032 多头的账本估计更保守时取账本估计

- 规则:[输入](README.md#输入)、[计算](README.md#计算)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`long1(mb = 18 000, maint = 300, avail = 6 000, liq_px = 42 211.06, ratio = 300 / 18 000)`(账本估计 `est_liq_dist` = 0.293532338,比交易所的 0.296482333 更近)
- 实现:`python/treasury/tests/test_scenario.py::test_trs_032_long_ledger_binding`

| 行 | 输出项 | 期望(误差 ≤ 0.1 个百分点) |
|---|---|---|
| TRS-032.1 | `explain` 的下跌方向 `s_liq` | 来源 `ledger`,−0.293532338 |
| TRS-032.2 | `headroom.down.L1` | −0.058043118 |
| TRS-032.3 | `headroom.down.L2` | −0.168861574 |
| TRS-032.4 | `headroom.down.L3` | −0.215035931 |

### TRS-033 多空分在两个 venue 时两个方向各取自己的持仓

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;账户 `A2`:`PP2` 同 `short1()` 的空头与报告,`PP3` 同 `long1()` 的多头与报告(各自的 venue)
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `A2.headroom.up` 的 L1/L2/L3 = 0.034825867 / 0.124810725 / 0.175938485(来自 `PP2`)
  - `A2.headroom.down` 的 L1/L2/L3 = −0.061976444 / −0.172332157 / −0.218313704(来自 `PP3`)
  - 误差均 ≤ 0.1 个百分点
- 实现:`python/treasury/tests/test_scenario.py::test_trs_033_two_directions`

### TRS-034 同一方向多个 venue 时取最危险的持仓

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;账户 `A2`:`PP2` 同 `short1()`;`PP3` 空头 `BTC/PP3` 1.0(mark 60 000),报告 `rep_margin_balance = 12 000`、`rep_maint = 300`、`rep_avail = 2 000`、`liq_px = 71 641.79`(`s_liq` = 0.194029833)
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `A2.headroom.up.L1 = 0`(当前距离 0.194 已低于 0.25,已越过 L1)
  - `A2.headroom.up.L2` = 0.038286812、`L3` = 0.085481667(误差 ≤ 0.1 个百分点;来自 `PP3`)
- 实现:`python/treasury/tests/test_scenario.py::test_trs_034_most_dangerous`

### TRS-035 同一保证金池多个永续持仓时用保证金模型并标近似

- 规则:[计算](README.md#计算)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`A1/PP2` 空头 `BTC/PP2` 1.0(mark 60 000)与空头 `ETH/PP2` 10(mark 3 000),`N` = 90 000;报告 `rep_margin_balance = 27 000`、`rep_maint = 450`、`rep_ratio = 450 / 27 000`,两条 `liq_px` 都给出;ledger `est_margin_balance` = 27 000
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `explain` 的上涨方向 `s_liq` 来源 `model`,值 0.293532338(= (27 000 − 450)/(90 000 + 450),误差 ≤ 0.1 个百分点)
  - `headroom.up.approximate = true`;`headroom.up.L2` = 0.124810729(误差 ≤ 0.1 个百分点)
- 实现:`python/treasury/tests/test_scenario.py::test_trs_035_pool_model`

### TRS-036 缺交易所强平价时用保证金模型并标近似

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`,但报告的 `items` 为空(无 `liq_px`)
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `explain` 的上涨方向 `s_liq` 来源 `model`,0.293532338(误差 ≤ 0.1 个百分点)
  - `headroom.up.approximate = true`;L1/L2/L3 与 TRS-030 的值之差均 ≤ 0.1 个百分点
- 实现:`python/treasury/tests/test_scenario.py::test_trs_036_missing_liq_px`

### TRS-037 模型的保证金余额取报告值与账本估计中较小者

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()` 但报告 `rep_margin_balance = 17 000`、`items` 为空;ledger `est_margin_balance` = 18 000
- 步骤:
  1. `round_at(T0)`
- 期望:
  - 上涨方向 `s_liq` = 0.276948590(= (17 000 − 300)/60 300,误差 ≤ 0.1 个百分点)
  - `headroom.up.L2` = 0.110390078(误差 ≤ 0.1 个百分点)
- 实现:`python/treasury/tests/test_scenario.py::test_trs_037_model_mb_min`

### TRS-038 比率交叉校验先到时 L2 取比率给出的变动

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`(`margin_ratio_crosscheck = 0.80`);`short1(mb = 18 000, maint = 300, avail = 6 000, liq_px = 77 611.94, ratio = 0.70)`(交易所报告比率 0.70,`M(0)` = max(300, 0.70 × 18 000) = 12 600)
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `headroom.up.L2` = 0.029702970(+2.97 %,误差 ≤ 0.1 个百分点)
  - `headroom.up.L1` = 0.034825867、`L3` = 0.175938485(不受比率影响,误差 ≤ 0.1 个百分点)
  - 存在建议时其 `urgency = urgent`(0.0297 < `min_headroom` 0.10)
- 实现:`python/treasury/tests/test_scenario.py::test_trs_038_ratio_binds`

### TRS-039 已处于或越过的档输出 0

- 规则:[计算](README.md#计算)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1(mb = 18 000, maint = 300, avail = 6 000, liq_px = 每行, ratio = 300 / 18 000)`
- 实现:`python/treasury/tests/test_scenario.py::test_trs_039_already_beyond`

| 行 | `liq_px`(上涨方向 `s_liq`) | 期望 `headroom.up` L1 / L2 / L3(误差 ≤ 0.1 个百分点) |
|---|---|---|
| TRS-039.1 | 72 000(0.20) | 0 / 0.043478261 / 0.090909091 |
| TRS-039.2 | 69 000(0.15) | 0 / 0 / 0.045454545 |
| TRS-039.3 | 65 000(0.083333) | 0 / 0 / 0 |

### TRS-040 求不到解的方向输出上限并标 capped

- 规则:[计算](README.md#计算)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`A1/PP2` 空头 `BTC/PP2` 0.01(mark 60 000,`N` = 600),报告 `rep_margin_balance = 每行`、`rep_maint = 3`、`items` 为空
- 实现:`python/treasury/tests/test_scenario.py::test_trs_040_capped`

| 行 | `rep_margin_balance`(模型 `s_liq`) | `stress_max_move` | 期望 |
|---|---|---|---|
| TRS-040.1 | 10 000(16.58) | 3.0 | `up.L1 = up.L2 = up.L3 = 3.0`;`capped = true` |
| TRS-040.2 | 1 812(3.000) | 3.0 | `up.L3` = 2.636(= 4.000/1.10 − 1,误差 ≤ 0.1 个百分点);`capped = false` |
| TRS-040.3 | 10 000 | 20.0 | `up.L1` = 13.063018(误差 ≤ 0.1 个百分点);`capped = false` |

### TRS-041 没有永续持仓的账户两个方向都是无强平风险

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;账户 `A4` 只有 `SP1` 现金 USDT 10 000 与 `BTC` 1.0,`PP2` 无持仓、报告 `rep_margin_balance = 0`、`rep_avail = 0`
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `headroom.up` 与 `headroom.down` 的 L1、L2、L3 均为 `null`;`approximate = false`
  - `locations.PP2.demand ≤ 0`;`suggestions` 中没有 `to = PP2` 的项
- 实现:`python/treasury/tests/test_scenario.py::test_trs_041_no_perp`

### TRS-042 阈值只来自 RiskParams,改动随下一轮生效

- 规则:[输入](README.md#输入)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`;journal 在 `T0 + 30 s` 写入新的 `ConfigBlob{RiskParams}`,`liq_dist_block_open = 0.20`(其余同 `risk_t`)
- 步骤:
  1. `round_at(T0)`
  2. `round_at(T0 + 60 s)`
- 期望:
  - 步骤 1 后 `headroom.up.L1` = 0.034825867
  - 步骤 2 后 `headroom.up.L1` = 0.077943611(= 1.293532333/1.20 − 1);误差均 ≤ 0.1 个百分点
- 实现:`python/treasury/tests/test_scenario.py::test_trs_042_thresholds_from_riskparams`

### TRS-043 模型二分的结果与闭式解相差不超过 0.1 个百分点

- 规则:[计算](README.md#计算)
- 级别:prop
- 里程碑:M5
- 性质:单个永续持仓、无交易所 `liq_px`、维持保证金按名义同比例时,模型二分得到的 `s_liq` 与闭式解(空头 `(MB − M)/(N + M)`,多头 `−(MB − M)/(N − M)`)之差 ≤ 0.001;L1–L3 与按闭式 `s_liq` 换算的值之差 ≤ 0.001
- 生成:方向 ∈ {多, 空};`N` ∈ [100, 10 000 000] USDT;`MB/N` ∈ [0.02, 2.0];`M/N` ∈ [0.001, 0.05];`risk_t` 阈值固定;`stress_max_move = 3.0`(超出时期望 `capped`)
- 参考:`tests/model_ref.py` 的闭式实现(`Decimal`,精度 40 位)
- 例数:PR 2 000
- 实现:`python/treasury/tests/test_scenario.py::test_trs_043_bisection_precision`

### TRS-050 空头保证金不足时的需求与缺口

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`(`target_headroom = 0.20`、`reserve_usd.perp = 1 000`)、`risk_t`;`short1()`(`rep_avail = 6 000`)
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `locations.PP2.demand = 5 214`(追加后 `MB` = 23 214 时 `s_liq` = 0.38、`s*_L2` = 0.20)
  - `locations.PP2.reserve = 1 000`;`locations.PP2.surplus = −5 214`(= min(6 000 − 1 000, −5 214))
- 实现:`python/treasury/tests/test_plan.py::test_trs_050_demand`

### TRS-051 需求取满足目标的最小整数 USDT

- 规则:[计算](README.md#计算)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`(`target_headroom = 0.20`)、`risk_t`;`A1/PP2` 空头 1.0(mark 60 000),报告与 ledger 的保证金余额均为每行的值,`rep_maint = 300`,`items` 为空
- 实现:`python/treasury/tests/test_plan.py::test_trs_051_demand_boundary`

| 行 | 保证金余额 | 期望 `demand` |
|---|---|---|
| TRS-051.1 | 23 213 | 1 |
| TRS-051.2 | 23 214 | 0 |
| TRS-051.3 | 23 215 | −1 |

### TRS-052 保证金位置的闲置不超过移出后仍满足目标的额度

- 规则:[计算](README.md#计算)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`(`reserve_usd.perp = 1 000`、`target_headroom = 0.20`)、`risk_t`;`A1/PP2` 空头 1.0,报告与 ledger 保证金余额 30 000、`rep_maint = 300`、`items` 为空,`rep_avail` 见各行
- 实现:`python/treasury/tests/test_plan.py::test_trs_052_margin_surplus_cap`

| 行 | `rep_avail` | 期望 `demand` / `surplus` |
|---|---|---|
| TRS-052.1 | 15 000 | −6 786 / 6 786(受余量目标限制) |
| TRS-052.2 | 3 000 | −6 786 / 2 000(受可用保证金限制) |

### TRS-053 现金位置低于保留额即缺口

- 规则:[计算](README.md#计算)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`(`reserve_usd.spot = 500`)、`risk_t`;`spot1(每行)`
- 实现:`python/treasury/tests/test_plan.py::test_trs_053_cash_reserve_boundary`

| 行 | `SP1` USDT free | 期望 `SP1.demand` / `surplus` |
|---|---|---|
| TRS-053.1 | 400 | 0 / −100 |
| TRS-053.2 | 499.999999 | 0 / −0.000001 |
| TRS-053.3 | 500 | 0 / 0 |
| TRS-053.4 | 500.000001 | 0 / 0.000001 |

### TRS-054 需求按两个方向中需要更多的一方

- 规则:[计算](README.md#计算)
- 级别:prop
- 里程碑:M5
- 性质:任一保证金位置,`demand` = max(只按上涨方向求得的最小追加额, 只按下跌方向求得的最小追加额)(各自为满足 `|s*_L2| ≥ target_headroom` 的最小整数 USDT,已满足时为负的最大可移出额);只有一个方向有持仓时等于该方向的值
- 生成:同一位置 1–3 个永续持仓,方向随机,名义 ∈ [1 000, 1 000 000] USDT;保证金余额 ∈ [0.1, 1.5] × Σ 名义;`rep_maint` = 0.005 × Σ 名义;`items` 为空;`target_headroom` ∈ [0.05, 0.50]
- 参考:`tests/model_ref.py` 按方向分别二分的实现
- 例数:PR 1 000
- 实现:`python/treasury/tests/test_plan.py::test_trs_054_demand_takes_max`

### TRS-060 一个缺口一个来源:跨所进合约钱包的建议

- 规则:[计算](README.md#计算)、[划转通道表](README.md#划转通道表)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`(`channels.cross = {1, 0, 600}`、`channels.internal = {0, 0, 5}`)、`risk_t`;`short1()`;`spot1(20 000)`
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `A1.locations.SP1.surplus = 19 500`;`A1.locations.PP2.surplus = −5 214`
  - `A1.suggestions` 恰一条:`{from: SP1, to: PP2, asset: USDT, amount: 5 214, cost: 1, latency: 605, urgency: normal, notify: true}`(时延 = 跨所 600 + `PP2` 端内部划转 5)
- 实现:`python/treasury/tests/test_plan.py::test_trs_060_single_pair`

### TRS-061 多个来源时先用单位成本最低的

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;账户 `A3`:`SP1` USDT free 3 500(闲置 3 000)、`SP4` USDT free 10 000(闲置 9 500)、`PP2` 同 `short1()`(缺口 5 214)
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `suggestions` 恰一条:`{from: SP4, to: PP2, amount: 5 214, cost: 1}`(单位成本 1/5 214 < 1/3 000)
  - 没有 `from = SP1` 的建议
- 实现:`python/treasury/tests/test_plan.py::test_trs_061_lowest_unit_cost`

### TRS-062 单位成本相同时先补两个方向中余量更小的缺口

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:直接调 `pair(locations, channels)`:闲置 `SP1` 10 000;缺口 `PP2` 2 000(该位置较小的 `|s*_L2|` = 0.15)、缺口 `PP3` 2 000(0.12);两缺口的通道都是跨所 + 合约端内部(成本 1、时延 605)
- 步骤:
  1. 调用 `pair`
- 期望:
  - 返回两条,依次 `{SP1 → PP3, 2 000}`、`{SP1 → PP2, 2 000}`
- 实现:`python/treasury/tests/test_pair.py::test_trs_062_tie_by_headroom`

### TRS-063 再相同时按位置名字典序

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:同 TRS-062,但两个缺口的较小 `|s*_L2|` 都是 0.12
- 步骤:
  1. 调用 `pair`
- 期望:
  - 依次 `{SP1 → PP2, 2 000}`、`{SP1 → PP3, 2 000}`
  - 把输入中两个缺口的顺序对调后再调一次,输出逐字节相同
- 实现:`python/treasury/tests/test_pair.py::test_trs_063_tie_by_name`

### TRS-064 同所内部划转先于跨所提现

- 规则:[计算](README.md#计算)、[划转通道表](README.md#划转通道表)
- 级别:unit
- 里程碑:M5
- 前置:直接调 `pair`:缺口 `PP2`(交易所 E2)2 000;闲置 `W2`(合成的 E2 现货类位置,与 `PP2` 之间按 `internal` 通道)3 000、闲置 `SP1`(E1)10 000;`channels` 同 `pol_t`
- 步骤:
  1. 调用 `pair`
- 期望:
  - 返回恰一条 `{W2 → PP2, 2 000, cost: 0, latency: 5}`;没有 `from = SP1` 的项
- 实现:`python/treasury/tests/test_pair.py::test_trs_064_internal_first`

### TRS-065 闲置不足时只部分补足

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`(缺口 5 214);`spot1(3 500)`(闲置 3 000)
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `suggestions` 恰一条 `{from: SP1, to: PP2, amount: 3 000, cost: 1}`;`locations.PP2.surplus` 仍为 −5 214(配对前的值)
- 实现:`python/treasury/tests/test_plan.py::test_trs_065_partial`

### TRS-066 两端都是合约钱包时各加一段内部划转

- 规则:[计算](README.md#计算)、[划转通道表](README.md#划转通道表)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;账户 `A2`:`PP2` 同 `short1()`(缺口 5 214);`PP3` 空头 `BTC/PP3` 1.0,报告与 ledger 保证金余额 30 000、`rep_maint = 300`、`rep_avail = 15 000`、`items` 为空(闲置 6 786)
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `suggestions` 恰一条 `{from: PP3, to: PP2, amount: 5 214, cost: 1, latency: 610}`(5 + 600 + 5)
- 实现:`python/treasury/tests/test_plan.py::test_trs_066_contract_to_contract`

### TRS-067 现金位置的缺口从合约钱包补,只报告不推送

- 规则:[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`(`max_cost_ratio_bp = 100`)、`risk_t`;`A1/PP2` 空头 1.0,保证金余额 30 000、`rep_maint = 300`、`rep_avail = 15 000`、`items` 为空(闲置 6 786);`spot1(400)`(缺口 100)
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `suggestions` 恰一条 `{from: PP2, to: SP1, amount: 100, cost: 1, latency: 605, urgency: normal, notify: false}`(成本比 1 % 等于上限,照常建议)
- 实现:`python/treasury/tests/test_plan.py::test_trs_067_cash_gap_report_only`

### TRS-068 配对结果与输入顺序无关

- 规则:[计算](README.md#计算)
- 级别:prop
- 里程碑:M5
- 性质:同一组位置(≤ 12 个)以任意顺序输入 `pair`,返回的建议列表逐字节相同;每条建议的 `amount` ≤ 其来源的闲置额与目标的缺口额;建议额之和不超过闲置总额与缺口总额中较小者;任一来源或目标不是"待转入"位置
- 生成:2–12 个位置,类别 ∈ {现货, 合约},交易所 E1–E6(同一交易所至多一个市场),`S` ∈ [−50 000, 50 000] USDT(raw 整数),较小的 `|s*_L2|` ∈ [0, 0.5];`channels` 同 `pol_t`
- 参考:`tests/pair_ref.py` 的逐步贪心参考实现
- 例数:PR 1 000
- 实现:`python/treasury/tests/test_pair.py::test_trs_068_order_independent`

### TRS-070 金额低于 `min_transfer_usd` 的建议被滤掉

- 规则:[计算](README.md#计算)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`(`min_transfer_usd = 10`);直接调 `plan` 的过滤步骤,输入一条 `internal` 通道(成本 0)的配对结果,金额见各行
- 实现:`python/treasury/tests/test_pair.py::test_trs_070_min_transfer`

| 行 | 配对金额 | 期望 |
|---|---|---|
| TRS-070.1 | 9.999999 | `suggestions` 为空 |
| TRS-070.2 | 10 | 保留,`amount = 10` |
| TRS-070.3 | 10.000001 | 保留 |

### TRS-071 成本比超过上限的建议被滤掉

- 规则:[计算](README.md#计算)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`(`max_cost_ratio_bp = 100`,`channels.cross.fixed_usd = 1`、`ratio_bp = 0`);输入一条跨所配对结果,金额见各行
- 实现:`python/treasury/tests/test_pair.py::test_trs_071_cost_ratio`

| 行 | 配对金额(成本比) | 期望 |
|---|---|---|
| TRS-071.1 | 99.999999(1.00000001 %) | `suggestions` 为空 |
| TRS-071.2 | 100(1 %) | 保留 |
| TRS-071.3 | 100.000001 | 保留 |

### TRS-080 任一方向 L2 余量低于 `min_headroom` 即紧急

- 规则:[计算](README.md#计算)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`(`min_headroom = 0.10`)、`risk_t`;`short1(mb = 18 000, maint = 1, avail = 6 000, liq_px = 每行, ratio = 1 / 18 000)`(交易所值起作用);`spot1(20 000)`
- 实现:`python/treasury/tests/test_plan.py::test_trs_080_urgency_boundary`

| 行 | `liq_px`(`headroom.up.L2`) | 期望 `suggestions[0].urgency` / `notify` |
|---|---|---|
| TRS-080.1 | 75 893.1(0.0999) | `urgent` / `true` |
| TRS-080.2 | 75 900(0.1000) | `normal` / `true` |
| TRS-080.3 | 75 906.9(0.1001) | `normal` / `true` |

### TRS-081 引擎已在 L1 及以上时建议附提示

- 规则:[计算](README.md#计算)、[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`、`spot1(20 000)`;journal 中 `A1` 的 `MarginStress` cap 见各行(`Note{CapSet}` 的 `CapRecord.level`)
- 实现:`python/treasury/tests/test_plan.py::test_trs_081_engine_level_hint`

| 行 | journal 中的 cap | 期望 `headroom.engine_level` / `suggestions[0].reason` |
|---|---|---|
| TRS-081.1 | 无 | 0 / 不含"引擎已禁开" |
| TRS-081.2 | `CapSet{MarginStress, level 1}` | 1 / 含"引擎已禁开/已在减仓,划转只能补救余量" |
| TRS-081.3 | `CapSet{MarginStress, level 3}` | 3 / 含同一提示 |
| TRS-081.4 | `CapSet{…, level 2}` 之后 `CapCleared{MarginStress}` | 0 / 不含提示 |

### TRS-082 同一建议在 `renotify_min` 内不重复推送

- 规则:[输出与登记](README.md#输出与登记)、[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`(`renotify_min = 30`)、`risk_t`;`short1()`、`spot1(20 000)`(每轮都产生 `{SP1 → PP2}` 的普通建议)
- 步骤:
  1. `round_at(T0)`
  2. `round_at(T0 + 60 s)`
  3. `round_at(T0 + 1 799.999 s)`
  4. `round_at(T0 + 1 800 s)`
- 期望:
  - 各步该建议的 `notify` 依次为 `true`、`false`、`false`、`true`
- 实现:`python/treasury/tests/test_notify.py::test_trs_082_renotify_window`

### TRS-083 紧急度升高与紧急建议不受去重限制

- 规则:[输出与登记](README.md#输出与登记)、[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`(`renotify_min = 30`、`min_headroom = 0.10`)、`risk_t`;同 TRS-080 的夹具,每轮改 `liq_px`
- 步骤:
  1. `liq_px = 75 906.9`(普通),`round_at(T0)`
  2. `liq_px = 75 893.1`(紧急),`round_at(T0 + 60 s)`
  3. 仍紧急,`round_at(T0 + 120 s)`
  4. `liq_px = 75 906.9`(普通),`round_at(T0 + 180 s)`
- 期望:
  - `{SP1 → PP2}` 的 `notify` 依次为 `true`、`true`、`true`、`false`
- 实现:`python/treasury/tests/test_notify.py::test_trs_083_urgent_not_deduped`

### TRS-084 去重状态跨进程重启保留

- 规则:[输出与登记](README.md#输出与登记)
- 级别:integ
- 里程碑:M5
- 前置:`pol_t`(`renotify_min = 30`)、`risk_t`;`short1()`、`spot1(20 000)`;状态目录 `tmp_path`
- 步骤:
  1. 启动 `hft-treasury run`,假时钟 `T0`,完成一轮后 SIGTERM
  2. 以同一状态目录重新启动,假时钟 `T0 + 60 s`,完成一轮
- 期望:
  - 步骤 1 后 `notify.json` 含键 `A1|SP1|PP2|USDT`,时刻 `T0`;该轮建议 `notify = true`
  - 步骤 2 的 `latest.json` 中该建议 `notify = false`
- 实现:`python/treasury/tests/test_notify.py::test_trs_084_notify_state_survives_restart`

### TRS-085 去重状态缺失或损坏时按空表处理并记录

- 规则:[输出与登记](README.md#输出与登记)
- 级别:table
- 里程碑:M5
- 前置:同 TRS-084 步骤 1 之后;重启前按各行处理 `notify.json`;重启后在 `T0 + 60 s` 跑一轮
- 实现:`python/treasury/tests/test_notify.py::test_trs_085_notify_state_reset`

| 行 | `notify.json` | 期望 |
|---|---|---|
| TRS-085.1 | 删除 | 该建议 `notify = true`;`A1.notes` 含 `notify_state_reset` |
| TRS-085.2 | 写入 `{"A1|SP1` 截断的半个 JSON | 同上;进程未退出 |
| TRS-085.3 | 保持原样(对照) | `notify = false`;`notes` 不含 `notify_state_reset` |

### TRS-090 相同输入两次输出逐字节相同

- 规则:[计算](README.md#计算)、[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M5
- 性质:同一 `(inputs, policy, round_ts, notify 状态)` 调 `plan` 两次,序列化后的 `latest.json` 逐字节相同;在两次调用之间打乱 `inputs` 中账户、位置与持仓的列举顺序,结果仍逐字节相同
- 生成:1–10 个账户,每账户 1–4 个 venue(同一交易所至多一个市场),每个永续 venue 0–3 个持仓,数值取自 TRS-043 的取值域;`RecordTransfer` 0–2 笔
- 参考:两次输出的字节比较
- 例数:PR 500
- 实现:`python/treasury/tests/test_output.py::test_trs_090_deterministic`

### TRS-091 输出结构、键与数值单位

- 规则:[输出与登记](README.md#输出与登记)、[计算](README.md#计算)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`、`spot1(20 000)`;`run_round(T0)`
- 步骤:
  1. 读 `latest.json`
- 期望:
  - 顶层键恰为 `round_ts`、`accounts`;`A1` 的键依次为 `account`、`kind`、`status`、`locations`、`headroom`、`suggestions`、`notes`
  - `locations.PP2` 的键恰为 `balance`、`reserve`、`demand`、`surplus`、`in_transit`、`margin_ratio`、`data_age_ms`;`headroom` 的键恰为 `up`、`down`、`engine_level`、`day_min_l2`;建议的键恰为 `from`、`to`、`asset`、`amount`、`cost`、`latency`、`urgency`、`reason`、`notify`
  - 金额为 USDT raw 整数(`suggestions[0].amount = 5214000000`),比例为 1e-9 raw 整数(`headroom.up.L2` 在 [123810725, 125810725] 内),JSON 中没有浮点数
  - `kind = "Live"`、`status = "ok"`
- 实现:`python/treasury/tests/test_output.py::test_trs_091_schema`

### TRS-092 历史文件每个 UTC 小时写一份,保留 30 天

- 规则:[输出与登记](README.md#输出与登记)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`、`spot1(20 000)`;状态目录 `tmp_path/history/` 预置一个 `round_ts = T0 − 30 d` 的文件 `h_old30` 与一个 `T0 − 30 d − 1 s` 的文件 `h_old31`;按各行依次 `run_round`
- 实现:`python/treasury/tests/test_output.py::test_trs_092_history_hourly`

| 行 | 这一轮的 `round_ts` | 期望 `history/` |
|---|---|---|
| TRS-092.1 | `T0 + 30 s` | 新增 `T0 + 30 s` 的文件(该 UTC 小时最早的一轮);`h_old31` 被删除,`h_old30` 保留 |
| TRS-092.2 | `T0 + 90 s` | 无新增 |
| TRS-092.3 | `T0 + 3 599.999 s` | 无新增 |
| TRS-092.4 | `T0 + 3 600 s` | 新增 `T0 + 3 600 s` 的文件 |

### TRS-093 状态文件原子写,写入中途崩溃不留半截文件

- 规则:[输出与登记](README.md#输出与登记)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`、`spot1(20 000)`;`run_round(T0)` 已写出 `latest.json`(内容 `L0`);第二次 `run_round` 前把 `os.replace` 替换为抛出 `OSError` 的桩
- 步骤:
  1. `run_round(T0 + 60 s)`
  2. 恢复 `os.replace`,`run_round(T0 + 120 s)`
- 期望:
  - 步骤 1 后 `latest.json` 的内容等于 `L0`,`latest.json.sha256` 与 `L0` 一致;状态目录可有 `latest.json.tmp`
  - 步骤 2 后 `latest.json` 的 `round_ts = T0 + 120 s`;目录中没有 `*.tmp`
- 实现:`python/treasury/tests/test_output.py::test_trs_093_atomic_write`

### TRS-094 每个输出文件旁有内容的 SHA-256

- 规则:[输出与登记](README.md#输出与登记)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`run_round(T0 + 30 s)`(写出 `latest.json` 与 `history/` 文件)
- 步骤:
  1. 对每个输出文件计算 SHA-256
- 期望:
  - `latest.json.sha256`、`history/<ts>.json.sha256`、`notify.json.sha256` 都存在,内容等于对应文件 SHA-256 的小写十六进制(64 个字符)
- 实现:`python/treasury/tests/test_output.py::test_trs_094_sha256_sidecar`

### TRS-095 当日最低 L2 余量按 UTC 日累计

- 规则:[输出与登记](README.md#输出与登记)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()` 的 `liq_px` 每轮改,使 `headroom.up.L2` 依次为 0.15、0.12、0.13、0.14;`A1` 无多头
- 步骤:
  1. `round_at(T0 + 23 h 57 min)`(0.15)
  2. `round_at(T0 + 23 h 58 min)`(0.12)
  3. `round_at(T0 + 23 h 59 min 59.999 s)`(0.13)
  4. `round_at(T0 + 24 h)`(0.14)
- 期望:
  - 步骤 3 后 `day_min_l2 = {utc_date: 2026-10-01, up: 0.12, down: null}`(误差 ≤ 0.1 个百分点)
  - 步骤 4 后 `day_min_l2 = {utc_date: 2026-10-02, up: 0.14, down: null}`
- 实现:`python/treasury/tests/test_output.py::test_trs_095_day_min`

### TRS-096 每轮不写 journal、不连 ctl socket、只写自己的状态目录

- 规则:[输出与登记](README.md#输出与登记)、[概要](README.md#概要)、[依赖](README.md#依赖)
- 级别:integ
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;journal 夹具目录只读挂载(`chmod -R a-w`);`socket.socket.connect` 打桩记录全部目标;文件写入经审计钩子(`sys.addaudithook` 记录 `open` 的写模式路径)
- 步骤:
  1. `run_round(T0)`
- 期望:
  - 这一轮成功(`latest.json` 写出);journal 夹具目录中的文件与修改时刻不变
  - `connect` 记录为空(没有连接 ctl socket、kill-switch 或任何网络地址)
  - 写模式打开的路径都在状态目录之下
- 实现:`python/treasury/tests/test_output.py::test_trs_096_no_side_channels`

### TRS-097 once、explain、stress 只打印、不写状态目录

- 规则:[输出与登记](README.md#输出与登记)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`、`spot1(20 000)`;状态目录已有 `latest.json`(内容 `L0`)与 `notify.json`(内容 `N0`)
- 实现:`python/treasury/tests/test_cli.py::test_trs_097_readonly_subcommands`

| 行 | 命令 | 期望 |
|---|---|---|
| TRS-097.1 | `hft-treasury once --config …` | 退出 0;stdout 是与 `latest.json` 同结构的 JSON;`latest.json` = `L0`、`notify.json` = `N0`、`history/` 无新增 |
| TRS-097.2 | `hft-treasury explain --config … --account A1` | 退出 0;状态目录不变 |
| TRS-097.3 | `hft-treasury stress --config … --account A1 --move 0.5` | 退出 0;状态目录不变 |

### TRS-098 explain 给出每个位置的推导与配对步骤

- 规则:[输出与登记](README.md#输出与登记)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`short1()`、`spot1(20 000)`
- 步骤:
  1. `hft-treasury explain --config … --account A1`
- 期望:
  - `PP2` 项:`B = 6 000`、`R = 1 000`、`D = 5 214`、`S = −5 214`、`in_transit = 0`、上涨方向 `s_liq` 来源 `exchange`、下跌方向为"无强平风险"
  - `SP1` 项:`B = 20 000`、`R = 500`、`D = 0`、`S = 19 500`
  - 配对步骤恰一步:`SP1 → PP2`,`x = 5 214`,单位成本 = 1/5 214
- 实现:`python/treasury/tests/test_cli.py::test_trs_098_explain`

### TRS-099 stress 给出样例账户在普涨下的永续所权益占比

- 规则:[输出与登记](README.md#输出与登记)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;账户 `A4`(评审 §2.2 的样例,总资金 100 000):`PP2` 空头 `BTC/PP2` 1.0(mark 60 000)、保证金钱包 40 000、`rep_maint = 300`;`SP1` 持 `BTC` 1.0(bid 60 000)、USDT 0
- 实现:`python/treasury/tests/test_cli.py::test_trs_099_stress_sample`

| 行 | `--move` | 期望 `PP2` 权益 / 维持保证金 / 保证金率 / 权益占比(占比误差 ≤ 1 个百分点) |
|---|---|---|
| TRS-099.1 | 0.5 | 10 000 / 450 / 0.045 / 0.100 |
| TRS-099.2 | 0.6 | 4 000 / 480 / 0.12 / 0.040 |
| TRS-099.3 | −0.5 | 70 000 / 150 / 0.002142857 / 0.700 |
| TRS-099.4 | 0 | 40 000 / 300 / 0.0075 / 0.400 |

### TRS-100 stress 的变动必须在 (−1, `stress_max_move`] 内

- 规则:[输出与登记](README.md#输出与登记)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`(`stress_max_move = 3.0`)、`risk_t`;账户 `A4`
- 实现:`python/treasury/tests/test_cli.py::test_trs_100_stress_bounds`

| 行 | `--move` | 期望 |
|---|---|---|
| TRS-100.1 | `-1` | 退出码 2 |
| TRS-100.2 | `-0.999999999` | 退出 0 |
| TRS-100.3 | `3.0` | 退出 0 |
| TRS-100.4 | `3.000000001` | 退出码 2 |
| TRS-100.5 | `abc` | 退出码 2 |

### TRS-101 子命令的账户不存在时退出 2

- 规则:[输出与登记](README.md#输出与登记)
- 级别:table
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;两个实例的 refdata 中都没有账户 `ZZ`
- 实现:`python/treasury/tests/test_cli.py::test_trs_101_unknown_account`

| 行 | 命令 | 期望 |
|---|---|---|
| TRS-101.1 | `once --account ZZ` | 退出码 2;stderr 含 `ZZ` |
| TRS-101.2 | `explain --account ZZ` | 退出码 2 |
| TRS-101.3 | `stress --account ZZ --move 0.5` | 退出码 2 |

### TRS-102 once 带账户时只输出该账户

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`prod` 有 `A1`、`A3`,`sim` 有 `S9`
- 步骤:
  1. `hft-treasury once --config … --account A3`
  2. `hft-treasury once --config …`
- 期望:
  - 步骤 1 的 `accounts` 恰一项 `A3`
  - 步骤 2 的 `accounts` 依次为 `A1`、`A3`、`S9`
- 实现:`python/treasury/tests/test_cli.py::test_trs_102_once_filter`

### TRS-105 模拟账户同样计算、标 Sim、不与实盘混合

- 规则:[模拟账户](README.md#模拟账户)
- 级别:unit
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;`S9`(`sim` 实例)的数据同 `short1()` + `spot1(20 000)`;`A1`(`prod`)只有 `spot1(30 000)`
- 步骤:
  1. `round_at(T0)`
- 期望:
  - `S9.kind = "Sim"`,`S9.suggestions` 恰一条 `{SP1 → PP2, 5 214}`(与 TRS-060 相同)
  - `A1.kind = "Live"`;没有任何建议的 `from`/`to` 跨账户;输出中没有把 `A1` 与 `S9` 金额相加的字段
- 实现:`python/treasury/tests/test_plan.py::test_trs_105_sim_account`

### TRS-110 源码只导入允许的模块

- 规则:[依赖](README.md#依赖)、[概要](README.md#概要)、[输出与登记](README.md#输出与登记)
- 级别:table
- 里程碑:M5
- 前置:对 `src/hft_treasury/` 做 AST 扫描,允许的顶层导入 = 标准库中的 `argparse`、`dataclasses`、`datetime`、`decimal`、`hashlib`、`json`、`os`、`pathlib`、`signal`、`sys`、`time`、`tomllib`、`typing` 与 `polars`、`pyarrow`、`hft_py`、`hft_treasury`;每行在源码副本中加入一行导入后再扫描
- 实现:`python/treasury/tests/test_hygiene.py::test_trs_110_import_allowlist`

| 行 | 加入的导入 | 期望 |
|---|---|---|
| TRS-110.1 | 无(对照) | 扫描通过 |
| TRS-110.2 | `import httpx` | 扫描失败,报告 `httpx` |
| TRS-110.3 | `import hftctl` | 扫描失败 |
| TRS-110.4 | `import subprocess` | 扫描失败 |
| TRS-110.5 | `import socket` | 扫描失败 |
| TRS-110.6 | `import ccxt` | 扫描失败 |

### TRS-111 本包源码与 README 不出现策略族名

- 规则:[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M5
- 前置:`cargo xtask doc-check` 的名字越界检查对 `python/treasury/`(`.py` 与 Markdown)运行;临时副本中在 `plan.py` 注释里写入 `strategies/` 下任一策略目录名(由测试在运行时读取目录名得到,本文件不写出)
- 步骤:
  1. 对原目录运行
  2. 对临时副本运行
- 期望:
  - 步骤 1 退出 0
  - 步骤 2 退出非 0,报告 `plan.py` 的该行
- 实现:`python/treasury/tests/test_hygiene.py::test_trs_111_no_strategy_names`

### TRS-112 包的运行依赖只有 polars、pyarrow 与 hft-py

- 规则:[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M5
- 前置:读 `python/treasury/pyproject.toml`
- 步骤:
  1. 取 `[project].dependencies` 的包名集合
- 期望:
  - 集合恰为 `{polars, pyarrow, hft-py}`
- 实现:`python/treasury/tests/test_hygiene.py::test_trs_112_runtime_deps`

### TRS-120 10 个账户一轮在 5 s 内完成

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:integ
- 里程碑:M5
- 前置:`pol_t`、`risk_t`;journal 夹具含 10 个账户,每账户 1 个现货 venue 与 1 个永续 venue、永续 venue 2 个持仓;夹具 journal 共 1 小时、约 200 000 条记录
- 步骤:
  1. `run_round(T0)`,以 `time.monotonic_ns` 计时(含读 journal 与写状态文件)
- 期望:
  - 耗时 < 5 000 ms;`latest.json.accounts` 恰 10 项
- 实现:`python/treasury/tests/test_perf.py::test_trs_120_round_under_5s`

### TRS-130 整轮金样:journal 夹具到 latest.json 逐字节相同

- 规则:[输入](README.md#输入)、[计算](README.md#计算)、[输出与登记](README.md#输出与登记)
- 级别:golden
- 里程碑:M5
- 输入:`python/treasury/tests/fixtures/golden/TRS-130/journal/`(`prod` 与 `sim` seq journal,由 hft-journal 的测试写者 `examples/testwriter.rs` 生成,与 [hft-py TESTS](../../crates/hft-py/TESTS.md) 的 `TW` 夹具同一工具:`A1`、`A2`、`S9` 的余额、`MarginReport`、mark、`RecordTransfer` 一笔、`MarginStress` cap 一条、`ReconcileReport` 一轮)与同目录的 `treasury.toml`、`policy.toml`(取值同 `pol_t`),`round_ts = T0`
- 期望文件:`python/treasury/tests/fixtures/golden/TRS-130/expected/latest.json`(由人按 README 手算核对,不由当前代码生成)
- 实现:`python/treasury/tests/test_golden.py::test_trs_130_round_golden`

### TRS-131 经 hft-py 读真实 journal 得到在途与数据年龄

- 规则:[输入](README.md#输入)
- 级别:integ
- 里程碑:M5
- 前置:`prod` seq journal(同 TRS-130 的生成方式):`T0 − 120 s` 的 `RecordTransfer{A1, SP1 → PP2, USDT, 5 214}`、`T0 − 5 s` 到达的 `MarginReport{PP2, rep_margin_balance = 18 000}`;其余同 TRS-023;treasury 只经 `hft_py.open_journal` 与 `hft_py.ledger` 读取
- 步骤:
  1. `run_round(T0)`
- 期望:
  - `locations.PP2.in_transit = 5 214`;`locations.PP2.data_age_ms = 5 000`;与 TRS-023 的 `plan` 结果逐字节相同
- 实现:`python/treasury/tests/test_inputs.py::test_trs_131_via_hft_py`

### TRS-140 模拟账户长跑期间每分钟产出、无漏轮

- 规则:[实现说明](README.md#实现说明)、[配置文件字段](README.md#配置文件字段)
- 级别:host
- 里程碑:M6
- 环境:dev2,`sim` 实例与 `hft-treasury.service` 连续运行(门槛 ⑦ 的模拟账户长跑窗口)
- 授权:无
- 步骤:
  1. 窗口内每天从 journald 导出 `hft-treasury` 的每轮日志,并核对 `history/` 文件
  2. 统计每个 UTC 分钟是否恰有一轮(`round_ts` 落在该分钟)
- 通过标准:窗口内(扣除记录在案的护栏停机)漏轮数 = 0;每个 UTC 小时恰有一个 `history/<ts>.json`
- 证据:`ops-log/treasury/TRS-140-<窗口起止>.md`(含统计脚本输出)
- 实现:`python/treasury/tests/host/TRS-140.sh`

### TRS-141 老板确认 /treasury 面板可读

- 规则:[输出与登记](README.md#输出与登记)
- 级别:drill
- 里程碑:M5
- 环境:dev2 的 ops_bot,老板以 Viewer 身份私聊
- 授权:无
- 步骤:按 [launch](../../docs/runbooks/launch.md) 的 ops_bot 工作流核对,老板发 `/treasury` 与 `/treasury <模拟账户>`,逐项读出状态、两个方向的余量、在途与建议
- 通过标准:老板能复述每个账户的状态与最近的建议;记录中无"看不懂"项未解决
- 证据:`ops-log/drills/TRS-141-<日期>.md`(截图与老板确认)
- 实现:`python/treasury/tests/host/TRS-141.md`

### TRS-142 正式实盘后人工划转都已登记、对账无未解释差异

- 规则:[输出与登记](README.md#输出与登记)
- 级别:host
- 里程碑:M8
- 环境:升级主机 `prod` 实例,正式实盘开始后 30 天
- 授权:无(只读 journal 与 reconciler 报告)
- 步骤:
  1. 列出交易所侧该期间全部人工划转(由交易员导出)
  2. 与 journal 中的 `RecordTransfer` 按 `ext_ref` 逐条对照
  3. 检查该期间 `ReconcileReport` 中由划转引起的余额差异条目
- 通过标准:每笔人工划转都有 `RecordTransfer`;无因划转产生的未解释余额差异
- 证据:`ops-log/treasury/TRS-142-<月份>.md`
- 实现:`python/treasury/tests/host/TRS-142.sh`

## 验收

- 里程碑 M5 的全部用例有实现且通过;属性测试的大例数运行在每夜:TRS-043、TRS-054、TRS-068、TRS-090 各 100 000 例。
- [上线门槛](../../docs/testing.md#8-上线门槛) ⑦ 期间(模拟账户长跑):每分钟产出、无漏轮,证据为 TRS-140。
- 老板确认 `/treasury` 的可读性(门槛 ⑪ 的 ops_bot 工作流之一):TRS-141。
- 正式实盘开始后所有人工划转都有 `RecordTransfer` 登记,reconciler 无因划转产生的未解释差异:TRS-142。
