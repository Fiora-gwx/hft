// SDK-105.6: Qty field without per_group
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Qty [min = 0, max = 10, default = 0],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
