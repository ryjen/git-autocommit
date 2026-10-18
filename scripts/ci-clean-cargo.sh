#!/usr/bin/env bash
# Native, package-scoped cleanup for *this checkout* only.
# Never follow CARGO_TARGET_DIR, remove caches, or clean release dist archives.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "$0")/.." && pwd -P)"
target="$repo_root/target"
if [[ -L "$target" ]]; then
  echo "refusing symlinked Cargo target: $target" >&2
  exit 2
fi
if [[ -e "$target" && ! -d "$target" ]]; then
  echo "refusing non-directory Cargo target: $target" >&2
  exit 2
fi
if [[ ! -d "$target" ]]; then
  echo "no worktree-local Cargo target to clean"
  exit 0
fi

# Constrain clean to the package, leaving dependencies and shared registry caches
# intact; Cargo's explicit --target-dir defeats external CARGO_TARGET_DIR values.
cargo clean --manifest-path "$repo_root/Cargo.toml" --target-dir "$target" --package git-autocommit
