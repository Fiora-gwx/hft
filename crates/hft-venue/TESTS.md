---
component: hft-venue
prefix: VEN
spec: README.md
---

# hft-venue 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本 crate 的用例以桩协议 `StubProto` 与脚本化连接驱动 `FeedRuntime`、`SnapFetchTask`、`GwRuntime`、`AcctFetchTask` 与 `SessionDriver`,与具体交易所无关;各 `venue-*` 的报文金样与契约检查在各自的 `TESTS.md`,真实 venue 协议与运行时、sequencer 组合的跨组件场景在 [scenarios](../../docs/tests/scenarios.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | VEN-014、VEN-020、VEN-021、VEN-241、VEN-389 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 协议接口 | VEN-018、VEN-064、VEN-341 |
| 出环:`OutMsg = GwCmd` | VEN-194、VEN-196、VEN-199–VEN-206、VEN-214、VEN-235–VEN-238、VEN-240、VEN-241 |
| 运行时入口 | VEN-007、VEN-011、VEN-013、VEN-071、VEN-084、VEN-085、VEN-161、VEN-210、VEN-245、VEN-264 |
| 产生与消费的事件 | VEN-005、VEN-065、VEN-160、VEN-205、VEN-233、VEN-242、VEN-243、VEN-303、VEN-320、VEN-321 |
| 配置项 | VEN-006、VEN-008–VEN-012、VEN-015–VEN-017、VEN-028、VEN-067、VEN-068、VEN-076、VEN-080、VEN-081、VEN-159、VEN-176、VEN-208、VEN-209、VEN-219、VEN-222、VEN-229、VEN-234、VEN-239、VEN-246、VEN-251、VEN-253、VEN-256、VEN-257、VEN-261、VEN-293、VEN-301、VEN-388 |
| 实现说明 | 不适用:见子节 |
| 1. 线程与环(由 hft-core 装配) | VEN-013、VEN-020–VEN-027、VEN-030–VEN-036、VEN-069–VEN-074、VEN-086、VEN-087、VEN-212、VEN-380、VEN-383 |
| 2. `FeedRuntime::poll_once` 的固定顺序 | VEN-017、VEN-040–VEN-046、VEN-050–VEN-056、VEN-060–VEN-069、VEN-075、VEN-077–VEN-084、VEN-330、VEN-367、VEN-384 |
| 3. 去重、断裂与重同步 | VEN-001、VEN-100–VEN-109、VEN-129、VEN-171 |
| 3a. BBO 连接健康 | VEN-026、VEN-031、VEN-075、VEN-076、VEN-079、VEN-082、VEN-103、VEN-104、VEN-120–VEN-134、VEN-360、VEN-389 |
| 3b. 快照获取(snap-fetch) | VEN-002、VEN-007、VEN-130、VEN-133、VEN-150–VEN-163、VEN-387 |
| 3c. 分档快照(DepthTopN) | VEN-028、VEN-074、VEN-078、VEN-170–VEN-176、VEN-330、VEN-385 |
| 4. `GwRuntime::poll_once` 的循环结构 | VEN-014、VEN-190–VEN-249、VEN-357、VEN-369、VEN-386、VEN-389 |
| 4a. 账户查询:保证金轮询与配置核验 | VEN-003、VEN-007、VEN-015、VEN-211、VEN-225、VEN-227、VEN-250–VEN-265、VEN-313、VEN-364、VEN-365、VEN-389 |
| 4b. 私有频道归属与热备连接 | VEN-014、VEN-241、VEN-270–VEN-276、VEN-381 |
| 5. 心跳 | VEN-120、VEN-134、VEN-280–VEN-283 |
| 6. exch id → slot 私有表(`IdTable`) | VEN-004、VEN-215、VEN-290–VEN-297、VEN-358、VEN-386 |
| 6a. 去重环的重启重建 | VEN-300–VEN-304、VEN-382 |
| 7. 会话建立(warm 线程 `conn-builder`,`SessionDriver`) | VEN-310–VEN-315 |
| 8. 源文件规划 | VEN-341 |
| 延迟与资源契约 | VEN-046、VEN-053、VEN-340、VEN-350–VEN-369 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | VEN-341 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | VEN-381–VEN-388 |

## 用例

夹具(`crates/hft-venue/tests/support/`,只给本文件的用例用;本 crate 不依赖任何 `venue-*`,含 dev 依赖):

- **`StubProto`**:实现 `VenueProtocol` 的桩协议,`NAME = "stub-perp"`、`VENUE = V7`(测试 venue 号 7)、`MARKET = LinearPerp`、`ORDER_FRAME_CAP = 2048`。报文是以 `|` 分隔的文本行:
  - 行情:`B|<sym>|<u>|<bid>|<bq>|<ask>|<aq>`(Bbo,`seq_rule = Monotonic`,`u` 为 update_id)、`T|<sym>|<tid>|<px>|<qty>|<side>`(Trade,`Monotonic`)、`M|<sym>|<ex_ms>|<px>`(Mark,`ByTime`)、`I|<sym>|<ex_ms>|<px>`(Index,`ByTime`)、`F|<sym>|<ex_ms>|<rate>|<next_ms>`(Funding,`ByTime`)、`D|<sym>|<seq>|<n>|<px qty>×n…`(分档快照,`Snapshot`)、`DT|<sym>|<ex_ms>|<n>|…`(无序号分档快照,`ByTime`);控制 `ACK|<sub_id>|<ok>|<code>`、`PONG|<tsc>`、`BYE`(`ServerShutdown`);`B|ZZZ|…`(未订阅符号,`peek` 返回 `inst = 0`);`B|S1|x…`(类别已知、数值坏 → `DecodeErr::Number`)。
  - 回报:`O|<cid>|<exch_id>|<what>|<qty>|<cum>|<avg>|<fee>|<fee_asset>|<tid>`(`what` ∈ Ack/Fill/PartialFill/CancelAck/FeeOnly/Reject…;`cid` 为空表示回报不带 client id)、`X|<cause>|…`(venue 直接判定的 `ExternalFill`)、`E|<name>`(用户流事件:`listenKeyExtended` 为名单内,其余名字为未知)、`R|<req>|<code>`(响应码:`-2010` 登记为确定拒绝,`-1007` 登记为结果未知,其余未登记)、`MP|…`(保证金推送)、`FS|<sym>|<settle_ms>|<amt>|<disc>`(资金费结算)、`H|MARGIN_CALL`(`hint = MarginNow`)、`H|CFG`(`hint = ConfigRecheck`)、`Z|…`(类别已知、字段坏 → `DecodeErr::Shape`)。
  - client id 外形:`"st-"` + hft-types 规范 24 位十六进制。
  - 开关 `StubCfg`:`bbo_snapshot`(`bbo_snapshot_request` 返回 `Ok(true)` / `Ok(false)`)、`bbo_batch`(一次请求可覆盖的 inst 上限,传入更多时返回 `Err(ProtoErr::BufFull)`)、`account_cancel_all`(为假时 `encode` 对 `CancelAll{inst = 0}` 返回 `Err(EncodeErr::Unsupported(GwOp::CancelAll))`)、`query_by_cid`(为假时 `exch_order_id = 0` 的 Query/Cancel 返回 `Err(EncodeErr::UnknownTarget)`)、`dead_man`(`None` / `CountdownPerInst{min_ms: 5000, timeout_ms: 30000, refresh_ms: 10000, via: Ws}` / `CountdownAccount{…, via: Rest}`)、`session_identity`(`session_identity_request` 返回 `Ok(true)` / `Ok(false)`)、`user_stream`(为真时私有频道在独立用户流连接上)、`inst_cancel_all`(为假时 `encode` 对 `CancelAll{inst ≠ 0}` 也返回 `Unsupported(CancelAll)`)、`rest_query`(为真时 `order_channel(Query) = Rest`,查单经 `encode_rest`/`decode_rest`)、`keepalive`(`session_keepalive_request` 返回 `Ok(true)` / `Ok(false)`);`R|<req>|-1125` 置 `ctl = UserStreamEnded`,`E|SHUTDOWN` 置 `ctl = ServerShutdown`。类型参数给出 `CANCEL_ALL_SWEEP`(默认 false)与 `BBO_SNAPSHOT_BATCH`(默认 `u16::MAX`;BBO 补帧的 `BufFull` 由 `bbo_batch` 模拟请求长度上限)。
  - 每次 trait 方法调用记入 `CallLog`(方法名、调用线程 id、参数摘要),用于"从不调用"类断言。
- **`ScriptNet`**:基于 hft-net feature `test-io` 的 `ScriptIo` 与 `MemPeer`(真实 rustls 会话、真实 WS 分帧,[hft-net TESTS](../hft-net/TESTS.md) 的同名夹具):`push(conn, payload, at_tsc)` 让对端发一条 WS 文本帧,`pong(conn, at_tsc)` 回 WS Pong,`eof(conn)`/`rst(conn)` 断开,`set_ready(conn, bool)` 决定 `ReadyPoll::probe` 的结果,`sent(conn)` 返回本端发出的明文帧序列,`SysCount` 按连接与种类(`recvmsg`、`sendmsg`、`epoll_pwait`、`epoll_ctl`)计数。
- **`BuildStub`**:代替 conn-builder:读 `BuildReq` 环;用例调用 `complete(req, conn)` 把一条 `ScriptNet` 连接以请求方给出的 `ConnId` 交接,`fail(req)` 报建连失败;`retired()` 返回收到的 `RetireMsg`。
- **`Taps`**:`bbo`、`md`(含 bulk)、`rep`(含 bulk)、`snapreq`、`snap`、`acctreq`、`acct` 各环的消费端,按顺序收集元素;`raw(j)` 按顺序读出 journal `j` 的记录(`RawHdr` + payload + 记录头 `tsc`);`Order` 把各环与 journal 的写入合成一条全序日志(用于"先入环、后写 raw"一类断言)。
- **时钟**:hft-testkit `ManualTsc`,1 tick = 1 ns;`TscUtc` 取 `utc_ns = tsc`。用例中的 TSC 值即纳秒。
- **`rd_stub()`**:venue V7;inst `S1` = 701、`S2` = 702、`S3` = 703(价格指数 −2、数量指数 −3),规模用例另有 `P001`–`P300` = 7001–7300;资产 USDT(数量指数 −6)、BNB(−8);账户 A1(Live,`venue_account_ix = 0`)、A2(Live,`venue_account_ix = 11`)、A9(Sim);`VenueMeta{ws_max_streams_per_conn: 50, ws_max_in_msgs_per_s: 10, ws_max_conn_age_s: 86400}`;`VenueCaps{margin_data: Poll{interval_ms: 2000}, flags: AMEND | POST_ONLY | IOC | BATCH_CANCEL, reduce_only: true, ignorable_user_events: ["listenKeyExtended"], funding_settle: PerSymbolPush}`;`RateLimitTable`:(V7, A1)、(V7, A2) 下单桶各 10 条 / 1000 ms,REST 权重桶各 100 / 60 000 ms。
- **基准配置**(用例在 `前置` 里写"`F0`"等即表示取下列显式值,再列出本用例改动的键):
  - `F0`(`FeedRuntime`):`routes = All`、`ready = false`、`feed.bbo_conns = 2`、`feed.bbo_frames_per_poll = 8`、`feed.frames_per_poll = 64`、`feed.bbo_ping_ms = 1000`、`feed.bbo_dead_ms = 3000`、`feed.bbo_refresh_grace_ms = 500`、`feed.depth = topn`、`feed.topn_levels = 5`、`feed.snapshot_part_levels = 64`、`feed.raw_dups = false`、`feed.raw.bbo = on`、`feed.raw.md = on`、`feed.raw.sample_ms = 1000`、`feed.ring_full_park_us = 200`、`conn.rotate_margin_s = 3600`、`max_sub_insts = 50`;环容量 bbo 1024、md 1024、md-bulk 1 MiB、feedctl 256、snapreq 64、snap 256 KiB;`StubCfg{bbo_snapshot: true, bbo_batch: 4}`。
  - `S0`(`SnapFetchTask`):`snap.parallel = 2`、`gw.rest_idle_check_ms = 1000`、hft-net `rest_timeout_ms = 2000`;snap 环 256 KiB。
  - `G0`(`GwRuntime`,`CAP = 2048`):`gw.cmds_per_loop = 16`、`gw.cmds_per_account_per_loop = 8`、`gw.aux_every = 8`、`gw.rest_idle_check_ms = 1000`、`gw.rotate_wait_max_ms = 60000`、`gw.warm_interval_ns = 0`、`gw.id_table_cap = 16384`、`gw.id_tomb_cap = 4096`、`gw.id_tomb_ms = 120000`、`gw.unmatched_hold_ms = 5000`、`gw.unmatched_hold_cap = 256`、`gw.dedup_reseed_ms = 900000`、`gw.inflight_timeout_ms = 3000`、`gw.report_decode_down_after = 3`、`gw.stale_guard_ms = 1500`、`gw.margin_poll_min_ms = 1000`、`gw.margin_push_stale_ms = 5000`、`gw.acct_cfg_recheck_s = 600`、`gw.acct_query_timeout_ms = 3000`、`gw.acct_query_degraded_after = 3`、`gw.acct_body_max = 262144`、`gw.unknown_evt_note_every = 1000`、OMS 槽数 2048;账户 A1(`kill_ix = 0`)与 A2(`kill_ix = 11`),各自 primary 与 standby 已 `Open`、已登录;环容量 out 1024、rep 1024、rep-bulk 1 MiB、acctreq 64、acct 1 MiB;`kill_mask = GLOBAL | VENUE | HARD`,kill 字为 0;`SetEpoch{epoch = 5, accept_prev = false}` 已处理;`StubCfg{account_cancel_all: true, query_by_cid: true, dead_man: CountdownPerInst, user_stream: false}`。
  - `C0`(`AcctFetchTask`):`gw.acct_query_timeout_ms = 3000`、`gw.acct_body_max = 262144`、`gw.rest_idle_check_ms = 1000`;acct 环 1 MiB;`ACCT_RAW_CHUNK = 16384`。

### VEN-001 `InstSeq` 为 64 B、按 64 对齐,字段偏移固定

- 规则:[§3](README.md#3-去重断裂与重同步)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/hft-venue/tests/layout.rs::ven_001_inst_seq_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| VEN-001.1 | `size_of::<InstSeq>()`;`align_of` | 64;64 |
| VEN-001.2 | `last_fwd` 偏移与长度 | 0;6 × 8 B |
| VEN-001.3 | `l2_state`/`resync_slot`/`_pad`/`resync_n`/`_pad2` 偏移 | 48 / 49 / 50 / 52 / 56 |

### VEN-002 `SnapReq` 为 16 B,字段偏移固定

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/hft-venue/tests/layout.rs::ven_002_snap_req_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| VEN-002.1 | `size_of::<SnapReq>()` | 16 |
| VEN-002.2 | `kind`/`_pad`/`attempt`/`inst`/`req_id`/`_pad2` 偏移 | 0 / 1 / 2 / 4 / 8 / 12 |
| VEN-002.3 | `kind` 码值 | `L2 = 1`、`Bbo = 2` |

### VEN-003 `AcctReq` 为 40 B,字段偏移与码值固定

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/hft-venue/tests/layout.rs::ven_003_acct_req_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| VEN-003.1 | `size_of::<AcctReq>()` | 40 |
| VEN-003.2 | `kind`/`account`/`why`/`session`/`seq_no`/`poll_no`/`_pad`/`session_uid` 偏移 | 0 / 1 / 2 / 3 / 4 / 8 / 12 / 16 |
| VEN-003.3 | `why` 码值 | `Scheduled = 1`、`MarginNow = 2`、`ConfigRecheck = 3`、`PushStale = 4`、`Startup = 5` |
| VEN-003.4 | `session` 码值 | 0 = 不带、1 = Ok、2 = Failed |
| VEN-003.5 | `kind` 码值 | `Config = 1`、`Margin = 2`(= hft-venue-proto `AcctQueryKind`) |

### VEN-004 `IdEntry` 为 64 B、按 64 对齐,字段偏移固定

- 规则:[§6](README.md#6-exch-id--slot-私有表idtable)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/hft-venue/tests/layout.rs::ven_004_id_entry_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| VEN-004.1 | `size_of::<IdEntry>()`;`align_of` | 64;64 |
| VEN-004.2 | `hash`/`term_tsc`/`slot`/`gen`/`epoch`/`key_len`/`flags`/`key` 偏移 | 0 / 8 / 16 / 20 / 24 / 26 / 27 / 28 |
| VEN-004.3 | `key` 长度;`TERMINAL` 位值 | 36;1 |

### VEN-005 `ConnState.reason` 码表与 gen-schema 导出一致

- 规则:[产生与消费的事件](README.md#产生与消费的事件)
- 级别:table
- 里程碑:M1.5
- 前置:无参数;读本 crate 的 `const` 码表与 `gen-schema` 导出的 JSON
- 实现:`crates/hft-venue/tests/codes.rs::ven_005_conn_reason_codes`

| 行 | 码 | 期望(名 / 适用状态) |
|---|---|---|
| VEN-005.1 | 1–4 | `GAP`/Resync、`DECODE`/Resync、`SNAPSHOT_FAIL`/Degraded、`SUB_REJECTED`/Degraded |
| VEN-005.2 | 5–8 | `RACE_SINGLE`/Degraded、`BBO_REFRESH`/Resync、`BBO_SILENT`/Degraded、`DEADMAN`/Degraded |
| VEN-005.3 | 9–12 | `CLOCK_SKEW`/Degraded、`INFLIGHT_TIMEOUT`/Degraded、`RATE_LIMIT`/Degraded、`REPORT_DECODE`/Degraded |
| VEN-005.4 | 13–17 | `MIRROR`/Degraded、`ACCT_QUERY`/Degraded、`CONN_LOST`/Resync、`NEW_SUB`/Resync、`REQUESTED`/Resync |
| VEN-005.5 | 0、18 | 码表中不存在;导出 JSON 恰好 17 项,名与码逐项等于 VEN-005.1–4 |

### VEN-006 ECF1 段 6 的 `conn` 行为 8 B,编码往返

- 规则:[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:行 `{venue: V7, rotate_margin_s: 3600}`
- 实现:`crates/hft-venue/tests/layout.rs::ven_006_ecf1_conn_row`

| 行 | 断言 | 期望 |
|---|---|---|
| VEN-006.1 | `size_of`;`venue`/`_pad`/`rotate_margin_s` 偏移 | 8;0 / 1 / 4 |
| VEN-006.2 | 编码字节 | `07 00 00 00 10 0E 00 00` |
| VEN-006.3 | 解码上一行的字节 | `{venue: V7, rotate_margin_s: 3600}` |

### VEN-007 进程内记录头的码值与写读往返

- 规则:[§3b](README.md#3b-快照获取snap-fetch)、[§4a](README.md#4a-账户查询保证金轮询与配置核验)、[运行时入口](README.md#运行时入口)
- 级别:table
- 里程碑:M3
- 前置:按各行构造记录头,经本 crate 的写函数写入字节环再由读函数读回
- 实现:`crates/hft-venue/tests/codes.rs::ven_007_inproc_headers`

| 行 | 输入 | 期望 |
|---|---|---|
| VEN-007.1 | `FeedRoutes` 三个值 | `Bbo = 1`、`Md = 2`、`All = 3` |
| VEN-007.2 | `SnapRecHdr.status` 七个值 | `Ok = 1`、`HttpErr = 2`、`Timeout = 3`、`Decode = 4`、`TooLarge = 5`、`RateLimited = 6`、`ReqErr = 7` |
| VEN-007.3 | `AcctRecHdr.what`;`fail` | `Raw = 1`、`Done = 2`、`Failed = 3`;`Timeout = 1`、`HttpErr = 2`、`Decode = 3`、`TooLarge = 4`、`RateLimited = 5` |
| VEN-007.4 | `SnapRecHdr{snap_id: u64::MAX, req_id: 7, inst: S1, kind: Bbo, status: Ok, http_status: 200, conn: 64, levels: 1, body_len: 300, parts_len: 96}` + 300 B 体 + 96 B 尾 | 读回的头逐字段相等,体与尾逐字节相等 |
| VEN-007.5 | `AcctRecHdr{what: Raw, part: 3, parts: 4, step: 2, req_len: 0, body_len: 16384, http_status: 200, sent_tsc: 1, conn: 80}` | 读回逐字段相等 |

### VEN-008 行情与下单热路径配置键的默认值

- 规则:[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`FeedCfg::default()` 与 `GwCfg::default()`,不设任何键
- 实现:`crates/hft-venue/tests/config.rs::ven_008_hot_defaults`

| 行 | 键 | 期望默认 |
|---|---|---|
| VEN-008.1 | `feed.bbo_conns`;`feed.bbo_frames_per_poll`;`feed.frames_per_poll` | 2;8;64 |
| VEN-008.2 | `feed.ready_poll`;`feed.bbo_ping_ms`;`feed.bbo_dead_ms`;`feed.bbo_refresh_grace_ms` | `auto`;1000;3000;500 |
| VEN-008.3 | `feed.depth`;`feed.topn_levels`;`feed.snapshot_part_levels` | `topn`;5;64 |
| VEN-008.4 | `feed.resync_slots`;`feed.resync_buf`;`feed.snapshot_backoff_ms` | 4;1024;`(100, 5000)` |
| VEN-008.5 | `feed.raw_dups`;`feed.raw.sample_ms`;`feed.ring_full_park_us` | false;1000;200 |
| VEN-008.6 | `conn.rotate_margin_s` | 3600 |
| VEN-008.7 | `gw.cmds_per_loop`;`gw.cmds_per_account_per_loop`;`gw.aux_every`;`gw.rest_idle_check_ms` | 16;8;8;1000 |
| VEN-008.8 | `gw.rotate_wait_max_ms`;`gw.warm_interval_ns`;`gw.inflight_timeout_ms`;`gw.stale_guard_ms` | 60000;0;3000;1500 |

### VEN-009 账户查询、id 表、暂存与 snap 配置键的默认值

- 规则:[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`GwCfg::default()`、`AcctCfg::default()`、`SnapCfg::default()`,不设任何键
- 实现:`crates/hft-venue/tests/config.rs::ven_009_warm_defaults`

| 行 | 键 | 期望默认 |
|---|---|---|
| VEN-009.1 | `snap.parallel` | 2 |
| VEN-009.2 | `gw.id_table_cap`;`gw.id_tomb_cap`;`gw.id_tomb_ms` | 16384;4096;120000 |
| VEN-009.3 | `gw.unmatched_hold_ms`;`gw.unmatched_hold_cap`;`gw.dedup_reseed_ms` | 5000;256;900000 |
| VEN-009.4 | `gw.report_decode_down_after`;`gw.unknown_evt_note_every` | 3;1000 |
| VEN-009.5 | `gw.margin_poll_min_ms`;`gw.margin_push_stale_ms`;`gw.acct_cfg_recheck_s` | 1000;5000;600 |
| VEN-009.6 | `gw.acct_query_timeout_ms`;`gw.acct_query_degraded_after`;`gw.acct_body_max` | 3000;3;262144 |
| VEN-009.7 | 常量 `ACCT_RAW_CHUNK`;`MAX_REST_ENDPOINTS`;`FEEDCMD_PER_POLL`;`FRAME_EVENTS_MAX`;`SNAP_BBO_BATCH_MAX` | 16384;4;8;64;32 |

### VEN-010 就绪探测 `auto` 只在 Spin 线程且计划连接数 ≥ 3 时生效

- 规则:[配置项](README.md#配置项)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:table
- 里程碑:M1.5
- 前置:调用 `ready_poll_on(mode, policy, thread_conns)`
- 实现:`crates/hft-venue/tests/config.rs::ven_010_ready_poll_on`

| 行 | `mode` / 线程 `IdlePolicy` / 该线程行情任务计划连接数之和 | 期望 |
|---|---|---|
| VEN-010.1 | `auto` / `Spin` / 3 | true |
| VEN-010.2 | `auto` / `Spin` / 2 | false |
| VEN-010.3 | `auto` / `Spin` / 4 | true |
| VEN-010.4 | `auto` / `SpinThenPark{spin_us: 50, park_us: 1000}` / 12 | false |
| VEN-010.5 | `auto` / `Park{park_us: 1000}` / 12 | false |
| VEN-010.6 | `off` / `Spin` / 12 | false |

### VEN-011 取值为 0 无意义的键为 0 时构造失败

- 规则:[配置项](README.md#配置项)、[运行时入口](README.md#运行时入口)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`/`G0`/`S0`,每行只改一个键
- 实现:`crates/hft-venue/tests/config.rs::ven_011_zero_keys`

| 行 | 键 = 值 | 期望 |
|---|---|---|
| VEN-011.1 | `feed.bbo_frames_per_poll = 0`;`= 1` | `Err(InitErr::Cfg("feed.bbo_frames_per_poll"))`;`Ok` |
| VEN-011.2 | `feed.frames_per_poll = 0`;`= 1` | `Err(InitErr::Cfg("feed.frames_per_poll"))`;`Ok` |
| VEN-011.3 | `feed.bbo_conns = 0`;`= 1` | `Err(InitErr::Cfg("feed.bbo_conns"))`;`Ok` |
| VEN-011.4 | `feed.bbo_ping_ms = 0`;`feed.bbo_dead_ms = 0` | 各 `Err(InitErr::Cfg(<键名>))` |
| VEN-011.5 | `snap.parallel = 0`;`= 1` | `Err(InitErr::Cfg("snap.parallel"))`;`Ok` |
| VEN-011.6 | `gw.cmds_per_loop = 0`;`gw.cmds_per_account_per_loop = 0`;`gw.aux_every = 0` | 各 `Err(InitErr::Cfg(<键名>))`;三者为 1 时 `Ok` |
| VEN-011.7 | `gw.unmatched_hold_cap = 0`;`gw.acct_query_degraded_after = 0`;`gw.report_decode_down_after = 0` | 各 `Err(InitErr::Cfg(<键名>))`;三者为 1 时 `Ok` |

### VEN-012 BBO 槽需求超过 `MAX_BBO_CONN_SLOTS` 时构造失败

- 规则:[配置项](README.md#配置项)、[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;`MAX_BBO_CONN_SLOTS = 8`;每行给出 `max_sub_insts`、生效 `ws_max_streams_per_conn`、`feed.bbo_conns`
- 实现:`crates/hft-venue/tests/config.rs::ven_012_bbo_slots`

| 行 | `max_sub_insts` / 流数 / `bbo_conns` | 期望(组数;`bbo_conns × 组数 + 1`) |
|---|---|---|
| VEN-012.1 | 300 / 100 / 2 | 3 组;7 → `Ok` |
| VEN-012.2 | 301 / 100 / 2 | 4 组;9 → `Err(InitErr::BboSlots)` |
| VEN-012.3 | 50 / 50 / 7 | 1 组;8 → `Ok`(边界) |
| VEN-012.4 | 50 / 50 / 8 | 1 组;9 → `Err(InitErr::BboSlots)` |
| VEN-012.5 | 51 / 50 / 2 | 2 组;5 → `Ok` |
| VEN-012.6 | 0 / 50 / 2 | 0 组;1 → `Ok`,运行中不建任何 BBO 连接 |

### VEN-013 环与 journal 的有无必须与任务路由一致

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[运行时入口](README.md#运行时入口)
- 级别:table
- 里程碑:M3
- 前置:`F0`;每行按所列给出 `FeedDeps` 中各项的有无
- 实现:`crates/hft-venue/tests/config.rs::ven_013_deps_match_routes`

| 行 | `routes` / 给出的项 | 期望 |
|---|---|---|
| VEN-013.1 | `Bbo` / `bbo`、`snapreq`(-bbo)、`snap`(-bbo)、raw(-bbo) | `Ok` |
| VEN-013.2 | `Bbo` / 同上另加 `md` | `Err(InitErr::DepsMismatch)` |
| VEN-013.3 | `Bbo` / 缺 `bbo` | `Err(InitErr::DepsMismatch)` |
| VEN-013.4 | `Md`(`feed.depth = topn`) / `md`、`bulk`、raw | `Ok` |
| VEN-013.5 | `Md`(`feed.depth = topn`) / 另加 `snapreq` | `Err(InitErr::DepsMismatch)` |
| VEN-013.6 | `Md` / 另加 `bbo` | `Err(InitErr::DepsMismatch)` |
| VEN-013.7 | `All` / `bbo`、`md`、`bulk`、`snapreq`、`snap`、raw | `Ok` |
| VEN-013.8 | `All` / 缺 `snapreq` | `Err(InitErr::DepsMismatch)` |

### VEN-014 `GwRuntime::new` 拒绝模拟账户与过小的帧容量

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[§4b](README.md#4b-私有频道归属与热备连接)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`;每行改账户集合或 `CAP`/`ORDER_FRAME_CAP`
- 实现:`crates/hft-venue/tests/config.rs::ven_014_gw_new_rejects`

| 行 | 输入 | 期望 |
|---|---|---|
| VEN-014.1 | 账户 [A1, A9] | `Err(InitErr::SimAccount)`;不分配 id 表、不读 raw-gw |
| VEN-014.2 | 账户 [A1, A2] | `Ok` |
| VEN-014.3 | `CAP = 2048`,`ORDER_FRAME_CAP = 8192`(`StubProto` 的批量变体) | `Err(InitErr::FrameCap)` |
| VEN-014.4 | `CAP = 8192`,`ORDER_FRAME_CAP = 8192` | `Ok`(边界) |
| VEN-014.5 | `CAP = 2048`,`ORDER_FRAME_CAP = 2047` | `Ok` |

### VEN-015 id 表容量、保证金轮询下限与 acct-fetch 的构造校验

- 规则:[配置项](README.md#配置项)、[§6](README.md#6-exch-id--slot-私有表idtable)、[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:table
- 里程碑:M3
- 前置:`G0`(OMS 槽数 2048)、`C0`;每行只改所列项
- 实现:`crates/hft-venue/tests/config.rs::ven_015_gw_new_limits`

| 行 | 输入 | 期望 |
|---|---|---|
| VEN-015.1 | `gw.id_table_cap = 16383` | `Err(InitErr::IdTableCap)`(不是 2 的幂) |
| VEN-015.2 | `gw.id_table_cap = 8192`(< 2 × (2048 + 4096) = 12288) | `Err(InitErr::IdTableCap)` |
| VEN-015.3 | `gw.id_table_cap = 16384` | `Ok` |
| VEN-015.4 | `gw.id_tomb_cap = 6144`,`gw.id_table_cap = 16384`(= 2 × 8192) | `Ok`(边界) |
| VEN-015.5 | `gw.id_tomb_cap = 6145`,`gw.id_table_cap = 16384` | `Err(InitErr::IdTableCap)` |
| VEN-015.6 | 生效 `margin_data = Poll{interval_ms: 999}`,`gw.margin_poll_min_ms = 1000` | `Err(InitErr::MarginPoll)` |
| VEN-015.7 | `Poll{interval_ms: 1000}`;`Poll{interval_ms: 1001}` | `Ok`;`Ok` |
| VEN-015.8 | `AcctFetchTask::new` 账户 [A1, A9] | `Err(InitErr::SimAccount)` |

### VEN-016 计划轮换余量不小于交易所强断时长时构造失败

- 规则:[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`F0`、`G0`;每行给出生效 `ws_max_conn_age_s` 与 `conn.rotate_margin_s`;对 `FeedRuntime`、`SnapFetchTask`、`GwRuntime`、`AcctFetchTask` 各构造一次
- 实现:`crates/hft-venue/tests/config.rs::ven_016_rotate_margin`

| 行 | `ws_max_conn_age_s` / `rotate_margin_s` | 期望(四个构造器相同) |
|---|---|---|
| VEN-016.1 | 3600 / 3600 | `Err(InitErr::RotateMargin)` |
| VEN-016.2 | 3601 / 3600 | `Ok` |
| VEN-016.3 | 3599 / 3600 | `Err(InitErr::RotateMargin)` |
| VEN-016.4 | 0 / 3600 | `Ok`(不做计划轮换) |

### VEN-017 订阅帧节奏不足 1 帧/s 时构造失败

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)、[§3a](README.md#3a-bbo-连接健康)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`;每行给出生效 `ws_max_in_msgs_per_s` 与 `feed.bbo_ping_ms`;BBO 连接节奏 = ⌊`ws_max_in_msgs_per_s` / 2⌋ − ⌈1000 / `bbo_ping_ms`⌉,其余连接 = ⌊`ws_max_in_msgs_per_s` / 2⌋
- 实现:`crates/hft-venue/tests/config.rs::ven_017_sub_rate`

| 行 | `ws_max_in_msgs_per_s` / `bbo_ping_ms` / `routes` | 期望 |
|---|---|---|
| VEN-017.1 | 4 / 1000 / `Bbo` | 节奏 1 → `Ok` |
| VEN-017.2 | 3 / 1000 / `Bbo` | 节奏 0 → `Err(InitErr::SubRate)` |
| VEN-017.3 | 5 / 1000 / `Bbo` | 节奏 1 → `Ok` |
| VEN-017.4 | 10 / 250 / `Bbo` | 5 − 4 = 1 → `Ok` |
| VEN-017.5 | 10 / 249 / `Bbo` | 5 − 5 = 0 → `Err(InitErr::SubRate)` |
| VEN-017.6 | 2 / 1000 / `Md` | 节奏 1 → `Ok`(不减探测) |
| VEN-017.7 | 1 / 1000 / `Md` | 节奏 0 → `Err(InitErr::SubRate)` |

### VEN-018 运行时只读 refdata 生效值,不读协议默认数值

- 规则:[协议接口](README.md#协议接口)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`、`G0`;`StubProto::default_caps()` 给 `margin_data = Poll{interval_ms: 5000}`,`routes()` 的 Bbo 路由 `max_streams = 200`、`max_in_msgs_per_s = 20`;refdata 生效值为 `Poll{interval_ms: 2000}`、`ws_max_streams_per_conn = 50`、`ws_max_in_msgs_per_s = 10`
- 步骤:
  1. 订阅 `P001`–`P051` 的 `BBO`,完成建连;gw 运行 10 s
- 期望:
  - BBO 连接分 2 组(按 50 拆分,不是 200);订阅帧节奏按 10 条/s 计算(VEN-017 的公式);Margin 序列间隔 2 s
  - `CallLog` 中 `default_caps` 调用数为 0;`routes()` 只用于取路由种类与档位
- 实现:`crates/hft-venue/tests/config.rs::ven_018_refdata_effective_values`

### VEN-020 `routes = Bbo` 的任务只建 Bbo 路由、只写 bbo 环与 `raw-feed-<venue>-bbo`

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:integ
- 里程碑:M3
- 前置:`F0`,`routes = Bbo`;deps 按 VEN-013.1
- 步骤:
  1. 投递 `FeedCmd::Subscribe{inst: S1, streams: BBO | BOOK | TRADE | MARK}`,`poll_once` 一次
  2. `BuildStub` 完成全部 `BuildReq`;各连接回全部 `ACK`
  3. 对每条连接 `push` `B|S1|…`、`T|S1|…`、`D|S1|…` 各一条,`poll_once` 一次
- 期望:
  - 步骤 1 后 `BuildReq` 恰好 2 条,`conn` 为 0 与 1,路由都是 Bbo(R1);没有连接号 ≥ 8 的请求
  - 步骤 2 后各连接发出的订阅帧只含 `S1` 的 Bbo 频道
  - 步骤 3 后 bbo 环有 1 条 `Bbo{inst: S1}`(另一条为重复帧被丢),另有连接级 `ConnState{Up}` 2 条与 S1 的成员 `Up` 2 条;md 环与 bulk 环为空;`T`、`D` 行按"本路由不认识的类"计数丢弃;raw 记录只出现在 `raw-feed-V7-bbo`
- 实现:`crates/hft-venue/tests/feed_routes.rs::ven_020_bbo_task_only_bbo`

### VEN-021 `routes = Md` 的任务只建深度与 Md 路由、只写 md 环与 `raw-feed-<venue>`

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:integ
- 里程碑:M3
- 前置:`F0`,`routes = Md`;deps 按 VEN-013.4
- 步骤:
  1. 投递 `Subscribe{S1, BBO | BOOK | TRADE | MARK}`,`poll_once`
  2. 完成全部 `BuildReq`,回 `ACK`;推 `D|S1|10|5|…`、`T|S1|77|…`、`M|S1|1000|…`、`B|S1|…`,`poll_once`
- 期望:
  - `BuildReq` 恰好 2 条:DepthTopN(R2)与 Md(R3),连接号都在 8–63;没有连接号 < 8 的请求
  - md 环依次为 `Bulk{L2Snapshot}`、`Trade`、`MarkPrice`(及 `ConnState`);bbo 环为空;`B` 行计数丢弃;raw 记录只出现在 `raw-feed-V7`
- 实现:`crates/hft-venue/tests/feed_routes.rs::ven_021_md_task_only_md`

### VEN-022 同一行情输入在拆分与 `All` 两种形态下 bbo 环与 md 环序列各自相同

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:prop
- 里程碑:M3
- 性质:同一脚本(订阅命令 + 各连接上的帧 + 断开 + 时钟推进)分别喂给 `Bbo + Md` 两个任务与单个 `All` 任务后,两种形态的 bbo 环元素序列逐字节相同,md 环与 bulk 环的元素序列逐字节相同
- 生成:3 个 inst;每 inst 的 `streams` 从 {BBO, BOOK, TRADE, MARK, FUNDING} 的子集抽取;0–400 帧,类别随机、`u`/序号单调或重复(30%)、每条连接 0–2 次 `eof`;`bbo_frames_per_poll` 取 1–8,`frames_per_poll` 取 1–64;各连接帧量保证每帧在两种形态下都在同一 `now_tsc` 前被读到
- 参考:逐元素比较两种形态的输出
- 例数:PR 256
- 实现:`crates/hft-venue/tests/feed_routes.rs::ven_022_split_equals_all`

### VEN-023 `FeedCmd` 的分发目标

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:table
- 里程碑:M3
- 前置:`MAX_BBO_CONN_SLOTS = 8`;调用 `feed_cmd_dest(&cmd)`(拆分形态;`All` 形态只有一个 feedctl 环,不调用)
- 实现:`crates/hft-venue/tests/feed_cmd.rs::ven_023_feed_cmd_dest`

| 行 | 命令 | 期望 |
|---|---|---|
| VEN-023.1 | `Resync{S1}` | `Md` |
| VEN-023.2 | `Rotate{conn: 7}` | `Bbo` |
| VEN-023.3 | `Rotate{conn: 8}` | `Md` |
| VEN-023.4 | `Subscribe{S1, BBO}`;`Subscribe{S1, TRADE}` | `Both`;`Both` |
| VEN-023.5 | `Unsubscribe{S1}` | `Both` |

### VEN-024 任务丢弃不属于它的命令并计数

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Bbo`,BBO 连接 0、1 已 Up 且订阅了 S1
- 步骤:
  1. 投递 `Resync{S1}`、`Rotate{conn: 9}`,`poll_once`
- 期望:
  - bbo 环无新元素;没有 `BuildReq`;计数 `cmd_ignored = 2`;连接 0、1 不受影响
- 实现:`crates/hft-venue/tests/feed_cmd.rs::ven_024_foreign_cmd_ignored`

### VEN-025 `Subscribe.streams` 的每一位落到对应路由与流

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:table
- 里程碑:M3
- 前置:`F0`,`routes = All`;每行对 S1 投递一条 `Subscribe{S1, streams}`,完成建连后读 `CallLog` 中 `subscribe` 收到的 `SubSpec`
- 实现:`crates/hft-venue/tests/feed_cmd.rs::ven_025_stream_bits`

| 行 | `streams`(SDK `EvMask` 位值) | 期望 `SubSpec` 集合 |
|---|---|---|
| VEN-025.1 | `BBO`(1) | `{S1, Bbo, levels 0, R1}` |
| VEN-025.2 | `BOOK`(2) | `{S1, DepthTopN, levels 5, R2}` |
| VEN-025.3 | `TRADE`(4) | `{S1, Trade, 0, R3}` |
| VEN-025.4 | `FUNDING`(8) | `{S1, Funding, 0, R3}` |
| VEN-025.5 | `MARK`(16) | `{S1, Mark, 0, R3}` |
| VEN-025.6 | `INDEX`(32) | `{S1, Index, 0, R3}` |
| VEN-025.7 | `LIQUIDATION`(64) | `{S1, Liquidation, 0, R3}` |
| VEN-025.8 | `BBO \| TRADE`(5) | 上表 .1 与 .3 两项;无任何深度流 |
| VEN-025.9 | 0 | 无任何 `SubSpec`;无 `BuildReq` |

### VEN-026 重发的 `Subscribe` 按流集合差分订阅与退订

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[§3a](README.md#3a-bbo-连接健康)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = All`;S1 已按 `BBO | TRADE | BOOK` 订阅,全部回执已齐,两条 BBO 连接上 S1 是成员
- 步骤:
  1. 投递 `Subscribe{S1, BBO | TRADE | BOOK}`(相同集合),`poll_once`
  2. 投递 `Subscribe{S1, BBO | MARK}`,`poll_once`,对退订与订阅帧回 `ACK`
  3. 投递 `Unsubscribe{S1}`,`poll_once`,回 `ACK`
- 期望:
  - 步骤 1 后没有任何订阅或退订帧
  - 步骤 2 后发出 Trade 与 DepthTopN 的退订帧、Mark 的订阅帧,Bbo 不动;此后深度连接上的 `D|S1|…` 按未订阅符号计数丢弃,md 环不再出现 S1 的 `L2Snapshot`
  - 步骤 3 后发出 Bbo 与 Mark 的退订帧;回执后 bbo 环恰好两条成员事件 `ConnState{Down, role=Md, conn ∈ {0, 1}, inst: S1}`
- 实现:`crates/hft-venue/tests/feed_cmd.rs::ven_026_subscribe_diff`

### VEN-027 订阅本 venue 不存在的 inst 被计数丢弃

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:unit
- 里程碑:M3
- 前置:`F0`;inst 9999 不属于 V7
- 步骤:
  1. 投递 `Subscribe{inst: 9999, streams: BBO}`,`poll_once`
- 期望:
  - 无 `BuildReq`、无订阅帧、bbo/md 环无元素;计数 `cmd_bad_inst = 1`
- 实现:`crates/hft-venue/tests/feed_cmd.rs::ven_027_unknown_inst`

### VEN-028 分档快照档数按 `feed.topn_levels` 选择

- 规则:[配置项](README.md#配置项)、[§3c](README.md#3c-分档快照depthtopn)
- 级别:table
- 里程碑:M3
- 前置:`F0`;`StubProto` 的 DepthTopN 路由档位如列;对 S1 订阅 `BOOK`
- 实现:`crates/hft-venue/tests/topn.rs::ven_028_topn_level_pick`

| 行 | 路由档位 / `feed.topn_levels` | 期望 `SubSpec.levels` |
|---|---|---|
| VEN-028.1 | [5, 20] / 5 | 5 |
| VEN-028.2 | [5, 20] / 4 | 5 |
| VEN-028.3 | [5, 20] / 6 | 20 |
| VEN-028.4 | [5, 20] / 20 | 20 |
| VEN-028.5 | [5, 20] / 21 | 20(都小于设定时取最大档) |
| VEN-028.6 | [10] / 5 | 10 |

### VEN-030 BBO 连接取最小空闲槽,连接号即槽号

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;`max_sub_insts = 100`;生效 `ws_max_streams_per_conn = 50`
- 步骤:
  1. 依次订阅 `P001`–`P051`(各 `BBO`),`poll_once` 直到不再产生 `BuildReq`
- 期望:
  - `BuildReq` 依次为 `conn = 0, 1`(组 1)与 `2, 3`(组 2);组 1 的两条连接订阅 `P001`–`P050`,组 2 的两条只订阅 `P051`
- 实现:`crates/hft-venue/tests/conn_id.rs::ven_030_bbo_slot_smallest_free`

### VEN-031 BBO 槽只在旧连接的 `Down{inst = 0}` 推出之后复用

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[§3a](README.md#3a-bbo-连接健康)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;连接 0、1 已 Up,S1 是两者的成员
- 步骤:
  1. `ScriptNet.rst(0)`,`poll_once`
  2. `BuildStub` 完成新请求,回 `ACK`,推一帧 `B|S1|…`
- 期望:
  - 步骤 1 的同一次 `poll_once` 内:bbo 环依次出现 `ConnState{Down, role=Md, conn: 0, inst: 0, reason: RESET 位}`,之后新 `BuildReq{conn: 0}`(`RetireMsg` 先于 `BuildReq`)
  - 步骤 2 后新连接 0 的 `ConnState{Up, conn: 0, inst: 0}` 在 bbo 环中位于步骤 1 的 `Down{conn: 0}` 之后;bbo 环中不存在 `Down` 之前出现的、来自新连接的任何记录
- 实现:`crates/hft-venue/tests/conn_id.rs::ven_031_slot_reuse_after_down`

### VEN-032 建连失败释放连接号,该号可直接复用

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;连接 0 已 Up;池需要第二条
- 步骤:
  1. `poll_once` 得 `BuildReq{conn: 1}`;`BuildStub.fail` 该请求;`poll_once`
- 期望:
  - 第二次 `poll_once` 发出新的 `BuildReq{conn: 1}`;bbo 环中没有 `conn = 1` 的任何记录(含 `Down`)
- 实现:`crates/hft-venue/tests/conn_id.rs::ven_032_build_fail_releases_id`

### VEN-033 任意故障序列下 BBO 连接数与槽号不越界,同槽记录按 Up/Down 交替

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:prop
- 里程碑:M1.5
- 性质:任一时刻同一 venue 的 BBO 连接(活跃、在建、待退役)≤ `MAX_BBO_CONN_SLOTS`(8);全部 BBO 记录的 `conn` < 8;对每个槽,bbo 环中连接级记录按 `Up, …, Down, Up, …` 交替,不出现两个 `Up` 之间没有 `Down`
- 生成:`feed.bbo_conns` 取 1–3;订阅 1–150 个 inst(`ws_max_streams_per_conn = 50`,满足 VEN-012 的约束);10⁴ 步,每步从 {`eof`/`rst` 任一连接、建连失败、建连成功、`BYE`、时钟推进 0–4000 ms 使计划轮换或静默触发、`Rotate{conn}`} 抽取
- 参考:逐步统计 `BuildStub` 上未完成的请求 + 活跃连接 + 已退役未回收的连接
- 例数:PR 256
- 实现:`crates/hft-venue/tests/conn_id.rs::ven_033_bbo_slots_bounded`

### VEN-034 非 BBO 连接号在各自号段内循环分配并跳过在用号

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:table
- 里程碑:M3
- 前置:按行构造对应运行时;让该号段除所列在用号外全部用过一遍后再申请
- 实现:`crates/hft-venue/tests/conn_id.rs::ven_034_conn_id_ranges`

| 行 | 号段归属 / 在用号 | 期望 |
|---|---|---|
| VEN-034.1 | feed 深度与 Md 连接(8–63)/ 首次 | 第一条为 8 |
| VEN-034.2 | 同上 / 已分配到 63,8 在用 | 下一条为 9 |
| VEN-034.3 | snap-fetch(64–79)/ 已分配到 79,64 空闲 | 下一条为 64 |
| VEN-034.4 | acct-fetch(80–111)/ 首次 | 80 |
| VEN-034.5 | gw(112–255)/ 已分配到 255,112–115 在用 | 下一条为 116 |

### VEN-035 warm 任务的连接复用既有出口地址

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:table
- 里程碑:M3
- 前置:hft-core 出口表 `src_ips = [10.0.0.11, 10.0.0.12, 10.0.0.13]`;`venues.V7.feed.conns = [{stream: bbo, src_ip: 10.0.0.11}, {stream: md, src_ip: 10.0.0.12}, {stream: md, src_ip: 10.0.0.13}]`;A1 的 `venues.V7.gw.src_ip = 10.0.0.13`
- 实现:`crates/hft-venue/tests/conn_id.rs::ven_035_warm_src_ip`

| 行 | 请求方 | 期望 `BuildReq.src_ip_idx` |
|---|---|---|
| VEN-035.1 | snap-fetch 的 REST 连接 | 1(第一条 `stream = md` 的地址) |
| VEN-035.2 | acct-fetch 为 A1 建的 REST 连接 | 2(A1 的 `gw.src_ip`) |

### VEN-036 热任务生产到 warm 线程的环不产生唤醒系统调用

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:integ
- 里程碑:M3
- 前置:`F0`(`routes = Bbo`)与 `G0`,各在 `IdlePolicy::Spin` 的线程上;`snapreq`、`acctreq`、`RetireMsg`、`BuildReq` 的生产端 `wake = None`
- 步骤:
  1. 触发 100 次 BBO 补帧请求、100 个账户查询序列、100 次连接退役与补建
- 期望:
  - feed 与 gw 线程上 `futex` 与对 eventfd 的 `write` 调用数均为 0;snap-fetch、acct-fetch 与 `BuildStub` 在各自下一次被调用时取到全部元素
- 实现:`crates/hft-venue/tests/conn_id.rs::ven_036_no_wake_to_warm`


### VEN-040 每次 `poll_once` 至多取 `FEEDCMD_PER_POLL` 条命令

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`;`FEEDCMD_PER_POLL = 8`;feedctl 环中有 12 条 `Subscribe{P001…P012, BBO}`
- 步骤:
  1. `poll_once` 一次,读 `LoopStat`
  2. 再 `poll_once` 一次
- 期望:
  - 步骤 1:`LoopStat.cmds = 8`、`did_work = true`,feedctl 环余 4 条
  - 步骤 2:`cmds = 4`,feedctl 环为空
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_040_cmds_per_poll`

### VEN-041 就绪探测生效时每轮一次 `epoll_pwait(0)`,只读就绪连接

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`ready = true`,`feed.bbo_conns = 6`;6 条 BBO 连接(0–5)已 Up;`set_ready` 只对 1 与 4 为真,二者各有 1 帧
- 步骤:
  1. `poll_once` 一次
- 期望:
  - `SysCount`:`epoll_pwait` 恰好 1 次且 `timeout = 0`;连接 1、4 各 1 次 `recvmsg`;连接 0、2、3、5 的 `recvmsg` 为 0
  - 连接 0、2、3、5 各收到一次 `note_empty(t_probe)`,`t_probe` = 该次探测调用之前读的 TSC
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_041_ready_probe_reads_ready_only`

### VEN-042 未就绪连接的确认点决定下一帧的 `rx_empty_gap_ns`

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`ready = true`;连接 2 已 Up、无数据
- 步骤:
  1. `now = 1_000_000`,探测前读到的 TSC 为 1 000 000,连接 2 不就绪,`poll_once`
  2. 对端在 1 200 000 发 `B|S1|11|…`;`now = 1_250_000`,连接 2 就绪,`poll_once`(该次 `recvmsg` 的 `rx_tsc = 1 250 000`)
- 期望:
  - 步骤 2 转发的 `Bbo.h.rx_empty_gap_ns = 250000`(按 [hft-net §7](../hft-net/README.md#7-时间戳) 由步骤 1 的确认点算出);`CallLog` 中 `decode_md` 收到的 `RxMeta.rx_empty_gap_ns = 250000`
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_042_note_empty_gap`

### VEN-043 上一轮留下完整帧的连接即使不就绪也被处理

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`ready = true`,`feed.bbo_frames_per_poll = 8`;连接 0 一次读入 12 帧(`u` = 1–12),连接 1 无数据
- 步骤:
  1. `poll_once`(连接 0 就绪)
  2. `poll_once`(连接 0 不就绪)
- 期望:
  - 步骤 1 转发 `u` = 1–8;步骤 2 转发 `u` = 9–12,且步骤 2 中连接 0 的 `recvmsg` 次数为 0
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_043_leftover_frames_without_ready`

### VEN-044 陈旧的就绪事件只多一次空读

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`ready = true`;`set_ready(1, true)` 但连接 1 没有数据
- 步骤:
  1. `poll_once`
- 期望:
  - 连接 1 恰好 1 次 `recvmsg`(返回 EAGAIN);bbo 环无新元素;`LoopStat.did_work = false`
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_044_spurious_ready_one_read`

### VEN-045 就绪探测不生效时每条活跃连接每轮读一次、不调 `epoll_pwait`

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`ready = false`,4 条 BBO 连接已 Up,其中连接 2 有 300 B 数据(短读)
- 步骤:
  1. `poll_once` 一次
- 期望:
  - `SysCount`:`epoll_pwait` 为 0;每条连接恰好 1 次 `recvmsg`(连接 2 短读后不再补一次空读)
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_045_no_ready_poll_all_once`

### VEN-046 feed 任务的系统调用只有 `recvmsg`、控制帧 `sendmsg` 与至多一次 `epoll_pwait(0)`

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:integ
- 里程碑:M1.5
- 前置:`F0`,`routes = All`,`ready = true`;S1–S3 订阅 `BBO | BOOK | TRADE`;每条连接每 1 ms 一帧,每 1000 ms 回 Pong
- 步骤:
  1. 运行 10⁴ 次 `poll_once`,时钟每次推进 100 µs
- 期望:
  - `SysCount` 只出现 `recvmsg`、`sendmsg`、`epoll_pwait` 三类;每次 `poll_once` 的 `epoll_pwait` ≤ 1 且 `timeout = 0`;`epoll_ctl` 为 0;`sendmsg` 的帧全部是 Ping、Pong 或订阅帧;任务不持有任何 `RestPool`
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_046_feed_syscalls`

### VEN-050 BBO 连接之间按帧轮转,每连接每轮至多 `bbo_frames_per_poll` 帧,起点每轮轮换

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.bbo_frames_per_poll = 8`;连接 0、1 各积压 20 帧(`c0f1…c0f20`、`c1f1…c1f20`)
- 步骤:
  1. `poll_once` 三次,读 `CallLog` 中 `classify` 的调用顺序(连接号、帧号)
- 期望:
  - 第 1 次 `poll_once`:`c0f1, c1f1, c0f2, c1f2, …, c0f8, c1f8`(16 帧)
  - 第 2 次 `poll_once`:`c1f9, c0f9, c1f10, c0f10, …, c1f16, c0f16`(16 帧,起点为连接 1)
  - 第 3 次 `poll_once`:`c0f17, c1f17, …, c0f20, c1f20`(8 帧)
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_050_bbo_frame_round_robin`

### VEN-051 只有一条连接积压时每轮只处理该连接的 `bbo_frames_per_poll` 帧

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.bbo_frames_per_poll = 8`;连接 0 积压 20 帧,连接 1 无数据
- 步骤:
  1. `poll_once` 一次
- 期望:
  - 处理 8 帧(`c0f1`–`c0f8`);`LoopStat.frames = 8`;连接 0 余 12 帧
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_051_single_backlog_cap`

### VEN-052 三条 BBO 连接时轮转起点依次为 0、1、2、0

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.bbo_conns = 3`,`feed.bbo_frames_per_poll = 1`;连接 0–2 各积压 10 帧
- 步骤:
  1. `poll_once` 四次
- 期望:
  - 四轮处理的第一帧依次来自连接 0、1、2、0;每轮恰好 3 帧
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_052_start_rotates`

### VEN-053 同线程多个 feed-bbo 任务时新到 BBO 帧的等待帧数不超过上界

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:prop
- 里程碑:M1.5
- 性质:一个 `TaskLoop` 承载两个 `routes = Bbo` 的任务 A、B(各 2 条 BBO 连接);任一时刻在 B 的连接 1 上新到的一帧,从到达到被处理之间处理的其他帧数 ≤ A 的 2 × `bbo_frames_per_poll` + B 的另一条连接 1 帧 + 同一连接上排在它前面的帧数
- 生成:`feed.bbo_frames_per_poll` 取 1–8;每条连接初始积压 0–64 帧;目标帧的到达时刻随机落在某一轮之前
- 参考:按插桩计数(每处理一帧加 1)与上式比较
- 例数:PR 512
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_053_bbo_wait_bound`

### VEN-054 非 Bbo 路由逐连接处理,每连接每轮至多 `frames_per_poll` 帧

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Md`,`feed.frames_per_poll = 64`;深度连接(8)与 Md 连接(9)各积压 100 帧
- 步骤:
  1. `poll_once` 一次
- 期望:
  - `classify` 调用顺序为连接 8 的 64 帧,然后连接 9 的 64 帧(不交错);两条连接各余 36 帧
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_054_md_conn_by_conn`

### VEN-055 `routes = All` 时同一轮先做 Bbo 路由再做其余路由,各自不超过上限

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = All`,`feed.bbo_frames_per_poll = 8`,`feed.frames_per_poll = 64`;BBO 连接 0、1 各 10 帧,深度连接 8 与 Md 连接 9 各 100 帧
- 步骤:
  1. `poll_once` 一次
- 期望:
  - 先处理 16 帧 BBO(按帧轮转),再处理连接 8 的 64 帧、连接 9 的 64 帧;`LoopStat.frames = 144`
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_055_all_routes_order`

### VEN-056 每连接帧数上限的边界

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`feed.bbo_frames_per_poll = 8`,`feed.frames_per_poll = 64`;每行只有一条连接有积压,`poll_once` 一次
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_056_frame_cap_boundary`

| 行 | 连接 / 积压帧数 | 期望(这一轮处理 / 余) |
|---|---|---|
| VEN-056.1 | BBO 连接 0 / 7 | 7 / 0 |
| VEN-056.2 | BBO 连接 0 / 8 | 8 / 0 |
| VEN-056.3 | BBO 连接 0 / 9 | 8 / 1 |
| VEN-056.4 | 深度连接 8 / 63 | 63 / 0 |
| VEN-056.5 | 深度连接 8 / 64 | 64 / 0 |
| VEN-056.6 | 深度连接 8 / 65 | 64 / 1 |

### VEN-060 控制帧不产生行情事件,订阅回执在任何 raw 模式下都被记录

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)、[配置项](README.md#配置项)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.raw.bbo = off`;连接 0 已 Up,S1 的订阅帧已发
- 步骤:
  1. 推 `ACK|1|1|0`,`poll_once`
- 期望:
  - bbo 环只多一条成员事件 `ConnState{Up, conn: 0, inst: S1}`,没有 `Bbo`;`raw-feed-V7-bbo` 多一条 `RawRx{what: SubAck, conn: 0}`,无数据帧记录
- 实现:`crates/hft-venue/tests/feed_frame.rs::ven_060_control_frames`

### VEN-061 订阅被拒推 `Degraded{SUB_REJECTED}`

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;连接 0 已 Up,S1 的订阅 id 为 1
- 步骤:
  1. 推 `ACK|1|0|1121`,`poll_once`
- 期望:
  - bbo 环恰好一条 `ConnState{Degraded, role: Md, conn: 0, inst: S1, reason: 4}`;没有 S1 的成员 `Up`
- 实现:`crates/hft-venue/tests/feed_frame.rs::ven_061_sub_rejected`

### VEN-062 未订阅符号计数丢弃,raw 照写

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.raw.bbo = on`;只订阅 S1
- 步骤:
  1. 推 `B|ZZZ|5|…`,`poll_once`
- 期望:
  - 无 `Bbo`;计数 `unsubscribed = 1`;`decode_md` 未被调用;raw 多一条 `RawRx{what: WsText}`
- 实现:`crates/hft-venue/tests/feed_frame.rs::ven_062_unsubscribed_symbol`

### VEN-063 行情解码失败不 panic、计数,BBO 类不推 Resync

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.raw.bbo = on`;S1 已订阅,`last_fwd[BBO] = 10`;每行推该帧后再推 `B|S1|11|…`,`poll_once`
- 实现:`crates/hft-venue/tests/feed_frame.rs::ven_063_md_decode_errors`

| 行 | 坏帧 | 期望 |
|---|---|---|
| VEN-063.1 | `B\|S1\|` (截断) | `decode_err{Bbo} += 1`;无 `ConnState`;raw 照写;随后的 `u = 11` 被转发 |
| VEN-063.2 | `B\|S1\|12\|1x.5\|…`(坏数字) | 同上;`last_fwd` 不因坏帧前移(仍转发 `u = 11`) |
| VEN-063.3 | `B\|S1\|12\|99999999999999999999\|…`(定点溢出) | 同上 |
| VEN-063.4 | 1 MiB 的单条帧(超出 `max_msg`) | 连接置 `RX_FULL`,按致命处理(同 VEN-075),不 panic |

### VEN-064 解码时传给 venue 的 `RxMeta` 等于该帧的 hft-net 戳

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;`ScriptNet` 给该帧的戳:`rx_tsc = 5000`、`hw_rx_ns = 4100`、`sw_rx_ns = 4500`、`rx_empty_gap_ns = 700`、`flags = HW | SW`;连接号 1
- 步骤:
  1. 推 `B|S1|20|…`,`poll_once`
- 期望:
  - `CallLog` 中 `decode_md` 的 `RxMeta = {rx_tsc: 5000, hw_rx_ns: 4100, sw_rx_ns: 4500, prev_override: 0, conn: 1, ts_flags: HW | SW, rx_empty_gap_ns: 700}`;转发的 `Bbo.h` 对应字段相同
- 实现:`crates/hft-venue/tests/feed_frame.rs::ven_064_rx_meta_passthrough`

### VEN-065 各类行情事件进对应的环

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)、[产生与消费的事件](README.md#产生与消费的事件)
- 级别:table
- 里程碑:M3
- 前置:`F0`,`routes = All`;S1 订阅 `BBO | BOOK | TRADE | MARK | INDEX | FUNDING | LIQUIDATION`,回执已齐
- 实现:`crates/hft-venue/tests/feed_frame.rs::ven_065_event_rings`

| 行 | 帧 | 期望 |
|---|---|---|
| VEN-065.1 | `B\|S1\|…` | bbo 环 `FeedMsg::Bbo` |
| VEN-065.2 | `T\|S1\|…` | md 环 `FeedMsg::Trade` |
| VEN-065.3 | `M\|S1\|…`;`I\|S1\|…`;`F\|S1\|…` | md 环 `MarkPrice`;`IndexPrice`;`Funding` |
| VEN-065.4 | 强平流帧 | md 环 `Liquidation` |
| VEN-065.5 | `D\|S1\|10\|5\|…` | md-bulk 一条 `L2Snapshot{origin: TopN}`,md 环随后一条 `FeedMsg::Bulk{kind: L2Snapshot, len}` |

### VEN-066 先入环、后写 raw,记录头取该帧的戳

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.raw.bbo = on`;帧戳 `rx_tsc = 9000`、`hw_rx_ns = 8800`;连接 0,S1 的订阅内部 id 为 3
- 步骤:
  1. 推 `B|S1|30|…`(42 B),`poll_once`
- 期望:
  - `Order` 日志中该帧的 `Bbo` 入环在 raw 追加之前
  - raw 记录:`RawHdr{venue: V7, conn: 0, what: WsText, stream: 3, hw_rx_ns: 8800, payload_len: 42}`,记录头 `tsc = 9000`,payload 与原帧逐字节相同
- 实现:`crates/hft-venue/tests/feed_frame.rs::ven_066_ring_then_raw`

### VEN-067 raw-feed 的三种模式

- 规则:[配置项](README.md#配置项)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:table
- 里程碑:M3
- 前置:`F0`,`routes = All`,`feed.raw.sample_ms = 1000`;S1 订阅 `BBO | TRADE`;在 0–2500 ms 内 S1 每 100 ms 一帧 Bbo 与一帧 Trade(各 25 帧),期间一次订阅回执、一条连接断开重连、一次 BBO 补帧响应
- 实现:`crates/hft-venue/tests/raw_modes.rs::ven_067_raw_modes`

| 行 | `feed.raw.bbo` / `feed.raw.md` | 期望 |
|---|---|---|
| VEN-067.1 | `on` / `on` | raw 中 Bbo 数据帧 25 条、Trade 25 条,均无 `SAMPLED` |
| VEN-067.2 | `sampled` / `on` | Bbo 数据帧 3 条(0 ms、1000 ms、2000 ms 各一),`rflags` 含 `SAMPLED`;Trade 25 条 |
| VEN-067.3 | `off` / `sampled` | Bbo 数据帧 0 条;Trade 3 条带 `SAMPLED` |
| VEN-067.4 | 三种模式各一次 | 订阅回执(`SubAck`)、连接与断开标记(`Connect`、`Disconnect`)、BBO 补帧响应(`RestResponse`)每种都恰好被记录,条数与 `on` 相同 |
| VEN-067.5 | `sampled` / `on`;S1 上一条被抽中的帧 `rx_tsc = t`,随后的帧在 `t + 999 ms`、`t + 1000 ms`、`t + 1001 ms` | 只有 `t + 1000 ms` 的帧被写(写入后 `t + 1001 ms` 的帧不写) |

### VEN-068 赛马重复帧只在 `feed.raw_dups` 打开且模式为 `on` 时写 raw

- 规则:[配置项](README.md#配置项)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;两条连接送达同一组 `u` = 1–10
- 实现:`crates/hft-venue/tests/raw_modes.rs::ven_068_raw_dups`

| 行 | `feed.raw.bbo` / `feed.raw_dups` | 期望 raw 数据帧条数 |
|---|---|---|
| VEN-068.1 | `on` / false | 10(只有被转发的帧) |
| VEN-068.2 | `on` / true | 20 |
| VEN-068.3 | `sampled` / true | 与 `sampled` / false 相同(`raw_dups` 不生效) |

### VEN-069 单帧事件数超过 `FRAME_EVENTS_MAX` 时多出的丢弃并计数

已删除:RG28 改为整帧按解码错误处理(不部分丢弃),见 VEN-087。

### VEN-070 bbo 环满:这一轮停读、不丢、不自旋,按空闲处理

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:integ
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.ring_full_park_us = 200`;bbo 环容量 4,已被占满且消费者暂停;连接 0 有 3 帧 `u` = 21–23
- 步骤:
  1. `now = 1_000_000`,`poll_once`
  2. 消费者取走 4 条;`now = 1_200_000`,`poll_once`
- 期望:
  - 步骤 1:无新 `Bbo`;`LoopStat{did_work: false, ring_full: 1, next_deadline: Some(1_200_000)}`;三帧仍在连接缓冲中
  - 步骤 2:依次转发 `u` = 21、22、23;与环容量 1024 时的同一输入相比,bbo 环元素序列逐条相同
- 实现:`crates/hft-venue/tests/ring_full.rs::ven_070_bbo_ring_full`

### VEN-071 环满期间 `has_pending` 不计 socket 中的待读数据

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[运行时入口](README.md#运行时入口)
- 级别:unit
- 里程碑:M1.5
- 前置:同 VEN-070 步骤 1 之后的状态
- 步骤:
  1. 读 `has_pending()`
  2. 向 feedctl 投递一条 `Subscribe`,读 `has_pending()`
  3. 取出该命令并让消费者腾出 4 个空位,读 `has_pending()`
- 期望:
  - 步骤 1 为 false;步骤 2 为 true;步骤 3 为 true(TLS 缓冲中有未处理的完整帧,环有空位)
- 实现:`crates/hft-venue/tests/ring_full.rs::ven_071_has_pending_ring_full`

### VEN-072 环满时 `SpinThenPark` 线程带超时 park,同线程其他任务照常被调用

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:integ
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,bbo 环满(同 VEN-070);hft-rt `TaskLoop` 承载本任务与一个计数任务 `Tick`,`IdlePolicy::SpinThenPark{spin_us: 10, park_us: 1000}`
- 步骤:
  1. 运行 100 轮,时钟每轮推进 1 µs;再推进到空闲满 10 µs 后调 `idle_action`
- 期望:
  - `Tick.poll_once` 被调用 100 次;feed 任务每轮 `did_work = false`
  - `idle_action` 返回 `Park{timeout_ns: 200000}`(= `next_deadline − now`,小于 `park_us`)
- 实现:`crates/hft-venue/tests/ring_full.rs::ven_072_ring_full_parks`

### VEN-073 md 环满只停读输出到 md 环的连接,BBO 照常

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = All`;md 环容量 4 已满;深度连接 8 有 5 帧、BBO 连接 0 有 3 帧
- 步骤:
  1. `poll_once`
- 期望:
  - bbo 环多 3 条 `Bbo`;md 环不变;连接 8 的 5 帧仍在缓冲;`LoopStat{did_work: true, ring_full: 1}`
- 实现:`crates/hft-venue/tests/ring_full.rs::ven_073_md_full_bbo_continues`

### VEN-074 bulk 环放不下下一片时按环满处理

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[§3c](README.md#3c-分档快照depthtopn)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Md`;md-bulk 环剩余空间小于一片 64 档 `L2Snapshot` 的记录长度;深度连接有 2 帧 `D|S1|…`
- 步骤:
  1. `poll_once`
  2. 消费者取走 bulk 中的记录;`poll_once`
- 期望:
  - 步骤 1:无新 bulk 记录、无 `FeedMsg::Bulk`;`ring_full = 1`;两帧仍在缓冲
  - 步骤 2:两条 `L2Snapshot` 与对应两条 `FeedMsg::Bulk` 按帧序写入
- 实现:`crates/hft-venue/tests/ring_full.rs::ven_074_bulk_full`

### VEN-075 致命 `ErrBits` 退役连接并推 `Down`,赛马池只剩一条推 `RACE_SINGLE`

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)、[§3a](README.md#3a-bbo-连接健康)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.bbo_conns = 2`;连接 0、1 已 Up
- 步骤:
  1. `ScriptNet.eof(1)`,`poll_once`
- 期望:
  - bbo 环依次:`ConnState{Down, role: Md, conn: 1, inst: 0, reason: PEER_EOF}`、`ConnState{Degraded, role: Md, conn: 0, inst: 0, reason: 5}`;`RetireMsg` 一条;同一轮发出补建 `BuildReq`
  - 之后连接 0 上的 `Bbo` 照常转发
- 实现:`crates/hft-venue/tests/feed_err.rs::ven_075_fatal_and_race_single`

### VEN-076 `feed.bbo_conns = 1` 时连接致命不推 `RACE_SINGLE`

- 规则:[§3a](README.md#3a-bbo-连接健康)、[配置项](README.md#配置项)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.bbo_conns = 1`;连接 0 已 Up
- 步骤:
  1. `ScriptNet.rst(0)`,`poll_once`
- 期望:
  - bbo 环只有 `ConnState{Down, conn: 0, inst: 0, reason: RESET}`;没有 `reason = 5` 的记录
- 实现:`crates/hft-venue/tests/feed_err.rs::ven_076_single_conn_no_race_single`

### VEN-077 非致命 `ErrBits` 不产生 `ConnState`、不退役

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;连接 0 已 Up
- 步骤:
  1. 依次对连接 0 置 `TX_PARTIAL`、`TS_MISSING`、`KEY_UPDATE`,各 `poll_once`
- 期望:
  - bbo 环无新 `ConnState`;无 `RetireMsg`;连接 0 仍活跃
- 实现:`crates/hft-venue/tests/feed_err.rs::ven_077_nonfatal_bits`

### VEN-078 深度连接断开不影响 bbo 环

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)、[§3c](README.md#3c-分档快照depthtopn)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = All`;S1、S2 订阅 `BBO | BOOK`
- 步骤:
  1. `ScriptNet.eof(8)`(深度连接),同时 BBO 连接 0 推 `B|S1|40|…`,`poll_once`
- 期望:
  - bbo 环:`Bbo{S1, u: 40}`,没有任何 `ConnState`;md 环:`Down{conn: 8, inst: 0}` 与 S1、S2 各一条 `Resync{reason: 15}`
- 实现:`crates/hft-venue/tests/feed_err.rs::ven_078_depth_drop_isolated`

### VEN-079 交接后推 `Up` 并以限速节奏发订阅帧

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)、[§3a](README.md#3a-bbo-连接健康)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;生效 `ws_max_in_msgs_per_s = 10`,`feed.bbo_ping_ms = 1000`(BBO 连接订阅节奏 4 帧/s);`StubProto::subscribe` 每帧装 10 个流;订阅 `P001`–`P050`
- 步骤:
  1. `t = 0` 完成连接 0 的交接,随后每 10 ms `poll_once` 一次,运行 3 s
- 期望:
  - `t = 0` 的那一轮 bbo 环有 `ConnState{Up, role: Md, conn: 0, inst: 0}`,先于任何订阅帧
  - 订阅帧共 5 帧,第 k 帧(k = 0…4)的发送时刻 ≥ k × 250 ms;任一 1000 ms 窗口内订阅帧 ≤ 4、订阅帧与探测帧之和 ≤ 5
- 实现:`crates/hft-venue/tests/feed_pool.rs::ven_079_install_up_and_paced_subs`

### VEN-080 计划轮换时刻 = Open + `ws_max_conn_age_s` − `rotate_margin_s`

- 规则:[配置项](README.md#配置项)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:table
- 里程碑:M3
- 前置:`F0`,`routes = Bbo`,`feed.bbo_conns = 1`;连接 0 在 `T0 = 10⁹` 交接;每行给出 `ws_max_conn_age_s` 与 `conn.rotate_margin_s`,在所列时刻 `poll_once`
- 实现:`crates/hft-venue/tests/feed_pool.rs::ven_080_rotate_time`

| 行 | 强断时长 / 余量 / 时刻 | 期望 |
|---|---|---|
| VEN-080.1 | 86400 s / 3600 s / `T0 + 82800 s − 1 ns` | 无替补 `BuildReq` |
| VEN-080.2 | 同上 / `T0 + 82800 s` | 一条 `BuildReq`(原因 `Rotate(0)`) |
| VEN-080.3 | 0 / 3600 s / `T0 + 10 天` | 无替补 `BuildReq` |

### VEN-081 `FeedCmd::Rotate{conn}` 立即开始替补

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[配置项](README.md#配置项)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Bbo`;连接 0、1 已 Up,计划轮换时刻在 10 h 后
- 步骤:
  1. 投递 `Rotate{conn: 1}`,`poll_once`
- 期望:
  - 这一轮发出一条替补 `BuildReq`(原因 `Rotate(1)`,连接号取最小空闲槽 2);连接 1 仍活跃,不推 `Down`
- 实现:`crates/hft-venue/tests/feed_pool.rs::ven_081_rotate_cmd`

### VEN-082 赛马池替补收到第一帧数据后退役最早安装的连接

- 规则:[§3a](README.md#3a-bbo-连接健康)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Bbo`,`feed.bbo_conns = 2`;连接 0(最早)、1 已 Up,S1 是两者成员;连接 0 到达计划轮换时刻
- 步骤:
  1. 完成替补连接 2 的交接,回 `ACK`,`poll_once`
  2. 连接 2 推 `B|S1|…`,`poll_once`
- 期望:
  - 步骤 1 后 S1 的成员 `Up` 来自连接 0、1、2(三条重叠);连接 0 仍活跃
  - 步骤 2 后 bbo 环:`ConnState{Down, conn: 0, inst: 0}`;S1 只剩连接 1、2 两个成员;没有 `RACE_SINGLE`
- 实现:`crates/hft-venue/tests/feed_pool.rs::ven_082_race_replacement_retires_oldest`

### VEN-083 服务端下线预告触发替补

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Bbo`;连接 0、1 已 Up
- 步骤:
  1. 连接 1 推 `BYE`,`poll_once`
- 期望:
  - 连接 1 被置 `SERVER_HINT`;这一轮发出替补 `BuildReq`(原因 `Hint(1)`);bbo 环没有行情事件也没有 `Down`
- 实现:`crates/hft-venue/tests/feed_pool.rs::ven_083_server_shutdown_hint`

### VEN-084 `LoopStat` 反映这一轮的工作量与最早期限

- 规则:[运行时入口](README.md#运行时入口)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;连接 0 已 Up,下次探测在 `t = 2_000_000_000`,`now = 1_500_000_000`
- 步骤:
  1. 推 3 帧、投递 2 条命令,`poll_once`
  2. 不给任何输入,`poll_once`
- 期望:
  - 步骤 1:`{did_work: true, frames: 3, cmds: 2, ring_full: 0}`
  - 步骤 2:`{did_work: false, frames: 0, cmds: 0, next_deadline: Some(2_000_000_000)}`
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_084_loop_stat`

### VEN-085 feed 的 `has_pending`

- 规则:[运行时入口](README.md#运行时入口)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = All`;各环有充足空位(除 .5 行)
- 实现:`crates/hft-venue/tests/feed_poll.rs::ven_085_has_pending`

| 行 | 状态 | 期望 |
|---|---|---|
| VEN-085.1 | feedctl、snap 环为空,连接 TLS 缓冲无未处理字节 | false |
| VEN-085.2 | feedctl 有 1 条命令 | true |
| VEN-085.3 | snap 环有 1 条记录 | true |
| VEN-085.4 | 连接 0 的 TLS 缓冲有一帧未处理 | true |
| VEN-085.5 | 同 .4,但 bbo 环已满 | false(VEN-071) |

### VEN-086 raw journal 写满时该条 raw 不写、入环照常

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Bbo`;raw-feed 写者被测试替身配置为下一次 `append` 返回 `AppendErr::Full`,`roll` 返回 `NextNotReady`
- 步骤:
  1. 连接 0 送来 `B|S1|101|…`、`B|S1|102|…`
- 期望:
  - bbo 环 2 条 `Bbo`(`update_id` 101、102);raw-feed 只有第 2 条帧的 `RawRx`;`raw_append_err{journal: raw-feed-V7-bbo, err: Full} = 1`
  - 无 `ConnState`;连接未退役;`poll_once` 返回 `did_work = true`
- 实现:`crates/hft-venue/tests/feed_raw.rs::ven_086_raw_full_drops_record_only`

### VEN-087 单帧事件数超过 `FRAME_EVENTS_MAX` 时整帧按解码错误处理

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:table
- 里程碑:M3
- 前置:`F0`,`routes = Md`;`FRAME_EVENTS_MAX = 64`;`StubProto` 的成交帧可携带多笔成交;之后同一连接再来一帧 1 笔成交
- 实现:`crates/hft-venue/tests/feed_frame.rs::ven_087_frame_events_over_is_decode_error`

| 行 | 一帧中的成交笔数 | 期望 |
|---|---|---|
| VEN-087.1 | 63 | md 环 63 条 `Trade`;`frame_events_over = 0` |
| VEN-087.2 | 64 | md 环 64 条 `Trade`;`frame_events_over = 0` |
| VEN-087.3 | 65 | 该帧 0 条 `Trade`;`frame_events_over = 1`,解码失败计数 1;下一帧的 1 条 `Trade` 照常入环 |
| VEN-087.4 | 65,且该帧属分档快照路由(65 档) | 0 条 `L2Snapshot`;推 `ConnState{Resync, inst, reason: DECODE(2)}` |

### VEN-100 `Monotonic` 规则:`last ≤ last_fwd` 丢,否则转发

- 规则:[§3](README.md#3-去重断裂与重同步)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;S1 的 `last_fwd[BBO] = 100`,不在补帧;按行顺序依次喂入
- 实现:`crates/hft-venue/tests/seq.rs::ven_100_monotonic`

| 行 | 帧的 `u` | 期望 |
|---|---|---|
| VEN-100.1 | 99 | 丢;`dup` 计数 +1 |
| VEN-100.2 | 100 | 丢 |
| VEN-100.3 | 101 | 转发;`last_fwd = 101` |
| VEN-100.4 | 105 | 转发(跳号不是断裂);`last_fwd = 105` |
| VEN-100.5 | 103 | 丢 |

### VEN-101 `ByTime` 规则按交易所时间去重

- 规则:[§3](README.md#3-去重断裂与重同步)
- 级别:table
- 里程碑:M3
- 前置:`F0`,`routes = Md`;S1 订阅 `MARK`,`last_fwd[Mark] = 1_000`(ms 转 ns 后为 10⁹)
- 实现:`crates/hft-venue/tests/seq.rs::ven_101_by_time`

| 行 | 帧 | 期望 |
|---|---|---|
| VEN-101.1 | `M\|S1\|999\|…` | 丢 |
| VEN-101.2 | `M\|S1\|1000\|…` | 丢 |
| VEN-101.3 | `M\|S1\|1001\|…` | 转发 `MarkPrice` |

### VEN-102 各流、各 inst 的去重状态互不影响

- 规则:[§3](README.md#3-去重断裂与重同步)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = All`;S1、S2 订阅 `BBO | TRADE`;`last_fwd` 全为 0
- 步骤:
  1. 依次喂 `B|S1|50`、`T|S1|50`、`B|S2|50`、`T|S1|49`、`B|S1|51`
- 期望:
  - 转发 `Bbo{S1, 50}`、`Trade{S1, tid 50}`、`Bbo{S2, 50}`、`Bbo{S1, 51}`;`T|S1|49` 被丢
- 实现:`crates/hft-venue/tests/seq.rs::ven_102_streams_independent`

### VEN-103 两路随机交错同一 BBO 序列时输出严格递增、无重复

- 规则:[§3](README.md#3-去重断裂与重同步)、[§3a](README.md#3a-bbo-连接健康)
- 级别:prop
- 里程碑:M1.5
- 性质:bbo 环输出的 `update_id` 严格递增、无重复,且等于"按到达顺序扫描、只保留大于当前最大值的 id"的参考序列;任一路的最后一个 id 若是全局最大,则它出现在输出中
- 生成:一条长度 1–2000 的严格递增 id 序列;两路各以 0–20% 概率丢帧、以 0–50 帧的随机延迟送达;可选一路在随机位置断开(之后只剩另一路)
- 参考:上述扫描模型
- 例数:PR 2 000
- 实现:`crates/hft-venue/tests/seq.rs::ven_103_bbo_race_dedup`

### VEN-104 赛马一路断开后输出不断、推一次 `RACE_SINGLE`

- 规则:[§3a](README.md#3a-bbo-连接健康)、[§3](README.md#3-去重断裂与重同步)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;连接 0、1 都送达 `u` = 1–5
- 步骤:
  1. `rst(0)`;连接 1 继续送达 `u` = 6–10,每到一帧 `poll_once` 一次
- 期望:
  - bbo 环 `Bbo` 的 `u` 为 1–10 连续;`ConnState{Degraded, reason: 5}` 恰好一条
- 实现:`crates/hft-venue/tests/seq.rs::ven_104_race_single_continues`

### VEN-105 增量深度:快照与缓冲增量按 `align_first` 对齐后连续转发

- 规则:[增量深度与快照重同步(可选扩展)](README.md#增量深度与快照重同步可选扩展)
- 级别:prop
- 里程碑:M7
- 状态:可选扩展:全量增量深度
- 性质:对任意"快照比缓冲旧 / 比缓冲新 / 恰好对齐 / 对齐后第一条即 `Gap`"的序列,输出依次为 `Resync` → `L2Snapshot` → 连续 `L2Batch`(首条 `prev_id = snap_id`)→ `Up`,或在 `Gap` 时退避重试;重连后必经 `Resync`
- 生成:快照序号与缓冲增量原始序号的相对位置四类各占 1/4;增量 1–200 条;`feed.resync_buf ∈ {8, 1024}`
- 参考:按 hft-venue-proto 契约 1 的链续模型
- 例数:PR 500
- 实现:`crates/hft-venue/tests/resync.rs::ven_105_align_and_forward`

### VEN-106 增量深度:缓冲溢出、snap 记录失败与快照期间再次断裂

- 规则:[增量深度与快照重同步(可选扩展)](README.md#增量深度与快照重同步可选扩展)
- 级别:table
- 里程碑:M7
- 状态:可选扩展:全量增量深度
- 前置:`F0`,`feed.depth = incremental`,`feed.resync_buf = 8`,`feed.resync_slots = 4`,`feed.snapshot_backoff_ms = (100, 5000)`;inst S1 处于 `AwaitSnapshot`
- 实现:`crates/hft-venue/tests/resync.rs::ven_106_resync_failures`

| 行 | 情形 | 期望 |
|---|---|---|
| VEN-106.1 | 快照到达前缓冲第 9 条增量 | 清空缓冲,重新进入 `Resync`,下一个 `SnapReq{L2}` 在 100 ms 后 |
| VEN-106.2 | snap 记录 `HttpErr` / `Timeout` / `TooLarge` | 各按退避重试,退避序列 100、200、400…(上限 5000)ms |
| VEN-106.3 | 第 6 次连续失败 | `ConnState{Degraded, inst: S1, reason: SNAPSHOT_FAIL(3)}` |
| VEN-106.4 | 快照应用后、`Up` 之前再次断裂 | 重新 `Resync`,不推 `Up` |
| VEN-106.5 | 第 5 个 inst 同时进入重同步 | 排队,不占槽;前 4 个之一 `Up` 后才发它的 `SnapReq` |

### VEN-107 基准:帧 → 一条 `L2Batch`(2 KiB depth)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M7
- 状态:可选扩展:全量增量深度
- 基准名:`venue_feed_l2_path`
- 实现:`crates/hft-venue/benches/feed.rs::ven_107_feed_l2_path`

### VEN-108 基准:应用一条 1000 档 snap 记录

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M7
- 状态:可选扩展:全量增量深度
- 基准名:`venue_feed_snap_apply_1000`
- 实现:`crates/hft-venue/benches/feed.rs::ven_108_feed_snap_apply_1000`

### VEN-109 基准:REST 快照响应 → snap 记录(1000 档)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M7
- 状态:可选扩展:全量增量深度
- 基准名:`venue_snap_fetch_1000`
- 实现:`crates/hft-venue/benches/snap.rs::ven_109_snap_fetch_1000`

### VEN-120 BBO 连接每 `bbo_ping_ms` 探测一次,与有无数据无关

- 规则:[§3a](README.md#3a-bbo-连接健康)、[§5](README.md#5-心跳)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.bbo_ping_ms = 1000`;连接 0 在 `t = 0` 交接;`poll_once` 每 1 ms 一次,运行到 `t = 10 s`
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_120_probe_period`

| 行 | 连接上的数据 / `bbo_probe()` | 期望 |
|---|---|---|
| VEN-120.1 | 每 10 ms 一帧 Bbo / `WsPing` | 恰好 10 个 WS Ping,发送时刻 1000、2000、…、10000 ms;每个 payload = 发送时 TSC 的 8 字节小端 |
| VEN-120.2 | 无数据 / `WsPing` | 同上 |
| VEN-120.3 | 无数据 / `AppText` | 10 条 `app_ping` 文本帧,无 WS Ping |

### VEN-121 连接级与成员事件

- 规则:[§3a](README.md#3a-bbo-连接健康)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;订阅 S1、S2
- 步骤:
  1. 交接连接 0(`t = 100`),`poll_once`
  2. 回 S1、S2 的 `ACK`,`poll_once`
  3. `Unsubscribe{S2}`,回退订 `ACK`,`poll_once`
- 期望:
  - 步骤 1:`ConnState{Up, role: Md, conn: 0, inst: 0, rx_tsc: 100}`
  - 步骤 2:`ConnState{Up, conn: 0, inst: S1}` 与 `{Up, conn: 0, inst: S2}` 各一条
  - 步骤 3:`ConnState{Down, conn: 0, inst: S2}` 一条;S1 无新事件
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_121_conn_and_member_events`

### VEN-122 回帧时距上次发布 ≥ `bbo_ping_ms / 2` 才推 `Alive`

- 规则:[§3a](README.md#3a-bbo-连接健康)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.bbo_ping_ms = 1000`;连接 0 最近一次发布的记录 `rx_tsc = 1_000_000_000`;每行收到一帧 Pong,其 `rx_tsc` 如列
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_122_alive_threshold`

| 行 | Pong 的 `rx_tsc` | 期望 |
|---|---|---|
| VEN-122.1 | 1 499 999 999 | 无 `Alive` |
| VEN-122.2 | 1 500 000 000 | `ConnState{Alive, role: Md, conn: 0, inst: 0, account: 0, rx_tsc: 1500000000}` |
| VEN-122.3 | 1 500 000 001 | `Alive`,`rx_tsc = 1500000001` |

### VEN-123 盘口 10 s 不变时每条 BBO 连接每秒一条 `Alive`

- 规则:[§3a](README.md#3a-bbo-连接健康)
- 级别:integ
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;连接 0、1 已 Up,S1–S3 是成员;对端只在每次 Ping 后 50 ms 回 Pong,不推任何报价
- 步骤:
  1. 运行 10 s(`poll_once` 每 1 ms)
- 期望:
  - 每条连接恰好 10 条 `Alive`,`rx_tsc` 严格递增;同一连接相邻两条活性证据(`Up` 或 `Alive`)的 `rx_tsc` 之差 ≤ 1.05 × 10⁹ ns;无 `Bbo`、无 `Degraded`
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_123_quiet_book_alive`

### VEN-124 持续发布 `Bbo` 的连接不产出 `Alive`

- 规则:[§3a](README.md#3a-bbo-连接健康)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.bbo_conns = 1`;连接 0 每 10 ms 一帧新报价,每秒一个 Pong
- 步骤:
  1. 运行 5 s
- 期望:
  - bbo 环无 `Alive`;`Bbo` 500 条
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_124_busy_no_alive`

### VEN-125 只收到重复帧的赛马连接照常产出 `Alive`

- 规则:[§3a](README.md#3a-bbo-连接健康)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;连接 0 每个报价都比连接 1 早 1 ms 到达(连接 1 的帧全是重复帧);两条都每秒一个 Pong
- 步骤:
  1. 运行 5 s
- 期望:
  - 连接 1 每秒一条 `Alive`(共 5 条);连接 0 无 `Alive`
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_125_dup_only_conn_alive`

### VEN-126 超过 `bbo_dead_ms` 无入向帧 → `BBO_SILENT`、`Down` 并补建

- 规则:[§3a](README.md#3a-bbo-连接健康)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.bbo_dead_ms = 3000`;连接 0 最后一帧入向帧的 `rx_tsc = 10⁹`,之后对端不回任何帧;每行在所列时刻 `poll_once`
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_126_silent`

| 行 | `now` | 期望 |
|---|---|---|
| VEN-126.1 | 10⁹ + 3 × 10⁹ | 无 `Degraded`、无 `Down` |
| VEN-126.2 | 10⁹ + 3 × 10⁹ + 1 | 依次 `ConnState{Degraded, conn: 0, reason: 7}`、`ConnState{Down, conn: 0, inst: 0}`;`RetireMsg` 一条;补建 `BuildReq` 一条 |

### VEN-127 交接后从未收到入向帧的连接从交接时刻起算静默

- 规则:[§3a](README.md#3a-bbo-连接健康)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`,`feed.bbo_dead_ms = 3000`;连接 0 在 `t = 5 × 10⁹` 交接,对端从不回帧
- 步骤:
  1. `poll_once` 于 `t = 5 × 10⁹ + 1`、`8 × 10⁹`、`8 × 10⁹ + 1`
- 期望:
  - 前两次无 `Degraded`;第三次推 `Degraded{reason: 7}` 与 `Down`
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_127_silent_from_install`

### VEN-128 inst 的全部 BBO 连接都断过才推 `BBO_REFRESH`

- 规则:[§3a](README.md#3a-bbo-连接健康)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;连接 0、1 已 Up,S1、S2 是两者成员
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_128_refresh_when_all_down`

| 行 | 过程 | 期望 |
|---|---|---|
| VEN-128.1 | `rst(0)`;替补连接 0 交接并回 `ACK` | 无任何 `Resync` |
| VEN-128.2 | `rst(0)`、`rst(1)`;替补交接,第一条回 `ACK` | 回执齐的那一轮 S1、S2 各一条 `ConnState{Resync, role: Md, conn: <新连接>, inst, reason: 6}` |
| VEN-128.3 | 同 .2,第二条替补随后回 `ACK` | 不再推 `Resync`(两 inst 已在补帧) |

### VEN-129 补帧中的 inst 下一条 `u ≥ last_fwd` 的流上报价被转发并结束补帧

- 规则:[§3a](README.md#3a-bbo-连接健康)、[§3](README.md#3-去重断裂与重同步)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;S1 的 `last_fwd[BBO] = 200`,S1 处于补帧(VEN-128.2 之后);每行独立
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_129_refresh_ends_on_quote`

| 行 | 依次到达的帧 | 期望 |
|---|---|---|
| VEN-129.1 | `B\|S1\|200\|…` | 转发 `u = 200`,补帧结束;之后再来 `u = 200` 被丢 |
| VEN-129.2 | `B\|S1\|199\|…` | 丢;仍在补帧 |
| VEN-129.3 | `B\|S1\|201\|…` | 转发,补帧结束 |

### VEN-130 宽限期满仍无报价时经 snap-fetch 补帧

- 规则:[§3a](README.md#3a-bbo-连接健康)、[§3b](README.md#3b-快照获取snap-fetch)
- 级别:table
- 里程碑:M3
- 前置:`F0`,`routes = Bbo`,`feed.bbo_refresh_grace_ms = 500`,`StubCfg{bbo_snapshot: true}`;S1 进入补帧,回执齐的时刻 `t_a = 10⁹`;对端不推 S1 的报价;每行在所列时刻 `poll_once`
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_130_grace_then_snapreq`

| 行 | `now` | 期望 |
|---|---|---|
| VEN-130.1 | `t_a + 499 999 999` | snapreq 环无元素 |
| VEN-130.2 | `t_a + 500 000 000` | snapreq 环一条 `SnapReq{kind: Bbo(2), inst: S1, attempt: 0}`,`req_id` 本任务内单调 |
| VEN-130.3 | .2 之后 snap 环送回 `{status: Ok, Bbo{S1, u: 250}}` | bbo 环转发 `Bbo{S1, u: 250}`,补帧结束;本任务 raw 多一条 `RawRx{what: RestResponse, conn: <snap-fetch 连接号>}`,记录头 `tsc` = 记录里的响应接收戳 |

### VEN-131 订阅即推首帧的所不发补帧请求

- 规则:[§3a](README.md#3a-bbo-连接健康)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Bbo`,`StubCfg{bbo_snapshot: false}`;S1 进入补帧
- 步骤:
  1. 不推 S1 报价,运行 10 s
- 期望:
  - snapreq 环始终为空;S1 一直处于补帧;`CallLog` 中 `bbo_snapshot_request` 只在 `FeedRuntime::new` 时被调用一次
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_131_no_snap_when_pushes_first`

### VEN-132 snap 记录带回的报价按补帧状态处理

- 规则:[§3a](README.md#3a-bbo-连接健康)
- 级别:table
- 里程碑:M3
- 前置:`F0`,`routes = Bbo`;S1 的 `last_fwd[BBO] = 300`;每行独立
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_132_snap_bbo_rules`

| 行 | S1 状态 / snap 记录中的 `Bbo.update_id` | 期望 |
|---|---|---|
| VEN-132.1 | 补帧中 / 300 | 转发,补帧结束,`last_fwd = 300` |
| VEN-132.2 | 补帧中 / 299 | 丢弃并计数 `snap_stale`,仍在补帧 |
| VEN-132.3 | 补帧中 / 0(响应无序号) | 转发,补帧结束,`last_fwd` 仍为 300;随后流上 `u = 301` 照常转发 |
| VEN-132.4 | 已因流上报价结束补帧 / 310 | 丢弃并计数 `snap_late`;raw 照写 |

### VEN-133 补帧请求失败后在下一个宽限期重试

- 规则:[§3a](README.md#3a-bbo-连接健康)、[§3b](README.md#3b-快照获取snap-fetch)
- 级别:unit
- 里程碑:M3
- 前置:同 VEN-130.2 之后;snap 环送回 `{status: HttpErr, http_status: 503}` 的时刻 `t_f`
- 步骤:
  1. `poll_once` 于 `t_f + 499 999 999` 与 `t_f + 500 000 000`
- 期望:
  - 第一次无新 `SnapReq`;第二次一条 `SnapReq{Bbo, S1, attempt: 1}`;失败记录的原始体写进本任务 raw(`RestResponse`)
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_133_refresh_retry`

### VEN-134 深度、成交与标记价连接的 Pong 在 md 环产出 `Alive`

- 规则:[§3a](README.md#3a-bbo-连接健康)、[§5](README.md#5-心跳)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = All`,`feed.bbo_ping_ms = 1000`;深度连接 8 上 2 s 无数据
- 步骤:
  1. 连接 8 的 RTT Ping 得到 Pong(`rx_tsc = t_p`),`poll_once`
- 期望:
  - md 环一条 `ConnState{Alive, role: Md, conn: 8, inst: 0, rx_tsc: t_p}`;bbo 环无新元素
- 实现:`crates/hft-venue/tests/bbo_live.rs::ven_134_md_conn_alive`

### VEN-150 snap-fetch 在途数不超过 `snap.parallel`,其余按 FIFO 排队

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:unit
- 里程碑:M3
- 前置:`S0`,`snap.parallel = 2`;`StubCfg{bbo_batch: 1}`;snapreq-bbo 环中 6 条 `SnapReq{Bbo}`(S1、S2、S3、P001、P002、P003),`req_id` 1–6
- 步骤:
  1. `poll_once`
  2. 让第一个请求先响应,`poll_once`
- 期望:
  - 步骤 1:恰好 2 个请求在途(S1、S2);其余 4 条在本地 FIFO
  - 步骤 2:写出 `req_id = 1` 的记录,随后提交 S3 的请求;在途仍为 2
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_150_parallel_cap`

### VEN-151 每条 `SnapReq` 恰好得到一条 snap 记录

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:prop
- 里程碑:M3
- 性质:输入 n 条 `SnapReq` 后,snap 环中恰好 n 条记录,`req_id` 集合与输入相同,每个 `req_id` 一次
- 生成:n = 1–200;每个响应从 {200、503、429、超时、解码失败、超大体} 抽取;`snap.parallel` 取 1–4;`bbo_batch` 取 1–8
- 参考:按 `req_id` 计数
- 例数:PR 512
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_151_one_record_per_req`

### VEN-152 连续的 BBO 补帧请求合并成批,原始体只随每批第一条记录

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:unit
- 里程碑:M3
- 前置:`S0`,`snap.parallel = 2`;`StubCfg{bbo_batch: 4}`;`SNAP_BBO_BATCH_MAX = 32`;FIFO 中 6 条连续的 `SnapReq{Bbo}`
- 步骤:
  1. `poll_once` 直到全部响应
- 期望:
  - `CallLog`:`bbo_snapshot_request` 先以 6 条调用返回 `Err(BufFull)`,再以 3 条调用两次,各返回 `Ok(true)`;共 2 个 REST 请求
  - snap 环 6 条记录,每条 `status = Ok`、各带本 inst 的 1 个 `Bbo`;每批第一条记录 `body_len` = 响应体长度,其余 `body_len = 0`
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_152_bbo_batch_merge`

### VEN-153 单条也无法生成请求时记录 `ReqErr`

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:unit
- 里程碑:M3
- 前置:`S0`;`StubProto` 变体对任何调用返回 `Err(ProtoErr::Cfg)`;一条 `SnapReq{Bbo, S1, req_id: 9}`
- 步骤:
  1. `poll_once`
- 期望:
  - snap 环一条记录 `{req_id: 9, status: ReqErr(7), http_status: 0, body_len: 0}`;无 REST 请求
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_153_req_err`

### VEN-154 REST 权重余量不足时延后发送而不失败

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:unit
- 里程碑:M3
- 前置:`S0`;本实例行情出口 IP 的 REST 权重桶 10 / 60 000 ms,已用 9;每个补帧请求名义权重 2
- 步骤:
  1. 一条 `SnapReq{Bbo, S1}`,`poll_once`
  2. 时钟推进到窗口滚过,`poll_once`
- 期望:
  - 步骤 1:无 REST 请求、无 snap 记录
  - 步骤 2:提交该请求;响应后一条 `status = Ok` 的记录
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_154_weight_defer`

### VEN-155 snap-fetch 失败响应的状态码

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:table
- 里程碑:M3
- 前置:`S0`,hft-net `rest_timeout_ms = 2000`;每行一条 `SnapReq{Bbo, S1}`
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_155_failure_status`

| 行 | 响应 | 期望记录 |
|---|---|---|
| VEN-155.1 | HTTP 429 | `{status: RateLimited, http_status: 429}`,带响应体 |
| VEN-155.2 | HTTP 418 | `{status: RateLimited, http_status: 418}` |
| VEN-155.3 | HTTP 503 | `{status: HttpErr, http_status: 503}`,带响应体 |
| VEN-155.4 | 2000 ms 无响应 | `{status: Timeout, http_status: 0, body_len: 0}` |
| VEN-155.5 | 200 但 `decode_bbo_snapshot` 返回 `Err` | `{status: Decode, http_status: 200}`,带响应体,`parts_len = 0` |

### VEN-156 记录超过 snap 环容量的 1/4 时只写头、`status = TooLarge`

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:table
- 里程碑:M3
- 前置:`S0`,snap 环容量 64 KiB(上限 16 384 B);每行响应体长度如列(头与尾部合计 128 B)
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_156_too_large`

| 行 | 记录总长 | 期望 |
|---|---|---|
| VEN-156.1 | 16 384 | 完整记录,`status = Ok` |
| VEN-156.2 | 16 385 | 只写头,`status = TooLarge`,`body_len = 0`、`parts_len = 0` |

### VEN-157 snap 环满时暂缓且不丢

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:unit
- 里程碑:M3
- 前置:`S0`,`snap.parallel = 1`;snap 环已满;两条 `SnapReq` 排队,第一条的响应已到
- 步骤:
  1. `poll_once`
  2. 消费者取走记录,`poll_once` 两次
- 期望:
  - 步骤 1:无新记录;第二条请求未提交(该连接一连接一在途)
  - 步骤 2:两条记录按 `req_id` 顺序写入
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_157_snap_ring_full`

### VEN-158 snap-fetch 的 `FeedCtx` 与行情任务的实例互不影响

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:integ
- 里程碑:M3
- 前置:`StubProto` 的 `FeedCtx` 带一个随每次解码前移的私有计数;一份行情帧脚本与一份补帧响应脚本
- 步骤:
  1. 在同一线程上交替运行 `FeedRuntime` 与 `SnapFetchTask` 各 10⁴ 次
  2. 两者各自单独运行同样的脚本
- 期望:
  - 两种运行下 bbo 环序列逐字节相同,snap 环记录逐字节相同
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_158_feed_ctx_independent`

### VEN-159 warm 任务的空闲 REST 连接按 `gw.rest_idle_check_ms` 检查存活

- 规则:[配置项](README.md#配置项)、[§3b](README.md#3b-快照获取snap-fetch)
- 级别:table
- 里程碑:M3
- 前置:`S0`,`gw.rest_idle_check_ms = 1000`;两条空闲 REST 连接,上次检查在 `t_c`;对 `C0` 的 acct-fetch 池做同样的三行
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_159_idle_check`

| 行 | `now` / 对端 | 期望 |
|---|---|---|
| VEN-159.1 | `t_c + 999 999 999` / 对端在线 | 两条连接的 `recvmsg` 均为 0 |
| VEN-159.2 | `t_c + 10⁹` / 对端在线 | `RestPool::idle_check` 一次,两条各 1 次 `recvmsg` |
| VEN-159.3 | `t_c + 10⁹` / 连接 65 已被对端关闭 | 连接 65 退役(`RetireMsg`)并在同一轮发出补建 `BuildReq` |

### VEN-160 snap-fetch 不写 raw,原始响应由行情任务写入

- 规则:[产生与消费的事件](README.md#产生与消费的事件)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:integ
- 里程碑:M3
- 前置:`F0`(`routes = Bbo`)与 `S0` 组装;一次 BBO 补帧
- 步骤:
  1. 完成 VEN-130 的过程
- 期望:
  - snap-fetch 不持有任何 `JournalWriter`;`raw-feed-V7-bbo` 中该响应恰好一条 `RawRx{what: RestResponse, conn: 64–79 内的连接号}`,payload 与 REST 响应体逐字节相同
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_160_raw_written_by_feed`

### VEN-161 snap-fetch 的 `has_pending` 与 `LoopStat`

- 规则:[运行时入口](README.md#运行时入口)、[§3b](README.md#3b-快照获取snap-fetch)
- 级别:table
- 里程碑:M3
- 前置:`S0`
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_161_has_pending`

| 行 | 状态 | 期望 |
|---|---|---|
| VEN-161.1 | snapreq 环为空,无在途,本地 FIFO 为空 | `has_pending = false`;`poll_once` 返回 `did_work = false` |
| VEN-161.2 | snapreq 环有 1 条 | `has_pending = true` |
| VEN-161.3 | 一个在途请求的完整响应已在 TLS 缓冲 | `has_pending = true`;`poll_once` 写出记录且 `did_work = true` |

### VEN-162 一次合并的 BBO 补帧请求不超过 `SNAP_BBO_BATCH_MAX`

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:table
- 里程碑:M3
- 前置:`S0`,`snap.parallel = 2`;`StubCfg{bbo_batch: 64}`;`SNAP_BBO_BATCH_MAX = 32`;FIFO 中连续的 `SnapReq{Bbo}` 条数如列
- 实现:`crates/hft-venue/tests/snap_fetch.rs::ven_162_batch_max`

| 行 | 条数 | 期望 `bbo_snapshot_request` 调用 |
|---|---|---|
| VEN-162.1 | 31 | 1 次,31 个 inst |
| VEN-162.2 | 32 | 1 次,32 个 inst |
| VEN-162.3 | 33 | 2 次,先 32 个后 1 个 |

### VEN-163 一次合并的 BBO 补帧请求不超过 `P::BBO_SNAPSHOT_BATCH`

- 规则:[§3b](README.md#3b-快照获取snap-fetch)
- 级别:table
- 里程碑:M3
- 前置:`S0`,`snap.parallel = 2`;`StubCfg{bbo_batch: 64}`;`SNAP_BBO_BATCH_MAX = 32`;FIFO 中 5 条连续的 `SnapReq{Bbo}`(S1、S2、S3、S1、S2)
- 实现:`crates/hft-venue/tests/snap.rs::ven_163_bbo_batch_const`

| 行 | `P::BBO_SNAPSHOT_BATCH` | 期望 `bbo_snapshot_request` 调用与 `decode_bbo_snapshot` 的 `insts` |
|---|---|---|
| VEN-163.1 | 1 | 5 次,各 1 条;无 `BufFull`;每次 `decode_bbo_snapshot` 的 `insts` 为该请求的那一个 inst |
| VEN-163.2 | 2 | 3 次:2、2、1 条;`insts` 依次为 [S1, S2]、[S3, S1]、[S2] |
| VEN-163.3 | 3 | 2 次:3、2 条 |

### VEN-170 分档快照:新订阅 → `Resync{NEW_SUB}` → 首条快照 → `L2Snapshot{TopN}` + `Up`

- 规则:[§3c](README.md#3c-分档快照depthtopn)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Md`,`feed.depth = topn`,`feed.topn_levels = 5`
- 步骤:
  1. `Subscribe{S1, BOOK}`,完成建连并回 `ACK`,`poll_once`
  2. 推 `D|S1|10|5|…`,`poll_once`
- 期望:
  - 步骤 1:md 环 `ConnState{Resync, inst: S1, reason: 16}`;S1 的 `l2_state = AwaitFirst`
  - 步骤 2:md-bulk 一条 `L2Snapshot{origin: TopN, snap_id: 10, n_bids + n_asks = 10}`,md 环 `FeedMsg::Bulk` 后紧跟 `ConnState{Up, inst: S1}`;`l2_state = Live`;不产出任何 `L2Batch`
- 实现:`crates/hft-venue/tests/topn.rs::ven_170_new_sub_first_snapshot`

### VEN-171 分档快照的重复与倒序被丢

- 规则:[§3c](README.md#3c-分档快照depthtopn)、[§3](README.md#3-去重断裂与重同步)
- 级别:table
- 里程碑:M3
- 前置:`F0`,`routes = Md`;S1 已 `Live`,`last_fwd[深度] = 10`;按行顺序喂入
- 实现:`crates/hft-venue/tests/topn.rs::ven_171_snapshot_dedup`

| 行 | 帧 | 期望 |
|---|---|---|
| VEN-171.1 | `D\|S1\|10\|…` | 丢 |
| VEN-171.2 | `D\|S1\|9\|…` | 丢 |
| VEN-171.3 | `D\|S1\|11\|…` | 转发 |
| VEN-171.4 | `DT\|S1\|<与上一条相同的 ex_ms>\|…`(`ByTime` 变体) | 丢;`ex_ms` 加 1 的下一条转发 |

### VEN-172 深度连接断开 → 每个 inst `Resync{CONN_LOST}`,新连接首条快照后 `Up`

- 规则:[§3c](README.md#3c-分档快照depthtopn)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Md`;S1、S2 在连接 8 上 `Live`
- 步骤:
  1. `eof(8)`,`poll_once`
  2. 替补交接、回 `ACK`;推 `D|S1|20|…`,`poll_once`;推 `D|S2|30|…`,`poll_once`
- 期望:
  - 步骤 1:md 环 `Down{conn: 8, inst: 0}`、`Resync{S1, reason: 15}`、`Resync{S2, reason: 15}`;无 `SnapReq`
  - 步骤 2:S1 的 `L2Snapshot` 之后 `Up{S1}`;S2 同理;两者之间 S2 仍未 `Up`
- 实现:`crates/hft-venue/tests/topn.rs::ven_172_conn_lost`

### VEN-173 `FeedCmd::Resync{inst}` → `Resync{REQUESTED}`,下一条快照后 `Up`

- 规则:[§3c](README.md#3c-分档快照depthtopn)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Md`;S1 `Live`,`last_fwd[深度] = 40`
- 步骤:
  1. 投递 `Resync{S1}`,`poll_once`
  2. 推 `D|S1|41|…`,`poll_once`
- 期望:
  - 步骤 1:md 环 `ConnState{Resync, inst: S1, reason: 17}`;不发任何退订或 REST 请求
  - 步骤 2:`L2Snapshot{snap_id: 41}` 后 `Up{S1}`
- 实现:`crates/hft-venue/tests/topn.rs::ven_173_requested_resync`

### VEN-174 分档快照解码失败推 `Resync{DECODE}`,下一条快照恢复

- 规则:[§3c](README.md#3c-分档快照depthtopn)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:unit
- 里程碑:M3
- 前置:`F0`,`routes = Md`;S1 `Live`
- 步骤:
  1. 推坏帧 `D|S1|50|5|x`,`poll_once`
  2. 推 `D|S1|51|…`,`poll_once`
- 期望:
  - 步骤 1:`decode_err{DepthTopN} = 1`;md 环 `Resync{S1, reason: 2}`
  - 步骤 2:`L2Snapshot` 后 `Up{S1}`
- 实现:`crates/hft-venue/tests/topn.rs::ven_174_decode_resync`

### VEN-175 分档快照路径从不产生 L2 快照请求与 `L2Batch`

- 规则:[§3c](README.md#3c-分档快照depthtopn)
- 级别:prop
- 里程碑:M3
- 性质:`feed.depth = topn` 时,任意深度帧、断连、`Resync` 命令与订阅变化的序列之后:snapreq 环中 `kind = L2` 的请求数为 0;md 环中 `L2Batch` 为 0;`CallLog` 中 `snapshot_request`、`decode_snapshot`、`align_first` 调用数为 0
- 生成:1–5 个 inst;0–500 步,每步从 {深度帧(序号随机前进或重复)、坏帧、`eof`、`Resync{inst}`、`Subscribe`/`Unsubscribe`} 抽取
- 参考:计数断言
- 例数:PR 512
- 实现:`crates/hft-venue/tests/topn.rs::ven_175_topn_never_l2_rest`

### VEN-176 分档快照按 `feed.snapshot_part_levels` 切片

- 规则:[§3c](README.md#3c-分档快照depthtopn)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`F0`,`routes = Md`,`feed.snapshot_part_levels = 64`;每行一帧档数如列(买卖合计)
- 实现:`crates/hft-venue/tests/topn.rs::ven_176_snapshot_parts`

| 行 | 档数 | 期望 |
|---|---|---|
| VEN-176.1 | 20 | 1 片(`part = 0, parts = 1`),1 条 `FeedMsg::Bulk` |
| VEN-176.2 | 64 | 1 片 |
| VEN-176.3 | 65 | 2 片(64 + 1),2 条 `FeedMsg::Bulk`,按片序写入且相邻 |
| VEN-176.4 | 200 | 4 片(64 + 64 + 64 + 8) |

### VEN-190 `drain_out` 的批内重排

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`;每行是一次 `drain_out` 从出环取到的一批(全部为账户 A1,不超过每账户配额);RR = `oflags` 含 `RISK_REDUCING`;读发送顺序
- 实现:`crates/hft-venue/tests/gw_drain.rs::ven_190_batch_reorder`

| 行 | 出环顺序 | 期望发送顺序 |
|---|---|---|
| VEN-190.1 | `P1`(非 RR,槽 1)、`P2`(RR,槽 2)、`C3`(撤槽 9)、`P4`(RR,槽 4) | `P2, P4, P1, C3` |
| VEN-190.2 | `P1`(非 RR,槽 5)、`C2`(撤槽 9)、`A3`(RR 改单,目标槽 5)、`P4`(RR,槽 7) | `P4, P1, C2, A3`(`A3` 不越过同批中目标槽的 `P1`,留在原位) |
| VEN-190.3 | `A1`(RR 改单,目标槽 3,不在本批)、`P2`(非 RR) | `A1, P2` |
| VEN-190.4 | `P1`(非 RR,槽 5)、`Q2`(查槽 5)、`P3`(RR) | `P3, P1, Q2` |
| VEN-190.5 | `DeadManRefresh`、`P1`(RR) | `P1, DeadManRefresh` |
| VEN-190.6 | `P1`(RR,槽 5)、`C2`(撤槽 5) | `P1, C2`(不变) |

### VEN-191 每账户每圈至多 `cmds_per_account_per_loop` 条订单类命令,超出的暂存下圈先发

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`,`gw.cmds_per_loop = 16`,`gw.cmds_per_account_per_loop = 8`;每圈对每个账户调一次 `drain_out`(两账户均无回报帧);出环依次为 A1 的 20 条 `Place`(`a1…a20`)与 A2 的 2 条(`b1, b2`)
- 步骤:
  1. 运行 3 圈
- 期望:
  - 第 1 圈发出 `a1…a8` 与 `b1, b2`;第 2 圈发出 `a9…a16`;第 3 圈发出 `a17…a20`;A1 的发送顺序与出环顺序相同
  - 每条命令恰好一条 `SendReport{status: Written}`;任一时刻暂存条数 ≤ 16
- 实现:`crates/hft-venue/tests/gw_drain.rs::ven_191_per_account_quota`

### VEN-192 `kill_admit` 判定表

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M1.5
- 前置:`mask = GLOBAL | VENUE | HARD`(0x7);`ix = 0` 时 `acct_bits = account(0) | account_hard(0)` = 0x0010_0100,`ix = 11` 时 = 0x8008_0000;RR = `oflags` 含 `RISK_REDUCING`
- 实现:`crates/hft-venue/tests/kill.rs::ven_192_kill_admit_table`

| 行 | `word` / `ix` / `op` / RR | 期望 |
|---|---|---|
| VEN-192.1 | 0 / 0 / Place / 否 | true |
| VEN-192.2 | `GLOBAL` / 0 / Place / 否;是 | false;true |
| VEN-192.3 | `VENUE` / 0 / Amend / 否;是 | false;true |
| VEN-192.4 | `account(0)` / 0 / Place / 否;是 | false;true |
| VEN-192.5 | `account(1)` / 0 / Place / 否 | true(另一账户的位) |
| VEN-192.6 | `HARD` / 0 / Place / 是;Amend / 是 | false;false |
| VEN-192.7 | `account_hard(0)` / 0 / Place / 是 | false |
| VEN-192.8 | `account_hard(1)` / 0 / Place / 否 | true |
| VEN-192.9 | `GLOBAL \| HARD \| account_hard(0)` / 0 / Cancel;CancelAll;Query | true;true;true |
| VEN-192.10 | `HARD` / 0 / `DeadManRefresh`、`Rotate`、`SetEpoch`、`MirrorRestore` | 全部 true |
| VEN-192.11 | `account(11)` / 11 / Place / 是;否 | true;false |
| VEN-192.12 | `account_hard(11)` / 11 / Place / 是 | false |
| VEN-192.13 | 0xF8(保留位 3..7)/ 0 / Place / 否 | true |

### VEN-193 32 个位逐位置位时 `kill_admit` 与 hft-risk 投影语义一致

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:prop
- 里程碑:M1.5
- 性质:对任意 `word: u32`、`ix ∈ 0..=11`、`op ∈ {Place, Amend}`、RR ∈ {是, 否}:`kill_admit(word, mask, acct_bits(ix), ix, op, oflags)` = ¬hard ∧ (¬soft ∨ RR),其中 hard = `word & (HARD | account_hard(ix)) ≠ 0`,soft = `word & (GLOBAL | VENUE | account(ix)) ≠ 0`;`op ∈ {Cancel, CancelAll, Query}` 与维护类恒为 true
- 生成:32 个单比特字逐一 × `ix ∈ {0, 11}` × 两种 op × RR(穷举);另随机 `word` 与 `ix`
- 参考:上式(hft-risk kill 字投影表的语义)
- 例数:穷举 256 组 + PR 10 000 个随机字
- 实现:`crates/hft-venue/tests/kill.rs::ven_193_kill_admit_matches_projection`

### VEN-194 被 kill 拦下的订单回 `KillBlocked`、不编码不发送

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[出环](README.md#出环outmsg--gwcmd)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`;kill 字 = `account(0)`(A1 的账户 kill)
- 步骤:
  1. 出环:A1 `Place`(非 RR,槽 10,`decision_seq = 100`)、A1 `Place`(RR,槽 11)、A2 `Place`(非 RR,槽 12)、A1 `Cancel`(目标槽 3,镜像中存在)
- 期望:
  - 槽 10:`SendReport{decision_seq: 100, op: Place, slot: 10, status: KillBlocked(2)}`,`encode` 对它未被调用,无帧
  - 槽 11、12 与 `Cancel`:各一条 `SendReport{status: Written}`,各一帧
- 实现:`crates/hft-venue/tests/kill.rs::ven_194_kill_blocked`

### VEN-195 kill 字在每条订单命令前重新读取

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`;kill 字为 0;`StubProto::encode` 的测试钩子在编码第一条命令时向 kill 字 `store(GLOBAL, Release)`
- 步骤:
  1. 同一批出环:A1 `Place`(非 RR,槽 1)、A1 `Place`(非 RR,槽 2)
- 期望:
  - 槽 1 `Written`;槽 2 `KillBlocked`
- 实现:`crates/hft-venue/tests/kill.rs::ven_195_kill_reloaded_per_cmd`

### VEN-196 发送侧计数超交易所硬上限时回 `RateLimited`,按账户隔离

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[出环](README.md#出环outmsg--gwcmd)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`;(V7, A1) 与 (V7, A2) 下单桶各 10 条 / 1000 ms;`t = 0`
- 步骤:
  1. 1 ms 内出环 A1 的 11 条 `Place` 与 A2 的 1 条
  2. 出环 `ResetRate{account: A1}`,再出环 A1 的 1 条 `Place`
  3. A1 的下一条响应带 `RateObs{used: 10, limit: 10, window_ms: 1000}`;出环 A1 的 1 条 `Place`
- 期望:
  - 步骤 1:A1 前 10 条 `Written`,第 11 条 `RateLimited(3)` 且无帧;A2 那条 `Written`
  - 步骤 2:`ResetRate` 无 `SendReport`;其后那条 `Written`
  - 步骤 3:`RateLimited`
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_196_rate_limited`

### VEN-197 连接选择:primary 不可用改走 standby,都不可用回 `NoConn`

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`;A1 的 primary = 连接 112,standby = 113;每行一条 A1 `Place`
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_197_conn_choice`

| 行 | 连接状态 | 期望 |
|---|---|---|
| VEN-197.1 | 112 `Open`,113 `Open` | `Written`,`conn = 112` |
| VEN-197.2 | 112 `Closing`,113 `Open` | `Written`,`conn = 113` |
| VEN-197.3 | 112 与 113 都不是 `Open` | `NoConn(5)`;两条连接的 `MemPeer` 都没收到密文 |
| VEN-197.4 | 112 `Open` 但 `send_frame` 返回 `NotOpen`,113 `Open` | 改走 113,`Written`,`conn = 113`;112 没收到密文 |
| VEN-197.5 | 112 返回 `TxBacklog`,113 返回 `NotOpen` | `NoConn`;两条都没收到密文 |

### VEN-198 写入失败如实上报 `WriteError`,Place 照写槽镜像

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`;A1 primary 的 `send_frame` 返回 `SendErr::Io(104)`
- 步骤:
  1. 出环 A1 `Place`(槽 20,`gen = 3`)
  2. 出环 A1 `Query`(目标槽 20)
- 期望:
  - 步骤 1:`SendReport{op: Place, slot: 20, gen: 3, status: WriteError(4), os_errno: 104}`;不再换 standby 重试;槽镜像 20 为 `{gen: 3, epoch: 5, live: true}`
  - 步骤 2:用槽 20 的 client id 编码并发出(在 standby 上),`Written`
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_198_write_error`

### VEN-199 `SendReport` 的槽字段按 op 与状态填写

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[出环](README.md#出环outmsg--gwcmd)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`;A1 的槽镜像:槽 7 `{gen: 2, epoch: 5, live: true}`;每行一条命令
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_199_send_report_fields`

| 行 | 命令 / 情形 | 期望 `SendReport` |
|---|---|---|
| VEN-199.1 | `Place{slot: 9, gen: 1}` / 发出 | `{op: Place, slot: 9, gen: 1, target_slot: 0, target_gen: 0, status: Written, conn: 112, venue: V7, account: A1}`,`sent_tsc` = 发送时刻 |
| VEN-199.2 | `Cancel{target_slot: 7}` / 发出 | `{op: Cancel, slot: 7, gen: 2, target_slot: 7, target_gen: 2, status: Written}` |
| VEN-199.3 | `Amend{target_slot: 7}` / kill = `GLOBAL`,非 RR | `{op: Amend, target_slot: 7, target_gen: 2, status: KillBlocked}` |
| VEN-199.4 | `Query{target_slot: 7}` / 两连接都不可用 | `{op: Query, target_slot: 7, target_gen: 2, status: NoConn}` |
| VEN-199.5 | `CancelAll{inst: 0}` / 发出 | `{op: CancelAll, target_slot: 0, status: Written}` |
| VEN-199.6 | `Place{slot: 9}` / `encode` 返回 `FieldOverflow` | `{op: Place, slot: 9, status: EncodeError(6)}` |

### VEN-200 每条订单类 `GwCmd` 恰好一条 `SendReport`,维护类没有

- 规则:[出环](README.md#出环outmsg--gwcmd)、[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:prop
- 里程碑:M1.5
- 性质:任意命令序列处理完后,订单类命令(Place/Cancel/Amend/CancelAll/Query)与 `SendReport` 按 `(decision_seq, op)` 一一对应;维护类命令(`DeadManRefresh`、`Rotate`、`ResetRate`、`Resubscribe`、`SetEpoch`、`SessionKeepalive`、`MirrorRestore`)不产生 `SendReport`
- 生成:1–500 条命令,订单类与维护类混合;期间随机切换 kill 字、令连接 `NotOpen`/`TxBacklog`/`Io`、令 `encode` 失败、令限速满
- 参考:按 `decision_seq` 计数
- 例数:PR 1 000
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_200_one_report_per_order_cmd`

### VEN-201 Cancel/Amend 的目标不在槽镜像时回 `EncodeError`、不发帧

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`;槽 40 不在镜像中;槽 41 在镜像中但 `exch_order_id = 0`,`StubCfg{query_by_cid: false}`
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_201_missing_target`

| 行 | 命令 | 期望 |
|---|---|---|
| VEN-201.1 | `Cancel{target_slot: 40}` | `EncodeError`;`encode` 未被调用;无帧 |
| VEN-201.2 | `Amend{target_slot: 40}` | `EncodeError`;无帧 |
| VEN-201.3 | `Cancel{target_slot: 41}` | `encode` 返回 `UnknownTarget` → `EncodeError`;无帧 |

### VEN-202 Query 无法编码时回 `QueryUnsupported`

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M1.5
- 前置:同 VEN-201
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_202_query_unsupported`

| 行 | 命令 | 期望 |
|---|---|---|
| VEN-202.1 | `Query{target_slot: 40}` | `SendReport{op: Query, target_slot: 40, status: QueryUnsupported(8)}`;无帧 |
| VEN-202.2 | `Query{target_slot: 41}` | `QueryUnsupported`;无帧 |
| VEN-202.3 | `StubCfg{query_by_cid: true}`,`Query{target_slot: 41}` | `Written`;帧中带槽 41 的 client id |

### VEN-203 新单写槽镜像,Ack 补交易所 id,终态回报清除

- 规则:[出环](README.md#出环outmsg--gwcmd)、[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`,当前 epoch 5
- 步骤:
  1. A1 `Place{slot: 50, gen: 4, inst: S1}` 发出
  2. 回报 `O|<槽 50 的 cid>|EX-77|Ack|…`
  3. 回报 `O|<同 cid>|EX-77|Fill|…`(终态)
- 期望:
  - 步骤 1 后镜像 50 = `{gen: 4, epoch: 5, inst: S1, exch_order_id: 0, live: true}`
  - 步骤 2 后 `exch_order_id` = `EX-77` 的哈希(或数值),`IdTable` 有该条目
  - 步骤 3 后镜像 50 `live = false`;随后 `Cancel{target_slot: 50}` → `EncodeError`
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_203_mirror_lifecycle`

### VEN-204 冒烟单的帧与普通 Place 除 client id 外逐字节相同

- 规则:[出环](README.md#出环outmsg--gwcmd)
- 级别:unit
- 里程碑:M3
- 前置:`G0`;两条 Place 只差 `oflags` 的 `SMOKE` 位与槽号
- 步骤:
  1. 依次发出两条
- 期望:
  - 两帧把 client id 文本替换为同一占位后逐字节相同;两条都 `Written`;冒烟单写入槽镜像(`live = true`)
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_204_smoke_same_frame`

### VEN-205 发送顺序:帧 → `SendReport` → `RawTx`,并记 `TxTag` 与在途

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[产生与消费的事件](README.md#产生与消费的事件)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`;`tx_tags = Some`;`StubProto` 变体 `needs_per_order_signing() = true`(帧含签名)
- 步骤:
  1. 发出 A1 `Place{slot: 60}`
- 期望:
  - `Order` 日志:`SendReport{Written}` 入 rep 环先于 `RawTx` 追加;`TxTag{slot: 60}` 一条;`inflight[primary] = 1`
  - `RawTx` 记录头 `tsc = SendReport.sent_tsc`,`RawHdr.rflags` 含 `REDACTED`,payload 只有元数据(不含帧原文)
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_205_send_order_of_effects`

### VEN-206 `CancelAll`:有账户级全撤用一帧,没有则按镜像中有活动单的 inst 各发一帧,仍只回一条 `SendReport`

- 规则:[出环](README.md#出环outmsg--gwcmd)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`;A1 镜像中有活动单的 inst 为 S1(2 张)、S2(1 张);A2 有 S3(1 张)
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_206_cancel_all`

| 行 | `StubCfg.account_cancel_all` / 命令 | 期望 |
|---|---|---|
| VEN-206.1 | true / `CancelAll{account: A1, inst: 0}` | 1 帧;1 条 `SendReport{CancelAll, Written}` |
| VEN-206.2 | false / 同上 | `encode` 先以 `inst = 0` 返回 `Unsupported(CancelAll)`,随后以 `inst = S1`、`S2` 各编码一帧(共 2 帧,不含 S3);1 条 `SendReport{Written}` |
| VEN-206.3 | false,且 `inst_cancel_all = false` / 同上 | `inst = 0`、`S1`、`S2` 都返回 `Unsupported(CancelAll)`;随后 A1 的 3 张活动单展开为 `Cancel`:`encode_batch` 返回 `Ok` 时 1 帧,返回 `Unsupported` 时 3 帧(逐条 `encode`);1 条 `SendReport{CancelAll, Written}`,展开的 `Cancel` 不另回 `SendReport` |
| VEN-206.4 | VEN-206.3 且 `CANCEL_ALL_SWEEP = true` | 展开的帧之后提交 1 个 `ro_request(OpenOrders)` 请求;其响应(A1 的 1 张仍挂 + 1 张外来单)经 `decode_rest(Sweep)` 产出 1 条 `QueryResult` 入 rep 环 |
| VEN-206.5 | true,`StubCfg{rest_query: false}`,`order_channel(CancelAll) = Rest` 的桩变体 / 同上 | 无 WS 帧;1 个 REST 请求(`encode_rest`);1 条 `SendReport{CancelAll, Written}` |

### VEN-207 回报突发期间每处理一帧就检查一次出环

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:integ
- 里程碑:M1.5
- 前置:`G0`;A1 primary 一次读入 64 帧回报
- 步骤:
  1. 在第 10 帧处理完的时刻向出环写一条 A2 `Place`(RR),继续 `poll_once`
- 期望:
  - `Order` 日志中该 `Place` 的发送位于第 10 帧的事件之后、第 11 帧的事件之前
- 实现:`crates/hft-venue/tests/gw_loop.rs::ven_207_drain_after_each_frame`

### VEN-208 gw 各连接的轮询节拍

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[配置项](README.md#配置项)
- 级别:integ
- 里程碑:M1.5
- 前置:`G0`,`gw.aux_every = 8`,`gw.rest_idle_check_ms = 1000`,`StubCfg{user_stream: true}`;A1 有 primary、独立用户流、standby、2 个 REST 端点池(各 2 条连接,其中连接 130 有一个在途查单,其余空闲);上次空闲检查在第 1 圈前 1 ms,整个运行在 100 ms 内
- 步骤:
  1. 运行 80 圈
- 期望:
  - `SysCount.recvmsg`:primary 80、用户流 80、standby 10、连接 130 为 10(在途期间的 aux 圈)、其余 REST 连接 0;`epoll_pwait` 为 0
  - 每次 standby 或 REST socket 被读之后都紧跟一次 `drain_out`(插桩计数相等)
- 实现:`crates/hft-venue/tests/gw_loop.rs::ven_208_poll_cadence`

### VEN-209 gw 的空闲 REST 连接每 `rest_idle_check_ms` 只做一次存活检查

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`,`gw.rest_idle_check_ms = 1000`;A1 的 REST 连接 131、132 空闲;上次检查 `t_c`
- 实现:`crates/hft-venue/tests/gw_loop.rs::ven_209_rest_idle_check`

| 行 | `now` / 对端 | 期望 |
|---|---|---|
| VEN-209.1 | `t_c + 999 999 999` / 对端在线 | 131、132 的 `recvmsg` 为 0 |
| VEN-209.2 | `t_c + 10⁹` / 对端在线 | 各 1 次 `recvmsg`,之后各紧跟一次 `drain_out` |
| VEN-209.3 | `t_c + 10⁹` / 131 已被对端关闭 | 131 退役并发出补建 `BuildReq`;132 不受影响 |

### VEN-210 `wait_fds` 只在有在途请求时列入 REST 连接

- 规则:[运行时入口](README.md#运行时入口)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`;A1 有 primary 112、standby 113、REST 130(在途 1)、131(空闲)
- 步骤:
  1. 调 `wait_fds`
  2. 130 的响应到达并处理后再调 `wait_fds`
- 期望:
  - 步骤 1:fd 集合 = {112, 113, 130}
  - 步骤 2:= {112, 113}
- 实现:`crates/hft-venue/tests/gw_loop.rs::ven_210_wait_fds`

### VEN-211 每圈至多应用一条 acct 记录,排在最后一次 `drain_out` 之后

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:unit
- 里程碑:M3
- 前置:`G0`;acct 环积压 10 条记录
- 步骤:
  1. 运行 3 圈
- 期望:
  - 每圈恰好应用 1 条;`Order` 日志中每圈的 acct 应用位于该圈最后一次 `drain_out` 之后;3 圈后环中余 7 条
- 实现:`crates/hft-venue/tests/gw_loop.rs::ven_211_acct_one_per_loop`

### VEN-212 rep 环接近满时停读回报与 acct 记录,出环用预留空位继续

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`,`gw.cmds_per_loop = 16`;rep 环容量 64,消费者暂停;A1 primary 上有 5 帧回报,acct 环有 1 条记录;每行先把 rep 环填到所列空位数
- 实现:`crates/hft-venue/tests/gw_loop.rs::ven_212_rep_ring_reserve`

| 行 | rep 空位 / 出环 | 期望 |
|---|---|---|
| VEN-212.1 | 17 / 无 | 处理 1 帧回报(空位降到 16)后停读其余 4 帧;`ring_full` 不变 |
| VEN-212.2 | 16 / 3 条 Place | 不读回报帧、不应用 acct 记录;3 条 Place 照常发出并各推一条 `SendReport`;`ring_full` +1 |
| VEN-212.3 | 0 / 3 条 Place | 不从出环取命令(3 条留在出环);不读回报;`ring_full` +1;`LoopStat.did_work = false` |

### VEN-213 本系统 client id 直接解析出 `(epoch, slot, gen)`

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`,当前 epoch 5;A1 的连接 112
- 步骤:
  1. 112 上收到 `O|<epoch 5、A1、槽 70、gen 2 的 cid>|EX-1|Ack|…`
- 期望:
  - rep 环 `OrderEvent{what: Ack, epoch: 5, slot: 70, gen: 2, exch_order_id: EX-1 的值}`
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_213_cid_resolves`

### VEN-214 client id 的 epoch 接受窗口

- 规则:[出环](README.md#出环outmsg--gwcmd)、[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M3
- 前置:`G0`;恢复后收到 `SetEpoch{epoch: 70, accept_prev: true, prev_epoch: 5}`(跨 AZ 恢复的安全间隔使 `prev_epoch ≠ epoch − 1`);`IdTable` 为空;回报均为不带成交的 `Ack`,venue/account 与连接一致
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_214_epoch_window`

| 行 | 回报 cid 的 epoch / 此前的 `SetEpoch` | 期望 `OrderEvent` |
|---|---|---|
| VEN-214.1 | 70 / 上述 | `epoch: 70`,槽按 cid |
| VEN-214.2 | 5 / 上述 | `epoch: 5`,槽按 cid |
| VEN-214.3 | 69 / 上述 | 按解析失败处理:`slot = UNRESOLVED` |
| VEN-214.4 | 5 / 其后又收到 `SetEpoch{epoch: 70, accept_prev: false, prev_epoch: 5}` | `slot = UNRESOLVED` |
| VEN-214.5 | 70,但 cid 中的 account = A2 而回报来自 A1 的连接 | `slot = UNRESOLVED` |

### VEN-215 解析失败的回报查 `IdTable`,再失败给 `UNRESOLVED`

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[§6](README.md#6-exch-id--slot-私有表idtable)
- 级别:table
- 里程碑:M3
- 前置:`G0`;`IdTable` 有 `EX-9 → (epoch 5, 槽 80, gen 1)`;回报都不带成交
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_215_idtable_fallback`

| 行 | 回报 | 期望 |
|---|---|---|
| VEN-215.1 | `O\|\|EX-9\|CancelAck\|…`(无 cid) | `OrderEvent{CancelAck, epoch: 5, slot: 80, gen: 1}` |
| VEN-215.2 | `O\|web-123\|EX-9\|CancelAck\|…`(非本系统 cid) | 同上 |
| VEN-215.3 | `O\|\|EX-404\|Ack\|…` | `OrderEvent{Ack, slot: UNRESOLVED, exch_order_id: EX-404 的值}`,立即入环 |

### VEN-216 解析不到槽的成交先暂存,Ack 在期限内到达则按原顺序归属入环

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M3
- 前置:`G0`,`gw.unmatched_hold_ms = 5000`;A1 槽 90 的 Place 已发出,Ack 未到
- 步骤:
  1. `t = 0`:用户流先到两笔成交 `O||EX-5|PartialFill|1|1|…`、`O||EX-5|PartialFill|2|3|…`
  2. `t = 3 s`:Ack `O|<槽 90 cid>|EX-5|Ack|…`
- 期望:
  - 步骤 1 后 rep 环没有这两笔
  - 步骤 2:rep 环依次 `Ack{slot: 90}`、`PartialFill{slot: 90, cum_filled: 1}`、`PartialFill{slot: 90, cum_filled: 3}`;无 `ExternalFill`
- 实现:`crates/hft-venue/tests/gw_hold.rs::ven_216_hold_released_by_ack`

### VEN-217 暂存超过 `unmatched_hold_ms` 才转 `ExternalFill{Unknown}`

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M3
- 前置:`G0`,`gw.unmatched_hold_ms = 5000`;`t = 0` 收到无 cid、`exch_order_id = EX-6` 的 `Fill{inst: S1, side: Buy, qty: 2, px: 101.5, fee: 0.01, fee_asset: USDT, ex_ts_ns: 777}`(qty 2 = raw 2000,指数 −3;px 101.5 = raw 10150,指数 −2;fee 0.01 = raw 10000,指数 −6),之后无 Ack;每行在所列时刻 `poll_once`
- 实现:`crates/hft-venue/tests/gw_hold.rs::ven_217_hold_timeout`

| 行 | `now` | 期望 |
|---|---|---|
| VEN-217.1 | 5 × 10⁹ | rep 环无新元素 |
| VEN-217.2 | 5 × 10⁹ + 1 | 恰好一条 `ExternalFill{cause: Unknown, account: A1, inst: S1, side: Buy, qty: 2, px: 101.5, fee: 0.01, fee_asset: USDT, exch_order_id: EX-6, trade_id: 0, ex_ts_ns: 777}` |
| VEN-217.3 | 6 × 10⁹ | 无更多元素 |

### VEN-218 暂存超时的 `FeeOnly` 以 `UNRESOLVED` 原样入环

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M3
- 前置:`G0`,`gw.unmatched_hold_ms = 5000`
- 步骤:
  1. `t = 0` 收到无 cid 的 `O||EX-7|FeeOnly|0|…|0.02|BNB|…`(fee 0.02 BNB = raw 2000000,指数 −8);`poll_once` 于 `5 × 10⁹ + 1`
- 期望:
  - rep 环一条 `OrderEvent{what: FeeOnly, slot: UNRESOLVED, exch_order_id: EX-7, fee: 0.02, fee_asset: BNB}`;无 `ExternalFill`
- 实现:`crates/hft-venue/tests/gw_hold.rs::ven_218_fee_only_timeout`

### VEN-219 暂存缓冲满时最老一条立即按超时处理并计数

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`G0`,`gw.unmatched_hold_cap = 256`;A1 暂存为空;每行在 `t = 0` 连续收到所列条数的无 cid 成交(各不同 exch id)
- 实现:`crates/hft-venue/tests/gw_hold.rs::ven_219_hold_full`

| 行 | 条数 | 期望 |
|---|---|---|
| VEN-219.1 | 256 | 暂存 256 条;无 `ExternalFill`;`hold_evicted = 0` |
| VEN-219.2 | 257 | 第 257 条到达时第 1 条立即转 `ExternalFill{Unknown}`;暂存 256 条;`hold_evicted = 1` |
| VEN-219.3 | A1 暂存 256 条时 A2 收到 1 条 | A2 的那条进 A2 的暂存;A1 不受影响 |

### VEN-220 venue 直接给出的 `ExternalFill` 与非成交的外来回报立即入环

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M3
- 前置:`G0`
- 实现:`crates/hft-venue/tests/gw_hold.rs::ven_220_immediate_paths`

| 行 | 回报 | 期望 |
|---|---|---|
| VEN-220.1 | `X\|Liquidation\|…` | 同一轮 rep 环 `ExternalFill{cause: Liquidation}`;暂存为空 |
| VEN-220.2 | `O\|web-1\|EX-8\|Ack\|…`(他人订单,表中无) | 同一轮 `OrderEvent{Ack, slot: UNRESOLVED}` |
| VEN-220.3 | `O\|\|EX-8\|CancelAck\|…` | 同一轮 `OrderEvent{CancelAck, slot: UNRESOLVED}` |

### VEN-221 可忽略与未知的用户流事件只计数,未知事件按规则写 Note

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M3
- 前置:`G0`,`gw.unknown_evt_note_every = 1000`;每行在 A1 用户流上喂 n 条同类事件
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_221_skip_events`

| 行 | 事件 / n | 期望 |
|---|---|---|
| VEN-221.1 | `E\|listenKeyExtended`(名单内) / 10 000 | `user_evt_ignored{V7} = 10000`;Note 0 条 |
| VEN-221.2 | `E\|fooBar`(未知) / 1 | `user_evt_unknown{V7} = 1`;source = `Venue` 的未知事件 Note 1 条,`v` 带 `"fooBar"` |
| VEN-221.3 | 未知 / 999 | Note 1 条 |
| VEN-221.4 | 未知 / 1000 | Note 2 条(第 1 条与第 1000 条) |
| VEN-221.5 | 未知 / 1001 | Note 2 条 |
| VEN-221.6 | 未知 / 10 000 | Note 11 条 |
| VEN-221.7 | 以上每行 | 零 `ConnState`、零 `AcctReq`、无查单;每条事件各有一条 `RawRx` |

### VEN-222 回报解码失败推 `Degraded{REPORT_DECODE}`,同一连接连续失败达阈值按 Down

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`G0`,`gw.report_decode_down_after = 3`;A1 primary 112;`Z|…` 为类别已知、字段坏的回报;每行从计数为 0 开始
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_222_report_decode`

| 行 | 112 上的帧(时刻) | 期望 |
|---|---|---|
| VEN-222.1 | `Z`(0 s) | `ConnState{Degraded, account: A1, role: Order, conn: 112, reason: 12}`;该帧照写 `RawRx`;连接仍活跃 |
| VEN-222.2 | `Z`(0 s)、`Z`(10 s)、`Z`(20 s) | 前两条各一条 `Degraded`;第三条后 112 退役,推 `ConnState{Down, conn: 112, role: Order}` |
| VEN-222.3 | `Z`(0 s)、可解码的回报(5 s)、`Z`(10 s)、`Z`(20 s) | 不退役(成功解码清零计数) |
| VEN-222.4 | `Z`(0 s)、`Z`(30 s)、`Z`(61 s) | 不退役(第三次与这一轮的第一次相隔 > 60 s,以 61 s 为新一轮第一次) |
| VEN-222.5 | `Z`(0 s)、`Z`(30 s)、`Z`(60 s) | 退役(相隔恰为 60 s) |
| VEN-222.6 | `Z`(0 s)、`Z`(10 s) | 两条 `Degraded`;不退役 |

### VEN-223 "结果未知"与未登记码产出 `OutcomeUnknown`,不产出 Reject

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`;A1 槽 95 `gen = 1` 的 Place 在途;每行一条对该请求的响应
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_223_outcome_unknown`

| 行 | 响应码 | 期望 |
|---|---|---|
| VEN-223.1 | `-1007`(登记为结果未知) | `OrderEvent{what: OutcomeUnknown(11), slot: 95, gen: 1, reject_code: 0xFFFF_FC11}`,价量与成交字段为 0;无 `Reject` |
| VEN-223.2 | `-9999`(未登记) | `OutcomeUnknown`,`reject_code = 0xFFFF_D8F1` |
| VEN-223.3 | `-2010`(登记为确定拒绝) | `OrderEvent{what: Reject, reject_code: 0xFFFF_F826}`;无 `OutcomeUnknown` |
| VEN-223.4 | 以上每行 | `inflight[112]` 减 1 |

### VEN-224 响应中的限速观测校正发送侧计数

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`;A1 本地计数 used = 2 / 10
- 步骤:
  1. 收到带 `RateObs{used: 9, limit: 10, window_ms: 1000}` 的响应
  2. 出环 2 条 A1 `Place`
- 期望:
  - 第一条 `Written`(used 9 → 10),第二条 `RateLimited`
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_224_rate_obs_correction`

### VEN-225 回报给出的提示立即排一次账户查询

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:table
- 里程碑:M3
- 前置:`G0`;A1 无在途查询序列,下一次计划 Margin 在 10 s 后、Config 在 500 s 后
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_225_hints`

| 行 | 回报 | 期望 |
|---|---|---|
| VEN-225.1 | `H\|MARGIN_CALL` | 下一个 aux 检查时机推 `AcctReq{kind: Margin, account: A1, why: MarginNow(2)}` |
| VEN-225.2 | `H\|CFG` | 下一个 aux 检查时机推 `AcctReq{kind: Config, why: ConfigRecheck(3)}` |

### VEN-226 扣费资产与规则不符时计数并写 Note,不改事件

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M3
- 前置:`G0`;`StubProto` 对某成交回报给出 `fee_rule_deviation = true`
- 步骤:
  1. 收到该成交回报
- 期望:
  - `OrderEvent{Fill}` 的 `fee`、`fee_asset` 与报文一致;计数 `fee_rule_deviation{V7} = 1`;source = `Venue` 的 Meta Note 一条
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_226_fee_rule_deviation`

### VEN-227 推送型保证金数据由回报解码直接产出,并推后兜底期限

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:unit
- 里程碑:M3
- 前置:`G0`,生效 `margin_data = Push`,`gw.margin_push_stale_ms = 5000`
- 步骤:
  1. `t = 1 s` 收到 `MP|…`
- 期望:
  - rep 环 `MarginReport{source: Push}`(头 + 条目在 rep-bulk 连续);`next_margin_tsc = 6 × 10⁹`;没有 `AcctReq`
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_227_margin_push`

### VEN-228 REST 查单结果产出带累计量的 `QueryResult`

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M3
- 前置:`G0`,`StubCfg{rest_query: true}`;A1 对槽 96(gen 4)的 `Query` 经 `encode_rest` 走 REST 端点 0(连接 130 在途);响应体不带 client id
- 步骤:
  1. 第 8 圈 aux 轮询读到响应(`cum = 3` = raw 3000,指数 −3;`avg = 100.2` = raw 10020,指数 −2)
- 期望:
  - `decode_rest` 收到 `RestOrigin{what: Order, req: Some(ReqRef{slot: 96, gen: 4, op: Query})}`;rep 环 `OrderEvent{what: QueryResult, slot: 96, gen: 4, cum_filled: 3, avg_px: 100.2, detail: 2}`;`inflight` 中该请求清除
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_228_query_result`

### VEN-229 请求超过 `inflight_timeout_ms` 无响应推一次 `Degraded{INFLIGHT_TIMEOUT}` 并移出在途

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`,`gw.inflight_timeout_ms = 3000`;A1 在 `t = 0` 发出 `Place{slot: 97}`,无响应;每行在所列时刻 `poll_once`
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_229_inflight_timeout`

| 行 | `now` | 期望 |
|---|---|---|
| VEN-229.1 | 3 × 10⁹ | 无 `Degraded`;`inflight[112] = 1` |
| VEN-229.2 | 3 × 10⁹ + 1 | `ConnState{Degraded, account: A1, role: Order, conn: 112, reason: 10}` 一条;`inflight[112] = 0` |
| VEN-229.3 | 10 × 10⁹ | 无第二条 `Degraded` |
| VEN-229.4 | 11 × 10⁹ 时迟到的响应到达 | 照常解码入环;`inflight[112]` 仍为 0(不再减) |

### VEN-230 primary 致命:强制切到已登录的 standby 并推 `Down{role=Order}`

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`;A1 primary 112(在途 2)、standby 113 已登录
- 步骤:
  1. `rst(112)`,`poll_once`
  2. 出环 A1 `Place`
- 期望:
  - 步骤 1:rep 环 `ConnState{Down, account: A1, role: Order, conn: 112, reason: RESET}`;113 成为 primary;发出补建 `BuildReq`
  - 步骤 2:`SendReport{Written, conn: 113}`
- 实现:`crates/hft-venue/tests/gw_conn.rs::ven_230_primary_fatal_failover`

### VEN-231 没有 standby 时 primary 致命后下单回 `NoConn`,直到新连接交接

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`;A1 只有 primary 112
- 步骤:
  1. `eof(112)`,`poll_once`;出环 A1 `Place`
  2. 交接新连接 114,出环 A1 `Place`
- 期望:
  - 步骤 1:`Down{conn: 112}`;那条 Place `NoConn`
  - 步骤 2:`ConnState{Up, account: A1, role: Order, conn: 114}`;Place `Written`,`conn = 114`
- 实现:`crates/hft-venue/tests/gw_conn.rs::ven_231_no_standby`

### VEN-232 standby 与独立用户流连接致命时的处理

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M3
- 前置:`G0`,`StubCfg{user_stream: true}`;A1 primary 112、standby 113、用户流 120(私有频道归属)
- 实现:`crates/hft-venue/tests/gw_conn.rs::ven_232_standby_user_fatal`

| 行 | 事件 | 期望 |
|---|---|---|
| VEN-232.1 | `rst(113)` | 113 退役;`ConnState{Down, account: A1, role: Order, conn: 113}`;补建;primary 不变 |
| VEN-232.2 | `rst(120)` | `ConnState{Down, account: A1, role: UserStream, conn: 120}`;补建用户流连接 |

### VEN-233 下单与用户流连接交接后推 `Up`

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[产生与消费的事件](README.md#产生与消费的事件)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`(A1 尚无连接)
- 步骤:
  1. 依次交接 primary 112、standby 113,`poll_once`
- 期望:
  - rep 环 `ConnState{Up, account: A1, role: Order, conn: 112}` 与 `{Up, …, conn: 113}` 各一条
- 实现:`crates/hft-venue/tests/gw_conn.rs::ven_233_up_on_install`

### VEN-234 计划切换只在 primary 无在途时发生

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`G0`,`gw.rotate_wait_max_ms = 60000`;A1 primary 112 于 `t_r` 到计划轮换时刻,standby 113 `Open`;`StubCfg{user_stream: true}`(私有频道不在 112)
- 实现:`crates/hft-venue/tests/gw_conn.rs::ven_234_scheduled_switch`

| 行 | 112 的在途 | 期望 |
|---|---|---|
| VEN-234.1 | 0 | `t_r` 那一圈 `swap_primary`;`ConnState{Rotated, account: A1, role: Order, conn: 113, reason: ROTATE_DUE}`;112 在 `t_r + 1 s` 退役,其间 112 上的回报照常处理 |
| VEN-234.2 | 1,在 `t_r + 2 s` 收到响应 | `t_r + 2 s` 那一圈切换 |
| VEN-234.3 | 1(`t_r − 1 s` 发出),一直无响应;`gw.inflight_timeout_ms = 120000` | `t_r + 60 s` 放弃本次(不切换,无 `Rotated`);该请求于 `t_r + 119 s + 1 ns` 超时移出在途;`t_r + 120 s` 重新尝试并切换 |
| VEN-234.4 | 1,在 `t_r + 60 s` 收到响应 | 该圈切换(恰为上限时不放弃) |

### VEN-235 `GwCmd{Rotate}` 与服务端下线预告提前触发切换,仍遵守无在途

- 规则:[出环](README.md#出环outmsg--gwcmd)、[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M3
- 前置:`G0`;A1 primary 112 无在途,standby 113 `Open`,计划轮换在 10 h 后
- 实现:`crates/hft-venue/tests/gw_conn.rs::ven_235_early_switch`

| 行 | 触发 | 期望 |
|---|---|---|
| VEN-235.1 | 出环 `Rotate{account: A1}` | 本圈切换;`Rotated{conn: 113, reason: 0}`;无 `SendReport` |
| VEN-235.2 | 112 收到 `BYE` | 本圈切换;`Rotated{conn: 113, reason: SERVER_HINT}` |
| VEN-235.3 | 出环 `Rotate`,112 有 1 个在途 | 在途清零的那一圈才切换 |

### VEN-236 死手刷新覆盖槽镜像中有活动单的 inst,含冒烟单

- 规则:[出环](README.md#出环outmsg--gwcmd)、[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:table
- 里程碑:M3
- 前置:`G0`;A1 镜像:S1 有普通单,S2 只有一张冒烟单(`SMOKE`),S3 无活动单;每行出环 `DeadManRefresh{account: A1, arg: 30000}`
- 实现:`crates/hft-venue/tests/gw_deadman.rs::ven_236_deadman_refresh`

| 行 | `StubCfg.dead_man` / 情形 | 期望 |
|---|---|---|
| VEN-236.1 | `CountdownPerInst` / 有活动单 | `dead_man_refresh` 收到 `insts = [S1, S2]`、`timeout_ms = 30000`;发出帧;无 `SendReport` |
| VEN-236.2 | `CountdownPerInst` / A1 无任何活动单 | 不发任何帧 |
| VEN-236.3 | `CountdownAccount{via: Rest}` / A1 无任何活动单 | 照常提交一个 REST 请求 |
| VEN-236.4 | `CountdownAccount{via: Rest}` / 响应 HTTP 500 | 下一个 aux 圈读到后推 `ConnState{Degraded, account: A1, role: Order, reason: 8}` |
| VEN-236.5 | `None` | `dead_man_refresh` 返回 `Unsupported` → `Degraded{reason: 8}` |

### VEN-237 维护类命令:`Resubscribe`、`SessionKeepalive`、`SetEpoch`

- 规则:[出环](README.md#出环outmsg--gwcmd)
- 级别:table
- 里程碑:M3
- 前置:`G0`,`StubCfg{user_stream: true, keepalive: true}`;A1 的私有频道归属为用户流连接 120
- 实现:`crates/hft-venue/tests/gw_maint.rs::ven_237_maintenance_ops`

| 行 | 命令 / 情形 | 期望 |
|---|---|---|
| VEN-237.1 | `Resubscribe{A1}` | 120 上发出 `private_subscribe(unsub = false)` 的帧;113 不发 |
| VEN-237.2 | `SessionKeepalive{A1}` / 响应 200 | 该账户 REST 池提交一个会话维护请求;`RawTx` 带 `REDACTED`;无事件 |
| VEN-237.3 | `SessionKeepalive{A1}` / 响应 400 | `ConnState{Degraded, account: A1, role: UserStream, reason: 8}` |
| VEN-237.6 | `SessionKeepalive{A1}`,`StubCfg{keepalive: false}` | 不提交请求;计数 `keepalive_unsupported` 加 1;无事件 |
| VEN-237.7 | `SessionKeepalive{A1}` / 响应码 `-1125`(`ctl = UserStreamEnded`) | `ConnState{Down, account: A1, role: UserStream}`;用户流连接 120 退役并发出补建 `BuildReq` |
| VEN-237.4 | `SetEpoch{epoch: 6, accept_prev: true, prev_epoch: 5}`,之后 `Place` | 新单 client id 的 epoch = 6 |
| VEN-237.5 | 以上每行 | 无 `SendReport` |

### VEN-238 `MirrorRestore` 重建槽镜像与 `IdTable`,随后按原 epoch 编码

- 规则:[出环](README.md#出环outmsg--gwcmd)
- 级别:table
- 里程碑:M3
- 前置:`G0`;`SetEpoch{epoch: 9, accept_prev: true, prev_epoch: 8}` 已处理;OMS 槽数 2048;`StubCfg{query_by_cid: false}`
- 实现:`crates/hft-venue/tests/gw_maint.rs::ven_238_mirror_restore`

| 行 | 命令 | 期望 |
|---|---|---|
| VEN-238.1 | `MirrorRestore{slot: 30, gen: 2, epoch: 8, inst: S1, exch_order_id: 555}`,随后 `Query{target_slot: 30}` | 无 `SendReport`(对 MirrorRestore);Query 帧中 client id 的 epoch = 8、槽 30、gen 2,交易所 id = 555;`Written` |
| VEN-238.2 | 同 .1,随后收到只带 `555` 的回报 | 经 `IdTable` 解析为 `(8, 30, 2)` |
| VEN-238.3 | `MirrorRestore{slot: 31, exch_order_id: 0, …}`,随后 `Query{target_slot: 31}` | `QueryUnsupported` |
| VEN-238.4 | `MirrorRestore{slot: 2048, …}` | `ConnState{Degraded, account: A1, reason: 13}`;镜像不变 |
| VEN-238.5 | `MirrorRestore{inst: 9999, …}`(非本 venue) | `ConnState{Degraded, reason: 13}` |

### VEN-239 空闲预热默认关闭,打开后只预热不发送

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`G0`;A1 primary 112;每行空闲运行 5 ms
- 实现:`crates/hft-venue/tests/gw_loop.rs::ven_239_warm`

| 行 | `gw.warm_interval_ns` | 期望 |
|---|---|---|
| VEN-239.1 | 0 | `Conn::warm` 调用 0 次 |
| VEN-239.2 | 1 000 000 | 空闲满 1 ms 后每满 1 ms 调一次 `warm`(5 ms 内 4 次,参数为同一 `warm_frame`);`MemPeer` 收到 0 字节;rep 环与 raw 无新元素 |

### VEN-240 新单的 client id 由当前 epoch、venue、账户、槽与代数生成

- 规则:[出环](README.md#出环outmsg--gwcmd)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`,当前 epoch 5
- 步骤:
  1. 发出 A2 `Place{slot: 33, gen: 7}`,取帧中的 client id 文本
- 期望:
  - 文本 = `"st-"` + `ClientOrderId{epoch: 5, venue: V7, account: A2, slot: 33, gen: 7}` 的规范 24 位十六进制;`StubProto::parse_cid` 解回同一值
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_240_cid_generation`

### VEN-241 出环命令的账户不属于本 gw 时回 `NoConn` 并计数

- 规则:[出环](README.md#出环outmsg--gwcmd)、[§4b](README.md#4b-私有频道归属与热备连接)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`(服务 A1、A2)
- 步骤:
  1. 出环 A9(Sim)的 `Place{slot: 5}`
- 期望:
  - `SendReport{op: Place, slot: 5, account: A9, status: NoConn}`;无帧、`encode` 未被调用;计数 `cmd_bad_account = 1`
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_241_foreign_account`

### VEN-242 回报 sink 的每类输出进 rep 环的对应元素

- 规则:[产生与消费的事件](README.md#产生与消费的事件)
- 级别:table
- 里程碑:M3
- 前置:`G0`;`StubProto::decode_report` 对每行的帧调用一次所列 sink 方法
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_242_sink_to_rep`

| 行 | sink 调用 | 期望 rep 环 |
|---|---|---|
| VEN-242.1 | `order(&OrderEvent{Fill, …})` | `RepMsg::OrderEvent`,`cum_filled`、`avg_px` 原样 |
| VEN-242.2 | `position(..)`;`balance(..)` | `RepMsg::PositionReport`;`RepMsg::BalanceReport` |
| VEN-242.3 | `funding_settle(..)` | `RepMsg::FundingSettle` |
| VEN-242.4 | `external_fill(..)` | `RepMsg::ExternalFill`(不经暂存) |
| VEN-242.5 | `margin(hdr{n: 3}, items)` | `RepMsg::MarginReport(hdr)`,rep-bulk 中紧随 3 条 `MarginItem` |
| VEN-242.6 | `account_config(hdr{n_lev: 2}, lev)` | `RepMsg::AccountConfigReport(hdr)`,rep-bulk 中 2 条 `LeverageItem` |

### VEN-243 回报标明时钟越界或交易所限速时推对应的 `Degraded`

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[产生与消费的事件](README.md#产生与消费的事件)
- 级别:table
- 里程碑:M3
- 前置:`G0`;A1 槽 98 的 Place 在途于 primary 112;`StubProto` 把 `R|<req>|-1021` 判为确定拒绝且在 `ReportOutcome` 中标明时钟越界,把 `R|<req>|-1015` 判为确定拒绝且标明交易所限速
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_243_clock_skew_rate_limit`

| 行 | 响应 | 期望 |
|---|---|---|
| VEN-243.1 | `-1021` | `OrderEvent{Reject, slot: 98, reject_code: 0xFFFF_FC03}` 与 `ConnState{Degraded, account: A1, role: Order, conn: 112, reason: 9}`;连接不退役 |
| VEN-243.2 | `-1015` | `OrderEvent{Reject, reject_code: 0xFFFF_FC09}` 与 `ConnState{Degraded, …, reason: 11}` |

### VEN-244 primary 无帧时 poll 完也检查一次出环

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`,`StubCfg{user_stream: false}`;A1、A2 的 primary 都无数据;运行在 aux 圈之外
- 步骤:
  1. 运行 1 圈
- 期望:
  - `drain_out` 被调用 2 次(每账户 primary poll 之后各一次);出环中的 1 条 Place 在第一次调用时发出
- 实现:`crates/hft-venue/tests/gw_loop.rs::ven_244_drain_when_idle`

### VEN-245 gw 的 `has_pending`

- 规则:[运行时入口](README.md#运行时入口)
- 级别:table
- 里程碑:M1.5
- 前置:`G0`;rep 环有充足空位(除 .5 行)
- 实现:`crates/hft-venue/tests/gw_loop.rs::ven_245_has_pending`

| 行 | 状态 | 期望 |
|---|---|---|
| VEN-245.1 | 出环、acct 环为空,各连接 TLS 缓冲无未处理字节 | false |
| VEN-245.2 | 出环有 1 条 | true |
| VEN-245.3 | acct 环有 1 条记录 | true |
| VEN-245.4 | primary 的 TLS 缓冲有一帧未处理 | true |
| VEN-245.5 | 同 .4,但 rep 环空位 ≤ `gw.cmds_per_loop` | false |

### VEN-246 填帧用 TSC 换算的墙钟与 `gw.stale_guard_ms` 过期窗口

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[配置项](README.md#配置项)
- 级别:unit
- 里程碑:M1.5
- 前置:`G0`,`gw.stale_guard_ms = 1500`;`TscUtc` 把 `now_tsc = 1_700_000_000_123_456_789` 换算为 `utc_ns` 同值;`StubProto` 帧中的过期字段 = `WallClock.unix_ms` + 过期窗口
- 步骤:
  1. 在该 `now_tsc` 发出一条 `Place`
- 期望:
  - `encode` 收到 `WallClock{unix_ms: 1700000000123, unix_us: 1700000000123456}`;帧中过期字段 = 1700000000123 + 1500;gw 线程上 `clock_gettime` 调用数为 0
- 实现:`crates/hft-venue/tests/gw_send.rs::ven_246_wall_clock_and_expiry`

### VEN-247 gw 从不自行刷新死手

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M3
- 前置:`G0`,`StubCfg{dead_man: CountdownPerInst}`;A1 在 S1 上有活动单
- 步骤:
  1. 不投递任何 `DeadManRefresh`,运行 10 min(时钟每圈推进 1 ms)
- 期望:
  - `CallLog` 中 `dead_man_refresh` 调用数为 0;两条下单连接与 REST 池都没有发出死手帧或请求
- 实现:`crates/hft-venue/tests/gw_deadman.rs::ven_247_no_self_refresh`

### VEN-248 用户流终止与私有连接维护预告的处置

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[§4b](README.md#4b-私有频道归属与热备连接)
- 级别:table
- 里程碑:M3
- 前置:`G0`;A1 primary 112(私有频道归属)、standby 113 已登录;A2 `StubCfg{user_stream: true}`,用户流连接 121;按行注入一帧
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_248_stream_end_and_shutdown`

| 行 | 帧 / 连接 | 期望 |
|---|---|---|
| VEN-248.1 | 用户流终止事件(`ctl = UserStreamEnded`)/ A2 的 121 | `ConnState{Down, account: A2, role: UserStream}`;121 退役,`BuildReq{role: UserStream}`;新连接交接、订阅回执齐后 `ConnState{Up, role: UserStream}`,并各排一次 Margin 与 Config 查询 |
| VEN-248.2 | 同上 / A1 的 112(私有频道在下单连接上) | `ConnState{Down, account: A1, role: UserStream}`;112 不退役,重发 `private_subscribe(unsub = false)`;回执齐后 `Up` 并排查询 |
| VEN-248.3 | `E\|SHUTDOWN`(`ctl = ServerShutdown`)/ 112,无在途 | 立即计划切换:私有频道迁到 113 后 `swap_primary`,`ConnState{Rotated, conn: 113, reason: SERVER_HINT}` |
| VEN-248.4 | 普通成交推送 / 112 | 无 `ConnState` |

### VEN-249 批量响应中逐条的 OutcomeUnknown 原样入环

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:unit
- 里程碑:M3
- 前置:`G0`;A1 的 3 条 Place(槽 40、41、42)经 `encode_batch` 合成 1 帧已发出;`StubProto` 对批量响应逐条产出 `Ack{40}`、`OrderEvent{what: OutcomeUnknown, slot: 41, reject_code: 50004}`、`Reject{42}`,`ReportOutcome.outcome_unknown = None`
- 步骤:
  1. 注入该批量响应
- 期望:
  - rep 环依次 3 条 `OrderEvent`(41 为 `OutcomeUnknown`);gw 不再另行合成 `OutcomeUnknown`;`inflight` 减 1
- 实现:`crates/hft-venue/tests/gw_report.rs::ven_249_batch_outcome_unknown_passthrough`

### VEN-250 Config 序列在下单连接与私有频道首次就绪后执行一次,只产出一条报告

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:integ
- 里程碑:M3
- 前置:`G0` + `C0` 组装;A1 primary 112 已交接,私有频道订阅已发、回执未到;`acct_queries(Config)` 为 3 个请求(端点 0、0、1)
- 步骤:
  1. 运行 16 圈
  2. 回私有频道订阅回执,再运行 16 圈;acct-fetch 依次收到 3 个 200 响应
- 期望:
  - 步骤 1:acctreq 环为空
  - 步骤 2:恰好一条 `AcctReq{kind: Config, account: A1, why: Startup(5)}`;acct-fetch 按 step 0、1、2 的顺序发请求,第 3 个用端点 1 的池;rep 环恰好一条 `AccountConfigReport`
- 实现:`crates/hft-venue/tests/acct.rs::ven_250_startup_config`

### VEN-251 `Poll` 型保证金按 `max(interval_ms, margin_poll_min_ms)` 周期查询

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`G0` + `C0`;生效 `margin_data = Poll{interval_ms: 2000}`,`gw.margin_poll_min_ms = 1000`;上一个 Margin 序列在 `t_d` 完成;每行在所列时刻跑一个 aux 圈
- 实现:`crates/hft-venue/tests/acct.rs::ven_251_margin_poll_period`

| 行 | `now` | 期望 |
|---|---|---|
| VEN-251.1 | `t_d + 1 999 999 999` | 无 Margin `AcctReq` |
| VEN-251.2 | `t_d + 2 × 10⁹` | 一条 `AcctReq{kind: Margin, why: Scheduled(1)}` |
| VEN-251.3 | 改为 `Poll{interval_ms: 1500}`;aux 圈于 `t_d + 1.5 × 10⁹` | 一条 Margin `AcctReq` |

### VEN-252 `AcctQuery.every = N` 的请求只在 `poll_no` 为 N 的倍数时执行

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:unit
- 里程碑:M3
- 前置:`C0`;`acct_queries(Margin)` = [`{step 0, every 1}`, `{step 1, every 6}`]
- 步骤:
  1. 依次投递 `AcctReq{Margin, poll_no: 0…12}`,每个序列都成功
- 期望:
  - step 1 的请求恰好在 `poll_no` = 0、6、12 时发出;step 0 每次发出;每个序列都只产出一份 `MarginReport`
- 实现:`crates/hft-venue/tests/acct.rs::ven_252_every_n`

### VEN-253 `Push` 型只在推送过期后兜底查询

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`G0` + `C0`,生效 `margin_data = Push`,`gw.margin_push_stale_ms = 5000`;最后一次账户推送在 `t_p`;`acct_queries(Margin)` 只有一个 `push_fallback = true` 的请求
- 实现:`crates/hft-venue/tests/acct.rs::ven_253_push_fallback`

| 行 | 过程 | 期望 |
|---|---|---|
| VEN-253.1 | aux 圈于 `t_p + 4 999 999 999` | 无 `AcctReq` |
| VEN-253.2 | aux 圈于 `t_p + 5 × 10⁹` | 一条 `AcctReq{Margin, why: PushStale(4)}`;序列完成产出一份独立的 `MarginReport` |
| VEN-253.3 | .2 的序列在 `t_c` 完成,之后仍无推送 | 下一次兜底在 `t_c + 5 × 10⁹` |
| VEN-253.4 | .2 之后、`t_c + 5 s` 之前收到推送 | 兜底期限改为该推送时刻 + 5 s |

### VEN-254 账户查询的优先级与"同时至多一个序列"

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:table
- 里程碑:M3
- 前置:`G0` + `C0`;A1 同一 aux 检查时机上同时满足所列条件
- 实现:`crates/hft-venue/tests/acct.rs::ven_254_priority`

| 行 | 同时待执行 | 期望第一个 `AcctReq` 的 `why` |
|---|---|---|
| VEN-254.1 | `MarginNow`、`ConfigRecheck`、到期 Margin、到期 Config | `MarginNow` |
| VEN-254.2 | `ConfigRecheck`、到期 Margin、到期 Config | `ConfigRecheck` |
| VEN-254.3 | 到期 Margin、到期 Config | Margin `Scheduled` |
| VEN-254.4 | .1 的序列在途时又收到 `MarginNow` | 不推第二条 `AcctReq`;在途序列结束后的下一个检查时机执行 |
| VEN-254.5 | A1 序列在途,A2 到期 Margin | A2 的 `AcctReq` 照常推出(按账户独立) |

### VEN-255 序列开始时预记权重,余量 < 20% 推迟 Margin/Config

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:table
- 里程碑:M3
- 前置:`G0` + `C0`;A1 的 REST 权重桶 100 / 60 000 ms;Margin 序列两个请求权重 3 + 2;每行开始时桶已用量如列
- 实现:`crates/hft-venue/tests/acct.rs::ven_255_weight_and_defer`

| 行 | 已用 / 待执行 | 期望 |
|---|---|---|
| VEN-255.1 | 0 / 到期 Margin | 推 `AcctReq`;已用变为 5 |
| VEN-255.2 | 80 / 到期 Margin | 余量 20% → 推出 |
| VEN-255.3 | 81 / 到期 Margin;到期 Config | 余量 19% → 都推迟,无 `AcctReq` |
| VEN-255.4 | 81 / `MarginNow` | 推出 `AcctReq{why: MarginNow}` |
| VEN-255.5 | 81 / 订单 `Query`、`DeadManRefresh` | 照常发出,不推迟 |

### VEN-256 序列失败:无部分报告、指数退避、连续失败达阈值 `Degraded`,恢复后 `Up`

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`G0` + `C0`,`gw.acct_query_degraded_after = 3`;A1 的 Margin 序列有 2 个请求;每行连续处理所列结果
- 实现:`crates/hft-venue/tests/acct.rs::ven_256_failure_backoff`

| 行 | 序列结果 | 期望 |
|---|---|---|
| VEN-256.1 | 第 1 个请求 200、第 2 个 503 | acct 环:`Raw`(请求 1)、`Raw`(请求 2)、`Failed{fail: HttpErr}`;rep 环无 `MarginReport`、无 `PositionReport` |
| VEN-256.2 | 连续失败 1、2、3、4、5、6、7 次 | 每次失败后的下一个序列最早分别在 1 s、2 s、4 s、8 s、16 s、30 s、30 s 后开始 |
| VEN-256.3 | 连续失败 2 次 | 无 `Degraded` |
| VEN-256.4 | 连续失败 3 次 | 第 3 次失败应用时 rep 环 `ConnState{Degraded, account: A1, role: Rest, reason: 14}`,恰好一条;第 4 次失败不再推 |
| VEN-256.5 | .4 之后一个序列成功 | rep 环依次 `MarginReport` 系列、`ConnState{Up, account: A1, role: Rest}`;`fails = 0` |
| VEN-256.6 | 从未 Degraded 时成功 | 无 `ConnState{Up, role: Rest}` |

### VEN-257 acct-fetch 的失败分类与 `Failed` 记录

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`C0`,`gw.acct_query_timeout_ms = 3000`,`gw.acct_body_max = 262144`,acct 环容量 1 MiB;一条 `AcctReq{Margin}`,单请求序列
- 实现:`crates/hft-venue/tests/acct_fetch.rs::ven_257_failure_kinds`

| 行 | 响应 | 期望 acct 记录 |
|---|---|---|
| VEN-257.1 | 3000 ms 无响应 | `Raw`(首块,请求元数据、体为空、`http_status = 0`)后 `Failed{fail: Timeout(1)}` |
| VEN-257.2 | 2 999 ms 时 200 | `Raw` 与 `Done`;无 `Failed` |
| VEN-257.3 | HTTP 401 | `Raw`(带体)、`Failed{HttpErr(2)}` |
| VEN-257.4 | 200 但 `acct_decode` 返回 `Err` | `Raw`、`Failed{Decode(3)}` |
| VEN-257.5 | 体长 262 144 B | `Raw` × 16、`Done` |
| VEN-257.6 | 体长 262 145 B | 一条 `Raw`(请求元数据、体为空)、`Failed{TooLarge(4)}` |
| VEN-257.7 | 429 | `Raw`、`Failed{RateLimited(5)}` |
| VEN-257.8 | acct 环容量改为 64 KiB;`Done` 记录 16 385 B | `Failed{TooLarge(4)}`;无 `Done` |

### VEN-258 `acct_decode` 从不在 gw 线程上调用

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:integ
- 里程碑:M3
- 前置:`G0` 与 `C0` 各在自己的测试线程上运行;运行 100 个 Config 与 Margin 序列
- 步骤:
  1. 读 `CallLog`
- 期望:
  - `acct_request`、`acct_decode`、`acct_finish` 的调用线程 id 全部等于 acct-fetch 线程;gw 线程上这三个方法的调用数为 0
- 实现:`crates/hft-venue/tests/acct_fetch.rs::ven_258_decode_off_gw_thread`

### VEN-259 原始响应按 `ACCT_RAW_CHUNK` 分块,gw 写 `RawTx` 与连续的 `RawRx`,拼接后与原响应相同

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:integ
- 里程碑:M3
- 前置:`G0` + `C0`;`ACCT_RAW_CHUNK = 16384`;一个 Margin 请求的响应体 51 200 B(50 KiB),acct-fetch 的连接 80
- 步骤:
  1. 完成该序列,gw 运行 6 圈
- 期望:
  - acct 环:4 条 `Raw{part: 0..3, parts: 4}`(体长 16384、16384、16384、2048,首块 `req_len > 0`),随后 1 条 `Done`
  - gw 每圈应用 1 条;raw-gw 中 `conn = 80` 的记录依次为 1 条 `RawTx`(`REDACTED`,记录头 `tsc = sent_tsc`,payload 为方法 + 路径,不含签名)与 4 条 `RawRx{what: RestResponse}`;4 条 payload 依次拼接与原响应逐字节相同
- 实现:`crates/hft-venue/tests/acct_fetch.rs::ven_259_raw_chunks`

### VEN-260 `Done` 记录的报告原样推入 rep 环且连续

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:unit
- 里程碑:M3
- 前置:`G0` + `C0`;A1 持有 S1、S2、S3 三个永续仓位
- 步骤:
  1. 完成一个 Margin 序列
- 期望:
  - rep 环在 `Done` 应用的那一圈连续出现:`MarginReport`(头,`n = 3`,条目在 rep-bulk)、`PositionReport{S1}`、`{S2}`、`{S3}`,中间没有其他元素
- 实现:`crates/hft-venue/tests/acct.rs::ven_260_done_contiguous`

### VEN-261 账户配置按周期、按提示与会话重建重查

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`G0` + `C0`,`gw.acct_cfg_recheck_s = 600`;上一个 Config 序列在 `t_d` 完成
- 实现:`crates/hft-venue/tests/acct.rs::ven_261_config_recheck`

| 行 | 过程 | 期望 |
|---|---|---|
| VEN-261.1 | aux 圈于 `t_d + 599 999 999 999` | 无 Config `AcctReq` |
| VEN-261.2 | aux 圈于 `t_d + 600 × 10⁹` | `AcctReq{Config, why: Scheduled}` |
| VEN-261.3 | 切换后交接了新会话 UID | 下一个检查时机 `AcctReq{Config, why: ConfigRecheck, session: 1, session_uid: <新 UID>}` |

### VEN-262 现货产品线不执行 Margin 序列

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:unit
- 里程碑:M3
- 前置:`G0` + `C0`,`StubProto` 的现货变体(`acct_queries(Margin)` 为空,生效 `margin_data = NotApplicable`)
- 步骤:
  1. 运行 60 s
- 期望:
  - acctreq 环中没有 `kind = Margin` 的请求;Config 照常
- 实现:`crates/hft-venue/tests/acct.rs::ven_262_spot_no_margin`

### VEN-263 REST 端点表的下标选择对应的池

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:unit
- 里程碑:M3
- 前置:`C0`;端点表两项(0 = 主端点,1 = 身份查询主机);Config 序列 step 2 的 `RestReqBuf.endpoint = 1`
- 步骤:
  1. 执行 Config 序列
- 期望:
  - step 0、1 在端点 0 的池提交,step 2 在端点 1 的池提交;端点 1 的池只连该主机
- 实现:`crates/hft-venue/tests/acct_fetch.rs::ven_263_endpoint_pool`

### VEN-264 acct-fetch 的 `has_pending`

- 规则:[运行时入口](README.md#运行时入口)、[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:table
- 里程碑:M3
- 前置:`C0`
- 实现:`crates/hft-venue/tests/acct_fetch.rs::ven_264_has_pending`

| 行 | 状态 | 期望 |
|---|---|---|
| VEN-264.1 | acctreq 环为空,无在途序列 | false |
| VEN-264.2 | acctreq 环有 1 条 | true |
| VEN-264.3 | 在途请求的完整响应已在 TLS 缓冲 | true |

### VEN-265 原始响应分块的边界

- 规则:[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:table
- 里程碑:M3
- 前置:`C0`,`ACCT_RAW_CHUNK = 16384`;单请求 Margin 序列,响应 200,体长如列
- 实现:`crates/hft-venue/tests/acct_fetch.rs::ven_265_chunk_boundary`

| 行 | 体长 | 期望 `Raw` 记录 |
|---|---|---|
| VEN-265.1 | 16 383 | 1 条(`parts = 1`) |
| VEN-265.2 | 16 384 | 1 条 |
| VEN-265.3 | 16 385 | 2 条(16 384 + 1) |

### VEN-270 热备与在建连接从不发私有频道订阅

- 规则:[§4b](README.md#4b-私有频道归属与热备连接)
- 级别:prop
- 里程碑:M3
- 性质:任意次计划切换、强制切换、重连之后,任一时刻处于 standby 或在建状态的连接上发出的帧中都不含 `private_subscribe(unsub = false)` 的输出;任一时刻至多一条连接持有私有订阅(计划切换的重叠窗口除外,窗口内至多两条)
- 生成:0–50 次事件,从 {primary `rst`、standby `rst`、计划轮换到期、`GwCmd{Rotate}`、新连接订阅被拒、回执延迟 0–6 s} 抽取
- 参考:按 `ScriptNet.sent` 逐帧比对
- 例数:PR 256
- 实现:`crates/hft-venue/tests/private.rs::ven_270_standby_never_subscribes`

### VEN-271 私有频道归属随 venue 形态确定

- 规则:[§4b](README.md#4b-私有频道归属与热备连接)
- 级别:table
- 里程碑:M3
- 前置:`G0`;交接 primary 112、standby 113(与独立用户流 120,若有)
- 实现:`crates/hft-venue/tests/private.rs::ven_271_owner_by_shape`

| 行 | `StubCfg.user_stream` | 期望 |
|---|---|---|
| VEN-271.1 | false | `priv_owner = 112`;私有订阅帧只在 112 上发出;帧为交接时随附的预生成帧,gw 线程上 `private_subscribe` 调用数为 0 |
| VEN-271.2 | true | `priv_owner = 120`;私有订阅只在 120 上;112、113 都不发 |

### VEN-272 计划切换先建后断,重叠窗口内同一成交只产出一次

- 规则:[§4b](README.md#4b-私有频道归属与热备连接)、[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:integ
- 里程碑:M3
- 前置:`G0`,`StubCfg{user_stream: false}`;A1 primary 112 持有私有订阅,standby 113,112 到计划轮换时刻、无在途
- 步骤:
  1. 运行到 113 上的私有订阅帧发出;回全部回执
  2. 在回执之后、旧连接退订之前,112 与 113 都送达同一笔成交(同一 `tid`)与同一条 `FS|S1|…` 资金费推送
  3. 继续运行到切换完成
- 期望:
  - 帧序:113 订阅 → 113 回执齐 → `priv_owner = 113` → 112 退订(或随退役)→ `swap_primary` → `ConnState{Rotated, conn: 113}`
  - 步骤 2 的成交只产出一条 `OrderEvent{Fill}`,资金费只产出一条 `FundingSettle`
- 实现:`crates/hft-venue/tests/private.rs::ven_272_make_before_break`

### VEN-273 新连接订阅被拒或 5 s 内回执不全时放弃本次切换

- 规则:[§4b](README.md#4b-私有频道归属与热备连接)
- 级别:table
- 里程碑:M3
- 前置:同 VEN-272 步骤 1 之前;113 的私有订阅在 `t_s` 发出
- 实现:`crates/hft-venue/tests/private.rs::ven_273_switch_abandoned`

| 行 | 113 的回执 | 期望 |
|---|---|---|
| VEN-273.1 | `t_s + 4 999 999 999` 全部到齐 | 继续切换(同 VEN-272) |
| VEN-273.2 | 到 `t_s + 5 × 10⁹` 只回一半 | 放弃:`priv_owner` 仍为 112,不 `swap_primary`,无 `Rotated`;113 上发出退订帧;下次尝试在 `t_s + 5 s + gw.rotate_wait_max_ms` |
| VEN-273.3 | 被拒 | 同 .2(在收到拒绝的那一圈放弃) |

### VEN-274 归属连接致命:立即 `Down{UserStream}`,新归属订阅齐后 `Up` 并排一次 Margin 与 Config

- 规则:[§4b](README.md#4b-私有频道归属与热备连接)
- 级别:unit
- 里程碑:M3
- 前置:`G0` + `C0`,`StubCfg{user_stream: false}`;A1 primary 112 为归属,standby 113 已登录
- 步骤:
  1. `rst(112)`,`poll_once`
  2. 113 回私有订阅回执,运行到下一个 aux 检查时机
- 期望:
  - 步骤 1:rep 环同一圈有 `ConnState{Down, role: Order, conn: 112}` 与 `ConnState{Down, account: A1, role: UserStream}`;113 上发出私有订阅帧
  - 步骤 2:`ConnState{Up, account: A1, role: UserStream}`;随后一条 Margin 与一条 Config 的 `AcctReq`
- 实现:`crates/hft-venue/tests/private.rs::ven_274_forced_switch`

### VEN-275 不在归属连接上的私有推送在切换窗口外被丢弃并计数

- 规则:[§4b](README.md#4b-私有频道归属与热备连接)
- 级别:unit
- 里程碑:M3
- 前置:`G0`;归属 112,没有进行中的切换
- 步骤:
  1. 113 送达一条成交推送
- 期望:
  - rep 环无该成交;计数 `priv_not_owner = 1`;该帧照写 `RawRx`
- 实现:`crates/hft-venue/tests/private.rs::ven_275_push_not_owner`

### VEN-276 独立用户流连接的轮换同样先建后断

- 规则:[§4b](README.md#4b-私有频道归属与热备连接)
- 级别:unit
- 里程碑:M3
- 前置:`G0`,`StubCfg{user_stream: true}`;用户流 120 到计划轮换时刻
- 步骤:
  1. 交接新用户流连接 121,121 收到首条事件,`poll_once`
- 期望:
  - 121 收到首条事件之前 120 一直是归属;之后 `priv_owner = 121`,120 退役;期间无 `ConnState{Down, role: UserStream}`
- 实现:`crates/hft-venue/tests/private.rs::ven_276_user_stream_rotation`

### VEN-280 服务端 WS Ping 由 hft-net 回 Pong,不产生事件

- 规则:[§5](README.md#5-心跳)
- 级别:unit
- 里程碑:M1.5
- 前置:`F0`,`routes = Bbo`;连接 0 已 Up
- 步骤:
  1. 对端发 WS Ping,`poll_once`
- 期望:
  - 本端发出一个 WS Pong;bbo 环无新元素;该连接的 `last_rx_tsc` 前移(不计入静默)
- 实现:`crates/hft-venue/tests/heartbeat.rs::ven_280_server_ping`

### VEN-281 应用层心跳按 `unconditional` 决定是否在有数据时发送

- 规则:[§5](README.md#5-心跳)
- 级别:table
- 里程碑:M3
- 前置:`G0`;`heartbeat(Order) = {ping: AppText, interval_ms: 20000, pong_timeout_ms: 10000, idle_timeout_ms: 0}`;A1 primary 每 1 s 收到一帧数据,运行 60 s
- 实现:`crates/hft-venue/tests/heartbeat.rs::ven_281_app_heartbeat`

| 行 | `unconditional` | 期望 |
|---|---|---|
| VEN-281.1 | false | 0 条应用心跳 |
| VEN-281.2 | true | 3 条(20 s、40 s、60 s) |
| VEN-281.3 | false,且 20 s 内无任何入向数据 | 第 20 s 发 1 条 |

### VEN-282 下单与深度连接每 5 s 一次 RTT Ping

- 规则:[§5](README.md#5-心跳)
- 级别:unit
- 里程碑:M3
- 前置:`G0` 与 `F0`(`routes = Md`);A1 primary 112、深度连接 8;对端在 Ping 后 3 ms 回 Pong
- 步骤:
  1. 运行 20 s
- 期望:
  - 112 与 8 各发出 4 个 WS Ping(5、10、15、20 s);每个 Pong 记一个 3 ms 的 RTT 样本
- 实现:`crates/hft-venue/tests/heartbeat.rs::ven_282_rtt_ping`

### VEN-283 `pong_timeout_ms` 与 `idle_timeout_ms` 超时按致命处理

- 规则:[§5](README.md#5-心跳)
- 级别:table
- 里程碑:M1.5
- 前置:`F0`,`routes = Md`;`heartbeat(Md) = {ping: AppJson, interval_ms: 1000, unconditional: true, pong_timeout_ms: 5000, idle_timeout_ms: 30000}`;连接 8 在 `t_0` 发出应用 ping
- 实现:`crates/hft-venue/tests/heartbeat.rs::ven_283_timeouts`

| 行 | 过程 | 期望 |
|---|---|---|
| VEN-283.1 | 无 pong,`poll_once` 于 `t_0 + 5 × 10⁹` | 不退役 |
| VEN-283.2 | 无 pong,`poll_once` 于 `t_0 + 5 × 10⁹ + 1` | 连接置 `PING_TIMEOUT`,退役,推 `ConnState{Down, conn: 8, reason: PING_TIMEOUT 位}` |
| VEN-283.3 | pong 按时到达,最后一帧入向数据在 `t_last`,之后无数据 | `t_last + 30 × 10⁹` 不退役;`t_last + 30 × 10⁹ + 1` 置 `IDLE_TIMEOUT` 并退役 |

### VEN-290 Ack 带交易所 id 时写入 `IdTable`,查找返回 `(epoch, slot, gen)`

- 规则:[§6](README.md#6-exch-id--slot-私有表idtable)
- 级别:unit
- 里程碑:M3
- 前置:`G0`,当前 epoch 5
- 步骤:
  1. 收到 `O|<槽 100 gen 3 的 cid>|EX-100|Ack|…`
  2. 收到只带 `EX-100` 的 `PartialFill`
- 期望:
  - 步骤 1:`IdTable.len = 1`
  - 步骤 2:`OrderEvent{PartialFill, epoch: 5, slot: 100, gen: 3}` 立即入环(不进暂存)
- 实现:`crates/hft-venue/tests/idtable.rs::ven_290_insert_lookup`

### VEN-291 终态条目保留,保留期内晚到费用与迟到成交仍归到原代

- 规则:[§6](README.md#6-exch-id--slot-私有表idtable)
- 级别:unit
- 里程碑:M3
- 前置:`G0`,`gw.id_tomb_ms = 120000`;槽 100 gen 3 的单(`EX-100`)在 `t_t` 收到 `Fill`(终态)
- 步骤:
  1. `t_t + 60 s` 收到只带 `EX-100` 的 `FeeOnly`
  2. `t_t + 90 s` 收到只带 `EX-100` 的 `PartialFill`
- 期望:
  - 条目 `flags = TERMINAL`、`term_tsc = t_t`,下标在 `tomb` 中
  - 步骤 1、2 的 `OrderEvent` 都带 `(5, 100, 3)`
- 实现:`crates/hft-venue/tests/idtable.rs::ven_291_tombstone_resolves`

### VEN-292 同一槽的新一代订单不覆盖旧代的保留条目

- 规则:[§6](README.md#6-exch-id--slot-私有表idtable)
- 级别:unit
- 里程碑:M3
- 前置:同 VEN-291 终态之后
- 步骤:
  1. 槽 100 gen 4 下单,Ack 带 `EX-200`
  2. 收到只带 `EX-100` 的 `FeeOnly`;收到只带 `EX-200` 的 `Fill`
- 期望:
  - `EX-100` 仍解析为 `(5, 100, 3)`;`EX-200` 解析为 `(5, 100, 4)`
- 实现:`crates/hft-venue/tests/idtable.rs::ven_292_new_gen_keeps_old`

### VEN-293 只淘汰满保留时长的终态条目

- 规则:[§6](README.md#6-exch-id--slot-私有表idtable)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`G0`,`gw.id_tomb_ms = 120000`,`gw.id_tomb_cap = 4`,`gw.id_table_cap = 16384`;`tomb` 已有 4 条,头部条目 `term_tsc = t_h`;每行在所列时刻让第 5 张单到终态
- 实现:`crates/hft-venue/tests/idtable.rs::ven_293_eviction`

| 行 | `now` | 期望 |
|---|---|---|
| VEN-293.1 | `t_h + 120 × 10⁹ − 1` | 头部不淘汰,仍可查到;新到终态的条目被删除(之后只带其交易所 id 的回报解析为 `UNRESOLVED`);计数 `idtable_full` +1 |
| VEN-293.2 | `t_h + 120 × 10⁹` | 头部被淘汰(物理删除后其交易所 id 查不到);新条目入 `tomb`;其余条目仍可查到 |

### VEN-294 表负载将超过 0.5 且头部未到期时插入失败并计数

- 规则:[§6](README.md#6-exch-id--slot-私有表idtable)
- 级别:unit
- 里程碑:M3
- 前置:`G0`,OMS 槽数 4、`gw.id_tomb_cap = 4`、`gw.id_table_cap = 16`(= 2 × (4 + 4),满足 VEN-015 的下限),`gw.id_tomb_ms = 120000`;表中 8 条(负载 0.5),全部为 1 s 前到终态的条目
- 步骤:
  1. 新单 Ack 带新的交易所 id
- 期望:
  - 插入失败;`idtable_full` +1;该单之后带 cid 的 `Fill` 仍按 client id 归属到其槽
- 实现:`crates/hft-venue/tests/idtable.rs::ven_294_full_insert_fails`

### VEN-295 交易所 id 的存储:数值直接存,字符串存 xxh3-64,超长不入表

- 规则:[§6](README.md#6-exch-id--slot-私有表idtable)
- 级别:table
- 里程碑:M3
- 前置:`G0`;每行一条 Ack 带所列交易所 id
- 实现:`crates/hft-venue/tests/idtable.rs::ven_295_id_forms`

| 行 | 交易所 id | 期望 |
|---|---|---|
| VEN-295.1 | `"8389765412"`(数值) | `OrderEvent.exch_order_id = 8389765412` |
| VEN-295.2 | `"5f1e8a3c-…-9b2d"`(36 B UUID) | `exch_order_id = xxh3_64(原文)`;`IdEntry.key` 为原文,`key_len = 36` |
| VEN-295.3 | 37 B 字符串 | 不入表;计数 `id_too_long` +1;`exch_order_id = xxh3_64(原文)` |

### VEN-296 `IdTable` 与 `HashMap` 参考模型在随机操作下一致

- 规则:[§6](README.md#6-exch-id--slot-私有表idtable)
- 级别:prop
- 里程碑:M3
- 性质:插入、查找、置终态、淘汰的任意序列之后,每次查找结果与参考模型相同;后移删除后其余条目全部可查;负载 0.5 时最长探测长度 ≤ 32
- 生成:10⁶ 次操作,键为随机 1–36 B 字节串,时钟随机前进 0–10 s
- 参考:`HashMap<Vec<u8>, (u16, u32, u32, Option<Tsc>)>` 加按终态时刻排序的队列
- 例数:PR 4(每例 10⁶ 次操作);每夜 64
- 实现:`crates/hft-venue/tests/idtable.rs::ven_296_model_check`

### VEN-297 重启后终态订单的晚到费用以 `UNRESOLVED` + 交易所 id 入环

- 规则:[§6](README.md#6-exch-id--slot-私有表idtable)
- 级别:unit
- 里程碑:M3
- 前置:`G0`,`gw.unmatched_hold_ms = 5000`;新建 `GwRuntime`(id 表为空),`MirrorRestore` 只恢复了非终态槽
- 步骤:
  1. `t = 0` 收到只带 `EX-OLD` 的 `FeeOnly`;`poll_once` 于 `5 × 10⁹ + 1`
- 期望:
  - rep 环一条 `OrderEvent{FeeOnly, slot: UNRESOLVED, exch_order_id: EX-OLD 的值}`
- 实现:`crates/hft-venue/tests/idtable.rs::ven_297_restart_late_fee`

### VEN-300 重启时从 raw-gw 尾部重建去重环,重推的成交与资金费不再产出

- 规则:[§6a](README.md#6a-去重环的重启重建)
- 级别:integ
- 里程碑:M3
- 前置:`G0`,`gw.dedup_reseed_ms = 900000`;第一个 `GwRuntime` 已产出 A1 的一条 `X|Adl|…`(`ExternalFill`,`tid = 0`,按 `(inst, exch_order_id, fill_seq)` 去重)与一条 `FS|S1|<settle>|…`;关闭
- 步骤:
  1. 用同一 raw-gw journal 新建 `GwRuntime`(`now` = 上述记录之后 10 min)
  2. `ScriptNet` 在重订阅后重推这两条
- 期望:
  - 步骤 2 后 rep 环没有这两条的任何事件
- 实现:`crates/hft-venue/tests/reseed.rs::ven_300_reseed_suppresses_repush`

### VEN-301 重建窗口的边界

- 规则:[§6a](README.md#6a-去重环的重启重建)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M3
- 前置:`G0`,`gw.dedup_reseed_ms = 900000`;`new()` 时 `now = t_n`;raw-gw 中有一条成交推送记录,其记录头 `tsc` 如列;重启后交易所重推同一笔
- 实现:`crates/hft-venue/tests/reseed.rs::ven_301_window_boundary`

| 行 | 记录 `tsc` | 期望 |
|---|---|---|
| VEN-301.1 | `t_n − 900 × 10⁹` | 在窗口内:重推不产出 |
| VEN-301.2 | `t_n − 900 × 10⁹ − 1` | 窗口外:重推照常产出一次 |

### VEN-302 重建不写环、不写 raw、不改槽镜像与 `IdTable`,并清空 `GwCtx` 其余状态

- 规则:[§6a](README.md#6a-去重环的重启重建)
- 级别:unit
- 里程碑:M3
- 前置:`G0`;raw-gw 尾部含 Ack、Fill、余额推送、保证金推送、资金费推送共 200 条
- 步骤:
  1. `GwRuntime::new`
- 期望:
  - rep 环为空;raw-gw 的写位置不变;槽镜像全部 `live = false`;`IdTable.len = 0`
  - `StubProto` 的 `GwCtx` 调试视图:累计量合成表为空、保证金推送缓存为初值;去重环含尾部中成交与资金费推送的全部键
- 实现:`crates/hft-venue/tests/reseed.rs::ven_302_reseed_side_effect_free`

### VEN-303 按 `Connect` 标记还原帧所属账户,账户之间不串

- 规则:[§6a](README.md#6a-去重环的重启重建)、[产生与消费的事件](README.md#产生与消费的事件)
- 级别:unit
- 里程碑:M3
- 前置:`G0`;raw-gw 中连接 112 的 `Connect` 记录体 `[A1, Order]`、连接 140 的 `[A2, Order]`;112 上有一笔 `tid = 9001` 的成交
- 步骤:
  1. 重建后,A2 的连接上到达 `tid = 9001` 的成交(同 inst)
- 期望:
  - A2 的这笔成交照常产出(A1 的去重键不影响 A2)
- 实现:`crates/hft-venue/tests/reseed.rs::ven_303_per_account_rings`

### VEN-304 账户查询响应不参与重建

- 规则:[§6a](README.md#6a-去重环的重启重建)
- 级别:unit
- 里程碑:M3
- 前置:`G0`;raw-gw 中 acct-fetch 连接 80 上有 `RestResponse` 记录
- 步骤:
  1. `GwRuntime::new`
- 期望:
  - `CallLog` 中重建阶段的 `classify_report`/`decode_report` 调用数 = 下单与用户流连接上的数据帧数;连接 80 的记录未被喂入
- 实现:`crates/hft-venue/tests/reseed.rs::ven_304_acct_responses_skipped`

### VEN-310 会话建立:bootstrap、endpoint、登录与交接

- 规则:[§7](README.md#7-会话建立warm-线程-conn-buildersessiondriver)
- 级别:table
- 里程碑:M1.5
- 前置:`SessionDriver<StubProto>`;`StubProto` 的变体按行设定 `bootstrap` 与 `login_result`
- 实现:`crates/hft-venue/tests/session.rs::ven_310_session_flow`

| 行 | 角色 / 变体 | 期望 |
|---|---|---|
| VEN-310.1 | Md / `bootstrap = None`,无登录 | `endpoint` → `submit` → `Open` → `SessEvent::Ready`;交接的连接号 = `BuildReq.conn` |
| VEN-310.2 | UserStream / `bootstrap = Some`(REST 取 token) | `bootstrap` 收到的 `cred` 为该账户的 REST 凭证;先执行 bootstrap REST,再以其 2xx 响应体作 `boot` 调 `endpoint`;`Ready` |
| VEN-310.3 | Order / `login_result`:`Continue` 后 `Ok` | 发两步登录帧,`Ready` |
| VEN-310.4 | Order / `Rejected(4001)` | `SessEvent::Failed`,带码 4001;不交接,连接被 `reclaim` |

### VEN-311 NAPI 落点不符的连接不交接

- 规则:[§7](README.md#7-会话建立warm-线程-conn-buildersessiondriver)
- 级别:unit
- 里程碑:M3
- 前置:`SessionDriver`;`Connector::napi_ifindex` 对该连接返回与计划网卡不同的 ifindex
- 步骤:
  1. 驱动到 `Open` 与登录成功
- 期望:
  - `SessEvent::Failed`(按建连失败);无 `Handoff`;连接被 `reclaim`
- 实现:`crates/hft-venue/tests/session.rs::ven_311_napi_mismatch`

### VEN-312 请求方启用就绪探测时交接前注册 epfd,回收前注销

- 规则:[§7](README.md#7-会话建立warm-线程-conn-buildersessiondriver)
- 级别:unit
- 里程碑:M1.5
- 前置:`SessionDriver` 持有 feed-bbo 任务的 `ReadyHandle`;BBO 连接 3 建好
- 步骤:
  1. 交接连接 3
  2. 收到连接 3 的 `RetireMsg`
- 期望:
  - 步骤 1:`ReadyHandle::add(3)` 在 `Handoff` 推出之前,`epoll_ctl(ADD)` 发生在 warm 线程
  - 步骤 2:`ReadyHandle::del(3)` 在 `Connector::reclaim` 之前;热线程上 `epoll_ctl` 计数为 0
- 实现:`crates/hft-venue/tests/session.rs::ven_312_ready_epfd_register`

### VEN-313 会话 key 的 UID 随交接带给 gw,并入下一条 Config 请求

- 规则:[§7](README.md#7-会话建立warm-线程-conn-buildersessiondriver)、[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:table
- 里程碑:M3
- 前置:`SessionDriver` + `G0` + `C0`;A1 的 `AccountMeta.venue_uid = "acct-0001"`;每行设定 `StubCfg.session_identity` 与查询结果
- 实现:`crates/hft-venue/tests/session.rs::ven_313_session_uid`

| 行 | 设定 | 期望 |
|---|---|---|
| VEN-313.1 | `Ok(true)`,会话 UID `acct-0001`,REST UID `acct-0001` | `Handoff.session_uid = "acct-0001"`、状态 Ok;Config `AcctReq{session: 1, session_uid: "acct-0001"}`;报告 `uid_keys = SESSION \| REST`,`mismatches` 无 `UID` |
| VEN-313.2 | `Ok(true)`,会话 UID `acct-0002`,REST UID `acct-0001` | 报告 `uid_keys = SESSION \| REST`,`mismatches` 含 `UID` |
| VEN-313.3 | `Ok(true)`,会话 UID `acct-0001`,REST UID `acct-0003` | 报告含 `UID`,`uid_keys = SESSION \| REST` |
| VEN-313.4 | `Ok(false)`,REST UID `acct-0001` | 无身份查询请求;`AcctReq.session = 0`;报告 `uid_keys = SESSION \| REST` |

### VEN-314 会话 UID 查询连续失败时以 `Failed` 交接

- 规则:[§7](README.md#7-会话建立warm-线程-conn-buildersessiondriver)
- 级别:unit
- 里程碑:M3
- 前置:`SessionDriver` + `G0` + `C0`,`gw.acct_query_degraded_after = 3`,`gw.acct_query_timeout_ms = 3000`;`session_identity = Ok(true)`,身份查询每次超时
- 步骤:
  1. 驱动会话建立直到交接
  2. gw 运行到下一个 Config 序列完成
- 期望:
  - 步骤 1:身份查询恰好 3 次,间隔按 §4a 的退避(1 s、2 s);之后 `Handoff` 状态 Failed;连接照常交接可用
  - 步骤 2:rep 环 `ConnState{Degraded, account: A1, role: Rest, reason: 14}`;Config `AcctReq.session = 2`;报告 `uid_keys` 不含 `SESSION`
- 实现:`crates/hft-venue/tests/session.rs::ven_314_session_uid_failed`

### VEN-315 登录帧与凭据只出现在 warm 线程

- 规则:[§7](README.md#7-会话建立warm-线程-conn-buildersessiondriver)
- 级别:integ
- 里程碑:M1.5
- 前置:`SessionDriver`、`G0`、`F0` 各在自己的测试线程;凭据为哨兵字节串 `SENTINEL-KEY-…`
- 步骤:
  1. 建立 A1 的 primary、standby、行情连接,运行 1 000 圈下单
- 期望:
  - `CallLog` 中 `login`、`endpoint`(带凭据)、`session_identity_request`、`private_subscribe` 的调用线程全是 warm 线程;gw 与 feed 线程发出的全部帧都不含哨兵字节串(`needs_per_order_signing() = false`)
- 实现:`crates/hft-venue/tests/session.rs::ven_315_credentials_warm_only`

### VEN-320 `RawTx` 从不包含登录帧、签名字段与签名的 REST 请求

- 规则:[产生与消费的事件](README.md#产生与消费的事件)
- 级别:integ
- 里程碑:M3
- 前置:`SessionDriver` + `G0` + `C0` + `F0`;全部凭据与签名用哨兵字节串
- 步骤:
  1. 运行登录、下单、撤单、查单、死手刷新、会话维护、Config 与 Margin 序列各 10 次;扫描全部 raw journal
- 期望:
  - raw 全文不含哨兵字节串;每条签名请求的 `RawTx` 带 `REDACTED` 且只有方法 + 路径;没有任何登录帧的 `RawTx`
- 实现:`crates/hft-venue/tests/raw.rs::ven_320_redaction_scan`

### VEN-321 `RawRx` 覆盖回报、账户查询响应与连接标记

- 规则:[产生与消费的事件](README.md#产生与消费的事件)
- 级别:unit
- 里程碑:M3
- 前置:`G0` + `C0`
- 步骤:
  1. 交接 A1 的 112(Order)与 120(UserStream);收到 3 帧回报;完成一个 Config 序列
- 期望:
  - raw-gw:`Connect` 记录 2 条,记录体分别为 `[A1, Order(2)]` 与 `[A1, UserStream(3)]`;回报 `RawRx` 3 条;Config 响应的 `RawTx` + `RawRx` 按 VEN-259 的形状
- 实现:`crates/hft-venue/tests/raw.rs::ven_321_raw_coverage`

### VEN-330 重连混沌:1000 次断连重连中簿从不在新快照之前回到 `Live`

- 规则:[§3c](README.md#3c-分档快照depthtopn)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:integ
- 里程碑:M3
- 前置:`F0`,`routes = Md`;S1、S2 订阅 `BOOK`;随机种子固定
- 步骤:
  1. 1000 次:随机时刻 `eof`/`rst` 深度连接、随机延迟 0–2 s 后交接替补、随机 0–5 帧旧序号快照与新快照交错
- 期望:
  - 每次断开之后,md 环中该 inst 的下一条 `ConnState{Up, inst}` 前面一定紧挨着一条在断开之后从新连接收到的 `L2Snapshot`;断开与这条 `Up` 之间没有任何该 inst 的 `L2Snapshot` 来自旧连接
- 实现:`crates/hft-venue/tests/chaos.rs::ven_330_reconnect_chaos`

### VEN-340 `poll_once` 稳态零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M1.5
- 前置:测试二进制以 hft-testkit `CountingAlloc` 运行;各运行时在 `new` 与预热 1 000 轮之后开始计数;各自有持续的输入流
- 实现:`crates/hft-venue/tests/alloc.rs::ven_340_zero_alloc`

| 行 | 运行时 / 负载(10⁵ 次 `poll_once`) | 期望 `allocs()` |
|---|---|---|
| VEN-340.1 | `FeedRuntime`(`routes = All`;BBO、成交、分档快照、Pong) | 0 |
| VEN-340.2 | `GwRuntime`(下单、撤单、回报、暂存、acct 记录应用) | 0 |
| VEN-340.3 | `SnapFetchTask`(BBO 补帧请求与响应) | 0 |
| VEN-340.4 | `AcctFetchTask`(Config 与 Margin 序列) | 0 |

### VEN-341 依赖闭包中没有任何 `venue-*` 与被禁止的 crate

- 规则:[依赖](README.md#依赖)、[§8](README.md#8-源文件规划)
- 级别:table
- 里程碑:M1.5
- 前置:`cargo metadata` 输出;`cargo tree -p hft-venue -e normal,dev`
- 实现:`crates/hft-venue/tests/deps.rs::ven_341_dependency_closure`

| 行 | 检查 | 期望 |
|---|---|---|
| VEN-341.1 | 名字以 `venue-` 开头的包 | 0 个(含 dev 依赖) |
| VEN-341.2 | `hft-sequencer`、`hft-oms`、`hft-risk` | 不在闭包中 |
| VEN-341.3 | `tokio`、`serde_json` | 不在 normal 闭包中 |
| VEN-341.4 | 本 crate 源码中对 `libc::` 的直接调用 | 0 处 |
| VEN-341.5 | `hft-journal` 的特性 | 启用 `writer` |
| VEN-341.6 | `kill_admit` 的定义位置 | `src/kill.rs`,`pub`,无 `&self`(纯函数) |

### VEN-350 基准:BBO 帧 → `Bbo` 入 bbo 环

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_feed_bbo_path`
- 实现:`crates/hft-venue/benches/feed.rs::ven_350_feed_bbo_path`

### VEN-351 基准:赛马重复帧丢弃

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_feed_dup_drop`
- 实现:`crates/hft-venue/benches/feed.rs::ven_351_feed_dup_drop`

### VEN-352 基准:分档快照帧 → `L2Snapshot{TopN}` 入 md 环与 bulk 环

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`venue_feed_topn_path`
- 实现:`crates/hft-venue/benches/feed.rs::ven_352_feed_topn_path`

### VEN-353 基准:raw journal 追加 300 B

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_raw_append`
- 实现:`crates/hft-venue/benches/raw.rs::ven_353_raw_append`

### VEN-354 基准:订单类 `GwCmd` 出环 → `sendmsg` 返回(回环)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_gw_place_loopback`
- 实现:`crates/hft-venue/benches/gw.rs::ven_354_gw_place_loopback`

### VEN-355 基准:同上,不含 `sendmsg`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_gw_place_mem`
- 实现:`crates/hft-venue/benches/gw.rs::ven_355_gw_place_mem`

### VEN-356 基准:回报帧 → `OrderEvent` 入环

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_gw_report_path`
- 实现:`crates/hft-venue/benches/gw.rs::ven_356_gw_report_path`

### VEN-357 基准:回报突发 64 帧期间出环订单的等待

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_gw_drain_during_burst`
- 实现:`crates/hft-venue/benches/gw.rs::ven_357_gw_drain_during_burst`

### VEN-358 基准:`IdTable` 查、插、置终态、淘汰

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[§6](README.md#6-exch-id--slot-私有表idtable)
- 级别:bench
- 里程碑:M3
- 基准名:`venue_idtable`
- 实现:`crates/hft-venue/benches/idtable.rs::ven_358_idtable`

### VEN-359 基准:`kill_admit`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_kill_admit`
- 实现:`crates/hft-venue/benches/gw.rs::ven_359_kill_admit`

### VEN-360 基准:每轮每条 BBO 连接的健康维护

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[§3a](README.md#3a-bbo-连接健康)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_bbo_live_tick`
- 实现:`crates/hft-venue/benches/feed.rs::ven_360_bbo_live_tick`

### VEN-361 基准:回报帧 → `ExternalFill` 入环

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`venue_gw_external_fill`
- 实现:`crates/hft-venue/benches/gw.rs::ven_361_gw_external_fill`

### VEN-362 基准:可忽略与未知用户流事件跳过

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`venue_gw_skip_event`
- 实现:`crates/hft-venue/benches/gw.rs::ven_362_gw_skip_event`

### VEN-363 基准:最大保证金推送在 gw 上内联解码

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`venue_gw_margin_push`
- 实现:`crates/hft-venue/benches/gw.rs::ven_363_gw_margin_push`

### VEN-364 基准:应用一条 16 KiB 的 acct 记录

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:bench
- 里程碑:M3
- 基准名:`venue_gw_acct_apply_16k`
- 实现:`crates/hft-venue/benches/gw.rs::ven_364_gw_acct_apply_16k`

### VEN-365 基准:50 KiB 账户查询响应解码并写 acct 记录

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:bench
- 里程碑:M3
- 基准名:`venue_acct_fetch_50k`
- 实现:`crates/hft-venue/benches/acct.rs::ven_365_acct_fetch_50k`

### VEN-366 基准:feed-bbo 空闲轮(2 条连接,不探测)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_idle_loop_bbo`
- 实现:`crates/hft-venue/benches/feed.rs::ven_366_idle_loop_bbo`

### VEN-367 基准:feed-bbo 空闲轮(就绪探测生效,6 条连接)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[§2](README.md#2-feedruntimepoll_once-的固定顺序)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_idle_loop_bbo_ready`
- 实现:`crates/hft-venue/benches/feed.rs::ven_367_idle_loop_bbo_ready`

### VEN-368 基准:`routes = All` 空闲轮(3 条连接)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`venue_idle_loop`
- 实现:`crates/hft-venue/benches/feed.rs::ven_368_idle_loop`

### VEN-369 基准:gw 出环等待(8 个空闲 socket)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[§4](README.md#4-gwruntimepoll_once-的循环结构)
- 级别:bench
- 里程碑:M1.5
- 基准名:`venue_gw_idle_8sock`
- 实现:`crates/hft-venue/benches/gw.rs::ven_369_gw_idle_8sock`

### VEN-380 dev2 上满环期间 `SpinThenPark` 线程 park 而不忙等

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)
- 级别:host
- 里程碑:M3
- 环境:dev2,hft-core `sim` 实例(只行情),feed 线程 `IdlePolicy::SpinThenPark`(取值按 hft-core dev2 线程计划),`feed.ring_full_park_us = 200`
- 授权:无
- 步骤:
  1. 用调试开关让 sequencer 暂停消费 bbo 环 60 s,行情照常到达
  2. 期间每 1 s 读该 feed 线程的 `/proc/<pid>/task/<tid>/schedstat` 与 `TaskLoop` 的 park 计数
- 通过标准:满环的 60 s 内该线程运行时间 < 30 s(未忙等);park 次数 > 0;恢复消费后 bbo 环序列无缺口(与 raw-feed 对照)
- 证据:`tests/host/VEN-380.sh` 的输出 `VEN-380-dev2-<日期>.txt`
- 实现:`tests/host/VEN-380.sh`

### VEN-381 私有频道能否在两条连接上同时订阅(M3 之前必须核实)

- 规则:[§4b](README.md#4b-私有频道归属与热备连接)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M3
- 环境:dev2;Binance 演示环境(USDⓈ-M 与现货)、OKX 模拟盘、Bitget 模拟盘的测试账户;方法见开发备忘第 4 条
- 授权:无(测试网/模拟盘账户)
- 步骤:
  1. 每个产品线开两条连接同时订阅该账户的私有频道(Binance USDⓈ-M 用同一 listenKey;现货两条 ws-api 各 `userDataStream.subscribe`;OKX/Bitget 两条 `/private` 订阅 `orders`)
  2. 下一笔远离对手价的最小名义单并撤单;比对两路 raw
- 通过标准:每个产品线得出"两路都收到 / 只有一路收到 / 第二路被拒"之一的结论,写进该 venue README;结论为"不允许"的产品线,其 README 写明"先断后建 + VERIFY"
- 证据:`VEN-381-<venue>-<日期>.txt`(两路 raw 的对比)
- 实现:`tests/host/VEN-381.sh`

### VEN-382 私有流断开期间的成交在重连后是否补推

- 规则:[§6a](README.md#6a-去重环的重启重建)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M3
- 环境:dev2 测试网实例;三所测试网/模拟盘账户;方法见开发备忘第 8 条
- 授权:无(测试网/模拟盘账户)
- 步骤:
  1. 持仓时断开私有连接;期间用另一会话制造一笔成交(强平/ADL 难以制造时用普通成交并注明)
  2. 重连并重订阅;比对 raw-gw 与成交历史
- 通过标准:每所得出"补推 / 不补推 / 补推回看时长 X"的结论,写进各 venue README 的"用户流事件表";`gw.dedup_reseed_ms` 不小于各所最长回看时长(否则同一改动调大,并在 [docs/changes.md](../../docs/changes.md) 记一行)
- 证据:`VEN-382-<venue>-<日期>.txt`
- 实现:`tests/host/VEN-382.sh`

### VEN-383 warm 线程 park 周期对 BBO 补帧与 `MarginNow` 的影响

- 规则:[§1](README.md#1-线程与环由-hft-core-装配)、[开发备忘](README.md#开发备忘)
- 级别:host
- 里程碑:M3
- 环境:dev2;M3 末满订阅探针主机(按 hft-core 线程计划)
- 授权:dev2 为无;探针主机 `[需授权]`
- 步骤:
  1. 强制断开全部 BBO 连接 200 次,记录"`SnapReq` 推出 → 补帧 `Bbo` 入环"的时长
  2. 注入 200 次 `MarginNow` 提示,记录"提示 → `MarginReport` 入环"的时长
- 通过标准:两组分布的 p50/p99 与当时 warm 线程 `park_us`、REST RTT p99 一并记录;p99 − REST RTT p99 > `park_us` 时,同一记录给出调小 `park_us` 的改动并复测
- 证据:`VEN-383-<host>-<日期>.txt`
- 实现:`tests/host/VEN-383.sh`

### VEN-384 就绪探测收益与 `feed.bbo_frames_per_poll` 取值

- 规则:[§2](README.md#2-feedruntimepoll_once-的固定顺序)、[开发备忘](README.md#开发备忘)
- 级别:host
- 里程碑:M3
- 环境:M1.5 延迟探针与 M3 末满订阅探针主机
- 授权:`[需授权]`(租用实例)
- 步骤:
  1. `feed.ready_poll = auto` 与 `off` 各运行 1 h,记录 `venue_idle_loop_bbo*` 与 `kernel_rx` 直方图
  2. `feed.bbo_frames_per_poll` 取 4、6、8 各运行 1 h(含资金费结算整点),记录 BBO 突发下的 `ingest` p99
- 通过标准:选定值及依据写进探针 Agent Note;若 `auto` 不优于 `off`,同一改动把默认值改为 `off` 并在 [docs/changes.md](../../docs/changes.md) 记一行
- 证据:`VEN-384-<host>-<日期>.txt`
- 实现:`tests/host/VEN-384.sh`

### VEN-385 各所分档快照的实际节拍与负载

- 规则:[§3c](README.md#3c-分档快照depthtopn)、[开发备忘](README.md#开发备忘)
- 级别:host
- 里程碑:M3
- 环境:dev2,主网公共行情,`feed.raw.md = on`
- 授权:无
- 步骤:
  1. 每所对 50 个合约订阅 `DepthTopN` 1 h;统计 raw 中每条消息的间隔、字节数与跨合约的对齐
- 通过标准:三所各给出空闲与突发时的节拍、字节数分布,写进各 venue README 的"分档快照流"一节(该节相应条目改为已核实)
- 证据:`VEN-385-<venue>-<日期>.txt`
- 实现:`tests/host/VEN-385.sh`

### VEN-386 成交推送先于 Ack 的间隔与终态后费用的延迟

- 规则:[§4](README.md#4-gwruntimepoll_once-的循环结构)、[§6](README.md#6-exch-id--slot-私有表idtable)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 测试网实例,三所测试网/模拟盘账户,联调窗口内
- 授权:无(测试网/模拟盘账户)
- 步骤:
  1. 统计联调窗口内全部订单"成交推送到达 → Ack 到达"的间隔与"终态 → 费用推送"的延迟
- 通过标准:p99.9 × 3 ≤ `gw.unmatched_hold_ms`(5 000 ms)且 ≤ `gw.id_tomb_ms`(120 s);超出的所在同一改动调大对应键并在 [docs/changes.md](../../docs/changes.md) 记一行
- 证据:`VEN-386-<venue>-<日期>.txt`
- 实现:`tests/host/VEN-386.sh`

### VEN-387 BBO 补帧风暴下 `snap.parallel` 不触发 418/429

- 规则:[§3b](README.md#3b-快照获取snap-fetch)、[开发备忘](README.md#开发备忘)
- 级别:host
- 里程碑:M3
- 环境:dev2,主网公共行情,三所 6 个产品线满订阅,`snap.parallel = 2`
- 授权:无
- 步骤:
  1. 对每个产品线同时断开全部 BBO 连接 20 次,使所有 inst 进入补帧
- 通过标准:snap 记录中 `RateLimited` 为 0;每次补帧完成时长写进记录;出现 418/429 时按开发备忘第 7 条下调 `snap.parallel`(同一改动在 [docs/changes.md](../../docs/changes.md) 记一行)并复测
- 证据:`VEN-387-dev2-<日期>.txt`
- 实现:`tests/host/VEN-387.sh`

### VEN-388 `gw.aux_every` 与 `gw.rest_idle_check_ms` 的取值

- 规则:[配置项](README.md#配置项)、[开发备忘](README.md#开发备忘)
- 级别:host
- 里程碑:M3
- 环境:M3 末满订阅探针主机,`gw-ingest/poll-cadence` 基准与测试网查单
- 授权:`[需授权]`
- 步骤:
  1. `gw.aux_every` 取 4、8、16,`gw.rest_idle_check_ms` 取 500、1000、2000,各组合跑 `venue_gw_idle_8sock`、`gw-ingest/poll-cadence` 与 100 次查单响应延迟
- 通过标准:选定值及三组直方图写进探针 Agent Note;选定值与本 crate 默认不同时同一改动更新默认值并在 [docs/changes.md](../../docs/changes.md) 记一行
- 证据:`VEN-388-<host>-<日期>.txt`
- 实现:`tests/host/VEN-388.sh`

### VEN-389 每个 core 产品线在测试网/模拟盘上的运行时冒烟

- 规则:[概要](README.md#概要)、[§3a](README.md#3a-bbo-连接健康)、[§4](README.md#4-gwruntimepoll_once-的循环结构)、[§4a](README.md#4a-账户查询保证金轮询与配置核验)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 上 `prod` 角色测试网实例(`feed_env = "testnet"`、`order_env = "testnet"`,测试网 refdata);Binance、OKX、Bitget 的现货与 USDT 永续
- 授权:无(测试网/模拟盘账户)
- 步骤:
  1. 每个产品线连续运行 24 h:订阅 5 个 inst 的 `BBO | BOOK | TRADE | MARK | FUNDING`;每 10 min 下一笔远离对手价的最小名义单并撤单
  2. 期间对每个产品线各做一次:断开全部 BBO 连接、断开深度连接、断开下单 primary、触发一次计划切换(`GwCmd{Rotate}`)
- 通过标准:bbo 环无重复 `update_id`;两次全断后各 inst 在 `bbo_refresh_grace_ms` 内收到流上报价或补帧 `Bbo`;深度断开后每个 inst 先 `L2Snapshot` 后 `Up`;每条订单类命令恰好一条 `SendReport`,每张单都收敛到终态;启动后的 `AccountConfigReport` `mismatches = 0` 且 `uid_keys` 覆盖全部交易 key(`uid_policy = "unverifiable"` 的账户按 hft-refdata 的放行方式);未知用户流事件逐条有结论;`RawTx` 哨兵扫描为空
- 证据:`VEN-389-<venue>-<日期>.txt`,并列入门槛 ⑥ 的记录
- 实现:`tests/host/VEN-389.sh`

## 验收

- **里程碑**:M1.5 纵切片(BBO 行情、gw 下单与撤单、kill 判定、会话建立)以本文件里程碑为 M1.5 的用例全部有实现且通过为完成;M3 以里程碑 ≤ M3 的全部用例为完成;M6 的 `testnet` 用例在 dev2 联调窗口执行并按 `证据` 归档。
- **属性测试例数**:PR 的例数写在各用例;每夜对 VEN-022、VEN-033、VEN-053、VEN-103、VEN-151、VEN-175、VEN-193、VEN-200、VEN-270 各跑 PR 例数的 50 倍,VEN-296 跑 64 例(每例 10⁶ 次操作)。
- **预算**:VEN-350–VEN-369 的基准名与数值上限登记在本 crate README 的[延迟与资源契约](README.md#延迟与资源契约)与 [benchmarks](../../benchmarks/README.md);判定在统一升级主机类 `c16`/`c32` 的参考机上(`bench-gate --reference`),`dev2` 只验功能;主机内分段(`ingest`、`egress`、`hedge_host`、`rep_to_order`、`tick_to_trade`)的数字归 [latency-budget](../../docs/latency-budget.md),判定口径归 [testing §8](../../docs/testing.md#8-上线门槛) 门槛 ⑩,第一份数据来自 M1.5 延迟探针。
- **稳态零分配**:VEN-340 四行均为 0。
- **重连混沌**:VEN-330 的 1000 次断连重连中,没有一次簿在收到新快照之前回到 `Live`。
- **测试网/模拟盘**:VEN-389 对 Binance、OKX、Bitget 的现货与 USDT 永续全部通过,作为门槛 ⑥ 的运行时部分(行情与下单都用测试网,`feed_env = "testnet"`);VEN-381 在 M3 之前完成。
- **发布**:改动本 crate 的版本先在 dev2 测试网窗口冒烟,再上升级主机(步骤归 [release-and-deploy](../../docs/runbooks/release-and-deploy.md))。
- **跨组件**:模拟账户的订单从不出现在任何 `out-<venue>` 环,由 [hft-sim](../hft-sim/README.md) 与 apps/hft-core 的装配断言覆盖;真实 venue 协议 + 本 crate 运行时 + sequencer 的组合(如未知用户流事件、时钟越界拒单)见 [scenarios](../../docs/tests/scenarios.md) 的 C12、FP-8。
- **可选扩展"全量增量深度"**:VEN-105–VEN-109(`状态:可选扩展:全量增量深度`)在某 venue 启用 `feed.depth = incremental` 的同一提交加入 `tools/xtask/test-trace.toml` 的 `enabled_extensions` 并通过;L2 双路合并(开发备忘)启用前先补用例。
