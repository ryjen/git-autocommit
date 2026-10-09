# Predictive commit-grouping baselines

Status: offline research evaluation only.

Tracking: #88 and #92. Dataset semantics are defined in
[Predictive dataset contract](predictive-dataset.md).

## Purpose

Simple baselines establish whether the historical dataset contains useful,
non-trivial commit-boundary signal before introducing a JEPA-style local latent
model. Jev is evaluated separately as an external structured-decision baseline
under #104, not as a latent architecture.

Every baseline must consume only `change.observation/v1`. Record identity,
historical provenance, and target labels are unavailable to the prediction
function by API design.

## Initial baselines

The first evaluator provides three deterministic baselines.

### `sanity-single-group/v1`

Assign every observed path to one commit.

This measures how much of the evaluation set is already explained by a trivial
"everything belongs together" policy.

### `sanity-singleton/v1`

Assign every observed path to its own commit.

This measures the opposite trivial extreme and is especially important on data
with many naturally independent paths.

### `parent-directory/v1`

Group UTF-8 paths by exact parent directory. Repository-root files are grouped
together. Non-UTF-8 paths remain opaque in `structural-v1` and are assigned
individually rather than deriving topology from their base64 representation.

This heuristic has no fitted parameters and intentionally uses only path
structure. It is not expected to be a strong product policy; it is a stable,
interpretable reference point.

Nearest-neighbour retrieval and any conventional fitted baseline remain separate
follow-up work in #92 so metric correctness and leakage boundaries can be
reviewed first.

## Metrics

For each sample, the evaluator reports:

- pairwise same-group true/false positives and false negatives;
- pairwise precision, recall, and F1;
- Adjusted Rand Index (ARI);
- exact partition match, invariant to arbitrary group labels;
- predicted and target group counts;
- absolute group-count error.

Pairwise counts and ARI are derived from target/predicted contingency counts
rather than enumerating every path pair. Exact partition equivalence uses
bidirectional label mappings. This keeps scoring approximately O(paths log
groups) instead of O(paths²), which matters for large local research corpora.

Pairwise precision or recall is emitted as JSON `null` when its denominator is
zero rather than inventing a numeric score. This matters for singleton-heavy and
single-group-heavy samples.

Aggregate pair counts are micro-aggregated across the evaluation set. ARI,
exact-match rate, and group-count error are summarized across samples.

## Segmentation

Reports include deterministic segments for:

- observed path count: 1, 2–4, 5–8, 9–16, 17+;
- target group count: 1, 2, 3–4, 5+;
- binary-file presence;
- low-value-file presence;
- textual changed lines: 0–20, 21–100, 101–500, 501+.

The intent is to make trivial sample composition visible rather than relying on
one favorable global metric.

## Input boundary

Input is JSON Lines containing `predictive.dataset-record/v1` records.

The evaluator uses a dedicated strict deserialization schema. Unknown fields
inside `change.observation/v1` are rejected. In particular, a sample ID,
historical commit count, OID, target-derived field, or provenance field cannot
silently appear in the predictor observation.

Prediction receives only:

```text
change.observation/v1
        |
        v
    baseline
        |
        v
 predicted partition
```

Scoring receives the target only after prediction:

```text
predicted partition ----+
                        +--> metrics
commit.structure/v1 ----+
```

The record-level `sample_id` is used for duplicate detection and result
attribution only. It is not available to the baseline predictor.

## Running the evaluator

Generate local records using the extractor from #91, concatenate eligible
records into a JSONL file, then run:

```sh
cargo run --example predictive_baseline -- \
  --input ./samples.jsonl \
  --baseline parent-directory \
  --pretty
```

The other initial values are:

```text
single-group
singleton
parent-directory
```

The command prints one `predictive.baseline-report/v1` JSON document. It does
not contact a model endpoint or external service.

## Reproducibility boundary

The initial evaluator intentionally does not choose train/validation/test splits
itself. Split construction belongs to the dataset-manifest work because windows
must be assigned after disjoint historical ranges are established.

For held-out claims:

- evaluate only records from the declared held-out split;
- never tune heuristic/fitted parameters on test data;
- compare baselines on identical eligible sample IDs;
- retain exclusion counts and split-manifest digest with the experiment result;
- report temporal and repository holdouts separately when both exist.

Nearest-neighbour and fitted baselines will require explicit training and
evaluation inputs with disjoint sample identities.

## Decision boundary

These reports are research evidence only. They do not influence commit
authorization, deterministic validation, human review, signing, or Git mutation.

A latent model should not proceed toward runtime consideration merely because
it beats a sanity baseline. It must outperform the strongest appropriate simple
baseline on leakage-checked held-out data at a cost appropriate for a CLI tool.
