# Agent Note: 定点数按资产定指数,乘积用 i128

Status: proposed

## Problem

早期设计是"i64 + 编译期 scale"([13 R4](../../../../design/13-设计评审与优化.md),原 10 §4、07 §3)。单一全局 scale 覆盖不了我们的品种:BTC 数量步长 1e-8;PEPE/SHIB 类数量 1e11–1e12;BSC 代币价格可到 1e-10。i64 固定 1e-8 数量上限约 9.2e10,会溢出;固定 1e-9 价格对 1e-10 的代币只剩个位精度;名义 = px × qty 直接 i64 相乘必溢出。

## Proposal

归 [`hft-types`](../../../../crates/hft-types/README.md) 与 [`hft-refdata`](../../../../crates/hft-refdata/README.md):

1. **指数按资产**:同一资产(SymbolGroup)在所有 venue 用同一 `px_exp/qty_exp`,写在 refdata;跨所比较零换算。交易所的 tick/lot 是该指数下的整数步长。
2. `Px`、`Qty` 是 `#[repr(transparent)]` 的 i64 新类型,不暴露裸 i64 算术。
3. 名义与 PnL 用 i128,或以 USDT 1e-6 计的 i64 `Notional`;溢出显式处理(checked + 错误位)。
4. 链上 wei 用 U256,只在 `hft-amm`/`hft-chain-evm` 内部;进入 sequencer 时按资产指数换算一次。
5. refdata 加载时校验:同一资产的指数一致、tick/lot 能被指数整除,否则拒绝启动。

规则见 [hot-path-rules N1–N5](../../../../docs/hot-path-rules.md#数值n)。

## Alternatives considered

**全局编译期 scale。** 见 Problem:要么溢出、要么丢精度。

**每 venue 每合约各自指数。** 跨所比较(套利的核心运算)每次都要换算,且同一资产两边步长不同时容易出现"看起来相等其实不等"的错误。

**`rust_decimal` / 十进制浮点。** 128 位运算慢一个量级,且分配与分支更多;[research/04 §2.8](../../../../design/research/04-oss-frameworks-and-patterns.md) 的对比结论是定点 i64 足够。

**f64。** 舍入不确定,违反回放逐字节一致;下单路径禁浮点。

## Acceptance criteria

- 属性测试覆盖 BTC(1e-8 数量)、PEPE 类(1e12 数量)、BSC 1e-10 价格,名义/PnL 与 i128 十进制参考实现逐位相等,无溢出。
- refdata 加载对"同资产不同指数"的 blob 返回错误并拒绝。
- `hot-check` 证明下单路径 crate 无 `f32`/`f64`。

## Risks

- 资产指数一旦选定,改变它就是改语义:需要新 refdata 版本,旧 journal 按旧 blob 回放(journal 自足保证可行)。
- 极端新币可能超出所选指数的范围:refdata 生成时检查 `max_qty × 10^qty_exp < 2^63`,不满足则调整指数并记录。
