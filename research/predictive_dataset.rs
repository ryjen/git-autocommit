use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const OBSERVATION_SCHEMA: &str = "change.observation/v1";
const TARGET_SCHEMA: &str = "commit.structure/v1";
const RECORD_SCHEMA: &str = "predictive.dataset-record/v1";
const FEATURE_PROFILE: &str = "structural-v1";
const LOW_VALUE_PROFILE: &str = "git-autocommit-default-v1";
const SAMPLE_DOMAIN: &[u8] = b"git-autocommit predictive sample v1";
const TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const TOKEN_FILE_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN_FILE";

#[derive(Debug, Clone)]
pub struct ExtractConfig {
    pub repo: PathBuf,
    pub base: String,
    pub tip: String,
    pub min_commits: usize,
    pub max_commits: usize,
    pub max_paths: usize,
    pub max_record_bytes: usize,
}

impl ExtractConfig {
    pub fn new(repo: PathBuf, base: String, tip: String) -> Self {
        Self {
            repo,
            base,
            tip,
            min_commits: 2,
            max_commits: 8,
            max_paths: 1_024,
            max_record_bytes: 1_048_576,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DatasetRecord {
    pub schema: &'static str,
    pub sample_id: String,
    pub observation: Observation,
    pub target: Target,
    pub provenance: Provenance,
}

#[derive(Debug, Serialize)]
pub struct Observation {
    pub schema: &'static str,
    pub feature_profile: &'static str,
    pub paths: Vec<PathObservation>,
}

#[derive(Debug, Serialize)]
pub struct PathObservation {
    pub id: String,
    pub path: EncodedPath,
    pub status: PathStatus,
    pub binary: bool,
    pub additions: Option<usize>,
    pub deletions: Option<usize>,
    pub path_depth: usize,
    pub suffix: Option<String>,
    pub low_value: bool,
}

#[derive(Debug, Serialize)]
pub struct EncodedPath {
    pub encoding: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathStatus {
    Added,
    Modified,
    Deleted,
    TypeChanged,
}

#[derive(Debug, Serialize)]
pub struct Target {
    pub schema: &'static str,
    pub groups: Vec<TargetGroup>,
}

#[derive(Debug, Serialize)]
pub struct TargetGroup {
    pub id: String,
    pub paths: Vec<String>,
    pub source_commit_oids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Provenance {
    pub oid_algorithm: String,
    pub base_oid: String,
    pub tip_oid: String,
    pub window_commit_oids: Vec<String>,
    pub extractor_version: String,
    pub extraction: ExtractionPolicy,
}

#[derive(Debug, Serialize)]
pub struct ExtractionPolicy {
    pub min_commits: usize,
    pub max_commits: usize,
    pub max_paths: usize,
    pub max_record_bytes: usize,
    pub feature_profile: &'static str,
    pub low_value_profile: &'static str,
    pub rename_detection: bool,
}

#[derive(Debug)]
struct DiffStats {
    additions: Option<usize>,
    deletions: Option<usize>,
}

pub fn extract(config: &ExtractConfig) -> Result<DatasetRecord> {
    validate_config(config)?;

    let base_oid = resolve_commit(&config.repo, &config.base)
        .with_context(|| format!("unable to resolve base revision {}", config.base))?;
    let tip_oid = resolve_commit(&config.repo, &config.tip)
        .with_context(|| format!("unable to resolve tip revision {}", config.tip))?;
    if base_oid == tip_oid {
        bail!("ineligible sample: base and tip resolve to the same commit");
    }

    validate_attribute_isolation(&config.repo, &tip_oid)?;

    let oid_algorithm = git_text(&config.repo, &["rev-parse", "--show-object-format"], None)?;
    let commits = first_parent_window(&config.repo, &base_oid, &tip_oid)?;
    if commits.len() < config.min_commits {
        bail!(
            "ineligible sample: window has {} commits but min_commits is {}",
            commits.len(),
            config.min_commits
        );
    }
    if commits.len() > config.max_commits {
        bail!(
            "ineligible sample: window has {} commits but max_commits is {}",
            commits.len(),
            config.max_commits
        );
    }

    let statuses = aggregate_statuses(&config.repo, &base_oid, &tip_oid)?;
    if statuses.is_empty() {
        bail!("ineligible sample: aggregate base..tip change is empty");
    }
    if statuses.len() > config.max_paths {
        bail!(
            "ineligible sample: aggregate change has {} paths but max_paths is {}",
            statuses.len(),
            config.max_paths
        );
    }
    validate_diff_attributes(&config.repo, &tip_oid, statuses.keys())?;
    let stats = aggregate_numstat(&config.repo, &base_oid, &tip_oid)?;
    if statuses.keys().ne(stats.keys()) {
        bail!("ineligible sample: status and numstat path sets differ");
    }

    let observed_paths: BTreeSet<Vec<u8>> = statuses.keys().cloned().collect();
    let commit_paths = historical_commit_paths(&config.repo, &base_oid, &commits)?;
    for (index, paths) in commit_paths.iter().enumerate() {
        if paths.is_empty() {
            bail!(
                "ineligible sample: historical commit {} has no file-level changes",
                commits[index]
            );
        }
        for path in paths {
            if !observed_paths.contains(path) {
                bail!(
                    "ineligible sample: historical path {} is absent from aggregate base..tip observation",
                    display_path(path)
                );
            }
        }
    }

    let sample_id = sample_id(&config.repo, &oid_algorithm, &base_oid, &commits, config)?;
    let mut path_ids = BTreeMap::new();
    let mut observations = Vec::with_capacity(statuses.len());
    for (index, (path, status)) in statuses.iter().enumerate() {
        let id = format!("p{index:04}");
        path_ids.insert(path.clone(), id.clone());
        let stat = stats
            .get(path)
            .context("internal error: missing numstat entry for observed path")?;
        let binary = stat.additions.is_none() && stat.deletions.is_none();
        if stat.additions.is_none() != stat.deletions.is_none() {
            bail!(
                "ineligible sample: inconsistent binary numstat for {}",
                display_path(path)
            );
        }
        observations.push(PathObservation {
            id,
            path: encode_path(path),
            status: *status,
            binary,
            additions: stat.additions,
            deletions: stat.deletions,
            path_depth: path_depth(path),
            suffix: path_suffix(path),
            low_value: is_low_value(path),
        });
    }

    let groups = build_groups(&commits, &commit_paths, &path_ids)?;
    let record = DatasetRecord {
        schema: RECORD_SCHEMA,
        sample_id,
        observation: Observation {
            schema: OBSERVATION_SCHEMA,
            feature_profile: FEATURE_PROFILE,
            paths: observations,
        },
        target: Target {
            schema: TARGET_SCHEMA,
            groups,
        },
        provenance: Provenance {
            oid_algorithm,
            base_oid,
            tip_oid,
            window_commit_oids: commits,
            extractor_version: format!("git-autocommit/{}", env!("CARGO_PKG_VERSION")),
            extraction: ExtractionPolicy {
                min_commits: config.min_commits,
                max_commits: config.max_commits,
                max_paths: config.max_paths,
                max_record_bytes: config.max_record_bytes,
                feature_profile: FEATURE_PROFILE,
                low_value_profile: LOW_VALUE_PROFILE,
                rename_detection: false,
            },
        },
    };

    let encoded = serde_json::to_vec(&record).context("unable to serialize dataset record")?;
    if encoded.len() > config.max_record_bytes {
        bail!(
            "ineligible sample: serialized record is {} bytes but max_record_bytes is {}",
            encoded.len(),
            config.max_record_bytes
        );
    }

    Ok(record)
}

pub fn write_record(record: &DatasetRecord, output: Option<&Path>) -> Result<()> {
    let mut encoded = serde_json::to_vec(record).context("unable to serialize dataset record")?;
    encoded.push(b'\n');
    match output {
        Some(path) => {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .with_context(|| {
                    format!(
                        "unable to create output {}; existing files are never overwritten",
                        path.display()
                    )
                })?;
            file.write_all(&encoded)
                .with_context(|| format!("unable to write {}", path.display()))?;
        }
        None => {
            io::stdout()
                .lock()
                .write_all(&encoded)
                .context("unable to write dataset record to stdout")?;
        }
    }
    Ok(())
}

fn validate_config(config: &ExtractConfig) -> Result<()> {
    if config.min_commits == 0 {
        bail!("min_commits must be at least 1");
    }
    if config.max_commits < config.min_commits {
        bail!("max_commits must be greater than or equal to min_commits");
    }
    if config.max_paths == 0 {
        bail!("max_paths must be at least 1");
    }
    if config.max_record_bytes == 0 {
        bail!("max_record_bytes must be at least 1");
    }
    Ok(())
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
}

fn first_parent_window(repo: &Path, base_oid: &str, tip_oid: &str) -> Result<Vec<String>> {
    let range = format!("{base_oid}..{tip_oid}");
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
        bail!("ineligible sample: tip is not after base on a usable first-parent window");
    }

    let mut expected_parent = base_oid.to_owned();
    for commit in &commits {
        let line = git_text(repo, &["rev-list", "--parents", "-n", "1", commit], None)?;
        let fields: Vec<&str> = line.split_ascii_whitespace().collect();
        if fields.len() != 2 {
            bail!("ineligible sample: merge or parentless commit {commit} is in the window");
        }
        if fields[0] != commit.as_str() {
            bail!("git returned unexpected commit identity while validating {commit}");
        }
        if fields[1] != expected_parent {
            bail!(
                "ineligible sample: base is not the direct first-parent predecessor of the selected window"
            );
        }
        expected_parent.clone_from(commit);
    }
    if commits.last().map(String::as_str) != Some(tip_oid) {
        bail!("ineligible sample: tip is not the end of the selected first-parent window");
    }
    Ok(commits)
}

fn validate_attribute_isolation(repo: &Path, tip_oid: &str) -> Result<()> {
    let info_attributes = PathBuf::from(git_text(
        repo,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/attributes",
        ],
        None,
    )?);
    match fs::symlink_metadata(&info_attributes) {
        Ok(_) => {
            bail!(
                "ineligible repository: {} is present; repository-local info/attributes can override historical attributes",
                info_attributes.display()
            );
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "unable to inspect repository-local attributes {}",
                    info_attributes.display()
                )
            });
        }
    }

    let attr_source = format!("--attr-source={tip_oid}");
    let tree = format!("{tip_oid}^{{tree}}");
    git_text(repo, &[&attr_source, "rev-parse", "--verify", &tree], None).context(
        "historical attribute isolation requires Git with global --attr-source support (Git 2.41+)",
    )?;
    Ok(())
}

fn validate_diff_attributes<'a, I>(repo: &Path, tip_oid: &str, paths: I) -> Result<()>
where
    I: IntoIterator<Item = &'a Vec<u8>>,
{
    let mut input = Vec::new();
    let mut expected = 0usize;
    for path in paths {
        input.extend_from_slice(path);
        input.push(0);
        expected += 1;
    }

    let attr_source = format!("--attr-source={tip_oid}");
    let output = git_bytes(
        repo,
        &[&attr_source, "check-attr", "--stdin", "-z", "diff"],
        Some(&input),
    )?;
    let fields: Vec<&[u8]> = output.split(|byte| *byte == 0).collect();
    let payload = fields.strip_suffix(&[&[][..]]).unwrap_or(&fields);
    if payload.len() != expected.saturating_mul(3) {
        bail!("unexpected git check-attr output while validating historical diff attributes");
    }

    for chunk in payload.chunks_exact(3) {
        if chunk[1] != b"diff" {
            bail!("unexpected attribute name while validating historical diff attributes");
        }
        if chunk[2] != b"unspecified" && chunk[2] != b"set" && chunk[2] != b"unset" {
            bail!(
                "ineligible sample: historical path {} uses config-dependent diff driver {:?}",
                display_path(chunk[0]),
                String::from_utf8_lossy(chunk[2])
            );
        }
    }
    Ok(())
}

fn aggregate_statuses(
    repo: &Path,
    base_oid: &str,
    tip_oid: &str,
) -> Result<BTreeMap<Vec<u8>, PathStatus>> {
    let attr_source = format!("--attr-source={tip_oid}");
    let output = git_bytes(
        repo,
        &[
            &attr_source,
            "diff",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--diff-algorithm=myers",
            "--ignore-submodules=none",
            "--name-status",
            "-z",
            base_oid,
            tip_oid,
            "--",
        ],
        None,
    )?;
    parse_name_status(&output)
}

fn aggregate_numstat(
    repo: &Path,
    base_oid: &str,
    tip_oid: &str,
) -> Result<BTreeMap<Vec<u8>, DiffStats>> {
    let attr_source = format!("--attr-source={tip_oid}");
    let output = git_bytes(
        repo,
        &[
            &attr_source,
            "diff",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--diff-algorithm=myers",
            "--ignore-submodules=none",
            "--numstat",
            "-z",
            base_oid,
            tip_oid,
            "--",
        ],
        None,
    )?;
    parse_numstat(&output)
}

fn historical_commit_paths(
    repo: &Path,
    base_oid: &str,
    commits: &[String],
) -> Result<Vec<BTreeSet<Vec<u8>>>> {
    let mut result = Vec::with_capacity(commits.len());
    let mut parent = base_oid.to_owned();
    for commit in commits {
        let output = git_bytes(
            repo,
            &[
                "diff",
                "--no-renames",
                "--no-ext-diff",
                "--no-textconv",
                "--ignore-submodules=none",
                "--name-only",
                "-z",
                &parent,
                commit,
                "--",
            ],
            None,
        )?;
        let paths: BTreeSet<Vec<u8>> = output
            .split(|byte| *byte == 0)
            .filter(|field| !field.is_empty())
            .map(|field| field.to_vec())
            .collect();
        result.push(paths);
        parent.clone_from(commit);
    }
    Ok(result)
}

fn parse_name_status(raw: &[u8]) -> Result<BTreeMap<Vec<u8>, PathStatus>> {
    let fields: Vec<&[u8]> = raw
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect();
    let mut result = BTreeMap::new();
    let mut index = 0;
    while index < fields.len() {
        let field = fields[index];
        let (status_bytes, path, consumed) = match field.iter().position(|byte| *byte == b'\t') {
            Some(tab) => (&field[..tab], &field[tab + 1..], 1),
            None => {
                let path = fields
                    .get(index + 1)
                    .context("malformed NUL-delimited name-status output")?;
                (field, *path, 2)
            }
        };
        if path.is_empty() {
            bail!("malformed name-status output contains an empty path");
        }
        let status = match status_bytes {
            b"A" => PathStatus::Added,
            b"M" => PathStatus::Modified,
            b"D" => PathStatus::Deleted,
            b"T" => PathStatus::TypeChanged,
            other => bail!(
                "unsupported no-rename diff status {}",
                String::from_utf8_lossy(other)
            ),
        };
        if result.insert(path.to_vec(), status).is_some() {
            bail!("duplicate path in aggregate name-status output");
        }
        index += consumed;
    }
    Ok(result)
}

fn parse_numstat(raw: &[u8]) -> Result<BTreeMap<Vec<u8>, DiffStats>> {
    let mut result = BTreeMap::new();
    for field in raw
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
    {
        let first_tab = field
            .iter()
            .position(|byte| *byte == b'\t')
            .context("malformed numstat output: missing additions separator")?;
        let second_tab = field[first_tab + 1..]
            .iter()
            .position(|byte| *byte == b'\t')
            .map(|offset| first_tab + 1 + offset)
            .context("malformed numstat output: missing deletions separator")?;
        let additions = &field[..first_tab];
        let deletions = &field[first_tab + 1..second_tab];
        let path = &field[second_tab + 1..];
        if path.is_empty() {
            bail!("malformed numstat output contains an empty path");
        }
        let stats = if additions == b"-" && deletions == b"-" {
            DiffStats {
                additions: None,
                deletions: None,
            }
        } else {
            DiffStats {
                additions: Some(parse_usize_ascii(additions, "additions")?),
                deletions: Some(parse_usize_ascii(deletions, "deletions")?),
            }
        };
        if result.insert(path.to_vec(), stats).is_some() {
            bail!("duplicate path in aggregate numstat output");
        }
    }
    Ok(result)
}

fn parse_usize_ascii(value: &[u8], label: &str) -> Result<usize> {
    let text = std::str::from_utf8(value)
        .with_context(|| format!("numstat {label} is not valid ASCII/UTF-8"))?;
    text.parse::<usize>()
        .with_context(|| format!("invalid numstat {label} value {text:?}"))
}

fn build_groups(
    commits: &[String],
    commit_paths: &[BTreeSet<Vec<u8>>],
    path_ids: &BTreeMap<Vec<u8>, String>,
) -> Result<Vec<TargetGroup>> {
    let mut union = UnionFind::new(commits.len());
    let mut owner_by_path: BTreeMap<&[u8], usize> = BTreeMap::new();
    for (commit_index, paths) in commit_paths.iter().enumerate() {
        for path in paths {
            if let Some(previous) = owner_by_path.insert(path.as_slice(), commit_index) {
                union.join(previous, commit_index);
            }
        }
    }

    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for index in 0..commits.len() {
        let root = union.root(index);
        members.entry(root).or_default().push(index);
    }
    let mut components: Vec<Vec<usize>> = members.into_values().collect();
    components.sort_by_key(|indices| indices[0]);

    let mut seen_paths = BTreeSet::new();
    let mut groups = Vec::with_capacity(components.len());
    for (group_index, indices) in components.into_iter().enumerate() {
        let mut group_paths = BTreeSet::new();
        let mut source_commit_oids = Vec::with_capacity(indices.len());
        for index in indices {
            source_commit_oids.push(commits[index].clone());
            group_paths.extend(commit_paths[index].iter().cloned());
        }
        let mut ids = Vec::with_capacity(group_paths.len());
        for path in group_paths {
            let id = path_ids.get(&path).with_context(|| {
                format!(
                    "internal error: target path {} is not in observation",
                    display_path(&path)
                )
            })?;
            if !seen_paths.insert(path) {
                bail!("internal error: target path assigned to more than one group");
            }
            ids.push(id.clone());
        }
        if ids.is_empty() {
            bail!("internal error: target group is empty");
        }
        groups.push(TargetGroup {
            id: format!("g{group_index:04}"),
            paths: ids,
            source_commit_oids,
        });
    }
    if seen_paths.len() != path_ids.len() {
        bail!("internal error: target groups do not cover every observed path");
    }
    Ok(groups)
}

fn sample_id(
    repo: &Path,
    oid_algorithm: &str,
    base_oid: &str,
    commits: &[String],
    config: &ExtractConfig,
) -> Result<String> {
    let mut preimage = Vec::new();
    push_field(&mut preimage, SAMPLE_DOMAIN);
    push_field(&mut preimage, OBSERVATION_SCHEMA.as_bytes());
    push_field(&mut preimage, FEATURE_PROFILE.as_bytes());
    push_field(&mut preimage, LOW_VALUE_PROFILE.as_bytes());
    push_field(&mut preimage, oid_algorithm.as_bytes());
    push_field(&mut preimage, base_oid.as_bytes());
    for commit in commits {
        push_field(&mut preimage, commit.as_bytes());
    }
    for value in [
        config.min_commits,
        config.max_commits,
        config.max_paths,
        config.max_record_bytes,
    ] {
        push_field(&mut preimage, value.to_string().as_bytes());
    }
    let digest = git_text(repo, &["hash-object", "--stdin"], Some(&preimage))?;
    Ok(format!("git-blob-{oid_algorithm}:{digest}"))
}

fn push_field(buffer: &mut Vec<u8>, field: &[u8]) {
    buffer.extend_from_slice(&(field.len() as u64).to_be_bytes());
    buffer.extend_from_slice(field);
}

fn encode_path(path: &[u8]) -> EncodedPath {
    match std::str::from_utf8(path) {
        Ok(value) => EncodedPath {
            encoding: "utf8",
            value: value.to_owned(),
        },
        Err(_) => EncodedPath {
            encoding: "base64",
            value: base64_standard(path),
        },
    }
}

fn path_depth(path: &[u8]) -> usize {
    path.iter().filter(|byte| **byte == b'/').count() + 1
}

fn path_suffix(path: &[u8]) -> Option<String> {
    let name = path.rsplit(|byte| *byte == b'/').next()?;
    let dot = name.iter().rposition(|byte| *byte == b'.')?;
    if dot == 0 || dot + 1 == name.len() {
        return None;
    }
    std::str::from_utf8(&name[dot..])
        .ok()
        .map(ToOwned::to_owned)
}

fn is_low_value(path: &[u8]) -> bool {
    const FILE_NAMES: [&[u8]; 3] = [b"Cargo.lock", b"flake.lock", b"package-lock.json"];
    const FRAGMENTS: [&[u8]; 2] = [b"/generated/", b"/vendor/"];
    const SUFFIXES: [&[u8]; 2] = [b".min.js", b".min.css"];

    let name = path.rsplit(|byte| *byte == b'/').next().unwrap_or(path);
    FILE_NAMES.contains(&name)
        || FRAGMENTS
            .iter()
            .any(|fragment| contains_bytes(path, fragment))
        || SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn base64_standard(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(c & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

fn display_path(path: &[u8]) -> String {
    match std::str::from_utf8(path) {
        Ok(value) => value.to_owned(),
        Err(_) => format!("<base64:{}>", base64_standard(path)),
    }
}

fn git_text(repo: &Path, args: &[&str], stdin: Option<&[u8]>) -> Result<String> {
    let bytes = git_bytes(repo, args, stdin)?;
    let text = String::from_utf8(bytes).context("Git returned non-UTF-8 metadata")?;
    Ok(text.trim().to_owned())
}

fn git_bytes(repo: &Path, args: &[&str], stdin: Option<&[u8]>) -> Result<Vec<u8>> {
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
        .arg("-c")
        .arg(format!("core.attributesFile={null_device}"))
        .arg("-c")
        .arg("core.bigFileThreshold=512m")
        .arg("--no-replace-objects")
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null_device)
        .env("GIT_ATTR_NOSYSTEM", "1")
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
    Ok(output.stdout)
}

#[derive(Debug)]
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
        }
    }

    fn root(&mut self, value: usize) -> usize {
        let parent = self.parent[value];
        if parent == value {
            value
        } else {
            let root = self.root(parent);
            self.parent[value] = root;
            root
        }
    }

    fn join(&mut self, left: usize, right: usize) {
        let left_root = self.root(left);
        let right_root = self.root(right);
        if left_root != right_root {
            let (keep, merge) = if left_root < right_root {
                (left_root, right_root)
            } else {
                (right_root, left_root)
            };
            self.parent[merge] = keep;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encoding_matches_standard_vectors() {
        assert_eq!(base64_standard(b""), "");
        assert_eq!(base64_standard(b"f"), "Zg==");
        assert_eq!(base64_standard(b"fo"), "Zm8=");
        assert_eq!(base64_standard(b"foo"), "Zm9v");
        assert_eq!(base64_standard(&[0xff, 0x00]), "/wA=");
    }

    #[test]
    fn non_utf8_paths_use_base64() {
        let path = encode_path(&[0xff, b'a']);
        assert_eq!(path.encoding, "base64");
        assert_eq!(path.value, "/2E=");
    }

    #[test]
    fn low_value_profile_matches_current_defaults() {
        assert!(is_low_value(b"Cargo.lock"));
        assert!(is_low_value(b"src/vendor/generated.c"));
        assert!(is_low_value(b"web/app.min.js"));
        assert!(!is_low_value(b"src/app.rs"));
    }

    #[test]
    fn path_suffix_is_byte_preserving_when_utf8() {
        assert_eq!(path_suffix(b"src/lib.rs").as_deref(), Some(".rs"));
        assert_eq!(path_suffix(b".gitignore"), None);
        assert_eq!(path_suffix(b"README"), None);
    }
}
