use crate::common::Fixture;
use xtask::exec::ScriptExec;

#[test]
fn tool_192_replay_wrapper_runs_binary_and_maps_exit_codes() {
    let f = Fixture::empty("replay-wrapper");
    f.write("sample.jrn", "input validated by replayer");
    for (code, want) in [(0, 0), (1, 1), (2, 2), (3, 1), (4, 1), (-1, 2)] {
        let mut exec = ScriptExec::default();
        exec.default_code = code;
        let args = ["sample.jrn", "--from-seq", "20", "--to-seq", "30"].map(str::to_owned);
        assert_eq!(xtask::replay_verify::run_with(&f.root, &args, &mut exec), want);
        assert_eq!(exec.calls.len(), 1);
        assert_eq!(exec.calls[0].program, "cargo");
        assert_eq!(
            &exec.calls[0].args[..10],
            [
                "run",
                "--locked",
                "--profile",
                "ci",
                "-p",
                "replayer",
                "--",
                "verify",
                "--journal",
                &f.path("sample.jrn").display().to_string()
            ]
        );
        assert_eq!(&exec.calls[0].args[10..], ["--from-seq", "20", "--to-seq", "30"]);
    }
    let mut exec = ScriptExec::default();
    assert_eq!(
        xtask::replay_verify::run_with(&f.root, &["sample.jrn".into(), "--unsupported".into()], &mut exec),
        2
    );
    assert!(exec.calls.is_empty());
}
