{
  description = "Rust development environment for git-autocommit";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShellNoCC {
            packages = with pkgs; [
              cargo
              clippy
              git
              rust-analyzer
              rustc
              rustfmt
            ];

            RUST_BACKTRACE = "1";

            shellHook = ''
              echo "git-autocommit Rust development shell"
              echo "  cargo test"
              echo "  cargo clippy --all-targets --all-features -- -D warnings"
              echo "  cargo fmt --all -- --check"
            '';
          };
        }
      );

      formatter = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        pkgs.nixfmt-rfc-style
      );
    };
}
