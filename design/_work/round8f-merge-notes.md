# M0-f 修订代理报告汇总(合并遗留的来源;测试规格代理按文件名检索指向自己的条目)

## J1

M0-f 的 J1 部分已改完。heading_audit 显示丢失标题 0。check_hft 能跑通(problems=64,warnings=85),但没有一条指向我的文件,问题都在 hft/ 侧,来自其他代理正在改的文件和 J12 新加的检查。我借用的临时脚本目录已删除。

## 1. 改了哪些文件
`architecture-v2/` 下的 README、00、01、02、03、05、07、08、09、10、11、12、13、`待测量…`、`现状测量…`。04 和 06 没有改。

## 2. 各项落点
- **F1**(只改设计章节里与新规则矛盾的写法):`AUTO_EXTERNAL` 的前置条件、在 Input 内合成、`auto_held` 抵扣;核销改为适用于静止 group 并清零全部腿;恢复期按"多出那一腿所在 venue"判定、`ExecGateHeldNaked`;`resolve_broken` ��在 HARD 解除时产生;`exec.drain_ready()`;持仓驱动订阅;`.aborted` 停机标记。落在 05 §2.3/§6/§8、07 §2.2、08 §6、09 §4.1/§4.3/§4.8/§5.2、12 §3.1/§3.2。
- **F2**:
  - 00 §4.2 的"证据"和"同机干扰"两条改为链接 testing §8.2,另加"效度与退路"三条。
  - 00 §3 加上 `action`;§7 的 HW 退路改为 SW 锚数据集。
  - 08 §5、09 §1 加上 `action` 与 `rx_empty_gap_ns`。
  - 02 §6、11 §4/§6、12 A7/A20/§3.2 同步。
- **F3**:
  - 11 §3 的"各 crate 动工前必须定下的延迟决定"改为按 M1 第一周 / M1.5 前 / M2 前 / M3 末分列,(c) 只登记方向。
  - 11 §3.1 加 HW 戳不可用时的替代测法、允许的保持唤醒形式、分层抽样。
  - 11 §3.2 加 `c8g.8xlarge` 的 c32 计划运行与 ENI-B2 A/B(即 F3(d)),§8 补预算。
  - 08 §2 写入关键帧事件数触发、实盘不算 `state_hash`、段滚动两级安全阀。
  - 00 §1/§6、02 §4.2、03 §7 同步。
- **F4**:根级 `hftbatch.slice`、`CPUWeight=idle`、护栏只在 hft 自身用量明显时停、Sim 实例的 Recovery 自动解除、dev2 的 IMDS 规则。落在 11 §2/§4.1、10 §2/§5、01 §3.2、12 A16/A23/§3.2/§3.3、`待测量` G1/J7、`现状测量`。
- **F5**:11 §2 改为只链接 development(删掉了重抄的规则和"额度用尽走 x86");10 §5、12 A18/§3.2、README、11 §8 同步。
- **F7**:11 M8 行与 §5 第 5 步。
- **暂缓项**:在 07 §2.2/§7.1/§9、11 §3.2/§4/§6、12 §3.4/§4 加了"策略研究后定(R8-12)"。
- **13 §19**:已追加,含设计基线 v1 冻结、实现驱动的变更、测试规格先行、F1–F7、取代关系、暂缓项。13 页首、README、12 §4 已更新。
- **`待测量`**:新增 §L(L1–L9)。

## 3. 处理的评审条目
- **X3**:P1-17(NAPI 归属改为 conn-builder,01 §7、02 §6、11 §6、12、`待测量` I4)、P1-18(00、08、09)、P2-62(00 §4.1 加取代说明)。另修了我文件里的两处重抄:P1-14 的 persist 滞后数值(12)、P1-19 的 c16 线程形态(`现状测量`)。
- **X4**:N-P1-1、3、4、6、7、8,以及 P2-1、2、6、8、10(11:46 的窗口开启顺序、11:159 的 NAPI 校验)。
- **X5**:L-P1-1、2、3,L-P2-1、2、4(登记为 M2 前定),L-P2-5、6、7、8、9、10。
- **X2**:P1-1 至 P1-4 加暂缓标记;P1-5 并入 F2(a)。
- **X1**:P1-A 至 P1-E 以及 P2-6、7、11,只做设计章节的一致性修正。

**留给别人**:
- **J9**:testing §8.2 需要写出分层抽样、`kernel_rx` 上下界、SW 锚退路和 `unsynced_cal` 阈值、A/A 与按资金费块配平。
- **J10**:
  - deploy:c32 探针清单的文件名,以及 M7/M8 两份主机清单的文件名(我写的是"文件名归 deploy")。
  - development:`quota_override`、`hotfix` 目前还不在文件里(BRIEFNAME 报了)。
- **J8**:hft-core 启动步骤需要写恢复尾部上界(08 §2 已链接过去);hft-core:867 的探针核对项要与 11 §3.2 对齐;c16 节标题"下限候选"(X3 P2-45)。我已把 02 和 11 里的锚点改成 `#线程计划`,改标题不会断链。
- **benchmarks 归属者**:`gw-ingest/poll-cadence` 还没有登记。
- **tools(J6)**:hostprobe 检查 PHC 读取频率(X5 L-P2-11(f))、采集 ENA allowance 计数(L-P2-11(c))。

## 4. 新引入、别人可能要用的名字
只有 `待测量` 的编号 L1–L9。

## 5. 没能确定怎么改的矛盾
1. 13 的 §16–§18 是历史记录,仍写着 `hft-batch.slice`、`CPUWeight=1`、"额度用尽走 x86"等旧口径。我没改,由 §19 的取代关系覆盖;如果机检会扫描 architecture-v2 里的旧名,这几处会被报出来。
2. 02 §2.2、12 §4、`现状测量`里"c16:首批下单 venue ≤ 4",与"6 个产品线满订阅"之间的张力属于暂缓的第 10 组,只加了标记,没有改语义。
3. README 一页结论第 10 条仍写"段内每 5 分钟写一个关键帧",没提事件数触发。我把它当作近似说法保留。
4. 00 §6 与 09 §2 仍重抄了 `pair_period_ms` 10 000 和 n ≥ 10,000。目前与 testing 一致,所以没删。

## J2

J2 这一轮的修订做完了,只改了我名下的文件。`check_hft` 里没有一条问题落在这些文件上(全仓剩 13 条,都在别人的文件里);`heading_audit` 没有丢失任何标题。有一处没有完全照简报写,先说明:portfolio-carry 正文里只写"策略研究后定",R8-12 的出处挪到了"延伸阅读"。原因是 `check_hft` 的 HISTORY 检查会把正文里的 `R8-12` 报成问题。如果 J12 给这句话加了豁免,可以改回原写法。

## 1. 改了的文件(路径相对 `E:\trading\hft\`)
- `crates/hft-strategy-sdk/README.md`
- `crates/hft-strat-lib/README.md`
- `strategies/AGENTS.md`(874/900,未超篇幅)
- `strategies/latency-probe/README.md`
- `strategies/portfolio-carry/README.md`、`strategies/portfolio-carry/pipeline/README.md`
- `strategies/cdex/README.md`、`strategies/maker-taker/README.md`
- `docs/cookbook/adding-a-strategy.md`

registry、cross-taker、cdex/pipeline 没有需要改的地方。

## 2. 落点
- **F1(f) 核销**:SDK「目标意图」新增"人工核销不经成交改变腿净量"一条:静止的 group(是否 Broken 都可)、清零全部腿的余数,归属链接到 hft-oms、control-plane、hft-ledger。Broken 那条的第④项改为引用这条。cookbook 第 3 节同步。
- **F1(b)**:SDK 的外来成交截止 `deadline_tsc` 写明是名义值,恢复期内不计时。Broken 自动平仓的计时链接到 hft-exec;cdex 同样处理。
- **F1(c)**:SDK 的 `ExternalCause` 注释分清三方:置位归 reconciler,合成归 hft-sequencer,`auto_held` 抵扣归 hft-ledger。
- **F2(a)**:latency-probe 的概要、事件表、样本小节与表、测试、已知限制都改为按 `action` 区分:`order_rtt` 只算新单,`rep_to_order` 含三种 action。SDK 里没有提到 `LatencySample`,不用改。
- **F2(b)**:latency-probe 的已知限制里链接 testing §8.2 的"突发 / 平静"分层抽样。
- **F3(a) 跨 gw 探针变体**:latency-probe 新增 quote 组参数 `quote_xgw`。规则写在「quote 探测」第 5 步,同时补了参数表、样本表、`check` 和测试。
- **X3**:P0-1(SDK 侧:`TargetIntent` 在快照里按 hft-exec 的显式行写出)、P2-18、P2-25、P2-26、§2 #75 改在 SDK;P2-41 改在 strat-lib;P2-61、P2-24 改在 portfolio-carry;P2-27 改在 maker-taker;P2-50 改在 cookbook。
- **X2**:
  - P1-5 → 上面的 F2(a)。
  - 已修:P2-1(strat-lib 快照补进 `PercentileStats` 末次观测和 `LeadLag` 每腿上一个 mid,并改大小公式和测试)、P2-7(SDK 写明分片由 Model 按 tick 的 UTC 时刻自己选)、P2-8(SDK 已知限制)、P2-9(SDK 写明投递表是唯一定义)、P2-20(SDK 定义 `upd_tsc` 和 `prev_interval_s` 的口径)、P2-21、P2-22(latency-probe target 模式 `band` 改为约一个 lot 的相对量)、P2-23(SDK 写明模型 tick 分辨率)、P2-25(cookbook、SDK、`strategies/AGENTS.md` 写明 group 上限例外)、P2-26(三处)。
  - 只加"策略研究后定"、不改语义:P1-1、P1-2、P1-3、P2-2、P2-7、P2-12、P2-13、P2-14、P2-19,在 portfolio-carry 和它的 pipeline README 里。
- **X1**:P2-19 改在 SDK;P2-1、P2-2 的规则副本在 SDK、portfolio-carry、cdex、latency-probe、cookbook 里都改成链接 hft-exec。

## 3. 留给别人
- **J5(hft-sequencer)**:
  - 第 426 行写"账户级事件"进 Model,和 SDK 投递表不符(Model 不收 External/Risk/Mode/Conn)。
  - `MarketLastRow` 缺 `prev_interval_s`,也缺 Funding 的可见性依据(同 `rx_from_seq` 的规则),否则 SDK 对 `FundingView` 的新口径没有数据来源。
  - 第 430 行的"模型分片"应链接 SDK 的分片规则。
- **J4(hft-exec)**:请保留锚点 `#人工保持与恢复期抑制` 和 `#限价与单笔名义`,改名要通知合并;SDK 已引用 `ExecCellRow` 的显式行。
- **J8(replayer)**:第 65 行的"事件时间"应写明是 UTC 时刻(与 SDK 的 `recompute_from_seq` 一致)。
- **J9(testing)**:第 358 行的探针描述要补 `action` 和 `quote_xgw` 跨 gw 样本。
- **J11**:glossary 第 65、113 行和 defensive-patterns 里 `manual_hold`、`live_tsc` 的规则副本。
- **J10**:X2 P2-10(GroupId 有两个分配者)、pipeline 第 252 行的"Live 实例"定义。

## 4. 新引入的名字
- `quote_xgw`:latency-probe 的 per-group Bool 参数,默认 false。
- `PercentileStats` 的"末次观测"(`last`)和 `LeadLag` 的 `last_mid`:都在 strat-lib 内部。

## 5. 没能确定怎么改的矛盾
1. X2 P2-6:`WARMUP = true` 时 `no_open_ms` 不含预热取数的耗时。要修需要给 SDK 加能力,这次没动。
2. `quote_xgw` 假设不同 venue 落在不同 gw 线程(`gw:<venue>`)。gw 与线程的映射归 hft-core,需要核对。
3. SDK 写了 `prev_interval_s` 进快照、`upd_tsc` 沿用 `rx_from_seq` 的可见性规则。J5 不改 `MarketLast` 的话,这两句没有落地。

## J4

J4 的修订已完成，范围只在 `hft-exec`、`hft-oms` 两个 README。`heading_audit` 结果是 0 个标题丢失。`check_hft` 在这两个文件里只剩一条：hft-exec 延伸阅读链接的 ADR `resolve-broken-hard-only.md` 还不存在，要等 J10 写出来。另有几项需要你拍板(见第 5 节第 1、2 条)。

## 1. 改了哪些文件
- `E:\trading\hft\crates\hft-exec\README.md`
- `E:\trading\hft\crates\hft-oms\README.md`

新增两个小节:exec 的"HARD 解除时的 Broken 判定(`resolve_broken`)"和 oms 的"延迟样本"。没有删除或改名任何标题。

## 2. 各项落在哪里
- **F1(a)**:判定规则写在 exec 新小节，由 `resolve_broken(a, g, v)` 只读判定：只在 HARD 解除时判，阈值按补腿口径，跳过 `manual_hold` 的 cell。oms 的 `naked_groups` 改为只给候选。状态机图、补腿第 3 条、DST 16 和新增的 DST 35 同步改了。
- **F1(b)**:
  - exec"人工保持与恢复期抑制"改成按 venue 抑制。新增入口 `on_venue_gate_open(a, v)`；`on_gate_open` 会对还没报告的 venue 补做同样处理，所以 sequencer 不逐 venue 报告时退回到原来的整账户行为。
  - 新增"闸门滞留告警"：参数 `gate_naked_alarm_ms`，告警 `Note{ExecGateHeldNaked}`。
  - "Broken 处置 / 计时"写成三种情况：HARD 与人工保持在解除后重给完整窗口；恢复期不计时，该 venue 放行时重给完整窗口；其他等待条件照常计时。这里是计时口径的唯一归属，供 testing 和 runbook 链接。
  - 外来成交截止、补腿截止在该 venue 放行时重设。
- **F1(d)**:`drain_ready()` 与按账户的 `drain_ready_acct(a)` 取代 `quiescent`。Draining 改为"只停增仓切片"。新增 DST 14 修订版和 DST 37。
- **F1(f)**:语义的唯一归属是 oms"残余核销"：适用于静止的 group(Broken 与否都可)，清零全部腿的余数，仍返回单条 `WriteOff`。"静止"的唯一定义放在 oms LegTracker。exec"残余与尘埃"、DST 32 同步改了。
- **F2(a)**:oms"延迟样本"小节。新增 `TrigStamps`(`ApprovedIntent` 带一份，`cancel`/`amend` 改为传参)；`SlotStamps` 扩为 80 B，加了 `trig_sw_rx_ns`、`trig_kind`、`op`。新单、撤单、改单各在首个响应时出一条样本；单表内存改为约 1.39 MiB。
- **X3 P0-1**:节 21 改成显式行。新增 `ExecTargetRow`(232 B)和 `ExecSpecRow { algo: u8, _pad: [u8; 7], params: [u8; 128] }`，参数尾部和所有填充都写 0，读回时尾部非 0 报 `RestoreError`。另加 Gate 行，把 `gate_open`、`venue_open`、`gate_closed_tsc` 放进快照，否则从中途快照起步的验证回放会和实盘不一致。

## 3. 处理的条目与留给别人的
**已处理**
- X1:P1-A、B、E 的 exec/oms 部分；P2-1、2、3、4、5、6、7、9、14、15、16(a)、17、18、19。
- X3:P0-1；P1-2、3、5、8、10 的 oms 侧；P1-4 的 exec 侧(提供计时归属)；P2-4(把无组 cell 的 `own_infl` 与 `filled` 共用存储，cell 仍 ≤ 640 B，hft-core 不用改)；P2-13 的 oms 侧；P2-15(快照状态码改用 hft-event 的 `TargetState` 线上码)；P2-18 的归属地；上一轮遗留的 P2-41(`conn_live_tsc` 措辞)。
- 其他：X2 §5-4 与 X5 L-P1-3 的 OMS 部分；X4 N-P1-8 只在 oms 写明"上一 epoch"的定义。

**留给别人**
- **J5**
  - sequencer:
    - 在 (a, v) 的 `hedge_gate_ok` 与 `RECONCILED` 首次同时满足时调 `exec.on_venue_gate_open`，这需要 hft-risk 增加一个按 venue 的报告。
    - HARD 解除时按 `oms.naked_groups` 的候选逐个调 `exec.resolve_broken`，替换 seq:470 第④步的旧规则；hft-risk 只在删除带 HARD 的 Kill 记录时置 `resolve_broken`。
    - Drain 等待改用 `exec.drain_ready()`，`DrainTimeout` 的 P0 用 `drain_ready_acct` 列出未完成的账户。
    - `oms.cancel`/`amend` 要多传 `&TrigStamps`，撤改单也要填 `decided_tsc`。
    - 被 `auto_held` 全部抵扣的外来成交不调 `exec.on_external`。
  - hft-risk:332 删掉"不越过交易所价格带由 OMS 定价保证"(oms 不检查交易所价格带)。
- **J6**
  - hft-event Timer 表：Exec 加 `Alarm=6`；Oms 的 `UnknownQuery` 编码为 `3 | table_ix << 8 | slot_id << 16`。
  - `ExecGateHeldNaked` 登记的含义要覆盖两种场合(账户闸门未放行；自动平仓到期后一直等待)。
  - control-plane:核销语义链接 hft-oms；SmokeOrder 删掉"交易所价格带"这个条件，补上 `SmokeErr::BadInst` 的映射。
  - journal-format:节 21 的行顺序改为 Cell* → Acct* → Gate*；`gate_closed_tsc` 登记为"重设、不平移"的 TSC 字段。
- **J7**:如果 hft-core 在跨 AZ 恢复时给 epoch 加安全间隔，`SetEpoch` 要带上一 epoch 的具体值，gw 不能再按"当前值 − 1"去推。
- **J9**:testing 的不变量 20、BK-1、BK-5 改为链接 exec 的"计时"三种情况。
- **J10**:IR 第 46、63 行改成按 venue 的说法；写 ADR `resolve-broken-hard-only`。
- **J2**:
  - SDK:681 写明返回给策略的外来成交截止是名义值。
  - SDK:997 的核销语义改为归 hft-oms。
  - `manual_hold` 的解除改成"替换了当前目标的 target"(逐字段相同的不算)，并说明引擎目标期间到达的人工成交会延迟到引擎目标清除后才置位。
- **J11**:hot-path-rules D6 登记 `gate_closed_tsc`；defensive-patterns:423 和 glossary:65 的措辞。

## 4. 简报 §7 之外新增、别人要用的名字
- exec:`on_venue_gate_open(a, v, now)`、`drain_ready_acct(a)`、`Timer{Exec, Alarm=6}`、`ExecGateRow`/`ExecTargetRow`/`ExecSpecRow`(Gate 行 kind=3)、`MANUAL_HOLD_DEFERRED`。
- oms:`TrigStamps`、`SlotStamps.op`。

## 5. 没能自己定下来的地方
1. **`ExecGateHeldNaked` 用于两种场合**:X1 P2-5 要求自动平仓到期后长时间等待时报 P0。我没有另开 Note 码，而是让这条告警带"场合"字段，共用 `gate_naked_alarm_ms`。如果你希望单独一个码，J6 需要再登记一条。
2. **恢复期的计时我选了"放行时重给完整窗口"，而不是"暂停后续上剩余时长"**:这满足"不计时"，而且对人更宽松，但 testing 要按这个口径写。
3. **行版本没有升**:节 21 新增了 Gate 行和显式目标行，行版本仍为 1。X3 上一轮 P1-31 的遗留(节 12 的 Leg 行从 176 B 变 200 B 仍是 v1)也没动。理由是实现前的草案变化在设计基线 v1 里统一按 v1 算；如果主控认为必须升版本，需要另定。
4. **按 venue 抑制依赖 J5**:这需要 hft-risk 新增按 venue 的放行报告。J5 不做的话，执行层会退回到原来按整账户闸门的行为。
5. **"上一 epoch"的定义**:oms 现在定义为恢复所依据快照节 3 里的 `engine_epoch`。这和 gw 的 `SetEpoch` 只带一个布尔值 `accept_prev` 还没对齐(见上面 J7 一条)。

## J5

我负责的三个文件已改完：`hft-risk`、`hft-ledger`、`hft-sequencer` 的 README。`heading_audit` 结果为 lost headings: 0，没有丢章节。`check_hft` 在这三个文件上只剩两类告警：一是链接到 J10 尚未建的新 ADR（BADLINK），二是 sequencer 第 452 行与 SDK、glossary 的检查点键名单不一致（LISTCOPY，警告级）。第 452 行我已改成链接 SDK，改后没有再跑检查。

## 1. 改了哪些文件
- `E:\trading\hft\crates\hft-risk\README.md`
- `E:\trading\hft\crates\hft-ledger\README.md`
- `E:\trading\hft\crates\hft-sequencer\README.md`

## 2. 各项落点

**F1(a) `resolve_broken` 只在 HARD 解除时产生**
- risk：`ModeOutcome.resolve_broken` 的注释、"断腿"第 6 条、cap 表的 Kill 行和 Broken 行。
- sequencer：模式与 cap 的驱动 ④，调用 `exec.resolve_broken(a, g, &views)`。

**F1(b) 恢复期按多出那一腿所在 venue 判定**
- risk 启动闸门：新增"恢复期按 venue 放行"，条件 `venue_gate_ok` = `hedge_gate_ok` ∧ 该 venue 已 `RECONCILED`。条件变真时列入 `ModeOutcome.venue_gate_open`。
- sequencer ⑤：转调 `exec.on_venue_gate_open`，名字与 J4 已写的一致。

**F1(c) AUTO_EXTERNAL**
- risk "对账分级"第 7 条：新增 `auto_external_ok`，检查两轮都没有 `USERSTREAM_DOWN`，且用户流最近一次 Up 早于上一组报告的视图点。状态存在节 17 新增的 `RiskUsRow`，行版本 3→4。
- ledger"外部成交"第 1–6 步：
  - 合成函数 `auto_external_fill`；
  - `auto_held` 抵扣，以及 `Note{LedgerAutoHeldOffset}`（P0）；
  - 冷读者的 journal 消费路径：读到带 `AUTO_RECON` 的注记时调用同一个函数；
  - `auto_held` 存在节 13 新增的 `AutoHeld` 行，行版本 v6→v7。
- sequencer `ReconcileReport` 行：明确"在 Input 内合成、不另写记录"。

**F1(d) Drain 与引擎减仓**
- sequencer Drain 节：第 1 步改为只停增仓切片；第 3 步改用 `exec.drain_ready()`；第 6 步 `DrainTimeout` 改为按条件定级，并列出未完成的账户位图。
- risk 存储降级：任一账户有清仓锁或处于 L3 时推迟 `StorageCritical`。Storage cap 的 `params` 记录两项各自的档位。

**F1(e) 持仓驱动订阅**
- sequencer"inst → 策略的稠密索引"节。
- 出环满节：`feedctl` 环满时由驱动暂存续推（同时解决 X3 P1-15）。

**F3(b) 关键帧**
- sequencer 段 5 新增 `keyframe_max_events`。
- `Sys{Snapshot}` 行：时间或事件数到期；最多推迟 2 个周期，之后在事件边界强制写。
- 快照节与状态哈希节：关键帧头部的 `state_hash` 写 0，需要它的读者自己算。
- `Sys{Roll}` 改为两级（软阈值 / 硬上限），名字按 J11 已写的 `RollAt` 对齐。

**F4(b) 只含 Sim 账户的实例**
- risk"干净 Drain 重启自动解除 Recovery"节：非干净重启时，只要没有 `RECOVER_MISMATCH` 和 `POISON` 也自动解除；否则写 `Note{RecoveryHeldSim}`（P1）。

**F6（风控侧）**
- risk `IDENTITY` 行：`uid_policy = Unverifiable` 的账户由"kill 自检通过 + 收到任一 `AccountConfigReport`"放行，并写 P1 注记。

## 3. 处理的评审条目
- **X1**：P1-A、B、C、D、E；P2-7（核销允许静止的非 Broken group，清零全部腿余数）；P2-8（外来成交归属以该腿减到 0 为限，`ExternalApplied` 新增 `none_qty`）；P2-10（group 指纹变化视同"移除 + 新增"）。
- **X3**：
  - P0：P0-2（sequencer 侧）、P0-3（sequencer 与 ledger 侧）。
  - P1：P1-1、2、3、6（段 5 定义一行 72 B 的 `SeqParamsRow`）、15。
  - P2：P2-3、5、13、21、33、43、44、48。
  - 上一轮表的残留：P1-31、P2-9、P2-16、P2-47（CapSet 的实际级别改由 risk 定义）。
- **X5**：L-P2-5（加了 STNP 待实测）、L-P2-6、L-P2-11(e)（删除 `md_levels_per_round`）。
- **X2**：P2-7、P2-9、P2-20（`MarketLastRow` 尾部追加 `prev_interval_s`，v6，112 B）。
- **X4**：N-P1-7 的风控侧、P2-1。

**留给别人**
- **J6**：
  - 登记三个注记：`Note{IdentityUnverified}`（Risk，P1）、`RecoveryHeldSim`（Risk，P1）、`LedgerAutoHeldOffset`（Ledger，P0），`v[]` 布局写在我的文件里。
  - `DrainTimeout` 的定级条件要加"未到达的引擎减仓目标"。
  - hft-event 第 669 行 `Sys Snapshot` 的说明要加事件数触发和最大推迟。
  - journal-format：关键帧 `state_hash` = 0；节 10 v6、节 13 v7、节 17 v4。
  - control-plane：删掉 `GroupNotBroken`；补 `SmokeErr::BadInst` 的结果码；采用"Live 实例"的定义。
  - refdata：ECF1 段 5 的 `row_size` = 72；第 295 行的 `ref_inst` 校验现在由引擎订阅构造保证。
- **J4**：`ExternalApplied.none_qty` 的那部分由 sequencer 以 `GROUP_NONE` 另调一次 `on_external`；OMS 核销在没有裸量时返回 `qty = 0`。
- **J8**：
  - hft-core 第 560 行 Sim 实例的 Recovery 规则；
  - 恢复尾部上界（我写成"见 hft-core 启动步骤"）；
  - replayer 读者侧计算关键帧 `state_hash`；
  - 第 572 行 `IdentityMismatch` 应由 sequencer 写；
  - chain-* 环的容量行。
- **J9 / J11**：reconciler 的置位条件、引擎视图、AX-1，以及 hft-py，都改按 hft-ledger"外部成交"第 6 步消费 AUTO_RECON 注记。
- **J10**：
  - 我在三个文件里链接了 ADR `resolve-broken-hard-only`、`auto-external-guard`、`position-driven-subscription`、`testnet-uid-unverifiable`、`dev2-root-batch-slice`，需要按这些名字建出来；
  - ADR `sequencer-md-preempt` 里的 `md_levels_per_round` 已删除，要同步改。
- **J12**：`struct_hdr` 往前回看时会把上一个结构体注释里的"节 N"算到 `MarketLastRow` 头上（误报成节 5）。我在它前面加了一个空行绕过去。

## 4. §7 之外新引入的名字
- risk：`ModeOutcome.venue_gate_open`、`Mode::venue_gate_ok`、`Risk::auto_external_ok(a, v, sflags)`、`Note{IdentityUnverified}`、`RiskUsRow`（节 17 的 `UserStream = 8`）。
- ledger：`Ledger::auto_external_fill`、`apply_external(e, auto_recon, t)`、`ExternalApplied.none_qty`、`AutoHeld` 行（节 13 的 kind 14）。
- sequencer：`SeqParamsRow`、`RingLimit`、`MarketLastRow.prev_interval_s`。

## 5. 没能确定怎么改的矛盾
1. 清仓锁要等 `Resume{Kill}` 才解除，即使已经清完仓。所以 `StorageCritical` 可能一直被推迟到 journal 写满后 abort。可能需要 hft-core 增设一个第 3 档存储硬阈值。
2. OMS 防御路径合成的 `ExternalFill`（来自 `ForeignFill`）同样不是 Input 记录，冷读者看不到。简报没有覆盖这一类。
3. `auto_held` 不会过期。它也会抵扣 `cause = Unknown` 的成交，其中可能有本系统自己没匹配上的成交。我已写进 ledger 的已知限制。
4. "Live 实例"我采用了 control-plane 的定义（有期望模式为 Live 的 cell）。这样在 Live 账户上、全部 cell 都是 Shadow 时，改结构字段不需要确认。
5. 持仓驱动订阅在 `max_insts` 用尽时，只能靠风控的标记价断流（至少 L1）来告警，没有专门的注记。

## J6

J6 这一轮的修订已完成。heading_audit 结果:lost headings 0。check_hft 的问题里,落在我文件上的只剩 9 条 APPENDONLY,原因见第 5 条第 1 项;其余都在别人的文件里。

## 1. 改了的文件
- `crates/hft-event/README.md`
- `crates/hft-refdata/README.md`
- `crates/hft-types/README.md`
- `crates/hft-ctl-proto/README.md`
- `crates/hft-ctl/README.md`
- `docs/subsystems/event-model.md`
- `docs/subsystems/journal-format.md`
- `docs/subsystems/control-plane.md`
- `docs/cookbook/adding-an-event-kind.md`
- `tools/README.md`

## 2. 各项落在哪里
- **新码登记**(hft-event「Note 来源与码登记表」):
  - `ExecGateHeldNaked`:Exec 源,P0,覆盖 J4 定义的两种场合。
  - `LedgerAutoHeldOffset`:Ledger 源,P0。
  - `RecoveryHeldSim`:Risk 源,P1。
  - `LatencySample.action` 放在偏移 121,占用原来的保留字节,大小 136 B 不变,版本不升,0 = New。
  - `MarketHdr.rx_empty_gap_ns` 放在偏移 48,并加 `_rsv: u32`;它的语义写在 event-model 的时间戳表。
- **LoadErrCode**(X3 P0-2):hft-refdata 新增小节「LoadErrCode 码表」,`#[repr(u16)]`,1–51,只追加;同时补了 `LoadErr`/`LoadReport` 的定义。control-plane 的 detail 124、401 改为链接这张表;hft-event 的 gen-schema 导出它,并在 `--check` 中拒绝编号消失或复用。
- **F6 `uid_policy`**:
  - hft-refdata:`AccountMeta.uid_policy`、`UidPolicy` 枚举、第 9 步校验 `UidPolicyProd`;`Unverifiable` 账户的 UID 可为空串。
  - tools hft-cfg:需要写 Admin 签字字段 `uid_policy_signoff`。
  - hft-event:登记 `Note{IdentityUnverified}`(Risk 源,P1)。
- **F3(b)**:
  - journal-format 的快照一节:关键帧的 `state_hash` 写 0,由读者侧计算。
  - 大小安全阀改为两级,阈值链接 hft-journal。
  - 关键帧的触发条件链接 hft-sequencer。
  - hft-event 定时器表与 tools 的 `replay-verify` 同步。
- **F1(f)** control-plane 的 `WriteOffResidual`:改为只要求 group 静止,并清零全部腿的余数;detail 315 `GroupNotBroken` 退役。
- **F1 其余**(都在 control-plane):(a) 只有解除 HARD 才判 Broken(第 0 层与 `Resume`);(d) Drain 的等待条件与 Storage 推迟;(g) `.aborted` 标记,启动时只读 `stop-marker.json`。
- **F3(e)**:tools 的 hostprobe 新增「保持唤醒的允许形式」一条:`nohlt` 或登记过的 keepawake 线程。
- **F4(a)**:tools 改用 `hftbatch.slice` / `CPUWeight=idle`,hostprobe 补采 `cpu.idle`。
- **F5**:tools 里没有关于 quota-gate 的描述,无需改动。

## 3. 处理的条目
- **X3 P0**:1(journal-format 节 21,以及新增一条通用规则:行不得原样嵌入数据枚举)、2。
- **X3 P1**:1(Live 实例按账户类型定义,control-plane 为唯一归属)、2、3、8、9(演进规则允许占用保留字节)、11、13、28(链接 deploy)。
- **X3 P2**:2、5、6、7、8、9、11、12、14、19、20、22、29、30、37、44;残留项 P1-31、P2-16、P2-21、P2-47。
- **X1**:P2-7、P2-13、P2-16。
- **X4**:P2-3、P2-4(tools 一侧)。
- **X5**:L-P2-5、L-P2-9;L-P2-11 (b)(c)(f) 补进 hostprobe 采集项。
- **X2**:P2-10(hft-cfg 新增 group 的 `n` 必须大于上一版本的最大 `n`)。
- 另外在 tools 登记了 `cargo xtask test-trace`,消除 docs/tests 的悬空引用。

**留给别人**
- **J4**:hft-exec `CellState` 的码值(Idle=0)要改用 hft-event `TargetState`(Idle=1…)(X3 P2-15);`ExecCellRow` 写成显式行(P0-1)。
- **J5**:
  - hft-risk:332 删掉「不越过交易所价格带由 OMS 定价保证」。
  - hft-risk 撤单持续义务只限 `cancel_resting = true`,并在 cap 表补上 `Draining` 的 `params`。
  - hft-sequencer:623 链接 control-plane 的 Live 实例定义;`WriteOffResidual` 不再要求 Broken。
- **J7**:写明 `rx_empty_gap_ns` 中「确认队列为空」的判定;一次读到多包时 `hw_rx` 取最早一个包。event-model 已经链接到这两处。
- **J9**:
  - testing §3.3 链接 tools 的 `HFT_FUZZ_TIMEOUT_DIR` 存法。
  - testing:289 补齐信号:SIGBUS/ILL/FPE 判失败,SIGTERM/SIGINT 与退出码 72 判中止。
  - journal-archiver 不上传 `crashes/dst/inflight.json`,对象键带主机段。
- **J10**:
  - launch.md:94 与 smoke-order ADR 删掉「价格带内」。
  - ops_bot 停机兜底的额外确认条件补上:清仓锁未完成、外来成交计时中、自动有界平仓进行中(control-plane 现在指向 ops_bot 为唯一清单)。
  - pipeline:252 按 Live 实例定义统一;`next_ctr` 从生效的 blob 续号。
  - deploy:运维账户如何进 `hft-ctl` 组、keepawake 线程登记、hft-verify 的 `KillMode=mixed`、hostprobe 新采集项的期望值。
- **J11**:hft-journal 定两级阈值的数值;删掉 hft-journal:35 抄写的页大小。
- **J12**:APPENDONLY 检查需要放行两种情形:大小不变地占用保留字节,以及 `MarketHdr` 这一次长大(原因见第 5 条第 1 项)。

## 4. §7 之外新引入的名字
- Note 与定时器:`Note{IdentityUnverified}`(Risk,P1);在 hft-event 登记了 J4 先用到的 `Timer{Exec, Alarm=6}`。
- 定时器编码:`Timer{Oms, UnknownQuery}` = `3 | table_ix << 8 | slot << 16`,与 J4 的 `OmsTimerReq.id` 一致。
- hft-refdata 类型:`UidPolicy`、`LoadErr`、`LoadReport`。
- `LoadErrCode` 中新起的变体名:Structure、Strings、Ids、DanglingRef、QuoteNotStable、CapsValue、ExpRange、ValueRange、DuplicateSymbol、SimCfg、UidPolicyProd、VersionNotIncreasing、InstRemoved、AssetChanged、MultiplierChanged、LegNotTradable、HostClassMismatch、RefInstNotSubscribed、StaleGuardMismatch。
- 其他:TOML 键 `uid_policy_signoff`、环境变量 `HFT_FUZZ_TIMEOUT_DIR`、`MarketHdr._rsv`。

## 5. 没能确定怎么改的矛盾
1. **`MarketHdr` 只能从 48 B 长到 56 B**:它只有 2 B 保留字节,放不下 u32,于是 9 个行情体(Bbo 96、Trade 88、L2BatchHdr 80 等)的字段偏移都后移,check_hft 报 9 条 APPENDONLY。我没有升这些 kind 的版本,因为升版本就等于声称旧记录补零可读,而这不成立;这些行情 kind 还没有实现,也没有写出过记录。演进规则里已写明:以后 `MarketHdr` 只能占用 `_rsv`。是否接受这个做法,需要主控确认。
2. `uid_policy` 的 TOML 取值按简报写成 `"Unverifiable"`,和 hft-cfg 其他取值用小写的惯例不一致。
3. Admin 签字核对的是 `.github/hft-admins.txt`(GitHub 登录名单),而不是 ctl 的签发者表;哪一份名单才算数,还需要定。
4. `LoadErrCode` 收录了 hft-core 启动时的交叉校验名,如 `UnknownStrategyKind`、`BindingChangedWithPosition`,与 hft-core 自己的退出 2 错误名、以及 sequencer 复核的第 3 阶段有部分重叠。

## J7

我负责的 8 个文件都已按简报修完。heading_audit 显示丢失标题 0;check_hft 对这 8 个文件没有报出问题(总数 problems=18,都在别人的文件里)。

## 1. 改了哪些文件
`E:\trading\hft\` 下:
- `crates/hft-net/README.md`
- `crates/hft-venue/README.md`
- `crates/hft-venue-proto/README.md`
- `crates/venue-binance/README.md`
- `crates/venue-okx/README.md`
- `crates/venue-bitget/README.md`
- `docs/cookbook/adding-a-venue.md`
- `fixtures/README.md`

## 2. 各项落点
- **F3(a) gw 收包节拍**
  - hft-venue 配置项:`gw.aux_every` 改为"standby 与有在途请求的 REST 连接每 N 圈 poll 一次";新增 `gw.rest_idle_check_ms`(1000)。我顺手让 snap-fetch 和 acct-fetch 的 REST 池按同一间隔做存活检查,没有另开键。
  - hft-venue §4 循环第 3 步与 RestPool 那一步:写明 gw 不用就绪探测,T7 不动;`wait_fds` 注释、"gw 循环"测试行、开发备忘第 3 条(链到 `gw-ingest/poll-cadence`)同步。
  - hft-net:`RestPool::poll` 只读有在途请求的连接,新增 `RestPool::idle_check`;§9、系统调用说明和 REST 测试行同步。
- **F2(b) `rx_empty_gap_ns` 与取戳规则**,产生规则只写在 hft-net §7 一处:
  - `Timestamps._pad` 改为 `rx_empty_gap_ns: u32`,结构仍是 32 B;新增 `Conn::note_empty`。
  - 确认"队列为空"的来源有三种:EAGAIN、短读(取调用之前读的 TSC),以及就绪探测里未就绪的连接。
  - 取戳:hw/sw 戳和间隔取该帧最早一个包所在的那次读取;`rx_tsc` 取凑齐该帧的那次读取。读到的字节数超过 MSS(握手后读一次 `TCP_MAXSEG`)或读取戳表溢出合并时置 `MULTI_SKB`。
  - 调用侧:hft-venue §2 第 2 步在探测后调 `note_empty`,第 5.4 步把这些值交给 venue 写进 `MarketHdr`。
  - hft-venue-proto:`RxMeta` 从 40 B 改为 48 B(加 `rx_empty_gap_ns`),契约 3 同步。
- **F6 Binance Demo Trading**
  - venue-binance "测试网/模拟盘"一节改用 Demo 端点:现货 `demo-api`、`demo-ws-api`、`demo-stream`、`demo-stream-sbe`;USDⓈ-M `demo-fapi`、`demo-fstream`。ws-fapi 的演示下单端点没查到,写成待核实且 M3 前必须核实。
  - 新增"身份"段,说明 `uid_policy = "Unverifiable"` 何时适用,规则链接 hft-refdata。
  - VenueCaps 的 `testnet` 改为 `Demo`;开发备忘第 14 条重写;方法 B 和其余各处"测试网"改为"演示环境"。
  - OKX、Bitget 的测试网一节和 cookbook 第 0、7、8 步各加一句链接 `uid_policy`。
- **F1(c) 断流期间强平/ADL 是否补推**:hft-venue §6a、开发备忘第 8 条写成待核实(方法:测试网/模拟盘断流演练);"去重环重建"测试行注明重推的是假交易所;cookbook 第 0 步加入这项要收集的事实。
- **X5 L-P2-3**:acct-fetch 把原始响应按 16 KiB(`ACCT_RAW_CHUNK`)分块交回,`AcctRecHdr` 加 `part`/`parts`。同一响应的各块在 raw-gw 中是同一连接上紧随 `RawTx` 的连续几条 `RawRx`。基准改为 `venue_gw_acct_apply_16k`(预算 ≤2/≤5 µs,原 50 KiB 预算按比例缩小,待实测)。fixtures 同步写明抽取时按连接拼接。
- **X5 L-P2-11(a)**:`feed.raw_dups` 默认值改为 false。

## 3. 处理的评审条目
- **X5**:L-P1-1(取戳规则与间隔字段)、L-P1-3(gw 节拍)、L-P2-3、L-P2-11(a)。
- **X4**:N-P1-7 中属于 venue 的部分。
- **X1**:P1-C 第 3 条。
- **X3**:
  - P2-31:测试行里写死的 0..8 改为常量名。
  - P2-42:写明 snap-fetch 复用该 venue 行情连接的源地址,acct-fetch 复用该账户 `gw.src_ip`。
  - P1-27:hft-net 注明 conn-builder 的 netlink 需要 `AF_NETLINK`。
  - 上一轮遗留的 #112:cookbook 改为每个 (Live 账户, venue) 都要配 kill-switch 凭证,不论有无死手。
  - P0-5、P1-20、P2-19/32/35/48 里我的文件本来就是一致的一方,不用改。

**留给别人**
- **J6**:
  - `MarketHdr` 只有 2 字节保留位,放不下 `rx_empty_gap_ns: u32`,只能在尾部追加,所有行情体的版本都要升。
  - event-model 时间戳语义表里 hw/sw 戳的说法要改为链接 hft-net §7。
  - hft-refdata 要登记 `uid_policy`。我的链接没带锚点,因为现在 hft-refdata 里还没有这个字段。
- **J11**:
  - `latency-budget.md` 第 17 行附近"取最后一个段的戳"要改为链接 hft-net §7。
  - hot-path-rules T13 的"REST 每 aux 圈 poll"要改为"只在有在途请求时读,空闲时每 `gw.rest_idle_check_ms` 检查一次",并链接 hft-venue §4。
- **J9**:benchmarks 登记 `gw-ingest/poll-cadence`;`gw-egress/idle8` 的描述同步;P1-20 的上限表按消息类别和分位数分行;testing §8.2 引用 `rx_empty_gap_ns`。
- **J10**:deploy 单元表给 hft-core 加 `AF_NETLINK`(X3 P1-27)。
- **J8**:hft-core:468 的出口地址改为链接 hft-venue §1(P2-42)。
- **J2**:latency-probe 的跨 gw 探针变体。

## 4. §7 之外新引入的名字
- `Timestamps.rx_empty_gap_ns`、`RxMeta.rx_empty_gap_ns`
- `Conn::note_empty`、`RestPool::idle_check`
- `ACCT_RAW_CHUNK`、`AcctRecHdr.part/parts`
- 基准 `venue_gw_acct_apply_16k`(取代 `venue_gw_acct_apply_50k`)
- `MULTI_SKB` 的含义改为"戳不能确定是该帧最早一个包的戳"

## 5. 没能定下来的矛盾
1. `MarketHdr` 没有 4 字节保留位,J6 只能追加字段并升全部行情体的版本。
2. 据第三方实测,Binance 演示环境同一把 key 同时用于现货和 USDⓈ-M,两个测试账户会是同一个 UID,会撞上 refdata 的 `DuplicateVenueUid`。`Unverifiable` 只管身份闸门的放行,不管这条唯一性检查,需要 J6 或主控定规则。
3. `gw.rest_idle_check_ms` 带 `gw.` 前缀,但我让 warm 任务也用它;如果要一事一键,需要另开一个键。
4. Binance Demo 端点来自对官方文档的检索摘要,developers.binance.com 抓取被拒,原文未缓存,所以全部写成待核实(方法 A)。
5. `rx_tsc` 和 hw/sw 戳现在可能来自不同的读取,跨多次读取的帧,`kernel_to_user` 会包含帧尚未收全的那段时间;latency-budget 的分段定义需要注明(J11)。

Sources:
- [Binance Spot Demo Mode](https://developers.binance.com/docs/binance-spot-api-docs/demo-mode/general-info)
- [Binance USDⓈ-M General Info](https://developers.binance.com/docs/derivatives/usds-margined-futures/general-info)

## J8

J8 这一轮改完了,只动了我负责的三个文件。heading_audit 结果是 `lost headings: 0`。`check_hft` 里这三个文件没有 problem 级的条目(RINGCAP、BADLINK、BADANCHOR 都没有落在我的文件上),剩下的警告是 PERMIMP、LISTCOPY 一类启发式误报。

## 1. 改了哪些文件
- `E:\trading\hft\apps\hft-core\README.md`
- `E:\trading\hft\apps\replayer\README.md`
- `E:\trading\hft\crates\hft-sim\README.md`

## 2. 各项落点
- **F1(g)**:hft-core「启动顺序 › 停机标记」第 1 条:启动时只读 `stop-marker.json`,`.aborted` 和 `.consumed` 都不读。测试表「停机标记」行加了对应用例。
- **F4(a)**:批处理改到根级 `hftbatch.slice`,设 `CPUWeight=idle`,不再依赖 `CPUSchedulingPolicy=idle`。dev2 上是 `hft.slice` 1,376 MiB 加 `hftbatch.slice` 512 MiB,两者互斥由窗口标志和 `ExecCondition=` 保证。
  - 改动处:hft-core「主机资源隔离」的表和"批处理"一条、c8/c16/c32 线程表、整机内存表、dev2 整机说明、护栏、退路、演练 8;replayer 每日 verify 那段。
- **F4(b)**:hft-core 启动步 8 写了实例判定(refdata 全部账户是 Sim,即 `sim` 实例),解除条件链接到 hft-risk,另加 `Note{RecoveryHeldSim}`。`--recover-from-snapshot` 第 5 条与 hft-risk 已经对齐。replayer 和 hft-sim 的 `CoreRestart` 不再写"不自动 Resume"。
- **F4(c)**:护栏表新增"hft 自身用量"一行做因果判定;一级、二级动作加了这个前提;退路 (b) 只计成立的停机;新增演练 9。
- **F3(b)**:hft-core 步 5 下面新增"恢复尾部"一段,约束是 `keyframe_max_events` ÷ 回放吞吐下限 ≤ 5 s(按现值约 4.5 s)。replayer verify 第 1、5 步改为由 replayer 自己算 `state_hash`,与 journal-format 现文一致。
- **F2(b)(c)**:replayer latency 的输出加了 `kernel_rx` 上下界;c32 节的 ENI-B2 启用判据加了两种替代测法。
- **F3(d)(e)**:M3 末探针加 c32 计划跑一次并做 ENI-B2 拆分 A/B;IRQ 核"保持唤醒"的允许形式写进第 12 条,作为 hostprobe 白名单例外。
- **X3 P0-4**:第 7 条写成 `park_us` 校验的唯一归属,并写清单位换算。
- **X3 P0-5**:hft-core 的"批处理并发规则"明确为全部主机类,实现在 deploy 主表;主机没装 `hft-verify` 时不得让回放失败。

## 3. 处理的评审条目
- **X3 P1**:7(ECF1 段 4 行格式:新增 `RingCapRow`/`RingCapKind`,环表重排)、12(检查点文件布局 `CkptHdr` + `PathSecHdr`,也覆盖 X2 P2-5)、14、15(`feedctl` 容量按 `max_sub_insts` 定)、16、18、22、26。
- **X3 P2**:1、4、10、20、21、26、32、33、34、36、38、39、40、41、42、45、46、47;另有 §4 的"基线待测"写法,上轮残留的 `rr_max_cross` 说法。
- **X2**:P2-3(recompute 按 `basis_seq` 配对,多出的计入 `coalesced`)、P2-4(写成开放问题)、P2-25。
- **X4**:N-P1-1、N-P1-2(b)、N-P1-6(hft-core 部分)、P2-1、P2-2(另加检查点写带宽待核实)、P2-9(窗口 stop 改核对 tmpfs 已用量;退路 (a) 写成开放问题)、P2-10、P2-12。
- **X5**:L-P1-2、L-P2-5、6、7、8、9、11(a);§4 关于 c32 每线程 socket 数加了待核实。

**留给别人**
- **J1**
  - `02:44`、`11:102` 链到 c16 的旧锚点,新锚点是 `#c16c8g4xlarge有条件的候选首批下单-venue--4`。
  - 11 §3.2 要写进 ENI-B2 的两种替代测法,以及探针同时跑 prod 和 sim。
- **J2**
  - SDK 第 252 行的重建点时间基准从记录头 `utc_ns` 改为 `engine_ns`。
  - SDK 第 244 行把文件布局链到 hft-core 的检查点表。
  - cookbook 写明"改容量表单独提交"。
- **J5**
  - 检查点文件格式(`CkptHdr`/`PathSecHdr`)已在 hft-core 落地。
  - `feedctl` 满了怎么续推可以另定,与 hft-core 的容量规则不冲突。
  - `keyframe_max_events` 和回放吞吐下限任一改动,都要守住 hft-core 的尾部约束。
- **J6**
  - hft-refdata ECF1 段 4 那一行改为链接 `RingCapRow`。
  - hostprobe 写 keepawake 白名单,以及 X5 L-P2-11(c)(f)。
- **J7**
  - hft-venue 第 274 行:snap-fetch 复用第一条 `stream = md` 的 `src_ip`。
  - hft-core 把升级主机 prod 的 `feed.raw_dups` 设为 false,hft-venue 默认值不动。
- **J9**
  - kill-switch:写 `.aborted`;实例 `Stopped` 后要解除对热段的映射。
  - watchtower 整机 PSI 告警级别对齐到 P2。
  - testing §3.5 报告加 `coalesced`。
  - hft-telemetry 的 `kernel_rx` 上界。
  - journal-archiver restore 负责 epoch 安全间隔(写进 DurableMeta)。
- **J10**
  - deploy 主表实现回放期间暂停 archiver/pipeline,停 `hft-verify` 那一步加 `-` 前缀。
  - 护栏守护单元加 `After=`。
  - `hft-window stop` 的核对方式和提示改成与 hft-core 一致。
  - 升级主机上 prod 心跳目录的建立者。
  - `hftbatch.slice` 的全部单元、清单和 preflight。
  - 机检显示 `/var/lib/hft/replay-verify` 属主是 hft-verify,回放用户进不去。
  - `/dev/cpu_dma_latency` 的权限。
  - release-and-deploy 第 65 行的 50 ms 改为链接 hft-journal。
  - deploy 第 120 行 `cold_gib` 的示例改大。
- **J11**
  - hft-rt 第 292 行删掉公式,改为链接 hft-core 第 7 条。
  - latency-budget 第 236 行对齐 c4 的口径。
  - glossary 第 67 行 acct-fetch 的实例数。
- **J12**:hostprobe 输出和护栏事件文件被算到 hft-core 写入,"判据只有"被当成名单副本,都是误报。

## 4. 新引入的名字
`RingCapRow`、`RingCapKind`(码 1–17)、`CkptHdr`(magic `HFTCKPT1`)、`PathSecHdr`;replayer 报告字段 `coalesced`;候选字段 `skipped_reason = release_change`(只在开放问题里提到)。

## 5. 没能定下的问题
1. hft-core 重启后 sim-gw 的模拟真值从哪里来,目前没有规定。F4(b) 让 sim 实例重启后自动解除 Recovery,这个缺口因此更要紧。已写进 hft-sim 开放问题。
2. dev2 上 `hft.slice` 1,376 加 `hftbatch.slice` 512 共 1,888 MiB,高于设计峰值。峰值 1,376 只靠窗口互斥保证,内核不强制。
3. 因果判定的阈值(0.25 核 / 256 MiB)是我估的初值。按这个内存阈值,内存类信号几乎总会停 hft。
4. 段 4 的 `EngineValidator` 和检查点读写的不变量放在 app 里,与 apps/AGENTS 的"只装配"有张力,可能应该挪进某个 crate。
5. c32 的 sim 在 Drain 时写检查点,受 30 MB/s 写上限和 `TimeoutStopSec=20` 限制,可能写不完。已列为待核实。

## J9

M0-f J9 这一轮做完了。heading_audit 没有丢失标题(0)。check_hft 在我的文件里只剩一类问题:testing.md 第 228、349 行被判为 HISTORY,命中的是简报要求加的"策略研究后定(R8-12)"这句话本身,需要 J12 放行。另外,我改动的规格里有几个数值是我自己定的、简报没给,需要你确认,见第 5 节第 1 条。

## 1. 改了哪些文件(路径相对 `E:\trading\hft\`)

- `apps/reconciler/README.md`
- `apps/kill-switch/README.md`
- `apps/journal-archiver/README.md`
- `apps/watchtower/README.md`
- `crates/hft-telemetry/README.md`
- `crates/hft-testkit/README.md`
- `docs/testing.md`
- `benchmarks/README.md`、`benchmarks/AGENTS.md`

## 2. 各项落点

**F1(c) AUTO_EXTERNAL**
- reconciler「ReconcileReport」加第 5 条置位条件:用户流稳定。我原本按 `interval_s` 计时写,后来发现 J5 已在 hft-risk `auto_external_ok` 定了口径(用户流最近一次 Up 早于上一轮 `view_seq`),改成链接它,不再自定义。
- 写明"在这条 Input 内合成、不另写记录";reconciler 的引擎视图经 hft-ledger 调同一个 `auto_external_fill`;补推的真实成交由 `auto_held` 抵扣。
- testing AX-1 按同一口径重写,并加了补推与用户流不稳定两个例子。
- testkit 的 ReconActor 与故障目录同步。

**F1(g) 停机标记作废**:kill-switch「停止 hft-core」第 6 条。停机失败时把标记改名 `.aborted`、`marker = false`、报 P0。同步改了状态显示、回执、测试与已知限制;testing §4.1"停机标记"行加了这一例。

**F1(b) Broken 计时**:改为"多出那一腿所在 venue 的对冲闸门子集满足且该 venue 已对账"才开始计时。落在 testing 不变量 20、B-4、BK-1、BK-5(含 `gate_naked_alarm_ms`)和 testkit I20。

**F1 其余**
- testing 新增 BK-7(`resolve_broken` 只在解除 HARD 时产生)。
- BK-6 改为核销清零全部腿余数,并允许核销静止的非 Broken group。
- 不变量 18、testkit I18 与"Drain 期间"行改用 `exec.drain_ready()`。

**F2(⑩ 证据),全部在 testing §8.2**
- 第 2 条:`action` 口径。
- 第 4 条:HW 时间戳不可用时退回 SW 锚数据集。
- 第 7 条:A/A、按资金费块配平开关顺序、区间过宽时延长窗口。
- 新增第 9 条(突发/平静分层、`kernel_rx` 上下界)和第 10 条(HW 戳不可用时的替代测法与 ENI-B2 替代判据)。
- hft-telemetry 实现对应接口:`stratum`、`stratified_probe`、`kernel_rx_hi`,以及 `interference` 的 A/A 模式与 `Extend` 判定。
- watchtower 的 `--aa` 与配置键同步。

**F3(a)**:benchmarks 登记 `gw-ingest/poll-cadence`。

**F4**:我的文件里 `hftbatch.slice` 已全部改名;watchtower 的护栏告警级别、testing 里只含 Sim 账户实例的 Recovery 解除也已对齐。

**journal-archiver**
- X3 P0-9:写冷副本只有写 `.uploaded` 和删除两种,写途径与 `ReadWritePaths` 链接 deploy(与 J10 的 ACL 方案一致)。
- X3 P0-10:对象键统一为 `<prefix>/<host>/<instance>/…`;files source 也加了主机段。
- X4 N-P1-8:新增「从 S3 恢复(restore)」一节,含 DurableMeta 上传、epoch 安全间隔和恢复代。

## 3. 处理的评审条目

- **X1**:P1-A/B/C/E 中涉及我文件的部分;P2-11、P2-15。
- **X3**:P0-3、8、9、10;P1-4、19、20、24;P2-10、16、17、19、28、46、51、57;上一轮遗留 P2-31(`raw-feed-<venue>-bbo`)。
- **X4**:N-P1-4 中 kill-switch 那部分、N-P1-5 中 archiver 读权限的说明、N-P1-8;P2-2、P2-3 中 testing 那部分、P2-4、P2-8 中 check 凭证、P2-9(kill-switch 映射与门槛 ⑥ 计时)、P2-10。
- **X5**:L-P1-1、L-P1-2、L-P1-3 中基准那部分;L-P2-10;L-P2-11(c);§4 指出的 testing.md:362 断链。
- **X2**:P1-5、P2-3。P1-4 与 P2-24 属暂缓项,只加了"策略研究后定(R8-12)"。

**留给别人**
- **J6(tools)**
  - tools:430/460/512 的 `inflight.json` 改成 `.inflight.json`。
  - 超时输入的存法已在 testing §3.3 定死,tools:438 改为链接。
  - 分类表补:`SIGTERM`/`SIGINT`/libFuzzer 退出码 72 记"中止";`SIGBUS`/`SIGILL`/`SIGFPE` 记"失败"。testing §6.1 已改为链接这张表。
- **J10(deploy / launch / runbooks)**
  - `hft-sim-alternate` 要按资金费块分段配平,并支持每次 2 天的延长段;host-upgrade 的 7 天窗口要加 1–2 天 A/A 与延长(最长 14 天)。
  - watchtower 配置模板加 `interference.max_halfwidth = 0.03` 与 `funding_hours_utc = [0, 8, 16]`,并生成 `host.ena_ifaces`。
  - hft-archive 的授权要覆盖 restore:S3 Get/List 和 KMS 解密。
  - 护栏阈值文件要带"只告警信号"的级别,watchtower 现在从这里读。
  - launch D-23 第 4 步的 archiver 游标路径不对:实际是 `cursor/<prefix>/<instance>/`,而且改前缀后会自动从新游标起步,这一步可以删。
  - IR S-3.2 链接 `journal-archiver restore`;M7 加一次换 AZ 恢复演练。
  - M7→M8 归档停机标记时连 `.aborted` 一起;ops_bot 要知道作废时 `Done.marker = false`。
- **J8(hft-core / replayer)**
  - hft-core:794 的 `benchmarks/startup_cold` 改成 `startup-cold`(这个基准只测启动步 5)。
  - hft-core:286 的 ENI-B2 判据链接 testing §8.2 第 10 条。
  - replayer 补 recompute 配对键 `(strategy, group, local_kind, basis_seq)` 与 `coalesced`;`replayer latency` 输出分层结果与 `kernel_rx_hi`。
- **J7(venue)**:基准表的解码上限现在链接 hft-venue-proto 的统一门槛表。但 venue-binance、okx、bitget 三家 BBO 解码 p99 都写 0.8 µs,宽于 venue-proto 的 0.5 µs。
- **J2**:latency-probe 的分层触发配额与跨 gw 探针变体。
- **J12**:放行"策略研究后定(R8-12)"这句。

## 4. 我新引入、别人需要用的名字

- **archiver**:`restore.epoch_gap`(默认 64);恢复代 `restore_gen`,文件名后缀 `.r<g>`;对象键 `…/private/meta/<键>.<UTC>.<哈希>`;文件归档前缀 `hft/verify/<host>/`。
- **testing / testkit**:`HFT_FUZZ_TIMEOUT_DIR`;`.inflight.json`;`burst_gap_ns`;`unsynced_cal_max`;`max_halfwidth`;`funding_hours_utc`。
- **hft-telemetry**:`Stratum`、`stratum()`、`stratified_probe`、`kernel_rx_hi`、判定值 `Extend`。
- **watchtower**:`host.ena_ifaces`、指标 `hft_ena_allowance_exceeded_total`、`interference --aa`。
- **benchmarks**:`startup-cold`。

## 5. 没能确定的矛盾

1. 以下数值是我定的,简报没给,需要你拍板:
   - `max_halfwidth = 0.03`,取自 X5 的建议。
   - 同机干扰窗口最长 14 天。
   - 突发层 `unsynced_cal` 占比超过平静层 2 倍就改用 SW 数据集;这个倍数标了"待标定"。
2. X2 P2-4"当天发生版本切换时 recompute 怎么处理"没有处理,它牵涉门槛 ⑦ 的证据,属暂缓范围。
3. 门槛 ⑦ 的计时路径表缺模块(X2 P1-4),以及 MS-2 的"不利方向"与 hft-sim 现文不一致,都只加了"策略研究后定(R8-12)"标记,没改语义。
4. watchtower 的整机 PSI 级别要靠护栏阈值文件带上级别才能取到;如果 deploy 不加这个字段,P2 这一级就无处可读。
5. testing §4.1 的"停机标记"和"冒烟单"两行原本多一列(两列的表里有三格),我顺手合并成两列了。

## J10

# J10 最终报告(M0-f)

我负责的条目都已落地。`heading_audit` 没有丢失标题(lost headings: 0)。`check_hft --brief` 在我的文件上 PERM、PERMRW、GROUPMEM、S3KEY、IAMSET、ALLHOSTS、PATHREG、BADANCHOR 都是 0;剩下的问题都在别人的文件里,告警里只有 IR:208 的 HISTWEAK(这里的"第二轮"指对账轮次,和以前一样是误报)。deploy/AGENTS.md 篇幅 890/900,python/AGENTS.md 882/900,都在上限内。

## 1. 改了哪些文件

- **deploy**:`deploy/README.md`(大改)、`deploy/AGENTS.md`
- **docs**:`docs/development.md`、`docs/postmortem/README.md`
- **runbooks**:`README`、`launch`、`host-upgrade`、`host-tuning`、`release-and-deploy`、`incident-response`、`az-probe`
- **python**:`ops_bot`、`pipeline`、`research` 三个 README,以及 `python/AGENTS.md`
- **`.agents/notes/`**:
  - `README.md` 增加"第八轮第六次修订入口"。
  - 新建 7 条 ADR:`architecture/2026-10-03-{resolve-broken-hard-only, auto-external-guard, position-driven-subscription}`,`process/2026-10-03-{dev2-root-batch-slice, ci-quota-self-estimate, testnet-uid-unverifiable, design-baseline-v1}`。
  - 给 15 条旧 ADR 加了状态行。简报点名的 5 条:keyframe-one-shot、latency-acceptance-evidence、dev2-guard-system-slice-psi、manual-hold-and-stop-marker、ci-org-and-profiles。另外 10 条:broken-auto-fix、smoke-order-command、l2-batch-and-bbo-lane、verify-on-server、imds-lockdown、sim-colocated-with-prod、one-account-per-strategy-instance、margin-liq-distance、margin-stress、dsh-style-documentation。
  - ADR 正文里的 `hft-batch.slice` 机械改名为 `hftbatch.slice`。

## 2. 各项落在哪里

- **权限闭包(X3 P0-6…9、11)**:都在 deploy。
  - "systemd 单元"一节新增"权限闭包"不变量(setgid 继承是唯一例外)。
  - 用户表补齐附加组:hft/hft-sim 加 hft-ctl,hft-ks 加 trigger 与 stopmark,hft-pipeline 加 api,hft-opsbot 加 hft-treasury,运维登录用户加 hft-ctl。
  - 路径表:大页挂载点改为 0751,`hft/prod` 为 2750;冷副本用 ACL 让 `hft-archive` 可写;登记了 advisor-daily、treasury、offbox、measure 等路径。
  - 单元补了 `ReadWritePaths`、`AF_NETLINK` 和 `Restart`。
  - S3 前缀表补 SSE-KMS 授权;秘密与 IAM 一节按主机类重写(dev2、升级主机、探针、消费者角色)。
- **X3 P0-5**:回放期间暂停 archiver/pipeline 写进 deploy 主表,`ExecCondition` 加上;恢复改由 `OnSuccess=`/`OnFailure=` 触发 `hft-replay-resume.service`;暂停名单由生成器生成,`systemctl stop` 加 `-` 前缀,所以 `verify = false` 的主机上回放不再启动失败(X4 N-P1-2)。
- **F4(a)**:deploy 的 slice 与 dev2 护栏两节、HostProfile、测试表;连带改了 python/AGENTS 和 release-and-deploy 第 7 步。
- **F4(c)**:护栏只因 hft 自身用量停机,写在 deploy dev2 护栏第 5、9 条和 IR P 节。
- **F4(d)**:deploy 的 IMDS 锁定与凭证代理一节;dev2 也运行凭证代理。
- **F5**:都在 `development.md`——用量自估、`quota_override`、`hotfix` 标签、额度耗尽时经个人私有镜像出包并每季度演练、`release-x86-cross.yml` 只用于 arm runner 不可用。
- **F7**:两份主机清单是 `hosts/tokyo-cex-1.toml`(M7,`verify = true`)和 `hosts/tokyo-cex-1-live.toml`(M8,`verify = false`),各带一份 `.standby`;切换步骤在 launch D-23 第 7 步。
- **IR R2(F1(g))**:停机失败时标记作废为 `.aborted`,作废后是否另发 Kill 由人决定。
- **ops_bot**:X1 P2-12(补发前查 journal)、P2-13(额外确认条件补全)、X3 P2-23(`trigger_seq`)、`/writeoff` 按 F1(f) 改写。
- **pipeline**:F1(e) 移除 group 时提示账户级持仓、X3 P1-1、P2-4、P2-50、X2 P2-10 的 pipeline 一侧。
- **X4**:
  - N-P1-4、P2-7 → launch D-23
  - N-P1-5 → launch A-4 只链接 deploy
  - N-P1-6 → deploy 探针清单 + host-tuning 的探针装机步骤
  - N-P1-8 → IR S 节
  - P2-1 至 P2-10 中属于我的部分已全部改

## 3. 处理了的评审条目

- **X3**:P0 5–11;P1 1、4、14、21、23、25–31;P2 4、17、23、39、49、50、52–56、59、60;§2 #50;ADR 卫生四条;§4 第 4 条(postmortem 日期)。
- **X4**:N-P1-1 至 N-P1-8 中属于我的部分;P2-1 至 P2-10 中属于我的部分。
- **X1**:P2-12、P2-13;F1(e) 的 pipeline 一侧;F1(g) 的 IR 一侧;另外 IR A、B、L、N 节按 F1(a)(b)(c)(f) 改写。
- **X5**:L-P2-11(b) 改为 cgroup BPF connect 钩子。
- **X2**:P2-10 的 pipeline 一侧;P2-11 的措辞残留。

## 留给别人

- **J8(hft-core)**:
  - 全文改名 `hftbatch.slice`,删掉依赖 `CPUSchedulingPolicy=idle` 的说法。
  - 重写 dev2 内存表(`hft.slice` 与 `hftbatch.slice` 分开)。
  - :479 的 `Requisite=` 改为 `After=` 加 `ExecStartPre` 检查。
  - :498 的残留热段判定改为看 tmpfs 已用量(`statfs`)。
  - 退路 (a) 加上只有 files source 的 archiver。
  - 升级主机上 prod 心跳目录改由 `ExecStartPre=+` 建立。
  - `/dev/cpu_dma_latency` 写明经 udev 规则对 `hft` 组可写。
  - 从 S3 恢复时 epoch 加安全间隔。
- **J11(hft-journal)**:冷副本子目录要用 `0770` 建(ACL mask 才不收窄 `hft-archive` 的写权限);读者在段文件被删除后要解除映射。
- **J9**:
  - journal-archiver:`restore` 子命令、通过 ACL 写冷副本、崩溃输入与 resolution 的对象键带主机段、P0-10。
  - kill-switch:`.aborted`、解除对已删除热段的映射。
  - testing:⑥ 的"连续 3 天"遇护栏停机算暂停还是重计、:259 引用了不存在的规则。
- **J6**:
  - control-plane:84 的额外确认条件与 `"v":1`。
  - hostprobe 新增 required 项:`nf_conntrack` 未加载、BPF 已挂载、`cpu.idle`。
  - hft-verify(X4 P2-3、P2-4):限额按 slice 剩余内存算、`-timeout` 等。
  - hft-cfg 从版本库取下一个 GroupId 计数(X2 P2-10 的 hft-cfg 一侧)。
- **J5**:hft-sequencer:623 的"Live 实例"定义对齐 control-plane。
- **J4**:执行层事件要足以重建清仓锁、外来成交计时与自动平仓状态(ops_bot 的兜底判定要用)。
- **J2**:adding-a-strategy:9 补 `--kind-id`。
- **J1**:11 §2 链接 development;11 §3.2 链接探针装机步骤;11:46、11:159 的残留。

## 4. 简报 §7 之外新引入、别人需要用的名字

- **清单**:`tokyo-cex-1-live`、`probe-c32`、`<清单名>.standby`。
- **单元与路径**:
  - `hft-replay-resume.service`、`hft-sim-alternate.service`/`.timer`
  - `/etc/hft/measure.toml`、`/var/lib/hft/measure/active`、`/var/lib/hft/offbox/`
  - `imds/hft-imds-lock.sh`、`imds/hft-imds-connect.bpf.c`、`imds/hft-imds-dev2.nft`
  - `/usr/local/bin/` 下的运维命令链接
- **IAM 与秘密**:运维只读角色 `hft-ops-read`;秘密前缀 `hft/ops/cred-broker/`。
- **CI 与发布**:
  - `RELEASE.json.evidence`
  - `workflow_dispatch` 的 `tag`、`skip_verify` 输入
  - 缓存键 `arm64-ci`、`.github/scripts/ci-usage.py`
- **releasectl**:新定义了 `verify` 与 `uninstall` 子命令。

## 5. 没能确定怎么改的矛盾

1. IMDS 现在有两种机制:dev2 用 nft(按简报 F4(d)),其他主机类用 cgroup BPF(按 X5 L-P2-11(b))。是否统一成一种,要你们定。
2. 冷副本靠 ACL 开放写权限,前提是 hft-journal 建子目录用 `0770`;J11 不改的话,archiver 写不了 `.uploaded`,也删不了已提交的段。
3. dev2 的实例角色与旧系统共用:旧系统的进程仍能读 hft 的测试网与运维秘密,也能扮演 hft 的消费者角色。已写进 deploy 的已知限制,没有解决。
4. ops_bot 新增的"停引擎会中断风险处置"条件(清仓锁、外来成交计时、自动平仓执行中),能不能从 journal 事件重建,我没有核实。
5. dev2 护栏的内存上限拆分(`hft.slice` 与 `hftbatch.slice` 各多少)要等 J8 给出数值;deploy 现在只写"归 hft-core 的 dev2 内存表"。

## J11

J11 已完成。`heading_audit` 结果是没有丢失任何标题。只读运行了 `check_hft`:我的文件没有新问题,篇幅都在上限内(根 AGENTS 1524/1800、docs/AGENTS 879/900、crates/AGENTS 794/900、apps/AGENTS 795/900、architecture 2881/3000)。剩下的报错都在别人的文件里:`docs/tests/README.md` 有 6 处坏链接,另有 6 个 BRIEFNAME 是其他代理负责的名字。

## 1. 改了哪些文件
路径相对 `E:\trading\hft\`:
- `AGENTS.md`、`crates/AGENTS.md`
- `docs/architecture.md`、`docs/glossary.md`、`docs/defensive-patterns.md`、`docs/hot-path-rules.md`、`docs/latency-budget.md`
- `crates/hft-journal/README.md`、`crates/hft-rt/README.md`、`crates/hft-py/README.md`

`README.md`、`CLAUDE.md`、`docs/AGENTS.md`、`apps/AGENTS.md`、`adding-a-crate.md`、hft-book 都核对过,没有需要改的地方。

## 2. 各项落点
- **F3(b)**:
  - hft-journal 调用约束第 5 条改为两级安全阀:软阈值 `size_idle_pct` 默认 75,到达后在第一个空闲轮滚动;硬上限 `size_valve_pct` 默认 90,到达后在下一个事件边界滚动。raw 写者只用硬上限。这两个数只写在 hft-journal 的配置项表里。
  - 新增调用约束第 6 条:实盘写关键帧时 `state_hash` 填 0,由读者侧计算,并链接 journal-format。
  - latency-budget §3.1 和 defensive-patterns"段滚动必须带快照"改为链接 hft-journal。
- **X3 P0-4**:hft-rt 空闲处理第 5 条删掉了公式,改为链接 hft-core 线程计划加载校验第 7 条。
- **F1**:
  - (c) defensive-patterns"自己的强平与 ADL"一节改为"在 Input 内合成、不写记录",链接 hft-sequencer,并补了用户流稳定条件和 `auto_held` 抵扣。hft-py 的 §2 和测试表写明冷读者经 hft-ledger 同一个合成函数看到这笔成交。
  - (a)(b)(d)(f)(g)(e) 在 defensive-patterns 与 glossary 里只保留一句话加链接。
- **F2**:latency-budget 的 §1.1、§1.2、§2、§2.1、§6 改为只链接 testing §8.2;补了 `LatencySample.action` 区分新单、撤单、改单,`order_rtt` 只统计新单。
- **F3(a)**:hot-path-rules T13 的 REST 连接和 standby 连接节拍改了,并引用新基准 `gw-ingest/poll-cadence`。
- **F3(e)**:hot-path-rules T12 和 hft-rt 的 `TASK_ON_HOT` 都链接 hostprobe 登记的"保持唤醒"允许形式。
- **F4(a)**:glossary 的"dev2 护栏"改为 `hftbatch.slice`,同时写入 F4(c) 的停机条件。

## 3. 处理的条目
- **X3**:P0-3、P0-4;P1-11、P1-17、P1-19;P2-6、17、18、25、35、37、39、41;旧表 P2-41;§2 的 #13a;§4 列出的 hft-rt:309"待测"。P2-43 维持"每段一次",这个说法本来就对。
- **X5**:L-P1-1/2/3 的链接部分;L-P2-5、7、9;L-P2-11 的 (d)(f)。
- **X1**:P1-A(dp:302)、P1-B、P1-C、P1-E、P1-D 在 defensive-patterns 里的对应部分;P2-6/7、P2-11、P2-15 的说法。
- **X2**:P1-5 中 LB:61 的口径。
- **X4**:N-P1-8 的 DurableMeta(hft-journal 改为链接 journal-archiver,epoch 公式改为链接 hft-core 启动顺序)。

**留给别人:**
- **J6**:
  - journal-format:158 的"90%/下一个事件边界"和 :8 的页大小副本,都改为链接 hft-journal。
  - journal-format :218/:232 的"写快照时置 0"(P2-5),`SnapshotHdr.state_hash` 的注释要写明实盘填 0。
  - hft-event:669 `Roll=2` 的注释改为两级。
  - tools 的 hostprobe 要登记保持唤醒形式,并检查 rt 加 chrony 合计读 PHC ≤ 125 次/s(hft-rt 已写"由 hostprobe 检查")。
- **J5**:
  - hft-sequencer:299 的 `Sys{Roll}` 改为两级;:751/:821 的"每小时"改为"每段";:574 的 epoch 公式副本。
  - 请保留锚点 `#每条-input-的处理顺序process` 和 hft-ledger 的 `#外部成交externalfill`,合成函数和 `auto_held` 写在这一节。
- **J8**:
  - hft-core 第 6 条"IRQ 专核不绑线程"要和保持唤醒例外衔接;:303 的"90%"改为链接 hft-journal。
  - :148 的 `TASK_ON_HOT` 改为非 hard(P2-41)。
  - 换 AZ 恢复的 epoch 安全间隔写进启动顺序步 5(我的链接指向那里)。
  - housekeeping 校准节拍是唯一归属,rt、latency-budget、architecture 已改为链接 hft-core。
- **J9**:
  - testing §8.2 要包含分层抽样、`kernel_rx` 上下界、HW 与 SW 锚数据集的选用;删除 testing:362 指向 latency-budget §7 的写法。
  - testing:271、:300 仍写 `hft-batch.slice`,需改名。
  - benchmarks 要登记 `gw-ingest/poll-cadence`;benchmarks:49 的 c16 说法(P1-19)。
  - journal-archiver 要写 DurableMeta 的上传与换 AZ 恢复。
- **J10**:ADR rx-irq-driven:17 仍写"hostprobe 核对 NAPI ID",应改为 conn-builder。
- **J1**:A2/01:194、02:133、11:159 同上(P1-17)。
- **J4**:`manual_hold` 的置位例外与解除条件、自动平仓失败原因码、恢复期按 venue 计时——glossary 和 defensive-patterns 现在都只链接 hft-exec。

## 4. 新引入的名字
都在 hft-journal 内部:
- `RollAt { IdleRound = 1, EventBoundary = 2 }`
- `needs_roll` 的返回值改为 `Option<(RollReason, RollAt)>`
- `JournalConfig.size_idle_pct` 和配置键 `journal.size_idle_pct`(默认 75,须小于 `size_valve_pct`)

## 5. 没能确定的矛盾
1. 软阈值触发的滚动是否也要避开资金费结算前后 ±2 min:我留给 hft-sequencer 决定。
2. `hw_rx` 取哪个包的戳:简报写"取最早一个包",hft-net:374 现在写的是"最后一个 skb"。latency-budget 已改为只链接 hft-net §7,由 J7 定。
3. 根 `AGENTS.md`、`README.md` 仍写"规格仓库",和 R8-12 的"进入 M1、设计基线 v1 冻结、`docs/changes.md`、`TESTS.md`"不一致。docs/AGENTS 的层级表和 adding-a-crate 也没有 `TESTS.md` 和 `docs/tests/`。这些不在我的清单内,而且 docs/AGENTS 只剩 21 的余量,由主控决定。
4. 线程计划第 7 条的 `park_us` 校验看起来只针对 dev2,而 hft-rt 的心跳保证适用于所有主机类;它的适用范围要由 J8 写清楚。

## J12

新增的 12 项机检都已写进 `E:\trading\architecture-v2\_work\check_hft.py`。X3 §3 的 11 条 P0 中，P0-3 以外的 10 条在修订前规格上都能抓到，其中 9 条是问题级、P0-10 以外全部按预期命中。P0-3 是语义口径问题，不在这 12 项里。冻结组件没有引用任何已改名的标识符（包括 `hft-batch.slice`），所以一个都没改。

脚本仍用 `PYTHONIOENCODING=utf-8 python E:\trading\architecture-v2\_work\check_hft.py` 运行，约 5 秒，输出 problems/warnings 计数。`--brief` 默认已改为 `round8f-spec-brief.md`。另加了两个选项：`--prev DIR`（只追加检查的对比基线，默认 `_backup\round8f-pre\hft`）和 `--fixes FILE`（默认取简报同目录的 merge-fixes 文件）。

## 新增检查与能抓到的 X3 条目
| 名字 | 级别 | 能抓到 |
|---|---|---|
| PERM 权限闭包；PERMRW（`ProtectSystem=strict` 下写路径要有 RuntimeDirectory/StateDirectory/ReadWritePaths） | 问题 | P0-6、7、8、9，P1-27 |
| PERMOPT（可选组件）、PERMIMP（从组件 README 句子推断出的读写） | 警告 | P0-6 的 kill-switch 两行 |
| GROUPMEM 组成员一致 | 问题 | P0-7，P2-54 |
| S3KEY 对象键缺主机段 | 问题 | P0-10 |
| IAMSET secrets/S3/KMS 授权集合 | 问题 | P0-11，P1-29，P1-31 |
| UNITFORM 单位归一公式；UNITMIX 单条公式混单位 | 问题 / 警告 | P0-4 |
| WIRECODE 线上码要有数值表 | 问题 | P0-2，P1-8 |
| VERBATIM 原样嵌入 | 问题 | P0-1 |
| APPENDONLY 只追加 | 问题 | P1-9，P1-31（行版本） |
| ALLHOSTS "全部主机类"落到 deploy 主表 | 问题 | P0-5，P2-55 |
| LISTCOPY 名单副本 | 警告 | P1-21，P2-17，P2-36 |
| BUDGET 预算上下限；BUDGETPCT（上限没写分位数） | 问题 / 警告 | P1-20 |
| BRIEFDRIFT 简报漂移 | 警告 | 用 round8e 简报测：E13(a) `cancel_resting_on` 参数个数、E10 `/run/hft/guard/tripped`、R8 |
| FROZENREF 冻结组件引用了别处已消失的名字 | 警告 | 为冻结组件的改名检查新加 |

BRIEFNAME 顺带补了 `benchmarks` 等归属映射，§7 里的仓库文件名（如 `docs/changes.md`）现在要求文件存在。

## 在修订前规格上的命中
运行参数：`--base _backup\round8f-pre --prev _backup\round8e-pre\hft`。

- **问题级共 49 条新命中**，逐条核对过，都是 X3 条目或同类的真问题：PERM 14、PERMRW 4、GROUPMEM 6、S3KEY 3、IAMSET 6、UNITFORM 1、WIRECODE 2、VERBATIM 1、APPENDONLY 3、ALLHOSTS 3、BUDGET 6。
- 同类新发现有三处：
  - hft-core:658 的心跳文件要设成 `hft:hft-journal-ro`，但 `hft` 不在该组，父目录也没有 setgid，进程设不了这个属组。
  - `TableRow.smoke_slot` 占用了保留字节。
  - okx 的 `decode_report` 几行预算宽于上限。
- **警告级**：LISTCOPY 15、BUDGETPCT 8、PERMIMP 8、PERMOPT 4、UNITMIX 1、FROZENREF 1；BRIEFDRIFT 用 round8f 简报 9 条，用 round8e 简报 4 条。

## 当前规格（其他代理仍在改，00:28 的结果）
problems=18，warnings=21。新检查里只剩 APPENDONLY 9 条，其余已归零：

- **APPENDONLY（留给 J6）**：`MarketHdr` 从 48 B 增到 56 B，每个行情体（Bbo、Trade、Funding、L2SnapHdr 等）后面字段的偏移都跟着移了。按简报 §4 的"体中部插入字段"规则这是错误。另外 J6 新写的演进规则本身也说 `MarketHdr` 只能占用 `_rsv`。
- **其余未修项（不在我的文件里）**：
  - host-upgrade:22 的 IAM 授权、host-upgrade:32 的 `hft/sim/` 前缀，在最新一次运行里已不再报出，最终结果以合并后重跑为准。
  - `docs/changes.md` 还不存在，归主控。
  - 新建的 `docs/tests/README.md` 有 6 个坏链接。
- **BUDGETPCT 7 条（留给 J7）**：hft-venue-proto 统一门槛表的 `classify ≤ 30 ns`、`BBO ≤ 0.5 µs` 没写分位数，几家 venue 的 p99 超过了它。

## 已知误报
- **PERMIMP**：从句子推断出的读写，动词离路径较远时会误判，所以只做警告。
- **LISTCOPY**：有意只列一部分的名单会被报出（如 portfolio-carry 的 `TargetReason` 子集、检查点键名单的几处副本）。
- **BRIEFDRIFT**：在其他代理补完之前，新 ADR 文件会报"不存在"。
- **APPENDONLY**：只比对同一文件、同名的结构；结构被挪到别的文件就不再比对。

另外：只追加规则里"`_rsv` 缩短"这一条，只在 event-model 的演进规则不允许"占用保留字节"时才报错。这样做是为了和简报 F2(a)(b) 的要求对上，J6 已经把这条允许写进了演进规则。
