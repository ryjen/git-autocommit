# Testing and static analysis

The repository uses a small test pyramid with explicit Cargo commands shared by local development and CI.

## Quality gates

Run the same commands CI uses:

```sh
cargo format-check
cargo static-analysis
cargo test-unit
cargo test-integration
cargo test-e2e
cargo build-release
```

`cargo static-analysis` runs Clippy against all targets and all features with warnings denied. `cargo format-check` keeps rustfmt validation separate so formatting failures are reported independently from semantic lint failures.

## Test pyramid

### Unit

```sh
cargo test-unit
```

Unit tests live with the application code and exercise deterministic logic without requiring a complete external workflow. They cover settings and argument resolution, token handling/redaction, endpoint validation, prompt/plan validation, and related invariants.

This should remain the broadest and fastest layer.

### Integration

```sh
cargo test-integration
```

Integration tests are the black-box tests under `tests/`. They exercise the compiled binary across real process, filesystem, Git, configuration, and loopback HTTP boundaries. This broad command intentionally includes every Cargo integration-test target so newly added integration tests cannot silently fall outside CI.

### End-to-end

```sh
cargo test-e2e
```

The dedicated E2E smoke gate runs `tests/commit_flow.rs`. It creates a temporary Git repository, stages changes, serves a model plan from a loopback HTTP endpoint, launches the real `git-autocommit` binary, creates commits, and verifies the resulting Git graph, staged snapshot, and unstaged worktree.

The E2E target is also included by `cargo test-integration`; CI reruns it separately to give the critical happy-path workflow its own failure signal. Keep this layer intentionally small because it is slower and spans the most moving parts.

## Adding tests

Prefer the lowest layer that can prove the behavior:

1. Add a unit test for pure/deterministic logic and validation.
2. Add an integration test when the behavior depends on a process, filesystem, Git, configuration, or HTTP boundary.
3. Add or extend E2E coverage only for critical user workflows that require the complete binary-to-Git path.

A bug fix should normally add a regression test at the lowest layer that reproduces the failure. Add a higher-level regression only when the failure depends on integration between components.
