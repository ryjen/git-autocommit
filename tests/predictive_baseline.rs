#[path = "../research/predictive_baseline.rs"]
mod predictive_baseline;

use predictive_baseline::{BaselineKind, evaluate, parse_jsonl};
use serde_json::Value;

const DATASET: &str = r#"{"schema":"predictive.dataset-record/v1","sample_id":"git-blob-sha1:one","observation":{"schema":"change.observation/v1","feature_profile":"structural-v1","paths":[{"id":"p0000","path":{"encoding":"utf8","value":"src/a.rs"},"status":"modified","binary":false,"additions":3,"deletions":1,"path_depth":2,"suffix":".rs","low_value":false},{"id":"p0001","path":{"encoding":"utf8","value":"src/b.rs"},"status":"modified","binary":false,"additions":2,"deletions":0,"path_depth":2,"suffix":".rs","low_value":false},{"id":"p0002","path":{"encoding":"utf8","value":"docs/guide.md"},"status":"modified","binary":false,"additions":5,"deletions":2,"path_depth":2,"suffix":".md","low_value":false}]},"target":{"schema":"commit.structure/v1","groups":[{"id":"g0000","paths":["p0000","p0001"],"source_commit_oids":["c1"]},{"id":"g0001","paths":["p0002"],"source_commit_oids":["c2"]}]},"provenance":{"oid_algorithm":"sha1","base_oid":"base","tip_oid":"tip","window_commit_oids":["c1","c2"],"extractor_version":"test","extraction":{"feature_profile":"structural-v1"}}}
{"schema":"predictive.dataset-record/v1","sample_id":"git-blob-sha1:two","observation":{"schema":"change.observation/v1","feature_profile":"structural-v1","paths":[{"id":"p0000","path":{"encoding":"utf8","value":"Cargo.toml"},"status":"modified","binary":false,"additions":1,"deletions":1,"path_depth":1,"suffix":".toml","low_value":false},{"id":"p0001","path":{"encoding":"utf8","value":"Cargo.lock"},"status":"modified","binary":false,"additions":20,"deletions":10,"path_depth":1,"suffix":".lock","low_value":true}]},"target":{"schema":"commit.structure/v1","groups":[{"id":"g0000","paths":["p0000","p0001"],"source_commit_oids":["c3"]}]},"provenance":{"oid_algorithm":"sha1","base_oid":"base2","tip_oid":"tip2","window_commit_oids":["c3","c4"],"extractor_version":"test","extraction":{"feature_profile":"structural-v1"}}}"#;

#[test]
fn parent_directory_report_is_machine_readable_and_segmented() {
    let records = parse_jsonl(DATASET).unwrap();
    let report = evaluate(&records, BaselineKind::ParentDirectory).unwrap();

    assert_eq!(report.sample_count, 2);
    assert_eq!(report.baseline, "parent-directory/v1");
    assert_eq!(report.aggregate.samples, 2);
    assert!(!report.by_path_count.is_empty());
    assert!(!report.by_target_group_count.is_empty());
    assert!(!report.by_low_value_presence.is_empty());

    let value: Value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["schema"], "predictive.baseline-report/v1");
    assert_eq!(value["observation_schema"], "change.observation/v1");
    assert_eq!(value["feature_profile"], "structural-v1");
}

#[test]
fn duplicate_sample_ids_fail_closed() {
    let records = parse_jsonl(DATASET).unwrap();
    let duplicate = vec![records[0].clone(), records[0].clone()];
    let error = evaluate(&duplicate, BaselineKind::SingleGroup).unwrap_err();
    assert!(error.to_string().contains("duplicate dataset sample_id"));
}

#[test]
fn sanity_baselines_are_both_reported_without_inventing_defined_precision() {
    let records = parse_jsonl(DATASET).unwrap();

    let single = evaluate(&records, BaselineKind::SingleGroup).unwrap();
    assert_eq!(single.baseline, "sanity-single-group/v1");
    assert!(single.aggregate.pairwise_precision.is_some());

    let singleton = evaluate(&records, BaselineKind::Singleton).unwrap();
    assert_eq!(singleton.baseline, "sanity-singleton/v1");
    assert_eq!(singleton.aggregate.pairwise_precision, None);
    assert_eq!(singleton.aggregate.pairwise_recall, Some(0.0));
}

#[test]
fn target_or_provenance_fields_inside_observation_are_rejected() {
    let leaked = DATASET.replacen(
        "\"feature_profile\":\"structural-v1\",",
        "\"feature_profile\":\"structural-v1\",\"sample_id\":\"forbidden\",",
        1,
    );
    let error = parse_jsonl(&leaked).unwrap_err();
    assert!(error.to_string().contains("invalid dataset JSON"));
}

#[test]
fn partial_numstat_fields_fail_closed() {
    let malformed = DATASET.replacen(
        "\"additions\":3,\"deletions\":1",
        "\"additions\":3,\"deletions\":null",
        1,
    );
    let records = parse_jsonl(&malformed).unwrap();
    let error = evaluate(&records, BaselineKind::SingleGroup).unwrap_err();
    assert!(error.to_string().contains("inconsistent binary/numstat fields"));
}

#[test]
fn textual_change_line_totals_saturate() {
    let large = DATASET.replacen(
        "\"additions\":3,\"deletions\":1",
        &format!("\"additions\":{},\"deletions\":1", usize::MAX),
        1,
    );
    let records = parse_jsonl(&large).unwrap();
    let report = evaluate(&records, BaselineKind::SingleGroup).unwrap();
    assert_eq!(report.samples[0].textual_change_lines, usize::MAX);
}
