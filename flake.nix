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
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "git-autocommit";
            version = "0.0.2";
            src = self;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [ pkgs.installShellFiles ];

            postInstall = ''
              installManPage man/git-autocommit.1
            '';

            meta = {
              description = "AI-assisted Git utility for atomic Conventional Commits";
              homepage = "https://github.com/ryjen/git-autocommit";
              license = pkgs.lib.licenses.asl20;
              mainProgram = "git-autocommit";
            };
          };
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
      });

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
              echo "  nix build"
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
