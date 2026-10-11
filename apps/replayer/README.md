---
description: "离线 CLI:用与实盘同一份 hft-sequencer 与策略集合回放 journal——verify 逐字节比对意图流与状态哈希(可从归档的 Model 检查点重算 Advice),sim 把录下的行情喂给 hft-sim 模拟网关 + 同一策略做排障、复现与压力用例(可选的 --ab 只给两组参数的相对差值),compare 逐决策比对两个实例的 journal(正式实例 Shadow 对照),latency 从 seq journal 对每条行情记录做按事件加权的主机内延迟统计(门槛 ⑩)"
kind: app
tier: cold
scope: core
status: impl
---

# replayer

## 概要

`replayer` 把 journal 喂回**同一个** [hft-sequencer](../../crates/hft-sequencer/README.md)(同一策略集合、执行层、风控、OMS 代码,同一编译配置),回答三个问题:代码改动是否改变了行为(`verify`);"这段行情下策略与模拟网关为什么会这样走"(`sim`,排障、复现与上线门槛的压力用例);两个实例在同一时段的决策是否一致(`compare`,正式实例 Shadow 与同参数模拟实例的逐决策比对)。另有一个不运行 sequencer 的统计子命令 `latency`:从 seq journal 对每条行情记录算主机内延迟分段,给门槛 ⑩ 提供按事件加权的证据。它只读 journal(本地冷副本或 S3 的 `.jrn.zst`)与 advisor 检查点,不连交易所、不写生产 journal、不发控制命令。**core** = `verify`、`sim`、`compare`、`latency`;可选扩展:`bench`(纯计算回归基线)、`raw-decode`(解码器重跑对比)、`verify --chain`(链上策略迁移时启用)。`sim` 用 [hft-sim](../../crates/hft-sim/README.md) 的同一撮合器,**不用于评估策略收益**:唯一的参数替换是默认关闭的 `sim --ab`,它只输出两组参数在同一段 journal、同一撮合器下的**差值**,报告首行强制标注"相对比较,不是收益估计,不得用于定仓";不提供参数扫描或收益排行。运行在 CI(GitHub 托管 runner,作业划分见 [development](../../docs/development.md))与运行实例的主机上(每日回放校验);journal 的实例名只有 `prod`(交易所账户实例:dev2 上是测试网/模拟盘联调实例,统一升级后是正式实盘实例)与 `sim`(只含模拟账户,[hft-core 实例角色](../hft-core/README.md#实例角色prod-与-sim)),两者用同一种方式验证。

## 目录

- [使用本组件](#使用本组件)
  - [verify](#verify)
  - [sim](#sim)
  - [compare](#compare)
  - [latency](#latency)
  - [bench(可选扩展)](#bench可选扩展)
  - [raw-decode(可选扩展)](#raw-decode可选扩展)
  - [--chain(可选扩展)](#--chain可选扩展)
  - [输入定位与退出码](#输入定位与退出码)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
replayer verify  --journal <dir|s3://…> [--from-seq N] [--to-seq M] [--instance prod|sim] [--advice journal|recompute]
                 [--model-checkpoint <file|dir|s3://…>] [--keep-going] [--report out.json | --report-dir <dir>]
replayer sim     --journal … [--from-seq N] [--to-seq M] [--accounts <a,b>] [--as-sim <account>=<sim-account.toml>] [--seed S] [--check-fills]
                 [--advice recompute|journal] [--model-checkpoint <dir>] [--stress <case>[,<case>…]|gate|all] [--stress-spec <spec.toml>]
                 [--ab <paramsA> <paramsB>] [--emit-journal] --out <dir>
replayer compare --a <journal> --b <journal> --map <a账户>=<b账户>[,…] [--from <utc>] [--to <utc>] [--window-ms 2000] --out <dir>
replayer latency --journal … [--from <utc>] [--to <utc>] [--instance prod|sim] [--venue <v>[,<v>…]]
                 --cal-max-err-ns <ns> --burst-gap-ns <ns> --out <dir>
replayer bench   --journal … [--baseline bench-baseline.json] [--threshold-pct 10]     # 可选扩展
replayer raw-decode --raw <dir> --seq <dir> [--venue binance-usdm]                     # 可选扩展
# 读 S3 输入的子命令另接受 [--cache-dir <dir>](见输入定位)
```

`cargo xtask replay-verify <journal>` 是 `replayer verify` 的 CI 包装(根 [AGENTS.md](../../AGENTS.md))。每个实例 journal 的每日 verify 在该 journal 冷副本所在的主机上运行(单元 `hft-replay-verify`,在根级 `hftbatch.slice` 内(`CPUWeight=idle`,规则归 [hft-core 主机资源隔离](../hft-core/README.md#主机资源隔离slice核与出口-ip))、内存上限按主机类见 [hft-core 整机表](../hft-core/README.md#内存与-journal-段按主机类唯一归属);它运行期间 archiver 与 pipeline 暂停,批处理并发规则归同一节;dev2 上受护栏约束,一级动作在 `hft-verify` 之后停它,护栏或联调窗口的标志文件存在时不启动,见 [hft-core dev2 护栏](../hft-core/README.md#dev2-护栏);dev2 启用退路 (a) 时改在另一台主机上从 S3 读 journal 运行),以每日模式运行:`replayer verify --journal <cold_root>/<instance>/seq --instance <instance> --advice recompute --model-checkpoint <advisor_daily>/<instance> --report-dir /var/lib/hft/replay-verify/<instance>`(`<advisor_daily>` 是 journal-archiver 存放每日检查点本地副本的目录,见 [journal-archiver](../journal-archiver/README.md#model-检查点每日归档);选日与报告规则见 [verify](#verify) 第 9 条)。**每次运行补跑全部缺少报告的 UTC 日**(不只前一日);窗口期间迟到的报告视同当日(补跑规则归 [testing §6](../../docs/testing.md#6-ci-运行矩阵),门槛 ⑦ 的计时归 [testing §8.1](../../docs/testing.md#81-模拟账户长跑的计时与证据))。结果写 `/var/lib/hft/replay-verify/<instance>/<date>.json`,不占 CI 分钟与 S3 流量。策略集合来自 [hft-strategy-registry](../../strategies/registry/README.md)(与 hft-core 依赖同一个 crate,[策略装配](../hft-core/README.md#策略装配)),与实盘同一份注册。

### verify

验证模式:输入 seq journal 的**全部** Input 记录(行情、回报——含 Sim 账户经 `rep-sim` 进入的回报、命令、定时器、`Advice`、`ConfigBlob`),要求重放产生的 Output 与实盘逐字节一致([08](../../design/08-数据面与回放.md))。

1. 从 `--from-seq` 之前最近的 `StateSnapshot` 恢复状态(未给时从第一个段首快照);恢复后立即把状态重新序列化、计算状态哈希,与该快照字节的哈希比较(快照自检)。**状态哈希由 replayer 计算**:实盘写关键帧时快照头的 `state_hash` 填 0(规则归 [journal-format](../../docs/subsystems/journal-format.md) 与 [hft-sequencer](../../crates/hft-sequencer/README.md)),replayer 对 journal 中的快照字节按同一算法(归 hft-sequencer)自行计算;头部非 0 时另与头部值核对。
2. 按 seq 顺序把每条 Input 交给 `VerifyDriver`:sequencer 不赋新 seq,断言下一个要写的 seq 与 journal 一致。
3. 每产生一条 Output(`OrderDecision`、`ExecDecision`、`CmdResult`、Output 类 `Note`…)与 journal 同 seq 记录**逐字节**比较(记录头 + 体;记录头 `tsc` 取 journal 值;`decided_tsc` 等测量字段按偏移屏蔽)。**Meta** 记录不参与比对与状态。分类表归 [hft-sequencer](../../crates/hft-sequencer/README.md) 与 [event-model](../../docs/subsystems/event-model.md),replayer 不自带分类。
4. `Recovery` 引导块后、首个 `EngineClock` 前的完整 `RecoverMismatch` 注记事件界定旧尾部的接受范围。来源取该引导块之前最近的快照,终点取旧运行最后一个完整事件;每个 (venue, account) 从注记的首处分歧 seq 起接受到终点。注记必须指向此范围内的 Output,账户与 venue 必须匹配;无账户的 Output 需要全部配置账户/venue 的注记,`ExecDecision` 需要该账户的全部 venue。注记缺来源、重复、越界、保留字段非零或出现在其他位置均退出 3。范围内仍逐字节比较,差异只列入 `recovery_diffs`,不影响退出码;未登记的账户、更早的输出、额外输出所对应的 Input 和恢复后正常输出仍严格比较。恢复段从其引导快照重新起步,不延续旧尾部重算状态。`RecoverFromSnapshot` 或版本变化也按独立运行处理。
5. 每个 `StateSnapshot` 位置比较状态哈希(覆盖簿、仓位、槽、风控、执行层、策略状态、参数版本):重放状态的哈希与 journal 中该快照字节的哈希(按第 1 步由 replayer 计算)比较。关键帧在验证驱动中同样一次写完,并与 journal 逐字节比对(规则归 hft-sequencer)。
6. 首个普通分歧即停止,报告:seq、kind、期望与实际的十六进制与字段级 diff、之前 1,000 条输入的 kind 摘要(不足 1,000 条时为全部已处理的输入)、最近快照 seq,以及 [hft-testkit](../../crates/hft-testkit/README.md#7-状态哈希对比) 的组件级二分结果。`--keep-going` 时继续并汇总(分歧后状态不可信,只用于调试;退出码仍为 1)。标准输出含 `summary`、`divergences`、`recovery_diffs`;给出 `--report <f>` 时另写 `<f 去掉 .json>.divergence.json`,其对象含后两个数组,每次重跑均覆盖,无对应差异的数组为空。`<f>` 本身按每日报告的格式(第 9 条)写,`date` 取输入第一条记录的 UTC 日。
7. `--advice recompute`:另在进程内按 advisor 规则运行各策略的 `Model`(同一 `StrategySet`),把重算的 `Advice` 载荷(`AdviceHdr` 与 payload)与 journal 中的逐字节比较——**Model 确定性检查**。配对键是 `(strategy, account, group, local_kind, basis_seq)`:journal 中的每条 Advice 必须有同键的重算 Advice 且载荷逐字节相同,否则为分歧,报告第一处的 `(strategy, account, group, local_kind, seq)`;重算多出、journal 中没有的(实盘 advisor 的 outbox 按 `(strategy, account, group, local_kind)` 只留最新一条,被覆盖的不进 journal,规则归 [hft-sequencer advisor](../../crates/hft-sequencer/README.md))计入 `coalesced`,不算分歧。默认 `journal`(Advice 是 Input,照读;不运行任何 `Model`,报告 `recompute = "skipped"`、`skipped_reason = "advice_journal"`)。
8. **Model 起点**(`--advice recompute` 时每个带 Model 的策略实例都需要一个):
   - `--model-checkpoint <file|dir|s3://…>`:目录或 S3 前缀下按 `<strategy_instance>.ckpt` 取该实例的检查点(格式与键归 [hft-core 检查点表](../hft-core/README.md#内存与-journal-段按主机类唯一归属);S3 上是 journal-archiver 每日归档的那一份,路径规则归 [journal-archiver](../journal-archiver/README.md))。键(`kind_id`、`LAYOUT_VERSION`、`schema.id`、`strategy_instance`)与 journal 中的实例不符 → 退出 2;文件损坏(`CkptHdr` 的 magic、`hdr_ver`、`body_len` 或 CRC 不符,或 `path_len` 超出体长)→ 退出 3;`last_seq` 不在输入覆盖的区间内(早于第一条记录的 seq − 1,或晚于最后一条记录)→ 该实例按下面"两者都没有"处理。目录中缺某实例的文件时该实例按"未给检查点"处理。`Model::restore` 后把 `last_seq + 1` 起的 journal 记录喂给 Model;状态重放的起点快照取 `--from-seq` 与各实例 `last_seq + 1` 中最小者之前最近的快照(与 advisor 镜像同一规则,模型视图在每个 seq 处与实盘相同;`last_seq + 1` 落在段首引导块内、早于该引导块的快照时取这个快照——引导块里没有投递给 Model 的记录);只比较 seq > max(`last_seq`, `--from-seq`) 的 Advice,之前的照读。
   - 未给检查点时:窗口内含该实例的 Model 空起步点(实例创建,或 `Note{ModelCheckpointDiscarded}` 之后的第一条记录)→ 从那里起算。
   - 两者都没有的实例不做重算,报告列为 `model_no_start`(实例名列入分歧明细的 `model_no_start`;不算分歧,退出码不受影响);每日报告据此填 `recompute`(第 9 条)。
   - **追赶重发**:advisor 从旧检查点追赶时,旧 basis 的 Advice 可在新运行再次入日志。重算保留该五元键的最终产出字节,每个运行各允许匹配一次;同一运行内的重复记录仍分歧,重发载荷改变也分歧。历史与待配对载荷共用诊断容量,未在新运行重发的历史条目不增加 coalesced。
   - **跨重启来源**:引擎从每次运行的引导快照独立恢复。已从合法创建点、检查点或丢弃注记重算的 Model,在完整前缀连续且实例身份、种类和参数引用未变时延续样本;缺来源的实例仍列 model_no_start。显式检查点早于所选运行时先重算其间的全部运行。预热中的 ModelPathReset 仍影响窗口内的比较边界。该注记 v[0] 校验原因码 1/2,不与实例编号比较;保留位非零或 last_seq 不早于注记均拒绝。SegmentRoll 结构记录沿用所在运行前一条记录的引擎时间,新运行的 Begin 取引导快照时间;End/BootDone 的零 TSC 不参与时钟换算。
   - **路径节被丢弃**:恢复 `--model-checkpoint` 时文件中路径节的版本与当前 `PATH_VERSION` 不符(只丢路径节,规则归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md#model-检查点) 的 `restore_path`),或窗口内出现该实例的 `Note{ModelPathReset}`(实盘重启时丢过路径节)→ 派生状态重建点 `recompute_from_seq` 按 [hft-strategy-sdk Model 检查点](../../crates/hft-strategy-sdk/README.md#model-检查点) 第 7 条的定义计算(`last_seq` 之后、引擎时间 `engine_ns` 首次 ≥ `last_seq` 所指记录的 `engine_ns` + `Model::path_settle_ms` × 10^6 的那条记录的 seq;`path_settle_ms` 取 Note 的 `v[2]`,replayer 自己丢路径节时取 `last_seq` 处生效参数下的值),`basis_seq < recompute_from_seq` 的 Advice 不比较,其后照常逐字节比较。时间基准是引擎时间(由记录头 `tsc` 与 `EngineClock` 算出,停机时间不计,定义归 [event-model](../../docs/subsystems/event-model.md));记录头没有 UTC 字段。`recompute_from_seq` 写进报告(一日内多次时写最后一次);报告格式归 [testing §3.5](../../docs/testing.md#35-验证回放)。
9. **每日模式**(`--report-dir <dir>`;与 `--report`、`--from-seq`、`--to-seq`、`--advice journal` 互斥,同时给出 → 退出 2):`--journal` 是一个实例的 seq 冷副本根 `<cold_root>/<instance>/seq`(或 S3 上同一实例的 `seq` 前缀),`--model-checkpoint` 是该实例的每日检查点根(其下按 `YYYY-MM-DD` 分目录)。按日期升序处理每个同时满足以下条件的 UTC 日 D:D 早于本次运行开始时的 UTC 日;`<yyyymmdd>` 目录下每个 `.jrn` 都有同名 `.seal`;`<dir>/<YYYY-MM-DD>.json` 不存在。每个 D 等同于一次 `verify --journal <根>/<yyyymmdd> --advice recompute --model-checkpoint <检查点根>/<YYYY-MM-DD>`(检查点目录不存在 → 全部带 Model 的实例 `model_no_start`;某实例的检查点键与 journal 不符时不按第 8 条退出 2,而是该实例按"两者都没有"处理、记 `model_no_start`,该日照常完成),报告写 `<dir>/<YYYY-MM-DD>.json`(先写同目录 `.tmp` 再改名)。某日以退出码 3 或 4 结束时同样写报告(`verify = "fail"`、`skipped_reason` = `"input_incomplete"` 或 `"incompatible"`)并继续下一日;某日以退出码 2 结束 → 不写该日报告(下次运行重试)、不再处理其后的日;进程退出码取已处理各日退出码的最大值。报告字段(格式归 testing §3.5)的填法:`instance` 取段头;`date` = D;`release` = `replayer --version` 的版本串;`verify` = 有无分歧;`first_divergent_seq`、`component` = 第 6 条的 seq 与二分结果;`recompute` = 任一实例重算分歧 → `"fail"`,否则任一带 Model 的实例 `model_no_start` → `"skipped"`(`skipped_reason = "model_no_start"`),否则 `"pass"`(没有带 Model 的实例时同为 `"pass"`、`coalesced = 0`);`coalesced` = 各实例之和;`checkpoint_key` = 所用检查点键的列表(每项 `strategy_instance`、`kind_id`、`layout_version`、`schema_id`、`last_seq`);`recompute_from_seq` 见第 8 条;`generated_utc` = 写报告时的 UTC。

verify 是**离线**工具(夜跑与 CI);hft-core 冷启动用 `RecoverDriver`(输出不一致只告警,见 [hft-core 启动顺序](../hft-core/README.md#启动顺序))。

### sim

模拟重演:把录下的 journal 行情与命令喂给 sequencer + [`hft_sim::SimExchange`](../../crates/hft-sim/README.md#纯核心-api),用于**排障与复现**(某次模拟或实盘的策略行为、执行层决定、拒单、强平路径)与上线门槛的压力用例,不评估收益。

- **重现 Sim 账户**(默认,`--accounts` 缺省为 journal 中全部 Sim 账户):这些账户的回报不从 journal 读,而由 `SimExchange` 用同一 `SimAccountCfg`(journal 中的 refdata)、同一 `[sim]` 全表(`SimTuning` 与 `[sim.latency_models]`/`[sim.margin_models]`,取自 hft-core 启动时写入 journal 的记录,[hft-core 配置文件字段](../hft-core/README.md#配置文件字段))与种子(`--seed` 缺省取 journal 记录的种子)重新生成;其余账户的记录照读。`--check-fills`:重新生成的成交与 journal 中的逐笔比较(价、量、`ex_ts_ns`),区间内无 `quiet_advance` 时必须一致,否则退出 1。
- **把 Live 账户当作 Sim 重演**(`--as-sim <account>=<file>`):丢弃该账户录下的交易所回报,把它的订单交给 `SimExchange`(`SimAccountCfg` 取自文件,其模型下标指向 journal 中的 `[sim]` 模型表;journal 来自没有 Sim 账户的实例时文件须自带两张模型表);用于复现"同一行情下策略与执行层的决策链"。结果只作排障,不与实盘收益比较。
- 起点:`--from-seq` 必须是一条 `StateSnapshot` 记录的 seq(未给时取第一个段首快照;不是快照 → 退出 2);`--accounts` 只能列 Sim 账户与 `--as-sim` 给出的账户(列入其他 Live 账户 → 退出 2);被重现账户的模拟真值按 [hft-sim 重启恢复](../../crates/hft-sim/README.md#重启恢复) 由起点快照 seed(`SimExchange::seed_account_from`,已吃流动性记忆为空);`SimSetup.tsc_hz_milli` 取 journal 的 `EngineClock` 行。只有从账户创建或 `SimReset` 处起步时,重现才保证逐笔一致;其他起点在报告里标 `approximate`。
- 参数与代码:参数取 journal 中的记录(唯一例外是下面的 `--ab`);代码可以是修复后的新版本——这正是复现与验证修复的用法。
- 时间:`SimReplayDriver` 以 journal 记录头 `tsc` 为时间,按 `(tsc, gen_seq)` 把模拟回报归并进输入流。
- **Advice 默认重算**(`--advice recompute`,缺省):重演中被重现账户的成交与 journal 不同,录下的 `Advice` 是按原状态算的,与重演状态不一致。因此 sim 模式丢弃 journal 中属于被重现账户所绑实例的 `Advice` 记录,在进程内按 advisor 规则对**重演后的**输入流运行这些实例的 `Model`(同一 `StrategySet`),产出的 `Advice` 以 `basis_seq` 处的记录头 `tsc` 加该实例在 journal 中的 Advice 中位滞后(`记录头 tsc − basis_tsc` 的中位数,先扫一遍窗口求得;条数为偶数时取较小的中间值,窗口内没有该实例的 Advice 时取 0)为时刻,按 `(tsc, gen_seq)` 插回输入流——结果只取决于 journal、种子与代码。Model 起点:`--model-checkpoint <dir>` 给出 `last_seq ≤ --from-seq` 的 advisor 检查点时从它恢复并追到起点;否则空 Model 起步,窗口内的 `InjectWarmup` 照常投递,报告标 `model_cold_start`。`--advice journal` 照读录下的 Advice,只在从 `SimReset`/账户创建处起步且 `--check-fills` 一致时与重演状态一致,其他情况报告标 `advice_inconsistent`。不被重现的账户的实例照读 journal 的 Advice。
- **压力用例**(`--stress <case>[,<case>…]`,或 `gate` = 两个门槛用例、`all` = 全部):把 [hft-sim 压力用例](../../crates/hft-sim/README.md#压力用例上线门槛必过项) 的行情变换套在输入流的全部行情记录上(引擎与 `SimExchange` 看到同一份变换后的行情),并按用例注入故障(gw 断开、引擎崩溃重启);每个用例输出 `stress-<case>.json`(`StressVerdict`)。门槛用例(`one-sided-30`、`l3-path`)判定不通过 → 退出 1;非门槛用例只报告,不影响退出码。被测账户交易的永续 inst 在 refdata 中缺分档维持保证金率 → 拒绝运行,退出 2。用例定义、起点与判定规则归 hft-sim;它们是 [testing §8](../../docs/testing.md#8-上线门槛) 门槛 ⑦ 的必过项。
- **自定义压力规格**(`--stress-spec <spec.toml>`):从文件读一个 `StressSpec`(`case = Custom`,价格路径、`StressScope`——含按 venue 单独变价的 `Venue(v)`,venue 写协议名——、盘口量、故障与杠杆取法,字段与校验归 [hft-sim](../../crates/hft-sim/README.md#压力用例上线门槛必过项)),按同一方式运行并输出 `stress-custom.json`;只报告,不是门槛,不影响退出码;规格不合法 → 退出 2。用于策略特有的场景(如两条永续腿的跨所单边行情),不需要改 hft-sim 的用例表。
- 输出目录:`decisions.parquet`(`OrderDecision`、`ExecDecision` 与风控结果)、`fills.parquet`(模拟成交与拒单)、`sim.json`(本次运行的起止 seq、种子、被重现账户,标志 `approximate`、`model_cold_start`、`advice_inconsistent`,`--check-fills` 的结果 `"match"`/`"mismatch"`/`null`)、`divergence.json`(与 journal 中同账户实际决策的第一处分歧,若有)、可选 `--emit-journal` 写回放专用 journal(`JournalWriter::open_offline` 写 `--out`),以便再做 verify。**不输出收益汇总或排行**;需要看模拟账户表现时看实时模拟账户的日报([hft-sim 报告字段](../../crates/hft-sim/README.md#报告字段))。

#### 参数相对比较(`--ab`,默认关闭)

`replayer sim … --accounts <a> --ab <paramsA> <paramsB>` 在**同一段**已录 journal、同一起点快照、同一 `[sim]` 全表与种子、同一份代码上运行两次重演,只把被重现账户所绑策略实例的 `StrategyParams` 分别替换为 A 与 B,输出两次运行的**差值**。只有显式给出 `--ab` 才会运行;不提供第三组参数、参数网格或排序。

- 参数文件:由 [tools/hft-cfg](../../tools/README.md) 生成的 `StrategyParams` 片段,只含被替换的那个策略实例;`kind_id` 与 journal 中该实例不一致、或含其他实例 → 退出 2。
- 约束:`--accounts` 恰好一个账户;强制 `--advice recompute`(显式给出 `--advice journal` → 退出 2);与 `--check-fills`、`--stress`、`--emit-journal` 互斥(同时给出 → 退出 2);`--as-sim` 可用。
- 输出 `ab-report.txt` 与 `ab.json`:`ab-report.txt` 的**第一行固定为"相对比较,不是收益估计,不得用于定仓"**,`ab.json` 的第一个字段 `notice` 是同一句话;正文只有 B − A 的差值——[hft-sim 报告字段](../../crates/hft-sim/README.md#报告字段)逐项的差(账户级与按 group)、目标与订单决策条数之差、拒单原因计数之差、第一处决策分歧的 seq;不输出任一组的绝对值。
- 差值只反映"同一段录下的行情、同一模拟撮合器"下两组参数的相对行为:行情不会对我们的订单作出反应,撮合按 hft-sim 的模型;结论不得用于定仓或放量(放量依据是实时模拟账户与上线门槛)。

### compare

逐决策比对两个实例的 seq journal,用于上线前正式实例 Shadow 运行期间与同参数模拟实例的对照([launch](../../docs/runbooks/launch.md) D 步,步骤与判定归 launch)。两个实例各有自己的 refdata 编号与行情连接,所以比对按名字映射、按时间对齐,不按 seq。D-shadow 的每日用法:

```
replayer compare --a s3://<bucket>/<prefix>/<host>/prod/… --b s3://<bucket>/<prefix>/<host>/sim/… \
                 --map <正式账户名>=sim-<模拟账户名> --from <当日 00:00Z> --to <次日 00:00Z> --out <dir>
```

参数:`--a`/`--b` 必填,接受本地冷副本目录或 S3 前缀([输入定位](#输入定位与退出码));`--map` 必填,至少一对,账户名是两边 refdata 的 `AccountMeta.label`(B 侧的模拟账户名以 `sim-` 开头),同一账户出现两次或名字在对应 journal 的 refdata 中不存在 → 退出 2;`--from`/`--to` 是 UTC 时刻(RFC 3339),缺省取两份 journal 时间范围的交集;`--window-ms` 缺省 2000(待标定,见开发备忘);`--out` 必填。

1. **映射**:`--map` 给出账户对(按账户名);inst 按 refdata 的规范符号对齐,group 按 `GroupSpec` 指纹(资产 + 各腿的 venue 与符号)对齐。两边所绑策略实例的 `kind_id` 不同 → 退出 2。
2. **参数分段**:按 `ConfigBlob`/`SetParams` 记录把时间轴切成"两边该策略实例参数体哈希相同"的区间;只在这些区间内比对,参数不同的区间在报告中列为 `params_differ`。窗口内没有参数相同的区间 → 退出 2。
3. **比对对象**(每个账户对):`ExecDecision` 的目标类决策(设目标、清目标;group、目标量、`reason`、`origin`)、模式门结果与 `OrderDecision`(有的话),以及策略 `Note`。时间用记录头 `tsc` 经段头校准换成 UTC。
4. **对齐**:按时间顺序,A 的每条决策与 B 中同 (group, 决策种类)、UTC 时间差的绝对值 ≤ `--window-ms`、尚未配对的最早一条配对;配对后逐字段比较,目标量按两边各自 inst 的 lot 归一后比较。
5. **输出**:`compare.json`(各类计数:一致、字段不同、只在 A、只在 B;配对时间差 B − A 的 p50/p99;参数分段表)与 `diffs.parquet`(每条不一致或未配对的决策:两边的 seq、UTC 时间、字段 diff,以及该时刻两边该 group 各腿的 BBO、账户仓位与可用保证金视图,供人工解释)。不输出收益。
6. 有任何不一致或未配对 → 退出 1(差异需要逐条解释,解释记录归 launch);全部一致 → 退出 0。

### latency

按事件加权的主机内延迟统计:对 seq journal 中**每一条**带 `MarketHdr` 的行情记录(含分档深度快照;记录集合唯一归 [hft-telemetry](../../crates/hft-telemetry/README.md) 的 `event_weighted()`),用记录头的 `tsc` 与 `MarketHdr` 里的 `hw_rx`/`sw_rx`/`rx_tsc` 时间戳算出 `kernel_rx`、`ingest`、`feed_host`(分段定义归 [latency-budget §2](../../docs/latency-budget.md#2-分段名全仓唯一))。数据全部来自 journal,热路径上没有额外成本;它补上探针按时间抽样会低估的突发尾部。门槛 ⑩ 的 `ingest` 与 `kernel_rx` 读这份统计,探针样本只用于订单类分段(判定归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法))。

1. **计算**在 [hft-telemetry](../../crates/hft-telemetry/README.md) 的离线模块 `event_weighted()`;本命令只负责输入定位、按记录扫描、分组与输出,不运行 sequencer,不读当前配置。
2. **跨时钟域换算**:含 PHC 端点(`hw_rx`)的分段,按样本前后两条 `ClockStatus`(housekeeping 每 1 000 ms 写一条 `(tsc, phc, calib_err_ns)`)线性插值换算到 TSC 域,逐样本给出误差界;误差界超过上限的样本计入 `unsynced_cal`、不进分位数;记录没有 HW 时间戳(`SW_TS`)时以 `sw_rx` 为锚。插值方法、误差界与上限归 [latency-budget §2.1](../../docs/latency-budget.md#21-跨时钟域换算)。
3. **分组**:`event_weighted()` 按数据集(段首 `HostProfile` 与发布版本,latency-budget §5 的数据集维度)各返回一份报告,按 seq 顺序;不同数据集不合并,`prod` 与 `sim` 的 journal 本来分开。每份报告内按 hft-telemetry 的 `EwKey`(venue、记录种类、分段、HW/SW 时间戳来源、inst 类别、突发/平静层)分组;`--venue` 只保留所列 venue 的组。**窗口**:`--from`/`--to`(缺省为输入的第一条与最后一条记录)按段头的时钟校准把记录头 `tsc` 换成 UTC,UTC ∈ [`--from`, `--to`) 的记录构成 `EventWeightedCfg` 的 seq 区间(`--from` ≥ `--to` → 退出 2);`--to` 之后的记录不计样本,但其后第一条 `ClockStatus` 照常读入(插值需要样本之后的那条校准对)。`cal_max_err_ns` 与 `burst_gap_ns` 取命令行 `--cal-max-err-ns`、`--burst-gap-ns`(必填;取值归 [latency-budget §2.1](../../docs/latency-budget.md#21-跨时钟域换算) 与 testing §8.2,由运行者按当时的值给出),不读任何配置文件。
4. **输出** `--out` 目录:`latency.json` = `cfg`(`from_seq`、`to_seq`、`cal_max_err_ns`、`burst_gap_ns`)+ `datasets` 数组,每个数据集一项:`host_profile`(十六进制)、`release`、`n`、`missing`、`gap_missing`、`negative`、`unsynced_cal`、`unsynced_cal_by_stratum`、`sw_anchor_share`(SW 锚样本占比)、`groups`(每个 `EwKey` 的 p50/p90/p99/p99.9/max 与样本数;`kernel_rx` 组即下界)、`kernel_rx_hi`(上界的同一组分位数;上界公式归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法)、算法归 hft-telemetry)、`cal_err`(逐样本误差界的 p50/p99/max)与 `probe_strata`(hft-telemetry `stratified_probe` 对同一窗口探针样本的分层结果:每组两层的样本数、p50/p99、权重与合成值,样本不足的层与合成值为 `null`);`latency-<k>.hdr`(第 k 个数据集,k 从 0 起;HdrHistogram V2 序列化,每个 `EwKey` 与每个 `kernel_rx_hi` 组各一个直方图,同一数据集的逐日输出按直方图相加合并成 7 天窗口)。同一输入两次运行输出逐字节相同。
5. **运行位置**:任何装有发布制品的主机,对本地冷副本或 S3 前缀运行;⑩ 测量窗口内不在被测主机上运行(窗口外或在别的主机上读 S3)。退出码:0 完成(含存在 `unsynced_cal` 的情况,是否可接受由 testing 判定);2 参数错误;3 输入不完整。不运行 sequencer,journal 引用未注册的 `kind_id` 不影响本命令(不退出 4)。

### bench(可选扩展)

触发条件:开始做纯计算回归门槛时(参考机从 M1.5 起按需租用,[benchmarks](../../benchmarks/README.md))。回放同一份 journal,测每条输入在 sequencer 内的处理时间(`decided_tsc − proc_tsc` 的回放等价物);绑到一个核,预热 1 个完整段;按 (kind, strategy) 输出 hdrhistogram p50/p99/p99.9/max。`--baseline` 给出时任一 (kind, strategy) p99 上涨 > `--threshold-pct`(10%)→ 退出 1([09 §2](../../design/09-可观测性、风控与控制面.md));基线与机器类型绑定。`cargo xtask bench-gate` 调用它。

### raw-decode(可选扩展)

触发条件:需要证明解码器改动不改变结果、或补充解码金样时。用当前 `venue-*` 解码器重跑 raw journal,与 seq journal 中实盘解码结果比较(同一 `rx_tsc` 的记录);不一致时输出 raw payload 与两份解码结果,可直接加入 [fixtures](../../fixtures/README.md) 金样。

### `--chain`(可选扩展)

触发条件:链上策略迁移、chain-core 启用时。`replayer verify --chain --journal <chain-bsc 段目录>` 用 chain-core 的回放驱动重放 `chain-bsc` journal,逐字节比较 `TxLifecycle` 迁移、钱包状态迁移与 `CmdResult`;驱动与分类表由 [hft-chain-evm](../../crates/hft-chain-evm/README.md) 提供。

### 输入定位与退出码

- `--journal`、`--a`、`--b` 接受本地目录(冷副本布局)、单个段文件,或 S3 前缀 `s3://<bucket>/<prefix>/<host>/<instance>/…`(带主机段;前缀与对象键布局归 [journal-archiver](../journal-archiver/README.md#输出s3-布局));`--model-checkpoint` 接受本地文件、目录或 S3 前缀。S3 输入先下载到 `--cache-dir`(缺省为系统临时目录下本进程新建的目录,退出时删除;给出时保留,哈希相符的已有文件不重复下载),解压后核对 manifest 中的 SHA-256,不符则退出 3。读 SSE-KMS 前缀(如 `private/`)的对象需要运行主机的角色有对应 KMS 解密权限(授权归 [deploy](../../deploy/README.md));下载失败(含无解密权限)退出 3。
- 多段输入按 `first_seq` 排序并校验段链哈希,断链退出 3。
- `--instance` 给出时,段头的 `instance` 与之不符 → 退出 2。

| 退出码 | 含义 |
|---|---|
| 0 | 一致 / 完成 / 基线未回归 |
| 1 | verify 发现分歧;`sim --check-fills` 成交不一致;门槛压力用例不通过;compare 有不一致或未配对的决策;bench 回归超阈值 |
| 2 | 参数或配置错误(含检查点键不符、`--ab` 参数文件不符或与其他选项冲突、压力用例缺分档维持保证金率、`--stress-spec` 不合法、compare 的 `--map` 不合法、策略类型不同或无参数相同的区间、sim 起点不是快照或 `--accounts` 含未经 `--as-sim` 的 Live 账户、`--instance` 与段头不符、每日模式与互斥选项同时给出、latency 的窗口为空) |
| 3 | 输入不完整(断链、哈希不符、缺快照、S3 下载失败、检查点文件损坏) |
| 4 | 输入 journal 的 schema 版本或策略集合与当前二进制不兼容(journal 引用了未注册的 `kind_id`) |

## 实现说明

<details><summary>实现要点</summary>

- 同一代码:replayer 链接的 `hft-sequencer`、`hft-exec`、`hft-risk`、`hft-oms`、`hft-ledger`、`hft-sim` 与策略集合同 hft-core 同版本、同 feature;发布制品里的 aarch64 replayer 与 hft-core 同一 `-C target-cpu` 构建;`replayer --version` 打印与 hft-core 相同的版本串与策略集合。CI 中 verify 用**被测提交**的 replayer 对样本或昨日 journal;质量门在 x86_64 runner 上运行,对同一 journal 必须得出与 aarch64 相同的结果(确定性不依赖架构,这本身是被测性质)。
- 读 journal:`hft-journal` reader 直接 mmap 段文件,记录零拷贝交给 sequencer;`MADV_SEQUENTIAL`,预读 64 MiB。
- 确定性前提由 sequencer 保证;replayer 自身不得引入任何非确定输入(不读环境时间、不读当前配置文件;`sim` 的随机性只来自种子)。
- `SimReplayDriver`(本 app,`src/sim.rs`)实现 sequencer 的 `Driver`:输入来自 journal(过滤掉被重现账户的交易所回报),Output 中属于被重现账户的订单类决策转给 `SimExchange::on_command`,`SimExchange::drain_due` 的回报按 `(tsc, gen_seq)` 插入输入流;`advance` 以已送入的最后一条记录的 `tsc` 推进。压力用例的故障由它注入:`GwDown` 交给 `SimExchange` 的连接状态接口;`CoreRestart` 在指定时刻按崩溃处理(丢弃未提交尾部,从本次重演最近的快照经恢复驱动重新起步,`down_ms` 内只向 `SimExchange` 送行情,恢复后按启动规则继续;`Recovery` 是否自动解除与实盘相同,按 [hft-core 启动顺序](../hft-core/README.md#启动顺序)步 8)。
- `--ab`:两次重演各自独立(各一份 sequencer、`SimExchange` 与 Model),共享只读的 journal 映射;参数替换在起点快照恢复之后、第一条输入之前以一条合成的 `SetParams` 输入进入(不写回任何 journal)。
- `compare`:两份 journal 各用一个只读扫描器取出 Output 类决策与参数记录,不运行 sequencer;对齐只依赖记录内容与段头时钟校准,结果确定。
- `latency`:一个只读扫描器按 seq 取出行情记录与 `ClockStatus`,先缓存到下一条 `ClockStatus` 再交给 `event_weighted()`(插值需要样本之后的那条校准对);内存只与两条校准对之间的记录数成正比。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/main.rs` | 子命令与退出码 |
| `src/input.rs` | 本地/S3 输入定位、下载、哈希与段链校验;检查点定位 |
| `src/verify.rs` | 分类执行、逐字节比较、分歧报告、`--advice recompute` 与 Model 起点 |
| `src/sim.rs` | `SimReplayDriver`、账户映射、成交比对、Advice 重算与插回、`--stress` 与故障注入、输出 Parquet |
| `src/ab.rs` | `--ab` 两次运行与差值报告 |
| `src/compare.rs` | 映射、参数分段、对齐、差异输出 |
| `src/latency.rs` | 行情记录与 `ClockStatus` 扫描、调用 `event_weighted()`、分组与直方图输出 |
| `src/bench.rs`(可选扩展) | 绑核、计时、基线比较 |
| `src/raw_decode.rs`(可选扩展) | raw 重解码对比 |
| `examples/replay_probe.rs`、`tests/fixture/{path_model,restart_model}.rs` | CI 的独立测试策略装配、1 h/3 h 文件及跨重启检查点文件生成;不属于发布二进制 |

</details>

## 延迟与资源契约

| 指标 | 目标 | 测量 |
|---|---|---|
| verify 吞吐 | 一天 seq journal 的回放时长,上限唯一登记在 [benchmarks 路径基准与门槛](../../benchmarks/README.md#路径基准与门槛) 的 `replay-throughput` 行 | 基准 `replay-throughput`(fixtures 1 h 样本 × 24 外推 + 每周一次真实整日) |
| sim 吞吐 | ≥ verify 的 50%;`--ab` 为两次重演,耗时约为单次的两倍 | `sim/replay_day`([hft-sim](../../crates/hft-sim/README.md#延迟与资源契约)) |
| compare 吞吐 | 两份一天的 journal < verify 一天的耗时(只扫 Output 与参数记录) | 基准 `compare_day` |
| latency 吞吐 | 一天的 seq journal < verify 一天的耗时(只扫行情记录与 `ClockStatus`) | 基准 `latency_day` |
| 内存 | 单进程 ≤ 所在主机类 `hft-replay-verify` 的 `MemoryMax`([hft-core 整机表](../hft-core/README.md#内存与-journal-段按主机类唯一归属))+ journal 页缓存(共享) | `/usr/bin/time -v`;cgroup `memory.peak` |

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-sequencer`、`hft-strategy-sdk`、`hft-exec`、`hft-strat-lib`、`hft-oms`、`hft-risk`、`hft-ledger`、`hft-book`、`hft-event`、`hft-refdata`、`hft-sim`(默认 feature,纯核心)、`hft-journal`(`JournalReader`;启用 `writer` 特性只为 `--emit-journal` 的 `JournalWriter::open_offline`)、`hft-strategy-registry`(经它引入已注册的 `strategies/*`)、`hft-telemetry`(直方图、账户报告字段与 `event_weighted()` 的计算)、`venue-*`(仅 raw-decode,可选扩展)。
- 外部:`arrow`/`parquet`(输出)、`zstd`、`tokio` + AWS SDK S3(只在输入下载环节)、`hdrhistogram`、`clap`。
- 禁止:`hft-net`、`hft-venue` 的运行时(不连任何网络)、`JournalWriter::open`(热段写者);`--emit-journal` 不写 hugetlbfs/tmpfs、不写生产 journal 根。

## 延伸阅读

- [08 数据面与回放](../../design/08-数据面与回放.md)、[07 策略层](../../design/07-策略层.md)、[10 §5](../../design/10-语言分工与代码组织.md)、[09 §2](../../design/09-可观测性、风控与控制面.md)
- [round8-decisions](../../design/_work/round8-decisions.md)(R8-5、R8-6:模拟系统、journal 重演只用于排障;R8-10:Shadow 对照与参数相对比较)
- Agent Note:`shadow-before-live`、`sim-accounts-not-backtest`
- [architecture.md §9](../../docs/architecture.md#9-回放恢复与确定性)、[testing](../../docs/testing.md)、[hft-sequencer](../../crates/hft-sequencer/README.md)、[hft-sim](../../crates/hft-sim/README.md)、[hft-py](../../crates/hft-py/README.md)

## 已知限制与后续工作

- RPL-270/271 在 CI 现场生成完整 1 h/3 h 的路径 Model 文件,由同一入口的独立测试进程执行。单个 Live 账户 acct-l 绑定实例 tsa,真实 SimExchange 产生回报,advisor 每 10 min 写检查点,sequencer 每 5 min 请求关键帧并保留整点滚段;检查点后窗口的比较数与路径版本变化后的 120 s 重建点从输入独立计算。CI 先构建 examples 再运行测试。生成文件尚未发布为带哈希的固定 T1/T3 样本,不计 RPL-010/040/046 的固定样本验收。

- 当前可执行入口覆盖本地连续、封闭 seq 文件的 `verify --advice journal|recompute`,支持单文件或目录递归搜集 `.jrn`、from/to seq、instance 校验、keep-going、JSON 报告与只读 `--check`。生产二进制只装配 registry;集成测试可向同一泛型入口注入测试策略集。读取直接使用原路径,先完成整组文件预检;目录内不按文件名推断顺序。
- 输入缓冲默认上限为 1,000,000 条逻辑记录、512 MiB 逻辑体、128 MiB 单条重组;可分别用 `--max-records`、`--max-body-bytes`、`--reassembly-bytes` 覆盖为正整数。超限退出 3,不返回部分验证结果。来源须保持不可变;不读取符号链接。`--check` 仅验证输入加载,输出明确标记未进行重放;不写报告。
- 连续封闭文件的恢复、指定快照恢复和 build_git 变化按 [sequencer 独立运行契约](../../crates/hft-sequencer/README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用) 重建。报告的 runs 数组逐运行列出 start_snapshot_seq、stop_before_seq(下一运行首 seq,末次为 null)、inputs_replayed、outputs_compared、snapshots_checked、mismatches 和 recovery_diffs。RPL-025 覆盖 20 min 后从 10 min 快照恢复再运行 20 min,含末片 seq 审计和旧行情篡改;RPL-026 覆盖真实 Drain 后换 build_git 再运行 10 min。两者保留新运行的整点滚段,三段文件仍计两次运行,只保留新运行的两段文件时独立验证。离线写者不运行 persist,长时夹具根据实际段头、完整 End 与文件摘要生成测试封签,不改 journal 字节,由正式读者校验。时间来自确定性虚拟时钟,每次重建均使用当前二进制,不证明跨版本语义兼容。
- 本地 Model 检查点支持任意名称的单文件或按实例名取文件的目录;必须显式搭配 recompute。`--max-model-records` 默认 1,000,000,约束待配对和差异条目;`--max-model-bytes` 默认 512 MiB,分别约束读入的检查点总字节和镜像待配对/历史产出/差异载荷合计字节。超限退出 3。重算仍使用原 journal Advice 驱动普通验证,报告单列 model_divergences、model_no_start、first_model_divergent_seq 和 first_output_divergent_seq;所有带 Model 的实例均缺起点时不填 coalesced。NoModel 由 SDK 类型查询识别,不从布局或资源声明推断。
- 每日报告、RecoveryGap、S3/zstd、sim/compare/latency 仍未接入,不支持的选项返回 2。完整 RPL-010/040/046 等 T1/T3 样本验收、组件二分和长时段流式内存门槛尚未完成。当前报告提供 masked kind/body 十六进制、语义标志、字段差异、最近快照及前 1,000 条已处理 Input 摘要;不能以短时真实文件用例代替完整回放验收。RecoverMismatch 注记只保存每个账户/venue 的首处分歧,故接受范围内的后续差异也逐项报告;缺少被引用旧尾部的单段文件不能作通过凭证。
- `--report` 使用已有目录中的 `.json` 文件,以同目录临时文件替换;分歧侧文件格式见 [verify](#verify)。字段差异按 schema 的 offset/width 截取,前缀后的可变载荷单列 payload,不归到最后一个头字段。输入文件、seal 或已读检查点路径不能用作报告目标。写出失败返回 2;输入损坏 3、schema/策略种类不兼容 4、输出或 Model 分歧 1 与正常 0 分别保留。

- **跨版本 verify 不成立**:verify 只证明"当前代码对这份 journal 的行为与生产一致";有意改变行为的新版本,分歧是预期的。发布流程用"旧版本 verify 旧 journal(必须一致)+ 新版本在模拟账户上运行"组合判断,见 [release-and-deploy](../../docs/runbooks/release-and-deploy.md)。
- 事件 schema 只追加,但新 kind 的输入在旧 journal 里不存在;旧 journal 喂给新代码时,新 kind 相关逻辑不被覆盖。
- `sim` 的结论受 hft-sim 的已知限制约束(无市场冲击、延迟是抽样的);它回答"会怎么走",不回答"会赚多少"。`--ab` 的差值同样只在这一模型内成立。
- `compare` 的两边行情来自各自的连接,决策时刻天然有差;时间窗过小会把同一决策判成两条未配对,过大会错配相邻决策。

### 开发备忘

<details><summary>给实现者</summary>

- `--advice recompute` 要求 advisor 的 `Model` 以事件驱动、只用事件时间运行(SDK 规则);若某策略的 Model 依赖墙钟或 I/O,本检查会发现。sim 模式的 Advice 插回时刻用"journal 中位滞后"是为了让重演的建议年龄分布与实时运行相近;它只影响排障重演,不影响 verify。
- **待标定**:`compare` 的 `--window-ms` 默认值与按 group 的配对规则,在正式实例 Shadow 的第一周按实际的配对时间差分布调整。
- **开放问题**:每日任务遇到当日含版本切换(段内有版本变化的引导块)时,切换之前的区间由旧二进制产生,用已安装的新二进制 verify/recompute 可能不一致;候选处理是按段换用对应版本的 replayer,或该区间在报告里标 `skipped_reason = release_change`(报告格式归 [testing §3.5](../../docs/testing.md#35-验证回放))。
- **开放问题**:`CoreRestart` 故障注入需要 sequencer 在同一进程内从内存中的快照重新 `boot`;若 hft-sequencer 的恢复驱动只接受 journal 读者,则由本 app 先把重演产出写成内存段再交给它。
- 实现顺序:input + verify → `--advice recompute` 与 `--model-checkpoint` → sim(先重现 Sim 账户,再 `--as-sim`)→ `--stress` 门槛用例 → compare → 非门槛压力用例 → `--ab` →(可选扩展)raw-decode、bench、`--chain`。

</details>
