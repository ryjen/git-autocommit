use anyhow::{Context, Result, anyhow, bail};
use clap::{ArgAction, Parser};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use tempfile::{NamedTempFile, TempDir};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8000/v1";
const DEFAULT_MODEL: &str = "dubnium-local";
const DEFAULT_TIMEOUT_SECONDS: f64 = 120.0;
const DEFAULT_MAX_DIFF_BYTES: usize = 120_000;
const DEFAULT_MAX_COMMITS: usize = 8;
const SYSTEM_PROMPT: &str = include_str!("../prompts/system.md");
const PLAN_PROMPT: &str = include_str!("../prompts/plan.md");

#[derive(Debug, Parser)]
#[command(
    name = "git-autocommit",
    about = "Split staged changes into atomic signed Conventional Commits.",
    after_help = "Configuration is loaded from .git/autocommit.toml. CLI and environment values take precedence. Normal commit hooks are not run."
)]
struct Cli {
    #[arg(long)]
    base_url: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    timeout: Option<f64>,
    #[arg(long)]
    prompt_dir: Option<PathBuf>,
    #[arg(long, action = ArgAction::SetTrue)]
    single: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    no_single: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    show_prompt: bool,
    #[arg(long)]
    show_config: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    base_url: Option<String>,
    model: Option<String>,
    timeout_seconds: Option<f64>,
    prompt_dir: Option<PathBuf>,
    max_diff_bytes: Option<usize>,
    max_commits: Option<usize>,
    single_commit: Option<bool>,
}

#[derive(Debug, Serialize)]
struct Settings {
    base_url: String,
    model: String,
    timeout_seconds: f64,
    prompt_dir: PathBuf,
    max_diff_bytes: usize,
    max_commits: usize,
    single_commit: bool,
    config_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct PlanEntry {
    message: String,
    files: Vec<String>,
}

#[derive(Debug)]
struct Repo {
    root: PathBuf,
}

impl Repo {
    fn discover() -> Result<Self> {
        let output = run_git_raw(None, &["rev-parse", "--show-toplevel"], None)?;
        if !output.status.success() {
            bail!("not inside a Git work tree");
        }
        let root = String::from_utf8(output.stdout)
            .context("Git returned a non-UTF-8 repository path")?;
        Ok(Self {
            root: PathBuf::from(root.trim()),
        })
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        let output = run_git_raw(Some(&self.root), args, None)?;
        ensure_git_success(output)
    }

    fn git_env(&self, args: &[&str], extra_env: &[(&str, OsString)]) -> Result<String> {
        let output = run_git_raw(Some(&self.root), args, Some(extra_env))?;
        ensure_git_success(output)
    }

    fn config_path(&self) -> Result<PathBuf> {
        let value = self.git(&["rev-parse", "--git-path", "autocommit.toml"])?;
        let path = PathBuf::from(value.trim());
        Ok(if path.is_absolute() { path } else { self.root.join(path) })
    }
}

fn run_git_raw(
    root: Option<&Path>,
    args: &[&str],
    extra_env: Option<&[(&str, OsString)]>,
) -> Result<Output> {
    let mut command = Command::new("git");
    if let Some(root) = root {
        command.arg("-C").arg(root);
    }
    command.args(args);
    if let Some(extra_env) = extra_env {
        for (key, value) in extra_env {
            command.env(key, value);
        }
    }
    command.output().context("git is not installed or not in PATH")
}

fn ensure_git_success(output: Output) -> Result<String> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        bail!("{}", if !stderr.is_empty() { stderr } else { stdout });
    }
    String::from_utf8(output.stdout).context("Git returned non-UTF-8 output")
}

fn positive_f64(value: f64, source: &str) -> Result<f64> {
    if !value.is_finite() || value <= 0.0 {
        bail!("{source} must be a positive number");
    }
    Ok(value)
}

fn positive_usize(value: usize, source: &str) -> Result<usize> {
    if value == 0 {
        bail!("{source} must be a positive integer");
    }
    Ok(value)
}

fn env_string(name: &str, legacy: Option<&str>) -> Option<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| legacy.and_then(|name| env::var(name).ok()).filter(|value| !value.is_empty()))
}

fn env_parse<T: std::str::FromStr>(name: &str) -> Result<Option<T>> {
    env::var(name)
        .ok()
        .map(|value| value.parse().map_err(|_| anyhow!("invalid {name}: {value}")))
        .transpose()
}

fn load_file_config(path: &Path) -> Result<FileConfig> {
    if !path.exists() {
        return Ok(FileConfig::default());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("unable to read config {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))
}

fn default_prompt_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from(".local/share"))
        .join("git-autocommit")
}

fn resolve_settings(cli: &Cli, config: FileConfig, config_path: PathBuf) -> Result<Settings> {
    if cli.single && cli.no_single {
        bail!("--single and --no-single cannot be used together");
    }
    let timeout = cli
        .timeout
        .or(env_parse::<f64>("GIT_AUTOCOMMIT_TIMEOUT")?)
        .or(config.timeout_seconds)
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS);
    let max_diff_bytes = env_parse::<usize>("GIT_AUTOCOMMIT_MAX_DIFF_BYTES")?
        .or(config.max_diff_bytes)
        .unwrap_or(DEFAULT_MAX_DIFF_BYTES);
    let max_commits = env_parse::<usize>("GIT_AUTOCOMMIT_MAX_COMMITS")?
        .or(config.max_commits)
        .unwrap_or(DEFAULT_MAX_COMMITS);
    let single_commit = if cli.single {
        true
    } else if cli.no_single {
        false
    } else {
        config.single_commit.unwrap_or(false)
    };
    Ok(Settings {
        base_url: cli
            .base_url
            .clone()
            .or_else(|| env_string("GIT_AUTOCOMMIT_BASE_URL", Some("DUBNIUM_LOCAL_LLM_BASE_URL")))
            .or(config.base_url)
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned()),
        model: cli
            .model
            .clone()
            .or_else(|| env_string("GIT_AUTOCOMMIT_MODEL", Some("DUBNIUM_LOCAL_LLM_MODEL")))
            .or(config.model)
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned()),
        timeout_seconds: positive_f64(timeout, "timeout")?,
        prompt_dir: cli
            .prompt_dir
            .clone()
            .or_else(|| env::var_os("GIT_AUTOCOMMIT_PROMPT_DIR").map(PathBuf::from))
            .or(config.prompt_dir)
            .unwrap_or_else(default_prompt_dir),
        max_diff_bytes: positive_usize(max_diff_bytes, "max_diff_bytes")?,
        max_commits: positive_usize(max_commits, "max_commits")?,
        single_commit,
        config_path,
    })
}

fn nul_paths(repo: &Repo, args: &[&str]) -> Result<Vec<String>> {
    let output = repo.git(args)?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn repository_snapshot(repo: &Repo) -> Result<(String, String, Vec<String>)> {
    let files = nul_paths(repo, &["diff", "--cached", "--name-only", "--no-renames", "-z"])?;
    if files.is_empty() {
        bail!("no staged changes");
    }
    Ok((
        repo.git(&["rev-parse", "HEAD"])?.trim().to_owned(),
        repo.git(&["write-tree"])?.trim().to_owned(),
        files,
    ))
}

fn staged_context(repo: &Repo, files: &[String], max_bytes: usize) -> Result<String> {
    let names = repo.git(&["diff", "--cached", "--name-status", "--no-renames"])?;
    let stat = repo.git(&["diff", "--cached", "--stat", "--no-renames"])?;
    let mut chunks = vec![format!("Changed files:\n{}", names.trim()), format!("Diff stat:\n{}", stat.trim())];
    let mut remaining = max_bytes;
    for path in files {
        let diff = repo.git(&[
            "diff", "--cached", "--no-ext-diff", "--no-color", "--no-renames", "--binary", "--", path,
        ])?;
        let bytes = diff.as_bytes();
        if bytes.len() > remaining {
            if remaining > 0 {
                chunks.push(format!(
                    "File: {path}\n{}\n[diff truncated]",
                    String::from_utf8_lossy(&bytes[..remaining])
                ));
            }
            chunks.push("[remaining file diffs omitted by git-autocommit]".to_owned());
            break;
        }
        chunks.push(format!("File: {path}\n{diff}"));
        remaining -= bytes.len();
    }
    Ok(chunks.join("\n\n"))
}

fn load_prompts(settings: &Settings) -> Result<(String, String)> {
    let system = settings.prompt_dir.join("system.md");
    let plan = settings.prompt_dir.join("plan.md");
    if system.is_file() && plan.is_file() {
        return Ok((
            fs::read_to_string(&system).with_context(|| format!("unable to read {}", system.display()))?,
            fs::read_to_string(&plan).with_context(|| format!("unable to read {}", plan.display()))?,
        ));
    }
    Ok((SYSTEM_PROMPT.to_owned(), PLAN_PROMPT.to_owned()))
}

fn render_plan_prompt(
    template: &str,
    context: &str,
    files: &[String],
    single: bool,
    max_commits: usize,
) -> Result<String> {
    let values = [
        (
            "grouping_instruction",
            if single {
                "Create exactly one commit containing every file.".to_owned()
            } else {
                "Split unrelated changes into separate atomic commits.".to_owned()
            },
        ),
        ("max_commits", max_commits.to_string()),
        ("files_json", serde_json::to_string(files)?),
        ("context", context.to_owned()),
    ];
    let mut rendered = template.to_owned();
    for (name, value) in values {
        let token = format!("{{{{{name}}}}}");
        if !rendered.contains(&token) {
            bail!("plan prompt is missing required token {token}");
        }
        rendered = rendered.replace(&token, &value);
    }
    Ok(rendered)
}

fn request_plan(settings: &Settings, system: &str, user: &str) -> Result<String> {
    let client = Client::builder()
        .timeout(Duration::from_secs_f64(settings.timeout_seconds))
        .build()?;
    let response = client
        .post(format!("{}/chat/completions", settings.base_url.trim_end_matches('/')))
        .json(&json!({
            "model": settings.model,
            "temperature": 0.1,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ]
        }))
        .send()
        .context("local AI unavailable")?
        .error_for_status()
        .context("local AI returned an error")?;
    let document: serde_json::Value = response.json().context("local AI returned invalid JSON")?;
    document["choices"][0]["message"]["content"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("local AI response message was not text"))
}

fn strip_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        let mut lines = trimmed.lines();
        lines.next();
        let mut body: Vec<&str> = lines.collect();
        body.pop();
        Box::leak(body.join("\n").into_boxed_str())
    } else {
        trimmed
    }
}

fn valid_conventional_message(message: &str) -> bool {
    let first = message.lines().next().unwrap_or_default();
    let Some((prefix, summary)) = first.split_once(": ") else { return false; };
    if summary.trim().is_empty() || summary.contains('\n') {
        return false;
    }
    let prefix = prefix.trim_end_matches('!');
    let kind = prefix.split_once('(').map(|(kind, _)| kind).unwrap_or(prefix);
    matches!(kind, "feat" | "fix" | "docs" | "style" | "refactor" | "perf" | "test" | "build" | "ci" | "chore" | "revert")
}

fn parse_plan(raw: &str, staged: &[String], max_commits: usize) -> Result<Vec<PlanEntry>> {
    let plan: Vec<PlanEntry> = serde_json::from_str(strip_fence(raw))
        .context("local AI did not return a JSON commit plan")?;
    if plan.is_empty() {
        bail!("local AI returned an empty commit plan");
    }
    if plan.len() > max_commits {
        bail!("commit plan exceeds the {max_commits}-commit limit");
    }
    let expected: BTreeSet<&str> = staged.iter().map(String::as_str).collect();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, entry) in plan.iter().enumerate() {
        if !valid_conventional_message(entry.message.trim()) {
            bail!("commit plan entry {} has an invalid Conventional Commit message", index + 1);
        }
        if entry.files.is_empty() {
            bail!("commit plan entry {} has no files", index + 1);
        }
        for file in &entry.files {
            *counts.entry(file.as_str()).or_default() += 1;
        }
    }
    let actual: BTreeSet<&str> = counts.keys().copied().collect();
    let duplicates: Vec<&str> = counts.iter().filter_map(|(path, count)| (*count > 1).then_some(*path)).collect();
    if !duplicates.is_empty() {
        bail!("commit plan duplicates paths: {}", duplicates.join(", "));
    }
    let unknown: Vec<&str> = actual.difference(&expected).copied().collect();
    if !unknown.is_empty() {
        bail!("commit plan invents paths: {}", unknown.join(", "));
    }
    let missing: Vec<&str> = expected.difference(&actual).copied().collect();
    if !missing.is_empty() {
        bail!("commit plan omits paths: {}", missing.join(", "));
    }
    Ok(plan)
}

fn assert_snapshot(repo: &Repo, head: &str, tree: &str) -> Result<()> {
    if repo.git(&["rev-parse", "HEAD"])?.trim() != head {
        bail!("HEAD changed while the commit plan was being generated");
    }
    if repo.git(&["write-tree"])?.trim() != tree {
        bail!("the staged index changed while the commit plan was being generated");
    }
    Ok(())
}

fn tree_entry(repo: &Repo, tree: &str, path: &str) -> Result<Option<(String, String)>> {
    let output = repo.git(&["ls-tree", "--full-tree", "-z", tree, "--", path])?;
    if output.is_empty() {
        return Ok(None);
    }
    let records: Vec<&str> = output.split('\0').filter(|record| !record.is_empty()).collect();
    if records.len() != 1 {
        bail!("unable to resolve staged tree entry for {path}");
    }
    let (metadata, actual_path) = records[0]
        .split_once('\t')
        .ok_or_else(|| anyhow!("invalid ls-tree output for {path}"))?;
    if actual_path != path {
        bail!("staged tree returned an unexpected path for {path}");
    }
    let mut parts = metadata.split_whitespace();
    let mode = parts.next().ok_or_else(|| anyhow!("missing mode for {path}"))?;
    parts.next();
    let object = parts.next().ok_or_else(|| anyhow!("missing object id for {path}"))?;
    Ok(Some((mode.to_owned(), object.to_owned())))
}

fn build_commit_tree(repo: &Repo, parent: &str, snapshot: &str, files: &[String]) -> Result<String> {
    let temp = TempDir::new()?;
    let index = temp.path().join("index");
    let env = [("GIT_INDEX_FILE", index.into_os_string())];
    repo.git_env(&["read-tree", parent], &env)?;
    for path in files {
        match tree_entry(repo, snapshot, path)? {
            Some((mode, object)) => {
                let cache = format!("{mode},{object},{path}");
                repo.git_env(&["update-index", "--add", "--cacheinfo", &cache], &env)?;
            }
            None => {
                repo.git_env(&["update-index", "--force-remove", "--", path], &env)?;
            }
        }
    }
    Ok(repo.git_env(&["write-tree"], &env)?.trim().to_owned())
}

fn create_signed_commit(repo: &Repo, tree: &str, parent: &str, message: &str) -> Result<String> {
    let mut file = NamedTempFile::new()?;
    writeln!(file, "{}", message.trim())?;
    let path = file.path().to_string_lossy().into_owned();
    Ok(repo
        .git(&["commit-tree", tree, "-p", parent, "-S", "-F", &path])?
        .trim()
        .to_owned())
}

fn create_commits(repo: &Repo, plan: &[PlanEntry], base_head: &str, snapshot: &str) -> Result<()> {
    let mut parent = base_head.to_owned();
    for entry in plan {
        let tree = build_commit_tree(repo, &parent, snapshot, &entry.files)?;
        parent = create_signed_commit(repo, &tree, &parent, &entry.message)?;
    }
    if repo.git(&["rev-parse", &format!("{parent}^{{tree}}")])?.trim() != snapshot {
        bail!("generated commits do not reproduce the original staged tree");
    }
    assert_snapshot(repo, base_head, snapshot)?;
    repo.git(&["update-ref", "HEAD", &parent, base_head])?;
    Ok(())
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let repo = Repo::discover()?;
    let config_path = repo.config_path()?;
    let settings = resolve_settings(&cli, load_file_config(&config_path)?, config_path)?;
    if cli.show_config {
        println!("{}", serde_json::to_string_pretty(&settings)?);
        return Ok(());
    }
    let (head, snapshot, files) = repository_snapshot(&repo)?;
    let (system_prompt, plan_template) = load_prompts(&settings)?;
    let context = staged_context(&repo, &files, settings.max_diff_bytes)?;
    let plan_prompt = render_plan_prompt(
        &plan_template,
        &context,
        &files,
        settings.single_commit,
        settings.max_commits,
    )?;
    if cli.show_prompt {
        println!("SYSTEM PROMPT\n\n{}\n\nPLAN PROMPT\n\n{}", system_prompt.trim(), plan_prompt.trim());
        return Ok(());
    }
    let plan = parse_plan(
        &request_plan(&settings, &system_prompt, &plan_prompt)?,
        &files,
        settings.max_commits,
    )?;
    if settings.single_commit && plan.len() != 1 {
        bail!("local AI ignored single-commit mode");
    }
    for (index, entry) in plan.iter().enumerate() {
        println!("{}. {}", index + 1, entry.message);
        for file in &entry.files {
            println!("   {file}");
        }
    }
    if !cli.dry_run {
        assert_snapshot(&repo, &head, &snapshot)?;
        create_commits(&repo, &plan, &head, &snapshot)?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("git-autocommit: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_complete_atomic_plan() {
        let staged = vec!["a".to_owned(), "dir/b".to_owned()];
        let plan = parse_plan(
            r#"[{"message":"feat(core): add behavior","files":["a"]},{"message":"test: cover behavior","files":["dir/b"]}]"#,
            &staged,
            8,
        )
        .unwrap();
        assert_eq!(plan.len(), 2);
    }

    #[test]
    fn rejects_duplicate_paths() {
        let staged = vec!["a".to_owned()];
        let error = parse_plan(
            r#"[{"message":"feat: one","files":["a"]},{"message":"test: two","files":["a"]}]"#,
            &staged,
            8,
        )
        .unwrap_err();
        assert!(error.to_string().contains("duplicates"));
    }

    #[test]
    fn cli_can_disable_configured_single_mode() {
        let cli = Cli::try_parse_from(["git-autocommit", "--no-single"]).unwrap();
        let settings = resolve_settings(
            &cli,
            FileConfig { single_commit: Some(true), ..Default::default() },
            PathBuf::from("x"),
        )
        .unwrap();
        assert!(!settings.single_commit);
    }
}
