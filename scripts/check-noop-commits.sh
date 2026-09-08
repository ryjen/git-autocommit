#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scripts/check-noop-commits.sh <base> <head>

Reject commits in <base>..<head> whose tree is identical to their first parent.
An intentional empty commit must include the exact trailer:

  Noop-Commit: intentional
EOF
}

if [[ $# -ne 2 ]]; then
  usage
  exit 2
fi

base=$1
head=$2

git rev-parse --verify "${base}^{commit}" >/dev/null
git rev-parse --verify "${head}^{commit}" >/dev/null

status=0
while IFS= read -r commit; do
  [[ -n "$commit" ]] || continue

  read -r -a ancestry <<<"$(git rev-list --parents -n 1 "$commit")"
  if (( ${#ancestry[@]} < 2 )); then
    continue
  fi

  parent=${ancestry[1]}
  commit_tree=$(git rev-parse "${commit}^{tree}")
  parent_tree=$(git rev-parse "${parent}^{tree}")
  [[ "$commit_tree" == "$parent_tree" ]] || continue

  if git show -s --format=%B "$commit" | grep -Fxq 'Noop-Commit: intentional'; then
    printf 'intentional no-op commit allowed: %s\n' "$commit" >&2
    continue
  fi

  printf 'no-op commit rejected: %s has the same tree as first parent %s\n' "$commit" "$parent" >&2
  printf 'If this empty commit is genuinely required, add the exact trailer: Noop-Commit: intentional\n' >&2
  status=1
done < <(git rev-list --reverse "${base}..${head}")

exit "$status"
