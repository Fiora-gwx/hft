// SDK-105.1: TP as declared
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 2, fields = PF) {
        open_th: Rate [min = 0, max = 100_000_000, default = 5_000_000, per_group, relax = Down, minor = RelPct(10)],
        cooldown_ms: DurationMs [min = 0, max = 600_000, default = 20_000, relax = Down, minor = Abs(1_000)],
        pct_bin: Rate [min = 10_000, max = 1_000_000, default = 100_000, per_group, structural],
        max_qty: Qty [min = 0, max = 1_000_000_000_000, default = 0, per_group, relax = Up, minor = Any],
        enabled: Bool [default = 1, manual_only, minor = Any],
        mode: Enum(["a", "b", "c"]) [default = 0, since = 2, manual_only],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
