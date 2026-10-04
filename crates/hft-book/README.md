---
description: "sequencer 拥有的每 inst 稠密数组 L2 簿与 BBO 缓存:应用已归一化的快照与增量、做通用序号校验与去重、维护最优档、导出关键帧;策略与冲击估算只读"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-book

## 概要

`hft-book` 拥有簿的数据结构与应用规则:`L2Book`(按 tick 索引的稠密数组,窗口 mid ± W,窗口外的有界溢出区,占用位图维护最优档)、`Top`(来自 `Bbo` 流的最优一档缓存)、`BookSet`(按 `InstId` 的稠密集合)、簿状态机 `Live/Stale/Resyncing` 与快照节 6、7 的行格式。L2 簿有两种来源:分档快照流(`origin = TopN`,整替换;首次上线需要深度时用它)与全量增量深度(可选扩展,序号链 + REST 重同步)。它不拥有交易所序号规则(由 venue 协议在 feed 线程归一化,见下文"决定")、不发起重同步(返回结果,由 [hft-sequencer](../hft-sequencer/README.md) 决定请求快照)、不计算 VWAP/冲击(由 [hft-strat-lib](../hft-strat-lib/README.md) 的 `ImpactModel` 用本 crate 的迭代器计算)。唯一写者是 sequencer 线程(tick-to-trade 的 t2 步);冷读者与回放用同一代码从 journal 重建簿。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 决定:簿只接受"已归一化"的增量,但自己做通用校验

**venue 协议负责一切交易所特有的序号语义**(Binance 合约 `pu == 上条 u`、现货 `U ≤ lastUpdateId+1 ≤ u` 的快照衔接、OKX `prevSeqId`、Bitget `pseq`(重置后为 0)、快照期间的增量缓冲与丢弃),并把结果归一化进 `L2Delta.prev_id/last_id`:`prev_id` = 本消息作用于其上的簿状态 id,快照后首条消息的 `prev_id` = 快照 `snap_id`([event-model](../../docs/subsystems/event-model.md#关键事件体))。**簿对每条消息只做一个通用比较** `prev_id == last_id`,外加去重与结构校验。理由:

1. 序号规则需要只有 feed 线程才有的上下文(REST 快照的 `lastUpdateId`、快照期间缓冲的增量、连接级状态),放进簿会让簿依赖每个 venue;
2. 通用比较只花一次整数比较,却能捕获 feed 归一化之外的错误——双路连接合流后的缺口、sequencer 批量/合并的 bug、回放输入损坏;
3. 同一簿代码因此可被 [hft-sim](../hft-sim/README.md)、回放与 PyO3 直接使用,不需要任何 venue 代码。

feed 线程检测到缺口时发 `ConnState::Resync`(簿 → `Resyncing`);簿自己检测到缺口时返回 `GAP` 并转 `Stale`,由 sequencer 请求该 inst 重同步。

### API

```rust
pub struct BookParams { pub tick: i64, pub window: u32 /* W,来自 InstHot.book_window */, pub ovf_cap: u16, pub crossed_tolerance: u16 }
impl BookParams { pub fn from_inst(h: &InstHot) -> BookParams; }   // ovf_cap 默认 64

pub struct L2Book { /* 见实现说明 */ }
impl L2Book {
    pub fn bytes_needed(p: &BookParams) -> usize;
    pub fn new_in(p: &BookParams, mem: &'static mut [u8]) -> Result<L2Book, BookErr>; // 调用方给内存(sequencer 用 hft-rt 的大页 Arena);mem 起点须 64 B 对齐
    pub fn new(p: &BookParams) -> L2Book;                                            // 堆分配;测试、回放、PyO3
    pub fn apply_snapshot(&mut self, h: &L2SnapHdr, levels: &[Level]) -> Outcome;    // bids 降序在前,asks 升序在后
    pub fn apply_batch(&mut self, h: &L2BatchHdr, levels: &[Level]) -> Outcome; // 一条交易所消息整体应用(feed 默认产出 L2Batch)
    pub fn apply_delta(&mut self, d: &L2Delta) -> Outcome;                         // 逐档记录的读者/工具用;同一套校验
    pub fn on_conn(&mut self, k: ConnStateKind) -> Outcome;   // Down:Live/Resyncing → Stale(ConnDown);Resync:Live/Stale → Resyncing;其他 kind 与 Uninit 簿不变
    pub fn mark_stale(&mut self, r: StaleReason);             // 例如 refdata 改了 tick;Uninit 以外 → Stale(r)(覆盖原因)
    pub fn state(&self) -> BookState;
    pub fn last_id(&self) -> u64;
    pub fn best_bid(&self) -> Option<Level>;
    pub fn best_ask(&self) -> Option<Level>;
    pub fn iter(&self, side: Side) -> LevelIter<'_>;          // 由内向外;窗口 → 溢出区;遇截断标记结束
    pub fn export(&self, out: &mut [Level]) -> (u16, u16, bool); // 关键帧:(n_bids, n_asks, truncated);买档在前(价降序)、卖档在后(价升序),含溢出区;
                                                              // out 不够时两侧各保留最内的 out.len()/2 档(远端截断)并返回 truncated = true
    pub fn meta(&self) -> BookMetaRow;                        // 节 6 的一行(`inst` 由 BookSet 填)
    pub fn state_hash(&self) -> u128;                         // xxh3-128(状态、last_id、全部非空档)
}

#[repr(u8)] pub enum BookErr     { MemTooSmall = 1 /* mem.len() < bytes_needed */, MemAlign = 2 /* 起点未 64 B 对齐 */,
                                   BadParams = 3 /* tick ≤ 0、window ∉ [64, 65536] 或 ovf_cap = 0 */,
                                   UnknownInst = 4 /* l2_insts 含 refdata 中不存在的 inst */, DupInst = 5 /* l2_insts 重复 */ }
#[repr(u8)] pub enum BookState   { Uninit = 0, Live = 1, Stale = 2, Resyncing = 3 }
#[repr(u8)] pub enum StaleReason { Gap = 1, Crossed = 2, ConnDown = 3, Split = 4, BadLevel = 5, Refdata = 6 }
#[repr(transparent)] pub struct Outcome(pub u16);
impl Outcome { APPLIED = 1; BID_TOP = 2; ASK_TOP = 4; DUPLICATE = 8; NOT_LIVE = 16; GAP = 32; CROSSED = 64;
               TRUNCATED = 128; RECENTERED = 256; INVALID = 512; WENT_STALE = 1024; WENT_LIVE = 2048; }
// BID_TOP/ASK_TOP = 本次调用使该侧最优档的价或量变化;WENT_STALE = 本次调用使簿离开 Live;WENT_LIVE = 由非 Live 进入 Live

#[repr(C, align(64))]
pub struct Top {                     // 64 B,一个缓存行;Bbo 流的最优一档
    pub bid_px: Px, pub bid_qty: Qty, pub ask_px: Px, pub ask_qty: Qty,
    pub update_id: u64, pub ex_ts_ns: i64, pub rx_tsc: Tsc, pub seq: u64,
}
impl Top { pub fn apply(&mut self, b: &Bbo, seq: Seq) -> TopOutcome; }
#[repr(transparent)] pub struct TopOutcome(pub u8); // BID_PX=1 ASK_PX=2 BID_QTY=4 ASK_QTY=8 DUPLICATE=16 OLDER=32 INVALID=64

pub struct BookSet { /* tops: [Top; n_inst];l2: 只为订阅了 L2 的 inst 分配;l2_slot: [u16; n_inst](0 = 无) */ }
impl BookSet {
    pub fn new_in(refdata: &RefData, l2_insts: &[InstId], mem: &'static mut [u8]) -> Result<BookSet, BookErr>;
    pub fn top(&self, i: InstId) -> &Top;          pub fn top_mut(&mut self, i: InstId) -> &mut Top;
    pub fn l2(&self, i: InstId) -> Option<&L2Book>; pub fn l2_mut(&mut self, i: InstId) -> Option<&mut L2Book>;
    /// 一次写完本 crate 的全部快照节:节 6 `BookMeta`(每个已分配的 L2 簿一行 `BookMetaRow`)与节 7 `Tops`
    /// (每个收到过 Bbo 的 inst 一行 `TopRow`)。引导块、关键帧、Drain、Recovery 与按需快照都调用它;不分片。
    pub fn snapshot(&self, w: &mut SnapshotWriter);
    /// 冷启动与冷读者自举:按行版本读回两节(未知行版本 → 错误);恢复后每个 `Top.rx_tsc` = 0。
    /// `SnapshotWriter`/`SnapshotReader`/`RestoreError` 是 SDK 的快照读写类型(引擎 L1 组件共用)。
    pub fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}
```

调用约束:

0. **整条消息原子应用**:`apply_batch` 先做序号与结构校验(任一档非法 → 整条不应用、转 `Stale`),再逐档写入,最后一次性做交叉检查与重新居中;sequencer 只在 `apply_batch` 返回后更新 mid 并调用策略——策略永远看不到半条消息之后的簿。

1. 只在 sequencer 线程调用可变方法;策略经 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 的只读视图(`Views::bbo`/`Views::book`)读取。
2. 同一 inst 的一条交易所消息拆成的 `L2Delta`(`idx = 0..N−1`,末条 `LAST`)必须连续到达该簿,中间不得插入同 inst 的其他 `L2Delta`;被打断视为 `Split` 并转 `Stale`。sequencer 不对 `L2Delta` 做合并(`CONFLATED` 只用于 `Bbo`)。
3. `origin = Keyframe` 的 `L2Snapshot` 是 sequencer 的输出,实盘与验证回放中**不**喂给簿;只有冷读者自举时用 `apply_snapshot` 应用它。
4. `Top` 与 `L2Book` 是两路独立数据,序号空间不同,本 crate 不做两者一致性检查([04 §4](../../design/04-行情接入层.md));只需要最优价的策略只读 `Top`。
5. `Top.rx_tsc` 是**最近一条被应用的 `Bbo`**(`TopOutcome` 不含 `DUPLICATE`/`OLDER`/`INVALID`)的 `MarketHdr.rx_tsc`,与 SDK `BboView.rx_tsc` 同义;只用于延迟测量与信号,**不是**报价可用性的依据:盘口长时间不变时 `Top` 仍是最新报价。报价是否可用按该 inst 的 BBO 成员连接健康判定(任一成员连接 Live、未在重同步、`conn_live_tsc` 在 `conn_liveness_ms` 内),由 sequencer 维护、经 SDK `BboView` 给出([hft-strategy-sdk](../hft-strategy-sdk/README.md)),本 crate 不持有连接状态([安静 ≠ 过期](../../docs/defensive-patterns.md#安静--过期报价可用性看连接健康))。
6. **分档快照簿**(`origin = TopN` 的 `L2Snapshot`,路由 `RouteKind::DepthTopN`,[hft-venue](../hft-venue/README.md)):每条按**整替换**经 `apply_snapshot` 应用(`Live` 状态下同样应用),没有序号链,不做 `prev_id` 校验、不产生 `GAP`、不发起 REST 重同步;`snap_id` 非 0 且 ≤ 当前 `last_id` 的快照按重复丢弃(`DUPLICATE`)。最近一次快照的 `origin = TopN` 时簿不接受 `L2Batch`/`L2Delta`(→ `INVALID`,不转状态)。`ConnState` Down 照常转 `Stale`,下一条 TopN 快照直接回到 `Live`。是否订阅分档快照或全量增量深度(可选扩展)由策略订阅与 refdata 决定,簿只按 `origin` 区分。

#### 快照行(草案,实现时以代码与编译期断言为准)

```rust
/// 节 6 `BookMeta`,行版本 1,32 B,键 inst。L2 档位不在这里,由紧随其后的 `L2Snapshot{origin = Keyframe}` 承载。
#[repr(C)] pub struct BookMetaRow {
    pub inst: InstId,           //  0 u32
    pub state: u8,              //  4 BookState
    pub stale_reason: u8,       //  5 StaleReason;0 = 无
    pub origin: u8,             //  6 最近一次快照的 `L2Snapshot.origin`(码值归 hft-event);0 = 尚无快照
    pub _pad0: u8,              //  7
    pub last_id: u64,           //  8
    pub base: i64,              // 16 窗口首档的 tick 下标
    pub truncated: u32,         // 24 溢出区截断累计次数
    pub crossed_run: u16,       // 28
    pub _pad1: [u8; 2],         // 30
}
/// 节 7 `Tops`,行版本 1,64 B,键 inst。与 `Top` 相同,只是不含 `rx_tsc`(TSC 不进快照,[hot-path-rules D6]):
/// 节 7 因此没有 TSC 字段,不在 D6 的 allow-list 里;恢复后 `Top.rx_tsc` 为 0,直到本次运行收到该 inst 的第一条被应用的 Bbo。
#[repr(C)] pub struct TopRow {
    pub inst: InstId, pub _pad: [u8; 4],                       //  0
    pub bid_px: Px, pub bid_qty: Qty, pub ask_px: Px, pub ask_qty: Qty, //  8
    pub update_id: u64, pub ex_ts_ns: i64, pub seq: u64,       // 40
}
const _: () = assert!(core::mem::size_of::<BookMetaRow>() == 32 && core::mem::size_of::<TopRow>() == 64);
```

行版本只在本节登记([journal-format 快照记录](../../docs/subsystems/journal-format.md));行格式只追加、零值表示未提供。

### 状态机

```
        apply_snapshot(Exchange | TopN)
 Uninit ──────────────────────────────▶ Live ◀────────────────────────────┐
                                         │ GAP / Split / BadLevel /        │ apply_snapshot
                                         │ Crossed 超容忍 / ConnDown        │
                                         ▼                                 │
                                       Stale ──ConnState::Resync──▶ Resyncing
 在 Stale/Resyncing:增量一律丢弃(NOT_LIVE,计数);Live 的 ConnState::Resync 直接进入 Resyncing
 分档快照簿(origin = TopN):Stale 时下一条 TopN 快照直接回到 Live,不经 Resyncing
```

`Stale` 与 `Resyncing` 期间策略不对该 inst 开仓,`ImpactModel` 返回"不可用"([04 §4](../../design/04-行情接入层.md))。

## 实现说明

<details><summary>实现要点</summary>

**布局**(每个 `L2Book`,W = `window`,L = 2W 向上取整到 64 的倍数):

```
qty:   [[i64; L]; 2]        // 下标 0 = 买,1 = 卖;qty[s][i] 是价格 (base + i) × tick 的量,0 = 空档
occ:   [[u64; L/64]; 2]     // 占用位图,与 qty 同步
ovf:   [OvfSide; 2]         // 窗口外档位:按价排序的定长数组(容量 ovf_cap)+ far_qty 聚合 + truncated 标志
hot:   { base: i64 /* 窗口首档的 tick 下标 */, best: [i32; 2] /* 窗口内下标,-1 = 窗口内无 */,
         last_id: u64, msg_last_id: u64, next_idx: u16, flags: u8 /* IN_MSG|SKIP_MSG */, state: BookState,
         crossed_run: u16, stale_reason: u8 }        // 热字段放同一缓存行
```

W = 2048 时每簿 ≈ 64 KiB 数量数组 + 1 KiB 位图 + 4 KiB 溢出区;400 个 L2 簿 ≈ 28 MiB,放在 sequencer 的大页区。

**`apply_batch` 步骤**:与下述 `apply_delta` 的第 2–5 步相同,只是第 2 步(序号)对整条消息做一次(`prev_id == last_id`,`last_id ≤ self.last_id` 为重复),第 3 步先对全部档做完结构校验再写第 4 步,第 5 步在末尾做一次。批量写入时最优档只在批末从位图重算一次。

**`apply_delta` 步骤**:

1. `state != Live` → `NOT_LIVE`。
2. 消息边界:`idx == 0` 且仍在上一条消息中(`IN_MSG`,上一条未见 `LAST`)→ 转 `Stale(Split)`;否则清 `SKIP_MSG`,若 `last_id ≤ self.last_id` → 整条消息是重复(双路合流的后到者),置 `SKIP_MSG`,返回 `DUPLICATE`(带 `LAST` 的档同时清 `SKIP_MSG`);否则若 `prev_id != self.last_id` → `GAP`,转 `Stale(Gap)`;否则开始新消息(`msg_last_id = last_id`,`next_idx = 0`)。`idx > 0` 时,`SKIP_MSG` → `DUPLICATE`;不在消息中、`idx != next_idx` 或 `last_id != msg_last_id` → 转 `Stale(Split)`。
3. 结构校验:`px > 0`、`qty ≥ 0`、`px % tick == 0`,否则 `INVALID`,转 `Stale(BadLevel)`。
4. `i = px / tick − base`。`0 ≤ i < L`:写 `qty[s][i]`,同步位图;最优档维护——买侧:`qty > 0 && i > best` → `best = i`;`i == best && qty == 0` → 用位图从 `i` 向下找最高置位(每步 64 档,最多 L/64 步),窗口内无则 `best = -1`;`i == best` 且量变 → 置 `BID_TOP`。卖侧对称。窗口外:在 `ovf[s]` 中二分插入/更新/删除(删除不存在的档是空操作);容量满时把新档与已有档中离中价最远的一档并入 `far_qty`(不再可见、不可删除),`truncated` 计数加 1 并置 `TRUNCATED`。
5. `LAST`:`self.last_id = msg_last_id`,清消息标志;交叉检查:两侧最优都存在且买 ≥ 卖 → `crossed_run += 1`,超过 `crossed_tolerance` 转 `Stale(Crossed)`,否则返回 `CROSSED`;不交叉则 `crossed_run = 0`。再做重新居中检查。

**最优档不在窗口内**:`best_*()` 返回该侧全部可见档(窗口与溢出区)中价格最优的一档;最优档只在溢出区时在 `LAST` 时触发重新居中。

**状态迁移的出口**:`GAP` → `Stale(Gap)`,返回 `GAP | WENT_STALE`;`Split` 与 `BadLevel` → 返回 `INVALID | WENT_STALE`;交叉超过容忍 → 返回 `APPLIED | CROSSED | WENT_STALE`(整条已写入);转 `Stale` 之后同一消息剩余的档按第 1 步返回 `NOT_LIVE`。`L2Batch` 的档数与 `n_bids + n_asks` 不符视同 `BadLevel`。检查顺序:最近快照 `origin = TopN` → `INVALID`(不转状态)→ 第 1 步 → 第 2 步 → 第 3 步。

**重新居中**(`LAST` 之后,当中价下标落在 `[W/2, 3W/2)` 之外,或任一侧最优只在溢出区):

1. 中价 tick 下标 `m`(一侧为空时用另一侧最优);新 `base' = m − W`,位移 `d = base' − base`。
2. `|d| ≥ L`:把窗口与溢出区的全部非空档收集到预分配的暂存区(容量 `2 × (L + ovf_cap)`),清空,按新 `base'` 重新插入。
3. 否则 `copy_within` 平移两侧数量数组 `|d|` 档;移出窗口的非空档进入溢出区;溢出区中落入新窗口的档移入;重算受影响的位图字与 `best`。
4. 置 `RECENTERED`。代价 O(L),罕见。

**`apply_snapshot`**(任一状态都可调用):先对全部档做第 3 步的结构校验(`qty = 0` 的档跳过;同价重复时后者生效)与交叉检查(两侧最优都存在且买 ≥ 卖),不通过 → 簿内容与 `last_id` 不变,转 `Stale(BadLevel)` 或 `Stale(Crossed)`,返回 `INVALID`(由 Live 离开时另置 `WENT_STALE`);通过 → 按位图只清非空档(不整块 memset),以快照中价设 `base`(中价下标 `m = ⌊(最优买下标 + 最优卖下标)/2⌋`,一侧为空用另一侧最优,两侧都空则 `base` 不变;`base = m − W`),逐档插入,`last_id = snap_id`,记下 `origin`,转 `Live`(由非 Live 进入时置 `WENT_LIVE`),返回 `APPLIED`。`origin = TopN` 时先做重复检查(调用约束第 6 条),中价下标仍在 `[base + W/2, base + 3W/2)` 内时不重新设 `base`(只清旧档、写新档),每条的代价与档数成正比。`origin = Keyframe`(只用于冷读者自举与恢复):沿用当前 `base` 写入档位与 `last_id`,不改 `state`、`stale_reason`、`origin`、`crossed_run` 与 `truncated`(这些由 `restore` 读回);簿为 `Uninit` 时按 Exchange 规则处理。

**`BookSet::snapshot`**:节 6 按 `inst` 升序每个已分配的 L2 簿写一行 `BookMetaRow`;节 7 按 `inst` 升序对 `Top.seq ≠ 0` 的 inst 写一行 `TopRow`(不写 `rx_tsc`)。`restore` 逆向填回,`Top.rx_tsc` 置 0;本 `BookSet` 没有分配 L2 簿的 inst 的节 6 行与超出 refdata 范围的节 7 行跳过;行内 `state`、`stale_reason`、`origin` 不是合法码值 → `Err`;快照之后的 L2 档位由 `L2Snapshot{Keyframe}`(冷读者与恢复)或重新订阅(实盘)补齐。

**`Top::apply`**:`update_id != 0 && update_id ≤ self.update_id` → `DUPLICATE`(相等)或 `OLDER`;两侧价都 > 0 且买 ≥ 卖 → `INVALID`;否则写入并按字段变化置位。约 98% 的 BBO 更新只改量不改价([bbo 实测](../../design/research/06-data-plane-replay-observability.md)),策略可凭 `BID_PX|ASK_PX` 位跳过纯量更新。

**规划源文件**

| 文件 | 职责 |
|---|---|
| `src/l2.rs` | `L2Book` 布局、`apply_delta`、`apply_snapshot` |
| `src/bitmap.rs` | 占用位图与最高/最低置位查找 |
| `src/overflow.rs` | `OvfSide` 有序定长数组、截断聚合 |
| `src/recenter.rs` | 重新居中 |
| `src/top.rs` | `Top`、`TopOutcome` |
| `src/set.rs` | `BookSet`、内存切分 |
| `src/snapshot.rs` | `BookMetaRow`、`TopRow`、`BookSet::snapshot`/`restore` |
| `src/export.rs` | `export`、`iter`、`meta`、`state_hash` |

</details>

## 延迟与资源契约

| 入口 | 预算(参考机,aarch64 Graviton,簿在 L2/L3 缓存;分配,M1.5 起在参考机实测复核,[latency-budget §6](../../docs/latency-budget.md#6-验收方法)) | 基准 |
|---|---|---|
| `apply_delta`,窗口内、最优不变 | p50 ≤ 15 ns,p99 ≤ 40 ns | `book_delta_inwin` |
| `apply_batch`(20 档的典型 depth 消息) | p50 ≤ 250 ns,p99 ≤ 600 ns | `book_batch_20` |
| `apply_delta`,删除最优档需位图重找 | p99 ≤ 100 ns([04 §1](../../design/04-行情接入层.md):≤ 100 ns/档) | `book_delta_best_remove` |
| `apply_delta`,窗口外(溢出区,cap 64) | p99 ≤ 150 ns | `book_delta_ovf` |
| `Top::apply` | p50 ≤ 8 ns | `book_top_apply` |
| `best_bid`/`best_ask` | ≤ 3 ns | `book_best` |
| `iter` 前 20 档 | ≤ 100 ns | `book_iter_20` |
| `apply_snapshot`(每侧 1 000 档) | p99 ≤ 30 µs | `book_snapshot_1000` |
| `apply_snapshot`(`origin = TopN`,每侧 20 档整替换) | p50 ≤ 1 µs,p99 ≤ 3 µs | `book_snapshot_topn20` |
| `BookSet::snapshot`(节 6、7;400 个 L2 簿 + 2 000 个 `Top`) | p99 ≤ 40 µs;计入关键帧一次写完的 ≤ 300 µs([latency-budget §3.1](../../docs/latency-budget.md#31-p999-记录项)) | `book_snapshot_sections` |
| 重新居中(W = 2048) | p99 ≤ 10 µs,罕见 | `book_recenter_2048` |
| `export`(每侧 200 档) | ≤ 3 µs | `book_export_200` |
| 回放吞吐(真实 Binance 合约 depth 1 h) | 单核 ≥ 20 M 增量/s | `book_replay_binance_1h` |

零分配(`new_in` 之后)、零系统调用;所有数组启动期预分配;热字段与数量数组分离。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:[hft-types](../hft-types/README.md)、[hft-refdata](../hft-refdata/README.md)(`InstHot` → `BookParams`)、[hft-event](../hft-event/README.md)(`L2Batch` 前缀 `L2BatchHdr`、`L2Delta`、`L2SnapHdr`(含 `origin`)、`Level`、`Bbo`、`ConnStateKind`)、[hft-strategy-sdk](../hft-strategy-sdk/README.md)(只用快照读写类型 `SnapshotWriter`/`SnapshotReader`/`RestoreError`)。
- 外部:`xxhash-rust`(xxh3,`state_hash`);dev:`proptest`、`criterion`。
- 禁止:`BTreeMap`/`HashMap` 于簿本体(只在测试参考模型中)、任何 venue crate、`hft-rt`(L1 不依赖 L3;大页内存由调用方以 `&mut [u8]` 传入)。`unsafe` 只用于索引热循环的越界检查消除,每处 `// SAFETY:` 写明 `0 ≤ i < L` 的来源。

## 延伸阅读

- [04 §4 簿](../../design/04-行情接入层.md)、[04 §6 断裂与恢复](../../design/04-行情接入层.md)、[research/04 §2.9 订单簿实现对比](../../design/research/04-oss-frameworks-and-patterns.md)、[research/06 §4.6 快照与增量拼接陷阱](../../design/research/06-data-plane-replay-observability.md)
- [event-model `L2Delta` 与序号归一化](../../docs/subsystems/event-model.md#关键事件体)、[journal-format 引导块与关键帧](../../docs/subsystems/journal-format.md#段滚动与引导块)
- 使用方:[hft-sequencer](../hft-sequencer/README.md)、hft-sequencer 为策略实现的 [hft-strategy-sdk](../hft-strategy-sdk/README.md) `BookRead` 视图(`hft-strat-lib` 的 `ImpactModel` 经该视图读簿,不直接依赖本 crate)、[hft-venue](../hft-venue/README.md)(序号归一化的实现者)

## 已知限制与后续工作

- 不做 L3(逐单)簿;挂单类策略需要队列位置时只对少数合约开,另建 crate 或扩展本 crate([04 §4](../../design/04-行情接入层.md))。
- 固定 tick:价格相关 tick(Hyperliquid 有效数字规则)需要按"最细可能 tick"建窗口。可选扩展(触发条件:接入 Hyperliquid)。
- 溢出区容量有限,极深档位会聚合为 `far_qty`;对大额 VWAP 的影响由 `TRUNCATED` 标志告知调用方。
- Binance 合约 RPI 订单不出现在 depth,"看得见的最优价未必可成交"([04 §5](../../design/04-行情接入层.md))——这是数据源限制,簿不补偿。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:各 venue 的簿是否会合法地短暂交叉(方法:对现有 24 h Binance/OKX/Bitget 捕获离线统计交叉持续的消息数),据此给 `crossed_tolerance_default` 取值(当前默认 0,即一旦交叉立即重同步);Bybit `orderbook.N` 的 `u` 是否逐条 +1(决定其 venue 协议如何构造 `prev_id`)。
- **待核实**:各所分档快照流的 `snap_id` 来源与单调性(Binance `depth{5,10,20}` 的 `lastUpdateId`、OKX `books5`、Bitget `books5/books15` 各自的序号字段;方法:各所文档 + 金样中连续 1 h 的序号检查),决定 TopN 的重复判定是否可用;不单调的所由 venue 协议把 `snap_id` 置 0(不去重)。
- 实现顺序:`bitmap.rs` → `l2.rs`(窗口内)+ 参考模型测试 → `overflow.rs` → `recenter.rs` → `top.rs` → `export.rs` → `set.rs` → 基准。
- `window` 的默认值由流水线按"覆盖 ±0.5% 价格、夹在 [256, 8192]"给出,写进 refdata(BTC tick 0.1、价 1e5 时约 5000 档)。

</details>
