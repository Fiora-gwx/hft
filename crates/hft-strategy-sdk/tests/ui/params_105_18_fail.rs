// SDK-105.18: minor = RelPct(101)
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Count [max = 10, minor = RelPct(101)],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
