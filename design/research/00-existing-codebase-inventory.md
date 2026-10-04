# 00 — 现有原型代码盘点（E:\trading\Trading 及其服务器快照）

> 状态：**完整版**。日期 2026-09-29。只读盘点，未改动原型仓库。
> 目的：为 v2 从零重建提供「现有资产清单 + keep/borrow/rewrite 判定 + 隐含需求」。
> 仓库：`E:\trading\Trading`，`main` @ `a6e62a0`（v0.3.7），Rust 2024 edition，toolchain 1.97.1，目标 `aarch64-unknown-linux-gnu`（ARM64 EC2 c7g.large，2 vCPU，Tokyo apne1-az2）。
> **重要**：仓库内 `cdex-arb/` 是 2026-09-08 的导入快照；9/20 之后的流水线、价差引擎、sizing、bsc_route、bsc_price 等只存在于服务器，本地最新镜像为 `E:\trading\analysis\pfview\new`（2026-09-26 10:24）及 `E:\trading\analysis\cdex-snapshot\cdex-arb`（9/25）。第 2 章以最新镜像为准并注明。

---

## 0. 仓库总览与代码量

| 区域 | 语言 | LOC | 一句话定位 |
|---|---|---:|---|
| `apps/*` + `crates/*`（workspace 21 成员） | Rust | **36,077** | 只采集不交易的市场数据管线：WS → raw WAL → Parquet → S3，含 UDS 控制面 |
| `cdex-arb/`（服务器最新镜像） | Python + Rust | Python ≈ 27k（含 `telegram_monitor_secure.py` 6.2k、`spread_engine.py` 2.1k、`bsc_trader.py` 1.9k）；`rust/cex-arb` 3,777 | 老板的实盘套利栈：Binance USDT 永续空头 × (BSC/Solana DEX 现货 或 Gate/KuCoin 现货) |
| `portfolio-arb/` | Rust | 12,661（含 `tune/`、archive） | BN 永续 × Gate 现货 basis+funding 组合套利（评分/调仓/OMS/对账/SQLite） |
| `oil-arb/` | Python | 9,991 | Gate 永续 BZ_USDT × CL_USDT z-score 均值回归 + 回测脚本群 |
| `python/telegram_bot/` | Python | 3,551 | 采集系统的受限 Telegram 运维面（bot + opsd） |
| `docs/ schemas/ deploy/ config/ tests/ tools/` | md/sh/toml/py | — | 设计规格、二进制格式规范、systemd、发布脚本、fixture、mock 交易所 |

Rust workspace 各 crate/app 行数（含 tests）：

| crate / app | LOC | 职责 |
|---|---:|---|
| `apps/collector` | 7,748 | 采集主进程：shard 规划、WS 客户端驱动、WAL ingress、控制面 UDS、监控任务、验收报告 |
| `apps/processor` | 5,571 | 冷路径：sealed WAL → 重放订单簿 → Parquet v2/v3 |
| `crates/okx` / `crates/bitget` / `crates/binance` | 3,355 / 3,256 / 2,659 | 三所适配器（REST 发现 + WS shard 客户端 + 订单簿同步器 + replay decoder） |
| `apps/uploader` | 2,216 | 校验后 bundle 上传 S3 + retention |
| `crates/parquet-store` | 1,600 | Arrow/Parquet schema、批构建、分区写 |
| `crates/wal` | 1,450 | WAL v1/v2 编解码、SegmentWriter、recovery、WalReader |
| `crates/catalog` | 1,423 | 标的发现、分类（assets.toml）、选择策略、catalog 原子激活 |
| `crates/s3-store` | 1,322 | ObjectStore trait、S3 key 布局、manifest、条件写 |
| `crates/telemetry` | 1,042 | ReceiveClock、Prometheus、AlertStore、磁盘策略、chrony |
| `apps/capturectl` / `apps/collectorctl` / `apps/replay` / `apps/catalog-builder` | 955 / 441 / 535 / 290 | CLI |
| `crates/storage-state` / `crates/control` / `crates/config` | 718 / 514 / 448 | 本地目录状态机 / UDS 协议 / collector.toml |
| `crates/orderbook` / `crates/domain` / `crates/test-support` | 283 / 244 / 7 | 整数订单簿 / 领域类型 / 占位 |

---

## 1. Rust workspace（市场数据采集系统）

### 1.1 定位与设计原则

README 与 `docs/superpowers/specs/2026-08-25-high-frequency-market-data-system-design.md`（506 行，21 章）确认这是**纯采集**系统：「collector 在解码前存储原始交易所报文，且永不包含交易执行逻辑」。§4 明确「Rust 模块化采集核心 + 独立冷处理进程 + 独立控制面」，进程间只用文件状态机和 UDS，不引入 NATS/Kafka。§18 性能目标：receive→WAL 入队 p99 < 1 ms、receive→WAL 写 p99 < 5 ms、receive→订单簿应用 p99 < 5 ms、CPU < 70%、RSS < 2.5 GiB。原则：raw-first、确定性整数域（`Quantum` → `price_ticks/quantity_lots: i64`）、四种时间戳分离、冷生产（安装 ≠ 启动）、`unsafe_code = "forbid"` + clippy `all = deny`。

### 1.2 领域类型（`crates/domain/src/*.rs`）

```rust
enum Exchange { Binance, Okx, Bitget }      enum Market { Spot, LinearPerpetual }
enum PrimaryCategory { Crypto, UsEquity, Commodity, HkEquity, KrEquity, CnEquity, PreMarket }
struct InstrumentId { exchange, market, symbol: String }   // 永不从 symbol 推断 exchange/market
struct ClockStamp { utc_ns: i64, monotonic_ns: u64, clock_synchronized: bool }
struct ProcessScope { host_boot_id: Uuid, process_instance_id: Uuid }   // monotonic 只在此范围内有效
struct BookLevel { price_ticks: i64, quantity_lots: i64 }
struct SequenceRange { first, last: i64, previous: Option<i64> }
struct BookUpdate { instrument, epoch: u64, exchange_event_ts_ns: Option<i64>, received: ClockStamp, sequence, bids, asks }
struct BookCheckpointMarker { instrument, epoch, sequence, received, hash: BookHash([u8;32]) }
enum ReplayEvent { BookSnapshot{..}, BookDelta{update, source_record_index}, Checkpoint(..), EpochInvalidated{reason} }
```

`PrimaryCategory` 暴露了老板的标的宇宙：除 crypto 外还有 **tokenized 美股/港股/韩股/A 股/商品/盘前**（`config/assets.toml` 202 行：PROM、AAPL、XAU、CAT、CRCL…；OKX `XAAPL-USDT`、Binance `AAPLUSDT` 永续等）。

### 1.3 订单簿模型（`crates/orderbook/src/book.rs`）

`OrderBook { instrument, epoch, sequence: Option<i64>, bids/asks: BTreeMap<i64,i64> }`；`replace_snapshot()` / `apply_delta()`（qty=0 删档、应用后校验不交叉，失败回滚 undo 日志）；`hash()` = SHA-256(epoch, sequence, 全部档位) 供 checkpoint 一致性校验。**正确性优先、非低延迟**（BTreeMap + 每次 `bids()` 全量分配 + 全簿哈希）。基线（`docs/runbooks/2026-08-27-latency-baseline.md`）：book apply p95 0.18 ms / p99 0.36 ms / max 1.3 ms。

### 1.4 WAL 设计（`crates/wal`，`schemas/wal-v2.md`）

- **Segment header**：`TRDWAL01` | version u16=2 | meta_len u32 (≤64 KiB) | JSON `SegmentMeta { segment_id, connection_id, shard_id, exchange, market, endpoint, source_channel, created_utc_ns, process: ProcessScope, catalog_version, collector_version, config_sha256 }` | CRC32C。
- **Record（小端）**：`body_len u32 | kind u8 | flags u8 | reserved u16 | index u64 | recv_utc_ns i64 | recv_monotonic_ns u64 | clock_synced u8 | data_family u8 | security_domain u8 | stream_len u16 | payload_len u32 (≤16 MiB) | stream | raw payload | CRC32C(kind..payload)`。
- `RecordKind`：`WebSocketText/Binary, Ping, Pong, RestSnapshot, Connect, Disconnect, SubscriptionAck/Error, SnapshotSync, CheckpointMarker, SequenceGap, LocalOverload, DiskProtection, OperatorCommand, RestMarketData, Trailer(255)` —— 连接生命周期与运维事件**同流记录**，`trading-replay gaps` 可解释每个缺口。`DataFamily`：`OrderBook, Trade, BookTicker, MarkPrice, IndexPrice, FundingRate, OpenInterest, MarketLiquidation, PrivateOrder, PrivateFill, PrivateLiquidationWarning, Control`；`SecurityDomain { Public, Private }`。
- **Writer**（`writer.rs:14 SegmentConfig`）：`max_bytes 512 MiB / max_age 300 s / sync_bytes 8 MiB / sync_interval 1 s`；`RollReason { Size, Age, Disconnect, Operator, Drain, Recovery }`；seal 写 `Trailer { record_count, first/last_utc_ns, bytes_before_trailer, clean_shutdown }` 后 rename 到 `sealed/`。`BufWriter<File>`，非 O_DIRECT/io_uring。
- **Recovery**：`scan_segment` / `recover_open_segment` 逐记录 CRC，截断尾部损坏，未 clean 进 `quarantine/wal-open`。
- **快照策略**：订单簿 shard 定时把 `BookRegistry` 状态写 `CheckpointMarker`（`apps/collector/src/main.rs:2460-2491`），rotate/drain/pause 前强制 checkpoint（`:865`）。**教训**：v0.3.4 前 512 MiB 滚动由 writer 内部触发、没有先写快照，84% 订单簿 segment 无法自段首重建被隔离；v0.3.5 改为运行时在 `SEGMENT_TARGET_BYTES` 走 `checkpoint → invalidate → rotate → reconnect`（`main.rs:906`）。**v2 必须把「每个 segment 自含起始快照」作为格式不变量。**
- **写线程**：`spawn_wal_writer` = `spawn_blocking` 单线程消费 `mpsc(8192)`；shard 侧 `try_raw()` 满则 `IngressError::Overload` → 客户端 `IngressDecision::Full` → 记 `LocalOverload` 并断线重连（**满队列不阻塞读循环，宁可丢+标记**）。

### 1.5 控制协议（`crates/control`，`schemas/control-v1.md`）

仅本地 UDS（`/run/trading/collector.sock`，0660，组 `trading-control`），禁 TCP；帧 = `u32 大端长度 + compact JSON`（≤1 MiB），**每连接一请求一响应即关闭**。`ControlRequest { protocol_version:1, request_id: Uuid, issued_at_utc_ns, command }`；`Command`：`status, health, recent_alerts{after_id,limit}, watch_alerts{after_id}, pause_all, resume_all, pause_symbol{instrument}, resume_symbol, reload_catalog, rotate, drain`；响应 `status: ok|invalid_request|conflict|unavailable|internal_error` + `collector_version` + `catalog_version` + `body`。幂等：缓存最近 4096 个 request_id 的响应；alert 游标「可重放不可跳过」。三层链路：Telegram bot（`trading-bot` 用户）→ `ops.sock`（`trading-opsd`，Python，sqlite 审计、两步确认、12 写/分限速）→ `collector.sock`。

### 1.6 配置 schema（`crates/config/src/collector.rs`）

`CollectorConfig { mode, data_dir, catalog_path, control_required, control_socket_path, control_group_gid, metrics_bind, wal_queue_capacity(8192), max_subscriptions_per_shard(prod 10), max_expected_messages_per_second_per_shard(5000), binance{rest_base, futures_rest_base, spot_ws_base, futures_ws_base, futures_ws_market_base?, spot/futures_snapshot_limit}, okx{rest_base, ws_public, ws_private, channel_preference[]}, bitget{rest_base, ws_public, ws_private}, private_data{enabled, required, *_secret_id (AWS Secrets Manager), aws_region} }`。

生产端点事实：Binance spot 用 `data-api.binance.vision` / `data-stream.binance.vision`；futures `fstream.binance.com/ws`，**aggTrade/markPrice/forceOrder 只在 `/market/ws` 投递**（v0.3.7 修复，根路径 ACK 但永不推送）；OKX prod 只用 `books`（`books-l2-tbt` 需 VIP）；Bitget `v2/ws/public`，**必须每 30 s 无条件发文本 `ping`**（否则 ~150 s 被踢），单连接 >10 msg/s 断开，订阅 50 args/批。

`collection-policy.v1`：`include_categories[] / include_instruments[] / exclude_instruments[]（exclude 优先）/ start_at / duration_seconds / raw_wal_enabled=true / processor_enabled / uploader_enabled`；策略 SHA-256 + catalog version 写 JSONL 审计；先 seal 老 segment 再原子切 catalog。

### 1.7 时间戳采集

`crates/telemetry/src/lib.rs:44-73 ReceiveClock`：`stamp()` = `SystemTime::now()`（UTC ns）+ `Instant::elapsed()`（monotonic ns）+ chrony 同步标志（`monitor_time` 任务周期性 `chronyc tracking`）。采样点：WS 库 yield 一条完整应用消息后、**解析/入队之前**（`crates/binance/src/client.rs:258,294,308,322,349`）。metrics：`trading_exchange_event_to_receive_latency_seconds`（仅同步时钟 live book 行，负值单独计数）、`trading_ingress_latency_seconds`、`trading_wal_write_latency_seconds`、`trading_book_apply_latency_seconds`。基线：WAL path p95 0.03 ms / p99 0.63 ms / max 5.7 ms；enqueue ≈0.4 ms。**局限**：两次 syscall 而非 TSC；用户态 recv 时间而非 `SO_TIMESTAMPING`；实测交易所→接收 p50 Binance 1.3 ms / OKX 6 ms / Bitget 6.5 ms（重尾 p99 140 ms，Bitget 侧批处理）。

### 1.8 交易所适配器（binance / okx / bitget 同构）

每 crate 五模块：`rest.rs`（发现 + turnover + 快照/OI/premium）、`client.rs`（**shard WS 客户端**）、`depth.rs`（parse + 同步器状态机）、`replay.rs`（WAL → `ReplayEvent`，冷路径复用同一同步器）、`checksum.rs`（top-25 交错 CRC32 流式）。

- **WS 客户端**：`tokio-tungstenite 0.27` + rustls(ring) + `set_tcp_nodelay`；`XxxShardClient::run(&self, ingress, books, stamp) -> ShardExit`。一连接 = 一 shard = (exchange, market, `PublicDataFamily{OrderBook,Trades,BookTicker,Derivatives}`, ≤N instruments)。`select!` 循环：socket / REST 快照 `FuturesUnordered`（Binance 限并发 `MAX_INFLIGHT_SNAPSHOTS=4` 防 429/418）/ 心跳 / supervisor 命令。每帧 `stamp()` → `ingress.try_raw()` 落 WAL → 再 `parse_*` → 同步器 → `books.apply()`。
- **JSON 解析**：`serde_json::from_slice` 到 typed struct；OKX/Bitget 有借用式 `parse_books_ref()`（`&'a str` 价格串直接喂 `Quantum::to_units`）。Binance ACK/快照用 `serde_json::Value` 动态查（`client.rs:622,738`）。无 simd-json。
- **消息类型**：Binance 组合流 `<sym>@depth@100ms` / `@trade` / `@aggTrade` / `@bookTicker` / `@markPrice@1s` / `@forceOrder` + REST OI/premiumIndex 60 s；OKX `books`（400 档，`seqId/prevSeqId/checksum`）、`trades`、`tickers`+`bbo-tbt`、`mark-price/index-tickers/funding-rate/liquidation-orders`；Bitget `books`（≤500 档，`seq/pseq`，无 checksum）、`trade`（首推 snapshot 批）、`ticker`+`books1`。
- **重连**：`apps/collector/src/shard.rs:189 ReconnectBackoff` 指数退避；shard 错峰启动；OKX/Bitget 25 s idle → `ping`，10 s 无 `pong` → `HeartbeatTimeout`；所有 Connect/Disconnect/SubscriptionAck/Error 落 WAL。
- **序列缺口**：Binance `DepthSynchronizer`（`depth.rs:148`）缓冲 diff → REST `lastUpdateId` → `U <= lastUpdateId+1 <= u`，永续 `pu == prev u`，失败 `SyncAction::Resnapshot` + `epoch+1` + WAL `SequenceGap`。OKX/Bitget `prevSeqId/pseq` 链式校验 + checksum（非零才校），失败只对**该 instrument** unsub/resub（交易所 subscribe 时重发快照）；一分钟 >5 次 → 整 shard 重连；resync 期间 update 丢弃并计数。

### 1.9 运行时与线程模型（`apps/collector`）

单 `#[tokio::main]` 多线程（worker = vCPU = 2），**无** core pinning / `current_thread` 隔离 / busy-poll。每 shard 一 `tokio::spawn`；WAL 写 `spawn_blocking` 单线程；控制面、`monitor_segment_size/disk/time` 独立任务。通信：`mpsc(8192)` → WAL；`mpsc(8)` supervisor；`watch<bool>` shutdown；`Arc<tokio::Mutex<RuntimeState>>`；`BookRegistry` 为 `Arc<Mutex>`。systemd `CPUWeight=1000`、`MemoryMax=3000M`、`Type=notify`。评价：吞吐 + 正确性 + 可运维的经典 tokio 服务；对 v2 tick-to-trade 目标需换 per-core 单线程 reactor / SPSC / 预分配。

### 1.10 processor → Parquet（`apps/processor`，`crates/parquet-store`，`schemas/parquet-v{2,3}.md`）

单线程轮询 `sealed/` → claim 到 `processing/` → `WalReader` → `EventNormalizer::on_record()`（`events.rs` 1,652 行）+ 订单簿重放 → Arrow `RecordBatch`（8192 行）→ zstd Parquet → bundle manifest → `ready/`。**v2 数据集**（15 个 `EventDataset`，每族独立 schema）：`book_top10, trade, book_ticker, mark_price, index_price, funding_rate, open_interest, market_liquidation, private_order_update, private_fill, private_liquidation_warning` + `order_book_update.v2` + `book_checkpoint.v1`；**v3** 修正 `book_top10_v3 / mark_price_v3 / index_price_v3 / funding_rate_v3`（v2 误把 next-funding time 放进 `exchange_transaction_ts_ns`；v3 只从通过 seq+checksum 校验的簿输出，含 `sequence_id, prev_sequence_id, book_epoch, quality_status, checksum_verified`）。统一信封：`exchange_event_ts_ns, exchange_transaction_ts_ns?, server_receive_utc_ns, server_receive_monotonic_ns, clock_synchronized` + `payload_json`（ticks/lots 整数）+ `stored_depth=10 / source_depth`；trade 行 `aggregation_kind`。本地分区 `<dataset>/category=/exchange=/market=/symbol=/date=/hour=HH/part-00000.parquet`。

### 1.11 uploader → S3（`crates/s3-store/src/key.rs`）

```
v2/{public|private}/raw_wal/exchange=<ex>/market=<mk>/date=YYYY-MM-DD/hour=HH/shard=<shard_id>/<segment_id>.wal.zst
v2/{public|private}/<dataset>/category=<cat>/exchange=/market=/symbol=/date=/hour=HH/<segment_id>-<part:03>.parquet
v2/{public|private}/manifests/date=/hour=HH/<segment_id>.json        catalogs/<version>.json
```
`ObjectStore` trait + `S3ObjectStore`（aws-sdk-s3，sigv4a，要求 SHA-256 checksum；`ensure_file/ensure_bytes` 条件创建，冲突即 `Conflict`）；逐对象校验 → `CommitManifest`（canonical JSON + sha256）→ `uploaded/`；`retention.rs` 只删远端 manifest 已证明的本地文件。区域 `ap-northeast-1`；原始 tick 只在 S3。

### 1.12 Telemetry、CLI、CI/部署、测试

- `metrics-exporter-prometheus` 于 `127.0.0.1:9898`；`AlertStore`（append-only JSONL + 300 s 指纹去重）→ `recent_alerts/watch_alerts` → opsd → Telegram；`DiskPolicy` 分级 → `DiskProtection` + 自动暂停；`tracing` JSON → journald。
- CLI：`trading-collectorctl {status|health|pause-all|resume-all|pause-symbol|resume-symbol|reload-catalog|rotate|drain|alerts}`；`trading-capturectl {policy|catalog{activate,recover,requeue}|instruments|start|schedule|pause|resume|stop|status}`（`systemd-run` 瞬态单元 `trading-capture-*`）；`trading-replay {inspect|gaps|records|recover|books}`；`trading-catalog-builder {build|audit}`；`tools/local_mock_exchange.py`。
- CI（`.github/workflows`）：`quality-gates.yml` = fmt / clippy `-D warnings` / test / `cargo audit`（新 RUSTSEC 会无改动变红）/ Python mypy+pytest / bats；`release-arm64.yml` = tag → preflight → `ubuntu-24.04-arm` 原生构建 → `file` 校验 → `package-release.sh` 产 `RELEASE.json`（版本/SHA/arch/逐文件 sha256）。服务器 `fetch-release.sh --version X`（只读 PAT）→ `releasectl.sh verify|install|activate|rollback`（`/opt/trading/releases/<v>` + `current`，保留 active+1 rollback）。
- systemd（`deploy/systemd/`）：`trading-collector`（notify，`Restart=always`，`ExecStop=-collectorctl drain`，`TimeoutStopSec=120`，`ProtectSystem=strict/NoNewPrivileges/CapabilityBoundingSet=`）、`trading-processor`（`CPUWeight=10`）、`trading-uploader`、`trading-opsd`（`RuntimeDirectory=trading` 唯一所有者，`IPAddressDeny=any`）、`trading-telegram`、三个 timer；polkit 限定 `trading-ops` 可控单元。
- AGENTS.md 事故沉淀：四小时同步会话拆解；`install -d -o` 只作用最深层目录 → EACCES 丢一天采集（**以服务用户 dry-run 单元命令**）；`Requires=` 拉起崩溃循环 57,915 次；双单元 `RuntimeDirectory` 互抢 chown 掉 `ops.sock`（v0.3.4）；Actions `success()` 沿 `needs` 链隐式跳过 → v0.2.4 无资产。
- 测试资产：`tests/fixtures/{binance,okx,bitget}/*.json` 真实报文；各 crate 同步器状态机/wiremock/replay 一致性测试；`apps/collector/tests/mock_end_to_end.rs`；`apps/processor/tests/cold_pipeline.rs`；`deploy/scripts/tests/deployment.bats`。

---

## 2. cdex-arb（老板的实盘套利栈）

### 2.1 两条执行路径、两个账户、四个常驻服务

- **DEX 路径**（主线，当前 LIVE）：Binance USDT 永续开空 + BSC PancakeSwap（V2/V3/V4 Infinity/Uniswap V3/V4）或 Solana Jupiter 买现货；平仓反之。买价差 `buy_spread = (bn_bid − chain_buy)/chain_buy`，卖价差 `sell_spread = (bn_ask − chain_sell)/chain_sell`（老板定义）。
- **CEX-CEX 路径**：Binance 永续 × Gate / KuCoin 现货，Rust `rust/cex-arb` 引擎，由 Telegram 经 `utils/rust_launcher.py` 以 `dry_run=false` 硬编码、`start_new_session=True` 起独立进程（无确认，见 memory `cdex-arb-system`）。
- **双实例**：`~/Trading/cdex-arb`（测试，`CDEX_ACCOUNT=test`，API :8000）与 `~/cdex-prod`（正式，`CDEX_ACCOUNT=prod`，API :8001）；一套 Telegram bot 经 `~/.cdex-keys/active_account` 指针切换（`/account prod|test`，KillMode=process 重启 ~10 s），跨账户只读 `/p prod` `/b prod` `/pl prod`；每条消息前缀 `[🧪测试]/[💰正式]`；非活动账户的推送写 `logs/telegram_suppressed.jsonl`。代码在测试目录，补丁后 `sync-code.sh` 同步到 prod。
- systemd（ubuntu 用户，无沙箱，`E:\trading\analysis\cdex-systemd-install.sh`、`cdex-srv/cdex-pipeline.service`）：`cdex-api`（FastAPI，`MemoryMax=900M`）、`cdex-telegram`、`cdex-monitor`（monitor_lite，唯一发 Bark 的进程）、`cdex-pipeline`（`KillMode=process`，子进程机器人存活）、`cdex-scanner`、`cdex-spread` / `cdex-prod-spread`（价差引擎）。**与采集器共用同一台 2 核机器**（9/23 死循环把负载打到 3.5）。

### 2.2 Python 包地图（服务器最新镜像）

| 包 / 文件 | LOC | 作用 |
|---|---:|---|
| `core/config_types.py` | 144 | `TokenConfig`（chain, token_symbol, cex_symbol, token_address, base_token_address, decimals, pool_fee, pool_address, slippage_bps, base_to_usdt_symbol, use_wbnb_routing, use_v2_swap, v3_swap_router/quoter, use_v4_swap + v4_pool_key{currency0,currency1,hooks,poolManager,fee,parameters}, use_univ4_swap + univ4_pool_id, use_1inch, use_sol_routing）、`TradingConfig`（trade_amount, spread_threshold, max_spread_threshold, max_position, open_wait_seconds, dex_poll_interval, close_spread_threshold, close_amount, dex_retry_max/delay）、`FullConfig`（+凭据） |
| `core/base_trader.py` | 142 | `DEXTrader` ABC：`get_price(base_amount)`, `buy_token`, `sell_token`, `get_token_balance`, `get_base_balance`；`TradeResult{success, tx_hash, amount_in/out, price, fee, fee_asset, error, simulation}` |
| `core/open_position_bot.py` / `close_position_bot.py` | 1,140 / 1,019 | 每币机器人内核：WS `fstream.binance.com/ws/<sym>@bookTicker` + 线程轮询 DEX（`dex_poll_interval` 1 s）；`_do_open_async()` / `_do_close_async()` 是引擎复用的执行内核 |
| `core/cex_depth.py` | 64 | 币安 20 档 VWAP 复核（顶档量 ≥ 2×单笔直接用；否则 REST depth，吃不下或偏离 >0.15% 不做并把单笔减半） |
| `core/instant_trade.py`, `cex_*_bot.py`, `gate_*_bot.py` | 557 / ~1,800 | Telegram `/open` `/close` 单次开平；CEX-CEX Python 版机器人（已被 Rust 替代） |
| `traders/bsc_trader.py` | 1,908 | BSC 全部报价/下单/回执逻辑（见 2.4） |
| `traders/bsc_route.py` | 187 | 多一跳路由（USDT→WBNB/ETH/BTCB→token，V3 `exactInput(path)` 或 V4 Universal Router 一笔 `execute`） |
| `traders/solana_trader.py` | 984 | Jupiter `swap/v1|v6 quote+swap`，`get_signature_statuses` + `get_transaction` 两阶段确认 |
| `traders/binance_trader.py` | 603 | REST `fapi` HMAC：`market_short/market_close`（MARKET，`/fapi/v1/order` + `userTrades` 回查均价）、positionRisk、bookTicker |
| `traders/gateio_spot_ws.py`, `kucoin_spot_trader.py` | 614 / 377 | Gate WS v4 / KuCoin REST 现货 |
| `utils/bsc_price.py` | 268 | 9/25 统一 BSC 取价入口：有交易配置 → 用 `BSCTrader` 同源报价（USDT 计价）；无配置 → `dex_price`；USDC/USD1/FDUSD/WBNB 汇率 30 s 缓存，BNB 取不到返回 None |
| `utils/dex_price.py` | 637 | 底层 V2 `getAmountsOut` / V3 `quoteExactInputSingle` / V4 `getSlot0` sqrtPrice / UniV4 StateView / 1inch / Jupiter |
| `monitoring/spread_engine.py` | 2,074 | **tick 级价差引擎**（见 2.5） |
| `monitoring/sizing.py` / `close_policy.py` / `watchlist.py` / `trade_pnl.py` | 166 / 143 / 116 / 211 | 仓位上限、动态平仓线、滚动名单、开平推送盈亏 |
| `monitoring/dex_pipeline.py` | 1,186 | 60 s 一轮全自动流水线（找币→核验→写配置→起/停机器人→清理） |
| `monitoring/opportunity_scanner.py` / `pool_finder.py` / `token_check.py` | 591 / 987 / 587 | 60 s 盯盘 + 30 min 找币；DexScreener/GeckoTerminal 找池 + 五道门；72h 历史推荐阈值 |
| `monitoring/telegram_monitor_secure.py` | 6,213 | Telegram 命令面（45 个 `cmd_*`）+ 20 min 面板推送 |
| `monitoring/accounts.py`, `portfolio_view.py`, `settlement.py`, `bridge.py`, `binance_transfer.py` | ~1,600 | 双账户、资产面板、结算报告、跨链桥、币安划转 |
| `api_service/` | 2,506 | FastAPI 127.0.0.1:8000；`spread_service.py` 维护 Binance/KuCoin/Gate 多币 WS（`websocket-client`，`ssl.CERT_NONE`）+ DEX 2 s 轮询；routers `binance/bsc/solana/portfolio/trading/spreads` |
| `monitor_lite/` | 3,172 | 3 s 刷新的持仓监控（ADL ≥5、池深 <$150k、强平距离 <30%、价差 >5%、负费率 → Bark critical + TG，每条件最多 2 次） |
| `configs/{bsc,solana,cex,gate}/<token>.py` | ~3k | **每币一个 Python 模块**导出 `TOKEN/TRADING/get_config()`（流水线自动生成的带 `pipeline_auto` 头） |
| `rust/cex-arb` | 3,777 | CEX-CEX Rust 引擎（见 2.3） |
| 根目录 `manager.py`, `main_manager.py`, `gate_manager.py`, `kucoin_manager.py`, `run_open/close/autobot.py`, `check_portfolio.py` | ~4.3k | 旧的交互式菜单/启动器 |

**参数源有三个并存**：每币 `configs/*.py`（静态）、`pipeline_config.json`（流水线，热加载，唯一被设计为「单一参数源」）、`engine_config.json`（引擎，热加载）；运行态用 JSON 文件当 IPC：`logs/engine_status.json`（`/engine` 读）、`logs/engine.ctl.json`（遥控 `{seq, execute, add, rm, refresh, unhalt}`）、`logs/pipeline.ctl.json`、`logs/engine/signals.jsonl`、`logs/engine_samples/<币>.jsonl`（10 s）、成交 CSV `logs/trades_<SYM>_<chain>_*.csv` / `closes_*.csv`（`trade_pnl.py` 读它算盈亏）。

### 2.3 Rust `cex-arb` 引擎（`cdex-arb/rust/cex-arb/src`）

- `main.rs`：`#[tokio::main(flavor = "current_thread")]`；启动时 `apply_market_rules()` 从交易所拉精度/step/minFunds（single source of truth）；`Executor::from_env()` 失败即退出（TG 靠 PID 存活探测）；`mpsc<BboUpdate>(256)`；每 25 s `ping_all()`。
- `feed.rs`：BN futures `bookTicker` 组合流 + Gate `spot.book_ticker` / KuCoin；**手写字节扫描** `find_bytes/extract_num`（无 serde），`recv_us` 在 parse 前采样；`set_nodelay`。
- `bot_open.rs` / `bot_close.rs`：事件驱动循环，`select!{ctrl_c, rx.recv()}` 后 `try_recv` **排空队列取最新 BBO**，信号持续 `signal_persist_seconds`，`open_wait_seconds` 冷却期间仍更新价格，连续 3 次失败熔断（`MAX_CONSECUTIVE_FAILURES`），`max_position` 到达停止。`spread.rs`：`calculate_bbo_trade_qty` 取两边顶档 USDT 价值较薄侧 × `depth_fraction`，floor 到 step。
- `exchange/executor.rs`：`execute_open` = `tokio::join!(bn.market_sell_precise, spot_buy)` **双腿并行**，`handle_dual_result` 一腿失败 → 用实际成交量重试另一腿 → 再失败回滚成功腿 → 回滚失败置 `halted`。
- `exchange/bn_futures.rs`：**下单走 `wss://ws-fapi.binance.com/ws-fapi/v1` + Ed25519 `session.logon`**（`ed25519-dalek` 解 PKCS8 DER base64），查询走 REST HMAC；`REQUEST_TIMEOUT 5 s`；KuCoin `wss://wsapi.kucoin.com/v1/private` `spot.sync_order`（~35 ms vs REST ~390 ms）；Gate `wss://api.gateio.ws/ws/v4/` 登录后 `spot.order_place`。`signing.rs` **手写 HMAC-SHA256/512 与 base64**（未用 `hmac` crate）。
- 依赖 `reqwest native-tls`、`tokio-tungstenite`；无 core pinning；配置 TOML 由 Python 生成到 `logs/rust_configs/`。

### 2.4 链上腿：BSC 定价 / 发送 / 回执（`traders/bsc_trader.py`，最新镜像）

- **定价**：V2 `getAmountsOut`；V3 `QuoterV2.quoteExactInputSingle`（PancakeSwap `0xB048…`，或自定义 Uniswap V3 quoter）；V4 Infinity 先走 `CLQuoter` 真实模拟（含冲击、买卖分开），失败退回 `CLPoolManager.getSlot0` sqrtPrice 中间价；Uniswap V4 `StateView.getSlot0`（只报价不交易）；1inch 聚合器；多一跳 `bsc_route`（`quoteExactInput(path)`）。所有 `eth_call` 走 web3.py `HTTPProvider` 同步调用。
- **RPC 分层**（9/24 事故后）：读走 `BSC_RPC_URLS`（按 pid 分摊多个公共节点），**发交易走 `BSC_TX_RPC_URL`**；`_rpc()` 多节点兜底；`send_raw_transaction` 前节点超时后节点报 `nonce too low` = 已上链而非失败（`:416-432`，防止换 nonce 重发同一笔）；回执轮询 0.5 s（原 50 ms 把公共节点打到 403）、节点报错换下一个；`receipt.status == 1` 才算成功。
- **Gas / nonce / 限价**：`gasPrice = max(网络价 × 1.2, BSC_GAS_PRICE_MIN_GWEI=0.1)`（Maxwell 后 0.45 s 出块按 gasPrice 排序）；`nonce = get_transaction_count(pending)`；`deadline` 600 s（路由）/ 3600 s（V4）/ 86400 s（V3 老路径）；9/25 起 `buy_token(max_price=)` / `sell_token(min_price=)` **从信号要求的价差反推限价**，`min_out` 不再用发送时的新报价（NAORIS 事故根因）。
- **收尾**：`_extract_actual_amounts_from_receipt` 解析 Transfer 事件取真实成交量；gas 费 = `gasUsed × effectiveGasPrice`。
- **Solana**（`solana_trader.py`）：Jupiter `quote` → `swap`（有 key 走 `api.jup.ag/swap/v1`，否则 `quote-api.jup.ag/v6`）→ `send_transaction` → `get_signature_statuses` 1 s 轮询 ≤30 s → `get_transaction(Confirmed)` 解析余额变化；9/23 起 `status.err` / `meta.err` 非空按失败（此前「确认≠成功」导致 ARC 幻影成交、82 ARC 裸多）。免费档 Jupiter 每机器人 1 次/s 询价，>3 个并发 429。

### 2.5 tick 级价差引擎（`monitoring/spread_engine.py`，9/24 起 LIVE，正式实例）

- **结构**：一个进程盯 configs/bsc 全部币（正式 30–37 个）。`BinanceFeed` 线程：`wss://fstream.binance.com/stream?streams=…@bookTicker` 合并流（`websocket-client run_forever`，实测时延 p50 2 ms，14 币 ~300 msg/s）。`ChainQuoter.quote_all()`：**Multicall3 `aggregate3` 一次 `eth_call` 批量报所有币的买价（单笔金额）/卖价（持仓量）/`balanceOf`**，按 `kind ∈ {v2, wbnb, v3, v3r, v4, v4r}` 组 calldata；`max_calls_per_chunk=12`（publicnode gas 上限），分块按节点时延加权轮流落到 7 个 `BSC_RPC_URLS`，节点熔断（2 次失败 120 s，403 → 600 s）；分级节拍：持仓/接近阈值币每区块报，远的每 4 轮，垃圾每 20 轮；非持仓币卖价每 3 轮、余额每 75 轮。`_block_feed` 订阅 `wss://bsc-rpc.publicnode.com` `newHeads`，新区块一到就报价（`min_quote_gap_ms=120`），兜底 `poll_ms=400`。实测 22–60 ms/轮。
- **信号**（`evaluate()` `:936-990`）：两边新鲜度门（合并流 <3 s、该币链上 <1.5 s）→ `|spread| > 30%` 脏数据 → `halted` 跳过 → 三条通道互斥：`armed`（`open_th` 且非 watch-only）、`spike`（未验证币，价差 ≥3% 且比 1 h 常态高 1.5 点，半仓一笔）、`carry`（价差盖住成本且资金费可观，半仓拿费）；`room = cap − held ≥ $6`、`not in_flight`、`now ≥ cooldown_until`（20 s）、`_exposure_ok`（账户级：全部持仓名义 + 在开 + 本笔 ≤ 币安钱包 × 1.5）；达标需**持续 `confirm_ms=600`（正式建议 900 ≈ 2 区块）且期间 ≥ `min_chain_quotes=2` 次链上报价达标**（挡单区块尖峰）。平仓：`sell_sp ≤ close_th`（close_policy 每分钟给）同样确认。
- **执行**（`fire()` → `ThreadPoolExecutor(max_workers=1)` 串行防抢 nonce → `_execute()` `:1325`）：把 bid/ask/顶档量/链上价灌进该币的 `OpenPositionBot` / `ClosePositionBot`，`asyncio.run(bot._do_open_async())`。内核流程（`open_position_bot.py:594` 起，9/25 后）：**先链上限价买（`max_price` = 币安可成交价 /(1 + 开仓线 − `limit_tol_pct` 0.3)）→ 成交后按实际买到币数在币安开空**；下单前 `get_price(fresh=True)` + `cex_depth.check` 20 档 VWAP；链上回执/余额读不到 → `halt_reason` 停该币等人工 `/engine unhalt`；币安开空连续 2 轮失败/保证金不足 → halt；链上滑点 >0.15% 下一笔减半。收尾：刷新余额/持仓、`run_close_policy(only=c)`、`_flatten_residual`（两腿 <$5 但币安多出 → reduce-only 平残余裸空）、`_hedge_fix`（两腿差 >5% 自动把多的一边平掉）。
- **风控开关**：`check_daily_loss()` 当日 UTC 币安账本（已实现+资金费+手续费）亏损 > 钱包 × `daily_loss_kill_pct=3%` → `execute=false` 切回影子 + 报警；`_take_halt` 按币停；`execute` 默认 false = 影子模式只记 `signals.jsonl`。
- **参数**（每 30 min `refresh_params`）：`open_th = max(72h 历史推荐, 按池费率成本线, min_open_th=1.8)`（9/25 v12 改：24h 出场价差中位数 + 价差外成本，下限 0.6）；**允许开仓条件**：历史 ✅/🟡，或引擎自己攒 ≥24 h 样本且 ≥3 h `buy_spread ≥ 成本线`，否则 👁 watch-only；1 h 买价差中位数 |>5%| → watch-only（坏池守卫）。
- **资源**：18 币 CPU ≈17%、100 MB；正式 3 币 2%。

### 2.6 仓位 / 平仓 / 名单 / 流水线 / 找币

- `sizing.py compute()`：上限 = min(硬上限 `per_coin_cap_usd` 150，账户占比 钱包×1.5×25%，**实际交易池计价币一侧** × 0.3/0.5/0.75%（<$100k/$100–500k/>$500k），池 24h 成交 × 1%，币安 OI × 0.05%，币安 24h 成交 × 0.02%，冲击探测：上限金额真实报价 vs $5 报价冲击 >0.3% 减半 ≤4 次）；单笔 = min(上限, max($6, 上限/2), 币安顶档 × 50%, `chunk_usd` 40)。`/engine size 币` 看谁在卡。9/26 分析：30 个币卡在硬上限 150，结论「保守了」。
- `close_policy.effective_close_threshold()`：基础 = 入场价差 − 成本线 − 0.5%，夹 [−1.0, +0.3]；资金费日化 <0.05% → 保本 +0.1；负费率/连续 2 期负 → 保本 + 已收费一半；>48 h → 24 h 卖价差 p20，>96 h → p40；最多净亏 0.3%；**硬动作**：强平距离 <15% 或 ADL ≥4 → `reduce_half`，<8% 或币价 +80% → `close_all`，两腿差 >5% → `hedge_fix`。ADL 未接线（恒 0）。
- `watchlist.refresh()`（30 min）：币安 USDT 永续正费率、24h 成交 ≥$2M、日化 ≥0.05%、OI ≥$2M（`openInterestHist.sumOpenInterestValue`），`score = 日化 × log10(OI)`，前 30 未配置 → `pool_finder`（只 BSC）→ 五道门 → 深度上限 ≥$6 → 自动写配置；黑名单 FLUID/BTW（同名假币）。
- `dex_pipeline.one_cycle()`（60 s）：快照（币安余额/持仓、两链钱包、机器人、费率；任一失败本轮不动）→ 先处理有仓的币 → 找币（30 min，`discovery`）→ `decide()`（钱够/没仓/历史不红/实时价与币安偏离 <10%）→ `launch()` 起 `run_autobot` 子进程；`executor="engine"` 后不再起机器人只找币/清理/记账；失败计数（0 笔异常停 → 冷却 30 min，3 次拉黑 24 h）；每天 20:00 UTC 清理 7 天没机会的自动配置。
- `pool_finder` **五道门**：价格偏离 ≥10% 假币 / ≥2% 非同资产；身份（币安 `capital/config/getall` 官方地址、GeckoTerminal FDV、CoinGecko 市值 <5% 仿盘）；深度真伪（流动性 >$50k 但换手 <1%、Jupiter $22 冲击 ≥50%）；72 h 价差历史（到过成本线从没回落 🔴、中位数 >50% 🔴）；可成交价（Jupiter / CLQuoter / quoter 真报价，DexScreener `priceUsd` 不用于决策）。股票代币别名 `{SYM}ON/{SYM}X/{SYM}B`。
- `token_check.round_trip_cost()`：成本线 = max(链估算, 0.10 + 2×池费 + gas + 0.10) + 0.3 执行 + 0.2 余量（1% 费池 → 3.2%）；推荐阈值只在 0.5–20% 搜（`SCAN_MAX_PCT`，9/23 死循环修复）。

### 2.7 主循环与时延花在哪

| 环节 | 现状（最新镜像） | 评价 |
|---|---|---|
| 币安 BBO 到达 | Python `websocket-client` 线程回调 → `_on_bbo` → `evaluate()`；p50 2 ms 网络 + GIL/线程切换 | 每 tick 触发全 Python 评估；`evaluate` 本身 O(1) 但 `time.time()`、dict 属性访问、日志 |
| 链上报价 | Multicall3 `eth_call` 同步 HTTP 到公共节点，22–60 ms/轮，块触发 + 400 ms 兜底；**最多每 0.45 s 看一次池子** | 最大时延源之一；无本地节点、无 mempool、无 gRPC 流；公共节点 403/限流是常态（9/24 全 dataseed 封 5 h） |
| 确认窗口 | 600–900 ms + ≥2 次链上报价 | 有意为之（挡尖峰），但 9/25 后有限价保护，文档建议可缩到 ~1 区块 |
| 下单前复核 | `get_price(fresh=True)` 一次 `eth_call` + 币安 REST depth 20 档（~60 ms） | 串行 REST |
| 链上腿 | web3.py 构造/签名/`send_raw_transaction` 到 `BSC_TX_RPC_URL`（公共）→ 下一区块 → 回执 0.5 s 轮询；**成交时延中位 1.25–1.3 s**；块内排位 20–40（对手 5–12） | 「追价」信号 ~80% 被抢先；报价→成交滑点均值 +0.08–0.15%，最差 +1.51% |
| 币安腿 | `requests` REST `POST /fapi/v1/order` HMAC + `userTrades` 回查；成交中位 0.26 s（链上先后 1.4 s 含等待） | 无 WS API、无 Ed25519 session（Rust 侧有但 Python 侧没用） |
| 执行串行 | `ThreadPoolExecutor(1)` + 每次 `asyncio.run()` 新建事件循环 | 一个币在执行时其他币信号排队；`_execute` 内所有 I/O 同步 |
| 参数/名单/费率 | 30 min 重算，`urllib` 同步 REST（币安 OI 40 ms/symbol、GeckoTerminal/DexScreener 限流） | 后台线程池 `bg/bg2/bg3` |
| 监控/面板 | `telegram_monitor_secure` 6.2k 行同步 REST，20 min 推送；`api_service` 独立 WS + 2 s DEX 轮询；`monitor_lite` 3 s 全量 REST | 三套独立行情连接，重复订阅，重复 RPC |

结论：**从「知道」到「上链」约 0.7–1.0 s，从「信号」到「双腿完成」约 2.5–3 s**，其中 Python 开销是次要项，主导项是**区块级采样 + 公共 RPC + 下一区块成交 + 串行执行**。9/26 分析给出的优先级：放宽上限（配置）→ 按资金占用决定平仓耐心/换仓 → 回执快速轮询（前 3 s 每 0.15 s）→ gas 加速/私有打包（48Club `fast.bsc-rpc.com`、blockrazor）→ Helius/Yellowstone gRPC → 自建 BSC 快节点。

### 2.8 Telegram / API / 监控（运维依赖面）

`telegram_monitor_secure.py` 45 个命令：资产 `/portfolio(/p) /pnl /balance /position /holdings /total /nowfunding /funding`；交易 `/open /close /yes /no`（预览 → 60 s 确认）；机器人 `/autobot /autobots /autoclose /bots /bot /setbot /pausebot /resumebot /stopbot /restartbot /botlog`；引擎 `/engine [on|off|add|rm|refresh|size|unhalt]`；流水线 `/pipeline [live|dry|on|off|run]`；名单/核验 `/watchlist /check /pool /savecfg /delcfg /why /alerts /ack`；对冲/资金 `/hedgefix /bridge /rebalance`；账户 `/account [test|prod]`；`/monitor /ip /ipnotify /menu /help`（7 页手册）。**授权 = chat_id 白名单，无只读角色**（群组成员=全权）。`api_service` FastAPI 127.0.0.1:8000（`/api/config`, `/portfolio`, `/trading/{preview,open,close}`, `/spreads`…）+ `dashboard/index.html`。`monitor_lite` 是唯一 Bark（iOS critical）通道。

---

## 3. portfolio-arb（Rust）与 oil-arb（Python）

### 3.1 portfolio-arb（`portfolio-arb/`，独立 Cargo workspace，12.7k LOC，2026-09-15 自 MM-Sodex 导入）

- **交易**：Binance USDT-M 永续空 × Gate 现货多（31 币，KuCoin 腿 5/29 归档），收 funding + basis 收敛。`config/portfolio.toml`：fees（BN 5 bp / Gate 1.6 bp）、strategy（`p_entry_high 75` 7d 分位、`entry_persistence_ms 1000`、`min_entry_spread_bp −200`）、sizing（base $100 / max $500、`bbo_fill_ratio 0.5`、`min_order_usd 6`）、risk `max_total_notional_usd 200k`（**未被运行时读取**，strategy-review P1）。
- **架构**（`docs/ARCHITECTURE_DEEP_DIVE.md`，非常详尽）：MD `feed.rs`（BN/Gate BBO，手写字节扫描）+ `funding.rs`（markPrice WS + REST 历史）→ 单一 BBO 消费循环（`mpsc(8192)`，`try_send` 满则**静默丢**）→ `score.rs` 1 Hz（资金费归一 4h、7d `PercentileTracker` 分位、power-law 分配 + max_share + 迭代 redistribute、`basis_kick`）→ `reconciler.rs` 1 Hz 六道闸门（noise floor / cost-vs-opportunity 滞回 break-even 天数 / settlement timing / BN:spot 40-60 / capital_in_use / venue capital，drift band 强制）→ `oms.rs` 串行 `LegOrder` 消费 → `exchange/executor.rs` `tokio::join!` 双腿 + 重试/回滚 → SQLite（`db.rs` 760 行：trades、external_trades、funding、position_snapshot、alert_state、score_log）+ 5 种定时对账（`spot_recon/trade_recon/avg_recon/fee_recon/manual_apply`）+ `tg_alert`（supervised spawn，panic → critical alert + stop）。`user_data.rs` BN listenKey WS。`latency.rs` 有测速插桩框架（`LATENCY_PROBE`）。`tune/` 离线 6d 回放 + 网格寻优，每日 03:00 子进程 → `state/optimized_params.toml` 热加载（≤36 h）。
- **已知缺陷**（DEEP_DIVE §2 + strategy-review）：默认多线程 tokio、无 pinning、`spawn_blocking` 从未用（rusqlite/blocking reqwest 直接在 worker 上跑）；`pf.tracker` 双层锁 + O(n) 有序插入在 BBO 消费线程内联；`params::current()` 每次 clone 22 字段结构；单 `Mutex<Connection>` 串行所有 DB；**WS 下单 `read` Mutex 跨 await 持有到回报（≤5 s）→ 每客户端一单在飞**；下单超时当普通 Err 重试无「先查原单」状态机（P0 重复成交风险）；两腿量不等用 min 更新仓位（P0 净敞口未即时修）；drift_override 从零仓开始恰为 100% 跳过成本闸；`.env` 读上级目录。
- **可复用**：策略规格文档（`docs/strategy_spec.md` §3–4 评分/闸门链公式）、Ed25519 ws-fapi 客户端、Gate WS 下单、`latency.rs` 插桩思路、SQLite schema、DEEP_DIVE 的性能诊断清单。

### 3.2 oil-arb（`oil-arb/`，Python 10k LOC）

Gate 永续 `BZ_USDT`（Brent）× `CL_USDT`（WTI）价差 4 h 滚动 z-score 均值回归（`z_entry 1.5`、`min_deviation 0.35`、6 档分层加仓、追踪止盈 1.0σ/0.5σ、无硬止损、跳变保护、μ-drift 闸）；`arb_bot.py` 1,602 行 asyncio；`gate_ws_api_client.py` Gate WS v4 下单/批量/改单（orjson）；`oil_ws_adapter.py` 单 WS 双合约 `book_ticker`（60 s staleness）；`auto_tune.py` 4 h cron → `state/oil_optimized.toml`。strategy-review 已复现 3 个 P0：IOC `finished` 零成交仍返回成功（`arb_bot.py:917`）、双腿各 10 张实成 2/5 本地记 10（`:1101`）、下单超时 position 置 None 且对账跳过（`:1083, :507`）。回测脚本群（黄金/跨所/周末/止损比较）是研究资产而非运行代码。**对 v2 主要价值是「相对价值 + 分层 + 追踪」策略模板和 Gate WS API 用法，不是代码。**

---

## 4. python/telegram_bot（采集系统运维面）

`src/trading_telegram/{bot.py 423, opsd.py 1180, uds.py 227, render.py 539, config.py}`。`bot.py` 用 `python-telegram-bot` 注册 `/status /health /alerts /audit /instruments /add /remove /pause_symbol /resume_symbol /pause /resume /start /stop /schedule /jobs /cancel /rotate /help`；危险命令走 `prepare → 内联按钮 confirm/reject`（60 s，绑定管理员/私聊/请求哈希/操作前状态；Telegram update_id 派生稳定 request_id 保证重试幂等）。`opsd.py`：UDS 服务 `ops.sock`，sqlite 审计/任务/告警游标，`WRITE_COMMANDS` 限速 12/分，`FixedCommandRunner` 只允许固定可执行文件（capturectl / catalog-builder），`systemd-run` 调度采集窗口（Asia/Shanghai），`watch_alerts` 长轮询 → 推送。`uds.py`：与 Rust `crates/control` 同一帧协议（u32 大端 + JSON，1 MiB，30 s 超时）。**这是原型里唯一有角色分离（trading / trading-ops / trading-bot 三用户）、两步确认、审计、幂等的运维面**，与 cdex-arb 的平铺白名单形成鲜明对比。

---

## 5. 文档 / 规范 / 部署 / 分析报告：策略逻辑与事故沉淀

### 5.1 文档资产

`docs/superpowers/specs/*design.md`（采集系统 21 章规格，含时间语义、故障语义、验收）、`docs/superpowers/plans/*`（4 个实施计划 + roadmap 含跨计划类型契约）、`docs/runbooks/{development, deploy, acceptance, collection-policy, latency, alerts, reliability-recovery, telegram-ops, telegram-setup, bitget-integration, cold-pipeline, live-smoke, local-smoke, 2026-08-27-latency-baseline, 2026-08-27-public-v2-acceptance}.md`、`schemas/{wal-v1,wal-v2,parquet-v1..v3,control-v1,collection-policy-v1}.md`。cdex 侧：`COMMANDS.md`、`OPERATION_GUIDE.md`（老板写的操作手册）、`E:\trading\analysis\cdex-arb-新手操作手册.md`、`cdex-arb-deploy-plan.md`（安全审计 + API key 清单）、`COBbasis/docs/strategy_spec_v2.md`。无 ADR 目录；设计决策散在 spec 与 AGENTS.md。

### 5.2 分析报告要点（E:\trading\analysis）

| 文档 | 结论 |
|---|---|
| `cdex-价差引擎设计-2026-09-24.md` | 老板代码里没有链上 tick 级开仓程序（`spread_monitor.py` 只是看板，Rust 只做 CEX-CEX）；自建引擎：一进程多币、合并流 + Multicall3、0.6 s 确认、复用机器人内核、滚动名单、动态平仓线；已知问题 ADL 未接、两腿失配只提醒 |
| `cdex-价差引擎影子试跑-2026-09-24.md` | 10.1 h 零错误、6 条影子信号；AIN 信号是 1–2 区块下探 → 加 `min_chain_quotes=2`；**事故**：4 节点 × 2 引擎并行 multicall → 全部 dataseed 403 五小时 → 节点熔断/分级节拍/12 call 上限；建议付费 RPC |
| `cdex-正式实盘启动-2026-09-24.md` | 单币上限七约束取最小 + 单笔至少两笔 + 20 档 VWAP + 滑点减半 |
| `cdex-正式账户复盘-2026-09-25.md` | 31 笔；NAORIS「高费率开、−0.3 平」：**滑点保护只防发出→上链，从不检查触发价**（`min_out` 用发送时新报价）→ P0 限价从信号价差反推、**开仓先链上后币安**、买入后立即 approve；carry 与价差通道混仓、残余裸空 5 张、推送账本错、报警噪音 |
| `cdex-策略优化分析-2026-09-26.md` | 先链上后币安优于同时下单（revert 时无需回滚币安）；「追价」信号 ~80% 被抢先，链上滑点均值 +0.08%；资金费没了多等只多赚 0.1–0.2%；**上限太保守**（30 币卡 $150）；优先级：放宽上限 → 平仓耐心/换仓 → 回执快轮询 → gas/私有打包 |
| `cdex-自动化流程核验-2026-09-23.md` | P0-1 `recommend()` 死循环烧 2 核 7.5 h（假币 10⁵% 价差）；P0-2 Solana 确认≠成功幻影成交 → ARC 裸多；新 sizing、五道门、VWAP、插针分类表 |
| `DEX加速与AIN插针复盘.md` | AIN 9/20 插针平仓：单点采样、失败重试不重判、1h 收盘历史看不见秒级尖峰；时延花在区块级采样 + 下一区块成交而非网络（11–15 ms）；技术地图：48Club fast/privacy RPC、Helius、Yellowstone gRPC、Jito bundle、自建节点、backrun |
| `cdex-dex-pipeline-设计.md` | 从 portfolio-arb 借的原则：单一参数源热加载、快照失败本轮不动、失败计数熔断、灰尘不碰、`deployable_frac/max_share/bn_reserve_frac`、信号持续、dry/live、关仓优先 |
| `strategy-review-20260916/策略分析.md` | portfolio-arb 值得继续（高于 oil-arb），但两者都不适合无人值守：P0 订单状态未知处理、两腿量差、总敞口未生效 |
| `spread-arbitrage-6h*/`（Rust 读 WAL） | 6 h Binance×OKX bookTicker：23 对 tokenized 股票/商品有第一档毛套利证据（SKHYNIX 正边际 98%、P95 9.8 bp；ZHIPU、NVDA、SNDK、SAMSUNG、CRCL、XAU、MU）；ANTHROPIC/OPENAI 合约语义不一致；未扣费 |

### 5.3 事故清单（v2 必须在设计上消灭的类别）

| 事故 | 根因类别 | v2 设计含义 |
|---|---|---|
| 9/24 交易节点 403 把已上链交易当失败 → 裸敞口 | 单一 RPC + 把 HTTP 错误当交易失败 | 交易状态机需 UNKNOWN 态；多节点回执 + 余额/nonce 核对；发送与读取节点分离 |
| 9/23 Solana 幻影成交（确认≠成功） | 只看 confirmation 不看 `err`，None 退回报价 | 成交量只能来自链上事件/余额差，禁止「用报价当成交」 |
| 9/20 AIN 插针平仓 | 单点采样、重试不重判、历史口径不一致 | 多区块/多次报价确认；每次重试重新评估；决策与回测同一数据源 |
| 9/25 NAORIS 滑点保护失效 | `min_out` 基于发送时报价 | 限价从信号反推；链上腿先行；approve 预做 |
| 9/23 `recommend()` 死循环 2 核 7.5 h | 无上界扫描 + 假币极端值 | 所有循环有界；watchdog；采集与交易不共机 |
| 9/24 并行 multicall 打爆全部公共节点 | 免费 RPC 依赖 | 付费/自建节点；节点熔断；请求预算 |
| Rust 引擎缺 Ed25519 key 无法启动、无确认直接实盘 | 凭据/权限模型缺失 | 只读角色、两步确认、dry/shadow/live 三态 |
| 采集：512 MiB 滚动无快照 84% 隔离；RuntimeDirectory 互抢；EACCES 丢一天 | 格式不变量、systemd 细节、以 root 验证 | segment 自含快照；单一目录所有者；以服务用户 dry-run |

---

## 6. Keep / Borrow / Rewrite 判定与隐含需求

### 6.1 判定表

| 组件 | 判定 | 理由 |
|---|---|---|
| `crates/domain`（Exchange/Market/InstrumentId/ClockStamp/Quantum/BookLevel） | **Keep（扩展）** | 干净、无依赖、整数域正确；v2 加交易侧类型（Order/Fill/Position/Venue 余额）与更多 Exchange/Market（Gate、KuCoin、BSC/Solana 链、Ondo/xStocks 等分类） |
| `crates/wal` 格式 + `schemas/wal-v2.md` | **Keep 格式，Borrow 实现** | 字节级规范成熟、S3 上已有历史数据；v2 写端需零拷贝/预分配/`io_uring` 或 mmap、segment 自含快照不变量、私有域加密；读端可直接复用 |
| `crates/orderbook` | **Borrow 作为参考实现** | 正确性测试与 hash 用于验证；热路径改为固定深度数组/价格索引的 L1/L2 结构 |
| 三个交易所 crate 的 `depth.rs` 同步器状态机 + `tests/fixtures` | **Keep 逻辑，Rewrite 外壳** | gap/checksum/epoch 语义经实盘验证；WS 外壳需换成单线程 reactor、`&[u8]` 借用解析、无 `serde_json::Value` |
| `crates/control` + `schemas/control-v1.md` + opsd/bot | **Keep 协议与权限模型** | 唯一有角色分离、幂等、两步确认、审计的运维面；扩展命令集到交易（arm/disarm、shadow/live、限额） |
| `crates/catalog` / `collection-policy` / `assets.toml` | **Keep** | 标的分类与原子激活直接适用于交易 universe |
| processor / parquet-store / s3-store / uploader | **Keep**（冷路径） | 与热路径解耦，v3 schema 修正已落地；仅需加交易事件数据集（order/fill/decision log） |
| telemetry（ReceiveClock/AlertStore/DiskPolicy） | **Borrow** | 语义正确但采样方式（两次 syscall）需换 TSC/`SO_TIMESTAMPING`；指标命名可沿用 |
| AGENTS.md / deploy 脚本 / systemd 沙箱 / release 流程 | **Keep** | 已用血泪校准；v2 交易服务必须同等级别沙箱 + 冷部署 + 服务用户 dry-run |
| `cdex-arb` 策略逻辑（价差定义、三通道、确认窗口、sizing 七约束、close_policy、五道门、watchlist 评分、成本线） | **Borrow（规格化）** | 这是老板两个月实盘校准出的规则，是 v2 的**需求文档**；以类型化配置 + 可回放决策日志重写 |
| `cdex-arb` 链上知识（合约地址、V4 pool key、Universal Router 命令序列、CLQuoter、Multicall3 组包、RPC 节点清单、gas 底价、nonce 歧义处理、`receipt.status`/`meta.err`） | **Borrow（移植到 Rust）** | 事实正确且实测过；实现（web3.py 同步、公共 RPC）必须重写 |
| `cdex-arb` 执行内核（Open/ClosePositionBot、`asyncio.run` 串行、JSON 文件 IPC、三参数源、6.2k 行 TG 单文件） | **Rewrite** | 结构性：Python GIL/线程、区块级轮询、串行执行、无交易状态机、状态在 CSV/JSON、无测试 |
| `cdex-arb/rust/cex-arb` | **Borrow 片段** | Ed25519 ws-fapi 客户端、KuCoin/Gate WS 下单、双腿 join+回滚模式可移植；手写 HMAC/base64、字节扫描应换标准 crate + SIMD 解析 |
| `portfolio-arb` | **Borrow 规格 + 少量代码** | 评分/闸门链/对账/SQLite schema 是好蓝本；并发模型与订单状态机需按 DEEP_DIVE 清单重做 |
| `oil-arb` | **Archive** | 研究脚本保留；实盘代码有已复现 P0，策略与主线（crypto 价差）不同 |
| `api_service` / `dashboard` / `monitor_lite` | **Rewrite（合并）** | 三套独立行情连接与 REST 轮询应统一为 v2 状态服务的一个只读视图 |

### 6.2 原型揭示的隐含需求

**交易所与工具**：Binance USDT-M 永续（数据 `fstream` + `/market/ws`；下单 ws-fapi Ed25519 `session.logon`；REST HMAC 查询；`income` 账本；`premiumIndex`/`openInterestHist`；一向持仓、2x 全仓、multi-asset off）、Binance 现货（汇率 USDC/USD1/FDUSD/BNB）、OKX（`books`、tokenized 股票 `X*-USDT`）、Bitget（30 s ping、10 msg/s）、Gate 现货 WS v4 下单 + Gate 永续（BZ/CL、XBR→BZ 迁移）、KuCoin 现货 WS `sync_order`；BSC（PancakeSwap V2/V3/V4 Infinity、Uniswap V3/V4、1inch、Multicall3、Universal Router/Permit2、`newHeads`）、Solana（Jupiter v1/v6、Jito/Helius 待接）。

**标的宇宙**：币安永续正费率长尾山寨（GUA/SIREN/SKYAI/PIPPIN/AKE/TA/BEAT/ARC/POWER/AIA/RAVE/AVAAI/XNY/M/XPIN/BLUAI/PIEVERSE/ARIA/UB/AIOT/TRADOOR/GENIUS/BAS/INX/LAB/LIGHT/PTB/B/H/Q/AIN/NAORIS/HANA/JCT/BTR/…）；tokenized 股票/商品/指数（AAPL、NVDA、SKHYNIX、SAMSUNG、ZHIPU、CRCL、MU、SNDK、XAU、CL、NFLXB/SQQQB/SPYB bStocks、Ondo/xStocks 别名）；符号跨所不一致（`PLAYUSDT` / `PLAY-USDT` / `PLAY_USDT`），同名假币极多 → **身份核验是一等需求**。

**限速/外部约束实测**：Binance depth 快照 429/418（≤4 并发）；Bitget 10 msg/s；BSC 公共 RPC 403（publicnode ≤12 call/multicall；ankr 需 key；48club/blockrazor 415）；Jupiter 免费档 429（>3 并发）；GeckoTerminal/DexScreener 限流（发现周期 ≥900 s）；BSC 0.45 s 出块、gas 0.05 gwei 底价常多等 1–3 块；Binance TLS 握手 ~30 ms（保持热连接）；服务器 minRTT Gate 0.27 ms、Binance 1.0–1.5 ms、OKX 1.4–1.8 ms。

**运维工作流（老板依赖）**：Telegram 是唯一操作界面（45 个命令 + 7 页手册 + 内联确认 + Bark critical）；测试/正式双账户切换与跨账户只读；每条消息账户标签；20 min 资产面板推送；开/平仓推送含两腿盈亏/资金费/手续费/gas（`trade_pnl`）；`/pnl` 结算报告；`/engine` 状态含「谁在卡上限」；影子模式 → `/engine on` 才实盘；`/pipeline live|dry|on|off`；`/hedgefix` 人工确认补腿；`/bridge /rebalance /binance_transfer` 资金搬运；watchlist 自动加币但「名单可宽、下单必须有依据」；成交 CSV 可审计；补丁后 `sync-code.sh` 同步两实例；采集与交易共机需资源隔离。

**策略/风控不变量**：先链上限价后 CEX；成交量只来自链上事实；两腿差 >5% 自动补腿、<$5 残余 reduce-only；账户级敞口 ≤ 钱包 × 1.5、单币 ≤25%；当日亏损 3% 熔断回影子；强平/ADL 硬动作；确认窗口 ≥2 区块且 ≥2 次链上报价；`|spread| > 30%` 脏数据；每币 halt 等人工；成本线按池费率；平仓线随资金费/持仓时长/分位动态；dry / shadow / live 三态；所有循环有界 + watchdog。
