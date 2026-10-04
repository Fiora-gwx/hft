---
description: "冷进程:尾随 journal 与主机状态,维护按 venue×stage 的 hdrhistogram 并导出 Prometheus,执行健康规则(对账、资金费差额、外部成交、断腿、保证金、跨所标记价偏离、advisor 滞后、策略 Note、模拟账户、累计验证运行)并按 P0/P1/P2 告警,写每日日报(账户报告 + 按 SDK 描述符通用汇总的策略报告,实盘与模拟同口径),向机外看门狗提供 /healthz(含维护静默截止 maintenance_until);计算门槛 ⑩ 的同机干扰;只告警、不发命令"
kind: app
tier: cold
scope: core
status: spec
---

# watchtower

## 概要

`watchtower` 合并了 metrics-exporter 与 risk-monitor([13 O4](../../design/13-设计评审与优化.md)):它尾随 seq/raw journal 与主机状态,把 `LatencySample` 等事件聚合成 hdrhistogram 导出到 Prometheus,按规则表判定健康并按级别告警(P0/P1/P2),每天写日报(延迟分段、业务计数、每个账户的 `AccountDaily`、每个策略实例的策略报告,模拟账户标注"模拟"),并在升级主机上经 `/healthz` 向 dev2 上的机外看门狗报告本机交易进程的健康。策略相关的告警与报告**只按 SDK 描述符通用处理**:策略 Note 的名称、级别与 `v[]` 含义、策略报告字段都来自发布制品里的策略 schema JSON(SDK `StrategyDesc.notes`/`report`),本 app 不依赖任何策略包、没有按策略写的代码([apps/AGENTS.md](../AGENTS.md#策略装配只在这里))。**只告警、不发命令**:需要自动降级的情形(行情静默、时钟不可信、存储退化、保证金分档、对账差异)由 [hft-risk](../../crates/hft-risk/README.md) 的 cap 在 sequencer 内处理,进入与解除都写进 journal。它不做交易所 REST 对账(那是 [reconciler](../reconciler/README.md)),不撤单(那是 [kill-switch](../kill-switch/README.md)),**永远不发升级命令**。规则的判定函数在 [hft-telemetry](../../crates/hft-telemetry/README.md),本 app 只装配输入、状态与投递。

## 目录

- [使用本组件](#使用本组件)
  - [输入](#输入)
  - [直方图与 Prometheus 导出](#直方图与-prometheus-导出)
  - [机外健康检查(/healthz)](#机外健康检查healthz)
  - [健康规则](#健康规则)
  - [日报](#日报)
  - [告警分级与投递](#告警分级与投递)
  - [降级命令(可选扩展)](#降级命令可选扩展)
  - [配置文件字段](#配置文件字段)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
watchtower run    --config /etc/hft/watchtower.toml      # hft-watchtower.service
watchtower --check --config …                            # 只加载与校验配置(含策略 schema 目录与护栏阈值文件),不连接、不读 journal
watchtower rules  --config …                             # 打印生效的规则表(阈值来自配置)
watchtower replay --config … --journal <dir> --hour H    # 离线对历史 journal 重跑规则(只输出,不告警)
watchtower daily  --config … --journal <dir> --date D    # 离线重算某日日报(与生产日报逐字段比对)
watchtower interference --config … --from D1 --to D7 --seed S [--instance prod] [--aa]   # 门槛 ⑩ 的同机干扰:分块 bootstrap(只读本机按小时直方图;S 是重抽样种子,写进报告;--aa 为窗口前 sim 两半都关的 A/A 运行)
```

### 输入

| 来源 | 读取方式 | 用途 |
|---|---|---|
| seq journal(每个被监视实例:dev2 上为 `sim` 与联调窗口内指向测试网的 `prod` 角色实例;升级主机上为 `prod` 与 `sim`) | `hft-journal` reader,尾随热段(只读映射)或冷副本 | `LatencySample`、`OrderEvent`、`ExternalFill`、`MarginReport`、`AccountConfigReport`、`FundingSettle`、`Funding`、`Mark`、`Bbo`、`ExecDecision`、`Advice`(只读头:`strategy`、`basis_seq`)、`StrategyParams`(只读头:`strategy` → `kind_id`)、`ConnState`、`ClockStatus`、`HostProfile`、`Note`(含 cap 变化、`StartupGate`、OMS/执行层/ledger/advisor 注记、`source = Strategy` 的策略 Note)、`CmdResult`、`ReconcileReport`、sim-gw 的真值报告 |
| 策略 schema 目录 | 启动时与发布切换后只读加载(`hft-core schemas --out` 的输出:每策略 `<kind_id>-<name>.schema.json` + `index.json`,即发布归档的 `schemas/strategies/`,布局归 [deploy](../../deploy/README.md));`index.json` 摘要不符 → 退出 2 | 策略 Note 的码、名、级别、`v[]` 含义(`notes`)与策略报告字段(`report`);格式归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) |
| replayer 每日验证报告 | `paths.replay_reports` 下 `<instance>/<date>.json`(字段归 [testing §3.5](../../docs/testing.md#35-验证回放)) | `ReplayVerify` 规则、日报 `gates` 节 |
| 累计验证运行记录 | `paths.verify_runs`(`hft-verify` 的运行记录目录,只读;格式归 [tools](../../tools/README.md);本单元经附加组读取,组与目录权限归 [deploy](../../deploy/README.md)) | `VerifyRun` 规则、日报 `gates` 节 |
| `raw-*` journal | 同上,只读记录头与 `conn_id`、交易所序号字段 | 每连接消息率、双路落差(可选扩展) |
| `HeartbeatBlock`(每个实例) | 只读 mmap(`hft_rt::HeartbeatBlock::open_ro`;位置与状态见 [hft-core](../hft-core/README.md#heartbeatblock)) | sequencer 停摆、实例状态、单线程停滞、`/healthz`、按小时的 `sim` 状态 |
| 主机 | `/proc/meminfo`、`/proc/vmstat`、`/proc/swaps`、`/proc/pressure/{memory,cpu}`(整机)、`system.slice` 的 `cpu.pressure` 与 `memory.pressure`、`statvfs`、`chronyc tracking`、`/proc/*/task/*/stat`、`/proc/interrupts`;交易与 sim 网卡(启动时从 `hostprofile.json` 取 ENI-B、ENI-C 与启用时的 ENI-B2 的接口名,见下)的 ENA 整形计数(`ETHTOOL_GSTATS`,即 `ethtool -S` 的 `*_allowance_exceeded`) | 内存、swap-in 与 PSI、磁盘、时钟、隔离核闯入(可选扩展);ENA 整形发生在 `hw_rx` 之前,`kernel_rx` 看不到,只能由该计数发现 |
| 护栏阈值文件(`dev2`) | 本 app 配置键 `host.guard_config`([配置文件字段](#配置文件字段),watchtower.toml 自己的键)所指的文件,由 deploy 生成、护栏守护与本 app 读同一份([hft-core dev2 护栏](../hft-core/README.md#dev2-护栏) 是数值的唯一归属) | `HostPressure` 的阈值 |
| 标志文件 | 护栏锁存 `/var/lib/hft/guard/tripped`(持久,主机重启不消失)、联调窗口 `/run/hft/window/active`(写入方与语义归 hft-core 与 deploy);维护静默文件,路径由本 app 的配置键 `host.maint_file` 给出 | `GuardAction`、`ReplayVerify` 的定级,`/healthz` 的 `maintenance_until` |
| systemd | `systemctl is-active` 轮询 `hft-*` 单元(含 `hft-kill-switch`、dev2 上的 `hft-offbox-watchdog`)与 `hft-dev2-guard` 事件文件 | 进程失活、护栏动作、`/healthz` |
| app 致命原因 | `host.fatal_files` 中各单元的 `fatal.json`(app 以退出 2 结束前写入,如 [reconciler 启动核验](../reconciler/README.md#启动核验只读-key)) | `ProcessDown` 的级别与详情 |
| 主机剖面 | `paths.hostprofile`([tools/hostprobe](../../tools/README.md#hostprobe) 的输出,只读;只在启动时读一次):ENI-A 的私有 IP 作 `/healthz` 监听地址,ENI-B、ENI-C、ENI-B2 的接口名作 ENA 计数的网卡;没有该角色的网卡(dev2)即不监听 / 不采集。文件缺失、不可解析,或升级主机类(`host_class ≠ dev2`)上 ENI-A 的地址为 `unreadable` → 退出 2 | `/healthz` 监听、ENA 计数 |
| pipeline 与 reconciler 状态 | pipeline 每实例的 `status.json` 的 `alerts`(格式归 [pipeline](../../python/pipeline/README.md));reconciler 的 `<state>/status.json`(格式归 [reconciler Unknown 订单收敛](../reconciler/README.md#unknown-订单收敛)第 5 条);都只读、按文件变化重读 | `PipelineAlert`、`ReconcilerAlert` |
| archiver 状态 | journal-archiver 状态目录下的 `alerts.jsonl`(只读,路径为配置键 `paths.archiver_alerts`);`audit.jsonl`、quarantine 目录 | `ArchiverAlert`;归档滞后(可选扩展 `ArchiveLag`) |
| 链上 journal(可选扩展) | 同 seq journal | chain-core 启用时的链上规则 |

读 journal 只用 reader;watchtower 崩溃不影响热进程。重启后从 `<state>/cursor.json` 继续,直方图从空开始(Prometheus 侧有历史;按小时直方图从 `<state>/hourly/` 恢复)。游标按实例记 `(segment_no, first_seq, 段头 SHA-256, last_seq)`,各实例互不影响(另记 `paths.archiver_alerts` 已读到的字节偏移,重启后不重复投递已读的行):

- 游标所指的段在该实例的 journal 目录中不存在或段头哈希不符 → 该实例从当前最新段的起点重新尾随,写一条 P2(规则名 `CursorReset`,scope 为该实例;当日日报该实例节标"不完整"),不按旧 `last_seq` 跳过新 journal 的记录。
- **`prod` 的 journal 根被替换**(M7 → M8 切换正式账户,[launch D-23](../../docs/runbooks/launch.md) 第 4 步把 `<cold_root>/prod/` 整体移走、新 journal 从空开始)走上一条:不需要移动或删除 `cursor.json`,watchtower 重启后 `prod` 自动从新 journal 起步,`sim` 的游标照常续读。
- `cursor.json` 整体缺失(首次启动,或被人移走)或内容不可解析 → 每个实例都从当前最新段的起点尾随,各写一条 `CursorReset` P2;已经尾随过的记录会再计一次,当日日报标"不完整"。`cursor.json` 先写 `.new` 再 rename。

### 直方图与 Prometheus 导出

- 维度:`(instance, venue, inst_class, stage, trigger_kind, account_kind)`;`stage` 只用 [latency-budget.md §2](../../docs/latency-budget.md) 的分段名,含 `hedge_host` 与由 `sw_rx` 拆出的诊断分段 `nic_to_kernel`、`kernel_to_user`(常态记录,不作门槛)。Sim 账户的样本单独成数据集(是模拟延迟的抽样),不与实盘合并。
- `hdrhistogram`:3 位有效数字,1 ns–10 s;每维度三份——10 s 滚动窗口(规则用)、按 UTC 小时(门槛 ⑩ 的同机干扰用 `tick_to_trade`、`rep_to_order`、`hedge_host`,持久化到 `<state>/hourly/<date>.json`)与按日累计(日报用)。含 PHC 端点的分段按 hft-telemetry 的插值换算,误差界超限的样本只计入 `unsynced_cal`。序列数上限见[延迟与资源契约](#延迟与资源契约)。
- 每 10 s 导出:`hft_latency_ns{instance,venue,inst_class,stage,q}`、`hft_latency_count_total`;`/metrics` 只绑 `127.0.0.1:9464`。
- 其他指标族:`hft_msgs_total{venue,conn}`、`hft_conn_state{venue,conn}`、`hft_reject_total{venue,code}`、`hft_ratelimit_hit_total{venue}`、`hft_exec_decisions_total{account,algo,reason}`、`hft_external_fills_total{account,venue,cause}`、`hft_margin_ratio{account,venue}`、`hft_liq_dist{account,venue}`、`hft_margin_data_age_ms{account,venue}`、`hft_margin_stress_level{account}`、`hft_funding_data_age_s{venue,inst_class}`、`hft_mark_div_bp{asset,venue}`、`hft_leg_gap_ns`、`journal_persist_lag_ms{instance,journal}`、`hft_clock_offset_ns`、`hft_disk_free_ratio{mount}`、`hft_mem_available_bytes`、`hft_psi{resource}`、`hft_ena_allowance_exceeded_total{ifname,kind}`、`hft_unknown_age_s`、`hft_advisor_lag_ms{instance,strategy}`、`hft_strategy_notes_total{instance,strategy,kind,note,level}`(`note` 为描述符中的名,未登记的码为 `unregistered`)、`hft_funding_corrected_total{account,venue}`、`hft_sim_tail_lag_ms{instance}`(sim-gw 落后 journal 末尾的量,**只记录**:不设任何规则,模拟不做延迟验收,实盘繁忙时滞后是预期行为)。gauge 的定义见 [hft-telemetry 保证金与账户指标](../../crates/hft-telemetry/README.md#保证金与账户指标)。

### 机外健康检查(/healthz)

升级主机上的 watchtower 另开一个只读 HTTP 监听,供机外看门狗 `hft-offbox-watchdog.service`(每 15 s 一次,连续 60 s 不可达或不健康即告警;所在主机、单元、告警通道与维护静默期间的投递方式归 [deploy](../../deploy/README.md#机外看门狗))判断交易主机是否失联:

- **监听**:启动时从 `hostprofile.json` 读到的 ENI-A 私有 IP + `healthz_port`(地址不由 deploy 生成,见[输入](#输入)的主机剖面);安全组只放行看门狗所在主机(归 deploy)。与只绑 `127.0.0.1` 的 `/metrics` 是两个监听;主机剖面中没有 ENI-A 时不监听(dev2)。只响应 `GET /healthz`(查询串忽略),其他方法或路径 404;不带鉴权,响应里没有账户名、key、仓位或金额。
- **响应**:健康 `200`,不健康 `503`,`Content-Type: application/json`:

```text
{ "v": 2, "host": "<host>", "utc_ns": …, "ok": true|false, "reasons": ["…"],
  "instances": [{ "name": "prod", "state": "Running", "hb_age_ms": 12, "live_accounts": 2 }],
  "kill_switch": { "active": true },
  "recon": { "last_report_utc_ns": …, "age_s": 21 },
  "watchtower": { "sample_age_ms": 400, "tail_lag_ms": 150 },
  "maintenance_until": null | <utc_ns> }
```

- **`maintenance_until`**(维护静默):每次组装响应时读 `host.maint_file`(由本机命令 `hft-maint start <分钟>` 写、`hft-maint stop` 删除,命令归 deploy;内容为一行十进制 Unix 秒,即截止时刻 UTC);文件不存在、内容不可解析或截止时刻已过 → `null`;截止时刻晚于"当前 + 30 min" → 按当前 + 30 min 截断并在 `reasons` 中加 `maintenance_truncated`。它只是转告看门狗,**不改变 `ok` 的判定**,也不影响本进程自己的告警投递(静默期间 watchtower 的 P0/P1 照常发出)。看门狗会缓存最近一次读到的值,所以运维在停掉 watchtower 之前先执行 `hft-maint start`,并等看门狗读到一次(步骤归 [release-and-deploy](../../docs/runbooks/release-and-deploy.md))。
- **`ok = false` 的条件**(任一,括号内为写进 `reasons` 的值):某个有 Live 账户的实例 `state ∈ {Warmup, Running, Draining}` 但心跳超过 `hb_stale_ms` 未前进(`hb_stale:<instance>`);该实例单元非 active 且最后状态不是 `Stopped`(`unit_down:<instance>`);`hft-kill-switch` 非 active(`kill_switch_down`);有 Live 账户的实例处于 `Running` 而最近一组 `ReconcileReport` 的年龄 > `healthz.recon_max_age_s`(`recon_stale:<instance>`;本次运行尚无报告时年龄从该实例进入 `Running` 起算);watchtower 自身的主机采样年龄 > 10 s(内部停滞,`watchtower_stalled`)。干净停机(`state = Stopped`)不算不健康,在 `instances[].state` 中如实报告。`hb_stale_ms` 与心跳年龄的口径同[健康规则](#健康规则)的 `SequencerStall`。
- 由独立的 `healthz` 线程从最近一次主机采样与尾随状态组装,不等规则线程;watchtower 进程本身挂掉时看门狗看到的是"不可达"。

### 健康规则

阈值全部来自配置(不硬编码);下表"默认"是首版取值,**待标定**。核心规则全部启用;"可选扩展"规则的触发条件在表后。

| 规则 | 输入 | 条件(默认) | 级别 |
|---|---|---|---|
| `SequencerStall` | `HeartbeatBlock` 区 0 | `hb_stale_ms` 内不前进且 `state ∈ {Warmup, Running}`;实例有正式 Live 账户(`is_prod = true`)→ P0;只有测试网账户(Live、`is_prod = false`)或只有 Sim 账户(如 `sim` 实例)→ P1。`hb_stale_ms` 取该实例 hft-core 配置的 `watchdog.hb_stale_ms`(经 `instances[].hft_core_config` 只读);心跳年龄 = 当前时刻 − `host-probe` 最近一次采样到该计数前进的时刻 | P0/P1 |
| `ThreadStall` | `HeartbeatBlock` 区 2..31 | 已 claim 的某槽(区 2 .. `n_slots + 1`)`hb_stale_ms` 内不前进,且实例 `state ∈ {Warmup, Running}` | P1 |
| `ProcessDown` | systemd + `HeartbeatBlock.state` + `fatal.json` | `hft-core` 某实例非 active(`failed`、自动重启中的 `activating` 或 `inactive`)且最后状态不是 `Stopped`(心跳块不存在且单元 `inactive` = 本次开机未启动,不算):有正式 Live 账户(`is_prod = true`)→ P0,只有测试网账户或 Sim 账户 → P1;`hft-kill-switch` 任何非 `active` 状态 → P0;`hft-reconciler`、`hft-journal-archiver`、dev2 上的 `hft-offbox-watchdog` 处于 `failed` 或自动重启中的 `activating` → P1(被人、护栏、联调窗口或回放暂停停下的 `inactive` 不算);非 active 且其 `fatal.json` 的 `code` 在 `p0_fatal_codes`(默认 `["UidMismatch"]`)中 → P0,详情带 `code`、账户与 venue | P0/P1 |
| `FeedSilent` | 每 (instance, venue) 最近行情事件 | 超过 `silent_ms[venue]`(Binance 2,000;OKX、Bitget 5,000)无行情;只在该实例 `state = Running` 时判定,计时起点为本次运行(最近一条 `EpochStart` 之后)的最近一条行情事件,本次运行尚无行情事件时为该实例进入 `Running` 的时刻 | P1 |
| `MarkDivergence` | 同一资产在各 venue 的最新 `Mark`(永续)或 BBO 中间价(现货),派生量见 [hft-telemetry](../../crates/hft-telemetry/README.md#告警判定所需的派生量) | 某 venue 相对该资产全部已订阅 venue(含自身)最新值的中位数的偏离 > `mark_div_bp`(默认 50 bp)持续 > `hold_s`(默认 30 s);只对至少两个 venue 订阅的资产判定;详情列出各 venue 的值、偏离与最近更新时刻(用于发现"连接有 pong 但单个 inst 静默"的报价)。只告警,不影响报价可用性判定 | P1 |
| `UnknownAge` | OMS `Unknown` 持续时间 | > 10 s 未收敛 | P0 |
| `ReconcileMismatch` | `Note{CapSet}`(cap 为 `Recon`)、`ReconcileReport` 条目(`Funding` 条目除外)与 `sflags` | 该 (account, venue) 被置 `Recon` cap → P0;任一 `Warn` 条目名义 ≥ `recon_warn_usd`($50)→ P1、≥ `recon_p0_usd`($500)→ P0;有剩余数量的孤儿单 → P1;`Summary` 带 `HISTORY_UNCHECKED`(成交历史里出现未归属成交,[reconciler](../reconciler/README.md#reconcilereport))→ P0;`Position` 条目带 `AUTO_EXTERNAL` 时详情注明"已由引擎自动入账为外来成交"(该笔另由 `ExternalFill` 规则报 P0) | P1/P0 |
| `FundingDiff` | `ReconcileReport{item = Funding}`、`Note{FundingCorrected}` | 每条 `Note{FundingCorrected}`(账本自动校正了一笔资金费,含无历史条目的挂起与窗口结束入账)→ P1;`Funding` 条目 `|exch_amount − engine_amount|` > max(`funding_diff_alert.abs`(1 USDT), `funding_diff_alert.rel`(1%)× `|exch_amount|`)而尚未见到对应的 `FundingCorrected` → P1;指纹 = (account, venue, inst, settle_ts),同一笔只报一次。资金费差额由账本自动校正,本规则只告警 | P1 |
| `ExternalFill` | ledger 入账时写的 `Note{ExternalFill}`(每笔入账恰好一条,含对账 `AUTO_EXTERNAL` 与 OMS 防御路径合成、没有 `ExternalFill` 记录的外来成交)与同一事件内的 `ExternalFill` 记录(只补详情);同一笔只报一次,被 ledger 去重而未入账(无该 Note)的不报;本规则消费该 Note,`EngineNote` 不再重复报 | 出现即报,详情含 `cause`(Liquidation/Adl/Manual/Delivery/Unknown)、名义、另一腿状态;级别按 `cause` 定:`Manual`(人工在交易所网页操作,该 cell 进入 `manual_hold`,[hft-exec](../../crates/hft-exec/README.md))为 P1,其余(含对账 `AUTO_EXTERNAL` 合成的 `Unknown`)为 P0 | P0 / P1 |
| `StaleGenFill` | `Note{OmsStaleGenFill}` 与墓碑期迟到成交注记 `Note{OmsLateFill}` | 出现 | P0 |
| `ForeignOrderFill` | `Note{OmsForeignOrder}`、`Note{OmsForeignFillViaOrderEvent}` | `OmsForeignFillViaOrderEvent`(带成交的外来单经回报到达、走防御路径入账)→ P0;`OmsForeignOrder`(无成交的外来单)→ P1 | P0/P1 |
| `EngineNote` | 引擎 `Note`(`source ≠ Strategy`、`≠ CtlAudit`)与其 `Note.level`(码名与默认级别登记在 [hft-event](../../crates/hft-event/README.md)) | 未被本表其他规则消费的码:`level` 为 P0/P1 → 同级告警(消息带码名与 `v[]` 的解释,翻译归 hft-telemetry);P2 → Grafana 注释 + 日报;Info → 不告警;指纹 = (instance, source, code, account, group)。新登记的引擎 Note 因此不需要改本 app 就会被告警(例如 `Note{ExecSerialFallback}`、`Note{ExecManualHold}`、`Note{OmsResumeTimeout}`、`Note{ModelPathReset}`)。本表其他规则可以按自己的条件另定级别,不受登记级别约束 | P0/P1/P2 |
| `NakedExposure` | ledger 重建的每 group 两腿差 | 名义差 > `naked_usd`(每账户配置)持续 > 30 s | P0 |
| `HedgeBroken` | `Note{CapSet}`(`Broken`、`HedgeFailure`) | 出现 | P0 |
| `HedgeAutoFix` | `Note{ExecAutoFixDone}`、`Note{ExecAutoFixFailed}`(执行层对持续 `broken_auto_fix_ms` 的 `Broken` cell 的有界自动平仓,[hft-exec](../../crates/hft-exec/README.md)) | `ExecAutoFixDone` → P1(详情:平掉的腿与数量;`HedgeFailure` cap 仍需人工 `Resume`);`ExecAutoFixFailed` → P0 | P1/P0 |
| `RecoveryAnomaly` | `Note{RecoverMismatch}`、`Note{TailSkipped}` | 出现 | P0 |
| `AdvisorLag` | `Note{source = Advisor, code = AdvisorLag}` | 出现即报;滞后的口径(`engine_ns` 差)、阈值 `advisor_max_lag_ms` 与写入频率(超阈值时每 10 s 至多一条)唯一定义在 [hft-event](../../crates/hft-event/README.md),本规则不另设阈值;指纹 = (instance, strategy) | P1 |
| `StrategyNote` | `Note{source = Strategy}` + 策略 schema 中的 `notes` 描述符 | 按该码描述符的 `level`:P0/P1 → 同级告警(消息带描述符的名与 `v[]` 含义);P2 → Grafana 注释 + 日报;Info → 只进日报;描述符中没有的码 → P2 并计入 `unregistered` | P0/P1/P2 |
| `ReplayVerify` | replayer 每日验证报告(含补跑写出的报告,`<date>` 为被验证日) | 任一日的 `verify` 或 `--advice recompute` 不一致 → P1(详情:第一个分歧的 `seq` 与组件);`recompute = skipped`(当日没有归档的 Model 检查点)→ P2;06:00 UTC 仍无前一日报告 → P1,但前一日或当前处于联调窗口(`/run/hft/window/active`)、护栏锁存(`/var/lib/hft/guard/tripped`)或实例停机时 → P2(补跑规则归 [testing §6](../../docs/testing.md#6-ci-运行矩阵));某日缺报告超过 3 天(当前时刻 − 该日次日 00:00 UTC > 72 h)→ P1(本机冷副本将按保留期删除,届时该日无法补跑)。"缺报告的日子" = 该实例冷副本中有已封存 `seq` 段、早于当日而没有报告的 UTC 日;前一日的窗口、锁存与停机取自该日的按小时状态(见[实现说明](#实现说明));scope = (instance, 日期) | P1/P2 |
| `VerifyRun` | `hft-verify` 运行记录 | 某次运行有失败(段记录的 `crash_sha1` 或 `failed_seeds` 非空,或 `violations > 0`:fuzz 崩溃、DST 违反、`hft-dst` 异常终止)→ P1(详情:目标或种子,提示 `hft-verify crash list`;失败、疑似与中止的分类归 [tools hft-verify](../../tools/README.md#hft-verify),清零规则归 [testing §6.1](../../docs/testing.md#61-服务器上的累计运行));只有疑似(`suspect_sha1` 非空:受限主机上的内存超限或超时)→ P2,提示由 CI 复现;中止的运行不告警;`hft-verify.timer` 处于 active 而最新段记录的 `end_utc` 早于 48 h 前 → P2 | P1/P2 |
| `PipelineAlert` | pipeline 各实例 `status.json` 的 `alerts` 项(`{level, code, group?, detail, at}`) | 每个新出现的项按其 `level` 投递(P2 → Grafana 注释 + 日报),详情带实例、`code`、group 与 `detail`;scope = (instance, `code`, group);事件型 | P1/P2 |
| `ReconcilerAlert` | reconciler `status.json` 的每 (account, venue) 项 | 某项 `alert` 由 `null` 变为 `"P1"`(连续查询失败达 `unknown_alert_rounds` 轮)→ P1,详情带账户、venue 与两个失败计数;变回 `null` 即条件消失;scope = (account, venue) | P1 |
| `StartupSlow` | `Note{StartupStep}` 结束记录的 `v[3]` 标志(位定义归 [hft-core 启动顺序](../hft-core/README.md#启动顺序)) | bit 0(已超过 `startup.budget_s`)→ P1;bit 1(步 7 因 `startup.gate_timeout_s` 到期结束)→ P1,同时 bit 2(`tail_lost`)置位 → P0;scope = (instance, 步号);事件型 | P1/P0 |
| `ArchiverAlert` | journal-archiver 的告警行(字段与 `kind` 归 [journal-archiver 上传与删除纪律](../journal-archiver/README.md#上传与删除纪律)) | 每一行按其 `level` 投递,详情带 `kind`、prefix、实例、journal 与小时;scope = (`kind`, prefix, instance, journal, hour);事件型 | P0/P1 |
| `MarginStress` | `MarginStress` cap 档位(`Note{CapSet}`、`Note{MarginLevel}` 与 `Risk` 事件);档位按到强平距离 `liq_dist` 划分,阈值 `liq_dist_block_open`/`liq_dist_reduce`/`liq_dist_force` 与比率交叉校验归 [hft-risk 保证金分档](../../crates/hft-risk/README.md#保证金分档marginstress),本规则不另设阈值 | L1(禁开)→ P1;L2(须在截止前减仓)→ P0;L3(执行层强制减仓)→ P0;详情:每个 venue 的 `liq_dist`、决定它的 inst、取值来源(交易所 `liq_px` 或 ledger 估算,取更保守者)、维持/保证金比率、L2 截止时刻;scope = (instance, account, 档位),条件 = 当前档位等于该档(升档与降档都开该档的新记录) | P1/P0 |
| `MarginDataStale` | `MarginReport` 年龄 | > `margin_stale_ms`(与 hft-risk 同一参数,读 `RiskParams`;hft-risk 同时把该账户置 L1) | P1 |
| `FundingStale` | 每个订阅永续 inst 的 `Funding` 事件 | 年龄 > `funding_stale_s`(300);或下次结算时刻已过 60 s 仍未更新;或持仓 inst 在结算后 5 min 内无 `FundingSettle`(推送或合成) | P1 |
| `AccountConfigDrift` | `AccountConfigReport.mismatches`;hft-risk 的 `AccountCfg` cap(`Note{CapSet}`,`params[0]` 原因位) | 该账户本次运行的启动闸门放行(`Note{StartupGate}` 的 `released = 1`)之后 `mismatches` 出现非 0 位(持仓模式、保证金模式、多资产、UID)→ P0;`AccountCfg` cap 的原因位只有 `LEVERAGE`(交易中永续 inst 的实际杠杆未知或超过 `RiskParams.max_perp_leverage_x100`,hft-risk 判定、禁开)→ P1;放行之前的不符不由本规则报(闸门挡住) | P0/P1 |
| `DailyLossTrip` | 风控熔断的 cap 记录 | 出现 | P0 |
| `PersistLag` | 各段头 `committed_len − synced_len` 与时间 | p99(60 s)> 50 ms P2;> 1 s P1;> 10 s P0 | P2/P1/P0 |
| `StorageDegraded` | `Storage` cap(housekeeping 门限触发,[hft-core `storage.*`](../hft-core/README.md#配置文件字段)) | cap 置位 → P1;存储原因的 Drain(`Storage` cap 某项档位为 2 之后出现 `Draining` 的 `Note{CapSet}`)→ P0;档位 2 而 Drain 被推迟期间保持 P1;命令发起的 Drain 不属本规则 | P1/P0 |
| `SimDrift` | sim-gw 真值报告(`PositionReport`/`BalanceReport`)vs ledger | 同一 Sim 账户连续 2 次报告差异超过 `dust` | P1 |
| `HostPressure`(`dev2`) | `MemAvailable`、swap-in 速率(`/proc/vmstat` `pswpin`)、`system.slice` 的 `cpu.pressure` 与 `memory.pressure`、整机 PSI、swap 已用量、hft 单元 `memory.current` | 任一护栏停机信号越过其阈值 → P1(单次采样即报:上一个 10 s 评估窗口内任一次 1 s 采样越线即命中,不等护栏要求的持续时间);任一 hft 单元 > 其 `MemoryMax` 的 90%([hft-core 内存表](../hft-core/README.md#内存与-journal-段按主机类唯一归属))→ P1;只告警信号(如整机 PSI)越过其阈值 → 按护栏表给出的级别(P2,进日报)。哪些信号是停机信号、哪些只告警、阈值与只告警信号的级别都从 `host.guard_config` 读,与护栏守护同一份(数值归 [hft-core dev2 护栏](../hft-core/README.md#dev2-护栏)) | P1/P2 |
| `GuardAction`(`dev2`) | `hft-dev2-guard` 事件文件、`/var/lib/hft/guard/tripped` | 一级动作 → P1;二级(停 hft-core)→ P1,有正式 Live 账户时 P0(dev2 不放正式实盘账户,正常情况下为 P1);守护在资源回落后自动重启 `hft-core@sim`(规则归 [hft-core dev2 护栏](../hft-core/README.md#dev2-护栏))→ P1;锁存标志存在期间每 24 h 重复一次 P1(详情:批处理在人工清除前不会启动;ack 不停止这项提醒,标志清除后记录关闭);护栏只告警、不动作的事件(同机其他业务单元的健康异常;停机信号越线而 hft 自身用量不明显,判定归 hft-core)→ P1 | P1/P0 |
| `DualFeedGap`(可选扩展) | raw journal 主/备连接序号 | 同 inst 落差 > 50 持续 5 s | P2 |
| `StageP99OverBudget` / `StageP99Sustained`(可选扩展) | 10 s 窗口直方图 | p99 > 预算 / 连续 3 窗口(只告警,不触发任何降级) | P2 / P1 |
| `ClockOffset`(可选扩展) | `ClockStatus` | offset > 200 µs P2;> 1 ms 或不同步 P1 | P2/P1 |
| `HugepagesLow`、`IsolatedCoreIntrusion`、`IrqDrift`(可选扩展) | 主机 | 见各自名称 | P1 |
| `ReconcileStale`、`DeadmanRefreshFail`、`RateLimitStorm`、`ArchiveLag`(可选扩展) | 对账时间、死手注记、限速计数、archiver 审计 | > 3 × 对账周期;连续 2 次刷新失败(P0);60 s > 10 次;小时后 45 min 未提交 | P1/P0 |
| `ShadowDivergence`(可选扩展) | 同实例 Shadow 与 Live 的意图流 | 24 h 一致率 < 99.9% | P2 |
| 链上规则(可选扩展) | chain journal | 包含率下降、Unknown 超过 3 块等 | P2/P0 |

- **可选扩展规则的触发条件**:`StageP99*`/`IsolatedCoreIntrusion`/`IrqDrift`/`HugepagesLow` — 统一升级主机并开始延迟验收(门槛 ⑩)时;`DualFeedGap` — 某 venue 启用 L2 双路合并时;`ClockOffset` — 运营中出现 hft-risk 时钟 cap 覆盖不到的告警需求时;`ReconcileStale`/`DeadmanRefreshFail`/`RateLimitStorm`/`ArchiveLag` — 运营一个月后按实际漏报情况逐条打开;`ShadowDivergence` — 新参数以 Shadow 长期并行运行、需要自动对比时;链上规则 — chain-core 启用时。
- **按主机类启用**:`HostPressure` 与 `GuardAction` 只在 `dev2` 启用——护栏只存在于 dev2([hft-core dev2 护栏](../hft-core/README.md#dev2-护栏));其他主机类(含 `c4`)没有护栏,配置模板把这两条写为 `enabled = false`、不设护栏阈值文件的配置键,也不另报 `MemAvailable`;`HostPressure` 启用而缺该键 → 退出 2。
- `NakedExposure` 与 ledger 同源,看不见漏记的成交;漏记由 `ReconcileMismatch`(交易所真值)与 `ExternalFill`/`ForeignOrderFill`/`StaleGenFill` 补上。
- 新增规则 = hft-telemetry 判定函数 + 本表一行 + 配置默认值([architecture.md §8](../../docs/architecture.md#8-新行为放哪))。

### 日报

每日 00:05 UTC 写 `/var/lib/hft/watchtower/daily/<date>.json`(每个被监视实例一节,节内为下表各项),ops_bot 读取并推送(Sim 账户与 `sim` 实例的条目带"模拟"标注;实盘与模拟同口径、分开出报):

| 节 | 内容 |
|---|---|
| `latency` | 各 stage 的 p50/p99/p99.9/max 与计数(按 `host.class` 标注;`dev2` 只作信息;p99.9 只记录),以及 `unsynced_cal` 计数与误差界分布;按 UTC 小时的 `tick_to_trade`、`rep_to_order`、`hedge_host` p50/p99/n 与该小时 `sim` 实例状态(`On`/`Off`/`Mixed`);`sim_tail_lag_ms` 的 p50/p99/max(只作信息);按小时的 ENA `*_allowance_exceeded` 增量(按网卡与种类,只作信息,⑩ 报告引用)。行情类分段的按事件加权统计不在日报里,由 `replayer latency` 离线给出 |
| `accounts` | 每个账户一行 `AccountDaily` 与每 group 一行(字段定义见 [hft-sim 报告字段](../../crates/hft-sim/README.md#报告字段),计算在 hft-telemetry `account_report`) |
| `strategies` | 每个策略实例(`strategy`, `account`)一组:按 `notes` 描述符逐码的当日计数(名、级别)与 `unregistered` 计数;按 `report` 描述符逐字段的当日值(名、单位、聚合方式 `Count`/`Sum`/`Last`/`Max`/`P99`;`scope = Group` 的字段按 group 分行)。计算在 hft-telemetry `strategy_report`,与 journal-archiver 的 `strategy_daily` 数据集同一函数 |
| `funding` | 每 (account, venue) 当日 `FundingCorrected` 笔数与金额合计、`FundingDiff` 告警笔数 |
| `calibration` | `calib_pairs` 中每对(正式实盘账户, 同参数 Sim 账户)的指标差异表与是否达标([hft-sim 与正式实盘并行校准](../../crates/hft-sim/README.md#与正式实盘并行校准)) |
| `alerts` | 当日各规则触发次数、P0/P1 列表与 ack 情况 |
| `gates` | 当日冷启动与闸门放行记录(含由停机标记写入的启动 Kill);Model 检查点沿用、只丢路径状态节或整份丢弃(`Note{ModelPathReset}`、`Note{ModelCheckpointDiscarded}` 与原因);当日收到的 `replay-verify` 与 recompute 报告(含补跑的日子与跳过)以及仍缺报告的日子;当日 `hft-verify` 运行摘要(运行数、失败数、疑似数,累计量以服务器账本为准) |

日报由 journal、同版本的策略 schema 目录与当日的按小时状态文件可重算:`watchtower daily` 离线重算结果与生产日报逐字段相等。`watchtower interference` 读 `<state>/hourly/` 的按小时直方图,按 [testing §8.2](../../docs/testing.md#82-延迟验收的测法) 的测法(块长、样本量门槛、置信水平、重抽样次数、通过线与区间半宽上限都取自 testing,由本 app 的配置模板传入,不在代码里写死)输出块表(含每块是否为资金费块与各类块的先开/先关计数)、每组的 (n 开, n 关, R, 置信区间, 判定) 与是否通过;判定含"区间过宽、延长窗口"。`--aa` 读窗口前 sim 两半都关的 A/A 日子,按同一配平规则给每块的两个小时贴"开/关"标签,输出各组的区间半宽(计算都是 hft-telemetry 的 `interference`)。

### 告警分级与投递

| 级别 | 含义 | 通道 | 重复 |
|---|---|---|---|
| P0 | 资金风险 | Telegram 告警群 + Bark critical + 电话(供应商未选定,见开发备忘;选定是门槛 ⑪ 的一项) | 每 60 s 重复直到 ack |
| P1 | 可用性 | Telegram 告警群 | 每 10 min 重复直到条件消失或 ack |
| P2 | 退化 | Grafana 注释 + 日报 | 不重复 |

- 投递由 watchtower 直接完成(独立的告警 bot token 与 Bark key),**不经 ops_bot**:ops_bot 宕机不应吞掉 P0。ops_bot 读 `<state>/alerts.jsonl` 做展示与 `/ack`。
- 告警记录:`{id, fingerprint, level, rule, scope, first_utc_ns, last_utc_ns, count, details, acked_by?, acked_utc_ns?}`;`fingerprint = hash(rule, scope)` 去重。scope 是该次命中的作用对象(规则输入所在的实例、账户、venue、inst、group、单元或日期中该规则具备的部分;规则表写了指纹或 scope 的按表);以单条记录为对象的事件型规则(`ExternalFill`、`StaleGenFill`、`ForeignOrderFill`、`HedgeBroken`、`HedgeAutoFix`、`RecoveryAnomaly`、`DailyLossTrip`、`ReconcileMismatch` 的 `HISTORY_UNCHECKED`)的 scope 含触发记录的 seq,每条记录一条告警;`StrategyNote` 的指纹 = (instance, strategy, code, group)。
- **记录的生命周期**:某指纹没有未关闭的记录时命中 → 新记录(新 `id`)并立即投递;已有未关闭记录时命中 → `count + 1`、更新 `last_utc_ns`,不另投递,命中级别高于记录级别时记录升级并立即按新级别投递。**条件**:窗口型规则 = 最近一次评估命中;事件型规则 = 同指纹在最近 `alert.repeat_s.p1` 内有命中。窗口型规则的"持续 > X"按连续命中的评估计(本次评估时刻 − 连续命中中第一次的评估时刻)。**关闭**:P0 在已 ack 且条件消失后关闭;P1、P2 在 ack 或条件消失后关闭;关闭后同指纹再命中开新记录。上表的重复按记录的当前级别;`GuardAction` 的锁存提醒按该规则。重启时从 `alerts.jsonl` 恢复未关闭的记录(同一 `id`,沿用重复节拍,不重发首条)。
- 每条消息带实例与账户标签(标签来自 refdata `AccountMeta`;Sim 账户另带"模拟")。
- ack:ops_bot `/ack <id>`(角色按 [control-plane](../../docs/subsystems/control-plane.md) 的权限表)→ ops_bot 追加写 `<state>/acks.jsonl`(每行的字段归 [ops_bot](../../python/ops_bot/README.md) 的 `/ack`;watchtower 只读该文件,按 `id` 匹配记录,不可解析的行与不是本 app 记录的 `id` 跳过;ops_bot 对它的写权限与本单元对状态目录的权限归 [deploy](../../deploy/README.md))。
- 整机失联(watchtower 与交易进程同时消失)由机外看门狗经 `/healthz` 发现并用它自己的独立通道告警(见[机外健康检查](#机外健康检查healthz))。

### 降级命令(可选扩展)

core 配置中 watchtower 不连 ctl socket、不在 hft-ctl principals 中登记,`rules.<name>.actions_enabled` 一律为 `false`,加载时出现 `true` → 退出 2。触发条件:运营证明某类降级必须由外部进程发起,且与 hft-risk 的 cap 去重后仍有必要;启用属于规格变更(先改本节与 hft-ctl 主体表,再放开加载校验)。启用后经 `hft-ctl` 客户端(uid `hft-watch`)只发降级命令,每条带 `reason = rule 名`,同一 (rule, scope) 10 min 内只发一次,从不发升级或 `Resume`。

### 配置文件字段

`/etc/hft/watchtower.toml`,`schema = 1`。每条规则的开关与阈值都必须显式写出(缺任何一项 → 退出 2);下表"默认 / 来源"列的规则阈值是 deploy 配置模板给出的首版取值。

| 键 | 类型 | 含义 | 默认 / 来源 |
|---|---|---|---|
| `instances` | [`{name, journal_cold, journal_ram, heartbeat_shm, hft_core_config}`] | 监视的 hft-core 实例;`name ∈ {"prod", "sim"}`(与 journal 实例名一致,不重复);`hft_core_config` 是该实例的 hft-core 配置文件(只读,取停摆窗口 `watchdog.hb_stale_ms`,与 kill-switch 同一口径) | 必填 |
| `strategy_schemas` | path | 策略 schema 目录(发布归档的 `schemas/strategies/`:`index.json` + 每策略 `<kind_id>-<name>.schema.json`) | 必填 |
| `paths.replay_reports` | path | replayer 每日验证报告目录 | `/var/lib/hft/replay-verify` |
| `paths.verify_runs` | path | `hft-verify` 运行记录目录(只读;布局归 tools) | `/var/lib/hft/verify/runs` |
| `paths.archiver_alerts` | path | journal-archiver 的告警行文件(只读) | `/var/lib/hft/archiver/alerts.jsonl` |
| `paths.pipeline_state` | path | pipeline 状态根(只读;其下 `<实例>/status.json`,`PipelineAlert` 读;deploy 模板填写) | `/var/lib/hft/pipeline` |
| `paths.reconciler_status` | path | reconciler 的 `status.json`(只读) | `/var/lib/hft/reconciler/status.json` |
| `paths.hostprofile` | path | hostprobe 的输出(只读,启动时读一次) | `/run/hft/hostprofile.json` |
| `export.listen` / `export.interval_s` | string / u16 | `/metrics` 地址与周期 | `127.0.0.1:9464` / 10 |
| `healthz_port` | u16 | `/healthz` 端口(地址取主机剖面的 ENI-A 私有 IP) | 9465 |
| `healthz.recon_max_age_s` | u32 | 最近一组对账报告的最大年龄 | 90(= 3 × reconciler `interval_s`) |
| `hist.sig_digits` / `hist.max_ns` | u8 / u64 | | 3 / 10 s |
| `hist.max_series` / `hist.mem_frac` | u32 / Rate | 序列数上限:取 `max_series` 与 `⌊mem_frac × 本单元 MemoryMax / 每序列字节⌋` 的较小者(`MemoryMax` 启动时读本单元 cgroup 的 `memory.max`;其值为 `max` 时只取 `max_series`) | 20,000(`dev2` 为 4,000,hft-core dev2 内存表按此计)/ 0.5 |
| `interference.*` | 表 | `watchtower interference` 的参数:`block_hours`、`min_n_each_side`、`max_ratio_ub`、`max_halfwidth`、`conf`、`resamples`、`hard_stages`、`funding_hours_utc`(取值唯一归属 [testing §8.2](../../docs/testing.md#82-延迟验收的测法),deploy 的配置模板按它填写) | 必填 |
| `rules.<name>.enabled` | bool | 每条规则开关 | 必填(每条都要写) |
| `rules.<name>.actions_enabled` | bool | 必须为 `false`(见[降级命令](#降级命令可选扩展)) | 必填 |
| `rules.<name>.*` | 阈值 | 见规则表 | 必填;模板取表中默认 |
| `rules.FundingDiff.funding_diff_alert` | `{abs: USDT 定点, rel: Rate}` | 资金费逐笔差额告警线(两者取大) | `{abs = 1 USDT, rel = 1%}` |
| `rules.MarkDivergence.mark_div_bp` / `.hold_s` | u32 / u32 | 跨所 mark/mid 偏离阈值与持续时间 | 50 / 30(待标定) |
| `rules.ProcessDown.p0_fatal_codes` | [string] | `fatal.json` 中按 P0 处理的 `code` | `["UidMismatch"]` |
| `host.fatal_files` | {unit → path} | 各单元的 `fatal.json` 位置 | `{hft-reconciler = "/var/lib/hft/reconciler/fatal.json"}` |
| `host.guard_config` | path | 本 app 自己的键(watchtower.toml;与热进程配置的 `[host]` 表无关):护栏阈值文件(`HostPressure` 用) | `dev2` 必填(deploy 生成);其他主机类不设 |
| `host.maint_file` | path | 本 app 的键:维护静默标志(deploy 的 `hft-maint` 写,内容为一行十进制 Unix 秒 = 截止时刻 UTC) | `/run/hft/maint/until` |
| `naked_usd` / `recon_warn_usd` / `recon_p0_usd` | {account → USD} | 每账户名义阈值 | 必填 |
| `calib_pairs` | [`{live, sim}`] | 校准配对的账户名 | `[]` |
| `budgets` | path | 分段预算表(从 latency-budget.md 生成) | 必填 |
| `alert.telegram_chat_id` / `.telegram_token_secret` / `.bark_key_secret` | | 告警通道;秘密由 deploy 的 `hft-secrets` 预取(前缀归 deploy) | 必填 |
| `alert.repeat_s` | `{p0, p1}` | | `{60, 600}` |
| `paths.state` | path | 游标、告警、日报、按小时直方图 | `/var/lib/hft/watchtower` |
| `host.guard_events` | path | dev2 护栏事件文件 | `/var/lib/hft/guard/events.jsonl` |
| `chain` | [`{name, journal_cold, heartbeat_shm}`] | 可选扩展 | `[]` |

## 实现说明

<details><summary>实现要点</summary>

- 线程:`tail-<instance>`(每实例一个,读 seq journal)、`tail-raw`、`host-probe`(1 s 周期)、`rules`(每 10 s 评估窗口;事件型规则在尾随线程即时判定)、`daily`、`export`(HTTP,回环)、`healthz`(HTTP,ENI-A;只读最近一次采样与尾随状态的快照)、`alert`(投递与重试)。线程间用有界 channel。
- 延迟样本:`LatencySample` 经 hft-telemetry 的 `stages` 换算(含 PHC 端点的分段要等下一条 `ClockStatus` 才出数,约 1 s);`hw_rx` 为 SW 的样本进入单独的 `ts_src=sw` 标签。按小时直方图在整点关闭时连同该小时 `sim` 状态(由 `host-probe` 每秒读 `sim` 实例的 `HeartbeatBlock.state` 累计)写进 `<state>/hourly/<date>.json`;同一项另记该小时是否采样到联调窗口标志、护栏锁存标志,以及每个实例是否有 `Stopped` 或单元非 active 的采样(`ReplayVerify` 的定级读它)。
- `NakedExposure`、`SimDrift`、日报的账户报告用 `hft-ledger` 从 journal 重建——与热进程同一份记账代码。
- 事件型规则(`ExternalFill`、`StaleGenFill`、`ForeignOrderFill`、`HedgeBroken`、`HedgeAutoFix`、`RecoveryAnomaly`、`DailyLossTrip`、`ArchiverAlert`、`PipelineAlert`、`ReconcilerAlert`、`StartupSlow`、`FundingDiff`、P0/P1 级的 `StrategyNote`、`AccountConfigDrift`、`AdvisorLag`、`MarginStress` 档位变化、`ReconcileMismatch` 的 `HISTORY_UNCHECKED`、P0/P1 级的 `EngineNote`)在读到记录时即判定,不等 10 s 窗口。
- `MarkDivergence` 在 `rules` 线程按 10 s 窗口评估,偏离的计算与持续计时是 hft-telemetry 的纯函数;`Mark`/`Bbo` 只取每 (venue, inst) 最新值,不进直方图。
- **策略描述符**:启动时用 hft-telemetry 的 `StrategyCatalog::load` 读策略 schema 目录(校验 `index.json` 摘要);尾随时从每个实例 journal 的 `StrategyParams` 头建立 `strategy → kind_id` 表,`Note{source = Strategy}` 经它找到描述符。journal 中出现目录里没有的 `kind_id`(发布与 schema 目录不同步)→ P1(规则名 `StrategyNote`)并按 `unregistered` 计数,不退出。发布切换(当前发布目录指向变化)后重新加载目录;重新加载失败(摘要不符或文件缺失)→ 保留旧目录继续运行并报 P1(同上)。
- `AdvisorLag` 只读 journal 中的 `Note{AdvisorLag}`,在 watchtower 侧不计算滞后、不读系统时钟;同一 journal 离线重跑得到相同命中。
- `fatal.json` 只在对应单元非 active 时读取;单元重新 active 后该告警随条件消失。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/main.rs` | 子命令 |
| `src/config.rs` | TOML、规则阈值校验(每条规则必须显式配置)、`actions_enabled` 校验、护栏阈值文件 |
| `src/tail.rs` | journal 尾随与事件分派 |
| `src/host.rs` | 主机采样、护栏事件、按小时 `sim` 状态 |
| `src/hist.rs` | 直方图表(10 s、按小时、按日)、序列数上限 |
| `src/rules.rs` | 规则装配、连续窗口状态、去重 |
| `src/healthz.rs` | `/healthz` 监听与响应组装(含读维护静默文件、截断到 30 min) |
| `src/daily.rs` | 日报(调用 hft-telemetry `account_report`、`strategy_report`)与 `interference` 子命令 |
| `src/catalog.rs` | 策略 schema 目录加载与 `strategy → kind_id` 表(只读 JSON,不链接策略包) |
| `src/export.rs` | Prometheus 文本格式 |
| `src/alert.rs` | Telegram/Bark 投递、去重、重复、ack 读取 |

</details>

## 延迟与资源契约

冷进程,无热路径约束。

| 指标 | 目标 |
|---|---|
| 事件写入 journal → 进入直方图 | p99 < 1 s(尾随热段) |
| 规则命中 → 告警发出 | P0 < 5 s(含一次重试);事件型 P0 < 2 s |
| `/healthz` 响应 | < 50 ms;响应内容的采样年龄 ≤ 2 s(正常运行时) |
| 日报生成 | 00:05 UTC 后 ≤ 5 min |
| CPU / 内存 | ≤ 1 核(`CPUQuota=100%`;`dev2` 另受 hft 单元所在 slice 的约束,划分见 [dev2 护栏](../hft-core/README.md#dev2-护栏));内存按 [hft-core 内存表](../hft-core/README.md#内存与-journal-段按主机类唯一归属);直方图序列数按 `hist.max_series` 与 `hist.mem_frac` 折算,超出并入 `other` |

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-event`、`hft-journal`(只用 `JournalReader`)、`hft-telemetry`(直方图、规则函数、账户报告、策略报告、策略目录与同机干扰计算)、`hft-ledger`、`hft-refdata`、`hft-rt`(只读映射 `HeartbeatBlock`)、`hft-ctl`(客户端,只在可选扩展"降级命令"启用后使用)。
- 外部:`hdrhistogram`、`tokio`、`hyper`(导出与 `/healthz`)、`reqwest`(投递)、`serde`/`toml`/`serde_json`。
- 禁止:`JournalWriter`、hot crate 运行时、任何 `strategies/*` 与 `hft-strategy-registry`(策略信息只来自 journal 与 schema JSON)。

## 延伸阅读

- [09 §1–4、§6](../../design/09-可观测性、风控与控制面.md)、[02 §6](../../design/02-硬件与托管方案.md)、[12](../../design/12-风险与未决问题.md)
- [latency-budget](../../docs/latency-budget.md)、[hft-telemetry](../../crates/hft-telemetry/README.md)、[hft-risk](../../crates/hft-risk/README.md)(cap 与 MarginStress 档位)、[hft-sim](../../crates/hft-sim/README.md)(报告字段与校准)、[hft-strategy-sdk 策略描述符](../../crates/hft-strategy-sdk/README.md#策略描述符note报表策略命令)、[hft-ledger](../../crates/hft-ledger/README.md)(资金费自动校正)、[testing §8](../../docs/testing.md#8-上线门槛)(门槛 ⑩ 的同机干扰、⑪ 的看门狗演练)
- [round8-decisions](../../design/_work/round8-decisions.md)(资金费只告警、断腿自动有界平仓、机外看门狗)
- 现有实现:Trading `crates/telemetry`(指标命名、磁盘水位滞回)、`docs/runbooks/alerts.md`

## 已知限制与后续工作

- 电话告警通道未选型;选定前 P0 只有 Telegram + Bark critical(选定是门槛 ⑪ 的一项)。
- 单实例:watchtower 自身宕机由 systemd 重启;在升级主机上,watchtower 或整机失联由机外看门狗经 `/healthz` 发现(看门狗所在主机归 deploy)。dev2 上的 watchtower 没有机外兜底(dev2 不放正式实盘账户)。
- 维护静默只作用于机外看门狗的电话通道;维护期间 watchtower 自己发出的 P0(例如激活时短暂的 `ProcessDown`)照常打电话,计划操作前由值班人知会。
- 名义阈值(`naked_usd`、`recon_*_usd`)按账户规模手工设定;运营调整 `RiskParams` 风险上限时同步调整(步骤见 [launch](../../docs/runbooks/launch.md))。保证金告警没有自己的阈值,跟随 hft-risk 的 `liq_dist_*` 标定。
- `MarkDivergence` 只是旁证告警:它不改变报价可用性的判定(安静 ≠ 过期,规则归 [defensive-patterns](../../docs/defensive-patterns.md)),也无法区分"某所真实偏离"与"某所报价停更"。
- ClickHouse/Grafana 看板定义不在本 README,放 `deploy/grafana/`。

### 开发备忘

<details><summary>给实现者</summary>

- `actions_enabled` 全部为 `false`(加载时校验);用告警记录校准阈值(尤其各所 `FeedSilent`、`FundingStale` 与 `MarkDivergence` 的误报),再评估是否需要任何动作。
- **待核实**:各 core venue 的资金费推送频率(决定 `funding_stale_s` 的合理下限);方法:在 dev2 上订阅 24 h,统计每 inst `Funding` 事件间隔分布。
- **待核实**:`mark_div_bp` 的合理默认值;方法:用现有 collector 数据统计各资产跨所 mark/mid 偏离的分布(p99.9),取其上方留余量。
- 电话告警供应商与 API 未选定(与机外看门狗共用,选型归 deploy);Bark critical 在 iOS 的静音穿透行为待核实(方法:演练时让测试手机处于静音与专注模式,记录是否响铃)。
- 每序列字节数(`hist.mem_frac` 折算用)以 hft-telemetry 的 `telemetry/series_bytes` 基准为准。
- `chronyc` 输出按字段名解析 `chronyc tracking` 文本,不依赖 `-c` 列序。
- **待核实**:非 root 的本单元用户能否经 `SIOCETHTOOL`/`ETHTOOL_GSTATS` 读 ENA 统计,以及 ENA 驱动在 c8g 上导出的 `*_allowance_exceeded` 名称;方法:M1.5 探针实例上以同一用户读一次并对照 `ethtool -S`。

</details>
