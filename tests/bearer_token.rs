use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command as StdCommand;
use std::thread;
use std::time::Duration;
use tempfile::{TempDir, tempdir};

const TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const TEST_TOKEN: &str = "integration-secret-token";

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
fn bearer_token_is_sent_and_show_config_redacts_it() {
    let repo = init_staged_repo();
    let endpoint = TcpListener::bind("127.0.0.1:0").expect("bind endpoint");
    let endpoint_addr = endpoint.local_addr().expect("endpoint address");

    let endpoint_thread = thread::spawn(move || {
        let (mut stream, _) = endpoint.accept().expect("accept model request");
        let request = read_request(&mut stream);
        let request_text = String::from_utf8_lossy(&request);
        assert!(
            request_text.starts_with("POST /v1/chat/completions HTTP/1.1"),
            "unexpected request path: {request_text}"
        );
        let authorization = request_text.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("authorization")
                .then(|| value.trim())
        });
        assert_eq!(authorization, Some("Bearer integration-secret-token"));

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

    let base_url = format!("http://{endpoint_addr}/v1");
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--dry-run")
        .arg("--base-url")
        .arg(&base_url)
        .env(TOKEN_ENV, TEST_TOKEN);
    command
        .assert()
        .success()
        .stdout(predicate::str::contains("test: commit staged change"));
    endpoint_thread.join().expect("endpoint thread");

    let mut show_config = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    show_config
        .current_dir(repo.path())
        .arg("--show-config")
        .arg("--base-url")
        .arg(&base_url)
        .env(TOKEN_ENV, TEST_TOKEN);
    show_config
        .assert()
        .success()
        .stdout(predicate::str::contains("\"bearer_token\": \"<redacted>\""))
        .stdout(predicate::str::contains(TEST_TOKEN).not());
}

#[test]
fn invalid_bearer_token_fails_before_connecting_without_echoing_it() {
    let repo = init_staged_repo();
    let endpoint = TcpListener::bind("127.0.0.1:0").expect("bind endpoint");
    endpoint
        .set_nonblocking(true)
        .expect("set endpoint nonblocking");
    let endpoint_addr = endpoint.local_addr().expect("endpoint address");
    let invalid = "secret token";

    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--dry-run")
        .arg("--base-url")
        .arg(format!("http://{endpoint_addr}/v1"))
        .env(TOKEN_ENV, invalid);
    command
        .assert()
        .failure()
        .stderr(predicate::str::contains(TOKEN_ENV))
        .stderr(predicate::str::contains(invalid).not());

    thread::sleep(Duration::from_millis(50));
    match endpoint.accept() {
        Err(error) if error.kind() == ErrorKind::WouldBlock => {}
        Ok(_) => panic!("endpoint unexpectedly received a request with an invalid token"),
        Err(error) => panic!("unexpected endpoint error: {error}"),
    }
}
