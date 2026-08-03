use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command as StdCommand;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::{TempDir, tempdir};

fn git(repo: &Path, args: &[&str]) -> std::process::Output {
    StdCommand::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git should be available")
}

fn assert_git_success(repo: &Path, args: &[&str]) {
    let output = git(repo, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_staged_repo() -> TempDir {
    let repo = tempdir().expect("temporary repository");
    assert_git_success(repo.path(), &["init", "--quiet"]);
    fs::write(repo.path().join("README.md"), "initial\n").expect("write initial file");
    assert_git_success(repo.path(), &["add", "README.md"]);
    assert_git_success(
        repo.path(),
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--quiet",
            "-m",
            "initial",
        ],
    );
    fs::write(repo.path().join("staged.txt"), "staged content\n").expect("write staged file");
    assert_git_success(repo.path(), &["add", "staged.txt"]);
    repo
}

fn accept_until(listener: &TcpListener, timeout: Duration) -> Option<TcpStream> {
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((stream, _)) => return Some(stream),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return None;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("listener failed: {error}"),
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set read timeout");
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                request.extend_from_slice(&buffer[..count]);
                if let Some(headers_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers_end = headers_end + 4;
                    let headers = String::from_utf8_lossy(&request[..headers_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if request.len() >= headers_end + content_length {
                        break;
                    }
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                break;
            }
            Err(error) => panic!("request read failed: {error}"),
        }
    }
    request
}

#[test]
fn model_request_ignores_environment_proxies() {
    let repo = init_staged_repo();
    let endpoint = TcpListener::bind("127.0.0.1:0").expect("bind endpoint");
    endpoint
        .set_nonblocking(true)
        .expect("endpoint nonblocking");
    let endpoint_addr = endpoint.local_addr().expect("endpoint address");
    let proxy = TcpListener::bind("127.0.0.1:0").expect("bind proxy");
    proxy.set_nonblocking(true).expect("proxy nonblocking");
    let proxy_addr = proxy.local_addr().expect("proxy address");

    let endpoint_thread = thread::spawn(move || {
        let mut stream = accept_until(&endpoint, Duration::from_secs(10))
            .expect("configured endpoint should receive the request directly");
        let request = read_request(&mut stream);
        assert!(
            String::from_utf8_lossy(&request).starts_with("POST /v1/chat/completions HTTP/1.1"),
            "unexpected endpoint request: {}",
            String::from_utf8_lossy(&request)
        );
        let plan = r#"[{"message":"test: commit staged change","files":["staged.txt"]}]"#;
        let body = format!(
            r#"{{"choices":[{{"message":{{"content":{}}}}}]}}"#,
            serde_json::to_string(plan).unwrap()
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write endpoint response");
    });

    let (stop_proxy_tx, stop_proxy_rx) = mpsc::channel();
    let proxy_thread = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match proxy.accept() {
                Ok((mut stream, _)) => {
                    let request = read_request(&mut stream);
                    let response = "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    stream.write_all(response.as_bytes()).ok();
                    return Some(request);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    if stop_proxy_rx.try_recv().is_ok() || Instant::now() >= deadline {
                        return None;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("proxy listener failed: {error}"),
            }
        }
    });

    let proxy_url = format!("http://{proxy_addr}");
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--dry-run")
        .arg("--base-url")
        .arg(format!("http://{endpoint_addr}/v1"))
        .env("HTTP_PROXY", &proxy_url)
        .env("HTTPS_PROXY", &proxy_url)
        .env("ALL_PROXY", &proxy_url)
        .env("http_proxy", &proxy_url)
        .env("https_proxy", &proxy_url)
        .env("all_proxy", &proxy_url)
        .env_remove("NO_PROXY")
        .env_remove("no_proxy");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("test: commit staged change"));

    endpoint_thread.join().expect("endpoint thread");
    stop_proxy_tx.send(()).expect("stop proxy listener");
    let proxy_request = proxy_thread.join().expect("proxy thread");
    assert!(
        proxy_request.is_none(),
        "proxy unexpectedly received model request: {}",
        proxy_request
            .as_deref()
            .map(String::from_utf8_lossy)
            .unwrap_or_default()
    );
}
