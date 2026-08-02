use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod implementation {
    include!("core.rs");

    pub(super) fn run_main() {
        main();
    }
}

const ACTIVE_GIT_OPERATIONS: &[(&str, &str)] = &[
    ("MERGE_HEAD", "merge"),
    ("CHERRY_PICK_HEAD", "cherry-pick"),
    ("REVERT_HEAD", "revert"),
    ("rebase-merge", "rebase"),
    ("rebase-apply", "rebase/am"),
    ("sequencer", "sequenced cherry-pick/revert"),
    ("BISECT_START", "bisect"),
];

fn run_git(root: Option<&Path>, args: &[&str]) -> std::io::Result<Output> {
    let mut command = Command::new("git");
    if let Some(root) = root {
        command.arg("-C").arg(root);
    }
    command.args(args).output()
}

fn output_error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if stderr.is_empty() { stdout } else { stderr }
}

fn repository_root() -> Result<Option<PathBuf>, String> {
    let output = run_git(None, &["rev-parse", "--show-toplevel"])
        .map_err(|error| format!("unable to inspect Git repository state: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let root = String::from_utf8(output.stdout)
        .map_err(|_| "Git returned a non-UTF-8 repository path".to_owned())?;
    Ok(Some(PathBuf::from(root.trim())))
}

fn git_path(root: &Path, marker: &str) -> Result<PathBuf, String> {
    let output = run_git(Some(root), &["rev-parse", "--git-path", marker])
        .map_err(|error| format!("unable to resolve Git state path {marker}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "unable to resolve Git state path {marker}: {}",
            output_error(&output)
        ));
    }
    let value = String::from_utf8(output.stdout)
        .map_err(|_| format!("Git returned a non-UTF-8 state path for {marker}"))?;
    let path = PathBuf::from(value.trim());
    Ok(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

fn informational_only() -> bool {
    env::args_os().skip(1).any(|argument| {
        argument == OsStr::new("-h")
            || argument == OsStr::new("--help")
            || argument == OsStr::new("--show-config")
    })
}

fn assert_safe_repository_state() -> Result<(), String> {
    if informational_only() {
        return Ok(());
    }
    let Some(root) = repository_root()? else {
        return Ok(());
    };
    for (marker, operation) in ACTIVE_GIT_OPERATIONS {
        let path = git_path(&root, marker)?;
        if fs::symlink_metadata(&path).is_ok() {
            return Err(format!(
                "refusing to run during an active Git {operation} operation ({marker}); complete or abort it first"
            ));
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = assert_safe_repository_state() {
        eprintln!("git-autocommit: {error}");
        std::process::exit(1);
    }
    implementation::run_main();
}
