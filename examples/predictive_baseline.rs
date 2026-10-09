use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use std::fs;
use std::path::PathBuf;

#[path = "../research/predictive_baseline.rs"]
mod predictive_baseline;

use predictive_baseline::{BaselineKind, evaluate, evaluate_nearest_neighbor, parse_jsonl};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Baseline {
    SingleGroup,
    Singleton,
    ParentDirectory,
    NearestNeighbor,
}

impl Baseline {
    fn direct(self) -> Option<BaselineKind> {
        match self {
            Self::SingleGroup => Some(BaselineKind::SingleGroup),
            Self::Singleton => Some(BaselineKind::Singleton),
            Self::ParentDirectory => Some(BaselineKind::ParentDirectory),
            Self::NearestNeighbor => None,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "predictive-baseline",
    about = "Evaluate commit-structure baselines over local JSONL records"
)]
struct Cli {
    /// Evaluation JSONL dataset records.
    #[arg(long)]
    input: PathBuf,

    /// Separate training JSONL records, required only for nearest-neighbor.
    #[arg(long)]
    train: Option<PathBuf>,

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
    let evaluation = parse_jsonl(&input)?;

    let report = match cli.baseline {
        Baseline::NearestNeighbor => {
            let train_path = cli
                .train
                .as_ref()
                .context("--train is required for --baseline nearest-neighbor")?;
            let train_input = fs::read_to_string(train_path)
                .with_context(|| format!("unable to read {}", train_path.display()))?;
            let training = parse_jsonl(&train_input)?;
            evaluate_nearest_neighbor(&training, &evaluation)?
        }
        baseline => {
            if cli.train.is_some() {
                bail!("--train is accepted only for --baseline nearest-neighbor");
            }
            evaluate(&evaluation, baseline.direct().expect("direct baseline"))?
        }
    };

    let rendered = if cli.pretty {
        serde_json::to_string_pretty(&report)?
    } else {
        serde_json::to_string(&report)?
    };
    println!("{rendered}");
    Ok(())
}
