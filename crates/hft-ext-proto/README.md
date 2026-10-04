---
description: "hft-core 与 chain-core 之间的外部 venue 协议帧(ext)、chain journal 专用记录(0x0310–0x033F)与可选的 chain-grpc-relay 帧(relay)的 Rust 定义;chain-core free profile 与 hft-core 都依赖它"
kind: crate
tier: pure
scope: optional
status: frozen
---

# hft-ext-proto

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核:无(`architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2 中没有只涉及本组件的行)。

> **可选扩展:CEX-DEX 策略从旧框架迁移时启用。** 新框架首要服务 CEX-CEX 策略,链上策略继续在旧框架运行、迁移不急;本规格保留,启用前按届时的实际情况复核。下文中的"core""迁移范围内"指该迁移启用之后的范围;外部 venue 协议与链上事件编号段在架构上保留。

`hft-ext-proto` 拥有链上执行的线格式:外部 venue 协议的帧与协议自有消息(`ext` 模块,hft-core ↔ chain-core,core)、只出现在 `chain-<chain>` journal 的记录体(kind `0x0310–0x033F`,core)、chain-grpc-relay 与 chain-core 之间的帧(`relay` 模块,可选:随 chain-grpc-relay 一起)。本 crate 随 chain-core 一起是 optional;不启用链上时 hft-core 不链接它。它与 [hft-event](../hft-event/README.md) 分开,只为让不涉及链上的组件不携带这些类型。字段**语义**归属 [external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)、[chain-grpc-relay](../../apps/chain-grpc-relay/README.md#帧格式) 与 [hft-chain-evm](../hft-chain-evm/README.md);本 crate 只给布局与编解码。seq journal 里的链上事件(`ChainHead`、`PoolQuote`、`SwapDecision`、`TxLifecycle`,0x0301–0x0304)仍在 hft-event。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 模块

**`ext` 模块**:external-venue 协议的帧头与协议自有消息(`BboSnapshot`、`SwapIntentMsg`、`IntentQuery`、`IntentAck`、`IntentState`、`Hello`、`Ack` 等)的**字段与语义归属** [external-venue-protocol](../../docs/subsystems/external-venue-protocol.md),Rust 类型实现在本 crate 的 `ext` 模块(`src/ext.rs`),`schema_hash` 也在这里计算;本 README 不复述其布局。`PoolQuote`、`ChainHead`、`TxLifecycle` 在协议与 seq journal 中逐字节相同。共享内存载体的槽类型 `ext::ExtSlot`(256 B,`#[repr(C, align(8))] [u8; 256]` 包装,派生 zerocopy `FromBytes/IntoBytes/KnownLayout/Immutable`,满足 [hft-rt `ShmSpsc`](../hft-rt/README.md#spsc-环进程内与共享内存同一实现) 的元素约束)与其布局身份常量 `ext::EXT_SLOT_LAYOUT_ID`(由 `schema_hash` 派生)也在这里。

**`relay` 模块**(可选,feature `relay`;触发条件:部署 chain-grpc-relay):[chain-grpc-relay](../../apps/chain-grpc-relay/README.md#帧格式) 与 chain-core 之间的帧(`RelayFrameHdr` 48 B + `RelayKind` 载荷,大载荷分片)的**字段与语义归属该 app README 的"帧格式"一节**,Rust 类型实现在本 crate 的 `relay` 模块(`src/relay.rs`),本 README 不复述其布局。共享内存环的槽类型 `relay::RelaySlot`(1 KiB,同样的 zerocopy 包装)与 `relay::RELAY_SLOT_LAYOUT_ID` 也在这里。relay 帧不进 seq journal;chain-core 把读到的帧写进自己的 raw journal。

### 编号与注册

本 crate 提供与 hft-event 同形的 `KINDS_EXT: &[KindInfo]`(`journals = CHAIN`),只含 `0x0310–0x033F`;hft-event 的注册表把该段标为"归 hft-ext-proto"。`cargo xtask gen-schema` 合并两张表,编号重叠或越段即失败。记录体同样遵守 hft-event 的 `Body` 约束(`#[repr(C)]`、显式 `_pad`、`size_of`/`offset_of` 编译期断言、尾部追加字段不得是 0 非法的枚举)。

### chain journal 记录(0x0310–0x033F)

只出现在 `chain-<chain>` journal(`KindInfo.journals = CHAIN`),由 chain-core 的 `chain-seq` 写。free profile 写 `IntentRx`、`IntentVerdict`、`SimResult`(`eth_call` 结果)、`Evidence`、`NonceEvent`、`WalletCalib`、`HeadSeen`(各 RPC 来源的首达)、`KillMap`;`PoolLog`/`PoolDrift`/`PoolRebuilt`(L0 池镜像)、`BundleBuilt`/`BuilderSubmit`/`BidParams`/`BuilderStat`(builder profile)属于可选扩展,编号与布局现在就固定。内容语义与填写时机见 [hft-chain-evm](../hft-chain-evm/README.md) 的 chain journal 一节。U256 数量以小端肢 `[u64; 4]` 保存原值(链上精确值,回放不需要节点);`pool_ix` 是 chain-core 路由表(`routes.toml` → `ConfigBlob`)中的池子下标。

```rust
pub struct HeadSeen      { pub chain: VenueId, pub source: u8, pub first: u8, pub _pad: u8, pub lag_ns: u32 /* 相对首达 */,
                           pub number: u64, pub rx_tsc: Tsc, pub seen_utc_ns: i64, pub hash: [u8; 32] }                       // 0x0310,64 B
pub struct PoolLog       { pub pool_ix: u32, pub what: u8 /* Swap=1 Mint=2 Burn=3 Flash=4 Sync=5 */, pub removed: u8, pub _pad: u16,
                           pub block: u64, pub log_index: u32, pub tx_index: u32,
                           pub amount0: [u64; 4], pub amount1: [u64; 4] /* 有符号补码 */, pub sqrt_price_x96: [u64; 4],
                           pub liquidity: [u64; 2], pub tick: i32, pub _pad2: u32 }                                            // 0x0311,144 B
pub struct PoolDrift     { pub pool_ix: u32, pub field: u8 /* Reserve0=1 Reserve1=2 SqrtPrice=3 Liquidity=4 */, pub _pad: [u8; 3],
                           pub block: u64, pub mirror: [u64; 4], pub actual: [u64; 4] }                                          // 0x0312,80 B
pub struct PoolRebuilt   { pub pool_ix: u32, pub ticks_loaded: u32, pub block: u64, pub elapsed_ns: u64, pub _rsv: u64 }       // 0x0313,32 B
pub struct IntentRx      { pub rx_tsc: Tsc, pub session: u64, pub msg: ext::SwapIntentMsg }                                     // 0x0314,16 B + 消息
pub struct IntentVerdict { pub intent_id: u64, pub verdict: u8 /* Accepted=1 Rejected=2 Duplicate=3 */, pub _pad: u8,
                           pub reason: u16, pub _pad2: u32, pub decide_tsc: Tsc, pub chain_seq: u64 }                           // 0x0315,32 B
pub struct SimResult     { pub intent_id: u64, pub block: u64, pub amount_in: Qty, pub amount_out: Qty, pub gas_used: u64,
                           pub elapsed_ns: u64, pub route: u32, pub cache_misses: u32,
                           pub status: u8 /* Ok=1 Revert=2 Error=3 */, pub level: u8, pub _pad: [u8; 6] }                     // 0x0316,64 B
pub struct BundleBuilt   { pub intent_id: u64, pub variant: u8, pub n_variants: u8, pub wallet: u8, pub _pad: u8, pub _pad2: u32,
                           pub nonce: u64, pub gas_limit: u64, pub gas_price_wei: u64, pub bribe: Qty /* BNB */,
                           pub max_block: u64, pub tx_hash: [u8; 32] }                                                          // 0x0317,88 B
pub struct BuilderSubmit { pub intent_id: u64, pub builder: u8, pub variant: u8, pub http_status: u16,
                           pub result: u8 /* Accepted=1 Rejected=2 Timeout=3 TransportErr=4 */, pub _pad: [u8; 3],
                           pub write_tsc: Tsc, pub resp_tsc: Tsc, pub bundle_hash16: [u8; 16] }                                 // 0x0318,48 B
pub struct Evidence      { pub intent_id: u64, pub source: u8, pub what: u8 /* Receipt=1 BlockTx=2 BundleStatus=3 BuilderResp=4 */,
                           pub status: u8 /* Ok=1 Reverted=2 NotFound=3 */, pub eflags: u8 /* LOG_OK=1 BAL_OK=2 */, pub _pad: u32,
                           pub block: u64, pub rx_tsc: Tsc, pub utc_ns: i64, pub tx_hash: [u8; 32], pub block_hash: [u8; 32] } // 0x0319,104 B
pub struct NonceEvent    { pub wallet: u8, pub what: u8 /* Grant=1 Consumed=2 Dropped=3 Tainted=4 */, pub _pad: [u8; 6],
                           pub nonce: u64, pub intent_id: u64, pub block: u64 }                                                 // 0x031A,32 B
pub struct WalletCalib   { pub wallet: u8, pub _pad: [u8; 7], pub chain_nonce: u64, pub local_next: u64,
                           pub native_balance: Qty, pub block: u64, pub utc_ns: i64 }                                          // 0x031B,48 B
pub struct BidParams     { pub builder: u8 /* 0 = 全部 */, pub _pad: [u8; 7], pub ratio: Rate, pub min_bribe: Qty, pub max_bribe: Qty,
                           pub version: u64, pub utc_ns: i64 }                                                                  // 0x031C,48 B
pub struct BuilderStat   { pub builder: u8, pub _pad: [u8; 7], pub hour_utc_ns: i64, pub submitted: u32, pub received: u32,
                           pub included: u32, pub first_seen: u32, pub self_frontrun: u32, pub other_frontrun: u32,
                           pub bribe_total: Qty, pub _rsv: [u64; 4] }                                                          // 0x031D,80 B
pub struct KillMap       { pub rx_tsc: Tsc, pub core_seq: u64 /* hft-core 心跳携带的 seq journal 位置 */,
                           pub n: u8, pub _pad: [u8; 7], pub bits: [KillBits; 32] /* 下标 = AccountId;每账户 GLOBAL/VENUE(=链)/HARD 位 */ } // 0x031F,56 B,Input
// KillMap:hft-core → chain-core 心跳里的每账户 KillBits 位图(帧字段同为 n + bits[32],在 ext 模块);chain-core 收到与上一份不同的位图时写这条 Input,
//          链上意图的 kill 判定只看它与意图的 sflags.RISK_REDUCING
// 0x031E 退役(原临时接口 BridgeCmd;临时接口已删除,编号不复用)
```

签名后的原始交易字节不入任何 journal(只记哈希与未签名字段)。

## 实现说明

<details><summary>实现要点</summary>

| 文件 | 职责 |
|---|---|
| `src/ext.rs` | 外部 venue 协议帧头、协议消息、`ExtSlot`、`EXT_SLOT_LAYOUT_ID`、`schema_hash` |
| `src/relay.rs` | relay 帧、`RelaySlot`、`RELAY_SLOT_LAYOUT_ID`(feature `relay`,可选) |
| `src/chain.rs` | chain journal 记录体(`event_body!` 宏复用 hft-event 导出的宏) |
| `src/kinds.rs` | `KINDS_EXT` |

`schema_hash` = 对 `ext` 全部布局(名字、偏移、类型)按固定顺序做 SHA-256 的前 16 字节,写进两端二进制与 `Hello`。

</details>

## 延迟与资源契约

编解码同 hft-event:定长体 encode ≤ 10 ns、decode + view ≤ 5 ns(基准 `extp_encode`、`extp_decode`);零分配、零系统调用。与其他 core crate 一样进 CI 门槛(`relay` feature 单独一个 job)。

## 测试与验收

- 每个体的大小与偏移断言;往返属性测试;`KINDS_EXT` 与 hft-event 的 `KINDS` 合并无冲突。
- fuzz:`fuzz/ext_frame`、`fuzz/relay_frame`(任意字节不 panic)。
- 验收:chain-core 与 hft-core 用同一 `schema_hash` 握手成功;chain journal 由 `journal-verify` 校验通过。

## 依赖

- 内部:[hft-types](../hft-types/README.md)、[hft-event](../hft-event/README.md)(`Body`、`KindInfo`、`PartHdr`、seq journal 中的链上体)。
- 外部:`zerocopy`(同版本)。
- 禁止:任何 I/O、`alloy`/`ruint`(U256 以 `[u64; 4]` 肢交换)。

## 延伸阅读

- [external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)、[chain-grpc-relay 帧格式](../../apps/chain-grpc-relay/README.md#帧格式)、[hft-chain-evm](../hft-chain-evm/README.md)、[journal-format](../../docs/subsystems/journal-format.md)
- [event-model kind 编号段](../../docs/subsystems/event-model.md#kind-编号段)

## 已知限制与后续工作

- `KillMap` 只描述 hft-core → chain-core 的每账户 kill 位图;chain-core 自己的 `Pause` 命令编码在 [hft-ctl-proto](../hft-ctl-proto/README.md)。

### 开发备忘

<details><summary>给实现者</summary>

- 在 hft-event 测试里断言 `0x0310–0x033F` 无定长体实现(编号段只归本 crate)。
- 待核实:relay 帧与 ext 帧是否可以共用一个帧头(当前分开定义;部署 chain-grpc-relay 时评估)。

</details>
