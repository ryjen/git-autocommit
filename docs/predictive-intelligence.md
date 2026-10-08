# Predictive commit-structure intelligence

Status: research architecture only. No predictive runtime is currently part of
`git-autocommit`.

Tracking: #88, with the trust-boundary slice in #89.

## Purpose

Two distinct research directions may improve understanding of staged changes:

- a **JEPA-style local latent representation** may learn which paths are related,
  which evidence is useful under a fixed context budget, or whether a proposed
  grouping is structurally unusual;
- **TypeSafe Jev**, a separate hosted structured-decision model, may be evaluated
  for narrow typed judgments such as pairwise same-group probability or
  candidate-group cohesion.

Jev is not a JEPA architecture and is not part of the local latent-model
training experiment. Both tracks are advisory. Neither is an authorization
mechanism or part of the repository-mutation correctness boundary.

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

Predictive/advisory research is a sidecar:

```text
captured staged snapshot
        |
        v
deterministic feature extraction
        |
        +--> local JEPA-style research
        |       +--> evidence relevance
        |       +--> file/group affinity
        |       +--> novelty/anomaly metadata
        |
        +--> optional Jev evaluation
                +--> pairwise/group decisions
                +--> cohesion/anomaly scores
        |
        v
advisory data only
```

Removing or disabling the entire predictive subsystem must not change whether a
repository mutation is valid or authorized.

## Allowed responsibilities

A JEPA-derived local component may eventually, if offline experiments justify it:

- rank already-captured staged evidence while remaining inside the existing hard
  byte limits;
- suggest file/group affinities for the generative planner;
- emit bounded novelty, confidence, or anomaly metadata for a reviewer.

A Jev experiment may only provide bounded typed advisory judgments over an
explicitly approved state representation. It cannot replace commit-message
generation and must not become a required network dependency.

Both tracks may provide research metrics and diagnostics.

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

Predictive/advisory functionality, if ever exposed at runtime, must be optional
and fail-safe:

- missing local predictor: use the existing deterministic behavior;
- unavailable or timed-out Jev service: treat advisory judgment as unavailable
  and use the existing deterministic behavior;
- malformed, oversized, incompatible, or non-finite output: discard it and use
  the existing deterministic behavior;
- model/schema version mismatch: do not guess compatibility;
- predictor/client crash: it must not leave repository state partially mutated.

No predictive failure may expand evidence budgets, relax validation, or change
the captured repository snapshot.

## Privacy and training boundary

JEPA-style representation experiments are offline and local by default. Jev is
a separate external-service experiment and therefore requires explicit opt-in.

The initial `change.observation/v1` dataset should prefer structural features
such as paths, status, diff statistics, language/file class, directory topology,
and the existing low-value/generated classifications. Raw source and raw diff
text are not required for the first dataset and should not be persisted by
default.

The first Jev experiment must use the same non-content structural observation
surface; raw source and raw diff text are not sent externally by default.

Any later content-bearing feature or external transmission requires an explicit
threat/privacy review, bounded representation, documented retention behavior,
and opt-in. Dataset tooling must not collect credentials, Git remotes with
embedded credentials, environment secrets, or unrelated repository contents.

## Evaluation boundary

JEPA-style latent modeling and Jev advisory evaluation are separate hypotheses,
not product requirements.

The same leak-safe corpus and split should first establish simple local
baselines: deterministic heuristics, nearest-neighbour retrieval, and a small
conventional model where justified. A JEPA-style latent model should proceed
toward runtime consideration only if it materially improves held-out
commit-boundary or evidence-selection quality at acceptable complexity and cost.

Jev should be evaluated as an external structured-decision baseline on the same
held-out samples, with probability calibration, latency, cost, privacy, and
service/version drift reported separately. A useful Jev result does not imply
that the default CLI should depend on the service.

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

The research workstream is tracked by #88, #90-#96, #102, and #104. JEPA
research is local/offline-first; Jev is a separate opt-in external-service track
under #104. Runtime-affecting work is explicitly gated until the v0.3
release/integration work in #80 is complete and evaluation supports proceeding.
