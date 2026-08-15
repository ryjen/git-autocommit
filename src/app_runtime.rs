use std::io::IsTerminal as _;

const REVIEW_ENV: &str = "GIT_AUTOCOMMIT_REVIEW";
const DEFAULT_REVIEW_BEFORE_COMMIT: bool = true;
const DEFAULT_CONTEXT_MAX_DIFF_BYTES: usize = 64_000;
const DEFAULT_CONTEXT_MAX_PROMPT_BYTES: usize = 96_000;
const DEFAULT_PROMPT_HEADROOM_BYTES: usize = 40_000;
const SMALL_STAGED_DIFF_BYTES: usize = 16_000;
const MEDIUM_STAGED_DIFF_BYTES: usize = 96_000;
const SMALL_CONTEXT_DIFF_BYTES: usize = 12_000;
const MEDIUM_CONTEXT_DIFF_BYTES: usize = 32_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewChoice {
    Commit,
    Retry,
    Abort,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
struct ModelTokenUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Debug)]
struct ModelPlanResponse {
    content: String,
    usage: Option<ModelTokenUsage>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct UsageTotals {
    requests: usize,
    prompt_tokens: u64,
    prompt_reports: usize,
    completion_tokens: u64,
    completion_reports: usize,
    total_tokens: u64,
    total_reports: usize,
}

impl UsageTotals {
    fn record(&mut self, usage: Option<ModelTokenUsage>) {
        self.requests = self.requests.saturating_add(1);
        let Some(usage) = usage else {
            return;
        };
        if let Some(tokens) = usage.prompt_tokens {
            self.prompt_tokens = self.prompt_tokens.saturating_add(tokens);
            self.prompt_reports = self.prompt_reports.saturating_add(1);
        }
        if let Some(tokens) = usage.completion_tokens {
            self.completion_tokens = self.completion_tokens.saturating_add(tokens);
            self.completion_reports = self.completion_reports.saturating_add(1);
        }
        if let Some(tokens) = usage.total_tokens {
            self.total_tokens = self.total_tokens.saturating_add(tokens);
            self.total_reports = self.total_reports.saturating_add(1);
        }
    }

    fn summary(&self) -> String {
        let request_label = if self.requests == 1 { "request" } else { "requests" };
        let mut fields = vec![format!("Model usage: {} {request_label}", self.requests)];
        if self.prompt_reports == 0
            && self.completion_reports == 0
            && self.total_reports == 0
        {
            fields.push("endpoint did not report token counts".to_owned());
            return fields.join("; ");
        }
        if self.prompt_reports > 0 {
            fields.push(format!(
                "{} prompt tokens ({}/{})",
                self.prompt_tokens, self.prompt_reports, self.requests
            ));
        }
        if self.completion_reports > 0 {
            fields.push(format!(
                "{} completion tokens ({}/{})",
                self.completion_tokens, self.completion_reports, self.requests
            ));
        }
        if self.total_reports > 0 {
            fields.push(format!(
                "{} total tokens ({}/{})",
                self.total_tokens, self.total_reports, self.requests
            ));
        }
        fields.join("; ")
    }
}

fn split_review_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<(Vec<OsString>, Option<bool>, bool)> {
    let mut filtered = Vec::new();
    let mut review_override = None;
    let mut show_usage = false;
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
        if options && argument == std::ffi::OsStr::new("--show-usage") {
            show_usage = true;
            continue;
        }
        filtered.push(argument);
    }

    Ok((filtered, review_override, show_usage))
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
        if !message.contains("--show-usage") {
            message.push_str(
                "\nUsage controls:\n      --show-usage  Print model request/token usage to stderr when reported by the endpoint\n",
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

fn parse_cli_with_review() -> Result<(Cli, Option<bool>, bool)> {
    let (arguments, review_override, show_usage) = split_review_args(env::args_os())?;
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => exit_for_clap(error),
    };
    Ok((cli, review_override, show_usage))
}

fn load_config_with_review(path: &Path) -> Result<(FileConfig, Option<bool>)> {
    if !path.exists() {
        return Ok((FileConfig::default(), None));
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("unable to read config {}", path.display()))?;
    let mut value: toml::Value =
        toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))?;
    let table = value.as_table_mut().ok_or_else(|| {
        anyhow!(
            "invalid config {}: top level must be a table",
            path.display()
        )
    })?;
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

fn uses_default_limit(environment_name: &str, configured: Option<usize>) -> bool {
    env::var_os(environment_name).is_none() && configured.is_none()
}

fn apply_default_context_ceilings(
    settings: &mut Settings,
    adaptive_diff_default: bool,
    prompt_default: bool,
) {
    if adaptive_diff_default {
        settings.max_diff_bytes = DEFAULT_CONTEXT_MAX_DIFF_BYTES;
    }
    if prompt_default {
        settings.max_prompt_bytes = if adaptive_diff_default {
            DEFAULT_CONTEXT_MAX_PROMPT_BYTES
        } else {
            DEFAULT_CONTEXT_MAX_PROMPT_BYTES.max(
                settings
                    .max_diff_bytes
                    .saturating_add(DEFAULT_PROMPT_HEADROOM_BYTES),
            )
        };
    }
}

fn adaptive_diff_budget(total_staged_diff_bytes: usize) -> usize {
    if total_staged_diff_bytes <= SMALL_STAGED_DIFF_BYTES {
        SMALL_CONTEXT_DIFF_BYTES
    } else if total_staged_diff_bytes <= MEDIUM_STAGED_DIFF_BYTES {
        MEDIUM_CONTEXT_DIFF_BYTES
    } else {
        DEFAULT_CONTEXT_MAX_DIFF_BYTES
    }
}

fn staged_diff_bytes(repo: &Repo) -> Result<usize> {
    Ok(repo
        .git(&[
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-color",
            "--no-renames",
            "--no-textconv",
            "--",
        ])?
        .len())
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
        if unsafe_message_character(character) {
            rendered.extend(character.escape_default());
        } else {
            rendered.push(character);
        }
    }
    rendered
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

fn parse_model_plan_response(document: serde_json::Value) -> Result<ModelPlanResponse> {
    let content = document["choices"][0]["message"]["content"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("local AI response message was not text"))?;
    let usage = document
        .get("usage")
        .filter(|value| !value.is_null())
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    Ok(ModelPlanResponse { content, usage })
}

fn request_plan_with_usage(
    settings: &Settings,
    system: &str,
    user: &str,
) -> Result<ModelPlanResponse> {
    validate_prompt_size(system, user, settings.max_prompt_bytes)?;
    let request_url = model_request_url(&settings.base_url)?;
    let client = Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .timeout(Duration::from_secs_f64(settings.timeout_seconds))
        .build()?;
    let mut request = client.post(request_url).json(&json!({
        "model": settings.model,
        "temperature": 0.1,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    }));
    if let Some(token) = &settings.bearer_token {
        request = request.header(AUTHORIZATION, token.header_value());
    }
    let response = request.send().context("local AI unavailable")?;
    reject_redirect(response.status())?;
    let response = response
        .error_for_status()
        .context("local AI returned an error")?;
    validate_response_content_length(response.content_length(), MAX_AI_RESPONSE_BYTES)?;
    let body = read_response_body(response, MAX_AI_RESPONSE_BYTES)?;
    let document: serde_json::Value =
        serde_json::from_slice(&body).context("local AI returned invalid JSON")?;
    parse_model_plan_response(document)
}

fn print_usage(show_usage: bool, usage: &UsageTotals) {
    if show_usage {
        eprintln!("{}", usage.summary());
    }
}

fn run_with_review() -> Result<()> {
    let (cli, review_cli_override, show_usage) = parse_cli_with_review()?;
    let repo = Repo::discover()?;
    let config_path = repo.config_path()?;
    let (file_config, review_config) = load_config_with_review(&config_path)?;
    let adaptive_diff_default = uses_default_limit(
        "GIT_AUTOCOMMIT_MAX_DIFF_BYTES",
        file_config.max_diff_bytes,
    );
    let prompt_default = uses_default_limit(
        "GIT_AUTOCOMMIT_MAX_PROMPT_BYTES",
        file_config.max_prompt_bytes,
    );
    let mut settings = resolve_settings(&cli, file_config, config_path)?;
    apply_default_context_ceilings(&mut settings, adaptive_diff_default, prompt_default);
    let review_before_commit = resolve_review_before_commit(review_cli_override, review_config)?;
    if cli.show_config {
        print_resolved_config(&settings, review_before_commit)?;
        return Ok(());
    }

    let (head, snapshot, files) = repository_snapshot(&repo)?;
    if adaptive_diff_default {
        settings.max_diff_bytes = adaptive_diff_budget(staged_diff_bytes(&repo)?);
    }
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
    validate_repairable_prompt_size(&system_prompt, &plan_prompt, settings.max_prompt_bytes)?;
    if review_before_commit && !cli.dry_run {
        require_review_terminal()?;
    }

    let mut active_prompt = plan_prompt.clone();
    let mut retry_attempt = 0_usize;
    let mut usage = UsageTotals::default();
    loop {
        let plan = request_validated_plan(
            |prompt| {
                let response = request_plan_with_usage(&settings, &system_prompt, prompt)?;
                usage.record(response.usage);
                Ok(response.content)
            },
            &active_prompt,
            &files,
            settings.max_commits,
            settings.single_commit,
        )?;
        print_plan(&plan);

        if cli.dry_run {
            print_usage(show_usage, &usage);
            return Ok(());
        }
        if !review_before_commit {
            assert_snapshot(&repo, &head, &snapshot)?;
            create_commits(&repo, &plan, &head, &snapshot, settings.sign_commits)?;
            print_usage(show_usage, &usage);
            return Ok(());
        }

        match read_review_choice()? {
            ReviewChoice::Commit => {
                assert_snapshot(&repo, &head, &snapshot)?;
                create_commits(&repo, &plan, &head, &snapshot, settings.sign_commits)?;
                print_usage(show_usage, &usage);
                return Ok(());
            }
            ReviewChoice::Abort => {
                eprintln!("Aborted; no commits created.");
                print_usage(show_usage, &usage);
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

    fn default_settings() -> Settings {
        let cli = Cli::try_parse_from(["git-autocommit"]).unwrap();
        resolve_settings(&cli, FileConfig::default(), PathBuf::from("x")).unwrap()
    }

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
    fn wrapper_flags_are_stripped_and_review_conflicts_are_rejected() {
        let (arguments, review, show_usage) = split_review_args(
            ["git-autocommit", "--review", "--show-usage", "--dry-run"]
                .into_iter()
                .map(OsString::from),
        )
        .unwrap();
        assert_eq!(review, Some(true));
        assert!(show_usage);
        assert_eq!(
            arguments,
            vec![
                OsString::from("git-autocommit"),
                OsString::from("--dry-run")
            ]
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
    fn usage_flag_after_option_terminator_is_not_interpreted() {
        let (arguments, review, show_usage) = split_review_args(
            ["git-autocommit", "--", "--show-usage"]
                .into_iter()
                .map(OsString::from),
        )
        .unwrap();
        assert_eq!(review, None);
        assert!(!show_usage);
        assert_eq!(
            arguments,
            vec![
                OsString::from("git-autocommit"),
                OsString::from("--"),
                OsString::from("--show-usage")
            ]
        );
    }

    #[test]
    fn review_choice_requires_an_explicit_action() {
        assert_eq!(parse_review_choice("c\n"), Some(ReviewChoice::Commit));
        assert_eq!(parse_review_choice("retry"), Some(ReviewChoice::Retry));
        assert_eq!(parse_review_choice("q"), Some(ReviewChoice::Abort));
        assert_eq!(parse_review_choice("\n"), None);
    }

    #[test]
    fn review_path_rendering_escapes_terminal_controls() {
        let rendered = terminal_safe_path("src/\u{1b}[31m.rs");
        assert!(!rendered.contains('\u{1b}'));
        assert!(rendered.contains("\\u{1b}"));
    }

    #[test]
    fn retry_prompt_requests_an_alternative_without_embedding_prior_output() {
        let prompt = retry_plan_prompt("PLAN", 2);
        assert!(prompt.starts_with("PLAN"));
        assert!(prompt.contains("human retry attempt 2"));
        assert!(prompt.contains("alternative JSON array"));
    }

    #[test]
    fn model_response_usage_is_optional_and_partial() {
        let complete = parse_model_plan_response(json!({
            "choices": [{"message": {"content": "[]"}}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15}
        }))
        .unwrap();
        assert_eq!(
            complete.usage,
            Some(ModelTokenUsage {
                prompt_tokens: Some(12),
                completion_tokens: Some(3),
                total_tokens: Some(15),
            })
        );

        let partial = parse_model_plan_response(json!({
            "choices": [{"message": {"content": "[]"}}],
            "usage": {"prompt_tokens": 7}
        }))
        .unwrap();
        assert_eq!(partial.usage.unwrap().prompt_tokens, Some(7));
        assert_eq!(partial.usage.unwrap().completion_tokens, None);

        let missing = parse_model_plan_response(json!({
            "choices": [{"message": {"content": "[]"}}]
        }))
        .unwrap();
        assert_eq!(missing.usage, None);
    }

    #[test]
    fn usage_totals_accumulate_only_reported_fields() {
        let mut usage = UsageTotals::default();
        usage.record(Some(ModelTokenUsage {
            prompt_tokens: Some(100),
            completion_tokens: Some(20),
            total_tokens: Some(120),
        }));
        usage.record(None);
        usage.record(Some(ModelTokenUsage {
            prompt_tokens: Some(50),
            completion_tokens: None,
            total_tokens: Some(50),
        }));

        assert_eq!(usage.requests, 3);
        assert_eq!(usage.prompt_tokens, 150);
        assert_eq!(usage.prompt_reports, 2);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.completion_reports, 1);
        assert_eq!(usage.total_tokens, 170);
        assert_eq!(usage.total_reports, 2);
        assert_eq!(
            usage.summary(),
            "Model usage: 3 requests; 150 prompt tokens (2/3); 20 completion tokens (1/3); 170 total tokens (2/3)"
        );
    }

    #[test]
    fn missing_usage_is_reported_without_estimation() {
        let mut usage = UsageTotals::default();
        usage.record(None);
        assert_eq!(
            usage.summary(),
            "Model usage: 1 request; endpoint did not report token counts"
        );
    }

    #[test]
    fn adaptive_diff_budget_uses_small_medium_and_large_tiers() {
        assert_eq!(adaptive_diff_budget(0), SMALL_CONTEXT_DIFF_BYTES);
        assert_eq!(
            adaptive_diff_budget(SMALL_STAGED_DIFF_BYTES),
            SMALL_CONTEXT_DIFF_BYTES
        );
        assert_eq!(
            adaptive_diff_budget(SMALL_STAGED_DIFF_BYTES + 1),
            MEDIUM_CONTEXT_DIFF_BYTES
        );
        assert_eq!(
            adaptive_diff_budget(MEDIUM_STAGED_DIFF_BYTES),
            MEDIUM_CONTEXT_DIFF_BYTES
        );
        assert_eq!(
            adaptive_diff_budget(MEDIUM_STAGED_DIFF_BYTES + 1),
            DEFAULT_CONTEXT_MAX_DIFF_BYTES
        );
    }

    #[test]
    fn implicit_context_limits_use_reduced_default_ceilings() {
        let mut settings = default_settings();
        apply_default_context_ceilings(&mut settings, true, true);
        assert_eq!(settings.max_diff_bytes, DEFAULT_CONTEXT_MAX_DIFF_BYTES);
        assert_eq!(settings.max_prompt_bytes, DEFAULT_CONTEXT_MAX_PROMPT_BYTES);
    }

    #[test]
    fn explicit_diff_limit_keeps_compatible_prompt_headroom() {
        let mut settings = default_settings();
        settings.max_diff_bytes = 120_000;
        apply_default_context_ceilings(&mut settings, false, true);
        assert_eq!(settings.max_diff_bytes, 120_000);
        assert_eq!(settings.max_prompt_bytes, 160_000);
    }

    #[test]
    fn explicit_context_limits_are_not_rewritten() {
        let mut settings = default_settings();
        settings.max_diff_bytes = 27_000;
        settings.max_prompt_bytes = 75_000;
        apply_default_context_ceilings(&mut settings, false, false);
        assert_eq!(settings.max_diff_bytes, 27_000);
        assert_eq!(settings.max_prompt_bytes, 75_000);
    }
}
