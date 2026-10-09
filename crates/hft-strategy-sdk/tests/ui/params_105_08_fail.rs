// SDK-105.8: Enum(["a", "b"]) with default = 2
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Enum(["a", "b"]) [default = 2],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
