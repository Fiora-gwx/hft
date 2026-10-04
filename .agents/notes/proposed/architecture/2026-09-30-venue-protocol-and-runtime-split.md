# Agent Note: venue = 纯协议 crate + 通用 venue 运行时

Status: proposed

> **第八轮(2026-10-02)**:首批交易所为 Binance、OKX、Bitget(现货 + USDT 永续),其余 optional;交易所差异以 `VenueCaps` 表达,策略与执行层不出现交易所名;见 [venue-caps](2026-10-02-venue-caps.md)、[ccex-first-scope](2026-10-02-ccex-first-scope.md)。

## Problem

我们要接 7 家交易所(Binance、Gate、OKX、Bybit、Bitget、KuCoin、Hyperliquid)。现有 collector 的三个交易所 crate 各自带 WS 外壳与 tokio 循环,重连、重同步、限速、raw 记录各写一遍([research/00 §1.8](../../../../design/research/00-existing-codebase-inventory.md))。同一份解码逻辑还需要被实盘、mock 交易所、raw 重放、金样测试、fuzz 共用;如果解码器和 socket 纠缠在一起,后四者都只能测一个复制品。

## Proposal

[13 O2](../../../../design/13-设计评审与优化.md)、[10 §2](../../../../design/10-语言分工与代码组织.md):

1. **`hft-venue-proto`(pure)** 只定义 `VenueProtocol` trait 及其纯类型(帧缓冲、错误位、重同步方法、订阅描述),不含任何实现与 I/O。
2. **`venue-<x>`(pure)** 实现 `VenueProtocol`,只含纯函数:解码行情与回报(`&[u8]` → 事件或错误位)、编码订单帧模板、登录/签名、限速表、重同步规则(快照与增量拼接的序号条件)、订阅消息。不碰 socket,只依赖 `hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto` 与编解码库。
3. **[`hft-venue`](../../../../crates/hft-venue/README.md)(hot)** 提供通用运行时:feed/gw 线程循环、连接池与错峰轮换、重同步状态机、raw journal 写入、限速令牌的发送侧计数、gw 侧填帧模板。它依赖 `hft-venue-proto`,不依赖任何具体 venue。

**trait 为什么单独成 crate**:Rust 孤儿规则要求 `impl VenueProtocol for BinanceUsdm` 所在的 crate 至少拥有 trait 或类型之一——实现写在 `venue-binance` 里,它就必须能依赖到 trait。trait 若放在 `hft-venue`,`venue-*` 就要依赖 hot 运行时(连带 `hft-net`、`hft-journal`),纯协议 crate 不再是 pure,也无法被 `hft-py`、fuzz、`mock-exchange` 轻量复用;实现若反过来写在 `hft-venue` 里,运行时就要依赖全部 venue。独立的 pure crate 是唯一让两边都向下依赖的放法。
4. 编译期分派:`hft-core` 以泛型把具体 venue 装进运行时(无 `dyn`)。
5. 新交易所 = 新 `venue-<x>` 实现 `VenueProtocol` + 在 `hft-core` 装配线程([cookbook](../../../../docs/cookbook/adding-a-venue.md))。

**实现者契约**(写在 `hft-venue-proto`):每个 `venue-<x>` 的 `codes.rs` 显式列出确定拒绝、结果未知、限速三类错误码,**未登记的码一律按结果未知(`Unknown`)处理**,不默认 `Reject`([defensive-patterns](../../../../docs/defensive-patterns.md#未登记的错误码默认进-unknown不默认-reject))。

## Alternatives considered

**每个 venue 自带完整连接循环(现有 collector 的做法)。** 7 份循环、7 份重连与重同步 bug;热路径优化要做 7 遍。

**通用"交易所 SDK"层(ccxt 式统一抽象)。** 抽象到最小公倍数会丢掉各所的专有能力(SBE、WS 下单会话、死手接口),且引入运行期分派;[13 §5](../../../../design/13-设计评审与优化.md) 明确不做。

**采用 nautilus 的适配器。** 绑定其引擎与 tokio;只作参照实现([research/04 §8](../../../../design/research/04-oss-frameworks-and-patterns.md))。

**trait 放在 `hft-venue` 或 `hft-event`。** 前者让纯协议 crate 依赖 hot 运行时(见上);后者把一个只服务 venue 接入的 trait 塞进全系统共用的事件定义 crate,扩大 `hft-event` 的变更面,且 trait 的关联类型会把帧缓冲等运行时相关的类型拉进 L0。

## Acceptance criteria

- `cargo xtask deps`:`venue-*` 只依赖 `hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto` 与编解码白名单;不依赖 `hft-net`、`hft-venue`、`hft-sequencer`、`hft-rt`;`hft-venue` 不依赖任何 `venue-*`。
- 同一 `venue-binance` 解码入口被实盘 feed 循环、`mock-exchange`、`replayer` 的 raw 模式、金样测试与 fuzz 目标调用(调用点清单写在该 crate README)。
- 解码 < 1 µs/条;金样全绿;fuzz 24 h 无 panic([11 §2](../../../../design/11-实施路线图.md))。

## Risks

- `VenueProtocol` trait 设计过窄,某所的特殊行为(如 OKX 登录后才能订阅某些频道、Bybit DCP 主题)塞不进去:trait 以关联类型与 `const` 参数表达差异;第二家、第三家接入时允许修订 trait,并更新本 Note。
- 纯协议 crate 的签名/登录代码持有密钥材料的引用:密钥只在启动期由运行时传入,不存在 venue crate 的静态状态里。
