# Agent Note: 线程计划数据化与主机类(dev2 / c4 / c8 / metal)

Status: proposed

> **第八轮(2026-10-02)**:dev2 的内存上限、journal 段大小与 dev2 护栏(`hft.slice`、cdex 健康守护)的唯一归属改为 [hft-core](../../../../apps/hft-core/README.md),本 Note 中的"600 MB"以它为准;dev2 与 cdex 正式账户共机,只做功能与正确性;延迟验收只在 M7 统一升级后;见 [ccex-first-scope](2026-10-02-ccex-first-scope.md)。

> **第八轮补充(2026-10-03)**:补充——dev2 护栏只按资源信号(`MemAvailable`、PSI)停 hft,cdex 单元重启或状态文件过期只告警;`c8` 上 `persist` 不与 `advisor` 同核;统一升级后的 `sim` 实例只用冷核;数值归 hft-core;见 [sim-only-instance](2026-10-03-sim-only-instance.md)。

> **第八轮第三次修订(2026-10-03)**:补充——统一升级主机同时运行 prod 与 sim 两个实例,`c8` 很可能不够,档位 M7 前按实测选定(候选 `c16`,16 vCPU / 32 GB 级别);每个下单 venue 的 gw 独占线程;deploy 的隔离核列表由 hft-core 线程计划生成;dev2 的 `hft.slice` 拆为 `hft-core.slice`、`hft-sim.slice` 与 `hftbatch.slice`;见 [sim-colocated-with-prod](2026-10-03-sim-colocated-with-prod.md)。

> **第八轮第四次修订(2026-10-03)**:补充——主机类追加 `c32`(`c8g.8xlarge`,prod + sim 同机的默认候选),`c16` 为下限候选,`c8` 只承载 prod;IRQ 独占一核,persist 与段预创建离开核 0,feed 拆为 `feed-bbo`/`feed-md`;见 [host-class-c32](2026-10-03-host-class-c32.md)、[rx-irq-driven](2026-10-03-rx-irq-driven.md)、[feed-split-and-snap-fetch](2026-10-03-feed-split-and-snap-fetch.md)。

## Problem

既有规格把线程布局写成"每个热角色一个 OS 线程、绑一个隔离核、忙等"([dedicated-gateway-threads](2026-09-30-dedicated-gateway-threads.md)),核分配表按 c7i.metal-24xl 与 c8g.2xlarge 写死。用户决定(第四轮裁决 R4-2):新框架必须先在**现有 2 核服务器**(c7g.large,2 vCPU,3.8 GB,与 cdex 同机)上运行给老板看效果,再按效果升级到 4–8 核。2 核上无法给 sequencer、两所 feed/gw 各一个忙等核;若热线程仍忙等,会抢走 cdex 的 CPU。升级时也不应改代码或重新构建。

## Proposal

1. **线程计划由配置驱动**:`[[threads]] = {name, core = <u16|none>, idle, tasks}`;task ∈ `sequencer`、`feed:<venue>`、`gw:<venue>`、`ext:<chain>`、`housekeeping`、`persist`、`ctl-gateway`、`conn-builder`、`advisor`。同一线程按固定顺序轮流调用各任务的 `poll_once()`/`step()`,每任务每轮有处理上限;环与事件语义不变,sequencer 的输入顺序已记录在 journal,确定性不受影响。归属:[hft-core 线程计划](../../../../apps/hft-core/README.md#线程计划)。
2. **任务契约**:`poll_once`/`step` 非阻塞、不自旋等待(环满时本轮停读,数据留在 TLS/内核缓冲)、空闲开销小,返回"本轮是否做了事"([hft-venue](../../../../crates/hft-venue/README.md)、[hft-sequencer](../../../../crates/hft-sequencer/README.md))。
3. **空闲策略**:hft-rt 提供 `IdlePolicy{Spin | SpinThenPark{spin_us, park_us} | Park{park_us}}`、`TaskLoop` 与 `Parker/Unparker`(futex;含 socket 任务的线程用 epoll + eventfd)。唤醒协议是 Dekker 式:消费者写 PARKING → 复查输入 → PARKED;生产者 commit → `SeqCst` 栅栏 → 读状态,只在 PARKED 时发系统调用唤醒。`Spin` 消费者的生产端不挂唤醒,热路径零额外指令([hft-rt](../../../../crates/hft-rt/README.md#线程计划的执行任务轮转与空闲策略))。
4. **主机类** `host.class`(与实际 CPU/内存不符拒绝启动):
   - `dev2`:线程 A = 两所 feed+gw,线程 B = sequencer,线程 C(不绑核、低优先级)= 全部 warm 任务;全部 `SpinThenPark`;不隔离核、无大页;journal 热段放 tmpfs 小段(4 KiB 页,预触摸 + mlock);订阅几十个合约;常驻 ≤ 600 MB;**无延迟验收**;watchtower `HostPressure` 盯 swap/PSI;部署侧只做可回滚的 NIC 合并与 IRQ 建议。
   - `c4`:每所 feed+gw 合并、热线程 `Spin`;影子可用,无延迟验收。
   - `c8`:一任务一线程一隔离核;**延迟验收在这一档**。
   - `metal`:可选扩展(LeadLag 实测证明 A 类需要更低延迟时)。
5. 升级 = 换主机类配置 + 部署对应 GRUB/cpuset,同一 ARM64 二进制。

## Alternatives considered

**保持"一角色一核"、一开始就上 c8。** 老板要先在现有服务器上看效果;且这会把"能否在小机器上跑"推迟到无法回退的时点。

**在 2 核上仍全部忙等、只降低线程数。** 两个忙等线程会占满两个 vCPU,cdex 与系统服务被饿死;实测的 2 核唤醒抖动(02 章)本来就是 Python/collector 抢核所致。

**每个主机类一套编译期布局(feature 开关)。** 违背"升级不重新构建";也让每个布局的代码路径各自分叉、测试矩阵翻倍。

**用 tokio 之类的执行器调度任务。** 违背热路径无异步运行时的规则([rust-and-python-only](2026-09-28-rust-and-python-only.md));我们需要的只是固定顺序轮转 + 空闲退让,自写 < 500 行。

## Acceptance criteria

- 同一份二进制只换 `host.class` 与 `[[threads]]`,在 `dev2`、`c4`、`c8` 上都能启动并通过启动闸门;`host.class` 与实际 CPU 不符 → 退出 3。
- `dev2`:hft-core 空载 CPU < 5%、常驻 ≤ 600 MB;`HostPressure` 7 天无 P1;cdex 的信号→下单延迟分布与 hft-core 未运行时相比不劣化。
- 唤醒协议的 `loom` 穷举无丢失唤醒;`Spin` 消费者的生产端无唤醒检查。
- 同一 journal 在 `dev2` 与 `c8` 上 `replay-verify` 结果相同(线程合并不影响确定性)。
- `c8` 上满足 [latency-budget](../../../../docs/latency-budget.md#7-主机类与验收口径) 的延迟验收。

## Risks

- `dev2` 上 park/唤醒带来 10–100 µs 量级的额外延迟与抖动;接受,因为 dev2 无延迟验收。若老板以 dev2 的延迟判断效果,需在报表中显式标注主机类。
- 合并线程里一个任务的长操作(如 feed 解析 1000 档快照 40–80 µs)会推迟同线程的 gw 任务;`c4`/`dev2` 的下单延迟因此受行情突发影响(13 R3)。
- 生产端唤醒检查的 `SeqCst` 栅栏在 `SpinThenPark` 消费者的环上每次 commit 增加 ≈ 20–40 ns;只影响 dev2/warm 消费者。
- 2 核上 tmpfs 热段 + mlock 占用 ≈ 512 MiB,与 cdex 争内存;由 `HostPressure` 与常驻预算兜底。
