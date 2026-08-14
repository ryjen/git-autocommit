use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod app {
    include!("app.rs");

    use std::io::IsTerminal as _;

    const REVIEW_ENV: &str = "GIT_AUTOCOMMIT_REVIEW";
    const DEFAULT_REVIEW_BEFORE_COMMIT: bool = true;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ReviewChoice {
        Commit,
        Retry,
        Abort,
    }

    fn split_review_args(
        args: impl IntoIterator<Item = OsString>,
    ) -> Result<(Vec<OsString>, Option<bool>)> {
        let mut filtered = Vec::new();
        let mut review_override = None;
        let mut options = true;

        for argument in args {
            if options && argument == std::ffi::OsStr::new("--") {
                options = false;
                filtered.push(argument);
                continue;
            }
            if options && argument == std::ffi::OsStr::new("--review") {
                if review_override == Some(false) {
                    bail!("--review and --no-review cannot be used together");
                }
                review_override = Some(true);
                continue;
            }
            if options && argument == std::ffi::OsStr::new("--no-review") {
                if review_override == Some(true) {
                    bail!("--review and --no-review cannot be used together");
                }
                review_override = Some(false);
                continue;
            }
            filtered.push(argument);
        }

        Ok((filtered, review_override))
    }

    fn exit_for_clap(error: clap::Error) -> ! {
        let kind = error.kind();
        let mut message = error.to_string();
        if matches!(kind, clap::error::ErrorKind::DisplayHelp) {
            if !message.contains("--no-review") {
                message.push_str(
                    "\nReview controls:\n      --review      Require interactive review before committing (default)\n      --no-review   Explicitly allow unattended commits\n",
                );
            }
            print!("{message}");
            std::process::exit(0);
        }
        if matches!(kind, clap::error::ErrorKind::DisplayVersion) {
            print!("{message}");
            std::process::exit(0);
        }
        eprint!("{message}");
        std::process::exit(2);
    }

    fn parse_cli_with_review() -> Result<(Cli, Option<bool>)> {
        let (arguments, review_override) = split_review_args(env::args_os())?;
        let cli = match Cli::try_parse_from(arguments) {
            Ok(cli) => cli,
            Err(error) => exit_for_clap(error),
        };
        Ok((cli, review_override))
    }

    fn load_config_with_review(path: &Path) -> Result<(FileConfig, Option<bool>)> {
        if !path.exists() {
            return Ok((FileConfig::default(), None));
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("unable to read config {}", path.display()))?;
        let mut value: toml::Value = toml::from_str(&text)
            .with_context(|| format!("invalid config {}", path.display()))?;
        let table = value
            .as_table_mut()
            .ok_or_else(|| anyhow!("invalid config {}: top level must be a table", path.display()))?;
        let review_before_commit = match table.remove("review_before_commit") {
            Some(value) => Some(value.as_bool().ok_or_else(|| {
                anyhow!(
                    "invalid config {}: review_before_commit must be a boolean",
                    path.display()
                )
            })?),
            None => None,
        };
        let config: FileConfig = value
            .try_into()
            .with_context(|| format!("invalid config {}", path.display()))?;
        Ok((config, review_before_commit))
    }

    fn select_review_before_commit(
        cli_override: Option<bool>,
        environment: Option<bool>,
        configured: Option<bool>,
    ) -> bool {
        cli_override
            .or(environment)
            .or(configured)
            .unwrap_or(DEFAULT_REVIEW_BEFORE_COMMIT)
    }

    fn resolve_review_before_commit(
        cli_override: Option<bool>,
        configured: Option<bool>,
    ) -> Result<bool> {
        Ok(select_review_before_commit(
            cli_override,
            env_parse::<bool>(REVIEW_ENV)?,
            configured,
        ))
    }

    fn print_resolved_config(settings: &Settings, review_before_commit: bool) -> Result<()> {
        let mut value = serde_json::to_value(settings)?;
        value
            .as_object_mut()
            .ok_or_else(|| anyhow!("resolved configuration was not a JSON object"))?
            .insert(
                "review_before_commit".to_owned(),
                serde_json::Value::Bool(review_before_commit),
            );
        println!("{}", serde_json::to_string_pretty(&value)?);
        Ok(())
    }

    fn require_review_terminal() -> Result<()> {
        if !std::io::stdin().is_terminal() {
            bail!(
                "review is enabled but stdin is not interactive; use --no-review to explicitly allow unattended commits"
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

    fn print_plan(plan: &[PlanEntry]) {
        for (index, entry) in plan.iter().enumerate() {
            println!("{}. {}", index + 1, entry.message);
            for file in &entry.files {
                println!("   {file}");
            }
        }
    }

    fn retry_plan_prompt(plan_prompt: &str, attempt: usize) -> String {
        format!(
            "{plan_prompt}\n\nThe previous valid commit plan was rejected by human review. This is human retry attempt {attempt}. Return a complete alternative JSON array for the same staged changes. When another valid plan is plausible, change the grouping and/or commit-message wording. Re-check every deterministic rule. Do not explain the alternative."
        )
    }

    fn run_with_review() -> Result<()> {
        let (cli, review_cli_override) = parse_cli_with_review()?;
        let repo = Repo::discover()?;
        let config_path = repo.config_path()?;
        let (file_config, review_config) = load_config_with_review(&config_path)?;
        let settings = resolve_settings(&cli, file_config, config_path)?;
        let review_before_commit =
            resolve_review_before_commit(review_cli_override, review_config)?;
        if cli.show_config {
            print_resolved_config(&settings, review_before_commit)?;
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
        if review_before_commit && !cli.dry_run {
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
            if !review_before_commit {
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
    }

    pub(super) fn run_cli() {
        let _legacy_main: fn() = main;
        if let Err(error) = run_with_review() {
            eprintln!("git-autocommit: {error:#}");
            std::process::exit(1);
        }
    }

    #[cfg(test)]
    mod review_tests {
        use super::*;

        #[test]
        fn review_defaults_on_and_uses_cli_environment_config_precedence() {
            assert!(select_review_before_commit(None, None, None));
            assert!(!select_review_before_commit(None, None, Some(false)));
            assert!(select_review_before_commit(None, Some(true), Some(false)));
            assert!(!select_review_before_commit(
                Some(false),
                Some(true),
                Some(true)
            ));
        }

        #[test]
        fn review_flags_are_stripped_and_conflicts_are_rejected() {
            let (arguments, review) = split_review_args(
                ["git-autocommit", "--review", "--dry-run"]
                    .into_iter()
                    .map(OsString::from),
            )
            .unwrap();
            assert_eq!(review, Some(true));
            assert_eq!(
                arguments,
                vec![OsString::from("git-autocommit"), OsString::from("--dry-run")]
            );

            let error = split_review_args(
                ["git-autocommit", "--review", "--no-review"]
                    .into_iter()
                    .map(OsString::from),
            )
            .unwrap_err();
            assert!(error.to_string().contains("cannot be used together"));
        }

        #[test]
        fn review_choice_requires_an_explicit_action() {
            assert_eq!(parse_review_choice("c\n"), Some(ReviewChoice::Commit));
            assert_eq!(parse_review_choice("retry"), Some(ReviewChoice::Retry));
            assert_eq!(parse_review_choice("q"), Some(ReviewChoice::Abort));
            assert_eq!(parse_review_choice("\n"), None);
        }

        #[test]
        fn retry_prompt_requests_an_alternative_without_embedding_prior_output() {
            let prompt = retry_plan_prompt("PLAN", 2);
            assert!(prompt.starts_with("PLAN"));
            assert!(prompt.contains("human retry attempt 2"));
            assert!(prompt.contains("alternative JSON array"));
        }
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
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                return Err(format!(
                    "refusing to run during an active Git {operation} operation ({marker}); complete or abort it first"
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "unable to inspect Git operation state at {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = assert_safe_repository_state() {
        eprintln!("git-autocommit: {error}");
        std::process::exit(1);
    }
    app::run_cli();
}
