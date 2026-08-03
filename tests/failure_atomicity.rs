#![cfg(unix)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::{TempDir, tempdir};

const TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const TOKEN_FILE_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN_FILE";
const REAL_GIT_ENV: &str = "GIT_AUTOCOMMIT_TEST_REAL_GIT";
const GIT_MODE_ENV: &str = "GIT_AUTOCOMMIT_TEST_GIT_MODE";
const READY_ENV: &str = "GIT_AUTOCOMMIT_TEST_READY";
const RELEASE_ENV: &str = "GIT_AUTOCOMMIT_TEST_RELEASE";

struct Snapshot {
    initial_head: String,
    staged_tree: String,
    app_worktree: String,
    notes_worktree: String,
}

struct PlanServer {
    base_url: String,
    request_received: Receiver<()>,
    release_response: Sender<()>,
    handle: thread::JoinHandle<()>,
}

struct GitWrapper {
    directory: TempDir,
    ready: PathBuf,
    release: PathBuf,
}

fn find_git() -> PathBuf {
    let output = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .expect("locate git");
    assert!(
        output.status.success(),
        "unable to locate git: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    PathBuf::from(
        String::from_utf8(output.stdout)
            .expect("git path should be UTF-8")
            .trim(),
    )
}

fn git(real_git: &Path, repo: &Path, args: &[&str]) -> Output {
    Command::new(real_git)
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git should run")
}

fn git_success(real_git: &Path, repo: &Path, args: &[&str]) -> String {
    let output = git(real_git, repo, args);
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

fn init_repository(real_git: &Path) -> TempDir {
    let repo = tempdir().expect("temporary repository");
    git_success(real_git, repo.path(), &["init", "--quiet"]);
    git_success(
        real_git,
        repo.path(),
        &["config", "user.name", "Test User"],
    );
    git_success(
        real_git,
        repo.path(),
        &["config", "user.email", "test@example.com"],
    );

    fs::create_dir_all(repo.path().join("docs")).expect("create docs directory");
    fs::write(repo.path().join("app.txt"), "base app\n").expect("write base app");
    fs::write(repo.path().join("notes.txt"), "base notes\n").expect("write base notes");
    fs::write(repo.path().join("docs/guide.md"), "base guide\n").expect("write base guide");
    git_success(real_git, repo.path(), &["add", "."]);
    git_success(
        real_git,
        repo.path(),
        &["commit", "--quiet", "-m", "chore: initialize fixture"],
    );
    repo
}

fn prepare_snapshot(real_git: &Path, repo: &Path) -> Snapshot {
    let initial_head = git_success(real_git, repo, &["rev-parse", "HEAD"]);
    fs::write(repo.join("app.txt"), "staged app\n").expect("write staged app");
    fs::write(repo.join("docs/guide.md"), "staged guide\n").expect("write staged guide");
    git_success(real_git, repo, &["add", "app.txt", "docs/guide.md"]);
    let staged_tree = git_success(real_git, repo, &["write-tree"]);

    let app_worktree = "staged app\nunstaged app\n".to_owned();
    let notes_worktree = "base notes\nunstaged notes\n".to_owned();
    fs::write(repo.join("app.txt"), &app_worktree).expect("write unstaged app change");
    fs::write(repo.join("notes.txt"), &notes_worktree)
        .expect("write unstaged notes change");

    Snapshot {
        initial_head,
        staged_tree,
        app_worktree,
        notes_worktree,
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

fn start_plan_server() -> PlanServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind model endpoint");
    let address = listener.local_addr().expect("model endpoint address");
    let (request_tx, request_received) = mpsc::channel();
    let (release_response, release_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept model request");
        let request = read_request(&mut stream);
        let request_text = String::from_utf8_lossy(&request);
        assert!(
            request_text.starts_with("POST /v1/chat/completions HTTP/1.1"),
            "unexpected request: {request_text}"
        );
        request_tx.send(()).expect("signal model request");
        release_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("model response was not released");

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

    PlanServer {
        base_url: format!("http://{address}/v1"),
        request_received,
        release_response,
        handle,
    }
}

fn autocommit_command(repo: &Path, base_url: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo)
        .arg("--base-url")
        .arg(base_url)
        .arg("--no-sign")
        .env_remove(TOKEN_ENV)
        .env_remove(TOKEN_FILE_ENV)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn spawn(mut command: Command) -> Child {
    command.spawn().expect("start git-autocommit")
}

fn wait_for_request(server: &PlanServer) {
    server
        .request_received
        .recv_timeout(Duration::from_secs(10))
        .expect("git-autocommit did not request a plan");
}

fn release_server(server: &PlanServer) {
    server
        .release_response
        .send(())
        .expect("release model response");
}

fn finish_server(server: PlanServer) {
    server.handle.join().expect("model endpoint thread");
}

fn assert_failure(output: &Output, expected: Option<&str>) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "git-autocommit unexpectedly succeeded\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    if let Some(expected) = expected {
        assert!(
            stderr.contains(expected),
            "stderr did not contain {expected:?}:\n{stderr}"
        );
    }
}

fn assert_snapshot_preserved(real_git: &Path, repo: &Path, snapshot: &Snapshot, head: &str) {
    assert_eq!(git_success(real_git, repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git_success(real_git, repo, &["write-tree"]),
        snapshot.staged_tree
    );
    assert_eq!(
        fs::read_to_string(repo.join("app.txt")).expect("read app worktree"),
        snapshot.app_worktree
    );
    assert_eq!(
        fs::read_to_string(repo.join("notes.txt")).expect("read notes worktree"),
        snapshot.notes_worktree
    );
    assert!(
        !repo.join(".git/index.lock").exists(),
        "index lock should be released"
    );
}

fn create_concurrent_head(real_git: &Path, repo: &Path, expected_old: &str) -> String {
    let tree = git_success(real_git, repo, &["rev-parse", "HEAD^{tree}"]);
    let commit = git_success(
        real_git,
        repo,
        &[
            "commit-tree",
            &tree,
            "-p",
            expected_old,
            "-m",
            "chore: concurrent head movement",
        ],
    );
    git_success(
        real_git,
        repo,
        &["update-ref", "HEAD", &commit, expected_old],
    );
    commit
}

fn create_git_wrapper() -> GitWrapper {
    let directory = tempdir().expect("temporary Git wrapper directory");
    let script = directory.path().join("git");
    fs::write(
        &script,
        r#"#!/bin/sh
for arg in "$@"; do
    if [ "$arg" = "commit-tree" ] && [ "$GIT_AUTOCOMMIT_TEST_GIT_MODE" = "fail-commit-tree" ]; then
        echo "forced commit-tree failure" >&2
        exit 97
    fi
    if [ "$arg" = "update-ref" ] && [ "$GIT_AUTOCOMMIT_TEST_GIT_MODE" = "pause-update-ref" ]; then
        : > "$GIT_AUTOCOMMIT_TEST_READY"
        attempts=0
        while [ ! -e "$GIT_AUTOCOMMIT_TEST_RELEASE" ]; do
            attempts=$((attempts + 1))
            if [ "$attempts" -ge 1000 ]; then
                echo "timed out waiting to release update-ref" >&2
                exit 98
            fi
            sleep 0.01
        done
    fi
done
exec "$GIT_AUTOCOMMIT_TEST_REAL_GIT" "$@"
"#,
    )
    .expect("write Git wrapper");
    let mut permissions = fs::metadata(&script)
        .expect("inspect Git wrapper")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).expect("make Git wrapper executable");

    GitWrapper {
        ready: directory.path().join("ready"),
        release: directory.path().join("release"),
        directory,
    }
}

fn configure_wrapper(command: &mut Command, wrapper: &GitWrapper, real_git: &Path, mode: &str) {
    let mut paths = vec![wrapper.directory.path().to_path_buf()];
    paths.extend(env::split_paths(
        &env::var_os("PATH").unwrap_or_else(OsString::new),
    ));
    command
        .env("PATH", env::join_paths(paths).expect("join wrapper PATH"))
        .env(REAL_GIT_ENV, real_git)
        .env(GIT_MODE_ENV, mode)
        .env(READY_ENV, &wrapper.ready)
        .env(RELEASE_ENV, &wrapper.release);
}

fn wait_for_path(path: &Path) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) {
        if path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {}", path.display());
}

#[test]
fn commit_creation_failure_leaves_repository_state_unchanged() {
    let real_git = find_git();
    let repo = init_repository(&real_git);
    let snapshot = prepare_snapshot(&real_git, repo.path());
    let server = start_plan_server();
    let wrapper = create_git_wrapper();
    let mut command = autocommit_command(repo.path(), &server.base_url);
    configure_wrapper(&mut command, &wrapper, &real_git, "fail-commit-tree");

    let child = spawn(command);
    wait_for_request(&server);
    release_server(&server);
    let output = child.wait_with_output().expect("wait for git-autocommit");
    finish_server(server);

    assert_failure(&output, Some("forced commit-tree failure"));
    assert_snapshot_preserved(
        &real_git,
        repo.path(),
        &snapshot,
        &snapshot.initial_head,
    );
}

#[test]
fn head_change_while_waiting_for_model_is_not_overwritten() {
    let real_git = find_git();
    let repo = init_repository(&real_git);
    let snapshot = prepare_snapshot(&real_git, repo.path());
    let server = start_plan_server();
    let command = autocommit_command(repo.path(), &server.base_url);

    let child = spawn(command);
    wait_for_request(&server);
    let concurrent_head = create_concurrent_head(
        &real_git,
        repo.path(),
        &snapshot.initial_head,
    );
    release_server(&server);
    let output = child.wait_with_output().expect("wait for git-autocommit");
    finish_server(server);

    assert_failure(
        &output,
        Some("HEAD changed while the commit plan was being generated"),
    );
    assert_snapshot_preserved(
        &real_git,
        repo.path(),
        &snapshot,
        &concurrent_head,
    );
}

#[test]
fn index_change_while_waiting_for_model_is_preserved_and_rejected() {
    let real_git = find_git();
    let repo = init_repository(&real_git);
    let snapshot = prepare_snapshot(&real_git, repo.path());
    let server = start_plan_server();
    let command = autocommit_command(repo.path(), &server.base_url);

    let child = spawn(command);
    wait_for_request(&server);
    fs::write(repo.path().join("docs/guide.md"), "concurrent staged guide\n")
        .expect("write concurrent staged guide");
    git_success(
        &real_git,
        repo.path(),
        &["add", "docs/guide.md"],
    );
    let concurrent_tree = git_success(&real_git, repo.path(), &["write-tree"]);
    release_server(&server);
    let output = child.wait_with_output().expect("wait for git-autocommit");
    finish_server(server);

    assert_failure(
        &output,
        Some("the staged index changed while the commit plan was being generated"),
    );
    assert_eq!(
        git_success(&real_git, repo.path(), &["rev-parse", "HEAD"]),
        snapshot.initial_head
    );
    assert_eq!(
        git_success(&real_git, repo.path(), &["write-tree"]),
        concurrent_tree
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("docs/guide.md"))
            .expect("read concurrent staged guide"),
        "concurrent staged guide\n"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("app.txt")).expect("read app worktree"),
        snapshot.app_worktree
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("notes.txt")).expect("read notes worktree"),
        snapshot.notes_worktree
    );
    assert!(
        !repo.path().join(".git/index.lock").exists(),
        "index lock should be released"
    );
}

#[test]
fn final_update_ref_conflict_keeps_only_the_concurrent_history_reachable() {
    let real_git = find_git();
    let repo = init_repository(&real_git);
    let snapshot = prepare_snapshot(&real_git, repo.path());
    let server = start_plan_server();
    let wrapper = create_git_wrapper();
    let mut command = autocommit_command(repo.path(), &server.base_url);
    configure_wrapper(&mut command, &wrapper, &real_git, "pause-update-ref");

    let child = spawn(command);
    wait_for_request(&server);
    release_server(&server);
    wait_for_path(&wrapper.ready);
    assert!(
        repo.path().join(".git/index.lock").exists(),
        "final update-ref should run while the index lock is held"
    );
    let concurrent_head = create_concurrent_head(
        &real_git,
        repo.path(),
        &snapshot.initial_head,
    );
    fs::write(&wrapper.release, b"release\n").expect("release update-ref");
    let output = child.wait_with_output().expect("wait for git-autocommit");
    finish_server(server);

    assert_failure(&output, None);
    assert_snapshot_preserved(
        &real_git,
        repo.path(),
        &snapshot,
        &concurrent_head,
    );
    assert_eq!(
        git_success(
            &real_git,
            repo.path(),
            &["rev-list", "--reverse", &format!("{}..HEAD", snapshot.initial_head)],
        ),
        concurrent_head
    );
    assert_eq!(
        git_success(&real_git, repo.path(), &["rev-list", "--all"])
            .lines()
            .count(),
        2,
        "generated commits must remain unreachable after ref conflict"
    );
}
