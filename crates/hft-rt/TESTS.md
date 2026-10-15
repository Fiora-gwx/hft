---
component: hft-rt
prefix: RT
spec: README.md
---

# hft-rt 测试规格

RT-002/166 验证测量环溢出与零分配，见 [延迟采集测试](../../tools/latency-report/TESTS.md)。

格式与规则见 [docs/tests](../../docs/tests/README.md)。级别在本组件的含义:

- `unit`:任意平台(含开发机)可跑的确定性测试——纯逻辑、注入的时钟读数(`ClockReads`)、`SysRoot` 夹具目录、`loom` 模型(`RUSTFLAGS="--cfg loom"`,每个 PR);标注"aarch64"的 unit 只在 aarch64 编译,在 CI 每夜 `ubuntu-24.04-arm` 专项运行。
- `proc`:依赖 Linux 内核设施(futex、epoll/eventfd、mmap 与 `/dev/shm`、`fork`/`SIGKILL`、`sched_setaffinity`、`mlockall`、`strace` 计数),在 CI `ubuntu-latest` 上运行;无大页、无隔离核。
- `host`:需要目标硬件或主机调优(Graviton 计数器、PHC、hugetlbfs、隔离核、deploy 设置的资源限制),在 M1.5 租用的 `c8g.4xlarge`、dev2 或升级主机上运行。
- `table`/`prop` 用例在 `前置` 写明是否需要 Linux;未写的在任意平台运行。用 `strace` 计数的 `proc` 用例由 CI 作业先安装 `strace`。

记号:`C1G` = `TscClock::from_hz_milli(1_000_000_000_000, base)`(1 GHz,1 tick = 1 ns);环元素除注明外为 `u64` 序号。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| SPSC 环(进程内与共享内存同一实现) | RT-001–RT-010, RT-020–RT-029 |
| 变长字节环与版本交换 | RT-030–RT-036 |
| 所有权交接、值单元与 kill 字 | RT-040–RT-046, RT-050 |
| 版本交换(可选) | 不适用:里程碑外(可选扩展:refdata 热替换启用时补写 `EpochSwap` 的 loom 与基准用例) |
| 时间 | RT-050–RT-060, RT-140, RT-144 |
| CPU、主机检查、内存 | RT-070–RT-088, RT-141–RT-143, RT-145 |
| 等待与存活 | RT-090–RT-099 |
| 线程计划的执行:任务轮转与空闲策略 | RT-110–RT-122 |
| 配置项 | RT-130–RT-131 |
| 实现说明 | RT-008, RT-021–RT-026, RT-033, RT-053, RT-054, RT-073–RT-079, RT-116 |
| 延迟与资源契约 | RT-150–RT-167 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

### RT-001 spsc 只接受 2 的幂且不小于 2 的容量

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)
- 级别:table
- 里程碑:M1
- 前置:`Backing::Heap`;元素 `u64`
- 实现:`crates/hft-rt/tests/spsc.rs::rt_001_capacity_rules`

| 行 | capacity | 期望 |
|---|---|---|
| RT-001.1 | 0 | `Err(RtErr::Capacity)` |
| RT-001.2 | 1 | `Err(RtErr::Capacity)` |
| RT-001.3 | 2 | `Ok` |
| RT-001.4 | 3 | `Err(RtErr::Capacity)` |
| RT-001.5 | 6 | `Err(RtErr::Capacity)` |
| RT-001.6 | 1024 | `Ok` |
| RT-001.7 | 65536 | `Ok` |
| RT-001.8 | 2^62 | `Err(RtErr::NoMem)` |

### RT-002 环满时 try_push 原样退回、环空时 try_pop 返回 None,顺序先进先出

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)
- 级别:unit
- 里程碑:M1
- 前置:`spsc::<u64>(4, Heap)`
- 步骤:
  1. `try_push(1)`…`try_push(4)`;`try_push(5)`
  2. `try_pop()`;`try_push(5)`
  3. 连续 `try_pop()` 五次
- 期望:
  - 步骤 1:前四次 `Ok(())`,第五次 `Err(5)`
  - 步骤 2:`Some(1)`、`Ok(())`
  - 步骤 3:`Some(2)`、`Some(3)`、`Some(4)`、`Some(5)`、`None`
- 实现:`crates/hft-rt/tests/spsc.rs::rt_002_full_empty_fifo`

### RT-003 free_hint 与 len_hint 在单线程下等于真实值

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)
- 级别:unit
- 里程碑:M1
- 前置:`spsc::<u64>(8, Heap)`
- 步骤:
  1. push 3 个;读 `free_hint()`、`len_hint()`
  2. pop 2 个;读 `len_hint()`;再 push 1 个后读 `free_hint()`
- 期望:
  - 步骤 1:5、3
  - 步骤 2:`len_hint() == 1`;`free_hint()` ∈ {4, 6}(生产者可能尚未刷新 `cached_tail`,只允许偏小)
- 实现:`crates/hft-rt/tests/spsc.rs::rt_003_hints`

### RT-004 提示值永不超过真实值

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)
- 级别:prop
- 里程碑:M1
- 性质:任意 push/pop/批量操作序列中,每步 `free_hint() ≤ capacity − 真实长度`、`len_hint() ≤ 真实长度`;按参考队列逐元素相同
- 生成:capacity ∈ {2, 4, 8, 64};1–2 000 步,每步从 {try_push, try_pop, write_batch(k)+commit, read_batch(k)+release, 读提示} 均匀抽取
- 参考:`VecDeque` 有界队列
- 例数:PR 2 000
- 实现:`crates/hft-rt/tests/prop_spsc.rs::rt_004_hints_never_exceed_truth`

### RT-005 write_batch 只在 commit 时一次发布

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`spsc::<u64>(8, Heap)`,环中已有 2 个元素(1、2)
- 步骤:
  1. `let mut b = p.write_batch(10);` 依次 `b.push(10..)` 直到返回 `false`
  2. commit 前消费者 `len_hint()`;`b.commit()` 后消费者依次 pop
  3. 新开 `write_batch(3)`,push 3 个后丢弃(不 commit);消费者 `try_pop()`
- 期望:
  - 步骤 1:前 6 次 `true`、第 7 次 `false`
  - 步骤 2:commit 前 `len_hint() == 2`;之后依次得到 1、2、10…15,再 pop 为 `None`
  - 步骤 3:`None`(未提交的元素从不可见)
- 实现:`crates/hft-rt/tests/spsc.rs::rt_005_write_batch_commit`

### RT-006 read_batch 绕回时给出两段,只在 release 时推进

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)
- 级别:unit
- 里程碑:M1
- 前置:`spsc::<u64>(8, Heap)`;先 push 6 个并 pop 6 个,再 push 100、101、102、103、104(下标 6–10,绕回)
- 步骤:
  1. `read_batch(8).slices()`
  2. 丢弃该批(不 release),再 `read_batch(3).slices()` 后 `release()`
  3. `len_hint()`
- 期望:
  - 步骤 1:`([100, 101], [102, 103, 104])`
  - 步骤 2:`([100, 101], [102])`(未 release 的元素再次交付)
  - 步骤 3:2
- 实现:`crates/hft-rt/tests/spsc.rs::rt_006_read_batch_wrap`

### RT-007 环两端与邮箱两端的 Send/Sync/Clone 约束

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)、[所有权交接、值单元与 kill 字](README.md#所有权交接值单元与-kill-字)、[开发备忘](README.md#开发备忘)
- 级别:table
- 里程碑:M1
- 前置:`static_assertions` 的 `assert_impl_all!` / `assert_not_impl_any!`(编译期)
- 实现:`crates/hft-rt/tests/traits.rs::rt_007_endpoint_traits`

| 行 | 类型 | 期望 |
|---|---|---|
| RT-007.1 | `Producer<u64>` | 实现 `Send`;不实现 `Sync`、`Clone` |
| RT-007.2 | `Consumer<u64>` | 实现 `Send`;不实现 `Sync`、`Clone` |
| RT-007.3 | `MailSender<Box<u64>>`、`MailReceiver<Box<u64>>` | 不实现 `Clone` |
| RT-007.4 | `ByteProducer`、`ByteConsumer` | 不实现 `Clone` |

### RT-008 loom 穷举 push、pop 与批量交错:无未初始化读、恰好一次、保序

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`loom` 模型;capacity 2;生产者线程依次写 1、2、3(一次 `try_push`、一次 `write_batch(2)`+`commit`),消费者线程 `try_pop` 与 `read_batch`+`release` 各一次并循环至收齐
- 步骤:
  1. `loom::model` 穷举全部交错
- 期望:
  - 每个交错中消费者收到的序列恰为 1、2、3,之后 `try_pop` 为 `None`;没有读到未发布的槽(每个槽的读写登记在 `loom::cell::UnsafeCell` 上,读与其写入之间没有先行关系即由 loom 报错)
- 实现:`crates/hft-rt/tests/loom_spsc.rs::rt_008_loom_spsc`

### RT-009 两线程压力:带序号元素连续无缺无重

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)
- 级别:unit
- 里程碑:M1
- 前置:生产者与消费者各一个线程;批量大小每次在 1–64 随机;capacity 依次取 2、16、1024、65536;元素数:PR 10^8(x86_64),每夜 arm 专项 10^9
- 步骤:
  1. 生产者按 0, 1, 2… 写入;消费者逐个读出并校验
- 期望:
  - 消费者读到的第 i 个元素 == i,总数等于写入数;无超时(每个 capacity 一轮 ≤ 120 s)
- 实现:`crates/hft-rt/tests/stress.rs::rt_009_two_thread_stress`

### RT-010 无唤醒挂钩的生产端在 commit 时不发系统调用

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)、[线程计划的执行](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:proc
- 里程碑:M1
- 前置:`spsc::<u64>(1024, Heap)`;生产端 `set_wake(None)`;生产线程以 `strace -f -c -e trace=futex,write,epoll_wait` 包裹
- 步骤:
  1. 生产线程 push 并 commit 10^5 个元素(消费者在另一线程忙等读取)
- 期望:
  - 生产线程的 `futex`、`write` 调用次数均为 0
- 实现:`crates/hft-rt/tests/proc_wake.rs::rt_010_no_wake_no_syscall`

### RT-020 共享内存环在两个进程间按序收发

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)、[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M1
- 前置:路径 `/dev/shm/hft-test-<pid>/ring`;`PageSize::Normal`;元素 `[u64; 8]`(64 B);`capacity = 1024`;`layout_id = 0x1234`
- 步骤:
  1. 父进程 `ShmSpsc::create(…, RingSide::Producer, 0x1234)`;`fork` 子进程 `attach(…, RingSide::Consumer, 0x1234)`
  2. 父进程写 10 000 个元素(首字为序号);子进程读完后以退出码报告结果
- 期望:
  - 子进程读到 0–9 999 连续,退出码 0;环头 `elem_size == 64`、`capacity == 1024`、`layout_id == 0x1234`、`state == Ready`
- 实现:`crates/hft-rt/tests/proc_shm.rs::rt_020_shm_round_trip`

### RT-021 共享内存环附着的每个错误

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:同 RT-020 创建的环(state = Ready,生产端被父进程占用);各行在子进程中执行 `attach`,行内说明对环头的改动(测试直接写文件)
- 实现:`crates/hft-rt/tests/proc_shm.rs::rt_021_attach_errors`

| 行 | 条件 | 期望 |
|---|---|---|
| RT-021.1 | `layout_id = 0x9999` | `Err(ShmErr::LayoutMismatch)` |
| RT-021.2 | 以 32 B 元素类型附着 | `Err(ShmErr::LayoutMismatch)` |
| RT-021.3 | 环头 `version` 改为 2 | `Err(ShmErr::VersionMismatch)` |
| RT-021.4 | 环头 magic 改为 `*b"XXXXXXXX"` | `Err(ShmErr::BadMagic)` |
| RT-021.5 | 路径不存在 | `Err(ShmErr::Io)` |
| RT-021.6 | 环头 `state` 改为 Creating(0) | `Err(ShmErr::NotReady)`(立即返回) |
| RT-021.7 | 环头 `state` 改为 Closed(2) | `Err(ShmErr::Closed)` |
| RT-021.8 | 以 `RingSide::Producer` 附着(父进程仍在) | `Err(ShmErr::SideTaken)` |
| RT-021.9 | 以 `RingSide::Consumer` 附着(对照行) | `Ok` |

### RT-022 生产者被 SIGKILL 后消费者只见已提交元素,重启的生产者从共享 head 继续

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)、[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M1
- 前置:父进程创建环并作消费者;子进程 A 附着为生产者,以每批 100 个、序号连续的 `write_batch`+`commit` 写入,在第 51 批写入 50 个元素后、`commit` 前通知父进程并挂起
- 步骤:
  1. 父进程 `SIGKILL` A;读空环
  2. 子进程 B 附着为生产者(A 的 pid 行已死,可接管),写序号 1 000 000–1 000 099 并 commit
  3. 父进程读空环
- 期望:
  - 步骤 1 读到 0–4 999,第 51 批的 50 个元素不可见
  - 步骤 2 的 `attach` 返回 `Ok`
  - 步骤 3 读到 1 000 000–1 000 099,紧接在 4 999 之后
- 实现:`crates/hft-rt/tests/proc_shm.rs::rt_022_producer_killed`

### RT-023 消费者被 SIGKILL 后重连收到未 release 的元素

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)
- 级别:proc
- 里程碑:M1
- 前置:父进程创建环并作生产者,写入 0–199;子进程 A 附着为消费者,`read_batch(100)` 读到 0–99 后不 release,通知父进程并挂起
- 步骤:
  1. 父进程 `SIGKILL` A;子进程 B 附着为消费者并读空环
- 期望:
  - B 读到 0–199(0–99 再次交付,至少一次)
- 实现:`crates/hft-rt/tests/proc_shm.rs::rt_023_consumer_killed_redelivers`

### RT-024 对端 pid 行按 (pid, 启动时间) 判定能否接管

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:父进程创建环并作生产者;测试直接改写环头行 2(consumer 行)后在子进程以 Consumer 附着
- 实现:`crates/hft-rt/tests/proc_shm.rs::rt_024_takeover_by_pid_start`

| 行 | consumer 行内容 | 期望 |
|---|---|---|
| RT-024.1 | pid = 0 | `Ok` |
| RT-024.2 | pid = 一个已退出进程的 pid | `Ok` |
| RT-024.3 | pid = 父进程 pid,start = 父进程真实启动时间 | `Err(ShmErr::SideTaken)` |
| RT-024.4 | pid = 父进程 pid,start = 真实值 + 1(pid 复用) | `Ok` |

### RT-025 创建者重建后对端检测到替换并重新附着

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)、[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M1
- 前置:子进程 C 创建环(生产者),父进程附着为消费者;记下 `peer().epoch` 为 e0
- 步骤:
  1. `SIGKILL` C;父进程调用 `replaced()`
  2. 子进程 C' 在同一路径 `create`;父进程调用 `replaced()`,随后重新 `attach`
  3. C' 写 10 个元素
- 期望:
  - 步骤 1:`replaced() == false`(文件仍是原 inode)
  - 步骤 2:`replaced() == true`;新 `attach` 返回 `Ok`,`peer().epoch == e0 + 1`
  - 步骤 3:父进程读到这 10 个元素
- 实现:`crates/hft-rt/tests/proc_shm.rs::rt_025_creator_rebuild`

### RT-026 epoch 取同名旧文件的 epoch 加 1,否则为 1

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:每行先在路径上放置文件(或不放),再 `create`,读自己的环头 `epoch`
- 实现:`crates/hft-rt/tests/proc_shm.rs::rt_026_epoch_rule`

| 行 | 旧文件 | 期望 epoch |
|---|---|---|
| RT-026.1 | 不存在 | 1 |
| RT-026.2 | 合法环头,epoch = 7 | 8 |
| RT-026.3 | 4 096 字节随机内容(magic 无效) | 1 |

### RT-027 peer 读对端存活信息,干净退出与关闭后 alive 为 false

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)、[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M1
- 前置:父进程创建环(生产者),子进程附着(消费者)
- 步骤:
  1. 子进程调用 `beat()` 3 次;父进程读 `peer()`
  2. 子进程干净退出(释放本方 pid 行);父进程读 `peer()`
  3. 新子进程附着为消费者;父进程写 `state = Closed`(干净关闭);子进程读 `peer()`
- 期望:
  - 步骤 1:`alive == true`、`pid` = 子进程 pid、`last_beat == 3`
  - 步骤 2:`alive == false`
  - 步骤 3:`alive == false`
- 实现:`crates/hft-rt/tests/proc_shm.rs::rt_027_peer_status`

### RT-028 共享内存环元素类型的大小与对齐限制在编译期检查

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)
- 级别:table
- 里程碑:M1
- 前置:`trybuild` 编译用例
- 实现:`crates/hft-rt/tests/compile_fail.rs::rt_028_shm_elem_constraints`

| 行 | 元素类型 | 期望 |
|---|---|---|
| RT-028.1 | `[u8; 4]`(4 B) | 编译失败 |
| RT-028.2 | `[u64; 1]`(8 B) | 编译通过 |
| RT-028.3 | `[u64; 512]`(4 KiB) | 编译通过 |
| RT-028.4 | `[u64; 513]` | 编译失败 |
| RT-028.5 | `[u8; 12]`(非 8 的倍数) | 编译失败 |
| RT-028.6 | 16 B 对齐的结构 | 编译失败 |
| RT-028.7 | 带数据的 enum(非 `FromBytes`) | 编译失败 |

### RT-029 hugetlbfs 上创建共享内存环

- 规则:[SPSC 环](README.md#spsc-环进程内与共享内存同一实现)、[开发备忘](README.md#开发备忘)
- 级别:host
- 里程碑:M1.5
- 环境:M1.5 租用的 `c8g.4xlarge`,deploy 调优已应用,`/dev/hugepages2M` 已挂载
- 授权:`[需授权]`(租用实例由负责人开机)
- 步骤:
  1. 以测试二进制在 `rt.shm.dir` 下 `ShmSpsc::create(…, PageSize::Huge2M, …)`(临时名 → `rename`)
  2. 另一进程 `attach` 并收发 10 000 个元素
- 通过标准:步骤 1 返回 `Ok`(hugetlbfs 支持 `rename`);步骤 2 收齐且有序;若 `rename` 返回 `EINVAL`,按开发备忘的退路改实现并在本用例记录
- 证据:`tests/host/RT-029.log`(命令输出与内核版本),归档到 M1.5 探针的 Agent Note
- 实现:`crates/hft-rt/tests/host/RT-029.sh`

### RT-030 ByteSpsc 只接受 2 的幂且不小于 64 字节的容量

- 规则:[变长字节环与版本交换](README.md#变长字节环与版本交换)
- 级别:table
- 里程碑:M1
- 前置:`Backing::Heap`
- 实现:`crates/hft-rt/tests/bytes.rs::rt_030_byte_capacity`

| 行 | capacity_bytes | 期望 |
|---|---|---|
| RT-030.1 | 32 | `Err(RtErr::Capacity)` |
| RT-030.2 | 63 | `Err(RtErr::Capacity)` |
| RT-030.3 | 64 | `Ok` |
| RT-030.4 | 100 | `Err(RtErr::Capacity)` |
| RT-030.5 | 262 144 | `Ok` |

### RT-031 ByteSpsc 记录按原样交付,零长度记录合法

- 规则:[变长字节环与版本交换](README.md#变长字节环与版本交换)
- 级别:unit
- 里程碑:M1
- 前置:`byte_spsc(256, Heap)`
- 步骤:
  1. `reserve(10)` 写入 0..10,`commit`;`reserve(0)`,`commit`
  2. 消费者 `peek()`;`advance()`;`peek()`;`advance()`;`peek()`
- 期望:
  - 步骤 1:第一次 `reserve` 返回长度 10 的切片;第二次返回长度 0 的切片
  - 步骤 2:`Some([0..10])`、`Some([])`、`None`
- 实现:`crates/hft-rt/tests/bytes.rs::rt_031_records_round_trip`

### RT-032 ByteSpsc 单条上限为容量的四分之一,满时 reserve 返回 None

- 规则:[变长字节环与版本交换](README.md#变长字节环与版本交换)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`byte_spsc(256, Heap)`(上限 64 B)
- 步骤:
  1. 空环上 `reserve(65)`;`reserve(64)` 后 `commit`
  2. 反复 `reserve(56)`+`commit` 直到 `reserve` 返回 `None`;消费者 `advance` 一条;再 `reserve(56)`
- 期望:
  - 步骤 1:`None`、`Some`(长度 64)
  - 步骤 2:返回 `None` 之前成功的次数为 2(64 + 8 头与 2 × (56 + 8) 共 200 字节,第三条放不下);消费者前进后 `reserve(56)` 返回 `Some`
- 实现:`crates/hft-rt/tests/bytes.rs::rt_032_size_limit_and_full`

### RT-033 尾部放不下时写绕回标记,消费者看不到标记

- 规则:[变长字节环与版本交换](README.md#变长字节环与版本交换)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`byte_spsc(256, Heap)`;每条记录长 48 B(占 56 B)
- 步骤:
  1. 写 r1–r4(共 224 B)并 commit;消费者读并 advance r1、r2
  2. 写 r5(尾部只剩 32 B,写绕回标记后从 0 开始)并 commit
  3. 消费者依次 `peek`/`advance` 至 `None`
- 期望:
  - 步骤 2 的 `reserve(48)` 返回 `Some`
  - 步骤 3 依次得到 r3、r4、r5 的内容,随后 `None`;没有长度为绕回标记的记录被交付
- 实现:`crates/hft-rt/tests/bytes.rs::rt_033_wrap_marker`

### RT-034 未提交的预留被覆盖,无预留时 commit 为空操作

- 规则:[变长字节环与版本交换](README.md#变长字节环与版本交换)
- 级别:unit
- 里程碑:M1
- 前置:`byte_spsc(256, Heap)`
- 步骤:
  1. `commit()`(无预留);消费者 `peek()`
  2. `reserve(8)` 写 `[1; 8]`;`reserve(4)` 写 `[2; 4]`;`commit()`;消费者 `peek()`,`advance()`,`peek()`
- 期望:
  - 步骤 1:`None`
  - 步骤 2:`Some([2, 2, 2, 2])`,随后 `None`(第一次预留从未发布)
- 实现:`crates/hft-rt/tests/bytes.rs::rt_034_reserve_overwrite`

### RT-035 ByteSpsc 与参考队列逐条相同且空环总能前进

- 规则:[变长字节环与版本交换](README.md#变长字节环与版本交换)
- 级别:prop
- 里程碑:M1
- 性质:任意 reserve/commit/peek/advance 序列后消费者看到的记录序列与参考相同;环为空时任何 `len ≤ capacity/4` 的 `reserve` 返回 `Some`
- 生成:capacity ∈ {64, 256, 4096};1–5 000 步;记录长度 0–capacity/4 均匀
- 参考:`VecDeque<Vec<u8>>` 加字节占用模型(头 8 B、8 B 对齐、绕回)
- 例数:PR 2 000
- 实现:`crates/hft-rt/tests/prop_bytes.rs::rt_035_bytes_match_model`

### RT-036 ByteSpsc 两线程变长记录压力

- 规则:[变长字节环与版本交换](README.md#变长字节环与版本交换)
- 级别:unit
- 里程碑:M1
- 前置:`byte_spsc(262_144, Heap)`;生产与消费各一线程;10^5 条记录(每夜 arm 专项 10^6),长度 0–65 536 随机,内容为序号派生的伪随机字节
- 步骤:
  1. 生产者写入;消费者逐条校验长度与内容
- 期望:
  - 全部记录按序、长度与内容一致,总条数相等
- 实现:`crates/hft-rt/tests/stress.rs::rt_036_bytes_stress`

### RT-040 Mailbox 槽满时原值退回,取后为空

- 规则:[所有权交接、值单元与 kill 字](README.md#所有权交接值单元与-kill-字)
- 级别:unit
- 里程碑:M1
- 前置:`let mb: &'static Mailbox<Box<u64>> = Box::leak(Box::new(Mailbox::new()));` `mb.split()` 得到发送端与接收端
- 步骤:
  1. `try_send(Box::new(1))`;`try_send(Box::new(2))`
  2. `try_take()`;`try_take()`
  3. `try_send(Box::new(2))`;`try_take()`
- 期望:
  - 步骤 1:`Ok(())`;`Err(b)` 且 `*b == 2`
  - 步骤 2:`Some(1)`;`None`
  - 步骤 3:`Ok(())`;`Some(2)`
- 实现:`crates/hft-rt/tests/mailbox.rs::rt_040_mailbox_send_take`

### RT-041 loom 穷举 Mailbox 收发:每个值恰好被取走一次

- 规则:[所有权交接、值单元与 kill 字](README.md#所有权交接值单元与-kill-字)
- 级别:unit
- 里程碑:M1
- 前置:`loom` 模型;值类型带 `Drop` 计数;发送线程依次尝试发送 v1、v2(失败的原样收回),接收线程循环 `try_take` 至收齐
- 步骤:
  1. `loom::model` 穷举全部交错
- 期望:
  - 每个交错中 v1、v2 各被取走一次、`Drop` 各一次;从不取到空槽的内容
- 实现:`crates/hft-rt/tests/loom_mailbox.rs::rt_041_loom_mailbox`

### RT-042 loom 穷举 SeqCell:读者从不看到撕裂的值

- 规则:[所有权交接、值单元与 kill 字](README.md#所有权交接值单元与-kill-字)
- 级别:unit
- 里程碑:M1
- 前置:`loom` 模型;`SeqCell<(u64, u64)>` 初值 (0, 0);一个写线程依次 `store((1, 1))`、`store((2, 2))`;一个读线程做两次单次尝试 `try_load`(`load` = 重复 `try_load` 直到 `Some`;重试循环会让 loom 无限读旧值,两个读线程穷举 > 10 min);两线程结束后主线程 `load`
- 步骤:
  1. `loom::model` 穷举全部交错
- 期望:
  - 每次返回 `Some` 的结果两半相等;两次都为 `Some` 时值不递减;主线程 `load()` 为 (2, 2)
- 实现:`crates/hft-rt/tests/loom_seqcell.rs::rt_042_loom_seqcell`

### RT-043 SeqCell 单线程存取

- 规则:[所有权交接、值单元与 kill 字](README.md#所有权交接值单元与-kill-字)
- 级别:unit
- 里程碑:M1
- 前置:`SeqCell<TscUtc>` 初值 `TscUtc { base_tsc: 0, base_utc_ns: 0, mult: 1 << 32, shift: 32, err_ns: 0 }`
- 步骤:
  1. `store(v)`,其中 `v.base_tsc = 5`、`v.err_ns = 7`;`load()`
- 期望:
  - `load()` 的各字段与 v 相同
- 实现:`crates/hft-rt/tests/seqcell.rs::rt_043_seqcell_store_load`

### RT-044 KillWord 是 128 字节对齐的 AtomicU32

- 规则:[所有权交接、值单元与 kill 字](README.md#所有权交接值单元与-kill-字)
- 级别:unit
- 里程碑:M1
- 前置:无参数
- 步骤:
  1. 读取 `size_of::<KillWord>()`、`align_of::<KillWord>()`;编译 `let _: &AtomicU32 = &kw.0;`
- 期望:
  - 128、128;编译通过(元素宽度 32 位)
- 实现:`crates/hft-rt/tests/layout.rs::rt_044_killword_layout`

### RT-045 loom 穷举 KillWord 写读:读到的总是某次完整写入

- 规则:[所有权交接、值单元与 kill 字](README.md#所有权交接值单元与-kill-字)
- 级别:unit
- 里程碑:M1
- 前置:`loom` 模型;初值 0;写线程 `store(0x0010_0107, Release)` 后 `store(0x8000_0004, Release)`;读线程 `load(Acquire)` 两次
- 步骤:
  1. `loom::model` 穷举全部交错
- 期望:
  - 每次读到的值 ∈ {0, 0x0010_0107, 0x8000_0004};同一读线程第二次读到的值在写入顺序上不早于第一次
- 实现:`crates/hft-rt/tests/loom_kill.rs::rt_045_loom_killword`

### RT-046 KillWord 多线程反复写读不出现未写过的值

- 规则:[所有权交接、值单元与 kill 字](README.md#所有权交接值单元与-kill-字)
- 级别:unit
- 里程碑:M1
- 前置:写线程在 {0, 0x0000_0100, 0x0010_0000, 0xFFFF_FFFF} 间循环写 10^7 次(每夜 arm 专项 10^8);读线程同时读同样次数
- 步骤:
  1. 读线程记录每个读到的值
- 期望:
  - 读到的值全部属于写入集合
- 实现:`crates/hft-rt/tests/stress.rs::rt_046_killword_threads`

### RT-050 TscUtc 按校准对把 TSC 换成 UTC,早于基准的读数给出更早的时刻

- 规则:[所有权交接、值单元与 kill 字](README.md#所有权交接值单元与-kill-字)、[时间](README.md#时间)
- 级别:table
- 里程碑:M1
- 前置:`base_tsc = 1000`、`base_utc_ns = 1_791_072_000_000_000_000`、`shift = 32`
- 实现:`crates/hft-rt/tests/time.rs::rt_050_tsc_utc`

| 行 | mult | t | 期望 `utc_ns(t)` |
|---|---|---|---|
| RT-050.1 | 2^32(1 GHz) | 2000 | 1_791_072_000_000_001_000 |
| RT-050.2 | 2^32 | 500 | 1_791_071_999_999_999_500 |
| RT-050.3 | 2^32 | 1000 | 1_791_072_000_000_000_000 |
| RT-050.4 | 40 × 2^32(25 MHz) | 1025 | 1_791_072_000_000_001_000 |

### RT-051 from_hz_milli 的乘数与 delta_ns 的换算、饱和

- 规则:[时间](README.md#时间)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`base` 任意;`clock = TscClock::from_hz_milli(hz_milli, base)`
- 实现:`crates/hft-rt/tests/time.rs::rt_051_delta_ns`

| 行 | hz_milli | 调用 | 期望 |
|---|---|---|---|
| RT-051.1 | 1_000_000_000_000(1 GHz) | `delta_ns(0, 1000)` | 1000 |
| RT-051.2 | 25_000_000_000(25 MHz) | `delta_ns(0, 25)` | 1000(mult = 171_798_691_840) |
| RT-051.3 | 3_000_000_000_000(3 GHz) | `delta_ns(0, 3_000_000_000)` | 999_999_999(mult = 1_431_655_765,向下截断) |
| RT-051.4 | 1_000_000_000_000 | `delta_ns(100, 50)` | 0(to < from) |
| RT-051.5 | 1_000_000_000_000 | `delta_ns(0, u64::MAX)` | `u64::MAX`(u128 中间量不溢出) |
| RT-051.6 | 1_000_000_000_000 | `delta_ns(7, 7)` | 0 |

### RT-052 to_utc_ns 以 base 校准对换算

- 规则:[时间](README.md#时间)
- 级别:table
- 里程碑:M1
- 前置:`C1G`,`base = CalibPair { tsc: 1000, realtime_ns: 1_791_072_000_000_000_000, phc_ns: 0, err_ns: 50 }`
- 实现:`crates/hft-rt/tests/time.rs::rt_052_to_utc_ns`

| 行 | t | 期望 |
|---|---|---|
| RT-052.1 | 1500 | 1_791_072_000_000_000_500 |
| RT-052.2 | 0 | 1_791_071_999_999_999_000 |
| RT-052.3 | 1000 | 1_791_072_000_000_000_000 |

### RT-053 calibrate_with 取各轮中位频率并判定不稳定与标称不符

- 规则:[时间](README.md#时间)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`CalibConfig { rounds: 5, samples: 8, phc_samples: 10, cpuid_tolerance_ppm: 100, step_threshold_ns: 50_000 }`;注入的 `ClockReads`:`mono_raw_ns` 与 `realtime_ns` 每读一次前进 50 ns,`tsc` = ⌊单调时钟 × 该轮频率 / 10^12⌋,`sleep_ms(200)` 使单调时钟前进 2 × 10^8 − 400 ns 并被计数(每组 8 次读数前进 400 ns,同一轮两组的首个读数恰相隔 2 × 10^8 ns,各行频率下 Δtsc 为整数)
- 实现:`crates/hft-rt/tests/calib.rs::rt_053_calibrate_with`

| 行 | 各轮频率(hz_milli) | nominal_hz_milli | 期望 |
|---|---|---|---|
| RT-053.1 | 5 轮均为 2_500_000_000_000 | `None` | `Ok`,`hz_milli == 2_500_000_000_000`;`sleep_ms` 被调用 5 次 |
| RT-053.2 | 2_500_000_500_000、2_499_999_500_000、2_500_000_000_000、2_500_000_100_000、2_499_999_900_000 | `None` | `Ok`,`hz_milli == 2_500_000_000_000`(中位;离散 0.4 ppm) |
| RT-053.3 | 4 轮 2_500_000_000_000,1 轮 2_500_002_500_000(离散恰 1 ppm) | `None` | `Ok` |
| RT-053.4 | 4 轮 2_500_000_000_000,1 轮 2_500_002_600_000(离散 1.04 ppm) | `None` | `Err(ClockErr::Unstable)` |
| RT-053.5 | 5 轮 2_500_250_000_000 | `Some(2_500_000_000_000)` | `Ok`(偏差恰 100 ppm) |
| RT-053.6 | 5 轮 2_500_252_500_000 | `Some(2_500_000_000_000)` | `Err(ClockErr::Mismatch)`(101 ppm) |
| RT-053.7 | 5 轮 2_499_750_000_000 | `Some(2_500_000_000_000)` | `Ok`(−100 ppm) |
| RT-053.8 | 5 轮 2_500_000_000_000 | `Some(2_400_000_000_000)` | `Err(ClockErr::Mismatch)` |

### RT-054 sample_pair_with 取窗口最窄的一次并合并 PHC 误差界

- 规则:[时间](README.md#时间)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`hz_milli = 1_000_000_000_000`(1 tick = 1 ns);`samples = 8`;注入的读数按"读 TSC → 读 REALTIME → 读 TSC"给出,8 次的窗口宽(ticks)依次为 500、300、120、900、120、700、800、1000;第 3 次的读数为 `(10_000, R, 10_120)`,R = 1_791_072_000_000_000_000
- 实现:`crates/hft-rt/tests/calib.rs::rt_054_sample_pair_with`

| 行 | PHC 读数 | 期望 CalibPair |
|---|---|---|
| RT-054.1 | 无 | `{ tsc: 10_060, realtime_ns: R, phc_ns: 0, err_ns: 60 }`(同宽 120 取较早的第 3 次) |
| RT-054.2 | `PhcOffset { phc_ns: R + 37, sys_ns: R + 5, err_ns: 40 }` | `{ tsc: 10_060, realtime_ns: R, phc_ns: R + 32, err_ns: 100 }` |
| RT-054.3 | 无;第 3 次窗口改为 121(读数 `(10_000, R, 10_121)`),其余不变 | 选第 5 次(120);若第 5 次读数为 `(20_000, R', 20_120)` 则 `{ tsc: 20_060, realtime_ns: R', err_ns: 60 }` |
| RT-054.4 | 无;8 次窗口都为 121,第 1 次读数 `(5_000, R, 5_121)` | `{ tsc: 5_060, err_ns: 61 }`(中点向下取整、误差向上取整) |

### RT-055 DriftTracker 先 Unsynced 两次,跳变超过阈值判 Stepped 并重新开始

- 规则:[时间](README.md#时间)
- 级别:table
- 里程碑:M1
- 前置:`DriftTracker::new(50_000)`;第 k 个校准对 `tsc = k × 10^9`、`realtime_ns = B + k × 10^9 + 偏移`(B = 1_791_072_000_000_000_000);每行是一个新的 tracker 上的推入序列,期望写每次 `push` 的返回
- 实现:`crates/hft-rt/tests/calib.rs::rt_055_drift_tracker`

| 行 | 各对的偏移(ns) | 期望 |
|---|---|---|
| RT-055.1 | 0、0、0 | `Unsynced`、`Unsynced`、`Synced` |
| RT-055.2 | 0、0、+50_000 | `Unsynced`、`Unsynced`、`Synced`(恰等于阈值) |
| RT-055.3 | 0、0、+50_001 | `Unsynced`、`Unsynced`、`Stepped { jump_ns: 50_001 }` |
| RT-055.4 | 0、0、−50_001 | `Unsynced`、`Unsynced`、`Stepped { jump_ns: -50_001 }` |
| RT-055.5 | 0、0、+50_001、+50_001、+50_001 | `Unsynced`、`Unsynced`、`Stepped { jump_ns: 50_001 }`、`Unsynced`、`Synced` |
| RT-055.6 | 61 个 0,然后 +49_999 | 前两个 `Unsynced`,其余 59 个 `Synced`,最后一个 `Synced` |

### RT-056 tsc_now 与 tsc_ordered 在同一线程内单调不减

- 规则:[时间](README.md#时间)
- 级别:unit
- 里程碑:M1
- 前置:x86_64 或 aarch64 任一平台(读 CPU 计数器,不依赖操作系统)
- 步骤:
  1. 连续调用 `tsc_now()` 10^6 次;连续调用 `tsc_ordered()` 10^6 次
- 期望:
  - 两个序列各自单调不减;最后一个值大于第一个
- 实现:`crates/hft-rt/tests/time.rs::rt_056_tsc_monotonic`

### RT-057 aarch64 上标称频率来自 CNTFRQ_EL0 且非 0

- 规则:[时间](README.md#时间)
- 级别:unit
- 里程碑:M1
- 前置:aarch64(CI 每夜 arm 专项)
- 步骤:
  1. `tsc_hz_nominal()`
- 期望:
  - `Some(f)` 且 `f > 0`
- 实现:`crates/hft-rt/tests/time.rs::rt_057_aarch64_nominal_hz`

### RT-058 PHC 设备不存在时打开失败、采样仍成功且 phc_ns 为 0

- 规则:[时间](README.md#时间)
- 级别:proc
- 里程碑:M1
- 前置:Linux;`C1G`
- 步骤:
  1. `Phc::open(Path::new("/dev/ptp_nonexistent"))`
  2. `sample_pair(&clock, None)`
- 期望:
  - 步骤 1:`Err(ClockErr::Phc(2))`(ENOENT)
  - 步骤 2:`Ok(p)`,`p.phc_ns == 0`,`p.err_ns > 0`,`p.realtime_ns` 与 `clock_gettime(CLOCK_REALTIME)` 相差 < 1 s
- 实现:`crates/hft-rt/tests/proc_time.rs::rt_058_no_phc`

### RT-059 SeqCell 分发的 TscUtc 与 TscClock 对同一读数给出相同 UTC

- 规则:[时间](README.md#时间)、[所有权交接、值单元与 kill 字](README.md#所有权交接值单元与-kill-字)
- 级别:unit
- 里程碑:M1
- 前置:`clock = TscClock::from_hz_milli(25_000_000_000, base)`,`base = CalibPair { tsc: 4_000, realtime_ns: 1_791_072_000_000_000_000, phc_ns: 0, err_ns: 10 }`;`cell.store(clock.tsc_utc(&base))`
- 步骤:
  1. 对 t ∈ {0, 4_000, 4_025, 10^12} 分别计算 `clock.to_utc_ns(t)` 与 `cell.load().utc_ns(t)`
- 期望:
  - 两者逐值相等;t = 4_025 时为 1_791_072_000_000_001_000
- 实现:`crates/hft-rt/tests/time.rs::rt_059_tsc_utc_matches_clock`

### RT-060 时钟换算属性:单调、可加、与 i128 参考一致

- 规则:[时间](README.md#时间)
- 级别:prop
- 里程碑:M1
- 性质:对任意 `hz_milli ∈ [10^9, 5 × 10^12]` 与 `a ≤ b ≤ c`:`delta_ns(a, c)` 与 `delta_ns(a, b) + delta_ns(b, c)` 相差 ≤ 1;`delta_ns` 对 `to` 单调不减;`to_utc_ns(t)` 等于 i128 参考 `base.realtime_ns + ((t − base.tsc) × mult) >> 32`
- 生成:hz 对数均匀;t 在 base.tsc ± 10^15 内均匀
- 参考:i128 计算
- 例数:PR 10 000
- 实现:`crates/hft-rt/tests/prop_time.rs::rt_060_clock_math`

### RT-070 绑核后以 sched_getcpu 复核,非法 CPU 号报错

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:table
- 里程碑:M1
- 前置:Linux;每行在一个新线程里执行
- 实现:`crates/hft-rt/tests/proc_cpu.rs::rt_070_pin_current_thread`

| 行 | cpu | 期望 |
|---|---|---|
| RT-070.1 | 0 | `Ok(())`,随后 `sched_getcpu() == 0` |
| RT-070.2 | 在线 CPU 数 − 1 | `Ok(())`,`sched_getcpu()` 等于该值 |
| RT-070.3 | 255(大于 runner 的最大 CPU 号) | `Err(RtErr::Os(22))`(EINVAL,掩码中没有在线 CPU) |

### RT-071 线程名按 15 字节截断并出现在 /proc

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:table
- 里程碑:M1
- 前置:Linux;每行在一个新线程里调用 `name_current_thread`,再读 `/proc/self/task/<tid>/comm`
- 实现:`crates/hft-rt/tests/proc_cpu.rs::rt_071_thread_name`

| 行 | name | 期望 comm |
|---|---|---|
| RT-071.1 | `"feed-bbo-bn"` | `"feed-bbo-bn"` |
| RT-071.2 | `"abcdefghijklmno"`(15 字节) | `"abcdefghijklmno"` |
| RT-071.3 | `"abcdefghijklmnopqrst"`(20 字节) | `"abcdefghijklmno"` |

### RT-072 HostCheck 各位的位号且两两不重叠

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:table
- 里程碑:M1
- 前置:无参数;另断言 20 个常量两两相与为 0
- 实现:`crates/hft-rt/tests/host.rs::rt_072_host_check_bits`

| 行 | 常量 | 期望 |
|---|---|---|
| RT-072.1 | `CLASS_MISMATCH` | `1 << 0` |
| RT-072.2 | `TSC_NOT_INVARIANT` | `1 << 1` |
| RT-072.3 | `HOT_NOT_ISOLATED` | `1 << 2` |
| RT-072.4 | `WARM_ON_ISOLATED` | `1 << 3` |
| RT-072.5 | `HOT_NOT_NOHZ` | `1 << 4` |
| RT-072.6 | `HOT_NOT_RCU_NOCB` | `1 << 5` |
| RT-072.7 | `IRQ_ON_HOT` | `1 << 6` |
| RT-072.8 | `TASK_ON_HOT` | `1 << 7` |
| RT-072.9 | `HUGEPAGES_SHORT` | `1 << 8` |
| RT-072.10 | `SHM_SHORT` | `1 << 9` |
| RT-072.11 | `MLOCK_FAILED` | `1 << 10` |
| RT-072.12 | `SWAP_ON` | `1 << 11` |
| RT-072.13 | `NUMA_BALANCING_ON` | `1 << 12` |
| RT-072.14 | `TIMER_MIGRATION_ON` | `1 << 13` |
| RT-072.15 | `THP_NOT_MADVISE` | `1 << 16` |
| RT-072.16 | `CNTVCT_TRAPS` | `1 << 17` |
| RT-072.17 | `CPU_NOT_NEOVERSE_V1_COMPAT` | `1 << 18` |
| RT-072.18 | `THP_ON` | `1 << 24` |
| RT-072.19 | `SMT_ON` | `1 << 25` |
| RT-072.20 | `CSTATE_DEEP` | `1 << 26` |

### RT-073 公共主机检查项在夹具上的命中与不命中

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`SysRoot` 指向夹具目录 F0(全部检查不命中):`cpu/online = "0-31"`,32 个不同 `core_id`、1 个 package;`MemTotal: 67108864 kB`;当前架构的不变计数器 clocksource;`isolated = "2-27"`;`nohz_full = "2-27"`;`/proc/cmdline` 含 `rcu_nocbs=1-27`;全部 IRQ 亲和为 `"0"` 或 `"28-31"`;热核与 IRQ 核上无其他任务;1 GiB 页 `free_hugepages = 8`、2 MiB 页 `= 1024`;`VmLck: 1048576 kB`;`/proc/swaps` 只有表头;`numa_balancing = 0`、`timer_migration = 0`;`statvfs` 夹具 `/dev/shm` 可用 4 GiB。`HostNeeds` N0:`class = C32`、`min_cpus = max_cpus = 32`、`min_mem_mib = 61_440`、`isa = 编译目标`、`hugepages_1g = 4`、`hugepages_2m = 512`、`shm_bytes = 0`、`held` 全 0、`warm = {28..31}`、`irq = {1}`;`hot = {2, 3}`、`hk = {0}`。每行只改所列一项;"期望"只写公共位(0–13)
- 实现:`crates/hft-rt/tests/host.rs::rt_073_common_checks`

| 行 | 相对 F0/N0 的改动 | 期望 `bits`(公共位) | 期望 `hard`(公共位) |
|---|---|---|---|
| RT-073.1 | 无 | 0 | 0 |
| RT-073.2 | `online = "0-15"` | `CLASS_MISMATCH` | `CLASS_MISMATCH` |
| RT-073.3 | `online = "0-32"` | `CLASS_MISMATCH` | `CLASS_MISMATCH` |
| RT-073.4 | `MemTotal` = 61 439 MiB | `CLASS_MISMATCH` | `CLASS_MISMATCH` |
| RT-073.5 | `MemTotal` = 61 440 MiB | 0 | 0 |
| RT-073.6 | `need.isa` = 另一种 ISA | `CLASS_MISMATCH` | `CLASS_MISMATCH` |
| RT-073.7 | `class = Metal`、`min_cpus = 24`、`max_cpus = 256`,23 个物理核 | `CLASS_MISMATCH` | `CLASS_MISMATCH` |
| RT-073.8 | 同上,24 个物理核 | 0 | 0 |
| RT-073.9 | clocksource = `"kvm-clock"` | `TSC_NOT_INVARIANT` | `TSC_NOT_INVARIANT` |
| RT-073.10 | `isolated = "3-27"` | `HOT_NOT_ISOLATED` | `HOT_NOT_ISOLATED` |
| RT-073.11 | `isolated = "3-27"`,`class = Dev2`、`min_cpus = max_cpus = 32` | 0(不检查) | 0 |
| RT-073.12 | `warm = {27, 28}` | `WARM_ON_ISOLATED` | `WARM_ON_ISOLATED` |
| RT-073.13 | `warm = {27, 28}`,`class = C4`、`min_cpus = max_cpus = 32` | 0(不检查) | 0 |
| RT-073.14 | `nohz_full = "4-27"` | `HOT_NOT_NOHZ` | 0 |
| RT-073.15 | cmdline `rcu_nocbs=2-27`(IRQ 核 1 不在) | `HOT_NOT_RCU_NOCB` | 0 |
| RT-073.16 | `/proc/irq/45/smp_affinity_list = "2"` | `IRQ_ON_HOT` | 0 |
| RT-073.17 | 一个任务 `processor = 3`、`Cpus_allowed_list = "0-31"` | `TASK_ON_HOT` | 0 |
| RT-073.18 | 一个任务 `processor = 1`(IRQ 核)、`Cpus_allowed_list = "0-31"` | `TASK_ON_HOT` | 0 |
| RT-073.19 | 一个任务 `processor = 29`、`Cpus_allowed_list = "28-31"` | 0 | 0 |
| RT-073.20 | `held.hugepages_1g = 1`,1 GiB `free = 3` | 0(3 ≥ 4 − 1) | 0 |
| RT-073.21 | `held.hugepages_1g = 1`,1 GiB `free = 2` | `HUGEPAGES_SHORT` | `HUGEPAGES_SHORT` |
| RT-073.22 | 2 MiB `free = 511` | `HUGEPAGES_SHORT` | `HUGEPAGES_SHORT` |
| RT-073.23 | 2 MiB `free = 512` | 0 | 0 |
| RT-073.24 | 大页需求 0,`shm_bytes` 1 GiB、`held.shm_bytes` 256 MiB,`/dev/shm` 可用 768 MiB | 0(本实例遗留段已扣除) | 0 |
| RT-073.25 | 同上,`/dev/shm` 可用 767 MiB | `SHM_SHORT` | `SHM_SHORT` |
| RT-073.26 | 同行 24,但 `held.shm_bytes = 0`(遗留段属于另一实例) | `SHM_SHORT` | `SHM_SHORT` |
| RT-073.27 | `VmLck: 0 kB` | `MLOCK_FAILED` | `MLOCK_FAILED` |
| RT-073.28 | `/proc/swaps` 有一条 swap | `SWAP_ON` | 0 |
| RT-073.29 | `numa_balancing = 1` | `NUMA_BALANCING_ON` | 0 |
| RT-073.30 | `timer_migration = 1` | `TIMER_MIGRATION_ON` | 0 |
| RT-073.31 | 1 GiB `free = 0`,`class = Dev2`、`min_cpus = max_cpus = 32`、大页需求 0 | 0 | 0 |

### RT-074 aarch64 主机检查项在夹具上的命中与不命中

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:table
- 里程碑:M1
- 前置:aarch64(CI 每夜 arm 专项);夹具同 RT-073 的 F0,另含 `transparent_hugepage/enabled = "always [madvise] never"` 与包含本二进制编译期全部目标特性的 `/proc/cpuinfo` Features;N0 同 RT-073
- 实现:`crates/hft-rt/tests/host.rs::rt_074_aarch64_checks`

| 行 | 改动 | 期望 bits | 期望 hard |
|---|---|---|---|
| RT-074.1 | 无 | 0 | 0 |
| RT-074.2 | THP = `"[always] madvise never"` | `THP_NOT_MADVISE` | 0 |
| RT-074.3 | THP = `"always madvise [never]"` | `THP_NOT_MADVISE` | 0 |
| RT-074.4 | Features 删去编译期集合中的第一个特性 | `CPU_NOT_NEOVERSE_V1_COMPAT` | `CPU_NOT_NEOVERSE_V1_COMPAT` |
| RT-074.5 | clocksource = `"tsc"` | `TSC_NOT_INVARIANT` | `TSC_NOT_INVARIANT` |
| RT-074.6 | 夹具根(非真实根) | 不含 `CNTVCT_TRAPS`(该项只在真实根上测量) | — |

### RT-075 x86_64 主机检查项在夹具上的命中与不命中

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:table
- 里程碑:M1
- 前置:x86_64(CI PR);夹具同 RT-073 的 F0,另含 clocksource `"tsc"`、cpuinfo flags 含 `constant_tsc nonstop_tsc`、`transparent_hugepage/enabled = "always madvise [never]"`、`smt/active = "0"`、cmdline 含 `idle=poll`;N0 同 RT-073
- 实现:`crates/hft-rt/tests/host.rs::rt_075_x86_checks`

| 行 | 改动 | 期望 bits | 期望 hard |
|---|---|---|---|
| RT-075.1 | 无 | 0 | 0 |
| RT-075.2 | THP = `"[always] madvise never"` | `THP_ON` | 0 |
| RT-075.3 | THP = `"always [madvise] never"` | `THP_ON` | 0 |
| RT-075.4 | `smt/active = "1"` | `SMT_ON` | 0 |
| RT-075.5 | cmdline 去掉 `idle=poll` | `CSTATE_DEEP` | 0 |
| RT-075.6 | cmdline 以 `processor.max_cstate=0` 代替 `idle=poll` | 0 | 0 |
| RT-075.7 | cpuinfo flags 去掉 `nonstop_tsc` | `TSC_NOT_INVARIANT` | `TSC_NOT_INVARIANT` |

### RT-076 check_host 不修改系统,命中项都有人读的说明行

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:unit
- 里程碑:M1
- 前置:RT-073.2、RT-073.14、RT-073.27 三处改动同时施加的夹具;记下夹具目录全部文件的 SHA-256
- 步骤:
  1. `check_host(...)`;重新计算夹具文件哈希
- 期望:
  - `bits == CLASS_MISMATCH | HOT_NOT_NOHZ | MLOCK_FAILED`、`hard == CLASS_MISMATCH | MLOCK_FAILED`
  - `lines` 至少 3 行,依次包含 `CLASS_MISMATCH`、`HOT_NOT_NOHZ`、`MLOCK_FAILED` 字样
  - 夹具文件哈希与调用前完全相同
- 实现:`crates/hft-rt/tests/host.rs::rt_076_report_lines_no_writes`

### RT-077 真实 aarch64 主机上用户态读计数器不陷入

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:proc
- 里程碑:M1
- 前置:aarch64 Linux(CI 每夜 arm 专项);`SysRoot` 指向真实根目录;N0 改为本机的 CPU 数与内存
- 步骤:
  1. `check_host(...)`
- 期望:
  - `bits` 不含 `CNTVCT_TRAPS` 与 `TSC_NOT_INVARIANT`
- 实现:`crates/hft-rt/tests/proc_host.rs::rt_077_cntvct_not_trapping`

### RT-078 HostClass 的码值

已删除:`HostClass` 的定义与编码移到 hft-types(RG6),码值由 TYP-086.16–TYP-086.23 覆盖;本 crate 只 `pub use`。

### RT-079 hard 恰为命中位与主机类 hard 集合的交集

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)、[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M1
- 性质:对任意主机类与任意一组命中条件,`hard == bits ∩ H(class)`,其中 H(dev2/c4) = {CLASS_MISMATCH, TSC_NOT_INVARIANT, SHM_SHORT, MLOCK_FAILED} ∪ 当前 ISA 的 hard 位,H(c8/c16/c32/metal) 另含 HOT_NOT_ISOLATED、WARM_ON_ISOLATED、HUGEPAGES_SHORT;dev2/c4 上 HOT_NOT_ISOLATED 与 WARM_ON_ISOLATED 从不出现在 `bits`
- 生成:主机类均匀;RT-073–RT-075 的每个改动以 30% 概率独立施加
- 参考:按 README 实现说明表计算的期望集合
- 例数:PR 1 000
- 实现:`crates/hft-rt/tests/prop_host.rs::rt_079_hard_is_class_subset`

### RT-085 匿名区域按页取整、预触摸后为 0,1 GiB 页不足时报 NoMem

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:table
- 里程碑:M1
- 前置:Linux CI(无 1 GiB 大页;2 MiB 大页 `nr_overcommit_hugepages = 0`,CI 作业为 journal 用例预留的 2 MiB 页可以存在)
- 实现:`crates/hft-rt/tests/proc_mem.rs::rt_085_anon_region`

| 行 | 调用 | 期望 |
|---|---|---|
| RT-085.1 | `HugeRegion::anon(10_000, Normal, None)`,再 `prefault()` | `Ok`;长度 12 288;全部字节为 0 |
| RT-085.2 | `HugeRegion::anon(1 << 30, Huge1G, None)` | `Err(RtErr::NoMem)` |
| RT-085.3 | `HugeRegion::anon((F + 1) × 2 MiB, Huge2M, None)`,F = 调用前 `hugepages-2048kB/free_hugepages` | `Err(RtErr::NoMem)` |

### RT-086 文件区域的打开模式与预触摸不改内容

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:table
- 里程碑:M1
- 前置:Linux;目录 `/dev/shm/hft-test-<pid>/`;`PageSize::Normal`
- 实现:`crates/hft-rt/tests/proc_mem.rs::rt_086_file_region`

| 行 | 调用 | 期望 |
|---|---|---|
| RT-086.1 | `file(p_new, 8192, Normal, CreateNew)` | `Ok`;文件长度 8192 |
| RT-086.2 | 再次 `file(p_new, 8192, Normal, CreateNew)` | `Err(RtErr::Os(17))`(EEXIST) |
| RT-086.3 | `file(p_missing, 8192, Normal, OpenExisting)` | `Err(RtErr::Os(2))`(ENOENT) |
| RT-086.4 | 4 096 字节的已有文件,`file(p, 8192, Normal, OpenExisting)` | `Err(RtErr::NoMem)` |
| RT-086.5 | 8 192 字节、内容全为 0xAB 的已有文件,`OpenExisting` 后 `prefault()` | 映射与文件内容仍全为 0xAB |

### RT-087 Arena 按类型对齐分配,用尽时报 ArenaFull

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:proc
- 里程碑:M1
- 前置:Linux;`Arena::new(HugeRegion::anon(4096, Normal, None)?)`
- 步骤:
  1. `alloc_slice::<u64>(10, 7)`
  2. `alloc_slice::<u8>(1, 1)`;`alloc_slice::<u64>(1, 9)`
  3. `alloc_slice::<u8>(0, 0)`
  4. `alloc_slice::<u8>(4096, 0)`
- 期望:
  - 步骤 1:长度 10、全部为 7、地址 % 8 == 0
  - 步骤 2:第二个切片地址 % 8 == 0、值为 9,且不与前面的切片重叠
  - 步骤 3:`Ok`,长度 0
  - 步骤 4:`Err(RtErr::ArenaFull)`
- 实现:`crates/hft-rt/tests/proc_mem.rs::rt_087_arena`

### RT-088 锁页上限不足时 lock_all 报 ENOMEM

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:proc
- 里程碑:M1
- 前置:Linux;子进程先 `setrlimit(RLIMIT_MEMLOCK, 64 KiB)`(非特权)
- 步骤:
  1. 子进程调用 `lock_all()` 并以退出码报告结果
- 期望:
  - `Err(RtErr::Os(12))`(ENOMEM)
- 实现:`crates/hft-rt/tests/proc_mem.rs::rt_088_lock_all_limit`

### RT-090 CachePadded 填满 128 字节

- 规则:[等待与存活](README.md#等待与存活)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-rt/tests/layout.rs::rt_090_cache_padded`

| 行 | 类型 | 期望 `(size_of, align_of)` |
|---|---|---|
| RT-090.1 | `CachePadded<u64>` | (128, 128) |
| RT-090.2 | `CachePadded<[u8; 200]>` | (256, 128) |
| RT-090.3 | `CachePadded<AtomicU32>` | (128, 128) |

### RT-091 Heartbeat 是一条 128 字节填充行

- 规则:[等待与存活](README.md#等待与存活)
- 级别:unit
- 里程碑:M1
- 前置:无参数
- 步骤:
  1. 读取 `size_of::<Heartbeat>()`、`align_of`;字段 `count`、`name`、`core`、`kind`、`_pad` 的偏移
- 期望:
  - 128、128;偏移 0、8、24、26、27
- 实现:`crates/hft-rt/tests/layout.rs::rt_091_heartbeat_layout`

### RT-092 Heartbeat 每次 beat 加 1,kind 码值固定

- 规则:[等待与存活](README.md#等待与存活)
- 级别:table
- 里程碑:M1
- 前置:新的 `Heartbeat`(计数 0);行 1 调用 `beat()` 三次后 `read()`;其余行断言 kind 常量
- 实现:`crates/hft-rt/tests/wait.rs::rt_092_heartbeat_beat_and_kinds`

| 行 | 项 | 期望 |
|---|---|---|
| RT-092.1 | 三次 `beat()` 后 `read()` | 3 |
| RT-092.2 | kind Sequencer/Feed/Gw/Persist/Housekeeping | 1/2/3/4/5 |
| RT-092.3 | kind Advisor/Ctl/Ext/ChainWorker/Mixed | 6/7/8/9/10 |

### RT-093 HeartbeatWatch 首次记基线,之后按连续未变次数报 Stalled

- 规则:[等待与存活](README.md#等待与存活)
- 级别:table
- 里程碑:M1
- 前置:新的 `Heartbeat` 与 `HeartbeatWatch::new()`;按行顺序执行
- 实现:`crates/hft-rt/tests/wait.rs::rt_093_heartbeat_watch`

| 行 | 动作 | 期望 `check` 返回 |
|---|---|---|
| RT-093.1 | `check` | `Advanced` |
| RT-093.2 | `check` | `Stalled { for_checks: 1 }` |
| RT-093.3 | `check` | `Stalled { for_checks: 2 }` |
| RT-093.4 | `check` | `Stalled { for_checks: 3 }` |
| RT-093.5 | `beat()` 后 `check` | `Advanced` |
| RT-093.6 | `check` | `Stalled { for_checks: 1 }` |

### RT-094 HeartbeatBlock 与 HkLine 的布局与已发布偏移一致

- 规则:[等待与存活](README.md#等待与存活)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-rt/tests/layout.rs::rt_094_heartbeat_block_layout`

| 行 | 项 | 期望 |
|---|---|---|
| RT-094.1 | `size_of::<HeartbeatBlock>()` | 4096 |
| RT-094.2 | `HeartbeatBlock` 偏移 beat/hk/slots | 0/128/256 |
| RT-094.3 | `HkLine` 偏移 magic/layout_version/pid/instance_id/_rsv/state/n_slots/_p | 0/8/12/16/17/18/19/20 |
| RT-094.4 | `HkLine` 偏移 boot_id/started_utc_ns/hk_utc_ns | 24/40/48 |
| RT-094.5 | `size_of::<HkLine>()` | 56 |

### RT-095 create 建出的心跳块可被另一进程只读映射并看到计数前进

- 规则:[等待与存活](README.md#等待与存活)
- 级别:proc
- 里程碑:M1
- 前置:Linux;路径 `/dev/shm/hft-test-<pid>/heartbeat`;`instance_id = 1`
- 步骤:
  1. 父进程 `HeartbeatBlock::create(path, 1)`;读环头字段与文件权限
  2. fork 子进程 `open_ro(path)`,每 10 ms 读区 0 计数,共 1 s;父进程每 1 ms `sequencer().beat()`
  3. 子进程尝试写映射的第一个字节
- 期望:
  - 步骤 1:magic = `*b"HFTHB\0\x01\0"`、`layout_version = 1`、`pid` = 父进程 pid、`instance_id = 1`、`state = 1`(Starting)、`n_slots = 0`、`boot_id` 等于 `/proc/sys/kernel/random/boot_id` 的 16 字节;文件模式 0640
  - 步骤 2:子进程读到的计数单调递增且最后一次 ≥ 500
  - 步骤 3:子进程被 SIGSEGV 终止(只读映射)
- 实现:`crates/hft-rt/tests/proc_hb.rs::rt_095_create_and_open_ro`

### RT-096 open_ro 拒绝坏 magic、未知版本与缺失文件

- 规则:[等待与存活](README.md#等待与存活)
- 级别:table
- 里程碑:M1
- 前置:Linux;先 `create` 一个合法块,再按行改写文件
- 实现:`crates/hft-rt/tests/proc_hb.rs::rt_096_open_ro_errors`

| 行 | 文件 | 期望 |
|---|---|---|
| RT-096.1 | magic 改为 8 个 0 | `Err(RtErr::BadMagic)` |
| RT-096.2 | `layout_version` 改为 2 | `Err(RtErr::BadVersion(2))` |
| RT-096.3 | 路径不存在 | `Err(RtErr::Os(2))` |
| RT-096.4 | 未改动(对照行) | `Ok` |

### RT-097 claim 最多 30 个槽,名字按 16 字节截断

- 规则:[等待与存活](README.md#等待与存活)
- 级别:proc
- 里程碑:M1
- 前置:Linux;新建的心跳块
- 步骤:
  1. 依次 `claim("t<i>", i, 2)`,i = 0..30
  2. 第 31 次 `claim("x", 0, 2)`
  3. 新块上 `claim("abcdefghijklmnopqrst", 5, 10)`
- 期望:
  - 步骤 1:30 次都返回 `Some`,彼此地址不同、都位于区 2..31;之后 `n_slots == 30`
  - 步骤 2:`None`,`n_slots` 仍为 30
  - 步骤 3:槽内 `name == *b"abcdefghijklmnop"`、`core == 5`、`kind == 10`
- 实现:`crates/hft-rt/tests/proc_hb.rs::rt_097_claim_limits`

### RT-098 set_state 写入的状态被只读映射读到

- 规则:[等待与存活](README.md#等待与存活)
- 级别:table
- 里程碑:M1
- 前置:Linux;父进程 `create`,子进程 `open_ro`;每行父进程 `set_state(s)` 后子进程读 `hk.state`
- 实现:`crates/hft-rt/tests/proc_hb.rs::rt_098_set_state`

| 行 | s | 期望子进程读到 |
|---|---|---|
| RT-098.1 | 2(Warmup) | 2 |
| RT-098.2 | 3(Running) | 3 |
| RT-098.3 | 4(Draining) | 4 |
| RT-098.4 | 5(Stopped) | 5 |

### RT-099 Backoff 先自旋后让出,从不睡眠

- 规则:[等待与存活](README.md#等待与存活)
- 级别:proc
- 里程碑:M1
- 前置:Linux;`Backoff::new(4)`;测试线程以 `strace -f -c -e trace=sched_yield,nanosleep,clock_nanosleep,futex` 计数
- 步骤:
  1. `snooze()` 1 000 次
  2. `reset()`;再 `snooze()` 4 次
- 期望:
  - 步骤 1:`sched_yield` 996 次;`nanosleep`、`clock_nanosleep`、`futex` 0 次
  - 步骤 2:`sched_yield` 无新增
- 实现:`crates/hft-rt/tests/proc_wait.rs::rt_099_backoff_never_sleeps`

### RT-110 一轮按配置顺序每个任务恰好调用一次

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:unit
- 里程碑:M1
- 前置:三个假任务 A、B、C(记录被调用的序号与 now;A 每次都 `did_work = true`,B、C 恒 `false`);`TaskLoop::new([A, B, C], Spin, None, &C1G)`
- 步骤:
  1. `round(100)` 一次
  2. 再 `round(200)`…`round(10_000)` 共 99 次
  3. 换成三个都返回 `false` 的任务,`round(1)`
- 期望:
  - 步骤 1:调用顺序 A、B、C,三者收到的 now 都为 100;返回 `true`
  - 步骤 2:B、C 各被调用 99 次(A 持续有工作不挤占)
  - 步骤 3:返回 `false`
- 实现:`crates/hft-rt/tests/taskloop.rs::rt_110_round_order_and_fairness`

### RT-111 Spin 策略空闲时只 Relax、有工作时 Continue

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:table
- 里程碑:M1
- 前置:`TaskLoop::new(tasks, IdlePolicy::Spin, None, &C1G)`;按行顺序调用 `idle_action(now, did_work)`
- 实现:`crates/hft-rt/tests/taskloop.rs::rt_111_spin_idle`

| 行 | now | did_work | 期望 |
|---|---|---|---|
| RT-111.1 | 0 | false | `Relax` |
| RT-111.2 | 10^9 | false | `Relax`(从不 park) |
| RT-111.3 | 10^9 + 1 | true | `Continue` |

### RT-112 SpinThenPark 在连续空闲达到 spin_us 后才 park

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:table
- 里程碑:M1
- 前置:`IdlePolicy::SpinThenPark { spin_us: 20, park_us: 1000 }`;`C1G`;任务无 `next_deadline`;按行顺序调用 `idle_action`
- 实现:`crates/hft-rt/tests/taskloop.rs::rt_112_spin_then_park`

| 行 | now(ns) | did_work | 期望 |
|---|---|---|---|
| RT-112.1 | 1_000 | false | `Relax`(空闲起点) |
| RT-112.2 | 20_999 | false | `Relax`(空闲 19 999 ns) |
| RT-112.3 | 21_000 | false | `Park { timeout_ns: 1_000_000 }`(空闲恰 20 µs) |
| RT-112.4 | 21_500 | true | `Continue`(空闲起点清除) |
| RT-112.5 | 22_000 | false | `Relax`(新的空闲起点) |
| RT-112.6 | 42_000 | false | `Park { timeout_ns: 1_000_000 }` |

### RT-113 Park 策略一轮空闲即 park

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:table
- 里程碑:M1
- 前置:`IdlePolicy::Park { park_us: 300 }`;`C1G`;任务无 `next_deadline`
- 实现:`crates/hft-rt/tests/taskloop.rs::rt_113_park_policy`

| 行 | now | did_work | 期望 |
|---|---|---|---|
| RT-113.1 | 5_000 | false | `Park { timeout_ns: 300_000 }` |
| RT-113.2 | 6_000 | true | `Continue` |

### RT-114 park 超时取 park_us 与最近任务期限的较小者

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:table
- 里程碑:M1
- 前置:`IdlePolicy::Park { park_us: 1000 }`;`C1G`;两个假任务的 `next_deadline` 见各行;`idle_action(now = 10_000, false)`
- 实现:`crates/hft-rt/tests/taskloop.rs::rt_114_park_timeout_deadline`

| 行 | next_deadline(任务 1, 任务 2) | 期望 |
|---|---|---|
| RT-114.1 | (None, None) | `Park { timeout_ns: 1_000_000 }` |
| RT-114.2 | (Some(60_000), None) | `Park { timeout_ns: 50_000 }` |
| RT-114.3 | (Some(60_000), Some(30_000)) | `Park { timeout_ns: 20_000 }` |
| RT-114.4 | (Some(2_000_000), None) | `Park { timeout_ns: 1_000_000 }` |
| RT-114.5 | (Some(10_000), None)(恰到期) | `Continue`(超时 0 不 park) |
| RT-114.6 | (Some(9_000), None)(已过期) | `Continue` |

### RT-115 持续有输入时 SpinThenPark 从不 park

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:unit
- 里程碑:M1
- 前置:`SpinThenPark { spin_us: 20, park_us: 1000 }`;`C1G`;now 每轮 +1 000 ns
- 步骤:
  1. 10 000 轮,每轮 `idle_action(now, true)`;其中每 100 轮插入一轮 `did_work = false`
- 期望:
  - 全部返回 `Continue` 或 `Relax`,没有一次 `Park`
- 实现:`crates/hft-rt/tests/taskloop.rs::rt_115_busy_never_parks`

### RT-116 loom 穷举唤醒协议:无丢失唤醒,PARKING 时生产者不发系统调用

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`loom` 模型;消费者按协议执行 PARKING → 检查 `has_pending` → CAS 到 PARKED → 等待(模型中的等待以"被唤醒或模型结束"表示)→ RUNNING;生产者执行写环 → `fence(SeqCst)` → 读 state → 按需 swap 与唤醒;唤醒调用以计数器替代
- 步骤:
  1. `loom::model` 穷举全部交错
- 期望:
  - 不存在"环中有数据而消费者停在 PARKED 且未被唤醒"的结束状态
  - 生产者读到 PARKING 的交错中唤醒计数为 0;唤醒计数只在 swap 返回 PARKED 时加 1
- 实现:`crates/hft-rt/tests/loom_wake.rs::rt_116_loom_wake_protocol`

### RT-117 Futex 后端:生产者写入唤醒 park 中的消费者,消费者运行时生产者不发系统调用

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:proc
- 里程碑:M1
- 前置:Linux;消费线程 `TaskLoop`(`Park { park_us: 1_000_000 }`、`Parker::new(Futex)`),任务读 `spsc` 环;生产端 `set_wake(Some(unparker))`;生产线程以 strace 计数 `futex`
- 步骤:
  1. 消费者空闲 100 ms 后(已 park),生产者 push 1 个元素并 commit,记录 TSC
  2. 消费者在忙(环中持续有元素)期间生产者再 commit 10 000 次
- 期望:
  - 步骤 1:消费者在 10 ms 内处理该元素(远小于 1 s 的 park 超时);该次 commit 恰好一次 `futex` 唤醒
  - 步骤 2:生产线程新增 `futex` 调用 0 次
- 实现:`crates/hft-rt/tests/proc_park.rs::rt_117_futex_wake`

### RT-118 Epoll 后端:socket 可读唤醒 park 中的线程

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:proc
- 里程碑:M1
- 前置:Linux;`socketpair`;任务的 `wait_fds` 返回其一端;`Park { park_us: 1_000_000 }`、`Parker::new(Epoll)`
- 步骤:
  1. 线程 park 后 100 ms,从另一端写 1 字节
- 期望:
  - 任务在 10 ms 内的 `poll_once` 中读到该字节
- 实现:`crates/hft-rt/tests/proc_park.rs::rt_118_epoll_socket_wake`

### RT-119 无输入时 park 不早于超时返回,心跳照常前进

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:proc
- 里程碑:M1
- 前置:Linux;`Park { park_us: 5_000 }`、`Parker::new(Futex)`;任务恒空闲;`run` 运行 1 s 后置 stop
- 步骤:
  1. 统计 1 s 内的轮数与心跳计数
- 期望:
  - 轮数 ≤ 201(每次 park 不早于 5 ms 返回);心跳计数等于轮数且 ≥ 50
- 实现:`crates/hft-rt/tests/proc_park.rs::rt_119_park_timeout_bound`

### RT-120 Spin 生产者写给 park 消费者的环不发唤醒,消费者靠超时发现数据

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:proc
- 里程碑:M1
- 前置:Linux;消费者 `Park { park_us: 2_000 }`;生产端 `set_wake(None)`;生产线程以 strace 计数 `futex`、`write`
- 步骤:
  1. 生产者每 50 ms push 1 个元素,共 20 个;消费者记录每个元素被取出与写入的 TSC 差
- 期望:
  - 生产线程 `futex`、`write` 0 次
  - 20 个元素全部被取出,每个延迟 ≤ 2 ms + 5 ms(超时加调度余量)
- 实现:`crates/hft-rt/tests/proc_park.rs::rt_120_spin_producer_no_wake`

### RT-121 run 每轮开头 beat 一次,stop 置位后返回

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)、[等待与存活](README.md#等待与存活)
- 级别:unit
- 里程碑:M1
- 前置:`TaskLoop::new([T], Spin, None, &C1G)`;任务 T 在第 5 次 `poll_once` 时把共享的 `stop` 置为 true
- 步骤:
  1. `run(&stop, &hb)`
- 期望:
  - `run` 返回;`hb.read() == 5`;T 被调用 5 次
- 实现:`crates/hft-rt/tests/taskloop.rs::rt_121_run_beats_and_stops`

### RT-122 has_pending 为真时取消 park

- 规则:[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:proc
- 里程碑:M1
- 前置:Linux;`Park { park_us: 100_000 }`、`Parker::new(Futex)`;任务的 `poll_once` 恒返回 `did_work = false` 并记录调用时的 TSC,`has_pending()` 前 3 次返回 true、之后 false;任务在第 6 次 `poll_once` 置 stop
- 步骤:
  1. `run`,统计相邻两次 `poll_once` 的间隔
- 期望:
  - 第 1→2、2→3、3→4 次的间隔各 < 10 ms(`has_pending` 为真,取消 park);第 4→5 次的间隔 ≥ 100 ms(进入 park 直到超时);`run` 在 1 s 内返回
- 实现:`crates/hft-rt/tests/proc_park.rs::rt_122_pending_cancels_park`

### RT-130 hft-rt 配置项的默认值

- 规则:[配置项](README.md#配置项)
- 级别:unit
- 里程碑:M1
- 前置:无参数
- 步骤:
  1. `RtConfig::default()`
- 期望:
  - `calib.rounds = 5`、`calib.samples = 8`、`calib.phc_samples = 10`、`calib.cpuid_tolerance_ppm = 100`、`calib.step_threshold_ns = 50_000`、`phc_device = "/dev/ptp_ena"`、`shm_dir = "/dev/hugepages2M/hft/rings"`、`stall_checks = 3`、`host_mode = HostMode::Prod`;`validate()` 返回 `Ok`
- 实现:`crates/hft-rt/tests/config.rs::rt_130_defaults`

### RT-131 配置项的边界校验

- 规则:[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1
- 前置:从 `RtConfig::default()` 出发,每行只改一项后 `validate()`
- 实现:`crates/hft-rt/tests/config.rs::rt_131_validate_bounds`

| 行 | 改动 | 期望 |
|---|---|---|
| RT-131.1 | `calib.rounds = 0` | `Err("rt.calib.rounds")` |
| RT-131.2 | `calib.rounds = 1` | `Ok` |
| RT-131.3 | `calib.samples = 0` | `Err("rt.calib.samples")` |
| RT-131.4 | `calib.samples = 1` | `Ok` |
| RT-131.5 | `calib.phc_samples = 0` | `Err("rt.phc.samples")` |
| RT-131.6 | `calib.phc_samples = 1` | `Ok` |
| RT-131.7 | `calib.phc_samples = 25` | `Ok` |
| RT-131.8 | `calib.phc_samples = 26` | `Err("rt.phc.samples")` |
| RT-131.9 | `calib.step_threshold_ns = 0` | `Err("rt.clock.step_threshold_ns")` |
| RT-131.10 | `calib.step_threshold_ns = 1` | `Ok` |
| RT-131.11 | `stall_checks = 0` | `Err("rt.heartbeat.stall_checks")` |
| RT-131.12 | `stall_checks = 1` | `Ok` |
| RT-131.13 | `calib.cpuid_tolerance_ppm = 0` | `Ok` |

### RT-140 目标机器上校准后的 TSC 换算 10 s 累计偏差小于 1 µs

- 规则:[时间](README.md#时间)
- 级别:host
- 里程碑:M1.5
- 环境:M1.5 租用的 `c8g.4xlarge`(之后每个参考机与 dev2 复跑);发布版测试二进制
- 授权:`[需授权]`(租用实例由负责人开机)
- 步骤:
  1. `TscClock::calibrate(&RtConfig::default().calib)`
  2. 读 `(tsc_ordered(), CLOCK_MONOTONIC_RAW)` 一对,睡 10 s,再读一对
- 通过标准:步骤 1 返回 `Ok`(无 `NotInvariant`/`Unstable`/`Mismatch`);`|delta_ns(t0, t1) − (m1 − m0)| < 1 000` ns;`hz_milli` 与 `tsc_hz_nominal()` 的偏差 ≤ 100 ppm
- 证据:`tests/host/RT-140.json`(hz、两对读数、偏差、`CNTFRQ_EL0`),存入 M1.5 探针 Agent Note 的附件
- 实现:`crates/hft-rt/tests/host/RT-140.sh`

### RT-141 dev2 上调优后的主机检查无 hard 位

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:host
- 里程碑:M3
- 环境:dev2(`host.class = dev2`),deploy 调优已应用
- 授权:无
- 步骤:
  1. 以 `hft-core --check` 或测试二进制在真实根上运行 `check_host`(`HostNeeds` 取 dev2 的 prod 实例装配值)
- 通过标准:`hard == 0`;`bits` 只含非 hard 位,逐位列入证据
- 证据:`tests/host/RT-141.json`(bits、hard、lines),归档到 ops-log
- 实现:`crates/hft-rt/tests/host/RT-141.sh`

### RT-142 统一升级主机上调优后的主机检查无 hard 位

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(选定的主机类),`prod` 与 `sim` 两个实例各自的 `HostNeeds`
- 授权:无
- 步骤:
  1. 对两个实例分别运行 `check_host`
- 通过标准:两次 `hard == 0`;`prod` 的 `bits` 不含 `HOT_NOT_ISOLATED`、`WARM_ON_ISOLATED`、`HUGEPAGES_SHORT`、`IRQ_ON_HOT`
- 证据:`tests/host/RT-142.json`,归档到升级主机验收记录
- 实现:`crates/hft-rt/tests/host/RT-142.sh`

### RT-143 dev2 上 lock_all 在 deploy 设定的锁页上限下成功

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:host
- 里程碑:M3
- 环境:dev2,以 hft-core 单元的资源限制(deploy 生成)启动测试二进制
- 授权:无
- 步骤:
  1. `lock_all()`;读 `/proc/self/status` 的 `VmLck` 与 `VmRSS`
- 通过标准:`Ok(())`;`VmLck ≥ VmRSS`
- 证据:`tests/host/RT-143.log`
- 实现:`crates/hft-rt/tests/host/RT-143.sh`

### RT-144 目标机器的计数器频率、PHC 读取与 1 h 漂移

- 规则:[时间](README.md#时间)、[开发备忘](README.md#开发备忘)
- 级别:host
- 里程碑:M1.5
- 环境:M1.5 租用的 `c8g.4xlarge`,`rt.phc.device` 可用
- 授权:`[需授权]`
- 步骤:
  1. 记录 `tsc_hz_nominal()`(`CNTFRQ_EL0`)与 `ls /dev/ptp*`
  2. 以 1 s 节拍运行 1 h:`sample_pair(&clock, Some(&phc))` + `DriftTracker::push`
- 通过标准:`CNTFRQ_EL0 ≠ 0`;1 h 内 `Stepped` 0 次;每对 `err_ns` 的分布(p50/p99/max)写入证据;PHC 每秒 1 次 ioctl
- 证据:`tests/host/RT-144.json`,结论写入 M1.5 探针 Agent Note(核实开发备忘中的计数器频率、PHC 设备名与开销)
- 实现:`crates/hft-rt/tests/host/RT-144.sh`

### RT-145 升级主机上分配 1 GiB 与 2 MiB 大页区域

- 规则:[CPU、主机检查、内存](README.md#cpu主机检查内存)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,deploy 预留的大页已就绪
- 授权:无
- 步骤:
  1. `HugeRegion::anon(1 << 30, Huge1G, Some(0))` 与 `anon(2 << 20, Huge2M, Some(0))`,各 `prefault()`
- 通过标准:两者 `Ok`;`/proc/self/numa_maps` 显示页在 node 0;`free_hugepages` 相应减少
- 证据:`tests/host/RT-145.log`
- 实现:`crates/hft-rt/tests/host/RT-145.sh`

### RT-150 进程内环单跳延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_spsc_hop`
- 实现:`crates/hft-rt/benches/rt.rs::rt_150_spsc_hop`

### RT-151 try_push 与 try_pop 的摊还耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_spsc_push_pop`
- 实现:`crates/hft-rt/benches/rt.rs::rt_151_spsc_push_pop`

### RT-152 批量吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_spsc_throughput`
- 实现:`crates/hft-rt/benches/rt.rs::rt_152_spsc_throughput`

### RT-153 共享内存环单跳延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_shm_spsc_hop`
- 实现:`crates/hft-rt/benches/rt.rs::rt_153_shm_spsc_hop`

### RT-154 ByteSpsc 1 KiB 记录单跳延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_bytes_hop_1k`
- 实现:`crates/hft-rt/benches/rt.rs::rt_154_bytes_hop_1k`

### RT-155 tsc_now 与 tsc_ordered 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_tsc_now`
- 实现:`crates/hft-rt/benches/rt.rs::rt_155_tsc_now`

### RT-156 KillWord 读的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_killword_load`
- 实现:`crates/hft-rt/benches/rt.rs::rt_156_killword_load`

### RT-157 delta_ns 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_tsc_delta`
- 实现:`crates/hft-rt/benches/rt.rs::rt_157_tsc_delta`

### RT-158 Mailbox::try_take(空)的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_mailbox`
- 实现:`crates/hft-rt/benches/rt.rs::rt_158_mailbox`

### RT-159 SeqCell::load 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_seqcell`
- 实现:`crates/hft-rt/benches/rt.rs::rt_159_seqcell`

### RT-160 Heartbeat::beat 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_heartbeat`
- 实现:`crates/hft-rt/benches/rt.rs::rt_160_heartbeat`

### RT-161 cpu_relax 本身的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_pause_cost`
- 实现:`crates/hft-rt/benches/rt.rs::rt_161_pause_cost`

### RT-162 sample_pair 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_sample_pair`
- 实现:`crates/hft-rt/benches/rt.rs::rt_162_sample_pair`

### RT-163 TaskLoop 四任务全空闲一轮的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_taskloop_idle_round`
- 实现:`crates/hft-rt/benches/rt.rs::rt_163_taskloop_idle_round`

### RT-164 生产端唤醒检查(消费者未 park)的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_wake_check`
- 实现:`crates/hft-rt/benches/rt.rs::rt_164_wake_check`

### RT-165 park 后被生产者唤醒到开始下一轮的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[线程计划的执行:任务轮转与空闲策略](README.md#线程计划的执行任务轮转与空闲策略)
- 级别:bench
- 里程碑:M1
- 基准名:`rt_unpark_latency`
- 实现:`crates/hft-rt/benches/rt.rs::rt_165_unpark_latency`

### RT-166 热路径入口零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M1
- 前置:测试二进制安装计数分配器;环、ByteSpsc、Mailbox、SeqCell、KillWord、Heartbeat 已在计数开始前建好
- 步骤:
  1. 记录分配计数 c0
  2. 各执行 10^5 次:`try_push`/`try_pop`、`write_batch`+`commit`、`read_batch`+`release`、`reserve`+`commit`、`peek`+`advance`、`try_send`/`try_take`(值预先装箱、循环往返)、`SeqCell::load`、`KillWord` 的 `load`、`tsc_now`、`delta_ns`、`beat`、`idle_action`
  3. 记录分配计数 c1
- 期望:
  - `c1 − c0 == 0`
- 实现:`crates/hft-rt/tests/alloc.rs::rt_166_zero_alloc_hot_entries`

### RT-167 热路径入口零系统调用

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:proc
- 里程碑:M1
- 前置:Linux;同 RT-166 的对象;被测线程以 `strace -f -c` 计数(所有系统调用)
- 步骤:
  1. 被测线程执行 RT-166 步骤 2 的循环(不含 `try_send`/`try_take` 的唤醒挂钩:`set_wake(None)`)
- 期望:
  - 循环期间被测线程的系统调用总数为 0
- 实现:`crates/hft-rt/tests/proc_syscall.rs::rt_167_zero_syscalls_hot_entries`

## 验收

- M1 完成:里程碑为 M1 的用例全部有同编号实现且通过(`unit` 在 PR;标注 aarch64 的 unit 与 RT-057、RT-074、RT-077 在 CI 每夜 arm 专项;`proc` 在 PR 的 `ubuntu-latest`);`host` 用例按各自里程碑在所列机器上执行并归档证据。
- loom 用例(RT-008、RT-041、RT-042、RT-045、RT-116)每个 PR 运行且穷举完成(不设 `LOOM_MAX_PREEMPTIONS` 上限,单个模型 ≤ 10 min)。
- 压力用例(RT-009、RT-036、RT-046)在每夜 arm 专项以 10 倍规模运行零失败(弱内存序只在 aarch64 上真实出现)。
- 基准:RT-150–RT-165 在参考机 `bench-gate --reference` 下不超过 README [延迟与资源契约](README.md#延迟与资源契约) 与"线程计划的执行"节的预算,且不宽于 [benchmarks 单 crate 上限表](../../benchmarks/README.md#单-crate-基准上限表) 的环跳与 TSC 读取行;同条件下 `rt_spsc_hop` 与 `rtrb` 的对照结果随里程碑报告存档;从 M1.5 第一次参考机运行起判定,CI 只报告。
- 目标机器上 `check_host` 调优后无 hard 位:RT-141(dev2)、RT-142(升级主机)。
