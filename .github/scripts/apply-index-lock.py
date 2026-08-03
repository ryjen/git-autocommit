from pathlib import Path


def replace_once(path: str, old: str, new: str, label: str) -> None:
    target = Path(path)
    text = target.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match in {path}, found {count}")
    target.write_text(text.replace(old, new, 1))


replace_once(
    "src/app.rs",
    "use std::fs;",
    "use std::fs::{self, OpenOptions};",
    "OpenOptions import",
)

replace_once(
    "src/app.rs",
    '''    fn config_path(&self) -> Result<PathBuf> {
        let value = self.git(&["rev-parse", "--git-path", "autocommit.toml"])?;
        let path = PathBuf::from(value.trim());
        Ok(if path.is_absolute() {
            path
        } else {
            self.root.join(path)
        })
    }
}

fn run_git_raw(''',
    '''    fn git_path(&self, name: &str) -> Result<PathBuf> {
        let value = self.git(&["rev-parse", "--git-path", name])?;
        let path = PathBuf::from(value.trim());
        Ok(if path.is_absolute() {
            path
        } else {
            self.root.join(path)
        })
    }

    fn config_path(&self) -> Result<PathBuf> {
        self.git_path("autocommit.toml")
    }
}

struct IndexLock {
    path: PathBuf,
    file: Option<fs::File>,
}

impl IndexLock {
    fn acquire(repo: &Repo) -> Result<Self> {
        let index_path = repo.git_path("index")?;
        let mut lock_path = index_path.as_os_str().to_os_string();
        lock_path.push(".lock");
        let path = PathBuf::from(lock_path);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| {
                format!(
                    "unable to lock staged index at {}; another Git process may be modifying it",
                    path.display()
                )
            })?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }
}

impl Drop for IndexLock {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

fn run_git_raw(''',
    "Git path and index lock",
)

replace_once(
    "src/app.rs",
    '''    assert_snapshot(repo, base_head, snapshot)?;
    repo.git(&["update-ref", "HEAD", &parent, base_head])?;
    Ok(())
}''',
    '''    let _index_lock = IndexLock::acquire(repo)?;
    assert_snapshot(repo, base_head, snapshot)?;
    repo.git(&["update-ref", "HEAD", &parent, base_head])?;
    Ok(())
}''',
    "critical section lock",
)

replace_once(
    "src/app.rs",
    '''    #[test]
    fn accepts_prompt_at_limit() {''',
    '''    #[test]
    fn index_lock_blocks_git_index_writes_and_cleans_up() {
        let temp = TempDir::new().unwrap();
        let repo = Repo {
            root: temp.path().to_path_buf(),
        };
        let init = run_git_raw(Some(&repo.root), &["init", "--quiet"], None).unwrap();
        assert!(init.status.success());
        fs::write(repo.root.join("staged.txt"), "content\n").unwrap();

        let lock = IndexLock::acquire(&repo).unwrap();
        let blocked = run_git_raw(Some(&repo.root), &["add", "staged.txt"], None).unwrap();
        assert!(!blocked.status.success());
        assert!(String::from_utf8_lossy(&blocked.stderr).contains("index.lock"));

        drop(lock);
        let added = run_git_raw(Some(&repo.root), &["add", "staged.txt"], None).unwrap();
        assert!(
            added.status.success(),
            "git add failed after lock release: {}",
            String::from_utf8_lossy(&added.stderr)
        );
    }

    #[test]
    fn accepts_prompt_at_limit() {''',
    "index lock test",
)

replace_once(
    "README.md",
    '''Before updating `HEAD`, it rechecks the live `HEAD` and index. The ref update uses Git's expected-old-value compare-and-swap behavior, so concurrent repository changes cause the operation to fail rather than overwrite newer state. Unstaged worktree content is never committed.''',
    '''Before updating `HEAD`, it acquires Git's worktree-specific index lock, then rechecks the live `HEAD` and staged tree while holding that lock. The lock remains held through Git's expected-old-value compare-and-swap ref update, so cooperating Git index writers cannot enter between validation and the `HEAD` move, while concurrent ref changes still cause the operation to fail. Unstaged worktree content is never committed.''',
    "repository mutation documentation",
)

replace_once(
    "README.md",
    '''| `staged index changed...` | The index changed while the model request was in flight. |
| Git signing failure | Configure Git signing or rerun with `--no-sign`. |''',
    '''| `staged index changed...` | The index changed while the model request was in flight. |
| `unable to lock staged index...` | Another Git process holds or is creating the index lock; let it finish and retry. |
| Git signing failure | Configure Git signing or rerun with `--no-sign`. |''',
    "index lock troubleshooting",
)
