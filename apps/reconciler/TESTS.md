---
component: reconciler
prefix: RCN
spec: README.md
---

# reconciler 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。分级、闸门与 cap 的规则归 [hft-risk](../../crates/hft-risk/README.md);外来成交的合成函数、`auto_held` 抵扣与资金费自动校正归 [hft-ledger](../../crates/hft-ledger/README.md);`ReconcileReport` 的布局与码表归 [hft-event](../../crates/hft-event/README.md)。本文件验证 reconciler 自己的配置、轮次触发、引擎视图、在途判定、差异计算、条目与标志的填法、Unknown 证据、启动核验与运行约束;跨组件的完整链路(如 testing AX-1、FD-2 的引擎一侧)在 [scenarios](../../docs/tests/README.md#2-文件与范围)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | RCN-005、RCN-022、RCN-193 |
| 目录 | 不适用:非规范 |
| 使用本组件 | RCN-002、RCN-007–RCN-010、RCN-166 |
| 覆盖范围与数据来源 | RCN-020–RCN-023、RCN-190 |
| 对账周期与比对基准 | RCN-030–RCN-035、RCN-040–RCN-047、RCN-050–RCN-053、RCN-114 |
| 比对项 | RCN-060–RCN-069、RCN-098、RCN-193 |
| 资金费比对 | RCN-072、RCN-080–RCN-098、RCN-133 |
| 现货库存与余额基线 | RCN-062、RCN-100–RCN-104 |
| ReconcileReport | RCN-034、RCN-036、RCN-042、RCN-054、RCN-069–RCN-076、RCN-110–RCN-116、RCN-144–RCN-146 |
| 启动闸门与严格放行 | RCN-031、RCN-033、RCN-130–RCN-135、RCN-194 |
| Unknown 订单收敛 | RCN-035、RCN-092、RCN-140–RCN-149、RCN-195 |
| 启动核验(只读 key) | RCN-160–RCN-167、RCN-191、RCN-192 |
| 与 gw 的分工 | RCN-067、RCN-170、RCN-171 |
| 链上钱包余额扫描(可选扩展) | 不适用:里程碑外 |
| 配置文件字段 | RCN-001–RCN-006、RCN-023、RCN-104、RCN-167、RCN-180、RCN-195 |
| 实现说明 | RCN-044–RCN-046、RCN-097、RCN-098、RCN-113、RCN-116、RCN-180–RCN-183 |
| 延迟与资源契约 | RCN-130、RCN-196、RCN-197 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | RCN-006、RCN-160、RCN-184 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

约定(用例 `前置` 引用下列名字时即取此处的内容;各用例另行写出它依赖的参数):

- **装置**(本 app `tests/support/`):`FakeRest`——按 venue 模拟只读 REST 端点的 HTTP 服务器,响应体取自各 `venue-*` 的金样格式,可逐请求注入延迟、超时、HTTP 429/5xx 与交易所错误码,记录每个请求的路径、参数、时刻与源地址;`FakeCtl`——ctl socket 替身,按 hft-ctl 协议记录收到的 `ReconcileReport` 组(并可回放给进程内 sequencer);**合成 journal**——测试内以 `JournalWriter::open_offline` 写出的段,或进程内 hft-core(hft-testkit 假交易所)录下的段;假时钟——hft-testkit 的可推进 UTC 与 TSC。
- **账户**(测试 refdata,`refdata_blob` 与 journal 引导块中的 refdata 相同):`acct-p`——Live,venue `binance-usdm`(`funding_settle = PerSymbolPush`;成交记录不带 client id),永续 inst `BTCUSDT`(lot 0.001 BTC);`acct-o`——Live,venue `okx-swap`(测试 refdata 设 `funding_settle = RateOnly`;成交可按 client id 过滤),永续 inst `BTC-USDT-SWAP`(lot 0.01 BTC);`acct-g`——Live,venue `bitget-usdt-futures`(测试 refdata 设 `funding_settle = BalanceOnlyPush`),永续 inst `BTCUSDT`、`ETHUSDT`;`acct-s`——Live,venue `binance-spot`,资产 BTC、USDT、BNB;`sim-a`——Sim 账户。各 Live 账户的 `venue_uid` 末 4 位依次为 `1001`、`1002`、`1003`、`1004`。
- **容差**(测试 refdata 的 lot 与 `AssetMeta.dust`):USDT `dust` = 0.01、BTC `dust` = 0.00001、BNB `dust` = 0.001;`funding_dust` = 结算资产 USDT 的 `dust` = 0.01;永续 mark = 60 000 USDT。
- **基础配置**(除非用例另写):`schema = 1`;`interval_s = 30`;`startup_verify_s = 60`;`user_stream_lag_ms = {各 venue = 2000}`;`http.timeout_ms = 5000`;`http.max_rps = {各 venue = 5}`;`unknown_min_age_ms = 2000`;`unknown_alert_rounds = 3`;`history_lookback_s = 60`;`notfound_grace_ms = 10000`;`notfound_confirm_rounds = 2`;`pending_in_max_s = 86400`;引擎配置 `funding_recon_window_ms = 172 800 000`、`funding_recon_lag_ms = 600 000`;秘密名以 `-readonly` 结尾;权限查询返回只读。
- "一轮"指一个 (account, venue) 的一次完整对账(带 `Summary` 的组);"报告组"指 `FakeCtl` 收到的同一 `report_id` 的全部条目。

### RCN-001 配置键的缺省值

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:只含必填键的 `reconciler.toml`(`instance.*`、`accounts = [acct-p]`、`refdata_blob`、`rest_base`、`http.max_rps`、`egress_bind`);以 `Config::load` 解析
- 实现:`apps/reconciler/tests/config.rs::rcn_001_defaults`

| 行 | 键 | 期望 |
|---|---|---|
| RCN-001.1 | `startup_verify_s` | 60 |
| RCN-001.2 | `interval_s` | 30 |
| RCN-001.3 | `user_stream_lag_ms` 中未列出的 venue | 2 000 |
| RCN-001.4 | `http.timeout_ms` | 5 000 |
| RCN-001.5 | `unknown_min_age_ms` | 2 000 |
| RCN-001.6 | `unknown_alert_rounds` | 3 |
| RCN-001.7 | `history_lookback_s` | 60 |
| RCN-001.8 | `notfound_grace_ms` | 10 000 |
| RCN-001.9 | `notfound_confirm_rounds` | 2 |
| RCN-001.10 | `paths.state` | `/var/lib/hft/reconciler` |
| RCN-001.11 | `pending_in_max_s` | 86 400 |

### RCN-002 缺必填键时 --check 退出 2 并一次列出全部缺项

- 规则:[配置文件字段](README.md#配置文件字段)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:基础配置;每行删去该行列出的键
- 实现:`apps/reconciler/tests/config.rs::rcn_002_required_keys`

| 行 | 删去的键 | 期望 |
|---|---|---|
| RCN-002.1 | `instance.journal_cold` | 退出 2;stderr 指明该键 |
| RCN-002.2 | `instance.journal_ram` | 退出 2 |
| RCN-002.3 | `instance.ctl_socket` | 退出 2 |
| RCN-002.4 | `accounts` | 退出 2 |
| RCN-002.5 | `refdata_blob` | 退出 2 |
| RCN-002.6 | `rest_base` | 退出 2 |
| RCN-002.7 | `http.max_rps` | 退出 2 |
| RCN-002.8 | `egress_bind` | 退出 2 |
| RCN-002.9 | `instance.ctl_socket` 与 `egress_bind` 同时删去 | 退出 2;stderr 两条错误都列出(一次收集全部错误) |

### RCN-003 未知键与 schema 不符时退出 2

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:基础配置;每行只做该行改动;运行 `reconciler --check --config <文件>`
- 实现:`apps/reconciler/tests/config.rs::rcn_003_unknown_key_and_schema`

| 行 | 改动 | 期望 |
|---|---|---|
| RCN-003.1 | 增加 `funding_lookback_h = 48` | 退出 2(未知键) |
| RCN-003.2 | 增加 `funding_recon_window_ms = 1`(属于 hft-ledger,本 app 不设该键) | 退出 2 |
| RCN-003.3 | `schema = 2` | 退出 2 |
| RCN-003.4 | 缺 `schema` | 退出 2 |
| RCN-003.5 | 无改动 | 退出 0 |

### RCN-004 数值参数的边界

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:基础配置;每行只改该行的键;运行 `reconciler --check`
- 实现:`apps/reconciler/tests/config.rs::rcn_004_numeric_bounds`

| 行 | 键 = 值 | 期望 |
|---|---|---|
| RCN-004.1 | `interval_s = 0` | 退出 2 |
| RCN-004.2 | `interval_s = 1` | 退出 0 |
| RCN-004.3 | `interval_s = 65535` | 退出 0 |
| RCN-004.4 | `interval_s = 65536` | 退出 2(超出 u16) |
| RCN-004.5 | `startup_verify_s = 0` | 退出 2 |
| RCN-004.6 | `startup_verify_s = 1` | 退出 0 |
| RCN-004.7 | `http.timeout_ms = 0` | 退出 2 |
| RCN-004.8 | `http.timeout_ms = 1` | 退出 0 |
| RCN-004.9 | `http.max_rps = {binance-usdm = 0}` | 退出 2 |
| RCN-004.10 | `http.max_rps = {binance-usdm = 65536}` | 退出 2 |
| RCN-004.11 | `unknown_alert_rounds = 0` | 退出 2 |
| RCN-004.12 | `unknown_alert_rounds = 255` | 退出 0 |
| RCN-004.13 | `unknown_alert_rounds = 256` | 退出 2(超出 u8) |
| RCN-004.14 | `notfound_confirm_rounds = 0` | 退出 2 |
| RCN-004.15 | `notfound_confirm_rounds = 1` | 退出 0 |
| RCN-004.16 | `unknown_min_age_ms = 0` | 退出 0 |
| RCN-004.17 | `notfound_grace_ms = 4294967296` | 退出 2(超出 u32) |
| RCN-004.18 | `user_stream_lag_ms = {binance-usdm = 0}` | 退出 0 |
| RCN-004.19 | `pending_in_max_s = 0` | 退出 2 |
| RCN-004.20 | `pending_in_max_s = 1` | 退出 0 |

### RCN-005 只能列 Live 账户:出现 Sim 账户或没有 Live 账户时退出 2

- 规则:[配置文件字段](README.md#配置文件字段)、[概要](README.md#概要)
- 级别:table
- 里程碑:M5
- 前置:基础配置;每行给出 `accounts`;运行 `reconciler --check`
- 实现:`apps/reconciler/tests/config.rs::rcn_005_live_accounts_only`

| 行 | `accounts` | 期望 |
|---|---|---|
| RCN-005.1 | `[acct-p, sim-a]` | 退出 2;stderr 指明 `sim-a` 是 Sim 账户 |
| RCN-005.2 | `[sim-a]`(只列 `sim` 实例的账户) | 退出 2 |
| RCN-005.3 | `[]` | 退出 2 |
| RCN-005.4 | `[acct-p]` | 退出 0 |

### RCN-006 账户与 venue 必须与 refdata 一致,秘密名必须以 -readonly 结尾

- 规则:[配置文件字段](README.md#配置文件字段)、[依赖](README.md#依赖)
- 级别:table
- 里程碑:M5
- 前置:基础配置;每行只做该行改动;运行 `reconciler --check`
- 实现:`apps/reconciler/tests/config.rs::rcn_006_accounts_match_refdata`

| 行 | 改动 | 期望 |
|---|---|---|
| RCN-006.1 | 增加账户 `acct-x`(refdata 中没有) | 退出 2 |
| RCN-006.2 | `acct-p.venues` 增加 `okx-swap`(refdata 中 `acct-p` 没有该 venue) | 退出 2 |
| RCN-006.3 | `acct-p` 的秘密名为 `hft/prod/acct-p/binance-usdm-trade` | 退出 2 |
| RCN-006.4 | `rest_base` 缺 `binance-usdm` | 退出 2 |
| RCN-006.5 | `http.max_rps` 缺 `binance-usdm` | 退出 2 |
| RCN-006.6 | `refdata_blob` 指向不存在的文件 | 退出 2 |
| RCN-006.7 | `refdata_blob` 是截断的 refdata 文件(hft-refdata 加载失败) | 退出 2 |

### RCN-007 --check 不连接交易所、不读 journal、不写状态目录

- 规则:[使用本组件](README.md#使用本组件)
- 级别:proc
- 里程碑:M5
- 前置:基础配置;`rest_base` 指向不可路由的地址;`instance.journal_cold`、`instance.journal_ram` 指向不存在的目录;`paths.state` 为空目录;`strace -f -e trace=connect,openat`
- 步骤:
  1. `reconciler --check --config <文件>`
- 期望:
  - 退出 0;没有任何 `connect` 到 `AF_INET`/`AF_INET6`;没有打开 journal 路径下的文件;`paths.state` 仍为空
- 实现:`apps/reconciler/tests/cli.rs::rcn_007_check_no_io`

### RCN-008 once --dry-run 打印条目、不连接 ctl、不写状态目录

- 规则:[使用本组件](README.md#使用本组件)
- 级别:integ
- 里程碑:M5
- 前置:基础配置;合成 journal:`acct-p` 引擎仓位 −0.010 BTC;`FakeRest` 返回仓位 −0.008 BTC;`paths.state` 中已有成交游标文件(记下其内容与 mtime);`FakeCtl` 监听 `instance.ctl_socket`
- 步骤:
  1. `reconciler once --config <文件> --account acct-p --dry-run`
- 期望:
  - 退出 0;标准输出是 JSON,含 `Summary` 与一条 `Position{inst = BTCUSDT, engine_qty = −0.010, exch_qty = −0.008, severity = Warn}`
  - `FakeCtl` 的连接数为 0;`paths.state` 下文件的内容与 mtime 不变,没有新文件
- 实现:`apps/reconciler/tests/cli.rs::rcn_008_once_dry_run`

### RCN-009 once 不带 --dry-run 时提交报告组并写轮次明细

- 规则:[使用本组件](README.md#使用本组件)
- 级别:integ
- 里程碑:M5
- 前置:同 RCN-008
- 步骤:
  1. `reconciler once --config <文件> --account acct-p`
- 期望:
  - 退出 0;`FakeCtl` 收到一个报告组(uid `hft-recon`、issuer `reconciler`),条目与 RCN-008 打印的相同;`paths.state/rounds/<report_id>.json` 存在
- 实现:`apps/reconciler/tests/cli.rs::rcn_009_once_submits`

### RCN-010 once 的参数与退出码

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`accounts = [acct-p, acct-o]`;合成 journal;`FakeRest` 按行注入
- 实现:`apps/reconciler/tests/cli.rs::rcn_010_once_exit_codes`

| 行 | 命令与注入 | 期望 |
|---|---|---|
| RCN-010.1 | `once --account acct-p`,全部端点返回 200 | 退出 0;只对 `binance-usdm` 跑了一轮 |
| RCN-010.2 | `once --account acct-p --venue okx-swap` | 退出 2(该 venue 不属于 `acct-p`) |
| RCN-010.3 | `once --account acct-x` | 退出 2(不在 `accounts`) |
| RCN-010.4 | `once --account acct-p`,仓位查询连续返回 HTTP 503 | 退出 1;`FakeCtl` 未收到报告组 |
| RCN-010.5 | `once --account acct-p`,账户信息接口返回的 UID 末 4 位 `9999` | 退出 2;`fatal.json{code = "UidMismatch"}`;没有任何对账请求 |
| RCN-010.6 | `once --account acct-o --venue okx-swap`,有一条 `Warn` 差异 | 退出 0(差异不影响退出码) |

### RCN-020 比对项按 VenueCaps 决定:现货没有仓位项与资金费项

- 规则:[覆盖范围与数据来源](README.md#覆盖范围与数据来源)
- 级别:table
- 里程碑:M5
- 前置:基础配置,四个 Live 账户;每个 venue 跑一轮,`FakeRest` 记录请求路径
- 实现:`apps/reconciler/tests/coverage.rs::rcn_020_items_by_caps`

| 行 | venue | 期望请求的端点种类 |
|---|---|---|
| RCN-020.1 | `binance-usdm` | 挂单、成交、仓位、余额、资金费流水;没有对现货端点的请求 |
| RCN-020.2 | `binance-spot` | 挂单、成交、余额;没有仓位与资金费流水请求;报告组中没有 `Position`、`Funding` 条目 |
| RCN-020.3 | `okx-swap` | 挂单、成交、仓位、余额、资金费类账单 |
| RCN-020.4 | `bitget-usdt-futures` | 挂单、成交、仓位、资产、资金费类账单 |

### RCN-021 每个 (account, venue) 的余额各比对一次,两个账户同所不同市场互不影响

- 规则:[覆盖范围与数据来源](README.md#覆盖范围与数据来源)
- 级别:integ
- 里程碑:M5
- 前置:基础配置;`acct-p`(`binance-usdm`)与 `acct-s`(`binance-spot`)是两个不同账户;`FakeRest` 对 `acct-s` 返回 USDT 余额比引擎多 5 USDT,对 `acct-p` 返回与引擎一致
- 步骤:
  1. 两个 (account, venue) 各跑一轮
- 期望:
  - `acct-s` 的报告组有一条 `Balance{asset = USDT, diff = 5}`;`acct-p` 的报告组没有 `Balance` 条目;每个报告组中同一资产至多一条 `Balance`
- 实现:`apps/reconciler/tests/coverage.rs::rcn_021_one_market_per_exchange`

### RCN-022 Sim 账户在任何触发下都不发请求

- 规则:[覆盖范围与数据来源](README.md#覆盖范围与数据来源)、[概要](README.md#概要)
- 级别:integ
- 里程碑:M5
- 前置:基础配置(`accounts = [acct-p]`);合成 journal 的 refdata 另含 `sim-a`,journal 中有 `Note{EpochStart}`、`sim-a` 的 `ConnState` Down→Up、`sim-a` 的 `Note{OmsNeedsRestQuery}`
- 步骤:
  1. `reconciler run`,推进假时钟 5 个周期
- 期望:
  - `FakeRest` 记录的请求全部属于 `acct-p`;`FakeCtl` 收到的报告组 `account` 全是 `acct-p`
- 实现:`apps/reconciler/tests/coverage.rs::rcn_022_sim_accounts_skipped`

### RCN-023 REST 请求从 egress_bind 发出、发往 rest_base

- 规则:[配置文件字段](README.md#配置文件字段)、[覆盖范围与数据来源](README.md#覆盖范围与数据来源)
- 级别:proc
- 里程碑:M5
- 前置:测试网络命名空间中给网卡两个地址 10.0.0.2、10.0.0.3;`egress_bind = 10.0.0.3`;`rest_base = {binance-usdm = "http://10.0.0.9:8080"}`(`FakeRest`)
- 步骤:
  1. 跑一轮
- 期望:
  - `FakeRest` 记录的每个请求的源地址都是 10.0.0.3,目标为 10.0.0.9:8080
- 实现:`apps/reconciler/tests/coverage.rs::rcn_023_egress_bind`

### RCN-030 每个 (account, venue) 按 interval_s 周期跑一轮

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`accounts = [acct-p, acct-o]`;假时钟从 t = 0 起推进 100 s;journal 中没有触发立即对账的记录
- 实现:`apps/reconciler/tests/schedule.rs::rcn_030_interval`

| 行 | `interval_s` | 期望 |
|---|---|---|
| RCN-030.1 | 30 | 每个 (account, venue) 恰好 4 轮(t = 0、30、60、90 s 开始);两个 (account, venue) 的轮次各自独立 |
| RCN-030.2 | 1 | 每个 (account, venue) 100 轮 |

### RCN-031 读到 EpochStart 后对每个 (Live 账户, venue) 各跑一轮并各自提交

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)、[启动闸门与严格放行](README.md#启动闸门与严格放行)
- 级别:integ
- 里程碑:M5
- 前置:基础配置,`accounts = [acct-p, acct-o, acct-s]`;上一周期刚在 t = 0 跑过;t = 7 s 时 journal 写入 `Note{EpochStart}`
- 步骤:
  1. 推进到 t = 12 s
- 期望:
  - t ∈ [7 s, 12 s) 内 `FakeCtl` 收到 3 个报告组,`report_id` 两两不同,每组一个 (account, venue);每组 `Summary.view_seq` ≥ `EpochStart` 的 seq
- 实现:`apps/reconciler/tests/schedule.rs::rcn_031_epoch_start_rounds`

### RCN-032 下单连接从 Down 回到 Up 时只对该 (venue, account) 立即跑一轮

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`accounts = [acct-p, acct-o]`;上一周期在 t = 0;t = 10 s 起按行写入 `ConnState`
- 实现:`apps/reconciler/tests/schedule.rs::rcn_032_conn_up_trigger`

| 行 | journal 记录 | 期望(t ∈ (10 s, 15 s)) |
|---|---|---|
| RCN-032.1 | `acct-p` 的 `binance-usdm` 下单连接 `ConnState{Down}` 后 `ConnState{Up}` | 只有 `acct-p` 一组报告 |
| RCN-032.2 | `acct-p` 下单连接 `ConnState{Up}`(之前已是 Up) | 没有报告组 |
| RCN-032.3 | `acct-p` 的 BBO 行情连接 Down→Up | 没有报告组 |
| RCN-032.4 | `acct-p` 下单连接 `ConnState{Down}`(未恢复) | 没有报告组 |

### RCN-033 处置命令生效后立即对该 (account, venue) 跑一轮

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)、[启动闸门与严格放行](README.md#启动闸门与严格放行)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`accounts = [acct-p, acct-s]`;上一周期在 t = 0;t = 10 s 时 journal 写入该行的 `CmdResult`
- 实现:`apps/reconciler/tests/schedule.rs::rcn_033_disposition_trigger`

| 行 | `CmdResult` | 期望(t ∈ (10 s, 15 s)) |
|---|---|---|
| RCN-033.1 | `AdoptExchange{acct-p, binance-usdm}` → `Applied` | `acct-p` 一组报告 |
| RCN-033.2 | `SetBalanceBaseline{acct-s, binance-spot}` → `Applied` | `acct-s` 一组报告 |
| RCN-033.3 | `CancelAll{acct-p, binance-usdm}` → `Applied` | `acct-p` 一组报告 |
| RCN-033.4 | `AdoptExchange{acct-p, …}` → `PreconditionFailed{StaleReport}` | 没有报告组 |
| RCN-033.5 | `SetMode{acct-p → Shadow}` → `Applied` | 没有报告组 |
| RCN-033.6 | `AdoptExchange{acct-s, binance-spot}` → `Applied` | 只有 `acct-s` 一组 |

### RCN-034 OmsNeedsRestQuery 只触发 Unknown 收敛这一步,提交的组不含 Summary 且不算一轮

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)、[ReconcileReport](README.md#reconcilereport)
- 级别:integ
- 里程碑:M5
- 前置:基础配置;`acct-o` 的槽 s7 在 t = 0 进入 `Unknown`(年龄已 > 2 000 ms);上一周期在 t = 1 s;t = 5 s 时 journal 写入 `Note{OmsNeedsRestQuery}`(s7);`FakeRest` 查单返回 `filled`、累计 0.02 BTC;`acct-o` 引擎仓位与交易所差 0.03 BTC(在本步不比较)
- 步骤:
  1. 推进到 t = 6 s
- 期望:
  - t ∈ (5 s, 6 s) 内 `FakeCtl` 收到一个报告组,只含一条 `UnknownResolved{slot = s7, order_status = Filled, exch_qty = 0.02, severity = Ignore}`,`count = 1`、`idx = 0`;没有 `Summary`、`Position`
  - `FakeRest` 在这一步只收到查单请求(没有仓位、余额、挂单、资金费请求);下一周期仍在 t = 31 s
  - 这一步不改变 `AUTO_EXTERNAL` 的轮次计数(见 RCN-121)
- 实现:`apps/reconciler/tests/schedule.rs::rcn_034_unknown_only_step`

### RCN-035 Unknown 收敛这一步查不出结果时不提交

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)、[Unknown 订单收敛](README.md#unknown-订单收敛)
- 级别:unit
- 里程碑:M5
- 前置:同 RCN-034,但查单请求返回 HTTP 429
- 步骤:
  1. 推进到 t = 6 s
- 期望:
  - `FakeCtl` 没有收到报告组;`status.json` 中 `acct-o` 的 `unknown_query_fail_rounds = 1`
- 实现:`apps/reconciler/tests/schedule.rs::rcn_035_unknown_step_no_result`

### RCN-036 report_id 单调递增且跨重启不重复

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:基础配置;假时钟 UTC 按行给出;`paths.state` 按行预置上次运行的最大 `report_id`
- 实现:`apps/reconciler/tests/report.rs::rcn_036_report_id`

| 行 | 条件 | 期望 |
|---|---|---|
| RCN-036.1 | 两个 (account, venue) 在同一 UTC 纳秒 `T` 开始一轮,无持久值 | `report_id` 分别为 `T`、`T + 1` |
| RCN-036.2 | 持久值 = `T + 100`,这一轮开始 UTC = `T` | `report_id = T + 101` |
| RCN-036.3 | 持久值 = `T − 1`,这一轮开始 UTC = `T` | `report_id = T` |
| RCN-036.4 | 进程重启(假时钟回拨 1 s) | 重启后第一个 `report_id` = 重启前最大值 + 1 |

### RCN-040 引擎视图在每个快照点与引擎 ledger 逐值相等

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:prop
- 里程碑:M5
- 性质:任意由进程内 hft-core(hft-testkit 假交易所、测试策略)产生的 journal,reconciler 的引擎视图(hft-ledger `JournalLedger`)在每个 `StateSnapshot` 所在 seq 处的永续仓位、每资产现金与现货库存(按 group 与 unassigned)、每 inst 资金费累计,与该快照 ledger 节中引擎 ledger 的值逐值相等
- 生成:1–4 个账户 × 1–2 个 venue;每 journal 200–5 000 个事件,事件从 {下单、成交、部分成交、撤单、`ExternalFill`(各 cause)、`FundingSettle`(推送与合成)、`BalanceReport`、`AdoptExchange`、`SetBalanceBaseline`、`RecordTransfer`、关键帧} 抽取
- 参考:同一 journal 中各 `StateSnapshot` 的 ledger 节(按 hft-ledger 快照行格式解码)
- 例数:PR 200;每夜 5 000
- 实现:`apps/reconciler/tests/prop_view.rs::rcn_040_view_equals_ledger`

### RCN-041 视图含 sequencer 合成的 FundingSettle{synth = 1}

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:unit
- 里程碑:M5
- 前置:合成 journal:`acct-o`(`RateOnly`)在 settle_ts = 08:00:00 UTC 有一条 Output `FundingSettle{inst = BTC-USDT-SWAP, synth = 1, amount = −1.20 USDT}`
- 步骤:
  1. 读取视图在该记录之后的资金费累计
- 期望:
  - `BTC-USDT-SWAP` 在 08:00:00 的已入账金额 = −1.20 USDT;USDT 现金减少 1.20
- 实现:`apps/reconciler/tests/view.rs::rcn_041_view_includes_synth_funding`

### RCN-042 带 AUTO_RECON 的 Note{ExternalFill} 按 hft-ledger 冷读者路径入账,不带的不入账

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)、[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:合成 journal:`acct-p` 引擎仓位 −0.010 BTC;一组 `ReconcileReport`(report_id `R`)的末条到达,同一事件内 sequencer 写出该行的注记
- 实现:`apps/reconciler/tests/view.rs::rcn_042_auto_recon_note`

| 行 | 同一事件内的注记 | 期望:视图中 `acct-p` 的 `BTCUSDT` 仓位 |
|---|---|---|
| RCN-042.1 | `Note{ExternalFill}`,`aflags = AUTO_RECON`,`side = Buy`、`qty = 0.004`、`px = 60 000` | −0.006(经 `auto_external_fill(acct-p, binance-usdm, BTCUSDT, −0.004, 60 000, R)` 与 `apply_external(.., auto_recon = true, ..)`) |
| RCN-042.2 | `Note{ExternalFill}`,`aflags = 0`(热进程对已入账成交的记录) | −0.010(不据它入账) |
| RCN-042.3 | 行 1 的注记在下一事件中再出现一次(同一 `R`) | −0.006(去重键 `(BTCUSDT, 0, R)` 已存在) |

### RCN-043 OMS 防御路径的 Note{OmsForeignFill} 按 hft-ledger 冷读者路径入账

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:unit
- 里程碑:M5
- 前置:合成 journal:`acct-p` 的一条带成交的 `SLOT_UNRESOLVED` 回报(成交 0.002 BTC 买)被 hft-oms 走防御路径,同一事件内有 Output `Note{OmsForeignFill}`(字段按 hft-oms 的布局);`FakeRest` 返回的交易所仓位已包含这笔成交
- 步骤:
  1. 跑一轮
- 期望:
  - 视图中 `BTCUSDT` 仓位已含 +0.002(经 `Ledger::external_from_oms_note` 与 `apply_external(.., auto_recon = false, ..)`);报告组中没有 `Position` 条目
- 实现:`apps/reconciler/tests/view.rs::rcn_043_oms_foreign_fill_note`

### RCN-044 从快照起步的视图与从头重放的视图相同

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:合成 journal 2 h,含关键帧 `K` 于 1 h 处;`Funding` 事件在 30 min 处把 `BTCUSDT` 的结算周期改为 4 h
- 步骤:
  1. 视图 A 从第一个段首快照起重放到末尾
  2. 视图 B 从 `K` 起重放到末尾
- 期望:
  - 两个视图的仓位、现金、库存、槽状态分类、资金费累计逐值相等;`funding_grid(binance-usdm, BTCUSDT)` 两者都返回 (14 400, 锚不变)
- 实现:`apps/reconciler/tests/view.rs::rcn_044_snapshot_start_same_view`

### RCN-045 槽状态只区分终态、非终态、Unknown 与墓碑期

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:合成 journal:`acct-p` 的槽按行处于该状态(以 hft-oms 的只读类型解读快照 OMS 节与其后的 `OrderDecision`/`OrderEvent`/OMS 迁移 Note)
- 实现:`apps/reconciler/tests/view.rs::rcn_045_slot_classes`

| 行 | OMS 状态 | 期望视图分类 |
|---|---|---|
| RCN-045.1 | `PendingNew` | 非终态(在途) |
| RCN-045.2 | `Live` | 非终态(静止) |
| RCN-045.3 | `PendingCancel` | 非终态(在途) |
| RCN-045.4 | `Unknown` | `Unknown` |
| RCN-045.5 | `VERIFY` | `Unknown` 类(在途) |
| RCN-045.6 | `Filled` 且在墓碑期内 | 墓碑期 |
| RCN-045.7 | `Cancelled` 且墓碑期已过 | 终态 |

### RCN-046 一轮使用 S0 时刻的视图副本,journal 之后的前进不改变这一轮的引擎侧值

- 规则:[实现说明](README.md#实现说明)、[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:unit
- 里程碑:M5
- 前置:基础配置;`acct-s` 在 `S0` 时 BTC 引擎库存 1.0;`FakeRest` 余额请求延迟 3 s,期间 journal 写入一笔 `acct-s` 的 BTC 买入成交 0.5(使 BTC 在途)与一笔与 BTC 无关的 BNB 余额调整 `AdoptExchange`
- 步骤:
  1. 跑一轮
- 期望:
  - `Summary.view_seq = S0`;BNB 的比较使用 `S0` 时的引擎值(调整前);BTC 因条件 ② 计入 `skipped_inflight`
- 实现:`apps/reconciler/tests/view.rs::rcn_046_s0_view_copy`

### RCN-047 常规成交经 JournalLedger 从槽映射入账,重复的成交增量按 cum_filled 去重

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:table
- 里程碑:M5
- 前置:合成 journal:`acct-p` 一条 Output `OrderDecision{New}`(槽 (epoch 3, slot 7, gen 1),`BTCUSDT` 买 0.005);起始引擎仓位 0;各行在其后依次追加 `OrderEvent`
- 实现:`apps/reconciler/tests/view.rs::rcn_047_regular_fill_dedup`

| 行 | 追加的 `OrderEvent`(槽 (3, 7, 1)) | 期望:视图中 `acct-p` 的 `BTCUSDT` 仓位 |
|---|---|---|
| RCN-047.1 | `PartialFill{cum_filled = 0.002}` | +0.002 |
| RCN-047.2 | 行 1 之后再来一条 `PartialFill{cum_filled = 0.002}`(重复) | +0.002 |
| RCN-047.3 | 行 2 之后 `Fill{cum_filled = 0.005}` | +0.005 |
| RCN-047.4 | 行 3 之后乱序到达 `PartialFill{cum_filled = 0.004}` | +0.005 |

### RCN-050 在途条件 ①:S0 时有在途槽的 inst 不比对

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-p` 的 `BTCUSDT` 引擎与交易所仓位差 0.005 BTC;`S0` 时 `BTCUSDT` 有一个处于该行状态的槽;`(S0 − 2 000 ms, S1]` 内没有该 inst 的事件
- 实现:`apps/reconciler/tests/inflight.rs::rcn_050_inflight_slot_states`

| 行 | 槽状态 | 期望 |
|---|---|---|
| RCN-050.1 | `PendingNew` | 无 `Position` 条目;`skipped_inflight = 1` |
| RCN-050.2 | 在途撤单 `PendingCancel` | 无 `Position`;`skipped_inflight = 1` |
| RCN-050.3 | 在途改单 `PendingAmend` | 无 `Position`;`skipped_inflight = 1` |
| RCN-050.4 | `Unknown` | 无 `Position`;`skipped_inflight = 1` |
| RCN-050.5 | `VERIFY` | 无 `Position`;`skipped_inflight = 1` |
| RCN-050.6 | 墓碑期内的旧代槽 | 无 `Position`;`skipped_inflight = 1` |
| RCN-050.7 | `Live`(静止挂单) | 一条 `Position{diff 对应 0.005 BTC}`;`skipped_inflight = 0` |
| RCN-050.8 | 只有终态槽 | 一条 `Position`;`skipped_inflight = 0` |

### RCN-051 在途条件 ②:窗口 (S0 − user_stream_lag, S1] 的边界

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`user_stream_lag_ms = {binance-usdm = 2000}`;`S0` 所指记录的引擎时间为 `e`;`BTCUSDT` 仓位差 0.005 BTC;没有在途槽;只有该行的一条事件
- 实现:`apps/reconciler/tests/inflight.rs::rcn_051_window_boundary`

| 行 | 事件 | 期望 |
|---|---|---|
| RCN-051.1 | `OrderEvent`,引擎时间 `e − 2 000 ms` | 不在窗口内:一条 `Position`;`skipped_inflight = 0` |
| RCN-051.2 | `OrderEvent`,引擎时间 `e − 2 000 ms + 1 ns` | 在途:无 `Position`;`skipped_inflight = 1` |
| RCN-051.3 | `ExternalFill`,seq = `S1` | 在途 |
| RCN-051.4 | `ExternalFill`,seq = `S1 + 1` | 不在窗口内:一条 `Position` |
| RCN-051.5 | `OrderDecision`,seq ∈ (`S0`, `S1`) | 在途 |
| RCN-051.6 | `FundingSettle`,seq ∈ (`S0`, `S1`) | 在途 |
| RCN-051.7 | 另一个 inst 的 `OrderEvent`,seq ∈ (`S0`, `S1`) | `BTCUSDT` 不在途:一条 `Position` |
| RCN-051.8 | `user_stream_lag_ms = {binance-usdm = 0}`,`OrderEvent` 引擎时间 = `e`、seq < `S0` | 不在窗口内 |

### RCN-052 资产的在途跟随以它为基础或计价资产的 inst

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-s` 的 BTC、USDT、BNB 三个资产都与交易所差 2 × `dust`;`S0` 时 inst 按行处于在途
- 实现:`apps/reconciler/tests/inflight.rs::rcn_052_asset_inflight`

| 行 | 在途 inst | 期望 |
|---|---|---|
| RCN-052.1 | `BTCUSDT`(现货) | BTC 与 USDT 不比对;BNB 一条 `Balance`;`skipped_inflight = 3`(1 个 inst + 2 个资产) |
| RCN-052.2 | `BNBUSDT` | BNB 与 USDT 不比对;BTC 一条 `Balance`;`skipped_inflight = 3` |
| RCN-052.3 | 无 | 三条 `Balance`;`skipped_inflight = 0` |

### RCN-053 连续 3 轮在途后按 S1 视图强制报告,且不计入 skipped_inflight

- 规则:[对账周期与比对基准](README.md#对账周期与比对基准)、[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`BTCUSDT` 在每一轮的条件 ② 窗口内都有一笔成交;`S1` 时引擎仓位 −0.010,交易所 −0.008
- 实现:`apps/reconciler/tests/inflight.rs::rcn_053_forced_after_three`

| 行 | 轮次 | 期望 |
|---|---|---|
| RCN-053.1 | 轮 1 | 无 `Position`;`skipped_inflight = 1` |
| RCN-053.2 | 轮 2 | 无 `Position`;`skipped_inflight = 1` |
| RCN-053.3 | 轮 3 | 一条 `Position{engine_qty = −0.010(S1 视图), exch_qty = −0.008}`;`skipped_inflight = 0` |
| RCN-053.4 | 轮 4(不再在途) | 按 `S0` 视图比对;连续在途计数清零 |

### RCN-054 skipped_inflight 等于跳过的项数并饱和于 65 535

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:差异计算纯函数 `diff::compute` 的输入:在途项数按行给出
- 实现:`apps/reconciler/tests/diff.rs::rcn_054_skipped_inflight_count`

| 行 | 在途项数 | 期望 `skipped_inflight` |
|---|---|---|
| RCN-054.1 | 5(5 个 inst) | 5 |
| RCN-054.2 | 65 534 | 65 534 |
| RCN-054.3 | 65 535 | 65 535 |
| RCN-054.4 | 65 536 | 65 535 |

### RCN-060 永续仓位差:≥ 1 lot 产生 Position 条目,< 1 lot 不产生

- 规则:[比对项](README.md#比对项)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-p` 的 `BTCUSDT` 引擎仓位 −0.010 BTC;无在途;交易所仓位按行给出;mark = 60 000
- 实现:`apps/reconciler/tests/items.rs::rcn_060_position_tolerance`

| 行 | 交易所仓位 | 期望 |
|---|---|---|
| RCN-060.1 | −0.010 | 无 `Position`;`Summary.severity = Ignore` |
| RCN-060.2 | −0.0109(差 0.9 lot) | 无 `Position` |
| RCN-060.3 | −0.009(差 1 lot) | 一条 `Position{inst = BTCUSDT, engine_qty = −0.010, exch_qty = −0.009, diff = 60.00 USDT, severity = Warn, view_seq = S0}`;`Summary.severity = Warn` |
| RCN-060.4 | −0.0089(差 1.1 lot) | 一条 `Position`,`diff = 66.00 USDT` |
| RCN-060.5 | +0.010(变号) | 一条 `Position`,`diff = 1 200.00 USDT` |

### RCN-061 余额差:≥ 资产 dust 产生 Balance 条目,< dust 不产生

- 规则:[比对项](README.md#比对项)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-s` 的 BNB 引擎余额(现金 + 库存 + 基线)1.000;无在途;交易所 BNB 余额(free + locked)按行给出;BNB `dust` = 0.001
- 实现:`apps/reconciler/tests/items.rs::rcn_061_balance_tolerance`

| 行 | 交易所 free + locked | 期望 |
|---|---|---|
| RCN-061.1 | 1.000 | 无 `Balance` |
| RCN-061.2 | 1.0009 | 无 `Balance` |
| RCN-061.3 | 1.001 | 一条 `Balance{asset = BNB, engine_qty = 1.000, exch_qty = 1.001, severity = Warn}` |
| RCN-061.4 | 0.999(free 0.5 + locked 0.499) | 一条 `Balance{exch_qty = 0.999}`(free 与 locked 合计) |

### RCN-062 引擎侧余额 = 各 group 库存 + unassigned + 现金 + 基线

- 规则:[现货库存与余额基线](README.md#现货库存与余额基线)、[比对项](README.md#比对项)
- 级别:unit
- 里程碑:M5
- 前置:合成 journal:`acct-s` 的 BTC:group g1 库存 0.3、g2 库存 0.2、unassigned 0.05,无基线;交易所 BTC 余额 0.55
- 步骤:
  1. 跑一轮
- 期望:
  - 无 `Balance` 条目(引擎侧 0.55 = 交易所 0.55)
- 实现:`apps/reconciler/tests/items.rs::rcn_062_engine_balance_composition`

### RCN-063 孤儿单:交易所有、引擎不知道的挂单产生 OpenOrder{slot = 0}

- 规则:[比对项](README.md#比对项)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-p` 引擎视图中有一个 `Live` 槽 s3(epoch 4, gen 2);`FakeRest` 的挂单列表按行给出
- 实现:`apps/reconciler/tests/items.rs::rcn_063_orphan_orders`

| 行 | 交易所挂单 | 期望 |
|---|---|---|
| RCN-063.1 | 只有 s3 的单(client id 解出 (4, s3, 2)),剩余量与引擎相同 | 无 `OpenOrder` |
| RCN-063.2 | s3 的单 + 一张 client id 为 `web123` 的单(剩余 0.004) | 一条 `OpenOrder{inst = BTCUSDT, slot = 0, gen = 0, epoch = 0, exch_qty = 0.004, severity = Warn}` |
| RCN-063.3 | s3 的单 + 一张 client id 解出 (4, s9, 1) 的单(引擎中 s9 是终态) | 一条 `OpenOrder{slot = 0, exch_qty = 该单剩余量}` |
| RCN-063.4 | s3 的单 + 一张 client id 解出 (3, s3, 2) 的单(旧 epoch) | 一条 `OpenOrder{slot = 0}` |
| RCN-063.5 | client id 校验位错误的单 | 一条 `OpenOrder{slot = 0}` |

### RCN-064 幽灵单:引擎 Live 而交易所没有的槽产生 OpenOrder{exch_qty = 0} 并查一次该单

- 规则:[比对项](README.md#比对项)
- 级别:unit
- 里程碑:M5
- 前置:基础配置;`acct-p` 的 `Live` 槽 s3(epoch 4, gen 2);`FakeRest` 挂单列表为空;按 client id 查单返回 `CANCELED`
- 步骤:
  1. 跑一轮
- 期望:
  - 一条 `OpenOrder{inst = BTCUSDT, slot = s3, gen = 2, epoch = 4, exch_qty = 0, severity = Warn}`
  - `FakeRest` 收到一次按 client id 查 s3 的请求;`rounds/<report_id>.json` 记录查询结果 `CANCELED`;报告组中没有关于 s3 的 `UnknownResolved`
- 实现:`apps/reconciler/tests/items.rs::rcn_064_phantom_order`

### RCN-065 配对挂单的剩余数量差:≥ 1 lot 报告,< 1 lot 不报

- 规则:[比对项](README.md#比对项)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`Live` 槽 s3 引擎剩余量 0.010 BTC;交易所同一单的剩余量按行给出
- 实现:`apps/reconciler/tests/items.rs::rcn_065_remaining_qty`

| 行 | 交易所剩余量 | 期望 |
|---|---|---|
| RCN-065.1 | 0.010 | 无 `OpenOrder` |
| RCN-065.2 | 0.0101 | 无 `OpenOrder` |
| RCN-065.3 | 0.009 | 一条 `OpenOrder{slot = s3, engine_qty = 0.010, exch_qty = 0.009, severity = Warn}` |

### RCN-066 冒烟单按本系统挂单处理,到期撤单后下一轮无差异

- 规则:[比对项](README.md#比对项)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-core 与假交易所;`SmokeOrder{acct-p, binance-usdm, BTCUSDT, Buy, ttl_ms = 60 000}` 建出带 `OrderFlags::SMOKE` 的槽并在假交易所挂单
- 步骤:
  1. 挂单后跑一轮
  2. `ttl_ms` 到期、OMS 撤单完成后再跑一轮
- 期望:
  - 两轮的报告组都没有 `OpenOrder` 条目,`Summary.severity = Ignore`
- 实现:`apps/reconciler/tests/items.rs::rcn_066_smoke_order_own`

### RCN-067 缺失与多出的成交写进轮次明细,自身强平订单被标注

- 规则:[比对项](README.md#比对项)、[与 gw 的分工](README.md#与-gw-的分工)
- 级别:unit
- 里程碑:M5
- 前置:基础配置;`acct-p`;`FakeRest` 的成交列表比 journal 多一笔交易所订单 id `O1`、成交 id `T1` 的成交(该单带 venue 的强平标记 `own_liquidation_marker`),并少一笔 journal 中有的成交 `T2`;交易所仓位因 `T1` 比引擎少 0.003 BTC
- 步骤:
  1. 跑一轮
- 期望:
  - 报告组有一条 `Position{engine_qty − exch_qty = 0.003}`
  - `rounds/<report_id>.json` 的缺失成交列表含 `{exch_order_id = O1, trade_id = T1, 时间, liquidation = true}`,多出成交列表含 `T2`
- 实现:`apps/reconciler/tests/items.rs::rcn_067_fill_diff_details`

### RCN-068 dust 取 journal 中最新生效的 refdata

- 规则:[比对项](README.md#比对项)
- 级别:unit
- 里程碑:M5
- 前置:合成 journal:refdata 中 BNB `AssetMeta.dust` 初值 0.001;轮 2 之前 journal 出现新的 refdata `ConfigBlob`(hft-core 经 `ReplaceRefdata` 重启后的引导块),BNB `dust` = 0.01;`refdata_blob` 文件不变;交易所与引擎 BNB 差 0.005
- 步骤:
  1. 跑轮 1
  2. 写入新的 refdata 引导块后跑轮 2
- 期望:
  - 轮 1 有一条 `Balance{asset = BNB}`;轮 2 没有
- 实现:`apps/reconciler/tests/items.rs::rcn_068_tolerance_from_journal`

### RCN-069 Summary 的 severity 与各条目的 severity

- 规则:[比对项](README.md#比对项)、[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:差异计算纯函数的输入按行给出
- 实现:`apps/reconciler/tests/diff.rs::rcn_069_severity`

| 行 | 本组条目 | 期望 |
|---|---|---|
| RCN-069.1 | 无差异条目 | `Summary.severity = Ignore` |
| RCN-069.2 | 一条 `Position`(Warn) | `Summary.severity = Warn` |
| RCN-069.3 | 只有一条 `Funding`(Warn) | `Summary.severity = Warn` |
| RCN-069.4 | 只有一条 `UnknownResolved` | 该条 `severity = Ignore`;`Summary.severity = Ignore` |
| RCN-069.5 | 无差异条目但置 `TRUNCATED` | `Summary.severity = Warn` |

### RCN-070 报告组的条目顺序、idx 与 count

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:unit
- 里程碑:M5
- 前置:差异计算输出:`UnknownResolved` 2 条(slot 9、slot 4)、`OpenOrder` 1 条、`Position` 2 条(inst 12、inst 7)、`Funding` 2 条(inst 7 的 08:00 与 inst 7 的 00:00)、`Balance` 1 条
- 步骤:
  1. 组装报告组
- 期望:
  - `count = 9`;`idx` 0–8 依次为:`Summary`、`UnknownResolved(slot 4)`、`UnknownResolved(slot 9)`、`OpenOrder`、`Position(inst 7)`、`Position(inst 12)`、`Funding(inst 7, 00:00)`、`Funding(inst 7, 08:00)`、`Balance`;全部条目 `report_id` 相同
- 实现:`apps/reconciler/tests/report.rs::rcn_070_entry_order`

### RCN-071 超过 256 条时截断尾部并置 TRUNCATED,被截掉的下一轮再报

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:差异计算输出 n 条非 Summary 条目(按行),其中最后 10 条是 `Balance`;轮 2 交易所状态不变
- 实现:`apps/reconciler/tests/report.rs::rcn_071_truncation`

| 行 | n | 期望 |
|---|---|---|
| RCN-071.1 | 254 | `count = 255`;无 `TRUNCATED` |
| RCN-071.2 | 255 | `count = 256`;无 `TRUNCATED` |
| RCN-071.3 | 256 | `count = 256`;`TRUNCATED`;最后一条 `Balance` 被截去 |
| RCN-071.4 | 299 | `count = 256`;`TRUNCATED`;被截去的 44 条在下一轮仍出现(下一轮同样截断时仍按同一顺序) |

### RCN-072 截断时资金费修正只取本组实际包含的 Funding 条目

- 规则:[ReconcileReport](README.md#reconcilereport)、[资金费比对](README.md#资金费比对)
- 级别:unit
- 里程碑:M5
- 前置:差异计算输入:`acct-p` 有 300 个超出容差的资金费差,每笔 −0.10 USDT;USDT 余额差 −30.00 USDT;没有其他差异
- 步骤:
  1. 组装报告组
- 期望:
  - 本组含 `Summary` + 255 条 `Funding`(按 `(inst, exch_ts_ns)` 升序的前 255 笔),`TRUNCATED`;USDT 的 `funding_adj` = −25.50;因此 USDT 余额修正后差 −4.50 ≥ `dust`,但 `Balance` 条目排在 `Funding` 之后、已无位置,被截去(不出现)
  - 轮次明细记录 `funding_adj = −25.50` 与被截去的条目数 46(45 条 `Funding` 与 1 条 `Balance`)
- 实现:`apps/reconciler/tests/report.rs::rcn_072_truncation_funding_adj`

### RCN-073 报告组经 hft-ctl 以 hft-recon 身份一次提交

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:integ
- 里程碑:M5
- 前置:基础配置;进程内 hft-core 的 ctl-gateway(真实)与 sequencer;reconciler 进程以 uid `hft-recon` 运行
- 步骤:
  1. 跑一轮(报告组 5 条)
- 期望:
  - ctl 环上依次出现 5 条 `ReconcileReport`,`report_id` 相同、`idx` 0–4、`count = 5`,中间没有其他来源的记录;一条 `Note{source = CtlAudit}` 的 issuer 为 `reconciler`、`cmd_kind = 0`
  - sequencer 在第 5 条到达后整组生效(`risk.on_reconcile` 调用 1 次)
- 实现:`apps/reconciler/tests/ctl.rs::rcn_073_submit_via_ctl`

### RCN-074 reconciler 身份发出的控制命令被拒

- 规则:[ReconcileReport](README.md#reconcilereport)、[启动闸门与严格放行](README.md#启动闸门与严格放行)
- 级别:proc
- 里程碑:M5
- 前置:真实 ctl-gateway 进程;客户端以 uid `hft-recon`、issuer `reconciler` 连接
- 步骤:
  1. 发送 `SetMode{acct-p → Live}`
  2. 发送 `Resume{Recon, acct-p, binance-usdm}`
- 期望:
  - 两次都返回 `Forbidden`;ctl 环中没有对应的 `ControlCmd`;各有一条 `Note{CtlAudit}` 记录被拒
- 实现:`apps/reconciler/tests/ctl.rs::rcn_074_commands_forbidden`

### RCN-075 每种条目的字段填法

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:差异计算输出每种条目各一条;检查编码后的 `ReconcileReport` 记录(hft-event 布局,schema_version 3)
- 实现:`apps/reconciler/tests/report.rs::rcn_075_fields_per_item`

| 行 | `item` | 期望非零字段;其余字段为 0 |
|---|---|---|
| RCN-075.1 | `Summary` | `venue`、`account`、`severity`、`view_seq`、`skipped_inflight`(有在途时)、`sflags`(有标志时) |
| RCN-075.2 | `Position` | `inst`、`engine_qty`、`exch_qty`、`diff`、`exch_ts_ns`、`view_seq`、`severity`;`sflags` 只可能含 `AUTO_EXTERNAL` |
| RCN-075.3 | `Balance` | `asset`、`engine_qty`、`exch_qty`、`diff`、`view_seq`、`severity`;`sflags = 0` |
| RCN-075.4 | `OpenOrder`(幽灵单) | `inst`、`slot`、`gen`、`epoch`、`severity`;`exch_qty = 0` |
| RCN-075.5 | `Funding` | `account`、`venue`、`inst`、`exch_ts_ns`、`exch_amount`、`engine_amount`、`diff = exch_amount − engine_amount`、`severity` |
| RCN-075.6 | `UnknownResolved` | `slot`、`gen`、`epoch`、`exch_qty`、`exch_ts_ns`、`order_status`、`history_checked`;`severity = Ignore` |

### RCN-076 USERSTREAM_DOWN 的置位条件

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`user_stream_lag_ms = {binance-usdm = 2000}`;`acct-p` 的用户流连接;`S0` 所指记录的引擎时间为 `e`;只有该行的 `ConnState` 记录
- 实现:`apps/reconciler/tests/report.rs::rcn_076_userstream_down_flag`

| 行 | 用户流的 `ConnState` | 期望 `Summary.sflags` |
|---|---|---|
| RCN-076.1 | 一直 Up | 无 `USERSTREAM_DOWN` |
| RCN-076.2 | `Down` 于 seq ∈ (`S0`, `S1`],之后在 `S1` 前恢复 Up | `USERSTREAM_DOWN` |
| RCN-076.3 | `Down` 于引擎时间 `e − 1 000 ms`(在窗口内),`e − 500 ms` 恢复 Up | `USERSTREAM_DOWN` |
| RCN-076.4 | `Down` 于引擎时间 `e − 2 000 ms`(窗口外),`e − 1 500 ms` 恢复 Up | 无 `USERSTREAM_DOWN` |
| RCN-076.5 | `S0` 时处于 Down(更早断开,窗口内无新的 Down 记录) | `USERSTREAM_DOWN` |
| RCN-076.6 | 另一账户 `acct-o` 的用户流在窗口内 Down | `acct-p` 的组无 `USERSTREAM_DOWN` |
| RCN-076.7 | `acct-p` 的 BBO 行情连接在窗口内 Down | 无 `USERSTREAM_DOWN` |

### RCN-080 PerSymbolPush 错配一笔 3 USDT:产生字段齐全的 Funding 条目,结算资产余额差被修正掉

- 规则:[资金费比对](README.md#资金费比对)
- 级别:unit
- 里程碑:M5
- 前置:基础配置;`acct-p`(`PerSymbolPush`);`S0_utc` = 2026-10-03T12:00:00Z;`BTCUSDT` 在 08:00:00Z 的推送入账额 −1.50 USDT;`FakeRest` 资金费流水:`BTCUSDT`、08:00:00Z、−4.50 USDT;交易所 USDT 余额比引擎少 3.00;仓位一致、无在途
- 步骤:
  1. 跑一轮
- 期望:
  - 一条 `Funding{account = acct-p, venue = binance-usdm, inst = BTCUSDT, exch_ts_ns = 08:00:00Z, exch_amount = −4.50, engine_amount = −1.50, diff = −3.00, severity = Warn}`
  - 没有 `Balance` 条目(USDT 修正后差 0);轮次明细记 `funding_adj(USDT) = −3.00`;`Summary.severity = Warn`
- 实现:`apps/reconciler/tests/funding.rs::rcn_080_per_symbol_push_mismatch`

### RCN-081 账本按 Funding 条目校正后下一轮该笔不再出现

- 规则:[资金费比对](README.md#资金费比对)
- 级别:integ
- 里程碑:M5
- 前置:RCN-080 的状态;`FakeCtl` 把报告组交给进程内 sequencer(真实 hft-ledger 与 hft-risk)
- 步骤:
  1. 跑一轮(同 RCN-080)
  2. sequencer 处理该组后,再跑一轮(交易所状态不变)
- 期望:
  - 步骤 1 之后 journal 中有 `Note{source = Ledger, code = FundingCorrected}`(`v[5] = diff = −3.00`)
  - 步骤 2 的报告组没有 `Funding` 与 `Balance` 条目,`Summary.severity = Ignore`
  - 两轮之后 `acct-p` 的 `RECONCILED` 位(hft-risk)为 1;没有 `Recon` cap
- 实现:`apps/reconciler/tests/funding.rs::rcn_081_corrected_next_round_clean`

### RCN-082 RateOnly 所的合成额与交易所流水逐笔比对

- 规则:[资金费比对](README.md#资金费比对)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-o`(`RateOnly`);08:00:00Z 合成入账额 −1.20 USDT;`FakeRest` 流水金额按行给出;`S0_utc` = 12:00:00Z
- 实现:`apps/reconciler/tests/funding.rs::rcn_082_rate_only`

| 行 | 交易所流水 | 期望 |
|---|---|---|
| RCN-082.1 | −1.20 | 无 `Funding` 条目 |
| RCN-082.2 | −1.21(差 0.01 = `funding_dust`) | 一条 `Funding{exch_amount = −1.21, engine_amount = −1.20, diff = −0.01}` |
| RCN-082.3 | −1.209(差 0.009) | 无 `Funding` 条目 |
| RCN-082.4 | 无该笔(交易所没有) | 一条 `Funding{exch_amount = 0, engine_amount = −1.20, diff = +1.20}` |

### RCN-083 只有交易所有的一笔:engine_amount = 0

- 规则:[资金费比对](README.md#资金费比对)
- 级别:unit
- 里程碑:M5
- 前置:基础配置;`acct-p`;引擎没有 16:00:00Z 的 `BTCUSDT` 入账;`FakeRest` 流水有 16:00:00Z、+0.80 USDT;`S0_utc` = 17:00:00Z
- 步骤:
  1. 跑一轮
- 期望:
  - 一条 `Funding{inst = BTCUSDT, exch_ts_ns = 16:00:00Z, exch_amount = +0.80, engine_amount = 0, diff = +0.80}`
- 实现:`apps/reconciler/tests/funding.rs::rcn_083_exchange_only`

### RCN-084 比对窗口 [S0_utc − funding_recon_window_ms, S0_utc − funding_recon_lag_ms] 的边界

- 规则:[资金费比对](README.md#资金费比对)
- 级别:table
- 里程碑:M5
- 前置:基础配置;journal 引擎配置 `funding_recon_window_ms = 172 800 000`、`funding_recon_lag_ms = 600 000`;`S0_utc` = `T`;`acct-p` 在该行的结算时刻有一笔交易所 −2.00、引擎 0 的差
- 实现:`apps/reconciler/tests/funding.rs::rcn_084_window_boundary`

| 行 | 结算时刻(归一化后) | 期望 |
|---|---|---|
| RCN-084.1 | `T − 600 000 ms` | 一条 `Funding` |
| RCN-084.2 | `T − 599 999 ms` | 无 `Funding`(太新) |
| RCN-084.3 | `T − 172 800 000 ms` | 一条 `Funding` |
| RCN-084.4 | `T − 172 800 001 ms` | 无 `Funding`(太旧,不再比对) |

### RCN-085 窗口参数取 journal 中最新生效的引擎配置,不读本 app 配置

- 规则:[资金费比对](README.md#资金费比对)
- 级别:unit
- 里程碑:M5
- 前置:合成 journal:引导块 `ConfigBlob{Engine}` 的 ledger 段 `funding_recon_lag_ms = 600 000`;之后一条生效的引擎配置把它改为 3 600 000;`S0_utc` = `T`;结算时刻 `T − 1 800 000 ms` 有一笔差
- 步骤:
  1. 在新配置生效之前跑一轮
  2. 新配置生效之后跑一轮
- 期望:
  - 步骤 1 有一条 `Funding`;步骤 2 没有(1 800 000 ms < 3 600 000 ms,太新)
- 实现:`apps/reconciler/tests/funding.rs::rcn_085_window_params_from_journal`

### RCN-086 流水结算时刻经 funding_settle_norm_ns 归一化后配对

- 规则:[资金费比对](README.md#资金费比对)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-p` 的 `BTCUSDT` 网格 `funding_grid = (28 800, 0)`;引擎在 08:00:00.000Z 入账 −1.50;`FakeRest` 流水 −1.50,原始时刻按行给出
- 实现:`apps/reconciler/tests/funding.rs::rcn_086_settle_ts_normalized`

| 行 | 流水原始时刻 | 期望 |
|---|---|---|
| RCN-086.1 | 08:00:00.800Z | 归一化为 08:00:00Z;配对成功,无 `Funding` 条目 |
| RCN-086.2 | 07:59:58.500Z | 归一化为 08:00:00Z;无 `Funding` 条目 |
| RCN-086.3 | 08:00:00.000Z | 无 `Funding` 条目 |

### RCN-087 结算周期变化后用视图维护的新网格归一化

- 规则:[资金费比对](README.md#资金费比对)
- 级别:unit
- 里程碑:M5
- 前置:合成 journal:`BTCUSDT` 初始 `funding_interval_s = 28 800`;一条 `Funding` Input 带 `interval_s = 14 400`;之后引擎在 12:00:00Z 入账 −0.70;`FakeRest` 流水 12:00:01.200Z、−0.70
- 步骤:
  1. 跑一轮
- 期望:
  - 视图 `funding_grid(binance-usdm, BTCUSDT) = (14 400, 0)`;流水归一化为 12:00:00Z;无 `Funding` 条目
  - 对照:若仍按 28 800 归一化会落到 08:00:00Z 或 16:00:00Z 并产生 2 条 `Funding`(测试断言这种情况没有发生)
- 实现:`apps/reconciler/tests/funding.rs::rcn_087_grid_follows_interval_change`

### RCN-088 BalanceOnlyPush 推送丢失:一条账户级 Funding 条目,余额差被修正

- 规则:[资金费比对](README.md#资金费比对)
- 级别:unit
- 里程碑:M5
- 前置:基础配置;`acct-g`(`BalanceOnlyPush`);08:00:00Z 的钱包推送丢失(已入钱包额 0);合成额 `BTCUSDT` −2.90、`ETHUSDT` −1.10;`FakeRest` 流水 `BTCUSDT` −3.00、`ETHUSDT` −1.00;交易所 USDT 余额比引擎少 4.00
- 步骤:
  1. 跑一轮
- 期望:
  - 恰好一条 `Funding{inst = INST_NONE, exch_ts_ns = 08:00:00Z, exch_amount = −4.00, engine_amount = 0, diff = −4.00}`;没有按 inst 的 `Funding` 条目
  - 没有 `Balance` 条目(USDT 差 −4.00 修正为 0)
- 实现:`apps/reconciler/tests/funding.rs::rcn_088_balance_only_push_lost`

### RCN-089 BalanceOnlyPush 推送重复到达只计一次,无条目

- 规则:[资金费比对](README.md#资金费比对)
- 级别:unit
- 里程碑:M5
- 前置:同 RCN-088,但 08:00:00Z 的推送 −4.00 到达两次(第二次被 hft-ledger 判为 `Duplicate`);交易所 USDT 余额与引擎一致
- 步骤:
  1. 跑一轮
- 期望:
  - 视图的已入钱包额 = −4.00;没有 `Funding` 条目,没有 `Balance` 条目
- 实现:`apps/reconciler/tests/funding.rs::rcn_089_duplicate_push_once`

### RCN-090 BalanceOnlyPush 各 inst 合成额与流水的归属差不产生条目

- 规则:[资金费比对](README.md#资金费比对)
- 级别:unit
- 里程碑:M5
- 前置:同 RCN-088,但推送正常到达(已入钱包额 −4.00)
- 步骤:
  1. 跑一轮
- 期望:
  - 没有 `Funding` 条目(钱包一致;`BTCUSDT` 合成 −2.90 vs 流水 −3.00 的归属差 0.10 不进条目)
- 实现:`apps/reconciler/tests/funding.rs::rcn_090_attribution_diff_no_entry`

### RCN-091 资金费流水查询失败:无 Funding 条目、不做余额修正、其他比对项照常

- 规则:[资金费比对](README.md#资金费比对)
- 级别:unit
- 里程碑:M5
- 前置:RCN-080 的状态,但资金费流水请求返回 HTTP 503;另有 `BTCUSDT` 仓位差 1 lot
- 步骤:
  1. 跑一轮
- 期望:
  - 没有 `Funding` 条目;一条 `Position`;一条 `Balance{asset = USDT, diff = −3.00}`(未修正);轮次明细没有 `funding_adj`
  - `status.json` 中 `acct-p` 的 `funding_query_fail_rounds = 1`、`alert = null`
- 实现:`apps/reconciler/tests/funding.rs::rcn_091_funding_query_fail`

### RCN-092 资金费流水连续 unknown_alert_rounds 轮查询失败时 status.json 报 P1,成功一次即清零

- 规则:[资金费比对](README.md#资金费比对)、[Unknown 订单收敛](README.md#unknown-订单收敛)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`unknown_alert_rounds = 3`;`acct-p` 的资金费流水请求按行逐轮返回
- 实现:`apps/reconciler/tests/funding.rs::rcn_092_funding_fail_alert`

| 行 | 轮 1–4 的结果 | 期望(轮 4 之后的 `status.json` 中 `acct-p` 项) |
|---|---|---|
| RCN-092.1 | 失败、失败、成功、失败 | `funding_query_fail_rounds = 1`、`alert = null` |
| RCN-092.2 | 失败、失败、失败、成功 | 轮 3 后 `alert = "P1"`;轮 4 后计数 0、`alert = null` |
| RCN-092.3 | 失败 × 4 | `funding_query_fail_rounds = 4`、`alert = "P1"` |

### RCN-093 结算资产余额的资金费修正(testing FD-2 的 reconciler 一侧)

- 规则:[资金费比对](README.md#资金费比对)
- 级别:table
- 里程碑:M5
- 前置:差异计算纯函数;`acct-p`;USDT `dust` = 0.01;本组 `Funding` 条目与 USDT 余额原始差(交易所 − 引擎)按行给出
- 实现:`apps/reconciler/tests/diff.rs::rcn_093_funding_balance_correction`

| 行 | 本组 `Funding` 条目的 diff | USDT 余额原始差 | 期望 |
|---|---|---|---|
| RCN-093.1 | −3.00 | −3.00 | 无 `Balance` 条目;轮次明细 `funding_adj = −3.00` |
| RCN-093.2 | +2.00(inst A)、−5.00(inst B) | −3.00 | 无 `Balance` 条目 |
| RCN-093.3 | −1.00 | −3.00 | 一条 `Balance{asset = USDT, engine_qty = 引擎值 − 1.00, exch_qty = 交易所值, diff 对应 −2.00, severity = Warn}` |
| RCN-093.4 | −3.00 | −3.009 | 无 `Balance` 条目(修正后差 −0.009 < `dust`) |
| RCN-093.5 | −3.00 | −3.01 | 一条 `Balance`(修正后差 −0.01 = `dust`) |
| RCN-093.6 | 无 | −3.00 | 一条 `Balance`(未修正) |

### RCN-094 非结算资产不受资金费修正影响

- 规则:[资金费比对](README.md#资金费比对)
- 级别:unit
- 里程碑:M5
- 前置:差异计算纯函数;`acct-p` 的结算资产 USDT;本组一条 `Funding{diff = −3.00}`;BNB 余额原始差 −3.00 BNB(BNB `dust` = 0.001)
- 步骤:
  1. 计算差异
- 期望:
  - 一条 `Balance{asset = BNB, diff 对应 −3.00 BNB}`(未修正);USDT 若无原始差,则出现一条 `Balance{asset = USDT}`,其修正后差 = +3.00(交易所 − (引擎 + funding_adj))
- 实现:`apps/reconciler/tests/diff.rs::rcn_094_non_settlement_asset`

### RCN-095 RateOnly 所在最近 lag 内的合成额计入结算资产余额的容差

- 规则:[资金费比对](README.md#资金费比对)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-o`(`RateOnly`);`S0_utc` = 16:05:00Z(16:00:00Z 结算落在最近 600 000 ms 内,不比对);16:00:00Z 合成额 −1.20 USDT;USDT 余额原始差按行给出(交易所 − 引擎)
- 实现:`apps/reconciler/tests/funding.rs::rcn_095_rate_only_lag_tolerance`

| 行 | USDT 原始差 | 期望 |
|---|---|---|
| RCN-095.1 | +1.20(交易所实收少付) | 无 `Balance`(容差 = 0.01 + 1.20) |
| RCN-095.2 | +1.209 | 无 `Balance` |
| RCN-095.3 | +1.21 | 一条 `Balance`(= 容差) |
| RCN-095.4 | 同行 1(交易所该笔流水为 0),`S0_utc` = 16:10:01Z(结算已出 lag 窗口) | 一条 `Funding{exch_amount = 0, engine_amount = −1.20, diff = +1.20}`;余额修正后差 0,无 `Balance` |

### RCN-096 BalanceOnlyPush 所在最近 lag 内未收到推送的结算按合成额计入容差

- 规则:[资金费比对](README.md#资金费比对)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-g`(`BalanceOnlyPush`);`S0_utc` = 16:05:00Z;16:00:00Z 合成额合计 −4.00(`BTCUSDT` −2.90、`ETHUSDT` −1.10);推送是否已到按行给出;交易所 USDT 已扣 4.00(原始差 −4.00)
- 实现:`apps/reconciler/tests/funding.rs::rcn_096_balance_only_lag_tolerance`

| 行 | 16:00:00Z 的推送 | 期望 |
|---|---|---|
| RCN-096.1 | 未到 | 无 `Balance`(容差 = 0.01 + 4.00) |
| RCN-096.2 | 已到(钱包已入 −4.00,原始差 0) | 无 `Balance`;容差不加合成额 |
| RCN-096.3 | 未到,且交易所另多扣 1.00(原始差 −5.00) | 一条 `Balance` |

### RCN-097 资金费游标丢失时回退到 funding_recon_window_ms 窗口并去重

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:基础配置;`paths.state` 中 `acct-p` 的资金费游标文件被删除;`FakeRest` 流水在 48 h 窗口内有 6 笔,其中 2 笔在分页的两页中重复出现
- 步骤:
  1. 跑一轮
- 期望:
  - 资金费流水请求的起始时刻 = `S0_utc − 172 800 000 ms`;参与比对的流水为 6 笔(重复的只计一次);游标文件重新写出
- 实现:`apps/reconciler/tests/funding.rs::rcn_097_cursor_lost_fallback`

### RCN-098 差异计算纯函数的性质

- 规则:[比对项](README.md#比对项)、[资金费比对](README.md#资金费比对)、[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M5
- 性质:对任意输入:(a) 仓位差 < 1 lot 的项从不产生条目,≥ 1 lot 的项总是产生 `Warn` 条目;(b) 把某项的差额绝对值增大(其余不变)不会使它从"有条目"变为"无条目";(c) 结算资产的余额条目当且仅当 `|exch_qty − (engine_qty + funding_adj)| ≥ dust` 时产生,`funding_adj` = 本组 `Funding` 条目中该资产 `exch_amount − engine_amount` 之和;(d) 非结算资产的条目与 `Funding` 条目无关;(e) 同一输入两次调用输出逐字节相同
- 生成:1–8 个永续 inst、1–6 个资产;引擎与交易所数量在 ±10^6 lot 内(定点 raw 值随机,含 0、±1 lot、±(lot − 1 raw));0–20 条资金费差,金额 ±10^4 USDT;dust 1–10^6 raw
- 参考:`apps/reconciler/tests/model.rs` 的逐项相减参考实现(只用 `i128`)
- 例数:PR 2 000;每夜 100 000
- 实现:`apps/reconciler/tests/prop_diff.rs::rcn_098_diff_properties`

### RCN-100 SetBalanceBaseline 之后平台币与尘埃不再报差

- 规则:[现货库存与余额基线](README.md#现货库存与余额基线)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-core;`acct-s` 的交易所余额含 BNB 0.37(平台币)与 DOGE 12(历史遗留),引擎中两者不属于任何 group;轮 1 报出 `Balance{BNB}`、`Balance{DOGE}`
- 步骤:
  1. 发 `SetBalanceBaseline{acct-s, binance-spot}`(Applied)
  2. 观察随即触发的一轮,与再下一个周期轮
- 期望:
  - 两轮都没有 BNB、DOGE 的 `Balance` 条目;引擎视图中 BNB 基线 = 0.37、DOGE 基线 = 12
  - 之后交易所 BNB 因扣费变为 0.36 → 下一轮一条 `Balance{BNB, diff 对应 −0.01}`(基线只由命令改变)
- 实现:`apps/reconciler/tests/baseline.rs::rcn_100_baseline_stops_reporting`

### RCN-101 网页手工卖出现货:Warn 持续到 Adopt,Adopt 后下一轮无差异

- 规则:[现货库存与余额基线](README.md#现货库存与余额基线)、[启动闸门与严格放行](README.md#启动闸门与严格放行)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-core;`acct-s` 的 group g1 有 BTC 0.5;交易所上人工卖出 0.1 BTC(引擎不知道)
- 步骤:
  1. 连续跑 3 轮
  2. 以轮 3 报告的 `(report_id, idx)` 发 `AdoptExchange{acct-s, binance-spot, Balance, asset = BTC, group = g1}`(Applied)
  3. 观察随即触发的一轮
- 期望:
  - 步骤 1 的 3 轮各有一条 `Balance{asset = BTC, engine_qty = 0.5, exch_qty = 0.4, severity = Warn}`(USDT 也有一条相应的差)
  - 步骤 3 的一轮没有 BTC 的 `Balance` 条目
- 实现:`apps/reconciler/tests/baseline.rs::rcn_101_manual_sell_adopt`

### RCN-102 基础币扣费的买入:引擎净量与交易所余额一致

- 规则:[现货库存与余额基线](README.md#现货库存与余额基线)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-core 与假交易所;`acct-s` 买入 0.1 BTC,手续费 0.0001 BTC 从 BTC 扣
- 步骤:
  1. 成交落定后跑一轮
- 期望:
  - 没有 BTC 的 `Balance` 条目(引擎库存 0.0999 = 交易所 0.0999)
- 实现:`apps/reconciler/tests/baseline.rs::rcn_102_base_fee_buy`

### RCN-103 在途划转:到账前不报,到账后清除,其余差照常报

- 规则:[现货库存与余额基线](README.md#现货库存与余额基线)
- 级别:table
- 里程碑:M5
- 前置:合成 journal:`RecordTransfer{account = acct-s, from = okx-spot, to = binance-spot, asset = USDT, amount = 500, ext_ref = "wd-77"}` 已 Applied(引擎 `binance-spot` 的 USDT 已加 500);`binance-spot` 的交易所 USDT 按行变化;USDT `dust` = 0.01
- 实现:`apps/reconciler/tests/baseline.rs::rcn_103_transfer_in_transit`

| 行 | 交易所 USDT(相对引擎) | 期望 |
|---|---|---|
| RCN-103.1 | −500(未到账) | 无 `Balance`;轮次明细 `pending_in(acct-s, binance-spot, USDT) = 500`、`ext_ref = "wd-77"` |
| RCN-103.2 | 0(已到账) | 无 `Balance`;`pending_in` 清除 |
| RCN-103.3 | 行 2 之后再变为 −500 | 一条 `Balance{diff 对应 −500}`(pending 已清除) |
| RCN-103.4 | −520(未到账,另有 20 的差) | 一条 `Balance{diff 对应 −20}` |
| RCN-103.5 | 进程重启后首轮,交易所 0(早已到账) | 无 `Balance`;重建的 `pending_in` 被首轮清除 |

### RCN-104 在途划转登记超过 pending_in_max_s 未到账时不再扣除,差额报 Warn

- 规则:[现货库存与余额基线](README.md#现货库存与余额基线)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`pending_in_max_s = 86400`;合成 journal:`RecordTransfer{account = acct-s, from = okx-spot, to = binance-spot, asset = USDT, amount = 500, ext_ref = "wd-77"}` 已 Applied,其记录 UTC = T;`binance-spot` 交易所 USDT 比引擎少 500(未到账);USDT `dust` = 0.01;每行一轮,`S0_utc` 按行给出
- 实现:`apps/reconciler/tests/baseline.rs::rcn_104_transfer_pending_expiry`

| 行 | `S0_utc` | 期望 |
|---|---|---|
| RCN-104.1 | T + 86 399 s | 无 `Balance`;轮次明细 `pending_in = 500` |
| RCN-104.2 | T + 86 400 s | 无 `Balance`;轮次明细 `pending_in = 500` |
| RCN-104.3 | T + 86 401 s | 一条 `Balance{asset = USDT, severity = Warn}`,差额对应 −500;轮次明细中 `ext_ref = "wd-77"` 标为过期,`pending_in = 0` |
| RCN-104.4 | 行 3 之后交易所到账(差额 0),`S0_utc` = T + 90 000 s | 无 `Balance` |

### RCN-110 AUTO_EXTERNAL:用户流缺口里漏掉的 ADL 在轮 2 置位(testing AX-1 的 reconciler 一侧)

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:unit
- 里程碑:M5
- 前置:基础配置;`acct-p` 的 `BTCUSDT` 引擎仓位 −0.010;用户流在 seq `d` 断开、在 seq `u` 恢复 Up,期间交易所把仓位 ADL 到 −0.006,gw 无 `ExternalFill`;轮 1 `view_seq = V1 > u`,轮 2 `view_seq = V2`;两轮都无在途、无 Unknown/VERIFY 槽,两次视图点之间该 inst 无 `OrderDecision`/`OrderEvent`/`ExternalFill`
- 步骤:
  1. 跑轮 1
  2. 跑轮 2
- 期望:
  - 轮 1:一条 `Position{engine_qty = −0.010, exch_qty = −0.006, sflags = 0, severity = Warn}`
  - 轮 2:一条 `Position{engine_qty = −0.010, exch_qty = −0.006, sflags = AUTO_EXTERNAL, severity = Warn}`;两轮 `Summary.sflags` 都没有 `USERSTREAM_DOWN`
  - 轮 2 的轮次明细记置位依据:两轮的 `report_id`、数量、`V1`、`V2`、用户流最近一次 Up 的 seq `u`
- 实现:`apps/reconciler/tests/auto_external.rs::rcn_110_two_rounds_set_flag`

### RCN-111 AUTO_EXTERNAL 的每个条件逐一不满足时不置位

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:以 RCN-110 为基准,每行只改该行条件;"R3"指轮 3,与轮 2 条件相同时
- 实现:`apps/reconciler/tests/auto_external.rs::rcn_111_each_condition`

| 行 | 改动 | 期望 |
|---|---|---|
| RCN-111.1 | 交易所被全部 ADL 到 0 | 轮 2 `Position{exch_qty = 0, sflags = AUTO_EXTERNAL}` |
| RCN-111.2 | 交易所 −0.012(变大) | 两轮都不置位 |
| RCN-111.3 | 交易所 +0.002(变号) | 两轮都不置位 |
| RCN-111.4 | 轮 1 窗口内读到用户流 `ConnState{Down}`(轮 1 带 `USERSTREAM_DOWN`) | 轮 2 不置位;轮 3 置位 |
| RCN-111.5 | 轮 2 `S0` 时用户流不在 Up(轮 2 带 `USERSTREAM_DOWN`) | 轮 2 不置位 |
| RCN-111.6 | 用户流在 `V1` 与 `V2` 之间断开又恢复,最近一次 Up 的 seq 介于 `V1`、`V2` 之间,两轮窗口都没读到 Down | 轮 2 不置位;轮 3 置位 |
| RCN-111.7 | 最近一次 Up 的 seq = `V1` | 轮 2 不置位("早于"为严格) |
| RCN-111.8 | 最近一次 Up 的 seq = `V1 − 1` | 轮 2 置位 |
| RCN-111.9 | 本次运行中用户流从未 Up(最近一次 Up 的 seq 为 0) | 不置位 |
| RCN-111.10 | `V1` 与 `V2` 之间该 inst 有一条 `OrderEvent`(成交 0,只是 Ack) | 轮 2 不置位;轮 3 置位 |
| RCN-111.11 | 轮 1 该 inst 在途(条件 ②,无 `Position` 条目) | 轮 2 不置位(上一轮没有该 inst 的差异);轮 3 置位 |
| RCN-111.12 | 轮 2 `S0` 时同 (account, venue) 的另一 inst 有 `Unknown` 槽 | 轮 2 不置位 |
| RCN-111.13 | 轮 1 `S0` 时同 (account, venue) 有 `VERIFY` 槽 | 轮 2 不置位 |
| RCN-111.14 | 轮 1 置 `HISTORY_UNCHECKED` | 轮 2 不置位 |
| RCN-111.15 | 轮 2 置 `TRUNCATED`(条目 300 条,`Position` 未被截去) | 轮 2 不置位 |
| RCN-111.16 | 两轮之间 `AdoptExchange` 把引擎改为 −0.009 | 轮 2 不置位(`engine_qty` 不同) |
| RCN-111.17 | 轮 2 交易所变为 −0.005 | 轮 2 不置位;轮 3(仍 −0.005)置位 |
| RCN-111.18 | 轮 2 交易所恢复为 −0.010(无差),轮 3 又是 −0.006 | 轮 3 不置位(不相邻);轮 4 置位 |
| RCN-111.19 | 两轮之间有一次只做 Unknown 收敛的步骤(另一 inst 的槽,已在轮 2 `S0` 前收敛) | 轮 2 置位(该步骤不算一轮) |

### RCN-112 现货余额差从不置 AUTO_EXTERNAL

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:unit
- 里程碑:M5
- 前置:基础配置;`acct-s` 的 BTC 引擎 0.5、交易所 0.3,连续 3 轮不变,用户流一直 Up,无在途
- 步骤:
  1. 跑 3 轮
- 期望:
  - 3 轮的 `Balance{asset = BTC}` 条目 `sflags` 都为 0
- 实现:`apps/reconciler/tests/auto_external.rs::rcn_112_spot_never_flagged`

### RCN-113 进程重启后重新计数

- 规则:[ReconcileReport](README.md#reconcilereport)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:RCN-110 的状态;轮 1 后 reconciler 进程重启
- 步骤:
  1. 重启后跑轮 2、轮 3
- 期望:
  - 重启后轮 1(轮 2)的 `Position` 不带 `AUTO_EXTERNAL`;轮 3 带
- 实现:`apps/reconciler/tests/auto_external.rs::rcn_113_restart_resets`

### RCN-114 置位的组进入 journal 后,视图含合成的外来成交,再下一轮该项无差异

- 规则:[ReconcileReport](README.md#reconcilereport)、[对账周期与比对基准](README.md#对账周期与比对基准)
- 级别:integ
- 里程碑:M5
- 前置:RCN-110 的场景以进程内 hft-core(真实 sequencer、hft-ledger、hft-risk)运行;`FakeCtl` 换成真实 ctl-gateway
- 步骤:
  1. 跑轮 1、轮 2
  2. 跑轮 3
- 期望:
  - 轮 2 的组生效后,journal 中同一事件内有带 `AUTO_RECON` 的 `Note{ExternalFill}`(`qty = 0.004`、`side = Buy`);reconciler 视图中 `BTCUSDT` 仓位 = −0.006
  - 轮 3 没有 `Position` 条目
- 实现:`apps/reconciler/tests/auto_external.rs::rcn_114_view_after_auto_external`

### RCN-115 交易所之后补推同一笔 ADL:视图经 auto_held 抵扣,无新差异

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:integ
- 里程碑:M5
- 前置:RCN-114 之后;用户流补推那笔真实 ADL 成交 `ExternalFill{cause = Adl, qty = 0.004}`(Input)
- 步骤:
  1. 补推落定后跑一轮
- 期望:
  - 视图中 `BTCUSDT` 仓位仍为 −0.006(`auto_held` 抵扣 0.004,入账 0);journal 中有 `Note{LedgerAutoHeldOffset}`;这一轮没有 `Position` 条目
- 实现:`apps/reconciler/tests/auto_external.rs::rcn_115_late_push_offset`

### RCN-116 AUTO_EXTERNAL 判定与参考模型一致

- 规则:[ReconcileReport](README.md#reconcilereport)、[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M5
- 性质:对任意两轮相邻输入,差异计算纯函数在当前轮对某 inst 置 `AUTO_EXTERNAL`,当且仅当参考模型中五个条件同时成立;置位的条目 `severity = Warn`;从不对 `Balance`、`OpenOrder`、`Funding`、`UnknownResolved` 置位
- 生成:上一轮与当前轮的 (engine_qty, exch_qty) 各在 {−3, −2, −1, 0, 1, 2, 3} lot 中取值;两轮视图点之间的事件 {无, OrderDecision, OrderEvent, ExternalFill} 各 25%;在途、Unknown/VERIFY、`HISTORY_UNCHECKED`、`TRUNCATED`、`USERSTREAM_DOWN` 各轮独立以 10% 置真;用户流最近一次 Up 的 seq 取 {0, 上一轮 view_seq − 1, 上一轮 view_seq, 两轮之间}
- 参考:`apps/reconciler/tests/model.rs` 中按 README 五条条件逐条写的判定函数
- 例数:PR 5 000;每夜 200 000
- 实现:`apps/reconciler/tests/prop_diff.rs::rcn_116_auto_external_model`

### RCN-130 EpochStart 之后每个 (Live 账户, venue) 一组报告在 5 s 内进入 journal

- 规则:[启动闸门与严格放行](README.md#启动闸门与严格放行)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-core 冷启动,3 个 Live 账户各 1 个 venue;`FakeRest` 每个请求延迟 50 ms;reconciler 在 hft-core 启动前已运行
- 步骤:
  1. hft-core 写出 `Note{EpochStart}`
- 期望:
  - 3 组报告都在 `EpochStart` 记录的 UTC 之后 5 000 ms 内进入 journal(以各组末条记录的 UTC 计);hft-risk 收到 3 组 `ReconGateInput`
- 实现:`apps/reconciler/tests/gate.rs::rcn_130_epoch_start_within_5s`

### RCN-131 严格闸门:有一条 Warn 的仓位差时 RECONCILED 不置位,Adopt 后再一轮全部 Ignore 才置位

- 规则:[启动闸门与严格放行](README.md#启动闸门与严格放行)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-core 冷启动;测试 refdata 中 `BTCUSDT` 的 lot 为 0.0001 BTC;`acct-p` 的引擎与交易所仓位差 0.00013 BTC(名义约 $8)
- 步骤:
  1. `EpochStart` 后的首轮
  2. 以首轮的 `(report_id, idx)` 发 `AdoptExchange`(Applied)
  3. 随即触发的一轮
- 期望:
  - 步骤 1 后 `acct-p` 的 `gate_bits` 中 `RECONCILED = 0`
  - 步骤 3 的组全部 `Ignore`(无差异条目),之后 `RECONCILED = 1`
- 实现:`apps/reconciler/tests/gate.rs::rcn_131_strict_gate_adopt`

### RCN-132 孤儿单:CancelAll 后下一轮无差异,RECONCILED 置位

- 规则:[启动闸门与严格放行](README.md#启动闸门与严格放行)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-core;假交易所上 `acct-p` 有一张引擎不知道的挂单
- 步骤:
  1. 首轮
  2. 发 `CancelAll{acct-p, binance-usdm}`(Applied,假交易所撤掉该单)
  3. 随即触发的一轮
- 期望:
  - 步骤 1 的组有 `OpenOrder{slot = 0}`,`RECONCILED = 0`
  - 步骤 3 的组无差异条目,`RECONCILED = 1`
- 实现:`apps/reconciler/tests/gate.rs::rcn_132_orphan_cancel_all`

### RCN-133 只有 Funding 条目的组不阻止 RECONCILED、不触发 Recon cap

- 规则:[启动闸门与严格放行](README.md#启动闸门与严格放行)、[资金费比对](README.md#资金费比对)
- 级别:integ
- 里程碑:M5
- 前置:RCN-080 的状态以进程内 hft-core 运行(冷启动后首轮)
- 步骤:
  1. 首轮
- 期望:
  - 组内只有 `Summary` 与 `Funding` 条目;处理后 `RECONCILED = 1`;没有 `Note{CapSet}`(cap 为 `Recon`)
- 实现:`apps/reconciler/tests/gate.rs::rcn_133_funding_only_not_blocking`

### RCN-134 reconciler 从不发恢复或升级命令

- 规则:[启动闸门与严格放行](README.md#启动闸门与严格放行)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-core;`acct-p` 的同一仓位差连续多轮,hft-risk 置 `Recon` cap;之后差异消失并连续 5 轮全部 Ignore
- 步骤:
  1. 运行 10 轮
- 期望:
  - ctl-gateway 审计中 issuer `reconciler` 的请求全部是 `ReconcileReport`(`cmd_kind = 0`);journal 中没有 issuer 为 `reconciler` 的 `ControlCmd`;`Recon` cap 仍在(等待人工 `Resume`)
- 实现:`apps/reconciler/tests/gate.rs::rcn_134_no_commands_issued`

### RCN-135 带在途的轮次不计入恢复所需的两轮 Ignore

- 规则:[启动闸门与严格放行](README.md#启动闸门与严格放行)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-core;`acct-p` 已有 `Recon` cap;此后连续三轮的情况为:全部 Ignore 且 `skipped_inflight = 0`、全部 Ignore 但 `skipped_inflight = 2`、全部 Ignore 且 `skipped_inflight = 0`
- 步骤:
  1. 三轮之后发 `Resume{Recon, acct-p, binance-usdm}`
- 期望:
  - 轮 2 的 `Summary.skipped_inflight = 2`;`Resume` 被接受(第一、三轮构成连续两轮 Ignore,带在途的轮 2 不计入也不清零)
  - 对照:中间那一轮换成带 `USERSTREAM_DOWN` 的轮次时,`Resume` 返回 `ReconNotClean`
- 实现:`apps/reconciler/tests/gate.rs::rcn_135_inflight_rounds_not_counted`

### RCN-140 只取 Unknown 且年龄超过 unknown_min_age_ms 的槽

- 规则:[Unknown 订单收敛](README.md#unknown-订单收敛)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`unknown_min_age_ms = 2000`;`acct-o` 的槽 s7 进入 `Unknown` 的记录 UTC 为 `t0`;这一轮开始的本机 UTC 按行给出
- 实现:`apps/reconciler/tests/unknown.rs::rcn_140_min_age`

| 行 | 这一轮开始时刻 | 期望 |
|---|---|---|
| RCN-140.1 | `t0 + 1 999 ms` | 不查 s7;无 s7 的 `UnknownResolved` |
| RCN-140.2 | `t0 + 2 000 ms` | 不查 s7(年龄须 > 2 000 ms) |
| RCN-140.3 | `t0 + 2 001 ms` | 查 s7 |
| RCN-140.4 | `t0 + 5 000 ms`,s7 已不是 `Unknown`(已收敛为 `Filled`) | 不查 s7 |

### RCN-141 按 client id 查到的结果给出 UnknownResolved 的状态码与累计成交

- 规则:[Unknown 订单收敛](README.md#unknown-订单收敛)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-o` 的 `Unknown` 槽 s7(epoch 4, gen 3),年龄 5 s;`FakeRest` 按 client id 查单的返回按行给出(交易所时间 `tx`)
- 实现:`apps/reconciler/tests/unknown.rs::rcn_141_query_status_codes`

| 行 | 查单结果 | 期望条目 |
|---|---|---|
| RCN-141.1 | `live`,累计成交 0 | `UnknownResolved{slot = s7, gen = 3, epoch = 4, order_status = New(1), exch_qty = 0, exch_ts_ns = tx}` |
| RCN-141.2 | `partially_filled`,累计 0.01 | `order_status = PartiallyFilled(2)`、`exch_qty = 0.01` |
| RCN-141.3 | `filled`,累计 0.02 | `order_status = Filled(3)`、`exch_qty = 0.02` |
| RCN-141.4 | `canceled`,累计 0 | `order_status = Cancelled(4)` |
| RCN-141.5 | `rejected` | `order_status = Rejected(5)` |
| RCN-141.6 | `expired` | `order_status = Expired(6)` |

### RCN-142 查到已成交的 UnknownResolved 让 OMS 把槽迁到 Filled

- 规则:[Unknown 订单收敛](README.md#unknown-订单收敛)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-core;`acct-o` 的槽 s7 因下单超时进入 `Unknown`;假交易所上该单已全部成交 0.02 BTC
- 步骤:
  1. 跑一轮(该轮报告组经真实 ctl-gateway 进入 sequencer)
- 期望:
  - 报告组含 `UnknownResolved{slot = s7, order_status = Filled, exch_qty = 0.02}`;sequencer 处理后 s7 为 `Filled`;同组 `UnknownResolved` 之后成交历史中该单的成交进入下一轮的成交比对且无缺失
- 实现:`apps/reconciler/tests/unknown.rs::rcn_142_resolved_to_filled`

### RCN-143 成交可按 client id 过滤的所:查不到订单时按成交历史给出结果

- 规则:[Unknown 订单收敛](README.md#unknown-订单收敛)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`history_lookback_s = 60`;`acct-o`(`okx-swap`)的 `Unknown` 槽 s7,决策时刻 `td`;查单返回"订单不存在";成交历史请求的结果按行给出
- 实现:`apps/reconciler/tests/unknown.rs::rcn_143_client_id_history`

| 行 | 成交历史 | 期望 |
|---|---|---|
| RCN-143.1 | 窗口内有 s7 的两笔成交,合计 0.02(等于下单量) | `UnknownResolved{order_status = Filled, exch_qty = 0.02}` |
| RCN-143.2 | 窗口内有 s7 的一笔成交 0.01(下单量 0.02) | `order_status = PartiallyFilled`、`exch_qty = 0.01` |
| RCN-143.3 | 窗口内没有 s7 的成交 | `order_status = NotFound(7)`、`exch_qty = 0`、`history_checked = 1` |
| RCN-143.4 | 成交历史请求返回 HTTP 500 | `order_status = NotFound`、`history_checked = 0` |
| RCN-143.5 | 成交历史请求的时间窗 | 请求参数的起点 = `td − 60 s`,终点 = 请求时刻 |

### RCN-144 成交不带 client id 的所:窗口内有未归属成交时全部 NotFound 为 0 并置 HISTORY_UNCHECKED

- 规则:[Unknown 订单收敛](README.md#unknown-订单收敛)、[ReconcileReport](README.md#reconcilereport)
- 级别:unit
- 里程碑:M5
- 前置:基础配置;`acct-p`(`binance-usdm`)有两个 `Unknown` 槽 s5、s6,查单都返回 `-2013`;`BTCUSDT` 成交历史窗口内有一笔交易所订单 id `O9` 的成交,`O9` 不属于引擎已知的任何槽、墓碑或已入账的 `ExternalFill`
- 步骤:
  1. 跑一轮
- 期望:
  - 两条 `UnknownResolved{order_status = NotFound, history_checked = 0}`(s5、s6);`Summary.sflags` 含 `HISTORY_UNCHECKED`
  - 轮次明细含未归属成交 `{exch_order_id = O9, trade_id, 时间, qty}`
- 实现:`apps/reconciler/tests/unknown.rs::rcn_144_unattributed_fill`

### RCN-145 已知墓碑、已入账外来成交与其他槽的成交不算未归属

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:同 RCN-144,但窗口内那笔成交的交易所订单 id 按行属于
- 实现:`apps/reconciler/tests/unknown.rs::rcn_145_attributed_fills`

| 行 | 成交的归属 | 期望 |
|---|---|---|
| RCN-145.1 | 引擎中另一个 `Live` 槽 s8 | 不置 `HISTORY_UNCHECKED` |
| RCN-145.2 | 墓碑期内的旧代槽 | 不置 |
| RCN-145.3 | 已入账的 `ExternalFill`(同一 `exch_order_id`) | 不置 |
| RCN-145.4 | 都不属于 | 置 `HISTORY_UNCHECKED` |

### RCN-146 HISTORY_UNCHECKED 只在见到未归属成交时置位

- 规则:[ReconcileReport](README.md#reconcilereport)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-p` 的 `Unknown` 槽 s5;每行一种情形,窗口内没有未归属成交
- 实现:`apps/reconciler/tests/unknown.rs::rcn_146_history_unchecked_only_cause`

| 行 | 情形 | 期望 `Summary.sflags` 的 `HISTORY_UNCHECKED` |
|---|---|---|
| RCN-146.1 | 成交历史请求失败 | 不置(`history_checked = 0`) |
| RCN-146.2 | 只查到一个轮次的 `-2013` | 不置 |
| RCN-146.3 | 查单请求 429(无条目) | 不置 |
| RCN-146.4 | 没有 `Unknown` 槽 | 不置 |

### RCN-147 成交不带 client id 的所:两个不同轮次且晚于宽限期的"不存在"才给 history_checked = 1

- 规则:[Unknown 订单收敛](README.md#unknown-订单收敛)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`notfound_grace_ms = 10000`、`notfound_confirm_rounds = 2`;`acct-p` 的 `Unknown` 槽 s5,决策时刻 `td`;窗口内无未归属成交;每次查单都返回 `-2013`,查询时刻按行给出
- 实现:`apps/reconciler/tests/unknown.rs::rcn_147_notfound_confirm`

| 行 | 查询序列(轮次:查询时刻) | 期望:最后一轮的 `history_checked` |
|---|---|---|
| RCN-147.1 | 轮 1:`td + 12 s` | 0 |
| RCN-147.2 | 轮 1:`td + 12 s`;轮 2:`td + 42 s` | 1 |
| RCN-147.3 | 轮 1:`td + 3 s`;轮 2:`td + 8 s` | 0(都早于宽限期) |
| RCN-147.4 | 轮 1:`td + 3 s`;轮 2:`td + 33 s`;轮 3:`td + 63 s` | 轮 2 0,轮 3 1 |
| RCN-147.5 | 轮 1:`td + 10 000 ms − 1 ms`;轮 2:`td + 40 s`;轮 3:`td + 70 s` | 轮 2 0(第 1 次不计入),轮 3 1 |
| RCN-147.6 | 轮 1:`td + 10 000 ms + 1 ms`;轮 2:`td + 40 s` | 轮 2 1 |
| RCN-147.7 | 轮 1:`td + 12 s`;Unknown 收敛步(`OmsNeedsRestQuery`):`td + 15 s` | 1(两个不同时机都计入) |
| RCN-147.8 | 同一轮内对 s5 重试两次查单都 `-2013`(`td + 12 s`、`td + 12.5 s`) | 0(同一轮只算一次) |
| RCN-147.9 | `notfound_confirm_rounds = 1`;轮 1:`td + 12 s` | 1 |
| RCN-147.10 | 轮 1:`td + 10 000 ms`(恰为宽限期);轮 2:`td + 40 s` | 轮 2 0("晚于"为严格) |

### RCN-148 查询本身失败时不发条目,下一轮重试,连续失败达阈值时 status.json 报 P1

- 规则:[Unknown 订单收敛](README.md#unknown-订单收敛)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`unknown_alert_rounds = 3`;`acct-o` 的 `Unknown` 槽 s7(年龄 > 2 s);查单请求的结果按行逐轮给出
- 实现:`apps/reconciler/tests/unknown.rs::rcn_148_query_failure`

| 行 | 轮 1–3 | 期望 |
|---|---|---|
| RCN-148.1 | 超时、超时、超时(`http.timeout_ms = 5000`) | 3 轮都没有 s7 的 `UnknownResolved`;轮 3 后 `status.json` 中 `acct-o` 项 `unknown_query_fail_rounds = 3`、`alert = "P1"` |
| RCN-148.2 | 429、503、成功(`live`) | 轮 3 有 `UnknownResolved{order_status = New}`;计数清 0、`alert = null` |
| RCN-148.3 | 429、429、(轮 3 前 s7 已收敛) | 轮 2 后 `unknown_query_fail_rounds = 2`、`alert = null` |
| RCN-148.4 | 503 × 2 | 每轮都向 `FakeRest` 重新发出查单(轮 2 有请求记录) |

### RCN-149 status.json 的格式与原子写

- 规则:[Unknown 订单收敛](README.md#unknown-订单收敛)
- 级别:unit
- 里程碑:M5
- 前置:基础配置,`accounts = [acct-p, acct-o]`;`acct-o` 查单连续失败 3 轮;测试钩子在轮 3 写 `status.json` 的 `.tmp` 之后、改名之前暂停并读取目录
- 步骤:
  1. 运行 3 轮,在暂停点读取 `status.json`,再继续
- 期望:
  - 暂停点读到的 `status.json` 是轮 2 后的完整旧内容;继续后为新内容:两项 `{account, venue, unknown_query_fail_rounds, funding_query_fail_rounds, alert}`,`acct-o` 项 `alert = "P1"`、`acct-p` 项全 0 与 `null`
- 实现:`apps/reconciler/tests/unknown.rs::rcn_149_status_file`

### RCN-160 key 有交易或提现权限时启动失败退出 2

- 规则:[启动核验(只读 key)](README.md#启动核验只读-key)、[依赖](README.md#依赖)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`FakeRest` 的 API key 权限查询返回按行给出
- 实现:`apps/reconciler/tests/identity.rs::rcn_160_key_permissions`

| 行 | 权限查询结果 | 期望 |
|---|---|---|
| RCN-160.1 | 只读 | 继续 UID 核验 |
| RCN-160.2 | 含交易权限 | 退出 2;没有任何对账请求 |
| RCN-160.3 | 含提现权限 | 退出 2 |
| RCN-160.4 | 交易与提现都有 | 退出 2 |

### RCN-161 UID 与 venue_uid 不符时写 fatal.json 并退出 2,不发任何对账请求

- 规则:[启动核验(只读 key)](README.md#启动核验只读-key)
- 级别:unit
- 里程碑:M5
- 前置:基础配置,`accounts = [acct-p, acct-o]`;`acct-o` 的账户信息接口返回 UID `880012349999`(末 4 位 `9999`,refdata `venue_uid` 末 4 位 `1002`)
- 步骤:
  1. `reconciler run`
- 期望:
  - 退出 2;`paths.state/fatal.json` = `{code: "UidMismatch", account: "acct-o", venue: "okx-swap", utc_ns: …}`,其中 UID 只出现末 4 位 `9999`(文件中没有完整 UID)
  - `FakeRest` 没有收到任何挂单、成交、仓位、余额、资金费或查单请求;`FakeCtl` 没有连接
- 实现:`apps/reconciler/tests/identity.rs::rcn_161_uid_mismatch`

### RCN-162 UID 查询持续失败超过 startup_verify_s 时写 IdentityUnverified 并退出 1

- 规则:[启动核验(只读 key)](README.md#启动核验只读-key)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`startup_verify_s = 60`;`acct-p` 的账户信息接口按行返回;假时钟
- 实现:`apps/reconciler/tests/identity.rs::rcn_162_identity_timeout`

| 行 | 账户信息接口 | 期望 |
|---|---|---|
| RCN-162.1 | 一直超时 | 在启动后 60 s 时退出 1;`fatal.json{code = "IdentityUnverified", account = acct-p, venue = binance-usdm}` |
| RCN-162.2 | 前 59 s 返回 503,之后返回与 `venue_uid` 相同的 UID | 核验通过,开始对账;没有 `fatal.json` |
| RCN-162.3 | 一直返回 429 | 期间持续重试(60 s 内账户信息请求 ≥ 2 次),60 s 时退出 1;`fatal.json{code = "IdentityUnverified"}` |

### RCN-163 核验通过后删除旧的 fatal.json

- 规则:[启动核验(只读 key)](README.md#启动核验只读-key)
- 级别:unit
- 里程碑:M5
- 前置:`paths.state/fatal.json` 是上次运行留下的 `{code: "UidMismatch", …}`;本次 UID 全部一致
- 步骤:
  1. `reconciler run`,等到轮 1 开始
- 期望:
  - 轮 1 开始之前 `fatal.json` 已不存在
- 实现:`apps/reconciler/tests/identity.rs::rcn_163_fatal_removed_after_pass`

### RCN-164 先核验全部 (Live 账户, venue),再发任何对账请求

- 规则:[启动核验(只读 key)](README.md#启动核验只读-key)
- 级别:unit
- 里程碑:M5
- 前置:基础配置,四个 Live 账户;`FakeRest` 记录请求顺序
- 步骤:
  1. `reconciler run`
- 期望:
  - 请求序列中,4 个权限查询与 4 个账户信息查询全部排在第一个对账请求(挂单、成交、仓位、余额、资金费、查单)之前
- 实现:`apps/reconciler/tests/identity.rs::rcn_164_verify_before_any_round`

### RCN-165 UID 解码与 hft-core 的 IDENTITY 查询用同一纯函数

- 规则:[启动核验(只读 key)](README.md#启动核验只读-key)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/venue/binance-usdm/account-info.json`、`fixtures/venue/okx-swap/account-config.json`、`fixtures/venue/bitget-usdt-futures/account-info.json`(各所账户信息接口的真实响应,脱敏)
- 期望文件:`fixtures/reconciler/RCN-165/uids.json`(每个输入解出的 UID;与 hft-core `IDENTITY` 金样中同一输入的 UID 相同,两边都调用对应 `venue-*` 的账户查询纯函数)
- 实现:`apps/reconciler/tests/identity.rs::rcn_165_uid_decode_same_function`

### RCN-166 run 与 once 都在任何对账请求之前核验;新增账户需重启后核验

- 规则:[启动核验(只读 key)](README.md#启动核验只读-key)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:基础配置
- 实现:`apps/reconciler/tests/identity.rs::rcn_166_verify_on_run_and_once`

| 行 | 操作 | 期望 |
|---|---|---|
| RCN-166.1 | `once --account acct-p` | 权限与 UID 查询(只针对 `acct-p` 的 venue)先于对账请求 |
| RCN-166.2 | `run` 运行中把 `acct-o` 加进配置文件(不重启) | 不对 `acct-o` 发任何请求 |
| RCN-166.3 | 行 2 之后重启 | `acct-o` 的权限与 UID 查询先于它的第一个对账请求 |

### RCN-167 加载校验与启动核验用 refdata_blob 文件,不用 journal 中更新的 refdata

- 规则:[配置文件字段](README.md#配置文件字段)、[启动核验(只读 key)](README.md#启动核验只读-key)
- 级别:unit
- 里程碑:M5
- 前置:基础配置,`accounts = [acct-p]`;`refdata_blob` 文件中 `acct-p` 的 `venue_uid` 末 4 位 `1001`;journal 末尾的 refdata `ConfigBlob`(`ReplaceRefdata` 之后)把它改为末 4 位 `7777`;账户信息接口返回末 4 位 `1001` 的 UID
- 步骤:
  1. `reconciler run`
- 期望:
  - 启动核验通过(按文件中的 `1001` 比较),不写 `fatal.json`;随后照常提交报告组
- 实现:`apps/reconciler/tests/identity.rs::rcn_167_verify_uses_refdata_file`

### RCN-170 reconciler 不轮询保证金数据

- 规则:[与 gw 的分工](README.md#与-gw-的分工)
- 级别:unit
- 里程碑:M5
- 前置:基础配置,四个 Live 账户;运行 10 轮;`FakeRest` 记录全部请求路径
- 步骤:
  1. 运行
- 期望:
  - 全部请求路径都属于[覆盖范围与数据来源](README.md#覆盖范围与数据来源)表列出的六类端点(挂单、成交、仓位、余额、资金费流水、按 client id 查单)或启动核验的权限查询与账户信息接口;启动核验之后不再出现权限与账户信息请求;没有保证金率、强平价、ADL 档或杠杆分档的请求
- 实现:`apps/reconciler/tests/coverage.rs::rcn_170_no_margin_polling`

### RCN-171 两腿是否对平不在 reconciler 判级

- 规则:[与 gw 的分工](README.md#与-gw-的分工)
- 级别:unit
- 里程碑:M5
- 前置:合成 journal:group g1 两腿(`acct-s` 的 BTC 现货 +0.5、`acct-p` 的 `BTCUSDT` 永续 −0.4,LegTracker 显示裸量 0.1),两腿各自与交易所一致
- 步骤:
  1. 对两个 (account, venue) 各跑一轮
- 期望:
  - 两个报告组都没有差异条目,`Summary.severity = Ignore`
- 实现:`apps/reconciler/tests/coverage.rs::rcn_171_no_cross_leg_grading`

### RCN-180 每所独立令牌桶限速,并按响应中的限速余量闭环降速

- 规则:[实现说明](README.md#实现说明)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:基础配置,`http.max_rps = {binance-usdm = 5, okx-swap = 2}`;一轮需要 `binance-usdm` 20 个请求、`okx-swap` 10 个请求
- 实现:`apps/reconciler/tests/rest.rs::rcn_180_rate_limit`

| 行 | 条件 | 期望 |
|---|---|---|
| RCN-180.1 | 两所同时跑一轮 | 任意 1 s 窗口内 `binance-usdm` 请求 ≤ 5、`okx-swap` ≤ 2;两所互不等待 |
| RCN-180.2 | `binance-usdm` 响应头 `X-MBX-USED-WEIGHT-1M` 报告的已用权重达到该所 1 min 限额的 20% | 该 1 min 计数窗口结束前不再向 `binance-usdm` 发请求;`okx-swap` 照常 |
| RCN-180.3 | 单个请求超过 `http.timeout_ms = 5000` | 该请求在 5 000 ms 时以超时失败 |

### RCN-181 成交游标持久化:重启后从游标继续,游标丢失时回退 24 h 并去重

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-p`;`FakeRest` 的成交接口按 `fromId` 分页
- 实现:`apps/reconciler/tests/rest.rs::rcn_181_trade_cursor`

| 行 | 条件 | 期望 |
|---|---|---|
| RCN-181.1 | 运行 2 轮后重启,再跑 1 轮 | 重启后首个成交请求从持久化的游标之后继续(`fromId` = 游标 + 1),没有重复请求已比对过的成交 |
| RCN-181.2 | 删除游标文件后启动 | 首个成交请求覆盖 `S0` 前 24 h;同一成交 id 在多页中出现时只计一次;游标重新写出 |

### RCN-182 Binance 按 symbol 的成交查询只覆盖近 24 h 有订单或持仓的 symbol,每 6 h 全量扫一次

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:基础配置;`acct-s`(`binance-spot`)有余额的资产对应 symbol:`BTCUSDT`、`BNBUSDT`、`ETHUSDT`;`S0` 前 24 h 内只有 `BTCUSDT` 有订单;假时钟
- 实现:`apps/reconciler/tests/rest.rs::rcn_182_binance_symbol_scan`

| 行 | 时刻 | 期望的成交查询 symbol |
|---|---|---|
| RCN-182.1 | 启动后轮 1 | {`BTCUSDT`, `BNBUSDT`, `ETHUSDT`}(启动即做一次全量) |
| RCN-182.2 | 轮 1 后 30 s | {`BTCUSDT`} |
| RCN-182.3 | 轮 1 后 6 h | {`BTCUSDT`, `BNBUSDT`, `ETHUSDT`} |

### RCN-183 journal 尾随与视图在单写者线程上,轮次任务只读视图副本

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:基础配置,四个 Live 账户并发跑轮次;journal 以每秒 10 000 条持续增长;测试构建记录视图写操作所在的线程 id
- 步骤:
  1. 运行 60 s
- 期望:
  - 视图的全部写操作在同一线程;每轮读到的视图副本在该轮内不变(同一轮两次读取同一 inst 的值相同)
- 实现:`apps/reconciler/tests/view.rs::rcn_183_single_writer_view`

### RCN-184 依赖闭包中没有下单路径与热进程组件

- 规则:[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M5
- 前置:`cargo metadata` 取 `apps/reconciler` 的依赖图(含启用的 feature)
- 步骤:
  1. 检查依赖图与 `cargo xtask deps` 的结果
- 期望:
  - 闭包中没有 `hft-venue`、`hft-net`、`hft-sequencer`;`hft-journal` 未启用 `writer` feature;`hft-oms`、`hft-risk` 只以类型被引用(`cargo xtask deps` 的白名单检查退出 0)
  - 对 `venue-*` 的引用只限其只读查询与账户查询模块(白名单中没有下单、撤单、改单的编码函数)
- 实现:`apps/reconciler/tests/build.rs::rcn_184_dependency_closure`

### RCN-190 测试网/模拟盘:每个 core venue 的只读端点在测试网上可用并被正确解码

- 规则:[覆盖范围与数据来源](README.md#覆盖范围与数据来源)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 联调窗口;`prod` 角色测试网实例;每个 core venue 一个测试网/模拟盘账户及其只读 key;`rest_base` 指向各测试网
- 授权:无
- 步骤:
  1. 每个账户在测试网下一笔小额订单并成交、挂一张远离对手价的单
  2. `reconciler once --account <每个账户> --dry-run`
- 通过标准:每个 (account, venue) 的输出中挂单、成交、仓位(永续)、余额与交易所网页一致;各端点没有返回解码错误;测试网不提供的端点(如资金费流水)在证据中注明
- 证据:`ops-log/<date>-RCN-190.md`(每个端点的请求路径、响应样本、解码结果;核实结果另写进对应 `venue-*` README 的开发备忘)
- 实现:`apps/reconciler/tests/host/RCN-190.sh`

### RCN-191 测试网/模拟盘:API key 权限查询接口能识别交易权限

- 规则:[启动核验(只读 key)](README.md#启动核验只读-key)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 联调窗口;每个 core venue 一把只读测试网 key 与一把交易测试网 key
- 授权:无
- 步骤:
  1. 以只读 key 运行 `reconciler --check` 与 `once --dry-run`
  2. 把配置中某 venue 的 key 换成交易 key(秘密名仍以 `-readonly` 结尾,模拟误配)后运行 `once --dry-run`
- 通过标准:步骤 1 退出 0;步骤 2 退出 2 且 stderr 指明权限不符;没有权限查询接口的所在证据中注明,并在上线流程中人工核对
- 证据:`ops-log/<date>-RCN-191.md`
- 实现:`apps/reconciler/tests/host/RCN-191.sh`

### RCN-192 测试网/模拟盘:只读 key 的 UID 与 venue_uid 一致,误配时退出 2

- 规则:[启动核验(只读 key)](README.md#启动核验只读-key)
- 级别:testnet
- 环境:dev2 联调窗口;测试网账户的 refdata `venue_uid` 已登记
- 授权:无
- 里程碑:M6
- 步骤:
  1. 正常配置启动 `hft-reconciler`
  2. 把两个账户的 key 对调后启动
- 通过标准:步骤 1 核验通过、开始对账;步骤 2 退出 2,`fatal.json{code = "UidMismatch"}`,watchtower `ProcessDown` 报 P0
- 证据:`ops-log/<date>-RCN-192.md`
- 实现:`apps/reconciler/tests/host/RCN-192.sh`

### RCN-193 测试网/模拟盘连续对账零误报(门槛 ⑥)

- 规则:[概要](README.md#概要)、[比对项](README.md#比对项)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 联调窗口,门槛 ⑥ 的联调期;每个测试网账户 × 每个 core venue;测试策略与 latency-probe 正常交易
- 授权:无
- 步骤:
  1. `hft-reconciler` 随测试网实例连续运行整个联调期
  2. 对每条 `Warn` 条目人工核对交易所网页与 journal
- 通过标准:每条 `Warn` 都有真实差异佐证(零误报);挂单、成交、仓位、余额(以及该所测试网提供的资金费流水)每个周期都有报告组
- 证据:`ops-log/<date>-RCN-193.md`(每条 `Warn` 的核对记录)
- 实现:`apps/reconciler/tests/host/RCN-193.sh`

### RCN-194 测试网:人为制造的 1 lot 差异在 2 轮内被 hft-risk 升为 cap

- 规则:[启动闸门与严格放行](README.md#启动闸门与严格放行)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 联调窗口;测试网账户 `tn-a`(永续);`recon_closeonly_notional` 设为不大于 1 lot 名义
- 授权:无
- 步骤:
  1. 在测试网网页上手工买入 1 lot(引擎不知道)
  2. 观察随后的报告组与 journal
- 通过标准:轮 1、轮 2 各一条 `Position` `Warn`;轮 2 之后 journal 中有 `Note{CapSet}`(cap 为 `Recon`);`AUTO_EXTERNAL` 未置位(仓位变大)
- 证据:`ops-log/<date>-RCN-194.md`
- 实现:`apps/reconciler/tests/host/RCN-194.sh`

### RCN-195 测试网:Binance 下单超时后"订单不存在"可信时长的实测

- 规则:[Unknown 订单收敛](README.md#unknown-订单收敛)、[配置文件字段](README.md#配置文件字段)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 联调窗口;Binance USDⓈ-M 与现货测试网账户;故障代理在下单请求发出后丢弃响应(制造 `-1007` 一类的未知状态)
- 授权:无
- 步骤:
  1. 重复 50 次:下单 → 丢弃响应 → 每 1 s 按 client id 查单并查成交,持续 60 s
- 通过标准:记录每次"订单不存在"变为可信(其后不再出现该单与成交)的时长分布;`notfound_grace_ms` 的默认值 ≥ 该分布的最大值,否则按实测改默认值并同步本文件 RCN-001
- 证据:`ops-log/<date>-RCN-195.md`(分布与结论;结论写进 venue-binance 开发备忘)
- 实现:`apps/reconciler/tests/host/RCN-195.sh`

### RCN-196 一轮 REST 往返时延

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M8
- 环境:升级主机(与交易所同 AZ),正式实例的只读 key
- 授权:无
- 步骤:
  1. 连续 24 h 记录每轮(单 account × 单 venue)从第一个请求发出到最后一个响应到达的时长
- 通过标准:每所 p99 不超过 [延迟与资源契约](README.md#延迟与资源契约) 表中该所的值
- 证据:`ops-log/<date>-RCN-196.md`
- 实现:`apps/reconciler/tests/host/RCN-196.sh`

### RCN-197 REST 权重占用与 CPU、内存

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M6
- 环境:dev2 联调窗口(测试网实例)与升级主机(M8 复核)
- 授权:无
- 步骤:
  1. 连续 24 h 记录各所响应头的已用权重、`hft-reconciler` 单元的 CPU 时间与 `memory.peak`
- 通过标准:任一分钟内各所已用权重 ≤ 该所 IP/账户限额的 20%;CPU ≤ 0.5 核(24 h 平均与任一 1 min 均值);`memory.peak` ≤ 该主机类的内存上限
- 证据:`ops-log/<date>-RCN-197.md`
- 实现:`apps/reconciler/tests/host/RCN-197.sh`

## 验收

门槛编号的唯一定义在 [testing §8](../../docs/testing.md#8-上线门槛)。

1. **用例**:里程碑 ≤ 当前里程碑的全部 `unit`/`table`/`prop`/`golden`/`integ`/`proc` 用例有同编号的测试函数,`cargo xtask test-trace` 三张清单为空([docs/tests §6](../../docs/tests/README.md#6-与代码的对应))。
2. **属性测试**:RCN-040、RCN-098、RCN-116 的 PR 例数每个 PR 运行,每夜例数每夜运行,零失败(失败的最小化反例进回归集)。
3. **门槛 ⑥**(M6 联调期):RCN-190–RCN-195 在 dev2 联调窗口内各有证据;RCN-193 零误报(每条 `Warn` 有真实差异佐证);RCN-194 中人为制造的 1 lot 差异在 2 轮内被 hft-risk 升为 cap。
4. **资源**:RCN-197 在 dev2 联调期与升级主机上各有一份证据。
5. **正式实盘首 7 天**:按第 3 条的同一口径复核(运行指标,不是门槛),并完成 RCN-196。
