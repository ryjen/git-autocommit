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
    '''fn assert_snapshot(repo: &Repo, head: &str, tree: &str) -> Result<()> {
    if repo.git(&["rev-parse", "HEAD"])?.trim() != head {
        bail!("HEAD changed while the commit plan was being generated");
    }
    if repo.git(&["write-tree"])?.trim() != tree {
        bail!("the staged index changed while the commit plan was being generated");
    }
    Ok(())
}''',
    '''fn assert_staged_tree(repo: &Repo, tree: &str) -> Result<()> {
    let output = run_git_raw(
        Some(&repo.root),
        &[
            "diff-index",
            "--cached",
            "--quiet",
            "--no-renames",
            tree,
            "--",
        ],
        None,
    )?;
    if output.status.success() {
        return Ok(());
    }
    if output.status.code() == Some(1) {
        bail!("the staged index changed while the commit plan was being generated");
    }
    ensure_git_success(output)?;
    Ok(())
}

fn assert_snapshot(repo: &Repo, head: &str, tree: &str) -> Result<()> {
    if repo.git(&["rev-parse", "HEAD"])?.trim() != head {
        bail!("HEAD changed while the commit plan was being generated");
    }
    assert_staged_tree(repo, tree)
}''',
    "read-only staged tree validation",
)

replace_once(
    "src/app.rs",
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
    }''',
    '''    #[test]
    fn index_lock_allows_validation_blocks_writes_and_cleans_up() {
        let temp = TempDir::new().unwrap();
        let repo = Repo {
            root: temp.path().to_path_buf(),
        };
        let init = run_git_raw(Some(&repo.root), &["init", "--quiet"], None).unwrap();
        assert!(init.status.success());
        fs::write(repo.root.join("staged.txt"), "content\n").unwrap();
        repo.git(&["add", "staged.txt"]).unwrap();
        let snapshot = repo.git(&["write-tree"]).unwrap();

        let lock = IndexLock::acquire(&repo).unwrap();
        assert_staged_tree(&repo, snapshot.trim()).unwrap();
        fs::write(repo.root.join("staged.txt"), "changed\n").unwrap();
        let blocked = run_git_raw(Some(&repo.root), &["add", "staged.txt"], None).unwrap();
        assert!(!blocked.status.success());
        assert!(String::from_utf8_lossy(&blocked.stderr).contains("index.lock"));
        assert_staged_tree(&repo, snapshot.trim()).unwrap();

        drop(lock);
        repo.git(&["add", "staged.txt"]).unwrap();
        assert!(assert_staged_tree(&repo, snapshot.trim()).is_err());
    }''',
    "locked validation test",
)
