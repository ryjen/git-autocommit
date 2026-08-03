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

fn init_repository() -> TempDir {
    let repo = tempdir().expect("temporary repository");
    git_success(repo.path(), &["init", "--quiet"]);
    git_success(repo.path(), &["config", "user.name", "Test User"]);
    git_success(repo.path(), &["config", "user.email", "test@example.com"]);

    fs::create_dir_all(repo.path().join("docs")).expect("create docs directory");
    fs::write(repo.path().join("app.txt"), "base app\n").expect("write base app");
    fs::write(repo.path().join("notes.txt"), "base notes\n").expect("write base notes");
    fs::write(repo.path().join("docs/guide.md"), "base guide\n").expect("write base guide");
    git_success(repo.path(), &["add", "."]);
    git_success(
        repo.path(),
        &["commit", "--quiet", "-m", "chore: initialize fixture"],
    );
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

fn mock_plan_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind model endpoint");
    let address = listener.local_addr().expect("model endpoint address");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept model request");
        let request = read_request(&mut stream);
        let request_text = String::from_utf8_lossy(&request);
        assert!(
            request_text.starts_with("POST /v1/chat/completions HTTP/1.1"),
            "unexpected request: {request_text}"
        );

        let plan = r#"[{"message":"feat(core): update application behavior","files":["app.txt"]},{"message":"docs: revise usage guide","files":["docs/guide.md"]}]"#;
        let body = format!(
            r#"{{"choices":[{{"message":{{"content":{}}}}}]}}"#,
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
fn successful_commit_flow_preserves_the_snapshot_and_unstaged_worktree() {
    let repo = init_repository();
    let initial_head = git_success(repo.path(), &["rev-parse", "HEAD"]);

    fs::write(repo.path().join("app.txt"), "staged app\n").expect("write staged app");
    fs::write(repo.path().join("docs/guide.md"), "staged guide\n")
        .expect("write staged guide");
    git_success(repo.path(), &["add", "app.txt", "docs/guide.md"]);
    let staged_tree = git_success(repo.path(), &["write-tree"]);

    fs::write(repo.path().join("app.txt"), "staged app\nunstaged app\n")
        .expect("write unstaged app change");
    fs::write(
        repo.path().join("notes.txt"),
        "base notes\nunstaged notes\n",
    )
    .expect("write unstaged notes change");

    let (base_url, endpoint) = mock_plan_server();
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--base-url")
        .arg(&base_url)
        .arg("--no-sign")
        .env_remove(TOKEN_ENV)
        .env_remove(TOKEN_FILE_ENV);
    command
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "1. feat(core): update application behavior",
        ))
        .stdout(predicate::str::contains("   app.txt"))
        .stdout(predicate::str::contains("2. docs: revise usage guide"))
        .stdout(predicate::str::contains("   docs/guide.md"));
    endpoint.join().expect("model endpoint thread");

    let commits = git_success(
        repo.path(),
        &["rev-list", "--reverse", &format!("{initial_head}..HEAD")],
    );
    let commits: Vec<&str> = commits.lines().collect();
    assert_eq!(commits.len(), 2, "expected exactly two generated commits");

    assert_eq!(
        git_success(repo.path(), &["show", "-s", "--format=%s", commits[0]]),
        "feat(core): update application behavior"
    );
    assert_eq!(
        git_success(
            repo.path(),
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "-r",
                commits[0],
            ],
        ),
        "app.txt"
    );
    assert_eq!(
        git_success(repo.path(), &["show", "-s", "--format=%s", commits[1]]),
        "docs: revise usage guide"
    );
    assert_eq!(
        git_success(
            repo.path(),
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "-r",
                commits[1],
            ],
        ),
        "docs/guide.md"
    );
    assert_eq!(
        git_success(repo.path(), &["rev-parse", &format!("{}^", commits[0])]),
        initial_head
    );
    assert_eq!(
        git_success(repo.path(), &["rev-parse", &format!("{}^", commits[1])]),
        commits[0]
    );

    assert_eq!(
        git_success(repo.path(), &["rev-parse", "HEAD^{tree}"]),
        staged_tree,
        "final commit tree must reproduce the original staged snapshot"
    );
    assert!(
        git(repo.path(), &["diff", "--cached", "--quiet"])
            .status
            .success(),
        "index should match the new HEAD"
    );
    assert_eq!(
        git_success(repo.path(), &["show", "HEAD:app.txt"]),
        "staged app"
    );
    assert_eq!(
        git_success(repo.path(), &["show", "HEAD:docs/guide.md"]),
        "staged guide"
    );

    assert_eq!(
        fs::read_to_string(repo.path().join("app.txt")).expect("read app worktree"),
        "staged app\nunstaged app\n"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("notes.txt")).expect("read notes worktree"),
        "base notes\nunstaged notes\n"
    );
    assert_eq!(
        git_success(repo.path(), &["diff", "--name-only"]),
        "app.txt\nnotes.txt"
    );
    assert!(
        !repo.path().join(".git/index.lock").exists(),
        "index lock should be released after success"
    );
}
