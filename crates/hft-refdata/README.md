---
description: "合约、资产指数、交易所能力表(VenueCaps)、账户(实盘/模拟、交易所 UID、绑定的策略实例)、策略实例表、费率、限速、端点、维护窗口与符号映射的唯一运行期来源;hft-cfg 从 TOML 生成 blob,hft-core 启动时加载校验(变更 = 计划内重启),feed/gw/sequencer/策略视图只读查表"
kind: crate
tier: pure
scope: core
status: spec
---

# hft-refdata

## 概要

`hft-refdata` 拥有参考数据的**格式、加载校验与查询**:`RefData`(不可变快照)、`InstMeta`/`InstHot`、`AssetMeta`、`VenueMeta`、**`VenueCaps`**(每个 `VenueId` 一行的交易所能力表;`VenueId` 按产品线划分,市场段由 `VenueMeta.product` 推出)、`AccountMeta`(含账户类型 `Live`/`Sim`、交易所 UID、绑定的策略实例)、策略实例表、`FeeSchedule`、`RateLimitTable`、端点、维护窗口、链上代币映射(optional)、符号→`InstId` 查表。本 crate 是下列事实的唯一归属:账户列表、标签、类型与绑定(`AccountMeta`;**一个账户只绑定一个策略实例**)、交易所差异(`VenueCaps`:手续费扣币规则、reduceOnly、要求的账户配置、资金费推送方式、保证金数据来源、自身强平识别方法、可忽略的用户流事件、能力位、死手、测试网)、交易所限速(`RateLimitTable`;余量比例在 `RiskParams`)、连接最长存活(`VenueMeta.ws_max_conn_age_s`)。group 成员、`open_allowed` 与阈值在 `StrategyParams`([hft-event SPB1](../hft-event/README.md#strategyparamsspb1));OMS/ledger/容量/环容量在 **`EngineConfig`**(另一个 blob,复用本 crate 的段式容器);`risk_class` 在 `RiskParams`。它不拥有取值:**唯一来源是 [`tools/hft-cfg`](../../tools/README.md#hft-cfg)**(从受版本控制的 TOML 编码;RDB1 只有本 crate 的 `RdbBuilder` 一个编码器);不拥有替换命令的语义与权限([control-plane](../../docs/subsystems/control-plane.md));不拥有跨线程发布机制。调用者:feed 线程(符号→`InstId`、线格式指数)、gw 线程(tick/lot、客户端 id 限制、能力位)、sim-gw(模拟账户配置、能力表)、sequencer(风控、OMS、执行层、策略视图的全部静态参数)、冷进程与 PyO3(解析 journal 里的 `ConfigBlob`)。热路径上只有查表,没有加载。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 生命周期(变更 = 重启)

```
hft-cfg(人工/定时)─ TOML(params/refdata/)→ RDB1 blob ─▶ 文件 + ctl `ReplaceRefdata`
     inst 部分可由 `hft-cfg refdata fetch` 拉各 core venue 的 instruments 接口生成待审 TOML diff;blob 只由 `hft-cfg build --kind refdata` 产生
启动:apps/hft-core 读 blob → RefData::load(blob, 上一版本(来自 journal))+ EngineConfig::load + 跨 blob 校验 + 与编译进二进制的
      VenueProtocol / StrategySet 交叉校验(启动线程,毫秒级)→ 以 ConfigBlob{boot} 写进 journal → 装配时各线程拿到 &'static RefData
运行期 ReplaceRefdata:sequencer 追加 ConfigBlob(内容)→ ControlCmd → CmdResult{Applied: 已登记,待重启}→ 请求 Drain → 重启后按上一行加载
回放/冷读者:从 journal 的 ConfigBlob 字节调用同一个 RefData::load —— 纯函数,结果逐字节相同
```

- **交易宇宙**:blob 包含近期可能交易的全部 inst(core venue 上满足门槛的现货与 USDT 永续,再加当前持仓涉及的 inst),而不是当前名单。名单增删、group 成员、`open_allowed` 与阈值都在 `StrategyParams`(按 (strategy, account) 一个 blob,内含全部 group,可热替换)。因此名单变化不需要改 refdata;只有宇宙本身(新 inst、费率档、限速、能力表、账户与实例)变化才需要。
- **core venue**:Binance(`BINANCE_USDM`、`BINANCE_SPOT`)、OKX(`OKX_SWAP`、`OKX_SPOT`)、Bitget(`BITGET_UTA_USDT_FUT`、`BITGET_UTA_SPOT`),各接现货 + USDT 本位永续;其余 `VenueId`(Gate、Bybit、KuCoin、Hyperliquid、链上)是 optional,refdata 可以包含它们,但只有对应 `venue-*` 编译进二进制时才能加载(否则 `UnknownVenue`)。
- 冷启动预算 < 30 s([08 §6](../../design/08-数据面与回放.md));refdata 变更随之有一次计划内重启(Drain → 撤单 → 快照 → 重启 → 对账 → 启动闸门放行)。
- **资金费周期**:refdata 只给默认值(`InstMeta.funding_interval_s`、`funding_anchor_s`);**运行期实际结算周期以 `Funding` 事件为准**([hft-event `Funding.interval_s`](../hft-event/README.md#行情0x01xx)),临时变化可经 `VenueNotice{FundingInterval}` 命令登记。
- **可选扩展:热替换**(触发条件:计划内重启的频率或时长成为运营负担):`load` 移到 ctl-gateway,sequencer 交换指针并经 [hft-rt](../hft-rt/README.md) `EpochSwap` 发布给 feed/gw;届时须列出允许的变更集(新增 inst、状态/费率/限速变化;不允许改指数、账户、实例)。下文第 10 步的跨版本不变量已按这一目标设计。

### 公开 API

```rust
pub struct RefData { /* 不可变;全部表启动/加载期一次性分配 */ }

impl RefData {
    pub fn load(blob: &[u8], current: Option<&RefData>) -> Result<RefData, LoadReport>; // 冷;纯函数、确定性
    pub fn version(&self) -> u64;
    pub fn sha256(&self) -> &[u8; 32];                      // blob 字节的 SHA-256,= ConfigBlob.sha256
    // —— 热查询(无分配、无分支外跳;越界 id 返回下标 0 的 NONE 行,status = Invalid)——
    pub fn inst(&self, id: InstId) -> &InstHot;             // 64 B 一行,一个缓存行
    pub fn symbols(&self, v: VenueId) -> &SymbolIndex;      // feed 线程启动时取一次并缓存
    pub fn fees(&self, a: AccountId, id: InstId) -> FeePair;   // 本账户 (venue, inst) 的 FeeOverrides 行 > 本账户 (venue, 全部 inst) 的 FeeOverrides 行 > Fees 中 (venue, 档) 的行;档 = Live 0、Sim 为其 fee_tier
    pub fn caps(&self, v: VenueId) -> &VenueCaps;           // 数组下标;未登记的 venue → NONE 行(全部能力位为 0)
    pub fn account(&self, a: AccountId) -> &AccountMeta;    // 热路径用到的字段(kind、bound_instance)在行首;0 或越界 → NONE 行(bound_instance = 0、venues = 0、is_prod = false)
    pub fn warnings(&self) -> &[LoadErr];                    // 加载成功时的非致命发现(只有两个 is_prod = false 账户在同一交易所不同 VenueId 上的 DuplicateVenueUid,见第 9 步);hft-core 启动时每条写一条 Note{RefdataWarning}(P1,码登记在 hft-event)
    // —— 冷查询 ——
    pub fn inst_meta(&self, id: InstId) -> &InstMeta;
    pub fn asset(&self, id: AssetId) -> &AssetMeta;
    pub fn venue(&self, v: VenueId) -> &VenueMeta;
    pub fn venue_uid(&self, a: AccountId, v: VenueId) -> Option<&str>;   // Live 账户在该 venue 的交易所 UID;Sim 或空串(只在 Unverifiable 账户)→ None
    pub fn instance(&self, s: StrategyInstanceId) -> Option<&InstanceMeta>;
    pub fn account_of(&self, s: StrategyInstanceId) -> Option<AccountId>; // 绑定到该实例的账户(一个实例至多一个账户,加载校验 InstanceMultiBound);未绑定 → None
    pub fn insts_of_asset(&self, a: AssetId) -> &[InstId];   // 按 InstId 升序
    pub fn insts_of_venue(&self, v: VenueId) -> &[InstId];   // 按 InstId 升序
    pub fn rate_limits(&self, v: VenueId) -> &[RateLimit];
    pub fn endpoints(&self, v: VenueId, env: EndpointEnv) -> &[Endpoint];
    pub fn maintenance(&self, id: InstId, utc_ns: i64) -> Option<&MaintWindow>; // utc_ns 来自事件,不读系统时钟;窗口区间 [start_ns, end_ns);该 inst 的窗口优先于全 venue 窗口,同级取 start_ns 最小者
    pub fn token(&self, chain: VenueId, a: AssetId) -> Option<&ChainToken>;  // optional 链上
    pub fn venue_account_ix(&self, v: VenueId, a: AccountId) -> Option<u8>; // 该 venue 上启用的 Live 账户按 AccountId 升序的稠密序号 0..11(kill 字位、OMS 表下标);Sim 或未启用 → None
    // —— 参数校验(SetParams 时由 ctl-gateway 与 sequencer 调用;冷路径)——
    pub fn check_binding(&self, a: AccountId, s: StrategyInstanceId, kind_id: u16) -> Result<(), LoadErr>; // AccountBound / KindMismatch
    pub fn check_group(&self, a: AccountId, g: &GroupSpecView<'_>) -> Result<AssetId, LoadErr>; // 跨 blob 第 2 条:按所列顺序逐腿检查,再查两腿之间;返回第一个错误;通过时返回报价资产
    // —— 跨 blob 与跨组件复核(冷路径;规则见实现说明"跨 blob 与跨组件校验")——
    pub fn check_rebind(&self, prev: &RefData, busy: impl Fn(AccountId) -> bool) -> Result<(), LoadReport>; // bound_instance 相对 prev 变化且 busy(a)(有仓位、挂单或 Unknown,调用方按快照判定)→ BindingChangedWithPosition,row = 账户
    pub fn check_ref_inst(&self, id: InstId) -> Result<(), LoadErr>;   // RefInstNotSubscribed
    // —— 装配与工具辅助(冷)——
    pub fn inst_count(&self) -> usize; pub fn venues_in_order(&self) -> &[VenueId]; pub fn instances_in_order(&self) -> &[InstanceMeta];
    pub fn caps_row(&self, v: VenueId) -> Option<u32>; pub fn venue_row(&self, v: VenueId) -> Option<u32>; // 段内行下标(错误定位)
    pub fn account_venues(&self) -> &[(AccountId, VenueId, Box<str>)]; pub fn blob(&self) -> &[u8]; pub fn created_utc_ns(&self) -> i64;
}
pub struct GroupSpecView<'a> { pub asset: AssetId, pub legs: &'a [LegView] }   // hft-event GroupSpec 的同形视图(hft-event 的 GroupSpec::view 在调用方存储里构造)
pub struct LegView { pub inst: InstId, pub venue: VenueId }
pub struct ProtocolFacts { pub venue: VenueId, pub market: MarketKind, pub caps: VenueCaps /* VenueProtocol::default_caps() */, pub stale_guard: StaleGuardKind }
pub struct AssemblyFacts<'a> { pub host: HostClass, pub max_universe_insts: u32, pub protocols: &'a [ProtocolFacts], pub kinds: &'a [u16] /* StrategySet::kinds() */ }
pub fn check_assembly(rd: &RefData, ec: &EngineConfig, f: &AssemblyFacts<'_>) -> Result<(), LoadReport>; // hft-core 启动与 ReplaceRefdata 装配校验调用;纯函数
pub struct RdbBuilder { /* 唯一的 RDB1 编码器;tools/hft-cfg 与测试 fixture 都用它 */ }
impl RdbBuilder { pub fn new(version: u64) -> Self; pub fn asset(&mut self, a: AssetSpec) -> AssetId; /* … venue/caps/inst/instance/account/account_venue/sim/sim_balance/fee/fee_override/limit/endpoint/maint/token/alias/mmr_tier … */
                  pub fn finish(self) -> Vec<u8>; pub fn finish_raw(self) -> RawBlob; } // finish_raw:可编辑容器(工具与测试钩子,blob::RawBlob)
pub struct EcfBuilder { /* 唯一的 ECF1 编码器 */ }
impl EcfBuilder { pub fn new(version: u64, host: HostClass) -> Self; pub fn section(&mut self, s: EngineSection, row_ver: u16, row_size: u32, count: u32, bytes: Vec<u8>) -> &mut Self; pub fn finish(self) -> Vec<u8>; }
#[cfg(feature = "tools")] pub fn dump(rd: &RefData) -> String;                 // 可读 TOML(枚举小写)
#[cfg(feature = "tools")] pub fn diff(a: &RefData, b: &RefData) -> Vec<DiffItem>; // 叶子级差异 (path, old, new),按 path 排序

pub struct EngineConfig { /* ECF1;不可变 */ }
impl EngineConfig {
    pub fn load(blob: &[u8], current: Option<&EngineConfig>) -> Result<EngineConfig, LoadReport>; // 容器级(冷读者只用它)
    pub fn validate(&self, rd: &RefData, validators: &[EngineValidator]) -> Result<(), LoadReport>; // 段语义:按段 id 升序调用各段的验证器
    pub fn section(&self, s: EngineSection) -> Option<SectionView<'_>>;  // 行格式由归属 crate 解析
    pub fn host_class(&self) -> HostClass;                               // hft_types::HostClass:dev2 | c4 | c8 | c16 | c32 | metal(编码见下文 ECF1;各类的取值与选定归 apps/hft-core)
}
pub struct SectionView<'a> { pub row_ver: u16, pub row_size: u32, pub count: u32, pub bytes: &'a [u8] }
#[repr(u16)] pub enum EngineSection { Oms = 1, Ledger = 2, Capacities = 3, Rings = 4, Sequencer = 5, Conn = 6, Exec = 7, Assembly = 8 }   // = ECF1 段 id(下文 ECF1 表,只追加)
pub struct EngineValidator { pub section: EngineSection, pub check: fn(&EngineConfig, &RefData) -> Result<(), LoadErr> }   // 归属 crate 提供;冷路径

pub struct LoadErr { pub code: LoadErrCode, pub section: u16 /* 段 id;无 = 0 */, pub row: u32 /* 段内行下标;无 = 0 */ }
pub struct LoadReport { pub errors: Vec<LoadErr> }   // 按校验顺序收集错误(顺序与停止规则见"加载校验";hft-core 一次打印全部)
impl LoadReport { pub fn code(&self) -> LoadErrCode; }  // 第一条错误的码:线上 arg 只带这一个

pub struct SymbolIndex { /* 每 venue 一张表 */ }
impl SymbolIndex {
    pub fn get(&self, wire_symbol: &[u8]) -> Option<InstId>;  // 交易所报文里的原样字节(大小写敏感)
    pub fn get_num(&self, venue_num_id: u64) -> Option<InstId>; // OKX instIdCode、SBE symbolId 等数值 id
    pub fn prewarm(&self) -> u64;                              // 顺序读全部条目,返回校验和防优化;在使用线程上调用
    pub fn from_entries(syms: &[(&[u8], InstId)], nums: &[(u64, InstId)]) -> SymbolIndex; // 加载器与测试用
    pub fn get_sorted(&self, s: &[u8]) -> Option<InstId>; pub fn get_hashed(&self, s: &[u8]) -> Option<InstId>; // 两条路径(get = get_hashed)
}
```

`GroupSpec` 的布局在 [hft-event](../hft-event/README.md#strategyparamsspb1);本 crate 只提供针对它的 refdata 一致性检查。hft-event 依赖本 crate,所以 `check_group` 接受本 crate 定义的同形视图 `GroupSpecView`(前 `n_legs` 条腿的 `inst`、`venue` 与 `asset`),由 hft-event 提供转换 `GroupSpec::view`(腿视图写在调用方给的存储里)。

### LoadErrCode 码表

`RefData::load`、`EngineConfig::load`、`check_binding`、`check_group` 与 hft-core 启动时的协议复核返回的错误码。**本节是数值的唯一登记处**:`#[repr(u16)]`,0 不用,只追加、永不复用;`gen-schema` 导出(`python/_generated/refdata.py` 与 `docs/generated/event-catalog.md`),`--check` 拒绝编号消失或复用。线上使用处:[control-plane](../../docs/subsystems/control-plane.md#结果码) detail 124 `GroupCheck` 与 401 `RefdataRecheck` 的 arg、`Note{RefdataRecheckFailed}` stage 2 的 `v[1]`([hft-sequencer](../hft-sequencer/README.md))。命令侧另有独立 detail 码的(`AccountBound`、`KindMismatch`、`SameExchangeLegs`)由映射方转成那些码,不放进 arg。

```rust
#[repr(u16)] pub enum LoadErrCode {
    // 加载校验第 1–4 步
    Structure = 1,               // magic/format、total_len、base_version ≠ 0、version = 0、段越界或重叠、段 id 重复、row_size 小于已知大小、对齐;无 OPTIONAL 位的未知段;退役段(不论 OPTIONAL);
                                 // 行内枚举或 Option 线编码的判别值非法(含 ECF1 host_class)
    Strings = 2,                 // StrRef 越界、非 UTF-8、符号或别名的长度或字符非法、实例名或账户标签重复
    Ids = 3,                     // id 不从 1 起稠密、重复或未排序
    UnknownVenue = 4,            // VenueId 不在 hft-types 常量表;hft-core 启动时:有 inst 的 venue 没有编译进来的 VenueProtocol
    InstanceIdRange = 5,
    DanglingRef = 6,             // 引用的 venue/inst/asset/账户/实例不存在,或代币行的 chain 不是链上 venue
    QuoteNotStable = 7,
    InstanceMultiBound = 8,
    // 第 5 步 能力表
    MissingCaps = 9, CapsMarketMismatch = 10, SpotBorrowRequired = 11, DeadManRange = 12,
    CapsValue = 13,              // 其余能力表取值非法(如 Poll 的 interval_ms 越界)
    // 第 6–8 步 指数、步长与范围
    ExpRange = 14,               // 指数或单位平移超出范围
    NotionalExp = 15, UnsupportedKind = 16,
    TickMismatch = 17,           // tick/lot/min_qty 与交易所原文不符、≤ 0,或 min_qty 不是 lot 的整数倍
    AssetExpTooCoarse = 18,
    ValueRange = 19,             // px_max/qty_max/max_qty/book_window 越界;check_assembly:inst 行数超过 max_universe_insts
    DuplicateSymbol = 20,        // 同 venue 的符号、数值 id 或别名冲突
    // 第 9 步 完整性
    SameExchangeMarkets = 21,
    DuplicateVenueUid = 22,      // 两个账户登记同一 (exchange, venue_uid):其一 is_prod = true,或两者在同一 VenueId 上;两者都是 is_prod = false 且 VenueId 不同时只进 warnings()
    MissingFee = 23, MissingVenueUid = 24,
    TooManyAccountsPerVenue = 25, SimIsProd = 26, SimHasUid = 27,
    SimCfg = 28,                 // Sim 账户的 SimAccounts 不恰为一行、没有 SimBalances,或余额的 venue 不在 venues 内;Live 账户有 SimAccounts 或 SimBalances 行
    UidPolicyProd = 29,          // uid_policy = Unverifiable 用在 is_prod = true 或 Sim 账户上
    // 第 10 步 跨版本
    AlreadyActive = 30,          // 与当前 blob 的 SHA-256 相同:幂等,不是错误(调用方按已生效处理)
    VersionNotIncreasing = 31,
    InstRemoved = 32,            // 旧 inst 行缺失或 (venue, symbol, asset) 变化
    AssetChanged = 33,           // 旧资产行缺失,或资产名、AssetExp 变化
    MultiplierChanged = 34,      // px_unit_shift 或 qty_unit_shift 变化
    AccountKindChanged = 35,     // 账户 id、is_prod 或 kind 变化
    UidChanged = 36,
    InstanceChanged = 37,        // 旧实例行缺失或 (kind_id, name) 变化
    // 参数校验
    AccountBound = 38, KindMismatch = 39,
    LegNotTradable = 40,         // 腿 inst 不存在或状态不是 Trading/ReduceOnly
    LegVenueNotInAccount = 41,   // 腿的 venue 与 inst 的 venue 不符,或不在账户 venues 内
    LegAssetMismatch = 42, QuoteMismatch = 43, SameExchangeLegs = 44,
    // ECF1 与跨 blob、跨组件
    NoValidator = 45,
    HostClassMismatch = 46,      // ECF1 host_class 与运行主机不符
    RefInstNotSubscribed = 47,   // ref_inst 不存在或状态不是 Trading/ReduceOnly(引擎无法订阅它)
    UnknownStrategyKind = 48, CapsExceedProtocol = 49,
    StaleGuardMismatch = 50,     // VenueMeta.stale_guard 与编译进来的协议不一致
    BindingChangedWithPosition = 51,
}
```

### 数据结构

> 草案:本节与下文段式容器的 `#[repr(C)]` 布局、行版本与码值实现时以代码与编译期断言为准;码值与字段名全仓一致。

```rust
#[repr(C, align(64))]
pub struct InstHot {                 // 64 B,热路径唯一需要的一行
    pub venue: VenueId, pub status: InstStatus, pub kind: InstKind, pub flags: u8, // flags: NO_AMEND=1(该 inst 不支持改单,覆盖 venue 能力位)
    pub px_exp: i8, pub qty_exp: i8, pub px_wire_exp: i8, pub qty_wire_exp: i8,   // wire = 资产指数 ± 合约平移
    pub asset: AssetId, pub quote: AssetId, pub book_window: u32,                  // 簿窗口半宽 W(tick 数)
    pub tick: i64,                   // 价格步长,资产 px_exp 下的整数
    pub lot: i64,                    // 数量步长,资产 qty_exp 下的整数(资产单位,不是张数)
    pub min_qty: Qty, pub max_qty: Qty, pub min_notional: Notional,
    pub l2_depth: u16, pub crossed_tolerance: u16, pub _pad: u32,
}

pub struct InstMeta {                // 冷字段
    pub id: InstId, pub symbol: Box<str> /* 加载时从 blob 字符串池复制 */, pub venue_num_id: u64,
    pub px_unit_shift: i8,           // venue 价格按 10^s 个资产单位报价:1000PEPEUSDT → 3
    pub qty_unit_shift: i8,          // 1 个 venue 数量单位 = 10^s 资产单位:1000PEPEUSDT → 3;OKX ctVal=1e7 → 7
    pub tick_str: Box<str>, pub lot_str: Box<str>, pub min_qty_str: Box<str>, // 交易所原文,加载时复核(第 7 步)
    pub funding_interval_s: u32,     // 默认周期;实际周期以 Funding 事件为准
    pub funding_anchor_s: u32,       // 结算对齐偏移(UTC 秒)
    pub max_leverage: u16, pub listed_utc_ns: i64, pub delivery_utc_ns: i64, // 0 = 永续/现货
    pub mmr_tiers: Box<[MmrTier]>, // 分档维持保证金率(RDB1 段 18,只对 Perp;按 notional_max 升序;空 = 未登记);hft-sim 的 MmrSource::RefdataTiers 与压力用例使用,缺失时 hft-sim 报 MmrTiersMissing
    pub price_band: PriceBand,         // 交易所价格限制带;up = down = Rate(0) 表示未登记(hft-sim 跳过 PriceLimit 检查)
}
pub struct MmrTier { pub notional_max: Notional /* 本档持仓名义上限(含) */, pub mmr: Rate /* 维持保证金率 */ }
pub struct PriceBand { pub up: Rate, pub down: Rate } // 委托价须在 [ref × (1 − down), ref × (1 + up)] 内;参考价的口径归 hft-sim
impl InstMeta { pub fn market(&self) -> MarketKind; }   // InstKind::Spot → Spot,Perp → LinearPerp(Delivery 不在 core,加载拒绝)

pub struct AssetMeta { pub id: AssetId, pub name: Box<str>, pub class: AssetClass, pub exp: AssetExp,
                       pub px_max: Px, pub qty_max: Qty,    // hft-cfg 给出的上限,用于范围校验
                       pub dust: Qty }                      // 资产级尘埃线(余额、库存与资金费比对的容差,按本资产 qty_exp);hft-risk、hft-ledger、reconciler 共用,全仓唯一来源;0 < dust ≤ qty_max
pub struct VenueMeta {               // 连接与协议常量;交易语义在 VenueCaps
    pub id: VenueId, pub exchange: Exchange, pub product: InstKind,
    pub ws_max_streams_per_conn: u16, pub ws_max_in_msgs_per_s: u16, pub ws_max_conn_age_s: u32,
    pub ws_new_conn_per_5min_ip: u16, pub app_ping_interval_s: u16,
    pub ts_unit: TsUnit,             // 交易所时间戳精度(Ms/Us/Ns)
    pub client_id_max_len: u8, pub client_id_prefix: Box<str>,
    pub max_batch: u8, pub max_inflight_amend: u8,
    pub crossed_tolerance_default: u16,
    pub stale_guard: StaleGuardKind, // None | RecvWindow | ExpireAt:下单请求是否带过期保护(超窗到达的请求被交易所拒绝,各所口径待核实(方法:见各 `venue-*` 的开发备忘));
                                     // NotFound 能否不经成交历史直接定终态由它与槽是否在链接内共同决定:只有 ExpireAt venue 的非链接槽在 grace 之后可定 Rejected,
                                     // 其余(链接腿、None/RecvWindow venue 的全部槽)按 Unknown + 成交历史确认收敛(规则归 hft-oms)
}
#[repr(u8)] pub enum StaleGuardKind { None = 1, RecvWindow = 2, ExpireAt = 3 }   // 与 hft-venue-proto `VenueProtocol::stale_guard()` 一致(启动时交叉校验)
#[repr(u8)] pub enum Exchange { Binance = 1, Okx = 2, Bitget = 3, Gate = 4, Bybit = 5, Kucoin = 6, Hyperliquid = 7, Chain = 32 }

pub struct InstanceMeta {            // 策略实例表(RDB1 段 15);StrategyInstanceId 的唯一分配处
    pub id: StrategyInstanceId,      // 稠密,1..=254(GroupId 的构造用它作高 8 位,hft-types),永久绑定 (kind_id, name)
    pub kind_id: u16,                // 策略种类(StrategyDesc.kind_id);hft-core 启动时核对它在编译进来的 StrategySet 里
    pub name: Box<str>,              // 如 "pc-main"、"pc-sim-a";唯一,ops_bot 与命令里的实例名
}

pub struct AccountMeta {
    pub id: AccountId,
    pub kind: AccountKind,           // Live | Sim(SimAccountCfg)
    pub bound_instance: StrategyInstanceId, // 唯一使用该账户的策略实例;0 = 未绑定(任何策略都不能用它下单)
    pub is_prod: bool,               // 正式资金账户;Sim 恒为 false;交易所测试网/模拟盘联调账户 = Live + is_prod = false(只出现在联调用实例的 refdata 中)
    pub venues: u64,                 // VenueId 位图:Live = 可交易的 venue;Sim = 撮合所用行情的 venue;同一 Exchange 至多一个 VenueId(SameExchangeMarkets)
    pub label: Box<str>,             // 账户名:命令 JSON 的 account、ops_bot 路由与显示、hft-cfg TOML 都用它;加载时唯一;
                                     // sim 实例的账户必须以 "sim-" 开头、prod 实例的不得以它开头(规则与退出码 AccountNamePrefix 归 apps/hft-core,hft-cfg 构建时预检)
    pub uid_policy: UidPolicy,       // 交易所 UID 能否核验;缺省 Verified(见下)
}
pub enum AccountKind { Live, Sim(SimAccountCfg) }   // 行内编码 u8:Live=1 Sim=2(与 CmdPreview.target_kind 同码)
#[repr(u8)] pub enum UidPolicy { Verified = 1, Unverifiable = 2 }   // gen-schema 随 RDB1 行导出
pub struct SimAccountCfg {           // 只对 Sim;四个字段的语义(撮合、延迟抽样、费率档、保证金与强平)与模型定义归 hft-sim
    pub initial_balances: Box<[SimBalance]>, // SimReset 回到这里(SimBalances 段)
    pub latency_model: u16,          // hft-core 配置 [sim.latency_models] 数组的下标(0 起)
    pub fee_tier: u8,                // 费率档:取 Fees 段中 (venue, tier = fee_tier) 的行;本账户有 FeeOverrides 行时以覆盖行为准
    pub margin_model: u16,           // hft-core 配置 [sim.margin_models] 数组的下标(0 起)
}
pub struct SimBalance { pub venue: VenueId, pub asset: AssetId, pub qty: Qty }
// SimAccounts(RDB1 段 16)每 Sim 账户一行定长 { account, fee_tier, latency_model, margin_model };两个 model 下标的范围
// 由 hft-core 启动时按其配置校验(越界 → 拒绝启动,见 hft-sim / apps/hft-core),refdata 本身不含模型定义。
// 每 (account, venue) 一行 AccountVenues(RDB1 段 14):只对 Live 账户,带交易所 UID(启动闸门 IDENTITY 用它比对);Sim 账户没有该段的行

pub struct FeePair { pub maker: Rate, pub taker: Rate }                     // 负 maker = 返佣
pub struct RateLimit { pub scope: LimitScope, pub bucket: LimitBucket, pub limit: u32, pub window_ms: u32, pub default_cost: u16 }
pub struct Endpoint { pub role: EndpointRole, pub env: EndpointEnv, pub url: Box<str>, pub conn_group: u8 }
pub struct MaintWindow { pub venue: VenueId, pub inst: InstId /* 0 = 全 venue */, pub start_ns: i64, pub end_ns: i64, pub kind: MaintKind }
pub struct ChainToken { pub chain: VenueId, pub asset: AssetId, pub address: [u8; 32], pub decimals: u8 }   // optional 链上

#[repr(u8)] pub enum InstStatus { Invalid = 0, Trading = 1, ReduceOnly = 2, Halted = 3, Maintenance = 4, PreTrading = 5, Delisted = 6 }
#[repr(u8)] pub enum InstKind   { Spot = 1, Perp = 2, Delivery = 3 }
#[repr(u8)] pub enum MaintKind  { VenueMaintenance = 1, MarketClosed = 2 /* 标的市场休市(Equity 类资产) */, Delisting = 3 }
#[repr(u8)] pub enum AssetClass { Crypto = 1, Stable = 2, Equity = 3 /* 股票类代币 */, Commodity = 4 }
#[repr(u8)] pub enum LimitScope { Ip = 1, Uid = 2, Account = 3, Conn = 4, Inst = 5 }
#[repr(u8)] pub enum LimitBucket { Orders = 1, Weight = 2, WsInMsgs = 3, NewConns = 4, Amends = 5 }
#[repr(u8)] pub enum EndpointRole { MdPublic = 1, MdMarket = 2, UserData = 3, OrderWs = 4, Rest = 5, RestBackup = 6 }
#[repr(u8)] pub enum EndpointEnv  { Prod = 1, Testnet = 2, Demo = 3 }
#[repr(u8)] pub enum TsUnit       { Ms = 1, Us = 2, Ns = 3 }
```

**`uid_policy`**(账户配置写 `uid_policy = "verified" | "unverifiable"`(hft-cfg 的取值一律小写),缺省 `"verified"`;对应 `UidPolicy::{Verified, Unverifiable}`):`Verified` = 启动闸门 `IDENTITY` 按 `AccountVenues` 登记的 UID 核验交易 key;`Unverifiable` = 交易所测试网读不到 UID 时的例外,只允许 `is_prod = false` 的 Live 账户(否则 `UidPolicyProd`),该账户的 `AccountVenues` UID 可为空串(空串不参与 `DuplicateVenueUid`),`IDENTITY` 改按 kill 自检加一次成功签名的只读请求放行并写 P1 `Note{IdentityUnverified}`(放行规则归 [hft-risk](../hft-risk/README.md));在 TOML 里声明它需要 Admin 签字([tools hft-cfg](../../tools/README.md#hft-cfg))。

**一个账户在每个交易所只用一个市场**:`AccountMeta.venues` 里同一 `Exchange` 至多出现一个 `VenueId`(现货或 USDT 永续之一),加载校验 `SameExchangeMarkets`;`GroupSpec` 的任两腿也不得在同一交易所(`SameExchangeLegs`)。同所现货与永续共用钱包(统一账户)的情形被本规则排除,因此本 crate 不建模余额域,ledger、SDK 视图与 hft-sim 都按 venue 记现金。

### 交易所能力表(VenueCaps)

每个 `VenueId` 一行。`VenueId` 本身按产品线划分(`VenueMeta.product`),所以能力表**只以 `VenueId` 为键**;行内的 `market` 是由 `VenueMeta.product` 推出的冗余字段(Spot → `Spot`,Perp → `LinearPerp`),加载时与 [hft-types `VenueId::market()`](../hft-types/README.md#id-与枚举) 及编译进来的 `VenueProtocol::MARKET` 交叉校验。策略视图按 venue 取这一行([hft-strategy-sdk](../hft-strategy-sdk/README.md));视图若同时给出市场段,只用于断言一致。**策略与执行层不出现交易所名字**:凡是交易所之间的差异,要么在这张表里,要么在对应 `venue-*` 的协议实现里。

```rust
pub struct VenueCaps {                       // #[repr(C)]:能力位与死手字段在首个缓存行;实际字段顺序与显式 _pad 以代码为准
    pub venue: VenueId, pub market: MarketKind,   // market = VenueMeta.product 推出的冗余字段
    pub fee_asset_rule: FeeAssetRule,        // 手续费扣哪个资产 → 决定成交净量(hft-ledger FillDelta.net_qty)
    pub reduce_only: bool,                   // 是否支持 reduceOnly(不支持时 hft-risk 只按本地仓位校验减仓量)
    pub required_position_mode: PositionMode,// 我们要求的账户配置;NotApplicable = 不检查
    pub required_margin_mode: MarginMode,
    pub required_multi_asset: Option<bool>,  // None = 不检查;线编码 u8:1 = None,2 = Some(false),3 = Some(true)
    pub required_spot_borrow: Option<bool>,  // 现货借币(含自动借币、现货杠杆)要求:Some(false) = 必须关闭;None = 不适用(永续行;或该产品线的现货账户不能借币)
                                             // 线编码同上;Some(true) 加载拒绝(SpotBorrowRequired:策略只做"多现货",从不借币卖出)
    pub funding_settle: FundingSettleKind,   // 资金费实收推送方式
    pub margin_data: MarginData,             // 保证金率、强平价、ADL 档的来源
    pub own_liquidation_marker: LiqMarker,   // 识别自身强平/ADL 订单的方法(判定由 venue crate 实现)
    pub liq_marker_arg: Box<str>,            // 方法参数(如 client id 前缀);无则 ""
    pub ignorable_user_events: Box<[Box<str>]>, // 用户流中可忽略的事件名:计数,不当解码失败
    pub flags: CapFlags,                     // AMEND=1 POST_ONLY=2 IOC=4 FOK=8 BATCH_CANCEL=16 PERP_CASH=32
    pub amend_qty_down_only: bool,           // 改单只能减量、不能改价(AMEND 置位时才有意义);hft-oms 拒绝该 venue 上改价或增量的 Amend(AMEND_UNSUPPORTED)
    pub dms: DeadManCap,                     // 交易所侧死手;None → 只有 kill-switch 兜底
    pub dms_min_ms: u32, pub dms_max_ms: u32,      // 交易所允许的超时范围(ms;max = 0 表示无上限);None 时为 0
    pub dms_timeout_ms: u32, pub dms_refresh_ms: u32, // 生效的超时与刷新周期(ms):DeadManRefresh 的 arg 取 timeout_ms,sequencer 的 DeadMan 定时器周期取 refresh_ms
    pub testnet: TestnetKind,                // 测试环境(端点见 Endpoint.env)
}
pub enum FeeAssetRule {
    Quote,                                   // 手续费以报价资产扣除:成交净量 = 毛量
    ReceivedAsset,                           // 以收到的资产扣除:买入现货时净量 = qty − fee
    ReceivedAssetUnlessDiscount { token: AssetId }, // 平台币余额足够时以平台币抵扣,否则同 ReceivedAsset(逐笔按回报里的 fee_asset 判定)
    Configured(AssetId),                     // 账户配置的固定扣费资产
}
#[repr(u8)] pub enum FundingSettleKind { PerSymbolPush = 1, BalanceOnlyPush = 2, RateOnly = 3, NotApplicable = 4 /* 现货 */ }
pub enum MarginData { Push, Poll { interval_ms: u32 }, NotApplicable }   // 线编码 u8 Push=1 Poll=2 NotApplicable=3 + interval_ms
#[repr(u8)] pub enum PositionMode { NotApplicable = 1, OneWay = 2, Hedge = 3 }      // 与 hft-event AccountConfigReport 同码
#[repr(u8)] pub enum MarginMode   { NotApplicable = 1, Cross = 2, Isolated = 3, Mixed = 4 } // Mixed 只作观测值(AccountConfigReport:各 inst 模式不一致);作为 required_margin_mode 加载拒绝
#[repr(u8)] pub enum MultiAsset   { NotApplicable = 1, Off = 2, On = 3 }       // 观测值编码(AccountConfigReport.multi_asset),不是要求
#[repr(u8)] pub enum LiqMarker    { NotIdentifiable = 1, ClientIdPrefix = 2, OrderTypeField = 3, ExecTypeField = 4 }
#[repr(u8)] pub enum DeadManCap   { None = 1, CountdownPerInst = 2, CountdownAccount = 3, OnDisconnect = 4, Schedule = 5 } // 与 hft-venue-proto DeadManSpec 的变体一一对应
#[repr(u8)] pub enum TestnetKind  { None = 1, Testnet = 2, Demo = 3 }
```

| 字段 | 谁用 | 含义与后果 |
|---|---|---|
| `fee_asset_rule` | hft-ledger、hft-oms(LegTracker)、hft-exec、hft-sim | 仓位、现货库存与对冲量按**净量**计;FeeOnly 晚到时补差 |
| `reduce_only` | hft-risk(classify)、hft-oms | 不支持的市场(现货)减仓单不带交易所 reduceOnly,数量由本地库存与可用余额约束 |
| `required_*` | gw → `AccountConfigReport.mismatches`;hft-risk 启动闸门 `ACCOUNT_CFG` 与 `AccountCfg` cap | 不一致不放行;运行中变化设 cap。`required_spot_borrow = Some(false)` 的现货行:账户的现货借币/自动借币开启 → `AUTO_BORROW` 位(默认值:`OKX_SPOT` 要求 `enableSpotBorrow = false`,`BITGET_UTA_SPOT` 要求现货杠杆关闭;`BINANCE_SPOT` 用现货账户、不能借币,为 None 并由 `ACCT_TYPE` 位核验账户类型;读取接口与字段待核实(方法:三所模拟盘上读配置,见本页开发备忘),结论写在各 `venue-*`) |
| `funding_settle` | venue 协议、hft-sequencer、hft-ledger | `PerSymbolPush` 直接入账;`BalanceOnlyPush` 收到 `FundingSettle{inst = INST_NONE}` 只校验,按"结算时刻持仓 × mark × 费率"合成每 inst 的 `synth = 1` 记录入账;`RateOnly` 只合成 |
| `margin_data` | gw(轮询或解码推送)→ `MarginReport`;hft-risk(MarginStress) | `Poll` 的 `interval_ms` 是轮询周期;数据年龄超过 `margin_stale_ms`(RiskParams)视为 L1 |
| `own_liquidation_marker` | venue 协议 → `ExternalFill.cause` | `NotIdentifiable`:外来成交一律 `cause = Unknown` |
| `ignorable_user_events` | hft-venue 运行时 | 名单内的事件只计数;名单外的未知事件计数并写 `Note{Venue}`,也不当解码失败、不触发查询 |
| `flags`、`dms` | hft-oms、hft-exec(选算法)、hft-venue、kill-switch | 能力位为 0 的操作不下发 |
| `amend_qty_down_only` | hft-oms | 为真时只放行价格不变、数量减少的 Amend,其余 Amend 拒绝(`AMEND_UNSUPPORTED`,规则归 hft-oms) |
| `testnet` | 上线流程的测试网冒烟 | 只描述有无;端点在 `Endpoint{env}` |

**`PERP_CASH`**:仅允许永续 venue,表示协议对成交、累计查询及外来成交提供完整的实际结算事实,格式归 [执行事件](../hft-event/README.md#执行0x02xx)。消费者必须按覆盖范围验证并去重,缺失不能退回平均成本猜现金。旧配置未声明时保留旧记账语义,不保证乱序交付下钱包实额一致。该位仍须属于编译协议明确支持的能力子集;模拟装配也要记录生效配置。

**死手**:`dms` 与各 `venue-*` 的 `VenueProtocol::dead_man()`([hft-venue-proto `DeadManSpec`](../hft-venue-proto/README.md))变体同名、同义,四个时长字段一律以 ms 计;`hft-cfg` 以协议默认值为底写入生效的整行(超时与刷新可按账户需要收紧)。hft-core 启动时核对 `dms` 变体与编译进来的协议一致(不符 → `CapsExceedProtocol`)。

**杠杆不在 refdata**:永续的实际杠杆是账户在交易所上的设置(运营按 [runbooks/launch](../../docs/runbooks/launch.md) 设定),由 gw 逐 inst 读到并经 [`AccountConfigReport` 的杠杆条目](../hft-event/README.md#执行0x02xx)报告;hft-risk 与策略视图只用读到的值。`InstMeta.max_leverage` 只是交易所允许的上限(供 `hft-cfg` 审阅),不参与任何风控计算;refdata 与 `EngineConfig` 都没有"默认杠杆"常量(风控的口径见 [hft-risk](../hft-risk/README.md))。

**默认值与覆盖**:每个 `venue-*` 经 `VenueProtocol::default_caps()`(协议同时声明 `const MARKET: MarketKind`,[hft-venue-proto](../hft-venue-proto/README.md))给出默认行,并在其 README 写明每个字段的出处,未核实的字段注明核实方法;`hft-cfg` 生成 refdata 时以默认行为底、叠加 TOML 中的覆盖,把**最终生效的整行**写进 blob(journal 自足)。覆盖只能收紧:能力位只能关不能开、`amend_qty_down_only` 只能由假改真、`ignorable_user_events` 只能追加;`required_*`、`fee_asset_rule`(账户的平台币抵扣设置)、`margin_data.interval_ms` 可改,覆盖处必须写 `reason`。hft-core 启动时用编译进来的 `default_caps` 复核"能力位 ⊆ 协议支持"且协议为真时 `amend_qty_down_only` 为真,不符 → `CapsExceedProtocol`,拒绝启动。

### 配置项

| 名称 | 类型 | 含义 | 来源 |
|---|---|---|---|
| `refdata.initial_blob` | 路径 | 冷启动时加载的 blob(无 journal 可恢复时) | `apps/hft-core` 配置 |
| `refdata.max_universe_insts` | u32,默认 8192(常量 `DEFAULT_MAX_UNIVERSE_INSTS`) | blob 中 inst 行数(交易宇宙)的预分配上限;超出时 `check_assembly` 报 `ValueRange`(装配失败)。与 ECF1 段 3 的 `max_insts`(本实例订阅的 inst 稠密表容量)不是同一个量:启动时校验 `max_insts ≤ max_universe_insts` | 同上 |
| `refdata.max_symbol_len` | 常量 27 | 符号表内联键长度 | 本 crate |

## 实现说明

<details><summary>实现要点</summary>

**段式容器**(RDB1 与 ECF1 共用;小端,所有段 8 B 对齐;行是 `#[repr(C)]` + `zerocopy`;编码器只有 Rust 的 `RdbBuilder`/`EcfBuilder`):

```
RdbHeader (64 B) { magic: *b"HFTRDB01", format: u16 = 1, n_sections: u16, _pad: u32,
                   version: u64, base_version: u64 /* 0 = 全量;只支持全量,≠ 0 → Structure */, created_utc_ns: i64, total_len: u64, _rsv: [u8; 16] }
SectionEntry (24 B) × n_sections { id: u16, row_ver: u16, row_size: u32, offset: u64, count: u32, flags: u32 /* OPTIONAL=1 */ }   // 段 id 在目录中唯一
RDB1 段(本 crate 定义行格式):Assets(1) Venues(2) Insts(3) Accounts(4) Fees(5:每 (venue, tier) 的 maker/taker) FeeOverrides(6:每 (account, venue[, inst]))
         RateLimits(7) Endpoints(8) Maint(9) Tokens(10,optional 链上) Aliases(11) Strings(12) VenueCaps(13)
         AccountVenues(14:Live 账户 × venue 的交易所 UID)
         Instances(15) SimAccounts(16:SimAccountCfg 的定长部分 = fee_tier 与两个模型下标) SimBalances(17)
         MmrTiers(18:每 (inst, 档) 一行 {inst, notional_max, mmr},按 (inst, notional_max) 升序;可缺省)
         StrRefs(19:StrRef 数组,ignorable_user_events 的名单)
RDB1 32–37:退役,永不复用;出现即拒绝(不论 OPTIONAL 位)
字符串引用 StrRef { off: u32, len: u32 } 指向 Strings 段(UTF-8,不以 0 结尾);ignorable_user_events 是 StrRef 列表(段 13 行内 {off, count} 指向段 19 中连续的 StrRef)
```

refdata 不含 group(在 `StrategyParams` 的 `GroupSpec`)、OMS/ledger/容量参数(在 `EngineConfig`)、`risk_class`(在 [hft-risk](../hft-risk/README.md) 的 `RiskParams`)与链上路由表(optional,归 [hft-chain-evm](../hft-chain-evm/README.md))。

行内字段的字节偏移以代码与 `gen-schema` 导出(`python/_generated/refdata.py`)为准,本 README 只写语义。

**`EngineConfig`(ECF1)**:引擎装配参数,按主机类由 hft-core 配置给出,以 `ConfigBlob{kind = Engine}` 记入 journal([hft-event `BlobKind`](../hft-event/README.md#变长记录前缀)),每段引导块重述。它不是参考数据,放在本 crate 只因复用同一容器与加载框架。头部与 RDB1 相同,`magic = *b"HFTECF01"`,`_rsv` 的首字节为 `host_class`,编码即 [hft-types `HostClass`](../hft-types/README.md#id-与枚举)(`host_class()` 返回该类型)。主机类的含义、选定时机与各类取值归 [apps/hft-core](../../apps/hft-core/README.md)。

| ECF1 段 | 名 | 行格式与语义校验归属 | 内容 |
|---|---|---|---|
| 1 | `oms` | [hft-oms](../hft-oms/README.md) | `OmsConfig`:每 (venue, account) 槽容量、过期窗口、扇出上限 K |
| 2 | `ledger` | [hft-ledger](../hft-ledger/README.md) | `LedgerConfig`:资产 → USDT 参考 inst(`ref_inst`) |
| 3 | `capacities` | [hft-sequencer](../hft-sequencer/README.md) | `Capacities`(容量,含 `max_insts`、`max_cells_total`、`max_groups_per_instance`;字段表归 hft-sequencer,各主机类的取值唯一归属 [apps/hft-core](../../apps/hft-core/README.md)) |
| 4 | `rings` | [apps/hft-core](../../apps/hft-core/README.md) | 各环容量(槽数或字节);行格式 `RingCapRow` 与各主机类取值归 [apps/hft-core 环容量](../../apps/hft-core/README.md#配置文件字段) |
| 5 | `sequencer` | hft-sequencer | sequencer 的引擎级参数(每环批量上限、关键帧触发、advisor 预算等;一行,字段表、行格式与默认值唯一归属 hft-sequencer) |
| 6 | `conn` | [hft-venue](../hft-venue/README.md) | 连接轮换余量(相对 `ws_max_conn_age_s`) |
| 7 | `exec` | [hft-exec](../hft-exec/README.md) | `ExecConfig`:执行层的引擎级参数(补腿阈值、冷却、截止、默认减仓切片等;变更 = Drain + 重启) |
| 8 | `assembly` | [hft-sequencer](../hft-sequencer/README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用) | 决策装配事实(实例角色、gw 顺序、feed 集合、账户 key 槽位要求及 advisor/Drain 参数);不含秘密 |

ECF1 段 id 只追加、永不复用。每段只有一种行格式(`SectionEntry` 只有一个 `row_ver` 与 `row_size`);一个归属 crate 需要两种行时各占一段,在上表追加。`EngineConfig::load` 只做容器级校验:同 RDB1 第 1 步(magic 为 `HFTECF01`,`host_class` 编码合法)与第 10 步的版本规则(`current` 存在时 `version > current.version`,SHA-256 相同 → `AlreadyActive`);冷读者只调用它。`EngineConfig::validate` 按段 id 升序对每个存在的段调用 `section` 相同的 `EngineValidator`;存在而没有验证器的段 → `NoValidator`(`section` = 段 id),验证器返回的错误原样收集。`host_class` 与运行主机不符由 `check_assembly` 拒绝(`HostClassMismatch`)。**容量可以缩小,但只经 Drain + 新 EngineConfig 重启**(变更 = 重启,与 refdata 相同;`load` 不比较新旧容量):恢复快照时,各归属 crate 的 `restore` 发现当前占用(槽、cell、定时器、实例、订阅 inst、积压的输出)超过新容量 → `CapacityShrink`,hft-core 退出 2(退出码归 [apps/hft-core](../../apps/hft-core/README.md));不超过则按新容量重新分配并继续。

**跨 blob 与跨组件校验**(全部为加载/命令失败条件):

1. hft-core 启动(本 crate 的 `check_assembly` 按此顺序检查并收集全部错误):ECF1 `host_class` 等于运行主机类(`HostClassMismatch`);inst 行数 ≤ `max_universe_insts`(`ValueRange`);每个有 inst 的 `VenueId` 有对应的 `VenueProtocol` 编译进来(`UnknownVenue`),其 `VenueCaps.market` 等于协议的 `MARKET`(`CapsMarketMismatch`)、`VenueCaps.flags ⊆ default_caps` 的能力位、协议的 `amend_qty_down_only` 为真时该行也为真,且 `dms` 变体与协议相同(`CapsExceedProtocol`)、`VenueMeta.stale_guard` 与协议一致(`StaleGuardMismatch`);每个 `InstanceMeta.kind_id` 在编译进来的 `StrategySet::kinds()` 里(`UnknownStrategyKind`)。某账户的 `bound_instance` 相对快照中的上一版本发生变化时,该账户必须无仓位、无挂单、无 Unknown(`check_rebind`:`BindingChangedWithPosition`,调用方用快照判定"有仓位、挂单或 Unknown");Sim 账户的 `latency_model`/`margin_model` 下标在 hft-core 的 `[sim.*_models]` 表内;实例角色与账户类型一致(`sim` 实例只含 Sim 账户);描述符 `StrategyDesc.non_prod_only = true` 的策略实例不得绑定 `is_prod = true` 的账户(`ProbeOnProdAccount`)——这两条的错误码与退出码归 [apps/hft-core](../../apps/hft-core/README.md)。
2. `SetParams`(ctl-gateway 先查、sequencer 复核):`check_binding`——`AccountMeta.bound_instance == hdr.strategy`(否则 `AccountBound`)且该实例的 `kind_id == hdr.kind_id`(`KindMismatch`);`check_group`——每腿 inst 存在且 `Trading`/`ReduceOnly`、`LegDef.venue` = inst 的 venue、该 venue 在账户 `venues` 内(`LegVenueNotInAccount`)、各腿基础资产 = `GroupSpec.asset`(`LegAssetMismatch`)、各腿同一报价资产(`QuoteMismatch`)、**任两腿的 venue 不属于同一 `Exchange`**(`SameExchangeLegs`,命令侧报 `InvalidArgs{SameExchangeLegs}`)。
3. 每个 `ref_inst`(EngineConfig 段 2)必须存在于 refdata 且状态为 `Trading`/`ReduceOnly`(`check_ref_inst`:否则 `RefInstNotSubscribed`;段 2 的验证器对每个 `ref_inst` 调用它)。引擎订阅恒含每个 `ref_inst`([hft-sequencer](../hft-sequencer/README.md) 的引擎自身订阅 ③),所以"被订阅"不另行校验;不存在或不可交易的 inst 无法订阅,也就没有汇率。

行可以追加字段(`row_size` 大于本版本已知大小时读前缀);未知段带 `OPTIONAL` 位则跳过,否则拒绝。blob 自身不含哈希,身份是整块字节的 SHA-256(写进 `ConfigBlob` 头)。

**加载校验**(按顺序;任何一步失败整体拒绝,不部分生效——[错误配置在加载时失败](../../AGENTS.md#常驻规则)。同一步内收集该步的全部错误,按该步规则的列出顺序、同一规则内按 (段 id, 行下标) 升序;某一步有错误时不进入下一步,`LoadReport` 只含这一步的错误):

1. 结构:magic/format、`total_len == blob.len()`、`base_version = 0`、`version ≥ 1`、段不越界不重叠、段 id 唯一、`row_size ≥` 已知大小、对齐;未知段与退役段按上文;行内枚举与 Option 线编码的判别值合法。
2. 字符串:`StrRef` 在界内、合法 UTF-8;符号与别名长度 1..=27,不含空白与控制字符;实例名与账户标签唯一。
3. ID:Assets、Insts、Accounts、Instances 的 id 从 1 起稠密、唯一、按 id 排序;Venues 与 VenueCaps 以 `VenueId` 为键(按常量表取值,不要求稠密),唯一、升序;`VenueId` 必须在 [hft-types 常量表](../hft-types/README.md#id-与枚举)内(`UnknownVenue`);Instances 的 id ≤ 254(`InstanceIdRange`,`GroupId` 构造要求)。
4. 引用:inst 的 venue/asset/quote 存在;quote 的 `class == Stable`;费率行的 (account, venue) 存在;维护窗口、端点、限速、代币行引用的 venue/inst/asset 存在;代币行的 chain 是链上 venue;别名行的 inst 存在且属于该行的 venue;`VenueCaps` 行的 venue 存在于 Venues,其 `fee_asset_rule` 引用的资产存在;`AccountMeta.bound_instance` 为 0 或存在于 Instances、`venues` 位图中的每个 venue 存在于 Venues,且每个实例至多被一个账户绑定(`InstanceMultiBound`);`AccountVenues`、`SimAccounts`、`SimBalances` 引用的账户、venue、资产存在;`MmrTiers` 行的 inst 存在。
5. 能力表:每个被 inst 引用的 `VenueId` 恰有一行 `VenueCaps`(`MissingCaps`),`market` 与 `VenueMeta.product`、`VenueId::market()` 一致(`CapsMarketMismatch`);`InstKind` 为 `Spot`/`Perp` 的 inst 与其 venue 的 `VenueMeta.product` 相同(`CapsMarketMismatch`;`Delivery` 留给第 6 步);现货行显式 `NotApplicable`:`required_position_mode`、`required_margin_mode`、`funding_settle`、`margin_data`,且 `required_multi_asset = None`(违反 → `CapsMarketMismatch`);现货行 `required_spot_borrow ∈ {None, Some(false)}`(`Some(true)` → `SpotBorrowRequired`),永续行 `required_spot_borrow = None`(`CapsMarketMismatch`);永续行 `funding_settle ≠ NotApplicable`(`CapsMarketMismatch`);`Poll` 的 `interval_ms ∈ [1000, 600000]`(`CapsValue`);`dms ≠ None` 时 `dms_refresh_ms > 0`、`dms_timeout_ms ≥ max(dms_min_ms, 2 × dms_refresh_ms)`(丢一次刷新不触发交易所侧全撤)且 `dms_max_ms = 0` 或 `dms_timeout_ms ≤ dms_max_ms`,`dms = None` 时四个时长字段为 0(`DeadManRange`);`required_margin_mode ≠ Mixed`(`CapsMarketMismatch`)。
6. 指数:`px_exp, qty_exp ∈ [-18, 6]`;**`px_exp + qty_exp ≤ −6`**(保证名义换算只做除法,见 [hft-types](../hft-types/README.md#实现说明);违反 → `NotionalExp`);`px_unit_shift, qty_unit_shift ∈ [-12, 12]`;`px_wire_exp = px_exp + px_unit_shift`、`qty_wire_exp = qty_exp − qty_unit_shift` 由加载器计算,不从 blob 读,且两者 ∈ [-18, 6](`parse_exact` 的指数范围;越界 → `ExpRange`);`InstKind::Delivery` 拒绝(`UnsupportedKind`)。
7. 步长复核:用 `parse_exact(tick_str, px_wire_exp)` 重新解析交易所原文,结果必须等于行内 `tick`(不等或其他解析错误 → `TickMismatch`;`PRECISION` → `AssetExpTooCoarse`)。`lot`(`lot_str`)、`min_qty`(`min_qty_str`)同理,以 `qty_wire_exp` 解析。`tick > 0`、`lot > 0`、`min_qty > 0`、`min_qty` 是 `lot` 整数倍(`TickMismatch`)。
8. 范围与唯一性:`px_max`、`qty_max` 的 raw ≤ `MAX_RAW / 4`;`max_qty ≤ qty_max`;`book_window × tick ≤ px_max`;`book_window ∈ [64, 65536]`;同 venue 内符号唯一、非零 `venue_num_id` 唯一、别名不与符号冲突;资产 `0 < dust ≤ qty_max`;`MmrTiers` 只出现在 `Perp` inst 上,同一 inst 的 `notional_max` 严格递增、`mmr` 非递减且 `0 < mmr < Rate(1 000 000 000)`(100%);`price_band` 的 `up`、`down` ∈ [0, Rate(1 000 000 000))——违反均为 `ValueRange`。
9. 完整性:**每个账户(Live 与 Sim)的 `venues` 中不得有两个 `VenueMeta.exchange` 相同的 `VenueId`**(`SameExchangeMarkets`:一个账户在同一交易所只用现货或永续之一);不同 `AccountId` 不得登记同一 (exchange, `venue_uid`)(`DuplicateVenueUid`:同一交易所子账户只属于一个逻辑账户;空串不比较)——至少一方 `is_prod = true`,或两方在同一 `VenueId` 上,是加载错误;只有两方都是 `is_prod = false` 且在同一交易所的不同 `VenueId` 上(演示环境的同一账户横跨同所的两个产品线)时加载成功并记一条 `LoadErr{DuplicateVenueUid, section = 14, row = 后一行}` 进 `RefData::warnings()`;Sim 账户的 `SimAccounts`/`SimBalances` 行只属于 Sim 账户(Live 账户有这两段的行 → `SimCfg`);`uid_policy = Unverifiable` 只用于 `is_prod = false` 的 Live 账户(`UidPolicyProd`),空串 UID 只允许出现在这样的账户上(否则 `MissingVenueUid`);Sim 账户的延迟/保证金模型分布(分位单调等)由 hft-sim 在 hft-core 启动时校验(`SimCfgErr::BadDist`),refdata 只校验下标范围;每个 `Trading`/`ReduceOnly` 的 inst,对其 venue 上启用的每个账户(含 Sim)都有费率(覆盖行,或该账户所用档的 `Fees` 行;缺 → `MissingFee`);**Live 账户**在 `venues` 中的每个 venue 都有 `AccountVenues` 行(`MissingVenueUid`);每个 venue 上启用的 Live 账户 ≤ `KillBits::MAX_ACCOUNTS_PER_GW`(12,`TooManyAccountsPerVenue`);**Sim 账户** `is_prod = false`(`SimIsProd`)、没有 `AccountVenues` 行(`SimHasUid`)、恰有一行 `SimAccounts`、至少一条 `SimBalances`、`SimBalances` 的 venue ⊆ `venues`。
10. 跨版本兼容(`current` 存在时;`current = None` 跳过本步):SHA-256 与 `current` 相同 → `Err`,`LoadReport` 恰为一条 `AlreadyActive`(幂等,不是错误;调用方按已生效处理;不执行第 1–9 步);否则 `version > current.version`(`VersionNotIncreasing`);`current` 中每个 inst id 在新 blob 中存在且 `(venue, symbol, asset)` 相同(下架必须以 `Delisted` 出现,不能删行);每个 `AssetId` 的资产名与 `AssetExp` 不变;每个 `InstId` 的合约乘数(`px_unit_shift`、`qty_unit_shift`)不变;账户 id、`is_prod`、`kind`(Live/Sim)不变(`AccountKindChanged`);Live 账户在新旧两版都登记的 venue 上 `venue_uid` 不变(`UidChanged`——换子账户就是新 `AccountId`;新增 venue 不算变化);实例 id 的 `(kind_id, name)` 不变(`InstanceChanged`);本步错误的 `row` 取 `current` 中对应行的下标(旧行在新 blob 中可能已不存在)。`bound_instance` 可变,但受跨组件第 1 条约束。由 `ReplaceRefdata` 触发的 Drain 在写快照前用将要生效的 blob 再跑一次本步,不通过则不以 75 退出、该 blob 标 `Rejected`。
11. 符号表构建(见下),总能成功;探测次数不是加载失败条件。

跨版本不变量保证重启前后快照里的 `InstId`、指数、账户身份仍然有效;若做可选的热替换,它也是并发安全的依据。

**指数规则**(`hft-cfg refdata fetch` 生成资产行时使用;本 crate 只做第 6、8 步的校验):`px_exp = min(该资产所有 inst 的 tick 指数) − 2`,`qty_exp = min(lot 指数) − 1`,再检查 `px_max`/`qty_max` 不超 `MAX_RAW / 4`;超出时放粗 `qty_exp`(可为正)。指数一经发布不可改——改就是新资产。

**符号表**:**有序数组**——每 venue 一个按键字节排序的 `[SymEntry]`(条目 32 B `{ key: [u8; 27], len: u8, inst: u32 }`),查找为二分(≤ 13 次比较 @ 8 192 条);另配一张 `FxHash` 开放寻址前置表(容量 `next_pow2(2n)`,存条目下标)作为可选快速路径,构建确定性(固定种子、按排序序插入)。可选扩展:换成静态最小完美哈希(触发条件:`refdata_symbol_get` 基准证明查表进入关键路径);任何实现下构建都不会失败。别名段把替代写法(大小写、`BTC_USDT` 与 `BTCUSDT`)映射到同一 `InstId`。数值 id 用同构的 `u64` 键表;`venue_num_id = 0` 表示无数值 id,不进表(`get_num(0)` 恒为 `None`)。

**内存布局**:`InstHot` 数组按 `InstId` 下标(`#[repr(align(64))]`,每 inst 一个缓存行);`VenueCaps` 按 `VenueId` 下标的定长数组(每行 ≤ 128 B,热路径只读能力位所在的首个缓存行;`ignorable_user_events` 等借用切片指向冷区);`AccountMeta` 按 `AccountId` 下标,热字段(`kind` 判别值、`bound_instance`、`venues`)在行首同一缓存行;`InstMeta`、字符串池、其余表放在同一次分配的冷区。blob 字节本身保留在 `RefData` 内(`blob()`);字符串与切片字段在加载时复制为 `Box<str>`/`Box<[T]>`(冷区),对外按 `&str`/`&[T]` 借用。

**规划源文件**

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `RefData` 与查询 API |
| `src/types.rs` | 行结构、枚举(只有类型) |
| `src/blob.rs` | RDB1 段目录解析(零拷贝视图)、行格式、`RawBlob`(工具与测试用的可编辑容器) |
| `src/validate.rs` | 第 1–10 步校验,`LoadErr`/`LoadReport` |
| `src/caps.rs` | `VenueCaps` 与其枚举、能力位比较、`check_assembly`(hft-core 的协议复核) |
| `src/accounts.rs` | `AccountMeta`、`AccountKind`/`SimAccountCfg`、`AccountVenues`、`Instances`、`check_binding`/`check_group` |
| `src/symbols.rs` | `SymbolIndex` 构建与查找 |
| `src/fees.rs` | 费率解析(覆盖行 > venue 默认) |
| `src/schema.rs` | 导出行布局描述与 `LoadErrCode` 码表(供 gen-schema 与 Python 只读解析) |
| `src/builder.rs` | `RdbBuilder`、`EcfBuilder`(唯一编码器,tools/hft-cfg 调用) |
| `src/engine.rs` | `EngineConfig`(ECF1)加载与段视图 |
| `src/dump.rs`(feature `tools`) | blob → TOML 可读转储与两版本 diff,供审阅 |

</details>

## 延迟与资源契约

| 入口 | 预算 | 基准 |
|---|---|---|
| `SymbolIndex::get`(≤ 16 字节符号,表已预热) | p50 ≤ 15 ns,p99 ≤ 40 ns | `refdata_symbol_get`、`refdata_symbol_get_miss` |
| `get_num` | p50 ≤ 10 ns | `refdata_num_get` |
| `inst()`、`fees()`、`caps()`、`account()` | ≤ 3 ns(数组下标) | `refdata_inst`、`refdata_caps` |
| `check_group`(4 腿) | 冷路径,≤ 1 µs | `refdata_check_group` |
| `load`(基准输入:5 000 inst、6 个 core venue、20 个账户;不是容量上限) | ≤ 30 ms,冷路径,在 warm 线程或回放驱动执行 | `refdata_load_5000` |

内存:5 000 inst ≈ `InstHot` 320 KB + 冷区与符号表 < 4 MB;`load` 分配,查询零分配。`load` 必须确定性:无随机种子、无 `HashMap` 迭代、无时钟。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:[hft-types](../hft-types/README.md)。
- 外部:`zerocopy`(行视图)、`sha2`(blob 身份;本 crate 不启用 `asm`:冷路径,sha2-asm 需要目标 C 工具链;同进程链接 hft-journal 时经特性合并启用)、`rustc-hash`(FxHash 前置表;表布局不进 journal,换哈希只影响性能);`toml` 仅在 feature `tools`。
- 禁止:`serde_json` 解析 blob、任何 I/O(文件读取由调用方完成)、线程与原子(跨线程发布在 [hft-rt](../hft-rt/README.md))、任何 `venue-*` 与 [hft-strategy-sdk](../hft-strategy-sdk/README.md)(默认能力行由 hft-cfg 从 venue crate 取得后写进 blob;协议与策略种类的复核在 hft-core 装配处)。

## 延伸阅读

- [13 O6 refdata 独立组件](../../design/13-设计评审与优化.md)、[04 §2 符号→inst_id 与指数](../../design/04-行情接入层.md)、[05 §2.1/§7 客户端 id 长度与各所限速](../../design/05-CEX执行层.md)、[09 §4.2 死手能力](../../design/09-可观测性、风控与控制面.md)
- [research/02 交易所接入](../../design/research/02-exchange-connectivity.md)(端点、限速、频道)
- `ReplaceRefdata` 语义与权限:[control-plane](../../docs/subsystems/control-plane.md);blob 在 journal 中的承载:[hft-event `ConfigBlob`](../hft-event/README.md#变长记录前缀);能力表的消费者:[hft-strategy-sdk](../hft-strategy-sdk/README.md)、[hft-exec](../hft-exec/README.md)、[hft-risk](../hft-risk/README.md)、[hft-ledger](../hft-ledger/README.md)、[hft-sim](../hft-sim/README.md)、各 `venue-*`
- 决策出处:[round8-decisions](../../design/_work/round8-decisions.md)(交易所无关、一账户一实例、不做同一交易所的现货 × 永续、现货禁借币与按实际杠杆计算);决策记录:[Agent Notes](../../.agents/notes/README.md) 中的 `refdata-component`、`self-contained-journal`、`venue-caps`、`one-account-per-strategy-instance`、`no-same-exchange-spot-perp`

## 已知限制与后续工作

- **合约乘数只支持 10 的幂**(`px_unit_shift`/`qty_unit_shift`):非 10 的幂的 `ctVal` 会让换算不再是指数平移;这类 inst 在 `hft-cfg refdata fetch` 阶段被排除(加载时也因步长复核失败而拒绝)。出现真实需求时增加 `mult_mantissa` 字段与 i128 换算路径。
- **反向永续、交割合约**不在 core(`InstKind::Delivery` 加载拒绝,`MarketKind` 无对应值)。
- **Hyperliquid 的有效数字 tick 规则**不是固定步长。可选扩展:`TickRule`(触发条件:接入 Hyperliquid)。
- 限速表只描述静态上限;交易所响应头里的实时余量由 venue 运行时闭环,不回写 refdata。
- `VenueCaps` 是静态表:交易所临时改变能力(如维护期间禁用改单)不经 refdata,由 venue 运行时按拒单码处理或经 `VenueNotice{Maintenance}` 登记。
- ECF1 段的语义校验依赖归属 crate 提供 `EngineValidator`;只链接了部分 crate 的进程(如冷读者)只能做容器级校验。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实的取值**(`hft-cfg` 写入前逐项核实,方法:官方文档 + 各所 instruments/exchangeInfo 接口实拉 + 测试网实测,结果写进各 `venue-*` README 的出处列):
  - 限速:Binance USDT-M `ORDERS` 300/10 s 与 1200/min(账户)、`REQUEST_WEIGHT` 2400/min(IP)待核实(方法同上);每连接最多 1024 个流、每连接每秒 ≤ 10 条入向消息——已确认;fstream `/public`、`/market`、`/private` 路由——已确认。OKX、Bitget 的下单与查询限速待核实(方法同上)。
  - 现货借币:OKX 账户配置接口(`GET /api/v5/account/config`)的 `enableSpotBorrow` 字段与各账户模式下是否存在;Bitget UTA 的现货杠杆/借币开关在哪个接口可读;Binance 现货账户确无借币(方法:三所模拟盘上读配置、尝试超余额卖出看是否被拒)。
  - 能力表(各 core venue 逐字段待核实):现货手续费扣币规则(买入是否以收到的基础币扣费、平台币抵扣开关);全仓下资金费推送是否带 symbol(Binance 疑似不带 → `BalanceOnlyPush`);保证金率与 ADL 档的推送/轮询接口;自身强平/ADL 订单的识别字段;用户流里应忽略的事件名(如 Binance `TRADE_LITE`、`ACCOUNT_CONFIG_UPDATE`、`MARGIN_CALL` 的处理方式);死手接口与范围(OKX `cancel-all-after` 10–120 s、Bitget 无死手待核实);测试网/模拟盘的有无与端点。方法:测试网下单/强平演练抓包、对照 raw journal 金样。
  - 各所客户端 id 长度(Binance 36、OKX 32、Bitget 40;optional 的 Gate 28 含 `t-`)。
- 实现顺序:`types.rs` → `blob.rs` + `builder.rs`(fixture 由它生成) → `validate.rs`(先写失败用例)→ `caps.rs` + `accounts.rs` → `symbols.rs` + 基准 → `dump.rs`。
- `inst()` 越界返回 NONE 行而不是 panic:热路径拿到 `status == Invalid` 即丢弃事件并计数;`caps()` 的 NONE 行全部能力位为 0,执行层据此不下单。

</details>
