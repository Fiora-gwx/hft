# Agent Note: resolve_broken 只在解除 HARD 时触发;恢复期抑制按多出那一腿所在 venue 判定

Status: proposed

## Problem

上一轮把"解除时仍有未对冲量 → Broken"挂在每一次 `Resume` 与干净重启的自动解除上:容差内的残余也可能被设成 `Broken` 并带上账户级 `HedgeFailure`,同时抵消了"放行时重设截止"的修法。恢复期的补腿、外来成交默认处理与自动有界平仓又绑在整个账户的启动闸门上,一个 venue 迟迟对不上账,别的 venue 上的裸腿就一直没人处理,也没有上界与告警;Broken 的计时在闸门未放行期间是否继续,testing 与 hft-exec 两处写法相反([X1 评审](../../../../design/_work/review-2026-10-03-M0e-X1资金正确性.md) P1-A、P1-B、P2-15)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-12;规范见 [round8f 简报](../../../../design/_work/round8f-spec-brief.md) F1(a)(b))。判定归 [hft-exec](../../../../crates/hft-exec/README.md),调用点归 [hft-sequencer](../../../../crates/hft-sequencer/README.md),处置归 [incident-response A](../../../../docs/runbooks/incident-response.md#a-裸敞口nakedexposure断腿broken)。

1. `resolve_broken` 只在删除带 HARD 的 Kill 记录时产生;判定阈值与补腿相同(该 group 裸量 > `tol` 且名义高于尘埃线),跳过 `manual_hold` 的 cell。非 HARD 的 `Resume{Kill}`、其他锁存的解除与 Recovery 的自动解除都不产生它,放行后的补腿与外来成交默认处理按新窗口处理。
2. 补腿、残余修剪、外来成交默认处理与自动有界平仓的开始条件改为"多出那一腿所在 venue 的 `hedge_gate_ok` 满足 ∧ 该 venue 的 `RECONCILED` 已置位";条件不满足期间 `broken_auto_fix_ms` 与外来成交截止都不计时。
3. 账户启动闸门未放行而存在 Broken 或超过补腿阈值的裸量,持续超过 `gate_naked_alarm_ms`(300 000)→ `Note{ExecGateHeldNaked}`(P0)。

## Alternatives considered

**保留"每次 Resume 都判 Broken",只加尘埃过滤。** 上一轮的做法;容差内、尘埃线以上的残余仍会被设成 Broken 并封住整个账户的开仓。

**恢复期抑制仍按整账户闸门,只加超时。** 超时后在未对账的 venue 上补腿,可能按过期的仓位下单;按 venue 判定只放行已对账的那一侧。

**闸门未放行期间照常计时(只当作开始前的等待)。** 崩溃前已 Broken 很久的 cell 在放行瞬间就自动平仓,人没有完整窗口;评审 P2-15 择"不计时"。

## Acceptance criteria

- DST:`Resume{Recon}` 时残余在容差内 → 不出现 Broken;解除 HARD 时裸量超过补腿阈值且不在 `manual_hold` → 恰好该 group 进入 Broken,`manual_hold` 的 cell 不进入。
- 冷启动后只有 venue A 对账完成:A 上多出的腿开始补腿或自动平仓,B 上的不开始也不计时;B 放行后给出完整的 `broken_auto_fix_ms`。
- 闸门未放行而有超阈值裸量 300 s → 恰好一条 `ExecGateHeldNaked`。

## Risks

- 按 venue 放行时,另一 venue 的仓位可能尚未核对;补腿只作用于已对账 venue 上多出的那一腿,另一腿不动。
- `gate_naked_alarm_ms` 取值待标定:冷启动对账慢的 venue 可能让它在正常重启中出现。
