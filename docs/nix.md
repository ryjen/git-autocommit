# Nix installation

The repository exposes a default flake package, app, package check, formatter, and development shell for Linux and macOS on x86-64 and ARM64.

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
