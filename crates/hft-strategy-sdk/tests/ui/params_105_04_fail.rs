// SDK-105.4: default = 11, max = 10
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Count [min = 0, max = 10, default = 11],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
