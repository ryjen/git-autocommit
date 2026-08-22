{
  description = "AI-assisted Git utility for atomic Conventional Commits";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      packageVersion = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
      commonRustArgs = {
        version = packageVersion;
        src = self;
        cargoLock.lockFile = ./Cargo.lock;
      };
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.rustPlatform.buildRustPackage (
            commonRustArgs
            // {
              pname = "git-autocommit";

              nativeBuildInputs = [ pkgs.git pkgs.installShellFiles ];

              postInstall = ''
                installManPage man/git-autocommit.1
              '';

              meta = {
                description = "AI-assisted Git utility for atomic Conventional Commits";
                homepage = "https://github.com/ryjen/git-autocommit";
                license = pkgs.lib.licenses.asl20;
                mainProgram = "git-autocommit";
              };
            }
          );
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/git-autocommit";
        };
      });

      checks = forAllSystems (system: {
        default = self.packages.${system}.default;
        package = self.packages.${system}.default;
      });

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              cargo-deny
              cargo-llvm-cov
              clippy
              git
              rust-analyzer
              rustc
              rustfmt
            ];

            RUST_BACKTRACE = "1";

            shellHook = ''
              echo "git-autocommit Rust development shell"
              echo "  cargo format-check"
              echo "  cargo static-analysis"
              echo "  cargo test-unit"
              echo "  cargo test-property"
              echo "  cargo test-integration"
              echo "  cargo test-e2e"
              echo "  cargo coverage"
              echo "  cargo supply-chain"
              echo "  cargo build-release"
              echo "  nix build"
              echo "  nix fmt"
            '';
          };
        }
      );

      formatter = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        pkgs.nixfmt
      );
    };
}
