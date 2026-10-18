#!/usr/bin/env bash
# Test only within a newly created, owned sandbox; never run real Cargo clean.
set -euo pipefail
source_root="$(cd -- "$(dirname -- "$0")/.." && pwd -P)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/git-autocommit-clean-XXXXXXXX")"
trap 'rm -rf -- "$tmp"' EXIT
repo="$tmp/repo"
mkdir -p "$repo/scripts" "$tmp/bin" "$tmp/shared"
cp "$source_root/scripts/ci-clean-cargo.sh" "$repo/scripts/"
printf '[package]\nname = "git-autocommit"\nversion = "0.3.1"\n' > "$repo/Cargo.toml"

cat > "$tmp/bin/cargo" <<'CARGO_MOCK'
#!/usr/bin/env bash
set -euo pipefail
printf '\n--invoke--\n' >> "$CARGO_ARGS_LOG"
printf '%s\n' "$@" >> "$CARGO_ARGS_LOG"
printf 'target=%s coverage=%s build=%s\n' "$CARGO_TARGET_DIR" "${CARGO_LLVM_COV_TARGET_DIR:-}" "${CARGO_LLVM_COV_BUILD_DIR:-}" >> "$CARGO_ENV_LOG"
exit "${MOCK_CARGO_EXIT:-0}"
CARGO_MOCK
chmod +x "$tmp/bin/cargo"
export PATH="$tmp/bin:$PATH"
export CARGO_ARGS_LOG="$tmp/args"
export CARGO_ENV_LOG="$tmp/env"
export CARGO_LLVM_COV_TARGET_DIR="$tmp/shared"
export CARGO_LLVM_COV_BUILD_DIR="$tmp/shared"
export CARGO_TARGET_DIR="$tmp/shared"

# Absent outputs are a successful no-op without consulting Cargo.
bash "$repo/scripts/ci-clean-cargo.sh"
test ! -e "$tmp/args"

# Worktree-owned target is selected explicitly rather than CARGO_TARGET_DIR.
mkdir -p "$repo/target"
printf 'keep shared' > "$tmp/shared/unrelated"
bash "$repo/scripts/ci-clean-cargo.sh"
grep -Fx -- 'clean' "$tmp/args"
grep -Fx -- '--package' "$tmp/args"
grep -Fx -- 'git-autocommit' "$tmp/args"
grep -Fx -- '--manifest-path' "$tmp/args"
grep -Fx -- "$repo/Cargo.toml" "$tmp/args"
grep -Fx -- '--target-dir' "$tmp/args"
grep -Fx -- "$repo/target" "$tmp/args"
test "$(cat "$tmp/shared/unrelated")" = "keep shared"

# A separate instrumented target must also be cleaned after coverage export.
mkdir -p "$repo/target/llvm-cov-target"
: > "$tmp/args"
: > "$tmp/env"
bash "$repo/scripts/ci-clean-cargo.sh"
grep -Fx -- 'llvm-cov' "$tmp/args"
grep -Fx -- '--workspace' "$tmp/args"
grep -Fx -- "target=$repo/target coverage=$repo/target/llvm-cov-target build=$repo/target/llvm-cov-target" "$tmp/env"
test "$(cat "$tmp/shared/unrelated")" = "keep shared"

# Symlinked target is rejected before any native command is invoked.
rm "$tmp/args"
rmdir "$repo/target/llvm-cov-target"
rmdir "$repo/target"
ln -s "$tmp/shared" "$repo/target"
if bash "$repo/scripts/ci-clean-cargo.sh"; then
  echo "FAIL: symlink target was permitted" >&2
  exit 1
fi
test ! -e "$tmp/args"
test -L "$repo/target"
test "$(cat "$tmp/shared/unrelated")" = "keep shared"
rm "$repo/target"

# Unexpected non-directory is likewise rejected.
printf 'not a directory' > "$repo/target"
if bash "$repo/scripts/ci-clean-cargo.sh"; then
  echo "FAIL: non-directory target was permitted" >&2
  exit 1
fi
test ! -e "$tmp/args"
rm "$repo/target"
mkdir -p "$repo/target"

# Coverage symlink is rejected before any Cargo invocation.
ln -s "$tmp/shared" "$repo/target/llvm-cov-target"
if bash "$repo/scripts/ci-clean-cargo.sh"; then
  echo "FAIL: symlinked coverage target permitted" >&2
  exit 1
fi
test ! -e "$tmp/args"
test "$(cat "$tmp/shared/unrelated")" = "keep shared"
rm "$repo/target/llvm-cov-target"
mkdir -p "$repo/target/llvm-cov-target"

# Propagate native Cargo failure; do not hide it with a successful cleanup.
export MOCK_CARGO_EXIT=17
set +e
bash "$repo/scripts/ci-clean-cargo.sh"
rc=$?
set -e
test "$rc" -eq 17
grep -Fx -- 'llvm-cov' "$tmp/args"
test "$(cat "$tmp/shared/unrelated")" = "keep shared"
echo "native package-scoped Cargo cleanup contract: PASS"
