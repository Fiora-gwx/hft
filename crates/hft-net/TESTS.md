---
component: hft-net
prefix: NET
spec: README.md
---

# hft-net 测试规格

NET-125 验证同一次 read 的多帧仍有独立测量标识，见 [延迟采集测试](../../tools/latency-report/TESTS.md)。

公共 REST 池交接、超时退役与重建的跨组件回归见 [hft-venue VEN-151–161](../hft-venue/TESTS.md#ven-151-每条-snapreq-恰好得到一条-snap-记录)；脚本 wait fd 的观察由该组件 VEN-210 覆盖。

格式与规则见 [docs/tests](../../docs/tests/README.md)。

CORE-120 非正式账户启动链路由 `apps/hft-core/tests/account_process.rs::core_120_mock_accounts_bootstrap_reauth_and_drift` 以独立子进程、两家真实协议和脚本 REST 验证撤空、基线、重新核验、超时/错误模式失败与余额差异不覆盖账本；不替代真实 TLS、交易所或正式恢复验收。资金费账单复用 FundingPoll，沿用现有实际结算记账与去重。

NET-080/081 同时验证缓冲优先轮询的 syscall 计数、跨记录帧、部分记录补读与时间戳不变；NET-320/322 的 `wire_poll` 基准逐条采样，并分别报告脚本 recv 调用数与 SPSC 批量交接。

## 覆盖

| README 小节 | 用例 |
|---|---|
| Warm REST 单请求传输 | NET-260–NET-262 |
| 概要 | NET-080、NET-151、NET-205、NET-220、NET-286、NET-372 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 类型总览 | NET-011、NET-287 |
| 签名草案 | NET-001–NET-010、NET-026、NET-029、NET-042、NET-043、NET-060、NET-062–NET-066、NET-082、NET-084、NET-085、NET-100–NET-102、NET-122、NET-130、NET-150、NET-158–NET-161、NET-200、NET-202、NET-203、NET-220–NET-227、NET-240–NET-253、NET-271、NET-272、NET-274、NET-275、NET-362 |
| 调用约束 | NET-011、NET-041、NET-065、NET-090、NET-155、NET-156、NET-180、NET-212、NET-220、NET-245、NET-287 |
| 配置项 | NET-038、NET-044、NET-099、NET-121、NET-157、NET-199、NET-207、NET-209、NET-213、NET-250、NET-264、NET-265、NET-300–NET-302、NET-340–NET-342、NET-360、NET-375 |
| 实现说明 | 不适用:见子节 |
| 1. 接收路径 | NET-020–NET-044、NET-080–NET-099、NET-120、NET-127、NET-335、NET-340、NET-374 |
| 2. 发送路径 | NET-060、NET-061、NET-065、NET-066、NET-150–NET-163、NET-341、NET-342 |
| 3. `warm` 与 scratch 会话(scratch 预热可选,默认关) | NET-151、NET-164、NET-180–NET-182 |
| 4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`) | NET-190–NET-213、NET-353、NET-371 |
| 5. socket 选项清单 | NET-205、NET-301、NET-372、NET-375 |
| 6. 源端口预计算(`RssPlan`,可选) | NET-350–NET-353 |
| 7. 时间戳 | NET-002、NET-080、NET-081、NET-120–NET-136、NET-274、NET-337、NET-360–NET-362、NET-370、NET-373、NET-376 |
| 8. `ConnPool` 与轮换 | NET-240–NET-254 |
| 9. REST 客户端 | NET-260–NET-277、NET-336 |
| 10. 内存 | NET-285、NET-287 |
| 11. 源文件规划 | NET-164、NET-080 |
| 延迟与资源契约 | NET-086、NET-267、NET-285、NET-286、NET-320–NET-331 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | NET-295 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | NET-370 |
| 开发备忘 | NET-371–NET-375 |

## 用例

夹具(`crates/hft-net/tests/support/`,只给本文件的用例用):

- `ScriptIo`:feature `test-io` 的脚本化 socket([源文件规划](README.md#11-源文件规划) `src/sock.rs`)。每次 `recvmsg`/`sendmsg` 按脚本给出返回值、字节、cmsg 戳与调用前后的 TSC;`SysCount` 按系统调用种类计数。
- `MemPeer`:进程内 rustls 0.23 server(套件 `TLS13_AES_128_GCM_SHA256`),与被测 `Conn` 经 `ScriptIo` 交换密文;可把任意明文字节(含手工构造的 WS 帧)按指定的记录边界加密,并记录收到的每条记录的序号与明文。
- `LoopPeer`:回环 TCP 上的 TLS + WS/HTTP 测试服务器线程(自签 CA 经 `extra_ca` 加载,配置带 `endpoint_override`);Linux 专用。
- 测试时钟:频率 1 GHz 构造的 hft-rt `TscClock`(1 tick = 1 ns);用例中的 TSC 值即 ns。
- 除非另写,`MemPeer`/`ScriptIo` 用例的 `NetConfig` 取:`rx_buf = 65536`、`tx_buf = 16384`、`max_msg = 65536`、`ts_expect_hw = false`、`tls_mode = unbuffered`、`quickack_after_read = {false, false}`,MSS = 1448。

### NET-001 `Timestamps` 为 32 B,字段偏移固定

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/hft-net/tests/layout.rs::net_001_timestamps_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| NET-001.1 | `size_of::<Timestamps>()` | 32 |
| NET-001.2 | `align_of::<Timestamps>()` | 8 |
| NET-001.3 | `offset_of!(hw_rx_ns)` / `sw_rx_ns` / `rx_tsc` | 0 / 8 / 16 |
| NET-001.4 | `offset_of!(flags)` / `rx_empty_gap_ns` | 24 / 28 |
| NET-001.5 | `Timestamps::default()` | 五个字段全为 0 |

### NET-002 `TsFlags` 码值为 HW = 1、SW = 2、MULTI_SKB = 4

- 规则:[签名草案](README.md#签名草案)、[7. 时间戳](README.md#7-时间戳)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/hft-net/tests/layout.rs::net_002_ts_flags_values`

| 行 | 常量 | 期望值 |
|---|---|---|
| NET-002.1 | `TsFlags::HW` | 1 |
| NET-002.2 | `TsFlags::SW` | 2 |
| NET-002.3 | `TsFlags::MULTI_SKB` | 4 |
| NET-002.4 | 三者两两按位与 | 0 |

### NET-003 `best_rx` 有 HW 取 HW,否则取 SW 并要求置 SW_TS,都没有为 (0, false)

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:`hw_rx_ns = 1_000`、`sw_rx_ns = 900`(按行改 `flags`)
- 实现:`crates/hft-net/tests/layout.rs::net_003_best_rx`

| 行 | `flags` | 期望 `best_rx()` |
|---|---|---|
| NET-003.1 | HW、SW | `(1_000, false)` |
| NET-003.2 | HW | `(1_000, false)` |
| NET-003.3 | SW | `(900, true)` |
| NET-003.4 | SW、MULTI_SKB | `(900, true)` |
| NET-003.5 | 0 | `(0, false)` |

### NET-004 `ConnPhase` 码值 Free = 0 … Dead = 6

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/hft-net/tests/layout.rs::net_004_conn_phase_codes`

| 行 | 变体 | `as u8` |
|---|---|---|
| NET-004.1 | `Free` | 0 |
| NET-004.2 | `TcpConnecting` | 1 |
| NET-004.3 | `TlsHandshake` | 2 |
| NET-004.4 | `WsUpgrade` | 3 |
| NET-004.5 | `Open` | 4 |
| NET-004.6 | `Closing` | 5 |
| NET-004.7 | `Dead` | 6 |

### NET-005 `Opcode` 只有 Text = 1 与 Binary = 2

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/hft-net/tests/layout.rs::net_005_opcode_codes`

| 行 | 断言 | 期望 |
|---|---|---|
| NET-005.1 | `Opcode::Text as u8` | 1 |
| NET-005.2 | `Opcode::Binary as u8` | 2 |
| NET-005.3 | `Opcode` 的变体数 | 2 |

### NET-006 `ErrBits` 位值固定,`FATAL` 恰为低 16 位

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/hft-net/tests/layout.rs::net_006_err_bits_values`

| 行 | 常量 | 期望 |
|---|---|---|
| NET-006.1 | `PEER_EOF`/`RESET`/`IO`/`TLS_ALERT` | `1<<0` / `1<<1` / `1<<2` / `1<<3` |
| NET-006.2 | `TLS_PROTO`/`WS_PROTO`/`WS_CLOSE`/`RX_FULL` | `1<<4` / `1<<5` / `1<<6` / `1<<7` |
| NET-006.3 | `TX_BACKLOG`/`PING_TIMEOUT`/`IDLE_TIMEOUT`/`HTTP_PROTO` | `1<<8` / `1<<9` / `1<<10` / `1<<11` |
| NET-006.4 | `TX_PARTIAL`/`TS_MISSING`/`WRONG_QUEUE` | `1<<16` / `1<<17` / `1<<18` |
| NET-006.5 | `KEY_UPDATE`/`ROTATE_DUE`/`SERVER_HINT` | `1<<19` / `1<<20` / `1<<21` |
| NET-006.6 | `FATAL.bits()` | `0x0000_FFFF` |
| NET-006.7 | 第 1–3 行每个常量 `& FATAL` | 等于自身 |
| NET-006.8 | 第 4–5 行每个常量 `& FATAL` | 0 |

### NET-007 `ConnKey` 4 B、`TxStamp` 24 B、`TxTag` 24 B,偏移固定

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/hft-net/tests/layout.rs::net_007_txts_layouts`

| 行 | 断言 | 期望 |
|---|---|---|
| NET-007.1 | `size_of::<ConnKey>()`;`venue`/`conn`/`_pad` 偏移 | 4;0 / 1 / 2 |
| NET-007.2 | `size_of::<TxStamp>()`;`key`/`tskey`/`kind`/`ts_ns` 偏移 | 24;0 / 4 / 8 / 16 |
| NET-007.3 | `size_of::<TxTag>()`;`key`/`tskey_end`/`slot`/`gen`/`sent_tsc` 偏移 | 24;0 / 4 / 8 / 12 / 16 |

### NET-008 `MaskedFrame` 64 字节对齐,payload 起于固定偏移

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/hft-net/tests/layout.rs::net_008_masked_frame_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| NET-008.1 | `align_of::<MaskedFrame<2048>>()` | 64 |
| NET-008.2 | `size_of::<MaskedFrame<2048>>()` / `size_of::<MaskedFrame<8192>>()` | 2112 / 8256 |
| NET-008.3 | `start`/`len`/`opcode`/`sealed`/`_pad`/`buf` 偏移 | 0 / 2 / 4 / 5 / 6 / 8 |
| NET-008.4 | `MaskedFrame::<2048>::HDR_MAX`;新帧 `body_mut().len()` | 14;2034 |
| NET-008.5 | `MaskedFrame::<2048>::new()` 的 `sealed` | 0 |

### NET-009 `MaskedFrame` 的 CAP 超过 65 535 编译失败

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:trybuild 编译失败用例 `tests/compile_fail/masked_frame_cap.rs`
- 步骤:
  1. 编译含 `MaskedFrame::<65_536>::new()` 的文件
  2. 编译含 `MaskedFrame::<65_535>::new()` 的文件
- 期望:
  - 步骤 1 编译失败,错误信息含 `CAP`
  - 步骤 2 编译成功
- 实现:`crates/hft-net/tests/compile_fail.rs::net_009_masked_frame_cap_over_u16`

### NET-010 `ConnSet` 按 ConnId 置位,覆盖 0 与 255

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:`ConnSet { bits: [0; 4] }`
- 实现:`crates/hft-net/tests/ready.rs::net_010_conn_set_bits`

| 行 | 置入的 ConnId | 期望 `bits` |
|---|---|---|
| NET-010.1 | 0 | `[1, 0, 0, 0]` |
| NET-010.2 | 63 | `[1<<63, 0, 0, 0]` |
| NET-010.3 | 64 | `[0, 1, 0, 0]` |
| NET-010.4 | 255 | `[0, 0, 0, 1<<63]` |

### NET-011 `Conn` 不是 `Sync`,`Handoff`/`RetireMsg` 是 `Send`

- 规则:[类型总览](README.md#类型总览)、[调用约束](README.md#调用约束)
- 级别:unit
- 里程碑:M1.5
- 前置:`static_assertions`
- 步骤:
  1. `assert_not_impl_any!(Conn: Sync)`
  2. `assert_impl_all!(Handoff: Send)`、`assert_impl_all!(RetireMsg: Send)`、`assert_impl_all!(ReadyHandle: Send)`
- 期望:
  - 两条断言都编译通过;把 `Conn` 改为 `Sync` 的变更使步骤 1 编译失败
- 实现:`crates/hft-net/tests/layout.rs::net_011_conn_not_sync`

### NET-020 服务端帧长度 0/125/126/65535/65536 都解出原长 payload

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer`;`rx_buf = 262144`、`max_msg = 131072`;每帧按 16 KiB 记录切分;服务端帧不带掩码
- 实现:`crates/hft-net/tests/ws_rx.rs::net_020_server_frame_lengths`

| 行 | payload 长度与头 | 期望 `next_frame()` |
|---|---|---|
| NET-020.1 | 0,头 `81 00` | `Some(Frame{opcode: Text, payload.len() = 0, reassembled: false})` |
| NET-020.2 | 125,头 `81 7D` | payload 125 B 与发送逐字节相同 |
| NET-020.3 | 126,头 `81 7E 00 7E` | payload 126 B 与发送逐字节相同 |
| NET-020.4 | 65535,头 `81 7E FF FF` | payload 65535 B 与发送逐字节相同 |
| NET-020.5 | 65536,头 `81 7F 00 00 00 00 00 01 00 00` | payload 65536 B 与发送逐字节相同 |

### NET-021 服务端帧带 MASK 位时置 WS_PROTO,不产出帧

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`;连接 `Open`
- 步骤:
  1. 服务端发明文 `81 85 01 02 03 04` + 5 B 掩码后的 `hello`
  2. `poll_rx`,`next_frame`
- 期望:
  - `next_frame()` 返回 `None`;`err()` 含 `WS_PROTO`;`phase() == Dead`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_021_masked_server_frame_rejected`

### NET-022 RSV1、RSV2、RSV3 任一置位都置 WS_PROTO

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer`;payload 为 `"x"`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_022_rsv_bits_rejected`

| 行 | 首字节 | 期望 |
|---|---|---|
| NET-022.1 | `0xC1`(RSV1) | `next_frame() == None`;`err()` 含 `WS_PROTO` |
| NET-022.2 | `0xA1`(RSV2) | 同上 |
| NET-022.3 | `0x91`(RSV3) | 同上 |
| NET-022.4 | `0x81`(对照) | 解出 `Frame{opcode: Text, payload: b"x"}`,`err()` 为空 |

### NET-023 控制帧载荷 124、125 B 回 Pong,126 B 置 WS_PROTO

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer`;每行一条新连接;服务端发 Ping
- 实现:`crates/hft-net/tests/ws_rx.rs::net_023_control_frame_len_limit`

| 行 | Ping 载荷长度 | 期望(`finish_poll` 之后) |
|---|---|---|
| NET-023.1 | 124 | 服务端收到 1 个 Pong,载荷 124 B 与 Ping 相同;`err()` 为空 |
| NET-023.2 | 125 | 服务端收到 1 个 Pong,载荷 125 B 与 Ping 相同;`err()` 为空 |
| NET-023.3 | 126(头 `89 7E 00 7E`) | 服务端收到 0 个 Pong;`err()` 含 `WS_PROTO` |

### NET-024 分片的控制帧置 WS_PROTO

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 FIN = 0 的 Ping(首字节 `0x09`,载荷 `"ab"`)
  2. `poll_rx`、`next_frame`、`finish_poll`
- 期望:
  - `err()` 含 `WS_PROTO`;服务端收到 0 个 Pong
- 实现:`crates/hft-net/tests/ws_rx.rs::net_024_fragmented_control_rejected`

### NET-025 未知 opcode 置 WS_PROTO

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer`;FIN = 1,载荷 `"x"`;每行一条新连接
- 实现:`crates/hft-net/tests/ws_rx.rs::net_025_unknown_opcode_rejected`

| 行 | opcode | 期望 |
|---|---|---|
| NET-025.1 | 0x3 | `next_frame() == None`;`err()` 含 `WS_PROTO` |
| NET-025.2 | 0x7 | 同上 |
| NET-025.3 | 0xB | 同上 |
| NET-025.4 | 0xF | 同上 |
| NET-025.5 | 0x2(对照) | 解出 `Frame{opcode: Binary}`;`err()` 为空 |

### NET-026 Ping 排队同载荷的 Pong,在 finish_poll 发出且不作为帧返回

- 规则:[1. 接收路径](README.md#1-接收路径)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 Ping(载荷 `"p1"`)后跟 Text `"d1"`
  2. `poll_rx`;`next_frame` 两次;`finish_poll`
- 期望:
  - 第一次 `next_frame` 返回 `Text "d1"`,第二次 `None`(Ping 不外露)
  - `finish_poll` 之前服务端未收到 Pong;之后收到恰好 1 个 Pong,载荷 `"p1"`,帧带 MASK 位
- 实现:`crates/hft-net/tests/ws_rx.rs::net_026_ping_queues_pong`

### NET-027 一轮内 4 个 Ping 按序回 4 个 Pong

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端连发 Ping `"a"`、`"b"`、`"c"`、`"d"`
  2. `poll_rx`;`next_frame` 直到 `None`;`finish_poll`
- 期望:
  - 服务端按序收到 4 个 Pong:`"a"`、`"b"`、`"c"`、`"d"`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_027_four_pings_four_pongs`

### NET-028 一轮内第 5 个 Ping 使队列清空,只回最后一个

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端连发 Ping `"a"`、`"b"`、`"c"`、`"d"`、`"e"`
  2. `poll_rx`;`next_frame` 直到 `None`;`finish_poll`
- 期望:
  - 服务端恰好收到 1 个 Pong,载荷 `"e"`;`err()` 为空
- 实现:`crates/hft-net/tests/ws_rx.rs::net_028_pong_queue_overflow_keeps_last`

### NET-029 载荷为最近一次 send_ping 的 tsc 的 Pong 记一次 RTT

- 规则:[1. 接收路径](README.md#1-接收路径)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;测试时钟
- 步骤:
  1. `send_ping(1_000_000)`;服务端读到 Ping,载荷 = `1_000_000u64.to_le_bytes()`
  2. 服务端回 Pong(同载荷);`poll_rx` 的那次 `recvmsg` 返回后 TSC = 1_250_000;`next_frame`
- 期望:
  - `next_frame() == None`;`stats().rtt` 恰有 1 个样本,值 250_000 ns
  - `last_rx_tsc() == 1_250_000`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_029_pong_records_rtt`

### NET-030 载荷不是我们发出的 tsc 的 Pong 不记 RTT、不置错误位

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer`;已 `send_ping(1_000_000)`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_030_foreign_pong_ignored`

| 行 | Pong 载荷 | 期望 |
|---|---|---|
| NET-030.1 | `999_999u64.to_le_bytes()` | `stats().rtt` 样本数 0;`err()` 为空;`last_rx_tsc` 前进 |
| NET-030.2 | 空 | 同上 |
| NET-030.3 | 7 B | 同上 |

### NET-031 Close(1000) 回显 Close(1000),置 WS_CLOSE,相位 Open → Closing → Dead

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`;连接 `Open`
- 步骤:
  1. 服务端发 Close,载荷 `03 E8`
  2. `poll_rx`;`next_frame`
  3. `finish_poll`
- 期望:
  - 步骤 2:`next_frame() == None`;`err()` 含 `WS_CLOSE`;`phase() == Closing`
  - 步骤 3:服务端收到 1 个带掩码的 Close,载荷 `03 E8`;`phase() == Dead`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_031_close_echo_and_phase`

### NET-032 无载荷的 Close 回显无载荷的 Close

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 Close(载荷长度 0);`poll_rx`、`next_frame`、`finish_poll`
- 期望:
  - 服务端收到 1 个 Close,载荷长度 0;`err()` 含 `WS_CLOSE`;`phase() == Dead`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_032_empty_close_echo`

### NET-033 载荷 1 B 的 Close 置 WS_PROTO 且不回显

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 Close,载荷 `03`;`poll_rx`、`next_frame`、`finish_poll`
- 期望:
  - `err()` 含 `WS_PROTO`、不含 `WS_CLOSE`;服务端收到 0 个 Close;`phase() == Dead`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_033_one_byte_close_rejected`

### NET-034 Text 分片拼成一条消息,reassembled = true

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 `01 03 "abc"`(Text,FIN = 0)、`00 03 "def"`(continuation,FIN = 0)、`80 02 "gh"`(continuation,FIN = 1)
  2. `poll_rx`;`next_frame` 两次
- 期望:
  - 第一次:`Frame{opcode: Text, payload: b"abcdefgh", reassembled: true}`;第二次 `None`
  - `stats().reassembled == 1`;`err()` 为空
- 实现:`crates/hft-net/tests/ws_rx.rs::net_034_fragments_reassembled`

### NET-035 没有未完成分片时收到 continuation 置 WS_PROTO

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 `80 02 "gh"`(continuation,FIN = 1),此前无分片
  2. `poll_rx`;`next_frame`
- 期望:
  - `next_frame() == None`;`err()` 含 `WS_PROTO`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_035_orphan_continuation_rejected`

### NET-036 分片进行中收到新的 Text 起始帧置 WS_PROTO

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 `01 03 "abc"`(FIN = 0),再发 `81 01 "x"`(新 Text,FIN = 1)
  2. `poll_rx`;`next_frame`
- 期望:
  - `next_frame() == None`;`err()` 含 `WS_PROTO`;没有 payload 为 `"x"` 或 `"abc"` 的帧被返回
- 实现:`crates/hft-net/tests/ws_rx.rs::net_036_new_message_during_fragments_rejected`

### NET-037 分片之间插入 Ping 照常回 Pong,消息照常拼接

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 `01 03 "abc"`、Ping `"p"`、`80 03 "def"`
  2. `poll_rx`;`next_frame` 直到 `None`;`finish_poll`
- 期望:
  - 返回恰好一帧 `Text "abcdef"`,`reassembled: true`;服务端收到 1 个 Pong `"p"`;`err()` 为空
- 实现:`crates/hft-net/tests/ws_rx.rs::net_037_ping_between_fragments`

### NET-038 长度字段等于 max_msg 时解出,超过 1 B 置 RX_FULL

- 规则:[1. 接收路径](README.md#1-接收路径)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer`;`max_msg = 1000`;`rx_buf = 65536`;每行一条新连接
- 实现:`crates/hft-net/tests/ws_rx.rs::net_038_max_msg_boundary`

| 行 | 帧长度字段 | 期望 |
|---|---|---|
| NET-038.1 | 999 | 解出 999 B 帧;`err()` 为空 |
| NET-038.2 | 1000 | 解出 1000 B 帧;`err()` 为空 |
| NET-038.3 | 1001 | 解析到帧头即置 `RX_FULL`(此时 payload 只到达 10 B);`next_frame() == None` |

### NET-039 分片累计超过 max_msg 置 RX_FULL

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`;`max_msg = 1000`
- 步骤:
  1. 服务端发 Text 分片 600 B(FIN = 0),再发 continuation 401 B(FIN = 1)
  2. `poll_rx`;`next_frame`
- 期望:
  - `next_frame() == None`;`err()` 含 `RX_FULL`;`stats().reassembled == 0`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_039_fragments_exceed_max_msg`

### NET-040 未分片的数据帧 payload 借用接收缓冲,不复制

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`;`ScriptIo` 暴露接收缓冲的地址区间 `[rx_base, rx_base + rx_buf)`
- 步骤:
  1. 服务端发 Text 300 B;`poll_rx`;`next_frame`
- 期望:
  - `payload.as_ptr()` 落在 `[rx_base, rx_base + rx_buf)` 内;`reassembled == false`;`stats().reassembled == 0`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_040_payload_borrows_rx_buffer`

### NET-041 在上一帧借用期间调用 next_frame 不能编译

- 规则:[调用约束](README.md#调用约束)
- 级别:unit
- 里程碑:M1.5
- 前置:trybuild 用例 `tests/compile_fail/frame_borrow.rs`
- 步骤:
  1. 编译:`let a = c.next_frame().unwrap(); let b = c.next_frame(); use(a.payload);`
- 期望:
  - 编译失败,错误为借用冲突(E0499)
- 实现:`crates/hft-net/tests/compile_fail.rs::net_041_frame_borrow_exclusive`

### NET-042 last_rx_tsc 在任一完整帧解出时前进,半帧不前进

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;初始 `last_rx_tsc() == 0`;每行的那次 `recvmsg` 返回后 TSC = 5_000
- 实现:`crates/hft-net/tests/ws_rx.rs::net_042_last_rx_tsc_advances`

| 行 | 服务端发送 | 期望 `next_frame` 之后的 `last_rx_tsc()` |
|---|---|---|
| NET-042.1 | Text `"d"` | 5_000 |
| NET-042.2 | Ping `"p"` | 5_000 |
| NET-042.3 | Pong `"q"` | 5_000 |
| NET-042.4 | Close `03 E8` | 5_000 |
| NET-042.5 | Text 头声明 100 B,只到达 40 B | 0 |

### NET-043 Binary 帧解出 opcode = Binary

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 `82 03 01 02 03`;`poll_rx`;`next_frame`
- 期望:
  - `Frame{opcode: Binary, payload: [1, 2, 3], reassembled: false}`
- 实现:`crates/hft-net/tests/ws_rx.rs::net_043_binary_frame`

### NET-044 接收缓冲容量边界:一帧一记录时帧明文 4074 B 可解出,4075 B 置 RX_FULL

- 规则:[1. 接收路径](README.md#1-接收路径)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer`;`rx_buf = 4096`、`max_msg = 65536`;整条 WS 帧放进一条 TLS 记录(记录开销 22 B);WS 头 4 B;每行一条新连接
- 实现:`crates/hft-net/tests/ws_rx.rs::net_044_rx_buf_boundary`

| 行 | payload 长度(帧明文 = 长度 + 4) | 期望 |
|---|---|---|
| NET-044.1 | 4069 | 解出 4069 B 帧;`err()` 为空 |
| NET-044.2 | 4070 | 解出 4070 B 帧;`err()` 为空 |
| NET-044.3 | 4071 | `next_frame() == None`;`err()` 含 `RX_FULL` |

### NET-060 seal 写出的 WS 头与偏移符合长度档位

- 规则:[签名草案](README.md#签名草案)、[2. 发送路径](README.md#2-发送路径)
- 级别:table
- 里程碑:M1.5
- 前置:`MaskedFrame<2048>`;`mask_key = 0x11223344`
- 实现:`crates/hft-net/tests/ws_tx.rs::net_060_seal_header_encoding`

| 行 | `(op, len)` | 期望 `as_bytes()` 前若干字节;`start`;`as_bytes().len()` |
|---|---|---|
| NET-060.1 | `(Text, 0)` | `81 80 11 22 33 44`;8;6 |
| NET-060.2 | `(Text, 125)` | `81 FD 11 22 33 44`;8;131 |
| NET-060.3 | `(Text, 126)` | `81 FE 00 7E 11 22 33 44`;6;134 |
| NET-060.4 | `(Binary, 2034)` | `82 FE 07 F2 11 22 33 44`;6;2042 |

### NET-061 seal 的掩码结果与参考实现逐字节一致

- 规则:[2. 发送路径](README.md#2-发送路径)
- 级别:prop
- 里程碑:M1.5
- 性质:对任意 `len`、`mask_key` 与 payload,`seal` 后 `as_bytes()` 等于参考实现的输出;参考实现解掩码后得回原 payload
- 生成:`len ∈ 0..=2034`(`MaskedFrame<2048>`)与 `0..=8178`(`MaskedFrame<8192>`);`mask_key ∈ u32`;payload 随机字节;`op ∈ {Text, Binary}`
- 参考:`tests/support/ws_ref.rs` 的逐字节实现(RFC 6455 §5.2、§5.3,键字节 = `mask_key.to_be_bytes()`)
- 例数:PR 10 000
- 实现:`crates/hft-net/tests/ws_tx.rs::net_061_seal_matches_reference`

### NET-062 seal 之后调用 body_mut 触发 debug_assert

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:debug 构建;`MaskedFrame<2048>` 已 `seal(Text, 10, 1)`
- 步骤:
  1. 调用 `body_mut()`
- 期望:
  - panic,信息含 `sealed`(`#[should_panic]`)
- 实现:`crates/hft-net/tests/ws_tx.rs::net_062_body_mut_after_seal_asserts`

### NET-063 seal 的 len 边界:CAP − 14 可 seal,CAP − 13 触发 debug_assert

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:debug 构建;`MaskedFrame<2048>`;每行新帧
- 实现:`crates/hft-net/tests/ws_tx.rs::net_063_seal_len_boundary`

| 行 | `len` | 期望 |
|---|---|---|
| NET-063.1 | 2033 | `sealed == 1`;`as_bytes().len() == 2041` |
| NET-063.2 | 2034 | `sealed == 1`;`as_bytes().len() == 2042` |
| NET-063.3 | 2035 | panic(debug_assert) |

### NET-064 对已 seal 的帧再 seal 触发 debug_assert;unseal 后可再填写

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:debug 构建;`MaskedFrame<2048>`
- 步骤:
  1. `seal(Text, 3, 7)` 后再 `seal(Text, 3, 7)`
  2. 新帧 `seal(Text, 3, 7)`;`unseal()`;`body_mut()[..2] = b"ok"`;`seal(Text, 2, 9)`
- 期望:
  - 步骤 1 第二次 seal panic
  - 步骤 2 不 panic;`sealed == 1`;参考实现解掩码后 payload = `"ok"`
- 实现:`crates/hft-net/tests/ws_tx.rs::net_064_double_seal_asserts`

### NET-065 未 seal 的帧 send_frame 返回 NotSealed,不加密不发送

- 规则:[2. 发送路径](README.md#2-发送路径)、[调用约束](README.md#调用约束)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;连接 `Open`;新帧写入 10 B 但未 seal
- 步骤:
  1. `send_frame(&f)`
- 期望:
  - 返回 `Err(SendErr::NotSealed)`;`SysCount.sendmsg == 0`;真实会话的发送记录序号不变(服务端此后收到的下一条记录序号仍为 0)
- 实现:`crates/hft-net/tests/ws_tx.rs::net_065_unsealed_frame_rejected`

### NET-066 next_mask 给出每连接确定的 xorshift64* 序列

- 规则:[2. 发送路径](README.md#2-发送路径)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo`;两条连接 c1、c2,掩码种子分别为 1 与 2(测试注入,代替 `getrandom`)
- 步骤:
  1. 各取 `next_mask()` 1 000 次
  2. 对 c1 `send_text(b"a")`、`send_text(b"b")`
- 期望:
  - c1 的序列与 `tests/support/xorshift.rs` 以种子 1 算出的序列逐值相同;c1 与 c2 的第一个值不同
  - 步骤 2 服务端收到的两帧掩码键 = c1 序列的第 1001、1002 个值
- 实现:`crates/hft-net/tests/ws_tx.rs::net_066_mask_key_sequence`

### NET-080 有数据的短读只做一次 recvmsg,并确认队列为空

- 规则:[1. 接收路径](README.md#1-接收路径)、[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo`;空闲空间 65536 B;脚本第 1 次 `recvmsg`:调用前 TSC 1_000、返回 300 B、返回后 TSC 1_100
- 步骤:
  1. `poll_rx(0)`
  2. 脚本第 2 次 `recvmsg`:调用前 TSC 2_000、返回 100 B、返回后 TSC 2_400;`poll_rx(0)`
- 期望:
  - 步骤 1 返回 300;`SysCount.recvmsg == 1`;脚本中没有被消费的第 2 次调用(无补读的 `EAGAIN`)
  - 步骤 2 这次读取记下的 `rx_empty_gap_ns == 1_400`(= 2_400 − 步骤 1 的调用前 TSC 1_000)
- 实现:`crates/hft-net/tests/rx_path.rs::net_080_short_read_single_syscall`

### NET-081 读满全部空闲空间时不再读,也不算确认为空

- 规则:[1. 接收路径](README.md#1-接收路径)、[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo`;`rx_buf = 4096`、缓冲为空;此前一次 `EAGAIN` 的调用前 TSC = 500;脚本:调用前 TSC 1_000、返回 4096 B、返回后 TSC 1_050
- 步骤:
  1. `poll_rx(0)`
  2. 脚本:调用前 TSC 3_000、返回 10 B、返回后 TSC 3_200;消费掉缓冲里的全部帧后 `poll_rx(0)`
- 期望:
  - 步骤 1 返回 4096;`SysCount.recvmsg == 1`;这次读取的 `rx_empty_gap_ns == 550`
  - 步骤 2 这次读取的 `rx_empty_gap_ns == 2_700`(起点仍是 500,读满那次没有确认)
- 实现:`crates/hft-net/tests/rx_path.rs::net_081_full_read_no_confirmation`

### NET-082 空闲空间为 0 时 poll_rx 不调用 recvmsg,不置 PEER_EOF

- 规则:[1. 接收路径](README.md#1-接收路径)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo`;`rx_buf = 4096`,缓冲已被一条未完成的记录占满(未调用 `next_frame` 消费)
- 步骤:
  1. `poll_rx(0)`
- 期望:
  - 返回 0;`SysCount.recvmsg == 0`;`err()` 不含 `PEER_EOF`;`phase() == Open`
- 实现:`crates/hft-net/tests/rx_path.rs::net_082_zero_free_space_no_syscall`

### NET-083 EAGAIN 返回 0 并确认队列为空,无错误位

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo`;脚本:调用前 TSC 7_000、返回 `EAGAIN`
- 步骤:
  1. `poll_rx(0)`
  2. 脚本:调用前 8_000、返回 50 B、返回后 8_500;`poll_rx(0)`
- 期望:
  - 步骤 1 返回 0;`err()` 为空;`SysCount.recvmsg == 1`
  - 步骤 2 这次读取的 `rx_empty_gap_ns == 1_500`
- 实现:`crates/hft-net/tests/rx_path.rs::net_083_eagain_confirms_empty`

### NET-084 recvmsg 返回 0 置 PEER_EOF,相位 Dead

- 规则:[签名草案](README.md#签名草案)、[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo`;脚本返回 0
- 步骤:
  1. `poll_rx(0)`;再 `poll_rx(0)`
- 期望:
  - `err()` 含 `PEER_EOF`;`phase() == Dead`;第二次 `poll_rx` 返回 0 且 `SysCount.recvmsg == 1`
- 实现:`crates/hft-net/tests/rx_path.rs::net_084_eof_sets_peer_eof`

### NET-085 recvmsg 的 errno 映射到 RESET 或 IO

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:`ScriptIo`;每行一条新连接;脚本 `recvmsg` 返回 −1 与该 errno
- 实现:`crates/hft-net/tests/rx_path.rs::net_085_recv_errno_mapping`

| 行 | errno | 期望 |
|---|---|---|
| NET-085.1 | `ECONNRESET`(104) | `err()` 含 `RESET`;`last_errno() == 104`;`phase() == Dead` |
| NET-085.2 | `ETIMEDOUT`(110) | `err()` 含 `RESET`;`last_errno() == 110` |
| NET-085.3 | `ENOMEM`(12) | `err()` 含 `IO`、不含 `RESET`;`last_errno() == 12` |
| NET-085.4 | `EINTR`(4) | `err()` 含 `IO`;`last_errno() == 4` |

### NET-086 回环上对端写 300 B 时一次 recvmsg 读完且不补读

- 规则:[1. 接收路径](README.md#1-接收路径)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`(WS);`rx_buf = 262144`;`SysCount` 在真实 socket 上计数
- 步骤:
  1. 对端写一个 Text 帧(TLS 后约 300 B);等待 1 ms
  2. `poll_rx`;`next_frame` 直到 `None`
- 期望:
  - `SysCount.recvmsg == 1`,没有返回 `EAGAIN` 的调用;解出 1 帧,内容与对端写入相同
- 实现:`crates/hft-net/tests/loopback.rs::net_086_loopback_short_read`

### NET-087 回环上积压超过空闲空间时分两次 poll_rx 读完,帧序列完整

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`(WS);`rx_buf = 65536`
- 步骤:
  1. 对端连续写 100 个 1 KiB Text 帧(约 103 KB 密文);等待 5 ms
  2. 循环:`poll_rx`;`next_frame` 直到 `None`;直到累计 100 帧
- 期望:
  - 第一次 `poll_rx` 读满 65536 B;解出的 100 帧按序与写入逐字节相同;每次 `poll_rx` 的 `recvmsg` 至多 1 次
- 实现:`crates/hft-net/tests/loopback.rs::net_087_loopback_backlog_two_polls`

### NET-088 poll_rx 先压缩缓冲,半帧在压缩后照常完成

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;`rx_buf = 4096`
- 步骤:
  1. 第 1 次读取:帧 A(1000 B)完整 + 帧 B 的前 2000 B;`poll_rx`;`next_frame` 取出 A
  2. 第 2 次读取:帧 B 余下的 1200 B;`poll_rx`;`next_frame`
- 期望:
  - 步骤 2 `poll_rx` 返回 1200(压缩后空闲空间 ≥ 1200,未读满);`next_frame` 返回帧 B,内容与发送逐字节相同
- 实现:`crates/hft-net/tests/rx_path.rs::net_088_compaction_keeps_partial`

### NET-089 积压 64 KiB 时第一次 next_frame 只解密第一帧所跨的记录

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer`;`rx_buf = 262144`;一次 `poll_rx` 读入约 64 KiB(多条记录、多帧);tls 模块的解密调用计数插桩
- 实现:`crates/hft-net/tests/rx_path.rs::net_089_on_demand_decrypt`

| 行 | 第一帧的记录分布 | 期望第一次 `next_frame` 后的解密记录数 |
|---|---|---|
| NET-089.1 | 第一帧独占 1 条记录 | 1 |
| NET-089.2 | 第一帧跨 2 条记录 | 2 |
| NET-089.3 | 第一帧与第二帧同在 1 条记录 | 1;第二次 `next_frame` 后仍为 1 |

### NET-090 两次 next_frame 之间 send_frame 不影响读位置

- 规则:[1. 接收路径](README.md#1-接收路径)、[调用约束](README.md#调用约束)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`;服务端已发 3 帧 `"f1"`、`"f2"`、`"f3"`,`poll_rx` 一次读入
- 步骤:
  1. `next_frame` 得 `"f1"`;`send_frame`(已 seal 的 `"o1"`)
  2. `next_frame` 两次
- 期望:
  - 步骤 2 依次得 `"f2"`、`"f3"`;服务端解密得 `"o1"`
- 实现:`crates/hft-net/tests/rx_path.rs::net_090_send_between_frames`

### NET-091 一帧跨 1、2、3 条 TLS 记录都解出同一内容

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer`;帧 Text 3000 B
- 实现:`crates/hft-net/tests/rx_path.rs::net_091_frame_across_records`

| 行 | 记录切分(明文字节) | 期望 |
|---|---|---|
| NET-091.1 | 3004 | 解出 3000 B,与发送逐字节相同,`reassembled == false` |
| NET-091.2 | 1000 + 2004 | 同上 |
| NET-091.3 | 1 + 1 + 3002 | 同上 |

### NET-092 任意帧序列在任意字节切分下解出的帧序列不变

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:prop
- 里程碑:M1.5
- 性质:服务端发出的数据帧序列(含分片、Ping、跨记录帧)经任意字节位置切分成任意次 `recvmsg` 后,`next_frame` 依次返回的 `(opcode, payload)` 序列与发送序列相同;Ping 的 Pong 数与 Ping 数一致(每轮 ≤ 4)
- 生成:1–200 帧;payload 0–20 000 B;分片概率 10%;记录大小 1–16 384;切分点在 `[1, 剩余字节]` 均匀抽取
- 参考:发送端记录的帧序列
- 例数:PR 2 000
- 实现:`crates/hft-net/tests/rx_prop.rs::net_092_any_split_same_frames`

### NET-093 服务端 KeyUpdate 后收发照常,置 KEY_UPDATE

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发帧 `"a"`、KeyUpdate(update_requested)、帧 `"b"`;`poll_rx`;`next_frame` 直到 `None`;`finish_poll`
  2. 客户端 `send_frame("c")`
- 期望:
  - 步骤 1 依次解出 `"a"`、`"b"`;`err()` 含 `KEY_UPDATE`,不含任何 `FATAL` 位;`finish_poll` 后服务端收到客户端的 KeyUpdate
  - 步骤 2 服务端以新密钥解密得 `"c"`
- 实现:`crates/hft-net/tests/rx_path.rs::net_093_key_update`

### NET-094 收到 close_notify 置 TLS_ALERT,相位 Dead

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 close_notify;`poll_rx`;`next_frame`
- 期望:
  - `next_frame() == None`;`err()` 含 `TLS_ALERT`;`phase() == Dead`
- 实现:`crates/hft-net/tests/rx_path.rs::net_094_close_notify`

### NET-095 收到致命 alert 置 TLS_ALERT

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 fatal alert `internal_error`(80);`poll_rx`;`next_frame`
- 期望:
  - `err()` 含 `TLS_ALERT`;`phase() == Dead`
- 实现:`crates/hft-net/tests/rx_path.rs::net_095_fatal_alert`

### NET-096 密文损坏一字节置 TLS_PROTO,不产出帧

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`;服务端帧 `"x"` 的记录在 `ScriptIo` 中把认证标签末字节异或 0x01
- 步骤:
  1. `poll_rx`;`next_frame`
- 期望:
  - `next_frame() == None`;`err()` 含 `TLS_PROTO`;`phase() == Dead`
- 实现:`crates/hft-net/tests/rx_path.rs::net_096_corrupt_record`

### NET-097 热线程上出现 BlockedHandshake 置 TLS_PROTO

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo` 测试构造:相位标为 `Open`、rustls 会话仍处于握手中的 `Conn`
- 步骤:
  1. 送入服务端握手字节;`poll_rx`;`next_frame`
- 期望:
  - `next_frame() == None`;`err()` 含 `TLS_PROTO`
- 实现:`crates/hft-net/tests/rx_path.rs::net_097_blocked_handshake_on_hot`

### NET-098 Closing 与 Dead 的连接不读不写

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:table
- 里程碑:M1.5
- 前置:`ScriptIo`;按行把连接带到该相位(Closing:收到 Close 未 `finish_poll`;Dead:`PEER_EOF`);已 seal 的帧 f
- 实现:`crates/hft-net/tests/rx_path.rs::net_098_closing_dead_inert`

| 行 | 相位 | 期望 |
|---|---|---|
| NET-098.1 | Closing | `poll_rx` 返回 0 且 `SysCount.recvmsg` 不增;`next_frame() == None`;`send_frame(&f) == Err(NotOpen(Closing))`;`send_text`、`send_ping` 同为 `NotOpen(Closing)` |
| NET-098.2 | Dead | 同上,`NotOpen(Dead)` |

### NET-099 quickack_after_read 默认不 setsockopt,开启时只在有数据的轮补一次

- 规则:[配置项](README.md#配置项)、[1. 接收路径](README.md#1-接收路径)
- 级别:table
- 里程碑:M1.5
- 前置:`ScriptIo`;Md 角色连接
- 实现:`crates/hft-net/tests/rx_path.rs::net_099_quickack`

| 行 | `quickack_after_read` | 这一轮的读取 | 期望 `finish_poll` 后 `SysCount.setsockopt(TCP_QUICKACK)` |
|---|---|---|---|
| NET-099.1 | `{md: false, order: false}` | 300 B | 0 |
| NET-099.2 | `{md: true, order: false}` | 300 B | 1(值为 1) |
| NET-099.3 | `{md: true, order: false}` | `EAGAIN` | 0 |
| NET-099.4 | `{md: false, order: true}` | 300 B | 0 |

### NET-100 ErrBits 累积到 clear_err 为止

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 KeyUpdate;`poll_rx`、`next_frame`;`mark(SERVER_HINT)`
  2. `err()`;`clear_err()`;`err()`
- 期望:
  - 第一次 `err()` 同时含 `KEY_UPDATE` 与 `SERVER_HINT`;`clear_err` 后 `err()` 为空
- 实现:`crates/hft-net/tests/rx_path.rs::net_100_err_bits_accumulate`

### NET-101 mark 只接受 PING_TIMEOUT、IDLE_TIMEOUT、SERVER_HINT

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:debug 构建;每行新连接
- 实现:`crates/hft-net/tests/rx_path.rs::net_101_mark_allowed_bits`

| 行 | `mark(bits)` | 期望 |
|---|---|---|
| NET-101.1 | `PING_TIMEOUT` | `err()` 含 `PING_TIMEOUT`;`err() & FATAL` 非空 |
| NET-101.2 | `IDLE_TIMEOUT` | `err()` 含 `IDLE_TIMEOUT` |
| NET-101.3 | `SERVER_HINT` | `err()` 含 `SERVER_HINT`;`err() & FATAL` 为空 |
| NET-101.4 | `RESET` | panic(debug_assert) |
| NET-101.5 | `ROTATE_DUE` | panic(debug_assert) |

### NET-102 stats 统计帧数与字节

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. 服务端发 Text 10 B、Binary 20 B、Ping 5 B;`poll_rx`;`next_frame` 直到 `None`
- 期望:
  - `stats().frames == 2`;`stats().bytes == 30`
- 实现:`crates/hft-net/tests/rx_path.rs::net_102_stats_frames_bytes`

### NET-120 cmsg 的 SCM_TIMESTAMPING 解析

- 规则:[1. 接收路径](README.md#1-接收路径)、[7. 时间戳](README.md#7-时间戳)
- 级别:table
- 里程碑:M1.5
- 前置:合成 cmsg 缓冲(`src/cmsg.rs` 解析函数);ts[0] = `{1_700_000_000 s, 5 ns}`,ts[2] = `{1_700_000_000 s, 3 ns}`(按行取舍)
- 实现:`crates/hft-net/tests/cmsg.rs::net_120_cmsg_parse`

| 行 | cmsg 内容 | 期望 `(sw_rx_ns, hw_rx_ns, flags)` |
|---|---|---|
| NET-120.1 | 只有 ts[0] | `(1_700_000_000_000_000_005, 0, SW)` |
| NET-120.2 | 只有 ts[2] | `(0, 1_700_000_000_000_000_003, HW)` |
| NET-120.3 | ts[0] 与 ts[2] | `(…005, …003, HW、SW)` |
| NET-120.4 | ts[1] 非 0,其余为 0 | `(0, 0, 0)` |
| NET-120.5 | 无 `SCM_TIMESTAMPING` | `(0, 0, 0)` |
| NET-120.6 | `MSG_CTRUNC` 置位 | `(0, 0, 0)` |
| NET-120.7 | 数据长度只够两个 timespec | `(0, 0, 0)` |

### NET-121 ts_expect_hw 时缺 HW 戳的数据读取置 TS_MISSING

- 规则:[配置项](README.md#配置项)、[7. 时间戳](README.md#7-时间戳)
- 级别:table
- 里程碑:M1.5
- 前置:`ScriptIo`;每行新连接
- 实现:`crates/hft-net/tests/timestamps.rs::net_121_ts_missing`

| 行 | `ts_expect_hw` | 本次 `recvmsg` | 期望 |
|---|---|---|---|
| NET-121.1 | true | 100 B,只有 SW | `err()` 含 `TS_MISSING` |
| NET-121.2 | true | 100 B,HW 与 SW | `err()` 不含 `TS_MISSING` |
| NET-121.3 | true | `EAGAIN` | `err()` 不含 `TS_MISSING` |
| NET-121.4 | false | 100 B,只有 SW | `err()` 不含 `TS_MISSING` |

### NET-122 stats 统计读到数据的读取次数与其中带 HW 戳的次数

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo`
- 步骤:
  1. 依次:100 B 带 HW;100 B 只有 SW;`EAGAIN`;100 B 带 HW;每次之后 `poll_rx`
- 期望:
  - `stats().ts_reads == 3`;`stats().hw_ts_reads == 2`
- 实现:`crates/hft-net/tests/timestamps.rs::net_122_ts_coverage_counters`

### NET-123 单次读取的单帧取该次读取的戳

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;此前 `EAGAIN` 调用前 TSC 1_000;读取:调用前 2_000、返回后 2_100、300 B、HW = 900_000、SW = 950_000
- 步骤:
  1. `poll_rx`;`next_frame`
- 期望:
  - `frame.ts == Timestamps{hw_rx_ns: 900_000, sw_rx_ns: 950_000, rx_tsc: 2_100, flags: HW、SW, rx_empty_gap_ns: 1_100}`
- 实现:`crates/hft-net/tests/timestamps.rs::net_123_single_read_frame_stamps`

### NET-124 跨两次读取的帧:HW/SW 取首次,rx_tsc 取凑齐的那次,间隔加两次 rx_tsc 之差

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;`EAGAIN` 调用前 TSC 1_000;读取 1:调用前 5_000、返回后 5_100、帧前 1000 B、HW = 10、SW = 11;读取 2:调用前 9_000、返回后 9_050、帧余下 500 B、HW = 20、SW = 21
- 步骤:
  1. `poll_rx`;`next_frame`(得 `None`);`poll_rx`;`next_frame`
- 期望:
  - 第二次 `next_frame` 的 `frame.ts`:`hw_rx_ns = 10`,`sw_rx_ns = 11`,`rx_tsc = 9_050`,`rx_empty_gap_ns = 8_050`(4_100 + 3_950)
- 实现:`crates/hft-net/tests/timestamps.rs::net_124_frame_across_reads`

### NET-125 同一次读取里的多帧共享该次读取的戳与 rx_tsc

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;一次读取 3 个完整帧,返回后 TSC 4_000,HW = 30
- 步骤:
  1. `poll_rx`;`next_frame` 三次
- 期望:
  - 三帧的 `ts.rx_tsc` 都是 4_000,`ts.hw_rx_ns` 都是 30,`ts.rx_empty_gap_ns` 相同
- 实现:`crates/hft-net/tests/timestamps.rs::net_125_frames_share_read_stamps`

### NET-126 所取读取的字节数超过 MSS 时置 MULTI_SKB

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;MSS = 1448;每行一次读取,含一个完整帧;每行新连接
- 实现:`crates/hft-net/tests/timestamps.rs::net_126_multi_skb_mss_boundary`

| 行 | 读取字节数 | 期望 `frame.ts.flags & MULTI_SKB` |
|---|---|---|
| NET-126.1 | 1447 | 0 |
| NET-126.2 | 1448 | 0 |
| NET-126.3 | 1449 | 非 0;`stats().multi_skb == 1` |

### NET-127 读取戳表溢出时合并最老两项、保留较早戳并置 MULTI_SKB

- 规则:[1. 接收路径](README.md#1-接收路径)、[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;`rx_buf = 65536`;一帧 Text 1650 B 分 17 次读取到达,每次 100 B(最后一次 ≤ 100 B),第 k 次的 HW = k、返回后 TSC = 1_000 × k;MSS = 1448
- 步骤:
  1. 17 次 `poll_rx`(期间 `next_frame` 均为 `None`);`next_frame`
- 期望:
  - 解出该帧;`ts.hw_rx_ns == 1`(第 1 次读取的戳被保留);`ts.flags & MULTI_SKB != 0`;`ts.rx_tsc == 17_000`
- 实现:`crates/hft-net/tests/timestamps.rs::net_127_stamp_table_overflow`

### NET-128 EAGAIN 作为确认来源

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;交接后:`EAGAIN`(调用前 1_000);读取(调用前 2_900、返回后 3_000、一个完整帧)
- 步骤:
  1. `poll_rx` 两次;`next_frame`
- 期望:
  - `frame.ts.rx_empty_gap_ns == 2_000`
- 实现:`crates/hft-net/tests/timestamps.rs::net_128_gap_from_eagain`

### NET-129 短读作为确认来源

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;交接后:读取 A(调用前 1_000、返回后 1_100、短读、帧 a);读取 B(调用前 4_900、返回后 5_000、帧 b)
- 步骤:
  1. `poll_rx`;`next_frame`;`poll_rx`;`next_frame`
- 期望:
  - 帧 a 的 `rx_empty_gap_ns == 0`(交接后尚无确认);帧 b 的 `rx_empty_gap_ns == 4_000`
- 实现:`crates/hft-net/tests/timestamps.rs::net_129_gap_from_short_read`

### NET-130 note_empty 作为确认来源

- 规则:[7. 时间戳](README.md#7-时间戳)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;交接后无确认
- 步骤:
  1. `note_empty(2_500)`
  2. 读取(调用前 2_950、返回后 3_000、一帧);`poll_rx`;`next_frame`
- 期望:
  - `frame.ts.rx_empty_gap_ns == 500`
- 实现:`crates/hft-net/tests/timestamps.rs::net_130_gap_from_note_empty`

### NET-131 交接后还没有任何确认时间隔为 0

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;`ConnPool::install` 后第一次读取(返回后 TSC 9_000)读满所给空间
- 步骤:
  1. `poll_rx`;`next_frame`
- 期望:
  - `frame.ts.rx_empty_gap_ns == 0`
- 实现:`crates/hft-net/tests/timestamps.rs::net_131_no_confirmation_zero`

### NET-132 warm 线程上的确认不随交接带到热线程

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;交接前在 warm 线程上 `poll_rx` 得 `EAGAIN`(调用前 100)
- 步骤:
  1. `install`;读取(调用前 5_000、返回后 5_100、一帧,读满所给空间);`poll_rx`;`next_frame`
- 期望:
  - `frame.ts.rx_empty_gap_ns == 0`(不是 5_000)
- 实现:`crates/hft-net/tests/timestamps.rs::net_132_install_resets_confirmation`

### NET-133 早于现有确认的 note_empty 不使起点后退

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;`EAGAIN`(调用前 2_000)
- 步骤:
  1. `note_empty(1_500)`
  2. 读取(返回后 3_000、一帧);`poll_rx`;`next_frame`
- 期望:
  - `frame.ts.rx_empty_gap_ns == 1_000`
- 实现:`crates/hft-net/tests/timestamps.rs::net_133_confirmation_monotonic`

### NET-134 rx_empty_gap_ns 在 u32::MAX 饱和

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;`note_empty(1_000)`;每行新连接,读取返回后 TSC 按行
- 实现:`crates/hft-net/tests/timestamps.rs::net_134_gap_saturation`

| 行 | 读取返回后 TSC(差值) | 期望 `rx_empty_gap_ns` |
|---|---|---|
| NET-134.1 | 4_294_968_294(4_294_967_294) | 4_294_967_294 |
| NET-134.2 | 4_294_968_295(4_294_967_295) | 4_294_967_295 |
| NET-134.3 | 4_294_968_296(4_294_967_296) | 4_294_967_295 |

### NET-135 回环上 rx_empty_gap_ns 不小于对端写入到读出的时长

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`(WS);对端写入前读 TSC 记为 `w`(同一主机同一 TSC 源);三组各 200 次
- 步骤:
  1. 组 A:`poll_rx` 得 `EAGAIN` 后,对端隔 `d ∈ {10, 100, 1000} µs` 写一帧,再 `poll_rx`、`next_frame`
  2. 组 B:先读一帧(短读),之后同组 A
  3. 组 C:`note_empty(tsc_now())` 后,同组 A
- 期望:
  - 每一帧 `rx_empty_gap_ns ≥ delta_ns(w, frame.ts.rx_tsc)`;三组均无 `rx_empty_gap_ns == 0` 的帧
- 实现:`crates/hft-net/tests/loopback.rs::net_135_gap_upper_bound`

### NET-136 任意切分下每帧的戳来源与 MULTI_SKB 判定不变

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:prop
- 里程碑:M1.5
- 性质:每帧的 `hw_rx_ns`/`sw_rx_ns` 等于其第一条 TLS 记录首字节所在读取的戳;`rx_tsc` 等于凑齐该帧的那次读取的返回后 TSC;所取读取字节数 > MSS 或该读取在戳表中被合并过时 `MULTI_SKB` 置位,否则不置;`rx_empty_gap_ns` 等于参考模型的值
- 生成:帧序列同 NET-092;每次读取的 HW/SW 戳与 TSC 单调递增随机;读取大小 1–4 000 B;MSS ∈ {536, 1448, 8948}
- 参考:`tests/support/stamp_model.rs`(按 README §7 逐条实现的读取戳模型)
- 例数:PR 2 000
- 实现:`crates/hft-net/tests/rx_prop.rs::net_136_stamp_source_model`

### NET-150 send_frame 加密一次、sendmsg 一次,返回 sendmsg 返回后的 TSC

- 规则:[2. 发送路径](README.md#2-发送路径)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;tx 为空;帧 `"order"` 已 seal;脚本 `sendmsg` 全部写出,返回后 TSC 7_777
- 步骤:
  1. `send_frame(&f)`
- 期望:
  - 返回 `Ok(SendOk{sent_tsc: 7_777, queued: 0})`;`SysCount.sendmsg == 1` 且该次 iovec 数 = 1;服务端解出 1 帧 `"order"`;`err()` 不含 `TX_PARTIAL`
- 实现:`crates/hft-net/tests/tx_path.rs::net_150_send_once`

### NET-151 真实连接的记录序号只在成功加密发送时前进

- 规则:[2. 发送路径](README.md#2-发送路径)、[3. `warm` 与 scratch 会话(scratch 预热可选,默认关)](README.md#3-warm-与-scratch-会话scratch-预热可选默认关)、[概要](README.md#概要)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`(服务端逐条解密并记录收到的记录序号);`tx_buf = 4096`;`warm_interval_ns = 0`(启用 scratch 预热时的同一性质由 NET-181 覆盖)
- 步骤:
  1. `send_frame(f1)`
  2. `warm(&f2)` 10⁶ 次;`seal` 一个新帧 f4;`send_frame` 一个未 seal 的帧(得 `NotSealed`);服务端发 100 帧,`poll_rx`/`next_frame` 直到取完
  3. `send_frame(f3)`
- 期望:
  - 服务端在本连接上收到的应用数据记录序号依次为 0、1,中间没有空洞;f1、f3 解密成功
  - 步骤 2 前后本连接的发送序号相同(测试钩子读出);未 seal 与 `NotOpen`/`TxBacklog` 不推进序号由 NET-065、NET-156、NET-157 覆盖;REST 请求的同一性质由 NET-276 覆盖
- 实现:`crates/hft-net/tests/tx_path.rs::net_151_tls_seq_advances_only_on_send`

### NET-152 短写把剩余密文留在 tx 并在 finish_poll 续写,不重加密

- 规则:[2. 发送路径](README.md#2-发送路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;帧密文 322 B;脚本第 1 次 `sendmsg` 只写 100 B,之后全部写出
- 步骤:
  1. `send_frame(&f)`
  2. `finish_poll(0)`
- 期望:
  - 步骤 1 返回 `Ok(SendOk{queued: 222})`;`err()` 含 `TX_PARTIAL`;`stats().short_writes == 1`
  - 步骤 2 `sendmsg` 写出剩余 222 B;服务端解出 1 帧;服务端收到的记录数 = 1(无重加密)
- 实现:`crates/hft-net/tests/tx_path.rs::net_152_short_write_resumes`

### NET-153 sendmsg 返回 EAGAIN 时整帧密文留在 tx

- 规则:[2. 发送路径](README.md#2-发送路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;帧密文 322 B;脚本第 1 次 `sendmsg` 返回 `EAGAIN`
- 步骤:
  1. `send_frame(&f)`;`finish_poll(0)`
- 期望:
  - `send_frame` 返回 `Ok(SendOk{queued: 322})`;`err()` 含 `TX_PARTIAL`;`finish_poll` 后服务端解出该帧
- 实现:`crates/hft-net/tests/tx_path.rs::net_153_eagain_queues_all`

### NET-154 有短写剩余时下一次 sendmsg 用两个 iovec

- 规则:[2. 发送路径](README.md#2-发送路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;第一帧短写后剩 222 B
- 步骤:
  1. `send_frame(&f2)`(未经 `finish_poll`)
- 期望:
  - 该次 `sendmsg` 的 iovec 数 = 2,第一个长 222、第二个为 f2 的整条记录;服务端按序解出 f1、f2
- 实现:`crates/hft-net/tests/tx_path.rs::net_154_two_iovecs_after_partial`

### NET-155 sendmsg 的 errno 返回 Io,密文留在 tx,连接致命

- 规则:[2. 发送路径](README.md#2-发送路径)、[调用约束](README.md#调用约束)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;每行新连接;脚本 `sendmsg` 返回 −1 与该 errno
- 实现:`crates/hft-net/tests/tx_path.rs::net_155_send_errno`

| 行 | errno | 期望 |
|---|---|---|
| NET-155.1 | `ECONNRESET`(104) | `Err(SendErr::Io(104))`;`err()` 含 `RESET`;发送序号已前进 1 |
| NET-155.2 | `EPIPE`(32) | `Err(SendErr::Io(32))`;`err()` 含 `RESET` |
| NET-155.3 | `ENOBUFS`(105) | `Err(SendErr::Io(105))`;`err()` 含 `IO`、不含 `RESET`;`last_errno() == 105` |

### NET-156 相位不是 Open 时 send_frame 返回 NotOpen 且不加密

- 规则:[2. 发送路径](README.md#2-发送路径)、[调用约束](README.md#调用约束)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;已 seal 的帧
- 实现:`crates/hft-net/tests/tx_path.rs::net_156_not_open`

| 行 | 相位 | 期望 |
|---|---|---|
| NET-156.1 | `Closing` | `Err(NotOpen(Closing))`;`SysCount.sendmsg == 0`;发送序号不变 |
| NET-156.2 | `Dead` | `Err(NotOpen(Dead))`;同上 |
| NET-156.3 | `WsUpgrade`(测试构造) | `Err(NotOpen(WsUpgrade))`;同上 |

### NET-157 tx 余量边界:剩余 + len + 22 = tx_buf 可发,超 1 B 返回 TxBacklog 并置 TX_BACKLOG

- 规则:[2. 发送路径](README.md#2-发送路径)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;`tx_buf = 4096`;tx 已有 1000 B 未写出的密文(脚本 `sendmsg` 返回 `EAGAIN`);每行新连接
- 实现:`crates/hft-net/tests/tx_path.rs::net_157_tx_backlog_boundary`

| 行 | 帧 `as_bytes().len()` | 期望 |
|---|---|---|
| NET-157.1 | 3073(1000 + 3073 + 22 = 4095) | `Ok`;`queued == 4095` |
| NET-157.2 | 3074(= 4096) | `Ok`;`queued == 4096` |
| NET-157.3 | 3075(= 4097) | `Err(TxBacklog)`;`err()` 含 `TX_BACKLOG`;发送序号不变;tx 仍为 1000 B |

### NET-158 加密失败返回 Encrypt 并置 TLS_PROTO,不发送

- 规则:[2. 发送路径](README.md#2-发送路径)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;`test-io` 注入下一次加密失败
- 步骤:
  1. `send_frame(&f)`
- 期望:
  - `Err(SendErr::Encrypt)`;`err()` 含 `TLS_PROTO`;`SysCount.sendmsg == 0`;tx 字节数不变
- 实现:`crates/hft-net/tests/tx_path.rs::net_158_encrypt_failure`

### NET-159 flush 续写剩余密文并返回剩余字节

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;tx 有 222 B 剩余
- 实现:`crates/hft-net/tests/tx_path.rs::net_159_flush`

| 行 | 脚本 `sendmsg` | 期望 |
|---|---|---|
| NET-159.1 | 写出 222 B | `Ok(0)`;服务端收齐该记录 |
| NET-159.2 | 写出 100 B | `Ok(122)` |
| NET-159.3 | `EAGAIN` | `Ok(222)` |
| NET-159.4 | `ECONNRESET` | `Err(Io(104))`;`err()` 含 `RESET` |
| NET-159.5 | tx 为空时调用 | `Ok(0)`;`SysCount.sendmsg == 0` |

### NET-160 send_text 发出带掩码的 Text 帧

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. `send_text(b"{\"op\":\"subscribe\"}", 0)`
- 期望:
  - 返回 `Ok`;服务端收到 1 帧,opcode Text,MASK 位为 1,解掩码后 payload 与输入相同
- 实现:`crates/hft-net/tests/tx_path.rs::net_160_send_text`

### NET-161 send_ping 发出载荷为 now_tsc 小端 8 字节的 Ping

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`
- 步骤:
  1. `send_ping(0x0102_0304_0506_0708)`
- 期望:
  - 服务端收到 Ping,解掩码后载荷 `08 07 06 05 04 03 02 01`
- 实现:`crates/hft-net/tests/tx_path.rs::net_161_send_ping_payload`

### NET-162 半帧积压期间发送不影响之后的解帧

- 规则:[2. 发送路径](README.md#2-发送路径)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer`;接收缓冲里有帧 X 的前一半
- 步骤:
  1. `send_frame(o1)`;`send_frame(o2)`
  2. 帧 X 余下部分到达;`poll_rx`;`next_frame`
- 期望:
  - 步骤 2 解出完整 X,与发送逐字节相同;服务端解出 o1、o2
- 实现:`crates/hft-net/tests/tx_path.rs::net_162_send_does_not_touch_rx`

### NET-163 极小发送缓冲下全部已加密帧按序完整送达

- 规则:[2. 发送路径](README.md#2-发送路径)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`(WS);客户端 `so_sndbuf = 4608`;对端每 1 ms 读一次
- 步骤:
  1. 连续 `send_frame` 1 000 帧(每帧 300–2 000 B,内容含序号);每次之后 `finish_poll`;遇 `TxBacklog` 时只 `finish_poll` 直到可发
- 期望:
  - 对端按序解出 1 000 帧,无重复、无缺失;所有 `Ok` 返回的帧都送达;`stats().short_writes ≥ 1`
- 实现:`crates/hft-net/tests/loopback.rs::net_163_tiny_sndbuf_all_delivered`

### NET-164 TLS 加密调用只出现在 src/tls.rs 的三个入口

- 规则:[3. `warm` 与 scratch 会话(scratch 预热可选,默认关)](README.md#3-warm-与-scratch-会话scratch-预热可选默认关)、[11. 源文件规划](README.md#11-源文件规划)
- 级别:unit
- 里程碑:M1.5
- 前置:扫描规则与 `cargo xtask ci` 的 grep 规则同源(`tools/xtask` 导出的函数);两份输入树:现行 `crates/hft-net/src`,以及在 `src/conn.rs` 里多一行 `WriteTraffic::encrypt` 调用的副本
- 步骤:
  1. 对现行树运行扫描
  2. 对副本运行扫描
- 期望:
  - 步骤 1 无违例
  - 步骤 2 报告 1 处违例,位置为 `src/conn.rs` 的那一行
- 实现:`crates/hft-net/tests/whitelist.rs::net_164_encrypt_call_whitelist`

### NET-180 warm 默认关闭时是空操作

- 规则:[3. `warm` 与 scratch 会话(scratch 预热可选,默认关)](README.md#3-warm-与-scratch-会话scratch-预热可选默认关)、[调用约束](README.md#调用约束)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;`warm_interval_ns = 0`;计数分配器
- 步骤:
  1. `warm(&f)` 1 000 次
- 期望:
  - 系统调用总数 0;分配次数 0;发送序号不变;tx 字节数不变;没有 scratch 会话被创建(scratch 会话计数 0)
- 实现:`crates/hft-net/tests/warm.rs::net_180_warm_disabled_noop`

### NET-181 warm 启用时 10⁶ 次预热后真实连接的下一帧被对端解密

- 规则:[3. `warm` 与 scratch 会话(scratch 预热可选,默认关)](README.md#3-warm-与-scratch-会话scratch-预热可选默认关)
- 级别:unit
- 里程碑:M7
- 状态:可选扩展:TLS 预热
- 前置:`MemPeer` + `ScriptIo`;计数全局分配器;`warm_interval_ns = 50_000`;scratch 会话已建
- 步骤:
  1. `warm(&f)` 10⁶ 次
  2. `send_frame(&g)`
- 期望:
  - 步骤 1 期间系统调用总数 0;计数分配器的分配次数 0(scratch 会话建好之后)
  - 服务端以序号 0 解密 g 成功;步骤 1 前后真实连接的发送序号相同
- 实现:`crates/hft-net/tests/warm.rs::net_181_warm_then_real_frame`

### NET-182 warm 不改真实连接的读写状态

- 规则:[3. `warm` 与 scratch 会话(scratch 预热可选,默认关)](README.md#3-warm-与-scratch-会话scratch-预热可选默认关)
- 级别:unit
- 里程碑:M7
- 状态:可选扩展:TLS 预热
- 前置:`MemPeer` + `ScriptIo`;`warm_interval_ns = 50_000`;接收缓冲有 2 个未取的帧,tx 有 100 B 剩余,`err()` 含 `TX_PARTIAL`
- 步骤:
  1. `warm(&f)` 1 000 次
  2. `next_frame` 两次;`finish_poll`
- 期望:
  - 步骤 1 前后 tx 字节数、`err()`、`last_rx_tsc()` 相同
  - 步骤 2 得到原来的 2 帧;服务端收到原来的 100 B 剩余且解密成功
- 实现:`crates/hft-net/tests/warm.rs::net_182_warm_keeps_conn_state`

### NET-190 WS 连接按 TcpConnecting → TlsHandshake → WsUpgrade → Open 建立并给出 Ready

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`(WS,TLS 1.3,只开 `TLS13_AES_128_GCM_SHA256`,密钥交换 X25519);`connect/tls/upgrade_timeout_ms = 3000`;`BuildReq{role: Md, deadline_ns: 开始 + 10 s}`
- 步骤:
  1. `submit`;反复 `drive(now)` 并在每次之间记录 `phase(job)`
- 期望:
  - 观察到的相位序列依次含 `TcpConnecting`、`TlsHandshake`、`WsUpgrade`、`Open`
  - 恰好一个事件 `Ready{tls: TlsInfo{version: 0x0304, suite: 0x1301, group: 0x001D}, napi_id: 0}`;`handoff(job)` 返回 `Ok`
- 实现:`crates/hft-net/tests/connector.rs::net_190_ws_happy_path`

### NET-191 Rest 角色握手后直接 Open,不发 Upgrade

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`(HTTP);`BuildReq{role: Rest}`
- 步骤:
  1. `submit`;`drive` 至事件
- 期望:
  - 事件 `Ready`;相位序列不含 `WsUpgrade`;服务端在 Ready 之前收到的应用数据字节数 = 0
- 实现:`crates/hft-net/tests/connector.rs::net_191_rest_no_upgrade`

### NET-192 Upgrade 请求的请求行与头

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`(WS,记录收到的 Upgrade 请求原文);`endpoints[0] = {host: "a.test", path: "/ws?x=1", port: 回环端口}`;上层额外头 `X-MBX-APIKEY: k1`
- 步骤:
  1. `submit`;`drive` 至 `Ready`
- 期望:
  - 请求行 `GET /ws?x=1 HTTP/1.1`;头含 `Host: a.test`、`Upgrade: websocket`、`Connection: Upgrade`、`Sec-WebSocket-Version: 13`、`X-MBX-APIKEY: k1`
  - `Sec-WebSocket-Key` 为 24 字符 base64,解码后 16 B
  - 不含 `Sec-WebSocket-Extensions`
- 实现:`crates/hft-net/tests/connector.rs::net_192_upgrade_request`

### NET-193 Sec-WebSocket-Accept 按 RFC 校验

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:table
- 里程碑:M1.5
- 前置:`LoopPeer`(WS);测试钩子把 `Sec-WebSocket-Key` 固定为 `dGhlIHNhbXBsZSBub25jZQ==`;服务端按行返回 Accept
- 实现:`crates/hft-net/tests/connector.rs::net_193_accept_check`

| 行 | 服务端 `Sec-WebSocket-Accept` | 期望 |
|---|---|---|
| NET-193.1 | `s3pPLMBiTxaQ9kYGzzhZRbK+xOo=` | `Ready` |
| NET-193.2 | `s3pPLMBiTxaQ9kYGzzhZRbK+xOp=` | `Failed{stage: WsUpgrade, why: BadAccept}` |
| NET-193.3 | 缺该头 | `Failed{stage: WsUpgrade, why: BadAccept}` |

### NET-194 connect 被拒给出 Failed{TcpConnecting, Errno(111)}

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:integ
- 里程碑:M1.5
- 前置:回环上一个没有监听的端口
- 步骤:
  1. `submit`;`drive` 至事件
- 期望:
  - `Failed{stage: TcpConnecting, why: Errno(111)}`
- 实现:`crates/hft-net/tests/connector.rs::net_194_connect_refused`

### NET-195 握手期收到 TLS alert 给出 TlsAlert(描述码)

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer` 收到 ClientHello 后回 alert `handshake_failure`(40)
- 步骤:
  1. `submit`;`drive` 至事件
- 期望:
  - `Failed{stage: TlsHandshake, why: TlsAlert(40)}`
- 实现:`crates/hft-net/tests/connector.rs::net_195_tls_alert`

### NET-196 证书不受信给出 Failed{TlsHandshake, Tls}

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer` 用自签证书;配置无 `extra_ca`
- 步骤:
  1. `submit`;`drive` 至事件
- 期望:
  - `Failed{stage: TlsHandshake, why: Tls}`
- 实现:`crates/hft-net/tests/connector.rs::net_196_untrusted_cert`

### NET-197 Upgrade 响应非 101 给出 HttpStatus(状态码)

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:table
- 里程碑:M1.5
- 前置:`LoopPeer`(WS)按行回状态
- 实现:`crates/hft-net/tests/connector.rs::net_197_upgrade_non_101`

| 行 | 状态 | 期望 |
|---|---|---|
| NET-197.1 | 403 | `Failed{stage: WsUpgrade, why: HttpStatus(403)}` |
| NET-197.2 | 200 | `Failed{stage: WsUpgrade, why: HttpStatus(200)}` |
| NET-197.3 | 101 | `Ready` |

### NET-198 Upgrade 响应带 Sec-WebSocket-Extensions 时失败

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer` 回 101 且带 `Sec-WebSocket-Extensions: permessage-deflate`
- 步骤:
  1. `submit`;`drive` 至事件
- 期望:
  - `Failed{stage: WsUpgrade, why: Extensions}`
- 实现:`crates/hft-net/tests/connector.rs::net_198_extensions_rejected`

### NET-199 三个阶段的超时在恰好到达超时时触发

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`ScriptIo`(connect 一直 `EINPROGRESS` 且不可写;或握手/Upgrade 期对端无响应);`connect_timeout_ms = tls_timeout_ms = upgrade_timeout_ms = 3000`;`deadline_ns` = 开始 + 60 s;`drive(now_ns)` 中 now = 该阶段开始 + 偏移
- 实现:`crates/hft-net/tests/connector.rs::net_199_stage_timeouts`

| 行 | 阶段 | 偏移 | 期望 |
|---|---|---|---|
| NET-199.1 | TcpConnecting | 2_999_999_999 ns | 无事件 |
| NET-199.2 | TcpConnecting | 3_000_000_000 ns | `Failed{stage: TcpConnecting, why: Timeout}` |
| NET-199.3 | TlsHandshake | 2_999_999_999 ns | 无事件 |
| NET-199.4 | TlsHandshake | 3_000_000_000 ns | `Failed{stage: TlsHandshake, why: Timeout}` |
| NET-199.5 | WsUpgrade | 2_999_999_999 ns | 无事件 |
| NET-199.6 | WsUpgrade | 3_000_000_000 ns | `Failed{stage: WsUpgrade, why: Timeout}` |

### NET-200 deadline_ns 早于阶段超时时按截止失败

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo`(connect 不可写);`connect_timeout_ms = 3000`;`deadline_ns = 开始 + 1_000_000_000`
- 步骤:
  1. `drive(开始 + 999_999_999)`;`drive(开始 + 1_000_000_000)`
- 期望:
  - 第一次无事件;第二次 `Failed{stage: TcpConnecting, why: Timeout}`
- 实现:`crates/hft-net/tests/connector.rs::net_200_deadline_first`

### NET-201 失败或退役的连接 reclaim 后对端收到 FIN,槽可复用

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)、[签名草案](README.md#签名草案)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`(WS);arena 只有 1 个槽
- 步骤:
  1. 建连得 `Ready`;`handoff`;装进 `ConnPool`;`retire(id, PEER_EOF)` 得 `RetireMsg`;`Connector::reclaim(msg)`
  2. 再 `submit` 一个作业
- 期望:
  - 步骤 1 后对端在 100 ms 内读到 EOF
  - 步骤 2 `submit` 返回 `Ok`(槽已归还),不是 `ArenaFull`
- 实现:`crates/hft-net/tests/connector.rs::net_201_reclaim_closes_and_frees`

### NET-202 submit 的拒绝原因

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:`ScriptIo`;`endpoints` 有 2 项;`src_ips = [10.0.0.5]`;`bind_device = {10.0.0.5 → "eth1"}`;作业表 16 项;arena 有 32 个空槽(行 2 除外)
- 实现:`crates/hft-net/tests/connector.rs::net_202_submit_errors`

| 行 | 条件 | 期望 |
|---|---|---|
| NET-202.1 | 已有 16 个未完成作业,再 submit | `Err(JobsFull)` |
| NET-202.2 | arena 无空槽 | `Err(ArenaFull)` |
| NET-202.3 | `endpoint = 2` | `Err(BadEndpoint)` |
| NET-202.4 | `src_ip_idx = 1` | `Err(BadSrcIp)` |
| NET-202.5 | `src_ips = [10.0.0.6]`,`bind_device` 只有 10.0.0.5 | `Err(BadSrcIp)` |
| NET-202.6 | 第 16 个作业(15 个在途) | `Ok` |

### NET-203 作业未到 Open 时 conn_mut 为 None、handoff 为 NotReady

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo`;作业处于 `TlsHandshake`
- 步骤:
  1. `conn_mut(job)`;`handoff(job)`
- 期望:
  - `conn_mut(job).is_none()`;`handoff(job) == Err(BuildErr::NotReady)`
- 实现:`crates/hft-net/tests/connector.rs::net_203_not_ready`

### NET-204 Ready 在处理过票据或 Open 后 20 ms 的第一次 drive 给出

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:table
- 里程碑:M1.5
- 前置:`ScriptIo` + `MemPeer` 服务端;连接在 `t0` 到达 Open;`drive(now)`
- 实现:`crates/hft-net/tests/connector.rs::net_204_ticket_wait`

| 行 | 服务端行为 | `drive` 时刻 | 期望 |
|---|---|---|---|
| NET-204.1 | Open 后立即发 1 张 NewSessionTicket | `t0 + 1 ms` | `Ready` |
| NET-204.2 | 不发票据 | `t0 + 19_999_999 ns` | 无事件 |
| NET-204.3 | 不发票据 | `t0 + 20_000_000 ns` | `Ready` |

### NET-205 每个 socket 的选项按清单设置

- 规则:[5. socket 选项清单](README.md#5-socket-选项清单)、[概要](README.md#概要)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`LoopPeer`;`tcp_user_timeout_ms = 5000`、`so_rcvbuf = 131072`、`so_sndbuf = 65536`(均低于 CI 主机的 `rmem_max`/`wmem_max`)、`tx_timestamps = false`、`bind_device = {127.0.0.1 → "lo"}`;分别建 Md、Order、Rest 三个角色的连接,对每个 fd 做 `getsockopt`
- 实现:`crates/hft-net/tests/connector.rs::net_205_socket_options`

| 行 | 选项 | 期望(三个角色相同) |
|---|---|---|
| NET-205.1 | `O_NONBLOCK`(fcntl) | 置位 |
| NET-205.2 | `TCP_NODELAY` | 1 |
| NET-205.3 | `TCP_USER_TIMEOUT` | 5000 |
| NET-205.4 | `SO_BUSY_POLL` | 0,且 `test-io` 记录到一次显式 `setsockopt(SO_BUSY_POLL, 0)` |
| NET-205.5 | `SO_PREFER_BUSY_POLL`、`SO_BUSY_POLL_BUDGET` | 0;没有对它们的 `setsockopt` |
| NET-205.6 | `SO_TIMESTAMPING` | `RX_HARDWARE、RX_SOFTWARE、RAW_HARDWARE、SOFTWARE`,不含 `TX_*`、`OPT_ID` |
| NET-205.7 | `SO_REUSEADDR` | 1 |
| NET-205.8 | `SO_RCVBUF` / `SO_SNDBUF` | 262144 / 131072(内核报告设定值的两倍) |
| NET-205.9 | `SO_BINDTODEVICE` | `"lo"` |
| NET-205.10 | `TCP_QUICKACK` | 没有对它的 `setsockopt` |

### NET-206 握手完成后各读一次 SO_INCOMING_NAPI_ID 与 TCP_MAXSEG;回环上 napi_id 为 0

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`(WS);`SysCount` 记录 `getsockopt` 的选项与调用时的相位
- 步骤:
  1. `submit`;`drive` 至 `Ready`;`napi_ifindex(0)`
- 期望:
  - `getsockopt(SO_INCOMING_NAPI_ID)` 与 `getsockopt(TCP_MAXSEG)` 各恰好 1 次,都发生在 TLS 握手完成之后
  - `Ready.napi_id == 0`;`conn.napi_id() == 0`;`napi_ifindex(0) == None` 且该调用不产生系统调用
- 实现:`crates/hft-net/tests/connector.rs::net_206_napi_and_mss_read_once`

### NET-207 源地址取 src_ips[src_ip_idx],端口由内核分配

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)、[配置项](README.md#配置项)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`;`src_ips = [127.0.0.2, 127.0.0.3]`;`bind_device` 两者都映射 `"lo"`;`rss_plan` 空
- 步骤:
  1. `submit(BuildReq{src_ip_idx: 1})`;`drive` 至 `Ready`;`getsockname`
- 期望:
  - 本地地址 127.0.0.3;`bind` 调用的端口参数为 0
- 实现:`crates/hft-net/tests/connector.rs::net_207_source_address`

### NET-208 Failed 之后作业不再产出事件

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo`;作业已给出 `Failed{TcpConnecting, Errno(111)}`
- 步骤:
  1. 再 `drive` 10 次
- 期望:
  - 10 次均返回 `None`;`conn_mut(job).is_none()`;`handoff(job) == Err(NotReady)`
- 实现:`crates/hft-net/tests/connector.rs::net_208_no_events_after_failed`

- 重连回归:`crates/hft-net/tests/connector.rs::net_208_reconnect_backoff_is_bounded_and_isolated`;同 endpoint/src_ip 指数退避至配置上限,另一端点不受阻。

### NET-209 extra_ca 只在配置含 endpoint_override 时加载

- 规则:[配置项](README.md#配置项)、[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:table
- 里程碑:M1.5
- 前置:`LoopPeer` 用测试 CA 签发的证书;`extra_ca` = 该 CA
- 实现:`crates/hft-net/tests/connector.rs::net_209_extra_ca_only_with_override`

| 行 | 配置含 `endpoint_override` | 期望 |
|---|---|---|
| NET-209.1 | 是 | `Ready` |
| NET-209.2 | 否 | `Failed{stage: TlsHandshake, why: Tls}` |

### NET-210 ClientHello 的套件顺序与密钥交换,TLS 1.2 端点也能建连

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:table
- 里程碑:M1.5
- 前置:`LoopPeer` 记录 ClientHello;按行限制服务端
- 实现:`crates/hft-net/tests/connector.rs::net_210_client_hello`

| 行 | 服务端 | 期望 |
|---|---|---|
| NET-210.1 | 不限制 | ClientHello 的 TLS 1.3 套件依次 `0x1301`、`0x1302`、`0x1303`;supported_groups 含 `0x11EC` |
| NET-210.2 | 只允许 TLS 1.2 | `Ready{tls.version: 0x0303}` |
| NET-210.3 | 只允许 TLS 1.3 + `0x1302` | `Ready{tls.suite: 0x1302}` |

### NET-211 每次建连的 Sec-WebSocket-Key 不同

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`(WS)
- 步骤:
  1. 建连 100 次,收集 `Sec-WebSocket-Key`
- 期望:
  - 100 个值两两不同
- 实现:`crates/hft-net/tests/connector.rs::net_211_ws_key_random`

### NET-212 热线程入口从不调用 connect、shutdown、close、getaddrinfo

- 规则:[调用约束](README.md#调用约束)
- 级别:unit
- 里程碑:M1.5
- 前置:`MemPeer` + `ScriptIo`;经 `Connector` 建好、`ConnPool::install` 交接的 WS 连接;脚本含数据、`EAGAIN`、短写、EOF、对端 Close
- 步骤:
  1. 对上述连接执行 10⁴ 轮 `poll_rx`/`next_frame`/`send_frame`/`send_text`/`send_ping`/`finish_poll`/`flush`,最后 `ConnPool::retire`
- 期望:
  - 热线程阶段 `SysCount` 中 `connect`、`shutdown`、`close`、`getaddrinfo`、`epoll_ctl` 的增量都为 0;REST 池的同一性质由 NET-277 覆盖
- 实现:`crates/hft-net/tests/hot_syscalls.rs::net_212_hot_never_connects`

### NET-213 端点钉选 IP 时不做 DNS 解析

- 规则:[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`ScriptIo`;解析器调用计数
- 实现:`crates/hft-net/tests/connector.rs::net_213_pinned_ips_no_dns`

| 行 | `endpoints[0].ips` | 期望 |
|---|---|---|
| NET-213.1 | `[127.0.0.1]` | 解析器调用 0 次;connect 目标 127.0.0.1 |
| NET-213.2 | 空,`host = "localhost"` | 解析器在 warm 线程调用 1 次;connect 目标为解析结果 |

### NET-220 ReadyPoll 恰好返回有数据的连接

- 规则:[签名草案](README.md#签名草案)、[调用约束](README.md#调用约束)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`;12 条已交接连接(ConnId 1–12)经 `ReadyHandle::add` 注册;对端只向 ConnId 3、9 写数据
- 步骤:
  1. `probe(&mut out)`
- 期望:
  - 返回 2;`out` 恰为 {3, 9};`SysCount.epoll_pwait == 1`,其 `timeout == 0`;`SysCount.epoll_ctl == 0`(probe 期间)
- 实现:`crates/hft-net/tests/ready.rs::net_220_probe_ready_set`

### NET-221 del 之后的连接不再出现在就绪集合

- 规则:[签名草案](README.md#签名草案)
- 级别:integ
- 里程碑:M1.5
- 前置:同 NET-220
- 步骤:
  1. `ReadyHandle::del(conn 3)`;`probe`
- 期望:
  - 返回 1;`out` 恰为 {9}
- 实现:`crates/hft-net/tests/ready.rs::net_221_del_removes`

### NET-222 读空的连接不再就绪(水平触发)

- 规则:[签名草案](README.md#签名草案)
- 级别:integ
- 里程碑:M1.5
- 前置:同 NET-220
- 步骤:
  1. `probe`;只对 3 `poll_rx` 直到 `EAGAIN`;再 `probe`
  2. 对 9 `poll_rx` 直到 `EAGAIN`;再 `probe`
- 期望:
  - 步骤 1 第二次 `probe` 返回 1,`out` 恰为 {9}
  - 步骤 2 `probe` 返回 0
- 实现:`crates/hft-net/tests/ready.rs::net_222_level_triggered`

### NET-223 probe 先清空 out

- 规则:[签名草案](README.md#签名草案)
- 级别:integ
- 里程碑:M1.5
- 前置:同 NET-220;`out` 预置 {1, 200}
- 步骤:
  1. `probe(&mut out)`
- 期望:
  - `out` 恰为 {3, 9}
- 实现:`crates/hft-net/tests/ready.rs::net_223_probe_clears_out`

### NET-224 就绪超过 64 条时其余留到下一次

- 规则:[签名草案](README.md#签名草案)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`;70 条注册连接都有数据
- 步骤:
  1. `probe` 两次(不读任何连接)
- 期望:
  - 第一次返回 64;两次 `out` 的并集 = 全部 70 个 ConnId
- 实现:`crates/hft-net/tests/ready.rs::net_224_more_than_64_ready`

### NET-225 重复注册与注销未注册连接返回 Os(errno)

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M1.5
- 前置:`ReadyPoll::new()`;连接 c
- 实现:`crates/hft-net/tests/ready.rs::net_225_ctl_errors`

| 行 | 操作 | 期望 |
|---|---|---|
| NET-225.1 | `add(c)` 两次 | 第一次 `Ok`;第二次 `Err(NetErr::Os(17))`(EEXIST) |
| NET-225.2 | 未注册时 `del(c)` | `Err(NetErr::Os(2))`(ENOENT) |
| NET-225.3 | `add(c)` 后 `del(c)` | 都 `Ok` |

### NET-226 epoll_pwait 出错时 probe 返回 0 且 out 为空

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M1.5
- 前置:`ScriptIo` 让 `epoll_pwait` 返回 −1/`EBADF`;`out` 预置 {5}
- 步骤:
  1. `probe(&mut out)`
- 期望:
  - 返回 0;`out` 为空;无 panic
- 实现:`crates/hft-net/tests/ready.rs::net_226_probe_error`

### NET-227 ReadyPoll 的 epfd 带 CLOEXEC

- 规则:[签名草案](README.md#签名草案)
- 级别:integ
- 里程碑:M1.5
- 前置:无参数
- 步骤:
  1. `ReadyPoll::new()`;对其 epfd `fcntl(F_GETFD)`
- 期望:
  - `FD_CLOEXEC` 置位
- 实现:`crates/hft-net/tests/ready.rs::net_227_epfd_cloexec`

### NET-240 空池先给 Fill,在建期间不再给请求

- 规则:[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M3
- 前置:`ConnPool::<3>::new(PrimaryStandby, 2, RotatePolicy{stagger_tsc: 3_600_000_000_000})`;测试 arena 造的 Open 连接 Handoff
- 步骤:
  1. `next_build(0)`;再 `next_build(0)`
  2. `install(h1, None)`;`next_build(0)`
- 期望:
  - 步骤 1 第一次 `Some(Fill)`,第二次 `None`
  - 步骤 2 `install` 返回 `Ok(h1 的 ConnId)`;`next_build` 返回 `Some(Fill)`(活跃 1 < 2)
- 实现:`crates/hft-net/tests/pool.rs::net_240_fill_then_pending`

### NET-241 build_failed 清除在建状态

- 规则:[签名草案](README.md#签名草案)、[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)
- 级别:unit
- 里程碑:M3
- 前置:同 NET-240;`next_build` 已给出 `Fill`
- 步骤:
  1. `next_build(0)`;`build_failed()`;`next_build(0)`
- 期望:
  - 依次 `None`、`Some(Fill)`
- 实现:`crates/hft-net/tests/pool.rs::net_241_build_failed_clears`

### NET-242 池满时不给请求,再 install 返回 Full

- 规则:[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M3
- 前置:`ConnPool::<3>::new(PrimaryStandby, 2, …)`;已 install 3 条
- 步骤:
  1. `next_build(0)`;`install(h4, None)`
- 期望:
  - `next_build` 返回 `None`;`install` 返回 `Err(PoolErr::Full)`
- 实现:`crates/hft-net/tests/pool.rs::net_242_full_pool`

### NET-243 install 非 Open 的连接返回 NotOpen

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M3
- 前置:Handoff 内的连接相位为 `Dead`
- 步骤:
  1. `install(h, None)`
- 期望:
  - `Err(PoolErr::NotOpen)`;池内活跃数仍为 0
- 实现:`crates/hft-net/tests/pool.rs::net_243_install_not_open`

### NET-244 第一条成为 primary,第二条成为 standby,active 先给 primary

- 规则:[签名草案](README.md#签名草案)、[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)
- 级别:unit
- 里程碑:M3
- 前置:`PrimaryStandby`,target 2;连接 c1(ConnId 7)、c2(ConnId 3)
- 步骤:
  1. `install(c1)`;`install(c2)`;遍历 `active()`
- 期望:
  - `primary().id() == 7`;`standby().id() == 3`;`active()` 依次 7、3
- 实现:`crates/hft-net/tests/pool.rs::net_244_primary_then_standby`

### NET-245 swap_primary 只交换索引,无系统调用

- 规则:[签名草案](README.md#签名草案)、[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)、[调用约束](README.md#调用约束)
- 级别:unit
- 里程碑:M3
- 前置:同 NET-244;`SysCount` 清零
- 步骤:
  1. `swap_primary()`
- 期望:
  - 返回 `Ok((3, 7))`;`primary().id() == 3`;`standby().id() == 7`;`SysCount` 全为 0;两条连接的 tx 与读位置不变
- 实现:`crates/hft-net/tests/pool.rs::net_245_swap_indices_only`

### NET-246 swap_primary 的拒绝:无 Open 的 standby、Race 模式

- 规则:[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M3
- 前置:按行构造池
- 实现:`crates/hft-net/tests/pool.rs::net_246_swap_errors`

| 行 | 池 | 期望 `swap_primary()` |
|---|---|---|
| NET-246.1 | PrimaryStandby,只有 primary | `Err(NoStandby)` |
| NET-246.2 | PrimaryStandby,standby 相位 `Dead` | `Err(NoStandby)` |
| NET-246.3 | Race,两条活跃 | `Err(WrongMode)` |

### NET-247 退役 primary 时 standby 升为 primary

- 规则:[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)
- 级别:unit
- 里程碑:M3
- 前置:同 NET-244
- 步骤:
  1. `retire(7, RESET)`;`next_build(0)`
- 期望:
  - `retire` 返回 `Ok(RetireMsg)`,其中 `why` 含 `RESET`;`primary().id() == 3`;`standby()` 为 `None`;`next_build` 返回 `Some(Fill)`
- 实现:`crates/hft-net/tests/pool.rs::net_247_retire_primary_promotes`

### NET-248 退役不在池中的连接返回 UnknownConn

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M3
- 前置:同 NET-244
- 步骤:
  1. `retire(42, RESET)`
- 期望:
  - `Err(PoolErr::UnknownConn)`;池内容不变
- 实现:`crates/hft-net/tests/pool.rs::net_248_retire_unknown`

### NET-249 致命退役最后一条活跃连接后立即给 Fill

- 规则:[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)
- 级别:unit
- 里程碑:M3
- 前置:`Race`,target 2;只有 1 条活跃连接 c1;无在建
- 步骤:
  1. `retire(c1, PEER_EOF)`;`next_build(0)`
- 期望:
  - 活跃数 0;`next_build` 返回 `Some(Fill)`
- 实现:`crates/hft-net/tests/pool.rs::net_249_last_retired_refills`

### NET-250 计划轮换时刻相差小于 rotate_stagger_s 时较晚的一条提前

- 规则:[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M9
- 前置:测试时钟;`rotate_stagger_s = 3600`(`stagger_tsc = 3_600_000_000_000`);池里已有 A,`rotate_at = 10_000 s`;每行再 `install(B, 行内时刻)`;时刻以秒写,换算为 ns
- 实现:`crates/hft-net/tests/pool.rs::net_250_rotate_stagger`

| 行 | B 的 `rotate_at` | 期望 A、B 的计划时刻 |
|---|---|---|
| NET-250.1 | 10_500 s | A 10_000 s;B 6_400 s |
| NET-250.2 | 13_600 s(差 = 3600 s) | 不变 |
| NET-250.3 | 13_600 s − 1 ns | A 10_000 s;B 6_400 s |
| NET-250.4 | 13_600 s + 1 ns | 不变 |
| NET-250.5 | 9_000 s | A 5_400 s;B 9_000 s |
| NET-250.6 | `None` | A 10_000 s;B 无计划 |

### NET-251 rotate_due 到期返回该连接并置 ROTATE_DUE,next_build 给 Rotate

- 规则:[签名草案](README.md#签名草案)、[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)
- 级别:table
- 里程碑:M3
- 前置:PrimaryStandby,target 2,两条活跃;c1(ConnId 7)`rotate_at = 1_000_000`,c2 无计划;无在建
- 实现:`crates/hft-net/tests/pool.rs::net_251_rotation_boundary`

| 行 | `now` | 期望 |
|---|---|---|
| NET-251.1 | 999_999 | `rotate_due` = `None`;c1 `err()` 不含 `ROTATE_DUE` |
| NET-251.2 | 1_000_000 | `rotate_due` = `Some(7)`;c1 `err()` 含 `ROTATE_DUE`;随后 `next_build` = `Some(Rotate(7))` |
| NET-251.3 | 1_000_001 | 同上一行 |

### NET-252 next_build 的优先顺序 Hint > Rotate > Fill

- 规则:[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)
- 级别:table
- 里程碑:M9
- 前置:PrimaryStandby,target 2;每行新池、无在建
- 实现:`crates/hft-net/tests/pool.rs::net_252_build_priority`

| 行 | 池状态 | 期望 `next_build` |
|---|---|---|
| NET-252.1 | 1 条活跃(7),带 `SERVER_HINT`,另 7 已到轮换时刻 | `Some(Hint(7))` |
| NET-252.2 | 1 条活跃(7),已到轮换时刻 | `Some(Rotate(7))` |
| NET-252.3 | 1 条活跃(7),无提示、未到期 | `Some(Fill)` |
| NET-252.4 | 2 条活跃,都无提示、未到期 | `None` |
| NET-252.5 | 2 条活跃,3 带 `SERVER_HINT` | `Some(Hint(3))` |

### NET-253 rotate_at 为 None 的连接永不到期

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M9
- 前置:一条活跃连接,`install(h, None)`
- 步骤:
  1. `rotate_due(u64::MAX)`
- 期望:
  - `None`;`err()` 不含 `ROTATE_DUE`
- 实现:`crates/hft-net/tests/pool.rs::net_253_no_plan_never_due`

### NET-254 Race 池两条都活跃,退役 primary 后另一条成为 primary

- 规则:[8. `ConnPool` 与轮换](README.md#8-connpool-与轮换)
- 级别:unit
- 里程碑:M3
- 前置:`Race`,target 2;install c1(1)、c2(2)
- 步骤:
  1. 遍历 `active()`;`retire(1, WS_CLOSE)`;遍历 `active()`
- 期望:
  - 第一次依次 1、2;第二次只有 2,`primary().id() == 2`
- 实现:`crates/hft-net/tests/pool.rs::net_254_race_both_active`

### NET-260 Content-Length 响应完整交出,请求带固定头

- 规则:[9. REST 客户端](README.md#9-rest-客户端)
- 级别:integ
- 里程碑:M3
- 前置:`LoopPeer`(HTTP,记录请求原文);`RestPool::<2>` 两条 Open 连接;`rest_timeout_ms = 2000`
- 步骤:
  1. `submit(RestReq{method: GET, path_query: b"/q?a=1", headers: [(b"X-K", b"v")], body: b"", tag: 11})`
  2. 服务端回 `200`,`Content-Length: 5`,正文 `hello`;循环 `poll` 至 `Some`
- 期望:
  - 服务端收到的请求行 `GET /q?a=1 HTTP/1.1`,头含 `Host`、`Accept-Encoding: identity`、`Connection: keep-alive`、`X-K: v`
  - `poll` 返回 `Ok(RestResp{tag: 11, status: 200, body: b"hello"})`;`take_retired()` 为 `None`
- 实现:`crates/hft-net/tests/rest.rs::net_260_content_length`

### NET-261 chunked 正文原地去 chunk

- 规则:[9. REST 客户端](README.md#9-rest-客户端)
- 级别:integ
- 里程碑:M3
- 前置:同 NET-260
- 步骤:
  1. 服务端回 `Transfer-Encoding: chunked`,块 `3\r\nabc\r\n`、`4\r\ndefg\r\n`、`0\r\n\r\n`
- 期望:
  - `body == b"abcdefg"`;`status == 200`
- 实现:`crates/hft-net/tests/rest.rs::net_261_chunked`

### NET-262 204 与 304 无正文

- 规则:[9. REST 客户端](README.md#9-rest-客户端)
- 级别:table
- 里程碑:M3
- 前置:同 NET-260;响应无 `Content-Length`、非 chunked
- 实现:`crates/hft-net/tests/rest.rs::net_262_no_body_statuses`

| 行 | 状态 | 期望 |
|---|---|---|
| NET-262.1 | 204 | `Ok(RestResp{status: 204, body: b""})`;连接不退役 |
| NET-262.2 | 304 | `Ok(RestResp{status: 304, body: b""})`;连接不退役 |
| NET-262.3 | 200 | `Err(RestFail{tag, err: Proto})`;`take_retired()` 给出该连接,`why` 含 `HTTP_PROTO` |

### NET-263 Connection: close 的响应交出后连接退役并补建

- 规则:[9. REST 客户端](README.md#9-rest-客户端)
- 级别:integ
- 里程碑:M3
- 前置:同 NET-260
- 步骤:
  1. 服务端回 `200`、`Content-Length: 2`、`Connection: close`、正文 `ok`;`poll`
  2. `take_retired()`;`next_build(0)`
- 期望:
  - `poll` 返回 `Ok(body == b"ok")`;`take_retired()` 给出该连接;`next_build` 返回 `Some(Fill)`
- 实现:`crates/hft-net/tests/rest.rs::net_263_connection_close`

### NET-264 rest_timeout_ms 恰好到达时超时,连接退役,迟到的响应不串到下一个请求

- 规则:[9. REST 客户端](README.md#9-rest-客户端)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`LoopPeer` 收到请求后不回;测试时钟;`rest_timeout_ms = 2000`;`submit` 于 `t0`(tag 21)
- 实现:`crates/hft-net/tests/rest.rs::net_264_timeout_boundary`

| 行 | 操作 | 期望 |
|---|---|---|
| NET-264.1 | `poll(t0 + 1_999_999_999)` | `None` |
| NET-264.2 | `poll(t0 + 2_000_000_000)` | `Some(Err(RestFail{tag: 21, err: Timeout}))`;`take_retired()` 给出该连接 |
| NET-264.3 | 之后服务端补发对 tag 21 的响应;在另一条连接 `submit` tag 22 并收到其响应 | tag 22 的 `RestResp.body` 是对 22 的响应;不出现 tag 21 的正文 |

### NET-265 响应超出接收缓冲给出 TooLarge

- 规则:[9. REST 客户端](README.md#9-rest-客户端)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`LoopPeer`;REST 连接 `rx_buf = 4096`;整条响应放进一条 TLS 记录(开销 22 B);响应头 H B
- 实现:`crates/hft-net/tests/rest.rs::net_265_too_large`

| 行 | 响应 | 期望 |
|---|---|---|
| NET-265.1 | 头 + 正文 = 4073 B | `Ok`,正文完整 |
| NET-265.2 | 头 + 正文 = 4074 B | `Ok`,正文完整 |
| NET-265.3 | 头 + 正文 = 4075 B | `Err(RestFail{err: TooLarge})`;连接退役 |
| NET-265.4 | `Content-Length: 5000`(头先到) | 头解析完即 `Err(TooLarge)`,不等正文 |

### NET-266 响应完整之前对端关闭给出 Closed

- 规则:[9. REST 客户端](README.md#9-rest-客户端)
- 级别:integ
- 里程碑:M3
- 前置:同 NET-260
- 步骤:
  1. 服务端回 `Content-Length: 100` 与 40 B 正文后关闭连接;`poll`
- 期望:
  - `Some(Err(RestFail{tag, err: Closed}))`;`take_retired()` 给出该连接
- 实现:`crates/hft-net/tests/rest.rs::net_266_closed_mid_response`

### NET-267 poll 只读有在途请求的连接

- 规则:[9. REST 客户端](README.md#9-rest-客户端)、[签名草案](README.md#签名草案)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M3
- 前置:`RestPool::<2>`,连接 r0、r1;`SysCount` 按 fd 计数
- 实现:`crates/hft-net/tests/rest.rs::net_267_poll_only_inflight`

| 行 | 在途 | 期望一次 `poll` 的 `recvmsg` 次数 |
|---|---|---|
| NET-267.1 | 无 | 0;返回 `None` |
| NET-267.2 | 只有 r1 | r0 上 0 次、r1 上 1 次 |
| NET-267.3 | r0 与 r1 | 各 1 次 |

### NET-268 idle_check 发现对端关闭的空闲连接并退役补建

- 规则:[9. REST 客户端](README.md#9-rest-客户端)、[签名草案](README.md#签名草案)
- 级别:table
- 里程碑:M3
- 前置:`LoopPeer`;`RestPool::<2>` 两条空闲连接 r0、r1;每行一个池
- 实现:`crates/hft-net/tests/rest.rs::net_268_idle_check_detects_close`

| 行 | 对端对 r1 的动作 | 期望 `idle_check` 返回值;`take_retired()` |
|---|---|---|
| NET-268.1 | 无 | 2;`None` |
| NET-268.2 | 发 FIN | 2;r1,`why` 含 `PEER_EOF`;随后 `next_build` = `Some(Fill)` |
| NET-268.3 | 发 RST | 2;r1,`why` 含 `RESET` |
| NET-268.4 | 发 close_notify | 2;r1,`why` 含 `TLS_ALERT` |
| NET-268.5 | 发未请求的 `HTTP/1.1 200 OK…` | 2;r1,`why` 含 `HTTP_PROTO` |

### NET-269 idle_check 不读有在途请求的连接

- 规则:[9. REST 客户端](README.md#9-rest-客户端)
- 级别:integ
- 里程碑:M3
- 前置:`RestPool::<2>`;r0 有在途请求,r1 空闲
- 步骤:
  1. `idle_check(now)`
- 期望:
  - 返回 1;r0 上 `recvmsg` 0 次,r1 上 1 次
- 实现:`crates/hft-net/tests/rest.rs::net_269_idle_check_skips_inflight`

### NET-270 submit 在空闲连接间按连接序号循环

- 规则:[9. REST 客户端](README.md#9-rest-客户端)
- 级别:integ
- 里程碑:M9
- 前置:`RestPool::<2>`,r0、r1 都空闲;每个请求在下一次 submit 前完成
- 步骤:
  1. 依次 submit 3 个请求
- 期望:
  - 服务端看到的连接依次 r0、r1、r0
- 实现:`crates/hft-net/tests/rest.rs::net_270_round_robin`

### NET-271 submit 的拒绝:Busy、NoConn、ReqTooLarge,均不加密

- 规则:[签名草案](README.md#签名草案)、[9. REST 客户端](README.md#9-rest-客户端)
- 级别:table
- 里程碑:M3
- 前置:`RestPool::<2>`;`tx_buf = 16384`
- 实现:`crates/hft-net/tests/rest.rs::net_271_submit_errors`

| 行 | 条件 | 期望 |
|---|---|---|
| NET-271.1 | r0、r1 都有在途 | `Err(Busy)`;发送序号不变 |
| NET-271.2 | 池里没有 Open 连接 | `Err(NoConn)` |
| NET-271.3 | `body` 20 000 B | `Err(ReqTooLarge)`;发送序号不变;`SysCount.sendmsg` 不增 |
| NET-271.4 | 只有 r0 在途 | `Ok`,请求走 r1 |

### NET-272 submit 时 sendmsg 出错返回 Send(Io),连接退役

- 规则:[签名草案](README.md#签名草案)、[9. REST 客户端](README.md#9-rest-客户端)
- 级别:unit
- 里程碑:M3
- 前置:`MemPeer` + `ScriptIo`;`sendmsg` 返回 `ECONNRESET`
- 步骤:
  1. `submit(req)`;`take_retired()`
- 期望:
  - `Err(RestErr::Send(SendErr::Io(104)))`;`take_retired()` 给出该连接
- 实现:`crates/hft-net/tests/rest.rs::net_272_submit_send_error`

### NET-273 每个请求一次加密一次 sendmsg,同一连接不做 pipelining

- 规则:[9. REST 客户端](README.md#9-rest-客户端)
- 级别:integ
- 里程碑:M3
- 前置:`RestPool::<1>`;`LoopPeer` 记录每条连接上请求到达的顺序
- 步骤:
  1. `submit(a)`;`submit(b)`
  2. 服务端回 a;`poll`;`submit(b)`
- 期望:
  - 步骤 1 第二次 `submit` 返回 `Err(Busy)`;此前服务端只收到 a
  - 步骤 2 后服务端收到 b;每个请求对应 `SysCount.sendmsg` +1 与发送序号 +1
- 实现:`crates/hft-net/tests/rest.rs::net_273_one_inflight_per_conn`

### NET-274 RestResp 的戳取响应首字节所在的读取

- 规则:[7. 时间戳](README.md#7-时间戳)、[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M3
- 前置:`MemPeer` + `ScriptIo`;响应分两次读取到达:第 1 次 HW = 40、返回后 TSC 1_000;第 2 次 HW = 50、返回后 TSC 2_000
- 步骤:
  1. `poll` 两次
- 期望:
  - 第二次 `poll` 返回 `Ok`,`resp.ts.hw_rx_ns == 40`,`resp.ts.rx_tsc == 2_000`
- 实现:`crates/hft-net/tests/rest.rs::net_274_rest_resp_stamps`

### NET-275 RestPool 的补建接口:连接数不足给 Fill,满时不给

- 规则:[签名草案](README.md#签名草案)
- 级别:unit
- 里程碑:M3
- 前置:`RestPool::<2>` 为空
- 步骤:
  1. `next_build(0)`;`next_build(0)`;`install(h1)`;`next_build(0)`;`install(h2)`;`next_build(0)`;`install(h3)`
- 期望:
  - 依次 `Some(Fill)`、`None`、`Ok`、`Some(Fill)`、`Ok`、`None`、`Err(PoolErr::Full)`
- 实现:`crates/hft-net/tests/rest.rs::net_275_rest_pool_build`

### NET-276 REST 请求也只在 submit 加密发送时推进记录序号

- 规则:[2. 发送路径](README.md#2-发送路径)、[9. REST 客户端](README.md#9-rest-客户端)
- 级别:unit
- 里程碑:M3
- 前置:`MemPeer`(服务端逐条解密并记录收到的记录序号);`RestPool::<1>` 一条 Open 连接
- 步骤:
  1. `submit` 一个请求;服务端回响应;`poll` 直到 `Some`
  2. `submit` 一个超过 tx 明文区的请求(得 `ReqTooLarge`)
  3. 再 `submit` 一个请求
- 期望:
  - 服务端收到的记录序号依次为 0、1;步骤 2 不推进发送序号
- 实现:`crates/hft-net/tests/rest.rs::net_276_only_submitted_requests_advance_tls`

### NET-277 RestPool 的热线程入口从不调用 connect、shutdown、close、getaddrinfo

- 规则:[调用约束](README.md#调用约束)、[9. REST 客户端](README.md#9-rest-客户端)
- 级别:unit
- 里程碑:M3
- 前置:`MemPeer` + `ScriptIo`;已交接的 REST 池;脚本含数据、`EAGAIN`、短写、EOF
- 步骤:
  1. 10⁴ 轮 `RestPool::submit`/`poll`/`idle_check`/`take_retired`
- 期望:
  - 热线程阶段 `SysCount` 中 `connect`、`shutdown`、`close`、`getaddrinfo`、`epoll_ctl` 的增量都为 0
- 实现:`crates/hft-net/tests/rest.rs::net_277_hot_rest_never_manages_connections`

### NET-285 稳态收发零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[10. 内存](README.md#10-内存)
- 级别:unit
- 里程碑:M1.5
- 前置:计数全局分配器;`MemPeer` + `ScriptIo`;`tls_mode = unbuffered`;`warm_interval_ns = 0`;预热 1 000 轮后清零计数
- 步骤:
  1. 10⁶ 轮:`poll_rx`、`next_frame`(300 B 帧)、`send_frame`(300 B)、`finish_poll`;另 10⁵ 次 `warm`(启用 scratch 时的零分配由 NET-181 覆盖)
- 期望:
  - 分配次数 0
- 实现:`crates/hft-net/tests/alloc.rs::net_285_steady_state_zero_alloc`

### NET-286 行情任务每轮的系统调用只有一次 epoll_pwait 与就绪连接上的 recvmsg

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[概要](README.md#概要)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`;12 条注册连接;每轮对端向随机 0–3 条写数据
- 步骤:
  1. 1 000 轮:`probe`;对就绪连接 `poll_rx`、`next_frame`、`finish_poll`
- 期望:
  - 每轮 `epoll_pwait == 1`;每轮 `recvmsg` 次数 = 该轮就绪数;`sendmsg`、`setsockopt` 等其他系统调用 0
- 实现:`crates/hft-net/tests/hot_syscalls.rs::net_286_feed_round_syscalls`

### NET-287 Conn 经 arena 槽号交接与回收,槽可复用

- 规则:[类型总览](README.md#类型总览)、[调用约束](README.md#调用约束)、[10. 内存](README.md#10-内存)
- 级别:integ
- 里程碑:M1.5
- 前置:`ConnArena` 2 个槽;`hft_rt::spsc` 交接环与回收环;warm 线程与热线程各一
- 步骤:
  1. warm 线程建连 → `handoff` → 经环送热线程 → `install` → 收发 10 帧 → `retire` → 经环送回 → `reclaim`;重复 10 次
- 期望:
  - 每次交接的 Handoff 带槽号 ∈ {0, 1};10 次后两个槽都空闲;每次都收发成功;无任何时刻两个线程同时持有同一槽(交接环计数核对)
- 实现:`crates/hft-net/tests/arena.rs::net_287_handoff_cycle`

### NET-295 依赖闭包里没有被禁止的 crate

- 规则:[依赖](README.md#依赖)
- 级别:table
- 里程碑:M1.5
- 前置:`cargo metadata` 取 `hft-net` 的 normal 依赖闭包;另有一份在 `Cargo.toml` 加入 `hft-journal` 的副本清单
- 实现:`crates/hft-net/tests/deps.rs::net_295_forbidden_deps`

| 行 | 检查 | 期望 |
|---|---|---|
| NET-295.1 | 闭包中的 `tokio`、`mio` | 不存在 |
| NET-295.2 | 闭包中的 `openssl`、`native-tls` | 不存在 |
| NET-295.3 | 闭包中的 `serde_json`、`tracing` | 不存在 |
| NET-295.4 | 闭包中的 `hft-venue`、`venue-*`、`hft-journal` | 不存在 |
| NET-295.5 | 闭包中的 `hft-event` | 存在;本 crate 源码对它的引用只有 `ConnRole` |
| NET-295.6 | 副本清单交给同一检查函数 | 报告 1 处违例:`hft-journal` |

### NET-300 NetConfig 默认值

- 规则:[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`NetConfig::default()`
- 实现:`crates/hft-net/tests/config.rs::net_300_defaults`

| 行 | 字段 | 期望 |
|---|---|---|
| NET-300.1 | `endpoints`、`src_ips`、`bind_device` | 空 |
| NET-300.2 | `src_port_range` | `40000..=40999` |
| NET-300.3 | `rss_plan`、`extra_ca` | `None` |
| NET-300.4 | `tx_timestamps` | false |
| NET-300.5 | `rx_buf` 按角色 Md / Order / UserStream / Rest | 262144 / 65536 / 65536 / 1048576 |
| NET-300.6 | `tx_buf`;`max_msg` | 16384;262144 |
| NET-300.7 | `so_rcvbuf`;`so_sndbuf` | 262144;65536 |
| NET-300.8 | `tcp_user_timeout_ms` | 5000 |
| NET-300.9 | `quickack_after_read` | `{md: false, order: false}` |
| NET-300.10 | `connect_timeout_ms`、`tls_timeout_ms`、`upgrade_timeout_ms` | 3000、3000、3000 |
| NET-300.11 | `rotate_stagger_s` | 3600 |
| NET-300.12 | `warm_interval_ns` | 0 |
| NET-300.13 | `rest_timeout_ms` | 2000 |
| NET-300.14 | `ts_expect_hw` | true |
| NET-300.15 | `tls_mode` | `Unbuffered` |

### NET-301 tcp_user_timeout_ms 原样写进 socket

- 规则:[配置项](README.md#配置项)、[5. socket 选项清单](README.md#5-socket-选项清单)
- 级别:table
- 里程碑:M1.5
- 前置:`LoopPeer`;每行一次建连
- 实现:`crates/hft-net/tests/config.rs::net_301_tcp_user_timeout`

| 行 | `tcp_user_timeout_ms` | 期望 `getsockopt(TCP_USER_TIMEOUT)` |
|---|---|---|
| NET-301.1 | 4999 | 4999 |
| NET-301.2 | 5000 | 5000 |
| NET-301.3 | 5001 | 5001 |

### NET-302 Connector 建出的连接带配置的 rx_buf 与 tx_buf

- 规则:[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`LoopPeer`;`NetConfig::default()`;测试钩子读 `Conn` 的缓冲容量
- 实现:`crates/hft-net/tests/config.rs::net_302_buffer_sizes_by_role`

| 行 | 角色 | 期望 rx / tx 容量 |
|---|---|---|
| NET-302.1 | Md | 262144 / 16384 |
| NET-302.2 | Order | 65536 / 16384 |
| NET-302.3 | UserStream | 65536 / 16384 |
| NET-302.4 | Rest | 1048576 / 16384 |

### NET-320 基准:next_frame 300 B 单记录单帧(内存对端)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`net_rx_300b_mem`
- 实现:`crates/hft-net/benches/rx.rs::net_320_rx_300b_mem`

### NET-321 基准:next_frame 2 KiB

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`net_rx_2k_mem`
- 实现:`crates/hft-net/benches/rx.rs::net_321_rx_2k_mem`

### NET-322 基准:积压 64 KiB 时取出第一帧

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`net_rx_first_of_64k`
- 实现:`crates/hft-net/benches/rx.rs::net_322_rx_first_of_64k`

### NET-323 基准:poll_rx + next_frame 回环 300 B

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`net_poll_loopback`
- 实现:`crates/hft-net/benches/rx.rs::net_323_poll_loopback`

### NET-324 基准:poll_rx 空轮询(EAGAIN)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`net_poll_empty`
- 实现:`crates/hft-net/benches/rx.rs::net_324_poll_empty`

### NET-325 基准:ReadyPoll::probe

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`net_ready_probe`
- 实现:`crates/hft-net/benches/ready.rs::net_325_ready_probe`

### NET-326 基准:MaskedFrame::seal 300 B

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`net_seal_300b`
- 实现:`crates/hft-net/benches/tx.rs::net_326_seal_300b`

### NET-327 基准:send_frame 加密部分 300 B

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`net_encrypt_300b_mem`
- 实现:`crates/hft-net/benches/tx.rs::net_327_encrypt_300b_mem`

### NET-328 基准:send_frame 含 sendmsg(回环)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`net_send_loopback`
- 实现:`crates/hft-net/benches/tx.rs::net_328_send_loopback`

### NET-329 基准:warm 300 B(可选)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M7
- 状态:可选扩展:TLS 预热
- 基准名:`net_warm_300b`
- 实现:`crates/hft-net/benches/tx.rs::net_329_warm_300b`

### NET-330 基准:ConnPool::swap_primary

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`net_pool_swap`
- 实现:`crates/hft-net/benches/tx.rs::net_330_pool_swap`

### NET-331 基准:RssPlan::pick(可选)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M7
- 状态:可选扩展:RSS 源端口预计算
- 基准名:`net_rss_pick`
- 实现:`crates/hft-net/benches/rss.rs::net_331_rss_pick`

### NET-335 fuzz:WS 解帧

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:fuzz
- 里程碑:M1.5
- 目标:`fuzz_ws_deframe`:任意字节作为服务端明文,按 fuzz 输入给出的切分点分批送入 `MemPeer` → `poll_rx`/`next_frame` 循环
- 语料:真实 raw journal 帧(`fixtures/<venue>/` 的 `.in`)+ NET-020–NET-044 的构造帧
- 判定:无 panic、无越界、无超时,口径见 [testing §3.3](../../docs/testing.md#33-fuzz);任何输入下 `next_frame` 返回的 payload 都落在接收缓冲或分片 scratch 内
- 实现:`crates/hft-net/fuzz/fuzz_targets/ws_deframe.rs::net_335_fuzz_ws_deframe`

### NET-336 fuzz:HTTP 响应解析

- 规则:[9. REST 客户端](README.md#9-rest-客户端)
- 级别:fuzz
- 里程碑:M6
- 目标:`fuzz_http_response`:任意字节作为一条在途请求的响应送入 `RestPool::poll`
- 语料:各所 REST 响应样本(`fixtures/<venue>/rest.*`)+ NET-260–NET-266 的构造响应
- 判定:同 [testing §3.3](../../docs/testing.md#33-fuzz);结果只可能是 `None`、`Ok(RestResp)` 或 `Err(RestFail)`
- 实现:`crates/hft-net/fuzz/fuzz_targets/http_response.rs::net_336_fuzz_http_response`

### NET-337 fuzz:cmsg 解析

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:fuzz
- 里程碑:M1.5
- 目标:`fuzz_cmsg`:任意字节与任意 `msg_flags` 作为 `recvmsg` 的控制缓冲送入 `src/cmsg.rs` 解析
- 语料:NET-120 各行的合成缓冲
- 判定:同 [testing §3.3](../../docs/testing.md#33-fuzz);输出的 `flags` 只含 `HW`、`SW` 位
- 实现:`crates/hft-net/fuzz/fuzz_targets/cmsg.rs::net_337_fuzz_cmsg`

### NET-340 buffered 后端通过同一组接收性质

- 规则:[配置项](README.md#配置项)、[1. 接收路径](README.md#1-接收路径)
- 级别:prop
- 里程碑:M9
- 性质:`tls_mode = buffered` 时 NET-092 与 NET-136 的性质成立,且同一输入下两种后端返回的帧序列与 `Timestamps` 逐字段相同
- 生成:同 NET-092
- 参考:同一输入在 `unbuffered` 后端上的输出
- 例数:PR 1 000
- 实现:`crates/hft-net/tests/rx_prop.rs::net_340_buffered_same_frames`

### NET-341 buffered 后端也只在发送时推进记录序号

- 规则:[配置项](README.md#配置项)、[2. 发送路径](README.md#2-发送路径)
- 级别:unit
- 里程碑:M9
- 前置:同 NET-151,`tls_mode = buffered`
- 步骤:
  1. 执行 NET-151 的全部步骤
- 期望:
  - 与 NET-151 的期望相同
- 实现:`crates/hft-net/tests/tx_path.rs::net_341_buffered_seq_only_on_send`

### NET-342 buffered 后端在极小发送缓冲下全部帧送达

- 规则:[配置项](README.md#配置项)、[2. 发送路径](README.md#2-发送路径)
- 级别:integ
- 里程碑:M9
- 前置:同 NET-163,`tls_mode = buffered`
- 步骤:
  1. 执行 NET-163 的步骤
- 期望:
  - 与 NET-163 的期望相同
- 实现:`crates/hft-net/tests/loopback.rs::net_342_buffered_tiny_sndbuf`

### NET-350 Toeplitz 哈希与 Microsoft RSS 验证向量一致

- 规则:[6. 源端口预计算(`RssPlan`,可选)](README.md#6-源端口预计算rssplan可选)
- 级别:table
- 里程碑:M7
- 状态:可选扩展:RSS 源端口预计算
- 前置:40 B key `6d5a56da255b0ec24167253d43a38fb0d0ca2bcbae7b30b477cb2da38030f20c6a42b73bbeac01fa`;输入 = 远端 IP ‖ 本地 IP ‖ 远端端口 ‖ 本地端口(向量的 source = 远端,destination = 本地)
- 实现:`crates/hft-net/tests/rss.rs::net_350_toeplitz_vectors`
- 备注:向量取自 Microsoft《Verifying the RSS Hash Calculation》的 IPv4/TCP 表

| 行 | 远端 → 本地 | 期望哈希 |
|---|---|---|
| NET-350.1 | 66.9.149.187:2794 → 161.142.100.80:1766 | `0x51ccc178` |
| NET-350.2 | 199.92.111.2:14230 → 65.69.140.83:4739 | `0xc626b0ea` |
| NET-350.3 | 24.19.198.95:12898 → 12.22.207.184:38024 | `0x5c2b394a` |
| NET-350.4 | 38.27.205.30:48228 → 209.142.163.6:2217 | `0xafc7327f` |
| NET-350.5 | 153.39.163.191:44251 → 202.188.127.2:1303 | `0x10e828a2` |

### NET-351 pick 返回落在目标队列、未被占用、范围内升序第一个端口

- 规则:[6. 源端口预计算(`RssPlan`,可选)](README.md#6-源端口预计算rssplan可选)
- 级别:prop
- 里程碑:M7
- 状态:可选扩展:RSS 源端口预计算
- 性质:`pick(local, remote, rport, q, used)` 返回 `Some(p)` 时 `p ∈ src_port_range`、`p ∉ used`、`indir[toeplitz(...) & 127] == q`,且范围内所有小于 p 的未占用端口都不落在 q;返回 `None` 时范围内不存在这样的端口
- 生成:随机 key 与 128 项间接表(4 队列);随机 IP 与远端端口;`used` 占范围的 0–90%;`src_port_range = 40000..=40999`
- 参考:逐端口线性扫描的参考实现
- 例数:PR 1 000
- 实现:`crates/hft-net/tests/rss.rs::net_351_pick_property`

### NET-352 哈希函数不是 Toeplitz 的 rss_plan 被拒绝加载

- 规则:[6. 源端口预计算(`RssPlan`,可选)](README.md#6-源端口预计算rssplan可选)
- 级别:unit
- 里程碑:M7
- 状态:可选扩展:RSS 源端口预计算
- 前置:`rss_plan` 文件声明 `hfunc = crc32`
- 步骤:
  1. 加载 `RssPlan`;以该配置 `submit` 一个作业
- 期望:
  - 加载返回错误;作业的 `bind` 端口参数为 0(内核分配)
- 实现:`crates/hft-net/tests/rss.rs::net_352_non_toeplitz_rejected`

### NET-353 落点与计划队列不符时置 WRONG_QUEUE,换端口最多重建 3 次

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)、[6. 源端口预计算(`RssPlan`,可选)](README.md#6-源端口预计算rssplan可选)
- 级别:table
- 里程碑:M7
- 状态:可选扩展:RSS 源端口预计算
- 前置:`ScriptIo`;`rss_plan` 启用;`BuildReq.rx_queue = 2`;脚本控制每次握手后 `SO_INCOMING_NAPI_ID` 对应的队列
- 实现:`crates/hft-net/tests/rss.rs::net_353_wrong_queue_retries`

| 行 | 各次落点队列 | 期望 |
|---|---|---|
| NET-353.1 | 2 | 1 次建连;`Ready`;不置 `WRONG_QUEUE` |
| NET-353.2 | 1、2 | 2 次建连(第 2 次用 `pick` 的下一个候选端口);`Ready` |
| NET-353.3 | 1、1、1、1 | 4 次建连(首次 + 3 次重建);`Ready`;连接 `err()` 含 `WRONG_QUEUE` |

### NET-360 tx_timestamps 关闭时不设 TX 标志、不 dup fd

- 规则:[7. 时间戳](README.md#7-时间戳)、[配置项](README.md#配置项)
- 级别:integ
- 里程碑:M1.5
- 前置:`LoopPeer`;`tx_timestamps = false`
- 步骤:
  1. 建连;发送 10 帧
- 期望:
  - `SO_TIMESTAMPING` 不含 `TX_SCHED`、`TX_SOFTWARE`、`OPT_ID`、`OPT_TSONLY`;`dup` 调用 0 次;没有 `TxTag` 产生
- 实现:`crates/hft-net/tests/txts.rs::net_360_tx_ts_off`

### NET-361 回环上 TX_SOFTWARE 戳按 tskey 与 TxTag 配对

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:integ
- 里程碑:M7
- 状态:可选扩展:TX 软件时间戳
- 前置:`LoopPeer`;`tx_timestamps = true`;`TxTsDrain` 注册该连接的 dup fd
- 步骤:
  1. 发送 3 帧(密文 322、500、700 B),记录 `TxTag.tskey_end`;等待 1 ms;`drain`
- 期望:
  - `tskey_end` 依次为 321、821、1521;`drain` 产出的 `TxStamp{kind: 2}` 的 `tskey` 恰为这三个值,`ts_ns > 0`
- 实现:`crates/hft-net/tests/txts.rs::net_361_tx_sw_pairing`

### NET-362 存在 dup fd 时 reclaim 仍使对端收到 FIN

- 规则:[7. 时间戳](README.md#7-时间戳)、[签名草案](README.md#签名草案)
- 级别:integ
- 里程碑:M7
- 状态:可选扩展:TX 软件时间戳
- 前置:同 NET-361
- 步骤:
  1. `retire`;`reclaim`(dup fd 仍由 `TxTsDrain` 持有)
- 期望:
  - 对端在 100 ms 内读到 EOF
- 实现:`crates/hft-net/tests/txts.rs::net_362_dup_fd_reclaim_fin`

### NET-370 M1.5 探针上 HW RX 戳可用性与覆盖率

- 规则:[7. 时间戳](README.md#7-时间戳)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:host
- 里程碑:M1.5
- 环境:M1.5 延迟探针(按需租用的 `c8g.4xlarge`,ENI-B,PHC 已启用),纵切片 hft-core 订阅一个 venue 的 BBO 流,`ts_expect_hw = true`
- 授权:`[需授权]`(租用与删除实例由负责人执行)
- 步骤:
  1. `ethtool -T <eni-b>` 记录能力;运行纵切片 ≥ 1 h
  2. 读各 BBO 连接的 `stats().ts_reads`、`hw_ts_reads`、`multi_skb` 与 `TS_MISSING` 计数
- 通过标准:HW 戳可用时 `hw_ts_reads / ts_reads ≥ 0.999`;不可用时记录"不可用"并按 [testing §8.2](../../docs/testing.md#82-延迟验收的测法) 改用 SW 数据集(本用例仍算完成);`multi_skb / frames` 的比例写入结论(> 1% 时触发已知限制中的逐包读评估)
- 证据:`tests/host/NET-370.sh` 的输出 `NET-370-<host>-<日期>.txt`,附在 M1.5 探针 Agent Note
- 实现:`tests/host/NET-370.sh`

### NET-371 ENA 上握手后 napi_id 非 0,映射到交易网卡

- 规则:[4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)](README.md#4-tls-与-ws-握手warm-线程-conn-builderconnector)
- 级别:host
- 里程碑:M1.5
- 环境:M1.5 延迟探针,内核 ≥ 6.9
- 授权:`[需授权]`
- 步骤:
  1. 建 20 条行情连接,记录 `Ready.napi_id` 与 `Connector::napi_ifindex(napi_id)`
  2. 与 `ip -j link` 中 ENI-B 的 ifindex 比较
- 通过标准:20 条的 `napi_id` 都非 0,`napi_ifindex` 都等于 ENI-B 的 ifindex;若握手后为 0,按开发备忘第 9 条的备选方案修改规格后重测
- 证据:`NET-371-<host>-<日期>.txt`,附在 M1.5 探针 Agent Note
- 实现:`tests/host/NET-371.sh`

### NET-372 SO_BUSY_POLL = 0 时空的非阻塞 recvmsg 不进入 napi_busy_loop

- 规则:[5. socket 选项清单](README.md#5-socket-选项清单)、[概要](README.md#概要)
- 级别:host
- 里程碑:M1.5
- 环境:M1.5 延迟探针;临时把 `net.core.busy_read` 设为 50
- 授权:`[需授权]`
- 步骤:
  1. bpftrace 计 `napi_busy_loop` 调用次数;对一条无数据的连接 `poll_rx` 10⁶ 次;同时跑 `net_poll_empty`
  2. 恢复 `busy_read = 0`
- 通过标准:`napi_busy_loop` 计数 0;`setsockopt(SO_BUSY_POLL, 0)` 在非特权进程下成功
- 证据:`NET-372-<host>-<日期>.txt`(bpftrace 输出与基准 JSON),附在 M1.5 探针 Agent Note
- 实现:`tests/host/NET-372.sh`

### NET-373 TCP recvmsg 报告的是本次读取最后一个 skb 的戳

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:host
- 里程碑:M1.5
- 环境:M1.5 延迟探针或 dev2;回环 + `tc netem` 让两包间隔 1 ms
- 授权:dev2 上运行时为无;探针上 `[需授权]`
- 步骤:
  1. 对端发两包(各 200 B,间隔 1 ms),本端在第二包到达后一次读出;重复 1 000 次
  2. 比较 `sw_rx_ns` 与两包各自的到达时刻(对端发送 TSC + netem 延迟)
- 通过标准:≥ 99% 的样本 `sw_rx_ns` 落在第二包的到达时刻 ± 50 µs 内;结果与开发备忘第 1 条一致则备忘改为已核实,否则修改 §7 的取戳规则
- 证据:`NET-373-<host>-<日期>.txt`
- 实现:`tests/host/NET-373.sh`

### NET-374 有数据时非阻塞 recvmsg 总是填满所给空间

- 规则:[1. 接收路径](README.md#1-接收路径)
- 级别:host
- 里程碑:M1.5
- 环境:dev2 或 M1.5 探针,内核 6.12
- 授权:dev2 上运行时为无;探针上 `[需授权]`
- 步骤:
  1. 回环对端持续写入;bpftrace 在 `tcp_recvmsg` 返回点比较返回值、所给长度与 `sk_rmem_alloc`;运行 10 min
- 通过标准:不存在"返回值 < 所给长度且返回后 `sk_rmem_alloc > 0`"的样本;存在时按开发备忘第 11 条把短读规则改为"再读一次"并更新 NET-080
- 证据:`NET-374-<host>-<日期>.txt`
- 实现:`tests/host/NET-374.sh`

### NET-375 dev2 上 SO_BINDTODEVICE 所需权限

- 规则:[5. socket 选项清单](README.md#5-socket-选项清单)、[配置项](README.md#配置项)
- 级别:host
- 里程碑:M3
- 环境:dev2,hft-core 单元(`AmbientCapabilities` 按 deploy 单元表)
- 授权:无
- 步骤:
  1. 以单元用户分别在带与不带 `CAP_NET_RAW` 时建连并读 `getsockopt(SO_BINDTODEVICE)`
- 通过标准:单元配置下建连成功且绑定网卡名与 `bind_device` 一致;不带 `CAP_NET_RAW` 时的结果写进开发备忘第 7 条
- 证据:`NET-375-dev2-<日期>.txt`
- 实现:`tests/host/NET-375.sh`

### NET-376 升级主机上生产连接 HW 戳覆盖率不低于 99.9%

- 规则:[7. 时间戳](README.md#7-时间戳)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,prod 实例,PHC 开启
- 授权:无
- 步骤:
  1. 连续 24 h 汇总全部交易连接的 `stats().ts_reads`、`hw_ts_reads` 与 `TS_MISSING` 计数
- 通过标准:`hw_ts_reads / ts_reads ≥ 0.999`(PHC 不可用时本项不适用,写明原因)
- 证据:`NET-376-<host>-<日期>.txt`,归入门槛 ⑩ 的证据目录
- 实现:`tests/host/NET-376.sh`

## 验收

- **完成标准**:里程碑已到的全部用例有同编号的实现且通过(`cargo xtask test-trace`,[docs/tests §6](../../docs/tests/README.md#6-与代码的对应));M1.5 纵切片用到的接收、发送、时间戳、Connector 与 ReadyPoll 用例(里程碑 M1.5)在 M1.5 探针之前全绿,其余随 M3。三项可选扩展(`TLS 预热`、`RSS 源端口预计算`、`TX 软件时间戳`)的用例带 `状态:可选扩展`,在该扩展启用的同一提交列入 `tools/xtask/test-trace.toml` 的 `enabled_extensions` 后才要求实现([docs/tests §6](../../docs/tests/README.md#6-与代码的对应))。
- **属性测试的大例数运行**(每夜):NET-092、NET-136 各 100 000 例;NET-061 1 000 000 例;NET-340 20 000 例;NET-351 10 000 例。
- **基准**:NET-320–NET-331 在参考机上满足 [延迟与资源契约](README.md#延迟与资源契约) 的预算(判定规则与上限登记归 [benchmarks](../../benchmarks/README.md));`kernel_rx` 在 M1.5 探针与升级主机上只记录,不是本组件的门槛(⑩ 的口径归 [testing §8](../../docs/testing.md#8-上线门槛))。
- **零分配**:NET-285 通过(只对 `tls_mode = unbuffered` 要求)。
- **fuzz**:NET-335–NET-337 零 panic、零超时;随轮转运行,不计入门槛 ③(计数目标归 [testing §8](../../docs/testing.md#8-上线门槛))。
- **HW 戳覆盖率**:NET-376 在升级主机上通过(PHC 开启时 ≥ 99.9%)。
- **可选扩展**:RSS 源端口预计算(NET-331、NET-350–NET-353)、TX 时间戳(NET-361、NET-362)、scratch TLS 预热(NET-181、NET-182、NET-329)默认关闭;启用其中任一项之前,对应用例必须全绿。
- **待核实项的关闭**:NET-370–NET-375 的结论写进 Agent Note,README 开发备忘的对应条目同一改动改为已核实或按备选方案修改规格与用例。

NET-264 同时验证主动取消先退役连接，缓冲中的迟到响应不会被下一 tag 消费；NET-274 核对响应的 ConnId 与 sent_tsc。
