# Testing and static analysis

The repository uses a small test pyramid with explicit Cargo commands shared by local development, Nix checks, and CI.

## Quality gates

Run the same commands CI uses:

```sh
cargo format-check
cargo static-analysis
cargo test-unit
cargo test-property
cargo test-integration
cargo test-e2e
cargo supply-chain
cargo build-release
```

`cargo static-analysis` runs Clippy against all targets and all features with warnings denied. `cargo format-check` keeps rustfmt validation separate so formatting failures are reported independently from semantic lint failures.

`cargo supply-chain` runs the checked-in cargo-deny policy, including current RustSec advisory and yanked-crate checks. See [dependency and supply-chain policy](supply-chain.md) for the license/source policy and exception process.

Coverage is a separate observability job rather than a percentage gate. With `cargo-llvm-cov` installed, run `cargo coverage` for the human summary and `cargo coverage-json` to export the machine-readable summary. See [code coverage](coverage.md) for baseline scope and future regression policy.

The Cargo aliases are the canonical command contract. Nix checks invoke these aliases or their deterministic policy subset rather than reproducing target/flag definitions in `flake.nix`.

## Nix aggregate

Run the complete Nix verification surface with:

```sh
nix flake check
```

The flake exposes named checks for:

- `format` -> `cargo format-check`;
- `static-analysis` -> `cargo static-analysis`;
- `unit` -> `cargo test-unit`;
- `property` -> `cargo test-property`;
- `integration` -> `cargo test-integration`;
- `e2e` -> `cargo test-e2e`;
- `supply-chain` -> sandbox-safe `cargo deny check bans licenses sources`;
- `build-release` -> `cargo build-release`;
- `package` -> the installable Nix package, including the binary and man page.

`checks.<system>.default` is an aggregate that depends on all of these outputs. `nix flake check` evaluates and builds the named checks as separate derivations, so a failure identifies the quality-gate layer instead of collapsing the entire test pyramid into one shell script.

Coverage is intentionally not part of the Nix aggregate because cargo-llvm-cov requires LLVM tools matched to the active Rust compiler. CI installs that matching Rust component directly for the coverage job instead of coupling the general Nix verification surface to a separate LLVM toolchain.

The full supply-chain check remains a distinct CI job because live advisory/yank checking requires current registry/advisory data that normal Nix builds cannot fetch from the network. The property/adversarial layer is intentionally included in Nix as the same bounded `cargo test-property` signal used by CI. Integration and E2E tests use temporary repositories and loopback model endpoints; they do not require external services or secrets.

## Test pyramid

### Unit

```sh
cargo test-unit
```

Unit tests live with the application code and exercise deterministic logic without requiring a complete external workflow. They cover settings and argument resolution, token handling/redaction, endpoint validation, prompt/plan validation, and related invariants.

This should remain the broadest and fastest layer.

### Property/adversarial

```sh
cargo test-property
```

Property tests use `proptest` to generate arbitrary Unicode commit messages, arbitrary model response text, repair diagnostics, and excerpt budgets. They assert deterministic safety invariants: accepted messages remain within the message policy, accepted plans cannot invent/omit/duplicate staged paths, repair prompts stay within their reserved growth budget, and excerpting never exceeds its byte budget.

These tests are also part of `cargo test-unit`; CI reruns the filtered property set separately so adversarial failures have a distinct signal. Keep case counts bounded so this remains suitable for every pull request. A failing proptest case is shrunk and reported with a reproducible regression seed.

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
