---
description: "冷进程:尾随已封存的交易 journal 段(seq/raw;链上 journal 为可选扩展),转成按 (exchange, dataset, symbol, hour) 分区的 zstd Parquet 并连同 journal 原件上传到独立的 S3 前缀 hft/<host>/<instance>/(测试网账户实例为 hft-testnet/<host>/prod/),每小时一份带 SHA-256 的 manifest,每日写账户报告与策略报告数据集(策略报告按 SDK 描述符通用汇总;模拟与实盘同口径),每日归档一份 Model 检查点供 recompute,上传持久元数据(DurableMeta),并把服务器上累计验证运行的文件原样上传到 hft/verify/<host>/;重建主机时以 restore 从 S3 取回已提交的段;不是研究采集系统"
kind: app
tier: cold
scope: core
status: spec
---

# journal-archiver

## 概要

`journal-archiver` 是 journal 的唯一下游归档者:它读取 persist 线程已落盘并封存(`.seal`)的 journal 段(`seq` 与各 raw journal,名单归 [journal-format](../../docs/subsystems/journal-format.md);可选扩展 `chain-<chain>`),做四件事——把 journal 段原件压缩上传(回放与取证的输入)、把事件按 `gen-schema` 生成的 Arrow schema 转成 Parquet 上传(交易复盘、审计、模拟结果分析的输入)、每 UTC 日写一份 `account_daily` 数据集(每个账户的 PnL 分解、成交率、两腿间隔、裸敞口、保证金占用;Sim 账户标注 `account_kind = sim`)与一份 `strategy_daily` 数据集(每个策略实例的策略 Note 计数与策略专项报表字段,按发布制品中策略 schema JSON 的 `notes`/`report` 描述符通用汇总,不依赖任何策略包)、每小时写一份 manifest 并在 S3 上逐对象核验后才允许本地保留策略删除冷副本。另有两项附带职责:每个 UTC 日把每个策略实例在 00:00 之后写出的第一份 Model 检查点归档(每日 `--advice recompute` 的起点,[Model 检查点每日归档](#model-检查点每日归档));以 `kind = files` 的 source 把服务器上 `hft-verify` 的运行记录、账本快照与崩溃输入原样上传到 `hft/verify/<host>/`([文件归档](#文件归档files-source));每个实例的持久元数据 `DurableMeta` 每次写盘后上传,供换 AZ 重建时的 [`restore`](#从-s3-恢复restore) 取回。它不读 RAM 热段、不写任何 journal(对冷副本目录只在段旁写 `.uploaded` 标记、经 `cold_retire` 删除已核验的段;`restore` 例外)、不向热进程发命令。

**范围**:它只归档**交易 journal**——新框架为自己交易所需订阅记录的数据,不是研究采集系统。行情研究数据仍由独立的现有 collector(`E:\trading\Trading`,collection-policy 控制、多所 × 150+ 合约、`book_ticker.v2` 等 Parquet 与其 S3 管线)负责,本进程不复刻它的数据集与 schema,也**不要求与 collector 的 Parquet schema 兼容**;两边写在不同的 S3 前缀(本进程:`hft/<host>/<instance>/…`)。研究需要对照时按两者共同的时间戳语义(交易所事件时间 `ex_ts_ns`、接收 UTC `rx_ns`)对齐。上传核验与封存纪律沿用现有 Trading 仓库 `s3-store` 与 Trading AGENTS.md(本仓库之外:`Trading/AGENTS.md`) 的规则(工程做法复用,数据布局不复用)。

## 目录

- [使用本组件](#使用本组件)
  - [输入:什么是"已封存"](#输入什么是已封存)
  - [输出:S3 布局](#输出s3-布局)
  - [Parquet 规则](#parquet-规则)
  - [每小时 manifest](#每小时-manifest)
  - [策略日报](#策略日报)
  - [Model 检查点每日归档](#model-检查点每日归档)
  - [文件归档(files source)](#文件归档files-source)
  - [上传与删除纪律](#上传与删除纪律)
  - [从 S3 恢复(restore)](#从-s3-恢复restore)
  - [配置文件字段](#配置文件字段)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
journal-archiver run   --config /etc/hft/journal-archiver.toml          # hft-journal-archiver.service,常驻
journal-archiver check --config … [--hour 2026-10-01T03] [--prefix hft-testnet]   # 只核验:本地段 ↔ manifest ↔ S3 对象;列出未提交与 quarantine 的小时
journal-archiver backfill --config … --from 2026-10-01T00 --to …T06    # 重做指定小时(已提交的对象只核验,不覆盖)
journal-archiver restore --config … --instance prod [--prefix hft] [--journals seq] [--hours 24] --yes   # 重建主机时从 S3 取回已提交的段与 DurableMeta(root,hft-core 启动前)
journal-archiver --check --config …                                     # 只加载与校验配置(前缀重叠、schema 目录摘要),不连 S3
```

运维在命令行运行 `check`、`backfill` 与 `restore` 时同样使用 `hft-archive` 的 S3 凭证(凭证文件位置与授权归 [deploy](../../deploy/README.md#秘密与实例角色)),不用实例角色。退出码:`check` 0 = 所查小时全部已提交且核验一致,1 = 有未提交、quarantine 或核验不符的小时(逐项列出),2 = 配置错误;`restore` 见[从 S3 恢复](#从-s3-恢复restore),不带 `--yes` 时不写任何文件、退出 2;其余子命令配置错误退出 2。

一个进程服务同一主机上的全部实例与 journal(dev2:只含 Sim 账户的 `sim` 实例,以及联调窗口内指向测试网的 `prod` 角色实例;升级主机:`prod` 与 `sim` 同机;可选扩展 `chain-core`),每个 (instance, journal) 一个工作单元,串行处理自己的小时;另有每日检查点与 files source 的工作单元。对象键带主机段:`<prefix>/<host>/<instance>/…`(`<host>` 是本机主机清单的逻辑主机名,[deploy 身份、路径与 S3 前缀](../../deploy/README.md#身份路径与-s3-前缀)),所以 dev2 与升级主机上同名的实例写不同的键。`sim` 实例归档到 `hft/<host>/sim/…`,与 `hft/<host>/prod/…` 分开。**测试网数据不进正式前缀**:dev2 联调窗口的 `prod` 角色实例与升级主机在 M7(测试网账户)期间的 `prod` 实例,都在 `sources` 中设 `prefix = "hft-testnet"`(对象键 `hft-testnet/<host>/prod/…`;M7 → M8 切换前用 `journal-archiver check --prefix hft-testnet` 确认测试网小时全部提交、该前缀下没有 quarantine 小时,步骤归 [launch](../../docs/runbooks/launch.md));加载时某 source 的 refdata 账户全部为非生产账户(`is_prod = false`)而目标前缀是 `<s3.prefix>/<host>/prod` → 退出 2(refdata 取该 source 冷副本中最新一个已封存 `seq` 段引导块里的 refdata `ConfigBlob`;还没有已封存段时不做这项检查)。各 source 的前缀第一段必须是 deploy S3 前缀表登记给 `hft-archive` 的 `hft` 或 `hft-testnet`(collector 的前缀不在其中,因此两者不重叠),同一 (instance, journal) 只能出现在一个 source 中,否则退出 2。游标、提交证明与 quarantine 都按 `(prefix, instance, journal)` 分开存放:source 的 `prefix` 一变(如 M7 → M8 从 `hft-testnet` 改回 `hft`),该单元就从新前缀下的空游标起步,从冷副本目录里第一个未提交的封存段开始,不沿用测试网 journal 的 `last_seq`。

### 输入:什么是"已封存"

"已封存"的定义归 [journal-format](../../docs/subsystems/journal-format.md#冷副本与热段):persist 在热段 `Sealed` 且全部同步后把冷副本截断到 `committed_len`、`fsync` 文件与目录,再写同名 `.seal`(`SealRecord`:`segment_no`、`first_seq`、`last_seq`、`len`、`sha256`、CRC)。archiver 只处理满足以下全部条件的冷副本段 `<cold_root>/<instance>/<journal>/<yyyymmdd>/<segment_no:010>-<first_seq:020>.jrn`(对应现有 WAL 的"trailer + 目录 fsync 才算封存"规则):

1. 同名 `.seal` 存在,CRC 正确,文件长度 = `SealRecord.len`;
2. 重新计算的 SHA-256(`3072..3200` 置零,规则同 journal-format)= `SealRecord.sha256`;
3. 段链:`hft_journal::verify` 对该段与前一段通过(`prev_header_sha256`、`first_seq` 连续;`RECOVERY_GAP` 记为 manifest 的 `gaps`,不算损坏);链断裂 → 该 journal 停止推进,P0,人工处理;
4. 本段最后一条是 `SegmentRoll{End}`(journal-format 校验第 5 条)。

- **未封存与损坏**:`.seal` 不存在,或长度不是 128 B、CRC 不符 → 视为尚未封存,下一次扫描重查;`.seal` 有效而段文件长度或 SHA-256 与之不符,或最后一条不是 `End` → 该段损坏,所在小时进入 quarantine(P1),该 journal 后续小时继续。
- **链断裂与缺段**:前一段在本地而 `prev_header_sha256` 或 `first_seq` 接不上 → 链断裂(上文第 3 条);前一段不在本地、且本单元游标的 `last_seq` ≠ 本段 `first_seq − 1`(游标为空而本段 `segment_no = 1` 除外)→ 缺失的 `segment_no`,记入 manifest 的 `gaps` 后继续;前一段不在本地而游标正好接上(已提交后按保留期删除)→ 不是缺口。
- **小时**:段所属的小时 = 段头 `created_utc_ns` 的 UTC 小时,该段的全部记录(含资金费窗口推迟整点滚动而落在整点之后的记录)进该小时的分区与 manifest。小时 H 的单元在 UTC 时间 ≥ H + 1 h、且该 journal 中属于 H 的段都有有效的 `.seal` 时处理(段损坏按"未封存与损坏"一条 quarantine)(实例在 H 内停机而之后没有新段时同样处理);同一 journal 的小时按时间顺序处理。

journal 名与种类只取自 [journal-format](../../docs/subsystems/journal-format.md)(含 BBO 拆成独立任务时的 `raw-feed-<venue>-bbo`;可选扩展 `chain-<chain>`)。`seq` 与 `chain-*` 按 UTC 整点滚动(段大小按主机类见 [hft-core 内存表](../hft-core/README.md#内存与-journal-段按主机类唯一归属),小段主机类一小时可能多段),"一小时 ≈ 一段";raw journal 或安全阀提前滚动时一小时可能有多段,manifest 按小时汇总。进程崩溃后的撕裂尾只存在于未封存段,archiver 不会读到。

### 输出:S3 布局

桶与区域沿用现有:`gwx-trading-data`、`ap-northeast-1`,但前缀独立:全部对象在 `<prefix>/<host>/<instance>/` 下(`<prefix>` = `s3.prefix`(`hft`)或该 source 的 `prefix`(`hft-testnet`)),与 collector 的前缀不重叠;`…/<instance>/private/*` 用 SSE-KMS(`private_kms_key_id`),`…/<instance>/public/*` 用桶默认加密。前缀表(写者、读者与消费者凭证覆盖的前缀)归 [deploy](../../deploy/README.md#身份路径与-s3-前缀),对象布局归本节。

| 对象 | 键 |
|---|---|
| journal 原件 | `<prefix>/<host>/<instance>/{domain}/journal/journal=<j>/date=YYYY-MM-DD/hour=HH/<segment_no:010>-<first_seq:020>.jrn.zst`(同目录另传 `.seal` 原件) |
| Parquet | `<prefix>/<host>/<instance>/{domain}/<dataset>/exchange=<e>/symbol=<s>/date=YYYY-MM-DD/hour=HH/<journal>-<first_seq>-<part:03>.parquet` |
| 每小时 manifest | `<prefix>/<host>/<instance>/{domain}/manifests/journal=<j>/date=YYYY-MM-DD/hour=HH/manifest.json` |
| 配置 blob | `<prefix>/<host>/<instance>/private/config/<kind>/<sha256>.bin`(`ConfigBlob` 内容,按哈希去重;`kind ∈ refdata\|strategy_params\|risk_params\|engine`) |
| 账户日报 | `<prefix>/<host>/<instance>/private/account_daily/date=YYYY-MM-DD/account_daily.parquet`(每账户一行 + 每 (账户, group) 一行) |
| 策略日报 | `<prefix>/<host>/<instance>/private/strategy_daily/date=YYYY-MM-DD/strategy_daily.parquet`(长表,列见[策略日报](#策略日报)) |
| refdata 目录 | `<prefix>/<host>/<instance>/private/catalogs/<refdata_version>.json`(可读索引) |
| 每日 Model 检查点 | `<prefix>/<host>/<instance>/private/advisor/<strategy_instance>/<YYYY-MM-DD>/<strategy_instance>.ckpt`(文件名与本机检查点相同,`replayer verify --model-checkpoint` 可直接以该日前缀为参数;`last_seq` 在文件头)([Model 检查点每日归档](#model-检查点每日归档)) |
| 持久元数据 | `<prefix>/<host>/<instance>/private/meta/<键>.<yyyymmddThhmmssZ>.<SHA-256 前 12 位>`(`DurableMeta` 的每个键文件,每次内容变化上传一个新对象,时刻取该版本键文件的 mtime(UTC,到秒),从不覆盖;[从 S3 恢复](#从-s3-恢复restore)) |
| 文件归档(files source) | 不可变文件 `hft/verify/<host>/<相对路径>`;快照文件 `hft/verify/<host>/<相对目录>/<主文件名>.<UTC>.<SHA-256 前 12 位><扩展名>`([文件归档](#文件归档files-source)) |

- **恢复代**:本机经 [`restore`](#从-s3-恢复restore) 重建后,该 (prefix, host, instance) 的恢复代 `g` ≥ 1;`run` 此后写出的每个对象(按内容哈希去重的配置 blob 与文件名已带时刻和哈希的持久元数据除外)在文件名的扩展名前加 `.r<g>`(如 `…-<first_seq:020>.r1.jrn.zst`、`manifest.r1.json`、`account_daily.r1.parquet`),manifest 记 `restore_gen`。`g = 0`(从未恢复)时文件名不带该段。
- `domain`:`public` = 行情类事件(`0x01xx`)与 `raw-feed-*`(含 `raw-feed-<venue>-bbo`);`private` = 执行、链上、控制、系统事件、`raw-gw-<venue>`(下单与回报,含账户数据)与 `chain-*`。journal 原件、`.seal` 与 manifest 的 domain 按 journal 定:`raw-feed-*` 为 `public`,其余(含 `seq`)为 `private`;Parquet 按 dataset 的事件 kind 定。
- 段旁的上传标记与段同名:`<segment_no:010>-<first_seq:020>.uploaded`。
- `exchange`:venue 名(`binance-usdm`、`binance-spot`、`okx-swap`、`okx-spot`、`bitget-usdt-futures`、`bitget-spot`;可选扩展 venue 与链名同理);与 venue 无关的事件为 `_sys`。
- `private` 域的执行类 dataset 带 `account` 与 `account_kind`(`live`/`sim`,取自该时刻 refdata 的 `AccountMeta.kind`)两列;Sim 账户的回报与实盘回报在同一 dataset,按该列区分,研究侧不得混算([hft-telemetry 数据集规则](../../crates/hft-telemetry/README.md))。
- `symbol`:refdata 的规范符号;与 inst 无关的事件为 `_all`;低量 symbol 合并为 `_small`(见 Parquet 规则)。

### Parquet 规则

- **schema**:每个 dataset 的 Arrow schema 由 `cargo xtask gen-schema` 从 [hft-event](../../crates/hft-event/README.md) 生成,archiver 不手写列;列名带单位后缀(`*_ns`、`*_tsc`、`px`/`qty` 为定点整数 + 同文件 KV 元数据 `px_exp`/`qty_exp` 按 asset 给出)。时间列:`ex_ts_ns`、`ex_send_ns`、`hw_rx_ns`、`rx_ns`(由 `rx_tsc` 经段头校准换算)、`proc_ns`(由记录头 `tsc` 换算),另保留原始 `seq`、`rx_tsc`、`tsc`([08 §4](../../design/08-数据面与回放.md))。
- **dataset ↔ 事件**:一个 kind 一个 dataset(`bbo`、`l2_batch`、`l2_snapshot`、`trade`、`funding`、`mark_price`、`order_decision`、`exec_decision`、`send_report`、`order_event`、`external_fill`、`margin_report`、`account_config_report`、`funding_settle`、`cmd`、`cmd_result`、`reconcile_report`、`conn_state`、`advice`(`payload` 为 binary 列,附 `strategy`、`local_kind`)、`latency_sample`、`note`、`host_profile`、`clock_status`…;可选扩展 `pool_quote`、`tx_lifecycle`),映射表由 `gen-schema` 输出;`Timer`、`StateSnapshot`、`ConfigBlob` 不进 Parquet(快照与 blob 只在 journal 原件与 `config/` 里)。raw journal 进 `raw_rx`/`raw_tx` dataset(`payload` 为 binary 列)。
- **压缩**:zstd level 3(与现有 `parquet-store` 一致),页级统计开。
- **row group**:目标 1 000 000 行或 128 MiB 先到者;每文件每 row group ≥ 100 000 行,小时末的最后一个 row group 除外([08 §4](../../design/08-数据面与回放.md),row group < 5 千行查询慢 5–10×)。
- **文件**:目标 100 MB–10 GB;超过 10 GB 切 `part`。一个 (dataset, exchange, symbol, hour) 在该小时行数 < `small_symbol_rows`(100 000)时并入 `symbol=_small` 文件,文件内按 `(symbol, seq)` 排序,row group 统计用于裁剪。
- **排序**:文件内按 `(inst, seq)`(行情)或 `seq`(其他)排序;`seq` 是回放顺序的唯一依据。
- **KV 元数据**:`hft.schema`(dataset + schema_version)、`hft.instance`、`hft.journal`、`hft.first_seq`、`hft.last_seq`、`hft.release`(版本 + git sha)、`hft.refdata_version`、`hft.host_profile_hash`;`created_by = hft-journal-archiver/<ver>`。
- 本地先写 `<staging>/<key>.new`(`create_new`)→ `fsync` → rename,记录 bytes 与 SHA-256;小时单元开始时先删除自己暂存目录中残留的 `.new`(上次崩溃留下)。

### 每小时 manifest

未压缩 JSON,字段顺序固定、`sort_keys`;manifest 自身的 SHA-256 按规范化 JSON 计算并写入本地提交证明。它满足 Trading AGENTS.md(本仓库之外:`Trading/AGENTS.md`) 的封存要求(版本/Git SHA、配置与目录哈希、时间窗、时钟状态、记录/字节数、逐文件 SHA-256)。

```json
{
  "schema_version": 1,
  "instance": "prod", "journal": "seq", "host": "tokyo-cex-1", "restore_gen": 0,
  "hour_utc": "2026-10-01T03:00:00Z",
  "release": {"version": "0.1.0", "git_sha": "…"},
  "host_profile_hash": "…",
  "config": {"refdata": "<sha256>", "strategy_params": ["<sha256>", …], "risk_params": ["<sha256>"]},
  "clock": {"synced_all_hour": true, "max_abs_offset_us": 31, "max_error_bound_us": 38},
  "segments": [{"segment_no": 0, "first_seq": 0, "last_seq": 0, "records": 0, "bytes": 0,
                "seal_sha256": "…", "prev_header_sha256": "…", "recovery_gap": false,
                "journal_key": "hft/tokyo-cex-1/prod/…/<segment_no>-<first_seq>.jrn.zst", "zst_bytes": 0, "zst_sha256": "…"}],
  "records_by_kind": {"Bbo": 0, "OrderEvent": 0, …},
  "objects": [{"key": "…", "dataset": "bbo", "exchange": "binance-usdm", "symbol": "PEPEUSDT",
               "rows": 0, "bytes": 0, "sha256": "…", "first_seq": 0, "last_seq": 0,
               "first_ts_ns": 0, "last_ts_ns": 0, "schema_version": 1}],
  "gaps": []
}
```

- `records_by_kind` 之和 = 各段 `records` 之和;Parquet 行数之和 = 进入 Parquet 的 kind 的记录数之和(`L2Delta` 等多记录事件按记录计),不等即不提交。
- `gaps`:本小时内 `RECOVERY_GAP`(整机掉电丢失 persist 滞后部分)或缺失的 `segment_no`,非空 → P1,并在研究侧标记该时段不完整。

### 策略日报

策略专项报表**不按策略写代码**:archiver 启动时用 [hft-telemetry](../../crates/hft-telemetry/README.md) 的 `StrategyCatalog::load` 读发布归档 `schemas/strategies/` 下的策略 schema JSON(`index.json` + 每策略 `<kind_id>-<name>.schema.json`,其中 `notes`、`report` 是 SDK `StrategyDesc.notes`/`report` 的导出,字段定义归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md#策略描述符note报表策略命令)),再从 journal 的 `StrategyParams` 头建立 `strategy → kind_id` 表。`strategy_daily` 是固定 schema 的长表,新增策略不改 schema、不改本 app:

| 列 | 类型 | 含义 |
|---|---|---|
| `date` | date | UTC 日 |
| `instance` / `account` / `account_kind` | string / u8 / string | 实例(`prod`/`sim`)、账户、`live`/`sim` |
| `strategy` / `kind_id` / `strategy_name` | u8 / u16 / string | 策略实例与种类(名取自 schema) |
| `group` | u32,可空 | `scope = Group` 的报表字段与 Note 计数按 group 分行;实例级为空 |
| `item` | string | `note` \| `report` \| `unregistered` |
| `name` / `code` / `level` | string / u32 / string | `note` 行:描述符的名、码、级别;`report` 行:字段名(`code` 为来源 Note 码) |
| `unit` / `agg` | string / string | `report` 行:单位与聚合方式(`Count`/`Sum`/`Last`/`Max`/`P99`) |
| `value` | i64 | `note` 行 = 当日条数;`report` 行 = 按聚合方式得到的原始定点值(按 `unit` 解释) |
| `samples` | u64 | 参与聚合的 Note 条数 |

KV 元数据另记 `hft.strategy_schema_index_sha256`(所用 `index.json` 的 SHA-256),便于离线用同一版本描述符重算。

### Model 检查点每日归档

每日 `replayer verify --advice recompute` 从这里取起点([testing §3.5](../../docs/testing.md#35-验证回放))。对设了 `advisor_dir` 的 `seq` source(默认 `<cold_root>/<instance>/advisor`,即 hft-core 的 `advisor.checkpoint_dir`;文件格式与命名归 [hft-core 检查点表](../hft-core/README.md#内存与-journal-段按主机类唯一归属)):

1. 监听该目录(`inotify` 的 rename,外加每 60 s 兜底扫描)。对每个策略实例,UTC 日 D 内第一次看到写出时刻(文件 `mtime`,UTC)≥ D 00:00 的 `<strategy_instance>.ckpt` 时立即复制一份;复制前校验文件头的 magic、体长与 CRC,不符 → 等下一份。
2. 本地副本写到 `paths.advisor_daily/<instance>/<D>/<strategy_instance>.ckpt`(先写 `.new` 再 rename;`hft-replay-verify` 经 `hft-journal-ro` 组只读,目录属主、模式与上层目录的穿越权限归 [deploy 路径表](../../deploy/README.md#身份路径与-s3-前缀)),保留 `retention.local_days.seq` 天。
3. 上传到上表的键(上传纪律同 journal,SSE-KMS),记入该实例 D 日最早一个尚未提交的 `seq` manifest 的 `objects`(`dataset = "advisor_ckpt"`)。
4. 同一 (实例, 策略实例, D) 已有归档 → 不覆盖(第一份为准)。D 日没有任何检查点(实例当天没运行或检查点写入失败)→ 不归档;当日 recompute 跳过(testing §3.5,watchtower P2)。

### 文件归档(files source)

`sources` 中 `kind = "files"` 的条目把一个本地目录里的文件原样上传(不转 Parquet、不写 manifest)。用途:服务器上 `hft-verify` 的运行记录、账本快照与崩溃输入(目录布局与命名归 [tools](../../tools/README.md#hft-verify);语料可再生,不上传),账本重建与门槛 ③④ 的计数依赖这份备份([testing §6.1](../../docs/testing.md#61-服务器上的累计运行))。两类文件:

- **对象键带主机段**:files source 的全部对象在 `<prefix>/<host>/` 下(`prefix` 缺省 `hft/verify`,`<host>` 同 journal 对象键),所以各主机分别产出的崩溃输入、最小化结果与复核结论不会互相撞键。
- **不可变文件**(`include` 匹配,如 `runs/**`、`crashes/**`、`resolutions/**`):对象键 = `<prefix>/<host>/<相对路径>`。**同键不同内容 → `Conflict`**(quarantine + P1),所以生产方必须以新文件名写新内容;会被改写的进度标记以隐藏文件写出(如 `hft-dst` 的进行中种子标记 `.inflight.json`,[hft-testkit](../../crates/hft-testkit/README.md#8-hft-dst-运行器)),不上传。
- **快照文件**(`snapshot` 匹配,如 `ledger/ledger.json`):内容每次变化并稳定后上传一个新对象 `<prefix>/<host>/<相对目录>/<主文件名>.<yyyymmddThhmmssZ>.<SHA-256 前 12 位><扩展名>`(时刻取该版本文件的 mtime,UTC,到秒),从不覆盖;本地状态记最近一次上传的 SHA-256,内容不变不重传。
- 共同规则:`*.new`、`*.tmp` 与隐藏文件不上传;文件最后一次修改后 `settle_s`(默认 60)内不变才上传(生产方以 rename 原子写出);上传纪律同 journal(上传前重算 SHA-256,`HEAD` 后 `If-None-Match: *` + `x-amz-checksum-sha256`,再 `HEAD` 核验);每个上传对象追加一行审计到 `<state>/audit.jsonl`;本地状态 `<state>/files/<name>.json` 记已上传的 (相对路径, SHA-256),重启后只核验不重传。
- files source 不删除本地文件(保留期由生产方管理);加载时检查其 `<prefix>/<host>` 不与任何 journal source 的 `<prefix>/<host>/<instance>` 重叠,重叠 → 退出 2。

### 上传与删除纪律

沿用现有 `s3-store` 的 `ensure_file` 语义与顺序(Trading `crates/s3-store`、`apps/uploader`):

1. 上传前重新计算每个本地文件的 SHA-256。
2. 对每个对象:`HEAD`(带 ChecksumMode)→ 存在则 bytes 与 SHA-256 必须一致(`AlreadyPresent`),否则 `Conflict`(不可重试,进入 quarantine,P1);不存在则 `PUT` 带 `If-None-Match: *` 与 `x-amz-checksum-sha256`,之后再 `HEAD` 核验。
3. 顺序:journal 原件 → Parquet → 配置 blob → manifest 最后。
4. 本地写提交证明 `<state>/committed/<prefix>/<instance>/<journal>/<hour>.json` = `{schema_version, bucket, manifest_key, manifest_sha256, committed_utc_ns}`,并追加审计行 `<state>/audit.jsonl`(永不删除)。
5. 重试:每小时单元 8 次,30 s 起翻倍,上限 30 min;8 次仍失败 → P1,之后按上限间隔继续重试,不进入 quarantine(网络与 S3 的暂时故障不需要人工);可重试错误之外(Conflict、校验和不符、行数不等)进入 `<state>/quarantine/<prefix>/<instance>/<journal>/<hour>/reason.json`,该 journal 后续小时继续处理,quarantine 小时 P1 告警并等待人工。
6. 提交成功后在段旁写 `.uploaded`(journal-format 规定的标记,内容 = manifest 键与其 SHA-256)。本单元对冷副本目录 `<cold_root>/<instance>/<journal>/` 只有这两种写:写 `.uploaded` 与下述删除;写的途径(组或 ACL)与单元的 `ReadWritePaths=<cold_root>` 归 [deploy 路径表](../../deploy/README.md#身份路径与-s3-前缀)。**删除本地冷副本**只由 `retention` 子任务调用 `hft_journal::cold_retire` 执行:段年龄超过保留期(`seq`/`chain-*` 7 天、raw 3 天,[08 §8](../../design/08-数据面与回放.md))**且**存在 `.uploaded` **且**删除前重新 `HEAD` 核验 manifest 与其中每个对象(key、bytes、SHA-256 全部一致);任一不符则不删并 quarantine。做法:每次 retention 对一个 (instance, journal) 先列出 `cold_retire` 将删除的段(已封存、超过保留期、有 `.uploaded`),对它们所属的每个小时重新 HEAD 核验;全部通过才调用 `cold_retire`,任一小时不符 → 该 journal 本次不调用(不删任何段),不符的小时 quarantine 并 P1。不删除 manifest 审计与提交证明。
7. 磁盘水位:冷副本卷使用率 ≥ 80% 时 retention 以 `keep_days = ⌈retention.min_local_hours / 24⌉`(`min_local_hours` = 24 h 时为 1 天)按第 6 条同样核验后调用 `cold_retire`,提前删除已提交且封存超过该时长的段;一轮之后使用率仍 ≥ 90% → P0(persist 即将写满)。
8. **告警**:本页各处的 P0/P1 都是向 `<state>/alerts.jsonl`(只追加,永不删除)写一行 `{utc_ns, level, kind, prefix, instance, journal, hour, detail}`,并写同级的 `tracing` 事件;投递由 watchtower 的 `ArchiverAlert` 规则完成([watchtower 健康规则](../watchtower/README.md#健康规则))。`kind`:`ChainBreak`(P0,链断裂)、`DiskFull`(P0,水位第 7 条)、`Quarantine`(P1,含 files source 的 `Conflict`)、`Gap`(P1,manifest 的 `gaps` 非空)、`RetryExhausted`(P1,第 5 条)、`Orphans`(P1,`restore` 的未提交对象)、`UnknownKindId`(P1,策略日报遇到目录里没有的 `kind_id`)。

### 从 S3 恢复(restore)

换 AZ 重建主机而冷副本卷摘不下来时,用本子命令从 S3 取回冷副本;前后步骤(新实例、待命网卡、`--recover-from-snapshot` 启动)归 [incident-response "交易主机失联"](../../docs/runbooks/incident-response.md#s-交易主机失联)。

**上传侧**:每个 `seq` source 另监视 `<cold_root>/<instance>/meta/`(`inotify` 的 rename,外加每 60 s 兜底扫描),`DurableMeta` 任一键文件内容变化即上传到上表"持久元数据"的键(不等 `settle_s`;上传纪律同 journal,SSE-KMS;临时文件 `<key>.tmp` 不上传)。

**`restore` 的步骤**(以 root 在新主机上运行,`[需授权]`):

1. **前提**:`hft-core@<instance>` 与本单元都未运行,`<cold_root>/<instance>/` 不存在或为空,本地状态里没有该 (prefix, instance) 的游标;否则退出 2,不覆盖任何文件。S3 读用 `hft-archive` 的凭证(读权限与 `private` 前缀的 KMS 解密授权归 [deploy](../../deploy/README.md#秘密与实例角色))。
2. **取回段**:对每个所选 journal(缺省只有 `seq`;raw journal 只用于取证,按需以 `--journals` 加入),按已提交的 manifest 取最后 `--hours`(缺省 24)个已提交小时的段:下载 `.jrn.zst` 与 `.seal`,解压后重算的 SHA-256(`3072..3200` 置零)须等于 manifest 的 `seal_sha256` 与 `.seal`,`hft_journal::verify` 对取回的段链通过(第一段的前驱不在本地,与按保留期删除后相同),才写进 `<cold_root>/<instance>/<journal>/<yyyymmdd>/`(先写 `.new` 再 rename;属主与模式按 deploy 路径表);任一不符 → 停止并退出 1,已写入的文件留给人检查。未提交的对象(故障主机已部分上传、manifest 未写)不取。
3. **本地状态**:每个取回的段旁写 `.uploaded`,并按取回的 manifest 写提交证明、游标(`last_committed_hour` = 最后取回的小时)与审计行;`run` 起来后不重传这些小时。
4. **epoch 安全间隔**:取 S3 上该实例各份 `engine_epoch` 的最大值(对象内容按 `DurableMeta` 的文件格式解析,任一份不可解析 → 退出 1)与取回 journal 中最新快照的 `engine_epoch` 的较大者,加 `restore.epoch_gap`,以 `DurableMeta::write_sync("engine_epoch", …)` 写进新的 `<cold_root>/<instance>/meta/`(毒丸计数不恢复,从 0 起)。hft-core 启动时取 `max(快照, DurableMeta) + 1`(规则归 [hft-core](../hft-core/README.md#启动顺序)),所以只要故障主机在最后一次上传 `engine_epoch` 之后的启动次数少于 `epoch_gap`,新主机就不会复用它用过的 epoch 与 client order id。
5. **恢复代**:恢复代 `g` = 该 (prefix, host, instance) 下已有 manifest 中最大的 `restore_gen` + 1,写进本地状态;之后 `run` 写出的对象按[恢复代](#输出s3-布局)规则带 `.r<g>`,新主机续写的段号与 seq 不会和故障主机已部分上传的对象撞键。`restore` 把这些未提交的对象列进 `<state>/restore/<g>/orphans.json` 并 P1;是否删除由人决定(`[需授权]`)。
6. **输出**:取回的小时范围、段数、最新快照的 seq 与 `engine_epoch`、写入的 `engine_epoch` 与恢复代;退出码 0 成功、1 校验失败或 S3 错误、2 前提不满足。

### 配置文件字段

`/etc/hft/journal-archiver.toml`,`schema = 1`。

| 键 | 类型 | 含义 | 默认 / 来源 |
|---|---|---|---|
| `sources` | [`{kind?, instance, cold_root, journals: [string], prefix?, advisor_dir?}` \| `{kind = "files", name, dir, include: [glob], snapshot: [glob], prefix?, settle_s?}`] | `kind` 缺省 `journal`:要归档的实例与 journal 名(`seq`、`raw-feed-*`、`raw-gw-*`;可选扩展 `chain-*`),`prefix` 覆盖该 source 的顶层前缀(测试网账户的实例用 `hft-testnet`),`advisor_dir` 开启[每日检查点归档](#model-检查点每日归档);`kind = "files"`:见[文件归档](#文件归档files-source) | 必填(journal 的 `prefix` 缺省 = `s3.prefix`;files 的 `prefix` 缺省 `hft/verify`,`settle_s` 缺省 60) |
| `s3.bucket` / `s3.region` | string | `gwx-trading-data` / `ap-northeast-1` | 必填 |
| `s3.private_kms_key_id` | string | `<prefix>/<host>/<instance>/private` 的 SSE-KMS key | 必填 |
| `s3.prefix` | string | `hft`(对象键 = `<prefix>/<host>/<instance>/…`;它与各 source 的 `prefix` 的取值规则见[使用本组件](#使用本组件),加载时检查) | 必填 |
| `paths.staging` / `paths.state` | path | 本地暂存与状态(与冷副本同卷,rename 原子) | 必填 |
| `parquet.zstd_level` | i32 | | 3 |
| `parquet.row_group_rows` / `parquet.row_group_bytes` | u32 / u64 | | 1 000 000 / 128 MiB(`dev2` 为 16 MiB,hft-core dev2 内存表按此计) |
| `parquet.min_row_group_rows` | u32 | | 100 000 |
| `parquet.small_symbol_rows` | u32 | 低于此行数的 symbol 并入 `_small` | 100 000 |
| `parquet.max_file_bytes` | u64 | | 10 GiB |
| `retention.local_days` | {journal 类 → u16} | 本地冷副本保留(传给 `cold_retire`) | `seq` 7、`raw-*` 3、`chain-*` 7;`dev2`(冷盘剩余约 119 GB,[现状测量 §2.1](../../design/现状测量与升级分析-2026-10-01.md))按实测日量调低,待标定 |
| `account_daily.enabled` | bool | 是否写账户日报数据集 | true |
| `strategy_daily.enabled` | bool | 是否写策略日报数据集 | true |
| `strategy_schemas` | path | 策略 schema 目录(发布归档的 `schemas/strategies/`);`index.json` 摘要不符 → 退出 2 | 必填 |
| `retention.min_local_hours` | u16 | 水位删除的下限 | 24 |
| `upload.max_attempts` / `upload.backoff` | u8 / `{base_s, max_s}` | | 8 / `{30, 1800}` |
| `threads.convert` | u8 | Parquet 转换并行度 | 2;`dev2`/`c4` 为 1 |
| `host` | string | 本机主机清单的逻辑主机名:journal、持久元数据与 files source 对象键的主机段([deploy](../../deploy/README.md#身份路径与-s3-前缀)) | 必填(deploy 生成) |
| `paths.advisor_daily` | path | 每日检查点的本地副本(属主与模式归 deploy 路径表) | `/var/lib/hft/archiver/advisor-daily` |
| `restore.epoch_gap` | u16 | [`restore`](#从-s3-恢复restore) 写入 `engine_epoch` 时加的安全间隔 | 64 |

## 实现说明

<details><summary>实现要点</summary>

**一个 (instance, journal, hour) 单元的步骤**

1. 等封存条件(上文 1–4),`inotify` 监听冷副本目录 + 每 30 s 兜底扫描。
2. 重算段文件 SHA-256 并与 `.seal` 比对;`hft_journal::verify` 校验段链。
3. 流式压缩段原件为 `.jrn.zst`(zstd level 9,多线程)并计算其 SHA-256。
4. 用 `hft-journal` 的 reader 顺序读记录,按 kind 分派到 dataset 写者;每个 (dataset, exchange, symbol) 一个累积缓冲,达到 row group 阈值即刷出;小时末按 `small_symbol_rows` 决定去向后关闭文件。
5. 对 `ConfigBlob` 记录:按内容 SHA-256 写 `config/` 对象(已存在则只核验)。
6. 核对计数(manifest 的两条等式);生成 manifest。
7. 上传(上文纪律),写提交证明与审计行。
8. 更新本单元游标 `<state>/cursor/<prefix>/<instance>/<journal>.json = {last_committed_hour, last_seq}`(先写 `.new` 再 rename)。

**账户日报单元**:某 UTC 日最后一小时的 `seq` 单元提交后,从该日 `seq` 冷副本用 [hft-telemetry `account_report`](../../crates/hft-telemetry/README.md#账户报告account_report)(与 watchtower 日报同一函数)重算每个账户与每 (账户, group) 的 `AccountDaily`,并用 `strategy_report`(与 watchtower 日报 `strategies` 节同一函数)按描述符汇总策略 Note,分别写 `account_daily`、`strategy_daily` Parquet,随次日第一个 manifest 上传(记入 `objects`)。journal 中出现 schema 目录里没有的 `kind_id` → 该实例的 Note 全部以 `item = unregistered` 计数并 P1,不阻塞归档。

- 转换在 `threads.convert` 个工作线程上并行(不同 journal 并行,同一 journal 的小时串行,保证游标单调)。
- 压缩与 Parquet 编码是 CPU 密集,进程在 `hftbatch.slice` 内、只用线程计划之外的核(永不占 `prod` 的热核),CPU 权重、`Nice` 与核集合按主机类取 [hft-core 线程计划](../hft-core/README.md#线程计划)与[主机资源隔离](../hft-core/README.md#主机资源隔离slice核与出口-ip)(dev2 护栏一级动作会停它;护栏锁存期间不启动)。
- `hft-replay-verify` 运行期间本单元暂停(停止与恢复由单元设置完成,归 deploy):中途停止只丢失未完成的小时单元进度,重启后已上传的对象只核验不重传,游标不倒退。
- 读冷副本用 `posix_fadvise(SEQUENTIAL)` + `DONTNEED`,不污染页缓存(同卷上有 persist 的写)。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/main.rs` | 子命令 `run`/`check`/`backfill`/`restore` |
| `src/config.rs` | TOML 与校验 |
| `src/seal.rs` | 封存条件判定、段链校验 |
| `src/convert.rs` | journal → dataset 分派与 Parquet 写者装配(schema 来自生成物) |
| `src/manifest.rs` | manifest 构造与计数核对 |
| `src/upload.rs` | 上传与提交证明 |
| `src/s3_vendored.rs` | 从 Trading `crates/s3-store` **vendoring** 的少数函数(`ensure_file`、HEAD 校验和核验、提交证明的规范化 JSON 与 SHA-256),源文件头注明来源 commit;不跨仓库依赖 |
| `src/retention.rs` | 核验后删除、水位策略 |
| `src/restore.rs` | `DurableMeta` 上传;`restore` 的取回、核验、`engine_epoch` 安全间隔、恢复代与未提交对象清单 |

</details>

## 延迟与资源契约

冷进程,无热路径约束;约束是**跟得上**与**不打扰热核**。

| 指标 | 目标 | 测量 |
|---|---|---|
| 小时结束 → 该小时 manifest 提交 | seq ≤ 15 min,raw/chain ≤ 30 min | 审计行时间戳 − `hour_utc` − 1 h;watchtower 告警阈值 45 min |
| 转换吞吐 | ≥ 50 MB/s journal 输入 / 工作线程(一天约 30 GB seq 在约 10 min CPU 时间内完成;日量参照[现状测量 §3.5](../../design/现状测量与升级分析-2026-10-01.md),按目标订阅重算待实测) | 基准 `archiver_convert_seq_1h`(用 fixtures 的 1 h 样本) |
| CPU / 内存 | CPU 权重、调度策略与核集合只在 [hft-core 主机资源隔离](../hft-core/README.md#主机资源隔离slice核与出口-ip) 定义,内存上限只在 [hft-core 内存表](../hft-core/README.md#内存与-journal-段按主机类唯一归属) 定义(`prod` 与 `sim` 两个实例的归档合计) | systemd 计量 |
| 每日检查点复制 | 检查点 rename 后 ≤ 2 min 完成本地复制 | 审计行时间戳 |
| 对热进程的影响 | persist 滞后 p99 不因 archiver 运行上升 > 10% | 对照实验:archiver 开/关各 24 h |

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-event`(生成的 schema 与 kind 表)、`hft-journal`(只用 `JournalReader`、`verify`、`cold_retire`,以及 `restore` 写 `engine_epoch` 用的 `DurableMeta`;不启用 `writer`)、`hft-refdata`(symbol 名、asset 指数、`AccountMeta.kind`)、`hft-telemetry`(`account_report`、`strategy_report`、`StrategyCatalog`)、`hft-ledger`(经 hft-telemetry 重建权益)。
- 外部:`arrow`、`parquet`(zstd 特性)、`zstd`、`sha2`、`tokio` + AWS SDK S3(冷进程允许)。Trading 仓库 `s3-store` 中需要的几个函数以 vendoring 方式拷进 `src/s3_vendored.rs`(带来源 commit 与原测试),不建跨仓库依赖、不新建 crate。
- 禁止:`JournalWriter`、`hft-ctl` 客户端(archiver 不发命令)、任何 hot crate 的运行时、任何 `strategies/*` 与 `hft-strategy-registry`。
- S3 凭证:来源由 deploy 决定(凭证代理按消费者写出的短期凭证文件,覆盖 `hft/` 与 `hft-testnet/` 两个前缀;哪些主机运行凭证代理归 deploy)。本 app 需要写、上传前的 `HEAD` 与 `restore` 的 Get/List,以及 `private` 前缀的 KMS 加解密,授权集合归 deploy;本 app 只按 AWS SDK 的标准凭证链读取,凭证文件更新后不重启也要用上新凭证(刷新与重读的规则归 [deploy](../../deploy/README.md))。

## 延伸阅读

- [08 §3–4、§8–9](../../design/08-数据面与回放.md)、[research/06](../../design/research/06-data-plane-replay-observability.md)
- [journal-format](../../docs/subsystems/journal-format.md)、[event-model](../../docs/subsystems/event-model.md)
- 现有实现:Trading `crates/parquet-store`(分区与写者)、`crates/s3-store`(键与 `ensure_file`)、`apps/uploader`(重试与 retention)、`docs/runbooks/cold-pipeline.md`
- [python/research](../../python/research/README.md)(消费方)

## 已知限制与后续工作

- 只归档已封存段:当日未封存数据由研究侧经 `hft-py` 直读冷副本或热段(只读),不经 S3。
- 换 AZ 恢复后,故障主机已上传而未提交的对象(`orphans.json` 列出)仍在原键上:按目录列举读取 Parquet 的读者在这些小时会多读一份,以 manifest 列出的对象为准;`restore` 只取回已提交小时,故障主机最后一个提交点之后的 journal 丢失,由 `--recover-from-snapshot` 与严格对账收敛。
- ClickHouse 热查询层(90 天)不在本进程;若上线,由它从 `hft/<host>/<instance>/public` 拉取。
- 本进程不承担研究采集:订阅集合由交易需要决定,不为研究扩大(扩大订阅会增加热线程负载);研究数据集与覆盖范围见现有 collector。
- S3 生命周期(90 天后转 IA)由桶策略配置,不由 archiver 执行;`preflight` 要求生命周期规则不覆盖 `hft/` 的过期删除(沿用现有 uploader preflight)。

### 开发备忘

<details><summary>给实现者</summary>

- 已决:不做跨仓库 git 依赖,也不新建 `hft-archive` crate;`ensure_file`、HEAD 核验与提交证明的函数 vendoring 进本 app,Trading 侧修复需手工同步(源 commit 写在文件头,`journal-archiver check` 的测试用 Trading 的原用例)。
- **待核实**(方法:用模拟账户运行头 7 天的数据统计每 symbol 每小时行数分布):`_small` 合并阈值 100 000 行是否让 Binance 全市场 `bbo` 的大多数 symbol 落入 `_small`;据统计结果调整(这是配置项,不改代码)。
- 现有 WAL 的教训(滚动不带快照导致 84% 段隔离)在新格式中由段首 `StateSnapshot` 解决;archiver 不需要"多种子恢复"逻辑,但 `check` 子命令应能报告某段是否以快照开头。
- 实现顺序:seal + journal 原件上传(先保证可回放)→ manifest → Parquet(先 `bbo`/`trade`/`order_event`)→ retention。

</details>
