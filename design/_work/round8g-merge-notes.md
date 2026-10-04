# M0-g 测试规格代理报告汇总(合并阶段的来源;按文件名检索指向自己的条目)

## T1

T1 已完成:三个 `TESTS.md` 都写好了,三项检查在我名下的文件里都是零问题(检查报出的其余问题都在别人的文件里)。

## 1. 写了哪些 TESTS.md

| 组件 | 用例数 | 按级别 | 表格子行 |
|---|---|---|---|
| hft-types | 85 | table 46、prop 14、unit 13、bench 10、golden 1、fuzz 1 | 570 |
| hft-book | 78 | unit 32、table 25、bench 13、prop 6、golden 2 | 136 |
| hft-rt | 108 | table 33、unit 29、proc 19、bench 16、host 7、prop 4 | 212 |

- **里程碑**:绝大多数用例是 M1。依赖交易所真实样本的金样、BOOK-102 基准是 M3;hft-rt 的 host 用例按机器分别是 M1.5、M3、M7。
- **hft-rt 的级别划分**:`unit` 在任意平台都能跑,靠注入的 `ClockReads`、`SysRoot` 夹具和 loom。`proc` 需要 Linux 内核(futex、epoll、fork/SIGKILL、mmap、绑核、mlockall、strace 计数)。`host` 需要 Graviton、PHC、hugetlbfs 或调优后的主机。
- 三个 README 的"测试与验收"一节都只剩一句链接,这一节下面没有 `###` 子标题,所以没删标题。

## 2. 规格改动(都在我名下的 README 里)

**hft-types**
- `MAX_RAW` 从 `1<<62` 改为 `(1<<62)−1`。原值下 `MAX_RAW + MAX_RAW` 恰好溢出 i64,"加减永不溢出"这条不变量在边界上不成立(TYP-003、006、021 暴露)。
- 新增不变量 4:`mul_rate`、`round_to_tick/lot`、`cap_cross_px`、`ratio` 这类没有错误通道的运算,超界时怎么饱和;`mul_qty` 结果恰为 `i64::MIN` 也算饱和(TYP-011、014、016、018、022、121)。
- 新增不变量 5:指数的合法范围,资产指数 [−18, 6],线格式指数 [−30, 18],越界返回 `EXP_RANGE`。这条补上了原来 `EXP_RANGE` 从未定义何时返回的缺口(TYP-011、042、046)。
- `write_fixed` 的缓冲从 32 字节改为 40 字节,越界指数返回 0(TYP-046 暴露:指数 −30 和 18 时 32 字节放不下)。
- 解析规则第 3 条改为"整数部末尾的零也不计入有效位";并新增第 7 条,规定几种错误同时出现时的报错先后。原规则下 `"…903000000"` 在指数 6 时会报溢出,和 README 里"往返对任意指数成立"的说法矛盾(TYP-042.10、TYP-047)。
- 其他补齐的取值:
  - tick/lot ≤ 0 返回 0;`ratio` 分母 ≤ 0 返回 `Rate(0)`(删掉了没有通道的"且置位")。
  - U256 换算的 d 限定在 [−18, 36];`qty_to_u256` 遇到不能整除时返回 `PRECISION`。
  - 新增 `VenueId::name()`(源文件表里列了它,但没给签名)。
  - `KillBits::account(12)` 在 release 构建返回 0。
  - `cap_cross_px` 的非法输入返回 `Px(0)`,只取整一次。
  - 资金费归一化在 i64 边界取范围内的相邻边界。
  - `FxErr::OK = 0`,并支持按位或聚合。
- `ClientOrderId`:写明了大端位序、校验字节的算法、解码报错的先后顺序、`slot` 超出 20 位时 release 只编码低 20 位;测试向量就是按这个算出来的(TYP-135–140)。
- **RF10**:GroupId 那一行改为"pipeline 与 hft-cfg 用同一续号规则",续号规则链接到 tools hft-cfg。

**hft-book**
- 新增 `BookErr` 码表(BOOK-002、004)。
- 快照的结构校验与失败处置、中价怎么定 base、任一状态都能应用快照(BOOK-010–015)。
- Keyframe 快照在恢复后只写档位,不改 `restore` 读回的元数据。否则恢复后 origin、`crossed_run`、base 和连续运行时不同,回放会分歧(BOOK-016、079)。
- 消息被打断判为 Split 的条件、各种失败对应的 Outcome 位、TOP 位的含义、检查顺序(BOOK-034、039、041、022)。
- `on_conn` 与 `mark_stale` 在每个状态下的迁移(BOOK-065、066)。
- 溢出区满时淘汰离中价最远的档,删除不存在的档是空操作,窗口外更优的档也算最优档(BOOK-051、053)。
- `export` 改为返回 `(u16, u16, bool)`,并规定截断规则。原写法"置标志"没有地方可置(BOOK-081)。
- `restore` 跳过本 BookSet 没有的簿,遇到非法码值报错(BOOK-078)。

**hft-rt**
- 新增 `RtErr`、`ClockErr` 的定义,以及 spsc/ByteSpsc 的容量规则(RT-001、030)。
- `ShmSpsc` 附着时 `NotReady`/`Closed` 立即返回,写明了 epoch 的来源,新增 `replaced()`,定义了 `PeerStatus.alive`(RT-021–027)。
- 时钟:
  - `TscUtc` 的换算公式。
  - 新增 `from_hz_milli`、`tsc_utc`、`calibrate_with` 和 `ClockReads` 注入接口(RT-053 要在任意平台测校准算法)。
  - `sample_pair` 增加时钟参数,因为不知道频率就没法把窗口换成 ns 误差界(RT-054)。
  - 写明"离散"的计算方式、PHC 误差相加。
  - `DriftTracker::new` 与判 `Unsynced` 的规则,原来完全没定义(RT-055)。
- 主机检查:
  - 给 `HostCheck` 各位分配位号,这个字段要进 journal(RT-072)。
  - `CLASS_MISMATCH` 原来的数据源 `lscpu`、`uname -m` 是命令,夹具无法提供,改为读文件(RT-073)。
  - `MLOCK_FAILED` 改为读 `VmLck`;CPU 特性清单改为"本二进制编译期启用的特性"。
  - `rt.host.mode` 由调用方应用,`check_host` 不读它;`SysRoot` 夹具新增 `statvfs` 文件,用来测 `SHM_SHORT` 的扣除逻辑(RT-073.24–26)。
- `prefault` 改为每页读一个字节再原值写回。原写法"写 0"会抹掉重新打开的已有文件内容(RT-086.5)。
- 其他:`OpenMode`、`Arena::new`、`HeartbeatWatch::new`、`claim` 名字截断、`Backoff::new`。
- 新增 `TaskLoop::new`、`idle_action` 和 `IdleAction`,用来以注入的时间测空闲策略(RT-111–115);park 超时为 0 时不 park。
- 新增 `RtConfig`/`CalibConfig` 的默认值与 `validate()`,供"默认值"和"边界"用例使用(RT-130、131)。

## 3. 留给别人

- **docs/tests/README(主控)**:格式示例 TYP-020.3/.4 期望 `"92233720368.54775807"` 解析成 `Qty(i64::MAX)`,但它超过 `MAX_RAW`,应改为 `OVERFLOW`。
- **T3 hft-refdata**:`MAX_RAW / 4` 现在等于 1 152 921 504 606 846 975,用例里如果写了具体数值需要核对。
- **T4 tools hft-cfg**:GroupId 续号用例(RF10)归你们。
- **T13 fixtures**:请登记三个金样或回放来源:`fixtures/decimal/<venue>.tsv`、`fixtures/binance-usdm/depth20/`、`fixtures/replay/binance-usdm-depth-24h.pointer.toml`。
- **T11 hft-sequencer**:恢复后应先应用 L2 Keyframe(新规则下它不改恢复出的元数据);另外 `export` 的签名已变。
- **T15 hft-ctl**:`wait_fds` 的参数类型写的是 `FdSet`,hft-rt 里叫 `WaitFds`,需要统一。
- **T16 hft-core**:下面几条是 hft-core 发起的行为,用例归 T16:每个校准节拍恰好一条 `ClockStatus`(Stepped 时多一条)、经真实 `KillWord` 的 kill 自检、`rt.host.mode` 的 prod/dev 判定。另外 `sample_pair`、`TaskLoop::new` 的签名已变。
- **T22 benchmarks**:登记 `types_*`、`types/clid_*`、`book_*`、`rt_*` 的基准名。

## 4. 合并遗留(round8f-merge-notes)处理结果

- **J10 / X2 P2-10**(GroupId 两个分配者):已改,按 RF10 改了 hft-types README。
- **hft-rt 第 292 行改为链接 hft-core 第 7 条**:已一致,J11 已经改过。
- **J11 开放问题 4**(`park_us` 校验是否只针对 dev2):已一致,hft-core 第 7 条现在写的是"全部主机类"。
- **`TASK_ON_HOT` 链接 hostprobe 的保持唤醒形式**:已一致。
- **PHC 读取频率合计由 hostprobe 检查**:已一致,hft-rt 侧已写明。
- **hft-rt:309 的"待测"**:已一致,J11 已改为合法的"待实测"。
- **hft-book**:没有遗留条目,J11 已核对过。

## 5. 需要你决定的地方

1. **GroupId 是否真的永不复用**:按 RF10 从"当前生效 blob 的最大计数器"续号时,如果被移除的恰好是计数最大的那个 group,它的 id 会被重新分配,这和"永不复用"的承诺冲突。tools 现在的写法是"`--against` 给出的上一版本里的最大值",同样挡不住更早版本移除的 id。要么改成按历史最大值续号,要么接受由指纹重置兜底。
2. **`DriftTracker` 判 `Unsynced` 的口径**:我采用的是最小定义"窗口不足 2 对"。如果你要的是"系统时钟与 PHC 偏离超过某值",需要定阈值;这会影响 hft-core 的 CLOCK 启动闸门。
3. **`CNTVCT_TRAPS` 的命中路径无法用测试构造**:只能在真实主机上确认"不命中"。
4. **增量深度金样 BOOK-106 与回放基准 BOOK-102**:依赖的增量深度属于可选扩展,venue-binance 的 core 部分不产出它,我暂按 M3 写。需要你决定:是提前用采集数据生成样本,还是改成随可选扩展启用。
5. **`TASK_ON_HOT` 在 IRQ 专用核上的保持唤醒白名单**:登记在 hostprobe,`check_host` 拿不到这份名单,所以这一位在白名单内的线程上会误报(只是告警,不阻断启动)。

## T2

我已写完 `crates/hft-event/TESTS.md`。在我名下的文件上,三项检查都已归零:`check_tests` 无问题(也无警告),`check_hft` 无问题,`heading_audit` 没有丢失标题。用例还没有实现,`实现` 字段只是约定的落点;我也没有跑过任何测试。

## 1. 写了哪些 TESTS.md

`E:\trading\hft\crates\hft-event\TESTS.md`,共 109 个用例,表驱动行 686 行。M1 有 104 个;M2 有 5 个(EVT-080–084,AdvisorLag,随 sequencer 的 advisor 一起完成)。

| 级别 | 用例数 |
|---|---|
| table | 62 |
| unit | 28 |
| prop | 6 |
| bench | 6 |
| fuzz | 4 |
| proc | 2 |
| golden | 1 |

简报点名的几项都有专门的用例:
- **布局**:每个 `#[repr(C)]` 结构的大小、对齐与偏移(EVT-001/040/050/060/070/090/130/150/160),含 RF1 的 `MarketHdr` 56 B 与 `rx_empty_gap_ns`@48。
- **编解码往返**:定长、Var、命令、多段、SPB1、定时器各一个属性测试。
- **注册表与码表**:`KINDS` 逐 kind 39 行;Note 登记表逐行 82 行(码名、来源、级别、`v[]` 位布局);全部线上码表。
- **定时器编码**:逐类别打包、解包、往返和错误(EVT-110–113)。
- **`LatencySample.action`**:只允许 0..=2,另有 v1–v3 旧记录的读出用例。
- **gen-schema**:编号消失、复用、只追加违规、Note 码表核对、hft-ext-proto 合并、违反时不改写、CI 基线(EVT-172–180)。
- **环拓扑登记**:EVT-144。

## 2. 规格改动

除特别注明外,都在 `crates/hft-event/README.md`。
- **记录头、标志与 kind**:写明每个 kind 的 `crc`(Always = ControlCmd、ConfigBlob、StateSnapshot、RawRx、RawTx)、`journals`、`optional` 取值,RawRx/RawTx 的角色为 Meta。由 EVT-004 暴露。
- **按体分类**:改成 `classify()` 唯一规则,判定字段非法时返回 `InvalidEnum`。由 EVT-007 暴露。
- **体 trait 与编解码**:
  - 新增 `EncodeErr{TooLong, BufTooSmall, Count}` 与 `DecodeErr::BadPart`。
  - 写明 `body` 的边界、`decode_record` 的各错误条件、`read`/`view`/`var` 的长度、对齐与 kind 规则,以及 Var 尾部长度 L 的推导。
  - 由 EVT-010–026 暴露。
- **原来只写在测试节里的规则移入正文**:oflags 保留位 debug_assert(EVT-052)、`action` 越界 debug_assert(EVT-072)、命令解码的各错误(EVT-154)。
- **执行段**:
  - 新增 `TargetOriginWire`(EVT-055)。
  - `FundingSettle` 合成记录的 `rx_tsc` 写 0,否则验证回放比对不确定(EVT-101/102)。
  - `SwapDecision` 无对冲腿时 `hedge_side` 写 Buy,否则读取必然 `InvalidEnum`(EVT-061)。
- **Note 登记表**:
  - `DrainTimeout` 改为按条件定级,与 hft-sequencer 一致。
  - 按 RF8 登记 `OmsForeignFill`。
  - 新增代码形式 `NoteName`/`VField`/`NOTE_NAMES`/`note_code`(EVT-075–078)。
  - Venue/Measure/Housekeeping 源的码只核对前缀(EVT-177)。
- **测量字段**:`MEASURE_MASK` 只列体内字节,`FieldDesc.measure` 的一致性只要求 Output kind。由 EVT-100/101 暴露。
- **定时器编码**:原来没有任何 API。新增 `TimerCode` 的打包与解包,写明错误类别与检查顺序。由 EVT-110–113 暴露。
- **SPB1**:新增 `ParamsErr::DupGroup` 与 `BadHdr`,并写明判定顺序。原来 blob 内的重复 `GroupId` 被 `parse_spb1` 报成 `BadGroup`,与 ctl 的 `GroupIdDup` 冲突。由 EVT-132 暴露。
- **环元素类型**:hk 环原来没有元素类型,新增 `HkMsg`;补 `CtlRecord`/`AdviceRecord` 的错误条件。由 EVT-140/143/144 暴露。
- **控制命令**:`encode` 自行填写 `cmd`、`cmd_ver`、`payload_len`;`InjectWarmup` 的上限注释原来漏算 CRC,改为 4 288 B。由 EVT-156/158 暴露。
- **变长前缀**:补 `multi_view`、`PartIter`、`Reassembler` 的签名与错误。由 EVT-161–165 暴露。
- **实现说明(gen-schema)**:
  - lock 原来只存字段列表哈希,无法判定改名或改型,改为存完整字段表。
  - 每次运行都核对,违反时不写任何生成物;CI 以合并基点为基线;首次生成不核对(对应 RF1)。
  - 新增失败条件,并登记 `notes_catalog.rs`(hft-telemetry 已在引用,但这里原来没有)。
  - 由 EVT-172–180 暴露。
- **测试与验收节**:只保留指向 TESTS.md 的一句,原节下没有 `###` 子标题。
- **`docs/subsystems/event-model.md`**:时间戳表里 `hw_rx_ns` 与 `rx_empty_gap_ns` 的产生规则改为链接 hft-net §7(RF12)。
- **`docs/cookbook/adding-an-event-kind.md`**:与 README 不一致的 `kinds.lock`、`tests/layout.snap` 改为 `schemas/event-layout.lock` 和对应的 EVT 用例;第 8 步加"在 TESTS.md 补用例"。

## 3. 留给别人
- **T4**:
  - doc-check 需要新增环名一致性检查(EVT-144)。
  - tools 的 gen-schema 节要与 hft-event 实现说明对齐:合并基点基线、违反不写、`notes_catalog.rs`、前缀核对。
  - EVT-172–180 的实现落在 `tools/xtask/tests`,TOOL 编号不要重复。
- **T11**:
  - EVT-080–084 落在 hft-sequencer 的测试里,不要另起编号。
  - 环表 hk 行的元素写 `HkMsg`。
  - 合成 `FundingSettle` 时写 `rx_tsc = 0`。
- **T15**:hft-ctl 写明 `ParamsErr` 到 detail 的映射(`DupGroup` → 111,其余 → 112);`UnknownKind`/`InvalidEnum` 回 `UnknownDiscriminant`(100)。
- **T5**:SDK 视图与 `TargetOriginWire` 的逐值往返、`LinkRole` 同值的用例放在 SDK 一侧。
- **T3**:写者按 `KindInfo.crc` 和 journal 类别置 CRC;读者重组使用 hft-event 的 `Reassembler`。
- **T22**:登记 6 个基准名:`event_encode_80b`、`event_encode_advice_1k`、`event_decode_view`、`event_crc_64k`、`event_cmd_decode`、`event_spb1_parse_500`。
- **T16**:housekeeping 与启动路径按 `HkMsg::Sys{timer_id}` 经 hk 环送出。
- **T12/T14**:"`QueryResult` 在 `cum_filled > 0` 时 `avg_px` 不得为 0"的反例用例放在 venue 侧。

## 4. 合并遗留处理
- **J4→J6**:
  - Timer 表的 `Exec Alarm=6` 与 `Oms UnknownQuery` 打包:已一致。
  - `ExecGateHeldNaked` 两种场合:已一致(RF3)。
- **J5→J6**:
  - 三个 Note 登记:已一致。
  - `DrainTimeout` 定级:已改。
  - `Sys{Snapshot}` 的触发说明:已一致。
- **J7→J6**:
  - `MarketHdr`:已一致(RF1 接受 56 B,不升版本)。
  - event-model 的取戳说明链接 hft-net §7:已改。
- **J11→J6**:`Roll=2` 两级安全阀:已一致。
- **J12**:`MarketHdr` 的 APPENDONLY:按 RF1 已一致,check_hft 不再报。
- **RF8**:`OmsForeignFill` 登记:已改。

## 5. 需要你决定(4 条)
1. **CRC 策略**:seq journal 中哪些 kind 带 CRC,原规格没有取值,我定为上面列的五个 Multi kind。请确认,这会影响写入成本。
2. **hk 环的写者**:环表里 hk 环只有 ×1(单生产者),但 housekeeping、advisor 和启动路径三方都往里写。单写者靠转发还是拆环,需要定。
3. **gen-schema 的 CI 基线**:取 PR 目标分支的合并基点(防止同一提交连基线一起改)需要 tools 支持;不接受的话删掉 EVT-180。
4. **EVT-196 的零系统调用测法**:用 seccomp 过滤器验证。如果 CI 环境不允许,降级为只统计分配次数。

## T3

T3 已完成。两个组件的 `TESTS.md` 都写好了,README 的"测试与验收"一节已全部迁过去。三项检查里,我名下的文件只剩一处 check_hft 报错,原因是它和 RF4 冲突,我没有消掉(见第 4 节)。

## 1. 写了哪些 TESTS.md

| 文件 | 用例数 | 按级别 | 表格子行 |
|---|---|---|---|
| `E:\trading\hft\crates\hft-refdata\TESTS.md` | 89 | table 62、unit 15、bench 7、prop 2、integ 1、fuzz 1、host 1 | 465 |
| `E:\trading\hft\crates\hft-journal\TESTS.md` | 120 | table 49、unit 43、proc 10、bench 9、host 5、integ 2、prop 1、fuzz 1 | 263 |

- **hft-refdata**:51 个 `LoadErrCode` 每个都有触发用例,含 ±1 边界。RF4、RF6 都有专门用例。另外覆盖了 `VenueCaps` 各项检查、ECF1 的容器、视图、验证器、版本规则与往返,以及每张码表的码值与往返。
- **hft-journal**:写者、读者、两级安全阀、关键帧 `state_hash` 原样透传、截断与崩溃恢复、Meta 与 `EVENT_END`、RF18(建目录权限 + 读者解除映射)、persist、退役、校验、DurableMeta 都有用例。
- journal-format 没有自己的 TESTS.md,它的格式断言放在 hft-journal 的 TESTS.md 里,覆盖表后附了"该页各节 → 用例"的对照清单。
- 级别分界写在 hft-journal 用例的开头:
  - `unit`:内存后端,任何平台都能跑;标"Linux"的 unit 用 tmpfs 或 ext4 临时目录。
  - `proc`:CI 上的多进程,或要挂 hugetlbfs 2M、用 sudo、strace 的用例。
  - `host`:1 GiB 大页、bpftrace、EBS、真实 ACL。

## 2. 规格改动(都在我名下的文件里)

**hft-refdata README**
- RF4:TOML 的 `uid_policy` 取值改成小写。
- RF6:`DuplicateVenueUid` 只在一对账户里至少一方是正式账户时报错。两方都不是正式账户时,加载照常成功,这条记进新增的 `warnings()`(REF-032)。
- 补了几个接口签名,原来的检查规则没有可调用的入口:
  - `check_assembly`(带 `ProtocolFacts`、`AssemblyFacts`),承接 hft-core 启动时的那组交叉校验,如 HostClassMismatch、UnknownStrategyKind;
  - `check_rebind`(处理 BindingChangedWithPosition)和 `check_ref_inst`(REF-061 到 REF-065);
  - `GroupSpecView`,把原来"实现时二选一"的写法定死(REF-059、REF-060)。
- `EngineConfig::load` 拆成容器级的 `load` 和按段的 `validate`;验证器带上段 id。原签名里验证器需要 `RefData`,但 `load` 没有这个参数,两者矛盾(REF-066 到 REF-069)。
- 补码表:`TsUnit`、`EngineSection`。
- `LoadReport` 的规则写明:同一步内收齐全部错误、有错就不进下一步、第 10 步的行号取旧版本里的行(REF-010、REF-011、REF-041)。
- 第 1 到 10 步按用例暴露的缺口补了规则,不加新功能:
  - 结构与 id:非法枚举值报 `Structure`;按 VenueId 排的表不要求稠密;
  - 引用与能力表:别名和 VenueCaps 也要检查悬空引用;inst 种类要和 venue 的产品线一致;死手刷新周期必须大于 0;
  - 指数与步长:线格式指数必须落在 [−18, 6];新增字段 `InstMeta.min_qty_str` 用于复核最小下单量;
  - 账户与版本:Live 账户不能有 Sim 段的行;`AlreadyActive` 时只返回这一条。
- 几个查询的语义写明:`fees` 的覆盖优先级、`venue_account_ix` 的排序、`account` 越界返回 NONE 行、维护窗口为左闭右开、`get_num(0)` 恒不命中。
- `ref_inst` 那条校验按持仓驱动订阅的新规则重写;ECF1 段 4 改为链接 `RingCapRow`。

**hft-journal README**
- RF18:冷副本下新建的子目录最终模式恰为 0770(父目录有 setgid 时为 2770);读者看到段已退役或文件已删除后解除映射(JNL-084、JNL-092、JNL-093)。
- 补了类型定义:`WriterEnv`、`OpenErr`、`ReadErr`、`GapInfo`、`GapReason`、`LogicalEvent`、`PrepState`、`VerifyReport`;新增 `AppendErr::InBoot`;`RollErr` 每个码都写明触发条件。
- `publish` 只推进到事件边界;raw 写者每条记录自动带 `EVENT_END`;引导块里只有 `BootDone` 带它。
- 预留量改成"当前段引导块大小 × 2",原来的"首段用配置的估计值"并没有对应的配置项。阈值改为整数比较,并写明多个滚动条件同时成立时的优先顺序。
- `append_var` 放不下时直接返回 Full,不写任何字节;`TooLarge` 的条件写全。
- 恢复截断点不低于 `committed_len`;冷副本未封存时也按第 2 步规则截断;`open` 之后第一个操作必须是 `roll(boot_reason)`。
- 读者各个起点、终点与缺段处置的语义写明;`reassembly_cap = 0` 表示不重组。
- DurableMeta 的文件格式与键名规则、`held_by_instance` 统计哪些文件、`cold_retire` 的删除条件写明;各参数的缺省值都有可调用的出处。

**journal-format**
- 节 21 补了每账户一条闸门行,行顺序链接 hft-exec;`gate_closed_tsc` 登记为不平移的 TSC 字段。
- 提交游标第 5 条写明截断点不低于 `committed_len`。

## 3. 留给别人

- **T2(hft-event)**:在 dev 构建里登记 hft-journal 测试要用的 kind 0x7F01 到 0x7F06;提供 `From<&GroupSpec> for GroupSpecView`。
- **T4(tools)**:
  - hft-cfg 的 `uid_policy` 改成小写;
  - 两个非正式账户 UID 相同时只在差异报告里列出(`warnings()`),不再判失败;
  - "ref_inst 被订阅"那条检查改成"存在且可交易";
  - 实现 REF-079(路径在 tools/hft-cfg 下)。
- **T16(hft-core)**:
  - 启动时调用 `check_assembly`、`check_rebind`,并把 `warnings()` 以 P1 报出;
  - 589、882 行的 `uid_policy` 改小写;
  - 原 refdata 测试里 `ProbeOnProdAccount` 那一项(错误码归 hft-core)请在 hft-core 的 TESTS.md 写。
- **T10(hft-ledger)**:段 2 的验证器对每个 `ref_inst` 调用 `check_ref_inst`。
- **T9 / T11**:JNL-005、JNL-006(hft-risk)和 JNL-007(hft-sequencer)是行布局断言,实现位置在你们的 crate。各归属 crate 自己写 `CapacityShrink` 的用例,refdata 只断言加载接受更小的容量。
- **T13**:raw 写者收到 `Full` 时怎么处置,README 现在写的是归 hft-venue。
- **T14、T20、T23**:venue-*、launch 与相关 ADR 里的 `uid_policy` 改小写;hot-path-rules D6 补登记 `gate_closed_tsc`。

## 4. 合并遗留的处理

- **已改**:
  - 节 21 的行顺序与 `gate_closed_tsc`(来自 J4);
  - `ref_inst` 校验重写(来自 J5);
  - ECF1 段 4 链接 `RingCapRow`(来自 J8);
  - 冷副本子目录 0770 与读者解除映射(J10 转 J11,即 RF18);
  - Binance 演示环境的 UID 重复(J7 §5-2,即 RF6);
  - `uid_policy` 小写(J6 §5-2,即 RF4,我名下部分)。
- **已一致**:
  - 关键帧 `state_hash = 0`,以及节 10、13、17 的行版本(行版本只在归属 crate 登记);
  - 段 5 的 `row_size = 72`(refdata 只链接 hft-sequencer);
  - hft-journal 的两级阈值数值、删页大小副本;
  - journal-format 第 158 行与第 8 行;
  - 软阈值滚动是否避开资金费结算(已由 hft-sequencer 定)。
- **部分**:J6 §5-4 提的码名重叠——hft-core 启动时交叉校验的码现在由 refdata 的 `check_assembly` 产生;hft-core 自己的退出码名留给 T16 对齐。
- **无法改**:check_hft 的 BRIEFNAME 仍要求在 refdata README 里找到 `uid_policy = "Verified" | "Unverifiable"`。这个字面值来自上一轮简报的命名表(检查默认读这份简报),与 RF4 冲突;要么改检查默认的简报,要么放行这一项。

## 5. 需要你决定的地方

1. 上面这条 BRIEFNAME:是改检查默认的简报,还是对这一项放行。
2. RF6 我按字面理解:两个非正式账户即使在**同一个** VenueId 上 UID 相同,也只告警。如果只想放过"同所不同产品线"的情形,需要再收紧。
3. `journal.disk.warn_pct` / `crit_pct` 写在 hft-journal 的配置表里,但 hft-journal 并不使用它们,我没有写缺省值用例。这两个键该归谁?
4. hft-refdata 的依赖只有 hft-types,但 `host_class()` 返回的是 hft-rt 的 `HostClass`。是在 refdata 里定义一个同码值的类型,还是加一条依赖?
5. 1 GiB 大页与冷盘卷上的 persist 滞后,最早的实测证据要到 M7(JNL-106、JNL-117)。是否要求更早?

## T4

三个 `TESTS.md` 已写完,三项检查在我名下的文件上问题都是 0。`check_tests` 无问题、无告警;`check_hft` 全仓还剩 24 个问题、9 个告警,都不在我的文件里;`heading_audit` 丢失标题 0。

## 1. 写了哪些 TESTS.md(按级别计数)

| 文件 | 用例数 | 按级别 |
|---|---|---|
| `E:\trading\hft\tools\TESTS.md`(TOOL) | 178(表行 494) | unit 93、table 62、integ 7、host 5、golden 4、bench 4、replay 2、proc 1 |
| `E:\trading\hft\crates\hft-testkit\TESTS.md`(TK) | 93(表行 123) | unit 51、table 26、prop 7、dst 6、bench 2、proc 1 |
| `E:\trading\hft\crates\hft-telemetry\TESTS.md`(TEL) | 90(表行 153) | unit 48、table 30、bench 6、prop 2、golden 2、host 2 |

三个 README 原来的"测试与验收"节都已整体迁入,README 里只留一句链接;这一节下原本没有 `###` 子标题。scaffold-gate 的用例放在 tools,testkit 的覆盖表引用 TOOL-024、TOOL-027–TOOL-029。

## 2. 规格改动

**tools/README.md**
- `ci` 的步骤里加上 `dst --hashtrail check`,与 development 已写的一致(TOOL-004、TOOL-009)。
- `dst`:
  - 新增 `--hashtrail check|write` 参数。`fixtures/dst/hashtrail.toml` 原先没有任何命令能产出或核对它。
  - 写明 `--suite` 的语义:不带时由种子派生套件,带时全部种子在该套件运行。
  - `--regressions` 的种子在其回归文件所属的套件中运行。
  - 复现命令加上 `--suite`(TOOL-047、050–052、054)。
- `new-strategy`:kind_id 已被占用或目录已存在时退出 1,不写任何文件(TOOL-071)。
- az-probe 的 `compare` 写明排名依据:按 (AZ, CPG) 的 core 交易所握手 minRTT 之和升序(TOOL-097)。
- hostprobe:
  - 删去没有任何定义的 `--mode live|bench`。
  - 补了 deploy 列为 required、但采集表里没有的项:IMDS 锁定(BPF 程序或 dev2 的 nft 规则)、护栏守护是否 active、出口地址是否关联 EIP。
  - 输出中写明 `pending_reboot` 与 `unreadable`。
  - `verify` 写明不符项的打印格式、退出码 0/1/2,且 required 项读不到也算不符(TOOL-100、105–110)。
- hot-check 表补了 hot-path-rules 中指名由 hot-check 检查、但表里漏掉的四行:M1、S1、N3 model_math、D6(TOOL-032)。
- gen-schema:`--check` 的失败条件改为链接 hft-event;找不到 hft-core 二进制时退出 2(TOOL-059、061)。
- hft-cfg:
  - strategy 构建的命令行加上 `--against`。
  - 按 RF10 写成唯一的 GroupId 续号规则,并补上"已有 id 的指纹不得变化"(TOOL-125)。
  - `uid_policy` 改为小写取值(TOOL-131)。
  - `ref_inst` 的检查从 strategy 校验中移除,改为只由 hft-core 启动时检查(它属于 ECF1 段 2,hft-cfg 在 strategy 构建时拿不到)。
  - 已知限制新增一条:生效 blob 中已移除的最大 `n` 可能被人工 TOML 再次使用。
- hft-secrets:
  - "不链接 AWS SDK"收窄为不链接 secretsmanager 与 kms 的 SDK。journal-archiver 本来就用 S3 的 SDK,原句与之矛盾。
  - deps 新增第 11 条规则,只允许 hft-secrets 的闭包含这两个 SDK。
  - 清单出现未登记的键或重复的名字时退出 2(TOOL-117、118)。
- hft-verify:
  - libFuzzer 内存限额的余量定为 64 MiB,且 `-malloc_limit_mb` 取同一值(TOOL-149)。
  - `host_tag` 的哈希算法与 `n` 的续接方式写明(TOOL-167)。
  - 账本折叠口径写明:没有 `RUN.json` 时运行的完成时刻怎么取;疑似段的 CPU 秒照常计入;清零时保留 `ranges`;`by_digest` 只被同 digest 的失败清零,DST 的 `by_digest` 另含运行数与天数(TOOL-154–166)。
  - 预计达成日期的公式写明(TOOL-170);`crash list` 的复现命令带上 `--suite`。

**hft-testkit/README.md**
- decoupling-gate 的"引擎"一侧与 tools 对齐为 `crates/*` 与 `apps/*` 路径(TOOL-012.5)。
- 失败行带 `--suite`;写明套件选择公式与缺省步数 10 000(TK-049、067、069)。
- I20 改为链接 hft-exec 的计时规则(RF2)。
- SimActor 在崩溃后由快照与 journal 重建模拟真值(RF9,TK-064)。
- hft-dst 增加 `--checkpoint-seeds` 参数(TK-077)。
- `FaultRates` 原注释写"缺省值见 testing.md",但 testing 里没有这些值;改为由各套件自己给出(TK-086)。
- `one_sided_move` 三种形状的语义写明(TK-017)。
- 新增 §9 `CountingAlloc`/`HistBench`。hot-path-rules 与 benchmarks 都说它们定义在本 crate,但 README 里原本没有(TK-089–091)。

**hft-telemetry/README.md**
- 按 RF12,`rx_empty_gap_ns` 的产生改为链接 hft-net §7。
- `frames_in_read` 写明包含记录自身,"同一连接"指 (venue, conn)(TEL-025、026)。
- `event_weighted` 改为按数据集返回多份报告。原签名只能返回一份,与"HostProfile 变化即分成两个数据集"矛盾(TEL-030)。
- 窗口对齐方式;负值、零值、缺失值各自怎么处理;`series_overflow` 计数器;`day_close` 的日期编码(TEL-009、020、022)。
- 交易所时钟的"偏移界"定为 `max_error_ns`(TEL-017)。
- `Sum` 聚合溢出时饱和并置位标记(TEL-049)。
- 跨所偏离的参照改为"全部 venue(含自身)的中位数",使原测试中"其余两所偏离不变"的说法成立(TEL-058)。

## 3. 留给别人

- **T22(benchmarks)**:登记这些基准名:`hft-verify/driver_rss`、`hostprobe/collect`、`hft-cfg/build_strategy_500`、`xtask/decoupling_gate`、`testkit/dst_step`、`testkit/golden_all`,以及 telemetry 的 6 个。
- **T18**:
  - watchtower 的 `MarkDivergence` 仍写"相对其余 venue 中位数",要与 telemetry 的新定义对齐。
  - journal-archiver 的上传排除规则要按 `.inflight.json` 这个隐藏文件名核对一遍。
- **T17**:`replayer latency` 要适配 `event_weighted` 返回多份报告(每个数据集一份)。
- **T3**:hft-refdata 第 244 行的 `uid_policy` 取值要按 RF4 改小写;RF6 里那条 P1 告警从哪里报出、何时报出,需要写明。
- **T21**:pipeline 的 GroupId 分配改为链接 tools hft-cfg 的续号规则(RF10)。
- **T1**:hft-types 的"永不复用"保证,要与 RF10 下"已移除的最大 n 可能被再用"这一边界情况的措辞对齐。
- **T19**:deploy 的期望剖面要包含 hostprobe 新增采集项的期望值:BPF 程序名、nft 表、守护状态、EIP 关联。
- **T16**:hft-core 要有一个用例,验证秘密文件读入后立即被删除。

## 4. 合并遗留处理结果

- J1 / J8 / J11 → hostprobe 的 PHC 读取频率、ENA allowance 计数、keepawake 白名单:**已一致**,并补了用例(TOOL-100、105、106.24/.25)。
- J9 → `.inflight.json` 改名:**已改**。
- J9 → 超时输入的存法改为链接 testing §3.3:**已改**。
- J9 → 分类表补信号:**已一致**。
- J10 → hostprobe 新增 required 项:`nf_conntrack` 与 `cpu.idle` **已一致**;BPF 已挂载这一项**已改**(另补了 dev2 的 nft 规则、守护状态与 EIP 关联)。
- J10 → hft-verify 的限额按 slice 剩余内存计算、`-timeout`:**已一致**,另补了余量取值。
- J10 → hft-cfg 的 GroupId 计数:**已改**,按 RF10。
- J6 自身两条:`uid_policy` 的大小写**已改**(RF4);Admin 名单**已一致**(RF5)。
- J9 → testkit 的 I18、ReconActor:**已一致**;I20 **已改**(RF2)。
- J9 / J8 → telemetry 的 `stratum`、`stratified_probe`、`kernel_rx_hi`、A/A 模式与 `Extend`:**已一致**。
- RF9、RF12:**已改**。

## 5. 需要负责人决定的地方

1. **GroupId 续号**:按 RF10,hft-cfg 只看生效 blob,所以生效 blob 里已移除的最大 `n` 可能被人工 TOML 再次使用。可以接受并靠 hft-types 的误复用兜底,也可以让 hft-cfg 另读 journal 历史。
2. **每个 PR 的 HashTrail 核对**:按 development,每个 PR 的 `ci` 都要核对 500 个一致性种子的 HashTrail,这与 PR 的 DST 冒烟 ≤ 3 分钟的预算冲突。
3. **hft-verify 的两处空白**:按 slice 剩余内存算出的限额过小或 ≤ 0 时怎么办;收到 SIGTERM 后子进程 10 s 内仍不退出时怎么办(例如是否改用 SIGKILL)。规格都没写,所以我没有为这两处写用例。语料上限只写了"待标定",配置放在哪里也没定。
4. **testkit 框架验收数值**:我定了"每个植入缺陷都要在 2 000 个种子内被找到"(TK-053),请确认这个数。
5. **hostprobe 的两处空白**:`osnoise` 子命令的退出码没有定义;改动 GRUB 后 `pending_reboot = true` 本身不影响 `verify` 的退出码,这是我的取舍,请确认。

## T5

T5 已完成。`crates/hft-strategy-sdk/TESTS.md` 写好了;三项检查在我名下的文件上都是 0 个问题,只剩一条不属于我的告警(见下)。

**1. TESTS.md**

共 211 个用例,表驱动的子行约 600 行。

| 级别 | integ | table | unit | prop | bench | golden | proc |
|---|---|---|---|---|---|---|---|
| 个数 | 76 | 56 | 42 | 15 | 10 | 6 | 6 |

- 里程碑:`params` 编解码器相关的 21 个用例填 M1,原因是 M1 的 hft-cfg 要用它;其余 190 个是 M2。
- 解耦验收写成了 SDK-004 至 SDK-009:
  - 新增第二个测试策略(`ci-probe`,带 Model 和插件)之后,`crates`、`apps`、`tools/hft-cfg`、`python` 的 diff 为零;
  - 通用工具不重新构建,只靠导出的 schema 就能处理新策略;
  - 同一 PR 同时改 SDK 和策略、或同时改策略和 hft-core 容量表,都被 `decoupling-gate` 拒绝;只改策略或只改 SDK 的 PR 能通过。
- 原 README "测试与验收"一节已整体迁入,README 里只留一句链接;该节下没有 `###` 子标题。
- 引擎侧的契约测试(投递表、模式表、Ctx、检查点)放在 `crates/hft-testkit/tests/sdk_contract/`,用的是测试内手写 `StrategySet` 装配的记录型策略 `RecProbe`,没有调用 `strategy_set!`。

**2. 规格改动**(都在 SDK README;第 4 条起为新写定的规则)
1. 改了检查点重建点的时间基准:`recompute_from_seq` 和 `path_settle_ms` 从 UTC 改为按 `engine_ns` 计。这样和 replayer 一致,也更保守(SDK-080)。
2. GroupId 续号规则改为链接 hft-types(构造)和 tools hft-cfg(续号),按 RF10。
3. 检查点文件格式加了锚点,链到 hft-core 的检查点表(J8 留言)。
4. `strategy_set!` 允许在 SDK 自己的 `tests/` 里调用(SDK-002)。
5. `Views` 的"本事件处理前"改成明确说法:输入已被引擎应用、本次意图还没处理(SDK-056)。
6. 定义了 `SubsErr::Full`(SDK-036)。
7. `Notice` 不论订阅时选的 `Part`,快速部分和 Model 都收(SDK-049)。
8. 模型 tick 的"跨过边界"写明了判定方法;journal 里第一条 `AdvisorTick` 不投递(SDK-037、038)。
9. 写定了 `fingerprint` 的输入字节(SDK-028,金样值已算好)。
10. `ParamError` 新增 `BadText`、`UnknownOp`;写明 `SchemaMismatch` 何时触发;默认值和命令字段出错时 `group_ix` 填 0xFFFF(SDK-092、112、186)。
11. 写明 `encode_body` 遇到同一键不同值、默认值个数不对时报什么错;`decode_body` 的 `n_fields` 不符报 `BodyLen`;`schema_digest` 按小端取值(SDK-095、097、107)。
12. 写明 `decode_cmd` 的各个错误;新增 `CmdError` 的四个变体;`cmds` 查不到种类时返回空(SDK-112、113、186)。
13. `NoteDesc.code` 必须 ≤ 0xFF_FFFF(SDK-114)。
14. `AdviceFull` 也覆盖非法的单条 Advice;`AdvicePayload` 的常量在编译期检查(SDK-173、174)。
15. 结果回填写明 `Shadowed`、`Rejected`、`TargetShadowed` 各对应什么情况(SDK-164、166)。
16. `model_math::to_*` 写明换算比例、饱和规则,NaN 和 ±∞ 返回 `None`(SDK-176)。
17. 新增 `SnapshotWriter::status()`,用来报告 `SnapshotFull`(SDK-025)。

另外改了 cookbook:新增写 `TESTS.md` 的第 5 步,在 `test-trace` 验证里加了这一项,并链接到 SDK 的确定性夹具用例。

**3. 留给别人**
- **T11 sequencer**:
  - `SubsErr::Full` 的定义要对齐;
  - 模型 tick 要能在重启后确定:镜像起步点必须能知道 `last_seq` 之前最近一条 `AdvisorTick` 的 UTC(SDK-039);
  - `create` 返回错误时回什么结果码没写。
- **T7 / T11**:`reduce_only` 命令下增仓目标的处理两边说法不一:exec 说"增加部分按 CLOSE_ONLY 拒",sequencer 说整条 `TargetRejected`,所以这条用例没写。
- **T8 oms**:策略填了 `role ≠ First`,或 `Simultaneous` 配 `Retry` 时,拒绝码没有定义,所以这条反例没写。
- **T6 registry**:"除这两处外不得调用 `strategy_set!`"要加上 SDK 自己 `tests/` 的例外。
- **T4 tools**:
  - `deps` 要检查 `testing` 特性只在 dev 依赖里启用;
  - 策略缺少 SDK 规定的基准入口时,目前没有机检。
- **T13 fixtures**:我用了 `fixtures/params/sdk-tp/`(其中有个新文件 `schema.golden.json`)和 `fixtures/strategies/sdk-probe/ckpt/`,要登记;旧路径 `fixtures/strategy-sdk/` 已不再使用。
- **T18 archiver**:报表聚合方式(`ReportAgg`)的用例归它写。
- **T22**:testing.md 第 103 行和 SDK 的检查点键列表触发了 LISTCOPY 告警(启发式,不算问题)。

**4. 合并遗留**
- J4 → SDK 三条:
  - 外来成交截止写明是名义值:已一致;
  - 核销语义归 hft-oms:已一致;
  - `manual_hold` 的解除:SDK 只链接 hft-exec,已一致。
- J8 → SDK 三条:
  - 重建点改用 `engine_ns`:已改;
  - 检查点布局链到 hft-core:已改;
  - cookbook 写明"改容量表单独提交":已一致。
- J2 → cookbook 补 `--kind-id`:已一致。
- J2 自己记下的 `prev_interval_s` 没有数据来源:sequencer 已在节 10 落地,已一致。
- RF10:已改。

**5. 需要负责人决定**
1. "`decode`/`validate` 超过 2 ms 就视为参数无效"这条依赖机器速度,无法写成确定的用例。建议改成只靠基准约束。
2. `kind_id`、Note 码、`op`、节 tag 规定"只追加、永不复用",但目前没有任何机检。
3. roadmap 把 SDK 排在 M2,但 M1 的 hft-cfg 就要用 `params` 模块。我按 M1 填了,请确认。
4. 策略 `Note` 的 `code = 0` 是否允许,现在没有规定。

## T6

T6(M0-g)已完成。按简报 §1 跑了三项检查,我名下的文件都是零问题:`check_tests` 0 个问题;`check_hft` 0 个问题,剩 2 条警告是 portfolio-carry 原有的 LISTCOPY 误报;`heading_audit` 丢失标题 0。

## 1. 写了哪些 TESTS.md
| 组件 | 用例数 | 按级别 |
|---|---|---|
| `crates/hft-strat-lib/TESTS.md`(SLB) | 152 | unit 55、table 56、prop 20、golden 3、bench 17、host 1 |
| `strategies/latency-probe/TESTS.md`(LPR) | 81 | unit 23、table 36、prop 5、integ 6、dst 1、bench 6、testnet 3、host 1 |
| `strategies/registry/TESTS.md`(REG) | 16 | unit 6、table 7、golden 1、integ 1、proc 1 |

三个 README 的"测试与验收"都已改成只留一句链接,原内容已全部迁入,这一节下没有 `###` 子标题。

## 2. 规格改动(都写在我名下的 README 里)
- **strat-lib**(由 SLB 用例暴露):
  - **补了缺的接口与错误类型**:
    - 新增 `CostErr`、`PctErr` 两个错误枚举;`ROUND_TRIP` 与 `FIXED` 改为互斥(SLB-006)。
    - 新增 `TriggerPx`、`ConfirmWindow`、`Throttle` 的 `Default`,`FundingStats::new`,`Expiring::new`。
    - 新增 `Confirm/Throttle/Dirty` 三个参数结构的单位换算函数,以及 `set_fill_max_ms`、`LeadLag::dropped`。
  - **快速部分构件补了快照与恢复**:`TriggerPx`、`ConfirmWindow`、`Throttle`、`Expiring` 的快照写成定长字段,`TriggerPx` 恢复后保持失效;恢复出错时状态不变(SLB-018、031、053、065)。
  - **资金费**:定下 `funding_flags` 码值;新增 `SettledChanged` 结果——原先"同一事件既结算又改周期"时两种结果只能返回一个,与 README 自己的属性测试矛盾(SLB-073、082)。另外写明了 GAP 判定、回退事件、汇总各项、预热与实时检测的优先级(SLB-068–087)。
  - **分位统计**:定下箱号取整、查询秩、`rank`、`coverage` 四项的算法,写明预热之后窗口末端如何移动,`restore` 要做哪些校验(SLB-093–113)。
  - **领先-滞后**:原文没有定义归一化、显著度与"不显著"的判据,cross-taker 的 `min_conf` 因此无法比较。现在补成完整公式,并写明 `bucket_ns = 0` 时的处理(SLB-116–127)。
  - **其他**:写明 `Throttle` 的截止边界、原因检查顺序、计数饱和、`reset_open_state` 只清哪两个字段;`vwap` 的方向按吃单方向定义,与 cdex 现有用法一致;改正"只订阅 BBO 时能得到 Top"的说法(SDK 规定此时簿为 `Uninit`);结构大小上限给 `CostLine`(168 B)开了例外。
- **latency-probe**(由 LPR 用例暴露):
  - **参数**:补齐各参数范围,写明字段下标;新增参数 `burst_period_ms`,作为分层触发配额;定下 `check` 的错误码。
  - **定价与数量**:写明 IOC 限价与单笔量公式。target 的 `band` 改为半个 `pair_lot`——原写法"约一个 lot"在只开一个 lot 时等于 100%,目标会被判为已到达、永远不交易。
  - **pair/target 阶段机**:写成一张阶段迁移表(平仓后的复查改由 `FLAT` 定时器驱动),补了定时器 tag 1/2 与登记规则、残余上限的口径(pair 按两腿净量加在途相抵)以及超限时只平仓、不再走阶段动作。
  - **事件与状态**:回报按 `RejectBits` 是否为 0 区分交易所拒单与门控拒绝;计数 Note 先于事件本身写出;`Throttle` 用法(quote 组不调 `on_fire`);恢复后各阶段的处理;运行模式一节与"只在 Live 探测"对齐。
- **registry**:
  - `export_schemas()` 最后一项改为由本组件生成 `index.json`;`schema_digest` 写成补零的 16 位小写十六进制。
  - kind_id 登记表定为账本:编号连续、退役保留行,由 `tests/kind_ids.rs` 检查。
  - `strategy_set!` 调用位置加上 SDK 自己 `tests/` 的例外。
- **其他名下文件**:`strategies/AGENTS.md` 加一句 core 策略包另有 `TESTS.md`(篇幅 894/900);portfolio-carry 只补了一句 `SettledChanged` 的处理,其余行为与参数未改。

## 3. 留给别人
- **T5(SDK)**:`RefView::fee`/`inst` 没有失败返回,`fee_round_trip` 的 `UnknownInst`/`NoFeeTier` 无从判断,需要可失败的查询接口。
- **T23 / T4**:hot-path-rules N3 与 `hot-check` 要豁免 `crates/hft-strat-lib/src/model/`(LeadLag 内部用 f64,SLB-134.2 期望通过)。
- **T22**:benchmarks 需登记 `stratlib/*` 17 个与 `strategies/latency-probe/*` 6 个基准名。
- **T4**:`new-strategy` 的编号占用检查按登记表的连续规则(下一个 = 最大值 + 1,退役行仍算占用)。
- **T16**:REG-011 的实现落在 `apps/hft-core/tests/schemas_cli.rs`。
- **portfolio-carry(暂缓,未改)**:`summary` 调用传的是毫秒参数,而函数要的是纳秒,单位不一致。

## 4. §4 合并遗留的处理
| 条目 | 结果 |
|---|---|
| J9→J2 latency-probe 分层触发配额 | 已改(新参数 `burst_period_ms`) |
| J7、J9→J2 跨 gw 探针变体 | 已一致 |
| J2 §5-2 `quote_xgw` 与 gw 线程的映射 | 已改(链接 hft-core 线程计划第 4 条,并写明 Sim 账户不产生跨 gw 样本) |
| SDK 代理留言:registry 的 `strategy_set!` 例外 | 已改 |
| J2 开头说明 / RF15:portfolio-carry 的"策略研究后定(R8-12)" | 已一致(维持现写法) |
| J2 §5-3 `prev_interval_s` | 已一致(`FundingStats` 不使用它) |
| J4→J2 `manual_hold` 规则副本 | 已一致(各策略都只链接 hft-exec) |
| J10→J2 cookbook 的 `--kind-id` | 已一致(不在我名下) |
| J1 §5-4 architecture-v2 里重抄的 `pair_period_ms` | 已一致(值未变,文件归 T23) |
| J2 §5-1(SDK `no_open_ms`)、J8→J2 的 SDK 条目 | 不在我名下,未改 |

## 5. 需要负责人决定(下面这些是我补写的规则,不是原设计里已有的)
1. LeadLag 的归一化与标准分数公式会决定 cross-taker 的 `min_conf` 含义,启用 cross-taker 前请确认。
2. 突发层配额的做法:策略只能看到"同一次读取"凑齐的 BBO,判别的只是突发层的一个子集;`burst_period_ms` 默认 0(不启用)。
3. latency-probe 测试网 24 h 功能验收(LPR-078)我放在 M3。原 README 写的是"M1.5 之前",但 M1.5 只有简化 sequencer,还没有这个 crate。
4. 补的参数范围(`pair_cross` 1 bp–5%、`max_inv_notional` 1–10,000 USDT、`max_consec_fail` 1–100 等)与 10 min 的计数 Note 间隔都标为暂定。
5. pair/target 残余上限的口径(两腿净量加在途相抵)与超限后进入 `Blocked`,是新定的风控相关规则。

## T7

T7 已完成:写了 `crates/hft-exec/TESTS.md`,共 292 个用例,三项机检在我名下的两个文件上都是 0 问题、0 告警,README 丢失标题 0。

## 1. 写了哪些 TESTS.md

`E:\trading\hft\crates\hft-exec\TESTS.md`,里程碑全部为 M2。

| 级别 | 用例数 |
|---|---|
| table | 145(共 783 行子用例) |
| unit | 110 |
| prop | 15 |
| bench | 15 |
| dst | 6 |
| replay | 1 |

- 页首写了公共夹具(时钟、账户、两腿 group、价格与深度、风控视图、显式参数集 C0、目标规格 T0/T1)。所有数值都是具体值,可以直接写成断言。
- README 原有的 37 条 DST 场景:与 testing 已有编号重合的,在 `## 验收` 里逐条写了去向(BK-n、MS-n、LK-n、QF-1、FL-1、LT-1、RC-11、MD-n、B2/B3/B4/B6、C1/C4/C11/C14、FP-1/FP-12),本文件不重复;其余写成 EXE-500–EXE-505。
- optional 链上 feature 与 MakerFirst 的行为列为里程碑外,M2 不实现;未启用 feature 时 MakerFirst 被拒绝的情形有用例(EXE-196)。

## 2. 规格改动(只改了 `crates/hft-exec/README.md`)

1. **入口签名**:`on_order_update`、`on_link`、`on_place_result`、`on_timer`、`on_mode`、`note_strategy_action` 加上 `out: &mut ExecOut`,`note_strategy_action` 另加 `now`;单 cell 入口的产出上限表补上 `note_strategy_action`。原签名没有输出参数,却在上限表里列了产出。最后一片自动平仓的 Done 记录必须在 `on_order_update` 里写出,否则会被同一 Input 里的 `CapCleared` 清掉。暴露用例:EXE-020、EXE-025、EXE-312.9。
2. **`new`**:定义 `ExecConfigError{ForceSlice, ZeroLimit, Capacity}` 及各自的触发条件。用例 EXE-003。
3. **目标的接受与替换**:`SpecInvalid` 补上参数范围:`depth_levels` 须在 1..=10,`depth_frac` 须在 (0, 1],对冲比的分子分母不能为 0。SDK 把这些范围交给执行层定,但执行层原来没写。用例 EXE-042。
4. **调用顺序第 4 条**:`set_target` 接受或替换目标时一律登记立即到期的 Wake 定时器;与当前目标逐字段相同的不登记。用例 EXE-026、EXE-401。
5. **推进第 5 步**,两处与其他小节矛盾:
   - Shadow 和 `open_allowed = false` 改为按"运行模式"只交出一片(原文写成直接门控)。
   - 只因报价不可用时进入 `Stuck{StaleQuote}`,而不是 `Gated`。
   - 用例 EXE-087、EXE-089、EXE-103。
6. **不交易带**:分母下限 floor 明确为向零取整。用例 EXE-101.4。
7. **PairOnFill**:
   - 第 5 条:链接以 `HedgeLimit` 原因断开时进入 `Broken{HedgeVeto}`。用例 EXE-068.6。
   - 第 7 条:LINK_FUNDS 缩量后低于 `pair_min` 时取 `pair_min`(原文与第 2 步"低于 pair_min 不发"冲突)。用例 EXE-167.8。
8. **两腿 lot 第 3 条**:价格上涨时取一个 `pair_lot`,只受 `cap_n` 里的风控项约束。原文说"受 cap_n 约束",但 cap_n 本身包含切片上限,自相矛盾。用例 EXE-182。
9. **补腿**:
   - 被抑制期间不记 `fix.first_tsc`,唯一例外是多出腿所在 venue 未放行。用例 EXE-207。
   - 命令补腿:列出 `ExecCmdErr` 全部取值(Hard、UnknownGroup、NotQuiescent、NothingToFix、QuoteUnavailable、ChainLeg);命令不看补腿阈值;出错时不改任何状态。用例 EXE-220.6、EXE-221。
10. **Broken 处置结束表**:写明交易所拒单与 EncodeError 记 `Rejected`,HEDGE_LIMIT 与引擎单被风控 #5/#6 拒记 `RiskReject`。用例 EXE-257。
11. **外来成交**:
    - 逐字段相同的 `set_target` 不算"已处理"。
    - 计时不经截止就清零时,写 `ExternalSeen{Cleared}`(detail 表 6 新增 `Cleared=5`)。
    - 截止到达时,无论是否发单都写 `ExternalDefault` 决定和 P0。
    - 用例 EXE-293.4、EXE-295、EXE-419。
12. **默认减仓**:新增"引擎目标与 cell 状态"一段:
    - 设定引擎目标时,替换了策略目标的 cell 进入 `Stuck{EngineOverride}`,否则进入 `Working`。
    - 到达时写 `State → Idle`。
    - 清仓目标和对齐目标到达后随之清除,不另写 `TargetCleared`。
    - 用例 EXE-369、EXE-356、EXE-420。
13. **ExecDecision 小节**:新增"冷读者重建"表,即 J10 留言要求的内容。用例 EXE-418–EXE-420。
14. **定时器**:续推用的 Continue 定时器全局只有一个(account 位 0、`GROUP_NONE`)。用例 EXE-022、EXE-440.4。
15. **"测试与验收"节**:只保留一句链接。该节下没有 `###` 子标题;规划源文件表的测试行改为指向 TESTS.md。

## 3. 留给别人

- **T11(hft-sequencer)**:核对上面几个入口的新签名;为 HedgeFix 命令把 `ExecCmdErr` 映射成 `CmdResult`。
- **T15(control-plane)**:HedgeFix 的结果码 / detail 表要登记 `ExecCmdErr` 六个值的映射,目前没有。
- **T16(hft-core)**:`ExecConfigError` 映射到退出码(建议与 `ExecRiskMismatch` 一样退出 2)。
- **T21(ops_bot)**:兜底判定可以直接链接 `hft-exec#execdecision-与进度通知` 的"冷读者重建"表。
- **T22**:请确认 `## 验收` 映射表里引用的场景编号都在 scenarios.md 中。覆盖表没有引用这些编号,是为了避免 scenarios.md 还没生成时报 COVREF。
- `crates/hft-oms/README.md` 当前是 CRLF 换行(备份里是 LF),可能是别人的脚本写入造成的,请 T8 核对。我自己的两个文件已恢复为 LF。

## 4. 合并遗留(round8f)的处理结果

| 来源 | 内容 | 结果 |
|---|---|---|
| J2 → J4 | 保留两个锚点;SDK 引用 `ExecCellRow` 的显式行 | 已一致(没有改任何标题) |
| J5 → J4 | `none_qty` 部分以 `GROUP_NONE` 再调一次 `on_external`;核销无裸量时 `qty = 0` | 已一致(用例 EXE-292、EXE-283.3) |
| J6 → J4 | 状态码改用 `TargetState`;`ExecCellRow` 写成显式行 | 已一致(用例 EXE-453、EXE-454) |
| J10 → J4 | 执行层事件要足以重建清仓锁、外来成交计时与自动平仓状态 | 已改(见第 2 节第 11–13 条) |
| J11 → J4 | glossary、defensive-patterns 只链接 hft-exec | 已一致(锚点检查 0 问题) |
| RF2 / RF3 | 恢复期放行时重给完整窗口;`ExecGateHeldNaked` 两种场合 | README 已一致;用例 EXE-248/249/316、EXE-325–329 |

## 5. 需要负责人决定的地方

1. **冷读者重建的口径**:目前允许多报:引擎目标接手自动平仓时,接手前那一片终态之前仍报"执行中"。另外,策略的 `Exit` 目标替换了清仓目标后,按"清仓已结束"处理。是否需要更保守?
2. **截止到达但裸量在阈值内**:目前仍写 P0 `ExecExternalDefault`(视为"策略未在截止前处理")。如果嫌是噪声,可以改成只在发单时报 P0。
3. **目标为 0、Normal 紧急度、带宽大于尘埃线**:这种组合下,腿在带内但没到尘埃线,于是既不出单也不判完成(EXE-105.2 只断言不出单)。是在 `set_target` 时拒绝,还是让目标为 0 的腿一律只看尘埃线?
4. **Drain 不等待 Broken 期间尚未到期的自动平仓**(EXE-396.11):Drain 可能在自动平仓开始前就完成。这是现有定义的直接结果,请确认可以接受。
5. **链上 optional feature 与 MakerFirst 的用例**列为里程碑外,feature 启用时再补写。

## T8

我已为 hft-oms 写完 `TESTS.md`,并在它的 README 里补齐了写断言时发现的规格缺口。简报 §1 的三项检查在我名下文件上都是零问题;只剩一条警告,是"规划源文件"这一行被标成非规范。

## 1. 写了哪个 TESTS.md
`E:\trading\hft\crates\hft-oms\TESTS.md`,共 287 个用例:

| 级别 | unit | table | prop | bench | dst | integ | golden | replay | testnet | host |
|---|---|---|---|---|---|---|---|---|---|---|
| 个数 | 128 | 118(共 602 行) | 11 | 16 | 6 | 4 | 1 | 1 | 1 | 1 |

除 OMS-420、OMS-421 属于 M6 外,其余都在 M2。README 原"测试与验收"一节已迁入,只留一句链接;这一节下面没有 `###` 子标题。

## 2. 规格改动(全部在 `crates/hft-oms/README.md`)
- **入口与签名**
  - 新增 `on_unknown_query`:`Timer{Oms, UnknownQuery}` 原来没有对应入口(OMS-135、OMS-141)。同时补了预算行 `oms/unknown_query`。
  - `hedge_gated` 增加参数 `retry_at`:原文要求按"该 venue 退避截止"定重试时刻,但这个值没有来源(OMS-189)。
  - `place` 的 Err 增加一个例外:OMS 自己的对冲单下单失败时,链接记一次执行失败(OMS-044)。
  - 定义了 `PlaceError`、`OpError`、`OmsConfigError` 的各个变体,其中新增 `TombGens`;`slots` 的取值范围写成 1..=65535(OMS-002–004、OMS-041–050)。
- **状态机与两张处理表**
  - 写明拥有者撤单、改单在哪些状态下允许。
  - 回报处理表的补充规则:`—` 格的处理;改单后又撤单时收到改单回报怎么办;Ack 时记下交易所单号;Unknown 收到部分成交后回到哪个状态;Rejected 与 Cancelled 同样处理;`Rejected(NOT_FOUND)` 的确切含义(OMS-055–066)。
  - SendReport 表:`query_count` 只在查询写出时加一;CancelAll 确定未写出时写一条 Note(OMS-073、OMS-074、OMS-138)。
- **私有状态位与保留字节**
  - 新增 `ACKED`、`HIST_WAIT`、`ALARMED`、`AGED` 四个位,并写明置位与清除规则。
  - `SlotExt` 占用原保留字节,加 `class` 与 `max_rest_ms`;两类在途改按 `ACKED` 划分(OMS-046、OMS-171、OMS-290)。
- **Unknown 收敛**
  - 写明查询时机、grace 的起点、成交历史等待期的行为。
  - 超过 `unknown_alarm_ms` 只升一次 P0;`OmsVenueReady` 在 `venue_ready` 每次由假变真时写一条。
  - `UnknownResolved` 有成交时按条目的 `exch_qty` 入账。
  - sweep ③ 改为只做升级与截止判定,查询改由定时器驱动(OMS-135–OMS-152)。
- **force_unknown 的范围**:由"非空闲槽"改为"非终态槽",与 testing 的 RC-5 一致(OMS-133)。
- **REPORT_DECODE**:写明连续计数何时清零。计数存在节 3 每张表行的尾部追加字段里,按 RF1 不升行版本(OMS-091)。
- **链接**
  - 定义 Open → Hedging → Done 的迁移条件和截止的判定时机(OMS-201、OMS-207)。
  - 改正 `hedge_req` 的语义:原注释让重试也累加,会把欠对冲量算两遍(OMS-188)。
  - "计次耗尽"写明为 `retries > hedge_retry_max`(OMS-195)。
  - 写明反向单的数量、计账与定价规则(OMS-198、OMS-199、OMS-212、OMS-229)。
  - `ref_px` 的来源改为 `ApprovedIntent.hedge_ref_px`;为 0 时不判超滑点(OMS-180、OMS-228)。
- **对冲定价 Note**:`price_hedge` 是纯函数,写不了 Note。改为由 `hedge_gated` 或下单时写,"每链接每次进入一条"靠 `LinkEntry.flags` 新加的三个位实现(OMS-222、OMS-230)。
- **其他**
  - `out_quota` 的语义(OMS-334);`cancel_open` 只撤 Open 类的 GTC/PostOnly(OMS-344)。
  - 找不到槽的费用仍输出一条 `FeeOut`(OMS-123);外来成交去重后为 0 时不出 `FillOut`(OMS-253);不带成交的 StaleGen 只计数(OMS-031)。
  - `net_filled` 不变量:原式漏了"卖出时基础资产扣费"这一情形(OMS-120)。
  - `recovery_end` 时清零连接状态(OMS-311);进入 Unknown 时给拥有者 `OrderUpdate{Unknown}`,并写明各种回到 Live 时是否给拥有者通知(OMS-057、OMS-062)。
  - 写明哪些单置 `LINKED`(OMS-245);Note 码表补了 `v[]` 布局,`OmsAnomaly` 新增种类 `ReconIgnored`;`batch_cancel` 的写法改成 `flags` 含 `BATCH_CANCEL`。

## 3. 留给别人
- **T2(hft-event)**
  - 登记新码 `OmsForeignFill`(Oms 源,Info)。
  - 把 `OrderEvent` 偏移 3 的 `_pad` 改成具名字段 `detail: u8`:QueryResult 时填订单状态码,CancelReject/AmendReject 时填 GONE。
- **T12、T14(venue 协议与各 venue)**:在 `codes.rs` 里判定 GONE 类,并填写 `detail`。
- **T11(hft-sequencer)**
  - 把 `Timer{Oms, UnknownQuery}` 分派给 `on_unknown_query`。
  - 调 `hedge_gated` 时传入 `retry_at`。
  - 填写 `ApprovedIntent` 的 `hedge_ref_px` 与 `hedge_px`。
  - 由 `ForeignFill` 构造 `ExternalFill` 时,其余字段填 0,保证与 `Note{OmsForeignFill}` 逐字段一致。
  - 收到 P0 级的 `Note{OmsUnknown}` 时设 `UnknownTimeout` cap;这个触发点目前 sequencer 和 risk 都没有写。
- **T10(hft-ledger)**:冷读者在同一事件内读到 `Note{OmsForeignFill}` 时,按字段构造 `ExternalFill` 并入账。
- **T16(hft-core)**:装配校验 `oms.tombstone_keep_ms ≥ gw.id_tomb_ms`。README 写了"须 ≥",但没有任何地方检查。
- **T22(benchmarks)**:登记新基准 `oms/unknown_query`。
- **T13、T2**:`SetEpoch` 的 `accept_prev` 不能按"当前 epoch − 1"去推上一 epoch(两次运行之间可能有安全间隔),应以 `MirrorRestore` 带的槽 epoch 为准。

## 4. 合并遗留
- **J5 "OMS 核销没有裸量时返回 qty = 0"**:已一致(OMS-282)。
- **J5 §5-2 / RF8 "防御路径合成的外来成交冷读者看不到"**:已改。新增 `Note{OmsForeignFill}`,入账字段写在"回报匹配与成交增量"第 8 条(OMS-114、OMS-115)。
- **J6 UnknownQuery 定时器编码与 `OmsTimerReq.id` 一致**:已一致(OMS-024),另补了缺失的入口 `on_unknown_query`。
- **J4 §5-5"上一 epoch"的定义**:OMS 一侧已一致;gw 一侧留给 T13、T2。
- **J12 `TableRow.smoke_slot` 占用保留字节**:按 RF1 接受,已一致,不改。

## 5. 需要你决定的地方
1. GONE 分类和查询状态码都依赖 T2 新增 `OrderEvent.detail`。T2 不加的话,OMS-061、OMS-064、OMS-066 无法实现。
2. `hedge_deadline_ms` 从第一腿首次成交起算。GTC 第一腿几秒后又成交、而这次的第二腿失败时,链接会立刻 Broken(OMS-204.2)。要不要改成从"最早一笔未对冲的成交"起算?
3. 撤单确定未写出后,槽停在 `PendingCancel`,撤单持续义务不会重发,只能等 `cancel_timeout_ms` 转 Unknown 再查询收敛(OMS-342.6)。Kill 期间连接断开时,撤单会晚 2 秒以上。
4. 本轮补的 README 布局改动没有升行版本:SlotExt 占用保留字节,节 3 每张表行尾追加 `decode_fails`。这是按 RF1 处理的;如果你认为基线已冻结、改动应走升版本的流程,需要你定。

## T9

T9 已完成:写好 `crates/hft-risk/TESTS.md`,README 的规格缺口已补,三项检查在我名下文件(TESTS.md 与 README)上都是 0 问题。另有约 10 处跨文件改动要别的代理配合,其中 T11 一处不改,保证金在快照回放时就会和实盘对不上(见第 3 节)。

## 1. 写了哪些 TESTS.md

`E:\trading\hft\crates\hft-risk\TESTS.md`:132 个用例,里程碑全部为 M2。按级别:table 65(共 625 行子用例)、unit 42、bench 14、prop 9、dst 2。README 原"测试与验收"一节的全部内容已迁入,该节只留一句指向 TESTS.md 的链接;这一节下原本没有 `###` 子标题。

简报点名的各项都有用例:
- 检查链 #0–#9 与 #7b 各自的通过、拒绝与 ±1 边界,以及 `RISK_REDUCING` 的放行。
- cap 表每一行的设置与解除。
- 启动闸门每一位,含 `uid_policy = Unverifiable`;`venue_gate_ok` / `venue_gate_open`。
- MarginStress 每条档线的 −1/0/+1、滞回、数据过期、比率交叉校验、交易所与估算取保守、重启跨截止与重启时处于 L3。
- 对账分级与 `auto_external_ok`;RF7 存储推迟;`RecoveryHeldSim`;日亏;令牌桶预留与退避。

## 2. 规格改动(全部在 `crates/hft-risk/README.md`)

1. **RF7(存储降级)**:推迟条件改为"存在未到达的 MarginStress/Hard 引擎目标",由新视图方法 `ModeView::engine_reduce_pending` 提供;新增 `ModeInput::StorageRecheck` 与 `drain_deferred()`,保证目标到达的同一条输入内发起 Drain(RSK-221、RSK-222、RSK-281)。
2. **mode 模块**:
   - 原文用到却没定义的 `ModeView`,现在写出了全部方法(RSK-053、RSK-077)。
   - 拒绝顺序写明;新增 `ModeErr::UnknownPending`;`Resume{Recon}` 条件不满足时返回 `ReconNotClean`(RSK-077、RSK-202)。
   - `is_upgrade` 写明取值(RSK-078);`kill_changed` 写明含义(RSK-140)。
3. **日亏**:Resume 后由 `ModeOutcome.daily_loss_resumed` 与 `Risk::on_daily_loss_resumed` 重设触发线;锁存期间 `on_pnl` 返回 None;当日补取权益时额度重设(RSK-190–RSK-192)。
4. **检查链**:
   - 拒绝只带一位;#3 按来源取位的优先级(RSK-024、RSK-045)。
   - IOC 越价与风险递减单越价的公式;ReduceOnly 状态的 inst 只放行风险递减单(RSK-021、RSK-028)。
   - Open 仓位的计算式;Reduce 超单笔上限给 `NOTIONAL_ORDER`;权益无效时账户名义只用绝对值(RSK-032–RSK-034)。
   - 新增 `Risk::notional` 作为名义的唯一实现(RSK-031)。
5. **令牌桶**:每笔 commit 扣三个桶各 1 个;inst 桶按 (account, inst)、无预留;退避次数的递增与清零规则(RSK-036–RSK-038)。
6. **启动闸门**:
   - 七个位的数值(1/2/4/8/16/32/64)。
   - 每次运行在 `RecoveryFlags` 时复位,从快照起步不复位(RSK-109);这同时修掉了原文 `set_run_start` 清零 `us_up_seq` 会让快照回放与实盘不一致的问题(RSK-203)。
   - `Gate(AccountCfg)` 改为携带报告头与 `keys_required`(RSK-101、RSK-103)。
   - 模拟账户的对冲闸门子集只看 sim-gw 连接,原文里它会永远等不到 kill 自检(RSK-101)。
   - 新增 `ModeCfg` 传入两个开关(RSK-091、RSK-101)。
7. **干净重启**:`RecoveryFlags` 定义码值并新增 `CLEAN_DRAIN` 位(条件 ①–③ 由 sequencer 判定);开关关闭时不写 `RecoveryHeldSim`(RSK-091、RSK-093)。
8. **cap 记录**:
   - 各来源的 `cflags`/`level`;Kill 的 `params[0]` 位;DailyLoss 的 `params[0]` 为触发线(RSK-070)。
   - 合并后记录有变化按一次设置处理;`ctl_state_version` 每次有效变化恰好 +1(RSK-071、RSK-076)。
   - 不限账户的 cap 投递给哪些账户(RSK-075)。
   - `loosens` 按字段列出放宽方向;原文"阈值变小即放宽"对 `naked_gate` 等字段是反的(RSK-006)。
9. **保证金**:
   - 进入 L2 时设截止;`on_margin` 也会发现截止已过;逐档下降的规则(RSK-162–RSK-165)。
   - MarginStale 截止 +1 ns,原文在截止那一刻会原地重复登记(RSK-167)。
   - 重启后进入 L3 同样要本次运行的数据确认才出步(RSK-169)。
   - 多个永续 venue 时 `margin_stale_ms` 默认取大(RSK-001)。
10. **杠杆**:原文 `on_groups_changed` 产生的 cap 无法生效;现在由 `ModeView::lev_bad` 读取,sequencer 先调 `on_groups_changed` 再 `apply(GroupsChanged)`(RSK-181)。
11. **对账**:
    - `ReconGateRowHdr` 加 `view_seq`,24 B → 32 B,按 RF1 不升版本(RSK-131)。
    - 名义取条目的 |`diff`|;定义 `ReconVerdicts`。
    - PREVIEW 改为列出条目键与差额,不再列"名义":它没有数据来源(RSK-079)。
12. **下架**:原文前后矛盾,统一为 venue 级 cap,不改 cell 的有效模式,只撤该 inst 的 Open 挂单(RSK-060、RSK-141、RSK-223)。
13. **其他**:
    - `on_rate_limit_hit` 返回 `Note{RateLimitHit}`(RSK-038)。
    - `RiskParamError::Invalid`(RSK-002)。
    - `restore` 的错误条件(RSK-009、RSK-231)。
    - `on_legs` 的缓存在从快照起步时重建(RSK-272)。

## 3. 留给别人

- **T11(hft-sequencer)**:
  - **必须做,否则规格不自洽**:
    - 从快照起步时对每个 group 调一次 `risk.on_legs`,否则快照回放与实盘不一致。
    - SetParams 时先调 `risk.on_groups_changed` 再 `apply(GroupsChanged)`,否则杠杆超限禁不了开。
  - **新接口要接上**:
    - 实现 `ModeView` 适配器。
    - 给 Risk 传 `ModeCfg`。
    - 恢复结束时置 `CLEAN_DRAIN` 位。
    - 处理 `daily_loss_resumed` 与 `StorageRecheck`。
    - `Gate(AccountCfg)` 带上 `keys_required`。
    - 追加 `on_rate_limit_hit` 返回的注记。
    - 用 `Risk::notional` 投影名义。
  - **映射与措辞**:
    - `UnknownPending` 映射为 `PreconditionFailed`;`Invalid` 映射为 `InvalidArgs{ParamInvalid}`。
    - 第 636 行"提供 exec.drain_ready() 的读数"与第 813 行测试措辞按 RF7 改。
- **T7(hft-exec)**:提供只读的 `engine_reduce_pending()`,口径同 `drain_ready_acct` 里的那一项。
- **T15(control-plane)**:登记 detail `UnknownPending`;第 1 层"有引擎强制减仓或清仓锁时推迟"按 RF7 改。
- **T2(hft-event)**:Kill cap 的 `params` 位值链接到 hft-risk 的 cap 表;`RateLimitHit` 的 `v[]` 布局链接到 hft-risk。
- **T3(journal-format)**:`GateRow.recovery_flags` 的注释补 `CLEAN_DRAIN`,码值链接到 hft-risk。
- **T22(testing)**:第 147 行的 StorageCritical 推迟条件按 RF7 改。

## 4. 合并遗留(round8f-merge-notes)

- 删掉"不越过交易所价格带由 OMS 定价保证"(J4/J6 → J5):已一致,原句已不存在。
- 撤单持续义务只限 `cancel_resting = true`,cap 表补 Draining 的 `params`:已一致。
- 按 venue 的放行报告 `venue_gate_open`(J4 → J5):已一致,补了用例 RSK-107。
- `resolve_broken` 只在删除带 HARD 的 Kill 时产出:已一致,用例 RSK-055。
- J5 §5-1(清仓锁会把 StorageCritical 推迟到 journal 写满):已改,按 RF7。
- J5 §5-5(max_insts 用尽只能靠标记价断流告警):已一致,本组件无需改。
- F4(b) `RecoveryHeldSim`、F6 `uid_policy`(RF4):已一致,另补清了开关关闭时的行为。

## 5. 需要负责人决定(测试仍覆盖不到)

1. 下架是否应把相关 cell 的有效模式降到 CloseOnly。我按原文中 venue 级 cap 的写法定为不改有效模式;改成要降需要 mode 知道每个 group 的腿。
2. AccountCfg cap 的原因位只增不减:只修好其中一项时,`params` 仍显示两项原因。
3. 已知限制里的"cap 容量 256 条,作用域爆炸时合并为账户级"没有可执行的规则,也没有用例。
4. `margin_ratio` 在 `view()` 里取哪个 venue 的值未定义,用例没有断言它。
5. 跨账户的链接腿,我让账户级 cap(如 MarginStress)经 `venue_level` 挡第一腿;这一点请确认。

## T10

hft-ledger 和 hft-sim 的 `TESTS.md` 都已写好。三项检查在我名下的文件上都是 0 问题:`check_tests` 只剩两条覆盖表"非规范"警告(`规划源文件`、`11. 源文件`),`check_hft` 在这四个文件上无问题无警告,`heading_audit` 丢失标题为 0。上次限流中断前没有写入任何文件,这次从头完成。

## 1. TESTS.md 与用例数

| 文件 | 合计 | unit | table | prop | golden | fuzz | bench | dst | integ | replay | host | testnet |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `crates/hft-ledger/TESTS.md` | 189 | 118 | 44 | 11 | – | 1 | 9 | 3 | 1 | – | 1 | 1 |
| `crates/hft-sim/TESTS.md` | 108 | 45 | 38 | 6 | 2 | – | 4 | 3 | 4 | 1 | 3 | 2 |

两份 README 的"测试与验收"都只留标题加一句链接,原来就没有 `###` 子标题。

## 2. 规格改动

**hft-ledger README**
- **RF8 消费**:新增 `external_from_oms_note` 和"外部成交"第 7 步,冷读者按 `Note{OmsForeignFill}` 入账;冷读者不再按普通 `Note{ExternalFill}` 入账。暴露用例 LED-075~080。
- **补上只有名字没有定义的类型**:`PnlDelta`、`BalanceVerdict`、`BaselineResult`、`RateView`、`LedgerConfigError`。暴露用例 LED-003、040~046、090、210。
- **`NetAdjust.delta` 口径**:改为"该腿有符号持仓的补差",与 OMS 的 `leg.net += delta` 对齐。暴露用例 LED-012/013/015/016。
- **平均成本法**:移出成本 = `cost × q / |Q|`,一次相除、向零截断,全平时恰好清零;写明 `cost` 与已实现的指数。暴露用例 LED-031/032。
- **资金费**
  - 合成额只取整一次(收入向下、支出向上),持仓为 0 返回 `None`。暴露用例 LED-093/094。
  - 合成后到时,由 ledger 自己写 `FundingMismatch`。暴露用例 LED-098.5。
  - 待配对表满时的淘汰规则。暴露用例 LED-102。
  - `hist_from_ns` 的三种取值规则。暴露用例 LED-136~138。
  - 无历史挂起表满时,返回被淘汰那一项的 `Corrected`。暴露用例 LED-134。
- **保证金**:比率按 HalfEven 取到 1e−9;无报告时估计值保持 `Rate::MAX`。暴露用例 LED-150/154。
- **轮次**:写明腿名义 = |净量| × 估值价。暴露用例 LED-200~202。
- **换算率**:= `mid_floor(bid, ask)`;过期按"最近一次入口调用的 t"判定。暴露用例 LED-210/212。

**hft-sim README**
- **RF9**:新增"重启恢复"一节,含状态所在快照节、4 步恢复、`SimSeedState` 定义;删去原开发备忘里的对应开放问题。用例 SIM-160~169。
- **回报洪峰**:每次 `poll_once` 至多取 256 条指令;待交付堆剩余 < 4096 时只交付、不取指令不推进;维护类指令丢弃并计数。暴露用例 SIM-105、145~147。
- **常量与配置**:`SIM_CODE_BASE = 0x1000_0000`,每个拒单原因配一个码(n);`SimSetup` 新增 `tsc_hz_milli`,用于 µs/ms 参数换算到事件时间。暴露用例 SIM-002/010/090。
- **撤、改、查、CancelAll 的回报**:订单表 1024 项;`MarginReport` 推送节拍改为"比率绝对变化 ≥ 0.01 或距上条 1 s"。暴露用例 SIM-076、100~104。
- **取整与锁定**
  - 手续费取整一次、平台币抵扣的折算方法。暴露用例 SIM-050。
  - 资金费取整与"仓位为 0 不结算"。暴露用例 SIM-061。
  - 强平价的取整方向、触发判定(`maint ≥ margin_balance` 精确比较)、强平成交价的取整。暴露用例 SIM-071/072。
  - 挂单锁定额的定义。暴露用例 SIM-080/081。
  - 记忆环满时覆盖最早一项。暴露用例 SIM-035。

## 3. 留给别人
- **T16 hft-core**:启动步 5 之后、步 6 启动 sim-gw 之前,按 hft-sim"重启恢复"调用 `seed_account_from`;装配 sim-gw 时填 `SimSetup.tsc_hz_milli`。
- **T17 replayer / reconciler**
  - replayer:起点 seed 改为链接"重启恢复";`tsc_hz_milli` 取 journal 的 EngineClock 行。
  - reconciler 的引擎视图:增加对 `Note{OmsForeignFill}` 的消费。
  - SIM-193~196、205 的实现落在 replayer 的 tests 里。
- **T21 hft-py**:§2 加 `OmsForeignFill` 的消费;LED-246 的实现落在 hft-py 的 tests 里。
- **T3 hft-refdata**:`AssetMeta` 没有资产 `dust` 字段(ledger、risk、reconciler 都在用);也没有分档维持保证金率和价格限制带字段(SIM-071.4、SIM-188 需要)。
- **T11 / T8**
  - 建议 sequencer 直接用 `external_from_oms_note` 构造该笔外来成交,做到单一实现。
  - `balance_check_quiet_ms` 的边界用例归 sequencer。
- **T22**:benchmarks 还没有登记 `ledger/*`、`sim/*` 这些基准名。
- **T4 testkit**:需要提供分配计数器,以及 SimActor 的崩溃注入(供 SIM-167~169 用)。

## 4. 合并遗留处理
- J5 §5-2(ForeignFill 冷读者看不到):**已改**,即上面的 RF8 消费。
- J8 §5-1(sim 重启后模拟真值从哪来):**已改**,即上面的 RF9。
- J5 §5-3(`auto_held` 不过期):**已一致**,已写在已知限制里,LED-059 覆盖抵扣条件。
- `LedgerAutoHeldOffset` 的登记、节 13 v7、`#外部成交externalfill` 锚点保留、`none_qty`、"被全部抵扣时不调执行层":**已一致**。
- J8 F4(b) 的 CoreRestart 措辞、J9/J11 冷读者按第 6 步消费:**已一致**。
- J9 §5-3(MS-2 的"不利方向"):暂缓项,没改语义,按现文写用例。

## 5. 需要你定的地方
1. **冷读者怎么得到普通成交和费用**:`FillDelta`/`FeeDelta` 没有 Output 记录,hft-py 和 reconciler 都说"经 ledger 的 journal 消费路径",但这条路径对常规成交从没定义过。这次只定义了外来成交三类(Input、AUTO_RECON 注记、`OmsForeignFill` 注记)。
2. **轮次 `tag` 没有来源**:`FillDelta` 和 OMS 槽都不带 tag,`RoundClosed` 的 tag 无法赋值。用例目前不断言 tag。
3. **RF9 的代价**:按"恢复后的 ledger 视图"恢复模拟真值,会抹平重启前真值与账本之间尚未报出的偏差;停机期间跨过的资金费结算不补。
4. **refdata 缺字段的后果**:若不加分档维持保证金率和价格限制带,价格带检查永远跳过,压力用例永远报 `MmrTiersMissing` 拒绝运行。
5. 链上腿和 `queue-hftbacktest` 挂单模型的用例标为"里程碑外",等各自启用时再补写。

## T11

T11 已完成:`crates/hft-sequencer/TESTS.md` 已写好,README 的规格缺口已补。简报 §1 的三个检查对我名下两个文件都是 0 问题、0 告警(check_tests、check_hft、heading_audit 均无命中;heading_audit 为 lost headings 0)。

## 1. 写了哪些 TESTS.md

只有 `E:\trading\hft\crates\hft-sequencer\TESTS.md`,共 348 个用例,表驱动行共 757 行。

| 级别 | 用例数 |
|---|---|
| unit | 155 |
| table | 141 |
| bench | 23 |
| prop | 13 |
| dst | 8 |
| integ | 7 |
| host | 1 |

- **里程碑**:M2 346 个,M3 1 个(SEQ-316 稳态零系统调用,host 级),M4 1 个(SEQ-243 sim 实例的引导块)。
- **README**:原"测试与验收"整节已迁入 TESTS.md,只留一句链接;该节下没有 `###` 子标题。

## 2. 规格改动(都在 `crates/hft-sequencer/README.md`)

- **环与容量**:补了本 crate 对 ECF1 段 3、段 5 的取值校验规则,不合法时报 `ValueRange` 或 `Structure`(SEQ-003、SEQ-004)。`CapKind` 写出五个变体及检查顺序(SEQ-006)。
- **主循环**:
  - 明确 a 段的交出顺序(SEQ-055)。
  - md 时间片改为"已过去 ≥ `md_round_ns` 就停",边界唯一(SEQ-066)。
  - 统一"在空闲轮写"的含义:上一轮空闲、本轮开始时环全空、`wants_continue()` 为假(SEQ-082、SEQ-085、SEQ-090)。
- **环境决定表**:
  - 只有整点滚动避开资金费结算窗口,窗口定为 [结算时刻 −120 s, +120 s) 的半开区间;按段用量触发的两级滚动不避开;所有触发都等续推完成(SEQ-082–SEQ-084)。这回答了 J11 留下的问题。
  - 段滚动时 `roll` 返回 `NextNotReady` 的处理与回放复现(SEQ-244)。
- **处理顺序与意图管线**:
  - 撤单、改单、新单都传 `TrigStamps`,撤单和改单也写 `decided_tsc`(SEQ-126、SEQ-127)。
  - 外来成交只对量非 0 的部分调执行层和投递事件;返回 `None` 时一概不调。补了 RF8 一句(SEQ-151–SEQ-155)。
  - `link_notional` 原来写 `mid × (1 ± rr_max_cross)`,对卖出的第二腿不是上界,改为两个方向都用 `(1 + rr_max_cross)`(SEQ-176、SEQ-177)。
- **Advice 路由**:GroupId 高 8 位与实例不符 → `AdviceMisrouted`;查不到 group → `AdviceStale`(SEQ-188)。
- **存储降级**:按 RF7 改写推迟条件;sequencer 在视图里提供 `exec.drain_ready()`(SEQ-277)。
- **冷启动恢复**:删掉 epoch 公式副本,改为链接 hft-core 启动步 5;`RecoverPlan.epoch` 是持久化的旧值;新增 `view().engine_epoch()`(SEQ-263)。
- **命令路径**:
  - "Live 实例"的定义改为只链接 control-plane(RF11;SEQ-283)。
  - `RefdataRecheck` 的 arg 取值与多项不满足时的优先级写明(SEQ-304)。
  - 补了 `CancelAll`、`Annotate` 在 sequencer 侧的处理(SEQ-348)。
- **定时器**:周期性系统定时器重启时替换同键的旧条目(SEQ-239)。

## 3. 留给别人

- **T15(control-plane)**:
  - "类别"第 1 条现在把 Live 实例定义为"按账户类型、不看期望模式",与 RF11 相反,需要改。
  - 401 `RefdataRecheck` 要补"容量不足时 arg = 0"。
- **T9(hft-risk)**:
  - `ModeView` 要提供 `drain_ready` 读数,并写明推迟结束后在哪个 `apply` 产出 `drain`。
  - 第 253 行注释仍是"清仓锁或 L3 期间推迟",与 RF7 不符。
- **T7(hft-exec)、T8(hft-oms)**:`recovery_end` 注释写"不重放尾部时 `stop_tsc` = 快照头 `snap_tsc`"。`snap_tsc` 是测量字段,应改为"所选快照所在记录的记录头 tsc"。
- **T7(hft-exec)**:PairOnFill"第一腿量"的第二腿名义估算同样有 `±` 的问题,对卖出的第二腿不是上界。
- **T2(hft-event)**:
  - 登记表里有 `Note{ConfigDiffers}`(Sequencer 源),全仓没有任何产生规则,需要删除或补规则。
  - Advice 一节"不属于该实例 → AdviceMisrouted"应写明是"高 8 位不符"。
- **T3(journal-format)**:节 15 一行里的"最近 4096 个 cmd_id"是容量值的副本,应改为链接。

## 4. 合并遗留的处理结果

- **已改**:
  - J4 的 `TrigStamps` 与撤改单的 `decided_tsc`。
  - J5 对 J4 留的 `none_qty` 另调 `on_external`(补写了量为 0 时不调)。
  - J6、J10 的"Live 实例"定义(按 RF11 改为链接)。
  - J11 的 epoch 公式副本,以及软阈值滚动是否避开结算窗口。
  - RF7、RF8 在 sequencer 侧的部分。
- **已一致**:
  - J2 的三项(Model 不收 External/Risk/Mode/Conn、`prev_interval_s` 与 Funding 可见性、模型分片链接 SDK)。
  - J4 的 `on_venue_gate_open`、`resolve_broken`、`drain_ready` 与 `drain_ready_acct`、`auto_held` 全部抵扣时不调执行层。
  - J6 的 `WriteOffResidual` 不再要求 Broken。
  - J8 的 `CkptHdr`、feedctl 续推、关键帧事件数与回放吞吐的尾部约束(SEQ-001.17 对它做了断言)。
  - J11 的 `Sys{Roll}` 两级、"每段"措辞、保留锚点。
- **无法改**:无。

## 5. 需要你定的地方

1. 定时器堆满(运行中超过 `max_timers`)时怎么处理,规格没写,目前没有用例。
2. "输入环满的轮次实盘驱动按空闲处理并带超时 park"这句含义不清,目前没有用例。
3. rep、bbo 多个环之间的轮转粒度要在 M2 动工前定;现在的用例只断言每环和合计上限,不断言环之间的先后。
4. `link_notional` 改为两个方向都用 `(1 + rr_max_cross)`,会让风控 #7b 对卖出第二腿的资金检查更保守。这是资金相关改动,需要你确认。
5. SEQ-277(存储危急的 Drain 被推迟)的期望要等 T9 写出推迟结束后的产出时机才能定成具体某个 Input。

## T12

T12 已完成:两份 `TESTS.md` 已写好,两个 README 的"测试与验收"一节只留一句链接(该节下没有 `###` 子标题)。三项检查在我名下文件上都是零问题:`check_tests.py` 和 `check_hft.py` 都没有报到这四个文件(`BUDGETPCT` 从 7 降到 0),`heading_audit` 显示丢失标题 0。

## 1. 写了哪些 TESTS.md

| 文件 | 用例数 | 按级别 |
|---|---|---|
| `E:\trading\hft\crates\hft-net\TESTS.md` | 203 | unit 83、table 59、integ 34、bench 12、host 7、prop 5、fuzz 3 |
| `E:\trading\hft\crates\hft-venue-proto\TESTS.md` | 102 | unit 51、table 46、prop 3、bench 2 |

- **里程碑**:hft-net 中 M1.5 纵切片要用到的部分(收发、时间戳、Connector、ReadyPoll)填 M1.5,连接池和 REST 填 M3,三项可选扩展填 M7。hft-venue-proto 中 `RxMeta` 布局和契约 3 填 M1.5,其余填 M3。
- **契约用例的做法**:hft-venue-proto 内置一个遵守全部契约的假协议,外加"只违反一条契约"的变体。每条契约 1–27 至少一个正例(检查函数对假协议通过)和一个反例(检查函数检出变体)。

## 2. 规格改动(都在我名下的两个 README)

**hft-net README**
- **读路径**:`poll_rx` 每次至多一次 `recvmsg`;空闲空间为 0 时不调用(零长度读会被误判为 EOF)。原测试表写"恰好读满时再读一次",与"读满即缓冲满"矛盾,一并改正。暴露用例 NET-080–082、NET-098。
- **接口与错误类型补全**,补了可以直接写成断言的结果:
  - `Conn::mark`、`next_mask`、`last_errno`、`ConnStats` 的字段;
  - 新增 `SendErr::NotSealed`;`TxBacklog` 同时置 `TX_BACKLOG`;新增 `ErrBits::HTTP_PROTO`;
  - `MaskedFrame` 的 CAP ≤ 65 535,误用时 debug_assert;
  - 掩码键按大端字节写入,Ping 载荷按小端 8 字节。
  - 暴露用例 NET-009、NET-060–066、NET-101、NET-157、NET-161。
- **WS 解帧**:载荷 1 B 的 Close、孤立的 continuation、分片进行中又来新消息,都置 `WS_PROTO`;Pong 队列溢出时只保留最后一个;热线程上的相位迁移 Open → Closing → Dead。暴露用例 NET-028、NET-031–036、NET-098。
- **时间戳**(§7):
  - "确认为空"的时刻只前移不后退,`install` 时清零;
  - cmsg 截断时两个戳都记 0;
  - 帧跨多次读取时,`kernel_to_user` 含等待帧收全的时间。
  - 暴露用例 NET-120、NET-132、NET-133。
- **ConnPool**:
  - 新增 `new(mode, target, policy)`;`next_build` 改为返回 `BuildWhy`,优先顺序 Hint > Rotate > Fill;
  - 新增 `build_failed`;`retire` 改为返回 `Result`;`rotate_due` 到期时置 `ROTATE_DUE`;
  - 写明错峰提前的公式;退役 primary 时 standby 升为 primary。
  - 暴露用例 NET-240–254。
- **Connector**:
  - 定义 `BuildEvent`、`TlsInfo`、`BuildFail`、`BuildErr`;`handoff` 返回 `Result`,`conn_mut` 返回 `Option`;
  - 写明超时判定为"≥"以及与 `deadline_ns` 先到者生效;等票据 20 ms 的规则;`napi_ifindex(0)` 直接返回 None。
  - 暴露用例 NET-190–213。
- **RestPool**:
  - 定义 `RestErr`、`RestFail`;`poll` 改为返回 `Option<Result<…>>`;新增补建与退役接口;
  - 写明 204/304、无长度、TooLarge、Timeout、Closed 各自的处理,以及失败后连接必退役。
  - 暴露用例 NET-260–275。
- **ReadyPoll**:就绪超过 64 条留到下一次;出错返回 0;`NetErr::Os`;EEXIST/ENOENT。暴露用例 NET-224–226。
- **配置**:由 `NetConfig` 承载默认值;UserStream 的 `rx_buf` 定为 64 KiB(原来没写)。暴露用例 NET-300、NET-302。
- **测试设施**:`sock.rs` 增加 feature `test-io`(脚本化 I/O 与系统调用计数),供全部 ScriptIo 用例使用。

**hft-venue-proto README**
- **RF14**:统一门槛表改成 p50/p99 两列,BBO 解码定为 0.4 / 0.8 µs,`classify` 定为 30 / 60 ns,venue 的 p50 和 p99 都不得宽于门槛。另写一条与 latency-budget `ingest` 的核对规则:BBO 路径三项之和不超过 ingest 目标 p50 的 1/3、p99 的 1/5;深度路径分别为 1/2、1/3。现值都满足。暴露用例 VPR-131、VPR-132。
- **契约 3**:`RxMeta.hw_rx_ns` = `best_rx()`;`conn` 写进 `MarketHdr`;实现方不做 HW → SW 的退化,也不写记录头标志。暴露用例 VPR-066–068。
- **契约 4**:`FxErr` 到 `DecodeErr` 的映射。暴露用例 VPR-069。
- **契约 8**:数字串可带前导 `-`,超出 i32 范围按非数字处理;补了 `code.rs` 的函数签名。暴露用例 VPR-020。
- **契约 17**:保证金率向上取整。暴露用例 VPR-104、VPR-105。
- **去重**:`dedup_key` 结果为 0 时取 1;键 0 → debug_assert。暴露用例 VPR-030、VPR-034。
- **类型**:新定义 `ProtoErr`(签名里在用却没有定义);各错误变体加注释;`dead_man_refresh` 在 `dead_man() = None` 时返回 `Unsupported`;`RateRule` 的窗口和上限 > 0。暴露用例 VPR-073、VPR-087、VPR-093、VPR-097。
- **testkit**:写明 `VecSink.exch_id` 语义、`GoldenDriver` 的编码类比对与差异报告、`ContractFail` 结构;新增 8 个检查函数,并扩展了 4 个既有检查。暴露用例 VPR-040–123。
  - 新增:`check_chain`、`check_md_basics`、`check_hot_alloc`、`check_session`、`check_cid`、`check_funding`、`check_margin`、`check_bbo_refresh`。
  - 扩展:`check_unknown_codes`(契约 6)、`check_fee_asset`(FeeOnly)、`check_default_caps`、`check_topn`。

## 3. 留给别人

- **T13(hft-venue)**:按上面的 hft-net 新接口核对 §2、§4、§5、§7 的用法——`next_build` 返回原因、`RestPool::poll` 返回 `Result`、`handoff` 返回 `Result`、`Conn::mark`。§2 第 5.4 步要写明:`RxMeta.hw_rx_ns` = `best_rx().0`,记录头 `HW_TS`/`SW_TS` 由运行时按 `ts_flags` 置位。
- **T14(venue-\*)**:`tests/contract.rs` 要调用新增的 8 个检查;venue-binance:453 现在列的是旧检查名单;Binance 的 `decode_snapshot ≤ 80 µs` 没写分位数。
- **T2(event-model:30)**:"确认为空"的归属应链接 hft-net §7(RF12)。
- **T22**:
  - testing.md:379 的"取法归 hft-venue"应改为链接 hft-net §7;
  - 在 benchmarks 登记基准名:12 个 `net_*`,以及 `bench_trade_id_ring`、`bench_dedup_key`。
- **T4(tools)**:
  - `doc-check` 要实现 VPR-130–132(必备节含两个产品线、预算 p50/p99 逐项比较、ingest 核对);
  - `xtask ci` 的加密白名单扫描要导出成可调用的函数(NET-164 用)。
- **T1(hft-rt)**:需要一个固定频率的 `TscClock` 测试构造函数(例如 `from_hz`),时间戳类用例依赖它。

## 4. 合并遗留(M0-f)的处理

| 条目 | 结果 |
|---|---|
| J6 → J7:写明"确认为空"的判定;多包时 `hw_rx` 取最早一个包 | 已一致(§7 已写),另补了"只前移"和交接清零 |
| J11 矛盾 2:hft-net 原写"最后一个 skb" | 已一致(§7 现为"最早一个包所在的读取") |
| J9 → J7(venue):三家 BBO p99 0.8 µs 宽于 0.5 µs | 已改(RF14 门槛表,各 venue 现值都不宽于门槛) |
| J12:`BUDGETPCT` 7 条留给 J7 | 已改(现为 0) |
| J7 矛盾 5:跨多次读取的帧,`kernel_to_user` 含等待时间 | hft-net §7 已注明;latency-budget 的定义是否加注归 T23 |
| J7 矛盾 3:`gw.rest_idle_check_ms` 适用范围 | 已一致(RF13,hft-net 只链接该键) |
| X3 P1-27:netlink 需要 `AF_NETLINK` | 已一致 |
| J10:ADR rx-irq-driven 中的 NAPI 说法;J7 的 SetEpoch、snap-fetch 源地址 | 无法改:不在我名下(分别归 T23、T13) |

## 5. 需要负责人决定的地方

1. **可选扩展的里程碑**:RSS、TX 时间戳、scratch 预热的用例填了 M7。到了 M7,`test-trace` 会要求这些功能必须有实现,即使从未启用。要么接受"M7 前实现但默认关",要么给"未启用的可选扩展"一个豁免办法。
2. **CI 上的回环测试权限**:NET-205、NET-207 在 `lo` 上设 `SO_BINDTODEVICE`。CI runner 是否需要 `CAP_NET_RAW` 取决于开发备忘第 7 条的核实结果;如果需要,这两条只能改成 host 级。
3. **契约 10 的 `ExpireAt` 声明**:"只有过期字段的生效位置已核实才可声明 `ExpireAt`"这一条无法机检,仍靠来源等级加评审把关。
4. **HW 戳、NAPI 等主机事实**:NET-370–375 只在 M1.5 探针或 dev2 上能做。结论如果与规格不符(例如握手后 napi_id 为 0、短读并不意味着队列已空),要按开发备忘的备选方案改规格和用例。

## T13

# T13 最终报告(M0-g,hft-venue)

`crates/hft-venue/TESTS.md` 已写完。三个检查在我名下的文件上都是零问题:`check_tests` 无问题、无告警;`check_hft` 中我的四个文件没有问题项,也没有告警;`heading_audit` 显示丢失标题为 0。

## 1. 写了哪些 TESTS.md

`E:\trading\hft\crates\hft-venue\TESTS.md`,共 253 个用例:

| 级别 | 用例数 |
|---|---|
| unit | 99 |
| table | 95(合计 406 行子用例) |
| bench | 20(延迟表中的每个基准名各一条) |
| integ | 19 |
| prop | 10 |
| host | 6 |
| testnet | 4 |

- **里程碑分布**:M1.5 有 106 个,M3 有 145 个,M6 有 2 个。
- **夹具**:桩协议 `StubProto` 与脚本化连接 `ScriptNet`,都放在 hft-venue 自己的 tests 里,不依赖任何 `venue-*`。
- **README 迁移**:"测试与验收"一节已整体迁入,现在只剩一句链接;该节下没有 `###` 子标题。
- **可选扩展"增量深度"**:没有写成用例,因为写成用例就得在 M3 实现。我在"验收"节列出了启用前必须补齐的用例清单。

## 2. 规格改动(都在 `crates/hft-venue/README.md`)

- **出环表**
  - `SetEpoch` 增加 `prev_epoch` 字段,gw 不再按 `epoch − 1` 推算上一 epoch(VEN-214)。
  - `CancelAll` 先按账户级编码,venue 返回 `Unsupported` 时改为按 inst 逐个发(VEN-206)。
  - 命令的账户不归本 gw 时回 `NoConn`(VEN-241)。
  - `SessionKeepalive` 失败时报 `Degraded{UserStream, DEADMAN}`(VEN-237)。
  - `MirrorRestore` 中"镜像满"明确为"槽号 ≥ OMS 槽数"(VEN-238)。
- **运行时入口**
  - 新增 `InitErr` 枚举,共 9 个变体。
  - 新增 `ready_poll_on`/`ReadyPollMode`、`feed_cmd_dest`/`FeedCmdDest`。
  - 新增三个常量:`FEEDCMD_PER_POLL`、`FRAME_EVENTS_MAX`、`SNAP_BBO_BATCH_MAX`。
  - 对应用例:VEN-010–017、023、069、152、162。
- **配置项**
  - 计数或时长类键取值为 0 时,构造返回 `InitErr::Cfg`。
  - 轮换余量 ≥ 交易所强断时长时返回 `RotateMargin`;`id_table_cap` 与 `margin_poll_min_ms` 不满足时也返回 `InitErr`。
  - 等待切换超时的语义改为"放弃本次,隔同样时长重试"。
  - 在途请求超时:每个请求只报一次,并移出在途计数。
  - 回报解码失败的 60 s 窗口语义写明。
  - 未知事件写 Note 的规则写明:第 1 条与第 k × N 条各写一条。
  - 写明 `raw.sample_ms` 的边界。
  - `bbo_conns = 1` 时不报 `RACE_SINGLE`。
- **§1、§2**
  - `Subscribe` 改为按流集合差分订阅;未知 inst 和不属于本任务的命令计数后丢弃。
  - 环满时只停读输出到满环的那些连接;取帧前检查环空位。
  - **矛盾修复**:原文写"rep 环满时出环照常取",但每条命令都要往 rep 环写一条 `SendReport`。现在改为 rep 环预留 `cmds_per_loop` 个空位,空位为 0 时不取出环。
  - **矛盾修复**:BBO 帧数上限改为"每条连接各至多 N 帧",与原测试表和延迟上界一致。
  - 订阅节奏的计算公式写明。
  - 赛马替补收到首帧后,退役同组最早安装的连接。
- **§3a(修了一个缺陷)**:原规则下,补帧期间同 `update_id` 的报价会被去重吞掉,REST 补帧返回 `update_id = 0` 的也会被丢,所以补帧可能永远结束不了。现在:
  - 补帧期间改用 `update_id ≥ last_fwd` 放行。
  - `update_id = 0` 的直接转发,且不改 `last_fwd`。
  - 补帧已结束后迟到的 snap 记录丢弃并计数。
  - `Alive` 的判定改用回帧的 `rx_tsc`;静默计时从连接安装时起算。
- **§3b**:批量合并遇 `BufFull` 时条数减半重试;新增状态 `ReqErr = 7`。
- **§4**
  - 带 `RISK_REDUCING` 的 Amend,若同批更早有其目标槽的 Place,则留在原位。
  - `WriteError` 的 Place 同样写槽镜像;Ack 时把交易所 id 写进槽镜像。
  - standby 致命时报 `Down`;下单与用户流连接交接后报 `Up`。
  - `Rotated` 的字段写明。
  - `CLOCK_SKEW` 与 `RATE_LIMIT` 改由 `ReportOutcome` 的提示触发。
- **§4a**:Push 型兜底查询的期限;`every` 按 `poll_no % N == 0`;退避为 2ⁿ⁻¹,`Degraded` 只推一次;超大或超时的响应只交回带请求元数据、体为空的 Raw 记录。
- **§4b**:放弃切换时对新连接发退订。
- **§6**:`tomb` 满而头部未到期时,新到终态的条目直接删除;超过 36 B 的交易所 id 不入表。
- **§6a**:写明重建窗口的边界。
- **§7**:UID 查询为"共至多 N 次(含第一次)"。

**cookbook `adding-a-venue.md`**
- 改动边界加上 docs/tests 前缀表与 fixtures 目录;crate 目录树加 `TESTS.md`。
- 基准门槛改为链接 hft-venue-proto 的统一门槛表,删掉手抄的旧数字(RF14)。
- 金样期望改为规范 JSON,与 fixtures 一致。
- 验证项改为指向各 venue 的 `TESTS.md`。
- `uid_policy` 取值改为小写 `"unverifiable"`(RF4)。

**`fixtures/README.md`**:登记 `params/sdk-tp/`(含 `schema.golden.json` 与 `.meta.toml`)和 `strategies/sdk-probe/ckpt/`。

## 3. 留给别人

- **T2(hft-event)**:在 `GwCmd.arg2` 里编入 `prev_epoch`,建议 `accept_prev | prev_epoch << 16`。
- **T11(hft-sequencer)**:`SetEpoch` 要填 `prev_epoch` = 快照节 3 的 epoch;SEQ-263.1 的期望要补上这个字段。
- **T12(hft-venue-proto)**:
  - `ReportOutcome` 缺"时钟越界 / 交易所限速"的提示字段;hft-venue §4 和 scenarios 的 FP-8 都依赖它。
  - trait 缺生成 `SessionKeepalive`(listenKey 续期)请求的方法。
  - 契约 24 要写明:输入的 inst 数超过本所单次可查数量时,返回 `BufFull`。
  - `VenueCfg` 里应有过期窗口字段(承载 `gw.stale_guard_ms`)。
- **T16(hft-core)**:`InitErr` 对应的退出码(建议 2);装配时调用 `ready_poll_on` 和 `feed_cmd_dest`;`FeedCfg.max_sub_insts` 的来源。
- **T4 / T22**:cookbook 第 6 步和 FP-8 都引用了"hft-testkit 的 VenueProtocol 级假交易所 + hft-venue 运行时",但 testkit 不依赖 hft-venue,这个夹具没有定义。建议 FP-8 像 C12 一样放到 `apps/hft-core/tests`。
- **T14**:只能按单个 inst 查 BBO 补帧的所(Binance USDⓈ-M、OKX)需要对多 inst 输入返回 `BufFull`。

## 4. 合并遗留的处理结果

| 条目 | 结果 |
|---|---|
| J4→J7:`SetEpoch` 要带上一 epoch 的具体值 | 已改(hft-venue 部分);hft-event 与 sequencer 部分见第 3 节 |
| J8→J7:snap-fetch 复用第一条 `stream = md` 的 `src_ip` | 已一致,另补 VEN-035 |
| J8→J7:hft-core 把 prod 的 `raw_dups` 设为 false | 已一致(hft-venue 默认值本来就是 false) |
| J9→J7:BBO 解码 p99 0.8 µs 与门槛 0.5 µs 不一致;BUDGETPCT | 不在我名下(hft-venue-proto 已按 p50/p99 定门槛,由 T12/T14 处理);cookbook 里的旧手抄数字已改为链接 |
| SDK 代理留言的两个 fixtures 路径 | 已改;旧路径 `fixtures/strategy-sdk/` 全仓无引用 |
| J7 自身:raw-gw 分块响应按连接拼接 | 已一致 |
| RF13:`gw.rest_idle_check_ms` 同时用于 warm 任务 | 已一致,由 VEN-159 覆盖 |
| RF4:`uid_policy` 取值小写 | 已改 |

## 5. 需要负责人决定

1. 增量深度只在"验收"节列出启用前须补的用例清单,不写成带里程碑的用例。这个写法是否接受?
2. 单帧产出超过 `FRAME_EVENTS_MAX`(64)条事件时,多出的丢弃并告警,这是一条会丢数据的路径。另一个选项是把整帧按解码错误处理。请选一种。
3. 补帧期间用 `update_id ≥ last_fwd` 放行,会让断线期间没变化的同一条报价再被转发一次。我认为这是期望行为,请确认。
4. 有几条 host 用例的通过标准数值是我定的,例如 VEN-380 的"满环期间线程运行时间 < 30 s / 60 s"。
5. host/testnet 实测结论(VEN-384/386/388)要求改默认值时,我写成"同一改动直接更新默认值"。是否接受?

草稿与改动脚本留在 `E:\trading\architecture-v2\_work\t13parts\`。

## T14

# T14 最终报告(M0-g:venue-binance / venue-okx / venue-bitget)

三份 `TESTS.md` 都已写完。只看我名下的 6 个文件,三项检查全部为零:`check_tests` 无问题;`check_hft` 无条目,BUDGETPCT 也为 0;`heading_audit` 丢失标题 0。OKX 和 Bitget 由两个分支按统一约定并行写,三所的规则合并后由我统一对齐。

## 1. 写了哪些 TESTS.md

| 组件 | 合计 | unit | table | prop | golden | fuzz | bench | integ | testnet | host |
|---|---|---|---|---|---|---|---|---|---|---|
| venue-binance(BIN) | 158 | 43 | 64(约 450 行) | 4 | 30 | 3 | 9 | 2 | 3 | — |
| venue-okx(OKX) | 176 | 68 | 54 | 5 | 36 | 3 | 7 | — | 3 | — |
| venue-bitget(BGT) | 154 | 37 | 57 | 6 | 34 | 3 | 6 | — | 10 | 1 |

- **里程碑**:绝大多数用例在 M3。门槛 ⑥ 的连续 3 天冒烟是 M6(BIN-216、BGT-220,OKX 一条)。"M3 之前必须核实"的事项写成 M3 的 testnet 用例,例如 BIN-214/215、BGT-210–218。
- **金样**:每种消息类型一条 golden。未采集的输入写"待采集(方法 X)";依赖待核实项的用例写了 `备注`。
- **错误码与 fuzz**:错误码表每个码一行 `table`。fuzz 按 testing §3.3,每所两个计入门槛 ③ 的目标,另加一个 encode 往返目标。
- **可选扩展**(增量深度、SBE、`mock` 反向编解码、`cancelReplace`):覆盖表标"里程碑外",`## 验收` 写明启用时同一改动补写用例。
- **README**:三份 README 的"测试与验收"都只留链接句;这一节下原本都没有 `###` 子标题。

## 2. 规格改动(都在我名下的 README;括号内是暴露问题的用例)

**venue-binance**
- **类枚举**:补齐 `MdClass`/`ReportClass` 的变体(BIN-010/011/013)。
- **路由与端点**:现货成交路由命名为 `spot-md`;现货 `endpoint(UserStream)` 返回 Unsupported;listenKey 经 `bootstrap` 获取,再由 `endpoint` 拼出私有流路径(BIN-003/004/005/007)。
- **行情解码**:
  - `peek` 的 ids 取值规则(BIN-021)。
  - `Funding.interval_s` 的推导(BIN-033)。
  - forceOrder 的字段映射、现货 trade 的 `ex_send_ns`(BIN-031/034)。
  - FxErr → DecodeErr 的映射(BIN-026)。
  - 分档快照中数量为 0 的档不输出(BIN-028)。
- **下单**:
  - 模板示例的价格改为最短十进制 `65000.1`;写明 reduceOnly 在帧中的位置(BIN-064/065/067)。
  - 现货 STP 取 `EXPIRE_MAKER`(BIN-066)。
  - 现货改单走 `amend.keepPriority`,只写新数量;`cancelReplace` 列为可选扩展(BIN-070)。
  - 撤单和查单一律按 `origClientOrderId`(BIN-068/071)。
  - 请求 id 的 op 字符表(BIN-077)。
  - `order.status` → 订单状态码(BIN-094/095)。
- **限速**:`rate_rules` 的映射;合约新连接核实前按 300/5 min;`rateLimits` 有多条时只取利用率最高的一条(BIN-091/170)。
- **VenueCaps**:补 `required_multi_asset` 行、`LiqMarker` 码与参数串、`flags = 31`;BNB 以资产名给出(BIN-171)。
- **账户配置核验**:各请求的名义权重(待核实)、`UNREADABLE` 的触发条件、签名 REST 的格式(BIN-140/142/145)。
- **保证金**:`margin_ratio` 向零取整,不采用交易所的比率字段;ADL 档从未取到时为 255;positionRisk → PositionReport 的映射(BIN-148–151)。
- **用户流**:ACCOUNT_UPDATE → 持仓/余额的映射(BIN-112);现货余额推送的时间戳取 `u`(BIN-121);现货改单回报 `REPLACED` → AmendAck(待核实,BIN-111)。
- **心跳**:`heartbeat(role)` 的取值(BIN-190)。
- **BBO 补帧**:合约一次只取一个 inst;现货多个 symbol 用 URL 编码数组;写明解码规则(BIN-050–055)。
- **订阅**:帧 id、流名规则与 Unsupported 情形(BIN-200–202)。
- **演示环境**:`uid_policy` 改小写(RF4);两个演示账户共用 UID 时只告警(RF6,BIN-212);`unverifiable` 账户缺 UID 时不置 `UID`/`UNREADABLE`(BIN-213)。

**venue-okx**(分支所改,下面两处是我的补充):
- 类枚举;路由与上限 100/4(我把它补进开发备忘第 5 条);各频道字段映射。
- CancelAll 交运行时展开成 Cancel,按 ≤ 20 条交给 `encode_batch`。
- REST 签名;REST 查单解码为 QueryResult;`51603` 产出 NotFound,不定终态。
- 登记 `51006`/`51008`/`51400`;子码按主码编码;批量响应逐条判定。
- CAA 的 `timeOut` 必须是 [10, 120] 秒内的整秒。
- **`margin_ratio` 由 `1/mgnRatio` 改为 `mmr/eq`**,以符合 hft-venue-proto 契约 17;我补了"向零取整"。
- 资金费按 `cashBal` 基线求差;`amendResult` 的映射。

**venue-bitget**(分支所改,下面三处是我的对齐):
- 分支所改:类枚举;行情按路由分类(V2 推送的频道名在第 64 字节之后);FOK 返回 Unsupported;Query 走 REST;`flags = 7`;确定拒绝表在核实前为空。
- **CancelAll**:改为与 OKX 同一规则——`encode` 返回 `Unsupported(CancelAll)`,由运行时展开为逐单 Cancel(BGT-071/076/092/013)。
- **BBO 一侧为空**:改为产出该侧为 0 的 `Bbo`,与 OKX 和下游的"缺一侧"处理一致;原来是报 Shape(BGT-027)。
- **unverifiable UID**:补了缺 UID 不置位的规则(BGT-142.18)。

## 3. 留给别人的跨文件问题

**T12(hft-venue-proto)**
- trait 缺两类出口,`BootCall`/`DeadManOut` 也未定义,`bootstrap` 拿不到凭证:
  - REST 形态的订单操作:Binance 合约全撤、OKX/Bitget 查单、OKX `orders-pending`。`encode` 只能写 WS 帧。
  - 会话维护请求,即 listenKey 的 PUT 续期。
- `ReportOutcome` 没有控制提示字段,三种情况无法上报:Binance `-1021`(时钟偏差)、`listenKeyExpired`/`eventStreamTerminated`(用户流断)、OKX 私有连接上的 `64008` 预告。
- `rate` 和 `outcome_unknown` 都只能带一条,而 OKX 批量响应需要逐条产出结果未知;契约 8 需要放行这种做法。
- 契约 16/23 要写明 `unverifiable` 账户的 UID 处置;testkit 要接受"现货钱包不能借币,用 ACCT_TYPE 代替 AUTO_BORROW"。
- `NAME` 的注释说它与 VenueId 常量名一致,但 Bitget 不成立(`bitget-usdt-futures` ≠ `BITGET_UTA_USDT_FUT`)。
- `default_caps` 拿不到 refdata,给不出扣费平台币的 AssetId(与 T3 相关)。
- `classify_report` 没有 rd 参数,refdata 追加的可忽略事件名只能在 `decode_report` 判定;契约 20 的措辞要对齐。
- BBO 补帧:`decode_bbo_snapshot` 不带 inst,而 OKX 的响应里没有 instId;批量容量也没有声明。
- REST 解码拿不到发起请求的 ReqRef(OKX `51603` 响应不带 `clOrdId`)。

**T13(hft-venue / fixtures)**
- 运行时要实现统一的全撤规则:`encode(CancelAll)` 返回 Unsupported 时,把该账户活动订单展开为 Cancel(支持批量就按批);OKX 另用 `orders-pending` 兜底扫一遍。
- BBO 补帧需要一种查询批量能力的方式。
- Bitget"120 s 无 ping 即断"的回归要落在运行时侧。
- fixtures README 要登记金样新增的期望写法:`frame`/`login`/`hint`/`outcome_unknown`。

**T2 / T8**:`OrderEvent` 没有 `detail` 字段,而 hft-oms 用它承载 QueryResult 的订单状态和撤单失败的 GONE。

**T8**:Binance 现货改单只能减量,帧里也没有价格;OMS 需要拒绝改价或增量的 Amend,或者在 VenueCaps 加一个标志。

## 4. 合并遗留的处理结果

| 条目 | 结果 |
|---|---|
| J7 F6 Binance 演示环境端点 | 已一致:仍写待核实,由 BIN-214 在 M3 核实 |
| J7 → J6 `uid_policy` 链接缺锚点 | 已改:三份 README 都链到 hft-refdata `#数据结构` |
| J7 §5.2 演示环境同一 UID 撞 `DuplicateVenueUid` | 已改:按 RF6 写进 README,并有 BIN-212 |
| J7(venue)BBO p99 0.8 宽于 0.5,BUDGETPCT 7 条 | 已一致:T12 已按 RF14 把门槛定为 p50/p99,数值与三所预算相同,venue 预算无需改,BUDGETPCT 为 0 |
| OKX/Bitget 测试网一节链接 `uid_policy` | 已一致;RF4 小写已改 |

## 5. 需要负责人决定的地方

1. Binance 现货改单只减量:是在 VenueCaps 增加能力位,还是由 OMS 规则约束。
2. OKX `margin_ratio` 改为 `mmr/eq`(遵循契约 17,不再用 `1/mgnRatio`),请确认。
3. 几项默认值是在待核实期间由我们自定的,请认可:
   - Binance 各连接的空闲超时;
   - OKX 每连接流数与入向上限 100/4;
   - 三所账户查询的名义权重;
   - Binance 现货 STP 取 `EXPIRE_MAKER`。
4. Bitget 现货账户处于高级(多资产)模式时目前不置任何不符位;是否像 OKX 一样判 `ACCT_TYPE`。
5. reconciler/kill-switch 用的只读 REST 响应(挂单、成交、资金费流水,fixtures 清单第 10 条)在 venue 层没有解码入口,三所都没有写对应的金样。

文件:
- `E:\trading\hft\crates\venue-binance\TESTS.md`、`README.md`
- `E:\trading\hft\crates\venue-okx\TESTS.md`、`README.md`
- `E:\trading\hft\crates\venue-bitget\TESTS.md`、`README.md`

## T15

我把三份 `TESTS.md` 写完了,三个检查在我名下的文件上都是零问题:`check_tests` 0、`check_hft` 无命中、`heading_audit` 丢失标题 0。

## 1. 写了哪些 TESTS.md

| 文件 | 用例数 | 按级别 | 表格行数 |
|---|---|---|---|
| `crates/hft-ctl-proto/TESTS.md` | 120 | table 78、golden 23、unit 8、prop 8、fuzz 2、bench 1 | 901 |
| `crates/hft-ctl/TESTS.md` | 100 | table 54、unit 27、integ 6、bench 4、prop 3、drill 3、golden 1、host 1、testnet 1 | 379 |
| `apps/kill-switch/TESTS.md` | 85 | table 40、unit 30、prop 4、host 4、drill 4、testnet 3 | 202 |

- **权限表每一格**:CTP-100–CTP-139 用脚本生成,39 种命令/参数组合 × 13 种主体(三种角色 × 私聊/群聊/CLI,加 ops_bot、pipeline、reconciler、cli:systemd)。
- **每条命令的每个前置条件**:CTL-200–CTL-215 是"客户端 → gateway → 真实 sequencer"的端到端结果码表。
- **与 T22 去重**:`docs/tests/scenarios.md` 的 MD-7、MD-9、MD-10、MD-11、MD-21 已覆盖的行,我从端到端表里删掉了,只在备注里指过去。
- **README 迁移**:三份 README 的"测试与验收"一节都只剩链接。kill-switch 那一节下面没有 `###` 子标题,所以没有删掉的子标题要列。

## 2. 规格改动

| 文件 | 小节 | 改了什么 | 暴露它的用例 |
|---|---|---|---|
| control-plane | 类别 | 按 RF11 改了 Live 实例的定义:绑定 Live 账户,且至少一个 cell 的期望模式为 Live | CTL-201.5/.6 |
| control-plane | 角色 | 按 RF5 写明 ctl 角色只认签发者表,与 `.github/hft-admins.txt` 无关 | CTL-040 |
| control-plane | 账户语义 | 第 2 条措辞;第 8 条的 group 规则;新增第 9 条逐命令的账户取值(必须具体 / 只能 `*` / 两者均可) | CTP-020–023 |
| control-plane | 结果码 | detail 102 的含义扩大;`Applied` 的产生者加上 ReconcileReport | CTP-023、CTL-103 |
| control-plane | 版本与确认 | 写明预览返回的 status 与 rflags 取值 | CTL-131、CTL-205.1 |
| control-plane | ReplaceRefdata | 版本先检查,不递增回 `StaleVersion` | CTL-211.8 |
| hft-ctl-proto | 请求与响应 | 原来只有占位,补齐:<br>• `code` 改为可空;status 与 code 一一对应;detail 的 arg 怎么渲染;`seq` 的含义<br>• `MAX_BLOB = 786 432`,替换原来含糊的"≤ 1 MiB − 信封"<br>• JSON 字段约定;哪些错误回 `invalid_request`;`chat_type` 规则<br>• `cmd_hash` 的精确字节定义<br>• 信封与命令体都写账户时必须相同<br>• `GetSchema` 与 ReconcileReport 的请求形状 | CTP-012–025、CTP-041、CTP-053、CTP-070 |
| hft-ctl-proto | 实现说明 | normalize 各步的报错顺序与绑定检查;normalize 留给 gateway 填的字段 | CTP-054、CTP-056 |
| hft-ctl-proto | 二次确认 | `expect_state_version = 0` 按缺省处理 | CTP-080.3 |
| hft-ctl-proto | 使用本组件 | `class_of`:SetParams、SetRisk 返回中性 | CTP-090 |
| hft-ctl-proto | 主体与角色码 | `CmdHdr.role` 取允许该命令的最高角色;pipeline 的 SetMode 必须给具体 group | CTL-033、CTP-111 |
| hft-ctl | gateway | 处理顺序 ①–⑪;一个请求的记录全写或全不写;ReplaceRefdata 的解析结果也放进 Mailbox(与 hft-sequencer 对齐) | CTL-020、CTL-066、CTL-100 |
| hft-ctl | 鉴权 | 未知 uid 的处理;每个 uid 只接受自己的 issuer 形式;CLI 登录名必须与 uid 一致;限速窗口与计数口径;审计 Note 的填法 | CTL-030–055、CTL-101 |
| hft-ctl | CmdResult 回传 | 挂起表满回 busy;去重缓存的登记与淘汰;`result()` 改为以原请求重发 | CTL-014、CTL-111–113、CTL-132 |
| hft-ctl | 命令校验 | 校验表里缺的结果码补齐 | CTL-060–067 |
| kill-switch | 判活 | Suspect 由任一信号停滞进入;"≥ 时长"的判定口径;Watching/Suspect 遇 Stopped 回 Idle;启动时的初始状态 | KS-041、043、047、049 |
| kill-switch | 使用本组件 | 退出码 1 与 3 的条件;`fire` 不带 `--yes` 时要交互确认 | KS-003、006、007 |
| kill-switch | 目标表 | 配置变更的检测周期与重建失败的处理 | KS-026 |
| kill-switch | 本地触发 | 触发请求的校验顺序;`req_id` 去重窗口含 600 s 整 | KS-092、097 |
| kill-switch | stop_core | `Inactive` 以 pid 消失判定;`Denied` 靠 stderr 文本判定(文本待核实);并发的 stop_core 合并时把范围扩成 `*` | KS-116、121 |
| cookbook | 验证 / 收尾 | 每一步的验证写成用例,并跑 `cargo xtask test-trace` | — |

## 3. 留给别人

- **T2(hft-event)**
  - "账户编码一致性"段说"升级与中性命令为 0 时拒绝",与 control-plane 冲突:解除 `Kill{Global}` 的 `Resume{Kill}` 必须带 `account = *`。
  - `CmdPreview` 没有字段承载 WriteOffResidual 预览要显示的"多出那一腿"。
  - CtlAudit 的 `v[]` 里没有对端 uid 字段。
- **T11(hft-sequencer)**
  - `ModeErr::UnknownPending` 在映射表里缺失,应映射到 `PreconditionFailed{UnknownPresent}`(CTL-204.11 依赖它)。
  - 预览返回的 status、ReplaceRefdata 先查版本这两条,要与 control-plane 新写的对齐。
  - 绕过 gateway 直接写环时,确认请求带 `expect_state_version = 0` 仍能跳过版本检查。
- **T16(apps/hft-core)**:配置表里没有 ctl-gateway 签发者表(`principals`)的来源。
- **T13(fixtures/README)**:目录组织里缺 `ctl/`(请求、规范化字节、cmd_hash、帧与响应金样)和 `refdata/ctl-p0.blob`。
- **T4(hft-testkit)**:我的用例假定测试策略能提供这些变体:`non_prod_only`、可配置的 `mem`/`ckpt`、结构字段 `win` 与非结构字段 `thr`、命令描述符 `op 1`/`op 2`。

## 4. 合并遗留的处理

- **J4/J5 → control-plane**(核销链接 hft-oms、SmokeOrder 去掉价格带、补 `BadInst` 映射、删除 `GroupNotBroken`):已一致。
- **J5 / X3 P1-1 的 Live 实例定义**:已改,按 RF11 的口径。
- **J10 → control-plane:84**(额外确认条件与 `"v":1`):已一致,条件清单归 ops_bot,标记格式归 kill-switch。
- **J8/J10 → kill-switch**(`.aborted`、实例停止后解除热段映射):已一致,由 KS-053、KS-117、KS-172 覆盖。
- **J6 §5.3 签字名单归属**:已改,按 RF5 写进 control-plane 角色一节。
- **J7 #112**(每个 Live 目标都要配 kill-switch 凭证):已一致(KS-023)。
- **J9 → J10**(ops_bot 要知道作废时 `marker = false`):不在我名下;kill-switch 侧已写清(KS-117)。

## 5. 需要你决定的地方

1. **WriteOffResidual 的预览内容**:要么在 `CmdPreview` 里加字段,要么删掉 control-plane 里"预览显示多出那一腿"这句。
2. **`expect_state_version = 0` 等于"不检查"**:gateway 已经拒绝这种确认,但 sequencer 侧是否也拒绝要定(T11/T2)。
3. **ctl 签发者表放在哪、谁维护、是否热加载**:现在任何配置里都没有它。
4. **`Denied` 靠 systemctl 的 stderr 文本区分**:文本要在 dev2 上核实(KS-171);如果不同 systemd 版本输出不一样,是否改为凡非 0 都按 `Failed` 处理。
5. **我为 JSON 线格式补的约定要你认可**:字段名、整数用十进制字符串、`MAX_BLOB`、错误分层这些原来 README 里只有占位,是我定的;它们一旦定下就是 Python 客户端与金样的合同。

生成用的脚本和分段草稿在 `E:\trading\architecture-v2\_work\t15\`。

## T16

I've written `apps/hft-core/TESTS.md` with 282 test cases and handled every leftover item addressed to hft-core. All three checks report zero problems on my files: `check_tests` and `check_hft` have no entries for hft-core, and `heading_audit` reports 0 lost headings.

## 1. TESTS.md written

| Level | Cases |
|---|---|
| table | 111 (637 rows) |
| proc | 57 |
| host | 39 |
| unit | 33 |
| integ | 23 |
| drill | 9 |
| prop | 5 |
| bench | 2 |
| golden | 1 |
| replay | 1 |
| testnet | 1 |

The cases cover every exit code and named error, all 13 thread-plan rules plus the syntax rule, and each host class (dev2, c4, c8, c16, c32). Memory, hugepage, capacity and ring-capacity tables are checked value by value, and the checkpoint file format (`CkptHdr`/`PathSecHdr`) has layout, round-trip and corruption cases. Every startup step has cases, including the stop marker and kill self-test. Crash recovery covers the two-layer poison guard and `--recover-from-snapshot`. The dev2 guard has every signal threshold at its boundary, both action levels, the causality check, the `tripped` flag, sim auto-restart, and drills 1–9 as `drill`/`host` cases with authorization stated.

The README's 测试与验收 section is now one sentence pointing to TESTS.md; it had no `###` subheadings to remove.

## 2. Spec changes (all in `apps/hft-core/README.md`)

- **命令行与退出码**: added an error-report rule. Each error prints as `error[<name>]` on stderr; steps 1–2 collect all errors; exit 2 wins over exit 3. Added a table of about 40 error names (refdata errors reuse the `LoadErrCode` names), a `Usage` error for bad command lines, and what `--check` does and does not do (it never reads or deletes secrets, locks memory or touches the journal). Exposed by CORE-005–014 and all exit-2 tables.
- **实例角色**: new `InstanceHostClass` error. `sim` may only run on dev2/c16/c32/metal, and `order_env = "prod"` only on c8 and above (CORE-041).
- **线程计划**: thread names must be unique within an instance (CORE-060.3). Rule 6 now says the keepawake exception is not a hft-core thread.
- **启动顺序**:
  - Defined the `v[]` layout of `Note{StartupStep}` and when steps 6–8 count as finished. Flag bits now carry the "over budget" and "gate timeout" alerts (CORE-220/221).
  - Step 4 now writes `Note{JournalTailLost}` (CORE-236).
  - Step 5 now exits 2 with `BindingChangedWithPosition` (CORE-289).
- **崩溃隔离**: the `SEQ` form of `--recover-from-snapshot` does not fall back to an older snapshot, and the issuer is `cli:systemd`. It also writes `Note{TailSkipped{Operator}}` (CORE-284/285).
- **配置文件字段**:
  - `host.mem_budget_mib`, `advisor.mem_budget_mib` and `max_sub_insts` are now required (CORE-360).
  - Storage thresholds trigger at "lag ≥ limit" and "free space < percentage" (CORE-362).
  - `endpoint_override` is now allowed in the `sim` instance (CORE-048, CORE-045.8).
  - acct-fetch reuses the account's `gw.src_ip` (CORE-169).
- **dev2 护栏**: after a second level-2 stop on the same UTC day, sim does not auto-restart even after UTC midnight (CORE-190). The "no anomaly" wording in drills 1, 5 and 8 is replaced with measurable criteria (CORE-210/213/216).
- **延迟与资源契约**: registered the benchmark name `core/advisor_ckpt_chunk`; the cold-start row now points to CORE-400 and CORE-267.

## 3. Left for other agents

- **T2 (hft-event)**: two alerts in my README have no registered Note to carry them:
  - the P0 when a corrupt stop marker falls back to a global Kill;
  - the P1 when a model checkpoint is skipped for exceeding the disk limit.

  Until these are registered, the tests assert the behaviour but not the alert level.
- **T3**:
  - hft-journal does not define "cold/hot conflict" anywhere; CORE-121 depends on it.
  - hft-refdata line 244 still writes `uid_policy` values capitalised (RF4 says lowercase).
- **T18 (watchtower)**: needs an alert rule on the new `StartupStep` flag bits.
- **T19 (deploy)**: needs test host manifests `test-<class>` for `gen-host`, plus the guard and `hft-shm-release` test locations (`deploy/dev2/tests/`).
- **T22 (benchmarks)**: register `core/advisor_ckpt_chunk`.
- **T13 (hft-venue)**: line 275 repeats the source-address reuse rule; it can become a link now that hft-core owns it.

## 4. Merge leftovers

- **Changed**:
  - fallback (a) now includes an archiver with only the `files` source, and the resolved open question is removed (J10);
  - the ENI-B2 criterion now links testing §8.2 item 10 (J9);
  - `startup-cold` is now referenced in the README (J9);
  - rule 6 now ties into the keepawake exception (J11);
  - the probe checklist now matches 11 §3.2 (J1);
  - `uid_policy` is lowercase (RF4);
  - acct-fetch reusing `gw.src_ip` is now stated (J7).
- **Already consistent**:
  - the `hftbatch.slice` rename and dev2 memory table;
  - the `Requisite=` replacement;
  - the tmpfs used-space check;
  - who creates the heartbeat directory (deploy uses `ExecStartPre=+` with a setgid `2750` directory, which also resolves J12's group concern);
  - `/dev/cpu_dma_latency` permissions (deploy's udev rule);
  - the epoch safety gap (journal-archiver `restore.epoch_gap`);
  - `feed.raw_dups`;
  - J11's points on the 90% valve, `TASK_ON_HOT` and the `park_us` scope;
  - J5's items: sim Recovery, tail bound, `IdentityMismatch` written by sequencer, chain ring rows;
  - the c16 heading;
  - J2's one-gw-per-thread assumption.
- **Could not change**: none.

## 5. Decisions for you

1. **`endpoint_override` in the `sim` instance**: I allowed it so CI can test the sim instance against a fake exchange. The risk is a production sim being pointed at fake market data by mistake.
2. **Poison guard vs systemd start limit**: if a crash happens within 600 s of the original start, systemd's start limit (3 starts in 600 s) stops the unit before the guard reaches its tail-skipping start. That is the safe direction, but tail-skipping then only works after long runs.
3. **Alerts without carriers**: until T2 registers the two Notes in §3, those P0/P1 alerts cannot be asserted.
4. **Drill 8 pass criterion**: I defined "legacy PSI does not rise" as the 10-minute mean staying within the pre-load mean plus 1 percentage point. That threshold is my choice and needs your confirmation.
5. **Upgrade-host cases**: the M7 host cases (CORE-077–079, 170–176, 400–408) need rented instances and authorization; they produce evidence only, not CI results.

## T17

T17 完成:`apps/replayer` 和 `apps/reconciler` 的 `TESTS.md` 已写好,两个 README 做了最小的规格补齐,三项检查在我名下文件上问题数为零。

**检查结果**
- `check_tests`:问题 0,我的文件没有警告。
- `check_hft`:问题 0。剩 1 条启发式警告(reconciler README:190 的 `instance.journal_cold`),这是 reconciler 自己的配置键,README 里已注明与 hft-core 无关,修订前就有。全仓其余问题都在别人的文件里。
- `heading_audit`:丢失标题 0。"测试与验收"下面两个文件都没有 `###` 子标题,所以没有删掉的标题。

## 1. 写了哪些 TESTS.md
- **`apps/replayer/TESTS.md`(RPL)**:143 个用例,另有 197 个表格行。按级别:unit 63、table 44、integ 13、proc 9、replay 4、bench 4、host 3、golden 2、dst 1。
- **`apps/reconciler/TESTS.md`(RCN)**:118 个用例,另有 261 个表格行。按级别:table 49、unit 33、integ 21、testnet 6、proc 3、prop 3、host 2、golden 1。
- 两个 README 原"测试与验收"表格的每一行都迁成了用例。replayer 三项可选扩展(bench、raw-decode、`--chain`)标为里程碑外,原有内容保留在验收第 8 条,启用时再补成编号用例。

## 2. 规格改动

**replayer README**
1. 每日回放的命令原来没写。新增 verify 第 9 条"每日模式"(`--report-dir`):选日条件、报告各字段怎么填、`recompute` 结论的优先级(fail > skipped > pass)、坏的一天怎么处理、与哪些选项互斥。暴露它的是 RPL-070 到 RPL-078。
2. verify 第 4、6、7、8 条:
   - Recovery 区间的差异列入 `recovery_diffs`。
   - 分歧明细写到标准输出和 `.divergence.json`;`--advice journal` 时报告 `recompute = "skipped"`。
   - 检查点文件损坏退出 3;`last_seq` 不在输入覆盖的区间内时按"无起点"处理。
   - 状态重放的起点快照规则(含段首引导块的例外)。
   - 路径节丢弃后的比较边界改为直接引用 SDK 的 `basis_seq < recompute_from_seq`。
   - 暴露用例:RPL-024、027、044、049、050、057、061。
3. sim:
   - 起点不是快照、或 `--accounts` 含未经 `--as-sim` 的 Live 账户,退出 2。
   - 中位滞后在偶数条和无样本时怎么取。
   - 新增输出 `sim.json`,用来承载 `approximate` 等标志。
   - `--ab` 与显式 `--advice journal` 同时给出时退出 2。
   - 暴露用例:RPL-097 到 RPL-104、RPL-144。
4. compare 的时间窗改为含边界(|Δ| ≤ `--window-ms`),暴露用例 RPL-166。
5. latency 按 hft-telemetry 实际接口重写:
   - 每个数据集一份结果和一个 `latency-<k>.hdr`;输出里有 `kernel_rx_hi`、按层的 `unsynced_cal`、`probe_strata`。
   - 窗口取 [from, to);`--to` 之后的下一条 `ClockStatus` 照常读入,供插值用。
   - `--cal-max-err-ns` 和 `--burst-gap-ns` 改为必填。
   - 暴露用例:RPL-190 到 RPL-206。
6. 其他:`--cache-dir` 的缺省行为、S3 下载失败退出 3、`--instance` 与段头不符退出 2。verify 吞吐的基准原来写 `replay_verify_day`,这个名字在 benchmarks 没有登记,且重抄了数值;改为链接已登记的 `replay-throughput`(RPL-250)。

**reconciler README**
1. `once` 的流程和退出码;`--dry-run` 不连接 ctl、不写状态目录(RCN-008 到 RCN-010)。
2. 读到 `Note{OmsNeedsRestQuery}` 时只做 Unknown 收敛的那一步,提交的组只含 `UnknownResolved`、不含 `Summary`,不算一轮(RCN-034、RCN-111.19)。原来没规定;如果带 `Summary`,风控会把它当成一轮"全部 Ignore",在没比对仓位的情况下放行。
3. 在途判定:
   - 条件 ① 列出具体状态,静止的 `Live` 挂单不算在途。
   - 条件 ② 的窗口按引擎时间定义。
   - 资产在途跟随以它为基础或计价资产的 inst。
   - 补了 `S0_utc` 的定义。
   - 暴露用例:RCN-050 到 RCN-052。
4. 比对项与报告组:
   - `funding_dust` 原来在全仓都没有定义,现定为结算资产的 `dust`。
   - `UnknownResolved` 一律 `Ignore`;`Summary.severity` 的规则。
   - 孤儿单、幽灵单、剩余数量差的配对规则和字段。
   - `report_id` 的生成方式、条目顺序、超过 256 条的截断;资金费修正只取本组实际包含的 `Funding` 条目。
   - 暴露用例:RCN-060 到 RCN-075。
5. 在途划转:新增 `pending_in` 规则(RCN-103)。原来只写了"到账前不报",没有机制。
6. "连续 3 轮查询失败报 P1"原来没有落点,现在写进 `<state>/status.json`(RCN-092、RCN-148、RCN-149)。
7. 配置:
   - 新增必填键 `refdata_blob`。原规格要求 `--check` 不读 journal,却又要按 refdata 拒绝 Sim 账户,两者矛盾;做法与 kill-switch 一致。
   - `http.max_rps` 原来写"按所"但没有值,`egress_bind` 原来只写了来源;两者都改为必填。
   - 补了加载校验清单。
   - 暴露用例:RCN-001 到 RCN-006。
8. 实现说明:限速闭环的阈值;每 6 h 全量扫描从启动后首次对账算起(RCN-180、RCN-182)。

## 3. 留给别人的问题
- **T10(hft-ledger)**:ledger 写着 reconciler 对 `BalanceOnlyPush` 的所会出按 inst 的"归属条目",但 reconciler(口径归它)只出账户级钱包条目。LED-125、LED-135.1 需要对齐。
- **T9 / T3**:资产级 `dust` 在全仓没有定义,refdata 的 `AssetMeta` 和 `RiskParams` 里都没有这个字段;risk、ledger、reconciler 都在用它,需要确定归属。
- **T9 / T11**:不含 `Summary` 的报告组不能产生闸门输入,也不能计作一轮 Ignore。
- **T18(watchtower)**:需要读取 reconciler 的 `status.json` 来发 P1。
- **T19(deploy)**:`reconciler.toml` 要生成 `refdata_blob`、`http.max_rps`、`egress_bind`;`hft-replay-verify` 的启动命令改用 replayer README 的每日模式命令行。
- **T22**:
  - testing §3.5 的 `checkpoint_key` 现在是列表(每个实例一项);`skipped_reason` 的取值有 `advice_journal`、`model_no_start`、`input_incomplete`、`incompatible`。
  - benchmarks 需要登记 `compare_day` 和 `latency_day`。

## 4. 合并遗留的处理结果
- **J8:replayer 第 65 行"事件时间应写 UTC"**:已一致。这条留言过时了,SDK 现在就是用引擎时间定义 `recompute_from_seq`;我把 replayer 改为直接引用 SDK 的定义。
- **J8:读者侧计算关键帧 `state_hash`**:已一致(verify 第 1、5 条),对应用例 RPL-016、RPL-017。
- **J8 / 其他代理留言:recompute 配对键与 `coalesced`**:已一致。README 的配对键比留言多了 `account`,这与 sequencer outbox 的键和 `AdviceHdr` 的字段相符;对应用例 RPL-042、RPL-045。
- **J8 / 其他代理留言:latency 分层结果、`kernel_rx_hi`、按数据集返回多份报告**:已改。
- **J9 / J11、RF8:reconciler 按 hft-ledger 第 6、7 步消费 `AUTO_RECON` 与 `OmsForeignFill` 注记**:已改,对应用例 RCN-042、RCN-043。
- **F1(c) 的 `AUTO_EXTERNAL` 五个条件、F4(b) 的 CoreRestart**:已一致。

## 5. 需要负责人决定的地方
1. 在途划转的 `pending_in` 没有超时:如果钱一直不到账,这个差额会被一直掩盖。是否设一个时限,超过后转为 `Warn`?
2. 每日模式下,检查点键不符(例如某次发布提升了 `LAYOUT_VERSION` 之后去补跑旧日)目前会退出 2 并停止处理后面的日子。这和开发备忘里"当日有版本切换"的开放问题是同一件事:是否改为把该实例记作 `model_no_start`、继续往后跑?
3. 我把 `UnknownResolved` 定为一律 `Ignore`(它是证据,不是差异),这样它不会挡住严格闸门。请确认。
4. 新增的 `refdata_blob` 读的是文件;hft-core 执行 `ReplaceRefdata` 之后,journal 里的 refdata 可能比文件新,启动核验用到的 `venue_uid` 会是旧值。是否接受?
5. 资产 `dust` 应该放在 refdata 还是 `RiskParams`(见第 3 节)。

## T18

我已为 watchtower 和 journal-archiver 写好 `TESTS.md`。三项机检中我名下的文件都是 0 个问题;heading_audit 丢失标题 0,两份 README 的"测试与验收"节下都没有子标题。剩下 3 条 CFGREF 提示,在 watchtower README 第 55、56、130 行,是改动前就有的启发式提示,不计入问题数。

## 1. 写了哪些 TESTS.md

| 文件 | 用例数 | 按级别 | 表格子行 |
|---|---|---|---|
| `E:\trading\hft\apps\watchtower\TESTS.md`(WT) | 148 | table 56、integ 57、unit 12、proc 12、host 8、drill 3 | 336 |
| `E:\trading\hft\apps\journal-archiver\TESTS.md`(ARC) | 122 | table 40、integ 49、unit 16、proc 6、host 8、drill 1、golden 1、bench 1 | 199 |

两份 README 原"测试与验收"表的每一行都已迁进用例。原表里有一行写错了:"watchtower 身份发 `SetMode → Live`,gateway 返回 `Forbidden`"。按 hft-ctl 的鉴权顺序,watchtower 的 uid 不在主体表里,应先被拒为 `unauthorized`,用例 WT-166 已按此改写。

## 2. 规格改动(只改我名下的两份 README)

**watchtower**
- **输入与游标**:游标重置的告警定名为 `CursorReset`;`cursor.json` 不可解析时按缺失处理;游标先写 `.new` 再 rename(WT-012–WT-016)。
- **/healthz**:定义了 `reasons` 的取值;本次运行还没有对账报告时,年龄从进入 Running 起算;维护静默文件格式定为一行十进制 Unix 秒;查询串忽略,其他方法返回 404(WT-052、WT-054–WT-059)。
- **健康规则**:
  - 停摆窗口取该实例 hft-core 配置的 `watchdog.hb_stale_ms`,配置表因此加 `instances[].hft_core_config`;并定义了心跳年龄。
  - ThreadStall 只看已 claim 的槽。
  - ProcessDown 按单元状态区分:reconciler、archiver、看门狗只在崩溃或重启循环时报。否则 dev2 每天回放暂停、联调窗口结束都会误报(WT-073)。
  - FeedSilent 只在实例 Running 时判定。
  - MarkDivergence 的参照改为"全部 venue(含自身)的中位数"。
  - ExternalFill 以 `Note{ExternalFill}` 为触发。
  - StaleGenFill 写明消费 `Note{OmsLateFill}`。
  - ReplayVerify 写清"缺报告的日子"、"超过 3 天"和前一日标志的来源。
  - VerifyRun 按段记录字段判失败与疑似;"超时"原文同时出现在失败和疑似里,已去掉重复。
  - MarginStress 的告警范围含档位。
  - AccountConfigDrift 只在闸门放行后报,并按原因位定级。
  - StorageDegraded 定义了"存储原因的 Drain"。
  - HostPressure 定为窗口内任一采样越线即报。
  - GuardAction 的锁存提醒不因 ack 停止。
  - 对应用例 WT-070–WT-122。
- **告警记录生命周期**:写明了告警指纹的取值,以及新建、计数、升级、条件消失、关闭、重开和重启恢复的规则;ack 行格式改为链接 ops_bot 的定义(WT-150–WT-162)。
- **新增核心规则 `ArchiverAlert`**:读 archiver 的告警行,配置加 `paths.archiver_alerts`(WT-122)。
- `memory.max` 为 `max` 时,序列数上限只取 `max_series`(WT-036)。

**journal-archiver**
- **使用本组件**:写明 `check`、`restore` 的退出码;`restore` 不带 `--yes` 时退出 2。
  - 测试网检查读哪份 refdata 也写明了。
  - 前缀第一段必须是 `hft` 或 `hft-testnet`;原文"不得与 collector 前缀重叠"无法检查,collector 的前缀没有登记。
  - 同一 (instance, journal) 不得出现在两个 source 里。
  - 对应用例 ARC-002、ARC-003、ARC-013、ARC-121。
- **输入**:区分"未封存则等待"与"已封存但损坏则 quarantine";区分链断裂与缺段;段归哪个小时取段头 `created_utc_ns`,并写明小时何时算完成(ARC-020–ARC-026)。
- **输出与暂存**:journal 原件与 manifest 的 domain;`.uploaded` 的命名;单元开始时清理残留的 `.new`;快照文件与持久元数据对象键中的时刻取文件 mtime(ARC-031、ARC-052、ARC-094、ARC-109、ARC-120)。
- **上传与删除**:
  - 重试 8 次耗尽后报 P1,之后按上限间隔继续重试,不进 quarantine。
  - 一个 journal 的待删段全部核验通过才调用 `cold_retire`。
  - 水位删除按天调用 `cold_retire`,与该接口按天的粒度一致。
  - 新增第 8 条:告警行写入 `alerts.jsonl`,并列出种类表。
  - 对应用例 ARC-106、ARC-111–ARC-116。
- **restore 与依赖**:meta 目录的 `.tmp` 不上传;S3 上的 `engine_epoch` 对象不可解析时退出 1;依赖一节写明使用 `DurableMeta`、不启用 `writer`(ARC-126、ARC-131、ARC-160)。

## 3. 留给别人

- **T3(hft-journal)**:`DurableMeta` 现在属于 `writer` feature,JNL-102.2 也断言不开 `writer` 就编译不过;但 archiver 的 `restore` 必须调用 `write_sync`,又不得启用 `writer`。需要把 `DurableMeta` 移出 `writer`,并同步 feature 表和 JNL-102.2。
- **T19(deploy)**:
  - watchtower 配置模板加 `instances[].hft_core_config`、`paths.archiver_alerts` 和 `rules.ArchiverAlert` 的开关。
  - `hft-maint` 改为写一行十进制 Unix 秒。
  - `hft-watch` 需要能读 `/var/lib/hft/archiver/alerts.jsonl`(该目录现为 `hft-archive` 0751)。
  - deploy 第 482 行的 `crashes/**/inflight.json` 应改为隐藏文件 `.inflight.json`。
  - 护栏阈值文件要带"只告警信号"的级别(J9 遗留)。
- **T4(hft-telemetry)**:`rules` 注册表需加 `ArchiverAlert` 的直通判定函数,WT-121 要求注册表、配置模板与装配表三者一致。
- **T22(benchmarks)**:登记基准名 `archiver_convert_seq_1h`,对应 ARC-155。

## 4. 合并遗留的处理

- archiver 不上传 `crashes/dst/inflight.json`、对象键带主机段:**已一致**(ARC-092.3、ARC-095);deploy 那一行留给 T19。
- watchtower 整机 PSI 级别对齐到 P2:**已一致**(WT-114.7、WT-114.8);级别要从护栏阈值文件读,留给 T19。
- archiver `restore` 负责 epoch 安全间隔:**已一致**(ARC-126、ARC-127)。
- `restore`、经 ACL 写冷副本、崩溃输入与 resolution 键带主机段:**已一致**(ARC-121–ARC-132、ARC-167、ARC-095)。
- DurableMeta 的上传与换 AZ 恢复:**已一致**(ARC-120、ARC-169);核对时发现上面 T3 的 feature 问题。
- RF18(冷副本子目录 0770 + ACL):hft-journal 侧已写,**已一致**;archiver 侧用主机用例 ARC-167 验证。
- 其他代理的留言:
  - MarkDivergence 参照改为含自身的中位数:**已改**。
  - `.inflight.json` 隐藏文件排除:**已一致**。
  - `ReportAgg`:规格归 hft-telemetry,TEL-049 已覆盖五种聚合;archiver 只在 ARC-071 做端到端比对,不另写。

## 5. 需要你决定的地方

1. **archiver 告警的投递链路是我补的。** 原规格里 archiver 的 P0/P1 没有任何投递途径(watchtower 读 archiver 状态只是可选扩展),incident-response 却把 "archiver quarantine" 列为 P1。我新增了核心规则 `ArchiverAlert` 去读 archiver 的告警行,需要你确认接受。
2. **告警生命周期的口径是我定的**:事件型告警的"条件"= 最近 600 s 内有命中;因此孤立的 P1 只投递一次,P0 一直重复到 ack。
3. **ProcessDown 不再报人为停掉的 reconciler。** 改动后,在升级主机上手工停掉 reconciler 不报 P1,只能靠 `/healthz` 的 `recon_stale` 让机外看门狗发现。
4. **archiver 只接受 `hft`、`hft-testnet` 两个前缀**,不再与 collector 的前缀做重叠比对。
5. **窗口型规则的持续时长按 10 s 评估节拍计。** 例如 MarkDivergence 的 `hold_s = 30` 实际在偏离出现后 40–50 s 才报,需要你认可。另外,dev2 和升级主机上的主机、演练用例(WT-196–WT-199、ARC-158、ARC-169)标了 `[需授权]`。

## T19

# T19 最终报告(deploy)

已写完 `deploy/TESTS.md`,共 176 个用例、750 行表驱动子用例。按简报 §1 跑了三项检查,我名下的两个文件都是 0 个问题,heading_audit 丢失标题 0。全仓还剩 3 个 BADLINK,都在别人的文件里。

## 1. 写了哪些测试规格

`E:\trading\hft\deploy\TESTS.md`(前缀 DEP),共 176 例,按级别:

| 级别 | 用例数 | 说明 |
|---|---|---|
| table | 107 | 主体:生成器、单元文件、权限闭包、预检等 |
| unit | 28 | |
| proc | 24 | 在 CI 的 systemd 容器里跑 |
| host | 14 | 需要真实主机 |
| drill | 2 | 运维演练 |
| prop | 1 | |

- 原 README"测试与验收"一节的表格已全部迁进来。这一节现在只留一句链接,没有子标题被删。
- 生成器、单元文件、路径表和清单的规则大多写成可离线运行的 table 用例。
- **权限闭包**:
  - DEP-155 逐行列出 58 行(单元用户、路径、操作、允许或拒绝)。
  - DEP-156 在容器里带上每个单元的沙箱属性,逐行真跑一遍。
  - DEP-157 是反例:不在组 G 里的进程不能把文件或套接字设成属组 G;setgid 目录是唯一例外。
- 简报点名的其他内容都有用例:
  - preflight 每一项:DEP-235–238,另有 168、179、186、196。
  - releasectl 的安装、激活、回滚、verify、uninstall:DEP-220–232。
  - 两份主机清单与 `.standby`:DEP-010、013–017。
  - IMDS 两种机制:DEP-170–175。
  - S3 前缀与 KMS 授权:DEP-158–160。
  - 秘密投递:DEP-165–167。
  - `hft-window`:DEP-071–075。
  - `hft-shm-release`:DEP-084。
  - 回放期间暂停 archiver 与 pipeline:DEP-137、138。
- 护栏的判定逻辑、slice 与 OOM 的具体数值已经由 T16 写在 CORE-144…217 里。覆盖表直接引用这些编号,没有重写。

## 2. 规格改动(都在 `deploy/README.md`)

| 小节 | 改了什么 | 暴露它的用例 |
|---|---|---|
| 制品从哪里来、releasectl 的安装一行 | 开发构建在非 dev2 主机上安装时退出 1;写明"安全解包"的拒绝条件;同一版本目录已存在时拒绝;离线建 venv 失败时目录改名为 `.failed.<秒>` | DEP-004、224 |
| 主机清单 | 清单出现未登记的键时生成失败(dev2 和 c4 各有允许的键集合);dev2 允许的键里补上 `schema` | DEP-010 |
| 运行期调优 | 写出 `min_free_kbytes` 与两个 dirty 参数的具体值(取自 02 §4.2) | DEP-038 |
| dev2 护栏 | `WatchdogSec=10`;窗口标志的内容(操作人取 `SUDO_USER`,标志已在时不改写);`hft-shm-release` 写事件时也用 `runuser -u hft-guard`;新增护栏配置 `/etc/hft/dev2-guard.toml`,区分停机信号与只告警信号,只告警信号带级别;二级动作用阻塞的 `systemctl stop` 等待,不读心跳块;退路 (a) 新主机的清单形态 | DEP-070、072、078、081、084、085、087 |
| resctrl | `enabled = false` 时不装单元,期望剖面里各项为 advisory;`hot` 与 `rest` 两组的掩码不相交、并集为全部 | DEP-095、096 |
| systemd 单元 | 以 root 运行的单元不套用 `ProtectSystem=strict` 与空的 `CapabilityBoundingSet`;`hft-secrets` 补 `--owner` 与 `--mode`;watchtower、reconciler、offbox 补 `RuntimeDirectory`;pipeline 补 `StateDirectory`;ops_bot、pipeline、offbox 补 `StateDirectoryMode=0750`;verify 单元行写出 `StateDirectory`;hostprobe 单元加 `Before=` 两个 hft-core 实例;凭证代理与 S3 消费者只装在 dev2 或 `formal_live = true` 的主机上 | DEP-120、122、133、145、146、166 |
| 身份、路径与 S3 前缀 | 每个主机类都创建全部用户;新增 `/run/hft/` 与 `/dev/shm/hft/` 两行;主机本地文件的模式;写明冷副本目录的 ACL 条目与 `stat` 显示的组位;`/var/lib/hft/verify/` 单列一行(机检原先把属主误判为 hft-replay);treasury 不读 `hft-testnet`(S3 前缀表与消费者一节原先互相矛盾) | DEP-150、153、154、158 |
| IMDS(RF17) | 写明两种机制各自的理由和适用主机类;BPF 程序名 `hft_imds4`/`hft_imds6`;dev2 的 nft 表、集合与两条规则;nft 集合不含运维登录用户;凭证代理失败后每 1 min 重试,剩余有效期不足 5 min 时报 P0 | DEP-170–177 |
| 验证包 | 排除的文件改为隐藏文件 `.inflight.json`(与 tools、archiver 一致) | DEP-197 |
| 机外看门狗 | `hft-maint` 参数校验(非正整数退出 2,大于 30 按 30);静默期内开始、截止时仍未恢复的故障,截止后补打电话 | DEP-202、204 |
| releasectl 的激活与预检 | 激活退出码 = hostprobe 的退出码;udev 规则只装在非 dev2 主机;预检各项的判据写到可执行程度(chrony 偏移 ≤ 5 ms、工具链命令名、冷盘可用空间比例、`hft-secrets --check`、watchtower 监听地址) | DEP-226、235 |
| HostProfile | 补上 hostprobe 新采集项的期望值:BPF 程序名、PHC 读取频率上限、keepawake 登记表(默认空)、dev2 的 nft 表、护栏守护状态、EIP 关联;`pending_reboot` 的判定方法 | DEP-245、247、248 |
| 实现说明 | `hft-sim-alternate` 的配置增加 `segment_days` 与 `funding_hours_utc`,排程按段、按资金费块配平,窗口外按"开"处理 | DEP-252、253 |
| 延迟与资源契约 | RTT 抖动定义为 p99 − p50;"现状"指 dev2 | DEP-261 |

## 3. 留给别人

- **T4(tools)**:
  - 需要定义 `hft-secrets --check --manifest` 的语义。deploy 的预检和 host-upgrade 都在用这个检查模式,tools 里还没有。
  - TOOL-100.13 的夹具程序名 `hft_imds_connect` 有 16 个字符,超过内核 15 字符的程序名上限;deploy 定的名字是 `hft_imds4`/`hft_imds6`。
- **T18(watchtower)**:`host.ena_ifaces` 和 `healthz_bind` 写的是"由 deploy 按清单生成",但清单里没有接口名,也没有 ENI-A 的 IP,做不到。建议改为从 `hostprofile.json` 或 IMDS 读取。
- **T16(hft-core)**:
  - dev2 也运行 `hft-cred-broker`,但 dev2 的内存表和 OOM 表里没有它。
  - 退路 (a) 要求"原 dev2 不装 hft-replay-verify",清单里没有字段能表达这一点。
- **T22(testing)**:§8.2 写的是 `hft-sim-alternate --seed --start`,实际命令是 `--config /etc/hft/measure.toml`。
- **T20(runbooks)**:host-upgrade 第 8 步写 `measure.toml` 时,要补 `segment_days` 和 `funding_hours_utc` 两个键。
- **T21(python)**:ops_bot、pipeline、treasury 都没有 `--check`(DEP-135 暂时改用 import 检查);pipeline 读的 collector S3 前缀没有在 deploy 的前缀表里登记。

## 4. 合并遗留(M0-f)的处理

| 来源 | 内容 | 结果 |
|---|---|---|
| J1 | 探针清单、M7 与 M8 两份清单的文件名 | 已一致 |
| J6 | 运维账户进 `hft-ctl`;`KillMode=mixed` | 已一致 |
| J6 | keepawake 线程登记;hostprobe 新采集项的期望值 | 已改 |
| J7 | hft-core 单元加 `AF_NETLINK` | 已一致 |
| J8 | 回放暂停与 `-` 前缀、守护单元的 `After=`、`hft-window stop` 的核对方式、prod 心跳目录的建立者、`hftbatch.slice`、`/var/lib/hft/replay-verify` 属主、`/dev/cpu_dma_latency`、`cold_gib` 示例 | 已一致(replay-verify 的路径行另外拆分了) |
| J8 | release-and-deploy 第 65 行 | 不是我的文件(T20) |
| J9 | `hft-sim-alternate` 按资金费块配平、延长段;护栏阈值文件带只告警信号的级别 | 已改 |
| J9 | watchtower 配置模板的同机干扰参数 | 已改(补了用例 DEP-149) |
| J9 | archiver 恢复数据所需的 S3 读与 KMS 解密授权 | 已一致 |
| J9 | `host.ena_ifaces` | 无法改:见第 3 节 T18 一条 |
| J11 | ADR rx-irq-driven | 不是我的文件(T23) |
| 其他测试代理的留言 | 期望剖面要有 BPF 程序名、nft 表、护栏守护状态、EIP 关联 | 已改 |
| RF17 | IMDS 两种机制写明理由和适用主机类 | 已改 |
| RF18 | 冷副本子目录 0770 | 已一致,DEP-154 用 0750 作对照,验证写者必须用 0770 |

## 5. 需要你决定的地方

1. 预检的"无编译工具链"现在包括 `gcc`、`cc`、`clang`。dev2 是旧系统所在的主机,可能装着 gcc,这一项会让 dev2 预检失败。要不要在 dev2 上卸掉 gcc,或者把名单缩小到 Rust 工具链?
2. watchtower 的 `healthz_bind` 和 `host.ena_ifaces` 只能在运行期取到。由谁、在激活时还是运行时填写,需要定一个归属。
3. 退路 (a) 之后,原 dev2 是否还保留 `hft-replay-verify`。如果不保留,清单需要新增一个字段。
4. pipeline 读的 collector(旧系统)S3 前缀叫什么名字,是否登记进 deploy 的前缀表。
5. 以 root 运行的单元现在统一不套用 `ProtectSystem=strict` 和空能力集。是否要再按单元收紧到最小能力集?

另外,覆盖表引用了 T16 正在写的 CORE 编号。T16 如果重新编号,机检的 COVREF 会报出来。

## T20

T20 已完成:新建 `docs/tests/drills.md`,并改了 `docs/runbooks/` 下 7 个页面。简报 §1 的三项检查中,我名下文件的问题和告警都是 0:`check_tests` 无问题,`check_hft` 无问题也无告警,`heading_audit` 没有丢失标题。

## 1. 交付的用例

我不写 `TESTS.md`,只写了 `E:\trading\hft\docs\tests\drills.md`,共 **112 个用例(DRL-1–DRL-112)**。

| 维度 | 分布 |
|---|---|
| 级别 | drill 40、host 41、testnet 31 |
| 里程碑 | M1.5 2、M3 6、M4 7、M5 27、M6 20、M7 28、M8 22 |
| 带 `门槛` 字段 | 52 个 |

- **页首约定**:编号规则、证据统一写 `ops-log/<UTC 日期>-DRL-n.md`、脚本放 `tests/host/DRL-n.sh`。组件 README 自己定义的演练(如 CORE-210–217 护栏演练、TOOL-098/112、CORE-407 的 ⑩ 判定)只在覆盖表里引用,不重写。
- **覆盖**:launch A/B/C/D 各步、D-smoke、D-shadow、D-23 八个小步、host-upgrade(含 ⑩ 测量窗口与新加的换 AZ 恢复演练)、host-tuning、az-probe、发布与回滚、refdata 变更、incident-response 各节与 kill 演练、两个季度 CI 应急出包演练。
- **runbook 侧**:每个演练或检查点步骤旁已标出对应的 `DRL-n` 或组件用例编号。

## 2. 规格改动(都在我名下的文件)

**按主控裁决改的:**
- **launch A-1**:`uid_policy` 改小写 `"unverifiable"`,补签字字段 `uid_policy_signoff`,并注明演示环境同一 UID 只报 P1(RF4、RF6;DRL-1)。
- **launch D-32**:加 Admin 名单核对(RF5;DRL-31)、签字记录(DRL-30)、每页走查(DRL-112)。
- **IR 告警表与 A 节第 7 步**:`ExecGateHeldNaked` 写成两种场合,按 `v[]` 的等待原因分别处置(RF3)。
- **IR A 节第三条路与第 6 步**:计时口径只链接 hft-exec(RF2)。

**按合并遗留改的(J9、J6 留给原 J10 的条目):**
- **launch C-18、host-upgrade 第 7/8 步与 description**:窗口前 1–2 天 A/A,按 2 天一段延长,至多 14 天,两类块各自配平(DRL-56/57)。
- **host-upgrade 第 11 步**:新增换 AZ 恢复演练,含迁回原 AZ;launch C-19 引用它(DRL-60)。
- **launch D-23 第 4 小步**:游标那句简化为"不需要处理"。
- **launch D-30**:删掉"在该所价格带内"。

**写用例时发现、我自行修正的规格缺陷:**
- **launch B-14 的"hft-sim 拒单注入"并不存在**:风控的链接腿规则也不让第二腿不可用时先开第一腿,所以无法按计划制造 Broken。已改为 A–C 期间第一次出现 Broken 时按 A 节处置;C 阶段结束仍未出现则签字列为风险。B、C 两阶段的验收随之调整(DRL-12)。
- **IR 开头与 F 节第 1 条写错了**:原文说"行情类条件型 cap",但 hft-risk 的 cap 表里没有这一类。已改为按报价可用逐单拒绝(`STALE_MID`/`LINK_STALE`)(DRL-10)。
- **IR kill 演练第 3 步**:开仓意图实际是被风控以 `KILLED` 拒绝,不会到 gw 的 `KillBlocked`(DRL-89)。
- **IR kill 演练第 7 步的"有在途链接"变体做不出来**:在途链接无法按计划制造。已改为"清仓未完成"(先 flatten 再 HARD)(DRL-96)。
- **看门狗时限前后矛盾**:runbook 写"60 s 内告警到达",deploy 却是"连续 60 s 不可达才告警"。已改为"判定时长后的一个探测周期内"(DRL-26)。
- **依赖可选告警**:host-upgrade 第 8 步依赖 `IsolatedCoreIntrusion`,IR I/J 节依赖 `ClockOffset` 等,但这几条在 watchtower 里都是可选扩展。已补上没有启用时用什么方式发现。
- **正式账户每月冒烟**:按 testing ⑪ 补上 `kill-switch dry-run`(DRL-99)。
- **runbooks README**:测试与验收一节改为链接 drills;"首次用于生产前走查"写明可在 dev2 或 M7 升级主机上用测试网/模拟账户做,az-probe 除外(DRL-112)。
- **IR N 节第 4 步**:"第二轮"改为"后一轮",消掉 HISTWEAK 告警。

## 3. 留给别人

- **T22(testing)**:
  - ⑪ 写的是"级别为 drill 的全部用例",会把切 Live 之后的 DRL-45–48、DRL-99/100 也算进去。建议改为"`门槛` 字段含 ⑪ 的用例";⑩ 证据那句措辞有歧义。
  - ⑪ 的"60 s 内"与 deploy"连续 60 s 不可达才告警"矛盾,建议改为 ≤ 75 s。
- **T19(deploy)**:`hft-sim-alternate` 还要支持按段排程、资金费块配平与 2 天延长段,`measure.toml` 的段写法要定下来。
- **T16(hft-core)**:RF6 的 `RefData::warnings()` 以什么 Note 或告警报 P1 还没写。
- **T10(hft-sim)**:实时 sim-gw 没有受控的故障注入(见第 5 节第 1 条)。
- **T23**:smoke-order ADR 仍写着"在交易所价格带内";development 的两个季度演练可以链接 DRL-110/111。
- **T18**:journal-archiver 验收写"换 AZ 演练的步骤归 launch",实际在 host-upgrade 第 11 步,链接要改。
- **T4**:tools 的 doc-check 表里没有 runbooks README 所说的"每页有步骤与验收节"这项检查。

## 4. 合并遗留处理结果

**已一致**
- J4:IR 第 46、63 行已是按 venue 的说法;本轮另按 RF2 改为只链接 hft-exec。
- J8:release-and-deploy 第 65 行已经链接 hft-journal,不再写 50 ms。
- J9:M7→M8 归档停机标记时 `.aborted` 已包含在 D-23 第 6 小步里。
- J12:host-upgrade 第 22 行(IAM)与第 32 行(`hft/sim/` 前缀)机检已不再报出。

**已改**
- J6:launch 第 94 行删掉"价格带内"(ADR 那一半留给 T23)。
- J9:host-upgrade 的 A/A 与延长(实现部分留给 T19)。
- J9:D-23 第 4 小步的游标说法。
- J9:IR S-3.2 链接到 `restore` 小节(加了锚点)。
- J9:M7 换 AZ 恢复演练。

**不在我名下**:其余 J10 条目(deploy、development、pipeline、ops_bot、ADR)。

## 5. 需要负责人决定

1. **Broken 处置演练没法按计划做**:是否给 sim-gw 加受控的拒单/断连注入(这是新功能)?不加的话,就接受 DRL-12 的"首次出现时演练,未出现则签字"。
2. **我在用例里定了几个原规格没有的容差**:卸载后旧系统回到基线 ±10%(DRL-61)、恢复通知 ≤ 30 s(DRL-26)、季度间隔 ≤ 92 天(DRL-66/110/111)。请确认。
3. **换 AZ 演练的迁回方式**:原实例换一块新冷盘卷再 `restore`,恢复代 `.r2`。是否接受?
4. **testing ⑪ 证据口径**:是"全部 drill 级用例"还是"`门槛` 含 ⑪ 的用例",由 T22 或负责人定。
5. **M5 的 kill 演练**:按测试网账户写的,前提是 dev2 联调窗口与测试网 key 在 M5 已经就绪。

## T21

## T21 最终报告(M0-g)

四份 TESTS.md 都已写完。三项检查在我名下文件上都是零问题:check_tests 0 问题 0 警告;check_hft 无命中;heading_audit 丢失标题为 0。

**命名冲突需要主控处理。** docs/tests §6 要求函数名以用例编号开头,pytest 要求以 `test_` 开头。我统一写成 `test_<前缀小写>_<三位数>_<描述>`。check_tests.py 现在已经放行 `.py` 的这种写法,所以没有报 IMPL。但 §6 的正文,以及 test-trace 第 3 条的正则 `^[a-z]+_[0-9]{3}_`,都还没有 Python 的写法,`test_ops_001_…` 不会被追溯到。这需要主控改 §6 和 test-trace。

### 1. 交付的 TESTS.md

| 文件 | 用例数 | 按级别 |
|---|---|---|
| `E:\trading\hft\python\ops_bot\TESTS.md` | 148(表格子行 542) | table 83、unit 47、integ 9、drill 3、prop 2、golden 2、host 2 |
| `E:\trading\hft\python\pipeline\TESTS.md` | 136(表格子行 300) | table 69、unit 53、integ 7、golden 2、prop 2、proc 1、replay 1、host 1 |
| `E:\trading\hft\crates\hft-py\TESTS.md` | 109 | table 54、unit 26、integ 19、proc 5、prop 2、golden 1、bench 1、host 1 |
| `E:\trading\hft\python\treasury\TESTS.md` | 81 | unit 38、table 30、integ 5、prop 4、host 2、golden 1、drill 1 |

- 四个 README 的"测试与验收"一节都只留标题和一句"用例与验收见 TESTS.md"。这四节下面原本都没有 `###` 子标题。
- 里程碑:ops_bot 为 M5(测试网用例为 M6);pipeline 为 M4;treasury 为 M5(另有 M6、M8 的 host 用例);hft-py 为 M2(和发布 venv 相关的两条为 M4)。

### 2. 规格改动(都在我名下的文件里)

**ops_bot README**
- 紧急停止第 6 步:
  - 停机兜底的各项条件改为逐项从 journal 重建,不漏报、可多报。
  - 执行层的几项链接 hft-exec 的"冷读者重建"表;MarginStress 和 Broken 按 cap 注记判断。
  - "在途 LegLink" = 有未终态的链接订单,或某 group 两腿差超过 `dust_notional`。(OPS-073)
- `Done.marker = false` 时,P0 写明"停机标记未生效",待补 Kill 条目保持 pending。(OPS-077)
- 群聊放行规则:只读面板全部放行;会发 `Annotate` 的命令只能私聊;`/ack` 在群里放行。(OPS-014)
- 角色:`/ack`、`/sub` 归 Trader/Admin,Viewer 不能用。(OPS-013、OPS-018)
- 待补 Kill:
  - 写明 `NoOp` 也算已生效、"同一范围"怎么判断。
  - 写明待补 Kill 文件损坏或缺失时怎么处理。(OPS-093–OPS-100)
- 推送表新增"treasury 划转建议"一行。(OPS-116、OPS-175、OPS-176)
- 另外补了限速窗口、审计码表、退出 2 的条件等。
- 参数输入的单位换算改为经 `hft_py.params.parse_value`,不再自写解析。(OPS-145)

**pipeline README**
- RF10 的 GroupId 改动和 RF11 的 Live 实例链接(见第 4 节)。
- 读取失败的判定:超时、限速返回、解析错误都算失败,不当作空结果;插件 `ok = False` 按同样方式处理。(PIP-031–PIP-033)
- 补写了 `status.json`、`alerts`、`publish.result`、`index.jsonl` 的键名。
- VersionConflict 的处理只有一种结果(PIP-112);预热批次失败只重发一次(PIP-147);交易宇宙候选每个 UTC 日生成一次(PIP-167)。

**hft-py README**
- RF8:冷读者按 hft-ledger 第 7 步对每条 `Note{OmsForeignFill}` 入账。(PY-106–PY-109)
- `params.decode` 既接受 `load_schemas` 的映射,也接受单个 schema,ops_bot 和 pipeline 不用改。(PY-127)
- `encode_cmd` 按命令名编码。(PY-130)
- 新增 `params.parse_value` / `format_value`,包装 SDK 的同名函数。(PY-134)
- 补写:`heartbeat.read` 的返回字段、`config_at` 的口径、Arrow 元数据、错误类型、测试用 cargo feature `testing`。
- 依赖新增 `hft-risk`,只用来解码 `RiskParams`。(PY-088)

**treasury README**
- 定义"在途"和"到账":到账 = 目的位置的交易所报告值与账本值之差小于该笔金额一半。(TRS-023–TRS-025)
- "待转入"位置在 journal 里没有余额,不参与配对。
- 保证金位置的闲置额取 `min(B−R, −D)`。原写法会把可用保证金全部建议移出,使余量跌破目标。(TRS-052)
- 比率交叉校验取 `M(0) = max(rep_maint, rep_ratio × MB)`(TRS-038);已越过的档输出 0(TRS-039)。
- 配对规则改为确定的:平局怎么排、跨所划转按几段计成本与时延。(TRS-060–TRS-068)
- 原文"不推送"与"`renotify_min` 去重"互相矛盾,改为:现金类缺口只报告、不推送,`notify` 字段和 `notify.json` 负责去重。(TRS-067、TRS-082–TRS-085)
- 补写输出结构(`status`、`in_transit`、`day_min_l2` 等)、每个文件旁的 SHA-256、子命令只读、`--move` 的取值范围。
- 配置:新增 `cycle_s` 键;没有默认值的键改为必填;加载时校验阈值次序,并拒绝名字像 key 的键。(TRS-001–TRS-004、TRS-090–TRS-101)

**python/AGENTS.md**:加了 TESTS.md 指针和 pytest 命名规则,篇幅 894/900。

### 3. 留给别人

- **T22**:
  - 在 benchmarks 登记基准名 `py_events`,预算出处为 hft-py 的"延迟与资源契约"(PY-160)。
  - scenarios.md 的 MD-12.7(进程被 SIGSTOP 停住)期望 `stop_core`,但 ops_bot 规则和 runbook 都要求只撤单,需要改 MD-12.7。
- **T3(hft-journal)**:需要提供测试写者 `examples/testwriter.rs`(启用 writer 特性)。hft-py 的 PY-070–PY-080 和 treasury 的 TRS-130/131 都依赖它。
- **T4(tools)**:`deps.toml` 需放行 hft-py → hft-risk。
- **T18(watchtower)**:
  - pipeline 在 `status.json` 里写的 P1/P2 告警目前没有任何组件投递给人。
  - `acks.jsonl` 的格式已在 ops_bot 侧定下,watchtower 侧应链接。
- **主控**:上面说的 §6 与 test-trace 的 Python 命名问题。

### 4. 合并遗留

| 条目 | 结果 |
|---|---|
| RF10 / X2 P2-10:pipeline 的 GroupId | 已改:构造链接 hft-types,续号链接 tools hft-cfg,本包只保留 `groups.json` |
| RF11:pipeline 的"Live 实例" | 已改:只链接 control-plane |
| J6:`next_ctr` 从生效 blob 续号 | 已改 |
| J10 F1(e):移除 group 时提示账户级持仓 | 已一致 |
| J6:ops_bot 兜底的额外条件 | 已一致 |
| J10/J4:这些条件能否从 journal 重建 | 已改:链接 hft-exec 冷读者重建表 |
| J9:ops_bot 处理 `Done.marker = false` | 已改 |
| RF8:hft-py 消费 `OmsForeignFill` | 已改 |
| J5→J9/J11:hft-py 消费 AUTO_RECON 注记 | 已一致 |
| research | 无遗留 |

### 5. 需要负责人决定

1. ops_bot 判"视图停滞"用的阈值是 `watchdog.hb_stale_ms`(1 s),而 journal 里的 `ClockStatus` 正好也是每 1 s 一条,可能误判为停滞。误判只会让兜底偏向只撤单(安全方向)。要不要单独设一个阈值?
2. 两个启动/恢复的取舍:
   - `pending_kill.json` 损坏时,现定为按全部范围补发 `Kill{Global}`。
   - schema 目录损坏时 ops_bot 启动退出 2,这会让老板的 `/stop` 通道不可用。要不要改为退回用 `GetSchema` 继续运行?
3. `/stop … core` 时如果控制面已经恢复,ops_bot 会以老板的同一身份自动发 Kill 的预览和确认,把老板对 `/stop core` 的确认当作本人确认。这样是否可以接受?
4. pipeline 遇到 VersionConflict 时,会把人工 blob 的差异自动导入为该实例的永久覆盖项,这是否可以接受?另外,hft-cfg 复用一个已被移除的 id 时,pipeline 的行为尚未定义。
5. treasury 新增的规则是否符合预期:到账判定、闲置额上限、紧急建议每轮都推送。另外,原测试表里多头的金样(−6.20 / −17.23 / −21.83 %)只在交易所强平价更保守时成立;按"取更保守者"的实际口径应是 −5.80 / −16.89 / −21.50 %。两种情形分别由 TRS-031 和 TRS-032 覆盖。

## T22

我已写完 `docs/tests/scenarios.md` 和 `docs/tests/invariants.md`,改了 testing.md 和 benchmarks。我名下文件的三项检查都清零:`check_tests.py` 的问题 0,`check_hft.py` 在我的文件上 0 条(全仓也是 0 个问题),`heading_audit` 丢失标题 0。

## 1. 写了什么
- **scenarios.md**:106 个用例(dst 70、integ 25、table 5、proc 4、prop 2),另有 327 个变体行(`<编号>.<k>`)。
  - 页首是编号族登记表、公共夹具 W1/W1-SIM、级别与落点、种子集。
  - RC、MD、FP 按简报 §6 编号,其余沿用原编号。
  - 没写成用例、只保留编号的:B-7、FP-13、INV-10(链上可选扩展,里程碑外);未使用、不再分配的:B-9 等和 C3、C5、MS-1。
  - 新增一个编号 MS-2a:MS-2 的上线策略版本,对应门槛 ⑦。
- **invariants.md**:INV-1–9、INV-11–20,共 19 条,级别都是 dst;每条有可编程的断言,检查函数写成 `crates/hft-testkit/src/dst/invariants.rs::inv_<n>_…`。
- **testing.md**:§3.6、§4、§4.1–§4.4、§5 的标题保留,正文各改成一句链接。§8 门槛表新加"证据(用例)"一列,按级别类和编号写明 ①–⑪ 各由哪些用例构成证据。另外更新了页首导语、§3.4,以及 ⑦ 和 §8.1 里的 MS-2a。
- **benchmarks**:README 新增"基准名登记",共 267 个名字(256 个 `bench` 用例加路径基准),写明各自的预算出处和用例编号。表由 `_work\t22tmp\gen_bench_registry.py` 和 `apply_bench_registry.py` 生成,可以重跑。AGENTS.md 加了一条登记规则。

## 2. 规格改动(都在我名下的文件里)
- **INV-11**:原写"期望为 Shadow 时有效模式为 Shadow",与 hft-risk 冲突;改为按 hft-risk:有仓位时是 `CloseOnly{deferred_shadow}`。
- **INV-13**:只限定为引擎不因模式变化发对冲单或补腿单。
- **INV-3、INV-4**:INV-3 的限额取 `max_pos_qty`;INV-4 写明在 `unknown_blocks_open = true` 时成立。
- **MD-2**:`Recon` 是 venue 级 cap,所以 `Resume{DailyLoss}` 后有效模式变为 Live,开仓改由检查链 #3 拒绝。
- **MD-8**:按 pipeline 现文,校准对文件缺 `scope` 时整个文件加载失败;Live 实例按 control-plane 现行定义写。
- **MD-14、C10**:`StorageCritical` 的推迟条件按 RF7 写。
- **MD-24**:预热块按 journal 顺序投递,不按 `chunk_no`。
- **BK-6**:残余改由 refdata 的 `min_qty` 触发,因为运行中 refdata 不变。
- **RC-11**:Broken 计时按 RF2 写。原文"两台主机上恢复的状态哈希相同"做不到,改为"已登记的 TSC 字段以外逐字节相同"。
- **testing §3.5**:日报加 `coalesced` 字段;配对键不再重抄(原文漏了 `account`),改为链接 replayer。
- **benchmarks**:bench-gate 的两条退出码规则(缺预算返回 1,`report` 类超预算不阻断)从"测试与验收"挪到"使用本组件"。

## 3. 留给别人
- **T4**
  - testkit README 的源文件表写的是 `src/dst/suites/`,但全仓用例都落在 `src/scenarios/<主题>.rs`,需要对齐。
  - testkit §6 应链接 invariants.md,并写明检查器 `I<n>` 就是 INV-n。
  - 测试夹具要补的能力:测试策略带一个结构字段(MD-7.6);probe-target 有 `advice_max_age_ms`(AL-1);AdvisorActor 按 `engine_ns` 注入落后(AL-1);能构造不同 `build_git`/版本和可改布局版本的 Model 替身(RC-4、RC-12)。
  - tools 的 TESTS 缺 bench-gate 两个用例:缺预算返回 1,`report` 类超预算返回 0 并在输出里标出。
- **T7(hft-exec)**:多个 `SerialFallback` 原因同时成立时记哪一个(MS-6);场合 1 的 `ExecGateHeldNaked` 的 `v[2]` 填什么(BK-5)。
- **T8(hft-oms)**
  - 超量成交时第二腿与 `Broken{Overfill}` 的先后(B6.1)。
  - HARD 下第二腿的说法与风控检查链 #1 不一致(C8)。
  - Reverse 成交后 `owed` 不变,链接永远到不了 Done(FP-5.1)。
  - `StaleGen` 带成交时"请求对账"的形式没定义(INV-2)。
  - 撤单没写出时槽停在 `PendingCancel`(B-1);CancelReject 回到 Live 时是否清 `CANCEL_SENT`(B-10)。
  - `recovery_end` 时 `t_state` 取哪个值(RC-3.2);`force_unknown` 的范围与 sequencer 写法不一致(RC-5)。
- **T9(hft-risk)**
  - 下架公告与"有效模式"第 2 条互相矛盾(C15)。
  - `MarginStale` 到期时刻的边界(A5.3);链接腿 `RATE` 与 `RATE_BACKOFF` 的优先级(B1.3)。
  - `unknown_blocks_open` 没有默认值(INV-4)。
  - `Resume{Recon}` 不可解除时回什么错误码(MD-2)。
  - 与 T11 一起:干净重启条件 ③ 是否按账户判、是否包含未到达的引擎目标(MD-15)。
- **T11(hft-sequencer)**
  - `Note{RecoverMismatch}` 写几条(C2、RC-5)。
  - 换机后状态哈希的表述(RC-11)。
  - 与 T3 一起:干净 Drain 重启后新段的 `roll_reason`(RC-4、RC-9)。
  - `ModeErr::UnknownPending` 不在结果码映射表里(FP-1.2)。
  - 与 T13 一起:限速拒单后谁产出 `ConnState`、谁写 Note(FP-2)。
- **T5/T6**:strat-lib `Throttle::set_backoff` 需要截止时刻,但 SDK 的 `rate_backoff` 只是布尔值(FP-2.2)。T6 另把 portfolio-carry README:308 的"testing MS-2"改成 MS-2a。
- **T3**:`SameExchangeLegs` 只能由两腿同一 inst 触发,依赖 `check_group` 先逐腿检查的顺序(MD-19.4),请确认。
- **T21**:ops_bot 补发 Kill 得到 `NoOp` 时是否算成功(MD-12)。
- **T10/T17**:`sim/replay_day` 同时被 RPL-251 和 SIM-213 引用,只能保留一个用例。

## 4. §4 合并遗留的处理结果
- **已改**
  - J4:不变量 20、BK-1、BK-5 改为链接 hft-exec 的计时规则(RF2)。
  - J5:AX-1 按 hft-ledger"外部成交"第 6 步消费 `AUTO_RECON` 注记。
  - J8:§3.5 报告加 `coalesced`。
  - RF7、RF11 已落在 MD-14、C10、MD-8。
- **已一致**
  - J1/J11:§8.2 已含分层抽样、`kernel_rx` 上下界、SW 锚和 A/A。
  - J1/J7/J11:benchmarks 已登记 `gw-ingest/poll-cadence`;`gw-egress/idle8` 和按消息类别、分位数分行的解码上限表已一致。
  - J2:§8.2 的探针描述已有 `action` 和跨 gw 变体。
  - J6:`HFT_FUZZ_TIMEOUT_DIR` 由 testing §3.3 定义;信号分类链接 tools 的表。
  - J10:门槛 ⑥ 遇护栏停机按暂停计时;原 :259 的链接现在有效。
  - J11:testing 里已无 `hft-batch.slice`;c16 的说法已一致。
  - RF14:venue-proto 已按 p50/p99 分行,benchmarks 只链接它。

## 5. 需要负责人决定
1. 是否接受把 MS-2 拆成 MS-2(测试策略 DST,M2)和 MS-2a(上线策略、门槛 ⑦,M4)。
2. §6 用了节内参数基线 R6(写在同一页、数值显式),用例前置只写差值;这算不算满足"前置显式设参数"。
3. RC-11 改写了"换机后状态哈希相同"这条原文。
4. MD-12、MD-17 两个 proc 用例需要 GitHub 托管 runner 上 systemd 为 PID 1 并能装 polkit 规则;做不到就改成演练。
5. INV-11、INV-13 的语义收窄,需要你确认。

