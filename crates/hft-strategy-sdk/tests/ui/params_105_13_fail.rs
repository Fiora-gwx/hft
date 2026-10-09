// SDK-105.13: structural field with minor = Abs(1)
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Count [max = 10, structural, minor = Abs(1)],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
