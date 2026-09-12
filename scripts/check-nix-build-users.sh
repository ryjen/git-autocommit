#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'error: Nix build-user contract invalid: %s\n' "$1" >&2
  exit 1
}

if [[ "$(uname -s)" != "Linux" ]]; then
  exit 0
fi

build_users_group="$(
  nix config show 2>/dev/null \
    | awk -F ' = ' '$1 == "build-users-group" { print $2; exit }'
)"

if [[ -z "${build_users_group}" ]]; then
  fail "build-users-group is empty; CI requires isolated Nix build users"
fi

group_record="$(
  awk -F: -v group="${build_users_group}" '$1 == group { print; exit }' /etc/group
)"

if [[ -z "${group_record}" ]]; then
  fail "build-users-group=${build_users_group}, but /etc/group has no matching group"
fi

group_gid="$(printf '%s\n' "${group_record}" | cut -d: -f3)"
build_user_count="$(
  awk -F: -v gid="${group_gid}" '$4 == gid { count++ } END { print count + 0 }' /etc/passwd
)"

if [[ "${build_user_count}" -eq 0 ]]; then
  supplemental_members="$(printf '%s\n' "${group_record}" | cut -d: -f4)"
  if [[ -z "${supplemental_members}" ]]; then
    fail "group ${build_users_group} exists but has no build-user members"
  fi
fi

printf 'Nix build-user contract OK: group=%s gid=%s\n' \
  "${build_users_group}" \
  "${group_gid}"
