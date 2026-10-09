use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[path = "../research/predictive_dataset.rs"]
mod predictive_dataset;

use predictive_dataset::{ExtractConfig, extract, write_record};

#[derive(Debug, Parser)]
#[command(
    name = "predictive-dataset",
    about = "Extract one local, non-content predictive commit-structure dataset record"
)]
struct Cli {
    /// Repository to inspect.
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// Commit immediately before the historical window.
    #[arg(long)]
    base: String,

    /// Final commit in the historical window.
    #[arg(long)]
    tip: String,

    /// Minimum commits allowed in the window.
    #[arg(long, default_value_t = 2)]
    min_commits: usize,

    /// Maximum commits allowed in the window.
    #[arg(long, default_value_t = 8)]
    max_commits: usize,

    /// Maximum aggregate paths allowed in one sample.
    #[arg(long, default_value_t = 1_024)]
    max_paths: usize,

    /// Maximum serialized record bytes.
    #[arg(long, default_value_t = 1_048_576)]
    max_record_bytes: usize,

    /// Write to a new file instead of stdout. Existing files are never overwritten.
    #[arg(long)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = ExtractConfig::new(cli.repo, cli.base, cli.tip);
    config.min_commits = cli.min_commits;
    config.max_commits = cli.max_commits;
    config.max_paths = cli.max_paths;
    config.max_record_bytes = cli.max_record_bytes;

    let record = extract(&config)?;
    write_record(&record, cli.output.as_deref())
}
