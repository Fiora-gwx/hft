// SDK-105.15: Enum field with minor = Abs(1)
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Enum(["a", "b"]) [default = 0, minor = Abs(1)],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
