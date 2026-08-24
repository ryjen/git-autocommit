# Code coverage

`git-autocommit` uses `cargo-llvm-cov` for source-based Rust coverage measurement. Coverage is an observability signal, not a substitute for behavioral assertions, property tests, fuzzing, integration tests, or E2E validation.

## Local measurement

Use the repository Nix development shell so Rust, `cargo-llvm-cov`, `llvm-cov`, and `llvm-profdata` stay on the same toolchain boundary:

```sh
nix --extra-experimental-features "nix-command flakes" develop --command cargo coverage
nix --extra-experimental-features "nix-command flakes" develop --command cargo coverage-json
```

`cargo coverage` runs the normal workspace test suite with all features and prints the human-readable LLVM coverage summary. `cargo coverage-json` reuses the collected profiles and writes a machine-readable summary to:

```text
target/coverage-summary.json
```

The repository ignores `/target`, so coverage profiles and reports do not pollute source control.

## What the baseline includes

The coverage run intentionally uses the default workspace test selection rather than a separate coverage-only test list. In this repository that includes:

- binary/unit tests;
- the bounded `proptest` property/adversarial tests compiled into the binary test target;
- all integration-test targets under `tests/`, including the commit-flow E2E smoke.

Property tests contribute coverage only for the generated cases executed during that run. A higher property-test case count can exercise additional branches, but it should not be interpreted as proportionally stronger semantic coverage. Keep property generation bounded for normal CI and use fuzzing for long-running state-space exploration.

The E2E test remains included because it operates entirely on temporary repositories and a loopback model server. Coverage instrumentation changes compilation and runtime overhead but does not change the intended Git/model boundary being exercised.

On the hardened Linux JIT runner, the coverage job sets `TMPDIR` to `${RUNNER_TEMP}/exec-tmp` before running the suite. This keeps the integration-test Git fault-injection wrapper on executable storage without weakening the runner's `noexec` `/tmp` hardening.

## CI reporting

The `Coverage baseline` CI job:

1. enters the Nix development shell, which provides the matching Rust and LLVM coverage tools;
2. runs `cargo coverage`, leaving the line/function/region summary in the job log;
3. exports `target/coverage-summary.json` with `cargo coverage-json`;
4. uploads that JSON as the `coverage-summary` workflow artifact.

No coverage data is sent to a third-party coverage service, and no external service or repository secret is required.

## Initial baseline

The first successful hardened-JIT coverage run after the coverage tooling and executable-temp fixes was CI run `32690866026` on 2026-08-24. The uploaded `coverage-summary` artifact recorded:

| Metric | Covered | Total | Baseline |
| --- | ---: | ---: | ---: |
| Lines | 1,828 | 1,987 | 91.998% |
| Functions | 188 | 215 | 87.442% |
| Regions | 2,892 | 3,213 | 90.009% |

These values are an observed baseline, not minimum targets.

## Regression policy

The initial rollout intentionally defines **no arbitrary percentage threshold**. Coverage should be evaluated as a regression and review signal:

- investigate material decreases from the recorded baseline, especially when they affect security, Git-state, model-boundary, or failure-atomicity behavior;
- new or changed deterministic code should be meaningfully exercised by the most appropriate test layer;
- small percentage movement caused by refactoring or generated-code shape is not by itself a failure;
- property tests, fuzzing, integration tests, and E2E tests remain complementary signals and must not be weakened merely to preserve a coverage number.

If later history shows that a stable mechanical guard is useful, prefer a conservative no-material-regression or changed-code policy over a globally invented percentage floor.

Do not optimize for 100% coverage or add superficial tests solely to move a percentage. Missing coverage should guide review toward untested behavior and risky boundaries, not become a vanity metric.
