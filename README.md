# git-autocommit

AI-assisted Git utility that splits staged changes into atomic, signed Conventional Commits using a local OpenAI-compatible model.

## Safety model

`git-autocommit` captures `HEAD` and the staged tree before calling the model. The model may only group repository-root-relative staged paths and propose commit messages. The plan must include every staged path exactly once.

Commits are built from the captured tree through temporary Git indexes and `git commit-tree -S`. The command verifies the resulting commit chain reproduces the captured staged tree, rechecks `HEAD` and the live index, and moves `HEAD` with a compare-and-swap update. Unstaged worktree content is never committed.

Normal commit hooks are intentionally not run because hooks can mutate content after analysis. Run required checks before invoking the command or enforce them in CI.

## Install

```sh
cargo install --path .
```

Install optional prompt overrides at:

```text
~/.local/share/git-autocommit/system.md
~/.local/share/git-autocommit/plan.md
```

The binary contains built-in prompts, so external prompt files are optional.

## Usage

```sh
git add ...
git-autocommit --dry-run
git-autocommit
```

Useful options:

```sh
git-autocommit --single
git-autocommit --no-single
git-autocommit --show-prompt
git-autocommit --show-config
```

## Per-repository configuration

Create `.git/autocommit.toml`:

```toml
base_url = "http://127.0.0.1:8000/v1"
model = "dubnium-local"
timeout_seconds = 120
max_diff_bytes = 120000
max_commits = 8
single_commit = false
# prompt_dir = "/home/me/.local/share/git-autocommit"
```

Precedence is CLI, `GIT_AUTOCOMMIT_*` environment variables, `.git/autocommit.toml`, then defaults. The legacy `DUBNIUM_LOCAL_LLM_BASE_URL` and `DUBNIUM_LOCAL_LLM_MODEL` variables remain supported.
