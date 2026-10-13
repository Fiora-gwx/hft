//! Full real-file pipeline; only S3 delivery and the already-built executable location are injected.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use hft_event::EventKind;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use xtask::EnvError;
use xtask::exec::{Cmd, Exec, Output, SysExec};

use crate::fixture;
use crate::long_model::heavy as long_model;

struct Pipeline {
    objects: BTreeMap<String, Vec<u8>>,
    codec: SysExec,
    downloads: usize,
    reports: Vec<Value>,
}
impl Exec for Pipeline {
    fn run(&mut self, cmd: &Cmd) -> Result<Output, EnvError> {
        match cmd.program.as_str() {
            "aws" => {
                assert_eq!(&cmd.args[..2], ["s3", "cp"]);
                fs::write(&cmd.args[3], &self.objects[&cmd.args[2]]).unwrap();
                self.downloads += 1;
                Ok(Output::default())
            }
            "zstd" => self.codec.run(cmd),
            "cargo" => {
                assert_eq!(
                    &cmd.args[..9],
                    [
                        "run",
                        "--locked",
                        "--profile",
                        "ci",
                        "-p",
                        "replayer",
                        "--example",
                        "replay_probe",
                        "--"
                    ]
                );
                // CI builds this example before tests. No test starts a compiler.
                let exe = std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("examples")
                    .join(format!("replay_probe{}", std::env::consts::EXE_SUFFIX));
                let result = Command::new(exe).args(&cmd.args[9..]).output().unwrap();
                let stdout = String::from_utf8(result.stdout).unwrap();
                self.reports.push(serde_json::from_str(&stdout).unwrap());
                Ok(Output {
                    code: result.status.code().unwrap(),
                    stdout,
                    stderr: String::from_utf8(result.stderr).unwrap(),
                })
            }
            other => panic!("unexpected program {other}"),
        }
    }
}

#[test]
fn rpl_281_generated_bundle_fetch_replays_full_and_checkpoint_window() {
    let f = long_model::three_hours();
    let f = &*f;
    let log = fixture::read(f);
    let from = long_model::h2(&log);
    let root = f.root.join("bundle-transport");
    fs::create_dir(&root).unwrap();
    let cache = fixture::Fixture {
        root: root.clone(),
        file: root.join("unused"),
    };
    let mut pipeline = Pipeline {
        objects: BTreeMap::new(),
        codec: SysExec::new(&root),
        downloads: 0,
        reports: Vec::new(),
    };
    let mut sources: Vec<PathBuf> = fs::read_dir(f.file.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    sources.extend([
        f.root.join("ckpt-1h/tsa.ckpt"),
        f.root.join("checkpoints/tsa.ckpt"),
    ]);
    sources.sort();
    assert_eq!(
        sources.len(),
        10,
        "four sealed segments and two Model checkpoints"
    );
    let mut originals = Vec::new();
    let mut manifest =
        "format = \"files-v1\"\nstrategy_set = \"testkit-replay-v1\"\njournal = \"prod/seq\"\n".to_owned();
    for path in &sources {
        let relative = path
            .strip_prefix(&f.root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let data = fs::read(path).unwrap();
        let hash = format!("{:x}", Sha256::digest(&data));
        let uri = format!("s3://synthetic-test-store/hft-fixtures/generated/{hash}.zst");
        manifest.push_str(&format!(
            "\n[[files]]\npath = {relative:?}\nuri = {uri:?}\nsha256 = {hash:?}\nbytes = {}\n",
            data.len()
        ));
        let compressed = root.join("member.zst");
        let out = Command::new("zstd")
            .args(["--quiet", "--keep", "--force"])
            .arg(path)
            .arg("-o")
            .arg(&compressed)
            .output()
            .expect("CI must install zstd");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        pipeline.objects.insert(uri, fs::read(compressed).unwrap());
        originals.push((relative, data));
    }
    manifest.push_str(&format!("\n[[checks]]\nname = \"verify\"\nadvice = \"journal\"\n\n[[checks]]\nname = \"model\"\nadvice = \"recompute\"\n\n[[checks]]\nname = \"checkpoint-window\"\nadvice = \"recompute\"\ncheckpoint = \"ckpt-1h/tsa.ckpt\"\nfrom_seq = {from}\n"));
    fs::create_dir_all(root.join("fixtures/replay")).unwrap();
    let pointer = "fixtures/replay/generated-three-hours.pointer.toml";
    fs::write(root.join(pointer), &manifest).unwrap();
    assert_eq!(xtask::fixtures::fetch::run_with(&root, &mut pipeline), 0);
    assert_eq!(pipeline.downloads, originals.len());
    pipeline.objects.clear();
    assert_eq!(xtask::fixtures::fetch::run_with(&root, &mut pipeline), 0);
    assert_eq!(pipeline.downloads, originals.len(), "cache reused without S3");
    assert_eq!(
        xtask::replay_verify::run_with(&root, &[pointer.into()], &mut pipeline),
        0
    );
    assert_eq!(pipeline.reports.len(), 3);
    let all_advice = log.recs.iter().filter(|r| r.kind == EventKind::ADVICE).count();
    let window_advice = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::ADVICE && r.seq > from)
        .count();
    assert_eq!(
        pipeline.reports[1]["summary"]["model_advice_compared"],
        all_advice
    );
    assert_eq!(
        pipeline.reports[2]["summary"]["model_advice_compared"],
        window_advice
    );
    assert_eq!(
        pipeline.reports[2]["summary"]["checkpoint_key"][0]["strategy_instance"],
        "tsa"
    );
    for field in ["inputs_replayed", "outputs_compared", "snapshots_checked"] {
        assert_eq!(
            pipeline.reports[0]["summary"][field],
            pipeline.reports[1]["summary"][field]
        );
    }
    let dest = root.join("target/fixtures-cache/generated-three-hours");
    for (path, original) in &originals {
        assert_eq!(fs::read(f.root.join(path)).unwrap(), *original);
        assert_eq!(fs::read(dest.join(path)).unwrap(), *original);
    }
    let changed = dest.join(&originals[0].0);
    let mut corrupt = originals[0].1.clone();
    corrupt[0] ^= 1;
    fs::write(changed, corrupt).unwrap();
    assert_eq!(
        xtask::replay_verify::run_with(&root, &[pointer.into()], &mut pipeline),
        2
    );
    assert_eq!(pipeline.reports.len(), 3, "corrupt cache rejected before replay");
    if let Some(dir) = std::env::var_os("HFT_REPLAY_EVIDENCE_DIR") {
        let evidence = PathBuf::from(dir).join(f.root.file_name().unwrap());
        fs::create_dir_all(&evidence).unwrap();
        fs::write(evidence.join("generated-bundle.pointer.toml"), manifest).unwrap();
        fs::write(evidence.join("bundle-pipeline.json"), json!({"status":"generated_not_frozen", "source_files":originals.len(),
            "downloads":pipeline.downloads, "window_from_seq":from, "reports":pipeline.reports, "corrupt_cache_exit":2}).to_string()).unwrap();
    }
    drop(cache);
}
