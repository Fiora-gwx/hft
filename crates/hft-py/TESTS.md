---
component: hft-py
prefix: PY
spec: README.md
---

# hft-py 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | PY-001、PY-100、PY-120、PY-140、PY-141 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| Python API(导入名 `hft_py`) | PY-001–PY-011、PY-020、PY-051、PY-070、PY-085、PY-090、PY-093、PY-100、PY-104、PY-120、PY-123、PY-130、PY-140 |
| 参数语义 | PY-002、PY-020–PY-036、PY-072–PY-077、PY-085–PY-094、PY-101–PY-105 |
| 参数编码(`params`) | PY-120–PY-134、SDK-121、TOOL-122 |
| Arrow 映射 | PY-040–PY-053 |
| 错误 | PY-006、PY-009、PY-010、PY-050、PY-060–PY-065、PY-092、PY-121、PY-128 |
| 实现说明 | 不适用:见子节 |
| 1. journal → Arrow | PY-001、PY-008、PY-034、PY-036、PY-055、PY-056、PY-063、PY-070–PY-080 |
| 2. ledger / pnl | PY-100–PY-113、LED-075–LED-081、LED-246 |
| 3. 源文件 | PY-143 |
| 4. 打包与发布 | PY-150–PY-153 |
| 延迟与资源契约 | PY-080、PY-160–PY-162 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | PY-141、PY-142 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**共用夹具**(用例的 `前置` 引用这里的名字;用例依赖的参数值仍在 `前置` 里写出):

- **时间与校准**:`D = 2026-10-01T00:00:00Z` = `1_790_812_800_000_000_000` ns。全部 journal 夹具的段头 `tsc_hz_milli = 3_000_000_000_000`(3 GHz),校准对 `calib_tsc = 0` ↔ `calib_realtime_ns = D`,每秒一条与该校准对一致(无漂移)的 `ClockStatus`;因此记录头 `tsc = 3 × (t − D)` 的记录 `proc_ns = t`。下文"时刻 t 的记录"即按此写记录头。
- **夹具的生成**:下列 journal 夹具都在 pytest 会话中生成(不入库):`examples/pyfixture.rs` 产出 refdata/引擎/策略参数 blob、`TW` 脚本与批量记录文件,`TW` 写出 journal;`RUST_REF` 的读出同样由它给出(`ref`、`ledger-ref` 子命令);热进程夹具(`J_EXT`、`F_FF`、`F_SAMPLE`、`F_FILLS`)由它的 `seq` 子命令以真实 hft-sequencer 生成(见 `SEQ_FX`)。`TW` 与它由 `HFT_PY_TOOLS` 指定,否则测试先 `cargo build` 这两个 example。热目录在 `/dev/shm` 下;大于 8 MiB 的段要求 `RLIMIT_MEMLOCK` 不受限(热段 `mlock`)。
- **`RD_PY`**(refdata,经 hft-refdata 的 `RdbBuilder` 生成,写进各夹具的引导块):资产 `BTC`(`AssetId(1)`,`px_exp = −2`,`qty_exp = −8`)、`ETH`(`AssetId(2)`,`px_exp = −2`,`qty_exp = −6`)、`USDT`(`AssetId(3)`,`px_exp = −4`,`qty_exp = −6`);venue `S1`(`VenueId(2)` = `BINANCE_SPOT`,现货)、`P2`(`VenueId(5)` = `OKX_SWAP`,USDT 永续;`VenueId` 由 hft-types 常量表按名字固定);inst `101` = BTC/S1、`102` = BTC/P2、`202` = ETH/P2(inst id 须从 1 起稠密,其余 id 为 `Delisted` 的占位现货 inst);策略实例 `alpha`(`StrategyId(1)`);prod 实例的 refdata 只有账户 `A1`(`AccountId(1)`,label `bn-1`,Live,venues {S1, P2}),sim 实例的 refdata 只有账户 `A9`(`AccountId(1)`(账户 id 同样稠密),label `sim-a`,Sim,venues {S1, P2})。
- **`J_MD`**(`pyfixture md prod 60000`,prod 实例 seq journal,1 个 64 MiB 的已封存段,只有冷副本;引导块含 `RD_PY` 与一条 `HostProfile`;每整秒一条 `ClockStatus`):引导块之后,inst 101 的 `Bbo` 在 `t_k = D + 10 s + k × 10 ms`(k = 0…59 999),inst 102 的 `Bbo` 在 `t_k + 5 ms`;每条 `Bbo` 的 `ex_ts_ns = t − 2 ms`,`hw_rx_ns = t − 1 ms`(k 为偶数)或 0(k 为奇数,无 HW 戳);inst 101 的 `Trade` 在 `D + 10 s + j × 1 s`(j = 0…599);账户 A1 的 `OrderDecision` 100 条与 `OrderEvent` 100 条(`D + 20 s` 起每 1 s 一对)。`Bbo` 价格:inst 101 bid = 60 000.00(raw 6 000 000,指数 −2)、ask = 60 000.01,数量 0.5(raw 50 000 000,指数 −8)。
- **`J_SIM`**(`pyfixture md sim 1000`):sim 实例 seq journal,结构同 `J_MD`,只含账户 A9,`Bbo` 每 inst 1 000 条(k = 0…999)。
- **`J_ALL`**(`pyfixture all`):prod 实例 seq journal,2 个已封存段;`hft_event::KINDS` 中允许出现在 seq journal 的每个非 optional kind 各 ≥ 3 条(写者自己的 `SegmentRoll` 除外;变长 kind 至少一条带非空尾部,`MarginReport` 另有一条 0 个条目);一条 `ControlCmd` 拆成 3 段(`PART`);`Bbo` 覆盖 inst 101、102、202,`OrderEvent` 属账户 A1 且 `fee_asset = USDT`(`USDT` 的 `qty_exp = −6`);期望值目录 `<kind>.jsonl` 由 `pyfixture ref`(Rust `JournalReader` + 按 hft-event 字段表的读出)生成,其中 `Bbo`、`OrderDecision`、`ExternalFill`、`MarginReport`、`ConfigBlob` 五个 kind 的读出另与 hft-event 的类型化解码逐字段比对(不一致即生成失败)。
- **`SEQ_FX`**(热进程夹具的共同做法):`pyfixture seq <名> [<行>] <dir>` 以 hft-testkit 的 [`SeqRig`](../hft-testkit/README.md#10-seqrig)(真实 `Sequencer` 跑在实盘驱动上,内存环、手动 TSC、`MemJournal`)空启动:refdata = `RD_PY` prod(各 inst `l2_depth = 0`,`F_SAMPLE` 第 4 行另把 `P2` 的 `funding_settle` 设为 `BalanceOnlyPush`),`alpha` 为夹具里的脚本策略(kind 0xFF01,group `G1` = 101/S1 + 102/P2、`G2` = 202/P2;它在下一条 `Bbo` 上按脚本下 GTC 限价单),`epoch = 3`,ledger 配置为默认值但 `funding_recon_lag_ms = 1 000`,风控限额不构成约束;TSC 与校准对按"时间与校准"(引导块在 `D`),之后每条 Input 时刻 +1 ms。启动后先交出 hft-core 环境在启动时产生的同一组 Input(连接 Up、`ClockOk`、账户配置与对账汇总、kill 自检、余额与保证金、三个 inst 的报价与 102/202 的标记价、`G1`/`G2` 的 `SetMode{Live}`),再按夹具交出各 Input;journal 记录原样转成 `TW` 脚本(引导块 → `boot`,其余 → `bulk`,每个事件以 `end` 结束)写出一个封存段,`pyfixture seq-check` 再逐条比对写出的 journal 与 `MemJournal`(seq、kind、`EVENT_END`、记录体;在两个事件之间追加的 Meta 记录,写者给它置 `EVENT_END`,不比该位)。夹具在每个具名步骤之后保存热进程的 ledger 节 1、13、22 字节(`expected/ledger-<步骤>.bin`)与它的 `positions()`/`inventory()`/`portfolio()` 读数及自启动起的 `PnlBreakdown`(`steps.json`,含该步骤 Input 的 seq 与 `proc_ns`)。"与热进程一致"= `hft_py.ledger(j, account="bn-1", t1 = 该步骤 Input 的 proc_ns + 1)` 的 `_testing.ledger_snapshot` 与该步骤的三节字节逐字节相同,四个读数逐值相同;`portfolio` 的 `flags` 不比 `BALANCE_UNRECONCILED`(冷读者不做静默期余额校验,[hft-ledger 冷消费者](../hft-ledger/README.md#冷消费者journalledger))。
- **`J_EXT`**(`pyfixture seq ext`,`SEQ_FX`,账户 A1):初始状态同 LED-055——策略在 `G1` 买入 `BTC/P2` 10.0 的 GTC 单经 `Ack` 与 `Fill` 全部成交于 60 000,之后 `BTC/P2` 标记价 60 500,P2 一轮无差异的对账(只有汇总条目;它的 `view_seq` 在用户流 Up 之后,`AUTO_EXTERNAL` 的前置条件)。事件 E1 = 对账报告 4242 的末条 Input(汇总条目;同组首条是 `BTC/P2` 的仓位条目:`engine_qty = 10.0`、`exch_qty = 6.0`、`AUTO_EXTERNAL`)+ 热进程按 `auto_external_fill` 合成入账并写出的 `Note{ExternalFill}`(`aflags = AUTO_RECON`,同 LED-076 步骤 1 的注记)等;事件 E2 = Input `ExternalFill{Adl, Sell, BTC/P2, 1.0, trade_id 77}`(补推的同向 ADL)+ 热进程写出的 `Note{LedgerAutoHeldOffset}`;事件 E3 = Input `OrderEvent{Fill, slot 9(本次运行从未用过), gen 1, detail = Buy, cum_filled 0.3, px 60 100, fee 0.09 USDT, exch_order_id 99}`(`StaleGen` 带成交)+ `Note{OmsForeignFill}`(`path = 2`、`trade_id = 0`、`qty = 0.3`、`px = 60 100`、`fee = 0.09`、`exch_order_id = 99`、`cum_filled = fill_seq = 0.3`);事件 E4 = Input `ExternalFill{Liquidation, Buy, BTC/P2, 0.4, trade_id 910}` + `Note{ExternalFill}`(`aflags = 0`)。具名步骤 E0(E1 之前)–E4。
- **`F_FF`**(`pyfixture seq ff <行>`,PY-109 各行)、**`F_SAMPLE`**(`pyfixture seq sample <行>`,PY-111 各行)、**`F_FILLS`**(`pyfixture seq fills <行>`,PY-112 各行):`SEQ_FX`,账户 A1,各行的 Input 见用例表;具名步骤 `start`/`placed`(各行的 Input 之前)、中间步骤与 `end`(最后一条 Input 之后)。
- **`J_LED20`**:`pyfixture led <种子>`(种子 1–20)的 20 个 journal 夹具:账户 A1、`alpha` 的 group `G1`(`0x0100_0001`,101/S1 + 102/P2)与 `G2`(`0x0100_0002`,202/P2);下单(`OrderDecision{New}`)与分次成交、永续上的外部成交、资金费推送、余额、BBO 与标记价,约 30 h(跨 UTC 日);3 个段,每段引导块重述配置并带当前账本的 `StateSnapshot{Boot}`(含 hft-oms 节 2 的未完成订单行),段 2、段 3 各有一条关键帧 `StateSnapshot{Keyframe}`。Rust 参照由 `pyfixture ledger-ref`(hft-ledger `JournalLedger` 从同一起点逐事件消费)给出每事件各 group 的 `PnlBreakdown` 增量与最终视图、三节快照字节。
- **`RUST_REF`**:Rust 参考读法 = `hft_journal::JournalReader`(`StartAt::Oldest`、`EndAt::Follow`、`OnGap::Error`)+ `hft_event` 解码 + [hft-ledger 冷消费者](../hft-ledger/README.md#冷消费者journalledger) `JournalLedger`(hft-ledger 测试中的同名驱动)。比较 hft-py 输出时一律以它为参照。
- **`TW`**(测试写者):hft-journal 的测试写者 `examples/testwriter.rs`(`required-features = ["writer"]`,只在测试中以子进程运行,不是 hft-py 的依赖),本文件用它的 `--script` 模式([hft-journal 测试写者](../hft-journal/README.md#读者)):命令从文件或 stdin 读,可交错写入、发布、滚动、暂停 persist、`abort` 与输出单调时钟标记;记录头 `tsc` 与段头校准对按"时间与校准"夹具写。参数:源记录(`J_MD` 或合成 `Bbo`)、热目录(tmpfs 下的临时目录 `<tmp>/hot/prod/seq/`)、冷目录 `<tmp>/cold/prod/seq/`、每事件一条记录、每 `publish_every` 个事件 `publish` 一次、`segment_bytes`、persist 同步间隔。默认用于本文件:`segment_bytes = 8 MiB`,`publish_every = 1`,persist `sync_interval_ms = 50`。"合成 `Bbo`"= inst 101,bid 60 000.00(raw 6 000 000)、ask 60 000.01(raw 6 000 001)、`bid_qty`/`ask_qty` 0.5(raw 50 000 000),第 i 条(从 0 起)的时刻 `D + 10 s + i × 1 ms`,`ex_ts_ns` 与 `hw_rx_ns` 同 `J_MD` 的规则。
- **`HB`**:在临时目录 `<tmp>/shm/` 下以 `HeartbeatBlock::create(<tmp>/shm/prod/heartbeat, 1)` 建块(hft-rt;建成后 `state = Starting`、`n_slots = 0`),测试按需 `set_state`、`sequencer().beat()`;hft-py 以 `root = "<tmp>/shm"` 读取。
- **`S_TP`**(schema 目录,测试时生成):`fixtures/params/sdk-tp/schema.golden.json`(SDK 的 `EchoProbe`:kind_id 0xFF01 = 65281,`TP` schema `schema_id = 65281`、`version = 2`,字段 `open_th`(Rate,per_group)、`cooldown_ms`(DurationMs)、`pct_bin`(Rate,per_group,structural)、`max_qty`(Qty,per_group)、`enabled`(Bool,manual_only)、`mode`(Enum a/b/c,since 2);`cmds`:op 1 `open`(`notional`: Notional 0..=10 000 000 000)、op 2 `close`(无字段,`reduce_only`)、op 3 `set_mode`(`m`: Enum x/y);取值见 [hft-strategy-sdk TESTS](../hft-strategy-sdk/TESTS.md) 的 `TP` 夹具)复制为 `65281-<name>.schema.json`,并写 `index.json` = `[{"kind_id":65281,"name":<name>,"file":"65281-<name>.schema.json","schema_id":65281,"schema_version":2,"schema_digest":<金样记录的摘要>}]`。`S_TP2` 另加一项 kind_id 0xFF02(`TargetProbe`,`cmds` 为空)。
- **`GT3_PY`**(`encode` 的 `groups` 输入):3 项,`id` = `0x0300_0001`、`0x0300_0002`、`0x0300_0003`,`asset = 1`,`open_allowed = True`,`legs = [{inst: 101, venue: 2, kind: "Cex"}, {inst: 102, venue: 5, kind: "Cex"}]`,`qty_exp = −8`,`px_exp = −2`;`header = {strategy: 3, account: 9, version: 7, source: "Pipeline", generated_at_ns: D}`。

### PY-001 带 hot_root 打开时读到 RAM 热段中尚未同步到冷副本的尾部

- 规则:[Python API](README.md#python-api导入名-hft_py)、[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:integ
- 里程碑:M2
- 前置:`TW` 以 `J_MD` 的前 2 000 条 `Bbo`(inst 101)写热目录 `<tmp>/hot/prod/seq/`,persist 已把前 1 500 条同步到冷目录 `<tmp>/cold/prod/seq/`,之后暂停 persist(`synced_len` 停在第 1 500 条之后)
- 步骤:
  1. `j = hft_py.open_journal("<tmp>/cold/prod/seq", hot_root="<tmp>/hot")`
  2. `df = pl.DataFrame(j.events("Bbo", inst=101))`
- 期望:
  - `df.height == 2000`;`df["seq"]` 严格递增、相邻差 1;最后一行的 `seq` = `RUST_REF` 在热段上读到的最后一条 `Bbo` 的 seq
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_001_hot_root_reads_unsynced_tail`

### PY-002 省略 hot_root 时只读冷副本,看不到未同步的尾部

- 规则:[参数语义](README.md#参数语义)
- 级别:integ
- 里程碑:M2
- 前置:同 PY-001(热段 2 000 条,冷副本同步到第 1 500 条)
- 步骤:
  1. `j = hft_py.open_journal("<tmp>/cold/prod/seq")`
  2. `pl.DataFrame(j.events("Bbo", inst=101)).height`
- 期望:
  - 步骤 2 返回 1500;`j.info()["last_seq"]` = 冷副本中最后一条记录的 seq(小于热段的 `last_seq`)
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_002_no_hot_root_cold_only`

### PY-003 单个段文件路径只读该段

- 规则:[Python API](README.md#python-api导入名-hft_py)
- 级别:unit
- 里程碑:M2
- 前置:`J_ALL` 的第 2 个段文件路径 `p2`(`<…>/<yyyymmdd>/0000000002-<first_seq>.jrn`)
- 步骤:
  1. `j = hft_py.open_journal(p2)`
  2. `j.info()`
- 期望:
  - `info()["segments"]` 恰好 1 项,其段号为 2;`first_seq` = 段 2 段头的 `first_seq`;`j.events(k)` 对每个 kind 返回的行全部落在段 2 的 seq 范围内
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_003_single_segment_file`

### PY-004 info() 的每个键

- 规则:[Python API](README.md#python-api导入名-hft_py)
- 级别:table
- 里程碑:M2
- 前置:`j = hft_py.open_journal("<J_ALL>/…/prod/seq")`;期望值取自 `RUST_REF` 读段头
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_004_info_keys`

| 行 | 键 | 期望 |
|---|---|---|
| PY-004.1 | 键集合 | 恰为 `{"journal_name", "instance", "segments", "first_seq", "last_seq", "schema", "host_profile"}` |
| PY-004.2 | `journal_name` | `"seq"` |
| PY-004.3 | `instance` | `"prod"` |
| PY-004.4 | `segments` | 2 项,按段号升序,每项带段号 `segment_no`、`first_seq`、冷副本路径 `path`(另有 `hot_path`、`sealed`) |
| PY-004.5 | `first_seq` / `last_seq` | 段 1 的 `first_seq` / 段 2 最后一条记录的 seq |
| PY-004.6 | `schema` | 与 `hft_py.schema_hash()` 的 kind 表一致时为空的差异列表 |
| PY-004.7 | `host_profile` | 段 2 引导块 `HostProfile` 记录的字段(含清单哈希) |

### PY-005 路径中的实例段不是 prod 或 sim 时拒绝

- 规则:[Python API](README.md#python-api导入名-hft_py)
- 级别:table
- 里程碑:M2
- 前置:把 `J_MD` 的冷目录分别复制到下表路径
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_005_instance_name`

| 行 | 路径 | 期望 |
|---|---|---|
| PY-005.1 | `<tmp>/prod/seq` | 打开成功,`info()["instance"] == "prod"` |
| PY-005.2 | `<tmp>/sim/seq` | 打开成功,`info()["instance"] == "sim"` |
| PY-005.3 | `<tmp>/test/seq` | `ValueError` |
| PY-005.4 | `<tmp>/Prod/seq` | `ValueError` |

### PY-006 路径不存在或没有任何段时抛 JournalError 并带路径

- 规则:[错误](README.md#错误)
- 级别:table
- 里程碑:M2
- 前置:空临时目录 `<tmp>/prod/seq/`;不存在的 `<tmp>/prod/nope`
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_006_missing_path`

| 行 | 输入 | 期望 |
|---|---|---|
| PY-006.1 | `open_journal("<tmp>/prod/nope")` | `hft_py.JournalError`,`e.path == "<tmp>/prod/nope"` |
| PY-006.2 | `open_journal("<tmp>/prod/seq")`(空目录) | `hft_py.JournalError`,`e.path` 为该目录 |
| PY-006.3 | `open_journal("<tmp>/prod/seq/0000000001-…jrn")`(文件不存在) | `hft_py.JournalError` |

### PY-007 只读挂载的 journal 目录可读,读后文件不变

- 规则:[1. journal → Arrow](README.md#1-journal--arrow)、[依赖](README.md#依赖)
- 级别:integ
- 里程碑:M2
- 前置:`J_ALL` 复制到 `<tmp>/ro/`,目录权限 0555、文件 0444(CI 中另以 `mount --bind -o ro` 再挂一次);记下全部文件的 `(size, mtime, sha256)` 与目录项清单
- 步骤:
  1. 打开并对每个 kind 调 `events()` 读完
  2. 再次列出目录与文件元数据
- 期望:
  - 步骤 1 无异常,行数与 `RUST_REF` 相同
  - 步骤 2 的目录项清单与全部 `(size, mtime, sha256)` 与步骤前相同(没有新建、改写或删除任何文件)
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_007_readonly_mount`

### PY-008 读取过程中热段被退役删除时在同一偏移切到冷副本,不丢不重

- 规则:[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:integ
- 里程碑:M2
- 前置:`TW` 写 3 个段(`segment_bytes = 8 MiB`),persist 已同步并封存段 1、2;`j = open_journal(cold, hot_root=hot)`;`rdr = j.events("Bbo", batch_rows=1000)` 已取出第一批
- 步骤:
  1. 删除热目录中的段 1 文件(模拟 `HotRetirer` 退役)
  2. 读完 `rdr`
- 期望:
  - 合并后的 `seq` 列严格递增、相邻差 1、首尾与 `RUST_REF` 相同;总行数 = 写入的 `Bbo` 数
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_008_hot_retired_switch_to_cold`

### PY-009 热段与冷副本都被删除时抛 JournalError

- 规则:[错误](README.md#错误)
- 级别:integ
- 里程碑:M2
- 前置:同 PY-008 的 3 个段;删除段 2 的热段与冷副本
- 步骤:
  1. `list(j.events("Bbo"))`
- 期望:
  - 交付完段 1 的全部批后抛 `hft_py.JournalError`;`e.seq` = 段 2 的 `first_seq`(期望的下一条 seq),`e.path` 含段 2 的冷副本路径
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_009_both_copies_deleted`

### PY-010 段链断裂(中间段缺失)时抛 JournalError 并带期望 seq

- 规则:[错误](README.md#错误)
- 级别:unit
- 里程碑:M2
- 前置:`TW` 生成 3 段夹具 `J3`(全部封存,只保留冷副本),再删除段 2 的冷副本与 `.seal`
- 步骤:
  1. `j = open_journal(J3)`;`list(j.events("Bbo"))`
- 期望:
  - `open_journal` 或第一次读取抛 `hft_py.JournalError`;`e.seq` = 段 2 的 `first_seq`;没有任何段 3 的行被交付
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_010_chain_broken`

### PY-011 prod 与 sim 两个实例分别打开,账户与记录互不混入

- 规则:[Python API](README.md#python-api导入名-hft_py)
- 级别:unit
- 里程碑:M2
- 前置:`J_MD`(prod,账户 A1)与 `J_SIM`(sim,账户 A9)
- 步骤:
  1. `jp = open_journal(J_MD)`,`js = open_journal(J_SIM)`
  2. 各读 `events("OrderEvent")`
- 期望:
  - `jp.info()["instance"] == "prod"`,`js.info()["instance"] == "sim"`
  - `jp` 的 `OrderEvent` 共 100 行,全部 `account == 1` 且 `account_kind == "Live"`;`js` 的 `OrderEvent` 全部 `account == 1`(sim 实例 refdata 中的 A9)且 `account_kind == "Sim"`;两者的 `seq` 各自从本实例 journal 的段 1 起递增
- 实现:`crates/hft-py/tests/python/test_open.py::test_py_011_two_instances`

列名以 `python/_generated/`(gen-schema 输出)为准;下文按字段名指代列(`MarketHdr` 的嵌套字段写叶子名,如 `inst`、`ex_ts_ns`)。

### PY-020 kind 名与编号得到相同结果

- 规则:[Python API](README.md#python-api导入名-hft_py)、[参数语义](README.md#参数语义)
- 级别:unit
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`
- 步骤:
  1. `a = pl.DataFrame(j.events("Bbo"))`
  2. `b = pl.DataFrame(j.events(0x0101))`
- 期望:
  - `a.height == b.height == 120000`;`a.equals(b)` 为真(列名、类型与值全同)
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_020_kind_name_and_number`

### PY-021 未知 kind 抛 ValueError

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_021_unknown_kind`

| 行 | `kind` | 期望 |
|---|---|---|
| PY-021.1 | `"Bbo2"` | `ValueError` |
| PY-021.2 | `0x01FF`(未注册编号) | `ValueError` |
| PY-021.3 | `-1` | `ValueError` |
| PY-021.4 | `0x10000` | `ValueError` |
| PY-021.5 | `"Bbo"` | 不抛异常(对照行) |

### PY-022 inst 过滤

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`;kind 见各行
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_022_inst_filter`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-022.1 | `events("Bbo", inst=101)` | 60 000 行,`inst` 全为 101 |
| PY-022.2 | `events("Bbo", inst=[101, 102])` | 120 000 行 |
| PY-022.3 | `events("Bbo", inst=None)` | 120 000 行 |
| PY-022.4 | `events("Bbo", inst=202)` | 0 行;reader 的 schema 与 PY-022.1 相同 |
| PY-022.5 | `events("ClockStatus", inst=101)` | `ValueError`(该 kind 无 inst 字段) |
| PY-022.6 | `events("ClockStatus")` | 行数 = journal 中 `ClockStatus` 条数(对照行) |

### PY-023 t0、t1 的三种写法得到相同结果

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`;每行调用 `events("Bbo", inst=101, t0=…, t1=…)`;窗口为 `[D + 10 s, D + 20 s)`,期望 1 000 行(k = 0…999)
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_023_time_forms`

| 行 | `t0` / `t1` 写法 | 期望 |
|---|---|---|
| PY-023.1 | ns 整数 `1_790_812_810_000_000_000` / `1_790_812_820_000_000_000` | 1 000 行,首行 `proc_ns = D + 10 s` |
| PY-023.2 | `datetime(2026,10,1,0,0,10,tzinfo=UTC)` / `datetime(2026,10,1,0,0,20,tzinfo=UTC)` | 与 PY-023.1 相同 |
| PY-023.3 | `"2026-10-01T00:00:10Z"` / `"2026-10-01T00:00:20Z"` | 与 PY-023.1 相同 |
| PY-023.4 | `"2026-10-01T08:00:10+08:00"` / `"2026-10-01T08:00:20+08:00"` | 与 PY-023.1 相同 |
| PY-023.5 | t0 用 ns 整数、t1 用 ISO 字符串 | 与 PY-023.1 相同 |

### PY-024 无时区的时刻一律拒绝

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`;`t1 = "2026-10-01T00:00:20Z"`,各行只改 `t0`
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_024_naive_time_rejected`

| 行 | `t0` | 期望 |
|---|---|---|
| PY-024.1 | `datetime(2026,10,1,0,0,10)`(无 tzinfo) | `ValueError` |
| PY-024.2 | `"2026-10-01T00:00:10"` | `ValueError` |
| PY-024.3 | `"2026-10-01"` | `ValueError` |
| PY-024.4 | `"10/01/2026 00:00:10 UTC"` | `ValueError` |

### PY-025 半开区间 [t0, t1) 的边界

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`,`events("Bbo", inst=101, time="proc", …)`;记 `t = D + 10 s + 100 ms`(k = 10 的记录),相邻记录在 `t − 10 ms`(k = 9)与 `t + 10 ms`(k = 11)
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_025_half_open_bounds`

| 行 | `t0` | `t1` | 期望 |
|---|---|---|---|
| PY-025.1 | `t` | `t + 1 ns` | 1 行,k = 10(t0 含) |
| PY-025.2 | `t + 1 ns` | `t + 10 ms` | 0 行(t1 不含) |
| PY-025.3 | `t − 1 ns` | `t` | 0 行 |
| PY-025.4 | `t − 10 ms` | `t` | 1 行,k = 9 |
| PY-025.5 | `t − 10 ms` | `t + 1 ns` | 2 行,k = 9、10 |

### PY-026 t0 大于 t1 拒绝,相等为空

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`,`events("Bbo", inst=101, …)`;`t = D + 10 s + 100 ms`
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_026_t0_after_t1`

| 行 | `t0` / `t1` | 期望 |
|---|---|---|
| PY-026.1 | `t + 1 ns` / `t` | `ValueError` |
| PY-026.2 | `t` / `t` | 0 行,不抛异常 |
| PY-026.3 | `t` / `t + 1 ns` | 1 行(对照) |

### PY-027 time 的三种来源各自过滤,字段为 0 的记录不出现

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`,`events("Bbo", inst=101, t0 = D + 10 s, t1 = D + 10 s + 40 ms, time = …)`;记录时刻 `t_k = D + 10 s + k × 10 ms`,`ex_ts_ns = t_k − 2 ms`,`hw_rx_ns = t_k − 1 ms`(k 偶数)或 0(k 奇数)
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_027_time_sources`

| 行 | `time` | 期望 |
|---|---|---|
| PY-027.1 | `"proc"` | 4 行,k = 0、1、2、3 |
| PY-027.2 | `"ex"` | 4 行,k = 1、2、3、4 |
| PY-027.3 | `"hw"` | 2 行,k = 2、4(k = 1、3 的 `hw_rx_ns = 0`,不出现,也不按 `sw_rx_ns` 或 `proc` 补入) |

### PY-028 time 对没有该字段的 kind 或未知取值拒绝

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_028_time_errors`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-028.1 | `events("ClockStatus", time="ex")` | `ValueError` |
| PY-028.2 | `events("ClockStatus", time="hw")` | `ValueError` |
| PY-028.3 | `events("Bbo", time="utc")` | `ValueError` |
| PY-028.4 | `events("ClockStatus", time="proc")` | 不抛异常(对照) |

### PY-029 account 按名字与 AccountId 过滤,结果带 account_kind

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`jp = open_journal(J_MD)`(refdata 只含 A1),`js = open_journal(J_SIM)`(refdata 只含 A9);kind `OrderEvent`
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_029_account_filter`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-029.1 | `jp.events("OrderEvent", account="bn-1")` | 100 行;`account` 全为 1;`account_kind` 全为 `"Live"` |
| PY-029.2 | `jp.events("OrderEvent", account=1)` | 与 PY-029.1 相同 |
| PY-029.3 | `js.events("OrderEvent", account="sim-a")` | 行数 = `J_SIM` 中 A9 的 `OrderEvent` 条数;`account_kind` 全为 `"Sim"` |
| PY-029.4 | `js.events("OrderEvent", account=1)`(A9 的 `AccountId`) | 与 PY-029.3 相同 |

### PY-030 account 用错时拒绝

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`jp = open_journal(J_MD)`
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_030_account_errors`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-030.1 | `events("Bbo", account="bn-1")` | `ValueError`(该 kind 无 account 字段) |
| PY-030.2 | `events("OrderEvent", account="nobody")` | `ValueError` |
| PY-030.3 | `events("OrderEvent", account="sim-a")`(prod 实例的 refdata 不含 A9) | `ValueError` |
| PY-030.4 | `events("OrderEvent", account=200)`(未定义的 id) | `ValueError` |

### PY-031 columns 只物化给出的列并按给出顺序

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_031_columns`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-031.1 | `events("Bbo", columns=["seq", "bid_px"])` | 列名恰为 `["seq", "bid_px"]` |
| PY-031.2 | `events("Bbo", columns=["bid_px", "seq"])` | 列名恰为 `["bid_px", "seq"]` |
| PY-031.3 | `events("Bbo", columns=["nope"])` | `ValueError` |
| PY-031.4 | `events("Bbo", columns=["seq", "seq"])` | `ValueError` |

### PY-032 decimals 默认不出浮点列,开启时按资产指数换算

- 规则:[参数语义](README.md#参数语义)、[Arrow 映射](README.md#arrow-映射)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`;inst 101 首条 `Bbo`:bid 60 000.00(raw 6 000 000,指数 −2)、ask 60 000.01(raw 6 000 001)、`bid_qty` 0.5(raw 50 000 000,指数 −8)
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_032_decimals`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-032.1 | `events("Bbo", inst=101)` | 没有以 `_f` 结尾的列 |
| PY-032.2 | `events("Bbo", inst=101, decimals=True)` 首行 | `bid_px_f == 60000.0`、`ask_px_f == 60000.01`、`bid_qty_f == 0.5`,类型 `float64`;`bid_px == 6000000`(int64 列不变) |
| PY-032.3 | `events("ClockStatus", decimals=True)` | 没有 `_f` 列,不抛异常 |
| PY-032.4 | `events("Bbo", inst=101, decimals=False)` | 同 PY-032.1 |

### PY-033 batch_rows 默认值为 1 000 000

- 规则:[参数语义](README.md#参数语义)
- 级别:unit
- 里程碑:M2
- 前置:`TW` 以合成 `Bbo`(inst 101)写 1 000 001 条到单个段(`segment_bytes = 256 MiB`)并封存;不传 `batch_rows`
- 步骤:
  1. `[b.num_rows for b in j.events("Bbo")]`
- 期望:
  - 结果为 `[1000000, 1]`
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_033_batch_rows_default`

### PY-034 每批不超过 batch_rows,且在段尾结束

- 规则:[参数语义](README.md#参数语义)、[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:table
- 里程碑:M2
- 前置:`TW` 以合成 `Bbo` 写入各行给出的条数;`events("Bbo", batch_rows=1000)`
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_034_batch_bounds`

| 行 | journal 内容 | 期望各批行数 |
|---|---|---|
| PY-034.1 | 1 个段,999 条 | `[999]` |
| PY-034.2 | 1 个段,1 000 条 | `[1000]` |
| PY-034.3 | 1 个段,1 001 条 | `[1000, 1]` |
| PY-034.4 | 2 个段,各 600 条 | `[600, 600]`(段尾结束一批) |
| PY-034.5 | 0 条 `Bbo` | `[]`(无批,不抛异常) |

### PY-035 batch_rows 小于 1 拒绝

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_035_batch_rows_invalid`

| 行 | `batch_rows` | 期望 |
|---|---|---|
| PY-035.1 | `0` | `ValueError` |
| PY-035.2 | `-1` | `ValueError` |
| PY-035.3 | `1` | 120 000 批,每批 1 行 |

### PY-036 惰性按批产出,已产出的批不依赖 journal 映射

- 规则:[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:integ
- 里程碑:M2
- 前置:`TW` 写 3 个封存段(只保留冷副本,在 `<tmp>/cold`),每段 600 条 `Bbo`;再把段 3 第 1 条记录头的 `len` 改成越过段尾的值
- 步骤:
  1. `rdr = open_journal(cold).events("Bbo", batch_rows=1000)`;`b1 = next(rdr)`;`b2 = next(rdr)`
  2. `next(rdr)`
  3. 删除全部段文件后计算 `b1["bid_px"]` 的总和
- 期望:
  - 步骤 1 得到段 1、段 2 的两批(各 600 行),不抛异常(段 3 的损坏在读到它之前不被检测)
  - 步骤 2 抛 `hft_py.JournalError`,`e.seq` = 段 3 的 `first_seq`
  - 步骤 3 返回与删除前相同的总和(600 × 6 000 000)
- 实现:`crates/hft-py/tests/python/test_events.py::test_py_036_lazy_batches_detached`

### PY-040 全部 kind 的 events() 结果与 Rust 读法逐字段相同

- 规则:[Arrow 映射](README.md#arrow-映射)、[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:golden
- 里程碑:M2
- 输入:`J_ALL`;对 `hft_event::KINDS` 中每个允许出现在 seq journal 的非 optional kind 调 `events(kind)`
- 期望文件:`fixtures/replay/hft-py/all-kinds/expected/<kind>.jsonl`(`RUST_REF` 逐条解码:记录头 `seq`、`tsc`、`flags` 与体的每个字段;变长 kind 另含原始体的十六进制)
- 前置:比较时 `Px`/`Qty` 取 int64 原值,`proc_ns` 另由 PY-042 检查;期望文件中 `Bbo`、`OrderDecision`、`ExternalFill`、`MarginReport`、`ConfigBlob` 的值经人工按 [hft-event](../hft-event/README.md) 布局核对
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_040_all_kinds_golden`

### PY-041 任意合法记录体经转置后逐字段等于 hft-event 解码

- 规则:[Arrow 映射](README.md#arrow-映射)、[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:prop
- 里程碑:M2
- 性质:对任意 kind 与该 kind 的任意合法体(hft-event 编码),`arrow_map` 转置得到的一行与 `hft_event` 解码的字段逐个相等;一批中各行的顺序与输入顺序相同
- 生成:kind 从 `KINDS` 均匀抽取;每批 1–2 048 条同 kind 记录,字段值取该类型的全域(枚举只取合法判别值,填充为 0);变长 kind 的尾部 0–64 个条目
- 参考:`hft_event` 的 `read`/`var` 解码结果
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-py/tests/arrow_map.rs::py_041_transpose_matches_decode`

### PY-042 每张表都带 seq、tsc、proc_ns、flags,proc_ns 按 TSC → UTC 换算

- 规则:[Arrow 映射](README.md#arrow-映射)、[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`;校准见"时间与校准"夹具(`proc_ns = t`)
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_042_header_columns`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-042.1 | `events("Bbo", inst=101)` 首行 | 含列 `seq`、`tsc`、`proc_ns`、`flags`;`tsc = 30_000_000_000`、`proc_ns = 1_790_812_810_000_000_000` |
| PY-042.2 | `events("Bbo", inst=102)` 首行 | `proc_ns = 1_790_812_810_005_000_000` |
| PY-042.3 | `events("ClockStatus")` | 同样含这四列 |
| PY-042.4 | `events("Bbo", inst=101)` 全部行 | `proc_ns − 1_790_812_810_000_000_000` = 行号 × 10 000 000 |

### PY-043 Px/Qty 为 int64 原值,元数据给出按 inst 的指数

- 规则:[Arrow 映射](README.md#arrow-映射)
- 级别:table
- 里程碑:M2
- 前置:`J_ALL`(含 inst 101、102、202 的 `Bbo`、`Trade` 与账户 A1 的 `OrderEvent`,`OrderEvent.fee_asset = USDT`、`USDT` 的 `qty_exp = −6`)
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_043_fixed_point_metadata`

| 行 | 列 | 期望 |
|---|---|---|
| PY-043.1 | `Bbo.bid_px` | Arrow 类型 `int64`;元数据 `px_exp` = `{"101": -2, "102": -2, "202": -2}`(refdata 中非 `Delisted` 的 inst) |
| PY-043.2 | `Bbo.bid_qty` | `int64`;`qty_exp` = `{"101": -8, "102": -8, "202": -6}` |
| PY-043.3 | `events("Bbo", inst=202).bid_qty` | `qty_exp` = `{"202": -6}` |
| PY-043.4 | `OrderEvent.fee` | `int64`;元数据 `exp_asset_col = "fee_asset"`,`qty_exp_by_asset` = `{"1": -8, "2": -6, "3": -6}`(refdata 的全部资产) |
| PY-043.5 | `Bbo.update_id` | `uint64`,无 `px_exp`/`qty_exp` 元数据 |

### PY-044 六个变长 kind 给原始体 binary 列,decode 函数与 Rust 解码一致

- 规则:[Arrow 映射](README.md#arrow-映射)
- 级别:table
- 里程碑:M2
- 前置:`J_ALL`;每行取该 kind 的全部行,对 `body` 列逐行调用对应解码函数,与 `RUST_REF` 的解码结果比较;`RawRx`/`RawTx` 只出现在 raw journal(seq journal 的读者拒绝它们),这两行改为对按 `python/_generated` 的 `RAW_RX` dtype 编码的逻辑体(`venue = 2`、`payload_len = 5`、载荷 `b"hello"`)调用解码函数
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_044_var_kinds_binary`

| 行 | kind | 解码函数 | 期望 |
|---|---|---|---|
| PY-044.1 | `ControlCmd` | `hft_py.decode.control_cmd` | binary 列 = 重组后的逻辑体字节;解码出的 `CmdHdr` 字段与载荷字节与 Rust 相同 |
| PY-044.2 | `ConfigBlob` | `hft_py.decode.config_blob` | `BlobHdr` 字段(`kind`、`encoding`、`boot`、`sha256`、`len`)相同,`payload` 长度 = `len` 且 SHA-256 = `sha256` |
| PY-044.3 | `StateSnapshot` | `hft_py.decode.state_snapshot` | 前缀字段相同,节表字节相同 |
| PY-044.4 | `Advice` | `hft_py.decode.advice` | `AdviceHdr` 字段相同,`payload` 字节相同 |
| PY-044.5 | `RawRx` | `hft_py.decode.raw_rx` | `venue == 2`、`payload_len == 5`、`payload == b"hello"` |
| PY-044.6 | `RawTx` | `hft_py.decode.raw_tx` | 同 PY-044.5 |
| PY-044.7 | `ConfigBlob` | `hft_py.decode.config_blob(b[:-1])`(截掉 1 字节) | `ValueError` |

### PY-045 Advice 给出 AdviceHdr 字段列与原始载荷

- 规则:[Arrow 映射](README.md#arrow-映射)
- 级别:unit
- 前置:`J_ALL` 中一条 `Advice`:`AdviceHdr{strategy = 3, account = 0, local_kind = 1, group = 0x0300_0001, len = 16, basis_seq = 1000, basis_tsc = 3_000_000_000}`,载荷 16 B `0x01 00 00 00 00 00 00 00` + `band = 12345`(i64 小端)
- 里程碑:M2
- 步骤:
  1. `pl.DataFrame(j.events("Advice"))` 取该行
- 期望:
  - `AdviceHdr` 的每个字段各为一列,值同上;载荷列为 16 字节且与上面的字节逐字节相同;不存在按载荷内容展开的列
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_045_advice_hdr_and_payload`

### PY-046 其余变长 kind 的前缀展开为列,尾部条目为 list<struct>

- 规则:[Arrow 映射](README.md#arrow-映射)
- 级别:table
- 里程碑:M2
- 前置:`J_ALL` 中各 kind 的第一条带非空尾部的记录;期望值来自 `RUST_REF`
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_046_var_prefix_and_list`

| 行 | kind | 期望 |
|---|---|---|
| PY-046.1 | `L2Batch` | 前缀 `L2BatchHdr` 每字段一列;条目列类型 `list<struct>`,元素个数 = 前缀给出的条目数,逐元素与 Rust 相同 |
| PY-046.2 | `L2Snapshot` | 前缀 `L2SnapHdr` 每字段一列;条目列元素个数 = `n_bids + n_asks`,顺序与记录相同 |
| PY-046.3 | `MarginReport` | 前缀每字段一列;每 inst 条目(含 `liq_px`)为 `list<struct>`,逐元素与 Rust 相同 |
| PY-046.4 | `AccountConfigReport` | 前缀每字段一列;每 inst 杠杆条目为 `list<struct>`,逐元素与 Rust 相同 |
| PY-046.5 | `MarginReport`,尾部 0 个条目的记录 | 条目列为空列表,不是 null |

### PY-047 多段 ControlCmd 重组为一行

- 规则:[Arrow 映射](README.md#arrow-映射)
- 级别:unit
- 里程碑:M2
- 前置:`J_ALL` 中拆成 3 段(`PART`)的 `ControlCmd`,第 0 段 seq = s
- 步骤:
  1. `df = pl.DataFrame(j.events("ControlCmd"))`
- 期望:
  - 该命令只出现 1 行,`seq = s`;binary 列长度 = 3 段 `chunk_len` 之和,与 `RUST_REF` 的 `LogicalEvent.body` 逐字节相同;df 中没有 seq 为 s + 1、s + 2 的行
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_047_multipart_one_row`

### PY-048 新读者读旧段:缺失字段补零并置 _missing 位

- 规则:[Arrow 映射](README.md#arrow-映射)
- 级别:unit
- 里程碑:M2
- 前置:按 [journal-format](../../docs/subsystems/journal-format.md) 布局改写 `J_MD` 段的副本:段头 schema 表把 `Bbo` 登记为比本 wheel 编译版本小 1 的版本、体长 88 B,全部 `Bbo` 记录截成 88 B 体(去掉 `update_id`),重算段头 CRC
- 步骤:
  1. `df = pl.DataFrame(open_journal(副本).events("Bbo", inst=101))`
- 期望:
  - 60 000 行;`update_id` 全为 0;`_missing` 列中 `update_id` 对应的位全为 1、其他位全为 0;其余字段与 `J_MD` 原值相同;不抛异常
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_048_new_reads_old`

### PY-049 旧读者读新段:截断到已知字段并在 info() 报告

- 规则:[Arrow 映射](README.md#arrow-映射)
- 级别:unit
- 里程碑:M2
- 前置:同 PY-048 的改写方式,但段头把 `Bbo` 登记为编译版本 + 1、体长 104 B,每条 `Bbo` 体尾追加 8 字节 `0xFF`
- 步骤:
  1. `j = open_journal(副本)`;`df = pl.DataFrame(j.events("Bbo", inst=101))`
- 期望:
  - 60 000 行,列集合与 `J_MD` 相同(没有多出的列),字段值与 `J_MD` 相同
  - `j.info()["schema"]` 恰有 1 项:`Bbo`,journal 版本 = 编译版本 + 1,本读者版本 = 编译版本
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_049_old_reads_new`

### PY-050 同一 kind 版本相同而大小不同时抛 SchemaError

- 规则:[Arrow 映射](README.md#arrow-映射)、[错误](README.md#错误)
- 级别:table
- 里程碑:M2
- 前置:改写 `J_MD` 段副本的段头 schema 表中 `Bbo` 一项(重算段头 CRC)
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_050_same_version_size_mismatch`

| 行 | 段头中 `Bbo` 的 `{version, size}` | 期望 |
|---|---|---|
| PY-050.1 | `{编译版本, 编译大小 + 8}` | `open_journal` 抛 `hft_py.SchemaError` |
| PY-050.2 | `{编译版本, 编译大小 − 8}` | `hft_py.SchemaError` |
| PY-050.3 | `{编译版本, 编译大小}` | 打开成功,`info()["schema"]` 为空(对照) |
| PY-050.4 | `{编译版本 + 1, 编译大小 + 8}` | 打开成功(按 PY-049 读取) |

### PY-051 schema_hash 与 catalog 和生成器输出同源

- 规则:[Python API](README.md#python-api导入名-hft_py)、[Arrow 映射](README.md#arrow-映射)
- 级别:unit
- 里程碑:M2
- 前置:同一提交的 `cargo xtask gen-schema` 输出(`python/_generated/` 与 `docs/generated/event-catalog.md`)
- 步骤:
  1. `h = hft_py.schema_hash()`;`c = hft_py.schema.catalog()`
- 期望:
  - `h` 等于 Rust 侧对 `KINDS` 的 `{kind, version, size}` 表求得的哈希,也等于 `python/_generated/` 中记录的值
  - `c` 的行数 = `KINDS.len()`;每行的 kind 编号、名字、layout、role 与 `event-catalog.md` 同一行相同
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_051_schema_hash_catalog`

### PY-052 生产构建读到 0x7Fxx 测试 kind 时抛 JournalError

- 规则:[Arrow 映射](README.md#arrow-映射)、[错误](README.md#错误)
- 级别:unit
- 里程碑:M2
- 前置:`J_MD` 副本中第 5 001 条记录的 kind 改为 `0x7F01`(体不变);发布剖面构建的 wheel
- 步骤:
  1. `list(open_journal(副本).events("Bbo"))`
- 期望:
  - 抛 `hft_py.JournalError`,`e.seq` = 被改记录的 seq;在此之前已产出的批只含 seq 更小的行
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_052_dev_kind_rejected`

### PY-053 本 wheel 不认识的 kind 不影响其他 kind 的读取

- 规则:[Arrow 映射](README.md#arrow-映射)
- 级别:unit
- 里程碑:M2
- 前置:`J_MD` 副本中把 100 条 `Trade` 的 kind 改为 `0x01FE`(未注册、属行情段)
- 步骤:
  1. `events("Bbo")`、`events("Trade")`
- 期望:
  - `Bbo` 仍为 120 000 行;`Trade` 为 500 行(600 − 100);不抛异常
- 实现:`crates/hft-py/tests/python/test_arrow.py::test_py_053_unknown_kind_skipped`

### PY-055 Polars 拿到的列缓冲就是 Rust 分配的缓冲

- 规则:[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:unit
- 里程碑:M2
- 前置:以 cargo feature `testing` 构建(`hft_py._testing.last_buffer_addrs()` 返回最近一批每列数据缓冲的地址);`j = open_journal(J_MD)`
- 步骤:
  1. `df = pl.DataFrame(j.events("Bbo", inst=101, batch_rows=60000))`
  2. `a = hft_py._testing.last_buffer_addrs()`
- 期望:
  - 对 `bid_px`、`ask_px`、`bid_qty`、`seq` 四列,Polars 列的数据缓冲地址(`Series.to_numpy(allow_copy=False)` 的 `ctypes.data`;Polars 2 已删除 `_get_buffer_info`)等于 `a` 中同名列的地址
- 实现:`crates/hft-py/tests/python/test_zero_copy.py::test_py_055_polars_zero_copy`

### PY-056 pyarrow 与 DuckDB 经 PyCapsule 消费同一流

- 规则:[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:table
- 里程碑:M2
- 前置:同 PY-055 的 `testing` 构建;每行新建 `rdr = j.events("Bbo", inst=101, batch_rows=60000)`
- 实现:`crates/hft-py/tests/python/test_zero_copy.py::test_py_056_pyarrow_duckdb`

| 行 | 消费方式 | 期望 |
|---|---|---|
| PY-056.1 | `pa.RecordBatchReader.from_stream(rdr).read_next_batch()` | `bid_px` 列 `buffers()[1].address` 等于 `last_buffer_addrs()["bid_px"]` |
| PY-056.2 | `duckdb.sql("select count(*), sum(bid_px) from rdr")` | `(60000, 360000000000)` |

### PY-060 带 CRC 的记录 CRC 不符时抛 JournalError 并带段路径与 seq

- 规则:[错误](README.md#错误)
- 级别:unit
- 里程碑:M2
- 前置:`J_ALL` 副本中第一条 `ControlCmd`(seq = c,CRC kind)体内翻转 1 个比特,CRC 不重算
- 步骤:
  1. `list(open_journal(副本).events("ControlCmd"))`
- 期望:
  - 抛 `hft_py.JournalError`,`e.seq == c`,`e.path` 为含该记录的段文件路径;`events("Bbo")` 读到 seq < c 的全部 `Bbo` 后同样在 c 处抛出
- 实现:`crates/hft-py/tests/python/test_corrupt.py::test_py_060_bad_record_crc`

### PY-061 段头 CRC 不符时抛 JournalError

- 规则:[错误](README.md#错误)
- 级别:table
- 里程碑:M2
- 前置:`J_ALL` 副本(2 段),在各行指定的段头不可变区翻转 1 个比特
- 实现:`crates/hft-py/tests/python/test_corrupt.py::test_py_061_bad_header_crc`

| 行 | 被改的段 | 期望 |
|---|---|---|
| PY-061.1 | 段 1 | `open_journal` 或第一次读取抛 `hft_py.JournalError`,`e.path` = 段 1 路径;没有任何行被交付 |
| PY-061.2 | 段 2 | 段 1 的行照常交付,到段 2 时抛 `hft_py.JournalError`,`e.path` = 段 2 路径 |
| PY-061.3 | 段 2 的段头 magic 改为 `0` | 同 PY-061.2 |

### PY-062 冷副本截断在一条记录中间时,交付到上一条为止后抛 JournalError

- 规则:[错误](README.md#错误)
- 级别:unit
- 里程碑:M2
- 前置:`J_MD` 副本(已封存段),把冷副本文件截断到第 70 000 条记录体的中间(`ftruncate`)
- 步骤:
  1. 收集 `events("Bbo")` 的批直到异常
- 期望:
  - 抛 `hft_py.JournalError`,`e.seq` = 被截断记录的 seq;异常前已交付的 `Bbo` 行与 `RUST_REF` 中 seq 小于该值的全部 `Bbo` 逐行相同
- 实现:`crates/hft-py/tests/python/test_corrupt.py::test_py_062_truncated_cold_copy`

### PY-063 热段 committed_len 之外的字节不被读取

- 规则:[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:integ
- 里程碑:M2
- 前置:`TW` 写 100 条合成 `Bbo` 并 `publish`;随后 `TW` 再追加 10 条但不 `publish`;再在 `committed_len` 之后的空白处写 4 KiB 随机字节(模拟未发布区的残留)
- 步骤:
  1. `pl.DataFrame(open_journal(cold, hot_root=hot).events("Bbo")).height`
- 期望:
  - 返回 100;不抛异常
- 实现:`crates/hft-py/tests/python/test_corrupt.py::test_py_063_beyond_committed_len`

### PY-064 多段组总长超过重组上限时抛 JournalError,不分配

- 规则:[错误](README.md#错误)
- 级别:unit
- 里程碑:M2
- 前置:`J_ALL` 副本中拆成 3 段的 `ControlCmd` 的第 0 段 `PartHdr.total_len` 改为 256 MiB + 1 B(hft-py 的 `JournalReader` 用默认 `reassembly_cap` = 256 MiB);记下读取前的进程 RSS
- 步骤:
  1. `list(open_journal(副本).events("ControlCmd"))`
- 期望:
  - 抛 `hft_py.JournalError`,`e.seq` = 第 0 段的 seq;读取期间 RSS 增长 < 16 MiB
- 实现:`crates/hft-py/tests/python/test_corrupt.py::test_py_064_reassembly_cap`

### PY-065 会让 Rust 侧失败的输入都变成 Python 异常,进程不退出

- 规则:[错误](README.md#错误)
- 级别:table
- 里程碑:M2
- 前置:同一个 pytest 进程依次执行各行;每行之后执行一次 `open_journal(J_MD).events("Bbo", inst=101)` 并断言得到 60 000 行
- 实现:`crates/hft-py/tests/python/test_corrupt.py::test_py_065_no_panic_across_ffi`

| 行 | 输入 | 期望 |
|---|---|---|
| PY-065.1 | 记录头 `len = 0xFFFF_FFF8` 的段副本上 `events("Bbo")` | `hft_py.JournalError` |
| PY-065.2 | `events("Bbo", t0 = 2**63)`(超出 i64) | `ValueError` |
| PY-065.3 | `config_at(seq = 2**64)` | `ValueError` |
| PY-065.4 | `tail(["Bbo"], poll_ms = 2**64)` | `ValueError` |
| PY-065.5 | `hft_py.params.decode(S_TP, b"")` | `hft_py.ParamsError` |
| PY-065.6 | `hft_py.params.decode(S_TP, b"\x00" * 47)`(短于 `StrategyParamsHdr`) | `hft_py.ParamsError` |
| PY-065.7 | `heartbeat.read("prod", root=…)` 读 4 096 字节全零的 `heartbeat` 文件(magic 不符) | `hft_py.SchemaError` |
| PY-065.8 | `hft_py.decode.config_blob(b"")` | `ValueError` |

### PY-070 写者持续追加并滚动 3 次时 tail 不丢不重

- 规则:[Python API](README.md#python-api导入名-hft_py)、[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:integ
- 里程碑:M2
- 前置:`TW` 以每秒 20 000 条的速率写合成 `Bbo`,共 200 000 条,`segment_bytes = 8 MiB`(写满约每 60 000 条滚动一次,共 3 次);`it = open_journal(cold, hot_root=hot).tail(["Bbo"], from_seq=None, poll_ms=50)` 在 `TW` 启动前打开
- 步骤:
  1. 迭代 `it` 直到累计 200 000 行或 30 s 超时
- 期望:
  - 累计恰好 200 000 行;`seq` 严格递增且同为 `Bbo` 的相邻行之间没有遗漏(与 `RUST_REF` 的 `Bbo` seq 序列相同);没有重复的 seq
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_070_tail_no_loss_no_dup`

### PY-071 跨段滚动时先读新段引导块,每段的 ConfigBlob 只交付一次

- 规则:[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:integ
- 里程碑:M2
- 前置:同 PY-070 的写入;`it = tail(["ConfigBlob"], poll_ms=50)`;`RUST_REF` 统计到每段引导块各有 N 条 `ConfigBlob{boot = 1}`
- 步骤:
  1. 迭代到写入结束
- 期望:
  - 交付的 `ConfigBlob` 行数 = 3 × N(打开后新出现的 3 个段);同一 `(seq)` 不出现两次;每次滚动后第一批的 `seq` 均落在新段引导块内
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_071_roll_reads_boot_block`

### PY-072 from_seq 为 None 时只交付打开之后新提交的记录

- 规则:[参数语义](README.md#参数语义)
- 级别:integ
- 里程碑:M2
- 前置:`TW` 已写并 `publish` 1 000 条合成 `Bbo`(最后一条 seq = s)
- 步骤:
  1. `it = j.tail(["Bbo"], from_seq=None, poll_ms=50)`
  2. `TW` 再写 10 条
  3. 迭代 `it` 直到 10 行
- 期望:
  - 得到恰好 10 行,`seq` 全部 > s;没有 seq ≤ s 的行
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_072_from_seq_none`

### PY-073 from_seq 的取值边界

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`TW` 已写 1 000 条合成 `Bbo`,最早可读记录 seq = o,最后一条 seq = l
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_073_from_seq_bounds`

| 行 | `from_seq` | 期望 |
|---|---|---|
| PY-073.1 | `o` | 第一批首行 `seq = o`(或 o 之后第一条 `Bbo`) |
| PY-073.2 | `o − 1` | `ValueError` |
| PY-073.3 | `l + 1` | 不抛异常;只交付之后新写入的记录 |
| PY-073.4 | `l + 2` | `ValueError` |

### PY-074 poll_ms 的取值边界

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(cold, hot_root=hot)`(`TW` 已写 10 条)
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_074_poll_ms_bounds`

| 行 | `poll_ms` | 期望 |
|---|---|---|
| PY-074.1 | `0` | `ValueError` |
| PY-074.2 | `-1` | `ValueError` |
| PY-074.3 | `1` | 不抛异常 |
| PY-074.4 | 省略(默认 200) | `testing` 构建下,2 000 ms 无新记录期间 `hft_py._testing.tail_polls()` 增加 9–11 次 |

### PY-075 只交付已发布的完整事件

- 规则:[参数语义](README.md#参数语义)
- 级别:integ
- 里程碑:M2
- 前置:`TW` 写一个事件的前 2 条记录(输入 `Bbo` 与一条输出 `Note`)但不 `end_event`、不 `publish`;`it = tail(["Bbo", "Note"], from_seq=None, poll_ms=10)` 已打开
- 步骤:
  1. 等待 200 ms 并取 `it` 中已有的批(非阻塞取法:测试在独立线程迭代并收集)
  2. `TW` 写完该事件(`end_event` + `publish`)后再等待 200 ms
- 期望:
  - 步骤 1 后收集到 0 行
  - 步骤 2 后恰好收到这 2 条记录(`Bbo` 1 行、`Note` 1 行)
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_075_only_published_events`

### PY-076 tail 按 seq 把记录切成连续同 kind 段,批带 kind 元数据

- 规则:[参数语义](README.md#参数语义)
- 级别:integ
- 里程碑:M2
- 前置:`TW` 按顺序写 `Bbo`、`Bbo`、`OrderEvent`、`Bbo`、`OrderEvent`、`OrderEvent`(各为独立事件,一次 `publish`);`it = tail(["Bbo", "OrderEvent"], poll_ms=10)` 在写入前打开
- 步骤:
  1. 迭代直到 6 行
- 期望:
  - 依次得到 4 个批:`Bbo` 2 行、`OrderEvent` 1 行、`Bbo` 1 行、`OrderEvent` 2 行;每批 schema 元数据 `kind` 分别为 `"Bbo"`、`"OrderEvent"`、`"Bbo"`、`"OrderEvent"`;拼接后的 `seq` 严格递增;没有 0 行的批
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_076_kind_runs`

### PY-077 tail 的 kinds 含未知 kind 时拒绝

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_MD)`
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_077_tail_unknown_kind`

| 行 | `kinds` | 期望 |
|---|---|---|
| PY-077.1 | `["Bbo", "Nope"]` | `ValueError` |
| PY-077.2 | `[0x01FF]` | `ValueError` |
| PY-077.3 | `["Bbo", 0x0101]` | 不抛异常(同一 kind 的两种写法) |

### PY-078 尾随中热段被退役时切到冷副本并解除映射

- 规则:[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:integ
- 里程碑:M2
- 前置:同 PY-070 的写入;尾随进行中,段 1 封存且 persist 已同步后,测试删除热目录中的段 1 文件
- 步骤:
  1. 继续迭代到写入结束
  2. 删除后 2 s 读取本进程 `/proc/self/maps`
- 期望:
  - 累计行数与 seq 序列同 PY-070(不丢不重)
  - 步骤 2 中不再有指向已删除段 1 文件(`(deleted)` 标记)的映射
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_078_retired_segment_unmapped`

### PY-079 写者崩溃并恢复后,tail 继续交付且不重复、不回退

- 规则:[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:integ
- 里程碑:M2
- 前置:`TW` 写合成 `Bbo` 5 000 条后以 `SIGKILL` 结束(最后一个事件只写了一半、未发布);`it = tail(["Bbo"], poll_ms=50)` 已收到全部已发布的行(最后 seq = s);随后以恢复模式重启 `TW`(按 hft-journal 恢复规则截断未完成事件)并再写 1 000 条
- 步骤:
  1. 继续迭代直到再收到 1 000 行或 30 s
- 期望:
  - 新收到恰好 1 000 行,首行 `seq = s + 1`(或恢复写入的新引导块之后的第一条 `Bbo`),全部 > s;没有 seq ≤ s 的行再次出现
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_079_writer_crash_recovery`

### PY-080 tail 的可见延迟不超过 poll_ms + 5 ms

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[1. journal → Arrow](README.md#1-journal--arrow)
- 级别:integ
- 里程碑:M2
- 前置:`poll_ms = 200`;`TW` 每 1 s 写并 `publish` 1 条合成 `Bbo`,写入时记 `CLOCK_MONOTONIC`(经同一测试进程的管道把时刻传给 pytest);共 50 条
- 步骤:
  1. 对每条记录计算"`tail` 产出该行的单调时刻 − 写者 `publish` 的单调时刻"
- 期望:
  - 50 个样本全部 ≤ 205 ms
- 实现:`crates/hft-py/tests/python/test_tail.py::test_py_080_tail_visibility_latency`

`config_at` 用例的夹具 **`J_CFG`**(`pyfixture cfg`,两次 `TW` 运行:第二次是重启,写段 2;refdata `RD_PY`,策略实例 `alpha` 绑定 A1;RiskParams 与 HostManifest 的内容是占位字节 `risk-params-v1`/`-v2`、`host-manifest`,Engine 是只有 `ledger` 节的 ECF1):段 1 引导块含 `ConfigBlob{boot = 1}` 的 Refdata(`version` 1)、RiskParams(`version` 1)、Engine、StrategyParams(`alpha/bn-1`,`version` 7)、HostManifest;之后依次是:① `ConfigBlob{StrategyParams, version 8}`(seq `p1`)→ `ControlCmd{SetParams}`(seq `p2`)→ `CmdResult{Applied}`(seq `p3`);② `ConfigBlob{StrategyParams, version 9}` → `SetParams` → `CmdResult{InvalidArgs}`(seq `r3`);③ `ConfigBlob{RiskParams, version 2}` → `SetRisk` → `CmdResult{Applied}`(seq `k3`);④ `ConfigBlob{Refdata, version 2}` → `ReplaceRefdata` → `CmdResult{Applied}`(seq `a3`)→ Drain;段 2 是重启后的新运行,引导块(`BootDone` 的 seq = `b2`)含 Refdata `version` 2。

### PY-085 config_at 返回该运行引导块中的配置

- 规则:[Python API](README.md#python-api导入名-hft_py)、[参数语义](README.md#参数语义)
- 级别:unit
- 里程碑:M2
- 前置:`j = open_journal(J_CFG)`;段 1 `BootDone` 的 seq = `b1`
- 步骤:
  1. `c = j.config_at(b1)`
- 期望:
  - `c["refdata"]["version"] == 1`、`c["risk"]["version"] == 1`、`c["strategy_params"]["alpha/bn-1"]["version"] == 7`;每项带 `seq`(段 1 引导块中该 `ConfigBlob` 的 seq)与 `sha256`(= 该 blob 的 `BlobHdr.sha256`)
- 实现:`crates/hft-py/tests/python/test_config.py::test_py_085_boot_block_config`

### PY-086 SetParams 与 SetRisk 只在 CmdResult Applied 之后生效

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_CFG)`
- 实现:`crates/hft-py/tests/python/test_config.py::test_py_086_applied_only`

| 行 | `config_at(seq)` | 期望 |
|---|---|---|
| PY-086.1 | `p1`(新 blob 已写、命令未到) | `strategy_params["alpha/bn-1"]["version"] == 7` |
| PY-086.2 | `p3 − 1`(命令已写、结果未到) | `version == 7` |
| PY-086.3 | `p3` | `version == 8`,`sha256` = ① 的 blob 的 SHA-256 |
| PY-086.4 | `r3`(② 被拒) | `version == 8` |
| PY-086.5 | `k3 − 1` | `risk["version"] == 1` |
| PY-086.6 | `k3` | `risk["version"] == 2` |

### PY-087 ReplaceRefdata 的 blob 到下一次引导块才生效

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_CFG)`
- 实现:`crates/hft-py/tests/python/test_config.py::test_py_087_refdata_next_boot`

| 行 | `config_at(seq)` | 期望 |
|---|---|---|
| PY-087.1 | `a3`(`Applied`) | `refdata["version"] == 1` |
| PY-087.2 | 段 1 最后一条记录的 seq | `refdata["version"] == 1` |
| PY-087.3 | `b2` | `refdata["version"] == 2`;`strategy_params["alpha/bn-1"]["version"] == 8`(段 2 引导块重述) |

### PY-088 各 BlobKind 的返回形态

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`c = open_journal(J_CFG).config_at(k3)`;期望值来自 `RUST_REF` 对同一 blob 的解码(Refdata 经 hft-refdata、RiskParams 经 hft-risk、Engine 经 hft-refdata 的 ECF1)
- 实现:`crates/hft-py/tests/python/test_config.py::test_py_088_blob_shapes`

| 行 | 键 | 期望 |
|---|---|---|
| PY-088.1 | `refdata` | dict;账户表含 label `bn-1`(`AccountId(1)`、`kind = Live`);inst 101 的 `px_exp = −2` |
| PY-088.2 | `risk` | dict;`bytes` 为该 blob 的原始字节(`risk-params-v2`),SHA-256 = `sha256`(hft-risk 提供 `RiskParams` blob 编解码后改为解码字段) |
| PY-088.3 | `engine` | dict;`sections` 含 `ledger` 的 512 字节原始行及 `assembly` 的 16 字节样例行(field=1,value=0);各节解码归各自 crate |
| PY-088.4 | `strategy_params["alpha/bn-1"]` | `hdr` 含 `strategy = 1`、`account = 1`、`kind_id`、`schema_id`、`schema_ver`、`source`、`version = 8`;`groups` 为 `GroupSpec` 字段的列表;`body` 为 `bytes`,长度 = `hdr.body_len` |
| PY-088.5 | `host_manifest` | dict,`bytes` 的 SHA-256 = 该项 `sha256` |
| PY-088.6 | `sim_config` | 不在结果中(prod 实例没有该 blob) |

### PY-089 seq 早于第一个完整引导块时拒绝

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_CFG)`;段 1 第一条记录 seq = `f`,`BootDone` 的 seq = `b1`
- 实现:`crates/hft-py/tests/python/test_config.py::test_py_089_before_boot`

| 行 | `config_at(seq)` | 期望 |
|---|---|---|
| PY-089.1 | `f` | `ValueError` |
| PY-089.2 | `b1 − 1` | `ValueError` |
| PY-089.3 | `b1` | 返回 dict(对照,同 PY-085) |
| PY-089.4 | `-1` | `ValueError` |

### PY-090 heartbeat.read 的字段与心跳块一致

- 规则:[参数语义](README.md#参数语义)、[Python API](README.md#python-api导入名-hft_py)
- 级别:unit
- 里程碑:M2
- 前置:`HB`;`set_state(3)`(Running);`sequencer().beat()` 5 次;housekeeping 行写 `hk_utc_ns = D + 60 s`
- 步骤:
  1. `hb = heartbeat.read("prod", root="<tmp>/shm")`
- 期望:
  - `hb["state"] == "Running"`、`hb["seq_beats"] == 5`、`hb["instance_id"] == 1`、`hb["n_slots"] == 0`、`hb["pid"]` = 建块进程的 pid、`hb["hk_utc_ns"] == 1_790_812_860_000_000_000`、`hb["boot_id"]` 为 32 位小写十六进制、`hb["mono_ns"]` 在读取前后两次 `time.monotonic_ns()` 之间
- 实现:`crates/hft-py/tests/heartbeat.rs::py_090_read_fields`

### PY-091 state 码到名字的映射

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`HB`;每行 `set_state(code)` 后 `heartbeat.read("prod", root="<tmp>/shm")["state"]`
- 实现:`crates/hft-py/tests/heartbeat.rs::py_091_state_names`

| 行 | `code` | 期望 |
|---|---|---|
| PY-091.1 | 1 | `"Starting"` |
| PY-091.2 | 2 | `"Warmup"` |
| PY-091.3 | 3 | `"Running"` |
| PY-091.4 | 4 | `"Draining"` |
| PY-091.5 | 5 | `"Stopped"` |

### PY-092 心跳文件缺失或布局不符时报错

- 规则:[参数语义](README.md#参数语义)、[错误](README.md#错误)
- 级别:table
- 里程碑:M2
- 前置:`<tmp>/shm/` 下按各行准备 `prod/heartbeat`
- 实现:`crates/hft-py/tests/heartbeat.rs::py_092_read_errors`

| 行 | 文件 | 期望 |
|---|---|---|
| PY-092.1 | 不存在 | `FileNotFoundError` |
| PY-092.2 | `HB` 建块后把 `magic` 改为 `*b"HFTHB\0\x02\0"` | `hft_py.SchemaError` |
| PY-092.3 | `HB` 建块后把 `layout_version` 改为 2 | `hft_py.SchemaError` |
| PY-092.4 | `HB` 原样 | 不抛异常(对照) |

### PY-093 Watch 按区 0 计数的变化计算心跳年龄

- 规则:[参数语义](README.md#参数语义)、[Python API](README.md#python-api导入名-hft_py)
- 级别:unit
- 里程碑:M2
- 前置:`HB`,`set_state(3)`;`w = heartbeat.Watch("prod", root="<tmp>/shm")`
- 步骤:
  1. `r1 = w.check()`
  2. 不 beat,等待 1 500 ms 后 `r2 = w.check()`
  3. `sequencer().beat()` 一次后立即 `r3 = w.check()`
  4. 以 `HeartbeatBlock::create` 在同一路径重建块(计数归 0)、`set_state(1)`,立即 `r4 = w.check()`
- 期望:
  - `r1 == {"state": "Running", "advanced": True, "age_ms": 0}`
  - `r2["advanced"] is False`,`1500 ≤ r2["age_ms"] ≤ 1550`
  - `r3["advanced"] is True`,`r3["age_ms"] ≤ 5`
  - `r4["advanced"] is True`,`r4["state"] == "Starting"`,`r4["age_ms"] ≤ 5`
- 实现:`crates/hft-py/tests/heartbeat.rs::py_093_watch_age`

### PY-094 读取心跳不改变心跳文件

- 规则:[参数语义](README.md#参数语义)
- 级别:unit
- 里程碑:M2
- 前置:`HB`,`set_state(3)`,beat 1 次;记下文件的 SHA-256 与 `mtime`;文件权限改为 0440(只读)
- 步骤:
  1. `heartbeat.read` 100 次、`Watch(…).check()` 100 次
- 期望:
  - 全部调用不抛异常;文件 SHA-256 与 `mtime` 与步骤前相同
- 实现:`crates/hft-py/tests/heartbeat.rs::py_094_read_is_read_only`

### PY-100 ledger 的仓位、库存与权益视图与 Rust 冷读者逐值相同

- 规则:[概要](README.md#概要)、[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:integ
- 里程碑:M2
- 前置:`J_LED20`(20 个种子);`t0 = None`、`t1 = None`
- 步骤:
  1. 对每个种子 `led = hft_py.ledger(j, account="bn-1")`,取 `positions()`、`inventory()`、`portfolio()`
- 期望:
  - 每个种子三者与 `RUST_REF` 的 hft-ledger `JournalLedger` 在 journal 末尾的 `InstPos`(按 venue、inst)、现货库存(按 group 与 `unassigned`)、按 venue 的权益视图字段逐值相等(定点原值)
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_100_ledger_views_match_rust`

### PY-101 ledger 从 t0 之前最近的快照起步

- 规则:[参数语义](README.md#参数语义)、[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:table
- 里程碑:M2
- 前置:`J_LED20` 种子 1 的副本,共 3 段,段 2、段 3 各有一条带 ledger 节的关键帧 `StateSnapshot`(seq `k2`、`k3`,时刻 `T2`、`T3`);`t1 = None`
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_101_start_snapshot`

| 行 | 条件 | 期望 |
|---|---|---|
| PY-101.1 | `t0 = T3`,三段齐全 | `positions()` 等于 hft-ledger `JournalLedger` 自 `k3` 的快照起消费到末尾的结果 |
| PY-101.2 | `t0 = T3 + 1 ns`,删除段 1 与段 2 的全部副本 | 不抛异常;结果同 PY-101.1 |
| PY-101.3 | `t0 = T3 − 1 ns`,删除段 1 与段 2 的全部副本 | 从最早可读段(段 3)的引导块起步,不抛异常;结果等于 hft-ledger `JournalLedger` 自段 3 引导块快照起的结果 |
| PY-101.4 | `t0 = None`,三段齐全 | 从段 1 引导块起步;结果同 PY-100 中种子 1 |

### PY-102 breakdown 是 [t0, t1) 内入账的 PnlDelta 之和

- 规则:[参数语义](README.md#参数语义)、[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:table
- 里程碑:M2
- 前置:`J_LED20` 种子 2;hft-ledger `JournalLedger` 记录每个事件的 `PnlDelta` 与该事件 Input 的 `proc_ns`;journal 时间范围 `[S, E)`,中点 `M`
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_102_breakdown_window`

| 行 | `t0` / `t1` | 期望 |
|---|---|---|
| PY-102.1 | `None` / `None` | `breakdown()` 各列 = 全部 `PnlDelta` 之和 |
| PY-102.2 | `S` / `M` | 各列 = `proc_ns ∈ [S, M)` 的事件的 `PnlDelta` 之和 |
| PY-102.3 | `M` / `E` | 各列 = 从 `M` 之前最近一条带 ledger 节的快照起步的 `JournalLedger` 中 `proc_ns ∈ [M, E)` 的和(快照不含估值价,与从头起步的增量不一定相同,见 README `ledger`) |
| PY-102.4 | `M` / `M` | 各列为 0 |

### PY-103 ledger 的账户可用名字或 id,模拟账户结果带 Sim 标注

- 规则:[参数语义](README.md#参数语义)、[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:table
- 里程碑:M2
- 前置:`J_LED20` 种子 3(账户 A1,prod);`J_SIM`(账户 A9,sim)
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_103_ledger_accounts`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-103.1 | `ledger(J_LED20#3, account="bn-1").portfolio()` | 与 `account=1` 的结果相同;`account_kind` 列全为 `"Live"` |
| PY-103.2 | `ledger(J_SIM, account="sim-a").portfolio()` | `account_kind` 列全为 `"Sim"`;值等于 hft-ledger `JournalLedger` 对 A9 的结果 |
| PY-103.3 | `ledger(J_LED20#3, account="sim-a")` | `ValueError`(prod 实例 refdata 中没有该账户) |

### PY-104 pnl 按四种键汇总与 Rust 相同

- 规则:[Python API](README.md#python-api导入名-hft_py)、[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`J_LED20` 种子 4;参照 = hft-ledger `JournalLedger` 的 `PnlDelta` 序列按同一键求和
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_104_pnl_by_keys`

| 行 | `by` | 期望 |
|---|---|---|
| PY-104.1 | `"group"` | 每个 GroupId(含 `GROUP_NONE`)一行,`PnlBreakdown` 各列与参照相等 |
| PY-104.2 | `"account"` | 每个账户一行,与参照相等 |
| PY-104.3 | `"round"` | 每个 (group, 事件结束时该 group 的 `round_id`) 一行,与参照相等 |
| PY-104.4 | `"day"` | 每个 UTC 日一行,与参照相等;各行之和等于 PY-104.2 的账户合计 |

### PY-105 pnl 的 by 取其他值时拒绝

- 规则:[参数语义](README.md#参数语义)
- 级别:table
- 里程碑:M2
- 前置:`j = open_journal(J_LED20#4)`
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_105_pnl_bad_by`

| 行 | `by` | 期望 |
|---|---|---|
| PY-105.1 | `"week"` | `ValueError` |
| PY-105.2 | `""` | `ValueError` |
| PY-105.3 | `"Group"` | `ValueError` |
| PY-105.4 | `None` | `ValueError` |

### PY-106 带 AUTO_RECON 的注记经同一合成函数入账

- 规则:[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:integ
- 里程碑:M2
- 前置:`J_EXT`;E1 的 Input 时刻 `T1`,E2 的时刻 `T2`
- 步骤:
  1. `led = hft_py.ledger(j, account="bn-1", t1 = T2)`(只消费到 E1)
- 期望:
  - E1 的事件内有热进程写出的 `Note{ExternalFill}`;`positions()` 中 `BTC/P2` 的数量 = 6.0(raw 600 000 000,指数 −8);与热进程在 E1 之后一致(`SEQ_FX`;`breakdown()` = 热进程自启动起的 `PnlBreakdown`)
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_106_auto_recon_note_booked`

### PY-107 补推的同向 ADL 被 auto_held 抵扣,不重复入账

- 规则:[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:integ
- 里程碑:M2
- 前置:`J_EXT`;E3 的时刻 `T3`
- 步骤:
  1. `led = hft_py.ledger(j, account="bn-1", t1 = T3)`(消费到 E2)
- 期望:
  - E2 的事件内有 `Note{LedgerAutoHeldOffset}`;`BTC/P2` 数量仍为 6.0;`breakdown()` 的 `external` 列与 PY-106 相同;与热进程在 E2 之后一致
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_107_adl_offset_by_auto_held`

### PY-108 不带 AUTO_RECON 的 ExternalFill 注记不被当作成交

- 规则:[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:integ
- 里程碑:M2
- 前置:`J_EXT` 全部事件
- 步骤:
  1. `led = hft_py.ledger(j, account="bn-1")`
- 期望:
  - E4 的事件内有 `Note{ExternalFill}`(`aflags = 0`);`BTC/P2` 数量 = 6.0 + 0.3(E3)+ 0.4(E4)= 6.7;E4 只按 Input 入账一次(若也按注记入账会是 7.1);与热进程在 E4 之后一致
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_108_plain_note_not_booked`

### PY-109 OMS 防御路径的外来成交经注记入账,与热进程逐值相同

- 规则:[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:table
- 里程碑:M2
- 前置:各行一个 `F_FF` 夹具(`SEQ_FX`;账户 A1 原无 `BTC/P2` 持仓;所列 `OrderEvent` 均为 `BTC/P2` 上 `detail = Buy`、`px = avg_px = 60 100`、`fee = 0.09` USDT、`exch_order_id = 99` 的 `Fill`);`led = hft_py.ledger(j, account="bn-1")`;热进程对每条产出 `ForeignFill` 的报告在同一事件内写 `Note{OmsForeignFill}`(`qty` = 按 `foreign` 表求得的增量,`trade_id = 0`,`fill_seq` = `cum_filled`)
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_109_oms_foreign_fill`

| 行 | Input 与注记 | 期望 |
|---|---|---|
| PY-109.1 | `slot = SLOT_UNRESOLVED`、`cum_filled = 0.3` → 注记 `path = 1`(防御路径) | `BTC/P2` 数量 +0.3;每个步骤与热进程一致 |
| PY-109.2 | slot 9(从未用过)、gen 1、`cum_filled = 0.3`(`StaleGen`)→ 注记 `path = 2` | +0.3;一致 |
| PY-109.3 | 已删除:hft-oms 的 `Note{OmsForeignFill}` 恒写 `trade_id = 0`(`OrderEvent` 不带成交 id),热进程不产生 `trade_id ≠ 0` 的这条注记 | — |
| PY-109.4 | 先 Input `ExternalFill{Unknown, Buy, 0.3, exch_order_id 99, fill_seq 0.5}` 入账,再 `slot = SLOT_UNRESOLVED`、`cum_filled = 0.5` → 注记 `path = 1`、`qty = 0.2`(OMS 先扣 `ext_held` 0.3) | 自外部成交之后数量不再变化(ledger 按 `(inst, 99, 0.5)` 去重);一致 |
| PY-109.5 | 同一外来单的两条报告(两个事件):`slot = SLOT_UNRESOLVED`、`cum_filled` 0.2 与 0.5 → 两条注记 `path = 1`、`qty` 0.2 与 0.3 | +0.5,两条都入账;一致 |

### PY-110 同一 journal 计算两次结果相同

- 规则:[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:unit
- 里程碑:M2
- 前置:`J_EXT` 与 `J_LED20` 种子 5
- 步骤:
  1. 对每个 journal 连续两次 `ledger(…).positions()`、`.breakdown()` 与 `pnl(…, by="group")`
- 期望:
  - 两次结果逐值相同(Polars `equals` 为真)
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_110_deterministic`

### PY-111 各类入账样例与热进程一致

- 规则:[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:table
- 里程碑:M2
- 前置:各行一个 `F_SAMPLE` 夹具(`SEQ_FX`,账户 A1);比较对象为 `hft_py.ledger(j, account="bn-1", t1 = …)` 在该夹具每个具名步骤的 `positions()`、`inventory()`、`portfolio()`、`breakdown()` 与三节字节,与热进程在同一步骤的读数("一致"按 `SEQ_FX`)
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_111_booking_samples`

| 行 | 事件 | 期望 |
|---|---|---|
| PY-111.1 | 策略在 `G1` 现货买入 101/S1 0.01 BTC,成交回报的手续费 0.000 01 BTC(基础币) | `101` 数量 0.009 99;`fee_cex` 非 0;一致 |
| PY-111.2 | `G1` 的永续腿经成交多 1.0,之后 Input `ExternalFill{Liquidation, Sell, BTC/P2, 0.4, px 59 000, fee 0.01}` 归属到该腿 | 数量 0.6;`external` 非 0;一致 |
| PY-111.3 | S1 的 `BalanceReport`(USDT、BTC)→ `SetBalanceBaseline{A1, S1}` → 又一条 USDT `BalanceReport` | 一致 |
| PY-111.4 | (`P2` 为 `BalanceOnlyPush`)`G1` 永续多 1.0;`Funding{rate 0.000 1, next_funding_ns = D}` 使热进程在结算时刻 `D` 合成 `FundingSettle{synth = 1}`(Output);2 s 后对账 `Funding` 钱包条目(`inst = INST_NONE`、`exch_ts_ns = D`、`exch_amount` = 合成额)经 `reconcile_funding` 入账 `AdjustDelta::Funding` | 合成之后 `funding` 列 = 合成额且非 0;两步都一致 |
| PY-111.5 | 策略在 `G1` 现货买入 0.01 BTC 成交(无永续腿)→ `WriteOffResidual{A1, G1}` 产生 `AdjustDelta::WriteOff` | 之后 `G1` 无非 0 库存(移到 `unassigned`);一致 |
| PY-111.6 | 对账报告(`BTC/P2` 仓位条目 `exch_qty = 0.5`、`engine_qty = 0` + 汇总)→ `AdoptExchange{A1, P2, BTC/P2, item = Position, G1, entry_px 60 000}` | 数量 0.5;一致 |

### PY-120 load_schemas 读取导出目录金样

- 规则:[概要](README.md#概要)、[参数编码(`params`)](README.md#参数编码params)
- 级别:unit
- 里程碑:M2
- 前置:`fixtures/strategy-schemas/`(`hft-core schemas --out` 的金样副本)
- 步骤:
  1. `m = hft_py.params.load_schemas("fixtures/strategy-schemas")`
- 期望:
  - `list(m.keys())` 等于 `index.json` 各项的 `kind_id`(升序);每个 `m[k]` 的 `kind_id`、`name`、`schema_id`、`schema_version`、`schema_digest` 与 `index.json` 对应项相同,`fields` 的个数与名字顺序与该 schema 文件的 `fields` 相同
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_120_load_schemas_golden`

### PY-121 index.json 与 schema 文件不一致时抛 ParamsError

- 规则:[参数编码(`params`)](README.md#参数编码params)、[错误](README.md#错误)
- 级别:table
- 里程碑:M2
- 前置:每行从 `S_TP` 复制一份目录再按"改动"修改
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_121_load_schemas_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| PY-121.1 | `index.json` 改为对象 `{"items": [ … ]}` | `hft_py.ParamsError` |
| PY-121.2 | 删除 `65281-<name>.schema.json` | `hft_py.ParamsError` |
| PY-121.3 | `index.json` 中 `schema_id` 改为 65282 | `hft_py.ParamsError` |
| PY-121.4 | `index.json` 中 `schema_digest` 末位改变 | `hft_py.ParamsError` |
| PY-121.5 | 删除 `index.json` | `hft_py.ParamsError` |
| PY-121.6 | `index.json` 不是合法 JSON(截掉最后一个 `]`) | `hft_py.ParamsError` |
| PY-121.7 | 不改 | 返回含键 65281 的映射(对照) |

### PY-122 load_schema 读单个文件

- 规则:[参数编码(`params`)](README.md#参数编码params)
- 级别:table
- 里程碑:M2
- 前置:`S_TP` 的 schema 文件路径 `f`
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_122_load_schema_single`

| 行 | 输入 | 期望 |
|---|---|---|
| PY-122.1 | `load_schema(f)` | `kind_id == 65281`,`schema_version == 2`,`fields` 依次为 `open_th`、`cooldown_ms`、`pct_bin`、`max_qty`、`enabled`、`mode` |
| PY-122.2 | 内容不是 JSON 的文件 | `hft_py.ParamsError` |
| PY-122.3 | 删掉 `fields` 键的副本 | `hft_py.ParamsError` |
| PY-122.4 | 不存在的路径 | `hft_py.ParamsError` |

### PY-123 encode 成功时返回 blob 与空错误列表,头部由 schema 补全

- 规则:[Python API](README.md#python-api导入名-hft_py)、[参数编码(`params`)](README.md#参数编码params)
- 级别:unit
- 里程碑:M2
- 前置:`s = load_schemas(S_TP)[65281]`;`GT3_PY`;`defaults = {"open_th": "0.6%"}`;`overrides = [{"group_ix": 1, "field": "max_qty", "value": "0.5"}]`
- 步骤:
  1. `blob, errs = hft_py.params.encode(s, header=GT3_PY.header, groups=GT3_PY.groups, defaults=defaults, overrides=overrides)`
- 期望:
  - `errs == []`;`blob` 解出的 `StrategyParamsHdr`:`strategy = 3`、`account = 9`、`kind_id = 65281`、`schema_id = 65281`、`schema_ver = 2`、`source = Pipeline`、`n_groups = 3`、`version = 7`、`generated_at_ns = D`、`schema_digest` = `index.json` 中的摘要
  - body 默认值中 `open_th = 6_000_000`(0.6%),其余字段为 `TP` 默认值;覆盖表恰 1 项 `(group_ix 1, field_ix 3, 50_000_000)`(0.5 BTC,指数 −8)
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_123_encode_ok`

### PY-124 encode 遇到错误时返回 (None, 错误列表),不抛异常

- 规则:[参数编码(`params`)](README.md#参数编码params)
- 级别:table
- 里程碑:M2
- 前置:同 PY-123,每行只做所列改动;`TP`:`open_th` max = 100 000 000(10%),`cooldown_ms` 不是 per_group,`mode` 为 Enum(a/b/c)
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_124_encode_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| PY-124.1 | `defaults = {"open_th": "10.0000001%"}`(原始值 100 000 001,上界 + 1) | `(None, [e])`,`e.variant == "OutOfRange"`、`e.field == "open_th"`、`e.group_ix is None` |
| PY-124.2 | `defaults = {"open_th": "10%"}` | `errs == []`(上界含) |
| PY-124.3 | `overrides = [{"group_ix": 0, "field": "cooldown_ms", "value": "5000"}]` | `e.variant == "NotPerGroup"`、`e.group_ix == 0` |
| PY-124.4 | `defaults = {"mode": "z"}` | `e.variant == "BadEnum"`(SDK `parse_value` 对不认识的枚举名)、`e.field == "mode"` |
| PY-124.5 | `overrides` 的 `max_qty` 值 `"0.5"`,而 `groups` 不含 `qty_exp`/`px_exp` | `e.variant == "BadText"` |
| PY-124.6 | `defaults = {"nope": "1"}` | `e.variant == "UnknownField"`、`e.field == "nope"` |
| PY-124.7 | `overrides = [{"group_ix": 3, "field": "open_th", "value": "1%"}]`(只有 3 个 group) | `e.variant == "GroupIxOutOfRange"`、`e.group_ix == 3` |
| PY-124.8 | 同一 `(0, open_th)` 两项,值 `"1%"` 与 `"2%"` | `e.variant == "OverrideOrder"` |
| PY-124.9 | PY-124.1 与 PY-124.6 的改动同时 | `blob is None`;`len(errs) == 2`,变体集合 `{"OutOfRange", "UnknownField"}` |

### PY-125 float 值被拒,Decimal 与原始 int 被接受

- 规则:[参数编码(`params`)](README.md#参数编码params)
- 级别:table
- 里程碑:M2
- 前置:同 PY-123
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_125_value_types`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-125.1 | `encode(…, defaults={"open_th": 0.006})` | `TypeError` |
| PY-125.2 | `encode(…, defaults={"open_th": Decimal("0.006")})` | `errs == []`,`open_th` 原始值 6 000 000 |
| PY-125.3 | `encode(…, defaults={"open_th": 6_000_000})` | `errs == []`,原始值 6 000 000(与 PY-125.2 的 blob 逐字节相同) |
| PY-125.4 | `encode_cmd(s, "open", {"notional": 1000.0})` | `TypeError` |
| PY-125.5 | `encode_cmd(s, "open", {"notional": Decimal("1000")})` | 与 `{"notional": "1000"}` 的结果逐字节相同 |

### PY-126 decode(encode(x)) == x

- 规则:[参数编码(`params`)](README.md#参数编码params)
- 级别:prop
- 里程碑:M2
- 性质:对 `TP` schema,任意合法输入 x(header、groups、defaults、overrides,值以原始定点 int 给出)编码成功,且 `decode(S_TP, blob)` 返回的 `{header, groups, defaults, overrides}` 与 x 相等(覆盖表按 `(group_ix, field)` 排序后比较)
- 生成:group 数 0–8(id = `0x0300_0001` 起递增);每个字段的默认值在 `[min, max]` 内均匀抽取(含两端);覆盖项 0–12 条,只取 per_group 字段,`(group_ix, field)` 不重复;`version` 1–2^63−1;`source` 两种
- 参考:输入 x 本身
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_126_roundtrip`

### PY-127 decode 按映射或单个 schema 选择 schema

- 规则:[参数编码(`params`)](README.md#参数编码params)
- 级别:table
- 里程碑:M2
- 前置:`b` = PY-123 的 blob(`kind_id = 65281`);`m2 = load_schemas(S_TP2)`(含 65281 与 65282)
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_127_decode_schema_selection`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-127.1 | `decode(m2, b)` | 成功,`header["kind_id"] == 65281` |
| PY-127.2 | `decode({k: v for k, v in m2.items() if k != 65281}, b)` | `hft_py.ParamsError` |
| PY-127.3 | `decode(m2[65281], b)` | 成功,与 PY-127.1 相同 |
| PY-127.4 | `decode(m2[65282], b)` | `hft_py.ParamsError`(单个 schema 的 `kind_id` 与头不符) |
| PY-127.5 | `b` 的头 `schema_id` 改为 65282,`decode(m2, b')` | `hft_py.ParamsError` |

### PY-128 decode 的容器与 body 错误都抛 ParamsError

- 规则:[参数编码(`params`)](README.md#参数编码params)、[错误](README.md#错误)
- 级别:table
- 里程碑:M2
- 前置:`b` = PY-123 的 blob;每行改一处后 `decode(load_schemas(S_TP), b')`
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_128_decode_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| PY-128.1 | 截成 40 字节(短于 `StrategyParamsHdr`) | `hft_py.ParamsError` |
| PY-128.2 | `body_len` 改为原值 + 4(不是 8 的倍数) | `hft_py.ParamsError` |
| PY-128.3 | `schema_ver` 改为 3(新于 schema 的 2) | `hft_py.ParamsError` |
| PY-128.4 | body 的 `n_fields` 改为 5 | `hft_py.ParamsError` |
| PY-128.5 | 第 0 个 `GroupSpec` 的 `_rsv` 写非 0 | `hft_py.ParamsError` |
| PY-128.6 | 不改 | 成功(对照) |

### PY-129 旧 schema 版本的 body 按新 schema 解码,缺失字段取默认值

- 规则:[参数编码(`params`)](README.md#参数编码params)
- 级别:unit
- 里程碑:M2
- 前置:`fixtures/params/sdk-tp/` 中 `schema_ver = 1` 的旧 body 金样(5 个字段,无 `mode`)
- 步骤:
  1. `p = decode(load_schemas(S_TP), 旧 blob)`
- 期望:
  - `p["header"]["schema_ver"] == 1`;`p["defaults"]` 有 6 个字段,`mode == 0`(默认值),其余 5 个为金样 `v1_old.input.toml` 的值(`open_th` 6 000 000、`cooldown_ms` 25 000、`pct_bin` 100 000、`max_qty` 0、`enabled` 0)
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_129_old_body_defaults`

### PY-130 encode_cmd 按命令名编码载荷

- 规则:[Python API](README.md#python-api导入名-hft_py)、[参数编码(`params`)](README.md#参数编码params)
- 级别:table
- 里程碑:M2
- 前置:`s = load_schemas(S_TP)[65281]`;载荷布局 `{ver = 1, op, n, _pad[5]} + i64[n]` 小端
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_130_encode_cmd`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-130.1 | `encode_cmd(s, "open", {"notional": "1000"})` | 16 字节:`01 01 01 00 00 00 00 00` + `1_000_000_000` 的 i64 小端 |
| PY-130.2 | `encode_cmd(s, "close", {})` | 8 字节:`01 02 00 00 00 00 00 00` |
| PY-130.3 | `encode_cmd(s, "set_mode", {"m": "y"})` | 16 字节:`01 03 01 00 00 00 00 00` + `1` 的 i64 小端 |
| PY-130.4 | `encode_cmd(s, "open", {"notional": 10_000_000_000})`(原始值上界) | 成功,值字段 = 10 000 000 000 |
| PY-130.5 | `encode_cmd(s, "open", {"notional": 0})`(下界) | 成功,值字段 = 0 |

### PY-131 encode_cmd 的错误抛 ParamsError

- 规则:[参数编码(`params`)](README.md#参数编码params)
- 级别:table
- 里程碑:M2
- 前置:`s = load_schemas(S_TP)[65281]`;`t = load_schemas(S_TP2)[65282]`(`cmds` 为空)
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_131_encode_cmd_errors`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-131.1 | `encode_cmd(s, "flip", {})` | `hft_py.ParamsError` |
| PY-131.2 | `encode_cmd(s, "open", {})`(缺字段) | `hft_py.ParamsError` |
| PY-131.3 | `encode_cmd(s, "close", {"x": "1"})`(多字段) | `hft_py.ParamsError` |
| PY-131.4 | `encode_cmd(s, "open", {"notional": 10_000_000_001})` | `hft_py.ParamsError` |
| PY-131.5 | `encode_cmd(s, "open", {"notional": -1})` | `hft_py.ParamsError` |
| PY-131.6 | `encode_cmd(s, "set_mode", {"m": "z"})` | `hft_py.ParamsError` |
| PY-131.7 | `encode_cmd(t, "open", {"notional": "1"})` | `hft_py.ParamsError` |

### PY-132 本 crate 中不存在第二个参数编码器

- 规则:[参数编码(`params`)](README.md#参数编码params)
- 级别:unit
- 里程碑:M2
- 前置:扫描 `crates/hft-py/src/` 与 `crates/hft-py/python/hft_py/` 的源文件
- 步骤:
  1. 在 Rust 源中查找 `ParamsBodyHdr`、`ParamOverride` 的结构体字面量,以及 `src/params.rs` 中的 `to_le_bytes`(SPB1 容器头 `StrategyParamsHdr` 是 hft-event 类型,由本 crate 按 `header` 与 schema 填写,不在扫描之列);在 Python 源中查找 `struct.pack`、`to_bytes(`
  2. 在 `src/params.rs` 中查找对 `hft_strategy_sdk::params::{encode_body, decode_body, encode_cmd}` 的调用
- 期望:
  - 步骤 1 零处命中;步骤 2 三个函数各至少一处调用。向 `src/params.rs` 加入一行 `ParamOverride { … }` 构造的补丁副本上本测试失败
- 实现:`crates/hft-py/tests/source_scan.rs::py_132_no_second_encoder`

### PY-133 schema 的描述符以只读形式给出

- 规则:[参数编码(`params`)](README.md#参数编码params)
- 级别:unit
- 里程碑:M2
- 前置:`s = load_schemas(S_TP)[65281]`
- 步骤:
  1. 读取 `s.notes`、`s.report`、`s.cmds`
  2. `s.notes[0]["name"] = "x"`
- 期望:
  - 步骤 1:`notes` 3 项(码 1 `EchoFire` Info、2 `EchoOverflow` P2、3 `EchoNoFeed` P2);`report` 1 项(`fires`、`agg = Count`、`scope = Group`、`note = 1`、`v_ix = 0`);`cmds` 3 项(op 1 `open`、2 `close`(`reduce_only` 为真)、3 `set_mode`)
  - 步骤 2 抛 `TypeError`;再次读取 `s.notes[0]["name"] == "EchoFire"`
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_133_descriptors_read_only`

### PY-134 parse_value 按字段单位解析文本并查范围,format_value 与之往返

- 规则:[参数编码(`params`)](README.md#参数编码params)、[Python API](README.md#python-api导入名-hft_py)
- 级别:table
- 里程碑:M2
- 前置:`s = load_schemas(S_TP)[65281]`(`open_th` Rate [0, 100 000 000];`cooldown_ms` DurationMs [0, 600 000];`max_qty` Qty [0, 10^12];`mode` Enum a/b/c;`enabled` Bool)
- 实现:`crates/hft-py/tests/python/test_params.py::test_py_134_parse_format_value`

| 行 | 调用 | 期望 |
|---|---|---|
| PY-134.1 | `parse_value(s, "open_th", "50bp")` | `5_000_000` |
| PY-134.2 | `parse_value(s, "open_th", "0.5%")` | `5_000_000` |
| PY-134.3 | `parse_value(s, "open_th", "10%")`(上界) | `100_000_000` |
| PY-134.4 | `parse_value(s, "open_th", "10.0000001%")`(上界 + 1 raw) | `ParamsError`,`variant = "OutOfRange"` |
| PY-134.5 | `parse_value(s, "open_th", "-1bp")` | `ParamsError`,`variant = "OutOfRange"` |
| PY-134.6 | `parse_value(s, "cooldown_ms", "5m")` | `300_000` |
| PY-134.7 | `parse_value(s, "cooldown_ms", "600001")` | `ParamsError`,`variant = "OutOfRange"` |
| PY-134.8 | `parse_value(s, "mode", "b")` | `1` |
| PY-134.9 | `parse_value(s, "enabled", "true")` | `1` |
| PY-134.10 | `parse_value(s, "max_qty", "0.001", exp=-8)` | `100_000` |
| PY-134.11 | `parse_value(s, "max_qty", "0.001")`(缺指数) | `ParamsError`,`variant = "BadText"` |
| PY-134.12 | `parse_value(s, "open_th", "abc")` | `ParamsError`,`variant = "BadText"` |
| PY-134.13 | `parse_value(s, "nosuch", "1")` | `ParamsError`,`variant = "UnknownField"` |
| PY-134.14 | `format_value(s, "open_th", 5_000_000)` | 与 Rust `params::format_value` 对同一字段与值的输出逐字节相同;再 `parse_value` 得 `5_000_000` |
| PY-134.15 | `format_value(s, "mode", 2)` | `"c"` |

### PY-140 公开接口中没有任何写入 API

- 规则:[概要](README.md#概要)、[Python API](README.md#python-api导入名-hft_py)
- 级别:table
- 里程碑:M2
- 前置:不带 `testing` 特性构建的 wheel 装在 `HFT_PY_RELEASE_PYTHON` 指向的解释器里(各行在该解释器的子进程中读取);"公开名字"= 不以 `_` 开头的属性
- 实现:`crates/hft-py/tests/python/test_surface.py::test_py_140_no_write_api`

| 行 | 对象 | 期望的公开名字集合 |
|---|---|---|
| PY-140.1 | `hft_py` | `{open_journal, ledger, pnl, params, heartbeat, decode, schema, schema_hash, JournalError, SchemaError, ParamsError}` |
| PY-140.2 | `open_journal(J_MD)` 返回的对象 | `{info, events, tail, config_at}` |
| PY-140.3 | `ledger(…)` 返回的对象 | `{positions, inventory, portfolio, breakdown}` |
| PY-140.4 | `hft_py.params` | `{load_schemas, load_schema, encode, decode, encode_cmd, parse_value, format_value, Error}` |
| PY-140.5 | `hft_py.heartbeat` | `{read, Watch}` |
| PY-140.6 | `hft_py.decode` | `{control_cmd, config_blob, state_snapshot, advice, raw_rx, raw_tx}` |
| PY-140.7 | `hft_py.schema` | `{catalog}` |
| PY-140.8 | 以上全部 | 没有名字匹配 `write\|append\|publish\|commit\|inject\|send\|create\|delete\|set_` |
| PY-140.9 | 发布 wheel 的 `hft_py` | 没有 `_testing` 属性 |

### PY-141 依赖图加入禁止的依赖时 cargo xtask deps 失败

- 规则:[概要](README.md#概要)、[依赖](README.md#依赖)
- 级别:table
- 里程碑:M2
- 前置:仓库的临时副本;每行只在 `crates/hft-py/Cargo.toml` 的 `[dependencies]` 加入所列依赖后运行 `cargo xtask deps`;`[dev-dependencies]` 中的策略包由 TOOL-031.37 覆盖
- 实现:`tools/xtask/tests/deps.rs::py_141_forbidden_deps`

| 行 | 加入的依赖 | 期望 |
|---|---|---|
| PY-141.1 | `hft-journal = { features = ["writer"] }` | 退出 1,输出点名 `hft-journal/writer` |
| PY-141.2 | `hft-sequencer` | 退出 1 |
| PY-141.3 | `hft-sim` | 退出 1 |
| PY-141.4 | `hft-net` | 退出 1 |
| PY-141.5 | `hft-venue` | 退出 1 |
| PY-141.6 | `strategies/x` | 退出 1 |
| PY-141.7 | `hft-strategy-registry` | 退出 1 |
| PY-141.8 | 不加 | 退出 0(含已登记的 `hft-risk`、`hft-ledger`、`hft-rt`) |

### PY-142 hft-py 的特性闭包不含 hft-journal 的 writer

- 规则:[依赖](README.md#依赖)
- 级别:proc
- 里程碑:M2
- 前置:CI arm64 runner;`--target aarch64-unknown-linux-gnu`
- 步骤:
  1. `cargo tree -p hft-py -e features`
  2. `cargo tree -p hft-py -e features --features testing`
- 期望:
  - 两次输出中都没有 `hft-journal feature "writer"`;都含 `hft-journal` 的默认特性(读者)
- 实现:`tools/xtask/tests/deps.rs::py_142_no_writer_feature`

### PY-143 类型存根与运行时模块一致

- 规则:[3. 源文件](README.md#3-源文件)
- 级别:proc
- 里程碑:M2
- 前置:不带 `testing` 特性构建的 wheel(含 `python/hft_py/*.pyi`)装在 `HFT_PY_RELEASE_PYTHON` 指向的解释器里,并装有 mypy
- 步骤:
  1. `python -m mypy.stubtest hft_py`
  2. 对删去 `heartbeat.Watch` 声明的存根副本(放在 `MYPYPATH` 下)再运行一次
- 期望:
  - 步骤 1 退出 0;步骤 2 退出非 0 且输出点名 `hft_py.heartbeat.Watch`
- 实现:`crates/hft-py/tests/python/test_surface.py::test_py_143_stubtest`

### PY-150 wheel 的文件名、标签与归档位置

- 规则:[4. 打包与发布](README.md#4-打包与发布)
- 级别:proc
- 里程碑:M2
- 前置:CI 发布流水线在 arm64 runner 上以 `maturin` 构建本版本(版本号 v)
- 步骤:
  1. 列出产物;`auditwheel show <wheel>`
  2. 读发布归档的 `RELEASE.json`
- 期望:
  - 产物文件名为 `hft_py-v-cp310-abi3-manylinux_2_28_aarch64.whl`;`auditwheel` 报告平台标签 `manylinux_2_28_aarch64`
  - wheel 位于归档的 `python/` 目录,`RELEASE.json` 中该文件的 SHA-256 与文件实际 SHA-256 相同
- 备注:在发布工作流(`release-aarch64.yml`,`.github/scripts/build-wheels.sh`)中运行;`manylinux_2_28` 标签与"wheel 版本 = 发布版本"需要该流水线以 manylinux 容器或 zig 构建并按发布标签设置版本,接入之前本用例未实现
- 实现:`crates/hft-py/tests/python/test_wheel.py::test_py_150_wheel_name_and_archive`

### PY-151 wheel 在干净的按版本 venv 中离线安装并通过冒烟

- 规则:[4. 打包与发布](README.md#4-打包与发布)
- 级别:table
- 里程碑:M2
- 前置:CI arm64 runner;按 `/opt/hft/releases/<v>/venv` 的结构新建空 venv;`pip install --no-index --find-links <归档>/python hft_py`;冒烟 = `open_journal(J_MD).events("Bbo")` 行数、`params.load_schemas("fixtures/strategy-schemas")`、`heartbeat.read` 读 `HB`
- 备注:在发布工作流(`release-aarch64.yml`,`.github/scripts/build-wheels.sh`)中运行;`manylinux_2_28` 标签与"wheel 版本 = 发布版本"需要该流水线以 manylinux 容器或 zig 构建并按发布标签设置版本,接入之前本用例未实现
- 实现:`crates/hft-py/tests/python/test_wheel.py::test_py_151_offline_install_smoke`

| 行 | 解释器 | 期望 |
|---|---|---|
| PY-151.1 | Python 3.12 | 安装不访问网络(`pip` 退出 0,`--no-index`);冒烟得到 120 000 行、映射非空、`state` 为 `"Starting"` |
| PY-151.2 | Python 3.10(abi3 下限) | 同 PY-151.1 |
| PY-151.3 | Python 3.12,`--find-links` 目录中删去 wheel | `pip` 退出非 0 |

### PY-152 与 python/* 各包一起装进同一个按版本的 venv

- 规则:[4. 打包与发布](README.md#4-打包与发布)
- 级别:proc
- 里程碑:M4
- 前置:发布归档(含 `python/` 下全部 wheel 与合并的 `requirements.lock`);按 deploy 的安装步骤在干净目录建该版本的单一 venv
- 步骤:
  1. `pip install --no-index --require-hashes -r requirements.lock`
  2. 依次 `import hft_py, hft_pipeline, hft_ops_bot, hft_treasury`;`pip check`
- 期望:
  - 步骤 1 退出 0;步骤 2 全部导入成功,`pip check` 退出 0;venv 中 `hft_py` 只有一个安装版本
- 实现:`crates/hft-py/tests/python/test_wheel.py::test_py_152_single_release_venv`

### PY-153 wheel 版本等于发布版本

- 规则:[4. 打包与发布](README.md#4-打包与发布)
- 级别:proc
- 里程碑:M2
- 前置:PY-151.1 的 venv;同一归档的 `RELEASE.json`
- 步骤:
  1. `importlib.metadata.version("hft_py")`
- 期望:
  - 结果等于 `RELEASE.json` 的 `version`
- 备注:在发布工作流(`release-aarch64.yml`,`.github/scripts/build-wheels.sh`)中运行;`manylinux_2_28` 标签与"wheel 版本 = 发布版本"需要该流水线以 manylinux 容器或 zig 构建并按发布标签设置版本,接入之前本用例未实现
- 实现:`crates/hft-py/tests/python/test_wheel.py::test_py_153_version_matches_release`

### PY-160 events() 定长 kind 扫描与转置吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`py_events`(数值上限只在 [benchmarks](../../benchmarks/README.md) 登记的预算出处,即本 crate"延迟与资源契约")
- 前置:合成 `Bbo` 1 000 000 条的已封存段(冷副本在 tmpfs),`batch_rows = 1 000 000`,单核
- 实现:`crates/hft-py/benches/py_events.rs::py_160_events_scan_throughput`

### PY-161 每批内存不超过 batch_rows × 行宽 × 2,且与 journal 总量无关

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M2
- 前置:本测试单独一个测试二进制;合成 `Bbo` 写入已封存段;`batch_rows = 100 000`;行宽 = 查询的 Arrow schema 各列每行字节数之和;测量迭代期间匿名驻留量的峰值增量(每次测量在独立子进程中先对同一份 1M 行夹具做一遍不计入的预热扫描,采样线程完成首次读取后才取基线;每行取三次独立测量中的最大值,读完每批即采样并丢弃;采样线程每次读取 `/proc/self/smaps_rollup` 的 `Anonymous` 后间隔 200 µs,读取或字段解析失败则测试失败,不以 0 代替;不使用 `/proc/self/status` 的异步 RSS 计数;计数分配器需要 `unsafe impl GlobalAlloc`,本 crate 不允许 `unsafe`,hft-testkit 的 `CountingAlloc` 只计次数)
- 实现:`crates/hft-py/tests/memory.rs::py_161_batch_memory_bound`

| 行 | 段中 `Bbo` 条数 | 期望 |
|---|---|---|
| PY-161.1 | 1 000 000 | 峰值增量 ≤ 100 000 × 行宽 × 2 |
| PY-161.2 | 2 000 000 | 峰值增量 ≤ 100 000 × 行宽 × 2,且与 PY-161.1 之差 < 5 % |
| PY-161.3 | 1 000 000,`batch_rows = 1 000 000` | 峰值增量 ≤ 1 000 000 × 行宽 × 2 |

### PY-162 一天的 seq journal 计算 ledger 用时少于 2 分钟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M4
- 环境:dev2,`hft-core@sim` 已连续运行 ≥ 1 个完整 UTC 日;以 `hft-journal-ro` 组的运维用户在 `hftbatch.slice` 中运行(`systemd-run --slice=hftbatch.slice`),解释器为 `/opt/hft/current-cold/venv`
- 授权:[需授权]
- 步骤:
  1. `time python -c "import hft_py; j = hft_py.open_journal('<cold_root>/sim/seq', hot_root='<hot_root>'); hft_py.ledger(j, account='<模拟账户>', t0='<日期>T00:00:00Z', t1='<日期+1>T00:00:00Z').breakdown()"`
- 通过标准:墙钟时间 < 120 s;进程退出 0;记录该日 seq journal 的字节数与记录数
- 证据:`ops-log/hft-py/PY-162-<日期>.txt`(`time` 输出、journal 字节数与记录数、主机类)
- 实现:`crates/hft-py/tests/host/PY-162.sh`

### PY-112 常规成交经 JournalLedger 按 cum_filled 去重入账

- 规则:[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:table
- 里程碑:M2
- 前置:各行一个 `F_FILLS` 夹具(`SEQ_FX`,账户 A1):策略下单得到 Output `OrderDecision{New}` 槽 (epoch 3, slot 1, gen 1) → (A1, `BTC/P2`, Buy 0.5, `G1`),`Ack` 之后为步骤 `placed`,再按行投递 `OrderEvent`(`detail = 0`,即回报不带方向);比较对象为 `hft_py.ledger(j, account="bn-1").positions()` 中 `BTC/P2` 自 `placed` 起的变化,以及 `placed` 与 `end` 两步与热进程一致(三节字节含 `G1` 的永续腿)
- 实现:`crates/hft-py/tests/python/test_ledger.py::test_py_112_fills_dedup_by_cum_filled`

| 行 | 槽 (3, 1, 1) 的 `OrderEvent` 序列(`cum_filled`) | 期望 |
|---|---|---|
| PY-112.1 | PartialFill 0.2 → Fill 0.5 | +0.5,归 `G1`;与热进程一致 |
| PY-112.2 | PartialFill 0.2 → 重复的 PartialFill 0.2 → Fill 0.5 | +0.5(重复的 0.2 不再入账);一致 |
| PY-112.3 | Fill 0.5 → 乱序到达的 PartialFill 0.2 | +0.5(`cum_filled` 不增加的回报不入账);一致 |
| PY-112.4 | 槽 (3, 1, 2)(gen 不同,无对应 `OrderDecision{New}`)的 Fill 0.5 | 不按槽 (3, 1, 1) 入账(热进程按 `StaleGen` 处理,回报无方向,不入账);一致 |

### PY-113 本 crate 不含第二个入账实现

- 规则:[2. ledger / pnl](README.md#2-ledger--pnl)
- 级别:unit
- 里程碑:M2
- 前置:扫描 `crates/hft-py/src/` 与 `crates/hft-py/python/hft_py/`
- 步骤:
  1. 在 `src/ledger.rs` 中查找对 `hft_ledger::JournalLedger` 的使用
  2. 在全部源文件中查找对 `InstPos`/库存字段的算术更新与按事件 kind 分派的入账代码
- 期望:
  - 步骤 1 至少一处;步骤 2 零处命中。向 `src/ledger.rs` 加入一行 `pos.qty += …` 的补丁副本上本测试失败
- 实现:`crates/hft-py/tests/source_scan.rs::py_113_no_second_booking`

## 验收

- 本文件里程碑 ≤ 当前里程碑的用例全部有同编号实现且通过(M2 起;wheel 与发布 venv 的用例在每次发布的 CI 作业中运行,PY-152 自 M4 起)。
- 属性测试 PY-041、PY-126 的例数:PR 2 000;每夜 100 000;一次失败即失败,种子写进回归集。
- 冷读者与热进程的账本一致性在 DST 中由 LED-080、LED-081 累计(门槛 ④,累计量见 [testing §8](../../docs/testing.md#8-上线门槛));本 crate 经 PyO3 的同一路径由 PY-106–PY-113 与 LED-246 在每个 PR 上验证。
- `py_events` 的数值按 [benchmarks](../../benchmarks/README.md) 的规则在参考机判定;PR 上只报告。
- PY-162 在 M4 首次于 dev2 执行并留证据;之后每次升级主机类或 hft-ledger 有性能相关改动时复测。
