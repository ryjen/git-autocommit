# Nix installation

The repository exposes a default flake package, app, aggregate check, formatter, and development shell for Linux and macOS on x86-64 and ARM64.

## Install from GitHub

```sh
nix profile install github:ryjen/git-autocommit
```

After installation, Git discovers the binary as a subcommand:

```sh
git autocommit --help
man git-autocommit
```

Update the installed profile entry with:

```sh
nix profile upgrade git-autocommit
```

The exact profile name is shown by `nix profile list` and may include the flake attribute name depending on the Nix version.

## Run without installing

```sh
nix run github:ryjen/git-autocommit -- --dry-run
```

The first `--` separates `nix run` arguments from `git-autocommit` arguments.

## Use from another flake

Add the repository as an input:

```nix
{
  inputs.git-autocommit.url = "github:ryjen/git-autocommit";

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

For a NixOS or Home Manager module, add the same package to the relevant `environment.systemPackages` or `home.packages` list.

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

The development shell includes `cargo-deny`. Cargo aliases are the canonical command contract used by local development, Nix, and CI, so the flake does not maintain a separate set of lint/test flags. See [testing and static analysis](testing.md) and [dependency and supply-chain policy](supply-chain.md).

The Nix flake also supports:

```sh
nix flake check
nix build
nix fmt
```

`nix flake check` builds named derivations for formatting, Clippy/static analysis, unit tests, property/adversarial tests, integration tests, E2E tests, the sandbox-safe supply-chain subset, the release-build Cargo alias, and the installable package. The default check is an aggregate that depends on all of those outputs. This keeps failures attributable to a specific layer while still providing one complete verification command.

To build an individual check directly, select its system-specific attribute, for example:

```sh
nix build .#checks.x86_64-linux.property
nix build .#checks.x86_64-linux.static-analysis
nix build .#checks.x86_64-linux.supply-chain
```

The Nix `supply-chain` check evaluates bans, licenses, and sources only. Live RustSec advisory and yanked-crate checks require current network-fetched metadata, so the full `cargo supply-chain` policy runs outside the Nix build sandbox as a distinct CI job. This avoids vendoring a stale advisory snapshot merely to make the derivation network-free.

The property check deliberately runs the same bounded `cargo test-property` contract used by CI. Integration and E2E checks use only temporary repositories and loopback HTTP endpoints, so no model service, network credential, or signing key is required.

`nix build` installs the binary under `bin/` and the manual page under `share/man/man1/` in the resulting package. The package version is read from `Cargo.toml` during flake evaluation so Cargo and Nix release metadata stay aligned.
