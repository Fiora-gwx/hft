// SDK-105.9: since 1, 2, 1
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 2, fields = PF) {
        a: Count [max = 1],
        b: Count [max = 1, since = 2],
        c: Count [max = 1],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
