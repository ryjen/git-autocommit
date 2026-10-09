use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const SINGLE_GROUP_BASELINE: &str = "sanity-single-group/v1";
pub const SINGLETON_BASELINE: &str = "sanity-singleton/v1";
pub const PARENT_DIRECTORY_BASELINE: &str = "parent-directory/v1";
pub const NEAREST_NEIGHBOR_BASELINE: &str = "nearest-neighbor-transfer/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineKind {
    SingleGroup,
    Singleton,
    ParentDirectory,
}

impl BaselineKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::SingleGroup => SINGLE_GROUP_BASELINE,
            Self::Singleton => SINGLETON_BASELINE,
            Self::ParentDirectory => PARENT_DIRECTORY_BASELINE,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetRecord {
    pub schema: String,
    pub sample_id: String,
    pub observation: Observation,
    pub target: Target,
    pub provenance: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub schema: String,
    pub feature_profile: String,
    pub paths: Vec<PathObservation>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathObservation {
    pub id: String,
    pub path: EncodedPath,
    pub status: String,
    pub binary: bool,
    pub additions: Option<usize>,
    pub deletions: Option<usize>,
    pub path_depth: usize,
    pub suffix: Option<String>,
    pub low_value: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodedPath {
    pub encoding: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub schema: String,
    pub groups: Vec<TargetGroup>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetGroup {
    pub id: String,
    pub paths: Vec<String>,
    pub source_commit_oids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    labels: Vec<usize>,
}

impl Partition {
    pub fn group_count(&self) -> usize {
        self.labels
            .iter()
            .copied()
            .max()
            .map_or(0, |value| value + 1)
    }
}

#[derive(Debug)]
struct PartitionCounts {
    contingency: BTreeMap<(usize, usize), u64>,
    target_sizes: BTreeMap<usize, u64>,
    predicted_sizes: BTreeMap<usize, u64>,
    total: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct PairCounts {
    pub true_positive: u64,
    pub false_positive: u64,
    pub false_negative: u64,
    pub true_negative: u64,
}

impl PairCounts {
    fn add(&mut self, other: Self) {
        self.true_positive += other.true_positive;
        self.false_positive += other.false_positive;
        self.false_negative += other.false_negative;
        self.true_negative += other.true_negative;
    }

    fn precision(self) -> Option<f64> {
        ratio(self.true_positive, self.true_positive + self.false_positive)
    }

    fn recall(self) -> Option<f64> {
        ratio(self.true_positive, self.true_positive + self.false_negative)
    }

    fn f1(self) -> Option<f64> {
        match (self.precision(), self.recall()) {
            (Some(precision), Some(recall)) if precision + recall > 0.0 => {
                Some(2.0 * precision * recall / (precision + recall))
            }
            (Some(0.0), Some(0.0)) => Some(0.0),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SampleMetrics {
    pub sample_id: String,
    pub path_count: usize,
    pub target_group_count: usize,
    pub predicted_group_count: usize,
    pub group_count_abs_error: usize,
    pub pair_counts: PairCounts,
    pub pairwise_precision: Option<f64>,
    pub pairwise_recall: Option<f64>,
    pub pairwise_f1: Option<f64>,
    pub adjusted_rand_index: f64,
    pub exact_partition_match: bool,
    pub has_binary: bool,
    pub has_low_value: bool,
    pub textual_change_lines: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AggregateMetrics {
    pub samples: usize,
    pub pair_counts: PairCounts,
    pub pairwise_precision: Option<f64>,
    pub pairwise_recall: Option<f64>,
    pub pairwise_f1: Option<f64>,
    pub mean_adjusted_rand_index: f64,
    pub exact_partition_match_rate: f64,
    pub mean_group_count_abs_error: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentMetrics {
    pub segment: String,
    #[serde(flatten)]
    pub metrics: AggregateMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluationReport {
    pub schema: &'static str,
    pub baseline: &'static str,
    pub observation_schema: &'static str,
    pub feature_profile: &'static str,
    pub sample_count: usize,
    pub aggregate: AggregateMetrics,
    pub by_path_count: Vec<SegmentMetrics>,
    pub by_target_group_count: Vec<SegmentMetrics>,
    pub by_binary_presence: Vec<SegmentMetrics>,
    pub by_low_value_presence: Vec<SegmentMetrics>,
    pub by_change_size: Vec<SegmentMetrics>,
    pub samples: Vec<SampleMetrics>,
}

pub fn parse_jsonl(input: &str) -> Result<Vec<DatasetRecord>> {
    let mut records = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record = serde_json::from_str::<DatasetRecord>(line)
            .with_context(|| format!("invalid dataset JSON on line {}", index + 1))?;
        records.push(record);
    }
    if records.is_empty() {
        bail!("dataset input contains no records");
    }
    Ok(records)
}

pub fn evaluate(records: &[DatasetRecord], baseline: BaselineKind) -> Result<EvaluationReport> {
    validate_record_set(records, "evaluation")?;

    let mut sample_metrics = Vec::with_capacity(records.len());
    for record in records {
        let predicted = predict(&record.observation, baseline)?;
        let target = target_partition(record)?;
        sample_metrics.push(score_sample(record, &target, &predicted)?);
    }

    Ok(build_report(sample_metrics, baseline.id()))
}

pub fn evaluate_nearest_neighbor(
    training: &[DatasetRecord],
    evaluation: &[DatasetRecord],
) -> Result<EvaluationReport> {
    validate_record_set(training, "training")?;
    validate_record_set(evaluation, "evaluation")?;

    let training_ids: BTreeSet<&str> = training
        .iter()
        .map(|record| record.sample_id.as_str())
        .collect();
    for record in evaluation {
        if training_ids.contains(record.sample_id.as_str()) {
            bail!(
                "training/evaluation sample_id overlap is not allowed: {:?}",
                record.sample_id
            );
        }
    }

    let mut training_commits = BTreeSet::new();
    for record in training {
        for oid in window_commit_oids(record, "training")? {
            training_commits.insert(oid);
        }
    }
    for record in evaluation {
        for oid in window_commit_oids(record, "evaluation")? {
            if training_commits.contains(oid) {
                bail!(
                    "training/evaluation historical commit overlap is not allowed: {oid}"
                );
            }
        }
    }

    let training_observations: Vec<&Observation> =
        training.iter().map(|record| &record.observation).collect();
    let mut sample_metrics = Vec::with_capacity(evaluation.len());
    for record in evaluation {
        let neighbor_index =
            nearest_observation_index(&record.observation, &training_observations)?;
        let neighbor = &training[neighbor_index];
        let predicted =
            transfer_partition(&record.observation, &neighbor.observation, &neighbor.target)?;
        let target = target_partition(record)?;
        sample_metrics.push(score_sample(record, &target, &predicted)?);
    }

    Ok(build_report(sample_metrics, NEAREST_NEIGHBOR_BASELINE))
}

fn validate_record_set(records: &[DatasetRecord], label: &str) -> Result<()> {
    if records.is_empty() {
        bail!("{label} dataset requires at least one record");
    }
    let mut seen_sample_ids = BTreeSet::new();
    for record in records {
        validate_record(record)?;
        if !seen_sample_ids.insert(record.sample_id.as_str()) {
            bail!("duplicate {label} dataset sample_id {:?}", record.sample_id);
        }
    }
    Ok(())
}

fn window_commit_oids<'a>(
    record: &'a DatasetRecord,
    label: &str,
) -> Result<Vec<&'a str>> {
    let values = record
        .provenance
        .get("window_commit_oids")
        .and_then(Value::as_array)
        .with_context(|| {
            format!(
                "{label} record {:?} is missing provenance.window_commit_oids",
                record.sample_id
            )
        })?;
    if values.is_empty() {
        bail!(
            "{label} record {:?} has an empty provenance.window_commit_oids",
            record.sample_id
        );
    }

    let mut seen = BTreeSet::new();
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let oid = value.as_str().with_context(|| {
            format!(
                "{label} record {:?} has a non-string window commit OID",
                record.sample_id
            )
        })?;
        if oid.is_empty() || !seen.insert(oid) {
            bail!(
                "{label} record {:?} has an empty or duplicate window commit OID",
                record.sample_id
            );
        }
        result.push(oid);
    }
    Ok(result)
}

fn build_report(sample_metrics: Vec<SampleMetrics>, baseline: &'static str) -> EvaluationReport {
    let aggregate = aggregate_metrics(sample_metrics.iter().collect());
    let by_path_count = segment(&sample_metrics, |sample| {
        path_count_bucket(sample.path_count)
    });
    let by_target_group_count = segment(&sample_metrics, |sample| {
        target_group_bucket(sample.target_group_count)
    });
    let by_binary_presence = segment(&sample_metrics, |sample| {
        if sample.has_binary {
            "binary-present"
        } else {
            "text-only"
        }
    });
    let by_low_value_presence = segment(&sample_metrics, |sample| {
        if sample.has_low_value {
            "low-value-present"
        } else {
            "no-low-value"
        }
    });
    let by_change_size = segment(&sample_metrics, |sample| {
        change_size_bucket(sample.textual_change_lines)
    });

    EvaluationReport {
        schema: "predictive.baseline-report/v1",
        baseline,
        observation_schema: "change.observation/v1",
        feature_profile: "structural-v1",
        sample_count: sample_metrics.len(),
        aggregate,
        by_path_count,
        by_target_group_count,
        by_binary_presence,
        by_low_value_presence,
        by_change_size,
        samples: sample_metrics,
    }
}

/// Predict from the observation only.
///
/// The signature intentionally makes record identity, target labels, and
/// provenance unavailable to the baseline implementation.
pub fn predict(observation: &Observation, baseline: BaselineKind) -> Result<Partition> {
    validate_observation(observation)?;
    match baseline {
        BaselineKind::SingleGroup => Ok(Partition {
            labels: vec![0; observation.paths.len()],
        }),
        BaselineKind::Singleton => Ok(Partition {
            labels: (0..observation.paths.len()).collect(),
        }),
        BaselineKind::ParentDirectory => parent_directory_partition(observation),
    }
}

fn parent_directory_partition(observation: &Observation) -> Result<Partition> {
    let mut group_by_key: BTreeMap<String, usize> = BTreeMap::new();
    let mut labels = Vec::with_capacity(observation.paths.len());

    for path in &observation.paths {
        let key = match path.path.encoding.as_str() {
            "utf8" => match path.path.value.rsplit_once('/') {
                Some((parent, _)) if !parent.is_empty() => format!("dir:{parent}"),
                _ => "dir:<root>".to_owned(),
            },
            "base64" => {
                // V1 preserves non-UTF-8 bytes but does not expose decoded path
                // topology as a feature. Treat an opaque path as a singleton
                // rather than learning from its encoded representation.
                format!("opaque:{}", path.id)
            }
            other => bail!("unsupported path encoding {other:?} in observation"),
        };

        let next = group_by_key.len();
        let group = *group_by_key.entry(key).or_insert(next);
        labels.push(group);
    }

    Ok(Partition { labels })
}

fn nearest_observation_index(
    observation: &Observation,
    training: &[&Observation],
) -> Result<usize> {
    validate_observation(observation)?;
    if training.is_empty() {
        bail!("nearest-neighbor training set is empty");
    }

    let mut best: Option<(u64, ObservationSignature, usize)> = None;
    for (index, candidate) in training.iter().enumerate() {
        validate_observation(candidate)?;
        let distance = observation_distance(observation, candidate);
        let signature = observation_signature(candidate);
        let current = (distance, signature, index);
        let replace = match &best {
            Some(best) => current < *best,
            None => true,
        };
        if replace {
            best = Some(current);
        }
    }
    Ok(best.expect("non-empty training set").2)
}

fn observation_distance(left: &Observation, right: &Observation) -> u64 {
    let left_signature = observation_signature(left);
    let right_signature = observation_signature(right);

    let mut distance = (left.paths.len().abs_diff(right.paths.len()) as u64) * 8;
    distance += left_signature.binary.abs_diff(right_signature.binary) as u64 * 3;
    distance += left_signature.low_value.abs_diff(right_signature.low_value) as u64 * 3;
    distance += left_signature
        .change_bucket
        .abs_diff(right_signature.change_bucket) as u64
        * 2;

    let statuses: BTreeSet<&str> = left_signature
        .status_counts
        .keys()
        .chain(right_signature.status_counts.keys())
        .map(String::as_str)
        .collect();
    for status in statuses {
        let left_count = left_signature
            .status_counts
            .get(status)
            .copied()
            .unwrap_or(0);
        let right_count = right_signature
            .status_counts
            .get(status)
            .copied()
            .unwrap_or(0);
        distance += left_count.abs_diff(right_count) as u64 * 2;
    }

    let suffixes: BTreeSet<&str> = left_signature
        .suffix_counts
        .keys()
        .chain(right_signature.suffix_counts.keys())
        .map(String::as_str)
        .collect();
    for suffix in suffixes {
        let left_count = left_signature
            .suffix_counts
            .get(suffix)
            .copied()
            .unwrap_or(0);
        let right_count = right_signature
            .suffix_counts
            .get(suffix)
            .copied()
            .unwrap_or(0);
        distance += left_count.abs_diff(right_count) as u64;
    }

    distance
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ObservationSignature {
    binary: usize,
    low_value: usize,
    change_bucket: usize,
    status_counts: BTreeMap<String, usize>,
    suffix_counts: BTreeMap<String, usize>,
}

fn observation_signature(observation: &Observation) -> ObservationSignature {
    let mut status_counts = BTreeMap::new();
    let mut suffix_counts = BTreeMap::new();
    let mut binary = 0usize;
    let mut low_value = 0usize;
    let mut change_lines = 0usize;

    for path in &observation.paths {
        *status_counts.entry(path.status.clone()).or_default() += 1;
        if let Some(suffix) = &path.suffix {
            *suffix_counts.entry(suffix.clone()).or_default() += 1;
        }
        binary += usize::from(path.binary);
        low_value += usize::from(path.low_value);
        if let (Some(additions), Some(deletions)) = (path.additions, path.deletions) {
            change_lines = change_lines.saturating_add(additions.saturating_add(deletions));
        }
    }

    ObservationSignature {
        binary,
        low_value,
        change_bucket: change_bucket_index(change_lines),
        status_counts,
        suffix_counts,
    }
}

fn transfer_partition(
    evaluation: &Observation,
    training: &Observation,
    target: &Target,
) -> Result<Partition> {
    validate_observation(evaluation)?;
    validate_observation(training)?;

    let training_by_id: BTreeMap<&str, &PathObservation> = training
        .paths
        .iter()
        .map(|path| (path.id.as_str(), path))
        .collect();

    let mut prototypes: Vec<Vec<&PathObservation>> = Vec::with_capacity(target.groups.len());
    for group in &target.groups {
        if group.paths.is_empty() {
            bail!("nearest-neighbor source target contains an empty group");
        }
        let mut paths = Vec::with_capacity(group.paths.len());
        for id in &group.paths {
            let path = training_by_id.get(id.as_str()).with_context(|| {
                format!("nearest-neighbor source target references unknown path ID {id:?}")
            })?;
            paths.push(*path);
        }
        prototypes.push(paths);
    }
    if prototypes.is_empty() {
        bail!("nearest-neighbor source target contains no groups");
    }

    let mut labels = Vec::with_capacity(evaluation.paths.len());
    for path in &evaluation.paths {
        let mut best_group = 0usize;
        let mut best_score = i32::MIN;
        for (group_index, prototype) in prototypes.iter().enumerate() {
            let score = prototype
                .iter()
                .map(|candidate| path_similarity(path, candidate))
                .max()
                .unwrap_or(i32::MIN);
            if score > best_score {
                best_score = score;
                best_group = group_index;
            }
        }
        labels.push(best_group);
    }

    Ok(Partition {
        labels: canonicalize_labels(labels),
    })
}

fn path_similarity(left: &PathObservation, right: &PathObservation) -> i32 {
    let mut score = 0i32;
    if left.suffix.is_some() && left.suffix == right.suffix {
        score += 8;
    }
    if parent_leaf(left) == parent_leaf(right) && parent_leaf(left).is_some() {
        score += 6;
    }
    if left.status == right.status {
        score += 4;
    }
    if left.binary == right.binary {
        score += 2;
    }
    if left.low_value == right.low_value {
        score += 2;
    }
    score += 3i32.saturating_sub(left.path_depth.abs_diff(right.path_depth).min(3) as i32);
    if path_change_bucket(left) == path_change_bucket(right) {
        score += 1;
    }
    score
}

fn parent_leaf(path: &PathObservation) -> Option<&str> {
    if path.path.encoding != "utf8" {
        return None;
    }
    let (parent, _) = path.path.value.rsplit_once('/')?;
    parent.rsplit('/').next().filter(|value| !value.is_empty())
}

fn path_change_bucket(path: &PathObservation) -> usize {
    match (path.additions, path.deletions) {
        (Some(additions), Some(deletions)) => {
            change_bucket_index(additions.saturating_add(deletions))
        }
        _ => 0,
    }
}

fn change_bucket_index(lines: usize) -> usize {
    match lines {
        0..=20 => 0,
        21..=100 => 1,
        101..=500 => 2,
        _ => 3,
    }
}

fn canonicalize_labels(labels: Vec<usize>) -> Vec<usize> {
    let mut remap = BTreeMap::new();
    let mut next = 0usize;
    labels
        .into_iter()
        .map(|label| {
            *remap.entry(label).or_insert_with(|| {
                let assigned = next;
                next += 1;
                assigned
            })
        })
        .collect()
}

fn validate_record(record: &DatasetRecord) -> Result<()> {
    if record.schema != "predictive.dataset-record/v1" {
        bail!("unsupported dataset record schema {:?}", record.schema);
    }
    if record.sample_id.is_empty() {
        bail!("dataset record has an empty sample_id");
    }
    validate_observation(&record.observation)?;
    if record.target.schema != "commit.structure/v1" {
        bail!("unsupported target schema {:?}", record.target.schema);
    }

    let observation_ids: BTreeSet<&str> = record
        .observation
        .paths
        .iter()
        .map(|path| path.id.as_str())
        .collect();
    if observation_ids.len() != record.observation.paths.len() {
        bail!("observation contains duplicate path IDs");
    }

    let mut target_ids = BTreeSet::new();
    let mut target_group_ids = BTreeSet::new();
    for group in &record.target.groups {
        if group.id.is_empty() || !target_group_ids.insert(group.id.as_str()) {
            bail!("target contains an empty or duplicate group ID");
        }
        if group.paths.is_empty() {
            bail!("target contains an empty group");
        }
        for id in &group.paths {
            if !observation_ids.contains(id.as_str()) {
                bail!("target references unknown observation path ID {id:?}");
            }
            if !target_ids.insert(id.as_str()) {
                bail!("target assigns observation path ID {id:?} more than once");
            }
        }
    }
    if target_ids != observation_ids {
        bail!("target does not cover every observation path ID exactly once");
    }

    Ok(())
}

fn validate_observation(observation: &Observation) -> Result<()> {
    if observation.schema != "change.observation/v1" {
        bail!("unsupported observation schema {:?}", observation.schema);
    }
    if observation.feature_profile != "structural-v1" {
        bail!(
            "unsupported predictive feature profile {:?}",
            observation.feature_profile
        );
    }
    if observation.paths.is_empty() {
        bail!("observation has no paths");
    }

    let mut ids = BTreeSet::new();
    for path in &observation.paths {
        if path.id.is_empty() || !ids.insert(path.id.as_str()) {
            bail!("observation contains an empty or duplicate path ID");
        }
        if path.path.value.is_empty() {
            bail!("observation path {:?} has an empty encoded value", path.id);
        }
        if path.path.encoding != "utf8" && path.path.encoding != "base64" {
            bail!(
                "observation path {:?} uses unsupported encoding {:?}",
                path.id,
                path.path.encoding
            );
        }
        let additions_missing = path.additions.is_none();
        let deletions_missing = path.deletions.is_none();
        if additions_missing != deletions_missing
            || path.binary != (additions_missing && deletions_missing)
        {
            bail!(
                "observation path {:?} has inconsistent binary/numstat fields",
                path.id
            );
        }
        if !matches!(
            path.status.as_str(),
            "added" | "modified" | "deleted" | "type_changed"
        ) {
            bail!(
                "observation path {:?} has unsupported status {:?}",
                path.id,
                path.status
            );
        }
        if path.path_depth == 0 {
            bail!("observation path {:?} has zero path depth", path.id);
        }
    }
    Ok(())
}

fn target_partition(record: &DatasetRecord) -> Result<Partition> {
    let index_by_id: BTreeMap<&str, usize> = record
        .observation
        .paths
        .iter()
        .enumerate()
        .map(|(index, path)| (path.id.as_str(), index))
        .collect();

    let mut labels = vec![usize::MAX; record.observation.paths.len()];
    for (group_index, group) in record.target.groups.iter().enumerate() {
        for id in &group.paths {
            let index = *index_by_id
                .get(id.as_str())
                .with_context(|| format!("target references unknown path ID {id:?}"))?;
            if labels[index] != usize::MAX {
                bail!("target assigns path ID {id:?} more than once");
            }
            labels[index] = group_index;
        }
    }
    if labels.contains(&usize::MAX) {
        bail!("target leaves at least one observation path unassigned");
    }
    Ok(Partition { labels })
}

fn score_sample(
    record: &DatasetRecord,
    target: &Partition,
    predicted: &Partition,
) -> Result<SampleMetrics> {
    if target.labels.len() != predicted.labels.len()
        || target.labels.len() != record.observation.paths.len()
    {
        bail!("partition length does not match observation path count");
    }

    let pair_counts = pair_counts(target, predicted);
    let exact_partition_match = same_partition(target, predicted);
    let adjusted_rand_index = adjusted_rand_index(target, predicted, exact_partition_match);

    let textual_change_lines = record
        .observation
        .paths
        .iter()
        .filter_map(|path| Some(path.additions?.saturating_add(path.deletions?)))
        .fold(0usize, usize::saturating_add);
    let has_binary = record.observation.paths.iter().any(|path| path.binary);
    let has_low_value = record.observation.paths.iter().any(|path| path.low_value);

    Ok(SampleMetrics {
        sample_id: record.sample_id.clone(),
        path_count: record.observation.paths.len(),
        target_group_count: target.group_count(),
        predicted_group_count: predicted.group_count(),
        group_count_abs_error: target.group_count().abs_diff(predicted.group_count()),
        pair_counts,
        pairwise_precision: pair_counts.precision(),
        pairwise_recall: pair_counts.recall(),
        pairwise_f1: pair_counts.f1(),
        adjusted_rand_index,
        exact_partition_match,
        has_binary,
        has_low_value,
        textual_change_lines,
    })
}

fn partition_counts(target: &Partition, predicted: &Partition) -> PartitionCounts {
    let mut contingency = BTreeMap::new();
    let mut target_sizes = BTreeMap::new();
    let mut predicted_sizes = BTreeMap::new();
    for (&target_label, &predicted_label) in target.labels.iter().zip(&predicted.labels) {
        *contingency
            .entry((target_label, predicted_label))
            .or_default() += 1;
        *target_sizes.entry(target_label).or_default() += 1;
        *predicted_sizes.entry(predicted_label).or_default() += 1;
    }
    PartitionCounts {
        contingency,
        target_sizes,
        predicted_sizes,
        total: target.labels.len() as u64,
    }
}

fn pair_counts(target: &Partition, predicted: &Partition) -> PairCounts {
    let counts = partition_counts(target, predicted);
    let true_positive = counts
        .contingency
        .values()
        .map(|&size| choose_two(size))
        .fold(0u64, u64::saturating_add);
    let target_same = counts
        .target_sizes
        .values()
        .map(|&size| choose_two(size))
        .fold(0u64, u64::saturating_add);
    let predicted_same = counts
        .predicted_sizes
        .values()
        .map(|&size| choose_two(size))
        .fold(0u64, u64::saturating_add);
    let false_positive = predicted_same.saturating_sub(true_positive);
    let false_negative = target_same.saturating_sub(true_positive);
    let total_pairs = choose_two(counts.total);
    let true_negative = total_pairs.saturating_sub(
        true_positive
            .saturating_add(false_positive)
            .saturating_add(false_negative),
    );

    PairCounts {
        true_positive,
        false_positive,
        false_negative,
        true_negative,
    }
}

fn same_partition(left: &Partition, right: &Partition) -> bool {
    if left.labels.len() != right.labels.len() {
        return false;
    }

    let mut left_to_right = BTreeMap::new();
    let mut right_to_left = BTreeMap::new();
    for (&left_label, &right_label) in left.labels.iter().zip(&right.labels) {
        if let Some(mapped) = left_to_right.insert(left_label, right_label) {
            if mapped != right_label {
                return false;
            }
        }
        if let Some(mapped) = right_to_left.insert(right_label, left_label) {
            if mapped != left_label {
                return false;
            }
        }
    }
    true
}

fn adjusted_rand_index(target: &Partition, predicted: &Partition, exact: bool) -> f64 {
    let counts = partition_counts(target, predicted);
    if counts.total < 2 {
        return if exact { 1.0 } else { 0.0 };
    }

    let sum_cells: f64 = counts
        .contingency
        .values()
        .map(|&size| choose_two(size) as f64)
        .sum();
    let sum_target: f64 = counts
        .target_sizes
        .values()
        .map(|&size| choose_two(size) as f64)
        .sum();
    let sum_predicted: f64 = counts
        .predicted_sizes
        .values()
        .map(|&size| choose_two(size) as f64)
        .sum();
    let total_pairs = choose_two(counts.total) as f64;
    let expected = sum_target * sum_predicted / total_pairs;
    let maximum = 0.5 * (sum_target + sum_predicted);
    let denominator = maximum - expected;
    if denominator.abs() < f64::EPSILON {
        if exact { 1.0 } else { 0.0 }
    } else {
        (sum_cells - expected) / denominator
    }
}

fn choose_two(value: u64) -> u64 {
    value.saturating_mul(value.saturating_sub(1)) / 2
}

fn aggregate_metrics(samples: Vec<&SampleMetrics>) -> AggregateMetrics {
    if samples.is_empty() {
        return AggregateMetrics {
            samples: 0,
            pair_counts: PairCounts::default(),
            pairwise_precision: None,
            pairwise_recall: None,
            pairwise_f1: None,
            mean_adjusted_rand_index: 0.0,
            exact_partition_match_rate: 0.0,
            mean_group_count_abs_error: 0.0,
        };
    }

    let mut pairs = PairCounts::default();
    let mut ari = 0.0;
    let mut exact = 0usize;
    let mut group_error = 0usize;
    for sample in &samples {
        pairs.add(sample.pair_counts);
        ari += sample.adjusted_rand_index;
        exact += usize::from(sample.exact_partition_match);
        group_error += sample.group_count_abs_error;
    }
    let count = samples.len() as f64;
    AggregateMetrics {
        samples: samples.len(),
        pair_counts: pairs,
        pairwise_precision: pairs.precision(),
        pairwise_recall: pairs.recall(),
        pairwise_f1: pairs.f1(),
        mean_adjusted_rand_index: ari / count,
        exact_partition_match_rate: exact as f64 / count,
        mean_group_count_abs_error: group_error as f64 / count,
    }
}

fn segment<F>(samples: &[SampleMetrics], key: F) -> Vec<SegmentMetrics>
where
    F: Fn(&SampleMetrics) -> &'static str,
{
    let mut groups: BTreeMap<&'static str, Vec<&SampleMetrics>> = BTreeMap::new();
    for sample in samples {
        groups.entry(key(sample)).or_default().push(sample);
    }
    groups
        .into_iter()
        .map(|(segment, samples)| SegmentMetrics {
            segment: segment.to_owned(),
            metrics: aggregate_metrics(samples),
        })
        .collect()
}

fn path_count_bucket(count: usize) -> &'static str {
    match count {
        0..=1 => "1",
        2..=4 => "2-4",
        5..=8 => "5-8",
        9..=16 => "9-16",
        _ => "17+",
    }
}

fn target_group_bucket(count: usize) -> &'static str {
    match count {
        0..=1 => "1",
        2 => "2",
        3..=4 => "3-4",
        _ => "5+",
    }
}

fn change_size_bucket(lines: usize) -> &'static str {
    match lines {
        0..=20 => "0-20-lines",
        21..=100 => "21-100-lines",
        101..=500 => "101-500-lines",
        _ => "501+-lines",
    }
}

fn ratio(numerator: u64, denominator: u64) -> Option<f64> {
    (denominator != 0).then(|| numerator as f64 / denominator as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn path(id: &str, value: &str) -> PathObservation {
        PathObservation {
            id: id.to_owned(),
            path: EncodedPath {
                encoding: "utf8".to_owned(),
                value: value.to_owned(),
            },
            status: "modified".to_owned(),
            binary: false,
            additions: Some(1),
            deletions: Some(0),
            path_depth: value.bytes().filter(|byte| *byte == b'/').count() + 1,
            suffix: value
                .rsplit_once('.')
                .map(|(_, suffix)| format!(".{suffix}")),
            low_value: false,
        }
    }

    fn record(groups: Vec<Vec<&str>>) -> DatasetRecord {
        DatasetRecord {
            schema: "predictive.dataset-record/v1".to_owned(),
            sample_id: "git-blob-sha1:test".to_owned(),
            observation: Observation {
                schema: "change.observation/v1".to_owned(),
                feature_profile: "structural-v1".to_owned(),
                paths: vec![
                    path("p0000", "src/a.rs"),
                    path("p0001", "src/b.rs"),
                    path("p0002", "docs/guide.md"),
                ],
            },
            target: Target {
                schema: "commit.structure/v1".to_owned(),
                groups: groups
                    .into_iter()
                    .enumerate()
                    .map(|(index, ids)| TargetGroup {
                        id: format!("g{index:04}"),
                        paths: ids.into_iter().map(ToOwned::to_owned).collect(),
                        source_commit_oids: vec![format!("c{index}")],
                    })
                    .collect(),
            },
            provenance: json!({
                "oid_algorithm": "sha1",
                "base_oid": "base",
                "tip_oid": "tip",
                "window_commit_oids": ["c0", "c1"],
                "extractor_version": "test"
            }),
        }
    }

    #[test]
    fn parent_directory_baseline_ignores_record_target_and_provenance() {
        let first = record(vec![vec!["p0000", "p0001"], vec!["p0002"]]);
        let mut second = record(vec![vec!["p0000"], vec!["p0001", "p0002"]]);

        second.sample_id = "different-record-id".to_owned();
        second.provenance = json!({"completely": "different"});
        second.target.groups.reverse();

        let first_prediction = predict(&first.observation, BaselineKind::ParentDirectory).unwrap();
        let second_prediction =
            predict(&second.observation, BaselineKind::ParentDirectory).unwrap();

        assert_eq!(first_prediction, second_prediction);
    }

    #[test]
    fn pairwise_metrics_distinguish_sanity_extremes() {
        let record = record(vec![vec!["p0000", "p0001"], vec!["p0002"]]);
        let target = target_partition(&record).unwrap();

        let single = predict(&record.observation, BaselineKind::SingleGroup).unwrap();
        let single_counts = pair_counts(&target, &single);
        assert_eq!(single_counts.true_positive, 1);
        assert_eq!(single_counts.false_positive, 2);
        assert_eq!(single_counts.false_negative, 0);

        let singleton = predict(&record.observation, BaselineKind::Singleton).unwrap();
        let singleton_counts = pair_counts(&target, &singleton);
        assert_eq!(singleton_counts.true_positive, 0);
        assert_eq!(singleton_counts.false_positive, 0);
        assert_eq!(singleton_counts.false_negative, 1);
    }

    #[test]
    fn parent_directory_baseline_matches_directory_partition() {
        let record = record(vec![vec!["p0000", "p0001"], vec!["p0002"]]);
        let report = evaluate(&[record], BaselineKind::ParentDirectory).unwrap();
        assert_eq!(report.aggregate.exact_partition_match_rate, 1.0);
        assert_eq!(report.aggregate.mean_adjusted_rand_index, 1.0);
        assert_eq!(report.aggregate.mean_group_count_abs_error, 0.0);
    }

    #[test]
    fn invalid_target_coverage_is_rejected() {
        let record = record(vec![vec!["p0000"], vec!["p0001"]]);
        let error = evaluate(&[record], BaselineKind::ParentDirectory).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("target does not cover every observation path ID")
        );
    }

    #[test]
    fn pair_counts_scale_without_pair_enumeration() {
        let target = Partition {
            labels: (0..20_000).map(|index| index / 2).collect(),
        };
        let predicted = target.clone();

        let counts = pair_counts(&target, &predicted);
        assert_eq!(counts.true_positive, 10_000);
        assert_eq!(counts.false_positive, 0);
        assert_eq!(counts.false_negative, 0);
        assert_eq!(counts.true_negative, 199_980_000);
    }

    #[test]
    fn exact_partition_is_label_invariant() {
        let left = Partition {
            labels: vec![0, 0, 1, 1],
        };
        let right = Partition {
            labels: vec![7, 7, 3, 3],
        };
        assert!(same_partition(&left, &right));
        assert_eq!(adjusted_rand_index(&left, &right, true), 1.0);
    }

    #[test]
    fn strict_input_rejects_feature_schema_drift() {
        let raw = r#"{
          "schema":"predictive.dataset-record/v1",
          "sample_id":"id",
          "observation":{
            "schema":"change.observation/v1",
            "feature_profile":"structural-v1",
            "sample_id":"leak",
            "paths":[]
          },
          "target":{"schema":"commit.structure/v1","groups":[]},
          "provenance":{}
        }"#;
        let error = parse_jsonl(raw).unwrap_err();
        assert!(error.to_string().contains("invalid dataset JSON"));
    }
}
