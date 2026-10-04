---
description: "BSC 链上合约(Solidity + Foundry,optional,链上策略迁移时启用):最小包装合约 SwapGuard(链上过期 maxBlock、minOut、可选的合约内出价;由 chain-core 调用),可选扩展的原子 swap 合约 AtomicSwap(直接调池、多跳)"
kind: dir
tier: cold
scope: optional
status: frozen
---

# contracts

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核:无(`architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2 中没有只涉及本组件的行)。

> **可选扩展:CEX-DEX 策略从旧框架迁移时启用。** 新框架首要服务 CEX-CEX 策略,链上策略继续在旧框架运行、迁移不急;本规格保留,启用前按届时的实际情况复核。下文中的"core""迁移范围内"指该迁移启用之后的范围;外部 venue 协议与链上事件编号段在架构上保留。

本目录拥有我们的链上合约。**`SwapGuard`(core)**:每个执行钱包一份的最小包装合约,每笔 swap 都经它调用路由器(Universal Router / V3 SwapRouter / V2 Router);它强制三件事——`block.number <= maxBlock`(每笔交易有链上过期,迟到上链只会 revert、只花 gas)、按余额差校验 `minOut`、(builder profile)把出价在合约内转给 builder(InTx)——并且不留资金(round 3 C1)。**调用方只有 [chain-core](../apps/chain-core/README.md)**:calldata 由 [hft-chain-evm](../crates/hft-chain-evm/README.md) 编码,经 free profile 走免费公共 RPC——交易进入公共 mempool,可能滞留很久后才上链,`maxBlock` 是它唯一的过期;`maxBlock = 发送时块号 + N`(默认 3),不出价(`bribeTo = 0`)。部署合约的 gas 是唯一的固定花费(约 1 美元级)。`Swapped` 事件是判定成交的日志证据。

**`AtomicSwap`(可选扩展)**:直接调池的原子 swap 合约,省掉路由器的 gas 与命令编排限制,经审计与影子对比后按路由逐个替换 `SwapGuard + 路由器`。触发条件:路由器额外 gas 在每日 gas 报表中成为可观成本,或路由器命令编排限制挡住了需要的路由。tier 标 `cold` 只表示它不是本机热路径代码;它的"延迟"是 gas。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### `SwapGuard`

```solidity
// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.26;

interface ISwapGuard {
    struct Guard {
        uint64  maxBlock;           // free profile = 发送时块号 + N(默认 3);builder profile = bundle maxBlockNumber;block.number > maxBlock → revert Expired()
        address tokenIn;            // 必须是 ERC20;原生 BNB 进出由路由器内层处理
        uint256 amountIn;           // 从 msg.sender 拉取的数量(ExactOut 时为输入上限 maxIn)
        address tokenOut;
        uint256 minOut;             // 本合约 tokenOut 余额增量下限(ExactOut 时为目标输出量)
        address bribeTo;            // builder 收款地址;0 = 不出价(free profile 恒为 0)
        uint256 bribe;              // 必须 == msg.value
        bytes32 intentId;           // 关联 chain journal 与 TxLifecycle
    }

    event Swapped(bytes32 indexed intentId, uint256 amountIn, uint256 amountOut, address bribeTo, uint256 bribe);

    /// 只允许 executor(本合约绑定的执行钱包)调用;router 必须在构造时写入的不可变白名单内
    function execute(Guard calldata g, address router, bytes calldata inner) external payable
        returns (uint256 amountIn, uint256 amountOut);

    /// onboarding:给白名单路由器授权本合约持有的 token 的额度(executor 可调)
    function approveRouter(address token, address router, uint256 amount) external;
    /// onboarding:本合约 → Permit2 → Universal Router(amount: uint160,expiration: uint48)
    function approvePermit2(address token, uint160 amount, uint48 expiration) external;

    function setPaused(bool paused) external;                 // executor 或 owner 可暂停;只有 owner 可恢复
    function rescue(address token, uint256 amount) external;  // 误转入的资产只能转给 owner
}
```

调用约定(hft-chain-evm 必须遵守):

| 约定 | 说明 |
|---|---|
| 一个执行钱包一个 `SwapGuard` | `executor` 与 `owner` 在构造时写死(`immutable`);合约只替这一个钱包做事 |
| 资金来源与去向都是 `msg.sender` | 输入用 `transferFrom(msg.sender, this, amountIn)`;输出与剩余输入在同一交易内转回 `msg.sender`;没有 `recipient` 参数 |
| 内层 calldata 的收款人是 `SwapGuard` | 路由器把输出付给合约,合约以**余额差**计 `amountOut`,不解析内层 calldata |
| `maxBlock` | free profile = 发送时块号 + N;builder profile(可选扩展)= bundle `maxBlockNumber`;路由器路径另设 `deadline`(时间戳)作为第二道 |
| 出价只在合约内 | free profile `bribe = 0`、`msg.value = 0`;builder profile `bribe == msg.value`,校验通过后 `bribeTo.call{value: bribe}`,不用 bundle 第二笔转账 |
| 失败会上链(free profile) | 没有 bundle 的"失败不上链":`Expired`、`minOut` 不满足的交易照常上链并花 gas,记为 `IncludedReverted` 计入 gas 成本 |
| 日志证据 | `Swapped(intentId, amountIn, amountOut, …)` 给出精确数量,`TxTracker` 以它为主、ERC20 `Transfer` 为交叉核对 |

### Onboarding 授权(round 3 C10)

上币时一次完成,交易路径上从不授权:

1. 执行钱包:`ERC20.approve(SwapGuard, cap)`(`cap` 按该币仓位上限的倍数,不做无限授权)。
2. V2/V3 路由器:`SwapGuard.approveRouter(token, router, amount)`。
3. Universal Router:`SwapGuard.approvePermit2(token, amount, expiration)`——合约内先 `ERC20.approve(Permit2, type(uint256).max)`(仅首次),再 `Permit2.approve(token, UR, amount, expiration)`;chain-core 在到期前刷新。

### `AtomicSwap`(可选扩展)

```solidity
interface IAtomicSwap {
    enum PoolKind { V2, V3, InfinityCl }
    struct Hop { PoolKind kind; address tokenIn; address tokenOut; uint24 fee; address hooks; bytes32 parameters; }
    struct SwapParams {
        Hop[] hops;                 // 1..3 跳,首尾相接
        uint8 mode;                 // 0 ExactIn 1 ExactOut
        uint256 amount; uint256 limit;
        uint64 maxBlock; address bribeTo; uint256 bribe; bytes32 intentId;
    }
    event Swapped(bytes32 indexed intentId, uint256 amountIn, uint256 amountOut, address bribeTo, uint256 bribe);
    function swap(SwapParams calldata p) external payable returns (uint256 amountIn, uint256 amountOut);
}
```

与 `SwapGuard` 相同的过期、余额差、InTx 出价与无滞留规则;区别是不经路由器,直接调池(见实现说明 §2)。

## 实现说明

<details><summary>实现要点</summary>

### 1. `SwapGuard.execute` 步骤

1. `nonReentrant`(瞬态存储锁;BSC 是否支持 EIP-1153 待核实,不支持则用存储槽锁)、`!paused`、`msg.sender == executor`、`block.number <= g.maxBlock`、`msg.value == g.bribe`、`router ∈ ROUTERS`(构造时写入的不可变白名单:UR、V3 SwapRouter、V2 Router)。
2. 记录前态:本合约的 `tokenIn`、`tokenOut` 余额。
3. `transferFrom(msg.sender, this, g.amountIn)`。
4. `router.call(inner)`;失败冒泡 revert。
5. `amountOut = balOut(after) − balOut(before)`,要求 `≥ g.minOut`;`amountIn = balIn(before) + g.amountIn − balIn(after)`。
6. 把 `amountOut` 与剩余 `tokenIn` 全部转回 `msg.sender`;**无滞留**:本合约 `tokenIn`/`tokenOut` 余额回到前态,原生 BNB 余额 = 前态(出价转出后),否则 revert `Residual()`。
7. `bribeTo != 0` 时 `bribeTo.call{value: g.bribe}("")`,失败 revert。
8. 发 `Swapped`。

安全面:外部调用只有白名单路由器、两种代币与 `bribeTo`;合约不持有资金,即使 executor 私钥泄露,攻击者能动的也只是该钱包自己授权给合约的额度(与私钥泄露本身等价),且输出只能回到调用者。无代理、无 `delegatecall`、无 `selfdestruct`;新版本 = 新部署 + refdata `routes` 段切换。

### 2. `AtomicSwap`(可选扩展)

- V2:池地址按工厂 CREATE2 推导,输入直接 `transferFrom` 到池,本地算输出后 `pair.swap`。
- V3:池地址按 Pancake V3 PoolDeployer + init code hash 推导;"期望回调者 = 该池"写入瞬态存储后 `pool.swap`,`pancakeV3SwapCallback` 校验 `msg.sender`;ExactOut 按反向嵌套回调。
- Infinity CL:`hooks` 必须为 0 或在 owner 维护的白名单;经 Vault `lock` 回调完成 `swap → settle → take`,回调校验 `msg.sender == vault`;原生 BNB 与 WBNB 在合约内包装。
- 末态:调用者余额差校验 `limit`;本合约对全部触及代币无滞留;InTx 出价;`Swapped`。

### 3. 目录与源文件

| 路径 | 范围 | 职责 |
|---|---|---|
| `foundry.toml` | core | solc、evm 版本、优化器、fuzz/invariant 次数(锁定) |
| `src/SwapGuard.sol` | core | 包装合约 |
| `src/libraries/{TransientLock.sol, SafeTransfer.sol}` | core | 锁、转账 |
| `src/interfaces/` | core | ERC20、Permit2、路由器最小接口 |
| `src/AtomicSwap.sol`、`src/libraries/{PoolAddress.sol, V2Hop.sol, V3Hop.sol, InfinityHop.sol}` | 可选 | 原子 swap |
| `test/{unit, fork, invariant, attack}/` | core(`AtomicSwap` 用例随其可选) | 见测试 |
| `script/DeployGuard.s.sol` | core | CREATE2 确定性部署 |
| `script/DeployAtomic.s.sol` | 可选 | CREATE2 确定性部署 |
| `deployments/bsc.json` | core | 每个执行钱包的 `SwapGuard` 地址、字节码哈希、编译器版本、部署块、owner、路由器白名单 |
| `audits/` | core | 内部审查清单签署、静态分析报告、(`AtomicSwap`)外部审计 |

</details>

## 延迟与资源契约

成本是 gas,基准是 `forge snapshot`:

| 路由 | 范围 | 目标 | 基准名 |
|---|---|---|---|
| `SwapGuard` + V3 单池 | core | 相对直接调用 V3 SwapRouter 增加 ≤ 30,000 gas | `gas/guard_v3_single` |
| `SwapGuard` + UR(V3 → Infinity,含出价) | core | 相对 UR 直接调用 + 独立出价交易,总 gas 不高于后者 | `gas/guard_ur_infinity` |
| `AtomicSwap` V3 单池 | 可选 | ≤ UR 同路由 × 0.8(目标,首次快照为基线) | `gas/atomic_v3_single` |

任何提交 gas 回归 > 2% 阻断合入。调用方的 `gasLimit` 仍按预检(`estimate_gas`)× 1.25。

## 测试与验收

| 层 | 范围 | 内容 |
|---|---|---|
| 单元 | core | 每条 revert 路径:`Expired`(`block.number = maxBlock + 1`)、`msg.value` 不符、非 executor、路由器不在白名单、暂停、`minOut` 不满足、`Residual`、出价转账失败 |
| fork | core | 以公共 RPC 为源、固定块号的 anvil 分叉:UR(V3 路径、Infinity 含原生 BNB、WBNB/ETH/BTCB 中转)、V3 SwapRouter、V2 Router 各路由真实兑换;`amountOut` 与路由器直接调用一致 |
| 迟到上链 | core | 同一已签名交易在 `maxBlock` 之后打包(模拟在公共 mempool 滞留)→ revert、只花 gas、余额不变(对应 hft-chain-evm `LATE_LANDING`);`bribeTo = 0` 时不发生任何 BNB 转出 |
| 不变量(Foundry invariant) | core | 任意调用序列后合约不持有代币与 BNB;只有 executor 能成功 `execute`;输出只进 `msg.sender`;`maxBlock`、`minOut` 总被执行;≥ 1,000,000 次运行 |
| 攻击 | core | 恶意代币在 `transfer` 中重入、内层 calldata 把输出付给第三方(余额差为 0 → `minOut` revert)、非白名单路由器、`rescue` 转给非 owner——全部 revert |
| 差分 | 可选 | `AtomicSwap` 输出 == 链上 Quoter 报价(逐 wei)== UR 输出 |
| 静态分析 | core | Slither、Aderyn:零高危,中危逐条关闭记录在 `audits/` |
| 上线门槛 | core | 以上全绿;内部审查清单两名工程师签署;测试钱包每种路由器真实小额往返;然后 chain-core 才进入迁移切换后的正式实盘(不存在"不经 SwapGuard"的 swap 路径) |
| 上线门槛 | 可选 | `AtomicSwap` 影子 1 周(每个意图同时构造两种 calldata、都预检,输出一致率 100%);外部审计(阈值见开发备忘);按路由逐个切换,`SwapGuard + 路由器` 保留为回退 |

内部审查清单:权限面(每个外部函数谁能调)、外部调用目标来源、余额前后态覆盖的代币集合、原生 BNB 流向、事件字段与 hft-chain-evm 的日志校验一致、部署参数与 `deployments/bsc.json` 一致。

## 依赖

| 依赖 | 用途 | 约束 |
|---|---|---|
| solc 0.8.26(锁定) | 编译 | 升级须重跑全部测试与 gas 快照 |
| Foundry(锁定版本) | 测试、fuzz、invariant、部署 | 版本写进 `foundry.toml` 与 CI |
| Pancake V2/V3/Infinity、Permit2 接口 | 自写最小接口 | 不引入整个 periphery |
| Slither、Aderyn | 静态分析 | CI 中运行 |

禁止:可升级合约与任何代理模式、`delegatecall`、外部预言机。

## 延伸阅读

- [06 §2.5 bundle 与自有合约](../design/06-链上执行层.md)、[research/03 §1.7、§4.1、§5](../design/research/03-onchain-bsc-solana.md)
- [hft-chain-evm](../crates/hft-chain-evm/README.md)(calldata、生命周期、onboarding)、[apps/chain-core](../apps/chain-core/README.md)、[external-venue-protocol §3.3 码表](../docs/subsystems/external-venue-protocol.md)、[hft-amm](../crates/hft-amm/README.md)(可选扩展:`AtomicSwap` 的本地报价差分)

## 已知限制与后续工作

- `SwapGuard` 依赖路由器的正确性;路由器被攻破的风险由"不留资金 + 输出只回调用者 + 余额差校验"限制在单笔交易额度内。
- 不支持 Infinity Bin 池、Uniswap V4、聚合器(与 hft-chain-evm 路由范围一致)。
- 可选扩展:F 类 backrun(06 §7)需要"起止同币"的变体(触发条件:F 类策略立项)。
- Solana 没有对应合约:Solana 腿(可选扩展)直接调用 AMM 程序指令,过期由 `lastValidBlockHeight` 提供。

### 开发备忘

<details><summary>给实现者</summary>

**待核实**(读合约源码并在 anvil 分叉上执行):

1. BSC 是否支持 EIP-1153(`TLOAD/TSTORE`);不支持则锁改用存储槽,`evm_version` 随之调整。
2. Universal Router 在 `msg.sender` 为合约、付款人为合约时的 Permit2 路径(`payerIsUser`)语义;V3 SwapRouter 与 V2 Router 的 `deadline` 与收款人参数。
3. builder profile(可选扩展)立项时:builder 是否把"合约内转给其收款地址的 BNB"计为出价(48Club 文档写"直接转给 Builder Control EOA 的 BNB";合约内部转账是否等价需实测)。
4. `AtomicSwap`(可选扩展)立项时:Pancake V3 PoolDeployer 与 `POOL_INIT_CODE_HASH`、Infinity Vault `lock` 回调接口。

**需要老板决定**:每币授权 `cap` 的倍数、owner 冷钥保管方式(硬件钱包 + 纸质备份,或 2-of-3 多签);`AtomicSwap` 立项时再定外部审计的触发阈值(单笔名义、授权上限)。

**实现顺序**(按依赖):`SwapGuard` + 单元/不变量 → fork(V3 路由器)→ UR 与 Permit2 → 攻击测试 → 部署脚本与 `deployments/bsc.json`(先于 chain-core 正式实盘的首次真实发送)。

</details>
