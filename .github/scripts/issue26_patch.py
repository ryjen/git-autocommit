from pathlib import Path
import re


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


def sub_once(text: str, pattern: str, replacement: str, label: str) -> str:
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.DOTALL)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return updated


# Cargo should point directly at the consolidated application again.
manifest_path = Path("Cargo.toml")
manifest = manifest_path.read_text()
manifest = replace_once(
    manifest,
    'path = "src/entrypoint.rs"',
    'path = "src/app.rs"',
    "Cargo binary path",
)
manifest_path.write_text(manifest)


app_path = Path("src/app.rs")
app = app_path.read_text()

app = replace_once(
    app,
    "use std::io::{Read, Write};",
    "use std::io::{IsTerminal, Read, Write};",
    "IsTerminal import",
)

app = replace_once(
    app,
    'const BEARER_TOKEN_FILE_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN_FILE";\n',
    'const BEARER_TOKEN_FILE_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN_FILE";\nconst REVIEW_ENV: &str = "GIT_AUTOCOMMIT_REVIEW";\n',
    "review environment constant",
)

app = replace_once(
    app,
    "const DEFAULT_SIGN_COMMITS: bool = true;\n",
    "const DEFAULT_SIGN_COMMITS: bool = true;\nconst DEFAULT_REVIEW_BEFORE_COMMIT: bool = true;\n",
    "review default constant",
)

app = replace_once(
    app,
    'const PLAN_PROMPT: &str = include_str!("../prompts/plan.md");\n',
    'const PLAN_PROMPT: &str = include_str!("../prompts/plan.md");\nconst ACTIVE_GIT_OPERATIONS: &[(&str, &str)] = &[\n    ("MERGE_HEAD", "merge"),\n    ("CHERRY_PICK_HEAD", "cherry-pick"),\n    ("REVERT_HEAD", "revert"),\n    ("rebase-merge", "rebase"),\n    ("rebase-apply", "rebase/am"),\n    ("sequencer", "sequenced cherry-pick/revert"),\n    ("BISECT_START", "bisect"),\n];\n',
    "active Git operation constants",
)

app = replace_once(
    app,
    "    #[arg(long, action = ArgAction::SetTrue)]\n    no_sign: bool,\n    #[arg(long)]\n    dry_run: bool,\n",
    "    #[arg(long, action = ArgAction::SetTrue)]\n    no_sign: bool,\n    #[arg(long, action = ArgAction::SetTrue)]\n    review: bool,\n    #[arg(long, action = ArgAction::SetTrue)]\n    no_review: bool,\n    #[arg(long)]\n    dry_run: bool,\n",
    "native review CLI flags",
)

app = replace_once(
    app,
    "    sign_commits: Option<bool>,\n",
    "    sign_commits: Option<bool>,\n    review_before_commit: Option<bool>,\n",
    "review file config",
)

app = replace_once(
    app,
    "    sign_commits: bool,\n    low_value_file_names: Vec<String>,\n",
    "    sign_commits: bool,\n    review_before_commit: bool,\n    low_value_file_names: Vec<String>,\n",
    "review resolved settings",
)

review_helpers = r'''

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewChoice {
    Commit,
    Retry,
    Abort,
}

fn require_review_terminal() -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!(
            "review is enabled but stdin is not interactive or stdout is not a terminal; use --no-review to explicitly allow unattended commits"
        );
    }
    Ok(())
}

fn parse_review_choice(input: &str) -> Option<ReviewChoice> {
    match input.trim().to_ascii_lowercase().as_str() {
        "c" | "commit" => Some(ReviewChoice::Commit),
        "r" | "retry" => Some(ReviewChoice::Retry),
        "a" | "abort" | "q" | "quit" => Some(ReviewChoice::Abort),
        _ => None,
    }
}

fn read_review_choice() -> Result<ReviewChoice> {
    loop {
        eprint!("\n[c] commit  [r] retry  [a] abort\n> ");
        std::io::stderr()
            .flush()
            .context("unable to flush review prompt")?;
        let mut input = String::new();
        let bytes = std::io::stdin()
            .read_line(&mut input)
            .context("unable to read review choice")?;
        if bytes == 0 {
            return Ok(ReviewChoice::Abort);
        }
        if let Some(choice) = parse_review_choice(&input) {
            return Ok(choice);
        }
        eprintln!("Enter c, r, or a.");
    }
}

fn terminal_safe_path(path: &str) -> String {
    let mut rendered = String::with_capacity(path.len());
    for character in path.chars() {
        if character.is_control() || unsafe_message_character(character) {
            rendered.extend(character.escape_default());
        } else {
            rendered.push(character);
        }
    }
    rendered
}

fn terminal_safe_paths(paths: &[&str]) -> String {
    paths
        .iter()
        .map(|path| terminal_safe_path(path))
        .collect::<Vec<_>>()
        .join(", ")
}

fn print_plan(plan: &[PlanEntry]) {
    for (index, entry) in plan.iter().enumerate() {
        println!("{}. {}", index + 1, entry.message.trim());
        for file in &entry.files {
            println!("   {}", terminal_safe_path(file));
        }
    }
}

fn retry_plan_prompt(plan_prompt: &str, attempt: usize) -> String {
    format!(
        "{plan_prompt}\n\nThe previous valid commit plan was rejected by human review. This is human retry attempt {attempt}. Return a complete alternative JSON array for the same staged changes. When another valid plan is plausible, change the grouping and/or commit-message wording. Re-check every deterministic rule. Do not explain the alternative."
    )
}
'''

app = replace_once(
    app,
    "struct PlanEntry {\n    message: String,\n    files: Vec<String>,\n}\n",
    "struct PlanEntry {\n    message: String,\n    files: Vec<String>,\n}\n" + review_helpers,
    "review helpers",
)

operation_guard = r'''

fn assert_safe_repository_state(repo: &Repo) -> Result<()> {
    for (marker, operation) in ACTIVE_GIT_OPERATIONS {
        let path = repo.git_path(marker)?;
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                bail!(
                    "refusing to run during an active Git {operation} operation ({marker}); complete or abort it first"
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                bail!(
                    "unable to inspect Git operation state at {}: {error}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}
'''

app = replace_once(
    app,
    "}\n\nstruct IndexLock {\n",
    "}" + operation_guard + "\nstruct IndexLock {\n",
    "active operation guard",
)

sign_resolution = '''    let sign_commits = resolve_toggle(
        cli.sign,
        cli.no_sign,
        "GIT_AUTOCOMMIT_SIGN_COMMITS",
        config.sign_commits,
        DEFAULT_SIGN_COMMITS,
        "--sign",
        "--no-sign",
    )?;
'''
review_resolution = sign_resolution + '''    let review_before_commit = resolve_toggle(
        cli.review,
        cli.no_review,
        REVIEW_ENV,
        config.review_before_commit,
        DEFAULT_REVIEW_BEFORE_COMMIT,
        "--review",
        "--no-review",
    )?;
'''
app = replace_once(
    app,
    sign_resolution,
    review_resolution,
    "review setting resolution",
)

app = replace_once(
    app,
    "        single_commit,\n        sign_commits,\n        low_value_file_names: config\n",
    "        single_commit,\n        sign_commits,\n        review_before_commit,\n        low_value_file_names: config\n",
    "review setting construction",
)

app = replace_once(
    app,
    '        bail!("commit plan duplicates paths: {}", duplicates.join(", "));\n',
    '        bail!("commit plan duplicates paths: {}", terminal_safe_paths(&duplicates));\n',
    "duplicate path rendering",
)
app = replace_once(
    app,
    '        bail!("commit plan invents paths: {}", unknown.join(", "));\n',
    '        bail!("commit plan invents paths: {}", terminal_safe_paths(&unknown));\n',
    "invented path rendering",
)
app = replace_once(
    app,
    '        bail!("commit plan omits paths: {}", missing.join(", "));\n',
    '        bail!("commit plan omits paths: {}", terminal_safe_paths(&missing));\n',
    "missing path rendering",
)

new_run = r'''fn run() -> Result<()> {
    let cli = Cli::parse();
    let repo = Repo::discover()?;
    if !cli.show_config {
        assert_safe_repository_state(&repo)?;
    }
    let config_path = repo.config_path()?;
    let settings = resolve_settings(&cli, load_file_config(&config_path)?, config_path)?;
    if cli.show_config {
        println!("{}", serde_json::to_string_pretty(&settings)?);
        return Ok(());
    }

    let (head, snapshot, files) = repository_snapshot(&repo)?;
    let (system_prompt, plan_template) = load_prompts(&settings)?;
    let context = staged_context(&repo, &files, &settings)?;
    let plan_prompt = render_plan_prompt(
        &plan_template,
        &context,
        &files,
        settings.single_commit,
        settings.max_commits,
    )?;
    if cli.show_prompt {
        println!(
            "SYSTEM PROMPT\n\n{}\n\nPLAN PROMPT\n\n{}",
            system_prompt.trim(),
            plan_prompt.trim()
        );
        return Ok(());
    }
    validate_repairable_prompt_size(
        &system_prompt,
        &plan_prompt,
        settings.max_prompt_bytes,
    )?;
    if settings.review_before_commit && !cli.dry_run {
        require_review_terminal()?;
    }

    let mut active_prompt = plan_prompt.clone();
    let mut retry_attempt = 0_usize;
    loop {
        let plan = request_validated_plan(
            |prompt| request_plan(&settings, &system_prompt, prompt),
            &active_prompt,
            &files,
            settings.max_commits,
            settings.single_commit,
        )?;
        print_plan(&plan);

        if cli.dry_run {
            return Ok(());
        }
        if !settings.review_before_commit {
            assert_snapshot(&repo, &head, &snapshot)?;
            create_commits(&repo, &plan, &head, &snapshot, settings.sign_commits)?;
            return Ok(());
        }

        match read_review_choice()? {
            ReviewChoice::Commit => {
                assert_snapshot(&repo, &head, &snapshot)?;
                create_commits(&repo, &plan, &head, &snapshot, settings.sign_commits)?;
                return Ok(());
            }
            ReviewChoice::Abort => {
                eprintln!("Aborted; no commits created.");
                return Ok(());
            }
            ReviewChoice::Retry => {
                assert_snapshot(&repo, &head, &snapshot)?;
                retry_attempt = retry_attempt.saturating_add(1);
                active_prompt = retry_plan_prompt(&plan_prompt, retry_attempt);
                validate_repairable_prompt_size(
                    &system_prompt,
                    &active_prompt,
                    settings.max_prompt_bytes,
                )?;
                eprintln!("Regenerating commit plan...");
            }
        }
    }
}'''

app = sub_once(
    app,
    r"fn run\(\) -> Result<\(\)> \{.*?\n\}\n\nfn main\(\)",
    new_run + "\n\nfn main()",
    "consolidated run pipeline",
)

unit_tests = r'''

    #[test]
    fn review_choice_requires_an_explicit_action() {
        assert_eq!(parse_review_choice("c\n"), Some(ReviewChoice::Commit));
        assert_eq!(parse_review_choice("retry"), Some(ReviewChoice::Retry));
        assert_eq!(parse_review_choice("q"), Some(ReviewChoice::Abort));
        assert_eq!(parse_review_choice("\n"), None);
    }

    #[test]
    fn review_path_rendering_escapes_terminal_controls_and_formatting() {
        let rendered = terminal_safe_path("src/line\nbreak\t\u{1b}[31m\u{202e}.rs");
        assert!(!rendered.contains('\n'));
        assert!(!rendered.contains('\t'));
        assert!(!rendered.contains('\u{1b}'));
        assert!(!rendered.contains('\u{202e}'));
        assert!(rendered.contains("\\n"));
        assert!(rendered.contains("\\t"));
        assert!(rendered.contains("\\u{1b}"));
        assert!(rendered.contains("\\u{202e}"));
    }

    #[test]
    fn plan_path_diagnostics_use_terminal_safe_rendering() {
        let staged = vec!["safe.txt".to_owned()];
        let error = parse_plan(
            r#"[{"message":"test: invalid path","files":["unsafe\\npath"]}]"#,
            &staged,
            8,
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(!message.contains('\n'));
        assert!(message.contains("unsafe\\npath"));
    }

    #[test]
    fn retry_prompt_requests_an_alternative_without_embedding_prior_output() {
        let prompt = retry_plan_prompt("PLAN", 2);
        assert!(prompt.starts_with("PLAN"));
        assert!(prompt.contains("human retry attempt 2"));
        assert!(prompt.contains("alternative JSON array"));
    }
'''

app = replace_once(
    app,
    "    #[test]\n    fn bearer_token_file_accepts_no_terminator_lf_or_crlf() {\n",
    unit_tests + "\n    #[test]\n    fn bearer_token_file_accepts_no_terminator_lf_or_crlf() {\n",
    "review regression unit tests",
)

app_path.write_text(app)


# Documentation should describe the actual approval precondition.
readme_path = Path("README.md")
readme = readme_path.read_text()
readme = replace_once(
    readme,
    "Review requires an interactive standard input. Non-interactive callers must make the trust decision explicit with `--no-review`, `GIT_AUTOCOMMIT_REVIEW=false`, or `review_before_commit = false`.",
    "Review requires interactive standard input and terminal-attached standard output so the plan being approved is visible. Non-interactive or redirected-output callers must make the trust decision explicit with `--no-review`, `GIT_AUTOCOMMIT_REVIEW=false`, or `review_before_commit = false`.",
    "README review precondition",
)
readme_path.write_text(readme)

man_path = Path("man/git-autocommit.1")
man = man_path.read_text()
man = replace_once(
    man,
    "Require interactive review before committing. This is the default.",
    "Require interactive review with terminal-visible plan output before committing. This is the default.",
    "man review option",
)
man_path.write_text(man)

changelog_path = Path("CHANGELOG.md")
changelog = changelog_path.read_text()
changelog = replace_once(
    changelog,
    "## [Unreleased]\n\n",
    "## [Unreleased]\n\n### Changed\n\n- Consolidated review CLI/configuration resolution, active-operation preflight, and commit/retry/abort orchestration into the native application pipeline; removed the temporary wrapper entrypoint.\n\n### Security\n\n- Escape newline, carriage-return, tab, terminal-control, bidirectional, and zero-width characters in staged paths rendered for approval and path-validation diagnostics.\n- Route active Git-operation preflight through the same credential-scrubbing Git launcher used by the rest of the application.\n\n",
    "changelog issue 26 entry",
)
changelog_path.write_text(changelog)


# The temporary wrapper is intentionally removed from the final diff.
entrypoint = Path("src/entrypoint.rs")
if not entrypoint.exists():
    raise SystemExit("src/entrypoint.rs is missing before consolidation")
entrypoint.unlink()

print("issue #26 consolidation patch applied")
