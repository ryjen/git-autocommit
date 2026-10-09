use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::Builder;

use crate::predictive_dataset::{DatasetRecord, ExtractConfig, extract};

const MANIFEST_SCHEMA: &str = "predictive.corpus-manifest/v1";
const REPOSITORY_DOMAIN: &[u8] = b"git-autocommit predictive repository v1";
const TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const TOKEN_FILE_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN_FILE";

#[derive(Debug, Clone)]
pub struct CorpusConfig {
    pub repo: PathBuf,
    pub history_start: String,
    pub train_end: String,
    pub validation_end: String,
    pub history_end: String,
    pub output_dir: PathBuf,
    pub min_commits: usize,
    pub max_commits: usize,
    pub max_paths: usize,
    pub max_record_bytes: usize,
    pub max_history_commits: usize,
    pub max_split_bytes: usize,
}

impl CorpusConfig {
    pub fn new(
        repo: PathBuf,
        history_start: String,
        train_end: String,
        validation_end: String,
        history_end: String,
        output_dir: PathBuf,
    ) -> Self {
        Self {
            repo,
            history_start,
            train_end,
            validation_end,
            history_end,
            output_dir,
            min_commits: 2,
            max_commits: 8,
            max_paths: 1_024,
            max_record_bytes: 1_048_576,
            max_history_commits: 10_000,
            max_split_bytes: 268_435_456,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CorpusManifest {
    pub schema: &'static str,
    pub split_policy: &'static str,
    pub history_fingerprint: String,
    pub limits: CorpusLimits,
    pub oid_algorithm: String,
    pub git_version: String,
    pub history: HistoryManifest,
    pub extraction: ExtractionManifest,
    pub splits: Vec<SplitManifest>,
    pub total_samples: usize,
}

#[derive(Debug, Serialize)]
pub struct CorpusLimits {
    pub max_history_commits: usize,
    pub max_split_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct HistoryManifest {
    pub start_oid: String,
    pub end_oid: String,
    pub train_end_oid: String,
    pub validation_end_oid: String,
    pub commit_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ExtractionManifest {
    pub observation_schema: &'static str,
    pub target_schema: &'static str,
    pub feature_profile: &'static str,
    pub low_value_profile: &'static str,
    pub extractor_version: String,
    pub rename_detection: bool,
    pub content_bearing_features: bool,
    pub min_commits: usize,
    pub max_commits: usize,
    pub max_paths: usize,
    pub max_record_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct SplitManifest {
    pub name: &'static str,
    pub file: String,
    pub blob_id: String,
    pub first_commit_oid: String,
    pub last_commit_oid: String,
    pub commit_count: usize,
    pub candidate_windows: usize,
    pub sample_count: usize,
    pub exclusions: BTreeMap<String, usize>,
}

#[derive(Debug)]
struct SplitRange {
    name: &'static str,
    commits: Vec<String>,
    predecessor_oid: String,
}

pub fn build_corpus(config: &CorpusConfig) -> Result<CorpusManifest> {
    validate_config(config)?;
    if config.output_dir.exists() {
        bail!(
            "output directory {} already exists; corpus generation never overwrites",
            config.output_dir.display()
        );
    }

    let history_start = resolve_commit(&config.repo, &config.history_start)?;
    let train_end = resolve_commit(&config.repo, &config.train_end)?;
    let validation_end = resolve_commit(&config.repo, &config.validation_end)?;
    let history_end = resolve_commit(&config.repo, &config.history_end)?;
    let history_predecessor = direct_parent(&config.repo, &history_start)?;
    let oid_algorithm = git_text(&config.repo, &["rev-parse", "--show-object-format"], None)?;
    let git_version = git_text(&config.repo, &["--version"], None)?;

    let commits = first_parent_range(
        &config.repo,
        &history_predecessor,
        &history_end,
        config.max_history_commits,
    )?;
    if commits.len() > config.max_history_commits {
        bail!(
            "selected history has {} commits but max_history_commits is {}",
            commits.len(),
            config.max_history_commits
        );
    }
    if commits.first().map(String::as_str) != Some(history_start.as_str()) {
        bail!("history_start is not the first commit in the selected first-parent range");
    }

    let train_index = position(&commits, &train_end, "train_end")?;
    let validation_index = position(&commits, &validation_end, "validation_end")?;
    let end_index = position(&commits, &history_end, "history_end")?;
    if end_index + 1 != commits.len() {
        bail!("history_end is not the end of the selected first-parent range");
    }
    if train_index >= validation_index || validation_index >= end_index {
        bail!(
            "split boundaries must satisfy history_start <= train_end < validation_end < history_end"
        );
    }

    let train = commits[..=train_index].to_vec();
    let validation = commits[train_index + 1..=validation_index].to_vec();
    let test = commits[validation_index + 1..=end_index].to_vec();
    for (name, split) in [
        ("train", train.as_slice()),
        ("validation", validation.as_slice()),
        ("test", test.as_slice()),
    ] {
        if split.len() < config.min_commits {
            bail!(
                "{name} split has {} commits but min_commits is {}",
                split.len(),
                config.min_commits
            );
        }
    }

    let split_ranges = vec![
        SplitRange {
            name: "train",
            commits: train,
            predecessor_oid: history_predecessor,
        },
        SplitRange {
            name: "validation",
            commits: validation,
            predecessor_oid: commits[train_index].clone(),
        },
        SplitRange {
            name: "test",
            commits: test,
            predecessor_oid: commits[validation_index].clone(),
        },
    ];

    validate_disjoint_commits(&split_ranges)?;

    let parent = output_parent(&config.output_dir);
    let staging = Builder::new()
        .prefix(".predictive-corpus-")
        .tempdir_in(parent)
        .with_context(|| format!("unable to create staging directory in {}", parent.display()))?;

    let mut manifests = Vec::with_capacity(split_ranges.len());
    let mut global_sample_ids = BTreeSet::new();
    let mut total_samples = 0usize;

    for split in &split_ranges {
        let (bytes, manifest, sample_ids) = build_split(config, split, &oid_algorithm)?;
        if manifest.sample_count == 0 {
            bail!(
                "{} split produced no eligible samples; refusing to publish an unusable corpus",
                split.name
            );
        }
        for sample_id in sample_ids {
            if !global_sample_ids.insert(sample_id.clone()) {
                bail!("sample_id {sample_id:?} appears in more than one corpus split");
            }
        }
        total_samples += manifest.sample_count;
        fs::write(staging.path().join(&manifest.file), &bytes)
            .with_context(|| format!("unable to write staged {}", manifest.file))?;
        manifests.push(manifest);
    }

    let history_fingerprint =
        history_fingerprint(&config.repo, &oid_algorithm, &history_start, &history_end)?;
    let manifest = CorpusManifest {
        schema: MANIFEST_SCHEMA,
        split_policy: "explicit-temporal-boundaries/v1",
        history_fingerprint,
        limits: CorpusLimits {
            max_history_commits: config.max_history_commits,
            max_split_bytes: config.max_split_bytes,
        },
        oid_algorithm,
        git_version,
        history: HistoryManifest {
            start_oid: history_start,
            end_oid: history_end,
            train_end_oid: train_end,
            validation_end_oid: validation_end,
            commit_count: commits.len(),
        },
        extraction: ExtractionManifest {
            observation_schema: "change.observation/v1",
            target_schema: "commit.structure/v1",
            feature_profile: "structural-v1",
            low_value_profile: "git-autocommit-default-v1",
            extractor_version: format!("git-autocommit/{}", env!("CARGO_PKG_VERSION")),
            rename_detection: false,
            content_bearing_features: false,
            min_commits: config.min_commits,
            max_commits: config.max_commits,
            max_paths: config.max_paths,
            max_record_bytes: config.max_record_bytes,
        },
        splits: manifests,
        total_samples,
    };

    let mut manifest_bytes =
        serde_json::to_vec_pretty(&manifest).context("unable to serialize corpus manifest")?;
    manifest_bytes.push(b'\n');
    fs::write(staging.path().join("manifest.json"), &manifest_bytes)
        .context("unable to write staged corpus manifest")?;

    fs::create_dir(&config.output_dir).with_context(|| {
        format!(
            "unable to claim output directory {}; existing paths are never overwritten",
            config.output_dir.display()
        )
    })?;

    for split in &manifest.splits {
        fs::hard_link(
            staging.path().join(&split.file),
            config.output_dir.join(&split.file),
        )
        .with_context(|| {
            format!(
                "unable to publish {} without replacing an existing file",
                split.file
            )
        })?;
    }

    fs::hard_link(
        staging.path().join("manifest.json"),
        config.output_dir.join("manifest.json"),
    )
    .context("unable to publish manifest.json completion marker")?;

    Ok(manifest)
}

fn build_split(
    config: &CorpusConfig,
    split: &SplitRange,
    oid_algorithm: &str,
) -> Result<(Vec<u8>, SplitManifest, BTreeSet<String>)> {
    let mut bytes = Vec::new();
    let mut sample_ids = BTreeSet::new();
    let mut exclusions: BTreeMap<String, usize> = BTreeMap::new();
    let mut candidate_windows = 0usize;

    for start in 0..split.commits.len() {
        for size in config.min_commits..=config.max_commits {
            let end = start.saturating_add(size);
            if end > split.commits.len() {
                break;
            }
            candidate_windows += 1;
            let base = if start == 0 {
                split.predecessor_oid.clone()
            } else {
                split.commits[start - 1].clone()
            };
            let tip = split.commits[end - 1].clone();

            let mut extract_config = ExtractConfig::new(config.repo.clone(), base, tip);
            extract_config.min_commits = size;
            extract_config.max_commits = size;
            extract_config.max_paths = config.max_paths;
            extract_config.max_record_bytes = config.max_record_bytes;

            match extract(&extract_config) {
                Ok(record) => {
                    if !sample_ids.insert(record.sample_id.clone()) {
                        bail!(
                            "duplicate sample_id {:?} generated within {} split",
                            record.sample_id,
                            split.name
                        );
                    }
                    append_record(&mut bytes, &record)?;
                    if bytes.len() > config.max_split_bytes {
                        bail!(
                            "{} split JSONL is {} bytes but max_split_bytes is {}",
                            split.name,
                            bytes.len(),
                            config.max_split_bytes
                        );
                    }
                }
                Err(error) => {
                    if let Some(reason) = exclusion_reason(&error.to_string()) {
                        *exclusions.entry(reason.to_owned()).or_default() += 1;
                    } else {
                        return Err(error).with_context(|| {
                            format!(
                                "unexpected extractor failure for {} window {}..{}",
                                split.name, base, tip
                            )
                        });
                    }
                }
            }
        }
    }

    let file = format!("{}.jsonl", split.name);
    let blob_id = git_blob_id(&config.repo, oid_algorithm, &bytes)?;
    let manifest = SplitManifest {
        name: split.name,
        file,
        blob_id,
        first_commit_oid: split
            .commits
            .first()
            .context("internal error: empty split")?
            .clone(),
        last_commit_oid: split
            .commits
            .last()
            .context("internal error: empty split")?
            .clone(),
        commit_count: split.commits.len(),
        candidate_windows,
        sample_count: sample_ids.len(),
        exclusions,
    };

    Ok((bytes, manifest, sample_ids))
}

fn append_record(bytes: &mut Vec<u8>, record: &DatasetRecord) -> Result<()> {
    serde_json::to_writer(&mut *bytes, record).context("unable to serialize corpus record")?;
    bytes.push(b'\n');
    Ok(())
}

fn exclusion_reason(message: &str) -> Option<&'static str> {
    if message.contains("merge or parentless commit") {
        Some("merge_or_parentless_commit")
    } else if message.contains("historical path") && message.contains("absent from aggregate") {
        Some("transient_or_net_zero_path")
    } else if message.contains("aggregate base..tip change is empty") {
        Some("empty_aggregate_change")
    } else if message.contains("max_paths") {
        Some("path_limit")
    } else if message.contains("max_record_bytes") || message.contains("serialized record") {
        Some("record_size_limit")
    } else if message.contains("has no file-level changes") {
        Some("empty_historical_commit")
    } else if message.contains("unsupported no-rename diff status") {
        Some("unsupported_diff_status")
    } else {
        None
    }
}

fn validate_config(config: &CorpusConfig) -> Result<()> {
    if config.min_commits < 2 {
        bail!("corpus min_commits must be at least 2");
    }
    if config.max_commits < config.min_commits {
        bail!("max_commits must be greater than or equal to min_commits");
    }
    if config.max_paths == 0
        || config.max_record_bytes == 0
        || config.max_history_commits == 0
        || config.max_split_bytes == 0
    {
        bail!(
            "max_paths, max_record_bytes, max_history_commits, and max_split_bytes must be positive"
        );
    }
    if config.output_dir.as_os_str().is_empty() {
        bail!("output_dir must not be empty");
    }
    Ok(())
}

fn validate_disjoint_commits(splits: &[SplitRange]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for split in splits {
        for commit in &split.commits {
            if !seen.insert(commit.as_str()) {
                bail!("commit {commit} appears in more than one split");
            }
        }
    }
    Ok(())
}

fn position(commits: &[String], oid: &str, label: &str) -> Result<usize> {
    commits
        .iter()
        .position(|commit| commit == oid)
        .with_context(|| format!("{label} {oid} is outside the selected first-parent history"))
}

fn resolve_commit(repo: &Path, revision: &str) -> Result<String> {
    git_text(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
        None,
    )
    .with_context(|| format!("unable to resolve revision {revision:?}"))
}

fn direct_parent(repo: &Path, commit: &str) -> Result<String> {
    let line = git_text(repo, &["rev-list", "--parents", "-n", "1", commit], None)?;
    let fields: Vec<&str> = line.split_ascii_whitespace().collect();
    if fields.len() < 2 {
        bail!("history_start must have a parent so a staged-like base state exists");
    }
    Ok(fields[1].to_owned())
}

fn first_parent_range(
    repo: &Path,
    base: &str,
    tip: &str,
    max_history_commits: usize,
) -> Result<Vec<String>> {
    let range = format!("{base}..{tip}");
    let max_count = format!("--max-count={}", max_history_commits.saturating_add(1));
    let count_text = git_text(
        repo,
        &["rev-list", "--count", &max_count, "--first-parent", &range],
        None,
    )?;
    let bounded_count = count_text
        .parse::<usize>()
        .context("Git returned an invalid bounded first-parent history count")?;
    if bounded_count > max_history_commits {
        bail!(
            "selected history exceeds max_history_commits {}",
            max_history_commits
        );
    }

    let output = git_text(
        repo,
        &["rev-list", "--first-parent", "--reverse", &range],
        None,
    )?;
    let commits: Vec<String> = output
        .lines()
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if commits.is_empty() {
        bail!("selected history range is empty");
    }
    if commits.len() != bounded_count {
        bail!("selected history changed while corpus boundaries were being resolved");
    }

    let mut expected_parent = base.to_owned();
    for commit in &commits {
        let line = git_text(repo, &["rev-list", "--parents", "-n", "1", commit], None)?;
        let fields: Vec<&str> = line.split_ascii_whitespace().collect();
        if fields.is_empty() || fields[0] != commit.as_str() {
            bail!("Git returned unexpected commit identity while validating history");
        }
        if fields.len() < 2 || fields[1] != expected_parent {
            bail!("selected commits do not form a contiguous first-parent history");
        }
        expected_parent.clone_from(commit);
    }
    Ok(commits)
}

fn history_fingerprint(
    repo: &Path,
    oid_algorithm: &str,
    history_start: &str,
    history_end: &str,
) -> Result<String> {
    let mut preimage = Vec::new();
    push_field(&mut preimage, REPOSITORY_DOMAIN);
    push_field(&mut preimage, oid_algorithm.as_bytes());
    push_field(&mut preimage, history_start.as_bytes());
    push_field(&mut preimage, history_end.as_bytes());
    let digest = git_text(repo, &["hash-object", "--stdin"], Some(&preimage))?;
    Ok(format!("git-blob-{oid_algorithm}:{digest}"))
}

fn git_blob_id(repo: &Path, oid_algorithm: &str, bytes: &[u8]) -> Result<String> {
    let digest = git_text(repo, &["hash-object", "--stdin"], Some(bytes))?;
    Ok(format!("git-blob-{oid_algorithm}:{digest}"))
}

fn push_field(buffer: &mut Vec<u8>, field: &[u8]) {
    buffer.extend_from_slice(&(field.len() as u64).to_be_bytes());
    buffer.extend_from_slice(field);
}

fn output_parent(output: &Path) -> &Path {
    match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

fn git_text(repo: &Path, args: &[&str], stdin: Option<&[u8]>) -> Result<String> {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }

    let null_device = if cfg!(windows) { "NUL" } else { "/dev/null" };
    command
        .arg("-C")
        .arg(repo)
        .arg("--no-replace-objects")
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null_device)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove(TOKEN_ENV)
        .env_remove(TOKEN_FILE_ENV)
        .env_remove("GIT_EXTERNAL_DIFF")
        .env_remove("GIT_DIFF_OPTS")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }

    let mut child = command.spawn().context("unable to execute Git")?;
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .context("unable to open Git stdin")?
            .write_all(input)
            .context("unable to write Git stdin")?;
    }
    let output = child.wait_with_output().context("unable to wait for Git")?;
    if !output.status.success() {
        let stderr = &output.stderr[..output.stderr.len().min(4_096)];
        bail!(
            "Git command failed ({}): {}",
            args.join(" "),
            String::from_utf8_lossy(stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .context("Git returned non-UTF-8 metadata")
        .map(|value| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusion_reasons_are_bounded_categories() {
        assert_eq!(
            exclusion_reason(
                "ineligible sample: historical path x is absent from aggregate base..tip observation"
            ),
            Some("transient_or_net_zero_path")
        );
        assert_eq!(
            exclusion_reason("ineligible sample: aggregate base..tip change is empty"),
            Some("empty_aggregate_change")
        );
        assert_eq!(exclusion_reason("unexpected new error"), None);
    }

    #[test]
    fn output_parent_uses_current_directory_for_bare_name() {
        assert_eq!(output_parent(Path::new("corpus")), Path::new("."));
        assert_eq!(output_parent(Path::new("/tmp/corpus")), Path::new("/tmp"));
    }
}
