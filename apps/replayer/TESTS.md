---
component: replayer
prefix: RPL
spec: README.md
---

# replayer 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。逐字节比对的分类表、状态哈希算法与关键帧规则归 [hft-sequencer](../../crates/hft-sequencer/README.md);Model 检查点的键与重建点定义归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md#model-检查点);按事件加权统计的计算、分层与上下界算法归 [hft-telemetry](../../crates/hft-telemetry/README.md)(其单元用例在 hft-telemetry 的 TESTS.md);压力用例的变换与判定归 [hft-sim](../../crates/hft-sim/README.md)。本文件只验证 replayer 自己的输入定位、驱动、比对、选项、报告与退出码。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | RPL-095、RPL-104、RPL-241、RPL-242 |
| 目录 | 不适用:非规范 |
| 使用本组件 | RPL-001–RPL-004、RPL-070、RPL-079 |
| verify | RPL-010–RPL-032、RPL-040–RPL-063、RPL-070–RPL-079、RPL-270–RPL-271 |
| sim | RPL-090–RPL-108、RPL-120–RPL-129、RPL-140–RPL-145、RPL-259 |
| compare | RPL-160–RPL-173 |
| latency | RPL-190–RPL-206 |
| bench(可选扩展) | RPL-256 |
| raw-decode(可选扩展) | RPL-257 |
| `--chain`(可选扩展) | RPL-258 |
| 输入定位与退出码 | RPL-001、RPL-003、RPL-004、RPL-015、RPL-030、RPL-049、RPL-220–RPL-225 |
| 实现说明 | RPL-002、RPL-031、RPL-032、RPL-093、RPL-094、RPL-100、RPL-107、RPL-125–RPL-127、RPL-145、RPL-172、RPL-206、RPL-243 |
| 延迟与资源契约 | RPL-250、RPL-251、RPL-252–RPL-255、SIM-213 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | RPL-105、RPL-106、RPL-240、RPL-241 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | RPL-260–RPL-269 |
| 开发备忘 | 不适用:非规范 |

## 用例

约定(用例 `前置` 引用下列名字时即取此处的内容;各用例另行写出它依赖的参数):

- **样本 T1**(`fixtures/replay/testkit-1h`,S3 指针 + 哈希):hft-testkit 假交易所驱动进程内 hft-core(实例名 `prod`)运行 1 h 录下的 seq journal;venue `v-spot`(现货)、`v-perp`(USDT 永续),一个 Live 账户 `acct-l`;hft-testkit 测试策略 A 的实例 `tsa`(带 Model:`LAYOUT_VERSION = 1`、`PATH_VERSION = 1`、`path_settle_ms = 120 000`)绑定 `acct-l`;引擎配置 `keyframe_period_ms = 300 000`、`keyframe_max_events = 1 500 000`、`advisor.model_checkpoint_ms = 600 000`;journal 两段,段首引导块快照记为 `B1`、`B2`,关键帧依次记为 `K1`、`K2`…(每 5 min 一个);本地冷副本目录记为 `$T1`。
- **样本 T3**(`fixtures/replay/testkit-3h`):与 T1 同配置运行 3 h;advisor 在 t = 1 h 之后写出的第一份检查点另存为 `ckpt-1h/tsa.ckpt`(`last_seq = L1`);t = 2 h 之后第一个关键帧的 seq 记为 `H2`。
- **样本 S1**(`fixtures/replay/sim-1h`):实例 `sim`,Sim 账户 `sim-a`(模拟 `v-spot`、`v-perp`);journal 以段首快照 `P0` 开头,`P0` 之后第一条 Input 是 `ControlCmd{SimReset{sim-a}}`,随后一条 `SetParams` 创建 hft-testkit 测试策略 A 的实例 `tss`(绑定 `sim-a`,带 Model);共录 1 h,其中 30 min 处的关键帧记为 `P6`;journal 中 `[sim]` 记录的种子为 7;窗口内 sim-gw 的 `quiet_advance = 0`。
- **合成 journal**:测试内以 `hft-journal` 的 `JournalWriter::open_offline` 在临时目录写出的段(重算段链哈希与记录校验);"篡改"指在样本副本上改字节后同样重算校验,使输入本身可读。
- 命令行中的 `$OUT` 是每个用例新建的空目录。

### RPL-001 缺必填参数或参数非法时退出 2 且不读取输入

- 规则:[使用本组件](README.md#使用本组件)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M2
- 前置:样本 T1;每行的命令以 `replayer` 二进制运行(表中以子命令开头的行省略了二进制名);以 `strace -f -e trace=openat` 记录每行运行打开的文件
- 实现:`apps/replayer/tests/cli.rs::rpl_001_bad_args_exit_2`

| 行 | 命令行 | 期望 |
|---|---|---|
| RPL-001.1 | `replayer`(无子命令) | 退出 2;没有打开 `$T1` 下任何文件 |
| RPL-001.2 | `replayer verify`(缺 `--journal`) | 退出 2 |
| RPL-001.3 | `replayer verify --journal $T1 --advice foo` | 退出 2;没有打开 `$T1` 下任何文件 |
| RPL-001.4 | `replayer verify --journal $T1 --instance live` | 退出 2 |
| RPL-001.5 | `replayer verify --journal $T1 --report a.json --report-dir $OUT` | 退出 2 |
| RPL-001.6 | `replayer verify --journal $T1 --from-seq 100 --to-seq 99` | 退出 2 |
| RPL-001.7 | `replayer sim --journal $T1`(缺 `--out`) | 退出 2 |
| RPL-001.8 | `compare --b $T1 --map acct-l=sim-a --out $OUT`(缺 `--a`) | 退出 2 |
| RPL-001.9 | `compare --a $T1 --b $T1 --out $OUT`(缺 `--map`) | 退出 2 |
| RPL-001.10 | `latency --journal $T1 --burst-gap-ns 50000 --out $OUT`(缺 `--cal-max-err-ns`) | 退出 2;`$OUT` 为空 |
| RPL-001.11 | `latency --journal $T1 --cal-max-err-ns 1000 --out $OUT`(缺 `--burst-gap-ns`) | 退出 2;`$OUT` 为空 |
| RPL-001.12 | `replayer latency --journal $T1 --cal-max-err-ns 1000 --burst-gap-ns 50000 --from 2026-10-01T01:00:00Z --to 2026-10-01T01:00:00Z --out $OUT` | 退出 2(窗口为空) |
| RPL-001.13 | `replayer latency …(同上)--from 2026-10-01T01:00:01Z --to 2026-10-01T01:00:00Z` | 退出 2 |

### RPL-002 replayer 与同一提交的 hft-core 打印相同的版本串与策略集合

- 规则:[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M2
- 前置:同一提交、同一剖面构建的 `replayer` 与 `hft-core` 二进制
- 步骤:
  1. 分别运行 `replayer --version` 与 `hft-core --version`
- 期望:
  - 两者都退出 0;第一行版本串逐字节相同
  - 策略集合部分(每行 `kind_id` 与策略名)逐行相同,行数等于 `hft-strategy-registry` 的注册条数
- 实现:`apps/replayer/tests/build.rs::rpl_002_version_matches_core`

### RPL-003 各子命令选项的缺省值

- 规则:[使用本组件](README.md#使用本组件)、[verify](README.md#verify)、[sim](README.md#sim)、[compare](README.md#compare)、[latency](README.md#latency)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M2
- 前置:样本 T1、S1;S1 的副本 `S1b` 另含第二个 Sim 账户 `sim-b`(由同一次运行录下);测试策略 A 的 Model 构造计数器(hft-testkit 测试策略暴露的 `MODEL_NEW_COUNT`)在每行开始前清零
- 实现:`apps/replayer/tests/cli.rs::rpl_003_defaults`

| 行 | 选项(未给出时) | 输入与命令 | 期望 |
|---|---|---|---|
| RPL-003.1 | verify `--advice` | `verify --journal $T1 --report $OUT/r.json` | `r.json` 中 `recompute = "skipped"`、`skipped_reason = "advice_journal"`;`MODEL_NEW_COUNT = 0` |
| RPL-003.2 | verify `--keep-going` | T1 中 `K2` 之后第一条 `OrderDecision` 与 `K5` 之后第一条 `OrderDecision` 各篡改 1 字节;`verify --journal …` | 退出 1;分歧明细只有 1 处,seq = `K2` 之后那一条 |
| RPL-003.3 | verify `--from-seq` | T1 中 `B1` 之后第一条 `Bbo` 改价 1 tick(改变其后一条决策) | 退出 1;分歧明细的"最近快照 seq" = `B1` |
| RPL-003.4 | sim `--advice` | `sim --journal S1 --out $OUT` | `MODEL_NEW_COUNT = 1`(`tss` 的 Model 被重算);journal 中 `tss` 的 `Advice` 记录投递给 sequencer 的次数为 0,sequencer 收到的 `tss` Advice 全部来自重算 |
| RPL-003.5 | sim `--seed` | `sim --journal S1 --out $OUT` | `sim.json.seed = 7` |
| RPL-003.6 | sim `--accounts` | `sim --journal S1b --out $OUT` | `sim.json.accounts = ["sim-a", "sim-b"]` |
| RPL-003.7 | compare `--window-ms` | `compare --a $T1 --b $T1 --map acct-l=acct-l --out $OUT` | `compare.json.window_ms = 2000` |
| RPL-003.8 | compare `--from`/`--to` | A 覆盖 00:00–02:00Z、B 覆盖 01:00–03:00Z 的两份 journal | `compare.json` 的比对区间 = [01:00Z, 02:00Z] |
| RPL-003.9 | latency `--from`/`--to` | `latency --journal $T1 --cal-max-err-ns 1000 --burst-gap-ns 50000 --out $OUT` | `latency.json.cfg.from_seq` = T1 第一条记录的 seq,`to_seq` = 最后一条记录的 seq |
| RPL-003.10 | `--cache-dir` | 以本地 S3 模拟服务提供 T1 的 S3 前缀,运行 `verify --journal s3://b/p/h1/prod/…` | 运行期间 `$TMPDIR` 下出现一个新目录且含下载的段;进程退出后该目录不存在 |

### RPL-004 给出 --cache-dir 时保留下载并复用哈希相符的已有文件

- 规则:[输入定位与退出码](README.md#输入定位与退出码)
- 级别:integ
- 里程碑:M5
- 前置:本地 S3 模拟服务提供 T1 的 S3 前缀(带主机段 `h1`)与 manifest;`--cache-dir $C`
- 步骤:
  1. `replayer verify --journal s3://b/p/h1/prod/… --cache-dir $C`
  2. 同一命令再运行一次
  3. 把 `$C` 中一个段文件改 1 字节后再运行一次
- 期望:
  - 步骤 1 退出 0;`$C` 中有 manifest 列出的全部段;S3 模拟服务记录的 GET 次数 = 段数
  - 步骤 2 退出 0;GET 次数不增加
  - 步骤 3 退出 0;只对被改的那一段多发 1 次 GET,下载后 `$C` 中该文件的 SHA-256 等于 manifest 值
- 实现:`apps/replayer/tests/input.rs::rpl_004_cache_dir_reuse`

### RPL-010 hft-testkit 驱动的 1 h journal 回放逐字节一致

- 规则:[verify](README.md#verify)
- 级别:replay
- 里程碑:M2
- 输入:`fixtures/replay/testkit-1h`(样本 T1);`replayer verify --journal $T1 --report $OUT/r.json`
- 期望:
  - 退出 0;`r.json`:`verify = "pass"`,无 `first_divergent_seq`、无 `component`
  - 分歧明细 `recovery_diffs` 为空;比较过的 Output 记录数 = T1 中 `B1` 之后的 Output 类记录数(不含 Meta)
  - 比较过的快照数 = T1 中 `StateSnapshot` 记录数(`B1`、`B2` 与全部关键帧)
- 实现:`apps/replayer/tests/verify.rs::rpl_010_testkit_1h_consistent`

### RPL-011 含 Sim 账户回报的 sim 实例 journal 回放一致且不读取模拟配置

- 规则:[verify](README.md#verify)
- 级别:replay
- 里程碑:M4
- 输入:`fixtures/replay/sim-1h`(样本 S1);`replayer verify --journal S1 --instance sim`;以 `strace -e trace=openat` 记录打开的文件
- 期望:
  - 退出 0;`verify = "pass"`
  - 进程打开的文件只有 S1 的段文件与二进制自身的依赖库;没有任何 `*.toml`
  - 测试构建中 `SimExchange::new` 的调用计数为 0
- 实现:`apps/replayer/tests/verify.rs::rpl_011_sim_instance_consistent`

### RPL-012 DST 生成的种子 journal 全部回放一致

- 规则:[verify](README.md#verify)
- 级别:dst
- 里程碑:M2
- 前置:hft-testkit DST 框架的默认场景集(两个测试策略、假交易所两 venue、Live 与 Sim 账户各一)
- 故障:hft-testkit 默认故障目录(断连、乱序、重复、丢回报、`CrashFault` 崩溃后以 `RecoverDriver` 重建),每个种子按其故障表注入
- 种子:PR 100(提交派生);每夜 100(日期派生);回归集全部
- 断言:每个种子运行结束后,以其内存段写出的 journal 运行 `replayer verify --advice recompute`(无检查点,Model 从实例创建处起算)均退出 0;`verify = "pass"`、`recompute = "pass"`;失败种子的 journal 与故障表留在 `target/dst/<seed>/`
- 实现:`apps/replayer/tests/dst.rs::rpl_012_dst_journals_verify`

### RPL-013 未给 --from-seq 时从第一个段首快照起步并比对其后全部 Output

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:样本 T1 的副本;`B1` 之后第一条使测试策略产生 `OrderDecision` 的 `Bbo`(seq `x`)的买价加 1 tick
- 步骤:
  1. `replayer verify --journal <副本>`
- 期望:
  - 退出 1;分歧明细:seq = `x` 之后同一事件内的第一条 `OrderDecision` 的 seq,kind = `OrderDecision`,"最近快照 seq" = `B1`
- 实现:`apps/replayer/tests/verify.rs::rpl_013_default_start_first_segment_snapshot`

### RPL-014 从 --from-seq 之前最近的快照起步

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:样本 T1 的副本;在 `K3 − 5` 处的 `Bbo` 与 `K3 + 5` 处的 `Bbo` 中只篡改每行指明的那一条(价加 1 tick,两条都会改变其后的决策)
- 实现:`apps/replayer/tests/verify.rs::rpl_014_start_at_nearest_snapshot`

| 行 | `--from-seq` | 篡改位置 | 期望 |
|---|---|---|---|
| RPL-014.1 | `K3` | `K3 − 5` | 退出 0(起点 `K3`,篡改在起点之前) |
| RPL-014.2 | `K3` | `K3 + 5` | 退出 1;"最近快照 seq" = `K3` |
| RPL-014.3 | `K3 + 1` | `K3 − 5` | 退出 0 |
| RPL-014.4 | `K4 − 1` | `K3 + 5` | 退出 1;"最近快照 seq" = `K3` |
| RPL-014.5 | `K4` | `K3 + 5` | 退出 0(起点 `K4`) |

### RPL-015 --from-seq 之前没有快照时退出 3

- 规则:[verify](README.md#verify)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 前置:样本 T1;`$T1/seg2` 只含 T1 的第 2 段
- 里程碑:M2
- 实现:`apps/replayer/tests/verify.rs::rpl_015_no_snapshot_before_from_exit_3`

| 行 | 输入与 `--from-seq` | 期望 |
|---|---|---|
| RPL-015.1 | `$T1`,`--from-seq` = 第 1 段第一条记录(`SegmentRoll{Begin}`)的 seq | 退出 3;没有任何 Output 被比较 |
| RPL-015.2 | `$T1/seg2`,`--from-seq` = `B2 − 1` | 退出 3 |
| RPL-015.3 | `$T1/seg2`,`--from-seq` = `B2` | 退出 0 |

### RPL-016 快照自检:头部 state_hash 为 0 时自算,非 0 时与自算值核对

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:样本 T1;`B1` 是引导块快照(头部 `state_hash` 非 0,实盘值记为 `H`);`K1` 是关键帧(头部 `state_hash = 0`);`--from-seq` 按行给出
- 实现:`apps/replayer/tests/verify.rs::rpl_016_snapshot_self_check`

| 行 | 改动 | `--from-seq` | 期望 |
|---|---|---|---|
| RPL-016.1 | 无 | `B1` | 退出 0;起点自检:replayer 对 `B1` 快照字节算出的哈希 = 重新序列化后的哈希 = `H` |
| RPL-016.2 | 无 | `K1` | 退出 0;对 `K1` 字节自算的哈希非 0 且 = 重新序列化后的哈希;没有因头部为 0 产生的分歧 |
| RPL-016.3 | `B1` 头部 `state_hash` 改为 `H ^ 1` | `B1` | 退出 3;标准输出指明 seq `B1` 与"快照自检失败" |
| RPL-016.4 | `K1` 头部 `state_hash` 改为 1 | `K1` | 退出 3 |
| RPL-016.5 | `K2` 头部 `state_hash` 改为 1 | `B1` | 退出 1;分歧 seq = `K2`,kind = `StateSnapshot`(重放写出的关键帧头部为 0,与 journal 逐字节不同) |

### RPL-017 每个快照位置比较状态哈希并给出组件级二分结果

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:样本 T1 的副本;关键帧 `K2` 中 hft-risk 节(节 4)的一个计数字段加 1(改动后重算记录校验)
- 步骤:
  1. `replayer verify --journal <副本> --report $OUT/r.json`
- 期望:
  - 退出 1;`r.json`:`verify = "fail"`、`first_divergent_seq = K2`、`component = "hft-risk"`
  - 分歧明细:kind = `StateSnapshot`;字段级 diff 指向节 4 的该字段;`K2` 之前的全部 Output 已比较且一致
- 实现:`apps/replayer/tests/verify.rs::rpl_017_snapshot_hash_and_bisect`

### RPL-018 VerifyDriver 不赋新 seq:journal 多出一条 Output 时在该 seq 报分歧

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal:T1 第 1 段的副本,在 seq `y`(某条 `OrderDecision` 之后、同一事件内)插入一条 `Note{source = Strategy}` Output 记录,其后各记录 seq 顺延
- 步骤:
  1. `replayer verify --journal <副本>`
- 期望:
  - 退出 1;分歧 seq = `y`;期望侧(journal)kind = `Note`,实际侧为"重放未产生";`y` 之前的记录全部一致
- 实现:`apps/replayer/tests/verify.rs::rpl_018_extra_output_in_journal`

### RPL-019 重放产生而 journal 缺失的 Output 在该 seq 报分歧

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal:T1 第 1 段的副本,删去 seq `z` 的一条 `OrderDecision`,其后各记录 seq 前移
- 步骤:
  1. `replayer verify --journal <副本>`
- 期望:
  - 退出 1;分歧 seq = `z`;实际侧 kind = `OrderDecision`,期望侧为 journal 中 seq `z` 处的那条记录(原 `z + 1`)
- 实现:`apps/replayer/tests/verify.rs::rpl_019_missing_output_in_journal`

### RPL-020 journal 中风控参数被改动后报出第一条受影响的决策

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:样本 T1 的副本;引导块中的 `RiskParams` ConfigBlob 的 `max_order_notional` 改为 T1 中最大单笔名义的一半(重算 blob 的 sha256 与记录校验);T1 中第一条名义超过新上限的 `OrderDecision` 的 seq 记为 `w`
- 步骤:
  1. `replayer verify --journal <副本> --report $OUT/r.json`
- 期望:
  - 退出 1;`first_divergent_seq = w`;期望侧 kind = `OrderDecision`,实际侧为风控拒绝产生的输出(hft-risk 的拒绝 `Note`)
  - `w` 之前的全部 Output 一致
- 实现:`apps/replayer/tests/verify.rs::rpl_020_risk_param_change_first_affected`

### RPL-021 翻转一个 Bbo 字节后在其后第一条决策处报分歧

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:样本 T1 的副本;seq `b` 的 `Bbo` 的卖价最低位翻转(该变化使测试策略在同一事件内改变下单价)
- 步骤:
  1. `replayer verify --journal <副本>`
- 期望:
  - 退出 1;分歧 seq = `b` 之后第一条 `OrderDecision` 的 seq;字段级 diff 只含价格字段
- 实现:`apps/replayer/tests/verify.rs::rpl_021_bbo_byte_flip`

### RPL-022 测量字段与 Output 记录头 tsc 不参与比较

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:样本 T1 的副本;每行只做该行的改动
- 实现:`apps/replayer/tests/verify.rs::rpl_022_measure_fields_masked`

| 行 | 改动 | 期望 |
|---|---|---|
| RPL-022.1 | 每条 `OrderDecision.decided_tsc` 加 12 345 | 退出 0 |
| RPL-022.2 | 关键帧 `K1` 头部的 `snap_tsc`、`snap_utc_ns` 各加 1 | 退出 0 |
| RPL-022.3 | 一条 `ExecDecision` 记录头的 `tsc` 加 1 000 | 退出 0(Output 记录头 `tsc` 取 journal 值) |
| RPL-022.4 | 一条 `OrderDecision` 的 `qty` 加 1 lot | 退出 1;分歧在该记录 |

### RPL-023 Meta 记录的内容改动不影响比对结果与状态

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:样本 T1 的副本;每行只改该行的 Meta 记录内容(长度不变,重算校验)
- 实现:`apps/replayer/tests/verify.rs::rpl_023_meta_not_compared`

| 行 | 改动的 Meta 记录 | 期望 |
|---|---|---|
| RPL-023.1 | 全部 `LatencySample` 的 `proc_tsc` 加 7 | 退出 0;各快照位置的状态哈希与改动前相同 |
| RPL-023.2 | 一条 `ClockStatus` 的 `calib_err_ns` 改为 999 | 退出 0 |
| RPL-023.3 | 一条 `Note{Measure}` 的 `v[0]` 改为 0 | 退出 0 |
| RPL-023.4 | 一条 `HostProfile` 的主机类字节改动 | 退出 0 |
| RPL-023.5 | 一条经 hk 环到达的 `Note{source = Advisor, code = AdvisorStats}` 的 `v[1]` 加 1 | 退出 0 |

### RPL-024 Recovery 区间按 journal 值接受、差异只列入 recovery_diffs

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:真实 sequencer 与 JournalWriter,最近快照后投递账户配置、对账、连接、时钟和自检输入,断言启动闸门已放行,再投递 SmokeOrder;BBO 连接活性使用非零时间戳。写者前注入一条 OrderDecision.qty 差异,由实际写者生成 CRC 与 seal。未 Drain 的旧运行在完整提交边界交接,正式恢复路径读取已发布的原始文件并生成一条 RecoverMismatch 注记;随后正常处理输入和两个 SnapshotNow
- 步骤:
  1. `replayer verify --journal <合成> --report $OUT/r.json`
- 期望:
  - 退出 0;`r.json.verify = "pass"`
  - 分歧明细 `recovery_diffs` 恰好一项,seq = `r`,字段含 qty,标准输出与侧文件一致;两次运行均完成比较,第二次快照至少 3 个且无分歧,来源字节不变
  - to_seq = r 时报告该项,from_seq = r + 1 时不报告;仅新段缺旧证据退出 3;篡改新段首条命令仍退出 1,保留旧恢复差异
- 实现:`apps/replayer/tests/recovery.rs::rpl_024_recovery_range_accepted`

### RPL-025 RecoverFromSnapshot 引导块处重新起步,前后两段各自一致

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal:运行 20 min → 崩溃 → 以 `--recover-from-snapshot` 从 10 min 处的快照起步(新段以 `RecoverFromSnapshot{snapshot_seq, skipped_from, skipped_to, …}` 引导块开头)→ 再运行 20 min;被跳过的尾部 [`skipped_from`, `skipped_to`] 留在旧段
- 步骤:
  1. `replayer verify --journal <合成>`
- 期望:
  - 退出 0;前一段从 `B1` 起比对到旧段末尾,后一段从新引导块的快照重新起步
  - 后一段的状态起点是新引导块中的快照,不是旧段末尾的重放状态(在旧段尾部篡改一条 `Bbo` 后,后一段的比对结果不变)
  - 按 10 秒间隔投递行情与校准,以 journal 的 engine_ns 验证完整的 20/10/20 min 边界;新运行重置 TSC 并使用两倍频率。UTC 从 00:30 起,新运行保留 01:00 的真实整点滚段,三段文件只计两次运行;测试从真实段头与完整 End 生成 seal,并由正式 CLI 校验。Boot 注记恰一条且跳过区间匹配物理提交边界;旧段末条 BBO 加 1 tick 后按原记录 CRC 策略更新校验并重算 seal,keep-going 必须发现旧运行分歧,新运行整组计数和只保留其两段文件时的独立验证与未篡改时一致;全部来源文件与 seal 字节不变
- 实现:`apps/replayer/tests/verify.rs::rpl_025_restart_at_recover_from_snapshot`

### RPL-026 版本变化标记的引导块处重新起步

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal:Drain 停机 → 以另一版本串冷启动(新段引导块带版本变化标记)→ 运行 10 min;两段均由当前代码产生
- 步骤:
  1. `replayer verify --journal <合成>`
- 期望:
  - 退出 0;在版本变化引导块处从其快照重新起步;两段各自一致
  - 旧运行 20 min 后真实 Drain,尾快照用途为 Drain;HostProfile.build_git 确实变化,新运行完整 10 min 且跨过 01:00 整点滚段。三段文件只计两次运行,seal 由测试从真实段头与完整 End 生成并交正式 CLI 校验;两次运行及仅新运行两段文件的独立验证均有非零输入、输出和多个快照,全部来源文件与 seal 字节不变
- 实现:`apps/replayer/tests/verify.rs::rpl_026_restart_at_version_change`

### RPL-027 分歧明细字段齐全,输入摘要以 1 000 条为上限

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:合成 journal:从快照起步后第 n 条 Input 处篡改一条使决策改变的 `Bbo`(n 按行);`--report $OUT/r.json`
- 实现:`apps/replayer/tests/verify.rs::rpl_027_divergence_details`

| 行 | 分歧前已处理的 Input 条数 | 期望 |
|---|---|---|
| RPL-027.1 | 999 | `r.divergence.json` 含 `seq`、`kind`、`expected_hex`、`actual_hex`、`field_diff`、`input_kinds`、`last_snapshot_seq`、`bisect`;`input_kinds` 覆盖 999 条 |
| RPL-027.2 | 1 000 | `input_kinds` 覆盖 1 000 条 |
| RPL-027.3 | 1 001 | `input_kinds` 覆盖分歧前最近的 1 000 条(不含第 1 条) |

### RPL-028 --keep-going 继续比对并汇总全部分歧,退出码仍为 1

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:样本 T1 的副本;`K2` 与 `K6` 之后各有一条 `OrderDecision` 的 `qty` 加 1 lot(两处互不影响状态)
- 步骤:
  1. `replayer verify --journal <副本> --keep-going --report $OUT/r.json`
- 期望:
  - 退出 1;`r.json.first_divergent_seq` = 第一处的 seq;分歧明细列出 2 处,按 seq 升序
- 实现:`apps/replayer/tests/verify.rs::rpl_028_keep_going`

### RPL-029 --to-seq 只比对到给定 seq

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:样本 T1 的副本;seq `q` 处一条 `OrderDecision` 的 `qty` 加 1 lot
- 实现:`apps/replayer/tests/verify.rs::rpl_029_to_seq`

| 行 | `--to-seq` | 期望 |
|---|---|---|
| RPL-029.1 | `q − 1` | 退出 0 |
| RPL-029.2 | `q` | 退出 1;分歧 seq = `q` |
| RPL-029.3 | 大于 T1 最后一条记录的 seq | 退出 1;处理到输入末尾 |

### RPL-030 --instance 与段头不符时退出 2

- 规则:[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M2
- 前置:样本 T1(段头 `instance = "prod"`)
- 实现:`apps/replayer/tests/verify.rs::rpl_030_instance_mismatch`

| 行 | 命令 | 期望 |
|---|---|---|
| RPL-030.1 | `verify --journal $T1 --instance prod` | 退出 0 |
| RPL-030.2 | `verify --journal $T1 --instance sim` | 退出 2;没有比较任何 Output |
| RPL-030.3 | `latency --journal $T1 --instance sim --cal-max-err-ns 1000 --burst-gap-ns 50000 --out $OUT` | 退出 2 |

### RPL-031 不读取环境时间与当前配置:不同的系统时间、时区与配置目录下结果逐字节相同

- 规则:[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M2
- 前置:样本 T1;`/etc/hft/` 下放一份与 T1 参数不同的 `prod.toml`(`max_order_notional` 减半)
- 步骤:
  1. 以 `TZ=UTC`、系统时间不变运行 `replayer verify --journal $T1 --report $OUT/a.json`
  2. 以 `faketime '2030-01-01 00:00:00'`、`TZ=Asia/Tokyo` 运行同一命令写 `$OUT/b.json`
- 期望:
  - 两次都退出 0;`a.json` 与 `b.json` 除 `generated_utc` 外逐字节相同
  - 两次运行都没有打开 `/etc/hft/` 下任何文件
- 实现:`apps/replayer/tests/determinism.rs::rpl_031_no_env_time_or_config`

### RPL-032 x86_64 与 aarch64 对同一 journal 得出相同结果

- 规则:[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M2
- 前置:同一提交的 x86_64 与 aarch64 `replayer`(标签作业的一致性集);样本 T1 与 RPL-021 的篡改副本
- 步骤:
  1. 两种架构各运行 `verify --journal $T1 --advice recompute --report r.json` 与 `verify --journal <篡改副本> --report d.json`
- 期望:
  - 两种架构的 `r.json` 除 `generated_utc` 外逐字节相同(`verify = "pass"`、`recompute = "pass"`)
  - 两种架构的 `d.json` 的 `first_divergent_seq`、`component` 相同,分歧明细逐字节相同
- 实现:`apps/replayer/tests/determinism.rs::rpl_032_cross_arch_same_result`

### RPL-040 --advice recompute 对测试策略 journal 一致

- 规则:[verify](README.md#verify)
- 级别:replay
- 里程碑:M2
- 输入:`fixtures/replay/testkit-1h`(样本 T1;`tsa` 在 T1 中由实例创建起步,无检查点);`replayer verify --journal $T1 --advice recompute --report $OUT/r.json`
- 期望:
  - 退出 0;`r.json`:`verify = "pass"`、`recompute = "pass"`、`coalesced = 0`
  - journal 中每条 `tsa` 的 `Advice` 都有同键的重算 Advice,`AdviceHdr` 与 payload 逐字节相同
- 实现:`apps/replayer/tests/recompute.rs::rpl_040_recompute_consistent`

### RPL-041 Model 中的 HashMap 迭代使重算分歧并报告第一处的键

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:hft-testkit 测试策略 A 的测试变体 `tsa-hashmap`(Model 产出载荷前按 `std::collections::HashMap` 的迭代顺序拼接 group 列表,`RandomState` 每进程不同);用它录 30 min journal(实例创建起步)
- 步骤:
  1. `replayer verify --journal <该 journal> --advice recompute --report $OUT/r.json`
- 期望:
  - 退出 1;`r.json`:`verify = "pass"`、`recompute = "fail"`
  - 分歧明细给出第一处不一致的 `(strategy, account, group, local_kind, seq)`,`seq` 为 journal 中该 Advice 的 seq;其前的 Advice 全部配对且一致
- 实现:`apps/replayer/tests/recompute.rs::rpl_041_hashmap_iteration_detected`

### RPL-042 实盘 outbox 合并掉的 Advice 计入 coalesced、不算分歧

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成运行:hft-testkit 让 advice 环在 10 s 内保持满(sequencer 侧不取),期间 advisor 的 outbox 对同一 `(tsa, acct-l, g1, local_kind = 1)` 覆盖 37 次;录下 journal
- 步骤:
  1. `replayer verify --journal <该 journal> --advice recompute --report $OUT/r.json`
- 期望:
  - 退出 0;`recompute = "pass"`;`coalesced = 37`
  - journal 中的每条 Advice 都配对成功
- 实现:`apps/replayer/tests/recompute.rs::rpl_042_coalesced_not_divergence`

### RPL-043 journal 中的 Advice 在重算中找不到同键者即为分歧

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:样本 T1 的副本;一条 `tsa` 的 `Advice`(seq `a`)的 `AdviceHdr.basis_seq` 改为 `basis_seq − 1`(保持其他字节不变,重算校验)
- 步骤:
  1. `replayer verify --journal <副本> --advice recompute`
- 期望:
  - 退出 1;分歧明细第一处的 seq = `a`,说明"无同键重算 Advice";该条之外的 Advice 配对成功
  - verify 部分照常比较 Output(Advice 是 Input,`basis_seq` 的改动不影响 sequencer 的状态时 `verify = "pass"`)
- 实现:`apps/replayer/tests/recompute.rs::rpl_043_unpaired_journal_advice`

### RPL-044 同键 Advice 的载荷差一个字节即为分歧

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:样本 T1 的副本;一条 `tsa` 的 `Advice`(seq `a`)只改该行指明的字节
- 实现:`apps/replayer/tests/recompute.rs::rpl_044_payload_byte_diff`

| 行 | 改动 | 期望 |
|---|---|---|
| RPL-044.1 | payload 最后一个字节加 1 | 退出 1;第一处分歧 seq = `a` |
| RPL-044.2 | `AdviceHdr.basis_tsc` 加 1 | 退出 1;第一处分歧 seq = `a` |
| RPL-044.3 | 无改动 | 退出 0 |

### RPL-045 配对键的五个分量缺一不可

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:合成 journal:同一策略实例 `tsa` 绑定两个 Live 账户 `acct-l`、`acct-m`,对 group `g1`、`local_kind = 1`、相同 `basis_seq` 各产出一条 Advice(载荷不同);每行在副本上只改 journal 中 `acct-l` 那条的一个键分量
- 实现:`apps/replayer/tests/recompute.rs::rpl_045_pairing_key_components`

| 行 | 改动 | 期望 |
|---|---|---|
| RPL-045.1 | 无 | 退出 0;两条各自与同账户的重算 Advice 配对 |
| RPL-045.2 | `account` 改为 `acct-m` | 退出 1(与 `acct-m` 的重算 Advice 载荷不同) |
| RPL-045.3 | `group` 改为 `g2` | 退出 1(无同键重算) |
| RPL-045.4 | `local_kind` 改为 2 | 退出 1 |
| RPL-045.5 | `strategy` 改为另一实例 `tsb` | 退出 1 |

### RPL-046 从检查点续算:第 1 h 末检查点 + 第 2 h 起点一致

- 规则:[verify](README.md#verify)
- 级别:replay
- 里程碑:M2
- 输入:`fixtures/replay/testkit-3h`(样本 T3);`replayer verify --journal T3 --advice recompute --model-checkpoint ckpt-1h/tsa.ckpt --from-seq H2 --report $OUT/r.json`
- 期望:
  - 退出 0;`recompute = "pass"`;`checkpoint_key` 含一项 `{strategy_instance = "tsa", layout_version = 1, last_seq = L1, …}`
  - 比较过的 Advice 恰为 seq > `H2` 的 `tsa` Advice(`H2 > L1`)
- 实现:`apps/replayer/tests/recompute.rs::rpl_046_checkpoint_resume`

### RPL-047 只比较 seq 大于 max(last_seq, --from-seq) 的 Advice

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:样本 T3 与 `ckpt-1h/tsa.ckpt`(`last_seq = L1`);每行在副本上把指明 seq 的那条 `tsa` Advice 的 payload 末字节加 1
- 实现:`apps/replayer/tests/recompute.rs::rpl_047_compare_after_start`

| 行 | `--from-seq` | 被改 Advice 的 seq | 期望 |
|---|---|---|---|
| RPL-047.1 | 不给 | `L1` 之前最后一条 | 退出 0(照读,不比较) |
| RPL-047.2 | 不给 | `L1` 之后第一条 | 退出 1 |
| RPL-047.3 | `H2` | `L1` 与 `H2` 之间的一条 | 退出 0 |
| RPL-047.4 | `H2` | `H2` 之后第一条 | 退出 1 |

### RPL-048 检查点键不符时退出 2

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:样本 T3;`ckpt-1h/tsa.ckpt` 的副本只改该行的 `CkptHdr` 字段并重算 CRC
- 实现:`apps/replayer/tests/recompute.rs::rpl_048_checkpoint_key_mismatch`

| 行 | 改动 | 期望 |
|---|---|---|
| RPL-048.1 | `kind_id` + 1 | 退出 2;没有比较任何 Advice |
| RPL-048.2 | `layout_version` = 2 | 退出 2 |
| RPL-048.3 | `schema_id` + 1 | 退出 2 |
| RPL-048.4 | `strategy` 改为另一实例号 | 退出 2 |
| RPL-048.5 | `account` 改为另一账户号 | 退出 2 |

### RPL-049 检查点文件损坏时退出 3

- 规则:[verify](README.md#verify)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M2
- 前置:样本 T3;`ckpt-1h/tsa.ckpt` 的副本只做该行改动(不重算 CRC,除非该行另有说明)
- 实现:`apps/replayer/tests/recompute.rs::rpl_049_checkpoint_corrupt_exit_3`

| 行 | 改动 | 期望 |
|---|---|---|
| RPL-049.1 | magic 第 1 字节改为 `0x00` | 退出 3 |
| RPL-049.2 | `hdr_ver` = 2(重算 CRC) | 退出 3 |
| RPL-049.3 | `body_len` 减 1(重算 CRC) | 退出 3 |
| RPL-049.4 | 原始样本节一个字节翻转 | 退出 3(CRC 不符) |
| RPL-049.5 | `PathSecHdr.path_len` = `body_len`(重算 CRC) | 退出 3(`path_len` 超出体长) |

### RPL-050 检查点 last_seq 不在输入覆盖区间内的实例按无起点处理

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:样本 T3 的第 2–3 h 两段作为输入(第一条记录的 seq 记为 `F`,最后一条为 `E`);检查点副本只改 `last_seq` 并重算 CRC;窗口内 `tsa` 没有实例创建点与 `ModelCheckpointDiscarded`
- 实现:`apps/replayer/tests/recompute.rs::rpl_050_checkpoint_outside_input`

| 行 | `last_seq` | 期望 |
|---|---|---|
| RPL-050.1 | `F − 2` | 退出 0;`recompute = "skipped"`、`skipped_reason = "model_no_start"`;分歧明细 `model_no_start = ["tsa"]` |
| RPL-050.2 | `F − 1` | 状态从输入第一个引导块的快照起步,Model 从 `F` 起续算;`model_no_start` 为空;`recompute = "pass"` |
| RPL-050.3 | `E` | 续算范围为空;`model_no_start` 为空;比较的 Advice 数为 0 |
| RPL-050.4 | `E + 1` | `model_no_start = ["tsa"]` |

### RPL-051 检查点目录中缺某实例文件时该实例按未给检查点处理

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal(3 h):两个带 Model 的实例 `tsa`、`tsb`,`tsb` 在第 2 h 中途由 `SetParams` 创建;目录 `$CK` 只含 `tsa.ckpt`(1 h 末写出)
- 步骤:
  1. `replayer verify --journal <合成> --advice recompute --model-checkpoint $CK --from-seq H2 --report $OUT/r.json`
- 期望:
  - 退出 0;`tsa` 从检查点续算、`tsb` 从其创建处起算,两者全部配对一致;`recompute = "pass"`;`model_no_start` 为空
- 实现:`apps/replayer/tests/recompute.rs::rpl_051_missing_file_in_dir`

### RPL-052 S3 前缀形式的检查点与本地文件结果相同

- 规则:[verify](README.md#verify)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:integ
- 里程碑:M5
- 前置:本地 S3 模拟服务;把 `ckpt-1h/tsa.ckpt` 放在 `s3://b/p/h1/prod/private/advisor/tsa/2026-10-01/tsa.ckpt`(journal-archiver 的键布局,SSE-KMS 模拟)
- 步骤:
  1. `verify --journal T3 --advice recompute --model-checkpoint ckpt-1h/ --from-seq H2 --report $OUT/a.json`
  2. `verify --journal T3 --advice recompute --model-checkpoint s3://b/p/h1/prod/private/advisor/tsa/2026-10-01/ --from-seq H2 --report $OUT/b.json`
- 期望:
  - 两次都退出 0;`a.json` 与 `b.json` 除 `generated_utc` 外逐字节相同
- 实现:`apps/replayer/tests/recompute.rs::rpl_052_s3_checkpoint_same_as_local`

### RPL-053 未给检查点时从窗口内的实例创建点起算

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal(1 h):前 20 min 只有行情,第 20 min 一条 `SetParams` 创建 `tsa`
- 步骤:
  1. `replayer verify --journal <合成> --advice recompute --report $OUT/r.json`
- 期望:
  - 退出 0;`recompute = "pass"`;`tsa` 的第一条重算 Advice 的 `basis_seq` 晚于该 `SetParams` 的 seq;journal 中的 `tsa` Advice 全部配对
- 实现:`apps/replayer/tests/recompute.rs::rpl_053_start_at_instance_creation`

### RPL-054 未给检查点时从 ModelCheckpointDiscarded 之后的第一条记录起算

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal:hft-core 重启时 `tsa.ckpt` 的 CRC 被破坏,advisor 写 Meta `Note{source = Advisor, code = ModelCheckpointDiscarded, v[0] = tsa, v[1] = 2(Corrupt)}`(seq `n`),其后 Model 空状态起步;journal 只含重启后的段
- 步骤:
  1. `replayer verify --journal <合成> --advice recompute --report $OUT/r.json`
- 期望:
  - 退出 0;`recompute = "pass"`;重算从 seq `n + 1` 起;`model_no_start` 为空
- 实现:`apps/replayer/tests/recompute.rs::rpl_054_start_after_checkpoint_discarded`

### RPL-055 既无检查点也无空起步点时报告 model_no_start、退出码不受影响

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:样本 T3 的第 2 段起的输入(`tsa` 创建点不在窗口内,窗口内无 `ModelCheckpointDiscarded`);不给 `--model-checkpoint`
- 步骤:
  1. `replayer verify --journal <输入> --advice recompute --report $OUT/r.json`
- 期望:
  - 退出 0;`verify = "pass"`、`recompute = "skipped"`、`skipped_reason = "model_no_start"`、无 `coalesced`;分歧明细 `model_no_start = ["tsa"]`
- 实现:`apps/replayer/tests/recompute.rs::rpl_055_model_no_start`

### RPL-056 恢复检查点时路径节版本不符:只丢路径节,重建点之前的 Advice 不比较

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:hft-testkit 测试策略 A 的变体 `tsa-p2`(`PATH_VERSION = 2`,`path_settle_ms = 120 000`,其余同 A);样本 T3 的 `ckpt-1h/tsa.ckpt`(路径节版本 1);T3 由 `PATH_VERSION = 1` 的代码录下,`tsa-p2` 的输出在路径状态收敛前与录下的不同、收敛后相同;`L1` 所指记录的引擎时间记为 `e0`;T3 中 `L1` 之后第一条引擎时间 ≥ `e0 + 120 000 × 10^6` 的记录的 seq 记为 `R`
- 步骤:
  1. 以 `tsa-p2` 构建的 replayer 运行 `verify --journal T3 --advice recompute --model-checkpoint ckpt-1h/ --report $OUT/r.json`(不给 `--from-seq`,比较范围从 `L1 + 1` 起)
- 期望:
  - 退出 0;`recompute = "pass"`;`recompute_from_seq = R`
  - `basis_seq < R` 的 `tsa` Advice 没有比较;`basis_seq ≥ R` 的全部逐字节相同
  - 原始样本节照常恢复(`Model::restore` 调用 1 次,`restore_path` 调用 0 次)
- 实现:`apps/replayer/tests/recompute.rs::rpl_056_path_section_dropped`

### RPL-057 重建点按引擎时间的边界取值

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:合成 journal:检查点 `last_seq = L` 所指记录的引擎时间 `e0`;`path_settle_ms = 1 000`;`L` 之后依次有记录 r1、r2、r3,引擎时间分别为 `e0 + 999 999 999`、`e0 + 1 000 000 000`、`e0 + 1 000 000 001`(ns);每条 rk 之后紧跟一条 `basis_seq = rk` 的 Advice,其载荷与连续运行不同的只有 `basis_seq < recompute_from_seq` 的那些(由测试 Model 构造)
- 实现:`apps/replayer/tests/recompute.rs::rpl_057_rebuild_point_boundary`

| 行 | 观察项 | 期望 |
|---|---|---|
| RPL-057.1 | `recompute_from_seq` | = r2 的 seq |
| RPL-057.2 | `basis_seq = r1` 的 Advice | 不比较 |
| RPL-057.3 | `basis_seq = r2` 的 Advice | 比较且一致 |
| RPL-057.4 | `basis_seq = r3` 的 Advice | 比较且一致 |

### RPL-058 窗口内的 ModelPathReset 注记按其 v[1]、v[2] 计算重建点

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal:实盘重启时路径节被丢弃,advisor 写 `Note{source = Advisor, code = ModelPathReset, strategy = tsa, v[0] = 1, v[1] = L, v[2] = 60 000}`;journal 中重启后的 Advice 在 60 s 引擎时间内与连续运行不同;回放时给出 `last_seq = L`、路径节版本与当前一致的检查点
- 步骤:
  1. `verify --journal <合成> --advice recompute --model-checkpoint <检查点> --report $OUT/r.json`
- 期望:
  - 退出 0;`recompute_from_seq` = `L` 之后第一条引擎时间 ≥ (`L` 所指记录的引擎时间 + 60 000 × 10^6)的记录的 seq;其前的 Advice 不比较
- 实现:`apps/replayer/tests/recompute.rs::rpl_058_path_reset_note`

### RPL-059 一日内多次路径节丢弃时 recompute_from_seq 取最后一次

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal(一个 UTC 日):两次重启各写一条 `Note{ModelPathReset}`,重建点分别为 `R1 < R2`
- 步骤:
  1. `verify --journal <合成> --advice recompute --model-checkpoint <检查点> --report $OUT/r.json`
- 期望:
  - 退出 0;`r.json.recompute_from_seq = R2`;两次重建点之前的 Advice 都不比较
- 实现:`apps/replayer/tests/recompute.rs::rpl_059_last_rebuild_point`

### RPL-060 重建点按引擎时间计,停机时长不计入

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal:路径节丢弃的检查点 `last_seq = L`(引擎时间 `e0`);`path_settle_ms = 120 000`;`L` 之后 30 s 引擎时间处 Drain 停机,停机 10 min 后重启(`EngineClock` 续接,引擎时间不含停机),重启后再运行 5 min
- 步骤:
  1. `verify --journal <合成> --advice recompute --model-checkpoint <检查点> --report $OUT/r.json`
- 期望:
  - 退出 0;`recompute_from_seq` = 重启后引擎时间首次 ≥ `e0 + 120 × 10^9` 的记录的 seq(重启后约 90 s 处);不是 UTC 时刻 `L` + 120 s 之后的第一条记录(那会落在停机之前)
- 实现:`apps/replayer/tests/recompute.rs::rpl_060_engine_time_excludes_downtime`

### RPL-061 recompute 的状态起点取 --from-seq 与 last_seq + 1 中较小者之前的快照

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:样本 T3 与 `ckpt-1h/tsa.ckpt`(`last_seq = L1`,`L1` 之前最近的快照为 `Kp`);`--from-seq H2`(`H2 > L1`);在副本中 `Kp` 与 `L1` 之间一条影响 `tsa` Model 视图的 `Bbo` 改价 1 tick
- 步骤:
  1. `verify --journal <副本> --advice recompute --model-checkpoint ckpt-1h/ --from-seq H2`
- 期望:
  - 退出 1:状态从 `Kp` 起重放,被篡改的 `Bbo` 进入重放(Output 分歧落在该 `Bbo` 之后第一条受影响的记录,其 seq < `H2`)
  - 对照:不改 `Bbo` 时退出 0,Model 在 `L1 + 1` 起看到的视图与实盘相同(全部 Advice 一致)
- 实现:`apps/replayer/tests/recompute.rs::rpl_061_state_start_covers_checkpoint`

### RPL-062 InjectWarmup 在重算中照常投递

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:合成 journal:`tsa` 创建后收到 3 块 `ControlCmd{InjectWarmup{instance = tsa, local_kind = 9, chunk_no = 0..2, last = 2 时为真}}`(均 Applied),此后的 Advice 依赖预热样本
- 步骤:
  1. `verify --journal <合成> --advice recompute`
- 期望:
  - 退出 0;重算 Model 收到 3 次 `EventRef::Warmup`(`chunk_no` 0、1、2);全部 Advice 一致
- 实现:`apps/replayer/tests/recompute.rs::rpl_062_warmup_delivered`

### RPL-063 Model 分歧不影响 verify 的 Output 比对结论

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:RPL-041 的 journal
- 步骤:
  1. `verify --journal <该 journal> --advice recompute --report $OUT/r.json`
- 期望:
  - 退出 1;`verify = "pass"`、无 `first_divergent_seq`;`recompute = "fail"`
- 实现:`apps/replayer/tests/recompute.rs::rpl_063_recompute_fail_verify_pass`

### RPL-070 每日模式按日期升序补跑全部缺少报告的已封存 UTC 日

- 规则:[verify](README.md#verify)、[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:合成冷副本根 `$R/sim/seq/`:`20261001`、`20261002`、`20261003` 三个日目录,段全部有 `.seal`;检查点根 `$CK/sim/` 有这三日的目录;报告目录 `$RD` 中已有 `2026-10-02.json`;运行时刻 UTC 2026-10-04 01:30(`faketime`)
- 步骤:
  1. `replayer verify --journal $R/sim/seq --instance sim --advice recompute --model-checkpoint $CK/sim --report-dir $RD`
- 期望:
  - 退出 0;新写出 `2026-10-01.json` 与 `2026-10-03.json`,文件修改时间前者早于后者;`2026-10-02.json` 未被改写(内容与 mtime 不变)
  - 每份报告的 `date` 是被验证的 UTC 日(`2026-10-01`、`2026-10-03`),不是运行日
- 实现:`apps/replayer/tests/daily.rs::rpl_070_backfill_missing_days`

### RPL-071 每日模式的选日条件

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M4
- 前置:同 RPL-070 的布局;运行时刻 UTC 2026-10-04 01:30;报告目录为空;每行只改该行条件
- 实现:`apps/replayer/tests/daily.rs::rpl_071_day_selection`

| 行 | 条件 | 期望 |
|---|---|---|
| RPL-071.1 | `20261004` 目录存在且全部段已封存 | 不处理 10-04(运行开始时的 UTC 日);其余三日各写一份报告 |
| RPL-071.2 | `20261003` 的最后一段缺 `.seal` | 不处理 10-03、不写其报告;10-01、10-02 照常 |
| RPL-071.3 | `$RD/2026-10-01.json.tmp` 残留(上次中断) | 10-01 照常处理,写出 `2026-10-01.json` 并删除 `.tmp` |
| RPL-071.4 | 三日的报告都已存在 | 退出 0;不读取任何段文件 |

### RPL-072 每日报告的字段填法

- 规则:[verify](README.md#verify)
- 级别:golden
- 里程碑:M4
- 输入:`fixtures/replay/daily/`:冷副本根(`sim` 实例一个 UTC 日 `20261001`,含两个带 Model 的实例 `tsa`、`tsb`,`tsa` 当日有一次路径节丢弃,重建点 `R`)与检查点根(`2026-10-01/tsa.ckpt`、`tsb.ckpt`);运行时刻 2026-10-02 01:30(`faketime`);每日模式命令同 RPL-070
- 期望文件:`fixtures/replay/daily/RPL-072/2026-10-01.json`(除 `generated_utc` 外逐字段相等:`instance = "sim"`、`date = "2026-10-01"`、`release` = `replayer --version` 的版本串、`verify = "pass"`、`recompute = "pass"`、`coalesced` = 两实例之和、`checkpoint_key` 两项、`recompute_from_seq = R`;无 `first_divergent_seq`、`component`、`skipped_reason`)
- 实现:`apps/replayer/tests/daily.rs::rpl_072_report_fields_golden`

### RPL-073 当日没有检查点目录时 recompute 为 skipped

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M4
- 前置:同 RPL-072 的冷副本,检查点根下没有 `2026-10-01/` 目录;窗口内 `tsa`、`tsb` 都没有空起步点
- 步骤:
  1. 每日模式运行
- 期望:
  - 退出 0;`2026-10-01.json`:`verify = "pass"`、`recompute = "skipped"`、`skipped_reason = "model_no_start"`、无 `coalesced`、`checkpoint_key` 为空列表
- 实现:`apps/replayer/tests/daily.rs::rpl_073_no_checkpoint_dir_skipped`

### RPL-074 recompute 结论的优先级:fail 高于 skipped 高于 pass

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M4
- 前置:同 RPL-072 的冷副本;每行给出检查点目录内容与 Model 行为
- 实现:`apps/replayer/tests/daily.rs::rpl_074_recompute_verdict_priority`

| 行 | 条件 | 期望 `recompute` / `skipped_reason` |
|---|---|---|
| RPL-074.1 | `tsa`、`tsb` 都有检查点且一致 | `"pass"` / 无 |
| RPL-074.2 | 只有 `tsa.ckpt`,`tsb` 无起点 | `"skipped"` / `"model_no_start"`;`tsa` 照常重算 |
| RPL-074.3 | 只有 `tsa.ckpt` 且 `tsa` 重算分歧 | `"fail"` / 无 |
| RPL-074.4 | 当日 journal 中没有任何带 Model 的实例(只有 `NoModel` 策略) | `"pass"` / 无;`coalesced = 0` |

### RPL-075 某日输入不完整或不兼容时写失败报告并继续下一日

- 规则:[verify](README.md#verify)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M4
- 前置:三个已封存的日 10-01、10-02、10-03,只在 10-02 做该行改动;运行时刻 10-04 01:30
- 实现:`apps/replayer/tests/daily.rs::rpl_075_bad_day_continues`

| 行 | 10-02 的改动 | 期望 |
|---|---|---|
| RPL-075.1 | 删去中间一段(段链断) | 三份报告都写出;`2026-10-02.json`:`verify = "fail"`、`skipped_reason = "input_incomplete"`;进程退出 3 |
| RPL-075.2 | 段中引用一个未注册的策略 `kind_id` | `2026-10-02.json`:`verify = "fail"`、`skipped_reason = "incompatible"`;10-03 照常;进程退出 4 |
| RPL-075.3 | 10-02 的检查点 `layout_version` 与实例不符 | 三份报告都写出;`2026-10-02.json`:`verify = "pass"`、`recompute = "skipped"`、`skipped_reason = "model_no_start"`;10-03 照常;进程退出 0 |
| RPL-075.4 | 10-02 的一条 `OrderDecision` 被篡改 | 三份报告都写出;10-02 `verify = "fail"`、带 `first_divergent_seq`;进程退出 1 |

### RPL-076 报告先写临时文件再改名,中断不留半截报告

- 规则:[verify](README.md#verify)
- 级别:proc
- 里程碑:M4
- 前置:同 RPL-070 的布局,报告目录为空;测试钩子在写第一份报告的 `.tmp` 之后、改名之前让进程 `SIGKILL`
- 步骤:
  1. 每日模式运行(被杀)
  2. 再次每日模式运行
- 期望:
  - 步骤 1 后报告目录中没有 `2026-10-01.json`(只有 `.tmp`)
  - 步骤 2 后三日的 `.json` 都存在且可解析,没有残留 `.tmp`
- 实现:`apps/replayer/tests/daily.rs::rpl_076_atomic_report_write`

### RPL-077 每日模式与 --report、--from-seq、--to-seq、--advice journal 互斥

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M4
- 前置:同 RPL-070 的布局
- 实现:`apps/replayer/tests/daily.rs::rpl_077_daily_exclusive_options`

| 行 | 另给的选项 | 期望 |
|---|---|---|
| RPL-077.1 | `--report x.json` | 退出 2;不写任何报告 |
| RPL-077.2 | `--from-seq 1` | 退出 2 |
| RPL-077.3 | `--to-seq 9` | 退出 2 |
| RPL-077.4 | `--advice journal` | 退出 2 |
| RPL-077.5 | 无 | 退出 0 |

### RPL-078 每日模式只读冷副本与检查点目录

- 规则:[verify](README.md#verify)、[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M4
- 前置:同 RPL-070 的布局;冷副本根与检查点根对运行用户只读(`0550`/`0440`),报告目录可写
- 步骤:
  1. 每日模式运行
- 期望:
  - 退出 0;冷副本与检查点目录下所有文件的内容、大小与 mtime 不变;报告目录只多出三个 `.json`
- 实现:`apps/replayer/tests/daily.rs::rpl_078_read_only_inputs`

### RPL-079 dev2 上每日回放单元为每个实例写出当日报告

- 规则:[使用本组件](README.md#使用本组件)
- 级别:host
- 里程碑:M4
- 环境:dev2,`sim` 实例已连续运行 ≥ 2 个 UTC 日;已安装当前发布的 `hft-replay-verify.service` 与 `.timer`
- 授权:无
- 步骤:
  1. 等 UTC 01:30 定时器触发,或 `systemctl start hft-replay-verify`
  2. 读 `/var/lib/hft/replay-verify/sim/` 下前一 UTC 日的报告
- 通过标准:前一 UTC 日的报告存在,`verify = "pass"`,`recompute ∈ {"pass", "skipped"}`;单元 `Result=success`;运行期间 `hft-journal-archiver` 处于停止状态、结束后恢复 active
- 证据:`ops-log/<date>-RPL-079.md`(报告文件内容与 `systemctl show` 输出)
- 实现:`apps/replayer/tests/host/RPL-079.sh`

### RPL-090 从 SimReset 起、同种子重演:成交逐笔一致,重算 Advice 与录下的逐字节相同

- 规则:[sim](README.md#sim)
- 级别:integ
- 里程碑:M4
- 前置:样本 S1(种子 7,无 `quiet_advance`);起点 `P0`(其后紧接 `SimReset{sim-a}` 与 `tss` 的创建)
- 步骤:
  1. `replayer sim --journal S1 --from-seq P0 --check-fills --out $OUT`
- 期望:
  - 退出 0;`sim.json`:`seed = 7`、`accounts = ["sim-a"]`、`approximate = false`、`model_cold_start = true`(未给检查点,空 Model 起步;原运行中 `tss` 同样在 `P0` 之后创建、空状态起步)、`advice_inconsistent = false`、`check_fills = "match"`
  - `fills.parquet` 的每笔(价、量、`ex_ts_ns`)与 journal 中 `sim-a` 的成交逐笔相同,条数相同
  - 重算的 `tss` Advice 与 journal 中录下的逐字节相同;没有 `divergence.json`
- 实现:`apps/replayer/tests/sim.rs::rpl_090_same_seed_reproduces`

### RPL-091 换种子:成交改变、不变量成立,重算的 Advice 随之改变且录下的 Advice 未被投递

- 规则:[sim](README.md#sim)
- 级别:integ
- 里程碑:M4
- 前置:样本 S1
- 步骤:
  1. `replayer sim --journal S1 --seed 8 --out $OUT`
- 期望:
  - 退出 0;`sim.json.seed = 8`;`fills.parquet` 至少一笔的 `ex_ts_ns` 或成交价与 journal 不同
  - 每条模拟成交之后 hft-testkit 不变量 INV 检查器(仓位守恒、余额非负)全部成立
  - sequencer 收到的 `tss` Advice 全部来自重算(投递计数:重算 = 全部,journal = 0);重算 Advice 序列与 RPL-090 的不同
- 实现:`apps/replayer/tests/sim.rs::rpl_091_other_seed_changes_fills`

### RPL-092 --check-fills 发现成交不一致时退出 1

- 规则:[sim](README.md#sim)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:unit
- 里程碑:M4
- 前置:样本 S1
- 步骤:
  1. `replayer sim --journal S1 --seed 8 --check-fills --out $OUT`
- 期望:
  - 退出 1;`sim.json.check_fills = "mismatch"`;`divergence.json` 给出第一笔不一致成交的 journal seq 与两侧(价、量、`ex_ts_ns`)
- 实现:`apps/replayer/tests/sim.rs::rpl_092_check_fills_mismatch_exit_1`

### RPL-093 被重现账户的回报不从 journal 读,其余账户照读

- 规则:[sim](README.md#sim)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M4
- 前置:样本 S1b(Sim 账户 `sim-a`、`sim-b`);副本中把 `sim-a` 的一条成交回报 `OrderEvent` 的成交价改 1 tick
- 步骤:
  1. `replayer sim --journal <副本> --accounts sim-a --out $OUT/a`
  2. `replayer sim --journal S1b --accounts sim-a --out $OUT/b`
  3. `replayer sim --journal <副本> --accounts sim-b --out $OUT/c`
- 期望:
  - 步骤 1 与 2 的 `decisions.parquet`、`fills.parquet` 逐字节相同(`sim-a` 的回报由 `SimExchange` 生成,journal 中被改的回报未进入输入流)
  - 步骤 3 中 `sim-a` 的回报照读 journal:`decisions.parquet` 中 `sim-a` 的决策在被改回报之后与步骤 2 不同
- 实现:`apps/replayer/tests/sim.rs::rpl_093_reproduced_account_reports_dropped`

### RPL-094 重演的 [sim] 全表与种子取自 journal,不读当前配置文件

- 规则:[sim](README.md#sim)、[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M4
- 前置:样本 S1;`/etc/hft/sim.toml` 放一份 `[sim]` 不同(延迟模型均值 ×2、种子 99)的配置
- 步骤:
  1. `replayer sim --journal S1 --check-fills --out $OUT`(`strace -e trace=openat`)
- 期望:
  - 退出 0;`check_fills = "match"`;`sim.json.seed = 7`;没有打开 `/etc/hft/` 下任何文件
- 实现:`apps/replayer/tests/sim.rs::rpl_094_sim_table_from_journal`

### RPL-095 --as-sim 把测试网账户当作 Sim 重演:决策链完整、无收益汇总

- 规则:[sim](README.md#sim)
- 级别:integ
- 里程碑:M6
- 前置:`fixtures/replay/testnet-1h`:测试网 `prod` 实例 1 h journal(Live 账户 `tn-a`,无 Sim 账户);`tn-a.toml`:`SimAccountCfg`(初始余额取自起点快照的同额,`fee_tier` 存在,延迟模型与保证金模型各 1 张,`latency_model = 0`、`margin_model = 0`)
- 步骤:
  1. `replayer sim --journal <该 journal> --as-sim tn-a=tn-a.toml --accounts tn-a --out $OUT`
- 期望:
  - 退出 0;`decisions.parquet` 含窗口内全部 `OrderDecision`、`ExecDecision` 与风控结果,按 seq 连续;`fills.parquet` 中 `tn-a` 的成交全部由 `SimExchange` 产生
  - `$OUT` 中只有 `decisions.parquet`、`fills.parquet`、`sim.json` 与(若有分歧)`divergence.json`;没有含 `pnl`、`profit`、`return` 字段或文件名的输出
- 实现:`apps/replayer/tests/sim.rs::rpl_095_as_sim_testnet_account`

### RPL-096 --as-sim 的账户文件不合法时退出 2

- 规则:[sim](README.md#sim)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M4
- 前置:RPL-095 的 journal(其实例没有 Sim 账户,journal 中没有 `[sim]` 模型表);每行给出 `tn-a.toml` 的改动
- 实现:`apps/replayer/tests/sim.rs::rpl_096_as_sim_bad_file`

| 行 | 账户文件 | 期望 |
|---|---|---|
| RPL-096.1 | 不带模型表 | 退出 2(journal 中无模型表,文件须自带) |
| RPL-096.2 | 带 1 张延迟模型表,`latency_model = 1` | 退出 2(下标越界,`SimCfgErr::LatencyModelIndex`) |
| RPL-096.3 | `fee_tier` 在 refdata `Fees` 中不存在 | 退出 2(`SimCfgErr::FeeTier`) |
| RPL-096.4 | 账户名 `tn-x` 不在 journal 的 refdata 中 | 退出 2 |
| RPL-096.5 | RPL-095 的合法文件 | 退出 0 |

### RPL-097 sim 的起点必须是快照,--accounts 不得含未经 --as-sim 的 Live 账户

- 规则:[sim](README.md#sim)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M4
- 前置:样本 S1b 的副本,另含一个 Live 账户 `acct-l`(实例 `prod` 的测试副本,只用于本用例);快照 seq `K1`
- 实现:`apps/replayer/tests/sim.rs::rpl_097_sim_start_and_accounts`

| 行 | 选项 | 期望 |
|---|---|---|
| RPL-097.1 | `--from-seq K1` | 退出 0 |
| RPL-097.2 | `--from-seq K1 + 1` | 退出 2 |
| RPL-097.3 | `--accounts sim-a,acct-l` | 退出 2 |
| RPL-097.4 | `--accounts acct-l --as-sim acct-l=f.toml`(合法文件) | 退出 0 |

### RPL-098 起点不是账户创建或 SimReset 时报告标 approximate

- 规则:[sim](README.md#sim)
- 级别:table
- 里程碑:M4
- 前置:样本 S1(起点 `P0`、`P6` 见约定)
- 实现:`apps/replayer/tests/sim.rs::rpl_098_approximate_flag`

| 行 | `--from-seq` | 期望 |
|---|---|---|
| RPL-098.1 | `P0` | `sim.json.approximate = false` |
| RPL-098.2 | `P6` | `sim.json.approximate = true`;模拟真值由 `P6` 中的 ledger 视图经 `SimExchange::seed_account_from` 初始化(`seed_account_from` 调用 1 次,已吃流动性记忆为空) |

### RPL-099 Advice 按 basis_seq 处的记录头 tsc 加中位滞后插回输入流

- 规则:[sim](README.md#sim)
- 级别:table
- 里程碑:M4
- 前置:合成 journal:`tss` 的 Advice 的滞后(`记录头 tsc − basis_tsc`,单位 tsc 刻度)按行给出;重演中一条重算 Advice 的 `basis_seq` 所指记录的记录头 tsc 为 1 000 000
- 实现:`apps/replayer/tests/sim.rs::rpl_099_advice_reinsert_median_lag`

| 行 | journal 中 `tss` 的 Advice 滞后 | 期望:该条重算 Advice 的插回时刻 |
|---|---|---|
| RPL-099.1 | 10、20、30 | 1 000 020 |
| RPL-099.2 | 10、20、30、40 | 1 000 020(偶数条取较小的中间值) |
| RPL-099.3 | 无 | 1 000 000(滞后取 0) |
| RPL-099.4 | 5 | 1 000 005 |

### RPL-100 插回的 Advice 与模拟回报按 (tsc, gen_seq) 归并进输入流

- 规则:[sim](README.md#sim)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M4
- 前置:合成 journal:三条录下的行情记录头 tsc 为 100、200、300;`SimExchange` 交付两条回报 `due_tsc = 200`(`gen_seq` 5、6);一条插回的重算 Advice 时刻为 200(`gen_seq` 4)
- 步骤:
  1. 运行 `SimReplayDriver`,记录交给 sequencer 的输入顺序
- 期望:
  - 顺序为:行情@100、行情@200、Advice@200(`gen_seq` 4)、回报@200(5)、回报@200(6)、行情@300
  - `SimExchange::advance` 的参数依次为已送入的最后一条记录的 tsc(100、200、200、200、200、300)
- 实现:`apps/replayer/tests/sim.rs::rpl_100_merge_order`

### RPL-101 Model 起点:给出不晚于起点的检查点时恢复并追到起点,否则冷起步并标注

- 规则:[sim](README.md#sim)
- 级别:table
- 里程碑:M4
- 前置:样本 S1 的 3 h 版本(`fixtures/replay/sim-3h`),`--from-seq` = t = 2 h 后第一个快照 `P`;检查点 `c1`(`last_seq < P`)与 `c2`(`last_seq > P`)
- 实现:`apps/replayer/tests/sim.rs::rpl_101_sim_model_start`

| 行 | `--model-checkpoint` | 期望 |
|---|---|---|
| RPL-101.1 | `c1` 所在目录 | `model_cold_start = false`;Model 从 `c1.last_seq + 1` 喂到 `P` 后开始产出插回的 Advice |
| RPL-101.2 | `c2` 所在目录 | `model_cold_start = true`(检查点晚于起点,不使用) |
| RPL-101.3 | 不给 | `model_cold_start = true`;窗口内的 `InjectWarmup` 照常投递 |

### RPL-102 --advice journal 只在 SimReset 起点且成交一致时不标 advice_inconsistent

- 规则:[sim](README.md#sim)
- 级别:table
- 里程碑:M4
- 前置:样本 S1(起点 `P0`、`P6` 见约定)
- 实现:`apps/replayer/tests/sim.rs::rpl_102_advice_journal_flag`

| 行 | 选项 | 期望 |
|---|---|---|
| RPL-102.1 | `--advice journal --from-seq P0 --check-fills` | 退出 0;`advice_inconsistent = false`;`check_fills = "match"`;`MODEL_NEW_COUNT = 0` |
| RPL-102.2 | `--advice journal --from-seq P6` | 退出 0;`advice_inconsistent = true` |
| RPL-102.3 | `--advice journal --from-seq P0`(不给 `--check-fills`) | `advice_inconsistent = true`(一致性未经 `--check-fills` 证实) |

### RPL-103 不被重现账户所绑实例的 Advice 照读 journal

- 规则:[sim](README.md#sim)
- 级别:unit
- 里程碑:M4
- 前置:样本 S1b;`tss` 绑定 `sim-a`,`tsb2` 绑定 `sim-b`(都带 Model)
- 步骤:
  1. `replayer sim --journal S1b --accounts sim-a --out $OUT`
- 期望:
  - `tss` 的 Advice 全部来自重算;`tsb2` 的 Advice 全部来自 journal(投递计数:journal = journal 中 `tsb2` Advice 条数,重算 = 0);`MODEL_NEW_COUNT = 1`
- 实现:`apps/replayer/tests/sim.rs::rpl_103_other_instances_read_journal_advice`

### RPL-104 sim 输出目录只含规定的文件,且不含收益汇总

- 规则:[sim](README.md#sim)
- 级别:table
- 里程碑:M4
- 前置:样本 S1
- 实现:`apps/replayer/tests/sim.rs::rpl_104_output_files`

| 行 | 运行 | 期望 `$OUT` 的文件集合 |
|---|---|---|
| RPL-104.1 | `sim --journal S1 --check-fills --out $OUT` | {`decisions.parquet`、`fills.parquet`、`sim.json`};任一文件中没有名为 `pnl`、`profit`、`return`、`sharpe` 的字段 |
| RPL-104.2 | `sim --journal S1 --seed 8 --out $OUT` | 上述三个 + `divergence.json`(第一处与 journal 中 `sim-a` 实际决策不同的 seq) |
| RPL-104.3 | `sim --journal S1 --emit-journal --out $OUT` | 上述三个 + `$OUT/journal/` 下的段文件 |

### RPL-105 --emit-journal 写出的回放专用 journal 可再做 verify

- 规则:[sim](README.md#sim)、[依赖](README.md#依赖)
- 级别:integ
- 里程碑:M4
- 前置:样本 S1
- 步骤:
  1. `replayer sim --journal S1 --emit-journal --out $OUT`
  2. `replayer verify --journal $OUT/journal --instance sim`
- 期望:
  - 步骤 1 退出 0;段由 `JournalWriter::open_offline` 写在 `$OUT/journal/` 下的普通文件系统
  - 步骤 2 退出 0
- 实现:`apps/replayer/tests/sim.rs::rpl_105_emit_journal_verifiable`

### RPL-106 --emit-journal 拒绝写 hugetlbfs、tmpfs 与生产 journal 根

- 规则:[依赖](README.md#依赖)
- 级别:table
- 里程碑:M4
- 前置:样本 S1;测试主机挂载 `/dev/hugepages`(hugetlbfs)与 `/dev/shm`(tmpfs)
- 实现:`apps/replayer/tests/sim.rs::rpl_106_emit_journal_rejects_hot_roots`

| 行 | `--out` | 期望 |
|---|---|---|
| RPL-106.1 | `/dev/shm/rpl-test` | 退出 2;该目录下没有段文件 |
| RPL-106.2 | `/dev/hugepages/rpl-test` | 退出 2 |
| RPL-106.3 | 生产 journal 冷副本根(测试中以 hft-core `paths.journal_cold` 的取值建出的同名目录代替)下的子目录 | 退出 2 |
| RPL-106.4 | 临时目录(ext4/xfs) | 退出 0 |

### RPL-107 同一输入与种子两次 sim 运行输出逐字节相同

- 规则:[实现说明](README.md#实现说明)、[sim](README.md#sim)
- 级别:unit
- 里程碑:M4
- 前置:样本 S1;`--seed 8`
- 步骤:
  1. 运行 `sim --journal S1 --seed 8 --out $OUT/a`
  2. 运行同一命令写 `$OUT/b`
- 期望:
  - `a` 与 `b` 下每个文件逐字节相同
- 实现:`apps/replayer/tests/sim.rs::rpl_107_sim_deterministic`

### RPL-108 代码可为修复后的新版本,参数仍取 journal 中的记录

- 规则:[sim](README.md#sim)
- 级别:unit
- 里程碑:M4
- 前置:样本 S1;hft-testkit 测试策略 A 的变体 `tss-fix`(下单数量取整方向与 A 不同,模拟一次修复);journal 中 `tss` 的 `StrategyParams` 的 `max_pos` 值为 `p`
- 步骤:
  1. 以 `tss-fix` 构建的 replayer 运行 `sim --journal S1 --out $OUT`
- 期望:
  - 退出 0;`decisions.parquet` 中 `tss` 的决策量按 `tss-fix` 的取整方向产生;所用参数的 `max_pos = p`(与 journal 一致)
- 实现:`apps/replayer/tests/sim.rs::rpl_108_new_code_journal_params`

### RPL-120 --stress gate 对满仓起点运行两个门槛用例

- 规则:[sim](README.md#sim)
- 级别:integ
- 里程碑:M4
- 前置:`fixtures/replay/stress-start`:测试策略在 Sim 账户 `sim-a` 上永续名义 ≥ 允许上限 90% 时的快照 `Q`;refdata 有分档维持保证金率;`MarginModel.mmr = RefdataTiers`;`RiskParams` 为该样本的最终参数(`liq_dist_force` 等取样本中的值)
- 步骤:
  1. `replayer sim --journal <样本> --from-seq Q --stress gate --advice recompute --accounts sim-a --out $OUT/a`
  2. 同一命令写 `$OUT/b`
- 期望:
  - 两次都退出 0;各写出 `stress-one-sided-30.json` 与 `stress-l3-path.json`,没有其他 `stress-*.json`
  - 每个文件含 `StressVerdict` 全部字段(`case`、`gate = true`、`passed = true`、`liquidations = 0`、`min_liq_dist`、`first_level_tsc`、`deleverage_steps`、`end_exposure_frac`、`escalated`)
  - `a` 与 `b` 的两个文件逐字节相同
- 实现:`apps/replayer/tests/stress.rs::rpl_120_gate_cases`

### RPL-121 门槛用例出现强平时退出 1

- 规则:[sim](README.md#sim)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:integ
- 里程碑:M4
- 前置:RPL-120 的样本副本,引导块中 `RiskParams.liq_dist_force = 0`(重算 blob 哈希)
- 步骤:
  1. `replayer sim --journal <副本> --from-seq Q --stress gate --accounts sim-a --out $OUT`
- 期望:
  - 退出 1;至少一个门槛用例的 `stress-*.json` 中 `liquidations > 0`、`passed = false`
- 实现:`apps/replayer/tests/stress.rs::rpl_121_gate_liquidation_exit_1`

### RPL-122 l3-path 报告的 L1、L2、L3 首次时刻依次递增

- 规则:[sim](README.md#sim)
- 级别:integ
- 里程碑:M4
- 前置:RPL-120 的样本
- 步骤:
  1. `replayer sim --journal <样本> --from-seq Q --stress l3-path --accounts sim-a --out $OUT`
- 期望:
  - 退出 0;`stress-l3-path.json`:`first_level_tsc[0] < first_level_tsc[1] < first_level_tsc[2]`,三者都非 0;`liquidations = 0`
- 实现:`apps/replayer/tests/stress.rs::rpl_122_l3_path_order`

### RPL-123 非门槛用例各跑一次,出现强平也退出 0 且报告标注

- 规则:[sim](README.md#sim)
- 级别:table
- 里程碑:M4
- 前置:RPL-121 的副本(`liq_dist_force = 0`,保证会强平);`--accounts sim-a --from-seq Q`
- 实现:`apps/replayer/tests/stress.rs::rpl_123_non_gate_cases_report_only`

| 行 | `--stress` | 期望 |
|---|---|---|
| RPL-123.1 | `basis-widen` | 退出 0;`stress-basis-widen.json`:`gate = false`、`passed = true`,`liquidations` 照实填写 |
| RPL-123.2 | `fast-30` | 退出 0;`stress-fast-30.json` 同上 |
| RPL-123.3 | `gw-down` | 退出 0;`stress-gw-down.json` 同上 |
| RPL-123.4 | `thin-book` | 退出 0;`stress-thin-book.json` 同上 |
| RPL-123.5 | `core-restart` | 退出 0;`stress-core-restart.json` 同上 |
| RPL-123.6 | `all` | 退出 1(门槛用例强平);7 个 `stress-*.json` 全部写出 |
| RPL-123.7 | `basis-widen,fast-30` | 退出 0;恰好写出这两个文件 |
| RPL-123.8 | `no-such-case` | 退出 2;不写任何 `stress-*.json` |

### RPL-124 被测账户交易的永续 inst 缺分档维持保证金率时拒绝运行

- 规则:[sim](README.md#sim)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:unit
- 里程碑:M4
- 前置:RPL-120 的样本副本,refdata ConfigBlob 中删去 `sim-a` 交易的一个永续 inst 的分档维持保证金率(重算 blob 哈希)
- 步骤:
  1. `replayer sim --journal <副本> --from-seq Q --stress gate --accounts sim-a --out $OUT`
- 期望:
  - 退出 2;不写任何 `stress-*.json`;标准错误输出含 `MmrTiersMissing` 与该 inst
- 实现:`apps/replayer/tests/stress.rs::rpl_124_mmr_tiers_missing_exit_2`

### RPL-125 引擎与 SimExchange 看到同一份变换后的行情

- 规则:[sim](README.md#sim)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M4
- 前置:RPL-120 的样本;`--stress one-sided-30`;测试钩子记录 sequencer 收到的每条 `Bbo` 与 `SimExchange::on_record` 收到的每条 `Bbo`
- 步骤:
  1. 运行至 `t0` + 30 min
- 期望:
  - 两侧记录的 `Bbo` 序列逐条相等(seq、inst、价、量);`t0` + 30 min 时 scope 内 inst 的 `Bbo` 中间价 = 原值 × (1 + 0.15) 按 tick 取整(买价向下、卖价向上)
- 实现:`apps/replayer/tests/stress.rs::rpl_125_same_transformed_market`

### RPL-126 gw-down 故障经 SimExchange 的连接状态接口注入

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M4
- 前置:RPL-120 的样本;`--stress gw-down --emit-journal`;起点快照中 `sim-a` 永续名义最大的 venue 为 `v-perp`
- 步骤:
  1. 运行并读取 `$OUT/journal`
- 期望:
  - `SimExchange::set_order_conn(sim-a, v-perp, false, t0 + 20 min)` 与 `(…, true, t0 + 30 min)` 各调用一次;输出 journal 中 `v-perp` 下单连接的 `ConnState{Down}` 与 `ConnState{Up}` 各一条,时刻相同
- 实现:`apps/replayer/tests/stress.rs::rpl_126_gw_down_injection`

### RPL-127 core-restart 故障按崩溃处理:丢弃未提交尾部、从最近快照恢复、停机期间只推进模拟交易所

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M4
- 前置:RPL-120 的样本;`--stress core-restart --emit-journal`(`CoreRestart{at_ms = 1 800 000, down_ms = 60 000}`);`sim` 实例非干净重启后无 `RecoverMismatch`、无毒丸
- 步骤:
  1. 运行并读取 `$OUT/journal`
- 期望:
  - `t0` + 30 min 处之后未提交的记录不出现在输出 journal;此后 60 s 内 sequencer 没有处理任何输入,`SimExchange::on_record` 照常收到行情
  - 重启后从本次重演最近的快照起步(新段引导块,`EngineClock` 一条);Recovery 按 sim 实例规则自动解除(输出中有解除 Recovery 的记录,无 `Note{RecoveryHeldSim}`)
- 实现:`apps/replayer/tests/stress.rs::rpl_127_core_restart_injection`

### RPL-128 --stress-spec 按 venue 单独变价:只报告,不影响退出码

- 规则:[sim](README.md#sim)
- 级别:integ
- 里程碑:M4
- 前置:RPL-121 的副本(会强平);`spec.toml`:`case = "Custom"`、`path = {OneSided, pct_bp = 3000, ramp_ms = 3 600 000, hold_ms = 1 800 000}`、`scope = {Venue = "binance-usdm"}`、`qty_scale_bp = 10000`、`leverage = "Operating"`;`sim-a` 在 `binance-usdm` 与 `okx-swap` 上各有一条永续腿
- 步骤:
  1. `replayer sim --journal <副本> --from-seq Q --stress-spec spec.toml --accounts sim-a --emit-journal --out $OUT`
- 期望:
  - 退出 0;写出 `stress-custom.json`(`case = Custom`、`gate = false`、`passed = true`)
  - 输出 journal 中只有 `binance-usdm` 的 inst 的 `Bbo` 价被变换;`okx-swap` 的 `Bbo` 与输入逐字节相同
- 实现:`apps/replayer/tests/stress.rs::rpl_128_custom_venue_scope`

### RPL-129 --stress-spec 不合法时退出 2

- 规则:[sim](README.md#sim)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M4
- 前置:RPL-120 的样本;每行给出 `spec.toml` 的内容
- 实现:`apps/replayer/tests/stress.rs::rpl_129_bad_stress_spec`

| 行 | 规格 | 期望 |
|---|---|---|
| RPL-129.1 | `scope = {Venue = "no-such-venue"}` | 退出 2(`SimCfgErr::UnknownVenue`);不写 `stress-custom.json` |
| RPL-129.2 | 缺 `path` | 退出 2 |
| RPL-129.3 | 多一个未知键 `foo = 1` | 退出 2 |
| RPL-129.4 | `case = "OneSided30"`(不是 Custom) | 退出 2 |
| RPL-129.5 | RPL-128 的合法规格 | 退出 0 |

### RPL-140 未给 --ab 时任何选项都不替换策略参数

- 规则:[sim](README.md#sim)
- 级别:table
- 里程碑:M4
- 前置:样本 S1;journal 中 `tss` 的 `StrategyParams` 参数体哈希为 `h`;测试钩子记录 sequencer 生效的 `tss` 参数体哈希
- 实现:`apps/replayer/tests/ab.rs::rpl_140_no_param_substitution_without_ab`

| 行 | 运行 | 期望 |
|---|---|---|
| RPL-140.1 | `sim --journal S1 --out $OUT` | 全程生效哈希 = `h`;输入流中没有合成的 `SetParams` |
| RPL-140.2 | `sim --journal S1 --stress fast-30 --out $OUT`(起点为满仓快照的副本) | 全程生效哈希 = `h` |
| RPL-140.3 | `sim --journal S1 --as-sim …`(RPL-095 的设置) | 全程生效哈希 = journal 中的值 |

### RPL-141 A 与 B 相同时全部差值为 0

- 规则:[sim](README.md#sim)
- 级别:unit
- 里程碑:M4
- 前置:样本 S1;`pA.toml` 与 `pB.toml` 都是 hft-cfg 生成的 `tss` 片段,内容相同
- 步骤:
  1. `replayer sim --journal S1 --accounts sim-a --ab pA.toml pB.toml --out $OUT`
- 期望:
  - 退出 0;`ab.json` 中每个差值字段 = 0(账户级与按 group 的报告字段差、目标决策条数差、订单决策条数差、每个拒单原因计数差);第一处决策分歧 seq 为 `null`
- 实现:`apps/replayer/tests/ab.rs::rpl_141_identical_params_zero_diff`

### RPL-142 ab-report.txt 首行与 ab.json.notice 为规定文字

- 规则:[sim](README.md#sim)
- 级别:unit
- 里程碑:M4
- 前置:样本 S1;`pA.toml`、`pB.toml`(B 的 `max_pos` 为 A 的 2 倍)
- 步骤:
  1. `replayer sim --journal S1 --accounts sim-a --ab pA.toml pB.toml --out $OUT`
- 期望:
  - 退出 0;`ab-report.txt` 第一行逐字节等于"相对比较,不是收益估计,不得用于定仓";`ab.json` 第一个键为 `notice`,值为同一句话
- 实现:`apps/replayer/tests/ab.rs::rpl_142_notice_text`

### RPL-143 --ab 报告只含 B − A 的差值,不出现任一组的绝对值

- 规则:[sim](README.md#sim)
- 级别:unit
- 里程碑:M4
- 前置:同 RPL-142;两次重演的 hft-sim 报告字段(测试钩子取出)记为 `rA`、`rB`
- 步骤:
  1. 运行并解析 `ab.json`、`ab-report.txt`
- 期望:
  - `ab.json` 的每个数值字段 = `rB` 对应字段 − `rA` 对应字段;不含 `rA` 或 `rB` 的任何字段原值(不存在名为 `a`、`b`、`abs`、`pnl` 的键)
  - `ab-report.txt` 中不出现 `rA` 或 `rB` 的已实现 PnL 的十进制表示
  - 第一处决策分歧 seq = 两次重演中第一条不同的 `OrderDecision`/`ExecDecision` 的 seq
- 实现:`apps/replayer/tests/ab.rs::rpl_143_only_differences`

### RPL-144 --ab 的参数文件与约束校验

- 规则:[sim](README.md#sim)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M4
- 前置:样本 S1b;合法的 `pA.toml`、`pB.toml`;每行只改该行条件
- 实现:`apps/replayer/tests/ab.rs::rpl_144_ab_validation`

| 行 | 条件 | 期望 |
|---|---|---|
| RPL-144.1 | `pB.toml` 的 `kind_id` 与 journal 中 `tss` 不同 | 退出 2;`$OUT` 中没有 `ab.json` |
| RPL-144.2 | `pA.toml` 另含实例 `tsb2` 的参数 | 退出 2 |
| RPL-144.3 | 同时给 `--check-fills` | 退出 2 |
| RPL-144.4 | 同时给 `--stress gate` | 退出 2 |
| RPL-144.5 | 同时给 `--emit-journal` | 退出 2 |
| RPL-144.6 | `--accounts sim-a,sim-b` | 退出 2 |
| RPL-144.7 | 不给 `--accounts`(缺省为两个 Sim 账户) | 退出 2 |
| RPL-144.8 | 同时给 `--advice journal` | 退出 2 |
| RPL-144.9 | 同时给 `--as-sim`(把 Live 账户当作 Sim,`--accounts` 为该账户) | 退出 0 |
| RPL-144.10 | 合法组合 | 退出 0 |

### RPL-145 两次重演各自独立,参数替换以合成 SetParams 在起点快照之后进入且不写回任何 journal

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M4
- 前置:样本 S1(只读副本,记录全部段文件的 SHA-256);`pA.toml`、`pB.toml`
- 步骤:
  1. `replayer sim --journal S1 --accounts sim-a --ab pA.toml pB.toml --out $OUT`,测试钩子记录两次重演各自的第一条 Input
- 期望:
  - 两次重演的第一条 Input 都是合成的 `SetParams`(分别为 A、B 的参数体),位于起点快照恢复之后;两次重演各有自己的 sequencer、`SimExchange` 与 Model 实例(构造计数各 2)
  - 运行后 S1 全部段文件的 SHA-256 不变;`$OUT` 中没有段文件
- 实现:`apps/replayer/tests/ab.rs::rpl_145_independent_runs_no_writeback`

### RPL-160 同一 journal 与自身比对全部一致

- 规则:[compare](README.md#compare)
- 级别:unit
- 里程碑:M6
- 前置:样本 T1
- 步骤:
  1. `replayer compare --a $T1 --b $T1 --map acct-l=acct-l --out $OUT`
- 期望:
  - 退出 0;`compare.json`:一致数 = A 中可比对决策数,字段不同 = 0、只在 A = 0、只在 B = 0;配对时间差 p50 = p99 = 0;参数分段表只有一段(覆盖整个窗口,参数体哈希相同)
  - `diffs.parquet` 行数为 0
- 实现:`apps/replayer/tests/compare.rs::rpl_160_self_compare`

### RPL-161 Shadow 实例与同参数 Sim 实例各录 1 h:输出配对统计与时间差分位

- 规则:[compare](README.md#compare)
- 级别:integ
- 里程碑:M6
- 前置:hft-testkit 以同一行情源分别驱动两个进程内 hft-core:A = `prod` 实例(Live 账户 `acct-l`,测试策略 A 实例 `tsa`,全部 cell `RunMode = Shadow`),B = `sim` 实例(Sim 账户 `sim-acct-l`,同参数的 `tsa`);B 侧行情晚到 3 ms;各录 1 h
- 步骤:
  1. `replayer compare --a <A> --b <B> --map acct-l=sim-acct-l --out $OUT`
- 期望:
  - `compare.json` 给出一致、字段不同、只在 A、只在 B 四类计数,配对时间差(B − A)的 p50 与 p99 落在 [2 ms, 4 ms];退出码 = 0(四类中后三类全为 0 时)或 1(否则)
  - 比对对象包括 `ExecDecision` 的目标类决策、模式门结果、`OrderDecision` 与策略 `Note`(各类计数在 `compare.json` 按种类分列)
- 实现:`apps/replayer/tests/compare.rs::rpl_161_shadow_vs_sim`

### RPL-162 参数不同的区间列为 params_differ、不计不一致

- 规则:[compare](README.md#compare)
- 级别:unit
- 里程碑:M6
- 前置:RPL-161 的两份 journal 副本;B 在 20–30 min 之间经 `SetParams` 把 `tsa` 的一个非结构字段改为不同值,30 min 时改回
- 步骤:
  1. `replayer compare --a <A> --b <B'> --map acct-l=sim-acct-l --out $OUT`
- 期望:
  - `compare.json` 的参数分段表有三段,中间一段 [20 min, 30 min) 标 `params_differ`;该段内的决策不进入四类计数;另两段的计数与 RPL-161 在同时段的计数相同
- 实现:`apps/replayer/tests/compare.rs::rpl_162_params_differ_interval`

### RPL-163 删去 B 中一条目标决策:报告只在 A、退出 1

- 规则:[compare](README.md#compare)
- 级别:unit
- 里程碑:M6
- 前置:样本 T1 作 A;B = T1 的副本删去一条 `ExecDecision{设目标}`(seq `t`)
- 步骤:
  1. `replayer compare --a $T1 --b <B> --map acct-l=acct-l --out $OUT`
- 期望:
  - 退出 1;`compare.json` 只在 A = 1、只在 B = 0、字段不同 = 0
  - `diffs.parquet` 一行:A 侧 seq = `t`、B 侧 seq 为空;含 A 侧 UTC 时间、该时刻两边该 group 各腿的 BBO、账户仓位与可用保证金视图
- 实现:`apps/replayer/tests/compare.rs::rpl_163_only_in_a`

### RPL-164 B 多一条决策时报告只在 B、退出 1

- 规则:[compare](README.md#compare)
- 级别:unit
- 里程碑:M6
- 前置:A = T1 的副本删去一条 `ExecDecision{清目标}`;B = T1
- 步骤:
  1. `replayer compare --a <A> --b $T1 --map acct-l=acct-l --out $OUT`
- 期望:
  - 退出 1;只在 B = 1;`diffs.parquet` 一行,A 侧 seq 为空
- 实现:`apps/replayer/tests/compare.rs::rpl_164_only_in_b`

### RPL-165 配对后逐字段比较,目标量按各自 inst 的 lot 归一

- 规则:[compare](README.md#compare)
- 级别:table
- 里程碑:M6
- 前置:合成的两份 journal,各一条同 group、同种类、时间差 10 ms 的 `ExecDecision{设目标}`;A 侧 inst 的 lot = 0.001,B 侧同规范符号 inst 的 lot = 0.01;`--window-ms 2000`
- 实现:`apps/replayer/tests/compare.rs::rpl_165_field_compare_lot_normalized`

| 行 | A 目标量 | B 目标量 | 其他字段 | 期望 |
|---|---|---|---|---|
| RPL-165.1 | 0.050(50 lot) | 0.05(5 lot) | 相同 | 一致;退出 0 |
| RPL-165.2 | 0.050 | 0.06 | 相同 | 字段不同 = 1;`diffs.parquet` 的字段 diff 只含目标量;退出 1 |
| RPL-165.3 | 0.050 | 0.05 | `reason` 不同 | 字段不同 = 1;退出 1 |
| RPL-165.4 | 0.050 | 0.05 | `origin` 不同 | 字段不同 = 1;退出 1 |

### RPL-166 时间窗边界:时间差绝对值 ≤ --window-ms 才配对

- 规则:[compare](README.md#compare)
- 级别:table
- 里程碑:M6
- 前置:合成的两份 journal,各一条同 group、同种类、字段相同的决策;`--window-ms 2000`;B − A 的 UTC 时间差按行给出
- 实现:`apps/replayer/tests/compare.rs::rpl_166_window_boundary`

| 行 | B − A | 期望 |
|---|---|---|
| RPL-166.1 | +1 999 ms | 配对,一致;退出 0 |
| RPL-166.2 | +2 000 ms | 配对,一致;退出 0 |
| RPL-166.3 | +2 001 ms | 只在 A = 1、只在 B = 1;退出 1 |
| RPL-166.4 | −2 000 ms | 配对,一致;退出 0 |

### RPL-167 一条 A 决策与 B 中尚未配对的最早一条配对

- 规则:[compare](README.md#compare)
- 级别:unit
- 里程碑:M6
- 前置:合成 journal:A 有两条同 group、同种类的决策 a1(t = 0)、a2(t = 500 ms);B 有 b1(t = 100 ms)、b2(t = 600 ms),字段两两相同;`--window-ms 2000`
- 步骤:
  1. 运行 compare
- 期望:
  - 配对为 (a1, b1)、(a2, b2);一致 = 2;配对时间差 p50 = p99 = 100 ms
- 实现:`apps/replayer/tests/compare.rs::rpl_167_earliest_unpaired`

### RPL-168 两边所绑策略实例的 kind_id 不同时退出 2

- 规则:[compare](README.md#compare)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:unit
- 里程碑:M6
- 前置:A = T1;B 的 `acct-l` 绑定测试策略 B 的实例(`kind_id` 不同)
- 步骤:
  1. `replayer compare --a $T1 --b <B> --map acct-l=acct-l --out $OUT`
- 期望:
  - 退出 2;`$OUT` 中没有 `compare.json`
- 实现:`apps/replayer/tests/compare.rs::rpl_168_kind_id_differs`

### RPL-169 --map 不合法时退出 2

- 规则:[compare](README.md#compare)
- 级别:table
- 里程碑:M6
- 前置:A = T1(账户 `acct-l`);B = RPL-161 的 B(账户 `sim-acct-l`)
- 实现:`apps/replayer/tests/compare.rs::rpl_169_bad_map`

| 行 | `--map` | 期望 |
|---|---|---|
| RPL-169.1 | 缺省 | 退出 2 |
| RPL-169.2 | `acct-x=sim-acct-l`(A 的 refdata 中没有 `acct-x`) | 退出 2 |
| RPL-169.3 | `acct-l=sim-acct-x`(B 的 refdata 中没有) | 退出 2 |
| RPL-169.4 | `acct-l=sim-acct-l,acct-l=sim-acct-l` | 退出 2(同一账户出现两次) |
| RPL-169.5 | `acct-l=sim-acct-l` | 退出码由比对结果决定(0 或 1) |

### RPL-170 窗口内没有参数相同的区间时退出 2

- 规则:[compare](README.md#compare)
- 级别:table
- 里程碑:M6
- 前置:RPL-161 的两份 journal;B 的副本中 `tsa` 全程参数与 A 不同
- 实现:`apps/replayer/tests/compare.rs::rpl_170_no_same_params_interval`

| 行 | 输入与窗口 | 期望 |
|---|---|---|
| RPL-170.1 | A 与参数全程不同的 B | 退出 2 |
| RPL-170.2 | A 与 B,`--from`/`--to` 落在两份 journal 时间范围之外 | 退出 2 |
| RPL-170.3 | A 与 B(参数相同) | 退出 0 或 1(按比对结果) |

### RPL-171 inst 按规范符号、group 按 GroupSpec 指纹对齐,与两边的编号无关

- 规则:[compare](README.md#compare)
- 级别:unit
- 里程碑:M6
- 前置:A = T1;B = 同一行情与参数下另一实例的 journal,其 refdata 的 `InstId` 编号与 A 不同(同一规范符号 `BTCUSDT@v-perp` 在 A 为 7、在 B 为 12),`GroupId` 编号也不同(同一 `GroupSpec` 指纹)
- 步骤:
  1. `replayer compare --a $T1 --b <B> --map acct-l=acct-l --out $OUT`
- 期望:
  - 全部决策按规范符号与指纹配对;只在 A = 只在 B = 0
- 实现:`apps/replayer/tests/compare.rs::rpl_171_align_by_symbol_and_fingerprint`

### RPL-172 compare 不运行 sequencer、不输出收益,结果确定

- 规则:[compare](README.md#compare)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M6
- 前置:RPL-161 的两份 journal
- 步骤:
  1. 运行 compare 两次,分别写 `$OUT/a`、`$OUT/b`
- 期望:
  - `a` 与 `b` 的 `compare.json`、`diffs.parquet` 逐字节相同;测试构建中 `Sequencer::boot` 调用计数为 0
  - `compare.json` 与 `diffs.parquet` 中没有 `pnl`、`profit`、`return` 字段
- 实现:`apps/replayer/tests/compare.rs::rpl_172_no_sequencer_deterministic`

### RPL-173 S3 前缀(带主机段)与本地冷副本的比对结果相同

- 规则:[compare](README.md#compare)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:integ
- 里程碑:M6
- 前置:本地 S3 模拟服务提供 RPL-161 两份 journal 的 S3 前缀 `s3://b/p/h1/prod/…` 与 `s3://b/p/h1/sim/…`(manifest 与 SHA-256)
- 步骤:
  1. 以本地目录运行 compare 写 `$OUT/l`
  2. 以 S3 前缀运行 compare 写 `$OUT/s`
- 期望:
  - 两次退出码相同;`compare.json`、`diffs.parquet` 逐字节相同
- 实现:`apps/replayer/tests/compare.rs::rpl_173_s3_same_as_local`

### RPL-190 合成 journal:插值换算后的分段与真值之差在逐样本误差界内

- 规则:[latency](README.md#latency)
- 级别:unit
- 里程碑:M3
- 前置:合成 journal:每 1 000 ms 一条 `ClockStatus`,TSC 与 PHC 之间有固定频差 +20 ppm,`calib_err_ns = 100`;1 000 条 `Bbo`,已知真值的 `hw_rx`、`rx_tsc`、记录头 `tsc`(`kernel_rx` 真值 = 3 000 ns,`ingest` 真值 = 1 500 ns);`--cal-max-err-ns 1000 --burst-gap-ns 50000`
- 步骤:
  1. `replayer latency --journal <合成> --cal-max-err-ns 1000 --burst-gap-ns 50000 --out $OUT`
- 期望:
  - 退出 0;`latency.json` 一个数据集;`kernel_rx` 组的 p50 与 3 000 ns 之差 ≤ 该组 `cal_err` 的 max;`ingest` 组 p50 = 1 500 ns(纯 TSC 分段,无换算误差);`unsynced_cal = 0`
- 实现:`apps/replayer/tests/latency.rs::rpl_190_interpolation_within_error`

### RPL-191 缺 ClockStatus 使误差界超限的样本计入 unsynced_cal、不进分位数

- 规则:[latency](README.md#latency)
- 级别:unit
- 里程碑:M3
- 前置:RPL-190 的合成 journal 副本,删去 t = 300 s 到 t = 360 s 之间的全部 `ClockStatus`(该区间的样本误差界 > 1 000 ns);该区间内 `Bbo` 有 60 条
- 步骤:
  1. 同 RPL-190 运行
- 期望:
  - `unsynced_cal = 60`;`kernel_rx` 组样本数 = 940;`cal_err` 的 max 记录了被排除样本的误差界(> 1 000 ns)
- 实现:`apps/replayer/tests/latency.rs::rpl_191_unsynced_cal_excluded`

### RPL-192 没有 HW 时间戳的记录以 sw_rx 为锚并归入 SW 数据集

- 规则:[latency](README.md#latency)
- 级别:unit
- 里程碑:M3
- 前置:合成 journal:500 条 `Bbo` 带 HW 时间戳,300 条 `MarketHdr` 的 `hw_rx` 来源为 `SW_TS`(只有 `sw_rx`)
- 步骤:
  1. 同 RPL-190 运行
- 期望:
  - `groups` 中同一 (venue, kind, stage) 分成 `ts_src = Hw`(500 条)与 `ts_src = Sw`(300 条)两组,不合并;`sw_anchor_share = 0.375`
- 实现:`apps/replayer/tests/latency.rs::rpl_192_sw_anchor`

### RPL-193 平静行情中插入 1 s 深度突发:按事件加权的 p99 反映突发,每秒抽一个样本的统计看不到

- 规则:[latency](README.md#latency)
- 级别:unit
- 里程碑:M3
- 前置:合成 journal(600 s):每秒 10 条 `Bbo`,`kernel_rx` 真值 2 µs;t = 300 s 的 1 s 内插入 2 500 条 `L2Snapshot{origin = TopN}` 与 2 500 条 `Bbo`(`kernel_rx` 真值 40 µs,每 5 条共享一个 `rx_tsc`);对照统计 = 每秒取第一条记录
- 步骤:
  1. 同 RPL-190 运行;另以测试代码计算对照统计
- 期望:
  - 把 `latency-0.hdr` 中 `Bbo` 的 `kernel_rx` 在 `Burst` 与 `Calm` 两层的直方图相加,p99 ≥ 38 µs;对照统计的 p99 ≤ 2.1 µs
  - 突发的 5 000 条记录落在 `stratum = Burst` 的组,其余 6 000 条落在 `Calm` 组
- 实现:`apps/replayer/tests/latency.rs::rpl_193_burst_visible`

### RPL-194 同一输入两次运行输出逐字节相同

- 规则:[latency](README.md#latency)
- 级别:unit
- 里程碑:M3
- 前置:RPL-193 的合成 journal
- 步骤:
  1. 运行两次,分别写 `$OUT/a`、`$OUT/b`
- 期望:
  - `latency.json` 与每个 `latency-<k>.hdr` 逐字节相同;`latency.json` 中没有运行时刻字段
- 实现:`apps/replayer/tests/latency.rs::rpl_194_deterministic_output`

### RPL-195 两天的直方图相加等于两天连在一起运行

- 规则:[latency](README.md#latency)
- 级别:unit
- 里程碑:M3
- 前置:合成 journal 跨两个 UTC 日(同一数据集);UTC 00:00 两侧 1 s 内没有同一次读出的记录(切点不拆分 `frames_in_read` 组)
- 步骤:
  1. `--from D1T00:00Z --to D2T00:00Z` 运行写 `$OUT/d1`
  2. `--from D2T00:00Z --to D3T00:00Z` 运行写 `$OUT/d2`
  3. `--from D1T00:00Z --to D3T00:00Z` 运行写 `$OUT/all`
  4. 测试代码把 `d1/latency-0.hdr` 与 `d2/latency-0.hdr` 中同键直方图相加
- 期望:
  - 步骤 4 的每个直方图与 `all/latency-0.hdr` 的同键直方图计数逐桶相等
  - `d1` 中 D1 最后一秒的样本没有因缺少后一条 `ClockStatus` 计入 `unsynced_cal`(读入了 `--to` 之后的第一条 `ClockStatus`)
- 实现:`apps/replayer/tests/latency.rs::rpl_195_daily_merge_equals_contiguous`

### RPL-196 分档深度快照记录进入统计并按记录种类单独成组

- 规则:[latency](README.md#latency)
- 级别:unit
- 里程碑:M3
- 前置:合成 journal:800 条 `Bbo`、200 条 `L2Snapshot{origin = TopN}`、50 条 `Trade`、10 条 `MarkPrice`、3 条 `Funding`、20 条 `ControlCmd`(无 `MarketHdr`)
- 步骤:
  1. 同 RPL-190 运行
- 期望:
  - `n = 1 063`;`groups` 中 `kind` 分别为 `Bbo`、`L2Snapshot`、`Trade`、`MarkPrice`、`Funding` 的组,样本数依次为 800、200、50、10、3;没有 `ControlCmd` 的组
- 实现:`apps/replayer/tests/latency.rs::rpl_196_record_kinds_grouped`

### RPL-197 rx_empty_gap_ns 非 0 的样本输出 kernel_rx 上界,为 0 的计入 gap_missing

- 规则:[latency](README.md#latency)
- 级别:table
- 里程碑:M3
- 前置:合成 journal:三组 HW `Bbo` 各 100 条,`rx_empty_gap_ns` 按行给出;`nic_to_kernel` 真值 1 000 ns
- 实现:`apps/replayer/tests/latency.rs::rpl_197_kernel_rx_upper_bound`

| 行 | `rx_empty_gap_ns` | 期望 |
|---|---|---|
| RPL-197.1 | 5 000 | `kernel_rx_hi` 组有 100 个样本,p50 = 6 000 ns |
| RPL-197.2 | 0 | 不进 `kernel_rx_hi`;`gap_missing` 增加 100 |
| RPL-197.3 | 1 | `kernel_rx_hi` 组 p50 = 1 001 ns |

### RPL-198 段首 HostProfile 变化时输出两个数据集与两个直方图文件

- 规则:[latency](README.md#latency)
- 级别:unit
- 里程碑:M3
- 前置:合成 journal 两段:段 1 的 `HostProfile` 为主机类 `c16`、发布 `v1.0.0`;段 2 为 `c32`、`v1.0.0`
- 步骤:
  1. 同 RPL-190 运行
- 期望:
  - `latency.json.datasets` 有 2 项,按 seq 顺序,`host_profile` 不同;`$OUT` 中有 `latency-0.hdr`、`latency-1.hdr`,没有 `latency-2.hdr`;两数据集的样本数之和 = `n` 之和
- 实现:`apps/replayer/tests/latency.rs::rpl_198_dataset_per_host_profile`

### RPL-199 按层给出 unsynced_cal 与总数

- 规则:[latency](README.md#latency)
- 级别:unit
- 里程碑:M3
- 前置:RPL-193 的合成 journal,另删去突发期间前后的两条 `ClockStatus`(突发层 5 000 条全部 `unsynced_cal`);`--burst-gap-ns 50000`
- 步骤:
  1. 运行
- 期望:
  - `unsynced_cal_by_stratum`:`Burst` 为 (5 000, 5 000),`Calm` 为 (0, 6 000);`unsynced_cal = 5 000`
- 实现:`apps/replayer/tests/latency.rs::rpl_199_unsynced_by_stratum`

### RPL-200 --venue 只保留所列 venue 的组

- 规则:[latency](README.md#latency)
- 级别:table
- 里程碑:M3
- 前置:合成 journal:`binance-usdm` 600 条、`okx-swap` 400 条 `Bbo`
- 实现:`apps/replayer/tests/latency.rs::rpl_200_venue_filter`

| 行 | `--venue` | 期望 |
|---|---|---|
| RPL-200.1 | 不给 | `groups` 含两个 venue |
| RPL-200.2 | `okx-swap` | `groups` 只含 `okx-swap`,样本数 400 |
| RPL-200.3 | `binance-usdm,okx-swap` | 两个 venue 都在 |
| RPL-200.4 | `no-such-venue` | 退出 2 |

### RPL-201 --from/--to 按 [from, to) 取记录

- 规则:[latency](README.md#latency)
- 级别:table
- 里程碑:M3
- 前置:合成 journal:三条 `Bbo` 的 UTC(记录头 `tsc` 经段头校准换算)分别为 T − 1 ns、T、T + 1 ns,另一条为 U − 1 ns、U;`--from T --to U`
- 实现:`apps/replayer/tests/latency.rs::rpl_201_window_half_open`

| 行 | 记录 | 期望 |
|---|---|---|
| RPL-201.1 | T − 1 ns | 不计样本 |
| RPL-201.2 | T | 计入 |
| RPL-201.3 | T + 1 ns | 计入 |
| RPL-201.4 | U − 1 ns | 计入 |
| RPL-201.5 | U | 不计样本 |

### RPL-202 --cal-max-err-ns 与 --burst-gap-ns 写进 cfg 并决定过滤与分层

- 规则:[latency](README.md#latency)
- 级别:table
- 里程碑:M3
- 前置:合成 journal:100 条 `Bbo` 的逐样本误差界为 900 ns,100 条为 1 100 ns;其中 50 条 `rx_empty_gap_ns = 40 000`、其余 0,每次读出 1 帧
- 实现:`apps/replayer/tests/latency.rs::rpl_202_cli_thresholds`

| 行 | 参数 | 期望 |
|---|---|---|
| RPL-202.1 | `--cal-max-err-ns 1000 --burst-gap-ns 50000` | `cfg.cal_max_err_ns = 1000`、`cfg.burst_gap_ns = 50000`;`unsynced_cal = 100`;`Burst` 组样本数 0 |
| RPL-202.2 | `--cal-max-err-ns 1200 --burst-gap-ns 50000` | `unsynced_cal = 0` |
| RPL-202.3 | `--cal-max-err-ns 1000 --burst-gap-ns 39999` | 带 40 000 的样本中未被过滤的进 `Burst` 组 |
| RPL-202.4 | `--cal-max-err-ns 1000 --burst-gap-ns 40000` | 这些样本进 `Calm` 组(> 阈值才是突发) |

### RPL-203 latency.json 的数据集项字段齐全,探针样本输出分层结果

- 规则:[latency](README.md#latency)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/replay/latency-mini/`:合成 journal(一个数据集;`Bbo` 2 000 条,其中突发层 400 条;行情触发的 `LatencySample` 300 条,突发层 60 条、平静层 240 条);`--cal-max-err-ns 1000 --burst-gap-ns 50000`
- 期望文件:`fixtures/replay/latency-mini/RPL-203/latency.json`(逐字段相等:`cfg`;`datasets[0]` 的 `host_profile`、`release`、`n`、`missing`、`gap_missing`、`negative`、`unsynced_cal`、`unsynced_cal_by_stratum`、`sw_anchor_share`、`groups`、`kernel_rx_hi`、`cal_err`、`probe_strata`;`probe_strata` 中平静层与突发层各有 p50/p99,权重为 0.8/0.2;样本不足的组其合成值为 `null`)
- 实现:`apps/replayer/tests/latency.rs::rpl_203_latency_json_golden`

### RPL-204 latency 不运行 sequencer,未注册的 kind_id 不导致退出 4

- 规则:[latency](README.md#latency)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:unit
- 里程碑:M3
- 前置:RPL-190 的合成 journal,另含一条引用未注册策略 `kind_id` 的 `StrategyParams` ConfigBlob
- 步骤:
  1. 同 RPL-190 运行
  2. `replayer verify --journal <同一 journal>`
- 期望:
  - 步骤 1 退出 0;测试构建中 `Sequencer::boot` 调用计数为 0
  - 步骤 2 退出 4
- 实现:`apps/replayer/tests/latency.rs::rpl_204_no_sequencer_no_exit_4`

### RPL-205 latency 输入缺段时退出 3

- 规则:[latency](README.md#latency)、[输入定位与退出码](README.md#输入定位与退出码)
- 级别:unit
- 里程碑:M3
- 前置:三段的合成 journal,删去中间一段
- 步骤:
  1. 同 RPL-190 运行
- 期望:
  - 退出 3;`$OUT` 中没有 `latency.json`
- 实现:`apps/replayer/tests/latency.rs::rpl_205_missing_segment_exit_3`

### RPL-206 扫描器只缓存两条 ClockStatus 之间的记录

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:合成 journal(1 h):每 1 000 ms 一条 `ClockStatus`,每秒 2 000 条行情记录;测试构建记录扫描器缓冲的峰值条数
- 步骤:
  1. 同 RPL-190 运行
- 期望:
  - 缓冲峰值 ≤ 2 000 + 1 条(任一时刻只缓存到下一条 `ClockStatus` 之前的记录)
- 实现:`apps/replayer/tests/latency.rs::rpl_206_bounded_buffer`

### RPL-220 单个段文件可直接作为输入

- 规则:[输入定位与退出码](README.md#输入定位与退出码)
- 级别:unit
- 里程碑:M2
- 前置:样本 T1 的第 1 段文件 `$T1/…/0000000001-….jrn`
- 步骤:
  1. `replayer verify --journal <该文件>`
- 期望:
  - 退出 0;比较过的 Output 只来自第 1 段
- 实现:`apps/replayer/tests/input.rs::rpl_220_single_segment_file`

### RPL-221 多段输入按 first_seq 排序,与目录列举顺序无关

- 规则:[输入定位与退出码](README.md#输入定位与退出码)
- 级别:unit
- 里程碑:M2
- 前置:样本 T1 的两段复制到一个目录,文件名改为使字典序与 `first_seq` 顺序相反(段头不变)
- 步骤:
  1. `replayer verify --journal <该目录>`
- 期望:
  - 退出 0;结果与 RPL-010 相同
- 实现:`apps/replayer/tests/input.rs::rpl_221_sort_by_first_seq`

### RPL-222 段链哈希断或缺中间段时退出 3

- 规则:[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M2
- 前置:三段的合成 journal(段 1–3);每行只做该行改动
- 实现:`apps/replayer/tests/input.rs::rpl_222_broken_chain_exit_3`

| 行 | 改动 | 期望 |
|---|---|---|
| RPL-222.1 | 删去段 2 | 退出 3;没有比较任何 Output |
| RPL-222.2 | 段 3 段头中前一段的哈希字段改 1 字节(段 3 自身的头部校验重算) | 退出 3 |
| RPL-222.3 | 只给段 2 与段 3 | 退出 0(段链在输入内部连续) |

### RPL-223 S3 对象哈希与 manifest 不符或下载失败时退出 3

- 规则:[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M5
- 前置:本地 S3 模拟服务提供 T1 的 S3 前缀;每行只做该行改动
- 实现:`apps/replayer/tests/input.rs::rpl_223_s3_hash_or_download_fail`

| 行 | 改动 | 期望 |
|---|---|---|
| RPL-223.1 | 一个 `.jrn.zst` 对象解压后改 1 字节 | 退出 3;标准错误输出含该对象键 |
| RPL-223.2 | manifest 中列出的一个对象不存在 | 退出 3 |
| RPL-223.3 | `private/` 前缀的对象对运行身份返回 `AccessDenied`(模拟无 KMS 解密权限) | 退出 3 |
| RPL-223.4 | 无改动 | 退出 0;结果与本地冷副本相同 |

### RPL-224 S3 前缀带主机段,与本地冷副本结果相同

- 规则:[输入定位与退出码](README.md#输入定位与退出码)
- 级别:integ
- 里程碑:M5
- 前置:本地 S3 模拟服务按 journal-archiver 的键布局 `s3://b/p/h1/prod/…` 提供 T1;另在 `s3://b/p/h2/prod/…` 放另一主机的 journal
- 步骤:
  1. `replayer verify --journal s3://b/p/h1/prod/… --report $OUT/s.json`
  2. `replayer verify --journal $T1 --report $OUT/l.json`
- 期望:
  - 两次退出 0;`s.json` 与 `l.json` 除 `generated_utc` 外逐字节相同;步骤 1 没有下载 `h2` 下的任何对象
- 实现:`apps/replayer/tests/input.rs::rpl_224_s3_prefix_with_host`

### RPL-225 journal 的 schema 版本或策略集合与当前二进制不兼容时退出 4

- 规则:[输入定位与退出码](README.md#输入定位与退出码)
- 级别:table
- 里程碑:M2
- 前置:样本 T1 的副本;每行只做该行改动
- 实现:`apps/replayer/tests/input.rs::rpl_225_incompatible_exit_4`

| 行 | 改动 | 期望 |
|---|---|---|
| RPL-225.1 | 引导块 `StrategyParams` 的 `kind_id` 改为未注册的值 | `verify` 退出 4;`sim` 退出 4 |
| RPL-225.2 | 段头 schema 版本改为当前二进制支持的最大值 + 1 | `verify` 退出 4 |
| RPL-225.3 | 无改动 | `verify` 退出 0 |

### RPL-240 replayer 的依赖闭包不含网络运行时与热段写者

- 规则:[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M2
- 前置:`cargo metadata` 取 `apps/replayer` 的依赖图(含启用的 feature)
- 步骤:
  1. 检查依赖图
- 期望:
  - 依赖闭包中没有 `hft-net`;`hft-venue` 不在闭包中(raw-decode 可选扩展未启用时);`hft-journal` 启用 `writer` feature,但 replayer 源码中没有 `JournalWriter::open(` 调用(只有 `open_offline`);`hft-sim` 只启用默认 feature(无 `task`)
  - `cargo xtask deps` 对该图退出 0
- 实现:`apps/replayer/tests/build.rs::rpl_240_dependency_closure`

### RPL-241 replayer 在无网络的命名空间中完成本地输入的全部子命令

- 规则:[概要](README.md#概要)、[依赖](README.md#依赖)
- 级别:proc
- 里程碑:M4
- 前置:`unshare -n`(只有 lo,且 lo 关闭);样本 T1、S1 的本地副本
- 步骤:
  1. 依次运行 `verify --journal $T1`、`sim --journal S1 --out $OUT/s`、`compare --a $T1 --b $T1 --map acct-l=acct-l --out $OUT/c`、`latency --journal $T1 --cal-max-err-ns 1000 --burst-gap-ns 50000 --out $OUT/l`
- 期望:
  - 四条命令退出 0;`strace -e trace=connect,socket` 没有记录到 `AF_INET`/`AF_INET6` 套接字
- 实现:`apps/replayer/tests/build.rs::rpl_241_no_network`

### RPL-242 replayer 不发控制命令、不写生产 journal

- 规则:[概要](README.md#概要)
- 级别:proc
- 里程碑:M4
- 前置:测试主机上以 hft-core `paths.ctl_socket` 的取值建一个 ctl socket 测试替身(记录连接);以 `paths.journal_cold` 的取值建一个生产 journal 根的测试替身目录
- 步骤:
  1. 对 T1、S1 运行 `verify`、`sim --emit-journal --out $OUT`、`compare`、`latency`
- 期望:
  - ctl socket 替身的连接数为 0;生产 journal 根替身目录中没有新文件、已有文件 mtime 不变
- 实现:`apps/replayer/tests/build.rs::rpl_242_no_ctl_no_prod_journal`

### RPL-243 读段文件用只读映射并声明顺序访问

- 规则:[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M2
- 前置:样本 T1;`strace -f -e trace=mmap,madvise`
- 步骤:
  1. `replayer verify --journal $T1`
- 期望:
  - 每个段文件以 `PROT_READ` 映射一次(没有 `PROT_WRITE`);每个映射区有一次 `madvise(…, MADV_SEQUENTIAL)`;退出 0
- 实现:`apps/replayer/tests/build.rs::rpl_243_mmap_sequential`

### RPL-250 verify 吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`replay-throughput`
- 实现:`apps/replayer/benches/replay.rs::rpl_250_replay_throughput`

### RPL-251 sim 吞吐

已删除:基准 `sim/replay_day` 的前缀归 hft-sim,由 SIM-213 覆盖(一个基准只留一个用例)。

### RPL-252 compare 吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M6
- 基准名:`compare_day`
- 实现:`apps/replayer/benches/replay.rs::rpl_252_compare_day`

### RPL-253 latency 吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`latency_day`
- 实现:`apps/replayer/benches/replay.rs::rpl_253_latency_day`

### RPL-254 整日回放的内存不超过所在主机类的 MemoryMax

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M4
- 环境:dev2(`hft-replay-verify` 的 `MemoryMax` 按 hft-core 整机表);`sim` 实例一个完整 UTC 日的冷副本
- 授权:无
- 步骤:
  1. `systemctl start hft-replay-verify`,结束后读该单元 cgroup 的 `memory.peak` 与 `/usr/bin/time -v` 的最大常驻集
- 通过标准:`memory.peak` ≤ 该主机类 `hft-replay-verify` 的 `MemoryMax`;单元未被 OOM 杀掉;报告写出
- 证据:`ops-log/<date>-RPL-254.md`
- 实现:`apps/replayer/tests/host/RPL-254.sh`

### RPL-255 排障时 sim 在 10 分钟内复现一个 1 h 窗口

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M4
- 环境:dev2 或任一装有当前发布制品的 aarch64 主机;`sim` 实例的 1 h journal(本机冷副本)
- 授权:无
- 步骤:
  1. `time replayer sim --journal <1 h 窗口> --out <dir>`
- 通过标准:墙钟时间 ≤ 600 s;退出 0
- 证据:`ops-log/<date>-RPL-255.md`(命令、墙钟时间、主机类)
- 实现:`apps/replayer/tests/host/RPL-255.sh`

### RPL-256 bench 在 p99 回归超过阈值时退出 1

- 规则:[bench(可选扩展)](README.md#bench可选扩展)
- 级别:proc
- 里程碑:M7
- 状态:可选扩展:replayer-bench
- 前置:样本 T1;同一机器上先以未修改的构建运行 `replayer bench --journal $T1` 写出 `base.json`;再以测试策略 A 热函数中加 200 ns 自旋的构建运行;`--threshold-pct 10`
- 步骤:
  1. `replayer bench --journal $T1 --baseline base.json --threshold-pct 10`(未修改构建)
  2. 同上(加自旋的构建)
- 期望:
  - 步骤 1 退出 0;步骤 2 退出 1,输出中 (kind, strategy = `tsa`) 的 p99 回归 > 10%
- 实现:`apps/replayer/tests/bench_ext.rs::rpl_256_bench_regression`

### RPL-257 raw-decode 发现解码差异并输出可做金样的记录

- 规则:[raw-decode(可选扩展)](README.md#raw-decode可选扩展)
- 级别:proc
- 里程碑:M7
- 状态:可选扩展:replayer-raw-decode
- 前置:一份 `binance-usdm` raw journal 与对应 seq journal;测试构建把解码器的一个字段映射改为错误映射
- 步骤:
  1. `replayer raw-decode --raw <dir> --seq <dir> --venue binance-usdm`(原解码器)
  2. 同上(改错的解码器)
- 期望:
  - 步骤 1 退出 0,无不一致;步骤 2 输出至少一条不一致,含 raw payload 与两份解码结果,格式可直接放进 `fixtures/`
- 实现:`apps/replayer/tests/raw_decode.rs::rpl_257_raw_decode_diff`

### RPL-258 verify --chain 逐字节重放 chain journal

- 规则:[`--chain`(可选扩展)](README.md#--chain可选扩展)
- 级别:replay
- 里程碑:M8
- 状态:可选扩展:chain
- 输入:chain-core 启用时录下的 `chain-bsc` 段目录(指针随该扩展立项登记在 `fixtures/replay/`)
- 期望:`replayer verify --chain --journal <dir>` 退出 0,`TxLifecycle` 迁移、钱包状态迁移与 `CmdResult` 逐字节一致;篡改一条 `CmdResult` 的副本退出 1
- 实现:`apps/replayer/tests/chain.rs::rpl_258_chain_verify`

### RPL-259 sim 起点按 hft-sim 重启恢复 seed,时间换算取 EngineClock 行

- 规则:[sim](README.md#sim)
- 级别:integ
- 里程碑:M4
- 前置:样本 S1;`--from-seq P6`;`sim-a` 在 `P6` 处的 ledger 有 `v-perp` 仓位与 1 笔挂单;journal 中 `EngineClock.tsc_hz_milli = 2 500 000 000`
- 步骤:
  1. `replayer sim --journal <S1> --from-seq P6 --out $OUT`
- 期望:
  - `SimExchange` 构造后对 `sim-a` 恰好调用一次 `seed_account_from`,其余额、仓位与订单表等于 `P6` 处按 hft-sim 重启恢复规则得到的值;`SimSetup.tsc_hz_milli = 2 500 000 000`;`sim.json` 的 `approximate = true`
- 实现:`apps/replayer/tests/sim.rs::rpl_259_seed_from_snapshot`

### RPL-260 本地文件命令实际验证并写出分歧报告

- 规则:[verify](README.md#verify)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:proc
- 里程碑:M2
- 前置:真实 sequencer 与 JournalWriter 现场生成封闭文件,两个 SnapshotNow 请求分别等待空闲轮兑现后再封段;W1 账户绑定 release registry 的首个注册 kind,参数取其 schema 默认值
- 步骤:运行生产 replayer 二进制并写报告;篡改 CmdResult 且重算 CRC;窗口结束于分歧前;恢复源数据重新验证;独立篡改后续快照状态节并重算 CRC;再损坏段头
- 期望:原文件退出 0,输入/输出/快照计数非空且来源不变;篡改退出 1,首个 seq 和字段 diff 与侧文件一致;窗口外分歧不计;成功重跑清除旧侧文件内容;状态节差异单列 payload,不误标 engine_ns;段头损坏退出 3 且无成功 JSON
- 实现:`apps/replayer/tests/cli.rs::rpl_260_real_file_verify_report_and_divergence`

### RPL-261 参数错误与未接入功能明确失败

- 规则:[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:table
- 里程碑:M2
- 前置:生产 replayer 二进制,不存在的输入路径
- 步骤:无子命令/无 journal、非法 advice/instance、反向窗口、零容量、未启用重算却给检查点、每日报告及 sim;另运行 --version
- 期望:非法及未接入功能均先于输入访问返回 2,无成功 JSON;version 返回 0 与本包版本串
- 实现:`apps/replayer/tests/cli.rs::rpl_261_invalid_and_unimplemented_options_fail_loud`

| 行 | 参数 | 期望 |
|---|---|---|
| RPL-261.1 | 无子命令 | 2 |
| RPL-261.2 | verify 缺 journal | 2 |
| RPL-261.3 | advice foo | 2 |
| RPL-261.4 | model-checkpoint a.ckpt,未给 advice recompute | 2 |
| RPL-261.5 | instance live | 2 |
| RPL-261.6 | from_seq 100,to_seq 99 | 2 |
| RPL-261.7 | max_records 0 | 2 |
| RPL-261.8 | report-dir | 2,尚未接入 |
| RPL-261.9 | sim | 2,尚未接入 |
| RPL-261.10 | --version | 0,本包版本串 |

### RPL-262 容量、实例与策略集合检查

- 规则:[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:integ
- 里程碑:M2
- 前置:真实 W1 测试策略账户的封闭文件
- 步骤:生产策略集合与测试策略集合分别经同一入口验证;分别限制记录数、逻辑体、重组为 1;指定错误实例
- 期望:生产二进制未知策略返回 4,测试集合实际验证通过;三种容量返回 3;实例不符返回 2
- 实现:`apps/replayer/tests/cli.rs::rpl_262_limits_instance_and_strategy_incompatibility`

### RPL-263 真实恢复与版本切换各自重建并严格验证

- 规则:[verify](README.md#verify)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:proc
- 里程碑:M2
- 前置:RPL-260 的真实引擎与文件写者,两次运行;恢复交接复制写者已发布的原始前缀供读者读取,最终文件由真实写者封段,不重新分配记录 seq;新运行使用两倍 TSC 频率和新校准点
- 步骤:各行现场执行恢复并写两个 SnapshotNow;生产二进制验证双段、独立第二段及只选一次运行的 from/to 窗口;篡改首段命令并重算 CRC,分别首错停止/keep-going;恢复首段后独立篡改第二段命令
- 期望:原始双段退出 0,runs 有两行且各行输入/输出数非零、快照数至少 3;来源不变,单段和窗口只计一次运行。旧段篡改退出 1,首错停止只计首段;keep-going 的第二段无分歧。新段篡改仍退出 1,只有第二段分歧。所有新段快照用途为 Recovery,Begin 原因为下表值
- 实现:`apps/replayer/tests/cli.rs::rpl_263_real_restart_runs_are_independent_and_strict`

| 行 | 恢复方式 | 新段原因 |
|---|---|---|
| RPL-263.1 | 最近快照恢复并重放尾部 | Recovery |
| RPL-263.2 | 指定尚无 EngineClock 的首个快照,跳过已含 EngineClock 的后续尾部 | RecoverFromSnapshot |
| RPL-263.3 | Drain 后更换 build_git | Recovery |

### RPL-264 恢复证据缺失、重复或越界时拒绝

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:RPL-024 的实际恢复日志;另对 CmdResult 注入差异,真实恢复生成四个账户/venue 的注记
- 步骤:独立执行下表各行
- 期望:非法证据返回 RecoveryRange,无成功结果;合法的全账户注记只报告一项 recovery_diffs,普通分歧为空
- 实现:`apps/replayer/tests/recovery.rs::rpl_264_invalid_recovery_evidence_fails_closed`

| 行 | 注记修改 |
|---|---|
| RPL-264.1 | seq 指向旧快照 |
| RPL-264.2 | seq 指向 Input |
| RPL-264.3 | seq 指向新段注记自身 |
| RPL-264.4 | seq 指向无账户 CmdResult,缺其余账户/venue 证据 |
| RPL-264.5 | 改为另一账户 |
| RPL-264.6 | v0 高位非零 |
| RPL-264.7 | v2 非零 |
| RPL-264.8 | 非 P0 |
| RPL-264.9 | pad 非零 |
| RPL-264.10 | 删除 EVENT_END |
| RPL-264.11 | 加 SHADOW 业务标志 |
| RPL-264.12 | seq 超过日志末尾 |
| RPL-264.13 | 全账户证据重复一个账户/venue |
| RPL-264.14 | 全账户证据缺一个账户/venue |

### RPL-265 恢复范围不放行更早或其他范围的输出

- 规则:[verify](README.md#verify)
- 级别:unit
- 里程碑:M2
- 前置:RPL-024 的实际恢复日志
- 步骤:分别篡改首处分歧之前的 CmdResult、旧尾部随后无账户的 CmdResult,各运行默认停止与 keep-going;另删除全部恢复注记
- 期望:篡改处均为普通首处分歧,不进 recovery_diffs;keep-going 仍验证两次运行;全部注记缺失时原始决策差异为普通分歧
- 实现:`apps/replayer/tests/recovery.rs::rpl_265_recovery_does_not_waive_other_scope_or_earlier_outputs`

### RPL-266 真实 advisor 文件重算与检查点连续性

- 规则:[verify](README.md#verify)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:integ
- 里程碑:M2
- 前置:W1 的绑定实例使用 ProbeTarget;真实 advisor 按 batch = 1 消费 12 s 行情,Advice 经环交给 LiveDriver,由真实文件写者记录;6 s 时保存实际 Model 检查点
- 步骤:泛型 CLI 分别不给检查点与给目录,生成报告;另运行默认 journal 模式和 NoModel 的原始文件
- 期望:两次重算均逐字节通过且比较量非零,有检查点的比较量更少、键数量等于文件数量;来源与检查点不变,侧文件与标准输出一致;journal 模式明确 skipped;NoModel 为 pass、比较量与 coalesced 均为 0
- 实现:`apps/replayer/tests/model.rs::rpl_266_real_advisor_files_recompute_with_and_without_checkpoints`

### RPL-267 Advice 完整头与载荷逐项篡改均定位分歧

- 规则:[verify](README.md#verify)
- 级别:table
- 里程碑:M2
- 前置:RPL-266 的真实封闭文件,每行独立修改第一条 Advice;记录带 CRC 时重算 CRC
- 步骤:运行不带检查点的 recompute
- 期望:每行退出 1、recompute = fail,first_model_divergent_seq 与首条 model_divergences 均指向被改的 Advice;字段差异非空,来源保持篡改后的字节;错路由引出的额外 Output 不吞掉下一 Input,verify 与 recompute 的结论独立
- 实现:`apps/replayer/tests/model.rs::rpl_267_advice_header_and_payload_divergences_are_reported`

| 行 | 单独修改的字段 |
|---|---|
| RPL-267.1 | strategy |
| RPL-267.2 | account |
| RPL-267.3 | local_kind |
| RPL-267.4 | group |
| RPL-267.5 | basis_seq |
| RPL-267.6 | basis_tsc |
| RPL-267.7 | payload |

### RPL-268 检查点身份、窗口、资源上限与只读保护

- 规则:[verify](README.md#verify)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:integ
- 里程碑:M2
- 前置:RPL-266 的文件和规范检查点;from_seq 位于检查点之后,窗口不含创建或丢弃注记
- 步骤:分别不给/给检查点;独立修改 kind/layout/schema/strategy/account 后重算检查点 CRC;last_seq 超过日志末尾或为 u64::MAX;破坏 CRC;将任意命名的单文件同时作为报告目标;两种诊断容量分别设为 1
- 期望:无起点逐名 skipped 且不填 coalesced;有效检查点比较量非零且从更早快照开始;身份错误退出 2、CRC/容量退出 3、无成功报告;越界实例 skipped,其他实例仍比较;报告覆盖被拒绝且检查点字节不变
- 实现:`apps/replayer/tests/model.rs::rpl_268_checkpoint_identity_origin_bounds_and_readonly_paths`

### RPL-269 检查点所需预热前缀的 Output 也严格验证

- 规则:[verify](README.md#verify)
- 级别:integ
- 里程碑:M2
- 前置:RPL-266 的检查点早于 from_seq,中间有更晚的关键帧
- 步骤:篡改预热前缀的 CmdResult,记录带 CRC 时重算 CRC,使用检查点与晚于该输出的 from_seq
- 期望:退出 1,first_output_divergent_seq 指向前缀篡改位置;Model 比较仍 pass
- 实现:`apps/replayer/tests/model.rs::rpl_269_checkpoint_prefix_outputs_are_verified_before_requested_window`

### RPL-270 完整时长的路径 Model 文件与检查点窗口

- 规则:[verify](README.md#verify)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:proc
- 里程碑:M2
- 前置:现场通过真实 LiveDriver、advisor、JournalWriter 与 SimExchange 生成 1 h 和 3 h 文件;Live 账户 acct-l、实例 tsa 使用测试策略 A,样本布局和路径版本均为 1、path_settle_ms = 120000。关键帧周期 300000 ms、事件上限 1500000,检查点周期 600000 ms;每 10 s 投递 BBO,回报按撮合器的实际截止时刻交付;CI 已构建 replay_probe example
- 步骤:独立进程对两份完整目录执行 verify/recompute;3 h 文件另取 1 h 之后的首份检查点,从 2 h 之后首个关键帧 H2 验证
- 期望:实际 engine_ns 满足 1 h/3 h,分别有 2/4 段、每小时至少 12 个快照和 720 条 Advice、非零 OrderDecision 与真实 Fill;关键帧相邻间隔不超过 310 s,检查点相邻间隔为 600–610 s(10 s 投递粒度)。全部退出 0,双 verdict 为 pass,全窗口 coalesced = 0;窗口比较数恰为 Advice.seq > H2 的条数,检查点身份和 last_seq 匹配;输入文件、seal 与检查点字节不变。现场生成文件不替代尚未发布的固定 T1/T3 指针
- 实现:`apps/replayer/tests/long_model.rs::rpl_270_full_duration_path_model_files_and_checkpoint_window`

### RPL-271 路径版本变化后的重建点来自实际引擎时间

- 规则:[verify](README.md#verify)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:proc
- 里程碑:M2
- 前置:RPL-270 的 3 h 文件和 1 h 后检查点;另一测试装配仅将 PATH_VERSION 提为 2,样本布局与 Model 算法不变
- 步骤:分别以路径版本 1、2 的独立进程从检查点执行 recompute;由 journal EngineClock 锚点和记录头独立计算检查点后首个 engine_ns ≥ L 时刻 + 120 s 的记录 R
- 期望:均退出 0、双 verdict 为 pass;同版本无重建点,版本 2 的 recompute_from_seq = R、比较数恰为 seq > L 且 basis_seq ≥ R 的 Advice 数,非零且小于同版本比较数;检查点输入字节不变
- 实现:`apps/replayer/tests/long_model.rs::rpl_271_path_version_rebuild_uses_actual_engine_time`

## 验收

门槛编号与累计量的唯一定义在 [testing §8](../../docs/testing.md#8-上线门槛);基准数值只在 [benchmarks](../../benchmarks/README.md)。

1. **用例**:里程碑 ≤ 当前里程碑的全部 `unit`/`table`/`golden`/`integ`/`proc`/`replay`/`dst`/`bench` 用例有同编号的测试函数、场景或基准,`cargo xtask test-trace` 三张清单为空([docs/tests §6](../../docs/tests/README.md#6-与代码的对应))。
2. **CI 样本**:改动策略、执行层、风控、OMS 或 sequencer 的 PR 上,RPL-010、RPL-011、RPL-040、RPL-046 的 `fixtures/replay/` 样本全部通过(作业划分归 [testing §6](../../docs/testing.md#6-ci-运行矩阵));标签作业的一致性集上 RPL-032 两种架构结果相同。
3. **DST**:RPL-012 的 PR 种子与回归集全过,每夜日期派生种子全过。
4. **每日回放**(门槛 ⑦ 的证据,M4 起):模拟账户连续运行期间与上线后,每个实例(`sim`;有运行的日子另有 `prod`——dev2 上是测试网联调实例,统一升级后是实盘实例)的每个 UTC 日都有一份每日模式报告且 `verify = "pass"`、`recompute ∈ {"pass", "skipped"}`(`skipped` 的处理归 testing §3.5 与 §8.1;联调窗口或护栏停机后由下一次运行补跑,RPL-070);RPL-079 在 dev2 上留存证据。
5. **排障**:RPL-255 通过(1 h 窗口 ≤ 10 min)。
6. **门槛 ⑩**:7 天窗口内每天都有一份 `replayer latency` 输出(RPL-190–RPL-206 的行为为前提;窗口、判定与证据格式归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法))。
7. **基准**:RPL-250、RPL-252、RPL-253 与 SIM-213 在参考机上不超过 benchmarks 登记的上限;RPL-254 在 dev2 上 `memory.peak` 不超过 `MemoryMax`。
8. **可选扩展**:RPL-256–RPL-258 带 `状态:可选扩展`,只在 `tools/xtask/test-trace.toml` 的 `enabled_extensions` 列出对应扩展后要求实现。
