{
  description = "exact — cycle-accurate Cortex-M judging frontend";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      flake-utils,
      rust-overlay,
      nixpkgs,
    }:
    (flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        # Matches mono-os: nightly with thumbv7m-none-eabi so the build worker
        # in exact-api can cross-compile user submissions.
        nightlyToolChain = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
            "rustfmt"
          ];
          targets = [
            "thumbv7m-none-eabi"
          ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            nightlyToolChain
            # Frontend
            nodejs_22
            pnpm
            # Local Postgres for dev (used from step 3 onward).
            postgresql_16
            # QEMU for the local runner dev mode (used from step 6 onward).
            qemu
          ];
          env = {
            RUST_SRC_PATH = "${nightlyToolChain}/lib/rustlib/src/library";
          };
        };
      }
    ));
}
