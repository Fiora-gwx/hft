"""Chinese, evidence-bound output; all recommendations are research candidates."""

from datetime import UTC, datetime


def number(value, digits=2):
    return "—" if value is None else f"{value:.{digits}f}"


def pair_label(record):
    return record["pair"].replace("|", " ↔ ")


def render(result: dict) -> str:
    provenance = result["provenance"]
    rows = result["pairs"]
    good = [r for r in rows if not r.get("excluded")]
    traded = [r for r in good if r["taker"]["trades"]]
    profitable = sum(r["taker"]["pnl_after_liquidation_quote"] > 0 for r in traded)
    horizons = provenance["config"].get("carry_horizons_days", [1, 3, 7])
    lines = [
        "# 策略离线校准报告",
        "",
        (
            f"配置情景下，{len(rows)} 个候选组合中 {len(good)} 个通过价格与数据检查；"
            f"{len(traded)} 个发生套利模拟成交，其中 {profitable} 个在扣除期末退出成本后为正。"
            "资金费日汇总的筛选结果与有时间戳数据的模拟结果分别列示。"
        ),
        "",
        (
            "本报告用已记录行情筛选研究参数。金额单位为计价币，费率为配置中的情景费率，未核实账户实际费率。"
            "每个交易对单独运行，结果不能相加作为一个组合的收益；同一 inst 在策略实例内不能重复分组。"
        ),
        "",
        "## 数据与可验证范围",
        "",
        f"- 输入 `{provenance['quotes']['path']}`，SHA-256 `{provenance['quotes']['sha256']}`。",
        f"- 实际交易所 {', '.join(provenance['venues'])}；共 {len(rows)} 个跨所组合。",
        f"- 数据范围 {provenance['start_utc']} 至 {provenance['end_utc']}，使用逐秒最后一条 BBO。",
        (
            "- 相同秒桶只配对两边都有的记录，决策时刻取双方 last_ns 较晚值，不向前填充、不读取未来行情。"
            "报价机会年龄用真实 last_ns 差值，严格遵守小于 max_quote_age_ms；没有 pong/连接日志，不能证明 quote_ok。"
        ),
        (
            "- 本地 funding_raw.csv 只有日合计和结算次数。结算次数不能反推周期，日合计不能充当当时已知预测。"
            "资金费汇总没有日期边界，不能确认与 BBO 区间一致。资金费时间序列及实际入退场仅在另行提供 funding-events CSV 后产生。"
        ),
        (
            "- 1 秒快照看不到秒内价差、订单队列、撤单、网络时延、Unknown、拒单、非对称成交和 Reverse 修复成本。"
            "两腿 IOC 都以固定不利限价全部成交只是情景假设；未模拟隐藏深度和价格冲击。"
        ),
        (
            "- 期末使用最后共同快照估算立即平仓，包含两腿手续费与价格余量，但不声称该深度足以平完。"
            "套利 PnL 不含持仓资金费，carry 结算数据完整性需由采集方确认。"
        ),
        (
            "- 缺少历史 tick/lot/min-notional、资金及杠杆快照时，必须显式配置假设。"
            "canonical_id 相同只代表候选映射，不能证明代币化股票或黄金合约的交割、倍率和交易时段等价。"
        ),
        "",
        "## 套利结果",
        "",
        (
            "净边已扣两腿价格加权 taker 费和 slip。分位数为浮点展示，订单再次通过整数信号校验。"
            "命中次数是采样秒数，非独立机会次数；容量是两方向顶档可配对名义金额的较大值，非可持续吞吐。"
        ),
        "",
        "| 标的 / 两腿 | 覆盖% | 净边 P50/P95/P99 bps | >阈值秒数 / 年龄合格秒数 | 顶档容量 P50 | 成交对数 | 开仓/平仓现金流 | 含期末退出 PnL |",
        "|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for r in sorted(good, key=lambda x: (x["symbol"], x["pair"])):
        t, edge = r["taker"], r["taker"]["net_edge_bps"]
        lines.append(
            f"| {r['symbol']} / {pair_label(r)} | {r['coverage_pct']:.1f} | "
            f"{number(edge['p50'])}/{number(edge['p95'])}/{number(edge['p99'])} | "
            f"{t['above_threshold']} / {t['age_eligible']} | {number(t['top_capacity_quote']['p50'])} | "
            f"{t['trades']} | {t['spread_cash_quote']:.4f} | {t['pnl_after_liquidation_quote']:.4f} |"
        )
    lines += [
        "",
        "## 资金费筛选",
        "",
        (
            "下表是整段历史日合计下的恒定费率情景，不是无前视回测。年化按单腿名义金额计；"
            "两边钱包各全额备资时，资本回报约为该值的一半。实际收益还受基差变化、保证金、强平及借贷约束。"
        ),
        "",
        f"| 标的 / 两腿 | 做空腿 | 收取差额 bps/日 | 配置净年化% | 往返成本 bps | 回本天数 | 100名义持有{'/'.join(map(str, horizons))}天净额 | 顶档容量 P50 |",
        "|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    funding = [r for r in good if "net_apr" in r["carry_daily"]]
    funding.sort(key=lambda r: r["carry_daily"]["net_apr"], reverse=True)
    for r in funding:
        c = r["carry_daily"]
        pnl = c["constant_rate_horizon_pnl_per_100_quote"]
        lines.append(
            f"| {r['symbol']} / {pair_label(r)} | {c['short_leg']} | {c['income_day_bps']:.2f} | "
            f"{c['net_apr'] * 100:.2f} | {c['round_trip_cost_bps']:.2f} | {number(c['breakeven_days'])} | "
            f"{' / '.join(number(v, 4) for v in pnl.values())} | {number(c['capacity_quote'])} |"
        )
    actual = [r for r in good if "pnl_after_liquidation_quote" in r["carry"]]
    if actual:
        lines += [
            "",
            "### 时间戳模式的模拟结果",
            "",
            "只使用当时可见的预测。实际结算完整性未经证明，资金费缺记录会使收益失真。",
            "",
            "| 标的 / 两腿 | 成交切片数 | 实际结算现金 | 借贷成本 | 含期末退出 PnL | 期末 leg0 数量（raw） |",
            "|---|---:|---:|---:|---:|---:|",
        ]
        for r in actual:
            c = r["carry"]
            lines.append(
                f"| {r['symbol']} / {pair_label(r)} | {c['trades']} | {c['funding_cash_quote']:.4f} | "
                f"{c['borrow_cost_quote']:.4f} | {c['pnl_after_liquidation_quote']:.4f} | {c['final_position_raw'][0]} |"
            )
    lines += [
        "",
        "## 每个标的的参数研究范围",
        "",
        (
            "范围用于下一轮模拟账户实验，不自动写入生产配置。数据只有一个交易日，没有独立验证集。"
            "先核实账户费率及合约元数据，再用新的逐笔样本检验这些候选。"
        ),
        "",
        "| 标的 | 套利 min_edge / slip（bps） | carry fee_horizon / enter_apr / exit_apr | 初始每腿金额上限 | 研究顺序 |",
        "|---|---|---|---:|---|",
    ]
    candidates = []
    for symbol in sorted({r["symbol"] for r in good}):
        group = [r for r in good if r["symbol"] == symbol]
        best = max(group, key=lambda r: r["taker"]["net_edge_bps"]["p99"])
        edge99 = best["taker"]["net_edge_bps"]["p99"]
        taker = "保持禁用；费用后 P99≤0" if edge99 <= 0 else "min_edge 1–5；slip 0–1，逐笔复核"
        cg = [r for r in funding if r["symbol"] == symbol and r["carry_daily"]["income_day_bps"] > 0]
        if cg:
            # Choose among sufficiently observed pairs first; a sparse higher-rate pair must
            # not hide a viable, better-covered pair of the same canonical symbol.
            covered = [r for r in cg if r["coverage_pct"] >= 80]
            best_c = max(
                covered or cg,
                key=lambda r: (
                    r["carry_daily"]["income_day_bps"] - r["carry_daily"]["round_trip_cost_bps"] / 3
                ),
            )
            c = best_c["carry_daily"]
            days = c["breakeven_days"]
            if days is not None and days < 7:
                horizon = max(1, min(7, int(days) + 1))
                carry = f"{horizon}–7 天；10–30% / 0–5%"
                score = c["income_day_bps"] - c["round_trip_cost_bps"] / 3
                order = "先做资金费逐笔采集" if score > 0 else "低优先级，三日成本未覆盖"
                if score > 0 and best_c["coverage_pct"] >= 80:
                    candidates.append((score, symbol, best_c["pair"]))
            else:
                carry, order = "7 天内费用未覆盖", "暂缓"
        else:
            carry, order = "缺数据或差额不利", "补采数据"
        cap = best["taker"]["top_capacity_quote"]["p05"]
        cap_text = number(min(100, cap * 0.1)) if cap is not None else "待核实"
        lines.append(f"| {symbol} | {taker} | {carry} | {cap_text} | {order} |")
    lines += [
        "",
        (
            "金额建议取顶档 P05 的 10% 与 100 的较小者，只是保守实验起点；还必须满足真实最小订单。"
            "套利 depth_frac 可比较 10%–50%，单笔不超过表中金额，持仓 cap 比较单笔的 1–5 倍。"
            "cooldown 保留 100–1000 ms；max_quote_age_ms 保留 5–20 ms，不能为了让秒级样本成交而放宽到 1 秒。"
            "carry entry_window 1–5 分钟、min_lead 10–30 秒、funding_max_age 10–30 秒、max_cross 0–1 bps"
            "都需要逐次预测与时钟证据；当前数据不能校准这些时间参数。"
        ),
        "",
        "## 先启用哪些标的",
        "",
    ]
    candidates.sort(reverse=True)
    if candidates:
        lines.append(
            "优先补采并进入模拟账户研究的候选为 "
            + "、".join(x[1].upper() for x in candidates[:5])
            + "，依据为三日费用摊销后差额为正且双边覆盖不低于 80%。这是样本内筛选，"
            "应确认合约等价、实际费用、结算持续性和容量后，每个 inst 只选一个 group。"
        )
    else:
        lines.append("当前费用与数据条件下，没有达到优先筛选条件的组合。")
    lines += [
        (
            "正式交易暂不启用任何标的。现有日汇总不能证明 carry 的预测信号可交易，"
            "秒级 BBO 不能证明套利双腿成交；仍需原有模拟、Shadow 和上线门槛。"
        ),
        "",
        "## 缺口与复现",
        "",
        "- S3 原始 tick 获取结果由操作者另行记录；工具只读本地输入，没有任何联网或交易接口。",
        "- Bitget 若未列于实际交易所列表，本报告不产生该所结果。",
        (
            "- summary.json 保存完整配置、输入哈希、源代码哈希、阈值命中数、费率敏感性和拒绝原因；"
            "trades.csv 保存两个策略的假设成交；funding_series.csv 保存时间戳模式下的预测年化序列。"
        ),
        "- 资金费时间戳模式的汇总也不自动证明结算记录完整；不得把缺结算当成零费率。",
        "",
    ]
    excluded = [r for r in rows if r.get("excluded")]
    if excluded:
        lines += ["| 排除组合 | 原因 |", "|---|---|"]
        lines.extend(f"| {r['symbol']} / {pair_label(r)} | {r['excluded']} |" for r in excluded)
    lines += ["", f"报告生成时间 {datetime.now(UTC).isoformat()}。", ""]
    return "\n".join(lines)
