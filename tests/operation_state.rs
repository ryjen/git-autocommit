use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
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

fn init_repo() -> TempDir {
    let repo = tempdir().expect("temporary repository");
    assert_git_success(repo.path(), &["init", "--quiet"]);
    repo
}

fn init_committed_repo() -> TempDir {
    let repo = init_repo();
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
    repo
}

fn marker_path(repo: &Path, marker: &str) -> PathBuf {
    let output = git(repo, &["rev-parse", "--git-path", marker]);
    assert!(
        output.status.success(),
        "git-path failed for {marker}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = String::from_utf8(output.stdout).expect("Git state path should be UTF-8");
    let path = PathBuf::from(value.trim());
    if path.is_absolute() {
        path
    } else {
        repo.join(path)
    }
}

fn create_marker(repo: &Path, marker: &str, directory: bool) {
    let path = marker_path(repo, marker);
    if directory {
        fs::create_dir_all(&path).expect("create Git operation directory");
    } else {
        fs::create_dir_all(path.parent().expect("marker parent"))
            .expect("create Git operation marker parent");
        fs::write(path, "active\n").expect("create Git operation marker");
    }
}

#[test]
fn rejects_active_git_operations_before_planning() {
    let operations = [
        ("MERGE_HEAD", "merge", false),
        ("CHERRY_PICK_HEAD", "cherry-pick", false),
        ("REVERT_HEAD", "revert", false),
        ("rebase-merge", "rebase", true),
        ("rebase-apply", "rebase/am", true),
        ("sequencer", "sequenced cherry-pick/revert", true),
        ("BISECT_START", "bisect", false),
    ];

    for (marker, operation, directory) in operations {
        let repo = init_repo();
        create_marker(repo.path(), marker, directory);

        let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
        command
            .current_dir(repo.path())
            .arg("--dry-run")
            .arg("--base-url")
            .arg("http://127.0.0.1:9/v1");

        command.assert().failure().stderr(
            predicate::str::contains(format!("active Git {operation} operation"))
                .and(predicate::str::contains(marker))
                .and(predicate::str::contains("complete or abort it first")),
        );
    }
}

#[test]
fn normal_repository_state_reaches_existing_validation() {
    let repo = init_committed_repo();

    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--dry-run")
        .arg("--base-url")
        .arg("http://127.0.0.1:9/v1");

    command.assert().failure().stderr(
        predicate::str::contains("no staged changes")
            .and(predicate::str::contains("active Git").not()),
    );
}

#[test]
fn help_remains_available_during_git_operations() {
    let repo = init_repo();
    create_marker(repo.path(), "MERGE_HEAD", false);

    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command.current_dir(repo.path()).arg("--help");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("Split staged changes"));
}
