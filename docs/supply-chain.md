# Dependency and supply-chain policy

`git-autocommit` uses `cargo-deny` to enforce a checked-in dependency policy against the committed Cargo dependency graph.

## Full policy

Run the same full policy used by CI from the Nix development shell:

```sh
nix develop
cargo supply-chain
```

`cargo supply-chain` expands to `cargo deny check` and evaluates `deny.toml` across all features.

The policy currently covers:

- RustSec vulnerabilities and security notices;
- yanked crate versions, which fail the check;
- unmaintained direct/workspace dependencies;
- unsound advisories across the dependency graph;
- an explicit SPDX license allowlist;
- wildcard dependency requirements;
- duplicate crate versions, reported as warnings for review rather than blanket failures;
- registry and Git sources, with unknown sources denied.

Only the crates.io registry is currently allowed. Git dependencies are denied unless an explicit source exception is added to `deny.toml`.

## Nix aggregate

Normal Nix derivations are network-sandboxed, while the advisory and yank checks require current RustSec and registry-index data. `nix flake check` therefore runs the deterministic subset:

```sh
cargo deny check bans licenses sources
```

as the named `supply-chain` Nix check. The distinct CI `Dependency and supply-chain policy` job runs the full `cargo supply-chain` command outside the Nix build sandbox so advisory and yank data can be refreshed.

This split keeps the Nix aggregate deterministic without turning a stale vendored advisory snapshot into the security source of truth.

## Exceptions

Do not add an exception merely to make CI green.

When an exception is required:

1. identify the exact crate/version, advisory, license, or source;
2. add the narrowest supported exception to `deny.toml`;
3. include a concrete `reason` explaining why the condition is currently acceptable;
4. prefer a version constraint over a crate-wide exception where supported;
5. remove the exception as soon as the dependency graph no longer requires it.

Advisory and yanked-crate ignores should be treated as temporary risk acceptances. A pull request adding one should state the remediation condition and, when useful, link the tracking issue.

## Duplicate versions

`multiple-versions = "warn"` is intentional for the initial policy. Duplicate transitive versions can be legitimate and are not automatically a security defect. The warning keeps them visible for dependency cleanup and review without encouraging unsafe lockfile manipulation or unnecessary direct dependency changes.

If a specific duplicate becomes security-, size-, or maintenance-relevant, address it directly or promote that condition to a narrower deny rule.

## Adding dependencies

Before adding or upgrading a dependency:

```sh
cargo update --dry-run
cargo supply-chain
```

Review new licenses, sources, advisories, and duplicate-version warnings as part of the dependency change. Supply-chain policy complements source review, tests, property checks, fuzzing, and static analysis; it does not replace them.
