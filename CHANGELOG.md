# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/ryjen/git-autocommit/releases/tag/v0.1.0
