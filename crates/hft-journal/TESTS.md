---
component: hft-journal
prefix: JNL
spec: README.md
---

# hft-journal 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本文件同时承担 [journal-format](../../docs/subsystems/journal-format.md) 的格式断言(该页没有自己的 `TESTS.md`);各节对应的用例列在覆盖表之后。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 写者 | JNL-008, JNL-010–JNL-038, JNL-063, JNL-068, JNL-120, JNL-121 |
| 读者 | JNL-002, JNL-008, JNL-012, JNL-074–JNL-088, JNL-103, JNL-118 |
| persist 与后台维护(persist 线程调用) | JNL-008, JNL-012, JNL-042, JNL-044–JNL-054, JNL-091–JNL-093, JNL-107, JNL-114 |
| 恢复与校验 | JNL-026, JNL-036, JNL-055, JNL-056, JNL-062–JNL-068, JNL-089, JNL-090, JNL-118, JNL-119, JNL-124 |
| 持久元数据(engine_epoch 等) | JNL-092, JNL-094–JNL-098, JNL-122 |
| cargo feature | JNL-087, JNL-101, JNL-102, JNL-122, JNL-123 |
| raw journal 模式 | JNL-104 |
| 小段 + 4K 页模式(tmpfs 热段) | JNL-040, JNL-105 |
| 段大小与热段内存 | JNL-043, JNL-099, JNL-100, JNL-106 |
| 配置项 | JNL-011, JNL-012, JNL-028 |
| 实现说明 | JNL-001–JNL-009, JNL-014, JNL-022, JNL-023, JNL-031, JNL-033, JNL-034, JNL-039–JNL-047, JNL-050, JNL-055–JNL-073, JNL-080, JNL-081, JNL-083, JNL-105, JNL-106 |
| 延迟与资源契约 | JNL-048, JNL-088, JNL-108–JNL-117, JNL-119 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | JNL-039, JNL-043, JNL-069 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | JNL-065, JNL-117 |
| 开发备忘 | 不适用:非规范 |

[journal-format](../../docs/subsystems/journal-format.md) 各节对应的用例(该页的规则由本文件验证):

- 三类 journal:JNL-019、JNL-033、JNL-104
- 目录与命名:JNL-010、JNL-041、JNL-099、JNL-101、JNL-124
- 段文件布局、SegmentHeader:JNL-001、JNL-002、JNL-008、JNL-031、JNL-040、JNL-041
- 提交游标:JNL-014–JNL-019、JNL-057–JNL-061、JNL-070、JNL-072
- 记录对齐与 CRC:JNL-009、JNL-014、JNL-058、JNL-082、JNL-121
- 变长与多段记录:JNL-004、JNL-021–JNL-025、JNL-060、JNL-078、JNL-079
- 段滚动与引导块:JNL-028–JNL-037、JNL-056、JNL-071、JNL-077、JNL-119、JNL-120
- 快照记录结构:JNL-004–JNL-007、JNL-038
- 冷副本与热段:JNL-003、JNL-044–JNL-052、JNL-091
- 校验与缺段检测:JNL-073、JNL-089、JNL-090
- 读者 API 概要:JNL-074–JNL-086
- 格式演进:JNL-001、JNL-082

## 用例

**级别分界**(Linux 专用部分):`unit` 默认用堆内存段后端(README"段内存后端",任意平台可跑);标"Linux"的 `unit` 用 `/dev/shm` 下的临时目录(tmpfs)作热目录、`/tmp` 下的临时目录(ext4)作冷目录,单进程,CI 的 `ubuntu-latest` 作业运行(测试建的临时目录——`/dev/shm/hftj-*`、临时目录下的 `hftj-*`——归创建它的测试所有,测试结束时删除,panic 时也删;设 `HFT_TEST_KEEP_TMP=1` 则保留以便排障,见 `tests/common/tmp.rs`);`proc` 是 CI Linux 作业里的多进程(fork 子进程写、`SIGKILL`、跨进程映射),或需要作业准备 hugetlbfs 2 MiB 挂载、`sudo`、`strace` 的用例(写在 `前置`);1 GiB 大页、bpftrace、EBS 卷、ACL 与真实服务用户只在 `host`。

**测试 kind**(hft-event 在 dev 构建登记,`0x7F01`–`0x7F06`):`TIn`(Input,定长 80 B,CRC Never,记录 104 B)、`TOut`(Output,56 B,记录 80 B)、`TMeta`(Meta,32 B,记录 56 B)、`TCrc`(Input,40 B,CRC Always,记录 72 B)、`TMulti`(Multi,Input,CRC Never)、`TVar`(Var,前缀 16 B,`KindInfo.max` = 尾部 65 000 B)。真实 kind 只在需要其语义时使用(`SegmentRoll`、`StateSnapshot`、`RawRx`)。

**`C0`**(seq 写者配置):`name = "seq"`、`kind = Seq`、`instance = Prod`、`page = Normal`、`segment_bytes = 1 048 576`、`hot_keep = 1`、`size_idle_pct = 75`、`size_valve_pct = 90`;`WriterEnv{writer_epoch = 7, build_git = [0xAB; 20], host_profile_sha256 = [0xCD; 32]}`;引导块 `BB0` = `SegmentRoll{Begin}` + `SegmentRoll{BootDone}`(各 72 B,共 144 B,所以 `reserve = 288`)。**`R0`**(raw 写者):`name = "raw-feed-binance-usdm"`、`kind = Raw`、`venue = BINANCE_USDM`,其余同 C0(引导块只有 `Begin`;raw 的全部记录带 CRC,`Begin` 为 80 B,`reserve = 160`)。**`H(cfg)`**:测试装配 `tests/common/harness.rs`——同一线程持有写者、`SegmentPreparer`、`Persister`、`HotRetirer` 与手动推进的 `mono_ns`/`utc_ns`(起点 `utc_ns = 2026-10-01T00:00:00Z`),预创建、复制与退役只在测试显式 `poll` 时发生;"模拟崩溃"= 丢弃写者、预创建者、persist 与退役器而不做任何清理,再以新的环 `open`。标"Linux"的 `unit` 在其他平台上以堆内存后端作热目录照样运行(Linux 专有的断言如 `/proc/self/maps`、`VmLck`、`ru_minflt` 只在 Linux 上检查)。测试与基准目标 `required-features = ["writer", "mem-backend"]`,以 `--all-features` 运行。`proc` 用例所需的 CI 准备经环境变量传入:`HFT_TEST_HUGE2M`(2 MiB hugetlbfs 挂载点)、`HFT_TEST_COLD_SMALL` 与 `HFT_TEST_SUDO`(JNL-048)、`HFT_TEST_SHM8`(JNL-100);未设置时用例打印缺少的前置后返回。**`open0`** = 在空目录上 `open`,以 `Startup` 首次 `roll` 并 `finish(BB0)`:段 1 `Begin` 的 seq = 1、`BootDone` 的 seq = 2,`committed_len = 4 096 + 144 = 4 240`,下一条普通记录 seq = 3。

### JNL-001 SegmentHeader 是 4 096 B 且每个字段偏移固定

- 规则:[journal-format SegmentHeader](../../docs/subsystems/journal-format.md#segmentheader)、[journal-format 段文件布局](../../docs/subsystems/journal-format.md#段文件布局)
- 级别:table
- 里程碑:M1
- 前置:编译期断言与 `offset_of!`(kill-switch 按同一偏移只读段头,偏移是对外契约)
- 实现:`crates/hft-journal/tests/layout.rs::jnl_001_segment_header_layout`

| 行 | 字段 | 偏移 |
|---|---|---|
| JNL-001.1 | `size_of::<SegmentHeader>()`、`align_of` | 4 096、8 |
| JNL-001.2 | `magic`、`format_version`、`journal_kind`、`venue`、`header_len` | 0、8、10、11、12 |
| JNL-001.3 | `journal_name`、`instance`、`segment_no`、`first_seq`、`prev_last_seq` | 16、48、64、72、80 |
| JNL-001.4 | `created_utc_ns`、`created_tsc`、`segment_bytes`、`writer_epoch`、`roll_reason`、`flags`、`_pad0` | 88、96、104、112、116、117、118 |
| JNL-001.5 | `tsc_hz_milli`、`calib_tsc`、`calib_realtime_ns`、`calib_phc_ns`、`calib_err_ns`、`clock_max_error_ns` | 120、128、136、144、152、156 |
| JNL-001.6 | `prev_header_sha256`、`host_profile_sha256`、`build_git`、`_pad1`、`n_schema`、`n_config`、`_pad2` | 160、192、224、244、248、250、252 |
| JNL-001.7 | `schema`(128 × 8 B,`SchemaEntry{kind, version, size}` 8 B)、`config_digest`、`_rsv`(1 756 B)、`header_crc32c` | 256、1 280、1 312、3 068 |
| JNL-001.8 | 写者行 `committed_len`、`last_seq`、`state`、`_w` | 3 072、3 080、3 088、3 092 |
| JNL-001.9 | persist 行 `persisted_len`、`synced_len`、`retired`、`_p`、`_tail` | 3 136、3 144、3 152、3 156、3 200 |
| JNL-001.10 | `SegmentHeaderFixed`(zerocopy 视图)大小 | 3 072 |

### JNL-002 段头 CRC 覆盖 0..3068,写者行与 persist 行不参与

- 规则:[journal-format SegmentHeader](../../docs/subsystems/journal-format.md#segmentheader)、[读者](README.md#读者)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0` 后写 10 条 `TIn` 并 publish;对段 1 的字节就地改一个字节后用 `JournalReader::open(Oldest)` 与 `verify`
- 实现:`crates/hft-journal/tests/layout.rs::jnl_002_header_crc_scope`

| 行 | 改动的字节 | 期望 |
|---|---|---|
| JNL-002.1 | 无 | `header_crc32c == crc32c(header[0..3068])`;读者交付 12 条记录 |
| JNL-002.2 | 偏移 64(`segment_no`)异或 1 | 读者 `poll` → `Err(Corrupt)`;`verify` 报 `{segment_no 1, offset 0, item 1}` |
| JNL-002.3 | 偏移 3 067(`_rsv` 末字节) | 同上 |
| JNL-002.4 | 偏移 3 080(`last_seq`) | 读者照常交付;`verify` 无段头问题 |
| JNL-002.5 | 偏移 3 160(persist 行 `_p`) | 同上 |

### JNL-003 SealRecord 是 128 B,CRC 覆盖前 124 B

- 规则:[journal-format 冷副本与热段](../../docs/subsystems/journal-format.md#冷副本与热段)
- 级别:table
- 里程碑:M1
- 前置:编译期断言
- 实现:`crates/hft-journal/tests/layout.rs::jnl_003_seal_record_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| JNL-003.1 | 大小 | 128 |
| JNL-003.2 | `magic`、`segment_no`、`first_seq`、`last_seq`、`len`、`sealed_utc_ns` | 0、8、16、24、32、40 |
| JNL-003.3 | `sha256`、`reason`、`flags`、`_pad`、`_rsv`、`crc32c` | 48、80、81、82、84、124 |
| JNL-003.4 | `magic` 取值 | `*b"HFTSEAL1"` |

### JNL-004 PartHdr、SnapshotHdr 与 SectionHdr 的布局

- 规则:[journal-format 变长与多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)、[journal-format 快照记录结构](../../docs/subsystems/journal-format.md#快照记录结构)
- 级别:table
- 里程碑:M1
- 前置:hft-event 实现的类型(`var.rs`),本 crate 的测试以 `offset_of!` 断言
- 实现:`crates/hft-journal/tests/layout.rs::jnl_004_part_snapshot_section_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| JNL-004.1 | `PartHdr` 大小;`total_len`、`chunk_len`、`part`、`parts`、`_rsv` | 16;0、4、8、10、12 |
| JNL-004.2 | `SnapshotHdr` 大小;`purpose`、`_pad`、`n_sections`、`at_seq`、`state_hash` | 96;0、1、4、8、16 |
| JNL-004.3 | `SnapshotHdr` 的 `n_books`、`_pad2`、`out_hash`、`snap_tsc`、`boot_id`、`snap_utc_ns`、`engine_ns` | 32、36、40、56、64、80、88 |
| JNL-004.4 | `SectionHdr` 大小;`section`、`version`、`count`、`len`、`_rsv` | 16;0、2、4、8、12 |
| JNL-004.5 | `SnapshotPurpose` 码值 | Boot 1、Keyframe 2、OnDemand 3、Recovery 4、Drain 5 |

### JNL-005 快照节 14 ModeRow 的布局

- 规则:[journal-format 快照记录结构](../../docs/subsystems/journal-format.md#快照记录结构)
- 级别:table
- 里程碑:M2
- 前置:行类型在 hft-risk(`hft-risk::mode`),断言写在该 crate 的测试里
- 实现:`crates/hft-risk/tests/layout.rs::jnl_005_mode_row_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| JNL-005.1 | 大小 | 8 |
| JNL-005.2 | `strategy`、`account`、`mode`、`flags`、`group` | 0、1、2、3、4 |
| JNL-005.3 | `flags` 的位与 `group` | `DEFAULT = 1`、`LANDED = 2`;`DEFAULT` 置位时 `group == 0`(`GROUP_NONE`) |

### JNL-006 快照节 18 GateRow 的布局

- 规则:[journal-format 快照记录结构](../../docs/subsystems/journal-format.md#快照记录结构)
- 级别:table
- 里程碑:M2
- 前置:行类型在 hft-risk
- 实现:`crates/hft-risk/tests/layout.rs::jnl_006_gate_row_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| JNL-006.1 | 大小 | 64 |
| JNL-006.2 | `account`、`released`、`clean_restart`、`recovery_flags`、`gate_bits` | 0、1、2、3、4 |
| JNL-006.3 | `reconciled_venues`、`cfg_ok_venues`、`identity_ok_venues`、`conns_ok_venues`、`ctl_state_version`、`standby_ok_venues`、`gflags` | 8、16、24、32、40、48、56 |

### JNL-007 快照节 20 SeqDayRow 与节 25 EngineClockRow 的布局

- 规则:[journal-format 快照记录结构](../../docs/subsystems/journal-format.md#快照记录结构)
- 级别:table
- 里程碑:M2
- 前置:行类型在 hft-sequencer
- 实现:`crates/hft-sequencer/tests/snapshot_rows.rs::jnl_007_seqday_engineclock_row_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| JNL-007.1 | `SeqDayRow` 大小;`last_day_rolled`、`_pad` | 8;0、4 |
| JNL-007.2 | `EngineClockRow` 大小;`base_engine_ns`、`base_tsc`、`tsc_hz_milli`、`clock_seq` | 32;0、8、16、24 |

### JNL-008 段头码值与本 crate 的错误码

- 规则:[journal-format SegmentHeader](../../docs/subsystems/journal-format.md#segmentheader)、[写者](README.md#写者)、[读者](README.md#读者)、[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:table
- 里程碑:M1
- 前置:`as u8`;`#[repr(u8)]`
- 实现:`crates/hft-journal/tests/layout.rs::jnl_008_code_tables`

| 行 | 码表 | 期望 |
|---|---|---|
| JNL-008.1 | `journal_kind` | Raw 1、Seq 2、Chain 3 |
| JNL-008.2 | `state` | Preparing 0、Open 1、Sealed 2 |
| JNL-008.3 | 段头 `flags` | `RECOVERY_GAP = 1` |
| JNL-008.4 | `InstanceName` | Prod 1、Sim 2 |
| JNL-008.5 | `AppendErr`;`RollErr`;`RollAt` | Full 1、TooLarge 2、Sealed 3、InBoot 4;NextNotReady 1、Io 2、InBoot 3;IdleRound 1、EventBoundary 2 |
| JNL-008.6 | `OpenErr` | DiskBacked 1、OfflineTarget 2、BadConfig 3、Io 4、ChainBroken 5、ColdHotConflict 6、SeqUnavailable 7、NotFound 8 |
| JNL-008.7 | `ReadErr`;`GapReason`;`PrepState` | Corrupt 1、BadCrc 2、ReassemblyCap 3、DevKind 4;MissingSegment 1、RecoveryGap 2、ChainBroken 3、Deleted 4;Ready 1、Prepared 2、Failed 3 |

### JNL-009 记录长度与对齐

- 规则:[journal-format 记录对齐与 CRC](../../docs/subsystems/journal-format.md#记录对齐与-crc)
- 级别:table
- 里程碑:M1
- 前置:hft-event `record_len(body, crc)`;写者按它推进 `off`
- 实现:`crates/hft-journal/tests/framing.rs::jnl_009_record_len`

| 行 | 输入 | 期望 |
|---|---|---|
| JNL-009.1 | `record_len(0, false)` | 24 |
| JNL-009.2 | `record_len(1, false)` | 32 |
| JNL-009.3 | `record_len(80, false)`(`TIn`) | 104 |
| JNL-009.4 | `record_len(40, true)`(`TCrc`) | 72(体末 4 B 填零后跟 CRC) |
| JNL-009.5 | `record_len(65 512, false)` | 65 536 |
| JNL-009.6 | 写者追加 `TIn`、`TCrc`、`TMeta` 后的三个记录起点 | 4 240、4 344、4 416(都是 8 的倍数,首尾相接) |

### JNL-010 段文件命名与目录

- 规则:[journal-format 目录与命名](../../docs/subsystems/journal-format.md#目录与命名)、[写者](README.md#写者)
- 级别:table
- 里程碑:M1
- 前置:Linux;`hot_dir = /dev/shm/<tmp>`、`cold_root = /tmp/<tmp>`;`H(C0)`,`roll` 的 `RollInfo.utc_ns` 按行设定
- 实现:`crates/hft-journal/tests/naming.rs::jnl_010_paths`

| 行 | 情形 | 期望 |
|---|---|---|
| JNL-010.1 | 段 1 的热段 | `<hot_dir>/prod/seq/0000000001.jrn` |
| JNL-010.2 | 段 1 冷副本(`first_seq = 1`,`utc_ns = 2026-10-01T00:00:00Z`) | `<cold_root>/prod/seq/20261001/0000000001-00000000000000000001.jrn` |
| JNL-010.3 | 段 1 封存 | 同目录 `0000000001-00000000000000000001.seal` |
| JNL-010.4 | 段 2 的 `utc_ns = 2026-10-01T23:59:59.999999999Z` | 日期目录 `20261001` |
| JNL-010.5 | 段 3 的 `utc_ns = 2026-10-02T00:00:00Z` | 日期目录 `20261002` |
| JNL-010.6 | `instance = Sim` 的同名 journal | `<hot_dir>/sim/seq/…`、`<cold_root>/sim/seq/…`,与 prod 不共用任何目录 |
| JNL-010.7 | R0 | 热段 `<hot_dir>/prod/raw-feed-binance-usdm/0000000001.jrn`;段头 `journal_kind = 1`、`venue = 1` |
| JNL-010.8 | 段头 `journal_name` 与 `instance` | `"seq"` 右补 0 到 32 B;`"prod"` 右补 0 到 16 B |

### JNL-011 配置与名字的校验

- 规则:[写者](README.md#写者)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1
- 前置:C0 只改所列项后 `JournalWriter::open`(内存后端);期望写返回值
- 实现:`crates/hft-journal/tests/config.rs::jnl_011_config_validation`

| 行 | 改动 | 期望 |
|---|---|---|
| JNL-011.1 | 无 | `Ok` |
| JNL-011.2 | `size_idle_pct = 89`、`size_valve_pct = 90` | `Ok` |
| JNL-011.3 | `size_idle_pct = 90`、`size_valve_pct = 90` | `Err(BadConfig)` |
| JNL-011.4 | `size_idle_pct = 91`、`size_valve_pct = 90` | `Err(BadConfig)` |
| JNL-011.5 | `size_idle_pct = 0` | `Err(BadConfig)` |
| JNL-011.6 | `size_idle_pct = 1` | `Ok` |
| JNL-011.7 | `size_valve_pct = 99` | `Ok` |
| JNL-011.8 | `size_valve_pct = 100` | `Err(BadConfig)` |
| JNL-011.9 | `segment_bytes = 1 048 577`(不是 4 KiB 的倍数) | `Err(BadConfig)` |
| JNL-011.10 | `segment_bytes = 4 096` | `Err(BadConfig)` |
| JNL-011.11 | `segment_bytes = 8 192` | `Ok` |
| JNL-011.12 | `page = Huge2M`、`segment_bytes = 3 MiB` | `Err(BadConfig)` |
| JNL-011.13 | 实例名字符串 `"Prod"`、`"test"`、`""` 解析为 `InstanceName` | 都失败(只有 `"prod"`、`"sim"`) |
| JNL-011.14 | `name` 为 33 字节 | `Err(BadConfig)` |

### JNL-012 参数缺省值

- 规则:[配置项](README.md#配置项)、[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)、[读者](README.md#读者)
- 级别:table
- 里程碑:M1
- 前置:只读常量与 `Default`/构造函数
- 实现:`crates/hft-journal/tests/config.rs::jnl_012_defaults`

| 行 | 项 | 期望 |
|---|---|---|
| JNL-012.1 | `DEFAULT_SIZE_IDLE_PCT`、`DEFAULT_SIZE_VALVE_PCT` | 75、90 |
| JNL-012.2 | `PersistConfig::default()` | `chunk_bytes = 4 194 304`、`sync_bytes = 1 048 576`、`sync_interval_ms = 20` |
| JNL-012.3 | `ReaderConfig::new(…)` | `reassembly_cap = 268 435 456`、`tail_poll_us = 50`、`start = Oldest`、`end = Follow`、`on_gap = Error` |
| JNL-012.4 | `ColdRetention::for_kind(Seq)`、`(Chain)`、`(Raw)` | `keep_days` 7、7、3;`require_uploaded` 都为 true |
| JNL-012.5 | `RETIRE_GRACE_MS` | 2 000 |

### JNL-013 append 返回的 seq 逐条加一,是 seq 的唯一来源

- 规则:[写者](README.md#写者)、[journal-format 记录对齐与 CRC](../../docs/subsystems/journal-format.md#记录对齐与-crc)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`
- 步骤:
  1. 追加 `TIn`(tsc 1 000)、`TOut`(tsc 1 000)、`TMeta`(tsc 1 001)
  2. 读 `next_seq()`
- 期望:三次返回 `Seq(3)`、`Seq(4)`、`Seq(5)`;`next_seq() == 6`;三条记录头的 `seq` 字段分别为 3、4、5,`tsc` 为 1 000、1 000、1 001
- 实现:`crates/hft-journal/tests/writer.rs::jnl_013_seq_is_assigned_by_append`

### JNL-014 记录头、体与填充按格式写出

- 规则:[journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标)、[journal-format 记录对齐与 CRC](../../docs/subsystems/journal-format.md#记录对齐与-crc)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;`TIn` 体为 80 个 0x11;`TCrc` 体为 40 个 0x22
- 步骤:
  1. `append(TIn)`、`append(TCrc)`、`end_event()`、`publish()`
  2. 读段内存 [4 240, 4 416)
- 期望:
  - 4 240 处:`len = 104`、`kind = 0x7F01`、`flags = 0`、`seq = 3`;[4 264, 4 344) 为 80 个 0x11
  - 4 344 处:`len = 72`、`kind = 0x7F04`、`flags = CRC | EVENT_END`、`seq = 4`;[4 368, 4 408) 为 0x22,[4 408, 4 412) 为 0,[4 412, 4 416) = `crc32c(段[4 344, 4 412))`(按最终 flags 计算)
  - [4 416, 4 416 + 4 096) 全零
- 实现:`crates/hft-journal/tests/writer.rs::jnl_014_record_bytes`

### JNL-015 publish 之前 committed_len 不动,之后推进到事件边界

- 规则:[写者](README.md#写者)、[journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;一个同进程读者 `Tail` 起步
- 步骤:
  1. `append(TIn)`、`append(TOut)`(不 publish)
  2. 读者 `poll`;读段头写者行
  3. `end_event()`、`publish()`;读者 `poll` 两次
- 期望:
  - 步骤 2:`committed_len == 4 240`、`last_seq == 2`;读者返回 `Pending`
  - 步骤 3:`committed_len == 4 424`、`last_seq == 4`;读者依次交付 seq 3、4
- 实现:`crates/hft-journal/tests/writer.rs::jnl_015_publish_moves_committed`

### JNL-016 事件中途调用 publish 不暴露未结束事件的记录

- 规则:[写者](README.md#写者)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;事件 E1 = [`TIn`, `TOut`] 已 `end_event` + `publish`(`committed_len = 4 424`)
- 步骤:
  1. 事件 E2:`append(TIn)`(seq 5),然后 `publish()`
  2. `append(TOut)`(seq 6)、`end_event()`、`publish()`
- 期望:步骤 1 后 `committed_len == 4 424`、`last_seq == 4`;步骤 2 后 `committed_len == 4 608`、`last_seq == 6`,seq 6 带 `EVENT_END`、seq 5 不带
- 实现:`crates/hft-journal/tests/writer.rs::jnl_016_publish_midevent_stops_at_boundary`

### JNL-017 end_event 把 EVENT_END 置在最后一条非 Meta 记录上

- 规则:[写者](README.md#写者)、[journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`;每行是一个事件内的追加序列,随后 `end_event()`;"M" 为 `append_meta(TMeta)`
- 实现:`crates/hft-journal/tests/writer.rs::jnl_017_end_event_placement`

| 行 | 事件内的序列 | 期望(带 `EVENT_END` 的记录) |
|---|---|---|
| JNL-017.1 | `TIn`、`TOut`、`TOut` | 只有第三条 |
| JNL-017.2 | `TIn`、`TOut`、M | 只有 `TOut`;M 不带 |
| JNL-017.3 | `TIn`、M、`TOut` | 只有 `TOut` |
| JNL-017.4 | 仅 `TIn` | `TIn` |
| JNL-017.5 | `TIn`、`TMulti`(3 段) | 只有 `TMulti` 末段;前两段带 `PART` 不带 `EVENT_END` |
| JNL-017.6 | 不追加任何记录 | 没有记录被修改(段字节与调用前相同) |
| JNL-017.7 | 第 1 行之后紧接再调一次 `end_event()` | 段字节与第一次之后相同 |

### JNL-018 事件之外的 Meta 自带 EVENT_END,事件之内的不带

- 规则:[写者](README.md#写者)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`
- 实现:`crates/hft-journal/tests/writer.rs::jnl_018_meta_event_end`

| 行 | 序列 | 期望 |
|---|---|---|
| JNL-018.1 | `append_meta(TMeta)`(上一事件已 `end_event`) | 该 `TMeta` 带 `EVENT_END`;`publish` 后 `committed_len` 包含它 |
| JNL-018.2 | `append(TIn)`、`append_meta(TMeta)` | `TMeta` 不带 `EVENT_END`;`end_event` 后 `TIn` 带 |
| JNL-018.3 | 第 1 行后接 `append(TIn)`、`end_event()` | 两条都带 `EVENT_END`(各自独立) |

### JNL-019 raw 写者给每条记录置 EVENT_END,end_event 为空操作

- 规则:[写者](README.md#写者)、[journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标)、[journal-format 三类 journal](../../docs/subsystems/journal-format.md#三类-journal)
- 级别:unit
- 里程碑:M1
- 前置:`H(R0)`,以 `Startup` 首次 `roll` 并 `finish`;`RawRx` 逻辑体 `RawHdr` + 512 B payload,以及一条 70 000 B payload 的 `RawRx`
- 步骤:
  1. `append_var(RawRx, 512 B)`、`append_var(RawRx, 70 000 B)`、`publish()`
  2. `end_event()` 后比较段字节
- 期望:
  - `Begin` 带 `EVENT_END`;第一条 `RawRx` 带 `EVENT_END | CRC`;第二条拆成 2 段,第 0 段 `PART | CRC`、第 1 段 `EVENT_END | CRC`
  - 步骤 2 段字节不变
- 实现:`crates/hft-journal/tests/writer.rs::jnl_019_raw_auto_event_end`

### JNL-020 追加触及预留区时返回 Full 且不写任何字节

- 规则:[写者](README.md#写者)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`(`reserve = 288`,普通追加的上界 = 1 048 576 − 288 = 1 048 288);用 `TVar` 把 `off` 推到行内的值并 publish
- 实现:`crates/hft-journal/tests/writer.rs::jnl_020_full_boundary`

| 行 | `off` | 追加 | 期望 |
|---|---|---|---|
| JNL-020.1 | 1 048 184 | `TIn`(104 B) | `Ok`;`off` 变为 1 048 288 |
| JNL-020.2 | 1 048 192 | `TIn` | `Err(Full)`;`next_seq` 不变;[1 048 192, 1 048 576) 全零 |
| JNL-020.3 | 1 048 192 | `TMeta`(56 B,止于 1 048 248) | `Ok` |
| JNL-020.4 | 1 048 288 | `append_meta(TMeta)` | `Err(Full)` |
| JNL-020.5 | 1 048 192 | 滚动到新段,在引导块内用 `TVar` 把 `off` 推到 1 048 192 后 `BootWriter.append(TIn)`(引导块可用预留区) | `Ok` |
| JNL-020.6 | 1 048 512 | `BootWriter.append(TIn)`(预留区也放不下:1 048 512 + 104 > 1 048 576) | `Err(Full)` |

### JNL-021 TooLarge 的触发条件

- 规则:[写者](README.md#写者)、[journal-format 变长与多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`(空段的普通空间 = 1 048 576 − 4 096 − 288 = 1 044 192 B)
- 实现:`crates/hft-journal/tests/writer.rs::jnl_021_too_large`

| 行 | 调用 | 期望 |
|---|---|---|
| JNL-021.1 | `append_var(TVar, 尾部 65 000 B)` | `Ok`;一条 65 040 B 的记录 |
| JNL-021.2 | `append_var(TVar, 尾部 65 001 B)` | `Err(TooLarge)`;不写字节 |
| JNL-021.3 | `append_var(TMulti, 逻辑体 15 × 65 488 B)`(15 段,982 920 B) | `Ok` |
| JNL-021.4 | `append_var(TMulti, 逻辑体 16 × 65 488 B)`(16 段,1 048 448 B > 1 044 192) | `Err(TooLarge)` |
| JNL-021.5 | `reserve_var(TMulti, u32::MAX)`(`len` 是 `u32`,整组远超一个空段) | `Err(TooLarge)`;不分配、不写字节 |

### JNL-022 多段记录按 65 488 B 切块,段间连续,只有末段不置 PART

- 规则:[journal-format 变长与多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`;`TMulti` 逻辑体为递增字节;`pieces` 分 3 片传入
- 实现:`crates/hft-journal/tests/writer.rs::jnl_022_multipart_split`

| 行 | 逻辑体长度 L | 期望 |
|---|---|---|
| JNL-022.1 | 65 488 | 1 段:记录 65 528 B,`PartHdr{total_len: 65 488, chunk_len: 65 488, part: 0, parts: 1}`,不置 `PART` |
| JNL-022.2 | 65 489 | 2 段:65 528 B + 48 B;`chunk_len` 65 488、1;第 0 段 `PART`,第 1 段不置 |
| JNL-022.3 | 130 977 | 3 段;seq 连续 3、4、5;三段 `kind`、`tsc` 相同;段间没有其他记录;返回 `Seq(3)` |
| JNL-022.4 | 0 | 1 段:记录 40 B,`total_len = 0` |

### JNL-023 多段组不跨段文件:放不下时返回 Full,滚动后写在新段

- 规则:[journal-format 变长与多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;`off` 推到 1 000 000;已预创建段 2
- 步骤:
  1. `append_var(TMulti, 2 × 65 488 B)`(需要 131 056 B,超过剩余 48 288 B)
  2. `roll(Size, info)`、`finish`;再次 `append_var(TMulti, 2 × 65 488 B)`
- 期望:步骤 1 `Err(Full)`,`next_seq`、`off` 与段字节都不变;步骤 2 `Ok`,两段都在段 2,紧接 `BootDone`
- 实现:`crates/hft-journal/tests/writer.rs::jnl_023_multipart_never_spans_segments`

### JNL-024 reserve_var 就地写入;未 commit 就丢弃时不留痕迹

- 规则:[写者](README.md#写者)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`
- 实现:`crates/hft-journal/tests/writer.rs::jnl_024_reserve_var`

| 行 | 步骤 | 期望 |
|---|---|---|
| JNL-024.1 | `s = reserve_var(RawRx 于 R0, 600)`,`s.buf()` 写满 0x5A,`s.commit()` | 返回 seq 等于原 `next_seq`;记录逻辑体为 600 个 0x5A |
| JNL-024.2 | `s = reserve_var(TMulti, 100)`,写 0x77,不 commit 直接 drop | `next_seq` 不变;`off` 不变;[off, off + 4 096) 全零 |
| JNL-024.3 | 第 2 行之后 `append(TIn)` | 返回第 2 行之前的 `next_seq`;该记录完整、后面全零 |

### JNL-025 Var 记录是单条记录,前缀加尾部

- 规则:[journal-format 变长与多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`
- 步骤:
  1. `append_var(TVar, [前缀 16 B, 尾部 1 000 B])`、`end_event`、`publish`
  2. 读者读取
- 期望:一条记录 `len = 1 040`,无 `PartHdr`、不置 `PART`;读者以 `Next::Record` 交付,体长 1 016
- 实现:`crates/hft-journal/tests/writer.rs::jnl_025_var_single_record`

### JNL-026 open 之后首次 roll 之前追加返回 Sealed

- 规则:[写者](README.md#写者)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`;目录里有一个已由 `seal_and_close` 封存的段 1
- 步骤:
  1. `(w, rep) = open(...)`;`w.append(TIn)`
  2. `w.roll(rep.boot_reason, info)`、`finish`;`w.append(TIn)`
- 期望:步骤 1 `Err(Sealed)`,段 1 字节不变;`rep.boot_reason == Startup`;步骤 2 `Ok`,记录在段 2
- 实现:`crates/hft-journal/tests/writer.rs::jnl_026_sealed_before_first_roll`

### JNL-027 引导块未 finish 就丢弃时,append 与 roll 返回 InBoot

- 规则:[写者](README.md#写者)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;已预创建段 2
- 步骤:
  1. `b = roll(Hourly, info)`;`b.append(TMeta)`;`drop(b)`(未 finish)
  2. `append(TIn)`;`roll(Hourly, info)`
- 期望:步骤 2 分别 `Err(AppendErr::InBoot)`、`Err(RollErr::InBoot)`;段 2 的记录只有 `Begin` 与 `TMeta`,没有 `BootDone`;段 2 的 `committed_len == 4 096`(引导块未发布)
- 实现:`crates/hft-journal/tests/writer.rs::jnl_027_dropped_bootwriter`

### JNL-028 两级安全阀的阈值(seq)

- 规则:[写者](README.md#写者)、[配置项](README.md#配置项)、[journal-format 段滚动与引导块](../../docs/subsystems/journal-format.md#段滚动与引导块)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`(`segment_bytes = 1 048 576`、75 / 90、`reserve = 288`);用 `TVar` 把 `committed_len` 推到行内的值并 publish;`now` 任意
- 实现:`crates/hft-journal/tests/roll.rs::jnl_028_size_valves_seq`

| 行 | `committed_len` | `used = committed_len + 288` | `needs_roll` |
|---|---|---|---|
| JNL-028.1 | 786 136 | 786 424 | `None` |
| JNL-028.2 | 786 144 | 786 432(= 75 %) | `Some((Size, IdleRound))` |
| JNL-028.3 | 943 424 | 943 712 | `Some((Size, IdleRound))` |
| JNL-028.4 | 943 432 | 943 720(≥ 90 %) | `Some((Size, EventBoundary))` |
| JNL-028.5 | 1 048 288 | 1 048 576 | `Some((Size, EventBoundary))` |

### JNL-029 raw 写者只有硬上限,另有整点截止;多个条件按优先级

- 规则:[写者](README.md#写者)
- 级别:table
- 里程碑:M1
- 前置:`H(R0)`(`reserve = 160`);`set_roll_deadline(at = 10 000)`(TSC)按行设定;`now` 按行
- 实现:`crates/hft-journal/tests/roll.rs::jnl_029_raw_valve_and_deadline`

| 行 | `committed_len` | deadline / `now` | `needs_roll` |
|---|---|---|---|
| JNL-029.1 | 900 000(> 75 %) | 无 | `None` |
| JNL-029.2 | 943 552 | 无 | `None` |
| JNL-029.3 | 943 560 | 无 | `Some((Size, EventBoundary))` |
| JNL-029.4 | 10 000 | `at = 10 000`,`now = 9 999` | `None` |
| JNL-029.5 | 10 000 | `at = 10 000`,`now = 10 000` | `Some((Hourly, EventBoundary))` |
| JNL-029.6 | 943 560 | `at = 10 000`,`now = 10 000` | `Some((Size, EventBoundary))`(大小优先于整点) |

### JNL-030 预留量随当前段引导块的大小变化

- 规则:[写者](README.md#写者)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`;段 2 的引导块 = `Begin` + 98 条 `TMeta` + `BootDone`(72 + 98 × 56 + 72 = 5 632 B,`reserve = 11 264`)
- 步骤:
  1. 把段 2 的 `committed_len` 推到 775 168、775 160,分别 `needs_roll`
- 期望:775 168(`used` = 786 432)→ `Some((Size, IdleRound))`;775 160 → `None`
- 实现:`crates/hft-journal/tests/roll.rs::jnl_030_reserve_follows_boot_size`

### JNL-031 roll 的产物:End、封存、新段头与 Begin

- 规则:[写者](README.md#写者)、[实现说明](README.md#实现说明)、[journal-format 段滚动与引导块](../../docs/subsystems/journal-format.md#段滚动与引导块)、[journal-format SegmentHeader](../../docs/subsystems/journal-format.md#segmentheader)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;追加到 seq 10(最后一个普通事件);预创建段 2;`RollInfo{utc_ns = U, calib = {tsc: 5 000, realtime_ns: U, phc_ns: 0, err_ns: 40}, tsc_hz_milli = 2 500 000 000 000, configs = [B1, B2, B3]}`(3 个 48 B 的 `BlobRef`)
- 步骤:
  1. `b = roll(Hourly, info)`;`b.finish()`
- 期望:
  - 段 1 末条:`SegmentRoll{mark: End, segment_no: 1, other_segment_no: 2, seq_mark: 10, reason: Hourly}`,seq 11,带 `EVENT_END`;段 1 `state = Sealed`,`committed_len` 等于 End 之后,`last_seq = 11`
  - 段 2 段头:`segment_no = 2`、`first_seq = 12`、`prev_last_seq = 11`、`created_utc_ns = U`、`calib_tsc = 5 000`、`calib_err_ns = 40`、`tsc_hz_milli = 2 500 000 000 000`、`roll_reason = 1`、`flags = 0`、`n_config = 3`、`config_digest = SHA-256(B1 ‖ B2 ‖ B3)`、`header_crc32c == crc32c(段头[0, 3 068))`、`state = Open`
  - 段 2 首条:`SegmentRoll{mark: Begin, segment_no: 2, other_segment_no: 1, seq_mark: 11}`,seq 12,不带 `EVENT_END`
  - 回收环里恰有一个 `RetiredSegment{segment_no: 1}`
- 实现:`crates/hft-journal/tests/roll.rs::jnl_031_roll_products`

### JNL-032 seq 引导块以 BootDone 结束,只有 BootDone 带 EVENT_END

- 规则:[写者](README.md#写者)、[journal-format 段滚动与引导块](../../docs/subsystems/journal-format.md#段滚动与引导块)
- 级别:unit
- 里程碑:M1
- 前置:接 JNL-031 的步骤 1 之前;引导块内追加 `TMeta`(代 HostProfile)、`TMeta`(代 ClockStatus)、`ConfigBlob{boot: 1}` × 3(各 1 段)、`StateSnapshot{Boot}`(3 段)
- 步骤:
  1. `b = roll(...)`,按上列顺序 `b.append*`,`seq_done = b.finish()`,`publish()`
- 期望:`BootDone` 的 `boot_records = 10`(物理记录数:`Begin` + 2 + 3 + 3 段 + 本条);`seq_done` 等于 `BootDone` 的 seq;引导块内只有 `BootDone` 带 `EVENT_END`;`StateSnapshot` 前两段带 `PART`
- 实现:`crates/hft-journal/tests/roll.rs::jnl_032_boot_block_close`

### JNL-033 raw 的滚动只写 Begin,finish 不写记录

- 规则:[实现说明](README.md#实现说明)、[journal-format 三类 journal](../../docs/subsystems/journal-format.md#三类-journal)
- 级别:unit
- 里程碑:M1
- 前置:`H(R0)`,段 1 已写 5 条 `RawRx`;预创建段 2
- 步骤:
  1. `b = roll(Hourly, info)`;`b.finish()`
- 期望:段 1 末条 `End`;段 2 只有一条 `Begin`(带 `EVENT_END`),`finish` 返回其 seq;段 2 中没有 `BootDone`
- 实现:`crates/hft-journal/tests/roll.rs::jnl_033_raw_roll`

### JNL-034 下一段未准备好时不滚动、不写任何记录

- 规则:[写者](README.md#写者)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;预创建通道为空
- 步骤:
  1. `roll(Size, info)`;追加 `TIn`
  2. 预创建段 2;`roll(Size, info)`
- 期望:步骤 1 返回 `Err(NextNotReady)`,段 1 末条不是 `End`、`state = Open`;随后的 `TIn` 写在段 1;步骤 2 成功,段 2 `first_seq` = 段 1 `End` 的 seq + 1
- 实现:`crates/hft-journal/tests/roll.rs::jnl_034_next_not_ready`

### JNL-035 预创建段不可用时返回 Io、丢弃它并继续用当前段

- 规则:[写者](README.md#写者)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`;向预创建通道塞入一个段头被改过的段
- 实现:`crates/hft-journal/tests/roll.rs::jnl_035_bad_prepared_segment`

| 行 | 预创建段的问题 | 期望 |
|---|---|---|
| JNL-035.1 | `segment_no = 3`(应为 2) | `Err(RollErr::Io)`;段 1 仍 `Open`、末条不是 `End`;通道已空 |
| JNL-035.2 | `journal_name = "raw-gw-okx-swap"` | 同上 |
| JNL-035.3 | `state = Open` | 同上 |
| JNL-035.4 | `magic = *b"HFTJRNL2"` | 同上 |
| JNL-035.5 | 第 1 行之后再塞入一个段头合法的段 2 | `roll` 成功 |

### JNL-036 seal_and_close 写 End(other = 0,Manual)并封存

- 规则:[写者](README.md#写者)、[恢复与校验](README.md#恢复与校验)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`、追加到 seq 7
- 步骤:
  1. `seal_and_close()`
  2. 在同一目录 `open`
- 期望:段 1 末条 `SegmentRoll{End, segment_no: 1, other_segment_no: 0, seq_mark: 7, reason: Manual}`(seq 8)、`EVENT_END`、`committed_len` 含它、`state = Sealed`;步骤 2 的 `RecoveryReport{last_seq: 8, repaired_segments: 0, bytes_discarded: 0, gap_possible: false, boot_reason: Startup}`
- 实现:`crates/hft-journal/tests/roll.rs::jnl_036_seal_and_close`

### JNL-037 引导块的配置条数不设 16 的上限

- 规则:[写者](README.md#写者)、[journal-format SegmentHeader](../../docs/subsystems/journal-format.md#segmentheader)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;`RollInfo.configs` 含 40 个 `BlobRef`
- 步骤:
  1. `roll(Hourly, info)`、`finish`
- 期望:段 2 `n_config = 40`,`config_digest = SHA-256(40 个 BlobRef 按序拼接)`
- 实现:`crates/hft-journal/tests/roll.rs::jnl_037_many_configs`

### JNL-038 关键帧快照原样写入,本 crate 不计算也不改写 state_hash

- 规则:[写者](README.md#写者)、[journal-format 快照记录结构](../../docs/subsystems/journal-format.md#快照记录结构)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`;`StateSnapshot` 逻辑体 = `SnapshotHdr{purpose, state_hash = 行内值, n_sections: 2, …}` + 两节共 200 000 B
- 实现:`crates/hft-journal/tests/writer.rs::jnl_038_snapshot_passthrough`

| 行 | `purpose`、写入的 `state_hash` | 期望(读者交付的逻辑体) |
|---|---|---|
| JNL-038.1 | Keyframe、16 个 0 | 与写入逐字节相同;偏移 16..32 为 0 |
| JNL-038.2 | Boot、16 个 0x3C | 与写入逐字节相同(偏移 16..32 仍为 0x3C) |
| JNL-038.3 | Keyframe、16 个 0x01(违反写者约定的输入) | 与写入逐字节相同(本 crate 不校验内容;约定由 hft-sequencer 的用例覆盖) |

### JNL-039 热目录不在 RAM 文件系统上时拒绝

- 规则:[实现说明](README.md#实现说明)、[依赖](README.md#依赖)、[journal-format 目录与命名](../../docs/subsystems/journal-format.md#目录与命名)
- 级别:table
- 里程碑:M1
- 前置:Linux;C0 的 `hot_dir` 按行设定
- 实现:`crates/hft-journal/tests/prepare.rs::jnl_039_disk_backed_rejected`

| 行 | `hot_dir` | 期望 |
|---|---|---|
| JNL-039.1 | `/tmp/<tmp>`(ext4) | `JournalWriter::open` → `Err(DiskBacked)`;`SegmentPreparer::poll` → `Failed`,目录中没有新文件 |
| JNL-039.2 | `/dev/shm/<tmp>`(tmpfs) | `Ok` |
| JNL-039.3 | 不存在的路径 | `Err(Io)` |

### JNL-040 预创建的段:段头 [P] 字段、长度与锁定

- 规则:[实现说明](README.md#实现说明)、[journal-format SegmentHeader](../../docs/subsystems/journal-format.md#segmentheader)、[小段 + 4K 页模式(tmpfs 热段)](README.md#小段--4k-页模式tmpfs-热段)
- 级别:unit
- 里程碑:M1
- 前置:Linux tmpfs;`H(C0)`、`open0`(段 1 活动);`RLIMIT_MEMLOCK` 足够
- 步骤:
  1. `SegmentPreparer::poll()`
  2. 读 `<hot_dir>/prod/seq/0000000002.jrn` 的大小与段头;读 `/proc/self/status` 的 `VmLck`
- 期望:
  - 返回 `Prepared`;文件大小 1 048 576
  - 段头:`magic = HFTJRNL1`、`format_version = 1`、`journal_kind = 2`、`venue = 0`、`header_len = 4 096`、`journal_name = "seq"`、`instance = "prod"`、`segment_no = 2`、`segment_bytes = 1 048 576`、`writer_epoch = 7`、`prev_header_sha256 = SHA-256(段 1 [0, 3 072))`、`host_profile_sha256 = [0xCD; 32]`、`build_git = [0xAB; 20]`、`n_schema` = 写者已知 kind 数且 `schema[]` 与 KindInfo 一致、`state = Preparing`;[4 096, 1 048 576) 全零
  - `VmLck` 增加 ≥ 1 048 576 B
- 实现:`crates/hft-journal/tests/prepare.rs::jnl_040_prepared_segment_header`

### JNL-041 首段的 prev_header_sha256 全零,段号从 1 起

- 规则:[journal-format SegmentHeader](../../docs/subsystems/journal-format.md#segmentheader)、[journal-format 目录与命名](../../docs/subsystems/journal-format.md#目录与命名)
- 级别:unit
- 里程碑:M1
- 前置:空目录;`H(C0)`
- 步骤:
  1. `open0`
- 期望:段 1 `segment_no = 1`、`prev_header_sha256 = [0; 32]`、`first_seq = 1`、`prev_last_seq = 0`、`roll_reason = 3`(Startup)
- 实现:`crates/hft-journal/tests/prepare.rs::jnl_041_first_segment`

### JNL-042 每个 journal 只保持一个准备好的下一段

- 规则:[实现说明](README.md#实现说明)、[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:table
- 里程碑:M1
- 前置:Linux tmpfs;`H(C0)`、`open0`
- 实现:`crates/hft-journal/tests/prepare.rs::jnl_042_one_prepared_at_a_time`

| 行 | 动作 | 期望 |
|---|---|---|
| JNL-042.1 | `poll()` | `Prepared`;热目录 2 个段文件 |
| JNL-042.2 | 再 `poll()` | `Ready`;仍 2 个文件 |
| JNL-042.3 | `roll` 消耗段 2 后 `poll()` | `Prepared`;出现 `0000000003.jrn` |
| JNL-042.4 | 段 3 的文件名已被一个外来文件占用(`O_EXCL` 失败)时 `poll()` | `Failed`;外来文件内容不变;没有新文件 |
| JNL-042.5 | 删除外来文件后 `poll()` | `Prepared` |

### JNL-043 hugetlbfs 2 MiB 页上的预创建、写入与复制

- 规则:[实现说明](README.md#实现说明)、[依赖](README.md#依赖)、[段大小与热段内存](README.md#段大小与热段内存)
- 级别:proc
- 里程碑:M1
- 前置:CI 作业 `sudo sysctl vm.nr_hugepages=16`,`sudo mount -t hugetlbfs -o pagesize=2M,uid=$(id -u) none /mnt/huge2m`;`page = Huge2M`、`segment_bytes = 4 MiB`;写者在子进程中运行
- 步骤:
  1. 子进程 `open0`,写 1 000 个事件(`TIn` + `TOut`),`publish`;父进程 `Persister::poll` 直到同步完成
  2. 子进程读 `getrusage` 的 `ru_minflt`,再写 1 000 个事件
- 期望:`fstatfs` 为 `HUGETLBFS_MAGIC`;冷副本 [0, committed_len) 除 3 072..3 200 外与热段相同;步骤 2 前后 `ru_minflt` 之差为 0
- 实现:`crates/hft-journal/tests/proc/huge2m.rs::jnl_043_hugetlbfs_2m`

### JNL-044 Persister 每次最多复制 chunk_bytes

- 规则:[实现说明](README.md#实现说明)、[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)、[journal-format 冷副本与热段](../../docs/subsystems/journal-format.md#冷副本与热段)
- 级别:unit
- 里程碑:M1
- 前置:Linux(热段 tmpfs、冷目录 ext4);`H(C1)`,C1 = C0 但 `segment_bytes = 16 MiB`;`PersistConfig{chunk_bytes: 4 MiB, sync_bytes: 1 MiB, sync_interval_ms: 20}`;`committed_len = 10 MiB`(已 publish)
- 步骤:
  1. `poll(mono, utc)` 三次(`mono` 每次 +1 ms)
- 期望:三次后热段 `persisted_len` 依次为 4 MiB、8 MiB、10 MiB;冷副本在第一次 `poll` 时创建,文件大小 16 MiB(`fallocate`),名字含 `first_seq` 与日期目录;冷副本 [0, 10 MiB) 除 3 072..3 200 外与热段相同
- 实现:`crates/hft-journal/tests/persist.rs::jnl_044_chunked_copy`

### JNL-045 按字节阈值与时间阈值各自触发 fdatasync

- 规则:[实现说明](README.md#实现说明)、[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:table
- 里程碑:M1
- 前置:Linux;`H(C1)`(C1 见 JNL-044;1 MiB 的 C0 装不下第 2 行的未同步量);`PersistConfig{4 MiB, 1 MiB, 20}`(第 3、4 行 `chunk_bytes = 8`,使一次 `poll` 恰好复制 8 B);上次同步在 `mono = t₀`;`fdatasync` 次数由操作日志计数;每行先追加使"未同步字节"为行内值、publish,再 `poll(mono)`
- 实现:`crates/hft-journal/tests/persist.rs::jnl_045_sync_thresholds`

| 行 | 未同步字节 | `mono` | 期望 |
|---|---|---|---|
| JNL-045.1 | 1 048 568 | `t₀ + 19 999 999` | 0 次 `fdatasync`;`synced_len` 不变 |
| JNL-045.2 | 1 048 576 | `t₀ + 1` | 1 次;热段 `synced_len` = `persisted_len`;冷副本偏移 3 144 处的 `synced_len` 同值 |
| JNL-045.3 | 8 | `t₀ + 20 000 000` | 1 次 |
| JNL-045.4 | 8 | `t₀ + 19 999 999` | 0 次 |

### JNL-046 段封存后冷副本截断并写 .seal

- 规则:[实现说明](README.md#实现说明)、[journal-format 冷副本与热段](../../docs/subsystems/journal-format.md#冷副本与热段)
- 级别:unit
- 里程碑:M1
- 前置:Linux;`H(C0)`、`open0`、写到 seq 50、`roll`(段 1 `Sealed`,End 的 seq 51、`reason = Hourly`);`utc_ns = S`
- 步骤:
  1. `Persister::poll(mono, S)` 直到段 1 `synced_len == committed_len`
  2. 读冷副本与 `.seal`;独立计算 `sha256(冷副本[0, len) 且 3 072..3 200 置零)`
- 期望:冷副本大小 = 段 1 `committed_len`;`.seal` 为 128 B:`magic = HFTSEAL1`、`segment_no = 1`、`first_seq = 1`、`last_seq = 51`、`len = committed_len`、`sealed_utc_ns = S`、`sha256` 等于独立计算值、`reason = 1`、`flags = 0`、`crc32c = crc32c(前 124 B)`
- 实现:`crates/hft-journal/tests/persist.rs::jnl_046_seal`

### JNL-047 未全部同步的已封存段不写 .seal

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:Linux;同 JNL-046,但 `chunk_bytes = 4 096` 使一次 `poll` 复制不完
- 步骤:
  1. `poll` 一次
- 期望:段 1 `synced_len < committed_len`;没有 `.seal`;冷副本大小仍为 1 048 576
- 实现:`crates/hft-journal/tests/persist.rs::jnl_047_no_seal_before_synced`

### JNL-048 冷盘写失败时写者不受影响,滞后上升,恢复空间后追上

- 规则:[实现说明](README.md#实现说明)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:proc
- 里程碑:M1
- 前置:CI 作业 `sudo mount -t tmpfs -o size=2M none /mnt/cold-small` 作 `cold_root`;`H(C1)`(16 MiB 段);写者在子进程中持续以 1 000 事件/s 写 10 s
- 步骤:
  1. 父进程每 20 ms `Persister::poll`,记录 `lag()` 的字节数
  2. 第 10 s 时卸载并换成足够大的 tmpfs(同一路径),继续 `poll` 5 s
- 期望:步骤 1 中子进程从不阻塞或报错(每次 `append` 返回 `Ok`,`committed_len` 单调增);`poll` 在 `ENOSPC` 后返回带错误计数的进度,`lag()` 字节单调不减;段 1 没有 `.seal`,不被退役;步骤 2 结束时 `lag()` 字节 = 0
- 实现:`crates/hft-journal/tests/proc/persist_fail.rs::jnl_048_cold_write_failure`

### JNL-049 lag 报告每个 journal 的未同步字节

- 规则:[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:unit
- 里程碑:M1
- 前置:Linux;`Persister` 管理 C0 与 R0 两个 journal;C0 已提交 100 000 B、已同步 60 000 B;R0 全部同步
- 步骤:
  1. `lag()`
- 期望:恰两项:`seq` 的字节 = 40 000(`committed_len − synced_len`),`raw-feed-binance-usdm` 的字节 = 0
- 实现:`crates/hft-journal/tests/persist.rs::jnl_049_lag`

### JNL-050 persist 线程每轮先预创建、再复制同步、最后退役

- 规则:[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:unit
- 里程碑:M1
- 前置:内存后端的操作日志(prepare、copy、sync、retire 各记一条);`H(C0)`:没有已准备的下一段、有未复制的数据、有满足退役条件的旧段
- 步骤:
  1. 按 hft-core 的 persist 循环执行一轮(`SegmentPreparer::poll` → `Persister::poll` → `HotRetirer::poll`)
- 期望:日志顺序为 prepare → copy → sync → retire;同一轮内 prepare 完成之前没有 copy
- 实现:`crates/hft-journal/tests/persist.rs::jnl_050_round_order`

### JNL-051 热段退役的条件

- 规则:[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)、[journal-format 冷副本与热段](../../docs/subsystems/journal-format.md#冷副本与热段)
- 级别:table
- 里程碑:M1
- 前置:Linux;`H(C0)`;当前活动段号 N = 5;段 1–4 状态按行设定;`HotRetirer::poll(utc)` 一次后读各段 `retired`
- 实现:`crates/hft-journal/tests/retire.rs::jnl_051_retire_conditions`

| 行 | `hot_keep` | 段 3 状态 | 期望 |
|---|---|---|---|
| JNL-051.1 | 1 | 已封存、`synced_len == committed_len`、`.seal` 已写 | 段 3 `retired = 1`;段 4 `retired = 0`(4 + 1 不小于 5) |
| JNL-051.2 | 1 | 已封存、`synced_len < committed_len` | 段 3 `retired = 0` |
| JNL-051.3 | 1 | 已封存、已同步、无 `.seal` | 段 3 `retired = 0` |
| JNL-051.4 | 0 | 同第 1 行;段 4 同样满足 | 段 3、段 4 都 `retired = 1` |
| JNL-051.5 | 2 | 同第 1 行 | 段 3 `retired = 0`(3 + 2 不小于 5);段 2 若满足则 `retired = 1` |

### JNL-052 宽限 2 s 后才 munmap 与 unlink

- 规则:[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:table
- 里程碑:M1
- 前置:Linux;JNL-051 第 1 行在 `utc = T` 置 `retired`
- 实现:`crates/hft-journal/tests/retire.rs::jnl_052_retire_grace`

| 行 | 下一次 `poll` 的 `utc` | 期望 |
|---|---|---|
| JNL-052.1 | `T + 1 999 999 999` | 返回 0;段 3 文件仍在 |
| JNL-052.2 | `T + 2 000 000 000` | 返回 1;段 3 文件已删除;本进程 `/proc/self/maps` 中没有它 |

### JNL-053 写者线程从不 munmap 或 unlink

- 规则:[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:unit
- 里程碑:M1
- 前置:内存后端记录 `unmap`/`unlink` 调用及调用线程;`H(C0)`
- 步骤:
  1. 在写者线程上 `open0` 后滚动 20 次
- 期望:写者线程上 `unmap` 与 `unlink` 调用次数都为 0;回收环里依次是段 1–20 的 `RetiredSegment`
- 实现:`crates/hft-journal/tests/retire.rs::jnl_053_writer_never_unmaps`

### JNL-054 audit 统计已删除仍被映射的大页

- 规则:[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:proc
- 里程碑:M1
- 前置:CI hugetlbfs 2 MiB 挂载(同 JNL-043);子进程只读映射段 2(4 MiB)后保持映射;父进程退役并删除段 2
- 步骤:
  1. 父进程 `HotRetirer::audit()`;子进程退出后再 `audit()`
- 期望:第一次报告 2 个 2 MiB 页(4 MiB);第二次报告 0
- 实现:`crates/hft-journal/tests/proc/audit.rs::jnl_054_audit_unlinked_mapped`

### JNL-055 恢复删除 Preparing 段

- 规则:[恢复与校验](README.md#恢复与校验)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`、写到 seq 20 并 publish;预创建段 2(`Preparing`);模拟崩溃(写者 `mem::forget`)
- 步骤:
  1. `open`
- 期望:段 2 被删除;`RecoveryReport.boot_reason = Recovery`;首次 `roll` 打开的新段号为 2(重新准备)
- 实现:`crates/hft-journal/tests/recover.rs::jnl_055_preparing_deleted`

### JNL-056 引导块没写完的段整段删除,上一段保持完好,新段同号重开

- 规则:[恢复与校验](README.md#恢复与校验)、[实现说明](README.md#实现说明)、[journal-format 段滚动与引导块](../../docs/subsystems/journal-format.md#段滚动与引导块)
- 级别:unit
- 里程碑:M1
- 前置:Linux;`H(C0)`、`open0`、写到 seq 30、`roll` 到段 2;引导块写了 `Begin` 与 `StateSnapshot` 的 2 段(无 `BootDone`),`Persister` 已把段 2 的这些字节复制到冷副本;模拟崩溃
- 步骤:
  1. `open`;首次 `roll(boot_reason)`
- 期望:段 2 的热段、冷副本与同步元数据都被删除;段 1 字节不变(已封存,末条 `End`);新段号仍为 2,`first_seq` = 段 1 `End` 的 seq + 1,`roll_reason = 4`(Recovery);`repaired_segments ≥ 1`
- 实现:`crates/hft-journal/tests/recover.rs::jnl_056_incomplete_boot_removed`

### JNL-057 尾部扫描后截断到扫描区内最后一个 EVENT_END 之后

- 规则:[实现说明](README.md#实现说明)、[journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;事件 E1 已 publish(`committed_len = c₁`);其后写 E2 = [`TIn`, `TOut`] 并 `end_event`(不 publish),再写 E3 的 `TIn` 与 `TOut`(无 `end_event`);模拟崩溃
- 步骤:
  1. `open`
- 期望:段 1 截断到 `c₁ + 184`(E2 保留),`End{Recovery}` 从该处开始;E3 的 184 B 清零;`RecoveryReport.bytes_discarded = 184`、`last_seq` 等于 E2 的 `TOut` 的 seq + 1(随后追加的 `End{Recovery}`)
- 实现:`crates/hft-journal/tests/recover.rs::jnl_057_truncate_half_event`

### JNL-058 扫描在第一条不合法记录处停止

- 规则:[实现说明](README.md#实现说明)、[journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标)、[journal-format 记录对齐与 CRC](../../docs/subsystems/journal-format.md#记录对齐与-crc)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`;E1 已 publish(`committed_len = c₁`);其后 E2(`TIn` + `TOut`,`end_event`,未 publish)与 E3(`TIn` + `TCrc`,`end_event`,未 publish);按行篡改 E3 的 `TIn` 记录头或 `TCrc`;模拟崩溃后 `open`
- 实现:`crates/hft-journal/tests/recover.rs::jnl_058_scan_stops_at_invalid`

| 行 | 篡改 | 期望 `committed_len` |
|---|---|---|
| JNL-058.1 | 无 | E3 之后 |
| JNL-058.2 | E3 `TIn.len = 0` | E2 之后 |
| JNL-058.3 | E3 `TIn.len = 100`(不是 8 的倍数) | E2 之后 |
| JNL-058.4 | E3 `TIn.len = 65 544` | E2 之后 |
| JNL-058.5 | E3 `TIn.len = 16` | E2 之后 |
| JNL-058.6 | E3 `TIn.seq` 加 1(不连续) | E2 之后 |
| JNL-058.7 | E3 `TIn.kind = 0x7FEE`(未注册) | E2 之后 |
| JNL-058.8 | E3 `TIn.kind = RawRx`(不允许出现在 seq journal) | E2 之后 |
| JNL-058.9 | E3 `TCrc` 的 CRC 翻转一位 | E2 之后 |

### JNL-059 截断点不低于 committed_len

- 规则:[实现说明](README.md#实现说明)、[journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;事件 E1 = [`TIn`, `TOut`, M(事件内 `TMeta`)]、`end_event`、`publish`(`committed_len` 在 M 之后,M 不带 `EVENT_END`);其后写 E2 的 `TIn`(无 `end_event`);模拟崩溃
- 步骤:
  1. `open`
- 期望:`committed_len` 等于崩溃前的值(M 保留,E2 的 `TIn` 清零);M 的记录头 flags 仍为 0
- 实现:`crates/hft-journal/tests/recover.rs::jnl_059_truncation_not_below_committed`

### JNL-060 未完成的多段组整体丢弃

- 规则:[journal-format 变长与多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;E1 已 publish;E2 = `TIn` + `TMulti`(3 段)但只写完前 2 段(第 2 段仍带 `PART`);模拟崩溃
- 步骤:
  1. `open`
- 期望:`committed_len` = E1 之后;E2 的 `TIn` 与两段全部清零;`bytes_discarded = 104 + 2 × 65 528`
- 实现:`crates/hft-journal/tests/recover.rs::jnl_060_incomplete_group_dropped`

### JNL-061 截断后清零到下一个全零页

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;E1 已 publish 于 `committed_len = c₁`;[c₁, c₁ + 6 000) 写入非零垃圾(不构成合法记录头,`len` 字段为 0xFFFF_FFFF);模拟崩溃
- 步骤:
  1. `open`
- 期望:[c₁, c₁ + 6 000) 全零;`committed_len = c₁`(之后追加的 `End{Recovery}` 从 c₁ 开始)
- 实现:`crates/hft-journal/tests/recover.rs::jnl_061_zero_to_clean_page`

### JNL-062 恢复先补复制未同步的段,再做任何新写入

- 规则:[实现说明](README.md#实现说明)、[恢复与校验](README.md#恢复与校验)
- 级别:unit
- 里程碑:M1
- 前置:Linux;`H(C0)`、`open0`、写 2 000 个事件并 publish;`Persister` 只复制并同步到一半;模拟崩溃
- 步骤:
  1. `open`(返回之前不调用 `Persister`)
- 期望:后端操作日志中,段 1 补复制并 `fdatasync` 到崩溃时的 `committed_len` 发生在追加 `End{Recovery}` 之前;`open` 返回时冷副本 [0, 崩溃时 committed_len) 除 3 072..3 200 外与热段相同;`RecoveryReport.bytes_copied` = 补复制的字节数
- 实现:`crates/hft-journal/tests/recover.rs::jnl_062_recopy_before_writes`

### JNL-063 崩溃后最后一个 Open 段补 End{Recovery} 并封存,新段以 Recovery 打开

- 规则:[实现说明](README.md#实现说明)、[恢复与校验](README.md#恢复与校验)、[写者](README.md#写者)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`、写到 seq 40(publish);模拟崩溃;预创建段 2
- 步骤:
  1. `(w, rep) = open(...)`;`w.roll(rep.boot_reason, info)`
- 期望:段 1 末条 `SegmentRoll{End, segment_no: 1, other_segment_no: 2, seq_mark: 40, reason: Recovery}`(seq 41)、`state = Sealed`;`rep.boot_reason = Recovery`、`rep.last_seq = 41`;段 2 `roll_reason = 4`、`first_seq = 42`、`flags = 0`
- 实现:`crates/hft-journal/tests/recover.rs::jnl_063_end_recovery`

### JNL-064 热目录为空且冷副本末段已封存:无缺口

- 规则:[实现说明](README.md#实现说明)、[恢复与校验](README.md#恢复与校验)
- 级别:unit
- 里程碑:M1
- 前置:Linux;`H(C0)`、`open0`、写到 seq 60、`seal_and_close`、persist 刷完(含 `.seal`);删除热目录中的全部段(模拟 `hft-shm-release`)
- 步骤:
  1. `(w, rep) = open(...)`;`w.roll(rep.boot_reason, info)`
- 期望:`rep.gap_possible = false`、`rep.boot_reason = Startup`、`rep.last_seq = 61`;新段 2 `flags = 0`(无 `RECOVERY_GAP`)、`first_seq = 62`、`prev_header_sha256` = 冷副本段 1 头 [0, 3 072) 的 SHA-256
- 实现:`crates/hft-journal/tests/recover.rs::jnl_064_cold_sealed_clean`

### JNL-065 热目录为空且冷副本末段未封存:截断并置 RECOVERY_GAP

- 规则:[实现说明](README.md#实现说明)、[恢复与校验](README.md#恢复与校验)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:unit
- 里程碑:M1
- 前置:Linux;`H(C0)`、`open0`、写 3 000 个事件;persist 同步到 `synced_len = s`(段内);冷副本 [s, s + 10 000) 有已复制未同步的字节,其中 s + 4 000 处开始的一页被置零(模拟掉电);删除热目录
- 步骤:
  1. `(w, rep) = open(...)`;`w.roll(rep.boot_reason, info)`
- 期望:冷副本段 1 截断到 s 之后最后一条完整且以 `EVENT_END` 结束的记录处,随后追加 `End{Recovery}` 并写 `.seal`;`rep.gap_possible = true`、`rep.boot_reason = Recovery`;新段 `flags = RECOVERY_GAP`(1)
- 实现:`crates/hft-journal/tests/recover.rs::jnl_065_cold_unsealed_gap`

### JNL-066 rebuild_from 指向最近的完整引导块或关键帧

- 规则:[恢复与校验](README.md#恢复与校验)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`;按行构造 journal 后模拟崩溃再 `open`
- 实现:`crates/hft-journal/tests/recover.rs::jnl_066_rebuild_from`

| 行 | journal 内容 | 期望 `rebuild_from` |
|---|---|---|
| JNL-066.1 | 段 1 引导块(含 `StateSnapshot{Boot}`)+ 100 个事件 | 段 1 的 `Begin` |
| JNL-066.2 | 同上,第 50 个事件之后有 `StateSnapshot{Keyframe}`(位置 P) | P |
| JNL-066.3 | 段 1 有关键帧 P,段 2 引导块完整 + 10 个事件 | 段 2 的 `Begin` |
| JNL-066.4 | 段 1 有关键帧 P;段 2 引导块不完整(恢复时删除) | P |

### JNL-067 段链断裂与冷热冲突拒绝打开

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:Linux;`H(C0)`;按行构造热目录与冷目录后 `open`
- 实现:`crates/hft-journal/tests/recover.rs::jnl_067_chain_and_conflict`

| 行 | 构造 | 期望 |
|---|---|---|
| JNL-067.1 | 热目录有段 3、段 5,缺段 4(段 4 也不在冷目录) | `Err(ChainBroken)` |
| JNL-067.2 | 段 5 的 `prev_header_sha256` 与段 4 头不符,无 `RECOVERY_GAP` | `Err(ChainBroken)` |
| JNL-067.3 | 段 5 的 `first_seq ≠` 段 4 `last_seq + 1`,无 `RECOVERY_GAP` | `Err(ChainBroken)` |
| JNL-067.4 | 同上但段 5 置 `RECOVERY_GAP` | `Ok` |
| JNL-067.5 | 段 4 的热段与冷副本在 [0, synced_len) 内某条记录体的一个字节不同 | `Err(ColdHotConflict)` |
| JNL-067.6 | 两者只在 3 072..3 200 不同 | `Ok` |
| JNL-067.7 | 段 4 热段 `synced_len = 4 240`,冷目录中没有段 4 的副本(冷副本根已搬空) | `Err(ColdHotConflict)`;热段文件字节不变 |
| JNL-067.8 | 段 4 热段 `synced_len = 4 240`,冷副本长 4 232 B | `Err(ColdHotConflict)` |
| JNL-067.9 | 段 4 热段 `synced_len = 0`,冷目录中没有段 4 的副本 | `Ok`(恢复第 3 步补复制) |

### JNL-068 空目录与磁盘上的热目录

- 规则:[写者](README.md#写者)、[恢复与校验](README.md#恢复与校验)
- 级别:table
- 里程碑:M1
- 前置:Linux;`H(C0)`
- 实现:`crates/hft-journal/tests/recover.rs::jnl_068_empty_and_disk`

| 行 | 情形 | 期望 |
|---|---|---|
| JNL-068.1 | 热目录与冷目录都为空 | `Ok`;`RecoveryReport{last_seq: 0, repaired_segments: 0, bytes_copied: 0, bytes_discarded: 0, gap_possible: false, boot_reason: Startup}` |
| JNL-068.2 | 热目录在 ext4 上 | `Err(DiskBacked)` |

### JNL-069 写者进程被杀后热段仍在、内容完整

- 规则:[依赖](README.md#依赖)、[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M1
- 前置:Linux tmpfs;子进程 `open0` 后写 4 000 个事件(每个 184 B,约 736 KB,在 C0 的 1 MiB 段的空闲阈值之内)并逐个 publish,然后 `SIGKILL` 自身
- 步骤:
  1. 父进程读热段文件
- 期望:文件存在,`state = Open`;[0, committed_len) 中的记录逐条合法、seq 从 1 连续到 `last_seq`、最后一条带 `EVENT_END`
- 实现:`crates/hft-journal/tests/proc/crash.rs::jnl_069_segment_survives_kill`

### JNL-070 事件写到一半被杀:恢复后该事件整体消失

- 规则:[实现说明](README.md#实现说明)、[journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标)
- 级别:proc
- 里程碑:M1
- 前置:Linux tmpfs;子进程写完事件 1–99(publish),第 100 个事件写了 `TIn` 与 1 条 `TOut` 后经管道通知父进程并停住
- 步骤:
  1. 父进程 `SIGKILL` 子进程;`open`;读者以 `EndAt::LastCompleteEvent` 从 `Oldest` 读
- 期望:事件 100 的两条记录不在 journal 中(字节为零);`bytes_discarded = 184`;读者交付的最后一个事件是事件 99,之后返回 `End`;`last_complete_event()` 等于事件 99 之后的位置
- 实现:`crates/hft-journal/tests/proc/crash.rs::jnl_070_kill_mid_event`

### JNL-071 引导块写到一半被杀:恢复删除该段

- 规则:[实现说明](README.md#实现说明)、[journal-format 段滚动与引导块](../../docs/subsystems/journal-format.md#段滚动与引导块)
- 级别:proc
- 里程碑:M1
- 前置:Linux;子进程在段 2 的引导块写 `StateSnapshot`(20 段)的第 7 段后停住;父进程的 `Persister` 已复制段 2 的已提交部分;一个读者在另一进程从 `Oldest` 尾随
- 步骤:
  1. `SIGKILL` 子进程;`open`;首次 `roll`
- 期望:段 2 的热段、冷副本与同步元数据被删除;段 1 字节不变;新段号 2;尾随读者从未交付段 2 的任何记录(它在 `BootDone` 提交前返回 `Pending`)
- 实现:`crates/hft-journal/tests/proc/crash.rs::jnl_071_kill_mid_boot`

### JNL-072 任意时刻被杀后,保留的恰是全部完整事件

- 规则:[实现说明](README.md#实现说明)、[journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标)、[journal-format 变长与多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)
- 级别:proc
- 里程碑:M1
- 前置:Linux tmpfs;C0 但 `segment_bytes = 2 MiB`;子进程按种子生成事件流(每事件 1–6 条记录,`TIn`/`TOut`/`TMeta`/`TMulti` 1–3 段,10% 事件后 `publish`,安全阀到期即滚动)。每个事件开始前按未发布写游标预留 `6 × 3 × 65536 B` 的整事件上界与 8192 B 滚动余量,不足则先发布、滚动;每完成一个 `end_event` 就把该事件的 (首 seq, 末 seq) 写进父进程的管道;父进程在种子决定的杀点 `SIGKILL`
- 步骤:
  1. 每个种子:运行到 `50 + (seed × 7919) % 2000` 个完整事件、杀死、确认退出信号恰为 `SIGKILL`、`open`、`roll`、从 `Oldest` 读完;未到杀点就 EOF 或普通退出均失败;每个种子的目录在该种子结束时释放(`HFT_TEST_KEEP_TMP=1` 除外)
- 期望:记被杀前最后一次 `publish` 覆盖的事件为 P、已 `end_event` 的事件为 R(P ⊆ R,按序),读到的事件序列是 R 的一个前缀且包含 P 的全部;没有半个事件、没有半条记录;seq 在每段内逐条 +1、跨段 `first_seq = prev_last_seq + 1`;多段组要么完整要么不在;恢复后新段 `Begin.seq_mark` = 上一段 `End` 的 seq
- 实现:`crates/hft-journal/tests/proc/crash.rs::jnl_072_random_kill`
- 备注:PR 运行 50 个种子;每夜 1 000 个(`## 验收`)

### JNL-073 冷副本掉电损坏:截断到最后一条合法记录并置 RECOVERY_GAP

- 规则:[实现说明](README.md#实现说明)、[journal-format 校验与缺段检测](../../docs/subsystems/journal-format.md#校验与缺段检测)
- 级别:prop
- 里程碑:M1
- 性质:对冷副本做一种损坏后删除热目录再 `open`:恢复后的冷副本是原内容在某条 `EVENT_END` 记录之后的前缀;新段置 `RECOVERY_GAP`;`verify` 把该位置报为 `recovery_gaps` 而不是 `issues`;损坏位置之前的记录全部保留
- 生成:先写 1–5 000 个随机事件(每事件 1–3 条 `TCrc`,使任何损坏都可检出)并完全同步但不封存;损坏 ∈ {在随机偏移截断文件;把从随机偏移起的 4 KiB 置零;把从随机偏移起的 4 KiB 填随机字节};随机偏移在段 1 引导块之后(≥ 4 240)、已写数据之内
- 参考:原事件序列的前缀判定(逐条比较 seq 与字节)
- 例数:PR 128;每夜 4 096
- 实现:`crates/hft-journal/tests/prop_powerloss.rs::jnl_073_cold_corruption`

### JNL-074 尾随读者只读到 committed_len,没有新数据时返回 Pending

- 规则:[读者](README.md#读者)、[journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标)、[journal-format 读者 API 概要](../../docs/subsystems/journal-format.md#读者-api-概要)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;读者 `ReaderConfig::new(…)` 改 `start = Tail`、`end = Follow`
- 步骤:
  1. 读者 `poll`
  2. 写者 `append(TIn)`(未 publish);读者 `poll`
  3. 写者 `end_event`、`publish`;读者 `poll` 两次
- 期望:步骤 1、2 返回 `Pending`;步骤 3 先交付 seq 3 的 `Record`,再返回 `Pending`;`position().segment_no == 1`、`position().seq == 3`
- 实现:`crates/hft-journal/tests/reader.rs::jnl_074_tail_bounded_by_committed`

### JNL-075 读到 Pending 后至少隔 tail_poll_us 才重读 committed_len

- 规则:[读者](README.md#读者)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`;内存后端统计读者对写者行的 Acquire 读次数;读者 `tail_poll_us = 50`;手动时钟
- 实现:`crates/hft-journal/tests/reader.rs::jnl_075_tail_poll_interval`

| 行 | 动作 | 期望(写者行读取次数的增量) |
|---|---|---|
| JNL-075.1 | 已知可读区间内有 100 条记录,连续 `poll` 100 次 | 0(只在区间开始时读过一次) |
| JNL-075.2 | 返回 `Pending` 后在 +49 999 ns 内 `poll` 1 000 次 | 0;都返回 `Pending` |
| JNL-075.3 | 在 +50 000 ns `poll` 一次 | 1 |

### JNL-076 读者的各种起点

- 规则:[读者](README.md#读者)、[journal-format 读者 API 概要](../../docs/subsystems/journal-format.md#读者-api-概要)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`;段 1(seq 1–100,含 seq 40 起的一个 3 段 `TMulti`,即 seq 40–42)、段 2(引导块 seq 101–104,普通事件 105–150,seq 120 为 `StateSnapshot{Keyframe}`)、段 3(引导块 151–154,事件到 seq 170);全部已 publish
- 实现:`crates/hft-journal/tests/reader.rs::jnl_076_start_positions`

| 行 | `start` | 期望(第一次交付) |
|---|---|---|
| JNL-076.1 | `Oldest` | seq 1(段 1 `Begin`) |
| JNL-076.2 | `Seq(77)` | seq 77 |
| JNL-076.3 | `Seq(41)`(多段组中间) | 以 `Event` 交付 seq 40 的组 |
| JNL-076.4 | `Seq(171)`(= last_seq + 1) | `Pending`,写者追加 seq 171 后交付它 |
| JNL-076.5 | `Seq(172)` | `open` → `Err(SeqUnavailable)` |
| JNL-076.6 | `SegmentBoot(2)` | seq 101(段 2 `Begin`),`in_boot_block() == true` |
| JNL-076.7 | `LatestBoot` | seq 151 |
| JNL-076.8 | `LatestKeyframe`(段 3 没有关键帧) | seq 151 |
| JNL-076.9 | 段 3 在 seq 160 写 `StateSnapshot{Keyframe}` 后 `LatestKeyframe` | seq 160 |
| JNL-076.10 | `Tail` | `Pending` |
| JNL-076.11 | 段 1 已从热与冷两处删除,`Seq(50)` | `open` → `Err(SeqUnavailable)` |
| JNL-076.12 | 空目录 | `open` → `Err(NotFound)` |

### JNL-077 引导块提交之前不交付该段,in_boot_block 标出引导块

- 规则:[读者](README.md#读者)、[journal-format 段滚动与引导块](../../docs/subsystems/journal-format.md#段滚动与引导块)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`、写到 seq 10;读者从 `Oldest` 尾随并读完段 1
- 步骤:
  1. 写者 `roll`,写引导块 `Begin`、`TMeta`、`StateSnapshot{Boot}`(2 段),不 finish;读者 `poll`
  2. 写者 `finish`、`publish`;读者逐条 `poll`,每次记录 `in_boot_block()`
- 期望:步骤 1 返回 `Pending`(段 1 的 End 已交付后);步骤 2 依次交付 `Begin`、`TMeta`、`StateSnapshot`(`Event`)、`BootDone`,四次 `in_boot_block()` 都为 true;再写一个普通事件,交付后 `in_boot_block() == false`
- 实现:`crates/hft-journal/tests/reader.rs::jnl_077_boot_block_visibility`

### JNL-078 多段记录重组为逻辑事件

- 规则:[读者](README.md#读者)、[journal-format 变长与多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`;读者 `reassembly_cap` 按行
- 实现:`crates/hft-journal/tests/reader.rs::jnl_078_reassembly`

| 行 | 输入 | `reassembly_cap` | 期望 |
|---|---|---|---|
| JNL-078.1 | `TMulti` 逻辑体 130 977 B(3 段,seq 3–5) | 256 MiB | 一次 `Event{seq: 3, parts: 3, kind: 0x7F05, body: 130 977 B 与写入相同}`,`flags` 含 `EVENT_END`;`position().seq == 5` |
| JNL-078.2 | `TMulti` 1 000 B(1 段) | 256 MiB | `Event{parts: 1}` |
| JNL-078.3 | `TVar` | 256 MiB | `Record` |
| JNL-078.4 | 第 1 行的输入 | 0 | 三次 `Record`(各带 `PartHdr`) |
| JNL-078.5 | 第 1 行的输入 | 130 976 | `Err(ReassemblyCap)`(读到第 0 段即返回,未分配 130 977 B) |
| JNL-078.6 | 第 1 行的输入 | 130 977 | `Event` |

### JNL-079 损坏的多段组报 Corrupt

- 规则:[读者](README.md#读者)、[journal-format 变长与多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`;写入 3 段组后就地篡改,读者从 `Oldest` 读
- 实现:`crates/hft-journal/tests/reader.rs::jnl_079_corrupt_groups`

| 行 | 篡改 | 期望 |
|---|---|---|
| JNL-079.1 | 第 2 段的 `chunk_len` 减 8(Σ chunk_len ≠ total_len) | `Err(Corrupt)` |
| JNL-079.2 | 交换第 0、1 段的 `part` 字段 | `Err(Corrupt)` |
| JNL-079.3 | 用写者把一条 `TIn` 插在第 0、1 段之间(测试钩子绕过写者检查) | `Err(Corrupt)` |
| JNL-079.4 | 第 1 段的 `parts = 4` | `Err(Corrupt)` |

### JNL-080 跨段读取:按 other_segment_no 打开下一段,热目录优先

- 规则:[实现说明](README.md#实现说明)、[读者](README.md#读者)
- 级别:table
- 里程碑:M1
- 前置:Linux;`H(C0)`;三段 journal(段 1、2 已封存并复制,段 3 活动);读者从 `Oldest` 读完
- 实现:`crates/hft-journal/tests/reader.rs::jnl_080_cross_segments`

| 行 | 情形 | 期望 |
|---|---|---|
| JNL-080.1 | 三段都在热目录 | 交付 seq 1 到 last_seq,逐条 +1,无重复、无缺失;段 1、2 从热段读 |
| JNL-080.2 | 段 1 的热段已退役删除 | 同样完整;段 1 从冷副本读 |
| JNL-080.3 | 读者读到段 2 中途时段 2 被置 `retired = 1` | 在同一偏移切到冷副本,交付序列与第 1 行相同 |

### JNL-081 缺段、恢复缺口与链断裂的报告与处置

- 规则:[读者](README.md#读者)、[实现说明](README.md#实现说明)、[journal-format 读者 API 概要](../../docs/subsystems/journal-format.md#读者-api-概要)
- 级别:table
- 里程碑:M1
- 前置:Linux;`H(C0)`;段 1–4,按行改造;读者从 `Oldest` 读,`on_gap` 按行
- 实现:`crates/hft-journal/tests/reader.rs::jnl_081_gaps`

| 行 | 改造 | `on_gap` | 期望 |
|---|---|---|---|
| JNL-081.1 | 读者打开之后,段 2 的热段与冷副本都删除 | Error | 读完段 1 后 `Gap{reason: Deleted, resume_segment_no: 3}`;之后每次 `poll` 返回同一个 `Gap` |
| JNL-081.2 | 同上 | SkipToNextBoot | 一次 `Gap`,下一次交付段 3 的 `Begin` |
| JNL-081.3 | 段 3 置 `RECOVERY_GAP`,`first_seq` = 段 2 `last_seq + 5`(写者测试钩子在滚动时跳过 4 个 seq) | Error | 段 2 读完后 `Gap{reason: RecoveryGap, expected_seq: 段 2 last_seq + 1}`,停住 |
| JNL-081.4 | 同上 | SkipToNextBoot | 一次 `Gap{RecoveryGap}`,随后交付段 3 的 `Begin` |
| JNL-081.5 | 段 3 `first_seq` 不连续且未置 `RECOVERY_GAP` | SkipToNextBoot | `Gap{reason: ChainBroken}` |
| JNL-081.6 | 段 2 的 `End.other_segment_no = 3` 但段 3 文件(热与冷)在读者打开之前已不存在、段 4 存在 | SkipToNextBoot | `Gap{reason: MissingSegment, resume_segment_no: 4}`,随后交付段 4 的 `Begin` |

### JNL-082 kind、CRC 与段头的读取错误

- 规则:[读者](README.md#读者)、[journal-format 记录对齐与 CRC](../../docs/subsystems/journal-format.md#记录对齐与-crc)、[journal-format 格式演进](../../docs/subsystems/journal-format.md#格式演进)
- 级别:table
- 里程碑:M1
- 前置:`H(C0)`、`open0`;就地篡改一条已提交的记录或段头;读者从 `Oldest` 读;"生产构建"指不启用 hft-event dev 特性编译的同一测试(不认 0x7Fxx)
- 实现:`crates/hft-journal/tests/reader.rs::jnl_082_read_errors`

| 行 | 篡改 | 期望 |
|---|---|---|
| JNL-082.1 | 一条记录的 `kind = 0x05FF`(本二进制未知) | 以 `Record` 交付,其后照常 |
| JNL-082.2 | 一条记录的 `kind = RawRx`(不允许出现在 seq) | `Err(Corrupt)` |
| JNL-082.3 | `TCrc` 体翻转一位 | `Err(BadCrc)` |
| JNL-082.4 | 生产构建读到 `TIn`(0x7F01) | `Err(DevKind)` |
| JNL-082.5 | 段头 `format_version = 2` | `Err(Corrupt)` |
| JNL-082.6 | 段文件截短到 4 000 B(映射不足 4 096) | `Err(Corrupt)` |
| JNL-082.7 | 一条记录的 `seq` 加 1 | `Err(Corrupt)` |

### JNL-083 LastCompleteEvent 只交付到恢复截断点

- 规则:[读者](README.md#读者)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`H(C0)`、`open0`;E1、E2 已 publish;E3 已 `end_event` 未 publish;E4 只写了 `TIn`;不经恢复,直接以 `EndAt::LastCompleteEvent` 打开读者(`Oldest`)
- 步骤:
  1. 读到底
- 期望:交付 E1、E2、E3 的全部记录后返回 `End`;不交付 E4;`last_complete_event()` = E3 之后的位置
- 实现:`crates/hft-journal/tests/reader.rs::jnl_083_last_complete_event`

### JNL-084 段文件被删除后读者在 1 s 内解除映射

- 规则:[读者](README.md#读者)
- 级别:table
- 里程碑:M1
- 前置:Linux;冷进程式读者(非同进程槽位)映射段 2 的热段;手动时钟驱动读者的 `fstat` 节拍;写者与读者在同一测试进程中,`/proc/self/maps` 的计数以读者打开前该路径的条目数为基数(写者、预创建段与退役队列的映射)
- 实现:`crates/hft-journal/tests/reader.rs::jnl_084_unmap_deleted_segments`

| 行 | 事件 | 期望 |
|---|---|---|
| JNL-084.1 | 段 2 置 `retired = 1`,读者 `poll` 一次 | 该次 `poll` 内 `munmap` 并关闭 fd:`/proc/self/maps` 中段 2 路径的条目数回到基数;之后从冷副本同偏移继续 |
| JNL-084.2 | 段 2 文件被 `unlink`(未置 `retired`),读者上次 `fstat` 之后 +1 000 ms 时 `poll` | 已解除映射:`/proc/self/maps` 中段 2(`(deleted)`)的条目数回到基数 |
| JNL-084.3 | 读者正在读的冷副本被 `cold_retire` 删除,+1 000 ms `poll` | 解除冷副本映射,返回 `Gap{Deleted}` |

### JNL-085 读者同时最多映射 2 个热段

- 规则:[读者](README.md#读者)
- 级别:unit
- 里程碑:M1
- 前置:Linux;`hot_keep = 4`;6 个热段;读者从 `Oldest` 读到段 6 末尾
- 步骤:
  1. 每次 `poll` 后统计 `/proc/self/maps` 中属于该 journal 热目录的映射数,减去读者打开前的基数(同进程写者的活动段、预创建段与退役队列的映射,读的过程中不变)
- 期望:最大值 ≤ 2
- 实现:`crates/hft-journal/tests/reader.rs::jnl_085_two_hot_maps`

### JNL-086 同进程尾随读者用固定槽位,滚动不触发 mmap/munmap

- 规则:[读者](README.md#读者)
- 级别:unit
- 里程碑:M1
- 前置:内存后端计数 `map`/`unmap`(只计读者 `poll` 期间);C0 `hot_keep = 1`;同进程读者以固定槽位方式打开(`ReaderConfig.same_process = true`)
- 步骤:
  1. 写者滚动 10 次,读者全程尾随并读完
- 期望:读者打开之后 `map` 与 `unmap` 的调用次数都为 0;交付序列完整
- 实现:`crates/hft-journal/tests/reader.rs::jnl_086_fixed_slots`

### JNL-087 读者只读映射,不写任何共享行

- 规则:[读者](README.md#读者)、[cargo feature](README.md#cargo-feature)
- 级别:proc
- 里程碑:M1
- 前置:Linux tmpfs;父进程写者 `open0` 并持续写;子进程 A 以读者打开后尝试向映射首页写 1 字节;子进程 B 只读 10 s
- 步骤:
  1. 等 A 退出;B 运行期间每 100 ms 采样段头 3 072..3 200
- 期望:A 被 `SIGSEGV` 终止;B 运行期间写者行与 persist 行的取值序列与没有 B 时的同一工作负载相同(只有写者与 persist 改动它们)
- 实现:`crates/hft-journal/tests/proc/reader.rs::jnl_087_reader_read_only`

### JNL-088 慢读者不影响写者

- 规则:[读者](README.md#读者)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:proc
- 里程碑:M1
- 前置:Linux tmpfs;C0 `segment_bytes = 4 MiB`、`hot_keep = 1`;子进程读者从 `Oldest` 尾随
- 步骤:
  1. 对读者 `SIGSTOP`;写者写 200 000 个事件(期间多次滚动与退役,persist 照常)
  2. `SIGCONT` 读者,读到末尾
- 期望:步骤 1 中写者每次 `append` 都返回 `Ok`,且没有一次调用阻塞:写者线程在全部 `append` 调用内的自愿上下文切换数与主缺页数(`getrusage(RUSAGE_THREAD)` 的 `ru_nvcsw`、`ru_majflt`)都为 0;读者恢复后交付的序列完整(早期段从冷副本读)
- 备注:只记录不判定每对 `append` 的最长墙钟时间与线程 CPU 时间、被抢占次数、次缺页数。tmpfs 热段与共享 CI runner 上不判时间门槛([延迟与资源契约](README.md#延迟与资源契约)表头、[testing §8](../../docs/testing.md#8-上线门槛));墙钟时间含其他可运行线程的抢占与 vCPU 被宿主机挪用,不反映实现。`append` 时延由 JNL-108、有读者时的 `publish` 由 JNL-110 在参考机上判定
- 实现:`crates/hft-journal/tests/proc/reader.rs::jnl_088_slow_reader`

### JNL-089 verify 对完好的 journal 不报问题

- 规则:[恢复与校验](README.md#恢复与校验)、[journal-format 校验与缺段检测](../../docs/subsystems/journal-format.md#校验与缺段检测)
- 级别:unit
- 里程碑:M1
- 前置:Linux;`H(C0)`;3 个段(段 1、2 已封存并有 `.seal`,段 3 活动)
- 步骤:
  1. `verify(cold_root/prod/seq, &VerifyOpts::default())`
- 期望:`issues`、`missing`、`recovery_gaps` 都为空;`unsealed == [3]`
- 实现:`crates/hft-journal/tests/verify.rs::jnl_089_verify_clean`

### JNL-090 verify 逐项检出损坏

- 规则:[恢复与校验](README.md#恢复与校验)、[journal-format 校验与缺段检测](../../docs/subsystems/journal-format.md#校验与缺段检测)
- 级别:table
- 里程碑:M1
- 前置:同 JNL-089(段 2 的引导块含 `StateSnapshot{Boot, n_books = 2}` 与 2 条 `L2Snapshot{Keyframe}`,段 1 含 `TCrc`),每行只做一处篡改(改段头的第 5–7 行重算 `header_crc32c`);期望写 `issues` 中必须出现的项(`{segment_no, item}`,偏移按篡改处;篡改已封存段时 item 5 可同时出现)或汇总字段
- 实现:`crates/hft-journal/tests/verify.rs::jnl_090_verify_detects`

| 行 | 篡改 | 期望 |
|---|---|---|
| JNL-090.1 | 段 2 `magic` | `{2, item 1}` |
| JNL-090.2 | 段 2 `format_version = 2` | `{2, item 1}` |
| JNL-090.3 | 段 2 段头一字节(CRC 失配) | `{2, item 1}` |
| JNL-090.4 | 删除段 2 | `missing == [(2, 2)]` |
| JNL-090.5 | 段 3 `prev_header_sha256` 一字节 | `{3, item 2}` |
| JNL-090.6 | 段 3 `first_seq` 加 1,无 `RECOVERY_GAP` | `{3, item 2}` |
| JNL-090.7 | 同上但置 `RECOVERY_GAP` | `issues` 为空;`recovery_gaps == [3]` |
| JNL-090.8 | 段 1 中一条记录 `len` 改为非 8 倍数 | `{1, item 3}` |
| JNL-090.9 | 段 1 中一条 `TCrc` 的 CRC | `{1, item 3}` |
| JNL-090.10 | 段 1 中一个多段组缺末段 | `{1, item 3}` |
| JNL-090.11 | 段 2 首条不是 `Begin` | `{2, item 4}` |
| JNL-090.12 | 段 2 缺 `BootDone` | `{2, item 4}` |
| JNL-090.13 | 段 2 `BootDone.boot_records` 加 1 | `{2, item 4}` |
| JNL-090.14 | 段 2 `StateSnapshot{Boot}` 的 `n_books = 3` 而引导块只有 2 条 `L2Snapshot{Keyframe}` | `{2, item 4}` |
| JNL-090.15 | 段 1 `.seal` 的 `crc32c` | `{1, item 5}` |
| JNL-090.16 | 段 1 `.seal` 的 `sha256` | `{1, item 5}` |
| JNL-090.17 | 段 1 `.seal` 的 `last_seq` 减 1 | `{1, item 5}` |
| JNL-090.18 | 段 1 末条改为普通记录(不是 `End`) | `{1, item 5}` |

### JNL-091 冷副本本地保留:封存且过期且已上传才删除

- 规则:[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)、[journal-format 冷副本与热段](../../docs/subsystems/journal-format.md#冷副本与热段)
- 级别:table
- 里程碑:M1
- 前置:Linux;冷目录有段 1(`sealed_utc_ns = T`)及其 `.seal`;`ColdRetention{keep_days: 7, require_uploaded}` 按行;`now_utc_ns` 按行
- 实现:`crates/hft-journal/tests/retain.rs::jnl_091_cold_retire`

| 行 | `.uploaded` | `require_uploaded` | `now − T` | 期望 |
|---|---|---|---|---|
| JNL-091.1 | 有 | true | 7 × 86 400 s + 1 ns | `.jrn`、`.seal`、`.uploaded` 都删除;日期目录变空后删除;`RetireReport` 计 1 段 |
| JNL-091.2 | 有 | true | 7 × 86 400 s | 不删 |
| JNL-091.3 | 无 | true | 30 天 | 不删 |
| JNL-091.4 | 无 | false | 7 × 86 400 s + 1 ns | 删除 |
| JNL-091.5 | 有,但段 1 没有 `.seal`(未封存) | true | 30 天 | 不删 |

### JNL-092 冷副本子目录以 0770 建立,属组与 setgid 经父目录继承

- 规则:[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)、[持久元数据(engine_epoch 等)](README.md#持久元数据engine_epoch-等)
- 级别:table
- 里程碑:M1
- 前置:Linux,非 root;进程 `umask = 077`;`<cold_root>/prod/` 由测试建立,属组为测试用户的一个附加组 G,模式按行
- 实现:`crates/hft-journal/tests/coldperm.rs::jnl_092_cold_subdir_mode`

| 行 | `<cold_root>/prod/` 模式 | 期望(`seq/`、`<yyyymmdd>/`、`meta/` 三级新目录) |
|---|---|---|
| JNL-092.1 | 0750 | 模式恰为 0770;属组为进程的主组 |
| JNL-092.2 | 2750(setgid) | 模式恰为 2770;属组为 G |

### JNL-093 部署 ACL 下归档用户可写 .uploaded、可删除已提交的段

- 规则:[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:host
- 里程碑:M5
- 环境:dev2,按 deploy 路径表建立的 `<cold_root>/prod/`(`hft:hft-journal-ro` 2750,访问 ACL 与默认 ACL `u:hft-archive:rwx`)
- 授权:[需授权]
- 步骤:
  1. `hft-core@prod`(测试网实例)运行至少滚动 2 段
  2. `runuser -u hft-archive -- touch <段 1 旁>.uploaded`;`runuser -u hft-archive -- rm <段 1>.jrn`(测试用的已上传段)
  3. `runuser -u hft-replay -- touch <日期目录>/x`
- 通过标准:hft-core 建立的 `seq/` 与日期目录 `getfacl` 显示 `mask::rwx`、`user:hft-archive:rwx`;步骤 2 成功;步骤 3 以 `EACCES` 失败
- 证据:`ops-log/journal/JNL-093-<日期>.txt`(`getfacl` 输出与命令结果)
- 实现:`crates/hft-journal/tests/host/JNL-093.sh`

### JNL-094 DurableMeta 的读写与文件格式

- 规则:[持久元数据(engine_epoch 等)](README.md#持久元数据engine_epoch-等)
- 级别:table
- 里程碑:M1
- 前置:Linux;`DurableMeta::open(cold_root, Prod)`
- 实现:`crates/hft-journal/tests/meta.rs::jnl_094_durable_meta_rw`

| 行 | 操作 | 期望 |
|---|---|---|
| JNL-094.1 | `read("engine_epoch")`(从未写过) | `Ok(None)` |
| JNL-094.2 | `write_sync("engine_epoch", 0)` 后 `read` | `Ok(Some(0))`;文件内容 `"0\ncrc32c:" + hex(crc32c(b"0")) + "\n"` |
| JNL-094.3 | `write_sync("engine_epoch", u64::MAX)` 后 `read` | `Some(18 446 744 073 709 551 615)` |
| JNL-094.4 | 先写 5 再写 6 | `read` 为 `Some(6)`;目录中没有 `.tmp` 文件 |
| JNL-094.5 | `DurableMeta::open(cold_root, Sim)` 读同一键 | `Ok(None)`(实例分开) |
| JNL-094.6 | `meta/` 目录 | 首次 `open` 时建立,模式同 JNL-092 |

### JNL-095 DurableMeta 写到一半断电只会读到旧值或新值

- 规则:[持久元数据(engine_epoch 等)](README.md#持久元数据engine_epoch-等)
- 级别:table
- 里程碑:M1
- 前置:Linux;键 `engine_epoch` 已持久为 5
- 实现:`crates/hft-journal/tests/meta.rs::jnl_095_durable_meta_crash`

| 行 | 模拟的中断点 | 期望 `read` |
|---|---|---|
| JNL-095.1 | 新值 6 的 `.tmp` 已写完、未 rename | `Some(5)` |
| JNL-095.2 | `.tmp` 只写了一半(`"6\ncrc"`) | `Some(5)` |
| JNL-095.3 | rename 已完成 | `Some(6)` |
| JNL-095.4 | 键文件不存在、只有 `.tmp` | `None` |

### JNL-096 损坏的 DurableMeta 文件拒绝读取

- 规则:[持久元数据(engine_epoch 等)](README.md#持久元数据engine_epoch-等)
- 级别:table
- 里程碑:M1
- 前置:Linux;直接写 `meta/engine_epoch` 的内容
- 实现:`crates/hft-journal/tests/meta.rs::jnl_096_durable_meta_corrupt`

| 行 | 文件内容 | 期望 |
|---|---|---|
| JNL-096.1 | `"42\ncrc32c:00000000\n"`(CRC 不符) | `Err` |
| JNL-096.2 | `"4x\ncrc32c:<4x 的 CRC>\n"` | `Err` |
| JNL-096.3 | `"042\ncrc32c:<042 的 CRC>\n"`(前导零) | `Err` |
| JNL-096.4 | `"42\n"`(缺 CRC 行) | `Err` |
| JNL-096.5 | `"18446744073709551616\ncrc32c:<对应 CRC>\n"`(超 u64) | `Err` |
| JNL-096.6 | 空文件 | `Err` |

### JNL-097 DurableMeta 键名规则

- 规则:[持久元数据(engine_epoch 等)](README.md#持久元数据engine_epoch-等)
- 级别:table
- 里程碑:M1
- 前置:Linux;`write_sync(key, 1)`
- 实现:`crates/hft-journal/tests/meta.rs::jnl_097_durable_meta_keys`

| 行 | key | 期望 |
|---|---|---|
| JNL-097.1 | `"engine_epoch"` | `Ok` |
| JNL-097.2 | 64 个 `a` | `Ok` |
| JNL-097.3 | 65 个 `a` | `Err(InvalidInput)` |
| JNL-097.4 | `""` | `Err(InvalidInput)` |
| JNL-097.5 | `"Engine"` | `Err(InvalidInput)` |
| JNL-097.6 | `"a/b"`、`"a.tmp"` | `Err(InvalidInput)` |

### JNL-098 write_sync 的落盘顺序

- 规则:[持久元数据(engine_epoch 等)](README.md#持久元数据engine_epoch-等)
- 级别:proc
- 里程碑:M1
- 前置:CI Linux,`strace -f -e trace=openat,write,fsync,fdatasync,rename,renameat,renameat2`
- 步骤:
  1. 运行一次 `write_sync("engine_epoch", 9)` 的测试二进制
- 期望:系统调用顺序为:写 `meta/engine_epoch.tmp` → `fsync(tmp)` → rename 到 `meta/engine_epoch` → `fsync(meta 目录)`;函数在最后一个 `fsync` 之后返回
- 实现:`crates/hft-journal/tests/proc/meta_strace.rs::jnl_098_write_sync_order`

### JNL-099 held_by_instance 只统计本实例目录下本格式的段文件

- 规则:[段大小与热段内存](README.md#段大小与热段内存)
- 级别:table
- 里程碑:M1
- 前置:Linux tmpfs `hot_dir`;`prod/seq/` 下 3 个 1 MiB 段文件(其中 1 个 `Preparing`);`sim/seq/` 下 2 个 1 MiB 段文件;`prod/seq/` 下另有按行添加的文件
- 实现:`crates/hft-journal/tests/held.rs::jnl_099_held_by_instance`

| 行 | 额外文件 | 调用 | 期望 |
|---|---|---|---|
| JNL-099.1 | 无 | `held_by_instance(&[hot_dir], Prod)` | `HeldBySelf{hugepages_1g: 0, hugepages_2m: 0, shm_bytes: 3 145 728}` |
| JNL-099.2 | 无 | `(…, Sim)` | `shm_bytes = 2 097 152` |
| JNL-099.3 | `notes.txt`(1 MiB) | `(…, Prod)` | 同第 1 行 |
| JNL-099.4 | `0000000009.tmp`(1 MiB) | 同上 | 同第 1 行 |
| JNL-099.5 | `0000000009.jrn`,前 8 字节不是 `HFTJRNL1` | 同上 | 同第 1 行 |
| JNL-099.6 | `0000000009.jrn`,大小 5 B(读不到 8 字节 magic) | 同上 | 同第 1 行 |
| JNL-099.7 | `0000000009.jrn.preparing`,1 MiB 全零 | 同上 | `shm_bytes = 4 194 304`,未完成的段头仍计入 |
| JNL-099.8 | `0000000009.jrn.preparing`,大小 5 B | 同上 | `shm_bytes = 3 149 824`,按 4 KiB 向上取整 |
| JNL-099.9 | `notes.jrn.preparing`,1 MiB | 同上 | 同第 1 行,名称不属于预创建者 |

### JNL-100 崩溃遗留的本实例段不算新需求,另一实例的段照算

- 规则:[段大小与热段内存](README.md#段大小与热段内存)
- 级别:integ
- 里程碑:M1
- 前置:Linux,`sudo mount -t tmpfs -o size=8M none /mnt/shm8`(CI 作业)作 `hot_dir`;hft-rt `check_host` 的 `HostNeeds{shm_bytes = 6 MiB, held = held_by_instance(...)}`
- 步骤:
  1. `prod/seq/` 下留 6 MiB 段文件(崩溃遗留),剩余可用 2 MiB;以 `Prod` 计算 `held` 后 `check_host`
  2. 同样 6 MiB 段文件放在 `sim/seq/` 下,以 `Prod` 计算 `held` 后 `check_host`
- 期望:步骤 1 不置 `SHM_SHORT`;步骤 2 置 `SHM_SHORT`
- 实现:`crates/hft-journal/tests/held.rs::jnl_100_held_space_check`

### JNL-101 replayer 的离线写者只写普通文件系统目录

- 规则:[cargo feature](README.md#cargo-feature)、[journal-format 目录与命名](../../docs/subsystems/journal-format.md#目录与命名)
- 级别:table
- 里程碑:M1
- 前置:Linux;C0 的 `hot_dir = /dev/shm/<tmp>`、`cold_root = /tmp/<cold>`;`out_dir` 按行
- 实现:`crates/hft-journal/tests/offline.rs::jnl_101_open_offline`

| 行 | `out_dir` | 期望 |
|---|---|---|
| JNL-101.1 | `/tmp/<out>`(ext4,生产根之外) | `Ok`;写 1 000 个事件并关闭后目录中只有段文件(无 `.seal`、无预创建的下一段),最后一段以 `End` 结尾;读者能从 `Oldest` 读完 |
| JNL-101.2 | `/dev/shm/<out>`(tmpfs) | `Err(OfflineTarget)` |
| JNL-101.3 | `/tmp/<cold>/replay`(在 `cold_root` 之下) | `Err(OfflineTarget)` |
| JNL-101.4 | `/dev/shm/hft/x` | `Err(OfflineTarget)` |

### JNL-102 不启用 writer 特性时写者类型不可用

- 规则:[cargo feature](README.md#cargo-feature)
- 级别:table
- 里程碑:M1
- 前置:测试在临时目录生成一个只依赖 `hft-journal` 默认特性的工程,以嵌套 `cargo check` 编译各行代码(不用 `trybuild`:它沿用被测 crate 的特性,`--all-features` 下 `writer` 已开)
- 实现:`crates/hft-journal/tests/feature.rs::jnl_102_writer_feature_gate`

| 行 | 代码 | 期望 |
|---|---|---|
| JNL-102.1 | 使用 `hft_journal::JournalWriter` | 编译失败 |
| JNL-102.2 | 使用 `Persister`、`SegmentPreparer`、`HotRetirer` | 各自编译失败 |
| JNL-102.3 | 使用 `JournalReader`、`verify`、`cold_retire`、`DurableMeta` | 编译通过 |
| JNL-102.4 | 不开 `writer` 时 `cargo build -p hft-journal --examples` | `testwriter` 不参与构建(`required-features`),构建退出 0(显式 `--example testwriter` 时 cargo 报缺少特性) |

### JNL-103 unsafe 只出现在 layout.rs 且每处带 SAFETY 注释

- 规则:[读者](README.md#读者)
- 级别:unit
- 里程碑:M1
- 前置:扫描 `src/**/*.rs`(去掉注释与字符串字面量)
- 步骤:
  1. 找出全部 `unsafe` 关键字
- 期望:全部位于 `src/layout.rs`;每处前 3 行内有 `// SAFETY:`
- 实现:`crates/hft-journal/tests/hygiene.rs::jnl_103_unsafe_confined`

### JNL-104 raw 写者不判断写哪些帧,所有记录带 CRC

- 规则:[raw journal 模式](README.md#raw-journal-模式)、[journal-format 三类 journal](../../docs/subsystems/journal-format.md#三类-journal)
- 级别:unit
- 里程碑:M1
- 前置:`H(R0)`;交给写者 100 个 `RawRx`,其中 30 个 `RawHdr.rflags = SAMPLED`
- 步骤:
  1. 全部 `append_var`
- 期望:journal 中恰有 100 条 `RawRx`(写者不过滤),逐条带 `CRC`,`RawHdr` 原样保留
- 实现:`crates/hft-journal/tests/writer.rs::jnl_104_raw_writes_all`

### JNL-105 tmpfs 小段模式与内存后端写出相同的段

- 规则:[小段 + 4K 页模式(tmpfs 热段)](README.md#小段--4k-页模式tmpfs-热段)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:Linux;同一个事件脚本(500 个事件、两次滚动、`TMulti` 与 `TCrc`)分别在内存后端与 tmpfs(`page = Normal`)上运行;`RollInfo` 与 `WriterEnv` 相同
- 步骤:
  1. 比较两边每个段的字节(写者行与 persist 行除外)
  2. tmpfs 一边在稳态再写 10 000 个事件,前后读写入线程的 `getrusage(RUSAGE_THREAD).ru_minflt`,排除同进程其他测试线程的缺页
- 期望:步骤 1 逐字节相同;步骤 2 缺页次数之差为 0
- 实现:`crates/hft-journal/tests/tmpfs.rs::jnl_105_tmpfs_equals_mem`

### JNL-106 1 GiB 大页上的写者、persist 与退役

- 规则:[实现说明](README.md#实现说明)、[段大小与热段内存](README.md#段大小与热段内存)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,`/dev/hugepages`(1 GiB 页),`seq` 段 1 GiB、`hot_keep = 1`
- 授权:[需授权]
- 步骤:
  1. 停机窗口内运行验证包里的主机测试 `tests/host/JNL-106.sh`(调用发布制品中的测试二进制):写满 3 段(安全阀滚动)、persist、退役
- 通过标准:`fstatfs` 为 `HUGETLBFS_MAGIC` 且页大小 1 GiB;3 次滚动全部成功;退役后 `HugeAudit` 为 0;预创建(`MAP_POPULATE`)耗时记录在报告中
- 证据:`ops-log/journal/JNL-106-<日期>.json`
- 实现:`crates/hft-journal/tests/host/JNL-106.sh`

### JNL-107 写者线程在基准期间没有 munmap 与 futex 唤醒

- 规则:[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:host
- 里程碑:M1.5
- 环境:参考机(`c8g.4xlarge` 按需租用),bpftrace
- 授权:无
- 步骤:
  1. 运行 `journal_append_80b` 基准(含滚动与退役)期间,bpftrace 按写者线程 tid 计数 `sys_enter_munmap` 与 `futex(FUTEX_WAKE)`
- 通过标准:两项计数都为 0
- 证据:`ops-log/bench/JNL-107-<日期>.txt`
- 实现:`crates/hft-journal/tests/host/JNL-107.sh`

### JNL-108 定长追加基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`journal_append_80b`
- 前置:≤ 128 B 定长体、无 CRC、热段已预触摸
- 实现:`crates/hft-journal/benches/append.rs::jnl_108_journal_append_80b`

### JNL-109 raw 多段追加基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`journal_append_raw_512b`
- 前置:512 B raw payload、含 CRC,`append_var` 与 `reserve_var` 两条路径
- 实现:`crates/hft-journal/benches/append.rs::jnl_109_journal_append_raw_512b`

### JNL-110 publish 基准(有、无尾随读者)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[读者](README.md#读者)
- 级别:bench
- 里程碑:M1
- 基准名:`journal_publish`
- 前置:两组:无读者;一个按 `tail_poll_us = 50` 尾随的读者在另一核
- 实现:`crates/hft-journal/benches/append.rs::jnl_110_journal_publish`

### JNL-111 滚动切换基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`journal_roll_switch`
- 前置:下一段已预创建;只计 `roll` 本身,不含引导块内容
- 实现:`crates/hft-journal/benches/roll.rs::jnl_111_journal_roll_switch`

### JNL-112 引导块写入基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`journal_boot_400books`
- 前置:400 个簿 × 200 档 + 快照,约 1.5 MB
- 实现:`crates/hft-journal/benches/roll.rs::jnl_112_journal_boot_400books`

### JNL-113 end_event 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`journal_end_event`
- 实现:`crates/hft-journal/benches/append.rs::jnl_113_journal_end_event`

### JNL-114 热段退役对热线程的影响基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[persist 与后台维护(persist 线程调用)](README.md#persist-与后台维护persist-线程调用)
- 级别:bench
- 里程碑:M1
- 基准名:`journal_retire_tlb`
- 前置:热线程测环跳分布,persist 线程在其间退役段;报告退役前后两段分布
- 实现:`crates/hft-journal/benches/retire.rs::jnl_114_journal_retire_tlb`

### JNL-115 读者顺序扫描基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`journal_read_scan`
- 前置:冷副本在页缓存中
- 实现:`crates/hft-journal/benches/read.rs::jnl_115_journal_read_scan`

### JNL-116 1 GiB 未复制数据的恢复基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`journal_recover_1g`
- 前置:1 GiB 已提交未复制的热段,冷目录在本地 NVMe/EBS
- 实现:`crates/hft-journal/benches/recover.rs::jnl_116_journal_recover_1g`

### JNL-117 persist 滞后在目标卷上的分布

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,`prod` 与 `sim` 同机运行;冷副本卷为清单 `[storage]` 中的 io2 或 gp3
- 授权:无
- 步骤:
  1. 连续 24 h 采集指标 `journal_persist_lag_ms`(watchtower)
- 通过标准:按冷副本卷类型满足 [延迟与资源契约](README.md#延迟与资源契约)"persist 滞后"一行的 p99(含预创建期间的暂停)
- 证据:`ops-log/journal/JNL-117-<日期>.json`(watchtower 导出的直方图)
- 实现:`crates/hft-journal/tests/host/JNL-117.sh`

### JNL-118 读者对任意字节的段文件不 panic

- 规则:[读者](README.md#读者)、[恢复与校验](README.md#恢复与校验)
- 级别:fuzz
- 里程碑:M1
- 目标:`crates/hft-journal/fuzz/reader`:把输入当作一个段文件(不足 4 096 B 时补零),依次 `JournalReader` 从 `Oldest` 读到 `End`/`Err`、`verify`、恢复的尾部扫描
- 语料:JNL-089 的段文件、JNL-090 与 JNL-073 的损坏样本
- 判定:不 panic、不越界、不死循环、单输入不超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz));重组缓冲不超过 `reassembly_cap`
- 实现:`crates/hft-journal/fuzz/fuzz_targets/reader.rs::jnl_118_fuzz_reader`

### JNL-119 24 h 运行后每个段都有完整引导块且校验无错

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[恢复与校验](README.md#恢复与校验)、[journal-format 段滚动与引导块](../../docs/subsystems/journal-format.md#段滚动与引导块)
- 级别:host
- 里程碑:M4
- 环境:dev2 的 `sim` 实例(tmpfs 小段模式),连续运行 24 h
- 授权:无
- 步骤:
  1. 运行结束后 `cargo xtask journal-verify <cold_root>/sim/seq` 与各 raw journal 目录
- 通过标准:`issues` 与 `missing` 为空;每个 `seq` 段的第一条是 `Begin` 且有 `BootDone`;`recovery_gaps` 只出现在有记录的护栏停机或崩溃之后
- 证据:`ops-log/journal/JNL-119-<日期>.json`(verify 报告)
- 实现:`crates/hft-journal/tests/host/JNL-119.sh`

### JNL-120 从任一段的引导块起步与连续读取重建出相同状态

- 规则:[写者](README.md#写者)、[读者](README.md#读者)、[journal-format 段滚动与引导块](../../docs/subsystems/journal-format.md#段滚动与引导块)
- 级别:integ
- 里程碑:M1
- 前置:`H(C0)` 但 `segment_bytes = 256 KiB`;测试内的桩状态机 `Stub`(对每条 Input 记录的字节做 xxh3-128 累积,引导块里以 `StateSnapshot{Boot}` 写出累积值,从引导块起步时读回);事件流 20 000 个,其中安全阀滚动与整点滚动交替发生(整点滚动由测试在每第 3 000 个事件之后调用 `roll(Hourly, …)`,模拟 `Timer{Sys, Roll}`),另有 2 次 `NextNotReady` 后重试
- 步骤:
  1. 读者 A 从 `Oldest` 连续读完,得到末状态 `h_A`
  2. 对每个段 k,读者 B_k 以 `SegmentBoot(k)` 起步、从引导块恢复 `Stub` 后读完,得到 `h_k`
- 期望:所有 `h_k == h_A`;每个段的首条是 `Begin`、都有 `BootDone`;段头 `roll_reason` 既有 `Size`(2)也有 `Hourly`(1)
- 实现:`crates/hft-journal/tests/roll.rs::jnl_120_boot_blocks_self_sufficient`

### JNL-121 CRC 由写者按 journal 类别与 KindInfo.crc 决定,调用方的 CRC 位被忽略

- 规则:[写者](README.md#写者)、[journal-format 三类 journal](../../docs/subsystems/journal-format.md#三类-journal)、[journal-format 记录对齐与 CRC](../../docs/subsystems/journal-format.md#记录对齐与-crc)
- 级别:table
- 里程碑:M1
- 前置:seq 写者 `H(C0)`、`open0`;raw 写者 `H(R0)`(以 `Startup` 首次 `roll` 后);每行追加一条记录、`end_event()`、`publish()` 后读该记录头
- 实现:`crates/hft-journal/tests/writer.rs::jnl_121_crc_policy_by_writer`

| 行 | 写者 / 记录 / 调用方 flags | 期望记录头 |
|---|---|---|
| JNL-121.1 | seq / `TIn`(Never)/ `CRC` | `len = 104`,`flags` 不含 `CRC` |
| JNL-121.2 | seq / `TCrc`(Always)/ 0 | `len = 72`,`flags` 含 `CRC`,末 4 字节 = 记录 [0, 68) 的 CRC32C |
| JNL-121.3 | seq / `TMulti`(Never,逻辑体 100 B)/ `CRC` | 单段,`flags` 不含 `CRC` |
| JNL-121.4 | raw / `TIn` / 0 | `len = 112`,`flags` 含 `CRC` |
| JNL-121.5 | seq / `TIn` / `PART` | `flags` 不含 `PART` |
| JNL-121.6 | seq / 先追加 `TIn`(调用方 flags = `EVENT_END`)再追加 `TOut`,然后 `end_event()` | `TIn` 的 `flags` 不含 `EVENT_END`;`TOut` 含 `EVENT_END` |

### JNL-122 DurableMeta 不需要 writer 特性

- 规则:[持久元数据(engine_epoch 等)](README.md#持久元数据engine_epoch-等)、[cargo feature](README.md#cargo-feature)
- 级别:unit
- 里程碑:M1
- 前置:同 JNL-102 的临时工程(不开 `writer`);临时目录作 `cold_root`
- 步骤:
  1. 编译并运行:`DurableMeta::open(root, &InstanceName::Prod)`、`write_sync("engine_epoch", 42)`、`read("engine_epoch")`
- 期望:
  - 编译通过;`read` 返回 `Ok(Some(42))`;`<root>/prod/meta/engine_epoch` 内容为 `"42\ncrc32c:<"42" 的 CRC32C>\n"`
- 实现:`crates/hft-journal/tests/feature.rs::jnl_122_durable_meta_without_writer`

### JNL-123 测试写者按参数写出可被读者完整读回的 journal

- 规则:[cargo feature](README.md#cargo-feature)
- 级别:proc
- 里程碑:M1
- 前置:Linux;`cargo build -p hft-journal --example testwriter --features writer`;热目录为 `/dev/shm` 下的临时目录,冷目录为 `/tmp` 下的临时目录
- 步骤:
  1. `testwriter --hot H --cold C --instance prod --synth-bbo 1000 --inst 101 --bid 6000000 --ask 6000001 --qty 50000000 --t0-ns 1759276810000000000 --step-ns 1000000 --publish-every 10 --segment-bytes 1048576 --sync-interval-ms 50 --seal`
  2. 以 `ReaderConfig::new(H, C, Prod, "seq")`、`end = LastCompleteEvent` 读全部记录
  3. 同样参数去掉 `--seal`,写到新的目录对 H2、C2,再读
  4. 不给 `--hot` 运行
  5. `--hot` 指向 ext4 目录运行
- 期望:
  - 步骤 1 退出 0;C 中每个段都有 `.seal`
  - 步骤 2 读到恰好 1 000 条 `Bbo`,第 i 条 `h.inst = 101`、`bid_px = 6 000 000`、`ask_px = 6 000 001`、`bid_qty = ask_qty = 50 000 000`、`h.ex_ts_ns = 1 759 276 810 000 000 000 + i × 1 000 000 − 2 000 000`,`h.hw_rx_ns` 在 i 偶数时为 `h.ex_ts_ns + 1 000 000`、奇数时为 0;每条带 `EVENT_END`
  - 步骤 3 退出 0;最后写入的一段(段号最大的非 `Preparing` 段;写者另保持一个已准备好的下一段)`state = Open`、无 `.seal`,读者读到 1 000 条 `Bbo`
  - 步骤 4 退出 1;步骤 5 退出 2(`DiskBacked`)
- 实现:`crates/hft-journal/tests/proc_testwriter.rs::jnl_123_testwriter_roundtrip`

### JNL-124 预创建各崩溃边界均可同号重开,保留已提交前缀

- 规则:[实现说明](README.md#实现说明)的预创建与恢复、[journal-format 目录与命名](../../docs/subsystems/journal-format.md#目录与命名)
- 级别:table
- 里程碑:M1
- 前置:内存后端与 Linux tmpfs 各跑全表;`H(C0)` 写到 seq 20 并发布,准备段 2;按表将段 2 物化为相应崩溃文件,然后重建写者与后台环
- 实现:`crates/hft-journal/tests/recover.rs::jnl_124_preparation_crash_publication`
- 共同期望:恢复删除遗留的段 2 预创建文件;段 1 已提交前缀(屏蔽原子行)逐字节保留,仅追加 seq 21 的 `End{Recovery}`;下一次滚动仍是段 2,`first_seq = 22`,正式文件存在而临时名消失;旁边的 `notes.jrn.preparing` 保留。同名正式文件的拒绝覆盖与失败后的临时文件清理由 JNL-042.4 验证。

| 行 | 杀点对应的段 2 文件 |
|---|---|
| JNL-124.1 | `.jrn.preparing` 已创建,长度 0 |
| JNL-124.2 | `.jrn.preparing` 已扩到段长,内容全零 |
| JNL-124.3 | `.jrn.preparing` 只写入 magic 前 4 B,其余全零 |
| JNL-124.4 | `.jrn.preparing` 的 [P] 段头完整,尚未 rename |
| JNL-124.5 | 已原子发布为 `.jrn`,尚未交给写者,`Preparing` |

## 验收

- 里程碑 M1:本文件中里程碑为 M1 的用例全部有同编号的实现且通过(`cargo xtask test-trace`);`unit`(内存后端)在任意平台运行,标 Linux 的 `unit` 与全部 `proc` 在 CI 的 Linux 作业中随 PR 运行。
- 大例数运行(每夜):JNL-072 1 000 个种子(PR 50 个),JNL-073 4 096 例。
- fuzz:JNL-118 在 PR 冒烟、CI 每夜与服务器累计中零失败(不计入上线门槛 ③,口径见 [testing §3.3](../../docs/testing.md#33-fuzz))。
- 基准:JNL-108–JNL-116 从 M1.5 起每个里程碑末在参考机上满足 [延迟与资源契约](README.md#延迟与资源契约) 的预算与 [benchmarks](../../benchmarks/README.md) 的单 crate 上限;`dev2` 与 tmpfs 热段只记录。
- 主机证据:JNL-107(M1.5,参考机)、JNL-119(M4,dev2 的 `sim` 实例 24 h)、JNL-093(M5,dev2)、JNL-106 与 JNL-117(M7,统一升级主机)。
