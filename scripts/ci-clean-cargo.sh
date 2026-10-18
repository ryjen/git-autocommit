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
coverage_target="$target/llvm-cov-target"
coverage_reports="$target/llvm-cov"
for path in "$coverage_target" "$coverage_reports"; do
  if [[ -L "$path" ]]; then
    echo "refusing symlinked coverage output: $path" >&2
    exit 2
  fi
  if [[ -e "$path" && ! -d "$path" ]]; then
    echo "refusing non-directory coverage output: $path" >&2
    exit 2
  fi
done
if [[ ! -d "$target" ]]; then
  echo "no worktree-local Cargo target to clean"
  exit 0
fi

# Attempt each native cleanup independently, retaining its error status.
status=0
if cargo clean --manifest-path "$repo_root/Cargo.toml" --target-dir "$target" --package git-autocommit; then
  echo "ordinary package cleanup passed"
else
  status=$?
  echo "ordinary package cleanup failed: $status" >&2
fi

# cargo llvm-cov defaults to target/llvm-cov-target rather than Cargo's
# ordinary target tree. Bind *all* possible overrides to this worktree.
if [[ -d "$coverage_target" ]]; then
  if (
    cd "$repo_root"
    CARGO_TARGET_DIR="$target" CARGO_LLVM_COV_TARGET_DIR="$coverage_target" CARGO_LLVM_COV_BUILD_DIR="$coverage_target" cargo llvm-cov clean --workspace --manifest-path "$repo_root/Cargo.toml"
  ); then
    echo "coverage cleanup passed"
  else
    coverage_rc=$?
    echo "coverage cleanup failed: $coverage_rc" >&2
    if [[ "$status" -eq 0 ]]; then status="$coverage_rc"; fi
  fi
fi
exit "$status"
