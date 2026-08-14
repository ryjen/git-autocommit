use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use tempfile::{TempDir, tempdir};

const REVIEW_ENV: &str = "GIT_AUTOCOMMIT_REVIEW";

fn git(repo: &Path, args: &[&str]) -> std::process::Output {
    StdCommand::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git should be available")
}

fn git_success(repo: &Path, args: &[&str]) {
    let output = git(repo, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    fs::write(repo.path().join("app.txt"), "staged\n").expect("write staged file");
    git_success(repo.path(), &["add", "app.txt"]);
    repo
}

#[test]
fn review_is_reported_as_enabled_by_default() {
    let repo = staged_repository();
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--show-config")
        .env_remove(REVIEW_ENV);

    command.assert().success().stdout(predicate::str::contains(
        "\"review_before_commit\": true",
    ));
}

#[test]
fn review_configuration_obeys_cli_environment_and_toml_precedence() {
    let repo = staged_repository();
    fs::write(
        repo.path().join(".git/autocommit.toml"),
        "review_before_commit = false\n",
    )
    .expect("write autocommit config");

    let mut configured = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    configured
        .current_dir(repo.path())
        .arg("--show-config")
        .env_remove(REVIEW_ENV);
    configured.assert().success().stdout(predicate::str::contains(
        "\"review_before_commit\": false",
    ));

    let mut environment = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    environment
        .current_dir(repo.path())
        .arg("--show-config")
        .env(REVIEW_ENV, "true");
    environment.assert().success().stdout(predicate::str::contains(
        "\"review_before_commit\": true",
    ));

    let mut cli = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    cli.current_dir(repo.path())
        .arg("--show-config")
        .arg("--no-review")
        .env(REVIEW_ENV, "true");
    cli.assert().success().stdout(predicate::str::contains(
        "\"review_before_commit\": false",
    ));
}

#[test]
fn default_review_fails_closed_without_an_interactive_stdin() {
    let repo = staged_repository();
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--base-url")
        .arg("http://127.0.0.1:9/v1")
        .env_remove(REVIEW_ENV)
        .stdin(Stdio::null());

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "review is enabled but stdin is not interactive",
        ))
        .stderr(predicate::str::contains("--no-review"))
        .stderr(predicate::str::contains("local AI unavailable").not());
}

#[test]
fn help_documents_review_controls() {
    let repo = staged_repository();
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command.current_dir(repo.path()).arg("--help");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("--review"))
        .stdout(predicate::str::contains("--no-review"))
        .stdout(predicate::str::contains("interactive review"));
}
