# Release and integration contract

`git-autocommit` follows the Micrantha release/integration pattern: the project produces a versioned release, and consumers integrate that release through an explicit package boundary. Development `main` is not a deployment channel.

## Mental model

```text
source/main
    |
    | validate + version
    v
vX.Y.Z tag
    |
    +--> GitHub Release assets + checksums + provenance
    |
    +--> Nix flake at vX.Y.Z
              |
              v
       consumer flake.lock
              |
              v
        host / dotfiles
```

The producer owns source, release metadata, packaging, and release evidence. The consumer owns when that release enters a machine configuration and records the exact revision in its own lockfile.

## Producer contract

A releasable version of `git-autocommit` has one coherent identity:

- `Cargo.toml` contains the SemVer version;
- `Cargo.lock` agrees with that version;
- `CHANGELOG.md` contains the matching dated release section;
- the Git release tag is `vX.Y.Z` for that version;
- the Nix flake at that tag builds the installable package;
- the installable package contains `git-autocommit` and `git-autocommit(1)`;
- native GitHub Release archives contain the binary, README, CHANGELOG, LICENSE, and manual page;
- release artifacts have SHA-256 checksums and build-provenance attestations.

The Git tag is the immutable source identity. GitHub Release archives are useful native distribution artifacts, but they do not replace the tag as the source/Nix integration boundary.

## Consumer contract

Long-lived consumers such as Dubnium/dotfiles or another project flake should reference a released tag:

```nix
inputs.git-autocommit = {
  url = "github:ryjen/git-autocommit/v0.2.0";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

The release number above is an example; select the intended reviewed release.

The consumer should then install the exported package through its normal Nix composition layer, for example `home.packages` or `environment.systemPackages`.

### Why the consumer lockfile matters

A tag expresses the intended release. The consumer's `flake.lock` records the exact revision and dependency graph actually selected. Together they provide a reviewable integration record.

Do not:

- copy a built binary into dotfiles;
- vendor the source tree into a host configuration;
- use a floating `github:ryjen/git-autocommit` input for durable deployment;
- bypass the consumer lockfile with an ad-hoc installation during normal host rollout.

A floating `main` input remains useful for deliberate development or pre-release testing where the caller explicitly wants current source.

## Upgrade flow

A normal upgrade is a dependency change, not an in-place mutation:

1. Publish and verify the new `vX.Y.Z` release.
2. Change the consumer flake input to that release tag.
3. Update the consumer lockfile.
4. Review the tag/revision transition and relevant changelog/security impact.
5. Run the consumer's normal build and host validation.
6. Merge and deploy through the consumer's normal path.

This keeps project release state separate from machine rollout state. A project release can exist without every host immediately consuming it.

## Rollback flow

Rollback should restore a previously reviewed dependency state:

1. restore the previous release tag and lockfile entry, normally by reverting the consumer change;
2. rebuild and validate the consumer configuration;
3. deploy through the same managed path used for upgrades.

Avoid replacing a bad deployment with an unmanaged binary because that makes the live machine diverge from the declarative configuration.

## Release verification

Before adopting a release, a consumer or release reviewer should be able to establish:

- the tag corresponds to the expected source revision;
- Cargo version/lockfile/changelog metadata agree;
- the release workflow completed successfully for that source;
- expected platform artifacts exist;
- checksums are present;
- provenance attestations are present;
- the Nix package builds for the target system;
- the installed package exposes both `git autocommit` and `man git-autocommit`.

For a Nix-managed Micrantha host, building the consumer configuration from its committed lockfile is the final integration test.

## Security boundary

Pinning a release does not make upstream source trustworthy by itself. It makes the selected upstream state explicit and stable enough to review, validate, attest, and roll back.

This boundary reduces several operational failure modes:

- a new upstream commit cannot silently enter an existing deployment;
- package upgrades become visible configuration changes;
- source and host configuration remain independently reviewable;
- rollback restores a known dependency graph rather than relying on local machine history;
- the executable and its manual page remain one packaged interface.

For public-source projects such as `git-autocommit`, the public repository remains the canonical source repository. Projects that intentionally separate private implementation from a public/community distribution surface can use the same consumer contract at the public release boundary; that source-visibility policy is separate from the versioning and integration mechanics described here.
