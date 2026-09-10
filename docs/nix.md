# Nix installation

The repository exposes a default flake package, app, package check, formatter, and development shell for Linux and macOS on x86-64 and ARM64.

For durable host or dotfiles integration, a released Git tag is the package boundary. Do not make a long-lived deployment depend on the floating repository `main` ref. The consuming flake lockfile should record the exact release revision.

The examples below use `v0.2.0`; replace it with the release you intend to deploy.

## Install a released version from GitHub

```sh
nix profile install github:ryjen/git-autocommit/v0.2.0
```

After installation, Git discovers the binary as a subcommand and the same package provides its manual page:

```sh
git autocommit --help
man git-autocommit
```

For a profile-managed installation, upgrade intentionally to a reviewed release rather than implicitly following `main`. The exact profile name is shown by `nix profile list` and may include the flake attribute name depending on the Nix version.

## Run without installing

For an exact released version:

```sh
nix run github:ryjen/git-autocommit/v0.2.0 -- --dry-run
```

Using the floating repository ref is suitable for explicit development/testing of current `main`, not durable machine configuration:

```sh
nix run github:ryjen/git-autocommit -- --dry-run
```

The first `--` separates `nix run` arguments from `git-autocommit` arguments.

## Use from another flake

Add a released tag as an input and let the consumer own the package-set revision:

```nix
{
  inputs.git-autocommit = {
    url = "github:ryjen/git-autocommit/v0.2.0";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { nixpkgs, git-autocommit, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      devShells.${system}.default = pkgs.mkShell {
        packages = [ git-autocommit.packages.${system}.default ];
      };
    };
}
```

For a NixOS or Home Manager configuration, add the same package to `environment.systemPackages`, `home.packages`, or the equivalent composition point. Do not copy the binary or source tree into the consumer repository.

The consumer's `flake.lock` is the integration and deployment record: it pins the exact Git revision and transitive inputs. Upgrading should be an explicit dependency change that updates the selected release tag and lockfile, runs the consumer's validation, and is reviewed before deployment. Rollback is the inverse operation: restore the previously reviewed tag/lockfile state.

See [Release and integration contract](release-integration.md) for the producer/consumer boundary and release verification expectations.

## Development

Enter the development shell, then use the repository-local Cargo quality gates:

```sh
nix develop
cargo format-check
cargo static-analysis
cargo test-unit
cargo test-property
cargo test-integration
cargo test-e2e
cargo supply-chain
cargo build-release
```

Cargo aliases are the canonical validation contract. CI runs those aliases directly so failures remain attributable to formatting, static analysis, each test layer, supply-chain policy, coverage, and release build behavior without asking Nix to repeat the same work.

The Nix flake has a narrower responsibility:

```sh
nix build
nix flake check
nix fmt
```

`nix build` proves the installable package can be built reproducibly from `Cargo.toml` and `Cargo.lock`, including the manual page. `nix flake check` intentionally points at that same package derivation instead of rebuilding the complete Cargo test pyramid a second time. This keeps Nix useful as a packaging/reproducibility boundary without maintaining a parallel quality-gate implementation.

The development shell still includes the tools needed by the canonical Cargo commands, including `cargo-deny`, Clippy, rustfmt, and rust-analyzer.

The package version is read from `Cargo.toml` during flake evaluation so Cargo and Nix release metadata stay aligned.
