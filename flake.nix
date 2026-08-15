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
            version = packageVersion;
            src = self;

            cargoLock.lockFile = ./Cargo.lock;

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
          };
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/git-autocommit";
        };
      });

      checks = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          package = self.packages.${system}.default;
          mkCargoCheck =
            {
              name,
              command,
              extraNativeBuildInputs ? [ ],
            }:
            package.overrideAttrs (old: {
              pname = "git-autocommit-check-${name}";
              nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ extraNativeBuildInputs;
              doCheck = false;
              buildPhase = ''
                runHook preBuild
                cargo ${command}
                runHook postBuild
              '';
              installPhase = ''
                runHook preInstall
                mkdir -p "$out"
                touch "$out/passed"
                runHook postInstall
              '';
              postInstall = "";
            });
          formatCheck = mkCargoCheck {
            name = "format";
            command = "format-check";
            extraNativeBuildInputs = [ pkgs.rustfmt ];
          };
          staticAnalysis = mkCargoCheck {
            name = "static-analysis";
            command = "static-analysis";
            extraNativeBuildInputs = [ pkgs.clippy ];
          };
          unitTests = mkCargoCheck {
            name = "unit";
            command = "test-unit";
          };
          propertyTests = mkCargoCheck {
            name = "property";
            command = "test-property";
          };
          integrationTests = mkCargoCheck {
            name = "integration";
            command = "test-integration";
          };
          e2eTests = mkCargoCheck {
            name = "e2e";
            command = "test-e2e";
          };
          releaseBuild = mkCargoCheck {
            name = "build-release";
            command = "build-release";
          };
          aggregate = pkgs.runCommand "git-autocommit-quality-gates-${packageVersion}" { } ''
            test -e ${formatCheck}/passed
            test -e ${staticAnalysis}/passed
            test -e ${unitTests}/passed
            test -e ${propertyTests}/passed
            test -e ${integrationTests}/passed
            test -e ${e2eTests}/passed
            test -e ${releaseBuild}/passed
            test -x ${package}/bin/git-autocommit
            mkdir -p "$out"
            touch "$out/passed"
          '';
        in
        {
          default = aggregate;
          format = formatCheck;
          static-analysis = staticAnalysis;
          unit = unitTests;
          property = propertyTests;
          integration = integrationTests;
          e2e = e2eTests;
          build-release = releaseBuild;
          package = package;
        }
      );

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
              echo "  cargo format-check"
              echo "  cargo static-analysis"
              echo "  cargo test-unit"
              echo "  cargo test-property"
              echo "  cargo test-integration"
              echo "  cargo test-e2e"
              echo "  cargo build-release"
              echo "  nix flake check"
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
