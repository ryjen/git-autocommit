# Fuzzing validation boundaries

`git-autocommit` uses `cargo-fuzz`/libFuzzer for long-running exploration of the untrusted model-output validation boundary. Fuzzing complements the bounded `proptest` property layer; it does not replace deterministic regression tests, integration tests, or E2E coverage.

## Prerequisites

`cargo-fuzz` requires a nightly Rust toolchain and LLVM sanitizer/libFuzzer support. On a rustup-managed Linux or macOS development environment:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz --locked
```

The fuzz crate is isolated under `fuzz/` and depends on the root package's normal `git_autocommit::validation` library API. Targets do not copy parser/validator code, call external model services, invoke Git, or mutate a developer repository.

## Targets

### Raw commit plans

```sh
cargo +nightly fuzz run raw_plan -- -runs=1000 -max_len=16384
```

`raw_plan` feeds arbitrary UTF-8 model-output text through the production `validate_requested_plan` boundary. For accepted plans it independently asserts:

- the plan is non-empty and within the commit-count bound;
- staged paths form the exact expected partition with no inventions, omissions, or duplicates;
- accepted commit messages still satisfy the production Conventional Commit validator;
- repeated validation has the same accept/reject result;
- single-commit mode accepts exactly the one-entry subset of otherwise valid plans.

The target ignores inputs above the production model-response byte ceiling.

### Conventional Commit messages

```sh
cargo +nightly fuzz run commit_message -- -runs=1000 -max_len=8192
```

`commit_message` feeds arbitrary UTF-8 message text through `validate_conventional_message` and asserts deterministic results across repeated validation. The target deliberately allows input beyond the 4096-byte message limit so oversized-message rejection is explored while still keeping each fuzz case bounded.

## Corpora and crashes

Checked-in seeds live under:

```text
fuzz/corpus/raw_plan/
fuzz/corpus/commit_message/
```

They include representative valid inputs plus malformed JSON, duplicate/invented/omitted paths, fenced model output, invalid scopes, trailer-like bodies, and bidi text. Useful minimized cases discovered by fuzzing should be retained as corpus seeds when they improve future exploration.

Generated crashes and build artifacts are ignored under `fuzz/artifacts/`, `fuzz/coverage/`, and `fuzz/target/`.

When a fuzz target finds a regression:

1. minimize/reproduce the input;
2. promote the failure to the lowest appropriate deterministic unit/property/integration test;
3. fix the production validation code;
4. keep a useful minimized corpus seed when it explores a distinct state.

Do not treat a fuzz-only reproducer as the final regression test.

## Longer campaigns

For sustained local exploration, omit the run count and use a bound appropriate to the production input surface:

```sh
cargo +nightly fuzz run raw_plan -- -max_len=262144
cargo +nightly fuzz run commit_message -- -max_len=8192
```

Stop with Ctrl-C. libFuzzer persists interesting inputs in the target corpus between runs.

## CI policy

The first fuzzing slice does **not** add a required pull-request fuzz job. The repository's self-hosted JIT runner has not yet provided enough executable evidence to establish that a cargo-fuzz smoke is stable and cost-effective alongside the existing deterministic/property suite.

Once runner execution is reliable, evaluate a short bounded smoke (for example a fixed `-runs` count) as a separate signal. Long-running fuzz campaigns should remain opt-in/on-demand and must not block every pull request.
