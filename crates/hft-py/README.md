---
description: "PyO3 绑定:运维与研究的 Python 通过它只读 journal(→ Arrow)、用与热进程同一份代码算 ledger/PnL、经 SDK params 编解码策略参数 blob(pipeline 的唯一编码途径);aarch64 wheel"
kind: crate
tier: cold
scope: core
status: impl
---

# hft-py

## 概要

`hft-py` 把 Rust 的只读能力交给 Python,core 有三件事:**reader**——`open_journal(...).events(...)` 把 seq/raw journal 按 kind、inst、时间窗读成 Arrow `RecordBatch`,经 Arrow PyCapsule 零拷贝交给 Polars/DuckDB/pyarrow,`tail()` 读尾部;**ledger**——`ledger`/`pnl` 用 [`hft-ledger`](../hft-ledger/README.md) 的同一套记账代码(含现货库存、净量与按 venue 的权益视图数据),模拟账户与实盘账户同样适用;**params**——包装 [`hft-strategy-sdk`](../hft-strategy-sdk/README.md) 的纯模块 `params`(参数规范编解码的唯一实现,hft-cfg 包装的也是它),按 `hft-core schemas --out <dir>` 导出的 schema JSON 编码与解码 `StrategyParams` blob;pipeline 只经它编码,Python 侧没有另一份编码器;它不依赖任何策略包。使用方是 [`python/pipeline`](../../python/pipeline/README.md) 框架与随策略包的 pipeline 插件(`strategies/<name>/pipeline/`,编码参数与预热载荷)、[`python/ops_bot`](../../python/ops_bot/README.md)(读尾部、PnL 面板)、[`python/treasury`](../../python/treasury/README.md)(读余额与保证金事件)、[`python/research`](../../python/research/README.md)(optional,分析模拟账户结果)。发控制命令**不在本 crate**:唯一的 Python ctl 客户端是 [hft-ctl](../hft-ctl/README.md) 的纯标准库 `hftctl.py`。**Python 永不写 journal**:本 crate 只链接 `hft-journal` 的读者,不存在任何写入 API;Python 对热进程唯一的写入口是控制命令,经 ctl-gateway。回放不在本 crate:验证回放与模拟重演用 [replayer](../../apps/replayer/README.md) 命令行。它是冷进程里的库,不在任何热路径上。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### Python API(导入名 `hft_py`)

```python
import hft_py, polars as pl

j = hft_py.open_journal("<cold_root>/prod/seq", hot_root="<hot_root>")   # <cold_root>/<instance>/<journal>(instance ∈ {prod, sim})或单个段文件;
                                                           # hot_root:RAM 热段根(热段在 <hot_root>/<instance>/<journal>/),省略时只读冷副本
j.info()          # {'journal_name', 'instance', 'segments': [...], 'first_seq', 'last_seq', 'schema': {...}, 'host_profile': {...}}

rdr = j.events("Bbo", inst=[101, 202], t0="2026-10-01T00:00:00Z", t1="2026-10-01T01:00:00Z", time="proc")
df = pl.DataFrame(rdr)                                     # PyCapsule __arrow_c_stream__,pyarrow 缓冲直接交给 Polars,无复制

for batch in j.tail(["OrderEvent", "ExternalFill", "MarginReport", "CmdResult"], from_seq=None, poll_ms=200):   # ops_bot/treasury 读尾部;batch 是 hft_py 的 RecordBatch
    ...

blob = j.config_at(seq=123456)                             # 该 seq 时刻生效的 ConfigBlob(refdata/风控/引擎配置)解码为 dict

led = hft_py.ledger(j, account="sim-a", t0=..., t1=...)     # 与热进程同一记账代码;实盘与模拟账户同样适用(模拟账户只在 sim 实例的 journal)
led.positions(); led.inventory(); led.portfolio()          # 永续仓位;现货库存(按 group 或 unassigned);按 venue 的权益视图数据
led.breakdown()                                            # [t0, t1) 内入账的 PnlBreakdown 之和(一行)
hft_py.pnl(j, by="round")                                  # 按开平一轮;by ∈ {"group","account","round","day"}

schemas = hft_py.params.load_schemas("<schema_dir>")        # hft-core schemas --out <dir> 的输出:index.json + <kind_id>-<name>.schema.json
schema = schemas[kind_id]                                   # 也可 hft_py.params.load_schema("<schema_dir>/<kind_id>-<name>.schema.json")
blob, errors = hft_py.params.encode(schema, header={...}, groups=[...], defaults={...}, overrides=[...])
hb = hft_py.heartbeat.read("<instance>")   # 读 hft-rt HeartbeatBlock(只读),ops_bot 判断 hft-core 是否健康以决定 /stop 走 gateway 还是 kill-switch 本地触发
w = hft_py.heartbeat.Watch("<instance>"); w.check()   # {"state", "advanced", "age_ms"}:心跳年龄(见"参数语义"下的心跳一条)
v = hft_py.params.parse_value(schema, "open_th", "50bp")      # 文本 → 原始定点值(SDK params::parse_value + [min, max]);ops_bot 的参数输入用它
txt = hft_py.params.format_value(schema, "open_th", v)           # 原始值 → 显示文本(SDK params::format_value)
cmd = hft_py.params.encode_cmd(schema, op="close", values={...})   # StrategyCmd 载荷(SDK params::encode_cmd;op 与字段来自 schema 的 cmds)
# 只做 schema 层校验(字段存在、单位、[min, max]);产出经 hftctl.py 以 SetParams 下发,策略自己的 validate 在 ctl-gateway 执行
params = hft_py.params.decode(schemas, blob)                # blob → {header, groups, defaults, overrides};按头里的 kind_id 选 schema(差异报告与展示用)

hft_py.schema_hash(); hft_py.schema.catalog()              # 生成器导出的事件表(与 docs/generated/event-catalog.md 同源)
```

### 参数语义

| 参数 | 类型 | 语义 |
|---|---|---|
| `kind` | `str` 或 `int` | 事件 kind 名或编号([event-model](../../docs/subsystems/event-model.md));未知 → `ValueError` |
| `inst` | `int`、序列或 `None` | 只取这些 `InstId`;对无 inst 的 kind 传值 → `ValueError` |
| `t0`, `t1` | ns 整数、带时区 `datetime`、带时区(`Z` 或 `±hh:mm`)的 ISO 字符串 | 半开区间 `[t0, t1)`;无时区的 `datetime` 与 ISO 字符串一律拒绝(`ValueError`);`t0 > t1` → `ValueError`,`t0 = t1` 为空结果 |
| `time` | `"proc"`(记录头 tsc 按 [event-model](../../docs/subsystems/event-model.md) 的 TSC → UTC 换算:段头校准对与 `ClockStatus`)、`"ex"`(`ex_ts_ns`)、`"hw"`(`hw_rx_ns`) | 过滤所用时间;不同来源不混用([latency-budget §5](../../docs/latency-budget.md)):所选字段为 0(无该来源的戳)的记录不出现在结果中,不回退到别的来源;kind 没有该字段 → `ValueError` |
| `account` | 账户名(`AccountMeta.label`;模拟账户以 `sim-` 开头)或 `AccountId` | 按账户过滤;模拟账户与实盘账户都可,结果带 `account_kind` 列;对无 account 字段的 kind 传值、名字解析不出 → `ValueError` |
| `columns` | 列名序列 | 只物化这些列(按给出的顺序);未知列名 → `ValueError` |
| `decimals` | `bool`,默认 `False` | `True` 时额外给出 `*_f` 浮点列(按资产指数换算),**只供展示与研究** |
| `batch_rows` | `int`,默认 1 000 000 | 每批行数上限;批在段尾同样结束;< 1 → `ValueError` |
| `hot_root` | 路径或 `None`(`open_journal`) | RAM 热段根;`None` 时只读冷副本(看不到尚未同步的尾部) |
| `from_seq`、`poll_ms` | `int` 或 `None`;`int`,默认 200(`tail`) | `from_seq = None` 只交付打开之后新提交的记录,`= n` 从 seq n 起(早于最早可读记录或晚于 `last_seq + 1` → `ValueError`);`poll_ms` 不在 1–86 400 000 → `ValueError`;两次检查之间释放 GIL,并处理 Python 信号 |

- **批对象**:迭代 `events()` 与 `tail()` 得到 `hft_py` 的 `RecordBatch`(`num_rows`、`column_names`、`metadata`、`b["列名"]` 给出该列的 Python 值列表;`__arrow_c_array__` 交给 `pyarrow.record_batch(b)`/`pl.DataFrame(b)`);`events()` 的读者另有 `__arrow_c_stream__`(每次调用从头读同一查询,DuckDB 等对同一对象多次取流也可)与 `__arrow_c_schema__`。
- **`tail(kinds, …)`**:按 seq 顺序把新记录切成连续的同 kind 段,每段产出一个 `RecordBatch`(schema 元数据 `kind` = kind 名);没有新记录时等待 `poll_ms` 后再查,不产出空批;只交付 `committed_len` 之内的完整事件;`kinds` 含未知 kind → `ValueError`。
- **`config_at(seq)`**:返回 `seq` 时刻生效的配置:该运行的引导块 `ConfigBlob{boot = 1}`,叠加其后到 `seq` 为止得到 `CmdResult{Applied}` 的 `SetParams`/`SetRisk` 所引用的 blob(按 `BlobRef.sha256`,[hft-event](../hft-event/README.md));`ReplaceRefdata` 的 blob 到下一次引导块才生效。结果按 `BlobKind`,每项是带 `seq`(生效位置:引导块中的记录或 `Applied` 的 `CmdResult`)、`sha256`(十六进制)、`version`(`BlobRef.version`)与 `encoding` 的 dict:`refdata` 另有资产、inst、账户、策略实例表(经 hft-refdata 加载);`engine` 另有 `host_class` 与 `sections`(节名 → 原始字节;各节行格式归各自 crate);`strategy_params` 按 `"<实例名>/<账户 label>"` 给出 `hdr`、`groups`(`GroupSpec` 字段)、`body`(原始字节,按 schema 解码用 `params.decode`)与 `version`;`risk`、`host_manifest`、`sim_config` 另有 `bytes`(原始字节:`RiskParams` 的 blob 编解码尚未在 hft-risk 提供)。`seq` 早于第一个完整引导块 → `ValueError`。
- **`ledger(j, account, t0, t1)`**:从 `t0` 之前(含)最近一条带 ledger 节的 `StateSnapshot` 起步(没有则从最早可读段的引导块:其中的 `StateSnapshot`,或在 `BootDone` 处以空账本起步),逐事件消费到 `t1` 之前(事件按首条记录的 `proc_ns` 归入窗口);起步所用配置是该点生效的配置(refdata、`EngineConfig` 的 `ledger` 节,没有则 `LedgerConfig::default`;各策略参数 blob 的 `GroupSpec` 经 `on_groups_changed` 交给账本,之后 `Applied` 的 `SetParams` 与新引导块同样更新);快照不含估值价与资金费周期表,所以从快照起步时,窗口内依赖它们的估值要等到新的行情与资金费事件才与从头起步的结果一致;`positions()`/`inventory()`/`portfolio()` 是 `t1` 时的状态,`breakdown()` 是 `[t0, t1)` 内入账的 `PnlDelta` 之和(`PnlBreakdown` 各列,USDT 1e-6 原值);每个事件的 `PnlDelta` = 该事件前后各 (账户, group) 累计 `pnl` 之差(group 取当时生效的全部 `GroupSpec` 与 `GROUP_NONE`)。四个方法都返回 `RecordBatch`,带 `account` 与 `account_kind` 列:`positions` 为非零的 `InstPos`(`venue`、`inst`、`qty`、`cost`/`realized_quote` 为 `decimal128(38, 0)`、`fees_quote`、`last_fill_ens`),`inventory` 为 `venue`、`asset`、`group`、`unassigned`(未分配给 group 的部分)、`qty`,`portfolio` 为各 venue 的 `VenueEquityView` 字段(`flags` 中的 `BALANCE_UNRECONCILED` 恒不置位:冷读者不做静默期余额校验,[hft-ledger 冷消费者](../hft-ledger/README.md#冷消费者journalledger))。`pnl(j, by)` 对整份 journal 与全部账户,每行是该键下 `PnlDelta` 之和:`group` 键 (账户, group),`account` 键账户,`round` 键 (账户, group, 事件结束时该 group 的 `round_id`),`day` 键 (账户, 事件 `proc_ns` 的 UTC 日,`YYYY-MM-DD`);`by` 取其他值或不是字符串 → `ValueError`。账本容量(`Capacities`)取固定值(`max_cells_total` 8 192、`max_groups_per_instance` 1 024 等,`max_insts` 按 refdata):ECF1 段 3 的行格式归 hft-sequencer,尚无解码。
- **心跳**:`heartbeat.read(instance, root="/dev/shm/hft")` 只读映射 `<root>/<instance>/heartbeat`(`HeartbeatBlock::open_ro`,布局归 [hft-rt](../hft-rt/README.md)),返回 `{"state"(`Starting`/`Warmup`/`Running`/`Draining`/`Stopped`), "seq_beats"(区 0 计数), "pid", "instance_id", "boot_id"(十六进制), "started_utc_ns", "hk_utc_ns", "n_slots", "mono_ns"(读取时的 `CLOCK_MONOTONIC`)}`;文件不存在 → `FileNotFoundError`,magic 或 `layout_version` 不符 → `SchemaError`(每次读取都复查;同一文件只映射一次,文件被换成新的 inode 时重新映射)。`Watch(instance, root=…).check()` 返回 `{"state", "advanced", "age_ms"}`:`age_ms` = 本 `Watch` 最近一次看到 `seq_beats` 变化(首次 `check` 记基线并视为变化;`pid` 或 `boot_id` 变化同样视为变化)以来的单调时钟毫秒数,规则同 hft-rt `HeartbeatWatch`;判活口径(`state` 与 `hb_stale_ms`)归 hft-rt。

### 参数编码(`params`)

- `params.encode`/`params.decode` 包装 SDK 纯模块 `params` 的规范编解码:产出 `StrategyParamsHdr` + `[GroupSpec]`(线上头归 [hft-event](../hft-event/README.md))+ body(body 布局归 [hft-strategy-sdk](../hft-strategy-sdk/README.md):按 `FieldDesc` 顺序的 i64 小端默认值 + `(group_ix, field_ix, value)` 覆盖表)。[`hft-cfg`](../../tools/README.md#hft-cfg) 链接的是同一份 Rust 代码,因此 blob 只有一个编码器;`header` 含 `group_ctr_hwm`,取值由调用方按 [hft-cfg 的 GroupId 续号与水位](../../tools/README.md#hft-cfg) 给出,本模块只编码、不检查续号;[`python/pipeline`](../../python/pipeline/README.md) 只经本模块编码,不写纯 Python 编码器。
- 输入的 schema 由 `hft-core schemas --out <dir>` 导出(随发布制品附带):每个策略一份 `<kind_id>-<name>.schema.json`(`ParamSchemaDesc`,另含 `StrategyDesc.notes`/`report` 描述符)+ `index.json`(数组,每项 `{kind_id, name, file, schema_id, schema_version, schema_digest}`,按 `kind_id` 升序;格式唯一归属 [strategies/registry](../../strategies/registry/README.md))。`load_schemas` 校验 index 与各文件一致(文件存在、`kind_id`/`name`/`schema_id`/`schema_version` 与 `schema_digest` 相符),不符 → `ParamsError`;返回 `kind_id` → schema 的 dict;schema 对象的 `fields`、`cmds`、`notes`、`report` 是只读映射(`types.MappingProxyType`)的元组,`schema_digest` 为 16 位小写十六进制;各策略的 `notes`/`report` 描述符以只读 dict 给出,供研究与 ops_bot 解读策略 Note。本 crate 不链接任何策略包,新增策略不需要改本 crate。
- 只做 schema 层校验:字段名存在、单位与类型匹配、值在 `[min, max]`、`per_group` 字段只出现在覆盖表。策略自己的 `validate`(跨字段、按 refdata)在 ctl-gateway 用装配处的 `StrategySet::decode_params` 执行,失败以 `CmdResult` 返回。
- `encode()` 返回 `(bytes | None, list[Error])`:有任何错误都不产出 blob,不抛异常。`Error` 有 `variant`(SDK `ParamError` 的变体名,如枚举名不认识为 `BadEnum`;字段名不在 schema 中为 `UnknownField`,在调用 SDK 之前判定;SDK `encode_body` 只报第一个错误)、`field`(字段名或 `None`)、`group_ix`(或 `None`)。SPB1 容器头 `StrategyParamsHdr` 由本模块按 `header` 与 schema 填写后与 `GroupSpec`、SDK `encode_body` 的 body 拼接,并经 hft-event `parse_spb1` 复核;`group_ctr_hwm` 省略时取 `groups` 中最大的 group 序号。`header` 给 `strategy`、`account`(id)、`version`、`source`(`"Pipeline"`/`"Manual"`)、`generated_at_ns`,`kind_id`、`schema_id`、`schema_ver`、`schema_digest` 由 schema 填;`groups` 每项给 `GroupSpec` 字段(`id`、`asset`、`open_allowed`、`legs`)与可选的 `qty_exp`/`px_exp`;`defaults` 是字段名 → 值,`overrides` 每项 `{"group_ix", "field", "value"}`。字段值接受 `str`(经 SDK `parse_value`,与 hft-cfg 的 TOML 文本同一写法)、`int`(原始定点值)与 `decimal.Decimal`;`Qty`/`Px` 字段的文本与 `Decimal` 值需要 `groups[i]` 给出 `qty_exp`/`px_exp`,缺 → `BadText`;`float` → `TypeError`(调用错误,不进列表)。
- `decode(schemas, blob)` 的第一个参数是 `load_schemas` 返回的映射(按头里的 `kind_id` 选,映射里没有 → `ParamsError`)或 `load_schema` 返回的单个 schema(其 `kind_id`/`schema_id` 与 blob 头不符 → `ParamsError`);容器错误(`parse_spb1`)、`schema.id` 不符、版本更新或 body 越界 → `ParamsError`;`decode(encode(x)) == x`。
- `encode_cmd(schema, op, values)`:`op` 是 `CmdDesc.name`,`values` 按字段名给出,值类型同 `encode`(`float` → `TypeError`);命令名不在该 schema 的 `cmds` 中、缺字段、多字段、越界 → `ParamsError`(一条命令只有一个结果,不返回错误列表)。
- `parse_value(schema, field, text, exp=None) -> int`:按字段名调 SDK `params::parse_value`(`Qty`/`Px` 字段需要 `exp`),再查 `[min, max]`;字段名不在该 schema 的参数字段中 → `ParamsError`(`UnknownField`),不能解析 → `ParamsError`(`BadText`),越界 → `ParamsError`(`OutOfRange`)。`format_value(schema, field, raw, exp=None) -> str` 调 SDK `params::format_value`,两者往返(`parse_value(format_value(x)) == x`)。Python 包对用户输入的单位换算只经这两个函数,不另写解析器。
- refdata 的编码不在本 crate:refdata 的唯一来源是 `hft-cfg`。

### Arrow 映射

- 列名、类型、每个 kind 的 schema 在运行时取自 [`hft-event`](../hft-event/README.md) 的 `schema::catalog()` 字段描述(`cargo xtask gen-schema` 导出 `python/_generated/` 的同一来源);每张表带 `seq`、`tsc`、`proc_ns`、`flags` 与体字段:嵌套结构展开为叶子名,填充字段不出列;叶子名重复或与表头列、`entries`、`body`、`payload`、`account_kind`、`_missing` 同名时改用以 `_` 连接的路径(仍冲突再加 `body_` 前缀);每个体字段的元数据 `hft.path` 是它在结构中的点分路径。有 `account` 字段的 kind 另有 `account_kind` 列(`"Live"`/`"Sim"`,按 journal 的 refdata)。
- `Px`/`Qty` 是 `int64` 原值,字段元数据带 `px_exp`/`qty_exp`(按 inst 的资产):JSON 对象,键为本次查询可能出现的 `InstId`(给出 `inst` 时为这些 inst,否则为 refdata 中状态不是 `Delisted` 的全部 inst)、值为该 inst 资产的指数;以另一资产计量的字段(如 `OrderEvent.fee` 按 `fee_asset`;没有 inst 而有资产字段的 kind 按该资产)的元数据改为 `exp_asset_col`(给出资产的列名)与 `qty_exp_by_asset`(键为 refdata 的全部 `AssetId`);`Notional`、`Rate` 字段的元数据 `exp` 为固定指数 −6、−9。流的 schema 在读第一批之前确定,所以元数据按查询而不按批;refdata 取最新的带 refdata 的引导块。`decimals = True` 的 `*_f` 列按同样的指数换算(查不到指数为 null)。
- 变长记录(`ControlCmd`、`ConfigBlob`、`StateSnapshot`、`Advice`、`RawRx`、`RawTx`)除前缀字段列外以 `large_binary` 列 `body` 给出逻辑体(多段记录重组后),`Advice` 另有 `payload` 列(前缀之后 `len` 字节),另提供解码函数 `hft_py.decode.<kind 的蛇形名>(b)`(`control_cmd`、`config_blob`、`state_snapshot`、`advice`、`raw_rx`、`raw_tx`,返回前缀字段的 dict 与载荷字节);`Advice` 载荷由策略自行约定,本 crate 只给出 `AdviceHdr` 字段与原始字节。其余变长 kind(`L2Batch`、`L2Snapshot`、`MarginReport`、`AccountConfigReport`)的定长前缀展开为列,重复的尾部条目给成 `list<struct>` 列 `entries`(字段按该条目的字段表)。解码函数对体短于前缀、或载荷长度与前缀中的长度字段(`ConfigBlob` 的 `blob.len`、`ControlCmd` 与 `RawRx`/`RawTx` 的 `payload_len`、`Advice` 的 `len`)不符的输入抛 `ValueError`。
- 读到比编译进来的 schema 新的记录(体更长):截断到已知字段并在 `j.info()` 里报告;旧记录(体更短):缺失字段补零并置 `_missing` 位图列(某段的段头把该 kind 登记为更小的体时才有这一列;`uint64`,第 i 位对应第 i 个体字段列,体字段超过 64 个时为定长二进制位图)。
- `hft_py.schema_hash()` 是编译进来的 kind 表(每项 `{kind, version, size}`)的哈希(按编号顺序对每项的 `(kind u16, version u16, size u32)` 小端字节做 FNV-1a 64;`gen-schema` 把同一值写成 `python/_generated/events.py` 的 `SCHEMA_HASH`);`hft_py.schema.catalog()` 每个 kind 一行(`kind`、`name`、`domain`、`role`、`layout`、`schema_version`、`journals`、`crc`、`optional`,写法同 `docs/generated/event-catalog.md`);段头 schema 表与它不同时逐 kind 比较:`version` 不同按上一条读取并在 `j.info()["schema"]` 列出;同一 kind `version` 相同而 `size` 不同(违反只追加)即主版本不兼容 → `SchemaError`。

### 错误

`hft_py.JournalError`(段损坏、CRC 失败、段链断裂——附段路径与 seq;路径不存在或没有任何段、`JournalReader` 的 `Gap` 与 `Err` 同样是它,异常带 `path`、`seq` 属性)、`hft_py.SchemaError`(主版本不兼容,见 [Arrow 映射](#arrow-映射))、`hft_py.ParamsError`(schema 不认识、版本不符、index 与 schema 文件不一致、blob 无法解码)。参数取值错误是 `ValueError`(见上表)。Rust 侧不 `panic` 跨越 FFI:所有错误转成上述异常。

## 实现说明

<details><summary>实现要点</summary>

### 1. journal → Arrow

1. `open_journal` 列出热段与冷副本并核对段头 schema 表,读取以 `hft-journal::JournalReader` 进行(`OnGap::Error`,读到 `Pending` 即本次 `events()` 的结尾):冷副本 `O_RDONLY` + `PROT_READ` 映射;同机 RAM 热段同样只读映射,读 `committed_len`(Acquire)确定可读边界。单个段文件路径从该段的引导块读起,读到下一段的 `Begin` 或缺段即结束。`proc_ns` 先用当前段段头的校准对,之后每条 `ClockStatus` 更新校准对。
2. `events()` 顺序扫描并按 `kind`、`inst`、`account` 与时间窗过滤(跳到 `t0` 附近的段索引优化留待吞吐成为瓶颈时再做)。
3. 每批(默认 1,000,000 行或段尾)把行存的 `#[repr(C)]` 体**按列转置**进 Arrow 缓冲(列缓冲从 4 096 行起按倍增长到 `batch_rows` 为止,不按 `batch_rows` 一次预留):每个定长字段一次顺序拷贝。journal 是行存,这一次转置不可避免;列缓冲填好后各复制一次进 Python `bytes`,经 `pyarrow.py_buffer`(不再复制)组成 `pyarrow.RecordBatch`(本 crate 不链接 arrow-rs,见"依赖");**零拷贝指的是 pyarrow → Polars/DuckDB 的交接**(经 PyCapsule `__arrow_c_stream__`/`__arrow_c_array__`,消费方直接引用这些缓冲,无复制)。
4. 返回 `RecordBatchReader`,惰性按批产出;Python 侧不持有 journal 映射的引用,段被回收不影响已产出的批。
5. `tail()`:读者记住 `(segment, offset)`;每 `poll_ms` 检查 `committed_len` 与新段;跨段滚动时先读新段首的 `StateSnapshot`/`ConfigBlob` 再继续;一个事件的记录先暂存,读到带 `EVENT_END` 的记录才交付(冷副本的同步边界可能切在事件中间);Python 线程在两次检查之间 `sleep`(冷路径,释放 GIL)。

### 2. ledger / pnl

- `ledger`/`pnl` 直接用 [hft-ledger](../hft-ledger/README.md#冷消费者journalledger) 的 journal 冷消费者 `JournalLedger`,本 crate 不另写入账实现:它从 Output `OrderDecision{New}` 维护按账户与 venue 隔离的订单槽映射(完整键归上述冷消费者契约),对 `OrderEvent` 的成交增量按 `cum_filled` 去重入账,另消费 `ExternalFill`、`FundingSettle`、`AdoptExchange` 调整、资金费自动校正 `AdjustDelta::Funding`、残余核销 `AdjustDelta::WriteOff` 与余额基线照常入账;`Funding` 事件同样喂给 `on_funding_interval`,资金费结算时刻的归一化参数取 `funding_grid`,与热进程一致);与热进程同一代码,PnL 与库存口径只有一套([07 §5](../../design/07-策略层.md))。带 `AUTO_EXTERNAL` 的 `ReconcileReport` 条目不另有成交记录:sequencer 在这条 Input 内确定地合成成交(规则归 [hft-sequencer](../hft-sequencer/README.md#每条-input-的处理顺序process)),本 crate 经同一消费路径调用 hft-ledger 的同一合成函数,因此看到同一笔成交与之后的 `auto_held` 抵扣([hft-ledger 外部成交](../hft-ledger/README.md#外部成交externalfill) 第 6 步)。OMS 防御路径的外来成交(`ForeignFill`、带成交的 `StaleGen`)同理不写 Input:本 crate 对同一事件内的每条 `Note{OmsForeignFill}` 经 hft-ledger 的同一消费路径入账(第 7 步;注记字段归 [hft-oms](../hft-oms/README.md#回报匹配与成交增量) 第 8 条)。不带 `AUTO_RECON` 的 `Note{ExternalFill}` 是热进程对已入账成交的记录,不据它入账。
- 模拟账户的报告(PnL 分解、成交率、两腿间隔、裸敞口、保证金占用)用同一 API 计算,结果带 `account_kind = Sim` 标注。
- `JournalLedger::apply` 读的是单段的 `ControlCmd` 原始记录(`PartHdr` + 载荷),本 crate 的扫描给的是重组后的逻辑体,交给它之前补上单段 `PartHdr`;其余 kind 原样交付。

### 3. 源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `#[pymodule] hft_py._native`,子模块注册 |
| `src/error.rs` | 核心错误类型与 Python 异常(`JournalError`、`SchemaError`、`ParamsError`) |
| `src/journal.rs` | 路径解析、段列表、记录扫描(TSC → UTC、段跟踪、错误映射)、`events`、`tail`、`info` |
| `src/arrow_map.rs` | kind → Arrow 列表(运行时取 hft-event 字段描述)与转置 |
| `src/cols.rs` | 列类型、列缓冲与批,转成 `pyarrow` 数组/`RecordBatch` |
| `src/config.rs` | `config_at` 与配置状态机 |
| `src/ledger.rs` | `ledger`、`pnl` 的核心(驱动 `JournalLedger`) |
| `src/params.rs` | `params.load_schemas`、`load_schema`、`encode`、`decode`、`encode_cmd`、`parse_value`、`format_value`(包装 SDK `params`,与 hft-cfg 共用) |
| `src/heartbeat.rs` | `heartbeat.read`、`Watch` |
| `src/decode.rs`、`src/schema.rs` | `hft_py.decode.*`、`schema_hash`、`schema.catalog` |
| `src/py_journal.rs`、`src/py_ledger.rs` | Python 类(`Journal`、批读者、尾随迭代器、`RecordBatch`、`Ledger`) |
| `src/time.rs` | 时间参数解析、tsc → UTC |
| `src/testing.rs` | `testing` 特性的测试钩子 |
| `python/hft_py/__init__.py`、`*.pyi`、`py.typed` | 包入口(再导出 `_native` 的公开名字)与类型存根(随 wheel 发布) |
| `pyproject.toml` | maturin 构建(混合布局,扩展模块 `hft_py._native`) |
| `examples/pyfixture.rs`、`examples/pyfixture/seq.rs` | 测试夹具生成器(refdata/引擎 blob、TW 脚本与批量记录文件、`RUST_REF` 读出;`seq.rs`:经 hft-testkit `SeqRig` 的热进程夹具);不进 wheel |
| `tests/python/`、`tests/*.rs` | pytest 套件与 Rust 测试 |

### 4. 打包与发布

- **wheel**:`maturin` 构建 `hft_py-<版本>-cp310-abi3-manylinux_2_28_aarch64.whl`(`cp310-abi3`:一个 wheel 覆盖 Python ≥ 3.10);生产主机是 aarch64,与 Rust 二进制同一目标三元组([development 目标 CPU](../../docs/development.md#目标-cpu))。研究主机若为 x86_64,可另出 x86_64 wheel,只含 reader 与 ledger。
- 只在 CI 的发布流水线里与 Rust 二进制一起构建,放进同一个不可变发布归档的 `python/` 目录([deploy](../../deploy/README.md)),SHA-256 写进 `RELEASE.json`;部署脚本以 `pip install --no-index` 装进**该版本全部 Python 包共用的单一 venv**(按版本,冷进程经 `/opt/hft/current-cold/venv` 使用,[deploy 单元组](../../deploy/README.md#单元组与分组激活);与 `python/*` 各包的 wheel 一起,离线安装)。**不在服务器编译**。
- 测试构建另开 cargo feature `testing`(导出 `hft_py._testing` 的测试钩子:最近一批的列缓冲地址、`tail` 的检查次数、`ledger` 结果的最终三节快照字节),发布 wheel 不含。
- PyO3 不启用 `extension-module` 特性:maturin 构建扩展时设置 `PYO3_BUILD_EXTENSION_MODULE`,仓库的 `.cargo/config.toml` 对普通 cargo 构建设置同一变量,所以 Rust 测试与 workspace 构建不链接 libpython。
- wheel 版本 = 发布版本;`hft_py.schema_hash()` 与 journal 段头的 schema 表比对,主版本不同 → `SchemaError`。

</details>

## 延迟与资源契约

冷组件,无热路径约束;吞吐目标:

| 操作 | 目标 | 测量 |
|---|---|---|
| `events()` 定长 kind 扫描 + 转置 | ≥ 1 GB/s journal 字节/核(`Bbo` ≈ 10 M 行/s) | `benches/py_events.rs`(Rust 侧)+ pytest 计时 |
| `tail()` 可见延迟 | ≤ `poll_ms` + 5 ms | pytest 计时 |
| `ledger` 一天 seq journal | < 2 分钟 | pytest 计时 |
| 内存 | 每批 ≤ `batch_rows` × 行宽 × 2;不整段加载 | — |

PY-161 的各次峰值测量使用独立子进程和相同预热,采样线程就绪后取基线;匿名驻留量取自 `/proc/self/smaps_rollup` 的 `Anonymous`,读取或字段解析失败则测试失败。

hft-py 以只读映射打开热段,对热进程的影响只有页表与共享 L3;研究负载不在 `tokyo-cex` 生产主机上跑(ops_bot 与 treasury 的 `tail` 例外,受 cgroup CPU 配额与 dev2 护栏约束,[12 §1](../../design/12-风险与未决问题.md))。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

| 依赖 | 用途 | 约束 |
|---|---|---|
| `pyo3` 0.29.x(`macros`、`abi3-py310`)、`maturin` | 绑定与打包 | abi3;不用默认特性 |
| `pyarrow` ≥ 14(Python 运行时依赖,wheel 的 `Requires-Dist`) | 由 Rust 填好的列缓冲构造 `pyarrow` 数组与 `RecordBatch`,`RecordBatchReader.from_batches` 给出 PyCapsule 流 | 不用 arrow-rs(`arrow-array`/`arrow-buffer` 的依赖闭包含 deny.toml 禁用的 `num-bigint` 与 CC0-1.0 许可的 `tiny-keccak`)与 `pyo3-arrow`;与 Polars、DuckDB 经 PyCapsule 互通([research/06 §7](../../design/research/06-data-plane-replay-observability.md)) |
| `serde_json` | 读 `index.json` 与 schema 中 SDK `OwnedSchema` 不保留的描述符 | — |
| [`hft-journal`](../hft-journal/README.md)(默认特性:只含 `JournalReader`) | 只读 | **禁止**启用 `writer` 特性 |
| [`hft-event`](../hft-event/README.md)、[`hft-refdata`](../hft-refdata/README.md)、[`hft-types`](../hft-types/README.md) | 类型、事件 schema、参数线上头 | — |
| [`hft-strategy-sdk`](../hft-strategy-sdk/README.md) | 纯模块 `params`(参数规范编解码)与 schema 描述类型 | 只用这两部分;不引入策略包 |
| [`hft-ledger`](../hft-ledger/README.md) | 记账 | — |
| [`hft-risk`](../hft-risk/README.md) | `config_at` 解码 `RiskParams` blob(编码归该 crate;在它提供 blob 编解码之前不链接,`risk` 给原始字节) | 只用 `RiskParams` 类型与其解码;不用检查链与模式状态 |
| [`hft-rt`](../hft-rt/README.md) | `HeartbeatBlock::open_ro`(`heartbeat.read`) | 只用只读心跳块,不用环与线程原语 |
| [`hft-testkit`](../hft-testkit/README.md)(feature `seq`)、`sha2`(dev 依赖) | `examples/pyfixture` 的夹具:`SeqRig` 以真实 hft-sequencer 生成热进程 journal 与逐步骤的 ledger 节([TESTS](TESTS.md) `SEQ_FX`);blob 摘要 | 只在 `[dev-dependencies]`,不进 wheel 与库 |

禁止:任何 `strategies/*`、`hft-sequencer`、`hft-sim`、`hft-net`、`hft-venue`、链上 crate 的运行时部分(经 dev 依赖 hft-testkit 进入夹具生成器的 hft-sequencer/hft-sim 除外)、任何能向 journal 或热进程写入的 API。

## 延伸阅读

- [08 §7 Python 访问](../../design/08-数据面与回放.md)、[research/06 §7](../../design/research/06-data-plane-replay-observability.md)、[research/04 §6](../../design/research/04-oss-frameworks-and-patterns.md)
- [journal-format](../../docs/subsystems/journal-format.md)、[event-model](../../docs/subsystems/event-model.md)、[replayer](../../apps/replayer/README.md)、[tools/hft-cfg](../../tools/README.md#hft-cfg)
- [python/AGENTS.md](../../python/AGENTS.md);范围决定见 [round8-decisions](../../design/_work/round8-decisions.md)

## 已知限制与后续工作

- **不提供回放**:验证回放与模拟重演只在 [replayer](../../apps/replayer/README.md) 命令行;本 crate 不链接 `hft-sequencer` 与策略包。研究需要的是模拟账户与实盘账户的 journal 分析,用 `events`/`ledger` 完成。
- **mmap 视图直出(真正的零拷贝读 journal)不提供**:把只读映射直接包成 numpy/Arrow 缓冲需要在本 crate 写 `unsafe`,而 [crates/AGENTS.md](../AGENTS.md) 只允许 4 个 crate 使用 `unsafe`。若转置吞吐成为研究瓶颈,方案是由 `hft-journal` 提供一个安全的"钉住段 + 导出 Arrow 缓冲"API,需 Agent Note。
- 可选扩展:链上 journal 读取与 AMM 报价(`amm` 子模块,包装 [hft-amm](../hft-amm/README.md))——触发条件:链上策略从旧框架迁移、研究需要读 chain journal 时,作为 cargo feature 加入。
- Parquet 历史数据不经本 crate(研究直接用 DuckDB/Polars 读 S3);`events()` 只覆盖本机 journal 保留期。
- `config_at` 的 `risk` 只给原始字节、`engine` 各节只给原始字节:`RiskParams` 的 blob 编解码与 ECF1 各节的解码归各自 crate,提供后再在本 crate 解码成 dict(同时补 PY-088 的对应行)。
- 冷账本的 `Capacities` 用固定值,直到 ECF1 段 3 有解码(hft-sequencer);从关键帧起步的窗口估值见"参数语义"的 `ledger` 一条。
- `events()` 没有按 `t0` 跳段:整份扫描;吞吐不够时用段头的校准对与 `created_utc_ns` 选起点段。
- Windows 不支持(journal 与热段只在 Linux)。

### 开发备忘

<details><summary>给实现者</summary>

- **已核实**(PY-055/056):Polars 2.0 的 `pl.from_arrow(流对象)` 返回 struct 型 Series,DataFrame 用 `pl.DataFrame(流对象)`;DuckDB 1.5 对同一对象先取 `__arrow_c_schema__` 再取流;`Series._get_buffer_info` 在 Polars 2 已删,缓冲地址经 `to_numpy(allow_copy=False)` 取。
- **待核实**:`maturin` 在 GitHub 托管 arm64 runner 上产出 `manylinux_2_28_aarch64` wheel 的方式(原生构建或 zig 交叉);核实方法:CI 试构建并在 dev2 上 `pip install --no-index` 冒烟。
- **依赖 hft-journal 的保证**:热段文件永不原地复用(只新建与删除),否则只读读者会读到被覆盖的数据;实现 `tail` 前与 hft-journal 规格核对。
- 参数规范编解码的唯一实现在 `hft-strategy-sdk` 的纯模块 `params`;本 crate 与 hft-cfg 只包装它。schema JSON 的字段名以 SDK 与 [registry](../../strategies/registry/README.md) 的导出为准。
- 实现顺序:`open_journal`/`events`、`tail`、`ledger`/`pnl`、`params.encode`/`decode` → 可选扩展的 `amm` feature。

</details>
