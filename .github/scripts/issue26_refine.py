from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


path = Path("src/app.rs")
text = path.read_text()

text = replace_once(
    text,
    "    #[arg(long, action = ArgAction::SetTrue)]\n    review: bool,\n    #[arg(long, action = ArgAction::SetTrue)]\n    no_review: bool,\n",
    "    /// Require interactive review before committing (default).\n    #[arg(long, action = ArgAction::SetTrue)]\n    review: bool,\n    /// Explicitly allow unattended commits without interactive review.\n    #[arg(long, action = ArgAction::SetTrue)]\n    no_review: bool,\n",
    "native review help",
)

text = replace_once(
    text,
    '            r#"[{"message":"test: invalid path","files":["unsafe\\\\npath"]}]"#,\n',
    '            r#"[{"message":"test: invalid path","files":["unsafe\\npath"]}]"#,\n',
    "newline diagnostic fixture",
)

old_tree_entry = '''fn tree_entry(repo: &Repo, tree: &str, path: &str) -> Result<Option<(String, String)>> {
    let output = repo.git(&["ls-tree", "--full-tree", "-z", tree, "--", path])?;
    if output.is_empty() {
        return Ok(None);
    }
    let records: Vec<&str> = output
        .split('\\0')
        .filter(|record| !record.is_empty())
        .collect();
    if records.len() != 1 {
        bail!("unable to resolve staged tree entry for {path}");
    }
    let (metadata, actual_path) = records[0]
        .split_once('\\t')
        .ok_or_else(|| anyhow!("invalid ls-tree output for {path}"))?;
    if actual_path != path {
        bail!("staged tree returned an unexpected path for {path}");
    }
    let mut parts = metadata.split_whitespace();
    let mode = parts
        .next()
        .ok_or_else(|| anyhow!("missing mode for {path}"))?;
    parts.next();
    let object = parts
        .next()
        .ok_or_else(|| anyhow!("missing object id for {path}"))?;
    Ok(Some((mode.to_owned(), object.to_owned())))
}
'''
new_tree_entry = '''fn tree_entry(repo: &Repo, tree: &str, path: &str) -> Result<Option<(String, String)>> {
    let output = repo.git(&["ls-tree", "--full-tree", "-z", tree, "--", path])?;
    if output.is_empty() {
        return Ok(None);
    }
    let display_path = terminal_safe_path(path);
    let records: Vec<&str> = output
        .split('\\0')
        .filter(|record| !record.is_empty())
        .collect();
    if records.len() != 1 {
        bail!("unable to resolve staged tree entry for {display_path}");
    }
    let (metadata, actual_path) = records[0]
        .split_once('\\t')
        .ok_or_else(|| anyhow!("invalid ls-tree output for {display_path}"))?;
    if actual_path != path {
        bail!("staged tree returned an unexpected path for {display_path}");
    }
    let mut parts = metadata.split_whitespace();
    let mode = parts
        .next()
        .ok_or_else(|| anyhow!("missing mode for {display_path}"))?;
    parts.next();
    let object = parts
        .next()
        .ok_or_else(|| anyhow!("missing object id for {display_path}"))?;
    Ok(Some((mode.to_owned(), object.to_owned())))
}
'''
text = replace_once(text, old_tree_entry, new_tree_entry, "tree-entry path rendering")

marker = '''    #[test]
    fn review_choice_requires_an_explicit_action() {
'''
conflict_test = '''    #[test]
    fn native_review_flags_reject_conflicting_overrides() {
        let cli = Cli::try_parse_from(["git-autocommit", "--review", "--no-review"]).unwrap();
        let error = resolve_settings(&cli, FileConfig::default(), PathBuf::from("x")).unwrap_err();
        assert!(error.to_string().contains("cannot be used together"));
    }

'''
text = replace_once(text, marker, conflict_test + marker, "review flag conflict regression")

path.write_text(text)
print("issue #26 review fixes applied")
