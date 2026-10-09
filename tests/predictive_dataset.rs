#[path = "../research/predictive_dataset.rs"]
mod predictive_dataset;

use predictive_dataset::{ExtractConfig, extract, write_record};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git should execute");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git metadata should be UTF-8")
        .trim()
        .to_owned()
}

fn init_repo(repo: &Path) {
    git(repo, &["init", "--quiet"]);
    git(repo, &["config", "user.name", "Dataset Test"]);
    git(repo, &["config", "user.email", "dataset@example.invalid"]);
}

fn commit_all(repo: &Path, message: &str) -> String {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "--quiet", "-m", message]);
    git(repo, &["rev-parse", "HEAD"])
}

fn config(repo: &Path, base: &str, tip: &str) -> ExtractConfig {
    ExtractConfig::new(repo.to_path_buf(), base.to_owned(), tip.to_owned())
}

#[test]
fn historical_window_coalesces_overlapping_file_commits() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    init_repo(repo);

    fs::write(repo.join("a.txt"), "a0\n").unwrap();
    fs::write(repo.join("b.txt"), "b0\n").unwrap();
    let base = commit_all(repo, "initial state");

    fs::write(repo.join("a.txt"), "a1\n").unwrap();
    let c1 = commit_all(repo, "first a change");

    fs::write(repo.join("b.txt"), "b1\n").unwrap();
    let c2 = commit_all(repo, "b change");

    fs::write(repo.join("a.txt"), "a2\n").unwrap();
    let c3 = commit_all(repo, "second a change");

    let cfg = config(repo, &base, &c3);
    let first = extract(&cfg).unwrap();
    let second = extract(&cfg).unwrap();

    assert_eq!(first.sample_id, second.sample_id);
    let value = serde_json::to_value(&first).unwrap();
    assert_eq!(value["observation"]["paths"].as_array().unwrap().len(), 2);

    let groups = value["target"]["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["paths"], serde_json::json!(["p0000"]));
    assert_eq!(groups[0]["source_commit_oids"], serde_json::json!([c1, c3]));
    assert_eq!(groups[1]["paths"], serde_json::json!(["p0001"]));
    assert_eq!(groups[1]["source_commit_oids"], serde_json::json!([c2]));

    let encoded = serde_json::to_string(&first).unwrap();
    assert!(!encoded.contains("first a change"));
    assert!(!encoded.contains("dataset@example.invalid"));
    assert!(!encoded.contains("content-marker-not-for-dataset-output"));

    let output = repo.join("record.jsonl");
    write_record(&first, Some(&output)).unwrap();
    let error = write_record(&first, Some(&output)).unwrap_err();
    assert!(error.to_string().contains("never overwritten"));

    let line = fs::read_to_string(output).unwrap();
    assert!(line.ends_with('\n'));
    let parsed: Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(parsed["schema"], "predictive.dataset-record/v1");
    assert!(
        parsed["sample_id"]
            .as_str()
            .unwrap()
            .starts_with("git-blob-")
    );
    assert!(parsed["observation"].get("sample_id").is_none());
    assert!(parsed["target"].get("sample_id").is_none());
}

#[test]
fn transient_historical_paths_are_rejected() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    init_repo(repo);

    fs::write(repo.join("keep.txt"), "base\n").unwrap();
    let base = commit_all(repo, "initial state");

    fs::write(repo.join("tmp.txt"), "temporary\n").unwrap();
    let _c1 = commit_all(repo, "add temporary file");

    fs::remove_file(repo.join("tmp.txt")).unwrap();
    fs::write(repo.join("keep.txt"), "changed\n").unwrap();
    let tip = commit_all(repo, "remove temporary file");

    let error = extract(&config(repo, &base, &tip)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("historical path tmp.txt is absent from aggregate")
    );
}

#[test]
fn rename_is_observed_as_delete_plus_add_without_rename_inference() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    init_repo(repo);

    fs::write(repo.join("old.txt"), "one\n").unwrap();
    let base = commit_all(repo, "initial state");

    git(repo, &["mv", "old.txt", "new.txt"]);
    let _rename = commit_all(repo, "rename file");

    fs::write(repo.join("new.txt"), "two\n").unwrap();
    let tip = commit_all(repo, "modify renamed file");

    let record = extract(&config(repo, &base, &tip)).unwrap();
    let value = serde_json::to_value(&record).unwrap();
    let paths = value["observation"]["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 2);

    let statuses: Vec<&str> = paths
        .iter()
        .map(|path| path["status"].as_str().unwrap())
        .collect();
    assert!(statuses.contains(&"added"));
    assert!(statuses.contains(&"deleted"));

    let groups = value["target"]["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["paths"].as_array().unwrap().len(), 2);
}

#[test]
fn future_worktree_attributes_do_not_change_historical_observation() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    init_repo(repo);

    fs::write(repo.join("sample.dat"), "zero\n").unwrap();
    let base = commit_all(repo, "initial state");

    fs::write(repo.join("sample.dat"), "one\n").unwrap();
    let _c1 = commit_all(repo, "first change");

    fs::write(repo.join("sample.dat"), "two\n").unwrap();
    let tip = commit_all(repo, "second change");

    let before = serde_json::to_value(extract(&config(repo, &base, &tip)).unwrap()).unwrap();

    fs::write(repo.join(".gitattributes"), "*.dat binary\n").unwrap();
    let _future = commit_all(repo, "future binary classification");

    let after = serde_json::to_value(extract(&config(repo, &base, &tip)).unwrap()).unwrap();
    assert_eq!(before, after);

    let path = &after["observation"]["paths"][0];
    assert_eq!(path["binary"], false);
    assert!(path["additions"].is_number());
    assert!(path["deletions"].is_number());
}

#[test]
fn repository_info_attributes_fail_closed() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    init_repo(repo);

    fs::write(repo.join("sample.txt"), "zero\n").unwrap();
    let base = commit_all(repo, "initial state");
    fs::write(repo.join("sample.txt"), "one\n").unwrap();
    let _c1 = commit_all(repo, "first change");
    fs::write(repo.join("sample.txt"), "two\n").unwrap();
    let tip = commit_all(repo, "second change");

    fs::write(repo.join(".git/info/attributes"), "*.txt binary\n").unwrap();
    let error = extract(&config(repo, &base, &tip)).unwrap_err();
    assert!(error.to_string().contains("info/attributes"));
}

#[test]
fn custom_historical_diff_drivers_fail_closed() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    init_repo(repo);

    fs::write(repo.join(".gitattributes"), "*.txt diff=custom\n").unwrap();
    fs::write(repo.join("sample.txt"), "zero\n").unwrap();
    let base = commit_all(repo, "initial state");
    fs::write(repo.join("sample.txt"), "one\n").unwrap();
    let _c1 = commit_all(repo, "first change");
    fs::write(repo.join("sample.txt"), "two\n").unwrap();
    let tip = commit_all(repo, "second change");

    let error = extract(&config(repo, &base, &tip)).unwrap_err();
    assert!(error.to_string().contains("config-dependent diff driver"));
}

#[test]
fn repository_core_attributes_file_is_ignored() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    fs::write(repo.join("sample.txt"), "zero\n").unwrap();
    let base = commit_all(&repo, "initial state");
    fs::write(repo.join("sample.txt"), "one\n").unwrap();
    let _c1 = commit_all(&repo, "first change");
    fs::write(repo.join("sample.txt"), "two\n").unwrap();
    let tip = commit_all(&repo, "second change");

    let external = temp.path().join("attributes");
    fs::write(&external, "*.txt binary\n").unwrap();
    git(
        &repo,
        &["config", "core.attributesFile", external.to_str().unwrap()],
    );

    let record = extract(&config(&repo, &base, &tip)).unwrap();
    let value = serde_json::to_value(record).unwrap();
    let path = &value["observation"]["paths"][0];
    assert_eq!(path["binary"], false);
    assert!(path["additions"].is_number());
    assert!(path["deletions"].is_number());
}

#[test]
fn dash_prefixed_revision_names_are_resolved_as_revisions() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    init_repo(repo);

    fs::write(repo.join("sample.txt"), "zero\n").unwrap();
    let base = commit_all(repo, "base");
    fs::write(repo.join("sample.txt"), "one\n").unwrap();
    let _first = commit_all(repo, "first");
    fs::write(repo.join("sample.txt"), "two\n").unwrap();
    let tip = commit_all(repo, "tip");

    git(repo, &["update-ref", "refs/heads/-base", &base]);
    git(repo, &["update-ref", "refs/heads/-tip", &tip]);

    let record = extract(&config(repo, "-base", "-tip")).unwrap();
    let value = serde_json::to_value(record).unwrap();
    assert_eq!(value["provenance"]["base_oid"], base);
    assert_eq!(value["provenance"]["tip_oid"], tip);
}
