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

### `nearest-neighbor-transfer/v1`

Nearest-neighbour retrieval uses a separate training JSONL input and rejects
both training/evaluation `sample_id` overlap and historical
`provenance.window_commit_oids` overlap before scoring. Commit OIDs are read
only by the evaluator as a leakage guard; they are never passed to neighbour
selection or partition transfer.

Neighbour distance is intentionally small and interpretable. It compares only
structural observation summaries:

- path-count difference;
- binary and low-value path counts;
- total textual-change bucket;
- status histogram;
- suffix histogram.

Equal-distance candidates are ordered by the same canonical structural
signature and then by deterministic training-input order. Exact path strings,
path IDs, sample IDs, target labels, and provenance do not participate in
neighbour selection.

After selecting a training observation, its target grouping is transferred to
the evaluation observation by assigning each evaluation path to the most similar
source-group prototype. Per-path similarity uses declared observation features:
suffix, UTF-8 parent-directory leaf, status, binary/low-value flags, path depth,
and change-size bucket. The transferred labels are canonicalized before scoring.

This baseline is deliberately simple; it is evidence about how far local
structural retrieval can go before a fitted or latent model is justified.

Any conventional fitted baseline remains separate follow-up work in #92.

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

The supported values are:

```text
single-group
singleton
parent-directory
nearest-neighbor
```

Nearest-neighbour additionally requires an explicit disjoint training file:

```sh
cargo run --example predictive_baseline -- \
  --input ./validation.jsonl \
  --train ./train.jsonl \
  --baseline nearest-neighbor \
  --pretty
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
