# Predictive commit-structure intelligence

Status: research architecture only. No predictive runtime is currently part of
`git-autocommit`.

Tracking: #88, with the trust-boundary slice in #89.

## Purpose

A JEPA/JEV-style or other latent predictor may be useful for understanding the
structure of staged changes: which paths are related, which evidence is most
useful under a fixed context budget, or whether an otherwise valid proposed
grouping looks unusual.

That information is advisory. It is not an authorization mechanism and is not
part of the repository-mutation correctness boundary.

## Authority boundary

The existing path remains authoritative:

```text
captured HEAD + staged tree
        |
        v
bounded staged evidence
        |
        v
generative commit-plan proposal
        |
        v
deterministic plan validation
        |
        v
explicit human review by default
        |
        v
temporary-index commit construction
        |
        v
final-tree verification
        |
        v
HEAD/index freshness check + expected-old-value ref update
```

Predictive research is a sidecar:

```text
captured staged snapshot
        |
        v
deterministic feature extraction
        |
        v
untrusted predictive component
        |
        +--> evidence relevance
        +--> file/group affinity
        +--> novelty/anomaly metadata
        |
        v
advisory data only
```

Removing or disabling the entire predictive subsystem must not change whether a
repository mutation is valid or authorized.

## Allowed responsibilities

A predictive component may eventually, if offline experiments justify it:

- rank already-captured staged evidence while remaining inside the existing hard
  byte limits;
- suggest file/group affinities for the generative planner;
- emit bounded novelty, confidence, or anomaly metadata for a reviewer;
- provide offline metrics and diagnostics.

All predictor output is treated as untrusted input. Any runtime-facing output
must have explicit schemas, size bounds, parsing/validation, and deterministic
fallback behavior.

## Prohibited responsibilities

A predictive component must never:

- approve, reject, or execute a commit;
- make `--no-review` implicit or suppress interactive review;
- weaken exact staged-path coverage, uniqueness, count, or message validation;
- introduce a path or content item outside the captured staged snapshot;
- execute Git, hooks, signing tools, or arbitrary commands;
- write the index, create refs, update `HEAD`, or participate in ref locking;
- alter final-tree verification or expected-old-value compare-and-swap;
- turn confidence, anomaly, novelty, or similarity into a policy decision.

A low confidence score is not a denial. A high confidence score is not an
approval.

## Failure semantics

Predictive functionality, if ever exposed at runtime, must be optional and
fail-safe:

- missing predictor: use the existing deterministic behavior;
- unavailable or timed-out predictor: use the existing deterministic behavior;
- malformed, oversized, incompatible, or non-finite predictor output: discard
  it and use the existing deterministic behavior;
- model/schema version mismatch: do not guess compatibility;
- predictor crash: it must not leave repository state partially mutated.

No predictive failure may expand evidence budgets, relax validation, or change
the captured repository snapshot.

## Privacy and training boundary

The first experiments are offline and local by default.

The initial `change.observation/v1` dataset should prefer structural features
such as paths, status, diff statistics, language/file class, directory topology,
and the existing low-value/generated classifications. Raw source and raw diff
text are not required for the first dataset and should not be persisted by
default.

Any later content-bearing feature requires an explicit threat/privacy review,
bounded representation, documented retention behavior, and opt-in where
appropriate. Dataset tooling must not collect credentials, Git remotes with
embedded credentials, environment secrets, or unrelated repository contents.

## Evaluation boundary

JEPA/JEV-style modeling is a hypothesis, not a product requirement.

The same leak-safe dataset and split should first establish simple baselines:
deterministic heuristics, nearest-neighbour retrieval, and a small conventional
model where justified. A latent model should proceed toward runtime
consideration only if it materially improves held-out commit-boundary or
evidence-selection quality at acceptable complexity and cost.

The first preferred target is commit-boundary prediction rather than commit
message generation. Useful metrics include pairwise same-commit
precision/recall/F1 and clustering metrics such as Adjusted Rand Index, with
results segmented by change size and repository/file class.

## Relationship to staged evidence

The current adaptive evidence allocator remains authoritative. Learned evidence
ranking, if later explored, may only choose what fills the existing bounded
budget; it may not raise `max_diff_bytes`, `max_prompt_bytes`, or otherwise
expand the amount of staged content sent to the configured generative endpoint.

See [Large staged changes](large-change-context.md).

## Sequencing

The research workstream is tracked by #88 through #96. Runtime-affecting work is
explicitly gated until the v0.3 release/integration work in #80 is complete and
the offline evaluation supports proceeding.
