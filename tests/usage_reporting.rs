use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command as StdCommand, Output};
use std::thread;
use std::time::Duration;
use tempfile::{TempDir, tempdir};

const TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const TOKEN_FILE_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN_FILE";

fn git(repo: &Path, args: &[&str]) -> Output {
    StdCommand::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git should be available")
}

fn git_success(repo: &Path, args: &[&str]) -> String {
    let output = git(repo, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output should be UTF-8")
        .trim()
        .to_owned()
}

fn staged_repository() -> TempDir {
    let repo = tempdir().expect("temporary repository");
    git_success(repo.path(), &["init", "--quiet"]);
    git_success(repo.path(), &["config", "user.name", "Test User"]);
    git_success(repo.path(), &["config", "user.email", "test@example.com"]);
    fs::write(repo.path().join("app.txt"), "base\n").expect("write base file");
    git_success(repo.path(), &["add", "app.txt"]);
    git_success(
        repo.path(),
        &["commit", "--quiet", "-m", "chore: initialize fixture"],
    );
    fs::write(repo.path().join("app.txt"), "changed\n").expect("write staged file");
    git_success(repo.path(), &["add", "app.txt"]);
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
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => panic!("request read failed: {error}"),
        }
    }
    request
}

fn usage_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind model endpoint");
    let address = listener.local_addr().expect("model endpoint address");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept model request");
        let request = read_request(&mut stream);
        let request_text = String::from_utf8_lossy(&request);
        assert!(request_text.starts_with("POST /v1/chat/completions HTTP/1.1"));

        let plan = r#"[{"message":"fix: update application behavior","files":["app.txt"]}]"#;
        let body = format!(
            r#"{{"choices":[{{"message":{{"content":{}}}}}],"usage":{{"prompt_tokens":321,"completion_tokens":17,"total_tokens":338}}}}"#,
            serde_json::to_string(plan).expect("serialize plan")
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write model response");
    });
    (format!("http://{address}/v1"), handle)
}

#[test]
fn show_usage_reports_endpoint_tokens_on_stderr_without_changing_plan_stdout() {
    let repo = staged_repository();
    let (base_url, endpoint) = usage_server();
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--base-url")
        .arg(&base_url)
        .arg("--dry-run")
        .arg("--show-usage")
        .env_remove(TOKEN_ENV)
        .env_remove(TOKEN_FILE_ENV);

    command
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "1. fix: update application behavior",
        ))
        .stdout(predicate::str::contains("Model usage:").not())
        .stderr(predicate::str::contains(
            "Model usage: 1 request; 321 prompt tokens (1/1); 17 completion tokens (1/1); 338 total tokens (1/1)",
        ));

    endpoint.join().expect("model endpoint thread");
}
