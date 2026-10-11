//! A real zstd codec and filesystem; only the S3 download is replaced with a local object store.

use std::collections::BTreeMap;
use std::fs;

use sha2::{Digest, Sha256};
use xtask::EnvError;
use xtask::exec::{Cmd, Exec, Output, SysExec};

use crate::common::Fixture;

struct Store {
    objects: BTreeMap<String, Vec<u8>>,
    calls: Vec<Cmd>,
    codec: SysExec,
    unavailable: bool,
    empty_success: bool,
}
impl Store {
    fn new(f: &Fixture) -> Self {
        Self {
            objects: BTreeMap::new(),
            calls: Vec::new(),
            codec: SysExec::new(&f.root),
            unavailable: false,
            empty_success: false,
        }
    }
}
impl Exec for Store {
    fn run(&mut self, cmd: &Cmd) -> Result<Output, EnvError> {
        self.calls.push(cmd.clone());
        if cmd.program != "aws" {
            return self.codec.run(cmd);
        }
        assert_eq!(&cmd.args[..2], ["s3", "cp"]);
        assert_eq!(&cmd.args[4..], ["--only-show-errors", "--no-progress"]);
        if self.unavailable {
            return Err(EnvError("aws not installed".into()));
        }
        if self.empty_success {
            return Ok(Output::default());
        }
        match self.objects.get(&cmd.args[2]) {
            Some(b) => {
                fs::write(&cmd.args[3], b).unwrap();
                Ok(Output::default())
            }
            None => Ok(Output {
                code: 1,
                stderr: "object missing or access denied".into(),
                ..Output::default()
            }),
        }
    }
}

fn uri(name: &str) -> String {
    format!("s3://test-bucket/hft-fixtures/{name}/object.zst")
}
fn pointer(f: &Fixture, name: &str, data: &[u8]) {
    f.write(
        &format!("fixtures/replay/{name}.pointer.toml"),
        &format!(
            "uri = {:?}\nsha256 = \"{:x}\"\nbytes = {}\nkind = \"seq-journal\"\n",
            uri(name),
            Sha256::digest(data),
            data.len()
        ),
    );
}
fn compressed(f: &Fixture, data: &[u8]) -> Vec<u8> {
    let raw = f.path("source");
    let zst = f.path("source.zst");
    fs::write(&raw, data).unwrap();
    let out = std::process::Command::new("zstd")
        .args(["--quiet", "--keep", "--force"])
        .arg(&raw)
        .arg("-o")
        .arg(&zst)
        .output()
        .expect("CI must install zstd");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    fs::read(zst).unwrap()
}
fn run(f: &Fixture, s: &mut Store) -> i32 {
    xtask::fixtures::fetch::run_with(&f.root, s)
}
fn cache_names(f: &Fixture) -> Vec<String> {
    let cache = f.path("target/fixtures-cache");
    if !cache.exists() {
        return Vec::new();
    }
    let mut names: Vec<_> = fs::read_dir(cache)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn tool_072_fetch_verifies_hash() {
    // Successful binary bytes, a one-byte content error, and a missing S3 object.
    let data: Vec<u8> = (0..130_000).map(|n| (n % 256) as u8).collect();
    for row in 1..=3 {
        let f = Fixture::empty("fetch-hash");
        pointer(&f, "a", &data);
        let mut s = Store::new(&f);
        if row != 3 {
            let mut object = data.clone();
            if row == 2 {
                object[65536] ^= 1;
            }
            s.objects.insert(uri("a"), compressed(&f, &object));
        }
        assert_eq!(run(&f, &mut s), if row == 1 { 0 } else { 2 });
        if row == 1 {
            assert_eq!(fs::read(f.path("target/fixtures-cache/a")).unwrap(), data);
            assert_eq!(cache_names(&f), ["a"]);
        } else {
            assert!(cache_names(&f).is_empty());
        }
        assert_eq!(s.calls.iter().filter(|c| c.program == "aws").count(), 1);
    }
}

#[test]
fn tool_193_fetch_revalidates_cache_and_reuses_only_exact_bytes() {
    let f = Fixture::empty("fetch-cache");
    let data = b"\x00\xffimmutable synthetic fixture\n";
    pointer(&f, "a", data);
    let mut s = Store::new(&f);
    s.objects.insert(uri("a"), compressed(&f, data));
    assert_eq!(run(&f, &mut s), 0);
    s.calls.clear();
    s.objects.clear();
    assert_eq!(
        run(&f, &mut s),
        0,
        "verified cache needs neither network nor codec"
    );
    assert!(s.calls.is_empty());
    for corrupt in [vec![b'x'; data.len()], data[..data.len() - 1].to_vec()] {
        fs::write(f.path("target/fixtures-cache/a"), corrupt).unwrap();
        assert_eq!(run(&f, &mut s), 2);
        assert!(
            cache_names(&f).is_empty(),
            "invalid old bytes are not usable after failed fetch"
        );
        s.objects.insert(uri("a"), compressed(&f, data));
        assert_eq!(run(&f, &mut s), 0);
        assert_eq!(fs::read(f.path("target/fixtures-cache/a")).unwrap(), data);
        s.objects.clear();
    }
}

#[test]
fn tool_194_fetch_preflights_all_pointers_before_side_effects() {
    let good = "uri = \"s3://test-bucket/hft-fixtures/a/object.zst\"\nsha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\nbytes = 3\n";
    let bad: Vec<String> = [
        ("s3://", "https://"),
        ("test-bucket/", "/"),
        ("hft-fixtures/", "private/"),
        ("a/object.zst", "../object.zst"),
        ("a/object.zst", "a//object.zst"),
        ("a/object.zst", "a/object.zip"),
        ("a/object.zst", "a/object.zst?token=x"),
        ("a/object.zst", "a/ object.zst"),
        ("ba7816", "za7816"),
        ("ba7816", "ba78"),
        ("bytes = 3", "bytes = -3"),
        ("bytes = 3", "bytes = 0"),
        ("bytes = 3", "bytes = \"3\""),
        ("bytes = 3", "other = 3"),
    ]
    .into_iter()
    .map(|(from, to)| good.replace(from, to))
    .chain(["not TOML".into()])
    .collect();
    for body in bad {
        let f = Fixture::empty("fetch-bad-pointer");
        f.write("fixtures/replay/a.pointer.toml", good);
        f.write("fixtures/replay/z.pointer.toml", &body);
        f.write(
            "target/fixtures-cache/a",
            "invalid cache preserved during preflight",
        );
        let mut s = Store::new(&f);
        assert_eq!(run(&f, &mut s), 2, "{body}");
        assert!(s.calls.is_empty());
        assert_eq!(
            f.read("target/fixtures-cache/a"),
            "invalid cache preserved during preflight"
        );
        assert_eq!(cache_names(&f), ["a"]);
    }
    for name in [".hidden", "a.b", "a b", ""] {
        let f = Fixture::empty("fetch-bad-name");
        f.write(&format!("fixtures/replay/{name}.pointer.toml"), good);
        let mut s = Store::new(&f);
        assert_eq!(run(&f, &mut s), 2);
        assert!(s.calls.is_empty());
        assert!(!f.path("target").exists());
    }
    let f = Fixture::empty("fetch-empty");
    let mut s = Store::new(&f);
    assert_eq!(run(&f, &mut s), 0);
    assert!(!f.path("target").exists());
    assert_eq!(f.xtask(&["fixtures", "fetch", "unexpected"]).code, 2);
}

#[test]
fn tool_195_fetch_codec_size_and_transfer_failures_leave_no_partial_cache() {
    for row in 0..6 {
        let f = Fixture::empty("fetch-codec");
        pointer(&f, "a", b"abc");
        let mut s = Store::new(&f);
        let bytes = match row {
            0 => b"not a zstd frame".to_vec(),
            1 => {
                let mut b = compressed(&f, b"abc");
                b.pop();
                b
            }
            2 => compressed(&f, b"ab"),
            3 => compressed(&f, b"abcd"),
            _ => compressed(&f, b"abc"),
        };
        s.objects.insert(uri("a"), bytes);
        s.unavailable = row == 4;
        s.empty_success = row == 5;
        assert_eq!(run(&f, &mut s), 2, "row {row}");
        assert!(cache_names(&f).is_empty(), "row {row}: staging retained");
    }
}

#[test]
fn tool_196_fetch_failure_preserves_other_verified_objects() {
    let f = Fixture::empty("fetch-batch");
    pointer(&f, "z", b"third");
    pointer(&f, "b", b"second");
    pointer(&f, "a", b"first");
    let mut s = Store::new(&f);
    s.objects.insert(uri("a"), compressed(&f, b"first"));
    s.objects.insert(uri("z"), compressed(&f, b"third"));
    assert_eq!(run(&f, &mut s), 2);
    assert_eq!(cache_names(&f), ["a"]);
    let downloads: Vec<_> = s
        .calls
        .iter()
        .filter(|c| c.program == "aws")
        .map(|c| c.args[2].clone())
        .collect();
    assert_eq!(downloads, [uri("a"), uri("b")]);
    s.calls.clear();
    s.objects.insert(uri("b"), compressed(&f, b"second"));
    assert_eq!(run(&f, &mut s), 0);
    assert_eq!(cache_names(&f), ["a", "b", "z"]);
    let downloads: Vec<_> = s
        .calls
        .iter()
        .filter(|c| c.program == "aws")
        .map(|c| c.args[2].clone())
        .collect();
    assert_eq!(downloads, [uri("b"), uri("z")]);
    assert_eq!(fs::read(f.path("target/fixtures-cache/a")).unwrap(), b"first");
}

#[cfg(unix)]
#[test]
fn tool_197_fetch_rejects_links_without_touching_their_targets() {
    use std::os::unix::fs::symlink;
    for row in 0..4 {
        let f = Fixture::empty("fetch-links");
        let outside = Fixture::empty("fetch-outside");
        outside.write("a", "abc");
        pointer(&f, "a", b"abc");
        let path = match row {
            0 => f.path("target"),
            1 => {
                fs::create_dir(f.path("target")).unwrap();
                f.path("target/fixtures-cache")
            }
            2 => {
                fs::create_dir_all(f.path("target/fixtures-cache")).unwrap();
                f.path("target/fixtures-cache/a")
            }
            _ => {
                f.remove("fixtures/replay/a.pointer.toml");
                f.path("fixtures/replay/a.pointer.toml")
            }
        };
        let source = if row < 2 {
            outside.root.clone()
        } else {
            outside.path("a")
        };
        symlink(source, &path).unwrap();
        let mut s = Store::new(&f);
        assert_eq!(run(&f, &mut s), 2);
        assert!(s.calls.is_empty());
        assert_eq!(outside.read("a"), "abc");
        assert!(fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        fs::remove_file(&path).unwrap();
    }
}
