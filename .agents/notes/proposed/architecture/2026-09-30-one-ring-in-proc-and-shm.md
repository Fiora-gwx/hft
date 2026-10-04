# Agent Note: 一套 SPSC 环实现同时承载进程内与共享内存

Status: proposed

## Problem

系统里有三类点对点通道:热进程内的线程间(feed/gw ↔ sequencer)、同机进程间(hft-core ↔ chain-core、冷进程 → ctl-gateway)、跨机(hft-core ↔ 另一台上的 chain-core)。早期设计为进程内用自写/`rtrb`,为进程间引入 iceoryx2([research/04 §8](../../../../design/research/04-oss-frameworks-and-patterns.md) 的决策表)。两套实现意味着两套语义、两套基准、一个大依赖,而我们的进程间通道都是单生产者单消费者。

## Proposal

[13 O1](../../../../design/13-设计评审与优化.md)、[10 §3](../../../../design/10-语言分工与代码组织.md):

1. [`hft-rt`](../../../../crates/hft-rt/README.md) 提供一个定长槽 SPSC 环(约 200 行,借 `rtrb`/Disruptor 结构):缓存行填充的头尾游标、批量发布、忙等消费。
2. 同一实现的存储可以是堆内(进程内,`Spsc<T: Copy>`)或 hugetlbfs/tmpfs 上的具名文件(跨进程,`ShmSpsc`);头部带魔数、版本、槽大小、生产者/消费者进程标识,附着时校验。
   - **元素约束不用自定义 trait**(早期草案的 `RingElem` 已删除):`ShmSpsc` 的元素直接以 `zerocopy::{FromBytes, IntoBytes, KnownLayout, Immutable} + Copy` 约束——对端是另一个进程,任何字节都必须是合法槽值,结构校验在上层解码;`hft-event` 的类型只需 derive zerocopy,不必为 `hft-rt` 的 trait 写 impl,也就不碰孤儿规则。进程内 `Spsc<T: Copy>` 不加 zerocopy 约束(`FeedMsg`/`RepMsg` 是带数据的枚举,无法 `FromBytes`,也不跨进程)。
   - 变长的控制记录走同一文件里的 `ByteSpsc`(按字节计容量,≥ 256 KiB,单条 ≤ capacity/4);warm → sequencer 的大对象(解析好的参数)走单槽所有权交接 `Mailbox<T: Send>`,`unsafe` 封在 `hft-rt` 内。
3. 同机进程间(hft-core ↔ chain-core)用 `ShmSpsc`;跨机用 UDP(带序号与确认),在外部 venue 协议层实现([external-venue-protocol](../../../../docs/subsystems/external-venue-protocol.md))。
4. 冷进程读数据不走环,尾随 journal;冷进程发命令经 ctl-gateway 的 Unix socket。
5. iceoryx2 推迟到出现真实的多订阅者需求时再评估。

## Alternatives considered

**`disruptor`/`rtrb` crate 做进程内 + iceoryx2 做进程间。** 两套语义与基准;iceoryx2 是大依赖(守护进程/服务发现概念),我们用不到它的多订阅者与 WaitSet;同机跨进程最好情况 ~240 ns,与自写共享内存环同量级([research/06 §0](../../../../design/research/06-data-plane-replay-observability.md))。

**Unix socket / 管道做进程间。** 每条消息一次系统调用与调度唤醒,尾部进入 ms 级;不适合每秒数千条的 `BboSnapshot`。

**Aeron(`rusteron`)。** 适合多机集群;单机部署是过度设计,可选扩展:多 AZ/多机时([12 §5](../../../../design/12-风险与未决问题.md))。

## Acceptance criteria

- 环跳(push → pop,跨核)p50 < 100 ns([11 §2](../../../../design/11-实施路线图.md)),进程内与共享内存两种承载各有基准,并与 `rtrb` 对照。
- 附着校验:版本或槽大小不符的一方拒绝附着并返回错误(测试)。
- 生产者崩溃后消费者能检测(心跳/代号)而不是永远自旋等待(测试)。

## Risks

- 跨进程环的生命周期与崩溃恢复比进程内复杂:文件命名、清理、重附着规则写在 `hft-rt` README。
- 若将来需要一对多广播(多个冷读者实时订阅行情),本设计不覆盖——届时写新 Note 评估 iceoryx2。
