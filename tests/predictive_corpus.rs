#[path = "../research/predictive_corpus.rs"]
mod predictive_corpus;
#[path = "../research/predictive_dataset.rs"]
mod predictive_dataset;

use predictive_corpus::{CorpusConfig, build_corpus};
use serde_json::Value;
use std::collections::BTreeSet;
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
    git(repo, &["config", "user.name", "Corpus Test"]);
    git(repo, &["config", "user.email", "corpus@example.invalid"]);
}

fn commit_file(repo: &Path, name: &str, content: &str, message: &str) -> String {
    fs::write(repo.join(name), content).unwrap();
    git(repo, &["add", name]);
    git(repo, &["commit", "--quiet", "-m", message]);
    git(repo, &["rev-parse", "HEAD"])
}

fn fixture(repo: &Path) -> Vec<String> {
    init_repo(repo);
    let mut commits = Vec::new();
    commits.push(commit_file(repo, "base.txt", "base\n", "base"));
    for index in 1..=6 {
        commits.push(commit_file(
            repo,
            &format!("file-{index}.txt"),
            &format!("{index}\n"),
            &format!("change {index}"),
        ));
    }
    commits
}

fn config(repo: &Path, commits: &[String], output: &Path) -> CorpusConfig {
    let mut config = CorpusConfig::new(
        repo.to_path_buf(),
        commits[1].clone(),
        commits[2].clone(),
        commits[4].clone(),
        commits[6].clone(),
        output.to_path_buf(),
    );
    config.min_commits = 2;
    config.max_commits = 2;
    config
}

fn read_records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn corpus_is_deterministic_and_splits_are_disjoint() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let commits = fixture(&repo);

    let first_dir = temp.path().join("corpus-one");
    let second_dir = temp.path().join("corpus-two");
    let first = build_corpus(&config(&repo, &commits, &first_dir)).unwrap();
    let second = build_corpus(&config(&repo, &commits, &second_dir)).unwrap();

    assert_eq!(first.total_samples, 3);
    assert_eq!(second.total_samples, 3);
    assert_eq!(
        fs::read(first_dir.join("manifest.json")).unwrap(),
        fs::read(second_dir.join("manifest.json")).unwrap()
    );

    let manifest: Value =
        serde_json::from_slice(&fs::read(first_dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema"], "predictive.corpus-manifest/v1");
    assert_eq!(manifest["history"]["commit_count"], 6);
    assert_eq!(manifest["splits"].as_array().unwrap().len(), 3);

    let mut all_sample_ids = BTreeSet::new();
    let mut all_window_commits = BTreeSet::new();
    for split in ["train", "validation", "test"] {
        let records = read_records(&first_dir.join(format!("{split}.jsonl")));
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert!(all_sample_ids.insert(record["sample_id"].as_str().unwrap().to_owned()));

        let window = record["provenance"]["window_commit_oids"]
            .as_array()
            .unwrap();
        assert_eq!(window.len(), 2);
        for oid in window {
            assert!(
                all_window_commits.insert(oid.as_str().unwrap().to_owned()),
                "historical commit appeared in more than one split"
            );
        }
    }

    assert_eq!(all_sample_ids.len(), 3);
    assert_eq!(all_window_commits.len(), 6);
}

#[test]
fn corpus_refuses_to_overwrite_existing_output_directory() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let commits = fixture(&repo);

    let output = temp.path().join("corpus");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("sentinel"), "keep").unwrap();

    let error = build_corpus(&config(&repo, &commits, &output)).unwrap_err();
    assert!(error.to_string().contains("never overwrites"));
    assert_eq!(fs::read_to_string(output.join("sentinel")).unwrap(), "keep");
    assert!(!output.join("manifest.json").exists());
}

#[test]
fn split_manifest_accounts_for_every_candidate_window() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let commits = fixture(&repo);

    let output = temp.path().join("corpus");
    let manifest = build_corpus(&config(&repo, &commits, &output)).unwrap();

    for split in &manifest.splits {
        let excluded: usize = split.exclusions.values().sum();
        assert_eq!(split.candidate_windows, split.sample_count + excluded);
        assert_eq!(split.candidate_windows, 1);
        assert_eq!(split.sample_count, 1);
    }
}

#[test]
fn replacement_refs_do_not_change_corpus_history() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let commits = fixture(&repo);

    let first_dir = temp.path().join("corpus-before-replace");
    build_corpus(&config(&repo, &commits, &first_dir)).unwrap();

    git(&repo, &["replace", &commits[3], &commits[1]]);

    let second_dir = temp.path().join("corpus-after-replace");
    build_corpus(&config(&repo, &commits, &second_dir)).unwrap();

    assert_eq!(
        fs::read(first_dir.join("manifest.json")).unwrap(),
        fs::read(second_dir.join("manifest.json")).unwrap()
    );
    for split in ["train", "validation", "test"] {
        assert_eq!(
            fs::read(first_dir.join(format!("{split}.jsonl"))).unwrap(),
            fs::read(second_dir.join(format!("{split}.jsonl"))).unwrap()
        );
    }
}

#[test]
fn history_commit_limit_is_enforced() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let commits = fixture(&repo);

    let output = temp.path().join("corpus");
    let mut cfg = config(&repo, &commits, &output);
    cfg.max_history_commits = 5;

    let error = build_corpus(&cfg).unwrap_err();
    assert!(error.to_string().contains("max_history_commits"));
    assert!(!output.exists());
}

#[test]
fn split_byte_limit_is_enforced_before_publication() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let commits = fixture(&repo);

    let output = temp.path().join("corpus");
    let mut cfg = config(&repo, &commits, &output);
    cfg.max_split_bytes = 1;

    let error = build_corpus(&cfg).unwrap_err();
    assert!(error.to_string().contains("max_split_bytes"));
    assert!(!output.exists());
}

#[test]
fn dash_prefixed_split_boundaries_are_resolved_as_revisions() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let commits = fixture(&repo);

    for (name, oid) in [
        ("-start", &commits[1]),
        ("-train", &commits[2]),
        ("-validation", &commits[4]),
        ("-end", &commits[6]),
    ] {
        git(&repo, &["update-ref", &format!("refs/heads/{name}"), oid]);
    }

    let output = temp.path().join("corpus");
    let mut cfg = CorpusConfig::new(
        repo.clone(),
        "-start".to_owned(),
        "-train".to_owned(),
        "-validation".to_owned(),
        "-end".to_owned(),
        output.clone(),
    );
    cfg.min_commits = 2;
    cfg.max_commits = 2;

    let manifest = build_corpus(&cfg).unwrap();
    assert_eq!(manifest.history.start_oid, commits[1]);
    assert_eq!(manifest.history.train_end_oid, commits[2]);
    assert_eq!(manifest.history.validation_end_oid, commits[4]);
    assert_eq!(manifest.history.end_oid, commits[6]);
    assert!(output.join("manifest.json").is_file());
}

#[test]
fn bounded_history_probe_preserves_requested_start() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    let mut commits = Vec::new();
    commits.push(commit_file(&repo, "base.txt", "base\n", "base"));
    for index in 1..=9 {
        commits.push(commit_file(
            &repo,
            &format!("file-{index}.txt"),
            &format!("{index}\n"),
            &format!("change {index}"),
        ));
    }

    let output = temp.path().join("corpus");
    let mut cfg = CorpusConfig::new(
        repo.clone(),
        commits[1].clone(),
        commits[3].clone(),
        commits[6].clone(),
        commits[9].clone(),
        output.clone(),
    );
    cfg.min_commits = 2;
    cfg.max_commits = 2;
    cfg.max_history_commits = 8;

    let error = build_corpus(&cfg).unwrap_err();
    assert!(error.to_string().contains("max_history_commits"));
    assert!(!output.exists());
}
