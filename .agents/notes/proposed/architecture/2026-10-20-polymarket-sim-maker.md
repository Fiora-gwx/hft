# Agent Note: Polymarket 模拟盘与挂单策略(P4 先于 P2/P3)

Status: proposed

## Problem

负责人 2026-10-20 调整顺序:先做行情稳定性与模拟,策略在模拟盘连续两天没有问题再接实盘(钱包、签名、下单放后)。[接入提案](2026-10-19-polymarket-venue.md)的 P4 依赖 P3 的 `Settlement`,而模拟盘要跑两天就必须解决三件事:

1. **结算。** 窗口结束后持仓按结果变为 1 或 0 USDC;不结转,槽位改绑会被 `BindSlot` 拒绝(`GroupHasPosition`),模拟账户也算不出盈亏。market 频道不推结算消息(2026-10-20 录制里只有 `book`、`price_change`、`last_trade_price`、`tick_size_change`)。
2. **挂单成交。** hft-sim 默认 `TradeThrough`:挂单只在成交价穿过挂价时成交。Polymarket 的 1 美分档位排队很长,成交几乎都发生在挂价本身,`TradeThrough` 会把挂单策略的成交量算成接近 0。
3. **策略。** hft 里还没有 Polymarket 交易策略。研究结论(仓库外报告《Polymarket BTC5分钟 期权视角与高频策略》,2026-10-20,基于 2026-08 的盘口与成交数据):只有挂单、且只在 σ ≥ 0.5–0.7 bp/√s 时为正(每份 +0.1–0.2 美分);最后一分钟剩余标准差 < 1 bp 时为负;币安 ≥1σ 变动后 0.15 s 内撤单。

## Proposal

1. **结算命令。** 新中性命令 `Settle{venue, inst, end_s, payout}`(payout ∈ {0, 1} 的价格单位),由发现进程在 Gamma 报告结算后提交,先记 journal 再生效:sequencer 按 payout 结转该 inst 全部账户的仓位(已实现盈亏、现金),sim-gw 把模拟库存换成 USDC;之后才允许改绑。实盘(P3)复用同一命令,gw 侧另做 redeem 对账。
2. **排队成交模型。** hft-sim 增加 `maker_fill = Queue`:挂单时记该价位已显示数量为"前方排队量";该价位的成交先消耗前方排队量,再成交本单;该价位数量减少(撤单)按比例减少前方排队量。只用 hft-sim 自己的簿副本与成交,不读外部数据。
3. **策略 `pm-maker`。** 订阅 Polymarket 当前窗口的 Up/Down 槽位与币安 BTC 现货 BBO;只挂 post-only 限价单(Up 与 Down 各挂买一价);σ(币安 1 s 收益的 30 分钟实现波动 × 1.39)低于门槛、剩余标准差 < 1 bp、或币安出现 ≥ 1σ 变动时撤单;单窗口库存上限;窗口结束前 60 s 停止挂单。参数按研究值起步。
4. **装配。** `--dry-run` 同时接币安现货与 Polymarket 两个 `live_feed` venue,模拟账户只持 USDC;结算命令与 `BindSlot` 由同一个发现进程(`bind_slots.py`)提交。费用:挂单免费,返佣不计入(偏保守)。

## Alternatives considered

- **策略在窗口结束前把库存吃单平掉,不做结算。** 吃单费 0.07·p·(1−p) 每份,在 0.5 附近 1.75 美分,大于研究的挂单边际(0.1–0.2 美分),策略会被平仓成本吃掉;而且收盘前流动性最差。
- **由 venue 从行情推断结算(收盘后盘口收敛到 0/1)。** 结算由预言机在收盘约 147 s 后给出,行情只是近似;推断错误会写进账本,不可接受。
- **沿用 `TradeThrough`。** 成交量接近 0,两天模拟得不出结论。

## Acceptance criteria

- 结算:一个完整窗口里模拟账户的 USDC 变动 = Σ(成交现金)+ 持仓 × payout,误差 0;结算后 `BindSlot` 改绑成功。
- 排队模型:对 2026-10-20 录制的同一窗口,模型成交量与"按录制逐笔排队重放"的离线结果一致(差 ≤ 1%)。
- 两天模拟:hft-core 不中断;每个窗口都改绑与结算;日报给出成交量、每份边际、盈亏、库存分布;没有一次因行情缺口导致的挂单滞留(簿 Stale 时撤单)。
- `replayer verify` 对模拟 journal 逐字节通过。

## Risks

- 排队模型高估成交(前方撤单比例假设):用保守的按比例规则,并与离线重放对照。
- 模拟延迟取抽样模型,对 0.15 s 级的撤单竞争只作方向性参考([hft-sim 适用范围](../../../../crates/hft-sim/README.md))。
