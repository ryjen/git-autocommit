# Predictive corpus and temporal split manifest

Status: offline research data preparation only.

Tracking: #88 and #102. The record-level schema is defined in
[Predictive dataset contract](predictive-dataset.md).

## Purpose

The one-window extractor is useful for validating record semantics, but
held-out evaluation needs a corpus whose train, validation, and test membership
is fixed before any baseline or model is tuned.

The corpus builder assigns explicit contiguous first-parent history ranges to
named temporal splits, then generates eligible sliding windows entirely inside
each split.

## Explicit temporal boundaries

The v1 interface requires four revisions:

- `history-start`: first commit included in training;
- `train-end`: last training commit;
- `validation-end`: last validation commit;
- `history-end`: last test commit and end of the corpus history range.

They are parsed as revisions only (using `rev-parse --verify --end-of-options`),
so valid dash-prefixed ref names cannot be interpreted as Git command options.
They must resolve onto one contiguous first-parent history and satisfy:

```text
history-start <= train-end < validation-end < history-end
```

Each split must contain at least `min_commits` commits.

The commit immediately before `history-start` is used only as the base tree for
windows beginning at `history-start`. It is not a member of any split or
prediction target.

## Window generation

For each split, the builder enumerates every contiguous window whose size is in
`min_commits..=max_commits` and whose commits remain entirely inside that
split.

For a split:

```text
predecessor -- c1 -- c2 -- c3 -- c4
                |-----|
                |----------|
                      |-----|
```

a window uses the commit immediately before its first member as the staged-like
base state and its last member as the tip passed to the #91 extractor.

The existing `change.observation/v1` and `commit.structure/v1` semantics remain
authoritative. The corpus builder does not reimplement grouping labels.

### Corpus resource bounds

V1 also bounds corpus-level work before publication:

- selected first-parent history: at most 10,000 commits by default;
- each serialized split JSONL: at most 256 MiB by default.

History sizing first uses a bounded `git rev-list --count` probe capped at
`max_history_commits + 1`. Oversized ranges fail before enumeration. Only a
range proven within the bound is then enumerated in oldest-to-newest order,
preserving the requested `history-start` boundary.

These limits are configurable but must remain positive and are recorded in the
manifest. They bound candidate enumeration and peak in-memory split
serialization independently of the per-record `max_record_bytes` limit.

## Output layout

A successful corpus is published as a new directory:

```text
corpus/
  manifest.json
  train.jsonl
  validation.jsonl
  test.jsonl
```

Each JSONL file contains complete `predictive.dataset-record/v1` records.

The manifest is `predictive.corpus-manifest/v1` and records:

- an object-format-aware local selected-history fingerprint;
- split-policy version and local Git version;
- exact resolved history and split boundary OIDs;
- total selected history commit count;
- observation/target schema, feature profile, low-value profile, and extractor version;
- explicit no-rename and no-content-bearing-feature flags;
- extraction/window/path/record bounds;
- explicit selected-history and per-split serialized-byte limits;
- per-split first/last OIDs and commit count;
- candidate window count;
- accepted sample count;
- bounded exclusion counts by reason;
- Git blob ID of each JSONL file;
- total sample count.

The manifest intentionally contains no remote URL, user/machine identity, commit
message, author identity, source text, or diff text.

## Publication semantics

The target output directory must not already exist.

All split files and `manifest.json` are first written beneath a temporary
directory in the requested output directory's parent.

Publication then uses a fail-safe no-clobber protocol:

1. create the requested output directory with `create_dir`, which atomically
   fails if another path already claimed that name;
2. hard-link each completed split file from staging into the new output
   directory, so an existing destination file is never replaced;
3. hard-link `manifest.json` **last** as the completion marker.

The staging and output directories share a parent/filesystem so hard links do
not cross a filesystem boundary.

A successful corpus therefore has a simple validity boundary: a valid
`manifest.json` is present only after every declared split file has been
published. An interrupted or failed build may leave a claimed but incomplete
output directory, but it does not publish a valid completion manifest and never
overwrites an existing output path. Such an incomplete directory must be
inspected/removed explicitly before retrying.

## Split leakage invariants

The builder enforces:

- commit assignment occurs before window generation;
- train, validation, and test commit lists are disjoint;
- generated windows never cross a split boundary;
- accepted sample IDs are globally unique across splits;
- each split's candidate count equals accepted samples plus deterministic
  exclusions;
- target labels do not influence split boundaries.

The split files themselves remain explicit inputs to later evaluation. A
retrieval/fitted evaluator must still reject training/evaluation sample overlap
rather than trusting filenames alone.

## Exclusion accounting

Every candidate window is either emitted or assigned one bounded exclusion
category. V1 categories include:

- `merge_or_parentless_commit`;
- `transient_or_net_zero_path`;
- `empty_aggregate_change`;
- `path_limit`;
- `record_size_limit`;
- `empty_historical_commit`;
- `unsupported_diff_status`;


Unexpected extractor or Git failures are **not** converted into exclusion counts. They abort corpus construction, leaving the requested output directory unpublished. Duplicate sample IDs likewise abort instead of being silently deduplicated.

## Git history isolation

Corpus split construction uses the same deterministic-history boundary as the
record extractor. Its Git child processes clear inherited `GIT_*` controls,
suppress global/system configuration, disable optional locks, and pass
`--no-replace-objects`.

This matters because repository-local replacement refs can otherwise rewrite
commit traversal without changing the requested boundary OIDs. A corpus manifest
must describe the repository's real object history, not a caller-specific
replacement view.

The extractor remains responsible for the stronger diff/attribute isolation
needed to build each individual record.

## Corpus file integrity

Each split's `blob_id` is produced by the local Git implementation via
`git hash-object --stdin` without `-w` over the exact JSONL bytes. The ID is
rendered with the repository object format, for example:

```text
git-blob-sha1:<digest>
```

These IDs support deterministic local comparison; they are not authorization or
security tokens.

The selected-history fingerprint is likewise domain-separated and based only on local Git object identity plus the selected history endpoints. It avoids remote URLs or working-directory/user identity; it identifies this selected local history, not a hosting account or clone path.

## Running the builder

Example:

```sh
cargo run --example predictive_corpus -- \
  --repo . \
  --history-start <first-train-commit> \
  --train-end <last-train-commit> \
  --validation-end <last-validation-commit> \
  --history-end <last-test-commit> \
  --output-dir ./predictive-corpus
```

Optional bounds:

```text
--min-commits       default 2
--max-commits       default 8
--max-paths         default 1024
--max-record-bytes    default 1048576
--max-history-commits default 10000
--max-split-bytes     default 268435456
```

The command prints the same manifest structure written to
`<output-dir>/manifest.json` after successful publication.

## Repository holdout

V1 defines one repository-local temporal corpus. Cross-repository generalization
remains a separate evaluation dimension.

A future multi-repository experiment should compose already-generated local
manifests and keep repository membership disjoint where a repository holdout is
claimed. The corpus builder must not discover or clone repositories
automatically.

## Decision boundary

Corpus generation is research data preparation. The output is not a commit
plan, approval, capability, mutation envelope, or runtime input.

No corpus, split assignment, baseline score, or future latent representation may
change the deterministic authorization and Git-mutation boundary documented in
[Predictive commit-structure intelligence](predictive-intelligence.md).
