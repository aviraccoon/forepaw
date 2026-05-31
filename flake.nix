{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      self,
      ...
    }:
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
          version = (builtins.fromTOML (builtins.readFile ./crates/forepaw/Cargo.toml)).package.version;
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "forepaw";
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            # Embed git SHA from flake source when available.
            # self.shortRev is set for fetched repos (nix run github:...)
            # and null for dirty working trees.
            FOREPAW_GIT_SHA = self.shortRev or "unknown";

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
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            targets = [
              "aarch64-apple-darwin"
              "x86_64-apple-darwin"
              "x86_64-unknown-linux-musl"
              "aarch64-unknown-linux-musl"
              "x86_64-pc-windows-msvc"
              "aarch64-pc-windows-msvc"
            ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              rustToolchain
              # Cross-compilation to Windows
              pkgs.cargo-xwin
              pkgs.lld
              # Cross-compilation to Linux
              pkgs.cargo-zigbuild
              pkgs.zig
              # Linting and auditing
              pkgs.cargo-audit
              pkgs.cargo-machete
              pkgs.cargo-outdated
            ];
          };
        }
      );
    };
}
