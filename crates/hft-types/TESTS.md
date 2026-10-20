---
component: hft-types
prefix: TYP
spec: README.md
---

# hft-types 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。约定:定点数在表中只写 raw 值,指数写在该用例的 `前置`;`MAX_RAW` = 4 611 686 018 427 387 903;Rate 的 1 bp = `Rate(100_000)`、1% = `Rate(10_000_000)`;"debug 构建"指 CI 的 `ci` 剖面(`debug-assertions = true`,[development 构建剖面](../../docs/development.md#构建剖面)),带 debug_assert 的用例以 `#[cfg_attr(debug_assertions, should_panic)]` 同时约束两种构建。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 定点数:按资产指数 | TYP-001–TYP-006, TYP-157 |
| 运算 | TYP-010–TYP-026 |
| 字符串 ↔ 定点(逐字节) | TYP-040–TYP-051 |
| 链上 U256 ↔ Qty(optional:链上组件使用) | TYP-070–TYP-072 |
| ID 与枚举 | TYP-080–TYP-086, TYP-005 |
| kill 字位 | TYP-100–TYP-105 |
| 模式、kill 级别、cap 来源与风控拒绝位 | TYP-110–TYP-114 |
| 对冲定价限值 | TYP-120–TYP-122 |
| 资金费结算时刻归一化 | TYP-130–TYP-131 |
| ClientOrderId 编解码 | TYP-135–TYP-142 |
| 布局规则(全仓 `#[repr(C)]` 结构适用) | TYP-001, TYP-005, TYP-086, TYP-150 |
| 错误 | TYP-155–TYP-157 |
| 实现说明 | TYP-160, TYP-051, TYP-070, TYP-102, TYP-105, TYP-023 |
| 延迟与资源契约 | TYP-170–TYP-180 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

### TYP-001 AssetExp 是 2 字节、两个 i8 字段依次位于偏移 0 与 1

- 规则:[定点数:按资产指数](README.md#定点数按资产指数)、[布局规则](README.md#布局规则全仓-reprc-结构适用)
- 级别:unit
- 里程碑:M1
- 前置:无参数
- 步骤:
  1. 读取 `size_of::<AssetExp>()`、`align_of::<AssetExp>()`、`offset_of!(AssetExp, px_exp)`、`offset_of!(AssetExp, qty_exp)`(同时以 `const _: () = assert!(…)` 编译期断言)
  2. `AssetExp::read_from_bytes(&[0xFC, 0xF8])`
- 期望:
  - 步骤 1 依次为 2、1、0、1
  - 步骤 2 返回 `AssetExp { px_exp: -4, qty_exp: -8 }`(任意两字节都可读出;取值范围由 hft-refdata 校验,本 crate 不拒绝)
- 实现:`crates/hft-types/tests/layout.rs::typ_001_asset_exp_layout`

### TYP-002 Px、Qty、Notional、Rate 是 8 字节透明 i64

- 规则:[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/layout.rs::typ_002_numeric_newtypes`

| 行 | 类型 | 期望 |
|---|---|---|
| TYP-002.1 | `Px` | `size_of == 8`、`align_of == 8`、`Px(-5).0 == -5` |
| TYP-002.2 | `Qty` | `size_of == 8`、`align_of == 8`、`Qty(-5).0 == -5` |
| TYP-002.3 | `Notional` | `size_of == 8`、`align_of == 8` |
| TYP-002.4 | `Rate` | `size_of == 8`、`align_of == 8` |

### TYP-003 MAX_RAW 与 Rate::MAX 的取值固定且两倍 MAX_RAW 不溢出 i64

- 规则:[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:unit
- 里程碑:M1
- 前置:无参数
- 步骤:
  1. 读取 `MAX_RAW`、`Rate::MAX`
  2. 计算 `MAX_RAW.checked_add(MAX_RAW)`、`(-MAX_RAW).checked_sub(MAX_RAW)`
- 期望:
  - `MAX_RAW == 4_611_686_018_427_387_903 == (1 << 62) - 1`;`Rate::MAX == Rate(i64::MAX)`
  - 步骤 2 依次为 `Some(9_223_372_036_854_775_806)`、`Some(-9_223_372_036_854_775_806)`
- 实现:`crates/hft-types/tests/layout.rs::typ_003_max_raw_constants`

### TYP-004 任意两个合法 Px 或 Qty 的加减在 i64 内不溢出

- 规则:[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:prop
- 里程碑:M1
- 性质:对 `a, b ∈ [-MAX_RAW, MAX_RAW]`,`Px(a) + Px(b)`、`Px(a) - Px(b)`、`Qty(a) + Qty(b)`、`Qty(a) - Qty(b)` 不 panic(`ci` 剖面开 overflow-checks),结果等于 i128 和差
- 生成:a、b 各取 {±MAX_RAW, ±(MAX_RAW − 1), 0, ±1} 与 `[-MAX_RAW, MAX_RAW]` 均匀值各半
- 参考:i128 加减
- 例数:PR 10 000
- 实现:`crates/hft-types/tests/prop_fixed.rs::typ_004_add_sub_never_overflow`

### TYP-005 被禁止的 API 不能编译

- 规则:[定点数:按资产指数](README.md#定点数按资产指数)、[ID 与枚举](README.md#id-与枚举)、[布局规则](README.md#布局规则全仓-reprc-结构适用)、[开发备忘](README.md#开发备忘)
- 级别:table
- 里程碑:M1
- 前置:`trybuild` 编译失败用例目录 `crates/hft-types/tests/ui/`;每行一个源文件
- 实现:`crates/hft-types/tests/compile_fail.rs::typ_005_forbidden_api_does_not_compile`

| 行 | 源码 | 期望 |
|---|---|---|
| TYP-005.1 | `Px(1).to_f64()` | 编译失败:方法不存在 |
| TYP-005.2 | `let x: f64 = Px(1).into();` | 编译失败:无 `From<Px> for f64` |
| TYP-005.3 | `Px(1) * Qty(1)` | 编译失败:未实现 `Mul` |
| TYP-005.4 | `Px(1) + Qty(1)` | 编译失败:无跨类型 `Add` |
| TYP-005.5 | `Qty(1) - Px(1)` | 编译失败:无跨类型 `Sub` |
| TYP-005.6 | `InstId::NONE` | 编译失败:关联常量不存在 |
| TYP-005.7 | `AssetId::NONE` | 编译失败:关联常量不存在 |
| TYP-005.8 | `fn f<T: zerocopy::FromBytes>() {} f::<Side>();` | 编译失败:线格式枚举只派生 `TryFromBytes` |
| TYP-005.9 | `assert_eq!(Px(1) + Px(2), Px(3))`(对照行) | 编译通过且断言成立 |

### TYP-006 同类型的加减、取负与比较

- 规则:[定点数:按资产指数](README.md#定点数按资产指数)、[开发备忘](README.md#开发备忘)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/fixed.rs::typ_006_same_type_ops`

| 行 | 表达式 | 期望 |
|---|---|---|
| TYP-006.1 | `Px(2) + Px(3)` | `Px(5)` |
| TYP-006.2 | `Px(2) - Px(3)` | `Px(-1)` |
| TYP-006.3 | `-Px(5)` | `Px(-5)` |
| TYP-006.4 | `Px(1) < Px(2)` | `true` |
| TYP-006.5 | `Qty(-3) + Qty(3)` | `Qty(0)` |
| TYP-006.6 | `Px(MAX_RAW) + Px(MAX_RAW)` | `Px(9_223_372_036_854_775_806)` |
| TYP-006.7 | `Px(-MAX_RAW) - Px(MAX_RAW)` | `Px(-9_223_372_036_854_775_806)` |

### TYP-010 mul_qty 的四种取整方向

- 规则:[运算](README.md#运算)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`e = AssetExp { px_exp: -4, qty_exp: -8 }`(积的指数 −12,换算到 `Notional` 除以 10^6);`px = Px(p)`、`q = Qty(1)`;每行分别以四种 `RoundMode` 调用,四个结果的 `FxErr` 都是 `OK`
- 实现:`crates/hft-types/tests/fixed.rs::typ_010_mul_qty_round_modes`

| 行 | p | Floor | Ceil | TowardZero | HalfEven |
|---|---|---|---|---|---|
| TYP-010.1 | 1_000_000 | 1 | 1 | 1 | 1 |
| TYP-010.2 | 1 | 0 | 1 | 0 | 0 |
| TYP-010.3 | 499_999 | 0 | 1 | 0 | 0 |
| TYP-010.4 | 500_000 | 0 | 1 | 0 | 0 |
| TYP-010.5 | 500_001 | 0 | 1 | 0 | 1 |
| TYP-010.6 | 1_500_000 | 1 | 2 | 1 | 2 |
| TYP-010.7 | 2_500_000 | 2 | 3 | 2 | 2 |
| TYP-010.8 | -1 | -1 | 0 | 0 | 0 |
| TYP-010.9 | -500_000 | -1 | 0 | 0 | 0 |
| TYP-010.10 | -1_500_000 | -2 | -1 | -1 | -2 |
| TYP-010.11 | -2_500_000 | -3 | -2 | -2 | -2 |
| TYP-010.12 | -2_500_001 | -3 | -2 | -2 | -3 |
| TYP-010.13 | 0 | 0 | 0 | 0 | 0 |

### TYP-011 mul_qty 在 i64 边界饱和并置 OVERFLOW,指数越界返回 EXP_RANGE

- 规则:[定点数:按资产指数](README.md#定点数按资产指数)、[运算](README.md#运算)
- 级别:table
- 里程碑:M1
- 前置:`m = TowardZero`;`e` 写作 `(px_exp, qty_exp)`
- 实现:`crates/hft-types/tests/fixed.rs::typ_011_mul_qty_saturation`

| 行 | px | q | e | 期望 `(Notional, FxErr)` |
|---|---|---|---|---|
| TYP-011.1 | 7 | 1_317_624_576_693_539_401 | (0, −6) | `(Notional(9_223_372_036_854_775_807), OK)`(恰为 i64::MAX) |
| TYP-011.2 | 7 | 1_317_624_576_693_539_400 | (0, −6) | `(Notional(9_223_372_036_854_775_800), OK)` |
| TYP-011.3 | 7 | 1_317_624_576_693_539_402 | (0, −6) | `(Notional(i64::MAX), OVERFLOW)` |
| TYP-011.4 | −7 | 1_317_624_576_693_539_401 | (0, −6) | `(Notional(-9_223_372_036_854_775_807), OK)` |
| TYP-011.5 | −4 | 2_305_843_009_213_693_952 | (0, −6) | `(Notional(-i64::MAX), OVERFLOW)`(积恰为 i64::MIN) |
| TYP-011.6 | MAX_RAW | MAX_RAW | (−4, −8) | `(Notional(i64::MAX), OVERFLOW)` |
| TYP-011.7 | MAX_RAW | −MAX_RAW | (−4, −8) | `(Notional(-i64::MAX), OVERFLOW)` |
| TYP-011.8 | 1 | 1 | (6, 6) | `(Notional(1_000_000_000_000_000_000), OK)`(防御路径:乘 10^18) |
| TYP-011.9 | 10 | 1 | (6, 6) | `(Notional(i64::MAX), OVERFLOW)` |
| TYP-011.10 | 1 | 1 | (−19, −8) | `(Notional(0), EXP_RANGE)` |
| TYP-011.11 | 1 | 1 | (−4, 7) | `(Notional(0), EXP_RANGE)` |
| TYP-011.12 | 0 | MAX_RAW | (−4, −8) | `(Notional(0), OK)` |

### TYP-012 mul_qty 与大整数参考实现逐值相同

- 规则:[运算](README.md#运算)、[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M1
- 性质:对合法指数对(两者 ∈ [−18, 6] 且和 ≤ −6)、`|px|, |q| ≤ MAX_RAW` 与四种 `RoundMode`,`mul_qty` 的结果等于参考:精确积按 m 取到 10^-6,`|r| ≤ i64::MAX` 时 `(r, OK)`,否则 `(±i64::MAX, OVERFLOW)`(同号)
- 生成:px、q 各一半取 `[-MAX_RAW, MAX_RAW]` 均匀值、一半取 BTC/PEPE 量级的对数均匀值;指数对在合法集合中均匀;m 均匀
- 参考:`crates/hft-types/tests/reference.rs` 的任意精度十进制实现
- 例数:PR 10 000
- 实现:`crates/hft-types/tests/prop_fixed.rs::typ_012_mul_qty_matches_reference`

### TYP-013 from_notional 求均价的取整、除零与溢出

- 规则:[运算](README.md#运算)
- 级别:table
- 里程碑:M1
- 前置:`e = (−4, −8)`(除注明外;均价 = n × 10^6 / q)
- 实现:`crates/hft-types/tests/fixed.rs::typ_013_from_notional`

| 行 | n | q | m | 期望 |
|---|---|---|---|---|
| TYP-013.1 | 5_000_000_000_000 | 5_000_000_000 | HalfEven | `Ok(Px(1_000_000_000))`(50 BTC 名义 5e6 USDT → 1e5) |
| TYP-013.2 | 1 | 3 | Floor | `Ok(Px(333_333))` |
| TYP-013.3 | 1 | 3 | Ceil | `Ok(Px(333_334))` |
| TYP-013.4 | 1 | 3 | HalfEven | `Ok(Px(333_333))` |
| TYP-013.5 | 5 | 10_000_000 | HalfEven | `Ok(Px(0))`(0.5 取偶) |
| TYP-013.6 | 15 | 10_000_000 | HalfEven | `Ok(Px(2))` |
| TYP-013.7 | 25 | 10_000_000 | HalfEven | `Ok(Px(2))` |
| TYP-013.8 | −15 | 10_000_000 | Floor | `Ok(Px(-2))` |
| TYP-013.9 | −15 | 10_000_000 | Ceil | `Ok(Px(-1))` |
| TYP-013.10 | −15 | 10_000_000 | TowardZero | `Ok(Px(-1))` |
| TYP-013.11 | −15 | 10_000_000 | HalfEven | `Ok(Px(-2))` |
| TYP-013.12 | 15 | −10_000_000 | HalfEven | `Ok(Px(-2))` |
| TYP-013.13 | 5 | 0 | HalfEven | `Err(FxErr::DIV_ZERO)` |
| TYP-013.14 | 4_611_686_018_427_387_903 | 1_000_000 | TowardZero | `Ok(Px(MAX_RAW))` |
| TYP-013.15 | 4_611_686_018_427_387_904 | 1_000_000 | TowardZero | `Err(FxErr::OVERFLOW)` |
| TYP-013.16 | −4_611_686_018_427_387_903 | 1_000_000 | TowardZero | `Ok(Px(-MAX_RAW))` |
| TYP-013.17 | i64::MAX | MAX_RAW,`e = (−18, −18)` | HalfEven | `Err(FxErr::OVERFLOW)`(n × 10^30 超出 i128,不 panic) |
| TYP-013.18 | 1 | 1,`e = (−19, −8)` | HalfEven | `Err(FxErr::EXP_RANGE)` |

### TYP-014 Px::mul_rate 的取整方向与饱和

- 规则:[运算](README.md#运算)、[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:table
- 里程碑:M1
- 前置:结果 = px × (10^9 + r) / 10^9 按 m 取整一次
- 实现:`crates/hft-types/tests/fixed.rs::typ_014_px_mul_rate`

| 行 | px | r | m | 期望 |
|---|---|---|---|---|
| TYP-014.1 | 1_000_000_000 | 100_000 | Floor | `Px(1_000_100_000)` |
| TYP-014.2 | 1_000_000_000 | −100_000 | Ceil | `Px(999_900_000)` |
| TYP-014.3 | 1 | 500_000_000 | Floor | `Px(1)` |
| TYP-014.4 | 1 | 500_000_000 | Ceil | `Px(2)` |
| TYP-014.5 | 1 | 500_000_000 | TowardZero | `Px(1)` |
| TYP-014.6 | 1 | 500_000_000 | HalfEven | `Px(2)` |
| TYP-014.7 | 5 | −500_000_000 | HalfEven | `Px(2)`(2.5 取偶) |
| TYP-014.8 | −1 | 500_000_000 | Floor | `Px(-2)` |
| TYP-014.9 | −1 | 500_000_000 | Ceil | `Px(-1)` |
| TYP-014.10 | −1 | 500_000_000 | TowardZero | `Px(-1)` |
| TYP-014.11 | −1 | 500_000_000 | HalfEven | `Px(-2)` |
| TYP-014.12 | 123 | 0 | Floor | `Px(123)` |
| TYP-014.13 | 123 | −1_000_000_000 | Floor | `Px(0)` |
| TYP-014.14 | 2_305_843_009_213_693_951 | 1_000_000_000 | Floor | `Px(4_611_686_018_427_387_902)` |
| TYP-014.15 | 2_305_843_009_213_693_952 | 1_000_000_000 | Floor | `Px(MAX_RAW)`(饱和) |
| TYP-014.16 | −2_305_843_009_213_693_952 | 1_000_000_000 | Floor | `Px(-MAX_RAW)`(饱和) |
| TYP-014.17 | MAX_RAW | i64::MAX | HalfEven | `Px(MAX_RAW)`(i128 中间量不溢出,不 panic) |

### TYP-015 Notional::mul_rate 计算费用的取整方向与饱和

- 规则:[运算](README.md#运算)、[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:table
- 里程碑:M1
- 前置:结果 = n × r / 10^9 按 m 取整一次
- 实现:`crates/hft-types/tests/fixed.rs::typ_015_notional_mul_rate`

| 行 | n | r | m | 期望 |
|---|---|---|---|---|
| TYP-015.1 | 1_000_000_000 | 200_000 | Ceil | `Notional(200_000)`(1000 USDT × 2 bp = 0.2 USDT) |
| TYP-015.2 | 1_000_001 | 375_000 | Ceil | `Notional(376)`(375.000375 向上) |
| TYP-015.3 | 1_000_001 | 375_000 | Floor | `Notional(375)` |
| TYP-015.4 | 1 | 500_000_000 | Floor | `Notional(0)` |
| TYP-015.5 | 1 | 500_000_000 | Ceil | `Notional(1)` |
| TYP-015.6 | 1 | 500_000_000 | HalfEven | `Notional(0)` |
| TYP-015.7 | −3 | 500_000_000 | Floor | `Notional(-2)` |
| TYP-015.8 | −3 | 500_000_000 | Ceil | `Notional(-1)` |
| TYP-015.9 | −3 | 500_000_000 | TowardZero | `Notional(-1)` |
| TYP-015.10 | −3 | 500_000_000 | HalfEven | `Notional(-2)` |
| TYP-015.11 | i64::MAX | 1_000_000_000 | Floor | `Notional(i64::MAX)`(恰为边界) |
| TYP-015.12 | i64::MAX | 2_000_000_000 | Floor | `Notional(i64::MAX)`(饱和) |
| TYP-015.13 | i64::MAX | −2_000_000_000 | Floor | `Notional(-i64::MAX)`(饱和) |

### TYP-016 round_to_tick 的四种取整方向与 MAX_RAW 处的饱和

- 规则:[运算](README.md#运算)、[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:table
- 里程碑:M1
- 前置:`tick = 10`;每行分别以四种 `RoundMode` 调用 `Px(px).round_to_tick(10, m)`
- 实现:`crates/hft-types/tests/fixed.rs::typ_016_round_to_tick`

| 行 | px | Floor | Ceil | TowardZero | HalfEven |
|---|---|---|---|---|---|
| TYP-016.1 | 123 | 120 | 130 | 120 | 120 |
| TYP-016.2 | 125 | 120 | 130 | 120 | 120 |
| TYP-016.3 | 135 | 130 | 140 | 130 | 140 |
| TYP-016.4 | 120 | 120 | 120 | 120 | 120 |
| TYP-016.5 | 0 | 0 | 0 | 0 | 0 |
| TYP-016.6 | −123 | −130 | −120 | −120 | −120 |
| TYP-016.7 | −125 | −130 | −120 | −120 | −120 |
| TYP-016.8 | −135 | −140 | −130 | −130 | −140 |
| TYP-016.9 | 4_611_686_018_427_387_891 | 4_611_686_018_427_387_890 | 4_611_686_018_427_387_900 | 4_611_686_018_427_387_890 | 4_611_686_018_427_387_890 |
| TYP-016.10 | 4_611_686_018_427_387_901 | 4_611_686_018_427_387_900 | 4_611_686_018_427_387_900(饱和) | 4_611_686_018_427_387_900 | 4_611_686_018_427_387_900 |
| TYP-016.11 | −4_611_686_018_427_387_901 | −4_611_686_018_427_387_900(饱和) | −4_611_686_018_427_387_900 | −4_611_686_018_427_387_900 | −4_611_686_018_427_387_900 |

### TYP-017 非正的 tick 或 lot 得到 0 而不是原值

- 规则:[运算](README.md#运算)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/fixed.rs::typ_017_nonpositive_step`

| 行 | 调用 | 期望 |
|---|---|---|
| TYP-017.1 | `Px(123).round_to_tick(0, Floor)` | `Px(0)` |
| TYP-017.2 | `Px(123).round_to_tick(-10, Ceil)` | `Px(0)` |
| TYP-017.3 | `Px(123).round_to_tick(1, HalfEven)` | `Px(123)`(对照行) |
| TYP-017.4 | `Qty(1999).round_to_lot(0, TowardZero)` | `Qty(0)` |
| TYP-017.5 | `Qty(1999).round_to_lot(-5, TowardZero)` | `Qty(0)` |
| TYP-017.6 | `Qty(1999).split_lot(0)` | `(Qty(0), Qty(1999))` |
| TYP-017.7 | `Qty(-1999).split_lot(-1000)` | `(Qty(0), Qty(-1999))` |

### TYP-018 round_to_lot 的四种取整方向与饱和

- 规则:[运算](README.md#运算)、[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:table
- 里程碑:M1
- 前置:`lot = 1000`;每行分别以四种 `RoundMode` 调用
- 实现:`crates/hft-types/tests/fixed.rs::typ_018_round_to_lot`

| 行 | q | Floor | Ceil | TowardZero | HalfEven |
|---|---|---|---|---|---|
| TYP-018.1 | 1999 | 1000 | 2000 | 1000 | 2000 |
| TYP-018.2 | −1999 | −2000 | −1000 | −1000 | −2000 |
| TYP-018.3 | 1500 | 1000 | 2000 | 1000 | 2000 |
| TYP-018.4 | 2500 | 2000 | 3000 | 2000 | 2000 |
| TYP-018.5 | −2500 | −3000 | −2000 | −2000 | −2000 |
| TYP-018.6 | 0 | 0 | 0 | 0 | 0 |
| TYP-018.7 | MAX_RAW | 4_611_686_018_427_387_000 | 4_611_686_018_427_387_000(饱和) | 4_611_686_018_427_387_000 | 4_611_686_018_427_387_000(饱和) |

### TYP-019 split_lot 把数量拆成可下单部分与同号余数

- 规则:[运算](README.md#运算)
- 级别:table
- 里程碑:M1
- 前置:`lot = 1000`
- 实现:`crates/hft-types/tests/fixed.rs::typ_019_split_lot`

| 行 | q | 期望 `(可下单, 余数)` |
|---|---|---|
| TYP-019.1 | 1999 | `(Qty(1000), Qty(999))` |
| TYP-019.2 | −1999 | `(Qty(-1000), Qty(-999))` |
| TYP-019.3 | 999 | `(Qty(0), Qty(999))` |
| TYP-019.4 | 2000 | `(Qty(2000), Qty(0))` |
| TYP-019.5 | 0 | `(Qty(0), Qty(0))` |
| TYP-019.6 | MAX_RAW | `(Qty(4_611_686_018_427_387_000), Qty(903))` |

### TYP-020 取到步长的结果是步长整数倍、方向不变量成立、拆分可还原

- 规则:[运算](README.md#运算)
- 级别:prop
- 里程碑:M1
- 性质:对 `tick ∈ [1, 10^12]`、`|px| ≤ MAX_RAW`、四种 m:结果是 tick 整数倍、`|结果| ≤ MAX_RAW`;未饱和时 `|结果 − px| < tick`,Floor 结果 ≤ px、Ceil 结果 ≥ px(即 `side.price_round()` 买价不高于输入、卖价不低于输入);HalfEven 与参考相同;`split_lot(lot)` 的 `(a, r)` 满足 `a + r == q`、`a % lot == 0`、`|r| < lot`、`r == 0` 或与 q 同号;`round_to_lot` 同 `round_to_tick` 的性质
- 生成:tick/lot 取 10 的幂与 `[1, 10^12]` 均匀值各半;px/q 取 `[-MAX_RAW, MAX_RAW]` 均匀值与 ±MAX_RAW 附近 ±10^13 内的值各半
- 参考:i128 实现的四种取整除法
- 例数:PR 10 000
- 实现:`crates/hft-types/tests/prop_fixed.rs::typ_020_rounding_invariants`

### TYP-021 mid_floor 向负无穷取整且在边界不溢出

- 规则:[运算](README.md#运算)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/fixed.rs::typ_021_mid_floor`

| 行 | a | b | 期望 |
|---|---|---|---|
| TYP-021.1 | 3 | 4 | `Px(3)` |
| TYP-021.2 | 4 | 4 | `Px(4)` |
| TYP-021.3 | −3 | −4 | `Px(-4)` |
| TYP-021.4 | −3 | 4 | `Px(0)` |
| TYP-021.5 | −4 | 3 | `Px(-1)` |
| TYP-021.6 | MAX_RAW | MAX_RAW | `Px(MAX_RAW)` |
| TYP-021.7 | MAX_RAW | MAX_RAW − 1 | `Px(4_611_686_018_427_387_902)` |
| TYP-021.8 | −MAX_RAW | −MAX_RAW | `Px(-MAX_RAW)` |
| TYP-021.9 | MAX_RAW | −MAX_RAW | `Px(0)` |
| TYP-021.10 | i64::MAX | i64::MAX | `Px(i64::MAX)`(违反不变量 1 的输入也不 panic) |

### TYP-022 ratio 按 HalfEven 求相对差,分母非正得 0,超界饱和

- 规则:[运算](README.md#运算)、[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:table
- 里程碑:M1
- 前置:无参数;调用 `Px(a).ratio(Px(b))`
- 实现:`crates/hft-types/tests/fixed.rs::typ_022_ratio`

| 行 | a | b | 期望 |
|---|---|---|---|
| TYP-022.1 | 1_000_100_000 | 1_000_000_000 | `Rate(100_000)` |
| TYP-022.2 | 999_900_000 | 1_000_000_000 | `Rate(-100_000)` |
| TYP-022.3 | 5 | 5 | `Rate(0)` |
| TYP-022.4 | 2 | 3 | `Rate(-333_333_333)` |
| TYP-022.5 | 2_000_000_001 | 2_000_000_000 | `Rate(0)`(0.5 取偶) |
| TYP-022.6 | 2_000_000_003 | 2_000_000_000 | `Rate(2)`(1.5 取偶) |
| TYP-022.7 | 1_999_999_999 | 2_000_000_000 | `Rate(0)` |
| TYP-022.8 | 1_999_999_997 | 2_000_000_000 | `Rate(-2)` |
| TYP-022.9 | 100 | 0 | `Rate(0)` |
| TYP-022.10 | 100 | −5 | `Rate(0)` |
| TYP-022.11 | MAX_RAW | 1 | `Rate(i64::MAX)`(饱和) |
| TYP-022.12 | −MAX_RAW | 1 | `Rate(-i64::MAX)`(饱和) |

### TYP-023 pair_lot 给出两腿 lot 对齐的最小第一腿单位

- 规则:[运算](README.md#运算)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:调用 `pair_lot(lot_f, lot_h, num, den)`
- 实现:`crates/hft-types/tests/fixed.rs::typ_023_pair_lot`

| 行 | lot_f | lot_h | num | den | 期望 |
|---|---|---|---|---|---|
| TYP-023.1 | 1 | 10 | 1 | 1 | `Some(Qty(10))` |
| TYP-023.2 | 10 | 1 | 1 | 1 | `Some(Qty(10))` |
| TYP-023.3 | 4 | 10 | 1 | 1 | `Some(Qty(20))` |
| TYP-023.4 | 7 | 7 | 1 | 1 | `Some(Qty(7))` |
| TYP-023.5 | 6 | 4 | 1 | 1 | `Some(Qty(12))` |
| TYP-023.6 | 1 | 5 | 1 | 2 | `Some(Qty(10))`(第二腿 5,恰 1 lot) |
| TYP-023.7 | 3 | 5 | 1 | 2 | `Some(Qty(30))` |
| TYP-023.8 | 1 | 5 | 2 | 1 | `Some(Qty(5))`(第二腿 10,恰 2 lot) |
| TYP-023.9 | 4 | 6 | 3 | 2 | `Some(Qty(4))`(第二腿 6,恰 1 lot) |
| TYP-023.10 | 1 | 1 | 4_294_967_295 | 4_294_967_295 | `Some(Qty(1))` |
| TYP-023.11 | 1 | MAX_RAW | 1 | 1 | `Some(Qty(MAX_RAW))` |
| TYP-023.12 | 2 | 2_305_843_009_213_693_951 | 1 | 1 | `Some(Qty(4_611_686_018_427_387_902))` |
| TYP-023.13 | 3 | 2_305_843_009_213_693_951 | 1 | 1 | `None`(结果超过 MAX_RAW) |
| TYP-023.14 | 2_147_483_647 | 2_305_843_009_213_693_951 | 1 | 1 | `None`(两素数之积) |
| TYP-023.15 | 1 | 4_611_686_018_427_387_904 | 1 | 1 | `None` |
| TYP-023.16 | 0 | 10 | 1 | 1 | `None` |
| TYP-023.17 | 10 | 0 | 1 | 1 | `None` |
| TYP-023.18 | −1 | 10 | 1 | 1 | `None` |
| TYP-023.19 | 10 | −1 | 1 | 1 | `None` |
| TYP-023.20 | 1 | 1 | 0 | 1 | `None` |
| TYP-023.21 | 1 | 1 | 1 | 0 | `None` |

### TYP-024 pair_lot 与暴力搜索参考逐值相同

- 规则:[运算](README.md#运算)
- 级别:prop
- 里程碑:M1
- 性质:`pair_lot` 返回的 q 是 `lot_f` 的整数倍、`q × num` 被 `den × lot_h` 整除,且是满足这两条的最小正数;q ≤ MAX_RAW 时为 `Some(q)`
- 生成:`lot_f, lot_h ∈ [1, 10^6]`;`num, den ∈ [1, 1000]`
- 参考:按 `lot_f` 的倍数逐个尝试的暴力搜索(倍数 ≤ 10 000 时);更大的倍数 k 不逐个尝试,改为断言 `lot_f × k` 合法且对 k 的每个素因子 p,`lot_f × k / p` 不合法(合法倍数对 gcd 封闭,故 k 最小)
- 例数:PR 2 000
- 实现:`crates/hft-types/tests/prop_fixed.rs::typ_024_pair_lot_matches_brute_force`

### TYP-025 Side 的取整方向、符号与反向

- 规则:[运算](README.md#运算)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/fixed.rs::typ_025_side_helpers`

| 行 | 表达式 | 期望 |
|---|---|---|
| TYP-025.1 | `Side::Buy.price_round()` | `RoundMode::Floor` |
| TYP-025.2 | `Side::Sell.price_round()` | `RoundMode::Ceil` |
| TYP-025.3 | `Side::Buy.sign()` | `1` |
| TYP-025.4 | `Side::Sell.sign()` | `-1` |
| TYP-025.5 | `Side::Buy.opposite()` | `Side::Sell` |
| TYP-025.6 | `Side::Sell.opposite()` | `Side::Buy` |
| TYP-025.7 | `const R: RoundMode = Side::Buy.price_round();` | 编译通过,`R == RoundMode::Floor` |

### TYP-026 RoundMode 的判别值

- 规则:[运算](README.md#运算)
- 级别:table
- 里程碑:M1
- 前置:读取 `m as u8`
- 实现:`crates/hft-types/tests/codes.rs::typ_026_round_mode_values`

| 行 | 变体 | 期望 |
|---|---|---|
| TYP-026.1 | `RoundMode::Floor` | 1 |
| TYP-026.2 | `RoundMode::Ceil` | 2 |
| TYP-026.3 | `RoundMode::TowardZero` | 3 |
| TYP-026.4 | `RoundMode::HalfEven` | 4 |

### TYP-040 parse_exact 拒绝语法外的输入

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)
- 级别:table
- 里程碑:M1
- 前置:`exp = -4`;同一输入也以 `parse_round(…, -4, HalfEven)` 调用,结果相同
- 实现:`crates/hft-types/tests/parse.rs::typ_040_parse_syntax_errors`

| 行 | 输入(字节) | 期望 |
|---|---|---|
| TYP-040.1 | `""` | `Err(EMPTY)` |
| TYP-040.2 | `"+1"` | `Err(SYNTAX)` |
| TYP-040.3 | `".5"` | `Err(SYNTAX)` |
| TYP-040.4 | `"5."` | `Err(SYNTAX)` |
| TYP-040.5 | `"1e"` | `Err(SYNTAX)` |
| TYP-040.6 | `"1e1234"`(指数 4 位) | `Err(SYNTAX)` |
| TYP-040.7 | `"--1"` | `Err(SYNTAX)` |
| TYP-040.8 | `"1.2.3"` | `Err(SYNTAX)` |
| TYP-040.9 | `"NaN"` | `Err(SYNTAX)` |
| TYP-040.10 | `"Infinity"` | `Err(SYNTAX)` |
| TYP-040.11 | `"null"` | `Err(SYNTAX)` |
| TYP-040.12 | `" 1"` | `Err(SYNTAX)` |
| TYP-040.13 | `"1 "` | `Err(SYNTAX)` |
| TYP-040.14 | `"1,000"` | `Err(SYNTAX)` |
| TYP-040.15 | `"1_000"` | `Err(SYNTAX)` |
| TYP-040.16 | `"0x10"` | `Err(SYNTAX)` |
| TYP-040.17 | `"-"` | `Err(SYNTAX)` |
| TYP-040.18 | `"e5"` | `Err(SYNTAX)` |
| TYP-040.19 | `"1e+"` | `Err(SYNTAX)` |
| TYP-040.20 | `"1.e5"` | `Err(SYNTAX)` |
| TYP-040.21 | `"-.5"` | `Err(SYNTAX)` |
| TYP-040.22 | `"1e5.0"` | `Err(SYNTAX)` |
| TYP-040.23 | `"1ee5"` | `Err(SYNTAX)` |
| TYP-040.24 | `[0xD9, 0xA1]`(阿拉伯数字 1) | `Err(SYNTAX)` |
| TYP-040.25 | `[0x31, 0x00]` | `Err(SYNTAX)` |
| TYP-040.26 | `"1."` 后接 39 个 `"0"`(41 字节) | `Err(SYNTAX)` |

### TYP-041 parse_exact 接受的写法与超精度

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)
- 级别:table
- 里程碑:M1
- 前置:`exp = -4`
- 实现:`crates/hft-types/tests/parse.rs::typ_041_parse_exact_values`

| 行 | 输入 | 期望 |
|---|---|---|
| TYP-041.1 | `"0"` | `Ok(0)` |
| TYP-041.2 | `"-0"` | `Ok(0)` |
| TYP-041.3 | `"-0.000"` | `Ok(0)` |
| TYP-041.4 | `"12.5"` | `Ok(125_000)` |
| TYP-041.5 | `"00012.5"` | `Ok(125_000)` |
| TYP-041.6 | `"-12.5"` | `Ok(-125_000)` |
| TYP-041.7 | `"0.0001"` | `Ok(1)` |
| TYP-041.8 | `"0.00010"` | `Ok(1)` |
| TYP-041.9 | `"0.10000000"` | `Ok(1_000)` |
| TYP-041.10 | `"0.00012345"` | `Err(PRECISION)`(不截断) |
| TYP-041.11 | `"1.0E-4"` | `Ok(1)` |
| TYP-041.12 | `"0E-8"` | `Ok(0)` |
| TYP-041.13 | `"1e3"` | `Ok(10_000_000)` |
| TYP-041.14 | `"1E+3"` | `Ok(10_000_000)` |
| TYP-041.15 | `"1e004"` | `Ok(100_000_000)` |
| TYP-041.16 | `"12345e-4"` | `Ok(12_345)` |
| TYP-041.17 | `"1e-5"` | `Err(PRECISION)` |
| TYP-041.18 | `"1.5e-4"` | `Err(PRECISION)` |
| TYP-041.19 | `"1e999"` | `Err(OVERFLOW)` |
| TYP-041.20 | `"0e999"` | `Ok(0)` |
| TYP-041.21 | `"-1e-999"` | `Err(PRECISION)` |
| TYP-041.22 | `"1."` 后接 38 个 `"0"`(恰 40 字节) | `Ok(10_000)` |
| TYP-041.23 | 27 个 `"0"` 后接 `"1"`(28 字节) | `Ok(10_000)` |

### TYP-042 parse_exact 的范围边界、有效位与指数范围

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)、[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:table
- 里程碑:M1
- 前置:每行给出 `exp`
- 实现:`crates/hft-types/tests/parse.rs::typ_042_parse_exact_bounds`

| 行 | 输入 | exp | 期望 |
|---|---|---|---|
| TYP-042.1 | `"4611686018427387903"` | 0 | `Ok(MAX_RAW)` |
| TYP-042.2 | `"4611686018427387904"` | 0 | `Err(OVERFLOW)` |
| TYP-042.3 | `"4611686018427387902"` | 0 | `Ok(4_611_686_018_427_387_902)` |
| TYP-042.4 | `"-4611686018427387903"` | 0 | `Ok(-MAX_RAW)` |
| TYP-042.5 | `"-4611686018427387904"` | 0 | `Err(OVERFLOW)` |
| TYP-042.6 | `"46116860184.27387903"` | −8 | `Ok(MAX_RAW)` |
| TYP-042.7 | `"46116860184.27387904"` | −8 | `Err(OVERFLOW)` |
| TYP-042.8 | `"12345678901234567891"`(20 位有效数字) | 0 | `Err(OVERFLOW)` |
| TYP-042.9 | `"0.12345678901234567891"` | −4 | `Err(OVERFLOW)`(有效位判定先于超精度) |
| TYP-042.10 | `"4611686018427387903000000"` | 6 | `Ok(MAX_RAW)`(整数部尾零不计有效位) |
| TYP-042.11 | `"1000000000000000000000"` | 3 | `Ok(1_000_000_000_000_000_000)` |
| TYP-042.12 | `"1.000000000000000000000"` | 0 | `Ok(1)` |
| TYP-042.13 | `"1"` | −30 | `Err(OVERFLOW)` |
| TYP-042.14 | `"0.000000000000000000000000000001"` | −30 | `Ok(1)` |
| TYP-042.15 | `"1"` | −31 | `Err(EXP_RANGE)` |
| TYP-042.16 | `"1"` | 19 | `Err(EXP_RANGE)` |
| TYP-042.17 | `"1000000000000000000"` | 18 | `Ok(1)` |
| TYP-042.18 | `"1"` | 18 | `Err(PRECISION)` |
| TYP-042.19 | `""` | 19 | `Err(EXP_RANGE)`(先于 EMPTY) |
| TYP-042.20 | `"x"` | −31 | `Err(EXP_RANGE)`(先于 SYNTAX) |

### TYP-043 线格式指数把合约单位与价格平移直接换成资产单位

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:每行给出线格式指数(资产指数 + 合约平移)
- 实现:`crates/hft-types/tests/parse.rs::typ_043_wire_exponent`

| 行 | 输入 | exp | 期望 |
|---|---|---|---|
| TYP-043.1 | `"0.0123456"`(1000PEPEUSDT 价格) | −9 | `Ok(12_345_600)`(资产 px_exp = −12) |
| TYP-043.2 | `"5"`(1000PEPEUSDT 数量 5 张) | −6 | `Ok(5_000_000)`(5000 枚,资产 qty_exp = −3) |
| TYP-043.3 | `"100000"`(BTC 典型价) | −4 | `Ok(1_000_000_000)` |
| TYP-043.4 | `"10000000"`(BTC 价上限) | −4 | `Ok(100_000_000_000)` |
| TYP-043.5 | `"0.00001"`(BTC lot) | −8 | `Ok(1_000)` |
| TYP-043.6 | `"1000000000000000000000"`(供应 1e21 枚) | 3 | `Ok(1_000_000_000_000_000_000)` |

### TYP-044 parse_round 按 RoundMode 取整,极端指数部按精确值取整

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)
- 级别:table
- 里程碑:M1
- 前置:每行给出 `exp` 与 `m`
- 实现:`crates/hft-types/tests/parse.rs::typ_044_parse_round`

| 行 | 输入 | exp | m | 期望 |
|---|---|---|---|---|
| TYP-044.1 | `"1.005"` | −2 | Floor | `Ok(100)` |
| TYP-044.2 | `"1.005"` | −2 | Ceil | `Ok(101)` |
| TYP-044.3 | `"1.005"` | −2 | TowardZero | `Ok(100)` |
| TYP-044.4 | `"1.005"` | −2 | HalfEven | `Ok(100)` |
| TYP-044.5 | `"1.015"` | −2 | HalfEven | `Ok(102)` |
| TYP-044.6 | `"-1.005"` | −2 | Floor | `Ok(-101)` |
| TYP-044.7 | `"-1.005"` | −2 | Ceil | `Ok(-100)` |
| TYP-044.8 | `"-1.005"` | −2 | TowardZero | `Ok(-100)` |
| TYP-044.9 | `"-1.005"` | −2 | HalfEven | `Ok(-100)` |
| TYP-044.10 | `"-1.015"` | −2 | HalfEven | `Ok(-102)` |
| TYP-044.11 | `"1.0050000001"` | −2 | HalfEven | `Ok(101)` |
| TYP-044.12 | `"1.00499999"` | −2 | HalfEven | `Ok(100)` |
| TYP-044.13 | `"0.001"` | −2 | Ceil | `Ok(1)` |
| TYP-044.14 | `"0.001"` | −2 | Floor | `Ok(0)` |
| TYP-044.15 | `"-0.001"` | −2 | Floor | `Ok(-1)` |
| TYP-044.16 | `"-0.001"` | −2 | Ceil | `Ok(0)` |
| TYP-044.17 | `"0.00012345"`(资金费率) | −9 | HalfEven | `Ok(123_450)` |
| TYP-044.18 | `"-0.000375"` | −9 | HalfEven | `Ok(-375_000)` |
| TYP-044.19 | `"1.0E-4"` | −9 | HalfEven | `Ok(100_000)`(1 bp) |
| TYP-044.20 | `"0.12345678901234567891"` | −4 | HalfEven | `Ok(1_235)`(第 20 位起只参与取整) |
| TYP-044.21 | `"2.5"` | 0 | HalfEven | `Ok(2)` |
| TYP-044.22 | `"3.5"` | 0 | HalfEven | `Ok(4)` |
| TYP-044.23 | `"4611686018427387903.4"` | 0 | Floor | `Ok(MAX_RAW)` |
| TYP-044.24 | `"4611686018427387903.4"` | 0 | Ceil | `Err(OVERFLOW)` |
| TYP-044.25 | `"1e999"` | −4 | Floor | `Err(OVERFLOW)` |
| TYP-044.26 | `"1e-999"` | −4 | Ceil | `Ok(1)` |
| TYP-044.27 | `"1e-999"` | −4 | Floor | `Ok(0)` |
| TYP-044.28 | `"-1e-999"` | −4 | Floor | `Ok(-1)` |
| TYP-044.29 | `"1"` | −31 | HalfEven | `Err(EXP_RANGE)` |

### TYP-045 parse_exact 与 parse_round 与任意精度参考逐值相同

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)
- 级别:prop
- 里程碑:M1
- 性质:对语法内的任意十进制串与 `exp ∈ [-30, 18]`、四种 m,`parse_round` 等于"精确值按 m 取到 10^exp,超过 MAX_RAW 为 OVERFLOW";`parse_exact` 按 README 第 7 条的先后返回同一个错误位或同一个值;语法外的串两者都返回 `SYNTAX`(空串 `EMPTY`)
- 生成:符号、0–25 位整数部(含前导零与尾零)、可选 0–25 位小数部、可选 `e/E` 指数(0–999,三种符号写法),总长 ≤ 40 字节;另 10% 为随机字节串
- 参考:`crates/hft-types/tests/reference.rs` 的任意精度十进制解析(逐字符,不经 f64)
- 例数:PR 10 000
- 实现:`crates/hft-types/tests/prop_parse.rs::typ_045_parse_matches_reference`

### TYP-046 write_fixed 输出最短十进制写法

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`out = [0xAA; 40]`;期望写出的字节为 `out[..n]`,`n` 为返回值
- 实现:`crates/hft-types/tests/format.rs::typ_046_write_fixed`

| 行 | raw | exp | 期望 |
|---|---|---|---|
| TYP-046.1 | 0 | −4 | `"0"`,n = 1 |
| TYP-046.2 | 1 | −4 | `"0.0001"` |
| TYP-046.3 | −1 | −4 | `"-0.0001"` |
| TYP-046.4 | 10_000 | −4 | `"1"` |
| TYP-046.5 | 125_000 | −4 | `"12.5"` |
| TYP-046.6 | −125_000 | −4 | `"-12.5"` |
| TYP-046.7 | 1_000 | −4 | `"0.1"` |
| TYP-046.8 | 12_340 | −4 | `"1.234"`(tick 10 的整数倍,恰 3 位小数) |
| TYP-046.9 | 5 | 3 | `"5000"` |
| TYP-046.10 | MAX_RAW | 0 | `"4611686018427387903"` |
| TYP-046.11 | −MAX_RAW | −8 | `"-46116860184.27387903"` |
| TYP-046.12 | MAX_RAW | 6 | `"4611686018427387903000000"` |
| TYP-046.13 | 1 | −18 | `"0.000000000000000001"` |
| TYP-046.14 | 1 | −30 | `"0."` + 29 个 `"0"` + `"1"`,n = 32 |
| TYP-046.15 | −1 | −30 | `"-0."` + 29 个 `"0"` + `"1"`,n = 33 |
| TYP-046.16 | i64::MAX | 18 | `"9223372036854775807"` + 18 个 `"0"`,n = 37 |
| TYP-046.17 | i64::MIN | 18 | `"-9223372036854775808"` + 18 个 `"0"`,n = 38 |
| TYP-046.18 | 1 | 19 | n = 0,`out` 仍为 40 个 0xAA |
| TYP-046.19 | 1 | −31 | n = 0,`out` 仍为 40 个 0xAA |

### TYP-047 write_fixed 与 parse_exact 往返

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)
- 级别:prop
- 里程碑:M1
- 性质:对 `|x| ≤ MAX_RAW`、`e ∈ [-30, 18]`:`parse_exact(write_fixed(x, e), e) == Ok(x)`;输出匹配 `^-?(0|[1-9][0-9]*)(\.[0-9]*[1-9])?$`、长度 ≤ 38;`x = 0` 时输出 `"0"`(不出现 `"-0"`)
- 生成:x 取 `[-MAX_RAW, MAX_RAW]` 均匀值、10 的幂及其 ±1 各三分之一;e 在 `[-30, 18]` 均匀
- 参考:断言本身(往返恒等与正则)
- 例数:PR 10 000
- 实现:`crates/hft-types/tests/prop_parse.rs::typ_047_write_parse_round_trip`

### TYP-048 tick 整数倍的值写出的小数位不超过 tick 的小数位

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)
- 级别:prop
- 里程碑:M1
- 性质:对 `tick = 10^t`(`t ∈ [0, 12]`)、raw 为 tick 整数倍、`e ∈ [-18, 6]`:`write_fixed(raw, e)` 的小数位数 ≤ `max(0, -(e + t))`
- 生成:t、e 均匀;raw = tick × k,`|k| ≤ MAX_RAW / tick` 均匀
- 参考:按 t、e 直接计算的位数上限
- 例数:PR 5 000
- 实现:`crates/hft-types/tests/prop_parse.rs::typ_048_tick_multiple_decimals`

### TYP-049 core venue 的真实价格与数量字符串解析结果固定

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)
- 级别:golden
- 里程碑:M3
- 输入:从 6 个 core 产品线(Binance、OKX、Bitget 的现货与 USDT 永续)的 raw journal 抽取的价格、数量、费率与资金费率原文,每产品线每类字段至少 200 条(含科学计数法写法,若出现),连同各 inst 的线格式指数
- 期望文件:`fixtures/decimal/<venue>.tsv`(每行:原文、线格式指数、`parse_exact` 期望 raw 或错误位名、`parse_round(HalfEven)` 期望 raw)
- 实现:`crates/hft-types/tests/golden.rs::typ_049_venue_decimal_samples`
- 当前覆盖:Binance/Bitget 四产品的 demo 公共流与 Bitget 私有费用数量子集;逐字解析结果由独立 Decimal 参考计算,来源行映射与分类计数随样本记录。该测试通过只证明已采样行的解析;OKX、每类 200 条与 raw journal 来源的完整验收仍待补,不得据此标记 TYP-049 完成。

### TYP-050 十进制解析的 fuzz 目标不崩溃且与参考一致

- 规则:[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)、[错误](README.md#错误)
- 级别:fuzz
- 里程碑:M1
- 目标:`crates/hft-types/fuzz/fuzz_targets/parse.rs`(任意字节 + 一个指数字节 → `parse_exact` 与 `parse_round` 四种 m)
- 语料:TYP-040–TYP-044 各行输入;TYP-049 金样到位后加入其原文
- 判定:无 panic、无越界;`Ok` 结果 `|raw| ≤ MAX_RAW`;与参考实现结果不同视同崩溃(差分目标);单输入超时口径见 [testing §3.3](../../docs/testing.md#33-fuzz)
- 实现:`crates/hft-types/fuzz/fuzz_targets/parse.rs::typ_050_fuzz_parse`

### TYP-051 SWAR 分块边界上的数字串

- 规则:[实现说明](README.md#实现说明)、[字符串 ↔ 定点(逐字节)](README.md#字符串--定点逐字节)
- 级别:table
- 里程碑:M1
- 前置:每行给出 `exp`;输入长度跨越 8 字节分块边界
- 实现:`crates/hft-types/tests/parse.rs::typ_051_swar_chunk_boundaries`

| 行 | 输入 | exp | 期望 |
|---|---|---|---|
| TYP-051.1 | `"1234567"` | 0 | `Ok(1_234_567)` |
| TYP-051.2 | `"12345678"` | 0 | `Ok(12_345_678)` |
| TYP-051.3 | `"123456789"` | 0 | `Ok(123_456_789)` |
| TYP-051.4 | `"1234567890123456"` | 0 | `Ok(1_234_567_890_123_456)` |
| TYP-051.5 | `"12345678901234567"` | 0 | `Ok(12_345_678_901_234_567)` |
| TYP-051.6 | `"1234567:"`(0x3A 紧邻 `'9'`) | 0 | `Err(SYNTAX)` |
| TYP-051.7 | `"1234567/"`(0x2F 紧邻 `'0'`) | 0 | `Err(SYNTAX)` |
| TYP-051.8 | `"12345678.12345678"` | −8 | `Ok(1_234_567_812_345_678)` |
| TYP-051.9 | `"0.0000000012345678"` | −16 | `Ok(12_345_678)` |
| TYP-051.10 | `"9999999999999999999"`(19 个 9) | 0 | `Err(OVERFLOW)` |
| TYP-051.11 | `"99999999"` | 0 | `Ok(99_999_999)` |

### TYP-070 qty_from_u256 的两个换算方向、取整与范围

- 规则:[链上 U256 ↔ Qty](README.md#链上-u256--qtyoptional链上组件使用)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:v 以十进制写出,调用时按小端 64 位肢传入;`d = token_decimals + qty_exp`
- 实现:`crates/hft-types/tests/u256.rs::typ_070_qty_from_u256`

| 行 | v | token_decimals | qty_exp | m | 期望 |
|---|---|---|---|---|---|
| TYP-070.1 | 10^18 | 18 | −8 | Floor | `Ok(Qty(100_000_000))` |
| TYP-070.2 | 10^18 + 1 | 18 | −8 | Floor | `Ok(Qty(100_000_000))` |
| TYP-070.3 | 10^18 + 1 | 18 | −8 | Ceil | `Ok(Qty(100_000_001))` |
| TYP-070.4 | 10^18 + 1 | 18 | −8 | TowardZero | `Ok(Qty(100_000_000))` |
| TYP-070.5 | 10^18 + 1 | 18 | −8 | HalfEven | `Ok(Qty(100_000_000))` |
| TYP-070.6 | 5 × 10^9 | 18 | −8 | HalfEven | `Ok(Qty(0))`(0.5 取偶) |
| TYP-070.7 | 5 × 10^9 | 18 | −8 | Ceil | `Ok(Qty(1))` |
| TYP-070.8 | 15 × 10^9 | 18 | −8 | HalfEven | `Ok(Qty(2))` |
| TYP-070.9 | 10^6 | 6 | −8 | Floor | `Ok(Qty(100_000_000))`(d = −2,乘方向) |
| TYP-070.10 | 46_116_860_184_273_879 | 6 | −8 | Floor | `Ok(Qty(4_611_686_018_427_387_900))` |
| TYP-070.11 | 46_116_860_184_273_880 | 6 | −8 | Floor | `Err(OVERFLOW)` |
| TYP-070.12 | 2^256 − 1(四肢全 `u64::MAX`) | 18 | −8 | Floor | `Err(OVERFLOW)` |
| TYP-070.13 | 75 × 10^35 | 30 | 6 | HalfEven | `Ok(Qty(8))`(d = 36,两次除法) |
| TYP-070.14 | 75 × 10^35 | 30 | 6 | Floor | `Ok(Qty(7))` |
| TYP-070.15 | 10^19 | 19 | 0 | Floor | `Ok(Qty(1))`(d = 19,单肢除数上限) |
| TYP-070.16 | 2 × 10^20 | 20 | 0 | Floor | `Ok(Qty(2))`(d = 20) |
| TYP-070.17 | 1 | 0 | −18 | Floor | `Ok(Qty(1_000_000_000_000_000_000))`(d = −18) |
| TYP-070.18 | 1 | 31 | 6 | Floor | `Err(EXP_RANGE)`(d = 37) |
| TYP-070.19 | 1 | 0 | −19 | Floor | `Err(EXP_RANGE)` |
| TYP-070.20 | 0 | 18 | −8 | Ceil | `Ok(Qty(0))` |
| TYP-070.21 | MAX_RAW | 0 | 0 | Floor | `Ok(Qty(MAX_RAW))` |
| TYP-070.22 | 2^64(肢 `[0, 1, 0, 0]`) | 0 | 0 | Floor | `Err(OVERFLOW)` |

### TYP-071 qty_to_u256 精确相乘,负数与不可整除被拒

- 规则:[链上 U256 ↔ Qty](README.md#链上-u256--qtyoptional链上组件使用)
- 级别:table
- 里程碑:M1
- 前置:期望的 v 以十进制写出,比较时按小端肢展开
- 实现:`crates/hft-types/tests/u256.rs::typ_071_qty_to_u256`

| 行 | q | token_decimals | qty_exp | 期望 |
|---|---|---|---|---|
| TYP-071.1 | 100_000_000 | 18 | −8 | `Ok(10^18)` |
| TYP-071.2 | −1 | 18 | −8 | `Err(NEGATIVE)` |
| TYP-071.3 | 100_000_000 | 6 | −8 | `Ok(10^6)` |
| TYP-071.4 | 123 | 6 | −8 | `Err(PRECISION)`(d = −2,123 不被 100 整除) |
| TYP-071.5 | MAX_RAW | 30 | 6 | `Ok(MAX_RAW × 10^36)` |
| TYP-071.6 | 1 | 31 | 6 | `Err(EXP_RANGE)` |
| TYP-071.7 | 0 | 18 | −8 | `Ok(0)` |

### TYP-072 U256 换算往返与取整夹逼

- 规则:[链上 U256 ↔ Qty](README.md#链上-u256--qtyoptional链上组件使用)
- 级别:prop
- 里程碑:M1
- 性质:对 `q ∈ [0, MAX_RAW]` 与 `d ∈ [0, 36]`:`qty_from_u256(qty_to_u256(q)?, …, m) == Ok(q)`(四种 m);对任意 v 与 `d ∈ [-18, 36]`:Floor 结果 ≤ v / 10^d 的精确值 ≤ Ceil 结果,两者相差 ≤ 1(未溢出时)
- 生成:q 均匀;token_decimals 与 qty_exp 组合使 d 落在范围内均匀;v 取四肢均匀随机与 10 的幂附近值各半
- 参考:任意精度整数
- 例数:PR 5 000
- 实现:`crates/hft-types/tests/prop_u256.rs::typ_072_u256_round_trip`

### TYP-080 ID newtype 的宽度与 StrategyInstanceId 别名

- 规则:[ID 与枚举](README.md#id-与枚举)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/ids.rs::typ_080_id_widths`

| 行 | 类型 | 期望 |
|---|---|---|
| TYP-080.1 | `InstId` | `size_of == 4` |
| TYP-080.2 | `AssetId` | `size_of == 2` |
| TYP-080.3 | `VenueId` | `size_of == 1` |
| TYP-080.4 | `AccountId` | `size_of == 1` |
| TYP-080.5 | `StrategyId` | `size_of == 1` |
| TYP-080.6 | `GroupId` | `size_of == 4` |
| TYP-080.7 | `SlotId` | `size_of == 4` |
| TYP-080.8 | `ConnId` | `size_of == 1` |
| TYP-080.9 | `Seq` | `size_of == 8` |
| TYP-080.10 | `Tsc` | `size_of == 8` |
| TYP-080.11 | `StrategyInstanceId` | `TypeId::of::<StrategyInstanceId>() == TypeId::of::<StrategyId>()` |

### TYP-081 具名 ID 常量的值固定

- 规则:[ID 与枚举](README.md#id-与枚举)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/ids.rs::typ_081_named_constants`

| 行 | 常量 | 期望 |
|---|---|---|
| TYP-081.1 | `INST_NONE` | `InstId(0)` |
| TYP-081.2 | `INST_ALL` | `InstId(0)` |
| TYP-081.3 | `ASSET_NONE` | `AssetId(0)` |
| TYP-081.4 | `GROUP_NONE` | `GroupId(0)` |
| TYP-081.5 | `GROUP_ALL` | `GroupId(4_294_967_295)` |
| TYP-081.6 | `ACCOUNT_ALL` | `AccountId(0)` |
| TYP-081.7 | `SlotId::UNRESOLVED` | `SlotId(0)` |
| TYP-081.8 | `MAX_BBO_CONN_SLOTS` | `8u8` |
| TYP-081.9 | `GroupId::N_MAX` | `16_777_214`(0xFF_FFFE) |

### TYP-082 GroupId::compose 只接受合法实例与计数器

- 规则:[ID 与枚举](README.md#id-与枚举)
- 级别:table
- 里程碑:M1
- 前置:调用 `GroupId::compose(StrategyId(s), n)`
- 实现:`crates/hft-types/tests/ids.rs::typ_082_group_id_compose`

| 行 | s | n | 期望 |
|---|---|---|---|
| TYP-082.1 | 1 | 1 | `Some(GroupId(0x0100_0001))` |
| TYP-082.2 | 254 | 0xFF_FFFE | `Some(GroupId(0xFEFF_FFFE))` |
| TYP-082.3 | 1 | 0xFF_FFFE | `Some(GroupId(0x01FF_FFFE))` |
| TYP-082.4 | 254 | 1 | `Some(GroupId(0xFE00_0001))` |
| TYP-082.5 | 0 | 1 | `None` |
| TYP-082.6 | 255 | 1 | `None` |
| TYP-082.7 | 1 | 0 | `None` |
| TYP-082.8 | 1 | 0xFF_FFFF | `None` |
| TYP-082.9 | 2 | 0x100_0000 | `None` |
| TYP-082.10 | 1 | `u32::MAX` | `None` |
| TYP-082.11 | 255 | 0xFF_FFFF | `None`(否则等于 `GROUP_ALL`) |
| TYP-082.12 | 0 | 0 | `None`(否则等于 `GROUP_NONE`) |

### TYP-083 GroupId 的高 8 位与低 24 位拆分

- 规则:[ID 与枚举](README.md#id-与枚举)
- 级别:table
- 里程碑:M1
- 前置:调用 `strategy()` 与 `counter()`
- 实现:`crates/hft-types/tests/ids.rs::typ_083_group_id_parts`

| 行 | GroupId | 期望 `(strategy(), counter())` |
|---|---|---|
| TYP-083.1 | 0x0300_0007 | `(StrategyId(3), 7)` |
| TYP-083.2 | 0xFEFF_FFFE | `(StrategyId(254), 0xFF_FFFE)` |
| TYP-083.3 | `GROUP_NONE` | `(StrategyId(0), 0)` |
| TYP-083.4 | `GROUP_ALL` | `(StrategyId(255), 0xFF_FFFF)` |

### TYP-084 合法构造的 GroupId 永不等于哨兵且互不相同

- 规则:[ID 与枚举](README.md#id-与枚举)
- 级别:prop
- 里程碑:M1
- 性质:对 `s ∈ 1..=254`、`n ∈ 1..=N_MAX`:`compose(s, n)` 为 `Some(g)`,`g ∈ [0x0100_0001, 0xFEFF_FFFE]`,`g ∉ {GROUP_NONE, GROUP_ALL}`,`g.strategy() == s`、`g.counter() == n`;两组不同的 (s, n) 给出不同的 g;`compose` 可在 `const` 上下文求值
- 生成:(s, n) 对均匀,另含 s ∈ {1, 254}、n ∈ {1, N_MAX} 的边界组合
- 参考:`(s << 24) | n` 的整数运算
- 例数:PR 10 000
- 实现:`crates/hft-types/tests/prop_ids.rs::typ_084_compose_unique_and_never_sentinel`

### TYP-085 VenueId 的市场段与常量名逐行固定

- 规则:[ID 与枚举](README.md#id-与枚举)
- 级别:table
- 里程碑:M1
- 前置:调用 `VenueId(v).market()` 与 `VenueId(v).name()`;同时断言每个具名常量(如 `BINANCE_USDM`)的值等于本表中 `name()` 为其名字的那一行的 v
- 实现:`crates/hft-types/tests/ids.rs::typ_085_venue_table`

| 行 | v | market() | name() |
|---|---|---|---|
| TYP-085.1 | 0 | `None` | `None` |
| TYP-085.2 | 1 | `Some(LinearPerp)` | `Some("BINANCE_USDM")` |
| TYP-085.3 | 2 | `Some(Spot)` | `Some("BINANCE_SPOT")` |
| TYP-085.4 | 3 | `Some(LinearPerp)` | `Some("GATE_USDT_FUT")` |
| TYP-085.5 | 4 | `Some(Spot)` | `Some("GATE_SPOT")` |
| TYP-085.6 | 5 | `Some(LinearPerp)` | `Some("OKX_SWAP")` |
| TYP-085.7 | 6 | `Some(Spot)` | `Some("OKX_SPOT")` |
| TYP-085.8 | 7(保留) | `None` | `None` |
| TYP-085.9 | 8 | `Some(LinearPerp)` | `Some("BYBIT_LINEAR")` |
| TYP-085.10 | 9 | `Some(Spot)` | `Some("BYBIT_SPOT")` |
| TYP-085.11 | 10 | `Some(LinearPerp)` | `Some("BITGET_UTA_USDT_FUT")` |
| TYP-085.12 | 11 | `Some(Spot)` | `Some("BITGET_UTA_SPOT")` |
| TYP-085.13 | 12 | `Some(LinearPerp)` | `Some("KUCOIN_FUT")` |
| TYP-085.14 | 13 | `Some(Spot)` | `Some("KUCOIN_SPOT")` |
| TYP-085.15 | 14 | `Some(LinearPerp)` | `Some("HYPERLIQUID_PERP")` |
| TYP-085.16 | 15 | `Some(Spot)` | `Some("POLYMARKET_CLOB")` |
| TYP-085.17 | 31 | `None` | `None` |
| TYP-085.18 | 32 | `None` | `Some("CHAIN_BSC")` |
| TYP-085.19 | 33 | `None` | `Some("CHAIN_SOLANA")` |
| TYP-085.20 | 34 | `None` | `None` |
| TYP-085.21 | 255 | `None` | `None` |
| TYP-085.22 | 16 | `None` | `None` |

### TYP-086 线格式枚举的码值往返,0 与越界值解码失败

- 规则:[ID 与枚举](README.md#id-与枚举)、[布局规则](README.md#布局规则全仓-reprc-结构适用)
- 级别:table
- 里程碑:M1
- 前置:解码用 `TryFromBytes::try_read_from_bytes(&[b])`;每个成功行另断言 `variant.as_bytes() == [b]`;四个类型 `size_of == 1`
- 实现:`crates/hft-types/tests/codes.rs::typ_086_wire_enum_codes`

| 行 | 类型 | 字节 | 期望 |
|---|---|---|---|
| TYP-086.1 | `Side` | 0 | `Err` |
| TYP-086.2 | `Side` | 1 | `Ok(Buy)` |
| TYP-086.3 | `Side` | 2 | `Ok(Sell)` |
| TYP-086.4 | `Side` | 3 | `Err` |
| TYP-086.5 | `Side` | 255 | `Err` |
| TYP-086.6 | `Tif` | 0 | `Err` |
| TYP-086.7 | `Tif` | 1 | `Ok(Gtc)` |
| TYP-086.8 | `Tif` | 2 | `Ok(Ioc)` |
| TYP-086.9 | `Tif` | 3 | `Ok(Fok)` |
| TYP-086.10 | `Tif` | 4 | `Ok(PostOnly)` |
| TYP-086.11 | `Tif` | 5 | `Err` |
| TYP-086.12 | `MarketKind` | 0 | `Err` |
| TYP-086.13 | `MarketKind` | 1 | `Ok(Spot)` |
| TYP-086.14 | `MarketKind` | 2 | `Ok(LinearPerp)` |
| TYP-086.15 | `MarketKind` | 3 | `Err` |
| TYP-086.16 | `HostClass` | 0 | `Err` |
| TYP-086.17 | `HostClass` | 1 | `Ok(Dev2)` |
| TYP-086.18 | `HostClass` | 2 | `Ok(C4)` |
| TYP-086.19 | `HostClass` | 3 | `Ok(C8)` |
| TYP-086.20 | `HostClass` | 4 | `Ok(Metal)` |
| TYP-086.21 | `HostClass` | 5 | `Ok(C16)` |
| TYP-086.22 | `HostClass` | 6 | `Ok(C32)` |
| TYP-086.23 | `HostClass` | 7 | `Err` |

### TYP-100 kill 字常量的位值

- 规则:[kill 字位](README.md#kill-字位)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/kill.rs::typ_100_kill_constants`

| 行 | 常量 | 期望 |
|---|---|---|
| TYP-100.1 | `KillBits::GLOBAL` | `0x0000_0001` |
| TYP-100.2 | `KillBits::VENUE` | `0x0000_0002` |
| TYP-100.3 | `KillBits::HARD` | `0x0000_0004` |
| TYP-100.4 | `KillBits::ACCOUNT0` | `0x0000_0100` |
| TYP-100.5 | `KillBits::ACCOUNT_HARD0` | `0x0010_0000` |
| TYP-100.6 | `KillBits::ACCOUNT_MASK` | `0x000F_FF00` |
| TYP-100.7 | `KillBits::ACCOUNT_HARD_MASK` | `0xFFF0_0000` |
| TYP-100.8 | `KillBits::MAX_ACCOUNTS_PER_GW` | `12` |
| TYP-100.9 | `size_of::<KillBits>()`、`align_of::<KillBits>()` | `4`、`4` |

### TYP-101 按账户序号的 kill 位与掩码

- 规则:[kill 字位](README.md#kill-字位)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/kill.rs::typ_101_account_bits`

| 行 | 调用 | 期望 |
|---|---|---|
| TYP-101.1 | `account(0)` | `0x0000_0100` |
| TYP-101.2 | `account(1)` | `0x0000_0200` |
| TYP-101.3 | `account(11)` | `0x0008_0000` |
| TYP-101.4 | `account_hard(0)` | `0x0010_0000` |
| TYP-101.5 | `account_hard(1)` | `0x0020_0000` |
| TYP-101.6 | `account_hard(11)` | `0x8000_0000` |
| TYP-101.7 | `gw_mask(0)` | `0x0010_0107` |
| TYP-101.8 | `gw_mask(5)` | `0x0200_2007` |
| TYP-101.9 | `gw_mask(11)` | `0x8008_0007` |
| TYP-101.10 | `hard_mask(0)` | `0x0010_0004` |
| TYP-101.11 | `hard_mask(11)` | `0x8000_0004` |

### TYP-102 12 个账户序号的位两两不重叠且恰好铺满两个位段

- 规则:[kill 字位](README.md#kill-字位)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:ix 遍历 0..12
- 步骤:
  1. 收集 24 个值 `account(ix)`、`account_hard(ix)`
  2. 两两求与;分别求全体的或
- 期望:
  - 每个值恰有 1 位;`account(ix) & ACCOUNT_MASK == account(ix)`,`account_hard(ix) & ACCOUNT_HARD_MASK == account_hard(ix)`
  - 24 个值两两相与为 0;每个值与 `GLOBAL | VENUE | HARD | 0xF8` 相与为 0(bit 3..7 保留,任何构造函数都不置位)
  - `account(0..12)` 的或 == `0x000F_FF00`;`account_hard(0..12)` 的或 == `0xFFF0_0000`
- 实现:`crates/hft-types/tests/kill.rs::typ_102_account_bits_partition`

### TYP-103 一个账户的 kill 位不进入其他账户的 gw 掩码

- 规则:[kill 字位](README.md#kill-字位)
- 级别:unit
- 里程碑:M1
- 前置:ix、jx 遍历 0..12,ix ≠ jx;另取字 `w = account(3) | account_hard(7)`
- 步骤:
  1. 对每对 (ix, jx) 计算 `account(jx) & gw_mask(ix)`、`account_hard(jx) & gw_mask(ix)`、`account_hard(jx) & hard_mask(ix)`
  2. 计算 `w & gw_mask(4)`、`w & gw_mask(3)`、`w & hard_mask(3)`、`w & hard_mask(7)`
- 期望:
  - 步骤 1 全部为 0
  - 步骤 2 依次为 `0`、`account(3)`(= `0x0000_0800`)、`0`、`account_hard(7)`(= `0x0800_0000`)
- 实现:`crates/hft-types/tests/kill.rs::typ_103_cross_account_isolation`

### TYP-104 账户序号 12 在 debug 构建断言失败、release 构建不越入其他位段

- 规则:[kill 字位](README.md#kill-字位)
- 级别:unit
- 里程碑:M1
- 前置:`#[cfg_attr(debug_assertions, should_panic)]`
- 步骤:
  1. 调用 `KillBits::account(12)`、`KillBits::account_hard(12)`、`KillBits::gw_mask(12)`
- 期望:
  - debug 构建:第一个调用触发 debug_assert(测试以 panic 通过)
  - release 构建:三者依次为 `KillBits(0)`、`KillBits(0)`、`KillBits(0x0000_0007)`
- 实现:`crates/hft-types/tests/kill.rs::typ_104_account_index_12`

### TYP-105 掩码函数可在 const 上下文求值且与运行期相同

- 规则:[kill 字位](README.md#kill-字位)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`const M: KillBits = KillBits::gw_mask(3); const H: KillBits = KillBits::hard_mask(3);`
- 步骤:
  1. 编译并读取 M、H;运行期调用 `gw_mask(3)`、`hard_mask(3)`
- 期望:
  - `M == gw_mask(3) == KillBits(0x0080_0807)`;`H == hard_mask(3) == KillBits(0x0080_0004)`
- 实现:`crates/hft-types/tests/kill.rs::typ_105_const_masks`

### TYP-110 RunMode 的码值,退役值与越界值解码失败

- 规则:[模式、kill 级别、cap 来源与风控拒绝位](README.md#模式kill-级别cap-来源与风控拒绝位)
- 级别:table
- 里程碑:M1
- 前置:`TryFromBytes::try_read_from_bytes(&[b])`
- 实现:`crates/hft-types/tests/codes.rs::typ_110_run_mode_codes`

| 行 | 字节 | 期望 |
|---|---|---|
| TYP-110.1 | 0 | `Err` |
| TYP-110.2 | 1 | `Ok(RunMode::Shadow)` |
| TYP-110.3 | 2(退役) | `Err` |
| TYP-110.4 | 3 | `Ok(RunMode::CloseOnly)` |
| TYP-110.5 | 4 | `Ok(RunMode::Live)` |
| TYP-110.6 | 5 | `Err` |
| TYP-110.7 | 255 | `Err` |

### TYP-111 KillLevel 的码值,退役值与越界值解码失败

- 规则:[模式、kill 级别、cap 来源与风控拒绝位](README.md#模式kill-级别cap-来源与风控拒绝位)
- 级别:table
- 里程碑:M1
- 前置:`TryFromBytes::try_read_from_bytes(&[b])`
- 实现:`crates/hft-types/tests/codes.rs::typ_111_kill_level_codes`

| 行 | 字节 | 期望 |
|---|---|---|
| TYP-111.1 | 0 | `Err` |
| TYP-111.2 | 1(退役) | `Err` |
| TYP-111.3 | 2 | `Ok(KillLevel::Venue)` |
| TYP-111.4 | 3 | `Ok(KillLevel::Account)` |
| TYP-111.5 | 4 | `Ok(KillLevel::Global)` |
| TYP-111.6 | 5 | `Err` |

### TYP-112 CapSource 的码值与可被 Resume 清除的集合

- 规则:[模式、kill 级别、cap 来源与风控拒绝位](README.md#模式kill-级别cap-来源与风控拒绝位)
- 级别:table
- 里程碑:M1
- 前置:调用 `CapSource(v).resumable()`;有名字的行另断言常量值
- 实现:`crates/hft-types/tests/codes.rs::typ_112_cap_source`

| 行 | v | 常量 | resumable() |
|---|---|---|---|
| TYP-112.1 | 0 | — | `false` |
| TYP-112.2 | 1 | `KILL` | `true` |
| TYP-112.3 | 2 | `DAILY_LOSS` | `true` |
| TYP-112.4 | 3 | `RECON` | `true` |
| TYP-112.5 | 4 | `UNKNOWN_TIMEOUT` | `true` |
| TYP-112.6 | 5 | `BROKEN` | `true` |
| TYP-112.7 | 6 | `REORG` | `true` |
| TYP-112.8 | 7 | `RECOVERY` | `true` |
| TYP-112.9 | 8 | `HALTED` | `true` |
| TYP-112.10 | 9 | `DRAINING` | `false` |
| TYP-112.11 | 10 | `CLOCK_BAD` | `false` |
| TYP-112.12 | 11 | `HEDGE_FAILURE` | `true` |
| TYP-112.13 | 12 | `MARGIN_STRESS` | `false` |
| TYP-112.14 | 13 | `STORAGE` | `false` |
| TYP-112.15 | 14 | `DELISTING` | `true` |
| TYP-112.16 | 15 | `ACCOUNT_CFG` | `false` |
| TYP-112.17 | 16 | — | `false` |
| TYP-112.18 | 255 | — | `false` |

### TYP-113 RejectBits 各常量的位号

- 规则:[模式、kill 级别、cap 来源与风控拒绝位](README.md#模式kill-级别cap-来源与风控拒绝位)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/codes.rs::typ_113_reject_bits_values`

| 行 | 常量 | 期望 |
|---|---|---|
| TYP-113.1 | `INVALID` | `1 << 0` |
| TYP-113.2 | `NOT_TRADABLE` | `1 << 1` |
| TYP-113.3 | `VENUE_NOT_READY` | `1 << 2` |
| TYP-113.4 | `KILLED` | `1 << 3` |
| TYP-113.5 | `CLOSE_ONLY` | `1 << 4` |
| TYP-113.6 | `HARD` | `1 << 5` |
| TYP-113.7 | `UNKNOWN_PENDING` | `1 << 6` |
| TYP-113.8 | `NO_MID` | `1 << 7` |
| TYP-113.9 | `STALE_MID` | `1 << 8` |
| TYP-113.10 | `PX_BAND` | `1 << 9` |
| TYP-113.11 | `NOTIONAL_ORDER` | `1 << 10` |
| TYP-113.12 | `NOTIONAL_GROUP` | `1 << 11` |
| TYP-113.13 | `NOTIONAL_ACCOUNT` | `1 << 12` |
| TYP-113.14 | `NOTIONAL_VENUE` | `1 << 13` |
| TYP-113.15 | `POSITION` | `1 << 14` |
| TYP-113.16 | `REDUCE_INVALID` | `1 << 15` |
| TYP-113.17 | `RATE` | `1 << 16` |
| TYP-113.18 | `RATE_BACKOFF` | `1 << 17` |
| TYP-113.19 | `DAILY_LOSS` | `1 << 18` |
| TYP-113.20 | `EXPOSURE_GATE` | `1 << 19` |
| TYP-113.21 | `RECON_DEVIATION` | `1 << 20` |
| TYP-113.22 | `CLOCK` | `1 << 22` |
| TYP-113.23 | `DRAINING` | `1 << 23` |
| TYP-113.24 | `MODE_SHADOW` | `1 << 24` |
| TYP-113.25 | `LOCAL_SEND_FAIL` | `1 << 25` |
| TYP-113.26 | `HEDGE_LIMIT` | `1 << 26` |
| TYP-113.27 | `NOT_BOUND` | `1 << 27` |
| TYP-113.28 | `LINK_STALE` | `1 << 28` |
| TYP-113.29 | `LINK_FUNDS` | `1 << 29` |

同一位号表还校验 `CAPACITY = 1 << 30`,TYP-114 包含该位。

### TYP-114 RejectBits 两两不重叠且不使用退役位 21

- 规则:[模式、kill 级别、cap 来源与风控拒绝位](README.md#模式kill-级别cap-来源与风控拒绝位)
- 级别:unit
- 里程碑:M1
- 前置:TYP-113 的 30 个常量(含 CAPACITY)
- 步骤:
  1. 两两求与;求全体的或
  2. 读取 `size_of::<RejectBits>()`
- 期望:
  - 两两相与全为 0;全体的或 == `0x7FDF_FFFF`(bit 21 与 bit 31 为 0)
  - `size_of::<RejectBits>() == 4`
- 实现:`crates/hft-types/tests/codes.rs::typ_114_reject_bits_disjoint`

### TYP-120 HedgePxLimits 是可复制、按字段比较的值类型

- 规则:[对冲定价限值](README.md#对冲定价限值)
- 级别:unit
- 里程碑:M1
- 前置:`a = HedgePxLimits { conn_liveness_ms: 1000, rr_max_cross: Rate(5_000_000) }`
- 步骤:
  1. `let b = a;`(复制),比较 `a == b`
  2. `let c = HedgePxLimits { conn_liveness_ms: 999, ..a };` 比较 `a == c`
  3. `format!("{a:?}")`
- 期望:
  - 步骤 1 为 `true`,`a` 仍可用(`Copy`)
  - 步骤 2 为 `false`
  - 步骤 3 的输出包含 `conn_liveness_ms: 1000` 与 `rr_max_cross`
- 实现:`crates/hft-types/tests/hedge.rs::typ_120_hedge_px_limits_value`

### TYP-121 cap_cross_px 取两个限值中更紧的一侧并按买卖方向取整一次

- 规则:[对冲定价限值](README.md#对冲定价限值)、[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:table
- 里程碑:M1
- 前置:`lim = HedgePxLimits { conn_liveness_ms: 1000, rr_max_cross: 列 rr }`;max_cross 与 rr 写 Rate raw
- 实现:`crates/hft-types/tests/hedge.rs::typ_121_cap_cross_px`

| 行 | side | touch | mid | max_cross | rr | tick | 期望 |
|---|---|---|---|---|---|---|---|
| TYP-121.1 | Buy | 1_000_000 | 999_000 | 10_000_000 | 5_000_000 | 1 | `Px(1_003_995)`(mid 侧更紧) |
| TYP-121.2 | Buy | 1_000_000 | 999_000 | 1_000_000 | 5_000_000 | 1 | `Px(1_001_000)`(touch 侧更紧) |
| TYP-121.3 | Buy | 1_000_000 | 1_000_000 | 5_000_000 | 5_000_000 | 1 | `Px(1_005_000)`(两侧相等) |
| TYP-121.4 | Buy | 1_000_000 | 999_000 | 1_234_000 | 5_000_000 | 100 | `Px(1_001_200)`(1_001_234 向下取到 100) |
| TYP-121.5 | Sell | 1_000_000 | 1_001_000 | 10_000_000 | 5_000_000 | 100 | `Px(996_000)`(mid 侧 995_995 更紧,向上取到 100) |
| TYP-121.6 | Sell | 1_000_000 | 1_001_000 | 1_000_000 | 5_000_000 | 1 | `Px(999_000)`(touch 侧更紧) |
| TYP-121.7 | Buy | 1_000_000 | 999_000 | 0 | 5_000_000 | 1 | `Px(1_000_000)`(= touch) |
| TYP-121.8 | Buy | 1_000_000 | 900_000 | 10_000_000 | 5_000_000 | 1 | `Px(904_500)`(低于 touch,照常返回) |
| TYP-121.9 | Buy | 3 | 1_000 | 500_000_000 | 10_000_000 | 1 | `Px(4)`(4.5 向下) |
| TYP-121.10 | Sell | 3 | 1 | 500_000_000 | 10_000_000 | 1 | `Px(2)`(1.5 向上) |
| TYP-121.11 | Buy | 0 | 1_000 | 1_000_000 | 5_000_000 | 1 | `Px(0)` |
| TYP-121.12 | Buy | 1_000 | 0 | 1_000_000 | 5_000_000 | 1 | `Px(0)` |
| TYP-121.13 | Sell | −1 | 1_000 | 1_000_000 | 5_000_000 | 1 | `Px(0)` |
| TYP-121.14 | Buy | 1_000 | −1 | 1_000_000 | 5_000_000 | 1 | `Px(0)` |
| TYP-121.15 | Buy | 1_000_000 | 999_000 | −1 | 5_000_000 | 1 | `Px(0)` |
| TYP-121.16 | Buy | 1_000_000 | 999_000 | 1_000_000 | −1 | 1 | `Px(0)` |
| TYP-121.17 | Buy | 1_000_000 | 999_000 | 1_000_000 | 5_000_000 | 0 | `Px(0)` |
| TYP-121.18 | Sell | 1_000_000 | 1_000_000 | 1_000_000_000 | 1_000_000_000 | 1 | `Px(0)`(结果为 0) |
| TYP-121.19 | Sell | 1_000_000 | 1_000_000 | 1_500_000_000 | 1_500_000_000 | 1 | `Px(0)`(结果为负) |
| TYP-121.20 | Buy | MAX_RAW | MAX_RAW | 1_000_000_000 | 1_000_000_000 | 10 | `Px(4_611_686_018_427_387_900)`(饱和) |

### TYP-122 cap_cross_px 与 i128 参考逐值相同且不越过两个上限

- 规则:[对冲定价限值](README.md#对冲定价限值)
- 级别:prop
- 里程碑:M1
- 性质:合法输入(touch、mid、tick > 0,两个 Rate ∈ [0, 10^9])下,买单结果 ≤ `mid × (1 + rr)` 与 `touch × (1 + max_cross)` 的较小者、比它小不到一个 tick;卖单结果 ≥ `mid × (1 − rr)` 与 `touch × (1 − max_cross)` 的较大者、比它大不到一个 tick(结果 > 0 时);结果是 tick 整数倍;与参考逐值相同
- 生成:touch、mid ∈ [1, 10^12] 且相差不超过 20%;max_cross、rr ∈ [0, 10^8];tick 取 10 的幂与随机值各半;side 均匀
- 参考:i128 精确计算后按 `side.price_round()` 取到 tick
- 例数:PR 10 000
- 实现:`crates/hft-types/tests/prop_hedge.rs::typ_122_cap_cross_matches_reference`

### TYP-130 资金费结算时刻归一化到最近边界,等距取较早者

- 规则:[资金费结算时刻归一化](README.md#资金费结算时刻归一化)
- 级别:table
- 里程碑:M1
- 前置:时间戳为 UTC ns;2026-10-04 00:00:00 UTC = 1_791_072_000_000_000_000
- 实现:`crates/hft-types/tests/funding.rs::typ_130_funding_norm`

| 行 | ts_ns | interval_s | anchor_s | 期望 |
|---|---|---|---|---|
| TYP-130.1 | 1_791_100_800_003_000_000(08:00:00.003) | 28_800 | 0 | 1_791_100_800_000_000_000(08:00) |
| TYP-130.2 | 1_791_100_799_990_000_000(07:59:59.990) | 28_800 | 0 | 1_791_100_800_000_000_000 |
| TYP-130.3 | 1_791_086_400_000_000_000(04:00 正中) | 28_800 | 0 | 1_791_072_000_000_000_000(较早者 00:00) |
| TYP-130.4 | 1_791_086_400_000_000_001 | 28_800 | 0 | 1_791_100_800_000_000_000 |
| TYP-130.5 | 1_791_086_399_999_999_999 | 28_800 | 0 | 1_791_072_000_000_000_000 |
| TYP-130.6 | 1_791_100_800_003_000_000 | 0 | 0 | 1_791_100_800_003_000_000(原样) |
| TYP-130.7 | 1_791_108_600_000_000_000(10:10) | 3_600 | 1_800 | 1_791_109_800_000_000_000(10:30) |
| TYP-130.8 | 1_791_100_800_003_000_000 | 28_800 | 32_400 | 1_791_104_400_000_000_000(锚大于周期,边界 01:00/09:00/17:00) |
| TYP-130.9 | −1 | 28_800 | 0 | 0 |
| TYP-130.10 | −14_400_000_000_000(−4 h 正中) | 28_800 | 0 | −28_800_000_000_000 |
| TYP-130.11 | i64::MAX | 28_800 | 0 | 9_223_344_000_000_000_000(最近边界超出 i64,取范围内相邻边界) |
| TYP-130.12 | i64::MIN | 28_800 | 0 | −9_223_344_000_000_000_000 |
| TYP-130.13 | 0 | 4_294_967_295 | 4_294_967_295 | 0 |

### TYP-131 资金费归一化与 i128 参考逐值相同

- 规则:[资金费结算时刻归一化](README.md#资金费结算时刻归一化)
- 级别:prop
- 里程碑:M1
- 性质:对任意 `ts_ns: i64`、`interval_s ∈ [0, 604_800]`、`anchor_s: u32`:结果等于参考;`interval_s > 0` 时结果是整秒且 `(结果/10^9 − anchor_s)` 是 `interval_s` 的整数倍;未触发越界规则时 `|结果 − ts_ns| ≤ interval_s × 5 × 10^8`
- 生成:ts 一半取全 i64 均匀、一半取 2020–2040 年内均匀;interval 取 {0, 3_600, 14_400, 28_800} 与均匀值各半;anchor 均匀
- 参考:i128 实现(含等距取较早、越界取相邻边界)
- 例数:PR 10 000
- 实现:`crates/hft-types/tests/prop_funding.rs::typ_131_funding_norm_matches_reference`

### TYP-135 ClientOrderId 编码为固定的 24 个小写十六进制字符

- 规则:[ClientOrderId 编解码](README.md#clientorderid-编解码)
- 级别:table
- 里程碑:M1
- 前置:字段写作 `{epoch, venue, account, slot, gen}`
- 实现:`crates/hft-types/tests/clid.rs::typ_135_cid_encode_golden`

| 行 | 字段 | 期望文本 |
|---|---|---|
| TYP-135.1 | {1, 1, 1, 5, 7} | `"100010101000050000000702"` |
| TYP-135.2 | {0, 0, 0, 0, 0} | `"100000000000000000000010"` |
| TYP-135.3 | {0x7FFF, 255, 255, 0xF_FFFF, 0xFFFF_FFFF} | `"17ffffffffffffffffffff17"` |
| TYP-135.4 | {0x1234, 10, 3, 0xA_BCDE, 0xDEAD_BEEF} | `"112340a03abcdedeadbeefa8"` |
| TYP-135.5 | {5, 99, 200, 1, 2} | `"1000563c80000100000002f9"` |
| TYP-135.6 | {0x8001, 1, 1, 5, 7}(保留位置 1) | `"100010101000050000000702"`(保留位被忽略,同行 1) |

### TYP-136 ClientOrderId 规范文本解码回原字段

- 规则:[ClientOrderId 编解码](README.md#clientorderid-编解码)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/clid.rs::typ_136_cid_decode_golden`

| 行 | 文本 | 期望 |
|---|---|---|
| TYP-136.1 | `"100010101000050000000702"` | `Ok({1, 1, 1, 5, 7})` |
| TYP-136.2 | `"100000000000000000000010"` | `Ok({0, 0, 0, 0, 0})` |
| TYP-136.3 | `"17ffffffffffffffffffff17"` | `Ok({0x7FFF, 255, 255, 0xF_FFFF, 0xFFFF_FFFF})` |
| TYP-136.4 | `"112340a03abcdedeadbeefa8"` | `Ok({0x1234, 10, 3, 0xA_BCDE, 0xDEAD_BEEF})` |
| TYP-136.5 | `"1000563c80000100000002f9"` | `Ok({5, 99, 200, 1, 2})`(不判断 venue/account 是否存在) |

### TYP-137 ClientOrderId 解码的每个错误码与先后顺序

- 规则:[ClientOrderId 编解码](README.md#clientorderid-编解码)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/clid.rs::typ_137_cid_decode_errors`

| 行 | 文本 | 期望 |
|---|---|---|
| TYP-137.1 | `""` | `Err(CidErr::Length)` |
| TYP-137.2 | `"10001010100005000000070"`(23 字符) | `Err(CidErr::Length)` |
| TYP-137.3 | `"1000101010000500000007020"`(25 字符) | `Err(CidErr::Length)` |
| TYP-137.4 | `"1000101010000500000007A2"`(大写) | `Err(CidErr::Charset)` |
| TYP-137.5 | `"10001010100005000000070g"` | `Err(CidErr::Charset)` |
| TYP-137.6 | 24 字节,第 1 字节为 0xC3 | `Err(CidErr::Charset)` |
| TYP-137.7 | `"200010101000050000000732"`(ver = 2,校验字节相符) | `Err(CidErr::Version)` |
| TYP-137.8 | `"000010101000050000000712"`(ver = 0,校验字节相符) | `Err(CidErr::Version)` |
| TYP-137.9 | `"100010101000050000000703"`(校验字节翻转一位) | `Err(CidErr::Check)` |
| TYP-137.10 | `"100010101000050000000602"`(gen 翻转一位) | `Err(CidErr::Check)` |
| TYP-137.11 | `"18001010100005000000070a"`(保留位 1,校验字节相符) | `Err(CidErr::Reserved)` |
| TYP-137.12 | `"200010101000050000000702"`(ver = 2 且校验字节不符) | `Err(CidErr::Version)`(Version 先于 Check) |
| TYP-137.13 | `"180010101000050000000702"`(保留位 1 且校验字节不符) | `Err(CidErr::Check)`(Check 先于 Reserved) |
| TYP-137.14 | `"10001010100005000000070G"`(大写且末位校验字节不符) | `Err(CidErr::Charset)`(Charset 先于 Check) |

### TYP-138 ClientOrderId 全字段随机往返

- 规则:[ClientOrderId 编解码](README.md#clientorderid-编解码)
- 级别:prop
- 里程碑:M1
- 性质:`decode(encode(x)) == Ok(x)`;文本 24 字节全在 `[0-9a-f]`,首字符 `'1'`
- 生成:epoch ∈ [0, 0x7FFF]、venue 与 account 为任意 u8、slot ∈ [0, 0xF_FFFF]、gen 为任意 u32,另含各字段的上下界组合
- 参考:往返恒等
- 例数:PR 10 000
- 实现:`crates/hft-types/tests/prop_clid.rs::typ_138_cid_round_trip`

### TYP-139 ClientOrderId 任一单比特翻转都被检出

- 规则:[ClientOrderId 编解码](README.md#clientorderid-编解码)
- 级别:prop
- 里程碑:M1
- 性质:对合法 x 的 96 位值翻转任一位后重新渲染成文本:翻转 bit 92–95(ver)→ `Err(Version)`;其余 92 位 → `Err(Check)`
- 生成:x 同 TYP-138;翻转位在 0..96 均匀
- 参考:按翻转位位置给出的期望错误
- 例数:PR 10 000
- 实现:`crates/hft-types/tests/prop_clid.rs::typ_139_single_bit_flip_detected`

### TYP-140 slot 超出 20 位在 debug 构建断言失败、release 构建只编码低 20 位

- 规则:[ClientOrderId 编解码](README.md#clientorderid-编解码)
- 级别:unit
- 里程碑:M1
- 前置:`#[cfg_attr(debug_assertions, should_panic)]`;`x = {1, 1, 1, 0x10_0005, 7}`
- 步骤:
  1. `x.encode(&mut out)`,再 `decode(&out)`
- 期望:
  - debug 构建:encode 触发 debug_assert(测试以 panic 通过)
  - release 构建:文本 == `"100010101000050000000702"`,解码得 `slot = 5`
- 实现:`crates/hft-types/tests/clid.rs::typ_140_slot_over_20_bits`

### TYP-141 解码不做语义判断

- 规则:[ClientOrderId 编解码](README.md#clientorderid-编解码)
- 级别:unit
- 里程碑:M1
- 前置:无参数
- 步骤:
  1. `decode(b"1000563c80000100000002f9")`(venue 99 未分配、account 200)
  2. `decode(b"17ffffffffffffffffffff17")`(epoch 0x7FFF)
- 期望:
  - 步骤 1 返回 `Ok`,`venue == VenueId(99)`、`account == AccountId(200)`
  - 步骤 2 返回 `Ok`,`epoch == 0x7FFF`
- 实现:`crates/hft-types/tests/clid.rs::typ_141_decode_has_no_semantics`

### TYP-142 CidErr 码值与保留位常量

- 规则:[ClientOrderId 编解码](README.md#clientorderid-编解码)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/clid.rs::typ_142_cid_err_codes`

| 行 | 常量 | 期望 |
|---|---|---|
| TYP-142.1 | `CidErr::Length as u8` | 1 |
| TYP-142.2 | `CidErr::Charset as u8` | 2 |
| TYP-142.3 | `CidErr::Version as u8` | 3 |
| TYP-142.4 | `CidErr::Check as u8` | 4 |
| TYP-142.5 | `CidErr::Reserved as u8` | 5 |
| TYP-142.6 | `ClientOrderId::RESERVED_BIT` | `0x8000` |

### TYP-150 本 crate 的每个 repr(C) 结构与线格式枚举都有编译期布局断言

- 规则:[布局规则](README.md#布局规则全仓-reprc-结构适用)
- 级别:unit
- 里程碑:M1
- 前置:扫描 `crates/hft-types/src/` 的源码文本
- 步骤:
  1. 列出全部 `#[repr(C)]` 结构与 `#[repr(u8)]` 枚举
  2. 对每个结构查找紧随其后的 `assert!(size_of::<T>() == …)`;对每个枚举查找 `TryFromBytes` 派生
- 期望:
  - 结构集合为 `{AssetExp}`,其后有大小断言与两个字段的偏移断言
  - 枚举集合为 `{RoundMode, Side, Tif, MarketKind, HostClass, RunMode, KillLevel, CidErr}`;其中线格式枚举 `Side`、`Tif`、`MarketKind`、`HostClass`、`RunMode`、`KillLevel` 派生 `TryFromBytes` 且不派生 `FromBytes`
- 实现:`crates/hft-types/tests/layout.rs::typ_150_layout_asserts_present`

### TYP-155 FxErr 的位值

- 规则:[错误](README.md#错误)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-types/tests/err.rs::typ_155_fx_err_values`

| 行 | 常量 | 期望 |
|---|---|---|
| TYP-155.1 | `FxErr::OK` | `FxErr(0)` |
| TYP-155.2 | `FxErr::SYNTAX` | `FxErr(1)` |
| TYP-155.3 | `FxErr::EMPTY` | `FxErr(2)` |
| TYP-155.4 | `FxErr::PRECISION` | `FxErr(4)` |
| TYP-155.5 | `FxErr::OVERFLOW` | `FxErr(8)` |
| TYP-155.6 | `FxErr::NEGATIVE` | `FxErr(16)` |
| TYP-155.7 | `FxErr::DIV_ZERO` | `FxErr(32)` |
| TYP-155.8 | `FxErr::EXP_RANGE` | `FxErr(64)` |
| TYP-155.9 | `size_of::<FxErr>()` | 1 |

### TYP-156 多个字段的错误按位或聚合后一次判断

- 规则:[错误](README.md#错误)
- 级别:unit
- 里程碑:M1
- 前置:三个字段 `"1.5"`、`""`、`"x"`,`exp = -4`
- 步骤:
  1. `let mut e = FxErr::OK;` 对三个字段依次 `parse_exact`,把每个 `Err(b)` 以 `e |= b` 聚合
  2. 计算 `e | FxErr::OVERFLOW`
- 期望:
  - 步骤 1 后 `e == FxErr(3)`(`EMPTY | SYNTAX`),第一个字段得 `Ok(15_000)`
  - 步骤 2 为 `FxErr(11)`
- 实现:`crates/hft-types/tests/err.rs::typ_156_fx_err_aggregate`

### TYP-157 全部公开函数对任意输入不 panic

- 规则:[错误](README.md#错误)、[定点数:按资产指数](README.md#定点数按资产指数)
- 级别:prop
- 里程碑:M1
- 性质:在 `ci` 剖面(overflow-checks 与 debug-assertions 打开)下,`mul_qty`、`from_notional`、两个 `mul_rate`、`round_to_tick`、`round_to_lot`、`split_lot`、`mid_floor`、`ratio`、`pair_lot`、`cap_cross_px`、`funding_settle_norm_ns`、`parse_exact`、`parse_round`、`write_fixed`、`qty_from_u256`、`qty_to_u256`、`GroupId::compose`、`ClientOrderId::decode` 对任意参数都返回;返回 `Px`/`Qty` 的函数在输入满足不变量 1 时结果 `|x| ≤ MAX_RAW`(debug_assert 只在 KillBits 与 encode 的越界序号上触发,本用例不生成这两类调用)
- 生成:全部整数参数取任意 i64/i8/u8/u32 值,包含超出 MAX_RAW 与指数范围的值;字节串任意
- 参考:无 panic + 结果范围断言
- 例数:PR 20 000
- 实现:`crates/hft-types/tests/prop_total.rs::typ_157_no_panic_on_any_input`

### TYP-160 范围核验表各行的名义例逐值成立

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`m = TowardZero`;e 写作 `(px_exp, qty_exp)`
- 实现:`crates/hft-types/tests/fixed.rs::typ_160_range_table_examples`

| 行 | 资产 | 调用 | 期望 |
|---|---|---|---|
| TYP-160.1 | BTC,50 BTC @ 1e5 | `Px(1_000_000_000).mul_qty(Qty(5_000_000_000), (−4, −8))` | `(Notional(5_000_000_000_000), OK)`(5e6 USDT) |
| TYP-160.2 | PEPE,1e11 枚 @ 1e-5 | `Px(10_000_000).mul_qty(Qty(100_000_000_000_000), (−12, −3))` | `(Notional(1_000_000_000_000), OK)`(1e6 USDT) |
| TYP-160.3 | 链上低价代币,1e13 枚 @ 1e-10 | `Px(100_000_000).mul_qty(Qty(10_000_000_000_000), (−18, 0))` | `(Notional(1_000_000_000), OK)`(1e3 USDT) |
| TYP-160.4 | 供应 > 4e18 的代币,1e21 枚 | `parse_exact("1000000000000000000000", 3)` | `Ok(1_000_000_000_000_000_000)` |

### TYP-170 parse_exact 解析典型价格的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`types_parse_px`
- 实现:`crates/hft-types/benches/types.rs::typ_170_parse_px`

### TYP-171 parse_exact 解析典型数量的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`types_parse_qty`
- 实现:`crates/hft-types/benches/types.rs::typ_171_parse_qty`

### TYP-172 parse_exact 解析科学计数法的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`types_parse_sci`
- 实现:`crates/hft-types/benches/types.rs::typ_172_parse_sci`

### TYP-173 write_fixed 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`types_write_fixed`
- 实现:`crates/hft-types/benches/types.rs::typ_173_write_fixed`

### TYP-174 mul_qty 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`types_mul_qty`
- 实现:`crates/hft-types/benches/types.rs::typ_174_mul_qty`

### TYP-175 round_to_tick 与 round_to_lot 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`types_round`
- 实现:`crates/hft-types/benches/types.rs::typ_175_round`

### TYP-176 qty_from_u256 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`types_u256`
- 实现:`crates/hft-types/benches/types.rs::typ_176_u256`

### TYP-177 ClientOrderId::encode 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`types/clid_encode`
- 实现:`crates/hft-types/benches/types.rs::typ_177_clid_encode`

### TYP-178 ClientOrderId::decode 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`types/clid_decode`
- 实现:`crates/hft-types/benches/types.rs::typ_178_clid_decode`

### TYP-179 cap_cross_px 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`types_cap_cross_px`
- 实现:`crates/hft-types/benches/types.rs::typ_179_cap_cross_px`

### TYP-180 全部公开函数零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M1
- 前置:测试二进制安装计数分配器 `CountingAlloc`;输入取 TYP-010–TYP-142 各表第一行
- 步骤:
  1. 记录分配计数 c0
  2. 对 TYP-157 列出的每个函数及 `KillBits` 的四个函数、`ClientOrderId::encode` 各调用 10 000 次
  3. 记录分配计数 c1
- 期望:
  - `c1 − c0 == 0`
- 实现:`crates/hft-types/tests/alloc.rs::typ_180_zero_alloc`

## 验收

- M1 完成:本文件中里程碑为 M1 的用例全部有同编号实现且通过;TYP-049(M3)在 6 个 core 产品线的十进制样本入库后通过。
- 属性测试(TYP-004、012、020、024、045、047、048、072、084、122、131、138、139、157):PR 按各用例"例数";CI 每夜各 100 000 例;M1 验收前在服务器验证包中各 1 000 000 例无反例,失败的最小化种子写进 `crates/hft-types/tests/regressions/`。
- fuzz:TYP-050 按 [testing §3.3](../../docs/testing.md#33-fuzz) 的 PR 冒烟、每夜轮转与服务器累计运行,零崩溃(不计入门槛 ③)。
- 基准:TYP-170–TYP-179 在参考机 `bench-gate --reference` 下不超过 README [延迟与资源契约](README.md#延迟与资源契约) 的预算(从 M1.5 第一次参考机运行起判定,[benchmarks](../../benchmarks/README.md#参考机与-ci));CI 只报告。
- `cargo xtask ci` 的 clippy 禁用表确认本 crate 以外的热 crate 不出现 f64 定点换算(本 crate 不提供 `to_f64`,TYP-005)。
