use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::fs;
use std::path::PathBuf;

#[path = "../research/predictive_baseline.rs"]
mod predictive_baseline;

use predictive_baseline::{BaselineKind, evaluate, parse_jsonl};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Baseline {
    SingleGroup,
    Singleton,
    ParentDirectory,
}

impl From<Baseline> for BaselineKind {
    fn from(value: Baseline) -> Self {
        match value {
            Baseline::SingleGroup => BaselineKind::SingleGroup,
            Baseline::Singleton => BaselineKind::Singleton,
            Baseline::ParentDirectory => BaselineKind::ParentDirectory,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "predictive-baseline",
    about = "Evaluate deterministic commit-structure baselines over local JSONL records"
)]
struct Cli {
    /// JSONL dataset records produced by the predictive dataset extractor.
    #[arg(long)]
    input: PathBuf,

    /// Baseline to score.
    #[arg(long, value_enum)]
    baseline: Baseline,

    /// Pretty-print the machine-readable report.
    #[arg(long)]
    pretty: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let input = fs::read_to_string(&cli.input)
        .with_context(|| format!("unable to read {}", cli.input.display()))?;
    let records = parse_jsonl(&input)?;
    let report = evaluate(&records, cli.baseline.into())?;

    let rendered = if cli.pretty {
        serde_json::to_string_pretty(&report)?
    } else {
        serde_json::to_string(&report)?
    };
    println!("{rendered}");
    Ok(())
}
