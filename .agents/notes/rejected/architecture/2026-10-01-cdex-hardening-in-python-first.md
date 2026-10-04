# Agent Note: 阶段 1' 先在 Python cdex 内落地链上安全规则,Rust chain-core 推迟到阶段 3

Status: rejected — 用户决定 cdex 免费版直接做在新框架上

## Problem

第四轮让 chain-core 以 free profile 在阶段 1' 先服务现有 Python cdex([chain-core-free-profile](../../proposed/architecture/2026-10-01-chain-core-free-profile.md))。全局评审(耦合评审 C10)指出:阶段 1' 的 Rust chain-core 依赖工程师 A 第 3–4 周才交付的 `hft-types`/`hft-event`/`hft-journal`,还需要一条 cdex ↔ chain-core 的临时接口——它本身是新的故障面(传输错误、会话、重放),而 cdex 当前的损失来自四类正确性缺陷:把传输错误当失败(9/24 403)、`pending` nonce 不可信、交易没有链上过期(迟到上链)、单源回执(幻影成交)。这四类在 Python 里就能修,修好之前每多等一周都在真金白银地暴露。

## Proposal

第六轮裁决(2026-10-01 全局评审,主控记录)A3、B-7、B-27;规格在 `python/cdex_hardening`(已删除,有价值的内容并入 [hft-chain-evm](../../../../crates/hft-chain-evm/README.md#相对旧框架链上引擎的改进点))(phase 1'):

1. 工程师 B 在现有 Python cdex 内落地新框架的四条安全规则 + SwapGuard:
   - 传输错误只进 `Unknown`,状态只由链上证据改变([defensive-patterns](../../../../docs/defensive-patterns.md#传输错误--业务失败));
   - 本地 nonce 分配器:在途表,只在包含/作废后推进;
   - 每笔交易经 SwapGuard 合约:`maxBlock = 发送块号 + N`、`minOut`([tx-onchain-expiry](../../proposed/architecture/2026-10-01-tx-onchain-expiry.md));
   - 回执来源 ≥ 3 个独立运营商(按封禁/限流域划分),≥ 2 个一致才算成交;可用独立来源 < 2 持续 N 块 → P0 转人工。
2. 链上成交量一律是基础资产(代币)数量:Buy 取 `amount_out`,Sell 取 `amount_in`;`risk_reducing` 意图另设 gas 硬上限,超过 → P0 转人工。
3. 结构化追加日志(JSONL,字段与 `TxLifecycle` 对齐),阶段 3 可导入 chain journal 与 Parquet。
4. Rust chain-core / `hft-chain-evm` 推迟到阶段 3(随 cdex 策略迁入 hft-core),按 cold tier 实现 free profile;external-venue-protocol §9 的临时接口删除。

## Alternatives considered

**阶段 1' 建 Rust chain-core,经临时接口服务 cdex(第四轮方案)。** 依赖未交付的 crate、引入新接口故障面、需要两个进程协同运维;修复上线时间晚于 Python 内修复数周。

**等阶段 3 一并重写,期间不改 cdex。** 四类缺陷都已造成过实际损失([postmortem](../../../../docs/postmortem/README.md)),继续暴露到第 13 周以后不可接受。

**只修其中最紧急的一两条。** 四条规则互相依赖:没有链上过期,`Dropped` 判定不可靠;没有多源回执,`Unknown` 无法收敛;没有本地 nonce,重发会撞号。

## Acceptance criteria

- cdex 测试账户 → 正式账户各 7 天:传输错误被当作失败 0 次、nonce 冲突导致的重复 swap 0 次、过期后成交 0 次、未解释的 `Unknown` 0 次。
- SwapGuard 合约 Foundry 测试通过(`block.number > maxBlock` 必 revert、`minOut` 不满足必 revert),小额实盘验证后启用。
- JSONL 日志能被阶段 3 的导入器无损转换(字段对照表写在 cdex_hardening README)。

## Risks

- 规则写两遍(Python 一遍、阶段 3 Rust 一遍):以 JSONL 与 `TxLifecycle` 字段对齐、共用同一套 DST 场景描述来降低漂移。
- Python cdex 的并发与结构问题(`asyncio.run` 串行、多参数源)仍在,本 Note 不解决;它们随阶段 3 迁移消失。
- 免费公共 RPC 的独立运营商数量可能不足 3:不足时按 B-27 进入 P0 转人工,不推断。
