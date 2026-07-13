# Nix installation

The repository exposes a default flake package, app, check, formatter, and development shell for Linux and macOS on x86-64 and ARM64.

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

```sh
nix develop
nix flake check
nix build
nix fmt
```

`nix build` installs the binary under `bin/` and the manual page under `share/man/man1/` in the resulting package.
