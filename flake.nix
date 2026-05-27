{
  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "forepaw";
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            # Darwin stdenv includes all frameworks via $SDKROOT.
            # No explicit framework buildInputs needed.

            meta = with pkgs.lib; {
              description = "Cross-platform desktop automation CLI";
              license = licenses.unlicense;
              mainProgram = "forepaw";
            };
          };
        }
      );

      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              # Rust toolchain
              rustc
              cargo
              clippy
              rustfmt
              rust-analyzer
              # Cross-compilation to Windows
              cargo-xwin
              lld
              # Cross-compilation to Linux
              cargo-zigbuild
              zig
              # Linting and auditing
              cargo-audit
              cargo-machete
              cargo-outdated
            ];
          };
        }
      );
    };
}
