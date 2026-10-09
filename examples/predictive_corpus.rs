use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[path = "../research/predictive_corpus.rs"]
mod predictive_corpus;
#[path = "../research/predictive_dataset.rs"]
mod predictive_dataset;

use predictive_corpus::{CorpusConfig, build_corpus};

#[derive(Debug, Parser)]
#[command(
    name = "predictive-corpus",
    about = "Build a deterministic local predictive corpus with temporal splits"
)]
struct Cli {
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// First historical commit included in the train split.
    #[arg(long)]
    history_start: String,

    /// Last commit in the train split.
    #[arg(long)]
    train_end: String,

    /// Last commit in the validation split.
    #[arg(long)]
    validation_end: String,

    /// Last commit in the test split and overall history range.
    #[arg(long)]
    history_end: String,

    /// New directory to publish atomically. It must not already exist.
    #[arg(long)]
    output_dir: PathBuf,

    #[arg(long, default_value_t = 2)]
    min_commits: usize,

    #[arg(long, default_value_t = 8)]
    max_commits: usize,

    #[arg(long, default_value_t = 1_024)]
    max_paths: usize,

    #[arg(long, default_value_t = 1_048_576)]
    max_record_bytes: usize,

    #[arg(long, default_value_t = 10_000)]
    max_history_commits: usize,

    #[arg(long, default_value_t = 268_435_456)]
    max_split_bytes: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = CorpusConfig::new(
        cli.repo,
        cli.history_start,
        cli.train_end,
        cli.validation_end,
        cli.history_end,
        cli.output_dir,
    );
    config.min_commits = cli.min_commits;
    config.max_commits = cli.max_commits;
    config.max_paths = cli.max_paths;
    config.max_record_bytes = cli.max_record_bytes;
    config.max_history_commits = cli.max_history_commits;
    config.max_split_bytes = cli.max_split_bytes;

    let manifest = build_corpus(&config)?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}
