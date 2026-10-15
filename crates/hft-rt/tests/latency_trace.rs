use hft_rt::latency_trace::*;
#[test]
fn rt_002_latency_overflow_preserves_fifo_and_reports_tail_loss() {
    let (writer, mut reader) = channel(2).unwrap();
    install(writer).unwrap();
    let c = Context {
        input_seq: 19,
        order: (7 << 32) | 4,
        ..Context::default()
    };
    set_context(c);
    at(Stage::ProcessStart, 100, [0; 4]);
    at(Stage::ProcessEnd, 110, [0; 4]);
    at(Stage::ProcessEnd, 120, [0; 4]);
    assert_eq!(reader.dropped(), 1);
    let first = reader.rx.try_pop().unwrap();
    assert_eq!((first.serial, first.tsc, first.context), (1, 100, c));
    assert_eq!(reader.rx.try_pop().unwrap().tsc, 110);
    at(Stage::ProcessStart, 130, [0; 4]);
    let resumed = reader.rx.try_pop().unwrap();
    assert_eq!((resumed.serial, resumed.dropped), (4, 1));
    assert!(reader.rx.try_pop().is_none());
}
