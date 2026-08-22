from pathlib import Path

cargo = Path('Cargo.toml')
text = cargo.read_text()
old = 'path = "src/entrypoint.rs"'
new = 'path = "src/app.rs"'
assert old in text
cargo.write_text(text.replace(old, new, 1))

app = Path('src/app.rs')
text = app.read_text()
old = 'use git_autocommit::validation::{PlanEntry, validate_requested_plan};'
new = 'use git_autocommit::validation::{PlanEntry, terminal_safe_path, validate_requested_plan};'
assert old in text
text = text.replace(old, new, 1)
marker = '\n#[path = "app/runtime.rs"]\nmod runtime;\n\nfn main() {\n    runtime::run_cli();\n}\n'
assert marker not in text
app.write_text(text.rstrip() + marker)

runtime = Path('src/app/runtime.rs')
text = runtime.read_text()
const_anchor = 'const MEDIUM_CONTEXT_DIFF_BYTES: usize = 32_000;\n'
assert const_anchor in text
operations = '''const ACTIVE_GIT_OPERATIONS: &[(&str, &str)] = &[\n    ("MERGE_HEAD", "merge"),\n    ("CHERRY_PICK_HEAD", "cherry-pick"),\n    ("REVERT_HEAD", "revert"),\n    ("rebase-merge", "rebase"),\n    ("rebase-apply", "rebase/am"),\n    ("sequencer", "sequenced cherry-pick/revert"),\n    ("BISECT_START", "bisect"),\n];\n\n'''
text = text.replace(const_anchor, const_anchor + '\n' + operations, 1)
function_anchor = 'fn require_review_terminal() -> Result<()> {\n'
assert function_anchor in text
safe_state = '''fn assert_safe_repository_state(repo: &Repo) -> Result<()> {\n    for (marker, operation) in ACTIVE_GIT_OPERATIONS {\n        let path = repo.git_path(marker)?;\n        match fs::symlink_metadata(&path) {\n            Ok(_) => {\n                bail!(\n                    "refusing to run during an active Git {operation} operation ({marker}); complete or abort it first"\n                );\n            }\n            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}\n            Err(error) => {\n                let display_path = terminal_safe_path(path.to_string_lossy().as_ref());\n                bail!("unable to inspect Git operation state at {display_path}: {error}");\n            }\n        }\n    }\n    Ok(())\n}\n\n'''
text = text.replace(function_anchor, safe_state + function_anchor, 1)
run_anchor = '    if cli.show_config {\n        println!("{}", serde_json::to_string_pretty(&settings)?);\n        return Ok(());\n    }\n\n    let (head, snapshot, files) = repository_snapshot(&repo)?;'
assert run_anchor in text
run_replace = '    if cli.show_config {\n        println!("{}", serde_json::to_string_pretty(&settings)?);\n        return Ok(());\n    }\n\n    assert_safe_repository_state(&repo)?;\n    let (head, snapshot, files) = repository_snapshot(&repo)?;'
text = text.replace(run_anchor, run_replace, 1)
runtime.write_text(text)

entrypoint = Path('src/entrypoint.rs')
assert entrypoint.exists()
entrypoint.unlink()
