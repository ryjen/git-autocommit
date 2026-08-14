# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-08-13

### Added

- Interactive review of every validated commit plan before repository mutation by default.
- Explicit `commit`, `retry`, and `abort` review actions; retries regenerate against the same captured staged snapshot and pass the same deterministic validation.
- `--review` / `--no-review`, `GIT_AUTOCOMMIT_REVIEW`, and `review_before_commit` controls with review enabled by default.

### Security

- Fail closed when review is enabled without interactive input or terminal-visible plan output; unattended callers must explicitly disable review.
- Revalidate `HEAD` and the staged tree before a human-requested retry and again before committing an approved plan.
- Escape terminal control, bidirectional, and zero-width characters in staged paths shown for approval without changing the paths used for validation or commits.
- Display the same trimmed commit-message content during review that commit creation will write.

## [0.1.0] - 2026-08-02

### Added

- AI-assisted planning of atomic Conventional Commits from the staged index.
- Deterministic validation requiring every staged path exactly once, with no invented, omitted, or duplicated files.
- Signed commits by default, with explicit configuration to disable signing.
- Dry-run, single-commit, prompt inspection, resolved-configuration, and prompt-customization modes.
- Environment-only bearer authentication, including regular-file and symlinked mounted-secret support.
- Real-binary integration coverage for successful mutation, failures, stale snapshots, and final ref conflicts.

### Security

- Bound staged diff context, complete rendered prompts, model response bodies, commit counts, messages, and credentials.
- Require HTTPS for non-loopback model endpoints.
- Reject redirects, implicit proxies, embedded URL credentials, query parameters, fragments, and ambiguous endpoint paths.
- Redact credentials and remove credential variables from Git and signing subprocess environments.
- Hold Git's worktree-specific index lock through final validation and compare-and-swap ref update.
- Preserve unstaged worktree content and reject concurrent `HEAD` or staged-index changes without overwriting caller state.

### Release assets

- Native archives for Linux x86_64 and arm64, cross-built Linux armv7, macOS Intel and Apple Silicon, and Windows x86_64.
- Per-archive SHA-256 files, a consolidated `SHA256SUMS`, and GitHub build-provenance attestations.

[Unreleased]: https://github.com/ryjen/git-autocommit/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ryjen/git-autocommit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ryjen/git-autocommit/releases/tag/v0.1.0
