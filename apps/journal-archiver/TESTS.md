---
component: journal-archiver
prefix: ARC
spec: README.md
---

# journal-archiver 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。段格式、`cold_retire` 与 `DurableMeta` 本身的行为在 [hft-journal](../../crates/hft-journal/TESTS.md) 验证,`account_report`/`strategy_report` 的计算在 [hft-telemetry](../../crates/hft-telemetry/TESTS.md) 验证;本文件验证本 app 的封存判定、对象键与 Parquet、manifest、上传与删除纪律、每日检查点、files source、`restore` 与配置。用例在 `前置` 中显式给出所用的取值。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | ARC-027、ARC-110、ARC-150、ARC-160 |
| 目录 | 不适用:非规范 |
| 使用本组件 | ARC-001–ARC-015 |
| 输入:什么是"已封存" | ARC-020–ARC-028 |
| 输出:S3 布局 | ARC-030–ARC-035、ARC-109、ARC-165、ARC-166 |
| Parquet 规则 | ARC-040–ARC-053、ARC-166 |
| 每小时 manifest | ARC-022、ARC-060–ARC-063、ARC-146、ARC-165、ARC-168 |
| 策略日报 | ARC-070–ARC-075、ARC-168 |
| Model 检查点每日归档 | ARC-080–ARC-088、ARC-171 |
| 文件归档(files source) | ARC-002、ARC-090–ARC-097 |
| 上传与删除纪律 | ARC-100–ARC-116、ARC-167 |
| 从 S3 恢复(restore) | ARC-035、ARC-120–ARC-132、ARC-169、ARC-170 |
| 配置文件字段 | ARC-001、ARC-002、ARC-046、ARC-140、ARC-147 |
| 实现说明 | ARC-072、ARC-076、ARC-077、ARC-145–ARC-149、ARC-159 |
| 延迟与资源契约 | ARC-155–ARC-159 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | ARC-008、ARC-160、ARC-161、ARC-170 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**约定**(用例只写与这里不同的部分;所写数值都是该用例的显式前置):

- **测试台 `ArcHarness`**(`apps/journal-archiver/tests/common/mod.rs`):在进程内运行 archiver 的工作单元,替换外部面——① 冷副本:临时 `<cold_root>`,段由 hft-testkit 的 journal 构造器按 journal-format 写出(段头 `segment_no`、`first_seq`、`created_utc_ns` 与 `.seal` 由用例给出,可按用例改坏);② S3:进程内假 S3,实现 `HEAD`(带 `ChecksumMode` 时返回 `x-amz-checksum-sha256`)、`PUT`(检查 `If-None-Match: *` 与 `x-amz-checksum-sha256`,记录 SSE 头)、`GET`、`LIST`,按顺序记录每个请求(方法、键、头),可按用例注入 5xx、超时、`412` 与校验和不符;③ 假 UTC 时钟;④ 假 `statvfs`、systemd `ActiveState`,inotify 可关闭(关闭时只靠兜底扫描)。"告警 X" = `<state>/alerts.jsonl` 新增一行 `kind = X` 的记录(字段见 ARC-116)。
- **配置 A0**:`schema = 1`;`host = "tokyo-cex-1"`;`s3 = {bucket = "gwx-trading-data", region = "ap-northeast-1", prefix = "hft", private_kms_key_id = "kms-test"}`;`sources = [{instance = "prod", cold_root = <tmp>, journals = ["seq", "raw-feed-binance-usdm", "raw-gw-binance-usdm"], advisor_dir = <tmp>/prod/advisor}, {instance = "sim", cold_root = <tmp>, journals = ["seq"]}, {kind = "files", name = "verify", dir = <tmp>/verify, include = ["runs/**", "crashes/**", "resolutions/**"], snapshot = ["ledger/ledger.json"], settle_s = 60}]`;`paths.staging`、`paths.state`、`paths.advisor_daily` 在临时根下;`parquet = {zstd_level = 3, row_group_rows = 1000000, row_group_bytes = 134217728, min_row_group_rows = 100000, small_symbol_rows = 100000, max_file_bytes = 10737418240}`;`retention.local_days = {seq = 7, raw-* = 3, chain-* = 7}`;`retention.min_local_hours = 24`;`upload = {max_attempts = 8, backoff = {base_s = 30, max_s = 1800}}`;`threads.convert = 2`;`restore.epoch_gap = 64`;`account_daily.enabled = true`;`strategy_daily.enabled = true`;`strategy_schemas` = S0。
- **策略目录 S0**:hft-testkit 测试策略 `probe-taker`(`kind_id = 65280`)经 `hft-core schemas --out` 导出;`notes`:码 1 `edge`(P1)、码 2 `info_tick`(Info);`report`:`edges`(Count, Instance, 码 1)、`edge_sum`(Sum, Group, 码 1, `v_ix = 0`)、`edge_last`(Last, Instance)、`edge_max`(Max, Group)、`edge_p99`(P99, Instance)。
- **refdata**(各段引导块的 `ConfigBlob`):`prod` 含账户 1(`kind = Live`、`is_prod = true`);`sim` 含账户 11(`kind = Sim`、`is_prod = false`)。
- **时刻与段**:`T0 = 2026-11-02T00:00:00Z`;"H03" = [T0 + 3 h, T0 + 4 h)。**标准小时段**:`prod` `seq` 段 5(`first_seq = 40001`,`created_utc_ns` = 该小时起点 + 0.2 s),引导块之后有 `Bbo` × 1 000(`binance-usdm` BTCUSDT)、`Trade` × 200、`OrderEvent` × 20、`Note` × 50、`Timer` × 30,末条 `SegmentRoll{End}`,`.seal` 有效;其前一段 4 在本地且已提交。键中的段名写作 `0000000005-00000000000000040001`。

### ARC-001 --check 对完整配置退出 0,不连 S3、不写状态

- 规则:[使用本组件](README.md#使用本组件)、[配置文件字段](README.md#配置文件字段)
- 级别:proc
- 里程碑:M5
- 前置:Linux;A0 与 S0 写在临时根下;`strace -f -e trace=connect,openat`
- 步骤:
  1. `journal-archiver --check --config <A0>`
- 期望:
  - 退出码 0;没有 `connect`;`paths.state`、`paths.staging` 下没有新文件;S0 的 `index.json` 被打开(摘要校验)
- 实现:`apps/journal-archiver/tests/proc/cli.rs::arc_001_check_ok`

### ARC-002 加载时的配置错误一次列出并退出 2

- 规则:[使用本组件](README.md#使用本组件)、[配置文件字段](README.md#配置文件字段)、[文件归档(files source)](README.md#文件归档files-source)、[策略日报](README.md#策略日报)
- 级别:table
- 里程碑:M5
- 前置:以 A0 为底,每行只做所列改动后运行 `journal-archiver --check --config <cfg>`
- 实现:`apps/journal-archiver/tests/config.rs::arc_002_load_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| ARC-002.1 | `s3.prefix = "v2"` | 退出 2;错误指明前缀第一段不是 `hft`/`hft-testnet` |
| ARC-002.2 | `prod` source 加 `prefix = "data"` | 退出 2 |
| ARC-002.3 | files source `prefix = "hft"`(`hft/tokyo-cex-1` 与 `hft/tokyo-cex-1/prod` 重叠) | 退出 2;错误指明重叠的两个前缀 |
| ARC-002.4 | files source `prefix = "hft/verify"`(缺省值) | 退出 0 |
| ARC-002.5 | 两个 journal source 都列出 `(prod, seq)` | 退出 2 |
| ARC-002.6 | S0 的 `index.json` 摘要改动一个十六进制位 | 退出 2,错误含 `DigestMismatch` |
| ARC-002.7 | `journals` 含 `raw-foo` | 退出 2(不是 journal-format 登记的名字) |
| ARC-002.8 | 删掉 `s3.private_kms_key_id` | 退出 2 |
| ARC-002.9 | 删掉 `host` | 退出 2 |
| ARC-002.10 | 删掉 `paths.state` | 退出 2 |
| ARC-002.11 | `schema = 2` | 退出 2 |
| ARC-002.12 | 加未知键 `upload.parallel = 4` | 退出 2 |
| ARC-002.13 | 同时做 .1、.5、.9 | 退出 2;stderr 恰好三行错误 |

### ARC-003 check 列出未提交与 quarantine 的小时

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:A0;`prod` `seq` 的 H01、H02 已提交;H03 已封存未提交;H04 在 quarantine;运行 `journal-archiver check --config <A0>`(按行另加参数)
- 实现:`apps/journal-archiver/tests/check.rs::arc_003_check_lists`

| 行 | 参数与条件 | 期望 |
|---|---|---|
| ARC-003.1 | 无 | 退出 1;输出列出 `(hft, prod, seq, 2026-11-02T03)` 未提交、`(hft, prod, seq, 2026-11-02T04)` quarantine;H01、H02 标"通过" |
| ARC-003.2 | `--hour 2026-11-02T02` | 退出 0;只输出 H02 一行 |
| ARC-003.3 | `--hour 2026-11-02T04` | 退出 1 |
| ARC-003.4 | H03 提交、H04 的 quarantine 由人移走并重做提交后 | 退出 0 |
| ARC-003.5 | `--prefix hft-testnet`,该前缀下没有任何提交与 quarantine | 退出 0,输出为空表 |
| ARC-003.6 | 配置中 `s3.prefix = "v2"` | 退出 2,不连 S3 |

### ARC-004 check 逐对象核验本地段、manifest 与 S3

- 规则:[使用本组件](README.md#使用本组件)、[每小时 manifest](README.md#每小时-manifest)
- 级别:table
- 里程碑:M5
- 前置:A0;`prod` `seq` H03 已提交;按行改动后运行 `journal-archiver check --hour 2026-11-02T03`
- 实现:`apps/journal-archiver/tests/check.rs::arc_004_check_verifies_objects`

| 行 | 改动 | 期望 |
|---|---|---|
| ARC-004.1 | 无 | 退出 0 |
| ARC-004.2 | 假 S3 中该小时一个 Parquet 对象的内容被替换(SHA-256 不同) | 退出 1;输出该键、manifest 中的 SHA-256 与 S3 返回的 SHA-256 |
| ARC-004.3 | 假 S3 中 manifest 被删除 | 退出 1;该小时标"manifest 缺失" |
| ARC-004.4 | 本地段 5 的冷副本被改动一个字节(`.uploaded` 仍在) | 退出 1;输出段 5 的 SHA-256 与 manifest `seal_sha256` 不符 |
| ARC-004.5 | check 运行期间 | 假 S3 只收到 `HEAD`、`GET`、`LIST`,没有 `PUT`;本地没有写入 |

### ARC-005 backfill 只核验已提交的对象,补做未提交的小时

- 规则:[使用本组件](README.md#使用本组件)、[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0;`prod` `seq` H00–H02 已提交,H03–H06 已封存而游标停在 H02(上次运行在 H02 后停止)
- 步骤:
  1. `journal-archiver backfill --config <A0> --from 2026-11-02T00 --to 2026-11-02T06`
- 期望:
  - H00–H02 的每个对象只收到 `HEAD`(无 `PUT`);H03–H06 各上传并提交,manifest 最后;游标为 H06
- 实现:`apps/journal-archiver/tests/check.rs::arc_005_backfill`

### ARC-006 backfill 遇到 S3 上同键不同内容时 quarantine,不覆盖

- 规则:[使用本组件](README.md#使用本组件)、[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 未提交;假 S3 中已有 H03 的 journal 原件键,内容与本地压缩结果不同
- 步骤:
  1. `journal-archiver backfill --from 2026-11-02T03 --to 2026-11-02T03`
- 期望:
  - 该键没有 `PUT`;`<state>/quarantine/hft/prod/seq/2026-11-02T03/reason.json` 含 `Conflict` 与该键;告警 `Quarantine`(P1);没有 manifest `PUT`
- 实现:`apps/journal-archiver/tests/check.rs::arc_006_backfill_conflict`

### ARC-007 一个进程服务全部实例与 journal,一个 journal 的 quarantine 不阻塞其他

- 规则:[使用本组件](README.md#使用本组件)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 内 `prod` 的三个 journal 与 `sim` `seq` 都有封存段;`prod` `raw-gw-binance-usdm` 的 H03 段 `.seal` 有效而 SHA-256 不符
- 步骤:
  1. `run`,推进到 T0 + 4 h 15 min
- 期望:
  - `prod` `seq`、`raw-feed-binance-usdm` 与 `sim` `seq` 的 H03 各自提交;`prod` `raw-gw-binance-usdm` 的 H03 进入 quarantine,告警 `Quarantine`;该 journal 的 H04 照常处理
- 实现:`apps/journal-archiver/tests/units.rs::arc_007_independent_units`

### ARC-008 S3 凭证文件更新后不重启即用上新凭证

- 规则:[使用本组件](README.md#使用本组件)、[依赖](README.md#依赖)
- 级别:proc
- 里程碑:M5
- 前置:Linux;本地 S3 兼容服务(MinIO)以两组凭证 K1、K2;`AWS_SHARED_CREDENTIALS_FILE` 指向的文件起初为 K1;archiver `run`
- 步骤:
  1. 上传一小时;把凭证文件原子替换为 K2,并在 S3 服务端吊销 K1;再封存一小时
- 期望:
  - 第二小时的请求都以 K2 签名并成功提交;进程 PID 不变
- 实现:`apps/journal-archiver/tests/proc/creds.rs::arc_008_credential_rotation`

### ARC-010 对象键带主机段,两台主机的同名实例互不冲突

- 规则:[使用本组件](README.md#使用本组件)、[输出:S3 布局](README.md#输出s3-布局)
- 级别:integ
- 里程碑:M5
- 前置:两个 archiver,`host` 分别为 `dev2` 与 `tokyo-cex-1`,共用同一个假 S3;两机各有 `sim` `seq` 段 5(内容不同、段名相同)
- 步骤:
  1. 两机各自 `run` 提交 H03
- 期望:
  - 键分别以 `hft/dev2/sim/` 与 `hft/tokyo-cex-1/sim/` 开头;两边都提交成功,没有 `Conflict`,没有 quarantine
- 实现:`apps/journal-archiver/tests/keys.rs::arc_010_host_segment`

### ARC-011 prod 与 sim 各写各的前缀,manifest 互不引用

- 规则:[使用本组件](README.md#使用本组件)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 内 `prod` 与 `sim` 都有 `seq` 段
- 步骤:
  1. `run` 提交 H03
- 期望:
  - `prod` 的全部对象键以 `hft/tokyo-cex-1/prod/` 开头,`sim` 的以 `hft/tokyo-cex-1/sim/` 开头;两个 manifest 的 `segments[].journal_key` 与 `objects[].key` 都只指向本实例前缀
- 实现:`apps/journal-archiver/tests/keys.rs::arc_011_prod_sim_separate`

### ARC-012 测试网实例的 source 写 hft-testnet 前缀

- 规则:[使用本组件](README.md#使用本组件)
- 级别:integ
- 里程碑:M6
- 前置:A0,`prod` source 另设 `prefix = "hft-testnet"`;`prod` refdata 只有账户 2(`kind = Live`、`is_prod = false`)
- 步骤:
  1. `run` 提交 H03
- 期望:
  - 全部对象键以 `hft-testnet/tokyo-cex-1/prod/` 开头;`hft/tokyo-cex-1/prod/` 下没有任何对象;提交证明在 `<state>/committed/hft-testnet/prod/seq/`
- 实现:`apps/journal-archiver/tests/keys.rs::arc_012_testnet_prefix`

### ARC-013 测试网数据不进正式前缀的加载检查

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:A0;`prod` 冷副本中最新已封存 `seq` 段的引导块 refdata 与 source 的前缀按行;运行 `journal-archiver --check`
- 实现:`apps/journal-archiver/tests/config.rs::arc_013_testnet_guard`

| 行 | 最新已封存段的 refdata 账户,`prod` source 前缀 | 期望 |
|---|---|---|
| ARC-013.1 | 只有 `is_prod = false` 的账户,`hft`(缺省) | 退出 2 |
| ARC-013.2 | 含一个 `is_prod = true` 的账户,`hft` | 退出 0 |
| ARC-013.3 | 只有 `is_prod = false`,`hft-testnet` | 退出 0 |
| ARC-013.4 | 冷副本中还没有已封存段 | 退出 0(不做该项检查) |
| ARC-013.5 | `sim` 只有 Sim 账户(`is_prod = false`),`hft`(目标 `hft/tokyo-cex-1/sim`) | 退出 0 |
| ARC-013.6 | 较早的段只有 `is_prod = false`、最新已封存段含 `is_prod = true`,`hft` | 退出 0(只看最新已封存段) |

### ARC-014 前缀改变后从新前缀的空游标起步,旧前缀的状态原样保留

- 规则:[使用本组件](README.md#使用本组件)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M7
- 前置:A0,`prod` source 起初 `prefix = "hft-testnet"`,已提交到 H05(游标 `last_seq = 900000`),H04 有一个 quarantine;之后停止 archiver,`<cold_root>/prod/` 整体移走,hft-core 以正式 refdata 起步写新 journal(段 1 起,`first_seq = 1`),source 改为 `prefix = "hft"`
- 步骤:
  1. 重启 archiver,推进到新 journal 第一个段封存且其小时结束之后
- 期望:
  - `<state>/cursor/hft/prod/seq.json` 新建,`last_seq` 为新段的最后 seq;新 journal 段 1 上传到 `hft/tokyo-cex-1/prod/…`
  - `<state>/cursor/hft-testnet/prod/seq.json`、`<state>/committed/hft-testnet/prod/seq/` 与 `<state>/quarantine/hft-testnet/prod/seq/2026-11-02T04/` 的内容与重启前逐字节相同
  - 新 journal 的上传没有因旧游标 `last_seq = 900000` 跳过任何段
- 实现:`apps/journal-archiver/tests/keys.rs::arc_014_prefix_switch`

### ARC-015 前缀改为正式而冷副本仍是测试网 journal 时拒绝启动

- 规则:[使用本组件](README.md#使用本组件)
- 级别:integ
- 里程碑:M7
- 前置:同 ARC-014,但没有移走 `<cold_root>/prod/`(最新已封存段的 refdata 只有测试网账户)
- 步骤:
  1. 以 `prefix = "hft"` 启动 `run`
- 期望:
  - 退出 2;假 S3 没有收到任何 `PUT`
- 实现:`apps/journal-archiver/tests/keys.rs::arc_015_prefix_switch_without_new_journal`

### ARC-020 封存条件:未封存则等待,损坏则 quarantine

- 规则:[输入:什么是"已封存"](README.md#输入什么是已封存)
- 级别:table
- 里程碑:M5
- 前置:A0;H03 只有标准小时段 5;按行改动段 5 或其 `.seal`;推进到 T0 + 4 h 15 min
- 实现:`apps/journal-archiver/tests/seal.rs::arc_020_seal_conditions`

| 行 | 改动 | 期望 |
|---|---|---|
| ARC-020.1 | 无 | H03 提交 |
| ARC-020.2 | 没有 `.seal` | H03 未处理(无 `PUT`、无 quarantine、无告警);补上有效 `.seal` 后下一次扫描提交 |
| ARC-020.3 | `.seal` 只有 64 B | 同 .2 |
| ARC-020.4 | `.seal` 的 CRC 不符 | 同 .2 |
| ARC-020.5 | `.seal` 有效,段文件长度 ≠ `SealRecord.len` | H03 quarantine(原因含长度),告警 `Quarantine` |
| ARC-020.6 | `.seal` 有效,段文件偏移 5 000 处改一个字节 | quarantine(原因含 SHA-256 不符) |
| ARC-020.7 | `.seal` 有效,最后一条记录不是 `SegmentRoll{End}` | quarantine |
| ARC-020.8 | 段文件偏移 3 080 处(写者行)改一个字节,`.seal` 不变 | H03 提交(3072..3200 按零参与哈希) |

### ARC-021 链断裂时该 journal 停止推进并报 P0,其他 journal 不受影响

- 规则:[输入:什么是"已封存"](README.md#输入什么是已封存)
- 级别:table
- 里程碑:M5
- 前置:A0;`prod` `seq` 段 4(本地)与段 5(H03)、段 6(H04);`sim` `seq` 段链完好;按行改动段 5 的段头;推进到 T0 + 5 h 15 min
- 实现:`apps/journal-archiver/tests/seal.rs::arc_021_chain_break`

| 行 | 改动 | 期望 |
|---|---|---|
| ARC-021.1 | `prev_header_sha256` ≠ 段 4 段头 0..3072 的 SHA-256 | 告警 `ChainBreak`(P0);`prod` `seq` 的 H03、H04 都不处理(游标不动);`sim` 的 H03、H04 提交 |
| ARC-021.2 | `first_seq` = 段 4 `last_seq` + 2,未置 `RECOVERY_GAP` | 同 .1 |
| ARC-021.3 | `first_seq` = 段 4 `last_seq` + 1 | H03、H04 提交,无告警 |

### ARC-022 RECOVERY_GAP 记入 gaps 并报 P1,照常提交

- 规则:[输入:什么是"已封存"](README.md#输入什么是已封存)、[每小时 manifest](README.md#每小时-manifest)
- 级别:integ
- 里程碑:M5
- 前置:A0;段 5 的段头 `flags = RECOVERY_GAP`,`first_seq` = 段 4 `last_seq` + 1 001
- 步骤:
  1. 推进到 T0 + 4 h 15 min
- 期望:
  - H03 提交;manifest `segments[0].recovery_gap = true`,`gaps` 恰有一项指明段 5 的恢复缺口;告警 `Gap`(P1);没有 `ChainBreak`
- 实现:`apps/journal-archiver/tests/seal.rs::arc_022_recovery_gap`

### ARC-023 缺失的段号记入 gaps,已提交后删除的前一段不算缺口

- 规则:[输入:什么是"已封存"](README.md#输入什么是已封存)
- 级别:table
- 里程碑:M5
- 前置:A0;H03 有段 5;按行设定段 4 的状态与游标
- 实现:`apps/journal-archiver/tests/seal.rs::arc_023_missing_segment`

| 行 | 段 4 与游标 | 期望 |
|---|---|---|
| ARC-023.1 | 段 4 不在本地,游标 `last_seq` = 段 5 `first_seq − 1`(段 4 已提交后按保留期删除) | H03 提交,`gaps = []`,无告警 |
| ARC-023.2 | 段 4 不在本地,游标 `last_seq` = 段 5 `first_seq − 1 − 3000`(段 4 从未提交) | H03 提交,`gaps` 有"缺失 segment_no 4",告警 `Gap` |
| ARC-023.3 | 游标为空,段 5 是本地最早的段 | 同 .2(缺失段 1–4) |
| ARC-023.4 | 游标为空,本地只有段 1(`segment_no = 1`,`prev_header_sha256` 全零) | 提交,`gaps = []` |

### ARC-024 段所属小时取段头 created_utc_ns

- 规则:[输入:什么是"已封存"](README.md#输入什么是已封存)
- 级别:table
- 里程碑:M5
- 前置:A0;`prod` `seq` 段按行;推进到所列小时结束后 15 min
- 实现:`apps/journal-archiver/tests/hours.rs::arc_024_segment_hour`

| 行 | 段 | 期望 |
|---|---|---|
| ARC-024.1 | 段 5 `created_utc_ns` = 03:00:00.2,段 6(安全阀滚动)= 03:41:07 | H03 一个 manifest,`segments` 两项(5、6),`records_by_kind` 是两段之和 |
| ARC-024.2 | 段 7 `created_utc_ns` = 23:00:00.1,含 23:00–00:02 的记录(资金费窗口推迟整点滚动);段 8 = 00:02:05 | 段 7 的全部记录(含 00:00–00:02 的)进 `date=2026-11-02/hour=23` 的分区与 H23 manifest;段 8 属次日 H00 |
| ARC-024.3 | 段 9 `created_utc_ns` = 03:59:59.999 | 属 H03 |
| ARC-024.4 | 段 10 `created_utc_ns` = 04:00:00.000 | 属 H04 |

### ARC-025 小时单元等到小时结束且该小时的段全部封存才处理

- 规则:[输入:什么是"已封存"](README.md#输入什么是已封存)
- 级别:table
- 里程碑:M5
- 前置:A0;按行设定 H03 的段与当前时刻
- 实现:`apps/journal-archiver/tests/hours.rs::arc_025_hour_completion`

| 行 | 条件 | 期望 |
|---|---|---|
| ARC-025.1 | 段 5(H03)已封存,当前 03:59:59 | H03 不处理 |
| ARC-025.2 | 同上,当前 04:00:00 且段 6(H04)已存在 | H03 处理 |
| ARC-025.3 | 段 5 已封存,段 6(`created_utc_ns` 03:41)未封存,当前 04:05 | H03 不处理;段 6 封存后的第一次扫描处理,`segments` 含 5、6 |
| ARC-025.4 | 实例在 03:30 干净 Drain(段 5 以 `End` 封存),之后没有新段,当前 04:00:30 | H03 处理,只含段 5 |
| ARC-025.5 | 实例 03:30 Drain、03:45 重启写段 6(`created_utc_ns` 03:45),当前 04:00:30,段 6 仍在写 | H03 不处理,直到段 6 封存;manifest 含段 5、6 |

### ARC-026 同一 journal 的小时按时间顺序处理,游标单调

- 规则:[输入:什么是"已封存"](README.md#输入什么是已封存)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 有一个段未封存(实例崩溃,尚未恢复),H04、H05 的段已封存
- 步骤:
  1. 推进到 T0 + 6 h;再让 H03 的段经恢复封存后推进 15 min
- 期望:
  - H03 封存之前 H04、H05 都不处理,游标停在 H02;之后 H03、H04、H05 依次提交,审计行的提交时刻按小时递增,游标依次为 H03、H04、H05
- 实现:`apps/journal-archiver/tests/hours.rs::arc_026_hours_in_order`

### ARC-027 不读未封存段的撕裂尾

- 规则:[输入:什么是"已封存"](README.md#输入什么是已封存)、[概要](README.md#概要)
- 级别:integ
- 里程碑:M5
- 前置:A0;`prod` `seq` 当前写入中的段 6 的冷副本在 `committed_len` 之后有一段任意字节(模拟撕裂尾),没有 `.seal`
- 步骤:
  1. 推进 2 h(段 6 始终未封存)
- 期望:
  - 没有对段 6 的读取(假文件系统记录对其 `read`/`mmap` 0 次);无告警;无 quarantine
- 实现:`apps/journal-archiver/tests/seal.rs::arc_027_unsealed_not_read`

### ARC-028 raw journal 与 BBO 拆分的 raw journal 同样归档

- 规则:[输入:什么是"已封存"](README.md#输入什么是已封存)
- 级别:integ
- 里程碑:M5
- 前置:A0,`prod` 的 `journals` 加 `raw-feed-binance-usdm-bbo`;H03 内 raw 段因安全阀滚动有 3 段
- 步骤:
  1. 推进到 T0 + 4 h 30 min
- 期望:
  - `raw-feed-binance-usdm` 与 `raw-feed-binance-usdm-bbo` 的 H03 各一个 manifest,前者 `segments` 3 项;journal 原件键在 `public/journal/journal=raw-feed-binance-usdm-bbo/` 下
- 实现:`apps/journal-archiver/tests/units.rs::arc_028_raw_journals`

### ARC-030 各类对象的键

- 规则:[输出:S3 布局](README.md#输出s3-布局)
- 级别:table
- 里程碑:M5
- 前置:A0;`prod` H03 为标准小时段 5,另含一条 refdata `ConfigBlob`(内容 SHA-256 记作 R)、一个 `probe-a` 实例的每日检查点;2026-11-02 全天已提交;`DurableMeta` 的 `engine_epoch` 在 03:00:00 写入
- 实现:`apps/journal-archiver/tests/keys.rs::arc_030_object_keys`

| 行 | 对象 | 期望的键 |
|---|---|---|
| ARC-030.1 | journal 原件 | `hft/tokyo-cex-1/prod/private/journal/journal=seq/date=2026-11-02/hour=03/0000000005-00000000000000040001.jrn.zst` |
| ARC-030.2 | `.seal` 原件 | 同目录 `0000000005-00000000000000040001.seal` |
| ARC-030.3 | `bbo` Parquet | `hft/tokyo-cex-1/prod/public/bbo/exchange=binance-usdm/symbol=BTCUSDT/date=2026-11-02/hour=03/seq-40001-000.parquet` |
| ARC-030.4 | manifest | `hft/tokyo-cex-1/prod/private/manifests/journal=seq/date=2026-11-02/hour=03/manifest.json` |
| ARC-030.5 | 配置 blob | `hft/tokyo-cex-1/prod/private/config/refdata/<R>.bin` |
| ARC-030.6 | 账户日报 | `hft/tokyo-cex-1/prod/private/account_daily/date=2026-11-02/account_daily.parquet` |
| ARC-030.7 | 策略日报 | `hft/tokyo-cex-1/prod/private/strategy_daily/date=2026-11-02/strategy_daily.parquet` |
| ARC-030.8 | refdata 目录 | `hft/tokyo-cex-1/prod/private/catalogs/<refdata_version>.json` |
| ARC-030.9 | 每日检查点 | `hft/tokyo-cex-1/prod/private/advisor/probe-a/2026-11-02/probe-a.ckpt` |
| ARC-030.10 | 持久元数据 | `hft/tokyo-cex-1/prod/private/meta/engine_epoch.20261102T030000Z.<文件 SHA-256 前 12 位>` |
| ARC-030.11 | files source 的不可变文件 `runs/tokyo-cex-1-20261102T010000Z/0.json` | `hft/verify/tokyo-cex-1/runs/tokyo-cex-1-20261102T010000Z/0.json` |

### ARC-031 domain:journal 原件与 manifest 按 journal 定,Parquet 按事件 kind 定

- 规则:[输出:S3 布局](README.md#输出s3-布局)
- 级别:table
- 里程碑:M5
- 前置:A0;H03 内三个 journal 都有封存段
- 实现:`apps/journal-archiver/tests/keys.rs::arc_031_domains`

| 行 | 对象 | 期望 domain |
|---|---|---|
| ARC-031.1 | `seq` 的 journal 原件、`.seal` 与 manifest | `private` |
| ARC-031.2 | `raw-feed-binance-usdm` 的原件与 manifest | `public` |
| ARC-031.3 | `raw-gw-binance-usdm` 的原件与 manifest | `private` |
| ARC-031.4 | `bbo`、`trade`、`funding`、`mark_price` Parquet(`0x01xx`) | `public` |
| ARC-031.5 | `order_event`、`exec_decision`、`note`、`cmd_result`、`reconcile_report` Parquet | `private` |
| ARC-031.6 | `raw_rx`(来自 `raw-feed-*`)/ `raw_tx`、`raw_rx`(来自 `raw-gw-*`) | `public` / `private` |

### ARC-032 private 对象用 SSE-KMS,public 对象用桶默认加密

- 规则:[输出:S3 布局](README.md#输出s3-布局)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 已提交
- 步骤:
  1. 读假 S3 记录的每个 `PUT` 的头
- 期望:
  - 键含 `/private/` 的每个 `PUT` 带 `x-amz-server-side-encryption: aws:kms` 与 `x-amz-server-side-encryption-aws-kms-key-id: kms-test`;键含 `/public/` 的 `PUT` 不带这两个头
- 实现:`apps/journal-archiver/tests/keys.rs::arc_032_sse_kms`

### ARC-033 exchange 与 symbol 分区取值

- 规则:[输出:S3 布局](README.md#输出s3-布局)
- 级别:table
- 里程碑:M5
- 前置:A0;H03 内各行的记录
- 实现:`apps/journal-archiver/tests/keys.rs::arc_033_partitions`

| 行 | 记录 | 期望分区 |
|---|---|---|
| ARC-033.1 | `binance-usdm` BTCUSDT 的 `Bbo` | `exchange=binance-usdm/symbol=BTCUSDT` |
| ARC-033.2 | `okx-spot` 的 `Trade`(refdata 规范符号 `BTC-USDT`) | `exchange=okx-spot/symbol=BTC-USDT` |
| ARC-033.3 | 与 venue 无关的 `ClockStatus` | `exchange=_sys` |
| ARC-033.4 | 与 inst 无关的 `ConnState`(`bitget-usdt-futures`) | `exchange=bitget-usdt-futures/symbol=_all` |

### ARC-034 执行类 dataset 带 account 与 account_kind,取自记录时刻的 refdata

- 规则:[输出:S3 布局](README.md#输出s3-布局)
- 级别:integ
- 里程碑:M5
- 前置:A0;`sim` H03 内账户 11 的 5 条 `OrderEvent`;`prod` H03 内账户 1 的 3 条 `OrderEvent`,其后同一小时的 `ConfigBlob`(refdata)新增账户 7(`kind = Live`),之后账户 7 的 2 条 `OrderEvent`
- 步骤:
  1. 提交 H03,读回两个实例的 `order_event` Parquet
- 期望:
  - `sim` 的 5 行 `account = 11`、`account_kind = "sim"`;`prod` 的 5 行中账户 1 的 3 行与账户 7 的 2 行都是 `account_kind = "live"`(账户 7 的行取自新 refdata)
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_034_account_columns`

### ARC-035 恢复代 g ≥ 1 时对象文件名带 .r<g>,配置 blob 与持久元数据除外

- 规则:[输出:S3 布局](README.md#输出s3-布局)、[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:table
- 里程碑:M5
- 前置:A0;本地状态中 `(hft, tokyo-cex-1, prod)` 的恢复代 g 按行;提交 H03 与该日的日报、检查点与持久元数据
- 实现:`apps/journal-archiver/tests/keys.rs::arc_035_restore_gen_names`

| 行 | g | 期望 |
|---|---|---|
| ARC-035.1 | 0 | 键与 ARC-030 相同,没有 `.r` 段;manifest `restore_gen = 0` |
| ARC-035.2 | 1 | `…-00000000000000040001.r1.jrn.zst`、`….r1.seal`、`seq-40001-000.r1.parquet`、`manifest.r1.json`、`account_daily.r1.parquet`、`strategy_daily.r1.parquet`、`<refdata_version>.r1.json`、`probe-a.r1.ckpt`;manifest `restore_gen = 1` |
| ARC-035.3 | 1 | 配置 blob 仍为 `config/refdata/<R>.bin`,持久元数据仍为 `meta/engine_epoch.<UTC>.<哈希>`(不带 `.r1`) |
| ARC-035.4 | 1 | files source 的键不带 `.r1` |

### ARC-040 每个 kind 的记录转成 Parquet 后读回逐字段相等

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/archiver/kinds/`:每个进入 Parquet 的事件 kind 各若干条记录的 seq journal 段与一个 `raw-gw` 段
- 期望文件:`fixtures/archiver/kinds/expected/<dataset>.jsonl`(每行一条记录的全部列;Arrow schema 来自 `cargo xtask gen-schema` 的生成物)
- 实现:`apps/journal-archiver/tests/golden.rs::arc_040_kinds_roundtrip`

### ARC-041 时间列由段头校准换算,原始 seq、rx_tsc 与 tsc 保留

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:unit
- 里程碑:M5
- 前置:段头校准对 `calib_tsc = 1 000 000 000`、`calib_realtime_ns = 1793588400 000 000 000`(H03 起点)、`tsc_hz_milli = 1 000 000 000 000`(1 GHz);一条 `Bbo`:记录头 `tsc = 1 000 500 000`,`MarketHdr.rx_tsc = 1 000 400 000`、`ex_ts_ns`、`ex_send_ns`、`hw_rx_ns` 给定
- 步骤:
  1. 转换并读回该行
- 期望:
  - `rx_ns = 1793588400 000 400 000`、`proc_ns = 1793588400 000 500 000`;`ex_ts_ns`、`ex_send_ns`、`hw_rx_ns`、`seq`、`rx_tsc`、`tsc` 与记录中的值相同
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_041_time_columns`

### ARC-042 价格与数量为定点整数,指数写进 KV 元数据

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:unit
- 里程碑:M5
- 前置:refdata 中 BTC 的 `px_exp = -2`、`qty_exp = -3`;一条 `Trade` `px = 6512345`(65 123.45)、`qty = 1500`(1.500)
- 步骤:
  1. 转换并读回
- 期望:
  - 列 `px` = 6512345、`qty` = 1500(整数类型);文件 KV 元数据含 BTC 的 `px_exp = -2`、`qty_exp = -3`
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_042_fixed_point`

### ARC-043 Timer、StateSnapshot 与 ConfigBlob 不进 Parquet,配置 blob 按哈希去重

- 规则:[Parquet 规则](README.md#parquet-规则)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 与 H04 的引导块各含同一份 refdata `ConfigBlob`(SHA-256 = R)与 `StateSnapshot`,另有 `Timer` × 30
- 步骤:
  1. 提交 H03、H04
- 期望:
  - 没有 `timer`、`state_snapshot`、`config_blob` dataset 的对象;`config/refdata/R.bin` 在 H03 `PUT` 一次,H04 只 `HEAD`(`AlreadyPresent`)
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_043_excluded_kinds`

### ARC-044 raw journal 进 raw_rx/raw_tx,payload 为 binary 列

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:unit
- 前置:`raw-gw-binance-usdm` 段含 3 条 `RawRx`(原文 120、80、64 B)与 2 条 `RawTx`
- 里程碑:M5
- 步骤:
  1. 转换并读回
- 期望:
  - `raw_rx` 3 行、`raw_tx` 2 行;`payload` 列类型为 binary,长度依次 120、80、64,字节与原文相同
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_044_raw_datasets`

### ARC-045 advice dataset 带 strategy 与 local_kind,载荷为 binary

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:unit
- 里程碑:M5
- 前置:一条 `Advice`(strategy 3、`local_kind = 2`、载荷 48 B)
- 步骤:
  1. 转换并读回 `advice`
- 期望:
  - 一行 `strategy = 3`、`local_kind = 2`、`payload` 48 B 与原载荷相同
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_045_advice_dataset`

### ARC-046 Parquet 用 zstd level 3 并开启页级统计

- 规则:[Parquet 规则](README.md#parquet-规则)、[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M5
- 前置:A0(`parquet.zstd_level = 3`);标准小时段
- 步骤:
  1. 转换,用 `parquet` 读取文件元数据
- 期望:
  - 每个列块的压缩编码为 `ZSTD`,写入参数记录 level 3;每页有统计(min/max/null_count);`created_by` = `hft-journal-archiver/<ver>`
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_046_compression`

### ARC-047 row group 的行数与字节阈值

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:table
- 里程碑:M5
- 前置:A0(`row_group_rows = 1000000`、`row_group_bytes = 134217728`、`min_row_group_rows = 100000`);H03 某 symbol 的 `bbo` 行数按行
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_047_row_groups`

| 行 | 行数 / 条件 | 期望 row group 行数 |
|---|---|---|
| ARC-047.1 | 1 000 000 | [1 000 000] |
| ARC-047.2 | 1 000 001 | [1 000 000, 1],末组例外 |
| ARC-047.3 | 2 500 000 | [1 000 000, 1 000 000, 500 000] |
| ARC-047.4 | 每行 200 B,`row_group_bytes = 16777216`(dev2 取值),行数 300 000 | 每组字节 ≤ 16 MiB,除末组外每组 ≥ 100 000 行 |
| ARC-047.5 | 300 000 行,`row_group_bytes = 1048576` 使字节阈值先到时每组只有约 5 000 行 | 每组 ≥ 100 000 行(行数下限优先于字节阈值,末组除外) |

### ARC-048 超过 max_file_bytes 时切 part

- 规则:[Parquet 规则](README.md#parquet-规则)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:A0,另设 `parquet.max_file_bytes = 8388608`(8 MiB,测试用);某 (dataset, exchange, symbol, hour) 的输出大小按行
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_048_parts`

| 行 | 未切分时的大小 | 期望 |
|---|---|---|
| ARC-048.1 | 8 MiB 以内 | 一个文件 `…-000.parquet` |
| ARC-048.2 | 约 20 MiB | `…-000`、`…-001`、`…-002` 三个文件,每个 ≤ 8 MiB;行按 `(inst, seq)` 在文件间连续不重叠;manifest `objects` 有三项 |

### ARC-049 低量 symbol 并入 _small

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:table
- 里程碑:M5
- 前置:A0(`small_symbol_rows = 100000`);H03 `bbo` 各 symbol 的行数按行
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_049_small_symbols`

| 行 | 行数 | 期望 |
|---|---|---|
| ARC-049.1 | ETHUSDT 99 999 | 进 `symbol=_small` 文件 |
| ARC-049.2 | ETHUSDT 100 000 | 自己的 `symbol=ETHUSDT` 文件 |
| ARC-049.3 | ETHUSDT 100 001 | 自己的文件 |
| ARC-049.4 | AAAUSDT 10、ZZZUSDT 20 | 两者都在 `_small`,文件内按 `(symbol, seq)` 排序;row group 统计中 `symbol` 的 min/max 为 `AAAUSDT`/`ZZZUSDT` |

### ARC-050 文件内排序:行情按 (inst, seq),其他按 seq

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:unit
- 前置:H03 `bbo` 两个 inst 交错到达;`order_event` 乱序于 inst
- 里程碑:M5
- 步骤:
  1. 转换并读回
- 期望:
  - `bbo` 文件按 `(inst, seq)` 严格递增;`order_event` 按 `seq` 严格递增
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_050_sort_order`

### ARC-051 Parquet 的 KV 元数据

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:unit
- 里程碑:M5
- 前置:标准小时段;发布版本 `0.1.0`、git sha `abc123`;refdata 版本 41;段头 `host_profile_sha256` = HP
- 步骤:
  1. 读 `bbo` 文件的 KV 元数据
- 期望:
  - `hft.schema` 含 dataset `bbo` 与其 `schema_version`;`hft.instance = "prod"`、`hft.journal = "seq"`、`hft.first_seq = 40001`、`hft.last_seq` = 段 5 末条 seq;`hft.release` 含版本 `0.1.0` 与 git sha `abc123`;`hft.refdata_version = 41`、`hft.host_profile_hash = HP`
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_051_kv_metadata`

### ARC-052 暂存文件先写 .new 再 rename,单元开始时清掉残留的 .new

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:table
- 里程碑:M5
- 前置:A0;H03 未提交
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_052_staging`

| 行 | 条件 | 期望 |
|---|---|---|
| ARC-052.1 | 无残留文件 | 每个暂存文件的系统调用顺序为 `openat(<key>.new, O_CREAT\|O_EXCL)` → 写 → `fsync` → rename 到 `<key>`;记录的 bytes 与 SHA-256 等于最终文件 |
| ARC-052.2 | 暂存目录中有上次崩溃留下的 `<H03 某键>.new`(半个文件) | 单元开始时删除它;随后写出成功,没有 `EEXIST` 错误 |

### ARC-053 dataset 与列来自 gen-schema 的生成物

- 规则:[Parquet 规则](README.md#parquet-规则)
- 级别:unit
- 里程碑:M5
- 前置:`cargo xtask gen-schema` 输出的 dataset ↔ kind 映射表与 Arrow schema
- 步骤:
  1. 比较 archiver 转换器注册的 dataset 名、列名与类型和生成物;扫描 `apps/journal-archiver/src/**/*.rs`
- 期望:
  - 两边逐项相等;源码中没有手写的列名字面量(列名只经生成物引用)
- 实现:`apps/journal-archiver/tests/parquet.rs::arc_053_schema_from_gen_schema`

### ARC-060 manifest 的字段、规范化 JSON 与提交证明中的哈希

- 规则:[每小时 manifest](README.md#每小时-manifest)、[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 为标准小时段;`ClockStatus` 全小时 `Synced`,最大偏移 31 µs、最大误差界 38 µs
- 步骤:
  1. 提交 H03,读 manifest 与 `<state>/committed/hft/prod/seq/2026-11-02T03.json`
- 期望:
  - manifest 键集合与 README 示例相同;`schema_version = 1`、`instance = "prod"`、`journal = "seq"`、`host = "tokyo-cex-1"`、`restore_gen = 0`、`hour_utc = "2026-11-02T03:00:00Z"`;`clock = {synced_all_hour: true, max_abs_offset_us: 31, max_error_bound_us: 38}`;`segments[0]` 的 `segment_no = 5`、`first_seq = 40001`、`seal_sha256` 等于 `.seal` 中的值、`journal_key` 为 ARC-030.1 的键
  - 文件字节等于按键排序、无多余空白的规范化 JSON;提交证明 `manifest_sha256` = 该字节串的 SHA-256,`manifest_key`、`bucket = "gwx-trading-data"` 与实际一致
- 实现:`apps/journal-archiver/tests/manifest.rs::arc_060_manifest_fields`

### ARC-061 两条计数等式不成立时不提交并 quarantine

- 规则:[每小时 manifest](README.md#每小时-manifest)
- 级别:table
- 里程碑:M5
- 前置:A0;H03 为标准小时段;按行在转换中注入故障
- 实现:`apps/journal-archiver/tests/manifest.rs::arc_061_count_equations`

| 行 | 故障 | 期望 |
|---|---|---|
| ARC-061.1 | 无 | `records_by_kind` 之和 = `segments[].records` 之和 = 段 5 的记录总数;Parquet 行数之和 = 进入 Parquet 的 kind 的记录数之和;提交 |
| ARC-061.2 | `bbo` 写者丢掉一行 | 不 `PUT` manifest;quarantine 原因 `RowCountMismatch`(含期望与实际行数);告警 `Quarantine` |
| ARC-061.3 | 读者对某 kind 少计一条 | 不提交;quarantine 原因 `RecordCountMismatch` |

### ARC-062 多记录事件按记录计数

- 规则:[每小时 manifest](README.md#每小时-manifest)
- 级别:unit
- 里程碑:M5
- 前置:H03 含一条 `L2Batch` 事件,由 1 条头记录与 12 条 `L2Delta` 记录组成
- 步骤:
  1. 生成 manifest
- 期望:
  - `records_by_kind` 中两种记录分别计 1 与 12;Parquet 对应 dataset 的行数与这些记录数满足等式,manifest 提交
- 实现:`apps/journal-archiver/tests/manifest.rs::arc_062_multi_record_events`

### ARC-063 manifest 的 config 与 release 取该小时生效的配置

- 规则:[每小时 manifest](README.md#每小时-manifest)
- 级别:unit
- 里程碑:M5
- 前置:H03 引导块含 refdata(SHA R1)、两份 `StrategyParams`(S1、S2)、`RiskParams`(K1);03:30 有新的 `StrategyParams`(S3)
- 步骤:
  1. 生成 manifest
- 期望:
  - `config = {refdata: R1, strategy_params: [S1, S2, S3], risk_params: [K1]}`;`release.version`、`release.git_sha` 与段头 `build_git` 一致;`host_profile_hash` 等于段头 `host_profile_sha256`
- 实现:`apps/journal-archiver/tests/manifest.rs::arc_063_config_and_release`

### ARC-070 strategy_daily 是固定 schema 的长表

- 规则:[策略日报](README.md#策略日报)
- 级别:unit
- 里程碑:M5
- 前置:A0 + S0;2026-11-02 的 `strategy_daily.parquet`
- 步骤:
  1. 读文件 schema
- 期望:
  - 列与类型依次为 `date`(date)、`instance`(string)、`account`(u8)、`account_kind`(string)、`strategy`(u8)、`kind_id`(u16)、`strategy_name`(string)、`group`(u32,可空)、`item`(string)、`name`(string)、`code`(u32)、`level`(string)、`unit`(string)、`agg`(string)、`value`(i64)、`samples`(u64);没有其他列
- 实现:`apps/journal-archiver/tests/daily.rs::arc_070_strategy_daily_schema`

### ARC-071 strategy_daily 与 watchtower 日报 strategies 节逐字段相等

- 规则:[策略日报](README.md#策略日报)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:A0 + S0;`prod` 实例 3(`probe-taker`)当日策略 Note:seq 100 码 1 group G1 `v[0] = 10`、seq 101 码 1 G2 `v[0] = 5`、seq 102 码 1 G1 `v[0] = 20`、seq 103 码 2、seq 104 码 99;同一 journal 与 S0 由 watchtower 生成当日日报
- 步骤:
  1. 该日最后一小时提交后读 `strategy_daily`
- 期望:
  - `item = "note"` 行:码 1 `value = 3`、码 2 `value = 1`;`item = "unregistered"` 行:码 99 `value = 1`
  - `item = "report"` 行:`edges` 3;`edge_sum` G1 30、G2 5;`edge_last` 20;`edge_max` G1 20、G2 5;`edge_p99` 20(`samples = 3`);`scope = Group` 的字段按 group 分行、`Instance` 的 `group` 为空
  - 与 watchtower 日报 `prod.strategies` 节的同一组数据逐字段相等
- 实现:`apps/journal-archiver/tests/daily.rs::arc_071_strategy_daily_equals_watchtower`

### ARC-072 目录里没有的 kind_id:该实例的 Note 全部计 unregistered 并报 P1,不阻塞归档

- 规则:[实现说明](README.md#实现说明)、[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0 + S0;`prod` 实例 5 的 `StrategyParams` 头 `kind_id = 65400`(S0 中没有)与 4 条策略 Note(码 1 × 3、码 2 × 1)
- 步骤:
  1. 该日最后一小时提交后读 `strategy_daily` 与告警
- 期望:
  - 实例 5 只有 `item = "unregistered"` 的行,合计 `value = 4`;告警 `UnknownKindId`(P1);该日各小时照常提交
- 实现:`apps/journal-archiver/tests/daily.rs::arc_072_unknown_kind_id`

### ARC-073 只新增一个测试策略的 schema 文件,archiver 代码与 strategy_daily schema 零改动

- 规则:[策略日报](README.md#策略日报)
- 级别:integ
- 里程碑:M5
- 前置:同一个已构建的 archiver 二进制;目录 S1 = S0 + 种类 `probe-extra`(`kind_id = 65300`,码 1 `extra_hit`,report `hits`(Count))
- 步骤:
  1. 以 S1 启动;journal 写 `probe-extra` 实例 6 的 2 条码 1 Note;提交该日
- 期望:
  - `strategy_daily` 有实例 6 的 `note` 行(`value = 2`)与 `report` 行 `hits = 2`;文件 schema 与 ARC-070 相同;二进制未重新构建
- 实现:`apps/journal-archiver/tests/daily.rs::arc_073_new_strategy_zero_change`

### ARC-074 strategy_daily 的 KV 元数据记 index.json 的 SHA-256

- 规则:[策略日报](README.md#策略日报)
- 级别:unit
- 里程碑:M5
- 前置:A0 + S0
- 步骤:
  1. 读 `strategy_daily.parquet` 的 KV 元数据
- 期望:
  - `hft.strategy_schema_index_sha256` = S0 的 `index.json` 文件的 SHA-256
- 实现:`apps/journal-archiver/tests/daily.rs::arc_074_index_sha_in_kv`

### ARC-075 日报数据集开关

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:A0;按行设定开关;提交 2026-11-02 全天
- 实现:`apps/journal-archiver/tests/daily.rs::arc_075_daily_switches`

| 行 | 配置 | 期望 |
|---|---|---|
| ARC-075.1 | 不写两个开关 | 生效值都是 `true`;两个数据集都写出 |
| ARC-075.2 | `account_daily.enabled = false` | 没有 `account_daily` 对象;`strategy_daily` 照常 |
| ARC-075.3 | `strategy_daily.enabled = false` | 没有 `strategy_daily` 对象 |

### ARC-076 account_daily 与 watchtower 日报 accounts 节逐字段相等

- 规则:[实现说明](README.md#实现说明)、[输出:S3 布局](README.md#输出s3-布局)
- 级别:integ
- 里程碑:M5
- 前置:A0;`prod` 账户 1(group G1、G2)与 `sim` 账户 11(group G1)当日有成交与资金费;watchtower 用同一 journal 生成日报
- 步骤:
  1. 该日最后一小时提交后读两个实例的 `account_daily`
- 期望:
  - `prod` 3 行(账户 1、1 × G1、1 × G2),`sim` 2 行;`sim` 的行 `account_kind = "sim"`、`prod` 的为 `"live"`;各行与 watchtower 日报 `accounts` 节对应行逐字段相等
- 实现:`apps/journal-archiver/tests/daily.rs::arc_076_account_daily_equals_watchtower`

### ARC-077 日报数据集在该日最后一小时提交后生成,随次日第一个 manifest 上传

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:A0;2026-11-02 H23 的 `seq` 段在 2026-11-03T00:02 封存
- 步骤:
  1. 推进到 2026-11-03T01:20
- 期望:
  - H23 提交之前没有生成 2026-11-02 的日报对象;两个日报对象出现在 2026-11-03 H00 的 manifest `objects` 中(`dataset` 为 `account_daily`、`strategy_daily`),在该 manifest 之前 `PUT`
- 实现:`apps/journal-archiver/tests/daily.rs::arc_077_daily_with_next_manifest`

### ARC-080 归档 UTC 00:00 之后写出的第一份检查点

- 规则:[Model 检查点每日归档](README.md#model-检查点每日归档)
- 级别:integ
- 里程碑:M5
- 前置:A0;`advisor_dir` 中 `probe-a.ckpt` 依次在 2026-11-01T23:50:00Z、2026-11-02T00:05:00Z、00:35:00Z 以 rename 写出(内容各不同,CRC 正确)
- 步骤:
  1. 推进到 2026-11-02T01:00
- 期望:
  - `paths.advisor_daily/prod/2026-11-02/probe-a.ckpt` 与 00:05 那份逐字节相同;23:50 那份不进 11-02 的目录;00:35 那份不覆盖(本地与 S3 都只有 00:05 的内容)
- 实现:`apps/journal-archiver/tests/ckpt.rs::arc_080_first_after_midnight`

### ARC-081 头部校验不过的检查点被跳过,下一份被归档

- 规则:[Model 检查点每日归档](README.md#model-检查点每日归档)、[hft-core 检查点表](../hft-core/README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M5
- 前置:A0;00:05 写出的一份按行损坏,00:20 写出一份完好的
- 实现:`apps/journal-archiver/tests/ckpt.rs::arc_081_corrupt_skipped`

| 行 | 00:05 那份的损坏 | 期望 |
|---|---|---|
| ARC-081.1 | magic 不是 `HFTCKPT1` | 归档 00:20 那份 |
| ARC-081.2 | `body_len` ≠ 文件长度 − 64 | 归档 00:20 那份 |
| ARC-081.3 | CRC 不符 | 归档 00:20 那份 |
| ARC-081.4 | 无损坏 | 归档 00:05 那份 |

### ARC-082 当天没有检查点时不归档、无错误

- 规则:[Model 检查点每日归档](README.md#model-检查点每日归档)
- 级别:integ
- 里程碑:M5
- 前置:A0;2026-11-02 全天 `advisor_dir` 中没有新写出的检查点(最近一份在 11-01 23:50)
- 步骤:
  1. 推进到 2026-11-03T01:00
- 期望:
  - `paths.advisor_daily/prod/2026-11-02/` 不存在;S3 没有该日的检查点对象;无告警;各小时照常提交
- 实现:`apps/journal-archiver/tests/ckpt.rs::arc_082_no_checkpoint_day`

### ARC-083 检查点上传并记入当日最早未提交的 seq manifest

- 规则:[Model 检查点每日归档](README.md#model-检查点每日归档)
- 级别:integ
- 里程碑:M5
- 前置:A0;2026-11-02 H00 已提交;00:05 写出检查点(复制在 00:06 完成);H01 尚未提交
- 步骤:
  1. 推进到 02:20
- 期望:
  - 检查点 `PUT` 到 ARC-030.9 的键,带 SSE-KMS;H01 的 manifest `objects` 有一项 `dataset = "advisor_ckpt"`、`key` 为该键、SHA-256 等于本地副本;H00 的 manifest 不变
- 实现:`apps/journal-archiver/tests/ckpt.rs::arc_083_upload_and_manifest`

### ARC-084 只归档 <strategy_instance>.ckpt,不归档 .prev 与临时文件

- 规则:[Model 检查点每日归档](README.md#model-检查点每日归档)
- 级别:integ
- 里程碑:M5
- 前置:A0;00:05 写出新检查点时,旧文件改名为 `probe-a.ckpt.prev`(mtime 00:05),临时文件 `probe-a.ckpt.tmp` 先出现后被 rename
- 步骤:
  1. 推进到 00:10
- 期望:
  - 归档的只有 `probe-a.ckpt`;没有任何以 `.prev` 或 `.tmp` 结尾的文件被复制或上传
- 实现:`apps/journal-archiver/tests/ckpt.rs::arc_084_only_ckpt_files`

### ARC-085 没有 inotify 事件时兜底扫描在 60 s 内归档

- 规则:[Model 检查点每日归档](README.md#model-检查点每日归档)
- 级别:table
- 里程碑:M5
- 前置:A0;inotify 关闭;检查点在 00:05:00 写出;按行推进
- 实现:`apps/journal-archiver/tests/ckpt.rs::arc_085_fallback_scan`

| 行 | 时刻 | 期望 |
|---|---|---|
| ARC-085.1 | 00:05:59(上次扫描在 00:05:00 之前一刻) | 至迟在 00:06:00 的扫描后本地副本存在 |
| ARC-085.2 | inotify 开启 | 00:05:01 之前本地副本存在 |

### ARC-086 本地副本先写 .new 再 rename,保留 local_days.seq 天

- 规则:[Model 检查点每日归档](README.md#model-检查点每日归档)
- 级别:table
- 里程碑:M5
- 前置:A0(`retention.local_days.seq = 7`);`paths.advisor_daily/prod/` 下有 2026-10-25 至 11-02 的日目录
- 实现:`apps/journal-archiver/tests/ckpt.rs::arc_086_local_copy`

| 行 | 条件 | 期望 |
|---|---|---|
| ARC-086.1 | 写本地副本 | 系统调用顺序为写 `probe-a.ckpt.new` → `fsync` → rename;目录中不留 `.new` |
| ARC-086.2 | 2026-11-02T12:00 的 retention | 2026-10-25 的日目录被删除(超过 7 天);10-26 及以后保留 |

### ARC-087 本地副本可作 replayer recompute 的起点

- 规则:[Model 检查点每日归档](README.md#model-检查点每日归档)
- 级别:integ
- 里程碑:M5
- 前置:A0;hft-testkit 测试策略的 `prod` 夹具 journal 一天,含 Advice;archiver 已归档该日检查点
- 步骤:
  1. `replayer verify --journal <该日 seq> --instance prod --advice recompute --model-checkpoint <paths.advisor_daily>/prod`
- 期望:
  - replayer 退出 0,报告 `recompute = "pass"`,`checkpoint_key` 指向该本地副本,从文件头的 `last_seq` 起重算
- 实现:`apps/journal-archiver/tests/ckpt.rs::arc_087_replayer_recompute`

### ARC-088 未设 advisor_dir 的 source 不做检查点归档

- 规则:[Model 检查点每日归档](README.md#model-检查点每日归档)、[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M5
- 前置:A0;`sim` source 没有 `advisor_dir`,`<cold_root>/sim/advisor/` 中有检查点写出
- 步骤:
  1. 推进到 01:00
- 期望:
  - `paths.advisor_daily/sim/` 不存在;没有对 `<cold_root>/sim/advisor/` 的 inotify 监听
- 实现:`apps/journal-archiver/tests/ckpt.rs::arc_088_no_advisor_dir`

### ARC-090 不可变文件按相对路径上传到 <prefix>/<host>/ 下

- 规则:[文件归档(files source)](README.md#文件归档files-source)
- 级别:integ
- 里程碑:M5
- 前置:A0;`<tmp>/verify/runs/tokyo-cex-1-20261102T010000Z/0.json` 以 rename 写出,mtime 为 T0 + 1 h
- 步骤:
  1. 推进到 T0 + 1 h + 61 s
- 期望:
  - 假 S3 收到 `HEAD` 后 `PUT` 到 `hft/verify/tokyo-cex-1/runs/tokyo-cex-1-20261102T010000Z/0.json`(带 `If-None-Match: *` 与 `x-amz-checksum-sha256`),再 `HEAD` 核验;`<state>/audit.jsonl` 追加一行;`<state>/files/verify.json` 记该相对路径与 SHA-256
- 实现:`apps/journal-archiver/tests/files.rs::arc_090_immutable_upload`

### ARC-091 文件最后一次修改后 settle_s 内不变才上传

- 规则:[文件归档(files source)](README.md#文件归档files-source)
- 级别:table
- 里程碑:M5
- 前置:A0(`settle_s = 60`);文件 `resolutions/x.json` 的 mtime = T;扫描时刻按行
- 实现:`apps/journal-archiver/tests/files.rs::arc_091_settle`

| 行 | 扫描时刻 | 期望 |
|---|---|---|
| ARC-091.1 | T + 59.999 s | 不上传 |
| ARC-091.2 | T + 60 s | 上传 |
| ARC-091.3 | T + 30 s 时文件再次被改写(mtime 变为 T + 30 s),扫描于 T + 61 s | 不上传;T + 90 s 的扫描上传 |

### ARC-092 排除规则:.new、.tmp、隐藏文件与 include 之外的文件不上传

- 规则:[文件归档(files source)](README.md#文件归档files-source)
- 级别:table
- 里程碑:M5
- 前置:A0;各行的文件写出后推进 120 s
- 实现:`apps/journal-archiver/tests/files.rs::arc_092_exclusions`

| 行 | 文件 | 期望 |
|---|---|---|
| ARC-092.1 | `runs/r1/3.json.tmp` | 不上传 |
| ARC-092.2 | `runs/r1/3.json.new` | 不上传 |
| ARC-092.3 | `crashes/dst/.inflight.json`,每 10 s 改写一次 | 不上传;零 `Conflict` |
| ARC-092.4 | `crashes/binance_bbo/.hidden` | 不上传 |
| ARC-092.5 | `corpus/binance_bbo/abc`(不在 `include`) | 不上传 |
| ARC-092.6 | `crashes/dst/9223372036854775813.json` | 上传 |

### ARC-093 不可变文件同键不同内容时 Conflict,不覆盖

- 规则:[文件归档(files source)](README.md#文件归档files-source)
- 级别:integ
- 里程碑:M5
- 前置:A0;`runs/r1/0.json` 已上传;之后本地被改写为不同内容
- 步骤:
  1. 推进 120 s
- 期望:
  - 没有对该键的 `PUT`;quarantine 记录含该相对路径与两边 SHA-256;告警 `Quarantine`(P1);同目录其他新文件照常上传
- 实现:`apps/journal-archiver/tests/files.rs::arc_093_immutable_conflict`

### ARC-094 快照文件每次变化上传一个带时刻与哈希的新对象

- 规则:[文件归档(files source)](README.md#文件归档files-source)
- 级别:integ
- 里程碑:M5
- 前置:A0;`ledger/ledger.json` 在 T0 + 1 h 与 T0 + 2 h 各以 rename 写出新内容(SHA-256 前 12 位分别为 `aaaaaaaaaaaa`、`bbbbbbbbbbbb`),T0 + 3 h 以相同内容再写一次
- 步骤:
  1. 推进到 T0 + 3 h + 120 s
- 期望:
  - 恰好两个对象:`hft/verify/tokyo-cex-1/ledger/ledger.20261102T010000Z.aaaaaaaaaaaa.json` 与 `…/ledger.20261102T020000Z.bbbbbbbbbbbb.json`(时刻取该版本文件的 mtime);T0 + 3 h 的写出不产生 `PUT`;已有对象从未被覆盖
- 实现:`apps/journal-archiver/tests/files.rs::arc_094_snapshot_files`

### ARC-095 两台主机写出同一相对路径时落在各自主机段下

- 规则:[文件归档(files source)](README.md#文件归档files-source)
- 级别:integ
- 里程碑:M5
- 前置:两个 archiver,`host` 为 `dev2` 与 `tokyo-cex-1`,共用假 S3;两机各写出内容不同的 `crashes/binance_bbo/abc.min` 与 `resolutions/binance_bbo_abc.json`
- 步骤:
  1. 两机各推进 120 s
- 期望:
  - 四个对象分别在 `hft/verify/dev2/…` 与 `hft/verify/tokyo-cex-1/…` 下;零 `Conflict`
- 实现:`apps/journal-archiver/tests/files.rs::arc_095_two_hosts`

### ARC-096 重启后已上传的文件只核验不重传

- 规则:[文件归档(files source)](README.md#文件归档files-source)
- 级别:integ
- 里程碑:M5
- 前置:A0;10 个不可变文件已上传,`<state>/files/verify.json` 记录了它们
- 步骤:
  1. 重启 archiver,推进 120 s
- 期望:
  - 对这 10 个键只有 `HEAD`(带 `ChecksumMode`),没有 `PUT`;本地状态不变
- 实现:`apps/journal-archiver/tests/files.rs::arc_096_restart_verify_only`

### ARC-097 files source 不删除本地文件

- 规则:[文件归档(files source)](README.md#文件归档files-source)
- 级别:integ
- 里程碑:M5
- 前置:A0;`retention` 运行;files source 目录中有 30 天前上传过的文件
- 步骤:
  1. 运行一次 retention
- 期望:
  - files source 目录中的文件数与运行前相同;没有对该目录的 `unlink`
- 实现:`apps/journal-archiver/tests/files.rs::arc_097_no_local_delete`

### ARC-100 上传前重新计算本地文件的 SHA-256,与暂存时记录的不符则 quarantine

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:table
- 里程碑:M5
- 前置:A0;H03 已转换完毕,暂存文件与其 SHA-256 已记录;按行在上传前改动暂存文件
- 实现:`apps/journal-archiver/tests/upload.rs::arc_100_recheck_before_upload`

| 行 | 改动 | 期望 |
|---|---|---|
| ARC-100.1 | 无 | 每个 `PUT` 的 `x-amz-checksum-sha256` 等于记录值;提交 |
| ARC-100.2 | 某 Parquet 暂存文件被改动一个字节 | 该对象不 `PUT`;H03 quarantine,原因 `ChecksumMismatch`;manifest 不 `PUT` |

### ARC-101 S3 已有同键且字节与哈希一致时视为已存在,不重传

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0;假 S3 已有 H03 journal 原件键,bytes 与 SHA-256 与本地一致
- 步骤:
  1. 提交 H03
- 期望:
  - 对该键只有一个带 `ChecksumMode` 的 `HEAD`,没有 `PUT`;审计行记 `AlreadyPresent`;其余对象照常 `PUT`
- 实现:`apps/journal-archiver/tests/upload.rs::arc_101_already_present`

### ARC-102 S3 已有同键不同内容时 Conflict,不覆盖

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:table
- 里程碑:M5
- 前置:A0;假 S3 已有 H03 某 Parquet 键,按行与本地不同
- 实现:`apps/journal-archiver/tests/upload.rs::arc_102_conflict`

| 行 | 差异 | 期望 |
|---|---|---|
| ARC-102.1 | bytes 相同、SHA-256 不同 | 该键无 `PUT`;H03 quarantine(原因 `Conflict`);告警 `Quarantine`;不重试 |
| ARC-102.2 | bytes 不同 | 同 .1 |

### ARC-103 PUT 带条件头与校验和,PUT 之后再 HEAD 核验

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:table
- 里程碑:M5
- 前置:A0;H03 未提交
- 实现:`apps/journal-archiver/tests/upload.rs::arc_103_put_and_verify`

| 行 | 条件 | 期望 |
|---|---|---|
| ARC-103.1 | S3 上没有该键 | 每个对象的请求序列为 `HEAD`(404)→ `PUT`(`If-None-Match: *`、`x-amz-checksum-sha256` = 本地值)→ `HEAD`(bytes 与 SHA-256 一致) |
| ARC-103.2 | `PUT` 成功后的 `HEAD` 返回不同的 SHA-256(注入) | H03 quarantine,原因 `ChecksumMismatch`;manifest 不 `PUT` |
| ARC-103.3 | `PUT` 返回 `412`(另一写者先写入),随后 `HEAD` 一致 | 记 `AlreadyPresent`,继续 |
| ARC-103.4 | `PUT` 返回 `412`,随后 `HEAD` 不一致 | `Conflict`,quarantine |

### ARC-104 上传顺序:journal 原件 → Parquet → 配置 blob → manifest 最后

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 为标准小时段并含一条新的 refdata `ConfigBlob`
- 步骤:
  1. 提交 H03,读假 S3 的 `PUT` 序列
- 期望:
  - 所有 `.jrn.zst` 与 `.seal` 的 `PUT` 早于任何 Parquet 的 `PUT`;所有 Parquet 早于 `config/` 的 `PUT`;manifest 是最后一个 `PUT`
- 实现:`apps/journal-archiver/tests/upload.rs::arc_104_upload_order`

### ARC-105 提交证明与审计行

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03、H04 依次提交;之后 archiver 重启
- 步骤:
  1. 读 `<state>/committed/hft/prod/seq/` 与 `<state>/audit.jsonl`
- 期望:
  - `2026-11-02T03.json` 与 `2026-11-02T04.json` 各含 `{schema_version, bucket, manifest_key, manifest_sha256, committed_utc_ns}`;`audit.jsonl` 每个上传对象一行,重启前后的行都在(文件只追加,行数单调)
- 实现:`apps/journal-archiver/tests/upload.rs::arc_105_commit_proof_and_audit`

### ARC-106 可重试错误按 30 s 起翻倍、上限 30 min 重试 8 次,之后报 P1 并按上限间隔继续

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0(`upload.max_attempts = 8`、`backoff = {30, 1800}`);从 t 起假 S3 对 H03 的第一个 `PUT` 一直返回 503
- 步骤:
  1. 推进到 t + 5 500 s;之后让假 S3 恢复,再推进 1 900 s
- 期望:
  - 尝试时刻为 t、t + 30、t + 90、t + 210、t + 450、t + 930、t + 1 890、t + 3 690(s);第 8 次失败后告警 `RetryExhausted`(P1);t + 5 490 再尝试一次;H03 没有进入 quarantine
  - 恢复后的下一次尝试成功,H03 提交
- 实现:`apps/journal-archiver/tests/upload.rs::arc_106_retry_schedule`

### ARC-107 不可重试的错误进入 quarantine,该 journal 后续小时继续

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 的某对象 `Conflict`,H04 的段与对象都没有故障
- 步骤:
  1. 推进到 T0 + 5 h 15 min
- 期望:
  - `<state>/quarantine/hft/prod/seq/2026-11-02T03/reason.json` 含 `Conflict`、键与两边哈希;H04 提交;游标 `last_committed_hour` 为 H04;H03 只出现在 quarantine 目录中,`check` 列出 H03 为 quarantine;重启后 H03 不被自动重做
- 实现:`apps/journal-archiver/tests/upload.rs::arc_107_quarantine_continue`

### ARC-108 上传中途崩溃后重启,已上传的对象只核验不重传

- 规则:[上传与删除纪律](README.md#上传与删除纪律)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 共 12 个对象;在第 7 个对象 `PUT` 成功后杀掉单元(模拟崩溃)
- 步骤:
  1. 重启,推进 5 min
- 期望:
  - 前 7 个对象只有 `HEAD`(`AlreadyPresent`);其余 5 个 `PUT`;manifest 最后 `PUT`;H03 提交恰好一次
- 实现:`apps/journal-archiver/tests/upload.rs::arc_108_crash_mid_upload`

### ARC-109 提交成功后才在段旁写 .uploaded

- 规则:[上传与删除纪律](README.md#上传与删除纪律)、[输出:S3 布局](README.md#输出s3-布局)
- 级别:table
- 里程碑:M5
- 前置:A0;H03 含段 5、6
- 实现:`apps/journal-archiver/tests/upload.rs::arc_109_uploaded_marker`

| 行 | 条件 | 期望 |
|---|---|---|
| ARC-109.1 | 提交成功 | `0000000005-00000000000000040001.uploaded` 与段 6 的同名标记出现,内容为 manifest 键与其 SHA-256;出现时刻晚于 manifest 的 `PUT` 与提交证明的写入 |
| ARC-109.2 | manifest `PUT` 之后、写 `.uploaded` 之前崩溃,重启 | manifest `HEAD` 为 `AlreadyPresent`;随后写出两个 `.uploaded`;manifest 没有第二次 `PUT` |
| ARC-109.3 | H03 quarantine | 段 5、6 旁没有 `.uploaded` |

### ARC-110 对冷副本目录只有写 .uploaded 与删除已提交的段两种写

- 规则:[上传与删除纪律](README.md#上传与删除纪律)、[概要](README.md#概要)
- 级别:proc
- 里程碑:M5
- 前置:Linux;A0;对 `<cold_root>` 开 `fanotify`(或 `inotifywait -r -m`)记录全部写事件;archiver 运行 6 h(含提交、retention 删除超过保留期的段)
- 步骤:
  1. 汇总 archiver 进程对 `<cold_root>` 下的创建、写、改名与删除事件
- 期望:
  - 创建与写入的文件名全部以 `.uploaded` 结尾;删除的都是已有 `.uploaded` 的段的 `.jrn`、`.seal`、`.uploaded` 与变空的日期目录;没有改名;没有对 `.jrn`/`.seal` 的写打开
- 实现:`apps/journal-archiver/tests/proc/cold_writes.rs::arc_110_cold_dir_writes`

### ARC-111 按保留期删除的条件

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:table
- 里程碑:M5
- 前置:A0(`seq` 7 天、`raw-*` 3 天);按行设定段的封存时刻(`SealRecord.sealed_utc_ns` 距当前)、`.uploaded` 与 S3 核验结果;运行一次 retention
- 实现:`apps/journal-archiver/tests/retention.rs::arc_111_retention_conditions`

| 行 | 条件 | 期望 |
|---|---|---|
| ARC-111.1 | `seq`,7 天 + 1 ns,有 `.uploaded`,核验一致 | `.jrn`、`.seal`、`.uploaded` 删除;日期目录变空后删除 |
| ARC-111.2 | `seq`,恰 7 天 | 不删 |
| ARC-111.3 | `seq`,30 天,无 `.uploaded` | 不删 |
| ARC-111.4 | `seq`,7 天 + 1 ns,有 `.uploaded`,manifest 中一个 Parquet 对象在 S3 上缺失 | 不删;该小时 quarantine;告警 `Quarantine` |
| ARC-111.5 | 同上,manifest 自身的 SHA-256 与提交证明不符 | 不删;quarantine |
| ARC-111.6 | `raw-gw-binance-usdm`,3 天 + 1 ns,有 `.uploaded`,核验一致 | 删除 |
| ARC-111.7 | `raw-gw-binance-usdm`,恰 3 天 | 不删 |
| ARC-111.8 | 未封存(无 `.seal`),30 天 | 不删 |

### ARC-112 同一 journal 的待删段中有一个小时核验不符时,本次不删任何段

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0;`prod` `seq` 的 2026-10-24 H01、H02 都超过 7 天且有 `.uploaded`,H02 的一个对象在 S3 上缺失;`prod` `raw-gw-binance-usdm` 有一个超过 3 天且核验一致的段
- 步骤:
  1. 运行一次 retention
- 期望:
  - `prod` `seq` 的 H01、H02 都不删(没有调用 `cold_retire`);H02 quarantine 并告警;`raw-gw-binance-usdm` 的那个段被删除
- 实现:`apps/journal-archiver/tests/retention.rs::arc_112_verify_gates_journal`

### ARC-113 磁盘水位提前删除与 P0

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:table
- 里程碑:M5
- 前置:A0(`retention.min_local_hours = 24`);冷副本卷使用率与段按行(段都在 7 天以内、已提交且核验一致,除非另写);运行一次 retention
- 实现:`apps/journal-archiver/tests/retention.rs::arc_113_watermark`

| 行 | 条件 | 期望 |
|---|---|---|
| ARC-113.1 | 使用率 79%,段封存于 25 h 前 | 不删 |
| ARC-113.2 | 使用率 80%,段 A 封存于 25 h 前、段 B 于 23 h 前 | A 删除、B 保留 |
| ARC-113.3 | 使用率 80%,段封存于恰 24 h 前 | 不删 |
| ARC-113.4 | 使用率 95%,删除 A 后仍为 91% | 告警 `DiskFull`(P0) |
| ARC-113.5 | 使用率 95%,删除后为 85% | 无 `DiskFull` |
| ARC-113.6 | 使用率 95%,只有未提交(无 `.uploaded`)的段,封存于 3 天前 | 不删;告警 `DiskFull`(P0) |

### ARC-114 提交证明、审计与 manifest 从不删除

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:integ
- 里程碑:M5
- 前置:A0;2026-10-20 至 11-02 的小时都已提交,10-20 至 10-25 的段已按保留期删除
- 步骤:
  1. 运行 retention 与水位删除各一次
- 期望:
  - `<state>/committed/` 下 10-20 起每个小时的提交证明仍在;`audit.jsonl` 行数不减;假 S3 没有收到任何 `DELETE`
- 实现:`apps/journal-archiver/tests/retention.rs::arc_114_never_delete_proofs`

### ARC-116 告警行的字段与种类

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:table
- 里程碑:M5
- 前置:A0;每行触发一种情形后读 `<state>/alerts.jsonl` 新增的行
- 实现:`apps/journal-archiver/tests/alerts.rs::arc_116_alert_lines`

| 行 | 情形 | 期望的行 |
|---|---|---|
| ARC-116.1 | `prod` `seq` H03 链断裂 | `{level: "P0", kind: "ChainBreak", prefix: "hft", instance: "prod", journal: "seq", hour: "2026-11-02T03", detail: …}`,另有同级 `tracing` 事件 |
| ARC-116.2 | 水位 91% 无可删 | `level: "P0"`、`kind: "DiskFull"` |
| ARC-116.3 | H03 `Conflict` | `level: "P1"`、`kind: "Quarantine"` |
| ARC-116.4 | files source `Conflict` | `level: "P1"`、`kind: "Quarantine"`,`journal` 为 files source 名 `verify` |
| ARC-116.5 | manifest `gaps` 非空 | `level: "P1"`、`kind: "Gap"` |
| ARC-116.6 | 重试 8 次耗尽 | `level: "P1"`、`kind: "RetryExhausted"` |
| ARC-116.7 | `restore` 发现未提交对象 | `level: "P1"`、`kind: "Orphans"` |
| ARC-116.8 | 策略日报遇到未知 `kind_id` | `level: "P1"`、`kind: "UnknownKindId"` |
| ARC-116.9 | archiver 重启 | 重启前的行全部仍在;文件只追加 |

### ARC-120 DurableMeta 的键文件变化即上传为新对象

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:table
- 里程碑:M5
- 前置:A0;`<cold_root>/prod/meta/` 中各行的写出(hft-journal `write_sync`:写 `<key>.tmp` → rename);inotify 按行
- 实现:`apps/journal-archiver/tests/restore.rs::arc_120_durable_meta_upload`

| 行 | 写出 | 期望 |
|---|---|---|
| ARC-120.1 | 03:00:00 `engine_epoch` = 41 | 2 min 内 `PUT` `hft/tokyo-cex-1/prod/private/meta/engine_epoch.20261102T030000Z.<哈希 12 位>`,带 SSE-KMS,内容与键文件逐字节相同;不等 `settle_s` |
| ARC-120.2 | 03:05 以相同内容再写一次 | 没有 `PUT` |
| ARC-120.3 | 03:10 `engine_epoch` = 42 | 新对象 `…engine_epoch.20261102T031000Z.<哈希>`;.1 的键没有第二次 `PUT` |
| ARC-120.4 | `engine_epoch.tmp` 出现后被 rename | `.tmp` 从未被上传 |
| ARC-120.5 | inotify 关闭,03:20 写 `engine_epoch` = 43 | 60 s 内由兜底扫描上传 |
| ARC-120.6 | `boot_guard` 键文件变化 | 同样上传为 `…/meta/boot_guard.<UTC>.<哈希>` |

### ARC-121 restore 的前提不满足时退出 2,不写任何文件

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:A0;假 S3 中 `hft/tokyo-cex-1/prod/` 有 30 个已提交小时;运行 `journal-archiver restore --config <A0> --instance prod --yes`(按行改参数或环境)
- 实现:`apps/journal-archiver/tests/restore.rs::arc_121_preconditions`

| 行 | 条件 | 期望 |
|---|---|---|
| ARC-121.1 | 实例 `prod` 的核心单元 `active` | 退出 2;`<cold_root>` 与本 app 状态目录下没有新文件;没有对段对象的 `GET` |
| ARC-121.2 | `hft-journal-archiver` `active` | 同 .1 |
| ARC-121.3 | `<cold_root>/prod/` 下有一个文件 | 同 .1;该文件未被改动 |
| ARC-121.4 | 本地状态已有 `(hft, prod)` 的游标 | 同 .1 |
| ARC-121.5 | 不带 `--yes` | 同 .1 |
| ARC-121.6 | `<cold_root>/prod/` 存在且为空,其余满足 | 继续执行,退出 0 |
| ARC-121.7 | `<cold_root>/prod/` 不存在,其余满足 | 继续执行,退出 0 |

### ARC-122 restore 取回最后 24 个已提交小时并逐段核验

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:integ
- 里程碑:M5
- 前置:A0;假 S3 中 `prod` 的 `seq` 已提交 2026-11-01T00 至 2026-11-02T05 共 30 小时,每小时一个段;`raw-gw-binance-usdm` 同样已提交
- 步骤:
  1. `journal-archiver restore --instance prod --yes`
- 期望:
  - 写入 2026-11-01T06 至 2026-11-02T05 的 24 个 `seq` 段及其 `.seal` 到 `<cold_root>/prod/seq/<yyyymmdd>/`;每段解压后的 SHA-256(3072..3200 置零)等于 manifest 的 `seal_sha256` 与 `.seal` 中的值;`hft_journal::verify` 对取回的段链无问题(第一段的前驱不在本地不算问题)
  - 没有任何 `raw-gw-binance-usdm` 对象的 `GET`;退出 0
- 实现:`apps/journal-archiver/tests/restore.rs::arc_122_restore_24_hours`

### ARC-123 restore 的段校验失败时停止并退出 1,已写的文件保留

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:table
- 里程碑:M5
- 前置:同 ARC-122;按行注入故障(第 10 个小时)
- 实现:`apps/journal-archiver/tests/restore.rs::arc_123_restore_verify_failure`

| 行 | 故障 | 期望 |
|---|---|---|
| ARC-123.1 | 该小时 `.jrn.zst` 解压后的 SHA-256 ≠ manifest `seal_sha256` | 退出 1;前 9 个小时的段已以正式文件名在本地,第 10 小时起没有;输出指明键与两边哈希;`meta/engine_epoch` 未写、没有 `.uploaded` |
| ARC-123.2 | 该小时 `.seal` 对象的 `sha256` ≠ manifest `seal_sha256` | 退出 1;`meta/engine_epoch` 未写、没有 `.uploaded` |
| ARC-123.3 | 第 10 小时的段对象缺失(manifest 存在) | 同 .2 |
| ARC-123.4 | 第 10 小时段的 `prev_header_sha256` 与第 9 小时段不接 | 同 .2(取回段链校验不通过) |

### ARC-124 未提交的对象不取回,列进 orphans.json 并报 P1

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:integ
- 里程碑:M5
- 前置:同 ARC-122;另有 2026-11-02T06 的段对象与 2 个 Parquet 对象已在 S3、该小时没有 manifest
- 步骤:
  1. `restore --instance prod --yes`
- 期望:
  - H06 的对象没有被 `GET`;`<state>/restore/1/orphans.json` 列出这 3 个键;告警 `Orphans`(P1);退出 0
- 实现:`apps/journal-archiver/tests/restore.rs::arc_124_orphans`

### ARC-125 restore 写好本地状态,随后 run 不重传这些小时

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:integ
- 里程碑:M5
- 前置:ARC-122 完成之后
- 步骤:
  1. 检查本地状态;启动 `run` 运行 30 min(没有新段)
- 期望:
  - 24 个段旁各有 `.uploaded`(内容 = 对应 manifest 键与 SHA-256);`<state>/committed/hft/prod/seq/` 有 24 个提交证明;游标 `last_committed_hour = 2026-11-02T05`、`last_seq` = 最后一段的 `last_seq`;`audit.jsonl` 有每小时的恢复记录
  - `run` 期间对这 24 个小时的键没有任何 `PUT`
- 实现:`apps/journal-archiver/tests/restore.rs::arc_125_local_state_after_restore`

### ARC-126 写入的 engine_epoch = max(S3 上的值, 最新快照的值) + epoch_gap

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:table
- 里程碑:M5
- 前置:同 ARC-122;S3 上 `meta/engine_epoch.*` 对象的值、取回 journal 最新快照的 `engine_epoch` 与 `restore.epoch_gap` 按行
- 实现:`apps/journal-archiver/tests/restore.rs::arc_126_epoch_gap`

| 行 | S3 上的值,快照值,`epoch_gap` | 期望 `meta/engine_epoch`(以 `DurableMeta::read` 读回) |
|---|---|---|
| ARC-126.1 | {38, 40},37,64 | 104 |
| ARC-126.2 | 无对象,37,64 | 101 |
| ARC-126.3 | {45},50,64 | 114 |
| ARC-126.4 | {40},37,1 | 41 |

### ARC-127 恢复后的启动 epoch 大于故障主机用过的任何 epoch

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)、[hft-core 启动顺序](../hft-core/README.md#启动顺序)
- 级别:integ
- 里程碑:M5
- 前置:故障主机最后一次上传的 `engine_epoch` = 40,之后又启动 63 次(用过的最大 epoch 103,未上传);ARC-126.1 的恢复结果(104)
- 步骤:
  1. 以 hft-core 启动顺序步 5 的公式(快照 37、`DurableMeta` 104)计算新主机 `--recover-from-snapshot` 启动的 epoch
- 期望:
  - 新 epoch = 105 > 103;以它生成的 `ClientOrderId` 与故障主机 epoch 41..103 生成的任何 id 都不同
- 实现:`apps/journal-archiver/tests/restore.rs::arc_127_epoch_after_recovery`

### ARC-128 恢复代取已有 manifest 的最大 restore_gen + 1,续写的同号段零 Conflict

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)、[输出:S3 布局](README.md#输出s3-布局)
- 级别:table
- 里程碑:M5
- 前置:同 ARC-124(H06 的段 31 已部分上传、未提交);S3 上已有 manifest 的 `restore_gen` 按行
- 实现:`apps/journal-archiver/tests/restore.rs::arc_128_restore_gen`

| 行 | 已有 manifest 的 `restore_gen` | 期望 |
|---|---|---|
| ARC-128.1 | 全部为 0 | 本地状态恢复代 `g = 1`;新主机 `run` 写出的段 31 键为 `…-<first_seq>.r1.jrn.zst`,`PUT` 成功,零 `Conflict`;H06 的 manifest 为 `manifest.r1.json`、`restore_gen = 1` |
| ARC-128.2 | 最大为 1(此前已恢复过一次) | `g = 2`;对象带 `.r2` |

### ARC-129 restore 的输出与退出码

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:unit
- 里程碑:M5
- 前置:ARC-122 与 ARC-126.1 的输入
- 步骤:
  1. 读 `restore` 的 stdout
- 期望:
  - 含取回的小时范围 `2026-11-01T06`..`2026-11-02T05`、段数 24、最新快照的 seq 与 `engine_epoch` 37、写入的 `engine_epoch` 104 与恢复代 1;退出码 0
- 实现:`apps/journal-archiver/tests/restore.rs::arc_129_output`

### ARC-130 --journals 与 --hours

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:table
- 里程碑:M5
- 前置:A0;假 S3 中 `prod` 的 `seq` 与 `raw-gw-binance-usdm` 已提交的小时数按行
- 实现:`apps/journal-archiver/tests/restore.rs::arc_130_journals_and_hours`

| 行 | 参数,已提交小时数 | 期望 |
|---|---|---|
| ARC-130.1 | `--journals seq,raw-gw-binance-usdm`,30 | 两个 journal 各取回 24 小时,raw 段写到 `<cold_root>/prod/raw-gw-binance-usdm/` |
| ARC-130.2 | 不带 `--hours`,只有 10 个已提交小时 | 取回 10 小时 |
| ARC-130.3 | `--hours 1`,30 | 只取回最后 1 小时 |
| ARC-130.4 | `--prefix hft-testnet`,该前缀下 5 小时、`hft` 下 30 小时 | 取回 `hft-testnet` 下的 5 小时;对 `hft/` 键没有 `GET` |

### ARC-131 S3 上的 engine_epoch 对象不可解析时退出 1

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:integ
- 里程碑:M5
- 前置:同 ARC-122;S3 上两份 `engine_epoch` 对象,其中一份 CRC 行不符
- 步骤:
  1. `restore --instance prod --yes`
- 期望:
  - 退出 1;`meta/engine_epoch` 未写;输出指明该对象键
- 实现:`apps/journal-archiver/tests/restore.rs::arc_131_epoch_object_corrupt`

### ARC-132 restore 只写 engine_epoch,不恢复毒丸计数

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:integ
- 里程碑:M5
- 前置:同 ARC-122;S3 上另有 `meta/boot_guard.*` 与 `meta/poison_input.*` 对象
- 步骤:
  1. `restore --instance prod --yes`
- 期望:
  - `<cold_root>/prod/meta/` 中只有 `engine_epoch` 一个键文件;`DurableMeta::read("boot_guard")` 返回 `None`
- 实现:`apps/journal-archiver/tests/restore.rs::arc_132_only_engine_epoch`

### ARC-140 未写出的键取缺省值

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:A0 去掉本行所列键后加载
- 实现:`apps/journal-archiver/tests/config.rs::arc_140_defaults`

| 行 | 键 | 期望的生效值 |
|---|---|---|
| ARC-140.1 | `parquet.zstd_level` | 3 |
| ARC-140.2 | `parquet.row_group_rows` / `parquet.row_group_bytes` | 1 000 000 / 134 217 728 |
| ARC-140.3 | `parquet.min_row_group_rows` | 100 000 |
| ARC-140.4 | `parquet.small_symbol_rows` | 100 000 |
| ARC-140.5 | `parquet.max_file_bytes` | 10 737 418 240 |
| ARC-140.6 | `retention.local_days` | `seq` 7、`raw-*` 3、`chain-*` 7 |
| ARC-140.7 | `retention.min_local_hours` | 24 |
| ARC-140.8 | `upload.max_attempts` / `upload.backoff` | 8 / `{base_s = 30, max_s = 1800}` |
| ARC-140.9 | `threads.convert` | 2 |
| ARC-140.10 | `paths.advisor_daily` | `/var/lib/hft/archiver/advisor-daily` |
| ARC-140.11 | `restore.epoch_gap` | 64 |
| ARC-140.12 | `account_daily.enabled` / `strategy_daily.enabled` | `true` / `true` |
| ARC-140.13 | journal source 的 `prefix` | 等于 `s3.prefix`(`hft`) |
| ARC-140.14 | files source 的 `prefix` / `settle_s` | `hft/verify` / 60 |
| ARC-140.15 | journal source 的 `kind` | `journal` |

### ARC-145 小时单元的步骤顺序

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:A0;H03 为标准小时段;测试台记录单元内各步骤的开始与结束
- 步骤:
  1. 提交 H03
- 期望:
  - 依次为:封存条件与段链校验 → 压缩原件(`.jrn.zst`)并算其 SHA-256 → 记录分派与 Parquet 写出 → 配置 blob → 两条计数等式 → 生成 manifest → 上传 → 提交证明与审计行 → 游标(写 `<state>/cursor/hft/prod/seq.json.new` 后 rename)
- 实现:`apps/journal-archiver/tests/units.rs::arc_145_unit_steps`

### ARC-146 journal 原件以 zstd level 9 压缩,解压后与段文件逐字节相同

- 规则:[实现说明](README.md#实现说明)、[每小时 manifest](README.md#每小时-manifest)
- 级别:unit
- 里程碑:M5
- 前置:标准小时段 5
- 步骤:
  1. 生成 `.jrn.zst`,读 zstd 帧头并解压
- 期望:
  - 帧参数记录压缩级别 9;解压结果与段文件逐字节相同;manifest 的 `zst_bytes`、`zst_sha256` 等于压缩文件的长度与 SHA-256
- 实现:`apps/journal-archiver/tests/units.rs::arc_146_zstd_original`

### ARC-147 不同 journal 并行转换,同一 journal 的小时串行

- 规则:[实现说明](README.md#实现说明)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:A0;`prod` `seq` 与 `raw-feed-binance-usdm` 各有 H03、H04 待处理,每个单元转换需 10 s(注入);`threads.convert` 按行
- 实现:`apps/journal-archiver/tests/units.rs::arc_147_parallelism`

| 行 | `threads.convert` | 期望 |
|---|---|---|
| ARC-147.1 | 2 | `seq` 与 `raw-feed-binance-usdm` 的单元转换区间有重叠;同一 journal 的 H03 与 H04 的区间不重叠且 H03 在前 |
| ARC-147.2 | 1 | 任意两个单元的转换区间都不重叠 |

### ARC-148 被回放暂停中途停止后,重启不重传、游标不倒退

- 规则:[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M5
- 前置:Linux;真实 archiver 进程 + MinIO;H03 的上传进行到一半
- 步骤:
  1. 向 archiver 发 SIGTERM(`hft-replay-verify` 的 `ExecStartPre` 停止它的效果);等进程退出;再启动
- 期望:
  - 重启后 H03 已上传的对象只 `HEAD`;游标值不小于停止前;H03 最终提交恰好一次
- 实现:`apps/journal-archiver/tests/proc/pause.rs::arc_148_pause_resume`

### ARC-149 读冷副本用 SEQUENTIAL 并在读后 DONTNEED

- 规则:[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M5
- 前置:Linux;`strace -f -e trace=openat,fadvise64`;H03 段 5
- 步骤:
  1. 提交 H03
- 期望:
  - 打开段 5 冷副本之后有 `fadvise64(fd, 0, 0, POSIX_FADV_SEQUENTIAL)`;读完之后有对已读范围的 `POSIX_FADV_DONTNEED`
- 实现:`apps/journal-archiver/tests/proc/fadvise.rs::arc_149_fadvise`

### ARC-150 不读 RAM 热段

- 规则:[概要](README.md#概要)、[输入:什么是"已封存"](README.md#输入什么是已封存)
- 级别:proc
- 里程碑:M5
- 前置:Linux;hft-testkit 写者在临时 tmpfs 热目录与冷副本目录写 `prod` `seq`(热段与冷副本同时存在);A0 的 `cold_root` 指向冷副本根;`strace -f -e trace=openat,mmap`
- 步骤:
  1. 运行 archiver 2 h(假时钟),提交两个小时
- 期望:
  - strace 中没有对热目录(tmpfs 与两个 hugetlbfs 挂载点)下任何路径的 `openat`;读取的段文件全部位于 `<cold_root>` 之下
- 实现:`apps/journal-archiver/tests/proc/hot_untouched.rs::arc_150_no_hot_segments`

### ARC-155 seq 转换吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M5
- 基准名:`archiver_convert_seq_1h`
- 实现:`apps/journal-archiver/benches/convert.rs::arc_155_convert_seq_1h`

### ARC-156 小时结束到 manifest 提交的滞后

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M6
- 门槛:⑦
- 环境:dev2,`sim` 实例连续运行期间;另在 M7 的统一升级主机上重复
- 授权:无
- 步骤:连续 7 天,对每个已提交小时计算 审计行中 manifest 的提交时刻 − `hour_utc` − 1 h
- 通过标准:`seq` 全部 ≤ 15 min;`raw-*` 全部 ≤ 30 min
- 证据:`ops-log/archiver/ARC-156-<主机>-<起始日期>.csv`
- 实现:`tests/host/ARC-156.sh`

### ARC-157 检查点 rename 到本地复制完成 ≤ 2 min

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M6
- 环境:dev2,`sim` 实例连续运行期间
- 授权:无
- 步骤:连续 7 天,对每日归档的检查点比较 `advisor_dir` 中该文件的 mtime 与审计行中本地复制完成的时刻
- 通过标准:每天每个策略实例之差 ≤ 120 s
- 证据:`ops-log/archiver/ARC-157-<起始日期>.csv`
- 实现:`tests/host/ARC-157.sh`

### ARC-158 archiver 运行不使 persist 滞后 p99 上升超过 10%

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,`prod` 与 `sim` 运行
- 授权:[需授权]
- 步骤:archiver 关闭 24 h、开启 24 h(补传关闭期间的积压),两段各取 watchtower 的 `journal_persist_lag_ms` 直方图
- 通过标准:开启段的 p99 ≤ 关闭段的 p99 × 1.10
- 证据:`ops-log/archiver/ARC-158-<日期>.json`(两段直方图)
- 实现:`tests/host/ARC-158.sh`

### ARC-159 CPU 与内存在 hft-core 规定的范围内,不占热核

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[实现说明](README.md#实现说明)
- 级别:host
- 里程碑:M5
- 环境:dev2;另在 M7 的统一升级主机上重复
- 授权:无
- 步骤:连续 24 h 读本单元的 `memory.peak` 与 `cpu.stat`;每小时读一次各线程的 CPU 亲和性(`taskset -p`)
- 通过标准:`memory.peak` ≤ 该主机类内存表给本单元的上限;各线程的亲和性只含线程计划之外的核(不含 `prod` 的任何热核)
- 证据:`ops-log/archiver/ARC-159-<主机>-<日期>.json`
- 实现:`tests/host/ARC-159.sh`

### ARC-160 依赖方向

- 规则:[依赖](README.md#依赖)
- 级别:table
- 里程碑:M5
- 前置:夹具工作区,按行给 `apps/journal-archiver` 加依赖后运行 `cargo xtask deps`
- 实现:`apps/journal-archiver/tests/deps.rs::arc_160_dependency_rules`

| 行 | 加入的依赖 | 期望 |
|---|---|---|
| ARC-160.1 | `hft-journal`,启用 `writer` | 退出 1 |
| ARC-160.2 | `hft-ctl` | 退出 1 |
| ARC-160.3 | `strategies/latency-probe` | 退出 1 |
| ARC-160.4 | `hft-strategy-registry` | 退出 1 |
| ARC-160.5 | `hft-venue` | 退出 1 |
| ARC-160.6 | `arrow`、`parquet`(zstd)、`zstd`、`sha2`、`aws-sdk-s3`、`hft-telemetry` | 退出 0 |

### ARC-161 vendored 的 s3-store 函数注明来源并带原测试

- 规则:[依赖](README.md#依赖)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:`apps/journal-archiver/src/s3_vendored.rs` 与 `apps/journal-archiver/tests/s3_vendored.rs`
- 步骤:
  1. 读源文件头;运行测试文件
- 期望:
  - 源文件头注释含来源仓库 `Trading`、路径 `crates/s3-store` 与 40 位十六进制 commit;测试文件含从 Trading 拷来的 `ensure_file` 用例(`AlreadyPresent`、`Conflict`、新建三种)并全部通过;`Cargo.toml` 没有指向 Trading 仓库的依赖
- 实现:`apps/journal-archiver/tests/s3_vendored.rs::arc_161_vendored_provenance`

### ARC-165 从 S3 下载的 journal 原件可直接回放验证

- 规则:[每小时 manifest](README.md#每小时-manifest)、[输出:S3 布局](README.md#输出s3-布局)
- 级别:integ
- 里程碑:M5
- 前置:A0;hft-testkit 测试策略的 `prod` 夹具 journal 一小时,已提交
- 步骤:
  1. 从假 S3 下载该小时 `.jrn.zst` 与 `.seal`,解压
  2. 计算 SHA-256(3072..3200 置零);运行 `replayer verify --journal <目录> --instance prod`
- 期望:
  - 哈希等于 manifest 的 `segments[].seal_sha256`;replayer 退出 0,报告 `verify = "pass"`
- 实现:`apps/journal-archiver/tests/compat.rs::arc_165_replay_from_s3`

### ARC-166 按分区过滤能裁剪文件

- 规则:[输出:S3 布局](README.md#输出s3-布局)、[Parquet 规则](README.md#parquet-规则)
- 级别:integ
- 里程碑:M5
- 前置:A0;2026-11-02 H00–H05 已提交到 MinIO,`bbo` 覆盖 3 个 exchange、每个 5 个 symbol
- 步骤:
  1. DuckDB 查询 `hft/tokyo-cex-1/prod/public/bbo/**`,条件 `exchange = 'binance-usdm' AND symbol = 'BTCUSDT' AND date = '2026-11-02' AND hour = '03'`,读查询计划
- 期望:
  - 实际打开的文件数为 1(其余分区被裁剪);结果行数等于该小时 BTCUSDT 的 `Bbo` 记录数;每个文件除末组外 row group ≥ 100 000 行
- 实现:`apps/journal-archiver/tests/compat.rs::arc_166_duckdb_pruning`

### ARC-167 dev2 上经 ACL 写 .uploaded 与删除已提交的段

- 规则:[上传与删除纪律](README.md#上传与删除纪律)
- 级别:host
- 里程碑:M5
- 环境:dev2,按 deploy 路径表建立的 `<cold_root>/sim/`(`hft-sim:hft-journal-ro` 2750,访问 ACL 与默认 ACL `u:hft-archive:rwx`);`hft-core@sim` 运行并在新的一天建立日期目录
- 授权:[需授权]
- 步骤:archiver 以 `hft-archive` 身份运行 48 h;第二天对日期目录执行 `getfacl`;检查 journald 与 archiver 告警
- 通过标准:每个已提交小时的段旁有 `.uploaded`;hft-core 新建的日期目录 `getfacl` 显示 `mask::rwx` 与 `user:hft-archive:rwx`;journald 中 archiver 没有 `EACCES`;将 `retention.local_days.seq` 临时设为 1 后 retention 删除了已提交且超过 1 天的段
- 证据:`ops-log/archiver/ARC-167-<日期>.txt`(`getfacl` 输出、`.uploaded` 列表与日志摘录)
- 实现:`tests/host/ARC-167.sh`

### ARC-168 模拟账户连续运行 7 天的归档验收

- 规则:[每小时 manifest](README.md#每小时-manifest)、[策略日报](README.md#策略日报)
- 级别:host
- 里程碑:M6
- 门槛:⑦
- 环境:dev2 的 `sim` 实例,门槛 ⑦ dev2 段的计时期间(另在 M7 满规模段重复)
- 授权:无
- 步骤:连续 7 天每日检查前一 UTC 日的 `account_daily`、`strategy_daily` 对象与各小时 manifest;第 7 天对 7 天的全部小时以 `hft-archive` 身份运行 `journal-archiver check`
- 通过标准:每天两个日报对象都存在;每个小时的 manifest 已提交;quarantine 为零,或每个 quarantine 有人工结论;`check` 退出 0
- 证据:`ops-log/archiver/ARC-168-<起始日期>.md`,归档到门槛 ⑦ 记录
- 实现:`tests/host/ARC-168.sh`

### ARC-169 换 AZ 恢复演练

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)
- 级别:drill
- 里程碑:M7
- 门槛:⑤
- 环境:统一升级主机(测试网账户)、第二 AZ 的待命网卡与新卷
- 授权:[需授权]
- 步骤:按 [incident-response "交易主机失联"](../../docs/runbooks/incident-response.md#s-交易主机失联) 与 [host-upgrade 第 11 步](../../docs/runbooks/host-upgrade.md#步骤) 的换 AZ 恢复演练:新实例挂待命网卡与新卷 → `journal-archiver restore --instance prod --yes` → `hft-core@prod --recover-from-snapshot` 启动 → 严格对账
- 通过标准:`restore` 退出 0,取回的小时数与段数与 S3 上最后 24 个已提交小时一致;hft-core 启动后的 epoch 大于故障主机用过的任何 epoch(对照故障主机 journal 中的 `Note{EpochStart}`);新主机续写的第一个小时上传零 `Conflict`;`orphans.json` 中的对象由人决定去留并记录
- 证据:演练记录(文件名含 `ARC-169` 与日期),归档到 host-upgrade 记录
- 实现:`tests/host/ARC-169.sh`

### ARC-170 hft-archive 凭证覆盖 restore 所需的读与解密

- 规则:[从 S3 恢复(restore)](README.md#从-s3-恢复restore)、[依赖](README.md#依赖)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,`hft-archive` 的凭证代理文件
- 授权:无
- 步骤:以 `hft-archive` 身份对 `hft/<host>/prod/private/` 下一个 manifest 与一个 `meta/engine_epoch.*` 对象 `GetObject`,对 `hft/<host>/prod/` `ListObjectsV2`;对 `hft-testnet/<host>/prod/` 重复
- 通过标准:全部成功(含 SSE-KMS 对象的解密),没有 `AccessDenied`
- 证据:`ops-log/archiver/ARC-170-<日期>.txt`
- 实现:`tests/host/ARC-170.sh`

### ARC-171 每日检查点的本地副本对回放用户只读

- 规则:[Model 检查点每日归档](README.md#model-检查点每日归档)
- 级别:host
- 里程碑:M5
- 环境:dev2,`/var/lib/hft/archiver/advisor-daily/` 按 deploy 路径表建立
- 授权:无
- 步骤:archiver 归档一份检查点后,`runuser -u hft-replay -- cat <副本>`;`runuser -u hft-replay -- touch <副本所在目录>/x`
- 通过标准:`cat` 成功且内容与 `advisor_dir` 中被归档的那份相同;`touch` 以 `EACCES` 失败
- 证据:`ops-log/archiver/ARC-171-<日期>.txt`
- 实现:`tests/host/ARC-171.sh`

## 验收

- **M5 完成**:本文件中里程碑 ≤ M5、级别为 `unit`、`table`、`golden`、`integ`、`proc` 的用例全部有同编号的测试函数且在 CI 通过(`cargo xtask test-trace`,规则见 [docs/tests §6](../../docs/tests/README.md#6-与代码的对应));`bench` 用例 ARC-155 的数值上限在 [benchmarks 基准名登记](../../benchmarks/README.md#基准名登记) 与本组件"延迟与资源契约"中;ARC-159、ARC-167、ARC-171 的 dev2 记录归档。
- **门槛 ⑦**([testing §8](../../docs/testing.md#8-上线门槛)):模拟账户连续运行期间连续 7 天,每日 `account_daily` 与 `strategy_daily` 生成,每小时 manifest 均提交、零 quarantine(或每个 quarantine 有人工结论),`journal-archiver check` 对 7 天全部小时通过(ARC-168);提交滞后与检查点复制时长(ARC-156、ARC-157)。
- **门槛 ⑤**:M7 期间的换 AZ 恢复演练(ARC-169,第二 AZ 的待命网卡 + 新卷 + `restore` + `--recover-from-snapshot`,测试网账户;步骤与记录归 [host-upgrade 第 11 步](../../docs/runbooks/host-upgrade.md#步骤));ARC-170 的凭证核对先于演练完成。
