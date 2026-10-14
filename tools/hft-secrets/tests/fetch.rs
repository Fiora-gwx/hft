//! The real binary and AWS SDK against local IMDSv2, Secrets Manager and KMS HTTP endpoints.
#![cfg(unix)]
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    root: PathBuf,
    url: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    requests: Arc<AtomicUsize>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut bytes = Vec::new();
    let end = loop {
        let mut b = [0; 1024];
        let n = stream.read(&mut b).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&b[..n]);
        if let Some(i) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let header = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let len = header
        .lines()
        .find_map(|s| {
            s.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while bytes.len() < end + len {
        let mut b = [0; 1024];
        let n = stream.read(&mut b).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&b[..n]);
    }
    (header, bytes[end..end + len].to_vec())
}
impl Fixture {
    fn new(fail_second: bool, role: bool) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "secrets-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("m.toml"),"[[secret]]\nname='a'\nsecret_id='arn:aws:secretsmanager:us-east-1:123456789012:secret:a'\njson_key='api_key'\nkind='plain'\n[[secret]]\nname='b'\nsecret_id='arn:aws:secretsmanager:us-east-1:123456789012:secret:b'\nkind='kms-ciphertext'\n").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let requests = Arc::new(AtomicUsize::new(0));
        let count = requests.clone();
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut s, _)) => {
                        let (h, b) = request(&mut s);
                        count.fetch_add(1, Ordering::Relaxed);
                        let (status, body) = if h.starts_with("PUT /latest/api/token ") {
                            (200, "test-token".to_owned())
                        } else if h.starts_with("GET /latest/meta-data/iam/security-credentials/ ") {
                            if role {
                                (200, "test-role".into())
                            } else {
                                (404, "".into())
                            }
                        } else if h.starts_with("GET /latest/meta-data/iam/security-credentials/test-role ") {
                            (200,json!({"Code":"Success","AccessKeyId":"TESTKEY","SecretAccessKey":"TESTSECRET","Token":"TESTTOKEN","Expiration":"2099-01-01T00:00:00Z"}).to_string())
                        } else if h.contains("GetSecretValue") {
                            let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
                            if v["SecretId"].as_str().unwrap().ends_with(":a") {
                                (200,json!({"SecretString":"{\"api_key\":\"S3CR3T-A\",\"other\":\"x\"}","VersionId":"v1"}).to_string())
                            } else if fail_second {
                                (400,json!({"__type":"AccessDeniedException","message":"S3CR3T must never reach diagnostic"}).to_string())
                            } else {
                                (
                                    200,
                                    json!({"SecretString":"Y2lwaGVydGV4dA==","VersionId":"v7"}).to_string(),
                                )
                            }
                        } else if h.contains("Decrypt") {
                            (200, json!({"Plaintext":"UzNDUjNULUI="}).to_string())
                        } else {
                            (404, "".into())
                        };
                        let reply = format!(
                            "HTTP/1.1 {status} Test\r\nContent-Type: application/x-amz-json-1.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = s.write_all(reply.as_bytes());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(e) => panic!("listener: {e}"),
                }
            }
        });
        Self {
            root,
            url,
            stop,
            thread: Some(thread),
            requests,
        }
    }
    fn run(&self, imds: &str) -> std::process::Output {
        let name = Command::new("id").arg("-un").output().unwrap();
        let name = std::str::from_utf8(&name.stdout).unwrap().trim();
        Command::new(env!("CARGO_BIN_EXE_hft-secrets"))
            .args(["fetch", "--manifest"])
            .arg(self.root.join("m.toml"))
            .arg("--out")
            .arg(self.root.join("out"))
            .args(["--owner", name, "--mode", "0400"])
            .env("AWS_EC2_METADATA_SERVICE_ENDPOINT", imds)
            .env("AWS_ENDPOINT_URL", &self.url)
            .env("AWS_REGION", "us-east-1")
            .env("AWS_EC2_METADATA_DISABLED", "false")
            .env("AWS_MAX_ATTEMPTS", "1")
            .env_remove("AWS_PROFILE")
            .env_remove("AWS_ACCESS_KEY_ID")
            .env_remove("AWS_SECRET_ACCESS_KEY")
            .env_remove("AWS_SESSION_TOKEN")
            .env("AWS_CONFIG_FILE", self.root.join("absent-config"))
            .env(
                "AWS_SHARED_CREDENTIALS_FILE",
                self.root.join("absent-credentials"),
            )
            .output()
            .unwrap()
    }
}
#[test]
fn tool_114_writes_files_and_names_only() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new(false, true);
    let o = f.run(&f.url);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read(f.root.join("out/a")).unwrap(), b"S3CR3T-A");
    assert_eq!(std::fs::read(f.root.join("out/b")).unwrap(), b"S3CR3T-B");
    let uid = std::fs::metadata(&f.root).unwrap().uid();
    for (path, mode) in [("out", 0o700), ("out/a", 0o400), ("out/b", 0o400)] {
        let m = std::fs::metadata(f.root.join(path)).unwrap();
        assert_eq!(m.mode() & 0o777, mode);
        assert_eq!(m.uid(), uid);
    }
    assert_eq!(o.stdout, b"a v1\nb v7\n");
    assert!(o.stderr.is_empty());
    assert!(f.requests.load(Ordering::Relaxed) >= 5);
}
#[test]
fn tool_115_all_or_nothing() {
    let f = Fixture::new(true, true);
    let o = f.run(&f.url);
    assert_eq!(o.status.code(), Some(1), "{}", String::from_utf8_lossy(&o.stderr));
    let e = String::from_utf8(o.stderr).unwrap();
    assert!(e.contains("b AccessDeniedException"));
    assert!(!e.contains("S3CR3T"));
    assert!(o.stdout.is_empty());
    assert!(!f.root.join("out").exists() || std::fs::read_dir(f.root.join("out")).unwrap().next().is_none());
}
#[test]
fn tool_116_env_errors() {
    let f = Fixture::new(false, false);
    for endpoint in [&f.url, "http://127.0.0.1:1"] {
        let o = f.run(endpoint);
        assert_eq!(o.status.code(), Some(2), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(!f.root.join("out").exists());
        assert!(o.stdout.is_empty());
    }
}
