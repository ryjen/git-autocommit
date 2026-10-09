# Predictive dataset contract

Status: research data contract only. No dataset extractor or predictive runtime
is currently part of `git-autocommit`.

Tracking: #88 and #90. The authority and failure boundary is defined in
[Predictive commit-structure intelligence](predictive-intelligence.md).

## Goal

Define a deterministic, leak-conscious representation for offline experiments
that predict file-level commit structure from a staged-like change.

The first contract is deliberately non-content-bearing. It describes structural
properties of a change without persisting source lines, diff text, commit
messages, environment state, credentials, or remote URLs.

Two versioned schemas are used:

- `change.observation/v1`: information available to a predictor;
- `commit.structure/v1`: historical target labels and audit metadata.

Targets are never valid authorization input to `git-autocommit`.

## Sample unit: a staged-like historical window

A single historical commit is not a useful commit-grouping example because all
of its changed paths have the same label. A grouping sample therefore starts
from a contiguous first-parent window of historical commits.

For a window:

```text
base ---- c1 ---- c2 ---- ... ---- cn = tip
   \_______________________________/
             one sample
```

the observation is derived from the aggregate `base..tip` change using the same
no-rename file-level semantics as the current tool. The historical commits are
used only to construct the target partition.

The extraction configuration records the allowed window-size range. Grouping
experiments should normally use windows containing at least two commits.
Single-commit windows may be retained only as an explicitly identified
sanity/calibration variant.

### File-level feasibility

`git-autocommit` assigns each staged path to exactly one generated commit and
does not split a file across multiple generated commits. Historical windows can
violate that constraint when the same path is touched by several commits.

To produce a feasible target:

1. create one node for each historical commit in the window;
2. connect two commit nodes when they touch the same observation path;
3. take connected components of that overlap graph;
4. coalesce each component into one target group;
5. order groups by the earliest constituent commit in the window.

This preserves historical separation where the file-level interface can express
it and coalesces only boundaries that the current product cannot represent.

### Eligibility

A v1 grouping sample is eligible only when all of the following hold:

- the window follows one linear first-parent chain;
- no commit in the window is a merge commit;
- the aggregate `base..tip` change is non-empty;
- every historical path used to construct a target is represented by the
  aggregate observation under the same no-rename semantics;
- after overlap coalescing, every observed path belongs to exactly one target
  group;
- configured path, group, and record-size bounds are satisfied.

The fourth rule excludes transient or net-zero changes such as a file created
and deleted inside the window or a path modified and fully reverted before
`tip`. Keeping such labels would teach the model about information that is not
present in a staged snapshot.

Generated files, vendored files, lockfiles, binaries, deletes, and type changes
are not excluded merely because of their class. They remain useful structural
evidence and can be segmented during evaluation.

## Path representation

Git paths are byte strings and are not guaranteed to be valid UTF-8. A path is
therefore represented explicitly:

```json
{
  "encoding": "utf8",
  "value": "src/app.rs"
}
```

or, when the raw Git path is not valid UTF-8:

```json
{
  "encoding": "base64",
  "value": "<base64 of raw path bytes>"
}
```

The encoded value must round-trip to the exact repository-relative path bytes.
No Unicode normalization, case folding, path cleaning, or platform separator
conversion is permitted.

Each unique path in a sample receives a deterministic local identifier such as
`p0000`, assigned by unsigned bytewise ordering of the decoded path bytes.
Target groups refer to these identifiers instead of repeating path content.

V1 does not define a pseudonymized public/export representation. Ad-hoc hashing
is prohibited because it can destroy path-topology semantics while creating a
false impression of anonymity. Any export-safe representation requires a
separate documented feature profile and privacy review.

## `change.observation/v1`

The observation contains only prediction-time data.

Illustrative shape:

```json
{
  "schema": "change.observation/v1",
  "feature_profile": "structural-v1",
  "paths": [
    {
      "id": "p0000",
      "path": {
        "encoding": "utf8",
        "value": "src/app.rs"
      },
      "status": "modified",
      "binary": false,
      "additions": 18,
      "deletions": 7,
      "path_depth": 2,
      "suffix": ".rs",
      "low_value": false
    }
  ]
}
```

The exact serialized schema implemented by #91 must be locked by fixtures before
dataset generation begins. The example above defines semantics, not permission
to add unreviewed fields.

Historical OIDs, extractor version, and window size are **provenance**, not
predictor features. An offline dataset record may package them beside the
observation and target, but training/evaluation feature projection must exclude
that provenance. In particular, the historical window commit count would leak
information about the intended grouping if it were part of the observation.

### Core feature semantics

V1 may contain:

- exact repository-relative path bytes through the encoding above;
- aggregate status using no-rename semantics: added, modified, deleted, or
  type-changed;
- binary/text classification;
- aggregate additions/deletions when meaningful for textual changes;
- deterministic hunk-count/diff-shape statistics that do not retain line text;
- path depth and suffix derived from exact path bytes where representable;
- the existing low-value/generated/vendor/lockfile classification when computed
  from a pinned extractor profile.

V1 must not contain:

- source contents or diff line text;
- commit messages, subjects, authors, emails, signatures, or trailers;
- author/committer timestamps as predictive features;
- branch names;
- Git remote URLs;
- environment variables or Git credential/config values;
- model responses, prompts, review decisions, or generated commit messages;
- information derived from commits after the sample tip.

Dependency or semantic-code graphs are intentionally not part of
`structural-v1`. Add them only through a separately versioned feature profile
so baseline comparisons remain meaningful.

### Stable sample identity

`sample_id` must be deterministic and domain-separated from other hashes. V1
uses the repository's own Git object hash implementation rather than introducing
a separate hashing dependency: the extractor serializes a canonical,
domain-separated preimage and passes it to `git hash-object --stdin`. The ID is
rendered as `git-blob-<object-format>:<digest>` (for example,
`git-blob-sha1:<40-hex>`). It is computed from immutable sample-defining inputs,
at minimum:

- observation schema version;
- feature-profile version;
- Git object-ID algorithm;
- base object ID;
- ordered window commit object IDs;
- extractor configuration affecting eligibility or feature meaning.

The exact preimage encoding and object format must be recorded and covered by
golden tests in #91. JSON formatting itself must not affect sample identity.
Because the ID is for dataset identity rather than authorization or integrity,
its security properties must not be relied upon. Repositories using different
Git object formats may produce different sample IDs for otherwise equivalent
windows.

## Dataset-record provenance

A local offline record carries the deterministic `sample_id` at the outer
record level and may carry a separate non-feature provenance envelope containing:

- Git object-ID algorithm;
- base and tip commit OIDs;
- ordered historical window commit OIDs;
- extractor version/source revision;
- extractor eligibility/bounds configuration.

The record-level `sample_id` and this envelope exist for reproducibility and
audit only. Neither is part of `change.observation/v1`, and models/baselines must
not consume either as a feature.

## `commit.structure/v1`

The target is stored separately from the observation contract even if an
offline dataset record packages them together.

Illustrative shape:

```json
{
  "schema": "commit.structure/v1",
  "groups": [
    {
      "id": "g0000",
      "paths": ["p0000", "p0002"],
      "source_commit_oids": ["<c1>", "<c2>"]
    },
    {
      "id": "g0001",
      "paths": ["p0001"],
      "source_commit_oids": ["<c3>"]
    }
  ]
}
```

Rules:

- every observation path ID appears exactly once across `groups`;
- no unknown path ID is allowed;
- groups are non-empty;
- group order follows the earliest constituent historical commit;
- source commit IDs are audit metadata, not predictor inputs;
- commit messages are not stored in v1;
- target metadata carries no approval, trust, or policy meaning.

## Rename and copy handling

Runtime grouping currently uses no-rename semantics. V1 mirrors that behavior.

A historical rename therefore appears structurally as deletion of the old path
and addition of the new path. The extractor must not use rename/copy inference
to provide a training-only identity signal that the runtime observation lacks.

If a later commit in the same window modifies the new path, normal path-overlap
coalescing applies.

## Splitting and leakage control

Dataset splitting happens before overlapping training windows are generated.

At minimum:

1. establish an ordered first-parent commit sequence for each repository;
2. assign disjoint commit ranges to train, validation, and test according to the
   experiment manifest;
3. generate windows wholly inside one assigned range;
4. reject any window that crosses a split boundary;
5. deduplicate globally by `sample_id`.

This guarantees that one historical commit cannot appear in more than one
split, including through overlapping windows.

The exact proportions are experiment policy rather than schema policy and must
be recorded in a version-controlled split manifest. Baseline and latent-model
comparisons must use the same manifest.

For generalization claims, evaluation should additionally include a
repository-holdout set when enough repositories are available. Temporal and
repository holdouts answer different questions and should be reported
separately.

## Repository identity and privacy

V1 dataset records do not store remote URLs.

A multi-repository dataset manifest may assign a deterministic repository
fingerprint based only on local Git object identity using a documented,
domain-separated construction. It must not silently derive identity from a
hosting-service URL, working-directory path, username, or machine name.

Datasets remain local by default. Sharing or publishing a dataset is a separate
privacy decision even when raw source text is absent: path names and Git object
IDs can still reveal repository structure or correlate with public history.

## Determinism and serialization

The extractor must make repeated extraction of the same Git revision,
configuration, and extractor version produce canonically equivalent records.

Required deterministic ordering:

- commits: first-parent order;
- paths: unsigned bytewise path order;
- target groups: earliest constituent commit order;
- path IDs and group IDs: assigned after the corresponding canonical ordering.

The extractor should emit UTF-8 JSON Lines with stable field ordering and no
non-deterministic timestamps. Tests should compare parsed semantic records and,
where the implementation promises canonical bytes, exact fixture bytes.

## Research extractor prototype

Issue #91 provides a research-only extractor without adding a second shipped
binary or changing the `git-autocommit` runtime. Run it as a Cargo example:

```sh
cargo run --example predictive_dataset -- \
  --repo . \
  --base <commit-before-window> \
  --tip <last-commit-in-window>
```

By default it emits one compact JSON record to stdout. To write a file:

```sh
cargo run --example predictive_dataset -- \
  --repo . \
  --base <base> \
  --tip <tip> \
  --output ./sample.jsonl
```

The `--output` path must not already exist; the extractor never overwrites an
existing file. Default bounds are 2–8 historical commits, 1024 aggregate paths,
and 1 MiB serialized record size. Those bounds are recorded in provenance and
also participate in sample identity because they affect eligibility.

The extractor invokes only read-only Git operations plus `git hash-object
--stdin` without `-w`; those Git operations do not update refs, the index,
Git configuration, or the object database.

Historical revision names are resolved with Git's option-terminated revision
parser (`rev-parse --verify --end-of-options`) so valid dash-prefixed ref names
cannot be reinterpreted as command options.

Historical diff features are isolated from the current checkout and ambient Git
configuration. The extractor requires Git 2.41+ global `--attr-source`
support, reads version-controlled attributes from the sample `tip`, pins the
diff algorithm and large-file threshold, disables replacement-object
interpretation, external/text conversion, and submodule-ignore variation,
clears inherited `GIT_*` controls, suppresses
global/system configuration and attributes, and overrides
`core.attributesFile` with the platform null device.

`$GIT_DIR/info/attributes` has higher precedence than in-tree attributes and
cannot be disabled through the normal attribute-source option. The v1 extractor
therefore fails closed whenever that repository-local file is present. It also
rejects historical `diff=<driver>` attribute values because custom diff-driver
configuration would otherwise make binary/text classification depend on local
Git config. Historical `diff` set/unset states, including the in-tree
`binary` macro, remain supported. The default stdout mode creates no
files. An explicit `--output` intentionally creates one new file and may make
the worktree dirty if the caller places it inside the repository; existing
files are never overwritten. Model credentials and external-diff environment
hooks are removed from child Git processes.

## Dataset manifest

A generated dataset must include a manifest recording at least:

- observation and target schema versions;
- feature profile;
- extractor version / source revision;
- Git version where behavior could affect extraction;
- window-size policy;
- path/group/record-size limits;
- low-value classification profile;
- eligibility/exclusion counters by reason;
- split-policy version and split-manifest digest;
- sample counts per split;
- whether any non-v1 content-bearing extension was enabled.

The manifest contains configuration and aggregate counts only; it must not
silently embed source or diff content.

## Schema evolution

Fields must not silently change meaning.

Compatible additive metadata that is explicitly non-feature-bearing may be
introduced only when old readers can safely ignore it. Any change to predictor
inputs, eligibility, path identity, target construction, or feature semantics
requires a new feature profile or schema version.

A reader encountering an unknown schema or feature profile must reject it
rather than guess compatibility.

## Security boundary

These schemas are research/evaluation artifacts. They are not executable plans,
capabilities, approvals, or mutation envelopes.

In particular, `commit.structure/v1` must never be accepted as an input that
causes `git-autocommit` to create commits. The project's existing decision not
to support external plan ingestion remains unchanged.
