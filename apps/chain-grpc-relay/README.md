---
description: "冷进程:把只提供 gRPC 的链上数据源(Helius LaserStream、仅 gRPC 的 BlockRazor 块流、直连 TPU 客户端)转成定长帧,经共享内存环或 UDP 喂给 chain-core;可选扩展,只在 chain-core 热化时需要"
kind: app
tier: cold
scope: optional
status: frozen
---

# chain-grpc-relay

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核:无(`architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2 中没有只涉及本组件的行)。

> **可选扩展:CEX-DEX 策略从旧框架迁移时启用。** 新框架首要服务 CEX-CEX 策略,链上策略继续在旧框架运行、迁移不急;本规格保留,启用前按届时的实际情况复核。下文中的"core""迁移范围内"指该迁移启用之后的范围;外部 venue 协议与链上事件编号段在架构上保留。

`chain-grpc-relay` 是 chain-core 与 gRPC 世界之间的协议转换器:它用 tokio + tonic 连接只提供 gRPC 的来源,把每条消息解成定长帧(大载荷分片),写进 [`hft-rt::ShmSpsc`](../../crates/hft-rt/README.md)(同机)或 UDP(跨机),由 chain-core 的 `chain-ingest`/`sol-ingest` 线程读取。它只在 chain-core 的依赖闭包必须去掉 tokio 时才有存在理由(Rust 的 gRPC/HTTP2/QUIC 客户端都依赖 tokio)。它**不拥有**任何业务状态、不做定价或决策、不写 journal(chain-core 读到的帧才是记录的来源);它**不在**任何 tick-to-trade 或 `chain_decide` 的关键路径上决定结果,只转运数据。**可选扩展,迁移范围内不需要**:chain-core 是 cold tier(允许 tokio 与 `tonic`),迁移范围内的 free profile 只用免费公共 RPC 的 HTTP 与 WS,没有 gRPC 来源。值得做的触发条件:启用了付费 gRPC 来源(BlockRazor 块流、Helius LaserStream、直连 TPU)**并且**决定把 chain-core 热化(依赖闭包去掉 tokio,例如 builder profile 需要微秒级投递)。此前保留本规格作为那种情形的设计。实例:`chain-grpc-relay@bsc`(仅当 BlockRazor 块流只有 gRPC)、`chain-grpc-relay@solana`(LaserStream;可加直连 TPU 出站通道)。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
chain-grpc-relay --config /etc/hft/chain-grpc-relay-bsc.toml [--check]      # hft-chain-grpc-relay@bsc.service
chain-grpc-relay --config /etc/hft/chain-grpc-relay-solana.toml [--check]   # hft-chain-grpc-relay@solana.service
```

退出码与 fail-loud 规则同 [apps/AGENTS.md](../AGENTS.md#配置与-fail-loud)(2 配置错误、3 主机前置条件,例如环头不匹配)。

### 来源与通道

| 实例 | 来源 | 方向 | 内容 | 范围 |
|---|---|---|---|---|
| `@bsc` | BlockRazor 块流(仅当它只有 gRPC) | 入 | 新块头 + 块内交易哈希 | 可选 |
| `@solana` | Helius LaserStream(Yellowstone gRPC) | 入 | 订阅账户的更新(池、tick/bin 数组、金库、配置)、slot 更新、本钱包签名的交易(`processed`) | 可选 |
| `@solana` | 直连 leader TPU(Yellowstone Jet,QUIC + SWQoS) | 出 | chain-core 交来的已签名交易 → 当前/下一 leader | 可选 |

每个来源一条入站环;另有一条 chain-core → relay 的控制环 `to_relay`(订阅变更、出站交易)。

### 与 chain-core 的接口

| 载体 | 同机 | 跨机(同 CPG,或可选的法兰克福部署) |
|---|---|---|
| 入站 | `/dev/hugepages2M/hft/rings/relay/<chain>/<source>.ring`(`ShmSpsc<RelaySlot>`,槽 1 KiB,容量 16,384;2 MiB 页 hugetlbfs 上的具名文件) | UDP,数据报 ≤ 1,400 B,帧分片;HMAC 截断 16 B |
| 控制 `to_relay` | `/dev/hugepages2M/hft/rings/relay/<chain>/to_relay.ring`(槽 1 KiB,容量 1,024) | 同左,带序号与确认(控制消息可靠) |

环文件由先启动的一方创建,另一方附着并校验环头;双方可独立重启(见实现说明 §4)。

### 帧格式

帧布局的唯一定义在本节,Rust 类型放在 [`hft-event`](../../crates/hft-event/README.md) 的 `relay` 模块(与 external-venue 协议的 `ext` 模块同一做法),chain-core 与本进程共用。

```rust
#[repr(C, align(8))]
pub struct RelayFrameHdr {        // 48 B
    pub magic: u16,               // 0xE7C2
    pub version: u8,              // 1
    pub kind: u8,                 // RelayKind
    pub source: u8,               // SourceId:1=BlockRazor 2=LaserStream 3=Tpu
    pub flags: u8,                // PART(非末片)| LAST | CONFLATED | REPLAY(来自断线后回补)
    pub chunk: u16,               // 分片序号(从 0)
    pub frame_len: u32,           // 整帧(未分片前)字节数,≤ 128 KiB
    pub chunk_len: u32,           // 本片载荷字节数
    pub frame_seq: u64,           // 每来源单调;缺口 = 丢帧
    pub src_ts_ns: i64,           // 来源给出的时间(块时间戳、Yellowstone created_at);无则 0
    pub rx_ns: i64,               // relay 收到该 gRPC 消息的 UTC(CLOCK_REALTIME)
    pub rx_tsc: u64,              // relay 收到时的 TSC(同机时 chain-core 可直接算 relay 一跳)
}

#[repr(u8)] pub enum RelayKind {
    SourceUp = 1,                 // {endpoint_id, resume_from: u64}
    SourceDown = 2,               // {reason: DownReason, last_seq_seen: u64}
    Heartbeat = 3,                // 每 100 ms,每条入站环一条
    Gap = 4,                      // {from_frame_seq, to_frame_seq}:relay 自己丢弃的区间
    BlockHead = 16,               // BSC:{number, hash[32], parent[32], ts_ms, tx_count}
    BlockTxHashes = 17,           // BSC:{number, hash[32], n, hashes[n][32]}(分片)
    AccountUpdate = 32,           // Solana:{pubkey[32], owner[32], slot, write_version, lamports, data_len, data[..]}(分片)
    SlotUpdate = 33,              // Solana:{slot, parent, status: Processed|Confirmed|Finalized|Dead}
    TxUpdate = 34,                // Solana:{signature[64], slot, err_code: u32 (0 = 成功), meta 摘要:本钱包 ATA 的前后 token 余额}
    TpuAck = 48,                  // 出站:{signature[64], leader, result: Sent|ConnErr|Timeout}(只是提交结果,不是链上证据)
}

#[repr(u8)] pub enum DownReason { StreamError = 1, IdleTimeout, AuthFailed, ServerClose, Backpressure, Shutdown }
```

`to_relay` 控制消息:`Subscribe{set_id, n, pubkeys[n]}`(整组替换 LaserStream 账户订阅)、`TpuSend{signature[64], last_valid_block_height, tx_len, tx[..]}`(可选)、`Ping`。

### 断流语义

- 任何来源断开、超时(`idle_timeout_ms` 内无消息)、鉴权失败或背压超限 → relay 在该来源的环上写 `SourceDown{reason}`,随后按退避重连(100 ms 起,×2,上限 5 s);连上后写 `SourceUp{resume_from}`。
- chain-core 收到 `SourceDown` 只把**该来源**标为不可用:BSC 的块流不可用只影响块首达统计与早证据,节点源(`newHeads`、logs、回执)不受影响;Solana 的 LaserStream 不可用时,`sol-ingest` 把受影响池子的报价置 `STALE`,在途签名的确认改由 RPC 的 `getSignatureStatuses` 继续([hft-chain-solana](../../crates/hft-chain-solana/README.md))。
- relay 进程整体消失:chain-core 在 500 ms 内没收到任何环的 `Heartbeat` → 视同该实例全部来源 `SourceDown(Shutdown)`。
- 传输层故障永远只影响"数据是否可用",不产生任何链上状态迁移(与 [defensive-patterns](../../docs/defensive-patterns.md) 一致);`TpuAck` 同理,只是提交结果。

### 配置(`/etc/hft/chain-grpc-relay-<chain>.toml`,`schema = 1`)

| 键 | 类型 | 含义 | 默认 / 来源 |
|---|---|---|---|
| `instance.chain` | `bsc`\|`solana` | 链 | 必填 |
| `sources` | [`{name, kind: blockrazor_grpc\|laserstream\|tpu, url, auth_secret, idle_timeout_ms}`] | 来源;`auth_secret` 是 Secrets Manager 引用 | 必填 |
| `out.transport` | `shm`\|`udp` | 载体 | 必填 |
| `out.dir` / `out.udp_addr` / `out.hmac_secret` | path / string / 引用 | 环目录或 UDP 目的地 | 按载体必填 |
| `out.ring_slots` / `out.slot_size` | u32 / u32 | 入站环容量与槽长 | 16,384 / 1,024 |
| `conflate.max_keys` | u32 | 账户更新合并表容量(背压时每 pubkey 只留最新) | 8,192 |
| `backpressure.down_after_ms` | u32 | 环持续满超过该时长 → `SourceDown(Backpressure)` | 1,000 |
| `replay.enabled` | bool | LaserStream 重连时从 `last_slot` 回补(服务端支持 24 h 回放,[research/03 §2.3](../../design/research/03-onchain-bsc-solana.md)) | true |
| `runtime.worker_threads` | u8 | tokio 运行时线程数 | 1(current_thread) |

## 实现说明

<details><summary>实现要点</summary>

### 1. 进程结构

- 一个进程、一个 tokio `current_thread` 运行时(可配到 2 个工作线程,但每条入站环的生产者只由一个任务持有,保证 SPSC 单写者)。
- 每个来源一个任务:连接 → 订阅 → 循环 `stream.message()` → 解 protobuf(`prost`)→ 编码帧 → 写环。另有一个任务读 `to_relay` 控制环(轮询间隔 1 ms)并执行订阅变更与 TPU 发送。
- 不做任何过滤以外的处理:BlockRazor 块内交易哈希全部转发(chain-core 自己按 `WatchSet` 过滤);LaserStream 账户数据原样转发字节,由 chain-core 按布局解码。

### 2. 编码与分片

1. 帧 = `RelayFrameHdr` + 载荷;载荷 > 槽长 − 48 时分片,每片带同一 `frame_seq`、递增 `chunk`,末片置 `LAST`。
2. chain-core 侧用预分配的重组缓冲(每来源 128 KiB);`frame_seq` 或 `chunk` 不连续 → 丢弃半帧并计为缺口。
3. UDP 载体与共享内存载体用同一帧格式;UDP 不重传(入站数据都是"最新值"或可由节点/RPC 补齐),缺口由 `frame_seq` 检出。

### 3. 背压

1. 写环前检查空槽;不足时:
   - `AccountUpdate`、`SlotUpdate`:放入合并表(按 pubkey / 只留最新 slot),置 `CONFLATED`,待有空间时写出;
   - `BlockHead`、`BlockTxHashes`、`TxUpdate`:丢弃并记录区间,空间恢复后先写 `Gap`。
2. 环持续满超过 `backpressure.down_after_ms` → 写 `SourceDown(Backpressure)`,断开该来源(避免 gRPC 服务端因我们消费慢而积压),空间恢复后重连。
3. relay 永不阻塞在环上,也不向 gRPC 流施加无界缓冲。

### 4. 重启与重连

- relay 重启:重新附着环,`frame_seq` 从新的随机起点开始并先写 `SourceUp`;chain-core 见到 `SourceUp` 即重置该来源的重组状态。
- chain-core 重启:消费者把读位置设为当前写位置,积压丢弃;relay 无感知。chain-core 重启后发一次 `Subscribe` 全量替换订阅,relay 重新订阅并由服务端推送账户的当前值(Yellowstone 订阅后首批为快照)。
- LaserStream 重连:若 `replay.enabled`,从断开前最后见到的 slot 请求回放,回放帧置 `REPLAY`。

### 5. 时间戳

- `rx_ns`:tonic 交付消息后立即读 `CLOCK_REALTIME`;`rx_tsc`:同一时刻的 TSC。冷进程允许读系统时钟。
- `src_ts_ns`:来源自带的时间(块时间戳、Yellowstone 的 `created_at`)。
- chain-core 读到帧时记自己的 `rx_tsc`;`chain-ingest` 把 `rx_ns`、`src_ts_ns` 与自己的 `rx_tsc` 一并写入 raw journal,块首达统计(`HeadSeen`)以 relay 的 `rx_ns` 计,relay 这一跳的延迟另行统计。

### 6. 与 journal 的关系

relay 不链接 `hft-journal` 的写者、不写任何 journal。chain-core 的 `chain-ingest`/`sol-ingest` 把从 relay 读到的帧写进 raw journal(`raw-chain-bsc-blockrazor`、`raw-chain-solana-laserstream`,[journal-format](../../docs/subsystems/journal-format.md#三类-journal)),`SourceUp`/`SourceDown`/`Gap` 由 `chain-seq` 作为连接状态记录写进 chain journal——"决策可见 ⟺ 已记录"的边界在 chain-core,不在 relay。relay 自己的诊断只写 journald(`tracing`)。

### 7. systemd 与隔离

- 单元 `hft-chain-grpc-relay@<chain>.service`(在 [deploy/systemd/](../../deploy/README.md)):`User=hft-relay`、`Restart=always`、`RestartSec=1`、`RestartPreventExitStatus=2 3`、`MemoryMax=2G`、`CPUQuota=100%`。
- **不在隔离核**:`AllowedCPUs` 限定在所在主机的冷核集合(tokyo-cex 上为 38–43,见 [hft-core 线程计划](../hft-core/README.md#线程计划);tokyo-chain 上为节点与 chain-core 以外的核),与 chain-core 的热核不重叠。
- 与 chain-core 没有 systemd 依赖:任一方可单独启动、重启;chain-core 在 relay 缺席时照常运行(相应来源为 `SourceDown`)。
- 秘密(Helius key、BlockRazor 凭据、HMAC 密钥)启动时从 Secrets Manager 取回,不写配置与日志。

### 8. 源文件

| 文件 | 职责 |
|---|---|
| `src/main.rs` | 参数、退出码、运行时 |
| `src/config.rs` | TOML 类型与校验 |
| `src/out.rs` | 环/UDP 生产者、分片、合并表、背压 |
| `src/sources/blockrazor.rs` | BlockRazor gRPC 块流 |
| `src/sources/laserstream.rs` | Yellowstone 订阅、回放、订阅替换 |
| `src/sources/tpu.rs` | 可选:Yellowstone Jet 出站 |
| `src/control.rs` | `to_relay` 控制环 |

</details>

## 延迟与资源契约

冷进程,不在隔离核,但它位于"来源 → chain-core"的数据路径上,因此给出 relay 这一跳的预算(同机,`chain-core 读到的 rx_tsc − relay rx_tsc`):

| 指标 | 目标 | 测量 |
|---|---|---|
| relay 一跳(解码 + 写环 + chain-core 读到) | p50 ≤ 30 µs,p99 ≤ 300 µs,p99.9 ≤ 3 ms | chain-core 侧直方图 `relay_hop`(按来源) |
| 相对直连的首达损失 | 块首达中位数劣化 ≤ 1 ms | 启用后首周与 bloXroute WS 直连对比 |
| CPU | ≤ 1 核(LaserStream 峰值) | cgroup 统计 |
| 内存 | ≤ 2 GiB(合并表 + 重组缓冲 + tonic 缓冲) | `MemoryMax` |

链上机会以块/slot 计(0.45 s / 0.4 s),p99.9 的毫秒级抖动可以接受;若 `relay_hop` p99 超过 1 ms 持续 3 个窗口,watchtower 告警。

## 测试与验收

| 测试 | 内容 |
|---|---|
| 解码金样 | 录制的 BlockRazor 与 LaserStream gRPC 消息 → 期望帧(字节级) |
| 分片属性 | 任意长度载荷 × 任意槽长:分片后重组逐字节相等;丢片 → 半帧丢弃并计缺口 |
| 背压 | 慢消费者:账户更新被合并且最终一致(每 pubkey 最后值);块与交易帧产生 `Gap`;持续满 → `SourceDown(Backpressure)` 并在恢复后重连 |
| 断流 | 服务端断开、空闲超时、鉴权失败 → `SourceDown` 原因正确;重连后 `SourceUp`;LaserStream 回放帧带 `REPLAY` 且 slot 连续 |
| 重启 | relay 重启与 chain-core 重启各 100 次:无半帧被当作完整帧、订阅在 chain-core 重发 `Subscribe` 后恢复 |
| 隔离 | 进程只在冷核上运行(`/proc/<pid>/status` 的 `Cpus_allowed_list`);依赖闭包检查:chain-core 不依赖本进程的任何 crate |
| fuzz | `fuzz/relay_frame`(chain-core 侧帧解码与重组) |
| 验收(启用时) | `relay_hop` 满足上表;连续 7 天无未检出的缺口(`frame_seq` 连续性由 chain-core 统计) |

## 依赖

| 依赖 | 用途 | 约束 |
|---|---|---|
| `tokio`(current_thread)、`tonic`、`prost`、`yellowstone-grpc-proto` | gRPC 客户端与 protobuf | 只允许在本进程;版本锁定 |
| BlockRazor 的 proto 定义 | 块流 | vendored,锁版本 |
| `yellowstone-jet`(可选) | 直连 TPU | 待核实其 API 与依赖 |
| [`hft-rt`](../../crates/hft-rt/README.md) | `ShmSpsc` 生产者 | — |
| [`hft-event`](../../crates/hft-event/README.md)(`relay` 模块) | 帧类型 | — |
| `tracing`、`serde`/`toml` | 日志与配置 | 冷进程允许 |

禁止:`hft-journal` 写者、`hft-chain-evm`、`hft-chain-solana`、`hft-sequencer`(relay 不含业务逻辑);任何 hot crate 反向依赖本进程。

## 延伸阅读

- [architecture.md §2](../../docs/architecture.md)(部署单元)、[apps/AGENTS.md](../AGENTS.md)(tokio 边界)、[apps/chain-core](../chain-core/README.md)
- [hft-chain-evm](../../crates/hft-chain-evm/README.md)(块流在 `chain-ingest` 中的用法)、[hft-chain-solana](../../crates/hft-chain-solana/README.md)(LaserStream 与 TPU)
- [research/03 §1.4、§2.3、§2.4](../../design/research/03-onchain-bsc-solana.md)、[06 §2.2、§5](../../design/06-链上执行层.md)

## 已知限制与后续工作

- relay 一跳在冷核上有毫秒级尾部;若将来某个 gRPC 来源的首达优势以 µs 计(目前没有这种来源),需要改为 warm 专属核或寻找非 tokio 客户端。
- 只覆盖入站流与可选的 TPU 出站;若某家 builder 的投递接口只接受 gRPC/HTTP2,需要在 `to_relay` 增加 `BuilderSubmit` 通道(目前五家均按 HTTP/1.1 JSON-RPC 规划,见 hft-chain-evm 开发备忘)。
- 法兰克福(可选)部署时,本进程随 `chain-core@solana` 迁到 fra 主机,载体仍为同机环。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:BlockRazor 块流是否有 WS/HTTP 版本(有则 `@bsc` 实例不需要);LaserStream 与标准 Yellowstone proto 的兼容性(research/03 §2.3 有一份评测称需 Helius SDK);Yellowstone Jet 是否可作为库嵌入。核实方法:各自官方文档 + 在测试环境订阅 10 分钟比对消息。
- `hft-rt::ShmSpsc` 的槽大小是元素类型的大小,允许 8 B–4 KiB(见 [hft-rt](../../crates/hft-rt/README.md#spsc-环进程内与共享内存同一实现)),1 KiB 的 `RelaySlot` 在范围内。
- 实现顺序:`out.rs`(环、分片、背压,先用假来源测)→ BlockRazor(若它只有 gRPC)或 LaserStream → 控制环与订阅替换 → 回放 → TPU(可选)。

</details>
