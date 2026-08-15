# Code coverage

`git-autocommit` uses `cargo-llvm-cov` for source-based Rust coverage measurement. Coverage is an observability signal, not a substitute for behavioral assertions, property tests, fuzzing, integration tests, or E2E validation.

## Local measurement

Install a Rust toolchain with LLVM tools plus `cargo-llvm-cov`, then run:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
cargo coverage
cargo coverage-json
```

`cargo coverage` runs the normal workspace test suite with all features and prints the human-readable LLVM coverage summary. `cargo coverage-json` reuses the collected profiles and writes a machine-readable summary to:

```text
target/coverage-summary.json
```

The repository already ignores `/target`, so coverage profiles and reports do not pollute source control.

## What the baseline includes

The coverage run intentionally uses the default workspace test selection rather than a separate coverage-only test list. In this repository that includes:

- binary/unit tests;
- the bounded `proptest` property/adversarial tests compiled into the binary test target;
- all integration-test targets under `tests/`, including the commit-flow E2E smoke.

Property tests contribute coverage only for the generated cases executed during that run. A higher property-test case count can exercise additional branches, but it should not be interpreted as proportionally stronger semantic coverage. Keep property generation bounded for normal CI and use fuzzing for long-running state-space exploration.

The E2E test remains included because it operates entirely on temporary repositories and a loopback model server. Coverage instrumentation changes compilation and runtime overhead but does not change the intended Git/model boundary being exercised.

## CI reporting

The `Coverage baseline` CI job:

1. installs the matching Rust `llvm-tools-preview` component;
2. installs `cargo-llvm-cov` through the upstream install action pinned to a reviewed commit;
3. runs `cargo coverage`, leaving the line/function/region summary in the job log;
4. exports `target/coverage-summary.json`;
5. uploads that JSON as the `coverage-summary` workflow artifact.

No coverage data is sent to a third-party coverage service, and no external service or repository secret is required.

## Baseline and future enforcement

The first successful CI execution after coverage instrumentation is merged is the initial recorded baseline. Record its line/function/region totals on issue #30 before considering the measurement rollout complete.

This first slice intentionally defines **no percentage threshold**. Once enough history exists to distinguish normal variation from regressions, prefer a policy such as:

- no material decrease from the established baseline; and/or
- meaningful coverage for new or changed deterministic code.

Do not optimize for 100% coverage or add superficial tests solely to move a percentage. Missing coverage should guide review toward untested behavior and risky boundaries, not become a vanity metric.
