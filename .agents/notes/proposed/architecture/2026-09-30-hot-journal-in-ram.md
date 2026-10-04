# Agent Note: 热 journal 只写 RAM 段,持久化在旁路

Status: proposed

## Problem

早期设计允许热 journal 放在"hugetlbfs 或普通 mmap"上([13 R2](../../../../design/13-设计评审与优化.md))。磁盘文件系统上的 `MAP_SHARED` 页被内核回写后会重新写保护,下一次写触发 `page_mkwrite` 缺页;遇到 stable pages 还要**等 I/O 完成**(ms 级)。这与是否 fsync 无关——不 fsync 也会发生——正是 sequencer p99.9 的杀手。原型把 `sync_data()` 放在写线程里每秒一次,同类问题([research/06 §2.6](../../../../design/research/06-data-plane-replay-observability.md))。

## Proposal

[08 §1–§2](../../../../design/08-数据面与回放.md):

1. 写者只写 RAM 段:hugetlbfs(1 GiB 页)或 tmpfs(`/dev/shm`)上的**具名文件,不用匿名 memfd**,段文件由冷线程预创建并预触摸,切段只是换指针;运行期零缺页。
2. housekeeping 核上的 persist 线程从 RAM 段顺序 `write` 到 NVMe/EBS 冷副本并周期 `fdatasync`,落后几毫秒;c7i.metal 无实例存储,冷副本放 io2 Block Express(fdatasync 均值 < 500 µs)或 gp3。
3. RAM 保留最近 2–4 个段(hugepages 预算 8–16 GiB);更早的只留冷副本。
4. 崩溃模型:进程崩溃 → RAM 段文件仍在,重启先补完复制再恢复;整机掉电 → 丢复制滞后部分,交易所侧真值 + 对账收敛。

实现归 [`hft-journal`](../../../../crates/hft-journal/README.md);规则见 [hot-path-rules M3](../../../../docs/hot-path-rules.md#内存m)。

5. 旧段回收:写者滚动后经 SPSC 把旧段句柄交还 housekeeping,宽限期后 `munmap` + `unlink`;启动自检把"已 unlink 仍映射"的段计入大页短缺;同进程尾随读者预映射固定段槽位,不反复 `munmap`。段大小、RAM 保留份数与冷盘容量**待实测**(用 S3 现有数据按目标订阅实算事件率)。

## Alternatives considered

**磁盘文件 mmap、热线程不 fsync。** 仍有 `page_mkwrite` 与 stable pages 等待,见 Problem。

**O_DIRECT。** 要求对齐缓冲,绕过 page cache 使尾随读者失去零拷贝;只在 GB/s 级写入时有意义,我们是 0.1–1 MB/s 级([research/06 §2.6](../../../../design/research/06-data-plane-replay-observability.md))。

**热线程 `write()` 系统调用。** 违反热路径"除 send/recv 外无系统调用"([hot-path-rules T7](../../../../docs/hot-path-rules.md#线程t))。

**匿名 memfd。** 13 R2 的原文提到 memfd;但匿名 `memfd_create` 的内存随进程退出释放,"进程崩溃不丢"不成立,除非另一个进程持有 fd。具名文件更简单,崩溃后重启的进程与冷读者都能按路径直接打开。

## Acceptance criteria

- journal 追加 p50 < 200 ns;persist 滞后 p99 < 50 ms([11 §2](../../../../design/11-实施路线图.md))。
- 稳态下写者线程 `page-faults` 计数为 0(`perf stat`)。
- `kill -9` 热进程后重启:冷副本与崩溃前 RAM 段逐字节一致,无记录丢失。

## Risks

- 整机掉电丢复制滞后部分(ms 到数百 ms):对订单与成交,交易所是权威,由 reconciler 收敛;persist 滞后纳入 `watchtower` 监控。
- hugepages 预算挤占簿与环:段大小与保留数是配置项,启动期校验预算。
- tmpfs 页在内存压力下可能被换出:生产 `swapoff`,由 [hostprobe](../../../../tools/README.md#hostprobe) 校验。
