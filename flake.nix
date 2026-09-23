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

              nativeBuildInputs = [
                pkgs.git
                pkgs.installShellFiles
              ];

              # Rootless Nix builders can set HOME to an unwritable /proc path.
              # Keep Cargo's package-cache lock in this derivation's writable temp area.
              preBuild = ''
                export CARGO_HOME="$TMPDIR/cargo-home"
                mkdir -p "$CARGO_HOME"
              '';

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
              cargo-zigbuild
              clippy
              git
              llvmPackages.llvm
              python3
              rust-analyzer
              rustc
              rustfmt
              zig
            ];

            RUST_BACKTRACE = "1";
            LLVM_COV = "${pkgs.llvmPackages.llvm}/bin/llvm-cov";
            LLVM_PROFDATA = "${pkgs.llvmPackages.llvm}/bin/llvm-profdata";

            shellHook = ''
              if [[ -t 1 ]]; then
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
              fi
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
